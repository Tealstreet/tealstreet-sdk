use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
use safe_cex_engine::{EngineUnknownOutcome, LogEvent, RequestId, Venue, VenueError};
use safe_cex_venue_grvt::Grvt;

fn project(ids: Vec<String>, indices: Vec<usize>, request: u64, status: Option<u16>) -> LogEvent {
    let poison = "https://venue.test/exchange?api_key=SYNTHETIC&signature=SYNTHETIC_BODY";
    let mut error = VenueError::new(poison)
        .venue_code(poison)
        .venue_message(poison);
    error.partial = Some(Box::new(safe_cex_engine::Partial {
        order_ids: ids,
        total: None,
        failed_index: None,
    }));
    error.elements = Some(Box::new(ElementOutcomes(
        indices
            .into_iter()
            .map(|index| ElementOutcome {
                element: Element {
                    index,
                    client_order_id: Some(poison.into()),
                    role: None,
                },
                result: ElementResult::Unknown {
                    error: Box::new(VenueError::new(poison)),
                },
            })
            .collect(),
    )));
    let event = Grvt::unknown_outcome_telemetry().unwrap()(&EngineUnknownOutcome {
        now: 0.,
        command: u32::MAX,
        request: RequestId(request),
        status,
        error: &error,
    });
    assert_unknown_projection(&event);
    event
}

fn text<'a>(event: &'a LogEvent, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| value.as_str())
}

#[test]
fn projection_matches_nx_and_excludes_error_payloads() {
    for request in [0, 1, u64::MAX] {
        let event = project(
            vec!["1".into(), "9".repeat(32)],
            vec![0, 65535],
            request,
            Some(503),
        );
        assert_unknown_projection(&event);
        assert_eq!(text(&event, "command"), Some("command.4294967295"));
        assert_eq!(text(&event, "request"), Some(request.to_string().as_str()));
        assert_eq!(
            text(&event, "placedOrderIds"),
            Some(format!("1,{}", "9".repeat(32)).as_str())
        );
        assert_eq!(text(&event, "unknownIndices"), Some("0,65535"));
        assert_eq!(event.fields.len(), 6);
    }
}

#[test]
fn projection_status_bounds_and_absence() {
    for status in [
        None,
        Some(0),
        Some(99),
        Some(100),
        Some(599),
        Some(600),
        Some(u16::MAX),
    ] {
        let event = project(vec![], vec![], 1, status);
        let actual = event
            .fields
            .iter()
            .find(|(key, _)| *key == "status")
            .map(|(_, value)| value);
        let expected = status
            .filter(|s| (100..=599).contains(s))
            .map(safe_cex_engine::log::Value::from);
        assert_eq!(actual, expected.as_ref(), "status {status:?}");
        assert_eq!(text(&event, "placedOrderIds"), Some(""));
        assert_eq!(text(&event, "unknownIndices"), Some(""));
    }
}

#[test]
fn projection_caps_lists_at_sixteen_without_changing_evidence() {
    for count in [16, 17] {
        let event = project(
            vec![u64::MAX.to_string(); count],
            vec![65535; count],
            1,
            None,
        );
        assert_eq!(
            text(&event, "unknownIndices").unwrap().split(',').count(),
            16
        );
        assert_eq!(
            text(&event, "placedOrderIds").unwrap().split(',').count(),
            16
        );
        assert_eq!(event.fields.len(), 5);
    }
}

#[test]
fn projection_omits_invalid_lists_as_a_whole() {
    for invalid in [
        "",
        "abc",
        "0x123",
        "1,2",
        "123\n",
        "https://venue.test/exchange?signature=SYNTHETIC_BODY",
        &"9".repeat(33),
    ] {
        let event = project(vec!["1".into(), invalid.into()], vec![0], 1, None);
        assert_eq!(text(&event, "placedOrderIds"), None, "invalid native ID");
    }
    let event = project(vec!["1".into()], vec![0, 65536], 1, None);
    assert_eq!(text(&event, "unknownIndices"), None);
}

