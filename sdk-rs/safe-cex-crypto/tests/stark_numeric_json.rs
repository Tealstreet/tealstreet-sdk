use safe_cex_crypto::starknet::{hash_typed_data, parse_felt};
use serde_json::{Value, json};
use std::process::Command;

const TOKENS: [&str; 17] = [
    "17",
    "9007199254740993",
    "18446744073709551615",
    "340282366920938463463374607431768211455",
    "1e20",
    "1e30",
    "1e3",
    "1.25e2",
    "1.0000000000000001",
    "-1e-4000",
    "-0",
    "-0.0",
    "1e-4000",
    "1.5",
    "1e400",
    "-1e400",
    "-1",
];
const ROUNDED: [Option<&str>; 17] = [
    Some("17"),
    Some("9007199254740992"),
    Some("18446744073709551616"),
    Some("340282366920938463463374607431768211456"),
    Some("100000000000000000000"),
    Some("1000000000000000019884624838656"),
    Some("1000"),
    Some("125"),
    Some("1"),
    Some("0"),
    Some("0"),
    Some("0"),
    Some("0"),
    None,
    None,
    None,
    Some("-1"),
];
const SHAPES_V0: [&str; 6] = [
    "scalar",
    "nested",
    "array",
    "domain",
    "ignored",
    "ignored-nested",
];
const SHAPES_V1: [&str; 11] = [
    "scalar",
    "nested",
    "array",
    "domain",
    "ignored",
    "ignored-nested",
    "preset",
    "enum",
    "ignored-slot",
    "u128",
    "i128",
];
const COERCION_RAW: [&str; 17] = [
    "true",
    "false",
    "null",
    "[]",
    "[1]",
    r#"["0x10"]"#,
    "[1,2]",
    "{}",
    r#""0x10""#,
    r#""0b10""#,
    r#""0o10""#,
    r#"" 17 ""#,
    r#""\ufeff17\ufeff""#,
    r#""+17""#,
    r#""-1""#,
    r#""1e3""#,
    r#""""#,
];
const COERCION_KINDS: [(i32, &str); 6] = [
    (0, "felt"),
    (0, "shortstring"),
    (1, "felt"),
    (1, "shortstring"),
    (1, "u128"),
    (1, "i128"),
];
const RECURSIVE_INPUTS: [(&str, &str, &str, &str); 10] = [
    (
        "i128/enum/number",
        "i128",
        "/message/choice/Chosen/0",
        "9007199254740993",
    ),
    (
        "i128/enum/radix",
        "i128",
        "/message/choice/Chosen/0",
        r#""0x10""#,
    ),
    (
        "i128/merkle/number",
        "i128",
        "/message/tree/0/x",
        "9007199254740993",
    ),
    (
        "i128/merkle/radix",
        "i128",
        "/message/tree/0/x",
        r#""0x10""#,
    ),
    ("u256/root-extra", "u256", "/message/value/extra", "1e400"),
    (
        "u256/nested-extra",
        "u256",
        "/message/holder/value/extra",
        "1e400",
    ),
    (
        "TokenAmount/root-extra",
        "TokenAmount",
        "/message/value/extra",
        "1e400",
    ),
    (
        "TokenAmount/nested-extra",
        "TokenAmount",
        "/message/value/amount/extra",
        "1e400",
    ),
    ("NftId/root-extra", "NftId", "/message/value/extra", "1e400"),
    (
        "NftId/nested-extra",
        "NftId",
        "/message/value/token_id/extra",
        "1e400",
    ),
];

