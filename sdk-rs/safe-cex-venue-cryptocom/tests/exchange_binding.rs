//! E2E: the production CryptocomExchange through the Exchange trait over a scripted transport.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::{
    ExchangeName, ExchangeOptions, Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts,
    PositionSide, UpdateOrderOpts,
};
use serde_json::{Value, json};
use support::{Harness, harness, options};

const SYMBOL: &str = "BTCUSD-PERP";

fn ok(result: Value) -> Value {
    json!({"id": 1, "method": "m", "code": 0, "result": result})
}

fn instruments() -> Value {
    ok(json!({"data": [{
        "inst_type": "PERPETUAL_SWAP", "tradable": true, "symbol": SYMBOL, "base_ccy": "BTC",
        "quote_ccy": "USD", "qty_tick_size": "0.0001", "price_tick_size": "0.5",
        "contract_size": "1", "underlying_symbol": "BTCUSD", "quantity_decimals": 4,
        "quote_decimals": 1, "max_leverage": "50"
    }, {
        "inst_type": "CCY_PAIR", "tradable": true, "symbol": "BTC_USD", "base_ccy": "BTC",
        "quote_ccy": "USD", "qty_tick_size": "0.0001", "price_tick_size": "0.5",
        "contract_size": "1", "underlying_symbol": "BTCUSD", "quantity_decimals": 4,
        "quote_decimals": 1, "max_leverage": "1"
    }]}))
}

fn tickers() -> Value {
    ok(json!({"data": [
        {"i": SYMBOL, "b": "30000", "k": "30001", "a": "30000.5", "c": "0.012", "v": "8", "vv": "81", "oi": "6"},
        {"i": "BTC_USD", "b": "1", "k": "2", "a": "1.5", "c": "0", "v": "1", "vv": "1", "oi": "0"}
    ]}))
}

fn open_orders() -> Value {
    ok(json!({"data": [{
        "order_id": "O1", "client_oid": "c-1", "instrument_name": SYMBOL, "quantity": "4",
        "cumulative_quantity": "1.5", "limit_price": "29000", "order_type": "LIMIT",
        "side": "BUY", "status": "ACTIVE", "update_time": 33, "create_time": 11,
        "time_in_force": "GOOD_TILL_CANCEL"
    }]}))
}

fn advanced_orders() -> Value {
    ok(json!({"data": [{
        "order_id": "A2", "instrument_name": SYMBOL, "quantity": "4", "cumulative_quantity": "0",
        "ref_price": "25000", "order_type": "STOP_LOSS", "side": "SELL", "status": "ACTIVE",
        "update_time": 33, "create_time": 11, "exec_inst": ["REDUCE_ONLY"]
    }]}))
}

fn positions() -> Value {
    ok(json!({"data": [{
        "instrument_name": SYMBOL, "quantity": "-3", "cost": "-90000", "open_position_pnl": "2",
        "isolation_type": "CROSS_MARGIN", "update_timestamp_ms": 44
    }]}))
}

fn balance() -> Value {
    ok(json!({"data": [{
        "total_margin_balance": "123", "total_available_balance": "90",
        "total_initial_margin": "33", "total_session_unrealized_pnl": "-4",
        "position_balances": [{"instrument_name": "USD", "quantity": "15", "max_withdrawal_balance": "9"}]
    }]}))
}

