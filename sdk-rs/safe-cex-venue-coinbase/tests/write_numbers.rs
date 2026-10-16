use safe_cex_venue_coinbase::{starbase::CoinbaseStarbaseProtocolError, starbase_orders::*};
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
#[path = "support/capture_metadata.rs"]
mod capture_metadata;
#[path = "support/compare.rs"]
mod comparison;
#[path = "support/special_numbers.rs"]
mod special;

fn error(error: CoinbaseStarbaseProtocolError) -> Value {
    json!({"error":error})
}
#[test]
fn write_numeric_builders_live_values_errors_and_float_bits() {
    let cases = numeric_capture_cases();
    assert!(
        cases.len() > 19_000,
        "maintained numeric corpus unexpectedly small"
    );
    for case in cases {
        let number = case
            .get("bits")
            .map(|v| f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap()));
        let context = case["context"].as_str().unwrap_or("");
        let actual = match case["name"].as_str().unwrap() {
            "protocolError" => match protocol_error::<Value>(context) {
                Err(e) => error(e),
                Ok(v) => json!({"ok":v}),
            },
            "requirePositive" => match require_positive(number, context) {
                Err(e) => error(e),
                Ok(value) => {
                    let mut output = json!({"ok":value});
                    special::project_number(&mut output, "ok", value);
                    output
                }
            },
            "coinbaseStarbaseTotalBaseAmount" => {
                match coinbase_starbase_total_base_amount(number.unwrap()) {
                    Err(e) => error(e),
                    Ok(value) => {
                        let mut output = json!(value);
                        special::project_number(
                            &mut output,
                            "totalBaseAmount",
                            value.total_base_amount,
                        );
                        json!({"ok":output})
                    }
                }
            }
            "coinbaseStarbaseTotalContracts" => {
                match coinbase_starbase_total_contracts(number.unwrap()) {
                    Err(e) => error(e),
                    Ok(value) => {
                        let mut output = json!(value);
                        special::project_number(
                            &mut output,
                            "totalContracts",
                            value.total_contracts,
                        );
                        json!({"ok":output})
                    }
                }
            }
            "coinbaseStarbaseUnresolvedValidUntil" => {
                match coinbase_starbase_unresolved_valid_until(number.unwrap()) {
                    Err(e) => error(e),
                    Ok(value) => {
                        let mut output = json!(value);
                        special::project_number(
                            &mut output,
                            "gatewayIntegerTimestamp",
                            value.gateway_integer_timestamp,
                        );
                        json!({"ok":output})
                    }
                }
            }
            name => panic!("unclassified numeric function {name}"),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

fn numeric_capture_cases() -> &'static Vec<Value> {
    static CASES: OnceLock<Vec<Value>> = OnceLock::new();
    CASES.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/write-numbers.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        capture_metadata::assert_capture_metadata(&Value::Array(cases.clone()), 19_602);
        cases
    })
}

#[test]
fn numeric_capture_preserves_signed_zero_bits() {
    let cases = numeric_capture_cases();
    assert!(
        cases.iter().any(|case| {
            case["name"] == "requirePositive" && case["bits"] == "8000000000000000"
        }),
        "capture must distinguish negative zero from positive zero"
    );
}

#[test]
fn numeric_capture_preserves_finite_payload_bits_and_canonicalizes_nan() {
    let cases = numeric_capture_cases();
    assert!(
        cases.iter().any(|case| {
            case["name"] == "requirePositive" && case["bits"] == "3fe0000000000000"
        }),
        "capture must retain the exact finite 0.5 payload"
    );
    assert!(
        cases
            .iter()
            .filter(|case| case["name"] == "requirePositive" && case.get("bits").is_some())
            .all(|case| {
                let bits = case["bits"].as_str().unwrap();
                let value = u64::from_str_radix(bits, 16).unwrap();
                (value & 0x7ff0000000000000) != 0x7ff0000000000000
                    || (value & 0x000fffffffffffff) == 0
                    || bits == "7ff8000000000000"
            }),
        "only NaN payloads may be canonicalized"
    );
}
