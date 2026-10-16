//! Production OkxExchange through the Exchange trait over a scripted transport.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::*;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::*;
use serde_json::{Value, json};

const MARKETS: &str = "/api/v5/public/instruments";
const TICKERS: &str = "/api/v5/market/tickers";
const CONFIG: &str = "/api/v5/account/config";
const BALANCE: &str = "/api/v5/account/account-position-risk";
const POSITIONS: &str = "/api/v5/account/positions";
const LEVERAGE: &str = "/api/v5/account/leverage-info";
const ORDERS: &str = "/api/v5/trade/orders-pending";
const ALGO_ORDERS: &str = "/api/v5/trade/orders-algo-pending";
const PLACE: &str = "/api/v5/trade/batch-orders";
const AMEND: &str = "/api/v5/trade/amend-order";
const CANCEL: &str = "/api/v5/trade/cancel-batch-orders";
const SET_LEVERAGE: &str = "/api/v5/account/set-leverage";
const SET_MODE: &str = "/api/v5/account/set-position-mode";
const FILLS: &str = "/api/v5/trade/fills";
const FILLS_HISTORY: &str = "/api/v5/trade/fills-history";

fn instrument() -> Value {
    json!({
        "instId": "BTC-USDT-SWAP", "instType": "SWAP", "ctType": "linear", "ctVal": "0.01",
        "ctValCcy": "BTC", "settleCcy": "USDT", "uly": "BTC-USDT", "instFamily": "BTC-USDT",
        "baseCcy": "", "quoteCcy": "", "lotSz": "1", "tickSz": "0.1", "minSz": "1",
        "maxLmtSz": "100000", "maxMktSz": "5000", "maxStopSz": "5000", "maxTriggerSz": "100000",
        "maxTwapSz": "100000", "maxIcebergSz": "100000", "lever": "100", "state": "live"
    })
}

fn ticker() -> Value {
    json!({
        "instId": "BTC-USDT-SWAP", "last": "50000", "askPx": "50001", "bidPx": "49999",
        "open24h": "49000", "volCcy24h": "10", "vol24h": "1000"
    })
}

fn position() -> Value {
    json!({
        "instId": "BTC-USDT-SWAP", "instType": "SWAP", "posSide": "net", "pos": "10",
        "avgPx": "50000", "markPx": "50100", "upl": "1", "liqPx": "40000", "lever": "10",
        "mgnMode": "cross", "imr": "50", "mmr": "5", "ccy": "USDT", "notionalUsd": "5010",
        "cTime": "1700000000000", "uTime": "1700000000000", "posId": "1"
    })
}

fn order() -> Value {
    json!({
        "instId": "BTC-USDT-SWAP", "instType": "SWAP", "ordId": "o1", "clOrdId": "c1",
        "side": "buy", "posSide": "net", "ordType": "limit", "px": "49000", "sz": "10",
        "accFillSz": "0", "state": "live", "reduceOnly": "false", "lever": "10",
        "ccy": "", "cTime": "1700000000000", "uTime": "1700000000000"
    })
}

fn okx(http: &Http) {
    http.ok("GET", MARKETS, json!([instrument()]));
    http.ok("GET", TICKERS, json!([ticker()]));
    http.ok(
        "GET",
        CONFIG,
        json!([{"posMode": "net_mode", "acctLv": "2"}]),
    );
    http.ok(
        "GET",
        BALANCE,
        json!([{"adjEq": "1000", "balData": [{"ccy": "USDT", "eq": "1000"}], "posData": []}]),
    );
    http.ok("GET", POSITIONS, json!([position()]));
    http.ok(
        "GET",
        LEVERAGE,
        json!([{"instId": "BTC-USDT-SWAP", "lever": "10", "mgnMode": "cross"}]),
    );
    http.ok("GET", ORDERS, json!([order()]));
    http.ok("GET", ALGO_ORDERS, json!([]));
    http.ok(
        "POST",
        PLACE,
        json!([{"ordId": "p1", "clOrdId": "cp1", "sCode": "0", "sMsg": ""}]),
    );
    http.ok(
        "POST",
        AMEND,
        json!([{"ordId": "o1", "clOrdId": "c1", "sCode": "0", "sMsg": ""}]),
    );
    http.ok(
        "POST",
        CANCEL,
        json!([{"ordId": "o1", "clOrdId": "c1", "sCode": "0", "sMsg": ""}]),
    );
    http.ok(
        "POST",
        SET_LEVERAGE,
        json!([{"instId": "BTC-USDT-SWAP", "lever": "5", "mgnMode": "cross"}]),
    );
    http.ok("POST", SET_MODE, json!([{"posMode": "long_short_mode"}]));
    http.ok(
        "GET",
        FILLS,
        json!([{
            "instId": "BTC-USDT-SWAP", "tradeId": "t1", "ordId": "o1", "billId": "b1", "side": "buy",
            "posSide": "net", "fillPx": "50000", "fillSz": "2", "fee": "-0.5", "feeCcy": "USDT",
            "fillPnl": "0", "execType": "T", "ts": "1700000000000"
        }]),
    );
    http.ok("GET", FILLS_HISTORY, json!([]));
}

