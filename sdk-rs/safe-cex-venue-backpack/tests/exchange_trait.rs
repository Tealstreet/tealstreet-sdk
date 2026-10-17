//! Every `not_implemented()` default of the Exchange trait, driven on a constructed
//! BackpackExchange through `dyn Exchange` against a scripted transport.
#[path = "support/adapter_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::RestResponseBody;
use safe_cex_types::{Order, PlaceOrderOpts, UpdateOrderOpts};
use serde_json::{Value, json};
use std::{
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn drive<F: Future>(rt: &support::Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
        rt.now.set(rt.now.get() + 1000.0);
        rt.pump();
    }
    panic!("scripted call must settle");
}
fn script(wire: &support::Wire, path: &str, body: Value) {
    wire.bodies.lock().unwrap().insert(
        path.into(),
        RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(body).unwrap(),
        ),
    );
}
fn calls(wire: &support::Wire) -> Vec<(String, String)> {
    wire.requests
        .lock()
        .unwrap()
        .drain(..)
        .map(|r| (r.method.to_uppercase(), r.url))
        .collect()
}
fn sent(wire: &support::Wire, method: &str, path: &str) -> bool {
    calls(wire).iter().any(|(m, u)| m == method && u == path)
}
const OPEN_ORDER: &str = r#"{"id":"111","symbol":"BTC_USDC","side":"Bid","orderType":"Limit","price":"10","quantity":"1","executedQuantity":"0","status":"New","createdAt":1,"timeInForce":"GTC","selfTradePrevention":"RejectTaker","reduceOnly":false}"#;

