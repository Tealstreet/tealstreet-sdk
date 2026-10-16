//! E2E: the production KrakenExchange through the Exchange trait over a scripted transport.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::{
    ExchangeName, Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, UpdateOrderOpts,
};
use serde_json::{Value, json};
use support::{Harness, harness, options};

const SYMBOL: &str = "PF_XBTUSD";

fn instruments() -> Value {
    json!({"result": "success", "instruments": [{
        "symbol": SYMBOL, "tradeable": true, "base": "BTC", "quote": "USD",
        "contractValueTradePrecision": 4, "tickSize": 0.5, "contractSize": 1,
        "marginLevels": [{"initialMargin": 0.02}], "contractType": "flexible_futures",
        "status": "open"
    }]})
}

fn tickers() -> Value {
    json!({"result": "success", "tickers": [{
        "symbol": SYMBOL, "bid": 30000, "ask": 30001, "last": 30000.5,
        "markPrice": 30000.25, "indexPrice": 30000.1, "vol24h": 12, "openInterest": 7,
        "fundingRate": 0.0001, "change24h": 1.5
    }]})
}

fn accounts() -> Value {
    json!({"result": "success", "accounts": {"flex": {
        "type": "multiCollateralMarginAccount", "portfolioValue": 1000, "availableMargin": 800,
        "totalUnrealized": 5, "currencies": {"USD": {"quantity": 1000, "value": 1000, "available": 800}}
    }}})
}

fn open_orders() -> Value {
    json!({"result": "success", "openOrders": [{
        "order_id": "o-1", "symbol": SYMBOL, "side": "buy", "orderType": "lmt",
        "limitPrice": 29000, "unfilledSize": 2, "filledSize": 0, "reduceOnly": false,
        "lastUpdateTime": "2023-11-14T22:13:20.000Z"
    }]})
}

fn open_positions() -> Value {
    json!({"result": "success", "openPositions": [{
        "symbol": SYMBOL, "side": "long", "size": 2, "price": 30000, "unrealizedPnl": 5
    }]})
}

fn futures(script: impl FnOnce(&support::Http)) -> Harness {
    harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", "/v3/instruments", instruments());
        http.ok("GET", "/v3/tickers", tickers());
        script(http);
    })
}

/// Markets and tickers loaded into the Store, as start() leaves them.
fn loaded(h: &Harness) {
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    let tickers = h.drive(h.venue().fetch_tickers()).expect("fetchTickers");
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .expect("store update");
}

fn order(opts: Value) -> PlaceOrderOpts {
    serde_json::from_value(opts).expect("place order opts")
}

fn data(request: &support::Request) -> String {
    request.data.as_str().unwrap_or_default().to_owned()
}

#[test]
fn fetch_markets_maps_perpetual_instruments() {
    let h = futures(|_| {});
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    assert_eq!(markets.len(), 1);
    let m = &markets[0];
    assert_eq!(m.symbol, SYMBOL);
    assert_eq!(m.id, "BTC/USD");
    assert_eq!(m.limits.leverage.max, 50.0, "1 / initialMargin");
    assert_eq!(h.http.calls("/v3/instruments").len(), 1);
}

#[test]
fn fetch_tickers_reads_the_rest_snapshot_for_known_markets() {
    let h = futures(|_| {});
    loaded(&h);
    let tickers = h.drive(h.venue().fetch_tickers()).expect("fetchTickers");
    assert_eq!(tickers.len(), 1);
    assert_eq!(tickers[0].symbol, SYMBOL);
    assert_eq!(tickers[0].mark, 30000.25);
    assert_eq!(tickers[0].index, 30000.1);
}

#[test]
fn private_reads_sign_and_map_orders_positions_and_balance() {
    let h = futures(|http| {
        http.ok("GET", "/v3/openorders", open_orders());
        http.ok("GET", "/v3/openpositions", open_positions());
    });
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "o-1");
    assert_eq!(orders[0].side, OrderSide::Buy);
    assert_eq!(orders[0].price, 29000.0);
    let signed = &h.http.calls("/v3/openorders")[0];
    assert_eq!(signed.headers["APIKey"], "fixture-key");
    assert!(
        signed.headers["Authent"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    assert!(signed.headers["Nonce"].as_str().is_some());
    assert_eq!(signed.method, "GET");
    assert_eq!(
        signed.base_url,
        "https://futures.kraken.com/derivatives/api"
    );
    assert!(
        signed.params.is_null(),
        "a signed GET carries no axios params"
    );

    let positions = h
        .drive(h.venue().fetch_positions(None))
        .expect("fetchPositions");
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, SYMBOL);
    assert_eq!(positions[0].contracts, 2.0);
    assert_eq!(positions[0].entry_price, 30000.0);
}

