mod support;
use safe_cex_engine::log::{LogEvent, Value};
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, Level};
use serde_json::json;
use support::*;

fn events<'a>(h: &'a Harness, code: &str) -> Vec<&'a LogEvent> {
    h.events.iter().filter(|e| e.code == code).collect()
}
fn field<'a>(event: &'a LogEvent, key: &str) -> &'a Value {
    &event
        .fields
        .iter()
        .find(|(name, _)| *name == key)
        .unwrap_or_else(|| panic!("missing {key}: {event:?}"))
        .1
}
fn place(h: &mut Harness, reduce: bool) -> safe_cex_engine::HttpRequest {
    h.command(Command::PlaceOrders {
        command: 71,
        orders: vec![serde_json::from_value(json!({"symbol":"BTCUSDT","type":"limit","side":"sell","amount":0.6,"price":110,"reduceOnly":reduce,"clientOrderId":"close-telemetry"})).unwrap()],
    });
    h.take("/batch-place-order")
}
fn booking(h: &mut Harness, size: &str) {
    h.private_message(&json!({"arg":{"channel":"orders"},"action":"update","data":[{"orderId":"42","clientOid":"close-telemetry","symbol":"BTCUSDT","size":size,"baseVolume":"0","price":"110","side":"sell","orderType":"limit","status":"live","reduceOnly":"YES","posMode":"one_way_mode"}]}).to_string());
}

#[test]
fn classic_startup_reports_modes_once_and_never_for_public_only() {
    let mut h = Harness::classic();
    let rows = events(&h, "bitget.account.mode");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].level, Level::Info);
    assert_eq!(field(rows[0], "accountMode"), &Value::from("classic"));
    assert_eq!(field(rows[0], "posMode"), &Value::from("one_way"));
    assert_eq!(field(rows[0], "marginMode"), &Value::from("crossed"));
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"update","data":[]}"#);
    assert_eq!(events(&h, "bitget.account.mode").len(), 1);
    assert!(events(&Harness::public(), "bitget.account.mode").is_empty());
}

