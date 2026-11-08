#[allow(dead_code)]
mod support;

use safe_cex_engine::{Action, Command, Event, Notice, Outcome};
use serde_json::{Value, json};
use support::*;

fn harness(minimum: f64, maximum: f64, step: f64) -> Harness {
    let mut instruments: Value = serde_json::from_str(INSTRUMENT).unwrap();
    let row = &mut instruments["data"][0];
    row["minSize"] = json!(minimum.to_string());
    row["maxLimitSize"] = json!(maximum.to_string());
    row["maxMarketSize"] = json!(maximum.to_string());
    row["lotSize"] = json!(step.to_string());
    let mut h = Harness::with_instruments(&instruments.to_string());
    h.open_private();
    h.seed();
    assert_eq!(h.pending.len(), 0);
    h
}

fn order(amount: f64) -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":60000})
}

fn place(h: &mut Harness, orders: Value) {
    h.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: serde_json::from_value(orders).unwrap(),
        }),
    );
    h.collect();
}

fn not_sent(h: &Harness) {
    assert_eq!(
        h.pending.len(),
        0,
        "no transport requests before whole-plan validation"
    );
    assert_eq!(h.failures.len(), 1);
    assert_eq!(h.failures[0].outcome, Some(Outcome::NotSent));
}

fn accept_batch(h: &mut Harness, first_id: u32) -> Vec<Value> {
    assert_eq!(h.pending.len(), 1);
    let request = h.take("batch-orders");
    let rows: Vec<Value> = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert!(rows.len() <= 20);
    let acknowledgements: Vec<_> = rows.iter().zip(first_id..).map(|(row, id)| {
        json!({"orderId":id.to_string(),"clientOrderId":row["clientOrderId"],"code":"0"})
    }).collect();
    h.respond(
        request,
        &json!({"code":"0","data":acknowledgements}).to_string(),
    );
    rows
}

fn next_batch(h: &mut Harness) {
    let (timer, at) = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 350.0 => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap();
    h.handle(at, Event::TimerFired { timer });
    h.collect();
}

#[test]
fn balanced_split_last_part_below_min_zero_writes() {
    let mut h = harness(4.0, 5.0, 1.0);
    place(&mut h, json!([order(11.0)]));
    not_sent(&h);
}

#[test]
fn positive_unsplit_below_native_minimum_zero_writes() {
    let mut h = harness(4.0, 5.0, 1.0);
    place(&mut h, json!([order(3.0)]));
    not_sent(&h);
}

#[test]
fn first_valid_last_subminimum_row_zero_writes() {
    let mut h = harness(4.0, 5.0, 1.0);
    let mut rows = vec![order(4.0); 20];
    rows.push(order(3.0));
    place(&mut h, json!(rows));
    not_sent(&h);
}

#[test]
fn first_valid_last_invalid_split_row_zero_writes() {
    let mut h = harness(4.0, 5.0, 1.0);
    place(&mut h, json!([order(4.0), order(11.0)]));
    not_sent(&h);
}

#[test]
fn sized_protective_below_minimum_zero_writes() {
    let mut h = harness(4.0, 5.0, 1.0);
    place(
        &mut h,
        json!([{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":3,"price":55000}]),
    );
    not_sent(&h);
}

#[test]
fn off_step_maximum_rebalances_without_oversize() {
    let mut h = harness(1.0, 4.6, 1.0);
    place(&mut h, json!([order(9.0)]));
    let rows = accept_batch(&mut h, 100);
    assert_eq!(
        rows.iter()
            .map(|row| row["size"].clone())
            .collect::<Vec<_>>(),
        json!(["3", "3", "3"]).as_array().unwrap().clone()
    );
    assert!(h.failures.is_empty());
    assert_eq!(h.pending.len(), 0);
}

#[test]
fn exact_fractional_maximum_control() {
    let mut h = harness(0.1, 0.3, 0.1);
    place(&mut h, json!([order(0.6)]));
    let rows = accept_batch(&mut h, 100);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row["size"] == "0.3"));
    assert!(h.failures.is_empty());
    assert_eq!(h.pending.len(), 0);
}

#[test]
fn rounded_part_above_max_zero_writes() {
    let mut h = harness(4.0, 4.6, 1.0);
    place(&mut h, json!([order(9.0)]));
    not_sent(&h);
}

#[test]
fn invalid_replacement_never_cancels() {
    for amount in [3.0, 11.0] {
        let mut h = harness(4.0, 5.0, 1.0);
        seed_entry(&mut h);
        amend(&mut h, amount);
        not_sent(&h);
        assert!(h.e.store().order("12").is_some());
    }
}

fn seed_entry(h: &mut Harness) {
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"4","filledSize":"0","price":"60000","state":"live"}]}"#);
    assert!(h.e.store().order("12").is_some());
}

fn amend(h: &mut Harness, amount: f64) {
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: None,
            amount: Some(amount),
        }),
    );
    h.collect();
}

