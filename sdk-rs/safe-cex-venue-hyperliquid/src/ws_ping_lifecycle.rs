//! Public/private websocket opening and deferred ping algorithms.
//! This venue-local contract binds JS properties, calls and timer registration;
//! the timer callback body is `run_ping_timeout`, not a host implementation.
use crate::spot_json::JsString;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketKind {
    Public,
    Private,
}
#[derive(Clone, Debug)]
pub struct PingCallback<V> {
    pub socket: V,
    pub kind: SocketKind,
}
pub trait WsPingHost {
    /// Cloning an object/function handle must retain its native identity.
    type Value: Clone;
    type Error;
    fn undefined(&self) -> Self::Value;
    fn literal(&self, text: &JsString) -> Self::Value;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn global(&mut self, name: &str) -> Result<Self::Value, Self::Error>;
    fn get(&mut self, object: &Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    /// Strict source-class assignment, including setter/rejection effects.
    fn set(
        &mut self,
        object: &Self::Value,
        name: &str,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn call(
        &mut self,
        function: &Self::Value,
        receiver: &Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    fn object(&mut self) -> Result<Self::Value, Self::Error>;
    fn define_data(
        &mut self,
        object: &Self::Value,
        name: &str,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    /// Allocate a lexical callback capturing the socket handle. A timer adapter
    /// invokes `run_ping_timeout` with this capture when the registered timer fires.
    fn callback(&mut self, capture: PingCallback<Self::Value>) -> Result<Self::Value, Self::Error>;
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn method<H: WsPingHost>(
    h: &mut H,
    object: &H::Value,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let function = h.get(object, name)?;
    h.call(&function, object, args)
}
fn start<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    method(h, socket, "stopPingLoop", &[])?;
    method(h, socket, "schedulePing", &[])?;
    Ok(())
}
fn schedule<H: WsPingHost>(h: &mut H, socket: &H::Value, kind: SocketKind) -> Result<(), H::Error> {
    let function = h.global("setTimeout")?;
    let callback = h.callback(PingCallback {
        socket: socket.clone(),
        kind,
    })?;
    let class = h.global(match kind {
        SocketKind::Public => "HyperliquidPublicWebSocket",
        SocketKind::Private => "HyperliquidPrivateWebSocket",
    })?;
    let interval = h.get(&class, "PING_INTERVAL_MS")?;
    let receiver = h.undefined();
    let id = h.call(&function, &receiver, &[callback, interval])?;
    h.set(socket, "hlPingTimeoutId", &id)
}
pub fn run_ping_timeout<H: WsPingHost>(
    h: &mut H,
    capture: &PingCallback<H::Value>,
) -> Result<(), H::Error> {
    let disposed = h.get(&capture.socket, "isDisposed")?;
    if !h.truthy(&disposed) {
        let connected = h.get(&capture.socket, "isConnected")?;
        if h.truthy(&connected) {
            method(h, &capture.socket, "ping", &[])?;
            method(h, &capture.socket, "schedulePing", &[])?;
        }
    }
    Ok(())
}
fn stop<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    let first = h.get(socket, "hlPingTimeoutId")?;
    if h.truthy(&first) {
        let function = h.global("clearTimeout")?;
        let id = h.get(socket, "hlPingTimeoutId")?;
        let receiver = h.undefined();
        h.call(&function, &receiver, &[id])?;
        let undefined = h.undefined();
        h.set(socket, "hlPingTimeoutId", &undefined)?;
    }
    Ok(())
}
fn ping<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    let disposed = h.get(socket, "isDisposed")?;
    if !h.truthy(&disposed) {
        let performance = h.global("performance")?;
        let now = method(h, &performance, "now", &[])?;
        h.set(socket, "pingAt", &now)?;
        // Resolve the sender before evaluating JSON.stringify and its literal.
        let sender = h.get(socket, "sendWithDiagnostics")?;
        let json = h.global("JSON")?;
        let stringify = h.get(&json, "stringify")?;
        let payload = h.object()?;
        let value = h.literal(&text("ping"));
        h.define_data(&payload, "method", &value)?;
        let data = h.call(&stringify, &json, &[payload])?;
        h.call(&sender, socket, &[data])?;
    }
    Ok(())
}
pub fn public_on_open<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    let disposed = h.get(socket, "isDisposed")?;
    if !h.truthy(&disposed) {
        let parent = h.get(socket, "parent")?;
        let log = h.get(&parent, "log")?;
        let message = h.literal(&text("Public WebSocket connection established"));
        let severity = h.global("LogSeverity")?;
        let debug = h.get(&severity, "Debug")?;
        h.call(&log, &parent, &[message, debug])?;
        method(h, socket, "trackLastReceived", &[])?;
        method(h, socket, "resetOrderbook", &[])?;
        let active = h.get(socket, "activeSubscriptions")?;
        method(h, &active, "clear", &[])?;
        method(h, socket, "subscribePublicTopics", &[])?;
        method(h, socket, "startPingLoop", &[])?;
    }
    Ok(())
}
pub fn private_on_open<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    let disposed = h.get(socket, "isDisposed")?;
    if !h.truthy(&disposed) {
        method(h, socket, "trackLastReceived", &[])?;
        method(h, socket, "subscribe", &[])?;
        method(h, socket, "startPingLoop", &[])?;
    }
    Ok(())
}
pub fn public_start_ping_loop<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    start(h, socket)
}
pub fn private_start_ping_loop<H: WsPingHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    start(h, socket)
}
pub fn public_schedule_ping<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    schedule(h, socket, SocketKind::Public)
}
pub fn private_schedule_ping<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    schedule(h, socket, SocketKind::Private)
}
pub fn public_stop_ping_loop<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    stop(h, socket)
}
pub fn private_stop_ping_loop<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    stop(h, socket)
}
pub fn public_ping<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    ping(h, socket)
}
pub fn private_ping<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    ping(h, socket)
}
