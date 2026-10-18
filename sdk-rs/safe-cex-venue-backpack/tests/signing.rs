#[path = "support/retained_input_observations.rs"]
mod retained_input_observations;
use safe_cex_num::JsJsonValue;
use safe_cex_venue_backpack::signing::{build_signing_payload, build_sorted_query_string};
use serde_json::{Value, json};
use std::process::Command;
fn from_observation(value: &Value) -> JsJsonValue {
    match value[0].as_str().unwrap() {
        "undefined" => JsJsonValue::Undefined,
        "null" => JsJsonValue::Null,
        "boolean" => JsJsonValue::Bool(value[1].as_bool().unwrap()),
        "number" => JsJsonValue::Number(f64::from_bits(
            u64::from_str_radix(value[1].as_str().unwrap(), 16).unwrap(),
        )),
        "bigint" => JsJsonValue::BigInt(value[1].as_str().unwrap().parse().unwrap()),
        "string" => JsJsonValue::String(value[1].as_str().unwrap().into()),
        "array" => JsJsonValue::Array(
            value[1]
                .as_array()
                .unwrap()
                .iter()
                .map(from_observation)
                .collect(),
        ),
        "object" => JsJsonValue::Object(
            value[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    (
                        entry[0].as_str().unwrap().into(),
                        from_observation(&entry[1]),
                    )
                })
                .collect(),
        ),
        kind => panic!("unknown captured type {kind}"),
    }
}

fn bits(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
fn compare(result: Result<String, String>, case: &Value, label: &str, index: usize) {
    if case["ok"] == true {
        assert_eq!(
            result.expect("source accepted preimage"),
            case["value"],
            "{label} bytes case {index}"
        );
    } else {
        assert_eq!(
            result.expect_err("source refused preimage"),
            case["error"],
            "{label} error precedence case {index}"
        );
    }
}
#[test]
fn actual_typescript_signing_preimages() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/signing_oracle.cjs"
        ))
        .output()
        .expect("one actual TypeScript signing corpus");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in ["node", "arch", "platform"] {
        assert!(!corpus["runtime"][key].as_str().unwrap().is_empty());
    }
    eprintln!("TypeScript signing runtime: {}", corpus["runtime"]);
    for (name, count) in [("sorted", 41), ("payload", 2132), ("utf16", 2)] {
        assert_eq!(
            corpus[name].as_array().unwrap().len(),
            count,
            "SIGNING-CARDINALITY source {name}"
        );
    }
    let mut compared = [0; 3];
    for (i, case) in corpus["sorted"].as_array().unwrap().iter().enumerate() {
        compare(
            build_sorted_query_string(&from_observation(&case["input"])),
            case,
            "buildSortedQueryString",
            i,
        );
        compared[0] += 1;
    }
    for (i, case) in corpus["payload"].as_array().unwrap().iter().enumerate() {
        compare(
            build_signing_payload(
                case["instruction"].as_str().unwrap(),
                &from_observation(&case["input"]),
                bits(&case["timestamp"]),
                bits(&case["window"]),
            ),
            case,
            "buildSigningPayload",
            i,
        );
        compared[1] += 1;
    }
    for (i, case) in corpus["utf16"].as_array().unwrap().iter().enumerate() {
        let retained = retained_input_observations::capture(case["raw"].as_str().unwrap());
        let expected_units = match i {
            0 => vec!["field".encode_utf16().collect::<Vec<_>>(), vec![0xd800]],
            1 => vec![vec![0xdc00]],
            _ => panic!("unexpected UTF-16 signing case"),
        };
        assert_eq!(
            retained_input_observations::string_units(&retained),
            expected_units,
            "native retained preimage UTF-16 units {i}"
        );
        assert!(
            retained.into_value().is_err(),
            "legacy prepared signing JSON projection still refuses UTF-16 {i}"
        );
        compared[2] += 1;
    }
    assert_eq!(
        compared,
        [41, 2132, 2],
        "SIGNING-CARDINALITY completed comparisons"
    );
    assert_eq!(
        corpus["utf16"][0]["error"], "URIError: URI malformed",
        "source lone-surrogate value refusal"
    );
    assert_eq!(
        corpus["utf16"][1]["units"],
        json!([56320, 61, 49]),
        "source lone-surrogate key output"
    );
    assert!(
        safe_cex_num::numeric_ingress::js_from_str::<String>(
            corpus["instructionUtf16"]["raw"].as_str().unwrap()
        )
        .is_err(),
        "shared UTF8 instruction refusal"
    );
    let expected: Vec<_> = "instruction=go"
        .encode_utf16()
        .chain([55296])
        .chain("&timestamp=1&window=2".encode_utf16())
        .collect();
    assert_eq!(
        corpus["instructionUtf16"]["units"],
        json!(expected),
        "source lone-surrogate instruction output"
    );
    assert_eq!(
        corpus["outside"],
        json!({"value":"field=host","getterCalls":1,"toStringCalls":1}),
        "source executable coercion outside prepared domain"
    );
    eprintln!(
        "signing: {} query cases, {} payload cases",
        corpus["sorted"].as_array().unwrap().len(),
        corpus["payload"].as_array().unwrap().len()
    );
}
