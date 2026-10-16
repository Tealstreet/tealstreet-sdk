//! The production BybitExchange, constructed from real options and driven through the
//! `Exchange` trait over a scripted wire.
#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_types::{OrderSide, OrderType, PositionSide};
use serde_json::json;
use support::{harness, options};

fn order_row(id: &str, symbol: &str) -> serde_json::Value {
    json!({
        "orderId": id, "orderLinkId": "", "symbol": symbol, "side": "Buy",
        "orderType": "Limit", "price": "25000", "qty": "0.010", "leavesQty": "0.010",
        "cumExecQty": "0", "orderStatus": "New", "timeInForce": "GTC",
        "reduceOnly": false, "closeOnTrigger": false, "stopOrderType": "",
        "triggerPrice": "", "stopLoss": "24000", "takeProfit": "", "tpslMode": "",
        "createType": "CreateByUser", "positionIdx": 0, "triggerBy": "", "slTriggerBy": "MarkPrice"
    })
}

#[test]
fn fetch_orders_pages_each_settle_coin_and_maps_virtual_stops() {
    let h = harness(options(), |w| {
        w.ok(
            "GET",
            "/v5/order/realtime",
            json!({"list": [order_row("o-1", "BTCUSDT")], "nextPageCursor": ""}),
        );
    });
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    let calls = h.wire.calls("/v5/order/realtime");
    let coins: Vec<_> = calls
        .iter()
        .map(|c| c.params["settleCoin"].clone())
        .collect();
    assert_eq!(
        coins,
        [json!("USDT"), json!("USDC")],
        "one read per UNIFIED settle coin"
    );
    assert!(
        calls
            .iter()
            .all(|c| c.params["category"] == "linear" && c.params["limit"] == "50"),
        "linear pages of 50: {calls:?}"
    );
    // Each settle coin answered the same row: an order and its virtual stop-loss, twice.
    assert_eq!(orders.len(), 4, "{orders:?}");
    assert_eq!(orders[0].id, "o-1");
    assert_eq!(orders[0].r#type, OrderType::Limit);
    assert_eq!(orders[0].side, OrderSide::Buy);
    assert_eq!(orders[0].price, 25000.0);
    assert_eq!(orders[0].remaining, 0.01);
    assert_eq!(orders[1].id, "o-1__stop_loss");
    assert_eq!(orders[1].r#type, OrderType::StopLoss);
    assert_eq!(
        orders[1].side,
        OrderSide::Sell,
        "virtual stop closes the parent side"
    );
    assert_eq!(orders[1].price, 24000.0);
}

#[test]
fn fetch_positions_reads_every_settle_coin_and_dedupes() {
    let h = harness(options(), |w| {
        w.ok(
            "GET",
            "/v5/position/list",
            json!({"list": [{
                "symbol": "BTCUSDT", "side": "Buy", "size": "0.5", "avgPrice": "25000",
                "positionValue": "12500", "leverage": "10", "unrealisedPnl": "12.5",
                "curRealisedPnl": "-1", "liqPrice": "20000", "positionIdx": 0,
                "positionIM": "1250", "updatedTime": "1700000000000", "seq": 7,
                "positionStatus": "Normal"
            }], "nextPageCursor": ""}),
        );
    });
    let positions = h
        .drive(h.venue().fetch_positions(None))
        .expect("fetchPositions");
    let calls = h.wire.calls("/v5/position/list");
    assert_eq!(calls.len(), 2, "USDT then USDC: {calls:?}");
    assert_eq!(calls[0].params["settleCoin"], "USDT");
    assert_eq!(calls[1].params["settleCoin"], "USDC");
    assert_eq!(
        positions.len(),
        2,
        "one row per settle coin, flattened: {positions:?}"
    );
    let p = &positions[0];
    assert_eq!(p.symbol, "BTCUSDT");
    assert_eq!(p.side, PositionSide::Long);
    assert_eq!(p.contracts, 0.5);
    assert_eq!(p.entry_price, 25000.0);
    assert_eq!(p.notional, 12500.0);
    assert_eq!(p.unrealized_pnl, 12.5);
    assert_eq!(p.leverage, 10.0);
    assert_eq!(p.margin_size, Some(1250.0));
}

#[test]
fn inherited_base_fallbacks_reject_not_implemented() {
    let h = harness(options(), |_| {});
    // ts: base.ts:799 fetchSymbolMaxLeverage, :989 scheduleCancel, :1138 fetchPublicTrades.
    for error in [
        h.drive(h.venue().fetch_symbol_max_leverage("BTCUSDT".into()))
            .map(|_| ())
            .unwrap_err(),
        h.drive(h.venue().schedule_cancel(Some(Some(1000.0))))
            .unwrap_err(),
        h.drive(h.venue().fetch_public_trades("BTCUSDT".into(), None))
            .map(|_| ())
            .unwrap_err(),
    ] {
        assert_eq!(error.message, "Not implemented");
    }
    assert!(
        h.wire.requests.borrow().is_empty(),
        "an inherited rejection sends nothing"
    );
}

fn instrument(symbol: &str, status: &str) -> serde_json::Value {
    json!({
        "symbol": symbol, "contractType": "LinearPerpetual", "status": status,
        "baseCoin": "BTC", "quoteCoin": "USDT", "settleCoin": "USDT", "deliveryTime": "0",
        "priceFilter": {"tickSize": "0.10", "minPrice": "0.10", "maxPrice": "199999.80"},
        "lotSizeFilter": {"qtyStep": "0.001", "minOrderQty": "0.001", "maxOrderQty": "100.000"},
        "leverageFilter": {"minLeverage": "1", "maxLeverage": "100.00", "leverageStep": "0.01"}
    })
}

#[test]
fn fetch_markets_joins_fees_and_prelaunch_markets() {
    let h = harness(options(), |w| {
        w.ok(
            "GET",
            "/v5/account/fee-rate",
            json!({"list": [{"symbol": "BTCUSDT", "takerFeeRate": "0.00055", "makerFeeRate": "0.0002"}]}),
        );
        w.ok(
            "GET",
            "/v5/market/instruments-info",
            json!({"list": [instrument("BTCUSDT", "Trading")], "nextPageCursor": ""}),
        );
        w.route_when(
            "GET",
            "/v5/market/instruments-info",
            "status",
            "PreLaunch",
            json!({"retCode": 0, "retMsg": "OK", "result": {"list": [instrument("NEWUSDT", "PreLaunch")]}}),
        );
    });
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    assert_eq!(
        markets
            .iter()
            .map(|m| m.symbol.as_str())
            .collect::<Vec<_>>(),
        ["BTCUSDT", "NEWUSDT"],
        "Trading first, then PreLaunch"
    );
    let btc = &markets[0];
    assert!(btc.active);
    assert_eq!(btc.precision.price, 0.1);
    assert_eq!(btc.precision.amount, 0.001);
    assert_eq!(btc.limits.amount.max, 100.0);
    assert_eq!(btc.limits.leverage.max, 100.0);
    assert_eq!(btc.taker_fee, Some(0.00055), "fee joined by symbol");
    assert_eq!(btc.maker_fee, Some(0.0002));
    assert_eq!(markets[1].taker_fee, None, "no fee row for NEWUSDT");
    assert_eq!(h.wire.calls("/v5/market/instruments-info").len(), 2);
}

#[test]
fn fetch_tickers_parses_rows_for_known_markets() {
    let h = harness(options(), |w| {
        w.ok(
            "GET",
            "/v5/market/tickers",
            json!({"category": "linear", "list": [
                {"symbol": "BTCUSDT", "lastPrice": "25000", "bid1Price": "24999.9",
                 "ask1Price": "25000.1", "markPrice": "25000.5", "indexPrice": "25001",
                 "price24hPcnt": "0.012", "openInterest": "1000", "fundingRate": "0.0001",
                 "nextFundingTime": "1700003600000", "volume24h": "10", "turnover24h": "250000"},
                {"symbol": "UNLISTED", "lastPrice": "1"}
            ]}),
        );
    });
    seed_market(&h);
    let tickers = h.drive(h.venue().fetch_tickers()).expect("fetchTickers");
    assert_eq!(
        tickers.len(),
        1,
        "rows without a market are skipped: {tickers:?}"
    );
    let t = &tickers[0];
    assert_eq!(t.symbol, "BTCUSDT");
    assert_eq!(t.last, 25000.0);
    assert_eq!(t.bid, 24999.9);
    assert_eq!(t.mark, 25000.5);
    assert_eq!(t.percentage, 1.2);
    assert_eq!(t.funding_rate, 0.0001);
    assert_eq!(t.volume, 10.0);
    assert_eq!(t.quote_volume, 250000.0);
    assert_eq!(
        h.wire.calls("/v5/market/tickers")[0].params["category"],
        "linear"
    );
}

#[test]
fn get_account_reads_user_and_inviter_ids() {
    let h = harness(options(), |w| {
        w.ok(
            "GET",
            "/v5/user/query-api",
            json!({"userID": 12345, "inviterID": 678, "vipLevel": "No VIP"}),
        );
    });
    let account = h.drive(h.venue().get_account()).expect("getAccount");
    let account = serde_json::to_value(&account).unwrap();
    assert_eq!(
        account["userId"], "12345",
        "numeric userID crosses as its text"
    );
    assert_eq!(account["affiliateId"], "678");

    let rejected = harness(options(), |w| {
        w.route(
            "GET",
            "/v5/user/query-api",
            200,
            json!({"retCode": 10003, "retMsg": "API key is invalid.", "result": {}}),
        );
    });
    let error = rejected
        .drive(rejected.venue().get_account())
        .expect_err("a non-zero retCode rejects");
    assert_eq!(error.message, "API key is invalid.");
}

fn wallet() -> serde_json::Value {
    json!({"list": [{
        "accountType": "UNIFIED", "totalEquity": "1000.5", "totalWalletBalance": "990",
        "totalMarginBalance": "995", "totalAvailableBalance": "800",
        "totalInitialMargin": "150", "totalMaintenanceMargin": "20",
        "coin": [{"coin": "USDT", "equity": "1000.5", "walletBalance": "990", "usdValue": "1000.4"}]
    }]})
}

#[test]
fn validate_account_checks_the_key_then_reads_balance() {
    let h = harness(options(), |w| {
        w.ok(
            "GET",
            "/v5/account/info",
            json!({"marginMode": "REGULAR_MARGIN", "unifiedMarginStatus": 6}),
        );
        w.ok("GET", "/v5/account/wallet-balance", wallet());
    });
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount");
    let result = serde_json::to_value(&result).unwrap();
    assert_eq!(result["error"], "");
    assert_eq!(
        result["balance"]["total"], 1000.5,
        "no positions: total is equity"
    );
    assert_eq!(result["balance"]["free"], 800.0);
    assert_eq!(result["balance"]["used"], 170.0, "IM + MM");
    assert_eq!(result["balance"]["coins"]["USDT"]["notional"], 1000.4);
    let wallet = h.wire.calls("/v5/account/wallet-balance");
    assert_eq!(wallet[0].params["accountType"], "UNIFIED");

    let bad = harness(options(), |w| {
        w.route(
            "GET",
            "/v5/account/info",
            200,
            json!({"retCode": 10002, "retMsg": "invalid request, please check your timestamp and recv_window param", "result": {}}),
        );
    });
    let result = bad
        .drive(bad.venue().validate_account())
        .expect("validateAccount");
    let result = serde_json::to_value(&result).unwrap();
    assert_eq!(result["error"], "Check your computer time and date");
    assert!(
        bad.wire.calls("/v5/account/wallet-balance").is_empty(),
        "a rejected key reads no balance"
    );
}

fn seed_market(h: &support::Harness) {
    let market: safe_cex_types::Market = serde_json::from_value(json!({
        "id": "BTCUSDT", "symbol": "BTCUSDT", "base": "BTC", "quote": "USDT", "active": true,
        "precision": {"amount": 0.001, "price": 0.1},
        "limits": {"amount": {"min": 0.001, "max": 100}, "leverage": {"min": 1, "max": 100}}
    }))
    .unwrap();
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}

fn position_list(leverage: &str) -> serde_json::Value {
    json!({"list": [{
        "symbol": "BTCUSDT", "side": "Buy", "size": "0.5", "avgPrice": "25000",
        "positionValue": "12500", "leverage": leverage, "unrealisedPnl": "0",
        "curRealisedPnl": "0", "liqPrice": "0", "positionIdx": 0, "updatedTime": "1", "seq": 1,
        "positionStatus": "Normal"
    }], "nextPageCursor": ""})
}

#[test]
fn set_leverage_clamps_to_the_market_and_refreshes_the_position() {
    let h = harness(options(), |w| {
        w.ok("GET", "/v5/position/list", position_list("10"));
        w.ok("POST", "/v5/position/set-leverage", json!({}));
    });
    seed_market(&h);
    h.drive(h.venue().set_leverage("BTCUSDT".into(), 150.0, None))
        .expect("setLeverage");
    let posts = h.wire.calls("/v5/position/set-leverage");
    assert_eq!(posts.len(), 1);
    assert_eq!(
        posts[0].body,
        json!({"category": "linear", "symbol": "BTCUSDT", "buyLeverage": "100", "sellLeverage": "100"}),
        "clamped to the market's 100x"
    );
    let reads = h.wire.calls("/v5/position/list");
    assert_eq!(reads.len(), 2, "position read before and after the write");
    assert_eq!(reads[0].params["symbol"], "BTCUSDT");

    let unchanged = harness(options(), |w| {
        w.ok("GET", "/v5/position/list", position_list("100"));
    });
    seed_market(&unchanged);
    unchanged
        .drive(
            unchanged
                .venue()
                .set_leverage("BTCUSDT".into(), 100.0, None),
        )
        .expect("setLeverage at the current leverage");
    assert!(
        unchanged.wire.calls("/v5/position/set-leverage").is_empty(),
        "an unchanged leverage sends nothing"
    );
}

#[test]
fn change_position_mode_switches_linear_usdt_and_refreshes() {
    let h = harness(options(), |w| {
        w.route(
            "POST",
            "/v5/position/switch-mode",
            200,
            json!({"retCode": 0, "retMsg": "All symbols switched successfully.", "result": {}}),
        );
        w.ok("GET", "/v5/position/list", position_list("10"));
    });
    h.drive(h.venue().change_position_mode(true, Some("ETHUSDT".into())))
        .expect("changePositionMode");
    let posts = h.wire.calls("/v5/position/switch-mode");
    assert_eq!(
        posts[0].body,
        json!({"category": "linear", "coin": "USDT", "mode": 3}),
        "the symbol is cleared: bulk linear USDT"
    );
    assert!(
        !h.wire.calls("/v5/position/list").is_empty(),
        "positions refreshed"
    );

    let inverse = harness(options(), |_| {});
    let error = inverse
        .drive(
            inverse
                .venue()
                .change_position_mode(true, Some("BTCUSD".into())),
        )
        .expect_err("inverse symbols are refused");
    assert_eq!(
        error.message,
        "Bybit position mode can only be changed on linear markets here (BTCUSD)"
    );
}

#[test]
fn change_margin_type_switches_the_account_and_restamps_positions() {
    let h = harness(options(), |w| {
        w.route(
            "POST",
            "/v5/account/set-margin-mode",
            200,
            json!({"retCode": 0, "retMsg": "Request accepted", "result": {"reasons": []}}),
        );
        w.ok("GET", "/v5/position/list", position_list("10"));
    });
    h.drive(h.venue().change_margin_type(true, None))
        .expect("changeMarginType");
    assert_eq!(
        h.wire.calls("/v5/account/set-margin-mode")[0].body,
        json!({"setMarginMode": "ISOLATED_MARGIN"})
    );
    let positions = safe_cex_store::Store::positions(h.store.as_ref());
    assert!(!positions.is_empty());
    assert!(
        positions
            .iter()
            .all(|p| p.margin_type == Some(safe_cex_types::MarginType::Isolated)),
        "refetched positions carry the new margin type: {positions:?}"
    );

    let refused = harness(options(), |w| {
        w.route(
            "POST",
            "/v5/account/set-margin-mode",
            200,
            json!({"retCode": 0, "retMsg": "Request accepted", "result": {"reasons": [{"reasonCode": "3400045", "reasonMsg": "Insufficient margin"}]}}),
        );
    });
    let error = refused
        .drive(refused.venue().change_margin_type(true, None))
        .expect_err("a reason refuses the switch");
    assert_eq!(error.message, "Insufficient margin");
}

#[test]
fn start_loads_public_and_private_state_and_dials_both_sockets() {
    let h = harness(options(), |w| {
        w.ok(
            "GET",
            "/v5/account/info",
            json!({"marginMode": "ISOLATED_MARGIN", "unifiedMarginStatus": 6}),
        );
        w.ok("GET", "/v5/user/query-api", json!({"vipLevel": "No VIP"}));
        w.ok("GET", "/v5/account/fee-rate", json!({"list": []}));
        w.ok(
            "GET",
            "/v5/market/instruments-info",
            json!({"list": [instrument("BTCUSDT", "Trading")]}),
        );
        w.route_when(
            "GET",
            "/v5/market/instruments-info",
            "status",
            "PreLaunch",
            json!({"retCode": 0, "retMsg": "OK", "result": {"list": []}}),
        );
        w.ok(
            "GET",
            "/v5/market/tickers",
            json!({"list": [{"symbol": "BTCUSDT", "lastPrice": "25000", "bid1Price": "1",
                "ask1Price": "1", "markPrice": "1", "indexPrice": "1", "price24hPcnt": "0",
                "openInterest": "0", "fundingRate": "0", "nextFundingTime": "0",
                "volume24h": "0", "turnover24h": "0"}]}),
        );
        w.ok(
            "GET",
            "/v5/order/realtime",
            json!({"list": [order_row("o-1", "BTCUSDT")], "nextPageCursor": ""}),
        );
        w.ok("GET", "/v5/position/list", position_list("10"));
        w.ok("GET", "/v5/account/wallet-balance", wallet());
    });
    h.drive(h.venue().start()).expect("start");
    for _ in 0..40 {
        h.runtime.pump();
        h.runtime.now.set(h.runtime.now.get() + 50.0);
    }
    let store = h.store.as_ref();
    let loaded = safe_cex_store::Store::loaded(store).clone();
    assert!(
        loaded.markets && loaded.tickers,
        "public state loaded: {loaded:?}"
    );
    assert!(
        loaded.orders && loaded.positions && loaded.balance,
        "private snapshot loaded: {loaded:?}"
    );
    assert_eq!(safe_cex_store::Store::markets(store).len(), 1);
    assert_eq!(safe_cex_store::Store::tickers(store)[0].last, 25000.0);
    assert!(
        safe_cex_store::Store::orders(store)
            .iter()
            .any(|o| o.id == "o-1"),
        "orders snapshot stored"
    );
    assert!(
        safe_cex_store::Store::positions(store)
            .iter()
            .all(|p| p.margin_type == Some(safe_cex_types::MarginType::Isolated)),
        "the margin probe ran before positions were stamped"
    );
    let dialed: Vec<String> = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .map(|(url, _)| url.clone())
        .collect();
    assert!(
        dialed.iter().any(|u| u.contains("/v5/public/linear")),
        "public socket dialed after markets: {dialed:?}"
    );
    assert!(
        dialed.iter().any(|u| u.contains("/v5/private")),
        "private socket dialed: {dialed:?}"
    );
    let public = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("/v5/public/linear"))
        .map(|(_, s)| s.clone())
        .unwrap();
    assert!(
        public
            .sent
            .borrow()
            .iter()
            .any(|frame| frame.contains("tickers.BTCUSDT")),
        "the public socket subscribed the loaded market: {:?}",
        public.sent.borrow()
    );
}

fn place(
    h: &support::Harness,
    opts: serde_json::Value,
) -> Result<Vec<String>, safe_cex_exchange::runtime::ExchangeError> {
    let opts: safe_cex_types::PlaceOrderOpts = serde_json::from_value(opts).unwrap();
    let venue = h.venue();
    h.drive(Box::pin(venue.base().place_order(venue, opts)))
}

#[test]
fn place_order_formats_a_limit_order_and_records_it() {
    let h = harness(options(), |w| {
        w.ok(
            "GET",
            "/v5/position/list",
            json!({"list": [], "nextPageCursor": ""}),
        );
        w.ok(
            "POST",
            "/v5/order/create",
            json!({"orderId": "new-1", "orderLinkId": ""}),
        );
    });
    seed_market(&h);
    let ids = place(
        &h,
        json!({"symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": 0.01,
               "price": 25000.07, "stopLoss": 24000}),
    )
    .expect("placeOrder");
    assert_eq!(ids, ["new-1"]);
    let posts = h.wire.calls("/v5/order/create");
    assert_eq!(posts.len(), 1);
    let body = &posts[0].body;
    assert_eq!(body["category"], "linear");
    assert_eq!(body["side"], "Buy");
    assert_eq!(body["orderType"], "Limit");
    assert_eq!(body["qty"], "0.01");
    assert_eq!(body["price"], "25000.1", "adjusted to the tick");
    assert_eq!(body["stopLoss"], "24000");
    assert_eq!(body["slTriggerBy"], "MarkPrice");
    assert_eq!(body["timeInForce"], "GTC");
    assert_eq!(body["reduceOnly"], false);
    assert_eq!(body["closeOnTrigger"], false);
    assert_eq!(body["positionIdx"], 0, "one-way: index 0");
    assert!(
        body.get("orderLinkId").is_none(),
        "no caller id, no generated id: {body}"
    );

    let with_id = harness(options(), |w| {
        w.ok(
            "GET",
            "/v5/position/list",
            json!({"list": [], "nextPageCursor": ""}),
        );
        w.ok("POST", "/v5/order/create", json!({"orderId": "new-2"}));
    });
    seed_market(&with_id);
    place(
        &with_id,
        json!({"symbol": "BTCUSDT", "type": "limit", "side": "sell", "amount": 0.01,
               "price": 26000, "clientOrderId": "caller-1"}),
    )
    .expect("placeOrder with a caller id");
    assert_eq!(
        with_id.wire.calls("/v5/order/create")[0].body["orderLinkId"],
        "caller-1",
        "the caller's id goes out verbatim as orderLinkId"
    );

    let rejected = harness(options(), |w| {
        w.ok(
            "GET",
            "/v5/position/list",
            json!({"list": [], "nextPageCursor": ""}),
        );
        w.route(
            "POST",
            "/v5/order/create",
            200,
            json!({"retCode": 110007, "retMsg": "ab not enough for new order", "result": {}}),
        );
    });
    seed_market(&rejected);
    let error = place(
        &rejected,
        json!({"symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": 0.01, "price": 1}),
    )
    .expect_err("a non-OK create rejects");
    assert_eq!(error.message, "ab not enough for new order");
}

fn store_order(fields: serde_json::Value) -> safe_cex_types::Order {
    let mut order = json!({
        "id": "o-1", "status": "open", "symbol": "BTCUSDT", "type": "limit", "side": "buy",
        "price": 25000, "amount": 0.01, "filled": 0, "remaining": 0.01, "reduceOnly": false,
        "timeInForce": "GoodTillCancel", "lastUpdate": 1
    });
    for (key, value) in fields.as_object().unwrap() {
        order[key] = value.clone();
    }
    serde_json::from_value(order).unwrap()
}

fn seed_orders(h: &support::Harness, orders: Vec<safe_cex_types::Order>) {
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            orders: Some(orders),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}

fn update(
    h: &support::Harness,
    order: safe_cex_types::Order,
    update: serde_json::Value,
) -> Result<Vec<String>, safe_cex_exchange::runtime::ExchangeError> {
    let update = serde_json::from_value(update).unwrap();
    h.drive(
        h.venue()
            .update_order(safe_cex_types::UpdateOrderOpts { order, update }),
    )
}

#[test]
fn update_order_amends_limits_conditionals_and_virtual_stops() {
    let h = harness(options(), |w| {
        w.ok("POST", "/v5/order/amend", json!({"orderId": "o-1"}));
    });
    seed_market(&h);
    let ids = update(
        &h,
        store_order(json!({})),
        json!({"price": 25100, "amount": 0.02}),
    )
    .expect("updateOrder limit");
    assert_eq!(ids, ["o-1"]);
    let limit = &h.wire.calls("/v5/order/amend")[0].body;
    assert_eq!(
        limit,
        &json!({"category": "linear", "orderId": "o-1", "symbol": "BTCUSDT",
                "qty": "0.02", "price": "25100"}),
        "a limit amends price and qty in place"
    );

    let conditional = store_order(json!({"id": "c-1", "type": "stop_market"}));
    update(&h, conditional, json!({"price": 24500})).expect("updateOrder conditional");
    assert_eq!(
        h.wire.calls("/v5/order/amend")[1].body,
        json!({"category": "linear", "orderId": "c-1", "symbol": "BTCUSDT",
               "triggerPrice": "24500"}),
        "a free-floating stop amends its trigger"
    );

    seed_orders(&h, vec![store_order(json!({}))]);
    let stop = store_order(json!({"id": "o-1__stop_loss", "type": "stop_market", "side": "sell"}));
    update(&h, stop, json!({"price": 24000})).expect("updateOrder virtual stop");
    assert_eq!(
        h.wire.calls("/v5/order/amend")[2].body,
        json!({"category": "linear", "orderId": "o-1", "symbol": "BTCUSDT",
               "stopLoss": "24000"}),
        "a virtual stop amends its parent's stopLoss"
    );

    let rejected = harness(options(), |w| {
        w.route(
            "POST",
            "/v5/order/amend",
            200,
            json!({"retCode": 10001, "retMsg": "params error", "result": {}}),
        );
    });
    seed_orders(&rejected, vec![store_order(json!({}))]);
    let stop = store_order(json!({"id": "o-1__stop_loss", "type": "stop_market"}));
    let error = update(&rejected, stop, json!({"price": 1})).expect_err("a virtual amend rejects");
    assert_eq!(error.message, "params error");
}

#[test]
fn update_order_falls_back_to_trading_stop_for_position_stops() {
    let h = harness(options(), |w| {
        w.ok(
            "GET",
            "/v5/position/list",
            json!({"list": [], "nextPageCursor": ""}),
        );
        w.ok("POST", "/v5/position/trading-stop", json!({}));
    });
    seed_market(&h);
    // No ticker, so no mark: the cancel-and-replace path cannot run.
    let stop = store_order(
        json!({"id": "s-1", "type": "take_profit_market", "side": "sell",
                                   "amount": 0.005, "info": {"tpslMode": "Partial"}}),
    );
    let ids = update(&h, stop, json!({"price": 27000})).expect("updateOrder trading stop");
    assert!(ids.is_empty(), "a trading stop returns no order ids");
    assert!(h.wire.calls("/v5/order/amend").is_empty());
    assert_eq!(
        h.wire.calls("/v5/position/trading-stop")[0].body,
        json!({"category": "linear", "symbol": "BTCUSDT", "positionIdx": 0,
               "tpslMode": "Partial", "takeProfit": "27000", "tpSize": "0.005"}),
    );
}

#[test]
fn cancel_orders_batches_per_category_and_skips_virtual_stops() {
    let h = harness(options(), |w| {
        w.ok("POST", "/v5/order/cancel-batch", json!({"list": []}));
    });
    let orders = vec![
        store_order(json!({"id": "o-1"})),
        store_order(json!({"id": "o-1__stop_loss", "type": "stop_market"})),
        store_order(json!({"id": "o-2"})),
    ];
    h.drive(h.venue().cancel_orders(orders))
        .expect("cancelOrders");
    let calls = h.wire.calls("/v5/order/cancel-batch");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].body,
        json!({"category": "linear", "request": [
            {"symbol": "BTCUSDT", "orderId": "o-1"},
            {"symbol": "BTCUSDT", "orderId": "o-2"}
        ]}),
        "virtual stops are dropped before the batch"
    );

    let only_virtual = harness(options(), |_| {});
    only_virtual
        .drive(
            only_virtual
                .venue()
                .cancel_orders(vec![store_order(json!({"id": "o-1__take_profit"}))]),
        )
        .expect("nothing to cancel");
    assert!(only_virtual.wire.requests.borrow().is_empty());

    let failed = harness(options(), |w| {
        w.ok("POST", "/v5/order/cancel-batch", json!({"list": []}));
    });
    failed.wire.routes.borrow_mut().clear();
    failed.wire.route(
        "POST",
        "/v5/order/cancel-batch",
        200,
        json!({"retCode": 0, "retMsg": "OK", "result": {"list": []},
               "retExtInfo": {"list": [{"code": 0, "msg": "OK"},
                                       {"code": 110001, "msg": "order not exists or too late to cancel"},
                                       {"code": 10001, "msg": "boom"}]}}),
    );
    let error = failed
        .drive(failed.venue().cancel_orders(vec![store_order(json!({}))]))
        .expect_err("a per-order failure rejects");
    assert_eq!(error.message, "boom", "already-gone rows are not failures");
}

