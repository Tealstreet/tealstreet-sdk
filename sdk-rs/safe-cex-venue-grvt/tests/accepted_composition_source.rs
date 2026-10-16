#![cfg(not(target_arch = "wasm32"))]

use std::process::{Command, Output};

fn source(mode: Option<&str>) -> Output {
    let mut command = Command::new("node");
    command.arg(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/accepted-composition-source.cjs"
    ));
    if let Some(mode) = mode {
        command.arg(mode);
    }
    command.output().expect("run maintained TypeScript source")
}

#[test]
fn composition_contract_expectations_match_actual_source() {
    let output = source(None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn composition_source_boundary_controls_fail_and_restore() {
    for (mode, label) in [
        ("id-boundary", "CLIENT-ID actual source"),
        ("cache-limit", "NONCE actual source"),
        ("duplicate-validator", "unique rule properties"),
    ] {
        let output = source(Some(mode));
        assert!(!output.status.success(), "source control accepted {mode}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(label));
        assert!(source(None).status.success(), "source restore {mode}");
    }
}
