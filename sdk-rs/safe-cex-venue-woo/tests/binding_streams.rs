//! Socket classes, stream/data reads and fill-driven position recovery of the
//! production WooExchange, driven end to end over a scripted transport.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)] // Shared support: each test crate uses a subset.
mod binding_harness;
use binding_harness::{Harness, Socket, harness, options};
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

const ACCOUNT: &str = "/v3/accountinfo";
const MARKETS: &str = "/v1/public/info";
const TICKERS: &str = "/v1/public/futures";
const POSITIONS: &str = "/v3/positions";
const ORDERS: &str = "/v1/orders";
const ALGO_ORDERS: &str = "/v3/algo/orders";
const FUNDING: &str = "/v1/public/funding_rates";
const LEVERAGE: &str = "/v1/client/futures_leverage";
const ORDERBOOK: &str = "/v1/public/orderbook/PERP_BTC_USDT";
const KLINE: &str = "/v1/kline";
const KLINE_HISTORY: &str = "https://api-pub.woox.io/v1/hist/kline";
const PUBLIC_URL: &str = "wss://wss.woox.io/ws/stream/fixture-app";
const PRIVATE_URL: &str = "wss://wss.woox.io/v2/ws/private/stream/fixture-app";

fn with(extra: Value) -> ExchangeOptions {
    let mut base = serde_json::to_value(options()).unwrap();
    for (k, v) in extra.as_object().unwrap() {
        base[k] = v.clone();
    }
    serde_json::from_value(base).unwrap()
}

fn account(mode: &str) -> Value {
    json!({
        "applicationId": "app-1", "referrerID": "", "accountMode": "FUTURES",
        "positionMode": mode, "freeCollateral": 10, "totalAccountValue": 20, "totalCollateral": 15,
    })
}

fn public_routes(http: &binding_harness::Http) {
    http.route(
        "GET",
        MARKETS,
        200.0,
        json!({"success": true, "rows": [
            {"symbol": "PERP_BTC_USDT", "base_imr": 0.02, "base_tick": 0.0001,
             "quote_tick": 0.1, "base_min": 0.0001, "base_max": 100},
        ]}),
    );
    http.route(
        "GET",
        TICKERS,
        200.0,
        json!({"success": true, "rows": [
            {"symbol": "PERP_BTC_USDT", "24h_open": 100, "24h_close": 101, "index_price": 101,
             "mark_price": 101, "last_funding_rate": 0.0001, "next_funding_time": 1, "24h_volume": 1,
             "24h_amount": 1, "open_interest": 1},
        ]}),
    );
}

fn private_routes(http: &binding_harness::Http, mode: &str) {
    public_routes(http);
    http.ok("GET", ACCOUNT, account(mode));
    http.ok("GET", POSITIONS, json!({"positions": []}));
    http.route(
        "GET",
        ORDERS,
        200.0,
        json!({"success": true, "rows": [], "meta": {"total": 0, "records_per_page": 25}}),
    );
    http.route(
        "GET",
        ALGO_ORDERS,
        200.0,
        json!({"success": true, "data": {"rows": [], "meta": {"total": 0, "records_per_page": 25}}}),
    );
}

fn json_of<T: serde::Serialize + ?Sized>(value: &T) -> Value {
    serde_json::to_value(value).unwrap()
}

fn socket(h: &Harness, url: &str) -> Rc<Socket> {
    h.runtime
        .sockets
        .borrow()
        .iter()
        .find(|(u, _)| u == url)
        .map(|(_, s)| s.clone())
        .unwrap_or_else(|| panic!("no socket dialled to {url}"))
}

fn sent(s: &Socket) -> Vec<Value> {
    s.sent
        .borrow()
        .iter()
        .map(|t| serde_json::from_str(t).unwrap())
        .collect()
}

fn push(h: &Harness, s: &Socket, frame: Value) {
    s.events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
    h.runtime.pump();
}

fn listen(h: &Harness, event: &str) -> Rc<RefCell<Vec<Value>>> {
    let seen: Rc<RefCell<Vec<Value>>> = Rc::default();
    let sink = seen.clone();
    h.venue().base().on(
        event,
        Rc::new(move |_, data| {
            sink.borrow_mut().push(
                safe_cex_exchange::js_runtime::value::to_json(data).expect("finite fixture event"),
            );
            Ok(())
        }),
    );
    seen
}

fn ticker(h: &Harness) -> Value {
    json_of(&*h.store.tickers())[0].clone()
}

fn started(extra: Value, script: impl FnOnce(&binding_harness::Http)) -> Harness {
    let h = harness(ExchangeName::Woo, with(extra), script);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    h
}