#[test]
fn demo_cancel_retires_rows_and_reduce_only_limits_reconcile_from_fills() {
    let demo = || {
        support::harness_as(safe_cex_types::ExchangeName::Bybitdemo, options(), |w| {
            w.ok(
                "GET",
                "/v5/position/list",
                json!({"list": [], "nextPageCursor": ""}),
            );
            w.ok("POST", "/v5/order/create", json!({"orderId": "ro-1"}));
            w.ok("POST", "/v5/order/cancel-batch", json!({"list": []}));
            w.ok(
                "GET",
                "/v5/execution/list",
                json!({"list": [{"orderId": "ro-1", "execQty": "0.01"},
                                {"orderId": "other", "execQty": "5"}],
                       "nextPageCursor": ""}),
            );
        })
    };
    let has = |h: &support::Harness, id: &str| h.store.orders().iter().any(|o| o.id == id);

    let h = demo();
    seed_orders(&h, vec![store_order(json!({}))]);
    h.drive(h.venue().cancel_orders(vec![store_order(json!({}))]))
        .expect("cancelOrders");
    assert!(!has(&h, "o-1"), "demo retires a cancelled row locally");

    let h = demo();
    seed_market(&h);
    place(
        &h,
        json!({"symbol": "BTCUSDT", "type": "limit", "side": "sell", "amount": 0.01,
               "price": 26000, "reduceOnly": true}),
    )
    .expect("placeOrder");
    assert!(has(&h, "ro-1"), "demo records the placed order locally");
    h.advance(1_100.);
    let fills = h.wire.calls("/v5/execution/list");
    assert_eq!(fills.len(), 1, "the 1s reconcile reads fills once");
    assert_eq!(fills[0].params["orderId"], "ro-1");
    assert_eq!(fills[0].params["category"], "linear");
    assert!(
        !has(&h, "ro-1"),
        "a fully filled reduce-only limit is retired"
    );
}

