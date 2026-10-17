//! BingX's production native signer consumes the shared synced clock.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use hmac::{Hmac, Mac};
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::rc::Rc;

struct ResetClock;
impl Drop for ResetClock {
    fn drop(&mut self) {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
    }
}
fn balance(request: &support::Seen) -> Option<Value> {
    (request.path == "/openApi/swap/v3/user/balance").then(|| {
        json!({"code":0,"data":[{
            "asset":"USDT","userId":"clock-bingx","balance":"10","availableMargin":"8",
            "usedMargin":"2","unrealizedProfit":"1"
        }]})
    })
}
#[test]
fn native_bingx_signed_read_uses_server_synced_time() {
    support::run_local(async {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 90_000.0);
        let _reset = ResetClock;
        let before = VIRTUAL_CLOCK.get_current_time_ms();
        let h = support::venue_harness(
            ExchangeName::Bingx,
            Rc::new(balance),
            json!({"key":"native-key","secret":support::SECRET,"tealstreetTunnel":false}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        assert_eq!(
            h.exchange.get_account().await.unwrap().user_id,
            "clock-bingx"
        );
        let after = VIRTUAL_CLOCK.get_current_time_ms();
        let requests = h.seen.borrow();
        assert_eq!(requests.len(), 1, "one actual signed HTTP read");
        let request = &requests[0];
        let timestamp = support::query_value(&request.query, "timestamp").unwrap();
        let parsed = timestamp.parse::<f64>().unwrap();
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
        mac.update(format!("timestamp={timestamp}").as_bytes());
        let signature = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(
            support::query_value(&request.query, "signature").unwrap(),
            signature,
            "wire HMAC matches the timestamp"
        );
        assert!(
            parsed >= before && parsed <= after,
            "BingX timestamp {parsed} must use synced interval [{before}, {after}]"
        );
        drop(requests);
        h.exchange.dispose().unwrap();
    });
}