#[test]
fn all_sixteen_not_implemented_defaults_are_bound_on_the_constructed_venue() {
    let (o, rt, store, wire) = support::fixture();
    let venue: Rc<dyn Exchange> = o.clone();
    script(
        &wire,
        "/api/v1/markets",
        json!([{"symbol":"BTC_USDC","marketType":"PERP","quantityIncrement":"0.25","priceIncrement":"0.125","minQuantity":"0.01","maxQuantity":"100","imfFunction":{"base":"0.05"}}]),
    );
    script(
        &wire,
        "/api/v1/tickers",
        json!([{"symbol":"BTC_USDC","lastPrice":"100","priceChangePercent":"0.05","volume":"2","quoteVolume":"200"}]),
    );
    script(
        &wire,
        "/api/v1/markPrices",
        json!([{"symbol":"BTC_USDC","markPrice":"101","indexPrice":"102","fundingRate":"0.0001","nextFundingTimestamp":7}]),
    );

    // 1 fetch_markets
    let markets = drive(&rt, venue.fetch_markets()).unwrap();
    assert_eq!(markets[0].symbol, "BTC_USDC");
    assert_eq!(
        markets[0].limits.leverage.max.to_bits(),
        20.0f64.to_bits(),
        "1/imf leverage"
    );
    assert!(sent(&wire, "GET", "/api/v1/markets"));

    // 2 fetch_tickers
    let tickers = drive(&rt, venue.fetch_tickers()).unwrap();
    assert_eq!(
        tickers[0].mark.to_bits(),
        101.0f64.to_bits(),
        "mark from markPrices"
    );
    assert_eq!(tickers[0].percentage.to_bits(), 5.0f64.to_bits());
    assert!(sent(&wire, "GET", "/api/v1/tickers"));

    // 3 start (public-data-only fixture: markets, tickers, then account slots loaded)
    drive(&rt, venue.start()).unwrap();
    assert!(
        store.loaded().markets && store.loaded().tickers,
        "start loads public state"
    );
    assert!(
        store.loaded().orders,
        "public-only start marks account state loaded"
    );
    calls(&wire);

    // 4 get_account
    script(
        &wire,
        "/api/v1/account",
        json!({"accountId":"acct-1","leverageLimit":"5"}),
    );
    let account = drive(&rt, venue.get_account()).unwrap();
    assert_eq!(account.user_id, "acct-1");
    assert!(sent(&wire, "GET", "/api/v1/account"));

    // 5 validate_account
    script(&wire, "/api/v1/capital", json!({}));
    script(
        &wire,
        "/api/v1/capital/collateral",
        json!({"netEquity":"10","netEquityAvailable":"7","netEquityLocked":"3","pnlUnrealized":"1"}),
    );
    let validation = drive(&rt, venue.validate_account()).unwrap();
    assert_eq!(
        validation.error.as_value(),
        &json!(""),
        "reachable capital endpoint validates"
    );
    assert_eq!(
        validation.balance.map(|b| b.total.to_bits()),
        Some(10.0f64.to_bits())
    );
    assert!(sent(&wire, "GET", "/api/v1/capital"));

    // 6 fetch_positions
    script(&wire, "/api/v1/position", json!([]));
    let positions = drive(&rt, venue.fetch_positions(None)).unwrap();
    assert!(
        positions
            .iter()
            .all(|p| p.symbol == "BTC_USDC" && p.contracts == 0.0)
    );
    assert!(sent(&wire, "GET", "/api/v1/position"));

    // 7 fetch_orders
    script(
        &wire,
        "/api/v1/orders",
        Value::Array(vec![serde_json::from_str(OPEN_ORDER).unwrap()]),
    );
    let orders = drive(&rt, venue.fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "111");
    assert!(sent(&wire, "GET", "/api/v1/orders"));
    let order: Order = orders[0].clone();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            orders: Some(vec![order.clone()]),
            ..Default::default()
        },
        false,
    )
    .unwrap();

    // 8 place_order (through the validated BaseExchange entry)
    let opts: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"BTC_USDC","type":"limit","side":"buy","amount":1.25,"price":10}),
    )
    .unwrap();
    let ids = drive(&rt, o.base.place_order(venue.as_ref(), opts)).unwrap();
    assert_eq!(ids, ["native-id"]);
    assert!(sent(&wire, "POST", "/api/v1/order"));

    // 9 update_order: cancel then replace
    let update: UpdateOrderOpts =
        serde_json::from_value(json!({"order": order, "update": {"price": 11}})).unwrap();
    let ids = drive(&rt, venue.update_order(update)).unwrap();
    assert_eq!(ids, ["native-id"]);
    let log = calls(&wire);
    let cancel = log
        .iter()
        .position(|c| c == &("DELETE".into(), "/api/v1/order".into()));
    let place = log
        .iter()
        .position(|c| c == &("POST".into(), "/api/v1/order".into()));
    assert!(
        cancel.is_some() && cancel < place,
        "cancel-and-replace order: {log:?}"
    );

    // 10 cancel_orders
    drive(&rt, venue.cancel_orders(vec![order])).unwrap();
    assert!(sent(&wire, "DELETE", "/api/v1/order"));

    // 11 set_leverage (account-wide PATCH)
    drive(&rt, venue.set_leverage("BTC_USDC".into(), 5.0, None)).unwrap();
    assert!(
        calls(&wire)
            .iter()
            .any(|(m, u)| m == "PATCH" && u == "/api/v1/account")
    );

    // 12-13 fixed hedged/cross: resolve after emitting, send nothing
    drive(&rt, venue.change_position_mode(false, None)).unwrap();
    drive(&rt, venue.change_margin_type(true, None)).unwrap();
    assert!(calls(&wire).is_empty(), "fixed modes never reach the venue");

    // 14-16 inherited BaseExchange rejections (base.ts:799, :989, :1138)
    let max = drive(&rt, venue.fetch_symbol_max_leverage("BTC_USDC".into())).unwrap_err();
    assert_eq!(max.message, "Not implemented");
    let schedule = drive(&rt, venue.schedule_cancel(Some(Some(60_000.0)))).unwrap_err();
    assert_eq!(schedule.message, "Not implemented");
    let trades = drive(&rt, venue.fetch_public_trades("BTC_USDC".into(), None)).unwrap_err();
    assert_eq!(trades.message, "Not implemented");
    assert!(calls(&wire).is_empty(), "inherited rejections send nothing");
}
