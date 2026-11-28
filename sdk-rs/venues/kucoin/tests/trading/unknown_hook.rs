use super::*;
use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
use safe_cex_engine::{EngineUnknownOutcome, LogEvent, RequestId, Value, Venue, VenueError};

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

fn field<'a>(event: &'a LogEvent, name: &str) -> Option<&'a Value> {
    event
        .fields
        .iter()
        .find_map(|(key, value)| (*key == name).then_some(value))
}

fn decimal(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn assert_projection(event: &LogEvent, request: RequestId, status: Option<u16>) {
    let expected = [
        "command",
        "outcome",
        "placedOrderIds",
        "unknownIndices",
        "request",
        "status",
    ];
    assert_eq!(event.fields.len(), if status.is_some() { 6 } else { 5 });
    assert!(event.fields.iter().all(|(key, _)| expected.contains(key)));
    let request_value = field(event, "request").unwrap().as_str().unwrap();
    // Exactly ^[0-9]{1,20}$; never an endpoint or signed venue URL.
    assert!(decimal(request_value, 20));
    assert_eq!(request_value, request.0.to_string());
    let command = field(event, "command").unwrap().as_str().unwrap();
    assert!(!command.is_empty() && command.len() <= 64);
    assert!(command.as_bytes()[0].is_ascii_alphabetic());
    assert!(
        command
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
    );
    assert_eq!(field(event, "outcome").unwrap().as_str(), Some("unknown"));
    for name in ["placedOrderIds", "unknownIndices"] {
        let list = field(event, name).unwrap().as_str().unwrap();
        let entries: Vec<_> = if list.is_empty() {
            vec![]
        } else {
            list.split(',').collect()
        };
        assert!(entries.len() <= 16);
        for entry in entries {
            if name == "placedOrderIds" {
                assert!(decimal(entry, 32));
            } else {
                assert!(decimal(entry, 5) && entry.parse::<u16>().is_ok());
            }
        }
    }
    match status {
        Some(status) => assert_eq!(field(event, "status"), Some(&Value::Num(f64::from(status)))),
        None => assert!(field(event, "status").is_none()),
    }
    for (_, value) in &event.fields {
        let text = format!("{value:?}").to_ascii_lowercase();
        for forbidden in [
            "http",
            "venue.test",
            "/api/",
            "?",
            "=",
            "query",
            "body",
            "signature",
            "payload",
            "synthetic_secret",
            "{",
        ] {
            assert!(!text.contains(forbidden), "unsafe telemetry field: {text}");
        }
    }
}

#[test]
fn engine_unknown_hook_emits_once_and_ignores_late_delivery() {
    for transport in [false, true] {
        let mut h = Harness::new();
        resting_stop(&mut h);
        amend_stop(&mut h, "old", 120.0);
        h.metadata(false, "CROSS");
        let request = h.take("/api/v1/orders/old");
        if transport {
            h.event(Event::HttpFailed {
                request: request.id,
                error: "SYNTHETIC_SECRET signed payload",
            });
        } else {
            h.response(&request, 503, REFUSED);
        }
        assert_eq!(failure(&h).outcome, Some(Outcome::Unknown));
        assert_eq!(events(&h).len(), 1);
        assert_projection(events(&h)[0], request.id, (!transport).then_some(503));
        h.response(&request, 200, r#"{"code":"200000","data":{}}"#);
        assert_eq!(events(&h).len(), 1);
        assert!(writes(&h).is_empty());
    }
}

#[test]
fn native_unknown_hook_emits_once_and_refusal_emits_none() {
    for (body, unknown) in [(BUSINESS_RATE_LIMITED, true), (REFUSED, false)] {
        let mut h = Harness::new();
        place(&mut h, vec![order()]);
        h.metadata(false, "CROSS");
        let request = h.take("/api/v1/orders");
        h.response(&request, 200, body);
        h.pace();
        assert_eq!(events(&h).len(), usize::from(unknown));
        if unknown {
            assert_eq!(failure(&h).outcome, Some(Outcome::Unknown));
            assert_projection(events(&h)[0], request.id, Some(200));
        } else {
            assert_eq!(failure(&h).outcome, Some(Outcome::Rejected));
        }
        h.response(&request, 200, body);
        assert_eq!(events(&h).len(), usize::from(unknown));
        assert!(writes(&h).is_empty());
    }
}

#[test]
fn native_protective_unknown_hook_does_not_duplicate_engine_event() {
    let mut h = Harness::new();
    resting_stop(&mut h);
    amend_stop(&mut h, "old", 120.0);
    h.metadata(false, "CROSS");
    let request = h.take("/api/v1/orders/old");
    h.response(&request, 200, r#"{"code":"429000","msg":"rate limited"}"#);
    assert_eq!(failure(&h).outcome, Some(Outcome::Unknown));
    assert_eq!(events(&h).len(), 1);
    assert_projection(events(&h)[0], request.id, Some(200));
    assert!(writes(&h).is_empty());
}

#[test]
fn native_unknown_hook_keeps_first_request_across_cancel_legs() {
    let mut h = Harness::new();
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["1".into(), "2".into()],
    });
    let first = h.take("/api/v1/orders/1");
    h.response(&first, 200, BUSINESS_RATE_LIMITED);
    h.pace();
    let second = h.take("/api/v1/orders/2");
    h.response(&second, 200, BUSINESS_RATE_LIMITED);
    h.pace();
    assert_eq!(events(&h).len(), 1);
    assert_projection(events(&h)[0], first.id, Some(200));
}

#[test]
fn native_unknown_hook_retains_only_confirmed_created_ids() {
    let mut h = Harness::new();
    let mut second_order = order();
    second_order.client_order_id = Some("second-caller".into());
    place(&mut h, vec![order(), second_order]);
    h.metadata(false, "CROSS");
    let first = h.take("/api/v1/orders");
    h.response(
        &first,
        200,
        r#"{"code":"200000","data":{"orderId":"123456789"}}"#,
    );
    h.pace();
    let second = h.take("/api/v1/orders");
    h.response(&second, 200, BUSINESS_RATE_LIMITED);
    h.pace();
    assert_eq!(events(&h).len(), 1);
    assert_projection(events(&h)[0], second.id, Some(200));
    assert_eq!(
        field(events(&h)[0], "placedOrderIds").unwrap().as_str(),
        Some("123456789")
    );
    assert!(writes(&h).is_empty());
}

#[test]
fn native_unknown_hook_reports_exhausted_protective_proof_once() {
    let mut h = Harness::new();
    resting_stop(&mut h);
    amend_stop(&mut h, "old", 120.0);
    h.metadata(false, "CROSS");
    let cancel = h.take("/api/v1/orders/old");
    h.response(
        &cancel,
        200,
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    let mut last = None;
    for attempt in 0..3 {
        let proof = h.take("/api/v1/orders/old");
        h.response(&proof, 400, r#"{"code":"400100","msg":"proof refused"}"#);
        last = Some(proof);
        if attempt < 2 {
            h.advance(100.);
        }
    }
    assert_eq!(failure(&h).outcome, Some(Outcome::Unknown));
    assert_eq!(events(&h).len(), 1);
    assert_projection(events(&h)[0], last.unwrap().id, Some(400));
    assert!(writes(&h).is_empty());
}

#[test]
fn unknown_hook_does_not_let_log_cap_split_a_native_id() {
    let callback = Kucoin::unknown_outcome_telemetry().unwrap();
    let mut error = VenueError::new("unknown").outcome(Outcome::Unknown);
    error.partial = Some(Box::new(safe_cex_engine::Partial {
        order_ids: (0..16).map(|_| "9".repeat(32)).collect(),
        total: None,
        failed_index: None,
    }));
    let event = callback(&EngineUnknownOutcome {
        now: 1.,
        command: 1,
        request: RequestId(9),
        status: None,
        error: &error,
    });
    let ids = field(&event, "placedOrderIds").unwrap().as_str().unwrap();
    assert!(ids.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
    assert_eq!(ids.split(',').count(), 15);
    assert!(ids.split(',').all(|id| id == "9".repeat(32)));
}

#[test]
fn unknown_hook_projection_matches_nx_9262_bounds_and_never_copies_payloads() {
    let callback = Kucoin::unknown_outcome_telemetry().expect("KuCoin adopts engine hook");
    let hostile = "https://venue.test/api/order?api_key=SYNTHETIC_SECRET&signature=SIGNED_PAYLOAD";
    let mut error = VenueError::new(hostile)
        .venue_code(hostile)
        .outcome(Outcome::Unknown);
    error.venue_message = Some(hostile.into());
    error.partial = Some(Box::new(safe_cex_engine::Partial {
        order_ids: vec![
            hostile.into(),
            "x".into(),
            "9".repeat(33),
            "9".repeat(32),
            "1".into(),
        ],
        total: None,
        failed_index: None,
    }));
    for status in [
        None,
        Some(0),
        Some(99),
        Some(100),
        Some(599),
        Some(600),
        Some(u16::MAX),
    ] {
        for request in [RequestId(0), RequestId(u64::MAX)] {
            let event = callback(&EngineUnknownOutcome {
                now: 1.,
                command: u32::MAX,
                request,
                status,
                error: &error,
            });
            assert_projection(
                &event,
                request,
                status.filter(|status| (100..=599).contains(status)),
            );
            assert_eq!(
                field(&event, "placedOrderIds").unwrap().as_str(),
                Some(format!("{},1", "9".repeat(32)).as_str())
            );
        }
    }
    let rows = (0..20)
        .chain([65_535, 65_536, usize::MAX])
        .map(|index| ElementOutcome {
            element: Element {
                index,
                client_order_id: Some(hostile.into()),
                role: None,
            },
            result: ElementResult::Unknown {
                error: Box::new(VenueError::new(hostile)),
            },
        })
        .collect();
    error.elements = Some(Box::new(ElementOutcomes(rows)));
    error.partial.as_mut().unwrap().order_ids = (1..=20).map(|id| id.to_string()).collect();
    let event = callback(&EngineUnknownOutcome {
        now: 1.,
        command: 1,
        request: RequestId(7),
        status: None,
        error: &error,
    });
    assert_projection(&event, RequestId(7), None);
    assert_eq!(
        field(&event, "unknownIndices")
            .unwrap()
            .as_str()
            .unwrap()
            .split(',')
            .count(),
        16
    );
    assert_eq!(
        field(&event, "placedOrderIds")
            .unwrap()
            .as_str()
            .unwrap()
            .split(',')
            .count(),
        16
    );
    error.elements.as_mut().unwrap().0.drain(..20);
    let event = callback(&EngineUnknownOutcome {
        now: 1.,
        command: 1,
        request: RequestId(7),
        status: None,
        error: &error,
    });
    assert_eq!(
        field(&event, "unknownIndices").unwrap().as_str(),
        Some("65535")
    );
}
