#![cfg(all(feature = "dispatch-signing", not(target_arch = "wasm32")))]
use safe_cex_engine::signing::*;
use safe_cex_venues::{EngineOptions, prepare_local_signer_for};
use serde_json::json;

fn options(venue: &str) -> EngineOptions {
    let mut raw = json!({"key":"synthetic-api-key","secret":"synthetic-secret","passphrase":"synthetic-passphrase"});
    match venue {
        "backpack" | "kraken" | "krakenspot" | "polymarket" => {
            raw["secret"] =
                safe_cex_crypto::encode(&[7; 32], safe_cex_crypto::Encoding::Base64).into();
        }
        "coinbase" => {
            raw["key"] = "organizations/synthetic/apiKeys/synthetic".into();
            raw["secret"] =
                safe_cex_crypto::encode(&[7; 32], safe_cex_crypto::Encoding::Base64).into();
        }
        "grvt" | "hyperliquid" | "hyperliquidspot" => {
            raw["secret"] = "01".repeat(32).into();
        }
        "paradex" => {
            raw["key"] = format!("0x{}", "01".repeat(32)).into();
            raw["paradexAddress"] = "0x123".into();
            raw["subkeyAddress"] = "0x456".into();
        }
        "lighter" | "lighter-robinhood" => {
            raw["secret"] = format!("01{}", "00".repeat(39)).into();
            raw["apiKeyIndex"] = "1".into();
        }
        "orderly" => {
            raw["secret"] = format!("{}2", "1".repeat(31)).into();
        }
        _ => {}
    }
    serde_json::from_value(raw).unwrap()
}
fn check(venue: &str) {
    check_options(venue, options(venue));
}
fn check_options(venue: &str, mut options: EngineOptions) {
    let (plain, ids) = prepare_local_signer_for(venue, &mut options)
        .unwrap_or_else(|_| panic!("{venue}: canonical setup"));
    let primary = ids
        .primary
        .as_ref()
        .unwrap_or_else(|| panic!("{venue}: private identity"));
    let key = primary.signing_ref().unwrap().clone();
    let algorithms = [
        Algorithm::HmacSha256,
        Algorithm::HmacSha512,
        Algorithm::Sha256SecretSuffix,
        Algorithm::Ed25519,
        Algorithm::Secp256k1Digest,
        Algorithm::StarkDigest,
        Algorithm::SchnorrDigest,
        Algorithm::Es256,
    ];
    let algorithm = algorithms
        .into_iter()
        .find(|alg| plain.can_sign(&key, *alg) == Capability::Supported)
        .unwrap_or_else(|| panic!("{venue}: signable key"));
    let mut request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::Sign {
            algorithm,
            key: key.clone(),
            payload: vec![PayloadPart::Bytes {
                bytes: vec![
                    7;
                    if algorithm == Algorithm::SchnorrDigest {
                        40
                    } else {
                        32
                    }
                ],
            }],
        }],
    };
    #[cfg(feature = "coinbase")]
    if venue == "coinbase" {
        let binding = options.coinbase_signing.as_ref().unwrap();
        let input = safe_cex_venue_coinbase::sign::signing_input_for(
            binding.algorithm,
            &binding.key_name,
            "synthetic-nonce",
            1_700_000_000,
            Some("GET api.coinbase.com/api/v3/brokerage/accounts"),
            false,
        )
        .unwrap();
        request.operations = vec![
            Operation::Present {
                key: primary.credential.clone(),
            },
            Operation::Sign {
                algorithm,
                key: key.clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: input.into_bytes(),
                }],
            },
        ];
    }
    let expected = plain
        .sign_ready(&request)
        .unwrap()
        .unwrap_or_else(|_| panic!("{venue}: real signature before protection"));
    let owner = plain
        .protect()
        .unwrap_or_else(|_| panic!("{venue}: protected setup"));
    let handle = owner.provider();
    assert_eq!(
        handle.sign_ready(&request).unwrap().unwrap(),
        expected,
        "{venue}: signature parity"
    );
    let mut queued = handle.sign(request.clone());
    owner.erase();
    assert_eq!(
        handle.can_sign(&key, algorithm),
        Capability::Unsupported,
        "{venue}"
    );
    assert_eq!(
        handle.sign_ready(&request),
        Some(Err(SignError::Unavailable)),
        "{venue}"
    );
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert_eq!(
        queued.as_mut().poll(&mut cx),
        std::task::Poll::Ready(Err(SignError::Unavailable)),
        "{venue}"
    );
}
macro_rules! family {
    ($test:ident,$feature:literal,$venue:literal) => {
        #[cfg(feature=$feature)]
        #[test]
        fn $test() {
            check($venue);
        }
    };
}
family!(apexomni, "apexomni", "apexomni");
family!(asterdex, "asterdex", "asterdex");
family!(backpack, "backpack", "backpack");
family!(binance, "binance", "binance");
family!(bingx, "bingx", "bingx");
family!(bitget, "bitget", "bitget");
family!(bitrue, "bitrue", "bitrue");
family!(bitunix, "bitunix", "bitunix");
family!(blofin, "blofin", "blofin");
family!(bybit, "bybit", "bybit");
family!(coinbase, "coinbase", "coinbase");
family!(cryptocom, "cryptocom", "cryptocom");
family!(grvt, "grvt", "grvt");
family!(hyperliquid, "hyperliquid", "hyperliquid");
family!(kraken, "kraken", "kraken");
family!(kucoin, "kucoin", "kucoin");
family!(kucoinspot, "kucoinspot", "kucoinspot");
family!(lighter, "lighter", "lighter");
family!(mexc, "mexc", "mexc");
family!(okx, "okx", "okx");
family!(orderly, "orderly", "orderly");
family!(paradex, "paradex", "paradex");
family!(phemex, "phemex", "phemex");
family!(polymarket, "polymarket", "polymarket");
family!(txflow, "txflow", "txflow");
family!(weex, "weex", "weex");
family!(woo, "woo", "woo");
family!(bitgetusdc, "bitget", "bitgetusdc");
family!(hyperliquidspot, "hyperliquid", "hyperliquidspot");
family!(krakenspot, "kraken", "krakenspot");
family!(lighter_robinhood, "lighter", "lighter-robinhood");

#[cfg(feature = "coinbase")]
#[test]
fn coinbase_es256() {
    // PKCS8 encoding of the fixed synthetic scalar 01..01, generated in-test.
    let mut der = vec![
        0x30, 0x41, 0x02, 0x01, 0x00, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02,
        0x01, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x04, 0x27, 0x30, 0x25,
        0x02, 0x01, 0x01, 0x04, 0x20,
    ];
    der.extend_from_slice(&[1; 32]);
    let mut options = options("coinbase");
    options.secret = Some(format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        safe_cex_crypto::encode(&der, safe_cex_crypto::Encoding::Base64)
    ));
    check_options("coinbase", options);
}
