use super::*;

fn accepted(ids: Vec<String>) -> WriteResult {
    WriteResult::Accepted { order_ids: ids }
}
fn released(writes: &mut CommandWrites, id: u64) {
    writes.register(RequestId(id), id).unwrap();
    writes.released(RequestId(id)).unwrap();
}

#[test]
fn id_count_overflow_preserves_exact_prefix_and_never_seals_success() {
    let mut writes = CommandWrites::new(2).unwrap();
    released(&mut writes, 1);
    let ids: Vec<_> = (0..4_100).map(|id| format!("native-{id}")).collect();
    writes.result(RequestId(1), accepted(ids.clone())).unwrap();
    assert_eq!(writes.succeed(), Err(Error::State));
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(error.message.contains("truncated"));
    let partial = error.partial.unwrap();
    assert!(!partial.order_ids.is_empty());
    assert!(partial.order_ids.len() <= 4_096);
    assert_eq!(partial.order_ids, ids[..partial.order_ids.len()]);
}

#[test]
fn oversized_identifier_is_never_rewritten_and_prior_ids_survive() {
    let mut writes = CommandWrites::new(2).unwrap();
    released(&mut writes, 1);
    writes
        .result(RequestId(1), accepted(vec!["known".into()]))
        .unwrap();
    released(&mut writes, 2);
    writes
        .result(
            RequestId(2),
            accepted(vec!["next".into(), "x".repeat(1_025), "unretained".into()]),
        )
        .unwrap();
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["known", "next"]);
}

#[test]
fn id_bytes_and_duplicate_entries_consume_one_command_budget() {
    let mut writes = CommandWrites::new(2).unwrap();
    for id in [1, 2] {
        released(&mut writes, id);
        writes
            .result(RequestId(id), accepted(vec!["x".repeat(1_000); 200]))
            .unwrap();
    }
    assert_eq!(writes.succeed(), Err(Error::State));
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["x".repeat(1_000)]);
}

#[test]
fn overflow_cannot_be_hidden_by_an_ordinary_rejection_or_retry() {
    let mut writes = CommandWrites::new(3).unwrap();
    released(&mut writes, 1);
    writes
        .result(
            RequestId(1),
            WriteResult::Rejected(VenueError::new("first refusal")),
        )
        .unwrap();
    released(&mut writes, 2);
    writes
        .result(
            RequestId(2),
            WriteResult::Rejected(
                VenueError::new("oversized partial").partial_ids(&["x".repeat(1_025)]),
            ),
        )
        .unwrap();
    let _ = writes.grant_resend(RequestId(2));
    assert_eq!(
        writes.retry(RequestId(2), RequestId(3)),
        Err(Error::RetryForbidden)
    );
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(error.message.starts_with("first refusal"));
    assert!(error.message.contains("truncated"));
}

#[test]
fn huge_native_error_text_is_presentation_only_and_does_not_erase_refusal() {
    let mut writes = CommandWrites::new(1).unwrap();
    released(&mut writes, 1);
    let source = "é".repeat(10_000);
    writes
        .result(
            RequestId(1),
            WriteResult::Rejected(
                VenueError::new(source.clone())
                    .kind(crate::ErrorKind::Rejected)
                    .venue_code("42")
                    .venue_message(source),
            ),
        )
        .unwrap();
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Rejected));
    assert_eq!(error.kind, Some(crate::ErrorKind::Rejected));
    assert_eq!(error.venue_code.as_deref(), Some("42"));
    assert!(error.message.len() <= 4_096 && error.message.ends_with("[truncated]"));
    assert!(error.venue_message.unwrap().len() <= 4_096);
}

#[test]
fn fallback_is_bounded_without_inventing_a_release_or_changing_partial_counts() {
    let mut writes = CommandWrites::new(1).unwrap();
    let ids: Vec<_> = (0..4_100).map(|id| format!("fallback-{id}")).collect();
    let error = writes
        .fail(VenueError::new("z".repeat(20_000)).partial(&ids, 9_000, 4_100))
        .unwrap();
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(error.message.len() <= 4_096 && error.message.contains("truncated"));
    let partial = error.partial.unwrap();
    assert_eq!(
        (partial.total, partial.failed_index),
        (Some(9_000), Some(4_100))
    );
    assert!(partial.order_ids.len() <= 4_096);
    assert_eq!(partial.order_ids, ids[..partial.order_ids.len()]);
}

#[test]
fn spare_input_capacity_is_not_retained_by_fact_storage() {
    let mut writes = CommandWrites::new(1).unwrap();
    released(&mut writes, 1);
    let mut id = String::with_capacity(1_000_000);
    id.push_str("exact-id");
    let mut ids = Vec::with_capacity(100_000);
    ids.push(id);
    writes.result(RequestId(1), accepted(ids)).unwrap();
    let Some(WriteResult::Accepted { order_ids }) = &writes.attempts[0].result else {
        panic!("accepted");
    };
    assert_eq!(order_ids.capacity(), order_ids.len());
    assert_eq!(order_ids[0].capacity(), order_ids[0].len());
    assert_eq!(order_ids[0], "exact-id");
}

