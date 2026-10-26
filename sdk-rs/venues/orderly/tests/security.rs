//! The venue holds only opaque refs from trusted setup; no signing material.
mod support;
use support::*;

#[test]
fn options_hold_only_opaque_refs_from_trusted_setup() {
    let (signer, options) = trusted(options());
    assert!(signer.is_some(), "the provider holds the seed");
    let primary = options.identities.primary.as_ref().expect("Primary refs");
    let seed = bs58::encode(SEED).into_string();
    for text in [
        format!("{options:?}"),
        format!("{primary:?}"),
        format!("{:?}", primary.credential),
    ] {
        assert!(!text.contains(&seed), "no seed text: {text}");
    }
}
