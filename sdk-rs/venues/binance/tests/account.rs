//! Private data and trading against Binance's documented payloads.
mod support;

use safe_cex_engine::model::{
    MarginType, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, PositionSide, RejectReason,
};
use safe_cex_engine::{Command, Event, Method, Query};
use safe_cex_venue_binance::Options;
use support::Harness;

const POSITION_RISK: &str = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0.010","entryPrice":"95000.0","breakEvenPrice":"95010.0","markPrice":"96000.0","unRealizedProfit":"10.00","liquidationPrice":"80000.5","isolatedMargin":"0","notional":"960.0","marginAsset":"USDT","isolatedWallet":"0","initialMargin":"48","maintMargin":"3.8","positionInitialMargin":"48","openOrderInitialMargin":"0","adl":1,"bidNotional":"0","askNotional":"0","updateTime":1720736417660}]"#;
const SYMBOL_CONFIG: &str = r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","isAutoAddMargin":false,"leverage":20,"maxNotionalValue":"$1000000"}]"#;
const ACCOUNT: &str = r#"{"totalInitialMargin":"48","totalWalletBalance":"1000","availableBalance":"952","assets":[{"asset":"USDT","walletBalance":"1000.0","unrealizedProfit":"10.0","marginBalance":"1010.0","maintMargin":"3.8","initialMargin":"48.0","availableBalance":"952.0","updateTime":1625474304765},{"asset":"BTC","walletBalance":"0.0","unrealizedProfit":"0.0","initialMargin":"0","availableBalance":"0"}],"positions":[]}"#;
const OPEN_ORDERS: &str = r#"[{"avgPrice":"0.00000","clientOrderId":"abc","cumQuote":"0","executedQty":"0.001","orderId":8389765519993909000,"origQty":"0.004","origType":"LIMIT","price":"90000","reduceOnly":false,"side":"BUY","positionSide":"BOTH","status":"PARTIALLY_FILLED","stopPrice":"0","closePosition":false,"symbol":"BTCUSDT","time":1579276756075,"timeInForce":"GTC","type":"LIMIT","updateTime":1579276756075,"workingType":"CONTRACT_PRICE","priceProtect":false}]"#;
const OPEN_ALGO_ORDERS: &str = r#"[{"algoId":2146760,"clientAlgoId":"x","algoType":"CONDITIONAL","orderType":"STOP_MARKET","symbol":"BTCUSDT","side":"SELL","positionSide":"BOTH","timeInForce":"GTC","quantity":"0","algoStatus":"NEW","triggerPrice":"85000","price":"0","workingType":"MARK_PRICE","reduceOnly":false,"closePosition":true,"createTime":1750485492076}]"#;

fn credentials() -> Options {
    Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }
}

/// A private engine with the account loaded and the user stream open.
fn ready() -> Harness {
    let mut h = Harness::started(credentials());
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond("/fapi/v1/leverageBracket", 200, r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125,"notionalCap":50000}]}]"#);
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond(
        "/fapi/v1/listenKey",
        200,
        r#"{"listenKey":"pqia91ma19a5s61cv6a81va65sdf19v8a65a1a5s61cv6a81va65sdf19v8a65a1"}"#,
    );
    h.respond(
        "/fapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/fapi/v3/positionRisk", 200, POSITION_RISK);
    h.respond("/fapi/v1/symbolConfig", 200, SYMBOL_CONFIG);
    h.respond("/fapi/v3/account", 200, ACCOUNT);
    h.respond("/fapi/v1/openOrders", 200, OPEN_ORDERS);
    h.respond("/fapi/v1/openAlgoOrders", 200, OPEN_ALGO_ORDERS);
    for path in ["/public/ws", "/market/ws", "/private/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    h
}

fn place(h: &mut Harness, command: u32, orders: Vec<PlaceOrderOpts>) {
    h.event(Event::Command(Command::PlaceOrders { command, orders }));
}

fn limit(side: OrderSide, amount: f64, price: f64) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side,
        amount,
        price: Some(price),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        close: None,
    }
}

