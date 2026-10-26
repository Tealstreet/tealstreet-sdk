//! `PrimarySource::ScalarOnly`: trusted local setup installs a signing-only
//! primary whose credential is explicitly absent (no fake KeyRef or API key).
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{
    CredentialSetupPolicy, Identities, Identity, KeyPresence, KeyRef, SetupFacts, SignError,
};

/// Arbitrary in-range scalar; never a vector or real key.
const SCALAR: &str = "1111111111111111111111111111111111111111111111111111111111111111";

fn scalar_only() -> CredentialSetupPolicy {
    serde_json::from_str(
        r#"{"primary":{"trimCredential":true,"secretEncoding":"hex",
            "signingAlgorithm":"secp256k1Digest","source":"scalarOnly"}}"#,
    )
    .unwrap()
}

fn setup(
    policy: CredentialSetupPolicy,
    primary: serde_json::Value,
) -> Result<(LocalSigner, Identities, SetupFacts), SignError> {
    LocalSigner::from_json_with_facts(
        serde_json::json!({ "primary": primary }).to_string(),
        policy,
        false,
    )
}

#[test]
fn absent_empty_and_blank_keys_install_an_absent_credential_with_original_facts() {
    for (primary, presence) in [
        (serde_json::json!({"secret": SCALAR}), KeyPresence::Absent),
        (
            serde_json::json!({"key": null, "secret": SCALAR}),
            KeyPresence::Absent,
        ),
        (
            serde_json::json!({"key": "", "secret": SCALAR}),
            KeyPresence::Empty,
        ),
        (
            serde_json::json!({"key": " \t", "secret": SCALAR}),
            KeyPresence::Blank,
        ),
    ] {
        let (_, ids, facts) = setup(scalar_only(), primary).unwrap();
        assert_eq!(facts.primary_key(), presence);
        let identity = ids.primary.unwrap();
        assert!(identity.credential_absent());
        assert_eq!(identity.credential_ref(), None);
        assert_eq!(identity.validate(), Ok(()));
        assert!(identity.signing_ref().is_ok());
        // Nothing is presentable: no ref matches the absent credential.
        assert!(!identity.owns(&identity.credential));
        assert!(!identity.owns(&KeyRef("primary.credential".into())));
    }
}

#[test]
fn a_nonblank_key_takes_the_ordinary_installed_path() {
    let (_, ids, facts) = setup(
        scalar_only(),
        serde_json::json!({"key": "api", "secret": SCALAR}),
    )
    .unwrap();
    assert_eq!(facts.primary_key(), KeyPresence::Nonblank);
    let identity = ids.primary.unwrap();
    assert!(!identity.credential_absent());
    assert_eq!(identity.credential, KeyRef("primary.credential".into()));
    assert_eq!(identity.credential_ref(), Some(&identity.credential));
    assert_eq!(identity.validate(), Ok(()));
}

#[test]
fn every_other_policy_keeps_todays_refusal_and_a_missing_scalar_is_never_absent() {
    let (_, ids, _) = setup(
        CredentialSetupPolicy::default(),
        serde_json::json!({"secret": SCALAR}),
    )
    .unwrap();
    let identity = ids.primary.unwrap();
    assert!(!identity.credential_absent() && identity.validate().is_err());
    // An empty ordinary credential is never a presentable ref.
    assert_eq!(identity.credential_ref(), None);

    let (_, ids, _) = setup(scalar_only(), serde_json::json!({"key": ""})).unwrap();
    let identity = ids.primary.unwrap();
    assert!(!identity.credential_absent() && identity.validate().is_err());
}

#[test]
fn scalar_only_is_primary_secp256k1_and_required_signing_only() {
    for policy in [
        r#"{"primary":{"source":"scalarOnly"}}"#,
        r#"{"primary":{"secretEncoding":"hex","signingAlgorithm":"ed25519","source":"scalarOnly"}}"#,
        r#"{"primary":{"secretEncoding":"hex","signingAlgorithm":"secp256k1Digest",
            "signing":"optional","source":"scalarOnly"}}"#,
        r#"{"primary":{"secretEncoding":"hex","signingAlgorithm":"secp256k1Digest"},
            "secondary":{"source":"scalarOnly"}}"#,
    ] {
        let policy: CredentialSetupPolicy = serde_json::from_str(policy).unwrap();
        assert!(matches!(
            setup(policy, serde_json::json!({"secret": SCALAR})),
            Err(SignError::InvalidRequest)
        ));
    }
}

