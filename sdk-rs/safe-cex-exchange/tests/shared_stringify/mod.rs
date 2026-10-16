use serde_json::Value;
#[test]
fn shared_websocket_stringify_matches_live_node() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/shared_stringify/oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 1619, "wire stringify corpus");
    for row in rows {
        let raw = row["raw"].as_str().unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(
            super::stringify_json(&value),
            row["json"].as_str().unwrap(),
            "wire JSON projection {raw}"
        );
        assert_eq!(
            super::stringify(&value),
            row["wire"].as_str().unwrap(),
            "string bypass {raw}"
        );
    }
}
