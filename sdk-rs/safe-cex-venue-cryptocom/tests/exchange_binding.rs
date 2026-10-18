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
        safe_cex_store::Store::orders(h.store.as_ref())
            .unwrap()
            .is_empty(),
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
        safe_cex_store::Store::positions(h.store.as_ref())
            .unwrap()
            .is_empty(),
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
        safe_cex_store::Store::balance(h.store.as_ref())
            .unwrap()
            .total,
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

// ts: cryptocom.exchange.ts cancelSymbolOrders / cancelAllOrders post the venue cancel-all,
// so orders the store has not mapped yet are cancelled too.
#[test]
fn cancel_symbol_and_all_orders_post_the_venue_cancel_all() {
    let h = venue(|http| http.ok("POST", "/private/cancel-all-orders", ok(json!({}))));
    loaded(&h);
    h.drive(h.venue().cancel_symbol_orders(SYMBOL.into()))
        .expect("cancelSymbolOrders");
    assert_eq!(
        only(&h, "/private/cancel-all-orders"),
        json!({"instrument_name": SYMBOL})
    );
    h.drive(h.venue().cancel_all_orders())
        .expect("cancelAllOrders");
    let calls = h.http.calls("/private/cancel-all-orders");
    assert_eq!(calls.len(), 2, "cancelAllOrders posts once");
    assert_eq!(envelope(&calls[1])["params"], json!({}));
    assert!(h.http.calls("/private/cancel-order").is_empty());
}

#[test]
fn cancel_all_orders_rethrows_a_venue_rejection() {
    let h = venue(|http| {
        http.route(
            "POST",
            "/private/cancel-all-orders",
            400.0,
            json!({"code": 40003, "message": "BAD_REQUEST"}),
        )
    });
    loaded(&h);
    let symbol = h
        .drive(h.venue().cancel_symbol_orders(SYMBOL.into()))
        .expect_err("cancelSymbolOrders rethrows");
    let all = h
        .drive(h.venue().cancel_all_orders())
        .expect_err("cancelAllOrders rethrows");
    assert_eq!(symbol.message, all.message);
    assert!(symbol.message.contains("BAD_REQUEST"), "{}", symbol.message);
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
    assert_eq!(
        safe_cex_store::Store::markets(h.store.as_ref())
            .unwrap()
            .len(),
        1
    );
    let urls: Vec<String> = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .map(|(u, _)| u.clone())
        .collect();
    assert!(urls.iter().any(|u| u.ends_with("/market")), "{urls:?}");
    assert!(urls.iter().any(|u| u.ends_with("/user")), "{urls:?}");
    assert_eq!(
        safe_cex_store::Store::orders(h.store.as_ref())
            .unwrap()
            .len(),
        2
    );
    assert_eq!(h.http.calls("/private/get-positions").len(), 1);
    let balance = safe_cex_store::Store::balance(h.store.as_ref()).unwrap();
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

fn started(script: impl FnOnce(&support::Http)) -> Harness {
    let h = venue(|http| {
        http.ok("POST", "/private/get-open-orders", open_orders());
        http.ok(
            "POST",
            "/private/advanced/get-open-orders",
            advanced_orders(),
        );
        http.ok("POST", "/private/get-positions", positions());
        http.ok("POST", "/private/user-balance", balance());
        script(http);
    });
    h.drive(h.venue().start()).expect("start");
    h
}

fn market_socket(h: &Harness) -> std::rc::Rc<support::Socket> {
    h.runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/market"))
        .expect("public market socket")
        .1
        .clone()
}

fn public_frame(h: &Harness, result: Value) {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    market_socket(h)
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"id": -1, "method": "subscribe", "code": 0, "result": result}).to_string(),
        )));
    for _ in 0..4 {
        h.runtime.advance(50.0);
    }
}

