#[test]
fn pending_worker_error_reporting_pins_source_order_before_porting() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_error_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_str(
        &String::from_utf8(output.stdout)
            .unwrap()
            .replace("\\ud800", "SURROGATE_WITNESS"),
    )
    .unwrap();
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 72, "WORKER-ERROR source scenario inventory");
    assert_eq!(
        rows.iter()
            .map(|r| r["calls"].as_array().unwrap().len())
            .sum::<usize>(),
        360,
        "WORKER-ERROR source call inventory"
    );
}

#[test]
fn worker_error_source_controls_pin_boundaries_and_callback_counts() {
    for (flag, assertion) in [
        (
            "--strict-boundary",
            "WORKER-ERROR equality flushes suppressed count",
        ),
        ("--drop-fallback", "WORKER-ERROR fallback effect count"),
    ] {
        let output = std::process::Command::new("node")
            .args([
                concat!(env!("CARGO_MANIFEST_DIR"), "/tests/worker_error_oracle.mjs"),
                flag,
            ])
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "WORKER-ERROR source mutation survived {flag}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(assertion),
            "WORKER-ERROR source control wrong failure {flag}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
