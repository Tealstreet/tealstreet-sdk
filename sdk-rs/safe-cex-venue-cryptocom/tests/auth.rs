use safe_cex_num::numeric_ingress::{JsNumberOutput, js_from_str};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_venue_cryptocom::signing::{
    generate_authenticated_request, generate_signature, generate_ws_auth_message,
};
use safe_cex_venue_cryptocom::signing_value::SigningValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Deserialize, Serialize)]
struct Input {
    #[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]
    params: Value,
}
impl JsNumberOutput for Input {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {} passthrough { params })
    }
}
fn entered<T>(calls: &AtomicUsize, expected: usize, value: T) -> T {
    assert_eq!(
        calls.load(Ordering::SeqCst),
        expected,
        "AUTH-EAGER-CLOCK-COUNT"
    );
    value
}
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
#[test]
fn live_auth() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/auth_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "auth_source:{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!(
        "auth capture {} Rust arch {}",
        corpus["header"],
        std::env::consts::ARCH
    );
    assert!(corpus["header"].get("architecture").unwrap().is_string());
    assert!(corpus["header"].get("nodeMajor").unwrap().is_number());
    let cases = corpus["cases"].as_array().unwrap();
    let inventory: Value = serde_json::from_str(include_str!("auth_input_inventory.json")).unwrap();
    assert_eq!(cases.len(), 42, "auth_source_count");
    assert_eq!(
        inventory.as_array().unwrap().len(),
        42,
        "auth_inventory_count"
    );
    let mut branches = [0; 3];
    let mut failures = 0;
    for (index, case) in cases.iter().enumerate() {
        for field in ["input", "kind", "explicit", "clockValues"] {
            assert_eq!(
                case.get(field).unwrap(),
                inventory[index].get(field).unwrap(),
                "auth_input_content:{index}:{field}"
            );
        }
        let input = &inventory[index]["input"];
        let text = |field: &str| input[field].as_str().unwrap();
        let params: Input = js_from_str(&format!("{{\"params\":{}}}", text("params"))).unwrap();
        let identity_params = SigningValue::from_value(params.params.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let times: [f64; 2] = std::array::from_fn(|n| {
            inventory[index]["clockValues"][n]
                .as_str()
                .unwrap()
                .parse()
                .unwrap()
        });
        let mut clock = VirtualClock::default();
        clock.now = {
            let calls = calls.clone();
            Arc::new(move || {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                assert!(n < 2, "native_clock_count_bound");
                times[n]
            })
        };
        let expected = &case["result"];
        let kind = index % 6 / 2;
        let explicit = index % 2 == 1;
        let expected_calls = if kind == 0 {
            0
        } else if kind == 1 && explicit {
            1
        } else {
            2
        };
        let error_expected = index / 6 == 6 && kind != 2;
        let native: Result<String, _> = match kind {
            0 => generate_signature(
                text("method"),
                text("id").parse().unwrap(),
                text("key"),
                text("secret"),
                &params.params,
                text("nonce").parse().unwrap(),
            ),
            1 => entered(
                &calls,
                expected_calls,
                generate_authenticated_request(
                    text("method"),
                    identity_params.clone(),
                    text("key"),
                    text("secret"),
                    explicit.then(|| text("nonce").parse().unwrap()),
                    &clock,
                ),
            )
            .map(|value| {
                assert_eq!(
                    bits(value.id),
                    expected.get("id").unwrap().as_str().unwrap(),
                    "auth_native_id"
                );
                assert_eq!(
                    bits(value.nonce),
                    expected.get("nonce").unwrap().as_str().unwrap(),
                    "auth_native_nonce"
                );
                assert_eq!(value.method, text("method"), "auth_native_method");
                assert_eq!(
                    value.method,
                    expected.get("method").unwrap().as_str().unwrap(),
                    "auth_source_native_method"
                );
                assert_eq!(value.api_key, text("key"), "auth_native_key");
                assert_eq!(
                    value.api_key,
                    expected.get("key").unwrap().as_str().unwrap(),
                    "auth_source_native_key"
                );
                assert!(
                    value.params.same_reference(&identity_params),
                    "auth_native_params_identity"
                );
                assert_eq!(
                    expected.get("paramsIdentity").unwrap(),
                    true,
                    "auth_source_params_identity"
                );
                value.sig
            }),
            2 => entered(
                &calls,
                expected_calls,
                generate_ws_auth_message(text("key"), text("secret"), &clock),
            )
            .map(|value| {
                assert_eq!(
                    bits(value.id),
                    expected.get("id").unwrap().as_str().unwrap(),
                    "ws_native_id"
                );
                assert_eq!(
                    bits(value.nonce),
                    expected.get("nonce").unwrap().as_str().unwrap(),
                    "ws_native_nonce"
                );
                assert_eq!(value.method, "public/auth", "ws_native_method");
                assert_eq!(
                    value.method,
                    expected.get("method").unwrap().as_str().unwrap(),
                    "ws_source_native_method"
                );
                assert_eq!(value.api_key, text("key"), "ws_native_key");
                assert_eq!(
                    value.api_key,
                    expected.get("key").unwrap().as_str().unwrap(),
                    "ws_source_native_key"
                );
                value.sig
            }),
            _ => unreachable!(),
        };
        assert_eq!(
            calls.load(Ordering::SeqCst),
            expected_calls,
            "auth_native_clock_calls"
        );
        assert_eq!(
            case.get("calls").unwrap(),
            expected_calls,
            "auth_source_clock_calls"
        );
        if error_expected {
            let error = native.expect_err("auth_native_error_required");
            assert_eq!(error.name, "TypeError", "auth_native_error_name");
            assert_eq!(
                error.message, "Cannot convert object to primitive value",
                "auth_native_error_message"
            );
            assert_eq!(
                expected.get("error").unwrap()["name"],
                error.name,
                "auth_source_error_name"
            );
            assert_eq!(
                expected["error"].get("message").unwrap().as_str().unwrap(),
                error.message,
                "auth_source_error_message"
            );
            failures += 1;
        } else {
            assert!(expected.get("error").is_none(), "auth_source_success");
            assert_eq!(
                native.expect("auth_native_success"),
                expected.get("sig").unwrap().as_str().unwrap(),
                "auth_native_signature:{index}"
            );
        }
        branches[kind] += 1;
    }
    assert_eq!(branches, [14, 14, 14], "auth_executed_branches");
    assert_eq!(failures, 4, "auth_executed_errors");
    // Source output-shape checks follow every native value/identity assertion.
    for (index, case) in cases.iter().enumerate() {
        let kind = index % 6 / 2;
        let expected = &case["result"];
        if index / 6 != 6 && kind == 1 {
            assert_eq!(
                expected.get("fields").unwrap(),
                &serde_json::json!(["id", "method", "api_key", "params", "sig", "nonce"]),
                "request_source_fields"
            );
        }
        if kind == 2 {
            assert_eq!(
                expected.get("fields").unwrap(),
                &serde_json::json!(["id", "method", "api_key", "sig", "nonce"]),
                "ws_source_fields"
            );
        }
    }
}
