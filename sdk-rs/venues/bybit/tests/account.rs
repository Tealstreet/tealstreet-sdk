//! Private data and trading against Bybit's documented payloads.
mod support;

use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::model::{
    MarginType, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, PositionSide, RejectReason,
};
use safe_cex_engine::{Command, Event, HttpRequest, Method, Query};
use support::Harness;
use support::Options;

/// "Get Open & Closed Orders", one-way: a limit buy with attached TP/SL.
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 5_000.0;

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
    ready_options(credentials())
}
fn ready_options(options: Options) -> Harness {
    let mut h = Harness::started(options);
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
        stp_mode: None,
        implied_volatility: None,
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
    h.message(
        "/v5/private",
        &execution_frame("0.001", "97001").replace(
            "0ab1bdf7-4219-438b-b30a-32ec863018f7",
            "0ab1bdf7-4219-438b-b30a-32ec863018f8",
        ),
    );
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
    // D4: a partial batch fails KnownPartial with its proven native IDs.
    let error = h.notices.iter().find_map(|notice| match notice {
        safe_cex_engine::Notice::CommandFailed { command: 7, error } => Some(error),
        _ => None,
    });
    let error = error.expect("partial batch failure");
    assert_eq!(
        error.partial.as_ref().map(|p| p.order_ids.clone()),
        Some(vec!["1666800494330512128".to_owned()])
    );
    assert_ne!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
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
    assert!(matches!(h.result(9), Some(Err(message)) if message == "connection reset"));
    // The venue may have the orders: reconcile once, never resend.
    h.reconcile_once(EMPTY_LIST);
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
        // The documented shape, echoing this amend's target (strict ACK).
        &format!(
            r#"{{"retCode":0,"retMsg":"OK","result":{{"orderId":"{ORDER_ID}","orderLinkId":"linear-004"}},"retExtInfo":{{}},"time":1672217093461}}"#
        ),
    );
    assert_eq!(h.result(12), Some(&Ok(format!(r#"["{ORDER_ID}"]"#))));

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
    // D2: an absent-order row is no local Cancelled transition; the one
    // reconcile order read decides.
    assert_eq!(store.order(other).unwrap().status, OrderStatus::Open);
    assert_eq!(h.result(14), Some(&Ok("[]".to_owned())));
    assert!(h.has_request("/v5/order/realtime"), "one reconcile read");
    while h.has_request("/v5/order/realtime") {
        h.respond("/v5/order/realtime", 200, EMPTY_LIST);
    }
    assert!(h.engine.store().order(other).is_none());
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
        side: None,
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
        side: None,
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
        symbol: None,
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
    // Public reads retain the TS factory broker header without credentials.
    assert_eq!(request.headers, vec![("X-Referer", "Tealstreet".into())]);
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
        assert!(matches!(h.result(50), Some(Err(_))));
        // An accepted batch may be hidden: reconcile once, never resend.
        h.reconcile_once(EMPTY_LIST);
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
fn partial_stops_cancel_recreate_and_full_stops_keep_the_position_update_path() {
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
            h.respond(
                "/v5/order/cancel-batch",
                200,
                r#"{"retCode":0,"retMsg":"OK","retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
            );
            let amend = h.take("/v5/position/trading-stop");
            assert_eq!(
                body(&amend),
                r#"{"category":"linear","symbol":"BTCUSDT","positionIdx":0,"tpslMode":"Partial","takeProfit":"87000","tpTriggerBy":"LastPrice","tpSize":"0.005"}"#
            );
            assert!(!h.has_request("/v5/order/amend"));
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
        symbol: None,
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

const LOG_SECRET: &str = "audit-origin-secret-value";
const SIGNED_URL: &str = "https://api.bybit.com/v5/order/create?api_key=audit-origin-secret-value&signature=audit-origin-signature";
fn log_safe_engine() -> Harness {
    ready_options(Options {
        key: Some(LOG_SECRET.into()),
        secret: Some(LOG_SECRET.into()),
        ..Options::default()
    })
}
fn assert_private_notices_are_safe(h: &Harness) {
    for notice in &h.notices {
        // Debug covers every notice field, including structured venue errors.
        let text = format!("{notice:?}");
        assert!(
            !text.contains(LOG_SECRET),
            "credential escaped in {notice:?}"
        );
        assert!(
            !text.contains(SIGNED_URL),
            "signed URL escaped in {notice:?}"
        );
        assert!(
            !text.contains("audit-origin-signature"),
            "signature escaped in {notice:?}"
        );
        assert!(
            !text.contains("origin_string["),
            "signed origin escaped in {notice:?}"
        );
    }
}
fn rejected_signed_origin(code: i64) -> String {
    serde_json::json!({"retCode": code, "retMsg": format!("error sign! origin_string[1000{LOG_SECRET}20000category=linear] signed_url[{SIGNED_URL}]"), "result":{}, "retExtInfo":{"debug": LOG_SECRET}}).to_string()
}
#[test]
fn signed_origin_never_escapes_on_retry_or_terminal_command_rejection() {
    let mut h = log_safe_engine();
    place(
        &mut h,
        9001,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    let request = h.take("/v5/order/create-batch");
    h.reply(request.id, 200, &rejected_signed_origin(10004));
    assert_private_notices_are_safe(&h);
    let retry = h.take("/v5/order/create-batch");
    h.reply(retry.id, 200, &rejected_signed_origin(10004));
    assert!(
        !h.has_request("/v5/order/create-batch"),
        "signature rejection retries exactly once"
    );
    assert!(h.result(9001).unwrap().is_err());
    assert!(h.logs.iter().any(|log| log.contains("retCode 10004")));
    assert_private_notices_are_safe(&h);
}
#[test]
fn signed_origin_in_private_read_retry_exhaustion_and_later_transport_failure_is_safe() {
    for transport_failure in [false, true] {
        let mut h = log_safe_engine();
        h.advance(5000.0);
        let request = h.take("/v5/account/wallet-balance");
        h.reply(request.id, 200, &rejected_signed_origin(10004));
        let retry = h.take("/v5/account/wallet-balance");
        if transport_failure {
            h.event(Event::HttpFailed {
                request: retry.id,
                error: SIGNED_URL,
            });
        } else {
            h.reply(retry.id, 503, SIGNED_URL);
        }
        assert!(!h.has_request("/v5/account/wallet-balance"));
        assert!(
            h.logs
                .iter()
                .any(|log| log.contains("failed") || log.contains("HTTP 503"))
        );
        assert_private_notices_are_safe(&h);
    }
}
#[test]
fn receive_window_rejection_drops_echoed_credentials_but_keeps_clock_correction() {
    let mut h = log_safe_engine();
    h.advance(5000.0);
    let response = serde_json::json!({"retCode":10002,"retMsg":format!("recv_window req_timestamp[6000],server_timestamp[8000], origin_string[{LOG_SECRET}] {SIGNED_URL}"),"result":{}}).to_string();
    let request = h.take("/v5/account/wallet-balance");
    h.reply(request.id, 200, &response);
    assert_private_notices_are_safe(&h);
    let retry = h.take("/v5/account/wallet-balance");
    assert_eq!(
        Harness::header(&retry, "X-BAPI-TIMESTAMP").as_deref(),
        Some("8000")
    );
    h.reply(retry.id, 200, &response);
    assert!(h.logs.iter().any(|log| log.contains("retCode 10002")));
    assert_private_notices_are_safe(&h);
}
#[test]
fn normal_business_rejections_remain_actionable_after_a_signature_retry() {
    let mut h = log_safe_engine();
    place(
        &mut h,
        9002,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    let request = h.take("/v5/order/create-batch");
    h.reply(request.id, 200, &rejected_signed_origin(10004));
    let retry = h.take("/v5/order/create-batch");
    h.reply(
        retry.id,
        200,
        r#"{"retCode":110007,"retMsg":"Available balance is insufficient","result":{}}"#,
    );
    let error = h.result(9002).unwrap().as_ref().unwrap_err();
    assert!(error.contains("Available balance is insufficient"));
    assert!(!h.has_request("/v5/order/create-batch"));
    assert_private_notices_are_safe(&h);
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
    // Account reads are managed: the fixture driver prepares and signs them.
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
    let (now, mut reads) = policy_poll_read(engine, now, "/v5/position/list");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = if r.url.contains("wallet-balance") {
                WALLET
            } else {
                EMPTY_LIST
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
fn query_result(
    h: &mut Harness,
    command: u32,
    query: Query,
    path: &str,
    bodies: &[&str],
) -> String {
    h.command(Command::Query { command, query });
    for body in bodies {
        let request = h.take(path);
        h.reply(request.id, 200, body);
    }
    h.result(command).unwrap().clone().unwrap()
}

#[test]
fn rest_fills_exclude_funding_settlement_and_invalid_rows() {
    let mut h = ready();
    let row = |id: &str, kind: &str, side: &str, price: &str, qty: &str, time: &str| {
        format!(
            r#"{{"symbol":"BTCUSDT","orderId":"o-{id}","execId":"{id}","side":"{side}","execPrice":"{price}","execQty":"{qty}","execType":"{kind}","execFee":"0.1","execTime":"{time}"}}"#
        )
    };
    let rows = [
        row("t1", "Trade", "Buy", "100", "1", "1672282722429"),
        row("t2", "AdlTrade", "Sell", "101", "2", "1672282722430"),
        row("t3", "BustTrade", "Buy", "102", "3", "1672282722431"),
        row("f1", "Funding", "Buy", "100", "1", "1672282722432"),
        row("f2", " settle ", "Buy", "100", "1", "1672282722433"),
        row("f3", "SETTLEMENT", "Buy", "100", "1", "1672282722434"),
        row("f4", "Delivery", "Buy", "100", "1", "1672282722435"),
        row("f5", "SessionSettlePnL", "Buy", "100", "1", "1672282722436"),
        row("b1", "Trade", "", "100", "1", "1672282722437"),
        row("b2", "Trade", "Buy", "0", "1", "1672282722438"),
        row("b3", "Trade", "Buy", "100", "", "1672282722439"),
        row("b4", "Trade", "Buy", "100", "1", ""),
    ];
    let body = format!(
        r#"{{"retCode":0,"retMsg":"OK","result":{{"nextPageCursor":"","category":"linear","list":[{}]}}}}"#,
        rows.join(",")
    );
    let result = query_result(
        &mut h,
        31,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
        "/v5/execution/list",
        &[&body],
    );
    let mut ids: Vec<String> = serde_json::from_str::<Vec<serde_json::Value>>(&result)
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    assert_eq!(ids, ["t1", "t2", "t3"], "{result}");
}

#[test]
fn income_ids_match_ts_and_hedge_legs_survive_while_page_repeats_collapse() {
    let mut h = ready();
    let leg = |id: &str, side: &str, size: &str, funding: &str| {
        format!(
            r#"{{"id":"{id}","symbol":"BTCUSDT","side":"{side}","size":"{size}","funding":"{funding}","fee":"0","cashFlow":"0","transactionTime":"1672128000000","type":"SETTLEMENT","currency":"USDT","category":"linear"}}"#
        )
    };
    // Legacy rows reuse one id; sides and signed sizes tell the legs apart.
    let long = leg("592324_BTCUSDT_1", "Buy", "2", "-0.5");
    let short = leg("592324_BTCUSDT_1", "Sell", "-2", "-0.5");
    let trade = r#"{"id":"t-row","symbol":"DOGEUSDT","side":"Buy","size":"100","funding":"0","fee":"0.0055","cashFlow":"-0.42","transactionTime":"1672127000000","type":"TRADE","currency":"USDT","tradeId":"trade-1","orderId":"order-1","category":"linear"}"#;
    let page1 = format!(
        r#"{{"retCode":0,"retMsg":"OK","result":{{"nextPageCursor":"c2","list":[{long},{short},{trade}]}}}}"#
    );
    let page2 = format!(
        r#"{{"retCode":0,"retMsg":"OK","result":{{"nextPageCursor":"","list":[{short}]}}}}"#
    );
    let result = query_result(
        &mut h,
        32,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(1672120000000.0),
            to_ms: Some(1672130000000.0),
            limit: None,
        },
        "/v5/account/transaction-log",
        &[&page1, &page2],
    );
    let rows = serde_json::from_str::<Vec<serde_json::Value>>(&result).unwrap();
    let ids: Vec<(&str, f64)> = rows
        .iter()
        .map(|r| (r["tranId"].as_str().unwrap(), r["income"].as_f64().unwrap()))
        .collect();
    // `mapBybitTransactionToIncomeRecords` emits these exact ids for the same rows:
    // TS gives both hedge legs one id and keeps both.
    assert_eq!(
        ids,
        [
            ("BTCUSDT-SETTLEMENT-1672128000000:funding", -0.5),
            ("BTCUSDT-SETTLEMENT-1672128000000:funding", -0.5),
            ("trade-1-DOGEUSDT-TRADE-1672127000000:flow", -0.42),
            ("trade-1-DOGEUSDT-TRADE-1672127000000:fee", -0.0055),
        ],
        "{result}"
    );
}

#[test]
fn omitted_order_history_limit_reads_every_page_instead_of_fifty() {
    let mut h = ready();
    let page = |from: usize, cursor: &str| {
        let rows: Vec<String> = (from..from + 50)
            .map(|i| format!(r#"{{"orderId":"o{i}","symbol":"BTCUSDT","side":"Buy","orderType":"Limit","orderStatus":"Filled","qty":"1","cumExecQty":"1","price":"100","createdTime":"{}"}}"#, 1672000000000u64 + i as u64))
            .collect();
        format!(
            r#"{{"retCode":0,"retMsg":"OK","result":{{"nextPageCursor":"{cursor}","list":[{}]}}}}"#,
            rows.join(",")
        )
    };
    let (first, second) = (page(0, "next"), page(50, ""));
    h.command(Command::Query {
        command: 33,
        query: Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    });
    let request = h.take("/v5/order/history");
    assert!(request.url.contains("limit=50"), "{}", request.url);
    h.reply(request.id, 200, &first);
    let request = h.take("/v5/order/history");
    assert!(request.url.contains("cursor=next"));
    h.reply(request.id, 200, &second);
    let rows =
        serde_json::from_str::<Vec<serde_json::Value>>(h.result(33).unwrap().as_ref().unwrap())
            .unwrap();
    assert_eq!(rows.len(), 100);
    let explicit = query_result(
        &mut h,
        34,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(3),
        },
        "/v5/order/history",
        &[&page(0, "")],
    );
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(&explicit)
            .unwrap()
            .len(),
        3
    );
}

fn coins(h: &Harness) -> Vec<(String, f64, f64)> {
    let mut coins: Vec<_> = h
        .engine
        .store()
        .coin_balances()
        .unwrap_or_default()
        .iter()
        .map(|c| (c.base.clone(), c.amount, c.notional))
        .collect();
    coins.sort_by(|a, b| a.0.cmp(&b.0));
    coins
}

#[test]
fn per_coin_holdings_follow_rest_snapshots_and_merge_stream_pushes() {
    let mut h = ready();
    assert_eq!(coins(&h), [("USDT".to_owned(), 1010.5, 1010.5)]);
    let push = |coin: &str, equity: &str, wallet: &str, usd: &str| {
        format!(
            r#"{{"topic":"wallet","id":"w","creationTime":1,"data":[{{"accountType":"UNIFIED","totalEquity":"1010.5","totalWalletBalance":"1000.5","totalMarginBalance":"1010.5","totalAvailableBalance":"915.5","totalInitialMargin":"95","totalMaintenanceMargin":"5","coin":[{{"coin":"{coin}","equity":"{equity}","walletBalance":"{wallet}","usdValue":"{usd}"}}]}}]}}"#
        )
    };
    h.message("/v5/private", &push("BTC", "0.5", "0.4", "30000"));
    assert_eq!(
        coins(&h),
        [
            ("BTC".to_owned(), 0.5, 30000.0),
            ("USDT".to_owned(), 1010.5, 1010.5)
        ]
    );
    // Missing equity falls back to wallet balance, missing USD value to the amount.
    h.message("/v5/private", &push("ETH", "", "2", ""));
    assert!(coins(&h).contains(&("ETH".to_owned(), 2.0, 2.0)));
    h.message("/v5/private", &push("BTC", "0", "0", "0"));
    assert!(coins(&h).contains(&("BTC".to_owned(), 0.0, 0.0)));
    // A failed periodic REST read keeps holdings; a full one replaces them.
    h.requests.clear();
    h.advance(5_000.0);
    h.respond("/v5/account/wallet-balance", 500, "upstream");
    assert_eq!(coins(&h).len(), 3);
    h.requests.clear();
    h.advance(5_000.0);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    assert_eq!(coins(&h), [("USDT".to_owned(), 1010.5, 1010.5)]);
}

#[test]
fn stale_classic_metadata_retries_the_unified_wallet_once() {
    let mut h = Harness::started(credentials());
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond(
        "/v5/account/info",
        200,
        &support::ACCOUNT_INFO.replace("\"unifiedMarginStatus\":4", "\"unifiedMarginStatus\":1"),
    );
    h.load_public();
    let refused = r#"{"retCode":10001,"retMsg":"accountType only support UNIFIED.","result":{},"retExtInfo":{},"time":1}"#;
    h.respond(
        "/v5/account/wallet-balance?accountType=CONTRACT",
        200,
        refused,
    );
    let retry = h.take("/v5/account/wallet-balance");
    assert!(retry.url.ends_with("accountType=UNIFIED"), "{}", retry.url);
    h.reply(retry.id, 200, WALLET);
    assert_eq!(h.engine.store().balance().free, 915.5);
    // A later refusal on UNIFIED is reported, not retried in a loop.
    h.requests.clear();
    h.advance(5_000.0);
    h.respond(
        "/v5/account/wallet-balance?accountType=UNIFIED",
        200,
        refused,
    );
    assert!(!h.has_request("/v5/account/wallet-balance"));
    assert_eq!(h.engine.store().balance().free, 915.5);
}

#[test]
fn ordinary_wallet_errors_do_not_switch_account_type() {
    let mut h = Harness::started(credentials());
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond(
        "/v5/account/info",
        200,
        &support::ACCOUNT_INFO.replace("\"unifiedMarginStatus\":4", "\"unifiedMarginStatus\":1"),
    );
    h.load_public();
    h.respond(
        "/v5/account/wallet-balance?accountType=CONTRACT",
        200,
        r#"{"retCode":10006,"retMsg":"Too many visits!","result":{},"retExtInfo":{},"time":1}"#,
    );
    assert!(!h.has_request("/v5/account/wallet-balance"));
    h.requests.clear();
    h.advance(5_000.0);
    assert!(h.has_request("/v5/account/wallet-balance?accountType=CONTRACT"));
}

#[test]
fn an_unsupported_order_type_keeps_its_last_state_without_starving_other_orders() {
    let mut h = ready();
    assert!(h.engine.store().order(ORDER_ID).is_some());
    let row = |id: &str, kind: &str| {
        let list = OPEN_ORDERS.split("\"list\":[").nth(1).unwrap();
        let row = &list[..list.find("}],").unwrap() + 1];
        row.replace(ORDER_ID, id).replace(
            "\"orderType\":\"Limit\"",
            &format!("\"orderType\":\"{kind}\""),
        )
    };
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"subscribe"}"#,
    );
    let page = format!(
        r#"{{"retCode":0,"retMsg":"OK","result":{{"list":[{},{},{}],"nextPageCursor":"","category":"linear"}}}}"#,
        row(ORDER_ID, "FutureType"),
        row("new-1", "Limit"),
        row("unseen-2", "FutureType"),
    );
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDT",
        200,
        &page,
    );
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    assert!(
        h.engine.store().order("new-1").is_some(),
        "healthy order starved"
    );
    assert!(
        h.engine.store().order(ORDER_ID).is_some(),
        "known exposure erased"
    );
    assert!(h.engine.store().order("unseen-2").is_none());
    assert!(
        h.logs
            .iter()
            .any(|m| m.contains("unseen-2") && m.contains("not shown"))
    );
    assert!(
        h.logs
            .iter()
            .any(|m| m.contains(ORDER_ID) && m.contains("last known state"))
    );
}

#[test]
fn an_unidentified_order_row_holds_absence_while_valid_peers_commit() {
    let mut h = ready();
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"subscribe"}"#,
    );
    let list = OPEN_ORDERS.split("\"list\":[").nth(1).unwrap();
    let good = &list[..list.find("}],").unwrap() + 1];
    let bad = "{}";
    let page = format!(
        r#"{{"retCode":0,"retMsg":"OK","result":{{"list":[{},{}],"nextPageCursor":"","category":"linear"}}}}"#,
        good.replace(ORDER_ID, "new-1"),
        bad,
    );
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDT",
        200,
        &page,
    );
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    assert!(h.engine.store().order(ORDER_ID).is_some());
    assert!(h.engine.store().order("new-1").is_some());
}

#[test]
fn unbounded_order_history_keeps_rows_at_the_page_limit_like_ts() {
    let mut h = ready();
    h.notices.clear();
    h.engine.set_log_level(safe_cex_engine::Level::Error);
    h.command(Command::Query {
        command: 35,
        query: Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    });
    for page in 0..20 {
        let rows: Vec<String> = (page * 50..page * 50 + 50)
            .map(|i| format!(r#"{{"orderId":"o{i}","symbol":"BTCUSDT","side":"Buy","orderType":"Limit","orderStatus":"Cancelled","qty":"1","cumExecQty":"0","price":"100","createdTime":"{}"}}"#, 1672000000000u64 + i as u64))
            .collect();
        let body = format!(
            r#"{{"retCode":0,"retMsg":"OK","result":{{"nextPageCursor":"c{}","list":[{}]}}}}"#,
            page + 1,
            rows.join(",")
        );
        let request = h.take("/v5/order/history");
        h.reply(request.id, 200, &body);
    }
    assert!(!h.has_request("/v5/order/history"));
    let rows =
        serde_json::from_str::<Vec<serde_json::Value>>(h.result(35).unwrap().as_ref().unwrap())
            .unwrap();
    assert_eq!(rows.len(), 1000);
    assert!(h.logs.iter().any(|m| m.contains("page limit")));
    let (index, event) = h
        .notices
        .iter()
        .enumerate()
        .find_map(|(i, notice)| match notice {
            safe_cex_engine::Notice::Event(event)
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some((i, event))
            }
            _ => None,
        })
        .expect("owned semantic order-history warning");
    assert_eq!(event.level, safe_cex_engine::Level::Warn);
    assert_eq!(
        event.fields,
        vec![("command", safe_cex_engine::Value::Num(35.0))]
    );
    let done = h
        .notices
        .iter()
        .position(|notice| {
            matches!(
                notice,
                safe_cex_engine::Notice::CommandDone { command: 35, .. }
            )
        })
        .unwrap();
    assert!(index < done, "marker must precede retained rows");
}

/// Pins the exact request sequence of a windowed, cursor-paged read: 7-day
/// windows newest first, the first walked over two cursor pages. Paging
/// refactors keep it.
#[test]
fn windowed_fill_request_sequence_is_pinned() {
    let mut h = ready();
    let day = 86_400_000.0;
    let to = 1_672_000_000_000.0;
    h.command(Command::Query {
        command: 35,
        query: Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(to - 15.0 * day),
            to_ms: Some(to),
            limit: None,
        },
    });
    let page = |id: &str, cursor: &str| {
        format!(
            r#"{{"retCode":0,"retMsg":"OK","result":{{"nextPageCursor":"{cursor}","list":[{{"symbol":"BTCUSDT","execId":"{id}","orderId":"o","side":"Buy","execPrice":"100","execQty":"1","execFee":"0.1","execType":"Trade","execTime":"1671999000000","isMaker":false}}]}}}}"#
        )
    };
    let mut sent = Vec::new();
    for (id, cursor) in [("e1", "c1"), ("e2", ""), ("e3", ""), ("e4", "")] {
        let request = h.take("/v5/execution/list");
        sent.push(request.url.split_once("/v5/").unwrap().1.to_owned());
        h.reply(request.id, 200, &page(id, cursor));
    }
    assert!(h.result(35).unwrap().is_ok());
    assert_eq!(
        sent,
        [
            "execution/list?category=linear&symbol=BTCUSDT&startTime=1671395200001&endTime=1672000000000&limit=50",
            "execution/list?category=linear&symbol=BTCUSDT&startTime=1671395200001&endTime=1672000000000&limit=50&cursor=c1",
            "execution/list?category=linear&symbol=BTCUSDT&startTime=1670790400001&endTime=1671395200000&limit=50",
            "execution/list?category=linear&symbol=BTCUSDT&startTime=1670704000000&endTime=1670790400000&limit=50",
        ]
    );
}

#[test]
fn write_failures_never_claim_a_refusal_the_answer_does_not_prove() {
    use safe_cex_engine::Outcome;
    let unknown = Some(Outcome::Unknown);
    for (body, outcome) in [
        // A 2xx without a Bybit envelope proves nothing.
        ("<html>gateway</html>", unknown),
        // A per-item internal error may follow acceptance.
        (
            r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"orderId":""}]},"retExtInfo":{"list":[{"code":10016,"msg":"Internal System Error"}]}}"#,
            unknown,
        ),
        // An explicit per-item refusal with nothing accepted.
        (
            r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"orderId":""}]},"retExtInfo":{"list":[{"code":110007,"msg":"insufficient balance"}]}}"#,
            Some(Outcome::Rejected),
        ),
    ] {
        let mut h = ready();
        place(
            &mut h,
            71,
            vec![limit("BTCUSDT", OrderSide::Buy, 0.001, 90_000.0)],
        );
        let request = h.take("/v5/order/create-batch");
        h.reply(request.id, 200, body);
        assert_eq!(h.outcomes, [(71, outcome)], "{body}");
    }
    // One leg cancelled, the other refused: something took effect.
    let mut h = ready();
    h.message(
        "/v5/private",
        &order_frame("New", "0", "0.002", "EC_NoError"),
    );
    let other = "5cf98598-39a7-459e-97bf-76ca765ee020";
    h.command(Command::CancelOrders {
        command: 14,
        ids: vec![ORDER_ID.into(), other.into()],
    });
    let cancel = h.take("/v5/order/cancel-batch");
    h.reply(
        cancel.id,
        200,
        // Strict ACK: the accepted row echoes its requested target.
        &format!(
            r#"{{"retCode":0,"retMsg":"OK","result":{{"list":[{{"orderId":"{ORDER_ID}"}},{{"orderId":""}}]}},"retExtInfo":{{"list":[{{"code":0,"msg":"OK"}},{{"code":110020,"msg":"not allowed"}}]}}}}"#
        ),
    );
    assert_eq!(h.outcomes, [(14, None)]);
}

