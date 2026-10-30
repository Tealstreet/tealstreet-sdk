//! REST-backed UTA trailing reconciliation. The primary strategy WS document
//! specifies TPSL/trigger rows, so trailing discovery must work without a push.
//! https://www.bitget.com/docs/catalog/trading/strategy-trading
mod support;

use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType, PlaceOrderOpts, StopTriggerType};
use safe_cex_engine::{Command, Event, RequestId, wire};
use serde_json::{Value, json};
use support::Harness;

fn trailing(id: &str, price: &str, variance: &str) -> Value {
    json!({
        "category":"USDT-FUTURES", "symbol":"BTCUSDT", "type":"trailing_stop",
        "orderId":id, "clientOid":"documented-rest", "qty":"0.01", "side":"sell",
        "posSide":"long", "reduceOnly":"yes", "status":"pending",
        "trailingStopParams":{
            "activationPrice":price, "activationType":"market", "trailType":"ratio",
            "trailVariance":variance, "preOrderType":"market"
        }
    })
}

fn respond_orders(h: &mut Harness, tpsl: &[Value], trailing: &[Value]) {
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond(
        "type=tpsl",
        200,
        &json!({"code":"00000","data":tpsl}).to_string(),
    );
    let request = h.take("type=trailing_stop");
    assert!(request.url.contains("category=USDT-FUTURES"));
    h.reply(
        request.id,
        200,
        &json!({"code":"00000","data":trailing}).to_string(),
    );
}

fn background(h: &mut Harness) {
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/current-position"))
    {
        h.respond(
            "/current-position",
            200,
            r#"{"code":"00000","data":{"list":[]}}"#,
        );
    }
    while h.requests.iter().any(|r| r.url.contains("/account/assets")) {
        h.respond("/account/assets", 200, r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"1000","unrealisedPnl":"0"}}"#);
    }
}

fn uta(rows: &[Value]) -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
    );
    background(&mut h);
    respond_orders(&mut h, &[], rows);
    h.login();
    background(&mut h);
    respond_orders(&mut h, &[], rows);
    h
}

fn metadata(h: &mut Harness) {
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/account/settings"))
    {
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
        );
    }
}

fn place_trailing(h: &mut Harness, command: u32, id: &str) {
    let opts: PlaceOrderOpts = serde_json::from_value(json!({
        "symbol":"BTCUSDT", "type":"trailing_stop_market", "side":"sell",
        "amount":0.01, "price":95
    }))
    .unwrap();
    h.command(Command::PlaceOrders {
        command,
        orders: vec![opts],
    });
    metadata(h);
    let request = h.take("/place-strategy-order");
    h.reply(
        request.id,
        200,
        &json!({"code":"00000","data":{"orderId":id}}).to_string(),
    );
}

fn poll(h: &mut Harness) {
    h.timer_after(2000.0);
    background(h);
}

fn poll_at(h: &mut Harness, at: f64) {
    let (timer, _) = h
        .timers
        .iter()
        .copied()
        .find(|(_, time)| *time == at)
        .unwrap();
    h.now = at;
    h.event(Event::TimerFired { timer });
    background(h);
}

fn doc_tpsl(status: &str) -> Value {
    // Primary Strategy Order Channel Push Data example, with only status varied.
    // It declares triggerType but carries both TP and SL prices in one row.
    json!({
        "category":"usdt-futures", "symbol":"BTCUSDT", "orderId":"111111111111111111",
        "clientOid":"myorder001", "qty":"0.01", "side":"buy", "posSide":"long",
        "reduceOnly":"no", "type":"tpsl", "status":status, "triggerType":"takeProfit",
        "tpTriggerBy":"market", "slTriggerBy":"market", "takeProfit":"110000", "stopLoss":"90000",
        "tpOrderType":"market", "slOrderType":"market", "tpLimitPrice":"", "slLimitPrice":"",
        "triggerBy":"market", "triggerPrice":"100000", "triggerOrderType":"limit",
        "triggerOrderPrice":"100500", "createdTime":"1730186725663", "updatedTime":"1730186725691"
    })
}

fn strategy_ws(h: &mut Harness, row: Value) {
    h.private_message(&json!({
        "action":"snapshot", "arg":{"instType":"UTA","topic":"strategy-order","symbol":"default"},
        "data":[row], "ts":1730186725700_u64
    }).to_string());
}

