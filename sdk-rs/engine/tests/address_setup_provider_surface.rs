//! Compile the same downstream overrides with and without address-nonce.
use safe_cex_engine::signing::{
    Capability, OwnedSetupExecution, SignError, SignFuture, SignRequest, Signer,
};
struct Provider;
impl Signer for Provider {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::ready(Err(SignError::Unsupported)))
    }
    fn owned_public_address_capability(&self) -> Capability {
        Capability::Unsupported
    }
    fn create_public_address_setup(&self, _: SignRequest) -> Option<Box<dyn OwnedSetupExecution>> {
        None
    }
}
#[test]
fn downstream_trait_override_is_feature_invariant() {
    let provider: &dyn Signer = &Provider;
    assert_eq!(
        provider.owned_public_address_capability(),
        Capability::Unsupported
    );
    assert!(
        provider
            .create_public_address_setup(SignRequest {
                receipt: Default::default(),
                operations: vec![],
                context: Default::default()
            })
            .is_none()
    );
}
#[test]
fn selection_error_variant_set_is_feature_invariant() {
    use safe_cex_engine::nonce::NonceSelectionError as Error;
    for error in [
        Error::MissingCredential,
        Error::InvalidSubject,
        Error::MissingSigningKey,
        Error::SigningNotConfigured,
        Error::SetupCapability(SignError::Unsupported),
    ] {
        let expected = match error {
            Error::MissingCredential | Error::InvalidSubject => SignError::InvalidRequest,
            Error::MissingSigningKey => SignError::MissingKey,
            Error::SigningNotConfigured => SignError::SigningNotConfigured,
            Error::SetupCapability(error) => error,
        };
        assert_eq!(error.sign_error(), expected);
    }
}