#[test]
fn configured_attempt_ceiling_is_finite() {
    assert_eq!(CommandWrites::new(257).err(), Some(Error::Capacity));
}

#[test]
fn terminal_fallback_does_not_charge_already_retained_native_ids_twice() {
    let mut writes = CommandWrites::new(1).unwrap();
    released(&mut writes, 1);
    let ids: Vec<_> = (0..4_096).map(|id| format!("native-{id}")).collect();
    writes.result(RequestId(1), accepted(ids.clone())).unwrap();
    let error = writes
        .fail(VenueError::new("later planning refusal").partial(&ids, 4_097, 4_096))
        .unwrap();
    assert_eq!(error.outcome, None);
    assert_eq!(error.message, "later planning refusal");
    let partial = error.partial.unwrap();
    assert_eq!(partial.order_ids, ids);
    assert_eq!(
        (partial.total, partial.failed_index),
        (Some(4_097), Some(4_096))
    );
}

#[test]
fn repeated_known_fallback_ids_are_copied_once_without_consuming_new_budget() {
    let mut writes = CommandWrites::new(1).unwrap();
    released(&mut writes, 1);
    writes
        .result(RequestId(1), accepted(vec!["native".into()]))
        .unwrap();
    let error = writes
        .fail(VenueError::new("planning").partial_ids(&vec!["native".into(); 5_000]))
        .unwrap();
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.unwrap().order_ids, ["native"]);
}

#[test]
fn genuinely_new_fallback_id_at_full_budget_is_unknown_and_preserves_every_native_id() {
    let mut writes = CommandWrites::new(1).unwrap();
    released(&mut writes, 1);
    let ids: Vec<_> = (0..4_096).map(|id| format!("native-{id}")).collect();
    writes.result(RequestId(1), accepted(ids.clone())).unwrap();
    let error = writes
        .fail(VenueError::new("planning").partial_ids(&["native-0".into(), "new".into()]))
        .unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(error.message.contains("truncated IDs"));
    assert_eq!(error.partial.unwrap().order_ids, ids);
}

#[test]
fn mixed_fallback_retains_first_occurrence_order_and_counts_only_new_ids() {
    let mut writes = CommandWrites::new(1).unwrap();
    released(&mut writes, 1);
    writes
        .result(RequestId(1), accepted(vec!["a".into(), "b".into()]))
        .unwrap();
    let error = writes
        .fail(VenueError::new("planning").partial_ids(&[
            "b".into(),
            "new".into(),
            "b".into(),
            "a".into(),
        ]))
        .unwrap();
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.unwrap().order_ids, ["b", "new", "a"]);
}

#[test]
fn exhausted_content_keeps_utf8_markers_and_all_native_outcomes() {
    let mut writes = CommandWrites::new(256).unwrap();
    for id in 1..=256 {
        released(&mut writes, id);
        writes
            .result(
                RequestId(id),
                WriteResult::Rejected(
                    VenueError::new("é".repeat(4_000))
                        .venue_code("界".repeat(3_000))
                        .venue_message("é".repeat(4_000)),
                ),
            )
            .unwrap();
    }
    let mut bytes = 0;
    for (_, error) in writes.rejections() {
        for text in [
            &error.message,
            error.venue_code.as_ref().unwrap(),
            error.venue_message.as_ref().unwrap(),
        ] {
            assert!(text.len() <= 4_096);
            assert!(text.ends_with("[truncated]"));
            bytes += text.len();
        }
    }
    assert!(bytes <= 64 * 1024 + 256 * 3 * "[truncated]".len());
    let last = writes.rejections().last().unwrap().1;
    assert_eq!(last.message, "[truncated]");
    assert_eq!(
        writes.fail(VenueError::new("fallback")).unwrap().outcome,
        Some(Outcome::Rejected)
    );
}

#[test]
fn explicit_report_refusal_does_not_charge_evidence_and_empty_retry_commits_once() {
    let mut writes = CommandWrites::new(3).unwrap();
    released(&mut writes, 1);
    writes
        .result(RequestId(1), accepted(vec!["known".into()]))
        .unwrap();
    released(&mut writes, 2);
    writes
        .result_with_reports(
            RequestId(2),
            WriteResult::Rejected(VenueError::new("refusal")),
            RejectionReports::new(vec!["r".repeat(7_000); 8]).unwrap(),
        )
        .unwrap();
    released(&mut writes, 3);
    let fact =
        WriteResult::Rejected(VenueError::new("é".repeat(5_000)).partial_ids(&["new".into()]));
    let before = writes.evidence;
    assert_eq!(
        writes.result_with_reports(
            RequestId(3),
            fact.clone(),
            RejectionReports::new(vec!["r".repeat(7_000); 2]).unwrap()
        ),
        Err(Error::Capacity)
    );
    assert_eq!(writes.evidence, before);
    assert!(writes.attempts[2].result.is_none());
    writes
        .result_with_reports(RequestId(3), fact, RejectionReports::default())
        .unwrap();
    assert_ne!(writes.evidence, before);
    let once = writes.evidence;
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(writes.evidence, once);
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.unwrap().order_ids, ["known", "new"]);
}

