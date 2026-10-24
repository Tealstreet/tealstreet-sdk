//! Declared read-key policy (docs/plans/RUST_READ_KEY_POLICY.md). No venue
//! declares a policy yet, so these drive the policy explicitly.
use super::construction::ReadCredentialsPolicy::{Install, PublicRead, ValidateOnly};
use super::*;

const NATIVE: &str = "Secondary read credentials require a nonempty key and secret";

fn options(json: &str) -> EngineOptions {
    serde_json::from_str(json).unwrap()
}
fn injected(primary: bool, secondary: bool) -> safe_cex_engine::signing::Identities {
    let identity = |name: &str| safe_cex_engine::signing::Identity {
        credential: safe_cex_engine::signing::KeyRef(format!("{name}/credential")),
        signing: safe_cex_engine::signing::KeyRef(format!("{name}/signing")),
        passphrase: None,
        stable_id: None,
        extra: Default::default(),
    };
    safe_cex_engine::signing::Identities {
        primary: primary.then(|| identity("primary")),
        secondary: secondary.then(|| identity("secondary")),
    }
}

#[test]
fn a_false_result_keeps_the_native_construction_refusal_exactly() {
    let error =
        read_key_options(ValidateOnly, &options(r#"{"readCredentialsValid":false}"#)).unwrap_err();
    assert_eq!(error.message, NATIVE);
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, None);
    // The raw-key refusal it replaces, for the same input shape.
    let raw = options(r#"{"readCredentials":{"key":"read"}}"#);
    assert_eq!(
        raw.read_credentials.as_ref().unwrap().validate(),
        Err(NATIVE)
    );
    for accepted in [r#"{"readCredentialsValid":true}"#, "{}"] {
        assert!(
            read_key_options(ValidateOnly, &options(accepted)).is_ok(),
            "{accepted}"
        );
    }
}

#[test]
fn the_result_travels_only_for_a_validate_only_family_and_never_with_the_key() {
    for policy in [Install, PublicRead] {
        assert!(read_key_options(policy, &options(r#"{"readCredentialsValid":true}"#)).is_err());
    }
    let both = options(
        r#"{"readCredentials":{"key":"read","secret":"value"},"readCredentialsValid":true}"#,
    );
    assert!(read_key_options(ValidateOnly, &both).is_err());
    // The unchanged default: an undeclared family keeps its read key as today.
    let installed = options(r#"{"readCredentials":{"key":"read","secret":"value"}}"#);
    assert!(read_key_options(Install, &installed).is_ok());
}

#[test]
fn an_injected_read_identity_is_refused_for_a_validate_only_family() {
    assert!(read_key_identities(ValidateOnly, &injected(true, true), false).is_err());
    assert!(read_key_identities(ValidateOnly, &injected(true, false), false).is_ok());
    assert!(read_key_identities(Install, &injected(true, true), false).is_ok());
    let mut with_signing = options("{}");
    with_signing.signing = Some(injected(true, true));
    assert!(read_key_options(ValidateOnly, &with_signing).is_err());
}

#[test]
fn a_public_only_public_read_family_never_holds_a_primary() {
    assert!(read_key_identities(PublicRead, &injected(true, true), true).is_err());
    assert!(read_key_identities(PublicRead, &injected(false, true), true).is_ok());
    // Private mode is unchanged.
    assert!(read_key_identities(PublicRead, &injected(true, true), false).is_ok());
    assert!(read_key_identities(Install, &injected(true, true), true).is_ok());
}

#[cfg(feature = "dispatch-signing")]
mod setup {
    use super::*;

    fn prepare(
        json: &str,
        public: bool,
        policy: construction::ReadCredentialsPolicy,
    ) -> (EngineOptions, safe_cex_engine::signing::Identities) {
        let mut options = options(json);
        let (_, identities) =
            prepare_local_signer_with_policy(&mut options, Default::default(), public, policy)
                .unwrap();
        (options, identities)
    }

    #[test]
    fn validate_only_records_the_result_and_installs_no_read_identity() {
        for invalid_read in [
            r#","readCredentials":{"key":"read"}"#,
            r#","readCredentials":{"key":"","secret":"value"}"#,
        ] {
            let mut options = options(&format!(
                r#"{{"key":"primary","secret":"value"{invalid_read}}}"#
            ));
            let Err(error) = prepare_local_signer_with_policy(
                &mut options,
                Default::default(),
                false,
                ValidateOnly,
            ) else {
                panic!("an invalid read key built");
            };
            assert_eq!(error.message, NATIVE);
            assert!(
                options.read_credentials.is_none(),
                "material left in options"
            );
        }
        for (read, expected) in [
            (
                r#","readCredentials":{"key":"read","secret":"value"}"#,
                Some(true),
            ),
            ("", None),
        ] {
            for public in [false, true] {
                let json = format!(r#"{{"key":"primary","secret":"value"{read}}}"#);
                let (options, identities) = prepare(&json, public, ValidateOnly);
                assert_eq!(
                    options.read_credentials_valid, expected,
                    "{json} public={public}"
                );
                assert!(
                    options.read_credentials.is_none(),
                    "material left in options"
                );
                assert!(
                    identities.secondary.is_none(),
                    "a read identity was installed"
                );
                assert!(identities.primary.is_some(), "primary construction changed");
            }
        }
    }

    #[test]
    fn an_invalid_read_key_outranks_a_malformed_primary_as_natively() {
        let base64 = safe_cex_engine::signing::CredentialSetupPolicy {
            primary: Some(safe_cex_engine::signing::IdentitySetupPolicy {
                secret_encoding: Some(safe_cex_engine::signing::KeyEncoding::Base64),
                ..Default::default()
            }),
            secondary: None,
        };
        let json = r#"{"key":"primary","secret":"not base64!","readCredentials":{"key":"read"}}"#;
        let mut validate_only = options(json);
        let Err(error) =
            prepare_local_signer_with_policy(&mut validate_only, base64, false, ValidateOnly)
        else {
            panic!("a malformed primary and an invalid read key built");
        };
        assert_eq!(error.message, NATIVE);
        assert!(
            validate_only.read_credentials.is_none(),
            "material left in options"
        );
        // Control: with a valid read key the primary decode failure surfaces.
        let mut valid_read = options(
            r#"{"key":"primary","secret":"not base64!","readCredentials":{"key":"read","secret":"value"}}"#,
        );
        let Err(primary) =
            prepare_local_signer_with_policy(&mut valid_read, base64, false, ValidateOnly)
        else {
            panic!("a malformed primary built");
        };
        assert_ne!(primary.message, NATIVE);
    }

    #[test]
    fn trusted_setup_never_overwrites_a_caller_supplied_result() {
        for preset in [
            r#""readCredentialsValid":false"#,
            r#""readCredentialsValid":true,"readCredentials":{"key":"read","secret":"value"}"#,
        ] {
            let mut options = options(&format!(r#"{{"key":"primary","secret":"value",{preset}}}"#));
            assert!(
                prepare_local_signer_with_policy(
                    &mut options,
                    Default::default(),
                    false,
                    ValidateOnly
                )
                .is_err(),
                "{preset}"
            );
        }
    }

    #[test]
    fn the_default_installs_the_read_key_exactly_as_before() {
        let json = r#"{"key":"primary","secret":"value","readCredentials":{"key":"read","secret":"value"}}"#;
        for public in [false, true] {
            let (options, identities) = prepare(json, public, Install);
            assert_eq!(options.read_credentials_valid, None);
            assert!(identities.primary.is_some() && identities.secondary.is_some());
        }
    }

    #[test]
    fn public_read_drops_the_primary_only_in_public_only_mode() {
        let json = r#"{"key":"primary","secret":"value","readCredentials":{"key":"read","secret":"value"}}"#;
        let (options, public) = prepare(json, true, PublicRead);
        assert!(
            public.primary.is_none(),
            "valid primary options still installed"
        );
        assert!(public.secondary.is_some());
        assert!(options.key.is_none() && options.secret.is_none());
        assert!(read_key_identities(PublicRead, &public, true).is_ok());
        let (_, private) = prepare(json, false, PublicRead);
        assert!(private.primary.is_some() && private.secondary.is_some());
    }
}

#[test]
fn the_bridge_record_carries_the_policy_only_when_declared() {
    let undeclared = serde_json::to_string(&construction::BridgeConstruction::default()).unwrap();
    assert!(!undeclared.contains("readCredentials"), "{undeclared}");
    for (policy, wire) in [(ValidateOnly, "validateOnly"), (PublicRead, "publicRead")] {
        let declared = serde_json::to_string(&construction::BridgeConstruction {
            read_credentials: policy,
            ..Default::default()
        })
        .unwrap();
        assert!(
            declared.ends_with(&format!(r#","readCredentials":"{wire}"}}"#)),
            "{declared}"
        );
    }
    // Families without a consumer declaration retain the Install policy.
    for venue in ["cryptocom", "krakenspot", "kraken", "weex"] {
        assert_eq!(
            construction::read_credentials_policy(venue),
            Install,
            "{venue}"
        );
    }
}

#[test]
fn one_validation_converts_a_validate_only_key_and_keeps_the_native_refusal() {
    let mut valid = options(r#"{"readCredentials":{"key":"read","secret":"value"}}"#);
    validate_read_key(ValidateOnly, &mut valid).unwrap();
    assert!(valid.read_credentials.is_none());
    assert_eq!(valid.read_credentials_valid, Some(true));
    let mut invalid_key = options(r#"{"readCredentials":{"key":"read"}}"#);
    let error = validate_read_key(ValidateOnly, &mut invalid_key).unwrap_err();
    assert_eq!(
        (error.message.as_str(), error.kind, error.outcome),
        (NATIVE, Some(ErrorKind::InvalidInput), None)
    );
    assert!(
        invalid_key.read_credentials.is_none(),
        "refused material kept"
    );
    let mut kept = options(r#"{"readCredentials":{"key":"read","secret":"value"}}"#);
    validate_read_key(Install, &mut kept).unwrap();
    assert!(kept.read_credentials.is_some() && kept.read_credentials_valid.is_none());
    let mut refused = options(r#"{"readCredentials":{"key":"read"}}"#);
    assert_eq!(
        validate_read_key(Install, &mut refused)
            .unwrap_err()
            .message,
        NATIVE
    );
    let mut doubled = options(
        r#"{"readCredentials":{"key":"read","secret":"value"},"readCredentialsValid":true}"#,
    );
    assert!(validate_read_key(ValidateOnly, &mut doubled).is_err());
    assert!(
        doubled.read_credentials.is_none(),
        "doubled input kept its material"
    );
    assert!(
        read_key_options(
            ValidateOnly,
            &options(r#"{"readCredentials":{"key":"read","secret":"value"}}"#)
        )
        .is_err()
    );
}

/// Wiring through the real entry points, with a test-only declaration.
#[cfg(feature = "weex")]
mod wiring {
    use super::*;
    use construction::test_policy;

    struct Built;
    impl Visit for Built {
        type Output = ();
        fn visit<V: safe_cex_engine::Venue + 'static>(self, _: safe_cex_engine::Engine<V>) {}
    }
    fn entropy() -> Option<safe_cex_engine::entropy::Entropy> {
        Some(safe_cex_engine::test_support::seeded_entropy(17))
    }
    fn build(json: &str) -> Result<(), VenueError> {
        build_with_signer("weex", options(json), entropy(), None, Built)
    }

    #[test]
    fn construction_keeps_the_native_refusal_and_refuses_stray_or_doubled_results() {
        let _declared = test_policy::declare("weex", ValidateOnly);
        let refused = build(r#"{"publicDataOnly":true,"readCredentialsValid":false}"#).unwrap_err();
        assert_eq!(refused.message, NATIVE);
        assert_eq!(
            (refused.kind, refused.outcome),
            (Some(ErrorKind::InvalidInput), None)
        );
        let raw = build(r#"{"publicDataOnly":true,"readCredentials":{"key":"read"}}"#).unwrap_err();
        assert_eq!(raw.message, NATIVE, "the raw refusal point is unchanged");
        assert!(build(r#"{"publicDataOnly":true,"readCredentials":{"key":"read","secret":"value"},"readCredentialsValid":true}"#).is_err());
        let mut injected_secondary = options(r#"{"publicDataOnly":true}"#);
        injected_secondary.signing = Some(injected(false, true));
        assert!(build_with_signer("weex", injected_secondary, entropy(), None, Built).is_err());
    }

    #[test]
    fn controls_construct() {
        build(r#"{"publicDataOnly":true}"#).unwrap();
        // Undeclared, WEEX's own arm refuses raw read material (dispatch custody).
        assert!(
            build(r#"{"publicDataOnly":true,"readCredentials":{"key":"read","secret":"value"}}"#)
                .is_err()
        );
        let _declared = test_policy::declare("weex", ValidateOnly);
        build(r#"{"publicDataOnly":true,"readCredentialsValid":true}"#).unwrap();
        // Declared, the raw key is converted to its result before the arm sees it.
        build(r#"{"publicDataOnly":true,"readCredentials":{"key":"read","secret":"value"}}"#)
            .unwrap();
    }

    #[test]
    fn an_undeclared_family_refuses_a_result_and_keeps_its_read_key() {
        assert_eq!(construction::read_credentials_policy("weex"), Install);
        assert!(build(r#"{"publicDataOnly":true,"readCredentialsValid":true}"#).is_err());
    }

    #[test]
    fn the_json_setup_refuses_a_read_identity_for_a_validate_only_family() {
        let setup = r#"{"primary":{"key":"k","secret":"s","passphrase":"p"},"secondary":{"key":"r","secret":"t"}}"#;
        assert!(local_signer_from_json_for_mode("weex", setup.into(), false).is_ok());
        let _declared = test_policy::declare("weex", ValidateOnly);
        let Err(error) = local_signer_from_json_for_mode("weex", setup.into(), false) else {
            panic!("a read identity was installed for a ValidateOnly family");
        };
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    }
}

#[test]
fn the_preflight_refuses_only_a_false_validate_only_result() {
    #[cfg(feature = "weex")]
    {
        let _declared = construction::test_policy::declare("weex", ValidateOnly);
        let error =
            read_key_preflight("weex", &options(r#"{"readCredentialsValid":false}"#)).unwrap_err();
        assert_eq!(
            (error.message.as_str(), error.kind, error.outcome),
            (NATIVE, Some(ErrorKind::InvalidInput), None)
        );
        assert!(read_key_preflight("weex", &options(r#"{"readCredentialsValid":true}"#)).is_ok());
    }
    // Undeclared families and raw keys are left to build_with_signer.
    assert!(read_key_preflight("weex", &options(r#"{"readCredentialsValid":false}"#)).is_ok());
    assert!(read_key_preflight("weex", &options(r#"{"readCredentials":{"key":"read"}}"#)).is_ok());
}

#[test]
fn backpack_history_declares_public_read_in_registry_and_bridge() {
    assert_eq!(
        construction::read_credentials_policy("backpack"),
        PublicRead
    );
    let bridge = construction::bridge("backpack", &options("{}"));
    assert_eq!(bridge.read_credentials, PublicRead);
    assert_eq!(
        serde_json::to_value(bridge).unwrap()["readCredentials"],
        "publicRead"
    );
}
