#[path = "support/input_identity.rs"]
mod input_identity;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_venue_weex::utils::{WeexUtilityInput, normalize_balance};
use serde_json::{Value, json};
use std::{path::Path, process::Command};
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
// Inspect the retained native tree directly; never serialize a Balance to witness it.
fn capture(value: &(impl serde::Serialize + ?Sized)) -> Value {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let value = safe_cex_num::numeric_ingress::js_capture_output(value).unwrap();
    match &value {
        Input::Float(_) | Input::Lexeme(_) => {
            json!({"type":"number","bits":bits(value.as_f64().unwrap())})
        }
        Input::Array(values) => Value::Array(values.iter().map(capture).collect()),
        Input::Object(values) => Value::Array(
            values
                .iter()
                .map(|(key, value)| json!([String::from_utf16(key).unwrap(), capture(value)]))
                .collect(),
        ),
        Input::Undefined => json!({"undefined":true}),
        Input::Null => Value::Null,
        Input::Bool(value) => json!(value),
        Input::Text(value) => json!(String::from_utf16(value).unwrap()),
    }
}
#[test]
fn balances_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/balance-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(corpus["header"]["arch"].is_string());
    assert!(corpus["header"]["nodeMajor"].is_number());
    assert!(corpus["header"]["node"].is_string());
    assert_eq!(corpus["header"]["nanBits"], "7ff8000000000000");
    let ids = [
        "distinct",
        "second",
        "fallback",
        "aliases",
        "zero",
        "signed",
        "empty",
        "null_row",
        "null",
        "object",
        "primitive_rows",
        "asset_error",
        "number_error",
        "selection_short_circuit",
        "selection_error_before_fallback",
        "numeric",
        "coercion",
        "max_overflow",
    ];
    let lengths = [1, 3, 1, 1, 1, 1, 0, 1, 0, 0, 5, 1, 1, 2, 2, 1, 1, 1];
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18, "balance::source_cases");
    let (mut compared, mut fields, mut infos) = (0, 0, 0);
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(case["id"], ids[index], "balance::source_identity");
        for route in 0..4 {
            let raw = case["raw"].as_str().unwrap();
            let input: WeexUtilityInput = match route {
                0 => serde_json::from_str(raw).unwrap(),
                1 => js_from_str(raw).unwrap(),
                2 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
            };
            let label = format!("balance::{}::route_{route}", ids[index]);
            match normalize_balance(&input) {
                Err(e) => {
                    assert_eq!(
                        e.name,
                        case["error"]["name"].as_str().unwrap(),
                        "{label}::error_name"
                    );
                    assert_eq!(
                        e.message,
                        case["error"]["message"].as_str().unwrap(),
                        "{label}::error_message"
                    );
                }
                Ok(value) => {
                    assert!(case.get("error").is_none(), "{label}::unexpected_success");
                    assert_eq!(
                        case["fields"].as_object().unwrap().len(),
                        4,
                        "{label}::source_fields"
                    );
                    for (name, n) in [
                        ("total", value.total),
                        ("free", value.free),
                        ("used", value.used),
                        ("upnl", value.upnl),
                    ] {
                        assert_eq!(
                            bits(n),
                            case["fields"][name]["bits"].as_str().unwrap(),
                            "{label}::{name}"
                        );
                        fields += 1;
                    }
                    let info = value.info.as_ref().unwrap();
                    assert_eq!(
                        case["info"].as_array().unwrap().len(),
                        lengths[index],
                        "{label}::source_info_count"
                    );
                    assert_eq!(
                        info.as_array().unwrap().len(),
                        lengths[index],
                        "{label}::native_info_count"
                    );
                    assert_eq!(
                        info,
                        &input.as_value().clone().into(),
                        "{label}::retained_info_identity"
                    );
                    assert_eq!(capture(info), case["info"], "{label}::native_info");
                    assert!(
                        value.currencies.is_none() && value.coins.is_none(),
                        "{label}::absent"
                    );
                    infos += 1;
                }
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 72, "balance::compared");
    assert_eq!(fields, 192, "balance::fields_compared");
    assert_eq!(infos, 48, "balance::infos_compared");
    input_identity::check(&corpus, 0x9da5979465fedbe6, "balance");
    let refusals = corpus["refusals"].as_array().unwrap();
    assert_eq!(refusals.len(), 2, "balance::refusal_count");
    let expected = [
        r#"[{"asset":"USDT","tag":"\ud800"}]"#.to_owned(),
        format!(
            "[{{\"asset\":\"USDT\",\"extra\":{}0{}}}]",
            "[".repeat(150),
            "]".repeat(150)
        ),
    ];
    for (index, case) in refusals.iter().enumerate() {
        assert_eq!(case["raw"], expected[index], "balance::refusal_identity");
        assert_eq!(case["source"], 0, "balance::source_refusal_acceptance");
        assert!(
            js_from_str::<WeexUtilityInput>(&expected[index]).is_err(),
            "balance::retained_refusal"
        );
    }
    assert_eq!(
        corpus["callable"]["bits"], "401d000000000000",
        "balance::callable_source"
    );
}
