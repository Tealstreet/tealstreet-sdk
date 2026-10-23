use super::*;

fn context(operation: Operation, pacing: Option<Pacing>) -> Context {
    Context::new(
        Owner::Command {
            id: 7,
            generation: 19,
        },
        3,
        operation,
        Some(Credential {
            slot: CredentialSlot::Primary,
            reference: KeyRef("account-primary".into()),
        }),
        0.,
        5_000.,
        pacing,
    )
    .unwrap()
}
fn lifecycle(id: u64, pacing: Option<Pacing>) -> Lifecycle {
    Lifecycle::new(
        context(Operation::Mutation, pacing),
        Attempt {
            request: RequestId(id),
            generation: 1,
        },
    )
}
fn ready(life: &mut Lifecycle) -> Permit {
    life.prepared(
        life.preparation(),
        Prepared {
            id: 4,
            expires_at: None,
        },
    )
    .unwrap();
    life.unsigned().unwrap();
    life.permit().unwrap()
}
fn clock(raw_ms: f64) -> ReleaseTime {
    ReleaseTime {
        raw_ms,
        protocol_ms: raw_ms + 10_000.,
        revision: 2,
    }
}
fn admission(_: &HttpRelease) -> Result<Admission, Error> {
    Ok(Admission::Unscheduled)
}
fn claim(
    life: &mut Lifecycle,
    permit: Permit,
    at: f64,
    pacing: &mut SendPacing,
) -> Result<Claim, Error> {
    life.claim(permit, clock(at), pacing, &mut admission)
}
fn released(claim: Claim) -> HttpRelease {
    match claim {
        Claim::Released(release) => release,
        other => panic!("not released: {other:?}"),
    }
}

fn bounded_lifecycle(id: u64, pacing: Option<Pacing>, deadline: f64) -> Lifecycle {
    Lifecycle::with_deadline(
        context(Operation::Mutation, pacing),
        Attempt {
            request: RequestId(id),
            generation: 1,
        },
        0.,
        deadline,
    )
    .unwrap()
}

#[test]
fn physical_window_may_only_narrow_a_live_logical_bound() {
    let logical = context(Operation::Mutation, None);
    let attempt = Attempt {
        request: RequestId(1),
        generation: 1,
    };
    for (created_at, deadline) in [
        (0., 0.),
        (100., 99.),
        (0., 5_001.),
        (5_000., 5_000.),
        (f64::NAN, 100.),
        (f64::NEG_INFINITY, 100.),
        (0., f64::NAN),
        (0., f64::INFINITY),
    ] {
        assert!(matches!(
            Lifecycle::with_deadline(logical.clone(), attempt, created_at, deadline),
            Err(Error::Invalid)
        ));
    }
    assert_eq!(Lifecycle::new(logical.clone(), attempt).deadline(), 5_000.);
    let equal = Lifecycle::with_deadline(logical.clone(), attempt, 0., 5_000.).unwrap();
    assert_eq!(equal.deadline(), 5_000.);
    let narrow = Lifecycle::with_deadline(logical.clone(), attempt, 0., 100.).unwrap();
    assert_eq!(narrow.deadline(), 100.);
    assert_eq!(narrow.context(), &logical);
}

#[test]
fn physical_deadline_precedes_logical_deadline_for_release_and_expiry() {
    for at in [100., 101., 4_999.] {
        let mut life = bounded_lifecycle(1, None, 100.);
        let permit = ready(&mut life);
        let mut calls = 0;
        assert_eq!(
            life.claim(permit, clock(at), &mut SendPacing::default(), &mut |_| {
                calls += 1;
                Ok(Admission::Unscheduled)
            }),
            Err(Error::TimedOut)
        );
        assert_eq!(calls, 0);
        assert!(life.release().is_none());
        assert!(!life.accepts_response());
    }
    let mut life = bounded_lifecycle(1, None, 100.);
    let permit = ready(&mut life);
    let release = released(claim(&mut life, permit, 99., &mut SendPacing::default()).unwrap());
    assert_eq!(release.deadline, 100.);
    assert_eq!(life.context().deadline(), 5_000.);
    assert!(!life.expire(99.).unwrap());
    assert!(life.expire(100.).unwrap());
    assert!(!life.expire(101.).unwrap());
    assert_eq!(life.release(), Some(release));
    assert!(!life.accepts_response());
}

#[test]
fn route_reprepare_cannot_renew_a_physical_window() {
    let mut life = bounded_lifecycle(1, None, 100.);
    let old_token = life.preparation();
    let old_permit = ready(&mut life);
    life.reprepare().unwrap();
    assert_eq!(life.deadline(), 100.);
    assert_eq!(life.context().deadline(), 5_000.);
    assert_eq!(
        life.prepared(
            old_token,
            Prepared {
                id: 1,
                expires_at: None
            }
        ),
        Err(Error::Revoked)
    );
    let current = ready(&mut life);
    assert_eq!(
        claim(&mut life, old_permit, 50., &mut SendPacing::default()),
        Err(Error::Revoked)
    );
    assert_eq!(
        claim(&mut life, current, 100., &mut SendPacing::default()),
        Err(Error::TimedOut)
    );
    assert!(life.expire(100.).unwrap());
}

#[test]
fn physical_pacing_refusal_does_not_consume_admission_or_the_next_slot() {
    let gap = Pacing {
        group: 9,
        minimum_gap_ms: 100.,
    };
    let mut pacing = SendPacing::new(&[gap]).unwrap();
    let mut first = lifecycle(1, Some(gap));
    let permit = ready(&mut first);
    claim(&mut first, permit, 10., &mut pacing).unwrap();
    let mut short = bounded_lifecycle(2, Some(gap), 110.);
    let permit = ready(&mut short);
    let mut calls = 0;
    assert_eq!(
        short.claim(permit, clock(11.), &mut pacing, &mut |_| {
            calls += 1;
            Ok(Admission::Unscheduled)
        }),
        Err(Error::TimedOut)
    );
    assert_eq!(calls, 0);
    let mut peer = lifecycle(3, Some(gap));
    let permit = ready(&mut peer);
    assert_eq!(
        claim(&mut peer, permit, 11., &mut pacing),
        Ok(Claim::WaitUntil(110.))
    );
    assert!(claim(&mut peer, permit, 110., &mut pacing).is_ok());
}

