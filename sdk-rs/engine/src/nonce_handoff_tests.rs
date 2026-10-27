use super::*;
use crate::book::Book;
use crate::credential_selection::{CredentialRole, SlotPolicy, Slots};
use crate::nonce::{KRAKEN, NonceClaim};
use crate::signing::{Identities, KeyRef, SignError, SignFuture, SignRequest, Signer};
use crate::{Command, Cx, Response, SocketId, TimerId};
use std::sync::atomic::AtomicUsize;

struct Probe;
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
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
struct Keys {
    keys: std::collections::HashMap<KeyRef, Vec<u8>>,
}
fn material(label: &str, slot: &str) -> Vec<u8> {
    format!("fixture decoded key for {label}/{slot}").into_bytes()
}
impl Keys {
    fn new(label: &str) -> Self {
        Self {
            keys: ["p", "s"]
                .into_iter()
                .map(|slot| (KeyRef(format!("{label}/{slot}/key")), material(label, slot)))
                .collect(),
        }
    }
}
impl Signer for Keys {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(
        &self,
        request: &SignRequest,
    ) -> Option<Result<crate::signing::SignResult, SignError>> {
        let valid = request.validate().is_ok()
            && request.operations.len() == 1
            && matches!(&request.operations[0], crate::signing::Operation::Sign { algorithm: Algorithm::HmacSha512, key, .. } if self.keys.contains_key(key));
        Some(if valid {
            Ok(crate::signing::SignResult::new(
                vec![vec![1; 64]],
                request.receipt,
            ))
        } else {
            Err(SignError::InvalidRequest)
        })
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        let result = self.sign_ready(&request).unwrap();
        Box::pin(async move { result })
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        if algorithm == Algorithm::HmacSha512 && self.keys.contains_key(key) {
            Capability::Supported
        } else {
            Capability::Unsupported
        }
    }
    fn legacy_nonce_association(
        &self,
        request: LegacyNonceRequest,
    ) -> Result<crate::nonce::handoff::LegacyNonceProof, SignError> {
        let bytes = self.keys.get(request.key()).ok_or(SignError::MissingKey)?;
        request.bind_key_bytes(bytes)
    }
}
fn engine(label: &str) -> Engine<Probe> {
    let mut e = crate::test_support::seeded_engine(Probe);
    let identity = |suffix: &str| {
        Identity::new(
            KeyRef(format!("{label}/{suffix}/credential")),
            Some(KeyRef(format!("{label}/{suffix}/key"))),
        )
        .with_stable_id(Some(format!("nonce-handoff/{label}")))
    };
    e.set_identities(Identities {
        primary: Some(identity("p")),
        secondary: Some(identity("s")),
    })
    .unwrap();
    e.set_signer(Some(Arc::new(Keys::new(label)))).unwrap();
    e
}
fn associate(e: &mut Engine<Probe>, slot: CredentialSlot) -> LegacyNonceAssociation {
    e.associate_legacy_nonce(slot, &KRAKEN, Algorithm::HmacSha512)
        .unwrap()
}
fn claim(e: &Engine<Probe>) -> NonceClaim {
    Slots::new(e.identities().primary.clone(), None, SlotPolicy::default())
        .select_with_nonce(CredentialRole::Mutation, &KRAKEN)
        .unwrap()
        .2
}
struct Held {
    joined: Arc<AtomicBool>,
    cleanup: Arc<AtomicUsize>,
}
impl Future for Held {
    type Output = Result<(), ()>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.joined.load(Ordering::Acquire) {
            self.cleanup.fetch_add(1, Ordering::AcqRel);
            Poll::Ready(Err(()))
        } else {
            Poll::Pending
        }
    }
}
struct HeldOwner {
    work: LegacyNonceWork<Held>,
    issuer: LegacyNonceIssuer,
    ready: Arc<AtomicBool>,
    cleaned: Arc<AtomicUsize>,
}
fn held(owner: NonceOwner) -> HeldOwner {
    let ready = Arc::new(AtomicBool::new(false));
    let cleaned = Arc::new(AtomicUsize::new(0));
    let mut issuer = None;
    let work = owner.run(|i| {
        issuer = Some(i);
        Held {
            joined: ready.clone(),
            cleanup: cleaned.clone(),
        }
    });
    HeldOwner {
        work,
        issuer: issuer.unwrap(),
        ready,
        cleaned,
    }
}
fn poll<F: Future>(work: &mut LegacyNonceWork<F>) -> Poll<F::Output> {
    Pin::new(work).poll(&mut Context::from_waker(std::task::Waker::noop()))
}
fn join(owner: &mut HeldOwner) {
    assert!(poll(&mut owner.work).is_pending());
    assert_eq!(owner.cleaned.load(Ordering::Acquire), 0);
    owner.ready.store(true, Ordering::Release);
    assert_eq!(poll(&mut owner.work), Poll::Ready(Err(())));
    assert_eq!(owner.cleaned.load(Ordering::Acquire), 1);
}
#[derive(Debug, PartialEq, Eq)]
struct Snapshot(Vec<(u64, Option<u64>, Phase, usize, bool)>);
fn snapshot(a: &[&LegacyNonceAssociation]) -> Snapshot {
    let store = lock().unwrap();
    Snapshot(
        std::iter::once(a[0].target)
            .chain(a.iter().map(|a| a.source))
            .map(|k| {
                let s = &store[&k];
                let t = s.tracking.as_ref().unwrap();
                (
                    s.reserved,
                    s.released,
                    t.phase,
                    Arc::as_ptr(&t.epoch) as usize,
                    t.untracked,
                )
            })
            .collect(),
    )
}

