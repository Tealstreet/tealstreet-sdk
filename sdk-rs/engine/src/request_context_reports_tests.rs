use super::*;

fn refused(message: &str) -> WriteResult {
    WriteResult::Rejected(VenueError::new(message))
}

fn reports(messages: &[&str]) -> RejectionReports {
    RejectionReports::new(messages.iter().map(|s| (*s).to_owned()).collect()).unwrap()
}

#[test]
fn report_bounds_count_retained_allocations_and_utf8_bytes() {
    for messages in [
        Vec::with_capacity(REPORT_MESSAGES + 1),
        vec![String::with_capacity(REPORT_MESSAGE_BYTES + 1)],
        vec!["é".repeat(REPORT_MESSAGE_BYTES / 2 + 1)],
        vec!["x".repeat(REPORT_MESSAGE_BYTES); 9],
    ] {
        assert_eq!(RejectionReports::new(messages), Err(Error::Capacity));
    }
    let value = reports(&["native diagnostic"]);
    assert!(!format!("{value:?}").contains("native diagnostic"));
}

#[test]
fn terminal_batch_reports_and_error_text_follow_original_leg_registration_not_retry_position() {
    let mut writes = CommandWrites::new(3).unwrap();
    // Engine-issued leg ids retain logical planning order even when physical
    // registration is reversed. Later retry A must still report before B.
    writes.register(RequestId(2), 20).unwrap();
    writes.register(RequestId(1), 10).unwrap();
    writes.released(RequestId(1)).unwrap();
    writes.released(RequestId(2)).unwrap();
    // B responds first, A is definitively refused and retried after B.
    writes
        .result_with_reports(RequestId(2), refused("B"), reports(&["B1", "B2"]))
        .unwrap();
    writes
        .result_with_reports(RequestId(1), refused("old A"), reports(&["old A"]))
        .unwrap();
    let _ = writes.grant_resend(RequestId(1));
    writes.retry(RequestId(1), RequestId(3)).unwrap();
    writes.released(RequestId(3)).unwrap();
    writes
        .result_with_reports(RequestId(3), refused("A"), reports(&["A1", "A2"]))
        .unwrap();
    let error = writes.fail(VenueError::new("callback fallback")).unwrap();
    assert_eq!(error.message, "A");
    assert_eq!(error.outcome, Some(Outcome::Rejected));
    assert_eq!(
        writes.reports().collect::<Vec<_>>(),
        ["A1", "A2", "B1", "B2"]
    );
}

#[test]
fn historical_retry_reports_consume_command_budget_but_capacity_failure_does_not_erase_native_fact()
{
    let mut writes = CommandWrites::new(2).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    writes.released(RequestId(1)).unwrap();
    writes
        .result_with_reports(
            RequestId(1),
            refused("old refusal"),
            RejectionReports::new(vec!["x".repeat(7_000); 8]).unwrap(),
        )
        .unwrap();
    let _ = writes.grant_resend(RequestId(1));
    writes.retry(RequestId(1), RequestId(2)).unwrap();
    writes.released(RequestId(2)).unwrap();
    let bytes = writes.report_bytes;
    let count = writes.report_messages;
    let fact = WriteResult::Rejected(
        VenueError::new("native mixed result").partial_ids(&["placed-id".into()]),
    );
    assert_eq!(
        writes.result_with_reports(
            RequestId(2),
            fact.clone(),
            RejectionReports::new(vec!["y".repeat(7_000); 2]).unwrap()
        ),
        Err(Error::Capacity)
    );
    assert!(writes.attempts[1].result.is_none());
    assert_eq!(
        (writes.report_bytes, writes.report_messages),
        (bytes, count)
    );
    // The adapter explicitly preserves the same native fact and partial IDs
    // after refusing only its reporting metadata.
    writes
        .result_with_reports(RequestId(2), fact, RejectionReports::default())
        .unwrap();
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.unwrap().order_ids, ["placed-id"]);
    assert!(writes.reports().next().is_none());
}

#[test]
fn message_count_is_command_wide_including_historical_retries() {
    let mut writes = CommandWrites::new(2).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    writes.released(RequestId(1)).unwrap();
    writes
        .result_with_reports(
            RequestId(1),
            refused("old"),
            RejectionReports::new(vec![String::new(); 255]).unwrap(),
        )
        .unwrap();
    let _ = writes.grant_resend(RequestId(1));
    writes.retry(RequestId(1), RequestId(2)).unwrap();
    writes.released(RequestId(2)).unwrap();
    assert_eq!(
        writes.result_with_reports(RequestId(2), refused("new"), reports(&["a", "b"])),
        Err(Error::Capacity)
    );
    assert_eq!(writes.report_messages, 255);
    assert!(writes.attempts[1].result.is_none());
    writes
        .result_with_reports(RequestId(2), refused("new"), reports(&["only"]))
        .unwrap();
    assert_eq!(writes.report_messages, 256);
    assert_eq!(writes.reports().collect::<Vec<_>>(), ["only"]);
}

