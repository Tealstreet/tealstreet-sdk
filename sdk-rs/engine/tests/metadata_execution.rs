//! Publication tokens cannot replace the shared Plan's observer authority.
#![cfg(all(feature = "http-plans", feature = "test-support"))]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::engine::Window;
use safe_cex_engine::metadata::{
    CredentialSubject, Fallback, NativeWork, Proposal, Rails, Snapshot, Token, WorkKey,
};
use safe_cex_engine::request_context::{
    Context, Operation, Plan, PlanPolicy, Policy, PreflightPolicy, PreflightPurpose, RetryEvidence,
    RetryPolicy, RoutingIntent,
    plan::{CallBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::signing::{Identity, KeyRef};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, Engine, Event, HttpRequest, Method, Notice, Query,
    Response, SocketId, TimerId, Venue, VenueError,
};
use std::collections::HashMap;
const CALL: [CallBound<'static>; 1] = [CallBound {
    timeout_ms: 20,
    attempts: 2,
    retry_window: RetryWindow::FreshAttempt,
    after_refusal: &[],
}];
const STEPS: [StepBound<'static>; 2] = [
    StepBound {
        wait_ms: 0,
        calls: &CALL,
    },
    StepBound {
        wait_ms: 0,
        calls: &CALL,
    },
];
const TAG: &str = "metadata-settings";
struct Pending {
    plan: Plan,
    slot: Slot,
    leg: Context,
    token: Token,
    page: usize,
}
#[derive(Default)]
struct Fixture {
    roots: HashMap<u32, Context>,
    admitted: Vec<Plan>,
    delivered: usize,
    retries: usize,
    queue_refusals: usize,
    boundary_detach: Vec<u32>,
    timer_detach: Vec<u32>,
    detach_timer: Option<TimerId>,
}
fn key(publish: bool) -> WorkKey {
    let identity = Identity {
        credential: KeyRef("ref:configured".into()),
        signing: KeyRef("ref:signer".into()),
        passphrase: None,
        stable_id: None,
        extra: Default::default(),
    };
    WorkKey {
        symbol: "BTC".into(),
        subject: CredentialSubject::ConfiguredPrimary(identity.clone()),
        slot: CredentialSlot::Primary,
        identity,
        intent: RoutingIntent::Public,
        operation: Operation::Read,
        native: "BTC-USDT".into(),
        work: NativeWork {
            kind: TAG,
            endpoints: &["/rails", "/margin"],
            step_waits_ms: &[0, 0],
            calls_per_step: &[1, 1],
            root_timeout_ms: 80,
            call_timeout_ms: 20,
            attempts: 2,
            retry_window: RetryWindow::FreshAttempt,
            retry: RetryPolicy::ReadFailure,
        },
        publish,
        fallback: Fallback::IndependentRails,
    }
}
fn policy(timeout_ms: u32) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms,
        pacing: None,
        retry: RetryPolicy::ReadFailure,
    }
}
fn proposal() -> Proposal {
    Proposal::complete(
        Snapshot::new(MarketSetting {
            leverage: 12.,
            buy_leverage: Some(12.),
            sell_leverage: Some(34.),
            margin_type: Some(safe_cex_engine::model::MarginType::Isolated),
        })
        .unwrap(),
    )
}
impl Fixture {
    fn issue(&mut self, cx: &mut Cx<'_, Pending>, plan: Plan, token: Token, page: usize) {
        let Ok(mut slots) = cx.plan_slots(&plan) else {
            return;
        };
        if slots.is_empty() {
            return;
        }
        let slot = slots.remove(0);
        let leg = cx.plan_leg_context(&plan, &slot).unwrap();
        if let Err((pending, error)) = cx.request_managed(
            leg.clone(),
            Method::Get,
            format!("https://metadata.test/{page}"),
            None,
            Pending {
                plan,
                slot,
                leg,
                token,
                page,
            },
        ) {
            assert_eq!(error, safe_cex_engine::request_context::Error::Revoked);
            self.queue_refusals += 1;
            cx.store.retire_metadata_work(&pending.token);
            let _ = cx.fail_plan(&pending.plan, "orphaned metadata");
        }
    }
}
impl Venue for Fixture {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.timer_detach.is_empty() {
            self.detach_timer = Some(cx.set_timer(5.));
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Command::Query {
            command,
            query: Query::Income { kind, .. },
        } = command
        else {
            panic!("genuine Read command required");
        };
        for (token, handle) in cx.store.metadata_executions() {
            if cx.validate_http_context(handle.context()).is_err() {
                cx.store.retire_metadata_work(&token);
            }
        }
        let key = key(kind.as_deref() != Some("return"));
        if let Some(handle) = cx.store.metadata_work(&key) {
            let root = cx
                .admit_http_command(policy(if kind.as_deref() == Some("short") {
                    10
                } else {
                    80
                }))
                .unwrap();
            cx.join_execution(&handle, &root, TAG).unwrap();
            self.roots.insert(command, root);
            return;
        }
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly,
                read_credential: None,
                write_credential: None,
            })
            .unwrap();
        let (plan, handle, root) = cx
            .begin_planned_execution(
                &preflight,
                &STEPS,
                PlanPolicy {
                    operation: Operation::Read,
                    credential: None,
                    pacing: None,
                    retry: RetryPolicy::ReadFailure,
                },
                Window::UntilTerminal,
                TAG,
            )
            .unwrap();
        let token = cx.store.bind_metadata_work(key, handle).unwrap();
        self.roots.insert(command, root);
        self.admitted.push(plan.clone());
        self.issue(cx, plan, token, 0);
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        response: Response<'_>,
    ) {
        cx.validate_plan_delivery(&pending.plan, &pending.leg)
            .unwrap();
        self.delivered += 1;
        if response.body == "retry" {
            let leg = cx
                .plan_retry_context(
                    &pending.plan,
                    &pending.slot,
                    &pending.leg,
                    RetryEvidence::ReadFailure,
                )
                .unwrap();
            pending.leg = leg.clone();
            self.retries += 1;
            cx.request_managed(
                leg,
                Method::Get,
                "https://metadata.test/retry".into(),
                None,
                pending,
            )
            .unwrap_or_else(|_| panic!("declared retry must queue"));
            return;
        }
        if pending.page == 0 {
            if !cx.store.metadata_eligible(&pending.token) {
                let resolution = cx
                    .store
                    .finish_metadata(
                        &pending.token,
                        Ok(Proposal::rails(Rails::new(12., 34.).unwrap())),
                    )
                    .unwrap();
                assert!(!resolution.published);
                cx.done_read_plan_after_data(
                    &pending.plan,
                    &pending.slot,
                    &pending.leg,
                    "return-only".into(),
                )
                .unwrap();
                return;
            }
            cx.finish_plan_call(&pending.plan, &pending.slot, &pending.leg)
                .unwrap();
            for id in std::mem::take(&mut self.boundary_detach) {
                cx.detach_execution_observer(&self.roots[&id], "detached")
                    .unwrap();
            }
            self.issue(cx, pending.plan, pending.token, 1);
            return;
        }
        cx.finish_plan_call(&pending.plan, &pending.slot, &pending.leg)
            .unwrap();
        let resolution = cx
            .store
            .finish_metadata(&pending.token, Ok(proposal()))
            .unwrap();
        assert!(resolution.published);
        cx.done_plan(&pending.plan, "published".into()).unwrap();
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        cx.store.retire_metadata_work(&pending.token);
        let _ = cx.fail_plan(&pending.plan, error);
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
        panic!("typed failure required");
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        assert_eq!(self.detach_timer.take(), Some(timer));
        for command in std::mem::take(&mut self.timer_detach) {
            cx.detach_execution_observer(&self.roots[&command], "detached during flight")
                .unwrap();
        }
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn engine(detach: Vec<u32>) -> Engine<Fixture> {
    let mut engine = support::seeded_engine(Fixture {
        boundary_detach: detach,
        ..Default::default()
    });
    engine.start(0.);
    engine
}
fn query(engine: &mut Engine<Fixture>, now: f64, command: u32, kind: &str) {
    engine.handle(
        now,
        Event::Command(Command::Query {
            command,
            query: Query::Income {
                symbol: None,
                kind: Some(kind.into()),
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }),
    );
}
fn actions(engine: &mut Engine<Fixture>, now: f64) -> Vec<Action> {
    prepared_actions(engine, now)
}
fn request(actions: &[Action]) -> HttpRequest {
    let requests = actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    requests.into_iter().next().unwrap()
}
fn reply(engine: &mut Engine<Fixture>, now: f64, request: &HttpRequest, body: &str) {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn terminals(actions: &[Action], id: u32) -> Vec<bool> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, .. }) if *command == id => Some(true),
            Action::Notice(Notice::CommandFailed { command, .. }) if *command == id => Some(false),
            _ => None,
        })
        .collect()
}
#[test]
fn founder_detach_between_slots_keeps_peer_and_fresh_retry_alive() {
    let mut e = engine(vec![7]);
    query(&mut e, 1., 7, "publish");
    let first = request(&actions(&mut e, 1.));
    query(&mut e, 2., 8, "publish");
    assert!(
        actions(&mut e, 2.)
            .iter()
            .all(|a| !matches!(a, Action::Http(_)))
    );
    reply(&mut e, 3., &first, "retry");
    let retry = request(&actions(&mut e, 3.));
    assert_ne!(first.id, retry.id);
    reply(&mut e, 4., &retry, "ok");
    let between = actions(&mut e, 4.);
    assert_eq!(terminals(&between, 7), [false]);
    assert!(terminals(&between, 8).is_empty());
    let second = request(&between);
    reply(&mut e, 5., &second, "ok");
    let done = actions(&mut e, 5.);
    assert_eq!(terminals(&done, 8), [true]);
    assert!(terminals(&done, 7).is_empty());
    assert_eq!(e.venue().retries, 1);
    assert_eq!(e.store().setting("BTC").unwrap().sell_leverage, Some(34.));
    assert!(e.store().metadata_executions().is_empty());
}
#[test]
fn all_detach_between_slots_prevents_issue_and_fresh_entry_prunes_dead_token() {
    let mut e = engine(vec![7, 8]);
    query(&mut e, 1., 7, "publish");
    let first = request(&actions(&mut e, 1.));
    query(&mut e, 2., 8, "publish");
    actions(&mut e, 2.);
    reply(&mut e, 3., &first, "ok");
    let out = actions(&mut e, 3.);
    assert!(out.iter().all(|a| !matches!(a, Action::Http(_))));
    assert_eq!(terminals(&out, 7), [false]);
    assert_eq!(terminals(&out, 8), [false]);
    assert!(e.store().setting("BTC").is_none());
    assert_eq!(e.venue().queue_refusals, 1);
    assert!(e.store().metadata_executions().is_empty());
    let count = e.venue().delivered;
    reply(&mut e, 4., &first, "ok");
    assert_eq!(e.venue().delivered, count);
    actions(&mut e, 4.);
    query(&mut e, 5., 9, "publish");
    let _fresh = request(&actions(&mut e, 5.));
    assert_eq!(e.store().metadata_executions().len(), 1);
    assert_eq!(e.venue().admitted.len(), 2);
}
#[test]
fn short_follower_expiry_does_not_revoke_live_founder() {
    let mut e = engine(vec![]);
    query(&mut e, 1., 7, "publish");
    let first = request(&actions(&mut e, 1.));
    query(&mut e, 2., 8, "short");
    actions(&mut e, 2.);
    reply(&mut e, 3., &first, "ok");
    let second = request(&actions(&mut e, 3.));
    reply(&mut e, 13., &second, "ok");
    let done = actions(&mut e, 13.);
    assert_eq!(terminals(&done, 8), [false]);
    assert_eq!(terminals(&done, 7), [true]);
    assert_eq!(e.store().setting("BTC").unwrap().leverage, 12.);
}
#[test]
fn same_projection_tag_with_different_policy_has_distinct_plans_and_supersedes_publish() {
    let mut e = engine(vec![]);
    query(&mut e, 1., 7, "publish");
    let first = request(&actions(&mut e, 1.));
    query(&mut e, 2., 8, "return");
    let other = request(&actions(&mut e, 2.));
    assert_ne!(first.id, other.id);
    assert_eq!(e.venue().admitted.len(), 2);
    reply(&mut e, 3., &first, "ok");
    let out = actions(&mut e, 3.);
    assert_eq!(terminals(&out, 7), [true]);
    assert!(out.iter().all(|a| !matches!(a, Action::Http(_))));
    assert!(e.store().setting("BTC").is_none());
    reply(&mut e, 4., &other, "ok");
    let out = actions(&mut e, 4.);
    assert_eq!(terminals(&out, 8), [true]);
    assert!(e.store().setting("BTC").is_none());
    assert!(e.store().metadata_executions().is_empty());
}

