use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, PlanError, WritePlan};
use safe_cex_engine::{Outcome, VenueError};

fn plan(count: usize) -> WritePlan {
    WritePlan::new(
        (0..count)
            .map(|index| Element {
                index,
                client_order_id: Some(format!("client-{index}")),
            })
            .collect(),
    )
}

fn placed(index: usize) -> ElementResult {
    ElementResult::Placed {
        order_ids: vec![format!("native-{index}")],
    }
}

fn refused() -> ElementResult {
    ElementResult::NotSent {
        error: Some(Box::new(
            VenueError::new("refused").outcome(Outcome::Rejected),
        )),
    }
}

#[test]
fn reporting_allows_the_venue_to_continue_after_a_middle_refusal() {
    let mut plan = plan(3);
    let mut requests = Vec::new();
    for index in 0..3 {
        requests.push(index);
        plan.record(index, if index == 1 { refused() } else { placed(index) })
            .unwrap();
    }
    assert_eq!(requests, [0, 1, 2]);
    assert_eq!(plan.outcomes()[1].result, refused());
    assert_eq!(plan.outcomes()[2].result, placed(2));
}

#[test]
fn reporting_preserves_the_venues_aggregate_classification() {
    for outcome in [
        None,
        Some(Outcome::NotSent),
        Some(Outcome::Rejected),
        Some(Outcome::Unknown),
    ] {
        let mut plan = plan(2);
        plan.record(0, placed(0)).unwrap();
        let mut native = VenueError::new("native refusal").partial(&["native-owned".into()], 9, 4);
        native.outcome = outcome;
        let mut error = plan.attach(native.clone());
        assert_eq!(error.elements.take().unwrap().0, plan.outcomes());
        assert_eq!(error, native);
    }
}

#[test]
fn the_venue_can_stop_with_a_not_sent_suffix() {
    let mut plan = plan(3);
    plan.record(0, placed(0)).unwrap();
    plan.record(1, refused()).unwrap();
    assert_eq!(
        plan.outcomes()[2].result,
        ElementResult::NotSent { error: None }
    );
}

#[test]
fn attaching_a_failure_does_not_close_the_journal() {
    let mut plan = plan(3);
    plan.record(0, placed(0)).unwrap();
    plan.record(1, refused()).unwrap();
    let snapshot = plan.attach(VenueError::new("intermediate refusal"));
    plan.record(2, placed(2)).unwrap();
    assert_eq!(plan.outcomes()[2].result, placed(2));
    assert_eq!(
        snapshot.elements.unwrap()[2].result,
        ElementResult::NotSent { error: None }
    );
}

#[test]
fn recorded_facts_are_not_reclassified_or_merged_into_native_ids() {
    let mut plan = plan(3);
    let partial = VenueError::new("native partial")
        .outcome(Outcome::Unknown)
        .partial(&["native-accepted".into()], 3, 1);
    let unknown = ElementResult::Unknown {
        error: Box::new(partial.clone()),
    };
    plan.record(0, placed(0)).unwrap();
    plan.record(1, unknown.clone()).unwrap();
    plan.record(2, placed(2)).unwrap();
    let error = plan.attach(partial.clone());
    assert_eq!(error.partial, partial.partial);
    assert_eq!(error.outcome, partial.outcome);
    assert_eq!(error.elements.unwrap()[1].result, unknown);
}

#[test]
fn mixed_batch_facts_keep_two_acceptances_one_refusal_and_a_later_acceptance() {
    let mut plan = plan(4);
    for (index, result) in [placed(0), placed(1), refused(), placed(3)]
        .into_iter()
        .enumerate()
    {
        plan.record(index, result).unwrap();
    }
    let rows = plan.outcomes();
    assert_eq!(rows[0].result, placed(0));
    assert_eq!(rows[1].result, placed(1));
    assert_eq!(rows[2].result, refused());
    assert_eq!(rows[3].result, placed(3));
}

