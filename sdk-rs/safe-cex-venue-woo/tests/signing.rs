use safe_cex_transport::*;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Value, json};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Waker},
};
struct Provider(String);
impl TransportProvider for Provider {
    fn name(&self) -> &str {
        "fixture"
    }
    fn rest(&self, c: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        DirectTransport.rest(c)
    }
    fn rest_adapter(&self, c: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        DirectTransport.rest_adapter(c)
    }
    fn proxy_prefix(&self, _: RestTransportContext<'_>) -> String {
        self.0.clone()
    }
    fn public_cors_proxy(&self) -> Option<String> {
        None
    }
    fn private_proxy(&self, _: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        None
    }
    fn require_proxy(&self, o: ExchangeOptions) -> ExchangeOptions {
        o
    }
    fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
        None
    }
    fn ws_routing(&self, e: &ExchangeName, o: &RoutingOptions) -> ResolvedRouting {
        DirectTransport.ws_routing(e, o)
    }
    fn ws_connection(&self, _: &str) -> Option<WsConnection> {
        None
    }
}
fn run<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(Waker::noop());
    match std::pin::pin!(future).as_mut().poll(&mut context) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("local HMAC unexpectedly yielded"),
    }
}
#[test]
fn signatures_match_original_hash_boundary_captures() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/signing.json")).unwrap();
    let options: ExchangeOptions =
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap();
    let mut count = 0;
    for case in cases
        .iter()
        .filter(|case| case["version"] == 3 || case["version"] == 1)
    {
        let provider = Provider(case["options"]["proxy"].as_str().unwrap_or_default().into());
        let mut input = case["input"].clone();
        if case["name"] == "v3-index-keys-first" {
            input["data"] = Value::Object(
                ["z", "10", "2", "a"]
                    .into_iter()
                    .map(|key| (key.into(), case["input"]["data"][key].clone()))
                    .collect(),
            );
        }
        let signed = if case["version"] == 1 {
            run(safe_cex_venue_woo::api::sign_v1(
                &safe_cex_exchange::js_runtime::value::from_json(&input),
                &options,
                &provider,
                1700000000123.0,
            ))
        } else {
            run(safe_cex_venue_woo::api::sign_v3(
                &safe_cex_exchange::js_runtime::value::from_json(&input),
                &options,
                &provider,
                1700000000123.0,
            ))
        }
        .unwrap();
        let config_view =
            oracle_view(safe_cex_exchange::js_runtime::value::to_json(&signed.config).unwrap());
        assert_eq!(signed.preimage, case["preimage"], "{}", case["name"]);
        assert_eq!(
            config_view["headers"]["x-api-signature"], case["signature"],
            "{}",
            case["name"]
        );
        for key in ["url", "method", "params", "data"] {
            assert_eq!(
                config_view.get(key),
                case["signed"].get(key),
                "{} {key}",
                case["name"]
            );
        }
        count += 1;
    }
    assert_eq!(
        count, 22,
        "Factory capture remains pending, never counted as exercised"
    );
}
#[test]
fn ordinary_json_signer_shapes_and_proxy_context() {
    let inputs = [
        json!({"method":{"toUpperCase":1},"url":"/x"}),
        json!({"method":{"toUpperCase":null},"url":"/x"}),
        json!({"method":false,"url":"/x"}),
        json!({"method":"get","url":123}),
        json!({"method":"get","url":{}}),
        json!({"method":"pOsT","url":"https://example.invalid/x","data":{"body":37},"params":{"query":41},"headers":{"x-custom":"53"},"baseURL":"https://base.invalid","timeout":71,"custom":97}),
        json!({"url":null}),
        json!({"url":false}),
        json!({"method":{"toUpperCase":false},"url":"/x"}),
        json!({"method":[],"url":"/x"}),
        json!({"method":"","url":"/x"}),
    ];
    let options: ExchangeOptions =
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap();
    let provider = AuditProvider(std::sync::Mutex::new(vec![]));
    let mut native = vec![];
    for version in [1, 3] {
        for (index, input) in inputs.iter().enumerate() {
            let before = provider.0.lock().unwrap().len();
            let result = if version == 1 {
                run(safe_cex_venue_woo::api::sign_v1(
                    &safe_cex_exchange::js_runtime::value::from_json(input),
                    &options,
                    &provider,
                    1700000000123.,
                ))
            } else {
                run(safe_cex_venue_woo::api::sign_v3(
                    &safe_cex_exchange::js_runtime::value::from_json(input),
                    &options,
                    &provider,
                    1700000000123.,
                ))
            };
            let mut entry = match index {
                0 | 8 => {
                    let expected = if version == 1 {
                        "nextConfig.method?.toUpperCase is not a function"
                    } else {
                        "c.method?.toUpperCase is not a function"
                    };
                    let error = result
                        .err()
                        .expect("WOO-AUDIT native optional method must throw");
                    assert_eq!(error, expected, "WOO-AUDIT native optional method error");
                    json!({"error":error})
                }
                3 | 4 | 7 => {
                    let error = result
                        .err()
                        .expect("WOO-AUDIT native nonstring URL must throw");
                    assert_eq!(
                        error, "nextConfig?.url?.startsWith is not a function",
                        "WOO-AUDIT native URL error"
                    );
                    json!({"error":error})
                }
                _ => {
                    let signed = result.unwrap();
                    let config_view =
                        safe_cex_exchange::js_runtime::value::to_json(&signed.config).unwrap();
                    if version == 3 {
                        assert_eq!(
                            config_view["method"],
                            if index == 5 { "POST" } else { "GET" },
                            "WOO-AUDIT native selected method"
                        );
                    }
                    if index == 5 && version == 3 {
                        assert_eq!(
                            config_view["data"]["body"].as_f64().unwrap().to_bits(),
                            37.0_f64.to_bits(),
                            "WOO-AUDIT native retained body bits"
                        );
                    }
                    if version == 1 {
                        match index {
                            1 => assert_eq!(config_view["method"], json!({"toUpperCase":null})),
                            2 => assert_eq!(config_view["method"], false),
                            5 => {
                                assert_eq!(config_view["method"], "pOsT");
                                assert_eq!(
                                    config_view["data"], "body=37",
                                    "WOO-AUDIT native V1 form body"
                                );
                            }
                            6 => assert!(config_view.get("method").is_none()),
                            9 => assert_eq!(config_view["method"], json!([])),
                            10 => assert_eq!(config_view["method"], ""),
                            _ => unreachable!("successful audit case floor"),
                        }
                    }
                    json!({"method":config_view.get("method"),"url":config_view.get("url"),"preimage":signed.preimage})
                }
            };
            let seen = provider.0.lock().unwrap();
            let proxy_calls = seen.len() - before;
            assert_eq!(
                proxy_calls,
                usize::from(index == 5),
                "WOO-AUDIT native proxy invocation count"
            );
            let proxy = if proxy_calls == 0 {
                Value::Null
            } else {
                let request = seen.last().unwrap();
                assert_eq!(
                    request.method,
                    if version == 1 { "pOsT" } else { "POST" },
                    "WOO-AUDIT native provider method spelling"
                );
                assert_eq!(
                    request.url, "https://example.invalid/x",
                    "WOO-AUDIT native provider URL before rewrite"
                );
                let params = request.params.as_ref().unwrap().as_object().unwrap();
                assert_eq!(
                    params.len(),
                    1,
                    "WOO-AUDIT native provider params field floor"
                );
                assert_eq!(
                    params["query"].as_f64().unwrap().to_bits(),
                    41.0_f64.to_bits(),
                    "WOO-AUDIT native provider params bits"
                );
                assert!(
                    matches!(request.body, Some(RestBody::Untransformed(_))),
                    "WOO-AUDIT native proxy body phase"
                );
                assert_eq!(
                    request.body.as_ref().unwrap().untransformed().unwrap(),
                    &safe_cex_types::retained_json::RetainedJsonInput::Object(vec![(
                        "body".encode_utf16().collect(),
                        safe_cex_types::retained_json::RetainedJsonInput::Float(37.0),
                    )]),
                    "WOO-AUDIT native proxy body"
                );
                assert_eq!(
                    request.headers.len(),
                    1,
                    "WOO-AUDIT native header field floor"
                );
                assert_eq!(
                    request.headers.get("x-custom").map(String::as_str),
                    Some("53"),
                    "WOO-AUDIT native proxy headers"
                );
                assert_eq!(
                    request.base_url.as_deref(),
                    Some("https://base.invalid"),
                    "WOO-AUDIT native proxy base URL"
                );
                assert_eq!(
                    request.timeout_ms,
                    Some(71),
                    "WOO-AUDIT native proxy timeout"
                );
                oracle_view(observed_context(request))
            };
            entry["proxy"] = proxy;
            entry["proxyCalls"] = json!(proxy_calls);
            native.push(entry);
        }
    }
    assert_eq!(
        provider.0.lock().unwrap().len(),
        2,
        "WOO-AUDIT native total proxy call floor"
    );
    assert_eq!(native.len(), 22, "WOO-AUDIT native case floor");
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/signing-domain-oracle.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&inputs).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reference.len(), 22, "WOO-AUDIT source case floor");
    for (index, (actual, expected)) in native.iter().zip(&reference).enumerate() {
        // The retained typed context omits unknown Axios config properties.
        if index % 11 == 5 {
            assert_eq!(
                expected["proxy"]["custom"], 97,
                "WOO-AUDIT source custom config witness"
            );
            assert!(
                actual["proxy"].get("custom").is_none(),
                "WOO-AUDIT exhaustive carrier has no custom field"
            );
            for (key, field) in [
                ("body", "data"),
                ("headers", "headers"),
                ("baseURL", "baseURL"),
                ("timeout", "timeout"),
                ("params", "params"),
                ("url", "url"),
                ("method", "method"),
            ] {
                assert_eq!(
                    actual["proxy"][key], expected["proxy"][field],
                    "WOO-AUDIT source proxy context {key}"
                );
            }
        } else {
            assert_eq!(actual["proxy"], expected["proxy"]);
        }
        if actual.get("error").is_some() {
            assert_eq!(
                expected["name"], "TypeError",
                "WOO-AUDIT source error brand"
            );
        }
        for key in ["method", "url", "preimage", "error", "proxyCalls"] {
            assert_eq!(
                actual.get(key).filter(|v| !v.is_null()),
                expected.get(key).filter(|v| !v.is_null()),
                "WOO-AUDIT source case {index} {key}"
            );
        }
    }
}
// The maintained oracle returns a JSON.stringify report; native assertions run above.
fn oracle_view(value: Value) -> Value {
    let text = safe_cex_num::js_json_stringify(&safe_cex_num::JsJsonValue::from(value))
        .unwrap()
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn observed_context(request: &RestRequest) -> Value {
    let RestRequest {
        method,
        url,
        base_url,
        headers,
        body,
        timeout_ms,
        params,
        query_params,
        query_profile,
        skip_remote_signing,
        sign_ms,
        config_extensions,
    } = request;
    assert!(
        query_params.is_none()
            && query_profile.is_none()
            && !skip_remote_signing
            && sign_ms.is_none()
            && config_extensions.is_empty(),
        "WOO-AUDIT native unused context fields"
    );
    json!({"method":method,"url":url,"params":params,"body":body.as_ref().map(|b| b.untransformed().unwrap().clone().into_value().unwrap()),"headers":headers,"baseURL":base_url,"timeout":timeout_ms})
}
struct AuditProvider(std::sync::Mutex<Vec<RestRequest>>);
impl TransportProvider for AuditProvider {
    fn name(&self) -> &str {
        "audit"
    }
    fn rest(&self, c: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        DirectTransport.rest(c)
    }
    fn rest_adapter(&self, c: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        DirectTransport.rest_adapter(c)
    }
    fn proxy_prefix(&self, c: RestTransportContext<'_>) -> String {
        self.0.lock().unwrap().push(c.config.clone());
        "https://proxy.invalid".into()
    }
    fn public_cors_proxy(&self) -> Option<String> {
        None
    }
    fn private_proxy(&self, _: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        None
    }
    fn require_proxy(&self, o: ExchangeOptions) -> ExchangeOptions {
        o
    }
    fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
        None
    }
    fn ws_routing(&self, e: &ExchangeName, o: &RoutingOptions) -> ResolvedRouting {
        DirectTransport.ws_routing(e, o)
    }
    fn ws_connection(&self, _: &str) -> Option<WsConnection> {
        None
    }
}
