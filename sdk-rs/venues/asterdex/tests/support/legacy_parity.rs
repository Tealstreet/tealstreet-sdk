use super::*;
use safe_cex_engine::{ErrorKind, Outcome};

pub(super) fn configured(
    minimum: f64,
    maximum: f64,
    step: f64,
    notional: f64,
    algos: u32,
) -> Harness {
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
    h.writes.clear();
    h.events.clear();
    h.engine.store_mut().set_tickers(vec![Ticker {
        symbol: "BTCUSDT".into(),
        last: 1.0,
        mark: 1.0,
        ..Ticker::default()
    }]);
    h
}

pub(super) fn ordinary(amount: f64, price: f64) -> serde_json::Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":price})
}

pub(super) fn batch(h: &mut Harness, orders: Vec<serde_json::Value>) {
    h.command(Command::PlaceOrders {
        command: 1,
        orders: orders
            .into_iter()
            .map(|o| serde_json::from_value(o).expect("fixture order"))
            .collect(),
    });
}

pub(super) fn write_sequence(h: &Harness) -> Vec<String> {
    h.writes
        .iter()
        .map(|(method, url)| {
            let (path, query) = url.split_once('?').expect("signed native query");
            let mut parts = vec![
                format!("{method:?}"),
                path.rsplit('/').next().unwrap().to_owned(),
            ];
            for key in ["type", "quantity", "orderId"] {
                if let Some(pair) = query
                    .split('&')
                    .find(|pair| pair.split_once('=').is_some_and(|(name, _)| name == key))
                {
                    parts.push(pair.to_owned());
                }
            }
            parts.join(" ")
        })
        .collect()
}

// Legacy TS: asterdex.exchange.ts:831-867, 911-913 does not rebalance or validate every lot.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | max-qty-splitting (audit trading.rs:1711)
// Legacy TS: asterdex.exchange.ts:975-997; dispatch/result sequence is intentional.
#[test]
fn legacy_limit_tail_is_sent_without_a_new_whole_plan_minimum_gate() {
    let mut h = configured(3.0, 5.0, 1.0, 0.0, 10);
    h.place(ordinary(7.0, 1.0));
    assert_eq!(quantities(&mut h), ["5", "2"]);
    assert!(h.result(1).is_ok());
    assert_eq!(
        write_sequence(&h),
        [
            "Post order type=LIMIT quantity=5",
            "Post order type=LIMIT quantity=2"
        ]
    );
}

// Legacy TS: asterdex.exchange.ts:747-750,975-997 refuses a trailing row and returns accepted IDs.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | serial-batch-preflight (audit trading.rs:1535)
// Legacy TS: asterdex.exchange.ts:975-997; dispatch/result sequence is intentional.
#[test]
fn legacy_batch_keeps_a_valid_prefix_when_the_last_row_is_invalid() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    let mut unsupported = ordinary(1.0, 1.0);
    unsupported["type"] = json!("trailing_stop_market");
    batch(&mut h, vec![ordinary(1.0, 1.0), unsupported]);
    assert_eq!(quantities(&mut h), ["1"]);
    assert!(h.result(1).is_ok());
    assert_eq!(write_sequence(&h), ["Post order type=LIMIT quantity=1"]);
}

// Inherited Rust local-formatting control; TS submits negative triggers to the venue.
// The TS sequence pin for this path uses a native child refusal below.
#[test]
fn inherited_bracket_keeps_the_entry_when_a_child_cannot_be_formatted() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    let mut bracket = ordinary(1.0, 1.0);
    bracket["stopLoss"] = json!(-1.0);
    h.place(bracket);
    assert_eq!(quantities(&mut h), ["1"]);
    assert!(h.result(1).is_ok());
    assert_eq!(write_sequence(&h), ["Post order type=LIMIT quantity=1"]);
}

// Legacy TS: :773-774 and :1571-1584 check each bracket, without reserving the batch total.
// Legacy TS: asterdex.exchange.ts:975-997; dispatch/result sequence is intentional.
#[test]
fn legacy_bracket_capacity_is_checked_per_intent_without_a_new_cumulative_gate() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 1);
    let mut bracket = ordinary(1.0, 1.0);
    bracket["stopLoss"] = json!(0.5);
    batch(&mut h, vec![bracket.clone(), bracket]);
    assert_eq!(quantities(&mut h), ["1", "1", "1", "1"]);
    assert!(h.result(1).is_ok());
    assert_eq!(
        write_sequence(&h),
        [
            "Post order type=LIMIT quantity=1",
            "Post order type=STOP_MARKET quantity=1",
            "Post order type=LIMIT quantity=1",
            "Post order type=STOP_MARKET quantity=1"
        ]
    );
}

