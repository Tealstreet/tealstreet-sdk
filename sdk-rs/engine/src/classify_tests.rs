use super::*;

const ROWS: &[Row] = &[
    Row {
        key: Key::Int(-2011),
        verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
        only: Some(Intent::Cancel),
    },
    Row {
        key: Key::Int(-2011),
        verdict: Verdict::Refused(ErrorKind::Rejected),
        only: None,
    },
    Row {
        key: Key::Int(-1021),
        verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
        only: None,
    },
    Row {
        key: Key::Int(-1003),
        verdict: Verdict::Refused(ErrorKind::RateLimited),
        only: None,
    },
    Row {
        key: Key::Int(-1022),
        verdict: Verdict::Refused(ErrorKind::AuthInvalid),
        only: None,
    },
    Row {
        key: Key::Range(-1199, -1100),
        verdict: Verdict::Refused(ErrorKind::InvalidInput),
        only: None,
    },
    Row {
        key: Key::Text("EOrder:Insufficient funds"),
        verdict: Verdict::Refused(ErrorKind::Rejected),
        only: None,
    },
];

const PREFIX: CodeTable = CodeTable {
    rows: &[Row {
        key: Key::Message("Order was never placed, already canceled, or filled"),
        verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
        only: Some(Intent::Cancel),
    }],
    success: &[],
    unlisted: Verdict::Refused(ErrorKind::Rejected),
    status: &[],
};

const TABLE: CodeTable = CodeTable {
    rows: ROWS,
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[(429, Verdict::Refused(ErrorKind::RateLimited))],
};

fn response(status: u16) -> Response<'static> {
    Response {
        status,
        headers: "",
        body: "",
    }
}

fn int(code: i64) -> Native<'static> {
    Native {
        code: NativeCode::Int(code),
        message: "native message",
    }
}

fn classify(status: u16, native: Option<&Native<'_>>, intent: Intent) -> Verdict {
    verdict(
        &TABLE,
        intent,
        Released::Response(&response(status)),
        native,
    )
}

#[test]
fn the_test_table_satisfies_its_own_invariants() {
    assert_table(&TABLE);
}

#[test]
fn transport_loss_after_release_is_unknown_network() {
    let (verdict, error) = write(&TABLE, Intent::Place, Released::Transport, None, "lost");
    assert_eq!(verdict, Verdict::Unknown(Some(ErrorKind::Network)));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.kind, Some(ErrorKind::Network));
}

#[test]
fn unproven_statuses_win_over_any_native_code() {
    for status in [408, 500, 502, 503, 504] {
        let verdict = classify(status, Some(&int(-1022)), Intent::Place);
        assert!(matches!(verdict, Verdict::Unknown(Some(_))), "{status}");
    }
    let resend = classify(503, Some(&int(-1021)), Intent::Place);
    assert!(!matches!(resend, Verdict::RefusedResend(_)));
    let (_, error) = write(
        &TABLE,
        Intent::Place,
        Released::Response(&response(500)),
        Some(&int(306)),
        "busy",
    );
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.venue_code.as_deref(), Some("306"));
}

#[test]
fn a_decoded_native_code_overrides_the_status_kind() {
    // KuCoin 400002 (clock skew) arrives with HTTP 401.
    let verdict = classify(401, Some(&int(-1021)), Intent::Place);
    assert_eq!(verdict, Verdict::RefusedResend(ErrorKind::AuthInvalid));
    assert_eq!(
        classify(400, Some(&int(-1003)), Intent::Place),
        Verdict::Refused(ErrorKind::RateLimited)
    );
}

#[test]
fn a_bare_status_is_refusal_only_when_the_table_documents_it() {
    assert_eq!(
        classify(429, None, Intent::Place),
        Verdict::Refused(ErrorKind::RateLimited)
    );
    assert_eq!(
        classify(418, None, Intent::Place),
        Verdict::Unknown(Some(ErrorKind::RateLimited))
    );
    assert_eq!(
        classify(401, None, Intent::Place),
        Verdict::Unknown(Some(ErrorKind::AuthInvalid))
    );
    assert_eq!(classify(400, None, Intent::Place), Verdict::Unknown(None));
}

