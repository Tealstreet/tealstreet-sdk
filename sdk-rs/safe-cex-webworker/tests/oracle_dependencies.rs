use std::{path::Path, process::Command};

#[test]
fn every_source_oracle_fails_loudly_when_its_compiler_import_is_missing() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut checked = 0;
    for entry in std::fs::read_dir(&tests).unwrap() {
        let path = entry.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("_oracle.mjs")
        {
            continue;
        }
        let output = Command::new("node")
            .arg(&path)
            .env(
                "SAFE_CEX_TYPESCRIPT_PATH",
                "/__safe_cex_missing_dependency_control__/typescript",
            )
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success()
                && stderr.contains("MODULE_NOT_FOUND")
                && stderr.contains("__safe_cex_missing_dependency_control__"),
            "{} must fail at the compiler import: {stderr}",
            path.display()
        );
        checked += 1;
    }
    assert!(
        checked >= 9,
        "source oracle discovery must not silently vanish"
    );
}

#[test]
fn runtime_package_oracles_fail_loudly_when_lodash_is_missing() {
    for script in [
        "subscriptions_oracle.mjs",
        "twap_state_oracle.mjs",
        "risk_postprocessor_oracle.mjs",
        "store_notification_oracle.mjs",
    ] {
        let output = Command::new("node")
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests")
                    .join(script),
            )
            .env(
                "SAFE_CEX_WORKER_LODASH_PATH",
                "/__safe_cex_missing_dependency_control__/lodash",
            )
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success()
                && stderr.contains("MODULE_NOT_FOUND")
                && stderr.contains("__safe_cex_missing_dependency_control__/lodash"),
            "{script} must fail at its lodash import: {stderr}"
        );
    }
}
