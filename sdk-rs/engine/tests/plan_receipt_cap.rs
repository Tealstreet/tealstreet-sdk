//! Receipt-cap controls retaining the original timing assertions.
#![cfg(all(feature = "http-plans", feature = "test-support"))]
mod support;

use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    CommandEntryError, Context, Error, Operation, Plan, PlanPolicy, Policy, Preflight,
    PreflightPolicy, PreflightPurpose, RetryPolicy,
    plan::{CallBound, CallTerms, PlanBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, CommandWait, Cx, Engine, Event, HttpRequest, Method, Notice, Query, Response,
    SocketId, Venue,
};

// This adapter is the only cutover from the original native-only entry.
fn receipt_preflight<P>(cx: &mut Cx<'_, P>) -> Result<Preflight, CommandEntryError> {
    cx.begin_preflight_with_command_wait(preflight_policy())
}

fn preflight_policy() -> PreflightPolicy {
    PreflightPolicy {
        purpose: PreflightPurpose::ReadOnly,
        read_credential: None,
        write_credential: None,
    }
}

enum Pending {
    Prerequisite {
        preflight: Preflight,
        context: Context,
    },
    Page {
        plan: Plan,
        slot: Slot,
        context: Context,
    },
}

struct Probe {
    native_ms: u32,
    attempts: u16,
    two_steps: bool,
    capped: bool,
    complete: bool,
    unstamped: bool,
    capture: Option<Result<(), CommandEntryError>>,
    entry_wait: Option<CommandWait>,
    borrowed_wait: Option<CommandWait>,
    handoff_validated: bool,
    admission: Option<Result<f64, Error>>,
    geometry: Option<(u64, usize, usize)>,
    selected_slots: usize,
}

impl Probe {
    fn new(native_ms: u32) -> Self {
        Self {
            native_ms,
            attempts: 1,
            two_steps: false,
            capped: true,
            complete: false,
            unstamped: false,
            capture: None,
            entry_wait: None,
            borrowed_wait: None,
            handoff_validated: false,
            admission: None,
            geometry: None,
            selected_slots: 0,
        }
    }
}