#[test]
fn a_success_status_without_a_native_error_is_a_malformed_ack() {
    assert_eq!(
        classify(200, None, Intent::Place),
        Verdict::Unknown(Some(ErrorKind::Internal))
    );
}

#[test]
fn intent_scoped_rows_win_and_satisfied_never_leaks_to_another_intent() {
    assert_eq!(
        classify(400, Some(&int(-2011)), Intent::Cancel),
        Verdict::Satisfied(SatisfiedIntent::Cancel)
    );
    assert_eq!(
        classify(400, Some(&int(-2011)), Intent::Place),
        Verdict::Refused(ErrorKind::Rejected)
    );
    let prefix = Native {
        code: NativeCode::None,
        message: "Order was never placed, already canceled, or filled",
    };
    let on = |status, native: &Native<'_>, intent| {
        verdict(
            &PREFIX,
            intent,
            Released::Response(&response(status)),
            Some(native),
        )
    };
    assert_eq!(
        on(200, &prefix, Intent::Cancel),
        Verdict::Satisfied(SatisfiedIntent::Cancel)
    );
    // No row for Amend: the code-less envelope is no evidence, so a 2xx stays malformed.
    assert_eq!(
        on(200, &prefix, Intent::Amend),
        Verdict::Unknown(Some(ErrorKind::Internal))
    );
    // Exact match only: an unstripped or foreign suffix is no success evidence.
    let suffixed = Native {
        code: NativeCode::None,
        message: "Order was never placed, already canceled, or filled; reason=other",
    };
    assert_eq!(
        on(200, &suffixed, Intent::Cancel),
        Verdict::Unknown(Some(ErrorKind::Internal))
    );
    assert_table(&PREFIX);
}

#[test]
fn success_codes_and_codeless_envelopes_never_become_refusals() {
    // A venue handing its success code to the classifier proves nothing.
    assert_eq!(
        classify(200, Some(&int(0)), Intent::Place),
        Verdict::Unknown(Some(ErrorKind::Internal))
    );
    // A code-less, unmatched envelope falls through to the status rules.
    let bare = Native {
        code: NativeCode::None,
        message: "<html>",
    };
    assert_eq!(
        classify(429, Some(&bare), Intent::Place),
        Verdict::Refused(ErrorKind::RateLimited)
    );
    assert_eq!(
        classify(400, Some(&bare), Intent::Place),
        Verdict::Unknown(None)
    );
}

#[test]
fn ranges_text_codes_and_unlisted_codes() {
    assert_eq!(
        classify(400, Some(&int(-1111)), Intent::Place),
        Verdict::Refused(ErrorKind::InvalidInput)
    );
    let text = Native {
        code: NativeCode::Text("EOrder:Insufficient funds"),
        message: "",
    };
    assert_eq!(
        classify(200, Some(&text), Intent::Place),
        Verdict::Refused(ErrorKind::Rejected)
    );
    assert_eq!(
        classify(400, Some(&int(-9999)), Intent::Place),
        Verdict::Unknown(None)
    );
}

#[test]
fn strict_integer_decoding_rejects_lookalikes() {
    assert_eq!(decode_int("-1022"), Some(-1022));
    assert_eq!(decode_int("\"40009\""), Some(40009));
    assert_eq!(decode_int("0"), Some(0));
    for token in [
        "40009.0",
        "4.0009e4",
        "+1",
        "01",
        "-0",
        "",
        "-",
        " 1",
        "1 ",
        "\"1",
        "1e3",
        "0x10",
        "99999999999999999999",
    ] {
        assert_eq!(decode_int(token), None, "{token:?}");
    }
}