#[test]
fn validate_account_returns_no_error_and_a_balance() {
    let h = futures(|http| http.ok("GET", "/v3/accounts", accounts()));
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount");
    assert_eq!(result.error.as_value(), &json!(""));
    assert!(result.balance.is_some(), "best-effort balance present");
}

#[test]
fn validate_account_maps_an_auth_error_envelope() {
    let h = futures(|http| {
        http.ok(
            "GET",
            "/v3/accounts",
            json!({"result": "error", "error": "apiKey invalid"}),
        )
    });
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount");
    assert!(
        result
            .error
            .as_value()
            .as_str()
            .unwrap_or("")
            .contains("Authentication failed"),
        "{:?}",
        result.error
    );
}

#[test]
fn place_order_posts_a_signed_form_and_returns_the_venue_id() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/sendorder",
            json!({"result": "success", "sendStatus": {"status": "placed", "order_id": "new-1"}}),
        )
    });
    loaded(&h);
    let ids = h
        .drive(Box::pin(safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(
            h.venue(),
            order(json!({"symbol": SYMBOL, "type": "limit", "side": "buy", "amount": 1, "price": 30000})),
        )))
        .expect("placeOrder");
    assert_eq!(ids, vec!["new-1"]);
    let sent = &h.http.calls("/v3/sendorder")[0];
    let body = data(sent);
    assert!(body.contains("orderType=lmt"), "{body}");
    assert!(body.contains("limitPrice=30000"), "{body}");
    assert!(body.contains("symbol=PF_XBTUSD"), "{body}");
    assert_eq!(
        sent.headers["Content-Type"],
        "application/x-www-form-urlencoded"
    );
}

#[test]
fn place_order_refuses_a_caller_client_order_id_before_sending() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/sendorder",
            json!({"result": "success", "sendStatus": {"status": "placed", "order_id": "x"}}),
        )
    });
    loaded(&h);
    let result = h.drive(Box::pin(safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(
        h.venue(),
        order(json!({"symbol": SYMBOL, "type": "limit", "side": "buy", "amount": 1, "price": 30000, "clientOrderId": "mine"})),
    )));
    assert!(result.is_err(), "Kraken has no client-order-ID rule");
    assert!(h.http.calls("/v3/sendorder").is_empty(), "nothing sent");
}

fn live_order() -> Order {
    serde_json::from_value(json!({
        "id": "o-1", "status": OrderStatus::Open, "symbol": SYMBOL, "type": OrderType::Limit,
        "side": "buy", "price": 29000, "amount": 2, "filled": 0, "remaining": 2, "reduceOnly": false, "lastUpdate": 1
    }))
    .expect("order")
}

#[test]
fn update_order_edits_a_limit_order() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/editorder",
            json!({"result": "success", "editStatus": {"status": "edited", "orderId": "o-1"}}),
        )
    });
    loaded(&h);
    let update: UpdateOrderOpts = serde_json::from_value(json!({
        "order": serde_json::to_value(live_order()).unwrap(), "update": {"price": 29500}
    }))
    .expect("update opts");
    let ids = h
        .drive(h.venue().update_order(update))
        .expect("updateOrder");
    assert_eq!(ids, vec!["o-1"]);
    let body = data(&h.http.calls("/v3/editorder")[0]);
    assert!(body.contains("limitPrice=29500"), "{body}");
    assert!(body.contains("orderId=o-1"), "{body}");
}

#[test]
fn cancel_orders_posts_one_cancel_for_a_single_order() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/cancelorder",
            json!({"result": "success", "cancelStatus": {"status": "cancelled"}}),
        )
    });
    h.drive(h.venue().cancel_orders(vec![live_order()]))
        .expect("cancelOrders");
    let body = data(&h.http.calls("/v3/cancelorder")[0]);
    assert!(body.contains("order_id=o-1"), "{body}");
}

