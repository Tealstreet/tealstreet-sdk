//! Contract V3 documented trading examples and adversarial lifecycle contracts.
use safe_cex_engine::model::*;
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Response, SocketId,
};
use safe_cex_venue_weex::{Options, WeexFutures};
use serde_json::{Value, json};

fn engine() -> Engine<WeexFutures> {
    engine_with_private_ws(false)
}
fn engine_with_private_ws(private_ws_headers: bool) -> Engine<WeexFutures> {
    let mut e = Engine::new(WeexFutures::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("passphrase".into()),
        private_ws_headers,
        ..Options::default()
    }));
    e.store_mut().set_markets(vec![Market {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.0,
            },
            leverage: MinMax {
                min: 1.0,
                max: 100.0,
            },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    e.store_mut().update_setting("BTCUSDT", |s| {
        *s = MarketSetting {
            leverage: 20.0,
            margin_type: Some(MarginType::Cross),
            ..MarketSetting::default()
        }
    });
    e
}
fn opts(id: &str) -> PlaceOrderOpts {
    PlaceOrderOpts {
        max_slippage_pct: None,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.01,
        price: Some(69000.0),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: (!id.is_empty()).then(|| id.to_owned()),
        close: None,
    }
}
fn order(id: &str, kind: OrderType) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind,
        side: OrderSide::Sell,
        price: 59000.0,
        amount: 0.2,
        filled: 0.0,
        remaining: 0.2,
        reduce_only: kind == OrderType::StopLoss || kind == OrderType::TakeProfit,
        close: None,
        client_order_id: Some("old-client".into()),
        time_in_force: Some(TimeInForce::PostOnly),
        last_update: 0.0,
        trigger_type: Some(StopTriggerType::MarkPrice),
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}
fn command(e: &mut Engine<WeexFutures>, command: Command) -> Vec<Action> {
    e.handle(1000.0, Event::Command(command));
    e.drain_actions().collect()
}
fn writes(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.method != Method::Get => Some(r.clone()),
            _ => None,
        })
        .collect()
}
fn body(r: &HttpRequest) -> Value {
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}
fn response(e: &mut Engine<WeexFutures>, r: &HttpRequest, value: Value) -> Vec<Action> {
    let text = value.to_string();
    raw_response(e, r, &text)
}
fn raw_response(e: &mut Engine<WeexFutures>, r: &HttpRequest, text: &str) -> Vec<Action> {
    e.handle(
        1001.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: text,
            },
        },
    );
    e.drain_actions().collect()
}
fn done(actions: &[Action]) -> Result<String, String> {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(result.clone()),
            _ => None,
        })
        .expect("completed command")
}
fn batch_ack(r: &HttpRequest, first: u64) -> Value {
    let payload = body(r);
    let rows = payload["batchOrders"].as_array().unwrap();
    Value::Array(rows.iter().enumerate().rev().map(|(i, row)| json!({"orderId":(first+i as u64).to_string(),"clientOrderId":row["newClientOrderId"],"success":true,"errorCode":"","errorMessage":""})).collect())
}
fn single_ack(r: &HttpRequest, id: u64) -> Value {
    let payload = body(r);
    json!({"orderId":id.to_string(),"clientOrderId":payload.get("newClientOrderId").unwrap_or(&payload["clientAlgoId"]),"success":true})
}
#[test]
fn regular_batches_are_five_sequential_and_correlated_by_echoed_id() {
    let mut e = engine();
    let orders: Vec<_> = (0..12).map(|i| opts(&format!("batch-{i}"))).collect();
    let actions = command(&mut e, Command::PlaceOrders { command: 1, orders });
    let mut request = writes(&actions).pop().unwrap();
    assert!(request.url.ends_with("/capi/v3/batchOrders"));
    assert_eq!(writes(&actions).len(), 1);
    assert_eq!(
        e.store()
            .orders()
            .iter()
            .filter(|o| o.status == OrderStatus::Creating)
            .count(),
        5
    );
    let payload = body(&request);
    assert_eq!(
        payload["batchOrders"][0],
        json!({"symbol":"BTCUSDT","side":"BUY","positionSide":"LONG","type":"LIMIT","timeInForce":"GTC","quantity":"0.01","price":"69000","newClientOrderId":"batch-0"})
    );
    for (size, first) in [(5, 100), (5, 105), (2, 110)] {
        assert_eq!(
            body(&request)["batchOrders"].as_array().unwrap().len(),
            size
        );
        let ack = batch_ack(&request, first);
        let actions = response(&mut e, &request, ack);
        if size == 2 {
            assert_eq!(
                serde_json::from_str::<Vec<String>>(&done(&actions).unwrap()).unwrap(),
                (100..112).map(|v| v.to_string()).collect::<Vec<_>>()
            );
            assert!(writes(&actions).is_empty());
        } else {
            request = writes(&actions)
                .pop()
                .expect("next batch after acknowledgement");
        }
    }
    assert_eq!(e.store().orders().len(), 12);
    assert!(
        e.store()
            .orders()
            .iter()
            .all(|o| o.status == OrderStatus::Open)
    );
}
#[test]
fn every_caller_id_is_preflighted_before_any_write() {
    let invalid = ["", "space is invalid", "unicode-💥", &"x".repeat(37)];
    for id in invalid {
        let mut e = engine();
        let mut bad = opts("good");
        bad.client_order_id = Some(id.into());
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![opts("first"), bad],
            },
        );
        assert!(writes(&actions).is_empty());
        assert!(done(&actions).is_err());
        assert!(e.store().orders().is_empty());
    }
    for bad in [
        {
            let mut o = opts("id");
            o.kind = OrderType::Conditional;
            o.trigger_price = Some(60000.0);
            o
        },
        {
            let mut o = opts("id");
            o.take_profit = Some(70000.0);
            o
        },
        {
            let mut o = opts("id");
            o.stop_loss = Some(68000.0);
            o
        },
        {
            let mut o = opts("id");
            o.amount = 100.001;
            o
        },
        opts("first"),
    ] {
        let mut e = engine();
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![opts("first"), bad],
            },
        );
        assert!(writes(&actions).is_empty());
        assert!(done(&actions).is_err());
    }
    let mut e = engine();
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts(".A:/_-0123456789az")],
        },
    );
    assert_eq!(
        body(&writes(&actions)[0])["batchOrders"][0]["newClientOrderId"],
        ".A:/_-0123456789az"
    );
}
#[test]
fn dedicated_shapes_keep_contiguous_batch_boundaries() {
    let mut e = engine();
    let mut conditional = opts("");
    conditional.kind = OrderType::Conditional;
    conditional.trigger_price = Some(68900.0);
    conditional.price = Some(68800.0);
    conditional.take_profit = Some(70500.0);
    conditional.stop_loss = Some(68000.0);
    conditional.tp_trigger_type = Some(StopTriggerType::MarkPrice);
    let mut protective = opts("");
    protective.kind = OrderType::StopLoss;
    protective.side = OrderSide::Sell;
    protective.price = Some(68000.0);
    protective.reduce_only = Some(true);
    let mut bracket = opts("");
    bracket.take_profit = Some(70000.0);
    bracket.stop_loss = Some(68000.0);
    let mut reducing = opts("");
    reducing.reduce_only = Some(true);
    reducing.side = OrderSide::Sell;
    let mut actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![
                opts("a"),
                conditional,
                protective,
                bracket,
                reducing,
                opts("z"),
            ],
        },
    );
    for (i, path) in [
        "batchOrders",
        "algoOrder",
        "placeTpSlOrder",
        "order",
        "order",
        "batchOrders",
    ]
    .iter()
    .enumerate()
    {
        let r = writes(&actions).pop().unwrap();
        assert!(r.url.ends_with(path));
        let b = body(&r);
        if i == 1 {
            assert_eq!(b["type"], "STOP");
            assert_eq!(b["triggerPrice"], "68900");
            assert_eq!(b["price"], "68800");
            assert_eq!(b["presetTakeProfitPrice"], "70500");
            assert_eq!(b["TpWorkingType"], "MARK_PRICE");
            assert!(b.get("newClientOrderId").is_none());
        }
        if i == 2 {
            assert_eq!(b["planType"], "STOP_LOSS");
            assert_eq!(b["triggerPriceType"], "MARK_PRICE");
            assert_eq!(b["positionSide"], "LONG");
            assert_eq!(b["executePrice"], "0");
        }
        if i == 3 {
            assert_eq!(b["tpTriggerPrice"], "70000");
            assert_eq!(b["slTriggerPrice"], "68000");
        }
        if i == 4 {
            assert_eq!(b["reduceOnly"], true);
            assert_eq!(b["positionSide"], "LONG");
        }
        let ack = if *path == "batchOrders" {
            batch_ack(&r, 100 + i as u64)
        } else if i == 2 {
            json!([{"orderId":812345678901234900u64,"success":true}])
        } else {
            single_ack(&r, 100 + i as u64)
        };
        actions = response(&mut e, &r, ack);
    }
    assert!(done(&actions).is_ok());
    assert!(e.store().order("812345678901234900").is_some());
}
#[test]
fn malformed_batch_responses_fail_closed_and_remove_only_provisional_rows() {
    for malformed in [
        r#"{}"#,
        r#"[]"#,
        r#"[{"orderId":"1","clientOrderId":"a","success":true},{"orderId":"2","clientOrderId":"a","success":true}]"#,
        r#"[{"orderId":"1","clientOrderId":"a","success":true},{"orderId":"2","clientOrderId":"unknown","success":true}]"#,
        r#"[{"orderId":"1","clientOrderId":"a","success":true},{"orderId":"2","clientOrderId":"b"}]"#,
        r#"[{"orderId":"1","clientOrderId":"a","success":true},{"orderId":"1","clientOrderId":"b","success":true}]"#,
        r#"[{"orderId":"1","clientOrderId":"a","success":true},{"orderId":"bogus","clientOrderId":"b","success":true}]"#,
        r#"[{"orderId":"1","clientOrderId":"a","success":true},{"orderId":"2","clientOrderId":"b","success":true"#,
    ] {
        let mut e = engine();
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![opts("a"), opts("b")],
            },
        );
        let r = writes(&actions).pop().unwrap();
        let actions = raw_response(&mut e, &r, malformed);
        assert!(done(&actions).is_err(), "{malformed}");
        assert!(writes(&actions).is_empty());
        assert!(e.store().orders().is_empty());
    }
}
#[test]
fn partial_success_retains_post_only_and_surfaces_each_rejected_row() {
    let mut e = engine();
    let mut post = opts("a");
    post.time_in_force = Some(TimeInForce::PostOnly);
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![post, opts("b"), opts("c")],
        },
    );
    let r = writes(&actions).pop().unwrap();
    assert_eq!(body(&r)["batchOrders"][0]["timeInForce"], "POST_ONLY");
    let actions = response(
        &mut e,
        &r,
        json!([
            {"clientOrderId":"c","success":false,"errorMessage":"bad price"},
            {"clientOrderId":"a","orderId":"101","success":true},
            {"clientOrderId":"b","success":false,"errorMessage":"bad size"}
        ]),
    );
    let error = done(&actions).unwrap_err();
    assert!(error.contains("bad size") && error.contains("bad price"));
    assert_eq!(e.store().orders().len(), 1);
    assert_eq!(
        e.store().order("101").unwrap().time_in_force,
        Some(TimeInForce::PostOnly)
    );
}
#[test]
fn ambiguous_failure_never_retries_or_dispatches_later_writes() {
    let mut e = engine();
    let orders = (0..7).map(|i| opts(&format!("o{i}"))).collect();
    let actions = command(&mut e, Command::PlaceOrders { command: 1, orders });
    let r = writes(&actions).pop().unwrap();
    e.handle(
        1002.0,
        Event::HttpFailed {
            request: r.id,
            error: "connection reset",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(done(&actions).is_err());
    assert!(writes(&actions).is_empty());
    assert!(e.store().orders().is_empty());
}
#[test]
fn cancels_ten_regulars_then_algo_once_and_honors_row_failures() {
    let mut e = engine();
    let mut ids = Vec::new();
    for i in 1..=11 {
        let id = i.to_string();
        ids.push(id.clone());
        e.store_mut().upsert_order(order(&id, OrderType::Limit));
    }
    for id in ["99__tp", "99__sl"] {
        ids.push(id.into());
        let mut o = order(id, OrderType::TakeProfit);
        o.parent_id = Some("99".into());
        e.store_mut().upsert_order(o);
    }
    let actions = command(&mut e, Command::CancelOrders { command: 1, ids });
    let r = writes(&actions).pop().unwrap();
    assert_eq!(
        body(&r)["orderIdList"],
        json!(["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"])
    );
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Canceling);
    let ack = Value::Array(
        (1..=10)
            .map(|i| json!({"orderId":i,"success":true}))
            .collect(),
    );
    let actions = response(&mut e, &r, ack);
    let r = writes(&actions).pop().unwrap();
    assert_eq!(body(&r)["orderIdList"], json!(["11"]));
    let actions = response(&mut e, &r, json!([{"orderId":"11","success":true}]));
    let r = writes(&actions).pop().unwrap();
    assert!(r.url.ends_with("/algoOrder?orderId=99"));
    let actions = response(&mut e, &r, json!({"orderId":"99","success":true}));
    assert_eq!(done(&actions).unwrap(), "[]");
    assert!(
        e.store()
            .orders()
            .iter()
            .all(|o| o.status == OrderStatus::Canceled)
    );
    let mut e = engine();
    for id in ["1", "2"] {
        e.store_mut().upsert_order(order(id, OrderType::Limit));
    }
    let actions = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["1".into(), "2".into()],
        },
    );
    let r = writes(&actions).pop().unwrap();
    let actions = response(
        &mut e,
        &r,
        json!([{"orderId":"2","success":false,"errorMessage":"not found"},{"orderId":"1","success":true}]),
    );
    assert!(done(&actions).is_err());
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Canceled);
    assert_eq!(e.store().order("2").unwrap().status, OrderStatus::Open);
}
#[test]
fn cancel_symbol_always_attempts_both_families_and_only_cancels_acknowledged_rows() {
    let mut e = engine();
    for (id, kind) in [
        ("1", OrderType::Limit),
        ("2", OrderType::StopLoss),
        ("3", OrderType::StopLoss),
    ] {
        e.store_mut().upsert_order(order(id, kind));
    }
    let actions = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 1,
            symbol: "BTCUSDT".into(),
        },
    );
    let r = writes(&actions).pop().unwrap();
    assert!(r.url.ends_with("allOpenOrders?symbol=BTCUSDT"));
    e.handle(
        1001.0,
        Event::HttpFailed {
            request: r.id,
            error: "timeout",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let r = writes(&actions).pop().unwrap();
    assert!(r.url.ends_with("algoOpenOrders?symbol=BTCUSDT"));
    let actions = response(&mut e, &r, json!([{"orderId":2,"success":true}]));
    assert!(done(&actions).is_err());
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Open);
    assert_eq!(e.store().order("2").unwrap().status, OrderStatus::Canceled);
    assert_eq!(e.store().order("3").unwrap().status, OrderStatus::Open);
}
#[test]
fn protective_amend_refuses_synthetic_id_and_size_and_preserves_execution_price() {
    for (id, price, amount) in [
        ("1__tp", Some(71000.0), None),
        ("1", None, Some(0.1)),
        ("1", Some(71000.0), Some(0.1)),
    ] {
        let mut e = engine();
        e.store_mut().upsert_order(order(id, OrderType::TakeProfit));
        let actions = command(
            &mut e,
            Command::AmendOrder {
                command: 1,
                id: id.into(),
                price,
                amount,
            },
        );
        assert!(writes(&actions).is_empty());
        assert!(done(&actions).is_err());
    }
    let mut e = engine();
    let mut o = order("812345678901234900", OrderType::TakeProfit);
    o.extra = Some(OrderExtra {
        conditional_order_price: Some(70900.0),
        ..OrderExtra::default()
    });
    e.store_mut().upsert_order(o);
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "812345678901234900".into(),
            price: Some(71000.04),
            amount: None,
        },
    );
    let r = writes(&actions).pop().unwrap();
    assert!(r.url.ends_with("modifyTpSlOrder"));
    assert_eq!(
        body(&r),
        json!({"orderId":"812345678901234900","triggerPrice":"71000","executePrice":"70900","triggerPriceType":"MARK_PRICE"})
    );
    let actions = response(&mut e, &r, json!({"success":true}));
    assert_eq!(done(&actions).unwrap(), r#"["812345678901234900"]"#);
    assert_eq!(
        e.store().order("812345678901234900").unwrap().price,
        71000.0
    );
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 2,
            id: "812345678901234900".into(),
            price: Some(72000.0),
            amount: None,
        },
    );
    let r = writes(&actions).pop().unwrap();
    let actions = response(&mut e, &r, json!({}));
    assert!(done(&actions).is_err());
    assert_eq!(
        e.store().order("812345678901234900").unwrap().status,
        OrderStatus::Open
    );
    assert_eq!(
        e.store().order("812345678901234900").unwrap().price,
        71000.0
    );
}
#[test]
fn cancel_replace_preflights_then_preserves_brackets_and_generates_new_client_id() {
    let mut e = engine();
    let mut original = opts("");
    original.side = OrderSide::Sell;
    original.amount = 0.2;
    original.price = Some(59000.0);
    original.take_profit = Some(70000.0);
    original.stop_loss = Some(58000.0);
    original.time_in_force = Some(TimeInForce::PostOnly);
    let placed = command(
        &mut e,
        Command::PlaceOrders {
            command: 99,
            orders: vec![original],
        },
    );
    let placement = writes(&placed).pop().unwrap();
    let ack = single_ack(&placement, 1);
    response(&mut e, &placement, ack);
    let old_client = e
        .store()
        .order("1")
        .unwrap()
        .client_order_id
        .clone()
        .unwrap();
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: Some(60000.04),
            amount: Some(0.1234),
        },
    );
    let r = writes(&actions).pop().unwrap();
    assert_eq!(r.method, Method::Delete);
    let actions = response(&mut e, &r, json!([{"orderId":"1","success":true}]));
    let r = writes(&actions).pop().unwrap();
    let payload = body(&r);
    assert!(r.url.ends_with("/order"));
    assert_eq!(payload["tpTriggerPrice"], "70000");
    assert_eq!(payload["slTriggerPrice"], "58000");
    assert_eq!(payload["quantity"], "0.123");
    assert_eq!(payload["price"], "60000");
    assert_eq!(payload["timeInForce"], "POST_ONLY");
    assert_ne!(payload["newClientOrderId"], old_client);
    let ack = single_ack(&r, 2);
    let actions = response(&mut e, &r, ack);
    assert_eq!(done(&actions).unwrap(), r#"["2"]"#);
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Canceled);
    let mut e = engine();
    e.store_mut().upsert_order(order("1", OrderType::Limit));
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: Some(-1.0),
            amount: None,
        },
    );
    assert!(writes(&actions).is_empty());
    assert!(done(&actions).is_err());
}
#[test]
fn private_terminal_updates_win_over_late_placement_ack_or_cancel_failure() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts("a")],
        },
    );
    let r = writes(&actions).pop().unwrap();
    let mut filled = order("1", OrderType::Limit);
    filled.status = OrderStatus::Closed;
    filled.filled = 0.01;
    filled.remaining = 0.0;
    e.store_mut().upsert_order(filled);
    let ack = batch_ack(&r, 1);
    response(&mut e, &r, ack);
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Closed);
    assert_eq!(e.store().order("1").unwrap().filled, 0.01);
    e.store_mut().upsert_order(order("2", OrderType::Limit));
    let actions = command(
        &mut e,
        Command::CancelOrders {
            command: 2,
            ids: vec!["2".into()],
        },
    );
    let r = writes(&actions).pop().unwrap();
    e.store_mut().set_order_status("2", OrderStatus::Closed);
    response(
        &mut e,
        &r,
        json!([{"orderId":"2","success":false,"errorMessage":"already filled"}]),
    );
    assert_eq!(e.store().order("2").unwrap().status, OrderStatus::Closed);
}
#[test]
fn leverage_margin_and_position_mode_use_documented_account_routes() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 200.0,
        },
    );
    let r = writes(&actions).pop().unwrap();
    assert!(r.url.ends_with("/account/leverage"));
    assert_eq!(
        body(&r),
        json!({"symbol":"BTCUSDT","marginType":"CROSSED","crossLeverage":"100"})
    );
    let actions = response(
        &mut e,
        &r,
        json!({"symbol":"BTCUSDT","marginType":"CROSSED","crossLeverage":"100","isolatedLongLeverage":"20","isolatedShortLeverage":"20"}),
    );
    assert_eq!(done(&actions).unwrap(), "[]");
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 100.0);
    let actions = command(
        &mut e,
        Command::SetMarginType {
            command: 2,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
    );
    let r = writes(&actions).pop().unwrap();
    assert_eq!(
        body(&r),
        json!({"symbol":"BTCUSDT","marginType":"ISOLATED","separatedType":"COMBINED"})
    );
    response(
        &mut e,
        &r,
        json!({"code":"200","msg":"success","requestTime":1764505776347u64}),
    );
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
    let actions = command(
        &mut e,
        Command::SetLeverage {
            command: 3,
            symbol: "BTCUSDT".into(),
            leverage: 50.0,
        },
    );
    let r = writes(&actions).pop().unwrap();
    assert_eq!(
        body(&r),
        json!({"symbol":"BTCUSDT","marginType":"ISOLATED","isolatedLongLeverage":"50","isolatedShortLeverage":"50"})
    );
    response(&mut e, &r, json!({"code":"200","msg":"success"}));
    let actions = command(
        &mut e,
        Command::SetPositionMode {
            command: 4,
            hedged: true,
        },
    );
    let r = writes(&actions).pop().unwrap();
    assert!(r.url.ends_with("/account/marginType"));
    assert_eq!(body(&r)["separatedType"], "SEPARATED");
    assert_eq!(body(&r)["marginType"], "ISOLATED");
    let actions = response(&mut e, &r, json!({"code":"200","msg":"success"}));
    assert_eq!(done(&actions).unwrap(), "[]");
    assert!(e.store().hedged());
}

