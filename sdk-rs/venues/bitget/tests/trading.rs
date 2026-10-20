mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, Event, Method};
use serde_json::{Value, json};
use support::*;

fn opts(value: Value) -> PlaceOrderOpts {
    serde_json::from_value(value).unwrap()
}
fn place(h: &mut Harness, values: Vec<Value>) {
    h.command(Command::PlaceOrders {
        command: 1,
        orders: values.into_iter().map(opts).collect(),
    });
}
fn limit() -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.0021,"price":90.04})
}
fn body(h: &mut Harness, path: &str) -> (safe_cex_engine::RequestId, Value) {
    let request = h.take(path);
    (
        request.id,
        serde_json::from_str(request.body.as_deref().unwrap()).unwrap(),
    )
}
fn batch_success(h: &mut Harness, request: safe_cex_engine::RequestId, ids: &[&str]) {
    h.reply(request,200,&json!({"code":"00000","data":{"successList":ids.iter().map(|id|json!({"orderId":id})).collect::<Vec<_>>(),"failureList":[]}}).to_string());
}
fn order_event(h: &mut Harness, value: Value) {
    h.private_message(
        &json!({"arg":{"channel":"orders"},"action":"snapshot","data":[value]}).to_string(),
    );
}
fn uta() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    h.respond(
        "/current-position",
        200,
        &POSITION
            .replace("\"data\":[", "\"data\":{\"list\":[")
            .replace("]}", "]}}"),
    );
    h.respond(
        "/account/assets",
        200,
        r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"900","unrealisedPnl":"0"}}"#,
    );
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-strategy-orders"))
    {
        h.respond(
            "/unfilled-strategy-orders",
            200,
            r#"{"code":"00000","data":[]}"#,
        );
    }
    h.login();
    h.engine.store_mut().set_hedged(true);
    h
}

#[test]
fn place_honors_caller_id_rounds_and_uses_broker_header() {
    let mut h = Harness::classic();
    let mut o = limit();
    o["clientOrderId"] = json!("caller.id:/_-1");
    place(&mut h, vec![o]);
    let request = h.take("/batch-place-order");
    assert_eq!(request.method, Method::Post);
    assert!(
        request
            .headers
            .contains(&("X-CHANNEL-API-CODE", "Tealstreet".into()))
    );
    let p: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(p["symbol"], "BTCUSDT");
    assert_eq!(p["marginCoin"], "USDT");
    assert_eq!(p["marginMode"], "crossed");
    assert_eq!(p["orderList"][0]["size"], "0.002");
    assert_eq!(p["orderList"][0]["price"], "90");
    assert_eq!(p["orderList"][0]["clientOid"], "caller.id:/_-1");
    assert_eq!(p["orderList"][0]["reduceOnly"], "NO");
    batch_success(&mut h, request.id, &["42"]);
    assert_eq!(h.done.last().unwrap().1.as_ref().unwrap(), "[\"42\"]");
}

#[test]
fn all_caller_ids_are_checked_before_any_order_write() {
    let mut h = Harness::classic();
    let good = limit();
    let mut bad = limit();
    bad["clientOrderId"] = json!("bad id");
    place(&mut h, vec![good, bad]);
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
    assert!(h.done[0].1.is_err());
    let mut one = limit();
    one["clientOrderId"] = json!("duplicate");
    place(&mut h, vec![one.clone(), one]);
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
}

#[test]
fn bybit_controls_reject_the_whole_batch_before_any_order_write() {
    for uta_mode in [false, true] {
        for (field, value) in [
            ("bybitBbo", json!({"sideType":"Queue","level":1})),
            ("bybitSlippageToleranceTicks", json!(2)),
        ] {
            let mut h = if uta_mode { uta() } else { Harness::classic() };
            let mut unsupported = limit();
            unsupported[field] = value;
            place(&mut h, vec![limit(), unsupported]);
            assert!(
                h.requests.iter().all(|r| r.method == Method::Get),
                "{field} emitted an order write (UTA: {uta_mode})"
            );
            assert!(
                h.done
                    .last()
                    .unwrap()
                    .1
                    .as_ref()
                    .unwrap_err()
                    .contains("Bybit"),
                "{field} was not rejected as unsupported (UTA: {uta_mode})"
            );
        }
    }
}

