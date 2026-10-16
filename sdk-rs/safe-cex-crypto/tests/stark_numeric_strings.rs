use safe_cex_crypto::starknet::{hash_typed_data, parse_felt};
use serde_json::{Value, json};
use std::process::Command;
#[test]
fn actual_sdk_numeric_strings() {
    let spellings = [
        "1",
        "+1",
        "0X1",
        "0b1",
        "0B1",
        "0o1",
        "0O1",
        " 1 ",
        "\u{a0}1\u{feff}",
        "",
        " ",
        "\u{feff}",
        "+0x1",
        "-0x1",
        "+0b1",
        "1e0",
        "1.0",
        "1_0",
        "\u{85}1",
        "-0",
        "+0",
        "0x00",
        "01",
        "0O7",
        "0B101",
        "+001",
        "-1",
    ];
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_numeric_strings_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "numeric_string_source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["spellings"],
        json!(spellings),
        "numeric_string_declared_inputs"
    );
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 108, "numeric_string_case_floor");
    let mut index = 0;
    for revision in [0, 1] {
        for kind in ["felt", "shortstring"] {
            for spelling in spellings {
                let row = &rows[index];
                index += 1;
                assert_eq!(row["revision"], revision);
                assert_eq!(row["kind"], kind);
                assert_eq!(row["spelling"], spelling);
                let address = serde_json::from_value(json!("0x1234")).unwrap();
                let actual = hash_typed_data(row["raw"].as_str().unwrap(), &address);
                if let Some(expected) = row["hash"].as_str() {
                    assert_eq!(
                        format!("{:#x}", actual.unwrap()),
                        expected,
                        "numeric_string_native_hash rev{revision}/{kind}/{spelling:?}"
                    );
                } else {
                    assert!(
                        actual.is_err(),
                        "numeric_string_native_refusal rev{revision}/{kind}/{spelling:?}"
                    );
                }
            }
        }
    }
    assert_eq!(index, 108, "numeric_string_completed_comparisons");
    let range = oracle["range"].as_array().unwrap();
    assert_eq!(range.len(), 40, "numeric_string_range_floor");
    for row in range {
        assert!(
            row["error"].is_object(),
            "numeric_string_source_range_refusal"
        );
        let address = serde_json::from_value(json!("0x1234")).unwrap();
        assert!(
            hash_typed_data(row["raw"].as_str().unwrap(), &address).is_err(),
            "numeric_string_native_range_refusal {} {}",
            row["shape"],
            row["spelling"]
        );
    }
}

#[test]
fn original_account_address_range() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_numeric_strings_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "account_source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = oracle["accounts"].as_array().unwrap();
    assert_eq!(rows.len(), 4, "account_native_case_floor");
    let expected = [
        "0x0",
        "0x800000000000011000000000000000000000000000000000000000000000000",
        "0x800000000000011000000000000000000000000000000000000000000000001",
        "0x800000000000011000000000000000000000000000000000000000000000002",
    ];
    for i in 0..4 {
        assert_eq!(
            rows[i]["address"], expected[i],
            "account_native_declared_inputs"
        );
        let actual = parse_felt(expected[i])
            .and_then(|address| hash_typed_data(rows[i]["raw"].as_str().unwrap(), &address));
        if i >= 2 {
            assert!(rows[i]["error"].is_object());
            assert!(actual.is_err(), "account_native_refusal {i}");
        } else {
            assert_eq!(
                format!("{:#x}", actual.unwrap()),
                rows[i]["hash"].as_str().unwrap(),
                "account_native_hash {i}"
            );
        }
    }
}

#[test]
fn original_domain_field_order() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_numeric_strings_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "domain_source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = oracle["domains"].as_array().unwrap();
    let permutations = [
        ["name", "version", "chainId"],
        ["name", "chainId", "version"],
        ["version", "name", "chainId"],
        ["version", "chainId", "name"],
        ["chainId", "name", "version"],
        ["chainId", "version", "name"],
    ];
    assert_eq!(rows.len(), 6, "domain_native_case_floor");
    let mut hashes = std::collections::BTreeSet::new();
    for i in 0..6 {
        assert_eq!(
            rows[i]["fields"],
            json!(permutations[i]),
            "domain_native_declared_order"
        );
        let hash = hash_typed_data(
            rows[i]["raw"].as_str().unwrap(),
            &parse_felt("0x1234").unwrap(),
        )
        .unwrap();
        assert_eq!(
            format!("{hash:#x}"),
            rows[i]["hash"].as_str().unwrap(),
            "domain_native_original_hash {i}"
        );
        hashes.insert(hash.to_string());
    }
    assert_eq!(hashes.len(), 6, "domain_native_distinct_hashes");
}
