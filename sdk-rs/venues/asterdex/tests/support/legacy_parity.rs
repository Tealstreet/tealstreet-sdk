use super::*;
use safe_cex_engine::{ErrorKind, Outcome};

fn configured(minimum: f64, maximum: f64, step: f64, notional: f64, algos: u32) -> Harness {
    let info = json!({"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":1,"quantityPrecision":3,"filters":[
        {"filterType":"PRICE_FILTER","tickSize":"0.1"},
        {"filterType":"LOT_SIZE","stepSize":step.to_string(),"minQty":minimum.to_string(),"maxQty":maximum.to_string()},
        {"filterType":"MARKET_LOT_SIZE","stepSize":step.to_string(),"minQty":minimum.to_string(),"maxQty":maximum.to_string()},
        {"filterType":"MIN_NOTIONAL","notional":notional.to_string()},
        {"filterType":"MAX_NUM_ALGO_ORDERS","limit":algos}
    ]}]}).to_string();
    let mut h = Harness::new();
    h.load(&info);
    h.requests.clear();
    h.sent_requests = 0;
    h.sent_writes = 0;
    h.engine.store_mut().set_tickers(vec![Ticker {
        symbol: "BTCUSDT".into(),
        last: 1.0,
        mark: 1.0,
        ..Ticker::default()
    }]);
    h
}

fn ordinary(amount: f64, price: f64) -> serde_json::Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":price})
}

fn batch(h: &mut Harness, orders: Vec<serde_json::Value>) {
    h.command(Command::PlaceOrders {
        command: 1,
        orders: orders
            .into_iter()
            .map(|o| serde_json::from_value(o).expect("fixture order"))
            .collect(),
    });
}

// Legacy TS: asterdex.exchange.ts:831-867, 911-913 does not rebalance or validate every lot.
#[test]
fn legacy_limit_tail_is_sent_without_a_new_whole_plan_minimum_gate() {
    let mut h = configured(3.0, 5.0, 1.0, 0.0, 10);
    h.place(ordinary(7.0, 1.0));
    assert_eq!(quantities(&mut h), ["5", "2"]);
    assert!(h.result(1).is_ok());
}

// Legacy TS: :975-997 catches each row failure and returns IDs if any succeeded.
#[test]
fn legacy_batch_keeps_a_valid_prefix_when_the_last_row_is_invalid() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    batch(&mut h, vec![ordinary(1.0, 1.0), ordinary(1.0, 0.0)]);
    assert_eq!(quantities(&mut h), ["1"]);
    assert!(h.result(1).is_ok());
}

// Legacy TS: :762-814 submits entry and children through the same partial batch driver.
#[test]
fn legacy_bracket_keeps_the_entry_when_a_child_cannot_be_formatted() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    let mut bracket = ordinary(1.0, 1.0);
    bracket["stopLoss"] = json!(-1.0);
    h.place(bracket);
    assert_eq!(quantities(&mut h), ["1"]);
    assert!(h.result(1).is_ok());
}

// Legacy TS: :773-774 and :1571-1584 check each bracket, without reserving the batch total.
#[test]
fn legacy_bracket_capacity_is_checked_per_intent_without_a_new_cumulative_gate() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 1);
    let mut bracket = ordinary(1.0, 1.0);
    bracket["stopLoss"] = json!(0.5);
    batch(&mut h, vec![bracket.clone(), bracket]);
    assert_eq!(quantities(&mut h), ["1", "1", "1", "1"]);
    assert!(h.result(1).is_ok());
}

