use super::*;
use crate::engine::*;
use crate::engine::{ExecutionHandle, Window};
use crate::request_context::{
    Context, Error, Operation, Plan, PlanPolicy, Policy, Preflight, PreflightPolicy,
    PreflightPurpose, RetryPolicy, Scope, Session, SessionPolicy,
    plan::{CallBound, RetryWindow, Slot, StepBound},
};

#[derive(Clone, Copy, Default)]
enum ForeignKind {
    #[default]
    Context,
    Plan,
    Execution,
}
#[derive(Clone, Copy)]
enum SessionAttack {
    Open,
    Context,
    Leg,
    Retire,
    Ready,
    Failed,
    Wait,
    Scoped,
}
#[derive(Default)]
struct ScopeProbe {
    kind: ForeignKind,
    root: Option<Context>,
    leg: Option<Context>,
    plan: Option<Plan>,
    slot: Option<Slot>,
    execution: Option<ExecutionHandle>,
    parked: Option<Preflight>,
    ready: usize,
    responses: usize,
    retired: usize,
    hold_parked: bool,
    last_refusal: Option<Error>,
    expiry_cleanup: bool,
    cleaned: usize,
    session_attack: Option<SessionAttack>,
    session: Option<Session>,
    session_parent: Option<Context>,
    session_plan_ready: bool,
    #[cfg(feature = "managed-sockets")]
    socket_mode: bool,
    #[cfg(feature = "managed-sockets")]
    socket_plan_tail: bool,
    #[cfg(feature = "managed-sockets")]
    socket_feed: Option<crate::socket_lifecycle::FeedId>,
}
fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 100,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
fn preflight_policy() -> PreflightPolicy {
    PreflightPolicy {
        purpose: PreflightPurpose::ReadOnly,
        read_credential: None,
        write_credential: None,
    }
}
fn session_policy() -> SessionPolicy {
    SessionPolicy {
        credential: None,
        repeat_safe: true,
        // Private egress, so the scoped-context attack reaches issuance.
        private_egress: crate::credential_selection::Slots::new(Some(()), None, Default::default())
            .select(crate::credential_selection::CredentialRole::SessionAuth)
            .unwrap()
            .0
            .session_egress(),
    }
}
impl ScopeProbe {
    fn assert_session_scope(&self, cx: &mut Cx<'_, Context>) {
        let attack = self.session_attack.unwrap();
        let session = self.session.as_ref().unwrap();
        let before = (cx.io.next_id, cx.io.timers.len(), cx.io.actions.len());
        match attack {
            SessionAttack::Open => {
                assert_eq!(cx.open_session(session_policy()).err(), Some(Error::State));
            }
            SessionAttack::Context => {
                assert_eq!(
                    cx.session_context(session, policy()).err(),
                    Some(Error::State)
                );
            }
            SessionAttack::Leg => {
                assert_eq!(
                    cx.session_leg_context(self.session_parent.as_ref().unwrap(), policy())
                        .err(),
                    Some(Error::State),
                );
            }
            SessionAttack::Retire => {
                assert_eq!(cx.retire_session(session).err(), Some(Error::State));
            }
            SessionAttack::Ready => assert!(cx.session_ready(session).is_empty()),
            SessionAttack::Failed => {
                assert!(
                    cx.session_failed(session, VenueError::new("foreign"))
                        .is_empty()
                );
            }
            SessionAttack::Scoped => {
                assert_eq!(
                    cx.session_scoped_context(session, Scope::Command(30), policy())
                        .err(),
                    Some(Error::State),
                );
            }
            SessionAttack::Wait => {
                let owner = self.leg.as_ref().unwrap();
                assert_eq!(
                    cx.session_wait(session, owner, owner.clone())
                        .err()
                        .map(|(_, error)| error),
                    Some(Error::State),
                );
            }
        }
        assert_eq!(
            (cx.io.next_id, cx.io.timers.len(), cx.io.actions.len(),),
            before,
            "foreign Session work must refuse before IDs, timers, holds or waiters change",
        );
        assert!(cx.session_live(session));
    }
}
impl Venue for ScopeProbe {
    type Pending = Context;
    fn start(&mut self, _cx: &mut Cx<'_, Context>) {
        #[cfg(feature = "managed-sockets")]
        if self.socket_mode {
            let feed = _cx.register_socket_feed(readiness_socket_policy()).unwrap();
            _cx.connect_socket(feed, "wss://ready.test").unwrap();
            self.socket_feed = Some(feed);
        }
    }
    #[cfg(feature = "managed-sockets")]
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Context>,
        event: crate::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), crate::socket_lifecycle::SocketError> {
        if self.socket_mode && matches!(event, crate::socket_lifecycle::LifecycleEvent::Ping(_)) {
            cx.request_preflight_ready(self.parked.as_ref().unwrap())
                .unwrap();
        }
        Ok(())
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        let id = command.completion_id().unwrap();
        if id == 10 && self.session_attack.is_some() {
            let owner = cx.admit_http_command(policy()).unwrap();
            let other = cx.http_context(Scope::Background, policy()).unwrap();
            let session = cx.open_session(session_policy()).unwrap();
            self.session_parent = Some(cx.session_context(&session, policy()).unwrap());
            cx.session_wait(&session, &owner, owner.clone()).unwrap();
            self.root = Some(owner);
            self.leg = Some(other);
            self.session = Some(session);
            return;
        }
        if id == 20 && self.session_plan_ready {
            let preflight = cx.begin_preflight(preflight_policy()).unwrap();
            let calls = [CallBound {
                timeout_ms: 100,
                attempts: 1,
                retry_window: RetryWindow::Shared,
                after_refusal: &[],
            }];
            let plan = cx
                .admit_plan(
                    &preflight,
                    &[StepBound {
                        wait_ms: 1,
                        calls: &calls,
                    }],
                    PlanPolicy {
                        operation: Operation::Read,
                        credential: None,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            assert!(cx.plan_slots(&plan).unwrap().is_empty());
            self.plan = Some(plan);
            return;
        }
        if id == 20 || self.hold_parked {
            let parked = match cx.begin_preflight(preflight_policy()) {
                Ok(handle) => handle,
                Err(crate::request_context::CommandEntryError::Rejected(reason)) => {
                    self.last_refusal = Some(reason);
                    return;
                }
                Err(other) => panic!("unexpected {other:?}"),
            };
            assert_eq!(cx.request_preflight_ready(&parked), Err(Error::Revoked));
            cx.park_preflight(&parked).unwrap();
            if !self.hold_parked {
                cx.request_preflight_ready(&parked).unwrap();
                cx.request_preflight_ready(&parked).unwrap();
            }
            self.parked = Some(parked);
            return;
        }
        let leg = match self.kind {
            ForeignKind::Context => {
                let root = cx.admit_http_command(policy()).unwrap();
                self.root = Some(root.clone());
                root
            }
            ForeignKind::Execution => {
                let root = cx.admit_http_command(policy()).unwrap();
                let execution = cx
                    .begin_execution(&root, policy(), Window::UntilTerminal, "foreign")
                    .unwrap();
                let leg = cx.http_leg_context(execution.context(), policy()).unwrap();
                self.root = Some(root);
                self.execution = Some(execution);
                leg
            }
            ForeignKind::Plan => {
                let preflight = cx.begin_preflight(preflight_policy()).unwrap();
                let count = if self.expiry_cleanup { 2 } else { 1 };
                let calls = (0..count)
                    .map(|_| CallBound {
                        timeout_ms: 100,
                        attempts: 1,
                        retry_window: RetryWindow::Shared,
                        after_refusal: &[],
                    })
                    .collect::<Vec<_>>();
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
                        },
                    )
                    .unwrap();
                let mut slots = cx.plan_slots(&plan).unwrap();
                let slot = slots.remove(0);
                let leg = cx.plan_leg_context(&plan, &slot).unwrap();
                for slot in slots {
                    let other = cx.plan_leg_context(&plan, &slot).unwrap();
                    cx.request_managed(
                        other.clone(),
                        Method::Get,
                        "https://read.test/other".into(),
                        None,
                        other,
                    )
                    .unwrap();
                }
                self.plan = Some(plan);
                self.slot = Some(slot);
                leg
            }
        };
        self.leg = Some(leg.clone());
        cx.request_managed(
            leg.clone(),
            Method::Get,
            "https://read.test/current".into(),
            None,
            leg,
        )
        .unwrap();
    }
    fn on_http_preflight_ready(&mut self, cx: &mut Cx<'_, Context>, preflight: &Preflight) {
        self.ready += 1;
        if self.session_attack.is_some() {
            self.assert_session_scope(cx);
            cx.done_preflight(preflight, "ready owner only".into())
                .unwrap();
            return;
        }
        if self.expiry_cleanup {
            assert_eq!(
                self.cleaned, 2,
                "all predecessor cleanup precedes readiness"
            );
            assert_eq!(self.retired, 1, "physical retirement precedes readiness");
            assert!(cx.io.http_plan_expiring.is_none());
            assert!(cx.io.actions.iter().any(|action| matches!(
                action,
                Action::Notice(Notice::CommandFailed { command: 10, .. })
            )));
            cx.done_preflight(preflight, "after expiry".into()).unwrap();
            return;
        }
        #[cfg(feature = "managed-sockets")]
        if self.socket_mode {
            if self.socket_plan_tail {
                let calls = [CallBound {
                    timeout_ms: 100,
                    attempts: 1,
                    retry_window: RetryWindow::Shared,
                    after_refusal: &[],
                }];
                self.plan = Some(
                    cx.admit_plan(
                        preflight,
                        &[StepBound {
                            wait_ms: 0,
                            calls: &calls,
                        }],
                        PlanPolicy {
                            operation: Operation::Read,
                            credential: None,
                            pacing: None,
                            retry: RetryPolicy::Never,
                        },
                    )
                    .unwrap(),
                );
                return;
            }
            self.retire_ready_sockets(cx);
        }
        assert!(cx.io.command_entry.is_none());
        assert!(cx.io.http_delivery.is_none());
        #[cfg(feature = "order-locks")]
        assert!(cx.io.order_response_token.is_none());
        if let Some(leg) = self.leg.as_ref() {
            cx.io.request_contexts.validate(leg, cx.now).unwrap();
            let ids = cx.io.next_id;
            let requests = cx.io.managed_http.len();
            let mutation = Policy {
                operation: Operation::Mutation,
                ..policy()
            };
            assert_eq!(cx.http_leg_context(leg, mutation), Err(Error::Revoked));
            assert_eq!(
                cx.http_context(Scope::Command(10), policy()),
                Err(Error::Revoked)
            );
            assert_eq!(cx.finish_http_context(leg), Err(Error::Revoked));
            assert_eq!(
                cx.http_write_protection_cancel_leg(leg),
                Err(Error::Revoked)
            );
            assert_eq!(
                cx.request_managed(
                    leg.clone(),
                    Method::Get,
                    "https://read.test/borrowed".into(),
                    None,
                    leg.clone()
                )
                .unwrap_err()
                .1,
                Error::Revoked
            );
            if let Some(plan) = self.plan.as_ref() {
                assert_eq!(cx.fail_plan(plan, "borrowed"), Err(Error::Revoked));
                assert_eq!(cx.done_plan(plan, "borrowed".into()), Err(Error::Revoked));
            }
            if let Some(root) = self.root.as_ref() {
                assert_eq!(cx.fail_context(root, "borrowed"), Err(Error::Revoked));
                assert_eq!(
                    cx.done_context(root, "borrowed".into()),
                    Err(Error::Revoked)
                );
                cx.fail(10, "numeric borrowed");
                cx.done(10, "numeric borrowed".into());
                if let Some(execution) = self.execution.as_ref() {
                    assert_eq!(
                        cx.abort_execution(execution, "borrowed"),
                        Err(Error::Revoked)
                    );
                    assert_eq!(
                        cx.detach_execution_observer(root, "borrowed"),
                        Err(Error::Revoked)
                    );
                }
            }
            assert_eq!(cx.io.next_id, ids);
            assert_eq!(cx.io.managed_http.len(), requests);
            cx.io.request_contexts.validate(leg, cx.now).unwrap();
        }
        cx.done_preflight(preflight, "cached".into()).unwrap();
    }
    fn on_http_plan_ready(&mut self, cx: &mut Cx<'_, Context>, plan: &Plan) {
        if self.session_plan_ready {
            self.ready += 1;
            self.assert_session_scope(cx);
            cx.fail_plan(plan, "Plan readiness tested").unwrap();
        }
        #[cfg(feature = "managed-sockets")]
        if !self.session_plan_ready && self.socket_plan_tail {
            self.retire_ready_sockets(cx);
            cx.fail_plan(plan, "tail callback completed").unwrap();
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: Response<'_>) {
        self.responses += 1;
        match self.kind {
            ForeignKind::Context => {
                cx.validate_read_http_delivery(self.root.as_ref().unwrap(), &context)
                    .unwrap();
                cx.done_context(self.root.as_ref().unwrap(), "foreign intact".into())
                    .unwrap();
            }
            ForeignKind::Plan => {
                let plan = self.plan.as_ref().unwrap();
                cx.validate_plan_delivery(plan, &context).unwrap();
                cx.finish_plan_call(plan, self.slot.as_ref().unwrap(), &context)
                    .unwrap();
                cx.done_plan(plan, "foreign intact".into()).unwrap();
            }
            ForeignKind::Execution => {
                let execution = self.execution.as_ref().unwrap();
                cx.validate_execution_read_delivery(execution, &context)
                    .unwrap();
                cx.complete_execution(execution, |_| Ok("foreign intact".into()))
                    .unwrap();
            }
        }
    }
    fn on_http_owner_retired(&mut self, _: &crate::request_context::HttpOwnerRetirement) {
        self.retired += 1;
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Context>,
        _: Context,
        _: Error,
        error: VenueError,
    ) {
        assert!(self.expiry_cleanup);
        self.cleaned += 1;
        cx.request_preflight_ready(self.parked.as_ref().unwrap())
            .unwrap();
        cx.fail_plan(self.plan.as_ref().unwrap(), error).unwrap();
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, error: &str) {
        panic!("unexpected {error}");
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn command(e: &mut Engine<ScopeProbe>, id: CommandId, at: f64) {
    e.handle(
        at,
        Event::Command(Command::CancelOrders {
            command: id,
            ids: vec![],
        }),
    );
}
fn terminal(actions: &[Action], id: CommandId) -> usize {
    actions.iter().filter(|a|matches!(a,Action::Notice(Notice::CommandDone{command,..}|Notice::CommandFailed{command,..}) if *command==id)).count()
}
fn session_attack_preserves_waiter(attack: SessionAttack) {
    session_attack_preserves_waiter_in_scope(attack, false);
}
fn session_attack_preserves_waiter_in_scope(attack: SessionAttack, plan_ready: bool) {
    let mut e = crate::test_support::seeded_engine(ScopeProbe {
        session_attack: Some(attack),
        session_plan_ready: plan_ready,
        ..Default::default()
    });
    command(&mut e, 10, 0.);
    let initial = crate::test_support::prepared_actions(&mut e, 0.);
    assert_eq!(terminal(&initial, 10), 0);
    command(&mut e, 20, 1.);
    let mut ready = crate::test_support::prepared_actions(&mut e, 1.);
    if plan_ready {
        let timer = ready
            .iter()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms == 2. => Some(*timer),
                _ => None,
            })
            .expect("native Plan delay arms readiness");
        e.handle(2., Event::TimerFired { timer });
        ready.extend(crate::test_support::prepared_actions(&mut e, 2.));
    }
    assert_eq!(terminal(&ready, 20), 1);
    assert_eq!(terminal(&ready, 10), 0);
    let session = e.venue.session.clone().unwrap();
    let drained = with_cx(&mut e, 2., |cx| cx.session_ready(&session));
    assert_eq!(
        drained.len(),
        1,
        "legitimate Session callback retains A's continuation"
    );
    let crate::request_context::Waiter::Ready(owner, _) = drained.into_iter().next().unwrap()
    else {
        panic!("A remains live after B's ready callback");
    };
    with_cx(&mut e, 2., |cx| cx.done_context(&owner, "A intact".into())).unwrap();
    let done = crate::test_support::prepared_actions(&mut e, 2.);
    assert_eq!(terminal(&done, 10), 1);
    assert!(with_cx(&mut e, 2., |cx| cx.session_ready(&session)).is_empty());
}
#[test]
fn retained_ready_cannot_drain_or_retire_another_sessions_waiters() {
    for attack in [
        SessionAttack::Ready,
        SessionAttack::Failed,
        SessionAttack::Retire,
        SessionAttack::Wait,
    ] {
        session_attack_preserves_waiter(attack);
    }
}
#[test]
fn retained_ready_cannot_issue_foreign_session_work_before_ids() {
    for attack in [SessionAttack::Context, SessionAttack::Open] {
        session_attack_preserves_waiter(attack);
    }
}
#[test]
fn retained_ready_cannot_derive_foreign_session_legs_before_ids() {
    session_attack_preserves_waiter(SessionAttack::Leg);
}
#[test]
fn readiness_cannot_issue_work_on_a_foreign_session_egress() {
    for plan_ready in [false, true] {
        session_attack_preserves_waiter_in_scope(SessionAttack::Scoped, plan_ready);
    }
}
#[test]
fn plan_readiness_cannot_consume_foreign_session_authority() {
    for attack in [
        SessionAttack::Ready,
        SessionAttack::Failed,
        SessionAttack::Retire,
        SessionAttack::Wait,
        SessionAttack::Context,
        SessionAttack::Open,
        SessionAttack::Leg,
    ] {
        session_attack_preserves_waiter_in_scope(attack, true);
    }
}
#[test]
fn ready_cannot_mutate_a_foreign_legacy_plan_or_execution_owner() {
    for kind in [
        ForeignKind::Context,
        ForeignKind::Plan,
        ForeignKind::Execution,
    ] {
        let mut e = crate::test_support::seeded_engine(ScopeProbe {
            kind,
            ..Default::default()
        });
        command(&mut e, 10, 0.);
        let initial = crate::test_support::prepared_actions(&mut e, 0.);
        let request = initial
            .iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r.id),
                _ => None,
            })
            .unwrap();
        command(&mut e, 20, 1.);
        let ready = crate::test_support::prepared_actions(&mut e, 1.);
        assert_eq!(terminal(&ready, 20), 1);
        assert_eq!(terminal(&ready, 10), 0);
        assert_eq!(e.venue.ready, 1);
        e.handle(
            2.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "ok",
                },
            },
        );
        let intact = crate::test_support::prepared_actions(&mut e, 2.);
        assert_eq!(e.venue.responses, 1);
        assert_eq!(terminal(&intact, 10), 1);
        assert!(intact.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone{command:10,result}) if result=="foreign intact")));
    }
}
#[test]
fn retired_and_wrong_issuer_wakes_cannot_reach_a_reused_numeric_id() {
    let mut e = crate::test_support::seeded_engine(ScopeProbe::default());
    command(&mut e, 20, 0.);
    let old = e.venue.parked.clone().unwrap();
    assert_eq!(
        terminal(&crate::test_support::prepared_actions(&mut e, 0.), 20),
        1
    );
    command(&mut e, 20, 1.);
    let new = e.venue.parked.clone().unwrap();
    assert_ne!(old.owner(), new.owner());
    let mut foreign = crate::test_support::seeded_engine(ScopeProbe::default());
    command(&mut foreign, 20, 0.);
    let wrong = foreign.venue.parked.clone().unwrap();
    let before = e.io.next_id;
    assert_eq!(
        e.io.request_contexts.queue_preflight_ready(&old),
        Err(Error::Revoked)
    );
    assert_eq!(
        e.io.request_contexts.queue_preflight_ready(&wrong),
        Err(Error::Revoked)
    );
    assert_eq!(e.io.next_id, before);
    assert_eq!(e.venue.ready, 2);
    e.flush_preflight_readiness(2.);
    assert_eq!(e.venue.ready, 2);
}