fn venue(script: impl FnOnce(&support::Http)) -> Harness {
    harness(options(), |http| {
        http.ok("GET", "/public/get-instruments", instruments());
        http.ok("GET", "/public/get-tickers", tickers());
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

/// The signed JSON envelope a private POST carried.
fn envelope(request: &support::Request) -> Value {
    serde_json::from_str(
        request
            .data
            .as_str()
            .expect("private body is a JSON string"),
    )
    .expect("signed envelope")
}

fn only(h: &Harness, path: &str) -> Value {
    let calls = h.http.calls(path);
    assert_eq!(calls.len(), 1, "{path} calls");
    assert_eq!(calls[0].method, "POST", "{path} is a private POST");
    let body = envelope(&calls[0]);
    assert_eq!(body["method"], path.trim_start_matches('/'));
    assert_eq!(body["api_key"], "fixture-key");
    assert!(
        body["sig"].as_str().is_some_and(|s| s.len() == 64),
        "hex HMAC"
    );
    body["params"].clone()
}

fn base(h: &Harness) -> &safe_cex_exchange::exchange::BaseExchange {
    safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref())
}

fn order(opts: Value) -> PlaceOrderOpts {
    serde_json::from_value(opts).expect("place order opts")
}

fn resting(id: &str, kind: OrderType) -> Order {
    serde_json::from_value(json!({
        "id": id, "status": "open", "symbol": SYMBOL, "type": kind, "side": "buy",
        "price": 29000, "amount": 4, "filled": 0, "remaining": 4, "reduceOnly": false,
        "lastUpdate": 1
    }))
    .expect("order")
}

#[test]
fn fetch_markets_maps_perpetual_instruments() {
    let h = venue(|_| {});
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    assert_eq!(markets.len(), 1, "spot pairs are filtered out");
    let m = &markets[0];
    assert_eq!(m.symbol, SYMBOL);
    assert_eq!(m.base, "BTC");
    assert_eq!(m.precision.price, 0.5);
    assert_eq!(m.limits.leverage.max, 50.0);
    let call = &h.http.calls("/public/get-instruments")[0];
    assert_eq!(call.method, "GET", "public endpoints go out as GET");
    assert_eq!(call.base_url, "https://api.crypto.com/exchange/v1");
    assert_eq!(call.params, json!({}));
    assert_eq!(call.headers["Content-Type"], "application/json");
}

#[test]
fn fetch_tickers_keeps_perpetuals() {
    let h = venue(|_| {});
    let tickers = h.drive(h.venue().fetch_tickers()).expect("fetchTickers");
    assert_eq!(tickers.len(), 1);
    assert_eq!(tickers[0].symbol, SYMBOL);
    assert_eq!(tickers[0].bid, 30000.0);
    assert_eq!(tickers[0].ask, 30001.0);
    assert_eq!(tickers[0].percentage, 1.2);
}

#[test]
fn fetch_orders_merges_regular_and_advanced_books() {
    let h = venue(|http| {
        http.ok("POST", "/private/get-open-orders", open_orders());
        http.ok(
            "POST",
            "/private/advanced/get-open-orders",
            advanced_orders(),
        );
    });
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[0].id, "O1");
    assert_eq!(orders[0].side, OrderSide::Buy);
    assert_eq!(orders[0].status, OrderStatus::Open);
    assert_eq!(orders[1].id, "A2");
    assert_eq!(orders[1].r#type, OrderType::StopLoss);
    assert_eq!(only(&h, "/private/get-open-orders"), json!({}));
    only(&h, "/private/advanced/get-open-orders");
    assert!(
        safe_cex_store::Store::orders(h.store.as_ref()).is_empty(),
        "fetchOrders stays pure"
    );
}

#[test]
fn fetch_positions_maps_signed_quantity() {
    let h = venue(|http| http.ok("POST", "/private/get-positions", positions()));
    loaded(&h);
    let positions = h
        .drive(h.venue().fetch_positions(None))
        .expect("fetchPositions");
    let short = positions
        .iter()
        .find(|p| p.side == PositionSide::Short)
        .expect("short");
    assert_eq!(short.contracts, 3.0);
    assert_eq!(
        short.entry_price, -30000.0,
        "cost / contracts keeps the sign of cost"
    );
    assert_eq!(short.unrealized_pnl, 2.0);
    assert_eq!(
        positions.len(),
        1,
        "no flat placeholders for a symbol with a live position"
    );
    only(&h, "/private/get-positions");
    assert!(
        safe_cex_store::Store::positions(h.store.as_ref()).is_empty(),
        "fetchPositions stays pure"
    );
}

#[test]
fn get_account_reads_the_account_uuid() {
    let h = venue(|http| {
        http.ok(
            "POST",
            "/private/get-accounts",
            ok(json!({"master_account": {}, "sub_account_list": [], "this_account": {"uuid": "u-1"}})),
        )
    });
    let account = h.drive(h.venue().get_account()).expect("getAccount");
    assert_eq!(account.user_id, "u-1");
    only(&h, "/private/get-accounts");
}

#[test]
fn validate_account_returns_the_balance_on_success() {
    let h = venue(|http| http.ok("POST", "/private/user-balance", balance()));
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount");
    assert_eq!(serde_json::to_value(&result.error).unwrap(), json!(""));
    let balance = result.balance.expect("balance");
    assert_eq!(balance.total, 123.0);
    let usd = balance
        .currencies
        .expect("currencies")
        .get("USD")
        .expect("USD")
        .clone();
    assert!(
        matches!(
            usd,
            safe_cex_types::CurrencyBalance::AvailableLocked {
                available: 9.0,
                locked: 6.0
            }
        ),
        "REST currency retains its source variant"
    );
    assert_eq!(
        serde_json::to_value(usd).unwrap(),
        json!({"available":9.0,"locked":6.0}),
        "validation preserves exact currency field names"
    );
    assert_eq!(
        safe_cex_store::Store::balance(h.store.as_ref()).total,
        0.0,
        "fetchBalance stays pure during validation"
    );
}

#[test]
fn validate_account_reports_an_authentication_rejection() {
    let h = venue(|http| {
        http.route(
            "POST",
            "/private/user-balance",
            401.0,
            json!({"code": 40102, "message": "INVALID_NONCE"}),
        )
    });
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount resolves");
    assert_eq!(
        serde_json::to_value(&result.error).unwrap(),
        json!("Crypto.com API Error 40102: INVALID_NONCE"),
        "the createAPI rejection handler formats the axios error"
    );
}

#[test]
fn place_order_signs_a_limit_create_order() {
    let h = venue(|http| {
        http.ok(
            "POST",
            "/private/create-order",
            ok(json!({"order_id": "N1", "client_oid": "my-id"})),
        )
    });
    loaded(&h);
    let ids = h
        .drive(Box::pin(base(&h).place_order(
            h.exchange.as_ref(),
            order(json!({
                "symbol": SYMBOL, "type": "limit", "side": "buy", "amount": 0.5, "price": 29000,
                "clientOrderId": "my-id"
            })),
        )))
        .expect("placeOrder");
    assert_eq!(ids, vec!["N1"]);
    let params = only(&h, "/private/create-order");
    assert_eq!(params["instrument_name"], SYMBOL);
    assert_eq!(params["side"], "BUY");
    assert_eq!(params["type"], "LIMIT");
    assert_eq!(params["quantity"], "0.5");
    assert_eq!(params["price"], "29000");
    assert_eq!(params["client_oid"], "my-id");
}

#[test]
fn place_order_rejects_a_client_order_id_over_36_units() {
    let h = venue(|_| {});
    loaded(&h);
    let err = h
        .drive(Box::pin(base(&h).place_order(
            h.exchange.as_ref(),
            order(json!({
                "symbol": SYMBOL, "type": "limit", "side": "buy", "amount": 1, "price": 1,
                "clientOrderId": "x".repeat(37)
            })),
        )))
        .expect_err("rule rejects");
    assert!(err.message.contains("Crypto.com"), "{}", err.message);
    assert!(h.http.calls("/private/create-order").is_empty());
}

#[test]
fn place_orders_places_each_order_in_series() {
    let h = venue(|http| {
        http.ok(
            "POST",
            "/private/create-order",
            ok(json!({"order_id": "N1"})),
        );
        http.ok(
            "POST",
            "/private/create-order",
            ok(json!({"order_id": "N2"})),
        );
    });
    loaded(&h);
    let one = json!({"symbol": SYMBOL, "type": "market", "side": "sell", "amount": 1});
    let ids = h
        .drive(Box::pin(base(&h).place_orders(
            h.exchange.as_ref(),
            vec![order(one.clone()), order(one)],
        )))
        .expect("placeOrders");
    assert_eq!(ids, vec!["N1", "N2"]);
    assert_eq!(h.http.calls("/private/create-order").len(), 2);
}

#[test]
fn update_order_amends_price_and_quantity() {
    let h = venue(|http| {
        http.ok(
            "POST",
            "/private/amend-order",
            ok(json!({"order_id": "O1"})),
        )
    });
    loaded(&h);
    let opts: UpdateOrderOpts = serde_json::from_value(json!({
        "order": resting("O1", OrderType::Limit), "update": {"price": 28000}
    }))
    .expect("update opts");
    let ids = h.drive(h.venue().update_order(opts)).expect("updateOrder");
    assert_eq!(ids, vec!["O1"]);
    let params = only(&h, "/private/amend-order");
    assert_eq!(params["order_id"], "O1");
    assert_eq!(params["new_price"], "28000");
    assert_eq!(params["new_quantity"], "4");
}

#[test]
fn cancel_orders_routes_triggers_to_the_advanced_endpoint() {
    let h = venue(|http| {
        http.ok("POST", "/private/cancel-order", ok(json!({})));
        http.ok("POST", "/private/advanced/cancel-order", ok(json!({})));
    });
    loaded(&h);
    h.drive(h.venue().cancel_orders(vec![
        resting("O1", OrderType::Limit),
        resting("A2", OrderType::StopLoss),
    ]))
    .expect("cancelOrders");
    assert_eq!(
        only(&h, "/private/cancel-order"),
        json!({"instrument_name": SYMBOL, "order_id": "O1"})
    );
    assert_eq!(
        only(&h, "/private/advanced/cancel-order"),
        json!({"order_id": "A2"})
    );
}

#[test]
fn set_leverage_posts_the_account_leverage() {
    let h = venue(|http| http.ok("POST", "/private/change-account-leverage", ok(json!({}))));
    h.drive(h.venue().set_leverage(SYMBOL.into(), 20.0, None))
        .expect("setLeverage");
    assert_eq!(
        only(&h, "/private/change-account-leverage"),
        json!({"instrument_name": SYMBOL, "leverage": "20"})
    );
}

#[test]
fn change_position_mode_only_warns_and_change_margin_type_rejects() {
    let h = venue(|_| {});
    h.drive(h.venue().change_position_mode(false, None))
        .expect("changePositionMode resolves: always hedged");
    assert!(h.http.requests.borrow().is_empty());
    let err = h
        .drive(h.venue().change_margin_type(true, Some(SYMBOL.into())))
        .expect_err("changeMarginType");
    assert_eq!(
        err.message,
        "Crypto.com does not support switching BTCUSD-PERP to isolated margin through Exchange v1"
    );
}

#[test]
fn inherited_base_members_reject_or_resolve_empty() {
    let h = venue(|_| {});
    for err in [
        h.drive(h.venue().fetch_symbol_max_leverage(SYMBOL.into()))
            .map(|_| ())
            .expect_err("base.ts:799"),
        h.drive(h.venue().schedule_cancel(None))
            .expect_err("base.ts:989"),
        h.drive(h.venue().fetch_public_trades(SYMBOL.into(), None))
            .map(|_| ())
            .expect_err("base.ts:1138"),
    ] {
        assert_eq!(err.message, "Not implemented");
    }
    let fills =
        h.drive(h.venue().fetch_historical_fills(
            serde_json::from_value(json!({})).expect("executions options"),
        ))
        .expect("base.ts:997");
    assert!(fills.is_empty());
}

#[test]
fn a_signature_rejection_in_a_200_envelope_resends_once_resigned() {
    let h = venue(|http| {
        http.ok(
            "POST",
            "/private/get-accounts",
            json!({"id": 1, "method": "private/get-accounts", "code": 40101, "message": "UNAUTHORIZED"}),
        );
        http.ok(
            "POST",
            "/private/get-accounts",
            ok(json!({"this_account": {"uuid": "u-2"}})),
        );
    });
    let account = h.drive(h.venue().get_account()).expect("resend succeeds");
    assert_eq!(account.user_id, "u-2");
    let calls = h.http.calls("/private/get-accounts");
    assert_eq!(calls.len(), 2, "exactly one resend");
    let (first, second) = (envelope(&calls[0]), envelope(&calls[1]));
    assert_eq!(
        second["params"],
        json!({}),
        "the replay re-signs its params"
    );
    assert_eq!(first["method"], second["method"]);
}

#[test]
fn start_loads_markets_tickers_sockets_and_the_private_snapshot() {
    let h = venue(|http| {
        http.ok("POST", "/private/get-open-orders", open_orders());
        http.ok(
            "POST",
            "/private/advanced/get-open-orders",
            advanced_orders(),
        );
        http.ok("POST", "/private/get-positions", positions());
        http.ok("POST", "/private/user-balance", balance());
    });
    h.drive(h.venue().start()).expect("start");
    assert_eq!(safe_cex_store::Store::markets(h.store.as_ref()).len(), 1);
    let urls: Vec<String> = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .map(|(u, _)| u.clone())
        .collect();
    assert!(urls.iter().any(|u| u.ends_with("/market")), "{urls:?}");
    assert!(urls.iter().any(|u| u.ends_with("/user")), "{urls:?}");
    assert_eq!(safe_cex_store::Store::orders(h.store.as_ref()).len(), 2);
    assert_eq!(h.http.calls("/private/get-positions").len(), 1);
    let balance = safe_cex_store::Store::balance(h.store.as_ref());
    assert_eq!(balance.total, 123.0, "tick stores the REST balance");
    let usd = &balance.currencies.as_ref().expect("currencies")["USD"];
    assert!(
        matches!(
            usd,
            safe_cex_types::CurrencyBalance::AvailableLocked {
                available: 9.0,
                locked: 6.0
            }
        ),
        "REST currency keeps raw available/locked shape in Store"
    );
    assert_eq!(
        serde_json::to_value(usd).unwrap(),
        json!({"available":9.0,"locked":6.0}),
        "Store preserves exact currency field names"
    );
    drop(balance);
    h.venue().dispose().expect("dispose");
}

#[test]
fn constructing_another_venue_name_is_refused() {
    let runtime = std::rc::Rc::new(support::Runtime::default());
    let http = std::rc::Rc::new(support::Http::default());
    *http.runtime.borrow_mut() = Some(runtime.clone());
    let opts: ExchangeOptions = options();
    let result = safe_cex_venue_cryptocom::binding::exchange::CryptocomExchange::new(
        ExchangeName::Kraken,
        opts,
        std::rc::Rc::new(safe_cex_store::store::DefaultStore::new()),
        safe_cex_venue_cryptocom::binding::exchange::CryptocomDependencies {
            runtime,
            http,
            external_public_data: None,
        },
    );
    assert!(result.is_err());
}

#[test]
fn a_non_2xx_signature_rejection_retries_through_axios_retry() {
    let h = venue(|http| {
        http.route(
            "POST",
            "/private/get-accounts",
            401.0,
            json!({"code": 40101, "message": "UNAUTHORIZED"}),
        );
        http.ok(
            "POST",
            "/private/get-accounts",
            ok(json!({"this_account": {"uuid": "u-3"}})),
        );
    });
    let account = h.drive(h.venue().get_account()).expect("retry succeeds");
    assert_eq!(account.user_id, "u-3");
    assert_eq!(h.http.calls("/private/get-accounts").len(), 2);
}

#[test]
fn disposal_during_fetch_markets_stops_start_before_store_update() {
    use std::task::{Context, Poll, Waker};
    let h = venue(|_| {});
    let released = h.http.hold("/public/get-instruments");
    let mut start = h.venue().start();
    for _ in 0..20 {
        assert!(matches!(
            start.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        h.runtime.pump();
        if !h.http.calls("/public/get-instruments").is_empty() {
            break;
        }
    }
    assert_eq!(h.http.calls("/public/get-instruments").len(), 1);
    h.venue().dispose().expect("dispose while HTTP is pending");
    released.set(true);
    h.drive(start).expect("disposed start resolves");
    assert!(safe_cex_store::Store::markets(h.store.as_ref()).is_empty());
    assert!(h.http.calls("/public/get-tickers").is_empty());
    assert!(h.runtime.sockets.borrow().is_empty());
}

#[test]
fn private_subscriptions_wait_for_a_successful_auth_ack() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let h = venue(|http| {
        http.ok("POST", "/private/get-open-orders", open_orders());
        http.ok(
            "POST",
            "/private/advanced/get-open-orders",
            advanced_orders(),
        );
        http.ok("POST", "/private/get-positions", positions());
        http.ok("POST", "/private/user-balance", balance());
    });
    h.drive(h.venue().start()).expect("start");
    h.runtime.advance(1000.0);
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/user"))
        .expect("private socket")
        .1
        .clone();
    let frames = || {
        socket
            .sent
            .borrow()
            .iter()
            .map(|s| serde_json::from_str::<Value>(s).expect("WS JSON"))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        frames()
            .iter()
            .filter(|f| f["method"] == "public/auth")
            .count(),
        1
    );
    assert!(frames().iter().all(|f| f["method"] != "subscribe"));
    // Crypto.com's source logs a failed ack and keeps the channels pending.
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"method": "public/auth", "code": 40101, "message": "UNAUTHORIZED"}).to_string(),
        )));
    h.runtime.pump();
    assert!(frames().iter().all(|f| f["method"] != "subscribe"));
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"method": "public/auth", "code": 0}).to_string(),
        )));
    h.runtime.pump();
    let subscribed = frames();
    let subscribed: Vec<_> = subscribed
        .iter()
        .filter(|f| f["method"] == "subscribe")
        .collect();
    assert_eq!(
        subscribed.len(),
        1,
        "frames={:?} reports={:?} console={:?}",
        frames(),
        h.runtime.reports.borrow(),
        h.runtime.console.borrow()
    );
    assert_eq!(
        subscribed[0]["params"]["channels"],
        json!([
            "user.order",
            "user.advance.order",
            "user.positions",
            "user.balance"
        ])
    );
    h.venue().dispose().expect("dispose");
}

#[test]
fn public_only_start_skips_private_sockets_and_rest() {
    let mut opts = serde_json::to_value(options()).unwrap();
    opts["publicDataOnly"] = json!(true);
    let h = harness(serde_json::from_value(opts).unwrap(), |http| {
        http.ok("GET", "/public/get-instruments", instruments());
        http.ok("GET", "/public/get-tickers", tickers());
    });
    h.drive(h.venue().start()).expect("public-only start");
    h.runtime.pump();
    assert_eq!(safe_cex_store::Store::markets(h.store.as_ref()).len(), 1);
    assert!(h.http.requests.borrow().iter().all(|r| r.method == "GET"));
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
    assert!(h.runtime.sockets.borrow()[0].0.ends_with("/market"));
    let loaded = safe_cex_store::Store::loaded(h.store.as_ref());
    assert!(loaded.orders && loaded.positions && loaded.balance);
    drop(loaded);
    h.venue().dispose().expect("dispose");
}
