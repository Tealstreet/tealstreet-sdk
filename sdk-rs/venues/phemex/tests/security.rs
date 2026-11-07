mod support;
use safe_cex_crypto::secrets::EraseSecrets;
use support::Options;

#[test]
fn credential_options_debug_redacts_and_erasure_scrubs_owned_fields() {
    let mut options = Options {
        key: Some("audit-secret-sentinel".into()),
        secret: Some("audit-secret-sentinel".into()),
        ..Options::default()
    };
    assert!(!format!("{options:?}").contains("audit-secret-sentinel"));
    options.erase_secrets();
    assert!(options.key.as_ref().is_none_or(String::is_empty));
    assert!(options.secret.as_ref().is_none_or(String::is_empty));
}

#[test]
fn trusted_provider_setup_leaves_only_opaque_refs_in_native_options() {
    let (_local, refs) =
        safe_cex_crypto::signer::LocalSigner::from_setup(safe_cex_crypto::signer::LocalSetup {
            primary: Some(safe_cex_crypto::signer::LocalIdentity {
                key: safe_cex_crypto::signer::Secret::new("audit-key-sentinel".into()),
                secret: safe_cex_crypto::signer::Secret::new("audit-secret-sentinel".into()),
                ..Default::default()
            }),
            ..Default::default()
        })
        .unwrap();
    let options = safe_cex_venue_phemex::Options {
        credentials: refs.primary,
        ..Default::default()
    };
    let debug = format!("{options:?}");
    assert!(!debug.contains("audit-key-sentinel"));
    assert!(!debug.contains("audit-secret-sentinel"));
    assert!(options.credentials.as_ref().unwrap().validate().is_ok());
}
