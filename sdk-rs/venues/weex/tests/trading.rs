//! Contract V3 documented trading examples and adversarial lifecycle contracts.
use safe_cex_engine::model::*;
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Response, SocketId,
};
mod support;
use safe_cex_venue_weex::WeexFutures;
use serde_json::{Value, json};
use support::setup::Options;

fn engine() -> Engine<WeexFutures> {
    engine_with_private_ws(false)
}
fn engine_with_private_ws(private_ws_headers: bool) -> Engine<WeexFutures> {
    engine_with(private_ws_headers, |local| local)
}
/// The provider signs (`true`) or refuses (`false`) requests in plan order.
fn gated_engine(plan: &[bool]) -> Engine<WeexFutures> {
    engine_with(false, |inner| {
        std::sync::Arc::new(support::setup::Gated::new(inner, plan))
    })
}
fn engine_with(
    private_ws_headers: bool,
    wrap: impl FnOnce(safe_cex_engine::signing::Provider) -> safe_cex_engine::signing::Provider,
) -> Engine<WeexFutures> {
    engine_from(
        Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("passphrase".into()),
            private_ws_headers,
            ..Options::default()
        },
        wrap,
    )
}
fn engine_from(
    options: Options,
    wrap: impl FnOnce(safe_cex_engine::signing::Provider) -> safe_cex_engine::signing::Provider,
) -> Engine<WeexFutures> {
    let mut e = support::setup::engine_with_provider(options, wrap);
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
        stp_mode: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
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
    safe_cex_engine::test_support::prepared_actions(e, 1000.0)
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
    safe_cex_engine::test_support::prepared_actions(e, 1001.0)
}
fn done(actions: &[Action]) -> Result<String, String> {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(Ok(result.clone())),
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(Err(error.legacy_text())),
            _ => None,
        })
        .expect("completed command")
}
fn failure(actions: &[Action]) -> &safe_cex_engine::VenueError {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("failed command")
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
fn bybit_execution_controls_reject_entire_batch_before_writes() {
    for bbo in [true, false] {
        let mut e = engine();
        let mut second = opts("second");
        if bbo {
            second.bybit_bbo = Some(BybitBbo {
                side_type: "Queue".into(),
                level: 1.0,
            });
        } else {
            second.bybit_slippage_tolerance_ticks = Some(2.0);
        }
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![opts("first"), second],
            },
        );
        assert!(writes(&actions).is_empty());
        assert!(e.store().orders().is_empty());
        assert!(done(&actions).unwrap_err().contains("Bybit"));
    }
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
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 1001.0);
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
            side: None,
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
            side: None,
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
            symbol: None,
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
            side: None,
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
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandProgress { .. })))
    );
    let actions = response(&mut e, &r, json!({"orderId":"1","success":true}));
    let r = writes(&actions).pop().unwrap();
    assert!(r.url.ends_with("algoOrder"));
    assert_eq!(
        receipt_rows(&actions, 1)[0].identity,
        body(&r)["clientAlgoId"]
    );
    assert_eq!(body(&r)["triggerPrice"], "68900");
    assert_eq!(body(&r)["price"], "68800");
    assert_eq!(body(&r)["quantity"], "0.3");
}

