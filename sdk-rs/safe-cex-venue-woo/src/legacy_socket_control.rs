//! Legacy socket lifecycle and control-message method bodies.
use crate::legacy_metadata::{JsString, Task};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Public,
    Private,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listener {
    Open,
    Message,
    Close,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callee {
    CreateSocket,
    Send(bool),
    EmitError,
    Execution(bool),
}
/// Primitives preserve JS values, property reads, iterator identity and thrown errors.
/// Construction primitives are pure; Call tokens retain resolved callee/receiver identity.
/// Recursive socket operations run through the supplied host bindings.
pub trait Host: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    type Socket;
    type Call;
    fn prepare(&mut self, callee: Callee) -> Result<Self::Call, Self::Error>;
    type Signer;
    type Iterator;
    type Callback;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn option(&mut self, key: &str) -> Result<Self::Value, Self::Error>;
    fn truthy(&mut self, v: &Self::Value) -> bool;
    fn nullish(&mut self, v: &Self::Value) -> bool;
    fn is_string(&mut self, v: &Self::Value) -> bool;
    fn equals(&mut self, v: &Self::Value, s: &str) -> bool;
    fn text(&mut self, v: &Self::Value) -> Result<JsString, Self::Error>;
    fn field(&mut self, v: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn string(&self, v: JsString) -> Self::Value;
    fn number(&self, v: f64) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn stringify(&mut self, v: &Self::Value) -> Result<JsString, Self::Error>;
    fn parse(&mut self, data: Self::Value) -> Result<Self::Value, Self::Error>;
    fn create_socket(
        &mut self,
        call: Self::Call,
        url: JsString,
    ) -> Result<Self::Socket, Self::Error>;
    fn set_socket(&mut self, socket: Self::Socket);
    fn socket(&mut self) -> Result<Self::Socket, Self::Error>;
    fn add_listener(
        &mut self,
        socket: Self::Socket,
        event: &str,
        listener: Listener,
    ) -> Result<(), Self::Error>;
    fn send(&mut self, call: Self::Call, text: JsString, logged: bool) -> Result<(), Self::Error>;
    fn ping(&mut self, channel: Channel) -> Result<(), Self::Error>;
    fn pong(&mut self) -> Result<(), Self::Error>;
    fn reset_orderbook(&mut self) -> Result<(), Self::Error>;
    fn subscribe(&mut self, channel: Channel) -> Result<(), Self::Error>;
    fn reconnecting_enabled(&mut self) -> bool;
    fn emit_reconnected(&mut self) -> Result<(), Self::Error>;
    fn topics(&mut self) -> Result<Self::Value, Self::Error>;
    fn object_values(&mut self, v: Self::Value) -> Result<Vec<Self::Value>, Self::Error>;
    /// Lodash flattenable arrays/arguments/spreadable values include holes as undefined.
    fn flattenable_values(
        &mut self,
        v: &Self::Value,
    ) -> Result<Option<Vec<Self::Value>>, Self::Error>;
    fn track_last_received(&mut self) -> Result<(), Self::Error>;
    fn diagnostic_receive(&mut self, data: &Self::Value) -> Result<(), Self::Error>;
    fn handlers(&mut self, key: &[u16]) -> Result<Self::Value, Self::Error>;
    fn stats(&mut self, key: JsString) -> Result<(), Self::Error>;
    fn iterator(&mut self, handlers: Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(
        &mut self,
        iterator: &mut Self::Iterator,
    ) -> Result<Option<Self::Callback>, Self::Error>;
    fn callback(&mut self, callback: Self::Callback, json: &Self::Value)
    -> Result<(), Self::Error>;
    fn execution_report(
        &mut self,
        call: Self::Call,
        data: Self::Value,
        algo: bool,
    ) -> Result<(), Self::Error>;
    fn emit_error(&mut self, call: Self::Call, error: Self::Value) -> Result<(), Self::Error>;
    fn timestamp(&mut self) -> Result<f64, Self::Error>;
    fn nanoid(&mut self) -> Result<JsString, Self::Error>;
    fn prepare_signer(&mut self, signer: Self::Value) -> Result<Self::Signer, Self::Error>;
    fn sign(
        &mut self,
        signer: Self::Signer,
        request: Self::Value,
    ) -> Result<Task<Self::Value, Self::Error>, Self::Error>;
    fn hmac_sha256_hex(
        &mut self,
        secret: Self::Value,
        text: JsString,
    ) -> Result<JsString, Self::Error>;
    fn private_snapshot(&mut self) -> Result<(), Self::Error>;
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn string<H: Host>(h: &H, s: &str) -> H::Value {
    h.string(u(s))
}
fn send<H: Host>(h: &mut H, object: H::Value, logged: bool) -> Result<(), H::Error> {
    let call = h.prepare(Callee::Send(logged))?;
    let payload = h.stringify(&object)?;
    h.send(call, payload, logged)
}
fn topic_payload<H: Host>(h: &H, topic: H::Value) -> H::Value {
    h.object(vec![("event", string(h, "subscribe")), ("topic", topic)])
}
fn connect<H: Host>(h: &mut H, channel: Channel) -> Result<(), H::Error> {
    if h.disposed()? {
        return Ok(());
    }
    let testnet = h.option("testnet")?;
    let env = if h.truthy(&testnet) {
        "testnet"
    } else {
        "livenet"
    };
    let kind = if channel == Channel::Public {
        "public"
    } else {
        "private"
    };
    let base = crate::constants::BASE_WS_URL
        .iter()
        .find(|(k, _)| *k == kind)
        .unwrap()
        .1
        .iter()
        .find(|(k, _)| *k == env)
        .unwrap()
        .1;
    let call = h.prepare(Callee::CreateSocket)?;
    let app = h.option("applicationId")?;
    let mut url = u(base);
    url.extend(h.text(&app)?);
    let socket = h.create_socket(call, url)?;
    h.set_socket(socket);
    for (event, listener) in [
        ("open", Listener::Open),
        ("message", Listener::Message),
        ("close", Listener::Close),
    ] {
        let socket = h.socket()?;
        h.add_listener(socket, event, listener)?;
    }
    Ok(())
}
// ts: WooPublicWebsocket.connectAndSubscribe
pub fn public_connect_and_subscribe<H: Host>(h: &mut H) -> Result<(), H::Error> {
    connect(h, Channel::Public)
}
// ts: WooPrivateWebscoket.connectAndSubscribe
pub fn private_connect_and_subscribe<H: Host>(h: &mut H) -> Result<(), H::Error> {
    connect(h, Channel::Private)
}
// ts: WooPublicWebsocket.onOpen
pub fn public_on_open<H: Host>(h: &mut H) -> Result<(), H::Error> {
    if h.disposed()? {
        return Ok(());
    }
    h.ping(Channel::Public)?;
    for topic in ["tickers", "bbos", "markprices"] {
        let payload = topic_payload(h, string(h, topic));
        send(h, payload, false)?;
    }
    h.reset_orderbook()?;
    h.subscribe(Channel::Public)?;
    if h.reconnecting_enabled() {
        h.emit_reconnected()?;
    }
    Ok(())
}
// ts: WooPublicWebsocket.subscribe
pub fn public_subscribe<H: Host>(h: &mut H) -> Result<(), H::Error> {
    let topics = h.topics()?;
    let values = h.object_values(topics)?;
    let mut flattened = vec![];
    for value in values {
        if let Some(items) = h.flattenable_values(&value)? {
            flattened.extend(items);
        } else {
            flattened.push(value);
        }
    }
    for topic in flattened {
        let payload = topic_payload(h, topic);
        send(h, payload, false)?;
    }
    Ok(())
}
// ts: WooPrivateWebscoket.subscribe
pub fn private_subscribe<H: Host>(h: &mut H) -> Result<(), H::Error> {
    if h.disposed()? {
        return Ok(());
    }
    for topic in ["executionreport", "algoexecutionreportv2"] {
        let payload = h.object(vec![
            ("topic", string(h, topic)),
            ("event", string(h, "subscribe")),
        ]);
        send(h, payload, true)?;
    }
    h.ping(Channel::Private)
}
// ts: WooPublicWebsocket.onMessage
pub fn public_on_message<H: Host>(h: &mut H, data: H::Value) -> Result<(), H::Error> {
    if h.disposed()? {
        return Ok(());
    }
    h.track_last_received()?;
    let json = h.parse(data)?;
    if !h.truthy(&json) {
        return Ok(());
    }
    let mut key = vec![];
    let event = h.field(&json, "event")?;
    if h.is_string(&event) {
        let event = h.field(&json, "event")?;
        if !h.equals(&event, "ping") {
            let event = h.field(&json, "event")?;
            if !h.equals(&event, "pong") {
                return Ok(());
            }
        }
        let event = h.field(&json, "event")?;
        key = h.text(&event)?;
    } else {
        let topic = h.field(&json, "topic")?;
        if h.is_string(&topic) {
            let topic = h.field(&json, "topic")?;
            key = h.text(&topic)?;
        }
    }
    let handlers = h.handlers(&key)?;
    if h.nullish(&handlers) {
        return Ok(());
    }
    let length = h.field(&handlers, "length")?;
    if !h.truthy(&length) {
        return Ok(());
    }
    h.stats(key)?;
    let mut iterator = h.iterator(handlers)?;
    while let Some(callback) = h.next(&mut iterator)? {
        h.callback(callback, &json)?;
    }
    Ok(())
}
fn optional_field<H: Host>(
    h: &mut H,
    value: &H::Value,
    key: &str,
) -> Result<Option<H::Value>, H::Error> {
    if h.nullish(value) {
        Ok(None)
    } else {
        h.field(value, key).map(Some)
    }
}
fn event_equals<H: Host>(h: &mut H, json: &H::Value, event: &str) -> Result<bool, H::Error> {
    Ok(optional_field(h, json, "event")?.is_some_and(|v| h.equals(&v, event)))
}
// ts: WooPrivateWebscoket.onMessage
pub fn private_on_message<H: Host>(h: &mut H, data: H::Value) -> Result<(), H::Error> {
    if h.disposed()? {
        return Ok(());
    }
    h.track_last_received()?;
    h.diagnostic_receive(&data)?;
    let json = h.parse(data)?;
    if event_equals(h, &json, "ping")? {
        let payload = h.object(vec![("event", string(h, "pong"))]);
        return send(h, payload, true);
    }
    if event_equals(h, &json, "pong")? {
        return h.pong();
    }
    if event_equals(h, &json, "auth")? {
        let success = optional_field(h, &json, "success")?;
        if success.is_some_and(|v| h.truthy(&v)) {
            return h.subscribe(Channel::Private);
        }
        let call = h.prepare(Callee::EmitError)?;
        let message = optional_field(h, &json, "message")?;
        let error = match message.filter(|v| h.truthy(v)) {
            Some(v) => v,
            None => optional_field(h, &json, "error")?
                .filter(|v| h.truthy(v))
                .unwrap_or_else(|| string(h, "WOO X private websocket authentication failed")),
        };
        return h.emit_error(call, error);
    }
    let topic = optional_field(h, &json, "topic")?;
    if topic.is_some_and(|v| h.equals(&v, "executionreport")) {
        h.stats(u("order"))?;
        let call = h.prepare(Callee::Execution(false))?;
        let data = h.field(&json, "data")?;
        return h.execution_report(call, data, false);
    }
    let topic = optional_field(h, &json, "topic")?;
    if topic.is_some_and(|v| h.equals(&v, "algoexecutionreportv2")) {
        h.stats(u("orderAlgo"))?;
        let call = h.prepare(Callee::Execution(true))?;
        let data = h.field(&json, "data")?;
        h.execution_report(call, data, true)?;
    }
    Ok(())
}
fn auth_payload<H: Host>(
    h: &mut H,
    call: H::Call,
    key: H::Value,
    signature: H::Value,
    timestamp: f64,
) -> Result<(), H::Error> {
    let params = h.object(vec![
        ("apikey", key),
        ("sign", signature),
        ("timestamp", h.number(timestamp)),
    ]);
    let payload = h.object(vec![("event", string(h, "auth")), ("params", params)]);
    let text = h.stringify(&payload)?;
    h.send(call, text, true)?;
    if h.reconnecting_enabled() {
        h.private_snapshot()?;
    }
    Ok(())
}
// ts: WooPrivateWebscoket.onOpen
pub fn private_on_open<H: Host>(host: Rc<RefCell<H>>) -> Task<(), H::Error> {
    let prepared = (|| {
        let mut h = host.borrow_mut();
        if h.disposed()? {
            return Ok(None);
        }
        let timestamp = h.timestamp()?;
        let remote = h.option("remoteSigner")?;
        if h.truthy(&remote) {
            let remote = h.option("remoteSigner")?;
            let signer = h.prepare_signer(remote)?;
            let id = h.nanoid()?;
            let remote = h.option("remoteSigner")?;
            let account = h.field(&remote, "accountName")?;
            let mut to_sign = u("|");
            let number = h.number(timestamp);
            to_sign.extend(h.text(&number)?);
            let extra = h.object(vec![("timestamp", h.number(timestamp))]);
            let ws_auth = h.object(vec![
                ("toSign", h.string(to_sign)),
                ("outputFormat", string(&*h, "hex")),
                ("extraData", extra),
            ]);
            let request = h.object(vec![
                ("id", h.string(id)),
                ("type", string(&*h, "ws-auth")),
                ("exchangeName", string(&*h, "woo")),
                ("accountName", account),
                ("wsAuth", ws_auth),
            ]);
            let result = h.sign(signer, request)?;
            Ok(Some((timestamp, result)))
        } else {
            let secret = h.option("secret")?;
            let mut to_sign = u("|");
            let number = h.number(timestamp);
            to_sign.extend(h.text(&number)?);
            let signature = h.hmac_sha256_hex(secret, to_sign)?;
            let signature = h.string(signature);
            let call = h.prepare(Callee::Send(true))?;
            let key = h.option("key")?;
            auth_payload(&mut *h, call, key, signature, timestamp)?;
            Ok(None)
        }
    })();
    match prepared {
        Err(e) => Box::pin(std::future::ready(Err(e))),
        Ok(None) => Box::pin(std::future::ready(Ok(()))),
        Ok(Some((timestamp, result))) => Box::pin(async move {
            let result = result.await?;
            let mut h = host.borrow_mut();
            let error = h.field(&result, "error")?;
            if h.truthy(&error) {
                let call = h.prepare(Callee::EmitError)?;
                let error = h.field(&result, "error")?;
                let mut message = u("Remote WS signing unavailable: ");
                message.extend(h.text(&error)?);
                let value = h.string(message);
                return h.emit_error(call, value);
            }
            let call = h.prepare(Callee::Send(true))?;
            let key = h.option("key")?;
            let signature = h.field(&result, "wsAuthSignature")?;
            auth_payload(&mut *h, call, key, signature, timestamp)
        }),
    }
}
