//! Pin reference hashing and actual Axios transform boundaries before porting factories.
use std::process::Command;
#[test]
fn live_api_signing_and_wire_boundaries() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/api-capture.json")).unwrap();
    assert_eq!(
        actual, expected,
        "live KuCoin hashing and Axios wire boundary changed"
    );
}

#[test]
fn missing_runtime_dependency_is_a_hard_failure() {
    let output = Command::new("node")
        .arg("--require")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/oracle-controls.cjs"
        ))
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-capture.cjs"
        ))
        .env("KUCOIN_ORACLE_MUTATION", "missing-axios")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("MODULE_NOT_FOUND")
            && stderr.contains("kucoin-intentionally-missing-axios"),
        "{stderr}"
    );
}

#[test]
fn live_api_retry_boundaries() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-retry-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/api-retry-capture.json")).unwrap();
    assert_eq!(
        actual, expected,
        "live KuCoin retry signing or write/read policy changed"
    );
}

#[test]
fn retry_source_mutation_is_rejected_by_the_live_gate() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "live_api_retry_boundaries", "--nocapture"])
        .env(
            "NODE_OPTIONS",
            concat!(
                "--require=",
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/oracle-controls.cjs"
            ),
        )
        .env("KUCOIN_ORACLE_MUTATION", "retry")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("live KuCoin retry signing or write/read policy changed")
    );
}
#[test]
fn missing_retry_dependency_is_a_hard_failure() {
    let output = Command::new("node")
        .arg("--require")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/oracle-controls.cjs"
        ))
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-retry-capture.cjs"
        ))
        .env("KUCOIN_ORACLE_MUTATION", "missing-retry")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("MODULE_NOT_FOUND")
            && stderr.contains("kucoin-intentionally-missing-retry"),
        "{stderr}"
    );
}

#[test]
fn signature_predicate_matches_actual_typescript() {
    use std::io::Write;
    let corpus = serde_json::json!([
        {"code": "400005"},
        {"code": 400005},
        {"code": "200000", "msg": "Invalid KC-API-SIGN"},
        {"msg": "invalid kc-api-sign header"},
        {"code": "400004", "msg": "Invalid KC-API-PASSPHRASE"},
        {"code": "400005 "},
        {"msg": null},
        {"msg": "INVALID KC-API-\u{17F}IGN"},
        {},
        null,
        "400005"
    ]);
    let expected = [
        true, true, true, true, false, false, false, false, false, false, false,
    ];
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/signature-rejection.cjs"
        ))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(corpus.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<bool> = serde_json::from_slice(&output.stdout).unwrap();
    let inputs = corpus.as_array().unwrap();
    assert_eq!(source.len(), inputs.len(), "source signature corpus floor");
    for ((data, rejected), source) in inputs.iter().zip(expected).zip(source) {
        assert_eq!(
            safe_cex_venue_kucoin::api::is_kucoin_signature_rejection(&data.clone().into()).unwrap(),
            rejected,
            "native signature predicate {data}"
        );
        assert_eq!(source, rejected, "source signature predicate {data}");
    }
}

#[path = "support/api_runtime.rs"]
mod api_runtime;