#[test]
fn replacement_keeps_per_leg_bracket_working_types_and_full_protection_is_supported() {
    let mut e = engine();
    let mut bracket = opts("");
    bracket.take_profit = Some(70000.0);
    bracket.stop_loss = Some(68000.0);
    bracket.tp_trigger_type = Some(StopTriggerType::MarkPrice);
    bracket.sl_trigger_type = Some(StopTriggerType::LastPrice);
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![bracket],
        },
    );
    let r = writes(&actions).pop().unwrap();
    let ack = single_ack(&r, 1);
    response(&mut e, &r, ack);
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 2,
            id: "1".into(),
            price: Some(69500.0),
            amount: None,
        },
    );
    let r = writes(&actions).pop().unwrap();
    let actions = response(&mut e, &r, json!([{"orderId":"1","success":true}]));
    let r = writes(&actions).pop().unwrap();
    assert_eq!(body(&r)["TpWorkingType"], "MARK_PRICE");
    assert_eq!(body(&r)["SlWorkingType"], "CONTRACT_PRICE");
    let mut e = engine();
    let mut full = opts("");
    full.kind = OrderType::TakeProfit;
    full.price = Some(70500.0);
    full.amount = 0.0;
    full.reduce_only = Some(true);
    full.side = OrderSide::Sell;
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![full],
        },
    );
    let r = writes(&actions).pop().unwrap();
    assert!(r.url.ends_with("placeTpSlOrder"));
    assert_eq!(body(&r)["quantity"], "0");
}

