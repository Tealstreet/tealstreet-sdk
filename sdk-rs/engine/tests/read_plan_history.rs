//! Original compiling negative controls for command-owned Plan history.
//! Numeric history is the labelled unsafe adapter, not a Plan authority.
#![cfg(all(feature = "http-plans", feature = "test-support"))]
mod support;

use safe_cex_engine::book::Book;
use safe_cex_engine::history::{Completion, HistoryRows, ReadCompletion};
use safe_cex_engine::log::code;
use safe_cex_engine::request_context::{
    Context, Error, Operation, Plan, PlanPolicy, Policy, PreflightPolicy, PreflightPurpose,
    RetryPolicy,
    plan::{CallBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Query, Response,
    SocketId, TimerId, Value, Venue, VenueError,
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

struct Page {
    context: Context,
    slot: Slot,
}

#[derive(Default)]
struct Fixture {
    plan: Option<Plan>,
    rows: Option<HistoryRows<String>>,
    stale: Option<Completion>,
    stale_timer: Option<TimerId>,
    live: Option<Context>,
    pages: usize,
    page_deadline: f64,
    rejected_context_casts: usize,
    read_failed_call: Option<Result<(), Error>>,
    failure: Option<VenueError>,
}

// This numeric construction is the ORIGINAL negative reproduction seam only.
// The runtime fix must replace it with opaque Plan history authority.
fn owner(command: u32) -> Completion {
    Completion::new(command)
}

impl Fixture {
    fn issue(&mut self, cx: &mut Cx<'_, Page>) {
        let plan = self.plan.as_ref().unwrap();
        let slot = cx.plan_slots(plan).unwrap().into_iter().next().unwrap();
        let context = cx.plan_leg_context(plan, &slot).unwrap();
        self.page_deadline = context.deadline();
        assert!(ReadCompletion::from_context(cx, 7, context.clone()).is_err());
        self.rejected_context_casts += 1;
        cx.request_managed(
            context.clone(),
            Method::Get,
            "https://history.test/page".into(),
            None,
            Page { context, slot },
        )
        .unwrap_or_else(|_| panic!("selected Plan slot must queue managed HTTP"));
    }
}

impl Venue for Fixture {
    type Pending = Page;
    fn start(&mut self, _: &mut Cx<'_, Page>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Page>, command: Command) {
        let Command::Query {
            command,
            query: Query::Income { kind, .. },
        } = command
        else {
            panic!("fixture uses one genuine read command");
        };
        if kind.as_deref() == Some("live") {
            self.live = Some(
                cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 1_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                })
                .unwrap(),
            );
            return;
        }
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
        if kind.as_deref() == Some("stale") {
            self.stale = Some(owner(command));
            cx.fail_plan(&plan, VenueError::new("fixture Plan sealed"))
                .unwrap();
            self.stale_timer = Some(cx.set_timer(1.));
            return;
        }
        self.plan = Some(plan);
        self.rows = Some(HistoryRows::new(owner(command), |rows| {
            Ok(format!("[{}]", rows.join(",")))
        }));
        self.issue(cx);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Page>, page: Page, response: Response<'_>) {
        let plan = self.plan.as_ref().unwrap();
        cx.validate_plan_delivery(plan, &page.context).unwrap();
        assert!(
            ReadCompletion::from_context(cx, 7, page.context.clone()).is_err(),
            "a real Plan leg must never cast into original Context-root history"
        );
        let rows = self.rows.as_mut().unwrap();
        rows.stage(response.body.to_owned());
        rows.accept_page();
        cx.finish_plan_call(plan, &page.slot, &page.context)
            .unwrap();
        self.pages += 1;
        assert_eq!(
            self.pages, 1,
            "test injects failure on the next physical page"
        );
        self.issue(cx);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Page>, page: Page, error: VenueError) {
        let plan = self.plan.as_ref().unwrap();
        self.read_failed_call = Some(cx.finish_plan_failed_call(plan, &page.slot, &page.context));
        self.failure = Some(error.clone());
        let mut rows = self.rows.take().unwrap();
        // The finalizer must never receive this unaccepted failed-page prefix.
        rows.stage("999".into());
        rows.finish(cx, Err(error));
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Page>, _: Page, _: &str) {
        panic!("typed failure callback must be retained");
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Page>, timer: TimerId) {
        assert_eq!(self.stale_timer.take(), Some(timer));
        let live = self
            .live
            .take()
            .expect("other live command is the working control");
        cx.validate_read_root_context(&live).unwrap();
        let mut stale = self.stale.take().unwrap();
        stale.incomplete("stale Plan history must stay silent");
        stale.finish(cx, Ok("[999]".into()));
        cx.done_context(&live, "live-control".into()).unwrap();
    }
    fn on_open(&mut self, _: &mut Cx<'_, Page>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Page>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Page>, _: SocketId, _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn query(command: u32, kind: &str) -> Event<'_> {
    Event::Command(Command::Query {
        command,
        query: Query::Income {
            symbol: None,
            kind: Some(kind.into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    })
}
fn engine() -> Engine<Fixture> {
    let mut engine = support::seeded_engine(Fixture::default());
    engine.set_log_level(safe_cex_engine::Level::Error);
    engine.start(0.);
    engine
}
fn released(engine: &mut Engine<Fixture>, now: f64) -> (HttpRequest, Vec<Action>) {
    let mut actions = prepared_actions(engine, now);
    let index = actions
        .iter()
        .position(|a| matches!(a, Action::Http(_)))
        .expect("actual Prepare/Bind/Claim must release a packet");
    let Action::Http(request) = actions.remove(index) else {
        unreachable!()
    };
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    (request, actions)
}
fn terminal(action: &Action, id: u32) -> bool {
    matches!(action, Action::Notice(Notice::CommandDone { command, .. }
        | Notice::CommandFailed { command, .. }) if *command == id)
}
fn marker(action: &Action, id: u32) -> bool {
    matches!(action, Action::Notice(Notice::Event(event))
        if event.code == code::HISTORY_INCOMPLETE
        && event.fields.iter().any(|(key, value)|
            *key == "command" && *value == Value::Num(f64::from(id))))
}

#[derive(Clone, Copy)]
enum Failure {
    Network,
    PhysicalTimeout,
    RootExpiry,
}

fn accepted_page_then_failure(failure: Failure) {
    let mut engine = engine();
    engine.handle(1., query(7, "paged"));
    let (first, _) = released(&mut engine, 1.);
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
    let (second, between) = released(&mut engine, 2.);
    assert_eq!(engine.venue().pages, 1);
    assert!(!between.iter().any(|a| terminal(a, 7) || marker(a, 7)));
    match failure {
        Failure::Network => engine.handle(
            3.,
            Event::HttpFailed {
                request: second.id,
                error: "fixture transport lost",
            },
        ),
        Failure::PhysicalTimeout => {
            let deadline = engine.venue().page_deadline;
            assert!(deadline < engine.venue().plan.as_ref().unwrap().deadline());
            let timers = between
                .iter()
                .filter_map(|a| match a {
                    Action::SetTimer { timer, at_ms } if *at_ms == deadline => Some(*timer),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(timers.len(), 1, "exact second managed leg timer");
            engine.handle(deadline, Event::TimerFired { timer: timers[0] });
        }
        Failure::RootExpiry => {
            let deadline = engine.venue().plan.as_ref().unwrap().deadline();
            // Public dispatch observes logical expiry before transport recovery.
            engine.prepare_dispatch(deadline);
        }
    }
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    if matches!(failure, Failure::RootExpiry) {
        assert!(
            !actions.iter().any(|a| marker(a, 7)),
            "root expiry cannot recover accepted rows"
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
        );
    } else {
        let kind = match failure {
            Failure::Network => ErrorKind::Network,
            Failure::PhysicalTimeout => ErrorKind::Timeout,
            Failure::RootExpiry => unreachable!(),
        };
        assert_eq!(engine.venue().failure.as_ref().unwrap().kind, Some(kind));
        assert_eq!(actions.iter().filter(|a| marker(a, 7)).count(), 1);
        assert_eq!(
            actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::CommandDone { command: 7, result }) =>
                        Some(result.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            ["[1]"],
            "same-Plan accepted-prefix terminal"
        );
    }
}

#[test]
fn released_network_failure_needs_one_plan_owned_accepted_prefix_terminal() {
    accepted_page_then_failure(Failure::Network);
}

#[test]
fn released_physical_timeout_needs_one_plan_owned_accepted_prefix_terminal() {
    accepted_page_then_failure(Failure::PhysicalTimeout);
}

#[test]
fn logical_root_expiry_is_strict_after_accepted_page() {
    accepted_page_then_failure(Failure::RootExpiry);
}

#[test]
fn empty_accepted_history_rejects_without_marker_control() {
    let mut engine = engine();
    engine.handle(1., query(7, "paged"));
    let (first, _) = released(&mut engine, 1.);
    engine.handle(
        2.,
        Event::HttpFailed {
            request: first.id,
            error: "fixture transport lost",
        },
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(engine.venue().read_failed_call, Some(Err(Error::Invalid)));
    assert_eq!(actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    assert!(!actions.iter().any(|a| marker(a, 7)));
    let actual = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, [engine.venue().failure.as_ref().unwrap()]);
}

#[test]
fn numeric_history_cannot_publish_after_its_plan_sealed() {
    let mut engine = engine();
    engine.handle(1., query(7, "stale"));
    let initial = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(initial.iter().filter(|a| terminal(a, 7)).count(), 1);
    let timer = engine.venue().stale_timer.unwrap();
    engine.handle(1., query(8, "live"));
    assert!(!engine.drain_actions().any(|a| terminal(&a, 8)));
    engine.handle(2., Event::TimerFired { timer });
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandDone { command: 8, result }) => Some(result.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        ["live-control"]
    );
    assert_eq!(
        [
            actions.iter().filter(|a| terminal(a, 7)).count(),
            actions.iter().filter(|a| marker(a, 7)).count(),
        ],
        [0, 0],
        "a sealed Plan cannot emit another terminal or history coverage"
    );
}

#[test]
fn receipt_capped_plan_and_context_cast_refusal_control() {
    let mut engine = engine();
    let Event::Command(command) = query(7, "paged") else {
        unreachable!()
    };
    engine.handle_command(1., command, Some(150)).unwrap();
    assert_eq!(engine.venue().plan.as_ref().unwrap().deadline(), 151.);
    assert_eq!(engine.venue().rejected_context_casts, 1);
    let (packet, actions) = released(&mut engine, 1.);
    assert!(packet.url.ends_with("/page"));
    assert!(!actions.iter().any(|a| terminal(a, 7) || marker(a, 7)));
}
