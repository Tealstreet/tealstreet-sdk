//! Private data and trading against Binance's documented payloads.
use support::Options;
mod support;

use safe_cex_engine::model::{
    MarginType, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, PositionSide, RejectReason,
};
use safe_cex_engine::{Command, Event, Method, Query};

use support::Harness;

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 3_000.0;

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
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
        close: None,
        stp_mode: None,
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
        .replace(r#""x":"NEW""#, r#""x":"TRADE""#)
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
        .replace(r#""z":"0.001""#, r#""z":"0.002""#)
        .replace(r#""t":0"#, r#""t":1"#);
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
    // Both lots still run, but the failed item cannot become whole-command
    // success. The engine retains every accepted native ID in the partial.
    assert_eq!(h.result(9), Some(&Err(r#"{"message":"Margin is insufficient.","partialOrderIds":["1","2","4","5","6"],"partialOrderCount":5}"#.to_owned())));
    let failure = &h.failures.iter().find(|(id, _)| *id == 9).unwrap().1;
    assert_eq!(failure.outcome, None);
    assert_eq!(
        failure.partial.as_ref().unwrap().order_ids,
        ["1", "2", "4", "5", "6"]
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
fn a_close_order_replacement_completes_the_placement_successfully() {
    // The conflicting-order cancel is a cancel leg: its ID-less answer must
    // not turn the whole placement into an unknown failure.
    let mut h = ready();
    let mut stop = limit(OrderSide::Sell, 0.0, 86000.0);
    stop.kind = OrderType::StopLoss;
    stop.close = Some(true);
    place(&mut h, 5, vec![stop]);
    let cancel = h.take("/fapi/v1/algoOrder");
    h.reply(
        cancel.id,
        400,
        "",
        r#"{"code":-2011,"msg":"Unknown order sent."}"#,
    );
    let placed = h.take("/fapi/v1/algoOrder");
    h.reply(placed.id, 200, "", r#"{"algoId":2146761}"#);
    assert!(
        matches!(h.result(5), Some(Ok(_))),
        "replacement placement result: {:?}",
        h.result(5)
    );
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
    assert_eq!(
        h.result(11),
        Some(&Err("HTTP response does not prove the write outcome".into()))
    );
    let failure = &h.failures.iter().find(|(id, _)| *id == 11).unwrap().1;
    assert_eq!(
        failure.kind,
        Some(safe_cex_engine::ErrorKind::VenueUnavailable)
    );
    assert_eq!(failure.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(failure.venue_code.as_deref(), Some("-1001"));
    assert_eq!(
        failure.venue_message.as_deref(),
        Some("Internal error; unable to process your request. Please try again.")
    );
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
        r#"{"symbol":"BTCUSDT","orderId":8389765519993909000,"status":"CANCELED","origQty":"0.004","executedQty":"0.001"}"#,
    );
    let replacement = h.take("/fapi/v1/order");
    assert_eq!(replacement.method, Method::Post);
    assert_eq!(param(&replacement.url, "price").as_deref(), Some("91000"));
    assert_eq!(
        param(&replacement.url, "quantity").as_deref(),
        Some("0.003")
    );
}

#[test]
fn leverage_and_position_mode_update_the_settings_frame() {
    let mut h = ready();
    h.event(Event::Command(Command::SetLeverage {
        command: 13,
        symbol: "BTCUSDT".into(),
        leverage: 500.0,
        side: None,
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
        symbol: None,
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
    h.advance(19_999.0);
    assert!(
        !h.closed.contains(&public),
        "under twenty seconds of silence is tolerated"
    );
    h.advance(1.0);
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

#[test]
fn cancellation_missing_from_store_errors_before_any_write_even_in_a_mixed_batch() {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    use safe_cex_engine::{Action, Notice};
    for mixed in [false, true] {
        let mut h = ready();
        h.engine.store_mut().set_orders(vec![Order {
            id: "123".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 90.0,
            amount: 1.0,
            filled: 0.0,
            remaining: 1.0,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }]);
        h.engine.drain_actions().for_each(drop);
        h.engine.handle(
            2000.0,
            Event::Command(Command::CancelOrders {
                command: 991,
                ids: if mixed {
                    vec!["123".into(), "456".into()]
                } else {
                    vec!["456".into()]
                },
            }),
        );
        let actions = h.engine.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 991, error })
                    if error.message.contains("456") && error.message.contains("not found")
                        && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
            )),
            "missing cancellation error: {actions:?}"
        );
        assert_eq!(
            h.engine.store().order("123").unwrap().status,
            OrderStatus::Open
        );
    }
}

// Drive the actual account loop through slow/fast updates, a parked stale callback,
// and a resumed read. Other timers are deliberately not fired.
fn policy_poll_read<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
    endpoint: &str,
) -> (f64, Vec<safe_cex_engine::HttpRequest>) {
    use safe_cex_engine::{Action, Event};
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 60_000.0 / POLL_MS,
        },
    );
    let slow: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            Action::Http(_) => panic!("policy updates must not issue account reads"),
            _ => None,
        })
        .collect();
    assert_eq!(slow.len(), 1, "exactly one recurring account poll");
    assert_eq!(slow[0].1, now + 60_000.0);
    engine.handle(
        now + 1.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let fast: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].1, now + 1_501.0);
    engine.handle(
        now + 2.0,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let paused: Vec<_> = engine.drain_actions().collect();
    assert!(
        paused
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == fast[0].0))
    );
    assert!(!paused.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    engine.handle(now + 1_501.0, Event::TimerFired { timer: fast[0].0 });
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
    engine.handle(
        now + 90_000.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let resumed: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, now + 91_500.0);
    engine.handle(
        now + 91_500.0,
        Event::TimerFired {
            timer: resumed[0].0,
        },
    );
    let reads: Vec<_> = safe_cex_engine::test_support::prepared_actions(engine, now + 91_500.0)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert!(
        reads.iter().any(|r| r.url.contains(endpoint)),
        "actual resumed account HTTP: {reads:?}"
    );
    let completed_at = now + 91_501.0;
    engine.handle(
        completed_at,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
    (completed_at, reads)
}

fn policy_poll_after_completion<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
) {
    use safe_cex_engine::{Action, Event};
    // A response may schedule an ordinary repair/deadline, but the parked poll
    // itself must still be registered and adopt this newest policy.
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 30_000.0 / POLL_MS,
        },
    );
    let times: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { at_ms, .. } => Some(at_ms),
            _ => None,
        })
        .collect();
    assert_eq!(times, vec![now + 30_000.0]);
}