#[test]
fn errors_keep_the_venue_message_and_native_evidence() {
    let (verdict, error) = write(
        &TABLE,
        Intent::Place,
        Released::Response(&Response {
            status: 400,
            headers: "Retry-After: 2\r\n",
            body: "",
        }),
        Some(&int(-1003)),
        "Too many requests",
    );
    assert_eq!(verdict, Verdict::Refused(ErrorKind::RateLimited));
    assert_eq!(error.message, "Too many requests");
    assert_eq!(error.outcome, Some(Outcome::Rejected));
    assert_eq!(error.venue_code.as_deref(), Some("-1003"));
    assert_eq!(error.venue_message.as_deref(), Some("native message"));
    assert_eq!(error.retry_after_ms, Some(2_000.0));
}

#[test]
fn reads_carry_kind_and_evidence_but_never_an_outcome() {
    let (_, error) = write(
        &TABLE,
        Intent::Read,
        Released::Response(&response(400)),
        Some(&int(-1022)),
        "bad key",
    );
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.outcome, None);
}

#[test]
fn retry_after_is_header_only_and_bounded() {
    let with = |headers: &'static str| Response {
        status: 200,
        headers,
        body: "",
    };
    assert_eq!(retry_after_ms(&with("")), None);
    assert_eq!(retry_after_ms(&with("Retry-After: 0\r\n")), None);
    assert_eq!(retry_after_ms(&with("Retry-After: soon\r\n")), None);
    assert_eq!(
        retry_after_ms(&with("Retry-After: 99999999\r\n")),
        Some(crate::startup::MAX_RATE_LIMIT_PAUSE_MS)
    );
}

#[test]
fn credential_evidence_comes_only_from_auth_refusal_rows() {
    assert_eq!(
        auth_outcome(&TABLE, &int(-1022)),
        Some(AuthOutcome::Rejected)
    );
    assert_eq!(auth_outcome(&TABLE, &int(-1003)), None);
    assert_eq!(auth_outcome(&TABLE, &int(-9999)), None);
}