#[test]
fn in_flight_legacy_and_target_owners_both_block_until_original_cleanup_joins() {
    let mut e = engine("both-owners");
    let mut target = held(
        e.register_nonce_owner(CredentialSlot::Primary, &KRAKEN)
            .unwrap(),
    );
    let old = claim(&e).authority();
    let reserved = old.reserve(0., 0.).unwrap();
    assert!(reserved.commit_release(|| {}).unwrap());
    let a = associate(&mut e, CredentialSlot::Primary);
    let mut source = held(e.register_legacy_nonce_owner(&a).unwrap());
    let permit = source.issuer.reserve(7000).unwrap();
    let mut takeover = e.fence_nonce_handoff(&[&a]).unwrap();
    let fenced = snapshot(&[&a]);
    assert!(source.issuer.reserve(0).is_err());
    assert!(
        crate::nonce::reserve_kraken_legacy(a.source.1, 9000).is_err(),
        "stale raw signer cannot reserve"
    );
    assert!(!old.matches_identity(e.identities().primary.as_ref().unwrap()));
    assert!(
        !reserved
            .commit_release(|| panic!("fenced target release"))
            .unwrap()
    );
    assert!(old.reserve(0., 0.).is_err());
    assert!(e.commit_nonce_handoff(&mut takeover).is_err());
    assert_eq!(snapshot(&[&a]), fenced);
    permit.released().unwrap();
    join(&mut source);
    let before_target_join = snapshot(&[&a]);
    assert!(
        e.commit_nonce_handoff(&mut takeover).is_err(),
        "target maximum is not target join"
    );
    assert_eq!(snapshot(&[&a]), before_target_join);
    join(&mut target);
    e.commit_nonce_handoff(&mut takeover).unwrap();
    assert!(e.commit_nonce_handoff(&mut takeover).is_err());
    let mut next = held(
        e.register_nonce_owner(CredentialSlot::Primary, &KRAKEN)
            .unwrap(),
    );
    let new = claim(&e).authority().reserve(0., 0.).unwrap();
    assert_eq!(new.nonce, 7001);
    assert!(new.commit_release(|| {}).unwrap());
    assert!(source.issuer.reserve(0).is_err());
    join(&mut next);
}

