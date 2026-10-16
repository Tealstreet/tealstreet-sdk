//! Frozen source-reference drift checks complement the Rust mapper comparisons.
use serde_json::Value;

#[test]
#[ignore = "requires Node and TypeScript"]
fn live_ts_history_reference() {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/history-capture.cjs"
        ))
        .output()
        .expect("execute Node");
    assert!(
        result.status.success(),
        "history capture failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let actual: Value = serde_json::from_slice(&result.stdout).unwrap();
    let expected: Value = serde_json::from_str(include_str!("history-reference.json")).unwrap();
    assert_eq!(
        actual.as_object().unwrap().keys().collect::<Vec<_>>(),
        expected.as_object().unwrap().keys().collect::<Vec<_>>(),
        "TS history reference group inventory"
    );
    for (name, expected) in expected.as_object().unwrap() {
        let actual = actual[name].as_array().unwrap();
        let expected = expected.as_array().unwrap();
        assert_eq!(
            actual.len(),
            expected.len(),
            "TS history reference {name} inventory"
        );
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(actual, expected, "TS history reference {name} case {index}");
        }
    }
}
