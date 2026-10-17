use super::*;
use safe_cex_exchange::js_runtime::value::from_json;
use serde_json::Value;
fn encode(value: f64) -> Value {
    if value.is_nan() {
        Value::from("NaN")
    } else if value == f64::INFINITY {
        Value::from("Infinity")
    } else if value == f64::NEG_INFINITY {
        Value::from("-Infinity")
    } else if value == 0. && value.is_sign_negative() {
        Value::from("-0")
    } else {
        serde_json::json!(value)
    }
}
fn candle(value: &Candle) -> Value {
    serde_json::json!({"timestamp":encode(value.timestamp),"open":encode(value.open),"high":encode(value.high),"low":encode(value.low),"close":encode(value.close),"volume":encode(value.volume)})
}
fn numeric_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|w| numeric_equal(v, w)))
        }
        _ => left == right,
    }
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn kline_state_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/candle-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let source_total: usize = cases
        .iter()
        .map(|case| case["outcomes"].as_array().unwrap().len())
        .sum();
    assert_eq!(source_total, 2172, "CANDLE-CARDINALITY source total");
    let mut native_total = 0;
    let mut comparisons = 0;
    for (index, case) in cases.iter().enumerate() {
        let mut state = None;
        let inputs = case["rows"].as_array().unwrap();
        let expected = case["outcomes"].as_array().unwrap();
        assert_eq!(
            inputs.len(),
            expected.len(),
            "CANDLE-CARDINALITY source case {index}"
        );
        let mut native = Vec::new();
        for raw in inputs {
            let result = apply_kline(&mut state, &from_json(raw));
            let mut actual = serde_json::Map::new();
            match result {
                Ok(Some(value)) => {
                    actual.insert("emitted".into(), candle(&value));
                }
                Ok(None) => {}
                Err(error) => {
                    actual.insert("error".into(), error.message().into());
                }
            }
            if let Some(value) = &state {
                actual.insert("state".into(), candle(value));
            }
            let actual = Value::Object(actual);
            native.push(actual);
        }
        assert_eq!(
            native.len(),
            expected.len(),
            "CANDLE-CARDINALITY expected/native case {index}"
        );
        native_total += native.len();
        for (step, (actual, expected)) in native.iter().zip(expected).enumerate() {
            comparisons += 1;
            assert!(
                numeric_equal(actual, expected),
                "case{index} step{step}: actual:{actual} expected:{expected}"
            );
        }
    }
    assert_eq!(comparisons, 2172, "CANDLE-CARDINALITY comparisons");
    assert_eq!(native_total, 2172, "CANDLE-CARDINALITY native total");
    assert_eq!(cases.len(), 1236);
}
