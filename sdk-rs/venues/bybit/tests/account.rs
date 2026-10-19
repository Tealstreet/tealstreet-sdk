//! Private data and trading against Bybit's documented payloads.
mod support;

use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::model::{
    MarginType, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, PositionSide, RejectReason,
};
use safe_cex_engine::{Command, Event, HttpRequest, Method, Query};
use safe_cex_venue_bybit::Options;
use support::Harness;

/// "Get Open & Closed Orders", one-way: a limit buy with attached TP/SL.
const OPEN_ORDERS: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"orderId":"fd4300ae-7847-404e-b947-b46980a4d140","orderLinkId":"test-000005","blockTradeId":"","symbol":"ETHUSDT","price":"1600.00","qty":"0.10","side":"Buy","isLeverage":"","positionIdx":0,"orderStatus":"New","cancelType":"UNKNOWN","rejectReason":"EC_NoError","avgPrice":"0","leavesQty":"0.10","leavesValue":"160","cumExecQty":"0.00","cumExecValue":"0","cumExecFee":"0","timeInForce":"GTC","orderType":"Limit","stopOrderType":"UNKNOWN","orderIv":"","triggerPrice":"0.00","takeProfit":"2500.00","stopLoss":"1500.00","tpTriggerBy":"LastPrice","slTriggerBy":"LastPrice","triggerDirection":0,"triggerBy":"UNKNOWN","lastPriceOnCreated":"","reduceOnly":false,"closeOnTrigger":false,"smpType":"None","smpGroup":"0","smpOrderId":"","tpslMode":"Full","tpLimitPrice":"","slLimitPrice":"","placeType":"","createdTime":"1684738540559","updatedTime":"1684738540561"}],"nextPageCursor":"page_args%3Dfd4300ae-7847-404e-b947-b46980a4d140%26symbol%3D6%26","category":"linear"},"retExtInfo":{},"time":1684765770483}"#;
const EMPTY_LIST: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":"","category":"linear"},"retExtInfo":{},"time":1684765770483}"#;
/// "Get Position Info", as a one-way BTCUSDT long.
const POSITIONS: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"positionIdx":0,"riskId":1,"riskLimitValue":"2000000","symbol":"BTCUSDT","side":"Buy","size":"0.010","avgPrice":"95000","positionValue":"950","tradeMode":0,"positionStatus":"Normal","autoAddMargin":0,"adlRankIndicator":2,"leverage":"10","breakEvenPrice":"95010","positionBalance":"95","markPrice":"96000","liqPrice":"86000.5","bustPrice":"","positionMM":"5","positionMMByMp":"5","positionIM":"95","positionIMByMp":"95","tpslMode":"Full","takeProfit":"0.00","stopLoss":"0.00","trailingStop":"0.00","unrealisedPnl":"10","curRealisedPnl":"-0.5","cumRealisedPnl":"-1","seq":5723621632,"isReduceOnly":false,"mmrSysUpdateTime":"","leverageSysUpdatedTime":"","sessionAvgPrice":"","createdTime":"1676538056258","updatedTime":"1697673600012"},{"positionIdx":0,"symbol":"ETHUSDT","side":"","size":"0","avgPrice":"0","positionValue":"0","leverage":"20","unrealisedPnl":"0","curRealisedPnl":"0","liqPrice":"","positionIM":"0","seq":-1,"positionStatus":"","updatedTime":""}],"nextPageCursor":"","category":"linear"},"retExtInfo":{},"time":1697684980172}"#;
/// "Get Wallet Balance".
const WALLET: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"totalEquity":"1010.5","accountIMRate":"0","accountIMRateByMp":"0","totalMarginBalance":"1010.5","totalInitialMargin":"95","totalInitialMarginByMp":"95","accountType":"UNIFIED","totalAvailableBalance":"915.5","accountMMRate":"0","accountMMRateByMp":"0","totalPerpUPL":"10","totalWalletBalance":"1000.5","accountLTV":"0","totalMaintenanceMargin":"5","totalMaintenanceMarginByMp":"5","coin":[{"availableToBorrow":"3","bonus":"0","accruedInterest":"0","availableToWithdraw":"0","totalOrderIM":"0","equity":"1010.5","totalPositionMM":"5","usdValue":"1010.5","spotHedgingQty":"0","unrealisedPnl":"10","collateralSwitch":true,"borrowAmount":"0.0","totalPositionIM":"95","walletBalance":"1000.5","cumRealisedPnl":"0","locked":"0","marginCollateral":true,"coin":"USDT","spotBorrow":"0"}]}]},"retExtInfo":{},"time":1690872862481}"#;
const ORDER_ID: &str = "fd4300ae-7847-404e-b947-b46980a4d140";
const OK_EMPTY: &str =
    r#"{"retCode":0,"retMsg":"OK","result":{},"retExtInfo":{},"time":1672281607343}"#;

fn credentials() -> Options {
    Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }
}

fn respond_startup(h: &mut Harness) {
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    h.load_public();
}

/// Answer the account snapshot reads.
fn respond_snapshot(h: &mut Harness) {
    let orders = h.take("/v5/order/realtime?category=linear&settleCoin=USDT");
    h.reply(orders.id, 200, OPEN_ORDERS);
    // The documented page names a cursor; its next page is empty.
    let next = h.take("/v5/order/realtime?category=linear&settleCoin=USDT");
    assert!(next.url.ends_with(
        "&cursor=page_args%253Dfd4300ae-7847-404e-b947-b46980a4d140%2526symbol%253D6%2526"
    ));
    h.reply(next.id, 200, EMPTY_LIST);
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDT",
        200,
        POSITIONS,
    );
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    h.respond("/v5/account/wallet-balance", 200, WALLET);
}

/// A private engine with the account loaded and the private socket authenticated.
fn ready() -> Harness {
    let mut h = Harness::started(credentials());
    respond_startup(&mut h);
    respond_snapshot(&mut h);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"auth","conn_id":"cejreaspqfh3sjdnldmg-p"}"#,
    );
    h.requests.clear();
    h
}