#[test]
fn parked_signing_metadata_cannot_extend_the_physical_cap() {
    for (route_expiry, freshness, revision) in [
        (4_000., 4_000., 2),
        (100., 4_000., 2),
        (4_000., 100., 2),
        (4_000., 4_000., 1),
    ] {
        let mut life = bounded_lifecycle(1, None, 100.);
        life.prepared(
            life.preparation(),
            Prepared {
                id: 4,
                expires_at: Some(route_expiry),
            },
        )
        .unwrap();
        life.begin_signing().unwrap();
        life.signed(
            life.preparation(),
            SignReceipt::default(),
            revision,
            freshness,
        )
        .unwrap();
        let permit = life.permit().unwrap();
        assert_eq!(
            claim(&mut life, permit, 100., &mut SendPacing::default()),
            Err(Error::TimedOut),
            "physical expiry wins over simultaneous stale metadata"
        );
        assert_eq!(life.deadline(), 100.);
        assert!(life.release().is_none());
    }
}

#[test]
fn physical_window_caps_signature_projection_and_scheduler_response_lease() {
    let mut life = bounded_lifecycle(1, None, 100.);
    life.prepared(
        life.preparation(),
        Prepared {
            id: 4,
            expires_at: Some(4_000.),
        },
    )
    .unwrap();
    life.begin_signing().unwrap();
    life.signed(
        life.preparation(),
        SignReceipt {
            stamp_ms: Some(10_000.),
            expiry_ms: Some(100_000.),
        },
        2,
        4_000.,
    )
    .unwrap();
    let permit = life.permit().unwrap();
    let release = released(
        life.claim(
            permit,
            clock(99.),
            &mut SendPacing::default(),
            &mut |release| release.response_lease(1_000.).map(Admission::Scheduled),
        )
        .unwrap(),
    );
    assert_eq!(release.deadline, 100.);
    assert_eq!(release.signature_valid_until, Some(100.));
    assert_eq!(release.response_lease, Some(100.));
}

#[test]
fn read_semantics_do_not_implicitly_authorize_retries() {
    // No Method is accepted here: both POST reads and GET mutations need an
    // explicit semantic class and a separate retry policy.
    let read = context(Operation::Read, None);
    assert_eq!(
        read.retry(RetryEvidence::ReadFailure, 1.),
        Err(Error::RetryForbidden)
    );
    let read = read.with_retry(RetryPolicy::ReadFailure).unwrap();
    let retry = read.retry(RetryEvidence::ReadFailure, 4_999.).unwrap();
    assert_eq!(retry, read);
    assert_eq!(retry.deadline(), 5_000.);
    assert_eq!(
        read.retry(RetryEvidence::ReadFailure, 5_000.),
        Err(Error::TimedOut)
    );
    assert_eq!(
        read.retry(RetryEvidence::DefinitiveRefusal, 1.),
        Err(Error::RetryForbidden)
    );
    assert_eq!(
        context(Operation::Mutation, None).with_retry(RetryPolicy::ReadFailure),
        Err(Error::RetryForbidden)
    );
}

#[test]
fn invalid_context_never_enters_the_lifecycle() {
    for deadline in [0., -1., f64::NAN, f64::INFINITY] {
        assert_eq!(
            Context::new(
                Owner::Background { generation: 1 },
                0,
                Operation::Read,
                None,
                0.,
                deadline,
                None
            ),
            Err(Error::Invalid)
        );
    }
    assert_eq!(
        Context::new(
            Owner::Background { generation: 1 },
            0,
            Operation::Read,
            None,
            0.,
            1.,
            Some(Pacing {
                group: 1,
                minimum_gap_ms: f64::NAN
            })
        ),
        Err(Error::Invalid)
    );
}

#[test]
fn route_and_signing_readiness_are_not_release_or_response_permission() {
    let mut life = lifecycle(1, None);
    assert!(!life.accepts_response());
    life.prepared(
        life.preparation(),
        Prepared {
            id: 9,
            expires_at: Some(1_000.),
        },
    )
    .unwrap();
    life.begin_signing().unwrap();
    assert!(!life.accepts_response());
    assert!(life.release().is_none());
    life.signed(
        life.preparation(),
        SignReceipt {
            stamp_ms: Some(10_000.),
            expiry_ms: None,
        },
        2,
        1_000.,
    )
    .unwrap();
    let permit = life.permit().unwrap();
    assert!(!life.accepts_response());
    let result = claim(&mut life, permit, 999., &mut SendPacing::new(&[]).unwrap()).unwrap();
    assert_eq!(released(result).time, clock(999.));
    assert!(life.accepts_response());
    assert!(life.settle());
    assert!(!life.accepts_response());
    assert!(!life.settle());
    assert!(life.release().is_some()); // retained for reconciliation only
}

#[test]
fn delayed_permit_revalidates_deadline_route_revision_and_freshness_before_admission() {
    for (time, expiry, fresh_until, revision, expected) in [
        (5_000., 1., 1., 1, Error::TimedOut),
        (500., 500., 900., 2, Error::Stale),
        (500., 900., 500., 2, Error::Stale),
        (500., 900., 900., 3, Error::Stale),
    ] {
        let mut life = lifecycle(1, None);
        life.prepared(
            life.preparation(),
            Prepared {
                id: 9,
                expires_at: Some(expiry),
            },
        )
        .unwrap();
        life.begin_signing().unwrap();
        life.signed(
            life.preparation(),
            SignReceipt::default(),
            revision,
            fresh_until,
        )
        .unwrap();
        let permit = life.permit().unwrap();
        let mut calls = 0;
        let result = life.claim(
            permit,
            clock(time),
            &mut SendPacing::new(&[]).unwrap(),
            &mut |_| {
                calls += 1;
                Ok(Admission::Unscheduled)
            },
        );
        assert_eq!(result, Err(expected));
        assert_eq!(calls, 0);
        assert!(life.release().is_none());
    }
}