pub fn assert_unknown_projection(event: &safe_cex_engine::LogEvent) {
    let command = regex::Regex::new(r"\A[a-zA-Z][a-zA-Z0-9_.-]{0,63}\z").unwrap();
    let request = regex::Regex::new(r"^[0-9]{1,20}$").unwrap();
    let order = regex::Regex::new(r"\A[0-9]{1,32}\z").unwrap();
    let index = regex::Regex::new(r"\A[0-9]{1,5}\z").unwrap();
    let forbidden = regex::Regex::new(
        r"(?i)[a-z]+://|/|\?|&|=|body|payload|signature|signed|query|endpoint|api[_-]?key|SYNTHETIC",
    )
    .unwrap();
    let mut names = std::collections::HashSet::new();
    for (key, value) in &event.fields {
        assert!(names.insert(key), "duplicate field");
        assert!(
            matches!(
                *key,
                "command" | "outcome" | "placedOrderIds" | "unknownIndices" | "request" | "status"
            ),
            "extra field"
        );
        assert!(!forbidden.is_match(key));
        assert!(
            !forbidden.is_match(&format!("{value:?}")),
            "unsafe field {key}"
        );
        match *key {
            "command" => assert!(command.is_match(value.as_str().unwrap())),
            "request" => assert!(request.is_match(value.as_str().unwrap())),
            "outcome" => assert_eq!(value.as_str(), Some("unknown")),
            "placedOrderIds" | "unknownIndices" => {
                let value = value.as_str().unwrap();
                let entries: Vec<_> = if value.is_empty() {
                    vec![]
                } else {
                    value.split(',').collect()
                };
                assert!(entries.len() <= 16, "NX list cap");
                for entry in entries {
                    if *key == "placedOrderIds" {
                        assert!(order.is_match(entry));
                    } else {
                        assert!(index.is_match(entry));
                        assert!(entry.parse::<u16>().is_ok());
                    }
                }
            }
            "status" => match value {
                safe_cex_engine::log::Value::Num(status) => {
                    assert!(status.fract() == 0. && (100. ..=599.).contains(status))
                }
                _ => panic!("status must be a number"),
            },
            _ => unreachable!(),
        }
    }
    assert_eq!(
        event
            .fields
            .iter()
            .find(|(key, _)| *key == "outcome")
            .and_then(|(_, value)| value.as_str()),
        Some("unknown")
    );
}

use super::{boot, command, drain, failure, http, opts, reply, seed};
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Context, Operation, Policy, ReconciliationData, RetryPolicy,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, Method, Notice, Outcome, Response, SocketId, TimerId,
    UnknownOutcomeTelemetry,
};
struct Demo<const REGISTERED: bool>;

