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

#[test]
fn captured_future_refuses_if_owner_is_erased_before_poll() {
    let (plain, key) = local();
    let owner = plain.protect().expect("protected setup");
    let provider = owner.provider();
    let mut future = provider.sign(request(key));
    owner.erase();
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert_eq!(
        future.as_mut().poll(&mut cx),
        std::task::Poll::Ready(Err(SignError::Unavailable))
    );
}

#[test]
fn setup_refuses_shared_plain_provider_instead_of_leaving_an_unprotected_alias() {
    let (plain, key) = local();
    let alias = plain.clone();
    assert!(matches!(plain.protect(), Err(SignError::InvalidRequest)));
    assert!(alias.sign_ready(&request(key)).unwrap().is_ok());
}

#[cfg(feature = "address-nonce")]
#[test]
fn captured_address_setup_refuses_after_erasure_and_still_joins_cleanup() {
    let key = KeyRef("synthetic-address-key".into());
    let mut signer = LocalSigner::new();
    signer
        .insert_declared(key.clone(), vec![1; 32], Algorithm::Secp256k1Digest)
        .unwrap();
    let plain: Provider = Arc::new(signer);
    let owner = plain.protect().unwrap();
    let provider = owner.provider();
    let work = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::PublicAddress { key }],
    };
    let mut execution = provider.create_public_address_setup(work).unwrap();
    owner.erase();
    assert_eq!(execution.start(), Err(SignError::Unavailable));
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert_eq!(
        execution.poll_result(&mut cx),
        std::task::Poll::Ready(Err(SignError::Unavailable))
    );
    assert_eq!(
        execution.poll_joined(&mut cx),
        std::task::Poll::Ready(Ok(()))
    );
}

#[test]
fn protected_signing_slots_cannot_be_interpolated_or_returned_by_either_entrypoint() {
    for operation_kind in 0..3 {
        let (plain, key) = local();
        let operation = match operation_kind {
            0 => Operation::Present { key: key.clone() },
            1 => Operation::Sha256 {
                payload: vec![PayloadPart::Credential { key: key.clone() }],
            },
            _ => Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: key.clone(),
                payload: vec![PayloadPart::Credential { key: key.clone() }],
            },
        };
        let work = SignRequest {
            operations: vec![operation],
            ..request(key)
        };
        assert!(
            plain.sign_ready(&work).unwrap().is_ok(),
            "legacy policy remains available"
        );
        let owner = plain.protect().unwrap();
        let provider = owner.provider();
        let sync = provider.sign_ready(&work).unwrap();
        let mut future = provider.sign(work);
        let asynchronous = future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()));
        owner.erase();
        assert!(
            sync.is_err(),
            "protected material use {operation_kind} refused synchronously"
        );
        assert!(
            matches!(asynchronous, std::task::Poll::Ready(Err(_))),
            "protected material use {operation_kind} refused asynchronously"
        );
    }
}

#[test]
fn protected_arbitrary_insert_is_not_presentation_authority() {
    // Ref spelling cannot grant permission, nor can replacing previously public material.
    for previously_public in [false, true] {
        let reference = KeyRef("primary.credential".into());
        let mut signer = if previously_public {
            LocalSigner::from_setup(LocalSetup {
                primary: Some(LocalIdentity {
                    key: Secret::new("synthetic-public".into()),
                    ..Default::default()
                }),
                secondary: None,
            })
            .unwrap()
            .0
        } else {
            LocalSigner::new()
        };
        signer.insert(reference.clone(), b"synthetic-private-replacement".to_vec());
        let plain: Provider = Arc::new(signer);
        let owner = plain.protect().unwrap();
        let work = SignRequest {
            operations: vec![Operation::Present {
                key: reference.clone(),
            }],
            ..request(reference)
        };
        assert!(
            owner.provider().sign_ready(&work).unwrap().is_err(),
            "only setup's credential/passphrase install grants presentation"
        );
    }
}