#[test]
fn protocol_expiry_projects_onto_raw_time_for_both_offset_signs() {
    for offset in [-5_000., 5_000.] {
        let mut life = lifecycle(1, None);
        life.prepared(
            life.preparation(),
            Prepared {
                id: 1,
                expires_at: None,
            },
        )
        .unwrap();
        life.begin_signing().unwrap();
        let protocol_ms = 2_000. + offset;
        // Receipt protocol UTC must remain nonnegative; use a positive base.
        let protocol_ms = protocol_ms + 10_000.;
        life.signed(
            life.preparation(),
            SignReceipt {
                stamp_ms: Some(protocol_ms),
                expiry_ms: Some(protocol_ms + 300.),
            },
            2,
            4_000.,
        )
        .unwrap();
        let permit = life.permit().unwrap();
        let result = life
            .claim(
                permit,
                ReleaseTime {
                    raw_ms: 2_000.,
                    protocol_ms,
                    revision: 2,
                },
                &mut SendPacing::new(&[]).unwrap(),
                &mut admission,
            )
            .unwrap();
        let release = released(result);
        assert_eq!(release.signature_valid_until, Some(2_300.));
        assert_eq!(release.deadline, 5_000.);
    }
}

#[test]
fn lifecycle_and_response_lease_cap_signature_expiry_projection() {
    let mut life = lifecycle(1, None);
    life.prepared(
        life.preparation(),
        Prepared {
            id: 1,
            expires_at: None,
        },
    )
    .unwrap();
    life.begin_signing().unwrap();
    life.signed(
        life.preparation(),
        SignReceipt {
            stamp_ms: Some(10_000.),
            expiry_ms: Some(100_000.),
        },
        2,
        6_000.,
    )
    .unwrap();
    let permit = life.permit().unwrap();
    let result = life
        .claim(
            permit,
            clock(100.),
            &mut SendPacing::new(&[]).unwrap(),
            &mut |release| release.response_lease(900.).map(Admission::Scheduled),
        )
        .unwrap();
    let release = released(result);
    assert_eq!(release.response_lease, Some(900.));
    assert_eq!(release.deadline, 5_000.);

    let mut other = lifecycle(2, None);
    let permit = ready(&mut other);
    assert_eq!(
        released(claim(&mut other, permit, 100., &mut SendPacing::new(&[]).unwrap()).unwrap())
            .deadline,
        5_000.
    );
}

#[test]
fn protocol_expiry_at_release_is_stale_and_raw_timeout_wins() {
    for (at, expected) in [(100., Error::Stale), (5_000., Error::TimedOut)] {
        let mut life = lifecycle(1, None);
        life.prepared(
            life.preparation(),
            Prepared {
                id: 1,
                expires_at: None,
            },
        )
        .unwrap();
        life.begin_signing().unwrap();
        life.signed(
            life.preparation(),
            SignReceipt {
                stamp_ms: None,
                expiry_ms: Some(10_100.),
            },
            2,
            6_000.,
        )
        .unwrap();
        let permit = life.permit().unwrap();
        assert_eq!(
            claim(&mut life, permit, at, &mut SendPacing::new(&[]).unwrap()),
            Err(expected)
        );
    }
}

#[test]
fn cancellation_and_single_use_permits_fence_late_and_duplicate_handoffs() {
    let mut first = lifecycle(1, None);
    let old = ready(&mut first);
    first.reprepare().unwrap();
    let fresh = ready(&mut first);
    let mut pacing = SendPacing::new(&[]).unwrap();
    assert_eq!(
        claim(&mut first, old, 10., &mut pacing),
        Err(Error::Revoked)
    );
    assert!(claim(&mut first, fresh, 10., &mut pacing).is_ok());
    assert_eq!(
        claim(&mut first, fresh, 10., &mut pacing),
        Err(Error::State)
    );
    let mut second = lifecycle(2, None);
    let permit = ready(&mut second);
    assert_eq!(
        claim(&mut second, fresh, 10., &mut pacing),
        Err(Error::Revoked)
    );
    assert!(second.settle());
    assert_eq!(
        claim(&mut second, permit, 10., &mut pacing),
        Err(Error::State)
    );
    assert!(!second.accepts_response());
}

#[test]
fn parked_signing_delayed_route_and_fast_ack_do_not_consume_or_erase_send_gap() {
    let gap = Pacing {
        group: 7,
        minimum_gap_ms: 450.,
    };
    let mut pacing = SendPacing::new(&[gap]).unwrap();
    let mut first = lifecycle(1, Some(gap));
    // Route and signer can remain parked longer than the gap. Only the claim
    // at 1000 starts pacing; settling a fast ACK must not erase that mark.
    let permit = ready(&mut first);
    assert_eq!(
        released(claim(&mut first, permit, 1_000., &mut pacing).unwrap())
            .time
            .raw_ms,
        1_000.
    );
    first.settle();
    let mut second = lifecycle(2, Some(gap));
    let permit = ready(&mut second);
    assert_eq!(
        claim(&mut second, permit, 1_001., &mut pacing),
        Ok(Claim::WaitUntil(1_450.))
    );
    assert_eq!(
        claim(&mut second, permit, 1_449., &mut pacing),
        Ok(Claim::WaitUntil(1_450.))
    );
    assert_eq!(
        released(claim(&mut second, permit, 1_450., &mut pacing).unwrap())
            .time
            .raw_ms,
        1_450.
    );
}

#[test]
fn concurrent_ready_claims_share_pacing_and_denial_consumes_neither_slot_nor_admission() {
    let gap = Pacing {
        group: 1,
        minimum_gap_ms: 450.,
    };
    let mut pacing = SendPacing::new(&[gap]).unwrap();
    let mut first = lifecycle(1, Some(gap));
    let p1 = ready(&mut first);
    let mut second = lifecycle(2, Some(gap));
    let p2 = ready(&mut second);
    assert_eq!(
        first.claim(p1, clock(10.), &mut pacing, &mut |_| Err(Error::Revoked)),
        Err(Error::Revoked)
    );
    assert!(claim(&mut second, p2, 10., &mut pacing).is_ok());
    assert_eq!(claim(&mut first, p1, 10., &mut pacing), Err(Error::State));
    let mut third = lifecycle(3, Some(gap));
    let p3 = ready(&mut third);
    let mut admissions = 0;
    assert_eq!(
        third.claim(p3, clock(10.), &mut pacing, &mut |_| {
            admissions += 1;
            Ok(Admission::Unscheduled)
        }),
        Ok(Claim::WaitUntil(460.))
    );
    assert_eq!(admissions, 0);
}

