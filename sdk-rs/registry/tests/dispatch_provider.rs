//! Trusted provider setup remains available without selecting the Binance family.
#![cfg(feature = "dispatch-signing")]

use safe_cex_engine::signing::{
    Algorithm, Context, Identity, Operation, PayloadPart, Provider, SignReceipt, SignRequest,
};
use safe_cex_engine::{ErrorKind, VenueError};
use safe_cex_venues::{
    EngineOptions, local_signer_from_json, local_signer_from_json_for,
    local_signer_from_json_for_mode, prepare_local_signer, prepare_local_signer_for,
    signer_setup_policy,
};

// Fixed independently computed HMAC-SHA256 vectors for `fixture:<credential>:body`.
// The registry tests do not call the provider's HMAC implementation as their oracle.
const RAW_PRIMARY_MAC: &str = "33ae9e9a09bd0cbc69775875f059a0496603e9b4808bb4ad679bf47f3616a88d";
const TRIMMED_PRIMARY_MAC: &str =
    "4dc7d31f16a522be974a3ce772daa6559505481dac82ace579af04442c3ad69b";
const SECONDARY_MAC: &str = "cd43b119df13e8087cb207ae5ad516c801f057ec1ccbb178e7c3876d55fc29bf";

fn assert_owned_identity(
    provider: &Provider,
    identity: &Identity,
    slot: &str,
    expected_key: &str,
    expected_passphrase: &str,
    expected_mac: &str,
) {
    assert_eq!(identity.validate(), Ok(()));
    assert_eq!(identity.credential.0, format!("{slot}.credential"));
    assert_eq!(
        identity.signing_ref().unwrap().clone().0,
        format!("{slot}.signing")
    );
    assert_eq!(
        identity.passphrase.as_ref().unwrap().0,
        format!("{slot}.passphrase")
    );
    let request = SignRequest {
        receipt: SignReceipt {
            nonce: None,
            stamp_ms: Some(1234.),
            expiry_ms: None,
        },
        context: Context {
            venue: "provider-fixture".into(),
            method: "GET".into(),
            host: "fixture.invalid".into(),
            path: "/provider-test".into(),
        },
        operations: vec![
            Operation::Present {
                key: identity.credential.clone(),
            },
            Operation::Present {
                key: identity.passphrase.clone().unwrap(),
            },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: identity.signing_ref().unwrap().clone(),
                payload: vec![
                    PayloadPart::Bytes {
                        bytes: b"fixture:".to_vec(),
                    },
                    PayloadPart::Credential {
                        key: identity.credential.clone(),
                    },
                    PayloadPart::Bytes {
                        bytes: b":body".to_vec(),
                    },
                ],
            },
        ],
    };
    assert!(provider.immediate());
    let result = provider
        .sign_ready(&request)
        .expect("trusted setup installs an immediate provider")
        .expect("opaque identities resolve inside the provider");
    assert_eq!(result.receipt, request.receipt);
    assert_eq!(result.values.len(), 3);
    assert_eq!(result.values[0], expected_key.as_bytes());
    assert_eq!(result.values[1], expected_passphrase.as_bytes());
    assert_eq!(safe_cex_crypto::hex(&result.values[2]), expected_mac);
}

fn assert_sanitized_setup_error(error: VenueError) {
    assert_eq!(error.message, "Invalid local signer setup");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert!(
        error.outcome.is_none(),
        "construction has no command outcome"
    );
    assert!(error.venue_code.is_none());
    assert!(error.venue_message.is_none());
}

