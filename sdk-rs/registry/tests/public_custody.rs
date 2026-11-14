#![cfg(any(feature = "txflow", feature = "hyperliquid"))]
//! Public agent accounts discard unused private custody.
use safe_cex_engine::{Engine, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer};
use serde_json::json;

struct PublicOnly;
impl Visit for PublicOnly {
    type Output = ();
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) {
        assert!(!engine.account_startup());
    }
}

#[test]
fn unused_private_cleanup_retains_public_agent_addresses() {
    for venue in [
        #[cfg(feature = "txflow")]
        "txflow",
        #[cfg(feature = "hyperliquid")]
        "hyperliquid",
        #[cfg(feature = "hyperliquid")]
        "hyperliquidspot",
    ] {
        let mut options: EngineOptions = serde_json::from_value(json!({
            "publicDataOnly":true, "key":"public-account", "applicationId":"public-application",
            "secret":"unused", "passphrase":"unused"
        }))
        .unwrap();
        let (_, identities) =
            safe_cex_venues::prepare_local_signer_for(venue, &mut options).unwrap();
        assert!(identities.primary.is_none() && identities.secondary.is_none());
        assert_eq!(options.key.as_deref(), Some("public-account"), "{venue}");
        assert_eq!(
            options.application_id.as_deref(),
            Some("public-application")
        );
        assert!(options.secret.is_none() && options.passphrase.is_none());
    }
}

#[test]
fn public_construction_discards_unused_private_custody() {
    let venues = [
        #[cfg(feature = "txflow")]
        "txflow",
        #[cfg(feature = "hyperliquid")]
        "hyperliquid",
        #[cfg(feature = "hyperliquid")]
        "hyperliquidspot",
    ];
    for venue in venues {
        for mode in [
            json!({"publicDataOnly":true}),
            json!({"publicMode":{"requested":true,"payloadSigner":true}}),
            json!({"publicMode":{"requested":null,"payloadSigner":true}}),
        ] {
            for stale in [
                json!({"payloadSigner":true}),
                json!({"secret":"stale-invalid-scalar"}),
                json!({"readCredentials":{"key":"unused","secret":"unused"}}),
                json!({"signing":{"primary":{"credential":"unused","signing":"unused"}}}),
            ] {
                // The shared keyless case has an unused PayloadSigner. Scalars
                // and refs can establish private eligibility on other venues;
                // exercise those only with an explicit public request.
                if mode["publicDataOnly"] != true
                    && mode["publicMode"]["requested"].is_null()
                    && stale.get("payloadSigner").is_none()
                {
                    continue;
                }
                let mut value = json!({"testnet":true,"accountStartup":true});
                if mode["publicDataOnly"] == true || mode["publicMode"]["requested"] == true {
                    value["key"] = "unused-public-key".into();
                    value["applicationId"] = "0x1111111111111111111111111111111111111111".into();
                }
                value
                    .as_object_mut()
                    .unwrap()
                    .extend(mode.as_object().unwrap().clone());
                value
                    .as_object_mut()
                    .unwrap()
                    .extend(stale.as_object().unwrap().clone());
                for injected in [false, true] {
                    let options: EngineOptions = serde_json::from_value(value.clone()).unwrap();
                    let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
                    let result = if injected {
                        build_with_signer(venue, options, entropy, None, PublicOnly)
                    } else {
                        build(venue, options, entropy, PublicOnly)
                    };
                    result.unwrap_or_else(|e| panic!("{venue} {mode} {stale}: {e:?}"));
                }
            }
        }
    }
}

#[cfg(all(feature = "txflow", feature = "dispatch-signing"))]
#[test]
fn public_capsule_ignores_unused_invalid_setup() {
    for mode in [
        json!({"publicDataOnly":true}),
        json!({"publicMode":{"requested":null,"payloadSigner":true}}),
    ] {
        let mut value = json!({"testnet":true,"payloadSigner":true});
        value
            .as_object_mut()
            .unwrap()
            .extend(mode.as_object().unwrap().clone());
        let mut options: EngineOptions = serde_json::from_value(value).unwrap();
        for capsule in [
            "invalid-json",
            r#"{"primary":{"secret":"stale"},"secondary":{"secret":"unused"}}"#,
        ] {
            let (_, ids) =
                safe_cex_venues::local_signer_from_capsule("txflow", capsule.into(), &mut options)
                    .unwrap();
            assert!(ids.primary.is_none() && ids.secondary.is_none());
        }
    }
}