#[test]
fn recurring_account_poll_obeys_live_policy_and_completion() {
    let mut h = ready();
    let now = h.now;
    let engine = &mut h.engine;
    let (now, mut reads) = policy_poll_read(engine, now, "/fapi/v3/account");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = if r.url.contains("positionRisk") {
                POSITION_RISK
            } else if r.url.contains("symbolConfig") {
                SYMBOL_CONFIG
            } else if r.url.contains("/account") {
                ACCOUNT
            } else {
                "[]"
            };
            engine.handle(
                now,
                safe_cex_engine::Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
        }
        reads = safe_cex_engine::test_support::prepared_actions(engine, now)
            .into_iter()
            .filter_map(|a| match a {
                safe_cex_engine::Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
    }
    assert!(reads.is_empty(), "account cycle must complete");
    policy_poll_after_completion(engine, now);
}

/// Closes the user stream, waits out its retry and answers the listen-key POST.
fn reconnect_private(h: &mut Harness) -> f64 {
    let socket = h.socket("/private/ws").unwrap();
    let before = h.timers.clone();
    h.event(Event::WsClosed { socket, code: 1006 });
    let delay = h
        .timers
        .iter()
        .filter(|t| !before.contains(t))
        .map(|(_, at)| at - h.now)
        .fold(f64::NAN, f64::min);
    h.advance(delay);
    h.respond("/fapi/v1/listenKey", 200, r#"{"listenKey":"fresh"}"#);
    let socket = h.socket("/private/ws").unwrap();
    h.event(Event::WsOpened { socket });
    delay
}

#[test]
fn a_flapping_user_stream_backs_off_although_each_attempt_opens() {
    let mut h = ready();
    for attempt in 0..6 {
        let nominal = 500.0 * 2f64.powi(attempt);
        let delay = reconnect_private(&mut h);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
    }
}

#[test]
fn the_user_stream_is_probed_every_five_seconds() {
    let mut h = ready();
    let private = h.socket("/private/ws").unwrap();
    let probes = |h: &Harness| {
        h.sent
            .iter()
            .filter(|(s, t)| *s == private && t == r#"{"id":42,"method":"LIST_SUBSCRIPTIONS"}"#)
            .count()
    };
    h.advance(4_999.0);
    assert_eq!(probes(&h), 0);
    h.advance(1.0);
    assert_eq!(probes(&h), 1);
    h.message("/private/ws", r#"{"result":[],"id":42}"#);
    h.advance(10_000.0);
    assert_eq!(probes(&h), 3);
}

#[test]
fn a_slow_listen_key_post_is_not_a_connect_timeout() {
    let mut h = ready();
    let socket = h.socket("/private/ws").unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    h.advance(500.0);
    assert!(h.has_request("/fapi/v1/listenKey"));
    // Slower than the 10 s connect timeout, within the 20 s session bound.
    h.advance(15_000.0);
    h.respond("/fapi/v1/listenKey", 200, r#"{"listenKey":"slow"}"#);
    let pending = h.socket("/private/ws").unwrap();
    assert_ne!(pending, socket);
    h.advance(9_999.0);
    assert!(!h.closed.contains(&pending));
    h.advance(1.0);
    assert!(h.closed.contains(&pending), "an unopened stream expires");
    h.advance(1_000.0);
    assert!(h.has_request("/fapi/v1/listenKey"), "and fetches a new key");
}

fn fills_over(h: &mut Harness, command: u32, days: f64) {
    h.event(Event::Command(Command::Query {
        command,
        query: Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(0.0),
            to_ms: Some(days * 86_400_000.0),
            limit: Some(10),
        },
    }));
}

const TRADE: &str = r#"[{"symbol":"BTCUSDT","id":7,"orderId":8,"side":"BUY","price":"100","qty":"1","realizedPnl":"0","commission":"0.1","commissionAsset":"USDT","time":1000,"positionSide":"BOTH","maker":false}]"#;

/// Pins the exact request sequence of windowed fills: one request per
/// 7-day window, oldest first. Paging refactors keep it.
#[test]
fn windowed_fills_request_sequence_is_pinned() {
    let mut h = ready();
    fills_over(&mut h, 22, 10.0);
    let mut sent = Vec::new();
    for _ in 0..2 {
        let r = h.take("/fapi/v1/userTrades");
        sent.push(
            r.url
                .split_once('?')
                .unwrap()
                .1
                .split('&')
                .filter(|p| {
                    !p.starts_with("timestamp=")
                        && !p.starts_with("signature=")
                        && !p.starts_with("recvWindow=")
                })
                .collect::<Vec<_>>()
                .join("&"),
        );
        h.reply(r.id, 200, "", TRADE);
    }
    assert!(h.result(22).unwrap().is_ok());
    assert_eq!(
        sent,
        [
            "symbol=BTCUSDT&limit=10&startTime=0&endTime=604800000",
            "symbol=BTCUSDT&limit=10&startTime=604800001&endTime=864000000",
        ]
    );
}

#[test]
fn fills_windows_cover_the_last_millisecond_of_the_range() {
    // [0, 7d] then the single millisecond 7d + 1, which the old split dropped.
    let mut h = ready();
    h.event(Event::Command(Command::Query {
        command: 24,
        query: Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(0.0),
            to_ms: Some(604_800_001.0),
            limit: Some(10),
        },
    }));
    let r = h.take("/fapi/v1/userTrades");
    h.reply(r.id, 200, "", "[]");
    let r = h.take("/fapi/v1/userTrades");
    assert_eq!(param(&r.url, "startTime").as_deref(), Some("604800001"));
    assert_eq!(param(&r.url, "endTime").as_deref(), Some("604800001"));
}

/// The shared history owner retains earlier accepted windows after a later
/// actual page failure and marks their missing coverage before completion.
#[test]
fn a_failed_later_fills_window_retains_prior_rows_with_owned_incompleteness() {
    let mut h = ready();
    fills_over(&mut h, 25, 10.0);
    let r = h.take("/fapi/v1/userTrades");
    h.reply(r.id, 200, "", TRADE);
    let r = h.take("/fapi/v1/userTrades");
    h.reply(r.id, 503, "", "unavailable");
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(h.result(25).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!h.command_errors.iter().any(|(command, _)| *command == 25));
    assert!(h.events.iter().any(|event| {
        event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
            && event
                .fields
                .contains(&("command", safe_cex_engine::Value::Num(25.0)))
    }));
    assert_eq!(h.command_notices, [(25, "incomplete"), (25, "done")]);
}

#[test]
fn trading_failures_carry_the_binance_code_kind_and_outcome() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let cases = [
        (
            400,
            r#"{"code":-2019,"msg":"Margin is insufficient."}"#,
            Some(ErrorKind::InsufficientBalance),
            Outcome::Rejected,
            Some("-2019"),
        ),
        (
            400,
            r#"{"code":-2018,"msg":"Balance is insufficient."}"#,
            Some(ErrorKind::InsufficientBalance),
            Outcome::Rejected,
            Some("-2018"),
        ),
        (
            400,
            r#"{"code":-2028,"msg":"Leverage is smaller than permitted: insufficient margin balance."}"#,
            Some(ErrorKind::InsufficientBalance),
            Outcome::Rejected,
            Some("-2028"),
        ),
        (
            400,
            r#"{"code":-2021,"msg":"Order would immediately trigger."}"#,
            Some(ErrorKind::InvalidInput),
            Outcome::Rejected,
            Some("-2021"),
        ),
        (
            400,
            r#"{"code":-5022,"msg":"Due to the order could not be executed as maker, the Post Only order will be rejected."}"#,
            Some(ErrorKind::InvalidInput),
            Outcome::Rejected,
            Some("-5022"),
        ),
        (
            400,
            r#"{"code":-2027,"msg":"Exceeded the maximum allowable position at current leverage."}"#,
            Some(ErrorKind::Rejected),
            Outcome::Rejected,
            Some("-2027"),
        ),
        (
            400,
            r#"{"code":-1111,"msg":"Precision is over the maximum defined for this asset."}"#,
            Some(ErrorKind::InvalidInput),
            Outcome::Rejected,
            Some("-1111"),
        ),
        (
            401,
            r#"{"code":-2015,"msg":"Invalid API-key, IP, or permissions for action."}"#,
            Some(ErrorKind::AuthInvalid),
            Outcome::Rejected,
            Some("-2015"),
        ),
        (
            429,
            r#"{"code":-1003,"msg":"Too many requests."}"#,
            Some(ErrorKind::RateLimited),
            Outcome::Rejected,
            Some("-1003"),
        ),
        (
            503,
            r#"{"code":-1007,"msg":"Timeout waiting for response from backend server. Send status unknown; execution status unknown."}"#,
            Some(ErrorKind::VenueUnavailable),
            Outcome::Unknown,
            Some("-1007"),
        ),
        (
            502,
            "<html>Bad Gateway</html>",
            Some(ErrorKind::VenueUnavailable),
            Outcome::Unknown,
            None,
        ),
        (
            503,
            r#"{"code":-2019,"msg":"Margin is insufficient."}"#,
            Some(ErrorKind::VenueUnavailable),
            Outcome::Unknown,
            Some("-2019"),
        ),
        (
            400,
            r#"{"code":-1000,"msg":"An unknown error occurred while processing the request."}"#,
            None,
            Outcome::Unknown,
            Some("-1000"),
        ),
    ];
    for (status, body, kind, outcome, code) in cases {
        let mut h = ready();
        place(&mut h, 3, vec![limit(OrderSide::Buy, 0.01, 90000.0)]);
        let order = h.take("/fapi/v1/order");
        h.reply(order.id, status, "", body);
        let [(3, error)] = h.failures.as_slice() else {
            panic!("{body}: {:?}", h.failures);
        };
        assert_eq!(
            (error.kind, error.outcome, error.venue_code.as_deref()),
            (kind, Some(outcome), code),
            "{body}"
        );
        assert_eq!(
            h.result(3),
            Some(&Err(error.message.clone())),
            "text is unchanged"
        );
        if status >= 500 {
            assert_eq!(
                error.message,
                "HTTP response does not prove the write outcome"
            );
            let native: Option<serde_json::Value> = serde_json::from_str(body).ok();
            assert_eq!(
                error.venue_message.as_deref(),
                native
                    .as_ref()
                    .and_then(|row| row.get("msg"))
                    .and_then(serde_json::Value::as_str)
            );
        }
    }

    let mut h = ready();
    place(&mut h, 4, vec![limit(OrderSide::Buy, 0.01, 90000.0)]);
    let order = h.take("/fapi/v1/order");
    h.event(Event::HttpFailed {
        request: order.id,
        error: "connection reset",
    });
    assert_eq!(h.failures[0].1.outcome, Some(Outcome::Unknown));
}

#[test]
fn a_rejected_replacement_after_a_confirmed_cancel_is_not_reported_as_rejected() {
    let mut h = ready();
    h.event(Event::Command(Command::AmendOrder {
        command: 12,
        id: "8389765519993909000".into(),
        price: Some(91000.0),
        amount: None,
    }));
    let cancel = h.take("/fapi/v1/order");
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"symbol":"BTCUSDT","orderId":8389765519993909000,"status":"CANCELED","origQty":"0.004","executedQty":"0.001"}"#,
    );
    let replacement = h.take("/fapi/v1/order");
    h.reply(
        replacement.id,
        400,
        "",
        r#"{"code":-2019,"msg":"Margin is insufficient."}"#,
    );
    let [(12, error)] = h.failures.as_slice() else {
        panic!("{:?}", h.failures);
    };
    assert_eq!(error.message, "Margin is insufficient.");
    assert_eq!(error.outcome, None, "the cancel already took effect");
}

