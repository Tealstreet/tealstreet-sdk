use safe_cex_venue_weex::{retry::WEEX_RETRY_CONFIG, signing::sign_weex_message};
use serde_json::Value;
use std::{path::Path, process::Command};

fn number(value: &Value) -> f64 {
    assert_eq!(value["type"], "number");
    f64::from_bits(u64::from_str_radix(value["bits"].as_str().unwrap(), 16).unwrap())
}

fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}

fn units(value: &Value) -> Vec<u16> {
    serde_json::from_value(value.clone()).unwrap()
}

#[test]
fn api_helpers_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/api-helpers-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(WEEX_RETRY_CONFIG.retries, 3, "api_helpers::config::retries");
    // Pinning a const is the point: this asserts the shipped WEEX retry config,
    // and the mutation controls in this tree require a NAMED runtime failure at
    // exit 101 ("compilation failure alone is not discrimination"). Clippy's
    // `const { assert!(..) }` would turn that into a compile error instead.
    #[allow(clippy::assertions_on_constants)]
    {
        assert!(
            WEEX_RETRY_CONFIG.should_reset_timeout,
            "api_helpers::config::reset"
        );
    }
    assert_eq!(
        corpus["metadata"]["retries"], 3,
        "api_helpers::source_config::retries"
    );
    assert_eq!(
        corpus["metadata"]["shouldResetTimeout"], true,
        "api_helpers::source_config::reset"
    );

    let sign_ids = [
        "empty",
        "ascii",
        "request",
        "nul",
        "unicode",
        "nonbmp",
        "lone_key_high",
        "lone_message_low",
        "pair",
        "reversed",
        "adjacent_high",
        "long_key",
        "long_message",
    ];
    let signatures = corpus["signatures"].as_array().unwrap();
    assert_eq!(signatures.len(), 13, "api_helpers::signature_count");
    let mut compared = 0;
    for (index, case) in signatures.iter().enumerate() {
        let id = sign_ids[index];
        assert_eq!(case["id"], id, "api_helpers::signature_identity");
        let secret = units(&case["secret"]);
        let message = units(&case["message"]);
        let result = sign_weex_message(&secret, &message);
        assert_eq!(
            result,
            case["result"].as_str().unwrap(),
            "api_helpers::signature::{id}"
        );
        assert_eq!(result.len(), 44, "api_helpers::sha256_base64_length::{id}");
        compared += 1;
    }

    let condition_ids = [
        "network_get",
        "network_post",
        "rate_first",
        "rate_second",
        "rate_income",
        "rate_delete",
        "tls_head",
        "server_error",
    ];
    let conditions = corpus["conditions"].as_array().unwrap();
    assert_eq!(conditions.len(), 8, "api_helpers::condition_count");
    let mut network_calls = 0;
    for (index, case) in conditions.iter().enumerate() {
        let id = condition_ids[index];
        assert_eq!(case["id"], id, "api_helpers::condition_identity");
        let method = units(&case["method"]);
        let url = case.get("url").map(units);
        let expected_calls = usize::from(!["network_post", "rate_delete"].contains(&id));
        assert_eq!(
            case["networkCalls"], expected_calls,
            "api_helpers::source_network_calls::{id}"
        );
        let mut calls = 0;
        let result = (WEEX_RETRY_CONFIG.retry_condition)(
            Some(&method),
            url.as_deref(),
            case.get("status").map(number),
            case.get("count").map(number),
            &mut || {
                calls += 1;
                case["network"].as_bool().unwrap()
            },
        );
        assert_eq!(
            result,
            case["result"].as_bool().unwrap(),
            "api_helpers::condition::{id}"
        );
        assert_eq!(
            calls, expected_calls,
            "api_helpers::native_network_calls::{id}"
        );
        network_calls += calls;
        compared += 1;
    }
    assert_eq!(network_calls, 6, "api_helpers::network_call_count");

    let delay_ids = [
        "rate",
        "rate_zero",
        "rate_negative_zero",
        "rate_nan",
        "rate_infinity",
        "server",
        "absent",
        "status_nan",
        "status_infinity",
    ];
    let delays = corpus["delays"].as_array().unwrap();
    assert_eq!(delays.len(), 9, "api_helpers::delay_count");
    let mut delay_calls = 0;
    for (index, case) in delays.iter().enumerate() {
        let id = delay_ids[index];
        assert_eq!(case["id"], id, "api_helpers::delay_identity");
        let expected_calls = usize::from(index < 5);
        assert_eq!(
            case["delayCalls"], expected_calls,
            "api_helpers::source_delay_calls::{id}"
        );
        let mut calls = 0;
        let result = (WEEX_RETRY_CONFIG.retry_delay)(
            number(&case["count"]),
            case.get("status").map(number),
            &mut || {
                calls += 1;
                number(&case["delay"])
            },
        );
        assert_eq!(
            bits(result),
            case["result"]["bits"],
            "api_helpers::delay::{id}"
        );
        assert_eq!(
            calls, expected_calls,
            "api_helpers::native_delay_calls::{id}"
        );
        delay_calls += calls;
        compared += 1;
    }
    assert_eq!(delay_calls, 5, "api_helpers::delay_call_count");
    assert_eq!(compared, 30, "api_helpers::compared");
}
