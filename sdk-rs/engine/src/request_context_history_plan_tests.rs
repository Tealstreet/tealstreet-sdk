use super::*;
use crate::book::Book;
use crate::history::{PlanHistoryRows, PlanReadCompletion};
use crate::request_context::plan::{CallBound, RetryWindow, StepBound};
use crate::request_context::{PlanPolicy, PreflightPolicy, PreflightPurpose, RetryPolicy};
use crate::{
    Command, Engine, Event, HttpRequest, Method, Notice, Query, Response, SocketId, TimerId, Venue,
};

#[derive(Default)]
struct Probe {
    plan: Option<Plan>,
    rows: Option<PlanHistoryRows<String>>,
    slot: Option<Slot>,
    accepted: bool,
    nested: bool,
    proof_before: bool,
    proof_inside: bool,
    proof_after: bool,
    refuse_cleanup: bool,
    cleanup_refused: bool,
}
impl Probe {
    fn issue(&mut self, cx: &mut Cx<'_, Context>) {
        let plan = self.plan.as_ref().unwrap();
        let slot = cx.plan_slots(plan).unwrap().remove(0);
        let context = cx.plan_leg_context(plan, &slot).unwrap();
        cx.request_managed(
            context.clone(),
            Method::Get,
            "https://history.test/boundary".into(),
            None,
            context,
        )
        .unwrap();
        self.slot = Some(slot);
    }
}
impl Venue for Probe {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let preflight = cx
            .begin_preflight_with_command_wait(PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly,
                read_credential: None,
                write_credential: None,
            })
            .unwrap();
        let calls = [CallBound {
            timeout_ms: 100,
            attempts: 1,
            retry_window: RetryWindow::Shared,
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
        let plan = cx
            .admit_plan(
                &preflight,
                &steps,
                PlanPolicy {
                    operation: Operation::Read,
                    credential: None,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        self.rows = Some(PlanHistoryRows::new(
            PlanReadCompletion::from_plan(cx, &plan).unwrap(),
        ));
        self.plan = Some(plan);
        self.issue(cx);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        assert!(!self.accepted);
        if self.refuse_cleanup {
            let plan = self.plan.as_ref().unwrap();
            let body = cx.io.request_contexts.plan_body(plan).unwrap();
            let index = body.schedule.ordinal(self.slot.as_ref().unwrap()).unwrap();
            let credit = body.entries[index].credit;
            let handoff = body.handoff.clone();
            let actions = cx.io.actions.len();
            let contexts = cx.io.request_contexts.contexts.len();
            let managed = cx.io.managed_http.len();
            let ids = cx.io.next_id;
            // One real managed request makes this an impossible Vec reservation.
            let refused = cx.io.prepare_read_history_cleanup(
                plan.owner(),
                Some(context.leg()),
                usize::MAX - 4,
            );
            assert!(matches!(refused, Err(Error::Capacity)));
            assert_eq!(cx.io.actions.len(), actions);
            assert_eq!(cx.io.request_contexts.contexts.len(), contexts);
            assert_eq!(cx.io.managed_http.len(), managed);
            assert_eq!(cx.io.next_id, ids);
            let body = cx.io.request_contexts.plan_body(plan).unwrap();
            assert!(body.entries[index].credit == credit);
            assert_eq!(body.handoff, handoff);
            assert!(
                body.entries[index]
                    .call
                    .as_ref()
                    .unwrap()
                    .context
                    .same_issuer(&context)
            );
            assert_eq!(self.rows.as_ref().unwrap().accepted_len(), 0);
            self.cleanup_refused = true;
        }
        let mut page = self
            .rows
            .as_mut()
            .unwrap()
            .admit(cx, self.slot.as_ref().unwrap(), &context)
            .unwrap();
        page.stage(response.body.to_owned()).unwrap();
        page.accept_page().unwrap();
        self.accepted = true;
        self.issue(cx);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, error: VenueError) {
        let plan = self.plan.as_ref().unwrap();
        let slot = self.slot.as_ref().unwrap();
        self.proof_before = cx
            .validate_read_plan_history_failure(plan, slot, &context)
            .is_ok();
        if self.nested {
            cx.with_owned_read_failure(
                || error.clone(),
                |cx| {
                    self.proof_inside = cx
                        .validate_read_plan_history_failure(plan, slot, &context)
                        .is_ok();
                },
            );
            assert!(
                cx.io.http_history_failure.is_none(),
                "nested failure cannot restore the outer proof"
            );
        }
        self.proof_after = cx
            .validate_read_plan_history_failure(plan, slot, &context)
            .is_ok();
        let rows = self.rows.as_mut().unwrap();
        if let Ok(failure) = rows.admit_failure(cx, slot, &context) {
            failure
                .finish_with(error, |rows| Ok(format!("[{}]", rows.join(","))))
                .unwrap();
        } else {
            let _ = rows.reject(cx, error);
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {
        panic!("typed failure required");
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn packet(engine: &mut Engine<Probe>, now: f64) -> HttpRequest {
    crate::test_support::prepared_actions(engine, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(packet) => Some(packet),
            _ => None,
        })
        .unwrap()
}
fn accepted_prefix(nested: bool) -> (Engine<Probe>, HttpRequest) {
    accepted_prefix_with(Probe {
        nested,
        ..Default::default()
    })
}
fn accepted_prefix_with(probe: Probe) -> (Engine<Probe>, HttpRequest) {
    let mut engine = crate::test_support::seeded_engine(probe);
    engine.start(0.);
    engine.handle(
        1.,
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
    let first = packet(&mut engine, 1.);
    engine.handle(
        2.,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 200,
                headers: "",
                body: "1",
            },
        },
    );
    let second = packet(&mut engine, 2.);
    assert_eq!(engine.venue().rows.as_ref().unwrap().accepted_len(), 1);
    (engine, second)
}
fn failed(engine: &mut Engine<Probe>, packet: HttpRequest) -> Vec<Action> {
    engine.handle(
        3.,
        Event::HttpFailed {
            request: packet.id,
            error: "boundary cause",
        },
    );
    engine.drain_actions().collect()
}
fn strict(actions: &[Action], kind: crate::ErrorKind) {
    assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.kind == Some(kind))).count(), 1);
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 7, .. })
    )));
    assert!(!actions.iter().any(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.code == crate::log::code::HISTORY_INCOMPLETE)));
}
#[test]
fn integrity_internal_failure_after_accepted_rows_has_no_transport_witness() {
    let (mut engine, packet) = accepted_prefix(false);
    engine.io.http_failure_kind = crate::ErrorKind::Internal;
    let actions = failed(&mut engine, packet);
    assert!(!engine.venue().proof_before && !engine.venue().proof_after);
    strict(&actions, crate::ErrorKind::Internal);
}
#[test]
fn nested_failure_callback_clears_proof_without_restoring_it() {
    let (mut engine, packet) = accepted_prefix(true);
    let actions = failed(&mut engine, packet);
    assert!(engine.venue().proof_before);
    assert!(!engine.venue().proof_inside && !engine.venue().proof_after);
    strict(&actions, crate::ErrorKind::Network);
}
#[test]
fn ordinary_network_callback_retains_exact_witness_until_its_terminal() {
    let (mut engine, packet) = accepted_prefix(false);
    let actions = failed(&mut engine, packet);
    assert!(engine.venue().proof_before && engine.venue().proof_after);
    assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[1]")).count(), 1);
}

#[test]
fn cleanup_reservation_refusal_preserves_slot_leg_handoff_and_accepted_rows() {
    let (mut engine, packet) = accepted_prefix_with(Probe {
        refuse_cleanup: true,
        ..Default::default()
    });
    assert!(engine.venue().cleanup_refused);
    assert_eq!(engine.venue().rows.as_ref().unwrap().accepted_len(), 1);
    let actions = failed(&mut engine, packet);
    assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[1]")).count(), 1);
    assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.code == crate::log::code::HISTORY_INCOMPLETE)).count(), 1);
}
