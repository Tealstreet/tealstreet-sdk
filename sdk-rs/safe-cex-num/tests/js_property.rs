#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::{js_array_index, js_object_or_array_entry};
use serde_json::Value;

#[test]
fn canonical_property_entries_match_live_node() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_property.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    source_cardinality::for_each(rows, 5135, "property", |row| {
        let key = row["key"].as_str().unwrap();
        assert_eq!(
            js_array_index(key).map(u64::from),
            row["index"].as_u64(),
            "canonical array index {key:?}"
        );
        for kind in ["array", "object"] {
            let actual = js_object_or_array_entry(&corpus[kind], key);
            let expected = &row[kind];
            assert_eq!(
                actual.is_some(),
                expected["found"].as_bool().unwrap(),
                "data entry presence {kind} {key:?}"
            );
            if let Some(actual) = actual {
                assert_eq!(
                    actual, &expected["value"],
                    "data entry value {kind} {key:?}"
                );
            }
        }
    });
    assert_eq!(
        js_object_or_array_entry(&corpus["array"], "0")
            .and_then(|value| js_object_or_array_entry(value, "items")),
        Some(&corpus["nested"]),
        "nested string-zero path"
    );
}
