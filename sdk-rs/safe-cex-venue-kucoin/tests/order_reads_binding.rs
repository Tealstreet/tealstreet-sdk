//! The constructed exchange must exercise the reader used by production.
#[path = "support/exchange_runtime.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::{Store, StoreUpdate};
use serde_json::{Value, json};
use support::drive;

fn row(id: &str, stop: &str, size: f64) -> Value {
    json!({"id":id,"symbol":"XBTUSDTM","type":"limit","side":"buy","price":"95","size":size,"filledSize":0,"status":"open","timeInForce":"GTC","reduceOnly":false,"createdAt":1700000000000u64,"stop":stop})
}

#[test]
fn constructed_order_reader_uses_both_books_and_retains_failed_categories() {
    let (venue, f) = support::futures();
    f.wire.script("GET", "/api/v1/contracts/active", json!({"code":"200000","data":[{"symbol":"XBTUSDTM","rootSymbol":"USDT","baseCurrency":"XBT","quoteCurrency":"USDT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"multiplier":0.001,"maxOrderQty":1000000,"maxLeverage":125,"isInverse":false}]}));
    let markets = drive(&f.runtime, venue.fetch_markets()).unwrap();
    Store::update(
        f.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    f.wire.take();
    f.wire.script(
        "GET",
        "/api/v1/orders",
        json!({"code":"200000","data":{"items":[row("normal", "", 11.0)]}}),
    );
    f.wire.script(
        "GET",
        "/api/v1/stopOrders",
        json!({"code":"200000","data":{"items":[row("algo", "up", 7.0)]}}),
    );
    let orders = drive(&f.runtime, venue.fetch_orders(None)).unwrap();
    assert_eq!(
        orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["normal", "algo"]
    );
    assert_eq!(orders[0].amount.to_bits(), (11.0f64 * 0.001).to_bits());
    assert_eq!(orders[1].amount.to_bits(), (7.0f64 * 0.001).to_bits());
    let requests = f.wire.take();
    assert_eq!(
        requests
            .iter()
            .map(|(m, u, _)| (m.as_str(), u.as_str()))
            .collect::<Vec<_>>(),
        [("GET", "/api/v1/orders"), ("GET", "/api/v1/stopOrders")]
    );
    assert!(
        requests
            .iter()
            .all(|(_, _, params)| params == &json!({"status":"active"}))
    );
    Store::update(
        f.store.as_ref(),
        StoreUpdate {
            orders: Some(orders),
            ..Default::default()
        },
        false,
    )
    .unwrap();

    // TS fetchOrders retains only the failed book from the current Store.
    // A successful empty sibling book must actually clear that category.
    f.wire.script(
        "GET",
        "/api/v1/orders",
        json!({"code":"400001","msg":"normal unavailable"}),
    );
    f.wire.script(
        "GET",
        "/api/v1/stopOrders",
        json!({"code":"200000","data":{"items":[]}}),
    );
    let retained = drive(&f.runtime, venue.fetch_orders(None)).unwrap();
    assert_eq!(
        retained.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["normal"]
    );
    assert_eq!(retained[0].amount.to_bits(), (11.0f64 * 0.001).to_bits());
    assert_eq!(f.wire.take().len(), 2);
    venue.dispose().unwrap();
}
