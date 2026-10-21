use safe_cex_crypto::secrets::EraseSecrets;
use safe_cex_venue_polymarket::Options;

#[test]
fn credential_options_debug_redacts_and_erasure_scrubs_owned_fields() {
    let mut options = Options {
        key: Some("audit-secret-sentinel".into()),
        ..Options::default()
    };
    assert!(!format!("{options:?}").contains("audit-secret-sentinel"));
    options.erase_secrets();
    assert!(options.key.as_ref().is_none_or(String::is_empty));
}

#[test]
fn nested_api_credentials_never_appear_in_debug_and_are_erased() {
    use safe_cex_venue_polymarket::PolymarketOptions;
    let nested = PolymarketOptions {
        api_key: Some("private-api-key".into()),
        api_secret: Some("private-api-secret".into()),
        api_passphrase: Some("private-api-passphrase".into()),
        ..PolymarketOptions::default()
    };
    let mut options = Options {
        polymarket: Some(nested),
        ..Options::default()
    };
    assert!(!format!("{options:?}").contains("private-api"));
    assert!(!format!("{:?}", options.polymarket).contains("private-api"));
    options.erase_secrets();
    let nested = options.polymarket.unwrap();
    assert!(
        nested.api_key.is_none() && nested.api_secret.is_none() && nested.api_passphrase.is_none()
    );
}
