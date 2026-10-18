#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::*;
use safe_cex_venue_weex::utils::*;
use serde_json::Value;
use std::{path::Path, process::Command};

#[test]
fn utility_enums_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/utils-oracle.cjs"))
        .output()
        .expect("node required");
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    cardinality::check(&corpus, "utils");
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
    for case in corpus["numbers"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        for route in 0..4 {
            let input: WeexUtilityInput = match route {
                0 => serde_json::from_str(raw).unwrap(),
                1 => js_from_str(raw).unwrap(),
                2 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
            };
            let native = input.as_value().as_f64().unwrap();
            assert_eq!(
                format!("{:016x}", native.to_bits()),
                case["native"]["bits"].as_str().unwrap(),
                "utility_numeric::{raw}::route_{route}"
            );
        }
    }
    let mut compared = 0;
    for case in corpus["outcomes"].as_array().unwrap() {
        let name = case["fn"].as_str().unwrap();
        let raw = case["raw"].as_str();
        for route in 0..4 {
            let input: Option<WeexUtilityInput> = raw.map(|raw| match route {
                0 => serde_json::from_str(raw).unwrap(),
                1 => js_from_str(raw).unwrap(),
                2 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
            });
            let label = format!("utility::{name}::{}::route_{route}", case["id"]);
            match name {
                "normalizePositionMode" => {
                    let actual = normalize_position_mode(input.as_ref());
                    if case["result"]["kind"] == "error" {
                        let error = actual.expect_err(&label);
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
                        let expected: PositionMode =
                            serde_json::from_value(case["result"]["value"].clone()).unwrap();
                        assert_eq!(actual.unwrap(), expected, "{label}");
                    }
                }
                "normalizeMarginType" => {
                    let actual = normalize_margin_type(input.as_ref());
                    if case["result"]["kind"] == "error" {
                        let error = actual.expect_err(&label);
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
                        let expected: MarginType =
                            serde_json::from_value(case["result"]["value"].clone()).unwrap();
                        assert_eq!(actual.unwrap(), expected, "{label}");
                    }
                }
                "normalizePositionSide" => {
                    let actual = normalize_position_side(input.as_ref());
                    if case["result"]["kind"] == "error" {
                        let error = actual.expect_err(&label);
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
                        let expected: PositionSide =
                            serde_json::from_value(case["result"]["value"].clone()).unwrap();
                        assert_eq!(actual.unwrap(), expected, "{label}");
                    }
                }
                "normalizeOrderSide" => {
                    let actual = normalize_order_side(input.as_ref());
                    if case["result"]["kind"] == "error" {
                        let error = actual.expect_err(&label);
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
                        let expected: Option<OrderSide> =
                            serde_json::from_value(case["result"]["value"].clone()).unwrap();
                        assert_eq!(actual.unwrap(), expected, "{label}");
                    }
                }
                "normalizeOrderStatus" => {
                    let actual = normalize_order_status(input.as_ref());
                    if case["result"]["kind"] == "error" {
                        let error = actual.expect_err(&label);
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
                        let expected: OrderStatus =
                            serde_json::from_value(case["result"]["value"].clone()).unwrap();
                        assert_eq!(actual.unwrap(), expected, "{label}");
                    }
                }
                "normalizeOrderType" => {
                    let actual = normalize_order_type(input.as_ref());
                    if case["result"]["kind"] == "error" {
                        let error = actual.expect_err(&label);
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
                        let expected: OrderType =
                            serde_json::from_value(case["result"]["value"].clone()).unwrap();
                        assert_eq!(actual.unwrap(), expected, "{label}");
                    }
                }
                "normalizeTriggerType" => {
                    let actual = normalize_trigger_type(input.as_ref());
                    if case["result"]["kind"] == "error" {
                        let error = actual.expect_err(&label);
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
                        let expected: OrderStopTriggerType =
                            serde_json::from_value(case["result"]["value"].clone()).unwrap();
                        assert_eq!(actual.unwrap(), expected, "{label}");
                    }
                }
                _ => panic!("unknown source function {name}"),
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 6384, "cardinality::utils_oracle.rs::compared");
    let mut retained_cases = 0;
    for case in corpus["refusals"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        for input in [
            serde_json::from_str::<WeexUtilityInput>(raw).unwrap(),
            js_from_str::<WeexUtilityInput>(raw).unwrap(),
        ] {
            assert_eq!(input.as_value().as_text(), Some([0xd800].as_slice()));
            match case["fn"].as_str().unwrap() {
                "normalizePositionMode" => {
                    let expected: PositionMode =
                        serde_json::from_value(case["native"].clone()).unwrap();
                    assert_eq!(normalize_position_mode(Some(&input)).unwrap(), expected);
                }
                "normalizeMarginType" => {
                    let expected: MarginType =
                        serde_json::from_value(case["native"].clone()).unwrap();
                    assert_eq!(normalize_margin_type(Some(&input)).unwrap(), expected);
                }
                "normalizePositionSide" => {
                    let expected: PositionSide =
                        serde_json::from_value(case["native"].clone()).unwrap();
                    assert_eq!(normalize_position_side(Some(&input)).unwrap(), expected);
                }
                "normalizeOrderSide" => {
                    let expected: Option<OrderSide> =
                        serde_json::from_value(case["native"].clone()).unwrap();
                    assert_eq!(normalize_order_side(Some(&input)).unwrap(), expected);
                }
                "normalizeOrderStatus" => {
                    let expected: OrderStatus =
                        serde_json::from_value(case["native"].clone()).unwrap();
                    assert_eq!(normalize_order_status(Some(&input)).unwrap(), expected);
                }
                "normalizeOrderType" => {
                    let expected: OrderType =
                        serde_json::from_value(case["native"].clone()).unwrap();
                    assert_eq!(normalize_order_type(Some(&input)).unwrap(), expected);
                }
                "normalizeTriggerType" => {
                    let expected: OrderStopTriggerType =
                        serde_json::from_value(case["native"].clone()).unwrap();
                    assert_eq!(normalize_trigger_type(Some(&input)).unwrap(), expected);
                }
                name => panic!("unknown UTF16 enum function {name}"),
            }
            retained_cases += 1;
        }
    }
    assert_eq!(retained_cases, 14, "retained UTF16 source comparisons");
    for case in corpus["depth"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        assert!(
            serde_json::from_str::<WeexUtilityInput>(raw).is_err(),
            "utility_depth_refusal"
        );
        assert!(
            js_from_str::<WeexUtilityInput>(raw).is_err(),
            "utility_depth_refusal"
        );
        let mut value = serde_json::json!("SELL");
        for _ in 0..150 {
            value = Value::Array(vec![value]);
        }
        assert!(
            js_from_value::<WeexUtilityInput>(value.clone()).is_err(),
            "utility_value_depth_refusal"
        );
        assert!(
            js_from_value_ref::<WeexUtilityInput>(&value).is_err(),
            "utility_value_depth_refusal"
        );
    }
    assert_eq!(
        corpus["callable"].as_array().unwrap().len(),
        7,
        "callable_inventory"
    );
    eprintln!(
        "utility corpus: {} cases x 7 rows x 4 ingress routes; header {}",
        corpus["cases"], corpus["header"]
    );
}
