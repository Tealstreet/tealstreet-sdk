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
    h.respond(
        "/unfilled-strategy-orders",
        200,
        r#"{"code":"00000","data":[]}"#,
    );
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
    h.private_message(r#"{"arg":{"topic":"order"},"data":[{"category":"usdt-futures","clientOid":"only-client","symbol":"BTCUSDT","qty":"1","price":"90","side":"buy","orderType":"limit","orderStatus":"new"}]}"#);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["only-client".into()],
    });
    let (request, p) = body(&mut h, "/api/v3/trade/cancel-order");
    assert!(p.get("orderId").is_none());
    assert_eq!(p["clientOid"], "only-client");
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
    let (request, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["qty"], "1");
    assert_eq!(p["stopLoss"], "85");
    h.reply(
        request,
        200,
        r#"{"code":"40017","msg":"replace rejected","data":null}"#,
    );
    let (rollback, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["qty"], "0.5");
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
    assert!(
        h.sockets
            .iter()
            .any(|(_, url)| url.contains("/bitget/v2/private"))
    );
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
