use safe_cex_num::JsJsonValue;
use safe_cex_venue_mexc::api::*;
use serde_json::{Value, json};
fn input(value: &Value) -> JsJsonValue {
    if let Some(entries) = value.get("$entries").and_then(Value::as_array) {
        return JsJsonValue::Object(
            entries
                .iter()
                .map(|pair| (pair[0].as_str().unwrap().to_owned(), input(&pair[1])))
                .collect(),
        );
    }
    if value.get("$undefined") == Some(&Value::Bool(true)) {
        return JsJsonValue::Undefined;
    }
    if let Some(s) = value.get("$bigint").and_then(Value::as_str) {
        return JsJsonValue::BigInt(s.parse().unwrap());
    }
    if let Some(s) = value.get("$number").and_then(Value::as_str) {
        return JsJsonValue::Number(match s {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            _ => panic!("invalid {s}"),
        });
    }
    match value {
        Value::Array(v) => JsJsonValue::Array(v.iter().map(input).collect()),
        Value::Object(v) => {
            JsJsonValue::Object(v.iter().map(|(k, v)| (k.clone(), input(v))).collect())
        }
        _ => JsJsonValue::from(value.clone()),
    }
}
// Test ingress preserves the existing tagged corpus, including native BigInt throw cases.
fn native_input(value: &JsJsonValue) -> safe_cex_exchange::js_runtime::value::Js {
    use safe_cex_exchange::js_runtime::value::Js;
    match value {
        JsJsonValue::Undefined => Js::Undefined,
        JsJsonValue::Null => Js::Null,
        JsJsonValue::Bool(v) => Js::Bool(*v),
        JsJsonValue::Number(v) => Js::Number(*v),
        JsJsonValue::String(v) => Js::str(v),
        JsJsonValue::BigInt(v) => Js::BigInt(std::rc::Rc::new(v.clone())),
        JsJsonValue::Array(v) => Js::array(v.iter().map(native_input).collect()),
        JsJsonValue::Object(v) => {
            let object = Js::object(vec![]);
            for (key, value) in v {
                object.define(key, native_input(value)).unwrap();
            }
            object
        }
    }
}
#[test]
fn mexc_api_live_typescript_signatures_and_bodies() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/api_oracle.mjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 186, "live API corpus census");
    for (i, case) in cases.iter().enumerate() {
        let args = case["args"].as_array().unwrap();
        let text = |i: usize| args[i].as_str().unwrap();
        let actual = match case["name"].as_str().unwrap() {
            "signMexcRequest" => {
                json!({"output":sign_mexc_request(text(0),text(1),text(2),text(3)).unwrap()})
            }
            "signMexcWsLogin" => {
                json!({"output":sign_mexc_ws_login(text(0),text(1),text(2)).unwrap()})
            }
            "isMexcPublicEndpoint" => json!({"output":is_mexc_public_endpoint(args[0].as_str())}),
            "serializeMexcParams" => {
                json!({"output":serialize_mexc_params(&input(&args[0])).unwrap()})
            }
            "getQueryString" => {
                json!({"output":get_query_string(&input(&args[0]["params"]),args[0]["url"].as_str()).unwrap()})
            }
            "normalizeRequestData" => match normalize_request_data(&native_input(&input(&args[0])))
            {
                Ok(value) => json!({"output":value}),
                Err(e) => {
                    json!({"error":{"name":e.value.get("name").unwrap().to_display(),"message":e.message()}})
                }
            },
            name => panic!("unhandled {name}"),
        };
        let expected = if case.get("error").is_some() {
            json!({"error":case["error"]})
        } else {
            json!({"output":case["output"]})
        };
        assert_eq!(
            actual, expected,
            "case {i} {} args {}",
            case["name"], case["args"]
        );
    }
    println!(
        "{} signatures, public-route and raw-body cases",
        cases.len()
    );
}
#[test]
fn mexc_api_typescript_negative_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_api_live_typescript_signatures_and_bodies",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "sign-preimage")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("mexc_api_live_typescript_signatures_and_bodies ... FAILED")
            && text.contains("signMexcRequest")
            && text.contains("assertion `left == right` failed"),
        "{text}"
    );
}