#[test]
fn legacy_setup_consumes_both_raw_slots_and_preserves_exact_signing_material() {
    let mut options: EngineOptions = serde_json::from_str(
        r#"{
            "key":" primary-key ","secret":"primary-secret","passphrase":"primary-pass",
            "readCredentials":{"key":"secondary-key","secret":"secondary-secret","passphrase":"secondary-pass"},
            "publicDataOnly":true
        }"#,
    )
    .unwrap();
    let (provider, identities) = prepare_local_signer(&mut options).unwrap();
    assert!(options.key.is_none());
    assert!(options.secret.is_none());
    assert!(options.passphrase.is_none());
    assert!(options.read_credentials.is_none());
    assert!(options.signing.is_none(), "setup returns refs separately");
    assert!(options.public_data_only);
    // The provider remains usable after the raw option owner is gone.
    drop(options);
    let primary = identities.primary.as_ref().unwrap();
    let secondary = identities.secondary.as_ref().unwrap();
    assert!(
        primary.stable_id.is_none(),
        "legacy setup invents no key-derived ID"
    );
    assert!(secondary.stable_id.is_none());
    assert_owned_identity(
        &provider,
        primary,
        "primary",
        " primary-key ",
        "primary-pass",
        RAW_PRIMARY_MAC,
    );
    assert_owned_identity(
        &provider,
        secondary,
        "secondary",
        "secondary-key",
        "secondary-pass",
        SECONDARY_MAC,
    );
}

#[test]
fn json_setup_preserves_explicit_ids_passphrases_and_trusted_key_decoding() {
    let (provider, identities) = local_signer_from_json(
        r#"{
            "primary":{"key":" primary-key ","secret":"primary-secret","passphrase":"primary-pass","stableId":"account/primary-7","trimCredential":true},
            "secondary":{"key":"secondary-key","secret":"7365636f6e646172792d736563726574","secretEncoding":"hex","passphrase":"secondary-pass","stableId":"account/secondary-9"}
        }"#
        .into(),
    )
    .unwrap();
    let primary = identities.primary.as_ref().unwrap();
    let secondary = identities.secondary.as_ref().unwrap();
    assert_eq!(primary.require_stable_id(), Ok("account/primary-7"));
    assert_eq!(secondary.require_stable_id(), Ok("account/secondary-9"));
    assert_owned_identity(
        &provider,
        primary,
        "primary",
        "primary-key",
        "primary-pass",
        TRIMMED_PRIMARY_MAC,
    );
    assert_owned_identity(
        &provider,
        secondary,
        "secondary",
        "secondary-key",
        "secondary-pass",
        SECONDARY_MAC,
    );
    let serialized = serde_json::to_string(&identities).unwrap();
    for material in [
        "primary-key",
        "primary-secret",
        "primary-pass",
        "secondary-key",
        "secondary-secret",
        "secondary-pass",
    ] {
        assert!(
            !serialized.contains(material),
            "only opaque refs leave setup"
        );
    }
}

#[test]
fn malformed_json_and_invalid_setup_metadata_return_only_sanitized_errors() {
    for input in [
        r#"{"primary":{"key":"sensitive-fixture","secret":}}"#,
        r#"{"primary":{"key":"sensitive-fixture","secret":"fixture"},"unknown":"sensitive-fixture"}"#,
        r#"{"primary":{"key":"sensitive-fixture","secret":"invalid-hex","secretEncoding":"hex"}}"#,
        r#"{"primary":{"key":"sensitive-fixture","secret":"fixture","stableId":"bad\nmapping"}}"#,
    ] {
        let error = local_signer_from_json(input.into())
            .err()
            .expect("malformed trusted setup is refused");
        assert_sanitized_setup_error(error);
    }
}

#[test]
fn failing_legacy_setup_also_consumes_all_raw_options() {
    let mut options: EngineOptions = serde_json::from_str(
        r#"{"key":"primary-key","secret":"primary-secret","passphrase":"primary-pass","readCredentials":{"key":"secondary-key","secret":"secondary-secret","passphrase":"secondary-pass"}}"#,
    )
    .unwrap();
    options.key = Some("x".repeat(1024 * 1024 + 1));
    let error = prepare_local_signer(&mut options)
        .err()
        .expect("oversized credential is refused before provider admission");
    assert_sanitized_setup_error(error);
    assert!(options.key.is_none());
    assert!(options.secret.is_none());
    assert!(options.passphrase.is_none());
    assert!(options.read_credentials.is_none());
    assert!(options.signing.is_none());
}

