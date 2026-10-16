//! Both Coinbase classes through the real native registry/HTTP/socket host.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use serde_json::json;
use std::rc::Rc;
use support::*;

#[test]
fn coinbase_variants_construct_and_run_through_native_registry() {
    run_local(async {
        for name in [
            safe_cex_types::ExchangeName::Coinbase,
            safe_cex_types::ExchangeName::Coinbasespot,
        ] {
            let h = venue_harness(name.clone(), Rc::new(|request| {
                Some(match request.path.as_str() {
                    "/api/v3/brokerage/portfolios" => json!({"portfolios":[{"name":"Synthetic Spot","uuid":"synthetic-portfolio","type":"DEFAULT","deleted":false}]}),
                    "/api/v3/brokerage/portfolios/synthetic-portfolio" => json!({"breakdown":{"spot_positions":[]}}),
                    "/api/v3/brokerage/products" => json!({"products":[{"product_id":"BTC-USDC","product_type":"SPOT","base_currency_id":"BTC","quote_currency_id":"USDC","price_increment":"0.01","base_increment":"0.001","base_min_size":"0.001","base_max_size":"100","price":"42.5","price_percentage_change_24h":"1","volume_24h":"3"}]}),
                    "/api/v3/brokerage/orders/historical/batch" => json!({"orders":[],"has_next":false}),
                    _ => return None,
                })
            }), json!({"key":"organizations/synthetic-org/apiKeys/synthetic-key","secret":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}), None, safe_cex_exchange::native::native_runtime()).await;
            assert_eq!(h.exchange.name(), &name);
            assert_eq!(
                h.exchange.get_account().await.unwrap().user_id,
                "synthetic-portfolio"
            );
            let markets = h.exchange.fetch_markets().await.unwrap();
            assert_eq!(markets[0].symbol, "BTC-USDC");
            assert_eq!(markets[0].precision.amount, 0.001);
            {
                let requests = h.seen.borrow();
                let request = requests
                    .iter()
                    .find(|request| request.path == "/api/v3/brokerage/portfolios")
                    .unwrap();
                assert!(
                    request
                        .headers
                        .get("coinbase-authorization")
                        .is_some_and(|value| value.starts_with("Bearer eyJ"))
                );
            }
            h.exchange.start().await.unwrap();
            eventually("both Coinbase sockets reached local native server", || {
                h.peers.borrow().len() == 2
            })
            .await;
            eventually("private subscriptions carry local JWT", || {
                h.received.borrow().iter().any(|message| {
                    message.contains("\"channel\":\"user\"") && message.contains("\"jwt\":\"eyJ")
                })
            })
            .await;
            h.exchange.dispose().unwrap();
        }
    });
}
#[test]
fn coinbase_spot_preserves_source_external_public_data_metadata_refusal() {
    run_local(async {
        let h = venue_harness(safe_cex_types::ExchangeName::Coinbasespot, Rc::new(|_| panic!("unsupported external provider must reject before venue HTTP")), json!({"key":"","secret":"","publicDataOnly":true,"externalPublicData":{"type":"websocket","url":"ws://127.0.0.1:1","exchangeId":"coinbasespot"}}), None, safe_cex_exchange::native::native_runtime()).await;
        let error = h.exchange.start().await.unwrap_err();
        assert!(
            error
                .message
                .contains("Exchange \"coinbasespot\" does not support public-only mode")
        );
        assert!(h.seen.borrow().is_empty());
        assert!(h.peers.borrow().is_empty());
    });
}
