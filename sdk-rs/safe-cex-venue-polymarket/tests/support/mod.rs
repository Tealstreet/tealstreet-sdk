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
pub fn run<F: Future>(future: F) -> F::Output {
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
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &serde_json::Value) {
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
pub fn client(
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
pub fn response(body: RestResponseBody) -> RestResponse {
    RestResponse {
        status: 200,
        reason: Some("OK".into()),
        headers: Default::default(),
        body,
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
