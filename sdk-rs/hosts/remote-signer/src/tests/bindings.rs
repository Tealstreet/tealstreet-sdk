use super::*;
use crate::provider::{Inner, Owned, PublicBindingSource};

const ORDERLY_KEY: &str = "ed25519:GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB";
fn closed_link() -> Arc<SignerLink> {
    SignerLink::new(
        1,
        Arc::new(|_| panic!("reading a binding must not request a signature")),
    )
}

#[test]
fn canonical_public_metadata_is_bound_to_the_original_ref_and_algorithm() {
    let public = bs58::decode(ORDERLY_KEY.strip_prefix("ed25519:").unwrap())
        .into_vec()
        .unwrap();
    for (venue, metadata, algorithm, expected) in [
        (
            "backpack",
            json!({"key":base64::engine::general_purpose::STANDARD.encode(&public)}),
            Algorithm::Ed25519,
            public,
        ),
        (
            "grvt",
            json!({"key":"synthetic-api-key", "grvtSignerAddress":format!("0x{}", "01".repeat(20))}),
            Algorithm::Secp256k1Digest,
            vec![1; 20],
        ),
    ] {
        let (remote, options) = account(closed_link(), venue, metadata);
        let (_, key, _) = refs(&options);
        let source = remote.public_binding_source().unwrap();
        assert_eq!(source.public_key(&key, algorithm), Ok(Some(expected)));
        assert_eq!(
            source.public_key(&KeyRef("foreign".into()), algorithm),
            Err(SignError::Unsupported)
        );
        assert_eq!(
            source.public_key(&key, Algorithm::HmacSha256),
            Err(SignError::Unsupported)
        );
    }
}

#[test]
fn invalid_grvt_metadata_stays_unbound_for_canonical_engine_validation() {
    for address in [Value::Null, json!(""), json!("stale-invalid-address")] {
        let (remote, options) = account(
            closed_link(),
            "grvt",
            json!({"key":"synthetic-public-account", "grvtSignerAddress":address.clone()}),
        );
        assert_eq!(options["grvtSignerAddress"], address);
        let (_, key, _) = refs(&options);
        assert_eq!(
            remote
                .public_binding_source()
                .unwrap()
                .public_key(&key, Algorithm::Secp256k1Digest),
            Err(SignError::Unavailable)
        );
    }
}

#[test]
fn orderly_alias_bindings_do_not_change_hosted_eligibility() {
    for venue in ["orderly", "kodiakfi", "mode", "what", "woofi"] {
        // Exercise the existing provider codec without enabling a hosted venue.
        let inner = Arc::new(Inner::new(
            venue.into(),
            policy::policy("binance").unwrap(),
            None,
            closed_link(),
            Owned {
                credential: KeyRef("original/credential".into()),
                signing: KeyRef("original/signing".into()),
                public: ORDERLY_KEY.as_bytes().to_vec(),
                passphrase: None,
                passphrase_value: None,
                broker: None,
            },
        ));
        let source = PublicBindingSource(inner);
        let expected = bs58::decode(ORDERLY_KEY.strip_prefix("ed25519:").unwrap())
            .into_vec()
            .unwrap();
        assert_eq!(
            source.public_key(&KeyRef("original/signing".into()), Algorithm::Ed25519),
            Ok(Some(expected))
        );
        assert!(
            remote_account(
                closed_link(),
                venue,
                None,
                &json!({"key":ORDERLY_KEY}).to_string()
            )
            .is_err()
        );
    }
}

#[test]
fn unresolved_wallet_bindings_and_malformed_ed25519_never_become_public_proof() {
    for venue in ["hyperliquid", "paradex", "grvt"] {
        let (remote, options) = account(
            closed_link(),
            venue,
            json!({"key":"synthetic-public-account"}),
        );
        let (_, key, _) = refs(&options);
        let algorithm = if venue == "paradex" {
            Algorithm::StarkDigest
        } else {
            Algorithm::Secp256k1Digest
        };
        assert_eq!(
            remote
                .public_binding_source()
                .unwrap()
                .public_key(&key, algorithm),
            Err(SignError::Unavailable)
        );
    }
    for key in ["not-a-canonical-public-key", &"A".repeat(44)] {
        let (remote, options) = account(closed_link(), "backpack", json!({"key":key}));
        let (_, reference, _) = refs(&options);
        assert!(
            remote
                .public_binding_source()
                .unwrap()
                .public_key(&reference, Algorithm::Ed25519)
                .is_err()
        );
    }
}