#[test]
fn concurrent_reserve_and_release_import_both_source_maxima_and_higher_target() {
    let mut e = engine("concurrent-maxima");
    let mut target = held(
        e.register_nonce_owner(CredentialSlot::Primary, &KRAKEN)
            .unwrap(),
    );
    let authority = claim(&e).authority();
    let target_nonce = authority.reserve(50., 50.).unwrap();
    assert!(target_nonce.commit_release(|| {}).unwrap());
    let a = associate(&mut e, CredentialSlot::Primary);
    let b = associate(&mut e, CredentialSlot::Secondary);
    let mut one = held(e.register_legacy_nonce_owner(&a).unwrap());
    let mut two = held(e.register_legacy_nonce_owner(&b).unwrap());
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let mut takeover = std::thread::scope(|scope| {
        let worker = |issuer: &LegacyNonceIssuer| {
            let mut permits = Vec::new();
            for _ in 0..128 {
                permits.push(issuer.reserve(0).unwrap());
            }
            barrier.wait();
            for p in permits.into_iter().rev() {
                p.released().unwrap();
            }
        };
        let issuer1 = &one.issuer;
        let issuer2 = &two.issuer;
        let h1 = scope.spawn(move || worker(issuer1));
        let h2 = scope.spawn(move || worker(issuer2));
        barrier.wait();
        let mut takeover = e.fence_nonce_handoff(&[&a, &b]).unwrap();
        assert!(e.commit_nonce_handoff(&mut takeover).is_err());
        h1.join().unwrap();
        h2.join().unwrap();
        takeover
    });
    join(&mut one);
    join(&mut two);
    join(&mut target);
    e.commit_nonce_handoff(&mut takeover).unwrap();
    let mut next = held(
        e.register_nonce_owner(CredentialSlot::Primary, &KRAKEN)
            .unwrap(),
    );
    let new = claim(&e).authority().reserve(0., 0.).unwrap();
    assert_eq!(
        new.nonce, 50001,
        "higher existing target reserved AND released floors survive"
    );
    let store = lock().unwrap();
    assert_eq!(store[&a.target].released, Some(50000));
    assert_eq!(store[&a.source].released, Some(128));
    assert_eq!(store[&b.source].released, Some(128));
    drop(store);
    assert!(new.commit_release(|| {}).unwrap());
    join(&mut next);
}

#[test]
fn forged_foreign_stale_and_partial_source_sets_change_no_floor_or_phase() {
    let mut e = engine("failed-fence");
    let a = associate(&mut e, CredentialSlot::Primary);
    let b = associate(&mut e, CredentialSlot::Secondary);
    let mut one = held(e.register_legacy_nonce_owner(&a).unwrap());
    let mut two = held(e.register_legacy_nonce_owner(&b).unwrap());
    let before = snapshot(&[&a, &b]);
    assert!(e.fence_nonce_handoff(&[&a]).is_err());
    assert_eq!(snapshot(&[&a, &b]), before);
    let mut foreign = engine("foreign-engine");
    assert!(foreign.fence_nonce_handoff(&[&a, &b]).is_err());
    assert_eq!(snapshot(&[&a, &b]), before);
    let forged = LegacyNonceAssociation {
        engine: a.engine.clone(),
        slot: a.slot,
        identity: a
            .identity
            .clone()
            .with_stable_id(Some("wrong-subject".into())),
        domain: a.domain,
        source: a.source,
        target: a.target,
    };
    assert!(e.fence_nonce_handoff(&[&forged, &b]).is_err());
    assert_eq!(snapshot(&[&a, &b]), before);
    let mut takeover = e.fence_nonce_handoff(&[&a, &b]).unwrap();
    join(&mut one);
    join(&mut two);
    let fenced = snapshot(&[&a, &b]);
    e.set_signer(Some(Arc::new(Keys::new("failed-fence"))))
        .unwrap();
    assert!(e.commit_nonce_handoff(&mut takeover).is_err());
    assert_eq!(snapshot(&[&a, &b]), fenced);
}

#[test]
fn exhaustion_and_incomplete_owners_leave_entire_transaction_unchanged() {
    let mut e = engine("exhausted");
    let a = associate(&mut e, CredentialSlot::Primary);
    let mut source = held(e.register_legacy_nonce_owner(&a).unwrap());
    source.issuer.reserve(u64::MAX).unwrap();
    let mut takeover = e.fence_nonce_handoff(&[&a]).unwrap();
    let before = snapshot(&[&a]);
    assert!(e.commit_nonce_handoff(&mut takeover).is_err());
    assert_eq!(snapshot(&[&a]), before);
    join(&mut source);
    assert_eq!(
        e.commit_nonce_handoff(&mut takeover),
        Err(NonceError::Exhausted)
    );
    assert_eq!(snapshot(&[&a]), before);
}