impl Venue for Probe {
    type Pending = Pending;
    fn start(&mut self, _: &mut Cx<'_, Pending>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        assert!(matches!(
            command,
            Command::Query {
                query: Query::Income { .. },
                ..
            }
        ));
        self.entry_wait = cx.command_wait();
        if self.unstamped {
            self.capture = Some(receipt_preflight(cx).map(|_| ()));
            return;
        }
        let preflight = if self.capped {
            receipt_preflight(cx).unwrap()
        } else {
            cx.begin_preflight(preflight_policy()).unwrap()
        };
        let context = cx
            .preflight_read_context(
                &preflight,
                Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 600_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
                CallTerms {
                    wait_ms: 0,
                    timeout_ms: 600_000,
                    attempts: 1,
                    retry_window: RetryWindow::Shared,
                },
            )
            .unwrap();
        cx.request_managed(
            context.clone(),
            Method::Get,
            "https://receipt.test/jwt".into(),
            None,
            Pending::Prerequisite { preflight, context },
        )
        .unwrap_or_else(|_| panic!("prerequisite must enter managed preparation"));
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: Response<'_>) {
        self.borrowed_wait = cx.command_wait();
        match pending {
            Pending::Prerequisite { preflight, context } => {
                cx.validate_preflight_delivery(&preflight, &context)
                    .unwrap();
                cx.finish_preflight_call(&preflight, &context).unwrap();
                self.handoff_validated = true;
                let calls = [CallBound {
                    timeout_ms: self.native_ms,
                    attempts: self.attempts,
                    retry_window: RetryWindow::FreshAttempt,
                    after_refusal: &[],
                }];
                let steps = [
                    StepBound {
                        wait_ms: 0,
                        calls: &calls,
                    },
                    StepBound {
                        wait_ms: 0,
                        calls: &calls,
                    },
                ];
                let steps = &steps[..if self.two_steps { 2 } else { 1 }];
                let bound = PlanBound::from_steps(steps).unwrap();
                self.geometry = Some((
                    bound.span_ms(),
                    bound.logical_calls(),
                    bound.physical_attempts(),
                ));
                match cx.admit_plan(
                    &preflight,
                    steps,
                    PlanPolicy {
                        independent_calls: false,
                        operation: Operation::Read,
                        credential: None,
                        pacing: None,
                        retry: RetryPolicy::ReadFailure,
                    },
                ) {
                    Ok(plan) => {
                        self.admission = Some(Ok(plan.deadline()));
                        let slots = cx.plan_slots(&plan).unwrap();
                        self.selected_slots = slots.len();
                        if self.complete {
                            let slot = slots.into_iter().next().unwrap();
                            let context = cx.plan_leg_context(&plan, &slot).unwrap();
                            cx.request_managed(
                                context.clone(),
                                Method::Get,
                                "https://receipt.test/page".into(),
                                None,
                                Pending::Page {
                                    plan,
                                    slot,
                                    context,
                                },
                            )
                            .unwrap_or_else(|_| panic!("live Plan page must prepare"));
                        }
                    }
                    Err(error) => {
                        self.admission = Some(Err(error));
                        cx.fail_preflight(&preflight, error.read_venue_error())
                            .unwrap();
                    }
                }
            }
            Pending::Page {
                plan,
                slot,
                context,
            } => {
                cx.validate_plan_delivery(&plan, &context).unwrap();
                cx.finish_plan_call(&plan, &slot, &context).unwrap();
                cx.done_plan(&plan, "complete".into()).unwrap();
            }
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, error: &str) {
        panic!("no fixture transport failure expected: {error}");
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::TimerId) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn release(engine: &mut Engine<Probe>, now: f64) -> HttpRequest {
    let mut packets = prepared_actions(engine, now).into_iter().filter_map(|a| {
        if let Action::Http(packet) = a {
            Some(packet)
        } else {
            None
        }
    });
    let packet = packets
        .next()
        .expect("real Prepare/Bind/Claim must release HTTP");
    assert!(packets.next().is_none());
    packet
}

fn respond(engine: &mut Engine<Probe>, packet: HttpRequest, now: f64) {
    engine.handle(
        now,
        Event::HttpResponse {
            request: packet.id,
            response: Response {
                status: 200,
                headers: "",
                body: "[]",
            },
        },
    );
}