#[test]
fn historical_refusal_retries_share_text_budget_with_terminal_attempt() {
    let mut writes = CommandWrites::new(32).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    for id in 1..=32 {
        if id > 1 {
            let _ = writes.grant_resend(RequestId(id - 1));
            writes.retry(RequestId(id - 1), RequestId(id)).unwrap();
        }
        writes.released(RequestId(id)).unwrap();
        writes
            .result(
                RequestId(id),
                WriteResult::Rejected(
                    VenueError::new("界".repeat(3_000))
                        .venue_code("42")
                        .venue_message("é".repeat(5_000)),
                ),
            )
            .unwrap();
    }
    let error = writes.fail(VenueError::new("unused fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Rejected));
    assert_eq!(error.message, "[truncated]");
    assert_eq!(error.venue_code.as_deref(), Some("[truncated]"));
    assert_eq!(error.venue_message.as_deref(), Some("[truncated]"));
    assert_eq!(writes.rejections().count(), 1);
}

#[test]
fn known_partial_effects_share_text_and_exact_identity_overflow_bounds() {
    let mut writes = CommandWrites::new(3).unwrap();
    released(&mut writes, 1);
    writes
        .result(RequestId(1), accepted(vec!["prior".into()]))
        .unwrap();
    released(&mut writes, 2);
    writes
        .result(
            RequestId(2),
            WriteResult::KnownPartial(VenueError::new("native ".repeat(1000)).partial_ids(&[
                "next".into(),
                "x".repeat(1_025),
                "unretained".into(),
            ])),
        )
        .unwrap();
    let _ = writes.grant_resend(RequestId(2));
    assert_eq!(
        writes.retry(RequestId(2), RequestId(3)),
        Err(Error::RetryForbidden)
    );
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(error.message.len() <= 4_096);
    assert!(error.message.contains("truncated"));
    assert_eq!(error.partial.unwrap().order_ids, ["next", "prior"]);
}

#[test]
fn unproven_envelopes_fix_classification_and_keep_only_bounded_native_details() {
    for status in [408, 500, 503, 600, u16::MAX] {
        let mut writes = CommandWrites::new(1).unwrap();
        released(&mut writes, 1);
        writes
            .unproven_result(RequestId(1), status, |evidence| {
                evidence.venue_code("10004");
                evidence.venue_code("must-not-replace-first-code");
                evidence.venue_message("native auth-looking diagnostic");
                assert!(evidence.order_id("validated-native-id"));
            })
            .unwrap();
        let error = writes.fail(VenueError::new("fallback")).unwrap();
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(
            error.kind,
            Some(if status == 408 {
                crate::ErrorKind::Timeout
            } else {
                crate::ErrorKind::VenueUnavailable
            })
        );
        assert_eq!(error.venue_code.as_deref(), Some("10004"));
        assert_eq!(
            error.venue_message.as_deref(),
            Some("native auth-looking diagnostic")
        );
        let partial = error.partial.unwrap();
        assert_eq!(partial.order_ids, ["validated-native-id"]);
        assert_eq!(partial.total, None);
        assert_eq!(partial.failed_index, None);
    }
}

#[test]
fn unproven_capture_refuses_proven_unreleased_duplicate_and_sealed_attempts_first() {
    let mut writes = CommandWrites::new(1).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    let budget = writes.evidence;
    let mut captured = false;
    assert_eq!(
        writes.unproven_result(RequestId(1), 500, |_| captured = true),
        Err(Error::State)
    );
    assert!(!captured);
    writes.released(RequestId(1)).unwrap();
    for status in [0, 99, 200, 400, 429, 499] {
        assert_eq!(
            writes.unproven_result(RequestId(1), status, |_| captured = true),
            Err(Error::Invalid)
        );
    }
    assert!(!captured);
    assert_eq!(writes.evidence, budget);
    assert!(writes.attempts[0].result.is_none());
    writes.unproven_result(RequestId(1), 500, |_| {}).unwrap();
    let charged = writes.evidence;
    assert_eq!(
        writes.unproven_result(RequestId(1), 503, |_| captured = true),
        Err(Error::State)
    );
    assert!(!captured);
    assert_eq!(writes.evidence, charged);
    writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(
        writes.unproven_result(RequestId(1), 500, |_| captured = true),
        Err(Error::State)
    );
    assert!(!captured);
}

#[test]
fn unproven_id_writer_shares_the_last_command_slot_without_charging_twice() {
    let prior: Vec<_> = (0..4_095).map(|id| format!("prior-{id}")).collect();
    let mut writes = CommandWrites::new(2).unwrap();
    let mut reference = CommandWrites::new(2).unwrap();
    for journal in [&mut writes, &mut reference] {
        released(journal, 1);
        journal
            .result(RequestId(1), accepted(prior.clone()))
            .unwrap();
        released(journal, 2);
    }
    writes
        .unproven_result(RequestId(2), 500, |evidence| {
            assert!(evidence.order_id("last-native"));
        })
        .unwrap();
    reference
        .result(
            RequestId(2),
            WriteResult::Failed(
                ResponseProof::from_status(500)
                    .error()
                    .unwrap()
                    .partial_ids(&["last-native".into()]),
            ),
        )
        .unwrap();
    assert_eq!(writes.evidence, reference.evidence);
    assert!(!writes.attempts[1].id_overflow);
    let charged = writes.evidence;
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(writes.evidence, charged);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    let ids = error.partial.unwrap().order_ids;
    assert_eq!(ids.len(), 4_096);
    assert_eq!(ids[0], "last-native");
    assert_eq!(&ids[1..], prior);
    assert!(!error.message.contains("truncated IDs"));
}

#[test]
fn unproven_writer_never_truncates_ids_or_erases_prior_partial_effects() {
    let mut writes = CommandWrites::new(2).unwrap();
    released(&mut writes, 1);
    writes
        .result(
            RequestId(1),
            WriteResult::KnownPartial(
                VenueError::new("known partial").partial_ids(&["known".into()]),
            ),
        )
        .unwrap();
    released(&mut writes, 2);
    writes
        .unproven_result(RequestId(2), 500, |evidence| {
            assert!(evidence.order_id("exact-prefix"));
            assert!(!evidence.order_id(&"x".repeat(1_025)));
            assert!(!evidence.order_id("after-overflow"));
        })
        .unwrap();
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(error.message.contains("truncated IDs"));
    assert_eq!(error.partial.unwrap().order_ids, ["known", "exact-prefix"]);
}

#[test]
fn unproven_diagnostics_cannot_reset_the_command_text_quota() {
    let mut writes = CommandWrites::new(7).unwrap();
    for id in 1..=6 {
        released(&mut writes, id);
        writes
            .result(
                RequestId(id),
                WriteResult::Rejected(
                    VenueError::new("m".repeat(4_096))
                        .venue_code("c".repeat(4_096))
                        .venue_message("v".repeat(4_096)),
                ),
            )
            .unwrap();
    }
    released(&mut writes, 7);
    writes
        .unproven_result(RequestId(7), 408, |evidence| {
            evidence.venue_code("10004");
            evidence.venue_message(&"漢".repeat(5_000));
            assert!(evidence.order_id("still-retain-money-evidence"));
        })
        .unwrap();
    let Some(WriteResult::Failed(error)) = &writes.attempts[6].result else {
        panic!("unproven response must retain a Failed fact");
    };
    assert_eq!(error.message, "[truncated]");
    assert_eq!(error.venue_code.as_deref(), Some("[truncated]"));
    assert_eq!(error.venue_message.as_deref(), Some("[truncated]"));
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(
        error.partial.unwrap().order_ids,
        ["still-retain-money-evidence"]
    );
}

// A detached observer is not the terminal physical execution. The original RED
// used terminal fail here; all physical facts and assertions remain unchanged.
fn observer_projection(writes: &CommandWrites, error: VenueError) -> VenueError {
    writes.project_failure(error).unwrap()
}

#[test]
fn observer_projection_before_send_preserves_registration_and_evidence_budget() {
    let mut writes = CommandWrites::new(2).unwrap();
    let budget = writes.evidence;
    for _ in 0..3 {
        let error = observer_projection(&writes, VenueError::new("x".repeat(100_000)));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert!(error.message.len() <= 4_096 && error.message.ends_with("[truncated]"));
        assert!(
            !writes.sealed,
            "one observer cannot seal the physical execution"
        );
        assert_eq!(
            writes.evidence, budget,
            "projection cannot charge physical facts"
        );
    }
    released(&mut writes, 1);
    writes
        .result(RequestId(1), accepted(vec!["peer-order".into()]))
        .unwrap();
    assert_eq!(writes.succeed(), Ok(()));
    assert_eq!(
        writes.project_failure(VenueError::new("after success")),
        Err(Error::State)
    );
}

#[test]
fn observer_projection_keeps_accepted_ids_and_unresolved_peer_live() {
    let mut writes = CommandWrites::new(2).unwrap();
    released(&mut writes, 1);
    writes
        .result(RequestId(1), accepted(vec!["prior-native".into()]))
        .unwrap();
    released(&mut writes, 2);
    let budget = writes.evidence;
    let error = observer_projection(&writes, VenueError::new("observer expired"));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["prior-native"]);
    assert!(!writes.sealed);
    assert_eq!(writes.evidence, budget);
    writes
        .result(RequestId(2), accepted(vec!["later-native".into()]))
        .unwrap();
    let error = observer_projection(&writes, VenueError::new("another observer detached"));
    assert_eq!(error.outcome, None);
    assert_eq!(
        error.partial.unwrap().order_ids,
        ["prior-native", "later-native"]
    );
    assert!(!writes.sealed);
    assert_eq!(writes.succeed(), Ok(()));
}

#[test]
fn observer_projection_does_not_consume_definitive_refusal_retry() {
    for fail_retry in [false, true] {
        let mut writes = CommandWrites::new(2).unwrap();
        released(&mut writes, 1);
        writes
            .result(
                RequestId(1),
                WriteResult::Rejected(VenueError::new("native refusal")),
            )
            .unwrap();
        writes.grant_resend(RequestId(1)).unwrap();
        let budget = writes.evidence;
        let error = observer_projection(&writes, VenueError::new("observer detached"));
        assert_eq!(error.outcome, Some(Outcome::Rejected));
        assert!(!writes.sealed);
        assert_eq!(writes.evidence, budget);
        writes.retry(RequestId(1), RequestId(2)).unwrap();
        if fail_retry {
            writes
                .result(
                    RequestId(2),
                    WriteResult::Failed(VenueError::new("provider unavailable")),
                )
                .unwrap();
            let error = observer_projection(&writes, VenueError::new("peer expired"));
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert!(!writes.sealed);
            assert_eq!(
                writes
                    .fail(VenueError::new("final execution bound"))
                    .unwrap()
                    .outcome,
                Some(Outcome::Unknown)
            );
        } else {
            writes.released(RequestId(2)).unwrap();
            writes
                .result(RequestId(2), accepted(vec!["retry-native".into()]))
                .unwrap();
            assert_eq!(writes.succeed(), Ok(()));
        }
    }
}

#[test]
fn observer_projection_matches_final_failure_without_committing_its_fallback_budget() {
    let mut writes = CommandWrites::new(1).unwrap();
    let before = writes.evidence;
    let fallback = VenueError::new("é".repeat(10_000)).partial_ids(&["bounded-native".into()]);
    let projected = writes.project_failure(fallback.clone()).unwrap();
    assert_eq!(writes.evidence, before);
    assert!(!writes.sealed);
    assert_eq!(writes.fail(fallback).unwrap(), projected);
    assert_ne!(writes.evidence, before);
    assert!(writes.sealed);
    assert_eq!(
        writes.project_failure(VenueError::new("duplicate")),
        Err(Error::State)
    );
    assert_eq!(writes.fail(VenueError::new("duplicate")), Err(Error::State));
}

#[test]
fn observer_projection_preserves_selected_native_diagnostics_reports_and_overflow() {
    for overflow in [false, true] {
        let mut writes = CommandWrites::new(2).unwrap();
        released(&mut writes, 1);
        let ids = if overflow {
            (0..4_100).map(|n| format!("native-{n}")).collect()
        } else {
            vec!["prior-native".into()]
        };
        writes.result(RequestId(1), accepted(ids)).unwrap();
        released(&mut writes, 2);
        writes
            .result_with_reports(
                RequestId(2),
                WriteResult::Rejected(
                    VenueError::new("native refusal")
                        .venue_code("native-code")
                        .venue_message("native message"),
                ),
                RejectionReports::new(vec!["native rejected leg".into()]).unwrap(),
            )
            .unwrap();
        let budget = writes.evidence;
        let report_bytes = writes.report_bytes;
        let reports: Vec<_> = writes.reports().map(str::to_owned).collect();
        let fallback = VenueError::new("unselected".repeat(10_000));
        let projected = writes.project_failure(fallback.clone()).unwrap();
        assert_eq!(projected.venue_code.as_deref(), Some("native-code"));
        assert_eq!(projected.venue_message.as_deref(), Some("native message"));
        assert_eq!(
            projected.outcome,
            if overflow {
                Some(Outcome::Unknown)
            } else {
                None
            }
        );
        assert_eq!(
            projected.partial.as_ref().unwrap().order_ids[0],
            if overflow { "native-0" } else { "prior-native" }
        );
        assert_eq!(writes.evidence, budget);
        assert_eq!(writes.report_bytes, report_bytes);
        assert_eq!(writes.reports().collect::<Vec<_>>(), reports);
        assert_eq!(writes.fail(fallback).unwrap(), projected);
        assert_eq!(
            writes.evidence, budget,
            "selected facts are already charged"
        );
        assert_eq!(writes.reports().collect::<Vec<_>>(), reports);
    }
}

// The original control used the HTTP-status-only writer with status200.
fn capture_native_unproven(
    writes: &mut CommandWrites,
    id: RequestId,
    capture: impl FnOnce(&mut UnprovenEvidence),
) -> Result<(), Error> {
    writes.unproven_native_result(id, capture)
}

#[test]
fn native_unproven_evidence_retains_exact_ids_and_prior_effects_without_http_status() {
    let mut writes = CommandWrites::new(2).unwrap();
    released(&mut writes, 1);
    writes
        .result(RequestId(1), accepted(vec!["prior".into()]))
        .unwrap();
    released(&mut writes, 2);
    assert_eq!(
        capture_native_unproven(&mut writes, RequestId(2), |evidence| {
            evidence.venue_code("0");
            evidence.venue_message("invalid native batch cardinality");
            assert!(evidence.order_id("native-uncorrelated"));
        }),
        Ok(())
    );
    let charged = writes.evidence;
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(writes.evidence, charged);
    assert_eq!(error.kind, Some(crate::ErrorKind::Internal));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.venue_code.as_deref(), Some("0"));
    assert_eq!(
        error.venue_message.as_deref(),
        Some("invalid native batch cardinality")
    );
    assert_eq!(
        error.partial.unwrap().order_ids,
        ["native-uncorrelated", "prior"]
    );
}

