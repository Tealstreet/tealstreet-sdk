mod support;
use safe_cex_engine::log::{LogEvent, Value};
use safe_cex_engine::{
    Command, EngineUnknownOutcome, Event, Level, Outcome, RequestId, Venue, VenueError,
};
use safe_cex_venue_bitget::BitgetFutures;
use support::*;

const HOSTILE: &str = "https://telemetry.invalid/api/orders?apiKey=key&signature=signed body={\"payload\":\"private\"}";

fn harness(frame_fed: bool) -> Harness {
    if !frame_fed {
        return Harness::classic();
    }
    let mut source = Harness::classic();
    let mut h = Harness::started_configure(
        FixtureOptions {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..FixtureOptions::default()
        },
        false,
        |engine| engine.set_frame_fed().unwrap(),
    );
    h.respond("/contracts", 200, CONTRACTS);
    h.engine
        .apply_frame(h.now, source.engine.snapshot_frame(source.now), true)
        .unwrap();
    h.collect();
    h
}

fn place(h: &mut Harness) -> safe_cex_engine::HttpRequest {
    h.command(Command::PlaceOrders {
        command: 71,
        orders: vec![serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"limit","side":"sell","amount":0.6,"price":110,"clientOrderId":"private-payload-marker"}"#).unwrap()],
    });
    h.take("/batch-place-order")
}

fn unknowns(h: &Harness) -> Vec<&LogEvent> {
    h.events
        .iter()
        .filter(|event| event.code == "order.outcome_unknown")
        .collect()
}

fn safe_projection(event: &LogEvent, request: RequestId) {
    let value = event
        .fields
        .iter()
        .find(|(key, _)| *key == "request")
        .unwrap()
        .1
        .as_str()
        .unwrap();
    // Equivalent to ^[0-9]{1,20}$, including the u64 boundary.
    assert!((1..=20).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_digit()));
    assert_eq!(value, request.0.to_string());
    safe_fields(event);
    assert_eq!(event.level, Level::Warn);
    assert_eq!(event.message, safe_cex_engine::UNKNOWN_OUTCOME_TEXT);
    assert!(
        event
            .fields
            .contains(&("command", Value::from("command-71")))
    );
    assert!(event.fields.contains(&("outcome", Value::from("unknown"))));
    assert!(event.fields.iter().all(|(key, _)| {
        [
            "command",
            "outcome",
            "placedOrderIds",
            "unknownIndices",
            "request",
            "status",
        ]
        .contains(key)
    }));
}

fn safe_fields(event: &LogEvent) {
    for (key, value) in &event.fields {
        let projected = format!("{key} {value:?}").to_ascii_lowercase();
        for forbidden in [
            "http",
            "://",
            "/",
            "endpoint",
            "url",
            "?",
            "&",
            "=",
            "body",
            "payload",
            "signature",
            "signed",
            "query",
            "apikey",
            "private-payload-marker",
            "{",
            "}",
        ] {
            assert!(
                !projected.contains(forbidden),
                "unsafe field {key}: {value:?}"
            );
        }
    }
}

#[test]
fn engine_unproven_http_is_one_safe_event_and_never_resends() {
    for frame_fed in [false, true] {
        let mut h = harness(frame_fed);
        let request = place(&mut h);
        h.reply(request.id, 503, HOSTILE);
        assert_eq!(unknowns(&h).len(), 1);
        safe_projection(unknowns(&h)[0], request.id);
        assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::Unknown));
        assert!(
            h.requests
                .iter()
                .all(|r| r.method == safe_cex_engine::Method::Get)
        );
        h.reply(request.id, 503, HOSTILE);
        h.event(Event::HttpFailed {
            request: request.id,
            error: HOSTILE,
        });
        assert_eq!(unknowns(&h).len(), 1);
    }
}

#[test]
fn engine_transport_unknown_has_no_duplicate_native_event() {
    for frame_fed in [false, true] {
        let mut h = harness(frame_fed);
        let request = place(&mut h);
        h.event(Event::HttpFailed {
            request: request.id,
            error: HOSTILE,
        });
        assert_eq!(unknowns(&h).len(), 1);
        safe_projection(unknowns(&h)[0], request.id);
        h.event(Event::HttpFailed {
            request: request.id,
            error: HOSTILE,
        });
        assert_eq!(unknowns(&h).len(), 1);
    }
}

