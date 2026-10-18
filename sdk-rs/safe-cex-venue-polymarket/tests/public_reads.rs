use safe_cex_exchange::{
    http::{HttpClientOptions, HttpFailure, HttpFuture, HttpHost, safe_cex_axios_create_no_proxy},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_transport::rate_limit::RateLimitClock;
use safe_cex_transport::*;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{
    cell::Cell,
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
#[derive(Default)]
struct Runtime {
    monotonic_ms: Rc<Cell<f64>>,
}
impl ExchangeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        1700000000123.0
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
    fn sleep_ms(&self, delay_ms: u64) -> TransportFuture<'_, ()> {
        let clock = self.monotonic_ms.clone();
        Box::pin(async move {
            clock.set(clock.get() + delay_ms as f64);
            Ok(())
        })
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
    fn note_success(
        &self,
        _: &ExchangeName,
        _: &RestResponse,
        _request: &safe_cex_transport::RestRequest,
    ) {
    }
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

use safe_cex_venue_polymarket::public_reads::get_server_time;
use std::process::Command;
struct Adapter {
    calls: Arc<Mutex<Vec<RestRequest>>>,
    result: Result<RestResponse, RestFailure>,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.calls.lock().unwrap().push(request);
        Box::pin(async { self.result.clone() })
    }
}
fn client(
    result: Result<RestResponse, RestFailure>,
    calls: Arc<Mutex<Vec<RestRequest>>>,
) -> safe_cex_exchange::http::HttpClient {
    safe_cex_axios_create_no_proxy(HttpClientOptions {
        response_transform: None,
        params_serializer: None,
        exchange: ExchangeName::Polymarket,
        options: serde_json::from_value(json!({"key":"", "secret":""})).unwrap(),
        request: RestRequest::default(),
        provider: None,
        signer: None,
        account_name: None,
        adapter: Arc::new(Adapter { calls, result }),
        host: Rc::new(Host),
        runtime: Rc::new(Runtime::default()),
        store: None,
        stats: None,
        logs: None,
        every: vec![],
        periodic: vec![],
        next_id: Arc::new(|| "id".into()),
        signing_now: Arc::new(|| 0),
    })
}
fn response(body: RestResponseBody) -> RestResponse {
    RestResponse {
        status: 200,
        reason: Some("OK".into()),
        headers: Default::default(),
        body,
    }
}
#[test]
fn server_time_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public_reads_oracle.cjs"
        ))
        .output()
        .unwrap();
    eprintln!("polymarket public reads oracle: one Node invocation for the entire corpus");
    assert!(
        output.status.success(),
        "time_oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("capture header: {}", corpus["capture"]);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 20, "time_source_case_count");
    let (mut numbers, mut refused) = (0, 0);
    for index in 0..20 {
        let case = cases.get(index).expect("time_source_case_present");
        assert_eq!(
            case["calls"],
            json!([["/time"]]),
            "time_source_requests:{index}"
        );
        let raw = case["raw"].as_str().unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        let bodies = [
            RestResponseBody::Encoded(raw.as_bytes().to_vec()),
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(value.clone())
                    .unwrap(),
            ),
            RestResponseBody::Transformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap(),
            ),
        ];
        let mut performed = 0;
        for body in bodies {
            let calls = Arc::new(Mutex::new(vec![]));
            let result = run(get_server_time(&client(
                Ok(response(body.clone())),
                calls.clone(),
            )));
            let calls = calls.lock().unwrap();
            assert_eq!(calls.len(), 1, "time_native_request_count:{index}");
            assert_eq!(calls[0].url, case["calls"][0][0], "time_native_url:{index}");
            assert_eq!(calls[0].method, "get", "time_native_method:{index}");
            assert!(
                calls[0].params.is_none() && calls[0].body.is_none(),
                "time_native_no_payload:{index}"
            );
            if case["type"] == "number" {
                assert_eq!(
                    format!("{:016x}", result.unwrap().to_bits()),
                    case["bits"],
                    "time_native_bits:{index}"
                );
                numbers += 1;
            } else {
                let error = result.expect_err("time_typed_boundary");
                assert_eq!(
                    error.response.unwrap().body,
                    body,
                    "time_decode_response_retained:{index}"
                );
                refused += 1;
            }
            performed += 1;
        }
        assert_eq!(performed, 3, "time_per_case_comparisons:{index}");
    }
    assert_eq!(numbers, 42, "time_numeric_comparisons");
    assert_eq!(refused, 18, "time_boundary_comparisons");
    let failures = corpus["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 3, "time_source_failure_count");
    for (index, code) in ["ERR_NETWORK", "ERR_BAD_RESPONSE", "ERR_CANCELED"]
        .iter()
        .enumerate()
    {
        assert_eq!(failures[index]["code"], *code, "time_source_failure_code");
        assert_eq!(
            failures[index]["preserved"], true,
            "time_source_failure_identity"
        );
        let expected = RestFailure {
            code: Some((*code).into()),
            transport_error: None,
            message: "fixture failure".into(),
            response: Some(Box::new(RestResponse {
                status: 429,
                ..response(RestResponseBody::Transformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        json!({"error":"limit"}),
                    )
                    .unwrap(),
                ))
            })),
            request: Box::new(RestRequest {
                url: "/changed".into(),
                ..Default::default()
            }),
            context: Some(Box::new(json!({"id":7}))),
        };
        let calls = Arc::new(Mutex::new(vec![]));
        let actual = run(get_server_time(&client(
            Err(expected.clone()),
            calls.clone(),
        )))
        .unwrap_err();
        assert_eq!(actual, expected, "time_failure_metadata:{index}");
        assert_eq!(calls.lock().unwrap().len(), 1, "time_failure_request_count");
    }
}

#[test]
fn http_fixture_monotonic_clock_tracks_both_sleep_paths() {
    let runtime = Runtime::default();
    assert_eq!(runtime.monotonic_now_ms(), 0.0);
    run(runtime.sleep(13)).unwrap();
    assert_eq!(runtime.monotonic_now_ms(), 13.0);
    run(RateLimitClock::sleep_ms(&runtime, 7)).unwrap();
    assert_eq!(runtime.monotonic_now_ms(), 20.0);
}
