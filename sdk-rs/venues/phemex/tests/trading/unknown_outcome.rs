use super::*;
use safe_cex_engine::{EngineUnknownOutcome, Outcome, RequestId, Venue, VenueError};

fn events(actions: &[Action]) -> Vec<&safe_cex_engine::LogEvent> {
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

#[test]
fn unknown_hook_engine_seals_once_without_resending() {
    for status in [408, 429, 503] {
        let mut e = engine();
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 77,
                orders: vec![opts("hook")],
            },
        );
        let request = writes(&actions)[0].clone();
        let actions = raw_status(&mut e, &request, status);
        assert_eq!(failure(&actions).outcome, Some(Outcome::Unknown));
        assert!(writes(&actions).is_empty());
        let diagnostics = events(&actions);
        assert_eq!(diagnostics.len(), 1, "HTTP {status}");
        unknown_fields::assert_event(diagnostics[0], 77, request.id.0, Some(status));
        let late = raw_status(&mut e, &request, status);
        assert!(events(&late).is_empty());
        assert!(writes(&late).is_empty());
        assert!(
            !late
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
}

#[test]
fn unknown_hook_native_decode_emits_once_and_refusal_emits_zero() {
    for (body, unknown) in [
        (r#"{"code":100,"msg":"unlisted"}"#, true),
        ("malformed", true),
        (r#"{"code":11001,"msg":"balance","data":null}"#, false),
    ] {
        let mut e = engine();
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 77,
                orders: vec![opts("hook")],
            },
        );
        let request = writes(&actions)[0].clone();
        let actions = raw(&mut e, &request, body);
        let diagnostics = events(&actions);
        assert_eq!(diagnostics.len(), usize::from(unknown), "{body}");
        if unknown {
            assert_eq!(failure(&actions).outcome, Some(Outcome::Unknown));
            unknown_fields::assert_event(diagnostics[0], 77, request.id.0, Some(200));
        } else {
            assert_eq!(failure(&actions).outcome, Some(Outcome::Rejected));
        }
        assert!(writes(&actions).is_empty());
        let late = raw(&mut e, &request, body);
        assert!(events(&late).is_empty());
        assert!(writes(&late).is_empty());
    }
}

#[test]
fn unknown_hook_transport_omits_status_and_never_projects_failure_text() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 77,
            orders: vec![opts("hook")],
        },
    );
    let request = writes(&actions)[0].clone();
    e.handle(
        1001.,
        Event::HttpFailed {
            request: request.id,
            error: "https://venue.test/g-orders?api_key=sentinel&signature=sentinel body payload",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let diagnostics = events(&actions);
    assert_eq!(diagnostics.len(), 1);
    unknown_fields::assert_event(diagnostics[0], 77, request.id.0, None);
    assert!(writes(&actions).is_empty());
}

#[test]
fn unknown_hook_projection_matches_nx_bounds_and_caps() {
    use safe_cex_engine::write_plan::{Element, ElementResult, WritePlan};
    let callback = PhemexPerpetuals::unknown_outcome_telemetry().expect("venue opt-in");
    let unsafe_text =
        "https://venue.test/g-orders?api_key=sentinel&signature=sentinel body payload";
    let ids: Vec<_> = [unsafe_text.to_string(), "opaque-uuid".into()]
        .into_iter()
        .chain((1..=20).map(|id| id.to_string()))
        .collect();
    let mut error = VenueError::new(unsafe_text)
        .venue_message(unsafe_text)
        .outcome(Outcome::Unknown)
        .partial(&ids, 30, 1);
    let mut plan = WritePlan::new(
        (0..20)
            .map(|index| Element {
                index,
                client_order_id: Some(unsafe_text.into()),
                role: None,
            })
            .collect(),
    );
    for index in 0..20 {
        plan.record(
            index,
            ElementResult::Unknown {
                error: Box::new(error.clone()),
            },
        )
        .unwrap();
    }
    error = plan.attach(error);
    for status in [None, Some(99), Some(100), Some(599), Some(600)] {
        let event = callback(&EngineUnknownOutcome {
            now: 1.,
            command: u32::MAX,
            request: RequestId(u64::MAX),
            status,
            error: &error,
        });
        unknown_fields::assert_event(
            &event,
            u32::MAX,
            u64::MAX,
            status.filter(|s| (100..=599).contains(s)),
        );
        for key in ["placedOrderIds", "unknownIndices"] {
            let value = event
                .fields
                .iter()
                .find(|(k, _)| *k == key)
                .unwrap()
                .1
                .as_str()
                .unwrap();
            assert_eq!(value.split(',').count(), 16, "pin list cap: {key}");
        }
    }
    let mut plan = WritePlan::new(
        [65535, 65536]
            .into_iter()
            .map(|index| Element {
                index,
                client_order_id: None,
                role: None,
            })
            .collect(),
    );
    for index in [65535, 65536] {
        plan.record(
            index,
            ElementResult::Unknown {
                error: Box::new(VenueError::new(unsafe_text)),
            },
        )
        .unwrap();
    }
    let error = plan.attach(VenueError::new(unsafe_text));
    let event = callback(&EngineUnknownOutcome {
        now: 1.,
        command: 77,
        request: RequestId(1),
        status: None,
        error: &error,
    });
    unknown_fields::assert_event(&event, 77, 1, None);
    assert_eq!(
        event
            .fields
            .iter()
            .find(|(k, _)| *k == "unknownIndices")
            .unwrap()
            .1
            .as_str(),
        Some("65535")
    );
    let ids = (0..16).map(|id| format!("{id:032}")).collect::<Vec<_>>();
    let error = VenueError::new(unsafe_text).partial(&ids, 20, 65536);
    let event = callback(&EngineUnknownOutcome {
        now: 1.,
        command: 77,
        request: RequestId(0),
        status: None,
        error: &error,
    });
    unknown_fields::assert_event(&event, 77, 0, None);
    let ids = event
        .fields
        .iter()
        .find(|(key, _)| *key == "placedOrderIds")
        .unwrap()
        .1
        .as_str()
        .unwrap();
    assert_eq!(
        ids.split(',').count(),
        15,
        "whole IDs fit the engine's 512-byte field budget"
    );
    assert_eq!(
        event
            .fields
            .iter()
            .find(|(key, _)| *key == "unknownIndices")
            .unwrap()
            .1
            .as_str(),
        Some("")
    );
}

#[test]
fn unknown_hook_definite_refusal_control_emits_zero() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 77,
            orders: vec![opts("hook")],
        },
    );
    let request = writes(&actions)[0].clone();
    let actions = raw(
        &mut e,
        &request,
        r#"{"code":11001,"msg":"balance","data":null}"#,
    );
    assert_eq!(failure(&actions).outcome, Some(Outcome::Rejected));
    assert!(events(&actions).is_empty());
    assert!(writes(&actions).is_empty());
}

#[test]
fn unknown_hook_retains_native_prefix_for_engine_and_decoded_unknowns() {
    for status in [200, 503] {
        let mut e = engine();
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 77,
                orders: vec![opts("prefix"), opts("suffix")],
            },
        );
        let first = writes(&actions)[0].clone();
        let actions = respond(
            &mut e,
            &first,
            json!({"code":0,"data":rest_row("123", "prefix")}),
        );
        let second = writes(&actions)[0].clone();
        let actions = if status == 200 {
            raw(&mut e, &second, "malformed")
        } else {
            raw_status(&mut e, &second, status)
        };
        assert_eq!(
            failure(&actions).partial.as_ref().unwrap().order_ids,
            ["123"]
        );
        let diagnostics = events(&actions);
        assert_eq!(diagnostics.len(), 1);
        unknown_fields::assert_event(diagnostics[0], 77, second.id.0, Some(status));
        assert_eq!(
            diagnostics[0]
                .fields
                .iter()
                .find(|(k, _)| *k == "placedOrderIds")
                .unwrap()
                .1
                .as_str(),
            Some("123")
        );
        assert!(writes(&actions).is_empty());
    }
}
