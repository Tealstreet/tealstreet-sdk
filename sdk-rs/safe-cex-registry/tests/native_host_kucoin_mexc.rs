//! The bound KuCoin and MEXC families reached through the production registry.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use hmac::{Hmac, Mac};
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::rc::Rc;
fn hmac(message: &str) -> Vec<u8> {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    mac.finalize().into_bytes().to_vec()
}
#[test]
fn kucoin_and_spot_registry_construct_and_sign_real_account_reads() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        for (name, path) in [(ExchangeName::Kucoin, "/api/v1/user-info"), (ExchangeName::Kucoinspot, "/api/v2/user-info")] {
            let h = support::venue_harness(name.clone(), Rc::new(move |request| (request.path == path).then(|| json!({"code":"200000","data":{"userId":"kucoin-native","subUserId":"sub-native","referrerUserId":null}}))), json!({"key":"native-key","secret":support::SECRET,"passphrase":"native-pass"}), None, safe_cex_exchange::native::native_runtime()).await;
            let account = h.exchange.get_account().await.unwrap();
            assert_eq!(account.user_id,"kucoin-native", "{name:?}");
            assert_eq!(account.sub_id,Some(Some("sub-native".into())));
            let requests = h.seen.borrow();
            let request = requests.iter().find(|r| r.path == path).unwrap();
            assert_eq!(request.headers["kc-api-key"],"native-key");
            let preimage = format!("{}GET{}", request.headers["kc-api-timestamp"], path);
            use base64::Engine;
            assert_eq!(base64::engine::general_purpose::STANDARD.decode(&request.headers["kc-api-sign"]).unwrap(), hmac(&preimage));
            assert_eq!(base64::engine::general_purpose::STANDARD.decode(&request.headers["kc-api-passphrase"]).unwrap(), hmac("native-pass"));
            h.exchange.dispose().unwrap();
        }
    });
}
#[test]
fn mexc_registry_constructs_and_signs_a_real_account_validation() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let h = support::venue_harness(ExchangeName::Mexc, Rc::new(|request| (request.path == "/api/v1/private/account/assets").then(|| json!({"success":true,"code":0,"data":[{"currency":"USDT","availableBalance":81,"equity":123,"unrealized":5,"frozenBalance":0}]}))), json!({"key":"native-key","secret":support::SECRET}), None, safe_cex_exchange::native::native_runtime()).await;
        let account = h.exchange.get_account().await.unwrap();
        use sha2::Digest;
        let digest = sha2::Sha256::digest(b"native-key");
        let suffix = digest[..12].iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(account.user_id, format!("mexc:{suffix}"));
        let requests = h.seen.borrow();
        let request = requests.iter().find(|r| r.path == "/api/v1/private/account/assets").unwrap();
        assert_eq!(request.headers["apikey"], "native-key");
        let expected = hmac(&format!("native-key{}",request.headers["request-time"])).iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(request.headers["signature"], expected);
        h.exchange.dispose().unwrap();
    });
}

#[test]
fn kucoin_native_candle_returns_keep_source_construction_order() {
    use safe_cex_registry::native_account::{
        NativeAccountSpec, native_account_call, open_native_account,
    };
    use safe_cex_registry::native_value::NativeValue;
    use serde_json::Value;
    use std::{cell::RefCell, process::Command};
    use tokio::net::TcpListener;

    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/kucoin_candle_order_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "maintained KuCoin candles: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(cases.len(), 2, "futures control and Spot regression");
    support::run_local(async {
        for case in cases {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let raw = case["raw"].clone();
            let endpoint = case["request"]["url"].as_str().unwrap().to_owned();
            let wanted_endpoint = endpoint.clone();
            let seen = Rc::new(RefCell::new(vec![]));
            tokio::task::spawn_local(support::serve_http(
                listener,
                Rc::new(move |request| {
                    assert_eq!(request.method, "GET");
                    assert!(
                        request.path.ends_with(&wanted_endpoint),
                        "actual candle endpoint: {request:?}"
                    );
                    Some(json!({"code":"200000", "data":raw}))
                }),
                seen.clone(),
            ));
            let account = open_native_account(NativeAccountSpec {
                hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
                exchange: case["name"].as_str().unwrap().into(),
                options:
                    json!({"key":"", "secret":"", "publicDataOnly":true, "overrideProxyUrl":origin})
                        .into(),
                account_name: None,
                sign: None,
                proxy_token: None,
            })
            .unwrap();
            let output = native_account_call(
                account.clone(),
                "fetchOHLCV".into(),
                vec![case["options"].clone().into()],
            )
            .await
            .unwrap();
            let NativeValue::Array(candles) = &output else {
                panic!("native candle array: {output:?}")
            };
            assert_eq!(candles.len(), 2);
            let mut keys = Vec::new();
            for (index, candle) in candles.iter().enumerate() {
                let NativeValue::Object(fields) = candle else {
                    panic!("native candle object")
                };
                keys.push(
                    fields
                        .iter()
                        .map(|(key, _)| String::from_utf16(key).unwrap())
                        .collect::<Vec<_>>(),
                );
                assert_eq!(
                    candle
                        .get("open")
                        .unwrap()
                        .as_f64()
                        .unwrap()
                        .is_sign_negative(),
                    case["negativeZeroOpen"][index].as_bool().unwrap(),
                    "native signed zero survives source projection"
                );
            }
            assert_eq!(
                serde_json::to_value(keys).unwrap(),
                case["keys"],
                "KUCOIN_REST_KEY_ORDER {} preserves maintained mapper Object.keys",
                case["name"]
            );
            let wire = safe_cex_exchange::js_runtime::globals::stringify_json(&output.to_js())
                .unwrap()
                .unwrap();
            let wire = String::from_utf16(&wire).unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&wire).unwrap(),
                case["values"],
                "source candle values and row order"
            );
            assert_eq!(
                wire,
                case["json"].as_str().unwrap(),
                "source JSON.stringify byte order"
            );
            assert_eq!(seen.borrow().len(), 1, "one real native HTTP request");
            native_account_call(account, "dispose".into(), vec![])
                .await
                .unwrap();
        }
    });
}

#[test]
fn candle_default_projection_is_proven_for_every_other_family() {
    use safe_cex_registry::native_value::NativeValue;
    use safe_cex_types::Candle;
    use std::process::Command;
    let candle = Candle {
        timestamp: 17.,
        open: -0.0,
        high: f64::INFINITY,
        low: f64::NEG_INFINITY,
        close: f64::from_bits(0x7ff8_0000_0000_0042),
        volume: 31.,
        last_side: None,
    };
    let native = NativeValue::encode(&candle).unwrap();
    let NativeValue::Object(fields) = &native else {
        panic!("actual Candle serializer object")
    };
    let keys: Vec<_> = fields
        .iter()
        .map(|(key, _)| String::from_utf16(key).unwrap())
        .collect();
    for (key, expected) in [
        ("open", candle.open),
        ("high", candle.high),
        ("low", candle.low),
        ("close", candle.close),
    ] {
        assert_eq!(
            native.get(key).unwrap().as_f64().unwrap().to_bits(),
            expected.to_bits(),
            "native default retains {key} bits"
        );
    }
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rest_candle_projection_source.cjs"
        ))
        .arg(serde_json::to_string(&keys).unwrap())
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "all default families need their own source-order proof: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let proofs: Vec<serde_json::Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        proofs.len(),
        25,
        "24 unchanged families plus TxFlow empty fallback; KuCoin/Bitrue override"
    );
}
