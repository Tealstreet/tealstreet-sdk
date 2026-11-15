use super::*;
use safe_cex_engine::model::OrderType;
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{HttpRequest, Method, Outcome, Response, VenueError};
use serde_json::{Value, json};

fn mutation(out: &[Action]) -> HttpRequest {
    let writes: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.method != Method::Get => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(writes.len(), 1, "one physical mutation: {out:?}");
    writes[0].clone()
}

fn pin(
    out: &[Action],
    command: u32,
    placed: &[String],
    identity: &str,
    telemetry: bool,
    created: usize,
) -> VenueError {
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get)),
        "no suffix, retry or restore: {out:?}"
    );
    let terminals = terminals(out, command);
    let [Err(error)] = terminals.as_slice() else {
        panic!("one Unknown terminal: {out:?}")
    };
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(
        error.message.contains(if telemetry {
            ENGINE_UNKNOWN_TEXT
        } else {
            ENGINE_HTTP_UNKNOWN_TEXT
        }) || (!telemetry && error.message.contains(ENGINE_UNKNOWN_AFTER_REFUSAL)),
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
    if telemetry {
        assert_eq!(
            out.iter()
                .filter(|action| matches!(action,
                Action::Notice(Notice::CommandError { command: id, .. }) if *id == command))
                .count(),
            1,
            "one attributed Unknown diagnostic"
        );
        assert!(
            error.message.contains(identity),
            "unconfirmed identity {identity}: {}",
            error.message
        );
        let events: Vec<_> = out
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                    Some(event)
                }
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 1, "one attributed companion event");
        assert_eq!(events[0].message, ENGINE_UNKNOWN_TEXT);
        for (name, expected) in [("command", command as f64), ("placedCount", created as f64)] {
            assert!(
                events[0].fields.iter().any(|(key, value)| *key == name
                    && *value == safe_cex_engine::log::Value::Num(expected)),
                "{name}: {:?}",
                events[0]
            );
        }
        assert!(
            events[0]
                .fields
                .iter()
                .any(|(key, value)| *key == "unknownRequest"
                    && value.as_str().is_some_and(|s| s.contains(identity)))
        );
    }
    error.clone()
}

fn normal(amount: f64) -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":38000})
}
fn algo(conditional: bool) -> Value {
    if conditional {
        json!({"symbol":"BTCUSDT","type":"conditional","side":"buy","amount":5,"triggerPrice":39000})
    } else {
        json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":5,"price":36000})
    }
}

