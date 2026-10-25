use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Algorithm, Operation, PayloadPart, SignError, SignRequest};
use safe_cex_venue_binance::Options;

#[test]
fn credential_options_debug_redacts_and_erasure_scrubs_owned_fields() {
    // Existing audit sentinel; credentials now belong to the provider, not Options.
    let (mut signer, refs) = LocalSigner::from_json(
        r#"{"primary":{"key":"audit-secret-sentinel","secret":"audit-secret-sentinel"}}"#.into(),
    )
    .unwrap();
    let identity = refs.primary.unwrap();
    let options = Options {
        credentials: Some(identity.clone()),
        ..Default::default()
    };
    assert!(!format!("{options:?}").contains("audit-secret-sentinel"));
    let request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::Sign {
            algorithm: Algorithm::HmacSha256,
            key: identity.signing.clone(),
            payload: vec![PayloadPart::Bytes { bytes: vec![] }],
        }],
    };
    assert!(signer.sign_now(&request).is_ok());
    signer.remove(&identity.signing);
    assert_eq!(signer.sign_now(&request), Err(SignError::InvalidRequest));
    signer.remove(&identity.credential);
    let request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::Present {
            key: identity.credential,
        }],
    };
    assert_eq!(signer.sign_now(&request), Err(SignError::InvalidRequest));
}
