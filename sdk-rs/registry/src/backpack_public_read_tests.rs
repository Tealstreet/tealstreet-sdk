//! Backpack's public-only read-key-only provider seam
//! (docs/plans/RUST_BACKPACK_PUBLIC_READ_PROVIDER.md). Policy overrides keep
//! the undeclared and PublicRead contracts independent of venue adoption.
use super::construction::ReadCredentialsPolicy::PublicRead;
use super::construction::test_policy;
use super::*;
use safe_cex_engine::credential_selection::{
    CredentialRole, CredentialUnavailable, PrimaryReason, SlotPolicy, Slots,
};

const PRIMARY_SEED: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=";
const READ_SEED: &str = "CQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQk=";
const SHORT_SEED: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBw==";

fn options(public: bool, primary: bool, read: Option<&str>) -> EngineOptions {
    let primary = if primary {
        format!(r#","key":"primary-key","secret":"{PRIMARY_SEED}""#)
    } else {
        String::new()
    };
    let read = read.map_or(String::new(), |seed| {
        format!(r#","readCredentials":{{"key":"read-key","secret":"{seed}"}}"#)
    });
    serde_json::from_str(&format!(r#"{{"publicDataOnly":{public}{primary}{read}}}"#)).unwrap()
}
fn setup(public: bool, primary: bool, read: Option<&str>) -> BackpackSignerSetup {
    backpack_provider(&options(public, primary, read)).unwrap()
}
fn installed(setup: &BackpackSignerSetup) -> bool {
    setup.provider.is_some()
}
/// A real signature from the installed provider under `key`, inline.
fn signature(setup: &BackpackSignerSetup, key: &safe_cex_engine::signing::KeyRef) -> Vec<u8> {
    let request = safe_cex_engine::signing::SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![safe_cex_engine::signing::Operation::Sign {
            algorithm: safe_cex_engine::signing::Algorithm::Ed25519,
            key: key.clone(),
            payload: vec![safe_cex_engine::signing::PayloadPart::Bytes {
                bytes: b"instruction=fillHistoryQueryAll".to_vec(),
            }],
        }],
    };
    let provider = setup.provider.as_ref().expect("a provider");
    let result = provider
        .sign_ready(&request)
        .expect("the local provider signs inline")
        .expect("the read identity signs");
    result.values[0].clone()
}

/// RED: a valid saved read key gets a read-only provider in public-only mode.
#[test]
fn public_only_installs_a_provider_holding_only_the_valid_read_identity() {
    let _declared = test_policy::declare("backpack", PublicRead);
    for primary in [false, true] {
        let setup = setup(true, primary, Some(READ_SEED));
        assert!(
            setup.identities.primary.is_none(),
            "a primary was installed in public-only mode"
        );
        let read = setup
            .identities
            .secondary
            .as_ref()
            .expect("the read identity");
        // Independent oracle: the same Ed25519 signature over the read seed.
        let expected =
            safe_cex_crypto::ed25519::sign(b"instruction=fillHistoryQueryAll", &[9u8; 32]).unwrap();
        assert_eq!(signature(&setup, &read.signing), expected.to_vec());
        assert!(matches!(setup.read_status, Some(Ok(()))));
    }
}

/// RED: the provider's identities give Read roles the read key and nothing else.
#[test]
fn public_only_selection_serves_reads_only() {
    let _declared = test_policy::declare("backpack", PublicRead);
    let setup = setup(true, true, Some(READ_SEED));
    let slots = Slots::new(
        setup.identities.primary.clone(),
        setup.identities.secondary.clone(),
        SlotPolicy::default(),
    );
    let (selection, _) = slots
        .select(CredentialRole::Read { scope: None })
        .expect("Read selects the read key");
    assert_eq!(selection.slot(), safe_cex_engine::CredentialSlot::Secondary);
    for role in [
        CredentialRole::Mutation,
        CredentialRole::SessionAuth,
        CredentialRole::PrimaryRead {
            reason: PrimaryReason::History,
            scope: None,
        },
    ] {
        assert_eq!(
            slots.select(role).err(),
            Some(CredentialUnavailable::Missing),
            "{role:?}"
        );
    }
}

/// Control: an invalid read key installs nothing and keeps #2073's typed status.
#[test]
fn public_only_with_an_invalid_read_key_installs_nothing_and_keeps_the_typed_status() {
    let _declared = test_policy::declare("backpack", PublicRead);
    let configuration = "Secondary read credentials require a nonempty key and secret";
    let empty_key: EngineOptions = serde_json::from_str(&format!(
        r#"{{"publicDataOnly":true,"readCredentials":{{"key":"","secret":"{READ_SEED}"}}}}"#
    ))
    .unwrap();
    let empty = backpack_provider(&empty_key).unwrap();
    assert!(empty.provider.is_none());
    let Some(Err(error)) = empty.read_status else {
        panic!("no typed status for an empty key");
    };
    assert_eq!(
        error.message(),
        configuration,
        "Configuration precedes Seed"
    );
    assert!(
        setup(true, true, None).read_status.is_none(),
        "absent read key"
    );
    for (seed, expected) in [
        ("", configuration),
        ("not base64!", "Invalid Backpack base64 seed"),
        (SHORT_SEED, "Ed25519 seed must be 32 bytes"),
    ] {
        let setup = setup(true, true, Some(seed));
        assert!(setup.provider.is_none() && setup.identities.secondary.is_none());
        assert!(setup.identities.primary.is_none());
        let Some(Err(error)) = setup.read_status else {
            panic!("no typed status for {seed}");
        };
        assert_eq!(error.message(), expected);
    }
}

/// Control: undeclared, public-only installs nothing, exactly as today.
#[test]
fn undeclared_public_only_is_unchanged() {
    let _undeclared =
        test_policy::declare("backpack", construction::ReadCredentialsPolicy::Install);
    assert_eq!(
        construction::read_credentials_policy("backpack"),
        construction::ReadCredentialsPolicy::Install
    );
    let setup = setup(true, true, Some(READ_SEED));
    assert!(setup.provider.is_none());
    assert!(setup.identities.primary.is_none() && setup.identities.secondary.is_none());
}

/// Control: private mode is unchanged, and a bad read key keeps the primary.
#[test]
fn private_mode_is_unchanged_with_the_declaration() {
    let _declared = test_policy::declare("backpack", PublicRead);
    let dual = setup(false, true, Some(READ_SEED));
    assert!(dual.identities.primary.is_some() && dual.identities.secondary.is_some());
    let bad_read = setup(false, true, Some(SHORT_SEED));
    assert!(bad_read.identities.primary.is_some() && bad_read.identities.secondary.is_none());
    assert!(installed(&bad_read));
}

/// The primary is never built or probed in public-only mode, even malformed.
#[test]
fn public_only_never_decodes_a_primary() {
    let _declared = test_policy::declare("backpack", PublicRead);
    let malformed: EngineOptions = serde_json::from_str(&format!(
        r#"{{"publicDataOnly":true,"key":"primary-key","secret":"not base64!","readCredentials":{{"key":"read-key","secret":"{READ_SEED}"}}}}"#
    ))
    .unwrap();
    let setup = backpack_provider(&malformed).expect("a malformed primary was decoded");
    assert!(setup.identities.primary.is_none() && setup.identities.secondary.is_some());
}

/// Guard, not proof: hosted admission ignores options and public-only startup
/// queues only the unsigned markets read, so this also passed before the seam.
/// It pins that installing the read-only provider does not change either.
#[test]
fn public_only_read_provider_grants_no_hosted_or_startup_authority() {
    let _declared = test_policy::declare("backpack", PublicRead);
    assert!(!hosted_signing::admitted(
        "backpack",
        &options(true, true, Some(READ_SEED))
    ));
    struct Started;
    impl Visit for Started {
        type Output = Vec<safe_cex_engine::Action>;
        fn visit<V: safe_cex_engine::Venue + 'static>(
            self,
            mut engine: safe_cex_engine::Engine<V>,
        ) -> Self::Output {
            engine.start(0.0);
            safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        }
    }
    let actions = build(
        "backpack",
        options(true, true, Some(READ_SEED)),
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        Started,
    )
    .unwrap();
    assert!(
        !actions.iter().any(|action| matches!(
            action,
            safe_cex_engine::Action::Sign { .. } | safe_cex_engine::Action::SignedOutbound { .. }
        )),
        "public-only startup produced signing work"
    );
}