#[test]
fn venue_aware_legacy_setup_applies_compiled_mexc_slot_policy_and_consumes_options() {
    let mut options: EngineOptions = serde_json::from_str(
        r#"{
            "key":"\u0085 primary-key \u0085","secret":"primary-secret","passphrase":" primary-pass ",
            "readCredentials":{"key":" secondary-key ","secret":"secondary-secret","passphrase":" secondary-pass "}
        }"#,
    )
    .unwrap();
    let (provider, identities) = prepare_local_signer_for("mexc", &mut options).unwrap();
    assert!(options.key.is_none());
    assert!(options.secret.is_none());
    assert!(options.passphrase.is_none());
    assert!(options.read_credentials.is_none());
    assert!(options.signing.is_none());
    drop(options);
    assert_owned_identity(
        &provider,
        identities.primary.as_ref().unwrap(),
        "primary",
        "primary-key",
        " primary-pass ",
        TRIMMED_PRIMARY_MAC,
    );
    assert_owned_identity(
        &provider,
        identities.secondary.as_ref().unwrap(),
        "secondary",
        " secondary-key ",
        " secondary-pass ",
        "6146699a63566667eab261b33ba97129f259bc5c5f1bc0288fa4efeac497714c",
    );
    let policy = signer_setup_policy("mexc");
    assert_eq!(policy.primary.unwrap().trim_credential, Some(true));
    assert_eq!(policy.secondary.unwrap().trim_credential, Some(false));
}

#[test]
fn venue_aware_json_overrides_opposite_metadata_with_rust_unicode_whitespace_semantics() {
    // NEL/EM SPACE/PARAGRAPH SEPARATOR trim; BOM and interior spaces survive.
    // Expected MACs were independently computed with Python stdlib hmac/hashlib.
    for (input, expected, mac, stable) in [
        (
            " primary-key ",
            "primary-key",
            TRIMMED_PRIMARY_MAC,
            "mexc:8103f888c531840bd2978455",
        ),
        (
            "\u{0085}\u{2003}primary key\u{2029}",
            "primary key",
            "01c338aeb432927fb69864e80e7ed18d66da37558bddc3444fde7fbaf103b794",
            "mexc:48b46201b873f564129f29ae",
        ),
        (
            "\u{feff} primary-key \u{feff}",
            "\u{feff} primary-key \u{feff}",
            "8584d88cb9f665449763f5b417f28b4de3ecc06c4b947d9ede214e914e077fd0",
            "mexc:b9bd26040f5d07457fb2dca5",
        ),
    ] {
        let setup = serde_json::json!({
            "primary": { "key": input, "secret": "primary-secret", "passphrase": "primary-pass", "stableId": stable, "trimCredential": false },
            "secondary": { "key": " secondary-key ", "secret": "secondary-secret", "passphrase": "secondary-pass", "stableId": "account/secondary", "trimCredential": true },
        });
        let (provider, identities) = local_signer_from_json_for("mexc", setup.to_string()).unwrap();
        assert_eq!(
            identities.primary.as_ref().unwrap().require_stable_id(),
            Ok(stable)
        );
        assert_eq!(
            identities.secondary.as_ref().unwrap().require_stable_id(),
            Ok("account/secondary")
        );
        assert_owned_identity(
            &provider,
            identities.primary.as_ref().unwrap(),
            "primary",
            expected,
            "primary-pass",
            mac,
        );
        assert_owned_identity(
            &provider,
            identities.secondary.as_ref().unwrap(),
            "secondary",
            " secondary-key ",
            "secondary-pass",
            "6146699a63566667eab261b33ba97129f259bc5c5f1bc0288fa4efeac497714c",
        );
    }
}