#[test]
fn injected_identities_can_never_be_credential_absent() {
    let injected: Identity = serde_json::from_str(
        r#"{"credential":"","signing":"primary.signing","credentialAbsent":true}"#,
    )
    .unwrap();
    assert!(!injected.credential_absent() && injected.validate().is_err());
    // The policy alone never marks an identity; only trusted local setup does.
    let mut ids = Identities {
        primary: Some(injected),
        secondary: None,
    };
    let _ = scalar_only().apply_to_identities(&mut ids, false);
    let primary = ids.primary.unwrap();
    assert!(!primary.credential_absent() && primary.validate().is_err());
    // A transported copy of a trusted absent identity loses the marker.
    let (_, ids, _) = setup(scalar_only(), serde_json::json!({"secret": SCALAR})).unwrap();
    let json = serde_json::to_string(ids.primary.as_ref().unwrap()).unwrap();
    let copy: Identity = serde_json::from_str(&json).unwrap();
    assert!(!copy.credential_absent() && copy.validate().is_err());
}

#[test]
fn the_default_policy_shape_serializes_without_a_source() {
    let policy: CredentialSetupPolicy =
        serde_json::from_str(r#"{"primary":{"source":"required"}}"#).unwrap();
    assert_eq!(
        serde_json::to_string(&policy).unwrap(),
        r#"{"primary":{"trimCredential":null,"passphrase":null,"secretEncoding":null,"extra":null,"signingAlgorithm":null},"secondary":null}"#
    );
}

#[cfg(feature = "secp256k1")]
#[test]
fn the_absent_primary_still_answers_its_declared_public_address() {
    let (signer, ids, _) = setup(scalar_only(), serde_json::json!({"secret": SCALAR})).unwrap();
    let key = ids.primary.unwrap().signing_ref().unwrap().clone();
    let request = serde_json::from_value(serde_json::json!({
        "receipt": {}, "context": {"venue": "", "method": "", "host": "", "path": ""},
        "operations": [{"kind": "publicAddress", "key": key}],
    }))
    .unwrap();
    assert_eq!(signer.sign_now(&request).unwrap().values[0].len(), 20);
    // Presenting the absent credential is refused by the provider.
    let present = serde_json::from_value(serde_json::json!({
        "receipt": {}, "context": {"venue": "", "method": "", "host": "", "path": ""},
        "operations": [{"kind": "present", "key": ""}],
    }))
    .unwrap();
    assert!(signer.sign_now(&present).is_err());
}

fn operations(signer: &LocalSigner, operations: serde_json::Value) -> Result<usize, SignError> {
    let request = serde_json::from_value(serde_json::json!({
        "receipt": {}, "context": {"venue": "", "method": "", "host": "", "path": ""},
        "operations": operations,
    }))
    .unwrap();
    signer.sign_now(&request).map(|result| result.values.len())
}

/// A declared asymmetric key is never emitted, whichever identity names it;
/// ordinary credential presentation and MAC interpolation are unchanged.
#[test]
fn declared_asymmetric_material_is_never_presented_or_interpolated() {
    for policy in [
        scalar_only(),
        serde_json::from_str(
            r#"{"primary":{"secretEncoding":"hex","signingAlgorithm":"secp256k1Digest"}}"#,
        )
        .unwrap(),
    ] {
        let (signer, ids, _) =
            setup(policy, serde_json::json!({"key": "api", "secret": SCALAR})).unwrap();
        let identity = ids.primary.unwrap();
        let signing = identity.signing_ref().unwrap().clone();
        for refused in [
            serde_json::json!([{"kind": "present", "key": signing}]),
            serde_json::json!([{"kind": "sha256", "payload": [{"kind": "credential", "key": signing}]}]),
            serde_json::json!([{"kind": "sign", "algorithm": "secp256k1Digest", "key": signing,
                "payload": [{"kind": "credential", "key": signing}]}]),
        ] {
            assert_eq!(operations(&signer, refused), Err(SignError::InvalidRequest));
        }
        let credential = identity.credential.clone();
        assert_eq!(
            operations(
                &signer,
                serde_json::json!([{"kind": "present", "key": credential}])
            ),
            Ok(1)
        );
        assert_eq!(
            operations(
                &signer,
                serde_json::json!([{"kind": "sha256",
                "payload": [{"kind": "credential", "key": credential}]}])
            ),
            Ok(1)
        );
    }
    // An undeclared (MAC) secret keeps today's interpolation.
    let (signer, ids, _) = setup(
        CredentialSetupPolicy::default(),
        serde_json::json!({"key": "api", "secret": "mac"}),
    )
    .unwrap();
    let signing = ids.primary.unwrap().signing_ref().unwrap().clone();
    assert_eq!(
        operations(
            &signer,
            serde_json::json!([{"kind": "sha256",
            "payload": [{"kind": "credential", "key": signing}]}])
        ),
        Ok(1)
    );
}
