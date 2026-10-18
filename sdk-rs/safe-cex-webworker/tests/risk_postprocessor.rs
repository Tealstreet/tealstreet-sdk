#![cfg(not(target_arch = "wasm32"))]
#[path = "support/capture_bits.rs"]
mod capture;
use capture::capture_bits;
use safe_cex_store::risk::calculate_orders_risk;
use safe_cex_types::Order;
use serde_json::json;

#[test]
fn shared_risk_error_is_catchable() {
    let order: Order = serde_json::from_value(json!({
        "id":"order", "symbol":"constructor", "status":"open", "type":"limit",
        "side":"sell", "price":100, "amount":1, "filled":0, "remaining":1,
        "reduceOnly":false, "lastUpdate":0
    }))
    .unwrap();
    let error = calculate_orders_risk(&[], &[], &[], &[order], None).unwrap_err();
    assert_eq!(error.name(), "TypeError");
    assert_eq!(error.key(), Some("constructor"));
}

use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_webworker::postprocessors::OrdersRiskProfilePostprocessor;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

fn install(store: &dyn Store, input: &Value) {
    store
        .update(
            StoreUpdate {
                markets: Some(serde_json::from_value(input["markets"].clone()).unwrap()),
                tickers: Some(serde_json::from_value(input["tickers"].clone()).unwrap()),
                positions: Some(serde_json::from_value(input["positions"].clone()).unwrap()),
                orders: Some(serde_json::from_value(input["orders"].clone()).unwrap()),
                ..Default::default()
            },
            true,
        )
        .unwrap();
}
fn assert_risk(actual: &Value, expected: &Value) {
    if let Some(bits) = expected.get("$bits").and_then(Value::as_str) {
        assert_eq!(
            capture_bits(actual.as_f64().unwrap()),
            u64::from_str_radix(bits, 16).unwrap(),
            "RISK-PROCESSOR pre-serialization numeric bits"
        );
        return;
    }
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            capture_bits(a.as_f64().unwrap()),
            capture_bits(b.as_f64().unwrap()),
            "RISK-PROCESSOR numeric bits"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_risk(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect::<std::collections::BTreeSet<_>>(),
                "RISK-PROCESSOR keys"
            );
            for (k, a) in a {
                assert_risk(a, &b[k]);
            }
        }
        _ => assert_eq!(actual, expected, "RISK-PROCESSOR value"),
    }
}
fn assert_native_risk(actual: &safe_cex_types::StoreData, expected: &Value) {
    for (key, profiles) in &actual.orders_risk {
        for (index, profile) in profiles.iter().enumerate() {
            for (field, value) in [
                ("orderPnl", Some(profile.order_pnl)),
                ("totalRealizedPnl", Some(profile.total_realized_pnl)),
                ("rollingUnrealizedPnl", Some(profile.rolling_unrealized_pnl)),
                ("positionSize", Some(profile.position_size)),
                ("fee", profile.fee),
            ] {
                if let Some(value) = value {
                    let bits = expected[key][index][field]["$bits"].as_str().unwrap();
                    assert_eq!(
                        capture_bits(value),
                        u64::from_str_radix(bits, 16).unwrap(),
                        "RISK-PROCESSOR native numeric bits {key}/{index}/{field}"
                    );
                }
            }
        }
    }
    assert_risk(
        &serde_json::to_value(&actual.orders_risk).unwrap(),
        expected,
    );
}
#[test]
fn actual_processor_reports_and_explicitly_clears_prior_risk() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/risk_postprocessor_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["rows"].as_array().unwrap().len(), 96);
    assert_eq!(source["controls"].as_array().unwrap().len(), 25);
    assert_eq!(source["ordering"].as_array().unwrap().len(), 3);
    let processor = OrdersRiskProfilePostprocessor;
    for row in source["rows"]
        .as_array()
        .unwrap()
        .iter()
        .chain(source["controls"].as_array().unwrap())
        .chain(source["ordering"].as_array().unwrap())
    {
        let real = DefaultStore::new();
        let store: &dyn Store = &real;
        install(store, &source["seed"]);
        let mut data = store.serialize().unwrap();
        processor
            .process(&mut data, |e| {
                Err(format!("unexpected seed failure: {}", e.message()))
            })
            .unwrap();
        assert!(
            !data.orders_risk.is_empty(),
            "RISK-PROCESSOR prior risk exists"
        );
        assert_native_risk(&data, &row["prior"]);
        let native_profile = serde_json::to_value(&data.orders_risk["prior"][0]).unwrap();
        let native_keys: Vec<_> = native_profile
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let source_keys: Vec<_> = row["prior"]["prior"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(source_keys.last(), Some(&"positionSize"));
        assert_eq!(
            native_keys, source_keys,
            "RISK-PROCESSOR source/native profile property order"
        );
        store
            .update(
                StoreUpdate {
                    orders_risk: Some(data.orders_risk.clone()),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        install(store, &row["input"]);
        let mut data = store.serialize().unwrap();
        let prior_keys: Vec<_> = data.orders_risk.keys().cloned().collect();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let captured = observed.clone();
        let unsubscribe = store.subscribe(Box::new(move |data, _| {
            captured
                .borrow_mut()
                .push(data.orders_risk.keys().cloned().collect::<Vec<_>>());
            Ok(())
        }));
        let mut events = Vec::new();
        let result = processor.process(&mut data, |error| {
            events.push(json!(["trace", prior_keys]));
            events.push(json!(["error", error.name(), error.message(), prior_keys]));
            if row["fail"] == true {
                Err(json!({"name":"TypeError","message":"report failed"}))
            } else {
                Ok(())
            }
        });
        assert_eq!(
            events,
            row["events"].as_array().unwrap().clone(),
            "RISK-PROCESSOR error visibility"
        );
        assert_eq!(
            serde_json::to_value(result.as_ref().err()).unwrap(),
            row["thrown"],
            "RISK-PROCESSOR reporting failure"
        );
        if result.is_err() {
            assert_eq!(
                data.orders_risk.keys().cloned().collect::<Vec<_>>(),
                prior_keys,
                "RISK-PROCESSOR reporting failure precedes clear"
            );
        } else if !events.is_empty() {
            assert!(
                data.orders_risk.is_empty(),
                "RISK-PROCESSOR explicit prior risk clear"
            );
        }
        assert_native_risk(&data, &row["risk"]);
        assert_eq!(
            serde_json::to_value(data.orders_risk.keys().collect::<Vec<_>>()).unwrap(),
            row["keys"]
        );
        if result.is_ok() {
            // Publish the real processor's derived output through the production Store.
            store
                .update(
                    StoreUpdate {
                        orders_risk: Some(data.orders_risk),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            assert_eq!(
                observed.borrow().len(),
                1,
                "RISK-PROCESSOR published derived state"
            );
            assert_eq!(
                serde_json::to_value(&observed.borrow()[0]).unwrap(),
                row["keys"],
                "RISK-PROCESSOR clear is visible"
            );
            if !events.is_empty() {
                assert!(
                    store.orders_risk().unwrap().is_empty(),
                    "RISK-PROCESSOR no fabricated zero rows"
                );
            }
        } else {
            assert!(observed.borrow().is_empty());
            assert_eq!(
                store
                    .orders_risk()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                prior_keys,
                "report failure precedes source clear"
            );
        }
        unsubscribe();
    }
    let cache = &source["cache"];
    assert!(cache["cached"].get("later").is_none());
    assert!(cache["fresh"].get("later").is_some());
    let real = DefaultStore::new();
    install(&real, &cache["input"]);
    let mut data = Store::serialize(&real).unwrap();
    processor.process(&mut data, |e| Err(e.message())).unwrap();
    assert_native_risk(&data, &cache["fresh"]);
}