#[test]
fn margin_change_preserves_symbol_mode_when_account_default_differs() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
        },
    );
    let r = writes(&actions).pop().unwrap();
    let actions = response(&mut e, &r, json!({"code":"200","msg":"success"}));
    let config = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("symbolConfig") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    response(
        &mut e,
        &config,
        json!([{"symbol":"BTCUSDT","marginType":"CROSSED","separatedType":"SEPARATED","crossLeverage":"20","isolatedLongLeverage":"10","isolatedShortLeverage":"15"}]),
    );
    assert!(!e.store().hedged());
    let actions = command(
        &mut e,
        Command::SetMarginType {
            command: 2,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
    );
    let r = writes(&actions).pop().unwrap();
    assert_eq!(body(&r)["separatedType"], "SEPARATED");
}

#[test]
fn conditional_amount_replacement_preserves_trigger_and_execution() {
    let mut e = engine();
    let mut old = order("1", OrderType::Conditional);
    old.price = 68900.0;
    old.extra = Some(OrderExtra {
        conditional_order_price: Some(68800.0),
        ..OrderExtra::default()
    });
    e.store_mut().upsert_order(old);
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: None,
            amount: Some(0.3),
        },
    );
    let r = writes(&actions).pop().unwrap();
    assert!(r.url.ends_with("algoOrder?orderId=1"));
    let actions = response(&mut e, &r, json!({"orderId":"1","success":true}));
    let r = writes(&actions).pop().unwrap();
    assert!(r.url.ends_with("algoOrder"));
    assert_eq!(body(&r)["triggerPrice"], "68900");
    assert_eq!(body(&r)["price"], "68800");
    assert_eq!(body(&r)["quantity"], "0.3");
}

