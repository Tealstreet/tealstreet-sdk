#[path = "support/binding_runtime.rs"]
#[allow(dead_code)]
mod binding_runtime;
use binding_runtime::*;
use safe_cex_exchange::{js_runtime::value::to_typed, runtime::ExchangeError};
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
                    json!([{"orderId":"1008","clientOrderId":"synthetic-client-1","success":true}])
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
fn response_array_cases(row: Value) -> Vec<(&'static str, Value, bool)> {
    let rows = json!([row]);
    vec![
        ("array", rows.clone(), true),
        ("data", json!({"data":rows}), true),
        ("items", json!({"items":rows}), false),
        ("list", json!({"list":rows}), false),
        ("rows", json!({"rows":rows}), false),
        ("data_items", json!({"data":{"items":rows}}), false),
        ("data_list", json!({"data":{"list":rows}}), false),
        ("data_rows", json!({"data":{"rows":rows}}), false),
    ]
}
#[test]
fn positions_response_array_keys_match_source() {
    let row = json!({"symbol":"BTCUSDT","positionSide":"LONG","size":"2","openValue":"200","leverage":"20","marginType":"CROSS","updatedTime":1700000000000.0});
    for (name, payload, accepted) in response_array_cases(row) {
        let h = harness(options(), |http| {
            http.ok("GET", endpoint("POSITIONS"), payload);
            script(http);
        });
        seed(&h);
        let positions = h
            .drive(h.venue().fetch_positions(Some(vec!["BTCUSDT".into()])))
            .unwrap();
        assert!(!positions.is_empty(), "{name}: retain fake zero positions");
        assert!(
            positions
                .iter()
                .all(|position| position.symbol == "BTCUSDT")
        );
        let open = positions
            .iter()
            .filter(|position| position.contracts != 0.0)
            .collect::<Vec<_>>();
        assert_eq!(
            open.len(),
            usize::from(accepted),
            "{name}: source array admission"
        );
        if accepted {
            assert_eq!(open[0].contracts, 2.0, "{name}");
            assert_eq!(open[0].entry_price, 100.0, "{name}");
        }
        assert_eq!(h.http.calls(endpoint("POSITIONS")).len(), 1, "{name}");
    }
}
#[test]
fn fills_response_array_keys_match_source() {
    let row = json!({"symbol":"BTCUSDT","id":"fill-1","orderId":"order-1","side":"BUY","price":"101.5","qty":"2","time":1700000000000.0});
    for (name, payload, accepted) in response_array_cases(row) {
        let h = harness(options(), |http| {
            http.ok("GET", endpoint("USER_TRADES"), payload);
            script(http);
        });
        seed(&h);
        let fills = h
            .drive(h.venue().fetch_historical_fills(
                serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap(),
            ))
            .unwrap();
        assert_eq!(
            fills.len(),
            usize::from(accepted),
            "{name}: source array admission"
        );
        if accepted {
            assert_eq!(fills[0].id.as_deref(), Some("fill-1"), "{name}");
            assert_eq!(fills[0].price, 101.5, "{name}");
            assert_eq!(fills[0].qty, 2.0, "{name}");
        }
        assert_eq!(h.http.calls(endpoint("USER_TRADES")).len(), 1, "{name}");
    }
}
fn historical_fill_row(id: &str, maker: Option<Value>, time: i64) -> Value {
    let mut row = json!({"symbol":"BTCUSDT","id":id,"orderId":"order-1","side":"BUY", "price":"101.5","qty":"2","time":time,"commissionAsset":"USDT"});
    if let Some(maker) = maker {
        row["maker"] = maker;
    }
    row
}
fn historical_fill_logs(h: &Harness) -> std::rc::Rc<std::cell::RefCell<Vec<Value>>> {
    let logs = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let captured = logs.clone();
    h.venue().base().on_args(
        "log",
        std::rc::Rc::new(move |_, args| {
            let message = to_typed::<String>(&args[0])?;
            let severity = to_typed::<String>(&args[1])?;
            captured
                .borrow_mut()
                .push(json!({"message":message,"severity":severity}));
            Ok(())
        }),
    );
    logs
}
#[test]
fn historical_fills_closed_maker_refusals_keep_good_rows_and_report_values() {
    for bad in [
        json!("true"),
        json!("false"),
        json!(1),
        json!(0),
        Value::Null,
        json!({}),
    ] {
        let rows = json!([
            historical_fill_row("before", Some(json!(false)), 1700000000000),
            historical_fill_row("refused", Some(bad.clone()), 1700000001000),
            historical_fill_row("after", Some(json!(true)), 1700000002000),
            historical_fill_row("missing", None, 1700000003000)
        ]);
        let h = harness(options(), |http| {
            http.ok("GET", endpoint("USER_TRADES"), rows);
            script(http);
        });
        seed(&h);
        let logs = historical_fill_logs(&h);
        let fills = h
            .drive(h.venue().fetch_historical_fills(
                serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap(),
            ))
            .expect("a closed maker field refuses only its row");
        assert_eq!(fills.len(), 3, "bad maker {bad}");
        for (id, maker) in [
            ("before", Some(false)),
            ("after", Some(true)),
            ("missing", None),
        ] {
            let fill = fills
                .iter()
                .find(|fill| fill.id.as_deref() == Some(id))
                .unwrap();
            assert_eq!(
                fill.maker, maker,
                "{id}: no boolean coercion or fabricated flag"
            );
            assert_eq!((fill.price, fill.qty), (101.5, 2.0));
            assert_eq!(fill.commission_asset.as_deref(), Some("USDT"));
        }
        let logs = logs.borrow();
        assert_eq!(logs.len(), 1, "each refused row needs a visible report");
        assert_eq!(logs[0]["severity"], "warning");
        let message = logs[0]["message"].as_str().unwrap();
        assert!(message.contains("Execution.maker"), "{message}");
        assert!(message.contains(&format!("received: {bad}")), "{message}");
        assert_eq!(h.http.calls(endpoint("USER_TRADES")).len(), 1);
    }
}
#[test]
fn historical_fills_closed_asset_refusals_keep_good_rows_and_report_values() {
    for (field, bad) in [
        ("commissionAsset", json!(17)),
        ("commissionAsset", json!(true)),
        ("commissionAsset", json!({"asset":"USDT"})),
        ("coin", json!(17)),
    ] {
        let mut refused = historical_fill_row("refused", Some(json!(false)), 1700000001000);
        refused.as_object_mut().unwrap().remove("commissionAsset");
        refused[field] = bad.clone();
        let h = harness(options(), |http| {
            http.ok(
                "GET",
                endpoint("USER_TRADES"),
                json!([
                    historical_fill_row("before", None, 1700000000000),
                    refused,
                    historical_fill_row("after", Some(json!(true)), 1700000002000)
                ]),
            );
            script(http);
        });
        seed(&h);
        let logs = historical_fill_logs(&h);
        let fills = h
            .drive(h.venue().fetch_historical_fills(
                serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap(),
            ))
            .expect("a closed commission asset refuses only its row");
        assert_eq!(fills.len(), 2);
        assert!(
            fills
                .iter()
                .any(|fill| fill.id.as_deref() == Some("before"))
        );
        assert!(fills.iter().any(|fill| fill.id.as_deref() == Some("after")));
        assert!(
            fills
                .iter()
                .all(|fill| fill.commission_asset.as_deref() == Some("USDT"))
        );
        let logs = logs.borrow();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["severity"], "warning");
        let message = logs[0]["message"].as_str().unwrap();
        assert!(message.contains("Execution.commissionAsset"), "{message}");
        assert!(message.contains(&format!("received: {bad}")), "{message}");
        assert_eq!(h.http.calls(endpoint("USER_TRADES")).len(), 1);
    }
}
#[test]
fn historical_fills_closed_all_refused_rows_return_empty_with_each_report() {
    let mut asset = historical_fill_row("asset", None, 1700000001000);
    asset["commissionAsset"] = json!(17);
    let h = harness(options(), |http| {
        http.ok(
            "GET",
            endpoint("USER_TRADES"),
            json!([
                historical_fill_row("maker", Some(json!("true")), 1700000000000),
                asset
            ]),
        );
        script(http);
    });
    seed(&h);
    let logs = historical_fill_logs(&h);
    let fills = h
        .drive(
            h.venue().fetch_historical_fills(
                serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap(),
            ),
        )
        .expect("all refused rows resolve after reporting each refusal");
    assert!(fills.is_empty());
    assert_eq!(logs.borrow().len(), 2);
    assert_eq!(h.http.calls(endpoint("USER_TRADES")).len(), 1);
}
#[test]
fn historical_fills_closed_refusal_does_not_replace_source_coercion_error() {
    let mut bad = historical_fill_row("coercion", Some(Value::Null), 1700000001000);
    bad["commissionAsset"] = json!(17);
    bad["commission"] = json!({"toString":null});
    let h = harness(options(), |http| {
        http.ok(
            "GET",
            endpoint("USER_TRADES"),
            json!([
                historical_fill_row("before", None, 1700000000000),
                bad,
                historical_fill_row("after", None, 1700000002000)
            ]),
        );
        script(http);
    });
    seed(&h);
    let logs = historical_fill_logs(&h);
    let error = h
        .drive(
            h.venue().fetch_historical_fills(
                serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap(),
            ),
        )
        .expect_err("source commission conversion runs before closed maker admission");
    assert!(
        error
            .message
            .contains("Cannot convert object to primitive value"),
        "{error:?}"
    );
    assert!(
        logs.borrow().is_empty(),
        "source errors are not typed-field refusals"
    );
    assert_eq!(h.http.calls(endpoint("USER_TRADES")).len(), 1);
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
        std::rc::Rc::new(move |_, value: &safe_cex_exchange::js_runtime::value::Js| {
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
fn accepted_post_only_placement_updates_the_real_store() {
    for has_stored_order in [false, true] {
        let h = harness(options(), |http| {
            // Queue the accepted response before the generic fallback fixture.
            http.ok("POST", endpoint("ORDER"), json!({"orderId":"1008"}));
            script(http);
        });
        seed(&h);
        if has_stored_order {
            let mut existing = order();
            existing.id = "1008".into();
            existing.time_in_force = ModelField::Value(OrderTimeInForce::GoodTillCancel);
            Store::update(
                h.store.as_ref(),
                serde_json::from_value(json!({"orders":[existing]})).unwrap(),
                false,
            )
            .unwrap();
        }
        let mut opts = place();
        opts.time_in_force = Some(OrderTimeInForce::PostOnly);
        let outcome = h.drive(Box::pin(
            h.venue().base().place_order(h.exchange.as_ref(), opts),
        ));
        let calls = h.http.calls(endpoint("ORDER"));
        assert_eq!(
            calls.len(),
            1,
            "venue accepted the actual placement before local Store update"
        );
        assert_eq!(body(&calls[0])["timeInForce"], "POST_ONLY");
        let ids = outcome.expect("accepted PostOnly placement must resolve after Store update");
        assert_eq!(ids, ["1008"]);
        let stored = h.store.orders();
        assert_eq!(stored.len(), usize::from(has_stored_order));
        if has_stored_order {
            assert_eq!(
                stored[0].time_in_force,
                ModelField::Value(OrderTimeInForce::PostOnly)
            );
            assert_eq!(stored[0].filled, 0.25, "only patch the source's TIF field");
        }
    }
}

#[test]
fn accepted_post_only_batch_updates_only_the_matching_store_row() {
    let h = harness(options(), |http| {
        http.ok(
            "POST",
            endpoint("BATCH_ORDERS"),
            json!([
                {"orderId":"1009","clientOrderId":"synthetic-client-2","success":true},
                {"orderId":"1008","clientOrderId":"synthetic-client-1","success":true}
            ]),
        );
        script(http);
    });
    seed(&h);
    let rows: Vec<_> = ["1008", "1009"]
        .into_iter()
        .map(|id| {
            let mut row = order();
            row.id = id.into();
            row.time_in_force = ModelField::Value(OrderTimeInForce::GoodTillCancel);
            row
        })
        .collect();
    Store::update(
        h.store.as_ref(),
        serde_json::from_value(json!({"orders":rows})).unwrap(),
        false,
    )
    .unwrap();
    let mut post_only = place();
    post_only.time_in_force = Some(OrderTimeInForce::PostOnly);
    let mut regular = place();
    regular.client_order_id = Some("synthetic-client-2".into());
    let outcome = h.drive(Box::pin(
        h.venue()
            .base()
            .place_orders(h.exchange.as_ref(), vec![post_only, regular]),
    ));
    let calls = h.http.calls(endpoint("BATCH_ORDERS"));
    assert_eq!(calls.len(), 1, "venue accepted both actual batch rows");
    assert_eq!(body(&calls[0])["batchOrders"].as_array().unwrap().len(), 2);
    assert_eq!(
        outcome.expect("accepted PostOnly batch must resolve after Store update"),
        ["1008", "1009"]
    );
    let stored = h.store.orders();
    assert_eq!(stored.len(), 2);
    assert_eq!(
        stored
            .iter()
            .find(|o| o.id == "1008")
            .unwrap()
            .time_in_force,
        ModelField::Value(OrderTimeInForce::PostOnly)
    );
    assert_eq!(
        stored
            .iter()
            .find(|o| o.id == "1009")
            .unwrap()
            .time_in_force,
        ModelField::Value(OrderTimeInForce::GoodTillCancel)
    );
}

#[test]
fn accepted_tpsl_amendment_updates_the_real_store_price() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        let h = setup();
        h.http.ok(
            "POST",
            endpoint("MODIFY_TP_SL_ORDER"),
            json!({"success":true}),
        );
        let mut protective = order();
        protective.r#type = ModelField::Value(kind);
        Store::update(
            h.store.as_ref(),
            serde_json::from_value(json!({"orders":[protective.clone()]})).unwrap(),
            false,
        )
        .unwrap();
        let update =
            serde_json::from_value(json!({"order":protective,"update":{"price":103.5}})).unwrap();
        let outcome = h.drive(h.venue().update_order(update));
        let calls = h.http.calls(endpoint("MODIFY_TP_SL_ORDER"));
        assert_eq!(calls.len(), 1, "venue accepted the actual TP/SL amendment");
        assert_eq!(body(&calls[0])["triggerPrice"], "103.5");
        assert_eq!(
            outcome.expect("accepted TP/SL amendment must resolve after Store update"),
            ["native-9"]
        );
        let stored = h.store.orders();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].price, ModelField::Value(103.5));
        assert_eq!(stored[0].amount, 2.0);
        assert_eq!(stored[0].r#type, ModelField::Value(kind));
    }
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
            delivered.borrow_mut().push(
                safe_cex_exchange::js_runtime::value::to_json(value).expect("finite fixture fill"),
            );
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
    assert_eq!(ids, ["1008"]);
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

#[test]
fn trait_nuke_positions_closes_unseen_positions_and_replaces_snapshot() {
    let h = harness(options(), |http| {
        script(http);
        http.ok("POST", endpoint("CLOSE_POSITIONS"), json!({}));
        http.ok("GET", endpoint("POSITIONS"), json!([]));
    });
    seed(&h);
    // Consume the initial position page, leaving an empty close-confirmation page.
    h.drive(h.venue().fetch_positions(None)).unwrap();
    assert!(h.store.positions().is_empty());
    expect_ok(h.drive(h.venue().nuke_positions(0)));
    let calls = h.http.calls(endpoint("CLOSE_POSITIONS"));
    assert_eq!(
        calls.len(),
        1,
        "close-all reaches positions absent from the local Store"
    );
    assert_eq!(body(&calls[0]), json!({}));
    assert_eq!(h.http.calls(endpoint("POSITIONS")).len(), 2);
    assert!(!h.store.positions().is_empty());
    assert!(h.store.positions().iter().all(|p| p.contracts == 0.0));
    assert!(h.http.calls(endpoint("ORDER")).is_empty());
}

#[test]
fn trait_nuke_closes_before_cancelling_both_order_families() {
    let h = harness(options(), |http| {
        script(http);
        http.ok("POST", endpoint("CLOSE_POSITIONS"), json!({}));
    });
    seed(&h);
    expect_ok(h.drive(h.venue().nuke(0)));
    let calls = h.http.calls(endpoint("CLOSE_POSITIONS"));
    assert_eq!(
        calls.len(),
        1,
        "trait nuke uses the venue close-all endpoint"
    );
    let sent = h.http.sent.lock().unwrap();
    let paths = sent.iter().map(|r| r.url.as_str()).collect::<Vec<_>>();
    let close = paths
        .iter()
        .position(|p| *p == endpoint("CLOSE_POSITIONS"))
        .unwrap();
    let positions = paths
        .iter()
        .position(|p| *p == endpoint("POSITIONS"))
        .unwrap();
    let orders = paths
        .iter()
        .position(|p| *p == endpoint("CANCEL_ALL_OPEN_ORDERS"))
        .unwrap();
    let algo = paths
        .iter()
        .position(|p| *p == endpoint("CANCEL_ALL_ALGO_ORDERS"))
        .unwrap();
    assert!(close < positions && positions < orders && positions < algo);
    drop(sent);
    assert!(h.http.calls(endpoint("ORDER")).is_empty());
}

#[test]
fn private_orders_frame_updates_the_real_store() {
    let h = setup();
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    let mut row = raw_order();
    row["orderId"] = json!("socket-order-42");
    row["price"] = json!("123.5");
    let socket = h.venue().base().get_private_sockets()[0].clone();
    socket
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(
            json!({"e":"orders","d":[row]}).to_string(),
        ))
        .unwrap();
    let orders = h.store.orders();
    let actual = orders.iter().find(|o| o.id == "socket-order-42").unwrap();
    assert_eq!(actual.price, 123.5);
    assert_eq!(actual.amount, 2.0);
    assert_eq!(actual.filled, 0.25);
    drop(orders);
    h.venue().dispose().unwrap();
}

