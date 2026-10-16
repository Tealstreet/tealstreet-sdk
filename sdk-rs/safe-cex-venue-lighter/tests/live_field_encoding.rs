use num_bigint::BigInt;
use safe_cex_venue_lighter::{sdk_field_encoding::*, transaction_builders::TransactionBuildError};
use serde_json::Value;
use std::{collections::BTreeMap, process::Command};
fn capture() -> &'static Value {
    static C: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/field-encoding-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "source oracle: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn string(v: &Value) -> Vec<u16> {
    assert_eq!(v["kind"], "string", "source string carrier");
    units(&v["units"])
}
fn number(v: &Value) -> f64 {
    assert_eq!(v["kind"], "number");
    f64::from_bits(u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
fn optional_number(v: &Value) -> Option<f64> {
    if v["kind"] == "undefined" {
        None
    } else {
        Some(number(v))
    }
}
fn bigint(v: &Value) -> BigInt {
    assert_eq!(v["kind"], "bigint");
    v["decimal"].as_str().unwrap().parse().unwrap()
}
enum Native {
    String(Vec<u16>),
    Pair([Vec<u16>; 2]),
}
#[test]
#[ignore = "requires live TypeScript oracle"]
fn field_encoding_native_strings_tuples_and_errors() {
    let oracle = capture();
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        150,
        "independent authored floor, stopped iteration149"
    );
    let mut counts = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        let method = row["method"].as_str().unwrap();
        let a = row["args"].as_array().unwrap();
        let label = format!("{method} iteration{i}");
        *counts.entry(method).or_insert(0usize) += 1;
        let result: Result<Native, TransactionBuildError> = match method {
            "uint32ToFieldElement" => uint32_to_field_element(number(&a[0])).map(Native::String),
            "uint64ToFieldElement" => uint64_to_field_element(&bigint(&a[0])).map(Native::String),
            "int64ToFieldElement" => int64_to_field_element(&bigint(&a[0])).map(Native::String),
            "splitUint64" => split_uint64(&bigint(&a[0])).map(Native::Pair),
            "hexToFieldElement" => Ok(Native::String(hex_to_field_element(&string(&a[0])))),
            "concatenateFieldElements" => {
                let elements: Vec<_> = a[0]["values"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(string)
                    .collect();
                concatenate_field_elements(&elements).map(Native::String)
            }
            "truncate" => {
                let ellipsis = if a[2]["kind"] == "undefined" {
                    None
                } else {
                    Some(string(&a[2]))
                };
                Ok(Native::String(truncate(
                    &string(&a[0]),
                    number(&a[1]),
                    ellipsis.as_deref(),
                )))
            }
            "formatAddress" => Ok(Native::String(format_address(
                &string(&a[0]),
                optional_number(&a[1]),
                optional_number(&a[2]),
            ))),
            _ => panic!("unwitnessed symbol"),
        };
        match result {
            Ok(Native::String(value)) => {
                assert!(row["error"].is_null(), "{label} native success");
                assert_eq!(value, string(&row["result"]), "{label} native UTF16 result");
            }
            Ok(Native::Pair(value)) => {
                assert!(row["error"].is_null(), "{label} native success");
                assert_eq!(row["result"]["kind"], "array");
                let expected = row["result"]["values"].as_array().unwrap();
                assert_eq!(expected.len(), 2, "independent tuple count");
                for k in 0..2 {
                    assert_eq!(
                        value[k],
                        string(&expected[k]),
                        "{label} native tuple field{k} UTF16"
                    );
                }
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
            ("uint32ToFieldElement", 18),
            ("uint64ToFieldElement", 12),
            ("int64ToFieldElement", 12),
            ("splitUint64", 12),
            ("hexToFieldElement", 26),
            ("concatenateFieldElements", 12),
            ("truncate", 39),
            ("formatAddress", 19)
        ]),
        "independent per-method floor"
    );
    // Returned UTF16 strings/ordered tuples are the encoder's native values.
    // No native decode/reencode pair or native JSON serialization is used.
    assert_eq!(
        oracle["contentHash"], "cea7cb8fce8e296edeb7b5a1504d5233f98d395395600de1c1a0f9ca161445fd",
        "authored source-only full corpus pin after native assertions"
    );
}
