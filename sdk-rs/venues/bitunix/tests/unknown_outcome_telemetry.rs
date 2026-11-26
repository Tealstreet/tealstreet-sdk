mod support;

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{
    Action, Command, Event, Level, LogEvent, Method, Notice, Outcome, Response, Value,
};
use support::*;

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

fn assert_fields(event: &LogEvent, request: safe_cex_engine::RequestId, status: Option<u16>) {
    assert_eq!(event.level, Level::Warn);
    let expected = [
        ("command", Value::from("bitunix.9001")),
        ("outcome", Value::from("unknown")),
        ("request", Value::from(request.0.to_string())),
        ("placedOrderIds", Value::from("")),
        ("unknownIndices", Value::from("")),
    ];
    assert_eq!(
        event.fields.len(),
        expected.len() + usize::from(status.is_some())
    );
    for field in expected {
        assert!(event.fields.contains(&field), "{event:?}");
    }
    if let Some(status) = status {
        assert!(event.fields.contains(&("status", Value::from(status))));
    }
    let value = event
        .fields
        .iter()
        .find(|(key, _)| *key == "request")
        .unwrap()
        .1
        .as_str()
        .unwrap();
    assert!(
        (1..=20).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_digit()),
        "request must match ^[0-9]{{1,20}}$"
    );
    for (_, value) in &event.fields {
        if let Some(value) = value.as_str() {
            let lower = value.to_ascii_lowercase();
            for forbidden in [
                "://",
                "/",
                "?",
                "&",
                "=",
                "{",
                "}",
                "signature",
                "apikey",
                "api-key",
                "secret",
                "payload",
            ] {
                assert!(!lower.contains(forbidden), "unsafe field: {value}");
            }
        }
    }
}

fn ready(frame_fed: bool) -> TestEngine {
    if !frame_fed {
        return startup().0;
    }
    let mut e = engine(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Options::default()
    });
    e.set_frame_fed().unwrap();
    let mut now = -1000.;
    let mut repairs = Vec::new();
    e.start(now);
    loop {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        if actions.is_empty() {
            if let Some((timer, at)) = repairs.pop() {
                now = at;
                e.handle(at, Event::TimerFired { timer });
                continue;
            }
            break;
        }
        for action in actions {
            match action {
                Action::Http(request) => {
                    let body = if request.url.ends_with("/trading_pairs") {
                        PAIRS
                    } else if request.url.ends_with("/tickers") {
                        TICKERS
                    } else if request.url.contains("get_pending_positions") {
                        POSITIONS
                    } else if request.url.contains("/tpsl/") {
                        TPSL
                    } else if request.url.contains("get_pending_orders") {
                        ORDERS
                    } else {
                        ACCOUNT
                    };
                    e.handle(
                        now,
                        Event::HttpResponse {
                            request: request.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                Action::SetTimer { timer, at_ms } if at_ms == 0. => repairs.push((timer, at_ms)),
                Action::ClearTimer { timer } => repairs.retain(|(pending, _)| *pending != timer),
                Action::WsConnect { .. } => panic!("frame-fed startup must not open a socket"),
                _ => {}
            }
        }
    }
    assert!(e.store().loaded().orders);
    e
}

fn run(frame_fed: bool, status: Option<u16>, native_code: i64, split: bool) {
    let mut e = ready(frame_fed);
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500,"amount":0.002}"#,
    )
    .unwrap();
    e.handle(
        101.,
        Event::Command(Command::PlaceOrders {
            command: 9001,
            orders: vec![order],
        }),
    );
    let requests = requests(&mut e);
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, Method::Post);
    assert!(request.url.ends_with("/api/v1/futures/trade/place_order"));
    let body: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["symbol"], "BTCUSDT");
    assert_eq!(body["qty"], "0.002");
    let reply = if native_code == 30042 {
        r#"{"code":30042,"msg":"Client ID duplicate","data":null}"#.into()
    } else {
        format!(r#"{{"code":{native_code},"msg":"native result"}}"#)
    };
    if let Some(status) = status {
        if split {
            e.handle(
                102.,
                Event::HttpStart {
                    request: request.id,
                    status,
                    headers: "",
                },
            );
            e.handle(
                103.,
                Event::HttpChunk {
                    request: request.id,
                    data: reply.as_bytes(),
                },
            );
            e.handle(
                104.,
                Event::HttpEnd {
                    request: request.id,
                },
            );
        } else {
            e.handle(
                104.,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status,
                        headers: "",
                        body: &reply,
                    },
                },
            );
        }
    } else {
        e.handle(
            104.,
            Event::HttpFailed {
                request: request.id,
                error: "response lost",
            },
        );
    }
    let actions = safe_cex_engine::test_support::prepared_now(&mut e);
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 9001,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1, "{actions:?}");
    let unknown = status != Some(200) || matches!(native_code, 10001 | 30042);
    assert_eq!(
        failures[0].outcome,
        Some(if unknown {
            Outcome::Unknown
        } else {
            Outcome::Rejected
        })
    );
    assert!(
        !actions.iter().any(
            |action| matches!(action, Action::Http(request) if request.method == Method::Post)
        )
    );
    let events = events(&actions);
    assert_eq!(events.len(), usize::from(unknown), "{actions:?}");
    if unknown {
        assert_fields(events[0], request.id, status);
    }
    for late in [
        Event::HttpFailed {
            request: request.id,
            error: "late duplicate",
        },
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 503,
                headers: "",
                body: "late",
            },
        },
    ] {
        e.handle(105., late);
        let actions = safe_cex_engine::test_support::prepared_now(&mut e);
        assert!(self::events(&actions).is_empty());
        assert!(!actions.iter().any(|action| matches!(
            action,
            Action::Notice(
                Notice::CommandFailed { command: 9001, .. }
                    | Notice::CommandDone { command: 9001, .. }
            )
        )));
        assert!(!actions.iter().any(
            |action| matches!(action, Action::Http(request) if request.method == Method::Post)
        ));
    }
}

