use safe_cex_exchange::{http::*, runtime::*};
use safe_cex_transport::{rate_limit::RateLimitClock, signing::*, *};
use safe_cex_types::ExchangeName;
use safe_cex_venue_mexc::{api::get_query_string, client::create_api};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
};
#[derive(Default)]
struct Runtime {
    attempt: Cell<u64>,
    runtime_clock_calls: Cell<u64>,
    retry_clock_calls: Cell<u64>,
    schedules: RefCell<Vec<u64>>,
    monotonic_ms: Rc<Cell<f64>>,
}
impl ExchangeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        self.runtime_clock_calls
            .set(self.runtime_clock_calls.get() + 1);
        1791800000123.0 + self.attempt.get() as f64
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.monotonic_ms.get()
    }
    fn sleep(&self, delay_ms: u64) -> ExchangeFuture<'static, ()> {
        let clock = self.monotonic_ms.clone();
        Box::pin(async move {
            clock.set(clock.get() + delay_ms as f64);
            Ok(())
        })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("factory must not spawn")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "id".into()
    }
    fn is_worker(&self) -> bool {
        false
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
        panic!("unexpected worker")
    }
}
impl RateLimitClock for Runtime {
    fn now_ms(&self) -> u64 {
        self.retry_clock_calls.set(self.retry_clock_calls.get() + 1);
        1791800000123 + self.attempt.get()
    }
    fn sleep_ms(&self, delay_ms: u64) -> TransportFuture<'_, ()> {
        self.schedules.borrow_mut().push(delay_ms);
        let clock = self.monotonic_ms.clone();
        Box::pin(async move {
            clock.set(clock.get() + delay_ms as f64);
            Ok(())
        })
    }
}

#[test]
fn client_runtime_monotonic_clock_tracks_both_sleep_paths() {
    let runtime = Runtime::default();
    assert_eq!(runtime.monotonic_now_ms(), 0.0);
    futures::executor::block_on(runtime.sleep(13)).unwrap();
    assert_eq!(runtime.monotonic_now_ms(), 13.0);
    futures::executor::block_on(RateLimitClock::sleep_ms(&runtime, 7)).unwrap();
    assert_eq!(runtime.monotonic_now_ms(), 20.0);
}

