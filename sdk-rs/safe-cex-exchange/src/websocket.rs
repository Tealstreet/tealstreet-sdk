//! Shared socket lifecycle; venue protocol callbacks and host I/O remain injected.
use crate::owned_tasks::{OwnedOperation, OwnedTasks};
#[cfg(test)]
use crate::runtime::ExchangeTask;
use crate::runtime::{
    ExchangeError, ExchangeFuture, ExchangeRuntime, LogSeverity, RuntimeTaskKind,
};
use safe_cex_store::Store;
pub use safe_cex_transport::relay::{
    FreshHeadersProvider, UpgradeHeaders, WebSocketConnectionOptions,
};
use safe_cex_transport::{
    TransportProvider, WsConnection,
    fallback::WsDirectFallback,
    relay::{RelayEvent, RelayWebSocket},
    socket::{SocketFrame, TunnelHost},
};
use safe_cex_types::ExchangeOptions;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
};

pub const WS_PING_THRESHOLD_MS: f64 = 6_000.0;
pub const WS_RECONNECT_THRESHOLD_MS: f64 = 20_000.0;
pub const WS_CLOSE_LOG_COOLDOWN_MS: f64 = 15_000.0;
pub const WS_ERROR_LOG_COOLDOWN_MS: f64 = 15_000.0;

pub trait WsSocket {
    fn ready_state(&self) -> u8;
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent>;
    fn send(&self, message: &str) -> Result<(), ExchangeError>;
    fn close(&self);
    fn terminate(&self);
    fn reconnect(&self);
    fn is_reconnecting(&self) -> bool;
}
pub enum WsEvent {
    Open,
    Message(SocketFrame),
    Error(String),
    Close { code: Option<u16>, reason: String },
}
pub trait WsRuntime: ExchangeRuntime {
    fn transport(&self) -> Arc<dyn TransportProvider>;
    fn tunnel_host(&self) -> Rc<dyn TunnelHost>;
    fn dial(
        &self,
        url: String,
        protocols: Vec<String>,
        options: WebSocketConnectionOptions,
        reconnecting: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>>;
    fn headers_capable(&self) -> bool;
    fn enable_reconnecting(&self) -> bool;
    fn min_reconnection_delay_ms(&self) -> f64;
    /// Stable library default (1000 + random * 4000), sampled once by the host.
    /// Separate from SDK tuning's stable 1000 + random * 2000 value.
    fn library_min_reconnection_delay_ms(&self) -> f64;
    fn diagnostics_enabled(&self) -> bool;
    fn lifecycle_logs_enabled(&self) -> bool;
    /// Explicit replay-host instrumentation; ordinary hosts retain no dial history.
    fn frame_receipts_enabled(&self) -> bool {
        false
    }
    /// Retain replay counters independently of exchange-level socket grouping.
    fn register_frame_receipts(&self, _receipts: Rc<RefCell<Vec<FrameDrainReceipt>>>) {}
}
pub trait WsParent {
    fn name(&self) -> &str;
    fn is_disposed(&self) -> bool;
    fn options(&self) -> ExchangeOptions;
    fn store(&self) -> Rc<dyn Store>;
    fn is_private_socket(&self, socket: &BaseWebSocket) -> bool;
    fn resync_coordinator(&self) -> Rc<ReopenResync>;
    fn fetch_and_set_private_snapshot(&self, retries: usize) -> ExchangeFuture<'static, ()>;
    fn log(&self, message: &str, severity: LogSeverity) -> Result<(), ExchangeError>;
    fn emit(&self, event: &str, message: &str) -> Result<(), ExchangeError>;
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError>;
    fn update_latency(&self, latency: f64) -> Result<(), ExchangeError>;
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError>;
    fn emit_and_log_error(
        &self,
        error: &str,
        message: Option<&str>,
        extra: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError>;
}
/// Dial inputs produced by an owned asynchronous preparation operation.
pub struct PreparedWebSocket {
    pub url: String,
    pub protocols: Vec<String>,
    pub options: Option<WebSocketConnectionOptions>,
}

pub trait WsProtocol {
    fn dispose(&self) {}

    fn reconnect(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        socket.reconnect_base()
    }

    fn recovers_after_offline(&self) -> bool {
        true
    }

