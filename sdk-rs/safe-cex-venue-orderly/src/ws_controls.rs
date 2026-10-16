//! Orderly websocket control bodies over original runtime values and callables.
use std::rc::Rc;
pub type Value<H> = Rc<<H as WsHost>::Value>;
pub type Result<H, T> = std::result::Result<T, <H as WsHost>::Error>;
pub type Entries<H> = Vec<(Vec<u16>, Value<H>)>;
pub trait WsHost: Sized {
    type Value: ?Sized;
    type Error;
    type Iterator;
    fn get(&self, value: &Value<Self>, key: &[u16]) -> Result<Self, Value<Self>>;
    fn set(&self, value: &Value<Self>, key: &[u16], next: Value<Self>) -> Result<Self, ()>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn string(&self, value: &Self::Value) -> Result<Self, Vec<u16>>;
    fn text(&self, value: &[u16]) -> Value<Self>;
    fn object(&self, entries: Vec<(&'static str, Value<Self>)>) -> Result<Self, Value<Self>>;
    fn stringify(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn parse(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn includes(&self, value: &Value<Self>, needle: &[u16]) -> Result<Self, bool>;
    fn own_values(&self, value: Value<Self>) -> Result<Self, Vec<Value<Self>>>;
    fn flatten_one(&self, values: Vec<Value<Self>>) -> Result<Self, Vec<Value<Self>>>;
    fn entries(&self, value: Value<Self>) -> Result<Self, Entries<Self>>;
    fn iterator(&self, value: Value<Self>) -> Result<Self, Self::Iterator>;
    fn next(&self, iterator: &mut Self::Iterator) -> Result<Self, Option<Value<Self>>>;
    fn call(
        &self,
        function: Value<Self>,
        receiver: Option<Value<Self>>,
        args: Vec<Value<Self>>,
    ) -> Result<Self, Value<Self>>;
    fn monotonic_now(&self) -> Result<Self, Value<Self>>;
    fn virtual_now(&self) -> Result<Self, Value<Self>>;
    fn sign(&self, message: Value<Self>, secret: Value<Self>) -> Result<Self, Value<Self>>;
    fn reconnecting(&self) -> bool;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn get<H: WsHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    h.get(v, &units(key))
}
fn text<H: WsHost>(h: &H, s: &str) -> Value<H> {
    h.text(&units(s))
}
fn disposed<H: WsHost>(h: &H, r: &Value<H>) -> Result<H, bool> {
    Ok(h.truthy(&*get(h, r, "isDisposed")?))
}
fn invoke<H: WsHost>(h: &H, r: &Value<H>, method: &str) -> Result<H, ()> {
    let function = get(h, r, method)?;
    h.call(function, Some(r.clone()), vec![])?;
    Ok(())
}
fn frame<H: WsHost>(
    h: &H,
    r: &Value<H>,
    method: &str,
    fields: Vec<(&'static str, Value<H>)>,
) -> Result<H, ()> {
    let function = get(h, r, method)?;
    let payload = h.object(fields)?;
    let encoded = h.stringify(payload)?;
    h.call(function, Some(r.clone()), vec![encoded])?;
    Ok(())
}
fn connect<H: WsHost>(h: &H, r: Value<H>, private: bool) -> Result<H, ()> {
    if disposed(h, &r)? {
        return Ok(());
    }
    let options = get(h, &get(h, &r, "parent")?, "options")?;
    let testnet = h.truthy(&*get(h, &options, "testnet")?);
    let table = &crate::types::BASE_WS_URL[usize::from(private)].1;
    let base = table[usize::from(testnet)].1;
    let function = get(h, &r, "createWebsocket")?;
    let options = get(h, &get(h, &r, "parent")?, "options")?;
    let url = [
        units(base),
        units("/"),
        h.string(&*get(h, &options, "applicationId")?)?,
    ]
    .concat();
    let socket = h.call(function, Some(r.clone()), vec![h.text(&url)])?;
    h.set(&r, &units("ws"), socket)?;
    for (event, method) in [
        ("open", "onOpen"),
        ("message", "onMessage"),
        ("close", "onClose"),
    ] {
        let socket = get(h, &r, "ws")?;
        let listener = get(h, &socket, "addEventListener")?;
        let callback = get(h, &r, method)?;
        h.call(listener, Some(socket), vec![text(h, event), callback])?;
    }
    Ok(())
}
pub fn public_connect<H: WsHost>(h: &H, r: Value<H>) -> Result<H, ()> {
    connect(h, r, false)
}
pub fn private_connect<H: WsHost>(h: &H, r: Value<H>) -> Result<H, ()> {
    connect(h, r, true)
}
pub fn public_open<H: WsHost>(h: &H, r: Value<H>) -> Result<H, ()> {
    if disposed(h, &r)? {
        return Ok(());
    }
    invoke(h, &r, "ping")?;
    for topic in ["tickers", "bbos", "markprices", "estfundingrate"] {
        frame(
            h,
            &r,
            "send",
            vec![("event", text(h, "subscribe")), ("topic", text(h, topic))],
        )?;
    }
    invoke(h, &r, "subscribe")?;
    if h.reconnecting() {
        invoke(h, &r, "emitWsReconnected")?;
    }
    Ok(())
}
pub fn private_open<H: WsHost>(h: &H, r: Value<H>) -> Result<H, ()> {
    if disposed(h, &r)? {
        return Ok(());
    }
    let timestamp = h.virtual_now()?;
    let send = get(h, &r, "sendWithLog")?;
    let options = get(h, &get(h, &r, "parent")?, "options")?;
    let key = get(h, &options, "key")?;
    let message = h.text(&h.string(&*timestamp)?);
    let options = get(h, &get(h, &r, "parent")?, "options")?;
    let signature = h.sign(message, get(h, &options, "secret")?)?;
    let params = h.object(vec![
        ("orderly_key", key),
        ("sign", signature),
        ("timestamp", timestamp),
    ])?;
    let payload = h.object(vec![("event", text(h, "auth")), ("params", params)])?;
    let encoded = h.stringify(payload)?;
    h.call(send, Some(r.clone()), vec![encoded])?;
    if h.reconnecting() {
        invoke(h, &get(h, &r, "parent")?, "fetchAndSetPrivateSnapshot")?;
    }
    Ok(())
}
pub fn public_subscribe<H: WsHost>(h: &H, r: Value<H>) -> Result<H, ()> {
    let values = h.own_values(get(h, &r, "topics")?)?;
    let topics = h.flatten_one(values)?;
    for topic in topics {
        frame(
            h,
            &r,
            "send",
            vec![("event", text(h, "subscribe")), ("topic", topic)],
        )?;
    }
    Ok(())
}
pub fn private_subscribe<H: WsHost>(h: &H, r: Value<H>) -> Result<H, ()> {
    if disposed(h, &r)? {
        return Ok(());
    }
    for topic in ["executionreport", "algoexecutionreportv2", "position"] {
        frame(
            h,
            &r,
            "sendWithLog",
            vec![("topic", text(h, topic)), ("event", text(h, "subscribe"))],
        )?;
    }
    invoke(h, &r, "ping")
}
fn ping<H: WsHost>(h: &H, r: Value<H>, method: &str) -> Result<H, ()> {
    if disposed(h, &r)? {
        return Ok(());
    }
    let now = h.monotonic_now()?;
    h.set(&r, &units("pingAt"), now)?;
    frame(h, &r, method, vec![("event", text(h, "ping"))])
}
pub fn public_ping<H: WsHost>(h: &H, r: Value<H>) -> Result<H, ()> {
    ping(h, r, "send")
}
pub fn private_ping<H: WsHost>(h: &H, r: Value<H>) -> Result<H, ()> {
    ping(h, r, "sendWithLog")
}
pub fn public_handle_ping<H: WsHost>(h: &H, r: Value<H>) -> Result<H, ()> {
    frame(h, &r, "send", vec![("event", text(h, "pong"))])
}
pub fn public_message<H: WsHost>(h: &H, r: Value<H>, event: Value<H>) -> Result<H, ()> {
    let data = get(h, &event, "data")?;
    if disposed(h, &r)? {
        return Ok(());
    }
    invoke(h, &r, "trackLastReceived")?;
    let handlers = h.entries(get(h, &r, "messageHandlers")?)?;
    for (topic, handler) in handlers {
        let event_prefix = [units("event\":\""), topic.clone()].concat();
        let topic_prefix = [units("topic\":\""), topic].concat();
        if h.includes(&data, &event_prefix)? || h.includes(&data, &topic_prefix)? {
            let json = h.parse(data.clone())?;
            let mut callbacks = h.iterator(handler)?;
            while let Some(callback) = h.next(&mut callbacks)? {
                if h.truthy(&*json) {
                    h.call(callback, None, vec![json.clone()])?;
                }
            }
            break;
        }
    }
    Ok(())
}
