//! Coinbase public websocket lifecycle and event routing. Listener construction remains separate.
//! Host methods are the existing BaseWebSocket/store/JSON.parse capabilities; no transport is opened here.
use crate::{
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
    utils::normalize_coinbase_usd_equivalent_product_id,
};
use indexmap::{IndexMap, IndexSet};
use safe_cex_types::Candle;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, Clone, PartialEq)]
pub struct PublicStreamFailure {
    pub native: Option<safe_cex_exchange::runtime::ExchangeError>,
    pub name: String,
    pub message: String,
}
type Failure = Rc<PublicStreamFailure>;
fn type_error(message: &str) -> Failure {
    Rc::new(PublicStreamFailure {
        name: "TypeError".into(),
        message: message.into(),
        native: None,
    })
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicSocketListener {
    Open,
    Message,
    Close,
}
pub type PublicSocket = Rc<RefCell<Frame>>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicSubscription {
    pub r#type: &'static str,
    pub product_ids: Vec<String>,
    pub channel: String,
}
#[derive(Debug, Clone)]
pub enum PublicStreamMessage {
    Frame(Frame),
    Candle(Rc<RefCell<Candle>>),
}
pub type PublicStreamCallback = Rc<dyn Fn(PublicStreamMessage) -> Result<(), Failure>>;
#[derive(Clone)]
pub enum PublicStreamHandler {
    Ohlcv,
    Trade,
    OrderBook,
    Pong,
    Ignore,
    Callback(PublicStreamCallback),
}
pub type PublicStreamHandlers = Rc<RefCell<Vec<PublicStreamHandler>>>;

pub trait PublicStreamHost {
    fn is_disposed(&self) -> bool;
    fn testnet(&self) -> bool;
    fn reconnecting_enabled(&self) -> bool;
    fn create_websocket(&self, url: &'static str) -> Result<PublicSocket, Failure>;
    fn add_listener(
        &self,
        socket: PublicSocket,
        listener: PublicSocketListener,
    ) -> Result<(), Failure>;
    fn reset_orderbook(&self) -> Result<(), Failure>;
    fn emit_ws_reconnected(&self) -> Result<(), Failure>;
    fn markets_symbols(&self) -> Result<Vec<String>, Failure>;
    /// Source send(JSON.stringify(payload)); the native payload is retained until this boundary.
    fn send_subscription(&self, payload: PublicSubscription) -> Result<(), Failure>;
    fn track_last_received(&self) -> Result<(), Failure>;
    /// Imported jsonParse helper: invalid JSON returns None, decoded JSON retains object identity.
    fn json_parse(&self, raw: &str) -> Option<Frame>;
    fn emit_error(&self, message: Option<Frame>) -> Result<(), Failure>;
    fn console_error_text(&self, message: Vec<u16>) -> Result<(), Failure>;
    fn console_error_frame(&self, frame: Frame) -> Result<(), Failure>;
    fn handle_pong_event(&self) -> Result<(), Failure>;
    fn record_message_handler_stats(&self, topic: &str) -> Result<(), Failure>;
}

pub struct PublicStreamReceiver<H> {
    pub host: H,
    pub ws: RefCell<Option<PublicSocket>>,
    pub topics: RefCell<IndexMap<String, f64>>,
    pub message_handlers: RefCell<IndexMap<String, PublicStreamHandlers>>,
}
impl<H> PublicStreamReceiver<H> {
    /// Field-state adapter for independently mapped bodies, not the full exchange/socket constructor.
    pub fn new(host: H) -> Self {
        let entries = [
            ("candles", PublicStreamHandler::Ohlcv),
            ("market_trades", PublicStreamHandler::Trade),
            ("l2_data", PublicStreamHandler::OrderBook),
            ("heartbeats", PublicStreamHandler::Pong),
            ("subscriptions", PublicStreamHandler::Ignore),
        ];
        Self {
            host,
            ws: RefCell::new(None),
            topics: RefCell::new(IndexMap::from([("heartbeats".into(), 1.0)])),
            message_handlers: RefCell::new(
                entries
                    .into_iter()
                    .map(|(k, v)| (k.into(), Rc::new(RefCell::new(vec![v]))))
                    .collect(),
            ),
        }
    }
}
fn own_keys<V>(values: &IndexMap<String, V>) -> Vec<String> {
    let mut indices = Vec::new();
    let mut others = Vec::new();
    for key in values.keys() {
        match key
            .parse::<u32>()
            .ok()
            .filter(|v| *v < u32::MAX && v.to_string() == *key)
        {
            Some(i) => indices.push((i, key.clone())),
            None => others.push(key.clone()),
        }
    }
    indices.sort_by_key(|v| v.0);
    indices.into_iter().map(|v| v.1).chain(others).collect()
}
fn inherited_object_key(key: &[u16]) -> bool {
    [
        "constructor",
        "__defineGetter__",
        "__defineSetter__",
        "hasOwnProperty",
        "__lookupGetter__",
        "__lookupSetter__",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toString",
        "valueOf",
        "__proto__",
        "toLocaleString",
    ]
    .iter()
    .any(|name| name.encode_utf16().eq(key.iter().copied()))
}
fn truthy(value: Option<&Frame>) -> bool {
    match value {
        None | Some(Frame::Null) => false,
        Some(Frame::Bool(v)) => *v,
        Some(Frame::Number(v)) => *v != 0.0 && !v.is_nan(),
        Some(Frame::String(v)) => !v.is_empty(),
        _ => true,
    }
}
fn equals(value: Option<Frame>, expected: &str) -> bool {
    matches!(value,Some(Frame::String(v)) if v.iter().copied().eq(expected.encode_utf16()))
}
fn key(value: Option<&Frame>) -> Result<Vec<u16>, Failure> {
    Ok(match value {
        None | Some(Frame::Undefined) => "undefined".encode_utf16().collect(),
        Some(Frame::Null) => "null".encode_utf16().collect(),
        Some(Frame::Bool(v)) => v.to_string().encode_utf16().collect(),
        Some(Frame::Number(v)) => safe_cex_num::js_number_string(*v).encode_utf16().collect(),
        Some(Frame::String(v)) => v.clone(),
        Some(Frame::Array(v)) => {
            let values = v.borrow().clone();
            let mut result = Vec::new();
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    result.push(44);
                }
                if !matches!(v, Frame::Null | Frame::Undefined) {
                    result.extend(key(Some(v))?);
                }
            }
            result
        }
        Some(Frame::Object(v)) => {
            if v.borrow()
                .contains_key(&"toString".encode_utf16().collect::<Vec<_>>())
            {
                return Err(type_error("Cannot convert object to primitive value"));
            }
            "[object Object]".encode_utf16().collect()
        }
    })
}
fn text(value: Option<Frame>) -> Result<String, Failure> {
    match value {
        Some(Frame::String(v)) => {
            String::from_utf16(&v).map_err(|_| type_error("Expected a UTF-8 product identifier"))
        }
        _ => Err(type_error("Expected a string product identifier")),
    }
}
fn number_text(value: Option<Frame>) -> Result<String, Failure> {
    Ok(String::from_utf16_lossy(&key(value.as_ref())?))
}
fn array(value: Option<Frame>, message: &str) -> Result<Rc<RefCell<Vec<Frame>>>, Failure> {
    match value {
        Some(Frame::Array(v)) => Ok(v),
        _ => Err(type_error(message)),
    }
}
fn handler<H>(r: &PublicStreamReceiver<H>, name: &str) -> Option<PublicStreamHandlers> {
    r.message_handlers.borrow().get(name).cloned()
}
fn next_item(items: &Rc<RefCell<Vec<Frame>>>, index: &mut usize) -> Option<Frame> {
    let value = items.borrow().get(*index).cloned();
    *index += 1;
    value
}
fn invoke<H: PublicStreamHost>(
    r: &PublicStreamReceiver<H>,
    cb: PublicStreamHandler,
    data: PublicStreamMessage,
) -> Result<(), Failure> {
    match cb {
        PublicStreamHandler::Callback(f) => f(data),
        PublicStreamHandler::Pong => r.host.handle_pong_event(),
        PublicStreamHandler::Ignore => Ok(()),
        other => {
            let PublicStreamMessage::Frame(data) = data else {
                return Err(type_error("Expected websocket event frame"));
            };
            match other {
                PublicStreamHandler::Ohlcv => handle_ohlcv_stream_events(r, &data),
                PublicStreamHandler::Trade => handle_trade_stream_events(r, &data),
                PublicStreamHandler::OrderBook => handle_order_book_stream_events(r, &data),
                _ => unreachable!(),
            }
        }
    }
}
fn dispatch_for_each<H: PublicStreamHost>(
    r: &PublicStreamReceiver<H>,
    callbacks: PublicStreamHandlers,
    value: Frame,
) -> Result<(), Failure> {
    // Array.forEach captures length, but reads each remaining callback immediately before invocation.
    let length = callbacks.borrow().len();
    for index in 0..length {
        let cb = callbacks.borrow().get(index).cloned();
        if let Some(cb) = cb {
            invoke(r, cb, PublicStreamMessage::Frame(value.clone()))?;
        }
    }
    Ok(())
}

