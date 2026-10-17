use super::hosts::{Host, error};
use crate::{
    listen_key::{ListenKeyHost, PrivateListener},
    private_dispatch::PrivateMessageHost,
    private_events::PrivateEventHost,
    public_stream::{PublicFrame, PublicSocketHost, SocketListener},
    stream_subscriptions::SubscriptionHost,
    websocket::{CloseCallback, PrivateCallbacks, WebsocketBindings},
};
use safe_cex_exchange::{
    js_runtime::{
        promise::{Promise, await_js},
        value::{Js, Thrown, arg, from_json, to_json},
    },
    runtime::{ExchangeError, ExchangeFuture},
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
struct BlobLease {
    blobs: Rc<RefCell<HashMap<u64, Js>>>,
    id: u64,
}
impl Drop for BlobLease {
    fn drop(&mut self) {
        self.blobs.borrow_mut().remove(&self.id);
    }
}

impl Host {
    pub fn bind_protocols(&self) -> Result<(), ExchangeError> {
        let weak = self.inner.borrow().clone();
        self.public
            .define(
                "connectAndSubscribe",
                Js::function("connectAndSubscribe", move |_, _| {
                    if let Some(inner) = weak.upgrade() {
                        inner
                            .public_websocket
                            .stream
                            .connect_and_subscribe()
                            .map_err(Thrown::from)?;
                    }
                    Ok(Js::Undefined)
                }),
            )
            .map_err(error)?;
        let weak = self.inner.borrow().clone();
        self.public
            .define(
                "ping",
                Js::function("ping", move |_, _| {
                    if let Some(inner) = weak.upgrade() {
                        inner.public_websocket.stream.ping().map_err(Thrown::from)?;
                    }
                    Ok(Js::Undefined)
                }),
            )
            .map_err(error)?;
        let weak = self.inner.borrow().clone();
        let runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> =
            self.scope.runtime.clone();
        self.private
            .define(
                "connectAndSubscribe",
                Js::function("connectAndSubscribe", move |_, _| {
                    let inner = weak.upgrade();
                    Ok(Js::Promise(Promise::spawn(
                        &runtime,
                        Box::pin(async move {
                            if let Some(inner) = inner {
                                inner.private_websocket.keys.connect_and_subscribe().await?;
                            }
                            Ok(Js::Undefined)
                        }),
                    )))
                }),
            )
            .map_err(error)?;
        let weak = self.inner.borrow().clone();
        self.private
            .define(
                "ping",
                Js::function("ping", move |_, _| {
                    if let Some(inner) = weak.upgrade() {
                        let host = &inner.private_websocket.keys.host;
                        crate::private_events::private_ping(
                            host.is_disposed(),
                            || host.monotonic_now_ms(),
                            |at| host.set_ping_at(at),
                            |frame| host.send_with_log(frame),
                        )
                        .map_err(Thrown::from)?;
                    }
                    Ok(Js::Undefined)
                }),
            )
            .map_err(error)?;
        Ok(())
    }
    fn socket_call(&self, private: bool, member: &str, args: Vec<Js>) -> Result<(), ExchangeError> {
        let socket = if private { &self.private } else { &self.public };
        let result = socket.call_method(member, args).map_err(error)?;
        if member == "createWebsocket" {
            socket.set("ws", result).map_err(error)?;
        }
        Ok(())
    }
    fn public_listener(&self, event: &str, callback: Js) -> Result<(), ExchangeError> {
        self.public
            .get("ws")
            .and_then(|ws| ws.call_method("addEventListener", vec![Js::str(event), callback]))
            .map_err(error)?;
        Ok(())
    }
    fn message_callback(
        &self,
        callback: Rc<dyn Fn(PublicFrame) -> ExchangeFuture<'static, ()>>,
    ) -> Js {
        let runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> =
            self.scope.runtime.clone();
        let blobs = self.blobs.clone();
        let next = self.next.clone();
        Js::function("onMessage", move |_, args| {
            let data = arg(&args, 0).get("data")?;
            let (frame, lease) = if matches!(&data, Js::Object(object) if object.class == "Blob" || object.class == "Buffer")
            {
                let id = next.get() + 1;
                next.set(id);
                blobs.borrow_mut().insert(id, data);
                (
                    PublicFrame::Blob(id),
                    Some(BlobLease {
                        blobs: blobs.clone(),
                        id,
                    }),
                )
            } else {
                (PublicFrame::Text(data.to_display()), None)
            };
            let future = callback(frame);
            Ok(Js::Promise(Promise::spawn(
                &runtime,
                Box::pin(async move {
                    let _lease = lease;
                    future.await?;
                    Ok(Js::Undefined)
                }),
            )))
        })
    }
    fn decode_blob(&self, blob: u64) -> ExchangeFuture<'_, String> {
        let data = self.blobs.borrow().get(&blob).cloned();
        Box::pin(async move {
            let data = data.ok_or_else(|| ExchangeError::new("BingX binary frame was released"))?;
            let buffer = if matches!(&data,Js::Object(object) if object.class == "Blob") {
                await_js(data.call_method("arrayBuffer", vec![]).map_err(error)?)
                    .await
                    .map_err(error)?
            } else {
                data
            };
            let bytes = buffer
                .get("bytes")
                .map_err(error)?
                .array_values()
                .ok_or_else(|| ExchangeError::new("BingX binary frame has no byte buffer"))?
                .iter()
                .map(|byte| {
                    byte.as_f64()
                        .filter(|n| n.fract() == 0.0 && (0.0..=255.0).contains(n))
                        .map(|n| n as u8)
                        .ok_or_else(|| ExchangeError::new("BingX binary byte is invalid"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            safe_cex_store::read_blob::read_blob(async move { Ok(bytes) })
                .await
                .map_err(ExchangeError::new)
        })
    }
    fn report(&self, e: &ExchangeError) {
        self.scope
            .runtime
            .report_exception(e, &json!({"venue":"bingx"}));
    }
}
impl PublicSocketHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed()
    }
    fn nanoid(&self) -> String {
        self.scope.runtime.unique_id()
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.scope.runtime.monotonic_now_ms()
    }
    fn iso_time(&self) -> Result<String, ExchangeError> {
        self.global("dayjs", vec![])?
            .call_method("toISOString", vec![])
            .map(|v| v.to_display())
            .map_err(error)
    }
    fn create_socket(&self, url: &str) -> Result<(), ExchangeError> {
        self.socket_call(false, "createWebsocket", vec![Js::str(url)])
    }
    fn add_listener(&self, listener: SocketListener) -> Result<(), ExchangeError> {
        match listener {
            SocketListener::Open(callback) => self.public_listener(
                "open",
                Js::function("onOpen", move |_, _| {
                    callback().map_err(Thrown::from)?;
                    Ok(Js::Undefined)
                }),
            ),
            SocketListener::Message(callback) => {
                self.public_listener("message", self.message_callback(callback))
            }
            SocketListener::Close => {
                self.public_listener("close", self.public.get("onClose").map_err(error)?)
            }
        }
    }
    fn send_with_log(&self, frame: &str) -> Result<(), ExchangeError> {
        self.socket_call(false, "sendWithLog", vec![Js::str(frame)])
    }
    fn set_ping_at(&self, at: f64) -> Result<(), ExchangeError> {
        self.public.set("pingAt", Js::Number(at)).map_err(error)
    }
    fn track_last_received(&self) -> Result<(), ExchangeError> {
        self.socket_call(false, "trackLastReceived", vec![])
    }
    fn handle_pong(&self) -> Result<(), ExchangeError> {
        self.socket_call(false, "handlePongEvent", vec![])
    }
    fn record_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.core(false)?.record_message_handler_stats(topic)
    }
    fn diagnostic_receive(
        &self,
        raw: &str,
        allow: &dyn Fn() -> bool,
        format: &dyn Fn() -> Result<Option<String>, ExchangeError>,
    ) -> Result<(), ExchangeError> {
        if !self.scope.runtime.diagnostics_enabled() || !allow() {
            return Ok(());
        }
        let formatted = format()?;
        self.core(false)?.emit_ws_diagnostic_receive(
            &Value::String(raw.into()),
            &safe_cex_exchange::websocket::WsDiagnosticReceiveOptions {
                format: Some(Rc::new(move |_| formatted.clone())),
                ..Default::default()
            },
        )
    }
    fn read_blob(&self, blob: u64) -> ExchangeFuture<'_, String> {
        self.decode_blob(blob)
    }
    fn report_exception(&self, e: &ExchangeError) {
        self.report(e)
    }
}
impl SubscriptionHost for Host {
    fn is_connected(&self) -> bool {
        self.cores(false).first().is_some_and(|c| c.is_connected())
    }
    fn set_timeout(
        &self,
        delay: f64,
        callback: Rc<dyn Fn() -> Result<(), ExchangeError>>,
    ) -> Result<u64, ExchangeError> {
        self.timer(delay, callback)
    }
    fn clear_timeout(&self, id: u64) -> Result<(), ExchangeError> {
        self.clear_timer(id);
        Ok(())
    }
    fn cleanup_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.core(false)?.cleanup_message_handler_stats(topic)
    }
}
impl ListenKeyHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed()
    }
    fn testnet(&self) -> bool {
        self.base().options().testnet == Some(true)
    }
    fn create_socket(&self, url: &str) -> Result<u64, ExchangeError> {
        self.private_listeners.borrow_mut().clear();
        self.socket_call(true, "createWebsocket", vec![Js::str(url)])?;
        Ok(self.next.get() + 1)
    }
    fn add_listener(&self, _socket: u64, listener: PrivateListener) -> Result<(), ExchangeError> {
        let name = match listener {
            PrivateListener::Message => "message",
            PrivateListener::Close => "close",
            PrivateListener::Open => "open",
        };
        let member = match listener {
            PrivateListener::Message => "onMessage",
            PrivateListener::Close => "onClose",
            PrivateListener::Open => "onOpen",
        };
        let callback = self.private.get(member).map_err(error)?;
        self.private
            .get("ws")
            .and_then(|ws| {
                ws.call_method("addEventListener", vec![Js::str(name), callback.clone()])
            })
            .map_err(error)?;
        self.private_listeners.borrow_mut().push((name, callback));
        Ok(())
    }
    fn remove_listener(
        &self,
        _socket: u64,
        listener: PrivateListener,
    ) -> Result<(), ExchangeError> {
        let name = match listener {
            PrivateListener::Message => "message",
            PrivateListener::Close => "close",
            PrivateListener::Open => "open",
        };
        let callbacks: Vec<_> = self
            .private_listeners
            .borrow()
            .iter()
            .filter(|(event, _)| *event == name)
            .map(|(_, callback)| callback.clone())
            .collect();
        for callback in callbacks {
            self.private
                .get("ws")
                .and_then(|ws| ws.call_method("removeEventListener", vec![Js::str(name), callback]))
                .map_err(error)?;
        }
        self.private_listeners
            .borrow_mut()
            .retain(|(event, _)| *event != name);
        Ok(())
    }
    fn close_socket(&self, _socket: u64) -> Result<(), ExchangeError> {
        self.private
            .get("ws")
            .and_then(|ws| ws.call_method("close", vec![]))
            .map_err(error)?;
        Ok(())
    }
    fn retry_failed_connect(&self, message: &str) -> Result<(), ExchangeError> {
        self.core(true)?.retry_failed_connect(message)
    }
    fn warning(&self, message: &str) -> Result<(), ExchangeError> {
        self.base().log(message, "warning")
    }
    fn set_timeout(
        &self,
        delay: f64,
        callback: Rc<dyn Fn() -> Result<(), ExchangeError>>,
    ) -> Result<u64, ExchangeError> {
        self.timer(delay, callback)
    }
    fn clear_timeout(&self, id: u64) -> Result<(), ExchangeError> {
        self.clear_timer(id);
        Ok(())
    }
    fn spawn(&self, future: ExchangeFuture<'static, ()>) -> Result<(), ExchangeError> {
        Host::spawn(self, future);
        Ok(())
    }
    fn monotonic_now_ms(&self) -> Result<f64, ExchangeError> {
        Ok(self.scope.runtime.monotonic_now_ms())
    }
    fn set_ping_at(&self, at: f64) -> Result<(), ExchangeError> {
        self.private.set("pingAt", Js::Number(at)).map_err(error)
    }
    fn send_with_log(&self, frame: &str) -> Result<(), ExchangeError> {
        self.socket_call(true, "sendWithLog", vec![Js::str(frame)])
    }
    fn base_dispose(&self) -> Result<(), ExchangeError> {
        self.core(true)?.dispose();
        Ok(())
    }
    fn report_exception(&self, e: &ExchangeError) {
        self.report(e)
    }
}
impl PrivateEventHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed()
    }
    fn now_ms(&self) -> f64 {
        self.scope.runtime.now_ms()
    }
    fn set_timeout(
        &self,
        delay: f64,
        callback: Rc<dyn Fn() -> Result<(), ExchangeError>>,
    ) -> Result<(), ExchangeError> {
        self.timer(delay, callback)?;
        Ok(())
    }
    fn spawn(&self, future: ExchangeFuture<'static, ()>) -> Result<(), ExchangeError> {
        Host::spawn(self, future);
        Ok(())
    }
    fn emit_fill(&self, payload: Value) -> Result<(), ExchangeError> {
        self.base().emit(
            "fill",
            safe_cex_exchange::js_runtime::value::from_json(&payload),
        )
    }
    fn report_exception(&self, e: &ExchangeError) {
        self.report(e)
    }
}
impl PrivateMessageHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed()
    }
    fn track_last_received(&self) -> Result<(), ExchangeError> {
        self.socket_call(true, "trackLastReceived", vec![])
    }
    fn read_blob(&self, blob: u64) -> ExchangeFuture<'_, String> {
        self.decode_blob(blob)
    }
    fn diagnostic_receive(&self, raw: &str) -> Result<(), ExchangeError> {
        self.core(true)?
            .emit_ws_diagnostic_receive(&Value::String(raw.into()), &Default::default())
    }
    fn send_with_log(&self, raw: &str) -> Result<(), ExchangeError> {
        self.socket_call(true, "sendWithLog", vec![Js::str(raw)])
    }
    fn handle_pong(&self) -> Result<(), ExchangeError> {
        self.socket_call(true, "handlePongEvent", vec![])
    }
    fn record_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.core(true)?.record_message_handler_stats(topic)
    }
}
impl WebsocketBindings for Host {
    fn bind_private(&self, callbacks: PrivateCallbacks) -> Result<(), ExchangeError> {
        let open = callbacks.open;
        self.private
            .define(
                "onOpen",
                Js::function("onOpen", move |_, _| {
                    open().map_err(Thrown::from)?;
                    Ok(Js::Undefined)
                }),
            )
            .map_err(error)?;
        self.private
            .define("onMessage", self.message_callback(callbacks.message))
            .map_err(error)?;
        let close = callbacks.close;
        self.private
            .define(
                "onClose",
                Js::function("onClose", move |_, args| {
                    let event = arg(&args, 0);
                    let event = if event.is_undefined() {
                        None
                    } else {
                        Some(to_json(&event)?)
                    };
                    close(event).map_err(Thrown::from)?;
                    Ok(Js::Undefined)
                }),
            )
            .map_err(error)?;
        Ok(())
    }
    fn bind_public_close(&self, callback: CloseCallback) -> Result<(), ExchangeError> {
        self.public
            .define(
                "onClose",
                Js::function("onClose", move |_, args| {
                    let event = arg(&args, 0);
                    let event = if event.is_undefined() {
                        None
                    } else {
                        Some(to_json(&event)?)
                    };
                    callback(event).map_err(Thrown::from)?;
                    Ok(Js::Undefined)
                }),
            )
            .map_err(error)?;
        Ok(())
    }
    fn public_base_close(&self, event: Option<Value>) -> Result<(), ExchangeError> {
        self.public_close
            .call(
                self.public.clone(),
                vec![event.as_ref().map_or(Js::Undefined, from_json)],
            )
            .map_err(error)?;
        Ok(())
    }
    fn private_base_close(&self, event: Option<Value>) -> Result<(), ExchangeError> {
        self.private_close
            .call(
                self.private.clone(),
                vec![event.as_ref().map_or(Js::Undefined, from_json)],
            )
            .map_err(error)?;
        Ok(())
    }
    fn public_base_dispose(&self) -> Result<(), ExchangeError> {
        self.core(false)?.dispose();
        Ok(())
    }
}