#[test]
fn overlapping_cancel_rejects_and_late_failure_cannot_rollback_the_retry() {
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
    assert!(writes(&actions).is_empty());
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command: 2, error })
        if error.legacy_text() == "Order amendment or cancellation is already in flight")));
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Canceling);
    e.handle(
        1002.0,
        Event::HttpFailed {
            request: first.id,
            error: "first timeout",
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(writes(&actions).is_empty());
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command: 1, error })
        if error.message == "first timeout" && error.outcome == Some(safe_cex_engine::Outcome::Unknown))));
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Open);
    let actions = command(
        &mut e,
        Command::CancelOrders {
            command: 3,
            ids: vec!["1".into()],
        },
    );
    let second = writes(&actions).pop().unwrap();
    assert_eq!(e.store().order("1").unwrap().status, OrderStatus::Canceling);
    e.handle(
        1003.0,
        Event::HttpFailed {
            request: first.id,
            error: "late duplicate first timeout",
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Http(_) | Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
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
            side: None,
        },
    );
    let req = writes(&actions).pop().unwrap();
    let actions = response(e, &req, json!({"code":"200"}));
    let socket = actions.iter().find_map(|action| match action {
        Action::WsConnect {
            socket, headers, ..
        } if !headers.is_empty() => Some(*socket),
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
fn imported_regular_order_with_brackets_amends_with_row_or_default_working_types() {
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
    let cancel = writes(&actions).pop().unwrap();
    assert_eq!(cancel.method, Method::Delete);
    let actions = response(&mut e, &cancel, json!([{"orderId":"1","success":true}]));
    let replacement = writes(&actions).pop().unwrap();
    let payload = body(&replacement);
    assert_eq!(payload["tpTriggerPrice"], "70000");
    assert_eq!(payload["slTriggerPrice"], "58000");
    assert_eq!(payload["TpWorkingType"], "CONTRACT_PRICE");
    assert_eq!(payload["SlWorkingType"], "MARK_PRICE");
    let ack = single_ack(&replacement, 2);
    assert_eq!(
        done(&response(&mut e, &replacement, ack)).unwrap(),
        r#"["2"]"#
    );
}
#[test]
fn imported_regular_order_amend_keeps_row_reported_leg_working_types() {
    let mut e = engine();
    import_orders(
        &mut e,
        json!([{"orderId":"1","symbol":"BTCUSDT","side":"SELL","type":"LIMIT","status":"NEW","origQty":"0.2","executedQty":"0","price":"59000","tpTriggerPrice":"50000","slTriggerPrice":"61000","tpOrderType":"MARK_PRICE","workingType":"CONTRACT_PRICE","updateTime":1773295700000u64}]),
        json!([]),
    );
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: Some(59500.0),
            amount: None,
        },
    );
    let cancel = writes(&actions).pop().unwrap();
    let actions = response(&mut e, &cancel, json!([{"orderId":"1","success":true}]));
    let replacement = writes(&actions).pop().unwrap();
    assert_eq!(body(&replacement)["TpWorkingType"], "MARK_PRICE");
    assert_eq!(body(&replacement)["SlWorkingType"], "CONTRACT_PRICE");
}
#[test]
fn accepted_conditional_echoing_algo_identity_is_reported_accepted() {
    for ack in [
        |client: &Value| json!({"algoId":"77","clientAlgoId":client,"success":true}),
        |_: &Value| json!({"orderId":"77","success":true}),
    ] {
        let mut e = engine();
        let mut conditional = opts("");
        conditional.kind = OrderType::Conditional;
        conditional.trigger_price = Some(68900.0);
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![conditional],
            },
        );
        let r = writes(&actions).pop().unwrap();
        assert!(r.url.ends_with("/algoOrder"));
        let actions = response(&mut e, &r, ack(&body(&r)["clientAlgoId"]));
        assert_eq!(done(&actions).unwrap(), r#"["77"]"#);
        assert_eq!(e.store().order("77").unwrap().kind, OrderType::Conditional);
    }
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
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandProgress { .. })))
        );
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
            side: None,
        },
    );
    let first = writes(&actions).pop().unwrap();
    let actions = command(
        &mut e,
        Command::SetLeverage {
            command: 2,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
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
            side: None,
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

fn receipt_rows(actions: &[Action], command: u32) -> Vec<OrderIdentityReceipt> {
    let (notice, rows) = actions
        .iter()
        .enumerate()
        .find_map(|(index, action)| match action {
            Action::Notice(Notice::CommandProgress {
                command: owner,
                receipts,
            }) if *owner == command => Some((index, receipts.clone())),
            _ => None,
        })
        .expect("native receipts");
    let write = actions
        .iter()
        .position(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
        .expect("write");
    assert!(notice < write);
    rows
}

#[test]
fn receipt_batches_follow_exact_wire_ids_for_five_plus_tail_without_early_tail_ids() {
    let mut e = engine();
    let first = command(
        &mut e,
        Command::PlaceOrders {
            command: 77,
            orders: (0..6).map(|_| opts("")).collect(),
        },
    );
    let request = writes(&first).pop().unwrap();
    let expected: Vec<_> = body(&request)["batchOrders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| OrderIdentityReceipt {
            symbol: "BTCUSDT".into(),
            kind: OrderIdentityKind::ClientOrderId,
            identity: row["newClientOrderId"].as_str().unwrap().into(),
        })
        .collect();
    assert_eq!(expected.len(), 5);
    assert_eq!(receipt_rows(&first, 77), expected);
    let next = response(&mut e, &request, batch_ack(&request, 100));
    let tail = writes(&next).pop().unwrap();
    let id = body(&tail)["batchOrders"][0]["newClientOrderId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        receipt_rows(&next, 77),
        vec![OrderIdentityReceipt {
            symbol: "BTCUSDT".into(),
            kind: OrderIdentityKind::ClientOrderId,
            identity: id.clone()
        }]
    );
    assert!(expected.iter().all(|row| row.identity != id));
}

#[test]
fn receipts_match_algo_protection_and_attached_parent_ids_without_invented_children() {
    for kind in [
        OrderType::Conditional,
        OrderType::TakeProfit,
        OrderType::StopLoss,
        OrderType::Limit,
    ] {
        let mut e = engine();
        let mut order = opts("");
        order.kind = kind;
        if kind == OrderType::Conditional {
            order.trigger_price = Some(70000.0);
        }
        if matches!(kind, OrderType::TakeProfit | OrderType::StopLoss) {
            order.reduce_only = Some(true);
        }
        if kind == OrderType::Limit {
            order.take_profit = Some(71000.0);
            order.stop_loss = Some(68000.0);
        }
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 78,
                orders: vec![order],
            },
        );
        let request = writes(&actions).pop().unwrap();
        let payload = body(&request);
        let id = payload
            .get("newClientOrderId")
            .unwrap_or(&payload["clientAlgoId"])
            .as_str()
            .unwrap();
        assert_eq!(
            receipt_rows(&actions, 78),
            vec![OrderIdentityReceipt {
                symbol: "BTCUSDT".into(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: id.into()
            }]
        );
    }
}
#[test]
fn isolated_side_leverage_keeps_the_other_payload_and_setting_leg() {
    for side in [Some(PositionSide::Long), Some(PositionSide::Short), None] {
        let mut e = engine();
        e.store_mut().update_setting("BTCUSDT", |s| {
            s.margin_type = Some(MarginType::Isolated);
            s.leverage = 10.;
            s.buy_leverage = Some(10.);
            s.sell_leverage = Some(20.);
        });
        let a = command(
            &mut e,
            Command::SetLeverage {
                command: 601,
                symbol: "BTCUSDT".into(),
                leverage: 37.,
                side,
            },
        );
        let r = &writes(&a)[0];
        let long = if side == Some(PositionSide::Short) {
            10.
        } else {
            37.
        };
        let short = if side == Some(PositionSide::Long) {
            20.
        } else {
            37.
        };
        assert_eq!(
            body(r),
            json!({"symbol":"BTCUSDT","marginType":"ISOLATED","isolatedLongLeverage":long.to_string(),"isolatedShortLeverage":short.to_string()})
        );
        let a = response(&mut e, r, json!({"code":"200","msg":"success"}));
        assert!(done(&a).is_ok());
        let s = e.store().setting("BTCUSDT").unwrap();
        assert_eq!((s.buy_leverage, s.sell_leverage), (Some(long), Some(short)));
    }
}

#[test]
fn weex_side_is_coupled_in_cross_and_rejections_keep_settings() {
    let mut e = engine();
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 602,
            symbol: "BTCUSDT".into(),
            leverage: 37.,
            side: Some(PositionSide::Short),
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(
        body(r),
        json!({"symbol":"BTCUSDT","marginType":"CROSSED","crossLeverage":"37"})
    );
    let a = response(&mut e, r, json!({"code":"200","msg":"success"}));
    assert!(done(&a).is_ok());
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(37.)
    );
    e.store_mut().update_setting("BTCUSDT", |s| {
        s.margin_type = Some(MarginType::Isolated);
        s.sell_leverage = Some(20.);
    });
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 603,
            symbol: "BTCUSDT".into(),
            leverage: 50.,
            side: Some(PositionSide::Short),
        },
    );
    let a = response(
        &mut e,
        &writes(&a)[0],
        json!({"code":"400","msg":"rejected"}),
    );
    assert!(done(&a).is_err());
    let s = e.store().setting("BTCUSDT").unwrap();
    assert_eq!((s.buy_leverage, s.sell_leverage), (Some(37.), Some(20.)));
}

