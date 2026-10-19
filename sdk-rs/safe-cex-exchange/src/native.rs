//! Production native host: a tokio `LocalSet` runtime, plain WebSocket dials and
//! an HTTP host that drives the supplied adapter once. Everything here must run
//! inside a `tokio::task::LocalSet` on a current-thread runtime.
use crate::{
    exchange::Exchange,
    http::{HttpFuture, HttpHost},
    owned_tasks::OwnedTasks,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask, RuntimeTaskKind},
    waiters::Waiters,
    websocket::{
        FrameDrainReceipt, UpgradeHeaders, WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket,
    },
};
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use safe_cex_transport::{
    DirectTransport, RestAdapter, RestFailure, RestRequest, RestResponse, TransportFuture,
    TransportProvider,
    how::{HowRestAdapter, big_int_as_string_json_parse},
    how_config::{CreateAxiosHttpOverWebsocketAdapterOptions, HOWRegion},
    native::{NativeSocket, NativeTunnelHost, prepare_websocket, spawn_local_task, wall_clock_ms},
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
    parser_receipts: RefCell<Vec<Rc<RefCell<Vec<FrameDrainReceipt>>>>>,
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
/// Parser histories from every diagnostic socket, including nested/disposed owners.
pub fn native_parser_receipts(runtime: &NativeRuntime) -> Vec<FrameDrainReceipt> {
    runtime
        .parser_receipts
        .borrow()
        .iter()
        .flat_map(|receipts| receipts.borrow().clone())
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
        parser_receipts: RefCell::default(),
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
/// Native proxy authorization retains its diagnostic marker outside HTTP headers.
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
    fn mark_rest_proxy_header(&self, request: &mut RestRequest, shape: &'static str) {
        request.config_extensions.insert(
            "safeCexRestDiagProxyHeader".into(),
            safe_cex_types::retained_json::RetainedJsonInput::Text(shape.encode_utf16().collect()),
        );
    }
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
    fn register_frame_receipts(&self, receipts: Rc<RefCell<Vec<FrameDrainReceipt>>>) {
        if self.replay_clock.is_some() {
            self.parser_receipts.borrow_mut().push(receipts);
        }
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
        let runtime: Rc<dyn ExchangeRuntime> = self.owner.upgrade().expect("live native runtime");
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
            let prepared = prepare_websocket(&url, &protocols, &headers).map_err(|cause| {
                let mut error = ExchangeError::new(cause.to_string());
                error.origin = Some(crate::runtime::ExchangeErrorOrigin::new(cause));
                error
            })?;
            let connecting = Box::pin(async move {
                let socket = prepared.await?;
                if replay {
                    frame_counts
                        .borrow_mut()
                        .push((url, socket.frame_counts.clone()));
                }
                Ok(socket)
            });
            Ok(native_ws_socket(&runtime, connecting) as Rc<dyn WsSocket>)
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

/// One host handle exists from constructor admission through handshake completion.
/// The socket owns cancellation; its pending event waiter does not own the dial.
#[derive(Clone)]
enum NativeSocketState {
    Connecting,
    Connected(Rc<NativeSocket>),
    Failed(String),
    Terminated,
}
struct NativeWsSocket {
    inner: Rc<RefCell<NativeSocketState>>,
    opened: Rc<Cell<bool>>,
    ended: Rc<Cell<bool>>,
    closing: Cell<bool>,
    opening: Rc<OwnedTasks>,
    waiters: Waiters,
}
fn native_ws_socket(
    runtime: &Rc<dyn ExchangeRuntime>,
    connecting: TransportFuture<'static, NativeSocket>,
) -> Rc<NativeWsSocket> {
    let socket = Rc::new(NativeWsSocket {
        inner: Rc::new(RefCell::new(NativeSocketState::Connecting)),
        opened: Rc::new(Cell::new(false)),
        ended: Rc::new(Cell::new(false)),
        closing: Cell::new(false),
        opening: Rc::new(OwnedTasks::default()),
        waiters: Waiters::default(),
    });
    let inner = Rc::downgrade(&socket.inner);
    let waiters = socket.waiters.clone();
    socket.opening.start_eager(
        runtime,
        move |_| {
            Box::pin(async move {
                let result = connecting.await;
                if let Some(inner) = inner.upgrade() {
                    *inner.borrow_mut() = match result {
                        Ok(socket) => NativeSocketState::Connected(Rc::new(socket)),
                        Err(error) => NativeSocketState::Failed(error.to_string()),
                    };
                    waiters.wake_all();
                }
            })
        },
        |_| {},
    );
    socket
}
impl WsSocket for NativeWsSocket {
    fn ready_state(&self) -> u8 {
        if self.closing.get() {
            return 2;
        }
        match &*self.inner.borrow() {
            NativeSocketState::Connecting => 0,
            NativeSocketState::Connected(inner) => inner.ready_state() as u8,
            NativeSocketState::Failed(_) | NativeSocketState::Terminated => 3,
        }
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let state = self.inner.clone();
        let opened = self.opened.clone();
        let ended = self.ended.clone();
        let registration = self.waiters.register();
        Box::pin(async move {
            let inner = std::future::poll_fn(|cx| {
                if ended.get() {
                    return std::task::Poll::Pending;
                }
                let current = state.borrow().clone();
                match current {
                    NativeSocketState::Connecting => {
                        registration.update(cx.waker());
                        std::task::Poll::Pending
                    }
                    NativeSocketState::Terminated => std::task::Poll::Pending,
                    inner => std::task::Poll::Ready(inner),
                }
            })
            .await;
            let inner = match inner {
                NativeSocketState::Connected(inner) => inner,
                NativeSocketState::Failed(message) => {
                    return Ok(if !opened.replace(true) {
                        WsEvent::Error(message)
                    } else {
                        ended.set(true);
                        WsEvent::Close {
                            code: Some(1006),
                            reason: String::new(),
                        }
                    });
                }
                NativeSocketState::Connecting | NativeSocketState::Terminated => unreachable!(),
            };
            if !opened.replace(true) {
                return Ok(WsEvent::Open);
            }
            let mut receiving = inner.receive();
            let frame = std::future::poll_fn(|cx| {
                if ended.get() {
                    return std::task::Poll::Ready(None);
                }
                registration.update(cx.waker());
                let result = receiving.as_mut().poll(cx);
                if result.is_pending() {
                    // Only the physical receive wait is exempt. CONNECTING
                    // still has finite handshake work and must block drain.
                    // The observer is a no-op outside a tracked replay task.
                    crate::task_receipts::waiting(RuntimeTaskKind::SocketReader);
                }
                result.map(Some)
            })
            .await;
            drop(receiving);
            drop(inner);
            let Some(frame) = frame else {
                return std::future::pending().await;
            };
            match frame {
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
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        match &*self.inner.borrow() {
            NativeSocketState::Connecting => {
                let mut error = ExchangeError::new("Sent before connected.");
                error.name = "InvalidStateError".into();
                Err(error)
            }
            NativeSocketState::Connected(inner) => inner
                .send(SocketFrame::Text(message.to_owned()))
                .map_err(|error| ExchangeError::new(error.to_string())),
            NativeSocketState::Failed(_) | NativeSocketState::Terminated => Ok(()),
        }
    }
    fn close(&self) {
        let inner = self.inner.borrow().clone();
        match inner {
            NativeSocketState::Connecting => {
                self.closing.set(true);
                // WsEvent::Error carries the source's normalized error message:
                // Event.message is empty, so BaseWebSocket falls back to type.
                *self.inner.borrow_mut() = NativeSocketState::Failed("error".into());
                self.opening.close();
                self.waiters.wake_all();
            }
            NativeSocketState::Connected(inner) => {
                let _ = inner.close(Some(1000), "");
            }
            NativeSocketState::Failed(_) | NativeSocketState::Terminated => {}
        }
    }
    fn terminate(&self) {
        self.ended.set(true);
        self.opening.close();
        let inner = self.inner.replace(NativeSocketState::Terminated);
        if let NativeSocketState::Connected(inner) = inner {
            let _ = inner.close(Some(1000), "");
        }
        self.waiters.wake_all();
    }
    fn reconnect(&self) {
        self.close();
    }
    fn is_reconnecting(&self) -> bool {
        false
    }
}
impl Drop for NativeWsSocket {
    fn drop(&mut self) {
        self.terminate();
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
        self.emit_value(event, &crate::js_runtime::value::Js::str(message))
            .map_err(|error| error.to_exchange_error().message)
    }
    fn emit_value(
        &self,
        event: &str,
        message: &crate::js_runtime::value::Js,
    ) -> Result<(), crate::js_runtime::value::Thrown> {
        let owner = self
            .events
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| {
                crate::js_runtime::value::Thrown::error(
                    "Error",
                    "native HTTP exchange owner released",
                )
            })?;
        owner
            .base()
            .emit(event, message.clone())
            .map_err(Into::into)
    }
    fn telemetry_endpoints(
        &self,
        name: &str,
    ) -> Option<Result<crate::js_runtime::value::Js, crate::js_runtime::value::Thrown>> {
        self.events
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .map(|owner| owner.base().http_endpoints(name))
    }
    fn record_request_outcome(&self, outcome: safe_cex_types::RequestOutcome) {
        if let Some(owner) = self.events.borrow().as_ref().and_then(Weak::upgrade) {
            drop(owner.base().store.record_request_outcome(outcome));
        }
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
    fn signing_capture(&self, request: &RestRequest) -> bool {
        request
            .config_extensions
            .get("__tealstreetSigningCapture")
            .is_some_and(safe_cex_num::js_retained_truthy)
    }
    fn is_development(&self) -> bool {
        crate::http_telemetry::is_development()
    }
    fn is_main_window(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "native_connecting_tests.rs"]
mod connecting_tests;