#[test]
fn unpinned_schnorr_binding_stays_unknown_and_ref_bound() {
    let (remote, options) = account(
        closed_link(),
        "lighter",
        json!({"key":"synthetic-public-account", "apiKeyIndex":"1"}),
    );
    let (_, key, _) = refs(&options);
    let source = remote.public_binding_source().unwrap();
    assert_eq!(source.public_key(&key, Algorithm::SchnorrDigest), Ok(None));
    assert_eq!(
        source.public_key(&KeyRef("foreign".into()), Algorithm::SchnorrDigest),
        Err(SignError::Unsupported)
    );
    let public =
        remote_account(closed_link(), "lighter", None, r#"{"publicDataOnly":true}"#).unwrap();
    assert!(public.public_binding_source().is_none());
}

fn schnorr_public(scalar: u8) -> safe_cex_crypto::admission::PublicKey {
    let mut key = [0; 40];
    key[0] = scalar;
    safe_cex_crypto::admission::PublicKey::new(
        safe_cex_crypto::admission::Family::Schnorr,
        &safe_cex_crypto::lighter::public_key(&key).unwrap(),
    )
    .unwrap()
}

#[test]
fn registered_schnorr_binding_is_initial_immutable_and_ref_bound() {
    let (remote, options) = account(
        closed_link(),
        "lighter",
        json!({"key":"synthetic-public-account", "apiKeyIndex":"1"}),
    );
    let public = schnorr_public(7);
    let remote = remote.with_registered_public_key(&public).unwrap();
    assert!(
        remote
            .with_registered_public_key(&schnorr_public(8))
            .is_err()
    );

    let (remote, _) = account(
        closed_link(),
        "lighter",
        json!({"key":"synthetic-public-account", "apiKeyIndex":"1"}),
    );
    let remote = remote.with_registered_public_key(&public).unwrap();
    let (_, key, _) = refs(&options);
    let (_, owned, _) = refs(&serde_json::from_str(&remote.options_json).unwrap());
    let source = remote.public_binding_source().unwrap();
    assert_eq!(
        source.public_key(&owned, Algorithm::SchnorrDigest),
        Ok(Some(public.as_bytes().to_vec()))
    );
    assert_eq!(
        source.public_key(&key, Algorithm::SchnorrDigest),
        Err(SignError::Unsupported)
    );
}

#[test]
fn published_or_used_remote_provider_cannot_gain_a_registration_binding() {
    for exposure in 0..4 {
        let (remote, options) = account(
            closed_link(),
            "lighter",
            json!({"key":"synthetic-public-account", "apiKeyIndex":"1"}),
        );
        let (_, key, _) = refs(&options);
        let provider = (exposure == 0).then(|| remote.provider.clone());
        let weak = (exposure == 1).then(|| Arc::downgrade(&remote.provider));
        if exposure == 2 {
            drop(remote.public_binding_source());
        }
        if exposure == 3 {
            assert_eq!(
                remote.provider.can_sign(&key, Algorithm::SchnorrDigest),
                Capability::Supported
            );
        }
        assert!(
            remote
                .with_registered_public_key(&schnorr_public(7))
                .is_err()
        );
        drop((provider, weak));
    }
    let (remote, _) = account(
        closed_link(),
        "bybit",
        json!({"key":"synthetic-public-account"}),
    );
    assert!(
        remote
            .with_registered_public_key(&schnorr_public(7))
            .is_err()
    );
}

#[test]
fn schnorr_registration_rejects_another_public_key_family() {
    let (remote, _) = account(
        closed_link(),
        "lighter",
        json!({"key":"synthetic-public-account", "apiKeyIndex":"1"}),
    );
    let other_family = safe_cex_crypto::admission::PublicKey::new(
        safe_cex_crypto::admission::Family::Ed25519,
        &[1; 32],
    )
    .unwrap();
    assert!(remote.with_registered_public_key(&other_family).is_err());
}

#[tokio::test]
async fn completed_setup_binding_stays_with_its_original_provider() {
    for (venue, algorithm, width) in [
        ("hyperliquid", Algorithm::Secp256k1Digest, 20),
        ("paradex", Algorithm::StarkDigest, 32),
    ] {
        let (link, _) = device(Box::new(
            move |_| json!({"wsAuthSignature":format!("0x{}", "01".repeat(width))}),
        ));
        let (remote, options) = account(link, venue, json!({"key":"synthetic-public-account"}));
        let (_, key, _) = refs(&options);
        let source = remote.public_binding_source().unwrap();
        assert_eq!(
            source.public_key(&key, algorithm),
            Err(SignError::Unavailable)
        );
        let operation = if width == 20 {
            Operation::PublicAddress { key: key.clone() }
        } else {
            Operation::PublicKey {
                key: key.clone(),
                algorithm,
            }
        };
        let mut setup = remote
            .provider
            .create_public_address_setup(request(vec![operation], Default::default()))
            .unwrap();
        setup.start().unwrap();
        std::future::poll_fn(|cx| setup.poll_result(cx))
            .await
            .unwrap();
        std::future::poll_fn(|cx| setup.poll_joined(cx))
            .await
            .unwrap();
        assert_eq!(source.public_key(&key, algorithm), Ok(Some(vec![1; width])));
        let (other, other_options) = account(
            closed_link(),
            venue,
            json!({"key":"synthetic-public-account"}),
        );
        let (_, other_key, _) = refs(&other_options);
        assert_eq!(
            source.public_key(&other_key, algorithm),
            Err(SignError::Unsupported)
        );
        assert_eq!(
            other
                .public_binding_source()
                .unwrap()
                .public_key(&other_key, algorithm),
            Err(SignError::Unavailable)
        );
    }
}