#[test]
fn old_failed_command_cannot_rollback_newer_optimistic_owner() {
    let mut e = engine();
    e.store_mut().upsert_order(order("1", OrderType::Limit));
    let actions = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["1".into()],
        },
    );
    let first = writes(&actions).pop().unwrap();
    let actions = command(
        &mut e,
        Command::CancelOrders {
            command: 2,
            ids: vec!["1".into()],
        },
    );
    let second = writes(&actions).pop().unwrap();
    e.handle(
        1002.0,
        Event::HttpFailed {
            request: first.id,
            error: "first timeout",
        },
    );
    e.drain_actions().for_each(drop);
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Canceling);
    let actions = response(&mut e, &second, json!([{"orderId":"1","success":true}]));
    assert_eq!(done(&actions).unwrap(), "[]");
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Canceled);
}

#[test]
fn cancel_symbol_acknowledges_order_arriving_after_dispatch() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 1,
            symbol: "BTCUSDT".into(),
        },
    );
    let r = writes(&actions).pop().unwrap();
    e.store_mut().upsert_order(order("123", OrderType::Limit));
    let actions = response(&mut e, &r, json!([{"orderId":"123","success":true}]));
    let r = writes(&actions).pop().unwrap();
    let actions = response(&mut e, &r, json!([]));
    assert_eq!(done(&actions).unwrap(), "[]");
    assert_eq!(
        e.store().order("123").unwrap().status,
        OrderStatus::Canceled
    );
}