#[test]
fn targeted_weex_mode_changes_only_one_symbol_and_keeps_other_mode_in_requests() {
    let mut e = engine();
    let mut markets = e.store().markets().to_vec();
    let mut eth = markets[0].clone();
    eth.id = "ETHUSDT".into();
    eth.symbol = "ETHUSDT".into();
    eth.base = "ETH".into();
    markets.push(eth);
    e.store_mut().set_markets(markets);
    e.store_mut().update_setting("ETHUSDT", |s| {
        s.leverage = 10.;
        s.margin_type = Some(MarginType::Isolated);
    });
    let a = command(
        &mut e,
        Command::SetPositionMode {
            command: 801,
            hedged: true,
            symbol: Some("ETHUSDT".into()),
        },
    );
    assert_eq!(writes(&a).len(), 1);
    let r = &writes(&a)[0];
    assert_eq!(
        body(r),
        json!({"symbol":"ETHUSDT","marginType":"ISOLATED","separatedType":"SEPARATED"})
    );
    let a = response(&mut e, r, json!({"code":"200","msg":"success"}));
    assert!(done(&a).is_ok());
    assert!(writes(&a).is_empty());
    let a = command(
        &mut e,
        Command::SetPositionMode {
            command: 802,
            hedged: false,
            symbol: Some("BTCUSDT".into()),
        },
    );
    assert_eq!(writes(&a).len(), 1);
    let r = &writes(&a)[0];
    assert_eq!(body(r)["symbol"], "BTCUSDT");
    let a = response(&mut e, r, json!({"code":"200","msg":"success"}));
    assert!(done(&a).is_ok());
    assert!(e.store().hedged());
    let a = command(
        &mut e,
        Command::SetMarginType {
            command: 803,
            symbol: "ETHUSDT".into(),
            isolated: false,
        },
    );
    assert_eq!(body(&writes(&a)[0])["separatedType"], "SEPARATED");
}