#[test]
fn start_connects_the_public_socket_and_its_frames_update_the_store() {
    let h = started(json!({"publicDataOnly": true}), |http| {
        public_routes(http);
        http.route(
            "GET",
            FUNDING,
            200.0,
            json!({"success": true, "rows": [
                {"symbol": "PERP_BTC_USDT", "est_funding_rate": "0.0005", "next_funding_time": 1700000060000.0},
            ]}),
        );
    });
    assert_eq!(
        h.runtime.sockets.borrow().len(),
        1,
        "public data only dials no private socket"
    );
    let public = socket(&h, PUBLIC_URL);
    assert_eq!(
        sent(&public),
        vec![
            json!({"event": "ping"}),
            json!({"event": "subscribe", "topic": "tickers"}),
            json!({"event": "subscribe", "topic": "bbos"}),
            json!({"event": "subscribe", "topic": "markprices"}),
        ]
    );
    push(
        &h,
        &public,
        json!({"topic": "tickers", "data": [
            {"symbol": "PERP_BTC_USDT", "close": 105.5, "amount": 5000, "volume": 50},
            {"symbol": "SPOT_BTC_USDT", "close": 1, "amount": 1, "volume": 1},
        ]}),
    );
    push(
        &h,
        &public,
        json!({"topic": "bbos", "data": [{"symbol": "PERP_BTC_USDT", "bid": 105.4, "ask": 105.6}]}),
    );
    push(
        &h,
        &public,
        json!({"topic": "markprices", "data": [{"symbol": "PERP_BTC_USDT", "price": 105.45}]}),
    );
    let t = ticker(&h);
    assert_eq!(
        (
            t["last"].clone(),
            t["quoteVolume"].clone(),
            t["volume"].clone()
        ),
        (json!(105.5), json!(5000.0), json!(50.0))
    );
    assert_eq!(
        (t["bid"].clone(), t["ask"].clone()),
        (json!(105.4), json!(105.6))
    );
    assert_eq!(t["mark"], json!(105.45));
    push(&h, &public, json!({"event": "ping"}));
    assert_eq!(sent(&public).last().unwrap(), &json!({"event": "pong"}));

    // startFundingRatesRefresh: one refresh per FUNDING_REFRESH_INTERVAL_MS, cleared by dispose.
    assert!(h.http.calls(FUNDING).is_empty());
    h.runtime.advance(60_000.0);
    assert_eq!(h.http.calls(FUNDING).len(), 1);
    let t = ticker(&h);
    assert_eq!(
        (t["fundingRate"].clone(), t["fundingTime"].clone()),
        (json!(0.0005), json!(1700000060000.0))
    );
    h.venue().dispose().unwrap();
    h.runtime.advance(60_000.0);
    h.runtime.advance(60_000.0);
    assert_eq!(
        h.http.calls(FUNDING).len(),
        1,
        "dispose clears the funding interval"
    );
}

#[test]
fn private_socket_authenticates_before_it_subscribes() {
    let h = started(json!({}), |http| private_routes(http, "ONE_WAY"));
    let private = socket(&h, PRIVATE_URL);
    let frames = sent(&private);
    assert_eq!(frames.len(), 1, "only auth before the auth ack: {frames:?}");
    let auth = &frames[0];
    assert_eq!(auth["event"], "auth");
    assert_eq!(auth["params"]["apikey"], "fixture-key");
    let ts = auth["params"]["timestamp"].as_f64().unwrap();
    let expected = safe_cex_crypto::hmac_sha256(
        b"fixture-secret",
        format!("|{ts}").as_bytes(),
        safe_cex_crypto::Encoding::Hex,
    );
    assert_eq!(auth["params"]["sign"], json!(expected));
    push(&h, &private, json!({"event": "auth", "success": true}));
    assert_eq!(
        sent(&private)[1..].to_vec(),
        vec![
            json!({"topic": "executionreport", "event": "subscribe"}),
            json!({"topic": "algoexecutionreportv2", "event": "subscribe"}),
            json!({"event": "ping"}),
        ]
    );
}

#[test]
fn private_auth_failure_is_reported_and_leaves_it_unsubscribed() {
    let h = started(json!({}), |http| private_routes(http, "ONE_WAY"));
    let errors = listen(&h, "error");
    let private = socket(&h, PRIVATE_URL);
    push(
        &h,
        &private,
        json!({"event": "auth", "success": false, "message": "signature mismatch"}),
    );
    assert_eq!(
        sent(&private).len(),
        1,
        "no subscription after a failed auth"
    );
    let text = errors
        .borrow()
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("|");
    assert!(text.contains("signature mismatch"), "{text}");
    assert!(!text.contains("Remote WS signing unavailable"), "{text}");
}

fn authed(mode: &str, script: impl FnOnce(&binding_harness::Http)) -> (Harness, Rc<Socket>) {
    let h = started(json!({}), |http| {
        script(http);
        private_routes(http, mode);
    });
    let private = socket(&h, PRIVATE_URL);
    push(&h, &private, json!({"event": "auth", "success": true}));
    // Let the 4-per-second request limiter drain the start-up reads.
    h.runtime.advance(1100.0);
    (h, private)
}

fn order(h: &Harness, id: &str) -> Option<Value> {
    json_of(&*h.store.orders())
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id)
        .cloned()
}