#[test]
fn actual_sdk_raw_json_number_ingress() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_numeric_json_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "numeric_json_source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["tokens"],
        json!(TOKENS),
        "numeric_json_declared_tokens"
    );
    assert_eq!(
        oracle["rounded"],
        json!(ROUNDED),
        "numeric_json_declared_rounding"
    );
    assert_eq!(oracle["shapesV0"], json!(SHAPES_V0));
    assert_eq!(oracle["shapesV1"], json!(SHAPES_V1));
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 289, "numeric_json_native_case_floor");
    let address = parse_felt("0x1234").unwrap();
    let mut row_index = 0;
    let mut accepted = 0;
    let mut refused = 0;
    let mut ignored = 0;
    for revision in [0, 1] {
        let shapes: &[&str] = if revision == 0 {
            &SHAPES_V0
        } else {
            &SHAPES_V1
        };
        for shape in shapes {
            for (token_index, token) in TOKENS.iter().enumerate() {
                let row = rows
                    .get(row_index)
                    .expect("numeric_json_native_index_coverage");
                row_index += 1;
                assert_eq!(row["id"], format!("json/{revision}/{shape}/{token_index}"));
                assert_eq!(row["revision"], revision);
                assert_eq!(row["shape"], *shape);
                assert_eq!(row["token"], *token);
                assert_eq!(row["address"], "0x1234");
                let is_ignored = shape.starts_with("ignored");
                let bounded = matches!(*shape, "preset" | "u128" | "i128");
                let expected = is_ignored
                    || (token_index < 13 && !(bounded && token_index == 3))
                    || (*shape == "i128" && token_index == 16);
                assert_eq!(
                    row["accepted"], expected,
                    "numeric_json_native_independent_acceptance"
                );
                let raw = row["raw"].as_str().unwrap();
                assert!(raw.contains(token), "numeric_json_native_original_lexeme");
                let result = hash_typed_data(raw, &address);
                if expected {
                    let actual = result.unwrap_or_else(|error| {
                        panic!("numeric_json_native_accepted {revision}/{shape}/{token}: {error}")
                    });
                    assert_eq!(
                        format!("{actual:#x}"),
                        row["hash"].as_str().unwrap(),
                        "numeric_json_native_hash {revision}/{shape}/{token}"
                    );
                    if is_ignored {
                        assert_eq!(
                            row["hash"], row["ignoredBaselineHash"],
                            "numeric_json_ignored_value_unchanged"
                        );
                        ignored += 1;
                    } else {
                        assert_eq!(
                            row["hash"], row["roundedHash"],
                            "numeric_json_binary64_rounded_hash"
                        );
                    }
                    accepted += 1;
                } else {
                    assert!(row["error"].is_object(), "numeric_json_source_refusal");
                    assert!(
                        result.is_err(),
                        "numeric_json_native_refusal {revision}/{shape}/{token}"
                    );
                    refused += 1;
                }
            }
        }
    }
    assert_eq!(row_index, 289, "numeric_json_native_completed_comparisons");
    assert_eq!(accepted, 239, "numeric_json_native_accepted_floor");
    assert_eq!(refused, 50, "numeric_json_native_refused_floor");
    assert_eq!(ignored, 85, "numeric_json_native_ignored_floor");
    assert_eq!(
        oracle["coercionRaw"],
        json!(COERCION_RAW),
        "numeric_json_coercion_declared_values"
    );
    assert_eq!(
        oracle["coercionKinds"],
        json!(COERCION_KINDS),
        "numeric_json_coercion_declared_types"
    );
    let coercions = oracle["coercionRows"].as_array().unwrap();
    assert_eq!(
        coercions.len(),
        102,
        "numeric_json_coercion_native_case_floor"
    );
    let mut index = 0;
    let mut coercion_accepted = 0;
    let mut coercion_refused = 0;
    for (revision, kind) in COERCION_KINDS {
        for (value_index, value_raw) in COERCION_RAW.iter().enumerate() {
            let row = coercions
                .get(index)
                .expect("numeric_json_coercion_native_index_coverage");
            index += 1;
            assert_eq!(
                row["id"],
                format!("coercion/{revision}/{kind}/{value_index}")
            );
            assert_eq!(row["revision"], revision);
            assert_eq!(row["kind"], kind);
            assert_eq!(row["valueRaw"], *value_raw);
            assert_eq!(row["address"], "0x1234");
            let expected = !matches!(value_index, 2 | 6 | 7)
                && !(value_index == 14 && kind != "i128")
                && !(value_index == 15 && matches!(kind, "u128" | "i128"));
            assert_eq!(
                row["accepted"], expected,
                "numeric_json_coercion_native_independent_acceptance"
            );
            let raw = row["raw"].as_str().unwrap();
            assert!(
                raw.contains(value_raw),
                "numeric_json_coercion_native_raw_value"
            );
            let result = hash_typed_data(raw, &address);
            if expected {
                let hash = result.unwrap_or_else(|error| panic!("numeric_json_coercion_native_accepted {revision}/{kind}/{value_index}: {error}"));
                assert_eq!(
                    format!("{hash:#x}"),
                    row["hash"].as_str().unwrap(),
                    "numeric_json_coercion_native_hash {revision}/{kind}/{value_index}"
                );
                coercion_accepted += 1;
            } else {
                assert!(
                    row["error"].is_object(),
                    "numeric_json_coercion_source_refusal"
                );
                assert!(
                    result.is_err(),
                    "numeric_json_coercion_native_refusal {revision}/{kind}/{value_index}"
                );
                coercion_refused += 1;
            }
        }
    }
    assert_eq!(
        index, 102,
        "numeric_json_coercion_native_completed_comparisons"
    );
    assert_eq!(
        coercion_accepted, 77,
        "numeric_json_coercion_native_accepted_floor"
    );
    assert_eq!(
        coercion_refused, 25,
        "numeric_json_coercion_native_refused_floor"
    );
    assert_eq!(row_index + index, 391, "numeric_json_native_combined_floor");
}

