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
