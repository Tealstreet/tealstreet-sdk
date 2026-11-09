#![cfg(any(
    feature = "bybit",
    feature = "bingx",
    feature = "weex",
    feature = "bitget",
    feature = "okx"
))]
//! Credentials in the separate browser capsule must participate in mode resolution.
use safe_cex_engine::{Engine, Venue};
use safe_cex_venues::{
    EngineOptions, Visit, build_with_signer, local_signer_from_capsule,
    local_signer_from_json_for_mode,
};
use serde_json::json;

struct PrivateOnly;
impl Visit for PrivateOnly {
    type Output = ();
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) {
        assert!(engine.account_startup());
        assert!(engine.signer().is_some());
    }
}

fn preserves_private_capsule(venue: &str) {
    for requested in [Some(false), None] {
        for mutable_options in [true, false] {
            let mut options: EngineOptions = serde_json::from_value(json!({
                "testnet": true, "publicMode": {"requested": requested}
            }))
            .unwrap();
            let capsule = json!({"primary": {
                "key": "fixture-key", "secret": "fixture-secret", "passphrase": "fixture-passphrase"
            }})
            .to_string();
            let (provider, identities) = if mutable_options {
                local_signer_from_capsule(venue, capsule, &mut options)
            } else {
                local_signer_from_json_for_mode(venue, capsule, &options)
            }
            .unwrap();
            assert!(
                identities.primary.is_some(),
                "{venue}: capsule lost its primary"
            );
            assert!(identities.secondary.is_none());
            options.signing = Some(identities);
            let construction = options.construction(venue);
            assert_eq!(construction.can_sign, Some(true), "{venue}");
            assert_eq!(construction.public_data_only, Some(false), "{venue}");
            let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
            build_with_signer(venue, options, entropy, Some(provider), PrivateOnly).unwrap();
        }
    }
    // Public intent cannot bypass another family's capsule validation.
    for mode in [
        json!({"publicDataOnly": true}),
        json!({"publicMode": {"requested": true}}),
    ] {
        let mut options: EngineOptions = serde_json::from_value(mode).unwrap();
        let error = local_signer_from_capsule(venue, "invalid-json".into(), &mut options)
            .err()
            .expect("public intent must retain capsule validation");
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
        assert_eq!(error.message, "Invalid local signer setup");
    }
    // An empty valid capsule cannot manufacture private capability.
    let mut options: EngineOptions =
        serde_json::from_value(json!({"publicMode": {"requested": null}})).unwrap();
    let (_, identities) = local_signer_from_capsule(venue, "{}".into(), &mut options).unwrap();
    assert!(identities.primary.is_none() && identities.secondary.is_none());
    options.signing = Some(identities);
    assert!(
        options
            .construction(venue)
            .public_data_only
            .unwrap_or(options.public_data_only)
    );
}

macro_rules! capsule_test {
    ($feature:literal, $test:ident, $venue:literal) => {
        #[cfg(feature = $feature)]
        #[test]
        fn $test() {
            preserves_private_capsule($venue);
        }
    };
}
capsule_test!("bybit", bybit_private_capsule, "bybit");
capsule_test!("bingx", bingx_private_capsule, "bingx");
capsule_test!("weex", weex_private_capsule, "weex");
capsule_test!("bitget", bitget_private_capsule, "bitget");
capsule_test!("okx", okx_private_capsule, "okx");
