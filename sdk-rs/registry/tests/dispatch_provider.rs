//! Trusted provider setup remains available without selecting the Binance family.
#![cfg(feature = "dispatch-signing")]

use safe_cex_engine::signing::{
    Algorithm, Context, Identity, Operation, PayloadPart, Provider, SignReceipt, SignRequest,
};
use safe_cex_engine::{ErrorKind, VenueError};
use safe_cex_venues::{EngineOptions, local_signer_from_json, prepare_local_signer};

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

#[cfg(feature = "bitunix")]
#[test]
fn enabling_provider_setup_does_not_admit_injection_into_unmigrated_bitunix() {
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
    for venue in ["bitunix"] {
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