#[test]
fn pacing_wait_cannot_extend_original_lifecycle_or_signing_freshness() {
    let gap = Pacing {
        group: 1,
        minimum_gap_ms: 6_000.,
    };
    let mut pacing = SendPacing::new(&[gap]).unwrap();
    let mut first = lifecycle(1, Some(gap));
    let permit = ready(&mut first);
    claim(&mut first, permit, 1., &mut pacing).unwrap();
    let mut second = lifecycle(2, Some(gap));
    let permit = ready(&mut second);
    assert_eq!(
        claim(&mut second, permit, 2., &mut pacing),
        Err(Error::TimedOut)
    );
    assert_eq!(SendPacing::new(&[gap, gap]).err(), Some(Error::Duplicate));
}

fn failure(message: &str) -> VenueError {
    VenueError::new(message)
}
fn failed(message: &str) -> WriteResult {
    WriteResult::Failed(failure(message))
}
fn rejected(message: &str) -> WriteResult {
    WriteResult::Rejected(failure(message))
}
fn seal(writes: &mut CommandWrites) -> VenueError {
    writes.fail(failure("fallback")).unwrap()
}

#[test]
fn unsent_sibling_cannot_choose_error_or_outcome_over_sent_native_refusal() {
    for callback_order in [[1, 2], [2, 1]] {
        let mut writes = CommandWrites::new(4).unwrap();
        writes.register(RequestId(1), 1).unwrap();
        writes.register(RequestId(2), 2).unwrap();
        writes.released(RequestId(2)).unwrap();
        for id in callback_order {
            writes
                .result(
                    RequestId(id),
                    if id == 1 {
                        failed("signer refused")
                    } else {
                        rejected("native refused")
                    },
                )
                .unwrap();
        }
        let error = seal(&mut writes);
        assert_eq!(error.outcome, Some(Outcome::Rejected));
        assert_eq!(error.message, "native refused");
    }
}

#[test]
fn earlier_released_retryable_refusal_then_failed_retry_is_unknown_for_every_failure_cause() {
    for code in ["10004", "10002"] {
        for cause in ["provider refused", "network lost; error sign"] {
            for released_retry in [false, true] {
                let mut writes = CommandWrites::new(3).unwrap();
                writes.register(RequestId(1), 5).unwrap();
                writes.released(RequestId(1)).unwrap();
                writes
                    .result(
                        RequestId(1),
                        WriteResult::Rejected(failure("clock refusal").venue_code(code)),
                    )
                    .unwrap();
                let _ = writes.grant_resend(RequestId(1));
                writes.retry(RequestId(1), RequestId(2)).unwrap();
                if released_retry {
                    writes.released(RequestId(2)).unwrap();
                }
                writes.result(RequestId(2), failed(cause)).unwrap();
                let error = seal(&mut writes);
                assert_eq!(error.outcome, Some(Outcome::Unknown));
                assert_eq!(error.message, cause);
            }
        }
    }
}

#[test]
fn aggregate_retains_effects_and_unknowns_independent_of_callback_order() {
    for order in [
        [1, 2, 3],
        [1, 3, 2],
        [2, 1, 3],
        [2, 3, 1],
        [3, 1, 2],
        [3, 2, 1],
    ] {
        let mut writes = CommandWrites::new(3).unwrap();
        for id in [1, 2, 3] {
            writes.register(RequestId(id), id).unwrap();
            writes.released(RequestId(id)).unwrap();
        }
        for id in order {
            writes
                .result(
                    RequestId(id),
                    match id {
                        1 => WriteResult::Accepted {
                            order_ids: vec!["native-1".into()],
                        },
                        2 => rejected("native refusal"),
                        _ => failed("connection lost"),
                    },
                )
                .unwrap();
        }
        let error = seal(&mut writes);
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(error.message, "native refusal");
        assert_eq!(error.partial.unwrap().order_ids, ["native-1"]);
    }
}

#[test]
fn no_physical_release_is_not_sent_and_unsettled_release_is_unknown() {
    for did_release in [false, true] {
        let mut writes = CommandWrites::new(1).unwrap();
        writes.register(RequestId(1), 0).unwrap();
        if did_release {
            writes.released(RequestId(1)).unwrap();
        }
        let error = seal(&mut writes);
        assert_eq!(
            error.outcome,
            Some(if did_release {
                Outcome::Unknown
            } else {
                Outcome::NotSent
            })
        );
        assert_eq!(
            writes.result(RequestId(1), rejected("late refusal")),
            Err(Error::State)
        );
        assert_eq!(writes.register(RequestId(2), 2), Err(Error::State));
        assert_eq!(writes.fail(failure("duplicate")), Err(Error::State));
    }
}

#[test]
fn unknown_write_cannot_enter_a_generic_retry_and_native_facts_require_release() {
    let mut writes = CommandWrites::new(4).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    assert_eq!(
        writes.result(RequestId(1), rejected("not sent")),
        Err(Error::State)
    );
    writes.released(RequestId(1)).unwrap();
    writes
        .result(RequestId(1), failed("network failure"))
        .unwrap();
    let _ = writes.grant_resend(RequestId(1));
    assert_eq!(
        writes.retry(RequestId(1), RequestId(2)),
        Err(Error::RetryForbidden)
    );
    assert_eq!(writes.register(RequestId(2), 1), Err(Error::Duplicate));
    assert_eq!(writes.released(RequestId(1)), Err(Error::State));
    assert_eq!(
        writes.result(RequestId(1), rejected("duplicate")),
        Err(Error::State)
    );
}