#[test]
fn undeclared_venue_policy_preserves_caller_trim_and_legacy_default_bytes() {
    for venue in ["binance", "unregistered-setup-fixture"] {
        assert_eq!(signer_setup_policy(venue), Default::default());
        for (trim, key, mac) in [
            (false, " primary-key ", RAW_PRIMARY_MAC),
            (true, "primary-key", TRIMMED_PRIMARY_MAC),
        ] {
            let setup = serde_json::json!({"primary": {
                "key": " primary-key ", "secret": "primary-secret", "passphrase": "primary-pass", "trimCredential": trim
            }});
            let (provider, identities) =
                local_signer_from_json_for(venue, setup.to_string()).unwrap();
            assert_owned_identity(
                &provider,
                identities.primary.as_ref().unwrap(),
                "primary",
                key,
                "primary-pass",
                mac,
            );
            assert!(identities.secondary.is_none());
        }
        let mut options: EngineOptions = serde_json::from_str(
            r#"{
            "key":" primary-key ","secret":"primary-secret","passphrase":"primary-pass"
        }"#,
        )
        .unwrap();
        let (provider, identities) = prepare_local_signer_for(venue, &mut options).unwrap();
        assert_owned_identity(
            &provider,
            identities.primary.as_ref().unwrap(),
            "primary",
            " primary-key ",
            "primary-pass",
            RAW_PRIMARY_MAC,
        );
    }
}

#[test]
fn venue_aware_whitespace_only_primary_and_malformed_setup_do_not_invent_credentials() {
    let mut raw: EngineOptions = serde_json::from_str(
        r#"{
        "key":"\u0085 \t\r\n","secret":"primary-secret"
    }"#,
    )
    .unwrap();
    let (raw_provider, raw_identities) = prepare_local_signer_for("mexc", &mut raw).unwrap();
    let (json_provider, json_identities) = local_signer_from_json_for(
        "mexc",
        r#"{
        "primary":{"key":"\u0085 \t\r\n","secret":"primary-secret","trimCredential":false}
    }"#
        .into(),
    )
    .unwrap();
    for (provider, identities) in [
        (raw_provider, raw_identities),
        (json_provider, json_identities),
    ] {
        let identity = identities.primary.unwrap();
        assert!(identity.credential.0.is_empty());
        assert!(identity.validate().is_err());
        if cfg!(feature = "mexc") {
            assert_eq!(
                identity.stable_id.as_deref(),
                Some("mexc:804dda9de6fb71fa4807af90")
            );
        } else {
            assert!(identity.stable_id.is_none());
        }
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Present {
                key: identity.credential,
            }],
        };
        assert!(provider.sign_ready(&request).unwrap().is_err());
    }
    for setup in [
        r#"{"primary":{"secret":"sentinel"},"unknown":"sentinel"}"#,
        r#"{"primary":{"secret":"sentinel","secretEncoding":"hex"}}"#,
        r#"{"primary":{"stableId":"bad\nmapping"}}"#,
    ] {
        assert_sanitized_setup_error(
            local_signer_from_json_for("mexc", setup.into())
                .err()
                .unwrap(),
        );
    }
    let mut oversized: EngineOptions = serde_json::from_str(r#"{
        "key":"primary-key","secret":"primary-secret","passphrase":"primary-pass",
        "readCredentials":{"key":"secondary-key","secret":"secondary-secret","passphrase":"secondary-pass"}
    }"#).unwrap();
    oversized.key = Some(" ".repeat(1024 * 1024 + 1));
    assert_sanitized_setup_error(
        prepare_local_signer_for("mexc", &mut oversized)
            .err()
            .unwrap(),
    );
    assert!(
        oversized.key.is_none() && oversized.secret.is_none() && oversized.passphrase.is_none()
    );
    assert!(oversized.read_credentials.is_none());
}

#[cfg(feature = "mexc")]
#[test]
fn mexc_provider_injection_requires_its_installed_provider() {
    struct Construct;
    impl safe_cex_venues::Visit for Construct {
        type Output = ();
        fn visit<V: safe_cex_engine::Venue + 'static>(self, _: safe_cex_engine::Engine<V>) {}
    }
    let (provider, identities) = local_signer_from_json_for(
        "mexc",
        r#"{"primary":{"key":" primary-key ","secret":"primary-secret"}}"#.into(),
    )
    .unwrap();
    let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
    options.signing = Some(identities.clone());
    let error = safe_cex_venues::build_with_signer(
        "mexc",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        None,
        Construct,
    )
    .unwrap_err();
    assert_eq!(error.message, "Engine signing provider is required");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert!(error.outcome.is_none());
    for refs in [false, true] {
        let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
        options.signing = refs.then(|| identities.clone());
        assert!(
            safe_cex_venues::build_with_signer(
                "mexc",
                options,
                Some(safe_cex_engine::test_support::seeded_entropy(17)),
                Some(provider.clone()),
                Construct
            )
            .is_ok()
        );
    }
}