#[test]
fn mexc_query_typescript_negative_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_api_live_typescript_signatures_and_bodies",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "query-sort")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_api_live_typescript_signatures_and_bodies ... FAILED")
            && text.contains("serializeMexcParams")
            && text.contains("assertion `left == right` failed"),
        "{text}"
    );
}

#[test]
fn mexc_factory_live_json_transform_boundary() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/factory_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 71, "live factory boundary census");
    assert_eq!(
        cases[61]["preimages"],
        json!(["key1791800000123  {\"a\":1} \n"])
    );
    assert_eq!(
        cases[61]["capture"][0]["data"],
        json!("{\"a\":1}"),
        "MEXC JSON content-type wire transform: whitespace"
    );
    let raw = &cases[16];
    assert_eq!(raw["preimages"], json!(["key1791800000123raw text"]));
    assert_eq!(
        raw["capture"][0]["data"],
        json!("\"raw text\""),
        "MEXC JSON content-type wire transform"
    );
    for case in &cases {
        if let Some(data) = case["scenario"]["config"].get("data") {
            let bytes = safe_cex_transport::RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(data.clone()).unwrap(),
            )
            .wire_bytes_for_content_type(Some("application/json"))
            .unwrap();
            assert_eq!(
                String::from_utf8(bytes).unwrap(),
                case["capture"][0]["data"].as_str().unwrap(),
                "shared post-signing JSON transform: {}",
                case["scenario"]
            );
        }
    }
    assert_eq!(cases[32]["capture"][0]["data"], json!("\"\""));
    assert_eq!(cases[40]["capture"][0]["skip"], true);
    assert_eq!(cases[49]["result"]["error"], "denied");
    assert_eq!(cases[49]["result"]["hasResponse"], true);
    assert_eq!(cases[56]["capture"].as_array().unwrap().len(), 4);
    assert_eq!(cases[58]["capture"].as_array().unwrap().len(), 1);
    assert_eq!(cases[60]["capture"][0]["timeout"], 20000);
    assert_eq!(cases[60]["capture"][0]["headers"]["Recv-Window"], "12345");
}

#[test]
fn mexc_factory_typescript_negative_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_factory_live_json_transform_boundary",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "wire-json")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_factory_live_json_transform_boundary ... FAILED")
            && text.contains("MEXC JSON content-type wire transform")
            && text.contains("assertion `left == right` failed"),
        "{text}"
    );
}

#[test]
fn mexc_shared_utf8_string_boundary_witnesses() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/string_boundary_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 10);
    for case in cases {
        assert_eq!(case["tsAccepted"], true);
        let json = case["json"].as_str().unwrap();
        let value = serde_json::from_str::<Value>(json);
        if case["representable"] == false {
            // Valid JSON accepted by live TS; rejection here is a representation
            // refusal, never an invalid-JSON claim or replacement-character repair.
            assert!(
                value.is_err(),
                "lone-surrogate representation boundary changed: {json}"
            );
            assert!(
                case["outputs"]
                    .as_object()
                    .unwrap()
                    .values()
                    .all(Value::is_string)
            );
        } else {
            let value = JsJsonValue::from(value.unwrap());
            for (name, result) in [
                (
                    "serializeMexcParams",
                    serialize_mexc_params(&value).map_err(|e| e.message.to_string()),
                ),
                (
                    "getQueryString",
                    get_query_string(&value, Some("/private?x=1"))
                        .map_err(|e| e.message.to_string()),
                ),
                (
                    "normalizeRequestData",
                    normalize_request_data(&native_input(&value)).map_err(|e| e.message()),
                ),
            ] {
                assert_eq!(
                    serde_json::to_string(&result.unwrap()).unwrap(),
                    case["outputs"][name].as_str().unwrap(),
                    "positive Unicode {name}: {json}"
                );
            }
        }
    }
}

