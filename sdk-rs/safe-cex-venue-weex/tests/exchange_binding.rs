#[path = "support/binding_runtime.rs"]
#[allow(dead_code)]
mod binding_runtime;
use binding_runtime::*;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::Store;
use safe_cex_types::*;
use serde_json::{Value, json};
fn endpoint(name: &str) -> &'static str {
    safe_cex_venue_weex::types::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == name)
        .unwrap()
        .1
}
fn raw_market() -> Value {
    json!({"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":3,"minOrderSize":"0.001","maxOrderSize":"50","minLeverage":"1","maxLeverage":"80","contractVal":"1"})
}
fn market() -> Market {
    serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":50},"leverage":{"min":1,"max":80}}})).unwrap()
}
fn raw_order() -> Value {
    json!({"orderId":"native-9","symbol":"BTCUSDT","side":"BUY","type":"LIMIT","status":"NEW","origQty":"2","price":"101.5","executedQty":"0.25","timeInForce":"GTC","createdTime":1700000000000.0})
}
fn order() -> Order {
    serde_json::from_value(json!({"id":"native-9","symbol":"BTCUSDT","side":"buy","type":"limit","status":"open","amount":2,"price":101.5,"filled":0.25,"remaining":1.75,"lastUpdate":1700000000000.0,"reduceOnly":false,"info":{},"timestamp":1700000000000.0})).unwrap()
}
fn place() -> PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":"BTCUSDT","side":"buy","type":"limit","price":101.5,"amount":2,"clientOrderId":"synthetic-client-1"})).unwrap()
}
fn seed(h: &Harness) {
    Store::update(
        h.store.as_ref(),
        serde_json::from_value(json!({"markets":[market()]})).unwrap(),
        false,
    )
    .unwrap();
}
fn script(http: &Http) {
    for (name, data) in [
        ("EXCHANGE_INFO", json!({"symbols":[raw_market()]})),
        (
            "TICKER_24HR",
            json!([{"symbol":"BTCUSDT","lastPrice":"101.5","markPrice":"102.25","indexPrice":"103.75","volume":"12","quoteVolume":"1200","priceChangePercent":"3.5"}]),
        ),
        (
            "BOOK_TICKER",
            json!([{"symbol":"BTCUSDT","bidPrice":"101.4","askPrice":"101.6"}]),
        ),
        (
            "ACCOUNT_CONFIG",
            json!({"canTrade":true,"dualSidePosition":false}),
        ),
        (
            "BALANCE",
            json!([{"asset":"USDT","balance":"150","availableBalance":"120","unrealizePnl":"3"}]),
        ),
        (
            "POSITIONS",
            json!([{"symbol":"BTCUSDT","positionSide":"LONG","size":"2","openValue":"200","leverage":"20","marginType":"CROSS","updatedTime":1700000000000.0}]),
        ),
        (
            "SYMBOL_CONFIG",
            json!([{"symbol":"BTCUSDT","marginType":"CROSS","separatedMode":"COMBINED","crossLeverage":"20","isolatedLongLeverage":"11","isolatedShortLeverage":"13"}]),
        ),
        ("OPEN_ORDERS", json!([raw_order()])),
        ("OPEN_ALGO_ORDERS", json!([])),
        (
            "DEPTH",
            json!({"bids":[["101.4","2"]],"asks":[["101.6","3"]],"lastUpdateId":"17"}),
        ),
        (
            "PUBLIC_TRADES",
            json!([{"id":"trade-7","price":"101.25","qty":"0.5","side":"SELL","time":1700000000000.0}]),
        ),
        (
            "KLINES",
            json!([[1700000000000.0, "100", "103", "99", "102", "12"]]),
        ),
        ("USER_TRADES", json!([])),
    ] {
        http.ok("GET", endpoint(name), data);
    }
    for name in [
        "ORDER",
        "BATCH_ORDERS",
        "ALGO_ORDER",
        "SET_LEVERAGE",
        "SET_MARGIN_TYPE",
        "SET_POSITION_MODE",
        "INCOME",
    ] {
        if let Some((_, path)) = safe_cex_venue_weex::types::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == name)
        {
            http.ok(
                "POST",
                path,
                if name == "BATCH_ORDERS" {
                    json!([{"orderId":"placed-8","clientOrderId":"synthetic-client-1","success":true}])
                } else {
                    json!({"orderId":"placed-8"})
                },
            );
        }
    }
    for name in [
        "BATCH_ORDERS",
        "ALGO_ORDER",
        "CANCEL_ALL_OPEN_ORDERS",
        "CANCEL_ALL_ALGO_ORDERS",
    ] {
        http.ok("DELETE", endpoint(name), json!({"success":true}));
    }
}
fn setup() -> Harness {
    let h = harness(options(), script);
    seed(&h);
    h
}
fn body(request: &safe_cex_transport::RestRequest) -> Value {
    serde_json::from_slice(&request.body.as_ref().unwrap().wire_bytes().unwrap()).unwrap()
}
fn expect_ok(result: Result<(), ExchangeError>) {
    result.unwrap();
}
#[test]
fn production_constructor_assembles_both_source_sockets_and_core_settings() {
    let h = setup();
    assert_eq!(h.venue().name(), &ExchangeName::Weex);
    assert_eq!(h.venue().class_name(), "WeexExchange");
    assert_eq!(h.venue().base().get_all_sockets().len(), 2);
    assert!(h.runtime.dials.borrow().is_empty());
    h.venue().dispose().unwrap();
    assert!(h.venue().base().is_disposed());
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
}
#[test]
fn get_account_e2e() {
    let h = setup();
    let account = h.drive(h.venue().get_account()).unwrap();
    assert!(account.user_id.starts_with("weex:"));
    assert_eq!(account.user_id.len(), 29);
    assert_eq!(h.http.calls(endpoint("ACCOUNT_CONFIG")).len(), 1);
    let calls = h.http.calls(endpoint("ACCOUNT_CONFIG"));
    assert_eq!(
        calls[0].headers.get("ACCESS-KEY").map(String::as_str),
        Some("synthetic-key")
    );
    assert!(
        calls[0]
            .headers
            .get("ACCESS-SIGN")
            .is_some_and(|signature| !signature.is_empty())
    );
}
#[test]
fn validate_account_e2e() {
    let h = setup();
    let account = h.drive(h.venue().validate_account()).unwrap();
    assert!(account.error.as_value().as_str().unwrap().is_empty());
    assert!(account.balance.is_some());
    assert_eq!(h.http.calls(endpoint("BALANCE")).len(), 1);
    assert_eq!(h.http.calls(endpoint("POSITIONS")).len(), 1);
}
#[test]
fn fetch_markets_e2e() {
    let h = setup();
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].symbol, "BTCUSDT");
    assert_eq!(markets[0].precision.price, 0.1);
    assert_eq!(markets[0].limits.leverage.max, 80.0);
    assert_eq!(h.http.calls(endpoint("EXCHANGE_INFO")).len(), 1);
}
#[test]
fn fetch_tickers_e2e() {
    let h = setup();
    let tickers = h.drive(h.venue().fetch_tickers()).unwrap();
    assert_eq!(tickers.len(), 1);
    assert_eq!(tickers[0].last, 101.5);
    assert_eq!(tickers[0].mark, 102.25);
    assert_eq!(tickers[0].bid, 101.4);
    assert_eq!(h.http.calls(endpoint("TICKER_24HR")).len(), 1);
    assert_eq!(h.http.calls(endpoint("BOOK_TICKER")).len(), 1);
}
#[test]
fn fetch_positions_e2e() {
    let h = setup();
    let positions = h
        .drive(h.venue().fetch_positions(Some(vec!["BTCUSDT".into()])))
        .unwrap();
    assert_eq!(positions[0].symbol, "BTCUSDT");
    assert_eq!(positions[0].contracts, 2.0);
    assert_eq!(positions[0].entry_price, 100.0);
    assert_eq!(h.http.calls(endpoint("POSITIONS")).len(), 1);
}
#[test]
fn fetch_orders_e2e() {
    let h = setup();
    let orders = h
        .drive(h.venue().fetch_orders(Some(vec!["BTCUSDT".into()])))
        .unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "native-9");
    assert_eq!(orders[0].remaining, 1.75);
    assert_eq!(orders[0].side, OrderSide::Buy);
    assert_eq!(h.http.calls(endpoint("OPEN_ORDERS")).len(), 1);
    assert_eq!(h.http.calls(endpoint("OPEN_ALGO_ORDERS")).len(), 1);
}
#[test]
fn fetch_symbol_max_leverage_e2e() {
    let h = setup();
    assert_eq!(
        h.drive(h.venue().fetch_symbol_max_leverage("BTCUSDT".into()))
            .unwrap(),
        80.0
    );
    assert!(h.http.sent.lock().unwrap().is_empty());
}
#[test]
fn set_leverage_e2e() {
    let h = setup();
    expect_ok(h.drive(h.venue().set_leverage("BTCUSDT".into(), 500.0, None)));
    let calls = h.http.calls(endpoint("SET_LEVERAGE"));
    assert_eq!(calls.len(), 1);
    assert_eq!(body(&calls[0])["crossLeverage"], "80");
    assert_eq!(h.http.calls(endpoint("SYMBOL_CONFIG")).len(), 2);
}
#[test]
fn change_margin_type_e2e() {
    let h = setup();
    expect_ok(h.drive(h.venue().change_margin_type(true, Some("BTCUSDT".into()))));
    let calls = h.http.calls(endpoint("SET_MARGIN_TYPE"));
    assert_eq!(calls.len(), 1);
    assert_eq!(body(&calls[0])["marginType"], "ISOLATED");
}
#[test]
fn change_position_mode_e2e() {
    let h = setup();
    expect_ok(h.drive(h.venue().change_position_mode(true, Some("BTCUSDT".into()))));
    let calls = h.http.calls(endpoint("SET_MARGIN_TYPE"));
    assert_eq!(calls.len(), 1);
    assert_eq!(body(&calls[0])["separatedType"], "SEPARATED");
    assert!(h.store.options().is_hedged);
}
#[test]
fn rejected_order_leg_keeps_successful_orders_without_error_event() {
    let h = harness(options(), |http| {
        http.route(
            "GET",
            endpoint("OPEN_ALGO_ORDERS"),
            400,
            json!({"msg":"algo endpoint unavailable"}),
        );
        script(http);
    });
    seed(&h);
    let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let captured = errors.clone();
    h.venue().base().on(
        "error",
        std::rc::Rc::new(move |_, value: &Value| {
            captured.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let orders = h
        .drive(h.venue().fetch_orders(None))
        .expect("allSettled retains the successful order leg");
    assert_eq!(h.http.calls(endpoint("OPEN_ORDERS")).len(), 1);
    assert_eq!(h.http.calls(endpoint("OPEN_ALGO_ORDERS")).len(), 1);
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "native-9");
    assert!(
        errors.borrow().is_empty(),
        "source allSettled must swallow rejected legs, emitted {:?}",
        errors.borrow()
    );
}

#[test]
fn place_order_e2e() {
    let h = setup();
    let ids = h
        .drive(Box::pin(
            h.venue().base().place_order(h.exchange.as_ref(), place()),
        ))
        .unwrap();
    assert_eq!(ids, ["placed-8"]);
    let calls = h.http.calls(endpoint("ORDER"));
    assert_eq!(calls.len(), 1);
    assert_eq!(body(&calls[0])["symbol"], "BTCUSDT");
    assert_eq!(body(&calls[0])["newClientOrderId"], "synthetic-client-1");
}
#[test]
fn cancel_orders_e2e() {
    let h = setup();
    let order = order();
    Store::update(
        h.store.as_ref(),
        serde_json::from_value(json!({"orders":[order.clone()]})).unwrap(),
        false,
    )
    .unwrap();
    expect_ok(h.drive(h.venue().cancel_orders(vec![order])));
    let calls = h.http.calls(endpoint("BATCH_ORDERS"));
    assert_eq!(calls.len(), 1);
    assert_eq!(body(&calls[0])["orderIdList"], json!(["native-9"]));
    assert_eq!(h.store.orders()[0].status, OrderStatus::Canceled);
}
#[test]
fn update_order_e2e() {
    let h = setup();
    let update =
        serde_json::from_value(json!({"order":order(),"update":{"price":103.5,"amount":3}}))
            .unwrap();
    let ids = h.drive(h.venue().update_order(update)).unwrap();
    assert_eq!(ids, ["placed-8"]);
    assert_eq!(h.http.calls(endpoint("BATCH_ORDERS")).len(), 1);
    let calls = h.http.calls(endpoint("ORDER"));
    assert_eq!(calls.len(), 1);
    assert_eq!(body(&calls[0])["price"], "103.5");
}
#[test]
fn fetch_public_trades_e2e() {
    let h = setup();
    let trades = h
        .drive(h.venue().fetch_public_trades("BTCUSDT".into(), Some(5.0)))
        .unwrap();
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].symbol, "BTCUSDT");
    assert_eq!(trades[0].price, 101.25);
    assert_eq!(trades[0].qty, 0.5);
    assert_eq!(h.http.calls(endpoint("PUBLIC_TRADES")).len(), 1);
}
#[test]
fn schedule_cancel_e2e() {
    let h = setup();
    let error = h
        .drive(h.venue().schedule_cancel(Some(Some(500.0))))
        .unwrap_err();
    assert_eq!(error.message, "Not implemented");
    assert!(h.http.sent.lock().unwrap().is_empty());
}
#[test]
fn start_e2e() {
    let h = setup();
    expect_ok(h.drive(h.venue().start()));
    assert!(h.store.loaded().markets);
    assert!(h.store.loaded().tickers);
    assert!(h.store.loaded().orders);
    assert!(h.store.loaded().balance);
    assert!(!h.runtime.dials.borrow().is_empty());
    h.venue().dispose().unwrap();
}
#[test]
fn fetch_ohlcv_e2e() {
    let h = setup();
    let opts =
        serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m","limit":1})).unwrap();
    let candles = h.drive(h.venue().fetch_ohlcv(opts)).unwrap();
    assert_eq!(candles.len(), 1);
    assert_eq!(candles[0].close, 102.0);
    assert_eq!(h.http.calls(endpoint("KLINES")).len(), 1);
}

