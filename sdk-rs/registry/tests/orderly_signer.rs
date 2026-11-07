//! Orderly's single provider setup (RUST_ORDERLY_PROVIDER_READS.md): the
//! registry installs the Ed25519 seed as declared base58 and the venue holds
//! only the refs.
#![cfg(feature = "orderly")]

use safe_cex_engine::signing::{
    Algorithm, Identities, Operation, PayloadPart, Provider, SignRequest,
};
use safe_cex_venues::{EngineOptions, Visit, build};
use serde_json::json;

const KEY: &str = "11111111111111111111111111111111";

struct Capture;
impl Visit for Capture {
    type Output = (Identities, Option<Provider>);
    fn visit<V: safe_cex_engine::Venue + 'static>(
        self,
        engine: safe_cex_engine::Engine<V>,
    ) -> Self::Output {
        (engine.identities().clone(), engine.signer())
    }
}

fn construct(options: serde_json::Value) -> (Identities, Option<Provider>) {
    let options: EngineOptions = serde_json::from_value(options).unwrap();
    build(
        "orderly",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        Capture,
    )
    .unwrap()
}

/// Base58 of the 32-byte seed `[7; 32]`.
fn seed() -> String {
    "US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx".into()
}

/// The presented key and an Ed25519 signature over `payload`.
fn present_and_sign(signer: &Provider, ids: &Identities, payload: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let identity = ids.primary.as_ref().expect("primary identity");
    let request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![
            Operation::Present {
                key: identity.credential.clone(),
            },
            Operation::Sign {
                algorithm: Algorithm::Ed25519,
                key: identity.signing_ref().unwrap().clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: payload.to_vec(),
                }],
            },
        ],
    };
    let mut values = signer.sign_ready(&request).unwrap().unwrap().values.clone();
    let signature = values.pop().unwrap();
    (values.pop().unwrap(), signature)
}

#[test]
fn the_registry_installs_the_ed25519_seed_and_presents_the_native_key() {
    // Either string may carry the `ed25519:` prefix, as natively.
    for (key, secret) in [
        (KEY.to_owned(), seed()),
        (format!("ed25519:{KEY}"), format!("ed25519:{}", seed())),
    ] {
        let (ids, signer) = construct(json!({
            "key": key, "secret": secret, "applicationId": "account-1",
        }));
        let signer = signer.expect("provider installed");
        let (presented, signature) = present_and_sign(&signer, &ids, b"1000GET/v1/positions");
        assert_eq!(presented, format!("ed25519:{KEY}").into_bytes());
        assert_eq!(
            signature,
            safe_cex_crypto::ed25519::sign(b"1000GET/v1/positions", &[7; 32])
                .unwrap()
                .to_vec()
        );
        assert!(ids.secondary.is_none());
    }
}

#[test]
fn public_data_only_installs_nothing() {
    let (ids, signer) = construct(json!({
        "publicDataOnly": true, "key": KEY, "secret": seed(), "applicationId": "account-1",
    }));
    assert_eq!(ids, Identities::default());
    assert!(signer.is_none());
}
