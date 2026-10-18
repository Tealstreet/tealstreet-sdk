// Primitive runtime ports delegate to the existing scripted host. Retain the
// actual console/reporter Js arguments before any diagnostic projection.
use super::host;
use safe_cex_exchange::{
    js_runtime::value::Js,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::retained_json::RetainedJsonInput;
use serde_json::Value;
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc, sync::Arc};
#[derive(Default)]
pub struct Runtime {
    inner: host::Runtime,
    pub raw_reports: RefCell<Vec<Js>>,
    pub error_console: RefCell<Vec<Vec<Js>>>,
}
impl Runtime {
    pub fn pump(&self) {
        self.inner.pump();
    }
}
impl ExchangeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        self.inner.now_ms()
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.inner.monotonic_now_ms()
    }
    fn sleep(&self, ms: u64) -> ExchangeFuture<'static, ()> {
        self.inner.sleep(ms)
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.inner.spawn_local(f)
    }
    fn console_values(&self, level: &str, values: &[Js]) {
        if level == "error" {
            self.error_console.borrow_mut().push(values.to_vec());
        }
        self.inner.console_values(level, values);
    }
    fn unique_id(&self) -> String {
        self.inner.unique_id()
    }
    fn is_worker(&self) -> bool {
        self.inner.is_worker()
    }
    fn random(&self) -> f64 {
        self.inner.random()
    }
    fn report_exception(&self, error: &ExchangeError, context: &RetainedJsonInput) {
        self.inner.report_exception(error, context);
    }
    fn report_raw_exception(&self, error: &Js, context: &RetainedJsonInput) {
        self.raw_reports.borrow_mut().push(error.clone());
        self.inner.report_raw_exception(error, context);
    }
    fn execute_worker_request(&self, value: Value) -> ExchangeFuture<'static, Value> {
        self.inner.execute_worker_request(value)
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        self.inner.transport()
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        self.inner.tunnel_host()
    }
    fn dial(
        &self,
        url: String,
        protocols: Vec<String>,
        options: WebSocketConnectionOptions,
        reconnecting: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        self.inner.dial(url, protocols, options, reconnecting)
    }
    fn headers_capable(&self) -> bool {
        self.inner.headers_capable()
    }
    fn enable_reconnecting(&self) -> bool {
        self.inner.enable_reconnecting()
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        self.inner.min_reconnection_delay_ms()
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        self.inner.library_min_reconnection_delay_ms()
    }
    fn diagnostics_enabled(&self) -> bool {
        self.inner.diagnostics_enabled()
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        self.inner.lifecycle_logs_enabled()
    }
}
