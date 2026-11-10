#[allow(dead_code)]
mod support;

use safe_cex_engine::{Action, Command, Event, Method, Outcome};
use serde_json::{Value, json};
use support::*;

const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

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

fn assert_unknown_halt(h: &mut Harness, expected_writes: &[&str], index: usize, placed: &[String]) {
    use safe_cex_engine::write_plan::ElementResult;
    assert_eq!(writes(h), expected_writes);
    assert_eq!(h.failures.len(), 1);
    let error = &h.failures[0];
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(
        error
            .partial
            .as_ref()
            .map_or(&[][..], |p| p.order_ids.as_slice()),
        placed
    );
    let rows = error
        .elements
        .as_ref()
        .expect("journal survives terminal projection");
    let ElementResult::Unknown { error: unknown } = &rows[index].result else {
        panic!("unknown physical write is not reported: {:?}", rows[index]);
    };
    if !placed.is_empty() {
        for (row, id) in rows[..index].iter().zip(placed) {
            assert!(matches!(&row.result, ElementResult::Placed { order_ids }
                if order_ids.as_slice() == std::slice::from_ref(id)));
        }
    }
    assert!(
        rows[index + 1..]
            .iter()
            .all(|row| matches!(row.result, ElementResult::NotSent { error: None }))
    );
    let guidance = ENGINE_UNKNOWN_TEXT;
    let message_ok = error.message.contains(guidance)
        && !error.message.to_ascii_lowercase().contains("reject")
        && !error.message.to_ascii_lowercase().contains("refused")
        && unknown.message.contains(guidance);
    let telemetry = h.actions.iter().filter(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(event))
        if event.code == safe_cex_engine::log::code::CHILD_LEG_FAILED
            && event.message.contains(guidance)
            && event.fields.iter().any(|(key, value)| *key == "outcome" && value.as_str() == Some("unknown"))
    )).count();
    assert!(
        message_ok && telemetry == 1,
        "standard message={message_ok}, telemetry={telemetry}, error={error:?}"
    );
    let mut timers: Vec<_> = h
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms > h.now && *at_ms <= h.now + 1000.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    timers.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (timer, at) in timers {
        h.handle(at, Event::TimerFired { timer });
        h.collect();
    }
    assert_eq!(
        writes(h),
        expected_writes,
        "no delayed continuation or retry"
    );
    assert_eq!(h.failures.len(), 1, "one terminal only");
}

fn unknown_normal_batch(h: &mut Harness, code: &str, message: Option<Value>) {
    let request = h.take("batch-orders");
    let mut rows: Vec<_> = (0..20).map(|i| {
        json!({"orderId":if i==19 {"0".into()} else {(100+i).to_string()},"code":if i==19 {code} else {"0"}})
    }).collect();
    if let Some(message) = message {
        rows[19]["msg"] = message;
    }
    h.respond(request, &json!({"code":"0","data":rows}).to_string());
}

#[test]
fn native_lots_stop_after_an_unknown_row() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts:2232-2257.
    for (code, message) in [
        ("103013", None),
        ("103013", Some(Value::Null)),
        ("103013", Some(json!("success"))),
        ("429", None),
        ("999999", None),
    ] {
        let mut h = setup(1);
        place(&mut h, json!([normal(21)]));
        unknown_normal_batch(&mut h, code, message);
        assert_unknown_halt(
            &mut h,
            &["batch-orders"],
            19,
            &(100..119).map(|id| id.to_string()).collect::<Vec<_>>(),
        );
    }
}

#[test]
fn grouped_batches_stop_after_an_unknown_row() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts:2240-2257.
    let mut h = setup(5);
    place(&mut h, json!(vec![normal(1); 21]));
    unknown_normal_batch(&mut h, "103013", None);
    assert_unknown_halt(
        &mut h,
        &["batch-orders"],
        19,
        &(100..119).map(|id| id.to_string()).collect::<Vec<_>>(),
    );
}

#[test]
fn protective_children_stop_after_an_unknown_row() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts:2295-2315.
    let mut h = setup(5);
    let child =
        json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":1,"price":55000});
    place(&mut h, json!([child, child, child]));
    h.reply(
        "order-tpsl",
        r#"{"code":"0","data":{"tpslId":"100","code":"0"}}"#,
    );
    pause(&mut h, 350);
    h.reply(
        "order-tpsl",
        r#"{"code":"0","data":{"code":"103013","msg":"Internal error"}}"#,
    );
    assert_unknown_halt(&mut h, &["order-tpsl", "order-tpsl"], 1, &["100".into()]);
}

