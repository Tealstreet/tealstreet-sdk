//! Revocation of the existing local provider shared by an account and its engine.
use safe_cex_crypto::signer::{LocalSigner, ProtectedBytes};
use safe_cex_engine::signing::{
    Algorithm, Capability, KeyRef, Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use std::sync::{Arc, Mutex};
use tokio::time::Instant;

impl ProtectedBytes for crate::delegated_memory::SecretCopy {
    fn as_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// One owner controls all provider handles, without exporting signing material.
pub struct ResidentCopy {
    signer: Arc<ResidentSigner>,
}

struct ResidentSigner(Mutex<State>);
struct State {
    signer: Option<LocalSigner>,
    deadline: Option<Instant>,
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
        }
    }

    pub fn provider(&self) -> Provider {
        self.signer.clone()
    }

    pub(crate) fn retain_until(&self, deadline: Instant) {
        self.signer.0.lock().unwrap().deadline = Some(deadline);
    }

    pub(crate) fn admit(&self) -> bool {
        let mut state = self.signer.0.lock().unwrap();
        if state.current().is_none() {
            return false;
        }
        state.deadline = None;
        true
    }

    pub(crate) fn revoke(&self) {
        self.signer.0.lock().unwrap().signer = None;
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
        self.0
            .lock()
            .unwrap()
            .current()
            .map_or(Capability::Unsupported, |signer| {
                signer.can_sign(key, algorithm)
            })
    }

    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(
            self.0
                .lock()
                .unwrap()
                .current()
                .ok_or(SignError::Unavailable)
                .and_then(|signer| signer.sign_now(request)),
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn resident() -> (ResidentCopy, Arc<AtomicUsize>) {
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
                Ok(Box::new(buffer))
            })
            .unwrap();
        (ResidentCopy::new(signer), releases)
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