#[test]
fn engine_sealed_unknown_emits_once_in_ordinary_and_frame_fed_modes() {
    for frame_fed in [false, true] {
        for status in [Some(408), Some(429), Some(503), None] {
            for split in [false, true] {
                run(frame_fed, status, 10001, split);
            }
        }
    }
}

#[test]
fn venue_decoded_unknown_emits_once_without_a_duplicate_hook_event() {
    for frame_fed in [false, true] {
        for split in [false, true] {
            run(frame_fed, Some(200), 10001, split);
        }
    }
}

#[test]
fn classified_duplicate_client_id_unknown_emits_once_in_both_feed_modes() {
    for frame_fed in [false, true] {
        for split in [false, true] {
            run(frame_fed, Some(200), 30042, split);
        }
    }
}

#[test]
fn definite_refusal_emits_no_unknown_event() {
    for frame_fed in [false, true] {
        run(frame_fed, Some(200), 20003, false);
    }
}

fn projected(error: &safe_cex_engine::VenueError, status: Option<u16>) -> LogEvent {
    let hook = <safe_cex_venue_bitunix::BitunixFutures as safe_cex_engine::Venue>::unknown_outcome_telemetry().expect("Bitunix opts into sealed-Unknown telemetry");
    hook(&safe_cex_engine::EngineUnknownOutcome {
        now: 123.,
        command: u32::MAX,
        request: safe_cex_engine::RequestId(u64::MAX),
        status,
        error,
    })
}

fn field<'a>(event: &'a LogEvent, key: &str) -> &'a Value {
    &event
        .fields
        .iter()
        .find(|(name, _)| *name == key)
        .unwrap()
        .1
}

