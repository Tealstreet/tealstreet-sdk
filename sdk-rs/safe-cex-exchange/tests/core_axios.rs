use safe_cex_exchange::error_reporting::{enhance_axios_error, is_network_error};
use serde_json::{Value, json};
#[test]
#[ignore = "requires live TypeScript compiler; CI runs include-ignored"]
fn axios_helpers_match_actual_typescript() {
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/axios-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["cases"].as_array().unwrap().len(),
        64,
        "AXIOS-CARDINALITY source cases"
    );
    for case in oracle["cases"].as_array().unwrap() {
        let actual = enhance_axios_error(
            &case["input"],
            case["isError"].as_bool().unwrap(),
            Some("venue"),
            case["timestamp"].as_str().unwrap(),
            case["xhr"].as_bool().unwrap(),
        );
        assert_eq!(actual.is_err(), case["threw"].as_bool().unwrap(), "{case}");
        if let Ok(error) = actual {
            let mut projection = json!({"message":error.message,"enhancedMessage":error.enhanced_message,"context":error.context});
            if let Some(message) = error.original_error.as_ref().and_then(|e| e.get("message")) {
                projection["originalMessage"] = message.clone();
            }
            assert_eq!(projection, case["result"], "{case}");
        }
        // Enhancement marks Error identity and mutates message before this source predicate executes.
        let mut shape = case["input"].clone();
        if case["isError"].as_bool().unwrap() && shape.is_object() {
            shape["isAxiosError"] = json!(true);
            if let Some(result) = case.get("result") {
                shape["message"] = result["message"].clone();
            }
        }
        assert_eq!(
            is_network_error(&shape),
            case["network"].as_bool().unwrap(),
            "{case}"
        );
    }
}
