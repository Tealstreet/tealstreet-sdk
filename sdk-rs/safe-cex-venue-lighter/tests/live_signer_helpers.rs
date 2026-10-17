use safe_cex_exchange::js_runtime::value::from_json;
use safe_cex_venue_lighter::signer_helpers::*;
use serde_json::{Value, json};
use std::process::Command;
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn lighter_signer_helpers_match_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/signer-helpers-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (name, count) in [("nonce", 992), ("provider", 66)] {
        assert_eq!(
            oracle[name].as_array().unwrap().len(),
            count,
            "{name} scalar/opaque corpus inventory"
        );
    }

    for case in oracle["nonce"].as_array().unwrap() {
        let input = case.get("value").map(from_json);
        let actual = match is_lighter_invalid_nonce_error(input.as_ref()) {
            Ok(value) => json!({"ok":value}),
            Err(error) => {
                json!({"error":{"name":error.value.get("name").unwrap().to_display(),"message":error.message()}})
            }
        };
        assert_eq!(
            actual, case["expected"],
            "isLighterInvalidNonceError {case}"
        );
    }
    for case in oracle["provider"].as_array().unwrap() {
        match get_provider(case.get("extra")) {
            Some(value) => assert_eq!(
                Some(value),
                case["expected"].get("ok"),
                "getProvider native value identity {case}"
            ),
            None => assert_eq!(
                case["expected"]["undefined"], true,
                "getProvider absence {case}"
            ),
        }
    }
}
