#![cfg(feature = "blofin")]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identities, Identity, KeyRef, Operation, SignError};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

fn fixture() -> (LocalSigner, Identities) {
    let mut provider = LocalSigner::new();
    let identity = Identity::new(
        KeyRef("ref:owned-public".into()),
        Some(KeyRef("ref:owned-primary".into())),
    )
    .with_passphrase(Some(KeyRef("ref:owned-presentation".into())));
    provider.insert(identity.credential.clone(), b"test-key".to_vec());
    provider.insert(
        identity.signing_ref().unwrap().clone(),
        b"test-secret".to_vec(),
    );
    provider.insert(
        identity.passphrase.clone().unwrap(),
        b"test-passphrase".to_vec(),
    );
    let ids = Identities {
        primary: Some(identity),
        secondary: Some(
            Identity::new(
                KeyRef("ref:unavailable-secondary-public".into()),
                Some(KeyRef("ref:unavailable-secondary".into())),
            )
            .with_passphrase(Some(KeyRef(
                "ref:unavailable-secondary-presentation".into(),
            ))),
        ),
    };
    (provider, ids)
}

#[test]
fn blofin_captures_primary_http_and_ws_as_base64_of_the_hex_mac() {
    let (provider, ids) = fixture();
    let (work, kind) = hosted_signing::prepare(
        "blofin",
        &ids,
        Input::Http {
            payload: "/api/v1/account/balance?accountType=futuresGET1700000000000nonce-1".into(),
            stamp_ms: 1_700_000_000_000.,
            revision: "0".into(),
        },
    )
    .unwrap();
    work.validate().unwrap();
    assert!(
        matches!(&work.operations[1], Operation::Sign { key, .. } if key.0 == "ref:owned-primary"),
        "hosted capture signs with primary even when secondary is configured"
    );
    let result = provider.sign_now(&work).unwrap();
    work.validate_result(&result).unwrap();
    let output = hosted_signing::finish(kind, &result).unwrap();
    // Independent Python stdlib vectors: base64(hmac_sha256(secret, preimage).hexdigest()).
    assert_eq!(
        output.signature.as_deref(),
        Some("MWJmOWE2ZTk4OWYxNzliOWVmOGIyOTc4NWFlMzgxMWY1YjAxZmYxMjM2NDc3MDQ3NmRlMjIwNWEzZWZhZTZmYw==")
    );
    assert_eq!(output.credential.as_deref(), Some("test-key"));
    assert_eq!(
        output.headers,
        Some(vec![("ACCESS-PASSPHRASE".into(), "test-passphrase".into())])
    );
    for (format, expected) in [
        (
            OutputFormat::Base64Hex,
            "NGFjZmU5ODJiYTAyYWE1OTQ3MGQ1YTJjOGE2MDc2YTZlMTY1ZDQ4YmI3YjFlN2U1YmMxMWExNGZkNTc3ZGRmYg==",
        ),
        (
            OutputFormat::Hex,
            "4acfe982ba02aa59470d5a2c8a6076a6e165d48bb7b1e7e5bc11a14fd577ddfb",
        ),
    ] {
        let (work, kind) = hosted_signing::prepare(
            "blofin",
            &ids,
            Input::WsAuth {
                to_sign: "/users/self/verifyGET1700000000000nonce-1".into(),
                output_format: format,
            },
        )
        .unwrap();
        let output = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
        assert_eq!(output.signature.as_deref(), Some(expected));
        assert!(output.credential.is_none());
        assert!(output.headers.is_none());
    }
}

#[test]
fn blofin_capture_without_a_provider_passphrase_refuses_before_provider_work() {
    let (_, mut ids) = fixture();
    ids.primary.as_mut().unwrap().passphrase = None;
    assert!(matches!(
        hosted_signing::prepare(
            "blofin",
            &ids,
            Input::Http {
                payload: "/api/v1/account/balanceGET0nonce".into(),
                stamp_ms: 0.,
                revision: "0".into(),
            },
        ),
        Err(SignError::Unavailable)
    ));
}
