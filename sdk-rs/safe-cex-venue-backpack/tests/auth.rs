use safe_cex_num::JsJsonValue;
use safe_cex_venue_backpack::signing::{generate_auth_headers, sign_request};
use serde_json::{Value, json};
use std::{
    future::Future,
    process::Command,
    task::{Context, Poll, Waker},
};
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

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("CPU signing future must complete when polled"),
    }
}
#[test]
fn actual_typescript_request_signing_and_headers() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/auth_oracle.cjs"
        ))
        .output()
        .expect("one actual TypeScript crypto corpus");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in ["node", "arch", "platform"] {
        assert!(!corpus["runtime"][key].as_str().unwrap().is_empty());
    }
    eprintln!("TypeScript auth runtime: {}", corpus["runtime"]);
    assert_eq!(
        corpus["sign"].as_array().unwrap().len(),
        198,
        "AUTH-CARDINALITY source sign cases"
    );
    assert_eq!(
        corpus["headers"].as_array().unwrap().len(),
        264,
        "AUTH-CARDINALITY source header cases"
    );
    assert_eq!(
        corpus["sign"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["ok"] == true)
            .count(),
        63,
        "AUTH-CARDINALITY source sign successes"
    );
    assert_eq!(
        corpus["headers"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["ok"] == true)
            .count(),
        80,
        "AUTH-CARDINALITY source header successes"
    );
    let mut compared = [0; 3];
    for (i, case) in corpus["sign"].as_array().unwrap().iter().enumerate() {
        compare(
            ready(sign_request(
                case["instruction"].as_str().unwrap(),
                &from_observation(&case["input"]),
                bits(&case["timestamp"]),
                bits(&case["window"]),
                case["secret"].as_str().unwrap(),
            )),
            case,
            "signRequest",
            i,
        );
        compared[0] += 1;
    }
    for (i, case) in corpus["headers"].as_array().unwrap().iter().enumerate() {
        let mut calls = 0;
        let result = ready(generate_auth_headers(
            case["instruction"].as_str().unwrap(),
            &from_observation(&case["input"]),
            case["apiKey"].as_str().unwrap(),
            case["secret"].as_str().unwrap(),
            if case["window"].is_null() {
                None
            } else {
                Some(bits(&case["window"]))
            },
            || {
                calls += 1;
                bits(&case["timestamp"])
            },
        ));
        assert_eq!(calls, 1, "generateAuthHeaders native clock count case {i}");
        assert_eq!(
            case["clockCalls"], 1,
            "actual TypeScript clock count case {i}"
        );
        if case["ok"] == true {
            let actual = result.expect("source accepted headers");
            let expected = case["value"].as_array().unwrap();
            assert_eq!(
                expected.len(),
                5,
                "AUTH-CARDINALITY source header fields case {i}"
            );
            assert_eq!(
                actual.len(),
                expected.len(),
                "generateAuthHeaders header census case {i}"
            );
            for ((key, value), expected) in actual.iter().zip(expected) {
                assert_eq!(
                    key, &expected[0],
                    "generateAuthHeaders native header key case {i}"
                );
                assert_eq!(
                    value, &expected[1],
                    "generateAuthHeaders native {key} case {i}"
                );
                compared[2] += 1;
            }
        } else {
            assert_eq!(
                result.expect_err("source refused headers"),
                case["error"],
                "generateAuthHeaders error precedence case {i}"
            );
        }
        compared[1] += 1;
    }
    assert_eq!(
        compared,
        [198, 264, 400],
        "AUTH-CARDINALITY completed comparisons"
    );
    let clock = std::cell::Cell::new(11.0);
    let calls = std::cell::Cell::new(0);
    let pending = generate_auth_headers(
        "clockProbe",
        &JsJsonValue::Object(vec![]),
        "caller-key",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        None,
        || {
            calls.set(calls.get() + 1);
            clock.get()
        },
    );
    assert_eq!(
        calls.get(),
        1,
        "generateAuthHeaders eager clock before poll"
    );
    assert_eq!(
        corpus["eagerClock"]["callsAtConstruction"], 1,
        "actual TypeScript eager clock before await"
    );
    clock.set(22.0);
    let headers = ready(pending).unwrap();
    assert_eq!(
        json!(headers),
        corpus["eagerClock"]["headers"],
        "generateAuthHeaders call-time snapshot after clock advances"
    );
    assert_eq!(
        calls.get(),
        1,
        "generateAuthHeaders clock stays single after poll"
    );
    assert_eq!(
        corpus["eagerClock"]["callsAfterAwait"], 1,
        "actual TypeScript clock stays single after await"
    );
    for key in ["instructionRaw", "keyRaw"] {
        typed_utf16::assert_projection(
            corpus["boundary"][key].as_str().unwrap(),
            &(safe_cex_num::numeric_ingress::js_from_str::<String>(
                corpus["boundary"][key].as_str().unwrap(),
            ))
            .expect("typed surrogate record is retained"),
        );
    }
    assert_eq!(
        corpus["boundary"]["keyUnits"],
        json!([107, 101, 121, 56320]),
        "source caller key surrogate output"
    );
    assert_eq!(
        corpus["boundary"]["signature"], corpus["boundary"]["replacementSignature"],
        "source instruction TextEncoder replacement output"
    );
    let signature = ready(sign_request(
        "go\u{fffd}",
        &JsJsonValue::Object(vec![]),
        7.0,
        5000.0,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    ))
    .unwrap();
    assert_eq!(
        signature, corpus["boundary"]["replacementSignature"],
        "accepted replacement instruction signature"
    );
    eprintln!(
        "auth: {} sign cases, {} header cases",
        corpus["sign"].as_array().unwrap().len(),
        corpus["headers"].as_array().unwrap().len()
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
