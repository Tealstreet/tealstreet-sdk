use safe_cex_crypto::starknet::{hash_typed_data, parse_felt};
use serde_json::{Value, json};
use std::process::Command;
#[test]
fn actual_sdk_nested_empty_tuples() {
    let shapes = [
        "scalar", "nested", "array", "enum", "merkle1", "merkle2", "merkle3",
    ];
    let variants = ["Zero", "Last"];
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_empty_tuples_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "empty_tuple_source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(oracle["shapes"], json!(shapes));
    assert_eq!(oracle["variants"], json!(variants));
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14, "empty_tuple_native_case_floor");
    let mut index = 0;
    let mut hashes = std::collections::BTreeSet::new();
    for shape in shapes {
        for variant in variants {
            let row = &cases[index];
            index += 1;
            assert_eq!(row["shape"], shape, "empty_tuple_native_declared_shape");
            assert_eq!(row["variant"], variant, "empty_tuple_native_declared_index");
            let result =
                hash_typed_data(row["raw"].as_str().unwrap(), &parse_felt("0x1234").unwrap());
            assert!(
                result.is_ok(),
                "empty_tuple_native_accepted {shape}/{variant}: {result:?}"
            );
            let hash = result.unwrap();
            assert_eq!(
                format!("{hash:#x}"),
                row["hash"].as_str().unwrap(),
                "empty_tuple_native_hash {shape}/{variant}"
            );
            hashes.insert(hash.to_string());
        }
    }
    assert_eq!(index, 14, "empty_tuple_native_completed_comparisons");
    assert_eq!(hashes.len(), 14, "empty_tuple_native_distinct_hashes");
}
