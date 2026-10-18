//! Crypto.com construction through the Rust registry and production native host.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::rc::Rc;

#[test]
fn cryptocom_registry_constructs_native_inputs_and_reads_markets_over_http() {
    support::run_local(async {
        let h = support::venue_harness(
            ExchangeName::Cryptocom,
            Rc::new(|request| {
                (request.path == "/exchange/v1/public/get-instruments").then(|| json!({"code":0,"result":{"data":[{
                    "inst_type":"PERPETUAL_SWAP","tradable":true,"symbol":"BTCUSD-PERP",
                    "base_ccy":"BTC","quote_ccy":"USD","qty_tick_size":"0.0001","price_tick_size":"0.5",
                    "contract_size":"1","underlying_symbol":"BTCUSD","quantity_decimals":4,
                    "quote_decimals":1,"max_leverage":"50"
                }]}}))
            }),
            json!({"key":"native-key","secret":support::SECRET}),
            None,
            safe_cex_exchange::native::native_runtime(),
        ).await;
        assert_eq!(
            h.exchange.base().name,
            ExchangeName::Cryptocom,
            "REGISTRY Crypto.com identity"
        );
        let markets = h
            .exchange
            .fetch_markets()
            .await
            .expect("Crypto.com native market read");
        assert_eq!(markets.len(), 1, "REGISTRY Crypto.com market census");
        assert_eq!(markets[0].symbol, "BTCUSD-PERP");
        assert_eq!(markets[0].precision.amount, 0.0001);
        assert_eq!(markets[0].precision.price, 0.5);
        assert!(
            h.store.markets().is_empty(),
            "fetchMarkets stays pure through registry construction"
        );
        assert!(
            h.seen.borrow().iter().any(|request| request.method == "GET"
                && request.path == "/exchange/v1/public/get-instruments"),
            "REGISTRY Crypto.com read reaches real local HTTP"
        );
        h.exchange.dispose().expect("Crypto.com dispose");
    });
}
