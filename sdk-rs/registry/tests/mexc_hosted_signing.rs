#![cfg(all(feature = "mexc", feature = "hosted-signing"))]

use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

#[test]
fn mexc_hosted_http_and_ws_match_independent_vectors() {
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("replay-key".into()),
            secret: Secret::new("replay-secret".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../venues/mexc/tests/fixtures/hosted-signing.json"
    ))
    .unwrap();
    for vector in vectors.as_array().unwrap() {
        let (work, kind) = hosted_signing::prepare(
            "mexc",
            &identities,
            Input::Http {
                payload: vector["payload"].as_str().unwrap().into(),
                stamp_ms: 1_700_000_000_000.,
                revision: "1".into(),
            },
        )
        .expect("MEXC needs hosted captures before admission");
        work.validate().unwrap();
        let result = provider.sign_now(&work).unwrap();
        work.validate_result(&result).unwrap();
        let output = hosted_signing::finish(kind, &result).unwrap();
        assert_eq!(output.signature.as_deref(), vector["signature"].as_str());
        assert_eq!(output.credential.as_deref(), Some("replay-key"));
    }
    let (work, kind) = hosted_signing::prepare(
        "mexc",
        &identities,
        Input::WsAuth {
            to_sign: "replay-key1700000000000".into(),
            output_format: OutputFormat::Hex,
        },
    )
    .unwrap();
    let output = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
    assert_eq!(
        output.signature.as_deref(),
        vectors[6]["signature"].as_str()
    );
    let policy = safe_cex_venues::remote_signing_policy("mexc");
    assert!(policy.enabled && policy.payload_supported);
    assert!(!policy.private_feed(true).streaming());
    assert!(policy.private_feed(false).streaming());
}
