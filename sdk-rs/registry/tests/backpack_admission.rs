//! Backpack admission (RUST_BACKPACK_SIGNER_ADMISSION.md, option B): the
//! registry installs a provider and identity derived from the configured key
//! and seed, so managed trading signs through the provider.
#![cfg(feature = "backpack")]

use safe_cex_engine::credential_selection::{
    CredentialRole, CredentialUnavailable, PrimaryReason, SlotPolicy, Slots,
};
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
                key: primary.signing_ref().unwrap().clone(),
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

fn assert_read_provider(identities: Identities, signer: Option<Provider>, primary: bool) {
    assert_eq!(identities.primary.is_some(), primary);
    let read = identities
        .secondary
        .expect("actual secondary identity registered");
    if let Some(primary) = identities.primary {
        assert_ne!(primary.credential, read.credential);
        assert_ne!(
            primary.signing_ref().unwrap().clone(),
            read.signing_ref().unwrap().clone()
        );
    }
    let request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![
            Operation::Present {
                key: read.credential.clone(),
            },
            Operation::Sign {
                algorithm: Algorithm::Ed25519,
                key: read.signing_ref().unwrap().clone(),
                payload: vec![PayloadPart::Bytes { bytes: vec![] }],
            },
        ],
    };
    let signer = signer.expect("secondary provider installed");
    let result = signer.sign_ready(&request).unwrap().unwrap();
    assert_eq!(result.values[0], b"read-api-key");
    assert_eq!(result.values[1], hex(RFC_SIGNATURE));
}

#[test]
fn registry_installs_the_exact_secondary_identity_and_provider() {
    let (identities, signer) = construct(json!({
        "key": "primary-api-key",
        "secret": base64(&[7; 32]),
        "readCredentials": {"key": "read-api-key", "secret": base64(&hex(RFC_SEED))},
    }))
    .unwrap();
    assert_read_provider(identities, signer, true);
}

#[test]
fn secondary_only_identity_uses_a_real_provider_without_a_primary() {
    let (identities, signer) = construct(json!({
        "readCredentials": {"key": "read-api-key", "secret": base64(&hex(RFC_SEED))},
    }))
    .unwrap();
    assert_read_provider(identities, signer, false);
}

#[test]
fn public_only_secondary_config_installs_read_scoped_authority_only() {
    let (identities, signer) = construct(json!({
        "publicDataOnly": true,
        "readCredentials": {"key": "read-api-key", "secret": base64(&hex(RFC_SEED))},
    }))
    .unwrap();
    assert!(identities.primary.is_none());
    assert_read_provider(identities.clone(), signer, false);
    let slots = Slots::new(
        identities.primary,
        identities.secondary,
        SlotPolicy::default(),
    );
    let (selected, _) = slots.select(CredentialRole::Read { scope: None }).unwrap();
    assert_eq!(selected.slot(), safe_cex_engine::CredentialSlot::Secondary);
    for role in [
        CredentialRole::Mutation,
        CredentialRole::SessionAuth,
        CredentialRole::PrimaryRead {
            reason: PrimaryReason::History,
            scope: None,
        },
    ] {
        assert_eq!(
            slots.select(role).err(),
            Some(CredentialUnavailable::Missing)
        );
    }
}

#[test]
fn public_only_invalid_secondary_installs_no_signing_authority() {
    for secret in ["not base64!".to_string(), base64(&[8; 31])] {
        let (identities, signer) = construct(json!({
            "publicDataOnly": true,
            "readCredentials": {"key": "read-api-key", "secret": secret},
        }))
        .unwrap();
        assert!(identities.primary.is_none());
        assert!(identities.secondary.is_none());
        assert!(signer.is_none());
    }
}

#[test]
fn malformed_secondary_does_not_change_existing_primary_setup() {
    for secret in [
        "not base64!".to_string(),
        base64(&[8; 31]),
        base64(&[8; 64]),
    ] {
        let (identities, signer) = construct(json!({
            "key": "primary-api-key",
            "secret": base64(&hex(RFC_SEED)),
            "readCredentials": {"key": "read-api-key", "secret": secret},
        }))
        .unwrap();
        assert!(identities.secondary.is_none());
        let primary = identities.primary.expect("primary retained");
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![
                Operation::Present {
                    key: primary.credential.clone(),
                },
                Operation::Sign {
                    algorithm: Algorithm::Ed25519,
                    key: primary.signing_ref().unwrap().clone(),
                    payload: vec![PayloadPart::Bytes { bytes: vec![] }],
                },
            ],
        };
        let signer = signer.expect("primary provider retained");
        let values = signer.sign_ready(&request).unwrap().unwrap().values.clone();
        assert_eq!(values[0], b"primary-api-key");
        assert_eq!(values[1], hex(RFC_SIGNATURE));
    }
}

