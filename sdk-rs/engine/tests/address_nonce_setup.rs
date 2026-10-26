#![cfg(all(feature = "address-nonce", feature = "test-support"))]
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::nonce::{HYPERLIQUID_AGENT, NonceDomain};
use safe_cex_engine::sign_meta::{SetupOperation, SignKind};
use safe_cex_engine::signing::*;
use safe_cex_engine::*;
use std::sync::{Arc, Mutex};
use std::task::{Context as TaskContext, Poll, Waker};

struct Probe {
    managed: bool,
    domain: Option<&'static NonceDomain>,
    slots: AddressSetupSlots,
    observed: bool,
}
impl Venue for Probe {
    type Pending = ();
    fn address_nonce_domain(&self) -> Option<&'static NonceDomain> {
        self.domain
    }
    fn public_address_slots(&self) -> AddressSetupSlots {
        self.slots
    }
    fn dispatch_nonce() -> bool {
        true
    }
    fn http_release() -> HttpRelease<Self> {
        HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
    }
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        self.observed = cx
            .address_nonce_identities()
            .primary
            .as_ref()
            .is_some_and(|id| id.verified_signing_address().is_ok());
        if self.managed {
            assert!(cx.observe_protocol_time(9000.75));
            let ids = cx.address_nonce_identities();
            let slots = Slots::new(ids.primary.clone(), None, SlotPolicy::default());
            for _ in 0..2 {
                let (_, identity, claim) = slots
                    .select_with_nonce(CredentialRole::Read { scope: None }, self.domain.unwrap())
                    .unwrap();
                let reference = identity.credential.clone();
                let context = cx
                    .http_context(
                        safe_cex_engine::request_context::Scope::Background,
                        safe_cex_engine::request_context::Policy {
                            operation: safe_cex_engine::request_context::Operation::Read,
                            credential: Some(safe_cex_engine::request_context::Credential {
                                slot: CredentialSlot::Primary,
                                reference,
                            }),
                            timeout_ms: 20_000,
                            pacing: None,
                            retry: safe_cex_engine::request_context::RetryPolicy::Never,
                        },
                    )
                    .unwrap();
                cx.request_signed_managed(
                    context,
                    None,
                    Method::Post,
                    "https://address.test/managed".into(),
                    None,
                    Box::new(Recipe { claim }),
                    (),
                )
                .unwrap();
            }
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn identity() -> Identity {
    Identity::new(KeyRef("credential".into()), Some(KeyRef("signing".into())))
}
fn engine(domain: Option<&'static NonceDomain>, slots: AddressSetupSlots) -> Engine<Probe> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        managed: false,
        domain,
        slots,
        observed: false,
    });
    e.set_identities(Identities {
        primary: Some(identity()),
        secondary: Some(identity()),
    })
    .unwrap();
    e
}
fn external() -> Engine<Probe> {
    let mut e = engine(Some(&HYPERLIQUID_AGENT), AddressSetupSlots::Both);
    e.set_address_setup_host_support(SetupHostSupport::OwnedPublicAddress1)
        .unwrap();
    e
}
fn task(e: &mut Engine<Probe>, slot: CredentialSlot) -> (RequestId, SignRequest, TimerId) {
    let actions: Vec<_> = e.drain_actions().collect();
    let mut timer = None;
    for action in actions {
        match action {
            Action::SetTimer { timer: id, at_ms } => {
                assert_eq!(at_ms, 20_100.);
                timer = Some(id);
            }
            Action::Sign {
                attempt,
                work,
                timeout_ms,
            } if matches!(e.sign_meta(attempt).unwrap().kind, SignKind::Setup { slot: own, .. } if own == slot) =>
            {
                assert_eq!(timeout_ms, 20_000);
                return (attempt, work, timer.unwrap());
            }
            _ => {}
        }
    }
    panic!("no actual setup task");
}
fn result(work: &SignRequest, length: usize) -> SignResult {
    SignResult::new(vec![vec![19; length]], work.receipt)
}
fn token(e: &mut Engine<Probe>) -> Arc<SetupRetirement> {
    e.drain_actions()
        .find_map(|a| match a {
            Action::RetireAddressSetup { token, .. } => Some(token),
            _ => None,
        })
        .expect("actual retirement token")
}
fn mint(
    e: &Engine<Probe>,
) -> Result<safe_cex_engine::nonce::NonceClaim, safe_cex_engine::nonce::NonceSelectionError> {
    let ids = e.identities();
    Slots::new(
        ids.primary.clone(),
        ids.secondary.clone(),
        SlotPolicy::default(),
    )
    .select_with_nonce(CredentialRole::Mutation, &HYPERLIQUID_AGENT)
    .map(|x| x.2)
}
#[test]
fn no_support_allocates_no_signing_or_timer_identity() {
    let mut e = engine(Some(&HYPERLIQUID_AGENT), AddressSetupSlots::Both);
    assert_eq!(
        e.initialize_address_nonce(100.).unwrap(),
        AddressSetupProgress::Ready
    );
    assert!(e.drain_actions().next().is_none());
    assert_eq!(
        e.identities()
            .primary
            .as_ref()
            .unwrap()
            .verified_signing_address(),
        Err(SignError::Unsupported)
    );
    let state = Arc::new(Mutex::new(State::default()));
    e.set_signer(Some(Arc::new(Provider(state)))).unwrap();
    e.reinitialize_address_nonce(101.).unwrap();
    assert!(
        e.sign_meta(RequestId(1)).is_some(),
        "unsupported setup spent no signing ID"
    );
}
#[test]
fn exact_live_setup_metadata_and_retired_none() {
    let mut e = external();
    e.initialize_address_nonce(100.).unwrap();
    let (id, work, _) = task(&mut e, CredentialSlot::Primary);
    assert_eq!(work.receipt, SignReceipt::default());
    assert_eq!(work.operations.len(), 1);
    let meta = e.sign_meta(id).unwrap();
    assert_eq!(meta.task, id);
    assert_eq!(meta.deadline_ms, 20_100.);
    assert_eq!(
        meta.kind,
        SignKind::Setup {
            generation: 1,
            operation: SetupOperation::PublicAddress,
            slot: CredentialSlot::Primary
        }
    );
    e.handle(
        101.,
        Event::Signed {
            attempt: id,
            result: Ok(result(&work, 20)),
        },
    );
    assert_eq!(e.sign_meta(id), None);
    assert!(
        e.identities()
            .primary
            .as_ref()
            .unwrap()
            .verified_signing_address()
            .is_err()
    );
    let t = token(&mut e);
    assert!(e.address_setup_retired(t, 102.));
    assert_eq!(
        e.identities()
            .primary
            .as_ref()
            .unwrap()
            .verified_signing_address(),
        Ok([19; 20])
    );
}
#[test]
fn clones_foreign_tokens_and_wire_handles_are_one_shot() {
    let mut e = external();
    e.initialize_address_nonce(100.).unwrap();
    let (id, work, _) = task(&mut e, CredentialSlot::Primary);
    e.handle(
        101.,
        Event::Signed {
            attempt: id,
            result: Ok(result(&work, 20)),
        },
    );
    let t = token(&mut e);
    let wire = t.to_wire();
    let mut other = external();
    other.initialize_address_nonce(100.).unwrap();
    assert!(!other.address_setup_retired(t.clone(), 102.));
    assert!(!other.address_setup_retired_wire(&wire, 102.));
    let a = Action::RetireAddressSetup {
        task: id,
        token: t.clone(),
    };
    assert_eq!(a, a.clone());
    assert!(e.address_setup_retired(t.clone(), 102.));
    assert!(!e.address_setup_retired(t, 103.));
    assert!(!e.address_setup_retired_wire(&wire, 103.));
}
#[test]
fn malformed_result_does_not_install_and_public_start_survives() {
    for length in [0, 19, 21, 64] {
        let mut e = external();
        e.initialize_address_nonce(100.).unwrap();
        let (id, work, _) = task(&mut e, CredentialSlot::Primary);
        e.handle(
            101.,
            Event::Signed {
                attempt: id,
                result: Ok(result(&work, length)),
            },
        );
        let t = token(&mut e);
        assert!(e.address_setup_retired(t, 102.));
        assert_eq!(
            e.identities()
                .primary
                .as_ref()
                .unwrap()
                .verified_signing_address(),
            Err(SignError::InvalidResult)
        );
        e.start(103.);
        assert!(!e.venue().observed);
    }
}
#[test]
fn early_timer_rearms_at_original_bound_equality_expires_and_late_result_is_inert() {
    let mut e = external();
    e.initialize_address_nonce(100.).unwrap();
    let (id, work, timer) = task(&mut e, CredentialSlot::Primary);
    e.handle(101., Event::TimerFired { timer });
    let next = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => {
                assert_eq!(at_ms, 20_100.);
                Some(timer)
            }
            _ => None,
        })
        .unwrap();
    assert_ne!(next, timer);
    e.handle(20_100., Event::TimerFired { timer: next });
    let t = token(&mut e);
    assert_eq!(e.sign_meta(id), None);
    e.handle(
        20_101.,
        Event::Signed {
            attempt: id,
            result: Ok(result(&work, 20)),
        },
    );
    assert!(e.address_setup_retired(t, 20_102.));
    assert!(mint(&e).is_err());
}
#[test]
fn nonfinite_initialization_and_delivery_are_inert() {
    let mut e = external();
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.] {
        assert!(e.initialize_address_nonce(now).is_err());
        assert!(e.drain_actions().next().is_none());
    }
    e.initialize_address_nonce(100.).unwrap();
    let (id, work, _) = task(&mut e, CredentialSlot::Primary);
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        e.handle(
            now,
            Event::Signed {
                attempt: id,
                result: Ok(result(&work, 20)),
            },
        );
        assert!(e.drain_actions().next().is_none());
        assert!(e.sign_meta(id).is_some());
    }
    e.handle(
        101.,
        Event::Signed {
            attempt: id,
            result: Ok(result(&work, 20)),
        },
    );
    let t = token(&mut e);
    assert!(e.address_setup_retired(t, 102.));
}
#[derive(Default)]
struct State {
    result: bool,
    joined: bool,
    starts: usize,
    cancels: usize,
}
struct Provider(Arc<Mutex<State>>);
struct Execution(Arc<Mutex<State>>);
impl Signer for Provider {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::ready(Err(SignError::Unsupported)))
    }
    fn owned_public_address_capability(&self) -> Capability {
        Capability::Supported
    }
    fn create_public_address_setup(
        &self,
        work: SignRequest,
    ) -> Option<Box<dyn OwnedSetupExecution>> {
        if !matches!(work.operations.as_slice(),[Operation::PublicAddress{key}] if key.0=="signing")
            || work.receipt != Default::default()
        {
            return None;
        }
        Some(Box::new(Execution(self.0.clone())))
    }
}
impl OwnedSetupExecution for Execution {
    fn start(&mut self) -> Result<(), SignError> {
        self.0.lock().unwrap().starts += 1;
        Ok(())
    }
    fn cancel(&mut self) -> Result<(), SignError> {
        self.0.lock().unwrap().cancels += 1;
        Ok(())
    }
    fn poll_result(&mut self, _: &mut TaskContext<'_>) -> Poll<Result<SignResult, SignError>> {
        if self.0.lock().unwrap().result {
            Poll::Ready(Ok(SignResult::new(vec![vec![19; 20]], Default::default())))
        } else {
            Poll::Pending
        }
    }
    fn poll_joined(&mut self, _: &mut TaskContext<'_>) -> Poll<Result<(), SignError>> {
        if self.0.lock().unwrap().joined {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}
fn owned(domain: Option<&'static NonceDomain>) -> (Engine<Probe>, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(State::default()));
    let mut e = engine(domain, AddressSetupSlots::Both);
    e.set_signer(Some(Arc::new(Provider(state.clone()))))
        .unwrap();
    (e, state)
}
fn poll(e: &mut Engine<Probe>, now: f64) {
    e.poll_address_setup(now, &mut TaskContext::from_waker(Waker::noop()));
}
#[test]
fn result_and_join_are_independent_and_claim_free_setup_needs_no_domain() {
    for result_first in [true, false] {
        let (mut e, state) = owned(None);
        e.initialize_address_nonce(100.).unwrap();
        {
            let mut s = state.lock().unwrap();
            s.result = result_first;
            s.joined = !result_first;
        }
        poll(&mut e, 101.);
        assert!(
            e.identities()
                .primary
                .as_ref()
                .unwrap()
                .verified_signing_address()
                .is_err()
        );
        assert!(!e.address_nonce_setup_ready());
        {
            let mut s = state.lock().unwrap();
            s.result = true;
            s.joined = true;
        }
        poll(&mut e, 102.);
        assert_eq!(
            e.identities()
                .primary
                .as_ref()
                .unwrap()
                .verified_signing_address(),
            Ok([19; 20])
        );
        assert!(mint(&e).is_err());
        e.start(103.);
        assert!(e.venue().observed);
        assert!(e.reinitialize_address_nonce(104.).is_err());
    }
}
#[test]
fn abort_ignoring_owner_blocks_reinitialize_until_actual_join() {
    let (mut e, state) = owned(Some(&HYPERLIQUID_AGENT));
    e.initialize_address_nonce(100.).unwrap();
    assert_eq!(state.lock().unwrap().starts, 2);
    assert_eq!(
        e.reinitialize_address_nonce(101.).unwrap(),
        AddressSetupProgress::Retiring
    );
    assert_eq!(
        e.reinitialize_address_nonce(102.).unwrap(),
        AddressSetupProgress::Retiring
    );
    assert_eq!(state.lock().unwrap().starts, 2);
    state.lock().unwrap().result = true;
    poll(&mut e, 103.);
    assert_eq!(state.lock().unwrap().starts, 2);
    state.lock().unwrap().joined = true;
    poll(&mut e, 104.);
    assert_eq!(state.lock().unwrap().starts, 4);
    poll(&mut e, 105.);
    assert!(mint(&e).is_ok());
}
#[test]
fn actual_cleanup_driver_outlives_engine_and_does_not_confuse_result_with_join() {
    use std::future::Future;
    let (mut e, state) = owned(Some(&HYPERLIQUID_AGENT));
    e.initialize_address_nonce(100.).unwrap();
    let cleanup = e.take_address_setup_cleanup();
    drop(e);
    let mut cleanup = Box::pin(cleanup);
    let mut cx = TaskContext::from_waker(Waker::noop());
    assert!(cleanup.as_mut().poll(&mut cx).is_pending());
    state.lock().unwrap().result = true;
    assert!(cleanup.as_mut().poll(&mut cx).is_pending());
    state.lock().unwrap().joined = true;
    assert_eq!(cleanup.as_mut().poll(&mut cx), Poll::Ready(Ok(())));
}
#[test]
fn legit_alias_has_own_slot_binding_copy_is_refused_and_foreign_identity_cannot_replace_it() {
    let (mut e, state) = owned(Some(&HYPERLIQUID_AGENT));
    {
        let mut s = state.lock().unwrap();
        s.result = true;
        s.joined = true;
    }
    e.initialize_address_nonce(100.).unwrap();
    let ids = e.identities().clone();
    let own = Slots::new(
        ids.primary.clone(),
        ids.secondary.clone(),
        SlotPolicy::default(),
    );
    assert!(
        own.select_with_nonce(CredentialRole::Read { scope: None }, &HYPERLIQUID_AGENT)
            .is_ok()
    );
    let forged = Slots::new(
        ids.primary.clone(),
        ids.primary.clone(),
        SlotPolicy::default(),
    );
    assert!(
        forged
            .select_with_nonce(CredentialRole::Read { scope: None }, &HYPERLIQUID_AGENT)
            .is_err()
    );
    let mut other = engine(Some(&HYPERLIQUID_AGENT), AddressSetupSlots::Both);
    other.set_identities(ids).unwrap();
    // Installed identities must not retain a different engine's proof.
    other.start(101.);
    assert!(!other.venue().observed);
}

struct Recipe {
    claim: safe_cex_engine::nonce::NonceClaim,
}
impl SignStep for Recipe {
    fn nonce_claim(&self) -> Option<&safe_cex_engine::nonce::NonceClaim> {
        Some(&self.claim)
    }
    fn prepare(
        &mut self,
        action: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let nonce = cx
            .reserve_nonce(&self.claim)
            .map_err(|_| SignError::Unavailable)?;
        let Action::Http(packet) = action else {
            return Err(SignError::InvalidRequest);
        };
        packet.body = Some(nonce.to_string());
        Ok(SignRequest {
            receipt: SignReceipt {
                nonce: Some(nonce),
                ..Default::default()
            },
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::Secp256k1Digest,
                key: KeyRef("signing".into()),
                payload: vec![PayloadPart::Bytes {
                    bytes: [vec![0; 24], nonce.to_be_bytes().to_vec()].concat(),
                }],
            }],
        })
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        10_000.
    }
}
#[test]
fn genuine_address_claim_prepares_binds_and_releases_exact_nonce_with_calibrated_aster_floor() {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    for domain in [&HYPERLIQUID_AGENT, &safe_cex_engine::nonce::ASTERDEX_V3] {
        let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
            managed: true,
            domain: Some(domain),
            slots: AddressSetupSlots::Primary,
            observed: false,
        });
        e.set_identities(Identities {
            primary: Some(identity()),
            secondary: None,
        })
        .unwrap();
        e.set_address_setup_host_support(SetupHostSupport::OwnedPublicAddress1)
            .unwrap();
        e.initialize_address_nonce(100.).unwrap();
        let (id, work, _) = task(&mut e, CredentialSlot::Primary);
        e.handle(
            101.,
            Event::Signed {
                attempt: id,
                result: Ok(result(&work, 20)),
            },
        );
        let t = token(&mut e);
        assert!(e.address_setup_retired(t, 102.));
        e.start(200.);
        let preparations: Vec<_> = e
            .drain_actions()
            .filter_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(preparations.len(), 2);
        let mut nonces = Vec::new();
        for preparation in preparations {
            e.http_prepared(
                201.,
                preparation,
                Prepared {
                    id: preparation.attempt().request.0,
                    expires_at: None,
                },
            );
            let sign = e
                .drain_actions()
                .find_map(|a| match a {
                    Action::Sign { attempt, work, .. } => Some((attempt, work)),
                    _ => None,
                })
                .expect("real prepared provider work");
            let nonce = sign.1.receipt.nonce.unwrap();
            nonces.push(nonce);
            if *domain == safe_cex_engine::nonce::ASTERDEX_V3 {
                assert!(nonce >= 9_001_000);
            } else {
                assert!(nonce >= 201);
            }
            e.handle(
                202.,
                Event::Signed {
                    attempt: sign.0,
                    result: Ok(SignResult::new(vec![vec![0; 65]], sign.1.receipt)),
                },
            );
            let permit = e
                .drain_actions()
                .find_map(|a| match a {
                    Action::ManagedHttp(a) => match *a {
                        HttpAction::Bind {
                            permit, request, ..
                        } => {
                            assert_eq!(request.body, Some(nonce.to_string()));
                            Some(permit)
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .expect("real bound address packet");
            assert!(matches!(e.claim_http(203., permit), Ok(Claim::Released(_))));
            assert!(e.claim_http(203., permit).is_err());
        }
        if *domain == safe_cex_engine::nonce::ASTERDEX_V3 {
            assert_eq!(nonces, [9_001_000, 9_001_001]);
        } else {
            assert_eq!(nonces, [201, 202]);
        }
    }
}

#[test]
fn prestart_support_can_be_installed_after_refusal_without_allocating_old_work() {
    let mut e = engine(Some(&HYPERLIQUID_AGENT), AddressSetupSlots::Primary);
    assert_eq!(
        e.initialize_address_nonce(100.).unwrap(),
        AddressSetupProgress::Ready
    );
    assert!(e.drain_actions().next().is_none());
    e.set_address_setup_host_support(SetupHostSupport::OwnedPublicAddress1)
        .unwrap();
    assert_eq!(
        e.reinitialize_address_nonce(100.).unwrap(),
        AddressSetupProgress::Pending
    );
    let (id, work, _) = task(&mut e, CredentialSlot::Primary);
    assert_eq!(id, RequestId(1));
    e.handle(
        101.,
        Event::Signed {
            attempt: id,
            result: Ok(result(&work, 20)),
        },
    );
    let t = token(&mut e);
    assert!(e.address_setup_retired(t, 102.));
    e.start(103.);
    assert!(e.venue().observed);
    assert!(
        e.set_address_setup_host_support(SetupHostSupport::Unavailable)
            .is_err()
    );
}
#[test]
fn nonfinite_timer_observation_keeps_immutable_timeout_armed() {
    let mut e = external();
    e.initialize_address_nonce(100.).unwrap();
    let (id, _, mut timer) = task(&mut e, CredentialSlot::Primary);
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        e.handle(now, Event::TimerFired { timer });
        timer = e
            .drain_actions()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => {
                    assert_eq!(at_ms, 20_100.);
                    Some(timer)
                }
                _ => None,
            })
            .expect("immutable bound remains armed");
        assert!(e.sign_meta(id).is_some());
    }
    e.handle(20_100., Event::TimerFired { timer });
    assert_eq!(e.sign_meta(id), None);
    assert!(mint(&e).is_err());
}
