use super::*;
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
    let mut typed_differences = 0;
    let mut surrogate_differences = 0;
    for (index, case) in cases.iter().enumerate() {
        let rows = case["rows"].as_array().unwrap();
        let opts = &case["opts"];
        let result = map_order_history_rows_to_executions(
            rows,
            "fallback",
            opts["orderId"].as_str(),
            opts["from"].as_f64(),
            opts["to"].as_f64(),
        );
        if let Some(error) = case.get("error") {
            assert_eq!(
                result.unwrap_err(),
                error.as_str().unwrap(),
                "case{index}: {case}"
            );
            continue;
        }
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
            assert_eq!(
                result.unwrap_err(),
                format!("Bybit typed Execution requires string {field}")
            );
            assert!(!case["result"][0][field].is_string());
            typed_differences += 1;
            continue;
        }
        let result = result.unwrap_or_else(|e| panic!("case{index}: {e}: {case}"));
        let mut actual = serde_json::to_value(&result).unwrap();
        for (value, execution) in actual.as_array_mut().unwrap().iter_mut().zip(&result) {
            if execution.commission == Some(f64::INFINITY) {
                value["commission"] = "Infinity".into();
            }
        }
        if raw["cumFeeDetail"].as_str() == Some("{\"USDT\":\"1\",\"bad\":\"\\ud800\"}") {
            assert_eq!(case["result"][0]["commission"], 1);
            assert!(actual[0].get("commission").is_none());
            actual[0]["commission"] = serde_json::json!(1);
            surrogate_differences += 1;
        }
        assert_eq!(
            normalize(&actual),
            normalize(&case["result"]),
            "case{index}: {case}"
        );
    }
    assert_eq!(typed_differences, 16);
    assert_eq!(surrogate_differences, 2);
    assert_eq!(cases.len(), 380);
}
