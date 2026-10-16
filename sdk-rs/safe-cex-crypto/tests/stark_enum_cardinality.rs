use safe_cex_crypto::starknet::hash_typed_data;
use serde_json::Value;
#[test]
fn enum_cardinality_matches_pinned_sdk_at_public_hash_entry() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/stark-enum-cardinality.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Stark enum SDK probe {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "STARK-ENUM source total");
    let mut comparisons = 0;
    for index in 0..12 {
        let row = cases
            .get(index)
            .expect("STARK-ENUM expected/native index coverage");
        assert_eq!(
            row["tuple"],
            ["()", "(felt)", "(felt,felt)"][index / 4],
            "STARK-ENUM source tuple inventory"
        );
        assert_eq!(
            row["values"].as_array().unwrap().len(),
            index % 4,
            "STARK-ENUM source payload count"
        );
        let actual = hash_typed_data(
            row["raw"].as_str().unwrap(),
            &starknet_crypto::Felt::from_hex("0x1234").unwrap(),
        );
        let declared = index / 4;
        let supplied = index % 4;
        // Exact ()/[] now delegates to the source-matching native composition.
        if supplied != declared {
            assert_eq!(
                actual.unwrap_err(),
                "Stark enum tuple payload count does not match declared types",
                "STARK-ENUM declared/native payload count"
            );
            if supplied > declared {
                assert!(
                    row.get("hash").is_some(),
                    "SDK ignores extra tuple values; Rust refusal is CHOSEN"
                );
            } else {
                assert!(
                    row.get("error").is_some(),
                    "SDK refuses missing declared values"
                );
            }
        } else {
            assert_eq!(
                format!("{:#x}", actual.unwrap()),
                row["hash"].as_str().unwrap(),
                "STARK-ENUM exact-arity positive hash"
            );
        }
        comparisons += 1;
    }
    assert_eq!(comparisons, 12, "STARK-ENUM native comparison total");
}