#[test]
fn untracked_raw_history_or_target_history_cannot_be_certified_by_new_owners() {
    let mut e = engine("raw-history");
    use sha2::{Digest, Sha256};
    let digest: [u8; 32] = Sha256::digest(material("raw-history", "p")).into();
    crate::nonce::reserve_kraken_legacy(digest, 77).unwrap();
    assert!(
        e.associate_legacy_nonce(CredentialSlot::Primary, &KRAKEN, Algorithm::HmacSha512)
            .is_err()
    );
    assert_eq!(lock().unwrap()[&(KRAKEN.name, digest)].reserved, 77);
    let mut e = engine("target-history");
    let n = claim(&e).authority().reserve(1., 1.).unwrap();
    assert!(n.commit_release(|| {}).unwrap());
    assert!(
        e.register_nonce_owner(CredentialSlot::Primary, &KRAKEN)
            .is_err()
    );
    assert!(
        e.associate_legacy_nonce(CredentialSlot::Primary, &KRAKEN, Algorithm::HmacSha512)
            .is_err()
    );
}

#[test]
fn dropping_or_never_polling_original_work_does_not_acknowledge_join() {
    for polled in [false, true] {
        let mut e = engine(if polled {
            "drop-polled"
        } else {
            "drop-unpolled"
        });
        let a = associate(&mut e, CredentialSlot::Primary);
        let mut original = held(e.register_legacy_nonce_owner(&a).unwrap());
        original.issuer.reserve(10).unwrap();
        let mut takeover = e.fence_nonce_handoff(&[&a]).unwrap();
        if polled {
            assert!(poll(&mut original.work).is_pending());
        }
        drop(original);
        let before = snapshot(&[&a]);
        assert!(e.commit_nonce_handoff(&mut takeover).is_err());
        assert_eq!(snapshot(&[&a]), before);
    }
}

