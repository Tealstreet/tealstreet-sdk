use num_bigint::BigInt;
use safe_cex_venue_lighter::{
    sdk_numeric_utils::*,
    sdk_protocol::{MAX_ACCOUNT_INDEX, MAX_API_KEY_INDEX, MAX_NONCE},
    transaction_builders::TransactionBuildError,
};
use serde_json::Value;
use std::{collections::BTreeMap, process::Command};
fn capture() -> &'static Value {
    static C: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/numeric-utils-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "TS oracle failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn number(v: &Value) -> f64 {
    assert_eq!(v["kind"], "number", "declared source Number input");
    f64::from_bits(u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
fn bigint(v: &Value) -> BigInt {
    assert_eq!(v["kind"], "bigint", "declared source BigInt input");
    v["decimal"].as_str().unwrap().parse().unwrap()
}
enum Native {
    BigInt(BigInt),
    Boolean(bool),
}
#[test]
#[ignore = "requires live TypeScript oracle"]
fn numeric_utils_native_values_and_errors() {
    let oracle = capture();
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        160,
        "independent authored floor, stopped iteration159"
    );
    for (name, n) in [
        ("MaxAccountIndex", MAX_ACCOUNT_INDEX),
        ("MaxApiKeyIndex", MAX_API_KEY_INDEX),
        ("MaxNonce", MAX_NONCE),
    ] {
        assert_eq!(
            n.to_bits(),
            number(&oracle["constants"][name]).to_bits(),
            "native source constant {name}"
        );
    }
    let mut counts = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        let method = row["method"].as_str().unwrap();
        let a = row["args"].as_array().unwrap();
        let label = format!("{method} iteration{i}");
        *counts.entry(method).or_insert(0usize) += 1;
        let result: Result<Native, TransactionBuildError> = match method {
            "hexToBigInt" => hex_to_big_int(&units(&a[0]["units"])).map(Native::BigInt),
            "validateAddress" => Ok(Native::Boolean(validate_address(&units(&a[0]["units"])))),
            "clampBigInt" => {
                clamp_big_int(&bigint(&a[0]), &bigint(&a[1]), &bigint(&a[2])).map(Native::BigInt)
            }
            "isInRange" => Ok(Native::Boolean(is_in_range(
                &bigint(&a[0]),
                &bigint(&a[1]),
                &bigint(&a[2]),
            ))),
            "numberToBigInt" => number_to_big_int(number(&a[0])).map(Native::BigInt),
            "percentageOf" => percentage_of(
                &bigint(&a[0]),
                number(&a[1]),
                if a[2]["kind"] == "undefined" {
                    None
                } else {
                    Some(number(&a[2]))
                },
            )
            .map(Native::BigInt),
            "isValidAccountIndex" => Ok(Native::Boolean(is_valid_account_index(&bigint(&a[0])))),
            "isValidApiKeyIndex" => Ok(Native::Boolean(is_valid_api_key_index(number(&a[0])))),
            "isValidNonce" => Ok(Native::Boolean(is_valid_nonce(&bigint(&a[0])))),
            _ => panic!("unwitnessed source symbol"),
        };
        match result {
            Ok(Native::BigInt(actual)) => {
                assert!(row["error"].is_null(), "{label} native success");
                assert_eq!(
                    actual,
                    bigint(&row["result"]),
                    "{label} native BigInt value"
                );
            }
            Ok(Native::Boolean(actual)) => {
                assert!(row["error"].is_null(), "{label} native success");
                assert_eq!(
                    row["result"]["kind"], "boolean",
                    "{label} native result kind"
                );
                assert_eq!(
                    actual,
                    row["result"]["value"].as_bool().unwrap(),
                    "{label} native boolean value"
                );
            }
            Err(error) => {
                assert!(!row["error"].is_null(), "{label} native error");
                assert_eq!(
                    error.name,
                    row["error"]["name"].as_str().unwrap(),
                    "{label} native error name"
                );
                assert_eq!(
                    error.message,
                    units(&row["error"]["message"]),
                    "{label} native error UTF16"
                );
            }
        }
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("hexToBigInt", 20),
            ("validateAddress", 13),
            ("clampBigInt", 16),
            ("isInRange", 16),
            ("numberToBigInt", 21),
            ("percentageOf", 43),
            ("isValidAccountIndex", 8),
            ("isValidApiKeyIndex", 15),
            ("isValidNonce", 8)
        ]),
        "independent per-method floor"
    );
    // All decoded native values/errors and constants run before the source-only pin.
    assert_eq!(
        oracle["contentHash"], "52d00e8799f3afe182e9f0c2958b5902fdb59cd8a2dcbdd38787dca9f963e997",
        "authored source-only full corpus pin after native assertions"
    );
}
