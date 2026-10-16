//! JS socket base backed by the shared Rust BaseWebSocket core. The core owns
//! dialing, close handling and reconnects; its protocol callbacks dispatch to the
//! JS object's ported members (connectAndSubscribe, onOpen, onMessage, ping).
use super::{
    globals::arg,
    promise::await_js,
    rt::Rt,
    value::{Getter, Js, Thrown},
};
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture},
    websocket::{BaseWebSocket, WsProtocol},
    ws_parent::WsExchangeParent,
};
use safe_cex_transport::socket::SocketFrame;
use std::{
    cell::{OnceCell, RefCell},
    rc::{Rc, Weak},
};

pub struct SocketCore {
    pub rt: Rt,
    pub this: RefCell<Js>,
    pub base: OnceCell<Rc<BaseWebSocket>>,
    pub listeners: RefCell<Vec<(String, Js)>>,
    pub on_close_member: RefCell<Option<Js>>,
}

struct JsProtocol(Weak<SocketCore>);

fn core_error(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

impl JsProtocol {
    fn core(&self) -> Result<Rc<SocketCore>, ExchangeError> {
        self.0
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Bitget socket facade was dropped"))
    }
    fn emit(&self, event: &str, payload: Js) -> Result<(), ExchangeError> {
        let core = self.core()?;
        let listeners: Vec<Js> = core
            .listeners
            .borrow()
            .iter()
            .filter(|(e, _)| e == event)
            .map(|(_, f)| f.clone())
            .collect();
        let skip = core.on_close_member.borrow().clone();
        for listener in listeners {
            if event == "close" && skip.as_ref().is_some_and(|s| s.same_reference(&listener)) {
                continue;
            }
            let result = listener.call(Js::Undefined, vec![payload.clone()]).map_err(core_error)?;
            if let Js::Promise(p) = result
                && let Some(Err(e)) = p.result()
            {
                return Err(core_error(e));
            }
        }
        Ok(())
    }
}

impl WsProtocol for JsProtocol {
    fn ping(&self, _socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        let core = self.core()?;
        let this = core.this.borrow().clone();
        this.call_method("ping", vec![]).map(|_| ()).map_err(core_error)
    }
    fn connect_and_subscribe(&self, _socket: Rc<BaseWebSocket>, is_reconnect: bool) -> Result<(), ExchangeError> {
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
    fn on_message(&self, _socket: &Rc<BaseWebSocket>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        let data = match frame {
            SocketFrame::Text(text) => Js::str(text),
            SocketFrame::Binary(bytes) => Js::str(&String::from_utf8_lossy(bytes)),
            SocketFrame::Close { code, reason, was_clean } => {
                return self.emit(
                    "close",
                    Js::object(vec![
                        ("code", Js::Number(f64::from(*code))),
                        ("reason", Js::str(reason)),
                        ("wasClean", Js::Bool(*was_clean)),
                    ]),
                );
            }
        };
        self.emit("message", Js::object(vec![("data", data)]))
    }
}

impl SocketCore {
    /// The core is created on first use, once the venue exists as `dyn Exchange`.
    pub fn base(self: &Rc<Self>) -> Result<Rc<BaseWebSocket>, Thrown> {
        if let Some(base) = self.base.get() {
            return Ok(base.clone());
        }
        let venue = self
            .rt
            .shared
            .venue
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| Thrown::error("Error", "Bitget socket used before the exchange was assembled"))?;
        let id = SOCKET_IDS.with(|n| {
            let id = n.get() + 1;
            n.set(id);
            id
        });
        let base = BaseWebSocket::new(
            id,
            WsExchangeParent::new(&venue),
            self.rt.shared.runtime.clone(),
            Rc::new(JsProtocol(Rc::downgrade(self))),
        );
        let _ = self.base.set(base.clone());
        Ok(base)
    }
}

thread_local! {
    static SOCKET_IDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn ex(e: ExchangeError) -> Thrown {
    Thrown::from(e)
}

/// `new BaseWebSocket(parent)`: socket object with the base members the ported
/// socket bodies read. The returned object owns its core.
pub fn base_websocket(rt: &Rt, parent: Js) -> Js {
    let socket = Js::object_with_class("BaseWebSocket", vec![("parent", parent.clone())]);
    let core = Rc::new(SocketCore {
        rt: rt.clone(),
        this: RefCell::new(socket.clone()),
        base: OnceCell::new(),
        listeners: RefCell::default(),
        on_close_member: RefCell::default(),
    });
    let def = |name: &str, f: Js| {
        let _ = socket.define(name, f);
    };
    let accessor = |name: &str, g: Getter| socket.define_accessor(name, Some(g), None);
    {
        let parent = parent.clone();
        accessor("isDisposed", Rc::new(move |_| Ok(Js::Bool(parent.get("isDisposed")?.truthy()))));
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
    let _ = socket.define("pingAt", Js::Number(0.0));
    let _ = socket.define("messageHandlers", Js::object(vec![]));
    let _ = socket.define("lastReceived", Js::Number(0.0));
    {
        let core = core.clone();
        def(
            "createWebsocket",
            Js::function("createWebsocket", move |_, a| {
                let url = arg(&a, 0).to_display();
                let base = core.base()?;
                base.start_websocket(url, vec![], None);
                Ok(ws_handle(&core))
            }),
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
                core.base()?.send_with_log(&arg(&a, 0).to_display()).map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let core = core.clone();
        def(
            "recordMessageHandlerStats",
            Js::function("recordMessageHandlerStats", move |_, a| {
                core.base()?.record_message_handler_stats(&arg(&a, 0).to_display()).map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let parent = parent.clone();
        def(
            "emitAndLogError",
            Js::function("emitAndLogError", move |_, a| parent.call_method("emitAndLogError", a)),
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
            "emitWsDiagnosticReceive",
            Js::function("emitWsDiagnosticReceive", move |_, a| {
                let data = super::value::to_json(&arg(&a, 0))?;
                core.base()?.emit_ws_diagnostic_receive(&data, &Default::default()).map_err(ex)?;
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
                core.base()?.cleanup_message_handler_stats(&arg(&a, 0).to_display()).map_err(ex)?;
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
                let next = if next.is_undefined() { 5000 } else { next.to_number()? as u64 };
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
                core.base()?.reconnect().map_err(ex)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let closer = core.clone();
        let on_close = Js::function("onClose", move |_, a| {
            let event = arg(&a, 0);
            let code = event.get("code").ok().and_then(|c| c.as_f64()).map(|c| c as u16);
            let reason = event.get("reason").map(|r| r.to_display()).unwrap_or_default();
            closer.base()?.on_close(code, &reason).map_err(ex)?;
            Ok(Js::Undefined)
        });
        *core.on_close_member.borrow_mut() = Some(on_close.clone());
        def("onClose", on_close);
    }
    {
        let core = core.clone();
        def(
            "dispose",
            Js::function("dispose", move |_, _| {
                if let Some(base) = core.base.get() {
                    base.dispose();
                }
                Ok(Js::Undefined)
            }),
        );
    }
    let keep = core.clone();
    let _ = socket.define("__core", Js::function("__core", move |_, _| {
        let _ = &keep;
        Ok(Js::Undefined)
    }));
    SOCKET_CORES.with(|c| c.borrow_mut().push((socket.clone(), core)));
    socket
}

thread_local! {
    static SOCKET_CORES: RefCell<Vec<(Js, Rc<SocketCore>)>> = const { RefCell::new(vec![]) };
}

/// Core for a socket object built by `base_websocket`.
pub fn core_of(socket: &Js) -> Option<Rc<SocketCore>> {
    SOCKET_CORES.with(|c| {
        c.borrow()
            .iter()
            .find(|(s, _)| s.same_reference(socket))
            .map(|(_, core)| core.clone())
    })
}

fn ws_handle(core: &Rc<SocketCore>) -> Js {
    let ws = Js::object_with_class("WebSocket", vec![]);
    {
        let core = core.clone();
        let _ = ws.define(
            "addEventListener",
            Js::function("addEventListener", move |_, a| {
                core.listeners.borrow_mut().push((arg(&a, 0).to_display(), arg(&a, 1)));
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
                    base.force_close();
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
                Ok(Js::Number(if core.base.get().is_some_and(|b| b.is_connected()) { 1.0 } else { 0.0 }))
            })),
            None,
        );
    }
    ws
}

/// Await-safe call of a socket member returning possibly a promise.
pub async fn call_member(socket: Js, name: &'static str, args: Vec<Js>) -> Result<Js, Thrown> {
    await_js(socket.call_method(name, args)?).await
}