struct IdentitySigner;
impl safe_cex_transport::signing::RemoteSigner for IdentitySigner {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        Box::pin(async move {
            match request {
                RemoteSignerRequest::Http(request) => {
                    Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                        id: request.id,
                        config: Some(SignedConfig {
                            headers: request.config.headers,
                            url: Some(request.config.url),
                            params: request.config.params,
                            data: request.config.data,
                        }),
                        error: None,
                    }))
                }
                RemoteSignerRequest::WsAuth { .. } => {
                    Err(TransportError::Host("unexpected WS request".into()))
                }
            }
        })
    }
}
#[derive(Default)]
struct Host {
    preimages: RefCell<Vec<String>>,
    rich: RefCell<Vec<RestRequest>>,
    unsigned_signature: Option<String>,
}
impl HttpHost for Host {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, error: RestFailure, _: &ExchangeName) -> RestFailure {
        error
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn note_success(&self, _: &ExchangeName, _: &RestResponse) {}
    fn note_failure(&self, _: &ExchangeName, _: &RestFailure) {}
    fn mark_start(&self, request: &mut RestRequest) {
        if request.query_params.is_some() {
            self.rich.borrow_mut().push(request.clone());
            return;
        }
        let header = |name: &str| {
            request
                .headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        };
        if header("Signature").is_none()
            || header("Signature") == self.unsigned_signature.as_deref()
        {
            return;
        }
        let param = if request.method.eq_ignore_ascii_case("get") {
            get_query_string(
                &request
                    .params
                    .clone()
                    .map(safe_cex_num::JsJsonValue::from)
                    .unwrap_or(safe_cex_num::JsJsonValue::Undefined),
                Some(&request.url),
            )
            .unwrap()
        } else {
            request
                .body
                .as_ref()
                .map(|body| {
                    safe_cex_venue_mexc::api::normalize_request_data(
                        &safe_cex_exchange::js_runtime::value::from_retained_input(
                            body.untransformed().unwrap().clone(),
                        ),
                    )
                    .unwrap()
                })
                .unwrap_or_default()
        };
        self.preimages.borrow_mut().push(format!(
            "{}{}{}",
            header("ApiKey").unwrap(),
            header("Request-Time").unwrap(),
            param
        ));
    }
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
struct Wire {
    runtime: Rc<Runtime>,
    failures: u64,
    response: String,
    capture: RefCell<Vec<RestRequest>>,
}
impl RestAdapter for Wire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.runtime.attempt.set(self.runtime.attempt.get() + 1);
            self.capture.borrow_mut().push(request.clone());
            if self.runtime.attempt.get() <= self.failures {
                return Err(RestFailure::from_transport(
                    TransportError::Network(NetworkFailure::from_code(
                        "network lost".into(),
                        Some("ECONNRESET".into()),
                    )),
                    request,
                ));
            }
            Ok(RestResponse {
                status: 200,
                reason: Some("OK".into()),
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    self.response.as_bytes().to_vec(),
                ),
            })
        })
    }
}
#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn mexc_factory_live_shared_http_composition() {
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
    let mut cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 71);
    let retry_output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/factory_oracle.mjs"
        ))
        .arg("--retry-body")
        .output()
        .unwrap();
    assert!(
        retry_output.status.success(),
        "{}",
        String::from_utf8_lossy(&retry_output.stderr)
    );
    let retries: Vec<Value> = serde_json::from_slice(&retry_output.stdout).unwrap();
    assert_eq!(retries.len(), 3);
    cases.extend(retries);
    let numbers_output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/factory_oracle.mjs"
        ))
        .arg("--response-numbers")
        .output()
        .unwrap();
    assert!(
        numbers_output.status.success(),
        "{}",
        String::from_utf8_lossy(&numbers_output.stderr)
    );
    let numbers: Vec<Value> = serde_json::from_slice(&numbers_output.stdout).unwrap();
    assert_eq!(numbers.len(), 4);
    cases.extend(numbers);
    for (index, case) in cases.iter().enumerate() {
        let scenario = &case["scenario"];
        let runtime = Rc::new(Runtime::default());
        let host = Rc::new(Host {
            unsigned_signature: scenario["config"]["headers"]
                .as_object()
                .and_then(|headers| {
                    headers
                        .iter()
                        .find(|(key, _)| key.eq_ignore_ascii_case("Signature"))
                })
                .and_then(|(_, value)| value.as_str())
                .map(str::to_owned),
            ..Host::default()
        });
        let wire = Arc::new(Wire {
            runtime: runtime.clone(),
            failures: scenario["failures"].as_u64().unwrap_or(0),
            response: scenario["response"]
                .as_str()
                .unwrap_or("{\"code\":0,\"data\":{\"id\":9007199254740993123}}")
                .into(),
            capture: RefCell::default(),
        });
        let mut raw_options = scenario["options"].clone();
        let remote = raw_options
            .as_object_mut()
            .unwrap()
            .remove("remoteSigner")
            .is_some();
        let api = create_api(
            HttpClientOptions {
                response_transform: None,
                params_serializer: None,
                exchange: ExchangeName::Mexc,
                options: serde_json::from_value(raw_options).unwrap(),
                request: RestRequest::default(),
                provider: None,
                signer: if remote {
                    Some(Arc::new(IdentitySigner))
                } else {
                    None
                },
                account_name: None,
                adapter: wire.clone(),
                host: host.clone(),
                runtime: runtime.clone(),
                store: None,
                stats: Some(Arc::default()),
                logs: Some(Arc::default()),
                every: vec![],
                periodic: vec![],
                next_id: Arc::new(|| "id".into()),
                signing_now: Arc::new(|| 0),
            },
            &TransportRegistry::default(),
            runtime.clone(),
            {
                let runtime = runtime.clone();
                Rc::new(move || ExchangeRuntime::now_ms(runtime.as_ref()))
            },
        )
        .unwrap();
        let config = &scenario["config"];
        let mut request = RestRequest {
            method: config["method"].as_str().unwrap_or("get").into(),
            url: config["url"].as_str().unwrap().into(),
            params: config.get("params").cloned(),
            body: config.get("data").cloned().map(|value| {
                RestBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap(),
                )
            }),
            ..Default::default()
        };
        if let Some(headers) = config.get("headers").and_then(Value::as_object) {
            request.headers = headers
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().into()))
                .collect();
        }
        let response = futures::executor::block_on(api.execute(request));
        let actual = match response {
            Ok(r) => json!({"data":r.body.json_value().unwrap()}),
            Err(e) => {
                json!({"error":e.message,"hasResponse":e.response.is_some(),"data":e.response.map(|r|r.body.json_value().unwrap()).unwrap_or(Value::Null)})
            }
        };
        if index < 74 {
            assert_eq!(actual, case["result"], "result {index}: {case}");
        }
        let expected_observed = case["observedResult"].clone();
        let observed = observe_numbers(&actual);
        assert_eq!(
            observed, expected_observed,
            "response numeric bits {index}: {case}"
        );
        assert_eq!(
            json!(*host.preimages.borrow()),
            case["preimages"],
            "preimages {index}: {case}"
        );
        let captures = wire.capture.borrow();
        assert_eq!(
            captures.len(),
            case["capture"].as_array().unwrap().len(),
            "attempts {index}"
        );
        for (attempt, request) in captures.iter().enumerate() {
            let expected = &case["capture"][attempt];
            assert_eq!(request.method, expected["method"], "method {index}");
            assert_eq!(
                request.base_url.as_deref(),
                expected["baseURL"].as_str(),
                "baseURL {index}"
            );
            assert_eq!(
                request.timeout_ms,
                expected["timeout"].as_i64(),
                "timeout {index}"
            );
            assert_eq!(
                request.skip_remote_signing, expected["skip"],
                "skip {index}"
            );
            assert_eq!(
                format!(
                    "{}{}",
                    request.base_url.as_deref().unwrap_or(""),
                    request.url
                ),
                expected["wireURL"],
                "wireURL {index}"
            );
            let body = request
                .body
                .as_ref()
                .map(|b| Value::String(String::from_utf8(b.wire_bytes().unwrap()).unwrap()))
                .unwrap_or(Value::Null);
            assert_eq!(body, expected["data"], "wire body {index}: {case}");
            assert_eq!(
                json!(request.headers),
                expected["headers"],
                "headers {index}"
            );
        }
    }
}

