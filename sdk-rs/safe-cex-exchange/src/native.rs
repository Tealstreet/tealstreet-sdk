//! Production native host: a tokio `LocalSet` runtime, plain WebSocket dials and
//! an HTTP host that drives the supplied adapter once. Everything here must run
//! inside a `tokio::task::LocalSet` on a current-thread runtime.
use crate::{
    exchange::Exchange,
    http::{HttpFuture, HttpHost},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask, RuntimeTaskKind},
    websocket::{UpgradeHeaders, WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use safe_cex_transport::{
    DirectTransport, RestAdapter, RestFailure, RestRequest, RestResponse, TransportFuture,
    TransportProvider,
    how::{HowRestAdapter, big_int_as_string_json_parse},
    how_config::{CreateAxiosHttpOverWebsocketAdapterOptions, HOWRegion},
    native::{NativeSocket, NativeTunnelHost, connect_websocket, spawn_local_task, wall_clock_ms},
    provider::RestAdapterRegion,
    socket::{
        SocketCapabilities, SocketFrame, TunnelDialOptions, TunnelHost, TunnelSocket, TunnelTask,
    },
    tealstreet::{TealstreetTransport, TealstreetTransportHost},
};
use safe_cex_types::ExchangeName;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
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
    random_bytes::<21>()
        .iter()
        .map(|b| ALPHABET[(b & 63) as usize] as char)
        .collect()
}
/// RFC 4122 version-4 UUID, as `uuid.v4()` spells it.
pub fn uuid_v4() -> String {
    let mut b = random_bytes::<16>();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

struct LocalTask(tokio::task::AbortHandle);
impl ExchangeTask for LocalTask {
    fn cancel(&self) {
        self.0.abort();
    }
}

struct HostTask {
    future: Pin<Box<dyn Future<Output = ()>>>,
    runtime: Weak<NativeRuntime>,
    registration: Option<crate::task_receipts::Registration>,
}
impl Future for HostTask {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<()> {
        if let Some(runtime) = self.runtime.upgrade() {
            crate::js_runtime::jobs::checkpoint(runtime);
        }
        let result = if let Some(registration) = &self.registration {
            let (entered, waker, version) = registration.enter(cx.waker());
            let result = self
                .future
                .as_mut()
                .poll(&mut std::task::Context::from_waker(&waker));
            if result.is_pending() {
                self.registration
                    .as_ref()
                    .expect("observed task")
                    .pending(version);
            }
            drop(entered);
            result
        } else {
            self.future.as_mut().poll(cx)
        };
        if let Some(runtime) = self.runtime.upgrade() {
            crate::js_runtime::jobs::checkpoint(runtime);
        }
        result
    }
}

/// `ExchangeRuntime` + `WsRuntime` on the current tokio `LocalSet`.
pub struct NativeRuntime {
    pub replay_clock: Option<Rc<crate::replay_clock::ReplayClock>>,
    raw_tasks: Rc<RefCell<crate::task_receipts::Tasks>>,
    frame_counts: Rc<
        RefCell<
            Vec<(
                String,
                Rc<RefCell<safe_cex_transport::native::NativeFrameCounts>>,
            )>,
        >,
    >,
    origin: Instant,
    owner: Weak<NativeRuntime>,
    transport: Arc<dyn TransportProvider>,
    tunnel: Rc<dyn TunnelHost>,
    min_reconnection_delay_ms: f64,
    library_min_reconnection_delay_ms: f64,
}
/// Direct transport: requests go straight to the venue, as `directTransport` does.
pub fn native_runtime() -> Rc<NativeRuntime> {
    native_runtime_with_transport(Arc::new(DirectTransport))
}
pub fn native_runtime_with_transport(transport: Arc<dyn TransportProvider>) -> Rc<NativeRuntime> {
    runtime_over(transport, Rc::new(NativeTunnelHost), None)
}
/// Isolated diagnostic host: fixed startup timestamps, manually driven replay timers.
pub fn native_replay_runtime(epoch: f64) -> Rc<NativeRuntime> {
    safe_cex_store::virtual_clock::set_thread_host_time(Some(epoch));
    runtime_over(
        Arc::new(DirectTransport),
        Rc::new(NativeTunnelHost),
        Some(crate::replay_clock::new(epoch)),
    )
}
pub fn replay_checkpoint(runtime: &Rc<NativeRuntime>) {
    crate::js_runtime::jobs::checkpoint(runtime.clone());
}
pub fn replay_pending_jobs(runtime: &Rc<NativeRuntime>) -> usize {
    crate::js_runtime::jobs::pending_non_timer(runtime.clone())
}
/// Raw ExchangeRuntime work, including unclassified legacy spawn_local calls.
/// Promise drivers delegate to pendingJobs; unknown/ready/finite work blocks.
pub fn replay_raw_tasks(runtime: &NativeRuntime) -> Value {
    crate::task_receipts::receipt(&runtime.raw_tasks)
}
pub fn native_frame_receipts(runtime: &NativeRuntime) -> Vec<Value> {
    runtime
        .frame_counts
        .borrow()
        .iter()
        .map(|(url, counts)| serde_json::json!({"url": url, "counts": counts.borrow().clone()}))
        .collect()
}
/// The account's proxy token (ts: the accessToken facade; exchange-server's per-account provider).
pub type ProxyTokenSource = Rc<dyn Fn() -> TransportFuture<'static, String>>;
/// ts: registerTealstreetTransport for a native host. `tealstreetTunnel` REST goes over
/// the HOW tunnel and tunnel sockets authenticate with `token`.
pub fn native_tealstreet_runtime(token: ProxyTokenSource) -> Rc<NativeRuntime> {
    native_tealstreet_runtime_with(token, Rc::new(NativeTunnelHost))
}
/// As `native_tealstreet_runtime`, dialing tunnel sockets through `dials` (another origin or a test).
pub fn native_tealstreet_runtime_with(
    token: ProxyTokenSource,
    dials: Rc<dyn TunnelHost>,
) -> Rc<NativeRuntime> {
    let tunnel: Rc<dyn TunnelHost> = Rc::new(TokenTunnel { dials, token });
    runtime_over(tealstreet_transport(tunnel.clone()), tunnel, None)
}
// TealstreetTransport takes an Arc host; this one is local like every native future.
#[allow(clippy::arc_with_non_send_sync)]
fn tealstreet_transport(tunnel: Rc<dyn TunnelHost>) -> Arc<dyn TransportProvider> {
    Arc::new(TealstreetTransport::new(Arc::new(NativeTealstreetHost {
        tunnel,
    })))
}
fn runtime_over(
    transport: Arc<dyn TransportProvider>,
    tunnel: Rc<dyn TunnelHost>,
    replay_clock: Option<Rc<crate::replay_clock::ReplayClock>>,
) -> Rc<NativeRuntime> {
    let random = random_unit();
    Rc::new_cyclic(|owner| NativeRuntime {
        replay_clock,
        raw_tasks: Rc::default(),
        frame_counts: Rc::default(),
        origin: Instant::now(),
        owner: owner.clone(),
        transport,
        tunnel,
        min_reconnection_delay_ms: 1000.0 + random * 2000.0,
        library_min_reconnection_delay_ms: 1000.0 + random * 4000.0,
    })
}
/// A tunnel host that authenticates with the account's proxy token.
struct TokenTunnel {
    dials: Rc<dyn TunnelHost>,
    token: ProxyTokenSource,
}
impl TunnelHost for TokenTunnel {
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
        self.dials.spawn_local(future)
    }
    fn capabilities(&self) -> SocketCapabilities {
        self.dials.capabilities()
    }
    fn now_ms(&self) -> u64 {
        self.dials.now_ms()
    }
    fn sleep_until(&self, deadline_ms: u64) -> TransportFuture<'static, ()> {
        self.dials.sleep_until(deadline_ms)
    }
    fn connect(
        &self,
        url: String,
        options: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
        self.dials.connect(url, options)
    }
    fn proxy_token(&self) -> TransportFuture<'_, String> {
        (self.token)()
    }
    fn is_token_expired(&self, token: &str) -> bool {
        crate::proxy_token::is_token_expired(token, &VIRTUAL_CLOCK)
    }
    fn sync_server_time(&self, server_time: u64) {
        self.dials.sync_server_time(server_time)
    }
}
/// The Tealstreet transport's host: HOW adapters over the token tunnel. The source's
/// `markRestProxyHeader` stamps a diagnostic config property, which RestRequest has no field for.
struct NativeTealstreetHost {
    tunnel: Rc<dyn TunnelHost>,
}
// HOW adapters are local (Rc tunnel clients) behind the transport's Arc adapter type.
#[allow(clippy::arc_with_non_send_sync)]
fn how(
    tunnel: &Rc<dyn TunnelHost>,
    options: CreateAxiosHttpOverWebsocketAdapterOptions,
) -> Arc<dyn RestAdapter> {
    match HowRestAdapter::with_options(tunnel.clone(), options) {
        Ok(adapter) => Arc::new(adapter),
        Err(error) => Arc::new(UnavailableTunnel(error.to_string())),
    }
}
impl TealstreetTransportHost for NativeTealstreetHost {
    fn get_proxy_token(&self) -> TransportFuture<'_, String> {
        self.tunnel.proxy_token()
    }
    fn is_node_or_bun_runtime(&self) -> bool {
        true
    }
    fn mark_rest_proxy_header(&self, _request: &mut RestRequest, _shape: &'static str) {}
    fn default_adapter(&self) -> Arc<dyn RestAdapter> {
        how(
            &self.tunnel,
            CreateAxiosHttpOverWebsocketAdapterOptions {
                custom_parser: None,
                region: None,
            },
        )
    }
    fn big_int_adapter(&self) -> Arc<dyn RestAdapter> {
        how(
            &self.tunnel,
            CreateAxiosHttpOverWebsocketAdapterOptions {
                custom_parser: Some(Rc::new(|text: &str| Ok(big_int_as_string_json_parse(text)))),
                region: None,
            },
        )
    }
    fn regional_adapter(&self, region: Option<RestAdapterRegion>) -> Arc<dyn RestAdapter> {
        how(
            &self.tunnel,
            CreateAxiosHttpOverWebsocketAdapterOptions {
                custom_parser: None,
                region: match region {
                    Some(RestAdapterRegion::Ja) => Some(HOWRegion::JA),
                    Some(RestAdapterRegion::Sg) => Some(HOWRegion::SG),
                    Some(RestAdapterRegion::Other(_)) | None => None,
                },
            },
        )
    }
}
/// The tunnel clients could not be built; every request reports why.
struct UnavailableTunnel(String);
impl RestAdapter for UnavailableTunnel {
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        let message = format!("HOW tunnel unavailable: {}", self.0);
        Box::pin(async move { Err(RestFailure::pre_transmit(message, request)) })
    }
}
fn random_unit() -> f64 {
    (u64::from_le_bytes(random_bytes::<8>()) >> 11) as f64 / (1u64 << 53) as f64
}
impl NativeRuntime {
    fn spawn_observed(
        &self,
        kind: Option<RuntimeTaskKind>,
        origin: Option<&'static std::panic::Location<'static>>,
        future: Pin<Box<dyn Future<Output = ()>>>,
    ) -> Box<dyn ExchangeTask> {
        let registration = self
            .replay_clock
            .as_ref()
            .map(|_| crate::task_receipts::register(&self.raw_tasks, kind, origin));
        Box::new(LocalTask(spawn_local_task(Box::pin(HostTask {
            future,
            runtime: self.owner.clone(),
            registration,
        }))))
    }
}
impl ExchangeRuntime for NativeRuntime {
    fn monotonic_now_ms(&self) -> f64 {
        self.replay_clock.as_ref().map_or_else(
            || self.origin.elapsed().as_secs_f64() * 1000.0,
            |clock| crate::replay_clock::elapsed(clock),
        )
    }
    fn now_ms(&self) -> f64 {
        // JavaScript wall-clock timestamps (Date.now) have integer milliseconds.
        // Keep sub-millisecond precision on monotonic_now_ms for pacing/metrics.
        self.replay_clock.as_ref().map_or_else(
            || wall_clock_ms().floor(),
            |clock| crate::replay_clock::now(clock),
        )
    }
    fn sleep(&self, delay_ms: u64) -> ExchangeFuture<'static, ()> {
        let clock = self.replay_clock.clone();
        let deadline = self.monotonic_now_ms() + delay_ms as f64;
        let mut sleep = if let Some(clock) = &clock
            && delay_ms > 0
        {
            crate::replay_clock::sleep(clock, delay_ms)
        } else {
            Box::pin(async move {
                if delay_ms == 0 {
                    tokio::task::yield_now().await;
                } else {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                Ok(())
            }) as ExchangeFuture<'static, ()>
        };
        if clock.is_none() {
            return sleep;
        }
        let origin = self.origin;
        Box::pin(std::future::poll_fn(move |cx| {
            let result = sleep.as_mut().poll(cx);
            let now = clock.as_ref().map_or_else(
                || origin.elapsed().as_secs_f64() * 1000.0,
                |clock| crate::replay_clock::elapsed(clock),
            );
            if result.is_pending() && delay_ms > 0 && now < deadline {
                crate::task_receipts::waiting(RuntimeTaskKind::Timer);
            }
            result
        }))
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.spawn_observed(None, Some(std::panic::Location::caller()), future)
    }
    fn spawn_classified(
        &self,
        kind: RuntimeTaskKind,
        future: Pin<Box<dyn Future<Output = ()>>>,
    ) -> Box<dyn ExchangeTask> {
        self.spawn_observed(Some(kind), None, future)
    }
    fn task_work_started(&self) {
        crate::task_receipts::work_started();
    }
    fn console_values(&self, severity: &str, message: &[crate::js_runtime::value::Js]) {
        let rendered = crate::runtime::console_text(message);
        let message = rendered.as_str();
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
    fn report_exception(
        &self,
        error: &ExchangeError,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        let context = match context.json_stringify() {
            Ok(Some(text)) => text,
            Ok(None) => "undefined".into(),
            Err(error) => format!("<{}: {}>", error.name, error.message),
        };
        eprintln!(
            "[safe-cex uncaught] {}: {} {context}",
            error.name, error.message
        );
    }
    fn report_raw_exception(
        &self,
        error: &crate::js_runtime::value::Js,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        let description = crate::runtime::console_text(std::slice::from_ref(error));
        let context = match context.json_stringify() {
            Ok(Some(text)) => text,
            Ok(None) => "undefined".into(),
            Err(error) => format!("<{}: {}>", error.name, error.message),
        };
        eprintln!("[safe-cex uncaught] {description} {context}");
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
    fn frame_receipts_enabled(&self) -> bool {
        self.replay_clock.is_some()
    }
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
        let frame_counts = self.frame_counts.clone();
        let replay = self.replay_clock.is_some();
        Box::pin(async move {
            // Header-capable source sockets select the wrapper branch even
            // when the native core owns reconnects. Dial that authenticated
            // attempt as a plain socket; header-less wrapper requests still
            // require unsupported host-managed reconnection.
            if reconnecting && options.headers.is_none() {
                return Err(ExchangeError::new(
                    "the native host dials plain sockets only; the core owns reconnection",
                ));
            }
            let headers: BTreeMap<String, String> = match &options.headers {
                None => BTreeMap::new(),
                Some(UpgradeHeaders::Static(map)) => map.clone().into_iter().collect(),
                Some(UpgradeHeaders::Fresh(provider)) => provider().into_iter().collect(),
            };
            let socket = match connect_websocket(&url, &protocols, &headers).await {
                Ok(socket) => {
                    if replay {
                        frame_counts
                            .borrow_mut()
                            .push((url, socket.frame_counts.clone()));
                    }
                    Ok(socket)
                }
                Err(error @ safe_cex_transport::TransportError::Network(_)) => Err(error),
                Err(cause) => {
                    let mut error = ExchangeError::new(cause.to_string());
                    error.origin = Some(crate::runtime::ExchangeErrorOrigin::new(cause));
                    return Err(error);
                }
            };
            Ok(Rc::new(native_ws_socket(socket, replay)) as Rc<dyn WsSocket>)
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

/// A plain socket delivers Open or the failed attempt's Error/Close pair.
/// After Close or termination, event delivery stays pending.
struct NativeWsSocket {
    inner: Result<Rc<NativeSocket>, String>,
    opened: Rc<Cell<bool>>,
    ended: Rc<Cell<bool>>,
    observe_tasks: bool,
}
fn native_ws_socket(
    inner: Result<NativeSocket, safe_cex_transport::TransportError>,
    observe_tasks: bool,
) -> NativeWsSocket {
    NativeWsSocket {
        inner: inner.map(Rc::new).map_err(|error| error.to_string()),
        opened: Rc::new(Cell::new(false)),
        ended: Rc::new(Cell::new(false)),
        observe_tasks,
    }
}
impl NativeWsSocket {
    fn next_native_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let inner = match &self.inner {
            Ok(inner) => inner.clone(),
            Err(message) => {
                if self.ended.get() {
                    return Box::pin(std::future::pending());
                }
                let event = if !self.opened.replace(true) {
                    WsEvent::Error(message.clone())
                } else {
                    self.ended.set(true);
                    WsEvent::Close {
                        code: Some(1006),
                        reason: String::new(),
                    }
                };
                return Box::pin(std::future::ready(Ok(event)));
            }
        };
        if !self.opened.replace(true) {
            return Box::pin(async { Ok(WsEvent::Open) });
        }
        let ended = self.ended.clone();
        Box::pin(async move {
            if ended.get() {
                return std::future::pending().await;
            }
            match inner.receive().await {
                Ok(SocketFrame::Close { code, reason, .. }) => {
                    ended.set(true);
                    Ok(WsEvent::Close {
                        code: Some(code),
                        reason,
                    })
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
}
impl WsSocket for NativeWsSocket {
    fn ready_state(&self) -> u8 {
        self.inner
            .as_ref()
            .map_or(3, |inner| inner.ready_state() as u8)
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let mut event = self.next_native_event();
        if !self.observe_tasks {
            return event;
        }
        Box::pin(std::future::poll_fn(move |cx| {
            let result = event.as_mut().poll(cx);
            if result.is_pending() {
                crate::task_receipts::waiting(RuntimeTaskKind::SocketReader);
            }
            result
        }))
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.inner
            .as_ref()
            .map_err(|message| ExchangeError::new(message.clone()))?
            .send(SocketFrame::Text(message.to_owned()))
            .map_err(|e| ExchangeError::new(e.to_string()))
    }
    fn close(&self) {
        if let Ok(inner) = &self.inner {
            let _ = inner.close(Some(1000), "");
        }
    }
    fn terminate(&self) {
        self.ended.set(true);
        self.close();
    }
    fn reconnect(&self) {
        // The core reconnects only host-managed wrappers; a plain socket just closes.
        self.close();
    }
    fn is_reconnecting(&self) -> bool {
        false
    }
}

/// ts: the `exchange.emitter` safeCexAxiosCreate emits through. The account host
/// binds its constructed exchange here; an unbound host refuses visibly.
pub type NativeHttpEvents = Rc<RefCell<Option<Weak<dyn Exchange>>>>;

/// HTTP host for the native runtime: drives the adapter exactly once and keeps
/// its failure and request/response provenance intact.
#[derive(Default)]
pub struct NativeHttpHost {
    pub events: NativeHttpEvents,
}
impl HttpHost for NativeHttpHost {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, error: RestFailure, _exchange: &ExchangeName) -> RestFailure {
        error
    }
    fn emit(&self, event: &str, message: &str) -> Result<(), String> {
        let exchange = self
            .events
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| format!("native HTTP host has no exchange to emit {event}"))?;
        exchange
            .base()
            .emit(event, crate::js_runtime::value::Js::str(message))
            .map_err(|error| error.message)
    }
    fn note_response(&self, response: &RestResponse, request: &RestRequest) {
        if let Some(exchange) = self.events.borrow().as_ref().and_then(Weak::upgrade) {
            exchange.base().note_http_response(
                &serde_json::json!({"status":response.status,"statusText":response.reason}),
                Some(&request.url),
            );
        }
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
