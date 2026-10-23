use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_venue_weex::Options;

#[test]
fn venue_options_hold_only_opaque_identity_refs() {
    let sentinel = || Secret::new("audit-secret-sentinel".into());
    let (_provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: sentinel(),
            secret: sentinel(),
            passphrase: Some(sentinel()),
            ..Default::default()
        }),
        secondary: None,
    })
    .unwrap();
    let options = Options {
        credentials: identities.primary,
        ..Options::default()
    };
    assert!(options.credentials.is_some());
    assert!(!format!("{options:?}").contains("audit-secret-sentinel"));
}