#[test]
fn native_unproven_capture_shares_http_quota_and_rejects_before_capture() {
    let mut writes = CommandWrites::new(3).unwrap();
    released(&mut writes, 1);
    let ids: Vec<_> = (0..4094).map(|i| format!("prior-{i}")).collect();
    writes.result(RequestId(1), accepted(ids)).unwrap();
    released(&mut writes, 2);
    writes
        .unproven_result(RequestId(2), 500, |e| assert!(e.order_id("http-native")))
        .unwrap();
    released(&mut writes, 3);
    writes
        .unproven_native_result(RequestId(3), |e| {
            e.venue_code("10004");
            e.venue_message(&"漢".repeat(5000));
            assert!(e.order_id("last-native"));
            assert!(!e.order_id("overflow"));
        })
        .unwrap();
    let charged = writes.evidence;
    let mut captures = 0;
    assert_eq!(
        writes.unproven_native_result(RequestId(3), |_| captures += 1),
        Err(Error::State)
    );
    assert_eq!(captures, 0);
    assert_eq!(writes.evidence, charged);
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(writes.evidence, charged);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(error.message.contains("truncated IDs"));
    let ids = error.partial.unwrap().order_ids;
    assert_eq!(ids.len(), 4096);
    assert!(ids.iter().any(|id| id == "http-native"));
    assert!(ids.iter().any(|id| id == "last-native"));
    assert!(!ids.iter().any(|id| id == "overflow"));
}