fn with_cx<R>(e: &mut Engine<ScopeProbe>, at: f64, f: impl FnOnce(&mut Cx<'_, Context>) -> R) -> R {
    f(&mut Cx {
        now: at,
        store: &mut e.store,
        io: &mut e.io,
    })
}
#[test]
fn queue_reservations_precede_ids_and_waking_never_allocates_or_renews() {
    let mut e = crate::test_support::seeded_engine(ScopeProbe {
        hold_parked: true,
        ..Default::default()
    });
    for id in 1..=CAPACITY as u32 {
        command(&mut e, id, 0.);
    }
    let ids = e.io.next_id;
    assert_eq!(e.io.request_contexts.commands.len(), CAPACITY);
    let ready_capacity = e.io.request_contexts.preflight_ready.capacity();
    let initial_capacity = e.io.request_contexts.preflight_initial.capacity();
    assert!(ready_capacity >= CAPACITY);
    assert!(initial_capacity >= CAPACITY);
    command(&mut e, 999, 0.);
    assert_eq!(e.venue.last_refusal, Some(Error::Capacity));
    assert_eq!(e.io.next_id, ids);
    let handle = e.venue.parked.clone().unwrap();
    for at in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            with_cx(&mut e, at, |cx| cx.request_preflight_ready(&handle)),
            Err(Error::Invalid)
        );
        assert!(e.io.request_contexts.preflight_ready.is_empty());
    }
    with_cx(&mut e, 1., |cx| {
        cx.request_preflight_ready(&handle).unwrap();
        cx.request_preflight_ready(&handle).unwrap();
        assert_eq!(cx.park_preflight(&handle), Err(Error::Revoked));
        assert_eq!(
            cx.done_preflight(&handle, "borrowed".into()),
            Err(Error::Revoked)
        );
    });
    assert_eq!(e.io.request_contexts.preflight_ready.len(), 1);
    assert_eq!(
        e.io.request_contexts.preflight_ready.capacity(),
        ready_capacity
    );
    assert_eq!(
        e.io.request_contexts.preflight_initial.capacity(),
        initial_capacity
    );
    assert_eq!(e.io.next_id, ids);
    e.prepare_dispatch(1.);
    assert_eq!(e.venue.ready, 1);
    assert_eq!(e.io.next_id, ids);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(terminal(&actions, CAPACITY as u32), 1);
    assert_eq!(terminal(&actions, 999), 1);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::ManagedHttp(_) | Action::SetTimer { .. }
    )));
}
#[test]
fn retirement_clears_queued_wake_before_generation_reuse_and_rejects_live_foreign_output() {
    let mut e = crate::test_support::seeded_engine(ScopeProbe {
        hold_parked: true,
        ..Default::default()
    });
    let mut foreign = crate::test_support::seeded_engine(ScopeProbe {
        hold_parked: true,
        ..Default::default()
    });
    command(&mut e, 20, 0.);
    command(&mut foreign, 20, 0.);
    let old = e.venue.parked.clone().unwrap();
    let wrong = foreign.venue.parked.clone().unwrap();
    assert_eq!(wrong.owner(), old.owner());
    assert_eq!(
        e.io.request_contexts.queue_preflight_ready(&wrong),
        Err(Error::Revoked)
    );
    with_cx(&mut e, 1., |cx| {
        cx.request_preflight_ready(&old).unwrap();
        cx.fail_preflight(&old, "cancelled before activation")
            .unwrap();
    });
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(terminal(&actions, 20), 1);
    assert_eq!(e.venue.ready, 0);
    assert!(e.io.request_contexts.preflight_ready.is_empty());
    command(&mut e, 20, 2.);
    let new = e.venue.parked.clone().unwrap();
    assert_ne!(old.owner(), new.owner());
    let ids = e.io.next_id;
    assert_eq!(
        e.io.request_contexts.queue_preflight_ready(&old),
        Err(Error::Revoked)
    );
    assert_eq!(e.io.next_id, ids);
    e.prepare_dispatch(2.);
    assert_eq!(e.venue.ready, 0);
    with_cx(&mut e, 3., |cx| cx.request_preflight_ready(&new)).unwrap();
    e.prepare_dispatch(3.);
    assert_eq!(e.venue.ready, 1);
    assert_eq!(terminal(&e.drain_actions().collect::<Vec<_>>(), 20), 1);
}
#[test]
fn an_empty_readiness_index_never_visits_live_legacy_or_execution_owners() {
    for kind in [ForeignKind::Context, ForeignKind::Execution] {
        let mut e = crate::test_support::seeded_engine(ScopeProbe {
            kind,
            ..Default::default()
        });
        command(&mut e, 10, 0.);
        crate::test_support::prepared_actions(&mut e, 0.);
        let ids = e.io.next_id;
        let plans = e.io.request_contexts.plan_scan_visits.get();
        for _ in 0..8 {
            e.flush_preflight_readiness(1.);
        }
        assert_eq!(e.io.next_id, ids);
        assert_eq!(e.io.request_contexts.plan_scan_visits.get(), plans);
        assert_eq!(e.venue.ready, 0);
        assert!(e.io.request_contexts.preflight_initial.is_empty());
        assert!(e.io.actions.is_empty());
    }
}

