use safe_cex_venue_coinbase::starbase_public::validate_context;
use serde_json::Value;
use std::process::Command;

fn oracle() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/validate-public-context.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn public_context_validation_matches_live_source_order_and_cardinality() {
    let cases = oracle()["cases"].as_array().unwrap().clone();
    assert_eq!(cases.len(), 19, "source context corpus total");
    let mut comparisons = 0;
    for case in &cases {
        match validate_context(&case["input"]) {
            Ok(()) => assert!(
                case["result"].get("error").is_none(),
                "native context success branch: {}",
                case["label"]
            ),
            Err(error) => {
                let expected = &case["result"]["error"];
                assert_eq!(
                    error.name,
                    expected["name"].as_str().unwrap(),
                    "native context error name: {}",
                    case["label"]
                );
                assert_eq!(
                    error.message,
                    expected["message"].as_str().unwrap(),
                    "native context error message: {}",
                    case["label"]
                );
            }
        }
        comparisons += 1;
    }
    assert_eq!(comparisons, 19, "native context comparison count");
}
