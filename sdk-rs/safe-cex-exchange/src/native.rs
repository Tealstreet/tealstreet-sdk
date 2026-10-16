//! Production native host: a tokio `LocalSet` runtime, plain WebSocket dials and
//! an HTTP host that drives the supplied adapter once. Everything here must run
//! inside a `tokio::task::LocalSet` on a current-thread runtime.
use crate::{
    http::{HttpFuture, HttpHost},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{UpgradeHeaders, WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_transport::{
    DirectTransport, RestAdapter, RestFailure, RestRequest, RestResponse, TransportProvider,
    native::{NativeSocket, NativeTunnelHost, connect_websocket, spawn_local_task, wall_clock_ms},
    socket::{SocketFrame, TunnelHost, TunnelSocket},
};
use safe_cex_types::ExchangeName;
use serde_json::Value;
use std::{
    cell::Cell,
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::getrandom(&mut bytes).expect("operating system entropy is available");
    bytes
}
/// nanoid's default alphabet and length.
pub fn nanoid() -> String {
    const ALPHABET: &[u8] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";
    random_bytes::<21>().iter().map(|b| ALPHABET[(b & 63) as usize] as char).collect()
}
/// RFC 4122 version-4 UUID, as `uuid.v4()` spells it.
pub fn uuid_v4() -> String {
    let mut b = random_bytes::<16>();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

struct LocalTask(tokio::task::AbortHandle);
impl ExchangeTask for LocalTask {
    fn cancel(&self) {
        self.0.abort();
    }
}

/// `ExchangeRuntime` + `WsRuntime` on the current tokio `LocalSet`.
pub struct NativeRuntime {
    origin: Instant,
    transport: Arc<dyn TransportProvider>,
    tunnel: Rc<NativeTunnelHost>,
    min_reconnection_delay_ms: f64,
    library_min_reconnection_delay_ms: f64,
}
impl NativeRuntime {
    /// Direct transport: requests go straight to the venue, as `directTransport` does.
    pub fn new() -> Rc<Self> {
        Self::with_transport(Arc::new(DirectTransport))
    }
    pub fn with_transport(transport: Arc<dyn TransportProvider>) -> Rc<Self> {
        let random = random_unit();
        Rc::new(Self {
            origin: Instant::now(),
            transport,
            tunnel: Rc::new(NativeTunnelHost),
            min_reconnection_delay_ms: 1000.0 + random * 2000.0,
            library_min_reconnection_delay_ms: 1000.0 + random * 4000.0,
        })
    }
}
fn random_unit() -> f64 {
    (u64::from_le_bytes(random_bytes::<8>()) >> 11) as f64 / (1u64 << 53) as f64
}
impl ExchangeRuntime for NativeRuntime {
    fn monotonic_now_ms(&self) -> f64 {
        self.origin.elapsed().as_secs_f64() * 1000.0
    }
    fn now_ms(&self) -> f64 {
        wall_clock_ms()
    }
    fn sleep(&self, delay_ms: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async move {
            if delay_ms == 0 {
                tokio::task::yield_now().await;
            } else {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
            Ok(())
        })
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        Box::new(LocalTask(spawn_local_task(future)))
    }
    fn console(&self, severity: &str, message: &str) {
        eprintln!("[safe-cex {severity}] {message}");
    }
    fn unique_id(&self) -> String {
        nanoid()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        random_unit()
    }
    fn report_exception(&self, error: &ExchangeError, context: &Value) {
        eprintln!("[safe-cex uncaught] {}: {} {context}", error.name, error.message);
    }
    fn report_raw_exception(&self, error: &Value, context: &Value) {
        eprintln!("[safe-cex uncaught] {error} {context}");
    }
    fn execute_worker_request(&self, _request: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async {
            Err(ExchangeError::new(
                "the native host runs no worker; worker requests are unavailable",
            ))
        })
    }
}
impl WsRuntime for NativeRuntime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        self.transport.clone()
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        self.tunnel.clone()
    }
    fn dial(
        &self,
        url: String,
        protocols: Vec<String>,
        options: WebSocketConnectionOptions,
        reconnecting: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        Box::pin(async move {
            if reconnecting {
                return Err(ExchangeError::new(
                    "the native host dials plain sockets only; the core owns reconnection",
                ));
            }
            let headers: BTreeMap<String, String> = match &options.headers {
                None => BTreeMap::new(),
                Some(UpgradeHeaders::Static(map)) => map.clone().into_iter().collect(),
                Some(UpgradeHeaders::Fresh(provider)) => provider().into_iter().collect(),
            };
            let socket = connect_websocket(&url, &protocols, &headers)
                .await
                .map_err(|e| ExchangeError::new(e.to_string()))?;
            Ok(Rc::new(NativeWsSocket::new(socket)) as Rc<dyn WsSocket>)
        })
    }
    fn headers_capable(&self) -> bool {
        true
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        self.min_reconnection_delay_ms
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        self.library_min_reconnection_delay_ms
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

/// A plain (non-reconnecting) socket. Open is delivered first; after Close or
/// terminate the stream stays pending, as a listener-less socket would.
pub struct NativeWsSocket {
    inner: Rc<NativeSocket>,
    opened: Rc<Cell<bool>>,
    ended: Rc<Cell<bool>>,
}
impl NativeWsSocket {
    fn new(inner: NativeSocket) -> Self {
        Self {
            inner: Rc::new(inner),
            opened: Rc::new(Cell::new(false)),
            ended: Rc::new(Cell::new(false)),
        }
    }
}
impl WsSocket for NativeWsSocket {
    fn ready_state(&self) -> u8 {
        self.inner.ready_state() as u8
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        if !self.opened.replace(true) {
            return Box::pin(async { Ok(WsEvent::Open) });
        }
        let (inner, ended) = (self.inner.clone(), self.ended.clone());
        Box::pin(async move {
            if ended.get() {
                return std::future::pending().await;
            }
            match inner.receive().await {
                Ok(SocketFrame::Close { code, reason, .. }) => {
                    ended.set(true);
                    Ok(WsEvent::Close { code: Some(code), reason })
                }
                Ok(frame) if !ended.get() => Ok(WsEvent::Message(frame)),
                Ok(_) => std::future::pending().await,
                Err(error) => {
                    ended.set(true);
                    Ok(WsEvent::Error(error.to_string()))
                }
            }
        })
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.inner
            .send(SocketFrame::Text(message.to_owned()))
            .map_err(|e| ExchangeError::new(e.to_string()))
    }
    fn close(&self) {
        let _ = self.inner.close(Some(1000), "");
    }
    fn terminate(&self) {
        self.ended.set(true);
        let _ = self.inner.close(Some(1000), "");
    }
    fn reconnect(&self) {
        // The core reconnects only host-managed wrappers; a plain socket just closes.
        self.close();
    }
    fn is_reconnecting(&self) -> bool {
        false
    }
}

/// HTTP host for the native runtime: drives the adapter exactly once and keeps
/// its failure and request/response provenance intact.
#[derive(Default)]
pub struct NativeHttpHost;
impl HttpHost for NativeHttpHost {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, error: RestFailure, _exchange: &ExchangeName) -> RestFailure {
        error
    }
    fn emit(&self, _event: &str, _message: &str) -> Result<(), String> {
        Ok(())
    }
    fn note_success(&self, _exchange: &ExchangeName, _response: &RestResponse) {}
    fn note_failure(&self, _exchange: &ExchangeName, _error: &RestFailure) {}
    fn mark_start(&self, _request: &mut RestRequest) {}
    fn signing_capture(&self, _request: &RestRequest) -> bool {
        false
    }
    fn is_development(&self) -> bool {
        false
    }
    fn is_main_window(&self) -> bool {
        false
    }
}
