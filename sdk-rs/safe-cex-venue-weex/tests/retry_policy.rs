use safe_cex_venue_weex::retry::{
    WeexRetryAfterValue, is_weex_read_only_request, parse_retry_after_ms, should_retry_weex_request,
};
use serde_json::Value;
use std::{path::Path, process::Command};
fn number(value: &Value) -> f64 {
    assert_eq!(value["type"], "number");
    f64::from_bits(u64::from_str_radix(value["bits"].as_str().unwrap(), 16).unwrap())
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn units(value: Option<&Value>) -> Option<Vec<u16>> {
    value.map(|v| serde_json::from_value(v.clone()).unwrap())
}
#[test]
fn retry_policy_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/retry-policy-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(corpus["header"]["arch"].is_string());
    assert!(corpus["header"]["node"].is_string());
    let date_ids = [
        "seconds",
        "string_seconds",
        "hex_seconds",
        "overflow_product",
        "zero",
        "negative_zero",
        "negative",
        "nan",
        "infinity",
        "negative_infinity",
        "empty",
        "space",
        "bad",
        "iso_future",
        "iso_past",
        "local_date",
        "now_nan",
        "now_infinity",
        "surrogate",
        "surrogate_prefix",
        "null",
        "boolean",
        "array",
        "object",
        "function",
        "bigint",
        "symbol",
        "undefined",
        "default_now",
    ];
    let dates = corpus["dates"].as_array().unwrap();
    assert_eq!(dates.len(), 29, "retry_policy::date_count");
    let (mut compared, mut date_calls) = (0, 0);
    for (index, case) in dates.iter().enumerate() {
        let id = date_ids[index];
        assert_eq!(case["id"], id, "retry_policy::date_identity");
        let string_units = units(case["input"].get("units"));
        let input = match case["input"]["kind"].as_str().unwrap() {
            "number" => WeexRetryAfterValue::Number(number(&case["input"]["value"])),
            "string" => WeexRetryAfterValue::String(string_units.as_ref().unwrap()),
            "other" => WeexRetryAfterValue::Other,
            other => panic!("unexpected kind {other}"),
        };
        if id == "negative_zero" {
            let WeexRetryAfterValue::Number(n) = input else {
                panic!()
            };
            assert_eq!(
                n.to_bits(),
                0x8000000000000000,
                "retry_policy::negative_zero_input"
            );
        }
        if id == "surrogate" {
            assert_eq!(
                string_units.as_ref().unwrap(),
                &[0xd800],
                "retry_policy::surrogate_input"
            );
        }
        let events = case["dateEvents"].as_array().unwrap();
        let should_parse = !(index < 4 || (20..28).contains(&index));
        assert_eq!(
            events.len(),
            usize::from(should_parse),
            "retry_policy::source_date_calls::{id}"
        );
        assert_eq!(
            case["clockCalls"],
            usize::from(id == "default_now"),
            "retry_policy::source_clock::{id}"
        );
        let mut calls = 0;
        let result = parse_retry_after_ms(input, number(&case["now"]), |input| {
            calls += 1;
            let expected: Vec<u16> = serde_json::from_value(events[0]["units"].clone()).unwrap();
            assert_eq!(input, expected, "retry_policy::date_argument::{id}");
            // Independent host capability witness: source Date.parse output,
            // before source subtracts now or applies Math.max.
            number(&events[0]["parsed"])
        });
        assert_eq!(
            calls,
            usize::from(should_parse),
            "retry_policy::native_date_calls::{id}"
        );
        assert_eq!(
            bits(result),
            case["result"]["bits"],
            "retry_policy::retry_after::{id}"
        );
        date_calls += calls;
        compared += 1;
    }
    assert_eq!(date_calls, 17, "retry_policy::date_calls");
    let read_ids = [
        "get",
        "head",
        "get_surrogate_url",
        "income",
        "income_query",
        "post_other",
        "post_missing",
        "post_empty",
        "post_suffix",
        "post_case",
        "post_long_s",
        "post_ligature",
        "post_ligature_long_s",
        "get_surrogate_method",
        "post_surrogate_url",
        "delete",
        "missing_method",
        "missing_config",
        "empty_method",
    ];
    let cases = corpus["readOnly"].as_array().unwrap();
    assert_eq!(cases.len(), 19, "retry_policy::read_count");
    for (index, case) in cases.iter().enumerate() {
        let id = read_ids[index];
        assert_eq!(case["id"], id, "retry_policy::read_identity");
        let method = units(case.get("method"));
        let url = units(case.get("url"));
        let result = is_weex_read_only_request(method.as_deref(), url.as_deref());
        assert_eq!(
            result,
            case["result"].as_bool().unwrap(),
            "retry_policy::read_only::{id}"
        );
        compared += 1;
    }
    let policy_ids = [
        "network_get",
        "network_post",
        "network_income",
        "network_head",
        "canceled",
        "timeout",
        "unsafe_tls",
        "no_code",
        "server_error",
        "rate_first",
        "rate_second",
        "rate_negative",
        "rate_nan",
        "rate_infinity",
        "rate_null",
        "mutation_rate",
        "missing_config_network",
        "response_no_status",
        "rate_income",
    ];
    let cases = corpus["policy"].as_array().unwrap();
    assert_eq!(cases.len(), 19, "retry_policy::policy_count");
    let mut network_calls = 0;
    for (index, case) in cases.iter().enumerate() {
        let id = policy_ids[index];
        assert_eq!(case["id"], id, "retry_policy::policy_identity");
        let method = units(case.get("method"));
        let url = units(case.get("url"));
        let expected_calls =
            usize::from(!["network_post", "mutation_rate", "missing_config_network"].contains(&id));
        assert_eq!(
            case["networkCalls"], expected_calls,
            "retry_policy::source_network_calls::{id}"
        );
        let mut calls = 0;
        let result = should_retry_weex_request(
            method.as_deref(),
            url.as_deref(),
            case.get("status").map(number),
            case.get("retryCount").map(number),
            || {
                calls += 1;
                case["network"].as_bool().unwrap()
            },
        );
        assert_eq!(
            calls, expected_calls,
            "retry_policy::native_network_calls::{id}"
        );
        assert_eq!(
            result,
            case["result"].as_bool().unwrap(),
            "retry_policy::policy::{id}"
        );
        network_calls += calls;
        compared += 1;
    }
    assert_eq!(network_calls, 16, "retry_policy::network_calls");
    assert_eq!(compared, 67, "retry_policy::compared");
}
