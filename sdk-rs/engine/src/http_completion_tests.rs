use super::*;
use crate::signing_dispatch::Completion;

fn fixture() -> (Engine<Probe>, Preparation, RequestId, crate::signing::SignRequest) {
    let (mut engine, tokens) = start(Probe {
        count: 1,
        signed: true,
        ..Probe::default()
    });
    let token = tokens[0];
    let (attempt, input) = work(&route(&mut engine, token, 10.));
    (engine, token, attempt, input)
}

fn complete(
    engine: &mut Engine<Probe>,
    at: f64,
    attempt: RequestId,
    input: &crate::signing::SignRequest,
) -> Completion {
    engine
        .io
        .core
        .finish_signing(
            at,
            attempt,
            Ok(crate::signing::SignResult::new(
                vec![b"value".to_vec()],
                input.receipt,
            )),
        )
        .expect("a genuinely prepared signing task completes once")
}

fn reprepare_packet(actions: &[Action], old: Preparation) -> Preparation {
    let managed: Vec<_> = actions.iter().filter_map(|a| match a {
        Action::ManagedHttp(a) => Some(a.as_ref()),
        _ => None,
    }).collect();
    let [HttpAction::Cancel { preparation, signing: Some(_) }, HttpAction::Prepare { preparation: next, request, .. }] = managed.as_slice() else {
        panic!("cancel then one replacement, no Bind: {actions:?}");
    };
    assert_eq!(*preparation, old);
    assert_ne!(*next, old);
    assert_eq!(request.url, "https://venue.test/read");
    assert_eq!(request.body.as_deref(), Some("{}"));
    assert!(request.signer_headers.is_empty());
    assert_eq!(request.timeout_ms, Some(989));
    *next
}

#[test]
fn ready_completion_retains_exact_packet_and_claims_only_after_bind() {
    let (mut engine, token, attempt, input) = fixture();
    let completion = complete(&mut engine, 11., attempt, &input);
    assert!(matches!(&completion, Completion::Ready(..)));
    engine.managed_sign_completed(11., completion);
    let actions: Vec<_> = engine.drain_actions().collect();
    let [Action::ManagedHttp(bind)] = actions.as_slice() else {
        panic!("one Bind without an extra timer: {actions:?}");
    };
    let HttpAction::Bind { preparation, permit, request } = bind.as_ref() else {
        panic!("one Bind");
    };
    assert_eq!(*preparation, token);
    assert_eq!(request.id, token.attempt().request);
    assert_eq!(request.method, Method::Post);
    assert_eq!(request.url, "https://venue.test/read?stamp=10");
    assert_eq!(request.body.as_deref(), Some("{}"));
    assert!(request.headers.is_empty());
    assert_eq!(request.signer_headers, [("x-credential".into(), "value".into())]);
    assert_eq!(request.timeout_ms, Some(989));
    let managed = &engine.io.core.managed_http[&request.id];
    assert!(managed.recipe.is_some());
    assert!(managed.lifecycle.release().is_none());
    assert_eq!(engine.venue().receipts, 0);
    assert!(matches!(engine.claim_http(12., *permit), Ok(Claim::Released(_))));
    assert_eq!(engine.venue().receipts, 1);
    assert!(engine.io.core.managed_http[&request.id].recipe.is_none());
}

#[test]
fn stale_completion_cancels_before_reprepare_without_consuming_admission() {
    let (mut engine, token, attempt, input) = fixture();
    assert!(engine.io.core.protocol_clock.observe(11., 511.));
    let completion = complete(&mut engine, 11., attempt, &input);
    assert!(matches!(&completion, Completion::Stale(_)));
    engine.managed_sign_completed(11., completion);
    let actions: Vec<_> = engine.drain_actions().collect();
    let next = reprepare_packet(&actions, token);
    assert!(engine.venue().releases.is_empty());
    assert!(engine.venue().failures.is_empty());
    assert!(signed(&mut engine, 12., attempt, input).is_empty());
    let (next_attempt, next_input) = work(&route(&mut engine, next, 13.));
    assert_ne!(next_attempt, attempt);
    assert_eq!(next_input.receipt.stamp_ms, Some(513.));
    bound(&signed(&mut engine, 14., next_attempt, next_input));
}