fn balance_info(h: &mut Harness, command: u32) -> serde_json::Value {
    h.command(Command::Venue {
        command,
        action: safe_cex_engine::VenueAction::FetchBalanceInfo,
    });
    serde_json::from_str(h.result(command).unwrap().as_ref().unwrap()).unwrap()
}

#[test]
fn balance_info_is_the_last_rest_wallet_account_verbatim_like_ts() {
    let mut h = Harness::started(credentials());
    assert_eq!(balance_info(&mut h, 90), serde_json::Value::Null);
    respond_startup(&mut h);
    respond_snapshot(&mut h);
    let expected: serde_json::Value =
        serde_json::from_str::<serde_json::Value>(WALLET).unwrap()["result"]["list"][0].clone();
    let info = balance_info(&mut h, 91);
    assert_eq!(info, expected);
    assert_eq!(info["accountType"], "UNIFIED");
    assert_eq!(info["coin"][0]["marginCollateral"], true);
    assert_eq!(info["coin"][0]["collateralSwitch"], true);

    // While the private topics are live, REST re-reads the wallet only as the 60 s
    // safety reconcile. Pushes stream the typed wallet; `info` is not merged from
    // them (the bybit wasm budget), so it stays the last REST account until then.
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "/v5/private",
        r#"{"topic":"wallet","id":"w","creationTime":1,"data":[{"accountType":"UNIFIED","totalEquity":"2000","totalWalletBalance":"2000","totalMarginBalance":"2000","totalAvailableBalance":"2000","totalInitialMargin":"0","totalMaintenanceMargin":"0","coin":[{"coin":"USDT","equity":"2000","walletBalance":"2000","usdValue":"2000"}]}]}"#,
    );
    assert_eq!(balance_info(&mut h, 92), expected);

    // A failed read keeps the last good payload.
    h.advance(60_000.0);
    assert!(h.has_request("/v5/account/wallet-balance"));
    while h.has_request("/v5/account/wallet-balance") {
        h.respond(
            "/v5/account/wallet-balance",
            200,
            r#"{"retCode":10002,"retMsg":"invalid request","result":{}}"#,
        );
    }
    assert_eq!(balance_info(&mut h, 93), expected);
}

