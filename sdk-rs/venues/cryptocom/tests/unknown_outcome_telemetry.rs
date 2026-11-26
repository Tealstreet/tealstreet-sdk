mod support;

use safe_cex_engine::{
    Action, Command, EngineUnknownOutcome, Event, LogEvent, Notice, Outcome, RequestId, Response,
    Value, Venue, VenueError,
};
use safe_cex_venue_cryptocom::Cryptocom;
use support::Harness;

const UNKNOWN: &str = r#"{"code":50001,"message":"venue unavailable"}"#;
const ACCEPTED: &str = r#"{"code":0,"result":{"order_id":"9007199254740993"}}"#;

fn loaded() -> Harness {
    let mut h = Harness::new();
    h.load();
    h.actions.clear();
    h
}

fn place(h: &mut Harness, count: usize) -> safe_cex_engine::HttpRequest {
    let order = serde_json::from_str(
        r#"{"symbol":"BTCUSD-PERP","type":"limit","side":"buy","amount":0.001,"price":90}"#,
    )
    .unwrap();
    h.event(Event::Command(Command::PlaceOrders {
        command: 71,
        orders: vec![order; count],
    }));
    h.http("private/create-order")
}

fn advance(h: &mut Harness) {
    h.now += 50.;
    while let Some(index) = h
        .actions
        .iter()
        .position(|action| matches!(action, Action::SetTimer { at_ms, .. } if *at_ms <= h.now))
    {
        let Action::SetTimer { timer, .. } = h.actions.remove(index) else {
            unreachable!()
        };
        h.event(Event::TimerFired { timer });
    }
}

fn events(h: &Harness) -> Vec<&LogEvent> {
    h.actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                Some(event)
            }
            _ => None,
        })
        .collect()
}

fn assert_unknown(h: &Harness, request: safe_cex_engine::RequestId, status: Option<u16>) {
    assert_eq!(h.failures.len(), 1);
    assert_eq!(h.failures[0].outcome, Some(Outcome::Unknown));
    let emitted = events(h);
    assert_eq!(emitted.len(), 1, "one Unknown event");
    let fields = &emitted[0].fields;
    assert_field_rules(emitted[0]);
    assert!(
        fields.iter().any(|(key, value)| *key == "request"
            && value.as_str() == Some(request.0.to_string().as_str()))
    );
    assert!(
        fields
            .iter()
            .any(|(key, value)| *key == "outcome" && value.as_str() == Some("unknown"))
    );
    let actual = fields.iter().find(|(key, _)| *key == "status");
    match status {
        Some(status) => assert_eq!(actual.unwrap().1, u32::from(status).into()),
        None => assert!(actual.is_none()),
    }
    assert!(
        !h.actions
            .iter()
            .any(|action| matches!(action, Action::Http(request)
        if request.url.ends_with("private/create-order"))),
        "no duplicate write"
    );
    for (key, value) in fields {
        assert!(
            [
                "request",
                "command",
                "outcome",
                "placedOrderIds",
                "unknownIndices",
                "status"
            ]
            .contains(key)
        );
        let text = value.as_str().unwrap_or("");
        for forbidden in [
            "http",
            "/",
            "?",
            "=",
            "{",
            "}",
            "sig",
            "api_key",
            "replay-key",
        ] {
            assert!(!text.contains(forbidden), "unsafe field {key}");
        }
    }
}