#[cfg(feature = "managed-sockets")]
#[test]
fn socket_callback_wake_cannot_drain_a_send_from_the_epoch_ready_just_retired() {
    for socket_plan_tail in [false, true] {
        let mut engine = crate::test_support::seeded_engine(ScopeProbe {
            socket_mode: true,
            socket_plan_tail,
            hold_parked: true,
            ..Default::default()
        });
        engine.start(0.);
        let socket = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::WsConnect { socket, .. } => Some(socket),
                _ => None,
            })
            .unwrap();
        engine.handle(1., Event::WsOpened { socket });
        engine.drain_actions().for_each(drop);
        command(&mut engine, 20, 2.);
        assert_eq!(engine.venue.ready, 0);
        let feed = engine.venue.socket_feed.unwrap();
        let epoch = with_cx(&mut engine, 2., |cx| cx.socket_epoch(feed)).unwrap();
        // A genuine registered epoch's queued lifecycle effect is processed during drain.
        engine
            .io
            .socket_effects
            .push_back(crate::socket_driver::Effect::Ping(epoch));
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert_eq!(engine.venue.ready, 1);
        assert_eq!(terminal(&actions, 20), 1);
        assert!(
            !actions.iter().any(|action| matches!(
                action,
                Action::WsSend { .. } | Action::WsConnect { .. } | Action::Sign { .. }
            )),
            "retired epoch work must cross the final socket fence"
        );
    }
}

