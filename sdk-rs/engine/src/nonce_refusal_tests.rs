use super::*;
use crate::request_context::{CommandWrites, Error, WriteResult};

// Only this exact native code proves a nonce refusal. No venue table adopts it.
const TABLE: CodeTable = CodeTable {
    rows: &[Row {
        key: Key::Text("invalidNonce"),
        verdict: Verdict::NonceRefused,
        only: None,
    }],
    success: &[],
    unlisted: Verdict::Unknown(None),
    status: &[],
};

fn refusal(status: u16) -> Fact {
    fact(
        &TABLE,
        Intent::Place,
        Released::Response(&Response {
            status,
            headers: "",
            body: "invalidNonce",
        }),
        Some(&Native {
            code: NativeCode::Text("invalidNonce"),
            message: "invalid nonce",
        }),
        "invalid nonce",
    )
    .for_attempt(crate::RequestId(2))
}

#[test]
fn definitive_nonce_refusal_is_not_sent_and_grants_no_resend() {
    let fact = refusal(200);
    assert_eq!(fact.error().outcome, Some(Outcome::NotSent));
    let write = WriteFact::Failed {
        fact,
        known_effects: false,
    };
    assert!(!write.resendable());
    let mut journal = CommandWrites::new(4).unwrap();
    journal.register(crate::RequestId(2), 2).unwrap();
    journal.released(crate::RequestId(2)).unwrap();
    journal
        .result(crate::RequestId(2), write.into_result())
        .unwrap();
    assert_eq!(
        journal.retry(crate::RequestId(2), crate::RequestId(3)),
        Err(Error::RetryForbidden)
    );
    assert_eq!(
        journal.fail(VenueError::new("terminal")).unwrap().outcome,
        Some(Outcome::NotSent)
    );
}

#[test]
fn unproven_nonce_responses_and_transport_stay_unknown() {
    for status in [408, 500, 502, 503, 504] {
        assert_eq!(refusal(status).error().outcome, Some(Outcome::Unknown));
    }
    let lost = fact(&TABLE, Intent::Place, Released::Transport, None, "lost");
    assert_eq!(lost.error().outcome, Some(Outcome::Unknown));
}

#[test]
fn a_later_nonce_refusal_cannot_erase_an_earlier_unknown() {
    let mut journal = CommandWrites::new(4).unwrap();
    journal.register(crate::RequestId(1), 1).unwrap();
    journal.released(crate::RequestId(1)).unwrap();
    journal
        .result(
            crate::RequestId(1),
            WriteResult::Failed(VenueError::new("lost").outcome(Outcome::Unknown)),
        )
        .unwrap();
    journal.register(crate::RequestId(2), 2).unwrap();
    journal.released(crate::RequestId(2)).unwrap();
    journal
        .result(
            crate::RequestId(2),
            WriteFact::Failed {
                fact: refusal(200),
                known_effects: false,
            }
            .into_result(),
        )
        .unwrap();
    assert_eq!(
        journal.fail(VenueError::new("terminal")).unwrap().outcome,
        Some(Outcome::Unknown)
    );
}

#[test]
fn no_current_venue_constructs_the_nonce_refusal_verdict() {
    fn visit(path: &std::path::Path, count: &mut usize) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, count);
            } else if path.extension().is_some_and(|e| e == "rs") {
                *count += 1;
                let source = std::fs::read_to_string(&path).unwrap();
                assert!(
                    !source.contains("NonceRefused"),
                    "new nonce verdict in {}",
                    path.display()
                );
            }
        }
    }
    let mut count = 0;
    visit(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../venues"),
        &mut count,
    );
    assert!(count > 500, "the complete venue source tree was inspected");
}

#[test]
fn nonce_refusal_requires_an_exact_native_code_not_a_range_default_or_status() {
    let native = Native {
        code: NativeCode::Text("anything"),
        message: "invalid nonce",
    };
    let mut table = TABLE;
    table.rows = &[];
    table.unlisted = Verdict::NonceRefused;
    table.status = &[(400, Verdict::NonceRefused)];
    let reply = Response {
        status: 400,
        headers: "",
        body: "",
    };
    assert!(matches!(
        verdict(
            &table,
            Intent::Place,
            Released::Response(&reply),
            Some(&native)
        ),
        Verdict::Unknown(_)
    ));
    assert!(matches!(
        verdict(&table, Intent::Place, Released::Response(&reply), None),
        Verdict::Unknown(_)
    ));
    table.rows = &[Row {
        key: Key::Range(1, 10),
        verdict: Verdict::NonceRefused,
        only: None,
    }];
    let native = Native {
        code: NativeCode::Int(5),
        message: "invalid nonce",
    };
    assert!(matches!(
        verdict(
            &table,
            Intent::Place,
            Released::Response(&reply),
            Some(&native)
        ),
        Verdict::Unknown(_)
    ));
}

#[cfg(feature = "serial-plans")]
#[test]
fn compacted_serial_history_preserves_rejections_and_known_effects_beside_nonce_not_sent() {
    for (first, expected) in [
        (
            WriteResult::Rejected(refusal(200).error().clone()),
            Some(Outcome::NotSent),
        ),
        (
            WriteResult::Rejected(VenueError::new("native rejection").outcome(Outcome::Rejected)),
            Some(Outcome::Rejected),
        ),
        (
            WriteResult::Accepted {
                order_ids: vec!["prior-native".into()],
            },
            None,
        ),
    ] {
        let mut journal = CommandWrites::new(4).unwrap().streamed_with(true);
        journal.register(crate::RequestId(1), 1).unwrap();
        journal.released(crate::RequestId(1)).unwrap();
        journal.result(crate::RequestId(1), first).unwrap();
        journal.validate_serial_continuation(Some(1)).unwrap();
        journal.compact_serial(1);
        journal.register(crate::RequestId(2), 2).unwrap();
        journal.released(crate::RequestId(2)).unwrap();
        journal
            .result(
                crate::RequestId(2),
                WriteFact::Failed {
                    fact: refusal(200),
                    known_effects: false,
                }
                .into_result(),
            )
            .unwrap();
        let terminal = journal.fail(VenueError::new("terminal")).unwrap();
        assert_eq!(terminal.outcome, expected);
        if expected.is_none() {
            assert_eq!(terminal.partial.unwrap().order_ids, ["prior-native"]);
        }
    }
}