#[test]
fn private_auth_headers_are_fresh_on_each_core_owned_dial() {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use safe_cex_transport::relay::UpgradeHeaders;
    let h = setup();
    let socket = h.venue().base().get_private_sockets()[0].clone();
    for delta in [0.0, 17.0] {
        h.runtime.advance(delta);
        let before = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
        socket.connect_and_subscribe(delta > 0.0).unwrap();
        h.runtime.pump();
        let after = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
        let dials = h.runtime.dials.borrow();
        let (_, options) = dials.last().unwrap();
        assert!(options.headers_required);
        let Some(UpgradeHeaders::Static(headers)) = &options.headers else {
            panic!("captured upgrade headers")
        };
        assert_eq!(headers.get("ACCESS-KEY").unwrap(), "synthetic-key");
        let timestamp = headers.get("ACCESS-TIMESTAMP").unwrap();
        let parsed = timestamp.parse::<f64>().unwrap();
        assert!(parsed >= before && parsed <= after);
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"synthetic-secret").unwrap();
        mac.update(format!("{timestamp}/v3/ws/private").as_bytes());
        assert_eq!(
            headers.get("ACCESS-SIGN").unwrap(),
            &base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
        );
    }
    assert_eq!(h.runtime.dials.borrow().len(), 2);
    h.venue().dispose().unwrap();
}
#[test]
fn incapable_private_transport_never_dials_and_rest_start_stays_available() {
    let h = setup();
    h.runtime.headers_capable.set(false);
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    assert!(
        h.runtime
            .dials
            .borrow()
            .iter()
            .all(|(_, options)| !options.headers_required)
    );
    assert!(h.store.loaded().balance);
    h.venue().dispose().unwrap();
}
#[test]
fn native_public_subscriptions_deliver_candles_and_book_deltas_then_cleanup() {
    use safe_cex_exchange::runtime::ExchangeFuture;
    use std::{cell::RefCell, rc::Rc};
    let h = setup();
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    let candles = Rc::new(RefCell::new(Vec::<Candle>::new()));
    let delivered = candles.clone();
    let cleanup = h
        .venue()
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap(),
            Rc::new(move |c| delivered.borrow_mut().push(c)),
        )
        .unwrap();
    let books = Rc::new(RefCell::new(Vec::<OrderBook>::new()));
    let delivered = books.clone();
    let cleanup_book = h
        .venue()
        .listen_order_book(
            "BTCUSDT".into(),
            Rc::new(move |b| delivered.borrow_mut().push(b)),
        )
        .unwrap();
    h.runtime.pump();
    let socket = h.venue().base().get_public_sockets()[0].clone();
    socket.on_message(&safe_cex_transport::socket::SocketFrame::Text(json!({"e":"kline","s":"BTCUSDT","d":[{"i":"1m","t":1700000000000.0,"o":"100","h":"103","l":"99","c":"102","v":"12"}]}).to_string())).unwrap();
    assert_eq!(candles.borrow().len(), 1);
    assert_eq!(candles.borrow()[0].close, 102.0);
    socket
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(
            json!({"e":"depth","s":"BTCUSDT","U":18,"u":18,"b":[["101.4","4"]],"a":[]}).to_string(),
        ))
        .unwrap();
    assert_eq!(books.borrow().last().unwrap().bids[0].amount, 4.0);
    assert_eq!(books.borrow().last().unwrap().bids[0].total, 4.0);
    cleanup().unwrap();
    cleanup_book().unwrap();
    socket
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(
            json!({"e":"kline","s":"BTCUSDT","d":[{"i":"1m","t":1700000000000.0,"c":"500"}]})
                .to_string(),
        ))
        .unwrap();
    assert_eq!(candles.borrow().len(), 1);
    h.venue().dispose().unwrap();
    // Ensure no pending snapshot observation needs a different executor.
    let ready: ExchangeFuture<'_, ()> = Box::pin(async { Ok(()) });
    expect_ok(h.drive(ready));
}
#[test]
fn private_fill_reaches_emitter_and_disposal_cancels_refresh_timers() {
    use std::{cell::RefCell, rc::Rc};
    let h = setup();
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    let fills = Rc::new(RefCell::new(Vec::<Value>::new()));
    let delivered = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, value| {
            delivered.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let socket = h.venue().base().get_private_sockets()[0].clone();
    socket.on_message(&safe_cex_transport::socket::SocketFrame::Text(json!({"e":"fill","d":[{"id":"fill-42","orderId":"native-9","orderSide":"SELL","symbol":"BTCUSDT","fillValue":"202.5","fillSize":"2","fillFee":"0.1","realizePnl":"3.5","createdTime":1700000000000.0}]}).to_string())).unwrap();
    assert_eq!(fills.borrow().len(), 1);
    assert_eq!(fills.borrow()[0]["id"], "fill-42");
    assert_eq!(fills.borrow()[0]["side"], "buy");
    assert_eq!(fills.borrow()[0]["price"], 101.25);
    let before = h.http.sent.lock().unwrap().len();
    h.venue().dispose().unwrap();
    h.runtime.advance(300.0);
    assert_eq!(
        h.http.sent.lock().unwrap().len(),
        before,
        "source socket dispose cancels its refresh callbacks"
    );
    assert!(socket.is_force_closed());
}
#[test]
fn batch_placement_uses_native_registration_and_caller_id_guard() {
    let h = setup();
    let ids = h
        .drive(Box::pin(
            h.venue()
                .base()
                .place_orders(h.exchange.as_ref(), vec![place()]),
        ))
        .unwrap();
    assert_eq!(ids, ["placed-8"]);
    let calls = h.http.calls(endpoint("BATCH_ORDERS"));
    assert_eq!(calls.len(), 1);
    assert_eq!(
        body(&calls[0])["batchOrders"][0]["newClientOrderId"],
        "synthetic-client-1"
    );
    let mut invalid = place();
    invalid.client_order_id = Some("bad?".into());
    let before = h.http.sent.lock().unwrap().len();
    assert!(
        h.drive(Box::pin(
            h.venue()
                .base()
                .place_orders(h.exchange.as_ref(), vec![place(), invalid])
        ))
        .is_err()
    );
    assert_eq!(h.http.sent.lock().unwrap().len(), before);
}
#[test]
fn response_transform_retains_large_order_ids_and_parses_error_bodies() {
    let h = harness(options(), |http| {
        script(http);
        http.ok("GET",endpoint("OPEN_ORDERS"),json!([{ "orderId":9007199254740993u64,"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","status":"NEW"}]));
    });
    seed(&h);
    // First scripted page is consumed before the distinct large-id response.
    h.drive(h.venue().fetch_orders(None)).unwrap();
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    assert_eq!(orders[0].id, "9007199254740993");
    let h = harness(options(), |http| {
        http.route(
            "GET",
            endpoint("ACCOUNT_CONFIG"),
            400,
            json!({"msg":"synthetic-denied"}),
        )
    });
    assert_eq!(
        h.drive(h.venue().get_account()).unwrap_err().message,
        "synthetic-denied"
    );
}

#[test]
fn income_page_and_range_use_bound_post_normalizer_and_shared_pager() {
    let records = json!([
        {"symbol":"BTCUSDT","incomeType":"FUNDING_FEE","income":"1.25","asset":"USDT","time":1700000000000.0,"billId":"income-a"},
        {"symbol":"BTCUSDT","incomeType":"FUNDING_FEE","income":"2.5","asset":"USDT","time":1700000000100.0,"billId":"income-b"}
    ]);
    let h = harness(options(), |http| {
        http.ok("POST", endpoint("INCOME"), records)
    });
    let page = h.drive(h.venue().fetch_income_history(None)).unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].income, 1.25);
    assert_eq!(page[0].category, Some(IncomeCategory::Funding));
    let range = h
        .drive(h.venue().fetch_income_history_range(
            1700000000000.0,
            1700000000100.0,
            Some("FUNDING_FEE".into()),
        ))
        .unwrap();
    assert_eq!(range.len(), 2);
    assert_eq!(
        range
            .iter()
            .map(|record| record.tran_id.as_str())
            .collect::<Vec<_>>(),
        ["income-b", "income-a"]
    );
    let calls = h.http.calls(endpoint("INCOME"));
    assert!(calls.len() >= 2);
    assert_eq!(body(&calls[1])["startTime"], 1700000000000.0);
    assert_eq!(body(&calls[1])["incomeType"], "FUNDING_FEE");
}

