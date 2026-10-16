use safe_cex_num::{
    PrecisionInput,
    numeric_ingress::{JsNumberInput, js_from_str, js_from_value, js_from_value_ref},
};
use safe_cex_venue_polymarket::utilities::{
    normalize_tick_size, parse_gamma_market_tokens, precision_from_tick_size,
};
use serde_json::Value;
use std::process::Command;

#[test]
fn utility_rows_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/utilities_oracle.cjs"
        ))
        .output()
        .unwrap();
    eprintln!("polymarket utilities oracle: one Node invocation for the entire corpus");
    assert!(
        output.status.success(),
        "actual_utilities_oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("capture header: {}", corpus["capture"]);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 41, "tick_source_case_count");
    assert_eq!(
        cases.iter().filter(|c| c.get("output").is_some()).count(),
        12,
        "tick_source_success_count"
    );
    let (mut comparisons, mut refusals) = (0, 0);
    for (index, case) in cases.iter().enumerate() {
        let raw = case["raw"].as_str().unwrap();
        let values = match case["kind"].as_str().unwrap() {
            "number" => {
                let value: Value = serde_json::from_str(raw).unwrap();
                let buffered: JsNumberInput = serde_json::from_str(raw).unwrap();
                let numbers: [f64; 4] = [
                    js_from_str(raw).unwrap(),
                    js_from_value(value.clone()).unwrap(),
                    js_from_value_ref(&value).unwrap(),
                    buffered.decode().unwrap(),
                ];
                numbers
                    .into_iter()
                    .map(|v| {
                        assert_eq!(
                            format!("{:016x}", v.to_bits()),
                            case["bits"],
                            "tick_numeric_bits:{index}"
                        );
                        Some(PrecisionInput::Number(v))
                    })
                    .collect::<Vec<_>>()
            }
            "string" => match js_from_str::<String>(raw) {
                Ok(value) => vec![Some(PrecisionInput::String(value))],
                Err(_) => {
                    assert!(
                        raw.contains("\\ud800") || raw.contains("\\udc00"),
                        "tick_only_surrogate_refusal"
                    );
                    assert_eq!(case["errorName"], "Error", "tick_surrogate_source_error");
                    refusals += 1;
                    continue;
                }
            },
            "null" => vec![None],
            other => panic!("unknown tick input kind {other}"),
        };
        let expected_routes = if case["kind"] == "number" { 4 } else { 1 };
        assert_eq!(
            values.len(),
            expected_routes,
            "tick_native_route_count:{index}"
        );
        let mut case_comparisons = 0;
        for value in values {
            let actual = normalize_tick_size(value.as_ref());
            if let Some(expected) = case.get("output") {
                assert_eq!(
                    actual.as_ref().map(String::as_str),
                    Ok(expected.as_str().unwrap()),
                    "tick_native_success:{index}"
                );
            } else {
                assert_eq!(case["errorName"], "Error", "tick_source_error_kind:{index}");
                let actual = actual.expect_err("tick_native_expected_error");
                let expected: Vec<u16> =
                    serde_json::from_value(case["errorUnits"].clone()).unwrap();
                assert_eq!(
                    actual.encode_utf16().collect::<Vec<_>>(),
                    expected,
                    "tick_native_error_message:{index}"
                );
            }
            comparisons += 1;
            case_comparisons += 1;
        }
        assert_eq!(
            case_comparisons, expected_routes,
            "tick_case_comparisons:{index}"
        );
    }
    assert_eq!(refusals, 2, "tick_surrogate_refusal_count");
    assert_eq!(comparisons, 99, "tick_total_native_comparisons");
    let token_cases = corpus["tokenCases"].as_array().unwrap();
    let expected_counts = [
        0, 0, 0, 1, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 1, 2, 2, 2, 2, 1,
    ];
    assert_eq!(
        token_cases.len(),
        expected_counts.len(),
        "tokens_source_case_count"
    );
    let (mut total_source, mut total_native, mut boundary_cases) = (0, 0, 0);
    for (index, case) in token_cases.iter().enumerate() {
        let expected = case["output"].as_array().unwrap();
        assert_eq!(
            expected.len(),
            expected_counts[index],
            "tokens_source_case_outputs:{index}"
        );
        total_source += expected.len();
        let native =
            parse_gamma_market_tokens(case["tokenIds"].as_str(), case["outcomes"].as_str());
        if index == 19 || index == 20 {
            assert!(native.is_empty(), "tokens_surrogate_input_refusal:{index}");
            boundary_cases += 1;
            continue;
        }
        assert_eq!(native.len(), expected.len(), "tokens_native_length:{index}");
        let mut performed = 0;
        for ((token_id, outcome), expected) in native.iter().zip(expected) {
            let token_units: Vec<u16> =
                serde_json::from_value(expected["tokenId"].clone()).unwrap();
            let outcome_units: Vec<u16> =
                serde_json::from_value(expected["outcome"].clone()).unwrap();
            assert_eq!(
                token_id.encode_utf16().collect::<Vec<_>>(),
                token_units,
                "tokens_native_token_id:{index}"
            );
            assert_eq!(
                outcome.encode_utf16().collect::<Vec<_>>(),
                outcome_units,
                "tokens_native_outcome:{index}"
            );
            performed += 1;
        }
        assert_eq!(
            performed, expected_counts[index],
            "tokens_case_comparisons:{index}"
        );
        total_native += performed;
    }
    assert_eq!(total_source, 19, "tokens_source_total_outputs");
    assert_eq!(boundary_cases, 2, "tokens_boundary_case_count");
    assert_eq!(total_native, 15, "tokens_native_total_comparisons");
    let precision_cases = corpus["precisionCases"].as_array().unwrap();
    assert_eq!(precision_cases.len(), 3565, "precision_source_case_count");
    let (mut precision_comparisons, mut precision_refusals) = (0, 0);
    for (index, case) in precision_cases.iter().enumerate() {
        let raw = case["raw"].as_str().unwrap();
        let inputs = match case["kind"].as_str().unwrap() {
            "number" => {
                let value: Value = serde_json::from_str(raw).unwrap();
                let buffered: JsNumberInput = serde_json::from_str(raw).unwrap();
                let values: [f64; 4] = [
                    js_from_str(raw).unwrap(),
                    js_from_value(value.clone()).unwrap(),
                    js_from_value_ref(&value).unwrap(),
                    buffered.decode().unwrap(),
                ];
                values
                    .into_iter()
                    .map(|value| {
                        assert_eq!(
                            format!("{:016x}", value.to_bits()),
                            case["inputBits"],
                            "precision_ingress_bits:{index}"
                        );
                        Some(PrecisionInput::Number(value))
                    })
                    .collect::<Vec<_>>()
            }
            "string" => match js_from_str::<String>(raw) {
                Ok(value) => vec![Some(PrecisionInput::String(value))],
                Err(_) => {
                    assert!(
                        raw.contains("\\ud800") || raw.contains("\\udc00"),
                        "precision_only_surrogate_refusal"
                    );
                    assert_eq!(
                        case["outputBits"], "4000000000000000",
                        "precision_surrogate_source_fallback"
                    );
                    precision_refusals += 1;
                    continue;
                }
            },
            "null" => vec![None],
            other => panic!("precision unknown input {other}"),
        };
        let routes = if case["kind"] == "number" { 4 } else { 1 };
        assert_eq!(inputs.len(), routes, "precision_route_count:{index}");
        let mut performed = 0;
        for input in inputs {
            let actual = precision_from_tick_size(input.as_ref());
            assert_eq!(
                format!("{:016x}", actual.to_bits()),
                case["outputBits"],
                "precision_native_value:{index}"
            );
            performed += 1;
        }
        assert_eq!(performed, routes, "precision_case_comparisons:{index}");
        precision_comparisons += performed;
    }
    assert_eq!(precision_refusals, 2, "precision_boundary_refusals");
    assert_eq!(
        precision_comparisons, 14159,
        "precision_total_native_comparisons"
    );
}
