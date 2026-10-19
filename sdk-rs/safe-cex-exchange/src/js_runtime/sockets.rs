//! JS socket base backed by the shared Rust BaseWebSocket core. The core owns
//! dialing, close handling and reconnects; its protocol callbacks dispatch to the
//! JS object's ported members (connectAndSubscribe, onOpen, onMessage, ping).
use super::{
    promise::Promise,
    value::arg,
    value::{Getter, Js, Thrown},
};
use crate::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture},
    websocket::{
        BaseWebSocket, PreparedWebSocket, SocketTimeout, WebSocketConnectionOptions, WsProtocol,
        WsRuntime,
    },
    ws_parent::WsExchangeParent,
};
use safe_cex_transport::socket::SocketFrame;
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};

/// Per-venue socket state: the WS runtime, the venue as `dyn Exchange` once
/// assembled (cores are created lazily after that) and the core registry.
pub struct JsSocketContext {
    pub runtime: Rc<dyn WsRuntime>,
    pub(crate) http: RefCell<Option<Rc<dyn super::client::JsHttpHost>>>,
    pub venue: RefCell<Option<Weak<dyn Exchange>>>,
    pub ids: Cell<u64>,
    pub cores: RefCell<Vec<(Js, Rc<SocketCore>)>>,
    prototype: OnceCell<Js>,
}

impl JsSocketContext {
    pub(crate) fn clear_timeout(&self, handle: &Js) {
        let cores: Vec<_> = self
            .cores
            .borrow()
            .iter()
            .map(|(_, core)| core.clone())
            .collect();
        for core in cores {
            let matching: Vec<_> = core
                .timeout_handles
                .borrow()
                .iter()
                .filter_map(|(timer, alias)| {
                    alias
                        .native_handle
                        .same_reference(handle)
                        .then_some((*timer, alias.id))
                })
                .collect();
            if let Some(base) = core.base.get() {
                for (timer, id) in matching {
                    if let Some(id) = id {
                        base.cancel_timeout(timer, id);
                    }
                }
            }
        }
    }

    pub fn new(runtime: Rc<dyn WsRuntime>) -> Rc<Self> {
        Rc::new(Self {
            runtime,
            http: RefCell::new(None),
            venue: RefCell::new(None),
            ids: Cell::new(0),
            cores: RefCell::default(),
            prototype: OnceCell::new(),
        })
    }
}

/// Event listeners registered on one `ws` handle, in registration order.
pub type ListenerList = Rc<RefCell<Vec<(String, Js, Rc<Cell<bool>>)>>>;

pub struct SocketCore {
    pub context: Rc<JsSocketContext>,
    pub this: RefCell<Js>,
    pub base: OnceCell<Rc<BaseWebSocket>>,
    /// Listeners of the current `ws` handle; a new createWebsocket replaces them,
    /// as a replaced socket object takes its listeners with it.
    pub listeners: RefCell<ListenerList>,
    pub on_close_member: RefCell<Option<Js>>,
    /// The JS `pingAt` value; every write also reaches the core's ping time.
    ping_at: RefCell<Js>,
    timeout_handles: RefCell<HashMap<SocketTimeout, TimeoutAlias>>,
    /// Source Number field before lazy native-core creation.
    last_received: Cell<f64>,
    /// Prevent a source `super.dispose()` from dispatching its override again.
    disposing: Cell<bool>,
}

struct TimeoutAlias {
    id: Option<u64>,
    native_handle: Js,
    value: Js,
}

impl Default for TimeoutAlias {
    fn default() -> Self {
        Self {
            id: None,
            native_handle: Js::Undefined,
            value: Js::Undefined,
        }
    }
}

struct JsProtocol(Weak<SocketCore>);

