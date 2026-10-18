// Native host fixtures for exchange accepted-set observations. Included only by
// instrumented exchange probes, never by the production module tree. Unexpected
// scheduling or I/O panics; these witnesses do not claim a production host.
use crate::{
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        scope::{Scope, ScopeShared},
        value::Js,
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask, LogSeverity},
    websocket::{
        BaseWebSocket, ReopenResync, WebSocketConnectionOptions, WsParent, WsProtocol, WsRuntime,
        WsSocket,
    },
};
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::ExchangeOptions;
use serde_json::{Value, json};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc, sync::Arc};

#[derive(Default)]
pub struct Runtime {
    pub reports: RefCell<Vec<Value>>,
}

impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        0.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("accepted fixture unexpectedly slept")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("accepted fixture unexpectedly scheduled work")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {
        panic!("accepted fixture unexpectedly logged")
    }
    fn unique_id(&self) -> String {
        panic!("accepted fixture unexpectedly requested an ID")
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        panic!("accepted fixture unexpectedly requested entropy")
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.reports.borrow_mut().push(json!({
            "name": error.name, "message": error.message, "context": context,
        }));
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("accepted fixture unexpectedly reported a raw exception")
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("accepted fixture unexpectedly called a worker")
    }
}

impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        panic!("accepted fixture unexpectedly requested a transport")
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("accepted fixture unexpectedly requested a tunnel")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        panic!("accepted fixture unexpectedly dialed")
    }
    fn headers_capable(&self) -> bool {
        panic!("unexpected header capability read")
    }
    fn enable_reconnecting(&self) -> bool {
        panic!("unexpected reconnect policy read")
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        panic!("unexpected reconnect delay read")
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        panic!("unexpected library delay read")
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

struct Http(Rc<Runtime>);
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, _: Js) -> JsFuture<'static> {
        panic!("accepted fixture unexpectedly sent HTTP")
    }
}

pub fn scope(globals: Js, this: Js) -> (Scope<()>, Rc<Runtime>) {
    let runtime = Rc::new(Runtime::default());
    let shared = Rc::new(ScopeShared::new(
        runtime.clone(),
        Rc::new(Http(runtime.clone())),
        Rc::new(DefaultStore::new()),
        globals,
        (),
    ));
    (Scope { shared, this }, runtime)
}

struct Parent;
impl WsParent for Parent {
    fn name(&self) -> &str {
        "accepted fixture"
    }
    fn is_disposed(&self) -> bool {
        false
    }
    fn options(&self) -> ExchangeOptions {
        panic!("unexpected socket options read")
    }
    fn store(&self) -> Rc<dyn Store> {
        panic!("unexpected parent Store read")
    }
    fn is_private_socket(&self, _: &BaseWebSocket) -> bool {
        false
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        panic!("unexpected resync")
    }
    fn fetch_and_set_private_snapshot(&self, _: usize) -> ExchangeFuture<'static, ()> {
        panic!("unexpected private fetch")
    }
    fn log(&self, _: &str, _: LogSeverity) -> Result<(), ExchangeError> {
        panic!("unexpected parent log")
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), ExchangeError> {
        panic!("unexpected parent event")
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        panic!("unexpected socket health update")
    }
    fn update_latency(&self, _: f64) -> Result<(), ExchangeError> {
        panic!("unexpected latency update")
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        panic!("unexpected reconnect event")
    }
    fn emit_and_log_error(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        panic!("unexpected parent error")
    }
}
struct Protocol;
impl WsProtocol for Protocol {}

pub fn socket(runtime: Rc<Runtime>) -> Rc<BaseWebSocket> {
    BaseWebSocket::new(1, Rc::new(Parent), runtime, Rc::new(Protocol))
}
