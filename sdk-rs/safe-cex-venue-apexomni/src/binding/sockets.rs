use super::hosts::{Host, error};
use crate::socket_host::{ApexSocketHost, SocketListener, SocketTask};
use safe_cex_exchange::{
    js_runtime::{
        promise::Promise,
        value::{Js, Thrown, arg},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
};
use safe_cex_num::JsJsonValue as J;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};

pub(super) struct SocketHost {
    pub owner: Weak<Host>,
    pub private: bool,
}
impl SocketHost {
    fn owner(&self) -> Result<Rc<Host>, ExchangeError> {
        self.owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Apex socket host released"))
    }
    fn socket<'a>(&self, owner: &'a Host) -> &'a Js {
        if self.private {
            &owner.private
        } else {
            &owner.public
        }
    }
    fn call(&self, method: &str, args: Vec<Js>) -> Result<(), ExchangeError> {
        self.socket(self.owner()?.as_ref())
            .call_method(method, args)
            .map_err(error)?;
        Ok(())
    }
}
impl Host {
    pub fn bind_protocols(&self) -> Result<(), ExchangeError> {
        let public = Rc::downgrade(self.public_body.borrow().as_ref().expect("public body"));
        let reconnect_public = public.clone();
        self.public
            .define(
                "connectAndSubscribe",
                Js::function("connectAndSubscribe", move |_, args| {
                    if let Some(public) = reconnect_public.upgrade() {
                        public
                            .connect_and_subscribe(arg(&args, 0).truthy())
                            .map_err(Thrown::from)?;
                    }
                    Ok(Js::Undefined)
                }),
            )
            .map_err(error)?;
        self.public
            .define(
                "ping",
                Js::function("ping", move |_, _| {
                    if let Some(public) = public.upgrade() {
                        public.ping().map_err(Thrown::from)?;
                    }
                    Ok(Js::Undefined)
                }),
            )
            .map_err(error)?;
        let private = Rc::downgrade(self.private_body.borrow().as_ref().expect("private body"));
        let reconnect_private = private.clone();
        self.private
            .define(
                "connectAndSubscribe",
                Js::function("connectAndSubscribe", move |_, _| {
                    if let Some(private) = reconnect_private.upgrade() {
                        private.connect_and_subscribe().map_err(Thrown::from)?;
                    }
                    Ok(Js::Undefined)
                }),
            )
            .map_err(error)?;
        self.private
            .define(
                "ping",
                Js::function("ping", move |_, _| {
                    if let Some(private) = private.upgrade() {
                        private.ping().map_err(Thrown::from)?;
                    }
                    Ok(Js::Undefined)
                }),
            )
            .map_err(error)?;
        Ok(())
    }
}
impl ApexSocketHost for SocketHost {
    fn initialize(&self, _: bool, treat: bool) -> Result<(), ExchangeError> {
        self.socket(self.owner()?.as_ref())
            .define("treatAnyMessageAsPong", Js::Bool(treat))
            .map_err(error)
    }
    fn is_disposed(&self) -> bool {
        self.owner.upgrade().is_none_or(|o| o.base().is_disposed())
    }
    fn is_connected(&self) -> bool {
        self.owner()
            .and_then(|o| o.core(self.private))
            .is_ok_and(|c| c.is_connected())
    }
    fn now_ms(&self) -> f64 {
        safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()
    }
    fn date_now_ms(&self) -> f64 {
        self.owner
            .upgrade()
            .map_or(0., |o| o.scope.runtime.now_ms())
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.owner
            .upgrade()
            .map_or(0., |o| o.scope.runtime.monotonic_now_ms())
    }
    fn nanoid(&self) -> String {
        self.owner()
            .and_then(|o| o.global("nanoid", vec![]))
            .map(|v| v.to_display())
            .unwrap_or_default()
    }
    fn enable_reconnecting(&self) -> bool {
        self.owner
            .upgrade()
            .is_some_and(|o| o.scope.runtime.enable_reconnecting())
    }
    fn private_route(
        &self,
        name: &ExchangeName,
        options: &ExchangeOptions,
    ) -> Result<Option<String>, ExchangeError> {
        let route = self
            .owner()?
            .scope
            .runtime
            .transport()
            .ws_routing(name, &safe_cex_transport::RoutingOptions::from(options));
        if route.ws_private.tunnelled {
            route
                .ws_private
                .host
                .map(Some)
                .ok_or_else(|| ExchangeError::new("Apex private WS tunnel host unavailable"))
        } else {
            Ok(None)
        }
    }
    fn create_socket(&self, url: &str) -> Result<(), ExchangeError> {
        let owner = self.owner()?;
        let socket = self.socket(&owner);
        let ws = socket
            .call_method("createWebsocket", vec![Js::str(url)])
            .map_err(error)?;
        socket.set("ws", ws).map_err(error)
    }
    fn add_listener(&self, listener: SocketListener) -> Result<(), ExchangeError> {
        let owner = self.owner()?;
        let socket = self.socket(&owner);
        let runtime: Rc<dyn ExchangeRuntime> = owner.scope.runtime.clone();
        let private = self.private;
        let weak = self.owner.clone();
        let (event, callback) = match listener {
            SocketListener::Open(callback) => (
                "open",
                Js::function("onOpen", move |_, _| {
                    let mut future = callback();
                    let promise = Promise::pending(&runtime);
                    match future
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop()))
                    {
                        Poll::Ready(result) => {
                            promise.settle(result.map(|()| Js::Undefined).map_err(Thrown::from))
                        }
                        Poll::Pending => {
                            let owner = weak
                                .upgrade()
                                .ok_or_else(|| Thrown::error("Error", "Apex host released"))?;
                            // The eager first poll may synchronously dispose the venue.
                            // Do not register new work after its cancellation sweep.
                            if owner.base().is_disposed() {
                                promise.settle(Ok(Js::Undefined));
                                return Ok(Js::Promise(promise));
                            }
                            let result = promise.clone();
                            owner.schedule(
                                0.,
                                Box::pin(async move {
                                    result.settle(
                                        future.await.map(|()| Js::Undefined).map_err(Thrown::from),
                                    );
                                    Ok(())
                                }),
                            );
                        }
                    }
                    Ok(Js::Promise(promise))
                }),
            ),
            SocketListener::Message(callback) => (
                "message",
                Js::function("onMessage", move |_, args| {
                    // The typed protocol consumes text; Buffer's explicit conversion
                    // uses the shared bridge, rather than transport-side UTF-8 loss.
                    let data = arg(&args, 0).get("data")?;
                    if !private && matches!(&data, Js::Object(o) if o.class == "Blob") {
                        let owner = weak
                            .upgrade()
                            .ok_or_else(|| Thrown::error("Error", "Apex host released"))?;
                        if owner.base().is_disposed() {
                            return Ok(Js::Undefined);
                        }
                        owner
                            .core(false)
                            .map_err(Thrown::from)?
                            .track_last_received()
                            .map_err(Thrown::from)?;
                        // Source public onMessage calls data.includes before JSON
                        // parsing; a browser Blob has no such method.
                        return Err(Thrown::type_error("data.includes is not a function"));
                    }
                    let data = data.to_js_string()?;
                    callback(&String::from_utf16_lossy(&data)).map_err(Thrown::from)?;
                    Ok(Js::Undefined)
                }),
            ),
            SocketListener::Close => ("close", socket.get("onClose").map_err(error)?),
        };
        socket
            .get("ws")
            .and_then(|w| w.call_method("addEventListener", vec![Js::str(event), callback]))
            .map_err(error)?;
        Ok(())
    }
    fn send(&self, frame: &str, with_log: bool) -> Result<(), ExchangeError> {
        self.call(
            if with_log { "sendWithLog" } else { "send" },
            vec![Js::str(frame)],
        )
    }
    fn set_ping_at(&self, at: f64) -> Result<(), ExchangeError> {
        self.socket(self.owner()?.as_ref())
            .set("pingAt", Js::Number(at))
            .map_err(error)
    }
    fn schedule_ping(&self, delay_ms: u64) -> Result<(), ExchangeError> {
        self.owner()?.core(self.private)?.schedule_ping(delay_ms);
        Ok(())
    }
    fn track_last_received(&self) -> Result<(), ExchangeError> {
        self.call("trackLastReceived", vec![])
    }
    fn diagnostic_receive(&self, frame: &str) -> Result<(), ExchangeError> {
        self.call("emitWsDiagnosticReceive", vec![Js::str(frame)])
    }
    fn record_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.owner()?
            .core(self.private)?
            .record_message_handler_stats(topic)
    }
    fn cleanup_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.owner()?
            .core(self.private)?
            .cleanup_message_handler_stats(topic)
    }
    fn handle_pong(&self, next: Option<u64>) -> Result<(), ExchangeError> {
        self.call(
            "handlePongEvent",
            next.map(|n| Js::Number(n as f64)).into_iter().collect(),
        )
    }
    fn emit_reconnected(&self) -> Result<(), ExchangeError> {
        self.call("emitWsReconnected", vec![])
    }
    fn reconnect(&self) -> Result<(), ExchangeError> {
        self.call("reconnect", vec![])
    }
    fn report_error(&self, error: ExchangeError) -> Result<(), ExchangeError> {
        self.owner()?.base().emit_and_log_error(
            safe_cex_exchange::error_reporting::ExchangeErrorInput::Error(Box::new(error)),
            None,
            None,
        )
    }
    fn warn(&self, message: &str, argument: &J) -> Result<(), ExchangeError> {
        self.owner()?
            .scope
            .globals
            .get("console")
            .and_then(|c| c.call_method("warn", vec![Js::str(message), super::hosts::js(argument)]))
            .map_err(error)?;
        Ok(())
    }
    fn set_timeout(&self, delay: u64, callback: SocketTask) -> Result<u64, ExchangeError> {
        Ok(self
            .owner()?
            .schedule(delay as f64, Box::pin(async move { callback().await })))
    }
    fn clear_timeout(&self, timer: u64) -> Result<(), ExchangeError> {
        self.owner()?.clear(timer);
        Ok(())
    }
    fn spawn(&self, future: ExchangeFuture<'static, ()>) -> Result<(), ExchangeError> {
        self.owner()?.schedule(0., future);
        Ok(())
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.call("dispose", vec![])
    }
    fn remote_signer_account(&self) -> Option<J> {
        self.owner
            .upgrade()?
            .scope
            .http
            .hosted_signer()
            .map(|(_, account)| account.map_or(J::Undefined, J::String))
    }
    fn remote_sign(&self, request: J) -> ExchangeFuture<'_, J> {
        // Clone only the capability before awaiting: an owned socket task must
        // not retain the host that owns its cancellation handle.
        let signer = self.owner().and_then(|owner| {
            owner
                .scope
                .http
                .hosted_signer()
                .map(|(signer, _)| signer)
                .ok_or_else(|| ExchangeError::new("Apex remote signer capability unavailable"))
        });
        let request = safe_cex_exchange::js_runtime::value::to_json(&super::hosts::js(&request))
            .map_err(error)
            .and_then(|value| {
                serde_json::from_value::<safe_cex_transport::signing::RemoteSignerRequest>(value)
                    .map_err(|e| ExchangeError::new(e.to_string()))
            });
        Box::pin(async move {
            let response = signer?
                .sign(request?)
                .await
                .map_err(|e| ExchangeError::new(e.to_string()))?;
            serde_json::to_value(&response)
                .map(J::from)
                .map_err(|e| ExchangeError::new(e.to_string()))
        })
    }
}
