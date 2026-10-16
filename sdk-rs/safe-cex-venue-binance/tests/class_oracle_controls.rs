use std::process::Command;

#[test]
fn actual_source_controls_fail_at_the_named_assertion() {
    let root = env!("CARGO_MANIFEST_DIR");
    let controls = [
        (
            "dispatch",
            "class-boundary-findings.mjs",
            "actual source escaped surrogate invokes dispatch callback once",
        ),
        (
            "alias",
            "class-boundary-findings.mjs",
            "actual source same-generation alias receives second price 12.3 rather than 4.2",
        ),
        (
            "private-open",
            "class-boundary-findings.mjs",
            "actual synchronous onOpen preserves original thrown failure",
        ),
        (
            "exchange-client",
            "exchange-classes.mjs",
            "actual PM FAPI client remains primary-only FAPI factory",
        ),
    ];
    for (control, script, assertion) in controls {
        let output = Command::new("node")
            .arg(format!("{root}/scripts/{script}"))
            .env("SAFE_CEX_CLASS_SOURCE_CONTROL", control)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "actual source control must assert-fail: {control}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("AssertionError") && stderr.contains(assertion),
            "actual source control must reach named assertion {assertion}: {stderr}"
        );
    }
}
