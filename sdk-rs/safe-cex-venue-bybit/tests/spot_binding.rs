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
    wire.ok("POST", "/v5/order/cancel-batch", json!({}));
}

#[test]
fn spot_methods_use_spot_requests_and_inherited_refusals() {
    let h = support::harness_as(ExchangeName::Bybitspot, support::options(), scripted);
    assert_eq!(h.venue().class_name(), "BybitSpotExchange");
    assert!(
        h.venue().base().is_spot,
        "Spot source flag reaches typed base"
    );
    assert!(!h.venue().base().is_demo);
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
    assert_eq!(
        h.drive(h.venue().validate_account())
            .unwrap()
            .balance
            .unwrap()
            .free,
        1000.
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
        .drive(Box::pin(venue.base().place_order(venue, options.clone())))
        .unwrap();
    assert_eq!(ids, ["created"]);
    let sent = h.wire.calls("/v5/order/create");
    assert_eq!(sent[0].body["category"], "spot");
    assert_eq!(sent[0].body["qty"], "0.01");
    assert_eq!(sent[0].body["orderType"], "Limit");
    assert_eq!(orders[0].side, OrderSide::Buy);
    assert_eq!(orders[0].r#type, OrderType::Limit);
    let batch = h
        .drive(Box::pin(
            venue
                .base()
                .place_orders(venue, vec![options.clone(), options]),
        ))
        .unwrap();
    assert_eq!(batch, ["created", "created"]);
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            orders: Some(orders.clone()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let amended = h
        .drive(
            h.venue().update_order(
                serde_json::from_value(json!({
                    "order":orders[0], "update":{"price":25100}
                }))
                .unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(amended, ["existing"]);
    assert_eq!(h.wire.calls("/v5/order/amend")[0].body["category"], "spot");
    h.drive(h.venue().cancel_orders(orders)).unwrap();
    assert_eq!(
        h.wire.calls("/v5/order/cancel-batch")[0].body["category"],
        "spot"
    );
    assert_eq!(
        h.store.orders().len(),
        1,
        "default route waits for private WS to retire an order"
    );
    assert_eq!(
        h.drive(h.venue().fetch_symbol_max_leverage("BTCUSDT".into()))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(
        h.drive(h.venue().fetch_public_trades("BTCUSDT".into(), None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
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

#[test]
fn private_spot_start_publishes_snapshot_and_subscribes_only_to_orders() {
    let h = support::harness_as(ExchangeName::Bybitspot, support::options(), scripted);
    h.drive(h.venue().start()).unwrap();
    h.advance(2000.);
    let loaded = h.store.loaded().clone();
    assert!(
        loaded.markets && loaded.tickers && loaded.balance && loaded.positions && loaded.orders,
        "Spot snapshot: {loaded:?}; logs={:?}",
        h.runtime.logs.borrow()
    );
    assert_eq!(h.store.orders()[0].id, "existing");
    assert!(h.store.positions().is_empty());
    let sockets = h.runtime.sockets.borrow();
    assert_eq!(sockets.len(), 2);
    let private = &sockets
        .iter()
        .find(|(url, _)| url.contains("/v5/private"))
        .unwrap()
        .1;
    let frames: Vec<serde_json::Value> = private
        .sent
        .borrow()
        .iter()
        .map(|frame| serde_json::from_str(frame).unwrap())
        .collect();
    let auth = frames
        .iter()
        .position(|frame| frame["op"] == "auth")
        .unwrap();
    let subscribe = frames
        .iter()
        .position(|frame| frame["op"] == "subscribe")
        .unwrap();
    assert!(auth < subscribe);
    assert_eq!(frames[subscribe]["args"], json!(["order"]));
    assert!(h.wire.calls("/v5/position/list").is_empty());
    h.venue().dispose().unwrap();
    assert!(sockets.iter().all(|(_, socket)| socket.closed.get()));
}

#[test]
fn brazil_spot_cancel_retires_the_local_order_fallback() {
    let mut options = support::options();
    options.bybit_site_id = Some(safe_cex_types::ExchangeOptionsBybitSiteId::BraBtl);
    let h = support::harness_as(ExchangeName::Bybitspot, options, scripted);
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            orders: Some(orders.clone()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h.drive(h.venue().cancel_orders(orders)).unwrap();
    assert!(h.store.orders().is_empty());
    h.venue().dispose().unwrap();
}

#[test]
fn spot_caller_id_is_verbatim_and_rejected_before_writes_when_invalid_or_split() {
    let h = support::harness_as(ExchangeName::Bybitspot, support::options(), scripted);
    let market = h.drive(h.venue().fetch_markets()).unwrap();
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(market),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let order: safe_cex_types::PlaceOrderOpts = serde_json::from_value(json!({
        "symbol":"BTCUSDT", "side":"buy", "type":"limit", "amount":0.01,
        "price":25000, "clientOrderId":"Caller_A-17"
    }))
    .unwrap();
    let venue = h.venue();
    h.drive(Box::pin(venue.base().place_order(venue, order.clone())))
        .unwrap();
    assert_eq!(
        h.wire.calls("/v5/order/create")[0].body["orderLinkId"],
        "Caller_A-17"
    );
    let count = h.wire.requests.borrow().len();
    for (id, amount) in [("invalid id", 0.01), ("Caller_A-17", 201.)] {
        let mut input = order.clone();
        input.client_order_id = Some(id.into());
        input.amount = amount;
        assert!(
            h.drive(Box::pin(venue.base().place_order(venue, input)))
                .is_err()
        );
        assert_eq!(
            h.wire.requests.borrow().len(),
            count,
            "invalid or split caller ID must write nothing"
        );
    }
    h.venue().dispose().unwrap();
}

#[test]
fn spot_private_frames_deliver_live_orders_and_fills_without_positions() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, rc::Rc};
    let h = support::harness_as(ExchangeName::Bybitspot, support::options(), scripted);
    h.drive(h.venue().start()).unwrap();
    h.advance(2000.);
    let private = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("/v5/private"))
        .unwrap()
        .1
        .clone();
    let fills = Rc::new(RefCell::new(vec![]));
    let observed = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, value| {
            let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
            observed.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let mut row = json!({"orderId":"spot-live","orderLinkId":"SpotClient1","symbol":"BTCUSDT","side":"Buy","orderType":"Limit",
        "price":"25000","qty":"0.01","leavesQty":"0.01","cumExecQty":"0","orderStatus":"New","timeInForce":"GTC",
        "reduceOnly":false,"closeOnTrigger":false,"stopOrderType":"","triggerPrice":"","stopLoss":"","takeProfit":"","tpslMode":"","positionIdx":0});
    private
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"topic":"order","data":[row.clone()]}).to_string(),
        )));
    h.runtime.pump();
    assert!(
        h.store.orders().iter().any(|o| o.id == "spot-live"),
        "Spot private order reaches the store"
    );
    row["orderStatus"] = json!("PartiallyFilled");
    row["cumExecQty"] = json!("0.004");
    row["cumExecValue"] = json!("100");
    row["avgPrice"] = json!("25000");
    for _ in 0..2 {
        private
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(
                json!({"topic":"order","data":[row.clone()]}).to_string(),
            )));
        h.runtime.pump();
    }
    assert_eq!(
        *fills.borrow(),
        vec![
            json!({"id":"spot-live","side":"buy","symbol":"BTCUSDT","price":25000.0,"amount":0.004})
        ],
        "Spot private replay emits one execution delta"
    );
    private
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"topic":"position","data":[{"symbol":"BTCUSDT","size":"10"}]}).to_string(),
        )));
    h.runtime.pump();
    assert!(
        h.store.positions().is_empty(),
        "Spot source has no position handler"
    );
    h.venue().dispose().unwrap();
    assert!(private.closed.get());
}

