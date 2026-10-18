//! Already-gone cancellations through the public venue, signed REST host and real Store.
#[allow(dead_code)]
#[path = "support/native_runtime.rs"]
mod support;

use safe_cex_exchange::exchange::Exchange;
use safe_cex_num::numeric_ingress::js_from_value;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::Order;
use serde_json::{Value, json};
use std::{
    future::Future,
    process::Command,
    task::{Context, Poll, Waker},
};

fn drive<F: Future>(runtime: &support::Runtime, future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    for _ in 0..64 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.now.set(runtime.now.get() + 1_000.0);
        runtime.pump();
    }
    panic!("public cancellation did not settle");
}

#[test]
fn already_gone_normal_ids_use_store_property_keys_while_algo_roots_stay_strict() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_rest_oracle.cjs"
        ))
        .arg("goneIdLookup")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8, "actual-source cancellation case floor");
    let expected = [
        vec!["1234"],
        vec!["1234"],
        vec!["true4"],
        vec!["1234"],
        vec!["[object Object]4"],
        vec!["0", "04"],
        vec!["123__stop_loss", "1234__stop_loss"],
        vec!["1234__stop_loss"],
    ];
    for (case, expected) in cases.iter().zip(expected) {
        let label = case["label"].as_str().unwrap();
        let source_ids: Vec<_> = case["orders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|order| order["id"].as_str().unwrap())
            .collect();
        assert_eq!(source_ids, expected, "maintained source {label}");
        let input: Value = serde_json::from_str(case["inputRaw"].as_str().unwrap()).unwrap();
        let orders: Vec<Order> = js_from_value(input["orders"].clone()).unwrap();
        let (venue, runtime, store, wire) = support::fixture();
        Store::update(
            &*store,
            StoreUpdate {
                markets: Some(js_from_value(json!([source["market"]])).unwrap()),
                orders: Some(orders),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let endpoint = if label.starts_with("algo-") {
            "/api/v1/trade/cancel-tpsl"
        } else {
            "/api/v1/trade/cancel-batch-orders"
        };
        wire.bodies
            .borrow_mut()
            .insert(endpoint.into(), input["responses"][0].clone());
        let cancel: Vec<Order> = js_from_value(input["args"][0].clone()).unwrap();
        let result = drive(&runtime, venue.cancel_orders(cancel));
        assert_eq!(
            result.is_err(),
            case["result"].get("error").is_some(),
            "outcome {label}: {result:?}"
        );
        let actual: Vec<_> = store
            .orders()
            .unwrap()
            .iter()
            .map(|order| order.id.clone())
            .collect();
        assert_eq!(actual, expected, "production Store reconciliation {label}");
        let requests = wire.requests.borrow();
        assert_eq!(requests.len(), 1, "one signed cancellation {label}");
        assert_eq!(support::path(&requests[0].url), endpoint, "route {label}");
        venue.dispose().unwrap();
    }
}
