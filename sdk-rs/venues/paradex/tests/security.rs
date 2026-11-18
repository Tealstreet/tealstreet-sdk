//! Trusted setup leaves the venue only refs and public account facts.
mod support;
use safe_cex_venue_paradex::Options;

#[test]
fn installed_options_hold_refs_and_public_facts_only() {
    for key in [support::STARK_KEY, &"11".repeat(32)] {
        let (options, _, _) = support::keyed(Options::default(), key);
        let identity = options.credentials.as_ref().unwrap();
        assert!(identity.credential_absent());
        assert!(identity.credential_ref().is_none());
        let shown = format!("{options:?} {identity:?}");
        assert!(!shown.contains(key.trim_start_matches("0x")));
    }
}