// Axios retries transformed error.config data. Pin both signing inputs and wire
// bytes: equal wire bytes alone cannot observe a stale retry preimage.
#[test]
fn mexc_head_retry_live_preimage_and_wire_parity() {
    use safe_cex_transport::retry::{ClassifiedRequest, ReadClassification, network_error_on_read};
    use safe_cex_transport::{
        NetworkFailure, RestAdapter, RestBody, RestFailure, RestRequest, RestResponse,
        TransportError,
    };
    struct Clock;
    impl safe_cex_transport::rate_limit::RateLimitClock for Clock {
        fn now_ms(&self) -> u64 {
            1791800000123
        }
        fn sleep_ms(&self, _: u64) -> safe_cex_transport::TransportFuture<'_, ()> {
            Box::pin(async { Ok(()) })
        }
    }
    struct Attempt(std::cell::RefCell<Vec<(String, String)>>);
    impl RestAdapter for Attempt {
        fn execute(
            &self,
            mut request: RestRequest,
        ) -> safe_cex_transport::RestFuture<'_, RestResponse> {
            Box::pin(async move {
                let data = request.body.as_ref().unwrap().untransformed().unwrap();
                let safe_cex_types::retained_json::RetainedJsonInput::Text(units) = data else {
                    panic!("native normalized request text")
                };
                let text = String::from_utf16(units).unwrap();
                let n = self.0.borrow().len();
                let preimage = format!("key{}{}", 1791800000123u64 + n as u64, text);
                let wire = request
                    .body
                    .as_ref()
                    .unwrap()
                    .wire_bytes_for_content_type(Some("application/json"))
                    .unwrap();
                self.0
                    .borrow_mut()
                    .push((preimage, String::from_utf8(wire).unwrap()));
                request.body = Some(
                    request
                        .body
                        .as_ref()
                        .unwrap()
                        .transform(Some("application/json"))
                        .unwrap(),
                );
                if n == 0 {
                    Err(RestFailure::from_transport(
                        TransportError::Network(NetworkFailure::from_code(
                            "network lost".into(),
                            Some("ECONNRESET".into()),
                        )),
                        request,
                    ))
                } else {
                    Ok(RestResponse {
                        status: 200,
                        reason: None,
                        headers: Default::default(),
                        body: safe_cex_transport::RestResponseBody::Encoded(b"{}".to_vec()),
                    })
                }
            })
        }
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/factory_oracle.mjs"
        ))
        .arg("--retry-body")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 3);
    for case in cases {
        let data = case["scenario"]["config"]["data"].clone();
        let request = RestRequest {
            method: "head".into(),
            url: "/private".into(),
            body: Some(RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(data).unwrap(),
            )),
            timeout_ms: Some(20000),
            ..Default::default()
        };
        let ClassifiedRequest::Read(read) = ReadClassification::Methods.classify(request) else {
            panic!("HEAD must be a read")
        };
        let adapter = Attempt(Default::default());
        futures::executor::block_on(network_error_on_read(read, 3).execute(&adapter, &Clock))
            .unwrap();
        let actual = adapter.0.borrow();
        assert_eq!(actual.len(), 2);
        for (index, (preimage, wire)) in actual.iter().enumerate() {
            assert_eq!(preimage, case["preimages"][index].as_str().unwrap());
            assert_eq!(wire, case["capture"][index]["data"].as_str().unwrap());
        }
        assert_eq!(
            case["capture"][0]["data"], case["capture"][1]["data"],
            "wire equality alone hides signature divergence"
        );
    }
}

#[test]
fn mexc_factory_shared_header_normalization() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/factory_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let case = cases
        .iter()
        .find(|c| {
            c["scenario"]["config"]["headers"]["apikey"] == "old-api-key"
                && c["scenario"]["options"] == json!({"key":"key","secret":"secret"})
        })
        .unwrap();
    let mut request = safe_cex_transport::RestRequest::default();
    for (name, value) in case["scenario"]["config"]["headers"].as_object().unwrap() {
        request
            .headers
            .insert(name.clone(), value.as_str().unwrap().into());
    }
    request.headers.insert("ApiKey".into(), "key".into());
    request
        .headers
        .insert("Request-Time".into(), "1791800000123".into());
    request.headers.insert(
        "Signature".into(),
        sign_mexc_request("secret", "key", "1791800000123", "").unwrap(),
    );
    request.headers.insert("Recv-Window".into(), "20000".into());
    request
        .headers
        .insert("Content-Type".into(), "application/json".into());
    // The shared repair replaces the former alias/default-header FINDING.
    request.merge_axios_headers(
        &[
            ("Accept".into(), "application/json, text/plain, */*".into()),
            ("Content-Type".into(), "application/json".into()),
        ]
        .into_iter()
        .collect(),
    );
    let actual = safe_cex_transport::signing::serialize_headers(&request.headers);
    assert_eq!(json!(actual), case["capture"][0]["headers"]);
}