#[cfg(feature = "dispatch-nonce")]
#[test]
fn nonce_stale_completion_keeps_the_cumulative_reprepare_bound() {
    for repairs in [0, crate::signing_dispatch::MAX_NONCE_REPREPARES] {
        let (mut engine, token, attempt, input) = fixture();
        let Completion::Ready(mut pending, _) = complete(&mut engine, 11., attempt, &input) else {
            panic!("prepared completion");
        };
        // Inject the dispatcher's stale classification, not nonce authority.
        pending.nonce_repairs = repairs;
        engine.managed_sign_completed(11., Completion::NonceStale(pending));
        let actions: Vec<_> = engine.drain_actions().collect();
        if repairs == 0 {
            let next = reprepare_packet(&actions, token);
            let (next_attempt, _) = work(&route(&mut engine, next, 12.));
            let completion = engine.io.core.finish_signing(13., next_attempt, Err(crate::signing::SignError::Refused)).unwrap();
            let Completion::Failed(pending, _) = completion else { panic!("refusal"); };
            assert_eq!(pending.nonce_repairs, 1);
        } else {
            assert_eq!(engine.venue().failure_messages, ["Signing nonce repair budget exhausted"]);
            assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. } | HttpAction::Prepare { .. }))));
        }
        assert!(engine.venue().releases.is_empty());
    }
}

#[test]
fn failed_completion_preserves_the_typed_failure_and_cleanup_once() {
    for error in [crate::signing::SignError::Refused, crate::signing::SignError::TimedOut] {
        let (mut engine, _, attempt, input) = fixture();
        engine.handle(11., Event::Signed { attempt, result: Err(error) });
        let actions: Vec<_> = engine.drain_actions().collect();
        assert_eq!(engine.venue().failures, [(if error == crate::signing::SignError::TimedOut { Error::TimedOut } else { Error::State }, Some(crate::Outcome::NotSent))]);
        assert_eq!(engine.venue().failure_messages, [error.venue_error().message]);
        assert!(matches!(actions.first(), Some(Action::ClearTimer { .. })));
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. } | HttpAction::Prepare { .. }))));
        assert!(signed(&mut engine, 12., attempt, input).is_empty());
        assert_eq!(engine.venue().failures.len(), 1);
    }
}

#[test]
fn completion_refuses_a_different_request_or_outbound_kind() {
    for wrong_kind in [false, true] {
        let (mut engine, _, attempt, input) = fixture();
        let Completion::Ready(pending, mut outbound) = complete(&mut engine, 11., attempt, &input) else { panic!("ready"); };
        // A hostile recipe cannot substitute another packet after signing.
        if wrong_kind {
            outbound = Action::WsSend { socket: SocketId(99), text: "wrong outbound".into() };
        } else {
            let Action::Http(packet) = &mut outbound else { panic!("HTTP"); };
            packet.id = RequestId(999);
        }
        engine.managed_sign_completed(11., Completion::Ready(pending, outbound));
        let actions: Vec<_> = engine.drain_actions().collect();
        assert_eq!(engine.venue().failure_messages, ["HTTP signing result is no longer valid"]);
        assert!(engine.io.core.managed_http.is_empty());
        assert!(engine.venue().releases.is_empty());
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_) | Action::WsSend { .. }) || matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. }))));
    }
}

#[test]
fn a_completed_packet_for_a_retired_or_expired_owner_never_binds() {
    for retire in [false, true] {
        let (mut engine, token, attempt, input) = fixture();
        let completion = complete(&mut engine, 11., attempt, &input);
        if retire {
            engine.io.discard_managed(token.attempt().request);
        }
        engine.managed_sign_completed(if retire { 12. } else { 1000. }, completion);
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. }))));
        assert!(engine.venue().releases.is_empty());
        assert_eq!(engine.venue().failures.len(), usize::from(!retire));
    }
}

