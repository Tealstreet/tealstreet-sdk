//! Backpack admission (RUST_BACKPACK_SIGNER_ADMISSION.md, option B): the
//! registry installs a provider and identity derived from the configured key
//! and seed, so managed trading signs through the provider.
#![cfg(feature = "backpack")]

use safe_cex_engine::signing::{
    Algorithm, Identities, Operation, PayloadPart, Provider, SignRequest,
};
use safe_cex_venues::{EngineOptions, Visit, build};
use serde_json::json;

// RFC 8032 section 7.1, test 1: seed, empty message and its signature.
const RFC_SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
const RFC_SIGNATURE: &str = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                TABLE[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

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

fn construct(options: serde_json::Value) -> Result<(Identities, Option<Provider>), String> {
    let options: EngineOptions = serde_json::from_value(options).unwrap();
    build(
        "backpack",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        Capture,
    )
    .map_err(|error| error.message)
}

#[test]
fn registry_installs_the_provider_and_identity_from_the_configured_seed() {
    let (identities, signer) = construct(json!({
        "key": "public-api-key",
        "secret": base64(&hex(RFC_SEED)),
    }))
    .unwrap();
    let primary = identities.primary.expect("primary identity registered");
    let signer = signer.expect("provider installed");
    let request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![
            Operation::Present {
                key: primary.credential.clone(),
            },
            Operation::Sign {
                algorithm: Algorithm::Ed25519,
                key: primary.signing.clone(),
                payload: vec![PayloadPart::Bytes { bytes: vec![] }],
            },
        ],
    };
    let values = signer.sign_ready(&request).unwrap().unwrap().values.clone();
    assert_eq!(values[0], b"public-api-key");
    assert_eq!(values[1], hex(RFC_SIGNATURE));
    // Read keys stay on the unadopted raw account-read path; only primary is installed.
    assert!(identities.secondary.is_none());
}

#[test]
fn malformed_or_wrong_length_seeds_are_refused_at_setup() {
    for (secret, message) in [
        ("not base64!".to_owned(), "Invalid Backpack base64 seed"),
        (base64(&[7; 31]), "Backpack Ed25519 seed must be 32 bytes"),
        (base64(&[7; 64]), "Backpack Ed25519 seed must be 32 bytes"),
    ] {
        let Err(error) = construct(json!({ "key": "public-api-key", "secret": secret })) else {
            panic!("a malformed seed must be refused at setup");
        };
        assert_eq!(error, message);
    }
}

#[test]
fn public_and_keyless_accounts_install_nothing() {
    for options in [
        json!({ "publicDataOnly": true, "key": "public-api-key", "secret": "unused" }),
        json!({}),
    ] {
        let (identities, signer) = construct(options).unwrap();
        assert!(identities.primary.is_none());
        assert!(signer.is_none());
    }
}