fn core_error(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

// `handlers` is captured once, but its length is read at both source record
// expressions. Reconciliation asks only for the second expression (missing=0).
fn read_handler_stats(
    core: &SocketCore,
    topic: &str,
    missing: usize,
) -> Result<(bool, safe_cex_num::PrototypeScalar), Thrown> {
    use safe_cex_num::PrototypeScalar as Scalar;
    let this = core.this.borrow().clone();
    let handlers = this.get("messageHandlers")?.get(topic)?;
    let has = missing == 1 && handlers.truthy() && handlers.get("length")?.truthy();
    let count = if handlers.truthy() {
        handlers.get("length")?
    } else {
        Js::Number(missing as f64)
    };
    let count = match count {
        Js::Undefined => Scalar::Undefined,
        Js::Null => Scalar::Null,
        Js::Bool(value) => Scalar::Boolean(value),
        Js::Number(value) => Scalar::Number(value),
        Js::String(value) => Scalar::String(String::from_utf16(&value).map_err(|_| {
            Thrown::type_error("lone UTF-16 surrogate at a statistics scalar boundary")
        })?),
        _ => {
            return Err(Thrown::type_error(
                "statistics handler length is outside the builtin property vocabulary",
            ));
        }
    };
    Ok((has, count))
}

fn connection_options(value: &Js) -> Result<WebSocketConnectionOptions, Thrown> {
    let optional = |name| -> Result<Js, Thrown> {
        if value.is_nullish() {
            Ok(Js::Undefined)
        } else {
            value.get(name)
        }
    };
    let number = |name| -> Result<Option<f64>, Thrown> {
        let value = optional(name)?;
        if value.is_undefined() {
            Ok(None)
        } else {
            Ok(Some(value.to_number()?))
        }
    };
    let start_closed = optional("startClosed")?;
    Ok(WebSocketConnectionOptions {
        min_reconnection_delay_ms: number("minReconnectionDelay")?,
        max_reconnection_delay_ms: number("maxReconnectionDelay")?,
        reconnection_delay_grow_factor: number("reconnectionDelayGrowFactor")?,
        min_uptime_ms: number("minUptime")?,
        connection_timeout_ms: number("connectionTimeout")?,
        max_retries: number("maxRetries")?,
        start_closed: (!start_closed.is_undefined()).then(|| start_closed.truthy()),
        headers_required: optional("headersRequired")?.truthy(),
        ..Default::default()
    })
}

fn create_websocket(core: &Rc<SocketCore>, args: &[Js]) -> Result<Js, Thrown> {
    let url = arg(args, 0).to_display();
    let protocols = match arg(args, 1) {
        Js::Undefined | Js::Null => vec![],
        Js::String(value) => vec![String::from_utf16_lossy(&value)],
        other => other
            .array_values()?
            .ok_or_else(|| Thrown::type_error("WebSocket protocols must be strings"))?
            .into_iter()
            .map(|value| value.to_display())
            .collect(),
    };
    let supplied = arg(args, 2);
    let options = if supplied.is_undefined() {
        None
    } else {
        Some(connection_options(&supplied)?)
    };
    if options
        .as_ref()
        .is_some_and(|options| options.headers_required)
        && !core.context.runtime.headers_capable()
    {
        return Err(Thrown::error(
            "HeaderCapableWebSocketUnavailableError",
            "Required WebSocket upgrade headers need a header-capable ws transport",
        ));
    }
    let base = core.base()?;
    let capable = core.context.runtime.headers_capable();
    // Native reconnects return through connectAndSubscribe/createWebsocket.
    // Resolve the retained provider inside each owned preparation, before dial;
    // a thrown auth error rejects preparation and cannot open a socket.
    base.start_prepared_websocket(move || async move {
        let mut options = options;
        if capable && !supplied.is_nullish() {
            let headers = supplied.get("headers").map_err(core_error)?;
            let headers = if headers.is_callable() {
                headers.call(Js::Undefined, vec![]).map_err(core_error)?
            } else {
                headers
            };
            if !headers.is_nullish() {
                let mut captured = indexmap::IndexMap::new();
                for (key, value) in headers.own_entries().map_err(core_error)? {
                    captured.insert(String::from_utf16_lossy(&key), value.to_display());
                }
                options.get_or_insert_with(Default::default).headers =
                    Some(safe_cex_transport::relay::UpgradeHeaders::Static(captured));
            }
        }
        Ok(Some(PreparedWebSocket {
            url,
            protocols,
            options,
        }))
    });
    Ok(ws_handle(core))
}

fn close_event(code: Option<u16>, reason: &str, was_clean: bool) -> Js {
    Js::object(vec![
        ("code", Js::Number(f64::from(code.unwrap_or(1005)))),
        ("reason", Js::str(reason)),
        ("wasClean", Js::Bool(was_clean)),
    ])
}

impl JsProtocol {
    fn recovers_after_offline(&self) -> bool {
        let Some(core) = self.0.upgrade() else {
            return false;
        };
        let this = core.this.borrow().clone();
        match this.get("recoversAfterOffline") {
            Ok(value) => value.truthy(),
            Err(thrown) => {
                core.context.runtime.report_exception(
                    &core_error(thrown),
                    &serde_json::json!({ "operation": "recoversAfterOffline" }).into(),
                );
                false
            }
        }
    }

    fn core(&self) -> Result<Rc<SocketCore>, ExchangeError> {
        self.0
            .upgrade()
            .ok_or_else(|| ExchangeError::new("socket facade was dropped"))
    }
    /// A receiver-defined `getWatchdog*ThresholdMs()`; absent or nullish keeps the core default.
    fn receiver_threshold(&self, member: &str) -> Option<f64> {
        let core = self.0.upgrade()?;
        let this = core.this.borrow().clone();
        let read = || -> Result<Option<f64>, Thrown> {
            let getter = this.get(member)?;
            if !getter.is_callable() {
                return Ok(None);
            }
            let value = getter.call(this.clone(), vec![])?;
            Ok(if value.is_nullish() {
                None
            } else {
                Some(value.to_number()?)
            })
        };
        read().unwrap_or_else(|thrown| {
            core.context.runtime.report_exception(
                &core_error(thrown),
                &serde_json::json!({ "operation": member }).into(),
            );
            None
        })
    }
    fn emit(&self, event: &str, payload: Js) -> Result<(), ExchangeError> {
        let core = self.core()?;
        let grouped = core
            .base
            .get()
            .is_some_and(|base| base.grouped_listener_dispatch());
        let current = core.listeners.borrow().clone();
        let listeners: Vec<_> = current
            .borrow()
            .iter()
            .filter(|(e, _, _)| e == event)
            .map(|(_, f, removed)| (f.clone(), removed.clone()))
            .collect();
        let skip = core.on_close_member.borrow().clone();
        for (listener, removed) in listeners {
            if !grouped && removed.get() {
                continue;
            }
            if event == "close" && skip.as_ref().is_some_and(|s| s.same_reference(&listener)) {
                continue;
            }
            if let Err(thrown) = listener.call(Js::Undefined, vec![payload.clone()]) {
                core.context.runtime.report_exception(
                    &core_error(thrown),
                    &serde_json::json!({ "operation": "websocket listener", "event": event })
                        .into(),
                );
                // RWS invokes its listeners in one host callback; plain EventTarget
                // reports each exception independently and continues dispatch.
                if grouped {
                    break;
                }
            }
        }
        Ok(())
    }
}

// Preserve the host binary object until a venue explicitly consumes its bytes
// or asks for text. Both WebSocket and the maintained reconnecting wrapper
// default to Blob, including when the wrapper uses Node ws underneath.
fn binary_data(bytes: &[u8], runtime: Rc<dyn crate::runtime::ExchangeRuntime>) -> Js {
    let values = Js::array(
        bytes
            .iter()
            .map(|byte| Js::Number(f64::from(*byte)))
            .collect(),
    );
    let buffer = Js::object_with_class(
        "ArrayBuffer",
        vec![
            ("bytes", values.clone()),
            ("byteLength", Js::Number(bytes.len() as f64)),
        ],
    );
    let size = bytes.len() as f64;
    let text = String::from_utf8_lossy(bytes).into_owned();
    let text_runtime = runtime.clone();
    Js::object_with_class(
        "Blob",
        vec![
            ("size", Js::Number(size)),
            (
                "arrayBuffer",
                Js::function("arrayBuffer", move |_, _| {
                    Ok(Js::Promise(Promise::settled(&runtime, Ok(buffer.clone()))))
                }),
            ),
            (
                "text",
                Js::function("text", move |_, _| {
                    Ok(Js::Promise(Promise::settled(
                        &text_runtime,
                        Ok(Js::str(&text)),
                    )))
                }),
            ),
        ],
    )
}

impl WsProtocol for JsProtocol {
    fn reconnect(&self, _socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        let core = self.core()?;
        let this = core.this.borrow().clone();
        this.call_method("reconnect", vec![])
            .map(|_| ())
            .map_err(core_error)
    }

    fn recovers_after_offline(&self) -> bool {
        JsProtocol::recovers_after_offline(self)
    }

    fn dispose(&self) {
        let Some(core) = self.0.upgrade() else {
            return;
        };
        if core.disposing.replace(true) {
            return;
        }
        let this = core.this.borrow().clone();
        let result = this.call_method("dispose", vec![]);
        core.disposing.set(false);
        // WsProtocol disposal is synchronous and infallible. Report a source
        // throw through the host and allow core-owned transport cleanup to run.
        if let Err(thrown) = result {
            core.context.runtime.report_exception(
                &core_error(thrown),
                &serde_json::json!({ "operation": "dispose" }).into(),
            );
        }
    }

    fn check_last_received(&self, _socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        let core = self.core()?;
        let this = core.this.borrow().clone();
        this.call_method("checkLastReceived", vec![])
            .map(|_| ())
            .map_err(core_error)
    }

    fn ping(&self, _socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        let core = self.core()?;
        let this = core.this.borrow().clone();
        this.call_method("ping", vec![])
            .map(|_| ())
            .map_err(core_error)
    }
    fn connect_and_subscribe(
        &self,
        _socket: Rc<BaseWebSocket>,
        is_reconnect: bool,
    ) -> Result<(), ExchangeError> {
        let core = self.core()?;
        let this = core.this.borrow().clone();
        this.call_method("connectAndSubscribe", vec![Js::Bool(is_reconnect)])
            .map(|_| ())
            .map_err(core_error)
    }
    fn on_open(&self, _socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let result = self.emit("open", Js::object(vec![("type", Js::str("open"))]));
        Box::pin(async move { result })
    }
    fn on_transport_close(
        &self,
        _socket: &Rc<BaseWebSocket>,
        code: Option<u16>,
        reason: &str,
    ) -> Result<(), ExchangeError> {
        self.emit("close", close_event(code, reason, code != Some(1006)))
    }
    /// ts: the venue registers its own close listener; the core handles the close only
    /// when that listener is the base `onClose`. A replaced `onClose` field or a
    /// no-event `handleClose = () => this.onClose()` owns it instead.
    fn owns_close_handling(&self) -> bool {
        let Ok(core) = self.core() else {
            return false;
        };
        let Some(base_member) = core.on_close_member.borrow().clone() else {
            return false;
        };
        let listeners = core.listeners.borrow().clone();
        !listeners
            .borrow()
            .iter()
            .any(|(event, listener, _)| event == "close" && listener.same_reference(&base_member))
    }
    fn watchdog_stale_threshold_ms(&self) -> Option<f64> {
        self.receiver_threshold("getWatchdogStaleThresholdMs")
    }
    fn watchdog_ping_threshold_ms(&self) -> Option<f64> {
        self.receiver_threshold("getWatchdogPingThresholdMs")
    }
    /// ts: a subclass `resyncsPrivateStateOnReopen` getter replaces base.ws.ts's
    /// private-socket default; an undefined receiver member keeps that default.
    fn resyncs_private_state_on_reopen(&self) -> Option<bool> {
        let core = self.0.upgrade()?;
        let this = core.this.borrow().clone();
        match this.get("resyncsPrivateStateOnReopen") {
            Ok(value) if value.is_undefined() => None,
            Ok(value) => Some(value.truthy()),
            Err(thrown) => {
                core.context.runtime.report_exception(
                    &core_error(thrown),
                    &serde_json::json!({ "operation": "resyncsPrivateStateOnReopen" }).into(),
                );
                None
            }
        }
    }
    fn on_error(&self, _socket: &BaseWebSocket, message: &str) -> Result<(), ExchangeError> {
        self.emit(
            "error",
            Js::object(vec![
                ("type", Js::str("error")),
                ("message", Js::str(message)),
            ]),
        )
    }
    fn on_message(
        &self,
        _socket: &Rc<BaseWebSocket>,
        frame: &SocketFrame,
    ) -> Result<(), ExchangeError> {
        let data = match frame {
            SocketFrame::Text(text) => Js::str(text),
            SocketFrame::Binary(bytes) => {
                let runtime = self.core()?.context.runtime.clone();
                binary_data(bytes, runtime)
            }
            SocketFrame::Close {
                code,
                reason,
                was_clean,
            } => {
                return self.emit("close", close_event(Some(*code), reason, *was_clean));
            }
        };
        self.emit("message", Js::object(vec![("data", data)]))
    }
}

impl SocketCore {
    fn timeout_handle(&self, timer: SocketTimeout) -> Js {
        let id = self.base.get().and_then(|base| base.timeout_id(timer));
        let mut handles = self.timeout_handles.borrow_mut();
        let entry = handles.entry(timer).or_default();
        if id.is_some() && entry.id != id {
            entry.id = id;
            entry.native_handle = Js::object_with_class("Timeout", vec![]);
            entry.value = entry.native_handle.clone();
        }
        entry.value.clone()
    }
    fn set_timeout_handle(&self, timer: SocketTimeout, value: Js) {
        self.timeout_handle(timer);
        self.timeout_handles
            .borrow_mut()
            .get_mut(&timer)
            .unwrap()
            .value = value;
    }

    /// The core is created on first use, once the venue exists as `dyn Exchange`.
    pub fn base(self: &Rc<Self>) -> Result<Rc<BaseWebSocket>, Thrown> {
        if let Some(base) = self.base.get() {
            self.apply_pong_policy(base)?;
            return Ok(base.clone());
        }
        let venue = self
            .context
            .venue
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| {
                Thrown::error("Error", "socket used before the exchange was assembled")
            })?;
        let ids = &self.context.ids;
        let id = ids.get() + 1;
        ids.set(id);
        let base = BaseWebSocket::new(
            id,
            WsExchangeParent::new(&venue),
            self.context.runtime.clone(),
            Rc::new(JsProtocol(Rc::downgrade(self))),
        );
        base.set_last_received(self.last_received.get());
        let weak = Rc::downgrade(self);
        base.set_handler_stats_reader(Rc::new(move |topic, missing| {
            let core = weak
                .upgrade()
                .ok_or_else(|| ExchangeError::new("socket facade was dropped"))?;
            read_handler_stats(&core, topic, missing).map_err(core_error)
        }));
        let _ = self.base.set(base.clone());
        self.apply_pong_policy(&base)?;
        Ok(base)
    }
    /// A receiver's own `treatAnyMessageAsPong` field drives the core's flag.
    fn apply_pong_policy(&self, base: &BaseWebSocket) -> Result<(), Thrown> {
        let this = self.this.borrow().clone();
        let treat = this.get("treatAnyMessageAsPong")?;
        if !treat.is_undefined() {
            base.treat_any_message_as_pong.set(treat.truthy());
        }
        Ok(())
    }
}