#[test]
fn valid_replacement_cancels_once_and_places_each_lot_once() {
    let mut h = harness(4.0, 5.0, 1.0);
    seed_entry(&mut h);
    amend(&mut h, 12.0);
    assert_eq!(h.pending.len(), 1);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    next_batch(&mut h);
    assert_eq!(h.pending.len(), 1);
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#);
    let rows = accept_batch(&mut h, 100);
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row["size"] == "4"));
    assert!(h.failures.is_empty());
    assert_eq!(h.pending.len(), 0);
}

#[test]
fn native_order_type_maximum_controls_every_lot() {
    let mut instruments: Value = serde_json::from_str(INSTRUMENT).unwrap();
    let row = &mut instruments["data"][0];
    row["minSize"] = json!("2");
    row["lotSize"] = json!("1");
    row["maxLimitSize"] = json!("5");
    row["maxMarketSize"] = json!("4");
    for kind in ["limit", "market"] {
        let mut h = Harness::with_instruments(&instruments.to_string());
        h.seed();
        let mut intent = order(5.0);
        intent["type"] = json!(kind);
        place(&mut h, json!([intent]));
        let rows = accept_batch(&mut h, 100);
        assert_eq!(
            rows.iter()
                .map(|row| row["size"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["3", "2"]
        );
        assert!(h.failures.is_empty());
        assert_eq!(h.pending.len(), 0);
    }
    for execution in [json!(60000), Value::Null] {
        let mut h = Harness::with_instruments(&instruments.to_string());
        h.seed();
        let intent = json!({"symbol":"BTCUSDT","type":"conditional","side":"buy","amount":5,"price":execution,"triggerPrice":61000});
        place(&mut h, json!([intent]));
        if execution.is_null() {
            not_sent(&h);
        } else {
            assert_eq!(h.pending.len(), 1);
            let request = h.take("order-algo");
            let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(payload["size"], "5");
            assert_eq!(payload["orderPrice"], "60000");
            h.respond(
                request,
                r#"{"code":"0","data":[{"algoId":"100","code":"0"}]}"#,
            );
            assert!(h.failures.is_empty());
            assert_eq!(h.pending.len(), 0);
        }
    }
}

#[test]
fn positional_zero_protection_remains_explicit() {
    let mut h = harness(4.0, 5.0, 1.0);
    place(
        &mut h,
        json!([{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":55000}]),
    );
    assert_eq!(h.pending.len(), 1);
    let request = h.take("order-tpsl");
    let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload["size"], "-1");
    h.respond(
        request,
        r#"{"code":"0","data":[{"tpslId":"100","code":"0"}]}"#,
    );
    assert!(h.failures.is_empty());
    assert_eq!(h.pending.len(), 0);
}

#[test]
fn final_trigger_rounding_to_zero_rejects_whole_group() {
    let mut h = harness(4.0, 5.0, 1.0);
    let mut invalid = order(4.0);
    invalid["stopLoss"] = json!(0.01);
    place(&mut h, json!([order(4.0), invalid]));
    not_sent(&h);
}

#[test]
fn exact_minimum_balanced_control() {
    let mut h = harness(4.0, 5.0, 1.0);
    let mut intent = order(12.0);
    intent["takeProfit"] = json!(65000.04);
    intent["stopLoss"] = json!(55000.04);
    intent["reduceOnly"] = json!(true);
    place(&mut h, json!([intent]));
    let rows = accept_batch(&mut h, 100);
    assert_eq!(rows.len(), 3);
    let mut ids = std::collections::HashSet::new();
    for row in rows {
        assert_eq!(row["size"], "4");
        assert_eq!(row["reduceOnly"], true);
        assert_eq!(row["attachAlgoOrders"][0]["tpTriggerPrice"], "65000");
        assert_eq!(row["attachAlgoOrders"][0]["slTriggerPrice"], "55000");
        assert!(ids.insert(row["clientOrderId"].clone().to_string()));
    }
    assert!(h.failures.is_empty());
    assert_eq!(h.pending.len(), 0);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
    );
}

#[test]
fn valid_grouped_chunks_preserve_each_caller_id_exactly_once() {
    let mut h = harness(4.0, 5.0, 1.0);
    let intents: Vec<_> = (0..21)
        .map(|id| {
            let mut row = order(4.0);
            row["clientOrderId"] = json!(format!("caller{id}"));
            row
        })
        .collect();
    place(&mut h, json!(intents));
    let mut rows = accept_batch(&mut h, 100);
    assert_eq!(rows.len(), 20);
    next_batch(&mut h);
    let tail = accept_batch(&mut h, 120);
    assert_eq!(tail.len(), 1);
    rows.extend(tail);
    let ids: Vec<_> = rows
        .iter()
        .map(|row| row["clientOrderId"].clone())
        .collect();
    assert_eq!(
        ids,
        (0..21)
            .map(|id| json!(format!("caller{id}")))
            .collect::<Vec<_>>()
    );
    assert!(h.failures.is_empty());
    assert_eq!(h.pending.len(), 0);
}
