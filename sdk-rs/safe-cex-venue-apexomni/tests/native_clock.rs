//! Native synced-clock witness over production Apex HTTP, sockets, models and placement.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_store::{Store, virtual_clock::VIRTUAL_CLOCK};
use safe_cex_types::*;
use serde_json::{Value, json};
use support::{Harness, harness, options};
fn raw_order() -> Value {
    json!({"id":"order-1","symbol":"BTC-USDC","type":"LIMIT","side":"BUY","status":"OPEN","timeInForce":"GOOD_TIL_CANCEL","price":"100.12","size":"1.234","cumSuccessFillSize":"0","reduceOnly":false,"triggerPrice":"0","openSlParams":{"triggerPrice":"90","triggerPriceType":"MARKET"},"openTpParams":{"triggerPrice":"110","triggerPriceType":"MARKET"}})
}
fn contract() -> Value {
    json!({"symbol":"BTC-USDC","crossSymbolName":"BTCUSDC","baseTokenId":"BTC","settleAssetId":"USDC","l2PairId":"11","enableTrade":true,"stepSize":"0.001","tickSize":"0.01","minOrderSize":"0.001","maxOrderSize":"1000","displayMinLeverage":"1","displayMaxLeverage":"50","initialMarginRate":"0.1"})
}
fn fixture() -> Harness {
    fixture_with_tick_interval(None)
}
fn fixture_with_tick_interval(tick_interval: Option<f64>) -> Harness {
    let mut opts = options();
    opts.extra = tick_interval
        .map(|interval| serde_json::from_value(json!({"tickInterval": interval})).unwrap());
    opts.omni_key_seed = Some(format!("0x{}1b", "01".repeat(64)));
    opts.passphrase = Some("fixture-passphrase".into());
    let h = harness(ExchangeName::Apexomni, opts, |http| {
        http.ok(
            "GET",
            "/v3/symbols",
            json!({"contractConfig":{"perpetualContract":[contract()]}}),
        );
        http.ok("GET", "/v3/account", json!({"id":"16909060","l2key":"fixture-l2","positions":[{"symbol":"BTC-USDC","side":"LONG","size":"4","entryPrice":"123","customImr":"0.1"}],"contractAccount":{"makerFeeRate":"0.0012","takerFeeRate":"0.0013"}}));
        http.ok(
            "GET",
            "/v3/account-balance",
            json!({"totalEquityValue":"100","maintenanceMargin":"2","initialMargin":"10"}),
        );
        http.ok("GET", "/v3/user", json!({"id":"16909060"}));
        http.ok("GET", "/v3/open-orders", json!([raw_order()]));
        http.ok("POST", "/v3/order", json!({"id":"9007199254740993"}));
        http.ok("POST", "/v3/delete-order", json!({}));
        http.ok("POST", "/v3/set-initial-margin-rate", json!({}));
        http.ok("POST", "/v3/delete-open-orders", json!({}));
    });
    Store::update(h.store.as_ref(), serde_json::from_value(json!({"markets":[{"id":"BTC-USDC","symbol":"BTCUSDC","base":"BTC","quote":"USDC","active":true,"precision":{"amount":0.001,"price":0.01},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":50}},"info":contract()}]})).unwrap(), false).unwrap();
    h.store.update_zk_link_config(Some(json!({"id":"16909060","l2Key":"fixture-l2","contractAccount":{"makerFeeRate":"0.0012","takerFeeRate":"0.0013"},"contractConfig":{"perpetualContract":[contract()]}}))).unwrap();
    h
}
fn placed() -> PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1.2345,"price":100.127,"clientOrderId":"tscaller-1"})).unwrap()
}
fn done(h: &Harness) {
    h.venue().dispose().unwrap();
    h.runtime.pump();
}

struct ResetClock;
impl Drop for ResetClock {
    fn drop(&mut self) {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
    }
}
#[test]
fn native_synced_clock_reaches_http_sockets_models_and_expiration() {
    let _reset = ResetClock;
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 40_000.0);
    let h = fixture();
    let mut failures = Vec::new();
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    let positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    for (kind, rows) in [
        ("position", serde_json::to_value(&positions).unwrap()),
        ("order", serde_json::to_value(&orders).unwrap()),
    ] {
        assert!(!rows.as_array().unwrap().is_empty());
        for row in rows.as_array().unwrap() {
            if kind == "position" && row["contracts"].as_f64() == Some(0.0) {
                continue;
            }
            let stamp = row["lastUpdate"].as_f64().unwrap();
            if !(before..=after).contains(&stamp) {
                failures.push(format!("{kind} {} raw timestamp {stamp}", row["id"]));
            }
        }
    }
    let calls = h.http.calls("/v3/account");
    assert!(!calls.is_empty());
    let timestamp = calls[0].headers["APEX-TIMESTAMP"]
        .as_str()
        .unwrap()
        .parse::<f64>()
        .unwrap();
    if !(before..=after).contains(&timestamp) {
        failures.push(format!("HTTP auth raw timestamp {timestamp}"));
    }
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    let sockets = h.runtime.sockets.borrow().clone();
    assert_eq!(sockets.len(), 2);
    for (url, _) in &sockets {
        let stamp = url
            .split("timestamp=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        if !(before..=after).contains(&stamp) {
            failures.push(format!("socket URL raw timestamp {stamp}"));
        }
    }
    let private = sockets
        .iter()
        .find(|(url, _)| url.contains("private"))
        .unwrap()
        .1
        .clone();
    let login: Value = private
        .sent
        .borrow()
        .iter()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .find(|v| v["op"] == "login")
        .unwrap();
    let login: Value = serde_json::from_str(login["args"][0].as_str().unwrap()).unwrap();
    let stamp = login["timestamp"].as_f64().unwrap();
    if !(before..=after).contains(&stamp) {
        failures.push(format!("private auth raw timestamp {stamp}"));
    }
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    h.drive(h.venue().base().place_order(h.exchange.as_ref(), placed()))
        .unwrap();
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    let calls = h.http.calls("/v3/order");
    assert_eq!(calls.len(), 1);
    let expiry = calls[0].data["expiration"]
        .as_str()
        .unwrap()
        .parse::<f64>()
        .unwrap();
    let source_expiry = |now: f64| {
        ((now + 30.0 * 86_400_000.0) / 3_600_000.0).ceil() * 3_600_000.0 + 7.0 * 86_400_000.0
    };
    if !(source_expiry(before)..=source_expiry(after)).contains(&expiry) {
        failures.push(format!("order raw expiration {expiry}"));
    }
    let fills = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = fills.clone();
    h.venue().base().on(
        "fill",
        std::rc::Rc::new(move |_, value| {
            let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
            sink.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 46_001.0);
    h.runtime.advance(6_000.0);
    let mut row = raw_order();
    row["status"] = json!("FILLED");
    row["cumSuccessFillSize"] = json!("1.234");
    row["updatedAt"] = json!(h.runtime.now.get());
    private
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(
                serde_json::to_string(
                    &json!({"topic":"ws_zk_accounts_v3","contents":{"orders":[row]}}),
                )
                .unwrap(),
            ),
        ));
    h.runtime.pump();
    let orders = h.store.orders();
    let order = orders.iter().find(|o| o.id == "order-1").unwrap();
    assert_eq!(order.status.value(), Some(&OrderStatus::Closed));
    assert_eq!(order.filled, 1.234);
    assert_eq!(fills.borrow().len(), 1);
    assert_eq!(fills.borrow()[0]["id"], "order-1");
    assert_eq!(fills.borrow()[0]["symbol"], "BTCUSDC");
    assert_eq!(fills.borrow()[0]["price"], 100.12);
    drop(orders);
    done(&h);
    assert!(
        failures.is_empty(),
        "native synced clock readers diverge:\n{}",
        failures.join("\n")
    );
}
