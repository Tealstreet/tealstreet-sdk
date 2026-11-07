#![cfg(feature = "bitunix")]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{
    Algorithm, Encoding, Identities, Identity, KeyRef, Operation, PayloadPart, SignError,
};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

fn fixture() -> (LocalSigner, Identities) {
    let mut provider = LocalSigner::new();
    let identity = Identity::new(
        KeyRef("ref:owned-public".into()),
        Some(KeyRef("ref:owned-primary".into())),
    );
    provider.insert(identity.credential.clone(), b"api-key-xyz".to_vec());
    provider.insert(
        identity.signing_ref().unwrap().clone(),
        b"test-secret-value".to_vec(),
    );
    let ids = Identities {
        primary: Some(identity),
        secondary: Some(Identity::new(
            KeyRef("ref:unavailable-secondary-public".into()),
            Some(KeyRef("ref:unavailable-secondary".into())),
        )),
    };
    (provider, ids)
}

// Node crypto vector from `base.ws-auth-signing.test.ts`, also reproduced with
// Python hashlib: hex(sha256(hex(sha256(toSign)) ‖ material)).
const TO_SIGN: &str = "nonce-abc1700000000api-key-xyz";
const GOLDEN: &str = "c99d3e027fcb8cbefc05f7a38dfc710011d07e79dbefd8b0bc7b44347bb0e624";

#[test]
fn bitunix_http_and_login_captures_sign_the_browser_golden_with_primary_refs() {
    let (provider, ids) = fixture();
    let inputs = [
        Input::Http {
            payload: TO_SIGN.into(),
            stamp_ms: 1_700_000_000_000.,
            revision: "0".into(),
        },
        Input::WsAuth {
            to_sign: TO_SIGN.into(),
            output_format: OutputFormat::BitunixDoubleSha256,
        },
    ];
    for (index, input) in inputs.into_iter().enumerate() {
        let (work, kind) = hosted_signing::prepare("bitunix", &ids, input).unwrap();
        work.validate().unwrap();
        let ops = &work.operations;
        assert_eq!(ops.len(), 3);
        assert!(matches!(&ops[0], Operation::Present { key } if key.0 == "ref:owned-public"));
        assert!(matches!(&ops[1], Operation::Sha256 { payload }
            if payload.len() == 1
                && matches!(&payload[0], PayloadPart::Bytes { bytes } if bytes == TO_SIGN.as_bytes())));
        assert!(
            matches!(&ops[2], Operation::Sign { algorithm: Algorithm::Sha256SecretSuffix, key, payload }
                if key.0 == "ref:owned-primary"
                    && matches!(payload[..], [PayloadPart::Result { index: 1, encoding: Encoding::Hex }])),
            "hosted capture signs with primary even when secondary is configured"
        );
        let result = provider.sign_now(&work).unwrap();
        work.validate_result(&result).unwrap();
        let output = hosted_signing::finish(kind, &result).unwrap();
        assert_eq!(output.signature.as_deref(), Some(GOLDEN));
        // HTTP presents the key for the `api-key` header; the login only signs.
        assert_eq!(
            output.credential.as_deref(),
            (index == 0).then_some("api-key-xyz")
        );
        assert!(output.headers.is_none());
    }
    let (work, kind) = hosted_signing::prepare("bitunix", &ids, Input::Credential {}).unwrap();
    let output = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
    assert_eq!(output.credential.as_deref(), Some("api-key-xyz"));
}

#[test]
fn the_output_format_must_name_the_family_scheme() {
    let (_, ids) = fixture();
    for format in [
        OutputFormat::Hex,
        OutputFormat::Base64,
        OutputFormat::Base64Hex,
    ] {
        assert!(matches!(
            hosted_signing::prepare(
                "bitunix",
                &ids,
                Input::WsAuth {
                    to_sign: TO_SIGN.into(),
                    output_format: format,
                },
            ),
            Err(SignError::InvalidRequest)
        ));
    }
    assert!(matches!(
        hosted_signing::prepare(
            "bitunix",
            &ids,
            Input::WsAuth {
                to_sign: String::new(),
                output_format: OutputFormat::BitunixDoubleSha256,
            },
        ),
        Err(SignError::InvalidRequest)
    ));
    #[cfg(feature = "bitget")]
    assert!(matches!(
        hosted_signing::prepare(
            "bitget",
            &ids,
            Input::WsAuth {
                to_sign: TO_SIGN.into(),
                output_format: OutputFormat::BitunixDoubleSha256,
            },
        ),
        Err(SignError::InvalidRequest)
    ));
}

#[test]
fn bitunix_capture_without_a_primary_refuses_before_provider_work() {
    let (_, mut ids) = fixture();
    ids.primary = None;
    assert!(matches!(
        hosted_signing::prepare(
            "bitunix",
            &ids,
            Input::Http {
                payload: TO_SIGN.into(),
                stamp_ms: 0.,
                revision: "0".into(),
            },
        ),
        Err(SignError::Unavailable)
    ));
}
