mod support;

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Command, HttpRequest, Method};
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
        json!({"code":"00000","data":{"orderId":"strategy-result","clientOid":body["clientOid"]}})
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
        row_refusal(&mut h, r, &body, uta);
        stopped(&h);
    }
}

#[test]
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
    let (r, _) = take_write(h, "/modify-order");
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
fn native_amend_refusal_continues_to_cancel_but_http_cancel_failure_stops_replacement() {
    let mut h = ready(false);
    amend_fallback(&mut h, 240.0);
    let (r, _) = take_write(&mut h, "/batch-cancel-orders");
    h.reply(r.id, 400, REJECTED);
    stopped(&h);
}

#[test]
fn failed_protective_replacement_restores_once_and_a_failed_restore_stops() {
    for restore_refused in [false, true] {
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
        let (replace, body) = take_write(&mut h, "/place-strategy-order");
        assert_eq!(body["stopLoss"], "85");
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
fn classic_split_envelope_refusal_continues_to_later_lots() {
    let mut h = ready(false);
    place(&mut h, vec![normal(6120.0)]);
    let (first, _) = take_write(&mut h, "/batch-place-order");
    h.reply(first.id, 200, REJECTED);
    let (last, body) = take_write(&mut h, "/batch-place-order");
    assert_eq!(body["orderList"].as_array().expect("last lot").len(), 1);
    accepted(&mut h, last, &body);
    stopped(&h);
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
    stopped(&h);
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
fn classic_amend_fallback_replacement_envelope_refusal_continues_to_later_lots() {
    let mut h = ready(false);
    amend_fallback(&mut h, 6120.0);
    cancel_ack(&mut h);
    let (first, _) = take_write(&mut h, "/batch-place-order");
    h.reply(first.id, 200, REJECTED);
    let (last, body) = take_write(&mut h, "/batch-place-order");
    accepted(&mut h, last, &body);
    stopped(&h);
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
fn classic_http_failure_stops_later_batches_and_standalone_stop() {
    let mut h = ready(false);
    place(&mut h, vec![normal(6120.0), stop()]);
    let (first, _) = take_write(&mut h, "/batch-place-order");
    h.reply(first.id, 400, REJECTED);
    stopped(&h);
}

#[test]
fn classic_nonzero_envelope_with_failure_list_stops_later_batches() {
    let mut h = ready(false);
    place(&mut h, vec![normal(6120.0), stop()]);
    let (first, _) = take_write(&mut h, "/batch-place-order");
    h.reply(first.id, 200, r#"{"code":"40017","msg":"native refusal","data":{"failureList":[{"errorCode":"40017","errorMsg":"row refused"}]}}"#);
    stopped(&h);
}
