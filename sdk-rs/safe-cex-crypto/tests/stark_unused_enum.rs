use safe_cex_crypto::starknet::{hash_typed_data, parse_felt};
use serde_json::{Value, json};
use std::process::Command;

const V1_VARIANTS: [&str; 14] = [
    "( )",
    "(\t)",
    "(\u{feff})",
    "(\u{85})",
    "(felt, felt)",
    "(felt,\tfelt)",
    "(NoSuchType)",
    "(,NoSuchType)",
    "(\n)",
    "(\r)",
    "(\u{2028})",
    "(\u{2029})",
    "NoSuchType",
    " () ",
];

#[test]
fn original_unused_enum_declarations() {
    let tuples = [
        None,
        Some("()"),
        Some("(,)"),
        Some("(felt,)"),
        Some("(,felt)"),
    ];
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_unused_enum_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "unused_enum_source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["tuples"],
        json!(tuples),
        "unused_enum_declared_inputs"
    );
    assert_eq!(
        source["variantTypes"],
        json!(V1_VARIANTS),
        "unused_enum_v1_declared_inputs"
    );
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 33, "unused_enum_native_case_floor");
    let address = parse_felt("0x1234").unwrap();
    let mut hashes = std::collections::BTreeSet::new();
    for (index, tuple) in tuples.iter().enumerate() {
        let row = rows
            .get(index)
            .expect("unused_enum_expected_native_index_coverage");
        assert_eq!(row["tuple"], json!(tuple));
        assert_eq!(row["revision"], 0);
        assert_eq!(row["selected"], false);
        assert_eq!(row["accepted"], true);
        assert_eq!(row["address"], "0x1234");
        assert_eq!(
            row["typeEncoding"], "Message(x:felt)",
            "unused_enum_original_revision_zero_encoding"
        );
        let raw = row["raw"].as_str().unwrap();
        let data: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(data["primaryType"], "Message");
        assert_eq!(data["message"], json!({ "x": "17" }));
        assert_eq!(
            data["types"]["Message"],
            json!([{ "name": "x", "type": "felt" }])
        );
        if let Some(tuple) = tuple {
            assert_eq!(
                data["types"]["Unused"],
                json!([{ "name": "Never", "type": tuple }]),
                "unused_enum_native_original_declaration"
            );
        } else {
            assert!(data["types"].get("Unused").is_none());
        }
        let result = hash_typed_data(raw, &address);
        let hash = result
            .unwrap_or_else(|error| panic!("unused_enum_native_accepted case {index}: {error}"));
        assert_eq!(
            format!("{hash:#x}"),
            row["hash"].as_str().unwrap(),
            "unused_enum_native_hash case {index}"
        );
        hashes.insert(hash.to_string());
    }
    assert_eq!(hashes.len(), 1, "unused_enum_native_unchanged_hash");
    let mut index = 5;
    let mut unused_hashes = std::collections::BTreeSet::new();
    let mut refusals = 0;
    for tuple in V1_VARIANTS {
        for selected in [false, true] {
            let row = rows.get(index).expect("unused_enum_v1_index_coverage");
            index += 1;
            assert_eq!(row["revision"], 1);
            assert_eq!(row["tuple"], tuple, "unused_enum_v1_original_type");
            assert_eq!(row["selected"], selected);
            assert_eq!(row["accepted"], !selected);
            assert_eq!(row["address"], "0x1234");
            let raw = row["raw"].as_str().unwrap();
            let data: Value = serde_json::from_str(raw).unwrap();
            assert_eq!(
                data["types"]["Choice"][1],
                json!({ "name": "Dormant", "type": tuple })
            );
            assert_eq!(data["message"]["marker"], "99");
            if selected {
                assert!(data["message"]["choice"].get("Dormant").is_some());
            } else {
                assert_eq!(data["message"]["choice"], json!({ "Live": ["17"] }));
            }
            let actual = hash_typed_data(raw, &address);
            if selected {
                assert!(
                    row["error"].is_object(),
                    "unused_enum_source_selected_refusal"
                );
                assert!(row["hash"].is_null());
                assert!(
                    actual.is_err(),
                    "unused_enum_native_selected_refusal {tuple:?}"
                );
                refusals += 1;
            } else {
                assert!(row["error"].is_null());
                let hash = actual.unwrap_or_else(|error| {
                    panic!("unused_enum_native_v1_accepted {tuple:?}: {error}")
                });
                assert_eq!(
                    format!("{hash:#x}"),
                    row["hash"].as_str().unwrap(),
                    "unused_enum_native_v1_hash {tuple:?}"
                );
                unused_hashes.insert(hash.to_string());
            }
        }
    }
    assert_eq!(index, 33, "unused_enum_native_completed_comparisons");
    assert_eq!(
        unused_hashes.len(),
        14,
        "unused_enum_native_original_type_hashes"
    );
    assert_eq!(
        refusals, 14,
        "unused_enum_native_completed_selected_refusals"
    );
}