#[test]
fn cancel_orders_surfaces_a_rejected_cancel() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/cancelorder",
            json!({"result": "success", "cancelStatus": {"status": "notFound"}}),
        )
    });
    let error = h
        .drive(h.venue().cancel_orders(vec![live_order()]))
        .expect_err("rejected cancel");
    assert!(error.message.contains("notFound"), "{}", error.message);
}

#[test]
fn set_leverage_puts_a_clamped_preference() {
    let h = futures(|http| {
        http.ok(
            "PUT",
            "/v3/leveragepreferences",
            json!({"result": "success"}),
        )
    });
    loaded(&h);
    h.drive(h.venue().set_leverage(SYMBOL.into(), 500.0, None))
        .expect("setLeverage");
    let body = data(&h.http.calls("/v3/leveragepreferences")[0]);
    assert!(
        body.contains("maxLeverage=50"),
        "clamped to market max: {body}"
    );
}

#[test]
fn change_margin_type_to_isolated_sets_a_leverage_preference() {
    let h = futures(|http| {
        http.ok(
            "PUT",
            "/v3/leveragepreferences",
            json!({"result": "success"}),
        );
        http.ok(
            "GET",
            "/v3/openpositions",
            json!({"result": "success", "openPositions": []}),
        );
    });
    loaded(&h);
    h.drive(h.venue().change_margin_type(true, Some(SYMBOL.into())))
        .expect("changeMarginType");
    let body = data(&h.http.calls("/v3/leveragepreferences")[0]);
    assert!(body.contains("maxLeverage=50"), "{body}");
}

#[test]
fn start_loads_markets_tickers_connects_sockets_and_ticks() {
    let h = futures(|http| {
        http.ok(
            "GET",
            "/v3/leveragepreferences",
            json!({"result": "success", "leveragePreferences": []}),
        );
        http.ok("GET", "/v3/openorders", open_orders());
        http.ok("GET", "/v3/openpositions", open_positions());
        http.ok("GET", "/v3/accounts", accounts());
    });
    h.drive(h.venue().start()).expect("start");
    for _ in 0..10 {
        h.runtime.advance(100.0);
    }
    assert_eq!(h.store.markets().len(), 1);
    assert_eq!(h.store.tickers().len(), 1);
    let loaded = h.store.loaded().clone();
    assert!(loaded.markets && loaded.tickers);
    assert!(
        loaded.orders && loaded.positions && loaded.balance,
        "{loaded:?}"
    );
    assert_eq!(h.store.orders().len(), 1);
    let urls: Vec<String> = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .map(|(u, _)| u.clone())
        .collect();
    assert!(
        urls.iter().any(|u| u == "wss://futures.kraken.com/ws/v1"),
        "{urls:?}"
    );
    let public = h.runtime.sockets.borrow()[0].1.clone();
    assert!(
        public
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("\"feed\":\"ticker\"")),
        "ticker subscription sent on open: {:?}",
        public.sent.borrow()
    );
}

#[test]
fn private_socket_sends_the_challenge_on_open_and_subscribes_after_auth() {
    let h = futures(|http| {
        http.ok(
            "GET",
            "/v3/leveragepreferences",
            json!({"result": "success", "leveragePreferences": []}),
        );
        http.ok("GET", "/v3/openorders", open_orders());
        http.ok("GET", "/v3/openpositions", open_positions());
        http.ok("GET", "/v3/accounts", accounts());
    });
    h.drive(h.venue().start()).expect("start");
    h.runtime.advance(100.0);
    let private = h.runtime.sockets.borrow()[1].1.clone();
    assert!(
        private
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("\"event\":\"challenge\"")),
        "challenge requested: {:?}",
        private.sent.borrow()
    );
    assert!(
        !private
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("\"event\":\"subscribe\"")),
        "no subscription before the challenge is signed"
    );
    private
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(
                json!({"event": "challenge", "message": "challenge-text"}).to_string(),
            ),
        ));
    for _ in 0..5 {
        h.runtime.advance(100.0);
    }
    let sent = private.sent.borrow();
    let subscriptions: Vec<&String> = sent
        .iter()
        .filter(|m| m.contains("\"event\":\"subscribe\""))
        .collect();
    assert_eq!(subscriptions.len(), 4, "{sent:?}");
    assert!(subscriptions[0].contains("\"signed_challenge\""));
}

