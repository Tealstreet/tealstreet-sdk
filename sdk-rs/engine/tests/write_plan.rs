use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, PlannedWrite, WritePlan};
use safe_cex_engine::{ErrorKind, Outcome, VenueError};

fn writes(values: &[u32]) -> Vec<PlannedWrite<u32>> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| PlannedWrite {
            payload: *value,
            elements: vec![Element {
                index,
                client_order_id: Some(format!("client-{index}")),
            }],
        })
        .collect()
}

fn validate(value: &u32) -> Result<(), VenueError> {
    if *value == 0 {
        Err(VenueError::new("invalid quantity").kind(ErrorKind::InvalidInput))
    } else {
        Ok(())
    }
}

fn placed(index: usize) -> ElementResult {
    ElementResult::Placed {
        order_ids: vec![format!("native-{index}")],
    }
}

#[test]
fn invalid_last_element_refuses_before_any_transport_write() {
    let mut requests = Vec::new();
    let error = match WritePlan::validate(writes(&[1, 2, 0]), validate) {
        Ok(mut plan) => {
            while let Some((index, write)) = plan.next_write() {
                requests.push(write.payload);
                plan.settle(index, vec![placed(index)]).unwrap();
            }
            panic!("invalid plan was released")
        }
        Err(error) => error,
    };
    assert!(requests.is_empty());
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    let elements = error.elements.unwrap();
    assert_eq!(elements.len(), 3);
    assert!(
        elements
            .iter()
            .all(|e| matches!(e.result, ElementResult::NotSent { .. }))
    );
    assert!(
        matches!(&elements[2].result, ElementResult::NotSent { error: Some(e) }
        if e.kind == Some(ErrorKind::InvalidInput))
    );
}

