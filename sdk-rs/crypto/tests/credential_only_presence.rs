//! Credential-only setup, phase 3 (docs/plans/RUST_CREDENTIAL_ONLY_SETUP.md,
//! D3 and D6): key presence, the typed login intent and the Optional
//! absence flag, all derived by trusted setup.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::CredentialSlot;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::signing::{
    CredentialSetupPolicy, Identities, Identity, KeyPresence, KeyRef, PrimaryLogin, SetupFacts,
    SignError,
};

fn optional() -> CredentialSetupPolicy {
    serde_json::from_str(r#"{"primary":{"signing":"optional"}}"#).unwrap()
}

fn facts(setup: &str) -> SetupFacts {
    let (_, _, facts) = LocalSigner::from_json_with_facts(setup.into(), optional(), false).unwrap();
    facts
}

/// C2: an omitted key and `"key": null` are Absent; `""` is Empty.
#[test]
fn c2_omitted_and_null_keys_are_absent_and_empty_is_empty() {
    for (setup, presence) in [
        (r#"{"primary":{}}"#, KeyPresence::Absent),
        (r#"{"primary":{"key":null}}"#, KeyPresence::Absent),
        (r#"{"primary":{"key":""}}"#, KeyPresence::Empty),
    ] {
        assert_eq!(facts(setup).primary_key(), presence, "{setup}");
    }
}

/// C3: presence is classified from the raw key with `str::trim` before any
/// trim policy: NEL is whitespace (Blank), BOM is not (Nonblank).
#[test]
fn c3_presence_matrix_classifies_the_raw_key() {
    for (key, presence) in [
        ("\"  \"", KeyPresence::Blank),
        ("\"\\u0085\"", KeyPresence::Blank),
        ("\"\\ufeff\"", KeyPresence::Nonblank),
        ("\" k \"", KeyPresence::Nonblank),
        ("\"k\"", KeyPresence::Nonblank),
    ] {
        let setup = format!(r#"{{"primary":{{"key":{key}}}}}"#);
        assert_eq!(facts(&setup).primary_key(), presence, "{key}");
    }
}

/// C9: the fact's Debug and serde forms carry only the variant name.
#[test]
fn c9_setup_facts_format_only_the_variant() {
    let facts = facts(r#"{"primary":{"key":"secret-looking-key"}}"#);
    assert_eq!(format!("{facts:?}"), "SetupFacts { primary_key: Nonblank }");
    assert_eq!(
        serde_json::to_string(&facts).unwrap(),
        r#"{"primaryKey":"nonblank"}"#
    );
}

/// C10: the login intent is derived from presence only; Empty selects the
/// primary for SessionAuth with its egress and carries no material.
#[test]
fn c10_login_intent_follows_presence_and_selects_primary() {
    assert_eq!(facts(r#"{"primary":{}}"#).primary_login(), None);
    assert_eq!(
        facts(r#"{"primary":{"key":""}}"#).primary_login(),
        Some(PrimaryLogin::Empty)
    );
    for key in ["\"  \"", "\"k\""] {
        let setup = format!(r#"{{"primary":{{"key":{key}}}}}"#);
        assert_eq!(facts(&setup).primary_login(), Some(PrimaryLogin::Installed));
    }
    let slots = Slots::new(Some(PrimaryLogin::Empty), None, SlotPolicy::default());
    let (selection, login) = slots.select(CredentialRole::SessionAuth).unwrap();
    assert_eq!(selection.slot(), CredentialSlot::Primary);
    assert!(selection.session_egress().is_some());
    assert_eq!(*login, PrimaryLogin::Empty);
}

/// C11r (b): the Optional absence is derived only by trusted local setup:
/// an Optional credential-only primary refuses `SigningNotConfigured`; the
/// same setup under Required stays `MissingKey`; a serialized copy never
/// carries the flag back.
#[test]
fn c11r_optional_absence_is_derived_by_trusted_setup_only() {
    let setup = r#"{"primary":{"key":"api-key"}}"#;
    let (_, ids, _) = LocalSigner::from_json_with_facts(setup.into(), optional(), false).unwrap();
    let flagged = ids.primary.unwrap();
    assert_eq!(flagged.signing_ref(), Err(SignError::SigningNotConfigured));
    let (_, ids, _) =
        LocalSigner::from_json_with_facts(setup.into(), CredentialSetupPolicy::default(), false)
            .unwrap();
    assert_eq!(
        ids.primary.unwrap().signing_ref(),
        Err(SignError::MissingKey)
    );
    let hop: Identity = serde_json::from_str(&serde_json::to_string(&flagged).unwrap()).unwrap();
    assert_eq!(hop.signing_ref(), Err(SignError::MissingKey));
    // Only an absent ref is flagged: an installed ref under Optional policy is
    // the same identity Required setup builds.
    let installed = r#"{"primary":{"key":"api-key","secret":"api-secret"}}"#;
    let (_, optional_ids, _) =
        LocalSigner::from_json_with_facts(installed.into(), optional(), false).unwrap();
    let (_, required_ids, _) = LocalSigner::from_json_with_facts(
        installed.into(),
        CredentialSetupPolicy::default(),
        false,
    )
    .unwrap();
    let optional_primary = optional_ids.primary.unwrap();
    assert!(optional_primary.signing_ref().is_ok());
    assert_eq!(optional_primary, required_ids.primary.unwrap());
}

/// C11j: JSON naming the flag cannot set it, and the flag takes part in
/// equality.
#[test]
fn c11j_flag_is_not_injectable_and_takes_part_in_equality() {
    for field in [
        "signingNotConfigured",
        "signing_not_configured",
        "optionalAbsent",
    ] {
        let json = format!(r#"{{"credential":"c","{field}":true}}"#);
        let parsed: Identity = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.signing_ref(), Err(SignError::MissingKey), "{field}");
    }
    let mut ids = Identities {
        primary: Some(Identity::new(KeyRef("c".into()), None)),
        secondary: None,
    };
    optional().apply_to_identities(&mut ids, false).unwrap();
    assert_ne!(
        ids.primary.unwrap(),
        Identity::new(KeyRef("c".into()), None)
    );
}

/// D3 pin (kuhwxi): moving presence into Secret must not start accepting
/// `null` for secret material. A null secret refuses through both entries;
/// a null passphrase stays "no passphrase", exactly as before D3.
#[test]
fn d3_null_secret_refuses_and_null_passphrase_is_none() {
    let null_secret = r#"{"primary":{"key":"k","secret":null}}"#;
    assert!(matches!(
        LocalSigner::from_json_with_facts(null_secret.into(), optional(), false),
        Err(SignError::InvalidRequest)
    ));
    assert!(matches!(
        LocalSigner::from_json_with_policy_and_mode(
            null_secret.into(),
            CredentialSetupPolicy::default(),
            false
        ),
        Err(SignError::InvalidRequest)
    ));
    let null_passphrase = r#"{"primary":{"key":"k","secret":"s","passphrase":null}}"#;
    let (_, ids, _) =
        LocalSigner::from_json_with_facts(null_passphrase.into(), optional(), false).unwrap();
    assert_eq!(ids.primary.unwrap().passphrase, None);
}
