#![cfg(feature = "test-support")]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Context, Operation, Policy, ReconciliationData, RetryPolicy,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, Cx, Engine, EngineUnknownOutcome, ErrorKind, Event, Level, LogEvent, Method,
    Notice, Outcome, RequestId, Response, SocketId, TimerId, UNKNOWN_OUTCOME_TEXT,
    UnknownOutcomeTelemetry, Value, Venue, VenueError,
};

struct Demo<const REGISTERED: bool>;

fn telemetry(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    // Even a hostile edit of an owned copy has no journal or completion authority.
    let mut copy = facts.error.clone();
    copy.kind = Some(ErrorKind::Rejected);
    copy.outcome = Some(Outcome::Rejected);
    copy.message = "pretend the write was rejected".into();
    LogEvent {
        level: Level::Warn,
        code: "order.outcome_unknown",
        message: UNKNOWN_OUTCOME_TEXT.into(),
        fields: vec![
            ("command", facts.command.into()),
            ("request", facts.request.0.to_string().into()),
            ("status", facts.status.map_or(0, u32::from).into()),
            ("now", facts.now.into()),
            ("kind", facts.error.kind.unwrap().as_str().into()),
            ("outcome", "unknown".into()),
            ("editedCopy", copy.message.into()),
        ],
    }
}

impl<const REGISTERED: bool> Venue for Demo<REGISTERED> {
    type Pending = Context;
    fn unknown_outcome_telemetry() -> Option<UnknownOutcomeTelemetry> {
        REGISTERED.then_some(telemetry)
    }
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {
        panic!("managed failure uses its typed callback")
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, error: VenueError) {
        cx.fail_context(&context, error).unwrap();
    }
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {
        panic!("engine-sealed response must not enter a native decoder")
    }
    fn capture_http_reconciliation(
        &self,
        _: &Context,
    ) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
        ReconciliationData::new("telemetry-demo".into())
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let context = cx
            .admit_http_command(Policy {
                operation: Operation::Mutation,
                credential: None,
                timeout_ms: 1000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .unwrap();
        cx.request_managed(
            context.clone(),
            None,
            Method::Post,
            "https://venue.test/order".into(),
            Some("{}".into()),
            context,
        )
        .unwrap();
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn released<const REGISTERED: bool>(frame_fed: bool) -> (Engine<Demo<REGISTERED>>, RequestId) {
    let mut engine = support::seeded_engine(Demo::<REGISTERED>);
    if frame_fed {
        engine.set_frame_fed().unwrap();
    }
    engine
        .store_mut()
        .set_markets(vec![safe_cex_engine::model::Market {
            id: "BTCUSD".into(),
            symbol: "BTCUSD".into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USD".into(),
            active: true,
            precision: safe_cex_engine::model::Precision {
                amount: 0.001,
                price: 0.1,
            },
            limits: safe_cex_engine::model::Limits {
                amount: safe_cex_engine::model::MinMax {
                    min: 0.001,
                    max: 100.,
                },
                leverage: safe_cex_engine::model::MinMax { min: 1., max: 100. },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }]);
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine.handle(
        1.,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: vec![],
        }),
    );
    let requests: Vec<_> = prepared_actions(&mut engine, 2.)
        .into_iter()
        .filter_map(|action| {
            if let Action::Http(request) = action {
                Some(request.id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests.len(), 1);
    (engine, requests[0])
}

fn failure(actions: &[Action]) -> &VenueError {
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert!(!actions.iter().any(|a| match a {
        Action::Http(_) | Action::Notice(Notice::CommandDone { .. }) => true,
        Action::ManagedHttp(action) => matches!(
            **action,
            safe_cex_engine::request_context::HttpAction::Prepare { .. }
                | safe_cex_engine::request_context::HttpAction::Bind { .. }
        ),
        _ => false,
    }));
    failures[0]
}

fn events(actions: &[Action]) -> Vec<&LogEvent> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                Some(event)
            }
            _ => None,
        })
        .collect()
}

fn status_reply<const REGISTERED: bool>(
    engine: &mut Engine<Demo<REGISTERED>>,
    request: RequestId,
    status: u16,
    split: bool,
) -> Vec<Action> {
    if split {
        engine.handle(
            3.,
            Event::HttpStart {
                request,
                status,
                headers: "",
            },
        );
        engine.handle(
            4.,
            Event::HttpChunk {
                request,
                data: b"unproven",
            },
        );
        engine.handle(6., Event::HttpEnd { request });
    } else {
        engine.handle(
            6.,
            Event::HttpResponse {
                request,
                response: Response {
                    status,
                    headers: "",
                    body: "unproven",
                },
            },
        );
    }
    engine.drain_actions().collect()
}

#[test]
fn registered_engine_sealed_503_emits_once_with_read_only_facts() {
    for frame_fed in [false, true] {
        for split in [false, true] {
            for (status, kind) in [
                (408, ErrorKind::Timeout),
                (429, ErrorKind::RateLimited),
                (503, ErrorKind::VenueUnavailable),
            ] {
                let (mut engine, request) = released::<true>(frame_fed);
                let actions = status_reply(&mut engine, request, status, split);
                assert_eq!(failure(&actions).outcome, Some(Outcome::Unknown));
                assert_eq!(failure(&actions).kind, Some(kind));
                let events = events(&actions);
                assert_eq!(events.len(), 1);
                for (key, value) in [
                    ("command", Value::Num(7.)),
                    ("request", Value::Str(request.0.to_string())),
                    ("status", Value::Num(status.into())),
                    ("now", Value::Num(6.)),
                    ("kind", kind.as_str().into()),
                ] {
                    assert!(events[0].fields.contains(&(key, value)));
                }
                engine.handle(
                    7.,
                    Event::HttpResponse {
                        request,
                        response: Response {
                            status: 200,
                            headers: "",
                            body: "late",
                        },
                    },
                );
                assert!(engine.drain_actions().next().is_none());
            }
        }
    }
}

#[test]
fn absent_callback_preserves_existing_error_and_actions() {
    let (mut plain, request) = released::<false>(false);
    let actions = status_reply(&mut plain, request, 503, false);
    let error = failure(&actions);
    assert_eq!(
        error.message,
        "HTTP response does not prove the write outcome. The outcome is unknown. Check Open Orders"
    );
    assert_eq!(error.kind, Some(ErrorKind::VenueUnavailable));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(events(&actions).is_empty());
    let (mut registered, request) = released::<true>(false);
    let observed = status_reply(&mut registered, request, 503, false);
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        serde_json::to_value(failure(&observed)).unwrap()
    );
}

#[test]
fn callback_cannot_reclassify_or_retry_a_lost_transport() {
    let (mut engine, request) = released::<true>(true);
    engine.handle(
        6.,
        Event::HttpFailed {
            request,
            error: "connection reset",
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let error = failure(&actions);
    assert_eq!(error.kind, Some(ErrorKind::Network));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_ne!(error.message, "pretend the write was rejected");
    assert_eq!(events(&actions).len(), 1);
    assert!(
        events(&actions)[0]
            .fields
            .contains(&("status", Value::Num(0.)))
    );
    engine.handle(
        7.,
        Event::HttpFailed {
            request,
            error: "duplicate",
        },
    );
    assert!(engine.drain_actions().next().is_none());
}
