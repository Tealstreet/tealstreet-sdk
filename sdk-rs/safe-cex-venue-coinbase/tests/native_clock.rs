//! Synced source clock reads through production Coinbase signing and private frames.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};

struct ResetClock;
impl Drop for ResetClock {
    fn drop(&mut self) {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
    }
}
#[test]
fn native_jwt_and_private_order_timestamp_use_the_synced_clock() {
    let _reset = ResetClock;
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 40_000.0);
    let mut failures = Vec::new();
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let h = support::harness(name.clone(), support::options(), |http| {
            http.ok("GET", "/api/v3/brokerage/portfolios", json!({"portfolios":[{"name":"clock","uuid":"clock-portfolio","type":"DEFAULT","deleted":false}]}));
            http.ok(
                "GET",
                "/api/v3/brokerage/portfolios/clock-portfolio",
                json!({"breakdown":{"spot_positions":[]}}),
            );
            http.route(
                "GET",
                "/api/v3/brokerage/cfm/balance_summary",
                403.0,
                json!({"message":"No CFM permissions"}),
            );
            http.ok("GET", "/api/v3/brokerage/products", json!({"products":[{"product_id":"BTC-USDC","base_currency_id":"BTC","quote_currency_id":"USDC","price_increment":"0.01","base_increment":"0.001","base_min_size":"0.001","base_max_size":"100","trading_disabled":false,"view_only":false,"price":"123.5","price_percentage_change_24h":"2.5","volume_24h":"1000"}]}));
            http.ok(
                "GET",
                "/api/v3/brokerage/orders/historical/batch",
                json!({"orders":[],"has_next":false}),
            );
        });
        h.runtime.now.set(123.0);
        let before = (VIRTUAL_CLOCK.get_current_time_ms() / 1000.0).floor();
        assert_eq!(
            h.drive(h.venue().get_account()).unwrap().user_id,
            "clock-portfolio"
        );
        let after = (VIRTUAL_CLOCK.get_current_time_ms() / 1000.0).floor();
        let calls = h.http.calls("/api/v3/brokerage/portfolios");
        assert_eq!(calls.len(), 1);
        let token = calls[0].headers["Coinbase-Authorization"]
            .as_str()
            .unwrap()
            .strip_prefix("Bearer ")
            .unwrap();
        let claims: Value = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(token.split('.').nth(1).unwrap())
                .unwrap(),
        )
        .unwrap();
        let nbf = claims["nbf"].as_f64().unwrap();
        if !(before..=after).contains(&nbf) {
            failures.push(format!(
                "{name:?} JWT uses raw clock: {nbf} outside {before}..={after}"
            ));
        }
        assert_eq!(claims["exp"].as_f64().unwrap() - nbf, 120.0);
        h.drive(h.venue().start()).unwrap();
        h.runtime.pump();
        let socket = h
            .runtime
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| url == "wss://advanced-trade-ws-user.coinbase.com")
            .unwrap()
            .1
            .clone();
        let before = VIRTUAL_CLOCK.get_current_time_ms();
        socket.events.borrow_mut().push_back(WsEvent::Message(SocketFrame::Text(json!({"channel":"user","events":[{"type":"snapshot","orders":[{"order_id":"clock-order","client_order_id":"clock-client","product_id":"BTC-USDC","order_type":"LIMIT","order_side":"BUY","status":"OPEN","cumulative_quantity":"0","leaves_quantity":"2.5","limit_price":"123.45"}]}]}).to_string())));
        h.runtime.pump();
        let after = VIRTUAL_CLOCK.get_current_time_ms();
        let orders = h.store.orders();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].id, "clock-order");
        let last_update = orders[0].last_update;
        if !(before..=after).contains(&last_update) {
            failures.push(format!(
                "{name:?} private order uses raw clock: {last_update} outside {before}..={after}"
            ));
        }
        drop(orders);
        h.venue().dispose().unwrap();
    }
    assert!(
        failures.is_empty(),
        "native synced clock readers diverge:\n{}",
        failures.join("\n")
    );
}