#[test]
fn weex_unknown_or_rejected_targeted_mode_does_not_emit_other_symbol_writes() {
    for symbol in ["", "UNKNOWN"] {
        let mut e = engine();
        let a = command(
            &mut e,
            Command::SetPositionMode {
                command: 804,
                hedged: true,
                symbol: Some(symbol.into()),
            },
        );
        assert!(writes(&a).is_empty());
        assert!(done(&a).is_err());
    }
    let mut e = engine();
    let a = command(
        &mut e,
        Command::SetPositionMode {
            command: 805,
            hedged: true,
            symbol: Some("BTCUSDT".into()),
        },
    );
    let a = response(
        &mut e,
        &writes(&a)[0],
        json!({"code":"400","msg":"rejected"}),
    );
    assert!(done(&a).is_err());
    assert!(!e.store().hedged());
    assert!(writes(&a).is_empty());
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for (status, body, kind, outcome) in [
        (
            400,
            r#"{"code":"40015","msg":"Insufficient balance"}"#,
            None,
            Outcome::Unknown,
        ),
        (
            429,
            r#"{"code":"429","msg":"Too many requests"}"#,
            Some(ErrorKind::RateLimited),
            Outcome::Unknown,
        ),
        (
            502,
            "Bad Gateway",
            Some(ErrorKind::VenueUnavailable),
            Outcome::Unknown,
        ),
    ] {
        let mut e = engine();
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 4,
                orders: vec![opts("classified")],
            },
        );
        let r = writes(&actions).pop().unwrap();
        e.handle(
            1001.0,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        let error = failure(&actions);
        if status == 502 {
            assert_eq!(
                error.message,
                "HTTP response does not prove the write outcome"
            );
        } else {
            assert_eq!(error.message, format!("HTTP {status}: {body}"));
        }
        assert_eq!(
            (error.kind, error.outcome),
            (kind, Some(outcome)),
            "{status}"
        );
    }
}