#[test]
fn private_positions_frame_updates_the_real_store() {
    let h = setup();
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    let socket = h.venue().base().get_private_sockets()[0].clone();
    socket.on_message(&safe_cex_transport::socket::SocketFrame::Text(
        json!({"e":"positions","d":[{"symbol":"BTCUSDT","positionSide":"LONG","size":"3","openValue":"330","leverage":"20","marginType":"CROSS","updatedTime":1700000000001.0}]}).to_string(),
    )).unwrap();
    let positions = h.store.positions();
    let actual = positions.iter().find(|p| p.contracts == 3.0).unwrap();
    assert_eq!(actual.symbol, "BTCUSDT");
    assert_eq!(actual.entry_price, 110.0);
    drop(positions);
    h.venue().dispose().unwrap();
}

#[test]
fn private_account_frame_refreshes_balance_in_the_real_store() {
    let h = setup();
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    assert_eq!(h.store.balance().total, 150.0);
    h.http.replace(
        "GET",
        endpoint("BALANCE"),
        json!([{"asset":"USDT","balance":"285","availableBalance":"240","unrealizePnl":"5"}]),
    );
    let socket = h.venue().base().get_private_sockets()[0].clone();
    socket
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(
            json!({"e":"account","d":[]}).to_string(),
        ))
        .unwrap();
    h.runtime.pump();
    h.runtime.advance(251.0);
    assert_eq!(h.store.balance().total, 285.0);
    assert_eq!(h.store.balance().free, 240.0);
    assert_eq!(h.store.balance().upnl, 5.0);
    h.venue().dispose().unwrap();
}

