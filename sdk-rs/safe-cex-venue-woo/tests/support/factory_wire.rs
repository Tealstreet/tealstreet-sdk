use safe_cex_exchange::{
    http::{HttpClientOptions, HttpFailure, HttpFuture, HttpHost},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_transport::rate_limit::RateLimitClock;
use safe_cex_transport::signing::*;
use safe_cex_transport::*;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Value, json};
use std::{
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
fn run<F: Future>(future: F) -> F::Output {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture unexpectedly yielded"),
    }
}
struct Runtime;
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1700000000123.0
    }
    fn now_ms(&self) -> f64 {
        1700000000123.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("HTTP must not spawn")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "fixture".into()
    }
    fn is_worker(&self) -> bool {
        true
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("HTTP must not dispatch worker")
    }
}
impl RateLimitClock for Runtime {
    fn now_ms(&self) -> u64 {
        1700000000123
    }
    fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct Host;
impl HttpHost for Host {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, e: HttpFailure, _: &ExchangeName) -> HttpFailure {
        e
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn note_success(&self, _: &ExchangeName, _: &RestResponse) {}
    fn note_failure(&self, _: &ExchangeName, _: &HttpFailure) {}
    fn mark_start(&self, _: &mut RestRequest) {}
    fn signing_capture(&self, _: &RestRequest) -> bool {
        false
    }
    fn is_development(&self) -> bool {
        false
    }
    fn is_main_window(&self) -> bool {
        false
    }
}
struct Capture(Arc<Mutex<Vec<RestRequest>>>);
impl RestAdapter for Capture {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.lock().unwrap().push(request);
        Box::pin(async {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(b"null".to_vec()),
            })
        })
    }
}
struct Echo;
impl RemoteSigner for Echo {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        Box::pin(async move {
            match request {
                RemoteSignerRequest::Http(r) => {
                    Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                        id: r.id,
                        config: Some(SignedConfig {
                            url: Some(r.config.url),
                            params: r.config.params,
                            data: r.config.data,
                            headers: r.config.headers,
                        }),
                        error: None,
                    }))
                }
                RemoteSignerRequest::WsAuth { .. } => {
                    Err(TransportError::Host("fixture does not sign WS".into()))
                }
            }
        })
    }
}
fn host(
    options: ExchangeOptions,
    capture: Arc<Mutex<Vec<RestRequest>>>,
    remote: bool,
) -> HttpClientOptions {
    HttpClientOptions {
        response_transform: None,
        params_serializer: None,
        exchange: ExchangeName::Woo,
        options,
        request: RestRequest::default(),
        provider: None,
        signer: if remote { Some(Arc::new(Echo)) } else { None },
        account_name: None,
        adapter: Arc::new(Capture(capture)),
        host: Rc::new(Host),
        runtime: Rc::new(Runtime),
        store: None,
        stats: None,
        logs: None,
        every: vec![],
        periodic: vec![],
        next_id: Arc::new(|| "id".into()),
        signing_now: Arc::new(|| 1700000000123),
    }
}
#[test]
fn factory_wire_matches_actual_axios_captures() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("../fixtures/signing.json")).unwrap();
    let mut count = 0;
    let mut failures = Vec::new();
    macro_rules! compare { ($a:expr, $b:expr, $($message:tt)+) => { if $a != $b { failures.push(format!("{}: got {:?}; expected {:?}", format!($($message)+), $a, $b)); } }; }
    for case in cases.iter().filter(|c| c["version"] == "factory") {
        let mut opts = json!({"key":"fixture-key","secret":"fixture-secret"});
        for key in ["key", "secret", "testnet"] {
            if let Some(v) = case["options"].get(key) {
                opts[key] = v.clone();
            }
        }
        if let Some(v) = case["options"]["extra"]
            .get("recvWindow")
            .filter(|v| !v.is_null())
        {
            opts["extra"] = json!({"recvWindow":v});
        }
        let options = serde_json::from_value(opts).unwrap();
        let captures = Arc::new(Mutex::new(vec![]));
        let api = crate::factory::create_api(
            host(
                options,
                captures.clone(),
                case["options"].get("remoteSigner").is_some(),
            ),
            &TransportRegistry::default(),
            Arc::new(Runtime),
        )
        .unwrap();
        let input = &case["input"];
        let request = RestRequest {
            url: input["url"].as_str().unwrap().into(),
            method: input["method"].as_str().unwrap_or("get").into(),
            body: input.get("data").cloned().map(RestBody::Untransformed),
            params: input.get("params").cloned(),
            ..Default::default()
        };
        run(api.execute(request)).unwrap();
        let got = captures.lock().unwrap();
        assert_eq!(got.len(), 1);
        let got = &got[0];
        let expected = &case["adapterInput"];
        compare!(got.url, expected["url"], "{} url", case["name"]);
        compare!(
            got.base_url.as_deref(),
            expected["baseURL"].as_str(),
            "{} base",
            case["name"]
        );
        compare!(
            got.timeout_ms,
            expected["timeout"].as_i64(),
            "{} timeout",
            case["name"]
        );
        let bytes = got.body.as_ref().map(|b| b.wire_bytes().unwrap());
        let expected_bytes = expected
            .get("body")
            .filter(|v| !v.is_null())
            .map(|v| v.as_str().unwrap().as_bytes().to_vec());
        compare!(bytes, expected_bytes, "{} body bytes", case["name"]);
        compare!(
            serde_json::to_value(&got.headers).unwrap(),
            expected["headers"],
            "{} complete header map",
            case["name"]
        );
        count += 1;
    }
    assert_eq!(count, 14);
    assert!(
        failures.is_empty(),
        "Actual Axios factory mismatch:\n{}",
        failures.join("\n")
    );
}

