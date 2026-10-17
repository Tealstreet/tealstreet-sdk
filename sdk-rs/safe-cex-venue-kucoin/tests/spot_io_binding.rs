//! Production Spot readers/writers through the constructed exchange and HTTP.
#[path = "support/exchange_runtime.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::{Store, StoreUpdate};
use serde_json::{Value, json};
use support::drive;

fn load(f: &support::Fixture, venue: &dyn Exchange) {
    f.wire.script("GET", "/api/v2/symbols", json!({"code":"200000","data":[{"symbol":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.0001","priceIncrement":"0.1","baseMinSize":"0.00001","baseMaxSize":"10000"}]}));
    let markets = drive(&f.runtime, venue.fetch_markets()).unwrap();
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].symbol, "BTC-USDT");
    assert_eq!(markets[0].precision.amount.to_bits(), 0.0001f64.to_bits());
    Store::update(
        f.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_eq!(f.wire.take()[0].1, "/api/v2/symbols");
}

#[test]
fn constructed_spot_reader_retains_distinct_prices_and_current_store_fallback() {
    let (venue, f) = support::spot();
    load(&f, venue.as_ref());
    f.wire.script("GET", "/api/v1/market/allTickers", json!({"code":"200000","data":{"ticker":[{"symbol":"BTC-USDT","buy":"91.25","sell":"103.5","last":"97.75","changeRate":"0.03125","vol":"7","volValue":"684.25"},{"symbol":"UNKNOWN","buy":"1"}]}}));
    let tickers = drive(&f.runtime, venue.fetch_tickers()).unwrap();
    assert_eq!(tickers.len(), 1);
    let t = &tickers[0];
    assert_eq!(
        (t.bid.to_bits(), t.ask.to_bits(), t.last.to_bits()),
        (91.25f64.to_bits(), 103.5f64.to_bits(), 97.75f64.to_bits())
    );
    assert_eq!(t.percentage.to_bits(), 3.125f64.to_bits());
    assert_eq!(f.wire.take()[0].1, "/api/v1/market/allTickers");
    Store::update(
        f.store.as_ref(),
        StoreUpdate {
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    f.wire.script(
        "GET",
        "/api/v1/market/allTickers",
        json!({"code":"400001","msg":"unavailable"}),
    );
    let retained = drive(&f.runtime, venue.fetch_tickers()).unwrap();
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].bid.to_bits(), 91.25f64.to_bits());
    venue.dispose().unwrap();
}

fn order() -> safe_cex_types::Order {
    serde_json::from_value(json!({"id":"spot-first","symbol":"BTC-USDT","status":"open","type":"limit","side":"buy","price":91.25,"amount":0.5,"filled":0.1,"remaining":0.4,"reduceOnly":false,"lastUpdate":3})).unwrap()
}
#[test]
fn constructed_spot_cancels_use_distinct_routes_and_symbol_params() {
    let (venue, f) = support::spot();
    load(&f, venue.as_ref());
    for route in [
        "/api/v1/hf/orders/spot-first",
        "/api/v1/hf/orders",
        "/api/v1/hf/orders/cancelAll",
    ] {
        f.wire
            .script("DELETE", route, json!({"code":"200000","data":{}}));
    }
    drive(&f.runtime, venue.cancel_orders(vec![order()])).unwrap();
    drive(&f.runtime, venue.cancel_symbol_orders("BTC-USDT".into())).unwrap();
    drive(&f.runtime, venue.cancel_all_orders()).unwrap();
    assert_eq!(
        f.wire.take(),
        vec![
            (
                "DELETE".into(),
                "/api/v1/hf/orders/spot-first".into(),
                json!({"symbol":"BTC-USDT"})
            ),
            (
                "DELETE".into(),
                "/api/v1/hf/orders".into(),
                json!({"symbol":"BTC-USDT"})
            ),
            (
                "DELETE".into(),
                "/api/v1/hf/orders/cancelAll".into(),
                Value::Null
            ),
        ]
    );
    venue.dispose().unwrap();
}

#[test]
fn constructed_spot_order_discovery_reaches_the_selected_symbol_page() {
    let (venue, f) = support::spot();
    load(&f, venue.as_ref());
    f.wire.script(
        "GET",
        "/api/v1/hf/orders/active/symbols",
        json!({"code":"200000","data":{"symbols":["BTC-USDT"]}}),
    );
    f.wire.script("GET", "/api/v1/hf/orders/active/page", json!({"code":"200000","data":{"totalPage":1,"items":[{"id":"spot-first","symbol":"BTC-USDT","type":"limit","side":"buy","price":"91.25","size":"0.5","dealSize":"0.1","active":true,"timeInForce":"GTC","postOnly":false,"lastUpdatedAt":1700000000000u64}]}}));
    let orders = drive(&f.runtime, venue.fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "spot-first");
    assert_eq!(orders[0].remaining.to_bits(), 0.4f64.to_bits());
    assert_eq!(
        f.wire.take(),
        vec![
            (
                "GET".into(),
                "/api/v1/hf/orders/active/symbols".into(),
                Value::Null
            ),
            (
                "GET".into(),
                "/api/v1/hf/orders/active/page".into(),
                json!({"symbol":"BTC-USDT","pageNum":"1","pageSize":"50"})
            ),
        ]
    );
    venue.dispose().unwrap();
}