#[test]
fn execution_reports_update_store_orders_and_emit_fills() {
    let (h, private) = authed("HEDGE_MODE", |_| {});
    let fills = listen(&h, "fill");
    let limit = json!({"symbol": "PERP_BTC_USDT", "orderId": 11, "side": "BUY", "type": "LIMIT",
        "quantity": 2, "price": 100, "reduceOnly": false, "executedQuantity": 0, "executedPrice": 0});
    let mut new = limit.clone();
    new["status"] = json!("NEW");
    push(
        &h,
        &private,
        json!({"topic": "executionreport", "data": new}),
    );
    let stored = order(&h, "11").expect("NEW report adds the order");
    assert_eq!(
        (stored["status"].clone(), stored["side"].clone()),
        (json!("open"), json!("buy"))
    );
    assert!(fills.borrow().is_empty());

    let mut filled = limit.clone();
    filled["status"] = json!("FILLED");
    filled["executedQuantity"] = json!(2);
    filled["executedPrice"] = json!(100.5);
    filled["executedTimestamp"] = json!(1700000001234.0);
    push(
        &h,
        &private,
        json!({"topic": "executionreport", "data": filled}),
    );
    assert_eq!(
        fills.borrow()[0],
        json!({"id": "11", "orderId": "11", "side": "buy", "symbol": "BTCUSDT",
               "price": 100.5, "amount": 2.0, "timestamp": 1700000001.0})
    );

    let algo = json!({"symbol": "PERP_BTC_USDT", "algoOrderId": 77, "rootAlgoOrderId": 70,
        "algoType": "STOP_LOSS", "side": "SELL", "quantity": 1, "triggerPrice": 90,
        "triggerPriceType": "MARK_PRICE", "reduceOnly": true, "totalExecutedQuantity": 0});
    let mut new = algo.clone();
    new["algoStatus"] = json!("NEW");
    push(
        &h,
        &private,
        json!({"topic": "algoexecutionreportv2", "data": [new]}),
    );
    let stored = order(&h, "77").expect("NEW algo report adds the order");
    assert_eq!(
        (stored["type"].clone(), stored["price"].clone()),
        (json!("stop_market"), json!(90.0))
    );

    let mut filled = algo.clone();
    filled["algoStatus"] = json!("FILLED");
    filled["averageExecutedPrice"] = json!(89.5);
    filled["totalExecutedQuantity"] = json!(1);
    push(
        &h,
        &private,
        json!({"topic": "algoexecutionreportv2", "data": filled}),
    );
    assert_eq!(
        fills.borrow()[1],
        json!({"id": "77", "orderId": "77", "side": "sell", "symbol": "BTCUSDT",
               "price": 89.5, "amount": 1.0})
    );
    assert_eq!(order(&h, "11").unwrap()["status"], "closed");
    assert_eq!(order(&h, "77").unwrap()["status"], "closed");
}

// No mapper override: reports pass through the real socket -> fill host -> mapper.
fn terminal_market_report(kind: &str, price: f64) {
    let (h, private) = authed("HEDGE_MODE", |http| {
        http.ok("GET", LEVERAGE, json!({"details":[{"leverage":"10"}]}));
    });
    let fills = listen(&h, "fill");
    let errors = listen(&h, "error");
    let base = json!({"symbol":"PERP_BTC_USDT","orderId":811,"side":"BUY",
        "type":"LIMIT","status":"NEW","quantity":2,"price":100,
        "reduceOnly":false,"executedQuantity":0,"executedPrice":0});
    push(&h, &private, json!({"topic":"executionreport","data":base}));
    assert_eq!(order(&h, "811").unwrap()["status"], "open");
    let terminal = json!({"symbol":"PERP_BTC_USDT","orderId":811,"side":"BUY",
        "type":kind,"status":"FILLED","quantity":2,"price":price,
        "reduceOnly":false,"executedQuantity":0.1,"executedPrice":100.5,
        "executedTimestamp":1700000001234.0});
    push(
        &h,
        &private,
        json!({"topic":"executionreport","data":terminal}),
    );
    assert_eq!(
        fills.borrow().len(),
        1,
        "{kind}: terminal report must emit fill through the real mapper; errors={:?}",
        errors.borrow()
    );
    assert_eq!(
        fills.borrow()[0],
        json!({"id":"811","orderId":"811","side":"buy",
        "symbol":"BTCUSDT","price":100.5,"amount":0.1,"timestamp":1700000001.0})
    );
    assert_eq!(
        order(&h, "811").unwrap()["status"],
        "closed",
        "{kind}: terminal status must survive mapping"
    );
    assert!(errors.borrow().is_empty(), "{kind}: {:?}", errors.borrow());
}

#[test]
fn ioc_execution_report_emits_fill_and_closes_order_through_real_mapper() {
    terminal_market_report("IOC", 101.0);
}

#[test]
fn fok_execution_report_emits_fill_and_closes_order_through_real_mapper() {
    terminal_market_report("FOK", 101.0);
}

#[test]
fn zero_price_market_execution_report_emits_fill_and_closes_order_through_real_mapper() {
    terminal_market_report("MARKET", 0.0);
}

fn position_rows(h: &Harness) -> Vec<Value> {
    json_of(&*h.store.positions()).as_array().unwrap().clone()
}

#[test]
fn a_fill_refreshes_the_symbol_position_and_dispose_clears_the_retry() {
    let (h, private) = authed("ONE_WAY", |http| {
        http.ok("GET", LEVERAGE, json!({"details": [{"leverage": "10"}]}));
    });
    let before = h.http.calls(POSITIONS).len();
    let fill = json!({"symbol": "PERP_BTC_USDT", "orderId": 12, "side": "BUY", "type": "LIMIT",
        "quantity": 0.5, "price": 100, "reduceOnly": false, "status": "FILLED", "executedQuantity": 0.5, "executedPrice": 100});
    push(
        &h,
        &private,
        json!({"topic": "executionreport", "data": fill}),
    );
    // The zero-delay refresh reads the symbol's positions; the empty read leaves only
    // the zero-size metadata row, mismatching the optimistic signature.
    assert_eq!(h.http.calls(POSITIONS).len(), before + 1);
    assert_eq!(h.http.calls(LEVERAGE)[0].params["symbol"], "PERP_BTC_USDT");
    let btc = position_rows(&h)
        .into_iter()
        .find(|p| p["symbol"] == "BTCUSDT")
        .unwrap();
    assert_eq!(
        (btc["contracts"].clone(), btc["leverage"].clone()),
        (json!(0.0), json!(10.0))
    );
    h.runtime.advance(100.0);
    assert_eq!(
        h.http.calls(POSITIONS).len(),
        before + 1,
        "the retry waits 150ms"
    );
    h.runtime.advance(50.0);
    assert_eq!(
        h.http.calls(POSITIONS).len(),
        before + 2,
        "signature mismatch retries"
    );
    h.venue().dispose().unwrap();
    assert!(
        h.venue().base().is_disposed(),
        "dispose reaches the BaseExchange core"
    );
    h.runtime.advance(300.0);
    h.runtime.advance(600.0);
    assert_eq!(
        h.http.calls(POSITIONS).len(),
        before + 2,
        "dispose cleared the retry timer"
    );
}

