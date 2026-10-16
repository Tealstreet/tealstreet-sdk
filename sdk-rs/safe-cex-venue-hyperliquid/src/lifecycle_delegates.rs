//! Listener forwarding, disposal, and debounced refresh lifecycle leaves.
//! Primitive bindings and virtual websocket/callee implementations are supplied
//! by the venue host. Timer and rejection callback bodies are owned here.
use crate::{spot_json::JsString, ws_ping_lifecycle::WsPingHost};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleCallbackKind {
    OrderRefreshTimeout,
    OrderRefreshError,
}
#[derive(Clone, Debug)]
pub struct LifecycleCallback<V> {
    pub exchange: V,
    pub kind: LifecycleCallbackKind,
}
pub trait LifecycleHost: WsPingHost {
    fn null(&self) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn is_nullish(&self, value: &Self::Value) -> bool;
    /// Source super property Get uses the method's home-object prototype and
    /// the original receiver, including getter/throw effects.
    fn super_get(&mut self, receiver: &Self::Value, name: &str)
    -> Result<Self::Value, Self::Error>;
    fn lifecycle_callback(
        &mut self,
        capture: LifecycleCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn subtract(
        &mut self,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn greater_than(
        &mut self,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<bool, Self::Error>;
    /// Template-substitution ToString (not the global String function).
    fn template_string(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
}
fn method<H: WsPingHost>(
    h: &mut H,
    receiver: &H::Value,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let function = h.get(receiver, name)?;
    h.call(&function, receiver, args)
}
fn forward<H: WsPingHost>(
    h: &mut H,
    exchange: &H::Value,
    name: &str,
    first: &H::Value,
    callback: &H::Value,
) -> Result<H::Value, H::Error> {
    let socket = h.get(exchange, "publicWebsocket")?;
    method(h, &socket, name, &[first.clone(), callback.clone()])
}
pub fn core_listen_ohlcv<H: WsPingHost>(
    h: &mut H,
    exchange: &H::Value,
    options: &H::Value,
    callback: &H::Value,
) -> Result<H::Value, H::Error> {
    forward(h, exchange, "listenOHLCV", options, callback)
}
pub fn core_listen_order_book<H: WsPingHost>(
    h: &mut H,
    exchange: &H::Value,
    symbol: &H::Value,
    callback: &H::Value,
) -> Result<H::Value, H::Error> {
    forward(h, exchange, "listenOrderBook", symbol, callback)
}
pub fn spot_listen_ohlcv<H: WsPingHost>(
    h: &mut H,
    exchange: &H::Value,
    options: &H::Value,
    callback: &H::Value,
) -> Result<H::Value, H::Error> {
    forward(h, exchange, "listenOHLCV", options, callback)
}
pub fn spot_listen_order_book<H: WsPingHost>(
    h: &mut H,
    exchange: &H::Value,
    symbol: &H::Value,
    callback: &H::Value,
) -> Result<H::Value, H::Error> {
    forward(h, exchange, "listenOrderBook", symbol, callback)
}
fn clear_timer<H: LifecycleHost>(
    h: &mut H,
    exchange: &H::Value,
    name: &str,
) -> Result<(), H::Error> {
    let first = h.get(exchange, name)?;
    if h.truthy(&first) {
        let clear = h.global("clearTimeout")?;
        let id = h.get(exchange, name)?;
        let undefined = h.undefined();
        h.call(&clear, &undefined, &[id])?;
        let null = h.null();
        h.set(exchange, name, &null)?;
    }
    Ok(())
}
fn super_dispose<H: LifecycleHost>(h: &mut H, receiver: &H::Value) -> Result<(), H::Error> {
    let dispose = h.super_get(receiver, "dispose")?;
    h.call(&dispose, receiver, &[])?;
    Ok(())
}
pub fn core_dispose<H: LifecycleHost>(h: &mut H, exchange: &H::Value) -> Result<(), H::Error> {
    let null = h.null();
    h.set(exchange, "_fetchOrdersInFlight", &null)?;
    let tif = h.get(exchange, "_placedLimitOrderTif")?;
    method(h, &tif, "clear", &[])?;
    clear_timer(h, exchange, "_orderRefreshDebounceTimer")?;
    clear_timer(h, exchange, "_hip3PositionRefreshDebounceTimer")?;
    let pending = h.get(exchange, "_pendingHip3PositionRefreshDexes")?;
    method(h, &pending, "clear", &[])?;
    clear_timer(h, exchange, "_retryChainTimer")?;
    method(h, exchange, "endRetryChain", &[])?;
    let public = h.get(exchange, "publicWebsocket")?;
    if !h.is_nullish(&public) {
        method(h, &public, "dispose", &[])?;
    }
    let private = h.get(exchange, "privateWebsocket")?;
    if !h.is_nullish(&private) {
        method(h, &private, "dispose", &[])?;
    }
    super_dispose(h, exchange)
}
pub fn spot_dispose<H: LifecycleHost>(h: &mut H, exchange: &H::Value) -> Result<(), H::Error> {
    super_dispose(h, exchange)?;
    let public = h.get(exchange, "publicWebsocket")?;
    method(h, &public, "dispose", &[])?;
    let private = h.get(exchange, "privateWebsocket")?;
    method(h, &private, "dispose", &[])?;
    Ok(())
}
pub fn public_dispose<H: LifecycleHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    method(h, socket, "stopPingLoop", &[])?;
    super_dispose(h, socket)
}
pub fn private_dispose<H: LifecycleHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    method(h, socket, "stopPingLoop", &[])?;
    let ws = h.get(socket, "ws")?;
    if !h.is_nullish(&ws) {
        let close = h.get(&ws, "close")?;
        if !h.is_nullish(&close) {
            h.call(&close, &ws, &[])?;
        }
    }
    Ok(())
}
pub fn end_retry_chain<H: LifecycleHost>(h: &mut H, exchange: &H::Value) -> Result<(), H::Error> {
    let no = h.boolean(false);
    h.set(exchange, "_retryChainActive", &no)?;
    let ids = h.get(exchange, "_pendingRetryOrderIds")?;
    method(h, &ids, "clear", &[])?;
    Ok(())
}
pub fn schedule_order_refresh<H: LifecycleHost>(
    h: &mut H,
    exchange: &H::Value,
) -> Result<(), H::Error> {
    let pending = h.get(exchange, "_orderRefreshPending")?;
    if h.truthy(&pending) {
        return Ok(());
    }
    let yes = h.boolean(true);
    h.set(exchange, "_orderRefreshPending", &yes)?;
    let first = h.get(exchange, "_orderRefreshDebounceTimer")?;
    if h.truthy(&first) {
        let clear = h.global("clearTimeout")?;
        let timer = h.get(exchange, "_orderRefreshDebounceTimer")?;
        let undefined = h.undefined();
        h.call(&clear, &undefined, &[timer])?;
    }
    let set = h.global("setTimeout")?;
    let callback = h.lifecycle_callback(LifecycleCallback {
        exchange: exchange.clone(),
        kind: LifecycleCallbackKind::OrderRefreshTimeout,
    })?;
    let delay = h.number(500.0);
    let undefined = h.undefined();
    let id = h.call(&set, &undefined, &[callback, delay])?;
    h.set(exchange, "_orderRefreshDebounceTimer", &id)
}
pub fn run_order_refresh_timeout<H: LifecycleHost>(
    h: &mut H,
    exchange: &H::Value,
) -> Result<(), H::Error> {
    let no = h.boolean(false);
    h.set(exchange, "_orderRefreshPending", &no)?;
    let null = h.null();
    h.set(exchange, "_orderRefreshDebounceTimer", &null)?;
    let date = h.global("Date")?;
    let now = method(h, &date, "now", &[])?;
    let last = h.get(exchange, "_lastOrderRefreshTime")?;
    let elapsed = h.subtract(&now, &last)?;
    let interval = h.get(exchange, "HIP3_ORDER_REFRESH_INTERVAL_MS")?;
    if h.greater_than(&elapsed, &interval)? {
        h.set(exchange, "_lastOrderRefreshTime", &now)?;
        let promise = method(h, exchange, "refreshOrders", &[])?;
        let catch = h.get(&promise, "catch")?;
        let callback = h.lifecycle_callback(LifecycleCallback {
            exchange: exchange.clone(),
            kind: LifecycleCallbackKind::OrderRefreshError,
        })?;
        h.call(&catch, &promise, &[callback])?;
    }
    Ok(())
}
pub fn run_order_refresh_error<H: LifecycleHost>(
    h: &mut H,
    exchange: &H::Value,
    error: &H::Value,
) -> Result<(), H::Error> {
    // Resolve the logger before template substitution and severity lookup.
    let log = h.get(exchange, "log")?;
    let mut text = "Debounced order refresh failed: "
        .encode_utf16()
        .collect::<Vec<_>>();
    text.extend(h.template_string(error)?.0);
    let message = h.literal(&JsString(text));
    let severity = h.global("LogSeverity")?;
    let warning = h.get(&severity, "Warning")?;
    h.call(&log, exchange, &[message, warning])?;
    Ok(())
}
