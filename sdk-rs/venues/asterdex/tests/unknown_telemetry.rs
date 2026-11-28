use safe_cex_engine::error::Partial;
use safe_cex_engine::log::Value;
use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
use safe_cex_engine::{EngineUnknownOutcome, LogEvent, Outcome, RequestId, Venue, VenueError};
use safe_cex_venue_asterdex::AsterDex;

const UNSAFE: &str = "https://signed.invalid/order?apiKey=fixture-key&signature=fixture-signature";

fn event(error: &VenueError, status: Option<u16>, request: u64) -> LogEvent {
    AsterDex::unknown_outcome_telemetry().expect("Aster opts into engine Unknown telemetry")(
        &EngineUnknownOutcome {
            now: 0.0,
            command: u32::MAX,
            request: RequestId(request),
            status,
            error,
        },
    )
}

fn field<'a>(event: &'a LogEvent, key: &str) -> &'a str {
    event
        .fields
        .iter()
        .find(|(name, _)| *name == key)
        .unwrap()
        .1
        .as_str()
        .unwrap()
}

fn schema(event: &LogEvent) {
    assert_eq!(event.code, "order.outcome_unknown");
    let mut keys: Vec<_> = event.fields.iter().map(|(key, _)| *key).collect();
    keys.sort_unstable();
    let expected = if keys.contains(&"status") {
        vec![
            "command",
            "outcome",
            "placedOrderIds",
            "request",
            "status",
            "unknownIndices",
        ]
    } else {
        vec![
            "command",
            "outcome",
            "placedOrderIds",
            "request",
            "unknownIndices",
        ]
    };
    assert_eq!(
        keys, expected,
        "NX #9262 allowlist, without extra or duplicate keys"
    );
    let command = field(event, "command");
    assert!((1..=64).contains(&command.len()) && command.as_bytes()[0].is_ascii_alphabetic());
    assert!(
        command
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    );
    assert_eq!(field(event, "outcome"), "unknown");
    let request = field(event, "request");
    assert!(
        (1..=20).contains(&request.len()) && request.bytes().all(|b| b.is_ascii_digit()),
        "^[0-9]{{1,20}}$"
    );
    for (key, width) in [("placedOrderIds", 32), ("unknownIndices", 5)] {
        let text = field(event, key);
        if !text.is_empty() {
            let items: Vec<_> = text.split(',').collect();
            assert!(items.len() <= 16);
            for item in items {
                assert!(
                    (1..=width).contains(&item.len()) && item.bytes().all(|b| b.is_ascii_digit())
                );
                if key == "unknownIndices" {
                    assert!(item.parse::<u32>().unwrap() <= 65535);
                }
            }
        }
    }
    for (_, value) in &event.fields {
        let text = format!("{value:?}").to_ascii_lowercase();
        for forbidden in [
            "http",
            "://",
            "/order",
            "?",
            "&",
            "=",
            "body",
            "payload",
            "signature",
            "apikey",
            "fixture-key",
            "fixture-signature",
            "endpoint",
        ] {
            assert!(!text.contains(forbidden), "field leaked {forbidden}");
        }
    }
    assert!(!event.message.contains(UNSAFE));
}

#[test]
fn nx_field_rules_bound_lists_and_exclude_signed_material() {
    let mut error = VenueError::new(UNSAFE)
        .outcome(Outcome::Unknown)
        .venue_code(UNSAFE);
    error.venue_message = Some(
        format!("body payload endpoint {UNSAFE}")
            .into_boxed_str()
            .into(),
    );
    let mut ids = vec![
        UNSAFE.into(),
        "".into(),
        "1".repeat(33),
        "123?signature=x".into(),
    ];
    ids.extend((0..18).map(|n| n.to_string()));
    ids.push("1".repeat(32));
    error.partial = Some(Box::new(Partial {
        order_ids: ids,
        total: None,
        failed_index: None,
    }));
    error.elements = Some(Box::new(ElementOutcomes(
        (0..18)
            .chain([65535, 65536])
            .map(|index| ElementOutcome {
                element: Element {
                    index,
                    client_order_id: Some(UNSAFE.into()),
                    role: None,
                },
                result: ElementResult::Unknown {
                    error: Box::new(VenueError::new(UNSAFE)),
                },
            })
            .collect(),
    )));
    for request in [0, u64::MAX] {
        let event = event(&error, Some(503), request);
        schema(&event);
        assert_eq!(
            field(&event, "placedOrderIds"),
            (0..16).map(|n| n.to_string()).collect::<Vec<_>>().join(",")
        );
        assert_eq!(
            field(&event, "unknownIndices"),
            (0..16).map(|n| n.to_string()).collect::<Vec<_>>().join(",")
        );
    }
    error.partial.as_mut().unwrap().order_ids = vec!["1".repeat(32)];
    error
        .elements
        .as_mut()
        .unwrap()
        .0
        .retain(|row| row.element.index >= 65535);
    let projected = event(&error, None, 1);
    schema(&projected);
    assert_eq!(field(&projected, "placedOrderIds"), "1".repeat(32));
    assert_eq!(field(&projected, "unknownIndices"), "65535");
    error.partial.as_mut().unwrap().order_ids = vec!["1".repeat(32); 16];
    let projected = event(&error, None, 1);
    schema(&projected);
    assert_eq!(field(&projected, "placedOrderIds").split(',').count(), 15);
    assert!(field(&projected, "placedOrderIds").len() <= safe_cex_engine::log::MAX_LOG_BYTES);
    error.partial = None;
    error.elements.as_mut().unwrap().0.push(ElementOutcome {
        element: Element {
            index: 0,
            client_order_id: Some(UNSAFE.into()),
            role: None,
        },
        result: ElementResult::Placed {
            order_ids: vec![UNSAFE.into(), "42".into()],
        },
    });
    let projected = event(&error, None, 1);
    schema(&projected);
    assert_eq!(field(&projected, "placedOrderIds"), "42");
}

#[test]
fn nx_status_bounds_and_absence_are_pinned() {
    for status in [
        None,
        Some(0),
        Some(99),
        Some(100),
        Some(599),
        Some(600),
        Some(u16::MAX),
    ] {
        let projected = event(&VenueError::new(UNSAFE), status, 1);
        schema(&projected);
        let actual = projected.fields.iter().find(|(key, _)| *key == "status");
        if let Some(status) = status.filter(|s| (100..=599).contains(s)) {
            assert_eq!(actual.unwrap().1, Value::Num(f64::from(status)));
        } else {
            assert!(actual.is_none());
        }
        assert_eq!(field(&projected, "placedOrderIds"), "");
        assert_eq!(field(&projected, "unknownIndices"), "");
    }
}