struct ManagedProbe;
impl Venue for ManagedProbe {
    type Pending = crate::request_context::Context;
    fn dispatch_nonce() -> bool {
        true
    }
    fn http_release() -> crate::HttpRelease<Self> {
        crate::HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
    }
    fn fence_uncertain_placements(&self) -> bool {
        true
    }
    fn start(&mut self, _: &mut Cx<'_, Self::Pending>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId, _: &str) {}
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        use crate::{classify::*, request_context::Operation};
        if pending.operation() == Operation::Read {
            #[cfg(feature = "order-locks")]
            if response.body == "clean" {
                cx.orders_reconciled(None);
            }
            cx.finish_http_context(&pending).unwrap();
        } else {
            const TABLE: CodeTable = CodeTable {
                rows: &[Row {
                    key: Key::Text("nonce"),
                    verdict: Verdict::NonceRefused,
                    only: None,
                }],
                success: &[],
                unlisted: Verdict::Unknown(None),
                status: &[],
            };
            let native = Native {
                code: NativeCode::Text("nonce"),
                message: "invalid nonce",
            };
            let fact = cx
                .classify_write(&pending, &TABLE, Some(&native), "nonce")
                .unwrap();
            let error = fact.error().clone();
            assert!(
                cx.http_write_fact(
                    &pending,
                    WriteFact::Failed {
                        fact,
                        known_effects: false
                    },
                    None
                )
                .unwrap()
                .is_none()
            );
            cx.fail_context(&pending, error).unwrap();
        }
    }
    fn capture_http_reconciliation(
        &self,
        _: &Self::Pending,
    ) -> Result<crate::request_context::ReconciliationData, crate::request_context::Error> {
        crate::request_context::ReconciliationData::new("nonce-fixture".into())
    }
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    ) {
        if pending.operation() == crate::request_context::Operation::Mutation {
            cx.fail_context(&pending, error).unwrap();
        }
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Self::Pending>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command) {
        if let Command::PlaceOrders { command, .. } = command {
            queue(cx, crate::request_context::Scope::Command(command), true);
        }
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
struct Recipe {
    claim: NonceClaim,
    key: KeyRef,
}
impl crate::SignStep for Recipe {
    fn nonce_claim(&self) -> Option<&NonceClaim> {
        Some(&self.claim)
    }
    fn freshness_ms(&self) -> f64 {
        20_000.
    }
    fn prepare(
        &mut self,
        action: &mut crate::Action,
        context: &crate::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let nonce = context
            .reserve_nonce(&self.claim)
            .map_err(|_| SignError::InvalidRequest)?;
        let crate::Action::Http(http) = action else {
            return Err(SignError::InvalidRequest);
        };
        http.body = Some(format!("nonce={nonce}"));
        Ok(SignRequest {
            operations: vec![crate::signing::Operation::Sign {
                algorithm: Algorithm::HmacSha512,
                key: self.key.clone(),
                payload: vec![crate::signing::PayloadPart::Bytes {
                    bytes: http.body.as_ref().unwrap().as_bytes().to_vec(),
                }],
            }],
            receipt: crate::signing::SignReceipt {
                stamp_ms: Some(context.stamp_ms()),
                nonce: Some(nonce),
                ..Default::default()
            },
            context: Default::default(),
        })
    }
    fn apply(
        &mut self,
        action: &mut crate::Action,
        result: &crate::signing::SignResult,
    ) -> Result<(), SignError> {
        let crate::Action::Http(http) = action else {
            return Err(SignError::InvalidRequest);
        };
        http.body
            .as_mut()
            .ok_or(SignError::InvalidRequest)?
            .push_str(&format!("&signature={:02x}", result.values[0][0]));
        Ok(())
    }
}
fn queue(
    cx: &mut Cx<'_, crate::request_context::Context>,
    scope: crate::request_context::Scope,
    mutation: bool,
) {
    use crate::request_context::{Operation, Policy, RetryPolicy};
    let role = if mutation {
        CredentialRole::Mutation
    } else {
        CredentialRole::Read { scope: None }
    };
    let slots = Slots::new(
        cx.io.core.identities.primary.clone(),
        None,
        SlotPolicy::default(),
    );
    let (selection, identity, claim) = slots.select_with_nonce(role, &KRAKEN).unwrap();
    let context = cx
        .http_context(
            scope,
            Policy {
                operation: if mutation {
                    Operation::Mutation
                } else {
                    Operation::Read
                },
                credential: Some(selection.authority(identity)),
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        )
        .unwrap();
    cx.request_signed_managed(
        context.clone(),
        None,
        crate::Method::Post,
        "https://venue.test/nonce".into(),
        None,
        Box::new(Recipe {
            claim,
            key: identity.signing_ref().unwrap().clone(),
        }),
        context,
    )
    .unwrap();
}
fn managed_engine(label: &str) -> Engine<ManagedProbe> {
    let ids = engine(label).identities().clone();
    let mut e = crate::test_support::seeded_engine(ManagedProbe);
    e.set_signer(Some(Arc::new(Keys::new(label)))).unwrap();
    e.set_identities(ids).unwrap();
    e
}
fn queue_read(e: &mut Engine<ManagedProbe>, now: f64) {
    queue(
        &mut Cx {
            now,
            store: &mut e.store,
            io: &mut e.io,
        },
        crate::request_context::Scope::Background,
        false,
    );
}
fn prepare(
    e: &mut Engine<ManagedProbe>,
    now: f64,
) -> (
    crate::request_context::Preparation,
    crate::request_context::Permit,
    String,
) {
    use crate::{
        Action,
        request_context::{HttpAction, Prepared},
    };
    let token = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("real managed Prepare");
    e.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((token, permit, request.body.unwrap())),
                _ => None,
            },
            _ => None,
        })
        .expect("real signed Bind")
}
fn finish(
    e: &mut Engine<ManagedProbe>,
    token: crate::request_context::Preparation,
    now: f64,
    body: &str,
) {
    e.handle(
        now,
        crate::Event::HttpResponse {
            request: token.attempt().request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
#[test]
fn managed_prepare_bind_and_claim_cross_the_handoff_only_with_fresh_nonce_authority() {
    let mut e = managed_engine("managed-binding");
    let mut target = held(
        e.register_nonce_owner(CredentialSlot::Primary, &KRAKEN)
            .unwrap(),
    );
    let a = e
        .associate_legacy_nonce(CredentialSlot::Primary, &KRAKEN, Algorithm::HmacSha512)
        .unwrap();
    let mut source = held(e.register_legacy_nonce_owner(&a).unwrap());
    source.issuer.reserve(90_000).unwrap();
    queue_read(&mut e, 1.);
    let (_, old, _) = prepare(&mut e, 1.);
    let mut takeover = e.fence_nonce_handoff(&[&a]).unwrap();
    assert_eq!(
        e.claim_http(2., old),
        Err(crate::request_context::Error::Revoked),
        "a genuine old Bind is refused before release"
    );
    assert!(e.io.core.managed_http.is_empty());
    assert!(e.io.pending.is_empty());
    join(&mut target);
    join(&mut source);
    e.commit_nonce_handoff(&mut takeover).unwrap();
    let mut current = held(
        e.register_nonce_owner(CredentialSlot::Primary, &KRAKEN)
            .unwrap(),
    );
    queue_read(&mut e, 3.);
    let (token, permit, body) = prepare(&mut e, 3.);
    assert!(body.starts_with("nonce=90001&signature="), "{body}");
    assert!(matches!(
        e.claim_http(4., permit),
        Ok(crate::request_context::Claim::Released(_))
    ));
    assert!(
        e.claim_http(4., permit).is_err(),
        "duplicate physical Claim is inert"
    );
    finish(&mut e, token, 5., "ok");
    assert!(e.io.core.managed_http.is_empty());
    assert!(e.io.pending.is_empty());
    join(&mut current);
}

#[test]
fn a_late_original_provider_result_cannot_bind_across_the_fenced_epoch() {
    struct AsyncKeys(Keys);
    impl Signer for AsyncKeys {
        fn sign(&self, request: SignRequest) -> SignFuture {
            self.0.sign(request)
        }
        fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
            self.0.can_sign(key, algorithm)
        }
        fn legacy_nonce_association(
            &self,
            request: LegacyNonceRequest,
        ) -> Result<crate::nonce::handoff::LegacyNonceProof, SignError> {
            self.0.legacy_nonce_association(request)
        }
    }
    let label = "late-provider";
    let mut e = managed_engine(label);
    let provider = Arc::new(AsyncKeys(Keys::new(label)));
    e.set_signer(Some(provider.clone())).unwrap();
    let mut target = held(
        e.register_nonce_owner(CredentialSlot::Primary, &KRAKEN)
            .unwrap(),
    );
    let a = e
        .associate_legacy_nonce(CredentialSlot::Primary, &KRAKEN, Algorithm::HmacSha512)
        .unwrap();
    let mut source = held(e.register_legacy_nonce_owner(&a).unwrap());
    queue_read(&mut e, 1.);
    let token = e
        .drain_actions()
        .find_map(|action| match action {
            crate::Action::ManagedHttp(action) => match *action {
                crate::request_context::HttpAction::Prepare { preparation, .. } => {
                    Some(preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .expect("genuine managed preparation");
    e.http_prepared(
        1.,
        token,
        crate::request_context::Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let (attempt, request) = e
        .drain_actions()
        .find_map(|action| match action {
            crate::Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .expect("actual async provider request");
    let mut original = provider.sign(request);
    let mut takeover = e.fence_nonce_handoff(&[&a]).unwrap();
    let Poll::Ready(result) = original
        .as_mut()
        .poll(&mut Context::from_waker(std::task::Waker::noop()))
    else {
        panic!("fixture provider is ready when the host polls it");
    };
    assert!(result.is_ok());
    e.handle(2., crate::Event::Signed { attempt, result });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|action| matches!(action,
        crate::Action::Http(_) | crate::Action::Sign { .. } | crate::Action::SignedOutbound { .. }
    ) || matches!(action, crate::Action::ManagedHttp(action) if matches!(**action,
        crate::request_context::HttpAction::Bind { .. } | crate::request_context::HttpAction::Prepare { .. }
    ))), "late provider actions: {actions:?}");
    assert!(e.io.core.managed_http.is_empty());
    assert!(e.io.pending.is_empty());
    join(&mut target);
    join(&mut source);
    e.commit_nonce_handoff(&mut takeover).unwrap();
}

#[cfg(feature = "order-locks")]
#[test]
fn nonce_not_sent_terminal_releases_its_scope_without_clearing_another_uncertain_placement_or_stale_census()
 {
    use crate::{Action, Event, Notice, Outcome};
    let mut e = managed_engine("nonce-fence");
    let mut owner = held(
        e.register_nonce_owner(CredentialSlot::Primary, &KRAKEN)
            .unwrap(),
    );
    e.start(0.);
    let place = |command, symbol: &str| {
        Command::PlaceOrders { command, orders: vec![serde_json::from_value(serde_json::json!({"symbol":symbol,"type":"limit","side":"buy","price":100,"amount":1})).unwrap()] }
    };
    let released = |e: &mut Engine<ManagedProbe>, now| {
        let (token, permit, _) = prepare(e, now);
        assert!(matches!(
            e.claim_http(now, permit),
            Ok(crate::request_context::Claim::Released(_))
        ));
        token
    };
    e.handle(1., Event::Command(place(1, "A")));
    let one = released(&mut e, 1.);
    e.handle(2., Event::Command(place(2, "A")));
    let two = released(&mut e, 2.);
    e.handle(2.5, Event::Command(place(3, "A")));
    let nonce = released(&mut e, 2.5);
    e.handle(
        3.,
        Event::HttpFailed {
            request: one.attempt().request,
            error: "lost one",
        },
    );
    assert!(e.io.core.order_locks.uncertain("A"));
    queue_read(&mut e, 4.);
    let stale = released(&mut e, 4.);
    e.handle(
        5.,
        Event::HttpFailed {
            request: two.attempt().request,
            error: "lost two",
        },
    );
    finish(&mut e, nonce, 7., "nonce");
    assert_eq!(e.drain_actions().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 3, error }) if error.outcome == Some(Outcome::NotSent))).count(), 1);
    assert!(
        e.io.core.order_locks.uncertain("A"),
        "a co-observed NotSent on the same symbol cannot clear the older Unknown"
    );
    finish(&mut e, stale, 8., "clean");
    assert!(
        e.io.core.order_locks.uncertain("A"),
        "a census preceding the second dirty mark cannot clear the fence"
    );
    e.drain_actions().for_each(drop);
    e.handle(9., Event::Command(place(4, "A")));
    let refused: Vec<_> = e.drain_actions().collect();
    assert_eq!(refused.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 4, error }) if error.outcome == Some(Outcome::NotSent))).count(), 1);
    assert!(!refused.iter().any(|a| matches!(a, Action::ManagedHttp(_))));
    queue_read(&mut e, 10.);
    let fresh = released(&mut e, 10.);
    finish(&mut e, fresh, 11., "clean");
    assert!(!e.io.core.order_locks.uncertain("A"));
    e.handle(12., Event::Command(place(5, "A")));
    let five = released(&mut e, 12.);
    finish(&mut e, five, 13., "nonce");
    e.handle(14., Event::Command(place(6, "A")));
    let six = released(&mut e, 14.);
    finish(&mut e, six, 15., "nonce");
    assert!(e.io.core.managed_http.is_empty());
    assert!(e.io.pending.is_empty());
    join(&mut owner);
}