#[test]
#[ignore = "requires installed live TypeScript; mandatory venue-oracle:bybit"]
fn bound_order_writes_match_maintained_source() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut cases = vec![];
    for fields in [
        json!({}),
        json!({"type": "conditional"}),
        json!({"type": "stop_market"}),
        json!({"type": "take_profit_market"}),
        json!({"type": "stop_market", "info": {"tpslMode": "Full"}, "amount": 0}),
        json!({"type": "take_profit_market", "info": {"tpslMode": "Partial"}}),
        json!({"id": "o-1__stop_loss", "type": "stop_market"}),
        json!({"id": "o-1__take_profit", "type": "take_profit_market"}),
    ] {
        for change in [
            json!({"amount": 0}),
            json!({"price": 0}),
            json!({"price": 25100, "amount": 0.02}),
        ] {
            for message in ["OK", "order not modified", "params error"] {
                cases.push(json!({"kind": "update", "parents": [store_order(json!({}))],
                    "options": {"order": store_order(fields.clone()), "update": change}, "message": message}));
            }
        }
    }
    for orders in [
        vec![],
        vec![store_order(json!({"id": "o-1__stop_loss"}))],
        vec![
            store_order(json!({})),
            store_order(json!({"id": "o-1__take_profit"})),
        ],
        (0..12)
            .map(|i| store_order(json!({"id": format!("order-{i}")})))
            .collect(),
    ] {
        for (message, extra) in [
            ("OK", json!({})),
            ("order not exists or too late to cancel", json!({})),
            ("params error", json!({})),
            (
                "OK",
                json!({"retExtInfo": {"list": [{"code": 110001, "msg": "order not exists"}]}}),
            ),
            (
                "OK",
                json!({"retExtInfo": {"list": [{"code": 10001, "msg": "bad order"}]}}),
            ),
        ] {
            cases.push(json!({"kind": "cancel", "parents": [], "orders": orders, "message": message, "extra": extra}));
        }
    }
    assert_eq!(cases.len(), 92, "independent order-write corpus floor");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/binding-order-fixtures.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), cases.len());
    for (index, (case, expected)) in cases.iter().zip(source).enumerate() {
        let h = harness(options(), |wire| {
            let mut response =
                json!({"retCode": 0, "retMsg": case["message"], "result": {"orderId": "amended"}});
            if let Some(extra) = case["extra"].as_object() {
                response.as_object_mut().unwrap().extend(extra.clone());
            }
            for endpoint in [
                "/v5/order/amend",
                "/v5/order/cancel-batch",
                "/v5/position/trading-stop",
            ] {
                wire.route("POST", endpoint, 200, response.clone());
            }
            wire.ok(
                "GET",
                "/v5/position/list",
                json!({"list": [], "nextPageCursor": ""}),
            );
        });
        seed_orders(&h, serde_json::from_value(case["parents"].clone()).unwrap());
        let result = if case["kind"] == "update" {
            update(
                &h,
                serde_json::from_value(case["options"]["order"].clone()).unwrap(),
                case["options"]["update"].clone(),
            )
            .map(|ids| json!(ids))
        } else {
            h.drive(
                h.venue()
                    .cancel_orders(serde_json::from_value(case["orders"].clone()).unwrap()),
            )
            .map(|_| serde_json::Value::Null)
        };
        let result = match result {
            Ok(value) => json!({"ok": value}),
            Err(error) => json!({"error": error.message}),
        };
        let calls: Vec<_> = h
            .wire
            .requests
            .borrow()
            .iter()
            .filter(|request| request.method == "POST")
            .map(|request| json!({"url": request.url, "body": request.body}))
            .collect();
        assert_eq!(
            json!({"calls": calls, "result": result}),
            expected,
            "source case {index}: {case}"
        );
    }
}