#[test]
fn private_balance_refresh_rejection_is_silent() {
    let h = harness(options(), |http| {
        script(http);
        http.ok(
            "GET",
            endpoint("BALANCE"),
            json!([{"asset":"USDT","balance":"150","availableBalance":"120","unrealizePnl":"3"}]),
        );
        http.route(
            "GET",
            endpoint("BALANCE"),
            400,
            json!({"msg":"balance unavailable"}),
        );
    });
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    let before = h.http.calls(endpoint("BALANCE")).len();
    assert_eq!(before, 2);
    assert!(h.runtime.reports.borrow().is_empty());
    assert_eq!(h.store.balance().total, 150.0);
    h.venue().base().get_private_sockets()[0]
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(
            json!({"e":"account","d":[{"equity":"999"}]}).to_string(),
        ))
        .unwrap();
    h.runtime.pump();
    h.runtime.advance(250.0);
    h.runtime.pump();
    assert_eq!(h.http.calls(endpoint("BALANCE")).len(), before + 1);
    assert_eq!(
        h.store.balance().total,
        150.0,
        "rejected refresh retains balance"
    );
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "source catch is silent: {:?}",
        h.runtime.reports.borrow()
    );
    h.venue().dispose().unwrap();
}

#[test]
fn gap_snapshot_rejection_is_silent_and_keeps_the_existing_book() {
    use std::{cell::RefCell, rc::Rc};
    let h = harness(options(), |http| {
        script(http);
        http.route(
            "GET",
            endpoint("DEPTH"),
            400,
            json!({"msg":"snapshot unavailable"}),
        );
    });
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    let logs = Rc::new(RefCell::new(Vec::<Value>::new()));
    let delivered = logs.clone();
    h.venue().base().on(
        "log",
        Rc::new(move |_, message| {
            delivered.borrow_mut().push(
                safe_cex_exchange::js_runtime::value::to_json(message)
                    .expect("finite fixture event"),
            );
            Ok(())
        }),
    );
    let books = Rc::new(RefCell::new(Vec::<OrderBook>::new()));
    let delivered = books.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            "BTCUSDT".into(),
            Rc::new(move |book| delivered.borrow_mut().push(book)),
        )
        .unwrap();
    h.runtime.pump();
    assert_eq!(books.borrow().len(), 1);
    assert_eq!(books.borrow()[0].bids[0].amount, 2.0);
    assert_eq!(h.http.calls(endpoint("DEPTH")).len(), 1);
    assert!(logs.borrow().is_empty());
    let socket = h.venue().base().get_public_sockets()[0].clone();
    socket
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(
            json!({"e":"depth","s":"BTCUSDT","U":20,"u":20,"b":[["101.4","99"]],"a":[]})
                .to_string(),
        ))
        .unwrap();
    h.runtime.pump();
    assert_eq!(h.http.calls(endpoint("DEPTH")).len(), 2);
    assert_eq!(
        books.borrow().len(),
        1,
        "failed gap snapshot does not publish a book"
    );
    assert!(
        logs.borrow().is_empty(),
        "source gap catch is silent: {:?}",
        logs.borrow()
    );
    socket
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(
            json!({"e":"depth","s":"BTCUSDT","U":18,"u":18,"b":[["101.4","4"]],"a":[]}).to_string(),
        ))
        .unwrap();
    assert_eq!(books.borrow().last().unwrap().bids[0].amount, 4.0);
    cleanup().unwrap();
    h.venue().dispose().unwrap();
}

