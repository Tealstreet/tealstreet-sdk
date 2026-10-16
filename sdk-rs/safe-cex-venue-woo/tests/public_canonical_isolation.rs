#[test]
fn canonical_oracle_isolated_dependencies_and_in_memory_control() {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-canonical-isolation.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "canonical isolated oracle and controls: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("four child probes")
    );
}