#[test]
fn trait_fetch_leverage_uses_cross_config_instead_of_market_maximum() {
    let h = setup();
    let actual = h
        .drive(h.venue().fetch_leverage_for_symbol("BTCUSDT".into()))
        .unwrap()
        .unwrap();
    assert_eq!(
        actual.buy_leverage, 20.0,
        "user leverage comes from the symbol config"
    );
    assert_eq!(actual.sell_leverage, 20.0);
    assert_eq!(h.store.fetched_leverage()["BTCUSDT"], actual);
    assert_eq!(h.http.calls(endpoint("SYMBOL_CONFIG")).len(), 1);
    assert!(h.http.calls(endpoint("EXCHANGE_INFO")).is_empty());
}

#[test]
fn trait_fetch_leverage_retains_distinct_isolated_sides() {
    let h = harness(options(), |http| {
        http.ok("GET", endpoint("SYMBOL_CONFIG"), json!([{"symbol":"BTCUSDT","marginType":"ISOLATED","separatedMode":"SEPARATED","crossLeverage":"20","isolatedLongLeverage":"11","isolatedShortLeverage":"13"}]));
        http.ok(
            "GET",
            endpoint("EXCHANGE_INFO"),
            json!({"symbols":[raw_market()]}),
        );
    });
    seed(&h);
    let actual = h
        .drive(h.venue().fetch_leverage_for_symbol("BTCUSDT".into()))
        .unwrap()
        .unwrap();
    assert_eq!(actual.buy_leverage, 11.0);
    assert_eq!(actual.sell_leverage, 13.0);
    assert_eq!(h.store.fetched_leverage()["BTCUSDT"], actual);
    assert!(h.http.calls(endpoint("EXCHANGE_INFO")).is_empty());
}