#[test]
fn private_fill_frame_emits_and_reconciles_the_real_order_store() {
    use std::{cell::Cell, rc::Rc};
    let h = setup();
    expect_ok(h.drive(h.venue().start()));
    h.runtime.pump();
    let before = h.store.orders().to_vec();
    assert_eq!(
        before
            .iter()
            .find(|order| order.id == "native-9")
            .unwrap()
            .filled,
        0.25
    );
    let fills = Rc::new(Cell::new(0usize));
    let observed = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, _| {
            observed.set(observed.get() + 1);
            Ok(())
        }),
    );
    let mut refreshed = raw_order();
    refreshed["status"] = json!("PARTIALLY_FILLED");
    refreshed["executedQty"] = json!("0.75");
    h.http
        .replace("GET", endpoint("OPEN_ORDERS"), json!([refreshed]));
    let reads = h.http.calls(endpoint("OPEN_ORDERS")).len();
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("private"))
        .unwrap()
        .1
        .clone();
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(
                json!({"e":"fill","d":[{
            "id":"fill-after-partial","orderId":"native-9","orderSide":"SELL","symbol":"BTCUSDT",
            "fillValue":"50.625","fillSize":"0.5","fillFee":"0.1","realizePnl":"3.5",
            "createdTime":1700000000000.0}]})
                .to_string(),
            ),
        ));
    h.runtime.pump();
    assert_eq!(
        fills.get(),
        1,
        "queued frame must reach the production fill emitter"
    );
    assert_eq!(
        h.store.orders().to_vec(),
        before,
        "source waits for REST order reconciliation"
    );
    h.runtime.advance(249.0);
    assert_eq!(h.http.calls(endpoint("OPEN_ORDERS")).len(), reads);
    h.runtime.advance(1.0);
    assert_eq!(h.http.calls(endpoint("OPEN_ORDERS")).len(), reads + 1);
    let orders = h.store.orders();
    let actual = orders.iter().find(|order| order.id == "native-9").unwrap();
    assert_eq!(actual.filled, 0.75);
    assert_eq!(actual.remaining, 1.25);
    assert_eq!(actual.price, 101.5);
    assert_eq!(actual.status.value(), Some(&OrderStatus::Open));
    drop(orders);
    assert_eq!(
        fills.get(),
        1,
        "REST reconciliation must not duplicate fills"
    );
    assert!(h.runtime.reports.borrow().is_empty());
    h.venue().dispose().unwrap();
}

