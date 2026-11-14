#![cfg(not(target_arch = "wasm32"))]
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::*;
use std::sync::Arc;

fn local() -> (Provider, KeyRef) {
    let (signer, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("synthetic-public-key".into()),
            secret: Secret::new("synthetic-test-secret".into()),
            ..Default::default()
        }),
        secondary: None,
    })
    .unwrap();
    (
        Arc::new(signer),
        identities.primary.unwrap().signing_ref().unwrap().clone(),
    )
}
fn request(key: KeyRef) -> SignRequest {
    SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::Sign {
            algorithm: Algorithm::HmacSha256,
            key,
            payload: vec![PayloadPart::Bytes {
                bytes: b"synthetic-payload".to_vec(),
            }],
        }],
    }
}
#[test]
fn protected_erase_refuses_every_outstanding_handle() {
    let (plain, key) = local();
    let expected = plain.sign_ready(&request(key.clone())).unwrap().unwrap();
    let owner = plain
        .protect()
        .expect("canonical local provider must support protected setup");
    let handles: Vec<_> = (0..4).map(|_| owner.provider()).collect();
    for p in &handles {
        assert_eq!(
            p.sign_ready(&request(key.clone())).unwrap().unwrap(),
            expected
        );
    }
    owner.erase();
    assert!(owner.is_erased());
    for p in handles {
        assert_eq!(
            p.sign_ready(&request(key.clone())),
            Some(Err(SignError::Unavailable))
        );
        assert_eq!(
            p.can_sign(&key, Algorithm::HmacSha256),
            Capability::Unsupported
        );
    }
    owner.erase();
}
#[test]
fn protected_owner_drop_erases_while_engine_handle_lives() {
    let (plain, key) = local();
    let owner = plain.protect().expect("protected setup");
    let engine = owner.provider();
    assert!(engine.sign_ready(&request(key.clone())).unwrap().is_ok());
    drop(owner);
    assert_eq!(
        engine.sign_ready(&request(key)),
        Some(Err(SignError::Unavailable))
    );
}
