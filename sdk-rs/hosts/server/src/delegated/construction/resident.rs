//! Revocation of the existing local provider shared by an account and its engine.

use safe_cex_engine::signing::{
    Algorithm, Capability, KeyRef, OwnedSetupExecution, ProtectedProvider, Provider, SignError,
    SignFuture, SignRequest, SignResult, Signer,
};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::time::Instant;

/// One owner controls all provider handles, without exporting signing material.
pub struct ResidentCopy {
    signer: Arc<ResidentSigner>,
    pub(super) state: Option<safe_cex_venues::TrustedSignerState>,
    pub(super) identities: safe_cex_engine::signing::Identities,
    pub(super) trusted_identities: Option<safe_cex_native::TrustedSignerIdentities>,
    #[cfg(test)]
    test_provider: Option<Provider>,
}

#[derive(Clone)]
struct ResidentSigner(Arc<Mutex<State>>);
struct State {
    signer: Option<ProtectedProvider>,
    deadline: Option<Instant>,
}

impl ResidentSigner {
    fn state(&self) -> Option<MutexGuard<'_, State>> {
        match self.0.lock() {
            Ok(state) => Some(state),
            Err(poison) => {
                // Recover only to erase. Never clear poison or reuse a provider
                // whose synchronous signing operation unwound.
                poison.into_inner().signer = None;
                None
            }
        }
    }
}

impl State {
    fn current(&mut self) -> Option<Provider> {
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
            || self
                .signer
                .as_ref()
                .is_some_and(ProtectedProvider::is_erased)
        {
            self.signer = None;
        }
        self.signer.as_ref().map(ProtectedProvider::provider)
    }
}

impl ResidentCopy {
    pub(super) fn new(
        signer: ProtectedProvider,
        identities: safe_cex_engine::signing::Identities,
        state: Option<safe_cex_venues::TrustedSignerState>,
    ) -> Self {
        Self {
            signer: Arc::new(ResidentSigner(Arc::new(Mutex::new(State {
                signer: Some(signer),
                deadline: None,
            })))),
            identities,
            state,
            trusted_identities: None,
            #[cfg(test)]
            test_provider: None,
        }
    }

    // COSIGNED admission fixtures pass their remote provider through the probe.
    #[cfg(test)]
    pub(crate) fn test_provider(provider: Provider) -> Arc<Self> {
        Arc::new(Self {
            signer: Arc::new(ResidentSigner(Arc::new(Mutex::new(State {
                signer: None,
                deadline: None,
            })))),
            identities: Default::default(),
            state: None,
            trusted_identities: None,
            test_provider: Some(provider),
        })
    }

    pub(crate) fn identities(&self) -> &safe_cex_engine::signing::Identities {
        &self.identities
    }

    #[cfg(test)]
    pub(crate) fn test_provider_with_identities(
        provider: Provider,
        identities: safe_cex_engine::signing::Identities,
    ) -> Arc<Self> {
        let mut resident = Self::test_provider(provider);
        Arc::get_mut(&mut resident).unwrap().identities = identities;
        resident
    }

    #[cfg(test)]
    pub(crate) fn test_protected_provider(
        protection: ProtectedProvider,
        identities: safe_cex_engine::signing::Identities,
    ) -> Arc<Self> {
        Arc::new(Self::new(protection, identities, None))
    }

    pub fn provider(&self) -> Provider {
        #[cfg(test)]
        if let Some(provider) = &self.test_provider {
            return provider.clone();
        }
        self.signer.clone()
    }

    pub(crate) fn retain_until(&self, deadline: Instant) {
        if let Some(mut state) = self.signer.state() {
            state.deadline = Some(deadline);
        }
    }

    pub(crate) fn admit(&self) -> bool {
        let Some(mut state) = self.signer.state() else {
            return false;
        };
        if state.current().is_none() {
            return false;
        }
        state.deadline = None;
        true
    }

    pub fn erase(&self) {
        self.revoke();
    }

    pub(crate) fn revoke(&self) {
        if let Some(mut state) = self.signer.state() {
            state.signer = None;
        }
    }
}

impl Drop for ResidentCopy {
    fn drop(&mut self) {
        self.revoke();
    }
}

