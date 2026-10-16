//! The JS `BaseWebSocket` superclass of the two venue socket classes, over the
//! Rust `BaseWebSocket` (dialing, reconnect/backoff, health, stats).
//!
//! The ported subclasses register `open`/`message` listeners on the object
//! `createWebsocket` returns; the Rust core dials and calls `WsProtocol`, which
//! this bridge forwards to those listeners and to the instance's own
//! `connectAndSubscribe`/`ping`. CHOSEN: `close` listeners are not invoked —
//! the Rust core runs the close/reconnect path the base `onClose` would.
use super::{
    Realm,
    value::{FunctionData, Kind, NativeConstruct, ObjectData, Prop, PropKey, Slot, Thrown, Value},
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
    socket_lifecycle::SocketSlot,
    websocket::{
        BaseWebSocket, WsDiagnosticReceiveOptions, WsDiagnosticSendOptions, WsParent, WsProtocol,
        WsRuntime,
    },
};
use safe_cex_transport::socket::SocketFrame;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

#[derive(Clone)]
pub(crate) struct SocketDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub parent: Rc<dyn WsParent>,
    pub base: Rc<BaseExchange>,
}

/// The listeners registered on the current `ws` object.
#[derive(Default)]
struct Listeners {
    open: Vec<Value>,
    message: Vec<Value>,
}

struct Bridge {
    realm: Realm,
    instance: RefCell<Weak<std::cell::RefCell<ObjectData>>>,
    listeners: RefCell<Rc<RefCell<Listeners>>>,
}

/// The per-instance native handle, stored on the JS instance.
struct Handle {
    socket: Rc<BaseWebSocket>,
    bridge: Rc<Bridge>,
}

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}

impl Bridge {
    fn instance(&self) -> Option<Value> {
        self.instance.borrow().upgrade().map(Value::Object)
    }
    fn fail(&self, r: &mut Realm, thrown: &Value) -> ExchangeError {
        r.exchange_error(thrown)
    }
    fn call_member(&self, name: &str, args: &[Value]) -> Result<Value, ExchangeError> {
        let mut r = self.realm.clone();
        let Some(instance) = self.instance() else {
            return Ok(Value::Undefined);
        };
        let f = r
            .get_str(&instance, name)
            .map_err(|e| self.fail(&mut r, &e))?;
        if !f.is_callable() {
            return Err(ExchangeError::new(format!(
                "Hyperliquid socket has no {name}"
            )));
        }
        r.call(&f, &instance, args)
            .map_err(|e| self.fail(&mut r, &e))
    }
}

impl WsProtocol for Bridge {
    fn ping(&self, _: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        self.call_member("ping", &[]).map(|_| ())
    }
    fn connect_and_subscribe(
        &self,
        _: Rc<BaseWebSocket>,
        is_reconnect: bool,
    ) -> Result<(), ExchangeError> {
        self.call_member("connectAndSubscribe", &[Value::Bool(is_reconnect)])
            .map(|_| ())
    }
    fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let realm = self.realm.clone();
        let listeners: Vec<Value> = self.listeners.borrow().borrow().open.clone();
        let instance = self.instance();
        Box::pin(async move {
            let mut r = realm;
            let event = r.object_from(vec![("type", Value::str("open"))]);
            for listener in listeners {
                let this = instance.clone().unwrap_or(Value::Undefined);
                let out = r
                    .call(&listener, &this, std::slice::from_ref(&event))
                    .map_err(|e| r.exchange_error(&e))?;
                // An async onOpen (auth before subscribe) is awaited.
                r.await_value(out).await.map_err(|e| r.exchange_error(&e))?;
            }
            Ok(())
        })
    }
    fn on_message(&self, _: &Rc<BaseWebSocket>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        let data = match frame {
            SocketFrame::Text(text) => Value::str(text),
            SocketFrame::Binary(bytes) => Value::str(&String::from_utf8_lossy(bytes)),
            _ => return Ok(()),
        };
        let mut r = self.realm.clone();
        let listeners: Vec<Value> = self.listeners.borrow().borrow().message.clone();
        let event = r.object_from(vec![("data", data)]);
        let this = self.instance().unwrap_or(Value::Undefined);
        for listener in listeners {
            r.call(&listener, &this, std::slice::from_ref(&event))
                .map_err(|e| r.exchange_error(&e))?;
        }
        Ok(())
    }
}

fn handle_of(r: &mut Realm, this: &Value) -> Result<Rc<Handle>, Thrown> {
    let slot = r.get_str(this, "__nativeSocket")?;
    slot.host::<Handle>().ok_or_else(|| {
        r.type_error("BaseWebSocket method called on an object that is not a socket")
    })
}
fn thrown(r: &Realm, e: ExchangeError) -> Thrown {
    r.error(&e.message)
}

