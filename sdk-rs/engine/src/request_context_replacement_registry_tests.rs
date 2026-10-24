use super::*;
use crate::request_context::{Credential, Pacing, RetryPolicy};
use crate::resource_ids::{MAX_RESOURCE_ID, ResourceIds};
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

fn refusal(registry: &mut ContextRegistry, context: &Context, request: u64) -> Delivery {
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
            generation: request,
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

fn unchanged(registry: &ContextRegistry, parent: &Context, request: u64, count: usize) {
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
        registry.replace(&parent, &delivery, MAX_RESOURCE_ID + 1, &[policy()], 1.),
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
        for id in 2..CAPACITY as u64 {
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
                    .register(RequestId(1000 + id), 1000 + id)
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
        .issue_with_egress(2, Scope::Command(7), policy(), 0., Some(&root), None)
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
fn public_replacement_children_cross_u32_without_narrowing_or_identity_reuse() {
    let mut io = super::super::Io::<()>::default();
    let mut store = Store::default();
    let parent = io
        .request_contexts
        .issue(1, Scope::Command(7), policy(), 0.)
        .unwrap();
    io.http_delivery = Some(refusal(&mut io.request_contexts, &parent, 11));
    let first = u64::from(u32::MAX);
    io.next_id = ResourceIds::starting_at(first);
    let mut cx = Cx {
        now: 1.,
        store: &mut store,
        io: &mut io,
    };
    let children = cx
        .http_replacement_contexts(&parent, &[policy(), policy()])
        .unwrap();
    assert_eq!(
        children.iter().map(Context::leg).collect::<Vec<_>>(),
        [first, first + 1]
    );
    assert_eq!(cx.io.next_id, ResourceIds::starting_at(first + 2));
    assert!(children.iter().all(|child| child.owner() == parent.owner()
        && child.deadline() == parent.deadline()
        && cx.io.request_contexts.validate(child, 1.).is_ok()));
    assert_eq!(
        cx.io.request_contexts.validate(&parent, 1.),
        Err(Error::Revoked)
    );
    assert!(!cx.io.next_id.failed());
    assert!(cx.io.pending.is_empty());
    assert!(cx.io.actions.is_empty());
    assert!(cx.io.timers.is_empty());
}

#[test]
fn last_safe_replacement_range_refuses_atomically_then_admits_its_final_child() {
    let mut io = super::super::Io::<()>::default();
    let mut store = Store::default();
    let parent = io
        .request_contexts
        .issue(1, Scope::Command(7), policy(), 0.)
        .unwrap();
    io.http_delivery = Some(refusal(&mut io.request_contexts, &parent, 11));
    io.next_id = ResourceIds::starting_at(MAX_RESOURCE_ID);
    let mut cx = Cx {
        now: 1.,
        store: &mut store,
        io: &mut io,
    };
    assert_eq!(
        cx.http_replacement_contexts(&parent, &[policy(), policy()]),
        Err(Error::Capacity)
    );
    assert_eq!(cx.io.next_id, ResourceIds::starting_at(MAX_RESOURCE_ID));
    unchanged(&cx.io.request_contexts, &parent, 11, 1);
    assert!(!cx.io.next_id.failed());
    assert!(cx.io.pending.is_empty());
    assert!(cx.io.actions.is_empty());
    assert!(cx.io.timers.is_empty());
    let children = cx.http_replacement_contexts(&parent, &[policy()]).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].leg(), MAX_RESOURCE_ID);
    assert_eq!(children[0].owner(), parent.owner());
    assert_eq!(children[0].deadline(), parent.deadline());
    assert_eq!(cx.io.next_id, ResourceIds::starting_at(MAX_RESOURCE_ID + 1));
    assert_eq!(cx.io.reserve_ids(1), None); // Exclusive-end sentinel is not issued.
    assert!(!cx.io.next_id.failed());
    assert!(cx.io.pending.is_empty());
    assert!(cx.io.actions.is_empty());
    assert!(cx.io.timers.is_empty());
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
    io.next_id = ResourceIds::starting_at(MAX_RESOURCE_ID);
    let mut cx = Cx {
        now: 1.,
        store: &mut store,
        io: &mut io,
    };
    assert_eq!(
        cx.http_replacement_contexts(&parent, &[policy(), policy()]),
        Err(Error::Capacity)
    );
    assert_eq!(cx.io.next_id, ResourceIds::starting_at(MAX_RESOURCE_ID));
    unchanged(&cx.io.request_contexts, &parent, 11, 1);
    let mut invalid = policy();
    invalid.timeout_ms = 0;
    assert_eq!(
        cx.http_replacement_contexts(&parent, &[invalid]),
        Err(Error::Invalid)
    );
    assert_eq!(
        cx.io.next_id,
        ResourceIds::starting_at(MAX_RESOURCE_ID + 1),
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

#[test]
fn public_replacement_requires_current_delivery_and_retry_permission() {
    for case in 0..3 {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let parent_policy = Policy {
            retry: if case == 2 {
                RetryPolicy::Never
            } else {
                RetryPolicy::DefinitiveRefusal
            },
            ..policy()
        };
        let parent = io
            .request_contexts
            .issue(1, Scope::Command(7), parent_policy.clone(), 0.)
            .unwrap();
        let correct = refusal(&mut io.request_contexts, &parent, 11);
        io.http_delivery = match case {
            0 => None,
            1 => Some(Delivery {
                attempt: Attempt {
                    request: RequestId(99),
                    generation: 99,
                },
                ..correct.clone()
            }),
            _ => Some(correct.clone()),
        };
        io.next_id = ResourceIds::starting_at(20);
        let mut cx = Cx {
            now: 1.,
            store: &mut store,
            io: &mut io,
        };
        let expected = match case {
            0 => Error::State,
            1 => Error::Revoked,
            _ => Error::RetryForbidden,
        };
        assert_eq!(
            cx.http_replacement_contexts(&parent, std::slice::from_ref(&parent_policy)),
            Err(expected)
        );
        unchanged(&cx.io.request_contexts, &parent, 11, 1);
        assert_eq!(
            cx.io.request_contexts.contexts.get(&parent.leg()),
            Some(&parent)
        );
        assert_eq!(
            cx.io.next_id,
            ResourceIds::starting_at(if case == 0 { 20 } else { 21 })
        );
        assert!(cx.io.pending.is_empty());
        assert!(cx.io.actions.is_empty());
        if case != 2 {
            cx.io.http_delivery = Some(correct);
            let children = cx
                .http_replacement_contexts(&parent, std::slice::from_ref(&parent_policy))
                .unwrap();
            assert_eq!(children.len(), 1);
            assert_eq!(children[0].leg(), if case == 0 { 20 } else { 21 });
            assert_eq!(
                cx.io.request_contexts.validate(&parent, 1.),
                Err(Error::Revoked)
            );
        }
    }
}

#[derive(Debug, PartialEq)]
struct RegistryState {
    contexts: crate::IdMap<u64, Context>,
    generation: u64,
    completion_root: Context,
    last_deadline: f64,
    mutation_declared: bool,
    pending: usize,
    deliveries: usize,
    latest: Option<RequestId>,
    reports: Vec<String>,
    rejections: Vec<(RequestId, VenueError)>,
}

fn registry_state(registry: &ContextRegistry, parent: &Context) -> RegistryState {
    let owner = &registry.commands[&7];
    let writes = owner.writes.as_ref().unwrap();
    RegistryState {
        contexts: registry.contexts.clone(),
        generation: owner.generation,
        completion_root: owner.context_root().unwrap().clone(),
        last_deadline: owner.last_deadline,
        mutation_declared: owner.mutation_declared,
        pending: owner.pending,
        deliveries: owner.deliveries,
        latest: writes.latest(parent.leg()),
        reports: writes.reports().map(str::to_owned).collect(),
        rejections: writes
            .rejections()
            .map(|(id, error)| (id, error.clone()))
            .collect(),
    }
}

#[test]
fn replacement_second_child_collision_defers_cleanup_and_commits_no_authority() {
    let mut io = super::super::Io::<()>::default();
    let mut store = Store::default();
    let parent = io
        .request_contexts
        .issue(1, Scope::Command(7), policy(), 0.)
        .unwrap();
    for (identity, timeout_ms) in [(99, 1), (21, 100)] {
        io.request_contexts
            .issue(
                identity,
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    timeout_ms,
                    retry: RetryPolicy::Never,
                    ..policy()
                },
                0.,
            )
            .unwrap();
    }
    io.http_delivery = Some(refusal(&mut io.request_contexts, &parent, 11));
    io.next_id = ResourceIds::starting_at(20);
    let before = registry_state(&io.request_contexts, &parent);
    let mut cx = Cx {
        now: 2.,
        store: &mut store,
        io: &mut io,
    };
    assert_eq!(
        cx.http_replacement_contexts(&parent, &[policy(), policy()]),
        Err(Error::Duplicate)
    );
    assert_eq!(registry_state(&cx.io.request_contexts, &parent), before);
    assert_eq!(
        cx.io.next_id,
        ResourceIds::starting_at(22),
        "the denied checked range is never reused"
    );
    assert!(!cx.io.request_contexts.contexts.contains_key(&20));
    assert!(
        cx.io.request_contexts.contexts.contains_key(&99),
        "denial cannot perform expiry cleanup"
    );
    assert!(cx.io.pending.is_empty());
    assert!(cx.io.actions.is_empty());
    let children = cx
        .http_replacement_contexts(&parent, &[policy(), policy()])
        .unwrap();
    assert_eq!(
        children.iter().map(Context::leg).collect::<Vec<_>>(),
        [22, 23]
    );
    assert!(
        children
            .iter()
            .all(|child| child.deadline() == 100. && child.owner() == parent.owner())
    );
    assert_eq!(cx.io.next_id, ResourceIds::starting_at(24));
    assert!(!cx.io.request_contexts.contexts.contains_key(&99));
    assert!(cx.io.request_contexts.contexts.contains_key(&21));
    assert_eq!(
        cx.io.request_contexts.validate(&parent, 2.),
        Err(Error::Revoked)
    );
    assert!(cx.io.pending.is_empty());
    assert!(cx.io.actions.is_empty());
}

#[test]
fn replacement_final_journal_denial_preserves_owner_contexts_and_money_facts() {
    for case in 0..5 {
        let mut registry = ContextRegistry::default();
        let parent = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
        registry
            .register_attempt(&parent, RequestId(11), 0.)
            .unwrap();
        *registry
            .release_flag(&parent, RequestId(11))
            .unwrap()
            .unwrap() = true;
        let delivery = refusal_delivery(&parent);
        let fact = match case {
            0 | 4 => Some(WriteResult::Accepted {
                order_ids: vec!["known-exact-id".into()],
            }),
            1 => Some(WriteResult::KnownPartial(
                VenueError::new("known effects").partial_ids(&["known-exact-id".into()]),
            )),
            2 => Some(WriteResult::Failed(
                VenueError::new("lost reply").partial_ids(&["known-exact-id".into()]),
            )),
            _ => None,
        };
        if let Some(fact) = fact {
            registry.result(&delivery, fact).unwrap();
        }
        let before = registry_state(&registry, &parent);
        assert_eq!(
            registry.replace(&parent, &delivery, 20, &[policy(), policy()], 1.),
            Err(Error::RetryForbidden)
        );
        assert_eq!(registry_state(&registry, &parent), before);
        assert!(!registry.contexts.contains_key(&20));
        assert!(!registry.contexts.contains_key(&21));
        let fallback = (case == 4).then(|| VenueError::new("presentation failure"));
        let result = registry.seal(7, Some(&delivery), Some(&parent), fallback);
        match (case, result) {
            (0, Seal::Success) => {}
            (1..=4, Seal::Failure(error, _)) => {
                assert_eq!(
                    error.outcome,
                    if matches!(case, 1 | 4) {
                        None
                    } else {
                        Some(Outcome::Unknown)
                    }
                );
                if case != 3 {
                    assert_eq!(error.partial.unwrap().order_ids, ["known-exact-id"]);
                }
            }
            _ => panic!("denied replacement cannot rewrite the original money fact"),
        }
    }
}

#[test]
fn replacement_old_command_generation_cannot_mutate_the_reissued_owner() {
    let mut registry = ContextRegistry::default();
    let old = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
    let old_delivery = refusal(&mut registry, &old, 11);
    registry.finish(7);
    let current = registry.issue(2, Scope::Command(7), policy(), 1.).unwrap();
    let current_delivery = refusal(&mut registry, &current, 11);
    assert_ne!(old.owner(), current.owner());
    let before = registry_state(&registry, &current);
    for (argument, delivery) in [
        (&old, &old_delivery),
        (&current, &old_delivery),
        (&old, &current_delivery),
    ] {
        assert_eq!(
            registry.replace(argument, delivery, 20, &[policy()], 2.),
            Err(Error::Revoked)
        );
        assert_eq!(registry_state(&registry, &current), before);
    }
    let child = registry
        .replace(&current, &current_delivery, 20, &[policy()], 2.)
        .unwrap();
    assert_eq!(child[0].owner(), current.owner());
    assert_eq!(child[0].deadline(), 101.);
}

#[derive(Default)]
struct PhysicalReplacement {
    root: Option<Context>,
    children: Vec<Context>,
    callbacks: usize,
    signed: bool,
    settled_children: usize,
    accepted_ids: Vec<String>,
    fail_child_staging: bool,
    immediate_failure_sealed: bool,
}

fn physical_policy() -> Policy {
    Policy {
        credential: None,
        pacing: None,
        ..policy()
    }
}

impl PhysicalReplacement {
    fn request_policy(&self) -> Policy {
        Policy {
            credential: self.signed.then(|| policy().credential.unwrap()),
            ..physical_policy()
        }
    }

    fn finish_child(&mut self, cx: &mut Cx<'_, Context>) {
        self.settled_children += 1;
        if self.settled_children == 2 {
            // Deliberately optimistic: only the engine may decide whether a
            // refused or unresolved replacement child prevents success.
            cx.done_context(
                self.root.as_ref().unwrap(),
                serde_json::to_string(&self.accepted_ids).unwrap(),
            )
            .unwrap();
        }
    }
}

struct ReplacementRecipe;
impl crate::SignStep for ReplacementRecipe {
    fn freshness_ms(&self) -> f64 {
        1_000.
    }

    fn prepare(
        &mut self,
        _: &mut crate::Action,
        cx: &crate::DispatchContext<'_>,
    ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
        Ok(crate::signing::SignRequest {
            receipt: crate::signing::SignReceipt {
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: None,
            },
            context: Default::default(),
            operations: vec![crate::signing::Operation::Present {
                key: crate::signing::KeyRef("replacement-test".into()),
            }],
        })
    }

    fn apply(
        &mut self,
        action: &mut crate::Action,
        result: &crate::signing::SignResult,
    ) -> Result<(), crate::signing::SignError> {
        let crate::Action::Http(request) = action else {
            return Err(crate::signing::SignError::InvalidRequest);
        };
        request.signer_headers.push((
            "x-test-credential".into(),
            String::from_utf8(result.values[0].clone())
                .map_err(|_| crate::signing::SignError::InvalidResult)?,
        ));
        Ok(())
    }
}

impl crate::Venue for PhysicalReplacement {
    type Pending = Context;
    fn capture_http_reconciliation(
        &self,
        _: &Context,
    ) -> Result<crate::request_context::ReconciliationData, Error> {
        crate::request_context::ReconciliationData::new("replacement-native-evidence".into())
    }
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        let root = cx
            .http_context(Scope::Command(7), self.request_policy())
            .unwrap();
        if self.signed {
            cx.request_signed_managed(
                root.clone(),
                crate::Method::Post,
                "https://replacement.test/write".into(),
                None,
                Box::new(ReplacementRecipe),
                root.clone(),
            )
            .unwrap();
        } else {
            cx.request_managed(
                root.clone(),
                crate::Method::Post,
                "https://replacement.test/write".into(),
                None,
                root.clone(),
            )
            .unwrap();
        }
        self.root = Some(root);
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Context>,
        root: Context,
        response: crate::Response<'_>,
    ) {
        self.callbacks += 1;
        if self.signed && self.root.as_ref().unwrap().leg() != root.leg() {
            let id: String = serde_json::from_str(response.body).unwrap();
            cx.http_write_result(
                &root,
                WriteResult::Accepted {
                    order_ids: vec![id.clone()],
                },
            )
            .unwrap();
            self.accepted_ids.push(id);
            self.finish_child(cx);
            return;
        }
        cx.http_write_result(
            &root,
            WriteResult::Rejected(VenueError::new("native batch refusal")),
        )
        .unwrap();
        cx.command_error_context(&root, "native batch refused; replacing with singles")
            .unwrap();
        self.children = cx
            .http_replacement_contexts(&root, &[self.request_policy(), self.request_policy()])
            .unwrap();
        // This is the actual original HTTP callback, not fabricated Delivery.
        // Passing the retained root cannot launder superseded physical authority.
        assert_eq!(cx.done_context(&root, "[]".into()), Err(Error::Revoked));
        assert_eq!(cx.fail_context(&root, "old callback"), Err(Error::Revoked));
        assert_eq!(
            cx.command_error_context(&root, "old diagnostic"),
            Err(Error::Revoked)
        );
        assert!(
            cx.http_write_result(&root, WriteResult::Accepted { order_ids: vec![] })
                .is_err()
        );
        assert!(
            cx.http_retry_context(&root, RetryEvidence::DefinitiveRefusal)
                .is_err()
        );
        if self.fail_child_staging {
            let background = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        retry: RetryPolicy::Never,
                        ..physical_policy()
                    },
                )
                .unwrap();
            for _ in 0..255 {
                cx.request_managed(
                    background.clone(),
                    crate::Method::Get,
                    "https://replacement.test/unrelated".into(),
                    None,
                    background.clone(),
                )
                .unwrap();
            }
            let child = self.children[0].clone();
            let failure = cx
                .request_managed(
                    child.clone(),
                    crate::Method::Post,
                    "https://replacement.test/refused-child".into(),
                    None,
                    child.clone(),
                )
                .unwrap_err();
            assert_eq!(failure, (child, Error::Capacity));
            self.immediate_failure_sealed = cx
                .fail_replacement_context(
                    &root,
                    &failure.0,
                    VenueError::new("replacement child staging refused"),
                )
                .is_ok();
            return;
        }
        if self.signed {
            for child in &self.children {
                cx.request_signed_managed(
                    child.clone(),
                    crate::Method::Post,
                    "https://replacement.test/single".into(),
                    None,
                    Box::new(ReplacementRecipe),
                    child.clone(),
                )
                .unwrap();
            }
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Context>,
        child: Context,
        _: Error,
        error: VenueError,
    ) {
        assert!(self.signed);
        assert!(self.children.contains(&child));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        // Attempt failure has already been registered by the engine.
        self.finish_child(cx);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: crate::SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: crate::SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: crate::SocketId, _: u16) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, error: &str) {
        panic!("unexpected physical failure: {error}");
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: crate::TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, Context>, _: crate::Command) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