#[test]
fn explicit_silence_and_default_reports_have_identical_native_outcomes() {
    for silent in [false, true] {
        let mut writes = CommandWrites::new(1).unwrap();
        writes.register(RequestId(1), 1).unwrap();
        writes.released(RequestId(1)).unwrap();
        if silent {
            writes
                .result_with_reports(RequestId(1), refused("native"), RejectionReports::default())
                .unwrap();
        } else {
            writes.result(RequestId(1), refused("native")).unwrap();
        }
        assert_eq!(
            writes.fail(VenueError::new("fallback")).unwrap().outcome,
            Some(Outcome::Rejected)
        );
        assert_eq!(
            writes.reports().collect::<Vec<_>>(),
            if silent { vec![] } else { vec!["native"] }
        );
    }
}

#[test]
fn accepted_facts_cannot_publish_rejection_reports_and_recovered_attempts_are_silent() {
    let mut writes = CommandWrites::new(2).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    writes.released(RequestId(1)).unwrap();
    let accepted = WriteResult::Accepted {
        order_ids: vec!["kept".into()],
    };
    assert_eq!(
        writes.result_with_reports(RequestId(1), accepted.clone(), reports(&["invalid"])),
        Err(Error::Invalid)
    );
    assert!(writes.attempts[0].result.is_none());
    assert_eq!((writes.report_bytes, writes.report_messages), (0, 0));
    writes
        .result_with_reports(RequestId(1), refused("transient"), reports(&["transient"]))
        .unwrap();
    let _ = writes.grant_resend(RequestId(1));
    writes.retry(RequestId(1), RequestId(2)).unwrap();
    writes.released(RequestId(2)).unwrap();
    writes
        .result_with_reports(RequestId(2), accepted, RejectionReports::default())
        .unwrap();
    writes.succeed().unwrap();
    assert!(writes.reports().next().is_none());
    assert!(
        writes.report_bytes > 0,
        "historical allocation still counts until retirement"
    );
}

#[cfg(feature = "serial-plans")]
#[test]
fn streamed_and_live_reports_preserve_order_silence_and_sealed_reads() {
    let mut writes = CommandWrites::new(3).unwrap().streamed_with(true);
    writes.register(RequestId(1), 1).unwrap();
    writes.released(RequestId(1)).unwrap();
    writes
        .result_with_reports(
            RequestId(1),
            refused("streamed"),
            reports(&["stream-α", "", "stream-tail"]),
        )
        .unwrap();
    writes.compact_serial(1);
    writes.register(RequestId(2), 30).unwrap();
    writes.register(RequestId(3), 20).unwrap();
    writes.released(RequestId(2)).unwrap();
    writes.released(RequestId(3)).unwrap();
    writes
        .result_with_reports(RequestId(2), refused("B"), reports(&["B1", "B2"]))
        .unwrap();
    writes
        .result_with_reports(
            RequestId(3),
            refused("old A"),
            reports(&["recovered retry"]),
        )
        .unwrap();
    let _ = writes.grant_resend(RequestId(3));
    writes.retry(RequestId(3), RequestId(4)).unwrap();
    writes.released(RequestId(4)).unwrap();
    writes
        .result_with_reports(
            RequestId(4),
            WriteResult::KnownPartial(VenueError::new("mixed A").partial_ids(&["kept".into()])),
            reports(&["A1", "A2"]),
        )
        .unwrap();
    let before = (
        writes.evidence,
        writes.attempts.len(),
        writes.legs.len(),
        writes.report_bytes,
        writes.report_messages,
    );
    let expected = ["stream-α", "", "stream-tail", "A1", "A2", "B1", "B2"];
    for _ in 0..2 {
        assert_eq!(writes.reports().collect::<Vec<_>>(), expected);
        assert_eq!(
            writes.rejections().map(|(id, _)| id).collect::<Vec<_>>(),
            [RequestId(1), RequestId(4), RequestId(2)]
        );
        assert_eq!(writes.refusal_count(), 3);
        assert_eq!(
            (
                writes.evidence,
                writes.attempts.len(),
                writes.legs.len(),
                writes.report_bytes,
                writes.report_messages
            ),
            before
        );
    }
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.message, "streamed");
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.unwrap().order_ids, ["kept"]);
    assert_eq!(writes.reports().collect::<Vec<_>>(), expected);
    assert_eq!(writes.refusal_count(), 3);
}