#[test]
fn signing_retains_core_transformed_body_phase() {
    let captures = Arc::new(Mutex::new(vec![]));
    let options =
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap();
    let api = crate::factory::create_api(
        host(options, captures.clone(), false),
        &TransportRegistry::default(),
        Arc::new(Runtime),
    )
    .unwrap();
    let body = RestBody::Untransformed(json!(" 1 "))
        .transform(None)
        .unwrap();
    assert!(matches!(body, RestBody::Transformed(_)));
    run(api.execute(RestRequest {
        method: "get".into(),
        url: "/v3/order".into(),
        headers: [("Content-Type".into(), "application/json".into())].into(),
        body: Some(body),
        ..RestRequest::default()
    }))
    .unwrap();
    let requests = captures.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].body,
        Some(RestBody::Encoded(b" 1 ".to_vec())),
        "signing must retain the core transform phase rather than parse and trim again"
    );
}

#[test]
fn factory_options_are_owned_unlike_live_source_secret_alias() {
    let source: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/factory-options-alias.json")).unwrap();
    let mut options: safe_cex_types::ExchangeOptions =
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap();
    let captures = Arc::new(Mutex::new(vec![]));
    let api = crate::factory::create_api(
        host(options.clone(), captures.clone(), false),
        &TransportRegistry::default(),
        Arc::new(Runtime),
    )
    .unwrap();
    for secret in ["fixture-secret", "changed-secret"] {
        options.secret = secret.into();
        run(api.execute(RestRequest {
            url: "/v1/orders".into(),
            method: "get".into(),
            ..Default::default()
        }))
        .unwrap();
    }
    let captures = captures.lock().unwrap();
    assert_ne!(
        source[0]["signature"], source[1]["signature"],
        "live source observes changed secret"
    );
    assert_eq!(captures.len(), 2, "WOO-CARDINALITY owned-options requests");
    for request in captures.iter() {
        assert_eq!(
            request.headers.get("x-api-signature").unwrap(),
            source[0]["signature"].as_str().unwrap(),
            "Rust retains initial owned options"
        );
    }
    assert_eq!(options.secret, "changed-secret");
}