#[cfg(feature = "session-bearer")]
#[test]
fn signed_completion_parks_and_wakes_with_only_the_current_bearer() {
    use crate::request_context::SessionPolicy;
    let (mut engine, _) = start(Probe::default());
    let mut cx = Cx { now: 0., store: &mut engine.store, io: &mut engine.io };
    let key = crate::request_context::Credential { slot: CredentialSlot::Primary, reference: crate::signing::KeyRef("fixture".into()) };
    let session = cx.open_session(SessionPolicy { credential: Some(key.clone()), repeat_safe: true, private_egress: None }).unwrap();
    let context = cx.http_context(Scope::Background, Policy { operation: Operation::Read, credential: Some(key), timeout_ms: 1000, pacing: None, retry: RetryPolicy::Never }).unwrap();
    cx.request_signed_managed(context.clone(), Some(&session), Method::Post, "https://venue.test/read".into(), Some("{}".into()), Box::new(Recipe), Pending(context)).unwrap();
    let token = engine.drain_actions().find_map(|a| match a { Action::ManagedHttp(a) => match *a { HttpAction::Prepare { preparation, .. } => Some(preparation), _ => None }, _ => None }).unwrap();
    let Owner::Session { generation } = session.owner() else { unreachable!() };
    engine.venue.session_fixture = Some(session);
    engine.venue.bearer_fixture = Some(1);
    let (attempt, input) = work(&route(&mut engine, token, 10.));
    engine.venue.bearer_fixture = None;
    let actions = signed(&mut engine, 11., attempt, input);
    assert!(matches!(actions.as_slice(), [Action::SetTimer { at_ms: 1000., .. }]));
    assert!(engine.http_bearer_parked(token.attempt().request));
    assert!(engine.io.core.managed_http[&token.attempt().request].recipe.is_some());
    assert!(engine.venue().releases.is_empty());
    engine.venue.bearer_fixture = Some(2);
    engine.wake_bearer_parks(12., generation);
    let actions: Vec<_> = engine.drain_actions().collect();
    let next = actions.iter().find_map(|a| match a { Action::ManagedHttp(a) => match a.as_ref() { HttpAction::Prepare { preparation, request, .. } => { assert!(request.headers.is_empty()); Some(*preparation) }, _ => None }, _ => None }).unwrap();
    assert!(matches!(&actions[0], Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Cancel { preparation, .. } if *preparation == token)));
    let (attempt, input) = work(&route(&mut engine, next, 13.));
    let actions = signed(&mut engine, 14., attempt, input);
    let permit = bound(&actions);
    let [Action::ManagedHttp(bind)] = actions.as_slice() else { panic!("only Bind"); };
    let HttpAction::Bind { request, .. } = bind.as_ref() else { panic!("Bind"); };
    assert_eq!(request.headers, [("Authorization", "fixture-2".into())]);
    assert_eq!(request.url, "https://venue.test/read?stamp=13");
    assert_eq!(request.timeout_ms, Some(986));
    assert!(matches!(engine.claim_http(15., permit), Ok(Claim::Released(_))));
}

#[test]
fn failure_hook_precedes_the_completed_recipes_destructor() {
    struct TrackedRecipe(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl Drop for TrackedRecipe {
        fn drop(&mut self) { self.0.store(false, std::sync::atomic::Ordering::SeqCst); }
    }
    impl crate::SignStep for TrackedRecipe {
        fn prepare(&mut self, outbound: &mut Action, context: &crate::DispatchContext<'_>) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
            Recipe.prepare(outbound, context)
        }
        fn apply(&mut self, outbound: &mut Action, result: &crate::signing::SignResult) -> Result<(), crate::signing::SignError> {
            Recipe.apply(outbound, result)
        }
        fn freshness_ms(&self) -> f64 { 3000. }
    }
    let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let (mut engine, _) = start(Probe { recipe_alive: Some(alive.clone()), ..Probe::default() });
    let mut cx = Cx { now: 0., store: &mut engine.store, io: &mut engine.io };
    let context = cx.http_context(Scope::Background, Policy {
        operation: Operation::Read,
        credential: Some(crate::request_context::Credential { slot: CredentialSlot::Primary, reference: crate::signing::KeyRef("fixture".into()) }),
        timeout_ms: 1000, pacing: None, retry: RetryPolicy::Never,
    }).unwrap();
    cx.request_signed_managed(context.clone(), None, Method::Post, "https://venue.test/read".into(), Some("{}".into()), Box::new(TrackedRecipe(alive.clone())), Pending(context)).unwrap();
    let token = engine.drain_actions().find_map(|a| match a { Action::ManagedHttp(a) => match *a { HttpAction::Prepare { preparation, .. } => Some(preparation), _ => None }, _ => None }).unwrap();
    let (attempt, _) = work(&route(&mut engine, token, 10.));
    engine.handle(11., Event::Signed { attempt, result: Err(crate::signing::SignError::Refused) });
    assert!(!alive.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(engine.venue().failures.len(), 1);
}
