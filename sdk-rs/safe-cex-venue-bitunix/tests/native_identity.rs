//! Concrete native request/format/batch boundaries, including malformed venue rows.
#[allow(dead_code)]
#[path = "support/venue_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn drive<F: Future>(rt: &support::Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
        rt.now.set(rt.now.get() + 1000.0);
        rt.pump();
    }
    panic!("scripted placement remained pending");
}
fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/native_reply_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn order() -> safe_cex_types::PlaceOrderOpts {
    serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}),
    )
    .unwrap()
}
#[test]
fn native_nonce_client_id_and_fallback_match_source_uuid() {
    let expected = source()["uuid"].as_str().unwrap().to_owned();
    assert_eq!(expected, "fixturenonce");
    let (venue, rt, _, wire) = support::fixture_with(false);
    *rt.id.borrow_mut() = Some("fixture-_nonce".into());
    wire.bodies.lock().unwrap().insert(
        "/api/v1/futures/trade/place_order".into(),
        json!({"code":0,"data":{}}),
    );
    let ids = drive(&rt, venue.base().place_order(venue.as_ref(), order())).unwrap();
    assert_eq!(
        ids,
        vec![format!("ts_{expected}")],
        "native generated client ID must use utils/uuid"
    );
    let requests = wire.requests.lock().unwrap();
    let request = requests
        .iter()
        .find(|r| r.url.ends_with("/place_order"))
        .unwrap();
    assert_eq!(
        request.headers["nonce"], expected,
        "native REST nonce must strip both separators"
    );
    let body: Value = match request.body.as_ref().unwrap() {
        safe_cex_transport::RestBody::Untransformed(v) => v.clone(),
        safe_cex_transport::RestBody::Transformed(t) => match t.value() {
            Value::String(s) => serde_json::from_str(s).unwrap(),
            v => v.clone(),
        },
        safe_cex_transport::RestBody::Encoded(b) => serde_json::from_slice(b).unwrap(),
    };
    assert_eq!(body["clientId"], format!("ts_{expected}"));
    drop(requests);
    wire.bodies.lock().unwrap().insert(
        "/api/v1/futures/trade/batch_order".into(),
        json!({"code":0,"data":{"successList":[{}],"failureList":[]}}),
    );
    let ids = drive(
        &rt,
        venue
            .base()
            .place_orders(venue.as_ref(), vec![order(), order()]),
    )
    .unwrap();
    assert_eq!(
        ids,
        vec![expected],
        "batch missing-ID fallback uses utils/uuid"
    );
    venue.dispose().unwrap();
}
#[test]
fn null_batch_rows_match_actual_source_errors_prefix_and_next_chunk() {
    for case in source()["cases"].as_array().unwrap() {
        let (venue, rt, _, wire) = support::fixture_with(false);
        *rt.id.borrow_mut() = Some("fixture-_nonce".into());
        wire.bodies.lock().unwrap().insert(
            "/api/v1/futures/trade/batch_order".into(),
            json!({"code":0,"data":case["reply"]}),
        );
        let errors = Rc::new(RefCell::new(vec![]));
        let sink = errors.clone();
        venue.base().on(
            "error",
            Rc::new(move |_, v| {
                sink.borrow_mut().push(v.as_str().unwrap().to_owned());
                Ok(())
            }),
        );
        let ids = drive(
            &rt,
            venue
                .base()
                .place_orders(venue.as_ref(), (0..7).map(|_| order()).collect()),
        )
        .unwrap();
        assert_eq!(
            json!(ids),
            case["ids"],
            "malformed batch must retain only source IDs: {case}"
        );
        assert_eq!(
            json!(
                errors
                    .borrow()
                    .iter()
                    .map(|s| s.strip_prefix("Exchange Error: ").unwrap())
                    .collect::<Vec<_>>()
            ),
            case["errors"],
            "exact caught TypeError/chunk behavior: {case}"
        );
        assert_eq!(
            wire.requests
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.url.ends_with("/batch_order"))
                .count(),
            case["calls"].as_u64().unwrap() as usize,
            "later chunks still run"
        );
        venue.dispose().unwrap();
    }
}

