#[path = "support/cardinality.rs"]
mod cardinality;
#[path = "support/retained_input.rs"]
mod retained_input;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_venue_weex::utils::*;
use serde_json::Value;
use std::{path::Path, process::Command};
#[test]
fn timestamps_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/timestamp-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    cardinality::check(&corpus, "timestamps");
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
            let offset = case["offset"].as_f64().unwrap();
            let zone = case["zone"].as_str().unwrap();
            let name = case["fn"].as_str().unwrap();
            let id = case["id"].as_str().unwrap();
            let result = match name {
                "normalizeTimestampToMs" => {
                    normalize_timestamp_to_ms(input.as_ref(), |local| local + offset)
                }
                "normalizeTimestampToSeconds" => {
                    normalize_timestamp_to_seconds(input.as_ref(), |local| local + offset)
                }
                _ => panic!("unknown {name}"),
            };
            let label = format!("timestamp::{name}::{id}::{zone}::route_{route}");
            if case["result"]["type"] == "error" {
                let error = result.unwrap_err();
                assert_eq!(
                    error.name,
                    case["result"]["name"].as_str().unwrap(),
                    "{label}"
                );
                assert_eq!(
                    error.message,
                    case["result"]["message"].as_str().unwrap(),
                    "{label}"
                );
            } else {
                let native = result.unwrap();
                assert_eq!(
                    format!("{:016x}", native.to_bits()),
                    case["result"]["bits"].as_str().unwrap(),
                    "{label}"
                );
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 1176, "cardinality::timestamps.rs::compared");
    for kind in ["depth", "refusals"] {
        for case in corpus[kind].as_array().unwrap() {
            let raw = case["raw"].as_str().unwrap();
            if kind == "refusals" {
                let native = retained_input::admit_utf16(raw);
                let number = if case["fn"] == "normalizeTimestampToMs" {
                    normalize_timestamp_to_ms(Some(&native), |value| value)
                } else {
                    normalize_timestamp_to_seconds(Some(&native), |value| value)
                }
                .unwrap();
                assert_eq!(
                    format!("{:016x}", number.to_bits()),
                    case["native"]["bits"].as_str().unwrap()
                );
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
    let mut deep = serde_json::json!("1700000000.875");
    for _ in 0..150 {
        deep = Value::Array(vec![deep]);
    }
    assert!(
        js_from_value::<WeexUtilityInput>(deep.clone()).is_err(),
        "timestamp_value_depth_refusal"
    );
    assert!(
        js_from_value_ref::<WeexUtilityInput>(&deep).is_err(),
        "timestamp_value_depth_refusal"
    );
    assert_eq!(corpus["callable"].as_array().unwrap().len(), 2);
    eprintln!(
        "timestamp cases {} x 4 routes; header {}",
        corpus["cases"].as_array().unwrap().len(),
        corpus["header"]
    );
}
