use safe_cex_transport::signing::try_parse_json;
use serde_json::Value;

fn source_node_count(observed: &Value) -> usize {
    let children = match &observed["children"] {
        Value::Array(children) => children.iter().map(source_node_count).sum(),
        Value::Object(children) => children.values().map(source_node_count).sum(),
        _ => 0,
    };
    1 + children
}

fn compare(value: &Value, expected: &Value) {
    match expected["kind"].as_str().unwrap() {
        "number" => {
            let n = safe_cex_num::js_number(Some(value)).unwrap();
            assert_eq!(
                format!("{:016x}", n.to_bits()),
                expected["bits"],
                "{}",
                if expected["bits"] == "8000000000000000" {
                    "signed-zero capture retains its observable sign"
                } else {
                    "non-NaN capture retains every payload bit"
                }
            );
            assert_eq!(
                safe_cex_num::js_string(Some(value)).unwrap(),
                expected["string"],
                "signer String(number) spelling"
            );
            assert_eq!(
                safe_cex_num::js_truthy(Some(value)),
                expected["truthy"],
                "overflow truthiness"
            );
            if n.is_finite() && !(n == 0.0 && n.is_sign_negative()) {
                assert_eq!(
                    value.as_number().unwrap().to_string(),
                    expected["string"],
                    "finite signer payload must collapse its numeric lexeme to binary64"
                );
            }
        }
        "array" => {
            let actual = value.as_array().unwrap();
            let children = expected["children"].as_array().unwrap();
            assert_eq!(
                actual.len(),
                children.len(),
                "PARSER-CARDINALITY native array length"
            );
            for (v, e) in actual.iter().zip(children) {
                compare(v, e);
            }
        }
        "object" => {
            let actual = value.as_object().unwrap();
            assert_eq!(
                actual.keys().cloned().collect::<Vec<_>>(),
                serde_json::from_value::<Vec<String>>(expected["keys"].clone()).unwrap(),
                "source Object.keys order"
            );
            for (k, v) in actual {
                compare(v, &expected["children"][k]);
            }
        }
        _ => assert_eq!(value, &expected["value"]),
    }
}
#[test]
fn parsed_signer_payload_matches_source_before_serialization() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/try-parse-json/vectors.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert!(
        cases
            .iter()
            .any(|c| c["raw"].as_str().unwrap().contains("9007199254740993"))
    );
    assert!(
        cases
            .iter()
            .any(|c| c["raw"].as_str().unwrap().contains("1e400"))
    );
    assert!(
        cases
            .iter()
            .any(|c| c["raw"].as_str().unwrap().contains("-0"))
    );
    assert_eq!(cases.len(), 62, "PARSER-CARDINALITY source cases");
    assert_eq!(
        cases
            .iter()
            .map(|case| source_node_count(&case["observed"]))
            .sum::<usize>(),
        156,
        "PARSER-CARDINALITY source total nodes"
    );
    let mut compared = 0;
    for (index, case) in cases.iter().enumerate() {
        let expected_nodes = if index < 54 {
            [2, 3, 3][index % 3]
        } else if index == 61 {
            5
        } else {
            1
        };
        assert_eq!(
            source_node_count(&case["observed"]),
            expected_nodes,
            "PARSER-CARDINALITY source per-case nodes: {index}"
        );
        let input: Value = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        compare(&parsed_value(input), &case["observed"]);
        compared += 1;
    }
    assert_eq!(compared, 62, "PARSER-CARDINALITY compared cases");
}
#[test]
fn existing_non_string_payload_is_not_reparsed() {
    for raw in ["{\"n\":9007199254740993}", "{\"n\":-0}", "[1e400]", "1.50"] {
        let input: safe_cex_types::retained_json::RetainedJsonInput =
            serde_json::from_str(raw).unwrap();
        assert_eq!(
            try_parse_json(input.clone()),
            input,
            "non-string source passthrough"
        );
    }
}
#[test]
fn source_parser_oracle_must_execute() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/try-parse-json/capture.cjs");
    for script in [script.clone(), script.with_file_name("boundaries.cjs")] {
        let output = std::process::Command::new("node")
            .arg(script)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "live source oracle must execute: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn inherited_parser_boundaries_now_retain_the_complete_source_tree() {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/try-parse-json/boundaries.json")).unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let input: Input = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        assert_eq!(
            case["sourceKind"], "object",
            "source accepts valid JSON boundary case"
        );
        let result = try_parse_json(input);
        assert!(
            matches!(result, Input::Object(_) | Input::Array(_)),
            "native parser admits source tree, including historical refusals"
        );
        assert_eq!(
            safe_cex_num::js_retained_json_stringify(&result)
                .unwrap()
                .unwrap(),
            case["sourceWire"],
            "complete native UTF-16 tree matches source wire independently of model projection"
        );
    }
}

fn consumed_array_control(retain: usize) {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/try-parse-json/vectors.json")).unwrap();
    let cases: Vec<_> = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["observed"]["kind"] == "array")
        .take(2)
        .collect();
    assert_eq!(
        cases.len(),
        2,
        "consuming control requires two source array observations"
    );
    let inputs: Vec<String> = cases
        .iter()
        .map(|case| {
            let input: String = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
            input
                .strip_prefix('[')
                .unwrap()
                .strip_suffix(']')
                .unwrap()
                .to_owned()
        })
        .collect();
    let expected = serde_json::json!({"kind":"array", "children":[
        cases[0]["observed"]["children"][0], cases[1]["observed"]["children"][0]
    ]});
    let mut decoded = parsed_value(Value::String(format!("[{},{}]", inputs[0], inputs[1])));
    decoded.as_array_mut().unwrap().truncate(retain);
    compare(&decoded, &expected);
}

#[test]
#[should_panic(expected = "PARSER-CARDINALITY native array length")]
fn consuming_empty_array_control_reaches_native_length_assertion() {
    consumed_array_control(0);
}

#[test]
#[should_panic(expected = "PARSER-CARDINALITY native array length")]
fn consuming_subset_array_control_reaches_native_length_assertion() {
    consumed_array_control(1);
}

fn parsed_value(value: Value) -> Value {
    let input = safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap();
    safe_cex_transport::RestResponseBody::Transformed(try_parse_json(input))
        .json_value()
        .unwrap()
}