fn sent_channels(h: &Harness, method: &str) -> Vec<Value> {
    market_socket(h)
        .sent
        .borrow()
        .iter()
        .filter_map(|m| serde_json::from_str::<Value>(m).ok())
        .filter(|m| m["method"] == method)
        .flat_map(|m| {
            m["params"]["channels"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .collect()
}

// ts: cryptocom.exchange.ts fetchOHLCV posts public/get-candlestick and maps each row.
#[test]
fn fetch_ohlcv_reads_venue_candlesticks() {
    let h = venue(|http| {
        http.ok(
            "GET",
            "/public/get-candlestick",
            ok(json!({"data": [
                {"t": 1_700_000_000_000_i64, "o": "1", "h": "2", "l": "0.5", "c": "1.5", "v": "10"}
            ]})),
        )
    });
    loaded(&h);
    let opts: safe_cex_types::OHLCVOptions =
        serde_json::from_value(json!({"symbol": SYMBOL, "interval": "1h"})).unwrap();
    let candles = h.drive(h.venue().fetch_ohlcv(opts)).expect("fetchOHLCV");
    assert_eq!(candles.len(), 1, "venue candles are returned");
    assert_eq!(candles[0].timestamp, 1_700_000_000_000.0);
    assert_eq!(
        (
            candles[0].open,
            candles[0].high,
            candles[0].low,
            candles[0].close,
            candles[0].volume
        ),
        (1.0, 2.0, 0.5, 1.5, 10.0)
    );
    let call = &h.http.calls("/public/get-candlestick")[0];
    assert_eq!(call.method, "GET", "public reads go out as GET");
    assert_eq!(
        call.params,
        json!({"instrument_name": SYMBOL, "timeframe": "1h"})
    );
}

// ts: cryptocom.ws-public.ts listenOrderBook subscribes book.<symbol>.50 and replaces both sides.
#[test]
fn listen_order_book_subscribes_and_delivers_snapshots() {
    let h = started(|_| {});
    let books = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let sink = books.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            SYMBOL.into(),
            std::rc::Rc::new(move |b| sink.borrow_mut().push(b)),
        )
        .expect("listenOrderBook");
    h.runtime.advance(50.0);
    assert!(
        sent_channels(&h, "subscribe").contains(&json!(format!("book.{SYMBOL}.50"))),
        "book subscription sent"
    );
    public_frame(
        &h,
        json!({"channel": "book", "subscription": format!("book.{SYMBOL}.50"),
               "instrument_name": SYMBOL,
               "data": [{"bids": [["30000", "1", "1"]], "asks": [["30001", "2", "1"]], "t": 1}]}),
    );
    let books = books.borrow();
    assert!(!books.is_empty(), "order book callback fired");
    let last = books.last().unwrap();
    assert_eq!((last.bids[0].price, last.bids[0].amount), (30000.0, 1.0));
    assert_eq!((last.asks[0].price, last.asks[0].amount), (30001.0, 2.0));
    drop(books);
    cleanup().expect("cleanup");
    assert!(
        sent_channels(&h, "unsubscribe").contains(&json!(format!("book.{SYMBOL}.50"))),
        "last listener unsubscribes"
    );
    h.venue().dispose().expect("dispose");
}

// ts: cryptocom.ws-public.ts listenOHLCV subscribes candlestick.<interval>.<symbol>.
#[test]
fn listen_ohlcv_subscribes_and_delivers_candles() {
    let h = started(|_| {});
    let candles = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let sink = candles.clone();
    let opts: safe_cex_types::OHLCVOptions =
        serde_json::from_value(json!({"symbol": SYMBOL, "interval": "1h"})).unwrap();
    let _cleanup = h
        .venue()
        .listen_ohlcv(opts, std::rc::Rc::new(move |c| sink.borrow_mut().push(c)))
        .expect("listenOHLCV");
    h.runtime.advance(50.0);
    let channel = format!("candlestick.1h.{SYMBOL}");
    assert!(
        sent_channels(&h, "subscribe").contains(&json!(channel)),
        "candlestick subscription sent"
    );
    public_frame(
        &h,
        json!({"channel": "candlestick", "subscription": channel, "instrument_name": SYMBOL,
               "interval": "1h",
               "data": [{"t": 1_700_000_000_000_i64, "o": "1", "h": "2", "l": "0.5", "c": "1.5", "v": "10"}]}),
    );
    let candles = candles.borrow();
    assert_eq!(candles.len(), 1, "candle callback fired once");
    assert_eq!(candles[0].close, 1.5);
    drop(candles);
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
        std::rc::Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
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
    assert!(
        safe_cex_store::Store::markets(h.store.as_ref())
            .unwrap()
            .is_empty()
    );
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
    assert_eq!(
        safe_cex_store::Store::markets(h.store.as_ref())
            .unwrap()
            .len(),
        1
    );
    assert!(h.http.requests.borrow().iter().all(|r| r.method == "GET"));
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
    assert!(h.runtime.sockets.borrow()[0].0.ends_with("/market"));
    let loaded = safe_cex_store::Store::loaded(h.store.as_ref()).unwrap();
    assert!(loaded.orders && loaded.positions && loaded.balance);
    h.venue().dispose().expect("dispose");
}

#[test]
fn dispose_sets_source_flag_without_removing_inherited_listeners() {
    use std::{cell::Cell, rc::Rc};
    let h = venue(|_| {});
    let observed = Rc::new(Cell::new(0));
    let seen = observed.clone();
    h.venue().base().on(
        "update",
        Rc::new(move |_, _| {
            seen.set(seen.get() + 1);
            Ok(())
        }),
    );
    h.venue().dispose().expect("source dispose");
    assert!(
        h.venue().base().is_disposed(),
        "source assignment reaches shared state"
    );
    h.venue()
        .base()
        .emit(
            "update",
            safe_cex_exchange::js_runtime::value::from_json(&json!({})),
        )
        .unwrap();
    assert_eq!(
        observed.get(),
        1,
        "source override does not invoke base listener cleanup"
    );
}

// ts: cryptocom.api.ts createAPI through safeCexAxiosCreate (provider baseURL and proxy
// authorization) and, under tealstreetTunnel, installTransportAdapter.
#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn create_api_routes_through_the_provider_and_tunnel_adapter() {
    use safe_cex_transport::{
        DirectTransport, PrivateProxy, ResolvedRouting, RestAdapter, RestAdapterContext,
        RestAdapterVariant, RestAuthorizer, RestFuture, RestRequest, RestResponse,
        RestResponseBody, RestTransport, RestTransportContext, RoutingOptions, TransportError,
        TransportFuture, TransportProvider, WsConnection,
    };
    use std::{cell::RefCell, rc::Rc, sync::Arc};
    const PROXIED: &str = "https://private-proxy/https://api.crypto.com/exchange/v1";
    struct Authorize;
    impl RestAuthorizer for Authorize {
        fn authorize<'a>(&'a self, request: &'a mut RestRequest) -> TransportFuture<'a, ()> {
            Box::pin(async move {
                let Some(safe_cex_transport::RestBody::Encoded(body)) = &request.body else {
                    panic!("ROUTING encoded body: {:?}", request.body)
                };
                let body: Value = serde_json::from_slice(body).unwrap();
                assert!(
                    body["sig"].as_str().is_some_and(|s| s.len() == 64),
                    "ROUTING venue signs before proxy authorization"
                );
                request
                    .headers
                    .insert("Authorization".into(), "Bearer proxy-token".into());
                Ok(())
            })
        }
    }
    struct Terminal(Rc<RefCell<Vec<RestRequest>>>);
    impl RestAdapter for Terminal {
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            self.0.borrow_mut().push(request);
            Box::pin(async {
                Ok(RestResponse {
                    status: 200,
                    reason: None,
                    headers: Default::default(),
                    body: RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(balance())
                            .unwrap(),
                    ),
                })
            })
        }
    }
    struct Provider {
        variants: Rc<RefCell<Vec<RestAdapterVariant>>>,
        sent: Rc<RefCell<Vec<RestRequest>>>,
    }
    impl TransportProvider for Provider {
        fn name(&self) -> &str {
            "cryptocom-routing"
        }
        fn rest(&self, c: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
            assert_eq!(c.exchange, &ExchangeName::Cryptocom, "ROUTING exchange");
            assert_eq!(
                c.config.base_url.as_deref(),
                Some("https://api.crypto.com/exchange/v1"),
                "ROUTING original base"
            );
            Ok(RestTransport {
                base_url: Some(PROXIED.into()),
                proxied: true,
                authorize: Some(Arc::new(Authorize)),
            })
        }
        fn rest_adapter(&self, c: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
            self.variants.borrow_mut().push(c.variant);
            Some(Arc::new(Terminal(self.sent.clone())))
        }
        fn proxy_prefix(&self, _: RestTransportContext<'_>) -> String {
            String::new()
        }
        fn public_cors_proxy(&self) -> Option<String> {
            None
        }
        fn private_proxy(&self, _: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
            None
        }
        fn require_proxy(&self, options: ExchangeOptions) -> ExchangeOptions {
            options
        }
        fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
            None
        }
        fn ws_routing(&self, name: &ExchangeName, options: &RoutingOptions) -> ResolvedRouting {
            DirectTransport.ws_routing(name, options)
        }
        fn ws_connection(&self, _: &str) -> Option<WsConnection> {
            None
        }
    }
    for tunnel in [false, true] {
        let runtime = Rc::new(support::Runtime::default());
        let variants = Rc::new(RefCell::new(vec![]));
        let sent = Rc::new(RefCell::new(vec![]));
        *runtime.provider.borrow_mut() = Some(Arc::new(Provider {
            variants: variants.clone(),
            sent: sent.clone(),
        }));
        let options = serde_json::from_value(json!({
            "key": "fixture-key", "secret": "fixture-secret",
            "tealstreetProxy": true, "tealstreetTunnel": tunnel
        }))
        .unwrap();
        let h = support::harness_on(runtime, options, |http| {
            http.ok("POST", "/private/user-balance", balance())
        });
        let result = h
            .drive(h.venue().validate_account())
            .expect("validateAccount");
        assert_eq!(serde_json::to_value(&result.error).unwrap(), json!(""));
        if tunnel {
            assert_eq!(
                *variants.borrow(),
                [RestAdapterVariant::Default],
                "ROUTING tunnel installs the default adapter once"
            );
            assert!(
                h.http.requests.borrow().is_empty(),
                "ROUTING adapter replaces host"
            );
            let requests = sent.borrow();
            assert_eq!(
                requests.len(),
                2,
                "ROUTING source validation then fetchBalance"
            );
            assert_eq!(requests[0].base_url.as_deref(), Some(PROXIED));
            assert_eq!(
                requests[0].headers.get("Authorization").map(String::as_str),
                Some("Bearer proxy-token")
            );
        } else {
            assert!(
                variants.borrow().is_empty(),
                "ROUTING no adapter without tunnel"
            );
            let calls = h.http.calls("/private/user-balance");
            assert_eq!(
                calls.len(),
                2,
                "ROUTING source validation then fetchBalance"
            );
            let call = &calls[0];
            assert_eq!(call.base_url, PROXIED, "ROUTING provider base");
            assert_eq!(call.headers["Authorization"], "Bearer proxy-token");
        }
    }
}

