//! Every `not_implemented()` default of the Exchange trait, driven on a venue
//! built by `MexcExchange::new` through `dyn Exchange` against a scripted wire.
#![cfg(not(target_arch = "wasm32"))]
#[path = "support/exchange_trait_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_types::{MarginType, Order, PlaceOrderOpts, PositionSide, UpdateOrderOpts};
use serde_json::{Value, json};
use std::{
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn drive<F: Future>(rt: &support::Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        rt.pump();
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
        rt.now.set(rt.now.get() + 1000.0);
    }
    panic!("scripted call must settle");
}
fn script(wire: &support::Wire, path: &str, data: Value) {
    wire.bodies
        .borrow_mut()
        .insert(path.into(), json!({"success":true,"code":0,"data":data}));
}
fn calls(wire: &support::Wire) -> Vec<(String, String, Value)> {
    wire.requests.borrow_mut().drain(..).collect()
}
fn bodies(log: &[(String, String, Value)], method: &str, path: &str) -> Vec<Value> {
    log.iter()
        .filter(|(m, u, _)| m == method && u == path)
        .map(|(_, _, b)| b.clone())
        .collect()
}
fn sent(log: &[(String, String, Value)], method: &str, path: &str) -> bool {
    !bodies(log, method, path).is_empty()
}

const DETAIL: &str = "/api/v1/contract/detail";
const TICKER: &str = "/api/v1/contract/ticker";
const ASSETS: &str = "/api/v1/private/account/assets";
const POSITION_MODE: &str = "/api/v1/private/position/position_mode";
const POSITIONS: &str = "/api/v1/private/position/open_positions";
const OPEN_ORDERS: &str = "/api/v1/private/order/list/open_orders";
const PLAN_ORDERS: &str = "/api/v1/private/planorder/list/orders";
const STOP_ORDERS: &str = "/api/v1/private/stoporder/list/orders";
const SUBMIT: &str = "/api/v1/private/order/submit";
const SUBMIT_BATCH: &str = "/api/v1/private/order/submit_batch";
const CANCEL: &str = "/api/v1/private/order/cancel";
const CANCEL_ALL: &str = "/api/v1/private/order/cancel_all";
const PLAN_PLACE: &str = "/api/v1/private/planorder/place/v2";
const CHANGE_LEVERAGE: &str = "/api/v1/private/position/change_leverage";
const CHANGE_MODE: &str = "/api/v1/private/position/change_position_mode";

fn script_venue(wire: &support::Wire) {
    script(
        wire,
        DETAIL,
        json!([{"symbol":"BTC_USDT","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT",
            "contractSize":0.0001,"priceUnit":0.1,"volUnit":1,"minVol":1,"maxVol":1000000,
            "minLeverage":1,"maxLeverage":125,"state":0,"apiAllowed":true,
            "makerFeeRate":0.0002,"takerFeeRate":0.0006}]),
    );
    script(
        wire,
        TICKER,
        json!([{"symbol":"BTC_USDT","lastPrice":60000,"fairPrice":60010,"indexPrice":60005,
            "bid1":59999,"ask1":60001,"riseFallRate":0.012,"holdVol":1000,"fundingRate":0.0001,
            "nextSettleTime":1700003600000u64,"volume24":5000,"amount24":300000}]),
    );
    script(wire, POSITION_MODE, json!(1));
    script(
        wire,
        ASSETS,
        json!([{"currency":"USDT","availableBalance":100,"equity":120,"unrealized":5,"frozenBalance":0}]),
    );
    script(
        wire,
        POSITIONS,
        json!([{"positionId":777,"symbol":"BTC_USDT","positionType":1,"openType":1,"holdVol":2,
            "holdAvgPrice":60000,"leverage":10,"unrealized":1,"state":1}]),
    );
    script(
        wire,
        OPEN_ORDERS,
        json!([{"orderId":"9001","symbol":"BTC_USDT","side":1,"orderType":1,"vol":3,"price":59000,
            "state":2,"dealVol":0,"leverage":10,"openType":1,"createTime":1699999999000u64,
            "updateTime":1699999999000u64}]),
    );
    script(wire, PLAN_ORDERS, json!([]));
    script(wire, STOP_ORDERS, json!([]));
    script(
        wire,
        SUBMIT,
        json!({"orderId":"1234567","ts":1700000000000u64}),
    );
    script(
        wire,
        SUBMIT_BATCH,
        json!([{"orderId":"2001","errorCode":0},{"orderId":"2002","errorCode":0}]),
    );
    script(wire, PLAN_PLACE, json!("7654321"));
    script(wire, CANCEL, json!([{"orderId":"9001","errorCode":0}]));
}

