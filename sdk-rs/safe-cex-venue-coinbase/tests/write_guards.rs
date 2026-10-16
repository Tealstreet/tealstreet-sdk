use safe_cex_venue_coinbase::{starbase::CoinbaseStarbaseProtocolError, starbase_orders::*};
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
#[path = "support/compare.rs"]
mod comparison;
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/write-guards.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn capture<T: serde::Serialize>(result: Result<T, CoinbaseStarbaseProtocolError>) -> Value {
    match result {
        Ok(v) => json!({"ok":v}),
        Err(e) => json!({"error":e}),
    }
}
fn capture_void(result: Result<(), CoinbaseStarbaseProtocolError>) -> Value {
    match result {
        Ok(()) => json!({"undefined":true}),
        Err(e) => json!({"error":e}),
    }
}
#[test]
fn write_collection_guards_live_errors_identity_and_validation_order() {
    for case in oracle()["cases"].as_array().unwrap() {
        let input = case
            .get("inputJson")
            .map(|v| serde_json::from_str::<Value>(v.as_str().unwrap()).unwrap());
        let value = input.as_ref();
        let context = case["context"].as_str().unwrap();
        let actual = match case["name"].as_str().unwrap() {
            "requireRecord" => {
                let result = require_record(value, context);
                if let Ok(v) = result.as_ref() {
                    assert!(std::ptr::eq(*v, value.unwrap().as_object().unwrap()));
                }
                capture(result)
            }
            "requireArray" => {
                let result = require_array(value, context);
                if let Ok(v) = result.as_ref() {
                    assert!(std::ptr::eq(*v, value.unwrap().as_array().unwrap()));
                }
                capture(result)
            }
            "requireString" => capture(require_string(value, context)),
            "decodeStringArray" => capture(decode_string_array(value, context)),
            "validateLabel" => capture_void(validate_label(value)),
            "parseEnum" => {
                let allowed: Vec<_> = case["allowed"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect();
                capture(parse_enum(value, &allowed, context))
            }
            "assertUnique" => {
                let strings: Vec<_> = value
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect();
                capture_void(assert_unique(&strings, context))
            }
            name => panic!("unclassified guard {name}"),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}
#[test]
fn write_label_lone_surrogate_shared_representation_boundary() {
    for case in oracle()["surrogateCases"].as_array().unwrap() {
        let encoded = format!(
            "\"{}\"",
            "\\ud800".repeat(case["length"].as_u64().unwrap() as usize)
        );
        assert!(
            serde_json::from_str::<Value>(&encoded).is_err(),
            "shared-repr:utf8-string boundary changed; revisit manifest"
        );
    }
}