#[test]
fn native_nonfinite_placement_amounts_are_visibly_refused_before_http() {
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order-operations-oracle.cjs"
        ))
        .arg("nativePlacementNumbers")
        .arg(support::MARKET)
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "source: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for ((amount, source_qty), source) in [
        (f64::NAN, Some("NaN")),
        (f64::INFINITY, None),
        (f64::NEG_INFINITY, Some("-Infinity")),
    ]
    .into_iter()
    .zip(rows)
    {
        // Source behavior is retained as evidence of the accepted strict-input difference.
        let payloads = source["result"]["ok"].as_array().unwrap();
        assert_eq!(payloads.len(), usize::from(source_qty.is_some()));
        if let Some(qty) = source_qty {
            let observed = &payloads[0]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|pair| pair[0] == "qty")
                .unwrap()[1];
            assert_eq!(
                observed["units"],
                json!(qty.encode_utf16().collect::<Vec<_>>())
            );
        }
        for batch in [false, true] {
            let (venue, rt, _, wire) = support::fixture_with(false);
            let errors = Rc::new(RefCell::new(Vec::new()));
            let captured = errors.clone();
            venue.base().on(
                "error",
                Rc::new(move |_, value: &Value| {
                    captured.borrow_mut().push(value.clone());
                    Ok(())
                }),
            );
            let mut opts = order();
            opts.amount = amount;
            let result = if batch {
                drive(
                    &rt,
                    venue
                        .base()
                        .place_orders(venue.as_ref(), vec![order(), opts]),
                )
            } else {
                drive(&rt, venue.base().place_order(venue.as_ref(), opts))
            };
            let error = result.expect_err("strict nonfinite placement amount must be refused");
            let message = format!(
                "Bitunix placement amount must be finite (received {})",
                safe_cex_num::js_number_string(amount)
            );
            assert_eq!(
                error.name, "TypeError",
                "field refusal must not leak a JSON decode error: amount={amount}, batch={batch}, {error:?}"
            );
            assert_eq!(error.message, message);
            assert_eq!(
                errors.borrow().len(),
                1,
                "one visible field refusal per call"
            );
            assert!(errors.borrow()[0].as_str().unwrap().contains(&message));
            assert!(
                wire.requests.lock().unwrap().is_empty(),
                "batch preflight must refuse before any hydration or order HTTP"
            );
            venue.dispose().unwrap();
        }
    }
}

#[test]
fn attached_amendment_recreates_only_after_matching_refusal_and_successful_cancel() {
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_types::{Order, UpdateOrderOpts};
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order-operations-oracle.cjs"
        ))
        .arg("attachedAmendmentRecovery")
        .arg(support::MARKET)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8, "actual-source fallback controls");
    for case in cases {
        let label = case["label"].as_str().unwrap();
        if label == "tp" || label == "sl" {
            assert_eq!(case["result"]["ids"], json!([format!("NEW__{label}")]));
            assert_eq!(case["calls"].as_array().unwrap().len(), 3);
            assert_eq!(case["calls"][1]["lane"], "limited");
            assert_eq!(case["calls"][2]["payload"]["tpOrderType"], "MARKET");
            assert_eq!(case["calls"][2]["payload"]["slOrderType"], "MARKET");
        } else {
            assert!(
                case["result"]["error"].is_string(),
                "source refusal {label}"
            );
        }
        let (venue, rt, store, wire) = support::fixture_with(false);
        let orders: Vec<Order> = serde_json::from_value(case["input"]["orders"].clone()).unwrap();
        Store::update(
            &*store,
            StoreUpdate {
                orders: Some(orders),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        for (path, body) in [
            ("/api/v1/futures/trade/modify_order", &case["modify"]),
            ("/api/v1/futures/trade/cancel_orders", &case["cancel"]),
            ("/api/v1/futures/trade/place_order", &case["placement"]),
        ] {
            wire.bodies
                .lock()
                .unwrap()
                .insert(path.into(), body.clone());
        }
        let errors = Rc::new(RefCell::new(Vec::new()));
        let sink = errors.clone();
        venue.base().on(
            "error",
            Rc::new(move |_, v| {
                sink.borrow_mut().push(v.as_str().unwrap().to_owned());
                Ok(())
            }),
        );
        let options: UpdateOrderOpts = serde_json::from_value(
            json!({"order":case["input"]["order"],"update":case["input"]["update"]}),
        )
        .unwrap();
        let actual = drive(&rt, venue.update_order(options));
        if let Some(ids) = case["result"].get("ids") {
            assert_eq!(
                json!(actual.expect("attached TP/SL replacement must succeed")),
                *ids,
                "{label}"
            );
        } else {
            assert_eq!(
                actual.unwrap_err().message,
                case["result"]["error"],
                "{label}"
            );
        }
        let expected_calls: Vec<Value> = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| json!({"endpoint":c["endpoint"],"payload":c["payload"]}))
            .collect();
        let requests = wire.requests.lock().unwrap();
        let actual_calls: Vec<Value> = requests
            .iter()
            .map(|r| {
                let body = match r.body.as_ref().unwrap() {
                    safe_cex_transport::RestBody::Untransformed(v) => v.clone(),
                    safe_cex_transport::RestBody::Transformed(t) => match t.value() {
                        Value::String(s) => serde_json::from_str(s).unwrap(),
                        v => v.clone(),
                    },
                    safe_cex_transport::RestBody::Encoded(b) => serde_json::from_slice(b).unwrap(),
                };
                json!({"endpoint":r.url,"payload":body})
            })
            .collect();
        assert_eq!(
            actual_calls, expected_calls,
            "exact modify/cancel/replacement ordering and both retained protection legs: {label}"
        );
        let statuses = |orders: &Value| {
            orders
                .as_array()
                .unwrap()
                .iter()
                .map(|o| json!([o["id"], o["status"]]))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            statuses(&serde_json::to_value(&*store.orders()).unwrap()),
            statuses(&case["orders"]),
            "cancel only the exact parent and children; neighbor survives: {label}"
        );
        let expected_errors: Vec<&str> = case["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            errors
                .borrow()
                .iter()
                .map(|v| v.strip_prefix("Exchange Error: ").unwrap())
                .collect::<Vec<_>>(),
            expected_errors,
            "visible original/fallback refusals: {label}"
        );
        drop(requests);
        venue.dispose().unwrap();
    }
}
