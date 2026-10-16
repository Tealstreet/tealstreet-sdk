use safe_cex_venue_mexc::exchange_errors::*;
use serde_json::{Value, json};
#[test]
fn mexc_exchange_errors_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/errors_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 132);
    for (i, case) in cases.iter().enumerate() {
        let result = match case["name"].as_str().unwrap() {
            "assertCommonSuccess" => {
                assert_common_success(case.get("payload"), case["action"].as_str().unwrap())
            }
            "assertRegularCancelSucceeded" => assert_regular_cancel_succeeded(case.get("payload")),
            name => panic!("unexpected {name}"),
        };
        match result {
            Ok(()) => assert_eq!(case["ok"], true, "case {i}: {case}"),
            Err(e) => assert_eq!(
                json!({"name":e.name,"message":e.message}),
                case["error"],
                "case {i}: {case}"
            ),
        }
    }
}
#[test]
fn mexc_exchange_errors_typescript_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_exchange_errors_live_typescript",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "success-guard")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_exchange_errors_live_typescript ... FAILED")
            && text.contains("assertCommonSuccess")
            && text.contains("assertion `left == right` failed"),
        "{text}"
    );
}
