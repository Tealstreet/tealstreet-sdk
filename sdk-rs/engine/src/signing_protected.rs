//! A separate erasure authority; engine handles never own that authority.
use super::*;
use std::sync::{Mutex, MutexGuard};

/// Trusted setup's owner of an already protected, immediate provider.
/// Share this owner with `Arc` when needed; the last owner drop erases even if
/// engine provider handles remain alive. No key read, export or replacement API.
pub struct ProtectedProvider {
    signer: Arc<Guarded>,
}

struct Guarded(Arc<Mutex<Option<Provider>>>);
impl Guarded {
    fn state(&self) -> MutexGuard<'_, Option<Provider>> {
        match self.0.lock() {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                // Recover only to erase; never clear poison or resume signing.
                *state = None;
                state
            }
        }
    }
}
impl ProtectedProvider {
    /// For canonical implementations only, after all private material has moved
    /// into protected storage. No other reference to the inner provider may live.
    pub fn new(provider: Provider) -> Result<Self, SignError> {
        if !provider.immediate() || Arc::strong_count(&provider) != 1 {
            return Err(SignError::InvalidRequest);
        }
        Ok(Self {
            signer: Arc::new(Guarded(Arc::new(Mutex::new(Some(provider))))),
        })
    }
    pub fn provider(&self) -> Provider {
        self.signer.clone()
    }
    /// Linearizes with inline signing, dropping and zeroizing the inner provider
    /// before returning. All existing handles then refuse permanently.
    pub fn erase(&self) {
        *self.signer.state() = None;
    }
    pub fn is_erased(&self) -> bool {
        self.signer.state().is_none()
    }
}
impl Drop for ProtectedProvider {
    fn drop(&mut self) {
        self.erase();
    }
}
impl Signer for Guarded {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(
            self.state()
                .as_ref()
                .map_or(Err(SignError::Unavailable), |p| {
                    p.sign_ready(request).unwrap_or(Err(SignError::Unavailable))
                }),
        )
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        let captured = Guarded(self.0.clone());
        Box::pin(async move {
            captured
                .sign_ready(&request)
                .unwrap_or(Err(SignError::Unavailable))
        })
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.state()
            .as_ref()
            .map_or(Capability::Unsupported, |p| p.can_sign(key, algorithm))
    }
    fn owned_public_address_capability(&self) -> Capability {
        self.state().as_ref().map_or(Capability::Unsupported, |p| {
            p.owned_public_address_capability()
        })
    }
    fn create_public_address_setup(
        &self,
        request: SignRequest,
    ) -> Option<Box<dyn OwnedSetupExecution>> {
        let execution = self
            .state()
            .as_ref()?
            .create_public_address_setup(request)?;
        Some(Box::new(GuardedSetup {
            execution,
            guard: Guarded(self.0.clone()),
        }))
    }
    fn legacy_nonce_association(
        &self,
        request: crate::nonce::handoff::LegacyNonceRequest,
    ) -> Result<crate::nonce::handoff::LegacyNonceProof, SignError> {
        self.state()
            .as_ref()
            .ok_or(SignError::Unavailable)?
            .legacy_nonce_association(request)
    }
}

struct GuardedSetup {
    execution: Box<dyn OwnedSetupExecution>,
    guard: Guarded,
}
impl OwnedSetupExecution for GuardedSetup {
    fn start(&mut self) -> Result<(), SignError> {
        let state = self.guard.state();
        state.as_ref().ok_or(SignError::Unavailable)?;
        self.execution.start()
    }
    fn cancel(&mut self) -> Result<(), SignError> {
        self.execution.cancel()
    }
    fn poll_result(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<SignResult, SignError>> {
        let state = self.guard.state();
        if state.is_none() {
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Fault(Arc<AtomicUsize>);
    impl Drop for Fault {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl Signer for Fault {
        fn immediate(&self) -> bool {
            true
        }
        fn sign(&self, _: SignRequest) -> SignFuture {
            Box::pin(std::future::ready(Err(SignError::Unavailable)))
        }
        fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
            panic!("synthetic signing fault")
        }
    }
    #[test]
    fn poisoned_signer_is_erased_before_engine_handle_release() {
        for explicit in [false, true] {
            let released = Arc::new(AtomicUsize::new(0));
            let owner = ProtectedProvider::new(Arc::new(Fault(released.clone()))).unwrap();
            let engine = owner.provider();
            let request = SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![],
            };
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    || engine.sign_ready(&request)
                ))
                .is_err()
            );
            if explicit {
                owner.erase();
                assert!(owner.is_erased());
            } else {
                drop(owner);
            }
            assert_eq!(released.load(Ordering::SeqCst), 1);
            assert_eq!(
                engine.sign_ready(&request),
                Some(Err(SignError::Unavailable))
            );
            assert_eq!(
                engine.can_sign(&KeyRef("synthetic".into()), Algorithm::HmacSha256),
                Capability::Unsupported
            );
            drop(engine);
            assert_eq!(released.load(Ordering::SeqCst), 1);
        }
    }
}
