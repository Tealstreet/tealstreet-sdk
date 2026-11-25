//! Actor-preserving signer handover, with a barrier around every owned sign job.
use super::*;
use crate::envelope::ModeTransitionRefusal;
use safe_cex_crypto::admission::{Family, PublicKey};
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, Identity, KeyRef, Operation, OwnedSetupExecution,
    PayloadPart, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use std::task::{Context, Poll};

type Aliases = HashMap<KeyRef, KeyRef>;

pub(crate) struct AccountSigner {
    state: Arc<Mutex<State>>,
    original: Option<safe_cex_engine::signing::Provider>,
    identities: Identities,
    link: Option<Arc<SignerLink>>,
    resident: Mutex<Option<Arc<ResidentCopy>>>,
    binding: Option<safe_cex_remote_signer::PublicBindingSource>,
}
// Engine handles retain dispatch access, never the account's erasure authority.
struct EngineSigner {
    state: Arc<Mutex<State>>,
    original: Option<safe_cex_engine::signing::Provider>,
}
struct State {
    provider: Option<safe_cex_engine::signing::Provider>,
    aliases: Option<Aliases>,
    active: usize,
}
struct Lease(Arc<Mutex<State>>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.lock().unwrap().active -= 1;
    }
}
#[cfg(test)]
#[path = "signing_lifetime_tests.rs"]
mod lifetime_tests;

