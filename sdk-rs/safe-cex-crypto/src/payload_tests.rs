use crate::*;
use futures::executor::block_on;
use safe_cex_types::ExchangeName;
use std::{collections::BTreeMap, future::Future, pin::Pin};

#[test]
fn node_crypto_hmac_vectors() {
    // Read the hex cases from the independent Node oracle, not copied literals.
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/node-crypto.json")).unwrap();
    let cases: Vec<_> = fixtures["primitives"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["encoding"] == "hex")
        .collect();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let key = case["key"].as_str().unwrap().as_bytes();
        let payload = case["payload"].as_str().unwrap().as_bytes();
        let actual = match case["algorithm"].as_str().unwrap() {
            "sha256" => hmac_sha256(key, payload, Encoding::Hex),
            "sha512" => hmac_sha512(key, payload, Encoding::Hex),
            other => panic!("unknown fixture algorithm {other}"),
        };
        assert_eq!(actual, case["signature"].as_str().unwrap());
    }
}

struct Signer {
    fail: bool,
}
impl PayloadSigner for Signer {
    fn sign<'a>(
        &'a self,
        request: PayloadSignerRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<PayloadSignerResult, String>> + 'a>> {
        Box::pin(async move {
            assert_eq!(request.exchange_name, ExchangeName::Binance);
            assert_eq!(request.key, "key");
            assert_eq!(request.payload, "exact\0雪");
            assert_eq!(
                request.scheme,
                PayloadScheme::HmacSha256 {
                    encoding: Encoding::Base64
                }
            );
            if self.fail {
                return Err("unavailable".into());
            }
            Ok(PayloadSignerResult {
                signature: "callback-signature".into(),
                headers: Some(BTreeMap::from([("timestamp".into(), "123".into())])),
            })
        })
    }
}

#[test]
fn callback_precedence_headers_and_errors() {
    for secret in ["", "local-secret"] {
        for fail in [false, true] {
            let signer = Signer { fail };
            let options = SigningOptions {
                key: "key",
                secret,
                payload_signer: Some(&signer),
            };
            assert!(can_sign_payload(&options));
            let result = block_on(sign_payload(
                &options,
                ExchangeName::Binance,
                "exact\0雪",
                Encoding::Base64,
            ));
            if fail {
                assert_eq!(result.unwrap_err(), "unavailable");
            } else {
                assert_eq!(
                    result.unwrap(),
                    PayloadSignerResult {
                        signature: "callback-signature".into(),
                        headers: Some(BTreeMap::from([("timestamp".into(), "123".into())]))
                    }
                );
            }
        }
    }
}

#[test]
fn local_payload_and_capability() {
    let mut options = SigningOptions {
        key: "",
        secret: "",
        payload_signer: None,
    };
    assert!(!can_sign_payload(&options));
    assert_eq!(
        block_on(sign_payload(
            &options,
            ExchangeName::Binance,
            "",
            Encoding::Hex
        ))
        .unwrap()
        .signature,
        "b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad"
    );
    options.secret = "test-secret-value";
    assert!(can_sign_payload(&options));
    assert_eq!(
        block_on(sign_payload(
            &options,
            ExchangeName::Binance,
            "1700000000GET/users/self/verify",
            Encoding::Base64
        ))
        .unwrap(),
        PayloadSignerResult {
            signature: "DCToOka+zCcbsBlRJK8Mq8CG969Due2fJmo/SgdpTgw=".into(),
            headers: None
        }
    );
}
