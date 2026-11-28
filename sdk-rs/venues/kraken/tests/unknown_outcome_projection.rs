use safe_cex_engine::error::Partial;
use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
use safe_cex_engine::{EngineUnknownOutcome, LogEvent, RequestId, Value, Venue, VenueError};
use safe_cex_venue_kraken::Kraken;

fn project(error: &VenueError, status: Option<u16>) -> LogEvent {
    Kraken::unknown_outcome_telemetry().unwrap()(&EngineUnknownOutcome {
        now: f64::NAN,
        command: u32::MAX,
        request: RequestId(u64::MAX),
        status,
        error,
    })
}

fn text<'a>(event: &'a LogEvent, key: &str) -> &'a str {
    event
        .fields
        .iter()
        .find(|(name, _)| *name == key)
        .unwrap()
        .1
        .as_str()
        .unwrap()
}

fn row(index: usize, result: ElementResult) -> ElementOutcome {
    ElementOutcome {
        element: Element {
            index,
            client_order_id: Some("https://never-log.test/?signature=payload".into()),
            role: None,
        },
        result,
    }
}

fn unknown(index: usize) -> ElementOutcome {
    row(
        index,
        ElementResult::Unknown {
            error: Box::new(VenueError::new("signed-body-never-log")),
        },
    )
}

#[test]
fn exact_nx_fields_literal_outcome_and_status_bounds() {
    let error = VenueError::default();
    for status in [
        None,
        Some(0),
        Some(99),
        Some(100),
        Some(599),
        Some(600),
        Some(u16::MAX),
    ] {
        let event = project(&error, status);
        let valid_status = status.filter(|s| (100..=599).contains(s));
        let mut names: Vec<_> = event.fields.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        let mut expected = vec![
            "command",
            "outcome",
            "placedOrderIds",
            "request",
            "unknownIndices",
        ];
        if valid_status.is_some() {
            expected.push("status");
        }
        expected.sort_unstable();
        assert_eq!(names, expected);
        assert_eq!(text(&event, "outcome"), "unknown");
        assert_eq!(text(&event, "command"), "kraken-4294967295");
        let command = text(&event, "command");
        assert!((1..=64).contains(&command.len()));
        assert!(command.as_bytes()[0].is_ascii_alphabetic());
        assert!(
            command
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        );
        assert_eq!(text(&event, "request"), "18446744073709551615");
        assert_eq!(text(&event, "placedOrderIds"), "");
        assert_eq!(text(&event, "unknownIndices"), "");
        assert_eq!(
            event
                .fields
                .iter()
                .find(|(key, _)| *key == "status")
                .map(|(_, value)| value),
            valid_status.map(Value::from).as_ref()
        );
    }
}

#[test]
fn unknown_indices_are_bounded_and_capped_at_sixteen() {
    let mut rows = vec![
        unknown(65_536),
        unknown(usize::MAX),
        unknown(0),
        unknown(65_535),
    ];
    rows.extend((1..=20).map(unknown));
    rows.push(row(88, ElementResult::NotSent { error: None }));
    let error = VenueError {
        elements: Some(Box::new(ElementOutcomes(rows))),
        ..VenueError::default()
    };
    let event = project(&error, None);
    let indices: Vec<_> = text(&event, "unknownIndices").split(',').collect();
    assert_eq!(indices.len(), 16);
    assert_eq!(indices[..2], ["0", "65535"]);
    assert_eq!(indices[15], "14");
    for index in indices {
        assert!((1..=5).contains(&index.len()) && index.bytes().all(|b| b.is_ascii_digit()));
        assert!(index.parse::<u32>().unwrap() <= 65_535);
    }
}

#[test]
fn placed_ids_use_only_proven_decimal_ids_and_cap_at_sixteen() {
    let tainted = "https://never-log.test/order?signature=signed-body-never-log";
    let mut ids = vec![
        "".into(),
        "9".repeat(33),
        tainted.into(),
        "0".into(),
        "9".repeat(32),
    ];
    ids.extend((1..=20).map(|i| i.to_string()));
    let error = VenueError {
        message: tainted.into(),
        venue_code: Some(tainted.into()),
        venue_message: Some(tainted.into()),
        partial: Some(Box::new(Partial {
            order_ids: vec![tainted.into(), "8765432109876543210".into()],
            total: None,
            failed_index: None,
        })),
        elements: Some(Box::new(ElementOutcomes(vec![row(
            0,
            ElementResult::Placed { order_ids: ids },
        )]))),
        ..VenueError::default()
    };
    let event = project(&error, Some(599));
    let placed: Vec<_> = text(&event, "placedOrderIds").split(',').collect();
    assert_eq!(placed.len(), 16);
    assert_eq!(placed[0], "0");
    assert_eq!(placed[1], "9".repeat(32));
    assert_eq!(placed[15], "14");
    for id in placed {
        assert!((1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()));
    }
    for (key, value) in event.fields {
        let field = format!("{key} {value:?}");
        for forbidden in [
            "http",
            "/order",
            "?",
            "=",
            "&",
            "signature",
            "signed-body-never-log",
            "8765432109876543210",
        ] {
            assert!(!field.contains(forbidden), "{field}");
        }
    }
}

#[test]
fn long_id_lists_stay_below_the_engine_field_truncation_limit() {
    let error = VenueError {
        elements: Some(Box::new(ElementOutcomes(vec![row(
            0,
            ElementResult::Placed {
                order_ids: vec!["9".repeat(32); 16],
            },
        )]))),
        ..VenueError::default()
    };
    let event = project(&error, None);
    let ids = text(&event, "placedOrderIds");
    assert!(ids.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
    assert_eq!(ids.split(',').count(), 15);
    assert!(ids.bytes().all(|b| b.is_ascii_digit() || b == b','));
}
