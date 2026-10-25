use safe_cex_engine::signing::{Identity, KeyRef};
use safe_cex_venue_bybit::Options;
#[test]
fn credential_options_contain_only_opaque_refs_and_debug_redacts_them() {
    let options = Options {
        credentials: Some(Identity::new(
            KeyRef("audit-opaque-reference".into()),
            Some(KeyRef("audit-signing-reference".into())),
        )),
        ..Options::default()
    };
    assert!(!format!("{options:?}").contains("audit-opaque-reference"));
    assert!(!format!("{options:?}").contains("audit-signing-reference"));
    assert_eq!(
        options.credentials.unwrap().credential.0,
        "audit-opaque-reference"
    );
}
