//! Original outcome assertions cut over to command-owned guarded Plan history.
#![cfg(all(feature = "http-plans", feature = "test-support"))]
mod support;

use safe_cex_engine::book::Book;
use safe_cex_engine::history::{PlanHistoryRows, PlanReadCompletion, ReadCompletion};
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
    rows: Option<PlanHistoryRows<String>>,
    stale: Option<PlanHistoryRows<String>>,
    stale_timer: Option<TimerId>,
    live: Option<Context>,
    pages: usize,
    page_deadline: f64,
    rejected_context_casts: usize,
    read_failed_call: Option<Result<(), Error>>,
    failure: Option<VenueError>,
    mode: String,
    encoder_runs: usize,
    terminal_refusal: Option<Error>,
    recovery_refusal: Option<Error>,
    last_context: Option<Context>,
    foreign: Option<Context>,
    foreign_refusal: Option<Option<Error>>,
    exact_failure_live: bool,
}

// Only construction and page admission replace the original numeric adapter.
fn owner<P>(cx: &mut Cx<'_, P>, plan: &Plan) -> PlanHistoryRows<String> {
    PlanHistoryRows::new(PlanReadCompletion::from_plan(cx, plan).unwrap())
}

impl Fixture {
    fn issue(&mut self, cx: &mut Cx<'_, Page>) {
        let plan = self.plan.as_ref().unwrap();
        let slot = cx.plan_slots(plan).unwrap().into_iter().next().unwrap();
        let context = cx.plan_leg_context(plan, &slot).unwrap();
        self.page_deadline = context.deadline();
        self.last_context = Some(context.clone());
        assert!(ReadCompletion::from_context(cx, 7, context.clone()).is_err());
        self.rejected_context_casts += 1;
        let request = cx
            .request_managed(
                context.clone(),
                Method::Get,
                "https://history.test/page".into(),
                None,
                Page { context, slot },
            )
            .unwrap_or_else(|_| panic!("selected Plan slot must queue managed HTTP"));
        if self.mode == "stream-failed" && self.pages == 1 {
            cx.stream_response(request, safe_cex_engine::json::ArrayAt::Root);
        }
    }
}

