#[allow(dead_code)]
mod support;

use safe_cex_engine::{Action, Command, Event, Method, Outcome};
use serde_json::{Value, json};
use support::*;

const TS: &str = "packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts";

fn setup(maximum: u32) -> Harness {
    let mut instrument: Value = serde_json::from_str(INSTRUMENT).unwrap();
    instrument["data"][0]["maxLimitSize"] = json!(maximum.to_string());
    instrument["data"][0]["minSize"] = json!("1");
    instrument["data"][0]["lotSize"] = json!("1");
    let mut h = Harness::with_instruments(&instrument.to_string());
    h.open_private();
    h.seed();
    h.requests.clear();
    h
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

fn normal(amount: u32) -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":60000})
}

fn pause(h: &mut Harness, millis: u32) {
    let at = h.now + f64::from(millis);
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("legacy continuation must schedule the remaining write");
    h.handle(at, Event::TimerFired { timer });
    h.collect();
}

fn writes(h: &Harness) -> Vec<&str> {
    h.requests
        .iter()
        .filter(|(method, _)| *method == Method::Post)
        .map(|(_, url)| url.rsplit('/').next().unwrap())
        .collect()
}

fn refused_first_batch(h: &mut Harness, message: Option<Value>) {
    let request = h.take("batch-orders");
    let payload: Vec<Value> = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.len(), 20);
    let mut rows: Vec<_> = payload.iter().enumerate().map(|(i, row)| {
        json!({"orderId":(100+i).to_string(),"clientOrderId":row["clientOrderId"],"code":"0"})
    }).collect();
    rows[19]["orderId"] = json!("0");
    rows[19]["code"] = json!("102014");
    if let Some(message) = message {
        rows[19]["msg"] = message;
    }
    h.respond(request, &json!({"code":"0","data":rows}).to_string());
}

#[test]
fn native_lots_continue_after_a_refused_row_without_a_failure_message() {
    // TS:2232-2257 ignores row.code; :2140-2141 adds no metadata admission gate.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: blofin | native-lot-metadata admission
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: blofin | trading.rs:830 max-qty-splitting
    for msg in [
        None,
        Some(Value::Null),
        Some(json!(" success ")),
        Some(json!("ORDER PLACED")),
    ] {
        let mut h = setup(1);
        place(&mut h, json!([normal(21)]));
        refused_first_batch(&mut h, msg);
        pause(&mut h, 350);
        let request = h.take("batch-orders");
        let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload.as_array().unwrap().len(), 1, "{TS}:2240");
        assert_eq!(payload[0]["size"], "1");
        h.respond(
            request,
            r#"{"code":"1","data":[{"code":"102014","msg":"tail refused"}]}"#,
        );
        let elements = h.failures.last().unwrap().elements.as_ref().unwrap();
        assert_eq!(elements.len(), 21);
        assert!(
            matches!(&elements[19].result, safe_cex_engine::write_plan::ElementResult::NotSent {error:Some(error)} if error.outcome == Some(Outcome::Rejected))
        );
        assert_eq!(writes(&h), ["batch-orders", "batch-orders"]);
    }
}

#[test]
fn grouped_batches_continue_after_a_refused_row() {
    // TS:2240-2257 dispatches the next chunk after a code-only row refusal.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: blofin | trading.rs:890 grouped-batch-chunking
    let mut h = setup(5);
    place(&mut h, json!(vec![normal(1); 21]));
    refused_first_batch(&mut h, None);
    pause(&mut h, 350);
    assert_eq!(writes(&h), ["batch-orders", "batch-orders"]);
    let request = h.take("batch-orders");
    let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
}

#[test]
fn ordinary_batches_stop_on_a_failure_message_even_with_accepted_native_ids() {
    // TS:2253-2257 throws on a supplied non-success message, including empty text.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: blofin | trading.rs:890 grouped-batch-chunking
    for msg in ["denied", ""] {
        let mut h = setup(1);
        place(&mut h, json!([normal(21)]));
        let request = h.take("batch-orders");
        let rows: Vec<_> = (100..120)
            .map(|id| json!({"orderId":id.to_string(),"code":"0","msg":msg}))
            .collect();
        h.respond(request, &json!({"code":"0","data":rows}).to_string());
        assert!(!h.failures.is_empty(), "{TS}:2254");
        assert_eq!(writes(&h), ["batch-orders"]);
        let elements = h.failures.last().unwrap().elements.as_ref().unwrap();
        assert!(matches!(
            elements[0].result,
            safe_cex_engine::write_plan::ElementResult::Placed { .. }
        ));
        assert!(matches!(
            elements[20].result,
            safe_cex_engine::write_plan::ElementResult::NotSent { error: None }
        ));
    }
}