#[test]
fn caller_id_rejects_brackets_stops_and_precision_induced_split() {
    for shape in [
        json!({"stopLoss":80}),
        json!({"type":"stop_market","price":80}),
    ] {
        let mut h = Harness::classic();
        let mut o = limit();
        o["clientOrderId"] = json!("caller");
        for (key, value) in shape.as_object().unwrap() {
            o[key] = value.clone();
        }
        place(&mut h, vec![o]);
        assert!(h.done[0].1.is_err());
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
    }
    let mut h = Harness::classic();
    h.engine.store_mut().update_markets(|m| {
        m.precision.amount = 1.0;
        m.limits.amount.max = 10.6;
    });
    let mut o = limit();
    o["amount"] = json!(10.6);
    o["clientOrderId"] = json!("caller");
    place(&mut h, vec![o]);
    assert!(h.done[0].1.is_err());
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
}

#[test]
fn split_preserves_size_and_batches_at_fifty() {
    let mut h = Harness::classic();
    h.engine.store_mut().update_markets(|m| {
        m.precision.amount = 1.0;
        m.limits.amount.max = 3.0;
    });
    let mut o = limit();
    o["amount"] = json!(10);
    place(&mut h, vec![o]);
    let (request, p) = body(&mut h, "/batch-place-order");
    assert_eq!(
        p["orderList"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["size"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["3", "3", "3", "1"]
    );
    batch_success(&mut h, request, &["a", "b", "c", "d"]);
    let mut h = Harness::classic();
    place(&mut h, (0..51).map(|_| limit()).collect());
    let (request, p) = body(&mut h, "/batch-place-order");
    assert_eq!(p["orderList"].as_array().unwrap().len(), 50);
    let ids: Vec<_> = (0..50).map(|i| i.to_string()).collect();
    let refs: Vec<_> = ids.iter().map(String::as_str).collect();
    batch_success(&mut h, request, &refs);
    let (_, p) = body(&mut h, "/batch-place-order");
    assert_eq!(p["orderList"].as_array().unwrap().len(), 1);
}

#[test]
fn sized_stop_clamps_and_floors_while_explicit_zero_stays_full_position() {
    let mut h = Harness::classic();
    h.engine
        .store_mut()
        .update_markets(|m| m.precision.amount = 1.0);
    h.engine
        .store_mut()
        .update_positions(|p| p.contracts = 30.6);
    place(
        &mut h,
        vec![json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":31,"price":80})],
    );
    let (request, p) = body(&mut h, "/place-tpsl-order");
    assert_eq!(p["planType"], "loss_plan");
    assert_eq!(p["size"], "30");
    assert_eq!(p["holdSide"], "buy");
    h.reply(request, 200, r#"{"code":"00000","data":{"orderId":"3"}}"#);
    place(
        &mut h,
        vec![
            json!({"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":0,"price":120}),
        ],
    );
    let (_, p) = body(&mut h, "/place-tpsl-order");
    assert_eq!(p["planType"], "pos_profit");
    assert!(p.get("size").is_none());
}

#[test]
fn plan_size_uses_correct_hedge_side_and_dust_downgrades() {
    let mut h = Harness::classic();
    h.engine.store_mut().set_hedged(true);
    h.engine
        .store_mut()
        .update_markets(|m| m.precision.amount = 1.0);
    h.engine.store_mut().update_positions(|p| p.contracts = 0.4);
    place(
        &mut h,
        vec![json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":1,"price":80})],
    );
    let (_, p) = body(&mut h, "/place-tpsl-order");
    assert_eq!(p["holdSide"], "long");
    assert_eq!(p["planType"], "pos_loss");
    assert!(p.get("size").is_none());
}

#[test]
fn hedge_reduce_entry_inverts_classic_wire_side_only() {
    let mut h = Harness::classic();
    h.engine.store_mut().set_hedged(true);
    let mut o = limit();
    o["side"] = json!("sell");
    o["reduceOnly"] = json!(true);
    place(&mut h, vec![o]);
    let (_, p) = body(&mut h, "/batch-place-order");
    let o = &p["orderList"][0];
    assert_eq!(o["side"], "buy");
    assert_eq!(o["tradeSide"], "close");
    assert!(o.get("reduceOnly").is_none());
}

#[test]
fn cancel_plan_retains_immutable_sized_type_after_position_is_flat() {
    let mut h = Harness::classic();
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"3","symbol":"BTCUSDT","size":"1","triggerPrice":"80","holdSide":"long","planType":"sl","status":"live"}]}"#);
    h.engine.store_mut().set_positions(vec![]);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["3".into()],
    });
    let (_, p) = body(&mut h, "/cancel-plan-order");
    assert_eq!(p["planType"], "loss_plan");
}