#[test]
fn projection_matches_nx_closed_fields_and_rejects_signed_content() {
    use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
    let attack = "https://venue.invalid/orders?apiKey=synthetic&signature=synthetic&body={payload}";
    let mut ids = vec![
        attack.to_owned(),
        "9".repeat(32),
        "8".repeat(33),
        String::new(),
        "-1".into(),
    ];
    ids.extend((1..=20).map(|index| index.to_string()));
    let mut error = safe_cex_engine::VenueError::new(attack)
        .venue_code(attack)
        .venue_message(attack)
        .outcome(Outcome::Unknown)
        .partial_ids(&ids);
    let indices = [65_536, 65_535].into_iter().chain(0..20);
    error.elements = Some(Box::new(ElementOutcomes(
        indices
            .map(|index| ElementOutcome {
                element: Element {
                    index,
                    client_order_id: Some(attack.into()),
                    role: None,
                },
                result: ElementResult::Unknown {
                    error: Box::new(
                        safe_cex_engine::VenueError::new(attack).outcome(Outcome::Unknown),
                    ),
                },
            })
            .collect(),
    )));
    let event = projected(&error, Some(599));
    assert_eq!(event.fields.len(), 6);
    assert_eq!(
        field(&event, "command").as_str(),
        Some("bitunix.4294967295")
    );
    assert_eq!(
        field(&event, "request").as_str(),
        Some("18446744073709551615")
    );
    let command = field(&event, "command").as_str().unwrap();
    assert!(
        (1..=64).contains(&command.len())
            && command.as_bytes()[0].is_ascii_alphabetic()
            && command
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    );
    assert_eq!(field(&event, "outcome").as_str(), Some("unknown"));
    assert_eq!(field(&event, "status"), &Value::Num(599.));
    let placed: Vec<_> = field(&event, "placedOrderIds")
        .as_str()
        .unwrap()
        .split(',')
        .collect();
    assert_eq!(placed.len(), 16);
    assert_eq!(placed[0], "9".repeat(32));
    assert_eq!(placed[15], "15");
    for id in placed {
        assert!((1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()));
    }
    let indices: Vec<_> = field(&event, "unknownIndices")
        .as_str()
        .unwrap()
        .split(',')
        .collect();
    assert_eq!(indices.len(), 16);
    assert_eq!(indices[0], "65535");
    assert_eq!(indices[15], "14");
    for index in indices {
        assert!(index.len() <= 5 && index.parse::<u16>().is_ok());
    }
    for (key, value) in &event.fields {
        assert!(
            [
                "command",
                "request",
                "outcome",
                "placedOrderIds",
                "unknownIndices",
                "status"
            ]
            .contains(key)
        );
        if let Some(text) = value.as_str() {
            assert!(text.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
            let lower = text.to_ascii_lowercase();
            for forbidden in [
                "://",
                "/",
                "?",
                "&",
                "=",
                "{",
                "}",
                "signature",
                "apikey",
                "api-key",
                "payload",
                "body",
            ] {
                assert!(
                    !lower.contains(forbidden),
                    "unsafe projected field {key}: {text}"
                );
            }
        }
    }
    assert_eq!(event.message, safe_cex_engine::UNKNOWN_OUTCOME_TEXT);
    assert_eq!(error.message, attack, "projection is read-only");
}

#[test]
fn projection_omits_absent_or_out_of_range_status() {
    let error = safe_cex_engine::VenueError::new("unknown").outcome(Outcome::Unknown);
    for status in [
        None,
        Some(0),
        Some(99),
        Some(100),
        Some(599),
        Some(600),
        Some(u16::MAX),
    ] {
        let event = projected(&error, status);
        let expected = status.filter(|value| (100..=599).contains(value));
        assert_eq!(
            event
                .fields
                .iter()
                .find(|(key, _)| *key == "status")
                .map(|(_, value)| value),
            expected.map(|value| Value::Num(f64::from(value))).as_ref()
        );
        assert_eq!(event.fields.len(), 5 + usize::from(expected.is_some()));
    }
}

#[test]
fn maximum_width_order_ids_are_never_truncated_by_the_log_cap() {
    let ids: Vec<_> = (0..20).map(|index| format!("{index:032}")).collect();
    let error = safe_cex_engine::VenueError::new("unknown")
        .outcome(Outcome::Unknown)
        .partial_ids(&ids);
    let event = projected(&error, None);
    let value = field(&event, "placedOrderIds").as_str().unwrap();
    assert!(value.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
    let rows: Vec<_> = value.split(',').collect();
    assert_eq!(rows.len(), 15);
    assert!(
        rows.iter()
            .all(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit()))
    );
}

#[test]
fn zero_identifiers_keep_the_command_label_and_decimal_request_shape() {
    let error = safe_cex_engine::VenueError::new("unknown").outcome(Outcome::Unknown);
    let hook = <safe_cex_venue_bitunix::BitunixFutures as safe_cex_engine::Venue>::unknown_outcome_telemetry().unwrap();
    let event = hook(&safe_cex_engine::EngineUnknownOutcome {
        now: 0.,
        command: 0,
        request: safe_cex_engine::RequestId(0),
        status: None,
        error: &error,
    });
    assert_eq!(field(&event, "command").as_str(), Some("bitunix.0"));
    assert_eq!(field(&event, "request").as_str(), Some("0"));
    assert!(!event.fields.iter().any(|(key, _)| *key == "status"));
}
