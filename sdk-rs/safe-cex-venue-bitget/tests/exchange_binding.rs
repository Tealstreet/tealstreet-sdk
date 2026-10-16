//! Production BitgetExchange through the Exchange trait over a scripted transport.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::*;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::*;
use serde_json::{Value, json};

const MARKETS: &str = "/api/v2/mix/market/contracts";
const TICKERS: &str = "/api/v2/mix/market/tickers";

fn market_row() -> Value {
    json!({
        "symbol": "BTCUSDT", "baseCoin": "BTC", "quoteCoin": "USDT", "symbolStatus": "normal",
        "pricePlace": "1", "priceEndStep": "1", "sizeMultiplier": "0.001", "minTradeNum": "0.001",
        "maxOrderQty": "100", "minLever": "1", "maxLever": "125"
    })
}

fn ticker_row() -> Value {
    json!({
        "symbol": "BTCUSDT", "lastPr": "50000", "bidPr": "49999", "askPr": "50001", "indexPrice": "50000.5",
        "changeUtc24h": "0.01", "fundingRate": "0.0001", "baseVolume": "10", "quoteVolume": "500000"
    })
}

fn position_row() -> Value {
    json!({
        "symbol": "BTCUSDT", "holdSide": "long", "total": "0.01", "openPriceAvg": "50000", "markPrice": "50100",
        "marginMode": "crossed", "posMode": "one_way_mode", "leverage": "10", "unrealizedPL": "1",
        "liquidationPrice": "40000", "achievedProfits": "0", "totalFee": "0", "deductedFee": "0", "marginSize": "50"
    })
}

fn order_row() -> Value {
    json!({
        "orderId": "o1", "clientOid": "c1", "symbol": "BTCUSDT", "side": "buy", "orderType": "limit",
        "price": "49000", "size": "0.01", "baseVolume": "0", "status": "live", "tradeSide": "open",
        "posSide": "long", "reduceOnly": "NO", "force": "gtc", "cTime": "1700000000000", "posMode": "one_way_mode"
    })
}

/// A Classic (non-UTA) account: the v3 settings probe answers 40084.
fn classic(http: &Http) {
    http.ok("GET", "/api/v2/spot/account/info", json!({"userId": "123", "inviterId": "9", "parentId": null}));
    http.ok("GET", MARKETS, json!([market_row()]));
    http.ok("GET", TICKERS, json!([ticker_row()]));
    http.ok(
        "GET",
        "/api/v2/mix/account/accounts",
        json!([{"marginCoin": "USDT", "available": "100", "accountEquity": "120", "unrealizedPL": "2", "locked": "5"}]),
    );
    http.ok("GET", "/api/v2/mix/position/all-position", json!([position_row()]));
    http.ok("GET", "/api/v2/mix/order/orders-pending", json!({"entrustedList": [order_row()], "endId": "o1"}));
    http.ok("GET", "/api/v2/mix/order/orders-plan-pending", json!({"entrustedList": [], "endId": null}));
    http.ok("POST", "/api/v2/mix/account/set-position-mode", json!({"posMode": "hedge_mode"}));
    http.ok("POST", "/api/v2/mix/account/set-margin-mode", json!({}));
    http.ok("POST", "/api/v2/mix/account/set-leverage", json!({}));
    http.ok(
        "POST",
        "/api/v2/mix/order/batch-place-order",
        json!({"successList": [{"orderId": "p1", "clientOid": "cp1"}], "failureList": []}),
    );
    http.ok("POST", "/api/v2/mix/order/modify-order", json!({"orderId": "o1", "clientOid": "c1"}));
    http.ok(
        "POST",
        "/api/v2/mix/order/batch-cancel-orders",
        json!({"successList": [{"orderId": "o1", "clientOid": "c1"}], "failureList": []}),
    );
    http.route(
        "GET",
        "/api/v3/account/settings",
        400.0,
        json!({"code": "40084", "msg": "Classic Account mode", "requestTime": 1, "data": null}),
    );
}

fn loaded(h: &Harness) {
    let markets = h.drive(h.venue().fetch_markets()).expect("markets");
    Store::update(&*h.store, StoreUpdate { markets: Some(markets), ..Default::default() }, false)
        .expect("store markets");
}

