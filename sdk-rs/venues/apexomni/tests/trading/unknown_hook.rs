use super::*;
use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
use safe_cex_engine::{
    EngineUnknownOutcome, LogEvent, Outcome, RequestId, Value, Venue, VenueError,
};

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
fn engine_unknown_hook_is_once_only_for_http_and_transport() {
    for transport in [false, true] {
        let mut e = ready();
        let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
        if transport {
            e.handle(
                NOW + 11.,
                Event::HttpFailed {
                    request: first.id,
                    error: "SYNTHETIC_SECRET signed payload",
                },
            );
        } else {
            response_status(&mut e, &first, NOW + 11., 503);
        }
        let out = actions(&mut e);
        assert!(out.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandFailed {command:42,error}) if error.outcome == Some(safe_cex_engine::Outcome::Unknown))));
        assert_eq!(events(&out).len(), 1);
        assert_projection(events(&out)[0], first.id, (!transport).then_some(503));
        assert!(placements(&http(out)).is_empty());
        response(&mut e, &first, NOW + 12., REAL_PLACE);
        assert!(events(&actions(&mut e)).is_empty());
    }
}

#[test]
fn native_unknown_hook_is_once_only_and_refusal_has_no_event() {
    for (body, unknown) in [
        (
            r#"{"code":987654,"msg":"https://venue.test/order?signature=SYNTHETIC_SECRET"}"#,
            true,
        ),
        ("{}", true),
        (REFUSED, false),
    ] {
        let mut e = ready();
        let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
        response(&mut e, &first, NOW + 11., body);
        let out = actions(&mut e);
        assert_eq!(events(&out).len(), usize::from(unknown));
        let error = out
            .iter()
            .find_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: 42, error }) => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            error.outcome,
            Some(if unknown {
                safe_cex_engine::Outcome::Unknown
            } else {
                safe_cex_engine::Outcome::Rejected
            })
        );
        if unknown {
            assert_projection(events(&out)[0], first.id, Some(200));
        }
        assert!(placements(&http(out)).is_empty());
        response(&mut e, &first, NOW + 12., body);
        assert!(events(&actions(&mut e)).is_empty());
    }
}

#[test]
fn native_proof_unknown_hook_is_once_only_at_the_existing_bound() {
    for lost in [false, true] {
        let (mut e, socket) = ready_with_socket();
        resting_stop(&mut e, socket, 0.);
        let cancel = amend_to(&mut e, "stop", 56000.).remove(0);
        response(&mut e, &cancel, NOW + 15., REAL_CANCEL);
        let mut last = None;
        for _ in 0..if lost { 2 } else { 31 } {
            let out = actions(&mut e);
            assert!(events(&out).is_empty());
            let (timer, at) = out
                .into_iter()
                .find_map(|action| match action {
                    Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                    _ => None,
                })
                .expect("next proof read");
            e.handle(at, Event::TimerFired { timer });
            let read = requests(&mut e).remove(0);
            if lost {
                e.handle(
                    at,
                    Event::HttpFailed {
                        request: read.id,
                        error: "lost proof",
                    },
                );
            } else {
                response(
                    &mut e,
                    &read,
                    at,
                    r#"{"data":{"id":"stop","status":"OPEN","cumSuccessFillSize":"0"}}"#,
                );
            }
            last = Some(read.id);
        }
        let out = actions(&mut e);
        assert_eq!(events(&out).len(), 1);
        assert_projection(events(&out)[0], last.unwrap(), (!lost).then_some(200));
        assert!(placements(&http(out)).is_empty());
    }
}

#[test]
fn unknown_hook_does_not_let_log_cap_split_a_native_id() {
    let callback = ApexOmni::unknown_outcome_telemetry().unwrap();
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
    let callback = ApexOmni::unknown_outcome_telemetry().expect("ApexOmni adopts engine hook");
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
