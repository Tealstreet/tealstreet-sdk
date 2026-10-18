//! Every `not_implemented()` default of the Exchange trait, driven on the
//! constructed KucoinExchange and KucoinSpotExchange through `dyn Exchange`
//! against a scripted REST transport.
#[path = "support/exchange_runtime.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_types::{Order, PlaceOrderOpts, UpdateOrderOpts};
use serde_json::{Value, json};
use std::rc::Rc;
use support::{Wire, drive};

fn ok(data: Value) -> Value {
    json!({"code":"200000","data":data})
}

fn sent(log: &[(String, String, Value)], method: &str, path: &str) -> bool {
    log.iter().any(|(m, u, _)| m == method && u == path)
}

fn body<'a>(log: &'a [(String, String, Value)], method: &str, path: &str) -> &'a Value {
    &log.iter()
        .find(|(m, u, _)| m == method && u == path)
        .unwrap_or_else(|| panic!("{method} {path} not sent: {log:?}"))
        .2
}

fn futures_wire(wire: &Wire) {
    wire.script(
        "GET",
        "/api/v1/contracts/active",
        ok(json!([{"symbol":"XBTUSDTM","rootSymbol":"USDT","baseCurrency":"XBT","quoteCurrency":"USDT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"multiplier":0.001,"maxOrderQty":1000000,"maxLeverage":125,"isInverse":false}])),
    );
    wire.script(
        "GET",
        "/api/v1/allTickers",
        ok(json!([{"symbol":"XBTUSDTM","price":"100","bestBidPrice":"99.5","bestAskPrice":"100.5","markPrice":"100.2","indexPrice":"100.1","volumeOf24h":"12","turnoverOf24h":"1200"}])),
    );
    let token = ok(
        json!({"token":"t","instanceServers":[{"endpoint":"wss://ws-api-futures.kucoin.com","pingInterval":18000}]}),
    );
    wire.script("POST", "/api/v1/bullet-public", token.clone());
    wire.script("POST", "/api/v1/bullet-private", token);
    wire.script(
        "GET",
        "/api/v1/positions",
        ok(json!([{"symbol":"XBTUSDTM","currentQty":5,"markPrice":"100","avgEntryPrice":"99","leverage":"10","unrealisedPnl":"1.5","liquidationPrice":"50","marginMode":"CROSS","positionSide":"BOTH"}])),
    );
    wire.script(
        "GET",
        "/api/v1/account-overview",
        ok(json!({"availableBalance":"70","orderMargin":"5","accountEquity":"100","unrealisedPNL":"1.5"})),
    );
    wire.script(
        "GET",
        "/api/v1/orders",
        ok(json!({"items":[{"id":"o1","symbol":"XBTUSDTM","type":"limit","side":"buy","price":"95","size":10,"filledSize":0,"status":"open","timeInForce":"GTC","reduceOnly":false,"createdAt":1700000000000u64,"stop":""}]})),
    );
    wire.script("GET", "/api/v1/stopOrders", ok(json!({"items":[]})));
}

#[test]
fn futures_binds_all_sixteen_defaults() {
    use safe_cex_exchange::js_runtime::value::Js;
    use std::cell::RefCell;

    let output = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/position-mode-report-source.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let (venue, f) = support::futures();
    let rt = &f.runtime;
    let wire = &f.wire;
    let venue: Rc<dyn Exchange> = venue;
    futures_wire(wire);
    let emitted = Rc::new(RefCell::new(Vec::new()));
    let received = emitted.clone();
    venue.base().on(
        "error",
        Rc::new(move |_, value| {
            received.borrow_mut().push(value.clone());
            Ok(())
        }),
    );

    // 1 start: public bootstrap, both sockets' tokens, tick and the order snapshot
    drive(rt, venue.start()).unwrap();
    let log = wire.take();
    for (m, p) in [
        ("GET", "/api/v1/contracts/active"),
        ("GET", "/api/v1/allTickers"),
        ("POST", "/api/v1/bullet-public"),
        ("POST", "/api/v1/bullet-private"),
        ("GET", "/api/v1/positions"),
        ("GET", "/api/v1/account-overview"),
        ("GET", "/api/v1/orders"),
        ("GET", "/api/v1/stopOrders"),
    ] {
        assert!(sent(&log, m, p), "start sends {m} {p}: {log:?}");
    }
    let loaded = f.store.loaded().clone();
    assert!(
        loaded.markets && loaded.tickers && loaded.positions && loaded.orders && loaded.balance
    );
    assert_eq!(f.store.balance().total.to_bits(), 100.0f64.to_bits());
    assert_eq!(f.store.orders().len(), 1, "start stores the open order");

    // 2 fetch_markets
    let markets = drive(rt, venue.fetch_markets()).unwrap();
    assert_eq!(
        (markets[0].id.as_str(), markets[0].symbol.as_str()),
        ("XBTUSDTM", "BTCUSDT")
    );
    assert_eq!(markets[0].contract_size, Some(0.001));
    assert_eq!(markets[0].limits.leverage.max.to_bits(), 125.0f64.to_bits());
    assert_eq!(
        wire.take()
            .iter()
            .map(|r| (r.0.as_str(), r.1.as_str()))
            .collect::<Vec<_>>(),
        [("GET", "/api/v1/contracts/active")]
    );

    // 3 fetch_tickers
    let tickers = drive(rt, venue.fetch_tickers()).unwrap();
    assert_eq!(tickers[0].symbol, "BTCUSDT");
    assert_eq!(
        (
            tickers[0].bid,
            tickers[0].ask,
            tickers[0].last,
            tickers[0].mark
        ),
        (99.5, 100.5, 100.0, 100.2)
    );
    assert!(sent(&wire.take(), "GET", "/api/v1/allTickers"));

    // 4 get_account
    wire.script(
        "GET",
        "/api/v1/user-info",
        ok(json!({"userId":"user-1","subUserId":"sub-1","referrerUserId":null})),
    );
    let account = drive(rt, venue.get_account()).unwrap();
    assert_eq!(account.user_id, "user-1");
    assert_eq!(account.sub_id, Some(Some("sub-1".into())));
    assert!(sent(&wire.take(), "GET", "/api/v1/user-info"));

    // 5 validate_account: overview probe then best-effort balance
    let validation = drive(rt, venue.validate_account()).unwrap();
    assert_eq!(validation.error.as_value(), &json!(""));
    assert_eq!(
        validation.balance.map(|b| b.free.to_bits()),
        Some(70.0f64.to_bits())
    );
    let log = wire.take();
    assert_eq!(
        log.iter()
            .filter(|r| r.1 == "/api/v1/account-overview")
            .count(),
        2,
        "{log:?}"
    );

    // 6 fetch_positions: native contracts scaled to base by contractSize
    let positions = drive(rt, venue.fetch_positions(None)).unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "BTCUSDT");
    assert_eq!(positions[0].contracts.to_bits(), 0.005f64.to_bits());
    assert_eq!(positions[0].leverage.to_bits(), 10.0f64.to_bits());
    assert!(sent(&wire.take(), "GET", "/api/v1/positions"));

    // 7 fetch_orders: normal then algo book, base-unit amounts
    let orders = drive(rt, venue.fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!((orders[0].id.as_str(), orders[0].amount), ("o1", 0.01));
    let log = wire.take();
    assert_eq!(
        body(&log, "GET", "/api/v1/orders"),
        &json!({"status":"active"})
    );
    assert!(sent(&log, "GET", "/api/v1/stopOrders"));
    let order: Order = orders[0].clone();

    // 8 place_order through the validated BaseExchange entry
    wire.script("POST", "/api/v1/orders", ok(json!({"orderId":"new-1"})));
    let opts: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":3,"price":96.04,"clientOrderId":"cid_1"}),
    )
    .unwrap();
    let ids = drive(rt, venue.base().place_order(venue.as_ref(), opts)).unwrap();
    assert_eq!(ids, ["new-1"]);
    let log = wire.take();
    let placed = body(&log, "POST", "/api/v1/orders");
    assert_eq!(
        (
            &placed["symbol"],
            &placed["side"],
            &placed["size"],
            &placed["price"],
            &placed["clientOid"]
        ),
        (
            &json!("XBTUSDTM"),
            &json!("buy"),
            &json!("3"),
            &json!("96"),
            &json!("cid_1")
        )
    );

    // 9 update_order: cancel then replace with the native remainder
    wire.script(
        "DELETE",
        "/api/v1/orders/o1",
        ok(json!({"cancelledOrderIds":["o1"]})),
    );
    let update: UpdateOrderOpts =
        serde_json::from_value(json!({"order": order, "update": {"price": 97}})).unwrap();
    let ids = drive(rt, venue.update_order(update)).unwrap();
    assert_eq!(ids, ["new-1"]);
    let log = wire.take();
    let cancel = log
        .iter()
        .position(|r| r.0 == "DELETE" && r.1 == "/api/v1/orders/o1");
    let place = log
        .iter()
        .position(|r| r.0 == "POST" && r.1 == "/api/v1/orders");
    assert!(
        cancel.is_some() && cancel < place,
        "cancel-and-replace order: {log:?}"
    );
    assert_eq!(body(&log, "POST", "/api/v1/orders")["size"], json!("10"));

    // 10 cancel_orders
    drive(rt, venue.cancel_orders(vec![order.clone()])).unwrap();
    let log = wire.take();
    assert_eq!(
        body(&log, "DELETE", "/api/v1/orders/o1"),
        &json!({"symbol":"XBTUSDTM"})
    );

    // 11 set_leverage: cross margin read from the position, then the cross endpoint
    wire.script("POST", "/api/v2/changeCrossUserLeverage", ok(json!(true)));
    drive(rt, venue.set_leverage("BTCUSDT".into(), 5.0, None)).unwrap();
    let log = wire.take();
    assert_eq!(
        body(&log, "POST", "/api/v2/changeCrossUserLeverage"),
        &json!({"symbol":"XBTUSDTM","leverage":"5"})
    );
    assert!(
        sent(&log, "GET", "/api/v1/positions"),
        "leverage refreshes positions"
    );

    // 12 change_position_mode: refused while a position is open, sent when flat
    let refused = drive(rt, venue.change_position_mode(true, None)).unwrap_err();
    assert_eq!(refused.name, source["rejection"]["name"].as_str().unwrap());
    assert_eq!(
        refused.message,
        source["rejection"]["message"].as_str().unwrap()
    );
    assert!(!sent(
        &wire.take(),
        "POST",
        "/api/v2/position/switchPositionMode"
    ));
    wire.script("GET", "/api/v1/positions", ok(json!([])));
    wire.script(
        "POST",
        "/api/v2/position/switchPositionMode",
        ok(json!({"positionMode":"1"})),
    );
    drive(rt, venue.change_position_mode(true, None)).unwrap();
    assert_eq!(
        body(&wire.take(), "POST", "/api/v2/position/switchPositionMode"),
        &json!({"positionMode":"1"})
    );

    // 13 change_margin_type
    wire.script(
        "POST",
        "/api/v2/position/changeMarginMode",
        ok(json!({"marginMode":"ISOLATED"})),
    );
    wire.script(
        "GET",
        "/api/v2/position/getMarginMode",
        ok(json!({"marginMode":"ISOLATED"})),
    );
    wire.script(
        "GET",
        "/api/v2/position/getPositionMode",
        ok(json!({"positionMode":"0"})),
    );
    drive(rt, venue.change_margin_type(true, Some("BTCUSDT".into()))).unwrap();
    let log = wire.take();
    assert_eq!(
        body(&log, "POST", "/api/v2/position/changeMarginMode"),
        &json!({"symbol":"XBTUSDTM","marginMode":"ISOLATED"})
    );
    assert!(
        sent(&log, "GET", "/api/v2/position/getMarginMode"),
        "refresh reads metadata for the flat symbol"
    );

    // Other TS overrides bound alongside the sixteen.
    wire.script(
        "DELETE",
        "/api/v3/orders",
        ok(json!({"cancelledOrderIds":[]})),
    );
    drive(rt, venue.cancel_symbol_orders("BTCUSDT".into())).unwrap();
    assert_eq!(
        body(&wire.take(), "DELETE", "/api/v3/orders"),
        &json!({"symbol":"XBTUSDTM"})
    );
    wire.script(
        "DELETE",
        "/api/v1/stopOrders",
        ok(json!({"cancelledOrderIds":[]})),
    );
    drive(rt, venue.cancel_all_orders()).unwrap();
    let log = wire.take();
    assert!(sent(&log, "DELETE", "/api/v3/orders") && sent(&log, "DELETE", "/api/v1/stopOrders"));
    wire.script(
        "GET",
        "/api/v1/kline/query",
        ok(json!([[1700000000000u64, 1, 3, 0.5, 2, 10]])),
    );
    let candles = drive(
        rt,
        venue.fetch_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1h","limit":2})).unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(
        (candles[0].timestamp, candles[0].close),
        (1_700_000_000.0, 2.0)
    );
    let log = wire.take();
    assert_eq!(
        body(&log, "GET", "/api/v1/kline/query")["granularity"],
        json!("60")
    );

    // 14-16 inherited BaseExchange rejections (base.ts:799, :989, :1138) send nothing
    let max = drive(rt, venue.fetch_symbol_max_leverage("BTCUSDT".into())).unwrap_err();
    assert_eq!(max.message, "Not implemented");
    let schedule = drive(rt, venue.schedule_cancel(Some(Some(60_000.0)))).unwrap_err();
    assert_eq!(schedule.message, "Not implemented");
    let trades = drive(rt, venue.fetch_public_trades("BTCUSDT".into(), None)).unwrap_err();
    assert_eq!(trades.message, "Not implemented");
    assert!(wire.take().is_empty(), "inherited rejections send nothing");

    assert_eq!(
        rt.dials.borrow().len(),
        3,
        "detail, one snapshot shard and private sockets dial"
    );
    assert_eq!(emitted.borrow().len(), 2, "preflight and catch each emit");
    for (actual, expected) in emitted
        .borrow()
        .iter()
        .zip(source["events"].as_array().unwrap())
    {
        assert!(
            actual.strict_equals(&Js::str(expected.as_str().unwrap())),
            "native error event: {actual:?}"
        );
    }
    assert_eq!(
        rt.errors.borrow().len(),
        2,
        "preflight and catch each report"
    );
    assert_eq!(
        *rt.errors.borrow(),
        source["reports"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>(),
        "raw Error report projection matches the constructed TS exchange"
    );
    // Socket listeners route to the public socket and return a working cleanup.
    let candles = venue
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap(),
            Rc::new(|_| {}),
        )
        .unwrap();
    candles().unwrap();
    let book = venue
        .listen_order_book("BTCUSDT".into(), Rc::new(|_| {}))
        .unwrap();
    book().unwrap();
    wire.take();
    venue.dispose().unwrap();
    assert!(venue.base().is_disposed());
}

fn spot_wire(wire: &Wire) {
    wire.script(
        "GET",
        "/api/v2/symbols",
        ok(json!([{"symbol":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.0001","priceIncrement":"0.1","baseMinSize":"0.00001","baseMaxSize":"10000"}])),
    );
    wire.script(
        "GET",
        "/api/v1/market/allTickers",
        ok(json!({"ticker":[{"symbol":"BTC-USDT","buy":"99.5","sell":"100.5","last":"100","changeRate":"0.02","vol":"3","volValue":"300"}]})),
    );
    let token = ok(
        json!({"token":"t","instanceServers":[{"endpoint":"wss://ws-api-spot.kucoin.com","pingInterval":18000}]}),
    );
    wire.script("POST", "/api/v1/bullet-public", token.clone());
    wire.script("POST", "/api/v1/bullet-private", token);
    wire.script(
        "GET",
        "/api/v1/hf/orders/active/symbols",
        ok(json!({"symbols":["BTC-USDT"]})),
    );
    wire.script(
        "GET",
        "/api/v1/hf/orders/active/page",
        ok(json!({"totalPage":1,"items":[{"id":"s1","symbol":"BTC-USDT","type":"limit","side":"buy","price":"95","size":"0.5","dealSize":"0.1","active":true,"timeInForce":"GTC","postOnly":false,"lastUpdatedAt":1700000000000u64}]})),
    );
    wire.script(
        "GET",
        "/api/v1/accounts",
        ok(json!([{"currency":"USDT","available":"70","holds":"30","balance":"100"},{"currency":"BTC","available":"1","holds":"0","balance":"1"}])),
    );
}

#[test]
fn spot_binds_all_sixteen_defaults() {
    let (venue, f) = support::spot();
    let rt = &f.runtime;
    let wire = &f.wire;
    let venue: Rc<dyn Exchange> = venue;
    spot_wire(wire);

    // 1 start: markets, tickers, public socket, synthesized positions, private socket and tick
    drive(rt, venue.start()).unwrap();
    let log = wire.take();
    for (m, p) in [
        ("GET", "/api/v2/symbols"),
        ("GET", "/api/v1/market/allTickers"),
        ("POST", "/api/v1/bullet-public"),
        ("POST", "/api/v1/bullet-private"),
        ("GET", "/api/v1/hf/orders/active/symbols"),
        ("GET", "/api/v1/hf/orders/active/page"),
        ("GET", "/api/v1/accounts"),
    ] {
        assert!(sent(&log, m, p), "start sends {m} {p}: {log:?}");
    }
    let loaded = f.store.loaded().clone();
    assert!(
        loaded.markets && loaded.tickers && loaded.positions && loaded.orders && loaded.balance
    );
    assert_eq!(
        f.store.balance().total.to_bits(),
        200.0f64.to_bits(),
        "USDT 100 + 1 BTC at 100"
    );
    assert_eq!(f.store.orders().len(), 1);

    // 2 fetch_markets
    let markets = drive(rt, venue.fetch_markets()).unwrap();
    assert_eq!(
        (markets[0].id.as_str(), markets[0].symbol.as_str()),
        ("BTC-USDT", "BTC-USDT")
    );
    assert_eq!(markets[0].precision.amount.to_bits(), 0.0001f64.to_bits());
    assert_eq!(markets[0].is_spot, Some(true));
    assert!(sent(&wire.take(), "GET", "/api/v2/symbols"));

    // 3 fetch_tickers
    let tickers = drive(rt, venue.fetch_tickers()).unwrap();
    assert_eq!(
        (tickers[0].bid, tickers[0].ask, tickers[0].last),
        (99.5, 100.5, 100.0)
    );
    assert_eq!(tickers[0].percentage.to_bits(), 2.0f64.to_bits());
    assert!(sent(&wire.take(), "GET", "/api/v1/market/allTickers"));

    // 4 get_account
    wire.script(
        "GET",
        "/api/v2/user-info",
        ok(json!({"uid":"uid-7","subUserId":null,"referrerUserId":"ref-1"})),
    );
    let account = drive(rt, venue.get_account()).unwrap();
    assert_eq!(account.user_id, "uid-7");
    assert_eq!(account.affiliate_id, Some(Some("ref-1".into())));
    assert!(sent(&wire.take(), "GET", "/api/v2/user-info"));

    // 5 validate_account: trade-accounts probe, then the balance read
    let validation = drive(rt, venue.validate_account()).unwrap();
    assert_eq!(validation.error.as_value(), &json!(""));
    assert_eq!(
        validation.balance.map(|b| b.free.to_bits()),
        Some(170.0f64.to_bits())
    );
    let log = wire.take();
    assert_eq!(log.iter().filter(|r| r.1 == "/api/v1/accounts").count(), 2);
    assert_eq!(
        body(&log, "GET", "/api/v1/accounts"),
        &json!({"type":"trade"})
    );

    // 6 fetch_positions: one flat long per active market, no request
    let positions = drive(rt, venue.fetch_positions(None)).unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(
        (positions[0].id.as_str(), positions[0].contracts),
        ("BTC-USDT:long", 0.0)
    );
    assert!(
        wire.take().is_empty(),
        "spot positions are synthesized from markets"
    );

    // 7 fetch_orders: active-symbol discovery then the paged book
    let orders = drive(rt, venue.fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!((orders[0].id.as_str(), orders[0].remaining), ("s1", 0.4));
    let log = wire.take();
    assert!(sent(&log, "GET", "/api/v1/hf/orders/active/symbols"));
    assert_eq!(
        body(&log, "GET", "/api/v1/hf/orders/active/page"),
        &json!({"symbol":"BTC-USDT","pageNum":"1","pageSize":"50"})
    );
    let order: Order = orders[0].clone();

    // 8 place_order through the validated BaseExchange entry
    wire.script(
        "POST",
        "/api/v1/hf/orders",
        ok(json!({"orderId":"spot-new"})),
    );
    let opts: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"BTC-USDT","type":"limit","side":"sell","amount":0.12345,"price":101.04,"clientOrderId":"cid-2"}),
    )
    .unwrap();
    let ids = drive(rt, venue.base().place_order(venue.as_ref(), opts)).unwrap();
    assert_eq!(ids, ["spot-new"]);
    let log = wire.take();
    assert_eq!(
        body(&log, "POST", "/api/v1/hf/orders"),
        &json!({"clientOid":"cid-2","symbol":"BTC-USDT","type":"limit","side":"sell","price":"101","size":"0.1235","timeInForce":"GTC"})
    );

    // 9 update_order: native alter endpoint
    wire.script(
        "POST",
        "/api/v1/hf/orders/alter",
        ok(json!({"newOrderId":"s2"})),
    );
    let update: UpdateOrderOpts =
        serde_json::from_value(json!({"order": order, "update": {"price": 96.06}})).unwrap();
    assert_eq!(drive(rt, venue.update_order(update)).unwrap(), ["s2"]);
    assert_eq!(
        body(&wire.take(), "POST", "/api/v1/hf/orders/alter"),
        &json!({"symbol":"BTC-USDT","orderId":"s1","newPrice":"96.1"})
    );

    // 10 cancel_orders
    wire.script(
        "DELETE",
        "/api/v1/hf/orders/s1",
        ok(json!({"orderId":"s1"})),
    );
    drive(rt, venue.cancel_orders(vec![order])).unwrap();
    assert_eq!(
        body(&wire.take(), "DELETE", "/api/v1/hf/orders/s1"),
        &json!({"symbol":"BTC-USDT"})
    );

    // 11 set_leverage: store-only fetchedLeverage write
    drive(rt, venue.set_leverage("BTC-USDT".into(), 5.0, None)).unwrap();
    let leverage = f.store.fetched_leverage().get("BTC-USDT").cloned().unwrap();
    assert_eq!((leverage.buy_leverage, leverage.sell_leverage), (1.0, 1.0));

    // 12-13 fixed modes resolve without a request
    drive(rt, venue.change_position_mode(true, None)).unwrap();
    drive(rt, venue.change_margin_type(true, Some("BTC-USDT".into()))).unwrap();

    // 14 fetch_symbol_max_leverage: spot is always 1x
    assert_eq!(
        drive(rt, venue.fetch_symbol_max_leverage("BTC-USDT".into())).unwrap(),
        1.0
    );
    assert!(
        wire.take().is_empty(),
        "spot leverage and modes send nothing"
    );

    // Other TS overrides bound alongside the sixteen.
    wire.script("DELETE", "/api/v1/hf/orders", ok(json!({})));
    drive(rt, venue.cancel_symbol_orders("BTC-USDT".into())).unwrap();
    assert_eq!(
        body(&wire.take(), "DELETE", "/api/v1/hf/orders"),
        &json!({"symbol":"BTC-USDT"})
    );
    wire.script("DELETE", "/api/v1/hf/orders/cancelAll", ok(json!({})));
    drive(rt, venue.cancel_all_orders()).unwrap();
    assert!(sent(&wire.take(), "DELETE", "/api/v1/hf/orders/cancelAll"));
    wire.script(
        "GET",
        "/api/v1/hf/fills",
        ok(json!({"items":[{"createdAt":1700000000000u64,"symbol":"BTC-USDT","orderId":"s1","price":"95","size":"0.1","side":"buy","fee":"0.01","feeCurrency":"USDT"}]})),
    );
    let fills = drive(
        rt,
        venue.fetch_historical_fills(
            serde_json::from_value(json!({"symbol":"BTC-USDT","limit":500})).unwrap(),
        ),
    )
    .unwrap();
    assert_eq!((fills[0].price, fills[0].qty), (95.0, 0.1));
    assert_eq!(
        body(&wire.take(), "GET", "/api/v1/hf/fills")["limit"],
        json!("100")
    );
    wire.script(
        "GET",
        "/api/v1/market/candles",
        ok(json!([
            ["1700003600", "2", "3", "4", "1", "10"],
            ["1700000000", "1", "2", "3", "0.5", "5"]
        ])),
    );
    let candles = drive(
        rt,
        venue.fetch_ohlcv(
            serde_json::from_value(json!({"symbol":"BTC-USDT","interval":"1h","limit":2})).unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(
        candles.iter().map(|c| c.timestamp).collect::<Vec<_>>(),
        [1_700_000_000.0, 1_700_003_600.0]
    );
    assert_eq!(
        body(&wire.take(), "GET", "/api/v1/market/candles")["type"],
        json!("1hour")
    );

    // 15-16 inherited BaseExchange rejections (base.ts:989, :1138) send nothing
    let schedule = drive(rt, venue.schedule_cancel(Some(Some(60_000.0)))).unwrap_err();
    assert_eq!(schedule.message, "Not implemented");
    let trades = drive(rt, venue.fetch_public_trades("BTC-USDT".into(), None)).unwrap_err();
    assert_eq!(trades.message, "Not implemented");
    assert!(wire.take().is_empty(), "inherited rejections send nothing");

    assert_eq!(
        rt.dials.borrow().len(),
        2,
        "public and private sockets dial"
    );
    assert!(rt.errors.borrow().is_empty(), "{:?}", rt.errors.borrow());
    // Socket listeners route to the public socket and return a working cleanup.
    let candles = venue
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTC-USDT","interval":"1m"})).unwrap(),
            Rc::new(|_| {}),
        )
        .unwrap();
    candles().unwrap();
    let book = venue
        .listen_order_book("BTC-USDT".into(), Rc::new(|_| {}))
        .unwrap();
    book().unwrap();
    wire.take();
    venue.dispose().unwrap();
    assert!(venue.base().is_disposed());
}

fn key_of(log: &[(String, String, Option<String>)], method: &str, path: &str) -> Option<String> {
    log.iter()
        .find(|(m, u, _)| m == method && u == path)
        .unwrap_or_else(|| panic!("{method} {path} not sent: {log:?}"))
        .2
        .clone()
}

// ts: kucoin.exchange.ts:77 — `this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, ...)`:
// reads go to the secondary key, writes (xhr.post and unlimitedXHR) stay on the primary.
#[test]
fn futures_secondary_key_signs_reads_and_primary_signs_writes() {
    let (venue, f) = support::futures_with(support::secondary_options());
    let rt = &f.runtime;
    let wire = &f.wire;
    let venue: Rc<dyn Exchange> = venue;
    futures_wire(wire);
    drive(rt, venue.start()).unwrap();
    let orders = drive(rt, venue.fetch_orders(None)).unwrap();
    wire.take_keys();

    wire.script(
        "GET",
        "/api/v1/user-info",
        ok(json!({"userId":"user-1","subUserId":null,"referrerUserId":null})),
    );
    drive(rt, venue.get_account()).unwrap();
    assert_eq!(
        key_of(&wire.take_keys(), "GET", "/api/v1/user-info").as_deref(),
        Some("secondary-key"),
        "futures GET signed with the secondary key"
    );

    wire.script("POST", "/api/v2/changeCrossUserLeverage", ok(json!(true)));
    drive(rt, venue.set_leverage("BTCUSDT".into(), 5.0, None)).unwrap();
    let log = wire.take_keys();
    assert_eq!(
        key_of(&log, "GET", "/api/v1/positions").as_deref(),
        Some("secondary-key"),
        "futures xhr.get positions refresh on the secondary key"
    );
    assert_eq!(
        key_of(&log, "POST", "/api/v2/changeCrossUserLeverage").as_deref(),
        Some("fixture-key"),
        "futures xhr.post signed with the primary key"
    );

    wire.script(
        "DELETE",
        "/api/v1/orders/o1",
        ok(json!({"cancelledOrderIds":["o1"]})),
    );
    drive(rt, venue.cancel_orders(orders)).unwrap();
    assert_eq!(
        key_of(&wire.take_keys(), "DELETE", "/api/v1/orders/o1").as_deref(),
        Some("fixture-key"),
        "futures unlimitedXHR cancel signed with the primary key"
    );
}

// ts: kucoinspot.exchange.ts:74 — the same split over createKucoinSpotAPI.
#[test]
fn spot_secondary_key_signs_reads_and_primary_signs_writes() {
    let (venue, f) = support::spot_with(support::secondary_options());
    let rt = &f.runtime;
    let wire = &f.wire;
    let venue: Rc<dyn Exchange> = venue;
    spot_wire(wire);
    drive(rt, venue.start()).unwrap();
    wire.take_keys();

    let orders = drive(rt, venue.fetch_orders(None)).unwrap();
    assert_eq!(
        key_of(&wire.take_keys(), "GET", "/api/v1/hf/orders/active/page").as_deref(),
        Some("secondary-key"),
        "spot GET signed with the secondary key"
    );

    wire.script(
        "DELETE",
        "/api/v1/hf/orders/s1",
        ok(json!({"orderId":"s1"})),
    );
    drive(rt, venue.cancel_orders(orders)).unwrap();
    assert_eq!(
        key_of(&wire.take_keys(), "DELETE", "/api/v1/hf/orders/s1").as_deref(),
        Some("fixture-key"),
        "spot cancel signed with the primary key"
    );
}

#[test]
fn source_identity_flags_survive_spot_facade_construction() {
    let (futures, _) = support::futures();
    assert!(!futures.base().is_spot && !futures.base().is_demo);
    let (spot, _) = support::spot();
    assert!(
        spot.base().is_spot,
        "KuCoin source isSpot flag reaches typed base"
    );
    assert!(!spot.base().is_demo);
    spot.dispose().unwrap();
    futures.dispose().unwrap();
}

fn private_topic_fixture(spot: bool) -> (Rc<dyn Exchange>, support::Fixture, Rc<support::Socket>) {
    let (venue, f): (Rc<dyn Exchange>, _) = if spot {
        let (venue, f) = support::spot();
        spot_wire(&f.wire);
        (venue, f)
    } else {
        let (venue, f) = support::futures();
        futures_wire(&f.wire);
        (venue, f)
    };
    drive(&f.runtime, venue.start()).unwrap();
    f.runtime.pump();
    for _ in 0..40 {
        let loaded = f.store.loaded().clone();
        if loaded.orders && loaded.positions && loaded.balance {
            break;
        }
        f.runtime.now.set(f.runtime.now.get() + 100.0);
        f.runtime.pump();
    }
    let loaded = f.store.loaded().clone();
    assert!(
        loaded.orders && loaded.positions && loaded.balance,
        "startup settled: {loaded:?}"
    );
    let topic = if spot {
        "/spotMarket/tradeOrdersV2"
    } else {
        "/contractMarket/tradeOrders"
    };
    let socket = f
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|socket| {
            socket.sent.borrow().iter().any(|text| {
                serde_json::from_str::<Value>(text).is_ok_and(|v| {
                    v["type"] == "subscribe" && v["topic"] == topic && v["privateChannel"] == true
                })
            })
        })
        .expect("production private subscription")
        .clone();
    assert_eq!(f.store.orders().len(), 1);
    assert!(
        f.runtime.errors.borrow().is_empty(),
        "{:?}",
        f.runtime.errors.borrow()
    );
    (venue, f, socket)
}

fn private_topic_frame(f: &support::Fixture, socket: &support::Socket, value: Value) {
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(value.to_string()),
        ));
    f.runtime.pump();
    assert!(
        f.runtime.errors.borrow().is_empty(),
        "{:?}",
        f.runtime.errors.borrow()
    );
}

fn private_order_topic(spot: bool, event: &str) -> Value {
    if spot {
        json!({"type":"message","topic":"/spotMarket/tradeOrdersV2","data":{
            "orderId":"s1","symbol":"BTC-USDT","side":"buy","orderType":"limit",
            "price":"96","size":"0.5","dealSize":"0.2","remainSize":"0.3",
            "matchSize":"0.1","matchPrice":"97.5","type":event,"status":"open",
            "ts":1700000000000000000u64
        }})
    } else {
        json!({"type":"message","topic":"/contractMarket/tradeOrders","data":{
            "orderId":"o1","symbol":"XBTUSDTM","side":"buy","orderType":"limit",
            "price":"96","size":10,"filledSize":2,"remainSize":8,
            "matchSize":1,"matchPrice":"97.5","type":event,"status":"open",
            "ts":1700000000000000000u64
        }})
    }
}

#[test]
fn private_topic_orders_update_real_store_for_both_families() {
    for spot in [false, true] {
        let (_venue, f, socket) = private_topic_fixture(spot);
        let mut frame = private_order_topic(spot, "update");
        frame["data"].as_object_mut().unwrap().remove("matchSize");
        private_topic_frame(&f, &socket, frame);
        let orders = f.store.orders().to_vec();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].symbol, if spot { "BTC-USDT" } else { "BTCUSDT" });
        assert_eq!(orders[0].price.to_number(), 96.0);
        let expected = if spot {
            (0.5, 0.2, 0.3)
        } else {
            (0.01, 0.002, 0.008)
        };
        assert_eq!(
            (orders[0].amount, orders[0].filled, orders[0].remaining),
            expected
        );
        private_topic_frame(&f, &socket, private_order_topic(spot, "canceled"));
        assert_eq!(
            f.store.orders()[0].status,
            safe_cex_types::OrderStatus::Canceled
        );
    }
}

#[test]
fn private_topic_fills_emit_incremental_amount_and_update_real_orders() {
    for spot in [false, true] {
        let (venue, f, socket) = private_topic_fixture(spot);
        let fills = Rc::new(std::cell::RefCell::new(vec![]));
        let saved = fills.clone();
        venue.base().on(
            "fill",
            Rc::new(move |_, value| {
                saved.borrow_mut().push(value.clone());
                Ok(())
            }),
        );
        private_topic_frame(&f, &socket, private_order_topic(spot, "match"));
        let amount: f64 = if spot { 0.1 } else { 0.001 };
        let id = if spot { "s1" } else { "o1" };
        let symbol = if spot { "BTC-USDT" } else { "BTCUSDT" };
        {
            let fills = fills.borrow();
            assert_eq!(fills.len(), 1);
            let fill = &fills[0];
            assert_eq!(fill.own_entries().unwrap().len(), 5);
            for (key, expected) in [("price", 97.5_f64), ("amount", amount)] {
                assert!(fill.has_own(key));
                assert_eq!(
                    fill.get(key).unwrap().as_f64().unwrap().to_bits(),
                    expected.to_bits()
                );
            }
            assert_eq!(
                safe_cex_exchange::js_runtime::value::to_json(fill).unwrap(),
                json!({"id":id,"side":"buy","symbol":symbol,"price":97.5,"amount":amount})
            );
        }
        assert_eq!(f.store.orders()[0].filled, if spot { 0.2 } else { 0.002 });
        let mut terminal = private_order_topic(spot, "filled");
        terminal["data"]
            .as_object_mut()
            .unwrap()
            .remove("matchSize");
        private_topic_frame(&f, &socket, terminal);
        assert_eq!(
            f.store.orders()[0].status,
            safe_cex_types::OrderStatus::Closed
        );
        assert_eq!(
            fills.borrow().len(),
            1,
            "terminal cumulative frame must not duplicate fill"
        );
    }
}

#[test]
fn private_topic_futures_account_frames_follow_source_orders_only_snapshot() {
    for topic in ["/contract/positionAll", "/contractAccount/wallet"] {
        let (_venue, f, socket) = private_topic_fixture(false);
        let positions = f.store.positions().to_vec();
        let balance = f.store.balance().clone();
        f.wire
            .script("GET", "/api/v1/orders", ok(json!({"items":[]})));
        f.wire.take();
        private_topic_frame(
            &f,
            &socket,
            json!({"type":"message","topic":topic,
            "data":{"symbol":"XBTUSDTM","currentQty":999,"balance":999}}),
        );
        assert_eq!(f.store.orders().len(), 1);
        f.runtime.now.set(f.runtime.now.get() + 499.0);
        f.runtime.pump();
        assert_eq!(f.store.orders().len(), 1, "source500ms debounce");
        f.runtime.now.set(f.runtime.now.get() + 1.0);
        f.runtime.pump();
        assert!(
            sent(&f.wire.take(), "GET", "/api/v1/orders"),
            "snapshot starts at500ms"
        );
        for _ in 0..40 {
            if f.store.orders().is_empty() {
                break;
            }
            f.runtime.now.set(f.runtime.now.get() + 100.0);
            f.runtime.pump();
        }
        assert!(
            f.store.orders().is_empty(),
            "{topic} refreshes real order Store; requests {:?}; errors {:?}; console {:?}",
            f.wire.requests.lock().unwrap(),
            f.runtime.errors.borrow(),
            f.runtime.console.borrow()
        );
        let log = f.wire.take();
        assert!(sent(&log, "GET", "/api/v1/stopOrders"), "{log:?}");
        assert!(!sent(&log, "GET", "/api/v1/positions"));
        assert!(!sent(&log, "GET", "/api/v1/account-overview"));
        assert_eq!(&*f.store.positions(), positions.as_slice());
        assert_eq!(&*f.store.balance(), &balance);
    }
}

#[test]
fn private_topic_spot_balance_refreshes_authoritative_real_store() {
    let (_venue, f, socket) = private_topic_fixture(true);
    f.wire.script(
        "GET",
        "/api/v1/accounts",
        ok(json!([
            {"currency":"USDT","available":"260","holds":"40","balance":"300"}
        ])),
    );
    let before = f.store.balance().total;
    private_topic_frame(
        &f,
        &socket,
        json!({"type":"message","topic":"/account/balance",
        "data":{"balance":999}}),
    );
    assert_eq!(
        f.store.balance().total,
        before,
        "partial wire balance waits for REST"
    );
    f.runtime.now.set(f.runtime.now.get() + 249.0);
    f.runtime.pump();
    assert_eq!(f.store.balance().total, before);
    f.runtime.now.set(f.runtime.now.get() + 1.0);
    f.runtime.pump();
    assert_eq!(
        (f.store.balance().free, f.store.balance().total),
        (260.0, 300.0)
    );
}

#[test]
fn private_topic_spot_positions_preserve_real_synthetic_store_without_fill() {
    let (venue, f, socket) = private_topic_fixture(true);
    let positions = f.store.positions().to_vec();
    let orders = f.store.orders().to_vec();
    let balance = f.store.balance().clone();
    assert!(!positions.is_empty());
    f.wire.take();
    let fills = Rc::new(std::cell::Cell::new(0));
    let saved = fills.clone();
    venue.base().on(
        "fill",
        Rc::new(move |_, _| {
            saved.set(saved.get() + 1);
            Ok(())
        }),
    );
    private_topic_frame(
        &f,
        &socket,
        json!({"type":"message","topic":"/contract/positionAll",
        "data":{"symbol":"BTC-USDT","currentQty":999}}),
    );
    f.runtime.now.set(f.runtime.now.get() + 500.0);
    f.runtime.pump();
    assert_eq!(&*f.store.positions(), positions.as_slice());
    assert_eq!(&*f.store.orders(), orders.as_slice());
    assert_eq!(&*f.store.balance(), &balance);
    assert_eq!(fills.get(), 0);
    assert!(
        f.wire.take().is_empty(),
        "unsupported position topic must not trigger HTTP"
    );
}

#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

fn native_ticker_coercion(spot: bool) {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_transport::socket::SocketFrame;
    let name = if spot { "kucoinspot" } else { "kucoin" };
    let source = ticker_coercion::source(name);
    assert_eq!(
        source["cases"].as_array().unwrap().len(),
        if spot { 256 } else { 72 }
    );
    let (venue, f, _) = private_topic_fixture(spot);
    let socket = f
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|socket| {
            socket.sent.borrow().iter().any(|text| {
                serde_json::from_str::<Value>(text).is_ok_and(|value| {
                    value["type"] == "subscribe" && value["privateChannel"] == false
                })
            })
        })
        .expect("constructed public subscription")
        .clone();
    for case in source["cases"].as_array().unwrap() {
        Store::update(
            f.store.as_ref(),
            StoreUpdate {
                tickers: Some(ticker_coercion::seed(name)),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        for frame in case["frames"].as_array().unwrap() {
            socket
                .events
                .borrow_mut()
                .push_back(WsEvent::Message(SocketFrame::Text(
                    frame.as_str().unwrap().into(),
                )));
            f.runtime.pump();
        }
        ticker_coercion::assert_native(&f.store.tickers(), case);
    }
    assert!(
        f.runtime.errors.borrow().is_empty(),
        "{:?}",
        f.runtime.errors.borrow()
    );
    venue.dispose().unwrap();
}

#[test]
fn production_ticker_coercion_futures_matches_constructed_typescript() {
    native_ticker_coercion(false);
}

#[test]
fn production_ticker_coercion_spot_matches_constructed_typescript() {
    native_ticker_coercion(true);
}