fn snapshot_rows(body: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(body).unwrap()["result"]["list"]
        .as_array()
        .unwrap()
        .clone()
}
fn snapshot_page(rows: &[serde_json::Value]) -> String {
    serde_json::json!({"retCode":0,"result":{"list":rows,"nextPageCursor":"","category":"linear"}})
        .to_string()
}
fn refresh_orders(h: &mut Harness, rows: &[serde_json::Value]) {
    h.message("/v5/private", r#"{"success":true,"op":"subscribe"}"#);
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDT",
        200,
        &snapshot_page(rows),
    );
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
}
#[test]
fn malformed_identified_rest_order_keeps_parent_and_protection_while_peers_commit() {
    for structural in [false, true] {
        let mut h = ready();
        let mut bad = snapshot_rows(OPEN_ORDERS).remove(0);
        bad["qty"] = if structural {
            serde_json::json!({"bad":1})
        } else {
            serde_json::json!("NaN")
        };
        let mut good = snapshot_rows(OPEN_ORDERS).remove(0);
        good["orderId"] = serde_json::json!("healthy-peer");
        refresh_orders(&mut h, &[bad, good]);
        assert!(
            h.engine.store().order("healthy-peer").is_some(),
            "healthy order starved"
        );
        for id in [
            ORDER_ID.to_string(),
            format!("{ORDER_ID}__stop_loss"),
            format!("{ORDER_ID}__take_profit"),
        ] {
            assert!(
                h.engine.store().order(&id).is_some(),
                "rejected lineage {id} erased"
            );
        }
        assert!(h.notices.iter().any(|n| matches!(n, safe_cex_engine::Notice::Event(e) if e.code == safe_cex_engine::log::code::ROW_SKIPPED)));
        h.requests.clear();
        h.advance(15_000.0);
        assert!(
            h.has_request("/v5/order/realtime"),
            "bounded repair missing"
        );
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
        assert!(
            h.engine.store().order(ORDER_ID).is_none(),
            "clean omission must remove retained order"
        );
    }
}
#[test]
fn malformed_rest_position_keeps_exposure_while_other_symbols_commit() {
    for structural in [false, true] {
        let mut h = ready();
        h.advance(POLL_MS);
        let mut rows = snapshot_rows(POSITIONS);
        rows[0]["size"] = if structural {
            serde_json::json!({"bad":1})
        } else {
            serde_json::json!("NaN")
        };
        rows[1]["side"] = serde_json::json!("Sell");
        rows[1]["size"] = serde_json::json!("2");
        rows[1]["avgPrice"] = serde_json::json!("1600");
        h.respond(
            "/v5/position/list?category=linear&settleCoin=USDT",
            200,
            &snapshot_page(&rows),
        );
        h.respond(
            "/v5/position/list?category=linear&settleCoin=USDC",
            200,
            EMPTY_LIST,
        );
        assert_eq!(
            h.engine
                .store()
                .positions()
                .iter()
                .find(|p| p.symbol == "ETHUSDT")
                .unwrap()
                .contracts,
            2.0,
            "healthy position starved"
        );
        assert_eq!(
            h.engine
                .store()
                .positions()
                .iter()
                .find(|p| p.symbol == "BTCUSDT")
                .unwrap()
                .contracts,
            0.01,
            "rejected exposure erased"
        );
        assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 10.0);
    }
}
#[test]
fn structural_ws_order_rejection_keeps_cached_lineage_and_admits_healthy_peer() {
    let mut h = ready();
    let mut bad = snapshot_rows(OPEN_ORDERS).remove(0);
    bad["qty"] = serde_json::json!({"bad":1});
    let mut good = snapshot_rows(OPEN_ORDERS).remove(0);
    good["orderId"] = serde_json::json!("ws-healthy");
    h.message(
        "/v5/private",
        &serde_json::json!({"topic":"order","data":[bad,good]}).to_string(),
    );
    assert!(
        h.engine.store().order("ws-healthy").is_some(),
        "healthy WS order starved"
    );
    assert!(h.engine.store().order(ORDER_ID).is_some());
    assert!(
        h.engine
            .store()
            .order(&format!("{ORDER_ID}__stop_loss"))
            .is_some()
    );
    h.advance(15_000.0);
    assert!(
        h.has_request("/v5/order/realtime"),
        "WS reject must request bounded repair"
    );
}
#[test]
fn structural_ws_position_rejection_keeps_exposure_and_admits_healthy_peer() {
    let mut h = ready();
    let mut rows = snapshot_rows(POSITIONS);
    rows[0]["size"] = serde_json::json!({"bad":1});
    rows[1]["side"] = serde_json::json!("Sell");
    rows[1]["size"] = serde_json::json!("3");
    h.message(
        "/v5/private",
        &serde_json::json!({"topic":"position","data":rows}).to_string(),
    );
    assert_eq!(
        h.engine
            .store()
            .positions()
            .iter()
            .find(|p| p.symbol == "ETHUSDT")
            .unwrap()
            .contracts,
        3.0,
        "healthy WS position starved"
    );
    assert_eq!(
        h.engine
            .store()
            .positions()
            .iter()
            .find(|p| p.symbol == "BTCUSDT")
            .unwrap()
            .contracts,
        0.01
    );
    h.advance(POLL_MS);
    assert!(h.has_request("/v5/position/list"));
}

