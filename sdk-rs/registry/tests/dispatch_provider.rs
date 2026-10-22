//! Trusted provider setup remains available without selecting the Binance family.
#![cfg(feature = "dispatch-signing")]

use safe_cex_engine::signing::{
    Algorithm, Context, Identity, Operation, PayloadPart, Provider, SignReceipt, SignRequest,
};
use safe_cex_engine::{ErrorKind, VenueError};
use safe_cex_venues::{
    EngineOptions, local_signer_from_json, local_signer_from_json_for, prepare_local_signer,
    prepare_local_signer_for, signer_setup_policy,
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
    assert_eq!(identity.signing.0, format!("{slot}.signing"));
    assert_eq!(
        identity.passphrase.as_ref().unwrap().0,
        format!("{slot}.passphrase")
    );
    let request = SignRequest {
        receipt: SignReceipt {
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
                key: identity.signing.clone(),
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
    for (input, expected, mac) in [
        (" primary-key ", "primary-key", TRIMMED_PRIMARY_MAC),
        (
            "\u{0085}\u{2003}primary key\u{2029}",
            "primary key",
            "01c338aeb432927fb69864e80e7ed18d66da37558bddc3444fde7fbaf103b794",
        ),
        (
            "\u{feff} primary-key \u{feff}",
            "\u{feff} primary-key \u{feff}",
            "8584d88cb9f665449763f5b417f28b4de3ecc06c4b947d9ede214e914e077fd0",
        ),
    ] {
        let setup = serde_json::json!({
            "primary": { "key": input, "secret": "primary-secret", "passphrase": "primary-pass", "stableId": "account/primary", "trimCredential": false },
            "secondary": { "key": " secondary-key ", "secret": "secondary-secret", "passphrase": "secondary-pass", "stableId": "account/secondary", "trimCredential": true },
        });
        let (provider, identities) = local_signer_from_json_for("mexc", setup.to_string()).unwrap();
        assert_eq!(
            identities.primary.as_ref().unwrap().require_stable_id(),
            Ok("account/primary")
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
        assert!(identity.stable_id.is_none());
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
fn setup_policy_does_not_adopt_unmigrated_mexc() {
    struct NeverConstruct;
    impl safe_cex_venues::Visit for NeverConstruct {
        type Output = ();
        fn visit<V: safe_cex_engine::Venue + 'static>(self, _: safe_cex_engine::Engine<V>) {
            panic!("setup policy cannot admit provider injection into an unmigrated venue");
        }
    }
    let (provider, identities) = local_signer_from_json_for(
        "mexc",
        r#"{
        "primary":{"key":" primary-key ","secret":"primary-secret"}
    }"#
        .into(),
    )
    .unwrap();
    for (refs, supplied_provider) in [(true, false), (false, true), (true, true)] {
        let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
        options.signing = refs.then(|| identities.clone());
        let error = safe_cex_venues::build_with_signer(
            "mexc",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            supplied_provider.then(|| provider.clone()),
            NeverConstruct,
        )
        .expect_err("setup normalization is independent of family signer adoption");
        assert_eq!(
            error.message,
            "Injected signing is not supported for this venue yet"
        );
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert!(error.outcome.is_none());
    }
}

#[cfg(feature = "bybit")]
#[test]
fn enabling_provider_setup_does_not_admit_injection_into_unmigrated_bybit() {
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
    for venue in [
        "bybit",
        "bybitinverse",
        "bybitspot",
        "bybitdemo",
        "hyrotrader",
    ] {
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