fn private_socket(h: &Harness) -> std::rc::Rc<support::Socket> {
    h.runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/user"))
        .expect("private user socket")
        .1
        .clone()
}

fn private_frame(h: &Harness, frame: Value) {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    private_socket(h)
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
    for _ in 0..4 {
        h.runtime.advance(50.0);
    }
}

fn authenticated_private_venue() -> Harness {
    let h = started(|_| {});
    h.runtime.advance(1000.0);
    private_frame(&h, json!({"method":"public/auth","code":0}));
    assert!(
        private_socket(&h).sent.borrow().iter().any(|text| {
            let f: Value = serde_json::from_str(text).expect("sent JSON");
            f["method"] == "subscribe"
                && f["params"]["channels"]
                    == json!([
                        "user.order",
                        "user.advance.order",
                        "user.positions",
                        "user.balance"
                    ])
        }),
        "private subscription reaches transport after auth"
    );
    h
}

fn live_order(id: &str, status: &str, filled: &str) -> Value {
    json!({"order_id":id,"instrument_name":SYMBOL,"quantity":"4",
        "cumulative_quantity":filled,"limit_price":"29001.5","order_type":"LIMIT",
        "side":"BUY","status":status,"update_time":77,"create_time":22,
        "time_in_force":"GOOD_TILL_CANCEL","exec_inst":["REDUCE_ONLY"]})
}

