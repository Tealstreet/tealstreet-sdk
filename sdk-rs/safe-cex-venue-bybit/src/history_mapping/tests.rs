use super::*;
use crate::api::truthy;
use safe_cex_exchange::js_runtime::value::{from_json, to_typed};
use safe_cex_types::Execution;
use serde_json::Value;
fn normalize(value: &Value) -> Value {
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(normalize).collect()),
        Value::Object(m) => {
            Value::Object(m.iter().map(|(k, v)| (k.clone(), normalize(v))).collect())
        }
        _ => value.clone(),
    }
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn history_rows_match_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/history-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let globals = Js::object(vec![]);
    safe_cex_exchange::js_runtime::globals::install_builtins::<()>(&globals, std::rc::Weak::new());
    let mut typed_differences = 0;
    let mut surrogate_fee_cases = 0;
    for (index, case) in cases.iter().enumerate() {
        let rows = case["rows"].as_array().unwrap();
        let opts = &case["opts"];
        let result = map_order_history_rows_to_executions(
            &globals,
            &from_json(&case["rows"]),
            Js::str("fallback"),
            opts.get("orderId").map(from_json).unwrap_or(Js::Undefined),
            opts.get("from").map(from_json).unwrap_or(Js::Undefined),
            opts.get("to").map(from_json).unwrap_or(Js::Undefined),
        );
        if let Some(error) = case.get("error") {
            assert_eq!(
                result.unwrap_err().to_exchange_error().message,
                error.as_str().unwrap(),
                "case{index}: {case}"
            );
            continue;
        }
        let result = result.and_then(|v| to_typed::<Vec<Execution>>(&v));
        let raw = &rows[0];
        if let Some(key) = ["symbol", "feeCurrency"]
            .into_iter()
            .find(|key| raw.get(*key).is_some_and(|v| truthy(v) && !v.is_string()))
        {
            let field = if key == "symbol" {
                "symbol"
            } else {
                "commissionAsset"
            };
            assert!(
                result.is_err(),
                "typed Execution must refuse the non-string {field}"
            );
            assert!(!case["result"][0][field].is_string());
            typed_differences += 1;
            continue;
        }
        let result = result
            .unwrap_or_else(|e| panic!("case{index}: {}: {case}", e.to_exchange_error().message));
        let mut actual = serde_json::to_value(&result).unwrap();
        for (value, execution) in actual.as_array_mut().unwrap().iter_mut().zip(&result) {
            if execution.commission == Some(f64::INFINITY) {
                value["commission"] = "Infinity".into();
            }
        }
        if raw["cumFeeDetail"].as_str() == Some("{\"USDT\":\"1\",\"bad\":\"\\ud800\"}") {
            assert_eq!(case["result"][0]["commission"], 1);
            assert_eq!(actual[0]["commission"], 1.0);
            surrogate_fee_cases += 1;
        }
        assert_eq!(
            normalize(&actual),
            normalize(&case["result"]),
            "case{index}: {case}"
        );
    }
    assert_eq!(typed_differences, 16);
    assert_eq!(surrogate_fee_cases, 2);
    assert_eq!(cases.len(), 380);
}