fn handoff(probe: Probe, wait: Option<u32>, at: f64) -> Engine<Probe> {
    let mut engine = support::seeded_engine(probe);
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine
        .handle_command(
            0.,
            Command::Query {
                command: 7,
                query: Query::Income {
                    symbol: None,
                    kind: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            },
            wait,
        )
        .unwrap();
    assert_eq!(
        engine.venue().entry_wait,
        Some(CommandWait::Remaining(wait.unwrap_or(120_000)))
    );
    let packet = release(&mut engine, 1.);
    assert!(packet.url.ends_with("/jwt"));
    respond(&mut engine, packet, at);
    assert!(engine.venue().handoff_validated);
    assert_eq!(
        engine.venue().borrowed_wait,
        None,
        "HTTP cannot mint a fresh receipt"
    );
    engine
}

#[test]
fn jwt_wait_does_not_restart_the_callers_original_read_plan_deadline() {
    let engine = handoff(Probe::new(100), Some(100), 60.);
    assert_eq!(engine.venue().geometry, Some((100, 1, 1)));
    assert_eq!(engine.venue().selected_slots, 1);
    assert_eq!(engine.venue().admission, Some(Ok(100.)));
}

#[test]
fn default_wait_stays_stamped_at_receipt_across_preflight() {
    let engine = handoff(Probe::new(200_000), None, 40_000.);
    assert_eq!(engine.venue().geometry, Some((200_000, 1, 1)));
    assert_eq!(engine.venue().admission, Some(Ok(120_000.)));
}

#[test]
fn caller_override_above_default_caps_the_plan_at_its_original_receipt() {
    let engine = handoff(Probe::new(400_000), Some(300_000), 5_000.);
    assert_eq!(engine.venue().geometry, Some((400_000, 1, 1)));
    assert_eq!(engine.venue().admission, Some(Ok(300_000.)));
}

#[test]
fn native_geometry_and_fresh_attempt_credit_are_not_shrunk_to_fake_a_cap() {
    let mut probe = Probe::new(100);
    probe.attempts = 2;
    probe.two_steps = true;
    let engine = handoff(probe, Some(150), 40.);
    assert_eq!(engine.venue().geometry, Some((400, 2, 4)));
    assert_eq!(
        engine.venue().selected_slots,
        1,
        "the next serial slot is not a peer"
    );
    assert_eq!(engine.venue().admission, Some(Ok(150.)));
}

fn assert_expired_handoff(at: f64) {
    let mut engine = handoff(Probe::new(100), Some(100), at);
    assert_eq!(engine.venue().geometry, Some((100, 1, 1)));
    assert_eq!(
        engine.venue().admission,
        Some(Err(Error::TimedOut)),
        "handoff at {at}"
    );
    assert_eq!(engine.venue().selected_slots, 0);
    let actions: Vec<_> = engine.drain_actions().collect();
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            Action::ClearTimer { .. } => None,
            Action::ManagedHttp(action)
                if matches!(
                    **action,
                    safe_cex_engine::request_context::HttpAction::Cancel { .. }
                ) =>
            {
                None
            }
            other => panic!("unexpected effect after rejected Plan admission: {other:?}"),
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(*failures[0], Error::TimedOut.read_venue_error());
}

#[test]
fn receipt_equality_refuses_plan_admission_after_valid_native_handoff() {
    assert_expired_handoff(100.);
}

#[test]
fn expired_receipt_refuses_plan_admission_after_valid_native_handoff() {
    assert_expired_handoff(101.);
}

#[test]
fn sub_millisecond_receipt_remainder_cannot_revive_plan_admission() {
    assert_expired_handoff(99.5);
}

#[test]
fn shorter_native_plan_is_live_and_completes_through_managed_delivery() {
    let mut probe = Probe::new(20);
    probe.complete = true;
    let mut engine = handoff(probe, Some(100), 60.);
    assert_eq!(engine.venue().geometry, Some((20, 1, 1)));
    assert_eq!(engine.venue().admission, Some(Ok(80.)));
    let packet = release(&mut engine, 61.);
    assert!(packet.url.ends_with("/page"));
    respond(&mut engine, packet, 62.);
    let terminals: Vec<_> = engine
        .drain_actions()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
            )
        })
        .collect();
    assert_eq!(terminals.len(), 1);
    assert!(
        matches!(&terminals[0], Action::Notice(Notice::CommandDone { command: 7, result }) if result == "complete")
    );
}

#[test]
fn native_only_mode_preserves_its_declared_lifetime() {
    let mut probe = Probe::new(100);
    probe.capped = false;
    let engine = handoff(probe, Some(100), 60.);
    assert_eq!(engine.venue().admission, Some(Ok(160.)));
    assert_eq!(engine.venue().geometry, Some((100, 1, 1)));
}

#[test]
fn unstamped_nonadvancing_receipt_cannot_fall_back_to_an_uncapped_preflight() {
    let mut probe = Probe::new(100);
    probe.unstamped = true;
    let mut engine = support::seeded_engine(probe);
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine.handle(
        f64::MAX,
        Event::Command(Command::Query {
            command: 7,
            query: Query::Income {
                symbol: None,
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }),
    );
    assert_eq!(engine.venue().entry_wait, None);
    let actions: Vec<_> = engine.drain_actions().collect();
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::STARTUP_INCOMPLETE =>
            {
                None
            }
            other => panic!("unexpected receipt-refusal effect: {other:?}"),
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(*failures[0], Error::Revoked.read_venue_error());
    assert!(!engine.venue().handoff_validated);
    assert_eq!(engine.venue().selected_slots, 0);
    assert_eq!(
        engine.venue().capture,
        Some(Err(CommandEntryError::Rejected(Error::Revoked)))
    );
}