#[test]
fn native_amend_sends_both_fields_new_client_id_and_returns_new_native_id() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.04),
        amount: None,
    });
    let (request, p) = body(&mut h, "/modify-order");
    assert_eq!(p["newPrice"], "95");
    assert_eq!(p["newSize"], "2");
    assert_ne!(p["newClientOid"], "caller");
    h.reply(request, 200, r#"{"code":"00000","data":{"orderId":"2"}}"#);
    assert_eq!(h.done[0].1.as_ref().unwrap(), "[\"2\"]");
    assert_eq!(
        h.engine.store().order("1").unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(
        h.engine
            .store()
            .order("2")
            .unwrap()
            .client_order_id
            .as_deref(),
        p["newClientOid"].as_str()
    );
}

#[test]
fn async_amend_resolves_only_by_new_client_id_without_repeating_write() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    let (request, p) = body(&mut h, "/modify-order");
    h.reply(request, 200, r#"{"code":"00000","data":{}}"#);
    let read = h.take("/order/detail");
    assert!(read.url.contains(p["newClientOid"].as_str().unwrap()));
    assert_eq!(read.method, Method::Get);
    h.reply(
        read.id,
        200,
        r#"{"code":"43025","msg":"order not yet available","data":null}"#,
    );
    h.timer_after(250.0);
    let read = h.take("/order/detail");
    h.reply(read.id, 200, r#"{"code":"00000","data":{"orderId":"2"}}"#);
    assert_eq!(h.done[0].1.as_ref().unwrap(), "[\"2\"]");
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
}

#[test]
fn unknown_amend_response_never_cancels_or_replaces() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    let request = h.take("/modify-order");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "lost response",
    });
    assert!(h.done[0].1.is_err());
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
}

#[test]
fn rejected_amend_falls_back_but_cancel_failure_prevents_replacement() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    h.respond(
        "/modify-order",
        200,
        r#"{"code":"40017","msg":"cannot modify","data":null}"#,
    );
    let (request, _) = body(&mut h, "/batch-cancel-orders");
    h.reply(request,200,r#"{"code":"00000","data":{"successList":[],"failureList":[{"orderId":"1","errorMsg":"cancel failed"}]}}"#);
    assert!(h.done[0].1.is_err());
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order"))
    );
    assert_eq!(
        h.engine.store().order("1").unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn canceling_virtual_child_cancels_parent_and_tombstones_both_siblings() {
    let mut h = Harness::classic();
    order_event(
        &mut h,
        json!({"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"live","presetStopLossPrice":"80","presetStopSurplusPrice":"120"}),
    );
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["1__stop_loss".into()],
    });
    let (request, p) = body(&mut h, "/batch-cancel-orders");
    assert_eq!(p["orderIdList"], json!([{"orderId":"1"}]));
    batch_success(&mut h, request, &["1"]);
    for id in ["1", "1__stop_loss", "1__take_profit"] {
        assert_eq!(
            h.engine.store().order(id).unwrap().status,
            OrderStatus::Canceled
        );
    }
}

#[test]
fn classic_sized_stop_amend_zero_preserves_size() {
    let mut h = Harness::classic();
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"3","symbol":"BTCUSDT","size":"0.5","triggerPrice":"80","holdSide":"long","planType":"loss_plan","status":"live"}]}"#);
    h.command(Command::AmendOrder {
        command: 1,
        id: "3".into(),
        price: Some(85.0),
        amount: Some(0.0),
    });
    let (_, p) = body(&mut h, "/modify-tpsl-order");
    assert_eq!(p["size"], "0.5");
    assert_eq!(p["triggerPrice"], "85");
}

