#![cfg(not(target_arch = "wasm32"))]

use std::process::{Command, Output};

fn source(mode: Option<&str>) -> Output {
    let mut command = Command::new("node");
    command.arg(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/accepted-value-source.cjs"
    ));
    if let Some(mode) = mode {
        command.arg(mode);
    }
    command
        .output()
        .expect("run independent language and actual source witnesses")
}

#[test]
fn value_contract_expectations_match_language_and_actual_source() {
    let output = source(None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn value_source_controls_fail_and_restore() {
    for (mode, label) in [
        ("default-nullish", "SCALAR language operator"),
        ("cancel-prefix", "CANCEL actual maintained catch"),
    ] {
        let output = source(Some(mode));
        assert!(!output.status.success(), "source control accepted {mode}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(label));
        assert!(source(None).status.success(), "source restore {mode}");
    }
}
