use super::*;
use safe_cex_engine::signing::{
    CredentialSetupPolicy, IdentitySetupPolicy, KeyPresence, PrimaryLogin, SigningPolicy,
};

#[test]
fn setup_facts_cannot_be_injected_through_options_json() {
    for name in ["setupFacts", "setup_facts", "SetupFacts"] {
        let options: EngineOptions =
            serde_json::from_str(&format!(r#"{{"{name}":{{"primaryKey":"empty"}}}}"#,)).unwrap();
        assert_eq!(options.setup_facts, None);
    }
}

#[test]
fn raw_trusted_setup_preserves_missing_empty_blank_and_nonblank_keys() {
    let policy = CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            signing: SigningPolicy::Optional,
            trim_credential: Some(false),
            ..Default::default()
        }),
        secondary: None,
    };
    for (input, presence, login, credential_installed) in [
        ("{}", KeyPresence::Absent, None, false),
        (r#"{"secret":"scalar"}"#, KeyPresence::Absent, None, false),
        (
            r#"{"key":null,"secret":"scalar"}"#,
            KeyPresence::Absent,
            None,
            false,
        ),
        (
            r#"{"key":""}"#,
            KeyPresence::Empty,
            Some(PrimaryLogin::Empty),
            false,
        ),
        (
            r#"{"key":" \t "}"#,
            KeyPresence::Blank,
            Some(PrimaryLogin::Installed),
            true,
        ),
        (
            r#"{"key":"api-key"}"#,
            KeyPresence::Nonblank,
            Some(PrimaryLogin::Installed),
            true,
        ),
    ] {
        let mut options: EngineOptions = serde_json::from_str(input).unwrap();
        let (_, identities) = prepare_local_signer_with_policy(
            &mut options,
            policy,
            false,
            construction::ReadCredentialsPolicy::Install,
        )
        .unwrap();
        let facts = options.setup_facts.expect("trusted setup records facts");
        assert_eq!(facts.primary_key(), presence);
        assert_eq!(facts.primary_login(), login);
        assert_eq!(
            identities.primary.as_ref().is_some_and(|identity| {
                identity.credential == safe_cex_engine::signing::KeyRef("primary.credential".into())
            }),
            credential_installed
        );
        assert!(options.key.is_none());
        assert!(options.secret.is_none());
    }
}

#[test]
fn json_trusted_setup_records_facts_before_venue_construction() {
    let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
    let (_, identities) = local_signer_from_json_for_mode(
        "provider-fixture",
        r#"{"primary":{"key":"api-key","secret":"scalar"}}"#.into(),
        &mut options,
    )
    .unwrap();
    assert_eq!(
        options.setup_facts.unwrap().primary_key(),
        KeyPresence::Nonblank
    );
    assert_eq!(
        options.setup_facts.unwrap().primary_login(),
        Some(PrimaryLogin::Installed)
    );
    assert!(identities.primary.unwrap().signing_ref().is_ok());
}