#[test]
fn a_fill_matching_the_refreshed_position_stops_retrying() {
    let (h, private) = authed("ONE_WAY", |_| {});
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, p, _)| p != POSITIONS);
    h.http.ok(
        "GET",
        POSITIONS,
        json!({"positions": [
        {"symbol": "PERP_BTC_USDT", "holding": 0.5, "averageOpenPrice": 100,
         "leverage": 10, "estLiqPrice": 50, "marginMode": "CROSS"}]}),
    );
    let before = h.http.calls(POSITIONS).len();
    let fill = json!({"symbol": "PERP_BTC_USDT", "orderId": 13, "side": "BUY", "type": "LIMIT",
        "quantity": 0.5, "price": 100, "reduceOnly": false, "status": "FILLED", "executedQuantity": 0.5, "executedPrice": 100});
    push(
        &h,
        &private,
        json!({"topic": "executionreport", "data": fill}),
    );
    h.runtime.advance(1.0);
    h.runtime.advance(150.0);
    h.runtime.advance(300.0);
    assert_eq!(
        h.http.calls(POSITIONS).len(),
        before + 1,
        "matching signature: no retry"
    );
    let btc = position_rows(&h)
        .into_iter()
        .find(|p| p["symbol"] == "BTCUSDT")
        .unwrap();
    assert_eq!(
        (btc["entryPrice"].clone(), btc["leverage"].clone()),
        (json!(100.0), json!(10.0))
    );
}

#[test]
fn fetch_positions_appends_zero_size_rows_from_metadata() {
    let h = started(json!({}), |http| {
        private_routes(http, "HEDGE_MODE");
        http.ok("GET", LEVERAGE, json!({"details": [{"leverage": "20"}]}));
    });
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, p, _)| p != POSITIONS);
    h.http.ok(
        "GET",
        POSITIONS,
        json!({"positions": [
        {"symbol": "PERP_BTC_USDT", "holding": 0.25, "averageOpenPrice": 100,
         "leverage": 20, "estLiqPrice": 60, "marginMode": "CROSS"}]}),
    );
    let leverage_reads = h.http.calls(LEVERAGE).len();
    let positions = json_of(
        &h.drive(
            h.venue()
                .fetch_positions(Some(vec!["BTCUSDT".into(), "ETHUSDT".into()])),
        )
        .unwrap(),
    );
    let rows: Vec<(Value, Value, Value)> = positions
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["id"].clone(),
                p["contracts"].clone(),
                p["leverage"].clone(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (json!("BTCUSDT:long"), json!(0.25), json!(20.0)),
            (json!("BTCUSDT:short"), json!(0.0), json!(20.0)),
            (json!("ETHUSDT:long"), json!(0.0), json!(20.0)),
            (json!("ETHUSDT:short"), json!(0.0), json!(20.0)),
        ]
    );
    let fake = &positions[1];
    for k in [
        "entryPrice",
        "notional",
        "unrealizedPnl",
        "liquidationPrice",
    ] {
        assert_eq!(fake[k], json!(0.0), "{k}");
    }
    assert_eq!(
        (fake["marginType"].clone(), fake["positionMode"].clone()),
        (json!("cross"), json!("hedged"))
    );
    let reads = h.http.calls(LEVERAGE)[leverage_reads..].to_vec();
    let symbols: Vec<Value> = reads.iter().map(|r| r.params["symbol"].clone()).collect();
    assert_eq!(
        symbols,
        vec![json!("PERP_BTC_USDT"), json!("PERP_ETH_USDT")]
    );
    assert_eq!(reads[0].params["position_mode"], "ONE_WAY");
    assert_eq!(reads[0].params["margin_mode"], "CROSS");
}

#[test]
fn fetch_positions_without_symbols_targets_open_positions_and_open_orders() {
    let h = started(json!({}), |http| {
        private_routes(http, "ONE_WAY");
        http.ok("GET", LEVERAGE, json!({"details": [{"leverage": "5"}]}));
    });
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, p, _)| p != POSITIONS);
    h.http.ok(
        "GET",
        POSITIONS,
        json!({"positions": [
        {"symbol": "PERP_BTC_USDT", "holding": -1, "averageOpenPrice": 100,
         "leverage": 5, "estLiqPrice": 150, "marginMode": "CROSS"}]}),
    );
    let leverage_reads = h.http.calls(LEVERAGE).len();
    let positions = json_of(&h.drive(h.venue().fetch_positions(None)).unwrap());
    let ids: Vec<Value> = positions
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].clone())
        .collect();
    assert_eq!(
        ids,
        vec![json!("BTCUSDT")],
        "one-way open position needs no metadata row"
    );
    assert_eq!(h.http.calls(LEVERAGE).len(), leverage_reads);
}