#[test]
fn reserve_racing_the_fence_is_either_imported_or_refused_never_reused() {
    let mut e = engine("reserve-race");
    let a = associate(&mut e, CredentialSlot::Primary);
    let mut owner = held(e.register_legacy_nonce_owner(&a).unwrap());
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let (mut takeover, maximum) = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let first = owner.issuer.reserve(100).unwrap();
            let mut last = first.nonce();
            first.released().unwrap();
            barrier.wait();
            for _ in 0..4096 {
                match owner.issuer.reserve(0) {
                    Ok(p) => {
                        last = p.nonce();
                        p.released().unwrap();
                    }
                    Err(NonceError::InvalidClaim) => break,
                    Err(error) => panic!("unexpected reservation error {error:?}"),
                }
            }
            last
        });
        barrier.wait();
        let takeover = e.fence_nonce_handoff(&[&a]).unwrap();
        (takeover, worker.join().unwrap())
    });
    join(&mut owner);
    e.commit_nonce_handoff(&mut takeover).unwrap();
    let mut target = held(
        e.register_nonce_owner(CredentialSlot::Primary, &KRAKEN)
            .unwrap(),
    );
    let n = claim(&e).authority().reserve(0., 0.).unwrap();
    assert_eq!(n.nonce, maximum + 1);
    join(&mut target);
}