#[test]
fn uta_entry_and_strategy_use_v3_shapes_and_actual_execution_side() {
    let mut h = uta();
    let mut o = limit();
    o["side"] = json!("sell");
    o["reduceOnly"] = json!(true);
    o["takeProfit"] = json!(120);
    place(&mut h, vec![o]);
    let (request, p) = body(&mut h, "/api/v3/trade/place-order");
    assert_eq!(p["category"], "USDT-FUTURES");
    assert_eq!(p["side"], "sell");
    assert_eq!(p["posSide"], "long");
    assert_eq!(p["qty"], "0.002");
    assert!(p.get("size").is_none());
    assert_eq!(p["takeProfit"], "120");
    h.reply(request, 200, r#"{"code":"00000","data":{"orderId":"4"}}"#);
    place(
        &mut h,
        vec![json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":80})],
    );
    let (_, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["tpslMode"], "partial");
    assert_eq!(p["qty"], "1");
    assert_eq!(p["stopLoss"], "80");
}

#[test]
fn uta_client_only_cancel_omits_native_id_and_strategy_absence_is_idempotent() {
    let mut h = uta();
    h.private_message(r#"{"arg":{"topic":"order"},"data":[{"category":"usdt-futures","clientOid":"strategy__close","symbol":"BTCUSDT","qty":"1","price":"90","side":"buy","orderType":"limit","orderStatus":"new"}]}"#);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["strategy__close".into()],
    });
    let (request, p) = body(&mut h, "/api/v3/trade/cancel-order");
    assert!(p.get("orderId").is_none());
    assert_eq!(p["clientOid"], "strategy__close");
    h.reply(request, 200, r#"{"code":"00000","data":{}}"#);
    h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"usdt-futures","orderId":"stop","symbol":"BTCUSDT","qty":"1","stopLoss":"80","side":"sell","status":"pending"}]}"#);
    h.command(Command::CancelOrders {
        command: 2,
        ids: vec!["stop".into()],
    });
    h.respond("/cancel-strategy-order",400,r#"{"code":"45001","msg":"Failed to stop the strategy because the corresponding strategy ID could not be found","data":null}"#);
    assert!(h.done.last().unwrap().1.is_ok());
}

#[test]
fn uta_zero_amend_resolves_full_position_and_failed_replace_rolls_back_once() {
    let mut h = uta();
    h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"usdt-futures","orderId":"stop","symbol":"BTCUSDT","qty":"0.5","stopLoss":"80","side":"sell","posSide":"long","status":"pending"}]}"#);
    h.command(Command::AmendOrder {
        command: 1,
        id: "stop".into(),
        price: Some(85.0),
        amount: Some(0.0),
    });
    h.respond(
        "/cancel-strategy-order",
        200,
        r#"{"code":"00000","data":{}}"#,
    );
    assert_eq!(h.progress.len(), 1);
    let (request, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["qty"], "1");
    assert_eq!(h.progress[0].1[0].identity, p["clientOid"]);
    assert_eq!(p["stopLoss"], "85");
    h.reply(
        request,
        200,
        r#"{"code":"40017","msg":"replace rejected","data":null}"#,
    );
    let (rollback, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["qty"], "0.5");
    assert_eq!(h.progress.len(), 2);
    assert_eq!(h.progress[1].1[0].identity, p["clientOid"]);
    assert_ne!(h.progress[0].1[0].identity, h.progress[1].1[0].identity);
    assert_eq!(p["stopLoss"], "80");
    h.reply(
        rollback,
        200,
        r#"{"code":"00000","data":{"orderId":"restored"}}"#,
    );
    assert!(
        h.done[0]
            .1
            .as_ref()
            .unwrap_err()
            .contains("replace rejected")
    );
}

#[test]
fn setting_commands_update_store_only_after_ack_and_failures_preserve_settings() {
    let mut h = Harness::classic();
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
        leverage: 500.0,
        side: None,
    });
    let (request, p) = body(&mut h, "/set-leverage");
    assert_eq!(p["leverage"].as_f64(), Some(125.0));
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    h.reply(request, 200, r#"{"code":"00000","data":{}}"#);
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 125.0);
    h.command(Command::SetMarginType {
        command: 2,
        symbol: "BTCUSDT".into(),
        isolated: true,
    });
    h.respond(
        "/set-margin-mode",
        200,
        r#"{"code":"40017","msg":"failed","data":null}"#,
    );
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Cross)
    );
    h.command(Command::SetPositionMode {
        command: 3,
        hedged: true,
        symbol: None,
    });
    h.respond("/set-position-mode", 200, r#"{"code":"00000","data":{}}"#);
    assert!(h.engine.store().hedged());
}