fn query_of(url: &str) -> Vec<(String, String)> {
    url.split_once('?')
        .map(|(_, q)| q)
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn param(url: &str, name: &str) -> Option<String> {
    query_of(url)
        .into_iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
}

#[test]
fn private_startup_signs_reads_and_loads_the_account() {
    let mut h = Harness::started(credentials());
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    let brackets = h.take("/fapi/v1/leverageBracket");
    assert_eq!(
        brackets.headers,
        vec![("X-MBX-APIKEY", "api-key".to_owned())]
    );
    let query = query_of(&brackets.url);
    let names: Vec<&str> = query.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(names, ["timestamp", "recvWindow", "signature"]);
    assert_eq!(param(&brackets.url, "timestamp").as_deref(), Some("1000"));
    h.reply(
        brackets.id,
        200,
        "",
        r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125}]}]"#,
    );
    assert_eq!(
        h.engine
            .store()
            .market("BTCUSDT")
            .unwrap()
            .limits
            .leverage
            .max,
        125.0
    );

    let h = &mut ready_from(h);
    let store = h.engine.store();
    let private = h.socket("/private/ws").unwrap();
    assert!(
        h.url_of(private).ends_with(
            "?listenKey=pqia91ma19a5s61cv6a81va65sdf19v8a65a1a5s61cv6a81va65sdf19v8a65a1"
        )
    );
    let loaded = store.loaded();
    assert!(
        loaded.balance && loaded.orders && loaded.positions && loaded.markets && loaded.tickers
    );
    assert!(!store.hedged());

    let position = &store.positions()[0];
    assert_eq!(position.id, "BTCUSDT");
    assert_eq!(position.side, PositionSide::Long);
    assert_eq!(position.contracts, 0.01);
    assert_eq!(position.leverage, 20.0);
    assert_eq!(position.margin_type, Some(MarginType::Cross));
    assert_eq!(position.liquidation_price, 80000.5);
    assert_eq!(position.margin_size, Some(48.0));
    assert_eq!(store.setting("BTCUSDT").unwrap().leverage, 20.0);

    let balance = store.balance();
    assert_eq!(
        (balance.total, balance.free, balance.used, balance.upnl),
        (1000.0, 952.0, 48.0, 10.0)
    );

    let orders = store.orders();
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[0].id, "8389765519993909000");
    assert_eq!(
        (orders[0].amount, orders[0].filled, orders[0].remaining),
        (0.004, 0.001, 0.003)
    );
    assert_eq!(orders[1].id, "2146760");
    assert_eq!(orders[1].kind, OrderType::StopLoss);
    assert_eq!(orders[1].close, Some(true));
}

