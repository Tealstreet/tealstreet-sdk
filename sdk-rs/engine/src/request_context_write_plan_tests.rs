use super::*;
use crate::write_plan::{AppliedEffect, Element, ElementResult, WritePlan};

fn journal(count: usize) -> WritePlan {
    WritePlan::new(
        (0..count)
            .map(|index| Element {
                index,
                client_order_id: Some(format!("client-{index}")),
            })
            .collect(),
    )
}

fn released(writes: &mut CommandWrites, id: u64) {
    writes.register(RequestId(id), id).unwrap();
    writes.released(RequestId(id)).unwrap();
}

fn accepted(writes: &mut CommandWrites, id: u64, ids: &[&str]) {
    released(writes, id);
    writes
        .result(
            RequestId(id),
            WriteResult::Accepted {
                order_ids: ids.iter().map(|id| (*id).into()).collect(),
            },
        )
        .unwrap();
}

fn placed(id: &str) -> ElementResult {
    ElementResult::Placed {
        order_ids: vec![id.into()],
    }
}

fn refusal(code: &str) -> VenueError {
    VenueError::new("definite refusal")
        .venue_code(code)
        .venue_message("native refusal text")
        .outcome(Outcome::Rejected)
}

fn refuse(writes: &mut CommandWrites, plan: &mut WritePlan, id: u64, index: usize, code: &str) {
    let error = refusal(code);
    plan.record(
        index,
        ElementResult::NotSent {
            error: Some(Box::new(error.clone())),
        },
    )
    .unwrap();
    released(writes, id);
    writes
        .result(RequestId(id), WriteResult::Rejected(plan.attach(error)))
        .unwrap();
}

fn pending(plan: &mut WritePlan, index: usize) {
    plan.record(
        index,
        ElementResult::Unknown {
            error: Box::new(VenueError::new("pending").outcome(Outcome::Unknown)),
        },
    )
    .unwrap();
}

fn record_failure(writes: &mut CommandWrites, plan: &WritePlan, id: u64, gateway: bool) {
    released(writes, id);
    if gateway {
        writes
            .unproven_result(RequestId(id), 503, |evidence| evidence.write_plan(plan))
            .unwrap();
    } else {
        writes
            .result(
                RequestId(id),
                WriteResult::Failed(
                    plan.attach(
                        VenueError::new("transport disconnected").outcome(Outcome::Unknown),
                    ),
                ),
            )
            .unwrap();
    }
}

fn assert_projection(
    writes: &mut CommandWrites,
    expected: &WritePlan,
    fallback: VenueError,
) -> VenueError {
    let preview = writes.project_failure(fallback.clone()).unwrap();
    let error = writes.fail(fallback).unwrap();
    assert_eq!(error, preview);
    assert_eq!(error.elements.as_ref().unwrap().0, expected.outcomes());
    error
}

#[test]
fn identity_only_fallback_does_not_invent_partial_counts_after_an_applied_prefix() {
    for native_ids in [&[][..], &["native-prefix"][..]] {
        let mut writes = CommandWrites::new(3).unwrap();
        let mut plan = journal(3);
        plan.record(
            0,
            if native_ids.is_empty() {
                ElementResult::Applied {
                    effect: AppliedEffect::PositionTrigger,
                    target_order_id: None,
                    position_key: Some("BTCUSDT".into()),
                }
            } else {
                placed(native_ids[0])
            },
        )
        .unwrap();
        accepted(&mut writes, 1, native_ids);
        refuse(&mut writes, &mut plan, 2, 1, "margin");
        // A pending client identity is not a native order ID or caller-leg counts.
        let fallback = plan
            .attach(VenueError::new("later refusal").partial_ids(&["client-only-trigger".into()]));
        let error = assert_projection(&mut writes, &plan, fallback);
        assert_eq!(error.outcome, None);
        assert_eq!(error.venue_code.as_deref(), Some("margin"));
        if native_ids.is_empty() {
            assert!(error.partial.is_none(), "{error:?}");
            assert_eq!(error.legacy_text(), "definite refusal");
        } else {
            let partial = error.partial.as_ref().unwrap();
            assert_eq!(partial.order_ids, native_ids);
            assert_eq!((partial.total, partial.failed_index), (None, None));
        }
    }
}