fn runtime_failure(outcome: Outcome) {
    let mut plan = WritePlan::validate(writes(&[1, 2, 3]), validate).unwrap();
    let mut requests = Vec::new();
    while let Some((index, write)) = plan.next_write() {
        requests.push(write.payload);
        if index == 1 {
            plan.fail(index, VenueError::new("scripted failure").outcome(outcome))
                .unwrap();
        } else {
            plan.settle(index, vec![placed(index)]).unwrap();
        }
    }
    assert_eq!(requests, [1, 2]); // No suffix, resend, cancel or rollback.
    assert!(plan.next_write().is_none());
    let error = plan.failure(VenueError::new("command stopped").outcome(outcome));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["native-0"]);
    let elements = error.elements.unwrap();
    assert_eq!(elements[0].result, placed(0));
    match outcome {
        Outcome::Rejected => assert!(matches!(&elements[1].result,
            ElementResult::NotSent { error: Some(e) } if e.outcome == Some(Outcome::Rejected))),
        _ => assert!(matches!(&elements[1].result,
            ElementResult::Unknown { error } if error.outcome == Some(Outcome::Unknown))),
    }
    assert_eq!(elements[2].result, ElementResult::NotSent { error: None });
    assert_eq!(
        elements.iter().map(|e| e.element.index).collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn rejected_second_element_reports_prefix_failure_and_unsent_suffix() {
    runtime_failure(Outcome::Rejected);
}

#[test]
fn ambiguous_second_element_stays_unknown_and_is_never_resent() {
    runtime_failure(Outcome::Unknown);
}

#[test]
fn reporting_observer_keeps_legacy_continuation_separate_from_dispatch_policy()
-> Result<(), Box<dyn std::error::Error>> {
    for outcome in [Outcome::Rejected, Outcome::Unknown] {
        let mut plan = WritePlan::validate(writes(&[1, 2, 3]), validate)?;
        let mut observed = vec![];
        while let Some((index, write)) = plan.observe_next_write() {
            observed.push(write.payload);
            assert!(
                plan.observe_next_write().is_none(),
                "pending cannot be observed twice"
            );
            if index == 1 {
                plan.fail(
                    index,
                    VenueError::new("legacy caught failure").outcome(outcome),
                )?;
                assert!(plan.next_write().is_none(), "dispatch policy still stops");
            } else {
                plan.settle(index, vec![placed(index)])?;
            }
        }
        assert_eq!(observed, [1, 2, 3]);
        assert!(plan.is_complete());
        assert!(plan.next_write().is_none());
        let error = plan.failure(VenueError::new("reported after legacy loop completed"));
        let elements = error.elements.as_ref().ok_or("missing journal")?;
        assert_eq!(elements.len(), 3);
        assert_eq!(elements[0].result, placed(0));
        assert_eq!(elements[2].result, placed(2));
        assert_eq!(
            error.partial.as_ref().ok_or("missing IDs")?.order_ids,
            ["native-0", "native-2"]
        );
    }
    Ok(())
}

#[test]
fn all_valid_control_releases_each_write_exactly_once() {
    let mut plan = WritePlan::validate(writes(&[1, 2, 3]), validate).unwrap();
    let mut requests = Vec::new();
    while let Some((index, write)) = plan.next_write() {
        requests.push(write.payload);
        plan.settle(index, vec![placed(index)]).unwrap();
    }
    assert_eq!(requests, [1, 2, 3]);
    assert_eq!(
        plan.outcomes()
            .iter()
            .map(|e| e.result.clone())
            .collect::<Vec<_>>(),
        [placed(0), placed(1), placed(2)]
    );
}

#[test]
fn batch_results_preserve_each_row_and_stop_following_writes() {
    let mut plan = WritePlan::validate(
        vec![
            PlannedWrite {
                payload: 1,
                elements: vec![
                    Element {
                        index: 0,
                        client_order_id: None,
                    },
                    Element {
                        index: 1,
                        client_order_id: None,
                    },
                ],
            },
            PlannedWrite {
                payload: 2,
                elements: vec![Element {
                    index: 2,
                    client_order_id: None,
                }],
            },
        ],
        validate,
    )
    .unwrap();
    assert_eq!(plan.next_write().unwrap().0, 0);
    assert!(plan.next_write().is_none());
    plan.settle(
        0,
        vec![
            placed(0),
            ElementResult::NotSent {
                error: Some(Box::new(
                    VenueError::new("row rejected").outcome(Outcome::Rejected),
                )),
            },
        ],
    )
    .unwrap();
    assert!(plan.next_write().is_none());
    assert_eq!(plan.outcomes()[0].result, placed(0));
    assert_eq!(
        plan.outcomes()[2].result,
        ElementResult::NotSent { error: None }
    );
}

#[test]
fn invalid_settlement_is_atomic_and_cannot_release_a_later_write() {
    let mut plan = WritePlan::validate(writes(&[1, 2]), validate).unwrap();
    plan.next_write().unwrap();
    assert!(plan.settle(1, vec![placed(0)]).is_err());
    assert!(plan.settle(0, vec![]).is_err());
    assert!(
        plan.settle(0, vec![ElementResult::Placed { order_ids: vec![] }])
            .is_err()
    );
    assert!(plan.next_write().is_none());
    assert!(matches!(
        plan.outcomes()[0].result,
        ElementResult::Unknown { .. }
    ));
    plan.settle(0, vec![placed(0)]).unwrap();
    assert!(plan.settle(0, vec![placed(0)]).is_err());
    assert_eq!(plan.next_write().unwrap().0, 1);
}

#[test]
fn unresolved_write_and_unclassified_failure_are_unknown() {
    let mut plan = WritePlan::validate(writes(&[1, 2]), validate).unwrap();
    plan.next_write().unwrap();
    let error = plan.failure(VenueError::new("interrupted").outcome(Outcome::NotSent));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(matches!(
        error.elements.unwrap()[0].result,
        ElementResult::Unknown { .. }
    ));
    assert!(plan.next_write().is_none());

    let mut plan = WritePlan::validate(writes(&[1, 2]), validate).unwrap();
    plan.next_write().unwrap();
    plan.fail(0, VenueError::new("connection reset")).unwrap();
    assert!(matches!(
        plan.outcomes()[0].result,
        ElementResult::Unknown { .. }
    ));
    assert!(plan.next_write().is_none());
}

#[test]
fn malformed_element_mapping_is_refused_without_dispatch() {
    for elements in [
        vec![],
        vec![Element {
            index: 9,
            client_order_id: None,
        }],
    ] {
        let error = WritePlan::validate(
            vec![PlannedWrite {
                payload: 1,
                elements,
            }],
            validate,
        )
        .unwrap_err();
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }
    let mut writes = writes(&[1, 2]);
    writes[1].elements[0].index = 0;
    assert!(WritePlan::validate(writes, validate).is_err());
}

#[test]
fn error_serialization_exports_indexed_outcomes_without_changing_plain_errors() {
    let plain = serde_json::to_value(VenueError::new("plain")).unwrap();
    assert!(plain.get("elements").is_none());
    let mut plan = WritePlan::validate(writes(&[1, 2, 3]), validate).unwrap();
    plan.next_write().unwrap();
    plan.settle(0, vec![placed(0)]).unwrap();
    plan.next_write().unwrap();
    plan.fail(
        1,
        VenueError::new("rejected")
            .venue_code("-1013")
            .outcome(Outcome::Rejected),
    )
    .unwrap();
    let encoded = serde_json::to_value(plan.failure(VenueError::new("stopped"))).unwrap();
    assert_eq!(
        encoded["elements"][0],
        serde_json::json!({
            "index": 0, "clientOrderId": "client-0", "status": "placed", "orderIds": ["native-0"]
        })
    );
    assert_eq!(encoded["elements"][1]["status"], "notSent");
    assert_eq!(encoded["elements"][1]["error"]["outcome"], "rejected");
    assert_eq!(encoded["elements"][1]["error"]["venueCode"], "-1013");
    assert_eq!(
        encoded["elements"][2],
        serde_json::json!({
            "index": 2, "clientOrderId": "client-2", "status": "notSent"
        })
    );
}

#[test]
fn validation_visits_the_complete_plan_even_when_the_first_write_is_invalid() {
    let visited = std::cell::RefCell::new(Vec::new());
    let error = WritePlan::validate(writes(&[0, 2, 0]), |value| {
        visited.borrow_mut().push(*value);
        validate(value)
    })
    .unwrap_err();
    assert_eq!(*visited.borrow(), [0, 2, 0]);
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

#[test]
fn unknown_classification_cannot_hide_inside_a_not_sent_row() {
    let mut plan = WritePlan::validate(writes(&[1, 2]), validate).unwrap();
    plan.next_write().unwrap();
    plan.settle(
        0,
        vec![ElementResult::NotSent {
            error: Some(Box::new(
                VenueError::new("lost acknowledgement").outcome(Outcome::Unknown),
            )),
        }],
    )
    .unwrap();
    assert!(matches!(
        plan.outcomes()[0].result,
        ElementResult::Unknown { .. }
    ));
    assert!(plan.next_write().is_none());
}

#[test]
fn lifecycle_preserves_complete_plan_journal_when_selecting_a_native_failure() {
    use safe_cex_engine::RequestId;
    use safe_cex_engine::request_context::{CommandWrites, WriteResult};
    let mut plan = WritePlan::validate(writes(&[1, 2, 3]), validate).unwrap();
    plan.next_write().unwrap();
    plan.settle(0, vec![placed(0)]).unwrap();
    plan.next_write().unwrap();
    let rejection = VenueError::new("native refusal").outcome(Outcome::Rejected);
    plan.fail(1, rejection.clone()).unwrap();
    let mut lifecycle = CommandWrites::new(3).unwrap();
    lifecycle.register(RequestId(1), 0).unwrap();
    lifecycle.released(RequestId(1)).unwrap();
    lifecycle
        .result(
            RequestId(1),
            WriteResult::Accepted {
                order_ids: vec!["native-0".into()],
            },
        )
        .unwrap();
    lifecycle.register(RequestId(2), 1).unwrap();
    lifecycle.released(RequestId(2)).unwrap();
    lifecycle
        .result(RequestId(2), WriteResult::Rejected(rejection))
        .unwrap();
    let error = lifecycle
        .fail(plan.failure(VenueError::new("plan stopped")))
        .unwrap();
    assert_eq!(error.message, "native refusal");
    assert_eq!(error.elements.unwrap().0, plan.outcomes());
    assert_eq!(error.partial.unwrap().order_ids, ["native-0"]);
    assert_ne!(error.outcome, Some(Outcome::NotSent));
    assert_ne!(error.outcome, Some(Outcome::Rejected));
}

#[test]
fn terminal_reporting_preserves_existing_native_reconciliation_ids() {
    let mut plan = WritePlan::validate(writes(&[1, 2]), validate).unwrap();
    plan.next_write().unwrap();
    plan.settle(0, vec![placed(0)]).unwrap();
    let error = plan.failure(
        VenueError::new("unmapped native evidence")
            .partial_ids(&["unmapped".into(), "native-0".into()]),
    );
    assert_eq!(error.partial.unwrap().order_ids, ["unmapped", "native-0"]);
    assert!(plan.next_write().is_none());
}

#[test]
fn terminal_failure_closes_an_unissued_plan_and_preserves_not_sent() {
    let mut plan = WritePlan::validate(writes(&[1, 2]), validate).unwrap();
    let error = plan.failure(VenueError::new("local admission refused").outcome(Outcome::NotSent));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(plan.next_write().is_none());
    assert!(
        plan.outcomes()
            .iter()
            .all(|e| matches!(e.result, ElementResult::NotSent { .. }))
    );
}

#[test]
fn serialized_fixture_matches_the_wasm_declaration_contract() {
    let element = |index, id: Option<&str>| Element {
        index,
        client_order_id: id.map(str::to_owned),
    };
    let mut plan = WritePlan::validate(
        vec![
            PlannedWrite {
                payload: 1,
                elements: vec![
                    element(0, Some("first")),
                    element(1, Some("cancel")),
                    element(2, None),
                    element(3, Some("deferred")),
                ],
            },
            PlannedWrite {
                payload: 2,
                elements: vec![element(4, Some("second")), element(5, Some("third"))],
            },
            PlannedWrite {
                payload: 3,
                elements: vec![element(6, None)],
            },
        ],
        validate,
    )
    .unwrap();
    plan.next_write().unwrap();
    plan.settle(
        0,
        vec![
            placed(0),
            ElementResult::Applied {
                effect: AppliedEffect::Cancel,
                target_order_id: Some("old-order".into()),
                position_key: None,
            },
            ElementResult::Applied {
                effect: AppliedEffect::Settings,
                target_order_id: None,
                position_key: Some("BTCUSD:long".into()),
            },
            ElementResult::Applied {
                effect: AppliedEffect::DeferredChild,
                target_order_id: None,
                position_key: None,
            },
        ],
    )
    .unwrap();
    plan.next_write().unwrap();
    plan.settle(
        1,
        vec![
            ElementResult::NotSent {
                error: Some(Box::new(
                    VenueError::new("rejected")
                        .venue_code("-1013")
                        .outcome(Outcome::Rejected),
                )),
            },
            ElementResult::Unknown {
                error: Box::new(VenueError::new("lost acknowledgement").outcome(Outcome::Unknown)),
            },
        ],
    )
    .unwrap();
    let actual = serde_json::to_value(plan.failure(VenueError::new("stopped").partial(
        &["native-0".into()],
        3,
        1,
    )))
    .unwrap();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/write-plan-error.json")).unwrap();
    assert_eq!(actual, fixture);
}

#[test]
fn unattributed_partial_acceptance_keeps_dispatched_rows_unknown() {
    let mut batch = writes(&[1, 2]);
    batch[0].elements.push(Element {
        index: 1,
        client_order_id: Some("second".into()),
    });
    batch[1].elements[0].index = 2;
    let mut plan = WritePlan::validate(batch, validate).unwrap();
    plan.next_write().unwrap();
    let native = VenueError::new("partial batch refusal")
        .outcome(Outcome::Rejected)
        .partial_ids(&["accepted-native".into()]);
    plan.fail(0, native.clone()).unwrap();
    let error = plan.failure(native);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["accepted-native"]);
    let rows = error.elements.unwrap();
    for row in &rows[..2] {
        assert!(matches!(&row.result, ElementResult::Unknown { error }
            if error.outcome == Some(Outcome::Unknown)));
    }
    assert_eq!(rows[2].result, ElementResult::NotSent { error: None });
    assert!(plan.next_write().is_none());
}

#[test]
fn attributed_partial_batch_preserves_two_acceptances_and_one_refusal() {
    let mut batch = writes(&[1, 2]);
    batch[0].elements.extend([
        Element {
            index: 1,
            client_order_id: Some("second".into()),
        },
        Element {
            index: 2,
            client_order_id: Some("third".into()),
        },
    ]);
    batch[1].elements[0].index = 3;
    let mut plan = WritePlan::validate(batch, validate).unwrap();
    plan.next_write().unwrap();
    let refusal = VenueError::new("row 3 refused").outcome(Outcome::Rejected);
    plan.settle(
        0,
        vec![
            placed(0),
            placed(1),
            ElementResult::NotSent {
                error: Some(Box::new(refusal.clone())),
            },
        ],
    )
    .unwrap();
    let error = plan.failure(refusal.partial_ids(&["native-0".into(), "native-1".into()]));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["native-0", "native-1"]);
    let rows = error.elements.unwrap();
    assert_eq!(rows[0].result, placed(0));
    assert_eq!(rows[1].result, placed(1));
    assert!(
        matches!(&rows[2].result, ElementResult::NotSent { error: Some(e) }
        if e.outcome == Some(Outcome::Rejected))
    );
    assert_eq!(rows[3].result, ElementResult::NotSent { error: None });
    assert!(plan.next_write().is_none());
}

