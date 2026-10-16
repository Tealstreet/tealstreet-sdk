//! Connection/opening/dispatch leaves for the four Hyperliquid websocket classes.
//! Properties, globals, strict assignment and virtual calls use venue-local host
//! bindings. Browser construction and full websocket classes remain separate.
use crate::{spot_json::JsString, ws_ping_lifecycle::WsPingHost};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WsVenue {
    Perp,
    Spot,
}
pub trait WsTransportHost: WsPingHost {
    /// Fresh source-module WSS_URLS import binding, retaining the distinct
    /// perpetual and spot production tables.
    fn urls_binding(&mut self, venue: WsVenue) -> Result<Self::Value, Self::Error>;
    fn boolean(&self, value: bool) -> Self::Value;
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
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
fn connect<H: WsTransportHost>(
    h: &mut H,
    socket: &H::Value,
    venue: WsVenue,
) -> Result<(), H::Error> {
    let disposed = h.get(socket, "isDisposed")?;
    if !h.truthy(&disposed) {
        let create = h.get(socket, "createWebsocket")?;
        // Source WSS_URLS binding is the venue's perp/spot table. Evaluate the
        // table reference before the conditional element-access key.
        let urls = h.urls_binding(venue)?;
        let parent = h.get(socket, "parent")?;
        let options = h.get(&parent, "options")?;
        let testnet = h.get(&options, "testnet")?;
        let key = if h.truthy(&testnet) {
            "TESTNET"
        } else {
            "PRODUCTION"
        };
        let url = h.get(&urls, key)?;
        let ws = h.call(&create, socket, &[url])?;
        h.set(socket, "ws", &ws)?;
        for (event, handler) in [
            ("open", "onOpen"),
            ("message", "onMessage"),
            ("close", "onClose"),
        ] {
            // Three fresh this.ws reads, and each callee is resolved before
            // its corresponding native handler argument is evaluated.
            let ws = h.get(socket, "ws")?;
            let add = h.get(&ws, "addEventListener")?;
            let event = h.literal(&text(event));
            let handler = h.get(socket, handler)?;
            h.call(&add, &ws, &[event, handler])?;
        }
    }
    Ok(())
}
/// Source's optional `_isReconnect = false` argument is unused on all four
/// methods. This leaf has no reconnect-dependent branch or parameter coercion.
pub fn public_connect_and_subscribe<H: WsTransportHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    connect(h, socket, WsVenue::Perp)
}
pub fn private_connect_and_subscribe<H: WsTransportHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    connect(h, socket, WsVenue::Perp)
}
pub fn spot_public_connect_and_subscribe<H: WsTransportHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    connect(h, socket, WsVenue::Spot)
}
pub fn spot_private_connect_and_subscribe<H: WsTransportHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    connect(h, socket, WsVenue::Spot)
}

pub fn spot_public_on_open<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    let disposed = h.get(socket, "isDisposed")?;
    if !h.truthy(&disposed) {
        method(h, socket, "trackLastReceived", &[])?;
        method(h, socket, "resetOrderbook", &[])?;
        method(h, socket, "subscribePublicTopics", &[])?;
        method(h, socket, "ping", &[])?;
    }
    Ok(())
}
pub fn spot_private_on_open<H: WsTransportHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    let disposed = h.get(socket, "isDisposed")?;
    if !h.truthy(&disposed) {
        method(h, socket, "trackLastReceived", &[])?;
        method(h, socket, "subscribe", &[])?;
        method(h, socket, "ping", &[])?;
        let parent = h.get(socket, "parent")?;
        let refresh = h.get(&parent, "requestSpotBalanceRefresh")?;
        let immediate = h.boolean(true);
        h.call(&refresh, &parent, &[immediate])?;
    }
    Ok(())
}
fn spot_ping<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    let disposed = h.get(socket, "isDisposed")?;
    if !h.truthy(&disposed) {
        let performance = h.global("performance")?;
        let now = method(h, &performance, "now", &[])?;
        h.set(socket, "pingAt", &now)?;
        let send = h.get(socket, "send")?;
        let json = h.global("JSON")?;
        let stringify = h.get(&json, "stringify")?;
        let payload = h.object()?;
        let ping = h.literal(&text("ping"));
        h.define_data(&payload, "method", &ping)?;
        let data = h.call(&stringify, &json, &[payload])?;
        h.call(&send, socket, &[data])?;
    }
    Ok(())
}
pub fn spot_public_ping<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    spot_ping(h, socket)
}
pub fn spot_private_ping<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    spot_ping(h, socket)
}
pub fn private_subscribe<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    method(h, socket, "subscribeSystemTopics", &[])?;
    method(h, socket, "subscribeAllActiveAssetData", &[])?;
    Ok(())
}
pub fn spot_private_subscribe<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<(), H::Error> {
    method(h, socket, "subscribeSystemTopics", &[])?;
    method(h, socket, "subscribeCoins", &[])?;
    Ok(())
}
