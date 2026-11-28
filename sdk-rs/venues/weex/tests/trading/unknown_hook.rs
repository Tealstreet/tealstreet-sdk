use super::*;
use safe_cex_engine::write_plan::{Element, ElementResult, WritePlan};
use safe_cex_engine::{EngineUnknownOutcome, LogEvent, RequestId, Venue, VenueError};

#[test]
fn bulk_cancellation_emits_once_across_native_and_engine_unknowns() {
    for (first_source, second_source) in [
        ("native", "engine"),
        ("native", "native"),
        ("engine", "native"),
        ("engine", "engine"),
        ("native", "transport"),
        ("transport", "native"),
        ("native", "success"),
        ("native", "refusal"),
    ] {
        let mut e = engine();
        e.store_mut().upsert_order(order("1", OrderType::Limit));
        e.store_mut().upsert_order(order("2", OrderType::StopLoss));
        let actions = command(
            &mut e,
            Command::CancelSymbolOrders {
                command: 1,
                symbol: "BTCUSDT".into(),
            },
        );
        let first = writes(&actions).pop().unwrap();
        let mut actions = cancellation_unknown(&mut e, &first, first_source, 1001.0);
        let Some(second) = writes(&actions).pop() else {
            assert!(matches!(first_source, "engine" | "transport"));
            assert_eq!(
                failure(&actions).outcome,
                Some(safe_cex_engine::Outcome::Unknown)
            );
            let found = events(&actions);
            assert_eq!(found.len(), 1, "{first_source}/{second_source}");
            assert_schema(
                found[0],
                if first_source == "engine" {
                    Some(503)
                } else {
                    None
                },
            );
            continue;
        };
        actions.extend(cancellation_unknown(&mut e, &second, second_source, 1002.0));
        assert_eq!(
            failure(&actions).outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
        let found = events(&actions);
        assert_eq!(found.len(), 1, "{first_source}/{second_source}");
        assert_schema(
            found[0],
            if first_source == "transport" {
                None
            } else if first_source == "engine" || second_source == "engine" {
                Some(503)
            } else if second_source == "transport" {
                None
            } else {
                Some(200)
            },
        );
        assert!(writes(&actions).iter().all(|r| r.method == Method::Delete));
    }
}

fn cancellation_unknown(
    e: &mut Engine<WeexFutures>,
    r: &HttpRequest,
    source: &str,
    now: f64,
) -> Vec<Action> {
    if source == "native" {
        response(
            e,
            r,
            json!({"code":"429","msg":"synthetic native uncertainty"}),
        )
    } else if source == "success" {
        response(e, r, json!([{"orderId":"2","success":true}]))
    } else if source == "refusal" {
        response(
            e,
            r,
            json!({"code":"42002","msg":"synthetic definite refusal"}),
        )
    } else {
        e.handle(
            now,
            if source == "transport" {
                Event::HttpFailed {
                    request: r.id,
                    error: "synthetic lost cancellation",
                }
            } else {
                Event::HttpResponse {
                    request: r.id,
                    response: Response {
                        status: 503,
                        headers: "",
                        body: "{}",
                    },
                }
            },
        );
        safe_cex_engine::test_support::prepared_actions(e, now)
    }
}

fn project(error: &VenueError, request: u64, status: Option<u16>) -> LogEvent {
    let hook = <WeexFutures as Venue>::unknown_outcome_telemetry()
        .expect("WEEX opts into the engine Unknown hook");
    hook(&EngineUnknownOutcome {
        now: 1001.0,
        command: u32::MAX,
        request: RequestId(request),
        status,
        error,
    })
}

fn field<'a>(event: &'a LogEvent, key: &str) -> Option<&'a safe_cex_engine::log::Value> {
    event
        .fields
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value)
}

