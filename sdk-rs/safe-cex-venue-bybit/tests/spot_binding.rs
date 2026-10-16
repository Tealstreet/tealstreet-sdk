//! Bybit Spot facade over its own source members and the real typed core.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::{ExchangeName, OrderSide, OrderType};
use serde_json::json;

fn scripted(wire: &support::Wire) {
    wire.ok(
        "GET",
        "/v5/market/instruments-info",
        json!({"list":[{
            "symbol":"BTCUSDT","baseCoin":"BTC","quoteCoin":"USDT","status":"Trading",
            "lotSizeFilter":{"qtyStep":"0.001","minOrderQty":"0.001","maxOrderQty":"100"},
            "priceFilter":{"tickSize":"0.1"}
        }]}),
    );
    wire.ok("GET", "/v5/market/tickers", json!({"list":[{
        "symbol":"BTCUSDT","lastPrice":"25000","bid1Price":"24999","ask1Price":"25001",
        "markPrice":"25000","indexPrice":"25000","price24hPcnt":"0.01","openInterest":"0","volume24h":"10"
    }]}));
    wire.ok(
        "GET",
        "/v5/user/query-api",
        json!({"userID":12345,"inviterID":678}),
    );
    wire.ok(
        "GET",
        "/v5/account/info",
        json!({"unifiedMarginStatus":6,"marginMode":"REGULAR_MARGIN"}),
    );
    wire.ok("GET", "/v5/account/wallet-balance", json!({"list":[{
        "accountType":"UNIFIED","totalEquity":"1000","totalWalletBalance":"1000","totalMaintenanceMargin":"0","totalInitialMargin":"0","totalAvailableBalance":"1000","totalPerpUPL":"0","coin":[]
    }]}));
    wire.ok("GET", "/v5/order/realtime", json!({"list":[{
        "orderId":"existing","orderLinkId":"","symbol":"BTCUSDT","side":"Buy","orderType":"Limit",
        "price":"25000","qty":"0.01","leavesQty":"0.01","cumExecQty":"0","orderStatus":"New",
        "timeInForce":"GTC","reduceOnly":false,"closeOnTrigger":false,"stopOrderType":"",
        "triggerPrice":"","stopLoss":"","takeProfit":"","tpslMode":"","createType":"CreateByUser","positionIdx":0
    }],"nextPageCursor":""}));
    wire.ok("POST", "/v5/order/create", json!({"orderId":"created"}));
    wire.ok("POST", "/v5/order/amend", json!({"orderId":"existing"}));
    wire.ok("POST", "/v5/order/cancel", json!({"orderId":"existing"}));
}

#[test]
fn spot_methods_use_spot_requests_and_inherited_refusals() {
    let h = support::harness_as(ExchangeName::Bybitspot, support::options(), scripted);
    assert_eq!(h.venue().class_name(), "BybitSpotExchange");
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].is_spot, Some(true));
    assert_eq!(markets[0].limits.leverage.max, 1.);
    assert_eq!(
        h.wire.calls("/v5/market/instruments-info")[0].params["category"],
        "spot"
    );
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let tickers = h.drive(h.venue().fetch_tickers()).unwrap();
    assert_eq!(tickers.len(), 1);
    assert_eq!(tickers[0].quote_volume, 250000.);
    assert_eq!(tickers[0].funding_rate, 0.);
    assert_eq!(
        h.wire.calls("/v5/market/tickers")[0].params["category"],
        "spot"
    );
    assert_eq!(h.drive(h.venue().get_account()).unwrap().user_id, "12345");
    assert_eq!(
        h.drive(h.venue().validate_account()).unwrap().error,
        "".into()
    );
    assert!(h.drive(h.venue().fetch_positions(None)).unwrap().is_empty());
    assert!(h.wire.calls("/v5/position/list").is_empty());
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "existing");
    assert_eq!(
        h.wire.calls("/v5/order/realtime")[0].params["category"],
        "spot"
    );
    let options = serde_json::from_value::<safe_cex_types::PlaceOrderOpts>(json!({
        "symbol":"BTCUSDT","side":"buy","type":"limit","amount":0.01,"price":25000,"timeInForce":"GoodTillCancel"
    })).unwrap();
    let venue = h.venue();
    let ids = h
        .drive(Box::pin(venue.base().place_order(venue, options)))
        .unwrap();
    assert_eq!(ids, ["created"]);
    let sent = h.wire.calls("/v5/order/create");
    assert_eq!(sent[0].body["category"], "spot");
    assert_eq!(sent[0].body["qty"], "0.01");
    assert_eq!(sent[0].body["orderType"], "Limit");
    assert_eq!(orders[0].side, OrderSide::Buy);
    assert_eq!(orders[0].r#type, OrderType::Limit);
    for result in [
        h.drive(h.venue().set_leverage("BTCUSDT".into(), 2., None)),
        h.drive(h.venue().change_position_mode(true, None)),
        h.drive(h.venue().change_margin_type(true, None)),
        h.drive(h.venue().schedule_cancel(None)),
    ] {
        assert_eq!(result.unwrap_err().message, "Not implemented");
    }
    h.venue().dispose().unwrap();
}

#[test]
fn public_only_spot_start_uses_spot_socket_and_skips_account_reads() {
    let mut options = support::options();
    options.public_data_only = Some(true);
    let h = support::harness_as(ExchangeName::Bybitspot, options, scripted);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    assert_eq!(h.store.markets().len(), 1);
    assert_eq!(h.store.tickers().len(), 1);
    let sockets = h.runtime.sockets.borrow();
    assert_eq!(sockets.len(), 1);
    assert!(
        sockets[0]
            .0
            .starts_with("wss://stream.bybit.com/v5/public/spot?requestId=")
    );
    assert!(h.wire.calls("/v5/account/info").is_empty());
    assert!(h.wire.calls("/v5/account/wallet-balance").is_empty());
    h.venue().dispose().unwrap();
}