#[test]
fn batch_order_ids_match_source_decimal_admission_before_store_updates() {
    let ids = json!([
        "0",
        "01",
        "-1",
        "+1",
        "1.0",
        "1e3",
        "not-an-id",
        " 1",
        "1 ",
        "１",
        "1\n",
        "1",
        "123",
        "9007199254740993"
    ]);
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/trading-oracle.cjs"
        ))
        .arg("binding-batch-ids")
        .arg(ids.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let ids = ids.as_array().unwrap();
    assert_eq!(source.len(), ids.len());
    fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
        &value["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pair| pair[0] == name)
            .unwrap()[1]
    }
    fn text(value: &Value) -> String {
        String::from_utf16(
            &value["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|unit| unit.as_u64().unwrap() as u16)
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }
    for (id, row) in ids.iter().zip(source) {
        let id = id.as_str().unwrap();
        let h = harness(options(), |http| {
            http.ok(
                "POST",
                endpoint("BATCH_ORDERS"),
                json!([{"orderId":id,"clientOrderId":"synthetic-client-1","success":true}]),
            );
            script(http);
        });
        seed(&h);
        let mut stored = order();
        stored.id = id.into();
        stored.time_in_force = ModelField::Value(OrderTimeInForce::GoodTillCancel);
        Store::update(
            h.store.as_ref(),
            serde_json::from_value(json!({"orders":[stored]})).unwrap(),
            false,
        )
        .unwrap();
        let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let reported = errors.clone();
        h.venue().base().on(
            "error",
            std::rc::Rc::new(move |_, value: &safe_cex_exchange::js_runtime::value::Js| {
                reported.borrow_mut().push(value.clone());
                Ok(())
            }),
        );
        let mut opts = place();
        opts.time_in_force = Some(OrderTimeInForce::PostOnly);
        let result = h.drive(Box::pin(
            h.venue()
                .base()
                .place_orders(h.exchange.as_ref(), vec![opts]),
        ));
        assert_eq!(h.http.calls(endpoint("BATCH_ORDERS")).len(), 1);
        let expected_error = field(&row["expected"], "error");
        if expected_error["kind"] == "null" {
            let expected = field(&row["expected"], "result")["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(text)
                .collect::<Vec<_>>();
            assert_eq!(result.unwrap(), expected, "accepted source ID {id:?}");
            assert!(errors.borrow().is_empty());
            assert_eq!(
                h.store.orders()[0].time_in_force,
                ModelField::Value(OrderTimeInForce::PostOnly)
            );
        } else {
            let error = result.expect_err(&format!("source rejects malformed batch ID {id:?}"));
            assert_eq!(error.name, text(field(expected_error, "name")));
            assert_eq!(error.message, text(field(expected_error, "message")));
            let errors = errors.borrow();
            assert_eq!(
                errors.len(),
                1,
                "production BaseExchange reports exactly one refusal"
            );
            assert!(
                errors[0].strict_equals(&safe_cex_exchange::js_runtime::value::Js::str(&format!(
                    "Exchange Error: {}",
                    error.message
                ))),
                "native emitter preserves the exact source refusal string"
            );
            assert_eq!(
                h.store.orders()[0].time_in_force,
                ModelField::Value(OrderTimeInForce::GoodTillCancel),
                "no PostOnly patch before full source validation"
            );
        }
    }
}

fn named_market(symbol: &str, base: &str) -> Value {
    let mut row = raw_market();
    row["symbol"] = json!(symbol);
    row["baseAsset"] = json!(base);
    row
}
#[test]
fn market_closed_rows_keep_good_markets_and_report_field_values() {
    let bad_values = [
        None,
        Some(Value::Null),
        Some(json!(17)),
        Some(json!(false)),
        Some(json!({})),
        Some(json!([])),
    ];
    for field in ["baseAsset", "symbol"] {
        for bad in &bad_values {
            let mut refused = named_market("BADUSDT", "BAD");
            match bad {
                Some(value) => {
                    refused[field] = value.clone();
                }
                None => {
                    refused.as_object_mut().unwrap().remove(field);
                }
            }
            let h = harness(options(), |http| {
                http.ok("GET", endpoint("EXCHANGE_INFO"), json!({"symbols":[named_market("ETHUSDT", "ETH"), refused, named_market("SOLUSDT", "SOL")]}));
                script(http);
            });
            seed(&h);
            let logs = historical_fill_logs(&h);
            let markets = h.drive(h.venue().fetch_markets()).unwrap();
            let identities: Vec<_> = markets
                .iter()
                .map(|m| {
                    (
                        m.id.as_str(),
                        m.symbol.as_str(),
                        m.base.as_str(),
                        m.quote.as_str(),
                    )
                })
                .collect();
            assert_eq!(
                identities,
                [
                    ("ETHUSDT", "ETHUSDT", "ETH", "USDT"),
                    ("SOLUSDT", "SOLUSDT", "SOL", "USDT")
                ],
                "bad {field}={bad:?} must not discard good markets or return the cached BTC snapshot"
            );
            assert_eq!(
                h.store.markets()[0].symbol,
                "BTCUSDT",
                "fetch returns the snapshot; caller commits it"
            );
            let logs = logs.borrow();
            assert_eq!(logs.len(), 1);
            assert_eq!(logs[0]["severity"], "warning");
            let message = logs[0]["message"].as_str().unwrap();
            let rendered = bad
                .as_ref()
                .map_or_else(|| "undefined".to_owned(), Value::to_string);
            assert!(message.contains(&format!("Market.{field}")), "{message}");
            assert!(
                message.contains(&format!("received: {rendered}")),
                "{message}"
            );
            assert_eq!(h.http.calls(endpoint("EXCHANGE_INFO")).len(), 1);
        }
    }
}
#[test]
fn market_closed_all_refused_rows_return_empty_with_each_report() {
    let mut bad = named_market("BADUSDT", "BAD");
    bad["baseAsset"] = json!(17);
    let mut absent = named_market("MISSINGUSDT", "BAD");
    absent.as_object_mut().unwrap().remove("baseAsset");
    let h = harness(options(), |http| {
        http.ok(
            "GET",
            endpoint("EXCHANGE_INFO"),
            json!({"symbols":[bad, absent]}),
        );
        script(http);
    });
    seed(&h);
    let logs = historical_fill_logs(&h);
    assert!(
        h.drive(h.venue().fetch_markets()).unwrap().is_empty(),
        "all refused rows must not return the stale cached market"
    );
    assert_eq!(logs.borrow().len(), 2);
}
#[test]
fn market_closed_refusal_does_not_replace_source_coercion_failure() {
    let mut bad = named_market("BADUSDT", "BAD");
    bad["baseAsset"] = json!(17);
    bad["makerFeeRate"] = json!({"toString":null});
    let h = harness(options(), |http| {
        http.ok(
            "GET",
            endpoint("EXCHANGE_INFO"),
            json!({"symbols":[named_market("ETHUSDT", "ETH"), bad]}),
        );
        script(http);
    });
    seed(&h);
    let logs = historical_fill_logs(&h);
    let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = errors.clone();
    h.venue().base().on(
        "error",
        std::rc::Rc::new(move |_, value| {
            sink.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(markets.len(), 1);
    assert_eq!(
        markets[0].symbol, "BTCUSDT",
        "source failure retains cached snapshot"
    );
    assert!(
        !logs.borrow().iter().any(|log| log["severity"] == "warning"),
        "a source coercion failure must not become a typed refusal"
    );
    assert_eq!(errors.borrow().len(), 1);
    assert!(
        errors.borrow()[0]
            .as_str()
            .unwrap()
            .contains("Cannot convert object to primitive value")
    );
}

#[test]
fn market_closed_source_keeps_raw_fields_and_catches_coercion_errors() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/market-row-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "13");
}

#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

#[test]
fn production_ticker_coercion_matches_constructed_typescript() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_store::StoreUpdate;
    use safe_cex_transport::socket::SocketFrame;
    let source = ticker_coercion::source("weex");
    assert_eq!(source["cases"].as_array().unwrap().len(), 96);
    let h = setup();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("public"))
        .unwrap()
        .1
        .clone();
    for case in source["cases"].as_array().unwrap() {
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                tickers: Some(ticker_coercion::seed("weex")),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        for frame in case["frames"].as_array().unwrap() {
            socket
                .events
                .borrow_mut()
                .push_back(WsEvent::Message(SocketFrame::Text(
                    frame.as_str().unwrap().into(),
                )));
            h.runtime.pump();
        }
        ticker_coercion::assert_native(&h.store.tickers(), case);
    }
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    h.venue().dispose().unwrap();
}