impl Signer for ResidentSigner {
    fn immediate(&self) -> bool {
        true
    }

    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.state().map_or(Capability::Unsupported, |mut state| {
            state.current().map_or(Capability::Unsupported, |signer| {
                signer.can_sign(key, algorithm)
            })
        })
    }

    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(
            self.state()
                .map_or(Err(SignError::Unavailable), |mut state| {
                    state
                        .current()
                        .ok_or(SignError::Unavailable)
                        .and_then(|signer| {
                            signer
                                .sign_ready(request)
                                .unwrap_or(Err(SignError::Unavailable))
                        })
                }),
        )
    }

    fn owned_public_address_capability(&self) -> Capability {
        self.state()
            .and_then(|mut s| s.current())
            .map_or(Capability::Unsupported, |p| {
                p.owned_public_address_capability()
            })
    }
    fn create_public_address_setup(
        &self,
        request: SignRequest,
    ) -> Option<Box<dyn OwnedSetupExecution>> {
        let execution = self
            .state()?
            .current()?
            .create_public_address_setup(request)?;
        Some(Box::new(ResidentSetup {
            execution,
            signer: self.clone(),
        }))
    }
    fn legacy_nonce_association(
        &self,
        request: safe_cex_engine::nonce::handoff::LegacyNonceRequest,
    ) -> Result<safe_cex_engine::nonce::handoff::LegacyNonceProof, SignError> {
        let mut state = self.state().ok_or(SignError::Unavailable)?;
        state
            .current()
            .ok_or(SignError::Unavailable)?
            .legacy_nonce_association(request)
    }

    fn sign(&self, request: SignRequest) -> SignFuture {
        // Retain the original state, not a computed signature or another account.
        // The captured work rechecks erasure and deadline only when executed.
        let captured = self.clone();
        Box::pin(async move {
            captured
                .sign_ready(&request)
                .unwrap_or(Err(SignError::Unavailable))
        })
    }
}