#[test]
fn identity_only_fallback_preserves_selected_native_partial_counts() {
    let mut writes = CommandWrites::new(2).unwrap();
    let mut plan = journal(2);
    let native = refusal("margin").partial(&["native-prefix".into()], 2, 1);
    plan.record(0, placed("native-prefix")).unwrap();
    plan.record(
        1,
        ElementResult::NotSent {
            error: Some(Box::new(native.clone())),
        },
    )
    .unwrap();
    released(&mut writes, 1);
    writes
        .result(RequestId(1), WriteResult::KnownPartial(native))
        .unwrap();
    let fallback = plan.attach(VenueError::new("terminal").partial_ids(&["native-prefix".into()]));
    let error = assert_projection(&mut writes, &plan, fallback);
    let partial = error.partial.unwrap();
    assert_eq!(partial.order_ids, ["native-prefix"]);
    assert_eq!((partial.total, partial.failed_index), (Some(2), Some(1)));
}

#[test]
fn definite_refusal_then_unknown_keeps_every_fact() {
    for gateway in [false, true] {
        let mut writes = CommandWrites::new(4).unwrap();
        let mut plan = journal(4);
        plan.record(0, placed("prefix")).unwrap();
        accepted(&mut writes, 1, &["prefix"]);
        refuse(&mut writes, &mut plan, 2, 1, "100400");
        pending(&mut plan, 2);
        record_failure(&mut writes, &plan, 3, gateway);
        if gateway {
            plan.record(
                2,
                ElementResult::Unknown {
                    error: Box::new(ResponseProof::from_status(503).error().unwrap()),
                },
            )
            .unwrap();
        }
        let error = assert_projection(&mut writes, &plan, VenueError::new("terminal fallback"));
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(error.venue_code.as_deref(), Some("100400"));
        assert_eq!(error.venue_message.as_deref(), Some("native refusal text"));
        assert_eq!(error.partial.unwrap().order_ids, ["prefix"]);
        assert_eq!(
            error.message,
            if gateway {
                UNKNOWN_AFTER_REFUSAL
            } else {
                "definite refusal"
            }
        );
    }
}

#[test]
fn held_restore_journals_the_cancel_refusal_and_restore_attempt() {
    for gateway in [true, false] {
        let mut writes = CommandWrites::new(3).unwrap();
        let mut plan = journal(3);
        plan.record(
            0,
            ElementResult::Applied {
                effect: AppliedEffect::Cancel,
                target_order_id: Some("old-stop".into()),
                position_key: None,
            },
        )
        .unwrap();
        accepted(&mut writes, 1, &[]);
        refuse(&mut writes, &mut plan, 2, 1, "100400");
        let fallback = if gateway {
            pending(&mut plan, 2);
            record_failure(&mut writes, &plan, 3, true);
            plan.record(
                2,
                ElementResult::Unknown {
                    error: Box::new(ResponseProof::from_status(503).error().unwrap()),
                },
            )
            .unwrap();
            VenueError::new("HTTP 503")
        } else {
            plan.record(2, placed("restored")).unwrap();
            accepted(&mut writes, 3, &["restored"]);
            let mut fallback = VenueError::new("protection restored");
            fallback.protection = Some(crate::error::Protection::Restored);
            plan.attach(fallback)
        };
        let error = assert_projection(&mut writes, &plan, fallback);
        assert_eq!(error.venue_code.as_deref(), Some("100400"));
        assert_eq!(error.venue_message.as_deref(), Some("native refusal text"));
        if gateway {
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert_eq!(error.message, UNKNOWN_AFTER_REFUSAL);
        } else {
            assert_eq!(error.outcome, None);
            assert_eq!(error.protection, Some(crate::error::Protection::Restored));
            assert_eq!(error.partial.unwrap().order_ids, ["restored"]);
        }
    }
}