#[cfg(feature = "managed-sockets")]
fn readiness_socket_policy() -> crate::socket_lifecycle::FeedPolicy {
    use crate::socket_lifecycle::*;
    FeedPolicy {
        role: SocketChannel::Private,
        supervision: crate::socket::Config::default(),
        auth: AuthMode::None,
        recovery: AuthRecovery::Reconnect,
        permanent_refusal_group: None,
        traffic: TrafficPolicy::AnyText,
        controls: ControlPolicy::Native,
        invalidation: Invalidation::Required,
        subscription_interval_ms: 0.,
    }
}
#[cfg(feature = "managed-sockets")]
struct RetiredSocketRecipe;
#[cfg(feature = "managed-sockets")]
impl crate::SignStep for RetiredSocketRecipe {
    fn freshness_ms(&self) -> f64 {
        3_000.
    }
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &crate::DispatchContext<'_>,
    ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
        panic!("retired socket must not reach provider preparation")
    }
    fn apply(
        &mut self,
        _: &mut Action,
        _: &crate::signing::SignResult,
    ) -> Result<(), crate::signing::SignError> {
        panic!("retired socket must not apply a signing result")
    }
}

#[cfg(feature = "managed-sockets")]
impl ScopeProbe {
    fn retire_ready_sockets(&mut self, cx: &mut Cx<'_, Context>) {
        let feed = self.socket_feed.unwrap();
        let epoch = cx.socket_epoch(feed).unwrap();
        cx.send(epoch.socket(), "stale-ready-frame");
        cx.send_signed(
            epoch.socket(),
            "stale-ready-signed".into(),
            Box::new(RetiredSocketRecipe),
        );
        cx.stop_socket_feed(feed);
        let fresh = cx.register_socket_feed(readiness_socket_policy()).unwrap();
        cx.connect_socket(fresh, "wss://ready.test/retired-before-drain")
            .unwrap();
        cx.stop_socket_feed(fresh);
    }
}