struct ResidentSetup {
    execution: Box<dyn OwnedSetupExecution>,
    signer: ResidentSigner,
}
impl OwnedSetupExecution for ResidentSetup {
    fn start(&mut self) -> Result<(), SignError> {
        let Some(mut state) = self.signer.state() else {
            let _ = self.execution.cancel();
            return Err(SignError::Unavailable);
        };
        if state.current().is_none() {
            let _ = self.execution.cancel();
            return Err(SignError::Unavailable);
        }
        self.execution.start()
    }
    fn cancel(&mut self) -> Result<(), SignError> {
        self.execution.cancel()
    }
    fn poll_result(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<SignResult, SignError>> {
        let Some(mut state) = self.signer.state() else {
            let _ = self.execution.cancel();
            return std::task::Poll::Ready(Err(SignError::Unavailable));
        };
        if state.current().is_none() {
            let _ = self.execution.cancel();
            return std::task::Poll::Ready(Err(SignError::Unavailable));
        }
        self.execution.poll_result(cx)
    }
    fn poll_joined(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), SignError>> {
        self.execution.poll_joined(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::ProtectedSigner;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    // Count destruction of the protected signer, while ordinary engine handles live.
    // Canonical crypto release tests separately assert every mapped byte is zero.
    struct ReadFault {
        owner: ProtectedProvider,
        releases: Arc<AtomicUsize>,
        armed: Arc<AtomicBool>,
    }
    impl ProtectedSigner for ReadFault {}
    impl Drop for ReadFault {
        fn drop(&mut self) {
            self.owner.erase();
            self.releases.fetch_add(2, Ordering::SeqCst);
        }
    }
    impl Signer for ReadFault {
        fn immediate(&self) -> bool {
            true
        }
        fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
            self.owner.provider().can_sign(key, algorithm)
        }
        fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
            assert!(
                !self.armed.swap(false, Ordering::SeqCst),
                "synthetic protected read fault"
            );
            self.owner.provider().sign_ready(request)
        }
        fn sign(&self, request: SignRequest) -> SignFuture {
            self.owner.provider().sign(request)
        }
    }

    #[test]
    fn settings_rebuild_keeps_the_shared_owner_until_the_last_account_drops() {
        let (resident, releases) = resident();
        let owner = Arc::new(resident);
        let provider = owner.provider();
        let prepared = owner
            .reconfigure(&crate::envelope::InitPayload {
                account_name: "synthetic-main".into(),
                exchange_name: "bybit".into(),
                key: "synthetic-key".into(),
                ..Default::default()
            })
            .unwrap();
        drop(owner);
        assert_eq!(releases.load(Ordering::SeqCst), 0);
        assert_eq!(
            provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
            Capability::Supported
        );
        drop(prepared);
        assert!(releases.load(Ordering::SeqCst) > 0);
        assert_eq!(
            provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
            Capability::Unsupported
        );
    }

    fn resident() -> (ResidentCopy, Arc<AtomicUsize>) {
        let (resident, releases, _) = resident_with_read_fault();
        (resident, releases)
    }

    fn resident_with_read_fault() -> (ResidentCopy, Arc<AtomicUsize>, Arc<AtomicBool>) {
        let (signer, identities) = LocalSigner::from_setup(LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("synthetic-key".into()),
                secret: Secret::new("synthetic-secret".into()),
                ..Default::default()
            }),
            secondary: None,
        })
        .unwrap();
        let releases = Arc::new(AtomicUsize::new(0));
        let armed = Arc::new(AtomicBool::new(false));
        let protected = Arc::new(signer).protect().unwrap();
        let owner = ProtectedProvider::new(Arc::new(ReadFault {
            owner: protected,
            releases: releases.clone(),
            armed: armed.clone(),
        }))
        .unwrap();
        (ResidentCopy::new(owner, identities, None), releases, armed)
    }

    fn request() -> SignRequest {
        SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![safe_cex_engine::signing::Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: KeyRef("primary.signing".into()),
                payload: vec![safe_cex_engine::signing::PayloadPart::Bytes {
                    bytes: vec![1, 2, 3],
                }],
            }],
        }
    }

    #[test]
    fn revoke_erases_shared_buffers_before_any_provider_handle_is_released() {
        let (resident, releases) = resident();
        let provider = resident.provider();
        assert!(provider.sign_ready(&request()).unwrap().is_ok());
        resident.revoke();
        assert_eq!(releases.load(Ordering::SeqCst), 2);
        assert_eq!(
            provider.sign_ready(&request()).unwrap(),
            Err(SignError::Unavailable)
        );
        assert!(!resident.admit());
        resident.revoke();
        drop(resident);
        drop(provider);
        assert_eq!(releases.load(Ordering::SeqCst), 2);
    }

    fn poison_resident() -> (ResidentCopy, Provider, Arc<AtomicUsize>, bool) {
        let (resident, releases, armed) = resident_with_read_fault();
        let provider = resident.provider();
        assert!(provider.sign_ready(&request()).unwrap().is_ok());
        armed.store(true, Ordering::SeqCst);
        let panicked = catch_unwind(AssertUnwindSafe(|| provider.sign_ready(&request()))).is_err();
        (resident, provider, releases, panicked)
    }

    #[test]
    fn poisoned_revoke_erases_before_provider_release_and_stays_unavailable() {
        let (resident, provider, releases, panicked) = poison_resident();
        let revoked = catch_unwind(AssertUnwindSafe(|| resident.revoke()));
        let erased_before_provider_release = releases.load(Ordering::SeqCst);
        let retained = catch_unwind(AssertUnwindSafe(|| resident.retain_until(Instant::now())));
        let admitted = catch_unwind(AssertUnwindSafe(|| resident.admit()));
        let dropped = catch_unwind(AssertUnwindSafe(|| drop(resident)));
        let signed = catch_unwind(AssertUnwindSafe(|| provider.sign_ready(&request())));
        let capability = catch_unwind(AssertUnwindSafe(|| {
            provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256)
        }));
        drop(provider);

        assert!(
            panicked,
            "the injected read must unwind through the resident lock"
        );
        assert!(revoked.is_ok(), "revocation must not repeat the read panic");
        assert_eq!(erased_before_provider_release, 2);
        assert!(retained.is_ok());
        assert!(matches!(admitted, Ok(false)));
        assert!(dropped.is_ok());
        assert!(matches!(signed, Ok(Some(Err(SignError::Unavailable)))));
        assert!(matches!(capability, Ok(Capability::Unsupported)));
        assert_eq!(releases.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn poisoned_owner_drop_erases_before_provider_release() {
        let (resident, provider, releases, panicked) = poison_resident();
        let dropped = catch_unwind(AssertUnwindSafe(|| drop(resident)));
        let erased_before_provider_release = releases.load(Ordering::SeqCst);
        let signed = catch_unwind(AssertUnwindSafe(|| provider.sign_ready(&request())));
        drop(provider);

        assert!(
            panicked,
            "the injected read must unwind through the resident lock"
        );
        assert!(dropped.is_ok(), "owner drop must recover for erasure");
        assert_eq!(erased_before_provider_release, 2);
        assert!(matches!(signed, Ok(Some(Err(SignError::Unavailable)))));
        assert_eq!(releases.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn release_deadline_erases_and_refuses_even_before_a_retention_task_runs() {
        let (resident, releases) = resident();
        let provider = resident.provider();
        resident.retain_until(Instant::now() + Duration::from_secs(1));
        assert!(provider.sign_ready(&request()).unwrap().is_ok());
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(
            provider.sign_ready(&request()).unwrap(),
            Err(SignError::Unavailable)
        );
        assert_eq!(releases.load(Ordering::SeqCst), 2);
        assert!(!resident.admit());
    }
    #[tokio::test(start_paused = true)]
    async fn retention_expiry_erases_before_live_handles_and_captured_futures_finish() {
        let (resident, releases) = resident();
        let provider = resident.provider();
        let canonical = resident.signer.state().unwrap().current().unwrap();
        let deferred = provider.sign(request());
        resident.retain_until(Instant::now() + Duration::from_secs(48 * 60 * 60));
        assert!(provider.sign_ready(&request()).unwrap().is_ok());
        tokio::time::advance(Duration::from_secs(48 * 60 * 60)).await;
        assert_eq!(
            provider.sign_ready(&request()),
            Some(Err(SignError::Unavailable))
        );
        assert_eq!(
            releases.load(Ordering::SeqCst),
            2,
            "protected owner erased while both engine handles remain live"
        );
        assert_eq!(
            canonical.sign_ready(&request()),
            Some(Err(SignError::Unavailable))
        );
        assert_eq!(
            deferred.await,
            Err(SignError::Unavailable),
            "no signature captured before expiry may escape after erasure"
        );
        assert!(!resident.admit());
    }

    #[tokio::test]
    async fn explicit_erase_or_last_owner_drop_refuses_previously_created_future() {
        for explicit in [true, false] {
            let (resident, releases) = resident();
            let provider = resident.provider();
            let deferred = provider.sign(request());
            if explicit {
                resident.erase();
            } else {
                drop(resident);
            }
            assert_eq!(releases.load(Ordering::SeqCst), 2);
            assert_eq!(deferred.await, Err(SignError::Unavailable));
            assert_eq!(
                provider.sign_ready(&request()),
                Some(Err(SignError::Unavailable))
            );
        }
    }
    #[tokio::test(start_paused = true)]
    async fn captured_wallet_setup_rechecks_retention_at_start_and_result() {
        for started in [false, true] {
            let key = KeyRef("synthetic-wallet".into());
            let mut signer = LocalSigner::new();
            signer
                .insert_declared(key.clone(), vec![1; 32], Algorithm::Secp256k1Digest)
                .unwrap();
            let resident = ResidentCopy::new(
                Arc::new(signer).protect().unwrap(),
                Default::default(),
                None,
            );
            let provider = resident.provider();
            let mut setup = provider
                .create_public_address_setup(SignRequest {
                    receipt: Default::default(),
                    context: Default::default(),
                    operations: vec![safe_cex_engine::signing::Operation::PublicAddress { key }],
                })
                .unwrap();
            resident.retain_until(Instant::now() + Duration::from_secs(48 * 60 * 60));
            if started {
                setup.start().unwrap();
            }
            tokio::time::advance(Duration::from_secs(48 * 60 * 60)).await;
            if !started {
                assert_eq!(setup.start(), Err(SignError::Unavailable));
            }
            let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
            assert_eq!(
                setup.poll_result(&mut cx),
                std::task::Poll::Ready(Err(SignError::Unavailable))
            );
            assert_eq!(setup.poll_joined(&mut cx), std::task::Poll::Ready(Ok(())));
            assert!(!resident.admit());
        }
    }
}