#[test]
fn unidentified_neighbor_cannot_restore_brackets_removed_by_an_admitted_parent() {
    let mut h = ready();
    let mut parent = snapshot_rows(OPEN_ORDERS).remove(0);
    parent["stopLoss"] = serde_json::json!("0");
    parent["takeProfit"] = serde_json::json!("0");
    refresh_orders(&mut h, &[serde_json::json!({}), parent]);
    assert!(h.engine.store().order(ORDER_ID).is_some());
    assert!(
        h.engine
            .store()
            .order(&format!("{ORDER_ID}__stop_loss"))
            .is_none()
    );
    assert!(
        h.engine
            .store()
            .order(&format!("{ORDER_ID}__take_profit"))
            .is_none()
    );
}
#[test]
fn bad_ws_order_during_clean_rest_join_coalesces_a_newer_repair() {
    let mut h = ready();
    h.message("/v5/private", r#"{"success":true,"op":"subscribe"}"#);
    let mut bad = snapshot_rows(OPEN_ORDERS).remove(0);
    bad["qty"] = serde_json::json!({"bad":1});
    for _ in 0..3 {
        h.message(
            "/v5/private",
            &serde_json::json!({"topic":"order","data":[bad]}).to_string(),
        );
    }
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
    assert_eq!(
        h.requests
            .iter()
            .filter(|r| r.url.contains("/v5/order/realtime"))
            .count(),
        2,
        "one fresh join, not one per bad event"
    );
    assert!(
        h.engine.store().order(ORDER_ID).is_some(),
        "older clean read cannot erase exposure after rejected stream evidence"
    );
}
#[test]
fn unidentified_position_neighbor_cannot_mix_new_oneway_and_cached_hedge_legs() {
    let mut h = coherent_hedge();
    let before = h
        .engine
        .store()
        .positions()
        .iter()
        .filter(|p| p.symbol == "BTCUSDT")
        .cloned()
        .collect::<Vec<_>>();
    h.advance(POLL_MS);
    let one_way = snapshot_rows(POSITIONS).remove(0);
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDT",
        200,
        &snapshot_page(&[serde_json::json!({}), one_way]),
    );
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    assert_eq!(
        h.engine
            .store()
            .positions()
            .iter()
            .filter(|p| p.symbol == "BTCUSDT")
            .cloned()
            .collect::<Vec<_>>(),
        before
    );
}
#[test]
fn rejected_duplicate_ws_order_cannot_be_replaced_by_its_valid_peer_in_either_order() {
    for reverse in [false, true] {
        let mut h = ready();
        let before = h.engine.store().order(ORDER_ID).unwrap().clone();
        let mut good = snapshot_rows(OPEN_ORDERS).remove(0);
        good["price"] = serde_json::json!("1700");
        let mut bad = good.clone();
        bad["qty"] = serde_json::json!({"bad":1});
        let rows = if reverse {
            vec![good, bad]
        } else {
            vec![bad, good]
        };
        h.message(
            "/v5/private",
            &serde_json::json!({"topic":"order","data":rows}).to_string(),
        );
        assert_eq!(h.engine.store().order(ORDER_ID).unwrap(), &before);
    }
}