/// A JS callback as a diagnostic formatter (errors and non-strings mean "drop").
fn formatter(realm: &Realm, f: Value) -> Option<Rc<dyn Fn(&serde_json::Value) -> Option<String>>> {
    if !f.is_callable() {
        return None;
    }
    let realm = realm.clone();
    Some(Rc::new(move |data: &serde_json::Value| {
        let mut r = realm.clone();
        let v = r.from_serde(data);
        match r.call(&f, &Value::Undefined, &[v]) {
            Ok(Value::String(u)) => Some(String::from_utf16_lossy(&u)),
            _ => None,
        }
    }))
}

pub(crate) fn base_websocket_class(realm: &mut Realm, deps: SocketDependencies) -> Value {
    let prototype = realm.new_object();
    let next_id = Rc::new(Cell::new(1u64));
    let construct: NativeConstruct = {
        let fallback = prototype.object().unwrap().clone();
        let deps = deps.clone();
        Rc::new(move |r, args, new_target| {
            let proto = r.prototype_of_target(new_target, &fallback)?;
            let instance = r.alloc(Kind::Ordinary, Some(&proto));
            r.define_str(&instance, "parent", arg(args, 0))?;
            r.define_str(&instance, "pingAt", Value::Number(0.0))?;
            let books = r.new_object();
            r.define_str(&instance, "orderBookInnerState", books)?;
            let bridge = Rc::new(Bridge {
                realm: r.clone(),
                instance: RefCell::new(Rc::downgrade(instance.object().unwrap())),
                listeners: RefCell::default(),
            });
            let id = next_id.get();
            next_id.set(id + 1);
            let socket = BaseWebSocket::new(
                id,
                deps.parent.clone(),
                deps.runtime.clone(),
                bridge.clone(),
            );
            let class_name = new_target
                .function()
                .map(|f| String::from_utf16_lossy(&f.name.0))
                .unwrap_or_default();
            let slot = if class_name.contains("Private") {
                SocketSlot::Private
            } else {
                SocketSlot::Public
            };
            deps.base.set_socket(slot, Some(socket.clone()));
            let handle = r.host_object(Rc::new(Handle { socket, bridge }));
            r.define_hidden(&instance, PropKey::str("__nativeSocket"), handle);
            Ok(instance)
        })
    };
    let class = realm.function_data(FunctionData {
        name: realm.js_text("BaseWebSocket"),
        length: 1,
        call: None,
        construct: Some(construct),
    });
    if let Some(o) = class.object() {
        o.borrow_mut().props.push(Prop {
            key: PropKey::str("prototype"),
            slot: Slot::Data(prototype.clone()),
            enumerable: false,
            writable: false,
            configurable: false,
        });
    }
    realm.define_hidden(&prototype, PropKey::str("constructor"), class.clone());

    type Body = fn(&mut Realm, &Rc<Handle>, &Value, &[Value]) -> Result<Value, Thrown>;
    let method = |realm: &Realm, name: &str, length: u8, body: Body| {
        let f = realm.native(name, length, move |r, this, args| {
            let h = handle_of(r, this)?;
            body(r, &h, this, args)
        });
        realm.define_hidden(&prototype, PropKey::str(name), f);
    };
    let getter = |realm: &Realm, name: &str, body: Body| {
        let f = realm.native(&format!("get {name}"), 0, move |r, this, args| {
            let h = handle_of(r, this)?;
            body(r, &h, this, args)
        });
        realm.define_getter(&prototype, PropKey::str(name), f);
    };
    method(realm, "createWebsocket", 3, |r, h, _, args| {
        let url = r.to_rust_string(&arg(args, 0))?;
        let protocols = match arg(args, 1) {
            Value::Undefined | Value::Null => vec![],
            Value::String(u) => vec![String::from_utf16_lossy(&u)],
            list => {
                let mut out = vec![];
                for p in r.collect_iterable(&list)? {
                    out.push(r.to_rust_string(&p)?);
                }
                out
            }
        };
        // A fresh ws object owns fresh listeners; the previous object's go dead.
        let listeners: Rc<RefCell<Listeners>> = Rc::default();
        *h.bridge.listeners.borrow_mut() = listeners.clone();
        let ws = r.new_object();
        let add = {
            let listeners = listeners.clone();
            r.native("addEventListener", 2, move |r, _, a| {
                let kind = r.to_rust_string(&arg(a, 0))?;
                let f = arg(a, 1);
                match kind.as_str() {
                    "open" => listeners.borrow_mut().open.push(f),
                    "message" => listeners.borrow_mut().message.push(f),
                    _ => {}
                }
                Ok(Value::Undefined)
            })
        };
        r.define_str(&ws, "addEventListener", add)?;
        let socket = h.socket.clone();
        let close = r.native("close", 0, move |_, _, _| {
            socket.close_current_websocket();
            Ok(Value::Undefined)
        });
        r.define_str(&ws, "close", close)?;
        let socket = h.socket.clone();
        let ready = r.native("get readyState", 0, move |_, _, _| {
            Ok(Value::Number(if socket.is_connected() { 1.0 } else { 3.0 }))
        });
        r.define_getter(&ws, PropKey::str("readyState"), ready);
        h.socket.start_websocket(url, protocols, None);
        Ok(ws)
    });
    method(realm, "send", 1, |r, h, _, args| {
        let message = r.to_rust_string(&arg(args, 0))?;
        h.socket.send(&message).map_err(|e| thrown(r, e))?;
        Ok(Value::Undefined)
    });
    method(realm, "sendWithLog", 1, |r, h, _, args| {
        let message = r.to_rust_string(&arg(args, 0))?;
        h.socket.send_with_log(&message).map_err(|e| thrown(r, e))?;
        Ok(Value::Undefined)
    });
    method(realm, "trackLastReceived", 0, |r, h, _, _| {
        h.socket.track_last_received().map_err(|e| thrown(r, e))?;
        Ok(Value::Undefined)
    });
    method(realm, "recordMessageHandlerStats", 1, |r, h, _, args| {
        let topic = r.to_rust_string(&arg(args, 0))?;
        h.socket
            .record_message_handler_stats(&topic)
            .map_err(|e| thrown(r, e))?;
        Ok(Value::Undefined)
    });
    method(realm, "cleanupMessageHandlerStats", 1, |r, h, _, args| {
        let topic = r.to_rust_string(&arg(args, 0))?;
        h.socket
            .cleanup_message_handler_stats(&topic)
            .map_err(|e| thrown(r, e))?;
        Ok(Value::Undefined)
    });
    method(realm, "emitWsDiagnosticReceive", 2, |r, h, _, args| {
        let data = r.to_serde(&arg(args, 0))?;
        let options = arg(args, 1);
        let format = if options.object().is_some() {
            r.get_str(&options, "format")?
        } else {
            Value::Undefined
        };
        let options = WsDiagnosticReceiveOptions {
            format: formatter(r, format),
            ..Default::default()
        };
        h.socket
            .emit_ws_diagnostic_receive(&data, &options)
            .map_err(|e| thrown(r, e))?;
        Ok(Value::Undefined)
    });
    method(realm, "emitWsDiagnosticSend", 2, |r, h, _, args| {
        let data = r.to_rust_string(&arg(args, 0))?;
        let options = arg(args, 1);
        let format = if options.object().is_some() {
            r.get_str(&options, "format")?
        } else {
            Value::Undefined
        };
        let format = formatter(r, format).map(|f| {
            Rc::new(move |text: &str| f(&serde_json::Value::String(text.into())))
                as Rc<dyn Fn(&str) -> Option<String>>
        });
        let options = WsDiagnosticSendOptions {
            format,
            ..Default::default()
        };
        h.socket
            .emit_ws_diagnostic_send(&data, &options)
            .map_err(|e| thrown(r, e))?;
        Ok(Value::Undefined)
    });
    method(realm, "resetOrderbook", 0, |_, h, _, _| {
        h.socket.reset_orderbook();
        Ok(Value::Undefined)
    });
    method(realm, "handlePongEvent", 1, |r, h, _, args| {
        let next = match arg(args, 0) {
            Value::Undefined => 5000.0,
            v => r.to_number(&v)?,
        };
        h.socket
            .handle_pong_event(next.max(0.0) as u64)
            .map_err(|e| thrown(r, e))?;
        Ok(Value::Undefined)
    });
    method(realm, "emitWsReconnected", 0, |r, h, _, _| {
        h.socket.emit_ws_reconnected().map_err(|e| thrown(r, e))?;
        Ok(Value::Undefined)
    });
    method(realm, "logLastLog", 1, |r, h, _, args| {
        let message = r.to_rust_string(&arg(args, 0))?;
        h.socket.log_last_log(&message);
        Ok(Value::Undefined)
    });
    method(realm, "onClose", 1, |r, h, _, args| {
        let event = arg(args, 0);
        let (code, reason) = if event.object().is_some() {
            let code = r.get_str(&event, "code")?;
            let reason = r.get_str(&event, "reason")?;
            let code = code.as_number().map(|c| c as u16);
            let reason = if reason.is_nullish() {
                String::new()
            } else {
                r.to_rust_string(&reason)?
            };
            (code, reason)
        } else {
            (None, String::new())
        };
        h.socket.on_close(code, &reason).map_err(|e| thrown(r, e))?;
        Ok(Value::Undefined)
    });
    method(realm, "dispose", 0, |_, h, _, _| {
        h.socket.dispose();
        Ok(Value::Undefined)
    });
    method(realm, "emitAndLogError", 3, |r, _, this, args| {
        let parent = r.get_str(this, "parent")?;
        r.call_method(&parent, "emitAndLogError", args)
    });
    getter(realm, "isConnected", |_, h, _, _| {
        Ok(Value::Bool(h.socket.is_connected()))
    });
    getter(realm, "isDisposed", |_, h, _, _| {
        Ok(Value::Bool(h.socket.is_disposed()))
    });
    getter(realm, "store", |r, _, this, _| {
        let parent = r.get_str(this, "parent")?;
        r.get_str(&parent, "store")
    });
    getter(realm, "emitter", |r, _, this, _| {
        let parent = r.get_str(this, "parent")?;
        r.get_str(&parent, "emitter")
    });
    class
}
