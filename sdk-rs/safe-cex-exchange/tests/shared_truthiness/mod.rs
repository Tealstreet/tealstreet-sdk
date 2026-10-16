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
            super::truthy(&value),
            row["truthy"].as_bool().unwrap(),
            "delegated truthiness: {raw}"
        );
        assert_eq!(
            row["network"].as_array().unwrap().len(),
            3,
            "TRUTHINESS-CARDINALITY source network branches"
        );
        for (index, key) in ["isAxiosError", "response", "code"].into_iter().enumerate() {
            let mut error = serde_json::json!({"isAxiosError":true,"code":"ERR_NETWORK","message":"Network Error"});
            error[key] = value.clone();
            assert_eq!(
                super::is_network_error(&error),
                row["network"][index].as_bool().unwrap(),
                "network branch {key}: {raw}"
            );
        }
    }
}