#[test]
fn expiry_cleanup_cannot_activate_or_fail_a_different_parked_preflight() {
    let mut e = crate::test_support::seeded_engine(ScopeProbe {
        kind: ForeignKind::Plan,
        hold_parked: false,
        ..Default::default()
    });
    command(&mut e, 10, 0.);
    crate::test_support::prepared_actions(&mut e, 0.);
    e.venue.hold_parked = true;
    command(&mut e, 20, 1.);
    let handle = e.venue.parked.clone().unwrap();
    let plan = e.venue.plan.clone().unwrap();
    with_cx(&mut e, 2., |cx| cx.request_preflight_ready(&handle)).unwrap();
    e.io.http_plan_expiring = Some(plan);
    with_cx(&mut e, 2., |cx| {
        assert_eq!(
            cx.validate_preflight_activation(&handle),
            Err(Error::Revoked)
        );
        assert_eq!(
            cx.fail_preflight(&handle, "foreign expiry"),
            Err(Error::Revoked)
        );
    });
    e.flush_preflight_readiness(2.);
    assert_eq!(e.venue.ready, 0);
    assert!(matches!(
        e.io.request_contexts
            .preflight_state(&handle)
            .unwrap()
            .activation,
        Activation::Queued
    ));
    e.io.http_plan_expiring = None;
    e.flush_preflight_readiness(2.);
    assert_eq!(e.venue.ready, 1);
}