fn assert_field_rules(event: &LogEvent) {
    assert_eq!(event.level, safe_cex_engine::Level::Warn);
    assert_eq!(event.code, "order.outcome_unknown");
    assert_eq!(event.message, safe_cex_engine::UNKNOWN_OUTCOME_TEXT);
    let text = |name| {
        event
            .fields
            .iter()
            .find(|(key, _)| *key == name)
            .unwrap()
            .1
            .as_str()
            .unwrap()
    };
    let request = text("request");
    assert!(
        (1..=20).contains(&request.len()) && request.bytes().all(|b| b.is_ascii_digit()),
        "request must match ^[0-9]{{1,20}}$"
    );
    let command = text("command");
    assert!((1..=64).contains(&command.len()) && command.as_bytes()[0].is_ascii_alphabetic());
    assert!(
        command
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    );
    assert_eq!(text("outcome"), "unknown");
    for (name, max_length, max_value) in [
        ("placedOrderIds", 32, None),
        ("unknownIndices", 5, Some(65_535)),
    ] {
        let value = text(name);
        let entries: Vec<_> = if value.is_empty() {
            vec![]
        } else {
            value.split(',').collect()
        };
        assert!(entries.len() <= 16);
        for entry in entries {
            assert!(
                (1..=max_length).contains(&entry.len())
                    && entry.bytes().all(|b| b.is_ascii_digit())
            );
            if let Some(max) = max_value {
                assert!(entry.parse::<u32>().unwrap() <= max);
            }
        }
    }
    assert_eq!(
        event.fields.len(),
        if event.fields.iter().any(|(key, _)| *key == "status") {
            6
        } else {
            5
        }
    );
    for (key, value) in &event.fields {
        assert!(
            [
                "request",
                "command",
                "outcome",
                "placedOrderIds",
                "unknownIndices",
                "status"
            ]
            .contains(key)
        );
        if *key == "status" {
            let Value::Num(status) = value else {
                panic!("numeric HTTP status")
            };
            assert!((100.0..=599.0).contains(status) && status.fract() == 0.);
        }
        let field = value.as_str().unwrap_or("").to_ascii_lowercase();
        for forbidden in [
            "http",
            "/",
            "?",
            "=",
            "{",
            "}",
            "signature",
            "sig",
            "api_key",
            "payload",
            "body",
        ] {
            assert!(!field.contains(forbidden), "unsafe emitted field {key}");
        }
    }
}

fn projected(error: &VenueError, request: u64, status: Option<u16>) -> LogEvent {
    Cryptocom::unknown_outcome_telemetry().expect("Crypto.com opts into the hook")(
        &EngineUnknownOutcome {
            now: 100.,
            command: u32::MAX,
            request: RequestId(request),
            status,
            error,
        },
    )
}

#[test]
fn projection_pins_decimal_request_bounds_and_optional_http_status_bounds() {
    let error = VenueError::new("https://venue.test/private?api_key=fixture&sig=signed-body")
        .outcome(Outcome::Unknown);
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
            let event = projected(&error, request, status);
            assert_field_rules(&event);
            assert_eq!(
                event.fields.iter().any(|(key, _)| *key == "status"),
                status.is_some_and(|s| (100..=599).contains(&s))
            );
        }
    }
}

#[test]
fn projection_caps_lists_at_sixteen_and_drops_non_native_ids_and_indices() {
    use safe_cex_engine::write_plan::{Element, ElementResult, WritePlan};
    let error = VenueError::new("signed payload must not be projected").outcome(Outcome::Unknown);
    let mut plan = WritePlan::new(
        (0..20)
            .chain([65_535, 65_536])
            .map(|index| Element {
                index,
                client_order_id: None,
                role: None,
            })
            .collect(),
    );
    for index in (0..20).chain([65_535, 65_536]) {
        plan.record(
            index,
            ElementResult::Unknown {
                error: Box::new(error.clone()),
            },
        )
        .unwrap();
    }
    let mut ids = vec![
        "https://venue.test/?sig=fixture".into(),
        "{body}".into(),
        "9".repeat(33),
        "".into(),
    ];
    ids.extend((0..20).map(|i| i.to_string()));
    let event = projected(&plan.attach(error.partial_ids(&ids)), u64::MAX, Some(200));
    assert_field_rules(&event);
    for field in ["placedOrderIds", "unknownIndices"] {
        let value = event
            .fields
            .iter()
            .find(|(key, _)| *key == field)
            .unwrap()
            .1
            .as_str()
            .unwrap();
        assert_eq!(
            value,
            (0..16).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
        );
    }
    let error = VenueError::new("fixture").partial_ids(&vec!["9".repeat(32); 16]);
    let event = projected(&error, 1, None);
    assert_field_rules(&event);
    let list = event
        .fields
        .iter()
        .find(|(key, _)| *key == "placedOrderIds")
        .unwrap()
        .1
        .as_str()
        .unwrap();
    assert_eq!(
        list.split(',').count(),
        15,
        "whole 32-digit IDs remain below the engine's 512-byte cap"
    );
}

