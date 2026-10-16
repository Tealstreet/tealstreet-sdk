use super::*;
fn class(n: f64) -> &'static str {
    if n.is_nan() {
        "NaN"
    } else if n == f64::INFINITY {
        "Infinity"
    } else if n == f64::NEG_INFINITY {
        "-Infinity"
    } else if n == 0. && n.is_sign_negative() {
        "-0"
    } else {
        "finite"
    }
}
fn normalize(v: Value) -> Value {
    match v {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalize(v))).collect()),
        v => v,
    }
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn primary_fill_mapping_preserves_prefix_and_numeric_classes() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/primary-fill-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (i, c) in cases.iter().enumerate() {
        let mut fills = vec![];
        let result = append_primary_fills(c["rows"].as_array().unwrap(), "fallback", &mut fills);
        assert_eq!(
            result.err().as_deref(),
            c.get("error").and_then(Value::as_str),
            "error case{i}: {c}"
        );
        assert_eq!(
            normalize(serde_json::to_value(&fills).unwrap()),
            normalize(c["result"].clone()),
            "fills case{i}: {c}"
        );
        let classes: Vec<_> = fills
            .iter()
            .map(|r| {
                vec![
                    class(r.timestamp),
                    class(r.price),
                    class(r.qty),
                    class(r.commission.unwrap()),
                ]
            })
            .collect();
        assert_eq!(
            serde_json::to_value(classes).unwrap(),
            c["classes"],
            "numeric classes case{i}"
        );
    }
    assert_eq!(cases.len(), 610);
}