#[test]
fn mexc_factory_typescript_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_factory_live_shared_http_composition",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "wire-json")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_factory_live_shared_http_composition ... FAILED"),
        "{text}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("assertion `left == right` failed"),
        "{stderr}"
    );
}

fn observe_numbers(value: &Value) -> Value {
    match value {
        Value::Number(n) => {
            let n = n
                .as_f64()
                .unwrap_or_else(|| n.to_string().parse::<f64>().unwrap());
            json!({"$number": if n.is_nan() { "7ff8000000000000".to_owned() } else { format!("{:016x}", n.to_bits()) }})
        }
        Value::Array(a) => Value::Array(a.iter().map(observe_numbers).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), observe_numbers(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn mexc_rich_factory_preserves_native_values_before_signing_and_wire() {
    use safe_cex_transport::query::{QueryKeyOrder, query_identity_eq};
    use safe_cex_types::QueryValue as Q;
    fn object(fields: Vec<(&str, Q)>) -> Q {
        Q::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
    let array = Q::Array(
        (0..11)
            .map(|index| Q::String(format!("v{index}")))
            .collect(),
    );
    let inputs = [
        object(vec![
            ("symbol", Q::String("BTC_USDT".into())),
            ("start_time", Q::Number(f64::NAN)),
            ("end_time", Q::Number(f64::NEG_INFINITY)),
            ("page_num", Q::Number(1.0)),
            ("page_size", Q::Number(100.0)),
        ]),
        object(vec![
            ("z", array),
            ("zero", Q::Number(-0.0)),
            ("positive", Q::Number(f64::INFINITY)),
        ]),
        object(vec![
            ("é", Q::String("first".into())),
            ("e\u{301}", Q::String("second".into())),
            ("a", Q::String("before".into())),
        ]),
        object(vec![
            ("zero", Q::Number(-0.0)),
            ("positive", Q::Number(f64::INFINITY)),
        ]),
        object(vec![("from", Q::Number(f64::NAN)), ("to", Q::Number(-0.0))]),
    ];
    let mut observations = Vec::new();
    let mut total_attempts = 0;
    let mut total_retry_clock = 0;
    let mut total_runtime_clock = 0;
    let mut total_schedules = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        let runtime = Rc::new(Runtime::default());
        let host = Rc::new(Host::default());
        let wire = Arc::new(Wire {
            runtime: runtime.clone(),
            failures: if index == 4 { 2 } else { 0 },
            response: "{\"code\":0,\"data\":[]}".into(),
            capture: RefCell::new(vec![]),
        });
        let api = create_api(
            HttpClientOptions {
                response_transform: None,
                params_serializer: None,
                exchange: ExchangeName::Mexc,
                options: serde_json::from_value(
                    json!({"key":"fixture-key","secret":"fixture-secret"}),
                )
                .unwrap(),
                request: RestRequest::default(),
                provider: None,
                signer: None,
                account_name: None,
                adapter: wire.clone(),
                host: host.clone(),
                runtime: runtime.clone(),
                store: None,
                stats: Some(Arc::default()),
                logs: Some(Arc::default()),
                every: vec![],
                periodic: vec![],
                next_id: Arc::new(|| "id".into()),
                signing_now: Arc::new(|| 0),
            },
            &TransportRegistry::default(),
            runtime.clone(),
            {
                let runtime = runtime.clone();
                Rc::new(move || ExchangeRuntime::now_ms(runtime.as_ref()))
            },
        )
        .unwrap();
        assert_eq!(
            host.rich.borrow().len(),
            0,
            "rich factory native prefix before call"
        );
        let response = futures::executor::block_on(
            api.execute(RestRequest {
                method: "get".into(),
                url: if index == 3 {
                    "/api/v1/contract/detail"
                } else {
                    "/private"
                }
                .into(),
                query_params: Some(input.clone()),
                ..Default::default()
            }),
        )
        .unwrap();
        let decoded = response.body.json_value().unwrap();
        assert_eq!(
            decoded.get("code").and_then(Value::as_i64),
            Some(0),
            "rich factory native decoded code"
        );
        assert_eq!(
            decoded.get("data").and_then(Value::as_array).unwrap().len(),
            0,
            "rich factory native decoded empty data count"
        );
        let expected_attempts = if index == 4 { 3 } else { 1 };
        assert_eq!(
            runtime.attempt.get(),
            expected_attempts,
            "rich factory native independent adapter attempts"
        );
        let retained = host.rich.borrow();
        assert_eq!(
            retained.len(),
            expected_attempts as usize,
            "rich factory native independent pre-render calls"
        );
        for (attempt, request) in retained.iter().enumerate() {
            assert!(
                request.params.is_none(),
                "rich factory native legacy projection absent"
            );
            assert_eq!(
                request.query_profile.unwrap().key_order,
                QueryKeyOrder::JsRoot,
                "rich factory native collation profile"
            );
            assert!(
                query_identity_eq(request.query_params.as_ref().unwrap(), input),
                "rich factory native retained number bits and original key/index order"
            );
            assert_eq!(
                request.timeout_ms,
                Some(20000 - attempt as i64),
                "rich factory native measured timeout budget"
            );
        }
        assert_eq!(
            *runtime.schedules.borrow(),
            if index == 4 { vec![0, 0] } else { vec![] },
            "rich factory native zero-delay scheduler calls"
        );
        total_attempts += runtime.attempt.get();
        total_retry_clock += runtime.retry_clock_calls.get();
        total_runtime_clock += runtime.runtime_clock_calls.get();
        total_schedules.extend_from_slice(&runtime.schedules.borrow());
        drop(retained);
        observations.push((wire, host));
    }
    assert_eq!(
        observations.len(),
        5,
        "rich factory native independent root floor"
    );
    assert_eq!(
        total_attempts, 7,
        "rich factory native independent adapter call floor"
    );
    assert_eq!(
        total_retry_clock, 9,
        "rich factory native retry clock call floor"
    );
    // The native real shared HTTP owner adds request statistics/log-clock reads;
    // the Source fixture declares that child as a real-Axios factory boundary.
    assert_eq!(
        total_runtime_clock, 25,
        "rich factory native signing plus shared HTTP clock call floor"
    );
    assert_eq!(
        total_schedules,
        [0, 0],
        "rich factory native total scheduling floor"
    );
    // Every native field/count assertion above precedes any wire/Source assertion.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rich_factory_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        5,
        "rich factory independent Source root floor"
    );
    for (index, (wire, _host)) in observations.iter().enumerate() {
        let captures = wire.capture.borrow();
        assert_eq!(
            source[index]["attempts"].as_u64(),
            Some(if index == 4 { 3 } else { 1 }),
            "rich factory independent Source attempts"
        );
        assert_eq!(
            source[index]["clock"].as_u64(),
            Some(if index == 3 {
                0
            } else if index == 4 {
                3
            } else {
                1
            }),
            "rich factory independent Source signing clock floor"
        );
        for (attempt, request) in captures.iter().enumerate() {
            let expected = &source[index]["capture"][attempt];
            assert_eq!(
                format!("{}{}", request.base_url.as_deref().unwrap(), request.url),
                expected["wireURL"].as_str().unwrap(),
                "rich factory actual signing/wire URL {index}/{attempt}"
            );
            let header = |name: &str| {
                request
                    .headers
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(_, value)| value.as_str())
            };
            assert_eq!(
                header("Request-Time"),
                expected["time"].as_str(),
                "rich factory actual signing clock header {index}/{attempt}"
            );
            assert_eq!(
                header("Signature"),
                expected["signature"].as_str(),
                "rich factory actual HMAC signature {index}/{attempt}"
            );
        }
    }
}

#[test]
fn mexc_rich_query_recurses_without_reordering_native_inputs() {
    use safe_cex_transport::query::{
        QueryArrayFormat, QueryKeyOrder, QueryProfile, query_identity_eq, render_query,
    };
    use safe_cex_types::QueryValue as Q;
    fn object(fields: Vec<(&str, Q)>) -> Q {
        Q::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }
    let array = Q::Array(
        (0..11)
            .map(|index| Q::String(format!("v{index}")))
            .collect(),
    );
    let cases = [
        (
            object(vec![
                ("symbol", Q::String("BTC_USDT".into())),
                ("start_time", Q::Number(f64::NAN)),
                ("end_time", Q::Number(f64::NEG_INFINITY)),
                ("page_num", Q::Number(1.0)),
                ("page_size", Q::Number(100.0)),
            ]),
            "end_time=-Infinity&page_num=1&page_size=100&start_time=NaN&symbol=BTC_USDT",
        ),
        (
            object(vec![("z", array.clone())]),
            "z=v0&z=v1&z=v10&z=v2&z=v3&z=v4&z=v5&z=v6&z=v7&z=v8&z=v9",
        ),
        (
            array,
            "0=v0&1=v1&10=v10&2=v2&3=v3&4=v4&5=v5&6=v6&7=v7&8=v8&9=v9",
        ),
        (
            object(vec![
                (
                    "z",
                    object(vec![("b", Q::Number(2.0)), ("a", Q::Number(1.0))]),
                ),
                ("a", Q::String("first".into())),
            ]),
            "a=first&z%5Ba%5D=1&z%5Bb%5D=2",
        ),
        (
            object(vec![
                ("b", Q::Null),
                ("u", Q::Undefined),
                (
                    "z",
                    Q::Array(vec![
                        Q::Undefined,
                        Q::Null,
                        Q::Bool(false),
                        Q::Number(-0.0),
                        Q::String("-0".into()),
                    ]),
                ),
                ("empty", object(vec![])),
                ("a", Q::Array(vec![])),
            ]),
            "b=&z=&z=false&z=0&z=-0",
        ),
    ];
    let mut rendered = Vec::new();
    for (input, _) in &cases {
        let before = input.clone();
        rendered.push(
            render_query(
                input,
                QueryProfile {
                    arrays: QueryArrayFormat::Repeat,
                    encode: true,
                    key_order: QueryKeyOrder::JsRoot,
                },
            )
            .unwrap(),
        );
        assert!(
            query_identity_eq(input, &before),
            "recursive query native retained key/index order and number bits"
        );
    }
    assert_eq!(
        rendered.len(),
        5,
        "recursive query independent native renderer root floor"
    );
    for (index, ((_, expected), actual)) in cases.iter().zip(&rendered).enumerate() {
        assert_eq!(
            actual, expected,
            "recursive query static wire expectation {index}"
        );
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/query_profile_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        5,
        "recursive query independent Source root floor"
    );
    for (index, actual) in rendered.iter().enumerate() {
        assert_eq!(
            actual,
            source[index]["rendered"].as_str().unwrap(),
            "recursive query actual Source renderer {index}"
        );
    }
}

#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn mexc_factory_retains_native_response_units_and_typed_options_refusal() {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    struct NativeResponse(Arc<Wire>);
    impl RestAdapter for NativeResponse {
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            Box::pin(async move {
                let mut response = self.0.execute(request).await?;
                let mut units: Vec<u16> =
                    r#"{"code":0,"data":[{"symbol":""#.encode_utf16().collect();
                units.push(0xd800);
                units.extend(r#"","vol":2,"price":3}]}"#.encode_utf16());
                response.body = RestResponseBody::Untransformed(Input::Text(units));
                Ok(response)
            })
        }
    }
    fn field<'a>(fields: &'a [(Vec<u16>, Input)], name: &str) -> &'a Input {
        &fields
            .iter()
            .find(|(key, _)| key.iter().copied().eq(name.encode_utf16()))
            .unwrap()
            .1
    }
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/factory_oracle.mjs"
        ))
        .arg("--response-units")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        source.len(),
        2,
        "native response independent maintained-source phase census"
    );
    for row in &source {
        assert_eq!(
            row,
            &json!({"symbolUnits":[0xd800],"code":0,"vol":2,"price":3,"http":1}),
            "native response actual maintained-source units and fields"
        );
    }
    let refused = serde_json::from_str::<safe_cex_types::ExchangeOptions>(
        r#"{"key":"\ud800","secret":"fixture-secret"}"#,
    );
    assert!(
        refused.is_err(),
        "factory native whole-options lone-surrogate refusal before create_api"
    );
    let keys = ["é", "😃", "a\u{FEFF}\u{3000}b", " \t"];
    let expected_units: [&[u16]; 4] = [&[233], &[55357, 56835], &[97, 65279, 12288, 98], &[]];
    let mut captures = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        let runtime = Rc::new(Runtime::default());
        let host = Rc::new(Host::default());
        let wire = Arc::new(Wire {
            runtime: runtime.clone(),
            failures: 0,
            response: r#"{"code":0,"data":[{"symbol":"\ud800","vol":2,"price":3}]}"#.into(),
            capture: RefCell::new(vec![]),
        });
        let adapter: Arc<dyn RestAdapter> = if index % 2 == 0 {
            wire.clone()
        } else {
            Arc::new(NativeResponse(wire.clone()))
        };
        let api = create_api(
            HttpClientOptions {
                response_transform: None,
                params_serializer: None,
                exchange: ExchangeName::Mexc,
                options: serde_json::from_value(json!({"key":key,"secret":"fixture-secret"}))
                    .unwrap(),
                request: RestRequest::default(),
                provider: None,
                signer: None,
                account_name: None,
                adapter,
                host,
                runtime: runtime.clone(),
                store: None,
                stats: Some(Arc::default()),
                logs: Some(Arc::default()),
                every: vec![],
                periodic: vec![],
                next_id: Arc::new(|| "id".into()),
                signing_now: Arc::new(|| 0),
            },
            &TransportRegistry::default(),
            runtime.clone(),
            {
                let runtime = runtime.clone();
                Rc::new(move || ExchangeRuntime::now_ms(runtime.as_ref()))
            },
        )
        .unwrap();
        let response = futures::executor::block_on(api.execute(RestRequest {
            method: "get".into(),
            url: "/private".into(),
            query_params: Some(safe_cex_types::QueryValue::Object(Default::default())),
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(
            response.status, 200,
            "factory native Unicode response status"
        );
        let RestResponseBody::Transformed(Input::Object(fields)) = &response.body else {
            panic!("native response retains parsed object and transform phase")
        };
        assert_eq!(fields.len(), 2, "native response complete root");
        assert_eq!(
            field(fields, "code"),
            &Input::Float(0.0),
            "native response source code"
        );
        let Input::Array(rows) = field(fields, "data") else {
            panic!("native response data array")
        };
        assert_eq!(rows.len(), 1, "native response complete rows");
        let Input::Object(row) = &rows[0] else {
            panic!("native response row object")
        };
        assert_eq!(row.len(), 3, "native response complete row");
        assert_eq!(
            field(row, "symbol"),
            &Input::Text(vec![0xd800]),
            "native response exact source lone units before model projection"
        );
        assert_eq!(
            field(row, "vol"),
            &Input::Float(2.0),
            "native response volume"
        );
        assert_eq!(
            field(row, "price"),
            &Input::Float(3.0),
            "native response price"
        );
        assert_eq!(
            runtime.attempt.get(),
            1,
            "factory native independent Unicode adapter count"
        );
        assert_eq!(
            wire.capture.borrow().len(),
            1,
            "factory native independent Unicode capture count"
        );
        let observed = wire.capture.borrow();
        let request = &observed[0];
        let key = request
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("ApiKey"))
            .expect("factory native program-readable ApiKey header")
            .1;
        assert_eq!(
            key.encode_utf16().collect::<Vec<_>>(),
            expected_units[index],
            "factory native scalar/whitespace API-key units"
        );
        drop(observed);
        captures.push(wire);
    }
    assert_eq!(
        captures.len(),
        4,
        "factory native independent factory/GET call floor plus one whole-options refusal"
    );
    // Native decoded response/header/count assertions precede Source and wire.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rich_factory_oracle.mjs"
        ))
        .arg("--unicode")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        5,
        "factory independent Source factory/GET root floor"
    );
    assert_eq!(
        source[0]["capture"][0]["apiKeyUnits"],
        json!([55296]),
        "factory Source actual program-readable lone-surrogate ApiKey"
    );
    for (index, wire) in captures.iter().enumerate() {
        let expected = &source[index + 1];
        assert_eq!(
            expected["responseUnits"],
            json!([55296]),
            "factory Source actual normalized response symbol versus native transformed Null"
        );
        let observed = wire.capture.borrow();
        let request = &observed[0];
        assert_eq!(
            format!("{}{}", request.base_url.as_deref().unwrap(), request.url),
            expected["capture"][0]["wireURL"].as_str().unwrap(),
            "factory Unicode actual wire URL"
        );
        let signature = request
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("Signature"))
            .unwrap()
            .1;
        assert_eq!(
            signature,
            expected["capture"][0]["signature"].as_str().unwrap(),
            "factory Unicode actual native signature"
        );
        assert_eq!(
            expected["capture"][0]["apiKeyUnits"],
            serde_json::to_value(expected_units[index]).unwrap(),
            "factory Unicode Source scalar/whitespace header units"
        );
    }
}