    /// Source receive timers dispatch the instance's checkLastReceived override.
    fn check_last_received(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        socket.check_last_received()
    }
    fn ping(&self, _socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        Err(ExchangeError::new("Not implemented"))
    }
    fn connect_and_subscribe(
        &self,
        _socket: Rc<BaseWebSocket>,
        _is_reconnect: bool,
    ) -> Result<(), ExchangeError> {
        Err(ExchangeError::new("Not implemented"))
    }
    fn on_open(&self, _socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn on_message(
        &self,
        _socket: &Rc<BaseWebSocket>,
        _frame: &SocketFrame,
    ) -> Result<(), ExchangeError> {
        Err(ExchangeError::new("Not implemented"))
    }
    /// Later error listeners run only when grouped dispatch ignores the base
    /// listener's stopImmediatePropagation, including after disposal.
    fn on_error(&self, _socket: &BaseWebSocket, _message: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    /// True when the venue replaced the base close handler (a TS subclass `onClose`
    /// arrow field): a transport close then skips the core's close handling.
    fn owns_close_handling(&self) -> bool {
        false
    }
    /// The transport's `close` event, after the core's own close handling (if it
    /// runs); TS fires venue close listeners for it.
    fn on_transport_close(
        &self,
        _socket: &Rc<BaseWebSocket>,
        _code: Option<u16>,
        _reason: &str,
    ) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn recovery_complete(&self) -> bool {
        true
    }
    fn watchdog_stale_threshold_ms(&self) -> Option<f64> {
        None
    }
    fn watchdog_ping_threshold_ms(&self) -> Option<f64> {
        None
    }
    fn resyncs_private_state_on_reopen(&self) -> Option<bool> {
        None
    }
}

type Formatter = Rc<dyn Fn(&Value) -> Option<String>>;
type Redactor = Rc<dyn Fn(&str) -> Option<String>>;
type ChannelFilter = Rc<dyn Fn(&Value) -> Result<bool, ExchangeError>>;
type SendFormatter = Rc<dyn Fn(&str) -> Option<String>>;
pub type MessageHandler = Rc<dyn Fn(&crate::js_runtime::value::Js) -> Result<(), ExchangeError>>;
pub type HandlerList = Rc<RefCell<Vec<MessageHandler>>>;
/// Source topic statistics using the existing native statistics record.
pub type MessageHandlersStats = indexmap::IndexMap<String, MessageHandlerStat>;
/// Source topic callback lists using the existing native callback carrier.
pub type MessageHandlers = indexmap::IndexMap<String, HandlerList>;
#[derive(Default)]
pub struct WsDiagnosticReceiveOptions {
    pub allow_channel: Option<ChannelFilter>,
    pub format: Option<Formatter>,
    pub redact: Option<Redactor>,
}
#[derive(Default)]
pub struct WsDiagnosticSendOptions {
    pub format: Option<SendFormatter>,
    pub redact: Option<Redactor>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderCapableWebSocketUnavailableError;
impl HeaderCapableWebSocketUnavailableError {
    pub fn new() -> Self {
        Self
    }
}
impl Default for HeaderCapableWebSocketUnavailableError {
    fn default() -> Self {
        Self::new()
    }
}
impl std::fmt::Display for HeaderCapableWebSocketUnavailableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Required WebSocket upgrade headers need a header-capable ws transport")
    }
}
impl std::error::Error for HeaderCapableWebSocketUnavailableError {}

#[derive(Debug, Clone, Default)]
pub struct MessageHandlerStat {
    pub last_message_timestamp: f64,
    pub last_callbacks_count: safe_cex_num::PrototypeScalar,
    pub had_handlers: Option<bool>,
    /// An own undefined value suppresses prototype fallback; None alone is absent.
    pub had_handlers_undefined: bool,
}
#[derive(Default)]
pub struct OrderBookInnerState {
    pub updates: Vec<Value>,
    pub is_snapshot_loaded: bool,
}
#[derive(Default)]
struct ResyncState {
    last: Option<(f64, Rc<()>)>,
    running: bool,
    rerun: bool,
}
#[derive(Default)]
pub struct ReopenResync {
    state: RefCell<ResyncState>,
}
impl ReopenResync {
    fn request(self: &Rc<Self>, socket: &Rc<BaseWebSocket>) {
        let now = socket.now();
        let mut state = self.state.borrow_mut();
        if state.last.as_ref().is_some_and(|(at, identity)| {
            !Rc::ptr_eq(identity, &socket.identity) && now - at < 2_000.0
        }) {
            return;
        }
        state.last = Some((now, socket.identity.clone()));
        if state.running {
            state.rerun = true;
            return;
        }
        state.running = true;
        drop(state);
        run_reopen_resync(self.clone(), socket.parent.clone(), socket.runtime.clone());
    }
}
// Cancellation must leave the account-wide latch recoverable, even before polling.
struct ReopenResyncGuard {
    coordinator: Rc<ReopenResync>,
    armed: bool,
}
impl ReopenResyncGuard {
    fn disarm(&mut self) {
        self.armed = false;
    }
}
impl Drop for ReopenResyncGuard {
    fn drop(&mut self) {
        if self.armed {
            let mut state = self.coordinator.state.borrow_mut();
            state.running = false;
            state.rerun = false;
        }
    }
}
fn run_reopen_resync(
    coordinator: Rc<ReopenResync>,
    parent: Rc<dyn WsParent>,
    runtime: Rc<dyn WsRuntime>,
) {
    let mut guard = ReopenResyncGuard {
        coordinator: coordinator.clone(),
        armed: true,
    };
    let first = parent.fetch_and_set_private_snapshot(10);
    let reporting_runtime = runtime.clone();
    runtime.spawn_local(Box::pin(async move {
        let mut next = first;
        loop {
            let result = next.await;
            let mut yielded = false;
            std::future::poll_fn(move |cx| {
                if yielded {
                    std::task::Poll::Ready(())
                } else {
                    yielded = true;
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                }
            })
            .await;
            let log_error = if let Err(error) = result {
                parent
                    .log(
                        &format!(
                            "Private resync after re-open failed: {}",
                            error.js_error_string()
                        ),
                        LogSeverity::Warning,
                    )
                    .err()
            } else {
                None
            };
            // Source schedules the queued run in finally, including when its
            // catch logger throws. Do that before projecting the rejection.
            let rerun = {
                let mut state = coordinator.state.borrow_mut();
                let rerun = state.rerun && !parent.is_disposed();
                state.rerun = false;
                state.running = rerun;
                rerun
            };
            let followup = if rerun {
                Some(parent.fetch_and_set_private_snapshot(10))
            } else {
                guard.disarm();
                None
            };
            if let Some(error) = log_error {
                reporting_runtime.report_exception(
                    &error,
                    &serde_json::json!({"operation":"private resync log"}).into(),
                );
            }
            if let Some(future) = followup {
                next = future;
            } else {
                break;
            }
        }
        guard.disarm();
    }));
}

pub fn lifecycle_log(runtime: &dyn WsRuntime, message: &str) {
    if runtime.lifecycle_logs_enabled() {
        runtime.console("log", message);
    }
}
pub fn get_node_web_socket(runtime: &dyn WsRuntime) -> bool {
    runtime.headers_capable()
}
pub fn get_close_code_description(code: u16) -> String {
    let description = match code {
        1000 => "Normal closure",
        1001 => "Going away (server shutdown or browser navigating away)",
        1002 => "Protocol error",
        1003 => "Unsupported data type",
        1005 => "No status code received",
        1006 => "Abnormal closure (connection lost)",
        1007 => "Invalid frame payload data",
        1008 => "Policy violation",
        1009 => "Message too big",
        1010 => "Missing extension",
        1011 => "Internal server error",
        1012 => "Service restart",
        1013 => "Try again later",
        1014 => "Bad gateway",
        1015 => "TLS handshake failure",
        _ => return format!("Unknown close code ({code})"),
    };
    description.into()
}
pub fn is_heartbeat_frame(message: &str) -> bool {
    let trimmed = safe_cex_num::js_trim(message);
    if trimmed.eq_ignore_ascii_case("ping") || trimmed.eq_ignore_ascii_case("pong") {
        return true;
    }
    let Ok(Value::Object(payload)) = serde_json::from_str::<Value>(trimmed) else {
        return false;
    };
    payload.contains_key("ping")
        || payload.contains_key("pong")
        || ["op", "type", "event", "method"].iter().any(|key| {
            payload
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|value| {
                    let lower = value.to_ascii_lowercase();
                    matches!(
                        lower.as_str(),
                        "ping" | "pong" | "server.ping" | "server.pong"
                    )
                })
        })
}

#[derive(Default)]
struct Dedup {
    signature: Option<String>,
    last_at: f64,
    suppressed: usize,
}
impl Dedup {
    fn message(
        &mut self,
        signature: String,
        now: f64,
        cooldown: f64,
        base: &str,
        noun: &str,
    ) -> Option<String> {
        let same = self.signature.as_ref() == Some(&signature);
        if same && now - self.last_at < cooldown {
            self.suppressed += 1;
            return None;
        }
        let suppressed = if same { self.suppressed } else { 0 };
        self.signature = Some(signature);
        self.last_at = now;
        self.suppressed = 0;
        Some(if suppressed > 0 {
            format!("{base} (suppressed {suppressed} duplicate {noun})")
        } else {
            base.into()
        })
    }
}
fn relay_retry_exhausted(next_attempt: i32, maximum: f64) -> bool {
    // RWS checks its previous counter (initially -1), then increments. Comparing
    // the next counter with > is not equivalent for fractional limits.
    f64::from(next_attempt) - 1.0 >= maximum
}

fn relay_reconnection_delay(
    options: &WebSocketConnectionOptions,
    library_minimum: f64,
    failures: i32,
) -> f64 {
    let minimum = options.min_reconnection_delay_ms.unwrap_or(library_minimum);
    let maximum = options.max_reconnection_delay_ms.unwrap_or(10_000.0);
    let factor = options.reconnection_delay_grow_factor.unwrap_or(1.3);
    let delay = minimum * factor.powi(failures);
    // RWS uses an ordered comparison, not f64::min's NaN handling.
    if delay > maximum { maximum } else { delay }
}
#[derive(Clone)]
struct RelayDescriptor {
    url: String,
    options: WebSocketConnectionOptions,
}
#[derive(Clone)]
struct ResolvedConnection {
    descriptor: RelayDescriptor,
    relay: bool,
    reconnecting: bool,
}
struct DormantSocket {
    owner: std::rc::Weak<BaseWebSocket>,
    connection: ResolvedConnection,
    operation: Rc<RefCell<Option<(u64, OwnedOperation)>>>,
    generation: Cell<u64>,
}
impl WsSocket for DormantSocket {
    fn ready_state(&self) -> u8 {
        3
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        Box::pin(std::future::pending())
    }
    fn send(&self, _: &str) -> Result<(), ExchangeError> {
        Err(ExchangeError::new("WebSocket is not open"))
    }
    fn close(&self) {
        let operation = self.operation.borrow_mut().take();
        if let Some((generation, operation)) = operation {
            if let Some(owner) = self.owner.upgrade()
                && owner.state.borrow().generation == generation
            {
                owner.reserve_socket_generation();
            }
            operation.cancel();
        }
    }
    fn terminate(&self) {
        self.close();
    }
    fn reconnect(&self) {
        let previous_generation = self.generation.get();
        self.close();
        if self.generation.get() != previous_generation {
            return;
        }
        let Some(owner) = self.owner.upgrade().filter(|owner| !owner.is_disposed()) else {
            return;
        };
        let current = owner
            .state
            .borrow()
            .native
            .as_ref()
            .is_some_and(|native| std::ptr::addr_eq(Rc::as_ptr(native), self));
        if !current {
            return;
        }
        let generation = owner.reserve_socket_generation();
        self.generation.set(generation);
        let operation_slot = self.operation.clone();
        let cleanup_slot = self.operation.clone();
        let weak = self.owner.clone();
        let runtime = owner.runtime.clone();
        let connection = self.connection.clone();
        owner.owned_tasks.start(
            owner.runtime.as_ref(),
            move |operation| {
                *operation_slot.borrow_mut() = Some((generation, operation));
                Box::pin(async move {
                    // Preserve the deferred constructor even when spawn/sleep poll inline.
                    let mut yielded = false;
                    std::future::poll_fn(move |cx| {
                        if std::mem::replace(&mut yielded, true) {
                            std::task::Poll::Ready(())
                        } else {
                            cx.waker().wake_by_ref();
                            std::task::Poll::Pending
                        }
                    })
                    .await;
                    let result = async {
                        runtime.sleep(0).await?;
                        let Some(owner) = weak.upgrade() else {
                            return Ok(());
                        };
                        if owner.is_disposed() || owner.state.borrow().generation != generation {
                            return Ok(());
                        }
                        owner.open_resolved_connection(connection, generation).await
                    }
                    .await;
                    if let Err(error) = result
                        && let Some(owner) = weak.upgrade()
                        && !owner.is_disposed()
                        && owner.state.borrow().generation == generation
                    {
                        owner.report_callback_result(
                            owner.handle_web_socket_error(&error.to_string()),
                        );
                    }
                })
            },
            move |_| {
                let mut slot = cleanup_slot.borrow_mut();
                if slot.as_ref().is_some_and(|(run, _)| *run == generation) {
                    slot.take();
                }
            },
        );
    }
    fn is_reconnecting(&self) -> bool {
        true
    }
}
#[derive(Default)]
struct SocketState {
    native: Option<Rc<dyn WsSocket>>,
    relay: Option<Rc<RelayWebSocket>>,
    relay_open: bool,
    relay_descriptor: Option<RelayDescriptor>,
    relay_task: Option<u64>,
    last_close_time: f64,
    on_close_count: usize,
    reconnect_count: usize,
    lifetime_close_count: usize,
    last_close_code: Option<u16>,
    last_close_at: f64,
    last_received: f64,
    last_connected_at: f64,
    ping_sent: bool,
    initialized: bool,
    ping_at: f64,
    reconnecting: bool,
    backoff_attempts: usize,
    backoff_code: Option<u16>,
    backoff_last_failure_at: f64,
    close_failures: usize,
    pause_until: f64,
    pause_announced: bool,
    connect_failure_announced: bool,
    close_log: Dedup,
    error_log: Dedup,
    diagnostic_window: f64,
    diagnostic_frames: usize,
    diagnostic_suppressed: usize,
    opened_before: bool,
    reconnect_listener_generation: Option<u64>,
    register_reconnect_open: bool,
    provisional_reconnect_open: Option<u64>,
    force_closed: bool,
    last_stats_reconcile: f64,
    tasks: HashMap<Timer, OwnedOperation>,
    timer_ids: HashMap<Timer, u64>,
    generation: u64,
    relay_operation: Option<OwnedOperation>,
    next_task: u64,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SocketTimeout {
    Ping,
    PongMiss,
    Receive,
}
impl From<SocketTimeout> for Timer {
    fn from(timer: SocketTimeout) -> Self {
        match timer {
            SocketTimeout::Ping => Self::Ping,
            SocketTimeout::PongMiss => Self::PongMiss,
            SocketTimeout::Receive => Self::Receive,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Timer {
    Ping,
    PongMiss,
    Receive,
    Reconnect,
}

enum SocketTaskKey {
    Timer(Timer, u64),
    Owned(u64),
}
// Construct before pinning: host discard must release the exact owned slot.
struct SocketTaskGuard {
    socket: std::rc::Weak<BaseWebSocket>,
    key: SocketTaskKey,
    completed: Rc<Cell<bool>>,
}
impl SocketTaskGuard {
    fn release(&mut self) {
        self.finish(true);
    }
    fn complete(&mut self) {
        self.finish(false);
    }
    fn finish(&mut self, cancelled: bool) {
        if self.completed.replace(true) {
            return;
        }
        let Some(socket) = self.socket.upgrade() else {
            return;
        };
        let (task, relay) = {
            let mut state = socket.state.borrow_mut();
            let task = match self.key {
                SocketTaskKey::Timer(timer, id) if state.timer_ids.get(&timer) == Some(&id) => {
                    if cancelled && timer == Timer::Reconnect {
                        state.reconnecting = false;
                    }
                    state.timer_ids.remove(&timer);
                    state.tasks.remove(&timer)
                }
                SocketTaskKey::Owned(id) if state.relay_task == Some(id) => {
                    state.relay_operation.take()
                }
                _ => None,
            };
            let relay = match self.key {
                SocketTaskKey::Owned(id) if state.relay_task == Some(id) => {
                    state.relay_task = None;
                    state.relay_open = false;
                    state.relay.take()
                }
                _ => None,
            };
            (task, relay)
        };
        if let Some(relay) = relay {
            relay.close(1000, "");
        }
        drop(task);
    }
}
impl Drop for SocketTaskGuard {
    fn drop(&mut self) {
        self.release();
    }
}

/// A parser-boundary receipt, retained separately for each physical dial.
/// `processed` includes failed handlers; `errors` makes those failures explicit.
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameDrainReceipt {
    pub url: String,
    pub generation: u64,
    pub consumed: u64,
    pub processed: u64,
    pub errors: u64,
    pub close_consumed: u64,
    pub close_processed: u64,
}

pub struct BaseWebSocket {
    pub id: u64,
    frame_receipts: Rc<RefCell<Vec<FrameDrainReceipt>>>,
    identity: Rc<()>,
    parent: Rc<dyn WsParent>,
    runtime: Rc<dyn WsRuntime>,
    protocol: Rc<dyn WsProtocol>,
    state: RefCell<SocketState>,
    owned_tasks: Rc<OwnedTasks>,
    pub order_book_inner_state: RefCell<HashMap<String, OrderBookInnerState>>,
    pub message_handlers: RefCell<indexmap::IndexMap<String, HandlerList>>,
    pub message_handlers_stats: RefCell<MessageHandlersStats>,
    pub prototype_realm: Rc<safe_cex_num::PrototypeRealm>,
    pub last_log: RefCell<VecDeque<String>>,
    pub treat_any_message_as_pong: Cell<bool>,
    pub direct_fallback: RefCell<WsDirectFallback>,
    connect_disabled: Cell<bool>,
    grouped_listener_dispatch: Cell<bool>,
    before_close: RefCell<Option<Rc<dyn Fn()>>>,
}
impl BaseWebSocket {
    pub fn new(
        id: u64,
        parent: Rc<dyn WsParent>,
        runtime: Rc<dyn WsRuntime>,
        protocol: Rc<dyn WsProtocol>,
    ) -> Rc<Self> {
        let frame_receipts = Rc::default();
        if runtime.frame_receipts_enabled() {
            runtime.register_frame_receipts(Rc::clone(&frame_receipts));
        }
        Rc::new(Self {
            id,
            frame_receipts,
            identity: Rc::new(()),
            parent,
            runtime,
            protocol,
            state: RefCell::new(SocketState::default()),
            owned_tasks: Rc::new(OwnedTasks::default()),
            order_book_inner_state: RefCell::new(HashMap::new()),
            message_handlers: RefCell::new(indexmap::IndexMap::new()),
            message_handlers_stats: RefCell::new(indexmap::IndexMap::new()),
            prototype_realm: safe_cex_num::PrototypeRealm::current(),
            last_log: RefCell::new(VecDeque::with_capacity(20)),
            treat_any_message_as_pong: Cell::new(false),
            direct_fallback: RefCell::new(WsDirectFallback::new(2)),
            connect_disabled: Cell::new(false),
            grouped_listener_dispatch: Cell::new(false),
            before_close: RefCell::new(None),
        })
    }
    // Runtime event delivery is the host boundary for an uncaught listener throw.
    fn report_callback_result(&self, result: Result<(), ExchangeError>) {
        if let Err(error) = result {
            self.runtime.report_exception(
                &error,
                &serde_json::json!({"operation":"websocket callback"}).into(),
            );
        }
    }
    fn received_transport_close(
        self: &Rc<Self>,
        code: Option<u16>,
        reason: &str,
    ) -> Result<(), ExchangeError> {
        let index = self.frame_receipts.borrow().len().checked_sub(1);
        if let Some(index) = index {
            self.frame_receipts.borrow_mut()[index].close_consumed += 1;
        }
        let result = self.transport_close(code, reason);
        if let Some(index) = index {
            self.frame_receipts.borrow_mut()[index].close_processed += 1;
        }
        result
    }
    pub fn frame_drain_receipts(&self) -> Vec<FrameDrainReceipt> {
        self.frame_receipts.borrow().clone()
    }
    fn now(&self) -> f64 {
        self.runtime.now_ms()
    }
    pub fn is_disposed(&self) -> bool {
        self.parent.is_disposed()
    }
    pub(crate) fn grouped_listener_dispatch(&self) -> bool {
        self.grouped_listener_dispatch.get()
    }
    pub fn is_recovery_complete(&self) -> bool {
        self.protocol.recovery_complete()
    }
    pub fn store(&self) -> Rc<dyn Store> {
        self.parent.store()
    }
    pub fn emitter(&self) -> &dyn WsParent {
        self.parent.as_ref()
    }
    pub fn reset_orderbook(&self) {
        for book in self.order_book_inner_state.borrow_mut().values_mut() {
            book.updates.clear();
            book.is_snapshot_loaded = false;
        }
    }
    pub fn ping(self: &Rc<Self>) -> Result<(), ExchangeError> {
        self.protocol.ping(self)
    }
    pub fn should_enforce_pong_miss_reconnect(&self) -> bool {
        !self.treat_any_message_as_pong.get()
            && !(self.state.borrow().last_received > 0.0
                && self.now() - self.state.borrow().last_received < WS_RECONNECT_THRESHOLD_MS)
    }
    pub fn get_watchdog_stale_threshold_ms(&self) -> Option<f64> {
        self.protocol.watchdog_stale_threshold_ms()
    }
    pub fn get_watchdog_ping_threshold_ms(&self) -> Option<f64> {
        self.protocol.watchdog_ping_threshold_ms()
    }
    pub fn mark_ping_sent(&self) {
        self.state.borrow_mut().ping_at = self.runtime.monotonic_now_ms();
    }
    /// Writes the ping time a venue recorded itself (`this.pingAt = performance.now()`).
    pub fn set_ping_at(&self, at: f64) {
        self.state.borrow_mut().ping_at = at;
    }
    /// Replaces the heartbeat deadline shared with pong and close handling.
    pub fn schedule_ping(self: &Rc<Self>, delay_ms: u64) {
        self.schedule(Timer::Ping, delay_ms as f64);
    }
    /// Identity of the currently owned timeout, shared with source field handles.
    pub(crate) fn timeout_id(&self, timer: SocketTimeout) -> Option<u64> {
        self.state.borrow().timer_ids.get(&timer.into()).copied()
    }
    /// A stale source handle cannot cancel a replacement in the same slot.
    pub(crate) fn cancel_timeout(&self, timer: SocketTimeout, id: u64) {
        if self.timeout_id(timer) == Some(id) {
            self.cancel(timer.into());
        }
    }
    pub fn handle_pong_event(self: &Rc<Self>, next_ping_time: u64) -> Result<(), ExchangeError> {
        self.record_message_handler_stats("pong")?;
        let ping_at = self.state.borrow().ping_at;
        if ping_at != 0.0 {
            self.parent
                .update_latency(((self.runtime.monotonic_now_ms() - ping_at) / 2.0).round())?;
        }
        self.schedule(Timer::Ping, next_ping_time as f64);
        self.cancel(Timer::PongMiss);
        if self.should_enforce_pong_miss_reconnect() {
            self.schedule(Timer::PongMiss, 1.5 * next_ping_time as f64);
        }
        Ok(())
    }
    pub fn record_message_handler_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        use safe_cex_num::PrototypeScalar as Scalar;
        if topic.contains("publicTrade") && !topic.contains("BTCUSDT") && !topic.contains("ETHUSDT")
        {
            return Ok(());
        }
        let now = self.now();
        let count = self.handler_stats_count(topic, 1)?;
        let has = self.handler_stats_present(topic)
            && safe_cex_num::PrototypeValue::from(count.clone()).truthy();
        let own = self.message_handlers_stats.borrow().contains_key(topic);
        if own {
            let mut stats = self.message_handlers_stats.borrow_mut();
            let entry = stats.get_mut(topic).expect("own statistics entry");
            entry.last_message_timestamp = now;
            entry.last_callbacks_count = count;
            if has {
                entry.had_handlers = Some(true);
                entry.had_handlers_undefined = false;
            }
        } else {
            let existing = self.prototype_realm.inherited(topic);
            if existing.truthy() {
                self.prototype_realm
                    .set(&existing, "lastMessageTimestamp", Scalar::Number(now))
                    .map_err(Self::stats_type_error)?;
                self.prototype_realm
                    .set(&existing, "lastCallbacksCount", count)
                    .map_err(Self::stats_type_error)?;
                if has {
                    self.prototype_realm
                        .set(&existing, "hadHandlers", Scalar::Boolean(true))
                        .map_err(Self::stats_type_error)?;
                }
            } else {
                self.message_handlers_stats.borrow_mut().insert(
                    topic.into(),
                    MessageHandlerStat {
                        last_message_timestamp: now,
                        last_callbacks_count: count,
                        had_handlers: Some(has),
                        had_handlers_undefined: false,
                    },
                );
            }
        }
        self.reconcile_message_handler_stats(now, false)
    }
    fn stats_type_error(message: String) -> ExchangeError {
        let mut error = ExchangeError::new(message);
        error.name = "TypeError".into();
        error
    }
    fn reconcile_message_handler_stats(&self, now: f64, force: bool) -> Result<(), ExchangeError> {
        let mut state = self.state.borrow_mut();
        if now - state.last_stats_reconcile < if force { 250.0 } else { 5000.0 } {
            return Ok(());
        }
        // Source commits the throttle before walking, including when a later
        // inherited primitive causes the walk to throw.
        state.last_stats_reconcile = now;
        drop(state);
        let own: Vec<String> = {
            let stats = self.message_handlers_stats.borrow();
            safe_cex_num::js_own_keys(stats.keys().map(String::as_str))
                .into_iter()
                .map(str::to_owned)
                .collect()
        };
        let mut keys = own.clone();
        keys.extend(
            self.prototype_realm
                .enumerable_keys()
                .into_iter()
                .filter(|key| !own.contains(key)),
        );
        for key in keys {
            let own_had = self
                .message_handlers_stats
                .borrow()
                .get(&key)
                .map(|entry| (entry.had_handlers, entry.had_handlers_undefined));
            let had = match own_had {
                Some((Some(had), _)) => had,
                Some((None, true)) => false,
                Some((None, false)) => self.prototype_realm.inherited("hadHandlers").truthy(),
                None => self
                    .prototype_realm
                    .get(&self.prototype_realm.inherited(&key), "hadHandlers")
                    .map_err(Self::stats_type_error)?
                    .truthy(),
            };
            if !had {
                continue;
            }
            let count = self.handler_stats_count(&key, 0)?;
            if own_had.is_some() {
                self.message_handlers_stats
                    .borrow_mut()
                    .get_mut(&key)
                    .expect("own statistics entry")
                    .last_callbacks_count = count;
            } else {
                self.prototype_realm
                    .set(
                        &self.prototype_realm.inherited(&key),
                        "lastCallbacksCount",
                        count,
                    )
                    .map_err(Self::stats_type_error)?;
            }
        }
        Ok(())
    }
    fn handler_stats_present(&self, topic: &str) -> bool {
        self.message_handlers.borrow().contains_key(topic)
            || self.prototype_realm.inherited(topic).truthy()
    }
    fn handler_stats_count(
        &self,
        topic: &str,
        missing: usize,
    ) -> Result<safe_cex_num::PrototypeScalar, ExchangeError> {
        use safe_cex_num::PrototypeScalar as Scalar;
        use safe_cex_num::PrototypeValue as Native;
        if let Some(handlers) = self.message_handlers.borrow().get(topic) {
            return Ok(Scalar::Number(handlers.borrow().len() as f64));
        }
        let handlers = self.prototype_realm.inherited(topic);
        if !handlers.truthy() {
            return Ok(Scalar::Number(missing as f64));
        }
        match self
            .prototype_realm
            .get(&handlers, "length")
            .map_err(Self::stats_type_error)?
        {
            Native::Number(value) => Ok(Scalar::Number(value)),
            Native::Undefined => Ok(Scalar::Undefined),
            Native::Null => Ok(Scalar::Null),
            Native::String(value) => Ok(Scalar::String(value)),
            Native::Boolean(value) => Ok(Scalar::Boolean(value)),
            _ => Err(ExchangeError::new(
                "statistics handler length is outside the builtin property vocabulary",
            )),
        }
    }
    pub fn cleanup_message_handler_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.message_handlers_stats.borrow_mut().shift_remove(topic);
        self.reconcile_message_handler_stats(self.now(), true)
    }
    fn parse_ws_diagnostic_json(data: &Value) -> Value {
        match data {
            Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
            _ => data.clone(),
        }
    }
    fn should_drop_ws_diagnostic(&self) -> bool {
        let now = self.now();
        let mut s = self.state.borrow_mut();
        if now - s.diagnostic_window >= 1000.0 {
            s.diagnostic_window = now;
            s.diagnostic_frames = 0;
        }
        s.diagnostic_frames += 1;
        if s.diagnostic_frames <= 200 {
            false
        } else {
            s.diagnostic_suppressed += 1;
            true
        }
    }
    fn cap_ws_diagnostic_payload(message: &str) -> Result<String, ExchangeError> {
        let units: Vec<u16> = message.encode_utf16().collect();
        if units.len() <= 4096 {
            Ok(message.into())
        } else {
            let prefix = String::from_utf16(&units[..4096]).map_err(|_| {
                ExchangeError::new("WebSocket diagnostic cap contains an unpaired UTF-16 surrogate")
            })?;
            Ok(format!("{prefix}...truncated ({} chars)", units.len()))
        }
    }
    fn emit_ws_diagnostic_event(&self, event: &str, message: &str) -> Result<(), ExchangeError> {
        let suppressed = {
            let mut s = self.state.borrow_mut();
            let n = s.diagnostic_suppressed;
            s.diagnostic_suppressed = 0;
            n
        };
        if suppressed > 0 {
            self.parent.emit(
                event,
                &format!(
                    "[{}] suppressed {suppressed} websocket diagnostic frame(s) due to rate limit",
                    self.parent.name()
                ),
            )?;
        }
        self.parent
            .emit(event, &Self::cap_ws_diagnostic_payload(message)?)?;
        Ok(())
    }
    pub fn emit_ws_diagnostic_receive(
        &self,
        data: &Value,
        options: &WsDiagnosticReceiveOptions,
    ) -> Result<(), ExchangeError> {
        if !self.runtime.diagnostics_enabled() {
            return Ok(());
        }
        let raw = stringify(data);
        if is_heartbeat_frame(&raw) {
            return Ok(());
        }
        if options
            .allow_channel
            .as_ref()
            .is_some_and(|filter| !filter(&Self::parse_ws_diagnostic_json(data)).unwrap_or(false))
        {
            return Ok(());
        }
        if self.should_drop_ws_diagnostic() {
            return Ok(());
        }
        let formatted = options
            .format
            .as_ref()
            .map_or(Some(raw), |format| format(data));
        let Some(formatted) = formatted.filter(|s| !s.is_empty()) else {
            return Ok(());
        };
        let redacted = redact_payload(&formatted);
        let redacted = options
            .redact
            .as_ref()
            .and_then(|redact| redact(&redacted))
            .unwrap_or(redacted);
        if !redacted.is_empty() {
            self.emit_ws_diagnostic_event("grafana-pws", &redacted)?;
        }
        Ok(())
    }
    pub fn emit_ws_diagnostic_send(
        &self,
        data: &str,
        options: &WsDiagnosticSendOptions,
    ) -> Result<(), ExchangeError> {
        if !self.runtime.diagnostics_enabled()
            || is_heartbeat_frame(data)
            || self.should_drop_ws_diagnostic()
        {
            return Ok(());
        }
        let redacted = redact_payload(data);
        let formatted = options
            .format
            .as_ref()
            .map_or(Some(redacted.clone()), |format| format(&redacted));
        let Some(formatted) = formatted.filter(|s| !s.is_empty()) else {
            return Ok(());
        };
        let result = options
            .redact
            .as_ref()
            .and_then(|redact| redact(&formatted))
            .unwrap_or(formatted);
        if !result.is_empty() {
            self.emit_ws_diagnostic_event("grafana-pws-sub", &result)?;
        }
        Ok(())
    }
    pub fn send_with_log(self: &Rc<Self>, data: &str) -> Result<(), ExchangeError> {
        self.send(data)?;
        self.emit_ws_diagnostic_send(data, &WsDiagnosticSendOptions::default())?;
        Ok(())
    }
    pub fn emit_and_log_error(
        &self,
        error: &str,
        message: Option<&str>,
        extra: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        self.parent.emit_and_log_error(error, message, extra)?;
        Ok(())
    }
    pub(crate) fn disable_connect_and_subscribe(&self) {
        self.connect_disabled.set(true);
    }
    pub fn connect_and_subscribe(self: &Rc<Self>, is_reconnect: bool) -> Result<(), ExchangeError> {
        if self.connect_disabled.get() {
            return Ok(());
        }
        self.protocol
            .connect_and_subscribe(self.clone(), is_reconnect)
    }
    pub async fn on_open(self: &Rc<Self>) -> Result<(), ExchangeError> {
        self.protocol.on_open(self.clone()).await
    }
    pub fn on_message(self: &Rc<Self>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        let index = {
            let mut receipts = self.frame_receipts.borrow_mut();
            let index = receipts.len().checked_sub(1);
            if let Some(index) = index {
                receipts[index].consumed += 1;
            }
            index
        };
        let result = self.protocol.on_message(self, frame);
        if let Some(index) = index {
            let mut receipts = self.frame_receipts.borrow_mut();
            receipts[index].processed += 1;
            receipts[index].errors += u64::from(result.is_err());
        }
        result
    }
    pub fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        self.parent.emit_ws_reconnected()?;
        Ok(())
    }
    pub fn log_last_log(&self, message: &str) {
        let mut log = self.last_log.borrow_mut();
        if log.len() == 20 {
            log.pop_front();
        }
        log.push_back(message.into());
    }
    fn reserve_socket_generation(&self) -> u64 {
        let mut state = self.state.borrow_mut();
        state.generation += 1;
        state.reconnect_listener_generation =
            state.register_reconnect_open.then_some(state.generation);
        state.provisional_reconnect_open = None;
        state.generation
    }
    /// Own an asynchronous message-handler tail without starting a connection.
    /// Run the source synchronous prefix in the callback first. Capture cleanup
    /// guards before constructing this future, including for unpolled disposal.
    /// Returns immediately; subsequent messages do not await this operation.
    pub fn start_message_operation<F>(self: &Rc<Self>, operation: F) -> OwnedOperation
    where
        F: Future<Output = Result<(), ExchangeError>> + 'static,
    {
        if self.is_disposed() {
            return OwnedOperation::inactive();
        }
        let weak = Rc::downgrade(self);
        self.spawn_owned(Box::pin(async move {
            if weak.upgrade().is_none_or(|socket| socket.is_disposed()) {
                return;
            }
            if let Err(error) = operation.await
                && let Some(socket) = weak.upgrade()
                && !socket.is_disposed()
            {
                // CHOSEN host projection of an unhandled promise rejection.
                // This is not a transport event and must not consume error_log.
                socket.runtime.report_exception(&error, &Value::Null.into());
            }
        }))
    }
    pub fn start_interval(
        self: &Rc<Self>,
        period_ms: u64,
        callback: impl FnMut() -> Result<(), ExchangeError> + 'static,
    ) -> Result<OwnedOperation, ExchangeError> {
        self.owned_tasks
            .timer(self.runtime.clone(), period_ms, true, callback)
    }
    pub fn start_timeout(
        self: &Rc<Self>,
        delay_ms: u64,
        callback: impl FnMut() -> Result<(), ExchangeError> + 'static,
    ) -> Result<OwnedOperation, ExchangeError> {
        self.owned_tasks
            .timer(self.runtime.clone(), delay_ms, false, callback)
    }
    /// Start a venue dial in the core-owned cancellation registry.
    pub fn start_websocket(
        self: &Rc<Self>,
        url: String,
        protocols: Vec<String>,
        options: Option<WebSocketConnectionOptions>,
    ) {
        self.start_prepared_websocket(move || async move {
            Ok(Some(PreparedWebSocket {
                url,
                protocols,
                options,
            }))
        });
    }
    /// Own preparation (for example a REST bullet-token fetch) and the dial as
    /// one cancellable task. None means the venue has handled a no-dial outcome.
    /// Preparation starts eagerly through the shared Promise boundary; the
    /// socket registry still cancels the actual preparation/dial body.
    pub fn start_prepared_websocket<F, P>(self: &Rc<Self>, prepare: P)
    where
        F: Future<Output = Result<Option<PreparedWebSocket>, ExchangeError>> + 'static,
        P: FnOnce() -> F + 'static,
    {
        if self.is_disposed() {
            return;
        }
        let generation = self.reserve_socket_generation();
        let weak = Rc::downgrade(self);
        self.spawn_owned_kind(
            Box::pin(async move {
                let Some(socket) = weak.upgrade() else {
                    return;
                };
                if socket.is_disposed() || socket.state.borrow().generation != generation {
                    return;
                }
                let result = async {
                    let Some(prepared) = prepare().await? else {
                        return Ok(());
                    };
                    if socket.is_disposed() || socket.state.borrow().generation != generation {
                        return Ok(());
                    }
                    socket
                        .create_websocket_for_generation(
                            prepared.url,
                            prepared.protocols,
                            prepared.options,
                            Some(generation),
                        )
                        .await
                }
                .await;
                if let Err(error) = result
                    && !socket.is_disposed()
                    && socket.state.borrow().generation == generation
                {
                    socket
                        .report_callback_result(socket.handle_web_socket_error(&error.to_string()));
                }
            }),
            false,
            true,
            RuntimeTaskKind::Finite,
        );
    }
    pub async fn create_websocket(
        self: &Rc<Self>,
        url: String,
        protocols: Vec<String>,
        options: Option<WebSocketConnectionOptions>,
    ) -> Result<(), ExchangeError> {
        self.create_websocket_for_generation(url, protocols, options, None)
            .await
    }
    async fn create_websocket_for_generation(
        self: &Rc<Self>,
        url: String,
        protocols: Vec<String>,
        options: Option<WebSocketConnectionOptions>,
        reserved_generation: Option<u64>,
    ) -> Result<(), ExchangeError> {
        let mut options = options.unwrap_or_else(|| WebSocketConnectionOptions {
            min_reconnection_delay_ms: Some(self.runtime.min_reconnection_delay_ms()),
            max_reconnection_delay_ms: Some(5_000.0),
            ..Default::default()
        });
        let capable = get_node_web_socket(self.runtime.as_ref());
        if options.headers_required && !capable {
            let error = HeaderCapableWebSocketUnavailableError::new();
            let mut error = ExchangeError::new(error.to_string());
            error.name = "HeaderCapableWebSocketUnavailableError".into();
            return Err(error);
        }
        let connection = self.runtime.transport().ws_connection(&url);
        options.protocols = protocols.clone();
        if let Some(WsConnection::Relay { headers } | WsConnection::Headers { headers }) =
            &connection
        {
            let extra = headers.clone();
            options.headers = Some(match options.headers.take() {
                Some(UpgradeHeaders::Fresh(provider)) => {
                    UpgradeHeaders::Fresh(Rc::new(move || {
                        let mut current = provider();
                        current.extend(extra.clone());
                        current
                    }))
                }
                Some(UpgradeHeaders::Static(mut headers)) => {
                    headers.extend(extra);
                    UpgradeHeaders::Static(headers)
                }
                None => UpgradeHeaders::Static(extra.into_iter().collect()),
            });
        }
        let generation = reserved_generation.unwrap_or_else(|| self.reserve_socket_generation());
        let relay = matches!(connection, Some(WsConnection::Relay { .. }));
        let reconnecting = relay
            || matches!(connection, Some(WsConnection::Headers { .. }))
            || (capable && options.headers.is_some())
            || self.runtime.enable_reconnecting();
        let resolved = ResolvedConnection {
            descriptor: RelayDescriptor { url, options },
            relay,
            reconnecting,
        };
        if reconnecting && resolved.descriptor.options.start_closed == Some(true) {
            self.stop_managed_relay();
            let mut state = self.state.borrow_mut();
            state.native = Some(Rc::new(DormantSocket {
                owner: Rc::downgrade(self),
                connection: resolved,
                operation: Rc::default(),
                generation: Cell::new(generation),
            }));
            state.relay_descriptor = None;
            return Ok(());
        }
        self.open_resolved_connection(resolved, generation).await
    }
    async fn open_resolved_connection(
        self: &Rc<Self>,
        connection: ResolvedConnection,
        generation: u64,
    ) -> Result<(), ExchangeError> {
        let ResolvedConnection {
            mut descriptor,
            relay,
            reconnecting,
        } = connection;
        if self.runtime.frame_receipts_enabled() {
            self.frame_receipts.borrow_mut().push(FrameDrainReceipt {
                url: descriptor.url.clone(),
                generation,
                ..FrameDrainReceipt::default()
            });
        }
        self.grouped_listener_dispatch.set(reconnecting);
        // The core has already activated a dormant wrapper; do not nest another one.
        if reconnecting && descriptor.options.start_closed == Some(true) {
            descriptor.options.start_closed = Some(false);
        }
        if relay {
            self.start_managed_relay(descriptor, 0.0)?;
        } else {
            let socket = self
                .runtime
                .dial(
                    descriptor.url,
                    descriptor.options.protocols.clone(),
                    descriptor.options,
                    reconnecting,
                )
                .await?;
            if self.is_disposed() || self.state.borrow().generation != generation {
                socket.terminate();
                socket.close();
                return Ok(());
            }
            {
                let mut state = self.state.borrow_mut();
                state.native = Some(socket.clone());
                state.relay = None;
                state.relay_descriptor = None;
                state.relay_open = false;
            }
            let weak = Rc::downgrade(self);
            self.spawn_owned_kind(
                Box::pin(async move {
                    loop {
                        let event = socket.next_event().await;
                        let Some(owner) = weak.upgrade() else {
                            socket.terminate();
                            socket.close();
                            break;
                        };
                        if owner.is_disposed() || owner.state.borrow().generation != generation {
                            socket.terminate();
                            socket.close();
                            break;
                        }
                        match event {
                            Ok(WsEvent::Open) => {
                                owner.dispatch_open(generation).await;
                            }
                            Ok(WsEvent::Message(frame)) => {
                                owner.direct_fallback.borrow_mut().on_message();
                                if let Err(error) = owner.on_message(&frame) {
                                    owner.report_callback_result(
                                        owner.handle_web_socket_error(&error.to_string()),
                                    );
                                }
                            }
                            Ok(WsEvent::Error(error)) => {
                                owner.report_callback_result(owner.handle_web_socket_error(&error))
                            }
                            Ok(WsEvent::Close { code, reason }) => {
                                owner.report_callback_result(
                                    owner.received_transport_close(code, &reason),
                                );
                                if !socket.is_reconnecting() {
                                    break;
                                }
                            }
                            Err(error) => {
                                owner.report_callback_result(
                                    owner
                                        .handle_web_socket_error(&error.to_string())
                                        .and_then(|()| owner.transport_close(Some(1006), "")),
                                );
                                break;
                            }
                        }
                    }
                }),
                false,
                false,
                RuntimeTaskKind::SocketReader,
            );
        }
        Ok(())
    }
    fn stop_managed_relay(&self) {
        let (task, relay) = {
            let mut state = self.state.borrow_mut();
            state.relay_open = false;
            state.relay_task = None;
            let task = state.relay_operation.take();
            (task, state.relay.take())
        };
        if let Some(task) = task {
            task.cancel();
        }
        if let Some(relay) = relay {
            relay.close(1000, "");
        }
    }
    fn start_managed_relay(
        self: &Rc<Self>,
        descriptor: RelayDescriptor,
        initial_delay: f64,
    ) -> Result<(), ExchangeError> {
        // Keep construction/preflight errors synchronous with create_websocket.
        let initial = Rc::new(
            RelayWebSocket::with_connection_options(
                self.runtime.tunnel_host(),
                &descriptor.url,
                descriptor.options.clone(),
            )
            .map_err(|error| {
                let mut mapped = ExchangeError::new(error.to_string());
                if matches!(
                    error,
                    safe_cex_transport::TransportError::UnavailableTransport(_)
                ) && mapped.message == HeaderCapableWebSocketUnavailableError::new().to_string()
                {
                    mapped.name = "HeaderCapableWebSocketUnavailableError".into();
                }
                mapped
            })?,
        );
        self.stop_managed_relay();
        let generation = self.state.borrow().generation;
        {
            let mut state = self.state.borrow_mut();
            state.relay_descriptor = Some(descriptor.clone());
            state.native = None;
        }
        let weak = Rc::downgrade(self);
        let runtime = self.runtime.clone();
        let library_minimum = runtime.library_min_reconnection_delay_ms();
        let uptime = descriptor.options.min_uptime_ms.unwrap_or(5_000.0);
        let timeout = descriptor.options.connection_timeout_ms.unwrap_or(4_000.0);
        let max_retries = descriptor.options.max_retries.unwrap_or(f64::INFINITY);
        self.spawn_owned_kind(
            Box::pin(async move {
                let mut initial = Some(initial);
                let mut delay = initial_delay;
                let mut failures = 0_i32;
                loop {
                    if relay_retry_exhausted(failures, max_retries) {
                        if let Some(socket) = weak.upgrade() {
                            socket.state.borrow_mut().relay_open = false;
                            socket.report_callback_result(
                                socket.on_close(Some(4000), "Max retries exhausted"),
                            );
                        }
                        return;
                    }
                    if delay > 0.0
                        && let Err(error) = runtime.sleep(delay as u64).await
                    {
                        if let Some(socket) = weak.upgrade() {
                            socket.report_callback_result(
                                socket.handle_web_socket_error(&error.to_string()),
                            );
                        }
                        return;
                    }
                    let Some(socket) = weak.upgrade() else { return };
                    if socket.is_disposed() || socket.state.borrow().generation != generation {
                        return;
                    }
                    let relay = if delay == 0.0
                        && let Some(relay) = initial.take()
                    {
                        relay
                    } else {
                        initial.take();
                        match RelayWebSocket::with_connection_options(
                            runtime.tunnel_host(),
                            &descriptor.url,
                            descriptor.options.clone(),
                        ) {
                            Ok(relay) => Rc::new(relay),
                            Err(error) => {
                                socket.report_callback_result(
                                    socket.handle_web_socket_error(&error.to_string()),
                                );
                                return;
                            }
                        }
                    };
                    socket.state.borrow_mut().relay = Some(relay.clone());
                    let deadline = socket.now() + timeout;
                    drop(socket);
                    let mut opened_at = None;
                    loop {
                        let mut event = std::pin::pin!(relay.next_event());
                        let event = if opened_at.is_none() {
                            let remaining = (deadline - runtime.now_ms()).max(0.0);
                            let mut timeout = runtime.sleep(remaining.ceil() as u64);
                            let timed = std::future::poll_fn(|cx| {
                                if let std::task::Poll::Ready(event) = event.as_mut().poll(cx) {
                                    return std::task::Poll::Ready(Ok(Some(event)));
                                }
                                match timeout.as_mut().poll(cx) {
                                    std::task::Poll::Ready(result) => {
                                        std::task::Poll::Ready(result.map(|()| None))
                                    }
                                    std::task::Poll::Pending => std::task::Poll::Pending,
                                }
                            })
                            .await;
                            match timed {
                                Ok(event) => event,
                                Err(error) => {
                                    if let Some(socket) = weak.upgrade() {
                                        socket.report_callback_result(
                                            socket.handle_web_socket_error(&error.to_string()),
                                        );
                                    }
                                    return;
                                }
                            }
                        } else {
                            Some(event.await)
                        };
                        let Some(socket) = weak.upgrade() else { return };
                        if socket.is_disposed() || socket.state.borrow().generation != generation {
                            return;
                        }
                        match event {
                            Some(Ok(RelayEvent::Open)) => {
                                opened_at = Some(socket.now());
                                socket.state.borrow_mut().relay_open = true;
                                socket.dispatch_open(generation).await;
                            }
                            Some(Ok(RelayEvent::Message(frame))) => {
                                socket.direct_fallback.borrow_mut().on_message();
                                if let Err(error) = socket.on_message(&frame) {
                                    socket.report_callback_result(
                                        socket.handle_web_socket_error(&error.to_string()),
                                    );
                                }
                            }
                            Some(Ok(RelayEvent::Close { code, reason, .. })) => {
                                socket.state.borrow_mut().relay_open = false;
                                socket.report_callback_result(
                                    socket.received_transport_close(Some(code), &reason),
                                );
                                break;
                            }
                            other => {
                                let (message, reason) = match other {
                                    None => ("TIMEOUT".to_owned(), "timeout"),
                                    Some(Ok(RelayEvent::Error(message))) => (message, ""),
                                    Some(Err(error)) => (error.to_string(), ""),
                                    _ => unreachable!(),
                                };
                                relay.close(1000, reason);
                                socket.state.borrow_mut().relay_open = false;
                                socket.report_callback_result(
                                    socket
                                        .transport_close(Some(1000), reason)
                                        .and_then(|()| socket.handle_web_socket_error(&message)),
                                );
                                break;
                            }
                        }
                    }
                    if opened_at.is_some_and(|at| runtime.now_ms() - at >= uptime) {
                        failures = 0;
                    }
                    delay =
                        relay_reconnection_delay(&descriptor.options, library_minimum, failures);
                    failures = failures.saturating_add(1);
                }
            }),
            true,
            false,
            RuntimeTaskKind::Finite,
        );
        Ok(())
    }
    pub fn track_connected(self: &Rc<Self>) {
        if self.is_disposed() {
            return;
        }
        let reopened = {
            let mut s = self.state.borrow_mut();
            s.last_connected_at = self.now();
            let reopened = s.opened_before;
            s.opened_before = true;
            reopened
        };
        if reopened {
            self.resync_private_state_after_reopen();
        }
    }
    pub fn resyncs_private_state_on_reopen(&self) -> bool {
        self.protocol
            .resyncs_private_state_on_reopen()
            .unwrap_or_else(|| self.parent.is_private_socket(self))
    }
    fn resync_private_state_after_reopen(self: &Rc<Self>) {
        let options = self.parent.options();
        if self.is_force_closed()
            || options.public_data_only.unwrap_or(false)
            || options.key.is_empty()
            || !self.resyncs_private_state_on_reopen()
        {
            return;
        }
        self.parent.resync_coordinator().request(self);
    }
    pub fn handle_web_socket_error(&self, message: &str) -> Result<(), ExchangeError> {
        if !self.is_disposed()
            && let Some(logged) = self.get_web_socket_error_log_message(message)
        {
            self.parent
                .log(&format!("WebSocket error: {logged}"), LogSeverity::Error)?;
        }
        // createWebsocket registers the stopping base listener before venue listeners.
        if self.grouped_listener_dispatch() {
            self.protocol.on_error(self, message)
        } else {
            Ok(())
        }
    }
    pub fn send(self: &Rc<Self>, message: &str) -> Result<(), ExchangeError> {
        if self.is_disposed() {
            return Ok(());
        }
        let (native, relay, open) = {
            let s = self.state.borrow();
            (s.native.clone(), s.relay.clone(), s.relay_open)
        };
        let ready = if relay.is_some() {
            if open { Some(1) } else { Some(0) }
        } else {
            native.as_ref().map(|s| s.ready_state())
        };
        if ready != Some(1) {
            let code = ready.map_or("undefined".into(), |n| n.to_string());
            let name = ready
                .and_then(|n| {
                    ["CONNECTING", "OPEN", "CLOSING", "CLOSED"]
                        .get(n as usize)
                        .copied()
                })
                .unwrap_or("undefined");
            if is_heartbeat_frame(message) {
                self.parent.log(
                    &format!("Skipping ping/pong - WebSocket state: {name} ({code})"),
                    LogSeverity::Debug,
                )?;
                return Ok(());
            }
            self.parent.log(&format!("WebSocket not ready for send (state: {name}, stateCode: {code}, messageType: string, messageLength: {}). Triggering reconnection.",message.encode_utf16().count()),LogSeverity::Warning)?;
            if !self.state.borrow().reconnecting {
                // TS calls async reconnect() without awaiting it: execute its
                // prefix now, but its rejected promise cannot throw from send.
                if let Err(error) = self.reconnect() {
                    self.report_detached_reconnect_error(error);
                }
            }
            return Ok(());
        }
        if let Some(relay) = relay {
            relay
                .send(SocketFrame::Text(message.into()))
                .map_err(|e| ExchangeError::new(e.to_string()))
        } else {
            native.expect("ready socket").send(message)
        }
    }
    fn report_detached_reconnect_error(&self, error: ExchangeError) {
        let reporter = self.runtime.clone();
        crate::owned_tasks::OwnedTasks::start_detached(
            self.runtime.as_ref(),
            Box::pin(async move {
                // A host may poll the task inline. Yield its first poll so the
                // local send/open stack completes before rejection reporting.
                let mut yielded = false;
                std::future::poll_fn(move |cx| {
                    if yielded {
                        std::task::Poll::Ready(())
                    } else {
                        yielded = true;
                        cx.waker().wake_by_ref();
                        std::task::Poll::Pending
                    }
                })
                .await;
                reporter.report_exception(
                    &error,
                    &serde_json::json!({"operation":"websocket unawaited reconnect"}).into(),
                );
            }),
        );
    }
    pub fn is_connected(&self) -> bool {
        let (relay_open, native) = {
            let state = self.state.borrow();
            (state.relay_open, state.native.clone())
        };
        relay_open || native.is_some_and(|socket| socket.ready_state() == 1)
    }
    /// Source `this.ws` truthiness, independent of its OPEN ready state.
    pub fn has_websocket(&self) -> bool {
        let state = self.state.borrow();
        state.native.is_some() || state.relay.is_some()
    }
    pub fn is_healthy(&self) -> bool {
        if !self.is_connected() {
            return false;
        }
        let received = self.state.borrow().last_received;
        received != 0.0
            && self.now() - received
                < self
                    .get_watchdog_stale_threshold_ms()
                    .unwrap_or(WS_RECONNECT_THRESHOLD_MS)
    }
    /// Raw source `lastReceived` field, including zero and non-finite values.
    pub fn last_received(&self) -> f64 {
        self.state.borrow().last_received
    }
    /// Assign only source `lastReceived`; callers own health synchronization.
    pub fn set_last_received(&self, value: f64) {
        self.state.borrow_mut().last_received = value;
    }
    pub fn last_message_timestamp(&self) -> Option<f64> {
        let received = self.state.borrow().last_received;
        (received > 0.0).then_some(received)
    }
    /// Instance-level close wrapper, shared by direct and managed-relay delivery.
    /// Clone before invocation so cleanup may reenter or replace the callback.
    pub fn set_before_close(&self, callback: Rc<dyn Fn()>) {
        *self.before_close.borrow_mut() = Some(callback);
    }
    // ts: reconnectNow. Clearing its timer reference does not cancel the pending
    // owned operation when called directly; timer firing removes it beforehand.
    pub fn reconnect_now(self: &Rc<Self>) -> Result<(), ExchangeError> {
        {
            let mut state = self.state.borrow_mut();
            state.timer_ids.remove(&Timer::Reconnect);
            state.tasks.remove(&Timer::Reconnect);
        }
        if self.parent.is_disposed() {
            lifecycle_log(
                self.runtime.as_ref(),
                &format!(
                    "[BaseWebSocket] ⚠️ {} disposed during reconnect delay, aborting",
                    self.parent.name()
                ),
            );
            return Ok(());
        }
        let options = self.parent.options();
        lifecycle_log(
            self.runtime.as_ref(),
            &format!(
                "[BaseWebSocket] 🔍 {} options check: publicDataOnly={}, hasKey={}, key={}, forceClosed={}",
                self.parent.name(),
                options
                    .public_data_only
                    .map_or("undefined", |v| if v { "true" } else { "false" }),
                !options.key.is_empty(),
                if options.key.is_empty() {
                    "empty"
                } else {
                    "[REDACTED]"
                },
                self.is_force_closed()
            ),
        );
        let options = self.parent.options();
        let public = options.public_data_only == Some(true) || options.key.is_empty();
        lifecycle_log(
            self.runtime.as_ref(),
            &if public {
                format!(
                    "[BaseWebSocket] 🌍 {} taking PUBLIC path (publicDataOnly={}, key empty={})",
                    self.parent.name(),
                    options.public_data_only.map_or("undefined", |v| if v {
                        "true"
                    } else {
                        "false"
                    }),
                    options.key.is_empty()
                )
            } else {
                format!(
                    "[BaseWebSocket] 🔐 {} taking PRIVATE path",
                    self.parent.name()
                )
            },
        );
        // Native dispatch can deliver Open inline; preserve the existing event
        // bridge's provisional notification before completing the reconnect.
        self.state.borrow_mut().register_reconnect_open = true;
        if !self.is_force_closed() {
            lifecycle_log(
                self.runtime.as_ref(),
                &format!(
                    "[BaseWebSocket] 📡 {} calling connectAndSubscribe() for {} WebSocket",
                    self.parent.name(),
                    if public { "public" } else { "private" }
                ),
            );
            if let Err(error) = self.connect_and_subscribe(false) {
                let mut s = self.state.borrow_mut();
                s.register_reconnect_open = false;
                s.reconnect_listener_generation = None;
                s.provisional_reconnect_open = None;
                return Err(error);
            }
        } else {
            lifecycle_log(
                self.runtime.as_ref(),
                &format!(
                    "[BaseWebSocket] ⚠️ {} forceClosed=true, skipping connectAndSubscribe()",
                    self.parent.name()
                ),
            );
        }
        {
            let mut s = self.state.borrow_mut();
            s.reconnect_listener_generation = Some(s.generation);
            s.register_reconnect_open = false;
        }
        lifecycle_log(
            self.runtime.as_ref(),
            &format!(
                "[BaseWebSocket] ✅ {} reconnection complete, setting reconnecting=false",
                self.parent.name()
            ),
        );
        self.state.borrow_mut().reconnecting = false;
        let now = self.now();
        self.state.borrow_mut().last_close_time = now;
        let emit = {
            let mut s = self.state.borrow_mut();
            s.provisional_reconnect_open.take() == Some(s.generation)
        };
        if emit && !self.is_disposed() {
            self.emit_ws_reconnected()?;
        }
        Ok(())
    }
    // ts: retryFailedConnect. Only the first error in the streak is announced.
    pub fn retry_failed_connect(self: &Rc<Self>, message: &str) -> Result<(), ExchangeError> {
        if self.parent.is_disposed() || self.is_force_closed() {
            return Ok(());
        }
        let announced = self.state.borrow().connect_failure_announced;
        if announced {
            self.parent.log(message, LogSeverity::Warning)?;
        } else {
            self.parent.emit_and_log_error(message, None, None)?;
        }
        self.state.borrow_mut().connect_failure_announced = true;
        self.on_close(Some(1006), "")
    }
    // Explicit policy substitutes the source subclass's protected readonly flag.
    pub fn recover_after_offline(self: &Rc<Self>, recovers: bool) -> Result<bool, ExchangeError> {
        if self.parent.is_disposed() || self.is_force_closed() {
            return Ok(true);
        }
        if !recovers || !self.protocol.recovers_after_offline() {
            return Ok(false);
        }
        let native = self.state.borrow().native.clone();
        if let Some(socket) = native.filter(|s| s.is_reconnecting()) {
            if socket.ready_state() == 1 {
                socket.reconnect();
            }
            return Ok(true);
        }
        self.reset_failure_streak();
        self.reset_reconnect_backoff();
        {
            let mut s = self.state.borrow_mut();
            s.on_close_count = 0;
            s.reconnect_count = 0;
        }
        let has_timer = self
            .state
            .borrow()
            .timer_ids
            .contains_key(&Timer::Reconnect);
        if has_timer {
            self.cancel(Timer::Reconnect);
            self.reconnect_now()?;
        } else {
            let native = self.state.borrow().native.clone();
            let relay = self.state.borrow().relay.clone();
            if let Some(socket) = native.filter(|s| s.ready_state() != 0) {
                socket.close();
            } else if let Some(relay) = relay.filter(|s| s.ready_state() != 0) {
                relay.close(None::<u16>, "");
            } else {
                let reconnecting = self.state.borrow().reconnecting;
                if reconnecting {
                    self.reconnect_now()?;
                }
            }
        }
        Ok(true)
    }
    fn transport_close(
        self: &Rc<Self>,
        code: Option<u16>,
        reason: &str,
    ) -> Result<(), ExchangeError> {
        if !self.protocol.owns_close_handling() {
            self.on_close(code, reason)?;
        }
        self.protocol.on_transport_close(self, code, reason)
    }
    pub fn on_close(self: &Rc<Self>, code: Option<u16>, reason: &str) -> Result<(), ExchangeError> {
        let before_close = self.before_close.borrow().clone();
        if let Some(before_close) = before_close {
            before_close();
        }
        let description = code.map_or("Unknown".into(), get_close_code_description);
        self.process_close(code, reason, &description)?;
        Ok(())
    }
    fn reset_reconnect_backoff(&self) {
        let mut s = self.state.borrow_mut();
        s.backoff_attempts = 0;
        s.backoff_code = None;
        s.backoff_last_failure_at = 0.0;
    }
    fn reset_failure_streak(&self) {
        let mut s = self.state.borrow_mut();
        s.connect_failure_announced = false;
        s.close_failures = 0;
        s.pause_until = 0.0;
        s.pause_announced = false;
    }
    fn register_close_result(&self, code: Option<u16>) {
        if code == Some(1000) {
            self.reset_failure_streak();
            return;
        }
        if code.is_none() {
            return;
        }
        let mut s = self.state.borrow_mut();
        s.close_failures += 1;
        if s.close_failures >= 8 {
            s.pause_until = self.now() + 120_000.0;
        }
    }
    fn get_reconnect_pause_remaining_ms(&self) -> f64 {
        (self.state.borrow().pause_until - self.now()).max(0.0)
    }
    pub fn reconnect_pause_remaining_ms(&self) -> f64 {
        self.get_reconnect_pause_remaining_ms()
    }
    pub fn is_reconnect_pause_active(&self) -> bool {
        self.get_reconnect_pause_remaining_ms() > 0.0
    }
    fn get_reconnect_delay(&self, code: Option<u16>) -> f64 {
        if code.is_none() || code == Some(1000) {
            self.reset_reconnect_backoff();
            return self.runtime.min_reconnection_delay_ms();
        }
        let now = self.now();
        let mut s = self.state.borrow_mut();
        if s.backoff_attempts == 0
            || now - s.backoff_last_failure_at > 60_000.0
            || s.backoff_code != code
        {
            s.backoff_attempts = 1;
        } else {
            s.backoff_attempts += 1;
        }
        s.backoff_code = code;
        s.backoff_last_failure_at = now;
        (2000.0 * 2.0_f64.powf(s.backoff_attempts.saturating_sub(1) as f64))
            .min(30_000.0)
            .max(self.runtime.min_reconnection_delay_ms())
    }
    fn get_close_log_message(
        &self,
        code: Option<u16>,
        reason: &str,
        description: &str,
    ) -> Option<String> {
        let code = code.filter(|code| *code != 1000)?;
        if self.is_disposed() {
            return None;
        }
        let base = if reason.is_empty() {
            format!("WebSocket closed: {description} (code {code})")
        } else {
            format!("WebSocket closed: {description} (code {code}) - {reason}")
        };
        self.state.borrow_mut().close_log.message(
            format!("{code}:{description}"),
            self.now(),
            WS_CLOSE_LOG_COOLDOWN_MS,
            &base,
            "close events",
        )
    }
    fn get_web_socket_error_log_message(&self, message: &str) -> Option<String> {
        let message = if message.is_empty() {
            "Unknown error"
        } else {
            message
        };
        self.state.borrow_mut().error_log.message(
            message.to_lowercase(),
            self.now(),
            WS_ERROR_LOG_COOLDOWN_MS,
            message,
            "ws error events",
        )
    }
    pub fn record_close_diagnostics(&self, code: Option<u16>, synthetic_if_no_socket: bool) {
        let mut s = self.state.borrow_mut();
        if synthetic_if_no_socket && s.native.is_none() && s.relay.is_none() {
            return;
        }
        s.lifetime_close_count += 1;
        if let Some(code) = code {
            s.last_close_code = Some(code);
            s.last_close_at = self.now();
        }
    }
    fn process_close(
        self: &Rc<Self>,
        code: Option<u16>,
        reason: &str,
        description: &str,
    ) -> Result<(), ExchangeError> {
        self.state.borrow_mut().last_received = 0.0;
        self.cancel(Timer::Receive);
        self.record_close_diagnostics(code, true);
        self.register_close_result(code);
        self.parent.sync_socket_health_to_store()?;
        if let Some(error) = self.get_close_log_message(code, reason, description) {
            self.runtime
                .console("error", &format!("[{}] {error}", self.parent.name()));
            self.parent.emit("grafana-error", &error)?;
        }
        if !self.is_disposed() {
            self.parent.log(
                &format!(
                    "WebSocket connection disconnected (code: {}, reason: {}), reconnecting...",
                    code.map_or("unknown".into(), |c| c.to_string()),
                    if reason.is_empty() { "none" } else { reason }
                ),
                LogSeverity::Warning,
            )?;
        }
        let native = self.state.borrow().native.clone();
        let managed = self.state.borrow().relay_descriptor.is_some()
            || native
                .as_ref()
                .is_some_and(|socket| socket.is_reconnecting());
        // The circuit breaker applies only when core owns native retry;
        // relay and host reconnect wrappers keep their own unlimited lifecycle.
        if managed {
            self.cancel(Timer::Ping);
            self.cancel(Timer::PongMiss);
            return Ok(());
        }
        {
            let mut s = self.state.borrow_mut();
            // ts-bug: processClose returns before ping/pong cleanup on a duplicate native close.
            if s.reconnecting {
                drop(s);
                self.parent.log(
                    "WebSocket connection already reconnecting, skipping...",
                    LogSeverity::Info,
                )?;
                return Ok(());
            }
            s.reconnecting = true;
        }
        self.cancel(Timer::Ping);
        self.cancel(Timer::PongMiss);
        {
            let mut s = self.state.borrow_mut();
            s.native = None;
            s.relay = None;
            s.relay_open = false;
            s.on_close_count += 1;
            if s.on_close_count > 100 {
                drop(s);
                self.parent
                    .log("onClose count > 100...", LogSeverity::Error)?;
                return Ok(());
            }
        }
        if !self.is_disposed() {
            let delay = self
                .get_reconnect_delay(code)
                .max(self.get_reconnect_pause_remaining_ms())
                .max(
                    self.runtime.min_reconnection_delay_ms()
                        - (self.now() - self.state.borrow().last_close_time),
                )
                .max(0.0);
            {
                let mut s = self.state.borrow_mut();
                if s.pause_until > self.now() && !s.pause_announced {
                    s.pause_announced = true;
                    let failures = s.close_failures;
                    let seconds = ((s.pause_until - self.now()) / 1000.0).ceil();
                    drop(s);
                    self.parent.log(&format!("Reconnection pause active after {failures} consecutive failures; retrying in {seconds}s"),LogSeverity::Warning)?;
                }
            }
            self.parent
                .log(&format!("Reconnecting in {delay}ms"), LogSeverity::Info)?;
            self.schedule(Timer::Reconnect, delay);
        }
        Ok(())
    }
    pub fn dispose(&self) {
        self.protocol.dispose();
        let (native, relay, tasks) = {
            let mut s = self.state.borrow_mut();
            s.force_closed = true;
            s.generation += 1;
            s.relay_open = false;
            s.timer_ids.clear();
            (
                s.native.take(),
                s.relay.take(),
                std::mem::take(&mut s.tasks),
            )
        };
        self.owned_tasks.close();
        for task in tasks.values() {
            task.cancel();
        }
        if let Some(ws) = native {
            ws.terminate();
            ws.close();
        }
        if let Some(ws) = relay {
            ws.close(1000, "");
        }
    }
    pub fn reconnect(self: &Rc<Self>) -> Result<(), ExchangeError> {
        self.protocol.reconnect(self)
    }
    pub(crate) fn reconnect_base(self: &Rc<Self>) -> Result<(), ExchangeError> {
        self.parent.log(
            "Explicit reconnect call from ping pong miss or online status change",
            LogSeverity::Info,
        )?;
        let (native, relay) = {
            let mut s = self.state.borrow_mut();
            if s.native.is_some() || s.relay_descriptor.is_some() {
                s.reconnect_count += 1;
                if s.reconnect_count > 100 {
                    drop(s);
                    self.parent
                        .log("Reconnect count > 100...", LogSeverity::Error)?;
                    return Ok(());
                }
            }
            (s.native.clone(), s.relay.clone())
        };
        if let Some(ws) = native {
            if ws.is_reconnecting() {
                let weak = Rc::downgrade(self);
                let delay = self
                    .runtime
                    .sleep(self.runtime.min_reconnection_delay_ms().ceil() as u64);
                self.spawn_owned(Box::pin(async move {
                    if delay.await.is_ok()
                        && let Some(socket) = weak.upgrade()
                        && !socket.is_disposed()
                    {
                        let native = socket.state.borrow().native.clone();
                        if let Some(ws) = native {
                            ws.reconnect();
                        }
                    }
                }));
            } else {
                ws.close();
            }
        } else if relay.is_some() || self.state.borrow().relay_descriptor.is_some() {
            let weak = Rc::downgrade(self);
            let delay = self
                .runtime
                .sleep(self.runtime.min_reconnection_delay_ms().ceil() as u64);
            self.spawn_owned(Box::pin(async move {
                if delay.await.is_ok()
                    && let Some(socket) = weak.upgrade()
                    && !socket.is_disposed()
                {
                    let descriptor = socket.state.borrow().relay_descriptor.clone();
                    socket.reserve_socket_generation();
                    if let Some(descriptor) = descriptor
                        && let Err(error) = socket.start_managed_relay(descriptor, 0.0)
                    {
                        socket.report_callback_result(
                            socket.handle_web_socket_error(&error.to_string()),
                        );
                    }
                }
            }));
        } else {
            self.on_close(None, "")?;
        }
        Ok(())
    }
    /// Source `if (this.ws) this.ws.close()`: close the current transport only.
    /// Keep superclass timers, listeners, generation and retained handles intact.
    pub fn close_current_websocket(&self) {
        let (native, relay) = {
            let state = self.state.borrow();
            (state.native.clone(), state.relay.clone())
        };
        if let Some(socket) = native {
            socket.close();
        }
        if let Some(socket) = relay {
            socket.close(1000, "");
        }
    }
    pub fn is_force_closed(&self) -> bool {
        self.state.borrow().force_closed
    }
    pub fn force_close(&self) {
        let (native, relay) = {
            let mut s = self.state.borrow_mut();
            s.force_closed = true;
            s.relay_open = false;
            if s.relay_descriptor.is_some() {
                s.generation += 1;
            }
            (s.native.clone(), s.relay.clone())
        };
        if let Some(ws) = native {
            ws.close();
        }
        if relay.is_some() || self.state.borrow().relay_descriptor.is_some() {
            self.stop_managed_relay();
        }
    }
    pub fn force_open(self: &Rc<Self>) -> Result<(), ExchangeError> {
        self.state.borrow_mut().force_closed = false;
        self.reset_reconnect_backoff();
        self.reset_failure_streak();
        let descriptor = self.state.borrow().relay_descriptor.clone();
        if let Some(descriptor) = descriptor {
            self.reserve_socket_generation();
            return self.start_managed_relay(descriptor, 0.0);
        }
        let native = self.state.borrow().native.clone();
        if let Some(ws) = native.filter(|ws| ws.is_reconnecting()) {
            ws.reconnect();
            Ok(())
        } else {
            self.connect_and_subscribe(false)
        }
    }
    pub fn track_last_received(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let was_zero = {
            let mut s = self.state.borrow_mut();
            let zero = s.last_received == 0.0;
            s.last_received = self.now();
            zero
        };
        let reset = {
            let s = self.state.borrow();
            (
                s.backoff_attempts != 0 || s.backoff_last_failure_at != 0.0,
                s.close_failures != 0 || s.pause_until != 0.0,
            )
        };
        if reset.0 {
            self.reset_reconnect_backoff();
        }
        if reset.1 {
            self.reset_failure_streak();
        }
        if self.treat_any_message_as_pong.get() {
            self.cancel(Timer::PongMiss);
        }
        if was_zero {
            let mut s = self.state.borrow_mut();
            s.initialized = true;
            s.on_close_count = 0;
            s.reconnect_count = 0;
            drop(s);
            self.parent.sync_socket_health_to_store()?;
            self.schedule(Timer::Receive, 1000.0);
        }
        Ok(())
    }
    pub fn check_last_received(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let now = self.now();
        let (received, ping_sent) = {
            let s = self.state.borrow();
            (s.last_received, s.ping_sent)
        };
        let diff = now - received;
        let received_at = iso_timestamp(received)?;
        let stale = self
            .get_watchdog_stale_threshold_ms()
            .unwrap_or(WS_RECONNECT_THRESHOLD_MS);
        let ping = self
            .get_watchdog_ping_threshold_ms()
            .unwrap_or(WS_PING_THRESHOLD_MS);
        if diff > stale {
            self.parent.log(
                &format!(
                    "[trackLastReceived] {} last received message {diff}ms ago (at {received_at}), reconnecting",
                    self.parent.name()
                ),
                LogSeverity::Error,
            )?;
            self.cancel(Timer::Receive);
            let mut s = self.state.borrow_mut();
            s.last_received = 0.0;
            s.ping_sent = false;
            drop(s);
            self.reconnect()?;
        } else if diff > ping && !ping_sent {
            self.parent.log(
                &format!(
                    "[trackLastReceived] {} last received message {diff}ms ago (at {received_at}), sending ping",
                    self.parent.name()
                ),
                LogSeverity::Info,
            )?;
            if self.ping().is_ok() {
                self.state.borrow_mut().ping_sent = true;
            }
        } else if diff <= ping {
            self.state.borrow_mut().ping_sent = false;
        }
        Ok(())
    }
    pub async fn authenticate_and_subscribe(
        self: &Rc<Self>,
        auth: ExchangeFuture<'static, ()>,
        subscribe: impl FnOnce(&Rc<Self>) -> Result<(), ExchangeError>,
    ) -> Result<(), ExchangeError> {
        if self.is_disposed() {
            return Ok(());
        }
        let result = auth.await;
        if self.is_disposed() {
            return Ok(());
        }
        if result.is_err() {
            self.direct_fallback.borrow_mut().disregard_silence();
            self.parent.log(
                "Private WebSocket signing could not complete; reconnecting",
                LogSeverity::Warning,
            )?;
            self.reconnect()?;
            return Ok(());
        }
        subscribe(self)
    }
    pub fn diagnostics(&self) -> WsHealth {
        let s = self.state.borrow();
        WsHealth {
            initialized: s.initialized,
            last_connected_at: s.last_connected_at,
            last_message_timestamp: (s.last_received > 0.0).then_some(s.last_received),
            lifetime_close_count: s.lifetime_close_count,
            last_close_code: s.last_close_code,
            last_close_at: s.last_close_at,
            on_close_count: s.on_close_count,
            reconnect_count: s.reconnect_count,
        }
    }
    pub fn dispatch_message(
        &self,
        key: &str,
        message: &crate::js_runtime::value::Js,
    ) -> Result<(), ExchangeError> {
        let Some(callbacks) = self.message_handlers.borrow().get(key).cloned() else {
            return Ok(());
        };
        if callbacks.borrow().is_empty() {
            return Ok(());
        }
        self.record_message_handler_stats(key)?;
        let mut index = 0;
        loop {
            let callback = callbacks.borrow().get(index).cloned();
            let Some(callback) = callback else {
                break;
            };
            callback(message)?;
            index += 1;
        }
        Ok(())
    }
    // Task handles are socket-owned: closing host I/O need not wake a pending
    // receive or auth future. Never call cancellation/drop while holding state.
    fn spawn_owned(self: &Rc<Self>, future: Pin<Box<dyn Future<Output = ()>>>) -> OwnedOperation {
        self.spawn_owned_kind(future, false, false, RuntimeTaskKind::Finite)
    }
    fn spawn_owned_kind(
        self: &Rc<Self>,
        future: Pin<Box<dyn Future<Output = ()>>>,
        relay: bool,
        eager: bool,
        kind: RuntimeTaskKind,
    ) -> OwnedOperation {
        let (key, generation) = {
            let mut state = self.state.borrow_mut();
            state.next_task += 1;
            if relay {
                state.relay_task = Some(state.next_task);
            }
            (state.next_task, state.generation)
        };
        let completed = Rc::new(Cell::new(false));
        let mut guard = SocketTaskGuard {
            socket: Rc::downgrade(self),
            key: SocketTaskKey::Owned(key),
            completed: completed.clone(),
        };
        let build = move |_| {
            Box::pin(async move {
                future.await;
                guard.complete();
            }) as Pin<Box<dyn Future<Output = ()>>>
        };
        let task = if eager {
            let runtime: Rc<dyn ExchangeRuntime> = self.runtime.clone();
            self.owned_tasks.start_eager(&runtime, build, |_| {})
        } else {
            self.owned_tasks
                .start_classified(self.runtime.as_ref(), kind, build, |_| {})
        };
        let operation = task.clone();
        // A host may poll inline, including invoking dispose before returning
        // its handle. In either case it must not install a stale handle.
        if completed.get() {
            drop(task);
        } else if self.is_disposed() || self.state.borrow().generation != generation {
            task.cancel();
        } else {
            if relay {
                let old = self.state.borrow_mut().relay_operation.replace(task);
                if let Some(old) = old {
                    old.cancel();
                }
            }
        }
        operation
    }
    async fn dispatch_open(self: &Rc<Self>, generation: u64) {
        self.track_connected();
        let weak = Rc::downgrade(self);
        let mut opening = Box::pin(async move {
            let Some(socket) = weak.upgrade() else {
                return;
            };
            if socket.is_disposed() || socket.state.borrow().generation != generation {
                return;
            }
            let mut opening = std::pin::pin!(socket.on_open());
            let mut first_poll = true;
            let result = std::future::poll_fn(|cx| {
                let result = opening.as_mut().poll(cx);
                // JS invokes onOpen's synchronous prefix before the next open listener.
                // A private auth await must not hold up the reconnect notification.
                if first_poll {
                    first_poll = false;
                    if !socket.is_disposed()
                        && socket.state.borrow().reconnect_listener_generation == Some(generation)
                    {
                        if socket.state.borrow().register_reconnect_open {
                            socket.state.borrow_mut().provisional_reconnect_open = Some(generation);
                        } else {
                            socket.report_callback_result(socket.emit_ws_reconnected());
                        }
                    }
                }
                result
            })
            .await;
            if let Err(error) = result
                && !socket.is_disposed()
                && socket.state.borrow().generation == generation
            {
                socket.report_callback_result(socket.handle_web_socket_error(&error.to_string()));
            }
        });
        let completed =
            std::future::poll_fn(|cx| std::task::Poll::Ready(opening.as_mut().poll(cx).is_ready()))
                .await;
        if !completed {
            self.spawn_owned(opening);
        }
    }
    fn cancel(&self, timer: Timer) {
        let task = {
            let mut state = self.state.borrow_mut();
            state.timer_ids.remove(&timer);
            state.tasks.remove(&timer)
        };
        if let Some(task) = task {
            task.cancel();
        }
    }
    fn schedule(self: &Rc<Self>, timer: Timer, delay: f64) {
        self.cancel(timer);
        let id = {
            let mut state = self.state.borrow_mut();
            state.next_task += 1;
            let id = state.next_task;
            state.timer_ids.insert(timer, id);
            id
        };
        let completed = Rc::new(Cell::new(false));
        let weak = Rc::downgrade(self);
        let mut guard = SocketTaskGuard {
            socket: weak.clone(),
            key: SocketTaskKey::Timer(timer, id),
            completed: completed.clone(),
        };
        let sleep = self.runtime.sleep(delay.max(0.0).ceil() as u64);
        let task = self.owned_tasks.start_classified(
            self.runtime.as_ref(),
            RuntimeTaskKind::Timer,
            |_| {
                Box::pin(async move {
                    if sleep.await.is_err() {
                        return;
                    }
                    guard.complete();
                    let Some(socket) = weak.upgrade() else {
                        return;
                    };
                    if socket.is_disposed() {
                        return;
                    }
                    let result = (|| -> Result<(), ExchangeError> {
                        match timer {
                            Timer::Ping => {
                                socket.ping()?;
                            }
                            Timer::PongMiss => {
                                let message =
                                    format!("WebSocket pong miss after {delay}ms, reconnecting...");
                                lifecycle_log(
                                    socket.runtime.as_ref(),
                                    &format!("{} {message}", socket.parent.name()),
                                );
                                socket.parent.emit("grafana-error", &message)?;
                                socket.reconnect()?;
                            }
                            Timer::Receive => {
                                if socket.state.borrow().last_received != 0.0 {
                                    socket.protocol.check_last_received(&socket)?;
                                    socket.parent.sync_socket_health_to_store()?;
                                    socket.schedule(Timer::Receive, 1000.0);
                                }
                            }
                            Timer::Reconnect => {
                                if let Err(error) = socket.reconnect_now() {
                                    socket.runtime.report_exception(&error, &Value::Null.into());
                                    return Ok(());
                                }
                            }
                        }
                        Ok(())
                    })();
                    socket.report_callback_result(result);
                })
            },
            |_| {},
        );
        if !completed.get()
            && self.state.borrow().timer_ids.get(&timer) == Some(&id)
            && !self.is_disposed()
        {
            let old = self.state.borrow_mut().tasks.insert(timer, task);
            drop(old);
        } else {
            task.cancel();
        }
    }
}
#[derive(Debug, Clone)]
pub struct WsHealth {
    pub initialized: bool,
    pub last_connected_at: f64,
    pub last_message_timestamp: Option<f64>,
    pub lifetime_close_count: usize,
    pub last_close_code: Option<u16>,
    pub last_close_at: f64,
    pub on_close_count: usize,
    pub reconnect_count: usize,
}

fn iso_timestamp(milliseconds: f64) -> Result<String, ExchangeError> {
    safe_cex_num::js_date_to_iso_string(milliseconds).map_err(|error| {
        let mut value = ExchangeError::new(error.message);
        value.name = error.name.into();
        value
    })
}
fn stringify(data: &Value) -> String {
    match data {
        Value::String(text) => text.clone(),
        _ => stringify_json(data),
    }
}

fn stringify_json(value: &Value) -> String {
    safe_cex_num::js_json_stringify(&safe_cex_num::JsJsonValue::from(value.clone()))
        .expect("JSON values contain no BigInt or executable serialization hooks")
        .expect("JSON values cannot be top-level undefined")
}
fn secret_key(key: &str) -> bool {
    [
        "apikey",
        "api_key",
        "key",
        "secret",
        "passphrase",
        "sign",
        "signature",
        "wsauthsignature",
        "jwt",
        "token",
        "sessiontoken",
        "session",
        "authkey",
        "listenkey",
        "password",
    ]
    .iter()
    .any(|known| key.eq_ignore_ascii_case(known))
}
fn js_whitespace(ch: char) -> bool {
    let mut bytes = [0; 4];
    safe_cex_num::js_trim(ch.encode_utf8(&mut bytes)).is_empty()
}

fn skip_space(text: &str, mut at: usize) -> usize {
    while let Some(ch) = text[at..].chars().next().filter(|ch| js_whitespace(*ch)) {
        at += ch.len_utf8();
    }
    at
}
fn json_secret_values(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut copied = 0;
    let mut scan = 0;
    while let Some(offset) = text[scan..].find('"') {
        let start = scan + offset;
        let Some(end_offset) = text[start + 1..].find('"') else {
            break;
        };
        let key_end = start + 1 + end_offset;
        scan = start + 1;
        if !secret_key(&text[start + 1..key_end]) {
            continue;
        }
        let colon = skip_space(text, key_end + 1);
        if text.as_bytes().get(colon) != Some(&b':') {
            continue;
        }
        let value_start = skip_space(text, colon + 1);
        if value_start == text.len() {
            continue;
        }
        let mut quoted_end = None;
        if text.as_bytes()[value_start] == b'"' {
            let mut at = value_start + 1;
            while at < text.len() {
                let ch = text[at..].chars().next().expect("character boundary");
                at += ch.len_utf8();
                if ch == '"' {
                    quoted_end = Some(at);
                    break;
                }
                if ch == '\\' && at < text.len() {
                    at += text[at..]
                        .chars()
                        .next()
                        .expect("escaped character")
                        .len_utf8();
                }
            }
        }
        let value_end = quoted_end.unwrap_or_else(|| {
            let mut at = value_start;
            for ch in text[value_start..].chars() {
                if matches!(ch, ',' | '}' | ']') || js_whitespace(ch) {
                    break;
                }
                at += ch.len_utf8();
            }
            at
        });
        if value_end == value_start {
            continue;
        }
        output.push_str(&text[copied..value_start]);
        output.push_str("\"[redacted]\"");
        copied = value_end;
        scan = value_end;
    }
    output.push_str(&text[copied..]);
    output
}
fn query_secret_values(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut copied = 0;
    for (equal, ch) in text.char_indices() {
        if ch != '=' || equal < copied {
            continue;
        }
        let before = &text[..equal];
        let start = before
            .char_indices()
            .rev()
            .find(|(_, ch)| matches!(ch, '?' | '&') || js_whitespace(*ch))
            .map_or(0, |(at, ch)| at + ch.len_utf8());
        if start < copied || !secret_key(&text[start..equal]) {
            continue;
        }
        let value_start = equal + 1;
        let mut value_end = value_start;
        for ch in text[value_start..].chars() {
            if ch == '&' || js_whitespace(ch) {
                break;
            }
            value_end += ch.len_utf8();
        }
        if value_end == value_start {
            continue;
        }
        output.push_str(&text[copied..value_start]);
        output.push_str("[redacted]");
        copied = value_end;
    }
    output.push_str(&text[copied..]);
    output
}
fn shorten_hex_secrets(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut copied = 0;
    let mut at = 0;
    let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    while at < bytes.len() {
        if !bytes[at].is_ascii_hexdigit() {
            at += 1;
            continue;
        }
        let start = at;
        while at < bytes.len() && bytes[at].is_ascii_hexdigit() {
            at += 1;
        }
        if at - start >= 40
            && (start == 0 || !word(bytes[start - 1]))
            && (at == bytes.len() || !word(bytes[at]))
        {
            output.push_str(&text[copied..start]);
            output.push_str(&text[start..start + 4]);
            output.push_str("...");
            output.push_str(&text[at - 4..at]);
            copied = at;
        }
    }
    output.push_str(&text[copied..]);
    output
}
fn redact_payload(message: &str) -> String {
    let structured = serde_json::from_str::<Value>(safe_cex_num::js_trim(message))
        .ok()
        .filter(|value| value.is_object() || value.is_array())
        .map(|mut value| {
            redact_json(&mut value, 0, false);
            stringify_json(&value)
        });
    let source = structured.as_deref().unwrap_or(message);
    shorten_hex_secrets(&query_secret_values(&json_secret_values(source)))
}
fn redact_json(value: &mut Value, depth: usize, redact_strings: bool) {
    if depth > 12 {
        return;
    }
    match value {
        Value::String(text) if redact_strings => *text = "[redacted]".into(),
        Value::Array(values) => {
            for value in values {
                redact_json(value, depth + 1, redact_strings);
            }
        }
        Value::Object(map) => {
            // Source rebuilds {}, whose local prototype setter creates no own key.
            map.remove("__proto__");
            let auth = redact_strings
                || ["op", "method", "type", "event"].iter().any(|key| {
                    map.get(*key).and_then(Value::as_str).is_some_and(|text| {
                        let lower = text.to_ascii_lowercase();
                        ["auth", "login", "signature", "jwt"]
                            .iter()
                            .any(|part| lower.contains(part))
                    })
                });
            for (key, value) in map {
                let normalized: String = key
                    .chars()
                    .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                    .map(|ch| ch.to_ascii_lowercase())
                    .collect();
                if [
                    "apikey",
                    "api_key",
                    "key",
                    "secret",
                    "passphrase",
                    "sign",
                    "signature",
                    "wsauthsignature",
                    "jwt",
                    "token",
                    "sessiontoken",
                    "session",
                    "authkey",
                    "listenkey",
                    "password",
                ]
                .contains(&normalized.as_str())
                {
                    *value = Value::String("[redacted]".into());
                } else {
                    redact_json(
                        value,
                        depth + 1,
                        auth && matches!(normalized.as_str(), "args" | "params"),
                    );
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    include!("dormant_socket_tests.rs");
    include!("root_ws_tests.rs");
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
    };
    type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);
    #[derive(Default)]
    struct Runtime {
        now: Rc<Cell<f64>>,
        frame_histories: RefCell<Vec<Rc<RefCell<Vec<FrameDrainReceipt>>>>>,
        jobs: RefCell<Vec<Job>>,
        inline_spawn: Cell<bool>,
        fail_sleep: Cell<bool>,
        pending_dial: Cell<bool>,
        open_each_dial: Cell<bool>,
        dial_drops: Rc<Cell<usize>>,
        diagnostics: Cell<bool>,
        exceptions: RefCell<Vec<ExchangeError>>,
        relaying: Cell<bool>,
        tunnel: RefCell<Option<Rc<RelayHost>>>,
        headers: Cell<bool>,
        dial_options: RefCell<Vec<WebSocketConnectionOptions>>,
        socket: Rc<Socket>,
        task_drop: Rc<RefCell<Option<Box<dyn FnOnce()>>>>,
    }
    struct DialDrop(Rc<Cell<usize>>);
    impl Drop for DialDrop {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    struct Task(Rc<Cell<bool>>, Rc<RefCell<Option<Box<dyn FnOnce()>>>>);
    impl Drop for Task {
        fn drop(&mut self) {
            let callback = self.1.borrow_mut().take();
            if let Some(callback) = callback {
                callback();
            }
        }
    }
    impl ExchangeTask for Task {
        fn cancel(&self) {
            self.0.set(true);
        }
    }
    impl Runtime {
        fn pump(&self) {
            let mut jobs = std::mem::take(&mut *self.jobs.borrow_mut());
            let mut pending = Vec::new();
            let mut cx = Context::from_waker(Waker::noop());
            for (mut future, cancelled) in jobs.drain(..) {
                if !cancelled.get() && future.as_mut().poll(&mut cx).is_pending() {
                    pending.push((future, cancelled));
                }
            }
            self.jobs.borrow_mut().extend(pending);
        }
        fn advance(&self, ms: f64) {
            self.now.set(self.now.get() + ms);
            self.pump();
        }
    }
    impl ExchangeRuntime for Runtime {
        fn monotonic_now_ms(&self) -> f64 {
            self.now.get()
        }
        fn now_ms(&self) -> f64 {
            self.now.get()
        }
        fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
            if self.fail_sleep.get() {
                return Box::pin(async { Err(ExchangeError::new("cancelled sleep")) });
            }
            let now = self.now.clone();
            let deadline = now.get() + delay as f64;
            Box::pin(std::future::poll_fn(move |_| {
                if now.get() >= deadline {
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                }
            }))
        }
        fn spawn_local(
            &self,
            mut future: Pin<Box<dyn Future<Output = ()>>>,
        ) -> Box<dyn ExchangeTask> {
            let cancelled = Rc::new(Cell::new(false));
            if self.inline_spawn.get()
                && future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_ready()
            {
                return Box::new(Task(cancelled, self.task_drop.clone()));
            }
            self.jobs.borrow_mut().push((future, cancelled.clone()));
            Box::new(Task(cancelled, self.task_drop.clone()))
        }
        fn console_values(&self, _: &str, _: &[crate::js_runtime::value::Js]) {}
        fn unique_id(&self) -> String {
            "id".into()
        }
        fn is_worker(&self) -> bool {
            false
        }
        fn random(&self) -> f64 {
            0.0
        }
        fn report_raw_exception(
            &self,
            _: &crate::js_runtime::value::Js,
            _: &safe_cex_types::retained_json::RetainedJsonInput,
        ) {
            panic!("unexpected raw exception report in this test runtime");
        }
        fn report_exception(
            &self,
            error: &ExchangeError,
            _: &safe_cex_types::retained_json::RetainedJsonInput,
        ) {
            self.exceptions.borrow_mut().push(error.clone());
        }
        fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
            Box::pin(async { Ok(Value::Null) })
        }
    }
    impl WsRuntime for Runtime {
        fn frame_receipts_enabled(&self) -> bool {
            true
        }
        fn register_frame_receipts(&self, receipts: Rc<RefCell<Vec<FrameDrainReceipt>>>) {
            self.frame_histories.borrow_mut().push(receipts);
        }
        fn transport(&self) -> Arc<dyn TransportProvider> {
            if self.relaying.get() {
                Arc::new(Relaying)
            } else {
                Arc::new(safe_cex_transport::DirectTransport)
            }
        }
        fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
            self.tunnel
                .borrow()
                .as_ref()
                .expect("test tunnel configured")
                .clone()
        }
        fn dial(
            &self,
            _: String,
            _: Vec<String>,
            options: WebSocketConnectionOptions,
            _: bool,
        ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
            self.dial_options.borrow_mut().push(options);
            let socket: Rc<dyn WsSocket> = if self.open_each_dial.get() {
                let socket = Rc::new(Socket::default());
                socket.events.borrow_mut().push_back(WsEvent::Open);
                socket
            } else {
                self.socket.clone()
            };
            let pending = self.pending_dial.get();
            let guard = DialDrop(self.dial_drops.clone());
            Box::pin(async move {
                let _guard = guard;
                if pending {
                    std::future::pending::<()>().await;
                }
                Ok(socket)
            })
        }
        fn headers_capable(&self) -> bool {
            self.headers.get()
        }
        fn enable_reconnecting(&self) -> bool {
            false
        }
        fn min_reconnection_delay_ms(&self) -> f64 {
            1000.0
        }
        fn library_min_reconnection_delay_ms(&self) -> f64 {
            3000.0
        }
        fn diagnostics_enabled(&self) -> bool {
            self.diagnostics.get()
        }
        fn lifecycle_logs_enabled(&self) -> bool {
            false
        }
    }
    struct RelayHost {
        now: Rc<Cell<f64>>,
        supports_headers: Cell<bool>,
        dials: RefCell<Vec<safe_cex_transport::socket::TunnelDialOptions>>,
        sockets: RefCell<Vec<Rc<RelayIo>>>,
        handshake: Cell<bool>,
    }
    impl RelayHost {
        fn new(runtime: &Runtime) -> Self {
            Self {
                now: runtime.now.clone(),
                supports_headers: Cell::new(true),
                dials: RefCell::new(Vec::new()),
                sockets: RefCell::new(Vec::new()),
                handshake: Cell::new(true),
            }
        }
    }
    struct RelayIo {
        frames: Rc<RefCell<VecDeque<SocketFrame>>>,
        closed: Cell<usize>,
        close_codes: RefCell<Vec<Option<u16>>>,
    }
    impl safe_cex_transport::socket::TunnelSocket for RelayIo {
        fn send(&self, _: SocketFrame) -> Result<(), safe_cex_transport::TransportError> {
            Ok(())
        }
        fn receive(&self) -> safe_cex_transport::TransportFuture<'static, SocketFrame> {
            let frames = self.frames.clone();
            Box::pin(std::future::poll_fn(move |_| {
                frames
                    .borrow_mut()
                    .pop_front()
                    .map_or(Poll::Pending, |frame| Poll::Ready(Ok(frame)))
            }))
        }
        fn close(
            &self,
            code: Option<u16>,
            _: &str,
        ) -> Result<(), safe_cex_transport::TransportError> {
            self.close_codes.borrow_mut().push(code);
            self.closed.set(self.closed.get() + 1);
            Ok(())
        }
    }
    impl TunnelHost for RelayHost {
        fn spawn_local(
            &self,
            _: Pin<Box<dyn Future<Output = ()>>>,
        ) -> Box<dyn safe_cex_transport::socket::TunnelTask> {
            unreachable!("Relay uses the SDK runtime event consumer")
        }
        fn capabilities(&self) -> safe_cex_transport::socket::SocketCapabilities {
            safe_cex_transport::socket::SocketCapabilities {
                runtime: safe_cex_transport::socket::SocketRuntime::NodeWs,
                upgrade_headers: self.supports_headers.get(),
                alpn: true,
            }
        }
        fn now_ms(&self) -> u64 {
            self.now.get() as u64
        }
        fn sleep_until(&self, deadline: u64) -> safe_cex_transport::TransportFuture<'static, ()> {
            let now = self.now.clone();
            Box::pin(std::future::poll_fn(move |_| {
                if now.get() as u64 >= deadline {
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                }
            }))
        }
        fn connect(
            &self,
            url: String,
            options: safe_cex_transport::socket::TunnelDialOptions,
        ) -> safe_cex_transport::TransportFuture<
            'static,
            Rc<dyn safe_cex_transport::socket::TunnelSocket>,
        > {
            assert_eq!(url, "wss://relay/tunnel");
            self.dials.borrow_mut().push(options);
            let frames = if self.handshake.get() {
                VecDeque::from([
                    SocketFrame::Text(r#"{"type":"rpc.auth","status":"success"}"#.into()),
                    SocketFrame::Text(r#"{"type":"rpc.relay","status":"open"}"#.into()),
                ])
            } else {
                VecDeque::new()
            };
            let socket = Rc::new(RelayIo {
                frames: Rc::new(RefCell::new(frames)),
                closed: Cell::new(0),
                close_codes: RefCell::new(Vec::new()),
            });
            self.sockets.borrow_mut().push(socket.clone());
            let socket: Rc<dyn safe_cex_transport::socket::TunnelSocket> = socket;
            Box::pin(async move { Ok(socket) })
        }
        fn proxy_token(&self) -> safe_cex_transport::TransportFuture<'_, String> {
            Box::pin(async { Ok("test-token".into()) })
        }
        fn is_token_expired(&self, _: &str) -> bool {
            false
        }
        fn sync_server_time(&self, _: u64) {}
    }
    struct Relaying;
    impl TransportProvider for Relaying {
        fn name(&self) -> &str {
            "relay-test"
        }
        fn rest(
            &self,
            context: safe_cex_transport::RestTransportContext<'_>,
        ) -> Result<safe_cex_transport::RestTransport, safe_cex_transport::TransportError> {
            safe_cex_transport::DirectTransport.rest(context)
        }
        fn rest_adapter(
            &self,
            context: safe_cex_transport::RestAdapterContext<'_>,
        ) -> Option<Arc<dyn safe_cex_transport::RestAdapter>> {
            safe_cex_transport::DirectTransport.rest_adapter(context)
        }
        fn proxy_prefix(&self, context: safe_cex_transport::RestTransportContext<'_>) -> String {
            safe_cex_transport::DirectTransport.proxy_prefix(context)
        }
        fn public_cors_proxy(&self) -> Option<String> {
            None
        }
        fn private_proxy(
            &self,
            _: &safe_cex_types::ExchangeName,
        ) -> Option<Arc<dyn safe_cex_transport::PrivateProxy>> {
            None
        }
        fn require_proxy(&self, options: ExchangeOptions) -> ExchangeOptions {
            options
        }
        fn server_proxy_url(&self, _: &safe_cex_types::ExchangeName) -> Option<String> {
            None
        }
        fn ws_routing(
            &self,
            exchange: &safe_cex_types::ExchangeName,
            options: &safe_cex_transport::RoutingOptions,
        ) -> safe_cex_transport::ResolvedRouting {
            safe_cex_transport::DirectTransport.ws_routing(exchange, options)
        }
        fn ws_connection(&self, _: &str) -> Option<WsConnection> {
            Some(WsConnection::Relay {
                headers: BTreeMap::new(),
            })
        }
    }
    #[derive(Default)]
    struct Socket {
        events: Rc<RefCell<VecDeque<WsEvent>>>,
        ready: Cell<u8>,
        sent: RefCell<Vec<String>>,
        closed: Cell<usize>,
        reconnected: Cell<usize>,
        managed: Cell<bool>,
    }
    impl WsSocket for Socket {
        fn ready_state(&self) -> u8 {
            self.ready.get()
        }
        fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
            let events = self.events.clone();
            Box::pin(std::future::poll_fn(move |_| {
                events
                    .borrow_mut()
                    .pop_front()
                    .map_or(Poll::Pending, |event| Poll::Ready(Ok(event)))
            }))
        }
        fn send(&self, s: &str) -> Result<(), ExchangeError> {
            self.sent.borrow_mut().push(s.into());
            Ok(())
        }
        fn close(&self) {
            self.closed.set(self.closed.get() + 1);
        }
        fn terminate(&self) {}
        fn reconnect(&self) {
            self.reconnected.set(self.reconnected.get() + 1);
        }
        fn is_reconnecting(&self) -> bool {
            self.managed.get()
        }
    }
    struct Parent {
        options: ExchangeOptions,
        disposed: Cell<bool>,
        events: RefCell<Vec<(String, String)>>,
        logs: RefCell<Vec<(String, LogSeverity)>>,
        sync: Cell<usize>,
        latency: Cell<f64>,
        coordinator: Rc<ReopenResync>,
        snapshots: Cell<usize>,
        snapshot_gate: Rc<Cell<bool>>,
        snapshot_failure: Cell<bool>,
        snapshot_error: RefCell<Option<ExchangeError>>,
        log_error: RefCell<Option<ExchangeError>>,
        announcement_error: RefCell<Option<ExchangeError>>,
    }
    impl Parent {
        fn new() -> Self {
            Self {
                options: serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
                disposed: Cell::new(false),
                events: RefCell::new(Vec::new()),
                logs: RefCell::new(Vec::new()),
                sync: Cell::new(0),
                latency: Cell::new(0.0),
                coordinator: Rc::new(ReopenResync::default()),
                snapshots: Cell::new(0),
                snapshot_gate: Rc::new(Cell::new(true)),
                snapshot_failure: Cell::new(false),
                snapshot_error: RefCell::new(None),
                log_error: RefCell::new(None),
                announcement_error: RefCell::new(None),
            }
        }
    }
    impl WsParent for Parent {
        fn name(&self) -> &str {
            "venue"
        }
        fn is_disposed(&self) -> bool {
            self.disposed.get()
        }
        fn options(&self) -> ExchangeOptions {
            self.options.clone()
        }
        fn store(&self) -> Rc<dyn Store> {
            panic!("unused test store")
        }
        fn is_private_socket(&self, _: &BaseWebSocket) -> bool {
            true
        }
        fn resync_coordinator(&self) -> Rc<ReopenResync> {
            self.coordinator.clone()
        }
        fn fetch_and_set_private_snapshot(&self, retries: usize) -> ExchangeFuture<'static, ()> {
            assert_eq!(retries, 10);
            self.snapshots.set(self.snapshots.get() + 1);
            let gate = self.snapshot_gate.clone();
            let fail = self.snapshot_failure.get();
            let error = self.snapshot_error.borrow().clone();
            Box::pin(std::future::poll_fn(move |_| {
                if gate.get() {
                    Poll::Ready(if let Some(error) = &error {
                        Err(error.clone())
                    } else if fail {
                        Err(ExchangeError::new("snapshot rejected"))
                    } else {
                        Ok(())
                    })
                } else {
                    Poll::Pending
                }
            }))
        }
        fn log(&self, s: &str, severity: LogSeverity) -> Result<(), ExchangeError> {
            self.logs.borrow_mut().push((s.into(), severity));
            self.log_error.borrow().clone().map_or(Ok(()), Err)
        }
        fn emit(&self, event: &str, s: &str) -> Result<(), ExchangeError> {
            self.events.borrow_mut().push((event.into(), s.into()));
            Ok(())
        }
        fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
            self.sync.set(self.sync.get() + 1);
            Ok(())
        }
        fn update_latency(&self, n: f64) -> Result<(), ExchangeError> {
            self.latency.set(n);
            Ok(())
        }
        fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
            self.emit("ws-reconnected", "")?;
            Ok(())
        }
        fn emit_and_log_error(
            &self,
            error: &str,
            _: Option<&str>,
            _: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
        ) -> Result<(), ExchangeError> {
            if let Some(error) = self.announcement_error.borrow().clone() {
                return Err(error);
            }
            self.emit("error", error)?;
            Ok(())
        }
    }
    #[derive(Default)]
    struct Protocol {
        pings: Cell<usize>,
        connects: Cell<usize>,
        connect_error: RefCell<Option<ExchangeError>>,
        dial_before_error: Cell<bool>,
        pending_open: Cell<bool>,
        message_error: Cell<bool>,
    }
    impl WsProtocol for Protocol {
        fn ping(&self, _: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
            self.pings.set(self.pings.get() + 1);
            Ok(())
        }
        fn connect_and_subscribe(
            &self,
            socket: Rc<BaseWebSocket>,
            _: bool,
        ) -> Result<(), ExchangeError> {
            self.connects.set(self.connects.get() + 1);
            if self.dial_before_error.get() {
                socket.start_websocket("wss://fixture".into(), vec![], Default::default());
            }
            match self.connect_error.borrow().clone() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }
        fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
            if self.pending_open.get() {
                return Box::pin(std::future::pending());
            }
            Box::pin(async { Ok(()) })
        }
        fn on_message(
            &self,
            socket: &Rc<BaseWebSocket>,
            _: &SocketFrame,
        ) -> Result<(), ExchangeError> {
            socket.track_last_received().unwrap();
            if self.message_error.get() {
                return Err(ExchangeError::new("fixture handler failed"));
            }
            Ok(())
        }
    }
    #[test]
    fn public_topic_timeout_cancel_removes_task_and_replacement_fires_once() {
        let (socket, _, runtime, _) = fixture();
        let calls = Rc::new(Cell::new(0));
        let capture = Rc::new(());
        let weak = Rc::downgrade(&capture);
        let old = socket
            .start_timeout(10, move || {
                let _capture = &capture;
                panic!("cancelled topic fired");
            })
            .unwrap();
        runtime.pump();
        old.cancel();
        assert_eq!(
            socket.owned_tasks.len(),
            0,
            "topic cancellation removes owned registry entry immediately"
        );
        assert!(
            weak.upgrade().is_none(),
            "topic cancellation releases callback before host next turn"
        );
        let target = calls.clone();
        let replacement = socket
            .start_timeout(10, move || {
                target.set(target.get() + 1);
                Ok(())
            })
            .unwrap();
        runtime.pump();
        old.cancel();
        runtime.advance(10.0);
        assert_eq!(
            calls.get(),
            1,
            "old topic cancellation must not remove replacement"
        );
        assert!(!replacement.is_active());
        assert_eq!(socket.owned_tasks.len(), 0);
    }
    #[test]
    fn failed_reconnect_hook_revokes_started_dial_listener_including_inline_open() {
        for inline in [false, true] {
            let (socket, parent, runtime, protocol) = fixture();
            runtime.inline_spawn.set(inline);
            protocol.dial_before_error.set(true);
            runtime.open_each_dial.set(true);
            *protocol.connect_error.borrow_mut() = Some(ExchangeError::new("after dial"));
            socket.report_callback_result(socket.on_close(Some(1000), ""));
            runtime.pump();
            runtime.advance(1000.0);
            for _ in 0..4 {
                runtime.pump();
            }
            assert!(
                parent
                    .events
                    .borrow()
                    .iter()
                    .all(|(event, _)| event != "ws-reconnected"),
                "failed hook must revoke its provisional open listener, inline={inline}"
            );
            assert_eq!(runtime.exceptions.borrow().len(), 1);
            // A later independent dial must not inherit or be cleared by the failed registration.
            *protocol.connect_error.borrow_mut() = None;
            socket.state.borrow_mut().reconnecting = false;
            socket.report_callback_result(socket.on_close(Some(1000), ""));
            runtime.pump();
            runtime.advance(1000.0);
            for _ in 0..4 {
                runtime.pump();
            }
            assert_eq!(
                parent
                    .events
                    .borrow()
                    .iter()
                    .filter(|(event, _)| event == "ws-reconnected")
                    .count(),
                1,
                "successor must retain its own reconnect listener, inline={inline}"
            );
        }
    }
    struct MessageAuthGuard(Rc<Cell<bool>>);
    impl Drop for MessageAuthGuard {
        fn drop(&mut self) {
            self.0.set(false);
        }
    }
    #[test]
    fn message_operation_disposal_drops_task_and_auth_guard_without_dial() {
        for mode in ["unpolled", "pending", "inline-pending"] {
            let (socket, _, runtime, _) = fixture();
            runtime.inline_spawn.set(mode == "inline-pending");
            let generation = socket.state.borrow().generation;
            let auth = Rc::new(Cell::new(true));
            let guard = MessageAuthGuard(auth.clone()); // Before future construction.
            let owner = socket.clone();
            socket.start_message_operation(async move {
                let _guard = guard;
                let _socket = owner;
                std::future::pending::<()>().await;
                Ok(())
            });
            if mode == "pending" {
                runtime.pump();
            }
            assert_eq!(
                socket.owned_tasks.len(),
                1,
                "message operation must be core-owned: {mode}"
            );
            assert_eq!(
                socket.state.borrow().generation,
                generation,
                "message work must not reserve a connection generation"
            );
            let weak = Rc::downgrade(&socket);
            socket.dispose();
            drop(socket);
            runtime.pump(); // Drop the TASK, not an awaiting caller.
            assert!(
                !auth.get(),
                "discarded message task must release auth latch: {mode}"
            );
            assert!(
                weak.upgrade().is_none(),
                "message task must release disposed socket: {mode}"
            );
            assert!(
                runtime.dial_options.borrow().is_empty(),
                "message operation must never dial"
            );
        }
    }
    struct MessageOperationProtocol {
        ready: Rc<Cell<bool>>,
        events: Rc<RefCell<Vec<&'static str>>>,
    }
    impl WsProtocol for MessageOperationProtocol {
        fn on_message(
            &self,
            socket: &Rc<BaseWebSocket>,
            frame: &SocketFrame,
        ) -> Result<(), ExchangeError> {
            if matches!(frame, SocketFrame::Text(text) if text == "reauth") {
                self.events.borrow_mut().push("sync-prefix");
                let gate = self.ready.clone();
                let events = self.events.clone();
                socket.start_message_operation(async move {
                    std::future::poll_fn(|_| {
                        if gate.get() {
                            Poll::Ready(())
                        } else {
                            Poll::Pending
                        }
                    })
                    .await;
                    events.borrow_mut().push("async-tail");
                    Err(ExchangeError::new("reauth rejected"))
                });
            } else {
                self.events.borrow_mut().push("next-message");
            }
            Ok(())
        }
    }
    #[test]
    fn message_operation_returns_before_completion_and_reports_rejection_once() {
        for inline in [false, true] {
            let (_, parent, runtime, _) = fixture();
            runtime.inline_spawn.set(inline);
            let ready = Rc::new(Cell::new(false));
            let events = Rc::new(RefCell::new(vec![]));
            let protocol = Rc::new(MessageOperationProtocol {
                ready: ready.clone(),
                events: events.clone(),
            });
            let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), protocol);
            socket
                .on_message(&SocketFrame::Text("reauth".into()))
                .unwrap();
            socket
                .on_message(&SocketFrame::Text("next".into()))
                .unwrap();
            runtime.pump();
            assert_eq!(
                *events.borrow(),
                ["sync-prefix", "next-message"],
                "message handler must return while async tail is pending"
            );
            ready.set(true);
            runtime.pump();
            assert_eq!(
                *events.borrow(),
                ["sync-prefix", "next-message", "async-tail"]
            );
            assert_eq!(
                runtime.exceptions.borrow().len(),
                1,
                "unawaited message rejection must be observable at host boundary"
            );
            assert_eq!(runtime.exceptions.borrow()[0].message, "reauth rejected");
            assert!(
                socket.owned_tasks.len() == 0,
                "completed message task must release registry slot"
            );
            assert!(
                parent.logs.borrow().is_empty(),
                "message rejection is not a transport diagnostic"
            );
            socket.report_callback_result(socket.handle_web_socket_error("reauth rejected"));
            assert!(
                !parent.logs.borrow().is_empty(),
                "message rejection must not consume transport dedup window"
            );
        }
    }
    struct PendingTokenRest {
        polls: Rc<Cell<usize>>,
        owner: Rc<()>,
    }
    impl safe_cex_transport::RestAdapter for PendingTokenRest {
        fn execute(
            &self,
            request: safe_cex_transport::RestRequest,
        ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
            assert_eq!(request.method, "post");
            assert_eq!(request.url, "/bullet-private");
            let owner = self.owner.clone();
            Box::pin(std::future::poll_fn(move |_| {
                let _owner = &owner;
                self.polls.set(self.polls.get() + 1);
                Poll::Pending
            }))
        }
    }
    #[test]
    fn preparation_task_disposal_cancels_rest_future_before_any_dial() {
        for mode in ["unpolled", "pending", "inline-pending"] {
            let (socket, _, runtime, _) = fixture();
            runtime.inline_spawn.set(mode == "inline-pending");
            let token_owner = Rc::new(());
            let token_weak = Rc::downgrade(&token_owner);
            let polls = Rc::new(Cell::new(0));
            let endpoint = PendingTokenRest {
                polls: polls.clone(),
                owner: token_owner,
            };
            socket.start_prepared_websocket(move || async move {
                use safe_cex_transport::RestAdapter;
                endpoint
                    .execute(safe_cex_transport::RestRequest {
                        method: "post".into(),
                        url: "/bullet-private".into(),
                        ..Default::default()
                    })
                    .await
                    .map_err(|error| ExchangeError::new(error.message))?;
                Ok(Some(PreparedWebSocket {
                    url: "wss://token-endpoint".into(),
                    protocols: vec![],
                    options: None,
                }))
            });
            if mode == "pending" {
                runtime.pump();
            }
            assert_eq!(
                socket.owned_tasks.len(),
                1,
                "REST preparation must be core-owned: {mode}"
            );
            assert!(
                polls.get() > 0,
                "preparation prefix runs before caller polling: {mode}"
            );
            let weak = Rc::downgrade(&socket);
            socket.dispose();
            drop(socket);
            runtime.pump(); // Drop the cancelled TASK, not a caller waiting for it.
            assert!(
                token_weak.upgrade().is_none(),
                "dispose must drop REST token future: {mode}"
            );
            assert!(
                weak.upgrade().is_none(),
                "preparation must not retain disposed socket"
            );
            assert!(
                runtime.dial_options.borrow().is_empty(),
                "disposed preparation must never dial"
            );
        }
    }
    #[test]
    fn discarded_preparation_is_recoverable_and_superseded_result_cannot_dial() {
        let (socket, _, runtime, _) = fixture();
        for polled in [false, true] {
            socket.start_prepared_websocket(std::future::pending);
            if polled {
                runtime.pump();
            }
            runtime.jobs.borrow_mut().clear(); // Host discards the producer TASK.
            assert!(
                socket.owned_tasks.len() == 0,
                "preparation discard must release its slot"
            );
            assert!(
                !socket.state.borrow().reconnecting,
                "preparation discard must not latch reconnect"
            );
        }
        let ready = Rc::new(Cell::new(false));
        let gate = ready.clone();
        socket.start_prepared_websocket(move || {
            std::future::poll_fn(move |_| {
                if gate.get() {
                    Poll::Ready(Ok(Some(PreparedWebSocket {
                        url: "wss://stale".into(),
                        protocols: vec![],
                        options: None,
                    })))
                } else {
                    Poll::Pending
                }
            })
        });
        runtime.pump();
        socket.start_websocket("wss://replacement".into(), vec![], None);
        ready.set(true);
        runtime.pump();
        assert_eq!(
            runtime.dial_options.borrow().len(),
            1,
            "only current preparation may dial"
        );
        socket.dispose();
        runtime.pump();
    }
    #[test]
    fn preparation_error_and_no_dial_use_existing_completion_paths() {
        let (socket, parent, runtime, _) = fixture();
        socket
            .start_prepared_websocket(|| async { Err(ExchangeError::new("bullet token refused")) });
        runtime.pump();
        assert!(
            parent
                .logs
                .borrow()
                .iter()
                .any(|(message, _)| message.contains("bullet token refused")),
            "preparation failure must reach existing diagnostics"
        );
        assert!(runtime.dial_options.borrow().is_empty());
        assert!(socket.owned_tasks.len() == 0);
        runtime.inline_spawn.set(true);
        socket.start_prepared_websocket(|| async { Ok(None) });
        assert!(
            socket.owned_tasks.len() == 0,
            "inline no-dial completion must not leave a stale handle"
        );
        assert!(runtime.dial_options.borrow().is_empty());
    }
    #[test]
    fn core_started_dial_errors_use_existing_socket_diagnostics() {
        let (socket, parent, runtime, _) = fixture();
        socket.start_websocket(
            "wss://venue".into(),
            vec![],
            Some(WebSocketConnectionOptions {
                headers_required: true,
                ..Default::default()
            }),
        );
        runtime.pump();
        assert!(parent.logs.borrow().iter().any(|(message, severity)| {
            message.contains("Required WebSocket upgrade headers")
                && *severity == LogSeverity::Error
        }));
        assert!(runtime.dial_options.borrow().is_empty());
        assert!(socket.owned_tasks.len() == 0);
    }
    #[test]
    fn core_started_dial_disposal_drops_unpolled_and_pending_tasks() {
        for mode in ["unpolled", "pending", "inline-pending"] {
            let (socket, _, runtime, _) = fixture();
            runtime.pending_dial.set(true);
            runtime.inline_spawn.set(mode == "inline-pending");
            socket.start_websocket("wss://venue".into(), vec![], None);
            if mode == "pending" {
                runtime.pump();
            }
            assert_eq!(
                socket.owned_tasks.len(),
                1,
                "dial must be core-owned for {mode}"
            );
            let weak = Rc::downgrade(&socket);
            socket.dispose();
            assert_eq!(
                runtime.dial_drops.get(),
                1,
                "dispose must drop the actual eager dial before the scheduler runs"
            );
            drop(socket);
            runtime.pump(); // The host drops cancelled TASKS, without polling the dial again.
            assert!(runtime.jobs.borrow().is_empty());
            assert!(
                weak.upgrade().is_none(),
                "pending dial retained disposed socket"
            );
            assert_eq!(runtime.dial_drops.get(), 1);
        }
    }
    #[test]
    fn disposal_cancels_pending_owned_work_and_releases_socket() {
        let (socket, _, runtime, protocol) = fixture();
        let weak = Rc::downgrade(&socket);
        protocol.pending_open.set(true);
        let generation = socket.state.borrow().generation;
        run(socket.dispatch_open(generation));
        runtime.pump();
        assert_eq!(socket.owned_tasks.len(), 1);
        socket.dispose();
        drop(socket);
        runtime.pump();
        assert!(
            weak.upgrade().is_none(),
            "disposed auth/event task retained socket"
        );
        assert!(runtime.jobs.borrow().is_empty());
    }
    #[test]
    fn frame_receipts_count_processed_messages_separately_for_each_dial() {
        let (socket, _, runtime, protocol) = fixture();
        assert_eq!(runtime.frame_histories.borrow().len(), 1);
        assert!(Rc::ptr_eq(
            &runtime.frame_histories.borrow()[0],
            &socket.frame_receipts
        ));
        run(socket.create_websocket("wss://first".into(), vec![], None)).unwrap();
        runtime.pump();
        socket.on_message(&SocketFrame::Text("one".into())).unwrap();
        socket.on_message(&SocketFrame::Binary(vec![1, 2])).unwrap();
        let receipts = socket.frame_drain_receipts();
        assert_eq!(receipts.len(), 1);
        assert_eq!(
            (
                receipts[0].consumed,
                receipts[0].processed,
                receipts[0].errors
            ),
            (2, 2, 0)
        );
        run(socket.create_websocket("wss://second".into(), vec![], None)).unwrap();
        runtime.pump();
        socket
            .on_message(&SocketFrame::Text("three".into()))
            .unwrap();
        let receipts = socket.frame_drain_receipts();
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].url, "wss://first");
        assert_eq!(receipts[0].processed, 2);
        assert_eq!(receipts[1].url, "wss://second");
        assert_eq!(receipts[1].processed, 1);
        protocol.message_error.set(true);
        assert!(socket.on_message(&SocketFrame::Text("bad".into())).is_err());
        socket.received_transport_close(Some(1000), "done").unwrap();
        let receipts = socket.frame_drain_receipts();
        assert_eq!(
            (
                receipts[1].consumed,
                receipts[1].processed,
                receipts[1].errors
            ),
            (2, 2, 1)
        );
        assert_eq!(
            (receipts[1].close_consumed, receipts[1].close_processed),
            (1, 1)
        );
        let weak = Rc::downgrade(&socket);
        socket.dispose();
        runtime.pump();
        drop(socket);
        assert!(
            weak.upgrade().is_none(),
            "replay counters retain no socket owner"
        );
        let history = runtime.frame_histories.borrow();
        let retained = history[0].borrow();
        assert_eq!(
            retained.len(),
            2,
            "disposed dial histories remain observable"
        );
        assert_eq!((retained[1].processed, retained[1].errors), (2, 1));
    }
    #[test]
    fn disposal_cancels_native_event_consumer_without_host_wakeup() {
        let (socket, _, runtime, _) = fixture();
        run(socket.create_websocket("wss://venue".into(), vec![], None)).unwrap();
        runtime.pump();
        assert_eq!(socket.owned_tasks.len(), 1);
        socket.dispose();
        runtime.pump();
        assert!(
            runtime.jobs.borrow().is_empty(),
            "pending native receive survived disposal"
        );
    }
    #[test]
    fn inline_spawn_completion_does_not_retain_handle() {
        let (socket, _, runtime, _) = fixture();
        runtime.inline_spawn.set(true);
        socket.spawn_owned(Box::pin(async {}));
        assert!(socket.owned_tasks.len() == 0);
        let owner = socket.clone();
        socket.spawn_owned(Box::pin(async move {
            owner.dispose();
            std::future::pending::<()>().await;
        }));
        assert!(socket.owned_tasks.len() == 0);
        runtime.pump();
        assert!(runtime.jobs.borrow().is_empty());
    }
    fn fixture() -> (Rc<BaseWebSocket>, Rc<Parent>, Rc<Runtime>, Rc<Protocol>) {
        let realm = Rc::<safe_cex_num::PrototypeRealm>::default();
        let _binding = realm.bind();
        let parent = Rc::new(Parent::new());
        let runtime = Rc::new(Runtime::default());
        runtime.now.set(1_000_000.0);
        let protocol = Rc::new(Protocol::default());
        let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), protocol.clone());
        (socket, parent, runtime, protocol)
    }
    fn run<T>(future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("future unexpectedly pending"),
        }
    }

    #[test]
    fn error_prefix_respects_dispatch_mode_even_after_disposal() {
        struct ErrorListener(Cell<usize>);
        impl WsProtocol for ErrorListener {
            fn on_error(&self, _: &BaseWebSocket, _: &str) -> Result<(), ExchangeError> {
                self.0.set(self.0.get() + 1);
                Ok(())
            }
        }
        for grouped in [false, true] {
            for disposed in [false, true] {
                let (_, parent, runtime, _) = fixture();
                let listener = Rc::new(ErrorListener(Cell::new(0)));
                let socket = BaseWebSocket::new(1, parent.clone(), runtime, listener.clone());
                socket.grouped_listener_dispatch.set(grouped);
                parent.disposed.set(disposed);
                if disposed {
                    socket.dispose();
                }
                socket.handle_web_socket_error("transport failed").unwrap();
                assert_eq!(listener.0.get(), usize::from(grouped));
                assert_eq!(parent.logs.borrow().len(), usize::from(!disposed));
            }
        }
    }

    #[test]
    fn raw_last_received_preserves_native_bits() {
        let (socket, parent, runtime, _) = fixture();
        assert_eq!(socket.last_received().to_bits(), 0);
        // Fixed IEEE-754 witnesses: both zeros, NaN payloads, infinities,
        // a negative finite value, a subnormal and a fractional timestamp.
        let cases = [
            0x8000_0000_0000_0000,
            0x7ff8_0000_0000_0042,
            0xfff8_0000_0000_1234,
            0x7ff0_0000_0000_0000,
            0xfff0_0000_0000_0000,
            0xbff0_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x412e_8480_8000_0000,
            0x0000_0000_0000_0000,
        ];
        for bits in cases {
            socket.set_last_received(f64::from_bits(bits));
            assert_eq!(
                socket.last_received().to_bits(),
                bits,
                "raw lastReceived bits"
            );
        }
        assert_eq!(
            parent.sync.get(),
            0,
            "assignment must not synchronize health"
        );
        assert!(
            parent.events.borrow().is_empty(),
            "assignment must not emit"
        );
        assert!(
            runtime.jobs.borrow().is_empty(),
            "assignment must not schedule"
        );
    }

    #[test]
    fn raw_last_received_drives_health_timestamp_and_reset() {
        let (socket, _, runtime, _) = fixture();
        runtime.socket.ready.set(1);
        run(socket.create_websocket("wss://fixture".into(), vec![], None)).unwrap();
        runtime.now.set(1_000_000.5);
        // Expected normalized timestamps are independent of the raw getter.
        let cases = [
            (1_000_000.25_f64, Some(1_000_000.25_f64), true),
            (0.0, None, false),
            (-0.0, None, false),
            (-1.0, None, false),
            (f64::NAN, None, false),
            (f64::NEG_INFINITY, None, false),
            (f64::INFINITY, Some(f64::INFINITY), true),
            (999_999.75, Some(999_999.75), true),
            (0.0, None, false),
        ];
        for (raw, expected, healthy) in cases {
            socket.set_last_received(raw);
            let expected_bits = expected.map(f64::to_bits);
            assert_eq!(
                socket.last_message_timestamp().map(f64::to_bits),
                expected_bits,
                "lastMessageTimestamp must observe source lastReceived"
            );
            assert_eq!(
                socket
                    .diagnostics()
                    .last_message_timestamp
                    .map(f64::to_bits),
                expected_bits,
                "health timestamp must observe source lastReceived"
            );
            assert_eq!(socket.is_healthy(), healthy, "health must observe reset");
        }
        socket.dispose();
    }

    #[test]
    fn close_current_websocket_retains_native_superclass_state() {
        let (socket, _, runtime, _) = fixture();
        socket.close_current_websocket();
        run(socket.create_websocket("wss://fixture".into(), vec![], None)).unwrap();
        let generation = socket.state.borrow().generation;
        let owned = socket.owned_tasks.len();
        assert!(owned > 0);
        socket.state.borrow_mut().timer_ids.insert(Timer::Ping, 42);
        socket.close_current_websocket();
        assert_eq!(runtime.socket.closed.get(), 1);
        assert_eq!(socket.state.borrow().generation, generation);
        assert_eq!(socket.state.borrow().timer_ids.get(&Timer::Ping), Some(&42));
        assert_eq!(socket.owned_tasks.len(), owned);
        assert!(socket.has_websocket());
        assert!(!socket.is_force_closed());
        assert!(Rc::ptr_eq(
            socket.state.borrow().native.as_ref().unwrap(),
            &(runtime.socket.clone() as Rc<dyn WsSocket>)
        ));
    }

    #[test]
    fn close_current_websocket_retains_relay_superclass_state() {
        let (socket, runtime, host) = managed_relay_fixture();
        runtime.pump();
        let relay = socket.state.borrow().relay.clone().unwrap();
        let generation = socket.state.borrow().generation;
        let owned = socket.owned_tasks.len();
        let receiver = socket.state.borrow().relay_task;
        let io = host.sockets.borrow().last().unwrap().clone();
        let closed = io.closed.get();
        socket.close_current_websocket();
        assert!(io.closed.get() > closed);
        assert_eq!(socket.state.borrow().generation, generation);
        assert_eq!(socket.state.borrow().relay_task, receiver);
        assert_eq!(socket.owned_tasks.len(), owned);
        assert!(!socket.is_force_closed());
        assert!(Rc::ptr_eq(
            socket.state.borrow().relay.as_ref().unwrap(),
            &relay
        ));
    }

    #[test]
    fn heartbeat_is_protocol_aware_and_never_substring_matching() {
        for s in [
            " ping ",
            "PONG",
            r#"{"op":"server.ping"}"#,
            r#"{"ping":null}"#,
        ] {
            assert!(is_heartbeat_frame(s), "{s}");
        }
        for s in [
            r#"{"op":"subscribe","symbol":"SPONGE"}"#,
            r#"{"id":"ping-order"}"#,
            r#"["ping"]"#,
            "pinging",
            r#"{"op":" server.ping "}"#,
        ] {
            assert!(!is_heartbeat_frame(s), "{s}");
        }
    }
    #[test]
    fn send_drops_only_heartbeats_and_disposal_never_reconnects() {
        let (s, p, _, _) = fixture();
        let io = Rc::new(Socket::default());
        s.state.borrow_mut().native = Some(io.clone());
        s.send("ping").unwrap();
        assert_eq!(io.closed.get(), 0);
        s.send(r#"{"symbol":"SPONGE"}"#).unwrap();
        assert_eq!(io.closed.get(), 1);
        p.disposed.set(true);
        s.send("subscribe").unwrap();
        assert_eq!(io.closed.get(), 1);
        io.ready.set(1);
        s.send("data").unwrap();
        assert!(io.sent.borrow().is_empty());
    }
    #[test]
    fn healthy_traffic_resets_only_streak_counters_and_preserves_durable_close() {
        let (s, _, r, _) = fixture();
        let io = Rc::new(Socket::default());
        s.state.borrow_mut().native = Some(io);
        s.record_close_diagnostics(Some(1013), false);
        {
            let mut state = s.state.borrow_mut();
            state.on_close_count = 101;
            state.reconnect_count = 101;
            state.backoff_attempts = 8;
            state.close_failures = 8;
            state.pause_until = r.now.get() + 120000.0;
        }
        s.track_last_received().unwrap();
        let d = s.diagnostics();
        assert_eq!((d.on_close_count, d.reconnect_count), (0, 0));
        assert_eq!(d.lifetime_close_count, 1);
        assert_eq!(d.last_close_code, Some(1013));
        assert!(!s.is_reconnect_pause_active());
    }
    #[test]
    fn backoff_resets_on_code_change_stale_window_normal_close_and_traffic() {
        let (s, _, r, _) = fixture();
        assert_eq!(s.get_reconnect_delay(Some(1006)), 2000.0);
        assert_eq!(s.get_reconnect_delay(Some(1006)), 4000.0);
        assert_eq!(s.get_reconnect_delay(Some(1013)), 2000.0);
        r.now.set(r.now.get() + 60001.0);
        assert_eq!(s.get_reconnect_delay(Some(1013)), 2000.0);
        assert_eq!(s.get_reconnect_delay(Some(1000)), 1000.0);
        for _ in 0..8 {
            s.register_close_result(Some(1006));
        }
        assert_eq!(s.reconnect_pause_remaining_ms(), 120000.0);
        s.track_last_received().unwrap();
        assert!(!s.is_reconnect_pause_active());
        assert_eq!(s.get_reconnect_delay(Some(1006)), 2000.0);
    }
    #[test]
    fn stats_reconcile_torn_down_topics_without_losing_bare_pong() {
        let (s, _, r, _) = fixture();
        s.message_handlers.borrow_mut().insert(
            "book.BTC".into(),
            Rc::new(RefCell::new(vec![Rc::new(|_| Ok(()))])),
        );
        s.record_message_handler_stats("book.BTC").unwrap();
        s.record_message_handler_stats("pong").unwrap();
        s.message_handlers.borrow_mut().shift_remove("book.BTC");
        r.now.set(r.now.get() + 5000.0);
        s.record_message_handler_stats("pong").unwrap();
        let stats = s.message_handlers_stats.borrow();
        assert_eq!(
            stats["book.BTC"].last_callbacks_count,
            safe_cex_num::PrototypeScalar::from(Some(0))
        );
        assert_eq!(
            stats["pong"].last_callbacks_count,
            safe_cex_num::PrototypeScalar::from(Some(1))
        );
        assert_eq!(stats["pong"].had_handlers, Some(false));
        drop(stats);
        s.record_message_handler_stats("publicTrade.SOLUSDT")
            .unwrap();
        assert!(
            !s.message_handlers_stats
                .borrow()
                .contains_key("publicTrade.SOLUSDT")
        );
    }
    #[test]
    fn partial_message_stats_preserve_absence_until_source_assigns_fields() {
        let (socket, _, runtime, _) = fixture();
        socket.message_handlers_stats.borrow_mut().insert(
            "partial".into(),
            MessageHandlerStat {
                last_message_timestamp: 1.0,
                ..MessageHandlerStat::default()
            },
        );
        runtime.now.set(runtime.now.get() + 5000.0);
        socket.cleanup_message_handler_stats("missing").unwrap();
        {
            let stats = socket.message_handlers_stats.borrow();
            assert_eq!(
                stats["partial"].last_callbacks_count,
                safe_cex_num::PrototypeScalar::from(None),
                "reconciliation must not pad an absent count"
            );
            assert_eq!(
                stats["partial"].had_handlers, None,
                "reconciliation must not pad an absent handler flag"
            );
        }
        socket.record_message_handler_stats("partial").unwrap();
        assert_eq!(
            socket.message_handlers_stats.borrow()["partial"].last_callbacks_count,
            safe_cex_num::PrototypeScalar::from(Some(1))
        );
        assert_eq!(
            socket.message_handlers_stats.borrow()["partial"].had_handlers,
            None,
            "existing partial topic must not acquire false"
        );
        socket.message_handlers.borrow_mut().insert(
            "partial".into(),
            Rc::new(RefCell::new(vec![Rc::new(|_| Ok(()))])),
        );
        socket.record_message_handler_stats("partial").unwrap();
        assert_eq!(
            socket.message_handlers_stats.borrow()["partial"].had_handlers,
            Some(true)
        );
    }
    #[test]
    fn forced_stats_reconciliation_has_250ms_floor() {
        let (s, _, r, _) = fixture();
        s.message_handlers
            .borrow_mut()
            .insert("a".into(), Rc::new(RefCell::new(vec![Rc::new(|_| Ok(()))])));
        s.record_message_handler_stats("a").unwrap();
        s.message_handlers.borrow_mut().clear();
        r.now.set(r.now.get() + 249.0);
        s.cleanup_message_handler_stats("missing").unwrap();
        assert_eq!(
            s.message_handlers_stats.borrow()["a"].last_callbacks_count,
            safe_cex_num::PrototypeScalar::from(Some(1))
        );
        r.now.set(r.now.get() + 1.0);
        s.cleanup_message_handler_stats("missing").unwrap();
        assert_eq!(
            s.message_handlers_stats.borrow()["a"].last_callbacks_count,
            safe_cex_num::PrototypeScalar::from(Some(0))
        );
    }
    #[test]
    fn reopen_resync_coalesces_siblings_but_not_same_socket_and_one_pending_followup() {
        let (s, p, r, protocol) = fixture();
        let sibling = BaseWebSocket::new(1, p.clone(), r.clone(), protocol);
        s.track_connected();
        sibling.track_connected();
        assert_eq!(p.snapshots.get(), 0);
        p.snapshot_gate.set(false);
        s.track_connected();
        r.pump();
        assert_eq!(p.snapshots.get(), 1);
        sibling.track_connected();
        s.track_connected();
        s.track_connected();
        r.pump();
        assert_eq!(p.snapshots.get(), 1);
        p.snapshot_gate.set(true);
        r.pump();
        r.pump();
        assert_eq!(p.snapshots.get(), 2);
        r.pump();
        assert_eq!(p.snapshots.get(), 2);
        r.advance(2000.0);
        sibling.track_connected();
        r.pump();
        assert_eq!(p.snapshots.get(), 3);
    }
    #[test]
    fn timer_tasks_discard_release_all_arms_and_preserve_replacement_slots() {
        for timer in [
            Timer::Ping,
            Timer::PongMiss,
            Timer::Receive,
            Timer::Reconnect,
        ] {
            for pending in [false, true] {
                let (socket, _, runtime, _) = fixture();
                socket.schedule(timer, 1_000.0);
                if pending {
                    runtime.pump();
                }
                let tasks = std::mem::take(&mut *runtime.jobs.borrow_mut());
                drop(tasks);
                assert!(
                    socket.state.borrow().tasks.is_empty(),
                    "discarded timer retained handle"
                );
                assert!(socket.state.borrow().timer_ids.is_empty());
                socket.schedule(timer, 1_000.0);
                let old = std::mem::take(&mut *runtime.jobs.borrow_mut());
                socket.schedule(timer, 2_000.0);
                drop(old);
                assert!(
                    socket.state.borrow().tasks.contains_key(&timer),
                    "old task removed replacement"
                );
                let replacement = std::mem::take(&mut *runtime.jobs.borrow_mut());
                drop(replacement);
                assert!(socket.state.borrow().tasks.is_empty());
            }
        }
    }
    #[test]
    fn live_reconnect_task_discard_does_not_swallow_the_next_close() {
        for pending in [false, true] {
            let (socket, _, runtime, _) = fixture();
            socket.report_callback_result(socket.on_close(Some(1006), "dropped connection"));
            assert!(socket.state.borrow().reconnecting);
            if pending {
                runtime.pump();
            }
            let tasks = std::mem::take(&mut *runtime.jobs.borrow_mut());
            drop(tasks);
            assert!(
                !socket.state.borrow().reconnecting,
                "discarded reconnect task latched live account"
            );
            socket.report_callback_result(socket.on_close(Some(1006), "second close"));
            assert!(
                socket.state.borrow().tasks.contains_key(&Timer::Reconnect),
                "second close was swallowed"
            );
            let old = std::mem::take(&mut *runtime.jobs.borrow_mut());
            socket.schedule(Timer::Reconnect, 2_000.0);
            drop(old);
            assert!(
                socket.state.borrow().reconnecting,
                "old guard cleared newer reconnect latch"
            );
        }
    }
    #[test]
    fn timer_inline_completion_and_owned_task_discard_leave_no_dead_handles() {
        let (socket, _, runtime, _) = fixture();
        runtime.inline_spawn.set(true);
        socket.schedule(Timer::Receive, 0.0);
        assert!(
            socket.state.borrow().tasks.is_empty(),
            "inline timer installed dead handle"
        );
        runtime.inline_spawn.set(false);
        for pending in [false, true] {
            socket.spawn_owned(Box::pin(std::future::pending()));
            if pending {
                runtime.pump();
            }
            let tasks = std::mem::take(&mut *runtime.jobs.borrow_mut());
            drop(tasks);
            assert!(
                socket.owned_tasks.len() == 0,
                "discarded owned task retained handle"
            );
        }
    }
    #[test]
    fn reopen_resync_task_discard_releases_account_latch_unpolled_and_pending() {
        for pending in [false, true] {
            let (socket, parent, runtime, protocol) = fixture();
            let sibling = BaseWebSocket::new(1, parent.clone(), runtime.clone(), protocol);
            socket.track_connected();
            sibling.track_connected();
            parent.snapshot_gate.set(false);
            socket.track_connected();
            if pending {
                runtime.pump();
            }
            assert_eq!(parent.snapshots.get(), 1);
            socket.track_connected(); // Queue a follow-up behind the discarded task.
            let tasks = std::mem::take(&mut *runtime.jobs.borrow_mut());
            drop(tasks); // Drop the TASK, not the initiating caller or its handle.
            assert!(
                !parent.coordinator.state.borrow().running,
                "discarded task latched running"
            );
            assert!(!parent.coordinator.state.borrow().rerun);
            runtime.advance(2_001.0);
            sibling.track_connected();
            assert_eq!(
                parent.snapshots.get(),
                2,
                "sibling resync must recover after task discard"
            );
            parent.snapshot_gate.set(true);
            runtime.pump();
            runtime.pump();
            assert!(!parent.coordinator.state.borrow().running);
            assert_eq!(
                parent.snapshots.get(),
                2,
                "discarded follow-up must not leak into replacement"
            );
        }
    }
    #[test]
    fn resync_defers_ready_failure_reactions_on_inline_hosts() {
        let (socket, parent, runtime, _) = fixture();
        runtime.inline_spawn.set(true);
        parent.snapshot_failure.set(true);
        socket.track_connected();
        socket.track_connected();
        assert_eq!(
            parent.snapshots.get(),
            1,
            "RESYNC native fetch prefix remains eager"
        );
        assert!(
            parent.logs.borrow().is_empty(),
            "RESYNC native rejection reaction waits for a scheduler turn"
        );
        runtime.pump();
        runtime.pump();
        assert_eq!(
            parent.logs.borrow().len(),
            1,
            "RESYNC native deferred logger runs once"
        );
    }

    #[test]
    fn diagnostic_cap_reports_unrepresentable_materialized_unit() {
        let (socket, parent, _, _) = fixture();
        let message = format!("{}💩tail", "a".repeat(4095));
        let error = socket
            .emit_ws_diagnostic_event("grafana-pws", &message)
            .expect_err("CAP native split surrogate must report representation refusal");
        assert_eq!(
            error.message,
            "WebSocket diagnostic cap contains an unpaired UTF-16 surrogate"
        );
        assert!(
            parent.events.borrow().is_empty(),
            "CAP native refusal emits no replacement diagnostic"
        );
    }

    #[test]
    fn snapshot_failure_logs_and_disposal_suppresses_queued_followup() {
        let (s, p, r, _) = fixture();
        p.snapshot_failure.set(true);
        p.snapshot_gate.set(false);
        s.track_connected();
        s.track_connected();
        r.pump();
        s.track_connected();
        p.disposed.set(true);
        p.snapshot_gate.set(true);
        r.pump();
        // The rejection reaction waits one scheduler turn, as in source.
        r.pump();
        assert_eq!(p.snapshots.get(), 1);
        assert!(
            p.logs
                .borrow()
                .iter()
                .any(|(s, _)| s.contains("Private resync after re-open failed"))
        );
    }
    fn resync_log_failure_rows() -> Vec<Value> {
        let mut rows = vec![];
        for disposed in [false, true] {
            let (socket, parent, runtime, _) = fixture();
            let mut expected = ExchangeError::new("resync log listener failed");
            expected.name = "TypeError".into();
            expected.code = Some(crate::js_runtime::value::Js::str("E_LISTENER"));
            *parent.log_error.borrow_mut() = Some(expected.clone());
            parent.snapshot_failure.set(true);
            parent.snapshot_gate.set(false);
            socket.track_connected();
            socket.track_connected();
            runtime.pump();
            socket.track_connected();
            assert!(
                parent.coordinator.state.borrow().rerun,
                "fixture must queue the follow-up before logger failure"
            );
            // The first future already captured its rejection; the queued run succeeds.
            parent.snapshot_failure.set(false);
            parent.disposed.set(disposed);
            parent.snapshot_gate.set(true);
            runtime.pump();
            runtime.pump();
            assert_eq!(
                *runtime.exceptions.borrow(),
                [expected.clone()],
                "resync listener error must remain observable and retain identity metadata"
            );
            assert_eq!(
                parent.snapshots.get(),
                if disposed { 1 } else { 2 },
                "source finally must retain queued resync after logger throw unless disposed"
            );
            runtime.pump();
            assert!(
                !parent.coordinator.state.borrow().running,
                "settled resync must release its latch"
            );
            assert!(!parent.coordinator.state.borrow().rerun);
            rows.push(json!({"disposed":disposed,"fetches":parent.snapshots.get(),"running":parent.coordinator.state.borrow().running,"error":{"name":expected.name,"message":expected.message}}));
            if !disposed {
                parent.log_error.borrow_mut().take();
                runtime.advance(2_001.0);
                socket.track_connected();
                runtime.pump();
                assert_eq!(
                    parent.snapshots.get(),
                    3,
                    "a later resync must recover after listener failure"
                );
            }
        }
        rows
    }
    #[test]
    fn resync_log_failure_preserves_finally_rerun_and_later_reentry() {
        assert_eq!(resync_log_failure_rows().len(), 2);
    }
    #[test]
    #[ignore = "mandatory core include-ignored CI executes actual runReopenResync"]
    fn resync_logger_throw_matches_actual_source_finally() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/listener-oracle.cjs"
            ))
            .arg("--resync")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "resync source oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            resync_log_failure_rows(),
            expected,
            "actual source finally behavior after logger throw"
        );
    }
    #[test]
    fn auth_failure_sends_nothing_disregards_silence_and_never_auth_invalid_phrase() {
        let (s, p, _, _) = fixture();
        let io = Rc::new(Socket::default());
        io.ready.set(1);
        s.state.borrow_mut().native = Some(io.clone());
        let called = Cell::new(false);
        run(s.authenticate_and_subscribe(
            Box::pin(async { Err(ExchangeError::new("remote signer unavailable")) }),
            |_| {
                called.set(true);
                Ok(())
            },
        ))
        .unwrap();
        assert!(!called.get());
        assert_eq!(io.closed.get(), 1);
        assert!(io.sent.borrow().is_empty());
        assert!(
            !p.logs
                .borrow()
                .iter()
                .any(|(message, _)| message.contains("auth failed"))
        );
        let mut fallback = s.direct_fallback.borrow_mut();
        assert_eq!(fallback.resolve_url("direct", &[Some("relay")]), "direct");
        assert_eq!(fallback.resolve_url("direct", &[Some("relay")]), "direct");
    }
    #[test]
    fn auth_await_disposal_blocks_subscribe() {
        let (s, p, _, _) = fixture();
        let gate = Rc::new(Cell::new(false));
        let g = gate.clone();
        let called = Cell::new(false);
        let mut future = std::pin::pin!(s.authenticate_and_subscribe(
            Box::pin(std::future::poll_fn(move |_| if g.get() {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            })),
            |_| {
                called.set(true);
                Ok(())
            }
        ));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut cx).is_pending());
        p.disposed.set(true);
        gate.set(true);
        assert!(future.as_mut().poll(&mut cx).is_ready());
        assert!(!called.get());
    }
    #[test]
    fn watchdog_pings_before_reconnect_and_any_message_only_cancels_miss() {
        let (s, _, r, protocol) = fixture();
        let io = Rc::new(Socket::default());
        s.state.borrow_mut().native = Some(io.clone());
        s.track_last_received().unwrap();
        r.advance(6001.0);
        assert_eq!(protocol.pings.get(), 1);
        r.advance(14000.0);
        assert_eq!(io.closed.get(), 1);
        s.treat_any_message_as_pong.set(true);
        s.handle_pong_event(5000).unwrap();
        assert!(s.state.borrow().tasks.contains_key(&Timer::Ping));
        assert!(!s.state.borrow().tasks.contains_key(&Timer::PongMiss));
    }
    #[test]
    fn disposal_cancels_pending_native_reconnect_and_ping_timers() {
        let (s, _, r, protocol) = fixture();
        s.on_close(Some(1006), "bad").unwrap();
        s.handle_pong_event(5000).unwrap();
        s.dispose();
        r.advance(100000.0);
        assert_eq!(protocol.connects.get(), 0);
        assert_eq!(protocol.pings.get(), 0);
        assert!(s.is_force_closed());
    }
    #[test]
    fn diagnostics_are_opt_in_redacted_rate_limited_and_utf16_capped() {
        let (s, p, r, _) = fixture();
        s.emit_ws_diagnostic_send(
            r#"{"token":"secret-value"}"#,
            &WsDiagnosticSendOptions::default(),
        )
        .unwrap();
        assert!(p.events.borrow().is_empty());
        r.diagnostics.set(true);
        s.emit_ws_diagnostic_send("ping", &WsDiagnosticSendOptions::default())
            .unwrap();
        for _ in 0..201 {
            s.emit_ws_diagnostic_send(
                r#"{"op":"login","args":["sensitive"],"key":"api-key"}"#,
                &WsDiagnosticSendOptions::default(),
            )
            .unwrap();
        }
        assert_eq!(p.events.borrow().len(), 200);
        assert!(
            p.events
                .borrow()
                .iter()
                .all(|(_, m)| !m.contains("sensitive") && !m.contains("api-key"))
        );
        r.advance(1000.0);
        s.emit_ws_diagnostic_send(&"😀".repeat(3000), &WsDiagnosticSendOptions::default())
            .unwrap();
        let events = p.events.borrow();
        assert_eq!(events.len(), 202);
        assert!(events[200].1.contains("suppressed 1"));
        assert!(events[201].1.ends_with("...truncated (6000 chars)"));
    }
    #[test]
    fn diagnostics_format_order_and_custom_null_redaction_match_ts() {
        let (s, p, r, _) = fixture();
        r.diagnostics.set(true);
        s.emit_ws_diagnostic_send(
            r#"{"token":"SECRET"}"#,
            &WsDiagnosticSendOptions {
                format: Some(Rc::new(|redacted| {
                    assert!(!redacted.contains("SECRET"));
                    Some(redacted.into())
                })),
                redact: Some(Rc::new(|_| None)),
            },
        )
        .unwrap();
        s.emit_ws_diagnostic_receive(
            &Value::String(r#"{"key":"SECRET"}"#.into()),
            &WsDiagnosticReceiveOptions {
                format: Some(Rc::new(|raw| {
                    assert!(stringify(raw).contains("SECRET"));
                    Some(stringify(raw))
                })),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.events.borrow().len(), 2);
        assert!(p.events.borrow().iter().all(|(_, m)| !m.contains("SECRET")));
    }
    #[test]
    fn required_headers_fail_before_dial_and_fresh_provider_is_retained() {
        let (s, _, r, _) = fixture();
        let error = run(s.create_websocket(
            "wss://venue".into(),
            vec![],
            Some(WebSocketConnectionOptions {
                headers_required: true,
                ..Default::default()
            }),
        ))
        .unwrap_err();
        assert_eq!(error.name, "HeaderCapableWebSocketUnavailableError");
        assert!(r.dial_options.borrow().is_empty());
        r.headers.set(true);
        let counter = Rc::new(Cell::new(0));
        let c = counter.clone();
        run(s.create_websocket(
            "wss://venue".into(),
            vec![],
            Some(WebSocketConnectionOptions {
                headers: Some(UpgradeHeaders::Fresh(Rc::new(move || {
                    c.set(c.get() + 1);
                    [("x-token".into(), c.get().to_string())]
                        .into_iter()
                        .collect()
                }))),
                headers_required: true,
                ..Default::default()
            }),
        ))
        .unwrap();
        let options = r.dial_options.borrow();
        let Some(UpgradeHeaders::Fresh(provider)) = &options[0].headers else {
            panic!("fresh provider must be retained")
        };
        assert_eq!(provider()["x-token"], "1");
        assert_eq!(provider()["x-token"], "2");
    }
    #[test]
    fn dispatch_exact_key_records_subscription_stats_and_allows_self_teardown() {
        let (s, _, _, _) = fixture();
        let weak = Rc::downgrade(&s);
        let calls = Rc::new(Cell::new(0));
        let c = calls.clone();
        s.message_handlers.borrow_mut().insert(
            "book.BTC".into(),
            Rc::new(RefCell::new(vec![Rc::new(move |_| {
                c.set(c.get() + 1);
                weak.upgrade()
                    .unwrap()
                    .message_handlers
                    .borrow_mut()
                    .shift_remove("book.BTC");
                Ok(())
            })])),
        );
        let c = calls.clone();
        s.message_handlers.borrow()["book.BTC"]
            .borrow_mut()
            .push(Rc::new(move |_| {
                c.set(c.get() + 1);
                Ok(())
            }));
        s.dispatch_message("book.ETH", &crate::js_runtime::value::Js::Null)
            .unwrap();
        assert_eq!(calls.get(), 0);
        s.dispatch_message("book.BTC", &crate::js_runtime::value::Js::Null)
            .unwrap();
        assert_eq!(calls.get(), 2);
        assert_eq!(
            s.message_handlers_stats.borrow()["book.BTC"].had_handlers,
            Some(true)
        );
    }
    #[test]
    fn handler_prefix_iteration_preserves_registration_order_after_delete_and_reinsert() {
        let (socket, _, _, _) = fixture();
        let keys = [
            "kline.5m.BTC",
            "kline.1m.ETH",
            "kline.15m.SOL",
            "kline.1m.BTC",
        ];
        for key in keys {
            socket
                .message_handlers
                .borrow_mut()
                .insert(key.into(), Rc::new(RefCell::new(vec![])));
        }
        let observed = || {
            socket
                .message_handlers
                .borrow()
                .keys()
                .filter(|key| key.starts_with("kline."))
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            observed(),
            keys,
            "prefix fallback must use registration order, not sorted or hashed order"
        );
        // Replacing an existing JS object property does not move it.
        socket
            .message_handlers
            .borrow_mut()
            .insert(keys[1].into(), Rc::new(RefCell::new(vec![])));
        assert_eq!(observed(), keys);
        socket.message_handlers.borrow_mut().shift_remove(keys[0]);
        assert_eq!(
            observed(),
            keys[1..],
            "removing a handler must preserve surviving prefix order"
        );
        socket
            .message_handlers
            .borrow_mut()
            .insert(keys[0].into(), Rc::new(RefCell::new(vec![])));
        assert_eq!(
            observed(),
            [keys[1], keys[2], keys[3], keys[0]],
            "reinserted handler must move to the end"
        );
    }

    #[test]
    fn dispatch_error_stops_captured_list_after_stats_and_releases_handlers() {
        let (socket, _, _, _) = fixture();
        let weak = Rc::downgrade(&socket);
        let later_calls = Rc::new(Cell::new(0));
        let retained = Rc::new(());
        let retained_weak = Rc::downgrade(&retained);
        let expected = ExchangeError::new("cannot convert object to primitive value");
        let thrown = expected.clone();
        let list: HandlerList = Rc::new(RefCell::new(vec![Rc::new(move |_| {
            let _retained = &retained;
            let socket = weak.upgrade().unwrap();
            assert!(
                socket.message_handlers_stats.borrow()["ticker"].had_handlers == Some(true),
                "matched-key stats must precede the throwing callback"
            );
            // Removing the key must not invalidate the captured list or borrow.
            socket.message_handlers.borrow_mut().shift_remove("ticker");
            Err(thrown.clone())
        })]));
        let calls = later_calls.clone();
        list.borrow_mut().push(Rc::new(move |_| {
            calls.set(calls.get() + 1);
            Ok(())
        }));
        socket
            .message_handlers
            .borrow_mut()
            .insert("ticker".into(), list);
        socket
            .message_handlers
            .borrow_mut()
            .insert("empty".into(), Rc::new(RefCell::new(vec![])));
        assert_eq!(
            socket.dispatch_message("absent", &crate::js_runtime::value::Js::Null),
            Ok(())
        );
        assert_eq!(
            socket.dispatch_message("empty", &crate::js_runtime::value::Js::Null),
            Ok(())
        );
        assert!(socket.message_handlers_stats.borrow().is_empty());
        let result = socket.dispatch_message("ticker", &crate::js_runtime::value::Js::Null);
        assert_eq!(
            later_calls.get(),
            0,
            "source throw must stop later callbacks"
        );
        assert_eq!(
            result,
            Err(expected),
            "dispatch must retain the original handler error"
        );
        assert_eq!(socket.message_handlers_stats.borrow().len(), 1);
        assert!(
            retained_weak.upgrade().is_none(),
            "captured handler must release after failing dispatch"
        );
        socket.dispose();
        drop(socket);
    }

    #[test]
    fn close_error_logs_dedupe_signatures_with_independent_windows() {
        let (s, _, r, _) = fixture();
        assert!(
            s.get_close_log_message(Some(1006), "one", "Abnormal closure")
                .is_some()
        );
        assert!(
            s.get_close_log_message(Some(1006), "different reason", "Abnormal closure")
                .is_none()
        );
        assert!(
            s.get_web_socket_error_log_message("Connection refused")
                .is_some()
        );
        assert!(
            s.get_web_socket_error_log_message("CONNECTION REFUSED")
                .is_none()
        );
        r.advance(15000.0);
        assert!(
            s.get_close_log_message(Some(1006), "", "Abnormal closure")
                .unwrap()
                .contains("suppressed 1 duplicate close events")
        );
        assert!(
            s.get_web_socket_error_log_message("Connection refused")
                .unwrap()
                .contains("suppressed 1 duplicate ws error events")
        );
    }
    #[test]
    fn prototype_stats_error_stops_dispatch_before_callbacks() {
        let (socket, _, _, _) = fixture();
        let called = Rc::new(Cell::new(false));
        let capture = called.clone();
        let handler: MessageHandler = Rc::new(move |_| {
            capture.set(true);
            Ok(())
        });
        socket
            .message_handlers
            .borrow_mut()
            .insert("__proto__".into(), Rc::new(RefCell::new(vec![handler])));
        let error = socket
            .dispatch_message("__proto__", &crate::js_runtime::value::Js::Null)
            .unwrap_err();
        assert_eq!(
            error.name, "TypeError",
            "stats throw must remain caller-visible"
        );
        assert!(
            !called.get(),
            "stats throw must stop dispatch before callbacks"
        );
    }
    #[test]
    #[ignore = "requires the live TypeScript compiler; CI runs --include-ignored"]
    fn live_ws_typescript_oracle() {
        let output = std::process::Command::new("node")
            .arg("scripts/ws-oracle.mjs")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("Node required for live WS oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Value = serde_json::from_slice(&output.stdout).expect("oracle JSON");
        for (group, count) in [
            ("retryBoundaries", 32),
            ("optionTuning", 3),
            ("reconnectThrows", 2),
            ("resyncErrors", 6),
            ("heartbeats", 12),
            ("closeCodes", 16),
            ("redactions", 102),
        ] {
            assert_eq!(
                expected[group].as_array().unwrap().len(),
                count,
                "WS-CARDINALITY source group: {group}"
            );
        }
        assert_eq!(
            expected["optionTuning"]
                .as_array()
                .unwrap()
                .iter()
                .map(|case| case["delays"].as_array().unwrap().len())
                .sum::<usize>(),
            9,
            "WS-CARDINALITY source total delays"
        );
        let cases = expected["inheritedStats"].as_array().unwrap();
        assert_eq!(
            cases.len(),
            78,
            "live prototype corpus must cover twelve members plus a miss"
        );
        for case in cases {
            let (socket, _, runtime, _) = fixture();
            let topic = case["topic"].as_str().unwrap();
            let original_referent = socket.prototype_realm.inherited(topic);
            let prototype_throw = topic == "__proto__"
                && !case["ownStats"].as_bool().unwrap()
                && case["ownHandlers"].as_u64() != Some(0);
            for field in ["recordError", "reconcileError"] {
                assert_eq!(
                    case[field]["name"].as_str(),
                    prototype_throw.then_some("TypeError"),
                    "actual source prototype mutation throw profile: {case}"
                );
            }

            if let Some(count) = case["ownHandlers"].as_u64() {
                let handlers: Vec<MessageHandler> = (0..count)
                    .map(|_| Rc::new(|_: &crate::js_runtime::value::Js| Ok(())) as MessageHandler)
                    .collect();
                socket
                    .message_handlers
                    .borrow_mut()
                    .insert(topic.into(), Rc::new(RefCell::new(handlers)));
            }
            if case["ownStats"].as_bool().unwrap() {
                socket.message_handlers_stats.borrow_mut().insert(
                    topic.into(),
                    MessageHandlerStat {
                        last_message_timestamp: 1.0,
                        last_callbacks_count: safe_cex_num::PrototypeScalar::Undefined,
                        had_handlers: Some(true),
                        had_handlers_undefined: false,
                    },
                );
            }
            runtime
                .now
                .set(case["recorded"]["timestamp"].as_f64().unwrap());
            let recorded_time = runtime.now.get();
            let result = socket.record_message_handler_stats(topic);
            assert_eq!(
                result.as_ref().err().map(|error| error.name.as_str()),
                case["recordError"]["name"].as_str(),
                "source prototype reconciliation throw must be a caller-visible error: {case}"
            );
            assert_eq!(
                result.err().map(|error| error.message),
                case["recordError"]["message"].as_str().map(str::to_owned),
                "source prototype reconciliation error message must be retained: {case}"
            );
            for stage in ["recorded", "reconciled"] {
                if stage == "reconciled" {
                    socket.message_handlers.borrow_mut().shift_remove(topic);
                    runtime.now.set(runtime.now.get() + 5000.0);
                    let reconciled =
                        socket.reconcile_message_handler_stats(runtime.now.get(), false);
                    assert_eq!(
                        reconciled.as_ref().err().map(|e| e.name.as_str()),
                        case["reconcileError"]["name"].as_str(),
                        "native later prototype reconciliation error name"
                    );
                    assert_eq!(
                        reconciled.err().map(|e| e.message),
                        case["reconcileError"]["message"]
                            .as_str()
                            .map(str::to_owned),
                        "native later prototype reconciliation error message"
                    );
                }
                let stats = socket.message_handlers_stats.borrow();
                let entry = stats.get(topic);
                assert_eq!(
                    entry.is_some(),
                    case[stage]["own"].as_bool().unwrap(),
                    "inherited stats must not become own records: {case}"
                );
                if let Some(entry) = entry {
                    assert_eq!(
                        entry.last_callbacks_count,
                        safe_cex_num::PrototypeScalar::from(
                            case[stage]["count"].as_u64().map(|count| count as usize)
                        ),
                        "stats must retain inherited handler arity including undefined: {stage} {case}"
                    );
                    assert_eq!(
                        entry.had_handlers,
                        case[stage]["had"].as_bool(),
                        "stats must retain inherited handler presence: {stage} {case}"
                    );
                    assert_eq!(
                        entry.last_message_timestamp,
                        case[stage]["timestamp"].as_f64().unwrap()
                    );
                } else {
                    // Supplemental source-fed comparison; the independently authored
                    // native-first sequences live in the Binance prototype tests.
                    let safe_cex_num::PrototypeValue::Reference(native) =
                        socket.prototype_realm.inherited(topic)
                    else {
                        panic!("native inherited statistics retain a reference")
                    };
                    let safe_cex_num::PrototypeValue::Reference(original) = &original_referent
                    else {
                        panic!("native initial inherited statistics are a reference")
                    };
                    assert_eq!(&native, original, "native inherited statistics identity");
                    assert!(
                        matches!(native.get("lastMessageTimestamp"),
                            safe_cex_num::PrototypeValue::Number(value) if value == recorded_time),
                        "native inherited statistics timestamp before source comparison"
                    );
                    assert_eq!(
                        case[stage]["type"],
                        if topic == "__proto__" {
                            "object"
                        } else {
                            "function"
                        },
                        "source stats mutation must retain inherited value identity: {case}"
                    );
                    assert!(
                        case[stage]["timestamp"].is_number(),
                        "source inherited referent mutation must remain witnessed: {case}"
                    );
                }
            }
            let inherited_fields = match &original_referent {
                safe_cex_num::PrototypeValue::Reference(value) => Some(value.own_fields()),
                _ => None,
            };
            runtime.now.set(runtime.now.get() + 250.0);
            let cleanup = socket.cleanup_message_handler_stats(topic);
            assert!(
                !socket.message_handlers_stats.borrow().contains_key(topic),
                "STATS-NATIVE-CLEANUP must remove own record before a throw"
            );
            assert_eq!(
                cleanup.as_ref().err().map(|error| error.name.as_str()),
                prototype_throw.then_some("TypeError"),
                "native cleanup prototype error profile"
            );
            if let safe_cex_num::PrototypeValue::Reference(original) = &original_referent {
                let safe_cex_num::PrototypeValue::Reference(retained) =
                    socket.prototype_realm.inherited(topic)
                else {
                    panic!("native cleanup must retain inherited referent")
                };
                assert_eq!(&retained, original, "native cleanup inherited identity");
                assert_eq!(
                    Some(retained.own_fields()),
                    inherited_fields,
                    "native cleanup preserves inherited fields"
                );
            }
            assert_eq!(
                case["cleaned"]["own"], false,
                "source cleanup own-record observation"
            );
            assert_eq!(
                cleanup.as_ref().err().map(|error| error.name.as_str()),
                case["cleanupError"]["name"].as_str(),
                "native/source cleanup error name"
            );
            assert_eq!(
                cleanup.err().map(|error| error.message),
                case["cleanupError"]["message"].as_str().map(str::to_owned),
                "native/source cleanup error message"
            );
        }
        for index in 0..32 {
            let case = &expected["retryBoundaries"][index];
            assert_eq!(
                !relay_retry_exhausted(
                    case["prior"].as_i64().unwrap() as i32 + 1,
                    case["limit"].as_f64().unwrap()
                ),
                case["attempt"].as_bool().unwrap(),
                "retry limits must match live RWS previous-counter comparison: {case}"
            );
        }
        for index in 0..3 {
            let case = &expected["optionTuning"][index];
            let (socket, _, runtime, _) = fixture();
            let source = &case["options"];
            let options = (!source.is_null()).then(|| WebSocketConnectionOptions {
                min_reconnection_delay_ms: source["minReconnectionDelay"].as_f64(),
                max_reconnection_delay_ms: source["maxReconnectionDelay"].as_f64(),
                reconnection_delay_grow_factor: source["reconnectionDelayGrowFactor"].as_f64(),
                ..Default::default()
            });
            run(socket.create_websocket("wss://fixture".into(), vec![], options)).unwrap();
            let options = runtime.dial_options.borrow().last().unwrap().clone();
            let delays = case["delays"].as_array().unwrap();
            assert_eq!(delays.len(), 3, "WS-CARDINALITY source per-case delays");
            let mut comparisons = 0;
            for (index, failures) in [0, 1, 7].into_iter().enumerate() {
                let expected = &delays[index];
                assert_eq!(
                    relay_reconnection_delay(
                        &options,
                        runtime.library_min_reconnection_delay_ms(),
                        failures
                    ),
                    expected.as_f64().unwrap(),
                    "omitted/explicit options must match live base plus RWS tuning: {case}"
                );
                comparisons += 1;
            }
            assert_eq!(comparisons, 3, "WS-CARDINALITY native delay comparisons");
            socket.dispose();
        }
        for index in 0..2 {
            let case = &expected["reconnectThrows"][index];
            let (socket, parent, runtime, protocol) = fixture();
            let message = "same failure";
            let transport_first = case["transportFirst"].as_bool().unwrap();
            if transport_first {
                socket.report_callback_result(socket.handle_web_socket_error(message));
            }
            let mut error = ExchangeError::new(message);
            error.name = "ReconnectHookError".into();
            *protocol.connect_error.borrow_mut() = Some(error);
            socket.report_callback_result(socket.on_close(Some(1000), ""));
            runtime.pump();
            runtime.advance(1000.0);
            let reports: Vec<String> = runtime
                .exceptions
                .borrow()
                .iter()
                .map(ExchangeError::js_error_string)
                .collect();
            assert_eq!(
                json!(reports),
                case["thrown"],
                "reconnect hook must reach host even after transport error"
            );
            assert_eq!(
                parent
                    .logs
                    .borrow()
                    .iter()
                    .filter(|(_, severity)| *severity == LogSeverity::Error)
                    .count(),
                usize::from(transport_first),
                "hook failure must not produce a transport error log"
            );
            if !transport_first {
                socket.report_callback_result(socket.handle_web_socket_error(message));
            }
            let logs: Vec<String> = parent
                .logs
                .borrow()
                .iter()
                .filter(|(_, severity)| *severity == LogSeverity::Error)
                .map(|(message, _)| message.clone())
                .collect();
            assert_eq!(
                json!(logs),
                case["logs"],
                "transport error must survive preceding reconnect hook failure"
            );
            assert_eq!(
                socket.state.borrow().error_log.suppressed,
                0,
                "reconnect hook must not consume transport dedup in either direction"
            );
            assert_eq!(
                socket.state.borrow().reconnecting,
                case["reconnecting"].as_bool().unwrap(),
                "thrown hook must skip source post-hook latch reset"
            );
            assert_eq!(
                case["reconnectListeners"], 0,
                "source throw must skip listener registration"
            );
            assert!(!socket.state.borrow().register_reconnect_open);
            assert!(
                socket
                    .state
                    .borrow()
                    .reconnect_listener_generation
                    .is_none(),
                "thrown hook must not install a post-hook listener"
            );
            assert_eq!(
                socket.state.borrow().last_close_time,
                0.0,
                "thrown hook must not advance reconnect completion time"
            );
        }
        let (socket, parent, runtime, protocol) = fixture();
        socket.report_callback_result(socket.on_close(Some(1000), ""));
        // An in-flight pong can re-arm timers after the first close cleared them.
        socket.handle_pong_event(5000).unwrap();
        socket.report_callback_result(socket.on_close(Some(1000), ""));
        runtime.pump();
        runtime.advance(5000.0);
        socket.state.borrow_mut().native = Some(runtime.socket.clone());
        runtime.advance(2500.0);
        assert_eq!(
            socket.state.borrow().reconnect_count,
            expected["duplicateNativeClose"]["reconnectCount"]
                .as_u64()
                .unwrap() as usize,
            "TS duplicate-close pong miss consumes an explicit reconnect attempt"
        );
        let misses: Vec<String> = parent
            .events
            .borrow()
            .iter()
            .filter(|(event, message)| event == "grafana-error" && message.contains("pong miss"))
            .map(|(_, message)| message.clone())
            .collect();
        assert_eq!(
            json!(misses),
            expected["duplicateNativeClose"]["misses"],
            "duplicate native close must preserve TS pong-miss timer"
        );
        assert_eq!(
            protocol.pings.get(),
            expected["duplicateNativeClose"]["pings"].as_u64().unwrap() as usize,
            "duplicate native close must preserve TS ping timer"
        );
        for index in 0..6 {
            let case = &expected["resyncErrors"][index];
            let (socket, parent, runtime, _) = fixture();
            let mut error = ExchangeError::new(case["message"].as_str().unwrap());
            error.name = case["name"].as_str().unwrap().into();
            *parent.snapshot_error.borrow_mut() = Some(error);
            socket.track_connected();
            socket.track_connected();
            // Match the source oracle's catch/finally microtask drain. Resync
            // deliberately yields a host turn before reporting the rejection.
            for _ in 0..8 {
                runtime.pump();
            }
            assert_eq!(parent.snapshots.get(), 1, "one re-open starts one snapshot");
            assert_eq!(parent.logs.borrow().len(), 1, "one catch reports one error");
            assert_eq!(
                parent.logs.borrow().last().unwrap().0,
                case["expected"].as_str().unwrap(),
                "resync must retain JS Error identity: {case}"
            );
        }
        for index in 0..12 {
            let case = &expected["heartbeats"][index];
            assert_eq!(
                is_heartbeat_frame(case["input"].as_str().unwrap()),
                case["expected"].as_bool().unwrap(),
                "{case}"
            );
        }
        for index in 0..16 {
            let case = &expected["closeCodes"][index];
            assert_eq!(
                get_close_code_description(case["input"].as_u64().unwrap() as u16),
                case["expected"].as_str().unwrap(),
                "{case}"
            );
        }
        for index in 0..102 {
            let case = &expected["redactions"][index];
            assert_eq!(
                redact_payload(case["input"].as_str().unwrap()),
                case["expected"].as_str().unwrap(),
                "{case}"
            );
        }
        let (socket, _, runtime, _) = fixture();
        socket.message_handlers_stats.borrow_mut().insert(
            "partial".into(),
            MessageHandlerStat {
                last_message_timestamp: 1.0,
                ..MessageHandlerStat::default()
            },
        );
        socket.message_handlers.borrow_mut().insert(
            "book.BTC".into(),
            Rc::new(RefCell::new(vec![Rc::new(|_| Ok(()))])),
        );
        socket.record_message_handler_stats("book.BTC").unwrap();
        socket.record_message_handler_stats("pong").unwrap();
        socket
            .message_handlers
            .borrow_mut()
            .shift_remove("book.BTC");
        runtime.now.set(runtime.now.get() + 5000.0);
        socket.record_message_handler_stats("pong").unwrap();
        for key in ["book.BTC", "pong", "partial"] {
            let stats = socket.message_handlers_stats.borrow();
            let actual = &stats[key];
            let expected = &expected["stats"][key];
            assert_eq!(
                actual.last_message_timestamp,
                expected["lastMessageTimestamp"].as_f64().unwrap()
            );
            assert_eq!(
                actual.last_callbacks_count,
                safe_cex_num::PrototypeScalar::from(
                    expected["lastCallbacksCount"].as_u64().map(|v| v as usize)
                )
            );
            assert_eq!(actual.had_handlers, expected["hadHandlers"].as_bool());
        }
        let actual: Vec<_> = [
            Some(1006),
            Some(1006),
            Some(1013),
            Some(1013),
            Some(1000),
            None,
            Some(1006),
        ]
        .map(|code| socket.get_reconnect_delay(code))
        .into();
        let expected: Vec<_> = expected["backoff"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_f64().unwrap())
            .collect();
        assert_eq!(actual, expected);
    }
    #[test]
    fn watchdog_log_timestamp_matches_js_iso_dates() {
        assert_eq!(iso_timestamp(0.0).unwrap(), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_timestamp(-1.0).unwrap(), "1969-12-31T23:59:59.999Z");
        assert_eq!(
            iso_timestamp(951782400123.0).unwrap(),
            "2000-02-29T00:00:00.123Z"
        );
    }
    #[test]
    fn review_managed_reconnect_can_synchronously_dispose() {
        struct Reentrant(std::rc::Weak<BaseWebSocket>);
        impl WsSocket for Reentrant {
            fn ready_state(&self) -> u8 {
                1
            }
            fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
                Box::pin(std::future::pending())
            }
            fn send(&self, _: &str) -> Result<(), ExchangeError> {
                Ok(())
            }
            fn close(&self) {}
            fn terminate(&self) {}
            fn reconnect(&self) {
                self.0.upgrade().unwrap().dispose();
            }
            fn is_reconnecting(&self) -> bool {
                true
            }
        }
        let (socket, _, runtime, _) = fixture();
        socket.state.borrow_mut().native = Some(Rc::new(Reentrant(Rc::downgrade(&socket))));
        socket.reconnect().unwrap();
        runtime.advance(1000.0);
        assert!(socket.is_force_closed());
    }
    #[test]
    fn relay_static_overlay_reaches_real_transport_with_sealed_policy_and_protocols() {
        let (socket, _, runtime, _) = fixture();
        runtime.headers.set(true);
        runtime.relaying.set(true);
        let host = Rc::new(RelayHost::new(&runtime));
        *runtime.tunnel.borrow_mut() = Some(host.clone());
        let headers = [
            ("x-auth".into(), "signed".into()),
            ("origin".into(), "caller-cannot-override".into()),
        ]
        .into_iter()
        .collect();
        run(socket.create_websocket(
            "wss://relay/tunnel?ts-relay=wss%3A%2F%2Fvenue".into(),
            vec!["feed".into()],
            Some(WebSocketConnectionOptions {
                headers: Some(UpgradeHeaders::Static(headers)),
                headers_required: true,
                ..Default::default()
            }),
        ))
        .unwrap();
        assert!(host.dials.borrow().is_empty());
        runtime.pump();
        let dials = host.dials.borrow();
        assert_eq!(dials.len(), 1);
        assert_eq!(dials[0].headers()["x-auth"], "signed");
        assert_eq!(
            dials[0].headers()["Origin"],
            "https://preview.tealstreet.io"
        );
        assert!(!dials[0].headers().contains_key("origin"));
        assert_eq!(dials[0].alpn_protocols(), ["http/1.1"]);
        assert_eq!(dials[0].protocols(), ["feed"]);
        assert!(socket.is_connected());
    }
    #[test]
    fn relay_fresh_headers_stay_lazy_and_refresh_on_each_reconnection() {
        let (socket, _, runtime, _) = fixture();
        runtime.headers.set(true);
        runtime.relaying.set(true);
        let host = Rc::new(RelayHost::new(&runtime));
        *runtime.tunnel.borrow_mut() = Some(host.clone());
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        let provider: FreshHeadersProvider = Rc::new(move || {
            count.set(count.get() + 1);
            [("x-auth".into(), format!("signed-{}", count.get()))]
                .into_iter()
                .collect()
        });
        let options = WebSocketConnectionOptions {
            headers: Some(UpgradeHeaders::Fresh(provider)),
            headers_required: true,
            ..Default::default()
        };
        run(socket.create_websocket(
            "wss://relay/tunnel?ts-relay=wss%3A%2F%2Fvenue".into(),
            vec![],
            Some(options.clone()),
        ))
        .unwrap();
        assert_eq!(calls.get(), 0);
        runtime.pump();
        assert_eq!(calls.get(), 1);
        socket.force_close();
        socket.force_open().unwrap();
        run(socket.create_websocket(
            "wss://relay/tunnel?ts-relay=wss%3A%2F%2Fvenue".into(),
            vec![],
            Some(options),
        ))
        .unwrap();
        assert_eq!(calls.get(), 1);
        runtime.pump();
        assert_eq!(calls.get(), 2);
        let dials = host.dials.borrow();
        assert_eq!(dials.len(), 2);
        assert_eq!(dials[0].headers()["x-auth"], "signed-1");
        assert_eq!(dials[1].headers()["x-auth"], "signed-2");
    }
    #[test]
    fn relay_owner_preflight_refuses_required_headers_before_provider_or_dial() {
        let (socket, _, runtime, _) = fixture();
        runtime.headers.set(true);
        runtime.relaying.set(true);
        let host = Rc::new(RelayHost::new(&runtime));
        host.supports_headers.set(false);
        *runtime.tunnel.borrow_mut() = Some(host.clone());
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        let provider: FreshHeadersProvider = Rc::new(move || {
            count.set(count.get() + 1);
            Default::default()
        });
        let error = run(socket.create_websocket(
            "wss://relay/tunnel?ts-relay=wss%3A%2F%2Fvenue".into(),
            vec![],
            Some(WebSocketConnectionOptions {
                headers: Some(UpgradeHeaders::Fresh(provider)),
                headers_required: true,
                ..Default::default()
            }),
        ))
        .unwrap_err();
        assert_eq!(error.name, "HeaderCapableWebSocketUnavailableError");
        assert_eq!(calls.get(), 0);
        assert!(host.dials.borrow().is_empty());
    }
    fn managed_relay_fixture() -> (Rc<BaseWebSocket>, Rc<Runtime>, Rc<RelayHost>) {
        managed_relay_options(None)
    }
    fn managed_relay_options(
        options: Option<WebSocketConnectionOptions>,
    ) -> (Rc<BaseWebSocket>, Rc<Runtime>, Rc<RelayHost>) {
        let (socket, _, runtime, _) = fixture();
        runtime.relaying.set(true);
        let host = Rc::new(RelayHost::new(&runtime));
        *runtime.tunnel.borrow_mut() = Some(host.clone());
        run(socket.create_websocket(
            "wss://relay/tunnel?ts-relay=wss%3A%2F%2Fvenue".into(),
            vec![],
            options,
        ))
        .unwrap();
        (socket, runtime, host)
    }
    #[test]
    fn managed_relay_retries_beyond_native_breaker_and_closes_old_receiver() {
        let (socket, runtime, host) = managed_relay_fixture();
        runtime.pump();
        for attempt in 1..=105 {
            assert!(socket.is_connected(), "relay attempt {attempt} must open");
            let io = host.sockets.borrow().last().unwrap().clone();
            io.frames.borrow_mut().push_back(SocketFrame::Close {
                code: 1006,
                reason: String::new(),
                was_clean: false,
            });
            runtime.pump();
            assert!(
                !socket.state.borrow().reconnecting,
                "managed relay must not latch core retry"
            );
            assert_eq!(
                socket.state.borrow().on_close_count,
                0,
                "managed relay must bypass native breaker"
            );
            runtime.advance(5_000.0);
        }
        assert_eq!(
            host.dials.borrow().len(),
            106,
            "automatic relay retries must remain unlimited"
        );
        let old = host.sockets.borrow().last().unwrap().clone();
        let old_task = socket.state.borrow().relay_task.unwrap();
        socket.force_close();
        assert!(
            socket.state.borrow().relay_task.is_none(),
            "force-close must release receiver slot before an event"
        );
        assert!(
            !socket.is_connected(),
            "force-close must synchronously clear relay readiness"
        );
        assert!(
            old.closed.get() > 0,
            "force-close must close actual relay transport"
        );
        socket.force_open().unwrap();
        runtime.pump();
        assert!(socket.is_connected());
        assert_ne!(socket.state.borrow().relay_task, Some(old_task));
        assert!(
            socket.state.borrow().relay_task != Some(old_task),
            "old receiver must not be orphaned"
        );
    }
    #[test]
    fn managed_relay_explicit_options_use_library_delay_and_custom_timeout_retry_limit() {
        let (socket, runtime, host) =
            managed_relay_options(Some(WebSocketConnectionOptions::default()));
        runtime.pump();
        host.sockets.borrow()[0]
            .frames
            .borrow_mut()
            .push_back(SocketFrame::Close {
                code: 1006,
                reason: String::new(),
                was_clean: false,
            });
        runtime.pump();
        runtime.advance(2_999.0);
        assert_eq!(
            host.dials.borrow().len(),
            1,
            "explicit empty options must use library minimum"
        );
        runtime.advance(1.0);
        assert_eq!(host.dials.borrow().len(), 2);
        socket.dispose();

        let (socket, runtime, host) = managed_relay_options(Some(WebSocketConnectionOptions {
            connection_timeout_ms: Some(123.0),
            max_retries: Some(0.0),
            ..Default::default()
        }));
        host.handshake.set(false);
        runtime.pump();
        runtime.advance(122.0);
        assert_eq!(host.sockets.borrow()[0].closed.get(), 0);
        runtime.advance(1.0);
        assert!(
            host.sockets.borrow()[0].closed.get() > 0,
            "explicit timeout must govern relay deadline"
        );
        runtime.advance(30_000.0);
        assert_eq!(
            host.dials.borrow().len(),
            1,
            "maxRetries zero must allow initial attempt but no retry"
        );
        assert!(socket.state.borrow().relay_task.is_none());
        socket.dispose();
    }

    #[test]
    fn managed_relay_timeout_and_task_discard_leave_recoverable_descriptor() {
        for polled in [false, true] {
            let (socket, runtime, host) = managed_relay_fixture();
            host.handshake.set(false);
            if polled {
                runtime.pump();
            }
            runtime.jobs.borrow_mut().clear();
            assert!(
                socket.state.borrow().relay_task.is_none(),
                "discarded relay task must release slot"
            );
            assert!(!socket.is_connected());
            host.handshake.set(true);
            socket.force_open().unwrap();
            runtime.pump();
            assert!(
                socket.is_connected(),
                "discarded relay task must permit force-open"
            );
        }
        let (socket, runtime, host) = managed_relay_fixture();
        host.handshake.set(false);
        runtime.pump();
        runtime.advance(3_999.0);
        assert_eq!(host.sockets.borrow()[0].closed.get(), 0);
        runtime.advance(1.0);
        assert!(
            host.sockets.borrow()[0].closed.get() > 0,
            "RWS outer deadline must close at four seconds"
        );
        assert_eq!(socket.state.borrow().on_close_count, 0);
        socket.force_close();
        runtime.advance(10_000.0);
        assert_eq!(
            host.dials.borrow().len(),
            1,
            "force-close must cancel retry backoff"
        );
        host.handshake.set(true);
        socket.force_open().unwrap();
        runtime.pump();
        assert!(socket.is_connected());
    }
    #[test]
    fn managed_relay_preserves_fresh_headers_backoff_and_explicit_restart_order() {
        let (socket, runtime, host) = managed_relay_fixture();
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let mut descriptor = socket.state.borrow().relay_descriptor.clone().unwrap();
        descriptor.options.headers = Some(UpgradeHeaders::Fresh(Rc::new(move || {
            observed.set(observed.get() + 1);
            [("x-auth".into(), observed.get().to_string())]
                .into_iter()
                .collect()
        })));
        socket.start_managed_relay(descriptor, 0.0).unwrap();
        runtime.pump();
        let close = || {
            host.sockets
                .borrow()
                .last()
                .unwrap()
                .frames
                .borrow_mut()
                .push_back(SocketFrame::Close {
                    code: 1006,
                    reason: String::new(),
                    was_clean: false,
                });
            runtime.pump();
        };
        for delay in [1000.0, 1300.0, 1690.0] {
            let before = host.dials.borrow().len();
            close();
            runtime.advance(delay - 1.0);
            assert_eq!(
                host.dials.borrow().len(),
                before,
                "managed backoff must not dial early"
            );
            runtime.advance(1.0);
            assert_eq!(
                host.dials.borrow().len(),
                before + 1,
                "managed backoff must follow RWS exponent"
            );
        }
        runtime.advance(5_000.0);
        let before = host.dials.borrow().len();
        close();
        runtime.advance(1_000.0);
        assert_eq!(
            host.dials.borrow().len(),
            before + 1,
            "stable uptime must reset retry exponent"
        );
        let old = host.sockets.borrow().last().unwrap().clone();
        let before = host.dials.borrow().len();
        socket.reconnect().unwrap();
        runtime.pump();
        assert_eq!(
            old.closed.get(),
            0,
            "explicit reconnect waits before closing current relay"
        );
        runtime.advance(999.0);
        assert_eq!(old.closed.get(), 0);
        runtime.advance(1.0);
        runtime.pump();
        assert!(
            old.closed.get() > 0,
            "explicit relay restart must close its previous transport"
        );
        assert_eq!(host.dials.borrow().len(), before + 1);
        for (index, dial) in host.dials.borrow().iter().enumerate() {
            assert_eq!(
                dial.headers()["x-auth"],
                (index + 1).to_string(),
                "each retry must resolve fresh headers"
            );
        }
        assert_eq!(calls.get(), host.dials.borrow().len());
        let old_jobs = std::mem::take(&mut *runtime.jobs.borrow_mut());
        let old = host.sockets.borrow().last().unwrap().clone();
        socket.force_open().unwrap();
        runtime.pump();
        assert!(
            old.closed.get() > 0,
            "force-open must close the previous relay without force-close first"
        );
        let new_task = socket.state.borrow().relay_task;
        drop(old_jobs);
        assert_eq!(
            socket.state.borrow().relay_task,
            new_task,
            "stale receiver guard must not remove replacement"
        );
        assert!(socket.is_connected());
    }
    #[test]
    fn managed_relay_inline_discard_and_sleep_error_release_only_active_run() {
        for fail_sleep in [false, true] {
            let (socket, runtime, host) = managed_relay_fixture();
            host.handshake.set(false);
            runtime.inline_spawn.set(true);
            runtime.fail_sleep.set(fail_sleep);
            socket.force_open().unwrap();
            runtime.jobs.borrow_mut().clear();
            assert!(
                socket.state.borrow().relay_task.is_none(),
                "inline relay discard/error must release run"
            );
            assert!(!socket.is_connected());
            host.handshake.set(true);
            runtime.fail_sleep.set(false);
            socket.force_open().unwrap();
            assert!(socket.is_connected());
            let weak = Rc::downgrade(&socket);
            socket.dispose();
            drop(socket);
            runtime.pump();
            assert!(
                weak.upgrade().is_none(),
                "relay tasks must not retain disposed socket"
            );
        }
    }
}

#[cfg(test)]
#[path = "../tests/shared_stringify/mod.rs"]
mod shared_stringify;
