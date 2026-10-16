// This composes live TS -> frozen snapshot drift checks with the normal Rust
// snapshot tests. --check never rewrites fixtures; no fallback masks source drift.
#[test]
#[ignore = "Runs maintained TypeScript through Node and the isolated compiler/dayjs"]
fn live_typescript_snapshot_drift() {
    for name in [
        "instrument",
        "clock",
        "fills",
        "order_book",
        "request",
        "socket",
        "terminal",
    ] {
        let output = std::process::Command::new("node")
            .arg(format!(
                "{}/tests/{name}_oracle.mjs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .arg("--check")
            .output()
            .expect("Node is required for live snapshot drift checks");
        assert!(
            output.status.success(),
            "{name}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
}
#[test]
fn json_float_boundary_roundtrips_engine_output() {
    let parsed: serde_json::Value = serde_json::from_str("106.60000000000001").unwrap();
    assert_eq!(parsed.as_f64().unwrap().to_bits(), 0x405a_a666_6666_6667);
}