#[test]
fn post_only_placement_reads_the_touch_price_from_the_order_book() {
    let h = started(json!({}), |http| {
        private_routes(http, "ONE_WAY");
        http.route(
            "GET",
            ORDERBOOK,
            200.0,
            json!({"success": true,
            "asks": [{"price": "100.2"}, {"price": 100.3}], "bids": [{"price": 99.9}]}),
        );
    });
    let opts: safe_cex_types::PlaceOrderOpts =
        serde_json::from_value(json!({"symbol": "BTCUSDT", "type": "limit", "side": "buy",
        "amount": 1, "price": 100.5, "timeInForce": "PostOnly"}))
        .unwrap();
    let result = h.drive(Box::pin(h.venue().base().place_order(h.venue(), opts)));
    assert_eq!(
        result.map_err(|e| e.message),
        Err("WOO post-only buy order would cross current ask 100.2".to_string())
    );
    assert_eq!(h.http.calls(ORDERBOOK).len(), 1);
}

#[test]
fn fetch_ohlcv_routes_recent_reads_to_kline_and_history_to_the_proxy_client() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route("GET", KLINE, 200.0, json!({"success": true, "rows": [
            {"start_timestamp": 1699999940000.0, "open": 2, "high": 3, "low": 1, "close": 2.5, "volume": 4},
            {"start_timestamp": 1699999880000.0, "open": 1, "high": 2, "low": 1, "close": 2, "volume": 3},
        ]}));
        http.route("GET", KLINE_HISTORY, 200.0, json!({"success": true, "data": {"rows": [
            {"start_timestamp": 1600000000000.0, "open": 9, "high": 9, "low": 9, "close": 9, "volume": 9},
        ]}}));
    });
    // ts: without `to` the distance is measured from epoch 0, so only a `to` near now reads KLINE.
    let recent = serde_json::from_value(
        json!({"symbol": "BTCUSDT", "interval": "1m", "to": 1_700_000_000_000.0_f64}),
    )
    .unwrap();
    let candles = json_of(&h.drive(h.venue().fetch_ohlcv(recent)).unwrap());
    assert_eq!(
        candles[0]["timestamp"],
        json!(1699999880.0),
        "rows are reversed"
    );
    assert_eq!(candles[1]["close"], json!(2.5));
    assert_eq!(
        h.http.calls(KLINE)[0].params,
        json!({"symbol": "PERP_BTC_USDT", "type": "1m", "limit": "500"})
    );
    let history = serde_json::from_value(json!({"symbol": "BTCUSDT", "interval": "1m",
        "from": 1600000000000.0, "to": 1600060000000.0, "limit": 5000}))
    .unwrap();
    let candles = json_of(&h.drive(h.venue().fetch_ohlcv(history)).unwrap());
    assert_eq!(candles[0]["timestamp"], json!(1600000000.0));
    assert_eq!(
        h.http.calls(KLINE_HISTORY)[0].params,
        json!({"symbol": "PERP_BTC_USDT", "type": "1m", "start_time": "1600000000000",
               "end_time": "1600060000000", "size": "1000"})
    );
}

#[test]
fn listen_order_book_joins_the_snapshot_with_buffered_deltas() {
    let h = started(json!({"publicDataOnly": true}), |http| {
        public_routes(http);
        http.route(
            "GET",
            ORDERBOOK,
            500.0,
            json!({"success": false, "message": "busy"}),
        );
        http.route(
            "GET",
            ORDERBOOK,
            500.0,
            json!({"success": false, "message": "busy"}),
        );
        // The v1 orderbook response carries the book at the top level.
        http.route(
            "GET",
            ORDERBOOK,
            200.0,
            json!({"success": true, "timestamp": 1000,
            "bids": [{"price": 100, "quantity": 1}, {"price": 99, "quantity": 2}],
            "asks": [{"price": 101, "quantity": 1}]}),
        );
    });
    let books: Rc<RefCell<Vec<Value>>> = Rc::default();
    let sink = books.clone();
    let public = socket(&h, PUBLIC_URL);
    let topic = "PERP_BTC_USDT@orderbookupdate";
    let cleanup = h
        .venue()
        .listen_order_book(
            "BTCUSDT".into(),
            Rc::new(move |b| sink.borrow_mut().push(json_of(&b))),
        )
        .unwrap();
    h.runtime.pump();
    assert!(sent(&public).contains(&json!({"event": "subscribe", "topic": topic})));
    assert_eq!(
        h.http.calls(ORDERBOOK).len(),
        1,
        "snapshot is requested immediately"
    );
    assert!(
        books.borrow().is_empty(),
        "a failed snapshot publishes nothing"
    );
    // Older than the snapshot: buffered, then dropped at the join.
    push(
        &h,
        &public,
        json!({"topic": topic, "ts": 500, "data": {"bids": [[97, 9]], "asks": []}}),
    );
    assert!(books.borrow().is_empty());
    push(
        &h,
        &public,
        json!({"topic": topic, "ts": 2000,
        "data": {"bids": [[100, 0], [98, 5]], "asks": [[101.5, 3]]}}),
    );
    // ts: rateLimit(4 per 1000ms) holds the third read until the window frees.
    h.runtime.advance(1000.0);
    assert_eq!(h.http.calls(ORDERBOOK).len(), 3);
    assert_eq!(
        books.borrow().len(),
        1,
        "the snapshot joins the buffered deltas once"
    );
    let book = books.borrow()[0].clone();
    assert_eq!(
        book["bids"],
        json!([
            {"price": 99.0, "amount": 2.0, "total": 2.0},
            {"price": 98.0, "amount": 5.0, "total": 7.0},
        ])
    );
    assert_eq!(
        book["asks"],
        json!([
            {"price": 101.0, "amount": 1.0, "total": 1.0},
            {"price": 101.5, "amount": 3.0, "total": 4.0},
        ])
    );
    push(
        &h,
        &public,
        json!({"topic": topic, "ts": 3000, "data": {"bids": [[99, 1]], "asks": []}}),
    );
    assert_eq!(
        books.borrow().last().unwrap()["bids"][0],
        json!({"price": 99.0, "amount": 1.0, "total": 1.0})
    );
    cleanup().unwrap();
    assert_eq!(
        sent(&public).last().unwrap(),
        &json!({"event": "unsubscribe", "topic": topic})
    );
}

