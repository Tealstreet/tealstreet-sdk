//! Backpack public WebSocket control bodies over UTF-16 strings and live state views.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub type JsString = Vec<u16>;
pub type SubscriptionState = Rc<RefCell<Subscriptions>>;
pub type Streams = Rc<RefCell<Vec<Option<JsString>>>>;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Subscriptions {
    values: Vec<JsString>,
}
impl Subscriptions {
    pub fn insert(&mut self, value: JsString) {
        if !self.values.contains(&value) {
            self.values.push(value);
        }
    }
    pub fn contains(&self, value: &[u16]) -> bool {
        self.values.iter().any(|v| v == value)
    }
    pub fn remove(&mut self, value: &[u16]) {
        self.values.retain(|v| v != value);
    }
    pub fn values(&self) -> &[JsString] {
        &self.values
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub method: &'static str,
    pub params: Option<Vec<Option<JsString>>>,
}
impl Message {
    pub fn to_json(&self) -> JsString {
        let mut out: JsString = "{\"method\":".encode_utf16().collect();
        json_string(&self.method.encode_utf16().collect::<Vec<_>>(), &mut out);
        if let Some(params) = &self.params {
            out.extend(",\"params\":[".encode_utf16());
            for (index, param) in params.iter().enumerate() {
                if index != 0 {
                    out.push(u16::from(b','));
                }
                match param {
                    Some(value) => json_string(value, &mut out),
                    None => out.extend("null".encode_utf16()),
                }
            }
            out.push(u16::from(b']'));
        }
        out.push(u16::from(b'}'));
        out
    }
}
pub(crate) fn json_string(value: &[u16], out: &mut JsString) {
    out.push(u16::from(b'"'));
    let mut index = 0;
    while index < value.len() {
        let unit = value[index];
        let escape = match unit {
            0x22 => Some("\\\""),
            0x5c => Some("\\\\"),
            8 => Some("\\b"),
            9 => Some("\\t"),
            10 => Some("\\n"),
            12 => Some("\\f"),
            13 => Some("\\r"),
            _ => None,
        };
        if let Some(escape) = escape {
            out.extend(escape.encode_utf16());
        } else if (0xd800..=0xdbff).contains(&unit)
            && value
                .get(index + 1)
                .is_some_and(|next| (0xdc00..=0xdfff).contains(next))
        {
            out.extend_from_slice(&value[index..index + 2]);
            index += 1;
        } else if unit < 0x20 || (0xd800..=0xdfff).contains(&unit) {
            out.extend(format!("\\u{unit:04x}").encode_utf16());
        } else {
            out.push(unit);
        }
        index += 1;
    }
    out.push(u16::from(b'"'));
}
fn stream(prefix: &str, symbol: &[u16]) -> JsString {
    prefix
        .encode_utf16()
        .chain(symbol.iter().copied())
        .collect()
}
fn kline_stream(symbol: &[u16], interval: &[u16]) -> JsString {
    let mut value: JsString = "kline.".encode_utf16().collect();
    value.extend_from_slice(interval);
    value.push(u16::from(b'.'));
    value.extend_from_slice(symbol);
    value
}
fn subscribe<E>(
    state: &SubscriptionState,
    stream: JsString,
    send: impl FnOnce(Message) -> Result<(), E>,
) -> Result<(), E> {
    if !state.borrow().contains(&stream) {
        state.borrow_mut().insert(stream.clone());
        send(Message {
            method: "SUBSCRIBE",
            params: Some(vec![Some(stream)]),
        })?;
    }
    Ok(())
}

// ts: BackpackPublicWebSocket.subscribeToOrderBook
pub fn subscribe_to_order_book<E>(
    state: &SubscriptionState,
    symbol: &[u16],
    send: impl FnOnce(Message) -> Result<(), E>,
) -> Result<(), E> {
    subscribe(state, stream("depth.", symbol), send)
}
// ts: BackpackPublicWebSocket.subscribeToTicker
pub fn subscribe_to_ticker<E>(
    state: &SubscriptionState,
    symbol: &[u16],
    send: impl FnOnce(Message) -> Result<(), E>,
) -> Result<(), E> {
    subscribe(state, stream("ticker.", symbol), send)
}
// ts: BackpackPublicWebSocket.subscribeToTrades
pub fn subscribe_to_trades<E>(
    state: &SubscriptionState,
    symbol: &[u16],
    send: impl FnOnce(Message) -> Result<(), E>,
) -> Result<(), E> {
    subscribe(state, stream("trade.", symbol), send)
}
// ts: BackpackPublicWebSocket.subscribeToBookTicker
pub fn subscribe_to_book_ticker<E>(
    state: &SubscriptionState,
    symbol: &[u16],
    send: impl FnOnce(Message) -> Result<(), E>,
) -> Result<(), E> {
    subscribe(state, stream("bookTicker.", symbol), send)
}
// ts: BackpackPublicWebSocket.subscribeToKline
pub fn subscribe_to_kline<E>(
    state: &SubscriptionState,
    symbol: &[u16],
    interval: &[u16],
    send: impl FnOnce(Message) -> Result<(), E>,
) -> Result<(), E> {
    subscribe(state, kline_stream(symbol, interval), send)
}

// ts: BackpackPublicWebSocket#resubscribe
pub fn resubscribe<E>(
    state: &SubscriptionState,
    send: impl FnOnce(Message) -> Result<(), E>,
) -> Result<(), E> {
    let values = state.borrow().values().to_vec();
    if !values.is_empty() {
        send(Message {
            method: "SUBSCRIBE",
            params: Some(values.into_iter().map(Some).collect()),
        })?;
    }
    Ok(())
}

// ts: BackpackPublicWebSocket.unsubscribe
pub fn unsubscribe<E>(
    state: &SubscriptionState,
    streams: &Streams,
    send: impl FnOnce(Message) -> Result<(), E>,
) -> Result<(), E> {
    let params = streams.borrow().clone();
    if !params.is_empty() {
        send(Message {
            method: "UNSUBSCRIBE",
            params: Some(params),
        })?;
        let length = streams.borrow().len();
        for index in 0..length {
            let value = streams.borrow().get(index).cloned().flatten();
            if let Some(value) = value {
                state.borrow_mut().remove(&value);
            }
        }
    }
    Ok(())
}

// ts: BackpackPublicWebSocket.unsubscribeFromKline
pub fn unsubscribe_from_kline<E>(
    symbol: &[u16],
    interval: &[u16],
    unsubscribe: impl FnOnce(Streams) -> Result<(), E>,
) -> Result<(), E> {
    unsubscribe(Rc::new(RefCell::new(vec![Some(kline_stream(
        symbol, interval,
    ))])))
}

// ts: BackpackPublicWebSocket.ping
pub fn ping<E>(
    is_disposed: bool,
    is_connected: impl FnOnce() -> Result<bool, E>,
    ping_at: &Cell<f64>,
    now: impl FnOnce() -> Result<f64, E>,
    send: impl FnOnce(Message) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed && is_connected()? {
        ping_at.set(now()?);
        send(Message {
            method: "PING",
            params: None,
        })?;
    }
    Ok(())
}

// ts: BackpackPublicWebSocket.onOpen
pub fn on_open<E>(
    is_disposed: bool,
    track_last_received: impl FnOnce() -> Result<(), E>,
    reset_and_refetch_order_books: impl FnOnce() -> Result<(), E>,
    resubscribe: impl FnOnce() -> Result<(), E>,
    ping: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed {
        track_last_received()?;
        reset_and_refetch_order_books()?;
        resubscribe()?;
        ping()?;
    }
    Ok(())
}