fn place(h: &mut Harness, command: u32, orders: Vec<PlaceOrderOpts>) {
    h.command(Command::PlaceOrders { command, orders });
}

fn limit(symbol: &str, side: OrderSide, amount: f64, price: f64) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: symbol.into(),
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
        close: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
    }
}

fn body(request: &HttpRequest) -> &str {
    request.body.as_deref().unwrap_or_default()
}

/// The signature Bybit expects: HMAC(timestamp + key + recvWindow + payload).
fn expected_signature(request: &HttpRequest) -> String {
    let payload = match request.method {
        Method::Get => request.url.split_once('?').map_or("", |(_, q)| q),
        _ => body(request),
    };
    let timestamp = Harness::header(request, "X-BAPI-TIMESTAMP").unwrap();
    hmac_sha256(
        b"secret",
        format!("{timestamp}api-key20000{payload}").as_bytes(),
        Encoding::Hex,
    )
}

#[test]
fn private_startup_signs_reads_and_loads_the_account() {
    let mut h = Harness::started(credentials());
    let info = h.take("/v5/account/info");
    assert_eq!(info.url, "https://api.bybit.com/v5/account/info");
    assert_eq!(
        Harness::header(&info, "X-BAPI-API-KEY").as_deref(),
        Some("api-key")
    );
    assert_eq!(
        Harness::header(&info, "X-BAPI-TIMESTAMP").as_deref(),
        Some("1000")
    );
    assert_eq!(
        Harness::header(&info, "X-BAPI-RECV-WINDOW").as_deref(),
        Some("20000")
    );
    assert_eq!(
        Harness::header(&info, "X-Referer").as_deref(),
        Some("Tealstreet")
    );
    assert_eq!(
        Harness::header(&info, "X-BAPI-SIGN"),
        Some(expected_signature(&info))
    );
    h.reply(info.id, 200, support::ACCOUNT_INFO);
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.load_public();
    assert_eq!(
        h.engine.store().market("BTCUSDT").unwrap().taker_fee,
        Some(0.00055)
    );
    assert!(h.has_request("/v5/order/realtime?category=linear&settleCoin=USDT&limit=50&cursor="));
    respond_snapshot(&mut h);

    let store = h.engine.store();
    let loaded = store.loaded();
    assert!(
        loaded.balance && loaded.orders && loaded.positions && loaded.markets && loaded.tickers
    );
    assert!(!store.hedged());

    let orders = store.orders();
    assert_eq!(orders.len(), 3, "the order and its attached TP/SL");
    assert_eq!(orders[0].id, ORDER_ID);
    assert_eq!(
        (
            orders[0].price,
            orders[0].amount,
            orders[0].remaining,
            orders[0].kind
        ),
        (1600.0, 0.1, 0.1, OrderType::Limit)
    );
    assert_eq!(orders[0].client_order_id.as_deref(), Some("test-000005"));
    let stop = store.order(&format!("{ORDER_ID}__stop_loss")).unwrap();
    assert_eq!(
        (stop.kind, stop.side, stop.price, stop.remaining),
        (OrderType::StopLoss, OrderSide::Sell, 1500.0, 0.1)
    );
    assert_eq!(stop.parent_id.as_deref(), Some(ORDER_ID));
    let take_profit = store.order(&format!("{ORDER_ID}__take_profit")).unwrap();
    assert_eq!(take_profit.price, 2500.0);

    assert_eq!(
        store.positions().len(),
        1,
        "flat rows are settings, not positions"
    );
    let position = &store.positions()[0];
    assert_eq!(position.id, "BTCUSDT");
    assert_eq!(
        (position.side, position.contracts, position.entry_price),
        (PositionSide::Long, 0.01, 95000.0)
    );
    assert_eq!(
        (
            position.leverage,
            position.liquidation_price,
            position.margin_size
        ),
        (10.0, 86000.5, Some(95.0))
    );
    assert_eq!(position.margin_type, Some(MarginType::Cross));
    assert_eq!(store.setting("ETHUSDT").unwrap().leverage, 20.0);
    assert_eq!(store.setting("BTCUSDT").unwrap().leverage, 10.0);

    let balance = store.balance();
    assert_eq!(
        (balance.total, balance.upnl, balance.used, balance.free),
        (1000.5, 10.0, 100.0, 915.5)
    );
}

#[test]
fn the_private_socket_authenticates_before_it_subscribes() {
    let mut h = Harness::started(credentials());
    respond_startup(&mut h);
    respond_snapshot(&mut h);
    let socket = h.socket("/v5/private").unwrap();
    assert_eq!(h.socket("/v5/private"), Some(socket));
    h.event(Event::WsOpened { socket });
    let auth = h.sent_on("/v5/private");
    assert_eq!(auth.len(), 1);
    // "Authentication": expires is ten seconds past now.
    let expires = h.now + 10_000.0;
    let signature = hmac_sha256(
        b"secret",
        format!("GET/realtime{expires}").as_bytes(),
        Encoding::Hex,
    );
    assert_eq!(
        auth[0],
        format!(r#"{{"op":"auth","args":["api-key","{expires}","{signature}"]}}"#)
    );
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"auth","conn_id":"cejreaspqfh3sjdnldmg-p"}"#,
    );
    assert_eq!(
        h.sent_on("/v5/private")[1],
        r#"{"op":"subscribe","args":["order","execution","position","wallet"]}"#
    );
}

#[test]
fn a_refused_auth_closes_the_socket_and_reconnects() {
    let mut h = Harness::started(credentials());
    respond_startup(&mut h);
    respond_snapshot(&mut h);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "/v5/private",
        r#"{"success":false,"ret_msg":"Params Error","op":"auth","conn_id":"cejreaspqfh3sjdnldmg-p"}"#,
    );
    assert!(h.closed.contains(&socket));
    assert_eq!(
        h.sent_on("/v5/private").len(),
        1,
        "no subscribe after a refusal"
    );
    assert!(h.logs.iter().any(|m| m == "[WS]auth:Params Error"));
    h.advance(1_000.0);
    assert_ne!(h.socket("/v5/private"), Some(socket));
}

