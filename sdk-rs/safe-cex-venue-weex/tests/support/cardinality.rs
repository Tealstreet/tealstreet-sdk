use serde_json::Value;
use std::collections::BTreeSet;
fn canonical(value: &Value) -> String {
    match value {
        Value::Object(values) => {
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort();
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|key| format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap(),
                        canonical(&values[key])
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(values) => format!(
            "[{}]",
            values.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        _ => value.to_string(),
    }
}
// Retain every collection boundary and object key, discard scalar values only.
pub fn shape(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(shape).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), shape(value)))
                .collect(),
        ),
        _ => Value::Null,
    }
}
fn identities(cases: &[Value]) -> (String, usize) {
    let mut hash = 0xcbf29ce484222325u64;
    let mut unique = BTreeSet::new();
    for case in cases {
        let identity = Value::Array(
            [
                "fn", "id", "name", "zone", "raw", "args", "mode", "margin", "leverage", "market",
            ]
            .into_iter()
            .map(|key| case[key].clone())
            .collect(),
        );
        let text = canonical(&identity);
        unique.insert(text.clone());
        for byte in text.bytes().chain(std::iter::once(b'\n')) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    (format!("{hash:016x}"), unique.len())
}
pub fn check(corpus: &Value, name: &str) {
    let inventory: Value =
        serde_json::from_str(include_str!("../cardinality-census.json")).unwrap();
    let expected = &inventory[name];
    for (key, count) in expected["counts"].as_object().unwrap() {
        let actual = match &corpus[key] {
            Value::Array(v) => v.len(),
            Value::Object(v) => v.len(),
            _ => panic!("cardinality::{name}::{key}::missing_collection"),
        };
        assert_eq!(
            actual as u64,
            count.as_u64().unwrap(),
            "cardinality::{name}::{key}::source_count"
        );
    }
    for (key, sub) in expected["subcorpusIdentities"].as_object().unwrap() {
        let (hash, unique) = identities(corpus[key].as_array().unwrap());
        assert_eq!(
            unique as u64,
            sub["uniqueIdentities"].as_u64().unwrap(),
            "cardinality::{name}::{key}::unique_identities"
        );
        assert_eq!(
            hash,
            sub["identityFnv64"].as_str().unwrap(),
            "cardinality::{name}::{key}::input_inventory"
        );
    }
    if name == "types" {
        for (key, count) in expected["constantLengths"].as_object().unwrap() {
            assert_eq!(
                corpus["constants"][key].as_array().unwrap().len() as u64,
                count.as_u64().unwrap(),
                "cardinality::types::{key}::source_entries"
            );
        }
        for (key, expected_shape) in expected["constantShapes"].as_object().unwrap() {
            assert_eq!(
                shape(&corpus["constants"][key]),
                *expected_shape,
                "cardinality::types::{key}::source_shape"
            );
        }
        return;
    }
    if name == "response" {
        for case in corpus["rows"].as_array().unwrap() {
            if let Some(capture) = case.get("capture") {
                let key = case["name"].as_str().unwrap();
                assert_eq!(
                    shape(capture),
                    expected["captureShapes"][key],
                    "cardinality::response::{key}::source_shape"
                );
            }
        }
    }
    let (hash, unique) = identities(
        corpus[expected["main"].as_str().unwrap()]
            .as_array()
            .unwrap(),
    );
    assert_eq!(
        unique as u64,
        expected["uniqueIdentities"].as_u64().unwrap(),
        "cardinality::{name}::unique_identities"
    );
    assert_eq!(
        hash,
        expected["identityFnv64"].as_str().unwrap(),
        "cardinality::{name}::input_inventory"
    );
    if name == "book" {
        let mut levels = 0;
        let mut successes = 0;
        for case in corpus["cases"].as_array().unwrap() {
            if let Some(result) = case.get("result") {
                successes += 1;
                let id = format!(
                    "{}::{}",
                    case["fn"].as_str().unwrap(),
                    case["id"].as_str().unwrap()
                );
                let counts = if let Some(rows) = result.as_array() {
                    levels += rows.len();
                    serde_json::json!(rows.len())
                } else {
                    let bids = result["bids"].as_array().unwrap().len();
                    let asks = result["asks"].as_array().unwrap().len();
                    levels += bids + asks;
                    serde_json::json!([bids, asks])
                };
                assert_eq!(
                    counts, expected["levels"][&id],
                    "cardinality::book::{id}::source_levels"
                );
            }
        }
        assert_eq!(successes, 25, "cardinality::book::source_successes");
        assert_eq!(levels, 40, "cardinality::book::source_total_levels");
    }
}
