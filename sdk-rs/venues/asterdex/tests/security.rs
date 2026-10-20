use safe_cex_crypto::secrets::EraseSecrets;
use safe_cex_venue_asterdex::Options;

#[test]
fn credential_options_debug_redacts_and_erasure_scrubs_owned_fields() {
    let mut options = Options {
        key: Some("audit-secret-sentinel".into()),
        secret: Some("audit-secret-sentinel".into()),
        auth_private_key: Some("audit-secret-sentinel".into()),
        read_key: Some("audit-secret-sentinel".into()),
        read_secret: Some("audit-secret-sentinel".into()),
        ..Options::default()
    };
    assert!(!format!("{options:?}").contains("audit-secret-sentinel"));
    options.erase_secrets();
    assert!(options.key.as_ref().is_none_or(String::is_empty));
    assert!(options.secret.as_ref().is_none_or(String::is_empty));
    assert!(
        options
            .auth_private_key
            .as_ref()
            .is_none_or(String::is_empty)
    );
    assert!(options.read_key.as_ref().is_none_or(String::is_empty));
    assert!(options.read_secret.as_ref().is_none_or(String::is_empty));
}