fn loaded(h: &Harness) {
    let markets = h.drive(h.venue().fetch_markets()).expect("markets");
    Store::update(
        &*h.store,
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .expect("store markets");
}

fn limit_order(amount: f64) -> PlaceOrderOpts {
    serde_json::from_value(json!({
        "symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": amount, "price": 49000
    }))
    .expect("PlaceOrderOpts")
}

#[test]
fn fetch_markets_maps_instruments() {
    let h = harness(ExchangeName::Okx, options(), okx);
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].symbol, "BTCUSDT");
    assert_eq!(markets[0].id, "BTC-USDT-SWAP");
    assert_eq!(markets[0].precision.amount, 1.0);
    assert_eq!(markets[0].limits.amount.min, 0.01);
    assert_eq!(h.http.calls(MARKETS)[0].params["instType"], "SWAP");
}

#[test]
fn fetch_tickers_parses_against_loaded_markets() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let tickers = h.drive(h.venue().fetch_tickers()).expect("fetchTickers");
    assert_eq!(tickers.len(), 1);
    assert_eq!(tickers[0].symbol, "BTCUSDT");
    assert_eq!(tickers[0].last, 50000.0);
    assert_eq!(tickers[0].bid, 49999.0);
}

#[test]
fn validate_account_signs_the_balance_read() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount");
    assert_eq!(serde_json::to_value(&result.error).unwrap(), json!(""));
    let calls = h.http.calls(BALANCE);
    assert_eq!(calls[0].headers["OK-ACCESS-KEY"], "fixture-key");
    assert_eq!(calls[0].headers["OK-ACCESS-PASSPHRASE"], "fixture-pass");
    assert!(calls[0].headers["OK-ACCESS-SIGN"].is_string());
}

#[test]
fn fetch_positions_maps_net_positions() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let positions = h
        .drive(h.venue().fetch_positions(None))
        .expect("fetchPositions");
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "BTCUSDT");
    assert_eq!(positions[0].contracts, 0.1);
    assert_eq!(positions[0].side, PositionSide::Long);
    assert_eq!(h.http.calls(CONFIG).len(), 1);
}

#[test]
fn fetch_orders_reads_normal_and_algo_pages() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "o1");
    assert_eq!(orders[0].price, 49000.0);
    assert!(!h.http.calls(ALGO_ORDERS).is_empty());
}

#[test]
fn place_order_batches_a_broker_tagged_limit() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let ids = h
        .drive(Box::pin(
            h.venue().base().place_order(h.venue(), limit_order(10.0)),
        ))
        .expect("placeOrder");
    assert_eq!(ids, vec!["p1"]);
    let sent = &h.http.calls(PLACE)[0].data;
    assert_eq!(sent[0]["instId"], "BTC-USDT-SWAP");
    assert_eq!(sent[0]["sz"], "10");
    assert!(
        sent[0]["clOrdId"]
            .as_str()
            .unwrap()
            .starts_with("e847386590ce4dBC")
    );
}

#[test]
fn place_orders_sends_one_batch() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let ids = h
        .drive(Box::pin(
            h.venue()
                .base()
                .place_orders(h.venue(), vec![limit_order(10.0)]),
        ))
        .expect("placeOrders");
    assert_eq!(ids, vec!["p1"]);
    assert_eq!(h.http.calls(PLACE).len(), 1);
}