fn rich_api(
    runtime: Rc<Runtime>,
    host: Rc<Host>,
    wire: Arc<Wire>,
    key: &str,
) -> impl RestAdapter + use<> {
    create_api(
        HttpClientOptions {
            response_transform: None,
            params_serializer: None,
            exchange: ExchangeName::Mexc,
            options: serde_json::from_value(json!({"key":key,"secret":"fixture-secret"})).unwrap(),
            request: RestRequest::default(),
            provider: None,
            signer: None,
            account_name: None,
            adapter: wire,
            host,
            runtime: runtime.clone(),
            store: None,
            stats: Some(Arc::default()),
            logs: Some(Arc::default()),
            every: vec![],
            periodic: vec![],
            next_id: Arc::new(|| "id".into()),
            signing_now: Arc::new(|| 0),
        },
        &TransportRegistry::default(),
        runtime.clone(),
        {
            let runtime = runtime.clone();
            Rc::new(move || ExchangeRuntime::now_ms(runtime.as_ref()))
        },
    )
    .unwrap()
}
#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn mexc_rich_exhausted_retry_retains_native_query_and_original_failure() {
    use safe_cex_transport::query::query_identity_eq;
    use safe_cex_types::QueryValue as Q;
    let input = Q::Object(
        [
            ("from".into(), Q::Number(f64::NAN)),
            ("to".into(), Q::Number(-0.0)),
        ]
        .into_iter()
        .collect(),
    );
    let runtime = Rc::new(Runtime::default());
    let host = Rc::new(Host::default());
    let wire = Arc::new(Wire {
        runtime: runtime.clone(),
        failures: 4,
        response: String::new(),
        capture: RefCell::new(vec![]),
    });
    let api = rich_api(runtime.clone(), host.clone(), wire.clone(), "fixture-key");
    let error = futures::executor::block_on(api.execute(RestRequest {
        method: "get".into(),
        url: "/private".into(),
        query_params: Some(input.clone()),
        ..Default::default()
    }))
    .unwrap_err();
    assert_eq!(
        error.code.as_deref(),
        Some("ECONNRESET"),
        "exhausted rich native original network code"
    );
    assert!(
        error.response.is_none(),
        "exhausted rich native absent response"
    );
    assert!(
        matches!(error.transport_error.as_deref(),Some(TransportError::Network(origin)) if origin.code()==Some("ECONNRESET")),
        "exhausted rich native original typed network cause"
    );
    assert_eq!(
        error.request.url, "/private",
        "exhausted rich native restored original URL"
    );
    assert_eq!(
        error.request.timeout_ms,
        Some(19997),
        "exhausted rich native remaining timeout"
    );
    assert!(
        error.request.params.is_none(),
        "exhausted rich native no legacy projection"
    );
    assert!(
        query_identity_eq(
            error
                .request
                .query_params
                .as_ref()
                .expect("exhausted rich native retained query carrier"),
            &input
        ),
        "exhausted rich native NaN/signed-zero bits and key order"
    );
    assert_eq!(
        runtime.attempt.get(),
        4,
        "exhausted rich independent native adapter attempt floor"
    );
    assert_eq!(
        runtime.retry_clock_calls.get(),
        7,
        "exhausted rich independent native retry clock floor"
    );
    assert_eq!(
        runtime.runtime_clock_calls.get(),
        12,
        "exhausted rich independent native signing/shared HTTP clock floor"
    );
    assert_eq!(
        *runtime.schedules.borrow(),
        [0, 0, 0],
        "exhausted rich independent native zero-delay scheduler floor"
    );
    let retained = host.rich.borrow();
    assert_eq!(
        retained.len(),
        4,
        "exhausted rich independent native pre-render call floor"
    );
    for request in retained.iter() {
        assert!(
            query_identity_eq(request.query_params.as_ref().unwrap(), &input),
            "exhausted rich native retained query at every attempt"
        );
    }
    let captures = wire.capture.borrow();
    assert_eq!(
        captures.len(),
        4,
        "exhausted rich independent native wire call floor"
    );
    // All native decoded fields/counts above precede wire and Source assertions.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rich_factory_oracle.mjs"
        ))
        .arg("--exhausted")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        1,
        "exhausted rich independent Source root floor"
    );
    let source = &source[0];
    assert_eq!(
        source["attempts"], 4,
        "exhausted rich independent Source adapter floor"
    );
    assert_eq!(
        source["clock"], 4,
        "exhausted rich independent Source signing clock floor"
    );
    assert_eq!(
        source["failure"]["code"], "ECONNRESET",
        "exhausted rich Source original code"
    );
    assert_eq!(
        source["failure"]["timeout"], 19997,
        "exhausted rich Source remaining timeout"
    );
    for (attempt, request) in captures.iter().enumerate() {
        let expected = &source["capture"][attempt];
        assert_eq!(
            format!("{}{}", request.base_url.as_deref().unwrap(), request.url),
            expected["wireURL"].as_str().unwrap(),
            "exhausted rich signing/wire URL {attempt}"
        );
        let signature = request
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("Signature"))
            .unwrap()
            .1;
        assert_eq!(
            signature,
            expected["signature"].as_str().unwrap(),
            "exhausted rich actual signature {attempt}"
        );
    }
}

