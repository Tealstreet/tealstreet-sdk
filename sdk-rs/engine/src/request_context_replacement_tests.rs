use super::*;

fn rejected(message: &str) -> WriteResult {
    WriteResult::Rejected(VenueError::new(message).outcome(Outcome::Rejected))
}

fn accept(writes: &mut CommandWrites, request: u64, id: &str) {
    writes.released(RequestId(request)).unwrap();
    writes
        .result(
            RequestId(request),
            WriteResult::Accepted {
                order_ids: vec![id.into()],
            },
        )
        .unwrap();
}

fn parent(capacity: usize) -> CommandWrites {
    let mut writes = CommandWrites::new(capacity).unwrap();
    writes.register(RequestId(1), 10).unwrap();
    writes.released(RequestId(1)).unwrap();
    writes
        .result(RequestId(1), rejected("batch refusal"))
        .unwrap();
    writes
}

#[test]
fn fully_recovered_replacement_supersedes_only_terminal_parent_failure() {
    let mut writes = parent(4);
    writes.register(RequestId(2), 20).unwrap();
    writes.replace(RequestId(1), &[30, 40]).unwrap();
    assert_eq!(writes.latest(10), None);
    let _ = writes.grant_resend(RequestId(1));
    assert_eq!(
        writes.retry(RequestId(1), RequestId(9)),
        Err(Error::Capacity)
    );
    writes.register(RequestId(3), 30).unwrap();
    writes.register(RequestId(4), 40).unwrap();
    // Physical callback order must not choose terminal projection.
    accept(&mut writes, 4, "a-second");
    accept(&mut writes, 2, "b");
    accept(&mut writes, 3, "a-first");
    assert_eq!(writes.reports().count(), 0);
    assert_eq!(writes.rejections().count(), 0);
    assert!(writes.attempts[0].released);
    assert!(matches!(
        writes.attempts[0].result,
        Some(WriteResult::Rejected(_))
    ));
    assert_eq!(writes.succeed(), Ok(()));
}

#[test]
fn nested_replacements_keep_parent_position_before_already_issued_siblings() {
    for reverse in [false, true] {
        let mut writes = parent(6);
        writes.register(RequestId(2), 20).unwrap();
        writes.replace(RequestId(1), &[30, 40]).unwrap();
        writes.register(RequestId(3), 30).unwrap();
        writes.register(RequestId(4), 40).unwrap();
        writes.released(RequestId(4)).unwrap();
        writes
            .result(RequestId(4), rejected("nested refusal"))
            .unwrap();
        writes.replace(RequestId(4), &[50, 60]).unwrap();
        writes.register(RequestId(5), 50).unwrap();
        writes.register(RequestId(6), 60).unwrap();
        let mut callbacks = [2, 3, 5, 6];
        if reverse {
            callbacks.reverse();
        }
        for id in callbacks {
            writes.released(RequestId(id)).unwrap();
            let fact = match id {
                2 => WriteResult::KnownPartial(
                    VenueError::new("sibling refusal").partial_ids(&["b-known".into()]),
                ),
                3 => WriteResult::Accepted {
                    order_ids: vec!["a-first".into()],
                },
                5 => WriteResult::Accepted {
                    order_ids: vec!["a-nested".into()],
                },
                _ => rejected("nested leaf refusal"),
            };
            writes.result(RequestId(id), fact).unwrap();
        }
        let error = writes.fail(VenueError::new("fallback")).unwrap();
        assert_eq!(error.outcome, None);
        assert_eq!(error.message, "nested leaf refusal");
        assert_eq!(
            error.partial.unwrap().order_ids,
            ["a-first", "a-nested", "b-known"]
        );
        assert_eq!(
            writes.reports().collect::<Vec<_>>(),
            ["nested leaf refusal", "sibling refusal"]
        );
        assert_eq!(
            writes.rejections().map(|(id, _)| id.0).collect::<Vec<_>>(),
            [6, 2]
        );
    }
}

#[test]
fn reserved_unissued_children_cannot_disappear_from_success_or_unknown_sealing() {
    for queued in [false, true] {
        let mut writes = parent(3);
        writes.replace(RequestId(1), &[30, 40]).unwrap();
        if queued {
            writes.register(RequestId(3), 30).unwrap();
            accept(&mut writes, 3, "known");
        }
        assert_eq!(writes.succeed(), Err(Error::State));
        let error = writes
            .fail(VenueError::new("unissued child deadline"))
            .unwrap();
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(
            error.partial.map(|partial| partial.order_ids),
            queued.then(|| vec!["known".into()])
        );
    }
}

#[test]
fn replacement_provider_refusal_retains_ancestor_release_and_accepted_ids() {
    let mut writes = parent(3);
    writes.replace(RequestId(1), &[30, 40]).unwrap();
    writes.register(RequestId(3), 30).unwrap();
    writes.register(RequestId(4), 40).unwrap();
    accept(&mut writes, 3, "known");
    writes
        .result(
            RequestId(4),
            WriteResult::Failed(VenueError::new("provider unavailable").outcome(Outcome::NotSent)),
        )
        .unwrap();
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["known"]);
}

