use safe_cex_engine::signing::{Identity, KeyRef};
use safe_cex_venue_bitget::Options;
#[test]
fn production_options_retain_only_opaque_identity_refs() {
    let options = Options {
        credentials: Some(
            Identity::new(
                KeyRef("ref:owned-credential".into()),
                Some(KeyRef("ref:owned-signing".into())),
            )
            .with_passphrase(Some(KeyRef("ref:owned-passphrase".into()))),
        ),
        ..Default::default()
    };
    let text = format!("{options:?}");
    assert!(!text.contains("test-secret-sentinel"));
    assert!(text.contains("ref:owned-signing"));
}