#[test]
fn detached_inflight_handle_is_pruned_before_new_same_key_admission() {
    let mut e = support::seeded_engine(Fixture {
        timer_detach: vec![7, 8],
        ..Default::default()
    });
    e.start(0.);
    let timer = e.venue().detach_timer.unwrap();
    actions(&mut e, 0.);
    query(&mut e, 1., 7, "publish");
    let first = request(&actions(&mut e, 1.));
    query(&mut e, 2., 8, "publish");
    actions(&mut e, 2.);
    let old = e.store().metadata_executions()[0].0.clone();
    e.handle(5., Event::TimerFired { timer });
    let detached = actions(&mut e, 5.);
    assert_eq!(terminals(&detached, 7), [false]);
    assert_eq!(terminals(&detached, 8), [false]);
    assert_eq!(
        e.store().metadata_executions().len(),
        1,
        "Store keeps no extra lifecycle callback"
    );
    reply(&mut e, 6., &first, "ok");
    let late = actions(&mut e, 6.);
    assert!(terminals(&late, 7).is_empty());
    assert!(terminals(&late, 8).is_empty());
    assert_eq!(e.venue().delivered, 0);
    assert!(e.store().setting("BTC").is_none());
    query(&mut e, 7., 9, "publish");
    let fresh = request(&actions(&mut e, 7.));
    assert_eq!(e.store().metadata_executions().len(), 1);
    assert_ne!(e.store().metadata_executions()[0].0, old);
    assert_eq!(e.venue().admitted.len(), 2);
    reply(&mut e, 8., &fresh, "ok");
    let margin = request(&actions(&mut e, 8.));
    reply(&mut e, 9., &margin, "ok");
    assert_eq!(terminals(&actions(&mut e, 9.), 9), [true]);
}