#[test]
fn replacement_reservation_is_atomic_and_counts_against_physical_retry_capacity() {
    let mut writes = parent(3);
    for (children, expected) in [
        (vec![], Error::Invalid),
        (vec![30, 30], Error::Duplicate),
        (vec![10], Error::Duplicate),
        (vec![30, 40, 50], Error::Capacity),
    ] {
        assert_eq!(writes.replace(RequestId(1), &children), Err(expected));
        assert_eq!(writes.latest(10), Some(RequestId(1)));
        assert_eq!(writes.occupied(), 1);
        assert_eq!(writes.legs.len(), 1);
    }
    writes.replace(RequestId(1), &[30, 40]).unwrap();
    assert_eq!(writes.occupied(), 3);
    assert_eq!(writes.register(RequestId(9), 90), Err(Error::Capacity));
    // Reserved registration must work at capacity without charging twice.
    writes.register(RequestId(3), 30).unwrap();
    writes.released(RequestId(3)).unwrap();
    writes
        .result(RequestId(3), rejected("child refusal"))
        .unwrap();
    let _ = writes.grant_resend(RequestId(3));
    assert_eq!(
        writes.retry(RequestId(3), RequestId(5)),
        Err(Error::Capacity)
    );
    writes.register(RequestId(4), 40).unwrap();
    assert_eq!(
        writes.replace(RequestId(1), &[50]),
        Err(Error::RetryForbidden)
    );
    assert_eq!(writes.latest(10), None);
}

#[test]
fn only_latest_effects_free_released_refusal_can_be_replaced() {
    for fact in [
        WriteResult::Accepted {
            order_ids: vec!["accepted".into()],
        },
        WriteResult::KnownPartial(VenueError::new("known effects")),
        WriteResult::Failed(VenueError::new("lost reply")),
        WriteResult::Rejected(VenueError::new("partial refusal").partial_ids(&["known".into()])),
        WriteResult::Rejected(
            VenueError::new("oversize identity").partial_ids(&["x".repeat(1025)]),
        ),
    ] {
        let mut writes = CommandWrites::new(4).unwrap();
        writes.register(RequestId(1), 10).unwrap();
        writes.released(RequestId(1)).unwrap();
        writes.result(RequestId(1), fact).unwrap();
        assert_eq!(
            writes.replace(RequestId(1), &[30]),
            Err(Error::RetryForbidden)
        );
        assert_eq!(writes.latest(10), Some(RequestId(1)));
    }
    let mut writes = parent(4);
    let _ = writes.grant_resend(RequestId(1));
    writes.retry(RequestId(1), RequestId(2)).unwrap();
    assert_eq!(
        writes.replace(RequestId(1), &[30]),
        Err(Error::RetryForbidden)
    );
    assert_eq!(writes.latest(10), Some(RequestId(2)));
    assert_eq!(
        writes.replace(RequestId(2), &[30]),
        Err(Error::RetryForbidden)
    );
    assert_eq!(
        writes
            .fail(VenueError::new("retry parked"))
            .unwrap()
            .outcome,
        Some(Outcome::Unknown)
    );
    assert_eq!(writes.replace(RequestId(1), &[30]), Err(Error::State));
}

#[test]
fn nested_single_child_groups_remain_bounded_without_packed_order_overflow() {
    let mut writes = parent(32);
    let mut previous = RequestId(1);
    for request in 2..=32 {
        let leg = u64::MAX - request;
        writes.replace(previous, &[leg]).unwrap();
        writes.register(RequestId(request), leg).unwrap();
        writes.released(RequestId(request)).unwrap();
        writes
            .result(RequestId(request), rejected("still refused"))
            .unwrap();
        previous = RequestId(request);
    }
    assert_eq!(writes.replace(previous, &[99]), Err(Error::Capacity));
    assert_eq!(writes.occupied(), 32);
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, Some(Outcome::Rejected));
    assert_eq!(writes.reports().collect::<Vec<_>>(), ["still refused"]);
}

#[test]
fn retried_ancestors_and_replacement_children_keep_physical_send_history() {
    for completion in ["parked", "provider", "rejected", "accepted"] {
        let mut writes = parent(9);
        let _ = writes.grant_resend(RequestId(1));
        writes.retry(RequestId(1), RequestId(2)).unwrap();
        writes.released(RequestId(2)).unwrap();
        writes
            .result(RequestId(2), rejected("parent retry refused"))
            .unwrap();
        writes.replace(RequestId(2), &[30]).unwrap();
        let _ = writes.grant_resend(RequestId(2));
        assert_eq!(
            writes.retry(RequestId(2), RequestId(8)),
            Err(Error::RetryForbidden)
        );
        assert_eq!(
            writes.replace(RequestId(2), &[40]),
            Err(Error::RetryForbidden)
        );
        writes.register(RequestId(3), 30).unwrap();
        writes.released(RequestId(3)).unwrap();
        writes
            .result(RequestId(3), rejected("child refusal"))
            .unwrap();
        let _ = writes.grant_resend(RequestId(3));
        writes.retry(RequestId(3), RequestId(4)).unwrap();
        match completion {
            "provider" => writes
                .result(
                    RequestId(4),
                    WriteResult::Failed(
                        VenueError::new("provider unavailable").outcome(Outcome::NotSent),
                    ),
                )
                .unwrap(),
            "rejected" => {
                writes.released(RequestId(4)).unwrap();
                writes
                    .result(RequestId(4), rejected("terminal native refusal"))
                    .unwrap();
            }
            "accepted" => accept(&mut writes, 4, "final-native-id"),
            _ => {}
        }
        assert!(
            writes.occupied() < 9,
            "capacity must not mask revoked authority"
        );
        if completion == "accepted" {
            assert_eq!(writes.reports().count(), 0);
            assert_eq!(writes.succeed(), Ok(()));
        } else {
            let error = writes
                .fail(VenueError::new("child retry did not complete"))
                .unwrap();
            assert_eq!(
                error.outcome,
                Some(if completion == "rejected" {
                    Outcome::Rejected
                } else {
                    Outcome::Unknown
                })
            );
        }
    }
}