fn ex(e: ExchangeError) -> Thrown {
    Thrown::from(e)
}

/// `BaseWebSocket.prototype`: the base members TS declares as methods, so a
/// derived arrow field can shadow them and still reach them via `super`.
fn base_prototype(context: &Rc<JsSocketContext>) -> Js {
    context
        .prototype
        .get_or_init(|| {
            let weak = Rc::downgrade(context);
            Js::object(vec![(
                "dispose",
                Js::function("dispose", move |this, _| {
                    if let Some(context) = weak.upgrade()
                        && let Some(core) = core_of(&context, &this)
                        && let Some(base) = core.base.get()
                    {
                        // A direct source override calling super must not
                        // redispatch itself through the typed protocol hook.
                        let was_disposing = core.disposing.replace(true);
                        base.dispose();
                        core.disposing.set(was_disposing);
                    }
                    Ok(Js::Undefined)
                }),
            )])
        })
        .clone()
}

/// The `BaseWebSocket` class: construct with `(parent)`; `prototype` is shared by
/// every socket of this context.
pub fn base_websocket_class(context: &Rc<JsSocketContext>) -> Js {
    let ctx = context.clone();
    let class = Js::constructor("BaseWebSocket", None, move |_, a| {
        Ok(base_websocket(&ctx, arg(&a, 0)))
    });
    let _ = class.define("prototype", base_prototype(context));
    class
}