#[test]
fn repeated_bad_ws_identity_does_not_starve_other_rest_orders() {
    let mut h = ready();
    h.message("/v5/private", r#"{"success":true,"op":"subscribe"}"#);
    for round in 0..2 {
        let mut bad = snapshot_rows(OPEN_ORDERS).remove(0);
        bad["qty"] = serde_json::json!({"bad":1});
        h.message(
            "/v5/private",
            &serde_json::json!({"topic":"order","data":[bad]}).to_string(),
        );
        let mut good = snapshot_rows(OPEN_ORDERS).remove(0);
        good["orderId"] = serde_json::json!("rest-only");
        good["price"] = serde_json::json!((1700 + round).to_string());
        h.respond(
            "/v5/order/realtime?category=linear&settleCoin=USDT",
            200,
            &snapshot_page(&[good]),
        );
        h.respond(
            "/v5/order/realtime?category=linear&settleCoin=USDC",
            200,
            EMPTY_LIST,
        );
        assert_eq!(
            h.engine
                .store()
                .order("rest-only")
                .expect("REST-only peer starved")
                .price,
            (1700 + round) as f64
        );
        assert!(
            h.engine.store().order(ORDER_ID).is_some(),
            "bad WS identity retained across older REST"
        );
        assert_eq!(
            h.requests
                .iter()
                .filter(|r| r.url.contains("/v5/order/realtime"))
                .count(),
            2
        );
    }
}
#[test]
fn repeated_bad_ws_symbol_does_not_starve_other_rest_positions() {
    let mut h = ready();
    for round in 1..=2 {
        h.advance(POLL_MS);
        let mut rows = snapshot_rows(POSITIONS);
        let mut bad = rows[0].clone();
        bad["size"] = serde_json::json!({"bad":1});
        h.message(
            "/v5/private",
            &serde_json::json!({"topic":"position","data":[bad]}).to_string(),
        );
        rows[0]["size"] = serde_json::json!("9");
        rows[1]["side"] = serde_json::json!("Buy");
        rows[1]["size"] = serde_json::json!(round.to_string());
        h.respond(
            "/v5/position/list?category=linear&settleCoin=USDT",
            200,
            &snapshot_page(&rows),
        );
        h.respond(
            "/v5/position/list?category=linear&settleCoin=USDC",
            200,
            EMPTY_LIST,
        );
        assert_eq!(
            h.engine
                .store()
                .positions()
                .iter()
                .find(|p| p.symbol == "ETHUSDT")
                .expect("REST-only position starved")
                .contracts,
            round as f64
        );
        assert_eq!(
            h.engine
                .store()
                .positions()
                .iter()
                .find(|p| p.symbol == "BTCUSDT")
                .unwrap()
                .contracts,
            0.01
        );
    }
}
fn coherent_hedge() -> Harness {
    let mut h = ready();
    h.advance(POLL_MS);
    let mut row = snapshot_rows(POSITIONS).remove(0);
    row["positionIdx"] = serde_json::json!(1);
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDT",
        200,
        &snapshot_page(&[row]),
    );
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    assert_eq!(
        h.engine
            .store()
            .positions()
            .iter()
            .filter(|p| p.symbol == "BTCUSDT")
            .count(),
        1
    );
    h
}
#[test]
fn unidentified_ws_neighbor_cannot_mix_oneway_and_coherent_cached_hedge() {
    let mut h = coherent_hedge();
    let before = h.engine.store().positions().to_vec();
    let one_way = snapshot_rows(POSITIONS).remove(0);
    h.message(
        "/v5/private",
        &serde_json::json!({"topic":"position","data":[{},one_way]}).to_string(),
    );
    assert_eq!(h.engine.store().positions(), before);
}

#[test]
fn leverage_ack_fences_older_position_reads_and_coalesces_fresh_metadata() {
    for ack in [
        OK_EMPTY,
        r#"{"retCode":110043,"retMsg":"leverage not modified","result":{}}"#,
    ] {
        let mut h = ready();
        h.advance(POLL_MS);
        h.command(Command::SetLeverage {
            command: 121,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
            side: None,
        });
        let write = h.take("/v5/position/set-leverage");
        h.reply(write.id, 200, ack);
        assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 20.0);
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
        assert_eq!(
            h.engine.store().setting("BTCUSDT").unwrap().leverage,
            20.0,
            "stale read overwrote acknowledged setting"
        );
        assert_eq!(
            h.engine
                .store()
                .positions()
                .iter()
                .find(|p| p.symbol == "BTCUSDT")
                .unwrap()
                .leverage,
            20.0,
            "stale read overwrote acknowledged position leverage"
        );
        assert_eq!(h.result(121), Some(&Ok("[]".to_owned())));
        assert_eq!(
            h.requests
                .iter()
                .filter(|r| r.url.contains("/v5/position/list"))
                .count(),
            2,
            "one fresh two-coin join after stale join settles"
        );
        h.respond(
            "/v5/position/list?category=linear&settleCoin=USDT",
            200,
            &POSITIONS.replace("\"leverage\":\"10\"", "\"leverage\":\"20\""),
        );
        h.respond(
            "/v5/position/list?category=linear&settleCoin=USDC",
            200,
            EMPTY_LIST,
        );
        assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 20.0);
        assert!(
            !h.has_request("/v5/position/set-leverage"),
            "read fence must never resend write"
        );
    }
}
#[test]
fn refused_leverage_does_not_invalidate_an_authoritative_position_read() {
    let mut h = ready();
    h.advance(POLL_MS);
    h.command(Command::SetLeverage {
        command: 122,
        symbol: "BTCUSDT".into(),
        leverage: 20.0,
        side: None,
    });
    let write = h.take("/v5/position/set-leverage");
    h.reply(
        write.id,
        200,
        r#"{"retCode":10001,"retMsg":"request parameter error","result":{}}"#,
    );
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDT",
        200,
        &POSITIONS.replace("\"leverage\":\"10\"", "\"leverage\":\"15\""),
    );
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 15.0);
    assert!(h.result(122).unwrap().is_err());
    assert!(!h.has_request("/v5/position/list"));
}

fn owned_history_query(kind: usize, limit: Option<u32>) -> (Query, &'static str) {
    match kind {
        0 => (
            Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit,
            },
            "/v5/execution/list",
        ),
        1 => (
            Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: None,
                to_ms: None,
                limit,
            },
            "/v5/order/history",
        ),
        2 => (
            Query::Income {
                symbol: Some("BTCUSDT".into()),
                kind: None,
                from_ms: None,
                to_ms: None,
                limit,
            },
            "/v5/account/transaction-log",
        ),
        _ => (
            Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit,
            },
            "/v5/market/recent-trade",
        ),
    }
}

fn owned_history_row(kind: usize, id: &str) -> String {
    match kind {
        0 => format!(
            r#"{{"symbol":"BTCUSDT","execId":"{id}","orderId":"o","side":"Buy","execPrice":"100","execQty":"1","execTime":"1671999000000"}}"#
        ),
        1 => format!(
            r#"{{"symbol":"BTCUSDT","orderId":"{id}","side":"Buy","orderStatus":"Cancelled","createdTime":"1671999000000","qty":"1"}}"#
        ),
        2 => format!(
            r#"{{"id":"{id}","symbol":"BTCUSDT","side":"Buy","size":"2","funding":"-0.5","fee":"0","cashFlow":"0","transactionTime":"1672128000000","type":"SETTLEMENT","currency":"USDT"}}"#
        ),
        _ => format!(
            r#"{{"execId":"{id}","price":"100","size":"1","time":"1671999000000","side":"Buy"}}"#
        ),
    }
}

fn owned_history_page(rows: &[String], cursor: &str) -> String {
    format!(
        r#"{{"retCode":0,"result":{{"list":[{}],"nextPageCursor":"{cursor}"}}}}"#,
        rows.join(",")
    )
}

fn owned_history_markers(h: &Harness, command: u32) -> Vec<usize> {
    h.notices
        .iter()
        .enumerate()
        .filter_map(|(index, notice)| match notice {
            safe_cex_engine::Notice::Event(event)
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                assert_eq!(event.level, safe_cex_engine::Level::Warn);
                assert_eq!(
                    event.fields,
                    [("command", safe_cex_engine::Value::Num(f64::from(command)))]
                );
                Some(index)
            }
            _ => None,
        })
        .collect()
}

