//! Without the `session-bearer` engine feature a Session-bound managed request
//! is refused at issue, before any effect (docs/plans/RUST_SESSION_BEARER_HOOK.md,
//! Phase 5). Builds that enable the feature run `session_bearer.rs` instead.
#![cfg(not(feature = "session-bearer"))]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Context, Credential, Error, HttpAction, Operation, Policy, RetryPolicy, Scope, Session,
    SessionPolicy,
};
use safe_cex_engine::signing::{
    Algorithm, KeyRef, Operation as SignOperation, PayloadPart, SignError, SignReceipt,
    SignRequest, SignResult,
};
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, HttpRequest, Method, Response,
    SignStep, SocketId, TimerId, Venue,
};

struct Recipe;
impl SignStep for Recipe {
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        Ok(SignRequest {
            receipt: SignReceipt::default(),
            operations: vec![SignOperation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: KeyRef("account/primary".into()),
                payload: vec![PayloadPart::Bytes {
                    bytes: b"body".to_vec(),
                }],
            }],
            context: Default::default(),
        })
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        60_000.
    }
}

fn policy(signed: bool) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: signed.then(|| Credential {
            slot: CredentialSlot::Primary,
            reference: KeyRef("account/primary".into()),
        }),
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Entry {
    Managed,
    WithHeaders,
    Signed,
}

#[derive(Default)]
struct Probe {
    session: Option<Session>,
    /// (entry point, bound, result, the refused pending was its own context)
    issues: Vec<(Entry, bool, Result<(), Error>, bool)>,
}
impl Probe {
    fn issue(&mut self, cx: &mut Cx<'_, Context>, entry: Entry, bound: bool) {
        let context = cx
            .http_context(Scope::Background, policy(entry == Entry::Signed))
            .unwrap();
        let session = self.session.clone().filter(|_| bound);
        let url = "https://fixture/private".to_owned();
        let result = match entry {
            Entry::Managed => cx.request_managed(
                context.clone(),
                session.as_ref(),
                Method::Get,
                url,
                None,
                context.clone(),
            ),
            Entry::WithHeaders => cx.request_managed_with_headers(
                context.clone(),
                session.as_ref(),
                Method::Get,
                url,
                &[("Content-Type", "application/json")],
                None,
                context.clone(),
            ),
            Entry::Signed => cx.request_signed_managed(
                context.clone(),
                session.as_ref(),
                Method::Post,
                url,
                None,
                Box::new(Recipe),
                context.clone(),
            ),
        };
        let (result, own) = match result {
            Ok(_) => (Ok(()), true),
            Err((returned, error)) => (Err(error), returned == context),
        };
        self.issues.push((entry, bound, result, own));
    }
}
impl Venue for Probe {
    type Pending = Context;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        self.session = Some(
            cx.open_session(SessionPolicy {
                credential: None,
                repeat_safe: true,
                private_egress: None,
            })
            .unwrap(),
        );
        for entry in [Entry::Managed, Entry::WithHeaders, Entry::Signed] {
            self.issue(cx, entry, false);
            self.issue(cx, entry, true);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        #[cfg(feature = "http-plans")]
        if let Command::FetchAccount { .. } = command {
            plan_issue(self, cx);
        }
        #[cfg(not(feature = "http-plans"))]
        let _ = (cx, command);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

/// A Plan-owned Session-bound leg: refused at issue, and the command fails
/// once with the named reason.
#[cfg(feature = "http-plans")]
fn plan_issue(probe: &mut Probe, cx: &mut Cx<'_, Context>) {
    use safe_cex_engine::request_context::plan::{CallBound, RetryWindow, StepBound};
    use safe_cex_engine::request_context::{PlanPolicy, PreflightPolicy, PreflightPurpose};
    let preflight = cx
        .begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::ReadOnly,
            read_credential: None,
            write_credential: None,
        })
        .unwrap();
    let calls = [CallBound {
        timeout_ms: 20_000,
        attempts: 1,
        retry_window: RetryWindow::Shared,
        after_refusal: &[],
    }];
    let plan = cx
        .admit_plan(
            &preflight,
            &[StepBound {
                wait_ms: 0,
                calls: &calls,
            }],
            PlanPolicy {
                operation: Operation::Read,
                credential: None,
                pacing: None,
                retry: RetryPolicy::Never,
                independent_calls: false,
            },
        )
        .unwrap();
    let slot = cx.plan_slots(&plan).unwrap().remove(0);
    let leg = cx.plan_leg_context(&plan, &slot).unwrap();
    let result = cx.request_managed(
        leg.clone(),
        probe.session.as_ref(),
        Method::Get,
        "https://fixture/plan".into(),
        None,
        leg,
    );
    probe.issues.push((
        Entry::Managed,
        true,
        result.map(drop).map_err(|(_, error)| error),
        true,
    ));
}

fn prepared_urls(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { request, .. } => Some(request.url.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Every entry point refuses a bound request `Invalid` before any effect and
/// hands back its own pending; the same call unbound issues. Only the three
/// unbound requests prepare, and no signing work starts for a bound one.
#[test]
fn every_entry_point_refuses_a_session_binding_without_the_feature() {
    let mut e: Engine<Probe> = support::seeded_engine(Probe::default());
    e.start(0.);
    let actions: Vec<Action> = e.drain_actions().collect();
    for (entry, bound, result, own) in &e.venue().issues {
        if *bound {
            assert_eq!(*result, Err(Error::Invalid), "{entry:?} bound");
            assert!(*own, "{entry:?}: the refused pending is its own context");
        } else {
            assert_eq!(*result, Ok(()), "{entry:?} unbound");
        }
    }
    assert_eq!(e.venue().issues.len(), 6);
    assert_eq!(prepared_urls(&actions).len(), 3, "only the unbound calls");
    assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. })));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(HttpRequest { .. })))
    );
}

/// A Plan-owned bound leg fails its command once, naming the feature.
#[cfg(feature = "http-plans")]
#[test]
fn a_plan_owned_session_binding_names_the_missing_feature() {
    let mut e: Engine<Probe> = support::seeded_engine(Probe::default());
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.handle(
        1.,
        safe_cex_engine::Event::Command(Command::FetchAccount { command: 7 }),
    );
    let actions: Vec<Action> = e.drain_actions().collect();
    assert_eq!(
        e.venue().issues.last().map(|issue| issue.2),
        Some(Err(Error::Invalid))
    );
    assert!(
        !prepared_urls(&actions)
            .iter()
            .any(|url| url.ends_with("/plan"))
    );
    let failed: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 7, error }) => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(failed.len(), 1, "one terminal");
    assert_eq!(
        failed[0].message,
        "Session-bound requests require the session-bearer feature"
    );
    // A Read Plan's admission failure carries no financial outcome.
    assert_eq!(failed[0].outcome, None);
}
