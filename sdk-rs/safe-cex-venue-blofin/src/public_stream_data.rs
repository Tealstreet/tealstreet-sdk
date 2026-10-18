//! Remaining bodies of blofin.ws-public.ts, composed with public_stream_core.
//! Hosts supply the inherited socket lifecycle; timers must defer their callbacks.
use crate::{public_stream_core as core, utility::UtilityInput};
use safe_cex_num::{js_truthy, numeric_ingress::js_from_str, parse_float, parse_int, round_usd};
use safe_cex_store::{
    Store, StoreMutationError, model_patch::ModelPatch, orderbook::calc_order_book_total,
    update_order_book::update_order_book,
};
use safe_cex_types::{
    Candle, OHLCVOptions, OrderBook, OrderBookOrders, TIMEFRAME_TO_SECONDS,
    js_coercion::{JsCoercionError, js_string},
    object_map::JsObjectMap,
};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug)]
pub enum PublicDataError<E> {
    Host(E),
    Coercion(JsCoercionError),
    Shape(String),
    Store(StoreMutationError),
    FrameTooLarge,
}
impl<E: std::fmt::Display> std::fmt::Display for PublicDataError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(e) => e.fmt(f),
            Self::Coercion(e) => f.write_str(e.message),
            Self::Shape(s) => f.write_str(s),
            Self::Store(e) => e.fmt(f),
            Self::FrameTooLarge => {
                f.write_str("BloFin subscription topic exceeds the maximum WebSocket frame size")
            }
        }
    }
}
impl<E> From<JsCoercionError> for PublicDataError<E> {
    fn from(e: JsCoercionError) -> Self {
        Self::Coercion(e)
    }
}
fn from_core<E>(e: core::PublicStreamError<E>) -> PublicDataError<E> {
    match e {
        core::PublicStreamError::Host(e) => PublicDataError::Host(e),
        core::PublicStreamError::FrameTooLarge => PublicDataError::FrameTooLarge,
    }
}
pub type PublicDataCallback<E> =
    Rc<dyn Fn(Rc<RefCell<UtilityInput>>) -> Result<(), PublicDataError<E>>>;
pub type PublicCleanup<E> = Rc<dyn Fn() -> Result<(), PublicDataError<E>>>;
pub type CandleCallback<E> = Rc<dyn Fn(Rc<RefCell<Candle>>) -> Result<(), E>>;
pub type BookCallback<E> = Rc<dyn Fn(Rc<RefCell<OrderBook>>) -> Result<(), E>>;
pub type CloseCallback<E> = Rc<dyn Fn(Option<&Value>) -> Result<(), E>>;

pub trait PublicDataHost: core::PublicStreamHost {
    fn store(&self) -> &dyn Store;
    fn is_connected(&self) -> bool;
    fn super_construct(&self) -> Result<(), Self::Error>;
    fn set_treat_any_message_as_pong(&self, enabled: bool);
    fn base_close_handler(&self) -> CloseCallback<Self::Error>;
    fn super_dispose(&self) -> Result<(), Self::Error>;
    fn track_last_received(&self) -> Result<(), Self::Error>;
    fn handle_pong(&self) -> Result<(), Self::Error>;
    fn cleanup_message_handler_stats(&self, key: &str) -> Result<(), Self::Error>;
    /// Bridge a scheduled body failure to the inherited timer's host error type.
    fn scheduled_error(&self, error: PublicDataError<Self::Error>) -> Self::Error;
}

