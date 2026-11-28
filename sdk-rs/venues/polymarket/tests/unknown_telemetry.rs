use regex::Regex;
use safe_cex_engine::error::Partial;
use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
use safe_cex_engine::{
    EngineUnknownOutcome, Level, LogEvent, Outcome, RequestId, Value, Venue, VenueError,
};
use safe_cex_venue_polymarket::Polymarket;

const HOSTILE: &str = "https://venue.test/api/order?api_key=SYNTHETIC_SECRET&signature=SYNTHETIC_SIGNATURE body=SIGNED_PAYLOAD";

// Production Polymarket trades use legacy requests. This probe tests engine
// sealing with the production venue's exact static hook, without migrating I/O.
struct ManagedHookProbe;

use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Context, Operation, Policy, ReconciliationData, RetryPolicy,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, Method, Notice, Response, SocketId, TimerId,
    UnknownOutcomeTelemetry,
};

impl Venue for ManagedHookProbe {
    type Pending = Context;
    fn unknown_outcome_telemetry() -> Option<UnknownOutcomeTelemetry> {
        Polymarket::unknown_outcome_telemetry()
    }
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {
        panic!("managed transport must use typed callback");
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, error: VenueError) {
        cx.fail_context(&context, error).unwrap();
    }
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {
        panic!("engine-sealed uncertainty must bypass decoder");
    }
    fn capture_http_reconciliation(
        &self,
        _: &Context,
    ) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
        ReconciliationData::new("polymarket-hook-test".into())
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
            HOSTILE.into(),
            Some("SYNTHETIC_SIGNED_BODY".into()),
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

fn released_probe(frame_fed: bool) -> (Engine<ManagedHookProbe>, RequestId) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(ManagedHookProbe);
    if frame_fed {
        engine.set_frame_fed().unwrap();
    }
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
    engine.store_mut().set_markets(vec![Market {
        id: "1".into(),
        symbol: "election-Yes".into(),
        pretty_symbol: None,
        base: "YES".into(),
        quote: "USD".into(),
        active: true,
        precision: Precision {
            amount: 0.01,
            price: 0.01,
        },
        limits: Limits {
            amount: MinMax {
                min: 1.,
                max: 1000.,
            },
            leverage: MinMax { min: 1., max: 1. },
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
    let requests: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut engine, 2.)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    (engine, requests[0])
}

#[test]
fn engine_sealed_unknown_hook_once_for_http_and_transport_without_decoder() {
    for frame_fed in [false, true] {
        for streamed in [false, true] {
            for status in [Some(408), Some(429), Some(503), None] {
                let (mut engine, request) = released_probe(frame_fed);
                match status {
                    Some(status) if streamed => {
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
                                data: HOSTILE.as_bytes(),
                            },
                        );
                        engine.handle(5., Event::HttpEnd { request });
                    }
                    Some(status) => engine.handle(
                        5.,
                        Event::HttpResponse {
                            request,
                            response: Response {
                                status,
                                headers: "",
                                body: HOSTILE,
                            },
                        },
                    ),
                    None => engine.handle(
                        5.,
                        Event::HttpFailed {
                            request,
                            error: HOSTILE,
                        },
                    ),
                }
                let actions: Vec<_> = engine.drain_actions().collect();
                assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(Outcome::Unknown))).count(), 1);
                let events: Vec<_> = actions
                    .iter()
                    .filter_map(|a| match a {
                        Action::Notice(Notice::Event(event))
                            if event.code == "order.outcome_unknown" =>
                        {
                            Some(event)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    events.len(),
                    1,
                    "frame_fed={frame_fed}, streamed={streamed}, status={status:?}"
                );
                assert_nx_fields(events[0], status.is_some());
                assert_eq!(
                    field(events[0], "request").as_str(),
                    Some(request.0.to_string().as_str())
                );
                assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
                engine.handle(
                    6.,
                    Event::HttpFailed {
                        request,
                        error: HOSTILE,
                    },
                );
                engine.handle(
                    7.,
                    Event::HttpResponse {
                        request,
                        response: Response {
                            status: 200,
                            headers: "",
                            body: "{}",
                        },
                    },
                );
                assert!(!engine.drain_actions().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown")));
            }
        }
    }
}

fn project(request: u64, status: Option<u16>, error: &VenueError) -> LogEvent {
    Polymarket::unknown_outcome_telemetry()
        .expect("Polymarket opts into engine-sealed Unknown telemetry")(&EngineUnknownOutcome {
        now: 123.,
        command: u32::MAX,
        request: RequestId(request),
        status,
        error,
    })
}

fn field<'a>(event: &'a LogEvent, name: &str) -> &'a Value {
    &event.fields.iter().find(|(key, _)| *key == name).unwrap().1
}

