use safe_cex_venue_woo::orders::is_algo_order;
use serde_json::Value;
use std::process::Command;

#[test]
fn algo_classification_matches_source_native_booleans() {
    let output = Command::new("node")
        .arg(format!(
            "{}/scripts/order-policy-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    // Fixed declaration inventory plus ten independently selected strict-equality
    // inputs; the oracle's length does not define this enumeration floor.
    let expected = [
        false, false, false, true, true, true, false, false, false, false, false, false, false,
        false, false, false,
    ];
    assert_eq!(cases.len(), 16, "POLICY independent native corpus count");
    for (index, result) in expected.into_iter().enumerate() {
        let case = &cases[index];
        assert_eq!(
            is_algo_order(case.get("input")),
            result,
            "POLICY native classification at {index}"
        );
        assert_eq!(
            case["result"].as_bool(),
            Some(result),
            "POLICY decoded source boolean at {index}"
        );
    }
}
