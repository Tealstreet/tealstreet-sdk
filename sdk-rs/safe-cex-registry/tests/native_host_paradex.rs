//! Construct Paradex through production native registry inputs and local HTTP.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::rc::Rc;

#[test]
fn paradex_registry_constructs_native_inputs_and_reads_market_precision() {
    support::run_local(async {
        let h = support::venue_harness(
            ExchangeName::Paradex,
            Rc::new(|request| match request.path.as_str() {
                "/v1/markets" => Some(json!({"results":[{
                    "market":"BTC-USD-PERP","base_currency":"BTC","quote_currency":"USD",
                    "asset_kind":"PERP","status":"ACTIVE","step_size":"0.003","tick_size":"0.25",
                    "min_order_size":"0.003","max_order_size":"100","initial_margin_fraction":"0.05"
                }]})),
                _ => None,
            }),
            json!({"key":"","secret":"","publicDataOnly":true}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        assert_eq!(
            h.exchange.base().name,
            ExchangeName::Paradex,
            "REGISTRY Paradex identity"
        );
        let markets = h
            .exchange
            .fetch_markets()
            .await
            .expect("REGISTRY Paradex native market read");
        assert_eq!(markets.len(), 1, "REGISTRY Paradex market census");
        assert_eq!(markets[0].symbol, "BTC-USD-PERP");
        assert_eq!(
            (markets[0].precision.amount, markets[0].precision.price),
            (0.003, 0.25)
        );
        assert!(
            h.store.markets().unwrap().is_empty(),
            "source fetchMarkets does not write Store"
        );
        let seen = h.seen.borrow();
        let request = seen
            .iter()
            .find(|request| request.path == "/v1/markets")
            .expect("REGISTRY Paradex reaches real local HTTP");
        assert_eq!(request.method, "GET");
        assert!(request.query.is_empty());
        drop(seen);
        h.exchange.dispose().expect("Paradex dispose");
    });
}