#[test]
fn native_pending_then_accepted_facts_can_arrive_out_of_order() {
    let mut plan = plan(3);
    plan.record(
        1,
        ElementResult::Unknown {
            error: Box::new(VenueError::new("pending").outcome(Outcome::Unknown)),
        },
    )
    .unwrap();
    plan.record(2, placed(2)).unwrap();
    plan.record(0, placed(0)).unwrap();
    plan.record(1, placed(1)).unwrap();
    let rows = plan.outcomes();
    assert_eq!(
        rows.iter().map(|row| row.element.index).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(
        rows.into_iter().map(|row| row.result).collect::<Vec<_>>(),
        [placed(0), placed(1), placed(2)]
    );
}

#[test]
fn construction_does_not_validate_or_refuse_a_venue_plan() {
    assert!(WritePlan::new(vec![]).outcomes().is_empty());
    let mut plan = WritePlan::new(vec![Element {
        index: 9,
        client_order_id: None,
    }]);
    plan.record(9, placed(9)).unwrap();
    assert_eq!(plan.outcomes()[0].element.index, 9);
}

#[test]
fn an_unknown_reporting_index_changes_no_other_facts() {
    let mut plan = plan(2);
    let before = plan.outcomes();
    assert_eq!(plan.record(9, placed(9)), Err(PlanError::UnknownElement));
    assert_eq!(plan.outcomes(), before);
    plan.record(1, placed(1)).unwrap();
    assert_eq!(plan.outcomes()[1].result, placed(1));
}

#[test]
fn all_applied_effects_report_existing_targets_without_new_ids() {
    for effect in [
        AppliedEffect::Cancel,
        AppliedEffect::Amend,
        AppliedEffect::TradingStop,
        AppliedEffect::Settings,
        AppliedEffect::DeferredChild,
        AppliedEffect::PositionTrigger,
    ] {
        let mut plan = plan(2);
        plan.record(
            0,
            ElementResult::Applied {
                effect,
                target_order_id: Some("existing-order".into()),
                position_key: None,
            },
        )
        .unwrap();
        plan.record(1, placed(1)).unwrap();
        let encoded = serde_json::to_value(plan.outcomes()).unwrap();
        assert_eq!(encoded[0]["status"], "applied");
        assert_eq!(encoded[0]["targetOrderId"], "existing-order");
        assert!(encoded[0].get("orderIds").is_none());
    }
}

#[test]
fn lifecycle_retains_recorded_elements_without_changing_physical_classification() {
    use safe_cex_engine::RequestId;
    use safe_cex_engine::request_context::{CommandWrites, WriteResult};
    let mut plan = plan(3);
    plan.record(0, placed(0)).unwrap();
    plan.record(1, refused()).unwrap();
    let lifecycle = || {
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
            .result(
                RequestId(2),
                WriteResult::Rejected(VenueError::new("native refusal").outcome(Outcome::Rejected)),
            )
            .unwrap();
        lifecycle
    };
    let without = lifecycle().fail(VenueError::new("fallback")).unwrap();
    let mut with = lifecycle()
        .fail(plan.attach(VenueError::new("fallback")))
        .unwrap();
    assert_eq!(with.elements.take().unwrap().0, plan.outcomes());
    assert_eq!(with, without);
}

#[test]
fn serialized_fixture_is_byte_identical_to_the_applied_contract() {
    let ids = [
        Some("first"),
        Some("cancel"),
        None,
        Some("deferred"),
        Some("second"),
        Some("third"),
        None,
    ];
    let mut plan = WritePlan::new(
        ids.into_iter()
            .enumerate()
            .map(|(index, id)| Element {
                index,
                client_order_id: id.map(str::to_owned),
            })
            .collect(),
    );
    for (index, result) in [
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
    ]
    .into_iter()
    .enumerate()
    {
        plan.record(index, result).unwrap();
    }
    let native = VenueError::new("stopped")
        .outcome(Outcome::Unknown)
        .partial(&["native-0".into()], 3, 1);
    let actual = format!(
        "{}\n",
        serde_json::to_string_pretty(&plan.attach(native)).unwrap()
    );
    assert_eq!(actual, include_str!("fixtures/write-plan-error.json"));
}

#[test]
fn plain_errors_omit_elements_and_keep_the_native_layout_budget() {
    assert!(
        serde_json::to_value(VenueError::new("plain"))
            .unwrap()
            .get("elements")
            .is_none()
    );
    assert!(std::mem::size_of::<VenueError>() <= 112);
}
