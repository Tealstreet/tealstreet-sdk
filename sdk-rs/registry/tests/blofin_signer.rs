//! BloFin's provider setup (RUST_BLOFIN_HMAC_DISPATCH_RECIPE.md): the shared
//! dispatch signer setup installs identities from the configured strings under
//! the declared passphrase policy; the venue holds only the refs.
#![cfg(feature = "blofin")]

use safe_cex_engine::signing::{Identities, Identity, Operation, Provider, SignRequest};
use safe_cex_venues::{EngineOptions, Visit, build};
use serde_json::json;

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
        "blofin",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        Capture,
    )
    .unwrap()
}

/// The presented key and passphrase bytes behind one identity's refs.
fn presented(signer: &Provider, identity: &Identity) -> (Vec<u8>, Vec<u8>) {
    let request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![
            Operation::Present {
                key: identity.credential.clone(),
            },
            Operation::Present {
                key: identity.passphrase.clone().expect("passphrase ref"),
            },
        ],
    };
    let mut values = signer.sign_ready(&request).unwrap().unwrap().values.clone();
    let passphrase = values.pop().unwrap();
    (values.pop().unwrap(), passphrase)
}

#[test]
fn registry_installs_both_slots_and_secondary_inherits_an_absent_passphrase() {
    let (identities, signer) = construct(json!({
        "key": "primary-key",
        "secret": "primary-secret",
        "passphrase": "primary-pass",
        "readCredentials": { "key": "secondary-key", "secret": "secondary-secret" },
    }));
    let signer = signer.expect("provider installed");
    let primary = identities.primary.as_ref().expect("primary identity");
    let secondary = identities.secondary.as_ref().expect("secondary identity");
    assert_eq!(
        presented(&signer, primary),
        (b"primary-key".to_vec(), b"primary-pass".to_vec())
    );
    assert_eq!(
        presented(&signer, secondary),
        (b"secondary-key".to_vec(), b"primary-pass".to_vec())
    );
}

#[test]
fn an_explicit_secondary_passphrase_is_its_own() {
    let (identities, signer) = construct(json!({
        "key": "primary-key",
        "secret": "primary-secret",
        "passphrase": "primary-pass",
        "readCredentials": {
            "key": "secondary-key", "secret": "secondary-secret", "passphrase": "secondary-pass",
        },
    }));
    assert_eq!(
        presented(&signer.unwrap(), identities.secondary.as_ref().unwrap()).1,
        b"secondary-pass"
    );
}

#[test]
fn public_and_keyless_accounts_install_nothing() {
    let (identities, _) = construct(json!({}));
    assert_eq!(identities, Identities::default());
    // Without a Primary key the account resolves public, as for OKX and Bitget,
    // and public mode bypasses the Primary passphrase requirement.
    construct(json!({ "passphrase": "p", "readCredentials": { "key": "k", "secret": "s" } }));
    construct(json!({ "publicDataOnly": true, "key": "k", "secret": "s" }));
}

/// BloFin's venue holds no raw read material, so an invalid read key is refused
/// here, by the shared construction gate, before any venue or provider exists.
#[test]
fn an_invalid_read_key_is_refused_at_construction() {
    let options: EngineOptions = serde_json::from_value(json!({
        "key": "primary-key",
        "secret": "primary-secret",
        "passphrase": "primary-pass",
        "readCredentials": { "key": "secondary-key", "secret": "" },
    }))
    .unwrap();
    assert!(
        build(
            "blofin",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(5)),
            Capture,
        )
        .is_err()
    );
}

/// The shared OKX/Bitget passphrase policy: a private Primary requires its
/// own; an absent Secondary inherits it; an explicit empty never falls back.
#[test]
fn missing_or_explicit_empty_passphrases_are_refused_before_engine_startup() {
    for options in [
        json!({ "key": "primary-key", "secret": "primary-secret",
            "readCredentials": { "key": "secondary-key", "secret": "secondary-secret", "passphrase": "secondary-pass" } }),
        json!({ "key": "primary-key", "secret": "primary-secret", "passphrase": "" }),
        json!({ "key": "primary-key", "secret": "primary-secret", "passphrase": "primary-pass",
            "readCredentials": { "key": "secondary-key", "secret": "secondary-secret", "passphrase": "" } }),
    ] {
        let options: EngineOptions = serde_json::from_value(options).unwrap();
        let error = build(
            "blofin",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(5)),
            Capture,
        )
        .map(|_| ())
        .unwrap_err();
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
        assert!(!error.message.contains("primary-secret"));
    }
}

/// As for Bitget, any engine-options passphrase is refused once identities
/// come from signer setup, even an empty one that would otherwise be ignored.
#[test]
fn an_engine_options_passphrase_is_refused_even_when_empty() {
    for passphrase in ["", "p"] {
        let options: EngineOptions =
            serde_json::from_value(json!({ "passphrase": passphrase })).unwrap();
        let error = safe_cex_venues::build_with_signer(
            "blofin",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(5)),
            None,
            Capture,
        )
        .map(|_| ())
        .unwrap_err();
        assert!(error.message.contains("signer setup"), "{}", error.message);
    }
}
