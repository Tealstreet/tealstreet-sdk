//! Captures precede Rust signing integration; these are independent TS/WASM answers.
use serde_json::Value;
use std::process::Command;

#[test]
#[ignore = "requires Node, maintained TypeScript, and repository Go WASM"]
fn lighter_signing_preimages_stay_pinned_to_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/capture-signing.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/signing-preimages.json")).unwrap();
    assert_eq!(
        expected["cases"].as_array().unwrap().len(),
        92,
        "signing historical source case count"
    );
    assert_eq!(
        actual["cases"].as_array().unwrap().len(),
        expected["cases"].as_array().unwrap().len(),
        "signing capture case inventory"
    );
    for (actual, expected) in actual["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(expected["cases"].as_array().unwrap())
    {
        let name = expected["name"].as_str().unwrap();
        assert_eq!(actual["name"], expected["name"], "signing case order");
        for field in [
            "outer",
            "sdkCalls",
            "sdkResults",
            "crypto",
            "wire",
            "nonceRequest",
            "result",
        ] {
            assert_eq!(actual[field], expected[field], "{name}/{field}");
        }
    }
    for field in [
        "captureHeader",
        "versions",
        "sources",
        "fixedNow",
        "secret",
        "publicKey",
        "entropyByte",
    ] {
        assert_eq!(actual[field], expected[field], "signing capture {field}");
    }
}
