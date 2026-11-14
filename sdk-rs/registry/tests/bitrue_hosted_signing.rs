#![cfg(all(feature = "bitrue", feature = "hosted-signing"))]

use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_venues::hosted_signing::{self, Input};

#[test]
fn bitrue_device_capture_matches_independent_http_and_listen_key_vectors() {
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("fixture-bitrue-public".into()),
            secret: Secret::new("fixture-bitrue-signing-material".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../venues/bitrue/tests/fixtures/hosted-signing.json"
    ))
    .unwrap();
    for vector in vectors.as_array().unwrap() {
        let (work, kind) = hosted_signing::prepare(
            "bitrue",
            &identities,
            Input::Http {
                payload: vector["payload"].as_str().unwrap().into(),
                stamp_ms: 1_700_000_000_123.,
                revision: "0".into(),
            },
        )
        .expect("Bitrue must admit its hosted HMAC codec");
        let result = provider.sign_now(&work).unwrap();
        work.validate_result(&result).unwrap();
        let output = hosted_signing::finish(kind, &result).unwrap();
        assert_eq!(output.credential.as_deref(), Some("fixture-bitrue-public"));
        assert_eq!(output.signature.as_deref(), vector["signature"].as_str());
    }
    let policy = safe_cex_venues::remote_signing_policy("bitrue");
    assert!(policy.enabled && policy.payload_supported);
    assert!(
        !policy.private_feed(true).streaming(),
        "PayloadSigner has the declared REST-only lifecycle"
    );
    assert!(
        policy.private_feed(false).streaming(),
        "RemoteSigner retains listen-key authentication"
    );
}
