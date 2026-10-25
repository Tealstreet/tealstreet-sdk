//! Exact canonical work for trusted hosted signing. Future dispatch recipes use
//! these same builders; this module owns no credentials or crypto execution.
use safe_cex_engine::num;
use safe_cex_engine::signing::{
    Context, Identity, PayloadPart, SignError, SignReceipt, SignRequest, hmac_sha256_work,
};

/// The window and payload are already serialized by the caller. Preserve their
/// bytes; the shared receipt validator checks the supplied integer wire stamp.
/// Bybit refuses a request once server time passes `stamp + recv_window`, so
/// that is the receipt's expiry (none unless the window is a positive number).
pub fn http_work(
    identity: &Identity,
    payload: String,
    stamp_ms: f64,
    recv_window: String,
    context: Context,
) -> Result<SignRequest, SignError> {
    let expiry_ms = recv_window
        .parse::<std::num::NonZeroU32>()
        .ok()
        .map(|window| window.get())
        .map(|window| stamp_ms + f64::from(window));
    hmac_sha256_work(
        identity,
        vec![
            PayloadPart::Bytes {
                bytes: num::format(stamp_ms).into_bytes(),
            },
            PayloadPart::Credential {
                key: identity.credential.clone(),
            },
            PayloadPart::Bytes {
                bytes: recv_window.into_bytes(),
            },
            PayloadPart::Bytes {
                bytes: payload.into_bytes(),
            },
        ],
        context,
        SignReceipt {
            stamp_ms: Some(stamp_ms),
            expiry_ms,
        },
    )
}

/// Legacy WS replies carry only the MAC, so sign the original caller preimage
/// without replacing its expiry. There is no locally generated time receipt.
pub fn ws_work(
    identity: &Identity,
    exact_preimage: String,
    context: Context,
) -> Result<SignRequest, SignError> {
    hmac_sha256_work(
        identity,
        vec![PayloadPart::Bytes {
            bytes: exact_preimage.into_bytes(),
        }],
        context,
        SignReceipt::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_crypto::signer::LocalSigner;
    use safe_cex_engine::signing::{Algorithm, KeyRef, Operation};

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
    fn http_grammar_preserves_encoded_payload_window_spelling_and_private_key_interpolation() {
        let id = identity();
        let context = Context {
            venue: "bybit".into(),
            method: "GET".into(),
            host: "api.bybit.com".into(),
            path: "/v5/order/realtime".into(),
        };
        let payload = "symbol=BTCUSDT&note=a%2Bb%20%C3%A9&arr%5B0%5D=x&arr%5B1%5D=y";
        let work = http_work(
            &id,
            payload.into(),
            1_709_209_849_123.,
            "05000".into(),
            context.clone(),
        )
        .unwrap();
        assert_eq!(work.context, context);
        // Bybit refuses a request once server time passes stamp + recv_window.
        assert_eq!(
            work.receipt,
            SignReceipt {
                stamp_ms: Some(1_709_209_849_123.),
                expiry_ms: Some(1_709_209_854_123.)
            }
        );
        assert_eq!(
            work.operations,
            vec![
                Operation::Present {
                    key: id.credential.clone()
                },
                Operation::Sign {
                    algorithm: Algorithm::HmacSha256,
                    key: id.signing_ref().unwrap().clone(),
                    payload: vec![
                        PayloadPart::Bytes {
                            bytes: b"1709209849123".to_vec()
                        },
                        PayloadPart::Credential { key: id.credential },
                        PayloadPart::Bytes {
                            bytes: b"05000".to_vec()
                        },
                        PayloadPart::Bytes {
                            bytes: payload.as_bytes().to_vec()
                        },
                    ],
                },
            ]
        );
        // Fixed independent Python stdlib HMAC-SHA256 vector.
        assert_eq!(
            mac(&work),
            "86b0f72185cadeab33d930e8b07bb20b3a3b3773a08c9c5567f608e0d1be24e0"
        );
    }

    #[test]
    fn exact_json_body_is_not_parsed_quoted_or_normalized() {
        let work = http_work(
            &identity(),
            r#"{"symbol":"BTCUSDT","qty":"0.0100","memo":"é/+%"}"#.into(),
            1_709_209_849_123.,
            "20000".into(),
            Context::default(),
        )
        .unwrap();
        assert_eq!(
            mac(&work),
            "943d19ca5f9f956a4845e517c50b7b25ffa943ae055852de5d43ef7f290074c4"
        );
        assert!(
            http_work(
                &identity(),
                String::new(),
                1_000.5,
                "5000".into(),
                Context::default()
            )
            .unwrap()
            .validate()
            .is_err()
        );
    }

    #[test]
    fn ws_grammar_signs_original_expiry_without_fabricated_timestamp_or_expiry_receipt() {
        let id = identity();
        let work = ws_work(&id, "GET/realtime1709209800000".into(), Context::default()).unwrap();
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
                        bytes: b"GET/realtime1709209800000".to_vec()
                    }],
                },
            ]
        );
        assert_eq!(
            mac(&work),
            "ed727abc59c372e822af2470fecf7203db4d531d084b7bc422ebc29c5423026f"
        );
    }
}
