use safe_cex_num::JsJsonValue;
use safe_cex_venue_backpack::signing::verify_signature;
use serde_json::Value;
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
fn actual_typescript_verification_composition() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/verify_oracle.cjs"
        ))
        .output()
        .expect("one actual TypeScript verification corpus");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in ["node", "arch", "platform"] {
        assert!(!corpus["runtime"][key].as_str().unwrap().is_empty());
    }
    eprintln!("TypeScript verify runtime: {}", corpus["runtime"]);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 507, "VERIFY-CARDINALITY source cases");
    for (label, total) in [
        ("valid", 324),
        ("changed instruction", 9),
        ("changed timestamp", 9),
        ("changed window", 9),
        ("changed params", 9),
        ("changed signature", 9),
        ("changed key", 9),
        ("key length 0", 9),
        ("key length 1", 9),
        ("key length 31", 9),
        ("key length 33", 9),
        ("key length 64", 9),
        ("signature length 0", 9),
        ("signature length 1", 9),
        ("signature length 32", 9),
        ("signature length 63", 9),
        ("signature length 65", 9),
        ("payload refusal", 36),
        ("weak identity key", 3),
    ] {
        assert_eq!(
            cases.iter().filter(|c| c["label"] == label).count(),
            total,
            "VERIFY-CARDINALITY subcorpus {label}"
        );
    }
    let mut compared = [0; 3];
    for (i, case) in cases.iter().enumerate() {
        let native = ready(verify_signature(
            case["instruction"].as_str().unwrap(),
            &from_observation(&case["input"]),
            bits(&case["timestamp"]),
            bits(&case["window"]),
            case["signature"].as_str().unwrap(),
            case["key"].as_str().unwrap(),
        ));
        let source = case["actual"].as_bool().expect("source native boolean");
        if case["boundary"] == true {
            assert!(
                source,
                "actual TypeScript ZIP215 identity acceptance case {i}"
            );
            assert!(!native, "chosen strict native weak-key refusal case {i}");
            compared[2] += 1;
        } else {
            assert_eq!(
                native, source,
                "verifySignature native boolean {} case {i}",
                case["label"]
            );
            compared[usize::from(!source)] += 1;
        }
    }
    assert_eq!(
        compared,
        [324, 180, 3],
        "VERIFY-CARDINALITY completed comparisons"
    );
}