fn owned_history_failure(
    h: &mut Harness,
    request: safe_cex_engine::RequestId,
    failure: usize,
    kind: usize,
) {
    match failure {
        0 => h.reply(request, 200, r#"{"retCode":10001,"retMsg":"bad request"}"#),
        1 => h.reply(request, 503, "unavailable"),
        2 => h.event(Event::HttpFailed {
            request,
            error: "connection lost",
        }),
        3 => h.reply(request, 200, r#"{"retCode":0}"#),
        4 => h.reply(request, 200, r#"{"retCode":0,"result":{}}"#),
        5 => h.reply(request, 200, r#"{"retCode":0,"result":{"list":null}}"#),
        // A valid row before a malformed row must not leak from a failed page.
        _ => h.reply(
            request,
            200,
            &owned_history_page(
                &[owned_history_row(kind, "must-not-leak"), "false".into()],
                "next",
            ),
        ),
    }
}

#[test]
fn owned_history_first_page_failure_is_typed_and_never_empty_success() {
    for kind in 0..4 {
        for failure in 0..7 {
            let mut h = ready();
            h.notices.clear();
            let (query, path) = owned_history_query(kind, None);
            h.command(Command::Query {
                command: 801,
                query,
            });
            let request = h.take(path);
            owned_history_failure(&mut h, request.id, failure, kind);
            assert!(
                h.result(801).unwrap().is_err(),
                "kind {kind}, failure {failure}"
            );
            let error = h
                .notices
                .iter()
                .find_map(|notice| match notice {
                    safe_cex_engine::Notice::CommandFailed {
                        command: 801,
                        error,
                    } => Some(error),
                    _ => None,
                })
                .unwrap();
            assert!(
                error.kind.is_some(),
                "typed: kind {kind}, failure {failure}"
            );
            // A native envelope refusal keeps its retCode; venue_error's numeric
            // fallback is an HTTP status, and no venueCode is fabricated from it.
            let native = (failure == 0).then(|| "10001".to_owned());
            assert_eq!(error.venue_code, native, "kind {kind}, failure {failure}");
            if failure == 2 {
                assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Network));
            }
            assert_eq!(h.done.iter().filter(|(id, _)| *id == 801).count(), 1);
            assert!(owned_history_markers(&h, 801).is_empty());
            assert!(!h.has_request(path));
        }
    }
}

#[test]
fn owned_history_later_failure_retains_only_accepted_rows_with_owned_marker() {
    for kind in 0..3 {
        for failure in 0..7 {
            let mut h = ready();
            h.notices.clear();
            h.engine.set_log_level(safe_cex_engine::Level::Error);
            let (query, path) = owned_history_query(kind, None);
            h.command(Command::Query {
                command: 802,
                query,
            });
            h.respond(
                path,
                200,
                &owned_history_page(&[owned_history_row(kind, "accepted")], "page2"),
            );
            let request = h.take(path);
            assert!(request.url.ends_with("&cursor=page2"));
            owned_history_failure(&mut h, request.id, failure, kind);
            let result = h.result(802).unwrap().as_ref().unwrap();
            let rows: Vec<serde_json::Value> = serde_json::from_str(result).unwrap();
            assert_eq!(rows.len(), 1, "kind {kind}, failure {failure}: {result}");
            assert!(!result.contains("must-not-leak"));
            if kind != 2 {
                assert!(result.contains("accepted"));
            }
            let markers = owned_history_markers(&h, 802);
            assert_eq!(markers.len(), 1);
            assert!(matches!(
                h.notices.get(markers[0] + 1),
                Some(safe_cex_engine::Notice::CommandDone { command: 802, .. })
            ));
            assert_eq!(h.done.iter().filter(|(id, _)| *id == 802).count(), 1);
            assert!(!h.has_request(path));
        }
    }
}

#[test]
fn owned_history_empty_or_filtered_pages_do_not_qualify_for_failure_recovery() {
    for kind in 0..2 {
        for filtered in [false, true] {
            let mut h = ready();
            h.notices.clear();
            let (query, path) = owned_history_query(kind, None);
            h.command(Command::Query {
                command: 803,
                query,
            });
            let rows = if filtered {
                vec![owned_history_row(kind, "filtered").replace("\"Buy\"", "\"invalid\"")]
            } else {
                vec![]
            };
            h.respond(path, 200, &owned_history_page(&rows, "page2"));
            let request = h.take(path);
            owned_history_failure(&mut h, request.id, 2, kind);
            assert!(h.result(803).unwrap().is_err());
            assert!(owned_history_markers(&h, 803).is_empty());
        }
    }
    // Income's empty native list proves exhaustion, even with a bogus cursor.
    let mut h = ready();
    let (query, path) = owned_history_query(2, None);
    h.command(Command::Query {
        command: 803,
        query,
    });
    h.respond(path, 200, &owned_history_page(&[], "ignored"));
    assert_eq!(h.result(803).unwrap().as_ref().unwrap(), "[]");
    assert!(!h.has_request(path));
}

#[test]
fn owned_history_cursor_and_bounded_budget_refusals_stay_strict_after_rows() {
    for kind in 0..3 {
        for mode in 0..3 {
            let mut h = ready();
            h.notices.clear();
            let (query, path) = owned_history_query(kind, Some(1000));
            h.command(Command::Query {
                command: 804,
                query,
            });
            let budget = if kind == 1 { 20 } else { 10 };
            let cursors: Vec<String> = match mode {
                0 => vec!["same".into(), "same".into()],
                1 => vec!["a".into(), "b".into(), "a".into()],
                _ => (0..budget).map(|n| format!("c{n}")).collect(),
            };
            for (index, cursor) in cursors.iter().enumerate() {
                h.respond(
                    path,
                    200,
                    &owned_history_page(&[owned_history_row(kind, &index.to_string())], cursor),
                );
            }
            assert!(h.result(804).unwrap().is_err(), "kind {kind}, mode {mode}");
            assert!(owned_history_markers(&h, 804).is_empty());
            assert!(!h.has_request(path));
        }
    }
}

#[test]
fn owned_history_raw_duplicate_limit_wins_over_cursor_refusal() {
    for kind in 0..3 {
        let mut h = ready();
        h.notices.clear();
        let (query, path) = owned_history_query(kind, Some(2));
        h.command(Command::Query {
            command: 805,
            query,
        });
        for _ in 0..2 {
            h.respond(
                path,
                200,
                &owned_history_page(&[owned_history_row(kind, "duplicate")], "same"),
            );
        }
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(h.result(805).unwrap().as_ref().unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(!h.has_request(path));
        assert!(owned_history_markers(&h, 805).is_empty());
    }
}

#[test]
fn owned_history_recovered_income_keeps_hedge_legs_and_dedupes_page_repeats() {
    let mut h = ready();
    h.notices.clear();
    let (query, path) = owned_history_query(2, None);
    h.command(Command::Query {
        command: 806,
        query,
    });
    let long = owned_history_row(2, "shared");
    let short = long
        .replace("\"Buy\"", "\"Sell\"")
        .replace("\"size\":\"2\"", "\"size\":\"-2\"");
    h.respond(
        path,
        200,
        &owned_history_page(&[long.clone(), short.clone()], "page2"),
    );
    h.respond(path, 200, &owned_history_page(&[long, short], "page3"));
    let request = h.take(path);
    owned_history_failure(&mut h, request.id, 6, 2);
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(h.result(806).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["tranId"], rows[1]["tranId"]);
    assert_eq!(rows[0]["income"], -0.5);
    assert_eq!(owned_history_markers(&h, 806).len(), 1);
}

#[test]
fn owned_history_window_recovery_preserves_first_incomplete_cause() {
    let mut h = ready();
    h.notices.clear();
    h.engine.set_log_level(safe_cex_engine::Level::Error);
    let day = 86_400_000.0;
    let to = 1_672_000_000_000.0;
    h.command(Command::Query {
        command: 807,
        query: Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(to - 8.0 * day),
            to_ms: Some(to),
            limit: None,
        },
    });
    for page in 0..20 {
        h.respond(
            "/v5/order/history",
            200,
            &owned_history_page(
                &[owned_history_row(1, &format!("o{page}"))],
                &format!("c{page}"),
            ),
        );
    }
    assert!(h.result(807).is_none());
    let request = h.take("/v5/order/history");
    assert!(!request.url.contains("cursor="));
    owned_history_failure(&mut h, request.id, 2, 1);
    let result = h.result(807).unwrap().as_ref().unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(result)
            .unwrap()
            .len(),
        20
    );
    let markers = owned_history_markers(&h, 807);
    assert_eq!(markers.len(), 1);
    let safe_cex_engine::Notice::Event(event) = &h.notices[markers[0]] else {
        panic!()
    };
    assert!(event.message.contains("page limit"));
    assert!(matches!(
        h.notices.get(markers[0] + 1),
        Some(safe_cex_engine::Notice::CommandDone { command: 807, .. })
    ));
}

#[test]
fn owned_history_missing_required_list_never_proves_empty_complete_coverage() {
    for kind in 0..4 {
        for body in [
            r#"{"retCode":0}"#,
            r#"{"retCode":0,"result":null}"#,
            r#"{"retCode":0,"result":{}}"#,
            r#"{"retCode":0,"result":{"list":null}}"#,
        ] {
            let mut h = ready();
            h.notices.clear();
            let (query, path) = owned_history_query(kind, None);
            h.command(Command::Query {
                command: 808,
                query,
            });
            h.respond(path, 200, body);
            assert!(h.result(808).unwrap().is_err(), "kind {kind}: {body}");
            assert!(owned_history_markers(&h, 808).is_empty());
            assert!(!h.has_request(path));
        }
    }
}

#[test]
fn unsupported_reports_wait_for_admitted_join_and_never_rearm_on_clean_rows() {
    let mut h = ready();
    let mut unsupported = snapshot_rows(OPEN_ORDERS).remove(0);
    unsupported["orderType"] = serde_json::json!("FutureType");
    let mut healthy = snapshot_rows(OPEN_ORDERS).remove(0);
    healthy["orderId"] = serde_json::json!("healthy-report-peer");
    let rows = [unsupported, healthy];
    let expected =
        format!("Bybit order {ORDER_ID} has an unsupported type; its last known state is kept");
    let reports = |h: &Harness| {
        h.logs
            .iter()
            .filter(|s| s.contains("unsupported type"))
            .cloned()
            .collect::<Vec<_>>()
    };
    h.message("/v5/private", r#"{"success":true,"op":"subscribe"}"#);
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDT",
        200,
        &snapshot_page(&rows),
    );
    assert!(
        reports(&h).is_empty(),
        "first settle-coin page cannot report yet"
    );
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDC",
        200,
        r#"{"retCode":0,"result":{}}"#,
    );
    assert!(
        reports(&h).is_empty(),
        "whole failed join cannot consume memory"
    );
    assert!(h.engine.store().order(ORDER_ID).is_some());
    for clean in [false, true, false] {
        h.advance(15_000.0);
        let page = if clean {
            OPEN_ORDERS.replace(
                "page_args%3Dfd4300ae-7847-404e-b947-b46980a4d140%26symbol%3D6%26",
                "",
            )
        } else {
            snapshot_page(&rows)
        };
        let previous = reports(&h);
        h.respond(
            "/v5/order/realtime?category=linear&settleCoin=USDT",
            200,
            &page,
        );
        assert_eq!(reports(&h), previous, "incomplete join cannot add reports");
        h.respond(
            "/v5/order/realtime?category=linear&settleCoin=USDC",
            200,
            EMPTY_LIST,
        );
        assert_eq!(reports(&h).as_slice(), std::slice::from_ref(&expected));
        assert!(
            h.engine.store().order(ORDER_ID).is_some(),
            "suppression cannot skip known-copy restoration"
        );
        if !clean {
            assert!(h.engine.store().order("healthy-report-peer").is_some());
        }
    }
}

