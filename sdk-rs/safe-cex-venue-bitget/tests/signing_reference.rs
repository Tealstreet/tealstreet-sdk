//! TypeScript reference capture, not a Rust createAPI parity claim.
//! This pins the two observable views, including replacement-token corruption.
use serde_json::Value;

#[test]
#[ignore = "requires Node and TypeScript plus the repository JS dependencies"]
fn live_ts_signing_reference() {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/signing-capture.cjs"
        ))
        .output()
        .expect("execute Node");
    assert!(
        result.status.success(),
        "signing capture failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let actual: Value = serde_json::from_slice(&result.stdout).unwrap();
    let expected: Value = serde_json::from_str(include_str!("signing-reference.json")).unwrap();
    let json_data = &actual;
    let actual = actual["cases"].as_array().unwrap();
    let expected = expected["cases"].as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "TS signing reference inventory"
    );
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(
            actual, expected,
            "TS createAPI/toJsonString reference {}",
            expected["name"]
        );
    }
    verify_json_string(json_data);
}

#[test]
#[ignore = "requires Node and TypeScript plus repository JS dependencies"]
fn signing_reference_rejects_literal_replacement() {
    for (mutation, name) in [
        ("literal-replacement", "key_dollar_dollar"),
        ("all-replacements", "key_repeated_in_value"),
    ] {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "live_ts_signing_reference",
                "--exact",
                "--ignored",
                "--nocapture",
            ])
            .env("BITGET_SIGNING_MUTATION", mutation)
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(101),
            "replacement mutation must fail reference assertion: {mutation}"
        );
        let diagnostics = String::from_utf8_lossy(&result.stdout).to_string()
            + &String::from_utf8_lossy(&result.stderr);
        assert!(
            diagnostics.contains(name),
            "named dollar substitution witness: {diagnostics}"
        );
    }
}

fn decode_json_input(input: &Value) -> safe_cex_num::JsJsonValue {
    use safe_cex_num::JsJsonValue as J;
    match input[0].as_str().unwrap() {
        "undefined" => J::Undefined,
        "null" => J::Null,
        "bool" => J::Bool(input[1].as_bool().unwrap()),
        "number" => J::Number(safe_cex_num::number_from_string(input[1].as_str().unwrap())),
        "bigint" => J::BigInt(input[1].as_str().unwrap().parse().unwrap()),
        "string" => J::String(input[1].as_str().unwrap().into()),
        "array" => J::Array(
            input[1]
                .as_array()
                .unwrap()
                .iter()
                .map(decode_json_input)
                .collect(),
        ),
        "object" => J::Object(
            input[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    (
                        entry[0].as_str().unwrap().into(),
                        decode_json_input(&entry[1]),
                    )
                })
                .collect(),
        ),
        tag => panic!("unexpected fixture tag {tag}"),
    }
}

fn verify_json_string(data: &Value) {
    use safe_cex_num::JsJsonValue;
    use safe_cex_venue_bitget::api::to_json_string;
    for case in data["toJsonCases"].as_array().unwrap() {
        let input = decode_json_input(&case["input"]);
        let project = |value| match to_json_string(value) {
            Ok(value) => serde_json::json!({"value":value}),
            Err(error) => serde_json::json!({"error":{"name":error.name,"message":error.message}}),
        };
        assert_eq!(project(&input), case["result"], "{}", case["name"]);
        if let JsJsonValue::Object(entries) = &input {
            let duplicate = JsJsonValue::Object(entries.iter().chain(entries).cloned().collect());
            assert_eq!(
                project(&duplicate),
                case["result"],
                "{} duplicate assignment keys",
                case["name"]
            );
        }
    }
    for case in data["toJsonSurrogates"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(case["escaped"].as_str().unwrap()).is_err(),
            "toJsonString shared UTF8 ingress refusal"
        );
        assert!(
            case["result"].is_string(),
            "toJsonString source accepts lone surrogate"
        );
    }
}

#[test]
fn frozen_json_string() {
    verify_json_string(&serde_json::from_str(include_str!("signing-reference.json")).unwrap());
}

#[test]
#[ignore = "requires Node and TypeScript plus repository JS dependencies"]
fn live_json_string() {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/signing-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    verify_json_string(&serde_json::from_slice(&result.stdout).unwrap());
}

#[test]
#[ignore = "requires Node and TypeScript plus repository JS dependencies"]
fn json_string_oracle_rejects_replace_all() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["live_json_string", "--exact", "--ignored", "--nocapture"])
        .env("BITGET_SIGNING_MUTATION", "all-replacements")
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(101),
        "toJsonString replace-all mutation must fail"
    );
    let diagnostics = String::from_utf8_lossy(&result.stdout).to_string()
        + &String::from_utf8_lossy(&result.stderr);
    assert!(
        diagnostics.contains("toJsonString:"),
        "toJsonString named failure: {diagnostics}"
    );
}