impl<const REGISTERED: bool> Venue for Demo<REGISTERED> {
    type Pending = Context;
    fn unknown_outcome_telemetry() -> Option<UnknownOutcomeTelemetry> {
        REGISTERED.then(Grvt::unknown_outcome_telemetry).flatten()
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
    let mut engine = safe_cex_engine::test_support::seeded_engine(Demo::<REGISTERED>);
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

fn status_reply(
    engine: &mut Engine<impl Venue>,
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
fn assert_once(actions: &[Action]) -> &LogEvent {
    assert_eq!(failure(actions).outcome, Some(Outcome::Unknown));
    let events = events(actions);
    assert_eq!(events.len(), 1);
    assert_unknown_projection(events[0]);
    events[0]
}
#[test]
fn engine_sealed_unknown_is_once_in_managed_and_frame_fed_modes() {
    for frame_fed in [false, true] {
        for streamed in [false, true] {
            for status in [408, 429, 503] {
                let (mut engine, request) = released::<true>(frame_fed);
                let actions = status_reply(&mut engine, request, status, streamed);
                let event = assert_once(&actions);
                assert_eq!(text(event, "request"), Some(request.0.to_string().as_str()));
                assert!(event.fields.contains(&("status", status.into())));
                assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
                let (mut control, control_request) = released::<false>(frame_fed);
                let plain = status_reply(&mut control, control_request, status, streamed);
                assert!(events(&plain).is_empty());
                assert_eq!(
                    serde_json::to_value(failure(&actions)).unwrap(),
                    serde_json::to_value(failure(&plain)).unwrap()
                );
                for late in [false, true] {
                    if late {
                        engine.handle(
                            8.,
                            Event::HttpFailed {
                                request,
                                error: "late",
                            },
                        );
                    } else {
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
                    }
                    assert!(engine.drain_actions().next().is_none());
                }
            }
        }
    }
}
#[test]
fn engine_transport_unknown_omits_status_and_does_not_repeat() {
    for frame_fed in [false, true] {
        let (mut engine, request) = released::<true>(frame_fed);
        engine.handle(
            6.,
            Event::HttpFailed {
                request,
                error: "https://host.test/lite/v1/create_order?signature=SYNTHETIC_BODY",
            },
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        let event = assert_once(&actions);
        assert!(!event.fields.iter().any(|(key, _)| *key == "status"));
        assert_eq!(text(event, "request"), Some(request.0.to_string().as_str()));
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        engine.handle(
            7.,
            Event::HttpFailed {
                request,
                error: "late",
            },
        );
        assert!(engine.drain_actions().next().is_none());
    }
}
#[test]
fn native_decoded_and_legacy_transport_unknown_are_once() {
    for transport in [false, true] {
        let mut engine = boot();
        let request = http(
            command(
                &mut engine,
                Command::PlaceOrders {
                    command: 72,
                    orders: vec![opts()],
                },
            ),
            "create_order",
        );
        let actions = if transport {
            engine.handle(
                super::NOW + 3.,
                Event::HttpFailed {
                    request: request.id,
                    error: "https://host.test/lite/v1/create_order?signature=SYNTHETIC_BODY",
                },
            );
            drain(&mut engine).collect()
        } else {
            reply(&mut engine, &request, 200, "{\"r\":{}}")
        };
        let event = assert_once(&actions);
        assert_eq!(text(event, "command"), Some("command.72"));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("create_order")))
        );
        let late = reply(&mut engine, &request, 200, r#"{"r":{"oi":"0x00"}}"#);
        assert!(events(&late).is_empty());
    }
}
#[test]
fn native_refusal_and_local_not_sent_emit_zero() {
    let mut engine = boot();
    let request = http(
        command(
            &mut engine,
            Command::PlaceOrders {
                command: 73,
                orders: vec![opts()],
            },
        ),
        "create_order",
    );
    let refused = reply(&mut engine, &request, 400, r#"{"c":2001,"m":"rejected"}"#);
    assert_eq!(failure(&refused).outcome, Some(Outcome::Rejected));
    assert!(events(&refused).is_empty());
    let mut invalid = opts();
    invalid.amount = 0.;
    let not_sent = command(
        &mut engine,
        Command::PlaceOrders {
            command: 74,
            orders: vec![invalid],
        },
    );
    assert_eq!(failure(&not_sent).outcome, Some(Outcome::NotSent));
    assert!(events(&not_sent).is_empty());
}

#[test]
fn native_unknown_keeps_accepted_prefix_and_stops_suffix() {
    let mut engine = boot();
    let orders = ["11", "22", "33"].map(|id| safe_cex_engine::model::PlaceOrderOpts {
        client_order_id: Some(id.into()),
        ..opts()
    });
    let first = http(
        command(
            &mut engine,
            Command::PlaceOrders {
                command: 75,
                orders: orders.to_vec(),
            },
        ),
        "create_order",
    );
    let second = http(
        reply(&mut engine, &first, 200, r#"{"r":{"oi":"0x00"}}"#),
        "create_order",
    );
    let actions = reply(&mut engine, &second, 200, r#"{"r":{}}"#);
    let event = assert_once(&actions);
    assert_eq!(text(event, "placedOrderIds"), Some("11"));
    assert_eq!(text(event, "unknownIndices"), Some(""));
    assert!(
        failure(&actions)
            .venue_message
            .as_deref()
            .is_some_and(|message| message.contains("already created GRVT order IDs: 11"))
    );
    assert!(!actions.iter().any(
        |action| matches!(action, Action::Http(request) if request.url.ends_with("create_order"))
    ));
}

#[test]
fn native_cancel_and_setting_unknown_emit_once_and_refusal_emits_zero() {
    for path in ["cancel_order", "set_initial_leverage"] {
        for status in [400, 408, 429, 503] {
            let mut engine = boot();
            seed(&mut engine);
            let mutation = if path == "cancel_order" {
                Command::CancelOrders {
                    command: 76,
                    ids: vec!["42".into()],
                }
            } else {
                Command::SetLeverage {
                    command: 76,
                    symbol: "BTC_USDT_PERP".into(),
                    leverage: 20.,
                    side: None,
                }
            };
            let request = http(command(&mut engine, mutation), path);
            let actions = reply(
                &mut engine,
                &request,
                status,
                r#"{"c":2001,"m":"https://host.test/lite/v1/endpoint?signature=SYNTHETIC_BODY"}"#,
            );
            if status == 400 {
                assert_eq!(failure(&actions).outcome, Some(Outcome::Rejected));
                assert!(events(&actions).is_empty());
            } else {
                assert_once(&actions);
            }
            assert!(!actions.iter().any(
                |action| matches!(action, Action::Http(request) if request.url.ends_with(path))
            ));
        }
    }
}

#[test]
fn engine_projection_uses_placed_journal_when_partial_is_absent() {
    let mut error = VenueError::new("https://host.test/lite/v1/endpoint?signature=SYNTHETIC_BODY");
    error.elements = Some(Box::new(ElementOutcomes(vec![ElementOutcome {
        element: Element {
            index: 0,
            client_order_id: None,
            role: None,
        },
        result: ElementResult::Placed {
            order_ids: vec!["17".into()],
        },
    }])));
    let before = serde_json::to_value(&error).unwrap();
    let event = Grvt::unknown_outcome_telemetry().unwrap()(&EngineUnknownOutcome {
        now: 0.,
        command: 1,
        request: RequestId(1),
        status: None,
        error: &error,
    });
    assert_unknown_projection(&event);
    assert_eq!(text(&event, "placedOrderIds"), Some("17"));
    assert_eq!(serde_json::to_value(&error).unwrap(), before);
}

struct PublishedProjection(Vec<String>);
impl safe_cex_engine::Venue for PublishedProjection {
    type Pending = ();
    fn start(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::SocketId) {}
    fn on_message(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, ()>,
        _: safe_cex_engine::SocketId,
        _: &str,
    ) {
    }
    fn on_close(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, ()>,
        _: safe_cex_engine::SocketId,
        _: u16,
    ) {
    }
    fn on_timer(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::TimerId) {}
    fn on_request_failed(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: (), _: &str) {}
    fn on_response(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, ()>,
        _: (),
        _: safe_cex_engine::Response<'_>,
    ) {
    }
    fn on_command(&mut self, cx: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::Command) {
        let event = project(self.0.clone(), (0..17).collect(), u64::MAX, Some(503));
        cx.event(event.level, event.code, event.message, event.fields);
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn published(ids: Vec<String>) -> LogEvent {
    let mut engine = safe_cex_engine::test_support::seeded_engine(PublishedProjection(ids));
    engine.handle(
        0.,
        safe_cex_engine::Event::Command(safe_cex_engine::Command::PlaceOrders {
            command: 1,
            orders: vec![],
        }),
    );
    let events: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            safe_cex_engine::Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == "order.outcome_unknown" =>
            {
                Some(event)
            }
            _ => None,
        })
        .collect();
    assert_eq!(events.len(), 1);
    let event = events.into_iter().next().unwrap();
    assert_unknown_projection(&event);
    event
}
#[test]
fn published_projection_retains_fifteen_full_width_ids() {
    let ids = vec!["9".repeat(32); 15];
    let event = published(ids.clone());
    assert_eq!(text(&event, "placedOrderIds"), Some(ids.join(",").as_str()));
    assert_eq!(text(&event, "placedOrderIds").unwrap().len(), 494);
}
#[test]
fn published_projection_keeps_whole_ids_within_byte_and_entry_limits() {
    for count in [16, 17] {
        let event = published(vec!["9".repeat(32); count]);
        assert_eq!(
            text(&event, "placedOrderIds"),
            Some(vec!["9".repeat(32); 15].join(",").as_str())
        );
        assert_eq!(
            text(&event, "unknownIndices").unwrap().split(',').count(),
            16
        );
    }
    let short = vec![u64::MAX.to_string(); 17];
    let event = published(short.clone());
    assert_eq!(
        text(&event, "placedOrderIds"),
        Some(short[..16].join(",").as_str())
    );
    for final_width in [17, 18] {
        let mut ids = vec!["9".repeat(32); 15];
        ids.push("8".repeat(final_width));
        let expected = if final_width == 17 {
            ids.join(",")
        } else {
            ids[..15].join(",")
        };
        let event = published(ids);
        assert_eq!(text(&event, "placedOrderIds"), Some(expected.as_str()));
        assert!(expected.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
        if final_width == 17 {
            assert_eq!(expected.len(), 512);
        }
    }
}