#[test]
fn default_provider_refuses_and_a_replayed_proof_is_not_a_fresh_association() {
    struct DefaultProvider;
    impl Signer for DefaultProvider {
        fn sign(&self, _: SignRequest) -> SignFuture {
            Box::pin(async { Err(SignError::Unsupported) })
        }
        fn can_sign(&self, _: &KeyRef, _: Algorithm) -> Capability {
            Capability::Supported
        }
    }
    let mut e = engine("default-refusal");
    e.set_signer(Some(Arc::new(DefaultProvider))).unwrap();
    assert!(
        e.associate_legacy_nonce(CredentialSlot::Primary, &KRAKEN, Algorithm::HmacSha512)
            .is_err()
    );
    struct Replay {
        proof: std::sync::Mutex<Option<crate::nonce::handoff::LegacyNonceProof>>,
    }
    impl Signer for Replay {
        fn sign(&self, _: SignRequest) -> SignFuture {
            Box::pin(async { Err(SignError::Unsupported) })
        }
        fn can_sign(&self, _: &KeyRef, _: Algorithm) -> Capability {
            Capability::Supported
        }
        fn legacy_nonce_association(
            &self,
            request: LegacyNonceRequest,
        ) -> Result<crate::nonce::handoff::LegacyNonceProof, SignError> {
            let mut proof = self.proof.lock().unwrap();
            if let Some(proof) = proof.take() {
                return Ok(proof);
            }
            *proof = Some(request.bind_key_bytes(b"fixture decoded material").unwrap());
            Err(SignError::Unsupported)
        }
    }
    let mut e = engine("replayed-proof");
    e.set_signer(Some(Arc::new(Replay {
        proof: Default::default(),
    })))
    .unwrap();
    let target = key(&KRAKEN, e.identities().primary.as_ref().unwrap()).unwrap();
    for _ in 0..2 {
        assert!(
            e.associate_legacy_nonce(CredentialSlot::Primary, &KRAKEN, Algorithm::HmacSha512)
                .is_err()
        );
        assert!(!lock().unwrap().contains_key(&target));
    }
}

