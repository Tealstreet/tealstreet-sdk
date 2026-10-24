//! New-contract binding controls; these are not original legacy RED evidence.
#![cfg(all(feature = "http-plans", feature = "test-support"))]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::history::PlanReadCompletion;
use safe_cex_engine::request_context::{
    Context, Error, Operation, Plan, PlanPolicy, PreflightPolicy, PreflightPurpose, RetryPolicy,
    plan::{CallBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Method, Notice, Query, Response, Venue,
    VenueError,
};
const CALLS: [CallBound<'static>; 1] = [CallBound {
    timeout_ms: 100,
    attempts: 1,
    retry_window: RetryWindow::Shared,
    after_refusal: &[],
}];
const STEPS: [StepBound<'static>; 2] = [
    StepBound {
        wait_ms: 0,
        calls: &CALLS,
    },
    StepBound {
        wait_ms: 0,
        calls: &CALLS,
    },
];
#[derive(Clone, Copy)]
enum Mode {
    Early,
    Late,
    Selected,
}
struct Page {
    slot: Slot,
    context: Context,
}
struct Probe {
    mode: Mode,
    plan: Option<Plan>,
    binding: Option<PlanReadCompletion>,
    pages: usize,
    expiry_callbacks: usize,
}
impl Probe {
    fn queue(&mut self, cx: &mut Cx<'_, Page>) {
        let plan = self.plan.as_ref().unwrap();
        let slot = cx.plan_slots(plan).unwrap().remove(0);
        self.queue_slot(cx, slot);
    }
    fn queue_slot(&mut self, cx: &mut Cx<'_, Page>, slot: Slot) {
        let plan = self.plan.as_ref().unwrap();
        let context = cx.plan_leg_context(plan, &slot).unwrap();
        cx.request_managed(
            context.clone(),
            Method::Get,
            "https://binding.test/page".into(),
            None,
            Page { slot, context },
        )
        .unwrap_or_else(|_| panic!("exact selected read queues"));
    }
}
impl Venue for Probe {
    type Pending = Page;
    fn start(&mut self, _: &mut Cx<'_, Page>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Page>, _: safe_cex_engine::SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Page>, _: safe_cex_engine::SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Page>, _: safe_cex_engine::SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Page>, _: safe_cex_engine::TimerId) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Page>, _: Page, _: &str) {
        panic!("typed managed failure required")
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Page>, _: Command) {
        let preflight = cx
            .begin_preflight_with_command_wait(PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly,
                read_credential: None,
                write_credential: None,
            })
            .unwrap();
        let plan = cx
            .admit_plan(
                &preflight,
                &STEPS,
                PlanPolicy {
                    operation: Operation::Read,
                    credential: None,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        self.plan = Some(plan.clone());
        match self.mode {
            Mode::Early => {
                let binding = PlanReadCompletion::from_plan(cx, &plan).unwrap();
                assert_eq!(binding.command(), 7);
                assert!(matches!(
                    PlanReadCompletion::from_plan(cx, &plan),
                    Err(Error::State)
                ));
                self.binding = Some(binding);
                self.queue(cx);
            }
            Mode::Late => self.queue(cx),
            Mode::Selected => {
                let slot = cx.plan_slots(&plan).unwrap().remove(0);
                let _context = cx.plan_leg_context(&plan, &slot).unwrap();
                assert!(matches!(
                    PlanReadCompletion::from_plan(cx, &plan),
                    Err(Error::State)
                ));
                cx.fail_plan(&plan, "unbound plan remains usable").unwrap();
            }
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Page>, page: Page, _: Response<'_>) {
        let plan = self.plan.as_ref().unwrap().clone();
        cx.validate_plan_delivery(&plan, &page.context).unwrap();
        match self.mode {
            Mode::Late if self.pages == 0 => {
                cx.finish_plan_call(&plan, &page.slot, &page.context)
                    .unwrap();
                let mut available = cx.plan_slots(&plan).unwrap();
                assert_eq!(available.len(), 1);
                let retained_slot = available.remove(0);
                assert!(
                    matches!(PlanReadCompletion::from_plan(cx, &plan), Err(Error::State)),
                    "binding after accepted DATA must be refused"
                );
                assert_eq!(cx.plan_slots(&plan).unwrap().len(), 1);
                self.pages += 1;
                // Reuse the exact Slot held before the refused bind.
                self.queue_slot(cx, retained_slot);
            }
            Mode::Late => {
                cx.done_read_plan_after_data(&plan, &page.slot, &page.context, "complete".into())
                    .unwrap();
                self.pages += 1;
            }
            Mode::Early => {
                let clone = plan.clone();
                assert_eq!(
                    cx.finish_plan_call(&clone, &page.slot, &page.context),
                    Err(Error::State)
                );
                assert_eq!(
                    cx.done_read_plan_after_data(
                        &clone,
                        &page.slot,
                        &page.context,
                        "unchecked".into()
                    ),
                    Err(Error::State)
                );
                assert_eq!(cx.fail_plan(&clone, "unchecked failure"), Err(Error::State));
                assert_eq!(cx.done_plan(&clone, "unchecked".into()), Err(Error::State));
                self.pages += 1;
            }
            Mode::Selected => unreachable!(),
        }
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Page>, _: Page, error: VenueError) {
        self.expiry_callbacks += 1;
        cx.fail_plan(self.plan.as_ref().unwrap(), error).unwrap();
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn engine(mode: Mode) -> Engine<Probe> {
    let mut e = support::seeded_engine(Probe {
        mode,
        plan: None,
        binding: None,
        pages: 0,
        expiry_callbacks: 0,
    });
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.handle_command(
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
        Some(1000),
    )
    .unwrap();
    e
}
fn release(e: &mut Engine<Probe>, now: f64) -> HttpRequest {
    let packets = prepared_actions(e, now)
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(packets.len(), 1);
    packets.into_iter().next().unwrap()
}
fn respond(e: &mut Engine<Probe>, r: HttpRequest, now: f64) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: "[]",
            },
        },
    )
}
#[test]
fn binding_after_settled_data_refuses_and_preserves_remaining_plan() {
    let mut e = engine(Mode::Late);
    let r = release(&mut e, 0.);
    respond(&mut e, r, 10.);
    let r = release(&mut e, 10.);
    respond(&mut e, r, 20.);
    assert_eq!(e.venue().pages, 2);
    let terminals=e.drain_actions().filter(|a|matches!(a,Action::Notice(Notice::CommandDone{command:7,result}) if result=="complete")).count();
    assert_eq!(terminals, 1);
}
#[test]
fn binding_after_selected_slot_refuses_and_preserves_failure_terminal() {
    let mut e = engine(Mode::Selected);
    assert_eq!(
        e.drain_actions()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
            .count(),
        1
    );
}
#[test]
fn early_binding_denies_clone_terminals_and_standalone_settlement() {
    let mut e = engine(Mode::Early);
    let r = release(&mut e, 0.);
    respond(&mut e, r, 10.);
    assert_eq!(e.venue().pages, 1);
    assert!(!e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
}
#[test]
fn bound_expiry_callback_public_failure_is_inert_then_engine_seals_once() {
    let mut e = engine(Mode::Early);
    let _r = release(&mut e, 0.);
    let deadline = e.venue().plan.as_ref().unwrap().deadline();
    e.prepare_dispatch(deadline);
    assert_eq!(e.venue().expiry_callbacks, 1);
    let failures = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(safe_cex_engine::ErrorKind::Timeout));
}
