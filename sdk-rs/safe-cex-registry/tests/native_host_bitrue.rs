//! Bitrue constructed through the registry and production native HTTP host.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::rc::Rc;
#[test]
fn bitrue_registry_validates_through_native_http_with_server_verified_signature() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let h = support::venue_harness(
            ExchangeName::Bitrue,
            Rc::new(|request| match request.path.as_str() {
                "/fapi/v2/account" => Some(json!({"code":"0","data":{"account":[]}})),
                _ => None,
            }),
            json!({"key":"native-bitrue-key","secret":support::SECRET}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        assert_eq!(
            serde_json::to_value(h.exchange.validate_account().await.unwrap().error).unwrap(),
            json!("")
        );
        let requests = h.seen.borrow();
        let account = requests
            .iter()
            .find(|r| r.path == "/fapi/v2/account")
            .unwrap();
        assert_eq!(
            account.headers.get("x-ch-apikey").map(String::as_str),
            Some("native-bitrue-key")
        );
        let timestamp = account.headers.get("x-ch-ts").unwrap();
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
        mac.update(format!("{timestamp}GET/fapi/v2/account").as_bytes());
        let expected = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(
            account.headers.get("x-ch-sign").unwrap(),
            &expected,
            "server verifies Bitrue preimage and HMAC"
        );
        h.exchange.dispose().unwrap();
    });
}
