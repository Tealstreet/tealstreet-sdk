use safe_cex_crypto::secrets::EraseSecrets;
use safe_cex_venue_apexomni::{Options, sign::LegacyCredentials};

/// Venue options hold refs only; the legacy oracle's credentials stay redacted
/// and erasable.
#[test]
fn credential_options_debug_redacts_and_erasure_scrubs_owned_fields() {
    assert!(!format!("{:?}", Options::default()).contains("audit-secret-sentinel"));
    let mut legacy = LegacyCredentials {
        key: Some("audit-secret-sentinel".into()),
        secret: Some("audit-secret-sentinel".into()),
        passphrase: Some("audit-secret-sentinel".into()),
    };
    assert!(!format!("{legacy:?}").contains("audit-secret-sentinel"));
    legacy.erase_secrets();
    assert!(legacy.key.as_ref().is_none_or(String::is_empty));
    assert!(legacy.secret.as_ref().is_none_or(String::is_empty));
    assert!(legacy.passphrase.as_ref().is_none_or(String::is_empty));
}
