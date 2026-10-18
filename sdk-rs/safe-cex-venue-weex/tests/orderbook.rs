#[path = "support/cardinality.rs"]
mod cardinality;
#[path = "support/retained_input.rs"]
mod retained_input;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::OrderBookOrders;
use safe_cex_venue_weex::utils::*;
use serde_json::Value;
use std::{path::Path, process::Command};
fn levels(actual: &[OrderBookOrders], expected: &Value, label: &str) -> usize {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "book::{label}::length");
    let mut compared = 0;
    for (i, (native, source)) in actual.iter().zip(expected).enumerate() {
        compared += 1;
        for (field, value) in [
            ("price", native.price),
            ("amount", native.amount),
            ("total", native.total),
        ] {
            assert_eq!(
                format!("{:016x}", value.to_bits()),
                source[field]["bits"].as_str().unwrap(),
                "book::{label}::{i}::{field}"
            );
        }
    }
    compared
}
#[test]
fn books_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/book-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    cardinality::check(&corpus, "book");
    assert!(corpus["header"]["arch"].is_string(), "capture_arch");
    assert!(corpus["header"]["node"].is_string(), "capture_node");
    assert!(
        corpus["header"]["nodeMajor"].is_number(),
        "capture_node_major"
    );
    assert_eq!(
        corpus["header"]["nanBits"], "7ff8000000000000",
        "capture_nan_canonical"
    );
    let mut compared = 0;
    for case in corpus["cases"].as_array().unwrap() {
        for route in 0..4 {
            let input: Option<WeexUtilityInput> = case["raw"].as_str().map(|raw| match route {
                0 => serde_json::from_str(raw).unwrap(),
                1 => js_from_str(raw).unwrap(),
                2 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
            });
            let name = case["fn"].as_str().unwrap();
            let label = format!("{name}::{}::route_{route}", case["id"].as_str().unwrap());
            let error = match name {
                "normalizeOrderBookSide" => match normalize_order_book_side(input.as_ref()) {
                    Ok(native) => {
                        assert!(case.get("error").is_none(), "book::{label}::expected_error");
                        compared += levels(&native, &case["result"], &label);
                        None
                    }
                    Err(error) => Some(error),
                },
                "normalizeOrderBook" => match normalize_order_book(input.as_ref()) {
                    Ok(native) => {
                        assert!(case.get("error").is_none(), "book::{label}::expected_error");
                        compared += levels(
                            &native.bids,
                            &case["result"]["bids"],
                            &format!("{label}::bids"),
                        );
                        compared += levels(
                            &native.asks,
                            &case["result"]["asks"],
                            &format!("{label}::asks"),
                        );
                        None
                    }
                    Err(error) => Some(error),
                },
                _ => panic!("unknown function {name}"),
            };
            if let Some(error) = error {
                assert_eq!(
                    error.name,
                    case["error"]["name"].as_str().unwrap(),
                    "book::{label}::error_name"
                );
                assert_eq!(
                    error.message,
                    case["error"]["message"].as_str().unwrap(),
                    "book::{label}::error_message"
                );
            }
        }
    }
    assert_eq!(compared, 160, "cardinality::book::compared_levels");
    for kind in ["depth", "refusals"] {
        for case in corpus[kind].as_array().unwrap() {
            let raw = case["raw"].as_str().unwrap();
            if kind == "refusals" {
                let native = retained_input::admit_utf16(raw);
                if case["fn"] == "normalizeOrderBookSide" {
                    assert!(normalize_order_book_side(Some(&native)).unwrap().is_empty());
                } else {
                    let book = normalize_order_book(Some(&native)).unwrap();
                    assert!(book.bids.is_empty() && book.asks.is_empty());
                }
            } else {
                assert!(
                    serde_json::from_str::<WeexUtilityInput>(raw).is_err(),
                    "retained depth direct refusal"
                );
                assert!(
                    js_from_str::<WeexUtilityInput>(raw).is_err(),
                    "retained depth numeric refusal"
                );
            }
        }
    }
    let mut deep = serde_json::json!("7");
    for _ in 0..150 {
        deep = Value::Array(vec![deep]);
    }
    let side = Value::Array(vec![Value::Array(vec![serde_json::json!("5"), deep])]);
    for value in [side.clone(), serde_json::json!({"bids":side})] {
        assert!(
            js_from_value::<WeexUtilityInput>(value.clone()).is_err(),
            "book_value_depth_refusal"
        );
        assert!(
            js_from_value_ref::<WeexUtilityInput>(&value).is_err(),
            "book_value_depth_refusal"
        );
    }
    assert_eq!(corpus["callable"].as_array().unwrap().len(), 2);
    eprintln!(
        "orderbook: {} cases x 4 routes; header {}",
        corpus["cases"].as_array().unwrap().len(),
        corpus["header"]
    );
}
