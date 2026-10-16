use safe_cex_venue_mexc::exchange_helpers::{
    is_native_batch_access_denied_error, note_regular_order_submit_error,
};
use serde_json::Value;
use std::{cell::Cell, process::Command};

fn oracle() -> Vec<Value> {
    let result = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/submit_error_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    let cases = result["cases"].as_array().unwrap().clone();
    assert_eq!(cases.len(), 33, "independent submit error fixture floor");
    assert_eq!(cases[32]["index"], 32, "last submit error fixture");
    cases
}

fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}

#[test]
fn mexc_native_batch_access_denial_matches_source() {
    for case in oracle() {
        let actual = is_native_batch_access_denied_error(case.get("input"));
        let expected = &case["denied"];
        match actual {
            Ok(denied) => assert_eq!(
                Some(denied),
                expected["ok"].as_bool(),
                "MEXC native batch denial: {}",
                case["index"]
            ),
            Err(error) => {
                assert_eq!(
                    error.name,
                    expected["error"]["name"].as_str().unwrap(),
                    "MEXC denial coercion name"
                );
                assert_eq!(
                    error.message,
                    expected["error"]["message"].as_str().unwrap(),
                    "MEXC denial coercion message"
                );
            }
        }
    }
}

#[test]
fn mexc_submit_cooldown_matches_source() {
    for case in oracle() {
        let states = case["states"].as_array().unwrap();
        assert_eq!(states.len(), 4, "independent cooldown state floor");
        for state in states {
            let mut cooldown = number(&state["initial"]);
            let calls = Cell::new(0);
            let now = || {
                calls.set(calls.get() + 1);
                number(&state["time"])
            };
            let outcome = note_regular_order_submit_error(&mut cooldown, case.get("input"), &now);
            let expected = number(&state["cooldown"]);
            if expected.is_nan() {
                assert!(
                    cooldown.is_nan(),
                    "MEXC submit cooldown NaN: {}",
                    case["index"]
                );
            } else {
                assert_eq!(
                    cooldown.to_bits(),
                    expected.to_bits(),
                    "MEXC submit cooldown: {}",
                    case["index"]
                );
            }
            assert_eq!(
                calls.get(),
                state["clockCalls"].as_u64().unwrap(),
                "MEXC submit clock access: {}",
                case["index"]
            );
            match outcome {
                Ok(()) => assert!(
                    state["outcome"].get("ok").is_some(),
                    "MEXC submit outcome: {}",
                    case["index"]
                ),
                Err(error) => {
                    assert_eq!(
                        error.name,
                        state["outcome"]["error"]["name"].as_str().unwrap(),
                        "MEXC submit coercion name"
                    );
                    assert_eq!(
                        error.message,
                        state["outcome"]["error"]["message"].as_str().unwrap(),
                        "MEXC submit coercion message"
                    );
                }
            }
        }
    }
}