type HandlerArray<E> = Rc<RefCell<Vec<PublicDataCallback<E>>>>;
type HandlerTable<E> = RefCell<JsObjectMap<HandlerArray<E>>>;
pub struct BlofinPublicWebsocket<H: PublicDataHost> {
    pub core: Rc<core::PublicStreamState<H::Timer>>,
    pub message_handlers: HandlerTable<H::Error>,
    host: Rc<H>,
    base_on_close: CloseCallback<H::Error>,
}
impl<H: PublicDataHost> BlofinPublicWebsocket<H> {
    pub fn new(host: Rc<H>) -> Result<Rc<Self>, PublicDataError<H::Error>> {
        host.super_construct().map_err(PublicDataError::Host)?;
        host.set_treat_any_message_as_pong(true);
        let stream = Rc::new(Self {
            core: Rc::default(),
            message_handlers: RefCell::new(JsObjectMap::new()),
            base_on_close: host.base_close_handler(),
            host,
        });
        for (key, funding) in [("tickers", false), ("funding-rate", true)] {
            let weak = Rc::downgrade(&stream);
            let cb: PublicDataCallback<H::Error> = Rc::new(move |data| {
                if let Some(s) = weak.upgrade() {
                    let snapshot = data.borrow().clone();
                    if funding {
                        s.handle_funding_rate_events(&snapshot)
                    } else {
                        s.handle_ticker_events(&snapshot)
                    }
                } else {
                    Ok(())
                }
            });
            stream
                .message_handlers
                .borrow_mut()
                .insert(key.into(), Rc::new(RefCell::new(vec![cb])));
        }
        Ok(stream)
    }
    pub fn on_close(&self, event: Option<&Value>) -> Result<(), PublicDataError<H::Error>> {
        (self.base_on_close)(event).map_err(PublicDataError::Host)?;
        core::arm_connection_failure_notice(&self.core, &self.host).map_err(from_core)
    }
    pub fn connect_and_subscribe(&self) -> Result<(), PublicDataError<H::Error>> {
        core::connect_and_subscribe(&self.core, &self.host).map_err(from_core)
    }
    pub fn on_open(&self) -> Result<(), PublicDataError<H::Error>> {
        core::on_open(&self.core, self.host.as_ref()).map_err(from_core)
    }
    pub fn on_markets_changed(&self, added: &[String]) -> Result<(), PublicDataError<H::Error>> {
        core::on_markets_changed(&self.core, self.host.as_ref(), added).map_err(from_core)
    }
    pub fn ping(&self) -> Result<(), PublicDataError<H::Error>> {
        core::ping(&self.core, self.host.as_ref()).map_err(from_core)
    }
    pub fn subscribe(&self) -> Result<(), PublicDataError<H::Error>> {
        core::subscribe(&self.core, self.host.as_ref()).map_err(from_core)
    }
    pub fn dispose(&self) -> Result<(), PublicDataError<H::Error>> {
        let timeout = self.core.connection_failure_notice_timeout.borrow().clone();
        if let Some(t) = timeout.filter(|t| self.host.timer_is_truthy(t)) {
            self.host.clear_timeout(&t).map_err(PublicDataError::Host)?;
            *self.core.connection_failure_notice_timeout.borrow_mut() = None;
        }
        self.host
            .markets_changed_off()
            .map_err(PublicDataError::Host)?;
        self.host.super_dispose().map_err(PublicDataError::Host)
    }
    pub fn on_message(&self, data: &str) -> Result<(), PublicDataError<H::Error>> {
        if self.host.is_disposed() {
            return Ok(());
        }
        core::mark_connection_established(&self.core, self.host.as_ref()).map_err(from_core)?;
        self.host
            .track_last_received()
            .map_err(PublicDataError::Host)?;
        if data == "pong" {
            return self.host.handle_pong().map_err(PublicDataError::Host);
        }
        let Some(idx) = data.find("\"channel\":\"") else {
            return Ok(());
        };
        let start = idx + 11;
        let Some(end) = data[start..]
            .find('"')
            .map(|n| start + n)
            .filter(|n| *n > start)
        else {
            return Ok(());
        };
        let channel = &data[start..end];
        if data.contains("\"event\":\"subscribe\"") {
            return Ok(());
        }
        let broadcast = self.message_handlers.borrow().get(channel).cloned();
        let exact = data[end..].find("\"instId\":\"").and_then(|idx| {
            let start = end + idx + 10;
            data[start..]
                .find('"')
                .filter(|n| *n > 0)
                .map(|n| format!("{channel}.{}", &data[start..start + n]))
        });
        let exact = exact.and_then(|key| self.message_handlers.borrow().get(&key).cloned());
        let has_broadcast = broadcast.as_ref().is_some_and(|v| !v.borrow().is_empty());
        let has_exact = exact.as_ref().is_some_and(|v| !v.borrow().is_empty());
        let inherited = !self.message_handlers.borrow().contains_key(channel)
            && matches!(
                channel,
                "constructor"
                    | "hasOwnProperty"
                    | "isPrototypeOf"
                    | "propertyIsEnumerable"
                    | "__defineGetter__"
                    | "__defineSetter__"
                    | "__lookupGetter__"
                    | "__lookupSetter__"
            );
        if !has_broadcast && !has_exact && !inherited {
            return Ok(());
        }
        let Ok(json) = js_from_str::<UtilityInput>(data) else {
            return Ok(());
        };
        if !js_truthy(Some(json.json())) {
            return Ok(());
        }
        if inherited {
            return Err(PublicDataError::Shape("broadcast is not iterable".into()));
        }
        let json = Rc::new(RefCell::new(json));
        for (handlers, has) in [(broadcast, has_broadcast), (exact, has_exact)] {
            if has {
                let handlers = handlers.expect("nonempty array");
                let mut idx = 0;
                loop {
                    let cb = handlers.borrow().get(idx).cloned();
                    let Some(cb) = cb else { break };
                    cb(json.clone())?;
                    idx += 1;
                }
            }
        }
        Ok(())
    }
    pub fn handle_ticker_events(
        &self,
        data: &UtilityInput,
    ) -> Result<(), PublicDataError<H::Error>> {
        let update = first_update(data, "open24h")?;
        let open = float(update.get("open24h"))?;
        let last = float(update.get("last"))?;
        let percentage = round_usd(((last - open) / open) * 100.0);
        let mut patch = ModelPatch::default();
        for (key, value) in [
            ("bid", float(update.get("bidPrice"))?),
            ("ask", float(update.get("askPrice"))?),
            ("last", last),
            ("mark", last),
            ("index", last),
            ("openInterest", 0.0),
            ("percentage", percentage),
            ("volume", float(update.get("volCurrency24h"))?),
            ("quoteVolume", float(update.get("vol24h"))?),
        ] {
            patch.set_number(key, value);
        }
        self.host
            .store()
            .update_ticker(&js_string(update.get("instId"))?, &patch)
            .map_err(PublicDataError::Store)
    }
    pub fn handle_funding_rate_events(
        &self,
        data: &UtilityInput,
    ) -> Result<(), PublicDataError<H::Error>> {
        let update = first_update(data, "instId")?;
        let id = js_string(update.get("instId"))?;
        let mut patch = ModelPatch::default();
        patch.set_number("fundingRate", float(update.get("fundingRate"))?);
        patch.set_number("fundingTime", float(update.get("fundingTime"))?);
        self.host
            .store()
            .update_ticker(&id, &patch)
            .map_err(PublicDataError::Store)
    }
    fn send_topic(
        &self,
        op: &str,
        topic: &core::SubscriptionTopic,
    ) -> Result<(), PublicDataError<H::Error>> {
        send_topic_host(self.host.as_ref(), op, topic)
    }