#[test]
fn the_first_subscription_resyncs_orders_changed_before_the_stream_was_live() {
    let mut h = ready();
    assert!(h.engine.store().order(ORDER_ID).is_some());
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"subscribe"}"#,
    );
    assert!(h.has_request("/v5/position/list"));
    assert!(h.has_request("/v5/account/wallet-balance"));
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDT",
        200,
        EMPTY_LIST,
    );
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    assert!(h.engine.store().orders().is_empty());
}

#[test]
fn subscribing_during_startup_queues_fresh_snapshots_after_the_in_flight_reads() {
    let mut h = Harness::started(credentials());
    respond_startup(&mut h);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"auth"}"#,
    );
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"subscribe"}"#,
    );
    respond_snapshot(&mut h);
    assert!(h.has_request("/v5/order/realtime?category=linear&settleCoin=USDT"));
    assert!(h.has_request("/v5/order/realtime?category=linear&settleCoin=USDC"));
    assert!(h.has_request("/v5/position/list?category=linear&settleCoin=USDT"));
    assert!(h.has_request("/v5/position/list?category=linear&settleCoin=USDC"));
    assert!(h.has_request("/v5/account/wallet-balance"));
    respond_snapshot(&mut h);
    assert!(h.requests.is_empty(), "the queued refresh ran exactly once");
}

/// "Order", the documented frame, as a linear one-way order.
fn order_frame(status: &str, cum: &str, leaves: &str, reject: &str) -> String {
    format!(
        r#"{{"id":"5923240c6880ab-c59f-420b-9adb-3639adc9dd90","topic":"order","creationTime":1672364262474,"data":[{{"symbol":"BTCUSDT","orderId":"5cf98598-39a7-459e-97bf-76ca765ee020","side":"Sell","orderType":"Limit","cancelType":"UNKNOWN","price":"97000","qty":"0.002","orderIv":"","timeInForce":"PostOnly","orderStatus":"{status}","orderLinkId":"web","lastPriceOnCreated":"","reduceOnly":false,"leavesQty":"{leaves}","leavesValue":"","cumExecQty":"{cum}","cumExecValue":"0","avgPrice":"97000","blockTradeId":"","positionIdx":0,"cumExecFee":"0","closedPnl":"0","createdTime":"1672364262444","updatedTime":"1672364262457","rejectReason":"{reject}","stopOrderType":"","tpslMode":"","triggerPrice":"","takeProfit":"","stopLoss":"","tpTriggerBy":"","slTriggerBy":"","tpLimitPrice":"","slLimitPrice":"","triggerDirection":0,"triggerBy":"","closeOnTrigger":false,"category":"linear","placeType":"","smpType":"None","smpGroup":"0","smpOrderId":"","feeCurrency":""}}]}}"#
    )
}

/// "Execution", the documented frame, as one fill of that order.
fn execution_frame(qty: &str, price: &str) -> String {
    format!(
        r#"{{"topic":"execution","id":"386825804_BTCUSDT_140612148849382","creationTime":1746270400355,"data":[{{"category":"linear","symbol":"BTCUSDT","closedSize":"0.5","execFee":"26.3725275","execId":"0ab1bdf7-4219-438b-b30a-32ec863018f7","execPrice":"{price}","execQty":"{qty}","execType":"Trade","execValue":"47950.05","feeRate":"0.00055","tradeIv":"","markIv":"","blockTradeId":"","markPrice":"95901.48","indexPrice":"","underlyingPrice":"","leavesQty":"0","orderId":"5cf98598-39a7-459e-97bf-76ca765ee020","orderLinkId":"","orderPrice":"94942.5","orderQty":"0.5","orderType":"Limit","stopOrderType":"UNKNOWN","side":"Sell","execTime":"1746270400353","isLeverage":"0","isMaker":true,"seq":140612148849382,"marketUnit":"","execPnl":"0.05","createType":"CreateByUser","feeCurrency":"USDT"}}]}}"#
    )
}

fn fills_in(h: &mut Harness) -> u32 {
    let frame = h.engine.take_frame(h.now).unwrap().to_vec();
    safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == 11)
        .map_or(0, |(_, payload)| {
            u32::from_le_bytes(payload[..4].try_into().unwrap())
        })
}

