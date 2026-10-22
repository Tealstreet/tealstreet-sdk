use super::*;
use crate::request_context::{Credential, Pacing, RetryPolicy};
use crate::{CredentialSlot, Outcome, Store};

fn policy() -> Policy {
    Policy {
        operation: Operation::Mutation,
        credential: Some(Credential {
            slot: CredentialSlot::Primary,
            reference: crate::signing::KeyRef("replacement-test".into()),
        }),
        timeout_ms: 100,
        pacing: Some(Pacing {
            group: 8,
            minimum_gap_ms: 25.,
        }),
        retry: RetryPolicy::DefinitiveRefusal,
    }
}

fn refusal(registry: &mut ContextRegistry, context: &Context, request: u32) -> Delivery {
    registry
        .register_attempt(context, RequestId(request), 0.)
        .unwrap();
    *registry
        .release_flag(context, RequestId(request))
        .unwrap()
        .unwrap() = true;
    let delivery = Delivery {
        context: context.clone(),
        attempt: Attempt {
            request: RequestId(request),
            generation: u64::from(request),
        },
    };
    registry
        .result(
            &delivery,
            WriteResult::Rejected(VenueError::new("native refusal")),
        )
        .unwrap();
    delivery
}

fn unchanged(registry: &ContextRegistry, parent: &Context, request: u32, count: usize) {
    assert_eq!(registry.contexts.len(), count);
    assert_eq!(registry.validate(parent, 1.), Ok(()));
    assert_eq!(
        registry.commands[&7]
            .writes
            .as_ref()
            .unwrap()
            .latest(parent.leg()),
        Some(RequestId(request))
    );
    assert_eq!(registry.commands[&7].pending, 1);
}

#[test]
fn replacement_policies_are_atomic_and_keep_exact_parent_bounds() {
    let mut registry = ContextRegistry::default();
    let parent = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
    let delivery = refusal(&mut registry, &parent, 11);
    for invalid in 0..5 {
        let mut child = policy();
        match invalid {
            0 => child.operation = Operation::Read,
            1 => child.credential = None,
            2 => child.pacing = None,
            3 => child.timeout_ms = 0,
            _ => child.retry = RetryPolicy::ReadFailure,
        }
        assert!(
            registry
                .replace(&parent, &delivery, 20, &[policy(), child], 1.)
                .is_err()
        );
        unchanged(&registry, &parent, 11, 1);
    }
    assert_eq!(
        registry.replace(&parent, &delivery, 20, &[], 1.),
        Err(Error::Invalid)
    );
    assert_eq!(
        registry.replace(&parent, &delivery, u32::MAX, &[policy()], 1.),
        Err(Error::Capacity)
    );
    unchanged(&registry, &parent, 11, 1);
    let mut short = policy();
    short.timeout_ms = 1;
    short.retry = RetryPolicy::Never;
    let children = registry
        .replace(&parent, &delivery, 20, &[policy(), short], 1.25)
        .unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].deadline(), 100.);
    assert_eq!(children[1].deadline(), 2.25);
    for (index, child) in children.iter().enumerate() {
        assert_eq!(child.owner(), parent.owner());
        assert_eq!(child.leg(), 20 + index as u64);
        assert_eq!(child.credential(), parent.credential());
        assert_eq!(child.pacing(), parent.pacing());
        assert_eq!(registry.validate(child, 1.25), Ok(()));
    }
    assert_eq!(
        registry.commands[&7].pending, 1,
        "reservation is not a physical attempt"
    );
    assert_eq!(registry.validate(&parent, 1.25), Err(Error::Revoked));
    assert_eq!(
        registry.validate_handle(&parent),
        Ok(()),
        "retirement remains allowed"
    );
    assert_eq!(
        registry.register_attempt(&parent, RequestId(99), 1.25),
        Err(Error::Revoked)
    );
    assert!(
        registry
            .result(&delivery, WriteResult::Accepted { order_ids: vec![] })
            .is_err()
    );
    // Even the original root cannot turn this old HTTP callback into completion.
    assert_eq!(
        registry.validate_completion(7, Some(&delivery), Some(&parent)),
        Err(Error::Revoked)
    );
    assert_eq!(registry.validate_completion(7, None, Some(&parent)), Ok(()));
}

#[test]
fn replacement_denies_foreign_sibling_expired_and_unproven_parents() {
    for case in 0..5 {
        let mut registry = ContextRegistry::default();
        let parent = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
        let mut delivery = refusal(&mut registry, &parent, 11);
        let argument = match case {
            0 => ContextRegistry::default()
                .issue(1, Scope::Command(7), policy(), 0.)
                .unwrap(),
            1 => registry.issue(2, Scope::Command(7), policy(), 0.).unwrap(),
            _ => parent.clone(),
        };
        if case == 2 {
            delivery.attempt.request = RequestId(99);
        }
        let count = registry.contexts.len();
        let now = if case == 3 { 100. } else { 1. };
        if case == 4 {
            registry.commands.get_mut(&7).unwrap().writes = None;
        }
        assert!(
            registry
                .replace(&argument, &delivery, 20, &[policy()], now)
                .is_err()
        );
        assert_eq!(registry.contexts.len(), count);
        assert!(!registry.contexts.contains_key(&20));
        if case != 4 {
            unchanged(&registry, &parent, 11, count);
            assert!(
                registry
                    .replace(&parent, &refusal_delivery(&parent), 20, &[policy()], 1.)
                    .is_ok()
            );
        }
    }
}