#[test]
fn projection_pins_unknown_index_bounds_without_elements() {
    for (index, expected) in [(65_535, "65535"), (65_536, "")] {
        let mut error = VenueError::new("fixture").partial_ids(&["1".into()]);
        error.partial.as_mut().unwrap().failed_index = Some(index);
        let event = projected(&error, 1, None);
        assert_field_rules(&event);
        assert!(
            event
                .fields
                .iter()
                .any(|(key, value)| *key == "unknownIndices" && value.as_str() == Some(expected))
        );
    }
}

// Master stops a batch at its first Unknown (SPLIT_PLAN_INVARIANT exception 1),
// so the engine seal reports once without dispatching the sibling.
#[test]
fn engine_sealed_unknown_stops_the_batch_and_emits_once_after_a_late_reply() {
    let mut h = loaded();
    let first = place(&mut h, 2);
    h.actions.clear();
    h.event(Event::HttpResponse {
        request: first.id,
        response: Response {
            status: 503,
            headers: "",
            body: UNKNOWN,
        },
    });
    advance(&mut h);
    assert_unknown(&h, first.id, Some(503));
    h.respond(&first, ACCEPTED);
    advance(&mut h);
    assert_unknown(&h, first.id, Some(503));
}

#[test]
fn released_transport_unknown_omits_status_and_emits_once() {
    let mut h = loaded();
    let request = place(&mut h, 1);
    h.event(Event::HttpFailed {
        request: request.id,
        error: "lost reply",
    });
    advance(&mut h);
    assert_unknown(&h, request.id, None);
    h.respond(&request, ACCEPTED);
    advance(&mut h);
    assert_unknown(&h, request.id, None);
}

#[test]
fn native_unknown_emits_once_without_an_engine_duplicate() {
    let mut h = loaded();
    let request = place(&mut h, 1);
    h.respond(&request, UNKNOWN);
    advance(&mut h);
    assert_unknown(&h, request.id, Some(200));
    h.respond(&request, ACCEPTED);
    advance(&mut h);
    assert_unknown(&h, request.id, Some(200));
}

// Native Unknown halts the batch, so a later engine seal cannot follow it.
#[test]
fn native_unknown_stops_the_batch_with_one_indexed_event() {
    let mut h = loaded();
    let first = place(&mut h, 2);
    h.respond(&first, UNKNOWN);
    advance(&mut h);
    assert_unknown(&h, first.id, Some(200));
    assert!(
        events(&h)[0]
            .fields
            .iter()
            .any(|(key, value)| *key == "unknownIndices" && value.as_str() == Some("0"))
    );
}

#[test]
fn definite_refusal_emits_no_unknown_event() {
    let mut h = loaded();
    let request = place(&mut h, 1);
    h.respond(&request, r#"{"code":306,"message":"invalid order"}"#);
    advance(&mut h);
    assert_eq!(h.failures.len(), 1);
    assert_eq!(h.failures[0].outcome, Some(Outcome::Rejected));
    assert!(events(&h).is_empty());
}

#[test]
fn frame_fed_and_streamed_engine_unknowns_emit_once() {
    for frame_fed in [false, true] {
        for streamed in [false, true] {
            for status in [408, 429, 503] {
                let mut h = loaded();
                if frame_fed {
                    let markets = h.engine.store().markets().to_vec();
                    let mut engine = support::fixture::engine(support::fixture::Options {
                        key: Some("replay-key".into()),
                        secret: Some("replay-secret".into()),
                        ..Default::default()
                    });
                    engine.set_frame_fed().unwrap();
                    engine.store_mut().set_markets(markets);
                    h.engine = engine;
                    h.engine.start(h.now);
                    h.collect();
                    h.actions.clear();
                }
                let request = place(&mut h, 1);
                if streamed {
                    h.event(Event::HttpStart {
                        request: request.id,
                        status,
                        headers: "",
                    });
                    h.event(Event::HttpChunk {
                        request: request.id,
                        data: UNKNOWN.as_bytes(),
                    });
                    h.event(Event::HttpEnd {
                        request: request.id,
                    });
                } else {
                    h.event(Event::HttpResponse {
                        request: request.id,
                        response: Response {
                            status,
                            headers: "",
                            body: UNKNOWN,
                        },
                    });
                }
                advance(&mut h);
                assert_unknown(&h, request.id, Some(status));
                h.respond(&request, ACCEPTED);
                advance(&mut h);
                assert_unknown(&h, request.id, Some(status));
            }
        }
    }
}
