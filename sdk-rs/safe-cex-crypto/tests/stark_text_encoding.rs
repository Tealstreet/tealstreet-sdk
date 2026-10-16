use safe_cex_crypto::starknet::{hash_typed_data, parse_felt};
use serde_json::{Value, json};
use std::process::Command;

const PAIRS: [(i32, &str); 7] = [
    (0, "felt"),
    (0, "shortstring"),
    (0, "string"),
    (0, "bool"),
    (1, "felt"),
    (1, "shortstring"),
    (1, "string"),
];
const BASE_INPUTS: [(&str, &str); 14] = [
    ("empty", r#""""#),
    ("numeric", r#""17""#),
    ("tab-only", r#""\t""#),
    ("nul", r#""A\u0000B""#),
    ("tab", r#""A\tB""#),
    ("one", r#""A\u0001B""#),
    ("lf-middle", r#""A\nB""#),
    ("cr-middle", r#""A\rB""#),
    ("lf-tail", r#""AB\n""#),
    ("cr-tail", r#""AB\r""#),
    ("ascii", r#""Hello""#),
    ("latin", r#""café""#),
    ("cjk", r#""雪""#),
    ("astral", r#""😀""#),
];

#[test]
fn actual_sdk_text_word_encoding() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stark_text_encoding_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "text_source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["pairs"],
        json!(PAIRS),
        "text_native_declared_revision_kinds"
    );
    let base: Vec<Value> = BASE_INPUTS
        .iter()
        .map(|(label, raw)| json!({ "label": label, "inputRaw": raw }))
        .collect();
    assert_eq!(
        oracle["baseInputs"],
        json!(base),
        "text_native_declared_inputs"
    );
    let first = "0123456789ABCDEFGHIJKLMNOPQRSTU";
    let second = "zyxwvutsrqponmlkjihgfedcba98765";
    assert_eq!(first.len(), 31);
    assert_eq!(second.len(), 31);
    let edges = [
        (1, "string", "edge31", first.to_owned(), true),
        (1, "string", "edge32", format!("{first}v"), true),
        (1, "string", "edge62", format!("{first}{second}"), true),
        (1, "string", "edge63", format!("{first}{second}!"), true),
        (1, "string", "nul31", format!("{}\0Z", "A".repeat(29)), true),
        (1, "string", "tab32", format!("{}\tQ", "B".repeat(30)), true),
        (1, "string", "lf31", format!("{}\n", "C".repeat(30)), true),
        (1, "string", "lf32", format!("{}\n", "D".repeat(31)), false),
        (0, "felt", "edge31", first.to_owned(), true),
        (0, "felt", "edge32", format!("{first}v"), false),
        (1, "felt", "edge31", first.to_owned(), true),
        (1, "felt", "edge32", format!("{first}v"), false),
    ];
    let expected_edges: Vec<Value> = edges.iter().map(|(revision, kind, label, text, accepted)| json!({ "revision": revision, "kind": kind, "label": label, "inputRaw": serde_json::to_string(text).unwrap(), "accepted": accepted })).collect();
    assert_eq!(
        oracle["edgeInputs"],
        json!(expected_edges),
        "text_native_declared_chunk_edges"
    );
    let mut expected = Vec::new();
    for (revision, kind) in PAIRS {
        for (index, (label, raw)) in BASE_INPUTS.iter().enumerate() {
            expected.push((
                revision,
                kind,
                *label,
                (*raw).to_owned(),
                !matches!(index, 6 | 7 | 11 | 12 | 13),
            ));
        }
    }
    for (revision, kind, label, text, accepted) in edges {
        expected.push((
            revision,
            kind,
            label,
            serde_json::to_string(&text).unwrap(),
            accepted,
        ));
    }
    assert_eq!(expected.len(), 110, "text_native_expected_case_floor");
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 110, "text_native_case_floor");
    let address = parse_felt("0x1234").unwrap();
    let mut accepted = 0;
    let mut refused = 0;
    for (index, (revision, kind, label, input_raw, should_accept)) in expected.iter().enumerate() {
        let row = rows.get(index).expect("text_native_index_coverage");
        assert_eq!(row["id"], format!("text/{revision}/{kind}/{label}"));
        assert_eq!(row["revision"], *revision);
        assert_eq!(row["kind"], *kind);
        assert_eq!(row["label"], *label);
        assert_eq!(row["inputRaw"], *input_raw);
        assert_eq!(
            row["accepted"], *should_accept,
            "text_native_independent_acceptance"
        );
        assert_eq!(row["address"], "0x1234");
        let raw = row["raw"].as_str().unwrap();
        let data: Value = serde_json::from_str(raw).unwrap();
        let text: String = serde_json::from_str(input_raw).unwrap();
        assert_eq!(data["message"], json!({ "x": text }));
        assert_eq!(
            data["types"]["Message"],
            json!([{ "name": "x", "type": kind }])
        );
        assert_eq!(
            row["units"],
            json!(text.encode_utf16().collect::<Vec<_>>()),
            "text_native_original_units"
        );
        let actual = hash_typed_data(raw, &address);
        if *should_accept {
            let hash = actual.unwrap_or_else(|error| {
                panic!("text_native_accepted {revision}/{kind}/{label}: {error}")
            });
            assert_eq!(
                format!("{hash:#x}"),
                row["hash"].as_str().unwrap(),
                "text_native_hash {revision}/{kind}/{label}"
            );
            if *revision == 1 && *kind == "string" {
                let words: Option<&[&str]> = match *label {
                    "empty" => Some(&["0", "0", "0"]),
                    "numeric" => Some(&["0", "12599", "2"]),
                    "tab-only" => Some(&["0", "9", "1"]),
                    "nul" => Some(&["0", "266306", "3"]),
                    "tab" => Some(&["0", "268610", "3"]),
                    "one" => Some(&["0", "266562", "3"]),
                    "lf-tail" | "cr-tail" => Some(&["0", "16706", "3"]),
                    "ascii" => Some(&["0", "310939249775", "5"]),
                    _ => None,
                };
                if let Some(words) = words {
                    assert_eq!(
                        row["byteWords"],
                        json!(words),
                        "text_native_pinned_bytearray_words {label}"
                    );
                }
            }
            accepted += 1;
        } else {
            assert!(row["error"].is_object(), "text_source_refusal");
            assert!(
                actual.is_err(),
                "text_native_refusal {revision}/{kind}/{label}"
            );
            refused += 1;
        }
    }
    assert_eq!(accepted, 72, "text_native_accepted_floor");
    assert_eq!(refused, 38, "text_native_refused_floor");
    assert_eq!(accepted + refused, 110, "text_native_completed_comparisons");
}
