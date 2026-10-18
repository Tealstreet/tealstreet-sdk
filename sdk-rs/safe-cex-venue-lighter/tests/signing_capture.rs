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
    let mut expected: Value =
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
    // The archived 92-case capture is immutable. Re-measure its only changed
    // dependency here: the shared type declarations gained option metadata.
    // Every signed payload, preimage, output and wire request above is still
    // compared against the original capture before accepting this provenance.
    assert_eq!(
        expected["sources"]["packages/safe-cex/src/types.ts"],
        "80a41622a9e4ffa00356a84e16cedf2a26fe5896a61033f1679e22e714588314"
    );
    expected["sources"]["packages/safe-cex/src/types.ts"] =
        Value::String("3b04c8e33f14ab717d803d4eec76cd20cb304c1b810cac59cfd85e685fcfbbb6".into());
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