#[test]
fn protective_children_continue_after_a_row_refusal_but_stop_on_envelope_failure() {
    // TS:2295-2315 checks only the envelope between TP/SL children.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: blofin | trading.rs:890 grouped-batch-chunking
    for envelope in ["0", "1"] {
        let mut h = setup(5);
        place(
            &mut h,
            json!([
                {"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":1,"price":55000},
                {"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":1,"price":65000}
            ]),
        );
        h.reply(
            "order-tpsl",
            &json!({"code":envelope,"data":[{"code":"102014","msg":"refused"}]}).to_string(),
        );
        if envelope == "0" {
            pause(&mut h, 350);
            assert_eq!(writes(&h), ["order-tpsl", "order-tpsl"]);
        } else {
            assert_eq!(writes(&h), ["order-tpsl"]);
            assert!(!h.failures.is_empty());
        }
    }
}

fn seed_normal(h: &mut Harness) {
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"4","filledSize":"0","price":"60000","state":"live"}]}"#);
}

fn amend(h: &mut Harness, id: &str, price: Option<f64>, amount: Option<f64>) {
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: id.into(),
            price,
            amount,
        }),
    );
    h.collect();
}

#[test]
fn normal_cancel_replacement_continues_after_an_ignored_row_refusal() {
    // TS:1355-1361/2445-2464: successful envelope ignores row failures before placeOrder.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: blofin | trading.rs:1449 cancel-replace ordinary/protective
    for envelope in ["0", "1"] {
        let mut h = setup(5);
        seed_normal(&mut h);
        amend(&mut h, "12", None, Some(3.0));
        h.reply(
            "cancel-batch-orders",
            &json!({"code":envelope,"data":[{"orderId":"12","code":"102014","msg":"refused"}]})
                .to_string(),
        );
        if envelope == "0" {
            pause(&mut h, 350);
            h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#);
            assert_eq!(writes(&h), ["cancel-batch-orders", "batch-orders"]);
        } else {
            assert_eq!(writes(&h), ["cancel-batch-orders"]);
            assert!(!h.failures.is_empty());
        }
    }
}

fn seed_siblings(h: &mut Harness) {
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"index","slTriggerPrice":"55000","slOrderPrice":"-1","slTriggerPriceType":"mark","state":"live"}]}"#);
}

#[test]
fn sibling_compensation_continues_after_an_ignored_cancel_row_refusal() {
    // TS:1410-1436/2488-2510: failed row under envelope 0 still reaches preserveOrder.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: blofin | compensating-sibling re-place
    for envelope in ["0", "1"] {
        let mut h = setup(5);
        seed_siblings(&mut h);
        h.handle(
            1.0,
            Event::Command(Command::CancelOrders {
                command: 7,
                ids: vec!["91__take_profit".into()],
            }),
        );
        h.collect();
        // Existing Rust amend fast path falls back to TS's cancel/preserve sequence.
        h.reply(
            "amend-tpsl",
            r#"{"code":"102014","msg":"refused","data":null}"#,
        );
        h.reply(
            "cancel-tpsl",
            &json!({"code":envelope,"data":[{"tpslId":"91","code":"102014","msg":"refused"}]})
                .to_string(),
        );
        if envelope == "0" {
            pause(&mut h, 1000);
            let request = h.take("order-tpsl");
            let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            assert!(body.get("tpTriggerPrice").is_none());
            assert_eq!(body["slTriggerPrice"], "55000");
            assert_eq!(body["slTriggerPriceType"], "mark");
            assert_eq!(writes(&h), ["amend-tpsl", "cancel-tpsl", "order-tpsl"]);
        } else {
            assert_eq!(writes(&h), ["amend-tpsl", "cancel-tpsl"]);
            assert!(!h.failures.is_empty());
        }
    }
}

fn seed_trigger(h: &mut Harness, reference: &str) {
    h.ws(&json!({"arg":{"channel":"orders-algo"},"data":[{
        "instId":"BTC-USDT","algoId":"500","orderType":"trigger","side":"buy",
        "marginMode":"cross","positionSide":"net","size":"4","orderPrice":"-1",
        "triggerPrice":"61000","triggerPriceType":"last","state":"live",
        "attachAlgoOrders":[{"slTriggerPrice":"55000","slOrderPrice":"-1","slTriggerPriceType":reference}]
    }]}).to_string());
    h.e.store_mut().update_ticker("BTCUSDT", |ticker| {
        ticker.mark = 60000.0;
        ticker.last = 60000.0;
        ticker.index = 60000.0;
    });
}

