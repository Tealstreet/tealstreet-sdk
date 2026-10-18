//! Phemex through the Rust registry, production native inputs and real local HTTP.
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
fn native_phemex_expiry_uses_shared_synced_dayjs_clock() {
    support::run_local(async {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 90_000.0);
        let _reset = ResetClock;
        let before = VIRTUAL_CLOCK.get_current_time_ms();
        let h = support::venue_harness(
            ExchangeName::Phemex,
            Rc::new(|request| match request.path.as_str() {
                "/public/products" => Some(json!({"data":{"perpProductsV2":[{
                    "symbol":"BTCUSDT","type":"PerpetualV2","baseCurrency":"BTC",
                    "quoteCurrency":"USDT","settleCurrency":"USDT","qtyStepSize":"0.003",
                    "tickSize":"0.25","maxOrderQtyRq":"100","maxLeverage":"50"
                }]}})),
                "/g-accounts/accountPositions" => Some(json!({"code":0,"data":{
                    "account":{"userID":123,"accountBalanceRv":"1000","totalUsedBalanceRv":"200"},
                    "positions":[]
                }})),
                _ => None,
            }),
            json!({"key":"native-key","secret":support::SECRET}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        assert_eq!(
            h.exchange.base().name,
            ExchangeName::Phemex,
            "REGISTRY Phemex identity"
        );
        let markets = h
            .exchange
            .fetch_markets()
            .await
            .expect("Phemex native market read");
        assert_eq!(markets.len(), 1, "REGISTRY Phemex market census");
        assert_eq!(markets[0].symbol, "BTCUSDT");
        assert_eq!(
            (markets[0].precision.amount, markets[0].precision.price),
            (0.003, 0.25)
        );
        assert!(h.store.markets().is_empty(), "fetchMarkets read purity");
        let account = h
            .exchange
            .get_account()
            .await
            .expect("Phemex native signed account read");
        assert_eq!(serde_json::to_value(account).unwrap()["userId"], "123");
        let after = VIRTUAL_CLOCK.get_current_time_ms();
        let seen = h.seen.borrow();
        let request = seen
            .iter()
            .find(|request| request.path == "/g-accounts/accountPositions")
            .expect("REGISTRY Phemex account reaches real local HTTP");
        assert_eq!(request.method, "GET");
        assert_eq!(request.query, "currency=USDT");
        assert_eq!(request.headers["x-phemex-access-token"], "native-key");
        let expiry = &request.headers["x-phemex-request-expiry"];
        let payload = format!("/g-accounts/accountPositionscurrency=USDT{expiry}");
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
        let expected: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(request.headers["x-phemex-request-signature"], expected);
        let expiry = expiry.parse::<f64>().unwrap();
        assert!(
            expiry >= (before / 1000.0).round() + 60.0 && expiry <= (after / 1000.0).round() + 60.0,
            "Phemex expiry {expiry} must use synced interval [{before}, {after}] plus source 60s"
        );
        drop(seen);
        h.exchange.dispose().expect("Phemex dispose");
    });
}