#[test]
fn native_unproven_writer_never_rewrites_ids_or_native_diagnostics_into_auth() {
    let mut writes = CommandWrites::new(1).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    let mut captures = 0;
    assert_eq!(
        writes.unproven_native_result(RequestId(1), |_| captures += 1),
        Err(Error::State)
    );
    assert_eq!(captures, 0);
    writes.released(RequestId(1)).unwrap();
    writes
        .unproven_native_result(RequestId(1), |e| {
            e.venue_code("10004");
            e.venue_message("invalid api key signature");
            assert!(e.order_id("exact"));
            assert!(!e.order_id(&"x".repeat(1025)));
            assert!(!e.order_id("after-overflow"));
        })
        .unwrap();
    let charged = writes.evidence;
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.kind, Some(crate::ErrorKind::Internal));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["exact"]);
    assert_eq!(error.venue_code.as_deref(), Some("10004"));
    assert_eq!(
        writes.unproven_native_result(RequestId(1), |_| captures += 1),
        Err(Error::State)
    );
    assert_eq!(captures, 0);
    assert_eq!(writes.evidence, charged);
}

#[test]
fn native_unproven_capture_cannot_borrow_a_prior_attempt_after_retry() {
    let mut writes = CommandWrites::new(2).unwrap();
    released(&mut writes, 1);
    writes
        .result(
            RequestId(1),
            WriteResult::Rejected(VenueError::new("definitively refused")),
        )
        .unwrap();
    writes.grant_resend(RequestId(1)).unwrap();
    writes.retry(RequestId(1), RequestId(2)).unwrap();
    let mut captures = 0;
    assert_eq!(
        writes.unproven_native_result(RequestId(1), |_| captures += 1),
        Err(Error::Revoked)
    );
    assert_eq!(
        writes.unproven_native_result(RequestId(2), |_| captures += 1),
        Err(Error::State)
    );
    assert_eq!(captures, 0);
    writes.released(RequestId(2)).unwrap();
    writes
        .unproven_native_result(RequestId(2), |e| {
            captures += 1;
            assert!(e.order_id("new-native"));
        })
        .unwrap();
    assert_eq!(captures, 1);
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["new-native"]);
}