#[test]
fn a_rejection_after_an_accepted_step_is_not_reported_as_rejected() {
    let mut e = engine();
    let orders: Vec<_> = (0..6).map(|i| opts(&format!("lot-{i}"))).collect();
    let actions = command(&mut e, Command::PlaceOrders { command: 5, orders });
    let first = writes(&actions).pop().unwrap();
    let ack = batch_ack(&first, 100);
    let actions = response(&mut e, &first, ack);
    let second = writes(&actions).pop().expect("second lot");
    e.handle(
        1002.0,
        Event::HttpResponse {
            request: second.id,
            response: Response {
                status: 400,
                headers: "",
                body: r#"{"code":"40015","msg":"Insufficient balance"}"#,
            },
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let error = failure(&actions);
    assert_eq!(
        error.outcome,
        Some(safe_cex_engine::Outcome::Unknown),
        "40015 does not prove refusal: {error:?}"
    );
    assert_eq!(
        error
            .partial
            .as_ref()
            .map(|partial| partial.order_ids.clone()),
        Some((100..105).map(|id| id.to_string()).collect())
    );
}

fn leverage_read(e: &mut Engine<WeexFutures>, command_id: u32) -> HttpRequest {
    let actions = command(
        e,
        Command::FetchLeverage {
            command: command_id,
            symbol: "BTCUSDT".into(),
        },
    );
    let mut reads: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.contains("/account/symbolConfig") => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(reads.len(), 1, "one fresh symbol configuration read");
    reads.pop().unwrap()
}
fn command_errors(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandError { message, .. }) => Some(message.clone()),
            _ => None,
        })
        .collect()
}
const ISOLATED_CONFIG: &str = r#"[{"symbol":"BTCUSDT","marginType":"ISOLATED","separatedType":"SEPARATED","crossLeverage":"20","isolatedLongLeverage":"7","isolatedShortLeverage":"9"}]"#;

#[test]
fn leverage_read_is_a_fresh_symbol_scoped_configuration_read() {
    let mut e = engine();
    let r = leverage_read(&mut e, 40);
    assert_eq!(r.method, Method::Get);
    assert!(r.url.contains("symbol=BTCUSDT"), "{}", r.url);
    assert!(r.headers.iter().any(|(k, _)| *k == "ACCESS-SIGN"));
    let actions = raw_response(&mut e, &r, ISOLATED_CONFIG);
    assert_eq!(
        done(&actions),
        Ok(r#"{"buyLeverage":7.0,"sellLeverage":9.0,"marginType":"isolated"}"#.into())
    );
    assert!(command_errors(&actions).is_empty());
    let setting = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(7.0));
    assert_eq!(setting.sell_leverage, Some(9.0));
    assert_eq!(setting.margin_type, Some(MarginType::Isolated));
}

