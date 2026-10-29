#![cfg(feature = "bingx")]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identities, Identity, KeyRef, SignError};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

fn fixture() -> (LocalSigner, Identities) {
    let mut provider = LocalSigner::new();
    let identity = Identity::new(
        KeyRef("ref:owned-public".into()),
        Some(KeyRef("ref:owned-primary".into())),
    );
    provider.insert(identity.credential.clone(), b"bingx-api-key".to_vec());
    provider.insert(
        identity.signing_ref().unwrap().clone(),
        b"bingx-test-secret".to_vec(),
    );
    let ids = Identities {
        primary: Some(identity),
        secondary: None,
    };
    (provider, ids)
}

// Node crypto: createHmac('sha256', 'bingx-test-secret').update(TO_SIGN).digest('hex'),
// the legacy BingX client's sorted repeat-format query.
const TO_SIGN: &str = "price=90000&quantity=0.012&side=BUY&symbol=BTC-USDT&timestamp=1700000000000";
const GOLDEN: &str = "8c8447ee861f85ae474b991b2067631c146496d94a02879011511a0fab1e81e4";

#[test]
fn bingx_http_and_ws_auth_captures_sign_the_hex_hmac_golden() {
    let (provider, ids) = fixture();
    let inputs = [
        Input::Http {
            payload: TO_SIGN.into(),
            stamp_ms: 1_700_000_000_000.,
            revision: "0".into(),
        },
        Input::WsAuth {
            to_sign: TO_SIGN.into(),
            output_format: OutputFormat::Hex,
        },
    ];
    for (index, input) in inputs.into_iter().enumerate() {
        let (work, kind) = hosted_signing::prepare("bingx", &ids, input).unwrap();
        work.validate().unwrap();
        let result = provider.sign_now(&work).unwrap();
        work.validate_result(&result).unwrap();
        let output = hosted_signing::finish(kind, &result).unwrap();
        assert_eq!(output.signature.as_deref(), Some(GOLDEN));
        // HTTP presents the key for the `X-BX-APIKEY` header; ws-auth only signs.
        assert_eq!(
            output.credential.as_deref(),
            (index == 0).then_some("bingx-api-key")
        );
        assert!(output.headers.is_none());
    }
    let (work, kind) = hosted_signing::prepare("bingx", &ids, Input::Credential {}).unwrap();
    let output = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
    assert_eq!(output.credential.as_deref(), Some("bingx-api-key"));
}

#[test]
fn bingx_refuses_the_bitunix_scheme_and_a_missing_primary() {
    let (_, ids) = fixture();
    let fused = Input::WsAuth {
        to_sign: TO_SIGN.into(),
        output_format: OutputFormat::BitunixDoubleSha256,
    };
    assert!(matches!(
        hosted_signing::prepare("bingx", &ids, fused),
        Err(SignError::InvalidRequest)
    ));
    let none = Identities {
        primary: None,
        secondary: None,
    };
    assert!(matches!(
        hosted_signing::prepare("bingx", &none, Input::Credential {}),
        Err(SignError::Unavailable)
    ));
}