#[test]
fn effect_authority_uses_normalized_current_facts_and_not_a_siblings_unknown_outcome() {
    let facts = [
        None,
        Some(WriteResult::Accepted { order_ids: vec![] }),
        Some(WriteResult::KnownPartial(VenueError::new(
            "known partial without IDs",
        ))),
        Some(WriteResult::KnownPartial(VenueError::new("m".repeat(9000)))),
        Some(WriteResult::Rejected(VenueError::new("refused"))),
        Some(WriteResult::Failed(VenueError::new("unproven"))),
        Some(WriteResult::Accepted {
            order_ids: vec!["n".repeat(1025)],
        }),
        Some(WriteResult::KnownPartial(
            VenueError::new("known").partial_ids(&["n".repeat(1025)]),
        )),
    ];
    for (case, fact) in facts.into_iter().enumerate() {
        let mut writes = CommandWrites::new(2).unwrap();
        released(&mut writes, 1);
        writes
            .result(
                RequestId(1),
                WriteResult::Failed(VenueError::new("other leg uncertain")),
            )
            .unwrap();
        released(&mut writes, 2);
        if let Some(fact) = fact {
            writes.result(RequestId(2), fact).unwrap();
        }
        let budget = writes.evidence;
        let counts = (
            writes.attempts.len(),
            writes.legs.len(),
            writes.report_bytes,
            writes.report_messages,
            writes.sealed,
        );
        for _ in 0..2 {
            assert_eq!(
                writes.validate_effects(RequestId(2)).is_ok(),
                matches!(case, 1..=3),
                "case {case}"
            );
            assert_eq!(
                writes.validate_housekeeping(RequestId(2)).is_ok(),
                matches!(case, 1..=5),
                "housekeeping case {case}"
            );
            assert_eq!(writes.evidence, budget);
            assert_eq!(
                (
                    writes.attempts.len(),
                    writes.legs.len(),
                    writes.report_bytes,
                    writes.report_messages,
                    writes.sealed
                ),
                counts
            );
        }
        assert_eq!(
            writes
                .project_failure(VenueError::new("preview"))
                .unwrap()
                .outcome,
            Some(Outcome::Unknown)
        );
        assert_eq!(
            writes.validate_effects(RequestId(2)).is_ok(),
            matches!(case, 1..=3)
        );
        writes.fail(VenueError::new("finish")).unwrap();
        assert_eq!(writes.validate_effects(RequestId(2)), Err(Error::State));
        assert_eq!(
            writes.validate_housekeeping(RequestId(2)),
            Err(Error::State)
        );
    }
}