#[test]
fn rest_trailing_displays_and_keeps_native_callback_through_amount_only_amend() {
    let mut h = uta(&[trailing("rest-trailing", "95", "3")]);
    let order = h.engine.store().order("rest-trailing").unwrap();
    assert_eq!(
        (order.kind, order.price, order.amount, order.remaining),
        (OrderType::TrailingStopLoss, 95.0, 0.01, 0.01)
    );
    assert_eq!(order.trigger_type, Some(StopTriggerType::LastPrice));
    h.command(Command::AmendOrder {
        command: 1,
        id: "rest-trailing".into(),
        price: None,
        amount: Some(0.02),
    });
    metadata(&mut h);
    background(&mut h);
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/cancel-strategy-order")),
        "amend results: {:?}; logs: {:?}",
        h.done,
        h.logs
    );
    let cancel = h.take("/cancel-strategy-order");
    h.reply(
        cancel.id,
        200,
        r#"{"code":"00000","data":{"orderId":"rest-trailing"}}"#,
    );
    let replacement = h.take("/place-strategy-order");
    let payload: Value = serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload["qty"], "0.02");
    assert_eq!(payload["trailingStopParams"][0]["trailType"], "ratio");
    assert_eq!(payload["trailingStopParams"][0]["trailVariance"], "3");
    assert_eq!(payload["trailingStopParams"][0]["activationPrice"], "95");
}

#[test]
fn eight_second_uta_poll_reads_trailing_and_removes_absent_orders_preserving_tpsl() {
    let mut h = uta(&[trailing("externally-cancelled", "95", "3")]);
    let started = h.now;
    strategy_ws(&mut h, doc_tpsl("pending"));
    // Service the stream-triggered read before measuring periodic polling.
    h.timer_after(500.0);
    respond_orders(&mut h, &[], &[trailing("externally-cancelled", "95", "3")]);
    // Return to the poll's original timeline without firing unrelated timers.
    for tick in 1..=4 {
        poll_at(&mut h, started + 2000.0 * f64::from(tick));
    }
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/unfilled-orders")),
        "UTA must request a complete strategy snapshot every fourth poll"
    );
    respond_orders(&mut h, &[], &[]);
    assert!(h.engine.store().order("externally-cancelled").is_none());
    assert!(
        h.engine
            .store()
            .orders()
            .iter()
            .any(|o| o.id.starts_with("111111111111111111") && o.kind == OrderType::StopLoss)
    );
}

#[test]
fn successful_write_discovers_trailing_without_any_strategy_push_and_coalesces_refreshes() {
    let mut h = uta(&[]);
    place_trailing(&mut h, 10, "first-trailing");
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/unfilled-orders")),
        "successful write must request a complete snapshot"
    );
    // Another successful write while that read is pending requires one newer read.
    place_trailing(&mut h, 11, "second-trailing");
    respond_orders(&mut h, &[], &[]);
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/unfilled-orders")),
        "a write after snapshot start must schedule a follow-up snapshot"
    );
    respond_orders(
        &mut h,
        &[],
        &[
            trailing("first-trailing", "95", "5"),
            trailing("second-trailing", "95", "5"),
        ],
    );
    assert_eq!(
        h.engine
            .store()
            .orders()
            .iter()
            .filter(|o| o.kind == OrderType::TrailingStopLoss)
            .count(),
        2
    );
    assert!(h.done.iter().filter(|(_, result)| result.is_ok()).count() >= 2);
}

