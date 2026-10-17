//! Historical fills through the production receiver and signed HTTP client.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::{ExchangeName, OrderSide};
use serde_json::{Value, json};
use support::{harness_as, options};
fn fill(symbol: &str) -> Value {
    json!({"execId":"fill1","orderId":"order1","symbol":symbol,"execType":"Trade",
        "execQty":"2","execPrice":"100","execTime":"1700000000000",
        "execFee":"-0.2","feeCurrency":"USDT","isMaker":true})
}
fn fills_case(name: ExchangeName, symbol: &str, category: &str) {
    let h = harness_as(name, options(), |wire| {
        let mut row = fill(symbol);
        row["side"] = json!("Buy");
        wire.ok("GET", "/v5/execution/list", json!({"list":[row]}));
    });
    let opts = serde_json::from_value(json!({"symbol":symbol,"from":1,"to":2,"limit":10})).unwrap();
    let rows = h.drive(h.venue().fetch_historical_fills(opts)).unwrap();
    assert_eq!(
        rows.len(),
        1,
        "fills must reach the real execution-history reader"
    );
    assert_eq!(rows[0].id.as_deref(), Some("fill1"));
    assert_eq!(rows[0].order_id.as_deref(), Some("order1"));
    assert_eq!(
        (rows[0].timestamp, rows[0].price, rows[0].qty, rows[0].side),
        (1700000000., 100., 2., OrderSide::Buy)
    );
    assert_eq!(rows[0].commission, Some(0.2));
    assert_eq!(rows[0].maker, Some(true));
    assert_eq!(rows[0].commission_asset.as_deref(), Some("USDT"));
    let calls = h.wire.calls("/v5/execution/list");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["category"], category);
    assert_eq!(calls[0].params["startTime"], "1");
    assert_eq!(calls[0].params["endTime"], "2");
    assert_eq!(calls[0].params["limit"], "10");
    h.venue().dispose().unwrap();
}
#[test]
fn primary_fill_history_reaches_signed_http() {
    fills_case(ExchangeName::Bybit, "BTCUSDT", "linear");
}
#[test]
fn demo_fill_history_reaches_signed_http() {
    fills_case(ExchangeName::Bybitdemo, "BTCUSDT", "linear");
}
#[test]
fn inverse_fill_history_reaches_signed_http() {
    fills_case(ExchangeName::Bybitinverse, "BTCUSD", "inverse");
}
#[test]
fn spot_retains_inherited_empty_fills() {
    let h = harness_as(ExchangeName::Bybitspot, options(), |_| {});
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap();
    assert!(
        h.drive(h.venue().fetch_historical_fills(opts))
            .unwrap()
            .is_empty()
    );
    assert!(h.wire.calls("/v5/execution/list").is_empty());
    h.venue().dispose().unwrap();
}