#[test]
fn metadata_for_flat_market_resolves_mode_and_reads_retry_without_writes() {
    let mut h = Harness::classic();
    h.engine.store_mut().set_positions(vec![]);
    place(&mut h, vec![limit()]);
    let request = h.take("/account/account?");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "network reset",
    });
    h.timer_after(250.0);
    let request = h.take("/account/account?");
    h.reply(request.id,200,r#"{"code":"00000","data":{"posMode":"hedge_mode","marginMode":"isolated","isolatedLongLever":"10","isolatedShortLever":"10"}}"#);
    let (_, p) = body(&mut h, "/batch-place-order");
    assert_eq!(p["marginMode"], "isolated");
    assert_eq!(p["orderList"][0]["tradeSide"], "open");
}

#[test]
fn post_write_private_watchdog_ignores_pongs_and_promotes_classic_fallback() {
    let mut h = Harness::classic();
    place(&mut h, vec![limit()]);
    let (request, _) = body(&mut h, "/batch-place-order");
    batch_success(&mut h, request, &["4"]);
    h.now += 4_000.0;
    h.private_message("pong");
    h.now += 1_000.0;
    for (timer, at) in h.timers.clone() {
        if at <= h.now {
            h.event(Event::TimerFired { timer });
        }
    }
    assert!(h.socket_contexts.iter().any(|(_, url, context)| {
        url == "wss://ws.bitget.com/v2/ws/private"
            && context
                .as_ref()
                .is_some_and(|context| context.feed == "private" && context.recover)
    }));
    assert!(h.sockets.iter().all(|(_, url)| !url.contains("tealstreet")));
}

#[test]
fn amend_old_ack_is_resolved_and_terminal_detail_is_preserved() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    let request = h.take("/modify-order");
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"orderId":"1"}}"#,
    );
    let read = h.take("/order/detail");
    h.reply(
        read.id,
        200,
        r#"{"code":"00000","data":{"orderId":"2","status":"filled","baseVolume":"2"}}"#,
    );
    assert_eq!(
        h.engine.store().order("2").unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(h.engine.store().order("2").unwrap().filled, 2.0);
}

#[test]
fn amend_old_cancellation_and_old_rest_cannot_hide_new_identity() {
    let mut h = Harness::classic();
    let old = h.take("/orders-pending");
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    let request = h.take("/modify-order");
    order_event(
        &mut h,
        json!({"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"canceled"}),
    );
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"orderId":"2"}}"#,
    );
    h.reply(
        old.id,
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    h.respond(
        "planType=profit_loss",
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    h.respond(
        "planType=track_plan",
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    assert_eq!(
        h.engine.store().order("2").unwrap().status,
        OrderStatus::Open
    );
    h.command(Command::CancelOrders {
        command: 2,
        ids: vec!["2".into()],
    });
    assert!(h.take("/batch-cancel-orders").body.unwrap().contains("2"));
}

#[test]
fn unknown_server_amend_failure_never_replaces() {
    for (status, code) in [
        (200, 40010),
        (200, 40725),
        (200, 45001),
        (400, 40010),
        (400, 40725),
        (400, 45001),
    ] {
        let mut h = Harness::classic();
        h.command(Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: Some(95.0),
            amount: None,
        });
        let r = h.take("/modify-order");
        h.reply(
            r.id,
            status,
            &json!({"code":code.to_string(),"msg":"unknown result","data":null}).to_string(),
        );
        assert!(h.done[0].1.is_err());
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
    }
}

#[test]
fn late_uta_write_wait_has_a_deadline_after_probe_failure() {
    let mut h = Harness::classic();
    place(&mut h, vec![limit()]);
    h.respond(
        "/batch-place-order",
        200,
        r#"{"code":"40085","msg":"Classic API unsupported","data":null}"#,
    );
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40014","msg":"bad passphrase","data":null}"#,
    );
    h.timer_after(10_000.0);
    assert!(
        h.done[0]
            .1
            .as_ref()
            .unwrap_err()
            .contains("account detection")
    );
}

