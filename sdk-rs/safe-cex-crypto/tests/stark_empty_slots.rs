use safe_cex_crypto::starknet::{hash_typed_data, parse_felt};
use serde_json::{Value, json};
use std::process::Command;

const TUPLES: [&str; 3] = ["(,)", "(felt,)", "(,felt)"];
const SHAPES: [&str; 5] = ["scalar", "nested", "array", "enum", "merkle"];
const IGNORED_KINDS: [&str; 3] = ["null", "object", "huge"];
const SCHEMAS: [&str; 5] = [
    "unselected",
    "presets",
    "dependencies",
    "utf16-order",
    "raw-quote",
];
const WHITESPACE: [&str; 8] = [
    "( )",
    "(\t)",
    "(\u{feff})",
    "(\u{85})",
    "(felt, felt)",
    "(felt,\tfelt)",
    " () ",
    "(, felt)",
];

#[test]
fn actual_sdk_partial_empty_slots() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_empty_slots_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "empty_slots_source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(oracle["tuples"], json!(TUPLES));
    assert_eq!(oracle["shapes"], json!(SHAPES));
    assert_eq!(oracle["ignoredKinds"], json!(IGNORED_KINDS));
    assert_eq!(oracle["variants"], json!([0, 1]));
    assert_eq!(oracle["schemas"], json!(SCHEMAS));
    assert_eq!(oracle["whitespace"], json!(WHITESPACE));

    let counters = oracle["counterMeasurements"].as_array().unwrap();
    assert_eq!(counters.len(), 3, "empty_slots_countermeasurement_floor");
    for (index, tuple) in TUPLES.iter().enumerate() {
        let counter = &counters[index];
        assert_eq!(counter["tuple"], *tuple);
        assert_eq!(
            counter["hash"], counter["zeroHash"],
            "empty_slots_ignored_value_preserves_hash"
        );
        assert_ne!(
            counter["hash"], counter["normalizedHash"],
            "empty_slots_original_declaration_changes_hash"
        );
    }

    let mut expected = Vec::new();
    for tuple in TUPLES {
        for shape in SHAPES {
            for kind in IGNORED_KINDS {
                for index in [0, 1] {
                    expected.push((format!("slot/{tuple}/{shape}/{kind}/{index}"), tuple, true));
                }
            }
        }
    }
    assert_eq!(expected.len(), 90, "empty_slots_native_shape_floor");
    for tuple in TUPLES {
        for schema in SCHEMAS {
            expected.push((format!("schema/{tuple}/{schema}"), tuple, true));
        }
    }
    assert_eq!(expected.len(), 105, "empty_slots_native_accepted_floor");
    for tuple in WHITESPACE {
        for length in [0, 1, 2] {
            expected.push((format!("whitespace/{tuple}/{length}"), tuple, false));
        }
    }
    assert_eq!(expected.len(), 129, "empty_slots_native_declared_floor");
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 129, "empty_slots_source_case_floor");
    let mut unique = std::collections::BTreeSet::new();
    let mut accepted = 0;
    let mut refused = 0;
    let mut unselected_hashes = std::collections::BTreeSet::new();
    for (index, (id, tuple, should_accept)) in expected.iter().enumerate() {
        let row = rows
            .get(index)
            .expect("empty_slots_expected_native_index_coverage");
        assert_eq!(
            row["id"],
            id.as_str(),
            "empty_slots_native_inventory {index}"
        );
        assert!(unique.insert(id), "empty_slots_native_unique_inventory");
        assert_eq!(row["tuple"], *tuple);
        assert_eq!(row["accepted"], *should_accept);
        assert_eq!(row["address"], "0x1234");
        let raw = row["raw"].as_str().unwrap();
        let data: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(data["primaryType"], "Message");
        let choice = data["types"]["Choice"].as_array().unwrap();
        let variant = choice
            .iter()
            .find(|variant| {
                variant["name"]
                    == if id.starts_with("schema/") && id.ends_with("/unselected") {
                        "Dormant"
                    } else {
                        "Chosen"
                    }
            })
            .unwrap();
        assert_eq!(variant["type"], *tuple, "empty_slots_native_raw_tuple {id}");
        let actual = hash_typed_data(raw, &parse_felt("0x1234").unwrap());
        if *should_accept {
            assert!(row["error"].is_null(), "empty_slots_source_accepted {id}");
            let hash =
                actual.unwrap_or_else(|error| panic!("empty_slots_native_accepted {id}: {error}"));
            assert_eq!(
                format!("{hash:#x}"),
                row["hash"].as_str().unwrap(),
                "empty_slot_native_hash {id}"
            );
            assert!(row["typeEncoding"].is_string());
            assert!(row["typeHash"].is_string());
            if id.starts_with("schema/") && id.ends_with("/unselected") {
                unselected_hashes.insert(hash.to_string());
            }
            accepted += 1;
        } else {
            assert!(row["hash"].is_null(), "empty_slots_source_refused {id}");
            assert!(row["error"].is_object());
            assert!(actual.is_err(), "empty_slots_native_refusal {id}");
            refused += 1;
        }
    }
    assert_eq!(unique.len(), 129, "empty_slots_native_completed_inventory");
    assert_eq!(accepted, 105, "empty_slots_native_completed_acceptance");
    assert_eq!(refused, 24, "empty_slots_native_completed_refusals");
    assert_eq!(
        unselected_hashes.len(),
        3,
        "empty_slots_native_unselected_variants_authenticate"
    );
}
