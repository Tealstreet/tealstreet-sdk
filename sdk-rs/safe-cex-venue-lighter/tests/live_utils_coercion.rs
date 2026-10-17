use safe_cex_exchange::js_runtime::value::{Js, Thrown, from_json, to_json};
use safe_cex_num::JsCoercionError;
use safe_cex_venue_lighter::utils::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::process::Command;

fn decode(value: &Value) -> Value {
    if value.get("$number").and_then(Value::as_str) == Some("-0") {
        return json!(-0.0);
    }
    match value {
        Value::Array(values) => Value::Array(values.iter().map(decode).collect()),
        Value::Object(values) => {
            Value::Object(values.iter().map(|(k, v)| (k.clone(), decode(v))).collect())
        }
        _ => value.clone(),
    }
}
fn encode(value: Value) -> Value {
    match value {
        Value::Number(ref n) if n.as_f64().is_some_and(|x| x == 0.0 && x.is_sign_negative()) => {
            json!({"$number":"-0"})
        }
        Value::Number(n) => {
            serde_json::from_str(&safe_cex_num::js_number_string(n.as_f64().unwrap())).unwrap()
        }
        Value::Array(values) => Value::Array(values.into_iter().map(encode).collect()),
        Value::Object(values) => {
            Value::Object(values.into_iter().map(|(k, v)| (k, encode(v))).collect())
        }
        _ => value,
    }
}
fn outcome<T: Serialize>(result: Result<T, JsCoercionError>) -> Value {
    match result {
        Ok(value) => json!({"ok":encode(serde_json::to_value(value).unwrap())}),
        Err(error) => json!({"error":{"name":error.name,"message":error.message}}),
    }
}
fn native_outcome<T: Serialize>(result: Result<T, Thrown>) -> Value {
    match result {
        Ok(value) => json!({"ok":encode(serde_json::to_value(value).unwrap())}),
        Err(error) => {
            json!({"error":{"name":error.value.get("name").unwrap().to_display(),"message":error.message()}})
        }
    }
}
fn native(value: Option<&Value>) -> Js {
    value.map_or(Js::Undefined, |value| from_json(&decode(value)))
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn lighter_utils_coercion_matches_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/utils-coercion-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (name, count) in [
        ("scalars", 456),
        ("minimums", 241),
        ("cancels", 13),
        ("reads", 872),
        ("scaleCases", 323679),
        ("decimalMarkets", 2875),
    ] {
        assert_eq!(
            oracle[name].as_array().unwrap().len(),
            count,
            "UTILS-CARDINALITY {name} source cases"
        );
    }
    for case in oracle["scalars"].as_array().unwrap() {
        let value = case.get("value").map(decode);
        let v = value.as_ref();
        for (name, actual, key) in [
            (
                "parseLighterApiKeyIndex",
                outcome(parse_lighter_api_key_index(v)),
                "index",
            ),
            (
                "toLighterApiKeyIndexString",
                outcome(to_lighter_api_key_index_string(v, None)),
                "string",
            ),
            (
                "toLighterApiKeyIndexString fallback",
                outcome(to_lighter_api_key_index_string(v, Some(""))),
                "fallback",
            ),
            (
                "parseLighterPositiveIntegerString",
                outcome(parse_lighter_positive_integer_string(v)),
                "positive",
            ),
            (
                "isLighterL1Address",
                outcome(is_lighter_l1_address(v)),
                "address",
            ),
            (
                "normalizeNonNegativeNumber",
                native_outcome(normalize_non_negative_number(&native(v))),
                "nonnegative",
            ),
        ] {
            assert_eq!(actual, case[key], "{name}: {case}");
        }
    }
    for case in oracle["minimums"].as_array().unwrap() {
        let market = native(Some(&case["market"]));
        let actual = get_lighter_order_minimums(&market);
        if let Ok(value) = &actual {
            for (field, number) in [
                ("minBaseAmount", value.min_base_amount),
                ("minQuoteAmount", value.min_quote_amount),
            ] {
                assert_eq!(
                    encode(json!(number)),
                    case["expected"]["ok"][field],
                    "getLighterOrderMinimums.{field} native identity: {case}"
                );
            }
        }
        assert_eq!(
            native_outcome(actual),
            case["expected"],
            "getLighterOrderMinimums: {case}"
        );
    }
    let cancel_cases = oracle["cancels"].as_array().unwrap();
    assert_eq!(
        cancel_cases.len(),
        13,
        "CANCEL-CARDINALITY source case count"
    );
    let source_lengths: Vec<_> = cancel_cases
        .iter()
        .map(|case| case["expected"].as_array().unwrap().len())
        .collect();
    assert_eq!(
        source_lengths.iter().sum::<usize>(),
        31,
        "CANCEL-CARDINALITY source total outputs"
    );
    assert_eq!(
        source_lengths,
        [3, 3, 3, 1, 3, 3, 3, 3, 1, 3, 3, 1, 1],
        "CANCEL-CARDINALITY source per-case outputs"
    );
    let mut compared_cancel_fields = 0;
    for case in cancel_cases {
        let markets = native(Some(&case["markets"]));
        let orders = native(Some(&case["orders"]));
        let actual = resolve_lighter_cancel_targets(&orders, &markets).unwrap();
        let rows = actual.array_values().unwrap();
        let expected = case["expected"].as_array().unwrap();
        assert_eq!(
            rows.len(),
            expected.len(),
            "resolveLighterCancelTargets inventory"
        );
        for (index, (value, source)) in rows.iter().zip(expected).enumerate() {
            compared_cancel_fields += 2;
            assert_eq!(
                String::from_utf16(value.get("orderId").unwrap().as_str().unwrap()).unwrap(),
                source["orderId"].as_str().unwrap(),
                "resolveLighterCancelTargets[{index}].orderId native identity"
            );
            assert_eq!(
                encode(json!(value.get("marketId").unwrap().as_f64().unwrap())),
                source["marketId"],
                "resolveLighterCancelTargets[{index}].marketId native identity"
            );
        }
        assert_eq!(
            encode(to_json(&actual).unwrap()),
            case["expected"],
            "resolveLighterCancelTargets: {case}"
        );
    }
    assert_eq!(
        compared_cancel_fields, 62,
        "CANCEL-CARDINALITY compared native fields"
    );
    for case in oracle["reads"].as_array().unwrap() {
        let source = case.get("source").map(decode);
        let keys: Vec<_> = case["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            native_outcome(read_number(
                &native(source.as_ref()),
                &keys,
                case.get("allowZero").map(|v| v.as_bool().unwrap())
            )),
            case["expected"],
            "readNumber {case}"
        );
    }
    for case in oracle["scaleCases"].as_array().unwrap() {
        let scale = case
            .get("scale")
            .map(|v| match v.get("$number").and_then(Value::as_str) {
                Some("NaN") => f64::NAN,
                Some("Infinity") => f64::INFINITY,
                Some("-Infinity") => f64::NEG_INFINITY,
                Some("-0") => -0.0,
                None => v.as_f64().unwrap(),
                tag => panic!("unknown tag {tag:?}"),
            });
        assert_eq!(
            outcome::<Option<f64>>(Ok(decimals_from_scale(scale))),
            json!({"ok":case["expected"]}),
            "decimalsFromScale {case}"
        );
    }
    for case in oracle["decimalMarkets"].as_array().unwrap() {
        let market = native(Some(&case["market"]));
        assert_eq!(
            native_outcome(get_lighter_amount_decimals(&market)),
            case["amount"],
            "getLighterAmountDecimals {case}"
        );
        assert_eq!(
            native_outcome(get_lighter_price_decimals(&market)),
            case["price"],
            "getLighterPriceDecimals {case}"
        );
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn cancel_source_cardinality_controls_fail_by_name() {
    for (mutation, assertion) in [
        ("empty-expected", "CANCEL-CARDINALITY source total outputs"),
        ("short-expected", "CANCEL-CARDINALITY source total outputs"),
        ("empty-scalars", "UTILS-CARDINALITY scalars source cases"),
        ("short-scalars", "UTILS-CARDINALITY scalars source cases"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lighter_utils_coercion_matches_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_UTILS_CARDINALITY_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(log.contains(assertion), "{log}");
    }
}