// Legacy TS: :1039-1040 cancels, then dispatches the replacement through placeOrder.
#[test]
fn legacy_cancel_replace_does_not_add_a_split_lot_minimum_preflight() {
    let mut h = configured(4.0, 5.0, 1.0, 0.0, 10);
    h.engine
        .store_mut()
        .set_orders(vec![order("10", OrderType::Limit)]);
    h.command(Command::AmendOrder {
        command: 1,
        id: "10".into(),
        price: Some(1.0),
        amount: Some(6.0),
    });
    let cancel = h.take(Method::Delete);
    h.reply(cancel, r#"{"orderId":10,"status":"CANCELED"}"#);
    assert_eq!(quantities(&mut h), ["5", "1"]);
}

#[test]
fn legacy_runtime_refusal_keeps_later_writes_and_partial_success_ids() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    h.place(ordinary(12.0, 1.0));
    let first = h.take(Method::Post);
    h.reply(first, r#"{"orderId":801}"#);
    let second = h.take(Method::Post);
    h.reply(second, r#"{"code":-2010,"msg":"insufficient margin"}"#);
    let third = h.take(Method::Post);
    h.reply(third, r#"{"orderId":803}"#);
    assert_eq!(h.sent_writes, 3);
    assert_eq!(h.result(1).as_ref().unwrap(), "[\"801\",\"803\"]");
}

#[test]
fn reporting_all_refusals_keeps_the_complete_journal_without_stopping_dispatch() {
    use safe_cex_engine::write_plan::ElementResult;
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    h.place(ordinary(12.0, 1.0));
    for _ in 0..3 {
        let next = h.take(Method::Post);
        h.reply(next, r#"{"code":-2010,"msg":"insufficient margin"}"#);
    }
    let error = &h.failures[0].1;
    let rows = error.elements.as_ref().expect("reporting-only journal");
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter().map(|row| row.element.index).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(rows.iter().all(|row| matches!(&row.result, ElementResult::NotSent { error: Some(e) } if e.outcome == Some(Outcome::Rejected))));
    assert_eq!(h.sent_writes, 3);
}

#[test]
fn reporting_gateway_ambiguity_preserves_the_existing_engine_seal() {
    use safe_cex_engine::write_plan::ElementResult;
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    h.place(ordinary(12.0, 1.0));
    let first = h.take(Method::Post);
    h.reply(first, r#"{"orderId":801}"#);
    let second = h.take(Method::Post);
    h.reply_status(second, 503, "gateway lost acknowledgement");
    let rows = h.failures[0]
        .1
        .elements
        .as_ref()
        .expect("engine seal journal");
    assert!(
        matches!(&rows[0].result, ElementResult::Placed { order_ids } if order_ids == &["801"])
    );
    assert!(matches!(rows[1].result, ElementResult::Unknown { .. }));
    assert_eq!(rows[2].result, ElementResult::NotSent { error: None });
    assert_eq!(h.sent_writes, 2);
}

#[test]
fn existing_whole_batch_caller_id_refusal_matches_ts() {
    let mut h = Harness::new();
    let mut invalid = limit();
    invalid["clientOrderId"] = json!("unsupported");
    batch(&mut h, vec![limit(), invalid]);
    assert_eq!(h.sent_requests, 0);
    assert_eq!(h.failures[0].1.outcome, Some(Outcome::NotSent));
    assert_eq!(h.failures[0].1.kind, Some(ErrorKind::InvalidInput));
}

#[test]
fn legacy_batch_refusal_does_not_stop_the_next_row() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    batch(&mut h, vec![ordinary(1.0, 1.0), ordinary(2.0, 1.0)]);
    let first = h.take(Method::Post);
    h.reply(first, r#"{"code":-2010,"msg":"insufficient margin"}"#);
    assert_eq!(quantities(&mut h), ["2"]);
    assert!(h.result(1).is_ok());
}

#[test]
fn legacy_bracket_refusal_does_not_stop_the_next_child() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    let mut bracket = ordinary(1.0, 1.0);
    bracket["stopLoss"] = json!(0.5);
    bracket["takeProfit"] = json!(2.0);
    h.place(bracket);
    let entry = h.take(Method::Post);
    h.reply(entry, r#"{"orderId":801}"#);
    let stop = h.take(Method::Post);
    h.reply(stop, r#"{"code":-2010,"msg":"insufficient margin"}"#);
    let profit = h.take(Method::Post);
    assert!(profit.url.contains("type=TAKE_PROFIT_MARKET"));
    h.reply(profit, r#"{"orderId":803}"#);
    assert_eq!(h.sent_writes, 3);
    assert_eq!(h.result(1).as_ref().unwrap(), "[\"801\",\"803\"]");
}

#[test]
fn legacy_capacity_refusal_keeps_the_next_batch_intent() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 1);
    let mut bracket = ordinary(1.0, 1.0);
    bracket["stopLoss"] = json!(0.5);
    bracket["takeProfit"] = json!(2.0);
    batch(&mut h, vec![bracket, ordinary(2.0, 1.0)]);
    assert_eq!(quantities(&mut h), ["2"]);
    assert!(h.result(1).is_ok());
}

#[test]
fn legacy_cancellation_refusal_does_not_stop_later_cancels() {
    use safe_cex_engine::write_plan::ElementResult;
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    h.engine.store_mut().set_orders(vec![
        order("10", OrderType::Limit),
        order("11", OrderType::Limit),
    ]);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["10".into(), "11".into()],
    });
    for id in ["10", "11"] {
        let cancel = h.take(Method::Delete);
        assert!(cancel.url.contains(&format!("orderId={id}")));
        h.reply(cancel, r#"{"code":-2010,"msg":"cancel refused"}"#);
    }
    assert_eq!(h.sent_writes, 2);
    let rows = h.failures[0].1.elements.as_ref().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|r| matches!(r.result, ElementResult::NotSent { error: Some(_) }))
    );
}

#[test]
fn legacy_cancel_replace_stops_if_its_single_cancel_is_refused() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    h.engine
        .store_mut()
        .set_orders(vec![order("10", OrderType::Limit)]);
    h.command(Command::AmendOrder {
        command: 1,
        id: "10".into(),
        price: Some(1.0),
        amount: Some(1.0),
    });
    let cancel = h.take(Method::Delete);
    h.reply(cancel, r#"{"code":-2010,"msg":"cancel refused"}"#);
    assert_eq!(h.sent_writes, 1);
    assert!(h.requests.is_empty());
    assert!(h.result(1).is_err());
}

#[test]
fn legacy_split_replacement_continues_after_a_rejected_lot() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    h.engine
        .store_mut()
        .set_orders(vec![order("10", OrderType::Limit)]);
    h.command(Command::AmendOrder {
        command: 1,
        id: "10".into(),
        price: Some(1.0),
        amount: Some(6.0),
    });
    let cancel = h.take(Method::Delete);
    h.reply(cancel, r#"{"orderId":10,"status":"CANCELED"}"#);
    let first = h.take(Method::Post);
    h.reply(first, r#"{"code":-2010,"msg":"insufficient margin"}"#);
    let tail = h.take(Method::Post);
    assert!(tail.url.contains("quantity=1"));
    h.reply(tail, r#"{"orderId":803}"#);
    assert_eq!(h.result(1).as_ref().unwrap(), "[\"803\"]");
    assert_eq!(h.sent_writes, 3);
}

#[test]
fn legacy_split_replacement_with_no_accepted_lots_still_fails() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    h.engine
        .store_mut()
        .set_orders(vec![order("10", OrderType::Limit)]);
    h.command(Command::AmendOrder {
        command: 1,
        id: "10".into(),
        price: Some(1.0),
        amount: Some(6.0),
    });
    let cancel = h.take(Method::Delete);
    h.reply(cancel, r#"{"orderId":10,"status":"CANCELED"}"#);
    for _ in 0..2 {
        let lot = h.take(Method::Post);
        h.reply(lot, r#"{"code":-2010,"msg":"insufficient margin"}"#);
    }
    assert!(h.result(1).is_err());
    assert_eq!(h.sent_writes, 3);
    assert_eq!(h.failures[0].1.elements.as_ref().unwrap().len(), 3);
}
