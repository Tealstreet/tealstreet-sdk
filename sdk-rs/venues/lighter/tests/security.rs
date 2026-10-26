use safe_cex_engine::signing::{Identity, KeyRef};
use safe_cex_venue_lighter::Options;

#[test]
fn venue_options_retain_only_public_scope_and_redact_opaque_provider_references() {
    let options = Options { identity: Some(Identity::new(KeyRef("audit-reference-sentinel".into()), Some(KeyRef("provider-signing-reference".into())))), api_key_index: Some("1".into()), ..Options::default() };
    assert!(!format!("{options:?}").contains("audit-reference-sentinel"));
    assert_eq!(options.api_key_index.as_deref(), Some("1"));
}