#[test]
fn actual_sdk_recursive_numeric_json_ingress() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_numeric_json_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "numeric_json_recursive_source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(oracle["rows"].as_array().unwrap().len(), 289);
    assert_eq!(oracle["coercionRows"].as_array().unwrap().len(), 102);
    let address = parse_felt("0x1234").unwrap();
    assert_eq!(
        oracle["recursiveInputs"],
        json!(RECURSIVE_INPUTS),
        "numeric_json_recursive_declared_inputs"
    );
    let recursive = oracle["recursiveRows"].as_array().unwrap();
    assert_eq!(recursive.len(), 10, "numeric_json_recursive_native_floor");
    for (index, (id, kind, pointer, value_raw)) in RECURSIVE_INPUTS.iter().enumerate() {
        let row = recursive
            .get(index)
            .expect("numeric_json_recursive_native_index_coverage");
        assert_eq!(row["id"], *id);
        assert_eq!(row["kind"], *kind);
        assert_eq!(row["pointer"], *pointer);
        assert_eq!(row["valueRaw"], *value_raw);
        assert_eq!(row["accepted"], true);
        assert_eq!(row["address"], "0x1234");
        let raw = row["raw"].as_str().unwrap();
        let data: Value = serde_json::from_str(raw).unwrap();
        let input: Value = serde_json::from_str(value_raw).unwrap();
        assert_eq!(
            data.pointer(pointer).unwrap(),
            &input,
            "numeric_json_recursive_native_raw_path"
        );
        let mut expected_types = json!({ "StarknetDomain": [
            {"name":"name","type":"shortstring"}, {"name":"version","type":"shortstring"},
            {"name":"chainId","type":"shortstring"}, {"name":"revision","type":"shortstring"}
        ] });
        let mut expected_message;
        if id.starts_with("i128/enum/") {
            expected_types["Choice"] = json!([{ "name": "Chosen", "type": "(i128)" }]);
            expected_types["Message"] =
                json!([{ "name": "choice", "type": "enum", "contains": "Choice" }]);
            expected_message = json!({ "choice": { "Chosen": [null] } });
        } else if id.starts_with("i128/merkle/") {
            expected_types["SignedBox"] = json!([{ "name": "x", "type": "i128" }]);
            expected_types["Message"] =
                json!([{ "name": "tree", "type": "merkletree", "contains": "SignedBox" }]);
            expected_message = json!({ "tree": [{ "x": null }, { "x": "23" }] });
        } else {
            let words = json!({ "low": "17", "high": "23" });
            let value = match *kind {
                "u256" => words,
                "TokenAmount" => json!({ "token_address": "0x123", "amount": words }),
                "NftId" => json!({ "collection_address": "0x123", "token_id": words }),
                _ => panic!("numeric_json_recursive_unknown_kind"),
            };
            if *id == "u256/nested-extra" {
                expected_types["Holder"] = json!([{ "name": "value", "type": "u256" }]);
                expected_types["Message"] = json!([{ "name": "holder", "type": "Holder" }]);
                expected_message = json!({ "holder": { "value": value } });
            } else {
                expected_types["Message"] = json!([{ "name": "value", "type": kind }]);
                expected_message = json!({ "value": value });
            }
            let parent = pointer
                .strip_prefix("/message")
                .unwrap()
                .rsplit_once('/')
                .unwrap()
                .0;
            expected_message.pointer_mut(parent).unwrap()["extra"] = Value::Null;
        }
        let message_pointer = pointer.strip_prefix("/message").unwrap();
        *expected_message.pointer_mut(message_pointer).unwrap() = input;
        assert_eq!(
            data["types"], expected_types,
            "numeric_json_recursive_native_original_types"
        );
        assert_eq!(
            data["message"], expected_message,
            "numeric_json_recursive_native_original_shape"
        );
        assert_eq!(
            data["domain"],
            json!({ "name": "Fixture", "version": "1", "chainId": "TEST", "revision": "1" })
        );
        assert_eq!(data["primaryType"], "Message");
        let hash = hash_typed_data(raw, &address)
            .unwrap_or_else(|error| panic!("numeric_json_recursive_native_accepted {id}: {error}"));
        assert_eq!(
            format!("{hash:#x}"),
            row["hash"].as_str().unwrap(),
            "numeric_json_recursive_native_hash {id}"
        );
        assert_eq!(
            row["hash"], row["baselineHash"],
            "numeric_json_recursive_source_baseline {id}"
        );
    }
    assert_eq!(
        289 + 102 + recursive.len(),
        401,
        "numeric_json_recursive_combined_floor"
    );
}
