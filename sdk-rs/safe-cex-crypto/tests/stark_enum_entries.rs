use safe_cex_crypto::starknet::{hash_typed_data, parse_felt};
use serde_json::{Value, json};
use std::process::Command;

const PRIME: &str = "3618502788666131213697322783095070105623107215331596699973092056135872020481";

#[test]
fn actual_sdk_enum_entry_selection_and_payloads() {
    let inputs = vec![
        (
            "single",
            "normal",
            r#"{"One":["17"]}"#.to_owned(),
            Some("One"),
            "hash",
        ),
        (
            "extra-known",
            "normal",
            r#"{"One":["17"],"Two":["23"]}"#.to_owned(),
            Some("One"),
            "hash",
        ),
        (
            "extra-null",
            "normal",
            r#"{"One":["17"],"Unknown":null}"#.to_owned(),
            Some("One"),
            "hash",
        ),
        (
            "extra-malformed",
            "normal",
            r#"{"One":["17"],"Unknown":{"bad":true}}"#.to_owned(),
            Some("One"),
            "hash",
        ),
        (
            "extra-range",
            "normal",
            format!(r#"{{"One":["17"],"Two":["{PRIME}"]}}"#),
            Some("One"),
            "hash",
        ),
        (
            "key-order",
            "numeric",
            r#"{"2":["23"],"1":["17"]}"#.to_owned(),
            Some("1"),
            "hash",
        ),
        (
            "array-enum",
            "index",
            r#"[["17"],["23"]]"#.to_owned(),
            Some("0"),
            "hash",
        ),
        (
            "string-enum",
            "index",
            r#""17""#.to_owned(),
            Some("0"),
            "hash",
        ),
        ("bool-enum", "index", "true".to_owned(), None, "refuse"),
        ("number-enum", "index", "17".to_owned(), None, "refuse"),
        ("empty-object", "normal", "{}".to_owned(), None, "refuse"),
        ("empty-array", "index", "[]".to_owned(), None, "refuse"),
        ("empty-string", "index", r#""""#.to_owned(), None, "refuse"),
        ("null-enum", "normal", "null".to_owned(), None, "refuse"),
        (
            "zero-null",
            "zero",
            r#"{"Zero":null}"#.to_owned(),
            Some("Zero"),
            "hash",
        ),
        (
            "zero-bool",
            "zero",
            r#"{"Zero":true}"#.to_owned(),
            Some("Zero"),
            "hash",
        ),
        (
            "zero-object",
            "zero",
            r#"{"Zero":{"bad":true}}"#.to_owned(),
            Some("Zero"),
            "hash",
        ),
        (
            "zero-string",
            "zero",
            r#"{"Zero":"ignored"}"#.to_owned(),
            Some("Zero"),
            "hash",
        ),
        (
            "zero-number",
            "zero",
            r#"{"Zero":1e400}"#.to_owned(),
            Some("Zero"),
            "hash",
        ),
        (
            "pair-object",
            "pair",
            r#"{"Pair":{"0":"17","1":"23","extra":null}}"#.to_owned(),
            Some("Pair"),
            "hash",
        ),
        (
            "pair-string",
            "pair",
            r#"{"Pair":"17"}"#.to_owned(),
            Some("Pair"),
            "hash",
        ),
        (
            "pair-short-string",
            "pair",
            r#"{"Pair":"1"}"#.to_owned(),
            Some("Pair"),
            "refuse",
        ),
        (
            "pair-bool",
            "pair",
            r#"{"Pair":true}"#.to_owned(),
            Some("Pair"),
            "refuse",
        ),
        (
            "partial-object",
            "partial",
            r#"{"Partial":{"0":null,"1":"23","extra":null}}"#.to_owned(),
            Some("Partial"),
            "hash",
        ),
        (
            "partial-string",
            "partial",
            r#"{"Partial":"12"}"#.to_owned(),
            Some("Partial"),
            "hash",
        ),
        (
            "extra-zero-array",
            "zero",
            r#"{"Zero":["1"]}"#.to_owned(),
            Some("Zero"),
            "chosen-extra-array",
        ),
        (
            "extra-pair-array",
            "pair",
            format!(r#"{{"Pair":["17","23","{PRIME}"]}}"#),
            Some("Pair"),
            "chosen-extra-array",
        ),
    ];
    let schemas = json!({
        "normal": [["One", "(felt)"], ["Two", "(felt)"]],
        "numeric": [["2", "(felt)"], ["1", "(felt)"]],
        "index": [["0", "(felt)"], ["1", "(felt)"]],
        "zero": [["Zero", "()"], ["One", "(felt)"]],
        "pair": [["Pair", "(felt,felt)"], ["Other", "(felt)"]],
        "partial": [["Partial", "(,felt)"], ["Other", "(felt)"]]
    });
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_enum_entries_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "enum_entries_source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["inputs"],
        json!(inputs),
        "enum_entries_native_independent_inventory"
    );
    assert_eq!(
        oracle["schemas"], schemas,
        "enum_entries_native_declared_schemas"
    );
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 27, "enum_entries_native_floor");
    let address = parse_felt("0x1234").unwrap();
    let mut hashes = 0;
    let mut source_refusals = 0;
    let mut chosen_arrays = 0;
    let mut ignored_hashes = std::collections::BTreeSet::new();
    let mut zero_hashes = std::collections::BTreeSet::new();
    for (index, (id, schema, input_raw, first_key, policy)) in inputs.iter().enumerate() {
        let row = rows.get(index).expect("enum_entries_native_index_coverage");
        assert_eq!(row["id"], *id);
        assert_eq!(row["schema"], *schema);
        assert_eq!(row["inputRaw"], *input_raw);
        assert_eq!(
            row["firstKey"],
            json!(first_key),
            "enum_entries_source_first_key {id}"
        );
        assert_eq!(row["policy"], *policy);
        assert_eq!(row["address"], "0x1234");
        assert_eq!(
            row["accepted"],
            *policy != "refuse",
            "enum_entries_native_independent_acceptance"
        );
        let raw = row["raw"].as_str().unwrap();
        assert!(
            raw.contains(input_raw),
            "enum_entries_native_original_raw_input"
        );
        let data: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(
            data["message"]["x"],
            serde_json::from_str::<Value>(input_raw).unwrap()
        );
        let variants: Vec<Value> = schemas[*schema]
            .as_array()
            .unwrap()
            .iter()
            .map(|variant| json!({ "name": variant[0], "type": variant[1] }))
            .collect();
        assert_eq!(
            data["types"]["Choice"],
            json!(variants),
            "enum_entries_native_original_variant_order"
        );
        assert_eq!(
            data["types"]["Message"],
            json!([{ "name": "x", "type": "enum", "contains": "Choice" }])
        );
        let result = hash_typed_data(raw, &address);
        match *policy {
            "hash" => {
                let actual = result
                    .unwrap_or_else(|error| panic!("enum_entries_native_accepted {id}: {error}"));
                assert_eq!(
                    format!("{actual:#x}"),
                    row["hash"].as_str().unwrap(),
                    "enum_entries_native_hash {id}"
                );
                if index < 5 {
                    ignored_hashes.insert(actual.to_string());
                }
                if (14..19).contains(&index) {
                    zero_hashes.insert(actual.to_string());
                }
                hashes += 1;
            }
            "refuse" => {
                assert!(row["error"].is_object());
                assert!(result.is_err(), "enum_entries_native_refusal {id}");
                source_refusals += 1;
            }
            "chosen-extra-array" => {
                assert!(
                    row["hash"].is_string(),
                    "enum_entries_source_ignores_extra_array_values"
                );
                assert_eq!(
                    result.unwrap_err(),
                    "Stark enum tuple payload count does not match declared types",
                    "enum_entries_native_chosen_exact_array_arity {id}"
                );
                chosen_arrays += 1;
            }
            _ => panic!("enum_entries_unknown_native_policy"),
        }
    }
    assert_eq!(hashes, 17, "enum_entries_native_hash_floor");
    assert_eq!(
        source_refusals, 8,
        "enum_entries_native_source_refusal_floor"
    );
    assert_eq!(chosen_arrays, 2, "enum_entries_native_chosen_array_floor");
    assert_eq!(
        hashes + source_refusals + chosen_arrays,
        27,
        "enum_entries_native_completed_comparisons"
    );
    assert_eq!(
        ignored_hashes.len(),
        1,
        "enum_entries_native_extra_keys_ignored"
    );
    assert_eq!(
        zero_hashes.len(),
        1,
        "enum_entries_native_zero_payload_not_visited"
    );
}
