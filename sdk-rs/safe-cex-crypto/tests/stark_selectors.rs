use safe_cex_crypto::starknet::{hash_typed_data, parse_felt};
use serde_json::{Value, json};
use std::process::Command;

const INPUTS: [(&str, &str); 24] = [
    ("name", r#""transfer""#),
    ("default", r#""__default__""#),
    ("l1-default", r#""__l1_default__""#),
    ("uppercase-default", r#""__DEFAULT__""#),
    ("empty-name", r#""""#),
    ("decimal-name", r#""123""#),
    ("hex", r#""0x123""#),
    ("uppercase-prefix", r#""0X123""#),
    ("empty-hex", r#""0x""#),
    ("empty-uppercase-hex", r#""0X""#),
    ("mixed-hex", r#""0xAbC""#),
    ("invalid-hex", r#""0xg""#),
    ("negative-hex-name", r#""0x-1""#),
    ("whitespace-name", r#"" 0x123 ""#),
    ("latin-name", r#""café""#),
    ("cjk-name", r#""雪""#),
    ("astral-name", r#""😀""#),
    ("lone-surrogate", r#""\ud800""#),
    (
        "prime-minus-one",
        r#""0x800000000000011000000000000000000000000000000000000000000000000""#,
    ),
    (
        "prime",
        r#""0x800000000000011000000000000000000000000000000000000000000000001""#,
    ),
    (
        "prime-plus-one",
        r#""0x800000000000011000000000000000000000000000000000000000000000002""#,
    ),
    (
        "uppercase-prime",
        r#""0X800000000000011000000000000000000000000000000000000000000000001""#,
    ),
    (
        "wide-300",
        r#""0x1000000000000000000000000000000000000000000000000000000000000000000000000011""#,
    ),
    (
        "wide-512",
        r#""0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff""#,
    ),
];

#[test]
fn actual_sdk_selector_names_and_hex() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_selectors_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "selector_source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let inputs: Vec<Value> = INPUTS
        .iter()
        .map(|(label, raw)| json!({ "label": label, "inputRaw": raw }))
        .collect();
    assert_eq!(
        oracle["inputs"],
        json!(inputs),
        "selector_native_independent_inventory"
    );
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 48, "selector_native_case_floor");
    let address = parse_felt("0x1234").unwrap();
    let mut index = 0;
    let mut accepted = 0;
    let mut refused = 0;
    let mut representation_refused = 0;
    for revision in [0, 1] {
        for (input_index, (label, input_raw)) in INPUTS.iter().enumerate() {
            let row = rows.get(index).expect("selector_native_index_coverage");
            index += 1;
            assert_eq!(row["id"], format!("selector/{revision}/{label}"));
            assert_eq!(row["revision"], revision);
            assert_eq!(row["label"], *label);
            assert_eq!(row["inputRaw"], *input_raw);
            assert_eq!(row["address"], "0x1234");
            let source_accepts =
                !matches!(input_index, 8 | 9) && !(revision == 0 && input_index >= 19);
            assert_eq!(
                row["accepted"], source_accepts,
                "selector_native_independent_acceptance"
            );
            let raw = row["raw"].as_str().unwrap();
            assert!(
                raw.contains(input_raw),
                "selector_native_original_json_input"
            );
            let result = hash_typed_data(raw, &address);
            if *label == "lone-surrogate" {
                assert_eq!(row["units"], json!([0xd800]));
                assert!(source_accepts && row["hash"].is_string());
                assert!(
                    result.is_err(),
                    "selector_native_canonical_utf8_refusal sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary"
                );
                representation_refused += 1;
                continue;
            }
            let data: Value = serde_json::from_str(raw).unwrap();
            assert_eq!(data["primaryType"], "Message");
            assert_eq!(
                data["types"]["Message"],
                json!([{ "name": "selector", "type": "selector" }])
            );
            assert_eq!(
                data["message"]["selector"],
                serde_json::from_str::<Value>(input_raw).unwrap()
            );
            if source_accepts {
                let hash = result.unwrap_or_else(|error| {
                    panic!("selector_native_accepted {revision}/{label}: {error}")
                });
                assert_eq!(
                    format!("{hash:#x}"),
                    row["hash"].as_str().unwrap(),
                    "selector_native_hash {revision}/{label}"
                );
                assert_eq!(
                    row["hash"], row["preparedHash"],
                    "selector_source_prepared_identity"
                );
                accepted += 1;
            } else {
                assert!(row["error"].is_object(), "selector_source_refusal");
                assert!(
                    result.is_err(),
                    "selector_native_refusal {revision}/{label}"
                );
                refused += 1;
            }
        }
    }
    assert_eq!(index, 48, "selector_native_completed_comparisons");
    assert_eq!(accepted, 37, "selector_native_accepted_floor");
    assert_eq!(refused, 9, "selector_native_refused_floor");
    assert_eq!(
        representation_refused, 2,
        "selector_native_utf8_refusal_floor"
    );
}

#[test]
fn actual_sdk_selector_json_coercion() {
    let inputs = [
        "true",
        "false",
        "17",
        "9007199254740993",
        "1e400",
        "[]",
        r#"["0x10"]"#,
        "[1,2]",
        "{}",
        "null",
        r#"{"toString":1}"#,
        r#"{"valueOf":1}"#,
    ];
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_selectors_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "selector_coercion_source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["rows"].as_array().unwrap().len(),
        48,
        "selector_coercion_original_floor"
    );
    assert_eq!(
        oracle["coercionInputs"],
        json!(inputs),
        "selector_coercion_native_independent_inventory"
    );
    let rows = oracle["coercionRows"].as_array().unwrap();
    assert_eq!(rows.len(), 24, "selector_coercion_native_floor");
    let address = parse_felt("0x1234").unwrap();
    let expected_text = [
        Some("true"),
        Some("false"),
        Some("17"),
        Some("9007199254740992"),
        Some("Infinity"),
        Some(""),
        Some("0x10"),
        Some("1,2"),
        Some("[object Object]"),
        None,
        None,
        Some("[object Object]"),
    ];
    let mut index = 0;
    let mut accepted = 0;
    let mut refused = 0;
    for revision in [0, 1] {
        for (input_index, input_raw) in inputs.iter().enumerate() {
            let row = rows
                .get(index)
                .expect("selector_coercion_native_index_coverage");
            index += 1;
            assert_eq!(
                row["id"],
                format!("selector-coercion/{revision}/{input_index}")
            );
            assert_eq!(row["revision"], revision);
            assert_eq!(row["inputRaw"], *input_raw);
            assert_eq!(row["address"], "0x1234");
            let should_accept = !matches!(input_index, 9 | 10);
            assert_eq!(
                row["accepted"], should_accept,
                "selector_coercion_native_independent_acceptance"
            );
            let raw = row["raw"].as_str().unwrap();
            assert!(raw.contains(input_raw));
            let data: Value = serde_json::from_str(raw).unwrap();
            assert_eq!(
                data["message"]["selector"],
                serde_json::from_str::<Value>(input_raw).unwrap()
            );
            assert_eq!(
                data["types"]["Message"],
                json!([{ "name": "selector", "type": "selector" }])
            );
            let result = hash_typed_data(raw, &address);
            if should_accept {
                let text = expected_text[input_index].unwrap();
                assert_eq!(
                    row["units"],
                    json!(text.encode_utf16().collect::<Vec<_>>()),
                    "selector_coercion_native_original_units"
                );
                let hash = result.unwrap_or_else(|error| {
                    panic!("selector_coercion_native_accepted {revision}/{input_index}: {error}")
                });
                assert_eq!(
                    format!("{hash:#x}"),
                    row["hash"].as_str().unwrap(),
                    "selector_coercion_native_hash {revision}/{input_index}"
                );
                assert_eq!(
                    row["hash"], row["stringHash"],
                    "selector_coercion_source_string_identity"
                );
                accepted += 1;
            } else {
                assert!(row["error"].is_object());
                assert!(
                    result.is_err(),
                    "selector_coercion_native_refusal {revision}/{input_index}"
                );
                refused += 1;
            }
        }
    }
    assert_eq!(index, 24, "selector_coercion_native_completed_comparisons");
    assert_eq!(accepted, 20);
    assert_eq!(refused, 4);
}