#[test]
fn deterministic_error_selection_uses_logical_order_and_keeps_all_native_partial_ids() {
    let mut writes = CommandWrites::new(2).unwrap();
    // Physical registration can differ from caller logical order.
    writes.register(RequestId(20), 20).unwrap();
    writes.register(RequestId(10), 10).unwrap();
    for id in [20, 10] {
        writes.released(RequestId(id)).unwrap();
    }
    writes
        .result(
            RequestId(20),
            WriteResult::Failed(failure("second").partial_ids(&["B".into()])),
        )
        .unwrap();
    writes
        .result(
            RequestId(10),
            WriteResult::Failed(failure("first").partial(&["A".into()], 3, 1)),
        )
        .unwrap();
    let error = seal(&mut writes);
    assert_eq!(error.message, "first");
    let partial = error.partial.unwrap();
    assert_eq!(partial.order_ids, ["A", "B"]);
    assert_eq!((partial.total, partial.failed_index), (Some(3), Some(1)));
}

#[test]
fn evidence_and_id_capacity_fail_closed() {
    assert_eq!(CommandWrites::new(0).err(), Some(Error::Capacity));
    let mut writes = CommandWrites::new(1).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    assert_eq!(writes.register(RequestId(2), 2), Err(Error::Capacity));
    let mut life = lifecycle(1, None);
    life.prepared(
        life.preparation(),
        Prepared {
            id: 1,
            expires_at: None,
        },
    )
    .unwrap();
    life.unsigned().unwrap();
    life.permits = u64::MAX;
    assert_eq!(life.permit(), Err(Error::Capacity));
}

fn scheduler() -> crate::read_scheduler::Scheduler<u8, u8> {
    use crate::read_scheduler::{Demand, ExpiryPolicy, Policy, RetryPolicy, Scheduler};
    let mut scheduler = Scheduler::new(2, vec![1]).unwrap();
    scheduler
        .register(
            1,
            &1,
            Policy {
                debounce_ms: 0.,
                refresh_debounce_ms: 0.,
                admission_timeout_ms: 100.,
                response_timeout_ms: 1_000.,
                response_expiry: ExpiryPolicy::Retry,
                retry: RetryPolicy::default(),
            },
        )
        .unwrap();
    scheduler.request(&1, 0., Demand::Refresh).unwrap();
    scheduler
}

#[test]
fn scheduler_pause_expiry_and_retired_tokens_deny_release_instead_of_sending_unleased() {
    for reason in ["pause", "expiry", "retired"] {
        let mut scheduler = scheduler();
        let (_, token) = scheduler.take_ready(0., |_| 0.).unwrap().unwrap();
        let at = match reason {
            "pause" => {
                scheduler.pause(&1, 50.).unwrap();
                20.
            }
            "expiry" => 100.,
            _ => {
                scheduler.retire(&1).unwrap();
                20.
            }
        };
        let mut life = lifecycle(1, None);
        let permit = ready(&mut life);
        assert_eq!(
            life.claim(
                permit,
                clock(at),
                &mut SendPacing::new(&[]).unwrap(),
                &mut |release| {
                    scheduler
                        .admit_http(token, release)
                        .map_err(|_| Error::Invalid)
                }
            ),
            Err(Error::Revoked)
        );
        assert!(!life.accepts_response());
        assert!(life.release().is_none());
        assert_eq!(
            claim(&mut life, permit, at, &mut SendPacing::new(&[]).unwrap()),
            Err(Error::State)
        );
        if reason == "pause" {
            // Pausing revokes an unsent reservation. Its replacement must be
            // immediately available without network-failure backoff/jitter.
            assert!(!scheduler.mark_issued(token, 50.).unwrap());
            let (_, replacement) = scheduler
                .take_ready(50., |_| panic!("unsent work drew retry jitter"))
                .unwrap()
                .unwrap();
            assert_ne!(token, replacement);
            assert!(scheduler.mark_issued(replacement, 50.).unwrap());
        }
    }
}

#[test]
fn scheduler_continuations_keep_first_release_lease_until_validated_progress() {
    let mut scheduler = scheduler();
    let (_, token) = scheduler.take_ready(0., |_| 0.).unwrap().unwrap();
    let mut pacing = SendPacing::new(&[]).unwrap();
    for (id, at, expected) in [(1, 20., 1_020.), (2, 100., 1_020.), (3, 300., 1_200.)] {
        if id == 3 {
            assert!(scheduler.progress(token, 200.).unwrap());
        }
        let mut life = lifecycle(id, None);
        let permit = ready(&mut life);
        let result = life
            .claim(permit, clock(at), &mut pacing, &mut |release| {
                scheduler
                    .admit_http(token, release)
                    .map_err(|_| Error::Invalid)
            })
            .unwrap();
        assert_eq!(released(result).response_lease, Some(expected));
    }
}

#[test]
fn validated_progress_does_not_expire_an_already_inflight_sibling_at_the_old_lease() {
    let mut scheduler = scheduler();
    let (_, token) = scheduler.take_ready(0., |_| 0.).unwrap().unwrap();
    let mut pacing = SendPacing::new(&[]).unwrap();
    let mut siblings = [lifecycle(1, None), lifecycle(2, None)];
    for life in &mut siblings {
        let permit = ready(life);
        let release = life
            .claim(permit, clock(20.), &mut pacing, &mut |release| {
                scheduler
                    .admit_http(token, release)
                    .map_err(|_| Error::Invalid)
            })
            .unwrap();
        assert_eq!(released(release).response_lease, Some(1_020.));
    }
    assert!(scheduler.progress(token, 200.).unwrap());
    for life in &mut siblings {
        assert!(!life.expire(1_020.).unwrap());
        assert!(life.accepts_response());
    }
    assert_eq!(scheduler.admit(token, 1_199.).unwrap(), Some(1_200.));
    assert_eq!(scheduler.admit(token, 1_200.).unwrap(), None);
    // Scheduler revocation, not a stale physical snapshot, ends the logical
    // read. The original lifecycle still provides the unconditional hard cap.
    assert!(siblings[0].expire(5_000.).unwrap());
}

#[test]
fn invalid_custom_lease_is_checked_before_admission_state_mutation() {
    let mut life = lifecycle(1, None);
    let permit = ready(&mut life);
    let mut committed = false;
    assert_eq!(
        life.claim(
            permit,
            clock(10.),
            &mut SendPacing::new(&[]).unwrap(),
            &mut |release| {
                let lease = release.response_lease(f64::NAN)?;
                committed = true;
                Ok(Admission::Scheduled(lease))
            }
        ),
        Err(Error::Revoked)
    );
    assert!(!committed);
    assert!(!life.accepts_response());
}

