//! Revocation of the existing local provider shared by an account and its engine.
use safe_cex_crypto::signer::{LocalSigner, ProtectedBytes};
use safe_cex_engine::signing::{
    Algorithm, Capability, KeyRef, Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::time::Instant;

impl ProtectedBytes for crate::delegated_memory::SecretCopy {
    fn as_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// One owner controls all provider handles, without exporting signing material.
pub struct ResidentCopy {
    signer: Arc<ResidentSigner>,
    #[cfg(test)]
    test_provider: Option<Provider>,
}

struct ResidentSigner(Mutex<State>);
struct State {
    signer: Option<LocalSigner>,
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
    fn current(&mut self) -> Option<&LocalSigner> {
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.signer = None;
        }
        self.signer.as_ref()
    }
}

impl ResidentCopy {
    pub(super) fn new(signer: LocalSigner) -> Self {
        Self {
            signer: Arc::new(ResidentSigner(Mutex::new(State {
                signer: Some(signer),
                deadline: None,
            }))),
            #[cfg(test)]
            test_provider: None,
        }
    }

    // COSIGNED admission fixtures pass their remote provider through the probe.
    #[cfg(test)]
    pub(crate) fn test_provider(provider: Provider) -> Self {
        Self {
            signer: Arc::new(ResidentSigner(Mutex::new(State {
                signer: None,
                deadline: None,
            }))),
            test_provider: Some(provider),
        }
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
                        .and_then(|signer| signer.sign_now(request))
                }),
        )
    }

    fn sign(&self, request: SignRequest) -> SignFuture {
        let result = self
            .sign_ready(&request)
            .unwrap_or(Err(SignError::Unavailable));
        Box::pin(std::future::ready(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, Secret};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;
    use zeroize::Zeroize;

    struct ReadFault {
        buffer: crate::delegated_memory::SecretCopy,
        armed: Arc<AtomicBool>,
    }

    impl Zeroize for ReadFault {
        fn zeroize(&mut self) {
            self.buffer.zeroize();
        }
    }

    impl ProtectedBytes for ReadFault {
        fn as_bytes(&self) -> &[u8] {
            assert!(
                !self.armed.swap(false, Ordering::SeqCst),
                "synthetic protected read fault"
            );
            self.buffer.as_bytes()
        }
    }

    fn resident() -> (ResidentCopy, Arc<AtomicUsize>) {
        let (resident, releases, _) = resident_with_read_fault();
        (resident, releases)
    }

    fn resident_with_read_fault() -> (ResidentCopy, Arc<AtomicUsize>, Arc<AtomicBool>) {
        let memory = crate::delegated_memory::DelegatedMemory::prepare("bybit", true).unwrap();
        let (signer, _) = LocalSigner::from_setup(LocalSetup {
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
        let signer = signer
            .with_protected_storage(|bytes| {
                let mut buffer = memory.zeroed(bytes.len()).unwrap();
                buffer.as_bytes_mut().copy_from_slice(bytes);
                let releases = releases.clone();
                buffer.check_release(move |allocation| {
                    assert!(!allocation.is_empty());
                    assert!(allocation.iter().all(|byte| *byte == 0));
                    releases.fetch_add(1, Ordering::SeqCst);
                });
                Ok(Box::new(ReadFault {
                    buffer,
                    armed: armed.clone(),
                }))
            })
            .unwrap();
        (ResidentCopy::new(signer), releases, armed)
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
}
