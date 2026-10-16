use safe_cex_venue_coinbase::starbase_public::parse_public_test_error;
use serde_json::{Value, json};
use std::process::Command;

fn oracle() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-test-error.cjs"
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
fn public_test_error_matches_live_source_and_preserves_optional_data() {
    let oracle = oracle();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 19, "source parser corpus total");
    let mut comparisons = 0;
    for case in cases {
        let actual = parse_public_test_error(case.get("input")).map(Value::Object);
        assert_eq!(
            actual,
            Some(case["result"].clone()).filter(|value| !value.is_null()),
            "{}",
            case["label"]
        );
        if let Some(Value::Object(actual)) = actual {
            assert_eq!(
                actual.get("code"),
                case["result"].get("code"),
                "native code field"
            );
            assert_eq!(
                actual.get("message"),
                case["result"].get("message"),
                "native message field"
            );
            assert_eq!(
                actual.contains_key("data"),
                case["result"].get("data").is_some(),
                "native data presence"
            );
            assert_eq!(
                actual.get("data"),
                case["result"].get("data"),
                "native data value"
            );
        }
        comparisons += 1;
    }
    assert_eq!(comparisons, 19, "native parser comparison count");

    let absent = parse_public_test_error(Some(&json!({"code":3,"message":"x"}))).unwrap();
    assert!(!absent.contains_key("data"), "absent data stays absent");
    let explicit_null =
        parse_public_test_error(Some(&json!({"code":3,"message":"x","data":null}))).unwrap();
    assert_eq!(
        explicit_null.get("data"),
        Some(&Value::Null),
        "explicit null data stays present"
    );
}