#[test]
fn set_leverage_posts_cross_leverage() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let positions = h.drive(h.venue().fetch_positions(None)).expect("positions");
    Store::add_or_update_positions(&*h.store, positions).expect("store positions");
    h.drive(h.venue().set_leverage("BTCUSDT".into(), 5.0, None))
        .expect("setLeverage");
    let sent = &h.http.calls(SET_LEVERAGE)[0].data;
    assert_eq!(sent["instId"], "BTC-USDT-SWAP");
    assert_eq!(sent["lever"], "5");
    assert_eq!(sent["mgnMode"], "cross");
}

#[test]
fn change_position_mode_posts_long_short_mode() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    h.drive(h.venue().change_position_mode(true, None))
        .expect("changePositionMode");
    assert_eq!(h.http.calls(SET_MODE)[0].data["posMode"], "long_short_mode");
}

#[test]
fn change_margin_type_reports_cross_only_without_a_request() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let errors = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let sink = errors.clone();
    h.venue().base().on(
        "error",
        std::rc::Rc::new(move |_, data| {
            sink.borrow_mut().push(data.to_string());
            Ok(())
        }),
    );
    let before = h.http.requests.borrow().len();
    h.drive(h.venue().change_margin_type(true, None))
        .expect("changeMarginType resolves");
    assert_eq!(h.http.requests.borrow().len(), before);
    assert!(
        errors.borrow().iter().any(|e| e.contains("Only cross mode is supported")),
        "{errors:?}"
    );
}

#[test]
fn update_and_cancel_orders_hit_amend_and_cancel_batch() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    Store::update(
        &*h.store,
        StoreUpdate {
            orders: Some(orders.clone()),
            ..Default::default()
        },
        false,
    )
    .expect("store orders");
    let update: UpdateOrderOpts = serde_json::from_value(json!({
        "order": orders[0], "update": {"price": 48000}
    }))
    .expect("UpdateOrderOpts");
    h.drive(h.venue().update_order(update))
        .expect("updateOrder");
    let amended = &h.http.calls(AMEND)[0].data;
    assert_eq!(amended["ordId"], "o1");
    assert_eq!(amended["newPx"], "48000");
    h.drive(h.venue().cancel_orders(orders))
        .expect("cancelOrders");
    assert_eq!(h.http.calls(CANCEL)[0].data[0]["ordId"], "o1");
}

#[test]
fn fetch_historical_fills_maps_executions() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let opts: ExecutionsOptions = serde_json::from_value(json!({"symbol": "BTCUSDT"})).unwrap();
    let fills = h
        .drive(h.venue().fetch_historical_fills(opts))
        .expect("fills");
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].price, 50000.0);
}

#[test]
fn inherited_base_methods_reject_without_a_request() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let before = h.http.requests.borrow().len();
    for result in [
        h.drive(h.venue().get_account()).map(|_| ()),
        h.drive(h.venue().fetch_symbol_max_leverage("BTCUSDT".into()))
            .map(|_| ()),
        h.drive(h.venue().schedule_cancel(None)),
        h.drive(h.venue().fetch_public_trades("BTCUSDT".into(), None))
            .map(|_| ()),
    ] {
        assert_eq!(result.unwrap_err().message, "Not implemented");
    }
    assert_eq!(h.http.requests.borrow().len(), before);
}

#[test]
fn start_loads_markets_tickers_account_and_sockets() {
    let h = harness(ExchangeName::Okx, options(), okx);
    h.drive(h.venue().start()).expect("start");
    h.runtime.advance(50.0);
    {
        let loaded = h.store.loaded();
        assert!(loaded.markets && loaded.tickers);
        assert!(loaded.positions && loaded.orders && loaded.balance);
    }
    let urls: Vec<String> = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .map(|(u, _)| u.clone())
        .collect();
    assert!(
        urls.iter().any(|u| u.ends_with("/ws/v5/public")),
        "{urls:?}"
    );
    assert!(
        urls.iter().any(|u| u.ends_with("/ws/v5/private")),
        "{urls:?}"
    );
    h.venue().dispose().expect("dispose");
}