#[test]
fn order_and_execution_frames_drive_the_lifecycle_and_report_each_execution() {
    let mut h = ready();
    let id = "5cf98598-39a7-459e-97bf-76ca765ee020";
    h.message(
        "/v5/private",
        &order_frame("New", "0", "0.002", "EC_NoError"),
    );
    let order = h.engine.store().order(id).unwrap().clone();
    assert_eq!(
        (order.status, order.side, order.price, order.amount),
        (OrderStatus::Open, OrderSide::Sell, 97000.0, 0.002)
    );
    h.engine.take_frame(h.now);

    h.message(
        "/v5/private",
        &order_frame("PartiallyFilled", "0.001", "0.001", "EC_NoError"),
    );
    h.message("/v5/private", &execution_frame("0.001", "97000.5"));
    let order = h.engine.store().order(id).unwrap();
    assert_eq!((order.filled, order.remaining), (0.001, 0.001));
    assert_eq!(fills_in(&mut h), 1, "one execution, one fill");

    h.message(
        "/v5/private",
        &order_frame("Filled", "0.002", "0", "EC_NoError"),
    );
    h.message("/v5/private", &execution_frame("0.001", "97001"));
    assert_eq!(
        h.engine.store().order(id).unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(fills_in(&mut h), 1);

    // A post-only order that would have taken liquidity is cancelled with its reason.
    let maker = order_frame("New", "0", "0.002", "EC_NoError").replace(id, "maker-1");
    h.message("/v5/private", &maker);
    let rejected =
        order_frame("Cancelled", "0", "0", "EC_PostOnlyWillTakeLiquidity").replace(id, "maker-1");
    h.message("/v5/private", &rejected);
    let order = h.engine.store().order("maker-1").unwrap();
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(
        order.extra.as_ref().and_then(|e| e.reject_reason),
        Some(RejectReason::PostOnly)
    );
    assert!(h.logs.iter().any(|m| m == "EC_PostOnlyWillTakeLiquidity"));

    // The shared private socket carries other categories; this connection ignores them.
    h.message(
        "/v5/private",
        &order_frame("New", "0", "1", "EC_NoError")
            .replace(id, "inverse-1")
            .replace(r#""category":"linear""#, r#""category":"inverse""#),
    );
    assert!(h.engine.store().order("inverse-1").is_none());
}

/// "Position", the documented frame, as the BTCUSDT long.
fn position_frame(size: &str) -> String {
    format!(
        r#"{{"id":"1003076014fb7eedb-c7e6-45d6-a8c1-270f0169171a","topic":"position","creationTime":1697682317044,"data":[{{"positionIdx":0,"tradeMode":0,"riskId":1,"riskLimitValue":"2000000","symbol":"BTCUSDT","side":"Buy","size":"{size}","entryPrice":"95000","leverage":"10","breakEvenPrice":"95010","positionValue":"1425","positionBalance":"0","markPrice":"96000","positionIM":"142.5","positionIMByMp":"142.5","positionMM":"0","positionMMByMp":"0","takeProfit":"0","stopLoss":"0","trailingStop":"0","unrealisedPnl":"15","curRealisedPnl":"1.26","cumRealisedPnl":"-25.06579337","sessionAvgPrice":"0","createdTime":"1694402496913","updatedTime":"1697682317038","tpslMode":"Full","liqPrice":"80000","bustPrice":"","category":"linear","positionStatus":"Normal","adlRankIndicator":0,"autoAddMargin":0,"leverageSysUpdatedTime":"","mmrSysUpdatedTime":"","seq":8327597863,"isReduceOnly":false}}]}}"#
    )
}

#[test]
fn a_stale_position_snapshot_cannot_overwrite_a_newer_stream_event() {
    let mut h = ready();
    h.advance(5_000.0);
    let usdt = h.take("/v5/position/list?category=linear&settleCoin=USDT");
    let usdc = h.take("/v5/position/list?category=linear&settleCoin=USDC");
    h.message("/v5/private", &position_frame("0.015"));
    let position = &h.engine.store().positions()[0];
    assert_eq!((position.contracts, position.unrealized_pnl), (0.015, 15.0));
    assert_eq!(
        h.engine.store().balance().upnl,
        15.0,
        "PnL follows the stream"
    );
    h.reply(usdt.id, 200, POSITIONS);
    h.reply(usdc.id, 200, EMPTY_LIST);
    assert_eq!(
        h.engine.store().positions()[0].contracts,
        0.015,
        "the older REST snapshot was discarded"
    );
    // The next reconciliation, read after the event, applies.
    h.advance(5_000.0);
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDT",
        200,
        POSITIONS,
    );
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    assert_eq!(h.engine.store().positions()[0].contracts, 0.01);

    h.message("/v5/private", &position_frame("0"));
    assert!(h.engine.store().positions().is_empty());
}

#[test]
fn mixed_symbol_modes_include_flat_rows_and_survive_a_stale_snapshot() {
    let mut h = ready();
    h.message("/v5/private", &position_frame("0"));
    h.advance(5_000.0);
    let usdt = h.take("/v5/position/list?category=linear&settleCoin=USDT");
    let usdc = h.take("/v5/position/list?category=linear&settleCoin=USDC");
    h.message(
        "/v5/private",
        &position_frame("0").replace(r#""positionIdx":0"#, r#""positionIdx":1"#),
    );
    h.reply(usdt.id, 200, POSITIONS);
    h.reply(usdc.id, 200, EMPTY_LIST);
    assert!(h.engine.store().hedged());
    assert!(h.engine.store().positions().is_empty());

    place(
        &mut h,
        71,
        vec![
            limit("BTCUSDT", OrderSide::Buy, 0.001, 90_000.0),
            limit("ETHUSDT", OrderSide::Sell, 0.01, 3_100.0),
        ],
    );
    let request = h.take("/v5/order/create-batch");
    let body = body(&request);
    let (btc, eth) = body.split_once(r#""symbol":"ETHUSDT""#).unwrap();
    assert!(btc.contains(r#""positionIdx":1"#));
    assert!(eth.contains(r#""positionIdx":0"#));
}

#[test]
fn a_snapshot_retains_the_mode_of_a_flat_hedged_symbol() {
    let mut h = Harness::started(credentials());
    respond_startup(&mut h);
    let hedged_flat = POSITIONS.replace(
        r#""positionIdx":0,"symbol":"ETHUSDT""#,
        r#""positionIdx":2,"symbol":"ETHUSDT""#,
    );
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDT",
        200,
        &hedged_flat,
    );
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    assert!(h.engine.store().hedged());
    assert_eq!(h.engine.store().positions().len(), 1);
    place(
        &mut h,
        72,
        vec![limit("ETHUSDT", OrderSide::Sell, 0.01, 3_100.0)],
    );
    let request = h.take("/v5/order/create-batch");
    assert!(body(&request).contains(r#""positionIdx":2"#));
}

#[test]
fn an_error_envelope_in_place_of_a_list_keeps_the_last_snapshot() {
    let mut h = ready();
    h.advance(5_000.0);
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDT",
        200,
        r#"{"retCode":10016,"retMsg":"Internal error","result":{},"retExtInfo":{},"time":1697684980172}"#,
    );
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
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
fn a_rejected_signature_is_resent_once() {
    let mut h = ready();
    h.advance(5_000.0);
    h.respond(
        "/v5/account/wallet-balance",
        200,
        r#"{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param. req_timestamp[6000],server_timestamp[8000],recv_window[20000]","result":{},"retExtInfo":{},"time":8000}"#,
    );
    let resent = h.take("/v5/account/wallet-balance");
    assert_eq!(
        Harness::header(&resent, "X-BAPI-TIMESTAMP").as_deref(),
        Some("8000"),
        "the clock is corrected from Bybit's own"
    );
    h.reply(
        resent.id,
        200,
        r#"{"retCode":10004,"retMsg":"error sign!","result":{},"retExtInfo":{},"time":8000}"#,
    );
    assert!(!h.has_request("/v5/account/wallet-balance"), "only once");
}

#[test]
fn places_a_batch_with_attached_brackets_and_exact_parameters() {
    let mut h = ready();
    let mut entry = limit("BTCUSDT", OrderSide::Buy, 0.0123, 90000.04);
    entry.stop_loss = Some(85000.0);
    entry.take_profit = Some(99000.0);
    let second = limit("ETHUSDT", OrderSide::Sell, 1.0, 3100.0);
    place(&mut h, 7, vec![entry, second]);
    let batch = h.take("/v5/order/create-batch");
    assert_eq!(batch.method, Method::Post);
    assert_eq!(batch.url, "https://api.bybit.com/v5/order/create-batch");
    assert_eq!(
        body(&batch),
        r#"{"category":"linear","request":[{"symbol":"BTCUSDT","side":"Buy","orderType":"Limit","qty":"0.012","price":"90000","stopLoss":"85000","takeProfit":"99000","reduceOnly":false,"slTriggerBy":"MarkPrice","tpTriggerBy":"LastPrice","timeInForce":"GTC","closeOnTrigger":false,"positionIdx":0},{"symbol":"ETHUSDT","side":"Sell","orderType":"Limit","qty":"1","price":"3100","reduceOnly":false,"timeInForce":"GTC","closeOnTrigger":false,"positionIdx":0}]}"#
    );
    assert_eq!(
        Harness::header(&batch, "X-BAPI-SIGN"),
        Some(expected_signature(&batch))
    );
    // "Batch Place Order": one result and one status per request.
    h.reply(
        batch.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"category":"linear","symbol":"BTCUSDT","orderId":"1666800494330512128","orderLinkId":"","createAt":"1713434102752"},{"category":"linear","symbol":"ETHUSDT","orderId":"","orderLinkId":"","createAt":""}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"},{"code":110007,"msg":"Insufficient available balance"}]},"time":1713434102753}"#,
    );
    assert_eq!(
        h.result(7),
        Some(&Ok(r#"["1666800494330512128"]"#.to_owned()))
    );
    assert!(
        h.logs
            .iter()
            .any(|m| m == "Order 2: Insufficient available balance")
    );
}

#[test]
fn a_batch_refused_whole_falls_back_to_single_orders() {
    let mut h = ready();
    place(
        &mut h,
        8,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    h.respond(
        "/v5/order/create-batch",
        200,
        r#"{"retCode":10001,"retMsg":"params error","result":{},"retExtInfo":{},"time":1}"#,
    );
    let single = h.take("/v5/order/create");
    assert_eq!(
        body(&single),
        r#"{"category":"linear","symbol":"BTCUSDT","side":"Buy","orderType":"Limit","qty":"0.01","price":"90000","reduceOnly":false,"timeInForce":"GTC","closeOnTrigger":false,"positionIdx":0}"#
    );
    // "Place Order", the documented response.
    h.reply(
        single.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"1321003749386327552","orderLinkId":"spot-test-postonly"},"retExtInfo":{},"time":1672211918471}"#,
    );
    assert_eq!(
        h.result(8),
        Some(&Ok(r#"["1321003749386327552"]"#.to_owned()))
    );
}

#[test]
fn a_transport_failure_on_a_batch_is_not_resent() {
    let mut h = ready();
    place(
        &mut h,
        9,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    let batch = h.take("/v5/order/create-batch").id;
    h.event(Event::HttpFailed {
        request: batch,
        error: "connection reset",
    });
    assert!(h.requests.is_empty(), "the venue may have the orders");
    assert!(matches!(h.result(9), Some(Err(message)) if message == "connection reset"));
}

#[test]
fn a_client_order_id_goes_out_verbatim_or_refuses_the_batch() {
    let mut h = ready();
    let mut first = limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0);
    first.client_order_id = Some("usdt-test-01".into());
    place(&mut h, 4, vec![first.clone()]);
    assert!(body(&h.take("/v5/order/create-batch")).contains(r#""orderLinkId":"usdt-test-01""#));

    let mut second = limit("BTCUSDT", OrderSide::Buy, 0.01, 89000.0);
    second.client_order_id = Some("not valid!".into());
    place(&mut h, 5, vec![first.clone(), second]);
    assert!(h.requests.is_empty());
    assert!(matches!(h.result(5), Some(Err(message)) if message.contains("not valid!")));

    let mut oversized = first.clone();
    oversized.amount = 2000.0;
    place(&mut h, 6, vec![oversized]);
    assert!(h.requests.is_empty(), "it would split into two orders");
    assert!(matches!(h.result(6), Some(Err(_))));

    let mut stop = first;
    stop.stop_loss = Some(80000.0);
    place(&mut h, 10, vec![stop]);
    assert!(h.requests.is_empty());
}

#[test]
fn a_stop_against_the_open_position_goes_through_trading_stop() {
    let mut h = ready();
    let mut stop = limit("BTCUSDT", OrderSide::Sell, 0.0, 86000.04);
    stop.kind = OrderType::StopLoss;
    place(&mut h, 11, vec![stop]);
    let request = h.take("/v5/position/trading-stop");
    assert_eq!(
        body(&request),
        r#"{"category":"linear","symbol":"BTCUSDT","positionIdx":0,"tpslMode":"Full","stopLoss":"86000","slTriggerBy":"MarkPrice"}"#
    );
    h.reply(request.id, 200, OK_EMPTY);
    assert_eq!(h.result(11), Some(&Ok("[]".to_owned())));
}

#[test]
fn amends_in_place_and_moves_attached_brackets_on_the_parent() {
    let mut h = ready();
    h.command(Command::AmendOrder {
        command: 12,
        id: ORDER_ID.into(),
        price: Some(1650.004),
        amount: Some(0.2),
    });
    let amend = h.take("/v5/order/amend");
    assert_eq!(
        body(&amend),
        format!(
            r#"{{"category":"linear","symbol":"ETHUSDT","orderId":"{ORDER_ID}","qty":"0.2","price":"1650"}}"#
        )
    );
    // "Amend Order", the documented response.
    h.reply(
        amend.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"c6f055d9-7f21-4079-913d-e6523a9cfffa","orderLinkId":"linear-004"},"retExtInfo":{},"time":1672217093461}"#,
    );
    assert_eq!(
        h.result(12),
        Some(&Ok(r#"["c6f055d9-7f21-4079-913d-e6523a9cfffa"]"#.to_owned()))
    );

    h.command(Command::AmendOrder {
        command: 13,
        id: format!("{ORDER_ID}__stop_loss"),
        price: Some(1450.0),
        amount: None,
    });
    let amend = h.take("/v5/order/amend");
    assert_eq!(
        body(&amend),
        format!(
            r#"{{"category":"linear","symbol":"ETHUSDT","orderId":"{ORDER_ID}","stopLoss":"1450"}}"#
        )
    );
    h.reply(
        amend.id,
        200,
        r#"{"retCode":110025,"retMsg":"order not modified","result":{},"retExtInfo":{},"time":1}"#,
    );
    assert_eq!(
        h.result(13),
        Some(&Ok("[]".to_owned())),
        "not modified is a no-op"
    );
}

#[test]
fn cancels_in_batches_and_treats_an_order_already_gone_as_canceled() {
    let mut h = ready();
    h.message(
        "/v5/private",
        &order_frame("New", "0", "0.002", "EC_NoError"),
    );
    let other = "5cf98598-39a7-459e-97bf-76ca765ee020";
    h.command(Command::CancelOrders {
        command: 14,
        ids: vec![
            ORDER_ID.into(),
            format!("{ORDER_ID}__take_profit"),
            other.into(),
        ],
    });
    let cancel = h.take("/v5/order/cancel-batch");
    assert_eq!(
        body(&cancel),
        format!(
            r#"{{"category":"linear","request":[{{"symbol":"ETHUSDT","orderId":"{ORDER_ID}"}},{{"symbol":"BTCUSDT","orderId":"{other}"}}]}}"#
        )
    );
    // "Batch Cancel Order": one status per request.
    h.reply(
        cancel.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"category":"linear","symbol":"ETHUSDT","orderId":"fd4300ae-7847-404e-b947-b46980a4d140","orderLinkId":""},{"category":"linear","symbol":"BTCUSDT","orderId":"","orderLinkId":""}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"},{"code":110001,"msg":"order not exists or too late to cancel"}]},"time":1713434299047}"#,
    );
    let store = h.engine.store();
    assert_eq!(store.order(ORDER_ID).unwrap().status, OrderStatus::Canceled);
    assert_eq!(
        store
            .order(&format!("{ORDER_ID}__stop_loss"))
            .unwrap()
            .status,
        OrderStatus::Canceled
    );
    assert_eq!(store.order(other).unwrap().status, OrderStatus::Canceled);
    assert_eq!(h.result(14), Some(&Ok("[]".to_owned())));
}

#[test]
fn cancel_all_for_a_symbol() {
    let mut h = ready();
    h.command(Command::CancelSymbolOrders {
        command: 15,
        symbol: "ETHUSDT".into(),
    });
    let cancel = h.take("/v5/order/cancel-all");
    assert_eq!(body(&cancel), r#"{"category":"linear","symbol":"ETHUSDT"}"#);
    // "Cancel All Orders", the documented response.
    h.reply(
        cancel.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"orderId":"fd4300ae-7847-404e-b947-b46980a4d140","orderLinkId":"test-000005"}],"success":"1"},"retExtInfo":{},"time":1707381118116}"#,
    );
    assert!(
        h.engine
            .store()
            .orders()
            .iter()
            .all(|o| o.status == OrderStatus::Canceled)
    );
}

#[test]
fn leverage_margin_and_position_mode_update_the_settings_frame() {
    let mut h = ready();
    h.command(Command::SetLeverage {
        command: 16,
        symbol: "BTCUSDT".into(),
        leverage: 500.0,
    });
    let request = h.take("/v5/position/set-leverage");
    assert_eq!(
        body(&request),
        r#"{"category":"linear","symbol":"BTCUSDT","buyLeverage":"100","sellLeverage":"100"}"#,
        "clamped to the market"
    );
    h.reply(request.id, 200, OK_EMPTY);
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 100.0);
    assert_eq!(h.engine.store().positions()[0].leverage, 100.0);

    h.command(Command::SetLeverage {
        command: 17,
        symbol: "ETHUSDT".into(),
        leverage: 20.0,
    });
    assert!(h.requests.is_empty(), "already at 20x");
    assert_eq!(h.result(17), Some(&Ok("[]".to_owned())));

    h.command(Command::SetMarginType {
        command: 18,
        symbol: "BTCUSDT".into(),
        isolated: true,
    });
    let request = h.take("/v5/account/set-margin-mode");
    assert_eq!(body(&request), r#"{"setMarginMode":"ISOLATED_MARGIN"}"#);
    // "Set Margin Mode", the documented refusal.
    h.reply(
        request.id,
        200,
        r#"{"retCode":3400045,"retMsg":"Set margin mode failed","result":{"reasons":[{"reasonCode":"3400000","reasonMsg":"Equity needs to be equal to or greater than 1000 USDC"}]}}"#,
    );
    assert!(
        matches!(h.result(18), Some(Err(message)) if message == "Equity needs to be equal to or greater than 1000 USDC")
    );

    h.command(Command::SetPositionMode {
        command: 19,
        hedged: true,
    });
    let request = h.take("/v5/position/switch-mode");
    assert_eq!(
        body(&request),
        r#"{"category":"linear","coin":"USDT","mode":3}"#
    );
    h.reply(request.id, 200, OK_EMPTY);
    assert!(h.engine.store().hedged());
    assert!(
        h.has_request("/v5/position/list"),
        "positions re-read in the new mode"
    );
}

#[test]
fn queries_return_history_in_the_shared_shapes() {
    let mut h = ready();
    h.command(Command::Query {
        command: 21,
        query: Query::Fills {
            symbol: Some("ETHPERP".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    });
    let request = h.take("/v5/execution/list");
    assert!(
        request
            .url
            .ends_with("?category=linear&symbol=ETHPERP&limit=1")
    );
    // "Get Trade History", the documented response.
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"nextPageCursor":"132766%3A2%2C132766%3A2","category":"linear","list":[{"symbol":"ETHPERP","orderType":"Market","underlyingPrice":"","orderLinkId":"","side":"Buy","indexPrice":"","orderId":"8c065341-7b52-4ca9-ac2c-37e31ac55c94","stopOrderType":"UNKNOWN","leavesQty":"0","execTime":"1672282722429","feeCurrency":"","isMaker":false,"execFee":"0.071409","feeRate":"0.0006","execId":"e0cbe81d-0f18-5866-9415-cf319b5dab3b","tradeIv":"","blockTradeId":"","markPrice":"1183.54","execPrice":"1190.15","markIv":"","orderQty":"0.1","orderPrice":"1236.9","execValue":"119.015","execType":"Trade","execQty":"0.1","closedSize":"","extraFees":"","seq":4688002127}]},"retExtInfo":{},"time":1672283754510}"#,
    );
    assert_eq!(
        h.result(21),
        Some(&Ok(r#"[{"id":"e0cbe81d-0f18-5866-9415-cf319b5dab3b","timestamp":1672282722.0,"symbol":"ETHPERP","orderId":"8c065341-7b52-4ca9-ac2c-37e31ac55c94","price":1190.15,"qty":0.1,"side":"buy","commission":0.071409,"maker":false}]"#.to_owned()))
    );

    h.command(Command::Query {
        command: 22,
        query: Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(1672120000000.0),
            to_ms: Some(1672130000000.0),
            limit: None,
        },
    });
    let request = h.take("/v5/account/transaction-log");
    assert!(request.url.contains(
        "accountType=UNIFIED&category=linear&startTime=1672120000000&endTime=1672130000000&limit=50"
    ));
    // "Get Transaction Log", the documented response (first two rows).
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"nextPageCursor":"","list":[{"transSubType":"","id":"592324_XRPUSDT_161440249321","symbol":"XRPUSDT","side":"Buy","funding":"-0.003676","orderLinkId":"","orderId":"1672128000-8-592324-1-2","fee":"0.00000000","change":"-0.003676","cashFlow":"0","transactionTime":"1672128000000","type":"SETTLEMENT","feeRate":"0.0001","bonusChange":"","size":"100","qty":"100","cashBalance":"5086.55825002","currency":"USDT","category":"linear","tradePrice":"0.3676","tradeId":"534c0003-4bf7-486f-aa02-78cee36825e4","extraFees":""},{"transSubType":"","id":"592324_XRPUSDT_161440249321","symbol":"XRPUSDT","side":"Buy","funding":"","orderLinkId":"linear-order","orderId":"592b7e41-78fd-42e2-9aa3-91e1835ef3e1","fee":"0.01908720","change":"-0.0190872","cashFlow":"0","transactionTime":"1672121182224","type":"TRADE","feeRate":"0.0006","bonusChange":"-0.1430544","size":"100","qty":"88","cashBalance":"5086.56192602","currency":"USDT","category":"linear","tradePrice":"0.3615","tradeId":"5184f079-88ec-54c7-8774-5173cafd2b4e","extraFees":""}]},"retExtInfo":{},"time":1672132481405}"#,
    );
    let result = h.result(22).unwrap().as_ref().unwrap();
    assert!(result.contains(r#"{"symbol":"XRPUSDT","incomeType":"FUNDING_FEE","income":-0.003676,"asset":"USDT","timestamp":1672128000000.0,"tranId":"534c0003-4bf7-486f-aa02-78cee36825e4-XRPUSDT-SETTLEMENT-1672128000000:funding""#), "{result}");
    assert!(result.contains(r#""incomeType":"COMMISSION","income":-0.0190872"#));

    h.command(Command::Query {
        command: 23,
        query: Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(1),
        },
    });
    let request = h.take("/v5/market/recent-trade");
    assert!(request.headers.is_empty(), "public reads are not signed");
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"category":"spot","list":[{"execId":"2100000000007764263","symbol":"BTCUSDT","price":"16618.49","size":"0.00012","side":"Buy","time":"1672052955758","isBlockTrade":false,"isRPITrade":true,"seq":"123456"}]},"retExtInfo":{},"time":1672053054358}"#,
    );
    assert!(h.result(23).unwrap().as_ref().unwrap().starts_with(
        r#"[{"id":"2100000000007764263","symbol":"BTCUSDT","price":16618.49,"qty":0.00012"#
    ));
}

#[test]
fn ambiguous_batch_responses_never_resend_creates() {
    for (status, response) in [
        (502, "upstream connection reset"),
        (500, r#"{"retCode":10016,"retMsg":"Server error"}"#),
        (200, "not json"),
        (200, r#"{"retCode":10016,"retMsg":"Server error"}"#),
    ] {
        let mut h = ready();
        place(
            &mut h,
            50,
            vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
        );
        let batch = h.take("/v5/order/create-batch");
        h.reply(batch.id, status, response);
        assert!(
            h.requests.is_empty(),
            "an accepted batch may be hidden: {status} {response}"
        );
        assert!(matches!(h.result(50), Some(Err(_))));
    }
}

#[test]
fn a_split_preserves_quantity_and_only_the_first_lot_has_brackets() {
    #[derive(serde::Deserialize)]
    struct Batch {
        request: Vec<Item>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Item {
        qty: String,
        stop_loss: Option<String>,
    }
    let mut h = ready();
    let mut markets = h.engine.store().markets().to_vec();
    markets[0].limits.amount.max = 10.0;
    markets[0].precision.amount = 1.0;
    h.engine.store_mut().set_markets(markets);
    let mut order = limit("BTCUSDT", OrderSide::Buy, 25.0, 90000.0);
    order.stop_loss = Some(85000.0);
    place(&mut h, 51, vec![order]);
    let request = h.take("/v5/order/create-batch");
    let batch: Batch = serde_json::from_str(body(&request)).unwrap();
    assert_eq!(
        batch
            .request
            .iter()
            .map(|row| row.qty.as_str())
            .collect::<Vec<_>>(),
        ["10", "10", "5"]
    );
    let total: f64 = batch
        .request
        .iter()
        .map(|row| row.qty.parse::<f64>().unwrap())
        .sum();
    assert_eq!(total, 25.0);
    assert_eq!(batch.request[0].stop_loss.as_deref(), Some("85000"));
    assert!(batch.request[1..].iter().all(|row| row.stop_loss.is_none()));
}

#[test]
fn partial_stops_amend_the_native_order_and_full_stops_modify_the_position() {
    for mode in ["Partial", "Full"] {
        let mut h = ready();
        let stop = format!(
            r#"{{"topic":"order","data":[{{"category":"linear","symbol":"BTCUSDT","orderId":"position-stop","side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"StopLoss","tpslMode":"{mode}","qty":"0.005","triggerPrice":"86000","triggerBy":"MarkPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0}}]}}"#
        );
        h.message("/v5/private", &stop);
        h.command(Command::AmendOrder {
            command: 52,
            id: "position-stop".into(),
            price: Some(87000.0),
            amount: None,
        });
        if mode == "Partial" {
            let amend = h.take("/v5/order/amend");
            assert_eq!(
                body(&amend),
                r#"{"category":"linear","symbol":"BTCUSDT","orderId":"position-stop","triggerPrice":"87000"}"#
            );
            assert!(
                !h.has_request("/v5/position/trading-stop"),
                "Partial trading-stop adds another order"
            );
        } else {
            let amend = h.take("/v5/position/trading-stop");
            assert_eq!(
                body(&amend),
                r#"{"category":"linear","symbol":"BTCUSDT","positionIdx":0,"tpslMode":"Full","stopLoss":"87000","slTriggerBy":"MarkPrice"}"#
            );
            assert!(!h.has_request("/v5/order/amend"));
        }
    }
}

#[test]
fn unknown_symbol_modes_are_read_before_any_batch_write() {
    let mut h = Harness::started(credentials());
    respond_startup(&mut h);
    // No position snapshots have answered; a settle-coin read would omit flat rows.
    h.requests.clear();
    place(
        &mut h,
        53,
        vec![
            limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0),
            limit("ETHUSDT", OrderSide::Sell, 1.0, 3000.0),
        ],
    );
    let btc = h.take("/v5/position/list?category=linear&symbol=BTCUSDT");
    assert!(!h.has_request("/v5/order/create-batch"));
    h.reply(
        btc.id,
        200,
        r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0,"size":"0"}]}}"#,
    );
    let eth = h.take("/v5/position/list?category=linear&symbol=ETHUSDT");
    assert!(!h.has_request("/v5/order/create-batch"));
    h.reply(eth.id, 200, r#"{"retCode":0,"result":{"list":[{"symbol":"ETHUSDT","positionIdx":1,"size":"0"},{"symbol":"ETHUSDT","positionIdx":2,"size":"0"}]}}"#);
    let batch = h.take("/v5/order/create-batch");
    assert!(body(&batch).contains(r#""positionIdx":0}"#));
    assert!(body(&batch).contains(r#""positionIdx":2}"#));
}

#[test]
fn a_missing_or_failed_mode_read_cannot_guess_and_place() {
    for (status, response) in [(503, "unavailable"), (200, EMPTY_LIST)] {
        let mut h = Harness::started(credentials());
        respond_startup(&mut h);
        h.requests.clear();
        place(
            &mut h,
            54,
            vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
        );
        let mode = h.take("/v5/position/list?category=linear&symbol=BTCUSDT");
        h.reply(mode.id, status, response);
        assert!(h.requests.is_empty());
        assert!(matches!(h.result(54), Some(Err(_))));
    }
}

#[test]
fn a_mode_switch_fences_old_position_snapshots_and_queues_a_refresh() {
    let mut h = ready();
    h.advance(5_000.0);
    let usdt = h.take("/v5/position/list?category=linear&settleCoin=USDT");
    let usdc = h.take("/v5/position/list?category=linear&settleCoin=USDC");
    h.command(Command::SetPositionMode {
        command: 55,
        hedged: true,
    });
    h.respond("/v5/position/switch-mode", 200, OK_EMPTY);
    h.reply(usdt.id, 200, POSITIONS);
    h.reply(usdc.id, 200, EMPTY_LIST);
    assert!(
        h.has_request("/v5/position/list?category=linear&settleCoin=USDT"),
        "fresh snapshot follows the old response"
    );
    assert!(h.has_request("/v5/position/list?category=linear&settleCoin=USDC"));
    place(
        &mut h,
        56,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    assert!(
        !h.has_request("/v5/order/create-batch"),
        "old snapshot cannot restore a known one-way mode"
    );
    let mode = h.take("/v5/position/list?category=linear&symbol=BTCUSDT");
    h.reply(mode.id, 200, r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":1,"size":"0"},{"symbol":"BTCUSDT","positionIdx":2,"size":"0"}]}}"#);
    let batch = h.take("/v5/order/create-batch");
    assert!(body(&batch).contains(r#""positionIdx":1}"#));
}
