//! All six Orderly names constructed through the production registry/native HTTP host.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use base64::Engine;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::rc::Rc;
#[test]
fn orderly_family_registry_reads_account_with_server_verified_ed25519() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        for name in [ExchangeName::Kodiakfi, ExchangeName::Mode, ExchangeName::Salsadex, ExchangeName::TealstreetDex, ExchangeName::What, ExchangeName::Woofi] {
            let h = support::venue_harness(name.clone(), Rc::new(|request| {
                (request.path == "/v1/client/info").then(|| json!({"success":true,"data":{"account_id":"orderly-native"}}))
            }), json!({"key":"native-key","secret":"11111111111111111111111111111111","applicationId":"account-native"}), None, safe_cex_exchange::native::native_runtime()).await;
            assert_eq!(h.exchange.get_account().await.unwrap().user_id, "orderly-native", "{name:?}");
            let requests = h.seen.borrow();
            let request = requests.iter().find(|r| r.path == "/v1/client/info").unwrap();
            assert_eq!(request.headers["orderly-key"], "ed25519:native-key");
            assert_eq!(request.headers["orderly-account-id"], "account-native");
            let preimage = format!("{}GET/v1/client/info", request.headers["orderly-timestamp"]);
            assert_eq!(request.headers["orderly-textsign"], preimage);
            let signature = base64::engine::general_purpose::URL_SAFE.decode(&request.headers["orderly-signature"]).unwrap();
            let key = ed25519_dalek::SigningKey::from_bytes(&[0;32]).verifying_key();
            let signature = ed25519_dalek::Signature::from_slice(&signature).unwrap();
            key.verify_strict(preimage.as_bytes(), &signature).unwrap();
            assert!(key.verify_strict(b"different-route", &signature).is_err());
            h.exchange.dispose().unwrap();
        }
    });
}
