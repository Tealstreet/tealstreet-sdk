//! The venue's LighterApiClient (from createLighterSDKClient) reached through the
//! public `Exchange` surface over a scripted JsHttpHost. Direct per-method
//! coverage lives in src/binding/sdk_client/tests.rs (the realm is crate-private).
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::{Js, from_json, to_json},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{TransportProvider, TransportRegistry, socket::TunnelHost};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_lighter::binding::exchange::{LighterDependencies, LighterNativeExchange};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);

#[derive(Default)]
struct Runtime {
    jobs: RefCell<Vec<Job>>,
    logs: RefCell<Vec<String>>,
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        self.0.set(true);
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        1_000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let flag = Rc::new(Cell::new(false));
        self.jobs.borrow_mut().push((future, flag.clone()));
        Box::new(Task(flag))
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        self.logs
            .borrow_mut()
            .push(format!("{severity}: {message}"));
    }
    fn unique_id(&self) -> String {
        "id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.0
    }
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &Value) {}
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Ok(Value::Null) })
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("no tunnel in sdk client tests")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        panic!("no sockets in sdk client tests")
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

struct Http {
    runtime: Rc<Runtime>,
    sent: RefCell<Vec<Value>>,
    replies: RefCell<VecDeque<Value>>,
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        self.sent.borrow_mut().push(to_json(&config).unwrap());
        let data = self.replies.borrow_mut().pop_front();
        Box::pin(async move {
            let data = data.expect("scripted reply");
            Ok(Js::object(vec![
                ("status", Js::Number(200.0)),
                ("data", from_json(&data)),
            ]))
        })
    }
}

fn venue(replies: Vec<Value>) -> (Rc<LighterNativeExchange>, Rc<Runtime>, Rc<Http>) {
    let runtime = Rc::new(Runtime::default());
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        sent: RefCell::default(),
        replies: RefCell::new(replies.into()),
    });
    let options: ExchangeOptions =
        serde_json::from_value(json!({ "key": "", "secret": "" })).unwrap();
    let venue = LighterNativeExchange::new(
        ExchangeName::Lighter,
        options,
        Rc::new(DefaultStore::new()),
        LighterDependencies {
            runtime: runtime.clone(),
            http: http.clone(),
            registry: Rc::new(TransportRegistry::default()),
            signing_entropy: None,
        },
    )
    .unwrap_or_else(|e| panic!("venue: {}", e.message));
    (venue, runtime, http)
}

fn drive<T>(runtime: &Runtime, future: ExchangeFuture<'_, T>) -> Result<T, ExchangeError> {
    let mut future = future;
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..256 {
        if let Poll::Ready(v) = future.as_mut().poll(&mut cx) {
            return v;
        }
        let jobs = std::mem::take(&mut *runtime.jobs.borrow_mut());
        for (mut job, cancelled) in jobs {
            if !cancelled.get() && job.as_mut().poll(&mut cx).is_pending() {
                runtime.jobs.borrow_mut().push((job, cancelled));
            }
        }
    }
    panic!("future never settled")
}

#[test]
fn fetch_markets_reads_through_the_client_get() {
    let (venue, runtime, http) = venue(vec![json!({ "code": 200, "order_book_details": [] })]);
    let markets = drive(&runtime, venue.fetch_markets()).unwrap();
    assert!(markets.is_empty());
    let sent = http.sent.borrow();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["method"], "get");
    assert_eq!(sent[0]["url"], "/api/v1/orderBookDetails");
    assert_eq!(sent[0]["__suppressExchangeError"], true);
    assert_eq!(sent[0]["headers"]["Content-Type"], "application/json");
}

#[test]
fn response_interceptor_error_emits_then_falls_back() {
    let (venue, runtime, http) = venue(vec![
        json!({ "error": { "code": 7, "message": "down" } }),
        json!({ "code": 200, "order_books": [] }),
    ]);
    let markets = drive(&runtime, venue.fetch_markets()).unwrap();
    assert!(markets.is_empty());
    let urls: Vec<Value> = http
        .sent
        .borrow()
        .iter()
        .map(|s| s["url"].clone())
        .collect();
    assert_eq!(
        urls,
        [
            json!("/api/v1/orderBookDetails"),
            json!("/api/v1/orderBooks")
        ]
    );
    let logs = runtime.logs.borrow();
    assert!(
        logs.iter().any(|l| l.contains("[7] down")),
        "emitAndLogError carried the interceptor message: {logs:?}"
    );
}