#[test]
fn old_route_and_signing_completions_cannot_populate_a_new_preparation() {
    let mut life = lifecycle(1, None);
    let old = life.preparation();
    life.prepared(
        old,
        Prepared {
            id: 1,
            expires_at: None,
        },
    )
    .unwrap();
    let old_sign = life.begin_signing().unwrap();
    life.reprepare().unwrap();
    assert_eq!(
        life.prepared(
            old,
            Prepared {
                id: 1,
                expires_at: None
            }
        ),
        Err(Error::Revoked)
    );
    let fresh = life.preparation();
    life.prepared(
        fresh,
        Prepared {
            id: 2,
            expires_at: None,
        },
    )
    .unwrap();
    let fresh_sign = life.begin_signing().unwrap();
    assert_eq!(
        life.signed(old_sign, SignReceipt::default(), 2, 1_000.),
        Err(Error::Revoked)
    );
    life.signed(fresh_sign, SignReceipt::default(), 2, 1_000.)
        .unwrap();
    assert!(life.permit().is_ok());
}

#[test]
fn lost_route_signer_and_permit_callbacks_are_bounded_by_the_original_deadline() {
    for stage in 0..4 {
        let mut life = lifecycle(1, None);
        if stage >= 1 {
            life.prepared(
                life.preparation(),
                Prepared {
                    id: 1,
                    expires_at: None,
                },
            )
            .unwrap();
        }
        if stage == 2 {
            life.begin_signing().unwrap();
        }
        if stage == 3 {
            life.unsigned().unwrap();
            life.permit().unwrap();
        }
        assert!(!life.expire(4_999.).unwrap());
        assert!(life.expire(5_000.).unwrap());
        assert!(!life.expire(5_001.).unwrap());
        assert!(!life.accepts_response());
        assert!(life.release().is_none());
    }
}

#[test]
fn partial_native_rejection_forbids_replaying_the_effective_logical_write() {
    let mut writes = CommandWrites::new(3).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    writes.released(RequestId(1)).unwrap();
    writes
        .result(
            RequestId(1),
            WriteResult::Rejected(failure("partial refusal").partial_ids(&["accepted".into()])),
        )
        .unwrap();
    let _ = writes.grant_resend(RequestId(1));
    assert_eq!(
        writes.retry(RequestId(1), RequestId(2)),
        Err(Error::RetryForbidden)
    );
    let error = seal(&mut writes);
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.unwrap().order_ids, ["accepted"]);
}

#[test]
fn fully_known_partial_execution_preserves_ids_and_each_native_refusal_without_unknown() {
    for order in [[1, 2, 3], [3, 2, 1]] {
        let mut writes = CommandWrites::new(3).unwrap();
        for id in [1, 2, 3] {
            writes.register(RequestId(id), id).unwrap();
            writes.released(RequestId(id)).unwrap();
        }
        for id in order {
            writes
                .result(
                    RequestId(id),
                    if id == 1 {
                        WriteResult::Accepted {
                            order_ids: vec!["accepted-native-id".into()],
                        }
                    } else {
                        WriteResult::Rejected(failure("native refusal").venue_code(id.to_string()))
                    },
                )
                .unwrap();
        }
        let error = seal(&mut writes);
        assert_eq!(error.outcome, None);
        assert_eq!(error.partial.unwrap().order_ids, ["accepted-native-id"]);
        assert_eq!(
            writes
                .rejections()
                .map(|(_, error)| error.venue_code.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["2", "3"]
        );
    }
}

#[test]
fn staged_retry_timeout_does_not_erase_prior_attempt_evidence() {
    let mut writes = CommandWrites::new(3).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    writes.released(RequestId(1)).unwrap();
    writes
        .result(RequestId(1), rejected("retryable refusal"))
        .unwrap();
    let _ = writes.grant_resend(RequestId(1));
    writes.retry(RequestId(1), RequestId(2)).unwrap();
    assert_eq!(seal(&mut writes).outcome, Some(Outcome::Unknown));
}

#[test]
fn recovered_retry_is_not_reported_as_a_rejected_logical_leg() {
    let mut writes = CommandWrites::new(3).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    writes.released(RequestId(1)).unwrap();
    writes
        .result(RequestId(1), rejected("recoverable clock refusal"))
        .unwrap();
    let _ = writes.grant_resend(RequestId(1));
    writes.retry(RequestId(1), RequestId(2)).unwrap();
    writes.released(RequestId(2)).unwrap();
    writes
        .result(
            RequestId(2),
            WriteResult::Accepted {
                order_ids: vec!["accepted-retry".into()],
            },
        )
        .unwrap();
    writes.register(RequestId(3), 2).unwrap();
    writes.released(RequestId(3)).unwrap();
    writes
        .result(RequestId(3), rejected("native refusal"))
        .unwrap();
    let error = seal(&mut writes);
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.unwrap().order_ids, ["accepted-retry"]);
    assert_eq!(
        writes.rejections().map(|(id, _)| id).collect::<Vec<_>>(),
        [RequestId(3)]
    );
}

#[test]
fn changed_group_policy_cannot_bypass_the_fixed_physical_send_gap() {
    let policy = Pacing {
        group: 1,
        minimum_gap_ms: 450.,
    };
    let mut pacing = SendPacing::new(&[policy]).unwrap();
    let mut first = lifecycle(1, Some(policy));
    let permit = ready(&mut first);
    claim(&mut first, permit, 10., &mut pacing).unwrap();
    let mut changed = lifecycle(
        2,
        Some(Pacing {
            group: 1,
            minimum_gap_ms: 0.,
        }),
    );
    let permit = ready(&mut changed);
    let mut admissions = 0;
    assert_eq!(
        changed.claim(permit, clock(11.), &mut pacing, &mut |_| {
            admissions += 1;
            Ok(Admission::Unscheduled)
        }),
        Ok(Claim::WaitUntil(460.))
    );
    assert_eq!(admissions, 0);
}