// ts: CoinbasePublicWebsocket.connectAndSubscribe
pub fn connect_and_subscribe<H: PublicStreamHost>(
    r: &PublicStreamReceiver<H>,
) -> Result<(), Failure> {
    if !r.host.is_disposed() {
        let _testnet = r.host.testnet();
        let socket = r
            .host
            .create_websocket("wss://advanced-trade-ws.coinbase.com")?;
        *r.ws.borrow_mut() = Some(socket);
        for listener in [
            PublicSocketListener::Open,
            PublicSocketListener::Message,
            PublicSocketListener::Close,
        ] {
            let socket = r.ws.borrow().clone().ok_or_else(|| {
                type_error("Cannot read properties of undefined (reading 'addEventListener')")
            })?;
            r.host.add_listener(socket, listener)?;
        }
    }
    Ok(())
}
// ts: CoinbasePublicWebsocket.onOpen
pub fn on_open<H: PublicStreamHost>(r: &PublicStreamReceiver<H>) -> Result<(), Failure> {
    on_open_with_reset(r, || r.host.reset_orderbook())
}
/// Bind the imported BaseWebSocket reset operation to the owning class's state.
pub fn on_open_with_reset<H: PublicStreamHost>(
    r: &PublicStreamReceiver<H>,
    reset: impl FnOnce() -> Result<(), Failure>,
) -> Result<(), Failure> {
    if !r.host.is_disposed() {
        reset()?;
        subscribe(r)?;
        if r.host.reconnecting_enabled() {
            r.host.emit_ws_reconnected()?;
        }
    }
    Ok(())
}
// ts: CoinbasePublicWebsocket.subscribe
pub fn subscribe<H: PublicStreamHost>(r: &PublicStreamReceiver<H>) -> Result<(), Failure> {
    let topics = own_keys(&r.topics.borrow());
    for topic in topics {
        let mut parts = topic.split('.');
        let channel = parts.next().unwrap_or_default().to_owned();
        let symbol = parts.next().unwrap_or_default();
        let product_ids = if !symbol.is_empty() {
            vec![symbol.into()]
        } else {
            r.host.markets_symbols()?
        };
        r.host.send_subscription(PublicSubscription {
            r#type: "subscribe",
            product_ids,
            channel,
        })?;
    }
    Ok(())
}
// ts: CoinbasePublicWebsocket.onMessage
pub fn on_message<H: PublicStreamHost>(
    r: &PublicStreamReceiver<H>,
    raw: &str,
) -> Result<(), Failure> {
    r.host.track_last_received()?;
    if !r.host.is_disposed() {
        let Some(json) = r.host.json_parse(raw) else {
            return Ok(());
        };
        if !truthy(Some(&json)) {
            return Ok(());
        }
        if equals(json.get("type"), "error") {
            r.host.emit_error(json.get("message"))?;
            return Ok(());
        }
        let channel = key(json.get("channel").as_ref())?;
        let callbacks = r
            .message_handlers
            .borrow()
            .iter()
            .find(|(name, _)| name.encode_utf16().eq(channel.iter().copied()))
            .map(|(_, v)| v.clone());
        let Some(callbacks) = callbacks else {
            if inherited_object_key(&channel) {
                return Err(type_error("this.messageHandlers[channel] is not iterable"));
            }
            let mut message: Vec<u16> = "No handler for channel ".encode_utf16().collect();
            message.extend(channel);
            r.host.console_error_text(message)?;
            r.host.console_error_frame(json)?;
            return Ok(());
        };
        // for-of is live; appending a callback during delivery is observable.
        let mut i = 0;
        loop {
            let cb = callbacks.borrow().get(i).cloned();
            let Some(cb) = cb else { break };
            i += 1;
            invoke(r, cb, PublicStreamMessage::Frame(json.clone()))?;
        }
    }
    Ok(())
}
// ts: CoinbasePublicWebsocket.handleOhlcvStreamEvents
pub fn handle_ohlcv_stream_events<H: PublicStreamHost>(
    r: &PublicStreamReceiver<H>,
    data: &Frame,
) -> Result<(), Failure> {
    let events = array(data.get("events"), "events is not iterable")?;
    let mut symbols = IndexSet::new();
    let mut index = 0;
    while let Some(event) = next_item(&events, &mut index) {
        if equals(event.get("type"), "update") {
            let mut groups: IndexMap<String, Vec<Rc<RefCell<Candle>>>> = IndexMap::new();
            let candles = array(event.get("candles"), "event.candles is not iterable")?;
            let mut i = 0;
            while let Some(candle) = next_item(&candles, &mut i) {
                let symbol = text(candle.get("product_id"))?;
                if inherited_object_key(&symbol.encode_utf16().collect::<Vec<_>>()) {
                    return Err(type_error("updatesBySymbol[symbol].push is not a function"));
                }
                let formatted = Candle {
                    timestamp: safe_cex_num::parse_int(&number_text(candle.get("start"))?, None),
                    open: safe_cex_num::parse_float(&number_text(candle.get("open"))?),
                    high: safe_cex_num::parse_float(&number_text(candle.get("high"))?),
                    low: safe_cex_num::parse_float(&number_text(candle.get("low"))?),
                    close: safe_cex_num::parse_float(&number_text(candle.get("close"))?),
                    volume: safe_cex_num::parse_float(&number_text(candle.get("volume"))?),
                    last_side: None,
                };
                groups
                    .entry(symbol)
                    .or_default()
                    .push(Rc::new(RefCell::new(formatted)));
            }
            for raw_symbol in own_keys(&groups) {
                let symbol = normalize_coinbase_usd_equivalent_product_id(&raw_symbol);
                let topic = format!("candles.{symbol}");
                let Some(callbacks) = handler(r, &topic) else {
                    continue;
                };
                if callbacks.borrow().is_empty() {
                    continue;
                }
                symbols.insert(symbol);
                let mut i = 0;
                loop {
                    let cb = callbacks.borrow().get(i).cloned();
                    let Some(cb) = cb else { break };
                    i += 1;
                    for candle in &groups[&raw_symbol] {
                        invoke(r, cb.clone(), PublicStreamMessage::Candle(candle.clone()))?;
                    }
                }
            }
        }
    }
    for symbol in symbols {
        r.host
            .record_message_handler_stats(&format!("candles.{symbol}"))?;
    }
    Ok(())
}
// ts: CoinbasePublicWebsocket.handleTradeStreamEvents
pub fn handle_trade_stream_events<H: PublicStreamHost>(
    r: &PublicStreamReceiver<H>,
    data: &Frame,
) -> Result<(), Failure> {
    let events = array(data.get("events"), "events is not iterable")?;
    let mut symbols = IndexSet::new();
    let mut index = 0;
    while let Some(event) = next_item(&events, &mut index) {
        if equals(event.get("type"), "update") {
            let trades = array(
                event.get("trades"),
                "Cannot read properties of undefined (reading '0')",
            )?;
            let trade = trades.borrow().first().cloned();
            if let Some(trade) = trade.filter(|v| truthy(Some(v))) {
                let symbol = text(trade.get("product_id"))?;
                symbols.insert(symbol.clone());
                if let Some(callbacks) = handler(r, &format!("market_trades.{symbol}")) {
                    dispatch_for_each(r, callbacks, trade)?;
                }
            }
        }
    }
    for symbol in symbols {
        r.host
            .record_message_handler_stats(&format!("market_trades.{symbol}"))?;
    }
    Ok(())
}
// ts: CoinbasePublicWebsocket.handleOrderBookStreamEvents
pub fn handle_order_book_stream_events<H: PublicStreamHost>(
    r: &PublicStreamReceiver<H>,
    data: &Frame,
) -> Result<(), Failure> {
    let events = array(data.get("events"), "events is not iterable")?;
    let mut symbols = IndexSet::new();
    let mut index = 0;
    while let Some(event) = next_item(&events, &mut index) {
        let symbol = text(event.get("product_id"))?;
        symbols.insert(symbol.clone());
        if let Some(callbacks) = handler(r, &format!("level2.{symbol}")) {
            dispatch_for_each(r, callbacks, event)?;
        }
    }
    for symbol in symbols {
        r.host
            .record_message_handler_stats(&format!("level2.{symbol}"))?;
    }
    Ok(())
}
// ts: CoinbasePublicWebsocket.ping
pub fn ping<H>(_: &PublicStreamReceiver<H>) {}
