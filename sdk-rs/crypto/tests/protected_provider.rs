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

#[cfg(all(feature = "stark", feature = "address-nonce"))]
#[test]
fn protected_public_key_factory_refuses_unbound_work_and_owner_drop_settles_cleanup() {
    let key = KeyRef("synthetic-stark".into());
    let mut signer = LocalSigner::new();
    signer
        .insert_declared(key.clone(), vec![1; 32], Algorithm::StarkDigest)
        .unwrap();
    signer.insert(KeyRef("synthetic-mac".into()), vec![2; 32]);
    let plain: Provider = Arc::new(signer);
    let owner = plain.protect().unwrap();
    let provider = owner.provider();
    let valid = SignRequest {
        operations: vec![Operation::PublicKey {
            algorithm: Algorithm::StarkDigest,
            key: key.clone(),
        }],
        ..request(key.clone())
    };
    for operations in [
        vec![],
        vec![Operation::Present { key: key.clone() }],
        request(key.clone()).operations,
        vec![Operation::PublicKey {
            algorithm: Algorithm::StarkDigest,
            key: KeyRef("missing".into()),
        }],
        vec![Operation::PublicKey {
            algorithm: Algorithm::StarkDigest,
            key: KeyRef("synthetic-mac".into()),
        }],
        vec![Operation::PublicKey {
            algorithm: Algorithm::Ed25519,
            key: key.clone(),
        }],
        vec![valid.operations[0].clone(), valid.operations[0].clone()],
    ] {
        assert!(
            provider
                .create_public_address_setup(SignRequest {
                    operations,
                    ..valid.clone()
                })
                .is_none()
        );
    }
    let mut receipt = valid.clone();
    receipt.receipt.stamp_ms = Some(1.);
    assert!(provider.create_public_address_setup(receipt).is_none());
    let mut contextual = valid.clone();
    contextual.context.venue = "synthetic-venue".into();
    assert!(provider.create_public_address_setup(contextual).is_none());
    let mut execution = provider.create_public_address_setup(valid.clone()).unwrap();
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert_eq!(execution.poll_result(&mut cx), std::task::Poll::Pending);
    assert_eq!(execution.poll_joined(&mut cx), std::task::Poll::Pending);
    execution.start().unwrap();
    assert_eq!(execution.start(), Err(SignError::InvalidRequest));
    drop(owner);
    assert!(provider.create_public_address_setup(valid).is_none());
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
fn protected_secondary_and_aliased_signing_slots_refuse_but_credential_recipes_keep_parity() {
    for alias in [false, true] {
        let identity = || LocalIdentity {
            key: Secret::new("synthetic-public".into()),
            secret: Secret::new("synthetic-private".into()),
            passphrase: Some(Secret::new("synthetic-passphrase".into())),
            ..Default::default()
        };
        let mut secondary = identity();
        if alias {
            secondary.secret = Secret::empty();
            secondary.signing_from_primary = true;
        }
        let (signer, ids) = LocalSigner::from_setup(LocalSetup {
            primary: Some(identity()),
            secondary: Some(secondary),
        })
        .unwrap();
        let plain: Provider = Arc::new(signer);
        let primary = ids.primary.unwrap();
        let secondary = ids.secondary.unwrap();
        let mut recipe = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Present {
                key: primary.credential.clone(),
            }],
        };
        for id in [&primary, &secondary] {
            recipe.operations.push(Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: id.signing_ref().unwrap().clone(),
                payload: vec![
                    PayloadPart::Credential {
                        key: id.credential.clone(),
                    },
                    PayloadPart::Credential {
                        key: id.passphrase.clone().unwrap(),
                    },
                    PayloadPart::Result {
                        index: 0,
                        encoding: Encoding::Raw,
                    },
                ],
            });
        }
        let expected = plain.sign_ready(&recipe).unwrap().unwrap();
        let owner = plain.protect().unwrap();
        let provider = owner.provider();
        assert_eq!(provider.sign_ready(&recipe).unwrap().unwrap(), expected);
        for id in [primary, secondary] {
            let work = SignRequest {
                operations: vec![Operation::Present {
                    key: id.signing_ref().unwrap().clone(),
                }],
                ..recipe.clone()
            };
            assert!(provider.sign_ready(&work).unwrap().is_err());
        }
        let mut future = provider.sign(recipe);
        drop(owner);
        assert_eq!(
            future
                .as_mut()
                .poll(&mut std::task::Context::from_waker(std::task::Waker::noop())),
            std::task::Poll::Ready(Err(SignError::Unavailable))
        );
    }
}