#[test]
fn per_request_longer_gap_is_anchored_to_its_own_handoff_and_survives_next_request() {
    let floor = Pacing {
        group: 1,
        minimum_gap_ms: 450.,
    };
    let mut pacing = SendPacing::new(&[floor]).unwrap();
    let mut first = lifecycle(
        1,
        Some(Pacing {
            group: 1,
            minimum_gap_ms: 900.,
        }),
    );
    let p1 = ready(&mut first);
    claim(&mut first, p1, 10., &mut pacing).unwrap();
    first.settle();
    let mut second = lifecycle(2, Some(floor));
    let p2 = ready(&mut second);
    assert_eq!(
        claim(&mut second, p2, 460., &mut pacing),
        Ok(Claim::WaitUntil(910.))
    );
    assert!(claim(&mut second, p2, 910., &mut pacing).is_ok());
    let mut third = lifecycle(3, Some(floor));
    let p3 = ready(&mut third);
    assert_eq!(
        claim(&mut third, p3, 911., &mut pacing),
        Ok(Claim::WaitUntil(1_360.))
    );
}

#[test]
fn host_tokens_round_trip_wide_generations_without_js_number_conversion() {
    let attempt = Attempt {
        request: RequestId(crate::resource_ids::MAX_RESOURCE_ID),
        generation: u64::MAX,
    };
    let preparation = Preparation {
        attempt,
        sequence: u64::MAX,
    };
    let permit = Permit {
        attempt,
        preparation: u64::MAX,
        sequence: u64::MAX,
    };
    assert_eq!(
        Preparation::from_wire(&preparation.to_wire()),
        Ok(preparation)
    );
    assert_eq!(Permit::from_wire(&permit.to_wire()), Ok(permit));
    assert!(preparation.to_wire().contains("18446744073709551615"));
    assert_eq!(preparation.attempt(), attempt);
    assert_eq!(permit.attempt(), attempt);
    assert_eq!(permit.prepared_id(), u64::MAX);
    assert!(Preparation::from_wire(&permit.to_wire()).is_err());
    assert!(Permit::from_wire(&preparation.to_wire()).is_err());
}

#[test]
fn host_tokens_reject_noncanonical_aliases_overflow_and_truncation() {
    for token in [
        "p1:0:1:0",
        "p1:1:0:0",
        "p1:01:1:0",
        "p1:+1:1:0",
        "p1:1:1:00",
        "p1:1:1:-1",
        "p1:1:1:0 ",
        "p1:1:1:０",
        "p1:1:1:0:",
        "p1:1:1",
        "p1:1::0",
        "p2:1:1:0",
        "p1:9007199254740992:1:0",
        "p1:1:18446744073709551616:0",
    ] {
        assert_eq!(
            Preparation::from_wire(token),
            Err(Error::Invalid),
            "{token}"
        );
    }
    for token in [
        "r1:1:1:0:0",
        "r1:1:1:0:01",
        "r1:1:1:+0:1",
        "r1:1:1:0:1:",
        "r1:1:1:0",
        "r1:1:1:18446744073709551616:1",
        "r2:1:1:0:1",
    ] {
        assert_eq!(Permit::from_wire(token), Err(Error::Invalid), "{token}");
    }
}

#[test]
fn decoded_old_generation_cannot_complete_or_claim_a_reprepared_request() {
    let mut life = lifecycle(1, None);
    let old_preparation = life.preparation().to_wire();
    let old_permit = ready(&mut life).to_wire();
    life.reprepare().unwrap();
    assert_eq!(
        life.prepared(
            Preparation::from_wire(&old_preparation).unwrap(),
            Prepared {
                id: 7,
                expires_at: None
            }
        ),
        Err(Error::Revoked)
    );
    let fresh = ready(&mut life);
    assert_eq!(
        claim(
            &mut life,
            Permit::from_wire(&old_permit).unwrap(),
            10.,
            &mut SendPacing::new(&[]).unwrap()
        ),
        Err(Error::Revoked)
    );
    assert!(claim(&mut life, fresh, 10., &mut SendPacing::new(&[]).unwrap()).is_ok());
}

#[test]
fn optimistic_success_never_erases_unresolved_failed_or_rejected_write_evidence() {
    for state in 0..4 {
        let mut writes = CommandWrites::new(2).unwrap();
        writes.register(RequestId(1), 1).unwrap();
        if state != 0 {
            writes.released(RequestId(1)).unwrap();
        }
        if state == 2 {
            writes
                .result(RequestId(1), rejected("native refusal"))
                .unwrap();
        }
        if state == 3 {
            writes
                .result(RequestId(1), WriteResult::Failed(failure("lost transport")))
                .unwrap();
        }
        assert_eq!(writes.succeed(), Err(Error::State));
        assert_eq!(
            seal(&mut writes).outcome,
            Some(match state {
                0 => Outcome::NotSent,
                2 => Outcome::Rejected,
                _ => Outcome::Unknown,
            })
        );
        assert_eq!(writes.succeed(), Err(Error::State));
    }
}

#[test]
fn write_success_seals_once_including_a_recovered_native_refusal() {
    let mut empty = CommandWrites::new(2).unwrap();
    assert_eq!(empty.succeed(), Err(Error::State));
    let mut writes = CommandWrites::new(3).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    writes.released(RequestId(1)).unwrap();
    writes
        .result(RequestId(1), rejected("native clock refusal"))
        .unwrap();
    let _ = writes.grant_resend(RequestId(1));
    writes.retry(RequestId(1), RequestId(2)).unwrap();
    assert_eq!(writes.succeed(), Err(Error::State));
    writes.released(RequestId(2)).unwrap();
    writes
        .result(
            RequestId(2),
            WriteResult::Accepted {
                order_ids: vec!["accepted".into()],
            },
        )
        .unwrap();
    assert_eq!(writes.succeed(), Ok(()));
    assert_eq!(writes.succeed(), Err(Error::State));
    assert_eq!(writes.fail(failure("late")), Err(Error::State));
    assert_eq!(writes.register(RequestId(3), 2), Err(Error::State));
    assert_eq!(writes.rejections().count(), 0);
}

