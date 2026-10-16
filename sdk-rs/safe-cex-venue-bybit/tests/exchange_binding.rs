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
    let coins: Vec<_> = calls.iter().map(|c| c.params["settleCoin"].clone()).collect();
    assert_eq!(coins, [json!("USDT"), json!("USDC")], "one read per UNIFIED settle coin");
    assert!(
        calls.iter().all(|c| c.params["category"] == "linear" && c.params["limit"] == "50"),
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
    assert_eq!(orders[1].side, OrderSide::Sell, "virtual stop closes the parent side");
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
    assert_eq!(positions.len(), 2, "one row per settle coin, flattened: {positions:?}");
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