fn import_orders(e: &mut Engine<WeexFutures>, normal: Value, algo: Value) {
    let _ = import_orders_with_socket(e, normal, algo);
}
fn import_orders_with_socket(
    e: &mut Engine<WeexFutures>,
    normal: Value,
    algo: Value,
) -> Option<SocketId> {
    let actions = command(
        e,
        Command::SetLeverage {
            command: 900,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
        },
    );
    let req = writes(&actions).pop().unwrap();
    let actions = response(e, &req, json!({"code":"200"}));
    let socket = actions.iter().find_map(|action| match action {
        Action::WsConnectWithHeaders { socket, .. } => Some(*socket),
        _ => None,
    });
    let requests: Vec<_> = actions
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    let _ = respond_account_reads(e, requests, normal, algo);
    socket
}
fn respond_account_reads(
    e: &mut Engine<WeexFutures>,
    requests: Vec<HttpRequest>,
    normal: Value,
    algo: Value,
) -> Vec<Action> {
    let mut actions = Vec::new();
    for request in requests {
        let value = if request.url.contains("openAlgoOrders") {
            algo.clone()
        } else if request.url.contains("openOrders") {
            normal.clone()
        } else if request.url.contains("accountConfig") {
            json!({"canTrade":true,"dualSidePosition":false})
        } else if request.url.contains("symbolConfig") {
            json!([{"symbol":"BTCUSDT","marginType":"CROSSED","separatedType":"COMBINED","crossLeverage":"20","isolatedLongLeverage":"20","isolatedShortLeverage":"20"}])
        } else {
            json!([])
        };
        actions.extend(response(e, &request, value));
    }
    actions
}
#[test]
fn imported_algo_bracket_children_preserve_both_working_types_on_amend() {
    let mut e = engine();
    import_orders(
        &mut e,
        json!([]),
        json!([{"algoId":"55","symbol":"BTCUSDT","side":"BUY","orderType":"STOP","algoStatus":"UNTRIGGERED","quantity":"0.02","price":"68800","triggerPrice":"68900","tpTriggerPrice":"70500","slTriggerPrice":"68000","tpOrderType":"MARK_PRICE","workingType":"CONTRACT_PRICE","updateTime":1773295700000u64}]),
    );
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "55".into(),
            price: Some(69000.0),
            amount: None,
        },
    );
    let cancel = writes(&actions).pop().unwrap();
    let actions = response(&mut e, &cancel, json!({"orderId":"55","success":true}));
    let replacement = writes(&actions).pop().unwrap();
    assert_eq!(body(&replacement)["TpWorkingType"], "MARK_PRICE");
    assert_eq!(body(&replacement)["SlWorkingType"], "CONTRACT_PRICE");
}
#[test]
fn unknown_imported_bracket_source_fails_before_cancel() {
    let mut e = engine();
    let mut old = order("1", OrderType::Limit);
    old.take_profit = Some(70000.0);
    old.stop_loss = Some(58000.0);
    e.store_mut().upsert_order(old);
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: Some(60000.0),
            amount: None,
        },
    );
    assert!(writes(&actions).is_empty());
    assert!(
        done(&actions)
            .unwrap_err()
            .contains("cannot preserve imported")
    );
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Open);
}
#[test]
fn price_only_amend_replaces_only_the_unfilled_quantity() {
    let mut e = engine();
    let mut partial = order("1", OrderType::Limit);
    partial.amount = 0.2;
    partial.filled = 0.1;
    partial.remaining = 0.1;
    e.store_mut().upsert_order(partial);
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: Some(60000.0),
            amount: None,
        },
    );
    let cancel = writes(&actions).pop().unwrap();
    let actions = response(&mut e, &cancel, json!([{"orderId":"1","success":true}]));
    let replacement = writes(&actions).pop().unwrap();
    assert_eq!(body(&replacement)["batchOrders"][0]["quantity"], "0.1");
    assert_eq!(
        body(&replacement)["batchOrders"][0]["timeInForce"],
        "POST_ONLY"
    );
    let ack = batch_ack(&replacement, 2);
    response(&mut e, &replacement, ack);
    assert_eq!(
        e.store().order("1").unwrap().filled + e.store().order("2").unwrap().amount,
        0.2
    );
}
#[test]
fn a_fill_while_canceling_blocks_stale_replacement_quantity() {
    for filled in [0.15, 0.2] {
        let mut e = engine();
        let mut partial = order("1", OrderType::Limit);
        partial.amount = 0.2;
        partial.filled = 0.1;
        partial.remaining = 0.1;
        e.store_mut().upsert_order(partial);
        let actions = command(
            &mut e,
            Command::AmendOrder {
                command: 1,
                id: "1".into(),
                price: Some(60000.0),
                amount: None,
            },
        );
        let cancel = writes(&actions).pop().unwrap();
        e.store_mut().update_order("1", |order| {
            order.filled = filled;
            order.remaining = 0.2 - filled;
            if filled == 0.2 {
                order.status = OrderStatus::Closed;
            }
        });
        let actions = response(&mut e, &cancel, json!([{"orderId":"1","success":true}]));
        assert!(writes(&actions).is_empty());
        assert!(
            done(&actions)
                .unwrap_err()
                .contains("changed while canceling")
        );
        assert_eq!(e.store().order("1").unwrap().filled, filled);
    }
}
#[test]
fn overlapping_settings_commands_fail_before_a_second_write_then_allow_retry() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
        },
    );
    let first = writes(&actions).pop().unwrap();
    let actions = command(
        &mut e,
        Command::SetLeverage {
            command: 2,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
        },
    );
    assert!(writes(&actions).is_empty());
    assert!(
        done(&actions)
            .unwrap_err()
            .contains("settings update is already in flight")
    );
    response(&mut e, &first, json!({"code":"200"}));
    let actions = command(
        &mut e,
        Command::SetLeverage {
            command: 3,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
        },
    );
    let second = writes(&actions).pop().unwrap();
    response(&mut e, &second, json!({"code":"200"}));
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
}
#[test]
fn imported_trailing_market_order_cancels_through_the_algo_endpoint() {
    let mut e = engine();
    import_orders(
        &mut e,
        json!([]),
        json!([{"algoId":"66","symbol":"BTCUSDT","side":"BUY","orderType":"TRAILING_MARKET","algoStatus":"UNTRIGGERED","quantity":"0.02","updateTime":1773295700000u64}]),
    );
    let actions = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["66".into()],
        },
    );
    let cancel = writes(&actions).pop().unwrap();
    assert!(cancel.url.ends_with("/algoOrder?orderId=66"));
}

