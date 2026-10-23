use safe_cex_engine::signing::{Identity, KeyRef};
use safe_cex_venue_bybit::Options;
#[test]
fn credential_options_contain_only_opaque_refs_and_debug_redacts_them() {
    let options = Options {
        credentials: Some(Identity {
            credential: KeyRef("audit-opaque-reference".into()),
            signing: KeyRef("audit-signing-reference".into()),
            passphrase: None,
            stable_id: None,
            extra: Default::default(),
        }),
        ..Options::default()
    };
    assert!(!format!("{options:?}").contains("audit-opaque-reference"));
    assert!(!format!("{options:?}").contains("audit-signing-reference"));
    assert_eq!(
        options.credentials.unwrap().credential.0,
        "audit-opaque-reference"
    );
}