#[test]
fn leverage_read_failures_report_once_and_keep_settings() {
    let mut e = engine();
    let before = e.store().setting("BTCUSDT");
    for (id, body) in [
        (41, "{\"code\":\"40001\"}"),
        (42, "[]"),
        (
            43,
            r#"[{"symbol":"ETHUSDT","marginType":"CROSSED","separatedType":"COMBINED","crossLeverage":"5","isolatedLongLeverage":"5","isolatedShortLeverage":"5"}]"#,
        ),
        (
            44,
            r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","separatedType":"COMBINED","crossLeverage":"0","isolatedLongLeverage":"5","isolatedShortLeverage":"5"}]"#,
        ),
    ] {
        let r = leverage_read(&mut e, id);
        let actions = raw_response(&mut e, &r, body);
        // One typed failure carries the error; no separate diagnostic.
        assert!(done(&actions).is_err(), "{body}");
        assert!(!failure(&actions).message.is_empty());
        assert!(command_errors(&actions).is_empty());
    }
    assert_eq!(e.store().setting("BTCUSDT"), before);
}

#[test]
fn a_leverage_write_during_the_read_wins_over_its_snapshot() {
    let mut e = engine();
    let read = leverage_read(&mut e, 45);
    let actions = command(
        &mut e,
        Command::SetLeverage {
            command: 46,
            symbol: "BTCUSDT".into(),
            leverage: 50.0,
            side: None,
        },
    );
    let write = writes(&actions).pop().unwrap();
    response(
        &mut e,
        &write,
        json!({"symbol":"BTCUSDT","marginType":"CROSSED","crossLeverage":"50","isolatedLongLeverage":"20","isolatedShortLeverage":"20"}),
    );
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 50.0);
    let actions = raw_response(&mut e, &read, ISOLATED_CONFIG);
    // The answer carries the confirmed write, not the stale snapshot.
    assert_eq!(
        done(&actions),
        Ok(r#"{"buyLeverage":50.0,"sellLeverage":50.0,"marginType":"cross"}"#.into())
    );
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 50.0);
}

#[test]
fn leverage_read_needs_a_known_market() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Command::FetchLeverage {
            command: 47,
            symbol: "ETHUSDT".into(),
        },
    );
    assert_eq!(done(&actions), Err("unknown market ETHUSDT".into()));
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}

fn auth_notices(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(Notice::AccountAuth { .. } | Notice::Deactivate)
            )
        })
        .count()
}

#[test]
fn provider_refusal_of_a_first_write_is_typed_not_sent_and_never_released() {
    let mut e = gated_engine(&[false]);
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 61,
            orders: vec![opts("refused")],
        },
    );
    assert!(
        writes(&actions).is_empty(),
        "a refused write is never released"
    );
    let mut actions = actions;
    actions.extend(e.drain_actions());
    let error = failure(&actions);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
    assert_eq!(
        auth_notices(&actions),
        0,
        "provider refusal is not venue auth"
    );
}

#[test]
fn provider_refusal_after_an_accepted_lot_is_neither_not_sent_nor_rejected() {
    let mut e = gated_engine(&[true, false]);
    let orders: Vec<_> = (0..6).map(|i| opts(&format!("lot-{i}"))).collect();
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 62,
            orders,
        },
    );
    let first = writes(&actions).pop().unwrap();
    let ack = batch_ack(&first, 100);
    let actions = response(&mut e, &first, ack);
    assert!(
        writes(&actions).is_empty(),
        "the second lot is refused before release"
    );
    let error = failure(&actions);
    assert_eq!(error.outcome, None, "the first lot was accepted: {error:?}");
}

