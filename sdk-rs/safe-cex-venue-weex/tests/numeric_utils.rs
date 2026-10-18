#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_venue_weex::utils::*;
use serde_json::Value;
use std::{path::Path, process::Command};
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
#[test]
fn numeric_utilities_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/numeric-utils-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    cardinality::check(&corpus, "numeric");
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
        let name = case["fn"].as_str().unwrap();
        let args: Vec<f64> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| f64::from_bits(u64::from_str_radix(a["bits"].as_str().unwrap(), 16).unwrap()))
            .collect();
        for route in 0..4 {
            let input: Option<WeexUtilityInput> = case["raw"].as_str().map(|raw| match route {
                0 => serde_json::from_str(raw).unwrap(),
                1 => js_from_str(raw).unwrap(),
                2 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
            });
            let label = format!("numeric_utility::{name}::{}::route_{route}", case["id"]);
            if name == "toIntegerString" {
                let result = to_integer_string(input.as_ref());
                if case["result"]["type"] == "error" {
                    let e = result.unwrap_err();
                    assert_eq!(e.name, case["result"]["name"].as_str().unwrap(), "{label}");
                    assert_eq!(
                        e.message,
                        case["result"]["message"].as_str().unwrap(),
                        "{label}"
                    );
                } else {
                    assert_eq!(
                        result.unwrap(),
                        case["result"]["value"].as_str().unwrap(),
                        "{label}"
                    );
                }
            } else {
                let result = match name {
                    "toNumber" => to_number(input.as_ref(), args.first().copied()),
                    "normalizePercentage" => normalize_percentage(input.as_ref(), args[0], args[1]),
                    _ => panic!("unknown {name}"),
                };
                if case["result"]["type"] == "error" {
                    let e = result.unwrap_err();
                    assert_eq!(e.name, case["result"]["name"].as_str().unwrap(), "{label}");
                    assert_eq!(
                        e.message,
                        case["result"]["message"].as_str().unwrap(),
                        "{label}"
                    );
                } else {
                    assert_eq!(
                        bits(result.unwrap()),
                        case["result"]["bits"].as_str().unwrap(),
                        "{label}"
                    );
                }
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 2132, "cardinality::numeric_utils.rs::compared");
    let mut retained_cases = 0;
    for case in corpus["refusals"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        for input in [
            serde_json::from_str::<WeexUtilityInput>(raw).unwrap(),
            js_from_str::<WeexUtilityInput>(raw).unwrap(),
        ] {
            assert_eq!(input.as_value().as_text(), Some([0xd800].as_slice()));
            match case["fn"].as_str().unwrap() {
                "toNumber" => assert_eq!(
                    bits(to_number(Some(&input), Some(131.0)).unwrap()),
                    case["native"]["bits"].as_str().unwrap()
                ),
                "normalizePercentage" => assert_eq!(
                    bits(normalize_percentage(Some(&input), 131.0, 100.0).unwrap()),
                    case["native"]["bits"].as_str().unwrap()
                ),
                "toIntegerString" => {
                    assert_eq!(case["native"]["utf16"], serde_json::json!([0xd800]));
                    let error = to_integer_string(Some(&input)).unwrap_err();
                    assert_eq!(error.name, "TypeError", "closed String output refusal");
                    assert!(!error.message.is_empty());
                }
                name => panic!("unknown UTF16 numeric function {name}"),
            }
            retained_cases += 1;
        }
    }
    assert_eq!(retained_cases, 6, "retained UTF16 source comparisons");
    for case in corpus["depth"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        assert!(
            serde_json::from_str::<WeexUtilityInput>(raw).is_err(),
            "numeric_utility_depth_refusal"
        );
        assert!(
            js_from_str::<WeexUtilityInput>(raw).is_err(),
            "numeric_utility_depth_refusal"
        );
        let mut value = serde_json::json!(".25");
        for _ in 0..150 {
            value = Value::Array(vec![value]);
        }
        assert!(
            js_from_value::<WeexUtilityInput>(value.clone()).is_err(),
            "numeric_utility_value_depth_refusal"
        );
        assert!(
            js_from_value_ref::<WeexUtilityInput>(&value).is_err(),
            "numeric_utility_value_depth_refusal"
        );
    }
    assert_eq!(corpus["callable"].as_array().unwrap().len(), 3);
    eprintln!(
        "numeric utilities: {} cases x 4 routes; header {}",
        corpus["cases"].as_array().unwrap().len(),
        corpus["header"]
    );
}