fn unknown_cancel_chunk(protective: bool) {
    let mut h = setup(5);
    let count = if protective { 11 } else { 21 };
    let rows: Vec<_> = (100..100+count).map(|id| if protective {
        json!({"instId":"BTC-USDT","tpslId":id.to_string(),"side":"sell","size":"-1","marginMode":"cross","positionSide":"net","slTriggerPrice":"55000","slOrderPrice":"-1","state":"live"})
    } else {
        json!({"instId":"BTC-USDT","orderCategory":"normal","orderId":id.to_string(),"orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"1","filledSize":"0","price":"60000","state":"live"})
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
    let key = if protective { "tpslId" } else { "orderId" };
    let rows: Vec<_> = payload
        .iter()
        .enumerate()
        .map(|(i, row)| json!({key:row[key],"code":if i+1==payload.len() {"103013"} else {"0"}}))
        .collect();
    h.respond(request, &json!({"code":"0","data":rows}).to_string());
    let elements = h.failures[0].elements.as_ref().unwrap();
    assert!(elements[..payload.len() - 1].iter().all(|row| matches!(
        row.result,
        safe_cex_engine::write_plan::ElementResult::Applied { .. }
    )));
    assert_unknown_halt(&mut h, &[endpoint], payload.len() - 1, &[]);
}

#[test]
fn normal_cancel_chunks_stop_after_an_unknown_row() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts:2436-2466.
    unknown_cancel_chunk(false);
}

#[test]
fn protective_cancel_chunks_stop_after_an_unknown_row() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts:2478-2510.
    unknown_cancel_chunk(true);
}

#[test]
fn normal_cancel_replacement_stops_after_an_unknown_cancel() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts:1355-1361,2445-2464.
    let mut h = setup(5);
    seed_normal(&mut h);
    amend(&mut h, "12", None, Some(3.0));
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"103013"}]}"#,
    );
    assert_unknown_halt(&mut h, &["cancel-batch-orders"], 0, &[]);
}

#[test]
fn protective_cancel_replacement_stops_after_an_unknown_cancel() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts:1355-1361,2445-2464.
    let mut h = Harness::new();
    h.open_private();
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"balance":"1000","available":"1000","frozen":"0"}]}"#,
    );
    h.reply("orders-pending", r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","filledSize":"4","price":"60000","slTriggerPrice":"55000","state":"partially_filled"}]}"#);
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    h.requests.clear();
    h.e.store_mut().update_ticker("BTCUSDT", |ticker| {
        ticker.mark = 60000.0;
        ticker.last = 60000.0;
        ticker.index = 60000.0;
    });
    amend(&mut h, "12__stop_loss", Some(54000.0), None);
    assert!(h.failures.is_empty(), "fixture admission: {:?}", h.failures);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"103013"}]}"#,
    );
    assert_unknown_halt(&mut h, &["cancel-batch-orders"], 0, &[]);
}

#[test]
fn sibling_compensation_stops_after_an_unknown_cancel() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts:1410-1436,2488-2510.
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
    h.reply(
        "amend-tpsl",
        r#"{"code":"102014","msg":"refused","data":null}"#,
    );
    h.reply(
        "cancel-tpsl",
        r#"{"code":"0","data":[{"tpslId":"91","code":"103013"}]}"#,
    );
    assert_unknown_halt(&mut h, &["amend-tpsl", "cancel-tpsl"], 1, &[]);
}

#[test]
fn opening_trigger_children_stop_after_a_code_less_unproven_ack() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts:1558-1559,2038-2050.
    let mut h = setup(5);
    let child = json!({"symbol":"BTCUSDT","type":"conditional","side":"buy","amount":1,"triggerPrice":61000});
    place(&mut h, json!([child, child, child]));
    h.reply(
        "order-algo",
        r#"{"code":"0","data":{"algoId":"100","code":"0"}}"#,
    );
    pause(&mut h, 350);
    h.reply(
        "order-algo",
        r#"{"code":"0","data":{"algoId":"101","msg":"unproven"}}"#,
    );
    assert_unknown_halt(&mut h, &["order-algo", "order-algo"], 1, &["100".into()]);
}

