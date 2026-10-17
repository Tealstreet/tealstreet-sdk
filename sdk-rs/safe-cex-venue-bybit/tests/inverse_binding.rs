//! The standalone Inverse facade over real options, core and signed transport.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, MarginType, PlaceOrderOpts};
use serde_json::{Value, json};

fn position(leverage: &str) -> Value {
    json!({"symbol":"BTCUSD","side":"Buy","size":"1000","avgPrice":"20000","positionValue":"0.05",
        "leverage":leverage,"unrealisedPnl":"0.01","curRealisedPnl":"0.001","liqPrice":"10000",
        "positionIdx":0,"positionIM":"0.005","updatedTime":"1","seq":1,"positionStatus":"Normal"})
}
fn order() -> Value {
    json!({"orderId":"existing","orderLinkId":"","symbol":"BTCUSD","side":"Buy","orderType":"Limit",
        "price":"20000","qty":"1000","leavesQty":"500","cumExecQty":"500","orderStatus":"New",
        "timeInForce":"GTC","reduceOnly":false,"closeOnTrigger":false,"stopOrderType":"","triggerPrice":"",
        "stopLoss":"","takeProfit":"","tpslMode":"","createType":"CreateByUser","positionIdx":0})
}
fn scripted(w: &support::Wire) {
    w.ok("GET","/v5/market/instruments-info",json!({"list":[{
        "symbol":"BTCUSD","contractType":"InversePerpetual","status":"Trading","baseCoin":"BTC","quoteCoin":"USD","settleCoin":"BTC","deliveryTime":"0",
        "priceFilter":{"tickSize":"0.5","minPrice":"0.5","maxPrice":"999999"},
        "lotSizeFilter":{"qtyStep":"1","minOrderQty":"1","maxOrderQty":"1000"},
        "leverageFilter":{"minLeverage":"1","maxLeverage":"100","leverageStep":"1"}
    }],"nextPageCursor":""}));
    w.route_when(
        "GET",
        "/v5/market/instruments-info",
        "status",
        "PreLaunch",
        json!({"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}),
    );
    w.ok("GET","/v5/market/tickers",json!({"list":[{
        "symbol":"BTCUSD","lastPrice":"20000","bid1Price":"19999.5","ask1Price":"20000.5","markPrice":"20000","indexPrice":"20010",
        "price24hPcnt":"0.01","openInterest":"1000000","fundingRate":"0.0001","nextFundingTime":"1700003600000","volume24h":"10","turnover24h":"200000"
    }]}));
    w.ok("GET", "/v5/account/fee-rate", json!({"list":[]}));
    w.ok(
        "GET",
        "/v5/user/query-api",
        json!({"userID":12345,"inviterID":678,"vipLevel":"No VIP","readOnly":0}),
    );
    w.ok(
        "GET",
        "/v5/account/info",
        json!({"unifiedMarginStatus":6,"marginMode":"REGULAR_MARGIN"}),
    );
    w.ok("GET","/v5/account/wallet-balance",json!({"list":[{
        "accountType":"UNIFIED","totalEquity":"1000","totalWalletBalance":"1000","totalMaintenanceMargin":"0","totalInitialMargin":"0","totalAvailableBalance":"1000","totalPerpUPL":"0","coin":[]
    }]}));
    w.ok(
        "GET",
        "/v5/order/realtime",
        json!({"list":[order()],"nextPageCursor":""}),
    );
    w.ok(
        "GET",
        "/v5/position/list",
        json!({"list":[position("10")],"nextPageCursor":""}),
    );
    w.ok("POST", "/v5/order/create", json!({"orderId":"created"}));
    w.ok(
        "POST",
        "/v5/order/create-batch",
        json!({"list":[{"orderId":"batch-created"}]}),
    );
    w.ok("POST", "/v5/order/amend", json!({"orderId":"existing"}));
    w.ok("POST", "/v5/order/cancel-batch", json!({}));
    w.ok("POST", "/v5/position/set-leverage", json!({}));
    w.route(
        "POST",
        "/v5/position/switch-mode",
        200,
        json!({"retCode":0,"retMsg":"All symbols switched successfully.","result":{}}),
    );
    w.route(
        "POST",
        "/v5/account/set-margin-mode",
        200,
        json!({"retCode":0,"retMsg":"Request accepted","result":{"reasons":[]}}),
    );
    w.ok("POST", "/v5/position/trading-stop", json!({}));
}
fn harness() -> support::Harness {
    support::harness_as(ExchangeName::Bybitinverse, support::options(), scripted)
}
fn seed(h: &support::Harness) {
    let markets = h.drive(h.venue().fetch_markets()).expect("Inverse markets");
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let tickers = h.drive(h.venue().fetch_tickers()).expect("Inverse tickers");
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}
fn opts() -> PlaceOrderOpts {
    serde_json::from_value(
        json!({"symbol":"BTCUSD","type":"limit","side":"buy","amount":12.6,"price":20000}),
    )
    .unwrap()
}

#[test]
fn inverse_constructs_with_its_own_identity_and_reads() {
    let h = harness();
    assert_eq!(h.venue().class_name(), "BybitInverseExchange");
    assert!(!h.venue().base().is_spot && !h.venue().base().is_demo);
    assert!(
        h.wire.requests.borrow().is_empty(),
        "constructor does not send or dial"
    );
    assert!(h.runtime.sockets.borrow().is_empty());
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(
        markets.len(),
        1,
        "inverse Trading plus empty PreLaunch reads"
    );
    assert_eq!(markets[0].symbol, "BTCUSD");
    assert_eq!(markets[0].quote, "USD");
    assert_eq!(markets[0].is_inverse, Some(true));
    assert_eq!(markets[0].precision.amount, 1.);
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let tickers = h.drive(h.venue().fetch_tickers()).unwrap();
    assert_eq!(tickers.len(), 1);
    assert_eq!(tickers[0].last, 20000.);
    assert_eq!(tickers[0].percentage, 1.);
    assert_eq!(tickers[0].quote_volume, 200000.);
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    assert_eq!(
        positions.len(),
        1,
        "one inverse category without a settle coin"
    );
    assert_eq!(positions[0].contracts, 0.05);
    assert_eq!(positions[0].notional, 1000.);
    assert_eq!(positions[0].unrealized_pnl, 0.01);
    assert_eq!(positions[0].realized_pnl, Some(0.001));
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "existing");
    assert_eq!(orders[0].remaining, 0.025);
    assert_eq!(
        orders[0].extra.as_ref().unwrap().remaining_native,
        Some(500.)
    );
    assert_eq!(
        orders[0].extra.as_ref().unwrap().remaining_notional,
        Some(500.)
    );
    assert_eq!(h.drive(h.venue().get_account()).unwrap().user_id, "12345");
    let validation = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(validation.error, "".into());
    assert_eq!(validation.balance.unwrap().free, 1000.);
    for path in [
        "/v5/market/instruments-info",
        "/v5/market/tickers",
        "/v5/order/realtime",
        "/v5/position/list",
    ] {
        for call in h.wire.calls(path) {
            assert_eq!(call.params["category"], "inverse", "{path}");
            assert!(
                call.params.get("settleCoin").is_none(),
                "source inverse settle coin is undefined: {call:?}"
            );
        }
    }
    h.venue().dispose().unwrap();
    assert!(h.venue().base().is_disposed());
}

#[test]
fn inverse_single_and_batch_writes_use_integer_usd_contracts() {
    let h = harness();
    seed(&h);
    let venue = h.venue();
    let ids = h
        .drive(Box::pin(venue.base().place_order(venue, opts())))
        .unwrap();
    assert_eq!(ids, ["created"]);
    let sent = h.wire.calls("/v5/order/create");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].body["category"], "inverse");
    assert_eq!(sent[0].body["qty"], "13");
    assert_eq!(sent[0].body["price"], "20000");
    assert!(
        sent[0].body.get("orderLinkId").is_none(),
        "no generated inverse client id"
    );
    let cached = h.drive(h.venue().fetch_orders(None)).unwrap();
    let created = cached
        .iter()
        .find(|o| o.id == "created")
        .expect("inverse recent-order shield");
    assert_eq!(created.remaining, 0.00065);
    assert_eq!(
        created.extra.as_ref().unwrap().remaining_notional,
        Some(13.)
    );
    let ids = h
        .drive(Box::pin(venue.base().place_orders(venue, vec![opts()])))
        .unwrap();
    assert_eq!(ids, ["batch-created"]);
    assert_eq!(
        h.wire.calls("/v5/order/create-batch")[0].body["request"][0]["qty"],
        "13"
    );
    h.venue().dispose().unwrap();
}