#[test]
fn report_capacity_refusal_cannot_create_effect_authority_before_fact_admission() {
    let mut writes = CommandWrites::new(2).unwrap();
    released(&mut writes, 1);
    writes
        .result_with_reports(
            RequestId(1),
            WriteResult::Rejected(VenueError::new("refused")),
            RejectionReports::new(vec!["r".repeat(4096); 15]).unwrap(),
        )
        .unwrap();
    released(&mut writes, 2);
    let budget = writes.evidence;
    let counts = (writes.report_bytes, writes.report_messages);
    assert_eq!(
        writes.result_with_reports(
            RequestId(2),
            WriteResult::KnownPartial(VenueError::new("known effects")),
            RejectionReports::new(vec!["r".repeat(4096)]).unwrap()
        ),
        Err(Error::Capacity)
    );
    assert_eq!(writes.validate_effects(RequestId(2)), Err(Error::State));
    assert_eq!(writes.evidence, budget);
    assert_eq!((writes.report_bytes, writes.report_messages), counts);
    writes
        .result_with_reports(
            RequestId(2),
            WriteResult::KnownPartial(VenueError::new("known effects")),
            RejectionReports::default(),
        )
        .unwrap();
    assert_eq!(writes.validate_effects(RequestId(2)), Ok(()));
    assert_eq!(
        writes.result(RequestId(2), accepted(vec![])),
        Err(Error::State)
    );
    assert_eq!(writes.validate_effects(RequestId(2)), Ok(()));
}

#[test]
fn effect_authority_rejects_an_old_retry_and_superseded_parent() {
    let mut writes = CommandWrites::new(3).unwrap();
    released(&mut writes, 1);
    writes
        .result(
            RequestId(1),
            WriteResult::Rejected(VenueError::new("refused")),
        )
        .unwrap();
    writes.grant_resend(RequestId(1)).unwrap();
    writes.retry(RequestId(1), RequestId(2)).unwrap();
    assert_eq!(writes.validate_effects(RequestId(1)), Err(Error::Revoked));
    assert_eq!(
        writes.validate_housekeeping(RequestId(1)),
        Err(Error::Revoked)
    );
    assert_eq!(writes.validate_effects(RequestId(2)), Err(Error::State));
    writes.released(RequestId(2)).unwrap();
    writes
        .result(
            RequestId(2),
            WriteResult::Rejected(VenueError::new("refused again")),
        )
        .unwrap();
    writes.replace(RequestId(2), &[3]).unwrap();
    assert_eq!(writes.validate_effects(RequestId(2)), Err(Error::Revoked));
    assert_eq!(
        writes.validate_housekeeping(RequestId(2)),
        Err(Error::Revoked)
    );
    writes.register(RequestId(3), 3).unwrap();
    writes.released(RequestId(3)).unwrap();
    writes.result(RequestId(3), accepted(vec![])).unwrap();
    assert_eq!(writes.validate_effects(RequestId(3)), Ok(()));
}

#[test]
fn proven_subset_does_not_erase_an_unresolved_remainder() {
    for ids in [vec![], vec!["proven-native".to_owned()]] {
        let mut writes = CommandWrites::new(4).unwrap();
        released(&mut writes, 1);
        let mut error =
            VenueError::new("validated subset; omitted targets unresolved").partial_ids(&ids);
        error.outcome = Some(Outcome::Unknown);
        writes
            .result(RequestId(1), WriteResult::PartialUnknown(error))
            .unwrap();
        released(&mut writes, 2);
        writes
            .result(RequestId(2), accepted(vec!["sibling".to_owned()]))
            .unwrap();
        let charged = writes.evidence;
        assert_eq!(writes.validate_effects(RequestId(1)), Ok(()));
        assert_eq!(
            writes.retry(RequestId(1), RequestId(3)),
            Err(Error::RetryForbidden)
        );
        assert_eq!(
            writes.replace(RequestId(1), &[3]),
            Err(Error::RetryForbidden)
        );
        assert_eq!(writes.succeed(), Err(Error::State));
        for _ in 0..2 {
            let preview = writes.project_failure(VenueError::new("fallback")).unwrap();
            assert_eq!(preview.outcome, Some(Outcome::Unknown));
            assert_eq!(
                preview.message,
                "validated subset; omitted targets unresolved"
            );
            assert_eq!(writes.evidence, charged);
        }
        let final_error = writes.fail(VenueError::new("fallback")).unwrap();
        assert_eq!(final_error.outcome, Some(Outcome::Unknown));
        let mut expected = ids;
        expected.push("sibling".to_owned());
        assert_eq!(final_error.partial.unwrap().order_ids, expected);
        assert_eq!(writes.evidence, charged);
        assert_eq!(writes.validate_effects(RequestId(1)), Err(Error::State));
    }
}