#[test]
fn demo_fill_reconciliation_pages_to_exhaustion_and_fails_closed() {
    let page =
        |rows: serde_json::Value, cursor: &str| json!({"list": rows, "nextPageCursor": cursor});
    let cases = [
        (
            vec![
                page(
                    json!([{"orderId": "ro-1", "execQty": "0.003"}, {"orderId": "other", "execQty": "100"}]),
                    "next",
                ),
                page(
                    json!([{"orderId": "ro-1", "closedSize": "0.007"}, {"orderId": "ro-1", "execQty": "-1"}, {"orderId": "ro-1", "execQty": "NaN"}]),
                    "",
                ),
            ],
            true,
        ),
        (
            vec![page(json!([{"orderId": "ro-1", "qty": "0.003"}]), "same")],
            false,
        ),
        (
            vec![page(
                json!([{"orderId": "other", "execQty": "100"}, {"orderId": "ro-1", "execQty": "0.003"}]),
                "",
            )],
            false,
        ),
        (
            (0..20)
                .map(|i| page(json!([]), &format!("cursor-{i}")))
                .collect(),
            false,
        ),
    ];
    for (pages, retired) in cases {
        let expected_calls = if pages.len() == 1 && pages[0]["nextPageCursor"] != "" {
            2
        } else {
            pages.len()
        };
        let h = support::harness_as(safe_cex_types::ExchangeName::Bybitdemo, options(), |wire| {
            wire.ok(
                "GET",
                "/v5/position/list",
                json!({"list": [], "nextPageCursor": ""}),
            );
            wire.ok("POST", "/v5/order/create", json!({"orderId": "ro-1"}));
            for page in pages {
                wire.ok("GET", "/v5/execution/list", page);
            }
        });
        seed_market(&h);
        place(
            &h,
            json!({"symbol": "BTCUSDT", "type": "limit", "side": "sell", "amount": 0.01,
                        "price": 26000, "reduceOnly": true}),
        )
        .unwrap();
        h.advance(1100.);
        let calls = h.wire.calls("/v5/execution/list");
        // The scripted limiter advances virtual time, so a long first read may
        // also fire the next reconciliation timer. Count the first read only.
        let first_read_len = calls
            .iter()
            .skip(1)
            .position(|call| call.params.get("cursor").is_none())
            .map_or(calls.len(), |index| index + 1);
        assert_eq!(
            first_read_len, expected_calls,
            "all pages read, repeated cursors stop"
        );
        assert_eq!(calls[0].params["symbol"], "BTCUSDT");
        assert_eq!(calls[0].params["limit"], "50");
        assert!(calls[0].params.get("cursor").is_none());
        if calls.len() > 1 {
            assert!(calls[1].params.get("cursor").is_some());
        }
        assert_eq!(
            !h.store.orders().iter().any(|order| order.id == "ro-1"),
            retired,
            "partial or truncated fill history cannot retire the order"
        );
        if expected_calls == 20 {
            assert!(
                h.runtime
                    .logs
                    .borrow()
                    .iter()
                    .any(|message| message.contains("exceeded 20 pages")),
                "a page limit is an observable error, never an empty fill result"
            );
        }
    }
}