#[test]
fn read_failures_carry_retry_evidence_and_never_an_outcome() {
    let at = |status, headers: &'static str, native: Option<&Native<'_>>| {
        read(
            &TABLE,
            Released::Response(&Response {
                status,
                headers,
                body: "",
            }),
            native,
            "read failed",
        )
    };
    let refused = at(400, "", Some(&int(-1022)));
    assert_eq!(refused.error.outcome, None);
    assert_eq!(refused.error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(refused.retry, None);
    assert_eq!(
        at(400, "", Some(&int(-1021))).retry,
        Some(ReadRetry::NativeResend)
    );
    // 408 decides before the table: never a native resend.
    assert_eq!(
        at(408, "", Some(&int(-1021))).retry,
        Some(ReadRetry::ReadFailure)
    );
    assert_eq!(
        at(429, "Retry-After: 2\r\n", None).retry,
        Some(ReadRetry::After(2_000.))
    );
    assert_eq!(at(503, "", None).retry, Some(ReadRetry::ReadFailure));
    assert_eq!(
        at(503, "Retry-After: 5\r\n", None).retry,
        Some(ReadRetry::After(5_000.))
    );
    // A rate limit without a header still waits; an auth refusal never retries.
    assert_eq!(
        at(429, "", None).retry,
        Some(ReadRetry::After(DEFAULT_RATE_LIMIT_DEFERRAL_MS))
    );
    assert_eq!(
        at(400, "", Some(&int(-1003))).retry,
        Some(ReadRetry::After(DEFAULT_RATE_LIMIT_DEFERRAL_MS))
    );
    assert_eq!(
        at(418, "", None).retry,
        Some(ReadRetry::After(DEFAULT_RATE_LIMIT_DEFERRAL_MS))
    );
    assert_eq!(at(401, "", None).retry, None);
    let lost = read(&TABLE, Released::Transport, None, "lost");
    assert_eq!(lost.error.kind, Some(ErrorKind::Network));
    assert_eq!(lost.error.outcome, None);
    assert_eq!(lost.retry, Some(ReadRetry::ReadFailure));
}

#[test]
#[should_panic(expected = "overlap")]
fn assert_table_rejects_unpartitioned_overlaps() {
    const BAD: &[Row] = &[
        Row {
            key: Key::Range(-10, -1),
            verdict: Verdict::Unknown(None),
            only: None,
        },
        Row {
            key: Key::Int(-5),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
    ];
    assert_table(&CodeTable {
        rows: BAD,
        success: &[],
        unlisted: Verdict::Unknown(None),
        status: &[],
    });
}

#[test]
#[should_panic(expected = "Satisfied needs")]
fn assert_table_rejects_satisfied_without_its_intent() {
    const BAD: &[Row] = &[Row {
        key: Key::Int(1),
        verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
        only: None,
    }];
    assert_table(&CodeTable {
        rows: BAD,
        success: &[],
        unlisted: Verdict::Unknown(None),
        status: &[],
    });
}

#[test]
#[should_panic(expected = "only documented 4xx")]
fn assert_table_rejects_unproven_status_rows() {
    assert_table(&CodeTable {
        rows: &[],
        success: &[],
        unlisted: Verdict::Unknown(None),
        status: &[(503, Verdict::Refused(ErrorKind::VenueUnavailable))],
    });
}

fn project(items: &[BatchItem<'_>], expected: usize) -> crate::request_context::WriteResult {
    batch(
        &TABLE,
        Intent::Place,
        &response(200),
        expected,
        items,
        "batch failed",
    )
    .unwrap()
}

#[test]
fn batch_projection_owns_count_ids_and_the_fact_kind() {
    use crate::request_context::WriteResult;
    let refused = BatchItem::Native(int(-1111));
    let unknown = BatchItem::Native(int(-9999));
    assert_eq!(
        project(&[BatchItem::Accepted("a"), BatchItem::Accepted("b")], 2),
        WriteResult::Accepted {
            order_ids: vec!["a".into(), "b".into()]
        }
    );
    let WriteResult::KnownPartial(error) = project(&[BatchItem::Accepted("a"), refused], 2) else {
        panic!("accepted plus refused is a known partial");
    };
    let partial = error.partial.unwrap();
    assert_eq!(partial.order_ids, ["a"]);
    assert_eq!((partial.total, partial.failed_index), (Some(2), Some(1)));
    assert!(matches!(
        project(&[refused, refused], 2),
        WriteResult::Rejected(_)
    ));
    let WriteResult::PartialUnknown(error) =
        project(&[refused, BatchItem::Accepted("a"), unknown], 3)
    else {
        panic!("any unknown item makes the batch unknown");
    };
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    let partial = error.partial.unwrap();
    assert_eq!(partial.order_ids, ["a"]);
    assert_eq!(partial.failed_index, Some(0), "first non-accepted position");
    let five = batch(
        &TABLE,
        Intent::Place,
        &response(503),
        1,
        &[BatchItem::Accepted("a")],
        "x",
    );
    assert!(
        matches!(five, Ok(WriteResult::Failed(_))),
        "a 5xx batch acknowledgement proves nothing"
    );
}

#[test]
fn a_short_or_empty_id_batch_ack_is_unknown_and_keeps_evidence_ids() {
    use crate::request_context::WriteResult;
    // Bybit row #3/#4: a short list must never become success or cancellation.
    for items in [
        &[BatchItem::Accepted("a")][..],
        &[][..],
        &[BatchItem::Accepted("a"), BatchItem::Accepted("")][..],
    ] {
        let WriteResult::Failed(error) = project(items, 2) else {
            panic!("count mismatch must be unknown");
        };
        assert_eq!(error.outcome, Some(Outcome::Unknown));
    }
    assert!(batch(&TABLE, Intent::Place, &response(200), 0, &[], "x").is_err());
    assert!(batch(&TABLE, Intent::Read, &response(200), 1, &[], "x").is_err());
}

#[test]
fn satisfied_cancel_items_are_effects_without_ids() {
    use crate::request_context::WriteResult;
    let gone = BatchItem::Native(int(-2011));
    let cancel = |items: &[BatchItem<'_>]| {
        batch(
            &TABLE,
            Intent::Cancel,
            &response(200),
            items.len(),
            items,
            "x",
        )
        .unwrap()
    };
    assert_eq!(
        cancel(&[gone, BatchItem::Accepted("a")]),
        WriteResult::Accepted {
            order_ids: vec!["a".into()]
        }
    );
    assert!(matches!(
        cancel(&[gone, BatchItem::Native(int(-1111))]),
        WriteResult::KnownPartial(_)
    ));
}

#[test]
#[should_panic(expected = "Message rows are only for code-less venues")]
fn assert_table_rejects_message_rows_beside_codes() {
    const BAD: &[Row] = &[
        Row {
            key: Key::Int(1),
            verdict: Verdict::Unknown(None),
            only: None,
        },
        Row {
            key: Key::Message("x"),
            verdict: Verdict::Unknown(None),
            only: None,
        },
    ];
    assert_table(&CodeTable {
        rows: BAD,
        success: &[],
        unlisted: Verdict::Unknown(None),
        status: &[],
    });
}

#[test]
#[should_panic(expected = "cannot be Satisfied or RefusedResend")]
fn assert_table_rejects_resend_without_a_native_code() {
    assert_table(&CodeTable {
        rows: &[],
        success: &[],
        unlisted: Verdict::Unknown(None),
        status: &[(429, Verdict::RefusedResend(ErrorKind::RateLimited))],
    });
}

#[test]
fn unknown_fact_with_proven_effects_preserves_partial_unknown_authority() {
    use crate::request_context::{CommandWrites, Error, WriteResult};
    let ids = vec!["proven-native".to_owned()];
    let fact = fact(
        &TABLE,
        Intent::Cancel,
        Released::Response(&response(200)),
        Some(&int(-9999)),
        "remainder unresolved",
    )
    .for_attempt(crate::RequestId(1))
    .with_order_ids(&ids);
    let supplied = WriteFact::Failed {
        fact,
        known_effects: true,
    };
    assert!(supplied.belongs_to(crate::RequestId(1)));
    assert!(!supplied.belongs_to(crate::RequestId(2)));
    assert!(!supplied.resendable());
    let result = supplied.into_result();
    assert!(
        matches!(result, WriteResult::PartialUnknown(_)),
        "proven effects must remain distinct from evidence-only IDs: {result:?}"
    );
    let mut journal = CommandWrites::new(4).unwrap();
    journal.register(crate::RequestId(1), 1).unwrap();
    journal.released(crate::RequestId(1)).unwrap();
    journal.result(crate::RequestId(1), result).unwrap();
    assert_eq!(journal.validate_effects(crate::RequestId(1)), Ok(()));
    assert_eq!(
        journal.retry(crate::RequestId(1), crate::RequestId(2)),
        Err(Error::RetryForbidden)
    );
    assert_eq!(journal.succeed(), Err(Error::State));
    let terminal = journal.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(terminal.outcome, Some(Outcome::Unknown));
    assert_eq!(terminal.partial.unwrap().order_ids, ids);
}

#[test]
fn batch_proven_subset_plus_unknown_keeps_only_subset_effect_authority() {
    use crate::request_context::WriteResult;
    let unknown = BatchItem::Native(int(-9999));
    let result = project(&[BatchItem::Accepted("proven"), unknown], 2);
    let WriteResult::PartialUnknown(error) = result else {
        panic!("proven subset must remain attributable: {result:?}");
    };
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    let partial = error.partial.unwrap();
    assert_eq!(partial.order_ids, ["proven"]);
    assert_eq!((partial.total, partial.failed_index), (Some(2), Some(1)));
    let gone = BatchItem::Native(int(-2011));
    let result = batch(
        &TABLE,
        Intent::Cancel,
        &response(200),
        2,
        &[gone, unknown],
        "uncertain",
    )
    .unwrap();
    let WriteResult::PartialUnknown(error) = result else {
        panic!("Satisfied is a proven cancel effect even without IDs");
    };
    assert!(
        error.partial.is_none(),
        "the existing error helper omits empty ID metadata"
    );
    assert_eq!(error.outcome, Some(Outcome::Unknown));
}

#[test]
fn wholly_unproven_batch_or_fact_never_borrows_subset_authority() {
    use crate::request_context::{CommandWrites, Error, WriteResult};
    let unknown = BatchItem::Native(int(-9999));
    let fact = fact(
        &TABLE,
        Intent::Place,
        Released::Response(&response(200)),
        Some(&int(-9999)),
        "unknown",
    )
    .for_attempt(crate::RequestId(1))
    .with_order_ids(&["evidence".into()]);
    let mut results = vec![
        WriteFact::Failed {
            fact,
            known_effects: false,
        }
        .into_result(),
        project(&[unknown], 1),
        project(&[BatchItem::Accepted("evidence")], 2),
        project(
            &[BatchItem::Accepted("evidence"), BatchItem::Accepted("")],
            2,
        ),
        batch(
            &TABLE,
            Intent::Place,
            &response(503),
            2,
            &[BatchItem::Accepted("evidence"), unknown],
            "unproven status",
        )
        .unwrap(),
    ];
    for result in results.drain(..) {
        assert!(
            matches!(result, WriteResult::Failed(_)),
            "unproven reply: {result:?}"
        );
        let mut journal = CommandWrites::new(4).unwrap();
        journal.register(crate::RequestId(1), 1).unwrap();
        journal.released(crate::RequestId(1)).unwrap();
        journal.result(crate::RequestId(1), result).unwrap();
        assert_eq!(
            journal.validate_effects(crate::RequestId(1)),
            Err(Error::State)
        );
        assert_eq!(
            journal.fail(VenueError::new("fallback")).unwrap().outcome,
            Some(Outcome::Unknown)
        );
    }
}

#[test]
fn a_cdn_refusal_is_a_block_not_a_bad_key() {
    // CloudFront refusing the egress address answers 403 before the venue
    // sees the request (Bybit), so the key is not the cause.
    let page = "<HTML><HEAD><TITLE>ERROR: The request could not be satisfied</TITLE></HEAD>\
        <BODY>Request blocked. Generated by cloudfront (CloudFront)</BODY></HTML>";
    for status in [401, 403] {
        let blocked = Response {
            status,
            headers: "",
            body: page,
        };
        assert_eq!(
            verdict(&TABLE, Intent::Place, Released::Response(&blocked), None),
            Verdict::Unknown(Some(ErrorKind::RateLimited)),
            "HTTP {status}"
        );
        let failure = read(&TABLE, Released::Response(&blocked), None, "blocked");
        assert_eq!(failure.error.kind, Some(ErrorKind::RateLimited));
        assert!(
            matches!(failure.retry, Some(ReadRetry::After(_))),
            "a block waits like a rate limit"
        );
    }
    // Controls: a bare 403 is still the key, and native evidence still wins.
    assert_eq!(
        classify(403, None, Intent::Place),
        Verdict::Unknown(Some(ErrorKind::AuthPermission))
    );
    let coded = Response {
        status: 403,
        headers: "",
        body: page,
    };
    assert_eq!(
        verdict(
            &TABLE,
            Intent::Place,
            Released::Response(&coded),
            Some(&int(-1022))
        ),
        Verdict::Refused(ErrorKind::AuthInvalid)
    );
}
