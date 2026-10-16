use safe_cex_num::{JsJsonValue, js_json_stringify, numeric_ingress::js_deserialize_passthrough};
use safe_cex_venue_paradex::request_helpers::{describe_error, signer_headers};
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};

#[test]
fn actual_typescript_request_helpers() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/request_helpers_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source_execution {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(corpus["header"]["arch"], arch, "capture_architecture");
    assert!(
        corpus["header"]["nodeMajor"].as_u64().unwrap() > 0,
        "capture_node_major"
    );
    println!("capture_header {}", corpus["header"]);
    assert_eq!(
        corpus["cases"].as_array().unwrap().len(),
        25,
        "describe_source_census"
    );
    assert_eq!(
        corpus["headers"].as_array().unwrap().len(),
        5,
        "headers_source_census"
    );
    let mut compared = [0; 2];
    for case in corpus["cases"].as_array().unwrap() {
        let trace = RefCell::new(Vec::new());
        let original = Rc::new("json-error");
        let replacement = Rc::new("replacement");
        let result: Result<Option<Vec<u16>>, Rc<&str>> = describe_error(
            || {
                trace.borrow_mut().push("json");
                match case["kind"].as_str().unwrap() {
                    "json" => {
                        let value: Value =
                            js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(
                                case["input"].as_str().unwrap(),
                            ))
                            .unwrap();
                        js_json_stringify(&JsJsonValue::from(value))
                            .map(|v| v.map(|s| s.encode_utf16().collect()))
                            .map_err(|_| original.clone())
                    }
                    "undefined" => Ok(None),
                    _ => Err(original.clone()),
                }
            },
            || {
                trace.borrow_mut().push("string");
                match case["kind"].as_str().unwrap() {
                    "replacement" => Err(replacement.clone()),
                    "utf16" => Ok(vec![0xd800]),
                    "fallback" => Ok(case["input"].as_str().unwrap().encode_utf16().collect()),
                    _ => panic!("unexpected fallback {}", case["label"]),
                }
            },
        );
        match result {
            Ok(value) => {
                let native = value.map_or_else(
                    || json!({"type":"undefined"}),
                    |units| json!({"type":"string","units":units}),
                );
                assert_eq!(
                    json!({"ok":true,"value":native}),
                    case["outcome"],
                    "describe_native_outcome {}",
                    case["label"]
                );
            }
            Err(error) => {
                assert!(
                    Rc::ptr_eq(&error, &replacement),
                    "describe_replacement_identity"
                );
                assert_eq!(
                    case["outcome"],
                    json!({"ok":false,"identity":"replacement"}),
                    "describe_source_throw"
                );
            }
        }
        assert_eq!(
            json!(*trace.borrow()),
            case["trace"],
            "describe_effect_order {}",
            case["label"]
        );
        compared[0] += 1;
    }
    let mut outputs = 0;
    for case in corpus["headers"].as_array().unwrap() {
        let input: Option<Vec<(String, String)>> =
            serde_json::from_value(case["input"].clone()).unwrap();
        let headers = input.map(RefCell::new);
        let result = signer_headers(headers.as_ref());
        let expected_len = usize::from(headers.is_some());
        assert_eq!(
            case["length"].as_u64().unwrap() as usize,
            expected_len,
            "headers_source_per_case"
        );
        assert_eq!(result.len(), expected_len, "headers_native_cardinality");
        if let Some(header) = headers.as_ref() {
            assert!(
                std::ptr::eq(result[0].headers, header),
                "headers_native_identity"
            );
            assert_eq!(case["same"], true, "headers_source_identity");
            assert_eq!(
                case["before"][0]["keys"],
                json!(["headers"]),
                "headers_source_config_shape"
            );
            assert_eq!(
                json!(*result[0].headers.borrow()),
                case["before"][0]["headers"],
                "headers_native_entries"
            );
            header
                .borrow_mut()
                .push(("late".into(), "mutation-visible".into()));
            assert_eq!(
                result[0].headers.borrow().last().unwrap().1,
                case["late"],
                "headers_late_alias"
            );
        } else {
            assert_eq!(case["same"], Value::Null, "headers_absent_identity");
        }
        outputs += result.len();
        compared[1] += 1;
    }
    assert_eq!(outputs, 4, "headers_native_total");
    assert_eq!(compared, [25, 5], "request_helpers_completed_comparisons");
}
