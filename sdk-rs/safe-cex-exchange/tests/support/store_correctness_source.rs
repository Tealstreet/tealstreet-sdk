pub fn capture() -> serde_json::Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_store_correctness_source.mjs"
        ))
        .output()
        .expect("maintained DefaultStore oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("source observations")
}