impl Venue for Fixture {
    type Pending = Page;
    fn start(&mut self, _: &mut Cx<'_, Page>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Page>, command: Command) {
        let Command::Query {
            command: _,
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
        self.mode = kind.clone().unwrap_or_default();
        let preflight = cx
            .begin_preflight_with_command_wait(PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly,
                read_credential: None,
                write_credential: None,
            })
            .unwrap();
        let calls: [CallBound<'static>; 3] = std::array::from_fn(|_| CallBound {
            timeout_ms: 100,
            attempts: 1,
            retry_window: RetryWindow::Shared,
            after_refusal: &[],
        });
        let parallel = [StepBound {
            wait_ms: 0,
            calls: &calls[..if self.mode == "latest" { 3 } else { 2 }],
        }];
        let declaration = if matches!(self.mode.as_str(), "peers" | "latest") {
            &parallel[..]
        } else {
            &STEPS[..]
        };
        let plan = cx
            .admit_plan(
                &preflight,
                declaration,
                PlanPolicy {
                    operation: Operation::Read,
                    credential: None,
                    pacing: None,
                    retry: RetryPolicy::Never,
                    independent_calls: false,
                },
            )
            .unwrap();
        if kind.as_deref() == Some("stale") {
            self.stale = Some(owner(cx, &plan));
            self.stale
                .as_mut()
                .unwrap()
                .reject(cx, VenueError::new("fixture Plan sealed"))
                .unwrap();
            self.stale_timer = Some(cx.set_timer(1.));
            return;
        }
        self.rows = Some(owner(cx, &plan));
        self.plan = Some(plan);
        self.issue(cx);
        if matches!(self.mode.as_str(), "peers" | "latest") {
            self.issue(cx);
        }
        if self.mode == "latest" {
            self.issue(cx);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Page>, page: Page, response: Response<'_>) {
        let plan = self.plan.as_ref().unwrap();
        cx.validate_plan_delivery(plan, &page.context).unwrap();
        assert!(ReadCompletion::from_context(cx, 7, page.context.clone()).is_err());
        let mut rows = self.rows.take().unwrap();
        let mut admitted = rows.admit(cx, &page.slot, &page.context).unwrap();
        admitted.stage(response.body.to_owned()).unwrap();
        match self.mode.as_str() {
            "early" => admitted
                .finish_with(|rows| {
                    self.encoder_runs += 1;
                    Ok(format!("[{}]", rows.join(",")))
                })
                .unwrap(),
            "encoder-panic" => admitted
                .finish_with(|_| panic!("encoder panic after preparation"))
                .unwrap(),
            "encoder-error" => admitted
                .finish_with(|_| {
                    self.encoder_runs += 1;
                    Err(VenueError::new("encode refused"))
                })
                .unwrap(),
            "peers" if self.pages == 0 => {
                self.terminal_refusal = admitted
                    .finish_with(|_| {
                        self.encoder_runs += 1;
                        Ok("invalid".into())
                    })
                    .err();
                assert_eq!(self.terminal_refusal, Some(Error::State));
                assert_eq!(self.encoder_runs, 0);
                // The refused terminal leaves this exact call and staged page usable.
                rows.admit(cx, &page.slot, &page.context)
                    .unwrap()
                    .accept_page()
                    .unwrap();
            }
            "peers" => admitted
                .finish_with(|rows| {
                    self.encoder_runs += 1;
                    Ok(format!("[{}]", rows.join(",")))
                })
                .unwrap(),
            "latest" => admitted.accept_page().unwrap(),
            _ => admitted.accept_page().unwrap(),
        }
        self.pages += 1;
        self.rows = Some(rows);
        if !matches!(
            self.mode.as_str(),
            "early" | "encoder-error" | "peers" | "latest"
        ) {
            assert_eq!(
                self.pages, 1,
                "test injects failure on the next physical page"
            );
            self.issue(cx);
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Page>, page: &mut Page, item: &str) {
        self.rows
            .as_mut()
            .unwrap()
            .admit(cx, &page.slot, &page.context)
            .unwrap()
            .stage(item.to_owned())
            .unwrap();
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Page>, page: Page, error: VenueError) {
        let plan = self.plan.as_ref().unwrap();
        self.read_failed_call = Some(cx.finish_plan_failed_call(plan, &page.slot, &page.context));
        self.failure = Some(error.clone());
        let mut rows = self.rows.take().unwrap();
        self.exact_failure_live = rows.admit_failure(cx, &page.slot, &page.context).is_ok();
        if let Some(foreign) = &self.foreign {
            assert!(
                foreign == &page.context,
                "foreign probe matches every equality field"
            );
            self.exact_failure_live = rows.admit_failure(cx, &page.slot, &page.context).is_ok();
            self.foreign_refusal = Some(rows.admit_failure(cx, &page.slot, foreign).err());
        }
        // The finalizer must never receive this unaccepted failed-page prefix.
        assert!(
            rows.admit(cx, &page.slot, &page.context).is_err(),
            "failed DATA cannot stage 999"
        );
        if let Ok(failed) = rows.admit_failure(cx, &page.slot, &page.context) {
            let result = failed.finish_with(error.clone(), |rows| {
                self.encoder_runs += 1;
                if self.mode == "empty-shape" {
                    Ok("[]".into())
                } else {
                    Ok(format!("[{}]", rows.join(",")))
                }
            });
            if let Err(reason) = result {
                self.terminal_refusal = Some(reason);
                let _ = rows.reject(cx, error);
            }
        } else {
            self.recovery_refusal = Some(Error::Revoked);
            let _ = rows.reject(cx, error);
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Page>,
        page: Page,
        _: Error,
        error: VenueError,
    ) {
        self.on_http_failed(cx, page, error);
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
        assert!(
            stale
                .reject(cx, VenueError::new("stale Plan history must stay silent"))
                .is_err()
        );
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

fn data(engine: &mut Engine<Fixture>, packet: &HttpRequest, body: &str, now: f64) {
    engine.handle(
        now,
        Event::HttpResponse {
            request: packet.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn successful_results(actions: &[Action]) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: 7, result }) => Some(result.as_str()),
            _ => None,
        })
        .collect()
}
#[test]
fn final_data_accepts_page_and_retires_the_unissued_suffix_once() {
    let mut engine = engine();
    engine.handle(1., query(7, "early"));
    let (packet, _) = released(&mut engine, 1.);
    data(&mut engine, &packet, "1", 2.);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(successful_results(&actions), ["[1]"]);
    assert_eq!(engine.venue().encoder_runs, 1);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_)) || marker(a, 7))
    );
    data(&mut engine, &packet, "999", 3.);
    assert!(
        !engine
            .drain_actions()
            .any(|a| terminal(&a, 7) || marker(&a, 7))
    );
}
#[test]
fn encoder_failure_has_one_strict_terminal_without_a_marker_or_suffix_send() {
    let mut engine = engine();
    engine.handle(1., query(7, "encoder-error"));
    let (packet, _) = released(&mut engine, 1.);
    data(&mut engine, &packet, "1", 2.);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    assert!(successful_results(&actions).is_empty());
    assert!(
        !actions
            .iter()
            .any(|a| marker(a, 7) || matches!(a, Action::Http(_)))
    );
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.message == "encode refused" && error.kind == Some(ErrorKind::Internal))));
}
#[test]
fn issued_peer_refuses_preparation_before_encoder_and_preserves_both_calls() {
    let mut engine = engine();
    engine.handle(1., query(7, "peers"));
    let packets: Vec<_> = prepared_actions(&mut engine, 1.)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(packet) => Some(packet),
            _ => None,
        })
        .collect();
    assert_eq!(packets.len(), 2);
    data(&mut engine, &packets[0], "1", 2.);
    assert_eq!(engine.venue().terminal_refusal, Some(Error::State));
    assert!(
        !engine
            .drain_actions()
            .any(|a| terminal(&a, 7) || marker(&a, 7))
    );
    data(&mut engine, &packets[1], "2", 3.);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(successful_results(&actions), ["[1,2]"]);
    assert_eq!(engine.venue().encoder_runs, 1);
}
#[test]
fn accepted_prefix_witness_survives_pure_shaping_to_empty_output() {
    let mut engine = engine();
    engine.handle(1., query(7, "empty-shape"));
    let (first, _) = released(&mut engine, 1.);
    data(&mut engine, &first, "1", 2.);
    let (second, _) = released(&mut engine, 2.);
    engine.handle(
        3.,
        Event::HttpFailed {
            request: second.id,
            error: "network gone",
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(successful_results(&actions), ["[]"]);
    assert_eq!(actions.iter().filter(|a| marker(a, 7)).count(), 1);
    assert_eq!(engine.venue().encoder_runs, 1);
}
#[test]
fn an_older_released_peer_failure_cannot_recover_an_accepted_prefix() {
    let mut engine = engine();
    engine.handle(1., query(7, "latest"));
    let packets: Vec<_> = prepared_actions(&mut engine, 1.)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(packet) => Some(packet),
            _ => None,
        })
        .collect();
    assert_eq!(packets.len(), 3);
    data(&mut engine, &packets[0], "1", 2.);
    let _ = engine.drain_actions().count();
    engine.handle(
        3.,
        Event::HttpFailed {
            request: packets[1].id,
            error: "older transport gone",
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(engine.venue().recovery_refusal, Some(Error::Revoked));
    assert_eq!(engine.venue().encoder_runs, 0);
    assert_eq!(actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    assert!(!actions.iter().any(|a| marker(a, 7)));
    assert!(successful_results(&actions).is_empty());
}

fn foreign_failure_context(physical_timeout: bool) {
    let mut foreign_engine = engine();
    foreign_engine.handle(1., query(7, "paged"));
    let foreign = foreign_engine.venue().last_context.clone().unwrap();
    let mut engine = support::seeded_engine(Fixture {
        foreign: Some(foreign),
        ..Default::default()
    });
    engine.set_log_level(safe_cex_engine::Level::Error);
    engine.start(0.);
    engine.handle(1., query(7, "paged"));
    let (packet, actions) = released(&mut engine, 1.);
    if physical_timeout {
        let deadline = engine.venue().page_deadline;
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == deadline => Some(*timer),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 1);
        engine.handle(deadline, Event::TimerFired { timer: timers[0] });
    } else {
        engine.handle(
            2.,
            Event::HttpFailed {
                request: packet.id,
                error: "genuine network failure",
            },
        );
    }
    assert!(
        engine.venue().exact_failure_live,
        "same-engine current Context remains a working positive"
    );
    assert_eq!(engine.venue().foreign_refusal, Some(Some(Error::Revoked)));
}
#[test]
fn foreign_equal_context_cannot_borrow_network_failure_proof() {
    foreign_failure_context(false);
}
#[test]
fn foreign_equal_context_cannot_borrow_expired_physical_timeout_proof() {
    foreign_failure_context(true);
}

fn fractional_cap_failure(at: f64, recovers: bool) {
    let mut engine = engine();
    let Event::Command(command) = query(7, "paged") else {
        unreachable!()
    };
    engine.handle_command(0.25, command, Some(150)).unwrap();
    assert_eq!(engine.venue().plan.as_ref().unwrap().deadline(), 150.25);
    let (first, _) = released(&mut engine, 0.25);
    data(&mut engine, &first, "1", 1.25);
    let (second, _) = released(&mut engine, 1.25);
    engine.handle(
        at,
        Event::HttpFailed {
            request: second.id,
            error: "late physical failure",
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    if recovers {
        assert_eq!(successful_results(&actions), ["[1]"]);
        assert_eq!(actions.iter().filter(|a| marker(a, 7)).count(), 1);
    } else {
        assert!(successful_results(&actions).is_empty());
        assert!(!actions.iter().any(|a| marker(a, 7)));
    }
}
#[test]
fn fractional_caller_cap_remains_live_half_a_millisecond_before_deadline() {
    fractional_cap_failure(149.75, true);
}
#[test]
fn fractional_caller_cap_expires_strictly_at_equality() {
    fractional_cap_failure(150.25, false);
}
#[test]
fn a_preparation_timeout_after_accepted_rows_cannot_mint_released_failure_proof() {
    let mut engine = engine();
    engine.handle(1., query(7, "paged"));
    let (first, _) = released(&mut engine, 1.);
    data(&mut engine, &first, "1", 2.);
    let actions: Vec<_> = engine.drain_actions().collect();
    let preparation = actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                safe_cex_engine::request_context::HttpAction::Prepare { preparation, .. } => {
                    Some(*preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.http_preparation_failed(
        3.,
        preparation,
        Error::TimedOut,
        VenueError::new("unreleased preparation expired"),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(engine.venue().recovery_refusal, Some(Error::Revoked));
    assert_eq!(engine.venue().encoder_runs, 0);
    assert_eq!(actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    assert!(successful_results(&actions).is_empty());
    assert!(!actions.iter().any(|a| marker(a, 7)));
}

#[test]
fn latest_failure_is_actual_claim_order_and_issued_peer_still_blocks_recovery() {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    let mut engine = engine();
    engine.handle(1., query(7, "latest"));
    let actions: Vec<_> = engine.drain_actions().collect();
    for action in actions {
        if let Action::ManagedHttp(action) = action
            && let HttpAction::Prepare {
                preparation,
                request,
                ..
            } = *action
        {
            engine.http_prepared(
                1.,
                preparation,
                Prepared {
                    id: request.id.0,
                    expires_at: None,
                },
            );
        }
    }
    let mut binds = Vec::new();
    for action in engine.drain_actions() {
        if let Action::ManagedHttp(action) = action
            && let HttpAction::Bind {
                permit, request, ..
            } = *action
        {
            binds.push((permit, request));
        }
    }
    assert_eq!(binds.len(), 3);
    // Ordinal 1 releases last, after ordinal 2. A max-slot shortcut is wrong.
    for index in [0, 2, 1] {
        assert!(matches!(
            engine.claim_http(1., binds[index].0),
            Ok(Claim::Released(_))
        ));
    }
    let _ = engine.drain_actions().count();
    data(&mut engine, &binds[0].1, "1", 2.);
    let _ = engine.drain_actions().count();
    engine.handle(
        3.,
        Event::HttpFailed {
            request: binds[1].1.id,
            error: "latest actual released failure",
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(engine.venue().exact_failure_live);
    assert_eq!(engine.venue().terminal_refusal, Some(Error::State));
    assert_eq!(engine.venue().encoder_runs, 0);
    assert_eq!(actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    assert!(successful_results(&actions).is_empty());
    assert!(!actions.iter().any(|a| marker(a, 7)));
}
#[test]
fn a_panicking_pure_encoder_permanently_fences_the_engine() {
    let mut engine = engine();
    engine.handle(1., query(7, "encoder-panic"));
    let (packet, _) = released(&mut engine, 1.);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        data(&mut engine, &packet, "1", 2.)
    }));
    assert!(result.is_err());
    assert!(engine.failed());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || engine.handle(3., query(8, "live"))
        ))
        .is_err()
    );
}

#[test]
fn streamed_failed_page_discards_staged_rows_and_keeps_only_accepted_prefix() {
    let mut engine = engine();
    engine.handle(1., query(7, "stream-failed"));
    let (first, _) = released(&mut engine, 1.);
    data(&mut engine, &first, "1", 2.);
    let (second, _) = released(&mut engine, 2.);
    engine.handle(
        3.,
        Event::HttpStart {
            request: second.id,
            status: 200,
            headers: "",
        },
    );
    engine.handle(
        3.,
        Event::HttpChunk {
            request: second.id,
            data: b"[999,",
        },
    );
    assert_eq!(engine.venue().rows.as_ref().unwrap().staged_len(), 1);
    assert_eq!(engine.venue().rows.as_ref().unwrap().accepted_len(), 1);
    assert_eq!(engine.venue().encoder_runs, 0);
    engine.handle(
        4.,
        Event::HttpFailed {
            request: second.id,
            error: "stream transport lost",
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(successful_results(&actions), ["[1]"]);
    assert_eq!(actions.iter().filter(|a| marker(a, 7)).count(), 1);
    assert_eq!(actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    assert_eq!(engine.venue().encoder_runs, 1);
}

fn malformed_page_after_accepted_prefix(stream: bool) {
    let mut engine = engine();
    engine.handle(1., query(7, if stream { "stream-failed" } else { "paged" }));
    let (first, _) = released(&mut engine, 1.);
    data(&mut engine, &first, "1", 2.);
    let (second, _) = released(&mut engine, 2.);
    engine.handle(
        3.,
        Event::HttpStart {
            request: second.id,
            status: 200,
            headers: "",
        },
    );
    engine.handle(
        3.,
        Event::HttpChunk {
            request: second.id,
            data: if stream { b"[999," } else { b"\xff" },
        },
    );
    if stream {
        assert_eq!(engine.venue().rows.as_ref().unwrap().staged_len(), 1);
    }
    engine.handle(4., Event::HttpEnd { request: second.id });
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(
        engine.venue().failure.as_ref().unwrap().kind,
        Some(ErrorKind::Internal)
    );
    assert!(!engine.venue().exact_failure_live);
    assert_eq!(engine.venue().encoder_runs, 0);
    assert_eq!(actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    assert!(successful_results(&actions).is_empty());
    assert!(!actions.iter().any(|a| marker(a, 7)));
}
#[test]
fn delivered_non_utf8_page_cannot_recover_an_accepted_prefix() {
    malformed_page_after_accepted_prefix(false);
}
#[test]
fn malformed_stream_after_staged_rows_cannot_recover_an_accepted_prefix() {
    malformed_page_after_accepted_prefix(true);
}