fn assert_schema(event: &LogEvent, status: Option<u16>) {
    assert_eq!(event.level, safe_cex_engine::Level::Warn);
    assert_eq!(event.code, "order.outcome_unknown");
    assert_eq!(event.message, safe_cex_engine::UNKNOWN_OUTCOME_TEXT);
    let mut keys = event.fields.iter().map(|(key, _)| *key).collect::<Vec<_>>();
    keys.sort_unstable();
    let mut expected = vec![
        "command",
        "outcome",
        "placedOrderIds",
        "request",
        "unknownIndices",
    ];
    if status.is_some() {
        expected.push("status");
        expected.sort_unstable();
    }
    assert_eq!(keys, expected, "NX #9262 admits no other fields");
    let command = field(event, "command").unwrap().as_str().unwrap();
    assert!(!command.is_empty() && command.len() <= 64);
    assert!(command.as_bytes()[0].is_ascii_alphabetic());
    assert!(
        command
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    );
    assert_eq!(field(event, "outcome").unwrap().as_str(), Some("unknown"));
    let request = field(event, "request").unwrap().as_str().unwrap();
    // Exact ^[0-9]{1,20}$ contract: ASCII digits, no whitespace or coercion.
    assert!((1..=20).contains(&request.len()) && request.bytes().all(|b| b.is_ascii_digit()));
    if let Some(status) = status {
        assert!((100..=599).contains(&status));
        assert_eq!(
            field(event, "status"),
            Some(&safe_cex_engine::log::Value::Num(status as f64))
        );
    }
    for (key, value) in &event.fields {
        if let Some(text) = value.as_str() {
            let lower = text.to_ascii_lowercase();
            for forbidden in [
                "http:",
                "https:",
                "/",
                "?",
                "&",
                "=",
                "signature",
                "api_key",
                "payload",
                "body",
                "synthetic_sensitive",
            ] {
                assert!(
                    !lower.contains(forbidden),
                    "{key} must never expose URL/query/body/signature content"
                );
            }
        }
    }
    for key in ["placedOrderIds", "unknownIndices"] {
        let value = field(event, key).unwrap().as_str().unwrap();
        let entries = if value.is_empty() {
            vec![]
        } else {
            value.split(',').collect::<Vec<_>>()
        };
        assert!(entries.len() <= 16);
        for entry in entries {
            assert!(!entry.is_empty() && entry.bytes().all(|b| b.is_ascii_digit()));
            if key == "placedOrderIds" {
                assert!(entry.len() <= 32);
            } else {
                assert!(entry.len() <= 5 && entry.parse::<u32>().unwrap() <= 65535);
            }
        }
    }
    let fields = event
        .fields
        .iter()
        .map(|(key, value)| {
            let value = match value {
                safe_cex_engine::log::Value::Num(value) => json!(value),
                _ => json!(value.as_str().expect("NX field must be string or status")),
            };
            ((*key).to_owned(), value)
        })
        .collect::<serde_json::Map<_, _>>();
    assert!(serde_json::to_vec(&fields).unwrap().len() <= 1024);
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

fn first_write(frame_fed: bool) -> (Engine<WeexFutures>, HttpRequest) {
    let mut e = engine();
    if frame_fed {
        e.set_frame_fed().unwrap();
    }
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts("hook")],
        },
    );
    let requests = writes(&actions);
    assert_eq!(requests.len(), 1);
    (e, requests[0].clone())
}

#[test]
fn engine_sealed_unknown_emits_once_in_ordinary_and_frame_fed_modes() {
    for frame_fed in [false, true] {
        for status in [408, 429, 503] {
            let (mut e, request) = first_write(frame_fed);
            let text = r#"{"code":"40006","msg":"synthetic refusal"}"#;
            e.handle(
                1001.0,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status,
                        headers: "",
                        body: text,
                    },
                },
            );
            let actions = e.drain_actions().collect::<Vec<_>>();
            let found = events(&actions);
            assert_eq!(found.len(), 1, "frame_fed={frame_fed}, status={status}");
            assert_eq!(
                failure(&actions).outcome,
                Some(safe_cex_engine::Outcome::Unknown)
            );
            assert_schema(found[0], Some(status));
            assert_eq!(
                field(found[0], "request").unwrap().as_str(),
                Some(request.id.0.to_string().as_str())
            );
            e.handle(
                1002.0,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status,
                        headers: "",
                        body: text,
                    },
                },
            );
            assert!(events(&e.drain_actions().collect::<Vec<_>>()).is_empty());
        }
    }
}