fn refusal_delivery(parent: &Context) -> Delivery {
    Delivery {
        context: parent.clone(),
        attempt: Attempt {
            request: RequestId(11),
            generation: 11,
        },
    }
}

#[test]
fn context_and_journal_capacity_fail_without_partial_supersession() {
    for fill_contexts in [false, true] {
        let mut registry = ContextRegistry::default();
        let parent = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
        let delivery = refusal(&mut registry, &parent, 11);
        for id in 2..CAPACITY as u32 {
            if fill_contexts {
                registry
                    .issue(
                        id,
                        Scope::Background,
                        Policy {
                            operation: Operation::Read,
                            retry: RetryPolicy::Never,
                            ..policy()
                        },
                        0.,
                    )
                    .unwrap();
            } else {
                // Use the existing journal's bounded physical attempt budget.
                registry
                    .commands
                    .get_mut(&7)
                    .unwrap()
                    .writes
                    .as_mut()
                    .unwrap()
                    .register(RequestId(1000 + id), u64::from(1000 + id))
                    .unwrap();
            }
        }
        let count = registry.contexts.len();
        assert_eq!(
            registry.replace(&parent, &delivery, 500, &[policy(), policy()], 1.),
            Err(Error::Capacity)
        );
        unchanged(&registry, &parent, 11, count);
        let children = registry
            .replace(&parent, &delivery, 500, &[policy()], 1.)
            .unwrap();
        assert_eq!(
            children.len(),
            1,
            "failed larger admission left no reservations"
        );
    }
}

#[test]
fn superseded_nonroot_cannot_seal_and_expired_root_timer_grants_no_send() {
    let mut registry = ContextRegistry::default();
    let root = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
    let parent = registry
        .issue_bound(2, Scope::Command(7), policy(), 0., Some(&root))
        .unwrap();
    let delivery = refusal(&mut registry, &parent, 11);
    registry
        .replace(&parent, &delivery, 20, &[policy()], 1.)
        .unwrap();
    assert_eq!(
        registry.validate_completion(7, None, Some(&parent)),
        Err(Error::Revoked)
    );
    assert_eq!(
        registry.validate_completion(7, Some(&delivery), Some(&root)),
        Err(Error::Revoked)
    );
    // Unrelated new work performs expired logical handle cleanup, not owner retirement.
    registry
        .issue(
            100,
            Scope::Background,
            Policy {
                operation: Operation::Read,
                retry: RetryPolicy::Never,
                ..policy()
            },
            100.,
        )
        .unwrap();
    assert!(!registry.contexts.contains_key(&root.leg()));
    assert_eq!(registry.validate_completion(7, None, Some(&root)), Ok(()));
    assert_eq!(registry.validate(&root, 100.), Err(Error::TimedOut));
    let Seal::Failure(error, _) =
        registry.seal(7, None, Some(&root), Some(VenueError::new("deadline")))
    else {
        panic!("reserved child cannot disappear");
    };
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    registry.finish(7);
    assert_eq!(
        registry.validate_completion(7, None, Some(&root)),
        Err(Error::Revoked)
    );
}

#[test]
fn public_bridge_checks_whole_id_range_and_never_rewinds_on_refusal() {
    let mut io = super::super::Io::<()>::default();
    let mut store = Store::default();
    let parent = io
        .request_contexts
        .issue(1, Scope::Command(7), policy(), 0.)
        .unwrap();
    io.http_delivery = Some(refusal(&mut io.request_contexts, &parent, 11));
    io.next_id = u32::MAX - 1;
    let mut cx = Cx {
        now: 1.,
        store: &mut store,
        io: &mut io,
    };
    assert_eq!(
        cx.http_replacement_contexts(&parent, &[policy(), policy()]),
        Err(Error::Capacity)
    );
    assert_eq!(cx.io.next_id, u32::MAX - 1);
    unchanged(&cx.io.request_contexts, &parent, 11, 1);
    let mut invalid = policy();
    invalid.timeout_ms = 0;
    assert_eq!(
        cx.http_replacement_contexts(&parent, &[invalid]),
        Err(Error::Invalid)
    );
    assert_eq!(
        cx.io.next_id,
        u32::MAX,
        "failed policy burns checked identities"
    );
    assert_eq!(
        cx.http_replacement_contexts(&parent, &[policy()]),
        Err(Error::Capacity)
    );
    unchanged(&cx.io.request_contexts, &parent, 11, 1);
    assert!(cx.io.pending.is_empty());
    assert!(cx.io.actions.is_empty());
}