impl AccountSigner {
    pub(super) fn provider(&self) -> safe_cex_engine::signing::Provider {
        Arc::new(EngineSigner {
            state: self.state.clone(),
            original: self.original.clone(),
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn new(
        provider: Option<safe_cex_engine::signing::Provider>,
        identities: Identities,
        link: Option<Arc<SignerLink>>,
        resident: Option<Arc<ResidentCopy>>,
    ) -> Arc<Self> {
        Self::new_bound(provider, identities, link, resident, None)
    }

    pub(super) fn new_bound(
        provider: Option<safe_cex_engine::signing::Provider>,
        identities: Identities,
        link: Option<Arc<SignerLink>>,
        resident: Option<Arc<ResidentCopy>>,
        binding: Option<safe_cex_remote_signer::PublicBindingSource>,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Arc::new(Mutex::new(State {
                provider: provider.clone(),
                aliases: None,
                active: 0,
            })),
            original: provider,
            identities,
            link,
            resident: Mutex::new(resident),
            binding,
        })
    }
    pub(super) fn resident(&self) -> Option<Arc<ResidentCopy>> {
        self.resident.lock().unwrap().clone()
    }
    #[cfg(test)]
    pub(crate) fn set_test_resident(&self, resident: Arc<ResidentCopy>) {
        *self.resident.lock().unwrap() = Some(resident);
    }

    pub(super) fn public_binding(
        &self,
        family: Family,
    ) -> Result<Option<PublicKey>, ModeTransitionRefusal> {
        let state = self.state.lock().unwrap();
        let owner = self.resident.lock().unwrap();
        if let Some(resident) = owner.as_ref() {
            let key = resident
                .identities()
                .primary
                .as_ref()
                .ok_or(ModeTransitionRefusal::Unavailable)?
                .signing_ref()
                .map_err(|_| ModeTransitionRefusal::Unavailable)?;
            return PublicKey::from_provider(family, &resident.provider(), key)
                .map(Some)
                .map_err(|_| ModeTransitionRefusal::Unavailable);
        }
        if state.active != 0 {
            return Err(ModeTransitionRefusal::Busy);
        }
        self.read_original_binding(family)
    }

    fn read_original_binding(
        &self,
        family: Family,
    ) -> Result<Option<PublicKey>, ModeTransitionRefusal> {
        use ModeTransitionRefusal::Unavailable;
        let original = self.original.as_ref().ok_or(Unavailable)?;
        let key = self
            .identities
            .primary
            .as_ref()
            .ok_or(Unavailable)?
            .signing_ref()
            .map_err(|_| Unavailable)?;
        if original.can_sign(key, family.algorithm()) != Capability::Supported {
            return Err(Unavailable);
        }
        if let Some(source) = &self.binding {
            return match source
                .public_key(key, family.algorithm())
                .map_err(|_| Unavailable)?
            {
                Some(bytes) => PublicKey::new(family, &bytes)
                    .map(Some)
                    .map_err(|_| Unavailable),
                None => Err(Unavailable),
            };
        }
        PublicKey::from_provider(family, original, key)
            .map(Some)
            .map_err(|_| Unavailable)
    }

    pub(super) fn upgrade(
        &self,
        resident: Arc<ResidentCopy>,
    ) -> Result<bool, ModeTransitionRefusal> {
        self.upgrade_proven(resident, None)
    }

    pub(super) fn upgrade_proven(
        &self,
        resident: Arc<ResidentCopy>,
        public: Option<&PublicKey>,
    ) -> Result<bool, ModeTransitionRefusal> {
        use ModeTransitionRefusal::{Busy, Unavailable};
        let mut state = self.state.lock().unwrap();
        let mut owner = self.resident.lock().unwrap();
        if owner.is_some() {
            return Ok(false);
        }
        if state.active != 0 {
            return Err(Busy);
        }
        let original = self.original.as_ref().ok_or(Unavailable)?;
        let primary = self.identities.primary.as_ref().ok_or(Unavailable)?;
        let key = primary.signing_ref().map_err(|_| Unavailable)?;
        let hmac = [
            Algorithm::HmacSha256,
            Algorithm::HmacSha512,
            Algorithm::Sha256SecretSuffix,
        ]
        .iter()
        .any(|algorithm| original.can_sign(key, *algorithm) == Capability::Supported);
        let families: Vec<_> = [
            Family::Ethereum,
            Family::Stark,
            Family::Schnorr,
            Family::Ed25519,
        ]
        .into_iter()
        .filter(|family| original.can_sign(key, family.algorithm()) == Capability::Supported)
        .collect();
        if original.can_sign(key, Algorithm::Es256) == Capability::Supported
            || (hmac && !families.is_empty())
            || (!hmac && families.len() != 1)
            || (public.is_some() && families.is_empty())
        {
            return Err(Unavailable);
        }
        let aliases = aliases(&self.identities, resident.identities()).map_err(|_| Unavailable)?;
        let provider = resident.provider();
        let mapped = aliases.get(key).ok_or(Unavailable)?;
        for algorithm in [
            Algorithm::HmacSha256,
            Algorithm::HmacSha512,
            Algorithm::Sha256SecretSuffix,
            Algorithm::Secp256k1Digest,
            Algorithm::StarkDigest,
            Algorithm::SchnorrDigest,
            Algorithm::Ed25519,
        ] {
            if original.can_sign(key, algorithm) == Capability::Supported
                && provider.can_sign(mapped, algorithm) != Capability::Supported
            {
                return Err(Unavailable);
            }
        }
        if let Some(family) = families.first().copied() {
            let bound = self.read_original_binding(family)?;
            let expected = bound.as_ref().or(public).ok_or(Unavailable)?;
            if expected.family() != family
                || public.is_some_and(|proof| {
                    proof.family() != family || proof.as_bytes() != expected.as_bytes()
                })
                || !expected
                    .matches_provider(&provider, mapped)
                    .map_err(|_| Unavailable)?
            {
                return Err(Unavailable);
            }
        }
        let link = self.link.as_ref().ok_or(Unavailable)?;
        if !link.promote_to_server_owned() {
            return Err(Busy);
        }
        state.provider = Some(provider);
        state.aliases = Some(aliases);
        *owner = Some(resident);
        Ok(true)
    }
}
impl EngineSigner {
    fn acquire(
        &self,
        mut request: SignRequest,
    ) -> Result<(safe_cex_engine::signing::Provider, SignRequest, Lease), SignError> {
        let mut state = self.state.lock().unwrap();
        let provider = state.provider.clone().ok_or(SignError::Unavailable)?;
        if let Some(aliases) = &state.aliases {
            for operation in &request.operations {
                if let Operation::Sign { key, algorithm, .. } = operation
                    && self.original.as_ref().is_none_or(|original| {
                        original.can_sign(key, *algorithm) != Capability::Supported
                    })
                {
                    return Err(SignError::Unsupported);
                }
            }
            map_request(&mut request, aliases)?;
        }
        state.active += 1;
        Ok((provider, request, Lease(self.state.clone())))
    }
}
impl Signer for EngineSigner {
    fn sign(&self, request: SignRequest) -> SignFuture {
        let acquired = self.acquire(request);
        Box::pin(async move {
            let (provider, request, _lease) = acquired?;
            provider.sign(request).await
        })
    }
    fn immediate(&self) -> bool {
        self.state
            .lock()
            .unwrap()
            .provider
            .as_ref()
            .is_some_and(|provider| provider.immediate())
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        let (provider, request, _lease) = match self.acquire(request.clone()) {
            Ok(work) => work,
            Err(error) => return Some(Err(error)),
        };
        provider.sign_ready(&request)
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        let state = self.state.lock().unwrap();
        let Some(provider) = &state.provider else {
            return Capability::Unsupported;
        };
        if state.aliases.is_some()
            && self
                .original
                .as_ref()
                .is_none_or(|original| original.can_sign(key, algorithm) != Capability::Supported)
        {
            return Capability::Unsupported;
        }
        let key = match &state.aliases {
            Some(aliases) => match aliases.get(key) {
                Some(key) => key,
                None => return Capability::Unsupported,
            },
            None => key,
        };
        provider.can_sign(key, algorithm)
    }
    fn owned_public_address_capability(&self) -> Capability {
        let state = self.state.lock().unwrap();
        if state.aliases.is_some()
            && self.original.as_ref().is_none_or(|original| {
                original.owned_public_address_capability() != Capability::Supported
            })
        {
            return Capability::Unsupported;
        }
        state
            .provider
            .as_ref()
            .map_or(Capability::Unsupported, |provider| {
                provider.owned_public_address_capability()
            })
    }
    fn create_public_address_setup(
        &self,
        request: SignRequest,
    ) -> Option<Box<dyn OwnedSetupExecution>> {
        let (provider, request, lease) = self.acquire(request).ok()?;
        let inner = provider.create_public_address_setup(request)?;
        Some(Box::new(Setup {
            inner,
            lease: Some(lease),
        }))
    }
}
struct Setup {
    inner: Box<dyn OwnedSetupExecution>,
    lease: Option<Lease>,
}
impl OwnedSetupExecution for Setup {
    fn start(&mut self) -> Result<(), SignError> {
        self.inner.start()
    }
    fn cancel(&mut self) -> Result<(), SignError> {
        self.inner.cancel()
    }
    fn poll_result(&mut self, cx: &mut Context<'_>) -> Poll<Result<SignResult, SignError>> {
        self.inner.poll_result(cx)
    }
    fn poll_joined(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SignError>> {
        let result = self.inner.poll_joined(cx);
        if result.is_ready() {
            drop(self.lease.take());
        }
        result
    }
}
fn aliases(old: &Identities, new: &Identities) -> Result<Aliases, SignError> {
    let mut aliases = HashMap::new();
    fn slot(
        aliases: &mut Aliases,
        old: Option<&Identity>,
        new: Option<&Identity>,
    ) -> Result<(), SignError> {
        let Some(old) = old else {
            return Ok(());
        };
        let new = new.ok_or(SignError::MissingKey)?;
        let mut pair = |old: &KeyRef, new: &KeyRef| {
            if aliases.get(old).is_some_and(|existing| existing != new) {
                return Err(SignError::InvalidRequest);
            }
            aliases.insert(old.clone(), new.clone());
            Ok(())
        };
        pair(&old.credential, &new.credential)?;
        pair(old.signing_ref()?, new.signing_ref()?)?;
        if let Some(old) = &old.passphrase {
            pair(old, new.passphrase.as_ref().ok_or(SignError::MissingKey)?)?;
        }
        for (role, old) in &old.extra {
            pair(old, new.extra.get(role).ok_or(SignError::MissingKey)?)?;
        }
        Ok(())
    }
    slot(&mut aliases, old.primary.as_ref(), new.primary.as_ref())?;
    slot(&mut aliases, old.secondary.as_ref(), new.secondary.as_ref())?;
    Ok(aliases)
}
fn map_request(request: &mut SignRequest, aliases: &Aliases) -> Result<(), SignError> {
    let map = |key: &mut KeyRef| -> Result<(), SignError> {
        *key = aliases.get(key).cloned().ok_or(SignError::MissingKey)?;
        Ok(())
    };
    for operation in &mut request.operations {
        let payload = match operation {
            Operation::Sign { key, payload, .. } => {
                map(key)?;
                Some(payload)
            }
            Operation::Sha256 { payload } => Some(payload),
            Operation::Present { key }
            | Operation::PublicAddress { key }
            | Operation::PublicKey { key, .. } => {
                map(key)?;
                None
            }
        };
        if let Some(payload) = payload {
            for part in payload {
                if let PayloadPart::Credential { key } = part {
                    map(key)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    type Calls = Arc<Mutex<Vec<SignRequest>>>;
    use safe_cex_engine::signing::{Context as SigningContext, Identity, SignReceipt};
    struct Recording {
        calls: Arc<Mutex<Vec<SignRequest>>>,
        wait: bool,
    }
    impl Signer for Recording {
        fn can_sign(&self, _: &KeyRef, algorithm: Algorithm) -> Capability {
            if algorithm == Algorithm::HmacSha256 {
                Capability::Supported
            } else {
                Capability::Unsupported
            }
        }
        fn sign(&self, request: SignRequest) -> SignFuture {
            self.calls.lock().unwrap().push(request);
            let wait = self.wait;
            Box::pin(async move {
                if wait {
                    std::future::pending().await
                } else {
                    Err(SignError::Refused)
                }
            })
        }
    }
    fn identities(prefix: &str) -> Identities {
        Identities {
            primary: Some(Identity::new(
                KeyRef(format!("{prefix}/credential")),
                Some(KeyRef(format!("{prefix}/signing"))),
            )),
            secondary: None,
        }
    }
    fn request() -> SignRequest {
        SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(100.),
                expiry_ms: Some(200.),
                ..Default::default()
            },
            context: SigningContext {
                venue: "bybit".into(),
                method: "POST".into(),
                path: "/order".into(),
                ..Default::default()
            },
            operations: vec![
                Operation::Present {
                    key: KeyRef("old/credential".into()),
                },
                Operation::Sign {
                    key: KeyRef("old/signing".into()),
                    algorithm: Algorithm::HmacSha256,
                    payload: vec![
                        PayloadPart::Bytes {
                            bytes: b"old/signing is literal payload".to_vec(),
                        },
                        PayloadPart::Credential {
                            key: KeyRef("old/credential".into()),
                        },
                    ],
                },
            ],
        }
    }
    // This synchronous resident records synthetic requests and owns no keys.
    // Original asynchronous Recording controls retain their existing behavior.
    struct ProtectedRecording(Calls);
    impl safe_cex_engine::signing::ProtectedSigner for ProtectedRecording {}
    impl Signer for ProtectedRecording {
        fn immediate(&self) -> bool {
            true
        }
        fn can_sign(&self, _: &KeyRef, algorithm: Algorithm) -> Capability {
            if algorithm == Algorithm::HmacSha256 {
                Capability::Supported
            } else {
                Capability::Unsupported
            }
        }
        fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
            self.0.lock().unwrap().push(request.clone());
            Some(Err(SignError::Refused))
        }
        fn sign(&self, request: SignRequest) -> SignFuture {
            Box::pin(std::future::ready(self.sign_ready(&request).unwrap()))
        }
    }
    fn resident(calls: Arc<Mutex<Vec<SignRequest>>>) -> Arc<ResidentCopy> {
        let protected =
            safe_cex_engine::signing::ProtectedProvider::new(Arc::new(ProtectedRecording(calls)))
                .unwrap();
        ResidentCopy::test_protected_provider(protected, identities("new"))
    }
    fn signer() -> (Arc<AccountSigner>, Calls, Arc<SignerLink>) {
        let calls: Calls = Arc::default();
        let link = SignerLink::new(
            1,
            Arc::new(|_| panic!("handover must not emit a browser sign request")),
        );
        let signer = AccountSigner::new(
            Some(Arc::new(Recording {
                calls: calls.clone(),
                wait: true,
            })),
            identities("old"),
            Some(link.clone()),
            None,
        );
        (signer, calls, link)
    }
    #[tokio::test]
    async fn unpolled_and_active_signing_block_handover_without_resend_or_cancellation() {
        let (signer, old_calls, link) = signer();
        let new_calls: Calls = Arc::default();
        let future = signer.provider().sign(request());
        assert_eq!(
            signer.upgrade(resident(new_calls.clone())),
            Err(ModeTransitionRefusal::Busy)
        );
        assert!(old_calls.lock().unwrap().is_empty());
        tokio::pin!(future);
        assert!(
            std::future::poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        assert_eq!(old_calls.lock().unwrap().len(), 1);
        assert_eq!(
            signer.upgrade(resident(new_calls.clone())),
            Err(ModeTransitionRefusal::Busy)
        );
        assert!(new_calls.lock().unwrap().is_empty());
        assert!(link.is_available());
    }
    #[tokio::test]
    async fn an_inflight_signature_finishes_once_before_the_new_provider_is_installed() {
        struct Completing(Mutex<Option<tokio::sync::oneshot::Receiver<()>>>);
        impl Signer for Completing {
            fn can_sign(&self, _: &KeyRef, algorithm: Algorithm) -> Capability {
                if algorithm == Algorithm::HmacSha256 {
                    Capability::Supported
                } else {
                    Capability::Unsupported
                }
            }
            fn sign(&self, _: SignRequest) -> SignFuture {
                let receiver = self
                    .0
                    .lock()
                    .unwrap()
                    .take()
                    .expect("original signed exactly once");
                Box::pin(async move {
                    receiver.await.unwrap();
                    Err(SignError::Refused)
                })
            }
        }
        let (done, receiver) = tokio::sync::oneshot::channel();
        let signer = AccountSigner::new(
            Some(Arc::new(Completing(Mutex::new(Some(receiver))))),
            identities("old"),
            Some(SignerLink::new(1, Arc::new(|_| false))),
            None,
        );
        let work = signer.provider().sign(request());
        tokio::pin!(work);
        assert!(
            std::future::poll_fn(|cx| Poll::Ready(work.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        let calls: Calls = Arc::default();
        assert_eq!(
            signer.upgrade(resident(calls.clone())),
            Err(ModeTransitionRefusal::Busy)
        );
        assert!(calls.lock().unwrap().is_empty());
        done.send(()).unwrap();
        assert_eq!(work.await.unwrap_err(), SignError::Refused);
        assert_eq!(signer.upgrade(resident(calls.clone())), Ok(true));
        assert!(signer.provider().sign(request()).await.is_err());
        assert_eq!(calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn settled_handover_maps_only_owned_refs_and_never_replays_old_work() {
        let (signer, old_calls, link) = signer();
        drop(signer.provider().sign(request()));
        let calls: Calls = Arc::default();
        assert_eq!(signer.upgrade(resident(calls.clone())), Ok(true));
        assert!(signer.provider().sign(request()).await.is_err());
        let mut expected = request();
        expected.operations[0] = Operation::Present {
            key: KeyRef("new/credential".into()),
        };
        expected.operations[1] = Operation::Sign {
            key: KeyRef("new/signing".into()),
            algorithm: Algorithm::HmacSha256,
            payload: vec![
                PayloadPart::Bytes {
                    bytes: b"old/signing is literal payload".to_vec(),
                },
                PayloadPart::Credential {
                    key: KeyRef("new/credential".into()),
                },
            ],
        };
        assert_eq!(*calls.lock().unwrap(), vec![expected]);
        assert!(old_calls.lock().unwrap().is_empty());
        link.suspend();
        assert!(
            link.is_available(),
            "browser departure cannot park a server-owned signer"
        );
        assert!(link.absent_for().is_none());
        assert!(!link.send_echo());
        let replacement: Calls = Arc::default();
        assert_eq!(signer.upgrade(resident(replacement.clone())), Ok(false));
        assert!(signer.provider().sign(request()).await.is_err());
        assert_eq!(calls.lock().unwrap().len(), 2);
        assert!(replacement.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn foreign_refs_and_expanded_algorithms_refuse_without_signing() {
        let (signer, _, _) = signer();
        let calls: Calls = Arc::default();
        signer.upgrade(resident(calls.clone())).unwrap();
        let mut foreign = request();
        foreign.operations[0] = Operation::Present {
            key: KeyRef("another-user".into()),
        };
        assert_eq!(
            signer.provider().sign(foreign).await.unwrap_err(),
            SignError::MissingKey
        );
        let mut expanded = request();
        if let Operation::Sign { algorithm, .. } = &mut expanded.operations[1] {
            *algorithm = Algorithm::Ed25519;
        }
        assert_eq!(
            signer.provider().sign(expanded).await.unwrap_err(),
            SignError::Unsupported
        );
        assert!(calls.lock().unwrap().is_empty());
    }
    #[test]
    fn missing_signing_role_refuses_instead_of_building_partial_aliases() {
        let old = Identities {
            primary: Some(Identity::new(KeyRef("old/credential".into()), None)),
            secondary: None,
        };
        assert_eq!(
            aliases(&old, &identities("new")),
            Err(SignError::MissingKey)
        );
    }

    #[test]
    fn missing_identity_role_refuses_before_retiring_browser_signing() {
        let (signer, _, link) = signer();
        let absent = ResidentCopy::test_provider(Arc::new(Recording {
            calls: Arc::default(),
            wait: false,
        }));
        assert_eq!(
            signer.upgrade(absent),
            Err(ModeTransitionRefusal::Unavailable)
        );
        assert!(signer.resident().is_none());
        assert!(link.is_available());
    }

    #[tokio::test]
    async fn a_same_identity_parked_cosigned_copy_can_upgrade_without_waiting_for_park_expiry() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = Arc::new(Registry::new(
                    EngineThreads::start(1).unwrap(),
                    Duration::from_millis(1),
                    Duration::from_secs(60),
                ));
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybit".into(),
                    key: "synthetic-key".into(),
                    ..Default::default()
                };
                let link = SignerLink::new(
                    1,
                    Arc::new(|_| panic!("no browser signing during handover")),
                );
                let remote = prepare(&init, false, Some(link.clone()))
                    .unwrap()
                    .remote
                    .unwrap();
                let options: Value = serde_json::from_str(&remote.options_json).unwrap();
                let identities = serde_json::from_value(options["signing"].clone()).unwrap();
                let running = Arc::new(crate::algos::Running::new(Arc::new(
                    tokio::sync::watch::channel(0).0,
                )));
                running.refresh(&[], &[json!({"id":"running-twap", "status":"running"})], 1);
                let (mut account, mut inbox) = testing::account_on(
                    testing::HeldLoad {
                        placed: Arc::default(),
                        load_after_ms: Some(0.),
                        ticker: true,
                    },
                    ("alice", "acct", "bybit"),
                    Some(link.clone()),
                    init.fingerprint(),
                    running,
                );
                Arc::get_mut(&mut account).unwrap().signing =
                    AccountSigner::new(Some(remote.provider), identities, Some(link.clone()), None);
                testing::bind_identity(&mut account, &init);
                testing::register(&registry, &account);
                let before = account.twap_owner.snapshot();
                account.hold_registration(
                    1,
                    Arc::new(|_| true),
                    Some(("token".into(), u64::MAX)),
                    "acct",
                );
                registry.park("old-session".into(), "alice", vec![(account.clone(), 1)]);
                assert!(link.absent_for().is_some());
                assert_eq!(account.signing.upgrade(resident(Arc::default())), Ok(true));
                assert!(link.is_available());
                assert!(link.absent_for().is_none());
                assert!(account.handle.disposal_cause().is_none());
                assert_eq!(account.twap_owner.snapshot(), before);
                assert_eq!(account.running.counts(), (0, 1));
                assert_eq!(account.running.jobs().1, 1);
                assert!(
                    inbox.try_recv().is_err(),
                    "handover must not cancel, restart or dispose the algo host"
                );
                assert!(Arc::ptr_eq(
                    &registry.accounts.lock().unwrap()
                        [&("alice".into(), account.registry_name().into())],
                    &account
                ));
                account.hold_registration(
                    2,
                    Arc::new(|_| true),
                    Some(("token".into(), u64::MAX)),
                    "acct",
                );
                assert!(Arc::ptr_eq(
                    &registry.held("alice", "acct", 2).unwrap(),
                    &account
                ));
                registry.dispose(&account, Disposal::PageDisposed);
            })
            .await;
    }
}

#[cfg(test)]
mod registration_tests {
    use super::*;
    use crate::{envelope::AccountSigningMode, out::Out};

    #[tokio::test]
    async fn actual_mode_broadcast_uses_each_confirmed_registration_and_skips_pending_or_retired_outputs()
     {
        tokio::task::LocalSet::new()
            .run_until(async {
                let link = SignerLink::new(0, Arc::new(|_| false));
                let (account, _inbox) = testing::account_on(
                    testing::HeldLoad {
                        placed: Arc::default(),
                        load_after_ms: Some(0.),
                        ticker: true,
                    },
                    ("alice", "canonical", "bybit"),
                    Some(link),
                    String::new(),
                    Arc::new(crate::algos::Running::new(Arc::new(
                        tokio::sync::watch::channel(0).0,
                    ))),
                );
                let (a, mut a_rx, _) = Out::channel(16);
                let (b, mut b_rx, _) = Out::channel(16);
                let (pending, mut pending_rx, _) = Out::channel(16);
                let a_registration = a
                    .account("alias-A")
                    .with_instance_id(Some("registration-A"));
                let b_registration = b
                    .account("alias-B")
                    .with_instance_id(Some("registration-B"));
                account.hold_registration(
                    1,
                    a_registration.outbox(),
                    Some(("token".into(), u64::MAX)),
                    "alias-A",
                );
                account.hold_registration(
                    2,
                    b_registration.outbox(),
                    Some(("token".into(), u64::MAX)),
                    "alias-B",
                );
                account.confirm_signing_mode(
                    1,
                    "alias-A",
                    AccountSigningMode::Cosigned,
                    a_registration.outbox(),
                    a_registration.instance_id(),
                );
                account.confirm_signing_mode(
                    2,
                    "alias-B",
                    AccountSigningMode::Cosigned,
                    b_registration.outbox(),
                    b_registration.instance_id(),
                );
                assert!(a_rx.try_recv().is_ok());
                assert!(b_rx.try_recv().is_ok());
                account.hold_registration(
                    3,
                    pending.account("pending").outbox(),
                    Some(("token".into(), u64::MAX)),
                    "pending",
                );
                account
                    .signing
                    .set_test_resident(ResidentCopy::test_provider(Arc::new(
                        safe_cex_crypto::signer::LocalSigner::new(),
                    )));
                account.broadcast_signing_mode();
                for (received, label, instance) in [
                    (a_rx.try_recv().unwrap(), "alias-A", "registration-A"),
                    (b_rx.try_recv().unwrap(), "alias-B", "registration-B"),
                ] {
                    let frame: Value = serde_json::from_str(&received).unwrap();
                    assert_eq!(frame["d"]["accountName"], label);
                    assert_eq!(frame["d"]["instanceId"], instance);
                    assert_eq!(frame["d"]["data"]["actualMode"], "delegated");
                    assert_eq!(frame["d"]["data"]["reason"], "delegated-enabled-elsewhere");
                }
                assert!(pending_rx.try_recv().is_err());
                a.retire_account("alias-A");
                account.broadcast_signing_mode();
                assert!(a_rx.try_recv().is_err());
                assert!(b_rx.try_recv().is_ok());
                assert!(pending_rx.try_recv().is_err());
                account.handle.dispose();
            })
            .await;
    }
}

#[cfg(test)]
#[path = "provider_lifetimes.rs"]
mod provider_lifetimes;
