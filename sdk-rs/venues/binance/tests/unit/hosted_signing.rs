//! Exact-byte work of the hosted-signing facade (crate access).
use super::hosted_signing::*;
use super::*;
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Algorithm, KeyRef};

fn identity() -> Identity {
    Identity::new(
        KeyRef("primary.credential".into()),
        Some(KeyRef("primary.signing".into())),
    )
}

fn mac(work: &SignRequest) -> String {
    let (signer, _) = LocalSigner::from_json(
        r#"{"primary":{"key":"primary-key+%é","secret":"fixture-secret"}}"#.into(),
    )
    .unwrap();
    let signed = signer.sign_now(work).unwrap();
    assert_eq!(signed.values[0], "primary-key+%é".as_bytes());
    safe_cex_crypto::hex(&signed.values[1])
}

#[test]
fn exact_query_keeps_reserved_field_positions_repeated_values_and_percent_spelling() {
    let id = identity();
    let context = Context {
        venue: "binance".into(),
        method: "POST".into(),
        host: "fapi.binance.com".into(),
        path: "/fapi/v1/order".into(),
    };
    let query = "symbol=BTCUSDT&timestamp=1709209849123&recvWindow=5000&note=a%2Bb%20%C3%A9&value=first&value=second";
    let work = http_work(&id, query.into(), 1_709_209_849_123., context.clone()).unwrap();
    assert_eq!(work.context, context);
    assert_eq!(
        work.receipt,
        SignReceipt {
            nonce: None,
            stamp_ms: Some(1_709_209_849_123.),
            expiry_ms: None
        }
    );
    assert_eq!(
        work.operations,
        vec![
            Operation::Present {
                key: id.credential.clone(),
            },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: id.signing_ref().unwrap().clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: query.as_bytes().to_vec()
                }],
            },
        ]
    );
    // Fixed independent Python stdlib HMAC-SHA256 vector.
    assert_eq!(
        mac(&work),
        "2cf174dc789e88cf3d35b79599e83e61296a773686ae0e347d73ed710ba00ac0"
    );
    assert!(
        http_work(&identity(), String::new(), 1_000.5, Context::default())
            .unwrap()
            .validate()
            .is_err()
    );
}

#[test]
fn exact_ws_preimage_does_not_draw_a_new_stamp_or_rewrite_key_escaping() {
    let id = identity();
    let work = ws_work(
        &id,
        "apiKey=primary-key%2B%25%C3%A9&timestamp=1709209849123".into(),
        Context::default(),
    )
    .unwrap();
    assert_eq!(work.receipt, SignReceipt::default());
    assert_eq!(
        work.operations,
        vec![
            Operation::Present {
                key: id.credential.clone(),
            },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: id.signing_ref().unwrap().clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: b"apiKey=primary-key%2B%25%C3%A9&timestamp=1709209849123".to_vec()
                }],
            },
        ]
    );
    assert_eq!(
        mac(&work),
        "47dbb20a2c36798579807e450312ce25eedb069b35edac2115ff85334800b390"
    );
}