fn stored_order(h: &Harness, id: &str) -> Order {
    safe_cex_store::Store::orders(h.store.as_ref())
        .unwrap()
        .iter()
        .find(|o| o.id == id)
        .unwrap_or_else(|| panic!("private order {id} reached Store"))
        .clone()
}

// Source onMessage processes result/channel even when the frame also has code:0.
#[test]
fn private_order_frames_create_partially_fill_close_and_cancel_orders() {
    let h = authenticated_private_venue();
    let fills = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let observed = fills.clone();
    h.venue().base().on(
        "fill",
        std::rc::Rc::new(move |_, _| {
            observed.set(observed.get() + 1);
            Ok(())
        }),
    );
    let mut unknown = live_order("unknown", "ACTIVE", "0");
    unknown["instrument_name"] = json!("UNKNOWN-PERP");
    let advanced = json!({"order_id":"ws-advanced","instrument_name":SYMBOL,
        "quantity":"2","cumulative_quantity":"0","ref_price":"25000.5",
        "limit_price":"24000","order_type":"STOP_LOSS","side":"SELL",
        "order_status":"ACTIVE","time_created":10,"time_updated":66,
        "time_in_force":"IMMEDIATE_OR_CANCEL"});
    private_frame(
        &h,
        json!({"method":"subscribe","code":0,"result":{
        "channel":"user.order","data":[unknown,live_order("ws-order","ACTIVE","0")]}}),
    );
    private_frame(
        &h,
        json!({"method":"subscribe","code":0,"result":{
        "channel":"user.advance.order","data":[advanced]}}),
    );
    let order = stored_order(&h, "ws-order");
    assert_eq!(
        (order.amount, order.filled, order.remaining),
        (4.0, 0.0, 4.0)
    );
    assert_eq!(
        (
            order.status.unwrap(),
            order.side.unwrap(),
            order.r#type.unwrap()
        ),
        (OrderStatus::Open, OrderSide::Buy, OrderType::Limit)
    );
    assert_eq!(order.price, 29001.5);
    assert_eq!(order.last_update, 77.0);
    assert_eq!(order.reduce_only, true);
    let advanced = stored_order(&h, "ws-advanced");
    assert_eq!(
        (advanced.r#type.unwrap(), advanced.side.unwrap()),
        (OrderType::StopLoss, OrderSide::Sell)
    );
    assert_eq!(
        (advanced.price.to_number(), advanced.last_update),
        (25000.5, 66.0)
    );
    assert!(
        !safe_cex_store::Store::orders(h.store.as_ref())
            .unwrap()
            .iter()
            .any(|o| o.id == "unknown")
    );
    private_frame(
        &h,
        json!({"code":0,"result":{"channel":"user.order","data":[
        live_order("ws-order","ACTIVE","1.5")]}}),
    );
    let partial = stored_order(&h, "ws-order");
    assert_eq!(
        (partial.status.unwrap(), partial.filled, partial.remaining),
        (OrderStatus::Open, 1.5, 2.5)
    );
    private_frame(
        &h,
        json!({"code":0,"result":{"channel":"user.order","data":[
        live_order("ws-order","FILLED","4"), live_order("O1","CANCELED","1.5")]}}),
    );
    let filled = stored_order(&h, "ws-order");
    assert_eq!(
        (filled.status.unwrap(), filled.filled, filled.remaining),
        (OrderStatus::Closed, 4.0, 0.0)
    );
    assert_eq!(stored_order(&h, "O1").status, OrderStatus::Canceled);
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    assert_eq!(
        fills.get(),
        0,
        "source Crypto.com order/fill state updates emit no fill event"
    );
    h.venue().dispose().expect("dispose");
}

#[test]
fn private_position_frames_update_hedged_sides_and_remove_zero_quantity() {
    let h = authenticated_private_venue();
    private_frame(
        &h,
        json!({"method":"subscribe","code":0,"result":{
        "channel":"user.positions","data":[
            {"instrument_name":"UNKNOWN-PERP","quantity":"9"},
            {"instrument_name":SYMBOL,"quantity":"-2","cost":"-60000",
             "target_leverage":"7","liquidation_price":"40000.5","mark_price":"30001",
             "open_position_pnl":"-3.5","isolation_type":"ISOLATED_MARGIN",
             "isolation_id":"isolation-1","update_timestamp_ms":88},
            {"instrument_name":SYMBOL,"quantity":"1","cost":"28000",
             "leverage":"4","open_position_pnl":"5","isolation_type":"CROSS_MARGIN",
             "update_timestamp_ms":89}
        ]}}),
    );
    let positions = safe_cex_store::Store::positions(h.store.as_ref()).unwrap();
    assert_eq!(
        positions.len(),
        2,
        "both rows delivered without unknown market"
    );
    let short = positions
        .iter()
        .find(|p| p.side == PositionSide::Short)
        .expect("short");
    assert_eq!(
        (short.contracts, short.entry_price, short.notional),
        (2.0, -30000.0, 60000.0)
    );
    assert_eq!((short.leverage, short.liquidation_price), (7.0, 40000.5));
    assert_eq!(short.unrealized_pnl, -3.5);
    assert_eq!(
        short.margin_type,
        Some(safe_cex_types::MarginType::Isolated)
    );
    assert_eq!(
        short.position_mode,
        Some(safe_cex_types::PositionMode::Hedged)
    );
    assert_eq!(short.last_update, Some(88.0));
    let info = short.info.value().expect("position info");
    assert_eq!(
        info.get("markPrice").unwrap().as_f64().unwrap().to_bits(),
        30001.0_f64.to_bits()
    );
    assert!(info.get("isolationId").unwrap().text_equals("isolation-1"));
    let serialized = serde_json::to_value(short).expect("position JSON");
    assert_eq!(serialized["info"].as_object().unwrap().len(), 2);
    assert_eq!(serialized["info"]["markPrice"].as_f64(), Some(30001.0));
    assert_eq!(serialized["info"]["isolationId"], "isolation-1");
    let long = positions
        .iter()
        .find(|p| p.side == PositionSide::Long)
        .expect("long");
    assert_eq!(
        (long.contracts, long.entry_price, long.leverage),
        (1.0, 28000.0, 4.0)
    );
    assert_ne!(short.id, long.id, "hedged sides keep distinct identities");
    drop(positions);
    private_frame(
        &h,
        json!({"code":0,"result":{"channel":"user.positions","data":[
        {"instrument_name":SYMBOL,"quantity":"0","cost":"0"}]}}),
    );
    assert!(
        safe_cex_store::Store::positions(h.store.as_ref())
            .unwrap()
            .is_empty(),
        "flat frame removes both sides"
    );
    h.venue().dispose().expect("dispose");
}

#[test]
fn private_balance_frames_replace_rest_currency_shape_and_ignore_empty_updates() {
    let h = authenticated_private_venue();
    private_frame(
        &h,
        json!({"method":"subscribe","code":0,"result":{
        "channel":"user.balance","data":[{
            "total_margin_balance":"200","total_available_balance":"140",
            "total_initial_margin":"60","total_session_unrealized_pnl":"-6",
            "position_balances":[
                {"instrument_name":"USD","quantity":"20","max_withdrawal_balance":"12"},
                {"instrument_name":"BTC","quantity":"2","max_withdrawal_balance":"3"}
            ]
        }]}}),
    );
    let balance = safe_cex_store::Store::balance(h.store.as_ref()).unwrap();
    assert_eq!(
        (balance.total, balance.free, balance.used, balance.upnl),
        (200.0, 140.0, 60.0, -6.0)
    );
    assert_eq!(
        serde_json::to_value(&balance.currencies).unwrap(),
        json!({
            "USD":{"total":20.0,"free":12.0,"used":8.0,"upnl":0.0},
            "BTC":{"total":2.0,"free":3.0,"used":0.0,"upnl":0.0}
        }),
        "WS currency shape follows source, replacing REST available/locked"
    );
    let before = serde_json::to_value(&balance).unwrap();
    drop(balance);
    assert!(
        safe_cex_store::Store::loaded(h.store.as_ref())
            .unwrap()
            .balance
    );
    private_frame(
        &h,
        json!({"code":0,"result":{"channel":"user.balance","data":[]}}),
    );
    assert_eq!(
        serde_json::to_value(safe_cex_store::Store::balance(h.store.as_ref()).unwrap()).unwrap(),
        before
    );
    h.venue().dispose().expect("dispose");
}

#[test]
fn releasing_shared_http_gate_resumes_both_native_market_reads() {
    use std::task::{Context, Poll, Waker};
    let h = venue(|_| {});
    let released = h.http.hold("/public/get-instruments");
    let mut first = h.venue().fetch_markets();
    let mut second = h.venue().fetch_markets();
    for _ in 0..64 {
        for future in [&mut first, &mut second] {
            assert!(matches!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop())),
                Poll::Pending
            ));
        }
        h.runtime.advance(50.0);
        if h.http.calls("/public/get-instruments").len() == 2 {
            break;
        }
    }
    assert_eq!(h.http.calls("/public/get-instruments").len(), 2);
    h.runtime.pump();
    released.set(true);
    let first = h.drive(first).expect("first native market read resumes");
    let second = h.drive(second).expect("second native market read resumes");
    for rows in [first, second] {
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].symbol, SYMBOL);
    }
}

