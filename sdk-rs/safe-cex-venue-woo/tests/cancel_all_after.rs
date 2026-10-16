use safe_cex_venue_woo::cancel_all_after::*;
use serde_json::{Value, json};
#[path = "support/number_bits.rs"]
mod capture_bits;
fn bits(value: &Value) -> String {
    capture_bits::number_bits(safe_cex_num::numeric_ingress::js_value_as_f64(value).unwrap())
}
#[test]
fn cancel_all_after_matches_live_source_fields_bits_and_error_order() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/cancel-all-after-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(source["capture"]["architecture"].is_string());
    assert!(source["capture"]["node"].is_string());
    for (key, number) in WOO_V3_CANCEL_ALL_AFTER_LIMITS {
        assert_eq!(
            *number,
            source["limits"][key].as_f64().unwrap(),
            "limit identity {key}"
        );
    }
    for (key, flag) in WOO_V3_CANCEL_ALL_AFTER_PARITY {
        assert_eq!(
            *flag,
            source["parity"][key].as_bool().unwrap(),
            "parity identity {key}"
        );
    }
    assert_eq!(
        source["limits"].as_object().unwrap().len(),
        WOO_V3_CANCEL_ALL_AFTER_LIMITS.len()
    );
    assert_eq!(
        source["parity"].as_object().unwrap().len(),
        WOO_V3_CANCEL_ALL_AFTER_PARITY.len()
    );
    assert_eq!(
        source["representation"]["firstCodeUnits"],
        json!([0xd800, 0xd800, 0xd800])
    );
    for case in source["paths"].as_array().unwrap() {
        assert_eq!(
            require_record(Some(&Value::Null), case["path"].as_str().unwrap()).unwrap_err(),
            case["message"].as_str().unwrap()
        );
    }
    assert!(
        serde_json::from_str::<String>(source["representation"]["input"].as_str().unwrap())
            .is_err(),
        "shared UTF-8 representation refuses lone surrogate"
    );
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 199);
    for (index, case) in cases.iter().enumerate() {
        let input: Option<Value> = case["raw"]
            .as_str()
            .map(|raw| serde_json::from_str(raw).unwrap());
        let operation = case["operation"].as_str().unwrap();
        let actual = match operation {
            "buildWooV3CancelAllAfterRequest" => {
                build_woo_v3_cancel_all_after_request(input.as_ref())
            }
            "decodeWooV3CancelAllAfterResponse" => {
                decode_woo_v3_cancel_all_after_response(input.as_ref())
            }
            "requireRecord" => {
                require_record(input.as_ref(), "probe").map(|x| Value::Object(x.clone()))
            }
            "requireNonnegativeSafeInteger" => {
                require_nonnegative_safe_integer(input.as_ref(), "probe")
                    .map(|x| json!(capture_bits::number_bits(x)))
            }
            _ => panic!("unexpected source operation"),
        };
        if let Some(error) = case.get("error") {
            assert_eq!(error["name"], "Error");
            assert_eq!(
                actual.expect_err("cancel-after exact validation priority: source rejected"),
                error["message"].as_str().unwrap(),
                "cancel-after exact validation priority case {index}"
            );
        } else {
            let actual = actual.unwrap_or_else(|e| panic!("case {index}: {e}"));
            let expected = &case["ok"];
            match operation {
                "buildWooV3CancelAllAfterRequest" => {
                    assert_eq!(
                        actual["method"], expected["method"],
                        "request method identity"
                    );
                    assert_eq!(actual["path"], expected["path"], "request path identity");
                    assert_eq!(
                        bits(&actual["body"]["triggerAfter"]),
                        expected["triggerAfter"],
                        "triggerAfter native bits case {index}"
                    );
                }
                "decodeWooV3CancelAllAfterResponse" => {
                    assert_eq!(
                        actual["accepted"], expected["accepted"],
                        "response accepted identity"
                    );
                    for field in ["timestamp", "expectedTriggerTime"] {
                        assert_eq!(
                            bits(&actual[field]),
                            expected[field],
                            "response {field} native field identity case {index}"
                        );
                    }
                }
                _ => assert_eq!(actual, *expected, "helper identity case {index}"),
            }
        }
    }
    for case in source["exactKeys"].as_array().unwrap() {
        let expected: Vec<&str> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key.as_str().unwrap())
            .collect();
        let result = require_exact_keys(case["input"].as_object().unwrap(), &expected, "probe");
        let label = if case["unicode"] == true {
            "exact key UTF-16 diagnostic order"
        } else {
            "exact key identity"
        };
        assert_eq!(result.err().as_deref(), case["error"].as_str(), "{label}");
    }
}
#[test]
fn cancel_all_after_owned_outputs_are_explicitly_mutable() {
    let mut request = build_woo_v3_cancel_all_after_request(Some(&json!(17))).unwrap();
    request["body"]["triggerAfter"] = json!(29);
    assert_eq!(
        request["body"]["triggerAfter"], 29,
        "chosen mutable request output"
    );
    let mut response = decode_woo_v3_cancel_all_after_response(Some(
        &json!({"success":true,"timestamp":101,"data":{"expectedTriggerTime":202}}),
    ))
    .unwrap();
    response["timestamp"] = json!(303);
    assert_eq!(response["timestamp"], 303, "chosen mutable response output");
}
