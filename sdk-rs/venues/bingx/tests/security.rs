use safe_cex_engine::signing::{Identity, KeyRef};
use safe_cex_venue_bingx::Options;
#[test]
fn credential_options_hold_only_opaque_refs() {
    let options = Options {
        credentials: Some(Identity {
            credential: KeyRef("ref:owned-primary".into()),
            signing: KeyRef("ref:owned-signing".into()),
            passphrase: None,
            stable_id: None,
            extra: Default::default(),
        }),
        ..Default::default()
    };
    assert_eq!(
        options.credentials.as_ref().unwrap().credential.0,
        "ref:owned-primary"
    );
    assert!(!format!("{options:?}").contains("secret:"));
}