#[test]
fn empty_secondary_key_retains_the_existing_configuration_refusal() {
    let error = construct(json!({
        "key": "primary-api-key",
        "secret": base64(&hex(RFC_SEED)),
        "readCredentials": {"key": "", "secret": base64(&[8; 32])},
    }))
    .err()
    .expect("empty secondary key refused");
    assert_eq!(
        error,
        "Secondary read credentials require a nonempty key and secret"
    );
}

#[test]
fn dual_read_setup_keeps_mutation_on_the_primary_provider() {
    use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
    let (identities, signer) = construct(json!({
        "key": "primary-api-key",
        "secret": base64(&hex(RFC_SEED)),
        "readCredentials": {"key": "read-api-key", "secret": base64(&[8; 32])},
    }))
    .unwrap();
    let slots = Slots::new(
        identities.primary,
        identities.secondary,
        SlotPolicy::default(),
    );
    let (selection, identity) = slots.select(CredentialRole::Mutation).unwrap();
    assert_eq!(selection.slot(), safe_cex_engine::CredentialSlot::Primary);
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
                payload: vec![PayloadPart::Bytes { bytes: vec![] }],
            },
        ],
    };
    let signer = signer.expect("primary provider retained");
    let values = signer.sign_ready(&request).unwrap().unwrap().values.clone();
    assert_eq!(values[0], b"primary-api-key");
    assert_eq!(values[1], hex(RFC_SIGNATURE));
}

struct CaptureStartup;
impl Visit for CaptureStartup {
    type Output = (Identities, bool, Vec<safe_cex_engine::Action>);
    fn visit<V: safe_cex_engine::Venue + 'static>(
        self,
        mut engine: safe_cex_engine::Engine<V>,
    ) -> Self::Output {
        let identities = engine.identities().clone();
        let has_signer = engine.signer().is_some();
        engine.start(10.);
        (identities, has_signer, engine.drain_actions().collect())
    }
}

#[test]
fn public_only_malformed_read_seed_keeps_native_config_error_and_no_start() {
    use safe_cex_engine::{Action, Notice, log::code};
    for (secret, expected) in [
        ("not base64!".to_string(), "Invalid Backpack base64 seed"),
        (base64(&[8; 31]), "Ed25519 seed must be 32 bytes"),
    ] {
        let options = serde_json::from_value(json!({
            "publicDataOnly": true,
            "readCredentials": {"key": "read-api-key", "secret": secret},
        }))
        .unwrap();
        let (identities, has_signer, actions) = build(
            "backpack",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(5)),
            CaptureStartup,
        )
        .unwrap();
        assert!(identities.primary.is_none());
        assert!(identities.secondary.is_none());
        assert!(!has_signer);
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Notice(Notice::Event(event))
            if event.code == code::CONFIG_INVALID && event.message == expected));
    }
}

#[test]
fn native_valid_base64_read_seeds_always_construct_and_sign_in_the_provider() {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::KeyEncoding;
    for byte in [0, 1, 7, 8, 127, 128, 255] {
        let secret = base64(&[byte; 32]);
        assert!(safe_cex_venue_backpack::sign::Credentials::new("read".into(), &secret).is_ok());
        let (signer, identities) = LocalSigner::from_setup(LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("primary".into()),
                secret: Secret::new(base64(&[7; 32])),
                secret_encoding: KeyEncoding::Base64,
                ..Default::default()
            }),
            secondary: Some(LocalIdentity {
                key: Secret::new("read".into()),
                secret: Secret::new(secret),
                secret_encoding: KeyEncoding::Base64,
                ..Default::default()
            }),
        })
        .expect("native-valid read seed must not refuse the primary setup");
        let read = identities.secondary.unwrap();
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::Ed25519,
                key: read.signing_ref().unwrap().clone(),
                payload: vec![PayloadPart::Bytes { bytes: vec![] }],
            }],
        };
        assert_eq!(signer.sign_now(&request).unwrap().values[0].len(), 64);
    }
}
