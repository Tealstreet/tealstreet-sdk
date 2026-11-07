use safe_cex_crypto::signer::LocalSigner;
use safe_cex_venue_okx::Options;

#[test]
fn venue_options_hold_only_opaque_provider_references() {
    let (_, identities) = LocalSigner::from_json(r#"{"primary":{"key":"audit-material-sentinel","secret":"audit-material-sentinel","passphrase":"audit-material-sentinel"}}"#.into()).unwrap();
    let options = Options {
        credentials: identities.primary,
        ..Default::default()
    };
    assert!(!format!("{options:?}").contains("audit-material-sentinel"));
    assert_eq!(
        options.credentials.unwrap().credential.0,
        "primary.credential"
    );
}
