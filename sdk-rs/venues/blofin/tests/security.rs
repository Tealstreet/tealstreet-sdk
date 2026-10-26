//! Options carry only opaque refs: configured strings reach trusted setup,
//! never the venue.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_venue_blofin::Options;

#[test]
fn options_hold_only_opaque_refs_from_trusted_setup() {
    let sentinel = "audit-sentinel-material";
    let (_, identities) = LocalSigner::from_json(format!(
        r#"{{"primary":{{"key":"{sentinel}","secret":"{sentinel}","passphrase":"{sentinel}"}}}}"#
    ))
    .unwrap();
    let options = Options {
        identities,
        ..Options::default()
    };
    assert!(options.identities.primary.is_some());
    assert!(!format!("{options:?}").contains(sentinel));
}