#[test]
fn provider_refusal_of_a_leverage_read_is_typed_and_keeps_settings() {
    let mut e = gated_engine(&[false]);
    let before = e.store().setting("BTCUSDT");
    let actions = command(
        &mut e,
        Command::FetchLeverage {
            command: 63,
            symbol: "BTCUSDT".into(),
        },
    );
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "a refused read is never released"
    );
    let error = failure(&actions);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert_eq!(auth_notices(&actions), 0);
    assert_eq!(e.store().setting("BTCUSDT"), before);
}

#[test]
fn a_later_sent_rejection_supersedes_an_earlier_provider_refusal() {
    let mut e = gated_engine(&[false]);
    e.store_mut().upsert_order(order("1", OrderType::Limit));
    e.store_mut().upsert_order(order("2", OrderType::StopLoss));
    let actions = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 64,
            symbol: "BTCUSDT".into(),
        },
    );
    // The regular-order step was refused before release; the algo step was sent.
    let r = writes(&actions).pop().unwrap();
    assert!(
        r.url.ends_with("algoOpenOrders?symbol=BTCUSDT"),
        "{}",
        r.url
    );
    e.handle(
        1001.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 400,
                headers: "",
                body: r#"{"code":"40015","msg":"rejected"}"#,
            },
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let error = failure(&actions);
    assert_ne!(
        error.outcome,
        Some(safe_cex_engine::Outcome::NotSent),
        "a write reached the venue: {error:?}"
    );
}

#[test]
fn a_read_only_account_still_fetches_leverage_with_its_secondary_identity() {
    let mut e = engine_from(
        Options {
            public_data_only: true,
            read_credentials: Some(safe_cex_engine::ReadCredentials {
                key: "read-key".into(),
                secret: "read-secret".into(),
                passphrase: Some("read-pass".into()),
            }),
            ..Options::default()
        },
        |local| local,
    );
    let r = leverage_read(&mut e, 65);
    let key = r.headers.iter().find(|(k, _)| *k == "ACCESS-KEY").unwrap();
    assert_eq!(key.1, "read-key");
    let actions = raw_response(&mut e, &r, ISOLATED_CONFIG);
    assert!(done(&actions).is_ok());
}

#[test]
fn a_late_settings_failure_cannot_clear_a_reused_commands_write_fence() {
    let mut e = engine();
    let setting = || Command::SetLeverage {
        command: 71,
        symbol: "BTCUSDT".into(),
        leverage: 30.,
        side: None,
    };
    let actions = command(&mut e, setting());
    let old = writes(&actions).pop().unwrap();
    assert!(e.store().metadata_write_pending("BTCUSDT"));
    let completed = response(&mut e, &old, json!({"code":"200"}));
    assert_eq!(done(&completed).unwrap(), "[]");
    assert!(!e.store().metadata_write_pending("BTCUSDT"));

    e.handle(2001., Event::Command(setting()));
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 2001.);
    let current = writes(&actions).pop().unwrap();
    assert_ne!(old.id, current.id);
    assert!(e.store().metadata_write_pending("BTCUSDT"));
    e.handle(
        2002.,
        Event::HttpFailed {
            request: old.id,
            error: "late old failure",
        },
    );
    let stale = safe_cex_engine::test_support::prepared_actions(&mut e, 2002.);
    assert!(!stale.iter().any(|action| matches!(
        action,
        Action::Notice(
            Notice::CommandDone { command: 71, .. } | Notice::CommandFailed { command: 71, .. }
        )
    )));
    assert!(e.store().metadata_write_pending("BTCUSDT"));

    e.handle(
        2003.,
        Event::HttpFailed {
            request: current.id,
            error: "current failure",
        },
    );
    let current = safe_cex_engine::test_support::prepared_actions(&mut e, 2003.);
    assert_eq!(
        current
            .iter()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandFailed { command: 71, .. })
            ))
            .count(),
        1
    );
    assert!(!e.store().metadata_write_pending("BTCUSDT"));
}