#[test]
fn uta_startup_reports_hedge_and_isolated_from_native_settings() {
    let mut h = Harness::private_started();
    h.respond("/account/settings", 200, r#"{"code":"00000","data":{"holdMode":"hedge_mode","symbolConfigList":[{"symbol":"BTCUSDT","category":"USDT-FUTURES","posMode":"hedge_mode","marginMode":"isolated"}]}}"#);
    h.respond(
        "/current-position",
        200,
        &POSITION
            .replace("\"data\":[", "\"data\":{\"list\":[")
            .replace("]}", "]}}")
            .replace("one_way_mode", "hedge_mode")
            .replace("crossed", "isolated"),
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
    let rows = events(&h, "bitget.account.mode");
    assert_eq!(rows.len(), 1);
    assert_eq!(field(rows[0], "accountMode"), &Value::from("uta"));
    assert_eq!(field(rows[0], "posMode"), &Value::from("hedge"));
    assert_eq!(field(rows[0], "marginMode"), &Value::from("isolated"));
}

#[test]
fn close_refusal_reports_native_reason_and_only_matching_open_reservations() {
    let mut h = Harness::classic();
    let mut regular = h.engine.store().orders()[0].clone();
    regular.id = "reduce".into();
    regular.side = OrderSide::Sell;
    regular.reduce_only = true;
    regular.amount = 0.5;
    regular.filled = 0.1;
    regular.remaining = 0.4;
    let mut plan = regular.clone();
    plan.id = "plan".into();
    plan.kind = OrderType::StopLoss;
    plan.amount = 0.2;
    plan.filled = 0.0;
    plan.remaining = 0.2;
    let mut other_side = regular.clone();
    other_side.id = "other-side".into();
    other_side.side = OrderSide::Buy;
    let mut terminal = regular.clone();
    terminal.id = "terminal".into();
    terminal.status = OrderStatus::Canceled;
    h.engine
        .store_mut()
        .set_orders(vec![regular, plan, other_side, terminal]);
    let r = place(&mut h, true);
    h.reply(
        r.id,
        200,
        r#"{"code":"40757","msg":"Not enough position is available","data":null}"#,
    );
    let rows = events(&h, "bitget.close.diagnostic");
    assert_eq!(rows.len(), 1);
    let e = rows[0];
    assert_eq!(field(e, "reason"), &Value::from("rejected"));
    assert_eq!(field(e, "symbol"), &Value::from("BTCUSDT"));
    assert_eq!(field(e, "side"), &Value::from("long"));
    assert_eq!(field(e, "requestedSize"), &Value::Num(0.6));
    assert_eq!(field(e, "positionTotal"), &Value::Num(1.0));
    assert_eq!(field(e, "reduceOnlyOrderCount"), &Value::Num(1.0));
    assert_eq!(field(e, "reduceOnlyOrderSize"), &Value::Num(0.4));
    assert_eq!(field(e, "planOrderCount"), &Value::Num(1.0));
    assert_eq!(field(e, "planOrderSize"), &Value::Num(0.2));
    assert_eq!(field(e, "nativeCode"), &Value::from("40757"));
    assert_eq!(
        field(e, "nativeMsg"),
        &Value::from("Not enough position is available")
    );
    assert!(h.done.last().unwrap().1.is_err());
}

#[test]
fn batch_close_refusal_is_visible_but_opening_failure_is_not_a_close_diagnostic() {
    for reduce in [true, false] {
        let mut h = Harness::classic();
        let r = place(&mut h, reduce);
        h.reply(r.id, 200, r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorCode":"40757","errorMsg":"Not enough position is available"}]}}"#);
        let rows = events(&h, "bitget.close.diagnostic");
        assert_eq!(rows.len(), usize::from(reduce));
        if reduce {
            assert_eq!(field(rows[0], "nativeCode"), &Value::from("40757"));
            assert_eq!(
                field(rows[0], "nativeMsg"),
                &Value::from("Not enough position is available")
            );
        }
    }
}

#[test]
fn native_booking_size_mismatch_is_reported_once_even_before_the_ack() {
    for before_ack in [false, true] {
        let mut h = Harness::classic();
        let r = place(&mut h, true);
        if before_ack {
            booking(&mut h, "0.4");
        }
        h.reply(r.id, 200, r#"{"code":"00000","data":{"successList":[{"orderId":"42","clientOid":"close-telemetry"}],"failureList":[]}}"#);
        booking(&mut h, "0.4");
        booking(&mut h, "0.3");
        let rows = events(&h, "bitget.close.diagnostic");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            field(rows[0], "reason"),
            &Value::from("booked-size-mismatch")
        );
        assert_eq!(field(rows[0], "requestedSize"), &Value::Num(0.6));
        assert_eq!(field(rows[0], "bookedSize"), &Value::Num(0.4));
        assert_eq!(
            h.engine.store().order("42").unwrap().amount,
            0.3,
            "telemetry must not rewrite native economics"
        );
    }
}

#[test]
fn matching_booking_never_emits_a_close_diagnostic() {
    let mut h = Harness::classic();
    let r = place(&mut h, true);
    h.reply(r.id, 200, r#"{"code":"00000","data":{"successList":[{"orderId":"42","clientOid":"close-telemetry"}],"failureList":[]}}"#);
    booking(&mut h, "0.6");
    booking(&mut h, "0.6");
    assert!(events(&h, "bitget.close.diagnostic").is_empty());
}

#[test]
fn raw_native_code_and_message_are_not_normalized_or_inferred() {
    for code in ["040757", "77777"] {
        let mut h = Harness::classic();
        let r = place(&mut h, true);
        let message = "  Not enough position is available\n\"exact native text\"  ";
        h.reply(
            r.id,
            200,
            &json!({"code":code,"msg":message,"data":null}).to_string(),
        );
        let rows = events(&h, "bitget.close.diagnostic");
        assert_eq!(rows.len(), 1);
        assert_eq!(field(rows[0], "nativeCode"), &Value::from(code));
        assert_eq!(field(rows[0], "nativeMsg"), &Value::from(message));
    }
    let mut h = Harness::classic();
    let r = place(&mut h, true);
    h.reply(r.id, 200, r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":"Not enough position is available"}]}}"#);
    let rows = events(&h, "bitget.close.diagnostic");
    assert_eq!(rows.len(), 1);
    assert_eq!(field(rows[0], "nativeCode"), &Value::from(""));
    assert_eq!(field(rows[0], "nativeCodePresent"), &Value::Bool(false));
    assert_eq!(
        field(rows[0], "nativeMsg"),
        &Value::from("Not enough position is available")
    );
}

#[test]
fn rest_order_read_reports_the_booking_without_rewriting_it() {
    let mut h = Harness::classic();
    let r = place(&mut h, true);
    h.reply(r.id, 200, r#"{"code":"00000","data":{"successList":[{"orderId":"42","clientOid":"close-telemetry"}],"failureList":[]}}"#);
    h.command(Command::Venue {
        command: 72,
        action: safe_cex_engine::VenueAction::RefreshOrders { symbols: None },
    });
    h.orders(r#"{"code":"00000","data":{"entrustedList":[{"orderId":"42","clientOid":"close-telemetry","symbol":"BTCUSDT","size":"0.4","baseVolume":"0","price":"110","side":"sell","orderType":"limit","status":"live","reduceOnly":"YES"}],"endId":""}}"#);
    assert_eq!(events(&h, "bitget.close.diagnostic").len(), 1);
    assert_eq!(h.engine.store().order("42").unwrap().amount, 0.4);
}

#[test]
fn frame_fed_account_modes_and_native_booking_are_logged_once() {
    let mut source = Harness::classic();
    let mut page = Harness::started_configure(
        FixtureOptions {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..FixtureOptions::default()
        },
        false,
        |e| e.set_frame_fed().unwrap(),
    );
    page.respond("/contracts", 200, CONTRACTS);
    let frame = source.engine.snapshot_frame(source.now).to_vec();
    page.engine.apply_frame(page.now, &frame, true).unwrap();
    page.collect();
    assert_eq!(events(&page, "bitget.account.mode").len(), 1);
    let r = place(&mut page, true);
    page.reply(r.id, 200, r#"{"code":"00000","data":{"successList":[{"orderId":"42","clientOid":"close-telemetry"}],"failureList":[]}}"#);
    let mut order = source.engine.store().orders()[0].clone();
    order.id = "42".into();
    order.client_order_id = Some("close-telemetry".into());
    order.side = OrderSide::Sell;
    order.reduce_only = true;
    order.amount = 0.4;
    order.filled = 0.0;
    order.remaining = 0.4;
    order.last_update = page.now + 1.0;
    source.engine.store_mut().set_orders(vec![order]);
    page.now += 1.0;
    let frame = source.engine.snapshot_frame(page.now).to_vec();
    for _ in 0..2 {
        page.engine.apply_frame(page.now, &frame, true).unwrap();
        page.collect();
    }
    assert_eq!(events(&page, "bitget.account.mode").len(), 1);
    assert_eq!(events(&page, "bitget.close.diagnostic").len(), 1);
}

#[test]
fn a_native_stop_amend_refusal_retains_the_raw_close_diagnostic() {
    let mut h = Harness::classic();
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"3","symbol":"BTCUSDT","size":"0.5","triggerPrice":"80","executePrice":"0","holdSide":"long","planType":"loss_plan","status":"live"}]}"#);
    h.command(Command::AmendOrder {
        command: 72,
        id: "3".into(),
        price: Some(85.0),
        amount: Some(0.0),
    });
    let r = h.take("/modify-tpsl-order");
    h.reply(
        r.id,
        200,
        r#"{"code":"040757","msg":"Not enough position is available","data":null}"#,
    );
    let rows = events(&h, "bitget.close.diagnostic");
    assert_eq!(rows.len(), 1);
    assert_eq!(field(rows[0], "nativeCode"), &Value::from("040757"));
    assert_eq!(field(rows[0], "requestedSize"), &Value::from(0.5));
    assert_eq!(field(rows[0], "side"), &Value::from("long"));
}