fn placement(case: &str) {
    let mut e = trading_engine();
    e.store_mut().update_markets(|m| {
        m.precision.amount = 1.;
        m.limits.amount.min = 4.;
        m.limits.amount.max = 5.;
    });
    let rows = match case {
        "split" => vec![normal(106.), algo(false)],
        "batch" | "lost" => {
            let mut rows = vec![normal(5.); 41];
            rows.push(algo(false));
            rows
        }
        "children" => {
            let mut o = normal(5.);
            o["stopLoss"] = json!(36000);
            o["takeProfit"] = json!(42000);
            vec![o]
        }
        "algo" => vec![algo(false); 3],
        "ordinary" => {
            let mut rows = vec![normal(5.); 21];
            rows.push(algo(false));
            rows
        }
        "conditional" => vec![algo(true); 3],
        _ => unreachable!(),
    };
    e.handle(
        5.,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: serde_json::from_value(json!(rows)).unwrap(),
        }),
    );
    let first = mutation(&prepared(&mut e, 5.));
    let payload: Value = serde_json::from_str(first.body.as_deref().unwrap()).unwrap();
    let rows = payload.as_array().cloned().unwrap_or_else(|| vec![payload]);
    let placed: Vec<_> = (0..rows.len()).map(|i| format!("placed-{i}")).collect();
    let acks: Vec<_> = placed
        .iter()
        .map(|id| {
            if first.url.ends_with("/order-algo") {
                json!({"algoId":id,"sCode":"0"})
            } else {
                json!({"ordId":id,"sCode":"0"})
            }
        })
        .collect();
    reply(
        &mut e,
        &Action::Http(first),
        &json!({"code":"0","data":acks}).to_string(),
        6.,
    );
    let second = mutation(&prepared(&mut e, 6.));
    let payload: Value = serde_json::from_str(second.body.as_deref().unwrap()).unwrap();
    let rows = payload.as_array().cloned().unwrap_or_else(|| vec![payload]);
    let mut placed = placed;
    let identity_row = if case == "lost" {
        rows.first()
    } else {
        rows.last()
    }
    .unwrap();
    let identity = identity_row
        .get("clOrdId")
        .or_else(|| identity_row.get("algoClOrdId"))
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    let acks: Vec<_> = rows.iter().enumerate().map(|(i, _)| {
        let unknown = case == "lost" || i + 1 == rows.len();
        let id = if unknown { "unproven-id".to_owned() } else {
            let id = format!("same-batch-{i}");
            placed.push(id.clone());
            id
        };
        let field = if second.url.ends_with("/order-algo") { "algoId" } else { "ordId" };
        json!({field: id, "sCode": if unknown { "50004" } else { "0" }, "sMsg": if unknown { "Operation timeout" } else { "" }})
    }).collect();
    if case == "lost" {
        e.handle(
            7.,
            Event::HttpFailed {
                request: second.id,
                error: "lost write reply",
            },
        );
    } else {
        reply(
            &mut e,
            &Action::Http(second.clone()),
            &json!({"code":"0","data":acks}).to_string(),
            7.,
        );
    }
    let mut out = prepared(&mut e, 7.);
    let error = pin(&out, 1, &placed, &identity, true, placed.len());
    let elements = error.elements.as_ref().expect("placement journal");
    assert!(elements.iter().any(|e|matches!(&e.result,ElementResult::Unknown {error} if error.message.contains(ENGINE_UNKNOWN_TEXT))));
    assert!(
        elements
            .iter()
            .any(|e| matches!(&e.result, ElementResult::NotSent { error: None })),
        "unsent suffix: {elements:?}"
    );
    let known: Vec<_> = elements
        .iter()
        .flat_map(|e| match &e.result {
            ElementResult::Placed { order_ids } => order_ids.clone(),
            _ => vec![],
        })
        .collect();
    assert_eq!(known, placed);
    reply(
        &mut e,
        &Action::Http(second),
        &envelope(r#"{"ordId":"late","sCode":"0"}"#),
        8.,
    );
    out.extend(prepared(&mut e, 8.));
    out.extend(fire_armed(&mut e, &out.clone(), 30_000.));
    pin(&out, 1, &placed, &identity, true, placed.len());
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts:2291-2302.
fn unknown_exception_split_tail() {
    placement("split");
}
#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts:2291-2302.
fn unknown_exception_later_batch() {
    placement("batch");
}
#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts:2319-2330.
fn unknown_exception_tpsl_child() {
    placement("children");
}
#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts:2319-2330.
fn unknown_exception_standalone_algo() {
    placement("algo");
}
#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts:2291-2302.
fn unknown_exception_ordinary_runtime() {
    placement("ordinary");
}
#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts:2319-2330.
fn unknown_exception_conditional_runtime() {
    placement("conditional");
}

#[test]
fn lost_reply_preserves_prefix_journal_and_one_companion_without_retry() {
    placement("lost");
}

#[test]
fn native_50004_emits_one_diagnostic_and_companion_without_changing_its_journal() {
    for body in [
        r#"{"code":"50004","msg":"Endpoint request timeout","data":[]}"#,
        r#"{"code":"0","data":[{"ordId":"uncertain","sCode":"50004","sMsg":"Operation timeout"}]}"#,
    ] {
        let mut e = trading_engine();
        let out = place(
            &mut e,
            vec![order(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000,"stopLoss":36000}"#,
            )],
        );
        let request = mutation(&out);
        reply(&mut e, &Action::Http(request.clone()), body, 6.);
        let mut out = prepared(&mut e, 6.);
        let diagnostics = out
            .iter()
            .filter(|action| {
                matches!(
                    action,
                    Action::Notice(Notice::CommandError { command: 1, .. })
                )
            })
            .count();
        let telemetry = out
            .iter()
            .filter(|action| {
                matches!(action,
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown")
            })
            .count();
        let payload: Vec<Value> = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        let identity = payload[0]["clOrdId"].as_str().unwrap();
        let error = pin(&out, 1, &[], identity, true, 0);
        assert_eq!(
            error.message,
            format!(
                "{ENGINE_UNKNOWN_TEXT}; {ENGINE_UNKNOWN_TEXT}; unconfirmed request {}",
                request.body.as_deref().unwrap()
            )
        );
        let elements = error.elements.as_ref().expect("placement journal");
        assert_eq!(elements.len(), 2);
        assert!(
            matches!(&elements[0].result, ElementResult::Unknown { error }
            if error.message == ENGINE_UNKNOWN_TEXT)
        );
        assert_eq!(elements[1].result, ElementResult::NotSent { error: None });
        assert_eq!((diagnostics, telemetry), (1, 1), "{out:?}");
        reply(
            &mut e,
            &Action::Http(request),
            &envelope(r#"{"ordId":"late","sCode":"0"}"#),
            7.,
        );
        out.extend(prepared(&mut e, 7.));
        out.extend(fire_armed(&mut e, &out.clone(), 30_000.));
        assert_eq!(pin(&out, 1, &[], identity, true, 0), error);
        assert_eq!(
            out.iter()
                .filter(|action| matches!(
                    action,
                    Action::Notice(Notice::CommandError { command: 1, .. })
                ))
                .count(),
            1
        );
    }
}

fn confirmed_sibling_placement(unknown_index: usize) {
    for code in ["0", "50004", "51412"] {
        let mut e = trading_engine();
        let out = place(
            &mut e,
            serde_json::from_value(json!([normal(1.), normal(1.), algo(false)])).unwrap(),
        );
        let request = mutation(&out);
        let payload: Vec<Value> = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload.len(), 2);
        assert!(payload.iter().all(|row| row["sz"] == "1"));
        let identity = payload[unknown_index]["clOrdId"].as_str().unwrap();
        let mut rows = vec![
            json!({"sCode":code}),
            json!({"ordId":"proven-sibling","sCode":"0"}),
        ];
        if unknown_index == 1 {
            rows.reverse();
        }
        let body = json!({"code":"0","data":rows}).to_string();
        reply(&mut e, &Action::Http(request.clone()), &body, 6.);
        let mut out = prepared(&mut e, 6.);
        let events: Vec<_> = out
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                    Some(event)
                }
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 1);
        assert!(
            events[0]
                .fields
                .iter()
                .any(|(key, value)| *key == "placedCount"
                    && *value == safe_cex_engine::log::Value::Num(1.)),
            "code={code}: {:?}",
            events[0]
        );
        let placed = ["proven-sibling".to_owned()];
        let error = pin(&out, 1, &placed, identity, true, 1);
        let elements = error.elements.as_ref().expect("placement journal");
        assert_eq!(elements.len(), 3);
        assert!(matches!(
            &elements[unknown_index].result,
            ElementResult::Unknown { .. }
        ));
        assert!(
            matches!(&elements[1 - unknown_index].result, ElementResult::Placed { order_ids } if order_ids == &placed)
        );
        assert_eq!(elements[2].result, ElementResult::NotSent { error: None });
        reply(&mut e, &Action::Http(request), &body, 7.);
        out.extend(prepared(&mut e, 7.));
        out.extend(fire_armed(&mut e, &out.clone(), 30_000.));
        assert_eq!(pin(&out, 1, &placed, identity, true, 1), error);
    }
}