    fn add_handler(&self, key: &str, cb: PublicDataCallback<H::Error>) {
        let mut handlers = self.message_handlers.borrow_mut();
        let mut combined = handlers
            .get(key)
            .map(|v| v.borrow().clone())
            .unwrap_or_default();
        combined.push(cb);
        handlers.insert(key.into(), Rc::new(RefCell::new(combined)));
    }
    fn remove_handler(
        &self,
        key: &str,
        cb: &PublicDataCallback<H::Error>,
    ) -> Result<bool, PublicDataError<H::Error>> {
        let mut retained = self
            .message_handlers
            .borrow()
            .get(key)
            .map(|v| v.borrow().clone())
            .unwrap_or_default();
        retained.retain(|f| !Rc::ptr_eq(f, cb));
        let empty = retained.is_empty();
        self.message_handlers
            .borrow_mut()
            .insert(key.into(), Rc::new(RefCell::new(retained)));
        if empty {
            self.core.topics.borrow_mut().remove(key);
            self.host
                .cleanup_message_handler_stats(key)
                .map_err(PublicDataError::Host)?;
        }
        Ok(empty)
    }
    fn schedule(
        &self,
        callback: PublicCleanup<H::Error>,
        timeout: &Rc<RefCell<Option<H::Timer>>>,
    ) -> Result<(), PublicDataError<H::Error>> {
        let host = self.host.clone();
        let timer = self
            .host
            .set_timeout(
                Rc::new(move || callback().map_err(|e| host.scheduled_error(e))),
                100.0,
            )
            .map_err(PublicDataError::Host)?;
        *timeout.borrow_mut() = Some(timer);
        Ok(())
    }
    fn clear_listener_timeout(
        &self,
        timeout: &RefCell<Option<H::Timer>>,
    ) -> Result<(), PublicDataError<H::Error>> {
        let t = timeout.borrow().clone();
        if let Some(t) = t.filter(|t| self.host.timer_is_truthy(t)) {
            self.host.clear_timeout(&t).map_err(PublicDataError::Host)?;
            *timeout.borrow_mut() = None;
        }
        Ok(())
    }
    fn wait_and_subscribe(
        self: &Rc<Self>,
        entries: Rc<Vec<ListenerEntry<H::Error>>>,
        timeout: Rc<RefCell<Option<H::Timer>>>,
    ) -> Result<(), PublicDataError<H::Error>> {
        if self.host.is_connected() {
            if !self.host.is_disposed() {
                for entry in entries.iter() {
                    self.add_handler(&entry.key, entry.callback.clone());
                    self.send_topic("subscribe", &entry.topic)?;
                    self.core
                        .topics
                        .borrow_mut()
                        .insert(entry.key.clone(), vec![entry.topic.clone()]);
                }
            }
        } else {
            let stream = self.clone();
            let timeout2 = timeout.clone();
            self.schedule(
                Rc::new(move || stream.wait_and_subscribe(entries.clone(), timeout2.clone())),
                &timeout,
            )?;
        }
        Ok(())
    }
    fn listener_cleanup(
        self: &Rc<Self>,
        entries: Rc<Vec<ListenerEntry<H::Error>>>,
        timeout: Rc<RefCell<Option<H::Timer>>>,
        book: Option<Rc<RefCell<BookState>>>,
    ) -> PublicCleanup<H::Error> {
        let stream = self.clone();
        Rc::new(move || {
            for entry in entries.iter() {
                stream.remove_handler(&entry.key, &entry.callback)?;
            }
            if let Some(book) = &book {
                let retained = book.borrow().book.clone();
                retained.borrow_mut().bids.clear();
                retained.borrow_mut().asks.clear();
            }
            stream.clear_listener_timeout(&timeout)?;
            for entry in entries.iter() {
                if stream.host.is_connected()
                    && stream
                        .message_handlers
                        .borrow()
                        .get(&entry.key)
                        .is_none_or(|v| v.borrow().is_empty())
                {
                    stream.send_topic("unsubscribe", &entry.topic)?;
                }
            }
            Ok(())
        })
    }
    pub fn listen_ohlcv(
        self: &Rc<Self>,
        opts: OHLCVOptions,
        callback: CandleCallback<H::Error>,
    ) -> Result<PublicCleanup<H::Error>, PublicDataError<H::Error>> {
        let timeout = Rc::new(RefCell::new(None));
        if !self.host.store().loaded()?.markets {
            let stream = self.clone();
            self.schedule(
                Rc::new(move || {
                    stream.listen_ohlcv(opts.clone(), callback.clone())?;
                    Ok(())
                }),
                &timeout,
            )?;
            let stream = self.clone();
            return Ok(Rc::new(move || stream.clear_listener_timeout(&timeout)));
        }
        let market = self
            .host
            .store()
            .markets()?
            .iter()
            .find(|m| m.symbol == opts.symbol)
            .cloned();
        let Some(market) = market else {
            return Ok(Rc::new(|| Ok(())));
        };
        let interval = serde_json::to_value(opts.interval).expect("timeframe enum");
        let interval = interval.as_str().expect("timeframe string");
        let channel = format!(
            "candle{}",
            crate::types::INTERVAL
                .get(interval)
                .and_then(Value::as_str)
                .unwrap_or("undefined")
        );
        let last = Rc::new(RefCell::new(None::<Rc<RefCell<Candle>>>));
        let parser_last = last.clone();
        let cb = callback.clone();
        let parser: PublicDataCallback<H::Error> = Rc::new(move |data| {
            let Some(next) = parse_candle(&data.borrow())? else {
                return Ok(());
            };
            let same = parser_last
                .borrow()
                .as_ref()
                .is_some_and(|old| same_candle(&old.borrow(), &next));
            if same {
                return Ok(());
            }
            let next = Rc::new(RefCell::new(next));
            *parser_last.borrow_mut() = Some(next.clone());
            cb(next).map_err(PublicDataError::Host)
        });
        let host = self.host.clone();
        let seconds = TIMEFRAME_TO_SECONDS
            .get(interval)
            .copied()
            .unwrap_or(f64::NAN);
        let secondary: PublicDataCallback<H::Error> = Rc::new(move |data| {
            let data = data.borrow().clone();
            let Some(snapshot) = book_snapshot(&data, true) else {
                let text = safe_cex_num::js_json_stringify(&safe_cex_num::JsJsonValue::from(
                    data.json().clone(),
                ))
                .expect("JSON-only snapshot")
                .unwrap_or_default();
                return host
                    .log_warning(&format!(
                        "BloFin: Invalid orderbook snapshot shape. Received: {text}"
                    ))
                    .map_err(PublicDataError::Host);
            };
            let timestamp = integer(snapshot.get("ts"))?;
            let input = timestamp / 1000.0;
            let normalized = if input > 1_000_000_000_000.0 {
                (input / 1000.0).floor()
            } else {
                input
            };
            let mut candle_start = (normalized / seconds).floor() * seconds;
            if input > 1_000_000_000_000.0 {
                candle_start *= 1000.0;
            }
            let bid = top_price(snapshot, "bids")?;
            let ask = top_price(snapshot, "asks")?;
            let previous = last.borrow().clone();
            let Some(previous) = previous else {
                return Ok(());
            };
            let candle = previous.borrow().clone();
            if !bid.is_finite()
                || !ask.is_finite()
                || !candle.close.is_finite()
                || candle.timestamp != candle_start
            {
                return Ok(());
            }
            let mut prices = [bid, candle.close, ask];
            prices.sort_by(|a, b| a.partial_cmp(b).expect("finite prices"));
            let close = prices[1];
            if close == candle.close {
                return Ok(());
            }
            let next = Rc::new(RefCell::new(Candle {
                close,
                high: js_max(candle.high, close),
                low: js_min(candle.low, close),
                ..candle
            }));
            *last.borrow_mut() = Some(next.clone());
            callback(next).map_err(PublicDataError::Host)
        });
        let entries = Rc::new(vec![
            entry(&channel, &market.id, parser),
            entry("books5", &market.id, secondary),
        ]);
        self.wait_and_subscribe(entries.clone(), timeout.clone())?;
        Ok(self.listener_cleanup(entries, timeout, None))
    }
    pub fn listen_order_book(
        self: &Rc<Self>,
        symbol: String,
        callback: BookCallback<H::Error>,
    ) -> Result<PublicCleanup<H::Error>, PublicDataError<H::Error>> {
        let timeout = Rc::new(RefCell::new(None));
        if !self.host.store().loaded()?.markets {
            let stream = self.clone();
            self.schedule(
                Rc::new(move || {
                    stream.listen_order_book(symbol.clone(), callback.clone())?;
                    Ok(())
                }),
                &timeout,
            )?;
            let stream = self.clone();
            return Ok(Rc::new(move || stream.clear_listener_timeout(&timeout)));
        }
        let market = self
            .host
            .store()
            .markets()?
            .iter()
            .find(|m| m.symbol == symbol)
            .cloned();
        let Some(market) = market else {
            return Ok(Rc::new(|| Ok(())));
        };
        let book = Rc::new(RefCell::new(BookState::default()));
        let primary_book = book.clone();
        let host = self.host.clone();
        let cb = callback.clone();
        let primary_topic = make_topic("books", &market.id);
        let ticker_symbol = market.symbol.clone();
        let parser: PublicDataCallback<H::Error> = Rc::new(move |data| {
            let data = data.borrow().clone();
            let mut state = primary_book.borrow_mut();
            let retained = state.book.clone();
            let mut book = retained.borrow_mut();
            let action = data.json().get("action").and_then(Value::as_str);
            if action == Some("snapshot") {
                let Some(snapshot) = book_snapshot(&data, false) else {
                    return Ok(());
                };
                state.last_seq = optional_seq(snapshot.get("seqId"))?;
                state.has_snapshot = true;
                state.resubscribe_pending = false;
                book.bids = levels(snapshot, "bids")?;
                book.asks = levels(snapshot, "asks")?;
            }
            if action == Some("update") {
                let Some(update) = book_snapshot(&data, false) else {
                    return Ok(());
                };
                let prev = optional_seq(update.get("prevSeqId"))?;
                let seq = optional_seq(update.get("seqId"))?;
                if !state.has_snapshot
                    || state.last_seq.as_ref().is_none_or(String::is_empty)
                    || prev.as_ref().is_none_or(String::is_empty)
                    || seq.as_ref().is_none_or(String::is_empty)
                    || prev != state.last_seq
                {
                    let pending = state.resubscribe_pending;
                    drop(book);
                    drop(state);
                    if !pending && host.is_connected() && !host.is_disposed() {
                        send_topic_host(host.as_ref(), "unsubscribe", &primary_topic)?;
                        send_topic_host(host.as_ref(), "subscribe", &primary_topic)?;
                    }
                    let mut state = primary_book.borrow_mut();
                    state.resubscribe_pending = true;
                    state.has_snapshot = false;
                    state.last_seq = None;
                    drop(state);
                    let mut book = retained.borrow_mut();
                    book.bids.clear();
                    book.asks.clear();
                    return Ok(());
                }
                for (side, bids) in [("bids", true), ("asks", false)] {
                    for level in raw_levels(update, side)? {
                        update_order_book(&mut book, bids, parse_level(level)?, true);
                    }
                }
                state.last_seq = seq;
            }
            drop(book);
            drop(state);
            let last = host
                .store()
                .tickers_by_symbol()?
                .get(&ticker_symbol)
                .map(|t| t.last);
            let mut book = retained.borrow_mut();
            if last.is_some_and(|n| n != 0.0 && !n.is_nan()) {
                let last = last.expect("truthy last");
                book.asks.retain(|l| l.price >= last);
                book.bids.retain(|l| l.price <= last);
            }
            calc_order_book_total(&mut book);
            drop(book);
            cb(retained).map_err(PublicDataError::Host)
        });
        let secondary_book = book.clone();
        let secondary: PublicDataCallback<H::Error> = Rc::new(move |data| {
            let data = data.borrow().clone();
            let Some(snapshot) = book_snapshot(&data, false) else {
                return Ok(());
            };
            let retained = secondary_book.borrow().book.clone();
            let mut book = retained.borrow_mut();
            for (side, bids) in [("bids", true), ("asks", false)] {
                let levels = if bids { &mut book.bids } else { &mut book.asks };
                let mut idx = 0;
                for row in raw_levels(snapshot, side)? {
                    if let Some(level) = snapshot_level(row)? {
                        if idx < levels.len() {
                            levels[idx] = level
                        } else {
                            levels.push(level)
                        }
                        idx += 1;
                    }
                }
            }
            calc_order_book_total(&mut book);
            drop(book);
            callback(retained).map_err(PublicDataError::Host)
        });
        let entries = Rc::new(vec![
            entry("books", &market.id, parser),
            entry("books5", &market.id, secondary),
        ]);
        self.wait_and_subscribe(entries.clone(), timeout.clone())?;
        Ok(self.listener_cleanup(entries, timeout, Some(book)))
    }
}
struct ListenerEntry<E> {
    key: String,
    topic: core::SubscriptionTopic,
    callback: PublicDataCallback<E>,
}
fn make_topic(channel: &str, id: &str) -> core::SubscriptionTopic {
    core::SubscriptionTopic {
        fields: [
            ("channel".into(), Value::String(channel.into())),
            ("instId".into(), Value::String(id.into())),
        ]
        .into_iter()
        .collect(),
    }
}
fn entry<E>(channel: &str, id: &str, callback: PublicDataCallback<E>) -> ListenerEntry<E> {
    ListenerEntry {
        key: format!("{channel}.{id}"),
        topic: make_topic(channel, id),
        callback,
    }
}
fn first_update<E>(data: &UtilityInput, field: &str) -> Result<Value, PublicDataError<E>> {
    let rows = data.json().get("data");
    let row = match rows {
        Some(Value::Array(rows)) => rows.first().cloned(),
        Some(Value::String(text)) => text.chars().next().map(|c| Value::String(c.to_string())),
        Some(Value::Null) => {
            return Err(PublicDataError::Shape("Cannot destructure property 'Symbol(Symbol.iterator)' of 'object null' as it is null.".into()));
        }
        _ => return Err(PublicDataError::Shape(iterable_error(rows)?)),
    };
    match row {
        Some(Value::Null) => Err(PublicDataError::Shape(format!(
            "Cannot read properties of null (reading '{field}')"
        ))),
        Some(row) => Ok(row),
        None => Err(PublicDataError::Shape(format!(
            "Cannot read properties of undefined (reading '{field}')"
        ))),
    }
}
fn float(v: Option<&Value>) -> Result<f64, JsCoercionError> {
    Ok(parse_float(&js_string(v)?))
}
fn integer(v: Option<&Value>) -> Result<f64, JsCoercionError> {
    Ok(parse_int(&js_string(v)?, Some(10)))
}
fn nullish<'a>(values: impl IntoIterator<Item = Option<&'a Value>>) -> Option<&'a Value> {
    values.into_iter().flatten().find(|v| !v.is_null())
}
fn book_snapshot(data: &UtilityInput, needs_ts: bool) -> Option<&Value> {
    let payload = data.json().get("data")?;
    let payload = if let Some(rows) = payload.as_array() {
        rows.first()?
    } else {
        payload
    };
    (payload.get("bids")?.is_array()
        && payload.get("asks")?.is_array()
        && (!needs_ts || js_truthy(payload.get("ts"))))
    .then_some(payload)
}
fn parse_candle<E>(data: &UtilityInput) -> Result<Option<Candle>, PublicDataError<E>> {
    let raw = data.json().get("data");
    let row = if let Some(Value::Array(rows)) = raw {
        rows.first()
    } else {
        raw.filter(|r| r.is_object())
    };
    let Some(row) = row.filter(|r| js_truthy(Some(r))) else {
        return Ok(None);
    };
    let (timestamp, open, high, low, close, volume) = if row.is_array() {
        (
            integer(row.get(0))? / 1000.0,
            float(row.get(1))?,
            float(row.get(2))?,
            float(row.get(3))?,
            float(row.get(4))?,
            float(nullish([row.get(7), row.get(5), row.get(6)]).or(Some(&Value::from(0))))?,
        )
    } else {
        let ts = integer(
            nullish([
                row.get("ts"),
                row.get("timestamp"),
                row.get("t"),
                row.get("time"),
            ])
            .or(Some(&Value::String(String::new()))),
        )?;
        let fields = (
            if ts > 1_000_000_000_000.0 {
                ts / 1000.0
            } else {
                ts
            },
            float(
                nullish([row.get("open"), row.get("o")]).or(Some(&Value::String(String::new()))),
            )?,
            float(
                nullish([row.get("high"), row.get("h")]).or(Some(&Value::String(String::new()))),
            )?,
            float(nullish([row.get("low"), row.get("l")]).or(Some(&Value::String(String::new()))))?,
            float(
                nullish([row.get("close"), row.get("c")]).or(Some(&Value::String(String::new()))),
            )?,
            float(
                nullish([row.get("volume"), row.get("vol"), row.get("v")])
                    .or(Some(&Value::from(0))),
            )?,
        );
        if !ts.is_finite() || ts <= 0.0 {
            return Ok(None);
        }
        fields
    };
    Ok([timestamp, open, high, low, close, volume]
        .iter()
        .all(|n| n.is_finite())
        .then_some(Candle {
            timestamp,
            open,
            high,
            low,
            close,
            volume,
            last_side: None,
        }))
}
fn same_candle(a: &Candle, b: &Candle) -> bool {
    a.timestamp == b.timestamp
        && a.open == b.open
        && a.high == b.high
        && a.low == b.low
        && a.close == b.close
        && a.volume == b.volume
}
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_positive() || b.is_sign_positive() {
            0.0
        } else {
            -0.0
        }
    } else {
        a.max(b)
    }
}
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else {
        a.min(b)
    }
}
fn optional_seq(v: Option<&Value>) -> Result<Option<String>, JsCoercionError> {
    v.map(|v| js_string(Some(v))).transpose()
}
fn raw_levels<'a, E>(snapshot: &'a Value, side: &str) -> Result<&'a [Value], PublicDataError<E>> {
    snapshot
        .get(side)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(PublicDataError::Shape("Invalid orderbook levels".into()))
}
fn level_fields<E>(row: &Value) -> Result<(Option<Value>, Option<Value>), PublicDataError<E>> {
    if let Some(rows) = row.as_array() {
        return Ok((rows.first().cloned(), rows.get(1).cloned()));
    }
    if let Some(text) = row.as_str() {
        let mut chars = text.chars();
        let value = |c: char| Value::String(c.to_string());
        return Ok((chars.next().map(value), chars.next().map(value)));
    }
    Err(PublicDataError::Shape(iterable_error(Some(row))?))
}
fn parse_level<E>(row: &Value) -> Result<OrderBookOrders, PublicDataError<E>> {
    let (price, amount) = level_fields(row)?;
    Ok(OrderBookOrders {
        price: float(price.as_ref())?,
        amount: float(amount.as_ref())?,
        total: 0.0,
    })
}
fn snapshot_level<E>(row: &Value) -> Result<Option<OrderBookOrders>, PublicDataError<E>> {
    let (price, amount) = level_fields(row)?;
    let amount = float(amount.as_ref())?;
    if amount == 0.0 {
        return Ok(None);
    }
    Ok(Some(OrderBookOrders {
        price: float(price.as_ref())?,
        amount,
        total: 0.0,
    }))
}
fn levels<E>(snapshot: &Value, side: &str) -> Result<Vec<OrderBookOrders>, PublicDataError<E>> {
    let mut levels = Vec::new();
    for row in raw_levels(snapshot, side)? {
        if let Some(level) = snapshot_level(row)? {
            levels.push(level)
        }
    }
    Ok(levels)
}