#[test]
fn order_book_rows_require_iterable_pairs_through_real_socket() {
    use std::{cell::RefCell, rc::Rc};
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/book-pair-oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source book parser: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 2);
    for case in cases.as_array().unwrap() {
        assert_eq!(case["error"]["name"], "TypeError");
        let h = started(|_| {});
        let books = Rc::new(RefCell::new(vec![]));
        let sink = books.clone();
        let errors = Rc::new(RefCell::new(vec![]));
        let failures = errors.clone();
        base(&h).on(
            "error",
            Rc::new(move |_, error| {
                failures.borrow_mut().push(error.clone());
                Ok(())
            }),
        );
        let cleanup = h
            .venue()
            .listen_order_book(
                SYMBOL.into(),
                Rc::new(move |book| {
                    sink.borrow_mut().push(book);
                }),
            )
            .unwrap();
        h.runtime.advance(50.0);
        let mut frame = case["update"].clone();
        frame["channel"] = json!("book");
        frame["subscription"] = json!(format!("book.{SYMBOL}.50"));
        public_frame(&h, frame);
        assert_eq!(
            books.borrow().len(),
            case["callbacks"].as_u64().unwrap() as usize,
            "Crypto.com {} plain numeric-key row must throw before book callback, matching source",
            case["side"]
        );
        assert!(
            errors.borrow().iter().any(|e| {
                e.as_str()
                    .is_some_and(|text| String::from_utf16_lossy(text).contains("TypeError"))
            }),
            "pair refusal must retain a visible TypeError cause: {:?}",
            errors.borrow()
        );
        public_frame(
            &h,
            json!({"channel":"book", "subscription":format!("book.{SYMBOL}.50"),
            "instrument_name":SYMBOL,"data":[{"bids":[["30000","1"]],"asks":[["30001","2"]],"t":2}]}),
        );
        assert_eq!(
            books.borrow().len(),
            1,
            "valid array pair still reaches listener after refusal"
        );
        cleanup().unwrap();
        h.venue().dispose().unwrap();
    }
}