#[test]
fn listen_ohlcv_streams_kline_candles_and_ignores_older_ones() {
    let h = started(json!({"publicDataOnly": true}), public_routes);
    let candles: Rc<RefCell<Vec<Value>>> = Rc::default();
    let sink = candles.clone();
    let public = socket(&h, PUBLIC_URL);
    let topic = "PERP_BTC_USDT@kline_1m";
    let options = serde_json::from_value(json!({"symbol": "BTCUSDT", "interval": "1m"})).unwrap();
    let cleanup = h
        .venue()
        .listen_ohlcv(
            options,
            Rc::new(move |c| sink.borrow_mut().push(json_of(&c))),
        )
        .unwrap();
    h.runtime.pump();
    let subscriptions = sent(&public);
    assert!(subscriptions.contains(&json!({"event": "subscribe", "topic": topic})));
    // ts: publicDataOnly skips the secondary @trades topic.
    assert!(
        !subscriptions.contains(&json!({"event": "subscribe", "topic": "PERP_BTC_USDT@trades"}))
    );
    let kline = |start: f64, close: f64| {
        json!({"topic": topic, "ts": start, "data": {"startTime": start, "open": 1.0, "high": 3.0,
            "low": 0.5, "close": close, "volume": 7.0}})
    };
    push(&h, &public, kline(1_700_000_040_000.0, 2.0));
    push(&h, &public, kline(1_700_000_040_000.0, 2.5));
    push(&h, &public, kline(1_700_000_000_000.0, 9.0));
    let seen = candles.borrow().clone();
    assert_eq!(seen.len(), 2, "the older candle is dropped");
    assert_eq!(seen[0]["timestamp"], json!(1_700_000_040.0));
    assert_eq!(seen[1]["close"], json!(2.5));
    cleanup().unwrap();
    // ts: cleanup unsubscribes @trades too, even when publicDataOnly never subscribed it.
    let tail = sent(&public);
    assert_eq!(
        tail[tail.len() - 2..],
        [
            json!({"event": "unsubscribe", "topic": topic}),
            json!({"event": "unsubscribe", "topic": "PERP_BTC_USDT@trades"}),
        ]
    );
}

#[test]
fn shared_order_boundary_new_ioc_must_be_stored() {
    let (h, private) = authed("HEDGE_MODE", |http| {
        http.ok("GET", LEVERAGE, json!({"details":[{"leverage":"10"}]}));
    });
    push(
        &h,
        &private,
        json!({"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":991,"side":"BUY","type":"IOC","status":"NEW","quantity":2,"price":101,"reduceOnly":false}}),
    );
    assert!(
        order(&h, "991").is_some(),
        "NEW IOC must reach store through actual mapper"
    );
}
#[test]
fn shared_order_boundary_new_zero_price_market_must_be_stored() {
    let (h, private) = authed("HEDGE_MODE", |http| {
        http.ok("GET", LEVERAGE, json!({"details":[{"leverage":"10"}]}));
    });
    push(
        &h,
        &private,
        json!({"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":992,"side":"BUY","type":"MARKET","status":"NEW","quantity":2,"price":0,"reduceOnly":false}}),
    );
    assert!(
        order(&h, "992").is_some(),
        "NEW MARKET undefined price must reach store through actual mapper"
    );
}
#[test]
fn shared_order_boundary_partial_ioc_must_emit_fill() {
    let (h, private) = authed("HEDGE_MODE", |http| {
        http.ok("GET", LEVERAGE, json!({"details":[{"leverage":"10"}]}));
    });
    let fills = listen(&h, "fill");
    push(
        &h,
        &private,
        json!({"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":993,"side":"BUY","type":"IOC","status":"PARTIAL_FILLED","quantity":2,"price":101,"reduceOnly":false,"executedQuantity":0.1,"executedPrice":100.5}}),
    );
    assert_eq!(
        fills.borrow().len(),
        1,
        "PARTIAL IOC must pass shared Store before emitting fill"
    );
}

fn journal_order(id: &str, side: &str, at: f64, pnl: Option<f64>) -> Value {
    let mut row = json!({"order_id":id,"symbol":"PERP_BTC_USDT","side":side,
        "type":"MARKET","status":"FILLED","quantity":"2","executed":"2",
        "average_executed_price":if side=="BUY" {"100"} else {"110"},
        "created_time":at,"updated_time":at,"total_fee":"0.1","fee_asset":"USDT"});
    if let Some(pnl) = pnl {
        row["realized_pnl"] = json!(pnl);
    }
    row
}

fn historical_positions(rows: Value, from: f64) -> (Harness, Vec<Value>) {
    let h = harness(ExchangeName::Woo, options(), |http| {
        public_routes(http);
        http.route(
            "GET",
            ORDERS,
            200.0,
            json!({"success":true,"rows":rows,"meta":{"total":2,"records_per_page":100}}),
        );
    });
    h.drive(h.venue().refresh_markets()).unwrap();
    let result = h
        .drive(h.venue().fetch_historical_positions(
            safe_cex_exchange::journal::HistoricalPositionOptions {
                account: "journal-fixture".into(),
                from: Some(from),
                to: Some(1_700_000_100_000.0),
            },
        ))
        .expect("WOO journal must read historical orders through the production binding");
    (h, result)
}

