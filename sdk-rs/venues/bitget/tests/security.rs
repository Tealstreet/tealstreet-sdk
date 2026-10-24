use safe_cex_engine::signing::{Identity, KeyRef};
use safe_cex_venue_bitget::Options;
#[test]
fn production_options_retain_only_opaque_identity_refs() {
    let options = Options {
        credentials: Some(Identity {
            credential: KeyRef("ref:owned-credential".into()),
            signing: KeyRef("ref:owned-signing".into()),
            passphrase: Some(KeyRef("ref:owned-passphrase".into())),
            stable_id: None,
            extra: Default::default(),
        }),
        ..Default::default()
    };
    let text = format!("{options:?}");
    assert!(!text.contains("test-secret-sentinel"));
    assert!(text.contains("ref:owned-signing"));
}