#[test]
fn inherited_base_members_reject_not_implemented() {
    let h = futures(|_| {});
    let msg = |e: safe_cex_exchange::runtime::ExchangeError| e.message;
    assert_eq!(
        msg(h.drive(h.venue().get_account()).unwrap_err()),
        "Not implemented"
    );
    assert_eq!(
        msg(h
            .drive(h.venue().fetch_symbol_max_leverage(SYMBOL.into()))
            .unwrap_err()),
        "Not implemented"
    );
    assert_eq!(
        msg(h
            .drive(h.venue().change_position_mode(true, None))
            .unwrap_err()),
        "Not implemented"
    );
    assert_eq!(
        msg(h.drive(h.venue().schedule_cancel(None)).unwrap_err()),
        "Not implemented"
    );
    assert_eq!(
        msg(h
            .drive(h.venue().fetch_public_trades(SYMBOL.into(), None))
            .unwrap_err()),
        "Not implemented"
    );
}

#[test]
fn spot_constructs_and_inherits_leverage_and_margin_rejections() {
    let h = harness(ExchangeName::Krakenspot, options(), |http| {
        http.ok(
            "GET",
            "/0/public/AssetPairs",
            json!({"error": [], "result": {"XXBTZUSD": {
                "wsname": "XBT/USD", "pair_decimals": 1, "lot_decimals": 8, "ordermin": "0.0001",
                "status": "online", "altname": "XBTUSD"
            }}}),
        );
    });
    let markets = h
        .drive(h.venue().fetch_markets())
        .expect("spot fetchMarkets");
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].symbol, "XBT/USD");
    assert_eq!(markets[0].base, "BTC", "XBT normalized");
    assert_eq!(
        h.drive(h.venue().set_leverage("XBT/USD".into(), 2.0, None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(
        h.drive(h.venue().change_margin_type(true, None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
}

#[test]
fn place_order_with_stop_loss_places_the_derived_algo_order_through_place_orders() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/sendorder",
            json!({"result": "success", "sendStatus": {"status": "placed", "order_id": "main-1"}}),
        );
        http.ok(
            "POST",
            "/v3/sendorder",
            json!({"result": "success", "sendStatus": {"status": "placed", "order_id": "stop-1"}}),
        );
    });
    loaded(&h);
    let ids = h
        .drive(Box::pin(safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(
            h.venue(),
            order(json!({"symbol": SYMBOL, "type": "limit", "side": "buy", "amount": 1, "price": 30000, "stopLoss": 29000})),
        )))
        .expect("placeOrder with stopLoss");
    assert_eq!(ids, vec!["main-1", "stop-1"]);
    let sent = h.http.calls("/v3/sendorder");
    assert_eq!(sent.len(), 2);
    let stop = data(&sent[1]);
    assert!(stop.contains("orderType=stp"), "{stop}");
    assert!(stop.contains("stopPrice=29000"), "{stop}");
    assert!(stop.contains("side=sell"), "{stop}");
}

#[test]
fn spot_place_order_posts_add_order_with_a_module_client_id() {
    let h = harness(ExchangeName::Krakenspot, options(), |http| {
        http.ok(
            "GET",
            "/0/public/AssetPairs",
            json!({"error": [], "result": {"XXBTZUSD": {
                "wsname": "XBT/USD", "pair_decimals": 1, "lot_decimals": 8, "ordermin": "0.0001",
                "status": "online", "altname": "XBTUSD"
            }}}),
        );
        http.ok(
            "POST",
            "/0/private/AddOrder",
            json!({"error": [], "result": {"txid": ["T-1"]}}),
        );
    });
    let markets = h
        .drive(h.venue().fetch_markets())
        .expect("spot fetchMarkets");
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .expect("store update");
    let ids = h
        .drive(Box::pin(safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(
            h.venue(),
            order(json!({"symbol": "XBT/USD", "type": "limit", "side": "buy", "amount": 0.5, "price": 30000})),
        )))
        .expect("spot placeOrder");
    assert_eq!(ids, vec!["T-1"]);
    let sent = &h.http.calls("/0/private/AddOrder")[0];
    let body = data(sent);
    assert!(body.contains("ordertype=limit"), "{body}");
    assert!(
        body.contains("nonce="),
        "spot signs the nonce into the body: {body}"
    );
    assert!(
        sent.headers["API-Sign"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
}