struct BookState {
    book: Rc<RefCell<OrderBook>>,
    last_seq: Option<String>,
    has_snapshot: bool,
    resubscribe_pending: bool,
}

impl Default for BookState {
    fn default() -> Self {
        Self {
            book: Rc::new(RefCell::new(OrderBook {
                bids: vec![],
                asks: vec![],
            })),
            last_seq: None,
            has_snapshot: false,
            resubscribe_pending: false,
        }
    }
}

fn send_topic_host<H: PublicDataHost>(
    host: &H,
    op: &str,
    topic: &core::SubscriptionTopic,
) -> Result<(), PublicDataError<H::Error>> {
    let payload = safe_cex_num::JsJsonValue::Object(vec![
        ("op".into(), safe_cex_num::JsJsonValue::String(op.into())),
        (
            "args".into(),
            safe_cex_num::JsJsonValue::Array(vec![safe_cex_num::JsJsonValue::Object(
                topic
                    .fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone().into()))
                    .collect(),
            )]),
        ),
    ]);
    let text = safe_cex_num::js_json_stringify(&payload)
        .expect("own JSON topic")
        .expect("object frame");
    host.send(&text).map_err(PublicDataError::Host)
}

fn top_price(snapshot: &Value, side: &str) -> Result<f64, JsCoercionError> {
    let row = snapshot.get(side).and_then(|v| v.get(0));
    let value = row.and_then(|r| {
        if r.is_array() {
            r.get(0).cloned()
        } else if r.is_object() {
            r.get("0").cloned()
        } else {
            r.as_str()
                .and_then(|s| s.chars().next())
                .map(|c| Value::String(c.to_string()))
        }
    });
    float(value.as_ref())
}

fn iterable_error(v: Option<&Value>) -> Result<String, JsCoercionError> {
    let kind = match v {
        None => "undefined".into(),
        Some(Value::Null) => "object null".into(),
        Some(Value::Bool(b)) => format!("boolean {b}"),
        Some(Value::Number(_)) => format!("number {}", js_string(v)?),
        Some(Value::Object(_)) => "object".into(),
        _ => "object".into(),
    };
    Ok(format!(
        "{kind} is not iterable (cannot read property Symbol(Symbol.iterator))"
    ))
}

impl<E> From<safe_cex_types::StoreReadError> for PublicDataError<E> {
    fn from(error: safe_cex_types::StoreReadError) -> Self {
        Self::Store(error.into())
    }
}