fn assert_nx_fields(event: &LogEvent, has_status: bool) {
    assert_eq!(event.level, Level::Warn);
    assert_eq!(event.code, "order.outcome_unknown");
    let mut names: Vec<_> = event.fields.iter().map(|(key, _)| *key).collect();
    names.sort_unstable();
    let mut expected = vec![
        "command",
        "outcome",
        "placedOrderIds",
        "request",
        "unknownIndices",
    ];
    if has_status {
        expected.push("status");
    }
    expected.sort_unstable();
    assert_eq!(names, expected, "closed NX FIELD_RULES whitelist");
    assert!(
        Regex::new(r"^[0-9]{1,20}$")
            .unwrap()
            .is_match(field(event, "request").as_str().unwrap())
    );
    assert!(
        Regex::new(r"^[a-zA-Z][a-zA-Z0-9_.-]{0,63}$")
            .unwrap()
            .is_match(field(event, "command").as_str().unwrap())
    );
    assert_eq!(field(event, "outcome").as_str(), Some("unknown"));
    for (name, pattern, max) in [
        ("placedOrderIds", r"^[0-9]{1,32}$", None),
        ("unknownIndices", r"^[0-9]{1,5}$", Some(65535u32)),
    ] {
        let text = field(event, name).as_str().unwrap();
        let entries: Vec<_> = if text.is_empty() {
            vec![]
        } else {
            text.split(',').collect()
        };
        assert!(entries.len() <= 16);
        let regex = Regex::new(pattern).unwrap();
        for entry in entries {
            assert!(regex.is_match(entry));
            if let Some(max) = max {
                assert!(entry.parse::<u32>().unwrap() <= max);
            }
        }
    }
    if has_status {
        let Value::Num(status) = field(event, "status") else {
            panic!("numeric HTTP status")
        };
        assert!(status.fract() == 0. && (100. ..=599.).contains(status));
    }
    for (key, value) in &event.fields {
        let text = format!("{key}={value:?}").to_ascii_lowercase();
        for forbidden in [
            "http",
            "://",
            "/api",
            "?",
            "&",
            "body",
            "payload",
            "signature",
            "api_key",
            "synthetic",
            "endpoint",
            "url",
        ] {
            assert!(!text.contains(forbidden), "unsafe projected field {key}");
        }
    }
    assert!(!event.message.contains(HOSTILE));
}

#[test]
fn projection_obeys_exact_nx_rules_and_never_copies_untrusted_text() {
    let mut error = VenueError::new(HOSTILE)
        .outcome(Outcome::Unknown)
        .venue_code(HOSTILE)
        .venue_message(HOSTILE);
    error.partial = Some(Box::new(Partial {
        order_ids: vec!["1".into(), "9".repeat(32), HOSTILE.into(), "9".repeat(33)],
        total: None,
        failed_index: None,
    }));
    error.elements = Some(Box::new(ElementOutcomes(
        vec![0, 65535, 65536]
            .into_iter()
            .map(|index| ElementOutcome {
                element: Element {
                    index,
                    client_order_id: Some(HOSTILE.into()),
                    role: None,
                },
                result: ElementResult::Unknown {
                    error: Box::new(VenueError::new(HOSTILE)),
                },
            })
            .collect(),
    )));
    for request in [0, 1, u64::MAX] {
        for status in [
            None,
            Some(0),
            Some(99),
            Some(100),
            Some(599),
            Some(600),
            Some(u16::MAX),
        ] {
            let event = project(request, status, &error);
            assert_nx_fields(&event, status.is_some_and(|s| (100..=599).contains(&s)));
            assert_eq!(
                field(&event, "placedOrderIds").as_str(),
                Some(format!("1,{}", "9".repeat(32)).as_str())
            );
            assert_eq!(field(&event, "unknownIndices").as_str(), Some("0,65535"));
        }
    }
}

#[test]
fn projection_caps_lists_and_retains_empty_lists() {
    let mut error = VenueError::new(HOSTILE).outcome(Outcome::Unknown);
    let empty = project(1, None, &error);
    assert_nx_fields(&empty, false);
    assert_eq!(field(&empty, "placedOrderIds").as_str(), Some(""));
    assert_eq!(field(&empty, "unknownIndices").as_str(), Some(""));
    error.partial = Some(Box::new(Partial {
        order_ids: (1..=20).map(|n| n.to_string()).collect(),
        total: None,
        failed_index: None,
    }));
    error.elements = Some(Box::new(ElementOutcomes(
        (0..20)
            .map(|index| ElementOutcome {
                element: Element {
                    index,
                    client_order_id: None,
                    role: None,
                },
                result: ElementResult::Unknown {
                    error: Box::new(VenueError::new(HOSTILE)),
                },
            })
            .collect(),
    )));
    let event = project(u64::MAX, Some(503), &error);
    assert_nx_fields(&event, true);
    assert_eq!(
        field(&event, "placedOrderIds").as_str().unwrap(),
        (1..=16)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    assert_eq!(
        field(&event, "unknownIndices").as_str().unwrap(),
        (0..16).map(|n| n.to_string()).collect::<Vec<_>>().join(",")
    );
}

#[test]
fn projection_stays_inside_engine_string_cap_without_truncation_markers() {
    let mut error = VenueError::new(HOSTILE).outcome(Outcome::Unknown);
    error.partial = Some(Box::new(Partial {
        order_ids: vec!["9".repeat(32); 16],
        total: None,
        failed_index: None,
    }));
    let event = project(1, Some(503), &error);
    assert_nx_fields(&event, true);
    let ids = field(&event, "placedOrderIds").as_str().unwrap();
    assert!(ids.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
    assert_eq!(ids.split(',').count(), 15);
}