#[test]
fn all_sixteen_not_implemented_defaults_are_bound_on_the_constructed_venue() {
    let (o, rt, store, wire) = support::fixture();
    let venue: Rc<dyn Exchange> = o.clone();
    script_venue(&wire);

    // 1 fetch_markets
    let markets = drive(&rt, venue.fetch_markets()).unwrap();
    assert_eq!(markets[0].symbol, "BTCUSDT", "fetch_markets symbol");
    assert_eq!(markets[0].limits.leverage.max.to_bits(), 125f64.to_bits());
    assert!(sent(&calls(&wire), "GET", DETAIL), "fetch_markets request");

    // 2 start: markets, tickers, position mode, balance/positions/orders, tick, ready
    drive(&rt, venue.start()).unwrap();
    let log = calls(&wire);
    for path in [
        DETAIL,
        TICKER,
        POSITION_MODE,
        ASSETS,
        POSITIONS,
        OPEN_ORDERS,
        PLAN_ORDERS,
    ] {
        assert!(sent(&log, "GET", path), "start request {path}: {log:?}");
    }
    let loaded = store.loaded().clone();
    assert!(
        loaded.markets && loaded.tickers && loaded.balance && loaded.positions && loaded.orders,
        "start loaded flags"
    );
    assert!(store.options().is_hedged, "start position-mode setting");
    assert_eq!(
        store.balance().total.to_bits(),
        120f64.to_bits(),
        "start balance"
    );
    assert!(
        rt.console
            .borrow()
            .iter()
            .any(|m| m.contains("Ready to trade on MEXC")),
        "start ready log: {:?}",
        rt.console.borrow()
    );
    rt.pump();
    let frames = rt.sent.borrow().clone();
    assert!(
        frames.iter().any(|f| f.contains("\"login\"")),
        "start private login frame: {frames:?}"
    );
    assert!(
        !frames.iter().any(|f| f.contains("personal.filter")),
        "private feeds wait for login: {frames:?}"
    );

    // 3 fetch_tickers
    let tickers = drive(&rt, venue.fetch_tickers()).unwrap();
    assert_eq!(tickers[0].mark.to_bits(), 60010f64.to_bits(), "ticker mark");
    assert!(sent(&calls(&wire), "GET", TICKER), "fetch_tickers request");

    // 4 get_account
    let account = drive(&rt, venue.get_account()).unwrap();
    assert!(account.user_id.starts_with("mexc:"), "account id");
    assert_eq!(account.user_id.len(), "mexc:".len() + 24, "account id hash");
    assert!(sent(&calls(&wire), "GET", ASSETS), "get_account request");

    // 5 validate_account
    let validation = drive(&rt, venue.validate_account()).unwrap();
    assert_eq!(validation.error.as_value(), &json!(""), "validation error");
    assert_eq!(
        validation.balance.map(|b| b.total.to_bits()),
        Some(120f64.to_bits()),
        "validation balance"
    );
    assert!(
        sent(&calls(&wire), "GET", ASSETS),
        "validate_account request"
    );

    // 6 fetch_positions
    let positions = drive(&rt, venue.fetch_positions(None)).unwrap();
    let long = positions
        .iter()
        .find(|p| p.side == PositionSide::Long && p.contracts > 0.0)
        .expect("open long position");
    assert_eq!(
        long.contracts.to_bits(),
        0.0002f64.to_bits(),
        "position contracts"
    );
    assert!(
        sent(&calls(&wire), "GET", POSITIONS),
        "fetch_positions request"
    );

    // 7 fetch_orders
    let orders = drive(&rt, venue.fetch_orders(None)).unwrap();
    let resting: Order = orders
        .iter()
        .find(|o| o.id == "9001")
        .expect("open order 9001")
        .clone();
    let log = calls(&wire);
    assert!(sent(&log, "GET", OPEN_ORDERS), "fetch_orders request");
    assert!(sent(&log, "GET", STOP_ORDERS), "fetch_orders stop request");

    // 8 place_order through the validated BaseExchange entry
    let opts: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":3,"price":59500}),
    )
    .unwrap();
    let ids = drive(&rt, o.base().place_order(venue.as_ref(), opts)).unwrap();
    assert_eq!(ids, ["1234567"], "place_order id");
    let submitted = bodies(&calls(&wire), "POST", SUBMIT);
    assert_eq!(
        submitted,
        [
            json!({"symbol":"BTC_USDT","price":"59500","vol":3,"side":1,"type":1,"openType":2,
            "leverage":10,"externalOid":"ts_fixtureid"})
        ],
        "place_order payload"
    );
    assert!(
        store.orders().iter().any(|o| o.id == "1234567"),
        "optimistic regular order"
    );

    // 8a place_orders: an all-regular batch goes to the native batch endpoint
    let batch: Vec<PlaceOrderOpts> = serde_json::from_value(json!([
        {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":59000},
        {"symbol":"BTCUSDT","type":"limit","side":"sell","amount":2,"price":61000}
    ]))
    .unwrap();
    let ids = drive(&rt, o.base().place_orders(venue.as_ref(), batch)).unwrap();
    assert_eq!(ids, ["2001", "2002"], "place_orders ids");
    let submitted = bodies(&calls(&wire), "POST", SUBMIT_BATCH);
    assert_eq!(submitted.len(), 1, "one batch request");
    assert_eq!(submitted[0][0]["stpMode"], json!(0), "batch stpMode");
    assert_eq!(submitted[0][1]["side"], json!(3), "batch second side");

    // 8b stop placement takes the plan-order path
    let stop: PlaceOrderOpts =
        serde_json::from_value(json!({"symbol":"BTCUSDT","type":"stop_market",
        "side":"sell","amount":2,"triggerPrice":58000.05}))
        .unwrap();
    let ids = drive(&rt, o.base().place_order(venue.as_ref(), stop)).unwrap();
    assert_eq!(ids, ["7654321"], "plan order id");
    let plan = bodies(&calls(&wire), "POST", PLAN_PLACE);
    assert_eq!(plan.len(), 1, "one plan placement");
    assert_eq!(
        plan[0]["triggerPrice"],
        json!("58000.1"),
        "plan trigger price"
    );
    assert_eq!(plan[0]["side"], json!(4), "plan closes long");

    // 9 update_order: cancel then replace at the new price
    let update: UpdateOrderOpts =
        serde_json::from_value(json!({"order": resting, "update": {"price": 59100}})).unwrap();
    let ids = drive(&rt, venue.update_order(update)).unwrap();
    assert_eq!(ids, ["1234567"], "update_order id");
    let log = calls(&wire);
    let cancel = log.iter().position(|(m, u, _)| m == "POST" && u == CANCEL);
    let place = log.iter().position(|(m, u, _)| m == "POST" && u == SUBMIT);
    assert!(
        cancel.is_some() && cancel < place,
        "cancel-and-replace order: {log:?}"
    );
    assert_eq!(
        bodies(&log, "POST", CANCEL),
        [json!(["9001"])],
        "update cancel"
    );
    assert_eq!(
        bodies(&log, "POST", SUBMIT)[0]["price"],
        json!("59100"),
        "update replacement price"
    );

    // 10 cancel_orders
    drive(&rt, venue.cancel_orders(vec![resting.clone()])).unwrap();
    assert!(sent(&calls(&wire), "POST", CANCEL), "cancel_orders request");

    // 11 set_leverage: one request per side, long by its open position id
    drive(&rt, venue.set_leverage("BTCUSDT".into(), 20.0, None)).unwrap();
    assert_eq!(
        bodies(&calls(&wire), "POST", CHANGE_LEVERAGE),
        [
            json!({"positionId":"777","leverage":20}),
            json!({"symbol":"BTC_USDT","positionType":2,"leverage":20,"openType":1})
        ],
        "set_leverage payloads"
    );
    let fetched = store.fetched_leverage()["BTCUSDT"].clone();
    assert_eq!(
        fetched.buy_leverage.to_bits(),
        20f64.to_bits(),
        "buy leverage"
    );
    assert_eq!(
        fetched.sell_leverage.to_bits(),
        20f64.to_bits(),
        "sell leverage"
    );

    // 12 change_position_mode
    drive(&rt, venue.change_position_mode(false, None)).unwrap();
    let log = calls(&wire);
    assert_eq!(
        bodies(&log, "POST", CHANGE_MODE),
        [json!({"positionMode":2})],
        "change_position_mode payload"
    );
    assert!(
        sent(&log, "GET", POSITIONS),
        "position refresh after mode change"
    );
    assert!(!store.options().is_hedged, "one-way setting");

    // 13 change_margin_type
    drive(&rt, venue.change_margin_type(false, Some("BTCUSDT".into()))).unwrap();
    assert_eq!(
        bodies(&calls(&wire), "POST", CHANGE_LEVERAGE),
        [
            json!({"positionId":"777","leverage":20,"openType":2}),
            json!({"symbol":"BTC_USDT","positionType":2,"leverage":20,"openType":2})
        ],
        "change_margin_type payloads"
    );
    assert_eq!(
        store.fetched_leverage()["BTCUSDT"].margin_type,
        Some(MarginType::Cross),
        "cross margin"
    );

    // 14-16 inherited BaseExchange rejections (base.ts:799, :989, :1138)
    let max = drive(&rt, venue.fetch_symbol_max_leverage("BTCUSDT".into())).unwrap_err();
    assert_eq!(max.message, "Not implemented");
    let schedule = drive(&rt, venue.schedule_cancel(Some(Some(60_000.0)))).unwrap_err();
    assert_eq!(schedule.message, "Not implemented");
    let trades = drive(&rt, venue.fetch_public_trades("BTCUSDT".into(), None)).unwrap_err();
    assert_eq!(trades.message, "Not implemented");
    let log = calls(&wire);
    assert!(
        !log.iter().any(|(m, _, _)| m == "POST"),
        "inherited rejections send nothing: {log:?}"
    );

    // Further TS overrides: symbol and account-wide cancels, then dispose.
    drive(&rt, venue.cancel_symbol_orders("BTCUSDT".into())).unwrap();
    assert_eq!(
        bodies(&calls(&wire), "POST", CANCEL_ALL),
        [json!({"symbol":"BTC_USDT"})],
        "cancel_symbol_orders payload"
    );
    drive(&rt, venue.cancel_all_orders()).unwrap();
    assert_eq!(
        bodies(&calls(&wire), "POST", CANCEL_ALL),
        [json!({})],
        "cancel_all_orders payload"
    );
    venue.dispose().unwrap();
    assert!(venue.base().is_disposed(), "dispose reaches the base");
    assert!(
        rt.errors.borrow().is_empty(),
        "reported: {:?}",
        rt.errors.borrow()
    );
    assert!(
        !rt.console
            .borrow()
            .iter()
            .any(|m| m.contains("without a minInterval")),
        "tick loop uses the metadata interval"
    );
}
