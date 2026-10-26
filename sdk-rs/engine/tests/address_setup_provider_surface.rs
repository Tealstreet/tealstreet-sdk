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

#[test]
fn downstream_venue_declarations_are_feature_invariant() {
    use safe_cex_engine::book::Book;
    use safe_cex_engine::*;
    struct Declared;
    impl Venue for Declared {
        type Pending = ();
        fn address_nonce_domain(&self) -> Option<&'static nonce::NonceDomain> {
            Some(&nonce::HYPERLIQUID_AGENT)
        }
        fn public_address_slots(&self) -> AddressSetupSlots {
            AddressSetupSlots::Both
        }
        fn start(&mut self, _: &mut Cx<'_, ()>) {}
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
    assert_eq!(
        Declared.address_nonce_domain(),
        Some(&nonce::HYPERLIQUID_AGENT)
    );
    assert_eq!(Declared.public_address_slots(), AddressSetupSlots::Both);
}
