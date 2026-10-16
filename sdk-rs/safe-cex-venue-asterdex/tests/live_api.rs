use safe_cex_venue_asterdex::api_helpers::*;
use serde_json::{Value, json};
use std::{
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = pin!(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("local HMAC must resolve immediately"),
    }
}
fn result<T: serde::Serialize>(result: Result<T, safe_cex_num::JsCoercionError>) -> Value {
    match result {
        Ok(value) => json!({"value":value}),
        Err(error) => json!({"error":{"name":error.name,"message":error.message}}),
    }
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn api_helpers_match_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    for case in data["chains"].as_array().unwrap() {
        let testnet = case["input"].as_bool().unwrap();
        assert_eq!(json!(get_aster_dex_code_chain_id(testnet)), case["id"]);
        assert_eq!(json!(get_aster_dex_aster_chain(testnet)), case["chain"]);
    }
    for case in data["queries"].as_array().unwrap() {
        let params: serde_json::Map<String, Value> = case["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e[0].as_str().unwrap().to_owned(), e[1].clone()))
            .collect();
        let params = &params;
        assert_eq!(
            result(build_aster_dex_code_query_string(params)),
            case["code"],
            "buildAsterDEXCodeQueryString {case}"
        );
        assert_eq!(
            result(build_query_string(params)),
            case["query"],
            "buildQueryString {case}"
        );
        let payload = build_v3_signing_payload(params);
        assert_eq!(
            result(payload.clone()),
            case["signing"],
            "buildV3SigningPayload {case}"
        );
        assert_eq!(
            result(payload.map(|p| p.keys().cloned().collect::<Vec<_>>())),
            case["signingKeys"],
            "buildV3SigningPayload key order {case}"
        );
    }
    for case in data["paths"].as_array().unwrap() {
        let config = &case["config"];
        let url = config["url"].as_str();
        let method = config["method"].as_str();
        let request = safe_cex_transport::RestRequest {
            url: url.unwrap_or("").to_owned(),
            method: method.unwrap_or("").to_owned(),
            ..Default::default()
        };
        assert_eq!(
            json!(is_asterdex_read(&request)),
            case["read"],
            "isAsterdexRead {case}"
        );
        assert_eq!(
            json!(is_public_endpoint(url)),
            case["public"],
            "isPublicEndpoint {case}"
        );
        assert_eq!(
            json!(is_aster_dex_code_signed_endpoint(url, method)),
            case["signed"],
            "isAsterDEXCodeSignedEndpoint {case}"
        );
        assert_eq!(json!(url.map(strip_query_string)), case["stripped"]);
    }
    for case in data["errors"].as_array().unwrap() {
        assert_eq!(
            result(should_suppress_unknown_order_error(
                case.get("config"),
                case.get("code"),
                case.get("message")
            )),
            case["suppressed"],
            "shouldSuppressUnknownOrderError {case}"
        );
        assert_eq!(
            result(get_aster_dex_error_detail(
                case.get("message"),
                case.get("code")
            )),
            case["detail"],
            "getAsterDEXErrorDetail {case}"
        );
    }
    for case in data["signatures"].as_array().unwrap() {
        assert_eq!(
            json!(
                ready(generate_v1_signature(
                    case["query"].as_str().unwrap(),
                    case["secret"].as_str().unwrap()
                ))
                .unwrap()
            ),
            case["signature"],
            "generateV1Signature {case}"
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript and ethers oracle"]
fn api_message_signatures_match_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["messageSignatures"].as_array().unwrap().len(),
        21,
        "12 valid plus 9 invalid signer cases"
    );
    for case in data["messageSignatures"].as_array().unwrap() {
        let actual = generate_aster_dex_code_message_signature(
            case["query"].as_str().unwrap(),
            case["key"].as_str().unwrap(),
            case.get("testnet").and_then(Value::as_bool),
        );
        if case["signature"].get("value").is_some() {
            assert_eq!(
                json!(actual.unwrap()),
                case["signature"]["value"],
                "generateAsterDEXCodeMessageSignature {case}"
            );
        } else {
            let source_error = &case["signature"]["error"];
            assert!(
                matches!(source_error["name"].as_str(), Some("Error" | "TypeError")),
                "generateAsterDEXCodeMessageSignature source error {case}"
            );
            if case["key"] == "not-a-private-key" {
                // ethers 6.17 includes the rejected key in this TypeError.
                // The shared Rust signer must keep key material out of errors.
                assert_eq!(source_error["name"], "TypeError", "{case}");
                assert!(
                    source_error["message"]
                        .as_str()
                        .unwrap()
                        .contains("not-a-private-key"),
                    "generateAsterDEXCodeMessageSignature source key witness {case}"
                );
            } else if case["key"] == format!("0x{}", "0".repeat(64)) {
                assert_eq!(source_error["name"], "Error", "{case}");
                assert_eq!(
                    source_error["message"], "Expected valid bigint: 0 < bigint < curve.n",
                    "generateAsterDEXCodeMessageSignature zero-key error {case}"
                );
            }
            assert_eq!(
                actual.as_ref().unwrap_err().name,
                source_error["name"].as_str().unwrap(),
                "native signing error class"
            );
            assert!(
                actual.is_err(),
                "generateAsterDEXCodeMessageSignature Rust refusal {case}"
            );
            assert!(
                case["key"].as_str().unwrap().is_empty()
                    || !actual
                        .as_ref()
                        .unwrap_err()
                        .message
                        .contains(case["key"].as_str().unwrap()),
                "generateAsterDEXCodeMessageSignature redacts private key material {case}"
            );
        }
    }
}

#[test]
#[ignore = "requires live isolated TypeScript and ethers oracle"]
fn api_message_signature_oracle_rejects_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "api_message_signatures_match_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_API_SIGNATURE_CONTROL", "wrong-chain")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("generateAsterDEXCodeMessageSignature"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript and ethers oracle"]
fn api_message_signature_oracle_requires_real_ethers() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-oracle.cjs"
        ))
        .env("ASTERDEX_API_PACKAGE_CONTROL", "missing-ethers")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "missing ethers unexpectedly passed"
    );
    assert!(diagnostics.contains("MODULE_NOT_FOUND"), "{diagnostics}");
    assert!(
        diagnostics.contains("asterdex-missing-ethers-control"),
        "{diagnostics}"
    );
}
