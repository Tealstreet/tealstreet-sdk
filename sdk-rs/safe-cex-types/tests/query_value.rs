use safe_cex_types::QueryValue;
use serde_json::Value;

fn tagged(value: &Value) -> QueryValue {
    if value.get("$undefined").and_then(Value::as_bool) == Some(true) {
        return QueryValue::Undefined;
    }
    if let Some(number) = value.get("$number").and_then(Value::as_str) {
        return QueryValue::Number(match number {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            _ => number.parse().unwrap(),
        });
    }
    match value {
        Value::Null => QueryValue::Null,
        Value::Bool(v) => QueryValue::Bool(*v),
        Value::String(v) => QueryValue::String(v.clone()),
        Value::Array(v) => QueryValue::Array(v.iter().map(tagged).collect()),
        Value::Object(v) => {
            QueryValue::Object(v.iter().map(|(k, v)| (k.clone(), tagged(v))).collect())
        }
        Value::Number(_) => panic!("source numeric tag missing"),
    }
}
#[test]
fn actual_qs_profiles_require_non_json_values_and_distinct_array_conventions() {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/query_value.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 6);
    for row in &rows {
        let QueryValue::Object(values) = tagged(&row["input"]) else {
            panic!("query source object");
        };
        if let Some(value) = values.get("textZero") {
            assert!(
                matches!(value, QueryValue::String(text) if text == "-0"),
                "QUERY-VALUE string numeric token retains string identity"
            );
            assert!(
                matches!(values.get("textOverflow"), Some(QueryValue::String(text)) if text == "1e400")
            );
            assert!(
                matches!(values.get("zero"), Some(QueryValue::Number(value)) if value.to_bits() == (-0.0_f64).to_bits()),
                "QUERY-VALUE native signed zero bits"
            );
        }
        if let Some(QueryValue::Number(n)) = values.get("startAt") {
            assert!(n.is_nan(), "QUERY-VALUE NaN must survive before renderer");
            assert!(
                matches!(values.get("absent"), Some(QueryValue::Undefined)),
                "QUERY-VALUE undefined must remain distinct from null"
            );
            assert!(matches!(values.get("nil"), Some(QueryValue::Null)));
            assert!(
                row["output"].as_str().unwrap().contains("startAt=NaN"),
                "QUERY-VALUE source NaN query spelling"
            );
            assert!(row["output"].as_str().unwrap().contains("limit=-Infinity"));
            assert!(row["output"].as_str().unwrap().contains("endAt=Infinity"));
            assert!(!row["output"].as_str().unwrap().contains("absent"));
        }
    }
    assert!(
        rows[0]["output"]
            .as_str()
            .unwrap()
            .contains("list%5B0%5D=NaN"),
        "QUERY-VALUE source indexed arrays"
    );
    assert!(
        rows[3]["output"]
            .as_str()
            .unwrap()
            .contains("list=NaN&list=Infinity"),
        "QUERY-VALUE source repeated arrays"
    );
}
