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
        io.next_id = 20;
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
        assert_eq!(cx.io.next_id, if case == 0 { 20 } else { 21 });
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

#[derive(Default)]
struct PhysicalReplacement {
    root: Option<Context>,
    children: Vec<Context>,
    callbacks: usize,
    signed: bool,
    settled_children: usize,
    accepted_ids: Vec<String>,
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
fn signed_replacement_children_keep_release_evidence_and_original_deadline() {
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
                id: u64::from(preparation.attempt().request.0),
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
                Action::Notice(n @ (Notice::CommandDone { .. } | Notice::CommandFailed { .. })) => {
                    Some(n)
                }
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
