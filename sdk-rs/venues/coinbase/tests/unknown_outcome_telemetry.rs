#[path = "support/unknown_fields.rs"]
mod unknown_fields;
use safe_cex_engine::{EngineUnknownOutcome, LogEvent, Outcome, RequestId, Venue, VenueError};
use safe_cex_venue_coinbase::Coinbase;
use unknown_fields::assert_field_rules;

fn projected(error: &VenueError, request: u64, status: Option<u16>) -> LogEvent {
    Coinbase::unknown_outcome_telemetry().expect("Coinbase opts into the hook")(
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