#[test]
fn queued_successor_waits_for_all_unreleased_expiry_cleanup_and_terminal() {
    let mut e = crate::test_support::seeded_engine(ScopeProbe {
        kind: ForeignKind::Plan,
        expiry_cleanup: true,
        ..Default::default()
    });
    command(&mut e, 10, 0.);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action,
        Action::ManagedHttp(http) if matches!(http.as_ref(),
            crate::request_context::HttpAction::Prepare { .. })))
            .count(),
        2
    );
    let plan = e.venue.plan.as_ref().unwrap();
    let deadline = plan.deadline();
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == deadline => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.venue.hold_parked = true;
    command(&mut e, 20, 1.);
    e.handle(deadline, Event::TimerFired { timer });
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(e.venue.cleaned, 2);
    assert_eq!(e.venue.ready, 1);
    assert_eq!(terminal(&actions, 10), 1);
    assert_eq!(terminal(&actions, 20), 1);
    let predecessor = actions
        .iter()
        .position(|action| {
            matches!(
                action,
                Action::Notice(Notice::CommandFailed { command: 10, .. })
            )
        })
        .unwrap();
    let successor = actions
        .iter()
        .position(|action| {
            matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 20, .. })
            )
        })
        .unwrap();
    assert!(predecessor < successor);
}
