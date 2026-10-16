use safe_cex_venue_coinbase::json_key_scanner::{JsonDuplicateKeyScanner, JsonKeyScanResult};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::process::Command;
const CORPUS: &[u8] = include_bytes!("fixtures/json-key-scanner.json");
fn result(value: &Value) -> JsonKeyScanResult {
    match value.as_str().unwrap() {
        "valid" => JsonKeyScanResult::Valid,
        "invalid" => JsonKeyScanResult::Invalid,
        "duplicate" => JsonKeyScanResult::Duplicate,
        _ => panic!("unknown source result"),
    }
}
#[test]
fn grammar_utf16_duplicate_precedence_and_scanner_reuse_match_live_source() {
    assert_eq!(
        format!("{:x}", Sha256::digest(CORPUS)),
        "d4c6435065966acf99adb2adea012e1841a5850d5a6e5cec22057ae04cf833e1",
        "independent scanner corpus content"
    );
    let cases: Value = serde_json::from_slice(CORPUS).unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 75, "independent scanner input floor");
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/json-key-scanner.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live scanner oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected = expected.as_array().unwrap();
    assert_eq!(expected.len(), 75, "independent scanner oracle floor");
    for (case, expected) in cases.iter().zip(expected) {
        let units: Vec<u16> = case["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
            .collect();
        let mut scanner = JsonDuplicateKeyScanner::new(&units);
        assert_eq!(
            scanner.scan(),
            result(&expected["first"]),
            "decoded native key scan result: {}",
            case["label"]
        );
        assert_eq!(
            scanner.scan(),
            result(&expected["second"]),
            "native scanner reuse result: {}",
            case["label"]
        );
        assert_eq!(
            case["label"], expected["label"],
            "native oracle case pairing"
        );
    }
}
