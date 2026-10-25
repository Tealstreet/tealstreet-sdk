//! ApeX Omni signer admission (RUST_APEXOMNI_SIGNER_ADMISSION.md): the
//! registry installs a provider and opaque identities from trusted setup, the
//! zkLink seed is the provider's `zklinkSeed` role, and raw key material never
//! reaches the venue.
#![cfg(all(feature = "apexomni", feature = "dispatch-signing"))]

use safe_cex_engine::signing::{ExtraKey, Identities, Provider};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer};
use serde_json::{Value, json};

/// The pinned zkLink corpus seed: a public test vector, never a real account.
fn seed() -> String {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../crypto/tests/fixtures/zklink/npm-wasm.json"
    ))
    .unwrap();
    corpus["cases"][0]["seed"].as_str().unwrap().to_owned()
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

/// R1: trusted setup installs the provider and identities, with the zkLink
/// seed as the primary's `zklinkSeed` role.
#[test]
fn registry_installs_the_provider_identities_and_zklink_role() {
    let options: EngineOptions = serde_json::from_value(json!({
        "key": "api-key",
        "secret": "bXlzZWNyZXQ=",
        "passphrase": "pass",
        "omniKeySeed": seed(),
    }))
    .unwrap();
    let (identities, signer) = build(
        "apexomni",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(11)),
        Capture,
    )
    .unwrap();
    assert!(signer.is_some(), "provider installed");
    let primary = identities.primary.expect("primary identity registered");
    assert!(
        !primary.credential.0.is_empty()
            && primary.signing_opt().is_some_and(|key| !key.0.is_empty())
    );
    assert!(primary.passphrase.is_some());
    assert!(primary.extra.contains_key(&ExtraKey::ZklinkSeed));
}

struct Accept;
impl Visit for Accept {
    type Output = ();
    fn visit<V: safe_cex_engine::Venue + 'static>(self, _: safe_cex_engine::Engine<V>) {}
}

fn material() -> Value {
    json!({
        "key": "api-key",
        "secret": "bXlzZWNyZXQ=",
        "passphrase": "pass",
        "omniKeySeed": seed(),
    })
}
fn refs_with(extra: Value) -> EngineOptions {
    let (identities, _) = build(
        "apexomni",
        serde_json::from_value(material()).unwrap(),
        Some(safe_cex_engine::test_support::seeded_entropy(11)),
        Capture,
    )
    .unwrap();
    let mut options: EngineOptions = serde_json::from_value(extra).unwrap();
    options.signing = Some(identities);
    options
}

/// R2: refs need a provider.
#[test]
fn registry_refuses_refs_without_a_provider() {
    let error = build_with_signer(
        "apexomni",
        refs_with(json!({})),
        Some(safe_cex_engine::test_support::seeded_entropy(11)),
        None,
        Accept,
    )
    .unwrap_err();
    assert_eq!(error.message, "Engine signing provider is required");
}

/// R2: refs never travel beside raw material; one test per field.
fn assert_raw_beside_refs_refused(raw: Value) {
    let error = build_with_signer(
        "apexomni",
        refs_with(raw.clone()),
        Some(safe_cex_engine::test_support::seeded_entropy(11)),
        Some(std::sync::Arc::new(
            safe_cex_crypto::signer::LocalSigner::new(),
        )),
        Accept,
    )
    .unwrap_err();
    assert_eq!(
        error.message, "ApeX credentials belong in signer setup, not engine options",
        "{raw}"
    );
}
#[test]
fn registry_refuses_a_raw_key_beside_refs() {
    assert_raw_beside_refs_refused(json!({"key": "api-key"}));
}
#[test]
fn registry_refuses_raw_signing_material_beside_refs() {
    assert_raw_beside_refs_refused(json!({"secret": "bXlzZWNyZXQ="}));
}
#[test]
fn registry_refuses_a_raw_passphrase_beside_refs() {
    assert_raw_beside_refs_refused(json!({"passphrase": "pass"}));
}
#[test]
fn registry_refuses_a_raw_omni_key_seed_beside_refs() {
    assert_raw_beside_refs_refused(json!({"omniKeySeed": seed()}));
}

/// F1 (claude-m26ps4): the 65-byte seed width is enforced at setup, with the
/// legacy start message, so a short seed never reaches the first order.
#[test]
fn a_64_byte_omni_key_seed_is_refused_at_setup() {
    let mut options = material();
    let short = seed();
    options["omniKeySeed"] = json!(short[..short.len() - 2]);
    let error = build(
        "apexomni",
        serde_json::from_value(options).unwrap(),
        Some(safe_cex_engine::test_support::seeded_entropy(11)),
        Capture,
    )
    .err()
    .expect("a short seed is refused at setup");
    assert_eq!(
        error.message,
        "Invalid OmniKey seed: expected 65-byte Ethereum signature"
    );
}

/// The no-options JSON setup path keeps the ApeX admission: a malformed
/// OmniKey seed is refused, a secondary never installs, a valid seed installs.
#[test]
fn json_setup_keeps_the_apexomni_secondary_and_seed_refusals() {
    use safe_cex_venues::local_signer_from_json_for;
    let primary = |seed: String| {
        json!({
            "key": "api-key",
            "secret": "bXlzZWNyZXQ=",
            "passphrase": "pass",
            "extra": { "zklinkSeed": { "value": seed, "encoding": "hex" } },
        })
    };
    let (_, identities) = local_signer_from_json_for(
        "apexomni",
        json!({ "primary": primary(seed()) }).to_string(),
    )
    .unwrap();
    let installed = identities.primary.expect("primary identity");
    assert!(installed.extra.contains_key(&ExtraKey::ZklinkSeed));

    // The family policy declares no secondary slot, so setup refuses it before
    // admission; either way no read identity installs.
    let secondary = json!({ "primary": primary(seed()), "secondary": primary(seed()) });
    assert!(local_signer_from_json_for("apexomni", secondary.to_string()).is_err());

    let short = json!({ "primary": primary("11".repeat(10)) });
    let Err(refused) = local_signer_from_json_for("apexomni", short.to_string()) else {
        panic!("a malformed OmniKey seed must be refused");
    };
    assert!(
        refused.message.contains("Invalid OmniKey seed"),
        "{}",
        refused.message
    );
}
