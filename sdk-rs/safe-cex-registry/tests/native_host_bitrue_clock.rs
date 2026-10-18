//! Bitrue constructed through the registry and production native HTTP host.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
struct ResetClock;
impl Drop for ResetClock {
    fn drop(&mut self) {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
    }
}
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::rc::Rc;
#[test]
fn native_bitrue_signed_read_uses_server_synced_time() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 90_000.0);
        let _reset = ResetClock;
        let before = VIRTUAL_CLOCK.get_current_time_ms();
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
        let after = VIRTUAL_CLOCK.get_current_time_ms();
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
        let timestamp = timestamp.parse::<f64>().unwrap();
        assert!(
            timestamp >= before && timestamp <= after,
            "Bitrue timestamp {timestamp} must use synced interval [{before}, {after}]"
        );
        drop(requests);
        h.exchange.dispose().unwrap();
    });
}
