use safe_cex_crypto::starknet::{hash_typed_data, parse_felt};
use serde_json::{Value, json};
use std::process::Command;

const INPUTS: [(&str, Option<&str>); 15] = [
    ("missing", None),
    ("null", Some("null")),
    ("string-zero", Some(r#""0""#)),
    ("string-one", Some(r#""1""#)),
    ("leading-zero", Some(r#""01""#)),
    ("plus-one", Some(r#""+1""#)),
    ("hex-one", Some(r#""0x1""#)),
    ("number-zero", Some("0")),
    ("number-one", Some("1")),
    ("true", Some("true")),
    ("false", Some("false")),
    ("empty-array", Some("[]")),
    ("one-array", Some("[1]")),
    ("object", Some("{}")),
    ("noncallable-to-string", Some(r#"{"toString":1}"#)),
];

#[test]
fn actual_sdk_original_domain_revision_recognition() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_domain_revision_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "domain_revision_source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["inputs"],
        json!(INPUTS),
        "domain_revision_native_independent_inventory"
    );
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 30, "domain_revision_native_case_floor");
    let address = parse_felt("0x1234").unwrap();
    let mut row_index = 0;
    let mut accepted = 0;
    let mut refused = 0;
    let mut hashes = [
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
    ];
    for revision in [0, 1] {
        for (input_index, (label, input_raw)) in INPUTS.iter().enumerate() {
            let row = rows
                .get(row_index)
                .expect("domain_revision_native_index_coverage");
            row_index += 1;
            assert_eq!(row["id"], format!("domain-revision/{revision}/{label}"));
            assert_eq!(row["revision"], revision);
            assert_eq!(row["label"], *label);
            assert_eq!(row["inputRaw"], json!(input_raw));
            assert_eq!(row["address"], "0x1234");
            assert_eq!(
                row["hasRevision"],
                input_raw.is_some(),
                "domain_revision_native_original_presence"
            );
            let should_accept = if revision == 0 {
                matches!(input_index, 0..=2)
            } else {
                matches!(input_index, 3 | 8 | 12)
            };
            assert_eq!(
                row["accepted"], should_accept,
                "domain_revision_native_independent_acceptance"
            );
            let raw = row["raw"].as_str().unwrap();
            let data: Value = serde_json::from_str(raw).unwrap();
            if let Some(input_raw) = input_raw {
                assert_eq!(
                    data["domain"]["revision"],
                    serde_json::from_str::<Value>(input_raw).unwrap(),
                    "domain_revision_native_original_value"
                );
            } else {
                assert!(data["domain"].get("revision").is_none());
            }
            assert_eq!(data["primaryType"], "Message");
            assert_eq!(data["message"], json!({ "x": "17" }));
            assert_eq!(
                data["types"]["Message"],
                json!([{ "name": "x", "type": "felt" }])
            );
            let name = if revision == 0 {
                "StarkNetDomain"
            } else {
                "StarknetDomain"
            };
            let kind = if revision == 0 { "felt" } else { "shortstring" };
            let mut fields = vec![
                json!({ "name": "name", "type": kind }),
                json!({ "name": "version", "type": kind }),
                json!({ "name": "chainId", "type": kind }),
            ];
            if revision == 1 {
                fields.push(json!({ "name": "revision", "type": "shortstring" }));
            }
            assert_eq!(
                data["types"][name],
                json!(fields),
                "domain_revision_native_original_types"
            );
            let actual = hash_typed_data(raw, &address);
            if should_accept {
                let hash = actual.unwrap_or_else(|error| {
                    panic!("domain_revision_native_accepted {revision}/{label}: {error}")
                });
                assert_eq!(
                    format!("{hash:#x}"),
                    row["hash"].as_str().unwrap(),
                    "domain_revision_native_hash {revision}/{label}"
                );
                hashes[revision].insert(hash.to_string());
                accepted += 1;
            } else {
                assert!(row["error"].is_object());
                assert!(
                    actual.is_err(),
                    "domain_revision_native_refusal {revision}/{label}"
                );
                refused += 1;
            }
        }
    }
    assert_eq!(
        row_index, 30,
        "domain_revision_native_completed_comparisons"
    );
    assert_eq!(accepted, 6, "domain_revision_native_accepted_floor");
    assert_eq!(refused, 24, "domain_revision_native_refused_floor");
    assert_eq!(
        hashes[0].len(),
        1,
        "domain_revision_native_v0_hash_identity"
    );
    assert_eq!(
        hashes[1].len(),
        1,
        "domain_revision_native_v1_hash_identity"
    );
}