#[test]
fn real_journal_folds_historical_orders_with_reported_pnl() {
    let (h, records) = historical_positions(
        json!([
            journal_order("entry", "BUY", 1_699_999_900.0, Some(0.0)),
            journal_order("exit", "SELL", 1_700_000_000.0, Some(20.0)),
        ]),
        1_699_999_800_000.0,
    );
    assert_eq!(
        records.len(),
        1,
        "real WOO order history must produce the closed round trip"
    );
    let r = &records[0];
    assert_eq!(r["account"], "journal-fixture");
    assert_eq!(r["symbol"], "BTCUSDT");
    assert_eq!(r["settleCurrency"], "USDT");
    assert_eq!(r["openedAt"], 1_699_999_900_000.0);
    assert_eq!(r["closedAt"], 1_700_000_000_000.0);
    assert_eq!(r["realizedPnl"], 20.0);
    assert_eq!(r["fees"], 0.2);
    assert_eq!(r["avgEntryPrice"], 100.0);
    assert_eq!(r["avgExitPrice"], 110.0);
    assert_eq!(r["provenance"], "derived");
    let calls = h.http.calls(ORDERS);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["start_t"], "1699999800000");
    assert_eq!(calls[0].params["end_t"], "1700000100000");
    assert_eq!(calls[0].params["realized_pnl"], "true");
    h.venue().dispose().unwrap();
}

#[test]
fn real_journal_refuses_missing_close_pnl_and_filters_older_closes() {
    for (pnl, from) in [
        (None, 1_699_999_800_000.0),
        (Some(20.0), 1_700_000_001_000.0),
    ] {
        let (h, records) = historical_positions(
            json!([
                journal_order("entry", "BUY", 1_699_999_900.0, Some(0.0)),
                journal_order("exit", "SELL", 1_700_000_000.0, pnl),
            ]),
            from,
        );
        assert!(
            records.is_empty(),
            "missing close PnL and closes before the requested window do not produce journal records"
        );
        assert_eq!(h.http.calls(ORDERS).len(), 1);
        h.venue().dispose().unwrap();
    }
}

#[test]
fn real_journal_does_not_report_an_open_interval() {
    let (h, records) = historical_positions(
        json!([journal_order("entry", "BUY", 1_699_999_900.0, Some(0.0))]),
        1_699_999_800_000.0,
    );
    assert!(
        records.is_empty(),
        "an open interval is not a closed journal record"
    );
    assert_eq!(h.http.calls(ORDERS).len(), 1);
    h.venue().dispose().unwrap();
}

#[test]
fn real_journal_default_window_uses_the_runtime_clock() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route("GET", ORDERS, 200.0, json!({"success":true,"rows":[]}));
    });
    let now = h.runtime.now.get();
    let records = h
        .drive(h.venue().fetch_historical_positions(
            safe_cex_exchange::journal::HistoricalPositionOptions {
                account: "journal-fixture".into(),
                from: None,
                to: None,
            },
        ))
        .expect("default WOO journal window must reach the production order reader");
    assert!(records.is_empty());
    let calls = h.http.calls(ORDERS);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].params["end_t"],
        safe_cex_num::js_number_string(now)
    );
    assert_eq!(
        calls[0].params["start_t"],
        safe_cex_num::js_number_string(now - 30.0 * 24.0 * 60.0 * 60.0 * 1000.0)
    );
    h.venue().dispose().unwrap();
}

fn assert_unsupported_private_topics_preserve_store(topics: &[&str], data: Value) {
    let (h, private) = authed("HEDGE_MODE", |_| {});
    push(
        &h,
        &private,
        json!({"topic":"executionreport","data":{
        "symbol":"PERP_BTC_USDT","orderId":777,"side":"BUY","type":"LIMIT",
        "quantity":2,"price":100,"reduceOnly":false,"status":"NEW",
        "executedQuantity":0,"executedPrice":0}}),
    );
    assert!(
        order(&h, "777").is_some(),
        "known private order makes preservation non-vacuous"
    );
    let orders = h.store.orders().to_vec();
    let positions = h.store.positions().to_vec();
    let balance = h.store.balance().clone();
    assert!(balance.total > 0.0);
    let fills = Rc::new(std::cell::Cell::new(0usize));
    let observed = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, _| {
            observed.set(observed.get() + 1);
            Ok(())
        }),
    );
    for topic in topics {
        push(&h, &private, json!({"topic":topic,"data":data}));
        assert_eq!(
            h.store.orders().to_vec(),
            orders,
            "{topic} cannot change source orders"
        );
        assert_eq!(
            h.store.positions().to_vec(),
            positions,
            "{topic} has no source position handler"
        );
        assert_eq!(
            *h.store.balance(),
            balance,
            "{topic} has no source balance handler"
        );
        assert_eq!(fills.get(), 0, "unsupported {topic} cannot invent a fill");
    }
    assert!(h.runtime.reports.borrow().is_empty());
    h.venue().dispose().unwrap();
}
#[test]
fn private_position_topics_preserve_the_rest_owned_store() {
    assert_unsupported_private_topics_preserve_store(
        &["position", "positions"],
        json!({
        "symbol":"PERP_BTC_USDT","holding":99,"averageOpenPrice":999,
        "positions":[{"symbol":"PERP_BTC_USDT","holding":99,"averageOpenPrice":999}]}),
    );
}
#[test]
fn private_balance_topics_preserve_the_rest_owned_store() {
    assert_unsupported_private_topics_preserve_store(
        &["balance", "account"],
        json!({
        "totalAccountValue":999999,"freeCollateral":888888,"totalCollateral":777777}),
    );
}