#[test]
fn known_partial_writes_cannot_be_sealed_as_success_or_lose_their_partial_projection() {
    let mut writes = CommandWrites::new(2).unwrap();
    for id in [1, 2] {
        writes.register(RequestId(id), id).unwrap();
        writes.released(RequestId(id)).unwrap();
    }
    writes
        .result(
            RequestId(1),
            WriteResult::Accepted {
                order_ids: vec!["known-effect".into()],
            },
        )
        .unwrap();
    writes
        .result(RequestId(2), rejected("native refusal"))
        .unwrap();
    assert_eq!(writes.succeed(), Err(Error::State));
    let error = seal(&mut writes);
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.unwrap().order_ids, ["known-effect"]);
    assert_eq!(
        writes.rejections().map(|(id, _)| id).collect::<Vec<_>>(),
        [RequestId(2)]
    );
}

#[test]
fn known_partial_effects_without_ids_forbid_retry_and_whole_success_or_rejection() {
    let mut writes = CommandWrites::new(2).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    let fact = WriteResult::KnownPartial(
        VenueError::new("cancelled one, refused another")
            .kind(crate::ErrorKind::Rejected)
            .venue_code("-2011"),
    );
    assert_eq!(writes.result(RequestId(1), fact.clone()), Err(Error::State));
    writes.released(RequestId(1)).unwrap();
    writes.result(RequestId(1), fact).unwrap();
    let _ = writes.grant_resend(RequestId(1));
    assert_eq!(
        writes.retry(RequestId(1), RequestId(2)),
        Err(Error::RetryForbidden)
    );
    assert_eq!(writes.succeed(), Err(Error::State));
    let error = writes.fail(VenueError::new("fallback")).unwrap();
    assert_eq!(error.outcome, None);
    assert_eq!(error.kind, Some(crate::ErrorKind::Rejected));
    assert_eq!(error.venue_code.as_deref(), Some("-2011"));
    assert!(
        error.partial.is_none(),
        "no invented IDs to encode known effects"
    );
    assert_eq!(
        writes.rejections().map(|(id, _)| id).collect::<Vec<_>>(),
        [RequestId(1)]
    );
    assert_eq!(
        writes.reports().collect::<Vec<_>>(),
        ["cancelled one, refused another"]
    );
}

#[test]
fn known_partial_effects_do_not_hide_an_unresolved_sent_sibling() {
    let mut writes = CommandWrites::new(2).unwrap();
    for id in [1, 2] {
        writes.register(RequestId(id), id).unwrap();
        writes.released(RequestId(id)).unwrap();
    }
    writes
        .result(
            RequestId(1),
            WriteResult::KnownPartial(VenueError::new("known effects")),
        )
        .unwrap();
    assert_eq!(
        writes.fail(VenueError::new("unresolved")).unwrap().outcome,
        Some(Outcome::Unknown)
    );
}

#[test]
fn a_write_resend_needs_an_engine_grant_and_each_grant_is_used_once() {
    let mut writes = CommandWrites::new(8).unwrap();
    writes.register(RequestId(1), 1).unwrap();
    writes.released(RequestId(1)).unwrap();
    writes.result(RequestId(1), rejected("clock")).unwrap();
    assert_eq!(
        writes.retry(RequestId(1), RequestId(2)),
        Err(Error::RetryForbidden)
    );
    writes.grant_resend(RequestId(1)).unwrap();
    writes.retry(RequestId(1), RequestId(2)).unwrap();
    writes.released(RequestId(2)).unwrap();
    writes
        .result(RequestId(2), rejected("clock again"))
        .unwrap();
    assert_eq!(
        writes.retry(RequestId(2), RequestId(3)),
        Err(Error::RetryForbidden)
    );
    let mut failed = CommandWrites::new(8).unwrap();
    failed.register(RequestId(1), 1).unwrap();
    failed.released(RequestId(1)).unwrap();
    failed.result(RequestId(1), self::failed("lost")).unwrap();
    assert_eq!(failed.grant_resend(RequestId(1)), Err(Error::State));
}
#[test]
fn full_plan_journal_backing_is_reserved_before_any_native_fact() {
    for (attempts, legs) in [(0, 1), (257, 1), (1, 0), (1, 2)] {
        assert!(matches!(
            CommandWrites::new_reserved(attempts, legs),
            Err(Error::Capacity)
        ));
    }
    let mut writes = CommandWrites::new_reserved(256, 128).unwrap();
    let backing = (writes.attempts.capacity(), writes.legs.capacity());
    assert!(backing.0 >= 256 && backing.1 >= 128);
    assert!(writes.attempts.is_empty() && writes.legs.is_empty());
    for leg in 1..=128 {
        let first = RequestId(leg * 2);
        let second = RequestId(leg * 2 + 1);
        writes.register(first, leg).unwrap();
        *writes.release_flag(first).unwrap() = true;
        writes
            .result(first, WriteResult::Rejected(VenueError::new("refused")))
            .unwrap();
        writes.grant_resend(first).unwrap();
        writes.retry(first, second).unwrap();
        *writes.release_flag(second).unwrap() = true;
        writes
            .result(
                second,
                WriteResult::Accepted {
                    order_ids: Vec::new(),
                },
            )
            .unwrap();
        assert_eq!(
            (writes.attempts.capacity(), writes.legs.capacity()),
            backing
        );
    }
    writes.succeed().unwrap();
}
#[test]
fn reserved_journal_never_grows_leg_backing_for_generic_registration_or_replacement() {
    for replacement in [false, true] {
        let mut writes = CommandWrites::new_reserved(4, 1).unwrap();
        let backing = (writes.attempts.capacity(), writes.legs.capacity());
        writes.register(RequestId(1), 1).unwrap();
        if replacement {
            *writes.release_flag(RequestId(1)).unwrap() = true;
            writes
                .result(
                    RequestId(1),
                    WriteResult::Rejected(VenueError::new("refused")),
                )
                .unwrap();
            writes.replace(RequestId(1), &[2, 3, 4]).unwrap();
            assert_eq!(
                (writes.attempts.capacity(), writes.legs.capacity()),
                backing
            );
        }
        for leg in 2..=4 {
            writes.register(RequestId(leg), leg).unwrap();
            assert_eq!(
                (writes.attempts.capacity(), writes.legs.capacity()),
                backing
            );
        }
        assert!(matches!(
            writes.register(RequestId(5), 5),
            Err(Error::Capacity)
        ));
    }
}
