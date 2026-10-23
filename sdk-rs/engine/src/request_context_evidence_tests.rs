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
