#![cfg(feature = "phemex")]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identities, Identity, KeyRef};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

#[test]
fn phemex_captures_primary_http_and_ws_as_hex_hmac_with_the_presented_key() {
    let mut provider = LocalSigner::new();
    let identity = Identity::new(
        KeyRef("ref:owned-public".into()),
        Some(KeyRef("ref:owned-primary".into())),
    );
    provider.insert(identity.credential.clone(), b"test-key".to_vec());
    provider.insert(
        identity.signing_ref().unwrap().clone(),
        b"test-secret".to_vec(),
    );
    let ids = Identities {
        primary: Some(identity),
        secondary: None,
    };
    let (work, kind) = hosted_signing::prepare(
        "phemex",
        &ids,
        Input::Http {
            payload: r#"/g-orders/createsymbol=BTCUSDT1709209909{"symbol":"BTCUSDT"}"#.into(),
            stamp_ms: 1_709_209_849_123.,
            revision: "0".into(),
        },
    )
    .unwrap();
    work.validate().unwrap();
    let result = provider.sign_now(&work).unwrap();
    work.validate_result(&result).unwrap();
    let output = hosted_signing::finish(kind, &result).unwrap();
    // Independent Python hmac vectors over the exact preimages.
    assert_eq!(
        output.signature.as_deref(),
        Some("c65a3f73379e52ae83f6da565acd1b50e1cd8d45c5bd74482faceef338416bff")
    );
    assert_eq!(output.credential.as_deref(), Some("test-key"));
    assert!(output.headers.is_none());
    let (work, kind) = hosted_signing::prepare(
        "phemex",
        &ids,
        Input::WsAuth {
            to_sign: "test-key1709209909".into(),
            output_format: OutputFormat::Hex,
        },
    )
    .unwrap();
    let output = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
    assert_eq!(
        output.signature.as_deref(),
        Some("3a2ba3d0cc7b75c9738bec26dc87b4e74acffe0db0760308f93e05a75d984824")
    );
    assert!(output.credential.is_none());
}