fn generated_order_id(order_type: OrderType, endpoint_name: &str, key: &str) {
    let h = setup();
    *h.runtime.id.borrow_mut() = Some("A_b-cD_e-fG_h-iJ_k-lM".into());
    let mut opts = place();
    opts.client_order_id = None;
    opts.r#type = order_type;
    if opts.r#type == OrderType::Conditional {
        opts.trigger_price = Some(100.0);
    }
    h.http.ok(
        "POST",
        endpoint(endpoint_name),
        json!({"orderId":"placed-8"}),
    );
    let ids = h
        .drive(Box::pin(
            h.venue().base().place_order(h.exchange.as_ref(), opts),
        ))
        .unwrap();
    assert_eq!(ids, ["placed-8"]);
    let calls = h.http.calls(endpoint(endpoint_name));
    assert_eq!(calls.len(), 1);
    assert_eq!(
        body(&calls[0])[key],
        "tsAbcDefGhiJklM",
        "production uuid must strip both nanoid separators"
    );
    assert!(!calls[0].headers["ACCESS-SIGN"].is_empty());
}
#[test]
fn generated_regular_order_id_strips_nanoid_separators() {
    generated_order_id(OrderType::Limit, "ORDER", "newClientOrderId");
}
#[test]
fn generated_conditional_order_id_strips_nanoid_separators() {
    generated_order_id(OrderType::Conditional, "ALGO_ORDER", "clientAlgoId");
}
#[test]
fn generated_protective_order_id_strips_nanoid_separators() {
    generated_order_id(OrderType::StopLoss, "PLACE_TP_SL_ORDER", "clientAlgoId");
}
#[test]
fn generated_minted_order_id_strips_nanoid_separators() {
    let h = setup();
    *h.runtime.id.borrow_mut() = Some("A_b-cD_e-fG_h-iJ_k-lM".into());
    assert_eq!(
        h.venue().base().mint_client_order_id(place()).as_deref(),
        Some("tsAbcDefGhiJklM")
    );
}