#[test]
fn failed_trailing_read_keeps_current_protection() {
    let mut h = uta(&[trailing("keep-on-failure", "95", "3")]);
    for _ in 0..4 {
        poll(&mut h);
    }
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond("type=tpsl", 200, r#"{"code":"00000","data":[]}"#);
    h.respond(
        "type=trailing_stop",
        500,
        r#"{"code":"500","msg":"temporarily unavailable","data":[]}"#,
    );
    assert!(h.engine.store().order("keep-on-failure").is_some());
}

#[test]
fn newer_common_strategy_frame_is_protected_from_an_older_rest_result() {
    let mut h = uta(&[trailing("newer-common-frame", "95", "3")]);
    for _ in 0..4 {
        poll(&mut h);
    }
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond("type=tpsl", 200, r#"{"code":"00000","data":[]}"#);
    let old = h.take("type=trailing_stop");
    h.now += 1.0;
    // Defensive common-schema compatibility only: the official strategy WS
    // page does not document a full trailing push. REST backs discovery above.
    strategy_ws(&mut h, trailing("newer-common-frame", "96", "7"));
    h.reply(
        old.id,
        200,
        &json!({"code":"00000","data":[trailing("newer-common-frame", "95", "3")]}).to_string(),
    );
    assert_eq!(
        h.engine.store().order("newer-common-frame").unwrap().price,
        96.0
    );
    h.timer_after(500.0);
    respond_orders(&mut h, &[], &[]);
    assert_eq!(
        h.engine.store().order("newer-common-frame").unwrap().price,
        96.0,
        "fresh WS grace must also cover omission"
    );
    h.command(Command::AmendOrder {
        command: 17,
        id: "newer-common-frame".into(),
        price: None,
        amount: Some(0.02),
    });
    metadata(&mut h);
    background(&mut h);
    let cancel = h.take("/cancel-strategy-order");
    h.reply(
        cancel.id,
        200,
        r#"{"code":"00000","data":{"orderId":"newer-common-frame"}}"#,
    );
    let replace = h.take("/place-strategy-order");
    let payload: Value = serde_json::from_str(replace.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload["trailingStopParams"][0]["activationPrice"], "96");
    assert_eq!(
        payload["trailingStopParams"][0]["trailVariance"], "7",
        "stale REST must not overwrite the newer native callback"
    );
}

#[test]
fn fresh_common_frame_absence_grace_expires_before_next_eight_second_poll() {
    let mut h = uta(&[trailing("fresh-then-gone", "95", "3")]);
    let started = h.now;
    // Defensive common-schema compatibility, not a documented trailing push.
    strategy_ws(&mut h, trailing("fresh-then-gone", "96", "7"));
    h.timer_after(500.0);
    respond_orders(&mut h, &[], &[]);
    assert!(h.engine.store().order("fresh-then-gone").is_some());
    for tick in 1..=4 {
        poll_at(&mut h, started + 2000.0 * f64::from(tick));
    }
    respond_orders(&mut h, &[], &[]);
    assert!(h.engine.store().order("fresh-then-gone").is_none());
}

#[test]
fn documented_rest_spread_distance_maps_to_distance_when_replacing() {
    // Current embedded OpenAPI uses spread on reads and distance on placement.
    let mut row = trailing("rest-distance", "95", "5");
    row["trailingStopParams"]["trailType"] = json!("spread");
    let mut h = uta(&[row]);
    h.command(Command::AmendOrder {
        command: 21,
        id: "rest-distance".into(),
        price: None,
        amount: Some(0.02),
    });
    metadata(&mut h);
    background(&mut h);
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/cancel-strategy-order")),
        "amend results: {:?}",
        h.done
    );
    let cancel = h.take("/cancel-strategy-order");
    h.reply(cancel.id, 200, r#"{"code":"00000","data":null}"#);
    let replacement = h.take("/place-strategy-order");
    let payload: Value = serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload["trailingStopParams"][0]["trailType"], "distance");
    assert_eq!(payload["trailingStopParams"][0]["trailVariance"], "5");
    assert_eq!(payload["trailingStopParams"][0]["activationPrice"], "95");
}

#[test]
fn exact_documented_tpsl_pending_and_terminal_statuses_do_not_emit_executions() {
    for terminal in ["success", "cancelled", "failed"] {
        let mut h = uta(&[]);
        h.engine.take_frame(h.now);
        strategy_ws(&mut h, doc_tpsl("pending"));
        assert_eq!(h.engine.store().orders().len(), 2);
        assert!(
            h.engine
                .store()
                .orders()
                .iter()
                .all(|o| o.status == OrderStatus::Open)
        );
        assert!(
            h.engine
                .store()
                .orders()
                .iter()
                .all(|o| o.side == OrderSide::Sell && o.reduce_only)
        );
        assert!(
            h.engine
                .store()
                .orders()
                .iter()
                .any(|o| o.kind == OrderType::TakeProfit && o.price == 110000.0)
        );
        assert!(
            h.engine
                .store()
                .orders()
                .iter()
                .any(|o| o.kind == OrderType::StopLoss && o.price == 90000.0)
        );
        strategy_ws(&mut h, doc_tpsl(terminal));
        assert!(
            h.engine
                .store()
                .orders()
                .iter()
                .all(|o| o.status != OrderStatus::Open)
        );
        let frame = h.engine.take_frame(h.now).unwrap().to_vec();
        assert!(
            !wire::sections(&frame).any(|(kind, _)| kind == wire::section::FILLS),
            "strategy status {terminal} is not an execution"
        );
    }
}

#[test]
fn exact_documented_tpsl_long_position_stays_long_during_hedged_amend() {
    let mut h = uta(&[]);
    h.engine.store_mut().set_hedged(true);
    strategy_ws(&mut h, doc_tpsl("pending"));
    let id = "111111111111111111__stop_loss";
    assert_eq!(h.engine.store().order(id).unwrap().side, OrderSide::Sell);
    h.command(Command::AmendOrder {
        command: 31,
        id: id.into(),
        price: Some(90001.0),
        amount: None,
    });
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/account/settings"))
    {
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
        );
    }
    background(&mut h);
    let cancel = h.take("/cancel-strategy-order");
    let payload: Value = serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload["orderId"], "111111111111111111");
    h.reply(cancel.id, 200, r#"{"code":"00000","data":null}"#);
    let replacement = h.take("/place-strategy-order");
    let payload: Value = serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload["side"], "sell");
    assert_eq!(payload["posSide"], "long");
    assert_eq!(payload["stopLoss"], "90001");
    assert_eq!(payload["takeProfit"], "110000");
}

