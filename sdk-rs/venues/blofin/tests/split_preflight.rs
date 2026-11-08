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
    h.requests.clear();
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
        h.requests.len(),
        0,
        "recording scripted transport saw a request"
    );
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
    assert_eq!(h.requests.len(), 1);
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
    assert_eq!(
        h.requests.len(),
        3,
        "one cancel, one verify read, one placement"
    );
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
        h.requests.clear();
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
        h.requests.clear();
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
    assert_eq!(h.requests.len(), 1);
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
    assert_eq!(h.requests.len(), 2);
}

fn three_write_plan() -> Harness {
    let mut instruments: Value = serde_json::from_str(INSTRUMENT).unwrap();
    let mut eth = instruments["data"][0].clone();
    eth["instId"] = json!("ETH-USDT");
    eth["baseCurrency"] = json!("ETH");
    instruments["data"].as_array_mut().unwrap().push(eth);
    let mut h = Harness::with_instruments(&instruments.to_string());
    h.seed();
    h.requests.clear();
    let mut intents = vec![order(4.0); 3];
    for (index, intent) in intents.iter_mut().enumerate() {
        intent["clientOrderId"] = json!(format!("element{index}"));
    }
    intents[1]["symbol"] = json!("ETHUSDT");
    place(&mut h, json!(intents));
    accept_batch(&mut h, 100);
    next_batch(&mut h);
    h
}

fn assert_outcomes(h: &Harness, ambiguous: bool) {
    use safe_cex_engine::write_plan::ElementResult;
    let error = h.failures.last().expect("terminal failure");
    assert_eq!(
        error.outcome,
        if ambiguous {
            Some(Outcome::Unknown)
        } else {
            None
        }
    );
    assert_eq!(
        error
            .partial
            .as_ref()
            .expect("accepted prefix IDs")
            .order_ids,
        ["100"]
    );
    let outcomes = error.elements.as_ref().expect("complete element outcomes");
    assert_eq!(outcomes.len(), 3);
    for (index, element) in outcomes.iter().enumerate() {
        assert_eq!(element.element.index, index);
        assert_eq!(
            element.element.client_order_id,
            Some(format!("element{index}"))
        );
    }
    assert_eq!(
        outcomes[0].result,
        ElementResult::Placed {
            order_ids: vec!["100".into()]
        }
    );
    if ambiguous {
        assert!(
            matches!(&outcomes[1].result, ElementResult::Unknown { error } if error.outcome == Some(Outcome::Unknown))
        );
    } else {
        assert!(
            matches!(&outcomes[1].result, ElementResult::NotSent { error: Some(error) } if error.outcome == Some(Outcome::Rejected) && error.venue_code.as_deref() == Some("103003"))
        );
    }
    assert_eq!(outcomes[2].result, ElementResult::NotSent { error: None });
    assert_eq!(
        h.requests
            .iter()
            .filter(|(method, _)| *method == safe_cex_engine::Method::Post)
            .count(),
        2,
        "no resend, suffix, cancel or rollback"
    );
    assert!(
        h.pending
            .iter()
            .all(|request| request.method == safe_cex_engine::Method::Get)
    );
}

#[test]
fn runtime_rejection_reports_placed_failed_and_unsent_elements() {
    let mut h = three_write_plan();
    h.reply(
        "batch-orders",
        r#"{"code":"1","data":[{"orderId":"0","code":"103003","msg":"Insufficient balance"}]}"#,
    );
    assert_outcomes(&h, false);
}

#[test]
fn ambiguous_http_reply_keeps_pending_element_unknown_and_suffix_unsent() {
    let mut h = three_write_plan();
    let request = h.take("batch-orders");
    h.respond_status(request, 503, "{}");
    assert_outcomes(&h, true);
}

#[test]
fn lost_reply_keeps_pending_element_unknown_and_suffix_unsent() {
    let mut h = three_write_plan();
    let request = h.take("batch-orders");
    h.handle(
        h.now,
        Event::HttpFailed {
            request: request.id,
            error: "reply lost",
        },
    );
    h.collect();
    assert_outcomes(&h, true);
}