#[test]
fn replacement_real_handoff_revokes_parent_but_retains_exact_expired_root_completion() {
    use crate::request_context::{Claim, HttpAction, Prepared};
    use crate::{Action, Event, Notice};
    let mut engine = crate::test_support::seeded_engine(PhysicalReplacement::default());
    engine.start(0.);
    let actions: Vec<_> = engine.drain_actions().collect();
    let preparation = actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare { preparation, .. } => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.http_prepared(
        1.,
        preparation,
        Prepared {
            id: 55,
            expires_at: None,
        },
    );
    let permit = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        engine.claim_http(2., permit),
        Ok(Claim::Released(_))
    ));
    engine.handle(
        3.,
        Event::HttpResponse {
            request: preparation.attempt().request,
            response: crate::Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    assert_eq!(engine.venue().callbacks, 1);
    let root = engine.venue().root.as_ref().unwrap().clone();
    assert_eq!(engine.venue().children.len(), 2);
    assert!(engine.claim_http(4., permit).is_err());
    engine.http_prepared(
        4.,
        preparation,
        Prepared {
            id: 55,
            expires_at: None,
        },
    );
    engine.handle(
        4.,
        Event::HttpResponse {
            request: preparation.attempt().request,
            response: crate::Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    assert_eq!(engine.venue().callbacks, 1);
    let after: Vec<_> = engine.drain_actions().collect();
    assert!(!after.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
    assert_eq!(
        after
            .iter()
            .filter(|action| matches!(action,
                Action::Notice(Notice::CommandError { command: 7, message })
                    if message == "native batch refused; replacing with singles"
            ))
            .count(),
        1,
        "the original batch diagnostic survives exactly once"
    );
    // New unrelated work cleans the expired root handle. The very SAME root
    // was both original logical authority and the replaced physical parent.
    let mut cx = Cx {
        now: 100.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.http_context(
        Scope::Background,
        Policy {
            operation: Operation::Read,
            retry: RetryPolicy::Never,
            ..physical_policy()
        },
    )
    .unwrap();
    assert!(!cx.io.request_contexts.contexts.contains_key(&root.leg()));
    assert_eq!(
        cx.io.request_contexts.validate(&root, cx.now),
        Err(Error::TimedOut)
    );
    let altered = root.clone().with_retry(RetryPolicy::Never).unwrap();
    assert_eq!(cx.done_context(&altered, "[]".into()), Err(Error::Revoked));
    cx.fail_context(&root, "logical deadline").unwrap();
    assert!(
        matches!(cx.io.actions.last(), Some(Action::Notice(Notice::CommandFailed { command: 7, error, .. })) if error.outcome == Some(Outcome::Unknown))
    );
    assert_eq!(cx.fail_context(&root, "duplicate"), Err(Error::Revoked));
    let next = cx
        .http_context(Scope::Command(7), physical_policy())
        .unwrap();
    assert_ne!(next.owner(), root.owner());
    assert_eq!(cx.done_context(&root, "[]".into()), Err(Error::Revoked));
    assert_eq!(cx.validate_completion_context(&next), Ok(()));
}

#[test]
fn replacement_child_capacity_abort_finishes_immediately_without_parent_authority_bypass() {
    use crate::request_context::{Claim, HttpAction, Prepared};
    use crate::{Action, Event, Notice};
    let mut engine = crate::test_support::seeded_engine(PhysicalReplacement {
        fail_child_staging: true,
        ..Default::default()
    });
    engine.start(0.);
    let preparation = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.http_prepared(
        1.,
        preparation,
        Prepared {
            id: 55,
            expires_at: None,
        },
    );
    let permit = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        engine.claim_http(2., permit),
        Ok(Claim::Released(_))
    ));
    engine.handle(
        3.,
        Event::HttpResponse {
            request: preparation.attempt().request,
            response: crate::Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    assert!(
        engine.venue().immediate_failure_sealed,
        "a refused replacement child must retire immediately in its parent's callback"
    );
    assert_eq!(engine.venue().callbacks, 1);
    assert!(!engine.io.request_contexts.commands.contains_key(&7));
    for child in &engine.venue().children {
        assert!(
            !engine
                .io
                .request_contexts
                .contexts
                .contains_key(&child.leg())
        );
    }
    assert_eq!(
        engine.io.managed_http.len(),
        255,
        "unrelated routes survive"
    );
    assert_eq!(engine.io.pending.len(), 255);
    assert_eq!(engine.io.timers.len(), 255);
    assert!(
        engine
            .io
            .managed_http
            .values()
            .all(|m| matches!(m.delivery().context.owner(), Owner::Background { .. }))
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].outcome, Some(Outcome::Unknown));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
    engine.handle(
        4.,
        Event::HttpResponse {
            request: preparation.attempt().request,
            response: crate::Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    assert_eq!(engine.venue().callbacks, 1);
    assert!(!engine.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
    )));
}

mod signed_replacement_handoff {
    use super::*;
    use crate::request_context::{Claim, HttpAction, Permit, Preparation, Prepared};
    use crate::signing::{SignError, SignRequest, SignResult};
    use crate::{Action, Event, Notice};

    fn preparations(actions: &[Action]) -> Vec<Preparation> {
        actions
            .iter()
            .filter_map(|action| match action {
                Action::ManagedHttp(action) => match action.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }
    fn route(
        engine: &mut crate::Engine<PhysicalReplacement>,
        preparation: Preparation,
        now: f64,
    ) -> (RequestId, SignRequest) {
        engine.http_prepared(
            now,
            preparation,
            Prepared {
                id: preparation.attempt().request.0,
                expires_at: None,
            },
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(!actions.iter().any(|a| match a {
            Action::Http(_) => true,
            Action::ManagedHttp(a) => matches!(a.as_ref(), HttpAction::Bind { .. }),
            _ => false,
        }));
        let work: Vec<_> = actions
            .into_iter()
            .filter_map(|a| match a {
                Action::Sign { attempt, work, .. } => Some((attempt, work)),
                _ => None,
            })
            .collect();
        assert_eq!(work.len(), 1);
        work.into_iter().next().unwrap()
    }
    fn result(work: &SignRequest) -> Result<SignResult, SignError> {
        Ok(SignResult::new(vec![b"presented".to_vec()], work.receipt))
    }
    fn finish(
        engine: &mut crate::Engine<PhysicalReplacement>,
        attempt: RequestId,
        value: Result<SignResult, SignError>,
        now: f64,
    ) -> Vec<Action> {
        engine.handle(
            now,
            Event::Signed {
                attempt,
                result: value,
            },
        );
        engine.drain_actions().collect()
    }
    fn permit(actions: &[Action]) -> Permit {
        actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Bind { permit, .. } => Some(*permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("validated signature must bind its child")
    }
    fn response(
        engine: &mut crate::Engine<PhysicalReplacement>,
        token: Preparation,
        at: f64,
        body: &str,
    ) {
        engine.handle(
            at,
            Event::HttpResponse {
                request: token.attempt().request,
                response: crate::Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }

    #[test]
    fn signed_replacement_children_keep_release_evidence_and_original_deadline() {
        for mode in 0..3 {
            let mut engine = crate::test_support::seeded_engine(PhysicalReplacement {
                signed: true,
                ..Default::default()
            });
            engine.start(0.);
            let root = preparations(&engine.drain_actions().collect::<Vec<_>>())[0];
            let (parent_attempt, parent_work) = route(&mut engine, root, 1.);
            let parent_ready = finish(&mut engine, parent_attempt, result(&parent_work), 2.);
            let parent_permit = permit(&parent_ready);
            assert!(matches!(
                engine.claim_http(2., parent_permit),
                Ok(Claim::Released(_))
            ));
            let mut released = 1;
            response(&mut engine, root, 3., "{}");
            let mut observed: Vec<_> = engine.drain_actions().collect();
            let children = preparations(&observed);
            assert_eq!(children.len(), 2);
            for child in &engine.venue().children {
                assert_eq!(child.deadline(), 100.);
            }
            let first = route(&mut engine, children[0], 4.);
            let second = route(&mut engine, children[1], 5.);
            // No provider completion means no bound packet or physical release.
            assert_eq!(released, 1);
            assert!(engine.venue().accepted_ids.is_empty());
            let mut child_permits = Vec::new();
            if mode != 2 {
                let ready = finish(&mut engine, first.0, result(&first.1), 70.);
                let p = permit(&ready);
                observed.extend(ready);
                assert!(matches!(engine.claim_http(71., p), Ok(Claim::Released(_))));
                assert!(
                    engine.claim_http(71.5, p).is_err(),
                    "live permit is single-use"
                );
                assert_eq!(engine.venue().settled_children, 0);
                released += 1;
                child_permits.push(p);
                response(&mut engine, children[0], 72., "\"accepted-child-1\"");
                observed.extend(engine.drain_actions());
            } else {
                observed.extend(finish(
                    &mut engine,
                    first.0,
                    Err(SignError::Unavailable),
                    70.,
                ));
            }
            assert_eq!(engine.venue().settled_children, 1);
            if mode == 0 {
                let ready = finish(&mut engine, second.0, result(&second.1), 73.);
                let p = permit(&ready);
                observed.extend(ready);
                assert!(matches!(engine.claim_http(74., p), Ok(Claim::Released(_))));
                released += 1;
                child_permits.push(p);
                response(&mut engine, children[1], 75., "\"accepted-child-2\"");
                observed.extend(engine.drain_actions());
            } else if mode == 1 {
                observed.extend(finish(
                    &mut engine,
                    second.0,
                    Err(SignError::Unavailable),
                    73.,
                ));
            } else {
                // The recipe freshness is1000ms, but the original raw cap is100ms.
                let late = finish(&mut engine, second.0, result(&second.1), 100.);
                assert!(!late.iter().any(|a| matches!(a, Action::ManagedHttp(a)
                if matches!(a.as_ref(), HttpAction::Bind { .. }))));
                observed.extend(late);
            }
            assert_eq!(released, 3 - mode);
            assert_eq!(engine.venue().settled_children, 2);
            assert_eq!(engine.venue().callbacks, released);
            assert!(engine.io.pending.is_empty());
            assert!(engine.io.managed_http.is_empty());
            assert!(!engine.signing_pending(parent_attempt));
            assert!(!engine.signing_pending(first.0));
            assert!(!engine.signing_pending(second.0));
            let terminals: Vec<_> = observed
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(
                        n @ (Notice::CommandDone { .. } | Notice::CommandFailed { .. }),
                    ) => Some(n),
                    _ => None,
                })
                .collect();
            assert_eq!(terminals.len(), 1);
            match terminals[0] {
                Notice::CommandDone { command: 7, result } if mode == 0 => {
                    assert_eq!(
                        serde_json::from_str::<Vec<String>>(result).unwrap(),
                        ["accepted-child-1", "accepted-child-2"]
                    );
                }
                Notice::CommandFailed { command: 7, error } if mode != 0 => {
                    assert_eq!(error.outcome, Some(Outcome::Unknown));
                    let ids = error
                        .partial
                        .as_ref()
                        .map(|p| p.order_ids.clone())
                        .unwrap_or_default();
                    assert_eq!(
                        ids,
                        if mode == 1 {
                            vec!["accepted-child-1"]
                        } else {
                            vec![]
                        }
                    );
                }
                other => panic!("wrong replacement seal: {other:?}"),
            }
            assert_eq!(
                observed
                    .iter()
                    .filter(|a| matches!(a,
                        Action::Notice(Notice::CommandError { command: 7, message })
                            if message == "native batch refused; replacing with singles"
                    ))
                    .count(),
                1
            );
            assert!(engine.claim_http(101., parent_permit).is_err());
            for p in child_permits {
                assert!(engine.claim_http(101., p).is_err());
            }
            response(&mut engine, root, 101., "{}");
            for (attempt, work) in [first, second] {
                let late = finish(&mut engine, attempt, result(&work), 101.);
                assert!(!late.iter().any(|a| matches!(
                    a,
                    Action::Notice(
                        Notice::CommandDone { .. }
                            | Notice::CommandFailed { .. }
                            | Notice::CommandError { .. }
                    )
                )));
            }
            assert_eq!(engine.venue().settled_children, 2);
            assert_eq!(engine.venue().callbacks, released);
        }
    }

    #[test]
    fn replacement_child_reprepare_fences_live_old_permit_and_keeps_original_authority() {
        for calibrated in [false, true] {
            let mut engine = crate::test_support::seeded_engine(PhysicalReplacement {
                signed: true,
                ..Default::default()
            });
            engine.start(0.);
            let root = preparations(&engine.drain_actions().collect::<Vec<_>>())[0];
            let (parent, parent_work) = route(&mut engine, root, 1.);
            let ready = finish(&mut engine, parent, result(&parent_work), 2.);
            assert!(matches!(
                engine.claim_http(2., permit(&ready)),
                Ok(Claim::Released(_))
            ));
            response(&mut engine, root, 3., "{}");
            let mut observed: Vec<_> = engine.drain_actions().collect();
            let children = preparations(&observed);
            assert_eq!(children.len(), 2);
            let child_context = engine.venue().children[0].clone();
            let old = children[0];
            let (old_signer, old_work) = route(&mut engine, old, 4.);
            let ready = finish(&mut engine, old_signer, result(&old_work), 5.);
            let old_permit = permit(&ready);
            assert_eq!(old_work.receipt.stamp_ms, Some(4.));
            if calibrated {
                assert!(engine.io.protocol_clock.observe(10., 510.));
                assert_eq!(engine.claim_http(10., old_permit), Err(Error::Stale));
            } else {
                engine.http_preparation_failed(
                    10.,
                    old,
                    Error::Stale,
                    VenueError::new("route expired"),
                );
            }
            let invalidated: Vec<_> = engine.drain_actions().collect();
            assert!(
                invalidated
                    .iter()
                    .any(|a| matches!(a, Action::ManagedHttp(a)
                if matches!(a.as_ref(), HttpAction::Cancel { preparation, signing: Some(id) }
                    if *preparation == old && *id == old_signer)))
            );
            let refreshed = preparations(&invalidated);
            assert_eq!(refreshed.len(), 1);
            let next = refreshed[0];
            assert_ne!(next, old);
            assert_eq!(next.attempt(), old.attempt());
            assert_eq!(
                engine.io.managed_http[&next.attempt().request]
                    .delivery()
                    .context,
                child_context
            );
            assert_eq!(child_context.deadline(), 100.);
            assert!(
                invalidated
                    .iter()
                    .any(|a| matches!(a, Action::ManagedHttp(a)
                if matches!(a.as_ref(), HttpAction::Prepare { request, .. }
                    if request.timeout_ms == Some(90))))
            );
            assert!(engine.http_release(old.attempt().request).is_none());
            assert_eq!(engine.venue().callbacks, 1);
            assert_eq!(engine.venue().settled_children, 0);
            assert_eq!(engine.claim_http(11., old_permit), Err(Error::Revoked));
            engine.http_prepared(
                12.,
                old,
                Prepared {
                    id: 900,
                    expires_at: None,
                },
            );
            engine.http_preparation_failed(
                13.,
                old,
                Error::Invalid,
                VenueError::new("old cleanup"),
            );
            let stale = finish(&mut engine, old_signer, result(&old_work), 14.);
            assert!(!stale.iter().any(|a| matches!(
                a,
                Action::Http(_)
                    | Action::Notice(
                        Notice::CommandDone { .. }
                            | Notice::CommandFailed { .. }
                            | Notice::CommandError { .. }
                    )
            )));
            assert!(!stale.iter().any(|a| matches!(a, Action::ManagedHttp(a)
                if matches!(a.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
            assert!(engine.http_preparation_pending(next));
            let (fresh_signer, fresh_work) = route(&mut engine, next, 20.);
            assert_ne!(fresh_signer, old_signer);
            assert_eq!(
                fresh_work.receipt.stamp_ms,
                Some(if calibrated { 520. } else { 20. })
            );
            let fresh_ready = finish(&mut engine, fresh_signer, result(&fresh_work), 21.);
            let fresh_permit = permit(&fresh_ready);
            let Ok(Claim::Released(release)) = engine.claim_http(22., fresh_permit) else {
                panic!("fresh replacement child must release");
            };
            assert_eq!(release.time.raw_ms, 22.);
            assert_eq!(
                release.time.protocol_ms,
                if calibrated { 522. } else { 22. }
            );
            // This recipe has no absolute signature expiry; the raw Context
            // deadline still bounds release independently of receipt metadata.
            assert_eq!(release.signature_valid_until, None);
            response(&mut engine, next, 23., "\"accepted-child-1\"");
            observed.extend(engine.drain_actions());
            // The second child is first routed after the global calibration.
            let (second, second_work) = route(&mut engine, children[1], 24.);
            assert_eq!(
                second_work.receipt.stamp_ms,
                Some(if calibrated { 524. } else { 24. })
            );
            let second_ready = finish(&mut engine, second, result(&second_work), 25.);
            assert!(matches!(
                engine.claim_http(26., permit(&second_ready)),
                Ok(Claim::Released(_))
            ));
            response(&mut engine, children[1], 27., "\"accepted-child-2\"");
            observed.extend(engine.drain_actions());
            assert_eq!(engine.venue().callbacks, 3);
            assert_eq!(engine.venue().settled_children, 2);
            let terminals: Vec<_> = observed
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(
                        n @ (Notice::CommandDone { .. } | Notice::CommandFailed { .. }),
                    ) => Some(n),
                    _ => None,
                })
                .collect();
            assert_eq!(terminals.len(), 1);
            let Notice::CommandDone { command: 7, result } = terminals[0] else {
                panic!("stale preparation must not poison replacement facts");
            };
            assert_eq!(
                serde_json::from_str::<Vec<String>>(result).unwrap(),
                ["accepted-child-1", "accepted-child-2"]
            );
            assert_eq!(
                observed
                    .iter()
                    .filter(
                        |a| matches!(a, Action::Notice(Notice::CommandError { command: 7, message })
                if message == "native batch refused; replacing with singles")
                    )
                    .count(),
                1
            );
            assert!(engine.io.pending.is_empty());
            assert!(engine.io.managed_http.is_empty());
            for id in [parent, old_signer, fresh_signer, second] {
                assert!(!engine.signing_pending(id));
            }
        }
    }
}

fn replacement_failure_fixture() -> (ContextRegistry, Context, Vec<Context>, Delivery) {
    let mut registry = ContextRegistry::default();
    let root = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
    let delivery = refusal(&mut registry, &root, 11);
    let children = registry
        .replace(&root, &delivery, 20, &[policy(), policy()], 1.)
        .unwrap();
    (registry, root, children, delivery)
}

#[test]
fn replacement_failure_denials_preserve_journal_and_all_owner_resources() {
    for case in 0..10 {
        let (mut registry, root, children, mut delivery) = replacement_failure_fixture();
        let mut argument_root = root.clone();
        let mut argument_child = children[0].clone();
        let now = if case == 6 { 100. } else { 2. };
        match case {
            0 => {
                argument_root = ContextRegistry::default()
                    .issue(1, Scope::Command(7), policy(), 0.)
                    .unwrap()
            }
            1 => argument_root = children[1].clone(),
            2 => {
                let (foreign, _, foreign_children, _) = replacement_failure_fixture();
                argument_child = foreign_children[0].clone();
                drop(foreign);
            }
            3 => argument_child = registry.issue(30, Scope::Command(7), policy(), 1.).unwrap(),
            4 => delivery.context = children[1].clone(),
            5 => delivery.attempt.request = RequestId(99),
            6 => (),
            7 => {
                registry.contexts.remove(&argument_child.leg());
            }
            8 => argument_root = root.clone().with_retry(RetryPolicy::Never).unwrap(),
            _ => argument_root = registry.issue(30, Scope::Command(8), policy(), 1.).unwrap(),
        }
        let before = registry.commands[&7]
            .writes
            .as_ref()
            .unwrap()
            .project_failure(VenueError::new("retained facts"))
            .unwrap();
        let contexts = registry.contexts.clone();
        let pending = registry.commands[&7].pending;
        let deliveries = registry.commands[&7].deliveries;
        assert!(
            registry
                .seal_replacement_failure(
                    &argument_root,
                    &argument_child,
                    &delivery,
                    now,
                    VenueError::new("denied abort")
                )
                .is_err(),
            "case {case}"
        );
        assert_eq!(registry.contexts, contexts, "case {case}");
        assert_eq!(registry.commands[&7].pending, pending);
        assert_eq!(registry.commands[&7].deliveries, deliveries);
        let writes = registry.commands[&7].writes.as_ref().unwrap();
        assert_eq!(
            writes
                .project_failure(VenueError::new("retained facts"))
                .unwrap(),
            before
        );
        assert!(writes.reports().next().is_none());
    }
}

#[test]
fn replacement_failure_cannot_borrow_an_older_parent_retry_delivery() {
    let mut registry = ContextRegistry::default();
    let root = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
    let old = refusal(&mut registry, &root, 11);
    registry.grant_resend(&old).unwrap();
    registry.register_attempt(&root, RequestId(12), 1.).unwrap();
    *registry
        .release_flag(&root, RequestId(12))
        .unwrap()
        .unwrap() = true;
    let current = Delivery {
        context: root.clone(),
        attempt: Attempt {
            request: RequestId(12),
            generation: 12,
        },
    };
    registry
        .result(
            &current,
            WriteResult::Rejected(VenueError::new("current refusal")),
        )
        .unwrap();
    let children = registry
        .replace(&root, &current, 20, &[policy()], 2.)
        .unwrap();
    assert!(matches!(
        registry.seal_replacement_failure(
            &root,
            &children[0],
            &old,
            3.,
            VenueError::new("old callback")
        ),
        Err(Error::Revoked)
    ));
    assert!(registry.commands[&7].writes.is_some());
    match registry
        .seal_replacement_failure(
            &root,
            &children[0],
            &current,
            3.,
            VenueError::new("current callback"),
        )
        .unwrap()
    {
        Seal::Failure(error, _) => assert_eq!(error.outcome, Some(Outcome::Unknown)),
        _ => panic!("replacement abort cannot grant success"),
    }
}

#[test]
fn replacement_failure_rejects_a_sent_child_hidden_by_its_unsent_retry() {
    let (mut registry, root, children, delivery) = replacement_failure_fixture();
    let child = &children[0];
    registry.register_attempt(child, RequestId(31), 2.).unwrap();
    *registry
        .release_flag(child, RequestId(31))
        .unwrap()
        .unwrap() = true;
    let child_delivery = Delivery {
        context: child.clone(),
        attempt: Attempt {
            request: RequestId(31),
            generation: 31,
        },
    };
    registry
        .result(
            &child_delivery,
            WriteResult::Rejected(VenueError::new("child refusal")),
        )
        .unwrap();
    registry.grant_resend(&child_delivery).unwrap();
    registry.register_attempt(child, RequestId(32), 3.).unwrap();
    assert_eq!(
        registry.commands[&7]
            .writes
            .as_ref()
            .unwrap()
            .latest(child.leg()),
        Some(RequestId(32))
    );
    assert!(matches!(
        registry.seal_replacement_failure(
            &root,
            child,
            &delivery,
            4.,
            VenueError::new("unsent retry")
        ),
        Err(Error::Revoked)
    ));
    assert_eq!(
        registry.commands[&7]
            .writes
            .as_ref()
            .unwrap()
            .project_failure(VenueError::new("retained sent facts"))
            .unwrap()
            .outcome,
        Some(Outcome::Unknown)
    );
}

#[test]
fn replacement_failure_keeps_retired_original_root_authority_and_rejects_superseded_children() {
    let mut registry = ContextRegistry::default();
    let root = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
    let parent = registry
        .issue_with_egress(2, Scope::Command(7), policy(), 0., Some(&root), None)
        .unwrap();
    let delivery = refusal(&mut registry, &parent, 11);
    let children = registry
        .replace(&parent, &delivery, 20, &[policy(), policy()], 1.)
        .unwrap();
    let child_delivery = refusal(&mut registry, &children[0], 31);
    let grandchildren = registry
        .replace(&children[0], &child_delivery, 40, &[policy()], 2.)
        .unwrap();
    assert!(matches!(
        registry.seal_replacement_failure(
            &root,
            &children[0],
            &delivery,
            3.,
            VenueError::new("superseded child")
        ),
        Err(Error::Revoked)
    ));
    assert!(matches!(
        registry.seal_replacement_failure(
            &root,
            &grandchildren[0],
            &delivery,
            3.,
            VenueError::new("wrong ancestor")
        ),
        Err(Error::Revoked)
    ));
    registry.contexts.remove(&root.leg());
    match registry
        .seal_replacement_failure(
            &root,
            &children[1],
            &delivery,
            3.,
            VenueError::new("exact original root"),
        )
        .unwrap()
    {
        Seal::Failure(error, _) => assert_eq!(error.outcome, Some(Outcome::Unknown)),
        _ => panic!("failure continuation cannot grant success"),
    }
}
