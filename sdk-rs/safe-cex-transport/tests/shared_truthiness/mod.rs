use serde_json::Value;

#[test]
fn delegated_truthiness_matches_live_node() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/shared_truthiness/oracle.cjs"
        ))
        .output()
        .expect("live Node truthiness oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 1623, "truthiness corpus count");
    for row in rows {
        let raw = row["raw"].as_str().unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(
            super::js_truthy(&value),
            row["truthy"].as_bool().unwrap(),
            "delegated truthiness: {raw}"
        );
    }
}