#[test]
fn opening_trigger_and_normalized_nested_protection_stop_after_failed_cancel() {
    // TS:1292-1297/2513-2554: normalization doesn't bypass a failed trigger cancellation.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: blofin | nested-trigger-reference refusal
    for reference in ["mark", "unknown"] {
        let mut h = setup(5);
        seed_trigger(&mut h, reference);
        amend(&mut h, "500", Some(62000.0), None);
        h.reply(
            "cancel-algo",
            r#"{"code":"0","data":{"algoId":"500","code":"102014","msg":"refused"}}"#,
        );
        assert_eq!(writes(&h), ["cancel-algo"]);
        assert!(!h.failures.is_empty());
    }
}

#[test]
fn opening_trigger_refuses_invalid_shape_before_cancel() {
    // TS:1271-1297/1971-1997 formats a valid trigger before cancelling the original.
    let mut h = setup(5);
    seed_trigger(&mut h, "mark");
    h.requests.clear();
    amend(&mut h, "500", Some(62000.0), Some(0.0));
    assert!(h.requests.is_empty(), "{TS}:1994");
    assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::NotSent));
}

#[test]
fn normal_and_protective_cancel_chunks_continue_after_ignored_row_refusals() {
    // TS:2436-2466/2478-2510 continues later cancellation chunks under envelope 0.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: blofin | trading.rs:1449 cancel-replace ordinary/protective
    for protective in [false, true] {
        let mut h = setup(5);
        let count = if protective { 11 } else { 21 };
        let rows: Vec<_> = (100..100 + count).map(|id| {
            if protective {
                json!({"instId":"BTC-USDT","tpslId":id.to_string(),"side":"sell","size":"-1","marginMode":"cross","positionSide":"net","slTriggerPrice":"55000","slOrderPrice":"-1","state":"live"})
            } else {
                json!({"instId":"BTC-USDT","orderCategory":"normal","orderId":id.to_string(),"orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"1","filledSize":"0","price":"60000","state":"live"})
            }
        }).collect();
        h.ws(
            &json!({"arg":{"channel":if protective {"orders-algo"} else {"orders"}},"data":rows})
                .to_string(),
        );
        let ids = (100..100 + count)
            .map(|id| {
                if protective {
                    format!("{id}__stop_loss")
                } else {
                    id.to_string()
                }
            })
            .collect();
        h.handle(
            1.0,
            Event::Command(Command::CancelOrders { command: 7, ids }),
        );
        h.collect();
        let endpoint = if protective {
            "cancel-tpsl"
        } else {
            "cancel-batch-orders"
        };
        let request = h.take(endpoint);
        let payload: Vec<Value> = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload.len(), count - 1);
        let key = if protective { "tpslId" } else { "orderId" };
        let rows: Vec<_> = payload.iter().enumerate().map(|(i, row)| {
            json!({key:row[key],"code":if i==0 {"102014"} else {"0"},"msg":"refused"})
        }).collect();
        h.respond(request, &json!({"code":"0","data":rows}).to_string());
        pause(&mut h, if protective { 1000 } else { 350 });
        assert_eq!(writes(&h), [endpoint, endpoint]);
        let request = h.take(endpoint);
        let tail: Vec<Value> = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0][key], (99 + count).to_string());
    }
}

#[test]
#[ignore = "NEEDS-ENGINE-CONTINUE: serial plans stop on Unknown; request_context.rs:2468"]
fn code_only_unknown_row_needs_engine_continuation() {
    // TS:2232-2257 continues even after an internal-error row with no message.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: blofin | trading.rs:890 grouped-batch-chunking
    let mut h = setup(1);
    place(&mut h, json!([normal(21)]));
    let request = h.take("batch-orders");
    let rows: Vec<_> = (0..20).map(|i| {
        json!({"orderId":if i==19 {"0".into()} else {(100+i).to_string()},"code":if i==19 {"103013"} else {"0"}})
    }).collect();
    h.respond(request, &json!({"code":"0","data":rows}).to_string());
    assert!(
        h.failures.is_empty(),
        "TS would continue; engine refused: {:?}",
        h.failures
    );
    pause(&mut h, 350);
    assert_eq!(
        writes(&h),
        ["batch-orders", "batch-orders"],
        "engine failure: {:?}",
        h.failures
    );
}
