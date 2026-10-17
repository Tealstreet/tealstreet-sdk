//! Synced virtualClock through the actual native Hyperliquid registry signer.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::rc::Rc;
const KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
const ADDRESS: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";
struct ResetClock;
impl Drop for ResetClock {
    fn drop(&mut self) {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
    }
}
#[test]
fn native_leverage_signature_uses_synced_virtual_nonce() {
    support::run_local(async {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 90_000.0);
        let _reset = ResetClock;
        let h = support::venue_harness(
            ExchangeName::Hyperliquid,
            Rc::new(|request| {
                (request.path == "/exchange")
                    .then(|| json!({"status":"ok","response":{"type":"default"}}))
            }),
            json!({"key":ADDRESS,"secret":KEY,"applicationId":ADDRESS}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        h.store
            .update(
                safe_cex_store::StoreUpdate {
                    markets: Some(vec![serde_json::from_value(json!({
                "id":"BTC", "symbol":"BTC-PERP", "base":"BTC", "quote":"USDC", "active":true,
                "precision":{"amount":0.00001,"price":1},
                "limits":{"amount":{"min":0.00001,"max":100},"leverage":{"min":1,"max":40}},
                "info":{"hyperliquidId":0}
            })).unwrap()]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        let before = VIRTUAL_CLOCK.get_current_time_ms();
        h.exchange
            .set_leverage("BTC-PERP".into(), 7.0, None)
            .await
            .unwrap();
        let after = VIRTUAL_CLOCK.get_current_time_ms();
        let requests = h.seen.borrow();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.path, "/exchange");
        let body: Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(
            body["action"],
            json!({"type":"updateLeverage","asset":0,"isCross":false,"leverage":7})
        );
        let expected = serde_json::to_value(
            safe_cex_venue_hyperliquid::signing::sign_l1_action(
                KEY,
                &body["action"],
                None,
                &body["nonce"],
                true,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(body["signature"]["r"], expected["r"]);
        assert_eq!(body["signature"]["s"], expected["s"]);
        assert_eq!(
            body["signature"]["v"].as_f64().unwrap().to_bits(),
            expected["v"].as_f64().unwrap().to_bits(),
            "actual signed action and wire nonce"
        );
        let nonce = body["nonce"].as_f64().unwrap();
        assert!(
            nonce >= before && nonce <= after,
            "Hyperliquid virtual nonce {nonce} must use synced interval [{before}, {after}]"
        );
        drop(requests);
        h.exchange.dispose().unwrap();
    });
}