#[test]
fn a_new_untracked_raw_call_refuses_the_whole_fence_without_half_closing_other_sources() {
    let mut e = engine("raw-after-register");
    let a = associate(&mut e, CredentialSlot::Primary);
    let b = associate(&mut e, CredentialSlot::Secondary);
    let mut one = held(e.register_legacy_nonce_owner(&a).unwrap());
    let mut two = held(e.register_legacy_nonce_owner(&b).unwrap());
    one.issuer.reserve(50).unwrap();
    two.issuer.reserve(60).unwrap();
    crate::nonce::reserve_kraken_legacy(b.source.1, 70).unwrap();
    let before = snapshot(&[&a, &b]);
    assert!(e.fence_nonce_handoff(&[&a, &b]).is_err());
    assert_eq!(snapshot(&[&a, &b]), before);
    assert_eq!(
        one.issuer.reserve(0).unwrap().nonce(),
        51,
        "unrelated source was not half closed"
    );
    join(&mut one);
    join(&mut two);
}

#[test]
fn one_legacy_digest_cannot_be_reassigned_to_a_different_stable_subject() {
    let mut e = engine("no-reassignment");
    let a = associate(&mut e, CredentialSlot::Primary);
    let before = snapshot(&[&a]);
    let mut ids = e.identities().clone();
    ids.primary.as_mut().unwrap().stable_id = Some("different-subject".into());
    e.set_identities(ids).unwrap();
    assert!(
        e.associate_legacy_nonce(CredentialSlot::Primary, &KRAKEN, Algorithm::HmacSha512)
            .is_err()
    );
    assert_eq!(snapshot(&[&a]), before);
}

#[test]
fn inconsistent_store_transaction_changes_neither_source_set_nor_target() {
    let mut e = engine("invalid-transaction");
    let a = associate(&mut e, CredentialSlot::Primary);
    let b = associate(&mut e, CredentialSlot::Secondary);
    let mut one = held(e.register_legacy_nonce_owner(&a).unwrap());
    let mut two = held(e.register_legacy_nonce_owner(&b).unwrap());
    one.issuer.reserve(30).unwrap().released().unwrap();
    two.issuer.reserve(40).unwrap().released().unwrap();
    let mut takeover = e.fence_nonce_handoff(&[&a, &b]).unwrap();
    join(&mut one);
    join(&mut two);
    lock().unwrap().get_mut(&b.source).unwrap().released = Some(41);
    let before = snapshot(&[&a, &b]);
    assert_eq!(
        e.commit_nonce_handoff(&mut takeover),
        Err(NonceError::InvalidClaim)
    );
    assert_eq!(
        snapshot(&[&a, &b]),
        before,
        "not even the first valid source was committed or reopened"
    );
}