#[test]
fn missing_id_first_retains_confirmed_sibling_placement() {
    confirmed_sibling_placement(0);
}

#[test]
fn missing_id_last_retains_confirmed_sibling_placement() {
    confirmed_sibling_placement(1);
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts:2355-2363,2375-2383.
fn unknown_exception_cancel_rows() {
    for algo in [false, true] {
        let mut e = trading_engine();
        let count = if algo { 21 } else { 41 };
        let ids: Vec<_> = (1..=count)
            .map(|i| {
                if algo {
                    format!("{i}_sl")
                } else {
                    i.to_string()
                }
            })
            .collect();
        for id in &ids {
            e.store_mut().upsert_order(stored(
                id,
                if algo {
                    OrderType::StopLoss
                } else {
                    OrderType::Limit
                },
            ));
        }
        e.handle(
            5.,
            Event::Command(Command::CancelOrders { command: 2, ids }),
        );
        let first = mutation(&prepared(&mut e, 5.));
        let rows: Vec<Value> = serde_json::from_str(first.body.as_deref().unwrap()).unwrap();
        let field = if algo { "algoId" } else { "ordId" };
        // Applied cancellation targets are not newly created order IDs.
        let acks: Vec<_> = rows
            .iter()
            .map(|r| json!({field:r[field],"sCode":"0"}))
            .collect();
        reply(
            &mut e,
            &Action::Http(first),
            &json!({"code":"0","data":acks}).to_string(),
            6.,
        );
        let second = mutation(&prepared(&mut e, 6.));
        let rows: Vec<Value> = serde_json::from_str(second.body.as_deref().unwrap()).unwrap();
        let identity = rows[0][field].as_str().unwrap().to_owned();
        let acks:Vec<_>=rows.iter().enumerate().map(|(i,r)|json!({field:r[field],"sCode":if i==0 {"51412"} else {"0"},"sMsg":if i==0 {"uncertain cancel"} else {""}})).collect();
        reply(
            &mut e,
            &Action::Http(second.clone()),
            &json!({"code":"0","data":acks}).to_string(),
            7.,
        );
        let mut out = prepared(&mut e, 7.);
        let error = pin(&out, 2, &[], &identity, true, 0);
        let elements = &error.elements.as_ref().expect("indexed cancel journal").0;
        assert_eq!(elements.len(), rows.len());
        assert!(matches!(elements[0].result, ElementResult::Unknown { .. }));
        for (element, row) in elements.iter().zip(&rows).skip(1) {
            assert!(matches!(
                &element.result,
                ElementResult::Applied {
                    effect: safe_cex_engine::write_plan::AppliedEffect::Cancel,
                    target_order_id: Some(target),
                    ..
                } if target == row[field].as_str().unwrap()
            ));
        }
        reply(
            &mut e,
            &Action::Http(second),
            &json!({"code":"0","data":[]}).to_string(),
            8.,
        );
        out.extend(prepared(&mut e, 8.));
        out.extend(fire_armed(&mut e, &out.clone(), 30_000.));
        pin(&out, 2, &[], &identity, true, 0);
    }
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/okx/okx.api.ts:22-23,66-75; packages/safe-cex/src/utils/signatureRejection.ts:19-40.
fn unknown_exception_signature_retry() {
    for status in [200, 408, 429, 503] {
        let mut e = trading_engine();
        let out = place(
            &mut e,
            vec![order(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000,"stopLoss":36000}"#,
            )],
        );
        let request = mutation(&out);
        let payload: Vec<Value> = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        let identity = payload[0]["clOrdId"].as_str().unwrap();
        e.handle(
            6.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body: r#"{"code":"59999","msg":"Invalid Sign","data":[]}"#,
                },
            },
        );
        let mut out = prepared(&mut e, 6.);
        // KNOWN-ISSUE (deferred to #3359): engine-sealed Unknown telemetry.
        let error = pin(&out, 1, &[], identity, status == 200, 0);
        let elements = error
            .elements
            .as_ref()
            .expect("signature-looking reply journal");
        assert_eq!(elements.len(), 2);
        assert!(matches!(&elements[0].result, ElementResult::Unknown { .. }));
        assert_eq!(elements[1].result, ElementResult::NotSent { error: None });
        out.extend(fire_armed(&mut e, &out.clone(), 30_000.));
        pin(&out, 1, &[], identity, status == 200, 0);
    }
}