#[test]
fn split_spot_orders_omit_brackets_only_after_the_first_lot() {
    let h = support::harness_as(ExchangeName::Bybitspot, support::options(), scripted);
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let options: safe_cex_types::PlaceOrderOpts = serde_json::from_value(json!({"symbol":"BTCUSDT","side":"buy","type":"limit","amount":200,"price":25000,"stopLoss":24000,"takeProfit":26000})).unwrap();
    let ids = h
        .drive(Box::pin(h.venue().base().place_order(h.venue(), options)))
        .expect("real Spot split placement");
    assert_eq!(ids, ["created", "created"]);
    let calls = h.wire.calls("/v5/order/create");
    assert_eq!(calls.len(), 2);
    for call in &calls {
        assert_eq!(call.body["qty"], "100");
        assert_eq!(call.body["price"], "25000");
        assert_eq!(call.body["category"], "spot");
    }
    assert_eq!(calls[0].body["stopLoss"], "24000");
    assert_eq!(calls[0].body["takeProfit"], "26000");
    for field in ["stopLoss", "takeProfit", "slTriggerBy", "tpTriggerBy"] {
        assert!(
            calls[1].body.get(field).is_none(),
            "second lot must omit {field}"
        );
    }
    h.venue().dispose().unwrap();
}

#[test]
fn trailing_order_without_ticker_keeps_the_source_error() {
    let h = support::harness_as(ExchangeName::Bybitspot, support::options(), scripted);
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let options = serde_json::from_value::<safe_cex_types::PlaceOrderOpts>(json!({
        "symbol":"BTCUSDT", "side":"sell", "type":"trailing_stop_market", "amount":0.01, "price":24000
    })).unwrap();
    let error = h
        .drive(Box::pin(h.venue().base().place_order(h.venue(), options)))
        .expect_err("source rejects missing ticker before sending a trailing order");
    assert_eq!(error.name, "Error");
    assert_eq!(
        error.message, "Ticker BTCUSDT not found",
        "actual Spot binding keeps the source error"
    );
    assert!(h.wire.calls("/v5/position/trading-stop").is_empty());
    assert!(h.wire.calls("/v5/order/create").is_empty());
    h.venue().dispose().unwrap();
}