#[test]
fn get_account_signs_and_maps_parent() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    let account = h.drive(h.venue().get_account()).expect("getAccount");
    assert_eq!(account.user_id, "123");
    let calls = h.http.calls("/api/v2/spot/account/info");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].headers["ACCESS-KEY"], "fixture-key");
    assert!(calls[0].headers["ACCESS-SIGN"].is_string(), "request interceptor signed the call");
}

#[test]
fn fetch_markets_maps_contracts() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].symbol, "BTCUSDT");
    assert_eq!(markets[0].limits.leverage.max, 125.0);
    assert_eq!(h.http.calls(MARKETS)[0].params["productType"], "usdt-futures");
}

#[test]
fn fetch_tickers_maps_known_markets() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    loaded(&h);
    let tickers = h.drive(h.venue().fetch_tickers()).expect("fetchTickers");
    assert_eq!(tickers.len(), 1);
    assert_eq!(tickers[0].last, 50000.0);
    assert_eq!(tickers[0].bid, 49999.0);
}

#[test]
fn inherited_base_methods_reject_like_ts() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    for result in [
        h.drive(h.venue().fetch_symbol_max_leverage("BTCUSDT".into())).map(|_| ()),
        h.drive(h.venue().schedule_cancel(Some(Some(1000.0)))),
        h.drive(h.venue().fetch_public_trades("BTCUSDT".into(), None)).map(|_| ()),
    ] {
        assert_eq!(result.expect_err("TS base rejects").message, "Not implemented");
    }
}

#[test]
fn probe_remaining_methods() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    loaded(&h);
    let report = |name: &str, r: Result<String, safe_cex_exchange::runtime::ExchangeError>| {
        eprintln!("PROBE {name}: {}", match r { Ok(v) => format!("ok {v}"), Err(e) => format!("err {}: {}", e.name, e.message) });
    };
    report("validate_account", h.drive(h.venue().validate_account()).map(|v| format!("{v:?}")));
    report("fetch_positions", h.drive(h.venue().fetch_positions(None)).map(|v| format!("{v:?}")));
    report("fetch_orders", h.drive(h.venue().fetch_orders(None)).map(|v| format!("{v:?}")));
    report("set_leverage", h.drive(h.venue().set_leverage("BTCUSDT".into(), 10.0, None)).map(|v| format!("{v:?}")));
    report("change_position_mode", h.drive(h.venue().change_position_mode(true, None)).map(|v| format!("{v:?}")));
    report("change_margin_type", h.drive(h.venue().change_margin_type(true, Some("BTCUSDT".into()))).map(|v| format!("{v:?}")));
    report("start", h.drive(h.venue().start()).map(|v| format!("{v:?}")));
    let positions = h.drive(h.venue().fetch_positions(None)).expect("positions");
    Store::update(&*h.store, StoreUpdate { positions: Some(positions), ..Default::default() }, false).expect("positions");
    report("set_leverage2", h.drive(h.venue().set_leverage("BTCUSDT".into(), 20.0, None)).map(|v| format!("{v:?}")));
    let opts: PlaceOrderOpts = serde_json::from_value(json!({"symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": 0.01, "price": 49000})).unwrap();
    report("place_order", h.drive(Box::pin(safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(h.exchange.as_ref(), opts))).map(|v| format!("{v:?}")));
    let orders = h.drive(h.venue().fetch_orders(None)).expect("orders");
    let update: UpdateOrderOpts = serde_json::from_value(json!({"order": orders[0], "update": {"price": 48000}})).unwrap();
    report("update_order", h.drive(h.venue().update_order(update)).map(|v| format!("{v:?}")));
    report("cancel_orders", h.drive(h.venue().cancel_orders(orders)).map(|v| format!("{v:?}")));
    for (url, s) in h.runtime.sockets.borrow().iter() { eprintln!("SOCK {url} {:?}", s.sent.borrow()); }
    for r in h.http.requests.borrow().iter() { eprintln!("REQ {} {} {}", r.method, r.url, r.params); }
}