#[test]
fn stale_join_discards_staged_unsupported_diagnostics_without_consuming_memory() {
    let mut h = ready();
    h.message("/v5/private", r#"{"success":true,"op":"subscribe"}"#);
    let mut row = snapshot_rows(OPEN_ORDERS).remove(0);
    row["orderId"] = serde_json::json!("staged-unsupported");
    row["orderType"] = serde_json::json!("FutureType");
    let page = snapshot_page(&[row]);
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDT",
        200,
        &page,
    );
    h.command(Command::CancelOrders {
        command: 901,
        ids: vec![ORDER_ID.into()],
    });
    h.respond("/v5/order/cancel-batch", 200,
        &format!(r#"{{"retCode":0,"result":{{"list":[{{"category":"linear","symbol":"ETHUSDT","orderId":"{ORDER_ID}"}}]}},"retExtInfo":{{"list":[{{"code":0,"msg":"OK"}}]}}}}"#));
    assert_eq!(
        h.engine.store().order(ORDER_ID).unwrap().status,
        OrderStatus::Canceled
    );
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    assert!(
        !h.logs.iter().any(|m| m.contains("staged-unsupported")),
        "old join cannot flush staged diagnostics after terminal ACK"
    );
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDT",
        200,
        &page,
    );
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDC",
        200,
        EMPTY_LIST,
    );
    assert_eq!(
        h.logs
            .iter()
            .filter(|m| m.as_str()
                == "Bybit order staged-unsupported has an unsupported type and is not shown")
            .count(),
        1
    );
}

// D1: every amend that writes `/v5/position/trading-stop` is a position TP/SL
// setting, so `34040` (not modified) is the requested state already holding.
#[test]
fn trading_stop_amends_treat_tpsl_not_modified_as_satisfied() {
    for kind in ["full", "trailing"] {
        let mut h = ready();
        h.message(
            "/v5/private",
            r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"position-stop","side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"StopLoss","tpslMode":"Full","qty":"0.005","triggerPrice":"86000","triggerBy":"MarkPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0}]}"#,
        );
        if kind == "trailing" {
            let mut trailing = h.engine.store().order("position-stop").unwrap().clone();
            trailing.kind = safe_cex_engine::model::OrderType::TrailingStopLoss;
            h.engine.store_mut().upsert_order(trailing);
        }
        h.command(Command::AmendOrder {
            command: 55,
            id: "position-stop".into(),
            price: Some(87000.0),
            amount: None,
        });
        let request = h.take("/v5/position/trading-stop");
        assert!(
            body(&request).contains(if kind == "trailing" {
                "trailingStop"
            } else {
                r#""tpslMode":"Full""#
            }),
            "{kind}"
        );
        h.reply(
            request.id,
            200,
            r#"{"retCode":34040,"retMsg":"not modified","result":{}}"#,
        );
        assert_eq!(h.result(55), Some(&Ok("[]".to_owned())), "{kind}");
        assert!(!h.has_request("/v5/position/trading-stop"), "{kind}");
    }
}

