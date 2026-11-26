mod support;

use safe_cex_engine::write_plan::{Element, ElementResult, WritePlan};
use safe_cex_engine::{
    Action, Command, EngineUnknownOutcome, Event, Level, LogEvent, Notice, Outcome, RequestId,
    Response, Value, Venue, VenueError,
};
use safe_cex_venue_blofin::Blofin;
use support::Harness;

fn ready(frame_fed: bool) -> Harness {
    let mut source = Harness::new();
    source.seed();
    if !frame_fed {
        return source;
    }
    let frame = source.e.snapshot_frame(source.now).to_vec();
    let mut h = Harness::frame_fed();
    h.e.apply_frame(h.now, &frame, true).unwrap();
    h.collect();
    h
}

fn place(h: &mut Harness) -> safe_cex_engine::HttpRequest {
    h.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: serde_json::from_str(
                r#"[{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}]"#,
            )
            .unwrap(),
        }),
    );
    h.collect();
    h.take("batch-orders")
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

fn text<'a>(event: &'a LogEvent, key: &str) -> &'a str {
    event
        .fields
        .iter()
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| value.as_str())
        .expect("required string field")
}

fn decimal(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn assert_projection(event: &LogEvent) {
    assert_eq!(event.level, Level::Warn);
    assert_eq!(event.code, "order.outcome_unknown");
    assert_eq!(event.message, safe_cex_engine::UNKNOWN_OUTCOME_TEXT);
    let mut keys: Vec<_> = event.fields.iter().map(|(key, _)| *key).collect();
    keys.sort_unstable();
    let has_status = keys.contains(&"status");
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
    assert_eq!(keys, expected);
    let command = text(event, "command");
    assert!((1..=64).contains(&command.len()));
    assert!(command.as_bytes()[0].is_ascii_alphabetic());
    assert!(
        command
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
    );
    assert_eq!(text(event, "outcome"), "unknown");
    // Equivalent to the required anchored regex ^[0-9]{1,20}$.
    assert!(decimal(text(event, "request"), 20));
    for (key, max_digits) in [("placedOrderIds", 32), ("unknownIndices", 5)] {
        let value = text(event, key);
        let entries: Vec<_> = if value.is_empty() {
            vec![]
        } else {
            value.split(',').collect()
        };
        assert!(entries.len() <= 16);
        assert!(value.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
        for entry in entries {
            assert!(decimal(entry, max_digits));
            if key == "unknownIndices" {
                assert!(entry.parse::<u32>().unwrap() <= 65_535);
            }
        }
    }
    for (key, value) in &event.fields {
        if *key == "status" {
            let Value::Num(status) = value else {
                panic!("numeric status only")
            };
            assert_eq!(status.fract(), 0.0);
            assert!((100.0..=599.0).contains(status));
        }
        let encoded = format!("{key}:{value:?}").to_ascii_lowercase();
        for forbidden in [
            "http",
            "://",
            "/api",
            "?",
            "&",
            "=",
            "body",
            "payload",
            "signature",
            "api_key",
            "synthetic_secret",
            "synthetic_signed",
        ] {
            assert!(
                !encoded.contains(forbidden),
                "field {key} leaked {forbidden}"
            );
        }
    }
}

#[test]
fn engine_sealed_unknown_emits_once_with_a_decimal_request() {
    for frame_fed in [false, true] {
        for split in [false, true] {
            for status in [408, 429, 503] {
                let mut h = ready(frame_fed);
                let request = place(&mut h);
                let request_id = request.id;
                let body = "https://venue.test/api/orders?api_key=SYNTHETIC_SECRET&signature=SYNTHETIC_SIGNED";
                if split {
                    h.handle(
                        2.0,
                        Event::HttpStart {
                            request: request_id,
                            status,
                            headers: "",
                        },
                    );
                    h.handle(
                        2.0,
                        Event::HttpChunk {
                            request: request_id,
                            data: body.as_bytes(),
                        },
                    );
                    h.handle(
                        2.0,
                        Event::HttpEnd {
                            request: request_id,
                        },
                    );
                    h.collect();
                } else {
                    h.respond_status(request, status, body);
                }
                assert_eq!(h.failures.len(), 1);
                assert_eq!(h.failures[0].outcome, Some(Outcome::Unknown));
                let emitted = events(&h);
                assert_eq!(
                    emitted.len(),
                    1,
                    "frame_fed={frame_fed}, split={split}, status={status}"
                );
                assert_projection(emitted[0]);
                assert_eq!(text(emitted[0], "request"), request_id.0.to_string());
                assert!(emitted[0].fields.contains(&("status", status.into())));
                h.handle(
                    3.0,
                    Event::HttpResponse {
                        request: request_id,
                        response: Response {
                            status,
                            headers: "",
                            body,
                        },
                    },
                );
                h.collect();
                assert_eq!(events(&h).len(), 1, "late replies must not emit again");
                assert!(
                    h.pending
                        .iter()
                        .all(|request| request.method == safe_cex_engine::Method::Get)
                );
            }
        }
    }
}

#[test]
fn lost_transport_emits_once_without_an_http_status() {
    let mut h = ready(true);
    let request = place(&mut h);
    h.handle(
        2.0,
        Event::HttpFailed {
            request: request.id,
            error: "SYNTHETIC_SECRET body signature",
        },
    );
    h.collect();
    let emitted = events(&h);
    assert_eq!(emitted.len(), 1);
    assert_projection(emitted[0]);
    assert!(!emitted[0].fields.iter().any(|(key, _)| *key == "status"));
    assert_eq!(text(emitted[0], "request"), request.id.0.to_string());
    h.handle(
        3.0,
        Event::HttpFailed {
            request: request.id,
            error: "late failure",
        },
    );
    h.collect();
    assert_eq!(events(&h).len(), 1);
    assert!(h.pending.is_empty());
}

#[test]
fn native_unknown_still_emits_once() {
    for frame_fed in [false, true] {
        let mut h = ready(frame_fed);
        let request = place(&mut h);
        let request_id = request.id;
        h.respond(
            request,
            r#"{"code":"0","data":[{"orderId":"0","code":"103013","msg":"fixture"}]}"#,
        );
        assert_eq!(h.failures.len(), 1);
        let emitted = events(&h);
        assert_eq!(emitted.len(), 1);
        assert_projection(emitted[0]);
        assert_eq!(text(emitted[0], "request"), request_id.0.to_string());
        assert!(
            h.pending
                .iter()
                .all(|request| request.method == safe_cex_engine::Method::Get)
        );
    }
}

#[test]
fn definite_refusal_emits_no_unknown_event() {
    for frame_fed in [false, true] {
        let mut h = ready(frame_fed);
        let request = place(&mut h);
        h.respond(
            request,
            r#"{"code":"0","data":[{"orderId":"0","code":"102014","msg":"fixture"}]}"#,
        );
        assert_eq!(h.failures.len(), 1);
        assert_eq!(h.failures[0].outcome, Some(Outcome::Rejected));
        assert!(events(&h).is_empty());
    }
}

fn project(error: &VenueError, request: u64, command: u32, status: Option<u16>) -> LogEvent {
    Blofin::unknown_outcome_telemetry().expect("BloFin opts into the static hook")(
        &EngineUnknownOutcome {
            now: 1.0,
            command,
            request: RequestId(request),
            status,
            error,
        },
    )
}

#[test]
fn nx_projection_uses_only_closed_fields_and_bounded_lists() {
    let mut journal = WritePlan::new(
        (0..18)
            .map(|index| Element {
                index,
                client_order_id: None,
                role: None,
            })
            .collect(),
    );
    for index in 0..18 {
        journal
            .record(
                index,
                ElementResult::Unknown {
                    error: Box::new(VenueError::new("SYNTHETIC_SECRET signature body")),
                },
            )
            .unwrap();
    }
    let ids: Vec<_> = (0..18).map(|id| id.to_string()).collect();
    let error = journal.attach(
        VenueError::new("https://venue.test/api/orders?signature=SYNTHETIC_SIGNED")
            .outcome(Outcome::Unknown)
            .partial_ids(&ids),
    );
    for request in [0, 1, u64::MAX] {
        let event = project(&error, request, u32::MAX, Some(599));
        assert_projection(&event);
        assert_eq!(text(&event, "request"), request.to_string());
        assert_eq!(text(&event, "placedOrderIds").split(',').count(), 16);
        assert_eq!(
            text(&event, "unknownIndices"),
            "0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15"
        );
    }
}

#[test]
fn nx_projection_filters_invalid_ids_indices_and_status_bounds() {
    let mut journal = WritePlan::new(
        [65_535, 65_536]
            .into_iter()
            .map(|index| Element {
                index,
                client_order_id: None,
                role: None,
            })
            .collect(),
    );
    for index in [65_535, 65_536] {
        journal
            .record(
                index,
                ElementResult::Unknown {
                    error: Box::new(VenueError::new("opaque payload")),
                },
            )
            .unwrap();
    }
    let ids = vec![
        "1".into(),
        "9".repeat(32),
        "9".repeat(33),
        "https://venue.test/api?signature=SYNTHETIC_SIGNED".into(),
        "1,2".into(),
        "123\n".into(),
        "".into(),
    ];
    let error = journal.attach(
        VenueError::new("SYNTHETIC_SECRET")
            .outcome(Outcome::Unknown)
            .partial_ids(&ids),
    );
    for status in [
        None,
        Some(99),
        Some(100),
        Some(599),
        Some(600),
        Some(u16::MAX),
    ] {
        let event = project(&error, u64::MAX, 0, status);
        assert_projection(&event);
        assert_eq!(
            text(&event, "placedOrderIds"),
            format!("1,{}", "9".repeat(32))
        );
        assert_eq!(text(&event, "unknownIndices"), "65535");
        assert_eq!(
            event.fields.iter().any(|(key, _)| *key == "status"),
            status.is_some_and(|value| (100..=599).contains(&value))
        );
    }
    let ids = vec!["9".repeat(32); 17];
    let error = VenueError::new("ignored")
        .outcome(Outcome::Unknown)
        .partial_ids(&ids);
    let event = project(&error, 1, 7, None);
    assert_projection(&event);
    assert_eq!(
        text(&event, "placedOrderIds").split(',').count(),
        15,
        "retain whole IDs below the engine's 512-byte string bound"
    );
    assert_eq!(text(&event, "unknownIndices"), "");
}
