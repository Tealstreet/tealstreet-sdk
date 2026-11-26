use regex::Regex;
use safe_cex_engine::error::Partial;
use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
use safe_cex_engine::{
    EngineUnknownOutcome, Level, LogEvent, Outcome, RequestId, Value, Venue, VenueError,
};
use safe_cex_venue_lighter::Lighter;

const HOSTILE: &str = "https://venue.test/api/order?api_key=SYNTHETIC_SECRET&signature=SYNTHETIC_SIGNATURE body=SIGNED_PAYLOAD";

fn project(request: u64, status: Option<u16>, error: &VenueError) -> LogEvent {
    Lighter::unknown_outcome_telemetry().expect("Lighter opts into engine-sealed Unknown telemetry")(
        &EngineUnknownOutcome {
            now: 123.,
            command: u32::MAX,
            request: RequestId(request),
            status,
            error,
        },
    )
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
