use safe_cex_crypto::poseidon_types::{SchnorrKeyPair, WasmError};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;
fn paths<T: DeserializeOwned>(raw: &str) -> Vec<T> {
    let value: Value = serde_json::from_str(raw).unwrap();
    vec![
        serde_json::from_str(raw).unwrap(),
        T::deserialize(value.clone()).unwrap(),
        T::deserialize(&value).unwrap(),
    ]
}
fn shape<T: DeserializeOwned + Serialize>(input: &Value) {
    for field in input.as_object().unwrap().keys() {
        let mut missing = input.clone();
        missing.as_object_mut().unwrap().remove(field);
        let mut null = input.clone();
        null[field] = Value::Null;
        for input in [missing, null, json!([]), Value::Null, json!(17)] {
            assert!(
                serde_json::from_value::<T>(input).is_err(),
                "Poseidon required map-only {field}"
            );
        }
    }
}
#[test]
#[ignore = "actual source Node oracle"]
fn poseidon_record_native_identity() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/poseidon-types-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        corpus["contentHash"], "a8880597b3aa2cf85faaf84d55572a6ef43d8b22837650d723977886a7ef5424",
        "Poseidon live encoded corpus content"
    );
    let key = json!({"privateKey":"private-A","publicKey":"public-B"});
    let error = json!({"error":"failure-C"});
    assert_eq!(
        corpus["bases"],
        json!({"SchnorrKeyPair":key,"WasmError":error}),
        "Poseidon independent base content"
    );
    for native in paths::<SchnorrKeyPair>(&key.to_string()) {
        assert_eq!(
            native.private_key, "private-A",
            "SchnorrKeyPair.privateKey native identity"
        );
        assert_eq!(
            native.public_key, "public-B",
            "SchnorrKeyPair.publicKey native identity"
        );
    }
    for native in paths::<WasmError>(&error.to_string()) {
        assert_eq!(native.error, "failure-C", "WasmError.error native identity");
    }
    let texts = [
        vec![],
        vec![32, 120, 32],
        vec![233],
        vec![0xd83d, 0xdc69],
        vec![0xfeff],
        vec![0x85],
        vec![0xd800],
        vec![0xdfff],
        vec![120, 0xd800, 121],
    ];
    let rows = corpus["strings"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        27,
        "Poseidon three string leaves times nine inputs"
    );
    for (index, row) in rows.iter().enumerate() {
        let units = &texts[index % 9];
        assert_eq!(row["units"], json!(units));
        assert_eq!(row["length"], units.len());
        assert_eq!(row["type"], "string");
        let expected = String::from_utf16(units);
        assert_eq!(row["representable"], expected.is_ok());
        let raw = row["encoded"].as_str().unwrap();
        if let Ok(text) = expected {
            if row["record"] == "SchnorrKeyPair" {
                for native in paths::<SchnorrKeyPair>(raw) {
                    if row["field"] == "privateKey" {
                        assert_eq!(native.private_key, text);
                        assert_eq!(native.public_key, "public-B");
                    } else {
                        assert_eq!(native.public_key, text);
                        assert_eq!(native.private_key, "private-A");
                    }
                }
            } else {
                for native in paths::<WasmError>(raw) {
                    assert_eq!(native.error, text, "WasmError native UTF16 identity");
                }
            }
        } else if row["record"] == "SchnorrKeyPair" {
            assert!(serde_json::from_str::<SchnorrKeyPair>(raw).is_err());
        } else {
            assert!(serde_json::from_str::<WasmError>(raw).is_err());
        }
    }
    shape::<SchnorrKeyPair>(&key);
    shape::<WasmError>(&error);
    // Wire checks come after every decoded native assertion.
    assert_eq!(
        serde_json::to_value(paths::<SchnorrKeyPair>(&key.to_string()).remove(0)).unwrap(),
        key
    );
    assert_eq!(
        serde_json::to_value(paths::<WasmError>(&error.to_string()).remove(0)).unwrap(),
        error
    );
}

#[test]
#[ignore = "actual source Node content control"]
fn poseidon_encoded_surrogate_control() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "poseidon_record_native_identity",
            "--include-ignored",
            "--nocapture",
        ])
        .env("POSEIDON_TYPES_MUTATION", "surrogate-content")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("Poseidon live encoded corpus content"),
        "{text}"
    );
}