/// 1d-ii A3: a reconcile asked for while an older order read is still out
/// issues its own fresh read; the older one predates the uncertain write.
#[test]
fn a_reconcile_during_an_in_flight_order_read_issues_its_own_read() {
    let mut h = ready();
    place(
        &mut h,
        31,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    let first = h.take("/v5/order/create-batch").id;
    h.event(Event::HttpFailed {
        request: first,
        error: "connection reset",
    });
    let reads = |h: &Harness| {
        h.requests
            .iter()
            .filter(|r| r.url.contains("/v5/order/realtime"))
            .count()
    };
    // One page per settle coin.
    let per_read = reads(&h);
    assert!(per_read > 0, "the first seal's reconcile read");
    // BTCUSDT is fenced now; a second seal on another symbol.
    place(
        &mut h,
        32,
        vec![limit("ETHUSDT", OrderSide::Buy, 0.1, 3000.0)],
    );
    let second = h.take("/v5/order/create-batch").id;
    h.event(Event::HttpFailed {
        request: second,
        error: "connection reset",
    });
    assert_eq!(
        reads(&h),
        2 * per_read,
        "the second seal does not wait on or join the older read"
    );
    // The abandoned older pages answer late; their rows never publish.
    let stale = r#"{"retCode":0,"retMsg":"OK","result":{"category":"linear","list":[{"symbol":"BTCUSDT","orderId":"abandoned-order","side":"Buy","orderType":"Limit","orderStatus":"New","qty":"0.01","price":"90000","positionIdx":0}],"nextPageCursor":""}}"#;
    let older: Vec<_> = h
        .requests
        .iter()
        .filter(|r| r.url.contains("/v5/order/realtime"))
        .take(per_read)
        .map(|r| r.id)
        .collect();
    for id in older {
        h.reply(id, 200, stale);
    }
    assert!(h.engine.store().order("abandoned-order").is_none());
}

/// P1 fence: an Unknown placement refuses a retry on its symbol NotSent until a
/// fresh, clean order read reconciles it. Bybit has no placement scope, so the
/// engine marks it only from the Unknown write fact (fe7's Phase B); until then
/// this stays RED at the first retry.
#[test]
fn an_unknown_placement_refuses_a_retry_on_its_symbol_until_reconciled() {
    let mut h = ready();
    let realtime = |h: &Harness| -> Vec<_> {
        h.requests
            .iter()
            .filter(|r| r.url.contains("/v5/order/realtime"))
            .map(|r| r.id)
            .collect()
    };
    const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"category":"linear","list":[],"nextPageCursor":""}}"#;
    // An earlier Unknown on ETH leaves its reconcile read out.
    place(
        &mut h,
        30,
        vec![limit("ETHUSDT", OrderSide::Buy, 0.1, 3000.0)],
    );
    let eth = h.take("/v5/order/create-batch").id;
    h.event(Event::HttpFailed {
        request: eth,
        error: "connection reset",
    });
    let older = realtime(&h);
    assert!(!older.is_empty(), "ETH's reconcile read");
    // The BTC placement's answer is lost: it may rest, so it seals Unknown.
    place(
        &mut h,
        31,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    let btc = h.take("/v5/order/create-batch").id;
    h.event(Event::HttpFailed {
        request: btc,
        error: "connection reset",
    });
    assert!(
        h.result(31).is_some_and(Result::is_err),
        "the placement seals Unknown"
    );
    let fresh: Vec<_> = realtime(&h)
        .into_iter()
        .filter(|id| !older.contains(id))
        .collect();
    assert!(!fresh.is_empty(), "BTC's own reconcile read");
    let fenced = |h: &Harness, command| {
        h.result(command)
            .is_some_and(|result| result.as_ref().is_err_and(|e| e.contains("uncertain")))
            && !h.has_request("/v5/order/create-batch")
    };
    // 1. A user retry on BTC is refused NotSent and sends nothing.
    place(
        &mut h,
        32,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    assert!(fenced(&h, 32), "a retry is fenced: {:?}", h.result(32));
    // 2. The older read, which predates the BTC write, answers late: still fenced.
    for id in older {
        h.reply(id, 200, EMPTY);
    }
    place(
        &mut h,
        33,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    assert!(
        fenced(&h, 33),
        "a stale read never releases: {:?}",
        h.result(33)
    );
    // 3. BTC's fresh clean read releases it, and a retry places again.
    for id in fresh {
        h.reply(id, 200, EMPTY);
    }
    place(
        &mut h,
        34,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    assert!(
        h.has_request("/v5/order/create-batch"),
        "a fresh read clears the fence: {:?}",
        h.result(34)
    );
}

/// 8x5tm1 re-ask hazard: engine re-asks while a slow (>1 s) reconcile read is
/// out join it instead of replacing it, and that read still releases.
#[test]
fn engine_re_asks_join_a_slow_reconcile_read_that_still_releases() {
    let mut h = ready();
    place(
        &mut h,
        41,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    let write = h.take("/v5/order/create-batch").id;
    h.event(Event::HttpFailed {
        request: write,
        error: "connection reset",
    });
    let realtime = |h: &Harness| -> Vec<_> {
        h.requests
            .iter()
            .filter(|r| r.url.contains("/v5/order/realtime"))
            .map(|r| r.id)
            .collect()
    };
    let pages = realtime(&h);
    assert!(!pages.is_empty(), "the seal's reconcile read");
    // The engine re-asks at +1, +2, +4 s; none replaces the read still out.
    h.advance(8_000.0);
    assert_eq!(realtime(&h), pages, "re-asks join the slow read");
    const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"category":"linear","list":[],"nextPageCursor":""}}"#;
    for id in pages {
        h.reply(id, 200, EMPTY);
    }
    place(
        &mut h,
        42,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    assert!(
        h.has_request("/v5/order/create-batch"),
        "the slow read releases: {:?}",
        h.result(42)
    );
}

/// Cancel-all after an Unknown placement keeps the engine default
/// (`fence_uncertain_cancel_all`): Bybit's native cancel-all cannot cancel an
/// order that lands after it, so it is refused NotSent until the fresh read.
/// Cancel by id stays admitted throughout.
#[test]
fn cancel_all_after_an_unknown_placement_waits_for_the_fresh_read() {
    let mut h = ready();
    place(
        &mut h,
        61,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    let write = h.take("/v5/order/create-batch").id;
    h.event(Event::HttpFailed {
        request: write,
        error: "connection reset",
    });
    let pages: Vec<_> = h
        .requests
        .iter()
        .filter(|r| r.url.contains("/v5/order/realtime"))
        .map(|r| r.id)
        .collect();
    h.command(Command::CancelSymbolOrders {
        command: 62,
        symbol: "BTCUSDT".into(),
    });
    assert!(
        h.result(62)
            .is_some_and(|r| r.as_ref().is_err_and(|e| e.contains("uncertain"))),
        "cancel-all is refused while BTC is uncertain: {:?}",
        h.result(62)
    );
    assert!(!h.has_request("/v5/order/cancel-all"));
    const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"category":"linear","list":[],"nextPageCursor":""}}"#;
    for id in pages {
        h.reply(id, 200, EMPTY);
    }
    h.command(Command::CancelSymbolOrders {
        command: 63,
        symbol: "BTCUSDT".into(),
    });
    assert!(
        h.has_request("/v5/order/cancel-all"),
        "the fresh read admits cancel-all: {:?}",
        h.result(63)
    );
}

/// A refused snapshot page is re-read once, re-signed with Bybit's corrected
/// clock, through the classifier (a streamed refusal carries no list).
#[test]
fn a_refused_order_page_is_re_read_with_the_corrected_clock() {
    let mut h = log_safe_engine();
    place(
        &mut h,
        33,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    let batch = h.take("/v5/order/create-batch").id;
    h.event(Event::HttpFailed {
        request: batch,
        error: "connection reset",
    });
    h.advance(5000.0);
    let page = h.take("/v5/order/realtime");
    let refusal = serde_json::json!({"retCode":10002,"retMsg":format!("recv_window req_timestamp[6000],server_timestamp[9000], origin_string[{LOG_SECRET}] {SIGNED_URL}"),"result":{}}).to_string();
    h.reply(page.id, 200, &refusal);
    let retry = h
        .requests
        .iter()
        .find(|r| r.url == page.url)
        .expect("the same page is re-read once")
        .clone();
    assert_eq!(
        Harness::header(&retry, "X-BAPI-TIMESTAMP").as_deref(),
        Some("9000")
    );
    assert_private_notices_are_safe(&h);
}

/// A second signature refusal of a page fails without leaking its signed origin.
#[test]
fn a_twice_refused_order_page_never_leaks_its_signed_origin() {
    let mut h = log_safe_engine();
    place(
        &mut h,
        34,
        vec![limit("BTCUSDT", OrderSide::Buy, 0.01, 90000.0)],
    );
    let batch = h.take("/v5/order/create-batch").id;
    h.event(Event::HttpFailed {
        request: batch,
        error: "connection reset",
    });
    let page = h.take("/v5/order/realtime");
    h.reply(page.id, 200, &rejected_signed_origin(10004));
    let retry = h
        .requests
        .iter()
        .position(|r| r.url == page.url)
        .map(|i| h.requests.remove(i))
        .expect("re-read once");
    h.reply(retry.id, 200, &rejected_signed_origin(10004));
    assert!(
        !h.requests.iter().any(|r| r.url == page.url),
        "no third read"
    );
    assert_private_notices_are_safe(&h);
    assert!(h.logs.iter().all(|line| !line.contains(LOG_SECRET)));
}

/// Answer every outstanding account read.
fn drain_account_reads(h: &mut Harness) {
    while h.has_request("/v5/order/realtime") {
        h.respond("/v5/order/realtime", 200, EMPTY_LIST);
    }
    while h.has_request("/v5/position/list") {
        h.respond("/v5/position/list", 200, EMPTY_LIST);
    }
    while h.has_request("/v5/account/wallet-balance") {
        h.respond("/v5/account/wallet-balance", 200, WALLET);
    }
}

fn polls_account(h: &Harness) -> bool {
    h.has_request("/v5/position/list") || h.has_request("/v5/account/wallet-balance")
}

#[test]
fn live_private_topics_turn_position_and_wallet_polls_into_a_60s_safety_reconcile() {
    let mut h = Harness::started(credentials());
    respond_startup(&mut h);
    respond_snapshot(&mut h);
    // Before the topics are live, every tick re-reads positions and the wallet.
    h.advance(POLL_MS);
    assert!(polls_account(&h), "{:?}", h.paths());
    drain_account_reads(&mut h);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"auth","conn_id":"c"}"#,
    );
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"subscribe","conn_id":"c"}"#,
    );
    // The subscribe ack covers the gap with one read.
    assert!(polls_account(&h));
    drain_account_reads(&mut h);
    // Live topics: no position or wallet reads for the next 55 s of ticks...
    // (a live venue answers the private pings, so the socket stays up).
    let pong = r#"{"success":true,"ret_msg":"pong","conn_id":"c","op":"pong"}"#;
    for _ in 0..11 {
        h.advance(POLL_MS);
        h.message("/v5/private", pong);
        assert!(!polls_account(&h), "{:?}", h.paths());
    }
    // ...then the 60 s safety reconcile, on the first tick past it.
    for _ in 0..2 {
        h.advance(POLL_MS);
        h.message("/v5/private", pong);
    }
    assert!(h.has_request("/v5/position/list") && h.has_request("/v5/account/wallet-balance"));
    drain_account_reads(&mut h);
    // A dropped private socket falls back to every tick until the topics are live again.
    h.event(Event::WsClosed { socket, code: 1006 });
    drain_account_reads(&mut h);
    h.advance(POLL_MS);
    assert!(polls_account(&h), "{:?}", h.paths());
}

#[test]
fn an_open_position_keeps_its_per_tick_position_read_while_the_wallet_waits() {
    let mut h = Harness::started(credentials());
    respond_startup(&mut h);
    respond_snapshot(&mut h);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"auth","conn_id":"c"}"#,
    );
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"subscribe","conn_id":"c"}"#,
    );
    // The gap read reports an open BTCUSDT long.
    while h.has_request("/v5/order/realtime") {
        h.respond("/v5/order/realtime", 200, EMPTY_LIST);
    }
    while h.has_request("/v5/position/list") {
        h.respond("/v5/position/list", 200, POSITIONS);
    }
    while h.has_request("/v5/account/wallet-balance") {
        h.respond("/v5/account/wallet-balance", 200, WALLET);
    }
    let pong = r#"{"success":true,"ret_msg":"pong","conn_id":"c","op":"pong"}"#;
    for _ in 0..11 {
        h.advance(POLL_MS);
        h.message("/v5/private", pong);
        // Venue-reported unrealized PnL: positions every tick; the wallet waits.
        assert!(h.has_request("/v5/position/list"), "{:?}", h.paths());
        assert!(
            !h.has_request("/v5/account/wallet-balance"),
            "{:?}",
            h.paths()
        );
        while h.has_request("/v5/position/list") {
            h.respond("/v5/position/list", 200, POSITIONS);
        }
    }
}
