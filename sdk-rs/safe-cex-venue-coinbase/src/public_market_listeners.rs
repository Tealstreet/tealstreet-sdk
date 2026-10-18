//! Stateful public listener field bodies sharing the existing dispatcher roots.
use crate::{
    public_data::PublicData,
    public_market_reads::{PublicOHLCVOptions, PublicOHLCVOptionsRoot},
    public_stream_dispatch::{
        PublicStreamFailure, PublicStreamHandler, PublicStreamHost, PublicStreamMessage,
        PublicStreamReceiver, PublicSubscription,
    },
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
    utils::normalize_coinbase_usd_equivalent_product_id,
};
use indexmap::IndexMap;
use safe_cex_num::parse_float;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};
pub type ListenerFailure = Rc<PublicStreamFailure>;
pub type PublicCleanup = Rc<dyn Fn() -> Result<(), ListenerFailure>>;
pub type PublicCandleCallback = Rc<dyn Fn(PublicData) -> Result<(), ListenerFailure>>;
pub type PublicBookCallback = Rc<dyn Fn(PublicListenerBook) -> Result<(), ListenerFailure>>;
pub type PublicListenerBook = Rc<RefCell<PublicBook>>;
pub type PublicBookLevels = Rc<RefCell<Vec<Rc<RefCell<PublicBookLevel>>>>>;
#[derive(Debug)]
pub struct PublicBookLevel {
    pub price: f64,
    pub amount: f64,
    pub total: f64,
}
pub struct PublicBook {
    pub bids: PublicBookLevels,
    pub asks: PublicBookLevels,
}
pub struct PublicBookInnerState {
    pub updates: Vec<PublicData>,
    pub is_snapshot_loaded: bool,
}
pub trait PublicMarketListenerHost: PublicStreamHost {
    fn is_connected(&self) -> bool;
    fn public_data_only_option(&self) -> bool;
    fn timeframe_seconds(&self, interval: &str) -> Option<f64>;
    fn daily_candle(&self, symbol: &str) -> Option<PublicData>;
    fn set_daily_candle(&self, symbol: String, candle: PublicData) -> Result<(), ListenerFailure>;
    fn utc_end_of_day_ms(&self) -> Result<f64, ListenerFailure>;
    /// Invoke fetchOHLCV hot and discard its returned Promise. Only a synchronous
    /// invocation exception propagates; an eventual rejection is detached.
    fn fetch_ohlcv_detached(&self, opts: PublicOHLCVOptionsRoot) -> Result<(), ListenerFailure>;
    fn dayjs_unix(&self, value: Option<Frame>) -> Result<f64, ListenerFailure>;
    fn add(&self, left: Option<Frame>, right: f64) -> Result<f64, ListenerFailure>;
    fn warn(&self, message: &str, value: Option<Frame>) -> Result<(), ListenerFailure>;
    fn set_timeout(&self, callback: PublicCleanup, delay_ms: f64) -> Result<(), ListenerFailure>;
    fn cleanup_message_handler_stats(&self, topic: &str) -> Result<(), ListenerFailure>;
    fn nanoid(&self) -> Result<String, ListenerFailure>;
    fn update_order_book(
        &self,
        book: PublicListenerBook,
        side: &str,
        level: Rc<RefCell<PublicBookLevel>>,
    ) -> Result<(), ListenerFailure>;
    fn sort_order_book(&self, book: PublicListenerBook) -> Result<(), ListenerFailure>;
    fn calc_order_book_total(&self, book: PublicListenerBook) -> Result<(), ListenerFailure>;
}
/// The owning class reads BaseWebSocket.isConnected from its actual ws slot.
/// Independent field-body adapters retain the imported host getter.
#[derive(Clone, Copy)]
pub enum PublicListenerConnectivity {
    Host,
    ReceiverSocket,
}
pub struct PublicListenerReceiver<H> {
    pub stream: Rc<PublicStreamReceiver<H>>,
    pub connectivity: PublicListenerConnectivity,
    /// Shared BaseWebSocket inner state: imported resetOrderbook must reset these
    /// same cells, not an unrelated host copy.
    pub inner_state: RefCell<IndexMap<String, Rc<RefCell<PublicBookInnerState>>>>,
}
fn is_connected<H: PublicMarketListenerHost>(r: &PublicListenerReceiver<H>) -> bool {
    match r.connectivity {
        PublicListenerConnectivity::Host => r.stream.host.is_connected(),
        PublicListenerConnectivity::ReceiverSocket => r.stream.ws.borrow().as_ref().is_some_and(|socket| {
            matches!(socket.borrow().get("readyState"), Some(Frame::Number(value)) if value == 1.0)
        }),
    }
}
fn failure(message: &str) -> ListenerFailure {
    Rc::new(PublicStreamFailure {
        name: "TypeError".into(),
        message: message.into(),
        native: None,
    })
}
fn data(message: PublicStreamMessage, first: &str) -> Result<PublicData, ListenerFailure> {
    match message {
        PublicStreamMessage::Candle(candle) => Ok(PublicData::Candle(candle)),
        PublicStreamMessage::Frame(Frame::Object(object)) => Ok(PublicData::Json(object)),
        PublicStreamMessage::Frame(Frame::Null) => Err(failure(&format!(
            "Cannot read properties of null (reading '{first}')"
        ))),
        // Property reads box other primitive/array values. Listener logic only
        // reads the named protocol properties, which are absent on these values.
        PublicStreamMessage::Frame(_) => Ok(PublicData::Generated(Rc::new(RefCell::new(
            IndexMap::new(),
        )))),
    }
}
fn scalar(value: Option<Frame>) -> Result<f64, ListenerFailure> {
    crate::public_data::number(value).map_err(|e| failure(e.message))
}
fn float(value: Option<Frame>) -> Result<f64, ListenerFailure> {
    crate::public_data::string(value)
        .map(|v| parse_float(&String::from_utf16_lossy(&v)))
        .map_err(|e| failure(e.message))
}
fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_positive() || b.is_sign_positive() {
            0.0
        } else {
            -0.0
        }
    } else if a > b {
        a
    } else {
        b
    }
}
fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else if a < b {
        a
    } else {
        b
    }
}
fn plus(left: Option<Frame>, right: Option<Frame>) -> Result<Frame, ListenerFailure> {
    let left = crate::public_data::primitive(left).map_err(|e| failure(e.message))?;
    let right = crate::public_data::primitive(right).map_err(|e| failure(e.message))?;
    if matches!(left, Some(Frame::String(_))) || matches!(right, Some(Frame::String(_))) {
        let mut text = crate::public_data::string(left).map_err(|e| failure(e.message))?;
        text.extend(crate::public_data::string(right).map_err(|e| failure(e.message))?);
        Ok(Frame::String(text))
    } else {
        Ok(Frame::Number(scalar(left)? + scalar(right)?))
    }
}
fn record(fields: IndexMap<Vec<u16>, Option<Frame>>) -> PublicData {
    PublicData::Generated(Rc::new(RefCell::new(fields)))
}
fn candle(timestamp: f64, price: f64) -> PublicData {
    record(
        [
            ("timestamp", timestamp),
            ("open", price),
            ("high", price),
            ("low", price),
            ("close", price),
            ("volume", 0.0),
        ]
        .into_iter()
        .map(|(k, v)| (k.encode_utf16().collect(), Some(Frame::Number(v))))
        .collect(),
    )
}
fn replace_fields(
    base: &PublicData,
    values: impl IntoIterator<Item = (&'static str, Frame)>,
) -> PublicData {
    let mut fields = base.fields();
    for (k, v) in values {
        fields.insert(k.encode_utf16().collect(), Some(v));
    }
    record(fields)
}
fn append<H>(stream: &PublicStreamReceiver<H>, topic: &str, parser: PublicStreamHandler) -> bool {
    let old = stream.message_handlers.borrow().get(topic).cloned();
    let mut values = old.as_ref().map(|v| v.borrow().clone()).unwrap_or_default();
    let subscribe = values.is_empty();
    values.push(parser);
    stream
        .message_handlers
        .borrow_mut()
        .insert(topic.into(), Rc::new(RefCell::new(values)));
    let old = stream.topics.borrow().get(topic).copied().unwrap_or(0.0);
    let old = if old == 0.0 || old.is_nan() { 0.0 } else { old };
    stream.topics.borrow_mut().insert(topic.into(), old + 1.0);
    subscribe
}
fn remove<H>(stream: &PublicStreamReceiver<H>, topic: &str, parser: &PublicStreamHandler) {
    let old = stream.message_handlers.borrow().get(topic).cloned();
    let values = old
        .map(|v| {
            v.borrow()
                .iter()
                .filter(|candidate| match (candidate, parser) {
                    (PublicStreamHandler::Callback(a), PublicStreamHandler::Callback(b)) => {
                        !Rc::ptr_eq(a, b)
                    }
                    _ => true,
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    stream
        .message_handlers
        .borrow_mut()
        .insert(topic.into(), Rc::new(RefCell::new(values)));
}
fn decrement<H>(stream: &PublicStreamReceiver<H>, topic: &str) {
    let old = stream
        .topics
        .borrow()
        .get(topic)
        .copied()
        .unwrap_or(f64::NAN);
    stream.topics.borrow_mut().insert(topic.into(), old - 1.0);
}
fn empty<H>(stream: &PublicStreamReceiver<H>, topic: &str) -> bool {
    stream
        .message_handlers
        .borrow()
        .get(topic)
        .expect("assigned handlers")
        .borrow()
        .is_empty()
}
fn send<H: PublicStreamHost>(
    stream: &PublicStreamReceiver<H>,
    kind: &'static str,
    channel: &str,
    symbol: &str,
) -> Result<(), ListenerFailure> {
    stream.host.send_subscription(PublicSubscription {
        r#type: kind,
        channel: channel.into(),
        product_ids: vec![symbol.into()],
    })
}
fn wait_candles<H: PublicMarketListenerHost + 'static>(
    r: Rc<PublicListenerReceiver<H>>,
    symbol: String,
    parser: PublicStreamHandler,
    secondary: PublicStreamHandler,
) -> Result<(), ListenerFailure> {
    if is_connected(&r) {
        if append(&r.stream, &format!("candles.{symbol}"), parser) {
            send(&r.stream, "subscribe", "candles", &symbol)?;
        }
        if !r.stream.host.public_data_only_option() {
            if append(&r.stream, &format!("market_trades.{symbol}"), secondary) {
                send(&r.stream, "subscribe", "market_trades", &symbol)?;
            }
        }
    } else {
        let next = r.clone();
        r.stream.host.set_timeout(
            Rc::new(move || {
                wait_candles(
                    next.clone(),
                    symbol.clone(),
                    parser.clone(),
                    secondary.clone(),
                )
            }),
            100.0,
        )?;
    }
    Ok(())
}
pub fn listen_ohlcv<H: PublicMarketListenerHost + 'static>(
    r: Rc<PublicListenerReceiver<H>>,
    opts: PublicOHLCVOptionsRoot,
    callback: PublicCandleCallback,
) -> Result<PublicCleanup, ListenerFailure> {
    let symbol = normalize_coinbase_usd_equivalent_product_id(&opts.borrow().symbol);
    let seconds = r
        .stream
        .host
        .timeframe_seconds(&opts.borrow().interval)
        .filter(|v| *v != 0.0 && !v.is_nan())
        .unwrap_or(f64::INFINITY);
    let rely = seconds < 300.0;
    let last = Rc::new(RefCell::new(None::<PublicData>));
    let (receiver, options, last_primary, cb) =
        (r.clone(), opts.clone(), last.clone(), callback.clone());
    let parser = PublicStreamHandler::Callback(Rc::new(move |message| {
        if rely {
            return Ok(());
        }
        let incoming = data(message, "close")?;
        *last_primary.borrow_mut() = Some(incoming);
        let interval = options.borrow().interval.clone();
        if interval.ends_with('d') || interval.ends_with('w') || interval.ends_with('M') {
            let name = options.borrow().symbol.clone();
            if receiver.stream.host.daily_candle(&name).is_none() {
                receiver
                    .stream
                    .host
                    .warn("fetching daily candle for formatting", None)?;
                let to = receiver.stream.host.utc_end_of_day_ms()?;
                let name = options.borrow().symbol.clone();
                receiver
                    .stream
                    .host
                    .fetch_ohlcv_detached(Rc::new(RefCell::new(PublicOHLCVOptions {
                        symbol: name,
                        interval: "1d".into(),
                        limit: Some(Value::from(2)),
                        from: None,
                        to: Some(Value::from(to)),
                    })))?;
                return Ok(());
            }
            let name = options.borrow().symbol.clone();
            let daily = receiver
                .stream
                .host
                .daily_candle(&name)
                .expect("ordinary cache lookup unchanged");
            let name = options.borrow().symbol.clone();
            let spread = receiver
                .stream
                .host
                .daily_candle(&name)
                .expect("ordinary cache lookup unchanged");
            let current = last_primary.borrow().clone().expect("assigned candle");
            let merged = replace_fields(
                &spread,
                [
                    (
                        "high",
                        Frame::Number(max(
                            scalar(daily.get("high"))?,
                            scalar(current.get("close"))?,
                        )),
                    ),
                    (
                        "low",
                        Frame::Number(min(
                            scalar(daily.get("low"))?,
                            scalar(current.get("close"))?,
                        )),
                    ),
                    (
                        "close",
                        current.get("close").unwrap_or(Frame::Number(f64::NAN)),
                    ),
                    ("volume", plus(daily.get("volume"), current.get("volume"))?),
                ],
            );
            if current.get("close").is_none() {
                if let PublicData::Generated(fields) = &merged {
                    fields
                        .borrow_mut()
                        .insert("close".encode_utf16().collect(), None);
                }
            }
            *last_primary.borrow_mut() = Some(merged.clone());
            let name = options.borrow().symbol.clone();
            receiver.stream.host.set_daily_candle(name, merged)?;
        }
        let current = last_primary.borrow().clone().expect("assigned candle");
        cb(current)
    }));
    let (receiver, last_secondary, cb) = (r.clone(), last.clone(), callback.clone());
    let secondary = PublicStreamHandler::Callback(Rc::new(move |message| {
        let trade = data(message, "price")?;
        let price = float(trade.get("price"))?;
        let time = receiver.stream.host.dayjs_unix(trade.get("time"))?;
        if rely && last_secondary.borrow().is_none() {
            *last_secondary.borrow_mut() = Some(candle(time - time % seconds, price));
        }
        let Some(mut current) = last_secondary.borrow().clone() else {
            return Ok(());
        };
        if time < scalar(current.get("timestamp"))? {
            return Ok(());
        }
        if rely {
            let timestamp = time - time % seconds;
            if !matches!(current.get("timestamp"), Some(Frame::Number(v)) if timestamp == v) {
                current = candle(timestamp, price);
                *last_secondary.borrow_mut() = Some(current.clone());
            }
        }
        let volume = float(trade.get("size"))?;
        // Object spread and high/low/close expressions precede imported add().
        let mut fields = current.fields();
        fields.insert(
            "high".encode_utf16().collect(),
            Some(Frame::Number(max(scalar(current.get("high"))?, price))),
        );
        fields.insert(
            "low".encode_utf16().collect(),
            Some(Frame::Number(min(scalar(current.get("low"))?, price))),
        );
        fields.insert("close".encode_utf16().collect(), Some(Frame::Number(price)));
        let next_volume = receiver
            .stream
            .host
            .add(current.get("volume"), volume / price)?;
        fields.insert(
            "volume".encode_utf16().collect(),
            Some(Frame::Number(next_volume)),
        );
        let merged = record(fields);
        *last_secondary.borrow_mut() = Some(merged.clone());
        cb(merged)
    }));
    wait_candles(r.clone(), symbol.clone(), parser.clone(), secondary.clone())?;
    Ok(Rc::new(move || {
        let primary_topic = format!("candles.{symbol}");
        let secondary_topic = format!("market_trades.{symbol}");
        r.stream
            .host
            .cleanup_message_handler_stats(&primary_topic)?;
        r.stream
            .host
            .cleanup_message_handler_stats(&secondary_topic)?;
        remove(&r.stream, &primary_topic, &parser);
        remove(&r.stream, &secondary_topic, &secondary);
        decrement(&r.stream, &primary_topic);
        decrement(&r.stream, &secondary_topic);
        if is_connected(&r) && empty(&r.stream, &primary_topic) {
            send(&r.stream, "unsubscribe", "candles", &symbol)?;
        }
        if is_connected(&r) && empty(&r.stream, &secondary_topic) {
            send(&r.stream, "unsubscribe", "market_trades", &symbol)?;
        }
        Ok(())
    }))
}
fn wait_book<H: PublicMarketListenerHost + 'static>(
    r: Rc<PublicListenerReceiver<H>>,
    symbol: String,
    parser: PublicStreamHandler,
) -> Result<(), ListenerFailure> {
    if is_connected(&r) {
        if append(&r.stream, &format!("level2.{symbol}"), parser) {
            send(&r.stream, "subscribe", "level2", &symbol)?;
        }
    } else {
        let next = r.clone();
        r.stream.host.set_timeout(
            Rc::new(move || wait_book(next.clone(), symbol.clone(), parser.clone())),
            100.0,
        )?;
    }
    Ok(())
}
pub fn listen_order_book<H: PublicMarketListenerHost + 'static>(
    r: Rc<PublicListenerReceiver<H>>,
    symbol: String,
    callback: PublicBookCallback,
) -> Result<PublicCleanup, ListenerFailure> {
    let symbol = normalize_coinbase_usd_equivalent_product_id(&symbol);
    let id = r.stream.host.nanoid()?;
    let book = Rc::new(RefCell::new(PublicBook {
        bids: Rc::new(RefCell::new(vec![])),
        asks: Rc::new(RefCell::new(vec![])),
    }));
    if !r.inner_state.borrow().contains_key(&id) {
        r.inner_state.borrow_mut().insert(
            id.clone(),
            Rc::new(RefCell::new(PublicBookInnerState {
                updates: vec![],
                is_snapshot_loaded: false,
            })),
        );
    }
    let (receiver, book_state) = (r.clone(), book.clone());
    let parser = PublicStreamHandler::Callback(Rc::new(move |message| {
        let event = data(message, "type")?;
        let kind = event.get("type");
        let snapshot = matches!(&kind,Some(Frame::String(v)) if v.iter().copied().eq("snapshot".encode_utf16()));
        let update =
            matches!(&kind,Some(Frame::String(v)) if v.iter().copied().eq("update".encode_utf16()));
        if snapshot {
            book_state.borrow_mut().bids = Rc::new(RefCell::new(vec![]));
            book_state.borrow_mut().asks = Rc::new(RefCell::new(vec![]));
        } else if !update {
            receiver.stream.host.warn("Unknown event type", kind)?;
            return Ok(());
        }
        let loaded = receiver
            .inner_state
            .borrow()
            .get(&id)
            .expect("shared inner state")
            .borrow()
            .is_snapshot_loaded;
        if snapshot || loaded {
            let updates = event.get("updates");
            let Some(Frame::Array(updates)) = updates else {
                return Err(failure(if updates.is_none() {
                    "Cannot read properties of undefined (reading 'forEach')"
                } else if matches!(updates, Some(Frame::Null)) {
                    "Cannot read properties of null (reading 'forEach')"
                } else {
                    "event.updates.forEach is not a function"
                }));
            };
            let len = updates.borrow().len();
            for i in 0..len {
                let Some(raw) = updates.borrow().get(i).cloned() else {
                    continue;
                };
                let value = data(PublicStreamMessage::Frame(raw), "side")?;
                let side = value.get("side");
                let side = match side {
                    Some(Frame::String(v)) if v.iter().copied().eq("bid".encode_utf16()) => "bids",
                    Some(Frame::String(v)) if v.iter().copied().eq("offer".encode_utf16()) => {
                        "asks"
                    }
                    _ => continue,
                };
                let level = Rc::new(RefCell::new(PublicBookLevel {
                    price: float(value.get("price_level"))?,
                    amount: float(value.get("new_quantity"))?,
                    total: 0.0,
                }));
                if snapshot {
                    let levels = if side == "asks" {
                        book_state.borrow().asks.clone()
                    } else {
                        book_state.borrow().bids.clone()
                    };
                    levels.borrow_mut().push(level);
                } else {
                    receiver
                        .stream
                        .host
                        .update_order_book(book_state.clone(), side, level)?;
                }
            }
            if snapshot {
                receiver
                    .inner_state
                    .borrow()
                    .get(&id)
                    .expect("shared inner state")
                    .borrow_mut()
                    .is_snapshot_loaded = true;
            }
        }
        let nonempty = !book_state.borrow().asks.borrow().is_empty()
            || !book_state.borrow().bids.borrow().is_empty();
        if nonempty {
            receiver.stream.host.sort_order_book(book_state.clone())?;
            receiver
                .stream
                .host
                .calc_order_book_total(book_state.clone())?;
            callback(book_state.clone())?;
        }
        Ok(())
    }));
    wait_book(r.clone(), symbol.clone(), parser.clone())?;
    Ok(Rc::new(move || {
        let topic = format!("level2.{symbol}");
        r.stream.host.cleanup_message_handler_stats(&topic)?;
        remove(&r.stream, &topic, &parser);
        decrement(&r.stream, &topic);
        if is_connected(&r) && empty(&r.stream, &topic) {
            send(&r.stream, "unsubscribe", "level2", &symbol)?;
        }
        Ok(())
    }))
}
pub trait PublicMarketDelegateHost {
    fn public_listen_ohlcv(
        &self,
        opts: PublicOHLCVOptionsRoot,
        callback: PublicCandleCallback,
    ) -> Result<PublicCleanup, ListenerFailure>;
    fn public_listen_order_book(
        &self,
        symbol: String,
        callback: PublicBookCallback,
    ) -> Result<PublicCleanup, ListenerFailure>;
}
pub fn exchange_listen_ohlcv(
    host: &impl PublicMarketDelegateHost,
    opts: PublicOHLCVOptionsRoot,
    callback: PublicCandleCallback,
) -> Result<PublicCleanup, ListenerFailure> {
    host.public_listen_ohlcv(opts, callback)
}
pub fn exchange_listen_order_book(
    host: &impl PublicMarketDelegateHost,
    symbol: String,
    callback: PublicBookCallback,
) -> Result<PublicCleanup, ListenerFailure> {
    host.public_listen_order_book(symbol, callback)
}
