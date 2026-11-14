mod support;

use safe_cex_engine::{Command, Method, Outcome, RequestId, VenueAction};
use serde_json::{Value, json};
use support::*;

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
    h
}

fn amend(h: &mut Harness, id: &str, price: Option<f64>, amount: Option<f64>) {
    h.command(Command::AmendOrder {
        command: 701,
        id: id.into(),
        price,
        amount,
    });
}

fn body(h: &mut Harness, path: &str) -> (RequestId, Value) {
    let r = h.take(path);
    (
        r.id,
        serde_json::from_str(r.body.as_deref().unwrap()).unwrap(),
    )
}

fn strategy(h: &mut Harness, quantity: &str, full: bool) {
    h.private_message(&json!({"arg":{"topic":"strategy-order"},"data":[{
        "category":"USDT-FUTURES","orderId":"stop","symbol":"BTCUSDT","qty":quantity,
        "tpslMode":if full {"full"} else {"partial"},"stopLoss":"80","side":"sell","posSide":"long","status":"pending","slTriggerBy":"market","slOrderType":"market"
    }]}).to_string());
}

fn assert_unknown_without_more_writes(h: &Harness) {
    let error = h.failures.last().expect("typed terminal failure");
    assert_eq!(error.outcome, Some(Outcome::Unknown), "{error:?}");
    assert_eq!(error.protection, None, "{error:?}");
    assert!(
        error
            .legacy_text()
            .to_ascii_lowercase()
            .contains("check open orders"),
        "{error:?}"
    );
    assert!(
        !h.requests.iter().any(|r| r.method == Method::Post),
        "{:?}",
        h.requests
    );
}