// KuCoin futures remains a provider-injection refusal control after BloFin adoption.
#[cfg(feature = "kucoin")]
#[test]
fn enabling_provider_setup_does_not_admit_injection_into_unmigrated_kucoin() {
    struct NeverConstruct;
    impl safe_cex_venues::Visit for NeverConstruct {
        type Output = ();
        fn visit<V: safe_cex_engine::Venue + 'static>(self, _: safe_cex_engine::Engine<V>) {
            panic!("unmigrated provider admission must fail before visiting an engine");
        }
    }
    let (provider, identities) = local_signer_from_json(
        r#"{"primary":{"key":"primary-key","secret":"primary-secret"}}"#.into(),
    )
    .unwrap();
    for venue in ["kucoin"] {
        for (refs, supplied_provider) in [(true, false), (false, true), (true, true)] {
            let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
            options.signing = refs.then(|| identities.clone());
            let error = safe_cex_venues::build_with_signer(
                venue,
                options,
                Some(safe_cex_engine::test_support::seeded_entropy(17)),
                supplied_provider.then(|| provider.clone()),
                NeverConstruct,
            )
            .expect_err("a feature gate must not silently migrate a venue");
            assert_eq!(
                error.message,
                "Injected signing is not supported for this venue yet"
            );
            assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
            assert!(
                error.outcome.is_none(),
                "construction has no command outcome"
            );
        }
    }
}

const REQUIRED_ALIASES: [&str; 7] = [
    "okx",
    "okxeu",
    "okxdemo",
    "okxeudemo",
    "bitget",
    "bitgetusdc",
    "bitgetinverse",
];

fn presented_passphrase(provider: &Provider, identity: &Identity) -> Vec<u8> {
    let request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::Present {
            key: identity.passphrase.clone().unwrap(),
        }],
    };
    provider.sign_ready(&request).unwrap().unwrap().values[0].clone()
}

#[test]
fn required_family_policy_applies_to_raw_json_and_opaque_setup_without_material_changes() {
    use safe_cex_engine::signing::{Identities, IdentitySetupPolicy, KeyRef, SetupFieldPolicy};
    for venue in REQUIRED_ALIASES {
        let policy = signer_setup_policy(venue);
        assert_eq!(
            policy.primary.unwrap().passphrase,
            Some(SetupFieldPolicy::RequiredForPrivate)
        );
        assert_eq!(
            policy.secondary.unwrap().passphrase,
            Some(SetupFieldPolicy::RequiredWithPrimaryFallback)
        );
        let mut raw: EngineOptions = serde_json::from_str(
            r#"{
            "key":" primary-key ","secret":"primary-secret","passphrase":" primary-pass ",
            "readCredentials":{"key":"secondary-key","secret":"secondary-secret"}
        }"#,
        )
        .unwrap();
        let (provider, ids) = prepare_local_signer_for(venue, &mut raw).unwrap();
        assert!(
            raw.key.is_none()
                && raw.secret.is_none()
                && raw.passphrase.is_none()
                && raw.read_credentials.is_none()
        );
        assert_owned_identity(
            &provider,
            ids.primary.as_ref().unwrap(),
            "primary",
            " primary-key ",
            " primary-pass ",
            RAW_PRIMARY_MAC,
        );
        assert_eq!(
            ids.secondary.as_ref().unwrap().passphrase,
            ids.primary.as_ref().unwrap().passphrase
        );
        assert_eq!(
            presented_passphrase(&provider, ids.secondary.as_ref().unwrap()),
            b" primary-pass "
        );
        let setup = r#"{"primary":{"key":" primary-key ","secret":"primary-secret","passphrase":" primary-pass ","stableId":"account/primary"},"secondary":{"key":"secondary-key","secret":"secondary-secret","stableId":"account/secondary"}}"#;
        let (provider, ids) = local_signer_from_json_for(venue, setup.into()).unwrap();
        assert_eq!(
            ids.primary.as_ref().unwrap().require_stable_id(),
            Ok("account/primary")
        );
        assert_eq!(
            ids.secondary.as_ref().unwrap().require_stable_id(),
            Ok("account/secondary")
        );
        assert_eq!(
            presented_passphrase(&provider, ids.secondary.as_ref().unwrap()),
            b" primary-pass "
        );
        let mut opaque = Identities {
            primary: Some(
                Identity::new(
                    KeyRef("primary.credential".into()),
                    Some(KeyRef("primary.signing".into())),
                )
                .with_passphrase(Some(KeyRef("primary.passphrase".into()))),
            ),
            secondary: Some(Identity::new(
                KeyRef("secondary.credential".into()),
                Some(KeyRef("secondary.signing".into())),
            )),
        };
        policy.apply_to_identities(&mut opaque, false).unwrap();
        assert_eq!(
            opaque.secondary.as_ref().unwrap().passphrase,
            opaque.primary.as_ref().unwrap().passphrase
        );
        opaque.secondary.as_mut().unwrap().passphrase = Some(KeyRef(String::new()));
        assert!(policy.apply_to_identities(&mut opaque, false).is_err());
        // An undeclared field remains default optional.
        assert_eq!(IdentitySetupPolicy::default().passphrase, None);
    }
}

