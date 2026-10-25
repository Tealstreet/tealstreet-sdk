//! Credential-only trusted setup (docs/plans/RUST_CREDENTIAL_ONLY_SETUP.md).
//! These go through the JSON surfaces so they compile against today's types.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{CredentialSetupPolicy, Identity, IdentitySetupPolicy};

fn policy(json: &str) -> CredentialSetupPolicy {
    serde_json::from_str(json).unwrap()
}

/// C1: a key-only setup installs an explicit absence, not an empty signing ref.
#[test]
fn c1_key_only_setup_installs_no_signing_ref() {
    let (_, identities) = LocalSigner::from_json_with_policy_and_mode(
        r#"{"primary":{"key":"api-key"}}"#.into(),
        CredentialSetupPolicy::default(),
        false,
    )
    .unwrap();
    let primary = identities.primary.unwrap();
    assert!(primary.validate().is_err());
    let json = serde_json::to_value(&primary).unwrap();
    assert_eq!(json["credential"], "primary.credential");
    assert!(
        json.get("signing").is_none(),
        "absent, not {}",
        json["signing"]
    );
}

/// C6: an Identity without `signing` parses as absent and serializes without
/// it; an explicit empty ref still parses and stays invalid.
#[test]
fn c6_identity_json_signing_is_optional_and_empty_stays_invalid() {
    let absent: Identity = serde_json::from_str(r#"{"credential":"c"}"#).unwrap();
    assert!(absent.validate().is_err());
    assert_eq!(
        serde_json::to_string(&absent).unwrap(),
        r#"{"credential":"c"}"#
    );
    let empty: Identity = serde_json::from_str(r#"{"credential":"c","signing":""}"#).unwrap();
    assert!(empty.validate().is_err());
}

/// C7 (policy half): the declared signing policy parses; Required is the
/// default and is never serialized.
#[test]
fn c7_signing_policy_declares_optional_and_defaults_to_required() {
    let optional = policy(r#"{"primary":{"signing":"optional"}}"#);
    assert_ne!(optional, CredentialSetupPolicy::default());
    assert_eq!(
        policy(r#"{"primary":{"signing":"required"}}"#).primary,
        Some(IdentitySetupPolicy::default())
    );
    assert!(
        serde_json::from_str::<CredentialSetupPolicy>(r#"{"primary":{"signing":"maybe"}}"#)
            .is_err()
    );
}

/// K6: every existing policy shape serializes exactly as before.
#[test]
fn k6_existing_setup_policies_serialize_unchanged() {
    for (input, expected) in [
        ("{}", r#"{"primary":null,"secondary":null}"#),
        (
            r#"{"primary":{"trimCredential":true,"passphrase":"requiredForPrivate"}}"#,
            r#"{"primary":{"trimCredential":true,"passphrase":"requiredForPrivate","secretEncoding":null,"extra":null,"signingAlgorithm":null},"secondary":null}"#,
        ),
        (
            r#"{"primary":{"secretEncoding":"hex","signingAlgorithm":"secp256k1Digest"}}"#,
            r#"{"primary":{"trimCredential":null,"passphrase":null,"secretEncoding":"hex","extra":null,"signingAlgorithm":"secp256k1Digest"},"secondary":null}"#,
        ),
    ] {
        assert_eq!(
            serde_json::to_string(&policy(input)).unwrap(),
            expected,
            "{input}"
        );
    }
}

/// C5 (control): selection is generic over slot material, so a credential-only
/// primary is chosen for Mutation and SessionAuth like any other primary.
#[test]
fn c5_credential_only_primary_selects_for_mutation_and_session_auth() {
    use safe_cex_engine::CredentialSlot;
    use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
    let (_, identities) = LocalSigner::from_json_with_policy_and_mode(
        r#"{"primary":{"key":"api-key"}}"#.into(),
        CredentialSetupPolicy::default(),
        false,
    )
    .unwrap();
    let slots = Slots::new(identities.primary, None, SlotPolicy::default());
    for role in [CredentialRole::Mutation, CredentialRole::SessionAuth] {
        let (selection, identity) = slots.select(role).unwrap();
        assert_eq!(selection.slot(), CredentialSlot::Primary);
        assert_eq!(identity.credential.0, "primary.credential");
        assert_eq!(
            selection.session_egress().is_some(),
            role == CredentialRole::SessionAuth
        );
    }
}