fn snapshot_delayed_past_common_frame_grace(h: &mut Harness, id: &str) -> RequestId {
    let started = h.now;
    for _ in 0..4 {
        poll(h);
    }
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond("type=tpsl", 200, r#"{"code":"00000","data":[]}"#);
    let old = h.take("type=trailing_stop");
    h.now += 1.0;
    // Adapted common-schema compatibility only. The verified lifecycle does
    // not require the venue to deliver a full trailing strategy push.
    strategy_ws(h, trailing(id, "96", "7"));
    for tick in 5..=8 {
        poll_at(h, started + 2000.0 * f64::from(tick));
    }
    // These polls prune the five-second grace while the old HTTP request is
    // still pending. The captured stream revision must remain authoritative.
    assert_eq!(h.engine.store().order(id).unwrap().price, 96.0);
    old.id
}

#[test]
fn delayed_absence_cannot_drop_newer_common_frame_after_grace_is_pruned() {
    let id = "delayed-absence";
    let mut h = uta(&[trailing(id, "95", "3")]);
    let old = snapshot_delayed_past_common_frame_grace(&mut h, id);
    h.reply(old, 200, r#"{"code":"00000","data":[]}"#);
    assert_eq!(h.engine.store().order(id).unwrap().price, 96.0);
    // A subsequent snapshot begun after the update is authoritative again: the
    // next orders read (the 60 s reconcile while the stream is live).
    for _ in 0..31 {
        poll(&mut h);
        if h.requests
            .iter()
            .any(|r| r.url.contains("/unfilled-orders"))
        {
            break;
        }
    }
    respond_orders(&mut h, &[], &[]);
    assert!(h.engine.store().order(id).is_none());
}

#[test]
fn delayed_row_cannot_replace_newer_callback_after_grace_is_pruned() {
    let id = "delayed-callback";
    let mut h = uta(&[trailing(id, "95", "3")]);
    let old = snapshot_delayed_past_common_frame_grace(&mut h, id);
    h.reply(
        old,
        200,
        &json!({"code":"00000","data":[trailing(id,"95","3")]}).to_string(),
    );
    assert_eq!(h.engine.store().order(id).unwrap().price, 96.0);
    h.command(Command::AmendOrder {
        command: 51,
        id: id.into(),
        price: None,
        amount: Some(0.02),
    });
    metadata(&mut h);
    background(&mut h);
    let cancel = h.take("/cancel-strategy-order");
    h.reply(cancel.id, 200, r#"{"code":"00000","data":null}"#);
    let replacement = h.take("/place-strategy-order");
    let payload: Value = serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload["trailingStopParams"][0]["activationPrice"], "96");
    assert_eq!(payload["trailingStopParams"][0]["trailVariance"], "7");
}

#[test]
fn skipped_uta_trailing_row_keeps_its_stored_copy_though_absent_trailing_is_removed() {
    // UTA polls never keep an absent trailing stop, so only the skipped row's
    // identity can keep it.
    let mut h = uta(&[
        trailing("skipped", "95", "3"),
        trailing("absent", "96", "3"),
    ]);
    let stored = h.engine.store().order("skipped").unwrap().clone();
    assert_eq!(stored.kind, OrderType::TrailingStopLoss);
    let started = h.now;
    for tick in 1..=4 {
        poll_at(&mut h, started + 2000.0 * f64::from(tick));
    }
    let mut unknown = trailing("skipped", "97", "3");
    unknown["status"] = json!("secret_new_status");
    respond_orders(&mut h, &[], &[unknown, trailing("fresh", "98", "3")]);
    assert_eq!(h.engine.store().order("skipped"), Some(&stored));
    assert!(h.engine.store().order("absent").is_none());
    assert!(h.engine.store().order("fresh").is_some());
}