#[test]
fn released_transport_unknown_omits_status() {
    let (mut e, request) = first_write(false);
    e.handle(
        1001.0,
        Event::HttpFailed {
            request: request.id,
            error: "synthetic lost reply",
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let found = events(&actions);
    assert_eq!(found.len(), 1);
    assert_schema(found[0], None);
    assert_eq!(
        field(found[0], "request").unwrap().as_str(),
        Some(request.id.0.to_string().as_str())
    );
}

#[test]
fn native_unknown_still_emits_exactly_one() {
    let (mut e, request) = first_write(false);
    let payload = body(&request);
    let actions = response(
        &mut e,
        &request,
        json!([{
            "clientOrderId":payload["batchOrders"][0]["newClientOrderId"],
            "success":false,"errorCode":"429","errorMessage":"synthetic native refusal"
        }]),
    );
    assert_eq!(events(&actions).len(), 1);
}

#[test]
fn native_unknown_fields_also_match_nx_rules() {
    let (mut e, request) = first_write(false);
    let payload = body(&request);
    let actions = response(
        &mut e,
        &request,
        json!([{
            "clientOrderId":payload["batchOrders"][0]["newClientOrderId"],
            "success":false,"errorCode":"429","errorMessage":"synthetic native refusal"
        }]),
    );
    let found = events(&actions);
    assert_eq!(found.len(), 1);
    assert_schema(found[0], Some(200));
    assert_eq!(
        field(found[0], "request").unwrap().as_str(),
        Some(request.id.0.to_string().as_str())
    );
}

#[test]
fn definite_refusal_emits_no_unknown_event() {
    let (mut e, request) = first_write(false);
    let payload = body(&request);
    let actions = response(
        &mut e,
        &request,
        json!([{
            "clientOrderId":payload["batchOrders"][0]["newClientOrderId"],
            "success":false,"errorCode":"42002","errorMessage":"synthetic refusal"
        }]),
    );
    assert_eq!(
        failure(&actions).outcome,
        Some(safe_cex_engine::Outcome::Rejected)
    );
    assert!(events(&actions).is_empty());
}

#[test]
fn request_and_optional_status_boundaries_are_closed() {
    let error = VenueError::new(
        "https://venue.invalid/order?api_key=SYNTHETIC_SENSITIVE&signature=x body=payload",
    );
    for request in [0, 1, u64::MAX] {
        for status in [None, Some(99), Some(100), Some(599), Some(600)] {
            let event = project(&error, request, status);
            assert_schema(&event, status.filter(|status| (100..=599).contains(status)));
            assert_eq!(
                field(&event, "request").unwrap().as_str(),
                Some(request.to_string().as_str())
            );
        }
    }
}

#[test]
fn indices_and_placed_ids_are_filtered_and_capped_at_sixteen() {
    let mut report = WritePlan::new(
        (0..20)
            .map(|index| Element {
                index,
                client_order_id: None,
                role: None,
            })
            .collect(),
    );
    for index in 0..20 {
        report
            .record(
                index,
                ElementResult::Unknown {
                    error: Box::new(VenueError::new("SYNTHETIC_SENSITIVE")),
                },
            )
            .unwrap();
    }
    let event = project(&report.attach(VenueError::new("unknown")), 1, Some(503));
    assert_schema(&event, Some(503));
    assert_eq!(
        field(&event, "unknownIndices").unwrap().as_str().unwrap(),
        (0..16).map(|n| n.to_string()).collect::<Vec<_>>().join(",")
    );
    let mut report = WritePlan::new(vec![
        Element {
            index: 65535,
            client_order_id: None,
            role: None,
        },
        Element {
            index: 65536,
            client_order_id: None,
            role: None,
        },
    ]);
    for index in [65535, 65536] {
        report
            .record(
                index,
                ElementResult::Unknown {
                    error: Box::new(VenueError::new("unknown")),
                },
            )
            .unwrap();
    }
    let event = project(&report.attach(VenueError::new("unknown")), 1, None);
    assert_eq!(
        field(&event, "unknownIndices").unwrap().as_str(),
        Some("65535")
    );
    let mut ids = vec![
        "https://venue.invalid?signature=x".to_owned(),
        "9".repeat(33),
        "abc".into(),
        "".into(),
    ];
    ids.extend((1..=20).map(|n| n.to_string()));
    let mut report = WritePlan::new(vec![Element {
        index: 0,
        client_order_id: None,
        role: None,
    }]);
    report
        .record(0, ElementResult::Placed { order_ids: ids })
        .unwrap();
    let event = project(&report.attach(VenueError::new("unknown")), 1, Some(503));
    assert_schema(&event, Some(503));
    assert_eq!(
        field(&event, "placedOrderIds").unwrap().as_str().unwrap(),
        (1..=16)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    report
        .record(
            0,
            ElementResult::Placed {
                order_ids: vec!["9".repeat(32)],
            },
        )
        .unwrap();
    let event = project(&report.attach(VenueError::new("unknown")), u64::MAX, None);
    assert_schema(&event, None);
    assert_eq!(
        field(&event, "placedOrderIds").unwrap().as_str(),
        Some("99999999999999999999999999999999")
    );
}

#[test]
fn accepted_prefix_is_retained_for_engine_and_native_unknowns() {
    for engine_sealed in [false, true] {
        let mut e = engine();
        let mut conditional = opts("");
        conditional.kind = OrderType::Conditional;
        conditional.trigger_price = Some(68000.0);
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![conditional.clone(), opts(""), conditional],
            },
        );
        let first = writes(&actions)[0].clone();
        let actions = response(&mut e, &first, json!({"success":true,"orderId":"100"}));
        let requests = writes(&actions);
        assert_eq!(requests.len(), 1);
        let second = &requests[0];
        let actions = if engine_sealed {
            e.handle(
                1002.0,
                Event::HttpResponse {
                    request: second.id,
                    response: Response {
                        status: 503,
                        headers: "",
                        body: "synthetic lost acknowledgement",
                    },
                },
            );
            e.drain_actions().collect::<Vec<_>>()
        } else {
            let payload = body(second);
            response(
                &mut e,
                second,
                json!([{
                    "clientOrderId":payload["batchOrders"][0]["newClientOrderId"],
                    "success":false,"errorCode":"429","errorMessage":"synthetic refusal"
                }]),
            )
        };
        let found = events(&actions);
        assert_eq!(found.len(), 1);
        assert_schema(found[0], Some(if engine_sealed { 503 } else { 200 }));
        assert_eq!(
            field(found[0], "placedOrderIds").unwrap().as_str(),
            Some("100")
        );
        assert_eq!(
            field(found[0], "unknownIndices").unwrap().as_str(),
            Some("1")
        );
        assert_eq!(
            field(found[0], "request").unwrap().as_str(),
            Some(second.id.0.to_string().as_str())
        );
        assert!(writes(&actions).is_empty());
    }
}

#[test]
fn cancellation_targets_and_unindexed_partial_ids_are_not_placements() {
    let error = VenueError::new("synthetic unknown").partial_ids(&["123".into()]);
    let event = project(&error, 1, None);
    assert_schema(&event, None);
    assert_eq!(field(&event, "placedOrderIds").unwrap().as_str(), Some(""));
    assert_eq!(field(&event, "unknownIndices").unwrap().as_str(), Some(""));
    let mut report = WritePlan::new(vec![Element {
        index: 0,
        client_order_id: Some("SYNTHETIC_SENSITIVE".into()),
        role: None,
    }]);
    report
        .record(
            0,
            ElementResult::Applied {
                effect: safe_cex_engine::write_plan::AppliedEffect::Cancel,
                target_order_id: Some("456".into()),
                position_key: None,
            },
        )
        .unwrap();
    let event = project(&report.attach(error), 1, None);
    assert_schema(&event, None);
    assert_eq!(field(&event, "placedOrderIds").unwrap().as_str(), Some(""));
}