// Keep unrelated balance polls responsive while the position/order fixture
// advances its original total time; their new engine deadlines are real.
fn advance_with_account_replies(h: &mut Harness, ms: f64) {
    let until = h.now + ms;
    while h.now < until {
        h.advance((until - h.now).min(1_000.0));
        while h.has_request("/fapi/v3/account") {
            h.respond("/fapi/v3/account", 200, ACCOUNT);
        }
        // A user-stream reconnect during the wait must finish within its
        // 20 s listenKey session bound, like the account siblings above.
        while h.has_request("/fapi/v1/listenKey") {
            h.respond("/fapi/v1/listenKey", 200, r#"{"listenKey":"renewed"}"#);
        }
    }
}

fn advance_to_position_read(h: &mut Harness, rows: &str) {
    advance_with_account_replies(h, 75_000.0);
    h.respond("/fapi/v3/positionRisk", 200, rows);
    h.respond("/fapi/v1/symbolConfig", 200, SYMBOL_CONFIG);
}

#[test]
fn an_unreadable_position_amount_keeps_that_leg_until_a_valid_snapshot_omits_it() {
    let mut h = ready();
    let row = |side: &str, amount: &str| {
        format!(
            r#"{{"symbol":"BTCUSDT","positionSide":"{side}","positionAmt":"{amount}","entryPrice":"95000.0","markPrice":"96000.0","unRealizedProfit":"1","liquidationPrice":"0","isolatedMargin":"0","notional":"960.0","updateTime":1720736417660}}"#
        )
    };
    advance_to_position_read(
        &mut h,
        &format!("[{},{}]", row("LONG", "0.010"), row("SHORT", "-0.020")),
    );
    let short = h.engine.store().position("BTCUSDT:short").unwrap().clone();
    assert_eq!(short.contracts, 0.02);

    advance_to_position_read(
        &mut h,
        &format!(
            "[{},{}]",
            row("LONG", "0.030"),
            row("SHORT", "garbled-amount")
        ),
    );
    let store = h.engine.store();
    assert_eq!(store.position("BTCUSDT:long").unwrap().contracts, 0.03);
    assert_eq!(store.position("BTCUSDT:short"), Some(&short));
    assert!(
        h.logs
            .iter()
            .any(|m| m == "Binance positionRisk row BTCUSDT has an unreadable positionAmt")
    );
    assert!(!h.logs.iter().any(|m| m.contains("garbled-amount")));

    advance_to_position_read(&mut h, &format!("[{}]", row("LONG", "0.030")));
    assert!(h.engine.store().position("BTCUSDT:short").is_none());
    assert_eq!(h.engine.store().positions().len(), 1);
}

#[test]
fn a_malformed_position_row_keeps_its_stored_leg_while_valid_peers_commit() {
    let mut h = ready();
    let row = |side: &str, amount: &str| {
        format!(
            r#"{{"symbol":"BTCUSDT","positionSide":"{side}","positionAmt":"{amount}","entryPrice":"95000.0","markPrice":"96000.0","unRealizedProfit":"1","liquidationPrice":"0","isolatedMargin":"0","notional":"960.0","updateTime":1720736417660}}"#
        )
    };
    advance_to_position_read(
        &mut h,
        &format!("[{},{}]", row("LONG", "0.010"), row("SHORT", "-0.020")),
    );
    let short = h.engine.store().position("BTCUSDT:short").unwrap().clone();
    h.logs.clear();

    let malformed = row("SHORT", "-0.050").replace(r#""entryPrice":"95000.0","#, "");
    advance_to_position_read(&mut h, &format!("[{},{malformed}]", row("LONG", "0.030")));
    let store = h.engine.store();
    assert_eq!(store.position("BTCUSDT:long").unwrap().contracts, 0.03);
    assert_eq!(store.position("BTCUSDT:short"), Some(&short));
    assert!(
        h.logs
            .iter()
            .any(|m| m == "Binance skipped malformed positionRisk row BTCUSDT:short")
    );

    // A row without an identity protects every stored leg but still lets peers commit.
    advance_to_position_read(
        &mut h,
        &format!("[{},{{\"positionAmt\":\"1\"}}]", row("LONG", "0.040")),
    );
    let store = h.engine.store();
    assert_eq!(store.position("BTCUSDT:long").unwrap().contracts, 0.04);
    assert_eq!(store.position("BTCUSDT:short"), Some(&short));
    assert!(
        h.logs
            .iter()
            .any(|m| m == "Binance skipped malformed positionRisk row without an identity")
    );

    advance_to_position_read(&mut h, &format!("[{}]", row("LONG", "0.040")));
    assert!(h.engine.store().position("BTCUSDT:short").is_none());
    assert_eq!(h.engine.store().positions().len(), 1);
}

fn risk_row(side: &str, amount: &str) -> String {
    format!(
        r#"{{"symbol":"BTCUSDT","positionSide":"{side}","positionAmt":"{amount}","entryPrice":"95000.0","markPrice":"96000.0","unRealizedProfit":"1","liquidationPrice":"0","isolatedMargin":"0","notional":"960.0","updateTime":1720736417660}}"#
    )
}

#[test]
fn a_skipped_position_row_with_no_stored_leg_is_an_error_logged_once_until_it_maps() {
    let mut h = ready();
    let malformed = risk_row("SHORT", "-0.050").replace(r#""entryPrice":"95000.0","#, "");
    let message = "Binance skipped malformed positionRisk row BTCUSDT:short";
    advance_to_position_read(
        &mut h,
        &format!("[{},{malformed}]", risk_row("LONG", "0.010")),
    );
    assert!(h.engine.store().position("BTCUSDT:short").is_none());
    assert_eq!(h.errors, [message]);
    // The same skip on the next poll is not reported again.
    advance_to_position_read(
        &mut h,
        &format!("[{},{malformed}]", risk_row("LONG", "0.010")),
    );
    assert_eq!(h.errors.len(), 1);
    assert_eq!(h.logs.iter().filter(|m| *m == message).count(), 1);

    // Once the leg maps, a recurrence is reported again, as a warning over its stored copy.
    let both = format!(
        "[{},{}]",
        risk_row("LONG", "0.010"),
        risk_row("SHORT", "-0.020")
    );
    advance_to_position_read(&mut h, &both);
    advance_to_position_read(
        &mut h,
        &format!("[{},{malformed}]", risk_row("LONG", "0.010")),
    );
    assert_eq!(h.errors.len(), 1);
    assert_eq!(h.logs.iter().filter(|m| *m == message).count(), 2);
}

#[test]
fn a_skipped_position_row_holds_the_startup_snapshot_while_the_catalogue_is_partial() {
    let mut h = Harness::started(credentials());
    let partial =
        support::EXCHANGE_INFO.replacen(r#""symbols":["#, r#""symbols":[{"symbol":1},"#, 1);
    h.respond("/fapi/v1/exchangeInfo", 200, &partial);
    h.respond("/fapi/v1/leverageBracket", 200, "[]");
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    let malformed = risk_row("BOTH", "0.050").replace(r#""entryPrice":"95000.0","#, "");
    h.respond("/fapi/v3/positionRisk", 200, &format!("[{malformed}]"));
    h.respond("/fapi/v1/symbolConfig", 200, SYMBOL_CONFIG);
    assert!(!h.engine.store().loaded().positions);
    assert!(h.errors.is_empty());
    // These siblings are outside the malformed-position assertion, but must
    // finish before their own engine deadlines while this test waits 75 s.
    h.respond("/fapi/v3/account", 200, ACCOUNT);
    h.respond(
        "/fapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/fapi/v1/openOrders", 200, "[]");
    h.respond("/fapi/v1/openAlgoOrders", 200, "[]");

    // Once discovery settles, the row is skipped and, with nothing stored, reported.
    h.advance(1_000.0);
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    // Catalogue refresh starts a new ticker seed. Complete those siblings so
    // discovery remains settled during the original position-refresh wait.
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    advance_to_position_read(&mut h, &format!("[{malformed}]"));
    assert!(h.engine.store().loaded().positions);
    assert_eq!(
        h.errors,
        ["Binance skipped malformed positionRisk row BTCUSDT"]
    );
}

#[test]
fn a_repeated_order_skip_logs_once_until_the_order_maps_again() {
    let mut h = ready();
    let order = |kind: &str| {
        format!(
            r#"[{{"orderId":2,"clientOrderId":"c2","executedQty":"0","origQty":"0.004","price":"89000","reduceOnly":false,"side":"BUY","status":"NEW","stopPrice":"0","closePosition":false,"symbol":"BTCUSDT","timeInForce":"GTC","type":"{kind}","workingType":"CONTRACT_PRICE"}}]"#
        )
    };
    let refresh = |h: &mut Harness, orders: &str| {
        let event = r#"{"e":"ORDER_TRADE_UPDATE","E":1568879465651,"T":1568879465650,"o":{"s":"BTCUSDT","c":"poke","S":"BUY","o":"LIMIT","f":"GTC","q":"0.001","p":"1000","ap":"0","sp":"0","x":"NEW","X":"NEW","i":9000,"l":"0","z":"0","L":"0","T":1568879465650,"t":0,"b":"0","a":"0","m":false,"R":false,"wt":"CONTRACT_PRICE","ot":"LIMIT","ps":"BOTH","cp":false,"rp":"0"}}"#;
        h.message("/private/ws", event);
        h.advance(10_000.0);
        h.respond("/fapi/v1/openOrders", 200, orders);
        h.respond("/fapi/v1/openAlgoOrders", 200, "[]");
    };
    let message = "Binance open order 2 has an unsupported type";
    let count = |h: &Harness| h.logs.iter().filter(|m| *m == message).count();
    refresh(&mut h, &order("FUTURE_KIND"));
    refresh(&mut h, &order("FUTURE_KIND"));
    assert_eq!(count(&h), 1);
    refresh(&mut h, &order("LIMIT"));
    refresh(&mut h, &order("FUTURE_KIND"));
    assert_eq!(count(&h), 2);
}

#[test]
fn an_open_order_of_unsupported_type_keeps_its_stored_copy_until_a_valid_snapshot_omits_it() {
    let mut h = ready();
    let order = |id: u64, kind: &str, price: &str| {
        format!(
            r#"{{"orderId":{id},"clientOrderId":"c{id}","executedQty":"0","origQty":"0.004","price":"{price}","reduceOnly":false,"side":"BUY","status":"NEW","stopPrice":"0","closePosition":false,"symbol":"BTCUSDT","timeInForce":"GTC","type":"{kind}","workingType":"CONTRACT_PRICE"}}"#
        )
    };
    let algo = |id: u64, kind: &str, trigger: &str| {
        format!(
            r#"{{"algoId":{id},"orderType":"{kind}","symbol":"BTCUSDT","side":"SELL","quantity":"0.004","triggerPrice":"{trigger}","workingType":"MARK_PRICE","reduceOnly":true,"closePosition":false}}"#
        )
    };
    let refresh = |h: &mut Harness, orders: &[String], algos: &[String]| {
        // Any private order event schedules the debounced REST order snapshot.
        let event = r#"{"e":"ORDER_TRADE_UPDATE","E":1568879465651,"T":1568879465650,"o":{"s":"BTCUSDT","c":"poke","S":"BUY","o":"LIMIT","f":"GTC","q":"0.001","p":"1000","ap":"0","sp":"0","x":"NEW","X":"NEW","i":9000,"l":"0","z":"0","L":"0","T":1568879465650,"t":0,"b":"0","a":"0","m":false,"R":false,"wt":"CONTRACT_PRICE","ot":"LIMIT","ps":"BOTH","cp":false,"rp":"0"}}"#;
        h.message("/private/ws", event);
        h.advance(10_000.0);
        h.respond(
            "/fapi/v1/openOrders",
            200,
            &format!("[{}]", orders.join(",")),
        );
        h.respond(
            "/fapi/v1/openAlgoOrders",
            200,
            &format!("[{}]", algos.join(",")),
        );
    };
    refresh(
        &mut h,
        &[order(1, "LIMIT", "90000"), order(2, "LIMIT", "89000")],
        &[
            algo(3, "STOP_MARKET", "85000"),
            algo(4, "STOP_MARKET", "84000"),
        ],
    );
    let stored = ["2", "4"].map(|id| h.engine.store().order(id).unwrap().clone());

    refresh(
        &mut h,
        &[order(1, "LIMIT", "90500"), order(2, "FUTURE_KIND", "1")],
        &[algo(3, "STOP_MARKET", "85500"), algo(4, "FUTURE_KIND", "1")],
    );
    let store = h.engine.store();
    assert_eq!(store.order("1").unwrap().price, 90500.0);
    assert_eq!(store.order("3").unwrap().price, 85500.0);
    for row in &stored {
        assert_eq!(store.order(&row.id), Some(row));
    }
    for id in ["2", "4"] {
        let expected = format!("Binance open order {id} has an unsupported type");
        assert!(h.logs.contains(&expected), "{expected}");
    }
    assert!(!h.logs.iter().any(|m| m.contains("FUTURE_KIND")));

    refresh(
        &mut h,
        &[order(1, "LIMIT", "90500")],
        &[algo(3, "STOP_MARKET", "85500")],
    );
    let ids: Vec<_> = h
        .engine
        .store()
        .orders()
        .iter()
        .map(|o| o.id.clone())
        .collect();
    assert_eq!(ids, ["1", "3"]);
}

#[test]
fn listen_key_failures_back_off_from_5s_to_the_60s_cap() {
    let mut h = ready();
    let socket = h.socket("/private/ws").unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    h.advance(500.0);
    for delay in [5_000.0, 10_000.0, 20_000.0, 40_000.0, 60_000.0, 60_000.0] {
        h.respond("/fapi/v1/listenKey", 503, "unavailable");
        let sent = h.now;
        assert!(
            h.timers
                .iter()
                .any(|(_, at)| support::jittered(at - sent, delay)),
            "next listen key retry after a jittered {delay} ms"
        );
        h.advance(delay);
        assert!(h.has_request("/fapi/v1/listenKey"));
    }
}

#[test]
fn a_skipped_order_is_an_error_only_when_no_stored_copy_is_kept() {
    let mut h = ready();
    let order = |id: u64, kind: &str| {
        format!(
            r#"{{"orderId":{id},"clientOrderId":"c{id}","executedQty":"0","origQty":"0.004","price":"89000","reduceOnly":false,"side":"BUY","status":"NEW","stopPrice":"0","closePosition":false,"symbol":"BTCUSDT","timeInForce":"GTC","type":"{kind}","workingType":"CONTRACT_PRICE"}}"#
        )
    };
    let refresh = |h: &mut Harness, orders: &[String]| {
        let event = r#"{"e":"ORDER_TRADE_UPDATE","E":1568879465651,"T":1568879465650,"o":{"s":"BTCUSDT","c":"poke","S":"BUY","o":"LIMIT","f":"GTC","q":"0.001","p":"1000","ap":"0","sp":"0","x":"NEW","X":"NEW","i":9000,"l":"0","z":"0","L":"0","T":1568879465650,"t":0,"b":"0","a":"0","m":false,"R":false,"wt":"CONTRACT_PRICE","ot":"LIMIT","ps":"BOTH","cp":false,"rp":"0"}}"#;
        h.message("/private/ws", event);
        advance_with_account_replies(h, 10_000.0);
        h.respond(
            "/fapi/v1/openOrders",
            200,
            &format!("[{}]", orders.join(",")),
        );
        h.respond("/fapi/v1/openAlgoOrders", 200, "[]");
    };
    refresh(&mut h, &[order(1, "LIMIT")]);
    assert!(h.engine.store().order("1").is_some());
    h.errors.clear();

    // Order 1 keeps its stored copy; order 2 has none and drops out.
    refresh(&mut h, &[order(1, "FUTURE_KIND"), order(2, "FUTURE_KIND")]);
    assert!(h.engine.store().order("1").is_some());
    assert!(h.engine.store().order("2").is_none());
    assert_eq!(h.errors, ["Binance open order 2 has an unsupported type"]);
    assert!(
        h.logs
            .contains(&"Binance open order 1 has an unsupported type".to_owned())
    );
    let bad = [order(1, "FUTURE_KIND"), order(2, "FUTURE_KIND")];
    refresh(&mut h, &bad);
    assert_eq!(h.errors.len(), 1, "repeated missing rows stay suppressed");
    // Reused id on another symbol is not the rejected row's retained copy.
    h.engine
        .store_mut()
        .update_order("1", |o| o.symbol = "ETHUSDT".into());
    refresh(&mut h, &bad);
    assert_eq!(h.errors.len(), 2, "a prior Warning must upgrade once");
    assert_eq!(h.errors[1], "Binance open order 1 has an unsupported type");
    refresh(&mut h, &bad);
    assert_eq!(h.errors.len(), 2);
    refresh(&mut h, &[order(1, "LIMIT"), order(2, "LIMIT")]);
    h.engine.store_mut().update_order("2", |o| {
        o.status = safe_cex_engine::model::OrderStatus::Canceled
    });
    refresh(&mut h, &bad);
    assert_eq!(h.errors.len(), 3, "mapping re-arms the missing-copy error");
}

#[test]
fn exhausted_snapshot_authority_keeps_stream_writes_and_never_issues_an_unfenced_read() {
    let mut h = ready();
    safe_cex_engine::test_support::exhaust_order_snapshots(h.engine.store_mut());
    h.message("/private/ws", r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"BTCUSDT","c":"fresh","i":99,"S":"BUY","o":"LIMIT","f":"GTC","q":"1","p":"90","x":"NEW","X":"NEW","l":"0","z":"0","L":"0","t":0}}"#);
    assert!(
        h.engine.store().order("99").is_some(),
        "native writes stay admissible"
    );
    assert!(h.engine.store().loaded().orders);
    for _ in 0..3 {
        h.advance(POLL_MS);
        assert!(!h.has_request("/fapi/v1/openOrders"));
        assert!(!h.has_request("/fapi/v1/openAlgoOrders"));
        assert!(h.engine.store().order("99").is_some());
    }
}

fn listen_key_requests(h: &Harness) -> Vec<&safe_cex_engine::HttpRequest> {
    h.requests
        .iter()
        .filter(|request| request.url.contains("/fapi/v1/listenKey"))
        .collect()
}

#[test]
fn a_hung_listen_key_create_is_bounded_and_backs_off() {
    let mut h = ready();
    let socket = h.socket("/private/ws").unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    while listen_key_requests(&h).is_empty() {
        h.advance(1.0);
    }
    assert_eq!(listen_key_requests(&h).len(), 1);
    h.advance(19_999.0);
    assert!(h.errors.is_empty(), "{:?}", h.errors);
    h.advance(1.0);
    assert_eq!(
        h.errors,
        ["Binance user data stream unavailable: network error: HTTP request timed out"]
    );
    h.take("/fapi/v1/listenKey");
    h.advance(5_000.0);
    assert_eq!(listen_key_requests(&h).len(), 1, "one create after backoff");
}

#[test]
fn listen_key_requests_present_only_the_key_on_livenet_and_sign_on_testnet() {
    for testnet in [false, true] {
        let mut h = Harness::started(Options {
            testnet,
            ..credentials()
        });
        h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
        let create = listen_key_requests(&h)
            .first()
            .copied()
            .cloned()
            .expect("listenKey create after the catalogue");
        assert_eq!(create.method, Method::Post);
        assert!(
            create
                .headers
                .iter()
                .any(|(name, value)| *name == "X-MBX-APIKEY" && value == "api-key")
        );
        assert_eq!(create.url.contains("signature="), testnet, "{}", create.url);
        assert_eq!(create.url.contains("timestamp="), testnet, "{}", create.url);
    }
}
