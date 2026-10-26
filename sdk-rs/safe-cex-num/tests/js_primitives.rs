#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::js_truthy;
use serde_json::Value;
#[test]
fn truthiness_matches_live_node() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_primitives.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality::for_each(rows, 1680, "truthy", |row| {
        let value = row["raw"]
            .as_str()
            .map(|raw| serde_json::from_str::<Value>(raw).unwrap());
        assert_eq!(
            js_truthy(value.as_ref()),
            row["expected"].as_bool().unwrap(),
            "{row}"
        );
    });
}

#[test]
fn number_any_matches_live_node() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_primitives.cjs"
        ))
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality::for_each(rows, 1680, "number-any", |row| {
        let value = row["raw"]
            .as_str()
            .map(|raw| serde_json::from_str::<Value>(raw).unwrap());
        match safe_cex_num::js_number(value.as_ref()) {
            Ok(actual) => {
                let expected: f64 = row["number"]["ok"]
                    .as_str()
                    .expect("expected success")
                    .parse()
                    .unwrap();
                assert!(
                    actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
                    "{row}: {actual:?}"
                );
            }
            Err(error) => assert_eq!(
                serde_json::json!({"error":{"name":error.name,"message":error.message}}),
                row["number"],
                "{row}"
            ),
        }
    });
}