#[test]
fn failed_write_ids_and_native_partial_metadata_reach_the_legacy_result() {
    let mut plan = WritePlan::validate(writes(&[1, 2, 3]), validate).unwrap();
    plan.next_write().unwrap();
    plan.settle(0, vec![placed(0)]).unwrap();
    plan.next_write().unwrap();
    plan.fail(
        1,
        VenueError::new("partial acknowledgement")
            .outcome(Outcome::Rejected)
            .partial(&["accepted-native".into()], 2, 1),
    )
    .unwrap();
    let error = plan.failure(VenueError::new("stopped").outcome(Outcome::NotSent));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    let partial = error.partial.unwrap();
    assert_eq!(partial.order_ids, ["native-0", "accepted-native"]);
    assert_eq!((partial.total, partial.failed_index), (Some(2), Some(1)));
}

#[test]
fn terminal_partial_metadata_is_preserved_when_merging_a_placed_prefix() {
    let mut plan = WritePlan::validate(writes(&[1, 2]), validate).unwrap();
    plan.next_write().unwrap();
    plan.settle(0, vec![placed(0)]).unwrap();
    let error = plan.failure(VenueError::new("stopped").partial(&["native-1".into()], 3, 1));
    let partial = error.partial.unwrap();
    assert_eq!(partial.order_ids, ["native-1", "native-0"]);
    assert_eq!((partial.total, partial.failed_index), (Some(3), Some(1)));
}

