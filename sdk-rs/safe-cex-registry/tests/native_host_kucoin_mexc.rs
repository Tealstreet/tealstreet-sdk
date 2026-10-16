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
