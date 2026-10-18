#[path = "support/retained_input_observations.rs"]
mod retained_input_observations;
include!("cardinality.inc");
use safe_cex_num::JsJsonValue;
use safe_cex_venue_backpack::api::*;
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
fn observe(value: &JsJsonValue) -> Value {
    match value {
        JsJsonValue::Undefined => json!(["undefined"]),
        JsJsonValue::Null => json!(["null"]),
        JsJsonValue::Bool(v) => json!(["boolean", v]),
        JsJsonValue::Number(v) => json!([
            "number",
            format!(
                "{:016x}",
                if v.is_nan() {
                    0x7ff8000000000000
                } else {
                    v.to_bits()
                }
            )
        ]),
        JsJsonValue::String(v) => json!(["string", v]),
        JsJsonValue::BigInt(v) => json!(["bigint", v.to_string()]),
        JsJsonValue::Array(v) => json!(["array", v.iter().map(observe).collect::<Vec<_>>()]),
        JsJsonValue::Object(v) => json!([
            "object",
            v.iter()
                .map(|(key, value)| json!([key, observe(value)]))
                .collect::<Vec<_>>()
        ]),
    }
}
#[test]
fn actual_typescript_api_helpers_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/api_helpers_oracle.cjs"
        ))
        .output()
        .expect("execute actual TypeScript API helper corpus once");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).expect("API oracle corpus");
    for key in ["node", "arch", "platform"] {
        assert!(
            !corpus["runtime"][key]
                .as_str()
                .expect("runtime metadata")
                .is_empty()
        );
    }
    eprintln!("TypeScript oracle runtime: {}", corpus["runtime"]);
    for (name, count) in [
        ("auth", 101),
        ("records", 30),
        ("collect", 900),
        ("publicCases", 172),
        ("instructions", 1548),
        ("utf16", 2),
        ("utf16Data", 3),
    ] {
        assert_eq!(
            corpus[name].as_array().unwrap().len(),
            count,
            "API-CARDINALITY source {name}"
        );
    }
    let mut compared = [0; 7];
    for case in corpus["auth"].as_array().unwrap() {
        let input = case["input"]
            .as_str()
            .map(|s| f64::from_bits(u64::from_str_radix(s, 16).unwrap()));
        let actual = resolve_backpack_auth_window(input);
        assert_eq!(
            format!("{:016x}", actual.to_bits()),
            case["expected"],
            "resolveBackpackAuthWindow bits {input:?}"
        );
        compared[0] += 1;
    }
    for case in corpus["records"].as_array().unwrap() {
        assert_eq!(
            is_record(&from_observation(&case["input"])),
            case["expected"],
            "isRecord typeof {case}"
        );
        compared[1] += 1;
    }
    for (index, case) in corpus["collect"].as_array().unwrap().iter().enumerate() {
        let result = collect_auth_params(
            &from_observation(&case["query"]),
            &from_observation(&case["body"]),
        );
        if case["ok"] == true {
            let native = observe(&result.expect("source accepted collection"));
            assert_native_cardinality(
                &case["expected"],
                &native,
                &format!("collectAuthParams case {index}"),
            );
            assert_eq!(
                native, case["expected"],
                "collectAuthParams native values/order case {index}"
            );
        } else {
            assert_eq!(
                result.expect_err("source refused collection"),
                case["error"],
                "collectAuthParams refusal precedence case {index}"
            );
        }
        compared[2] += 1;
    }
    for case in corpus["publicCases"].as_array().unwrap() {
        let url = case["url"].as_str().unwrap();
        assert_eq!(
            is_public_endpoint(url),
            case["expected"],
            "isPublicEndpoint substring {url}"
        );
        compared[3] += 1;
    }
    for case in corpus["instructions"].as_array().unwrap() {
        let method = case["method"].as_str().unwrap();
        let url = case["url"].as_str().unwrap();
        assert_eq!(
            get_instruction(method, url),
            case["expected"],
            "getInstruction priority {method} {url}"
        );
        compared[4] += 1;
    }
    // Record both sides of the deliberately owned result boundary.
    for (before, after, same) in [
        ("arrayBefore", "arrayAfter", "arraySame"),
        ("nestedBefore", "nestedAfter", "nestedSame"),
    ] {
        assert_eq!(
            corpus["alias"][same], true,
            "actual TypeScript alias {same}"
        );
        let mut input = from_observation(&corpus["alias"][before]);
        let snapshot = collect_auth_params(&JsJsonValue::Undefined, &input).unwrap();
        fn mutate_nested(value: &mut JsJsonValue, replacement: f64) {
            match value {
                JsJsonValue::Number(n) => *n = replacement,
                JsJsonValue::Array(rows) => {
                    rows.iter_mut().for_each(|v| mutate_nested(v, replacement))
                }
                JsJsonValue::Object(entries) => entries
                    .iter_mut()
                    .for_each(|(_, v)| mutate_nested(v, replacement)),
                _ => panic!("unexpected alias fixture"),
            }
        }
        mutate_nested(&mut input, if before == "arrayBefore" { 2.0 } else { 4.0 });
        assert_eq!(
            observe(&input),
            corpus["alias"][after],
            "original graph mutation {before}"
        );
        assert_ne!(
            observe(&input),
            observe(&snapshot),
            "owned snapshot stays frozen {before}"
        );
        assert_eq!(
            observe(&snapshot),
            corpus["alias"][before],
            "owned snapshot initial view {before}"
        );
    }
    assert_eq!(
        corpus["outside"],
        json!({"dateIsRecord":true,"functionIsRecord":false,"getterCalls":1,"getterOutput":["object",[["field",["number","4022000000000000"]]]],"symbolKeys":1,"sparseSame":true,"sparseHasZero":false}),
        "source behavior outside prepared-data domain"
    );
    for case in corpus["utf16"].as_array().unwrap() {
        assert!(
            safe_cex_num::numeric_ingress::js_from_str::<String>(case["raw"].as_str().unwrap())
                .is_err(),
            "shared UTF-8 URL refusal"
        );
        assert!(
            case["units"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| (0xd800..=0xdfff).contains(&v.as_u64().unwrap())),
            "source URL retains lone UTF-16 unit"
        );
        compared[5] += 1;
    }
    for (index, case) in corpus["utf16Data"].as_array().unwrap().iter().enumerate() {
        let retained = retained_input_observations::capture(case["raw"].as_str().unwrap());
        assert_eq!(
            json!(retained_input_observations::string_units(&retained)),
            case["units"],
            "native retained prepared-data UTF-16 units {index}"
        );
        assert!(
            retained.into_value().is_err(),
            "legacy prepared-data JSON projection still refuses UTF-16 {index}"
        );
        assert_eq!(
            case["record"],
            index != 0,
            "source surrogate classification {index}"
        );
        let expected_output = match index {
            0 => json!([]),
            1 => json!([{"key":[56320],"value":["number","3ff0000000000000"]}]),
            2 => json!([{"key":[102,105,101,108,100],"value":["units",[55296]]}]),
            _ => panic!("unexpected surrogate case"),
        };
        assert_eq!(
            case["output"], expected_output,
            "source surrogate collection output {index}"
        );
        assert_eq!(
            case["resultRecord"], true,
            "source surrogate collection {index}"
        );
        assert!(
            case["units"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|v| v.as_array().unwrap())
                .any(|v| (0xd800..=0xdfff).contains(&v.as_u64().unwrap())),
            "source prepared-data retains UTF-16 unit {index}"
        );
        compared[6] += 1;
    }
    assert_eq!(
        compared,
        [101, 30, 900, 172, 1548, 2, 3],
        "API-CARDINALITY completed comparisons"
    );
    eprintln!(
        "API helpers: {} window, {} classification, {} collection, {} public URL, {} instruction cases",
        corpus["auth"].as_array().unwrap().len(),
        corpus["records"].as_array().unwrap().len(),
        corpus["collect"].as_array().unwrap().len(),
        corpus["publicCases"].as_array().unwrap().len(),
        corpus["instructions"].as_array().unwrap().len()
    );
}