#[test]
fn required_material_rejects_missing_empty_and_failed_fallback_but_own_secondary_wins() {
    for venue in REQUIRED_ALIASES {
        for setup in [
            r#"{"primary":{"key":"key","secret":"secret"}}"#,
            r#"{"primary":{"key":"key","secret":"secret","passphrase":""}}"#,
            r#"{"secondary":{"key":"read","secret":"secret"}}"#,
            r#"{"primary":{"passphrase":""},"secondary":{"key":"read","secret":"secret"}}"#,
            r#"{"primary":{"passphrase":"usable"},"secondary":{"key":"read","secret":"secret","passphrase":""}}"#,
        ] {
            assert_sanitized_setup_error(
                local_signer_from_json_for(venue, setup.into())
                    .err()
                    .unwrap(),
            );
        }
        for raw_json in [
            r#"{"key":"key","secret":"secret"}"#,
            r#"{"key":"key","secret":"secret","passphrase":""}"#,
            r#"{"passphrase":"","readCredentials":{"key":"read","secret":"secret"}}"#,
            r#"{"passphrase":"usable","readCredentials":{"key":"read","secret":"secret","passphrase":""}}"#,
        ] {
            let mut raw: EngineOptions = serde_json::from_str(raw_json).unwrap();
            assert_sanitized_setup_error(prepare_local_signer_for(venue, &mut raw).err().unwrap());
            assert!(
                raw.key.is_none()
                    && raw.secret.is_none()
                    && raw.passphrase.is_none()
                    && raw.read_credentials.is_none()
            );
        }
        let (provider, ids) = local_signer_from_json_for(venue, r#"{"primary":{"passphrase":"primary"},"secondary":{"key":"read","secret":"secret","passphrase":"own"}}"#.into()).unwrap();
        assert_ne!(
            ids.secondary.as_ref().unwrap().passphrase,
            ids.primary.as_ref().unwrap().passphrase
        );
        assert_eq!(
            presented_passphrase(&provider, ids.secondary.as_ref().unwrap()),
            b"own"
        );
    }
}

#[test]
fn explicit_setup_mode_precedence_bypasses_only_primary_private_requirement() {
    for venue in REQUIRED_ALIASES {
        for (intent, public) in [
            (r#""publicDataOnly":true"#, true),
            (
                r#""publicDataOnly":false,"publicMode":{"requested":true}"#,
                true,
            ),
            (
                r#""publicDataOnly":true,"publicMode":{"requested":false}"#,
                false,
            ),
            (
                r#""publicDataOnly":true,"publicMode":{"requested":null}"#,
                false,
            ),
            (r#""publicDataOnly":true,"publicMode":{}"#, false),
            (r#""publicDataOnly":false"#, false),
        ] {
            let intent_options: EngineOptions =
                serde_json::from_str(&format!("{{{intent}}}")).unwrap();
            let raw_json = format!(r#"{{{intent},"key":"key","secret":"secret","passphrase":""}}"#);
            let mut raw: EngineOptions = serde_json::from_str(&raw_json).unwrap();
            assert_eq!(safe_cex_venues::signer_setup_public_data_only(&raw), public);
            let result = prepare_local_signer_for(venue, &mut raw);
            assert_eq!(result.is_ok(), public, "{venue}: {intent}");
            let missing = r#"{"primary":{"key":"key","secret":"secret","passphrase":""}}"#;
            assert_eq!(
                local_signer_from_json_for_mode(venue, missing.into(), &intent_options).is_ok(),
                public
            );
            // Even explicit public mode cannot admit incomplete required secondary material.
            let secondary = r#"{"primary":{"key":"key","secret":"secret"},"secondary":{"key":"read","secret":"secret"}}"#;
            assert_sanitized_setup_error(
                local_signer_from_json_for_mode(
                    venue,
                    secondary.into(),
                    &serde_json::from_str(r#"{"publicDataOnly":true}"#).unwrap(),
                )
                .err()
                .unwrap(),
            );
        }
    }
}

#[test]
fn passphrase_only_primary_survives_raw_setup_as_alias_and_defaults_remain_optional() {
    for venue in REQUIRED_ALIASES {
        let mut raw: EngineOptions = serde_json::from_str(r#"{"passphrase":"\u0085 \ufeff exact passphrase ","readCredentials":{"key":"read","secret":"secret"}}"#).unwrap();
        let (provider, ids) = prepare_local_signer_for(venue, &mut raw).unwrap();
        assert!(raw.passphrase.is_none());
        let primary = ids.primary.as_ref().unwrap();
        let secondary = ids.secondary.as_ref().unwrap();
        assert!(primary.credential.0.is_empty() && primary.signing_opt().is_none());
        assert_eq!(secondary.passphrase, primary.passphrase);
        assert_eq!(
            presented_passphrase(&provider, secondary),
            "\u{0085} \u{feff} exact passphrase ".as_bytes()
        );
        let (provider, ids) = local_signer_from_json_for(venue, r#"{"primary":{"passphrase":"\u0085 \ufeff exact passphrase "},"secondary":{"key":"read","secret":"secret"}}"#.into()).unwrap();
        assert_eq!(
            presented_passphrase(&provider, ids.secondary.as_ref().unwrap()),
            "\u{0085} \u{feff} exact passphrase ".as_bytes()
        );
    }
    assert!(
        local_signer_from_json(r#"{"primary":{"key":"key","secret":"secret"}}"#.into()).is_ok()
    );
    assert!(
        local_signer_from_json_for(
            "binance",
            r#"{"primary":{"key":"key","secret":"secret"}}"#.into()
        )
        .is_ok()
    );
}

#[test]
fn json_null_keeps_legacy_none_fallback_without_accepting_explicit_empty_material() {
    for venue in REQUIRED_ALIASES {
        let (provider, ids) = local_signer_from_json_for(venue,
            r#"{"primary":{"passphrase":"primary-pass"},"secondary":{"key":"read","secret":"read-secret","passphrase":null}}"#.into()).unwrap();
        assert_eq!(
            ids.secondary.as_ref().unwrap().passphrase,
            ids.primary.as_ref().unwrap().passphrase
        );
        assert_eq!(
            presented_passphrase(&provider, ids.secondary.as_ref().unwrap()),
            b"primary-pass"
        );
        assert!(local_signer_from_json_for(venue,
            r#"{"primary":{"passphrase":"primary-pass"},"secondary":{"key":"read","secret":"read-secret","passphrase":""}}"#.into()).is_err());
    }
}