#[test]
fn venue_error_keeps_the_pre_journal_native_layout_budget() {
    assert!(std::mem::size_of::<VenueError>() <= 112);
}

#[test]
fn applied_effects_continue_without_manufacturing_placed_identities() {
    for effect in [
        AppliedEffect::Cancel,
        AppliedEffect::Amend,
        AppliedEffect::TradingStop,
        AppliedEffect::Settings,
        AppliedEffect::DeferredChild,
        AppliedEffect::PositionTrigger,
    ] {
        let mut plan = WritePlan::validate(writes(&[1, 2, 3]), validate).unwrap();
        plan.next_write().unwrap();
        let applied = ElementResult::Applied {
            effect,
            target_order_id: Some("existing-order".into()),
            position_key: None,
        };
        plan.settle(0, vec![applied.clone()]).unwrap();
        assert_eq!(plan.next_write().unwrap().0, 1);
        plan.fail(1, VenueError::new("refused").outcome(Outcome::Rejected))
            .unwrap();
        let error = plan.failure(VenueError::new("stopped").outcome(Outcome::Rejected));
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert!(
            error.partial.is_none(),
            "applied targets are never newly placed IDs"
        );
        assert_eq!(error.elements.unwrap()[0].result, applied);
        assert!(plan.next_write().is_none());
    }
}

#[test]
fn empty_applied_targets_and_placed_ids_are_rejected_atomically() {
    let mut plan = WritePlan::validate(writes(&[1, 2]), validate).unwrap();
    plan.next_write().unwrap();
    for invalid in [
        ElementResult::Applied {
            effect: AppliedEffect::Cancel,
            target_order_id: Some(String::new()),
            position_key: None,
        },
        ElementResult::Applied {
            effect: AppliedEffect::Settings,
            target_order_id: None,
            position_key: Some(String::new()),
        },
        ElementResult::Placed { order_ids: vec![] },
        ElementResult::Placed {
            order_ids: vec![String::new()],
        },
    ] {
        assert!(plan.settle(0, vec![invalid]).is_err());
        assert!(plan.next_write().is_none());
        assert!(matches!(
            plan.outcomes()[0].result,
            ElementResult::Unknown { .. }
        ));
    }
    plan.settle(
        0,
        vec![ElementResult::Applied {
            effect: AppliedEffect::DeferredChild,
            target_order_id: None,
            position_key: None,
        }],
    )
    .unwrap();
    assert_eq!(plan.next_write().unwrap().0, 1);
}