// Legacy TS: :1039-1040 cancels, then dispatches the replacement through placeOrder.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | cancel-replace (audit trading.rs:1407)
// Legacy TS: asterdex.exchange.ts:975-997; dispatch/result sequence is intentional.
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
    assert_eq!(
        write_sequence(&h),
        [
            "Delete order orderId=10",
            "Post order type=LIMIT quantity=5",
            "Post order type=LIMIT quantity=1"
        ]
    );
}

// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | runtime placement continuation
// Legacy TS: asterdex.exchange.ts:975-997; dispatch/result sequence is intentional.
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
    assert_eq!(
        write_sequence(&h),
        [
            "Post order type=LIMIT quantity=5",
            "Post order type=LIMIT quantity=5",
            "Post order type=LIMIT quantity=2"
        ]
    );
}

// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | runtime placement continuation
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

// Legacy TS: asterdex.exchange.ts:970-971; exchanges/base.ts:1191-1200.
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

// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | serial-batch-preflight (audit trading.rs:1535)
// Legacy TS: asterdex.exchange.ts:975-997; dispatch/result sequence is intentional.
#[test]
fn legacy_batch_refusal_does_not_stop_the_next_row() {
    for (kind, native) in [
        ("limit", "LIMIT"),
        ("market", "MARKET"),
        ("stop_market", "STOP_MARKET"),
        ("take_profit_market", "TAKE_PROFIT_MARKET"),
    ] {
        let mut h = configured(1.0, 5.0, 1.0, 0.5, 10);
        h.engine.store_mut().set_tickers(vec![]);
        let mut first = ordinary(1.0, 1.0);
        first["type"] = json!(kind);
        first["triggerPrice"] = json!(0.5);
        batch(&mut h, vec![first, ordinary(2.0, 1.0)]);
        let first = h.take(Method::Post);
        h.reply(first, r#"{"code":-2010,"msg":"insufficient margin"}"#);
        assert_eq!(quantities(&mut h), ["2"]);
        assert!(h.result(1).is_ok(), "{kind}");
        assert_eq!(
            write_sequence(&h),
            [
                format!("Post order type={native} quantity=1"),
                "Post order type=LIMIT quantity=2".into(),
            ]
        );
    }
}

// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | separate-TP-SL-children (audit trading.rs:1574)
// Legacy TS: asterdex.exchange.ts:975-997; dispatch/result sequence is intentional.
#[test]
fn mini_parity_bracket_refusal_does_not_stop_the_next_child() {
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
    assert_eq!(
        write_sequence(&h),
        [
            "Post order type=LIMIT quantity=1",
            "Post order type=STOP_MARKET quantity=1",
            "Post order type=TAKE_PROFIT_MARKET quantity=1"
        ]
    );
}

// Legacy TS: asterdex.exchange.ts:975-997; dispatch/result sequence is intentional.
#[test]
fn legacy_capacity_refusal_keeps_the_next_batch_intent() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 1);
    let mut bracket = ordinary(1.0, 1.0);
    bracket["stopLoss"] = json!(0.5);
    bracket["takeProfit"] = json!(2.0);
    batch(&mut h, vec![bracket, ordinary(2.0, 1.0)]);
    assert_eq!(quantities(&mut h), ["2"]);
    assert!(h.result(1).is_ok());
    assert_eq!(write_sequence(&h), ["Post order type=LIMIT quantity=2"]);
}

// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | cancellation fan-out
// Legacy TS: asterdex.exchange.ts:1050-1081; dispatch/result sequence is intentional.
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
    assert_eq!(
        write_sequence(&h),
        ["Delete order orderId=10", "Delete order orderId=11"]
    );
}

// Legacy TS: asterdex.exchange.ts:1039-1040; dispatch/result sequence is intentional.
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
    assert_eq!(write_sequence(&h), ["Delete order orderId=10"]);
}

// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | cancel-replace (audit trading.rs:1407)
// Legacy TS: asterdex.exchange.ts:1040,865-867,975-997; dispatch/result sequence is intentional.
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
    assert_eq!(
        write_sequence(&h),
        [
            "Delete order orderId=10",
            "Post order type=LIMIT quantity=5",
            "Post order type=LIMIT quantity=1"
        ]
    );
}

// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | cancel-replace (audit trading.rs:1407)
// Legacy TS: asterdex.exchange.ts:1040,865-867,975-997; dispatch/result sequence is intentional.
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
    assert_eq!(
        write_sequence(&h),
        [
            "Delete order orderId=10",
            "Post order type=LIMIT quantity=5",
            "Post order type=LIMIT quantity=1"
        ]
    );
}

// Legacy TS: asterdex.exchange.ts:833-867,912; only regular orders split.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | max-qty-splitting (audit trading.rs:1711)
#[test]
fn mini_parity_conditional_is_one_unsplit_write() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    h.place(json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":7.0,"triggerPrice":0.5,"reduceOnly":true}));
    assert_eq!(quantities(&mut h), ["7"]);
    assert_eq!(
        write_sequence(&h),
        ["Post order type=STOP_MARKET quantity=7"]
    );
    assert!(h.result(1).is_ok());
}

// Legacy TS: asterdex.exchange.ts:773-774,813-814,833-867,912; capacity counts two children.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | separate-TP-SL-children (audit trading.rs:1574)
#[test]
fn mini_parity_bracket_splits_entry_but_admits_two_unsplit_children() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 2);
    let mut bracket = ordinary(7.0, 1.0);
    bracket["stopLoss"] = json!(0.5);
    bracket["takeProfit"] = json!(2.0);
    h.place(bracket);
    assert_eq!(quantities(&mut h), ["5", "2", "7", "7"]);
    assert_eq!(
        write_sequence(&h),
        [
            "Post order type=LIMIT quantity=5",
            "Post order type=LIMIT quantity=2",
            "Post order type=STOP_MARKET quantity=7",
            "Post order type=TAKE_PROFIT_MARKET quantity=7",
        ]
    );
    assert!(h.result(1).is_ok());
}

// Legacy TS: asterdex.exchange.ts:912,955; market submission does not read a ticker.
#[test]
fn mini_parity_market_submission_does_not_require_a_mark_snapshot() {
    let mut h = configured(1.0, 5.0, 1.0, 0.5, 10);
    h.engine.store_mut().set_tickers(vec![]);
    h.place(json!({"symbol":"BTCUSDT","type":"market","side":"buy","amount":1.0}));
    assert_eq!(quantities(&mut h), ["1"]);
    assert_eq!(write_sequence(&h), ["Post order type=MARKET quantity=1"]);
    assert!(h.result(1).is_ok());
}

// Legacy TS: asterdex.exchange.ts:1050-1081; a successful cancel survives an earlier refusal.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: asterdex | cancellation fan-out
#[test]
fn mini_parity_cancellation_continues_and_resolves_after_partial_success() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
    h.engine.store_mut().set_orders(vec![
        order("10", OrderType::Limit),
        order("11", OrderType::Limit),
    ]);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["10".into(), "11".into()],
    });
    let first = h.take(Method::Delete);
    h.reply(first, r#"{"code":-2010,"msg":"cancel refused"}"#);
    let second = h.take(Method::Delete);
    h.reply(second, r#"{"orderId":11,"status":"CANCELED"}"#);
    assert_eq!(
        write_sequence(&h),
        ["Delete order orderId=10", "Delete order orderId=11"]
    );
    assert!(h.result(1).is_ok());
}

// Legacy TS: asterdex.exchange.ts:773-774,1571-1584; refuse this bracket before sending its entry.
#[test]
fn legacy_single_intent_capacity_refusal_records_zero_requests() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 1);
    let mut bracket = ordinary(1.0, 1.0);
    bracket["stopLoss"] = json!(0.5);
    bracket["takeProfit"] = json!(2.0);
    h.place(bracket);
    assert_eq!(h.sent_requests, 0);
    assert_eq!(h.sent_writes, 0);
    assert!(write_sequence(&h).is_empty());
    assert!(h.result(1).is_err());
    assert_eq!(h.failures[0].1.outcome, Some(Outcome::NotSent));
}