/// `new BaseWebSocket(parent)`: socket object with the base members the ported
/// socket bodies read. The returned object owns its core.
pub fn base_websocket(context: &Rc<JsSocketContext>, parent: Js) -> Js {
    let socket = Js::object_with_class("BaseWebSocket", vec![("parent", parent.clone())]);
    if let Js::Object(o) = &socket {
        *o.proto.borrow_mut() = Some(base_prototype(context));
    }
    let core = Rc::new(SocketCore {
        context: context.clone(),
        this: RefCell::new(socket.clone()),
        base: OnceCell::new(),
        listeners: RefCell::default(),
        on_close_member: RefCell::default(),
        ping_at: RefCell::new(Js::Number(0.0)),
        last_received: Cell::new(0.0),
        disposing: Cell::new(false),
        timeout_handles: RefCell::default(),
    });
    let def = |name: &str, f: Js| {
        let _ = socket.define(name, f);
    };
    let accessor = |name: &str, g: Getter| {
        socket
            .define_accessor(name, Some(g), None)
            .expect("accessor target is a fresh extensible host object")
    };
    {
        let parent = parent.clone();
        accessor(
            "isDisposed",
            Rc::new(move |_| Ok(Js::Bool(parent.get("isDisposed")?.truthy()))),
        );
    }
    {
        let parent = parent.clone();
        accessor("store", Rc::new(move |_| parent.get("store")));
    }
    {
        let parent = parent.clone();
        accessor("emitter", Rc::new(move |_| parent.get("emitter")));
    }
    {
        let core = core.clone();
        accessor(
            "isConnected",
            Rc::new(move |_| Ok(Js::Bool(core.base.get().is_some_and(|b| b.is_connected())))),
        );
    }
    let _ = socket.define("ws", Js::Undefined);
    {
        let (reader, writer) = (core.clone(), core.clone());
        socket
            .define_accessor(
                "pingAt",
                Some(Rc::new(move |_| Ok(reader.ping_at.borrow().clone()))),
                Some(Rc::new(move |_, value: Js| {
                    // ts: handlePongEvent reads `if (this.pingAt)`, so a falsy value is no ping.
                    let at = if value.truthy() {
                        value.to_number()?
                    } else {
                        0.0
                    };
                    writer.base()?.set_ping_at(at);
                    *writer.ping_at.borrow_mut() = value;
                    Ok(())
                })),
            )
            .expect("accessor target is a fresh extensible host object");
    }
    for (field, timer) in [
        ("pingTimeoutId", SocketTimeout::Ping),
        ("pongMissTimeoutId", SocketTimeout::PongMiss),
        ("lastReceivedTimer", SocketTimeout::Receive),
    ] {
        let (reader, writer) = (core.clone(), core.clone());
        socket
            .define_accessor(
                field,
                Some(Rc::new(move |_| Ok(reader.timeout_handle(timer)))),
                Some(Rc::new(move |_, value| {
                    writer.set_timeout_handle(timer, value);
                    Ok(())
                })),
            )
            .expect("accessor target is a fresh extensible host object");
    }
    let _ = socket.define("messageHandlers", Js::object(vec![]));
    {
        let (reader, writer) = (core.clone(), core.clone());
        socket
            .define_accessor(
                "lastReceived",
                Some(Rc::new(move |_| {
                    Ok(Js::Number(reader.base.get().map_or_else(
                        || reader.last_received.get(),
                        |base| base.last_received(),
                    )))
                })),
                Some(Rc::new(move |_, value: Js| {
                    let value = value.to_number()?;
                    writer.last_received.set(value);
                    if let Some(base) = writer.base.get() {
                        base.set_last_received(value);
                    }
                    Ok(())
                })),
            )
            .expect("accessor target is a fresh extensible host object");
    }
    // Source field initializers without a native state accessor.
    for (field, value) in [
        ("recoversAfterOffline", Js::Bool(true)),
        ("lastCloseTime", Js::Number(0.0)),
        ("onCloseCount", Js::Number(0.0)),
        ("reconnecting", Js::Bool(false)),
        ("pingSent", Js::Bool(false)),
    ] {
        let _ = socket.define(field, value);
    }
    {
        let core = core.clone();
        def(
            "createWebsocket",
            Js::function("createWebsocket", move |_, a| create_websocket(&core, &a)),
        );
    }
    {
        let core = core.clone();
        def(
            "send",
            Js::function("send", move |_, a| {
                core.base()?.send(&arg(&a, 0).to_display()).map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        def(
            "sendWithLog",
            Js::function("sendWithLog", move |_, a| {
                core.base()?
                    .send_with_log(&arg(&a, 0).to_display())
                    .map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        def(
            "recordMessageHandlerStats",
            Js::function("recordMessageHandlerStats", move |_, a| {
                core.base()?
                    .record_message_handler_stats(&arg(&a, 0).to_display())
                    .map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let parent = parent.clone();
        def(
            "emitAndLogError",
            Js::function("emitAndLogError", move |_, a| {
                parent.call_method("emitAndLogError", a)
            }),
        );
    }
    {
        let core = core.clone();
        def(
            "trackLastReceived",
            Js::function("trackLastReceived", move |_, _| {
                core.base()?.track_last_received().map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        def(
            "checkLastReceived",
            Js::function("checkLastReceived", move |_, _| {
                core.base()?.check_last_received().map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        def(
            "emitWsDiagnosticReceive",
            Js::function("emitWsDiagnosticReceive", move |_, a| {
                let data = super::value::to_json(&arg(&a, 0))?;
                core.base()?
                    .emit_ws_diagnostic_receive(&data, &Default::default())
                    .map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        def(
            "emitWsReconnected",
            Js::function("emitWsReconnected", move |_, _| {
                core.base()?.emit_ws_reconnected().map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        def(
            "cleanupMessageHandlerStats",
            Js::function("cleanupMessageHandlerStats", move |_, a| {
                core.base()?
                    .cleanup_message_handler_stats(&arg(&a, 0).to_display())
                    .map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        def(
            "handlePongEvent",
            Js::function("handlePongEvent", move |_, a| {
                let next = arg(&a, 0);
                let next = if next.is_undefined() {
                    5000
                } else {
                    next.to_number()? as u64
                };
                core.base()?.handle_pong_event(next).map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        def(
            "reconnect",
            Js::function("reconnect", move |_, _| {
                core.base()?.reconnect_base().map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let closer = core.clone();
        let on_close = Js::function("onClose", move |_, a| {
            let event = arg(&a, 0);
            let code = event
                .get("code")
                .ok()
                .and_then(|c| c.as_f64())
                .map(|c| c as u16);
            let reason = event
                .get("reason")
                .map(|r| r.to_display())
                .unwrap_or_default();
            closer.base()?.on_close(code, &reason).map_err(ex)?;
            Ok(Js::Undefined)
        });
        *core.on_close_member.borrow_mut() = Some(on_close.clone());
        def("onClose", on_close);
    }
    {
        let core = core.clone();
        def(
            "retryFailedConnect",
            Js::function("retryFailedConnect", move |_, a| {
                let message = arg(&a, 0).to_js_string()?;
                core.base()?
                    .retry_failed_connect(&String::from_utf16_lossy(&message))
                    .map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        def(
            "recordCloseDiagnostics",
            Js::function("recordCloseDiagnostics", move |_, a| {
                let code = arg(&a, 0);
                let code = if code.is_undefined() {
                    None
                } else {
                    Some(code.to_number()? as u16)
                };
                let options = arg(&a, 1);
                let synthetic =
                    !options.is_nullish() && options.get("syntheticIfNoSocket")?.truthy();
                core.base()?.record_close_diagnostics(code, synthetic);
                Ok(Js::Undefined)
            }),
        );
    }
    let keep = core.clone();
    let _ = socket.define(
        "__core",
        Js::function("__core", move |_, _| {
            let _ = &keep;
            Ok(Js::Undefined)
        }),
    );
    context.cores.borrow_mut().push((socket.clone(), core));
    socket
}

/// Core for a socket object built by `base_websocket`.
pub fn core_of(context: &JsSocketContext, socket: &Js) -> Option<Rc<SocketCore>> {
    context
        .cores
        .borrow()
        .iter()
        .find(|(s, _)| s.same_reference(socket))
        .map(|(_, core)| core.clone())
}

fn ws_handle(core: &Rc<SocketCore>) -> Js {
    let ws = Js::object_with_class("WebSocket", vec![]);
    let listeners: ListenerList = Rc::default();
    *core.listeners.borrow_mut() = listeners.clone();
    {
        let listeners = listeners.clone();
        let _ = ws.define(
            "addEventListener",
            Js::function("addEventListener", move |_, a| {
                let (event, listener) = (arg(&a, 0).to_display(), arg(&a, 1));
                let mut list = listeners.borrow_mut();
                // EventTarget ignores a second registration of the same listener.
                if !list
                    .iter()
                    .any(|(e, f, _)| *e == event && f.same_reference(&listener))
                {
                    list.push((event, listener, Rc::new(Cell::new(false))));
                }
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let _ = ws.define(
            "removeEventListener",
            Js::function("removeEventListener", move |_, a| {
                let (event, listener) = (arg(&a, 0).to_display(), arg(&a, 1));
                listeners.borrow_mut().retain(|(e, f, removed)| {
                    let keep = !(*e == event && f.same_reference(&listener));
                    if !keep {
                        removed.set(true);
                    }
                    keep
                });
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        let _ = ws.define(
            "send",
            Js::function("send", move |_, a| {
                core.base()?.send(&arg(&a, 0).to_display()).map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        let _ = ws.define(
            "close",
            Js::function("close", move |_, _| {
                if let Some(base) = core.base.get() {
                    base.close_current_websocket();
                }
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        ws.define_accessor(
            "readyState",
            Some(Rc::new(move |_| {
                Ok(Js::Number(
                    if core.base.get().is_some_and(|b| b.is_connected()) {
                        1.0
                    } else {
                        0.0
                    },
                ))
            })),
            None,
        )
        .expect("accessor target is a fresh extensible host object");
    }
    ws
}