fn ticker_row_refusals(topic: &str, source: &str, target: &str, surrogate: bool) {
    use safe_cex_store::{Store, StoreUpdate};
    let h = started(json!({"publicDataOnly":true}), public_routes);
    let template = h.store.tickers()[0].clone();
    let mut tickers = vec![];
    for symbol in ["BTCUSDT", "ETHUSDT", "SOLUSDT", "LTCUSDT"] {
        let mut row = template.clone();
        row.id = symbol.into();
        row.symbol = symbol.into();
        tickers.push(row);
    }
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let public = socket(&h, PUBLIC_URL);
    let frame_row = |asset: &str, value: Value| {
        let mut row = json!({"symbol":format!("PERP_{asset}_USDT"),"close":10,"amount":20,
            "volume":30,"bid":40,"ask":50,"price":60});
        row[source] = value;
        row
    };
    let frame = json!({"topic":topic,"data":[
        frame_row("BTC",Value::Null),frame_row("ETH",json!(123)),
        frame_row("SOL",json!("unparsed")),frame_row("LTC",json!(456))
    ]});
    if surrogate {
        let text = frame.to_string().replace("\"unparsed\"", r#""\ud800""#);
        public
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(text)));
        h.runtime.pump();
    } else {
        push(&h, &public, frame);
    }
    let rows = json_of(&*h.store.tickers());
    let field = |symbol: &str| {
        rows.as_array()
            .unwrap()
            .iter()
            .find(|row| row["symbol"] == symbol)
            .unwrap()[target]
            .clone()
    };
    assert_eq!(field("ETHUSDT"), 123.);
    assert_eq!(field("LTCUSDT"), 456.);
    let original = serde_json::to_value(&template).unwrap()[target].clone();
    assert_eq!(field("BTCUSDT"), original);
    assert_eq!(field("SOLUSDT"), original);
    let reports = h.runtime.reports.borrow().join("\n");
    for (symbol, value) in [
        ("BTCUSDT", "null"),
        ("SOLUSDT", if surrogate { r"\ud800" } else { "unparsed" }),
    ] {
        assert!(
            reports.contains(symbol),
            "missing refused row identity: {reports}"
        );
        assert!(reports.contains(target), "missing refused field: {reports}");
        assert!(
            reports.contains(value),
            "missing original refused value: {reports}"
        );
    }
    assert_eq!(
        reports.matches("Refused WOO ticker row").count(),
        2,
        "{reports}"
    );
    h.venue().dispose().unwrap();
}
#[test]
fn ticker_row_refusals_keep_valid_ticker_siblings_and_report_each_value() {
    ticker_row_refusals("tickers", "close", "last", false);
}
#[test]
fn ticker_row_refusals_keep_valid_bbo_siblings_and_report_each_value() {
    ticker_row_refusals("bbos", "bid", "bid", false);
}
#[test]
fn ticker_row_refusals_keep_valid_mark_siblings_and_report_each_value() {
    ticker_row_refusals("markprices", "price", "mark", false);
}

#[test]
fn retained_ticker_frame_refuses_surrogate_row_only() {
    ticker_row_refusals("tickers", "close", "last", true);
}
#[test]
fn retained_bbo_frame_refuses_surrogate_row_only() {
    ticker_row_refusals("bbos", "bid", "bid", true);
}
#[test]
fn retained_mark_frame_refuses_surrogate_row_only() {
    ticker_row_refusals("markprices", "price", "mark", true);
}

#[test]
fn retained_ticker_frame_ignores_unread_surrogate_metadata() {
    let h = started(json!({"publicDataOnly":true}), public_routes);
    let public = socket(&h, PUBLIC_URL);
    let frame = r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":123,"amount":20,"volume":30,"ignored":"\ud800"}]}"#;
    public
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.into())));
    h.runtime.pump();
    assert_eq!(h.store.tickers()[0].last, 123.);
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    h.venue().dispose().unwrap();
}
#[test]
fn retained_ticker_frame_preserves_overflow_and_signed_zero_numbers() {
    let h = started(json!({"publicDataOnly":true}), public_routes);
    let public = socket(&h, PUBLIC_URL);
    let frame = r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":1e400,"amount":20,"volume":-0.0}]}"#;
    public
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.into())));
    h.runtime.pump();
    assert_eq!(h.store.tickers()[0].last, f64::INFINITY);
    assert_eq!(h.store.tickers()[0].volume.to_bits(), (-0.0f64).to_bits());
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    h.venue().dispose().unwrap();
}

#[test]
fn retained_ticker_frame_normalizes_before_identifier_admission() {
    let h = started(json!({"publicDataOnly":true}), public_routes);
    let public = socket(&h, PUBLIC_URL);
    let frame = r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT_\ud800","close":999,"amount":999,"volume":999},{"symbol":"PERP_\ud800_USDT","close":123,"amount":20,"volume":30}]}"#;
    public
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.into())));
    h.runtime.pump();
    assert_eq!(h.store.tickers()[0].last, 999.);
    assert_eq!(h.runtime.reports.borrow().len(), 1);
    let reports = h.runtime.reports.borrow().join("\n");
    assert!(
        reports.contains(r"\ud800") && reports.contains("identifier boundary"),
        "{reports}"
    );
    h.venue().dispose().unwrap();
}