#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

#[test]
fn production_ticker_coercion_matches_constructed_typescript() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_transport::socket::SocketFrame;
    let source = ticker_coercion::source("cryptocom");
    assert_eq!(source["cases"].as_array().unwrap().len(), 96);
    let mut opts = serde_json::to_value(options()).unwrap();
    opts["publicDataOnly"] = json!(true);
    let h = harness(serde_json::from_value(opts).unwrap(), |http| {
        http.ok("GET", "/public/get-instruments", instruments());
        http.ok("GET", "/public/get-tickers", tickers());
    });
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let template = h.store.markets().unwrap()[0].clone();
    let markets = ticker_coercion::seed("cryptocom")
        .into_iter()
        .map(|ticker| {
            let mut market = template.clone();
            market.info =
                Some(json!({"underlyingSymbol": ticker.id.split('-').next().unwrap()}).into());
            market.id = ticker.id;
            market.symbol = ticker.symbol;
            market
        })
        .collect();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let socket = market_socket(&h);
    for case in source["cases"].as_array().unwrap() {
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                tickers: Some(ticker_coercion::seed("cryptocom")),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        for frame in case["frames"].as_array().unwrap() {
            h.runtime.now.set(h.runtime.now.get() + 1_100.0);
            socket
                .events
                .borrow_mut()
                .push_back(WsEvent::Message(SocketFrame::Text(
                    frame.as_str().unwrap().into(),
                )));
            h.runtime.pump();
        }
        ticker_coercion::assert_native(&h.store.tickers().unwrap(), case);
    }
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    h.venue().dispose().unwrap();
}