#[test]
fn uncertain_native_amend_never_falls_through_to_cancel_and_replace() {
    for (status, code) in [(429, 429), (200, 429), (200, 25000), (200, 25001)] {
        let mut h = Harness::classic();
        amend(&mut h, "1", Some(95.0), None);
        let r = h.take("/modify-order");
        h.reply(
            r.id,
            status,
            &json!({"code":code.to_string(),"msg":"busy","data":null}).to_string(),
        );
        assert_unknown_without_more_writes(&h);
        assert_eq!(
            h.engine.store().order("1").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
    }
}

#[test]
fn uncertain_strategy_replacement_never_puts_back_a_second_stop() {
    for (status, code) in [(429, 429), (200, 429), (200, 25000), (200, 25001)] {
        let mut h = uta();
        strategy(&mut h, "0", true);
        amend(&mut h, "stop", Some(85.0), None);
        h.respond(
            "/cancel-strategy-order",
            200,
            r#"{"code":"00000","data":null}"#,
        );
        let r = h.take("/place-strategy-order");
        h.reply(
            r.id,
            status,
            &json!({"code":code.to_string(),"msg":"busy","data":null}).to_string(),
        );
        assert_unknown_without_more_writes(&h);
    }
}

#[test]
fn uncertain_put_back_does_not_claim_unprotected_or_try_again() {
    let mut h = uta();
    strategy(&mut h, "0", true);
    amend(&mut h, "stop", Some(85.0), None);
    h.respond(
        "/cancel-strategy-order",
        200,
        r#"{"code":"00000","data":null}"#,
    );
    h.respond(
        "/place-strategy-order",
        200,
        r#"{"code":"40017","msg":"invalid trigger","data":null}"#,
    );
    h.respond(
        "/place-strategy-order",
        429,
        r#"{"code":"429","msg":"busy","data":null}"#,
    );
    assert_unknown_without_more_writes(&h);
    assert!(
        !h.failures
            .last()
            .unwrap()
            .legacy_text()
            .contains("position has no stop")
    );
}

fn classic_stop(h: &mut Harness, plan: &str) {
    h.private_message(
        &json!({"arg":{"channel":"orders-algo"},"data":[{
            "orderId":"stop","symbol":"BTCUSDT","size":"0.5","triggerPrice":"80",
            "executePrice":"79","triggerType":"fill_price","holdSide":"long","planType":plan,
            "rangeRate":"0.7","status":"live"
        }]})
        .to_string(),
    );
}

#[test]
fn classic_amount_only_modify_keeps_required_trigger_and_limit_execution() {
    let mut h = Harness::classic();
    classic_stop(&mut h, "loss_plan");
    amend(&mut h, "stop", None, Some(0.6));
    let (_, p) = body(&mut h, "/modify-tpsl-order");
    assert_eq!(p["triggerPrice"], "80");
    assert_eq!(p["executePrice"], "79");
    assert_eq!(p["triggerType"], "fill_price");
    assert_eq!(p["size"], "0.6");
    assert!(!h.requests.iter().any(|r| r.url.contains("cancel")));
}

#[test]
fn classic_price_modify_keeps_limit_execution_through_a_source_frame() {
    let mut source = Harness::classic();
    classic_stop(&mut source, "loss_plan");
    let frame = source.engine.snapshot_frame(source.now).to_vec();
    let mut h = Harness::started_configure(
        FixtureOptions {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..FixtureOptions::default()
        },
        false,
        |engine| engine.set_frame_fed().unwrap(),
    );
    h.respond("/contracts", 200, CONTRACTS);
    h.engine.apply_frame(h.now, &frame, true).unwrap();
    h.collect();
    amend(&mut h, "stop", Some(85.0), None);
    let (_, p) = body(&mut h, "/modify-tpsl-order");
    assert_eq!(p["triggerPrice"], "85");
    assert_eq!(p["executePrice"], "79");
}

#[test]
fn classic_rest_plan_keeps_its_execution_price_on_amount_edit() {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.respond("/orders-pending", 200, OPEN_ORDER);
    h.respond("planType=profit_loss", 200, r#"{"code":"00000","data":{"entrustedList":[{"orderId":"stop","symbol":"BTCUSDT","size":"0.5","triggerPrice":"80","executePrice":"79","triggerType":"fill_price","holdSide":"long","planType":"loss_plan","status":"live"}],"endId":""}}"#);
    h.respond(
        "planType=track_plan",
        200,
        r#"{"code":"00000","data":{"entrustedList":[],"endId":""}}"#,
    );
    h.login();
    amend(&mut h, "stop", None, Some(0.6));
    let (_, p) = body(&mut h, "/modify-tpsl-order");
    assert_eq!(p["executePrice"], "79");
    assert_eq!(p["triggerPrice"], "80");
}

#[test]
fn strategy_size_edit_puts_back_original_venue_quantity_not_position_capacity() {
    let mut h = uta();
    strategy(&mut h, "0.5", false);
    let mut positions = h.engine.store().positions().to_vec();
    for p in &mut positions {
        p.contracts = 0.25;
    }
    h.engine.store_mut().set_positions(positions);
    amend(&mut h, "stop", Some(85.0), Some(0.0));
    h.respond(
        "/cancel-strategy-order",
        200,
        r#"{"code":"00000","data":null}"#,
    );
    let (replace, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["qty"], "0.25");
    h.reply(
        replace,
        200,
        r#"{"code":"40017","msg":"invalid qty","data":null}"#,
    );
    let (_, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["qty"], "0.5");
    assert_eq!(p["stopLoss"], "80");
}

#[test]
fn full_position_edit_and_put_back_keep_the_full_mode_sentinel() {
    let mut h = uta();
    strategy(&mut h, "0", true);
    amend(&mut h, "stop", Some(85.0), None);
    h.respond(
        "/cancel-strategy-order",
        200,
        r#"{"code":"00000","data":null}"#,
    );
    let (replace, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["tpslMode"], "full");
    assert!(p.get("qty").is_none());
    h.reply(
        replace,
        200,
        r#"{"code":"40017","msg":"invalid trigger","data":null}"#,
    );
    let (_, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["tpslMode"], "full");
    assert!(p.get("qty").is_none());
}

#[test]
fn moving_plan_edits_use_the_tpsl_family_and_preserve_range_rate() {
    let mut h = Harness::classic();
    classic_stop(&mut h, "moving_plan");
    amend(&mut h, "stop", Some(85.0), None);
    let (_, p) = body(&mut h, "/modify-tpsl-order");
    assert_eq!(p["rangeRate"], "0.7");
    assert_eq!(p["triggerPrice"], "85");
    assert!(p.get("executePrice").is_none());
    assert!(p.get("newCallbackRatio").is_none());
}

#[test]
fn account_nuke_includes_moving_plan_without_dropping_other_plan_types() {
    let mut h = Harness::classic();
    h.command(Command::Venue {
        command: 702,
        action: VenueAction::CancelAccountOrders,
    });
    h.respond("/cancel-all-orders", 200, r#"{"code":"00000","data":{}}"#);
    let mut kinds = Vec::new();
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/cancel-plan-order"))
    {
        let (id, p) = body(&mut h, "/cancel-plan-order");
        kinds.push(p["planType"].as_str().unwrap().to_owned());
        h.reply(
            id,
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[]}}"#,
        );
    }
    kinds.sort();
    assert_eq!(
        kinds,
        [
            "loss_plan",
            "moving_plan",
            "normal_plan",
            "pos_loss",
            "pos_profit",
            "profit_plan",
            "track_plan"
        ]
    );
}

#[test]
fn mixed_build_v1_frame_keeps_account_writes_but_refuses_unknown_execution() {
    use safe_cex_engine::wire::{FrameWriter, section};
    let mut source = Harness::classic();
    classic_stop(&mut source, "loss_plan");
    let mut frame = source.engine.snapshot_frame(source.now).to_vec();
    let mut legacy = vec![1, 1];
    legacy.extend_from_slice(&1u32.to_le_bytes());
    for text in ["stop", "loss_plan"] {
        legacy.extend_from_slice(&(text.len() as u16).to_le_bytes());
        legacy.extend_from_slice(text.as_bytes());
    }
    legacy.extend_from_slice(&0u32.to_le_bytes());
    let mut writer = FrameWriter::new(Vec::new(), source.now);
    writer.venue_state(&legacy);
    let replacement = writer.finish();
    let mut at = 16;
    loop {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let length = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == section::VENUE_STATE {
            frame.splice(at..at + 8 + length, replacement[16..].iter().copied());
            break;
        }
        at += 8 + length;
    }
    let mut h = Harness::started_configure(
        FixtureOptions {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..FixtureOptions::default()
        },
        false,
        |engine| engine.set_frame_fed().unwrap(),
    );
    h.respond("/contracts", 200, CONTRACTS);
    h.engine.apply_frame(h.now, &frame, true).unwrap();
    h.collect();
    assert!(h.engine.store().loaded().orders);
    amend(&mut h, "stop", Some(85.0), None);
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    let error = h
        .failures
        .last()
        .expect("unknown execution refused before I/O");
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(
        error.message.contains("execution price is unavailable"),
        "{error:?}"
    );
    h.command(Command::CancelOrders {
        command: 703,
        ids: vec!["1".into()],
    });
    let (id, p) = body(&mut h, "/batch-cancel-orders");
    assert_eq!(p["orderIdList"], json!([{"orderId":"1"}]));
    h.reply(
        id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"1"}],"failureList":[]}}"#,
    );
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 703 && result.is_ok())
    );
}