#[test]
fn private_order_frame_commits_the_normalized_order() {
    let h = setup();
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    let mut row = raw_order();
    row["price"] = json!("107.5");
    row["executedQty"] = json!("0.75");
    h.venue().base().get_private_sockets()[0]
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(
            json!({"e":"orders","d":[row]}).to_string(),
        ))
        .unwrap();
    let orders = h.store.orders();
    let order = orders.iter().find(|o| o.id == "native-9").unwrap();
    assert_eq!(order.price, 107.5);
    assert_eq!(order.filled, 0.75);
    assert_eq!(order.remaining, 1.25);
}
#[test]
fn private_position_frame_commits_the_normalized_position() {
    let h = setup();
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    h.venue().base().get_private_sockets()[0]
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(json!({"e":"positions","d":[{"symbol":"BTCUSDT","positionSide":"LONG","size":"7","openValue":"714","leverage":"14","marginType":"CROSS","updatedTime":1700000000000.0}]}).to_string())).unwrap();
    let positions = h.store.positions();
    let position = positions
        .iter()
        .find(|p| p.symbol == "BTCUSDT" && p.side == PositionSide::Long)
        .unwrap();
    assert_eq!(position.contracts, 7.0);
    assert_eq!(position.entry_price, 102.0);
    assert_eq!(position.leverage, 14.0);
}
#[test]
fn private_account_frame_refreshes_balance_from_rest() {
    let h = setup();
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    Store::update(
        h.store.as_ref(),
        serde_json::from_value(json!({"balance":{"total":1,"free":1,"used":0,"upnl":0}})).unwrap(),
        false,
    )
    .unwrap();
    let before = h.http.calls(endpoint("BALANCE")).len();
    h.venue().base().get_private_sockets()[0]
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(
            json!({"e":"account","d":[{"equity":"999"}]}).to_string(),
        ))
        .unwrap();
    h.runtime.pump();
    h.runtime.advance(250.0);
    h.runtime.pump();
    assert_eq!(h.http.calls(endpoint("BALANCE")).len(), before + 1);
    assert_eq!(h.store.balance().total, 150.0);
    assert_eq!(h.store.balance().free, 120.0);
    assert_eq!(h.store.balance().upnl, 3.0);
}