/// Continue [`ready`]'s startup from a harness that already answered brackets.
fn ready_from(mut h: Harness) -> Harness {
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond(
        "/fapi/v1/listenKey",
        200,
        r#"{"listenKey":"pqia91ma19a5s61cv6a81va65sdf19v8a65a1a5s61cv6a81va65sdf19v8a65a1"}"#,
    );
    h.respond(
        "/fapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/fapi/v3/positionRisk", 200, POSITION_RISK);
    h.respond("/fapi/v1/symbolConfig", 200, SYMBOL_CONFIG);
    h.respond("/fapi/v3/account", 200, ACCOUNT);
    h.respond("/fapi/v1/openOrders", 200, OPEN_ORDERS);
    h.respond("/fapi/v1/openAlgoOrders", 200, OPEN_ALGO_ORDERS);
    for path in ["/public/ws", "/market/ws", "/private/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    h
}

#[test]
fn order_events_drive_the_order_lifecycle_and_report_each_execution() {
    let mut h = ready();
    let new = r#"{"e":"ORDER_TRADE_UPDATE","E":1568879465651,"T":1568879465650,"o":{"s":"BTCUSDT","c":"TEST","S":"SELL","o":"LIMIT","f":"GTC","q":"0.002","p":"97000","ap":"0","sp":"0","x":"NEW","X":"NEW","i":8886774,"l":"0","z":"0","L":"0","T":1568879465650,"t":0,"b":"0","a":"0","m":false,"R":false,"wt":"CONTRACT_PRICE","ot":"LIMIT","ps":"BOTH","cp":false,"rp":"0"}}"#;
    h.message("/private/ws", new);
    let order = h.engine.store().order("8886774").unwrap().clone();
    assert_eq!(
        (order.status, order.side, order.price, order.amount),
        (OrderStatus::Open, OrderSide::Sell, 97000.0, 0.002)
    );
    assert_eq!(order.client_order_id.as_deref(), Some("TEST"));

    let partial = new
        .replace(r#""X":"NEW""#, r#""X":"PARTIALLY_FILLED""#)
        .replace(
            r#""l":"0","z":"0","L":"0""#,
            r#""l":"0.001","z":"0.001","L":"97000.5""#,
        )
        .replace(r#""ap":"0""#, r#""ap":"96999""#);
    h.message("/private/ws", &partial);
    let order = h.engine.store().order("8886774").unwrap();
    assert_eq!(
        (order.filled, order.remaining, order.status),
        (0.001, 0.001, OrderStatus::Open)
    );
    let frame = h.engine.take_frame(h.now).unwrap().to_vec();
    assert!(contains_section(&frame, 11), "fills go out in the frame");

    let filled = partial
        .replace(r#""X":"PARTIALLY_FILLED""#, r#""X":"FILLED""#)
        .replace(r#""z":"0.001""#, r#""z":"0.002""#);
    h.message("/private/ws", &filled);
    assert_eq!(
        h.engine.store().order("8886774").unwrap().status,
        OrderStatus::Closed
    );

    let rejected = new
        .replace("8886774", "8886775")
        .replace(r#""f":"GTC""#, r#""f":"GTX""#);
    h.message("/private/ws", &rejected);
    h.message(
        "/private/ws",
        &rejected
            .replace(r#""X":"NEW""#, r#""X":"EXPIRED""#)
            .replace(r#""rp":"0""#, r#""rp":"0","r":"GTX_ORDER_REJECT""#),
    );
    let order = h.engine.store().order("8886775").unwrap();
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(
        order.extra.as_ref().and_then(|e| e.reject_reason),
        Some(RejectReason::PostOnly)
    );

    // Events coalesce into one REST order snapshot ten seconds after the last.
    assert!(!h.has_request("/fapi/v1/openOrders"));
    h.advance(10_000.0);
    assert!(h.has_request("/fapi/v1/openOrders"));
    assert!(h.has_request("/fapi/v1/openAlgoOrders"));
}

fn contains_section(frame: &[u8], kind: u16) -> bool {
    let mut at = 16;
    for _ in 0..u16::from_le_bytes([frame[6], frame[7]]) {
        if u16::from_le_bytes([frame[at], frame[at + 1]]) == kind {
            return true;
        }
        at += 8 + u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
    }
    false
}

#[test]
fn a_stale_position_snapshot_cannot_overwrite_a_newer_stream_event() {
    let mut h = ready();
    let update = r#"{"e":"ACCOUNT_UPDATE","E":1564745798939,"T":1564745798938,"a":{"m":"ORDER","B":[],"P":[{"s":"BTCUSDT","pa":"0.015","ep":"95500.0","cr":"0","up":"20.0","mt":"cross","iw":"0","ps":"BOTH"}]}}"#;
    // The first event repairs at once; the second lands while that read is out.
    h.message("/private/ws", update);
    let risk = h.take("/fapi/v3/positionRisk");
    let config = h.take("/fapi/v1/symbolConfig");
    h.message(
        "/private/ws",
        &update.replace(r#""pa":"0.015""#, r#""pa":"0.020""#),
    );
    assert_eq!(h.engine.store().positions()[0].contracts, 0.02);
    h.reply(risk.id, 200, "", POSITION_RISK);
    h.reply(config.id, 200, "", SYMBOL_CONFIG);
    let position = &h.engine.store().positions()[0];
    assert_eq!(
        position.contracts, 0.02,
        "the older REST snapshot was discarded"
    );
    assert_eq!(
        position.leverage, 20.0,
        "leverage kept from the last snapshot"
    );

    // A fresh repair follows within the throttle window.
    assert!(!h.has_request("/fapi/v3/positionRisk"));
    h.advance(10_000.0);
    assert!(h.has_request("/fapi/v3/positionRisk"));

    let close = update.replace(r#""pa":"0.015""#, r#""pa":"0""#);
    h.message("/private/ws", &close);
    assert!(h.engine.store().positions().is_empty());
}

#[test]
fn places_regular_orders_then_bracket_stops_in_sequence() {
    let mut h = ready();
    let mut entry = limit(OrderSide::Buy, 0.0123, 90000.04);
    entry.stop_loss = Some(85000.0);
    entry.take_profit = Some(99000.0);
    place(&mut h, 7, vec![entry]);

    let order = h.take("/fapi/v1/order");
    assert_eq!(order.method, Method::Post);
    let q = query_of(&order.url);
    let get = |name: &str| q.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    assert_eq!(get("symbol"), Some("BTCUSDT"));
    assert_eq!(get("positionSide"), Some("BOTH"));
    assert_eq!(get("side"), Some("BUY"));
    assert_eq!(get("type"), Some("LIMIT"));
    assert_eq!(get("quantity"), Some("0.012"));
    assert_eq!(get("price"), Some("90000"));
    assert_eq!(get("timeInForce"), Some("GTC"));
    assert!(get("newClientOrderId").unwrap().starts_with("x-s3vFFRQp"));
    assert!(
        !h.has_request("/fapi/v1/algoOrder"),
        "stops wait for the entry"
    );

    h.reply(
        order.id,
        200,
        "",
        r#"{"orderId":22542179,"symbol":"BTCUSDT","status":"NEW"}"#,
    );
    let stop = h.take("/fapi/v1/algoOrder");
    assert_eq!(param(&stop.url, "type").as_deref(), Some("STOP_MARKET"));
    assert_eq!(param(&stop.url, "side").as_deref(), Some("SELL"));
    assert_eq!(param(&stop.url, "triggerPrice").as_deref(), Some("85000"));
    assert_eq!(param(&stop.url, "reduceOnly").as_deref(), Some("true"));
    assert_eq!(
        param(&stop.url, "workingType").as_deref(),
        Some("MARK_PRICE")
    );
    h.reply(stop.id, 200, "", r#"{"algoId":2146760,"algoStatus":"NEW"}"#);
    let take_profit = h.take("/fapi/v1/algoOrder");
    assert_eq!(
        param(&take_profit.url, "type").as_deref(),
        Some("TAKE_PROFIT_MARKET")
    );
    assert_eq!(
        param(&take_profit.url, "workingType").as_deref(),
        Some("CONTRACT_PRICE")
    );
    h.reply(take_profit.id, 200, "", r#"{"algoId":2146761}"#);

    assert_eq!(
        h.result(7),
        Some(&Ok(r#"["22542179","2146760","2146761"]"#.to_owned()))
    );
}

#[test]
fn a_rejected_entry_stops_the_bracket_and_reports_the_venue_message() {
    let mut h = ready();
    let mut entry = limit(OrderSide::Buy, 0.01, 90000.0);
    entry.stop_loss = Some(85000.0);
    place(&mut h, 3, vec![entry]);
    let order = h.take("/fapi/v1/order");
    h.reply(
        order.id,
        400,
        "",
        r#"{"code":-2019,"msg":"Margin is insufficient."}"#,
    );
    assert!(!h.has_request("/fapi/v1/algoOrder"));
    assert_eq!(
        h.result(3),
        Some(&Err("Margin is insufficient.".to_owned()))
    );
    assert!(h.logs.iter().any(|m| m == "Margin is insufficient."));
}

#[test]
fn batches_five_orders_per_request() {
    let mut h = ready();
    let orders = (0..6)
        .map(|i| limit(OrderSide::Buy, 0.01, 90000.0 - f64::from(i)))
        .collect();
    place(&mut h, 9, orders);
    let batch = h.take("/fapi/v1/batchOrders");
    let payload = param(&batch.url, "batchOrders").unwrap();
    assert!(
        payload.starts_with("%5B%7B%22symbol%22%3A%22BTCUSDT%22"),
        "{payload}"
    );
    h.reply(batch.id, 200, "", r#"[{"orderId":1},{"orderId":2},{"code":-2019,"msg":"Margin is insufficient."},{"orderId":4},{"orderId":5}]"#);
    let single = h.take("/fapi/v1/order");
    h.reply(single.id, 200, "", r#"{"orderId":6}"#);
    assert_eq!(
        h.result(9),
        Some(&Ok(r#"["1","2","4","5","6"]"#.to_owned()))
    );
}

#[test]
fn an_unhonorable_client_order_id_refuses_the_whole_batch() {
    let mut h = ready();
    let mut first = limit(OrderSide::Buy, 0.01, 90000.0);
    first.client_order_id = Some("ok-id".into());
    let mut second = limit(OrderSide::Buy, 0.01, 89000.0);
    second.client_order_id = Some("not valid!".into());
    place(&mut h, 4, vec![first, second]);
    assert!(h.requests.is_empty());
    assert!(matches!(h.result(4), Some(Err(message)) if message.contains("not valid!")));
}

#[test]
fn full_position_stops_replace_the_existing_close_order() {
    let mut h = ready();
    let mut stop = limit(OrderSide::Sell, 0.0, 86000.0);
    stop.kind = OrderType::StopLoss;
    stop.close = Some(true);
    place(&mut h, 5, vec![stop]);
    let cancel = h.take("/fapi/v1/algoOrder");
    assert_eq!(cancel.method, Method::Delete);
    assert_eq!(param(&cancel.url, "algoId").as_deref(), Some("2146760"));
    h.reply(
        cancel.id,
        400,
        "",
        r#"{"code":-2011,"msg":"Unknown order sent."}"#,
    );
    assert_eq!(
        h.engine.store().order("2146760").unwrap().status,
        OrderStatus::Canceled
    );
    let placed = h.take("/fapi/v1/algoOrder");
    assert_eq!(param(&placed.url, "closePosition").as_deref(), Some("true"));
    assert_eq!(param(&placed.url, "quantity"), None);
    assert!(
        !h.logs.iter().any(|m| m.contains("Unknown order")),
        "-2011 is silent"
    );
}

#[test]
fn cancel_all_reports_a_partial_failure_and_only_marks_what_succeeded() {
    let mut h = ready();
    h.event(Event::Command(Command::CancelSymbolOrders {
        command: 11,
        symbol: "BTCUSDT".into(),
    }));
    let regular = h.take("/fapi/v1/allOpenOrders");
    let algo = h.take("/fapi/v1/algoOpenOrders");
    h.reply(
        regular.id,
        200,
        "",
        r#"{"code":200,"msg":"The operation of cancel all open order is done."}"#,
    );
    h.reply(algo.id, 503, "", r#"{"code":-1001,"msg":"Internal error; unable to process your request. Please try again."}"#);
    let store = h.engine.store();
    assert_eq!(
        store.order("8389765519993909000").unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(store.order("2146760").unwrap().status, OrderStatus::Open);
    assert!(matches!(h.result(11), Some(Err(message)) if message.starts_with("Internal error")));
}

#[test]
fn amend_cancels_then_places_the_replacement() {
    let mut h = ready();
    h.event(Event::Command(Command::AmendOrder {
        command: 12,
        id: "8389765519993909000".into(),
        price: Some(91000.0),
        amount: None,
    }));
    let cancel = h.take("/fapi/v1/order");
    assert_eq!(cancel.method, Method::Delete);
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"orderId":8389765519993909000,"status":"CANCELED"}"#,
    );
    let replacement = h.take("/fapi/v1/order");
    assert_eq!(replacement.method, Method::Post);
    assert_eq!(param(&replacement.url, "price").as_deref(), Some("91000"));
    assert_eq!(
        param(&replacement.url, "quantity").as_deref(),
        Some("0.004")
    );
}

#[test]
fn leverage_and_position_mode_update_the_settings_frame() {
    let mut h = ready();
    h.event(Event::Command(Command::SetLeverage {
        command: 13,
        symbol: "BTCUSDT".into(),
        leverage: 500.0,
    }));
    let request = h.take("/fapi/v1/leverage");
    assert_eq!(
        param(&request.url, "leverage").as_deref(),
        Some("125"),
        "clamped to the bracket"
    );
    h.reply(
        request.id,
        200,
        "",
        r#"{"leverage":125,"maxNotionalValue":"50000","symbol":"BTCUSDT"}"#,
    );
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 125.0);
    assert_eq!(h.engine.store().positions()[0].leverage, 125.0);

    h.event(Event::Command(Command::SetPositionMode {
        command: 14,
        hedged: true,
    }));
    assert!(h.requests.is_empty(), "refused while a position is open");
    assert!(matches!(h.result(14), Some(Err(_))));
}

#[test]
fn near_the_weight_limit_background_reads_pause() {
    let mut h = ready();
    // The 3 s health tick refreshes the account once 25 s have passed.
    h.advance(27_000.0);
    let account = h.take("/fapi/v3/account");
    h.reply(account.id, 200, "x-mbx-used-weight-1m: 2000", ACCOUNT);
    h.advance(50_000.0);
    assert!(!h.has_request("/fapi/v3/account"), "paused for a minute");
    h.advance(15_000.0);
    assert!(h.has_request("/fapi/v3/account"));
}

#[test]
fn a_silent_socket_is_closed_and_reconnected() {
    let mut h = ready();
    let public = h.socket("/public/ws").unwrap();
    h.advance(20_000.0);
    assert!(
        !h.closed.contains(&public),
        "twenty seconds of silence is tolerated"
    );
    h.advance(5_000.0);
    assert!(h.closed.contains(&public));
    h.advance(1_000.0);
    assert_ne!(h.socket("/public/ws"), Some(public), "a new public socket");
    let private = h.socket("/private/ws").unwrap();
    assert!(
        h.sent
            .iter()
            .any(|(s, text)| *s == private && text.contains("LIST_SUBSCRIPTIONS"))
    );
}

#[test]
fn an_expired_listen_key_reconnects_with_a_new_one() {
    let mut h = ready();
    let private = h.socket("/private/ws").unwrap();
    h.message(
        "/private/ws",
        r#"{"e":"listenKeyExpired","E":1576653824250,"listenKey":"old"}"#,
    );
    assert!(h.closed.contains(&private));
    h.advance(1_000.0);
    h.respond("/fapi/v1/listenKey", 200, r#"{"listenKey":"fresh"}"#);
    let socket = h.socket("/private/ws").unwrap();
    assert!(h.url_of(socket).ends_with("?listenKey=fresh"));
    h.event(Event::WsOpened { socket });
    h.advance(10_000.0);
    assert!(
        h.has_request("/fapi/v1/openOrders"),
        "reconnect resyncs orders"
    );
}

#[test]
fn income_over_a_range_walks_every_page() {
    let mut h = ready();
    h.event(Event::Command(Command::Query {
        command: 21,
        query: Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(0.0),
            to_ms: Some(1_000.0),
            limit: None,
        },
    }));
    let rows = |range: std::ops::RangeInclusive<u32>| {
        let rows: Vec<String> = range
            .map(|i| format!(r#"{{"symbol":"BTCUSDT","incomeType":"COMMISSION","income":"-0.01","asset":"USDT","info":"","time":{i},"tranId":{i},"tradeId":"{i}"}}"#))
            .collect();
        format!("[{}]", rows.join(","))
    };
    let first = h.take("/fapi/v1/income");
    assert_eq!(param(&first.url, "startTime").as_deref(), Some("0"));
    h.reply(first.id, 200, "", &rows(1..=1000));
    let second = h.take("/fapi/v1/income");
    assert_eq!(param(&second.url, "startTime").as_deref(), Some("1000"));
    h.reply(second.id, 200, "", &rows(1000..=1000));
    let result = h.result(21).unwrap().as_ref().unwrap();
    assert_eq!(result.matches("tranId").count(), 1000);
    assert!(result.starts_with(r#"[{"symbol":"BTCUSDT","incomeType":"COMMISSION","income":-0.01,"asset":"USDT","timestamp":1000"#));
}

#[test]
fn an_error_envelope_in_place_of_a_list_keeps_the_last_snapshot() {
    let mut h = ready();
    h.advance(75_000.0);
    let risk = h.take("/fapi/v3/positionRisk");
    h.reply(risk.id, 200, "", r#"{"code":-1001,"msg":"Internal error"}"#);
    h.respond("/fapi/v1/symbolConfig", 200, SYMBOL_CONFIG);
    assert_eq!(
        h.engine.store().positions().len(),
        1,
        "positions were not wiped"
    );
    assert!(
        h.logs.iter().any(
            |m| m.contains("did not contain the expected list") && m.contains("Internal error")
        )
    );
}

#[test]
fn streamed_lists_arrive_in_chunks() {
    let mut h = Harness::started(credentials());
    let request = h.take("/fapi/v1/exchangeInfo").id;
    h.event(Event::HttpStart {
        request,
        status: 200,
        headers: "",
    });
    for chunk in support::EXCHANGE_INFO.as_bytes().chunks(5) {
        h.event(Event::HttpChunk {
            request,
            data: chunk,
        });
    }
    h.event(Event::HttpEnd { request });
    assert_eq!(h.engine.store().markets().len(), 1);
    assert_eq!(h.engine.store().markets()[0].limits.amount.max, 120.0);
}