#[cfg(feature = "http-plans")]
#[test]
fn batch_refusal_then_single_refusal_acceptance_and_http_503_keeps_latest_journal() {
    let mut writes = CommandWrites::new(5).unwrap();
    let mut plan = journal(4);
    released(&mut writes, 1);
    writes
        .result(RequestId(1), WriteResult::Rejected(refusal("10001")))
        .unwrap();
    refuse(&mut writes, &mut plan, 2, 0, "110007");
    plan.record(1, placed("single-1")).unwrap();
    accepted(&mut writes, 3, &["single-1"]);
    pending(&mut plan, 2);
    record_failure(&mut writes, &plan, 4, true);
    plan.record(
        2,
        ElementResult::Unknown {
            error: Box::new(ResponseProof::from_status(503).error().unwrap()),
        },
    )
    .unwrap();
    // This is the seal used by fail_plan_owned after unproven HTTP evidence.
    let prepared = writes
        .prepare_seal(ResponseProof::from_status(503).error(), None)
        .unwrap();
    let error = writes.commit_seal(prepared).unwrap();
    assert_eq!(error.elements.as_ref().unwrap().0, plan.outcomes());
    assert_eq!(error.venue_code.as_deref(), Some("10001"));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["single-1"]);
    assert_eq!(error.message, UNKNOWN_AFTER_REFUSAL);
    assert_eq!(writes.attempts.len(), 4, "single 3 was never sent");
}

#[test]
fn latest_journal_follows_callback_completion_not_registration() {
    for attached in [true, false] {
        for order in [[1, 2, 3], [3, 1, 2]] {
            let mut writes = CommandWrites::new(3).unwrap();
            let mut plan = journal(3);
            for id in 1..=3 {
                released(&mut writes, id);
                pending(&mut plan, (id - 1) as usize);
            }
            for id in order {
                let result = if id == 1 {
                    plan.record(0, placed("accepted")).unwrap();
                    WriteResult::Accepted {
                        order_ids: vec!["accepted".into()],
                    }
                } else {
                    let error = VenueError::new(format!("refusal-{id}")).outcome(Outcome::Rejected);
                    plan.record(
                        (id - 1) as usize,
                        ElementResult::NotSent {
                            error: Some(Box::new(error.clone())),
                        },
                    )
                    .unwrap();
                    WriteResult::Rejected(plan.attach(error))
                };
                writes.result(RequestId(id), result).unwrap();
            }
            let fallback = VenueError::new("terminal fallback");
            let fallback = if attached {
                plan.attach(fallback)
            } else {
                fallback
            };
            let error = assert_projection(&mut writes, &plan, fallback);
            assert_eq!(error.message, "refusal-2");
            assert_eq!(error.outcome, None);
            assert_eq!(error.partial.unwrap().order_ids, ["accepted"]);
        }
    }
}

#[test]
fn latest_unproven_journal_follows_callback_completion_not_registration() {
    for order in [[1, 2, 3], [3, 1, 2]] {
        let mut writes = CommandWrites::new(3).unwrap();
        let mut plan = journal(3);
        for id in 1..=3 {
            released(&mut writes, id);
            pending(&mut plan, (id - 1) as usize);
        }
        for id in order {
            if id == 1 {
                plan.record(0, placed("accepted")).unwrap();
                writes
                    .result(
                        RequestId(id),
                        WriteResult::Accepted {
                            order_ids: vec!["accepted".into()],
                        },
                    )
                    .unwrap();
            } else {
                plan.record(
                    (id - 1) as usize,
                    ElementResult::Unknown {
                        error: Box::new(ResponseProof::from_status(503).error().unwrap()),
                    },
                )
                .unwrap();
                writes
                    .unproven_result(RequestId(id), 503, |evidence| evidence.write_plan(&plan))
                    .unwrap();
            }
        }
        let error = assert_projection(&mut writes, &plan, VenueError::new("terminal fallback"));
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(error.partial.unwrap().order_ids, ["accepted"]);
    }
}