#[test]
fn native_unknown_remains_one_event_and_definite_refusal_has_none() {
    for frame_fed in [false, true] {
        for (code, count) in [("40010", 1), ("40757", 0)] {
            let mut h = harness(frame_fed);
            let request = place(&mut h);
            let body = format!(r#"{{"code":"{code}","msg":"refused","data":null}}"#);
            h.reply(request.id, 200, &body);
            assert_eq!(unknowns(&h).len(), count);
            for event in unknowns(&h) {
                safe_projection(event, request.id);
            }
            h.reply(request.id, 200, &body);
            assert_eq!(unknowns(&h).len(), count);
        }
    }
}

#[test]
fn delivered_malformed_body_has_one_safe_unknown_event() {
    for frame_fed in [false, true] {
        let mut h = harness(frame_fed);
        let request = place(&mut h);
        h.event(Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        });
        h.event(Event::HttpChunk {
            request: request.id,
            data: &[0xff],
        });
        h.event(Event::HttpEnd {
            request: request.id,
        });
        assert_eq!(unknowns(&h).len(), 1);
        safe_fields(unknowns(&h)[0]);
    }
}

#[test]
fn projection_rejects_hostile_error_content_and_keeps_max_request_id_decimal() {
    let error = VenueError::new(HOSTILE).outcome(Outcome::Unknown);
    let hook = BitgetFutures::unknown_outcome_telemetry().expect("Bitget opts in");
    for id in [0, 1, u64::MAX] {
        let facts = EngineUnknownOutcome {
            now: 1.0,
            command: 71,
            request: RequestId(id),
            status: Some(503),
            error: &error,
        };
        safe_projection(&hook(&facts), facts.request);
    }
}

fn projected(error: &VenueError, status: Option<u16>) -> LogEvent {
    BitgetFutures::unknown_outcome_telemetry().unwrap()(&EngineUnknownOutcome {
        now: 0.0,
        command: 71,
        request: RequestId(u64::MAX),
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
fn nx_schema_pins_status_bounds_list_caps_and_no_extra_fields() {
    use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
    let mut error = VenueError::new(HOSTILE).outcome(Outcome::Unknown);
    let mut ids = vec![HOSTILE.to_owned(), "9".repeat(33), String::new()];
    ids.extend((1..=17).map(|id| id.to_string()));
    error = error.partial_ids(&ids);
    let indices = [65536, usize::MAX, 65535].into_iter().chain(0..17);
    error.elements = Some(Box::new(ElementOutcomes(
        indices
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
        safe_projection(&event, RequestId(u64::MAX));
        assert_eq!(field(&event, "outcome"), &Value::from("unknown"));
        let command = field(&event, "command").as_str().unwrap();
        assert!(command.len() <= 64 && command.starts_with(|c: char| c.is_ascii_alphabetic()));
        assert!(
            command
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        );
        let placed = field(&event, "placedOrderIds").as_str().unwrap();
        assert_eq!(
            placed,
            (1..=16)
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        assert_eq!(placed.split(',').count(), 16);
        let unknown = field(&event, "unknownIndices").as_str().unwrap();
        assert_eq!(
            unknown,
            std::iter::once("65535".to_owned())
                .chain((0..15).map(|i| i.to_string()))
                .collect::<Vec<_>>()
                .join(",")
        );
        assert_eq!(unknown.split(',').count(), 16);
        let expected = status.filter(|s| (100..=599).contains(s));
        assert_eq!(event.fields.len(), if expected.is_some() { 6 } else { 5 });
        assert_eq!(
            event
                .fields
                .iter()
                .find(|(key, _)| *key == "status")
                .map(|(_, value)| value.clone()),
            expected.map(Value::from)
        );
    }
    let event = projected(&VenueError::new(HOSTILE), None);
    assert_eq!(field(&event, "placedOrderIds"), &Value::from(""));
    assert_eq!(field(&event, "unknownIndices"), &Value::from(""));
    let boundary_id = "9".repeat(32);
    let error = VenueError::new(HOSTILE).partial_ids(std::slice::from_ref(&boundary_id));
    assert_eq!(
        field(&projected(&error, None), "placedOrderIds"),
        &Value::from(boundary_id)
    );
    let long_ids = vec!["9".repeat(32); 16];
    let long_error = VenueError::new(HOSTILE).partial_ids(&long_ids);
    let event = projected(&long_error, None);
    let placed = field(&event, "placedOrderIds").as_str().unwrap();
    assert_eq!(
        placed.split(',').count(),
        15,
        "whole IDs fit the engine's 512-byte field cap"
    );
    assert_eq!(placed, long_ids[..15].join(","));
    let event = BitgetFutures::unknown_outcome_telemetry().unwrap()(&EngineUnknownOutcome {
        now: 0.0,
        command: u32::MAX,
        request: RequestId(0),
        status: None,
        error: &error,
    });
    assert_eq!(field(&event, "command"), &Value::from("command-4294967295"));
}