#[test]
fn mixed_batch_preserves_each_classified_row_and_stops_the_next_write() {
    use safe_cex_engine::write_plan::ElementResult;
    for code in ["103003", "777777"] {
        let mut h = harness(4.0, 5.0, 1.0);
        place(
            &mut h,
            json!([order(4.0), order(4.0), order(4.0), {
                "symbol":"BTCUSDT", "type":"stop_market", "side":"sell", "amount":4, "price":55000
            }]),
        );
        h.reply(
            "batch-orders",
            &json!({"code":"2", "data":[
                {"orderId":"100", "code":"0"},
                {"orderId":"0", "code":code, "msg":"row refused"},
                {"orderId":"102", "code":"0"}
            ]})
            .to_string(),
        );
        let outcomes = h.failures[0]
            .elements
            .as_ref()
            .expect("complete mixed batch outcomes");
        assert_eq!(outcomes.len(), 4);
        assert_eq!(
            outcomes[0].result,
            ElementResult::Placed {
                order_ids: vec!["100".into()]
            }
        );
        assert_eq!(
            outcomes[2].result,
            ElementResult::Placed {
                order_ids: vec!["102".into()]
            }
        );
        if code == "103003" {
            assert!(
                matches!(&outcomes[1].result, ElementResult::NotSent { error: Some(error) } if error.outcome == Some(Outcome::Rejected))
            );
        } else {
            assert!(
                matches!(&outcomes[1].result, ElementResult::Unknown { error } if error.outcome == Some(Outcome::Unknown))
            );
        }
        assert_eq!(outcomes[3].result, ElementResult::NotSent { error: None });
        assert_eq!(
            h.requests
                .iter()
                .filter(|(method, _)| *method == safe_cex_engine::Method::Post)
                .count(),
            1
        );
        assert!(
            h.pending
                .iter()
                .all(|request| request.method == safe_cex_engine::Method::Get)
        );
    }
}

#[test]
fn local_signing_veto_marks_pending_elements_not_sent_with_zero_requests() {
    use safe_cex_engine::write_plan::ElementResult;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let refuse = Arc::new(AtomicBool::new(false));
    let gate = refuse.clone();
    let mut h = Harness::with_signing_entropy(Box::new(move |bytes| {
        if gate.load(Ordering::SeqCst) {
            return Err("fixture entropy unavailable".into());
        }
        bytes.fill(7);
        Ok(())
    }));
    h.seed();
    h.requests.clear();
    refuse.store(true, Ordering::SeqCst);
    let mut first = order(4.0);
    first["clientOrderId"] = json!("pending0");
    let mut second = order(4.0);
    second["clientOrderId"] = json!("pending1");
    place(&mut h, json!([first, second]));
    not_sent(&h);
    let outcomes = h.failures[0]
        .elements
        .as_ref()
        .expect("pending write was settled");
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().all(|element| matches!(&element.result, ElementResult::NotSent { error: Some(error) } if error.outcome == Some(Outcome::NotSent))));
}

#[test]
fn shuffled_batch_acknowledgements_keep_their_caller_element_indices() {
    use safe_cex_engine::write_plan::ElementResult;
    let mut h = harness(4.0, 5.0, 1.0);
    place(&mut h, json!([order(4.0), order(4.0), order(4.0)]));
    let request = h.take("batch-orders");
    let rows: Vec<Value> = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    h.respond(request, &json!({"code":"2", "data":[
        {"clientOrderId":rows[2]["clientOrderId"], "orderId":"102", "code":"0"},
        {"clientOrderId":rows[0]["clientOrderId"], "orderId":"100", "code":"0"},
        {"clientOrderId":rows[1]["clientOrderId"], "orderId":"0", "code":"103003", "msg":"Insufficient balance"}
    ]}).to_string());
    let outcomes = h.failures[0]
        .elements
        .as_ref()
        .expect("caller-correlated outcomes");
    assert_eq!(
        outcomes[0].result,
        ElementResult::Placed {
            order_ids: vec!["100".into()]
        }
    );
    assert!(
        matches!(&outcomes[1].result, ElementResult::NotSent { error: Some(error) } if error.outcome == Some(Outcome::Rejected))
    );
    assert_eq!(
        outcomes[2].result,
        ElementResult::Placed {
            order_ids: vec!["102".into()]
        }
    );
    assert_eq!(h.requests.len(), 1);
}

#[test]
fn native_amend_invalid_wire_size_is_refused_before_transport() {
    for amount in [3.0, 4.5, 6.0] {
        let mut h = harness(4.0, 5.0, 1.0);
        h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"500","side":"sell","size":"4","marginMode":"cross","positionSide":"net","slTriggerPrice":"55000","slOrderPrice":"-1","slTriggerPriceType":"last","state":"live"}]}"#);
        h.handle(
            h.now,
            Event::Command(Command::AmendOrder {
                command: 7,
                id: "500__stop_loss".into(),
                price: None,
                amount: Some(amount),
            }),
        );
        h.collect();
        not_sent(&h);
        let elements = h.failures[0]
            .elements
            .as_ref()
            .expect("full gate rejected native amendment");
        assert_eq!(elements.len(), 1);
        assert!(
            matches!(&elements[0].result, safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(error) } if error.outcome == Some(Outcome::NotSent))
        );
    }
}
