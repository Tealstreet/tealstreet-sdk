mod support;

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Command, ErrorKind, HttpRequest, Method};
use serde_json::{Value, json};
use support::*;

const REJECTED: &str = r#"{"code":"40017","msg":"native refusal","data":null}"#;

fn ready(uta: bool) -> Harness {
    if !uta {
        let mut h = Harness::classic();
        h.requests.clear();
        return h;
    }
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
    h.requests.clear();
    h
}

fn normal(amount: f64) -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":90})
}

fn stop() -> Value {
    json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0.5,"price":80})
}

fn limits(h: &mut Harness, minimum: f64, step: f64) {
    h.engine.store_mut().update_markets(|market| {
        market.limits.amount.min = minimum;
        market.limits.amount.max = 3.0;
        market.precision.amount = step;
    });
}

fn place(h: &mut Harness, rows: Vec<Value>) {
    let orders: Vec<PlaceOrderOpts> = rows
        .into_iter()
        .map(|row| serde_json::from_value(row).expect("order fixture"))
        .collect();
    h.command(Command::PlaceOrders { command: 1, orders });
}

fn take_write(h: &mut Harness, path: &str) -> (HttpRequest, Value) {
    assert_eq!(
        h.requests
            .iter()
            .filter(|r| r.method != Method::Get)
            .count(),
        1
    );
    let r = h.take(path);
    let body = serde_json::from_str(r.body.as_deref().expect("write body")).expect("JSON body");
    (r, body)
}

fn accepted(h: &mut Harness, r: HttpRequest, body: &Value) {
    let response = if let Some(rows) = body["orderList"].as_array() {
        let rows: Vec<_> = rows
            .iter()
            .map(|row| {
                json!({
                    "orderId": format!("native-{}", row["clientOid"].as_str().expect("client ID")),
                    "clientOid": row["clientOid"]
                })
            })
            .collect();
        json!({"code":"00000","data":{"successList":rows,"failureList":[]}})
    } else {
        json!({"code":"00000","data":{
            "orderId":format!("native-{}", body["clientOid"].as_str().expect("client ID")),
            "clientOid":body["clientOid"]
        }})
    };
    h.reply(r.id, 200, &response.to_string());
}

fn stopped(h: &Harness) {
    assert!(
        h.requests.iter().all(|r| r.method == Method::Get),
        "no later mutation: {:?}",
        h.requests
    );
    assert_eq!(h.done.len(), 1);
    assert!(h.done[0].1.is_err());
}

fn pending_terminal_kind(h: &Harness) {
    stopped(h);
    assert_eq!(h.failures[0].kind, Some(ErrorKind::Rejected));
    assert_eq!(h.failures[0].outcome, None);
    assert_eq!(h.failures[0].venue_code.as_deref(), Some("40017"));
}