#[test]
fn inverse_controls_preserve_native_positions_and_source_mode_policy() {
    let h = harness();
    seed(&h);
    h.drive(h.venue().set_leverage("BTCUSD".into(), 11., None))
        .unwrap();
    let posts = h.wire.calls("/v5/position/set-leverage");
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].body["category"], "inverse");
    assert_eq!(posts[0].body["buyLeverage"], "11");
    h.drive(h.venue().change_position_mode(true, Some("BTCUSD".into())))
        .unwrap();
    let posts = h.wire.calls("/v5/position/switch-mode");
    assert_eq!(posts.len(), 1);
    assert_eq!(
        posts[0].body,
        json!({"category":"linear","coin":"USDT","mode":3}),
        "source ignores the supplied inverse symbol"
    );
    h.drive(h.venue().change_margin_type(true, Some("BTCUSD".into())))
        .unwrap();
    assert_eq!(
        h.wire.calls("/v5/account/set-margin-mode")[0].body["setMarginMode"],
        "ISOLATED_MARGIN"
    );
    assert_eq!(
        h.store.positions()[0].margin_type,
        Some(MarginType::Isolated)
    );
    h.venue().dispose().unwrap();
}

#[test]
fn inverse_amend_and_cancel_keep_usd_native_remaining() {
    let mut options = support::options();
    options.bybit_site_id = Some(safe_cex_types::ExchangeOptionsBybitSiteId::BraBtl);
    let h = support::harness_as(ExchangeName::Bybitinverse, options, scripted);
    seed(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            orders: Some(orders.clone()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let ids = h
        .drive(h.venue().update_order(
            serde_json::from_value(json!({"order":orders[0],"update":{"price":25000}})).unwrap(),
        ))
        .unwrap();
    assert_eq!(ids, ["existing"]);
    assert_eq!(
        h.wire.calls("/v5/order/amend")[0].body,
        json!({"category":"inverse","orderId":"existing","symbol":"BTCUSD","price":"25000"})
    );
    let orders = h.store.orders().to_vec();
    assert_eq!(
        orders[0].remaining, 0.02,
        "500 native USD contracts at amended price"
    );
    assert_eq!(
        orders[0].extra.as_ref().unwrap().remaining_notional,
        Some(500.)
    );
    h.drive(h.venue().cancel_orders(orders)).unwrap();
    assert_eq!(
        h.wire.calls("/v5/order/cancel-batch")[0].body,
        json!({"category":"inverse","request":[{"symbol":"BTCUSD","orderId":"existing"}]})
    );
    assert!(
        h.store.orders().is_empty(),
        "Brazil source retires local inverse orders"
    );
    h.venue().dispose().unwrap();
}

#[test]
fn inverse_start_owns_only_dedicated_public_and_authenticated_private_sockets() {
    use safe_cex_exchange::socket_lifecycle::SocketSlot;
    let h = harness();
    h.drive(h.venue().start()).unwrap();
    h.advance(2000.);
    let loaded = h.store.loaded().clone();
    assert!(
        loaded.markets && loaded.tickers && loaded.orders && loaded.positions && loaded.balance,
        "inverse snapshot loaded: {loaded:?}; {:?}",
        h.runtime.logs.borrow()
    );
    assert!(
        h.venue().base().get_socket(SocketSlot::Public).is_none(),
        "standalone inverse has no linear public socket"
    );
    assert!(
        h.venue()
            .base()
            .get_socket(SocketSlot::PublicSecondary)
            .is_some()
    );
    let sockets = h.runtime.sockets.borrow().clone();
    assert_eq!(sockets.len(), 2, "only inverse public and private sockets");
    assert!(
        sockets
            .iter()
            .any(|(url, _)| url.contains("/v5/public/inverse?"))
    );
    assert!(
        !sockets
            .iter()
            .any(|(url, _)| url.contains("/v5/public/linear"))
    );
    let private = &sockets
        .iter()
        .find(|(url, _)| url.contains("/v5/private"))
        .unwrap()
        .1;
    let frames: Vec<Value> = private
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
    assert!(auth < subscribe, "source authenticates before subscribing");
    assert_eq!(frames[subscribe]["args"], json!(["order", "position"]));
    let weak = std::rc::Rc::downgrade(&h.exchange);
    h.venue().dispose().unwrap();
    assert!(
        sockets.iter().all(|(_, socket)| socket.closed.get()),
        "inverse source disposes both sockets"
    );
    h.runtime.pump();
    drop(h);
    assert!(
        weak.upgrade().is_none(),
        "disposed inverse facade releases its ownership"
    );
}

#[test]
fn inverse_native_nan_mode_and_strict_not_modified_policy() {
    for leverage in ["NaN", "-0"] {
        let h = support::harness_as(ExchangeName::Bybitinverse, support::options(), |wire| {
            scripted(wire);
            wire.routes.borrow_mut().retain(|(_, path, _)| {
                path != "/v5/position/list" && path != "/v5/position/set-leverage"
            });
            wire.ok(
                "GET",
                "/v5/position/list",
                json!({"list":[position(leverage)],"nextPageCursor":""}),
            );
            wire.route(
                "POST",
                "/v5/position/set-leverage",
                200,
                json!({"retCode":110043,"retMsg":"not modified"}),
            );
        });
        seed(&h);
        let venue = h.venue();
        h.drive(Box::pin(venue.base().place_order(venue, opts())))
            .unwrap();
        let actual = h.store.positions()[0].leverage;
        if leverage == "NaN" {
            assert!(actual.is_nan(), "inverse mode publication retains NaN");
        } else {
            assert_eq!(
                actual.to_bits(),
                (-0.0f64).to_bits(),
                "inverse mode publication retains -0"
            );
        }
        let error = h
            .drive(h.venue().set_leverage("BTCUSD".into(), 1., None))
            .unwrap_err();
        assert_eq!(
            error.message, "not modified",
            "inverse rejects the primary not-modified exemption"
        );
        assert_eq!(h.wire.calls("/v5/position/set-leverage").len(), 1);
        h.venue().dispose().unwrap();
    }
}

#[test]
fn inverse_caller_id_guard_and_source_inherited_refusals() {
    let h = harness();
    seed(&h);
    let mut order = opts();
    order.client_order_id = Some("invalid/client/id".into());
    let venue = h.venue();
    let error = h
        .drive(Box::pin(venue.base().place_order(venue, order)))
        .unwrap_err();
    assert!(
        error.message.contains("Bybit inverse"),
        "dedicated source client-ID rule label: {error:?}"
    );
    assert!(
        h.wire.calls("/v5/order/create").is_empty(),
        "client ID rejects before physical writes"
    );
    for error in [
        h.drive(venue.fetch_symbol_max_leverage("BTCUSD".into()))
            .map(|_| ())
            .unwrap_err(),
        h.drive(venue.schedule_cancel(Some(Some(1.)))).unwrap_err(),
        h.drive(venue.fetch_public_trades("BTCUSD".into(), None))
            .map(|_| ())
            .unwrap_err(),
    ] {
        assert_eq!(
            error.message, "Not implemented",
            "exact inherited BaseExchange refusal"
        );
    }
    h.venue().dispose().unwrap();
}

#[test]
fn inverse_public_topics_follow_active_and_market_callbacks_on_the_secondary_slot() {
    use safe_cex_exchange::socket_lifecycle::SocketSlot;
    let h = harness();
    seed(&h);
    let mut markets = h.store.markets().to_vec();
    let mut second = markets[0].clone();
    second.id = "ETHUSD".into();
    second.symbol = "ETHUSD".into();
    second.base = "ETH".into();
    markets.push(second);
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h.venue()
        .base()
        .set_active_ticker_symbols(&["BTCUSD".into()])
        .unwrap();
    let socket = h
        .venue()
        .base()
        .get_socket(SocketSlot::PublicSecondary)
        .unwrap();
    socket.connect_and_subscribe(false).unwrap();
    h.runtime.pump();
    h.advance(100.);
    let wire = h.runtime.sockets.borrow()[0].1.clone();
    let subscriptions = || {
        wire.sent
            .borrow()
            .iter()
            .filter_map(|frame| {
                let frame: Value = serde_json::from_str(frame).unwrap();
                (frame["op"] == "subscribe").then(|| frame["args"].clone())
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(subscriptions(), [json!(["tickers.BTCUSD"])]);
    h.venue()
        .base()
        .set_active_ticker_symbols(&["ETHUSD".into()])
        .unwrap();
    assert_eq!(
        subscriptions().last().unwrap(),
        &json!(["tickers.ETHUSD"]),
        "dedicated inverse active callback uses its secondary socket"
    );
    h.venue().base().emit("markets:changed",safe_cex_exchange::js_runtime::value::from_json(&json!({"added":[{"symbol":"DOGEUSD","quote":"USD"},{"symbol":"DOGEUSD","quote":"USD"}],"removed":[]}))).unwrap();
    assert_eq!(
        subscriptions().last().unwrap(),
        &json!(["tickers.DOGEUSD", "tickers.DOGEUSD"]),
        "source market callback preserves duplicates"
    );
    h.advance(100.);
    h.venue().dispose().unwrap();
    let before = subscriptions();
    h.venue()
        .base()
        .set_active_ticker_symbols(&["BTCUSD".into()])
        .unwrap();
    assert_eq!(
        subscriptions(),
        before,
        "source disposal removes active ticker callback"
    );
}

#[test]
fn inverse_private_socket_delivers_native_contracts_and_fill_prices() {
    use safe_cex_exchange::{socket_lifecycle::SocketSlot, websocket::WsEvent};
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, rc::Rc};
    let h = harness();
    seed(&h);
    let fills = Rc::new(RefCell::new(Vec::<Value>::new()));
    let sink = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, value| {
            let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
            sink.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let socket = h.venue().base().get_socket(SocketSlot::Private).unwrap();
    socket.connect_and_subscribe(false).unwrap();
    h.runtime.pump();
    h.advance(1000.);
    let wire = h.runtime.sockets.borrow()[0].1.clone();
    let feed = |frame: Value| {
        wire.events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
        h.runtime.pump();
    };
    let mut fill = order();
    fill["orderStatus"] = json!("PartiallyFilled");
    fill["cumExecQty"] = json!("500");
    fill["cumExecValue"] = json!("0.025");
    fill["avgPrice"] = json!("20000");
    fill["rejectReason"] = json!("EC_NoError");
    feed(json!({"topic":"order","data":[fill.clone()]}));
    assert_eq!(h.store.orders().len(), 1);
    assert_eq!(h.store.orders()[0].remaining, 0.025);
    assert_eq!(
        fills.borrow().len(),
        1,
        "inverse order topic publishes a fill"
    );
    assert_eq!(
        fills.borrow()[0]["price"],
        20000.,
        "inverse fill uses cumulative qty divided by BTC value"
    );
    feed(json!({"topic":"order","data":[fill.clone()]}));
    assert_eq!(
        fills.borrow().len(),
        1,
        "inverse duplicate cumulative update emits no fill"
    );
    fill["cumExecQty"] = json!("750");
    fill["cumExecValue"] = json!("0.035");
    fill["avgPrice"] = json!("19000");
    feed(json!({"topic":"order","data":[fill]}));
    assert_eq!(
        fills.borrow().len(),
        2,
        "advancing cumulative update emits one delta fill"
    );
    assert_eq!(
        fills.borrow()[1]["amount"],
        250.0,
        "inverse incremental USD quantity"
    );
    assert_eq!(
        fills.borrow()[1]["price"],
        25000.0,
        "inverse price uses incremental qty/value, not averages"
    );
    feed(json!({"topic":"position","data":[position("NaN")]}));
    assert!(
        h.store.positions().is_empty(),
        "source updatePositions does not insert before the initial snapshot"
    );
    let positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            positions: Some(positions),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    feed(json!({"topic":"position","data":[position("NaN")]}));
    assert_eq!(h.store.positions().len(), 1);
    assert!(h.store.positions()[0].leverage.is_nan());
    assert_eq!(h.store.positions()[0].contracts, 0.05);
    assert_eq!(h.store.positions()[0].notional, 1000.);
    let count = fills.borrow().len();
    feed(json!({"topic":"execution","data":[{}]}));
    feed(json!({"topic":"wallet","data":[{}]}));
    assert_eq!(
        fills.borrow().len(),
        count,
        "maintained source ignores execution and wallet topics"
    );
    h.venue().dispose().unwrap();
    assert!(wire.closed.get());
}
