//! BloFin's single provider setup (RUST_BLOFIN_HMAC_DISPATCH_RECIPE.md): the
//! registry installs identities built from the configured strings, and the
//! venue holds the same refs for its adopted FetchLeverage Read.
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
    for options in [
        json!({ "publicDataOnly": true, "key": "k", "secret": "s", "passphrase": "p" }),
        json!({ "passphrase": "p", "readCredentials": { "key": "k", "secret": "s" } }),
        json!({}),
    ] {
        let (identities, signer) = construct(options);
        assert_eq!(identities, Identities::default());
        assert!(signer.is_none());
    }
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

#[test]
fn passphrase_refs_mirror_native_eligibility() {
    let read = |passphrase: Option<&str>| {
        let mut read = json!({ "key": "secondary-key", "secret": "secondary-secret" });
        if let Some(passphrase) = passphrase {
            read["passphrase"] = json!(passphrase);
        }
        read
    };
    let base = |passphrase: Option<&str>, secondary: serde_json::Value| {
        let mut options = json!({
            "key": "primary-key", "secret": "primary-secret", "readCredentials": secondary,
        });
        if let Some(passphrase) = passphrase {
            options["passphrase"] = json!(passphrase);
        }
        options
    };
    for (options, primary, secondary) in [
        // No Primary passphrase: refused natively even with the read key's own.
        (base(None, read(Some("secondary-pass"))), false, false),
        // An empty Primary passphrase is refused, as is its inheritance.
        (base(Some(""), read(None)), false, false),
        // An explicit empty Secondary passphrase never falls back to Primary.
        (base(Some("primary-pass"), read(Some(""))), true, false),
        // An empty Primary with an explicit nonempty Secondary signs the GET natively.
        (base(Some(""), read(Some("secondary-pass"))), false, true),
    ] {
        let (identities, _) = construct(options);
        assert_eq!(identities.primary.unwrap().passphrase.is_some(), primary);
        assert_eq!(
            identities.secondary.unwrap().passphrase.is_some(),
            secondary
        );
    }
}