#[test]
fn isolated_hedge_leverage_ack_only_updates_the_requested_long_side() {
    for uta_account in [false, true] {
        let mut h = if uta_account {
            uta()
        } else {
            Harness::classic()
        };
        h.engine.store_mut().set_hedged(true);
        h.engine
            .store_mut()
            .update_setting("BTCUSDT", |s| s.margin_type = Some(MarginType::Isolated));
        let mut long = h.engine.store().positions()[0].clone();
        long.id = "long".into();
        long.side = PositionSide::Long;
        long.leverage = 10.0;
        long.margin_type = Some(MarginType::Isolated);
        let mut short = long.clone();
        short.id = "short".into();
        short.side = PositionSide::Short;
        h.engine.store_mut().set_positions(vec![long, short]);
        h.command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
            side: None,
        });
        let (request, p) = body(&mut h, "/set-leverage");
        assert_eq!(p[if uta_account { "posSide" } else { "holdSide" }], "long");
        h.reply(request, 200, r#"{"code":"00000","data":{}}"#);
        assert_eq!(h.engine.store().position("long").unwrap().leverage, 20.0);
        assert_eq!(h.engine.store().position("short").unwrap().leverage, 10.0);
    }
}

#[test]
fn split_batch_and_uta_receipts_match_only_dispatched_native_ids() {
    for unified in [false, true] {
        let mut h = if unified { uta() } else { Harness::classic() };
        h.engine.store_mut().update_markets(|m| {
            m.precision.amount = 1.0;
            m.limits.amount.max = 3.0;
        });
        let mut o = limit();
        o["amount"] = json!(7);
        place(&mut h, vec![o]);
        let mut ids = Vec::new();
        for _ in 0..if unified { 3 } else { 1 } {
            let request = h.take(if unified {
                "/place-order"
            } else {
                "/batch-place-order"
            });
            let p: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            let count = h
                .progress_at_request
                .iter()
                .find(|(id, _)| *id == request.id)
                .unwrap()
                .1;
            let (command, receipts) = &h.progress[count - 1];
            assert_eq!(*command, 1);
            let actual: Vec<_> = if unified {
                vec![p["clientOid"].as_str().unwrap()]
            } else {
                p["orderList"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|o| o["clientOid"].as_str().unwrap())
                    .collect()
            };
            assert_eq!(
                receipts
                    .iter()
                    .map(|r| r.identity.as_str())
                    .collect::<Vec<_>>(),
                actual
            );
            assert!(receipts.iter().all(|r| r.symbol == "BTCUSDT"));
            ids.extend(actual.into_iter().map(str::to_owned));
            if unified {
                h.reply(
                    request.id,
                    200,
                    r#"{"code":"00000","data":{"orderId":"created"}}"#,
                );
            } else {
                batch_success(&mut h, request.id, &["a", "b", "c"]);
            }
        }
        assert_eq!(ids.len(), 3);
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            3
        );
    }
}
#[test]
fn native_amend_receipt_excludes_unused_fallback_replacement() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 91,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    let request = h.take("/modify-order");
    let p: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(h.progress.len(), 1);
    assert_eq!(h.progress[0].0, 91);
    assert_eq!(h.progress[0].1.len(), 1);
    assert_eq!(h.progress[0].1[0].identity, p["newClientOid"]);
    assert_eq!(h.progress[0].1[0].symbol, "BTCUSDT");
    h.reply(
        request.id,
        200,
        r#"{"code":"40017","msg":"cannot modify","data":null}"#,
    );
    let cancel = h.take("/batch-cancel-orders");
    h.reply(cancel.id,200,r#"{"code":"00000","data":{"successList":[],"failureList":[{"orderId":"1","errorMsg":"cancel failed"}]}}"#);
    assert_eq!(h.progress.len(), 1);
}

#[test]
fn cancellation_missing_from_store_errors_before_any_write_even_in_a_mixed_batch() {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    use safe_cex_engine::{Action, Notice};
    for mixed in [false, true] {
        let mut h = Harness::classic();
        h.engine.store_mut().set_orders(vec![Order {
            id: "123".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 90.0,
            amount: 1.0,
            filled: 0.0,
            remaining: 1.0,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }]);
        h.engine.drain_actions().for_each(drop);
        h.engine.handle(
            2000.0,
            Event::Command(Command::CancelOrders {
                command: 991,
                ids: if mixed {
                    vec!["123".into(), "456".into()]
                } else {
                    vec!["456".into()]
                },
            }),
        );
        let actions = h.engine.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandDone { command: 991, result: Err(error) })
                    if error.contains("456") && error.contains("not found")
            )),
            "missing cancellation error: {actions:?}"
        );
        assert_eq!(
            h.engine.store().order("123").unwrap().status,
            OrderStatus::Open
        );
    }
}