fn row_refusal(h: &mut Harness, r: HttpRequest, body: &Value, uta: bool) {
    if uta {
        h.reply(r.id, 200, REJECTED);
    } else {
        h.reply(r.id, 200, &json!({"code":"00000","data":{
            "successList":[],"failureList":[{"clientOid":body["orderList"][0]["clientOid"],"errorCode":"40017","errorMsg":"row refused"}]
        }}).to_string());
    }
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2284-2308,2319-2335.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:1802/max-qty-splitting
fn split_lot_row_refusal_stops_remaining_lots_and_standalone_stop() {
    for uta in [false, true] {
        let mut h = ready(uta);
        place(&mut h, vec![normal(6120.0), stop()]);
        let (r, body) = take_write(
            &mut h,
            if uta {
                "/place-order"
            } else {
                "/batch-place-order"
            },
        );
        row_refusal(&mut h, r, &body, uta);
        stopped(&h);
    }
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:1999-2008,2302-2308,2326-2334.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:1861/classic-batch-chunks-and-UTA-serial
fn later_batch_row_refusal_stops_remaining_rows_and_standalone_stop() {
    for uta in [false, true] {
        let mut h = ready(uta);
        let mut rows = vec![normal(1.0); 101];
        rows.push(stop());
        place(&mut h, rows);
        let path = if uta {
            "/place-order"
        } else {
            "/batch-place-order"
        };
        let (first, body) = take_write(&mut h, path);
        accepted(&mut h, first, &body);
        let (r, body) = take_write(
            &mut h,
            if uta {
                "/place-order"
            } else {
                "/batch-place-order"
            },
        );
        row_refusal(&mut h, r, &body, uta);
        stopped(&h);
    }
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:1997,2003.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:1814/zero-rounded-anonymous-row
fn zero_rounded_anonymous_row_is_omitted_and_the_remaining_order_is_sent() {
    for uta in [false, true] {
        let mut h = ready(uta);
        place(&mut h, vec![normal(0.0001), normal(1.0)]);
        let (r, body) = take_write(
            &mut h,
            if uta {
                "/place-order"
            } else {
                "/batch-place-order"
            },
        );
        if uta {
            assert_eq!(body["qty"], "1");
        } else {
            assert_eq!(body["orderList"].as_array().expect("rows").len(), 1);
            assert_eq!(body["orderList"][0]["size"], "1");
        }
        accepted(&mut h, r, &body);
        assert!(h.done[0].1.is_ok());
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
    }
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2050-2051,2119-2120,2302-2308,2326-2334.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:663/rounded-attached-trigger
fn rounded_attached_trigger_native_refusal_stops_later_placements() {
    for uta in [false, true] {
        let mut h = ready(uta);
        let mut rows = vec![normal(1.0); 51];
        rows[0]["stopLoss"] = json!(0.04);
        place(&mut h, rows);
        let (r, body) = take_write(
            &mut h,
            if uta {
                "/place-order"
            } else {
                "/batch-place-order"
            },
        );
        assert_eq!(
            if uta {
                &body["stopLoss"]
            } else {
                &body["orderList"][0]["presetStopLossPrice"]
            },
            "0"
        );
        if uta {
            assert_eq!(body["qty"], "1");
        } else {
            assert_eq!(body["orderList"].as_array().expect("first batch").len(), 50);
        }
        row_refusal(&mut h, r, &body, uta);
        stopped(&h);
    }
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2347-2359,2372-2388.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs/algo-price-and-quantity-validation
fn algo_refusal_stops_later_algo_writes() {
    for uta in [false, true] {
        let mut h = ready(uta);
        place(&mut h, vec![stop(), stop()]);
        let (r, _) = take_write(
            &mut h,
            if uta {
                "/place-strategy-order"
            } else {
                "/place-tpsl-order"
            },
        );
        h.reply(r.id, 200, REJECTED);
        stopped(&h);
    }
}

fn amend_fallback(h: &mut Harness, amount: f64) {
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: Some(amount),
    });
    let (r, body) = take_write(h, "/modify-order");
    assert_eq!(body["newSize"], amount.to_string());
    assert_eq!(body["newPrice"], "95");
    assert_ne!(body["newClientOid"], "caller");
    h.reply(r.id, 200, REJECTED);
    assert!(
        h.done.is_empty(),
        "native refusal must release cancel/replace fallback"
    );
}

fn cancel_ack(h: &mut Harness) {
    let (r, _) = take_write(h, "/batch-cancel-orders");
    h.reply(
        r.id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"1"}],"failureList":[]}}"#,
    );
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2472-2480,2505-2529.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs/native-amend-split-bypass
fn native_amend_refusal_continues_to_cancel_but_http_cancel_failure_stops_replacement() {
    let mut h = ready(false);
    amend_fallback(&mut h, 240.0);
    let (r, _) = take_write(&mut h, "/batch-cancel-orders");
    h.reply(r.id, 400, REJECTED);
    stopped(&h);
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2539-2569.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs/compensating-restore-validation
fn failed_protective_replacement_restores_once_and_a_failed_restore_stops() {
    for restore_refused in [false, true] {
        let mut h = ready(true);
        limits(&mut h, 2.0, 0.001);
        h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"usdt-futures","orderId":"stop","symbol":"BTCUSDT","qty":"0.5","stopLoss":"80","slTriggerBy":"market","slOrderType":"market","side":"sell","posSide":"long","status":"pending"}]}"#);
        h.command(Command::AmendOrder {
            command: 1,
            id: "stop".into(),
            price: Some(85.0),
            amount: Some(0.0),
        });
        let (cancel, _) = take_write(&mut h, "/cancel-strategy-order");
        h.reply(cancel.id, 200, r#"{"code":"00000","data":{}}"#);
        let (replace, body) = take_write(&mut h, "/place-strategy-order");
        assert_eq!(body["stopLoss"], "85");
        assert_eq!(body["qty"], "1");
        h.reply(replace.id, 200, REJECTED);
        let (restore, body) = take_write(&mut h, "/place-strategy-order");
        assert_eq!(body["stopLoss"], "80");
        assert_eq!(body["qty"], "0.5");
        if restore_refused {
            h.reply(
                restore.id,
                200,
                r#"{"code":"40017","msg":"restore refused","data":null}"#,
            );
        } else {
            accepted(&mut h, restore, &body);
        }
        stopped(&h);
        assert!(
            h.done[0]
                .1
                .as_ref()
                .expect_err("original failure")
                .contains("native refusal")
        );
    }
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2280-2316 resolves the collected IDs.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:1802/max-qty-splitting
// PARITY-EXCEPTION-PENDING bitget trading.rs:1802/max-qty-splitting/Classic-envelope-terminal: kind Rejected, outcome None; Sam to decide.
fn classic_split_envelope_refusal_continues_to_later_lots() {
    let mut h = ready(false);
    place(&mut h, vec![normal(6120.0)]);
    let (first, _) = take_write(&mut h, "/batch-place-order");
    h.reply(first.id, 200, REJECTED);
    let (last, body) = take_write(&mut h, "/batch-place-order");
    assert_eq!(body["orderList"].as_array().expect("last lot").len(), 1);
    accepted(&mut h, last, &body);
    pending_terminal_kind(&h);
    assert_eq!(
        h.failures[0]
            .partial
            .as_ref()
            .expect("partial result")
            .order_ids
            .len(),
        1
    );
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2008,2280-2316 resolves the collected IDs.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:1861/classic-batch-chunks-and-UTA-serial
// PARITY-EXCEPTION-PENDING bitget trading.rs:1861/classic-batch-chunks-and-UTA-serial/Classic-envelope-terminal: kind Rejected, outcome None; Sam to decide.
fn classic_batch_envelope_refusal_continues_to_later_rows_and_standalone_stop() {
    let mut h = ready(false);
    let mut rows = vec![normal(1.0); 51];
    rows.push(stop());
    place(&mut h, rows);
    let (first, _) = take_write(&mut h, "/batch-place-order");
    h.reply(first.id, 200, REJECTED);
    let (last, body) = take_write(&mut h, "/batch-place-order");
    accepted(&mut h, last, &body);
    let (stop, body) = take_write(&mut h, "/place-tpsl-order");
    accepted(&mut h, stop, &body);
    pending_terminal_kind(&h);
    assert_eq!(
        h.failures[0]
            .partial
            .as_ref()
            .expect("partial result")
            .order_ids
            .len(),
        2
    );
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2472-2480,2505-2529,2316 resolves replacement IDs.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:2378/amend-fallback-and-cancel-replace
// PARITY-EXCEPTION-PENDING bitget trading.rs:2378/amend-fallback-and-cancel-replace/Classic-envelope-terminal: kind Rejected, outcome None; Sam to decide.
fn classic_amend_fallback_replacement_envelope_refusal_continues_to_later_lots() {
    let mut h = ready(false);
    amend_fallback(&mut h, 6120.0);
    cancel_ack(&mut h);
    let (first, _) = take_write(&mut h, "/batch-place-order");
    h.reply(first.id, 200, REJECTED);
    let (last, body) = take_write(&mut h, "/batch-place-order");
    accepted(&mut h, last, &body);
    pending_terminal_kind(&h);
    assert_eq!(
        h.failures[0]
            .partial
            .as_ref()
            .expect("partial result")
            .order_ids
            .len(),
        1
    );
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2306-2308.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:1861/classic-batch-chunks-and-UTA-serial
fn classic_http_failure_stops_later_batches_and_standalone_stop() {
    let mut h = ready(false);
    place(&mut h, vec![normal(6120.0), stop()]);
    let (first, _) = take_write(&mut h, "/batch-place-order");
    h.reply(first.id, 400, REJECTED);
    stopped(&h);
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2302-2308.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:1861/classic-batch-chunks-and-UTA-serial
fn classic_nonzero_envelope_with_failure_list_stops_later_batches() {
    let mut h = ready(false);
    place(&mut h, vec![normal(6120.0), stop()]);
    let (first, _) = take_write(&mut h, "/batch-place-order");
    h.reply(first.id, 200, r#"{"code":"40017","msg":"native refusal","data":{"failureList":[{"errorCode":"40017","errorMsg":"row refused"}]}}"#);
    stopped(&h);
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2039,2059-2073,2127-2139.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:1802/max-qty-splitting
fn split_lots_below_the_native_value_minimum_are_sent_without_rebalancing() {
    for uta in [false, true] {
        let mut h = ready(uta);
        limits(&mut h, 2.0, 1.0);
        let mut order = normal(6.0);
        order["price"] = json!(1);
        place(&mut h, vec![order]);
        let mut sizes = vec![];
        for _ in 0..if uta { 2 } else { 1 } {
            let (r, body) = take_write(
                &mut h,
                if uta {
                    "/place-order"
                } else {
                    "/batch-place-order"
                },
            );
            if uta {
                assert_eq!(body["price"], "1");
                sizes.push(body["qty"].clone());
            } else {
                let rows = body["orderList"].as_array().expect("split rows");
                assert!(rows.iter().all(|row| row["price"] == "1"));
                sizes.extend(rows.iter().map(|row| row["size"].clone()));
            }
            accepted(&mut h, r, &body);
        }
        assert_eq!(sizes, vec![json!("3"), json!("3")]);
        assert!(h.done[0].1.is_ok());
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
    }
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:1994-2008,2284-2297,2319-2335.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:1861/classic-batch-chunks-and-UTA-serial
fn below_minimum_last_row_is_sent_after_the_valid_prefix() {
    for uta in [false, true] {
        let mut h = ready(uta);
        limits(&mut h, 2.0, 1.0);
        let mut rows = vec![normal(3.0); 50];
        rows.push(normal(1.0));
        place(&mut h, rows);
        let path = if uta {
            "/place-order"
        } else {
            "/batch-place-order"
        };
        let mut submitted = vec![];
        for _ in 0..if uta { 50 } else { 1 } {
            let (r, body) = take_write(&mut h, path);
            if uta {
                submitted.push(body["qty"].clone());
            } else {
                let rows = body["orderList"].as_array().expect("first batch");
                assert_eq!(rows.len(), 50);
                submitted.extend(rows.iter().map(|row| row["size"].clone()));
            }
            accepted(&mut h, r, &body);
        }
        assert_eq!(submitted, vec![json!("3"); 50]);
        let (r, body) = take_write(&mut h, path);
        if uta {
            assert_eq!(body["qty"], "1");
        } else {
            assert_eq!(body["orderList"].as_array().expect("last batch").len(), 1);
            assert_eq!(body["orderList"][0]["size"], "1");
        }
        row_refusal(&mut h, r, &body, uta);
        stopped(&h);
        assert_eq!(
            h.failures[0]
                .partial
                .as_ref()
                .expect("accepted prefix")
                .order_ids
                .len(),
            50
        );
    }
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2445-2481,2503-2529,2059-2073.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs:2378/amend-fallback-and-cancel-replace
fn amend_fallback_sends_each_below_minimum_value_lot_after_cancel() {
    let mut h = ready(false);
    limits(&mut h, 2.0, 1.0);
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(1.0),
        amount: Some(6.0),
    });
    let (amend, body) = take_write(&mut h, "/modify-order");
    assert_eq!(body["newSize"], "6");
    assert_eq!(body["newPrice"], "1");
    h.reply(amend.id, 200, REJECTED);
    cancel_ack(&mut h);
    let (replace, body) = take_write(&mut h, "/batch-place-order");
    let rows = body["orderList"].as_array().expect("replacement lots");
    assert_eq!(
        rows.iter()
            .map(|row| row["size"].clone())
            .collect::<Vec<_>>(),
        vec![json!("3"), json!("3")]
    );
    assert!(rows.iter().all(|row| row["price"] == "1"));
    h.reply(replace.id, 200, &json!({"code":"00000","data":{
        "successList":[{"orderId":"partial-first","clientOid":rows[0]["clientOid"]}],
        "failureList":[{"clientOid":rows[1]["clientOid"],"errorCode":"40017","errorMsg":"second lot refused"}]
    }}).to_string());
    stopped(&h);
    assert_eq!(
        h.failures[0]
            .partial
            .as_ref()
            .expect("partial replacement")
            .order_ids,
        vec!["partial-first"]
    );
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2167-2207,2240-2248.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitget | trading.rs/algo-price-and-quantity-validation
fn below_minimum_protective_quantity_is_sent_at_its_rounded_trigger() {
    for uta in [false, true] {
        let mut h = ready(uta);
        limits(&mut h, 2.0, 0.001);
        let mut order = stop();
        order["price"] = json!(80.04);
        place(&mut h, vec![order, stop()]);
        let (r, body) = take_write(
            &mut h,
            if uta {
                "/place-strategy-order"
            } else {
                "/place-tpsl-order"
            },
        );
        if uta {
            assert_eq!(body["qty"], "0.5");
            assert_eq!(body["stopLoss"], "80");
        } else {
            assert_eq!(body["size"], "0.5");
            assert_eq!(body["triggerPrice"], "80");
        }
        h.reply(r.id, 200, REJECTED);
        stopped(&h);
    }
}

const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";
const ENGINE_SEALED_UNKNOWN_TEXT: &str = "The outcome of this order change is unknown: a later request was not confirmed. Check Open Orders";
const UNKNOWN_REPLY: &str = r#"{"code":"40010","msg":"Request timeout","data":null}"#;

fn unknown_pin(h: &Harness, placed: &[String], unknown_identity: &str) {
    stopped(h);
    let error = &h.failures[0];
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert!(
        error.message.contains(unknown_identity),
        "{}",
        error.message
    );
    assert!(
        error.message.contains(ENGINE_UNKNOWN_TEXT)
            || error.message.contains(ENGINE_SEALED_UNKNOWN_TEXT),
        "{}",
        error.message
    );
    assert_eq!(
        error
            .partial
            .as_ref()
            .map(|p| p.order_ids.as_slice())
            .unwrap_or(&[]),
        placed
    );
    let events: Vec<_> = h
        .events
        .iter()
        .filter(|e| e.code == "order.outcome_unknown")
        .collect();
    assert_eq!(events.len(), 1, "one journal companion event");
    assert!(events[0].message.contains(ENGINE_UNKNOWN_TEXT));
    assert!(
        events[0]
            .fields
            .iter()
            .any(|(key, value)| *key == "placedCount"
                && *value == safe_cex_engine::log::Value::Num(placed.len() as f64))
    );
    assert!(events[0].fields.iter().any(|(key, value)| {
        *key == "unknownRequest"
            && value
                .as_str()
                .is_some_and(|value| value.contains(unknown_identity))
    }));
}

#[test]
fn classic_batch_row_timeout_emits_unknown_once_without_suffix_writes() {
    for accepted_rows in [false, true] {
        let mut h = ready(false);
        let mut orders = vec![normal(1.0); 51];
        orders.push(stop());
        place(&mut h, orders);
        let (first, body) = take_write(&mut h, "/batch-place-order");
        let rows = body["orderList"].as_array().unwrap();
        let unknown = rows[0]["clientOid"].as_str().unwrap();
        let successes: Vec<_> = rows.iter().skip(1).filter(|_| accepted_rows).map(|row| {
            json!({"orderId":format!("native-{}",row["clientOid"].as_str().unwrap()),"clientOid":row["clientOid"]})
        }).collect();
        let placed: Vec<_> = successes
            .iter()
            .map(|row| row["orderId"].as_str().unwrap().to_owned())
            .collect();
        h.reply(first.id, 200, &json!({"code":"00000","data":{
            "successList":successes,
            "failureList":[{"clientOid":unknown,"errorCode":"40010","errorMsg":"Request timed out"}]
        }}).to_string());
        assert_eq!(
            h.failures[0].outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
        assert_eq!(
            h.events
                .iter()
                .filter(|event| event.code == "order.outcome_unknown")
                .count(),
            1
        );
        unknown_pin(&h, &placed, unknown);
        assert!(h.failures[0].message.contains(ENGINE_UNKNOWN_TEXT));
        let event = h
            .events
            .iter()
            .find(|event| event.code == "order.outcome_unknown")
            .unwrap();
        assert_eq!(event.message, ENGINE_UNKNOWN_TEXT);
        accepted(&mut h, first, &body);
        unknown_pin(&h, &placed, unknown);
    }
    let mut h = ready(false);
    place(&mut h, vec![normal(1.0)]);
    let (request, body) = take_write(&mut h, "/batch-place-order");
    row_refusal(&mut h, request, &body, false);
    assert!(
        !h.events
            .iter()
            .any(|event| event.code == "order.outcome_unknown")
    );
}

fn unknown_placement_sequence(split: bool, bare: bool, attached: bool) {
    for uta in [false, true] {
        let mut h = ready(uta);
        let mut rows = if split {
            vec![normal(12120.0)]
        } else {
            vec![normal(1.0); 101]
        };
        if attached {
            for row in &mut rows {
                row["stopLoss"] = json!(0.04);
            }
        }
        rows.push(stop());
        place(&mut h, rows);
        let path = if uta {
            "/place-order"
        } else {
            "/batch-place-order"
        };
        let (first, body) = take_write(&mut h, path);
        let placed: Vec<String> = if uta {
            vec![format!("native-{}", body["clientOid"].as_str().unwrap())]
        } else {
            body["orderList"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| format!("native-{}", row["clientOid"].as_str().unwrap()))
                .collect()
        };
        accepted(&mut h, first, &body);
        let (r, body) = take_write(&mut h, path);
        let identity = if uta {
            body["clientOid"].as_str().unwrap()
        } else {
            body["orderList"][0]["clientOid"].as_str().unwrap()
        }
        .to_owned();
        if attached {
            assert_eq!(
                if uta {
                    &body["stopLoss"]
                } else {
                    &body["orderList"][0]["presetStopLossPrice"]
                },
                "0"
            );
        }
        h.reply(
            r.id,
            200,
            if bare {
                UNKNOWN_REPLY
            } else {
                r#"{"code":"00000","data":{}}"#
            },
        );
        unknown_pin(&h, &placed, &identity);
        h.reply(r.id, 200, r#"{"code":"00000","data":{"orderId":"late"}}"#);
        unknown_pin(&h, &placed, &identity);
    }
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2299-2308,2330-2334.
fn unknown_exception_max_quantity_split() {
    unknown_placement_sequence(true, false, false);
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2291-2308.
fn unknown_exception_max_quantity_classic_envelope() {
    unknown_placement_sequence(true, true, false);
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:1999-2008,2299-2308,2330-2334.
fn unknown_exception_batch_and_serial() {
    unknown_placement_sequence(false, false, false);
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2008,2291-2308.
fn unknown_exception_batch_classic_envelope() {
    unknown_placement_sequence(false, true, false);
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2050-2051,2119-2120,2299-2308,2330-2334.
fn unknown_exception_rounded_attached_trigger() {
    unknown_placement_sequence(false, false, true);
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2351-2359,2379-2388.
fn unknown_exception_algo_identity() {
    for uta in [false, true] {
        let mut h = ready(uta);
        place(&mut h, vec![stop(), stop(), stop()]);
        let path = if uta {
            "/place-strategy-order"
        } else {
            "/place-tpsl-order"
        };
        let (first, body) = take_write(&mut h, path);
        let placed = vec![format!("native-{}", body["clientOid"].as_str().unwrap())];
        accepted(&mut h, first, &body);
        let (r, body) = take_write(&mut h, path);
        let identity = body["clientOid"].as_str().unwrap();
        h.reply(r.id, 200, r#"{"code":"00000","data":{}}"#);
        unknown_pin(&h, &placed, identity);
    }
}

fn unknown_amend_leg(cancel: bool, replacement: bool) {
    let mut h = ready(false);
    if replacement {
        amend_fallback(&mut h, 12120.0);
        cancel_ack(&mut h);
        let (first, body) = take_write(&mut h, "/batch-place-order");
        let placed: Vec<_> = body["orderList"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| format!("native-{}", row["clientOid"].as_str().unwrap()))
            .collect();
        accepted(&mut h, first, &body);
        let (r, body) = take_write(&mut h, "/batch-place-order");
        let identity = body["orderList"][0]["clientOid"].as_str().unwrap();
        h.reply(r.id, 200, UNKNOWN_REPLY);
        unknown_pin(&h, &placed, identity);
    } else {
        h.command(Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: Some(95.0),
            amount: Some(240.0),
        });
        let (r, body) = take_write(&mut h, "/modify-order");
        if cancel {
            h.reply(r.id, 200, REJECTED);
            let (r, _) = take_write(&mut h, "/batch-cancel-orders");
            h.reply(r.id, 200, r#"{"code":"00000","data":{}}"#);
            unknown_pin(&h, &[], "1");
        } else {
            let identity = body["newClientOid"].as_str().unwrap();
            h.reply(r.id, 200, UNKNOWN_REPLY);
            unknown_pin(&h, &[], identity);
        }
    }
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2472-2480,2505-2529,1760-1769.
fn unknown_exception_amend_cancel_replace() {
    unknown_amend_leg(true, false);
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2472-2480,2505-2529.
fn unknown_exception_native_amend_split_bypass() {
    unknown_amend_leg(false, false);
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2551-2569.
fn unknown_exception_compensating_restore() {
    for restore in [false, true] {
        let mut h = ready(true);
        h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"usdt-futures","orderId":"stop","symbol":"BTCUSDT","qty":"0.5","stopLoss":"80","slTriggerBy":"market","slOrderType":"market","side":"sell","posSide":"long","status":"pending"}]}"#);
        h.command(Command::AmendOrder {
            command: 1,
            id: "stop".into(),
            price: Some(85.0),
            amount: Some(0.0),
        });
        let (cancel, _) = take_write(&mut h, "/cancel-strategy-order");
        h.reply(cancel.id, 200, r#"{"code":"00000","data":{}}"#);
        if restore {
            let (r, _) = take_write(&mut h, "/place-strategy-order");
            h.reply(r.id, 200, REJECTED);
        }
        let (r, body) = take_write(&mut h, "/place-strategy-order");
        let identity = body["clientOid"].as_str().unwrap();
        h.reply(r.id, 200, UNKNOWN_REPLY);
        unknown_pin(&h, &[], identity);
    }
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2291-2316 (40010, not 40017).
fn unknown_exception_split_envelope_terminal_witness() {
    unknown_placement_sequence(true, true, false);
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2008,2291-2316 (40010, not 40017).
fn unknown_exception_batch_envelope_terminal_witness() {
    unknown_placement_sequence(false, true, false);
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts:2472-2480,2505-2529,2291-2316 (40010).
fn unknown_exception_amend_envelope_terminal_witness() {
    unknown_amend_leg(false, true);
}
