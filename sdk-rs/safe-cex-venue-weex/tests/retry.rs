use safe_cex_venue_weex::retry::{clamp_weex_rate_limit_delay, parse_rate_limit_interval_ms};
use serde_json::Value;
use std::{path::Path, process::Command};
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
#[test]
fn retry_numbers_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/retry-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let inputs = [
        (-1.0, "negative"),
        (-0.0, "negative_zero"),
        (0.0, "zero"),
        (0.25, "fraction"),
        (59999.5, "below_cap"),
        (60000.0, "cap"),
        (60000.5, "above_cap"),
        (f64::INFINITY, "infinity"),
        (f64::NEG_INFINITY, "negative_infinity"),
        (f64::NAN, "nan"),
        (f64::from_bits(1), "subnormal"),
        (f64::MAX, "max"),
    ];
    let cases = corpus["clamp"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "retry::clamp_cases");
    let mut compared = 0;
    for (index, case) in cases.iter().enumerate() {
        let (input, id) = inputs[index];
        assert_eq!(case["id"], id, "retry::clamp_identity");
        assert_eq!(case["input"]["bits"], bits(input), "retry::clamp_input");
        assert_eq!(
            bits(clamp_weex_rate_limit_delay(input)),
            case["result"]["bits"],
            "retry::clamp::{id}"
        );
        compared += 1;
    }
    let ids = [
        "second",
        "minute",
        "hour",
        "day",
        "leading_zero",
        "zero",
        "missing_digits",
        "negative",
        "decimal",
        "missing_hyphen",
        "empty",
        "unknown",
        "suffix",
        "newline",
        "crlf",
        "unicode_line",
        "unicode_digit",
        "surrogate_prefix",
        "emoji_prefix",
        "bare",
        "overflow_digits",
        "overflow_product",
        "precision",
    ];
    let cases = corpus["intervals"].as_array().unwrap();
    assert_eq!(cases.len(), 23, "retry::interval_cases");
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(case["id"], ids[index], "retry::interval_identity");
        let units: Vec<u16> = serde_json::from_value(case["units"].clone()).unwrap();
        if ids[index] == "surrogate_prefix" {
            assert_eq!(units[0], 0xd800, "retry::surrogate_native_input");
        }
        assert_eq!(
            bits(parse_rate_limit_interval_ms(&units)),
            case["result"]["bits"],
            "retry::interval::{}",
            ids[index]
        );
        compared += 1;
    }
    assert_eq!(compared, 35, "retry::compared");
}
