//! Backpack's public-only read-key-only provider seam
//! (docs/plans/RUST_BACKPACK_PUBLIC_READ_PROVIDER.md). The `backpack` row is not
//! declared yet, so these declare PublicRead through the test-only seam.
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
/// The provider holds material for exactly the identities it reports.
fn installed(setup: &BackpackSignerSetup) -> bool {
    setup.provider.is_some()
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
        assert!(installed(&setup), "no provider for the read identity");
        assert!(!read.signing.0.is_empty());
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
    for (seed, expected) in [
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