#[test]
fn fill_only_socket_event_during_cancel_blocks_replacement_before_order_update() {
    let mut e = engine_with_private_ws(true);
    let socket=import_orders_with_socket(&mut e,json!([{"orderId":"1","symbol":"BTCUSDT","side":"SELL","type":"LIMIT","status":"PARTIALLY_FILLED","origQty":"0.2","executedQty":"0.1","price":"59000","updateTime":1773295700000u64}]),json!([])).unwrap();
    e.handle(1002.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: Some(60000.0),
            amount: None,
        },
    );
    let cancel = writes(&actions).pop().unwrap();
    e.handle(1003.0,Event::WsMessage {socket,text:r#"{"e":"fill","d":[{"id":"fill-2","orderId":"1","symbol":"BTCUSDT","orderSide":"SELL","fillSize":"0.05","fillValue":"3000"}]}"#});
    assert_eq!(
        e.store().order("1").unwrap().filled,
        0.1,
        "the cumulative orders frame has not arrived"
    );
    e.drain_actions().for_each(drop);
    let actions = response(&mut e, &cancel, json!([{"orderId":"1","success":true}]));
    assert!(writes(&actions).is_empty());
    assert!(
        done(&actions)
            .unwrap_err()
            .contains("changed while canceling")
    );
}
#[test]
fn imported_algo_children_with_unknown_trigger_source_cannot_authorize_amend() {
    for missing in ["tpOrderType", "workingType"] {
        let mut e = engine();
        let mut row = json!({"algoId":"55","symbol":"BTCUSDT","side":"BUY","orderType":"STOP","algoStatus":"UNTRIGGERED","quantity":"0.02","price":"68800","triggerPrice":"68900","tpTriggerPrice":"70500","slTriggerPrice":"68000","tpOrderType":"MARK_PRICE","workingType":"CONTRACT_PRICE","updateTime":1773295700000u64});
        row.as_object_mut().unwrap().remove(missing);
        import_orders(&mut e, json!([]), Value::Array(vec![row]));
        let actions = command(
            &mut e,
            Command::AmendOrder {
                command: 1,
                id: "55".into(),
                price: Some(69000.0),
                amount: None,
            },
        );
        assert!(writes(&actions).is_empty());
        assert!(
            done(&actions)
                .unwrap_err()
                .contains("cannot preserve imported")
        );
        assert_eq!(e.store().order("55").unwrap().status, OrderStatus::Open);
    }
}

#[test]
fn fill_only_socket_event_before_amend_waits_for_authoritative_order_reconciliation() {
    let mut e = engine_with_private_ws(true);
    let socket=import_orders_with_socket(&mut e,json!([{"orderId":"1","symbol":"BTCUSDT","side":"SELL","type":"LIMIT","status":"PARTIALLY_FILLED","origQty":"0.2","executedQty":"0.1","price":"59000","updateTime":1773295700000u64}]),json!([])).unwrap();
    e.handle(1002.0, Event::WsOpened { socket });
    let before_fill: Vec<_> = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    e.handle(1003.0,Event::WsMessage {socket,text:r#"{"e":"fill","d":[{"id":"fill-2","orderId":"1","symbol":"BTCUSDT","orderSide":"SELL","fillSize":"0.05","fillValue":"3000"}]}"#});
    assert_eq!(
        e.store().order("1").unwrap().filled,
        0.1,
        "the cumulative orders frame has not arrived"
    );
    e.drain_actions().for_each(drop);
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: Some(60000.0),
            amount: None,
        },
    );
    assert!(
        writes(&actions).is_empty(),
        "uncertain remaining size cannot authorize the destructive cancel"
    );
    assert!(
        done(&actions)
            .unwrap_err()
            .contains("awaiting order reconciliation")
    );
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Open);
    let current = json!([{"orderId":"1","symbol":"BTCUSDT","side":"SELL","type":"LIMIT","status":"PARTIALLY_FILLED","origQty":"0.2","executedQty":"0.15","price":"59000","updateTime":1773295701000u64}]);
    let actions = respond_account_reads(&mut e, before_fill, current.clone(), json!([]));
    let blocked = command(
        &mut e,
        Command::AmendOrder {
            command: 3,
            id: "1".into(),
            price: Some(60000.0),
            amount: None,
        },
    );
    assert!(
        writes(&blocked).is_empty(),
        "the older snapshot cannot clear fill uncertainty"
    );
    assert!(
        done(&blocked)
            .unwrap_err()
            .contains("awaiting order reconciliation")
    );
    let refresh = actions
        .into_iter()
        .find_map(|action| match action {
            Action::SetTimer {
                timer,
                at_ms: 3001.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(3001.0, Event::TimerFired { timer: refresh });
    let fresh = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    respond_account_reads(&mut e, fresh, current, json!([]));
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 2,
            id: "1".into(),
            price: Some(60000.0),
            amount: None,
        },
    );
    assert_eq!(
        writes(&actions).len(),
        1,
        "a fresh complete snapshot permits an explicit caller retry"
    );
    assert_eq!(writes(&actions)[0].method, Method::Delete);
}