#[test]
fn partial_unknown_survives_earlier_error_selection_and_keeps_explicit_reports() {
    let mut writes = CommandWrites::new(4).unwrap();
    // Logical order, not callback order, chooses the diagnostic.
    for id in 1..=3 {
        released(&mut writes, id);
    }
    writes
        .result(RequestId(3), accepted(vec!["later".into()]))
        .unwrap();
    writes
        .result_with_reports(
            RequestId(2),
            WriteResult::PartialUnknown(
                VenueError::new("subset uncertain").partial_ids(&["subset".into()]),
            ),
            RejectionReports::new(vec!["one row rejected; another unresolved".into()]).unwrap(),
        )
        .unwrap();
    writes
        .result(
            RequestId(1),
            WriteResult::Rejected(VenueError::new("first refusal")),
        )
        .unwrap();
    assert_eq!(
        writes.rejections().map(|(id, _)| id).collect::<Vec<_>>(),
        [RequestId(1)]
    );
    assert_eq!(
        writes.reports().collect::<Vec<_>>(),
        ["first refusal", "one row rejected; another unresolved"]
    );
    let budget = writes.evidence;
    let error = writes.project_failure(VenueError::new("fallback")).unwrap();
    assert_eq!(error.message, "first refusal");
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["subset", "later"]);
    assert_eq!(writes.validate_effects(RequestId(2)), Ok(()));
    assert_eq!(
        writes.fail(VenueError::new("fallback")).unwrap().outcome,
        Some(Outcome::Unknown)
    );
    assert_eq!(writes.evidence, budget);
}

#[test]
fn partial_unknown_admission_requires_release_and_respects_report_atomicity() {
    let mut writes = CommandWrites::new(2).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    let fact = || WriteResult::PartialUnknown(VenueError::new("validated subset"));
    assert_eq!(writes.result(RequestId(1), fact()), Err(Error::State));
    assert_eq!(writes.evidence, evidence::EvidenceBudget::default());
    assert_eq!(writes.attempts[0].result, None);
    writes.released(RequestId(1)).unwrap();
    writes
        .result_with_reports(
            RequestId(1),
            fact(),
            RejectionReports::new(vec!["r".repeat(4096); 15]).unwrap(),
        )
        .unwrap();
    assert_eq!(writes.rejections().count(), 0);
    released(&mut writes, 2);
    let budget = writes.evidence;
    let counts = (writes.report_messages, writes.report_bytes);
    assert_eq!(
        writes.result_with_reports(
            RequestId(2),
            fact(),
            RejectionReports::new(vec!["r".repeat(4096)]).unwrap(),
        ),
        Err(Error::Capacity)
    );
    assert_eq!(writes.evidence, budget);
    assert_eq!((writes.report_messages, writes.report_bytes), counts);
    assert_eq!(writes.validate_effects(RequestId(2)), Err(Error::State));
    assert_eq!(writes.attempts[1].result, None);
    writes
        .result_with_reports(RequestId(2), fact(), RejectionReports::default())
        .unwrap();
    assert_eq!(writes.validate_effects(RequestId(2)), Ok(()));
}

#[test]
fn subset_authority_distinguishes_own_overflow_from_sibling_overflow_and_evidence_only_ids() {
    for (own_ids, sibling_ids, expected) in [
        (vec!["exact".into()], vec!["x".repeat(1025)], true),
        (
            vec!["exact".into(), "x".repeat(1025), "after".into()],
            vec![],
            false,
        ),
    ] {
        let mut writes = CommandWrites::new(2).unwrap();
        released(&mut writes, 1);
        writes
            .result(
                RequestId(1),
                WriteResult::Failed(VenueError::new("evidence only").partial_ids(&sibling_ids)),
            )
            .unwrap();
        released(&mut writes, 2);
        writes
            .result(
                RequestId(2),
                WriteResult::PartialUnknown(VenueError::new("native subset").partial_ids(&own_ids)),
            )
            .unwrap();
        assert_eq!(writes.validate_effects(RequestId(1)), Err(Error::State));
        assert_eq!(writes.validate_effects(RequestId(2)).is_ok(), expected);
        assert_eq!(writes.reports().count(), 0);
        let error = writes.fail(VenueError::new("fallback")).unwrap();
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(error.partial.unwrap().order_ids, ["exact"]);
        assert!(error.message.contains("truncated IDs"));
    }
}