#[test]
fn native_metadata_path_stops_before_protection_after_unknown() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts:2140-2141,2232-2257,1561-1564.
    let mut instruments: Value = serde_json::from_str(INSTRUMENT).unwrap();
    instruments["data"][0]["lotSize"] = json!("-1");
    let mut h = Harness::with_instruments(&instruments.to_string());
    h.open_private();
    h.seed();
    h.requests.clear();
    place(
        &mut h,
        json!([normal(1), {"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":55000}]),
    );
    h.reply("batch-orders", r#"{"code":"0","data":[{"code":"103013"}]}"#);
    assert_unknown_halt(&mut h, &["batch-orders"], 0, &[]);
}

#[test]
fn normalized_nested_reference_replacement_reports_final_unknown() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts:1296-1297,2038-2050.
    let mut h = setup(5);
    seed_trigger(&mut h, "unknown");
    amend(&mut h, "500", Some(62000.0), None);
    h.reply(
        "cancel-algo",
        r#"{"code":"0","data":{"algoId":"500","code":"0"}}"#,
    );
    pause(&mut h, 350);
    let request = h.take("order-algo");
    let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload["attachAlgoOrders"][0]["slTriggerPriceType"], "last");
    h.respond(
        request,
        r#"{"code":"0","data":{"algoId":"101","msg":"unproven"}}"#,
    );
    assert_unknown_halt(&mut h, &["cancel-algo", "order-algo"], 1, &[]);
}

fn engine_sealed_unknown_batch(grouped: bool) {
    use safe_cex_engine::write_plan::ElementResult;
    for status in [408, 503] {
        let mut h = setup(if grouped { 5 } else { 1 });
        place(
            &mut h,
            if grouped {
                json!(vec![normal(1); 21])
            } else {
                json!([normal(21)])
            },
        );
        let request = h.take("batch-orders");
        h.respond_status(request, status, "{}");
        assert_eq!(writes(&h), ["batch-orders"]);
        assert_eq!(h.failures.len(), 1);
        let error = &h.failures[0];
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert!(error.message.contains(ENGINE_UNKNOWN_TEXT)
            || error.message.contains("outcome of this order change is unknown: a later request was not confirmed. Check Open Orders"));
        let rows = error
            .elements
            .as_ref()
            .expect("engine seal retains indexed outcomes");
        assert_eq!(rows.len(), 21);
        assert!(
            rows[..20]
                .iter()
                .all(|row| matches!(&row.result, ElementResult::Unknown { error }
            if error.message.contains(ENGINE_UNKNOWN_TEXT)))
        );
        assert!(matches!(
            rows[20].result,
            ElementResult::NotSent { error: None }
        ));
        let telemetry = || {
            h.actions.iter().filter(|action| matches!(action,
            Action::Notice(safe_cex_engine::Notice::Event(event))
            if event.code == safe_cex_engine::log::code::CHILD_LEG_FAILED
                && event.message.contains(ENGINE_UNKNOWN_TEXT)
                && event.fields.iter().any(|(key, value)| *key == "outcome" && value.as_str() == Some("unknown"))
        )).count()
        };
        assert_eq!(telemetry(), 1, "HTTP {status} must emit Unknown telemetry");
        let timers: Vec<_> = h
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms > h.now && *at_ms <= h.now + 1000. => {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .collect();
        for (timer, at) in timers {
            h.handle(at, Event::TimerFired { timer });
            h.collect();
        }
        assert_eq!(
            writes(&h),
            ["batch-orders"],
            "no retry or suffix after engine seal"
        );
        assert_eq!(h.failures.len(), 1);
        assert_eq!(h.actions.iter().filter(|action| matches!(action,
            Action::Notice(safe_cex_engine::Notice::Event(event))
            if event.code == safe_cex_engine::log::code::CHILD_LEG_FAILED
                && event.fields.iter().any(|(key, value)| *key == "outcome" && value.as_str() == Some("unknown"))
        )).count(), 1, "retirement must not duplicate telemetry");
    }
}

#[test]
fn engine_sealed_unknown_native_lots_emit_telemetry_and_halt() {
    // HTTP failures stop TS:2232-2257 too; the engine-owned seal must retain telemetry.
    engine_sealed_unknown_batch(false);
}

#[test]
fn engine_sealed_unknown_grouped_batches_emit_telemetry_and_halt() {
    // HTTP failures also stop TS:2232-2257; engine-sealed Unknown still needs journal and telemetry.
    engine_sealed_unknown_batch(true);
}