struct ScriptedWire {
    runtime: Rc<Runtime>,
    steps: Vec<(u16, &'static str)>,
    capture: RefCell<Vec<RestRequest>>,
}
impl RestAdapter for ScriptedWire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.runtime.attempt.set(self.runtime.attempt.get() + 1);
            self.capture.borrow_mut().push(request.clone());
            let (status, body) = *self
                .steps
                .get(self.runtime.attempt.get() as usize - 1)
                .expect("native unscripted extra attempt");
            let response = RestResponse {
                status,
                reason: Some(status.to_string()),
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(body.as_bytes().to_vec()),
            };
            if (200..300).contains(&status) {
                return Ok(response);
            }
            Err(RestFailure {
                code: Some("ERR_BAD_REQUEST".into()),
                message: format!("Request failed with status code {status}"),
                transport_error: None,
                response: Some(Box::new(response)),
                request: Box::new(request),
                context: None,
            })
        })
    }
}
#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn mexc_signature_rejection_resends_once_with_a_fresh_signature() {
    use safe_cex_venue_mexc::api::{is_mexc_signature_rejection, sign_mexc_request};
    const REJECTION: &str =
        r#"{"success":false,"code":602,"message":"Signature verification failed!"}"#;
    const OK: &str = r#"{"code":0,"data":[]}"#;
    let scenarios: [(&str, &str, Vec<(u16, &'static str)>); 8] = [
        ("get-envelope", "get", vec![(200, REJECTION), (200, OK)]),
        ("post-envelope", "post", vec![(200, REJECTION), (200, OK)]),
        (
            "post-envelope-twice",
            "post",
            vec![(200, REJECTION), (200, REJECTION)],
        ),
        ("post-http", "post", vec![(401, REJECTION), (200, OK)]),
        (
            "get-http-twice",
            "get",
            vec![(401, REJECTION), (401, REJECTION)],
        ),
        (
            "get-http-then-envelope",
            "get",
            vec![(401, REJECTION), (200, REJECTION)],
        ),
        (
            "msg-only",
            "get",
            vec![
                (
                    200,
                    r#"{"success":false,"code":10,"msg":"signature VERIFICATION failed"}"#,
                ),
                (200, OK),
            ],
        ),
        (
            "not-signature",
            "post",
            vec![
                (
                    200,
                    r#"{"success":false,"code":401,"message":"No authority"}"#,
                ),
                (200, OK),
            ],
        ),
    ];
    let expected_native: [(usize, Value); 8] = [
        (2, json!({"ok":0})),
        (2, json!({"ok":0})),
        (
            2,
            json!({"error":"Signature verification failed!","status":200}),
        ),
        (2, json!({"ok":0})),
        (
            2,
            json!({"error":"Request failed with status code 401","status":401}),
        ),
        (
            2,
            json!({"error":"Signature verification failed!","status":200}),
        ),
        (2, json!({"ok":0})),
        (1, json!({"error":"No authority","status":200})),
    ];
    let mut native = Vec::new();
    for ((label, method, steps), (attempts, result)) in scenarios.into_iter().zip(expected_native) {
        let runtime = Rc::new(Runtime::default());
        let wire = Arc::new(ScriptedWire {
            runtime: runtime.clone(),
            steps,
            capture: RefCell::default(),
        });
        let api = create_api(
            HttpClientOptions {
                response_transform: None,
                params_serializer: None,
                exchange: ExchangeName::Mexc,
                options: serde_json::from_value(
                    json!({"key":"fixture-key","secret":"fixture-secret"}),
                )
                .unwrap(),
                request: RestRequest::default(),
                provider: None,
                signer: None,
                account_name: None,
                adapter: wire.clone(),
                host: Rc::new(Host::default()),
                runtime: runtime.clone(),
                store: None,
                stats: Some(Arc::default()),
                logs: Some(Arc::default()),
                every: vec![],
                periodic: vec![],
                next_id: Arc::new(|| "id".into()),
                signing_now: Arc::new(|| 0),
            },
            &TransportRegistry::default(),
            runtime.clone(),
            {
                let runtime = runtime.clone();
                Rc::new(move || ExchangeRuntime::now_ms(runtime.as_ref()))
            },
        )
        .unwrap();
        let request = if method == "get" {
            RestRequest {
                method: "get".into(),
                url: "/private".into(),
                params: Some(json!({"symbol":"BTC_USDT"})),
                ..Default::default()
            }
        } else {
            RestRequest {
                method: "post".into(),
                url: "/private".into(),
                body: Some(RestBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        json!({"symbol":"BTC_USDT","vol":1}),
                    )
                    .unwrap(),
                )),
                ..Default::default()
            }
        };
        let actual = match futures::executor::block_on(api.execute(request)) {
            Ok(response) => json!({"ok": response.body.json_value().unwrap()["code"]}),
            Err(error) => json!({
                "error": error.message,
                "status": error.response.as_ref().map(|response| response.status),
            }),
        };
        assert_eq!(
            actual, result,
            "MEXC signature resend native result {label}"
        );
        let captures = wire.capture.borrow();
        assert_eq!(
            captures.len(),
            attempts,
            "MEXC signature resend native attempt count {label}"
        );
        let mut wires = Vec::new();
        for (index, request) in captures.iter().enumerate() {
            let header = |name: &str| request.headers.get(name).cloned().unwrap();
            let time = header("Request-Time");
            assert_eq!(
                time,
                (1791800000123_u64 + index as u64).to_string(),
                "MEXC signature resend native fresh signing time {label}/{index}"
            );
            let data = request
                .body
                .as_ref()
                .map(|body| String::from_utf8(body.wire_bytes().unwrap()).unwrap());
            let param = data.clone().unwrap_or_else(|| "symbol=BTC_USDT".into());
            assert_eq!(
                header("Signature"),
                sign_mexc_request("fixture-secret", "fixture-key", &time, &param).unwrap(),
                "MEXC signature resend native fresh signature {label}/{index}"
            );
            wires.push(json!({"time":time,"signature":header("Signature"),"data":data}));
        }
        native.push(json!({"label":label,"attempts":attempts,"wire":wires,"result":actual}));
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/signature_resend_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let inputs = source["predicateInputs"].as_array().unwrap();
    assert_eq!(inputs.len(), 18, "MEXC signature predicate source floor");
    for (index, input) in inputs.iter().enumerate() {
        assert_eq!(
            Value::Bool(is_mexc_signature_rejection(input).unwrap()),
            source["predicate"][index],
            "MEXC signature predicate source equality {input}"
        );
    }
    assert_eq!(
        source["scenarios"].as_array().unwrap().len(),
        8,
        "MEXC signature resend source scenario floor"
    );
    for (index, native) in native.iter().enumerate() {
        assert_eq!(
            native, &source["scenarios"][index],
            "MEXC signature resend source equality {}",
            native["label"]
        );
    }
}
