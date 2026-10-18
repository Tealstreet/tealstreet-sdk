//! Concrete public socket and the exchange's public subscription delegates.
use crate::{
    public_subscriptions::{
        BookCallback, BookCleanup, CandleCallback, PublicSubscriptionIo, PublicSubscriptions,
    },
    ws_helpers::{self, PublicEventDataDispatch, PublicStreamDispatch, PublicStreamTradeEvent},
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
    websocket::{BaseWebSocket, WsParent, WsProtocol, WsRuntime},
};
use safe_cex_num::{js_string, js_truthy};
use safe_cex_store::model_patch::ModelPatch;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{AnyTimeframe, Ticker};
use serde_json::Value;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub trait PublicStreamParent: WsParent {
    fn owner(&self) -> Rc<BaseExchange>;
    fn emit_ticker(
        &self,
        event: &str,
        symbol: &Value,
        ticker: &Ticker,
    ) -> Result<(), ExchangeError>;
    fn emit_trade(&self, trade: &PublicStreamTradeEvent) -> Result<(), ExchangeError>;
}
// ts: AsterDEXPublicWebsocket
pub struct PublicStream {
    pub socket: Rc<BaseWebSocket>,
    pub subscriptions: Rc<PublicSubscriptions>,
    parent: Rc<dyn PublicStreamParent>,
    runtime: Rc<dyn WsRuntime>,
}
struct Protocol(Weak<PublicStream>);
struct Io {
    socket: Rc<BaseWebSocket>,
    runtime: Rc<dyn WsRuntime>,
}
impl PublicSubscriptionIo for Io {
    fn is_disposed(&self) -> bool {
        self.socket.is_disposed()
    }
    fn is_connected(&self) -> bool {
        self.socket.is_connected()
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.socket.send(message)
    }
    fn ping(&self) -> Result<(), ExchangeError> {
        self.socket.ping()
    }
    fn enable_reconnecting(&self) -> bool {
        self.runtime.enable_reconnecting()
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        self.socket.emit_ws_reconnected()
    }
    fn cleanup_message_handler_stats(&self, stream: &str) -> Result<(), ExchangeError> {
        self.socket.cleanup_message_handler_stats(stream)
    }
    fn dispose(&self) {
        self.socket.dispose();
    }
}
impl WsProtocol for Protocol {
    fn ping(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        ws_helpers::public_ping(socket)
    }
    fn connect_and_subscribe(
        &self,
        socket: Rc<BaseWebSocket>,
        _: bool,
    ) -> Result<(), ExchangeError> {
        if let Some(stream) = self.0.upgrade() {
            socket.start_message_operation(stream.connect_and_subscribe());
        }
        Ok(())
    }
    fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let result = self
            .0
            .upgrade()
            .map_or(Ok(()), |s| s.subscriptions.on_open());
        Box::pin(async move { result })
    }
    fn on_error(&self, _: &BaseWebSocket, _: &str) -> Result<(), ExchangeError> {
        // Source's error listener calls this.onClose() without the error event.
        self.0
            .upgrade()
            .map_or(Ok(()), |stream| stream.socket.on_close(None, ""))
    }
    fn on_message(&self, _: &Rc<BaseWebSocket>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        self.0.upgrade().map_or(Ok(()), |s| s.on_message(frame))
    }
}
fn coerce(e: safe_cex_num::JsCoercionError) -> ExchangeError {
    let mut error = ExchangeError::new(e.message);
    error.name = e.name.into();
    error
}
fn string(v: Option<&Value>) -> Result<String, ExchangeError> {
    js_string(v).map_err(coerce)
}
impl PublicStream {
    // ts: AsterDEXPublicWebsocket.constructor
    pub fn new(
        id: u64,
        parent: Rc<dyn PublicStreamParent>,
        runtime: Rc<dyn WsRuntime>,
    ) -> Rc<Self> {
        let stream: Rc<Self> = Rc::new_cyclic(|weak| {
            let socket = BaseWebSocket::new(
                id,
                parent.clone(),
                runtime.clone(),
                Rc::new(Protocol(weak.clone())),
            );
            let subscriptions = PublicSubscriptions::new(
                parent.owner(),
                Rc::new(Io {
                    socket: socket.clone(),
                    runtime: runtime.clone(),
                }),
            );
            Self {
                socket,
                subscriptions,
                parent,
                runtime,
            }
        });
        let weak = Rc::downgrade(&stream);
        stream.socket.set_before_close(Rc::new(move || {
            if let Some(s) = weak.upgrade() {
                s.subscriptions.before_close();
            }
        }));
        for (key, handler) in [
            (
                "24hrTicker",
                Self::handle_ticker_update as fn(&Self, &Value) -> Result<(), ExchangeError>,
            ),
            ("aggTrade", Self::handle_trade_update),
            ("markPriceUpdate", Self::handle_mark_price_update),
            ("kline", Self::handle_kline_update),
        ] {
            let weak = Rc::downgrade(&stream);
            stream.socket.message_handlers.borrow_mut().insert(
                key.into(),
                Rc::new(RefCell::new(vec![Rc::new(move |data: &Value| {
                    weak.upgrade().map_or(Ok(()), |s| handler(&s, data))
                })])),
            );
        }
        stream
    }
    // ts: AsterDEXPublicWebsocket.connectAndSubscribe
    pub fn connect_and_subscribe(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        if self.parent.is_disposed() {
            return Box::pin(async { Ok(()) });
        }
        self.subscriptions.begin_connect();
        let socket = self.socket.clone();
        let url = ws_helpers::public_connection_url(
            self.parent.options().testnet.unwrap_or(false),
            &self.runtime.unique_id(),
        );
        Box::pin(async move {
            let _ = socket.create_websocket(url, Vec::new(), None).await;
            Ok(())
        })
    }
    // ts: AsterDEXPublicWebsocket.onMessage
    pub fn on_message(self: &Rc<Self>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        if self.parent.is_disposed() {
            return Ok(());
        }
        self.socket.track_last_received()?;
        let _ = (|| {
            let text = match frame {
                SocketFrame::Text(s) => s.clone(),
                SocketFrame::Binary(b) => String::from_utf8_lossy(b).into_owned(),
                SocketFrame::Close { .. } => return Ok(()),
            };
            let json = safe_cex_transport::signing::try_parse_params_json(
                serde_json::from_str(&text).map_err(|e| ExchangeError::new(e.to_string()))?,
            );
            if !js_truthy(Some(&json)) {
                return Ok(());
            }
            if json.get("id").and_then(Value::as_f64) == Some(-1.0) {
                return self.socket.handle_pong_event(5_000);
            }
            if json.get("id").is_some() && json.get("result") == Some(&Value::Null) {
                return Ok(());
            }
            if js_truthy(json.get("error")) {
                return Ok(());
            }
            if js_truthy(json.get("stream")) && js_truthy(json.get("data")) {
                let stream = json["stream"]
                    .as_str()
                    .ok_or_else(|| ExchangeError::new("stream.split is not a function"))?;
                let data = &json["data"];
                match stream {
                    "!markPrice@arr@1s" | "!markPrice@arr" => {
                        if let Some(items) = data.as_array() {
                            for item in items {
                                self.handle_mark_price_update(item)?;
                            }
                        }
                    }
                    "!ticker@arr" => {
                        if let Some(items) = data.as_array() {
                            for item in items {
                                self.handle_ticker_update(item)?;
                            }
                        }
                    }
                    _ => self.stream_data(stream, data)?,
                }
            } else if let Some(items) = json.as_array() {
                for item in items {
                    self.event_data(item)?;
                }
            } else if js_truthy(json.get("e")) {
                self.event_data(&json)?;
            }
            Ok::<(), ExchangeError>(())
        })();
        Ok(())
    }
    fn dispatch(&self, key: &str, data: &Value) -> Result<(), ExchangeError> {
        self.socket.record_message_handler_stats(key)?;
        let callbacks = self
            .socket
            .message_handlers
            .borrow()
            .get(key)
            .cloned()
            .ok_or_else(|| {
                ExchangeError::new("messageHandlers[eventType].forEach is not a function")
            })?;
        let length = callbacks.borrow().len();
        for index in 0..length {
            let callback = callbacks.borrow().get(index).cloned();
            if let Some(callback) = callback {
                callback(data)?;
            }
        }
        Ok(())
    }
    fn event_data(&self, data: &Value) -> Result<(), ExchangeError> {
        let keys: Vec<_> = self
            .socket
            .message_handlers
            .borrow()
            .keys()
            .cloned()
            .collect();
        let keys: Vec<_> = keys.iter().map(|key| (key.as_str(), ())).collect();
        match ws_helpers::public_event_data_dispatch(data, &keys).map_err(coerce)? {
            PublicEventDataDispatch::Ignore => Ok(()),
            PublicEventDataDispatch::Handlers { event_type, data } => {
                self.dispatch(&event_type, &data)
            }
            PublicEventDataDispatch::InheritedHandlerFailure { event_type, .. } => {
                self.dispatch(&event_type, data)
            }
        }
    }
    fn stream_data(&self, stream: &str, data: &Value) -> Result<(), ExchangeError> {
        let (key, assigned, depth) =
            match ws_helpers::public_stream_dispatch(stream, js_truthy(data.get("s")))
                .map_err(ExchangeError::new)?
            {
                PublicStreamDispatch::Ignore => return Ok(()),
                PublicStreamDispatch::DepthSnapshot { assigned_symbol } => {
                    ("", assigned_symbol, true)
                }
                PublicStreamDispatch::Handler {
                    key,
                    assigned_symbol,
                } => (key, assigned_symbol, false),
            };
        if !depth && !self.socket.message_handlers.borrow().contains_key(key) {
            return Ok(());
        }
        let mut data = data.clone();
        if let Some(symbol) = assigned {
            data.as_object_mut()
                .ok_or_else(|| ExchangeError::new("Cannot assign symbol on primitive"))?
                .insert("s".into(), Value::String(symbol));
        }
        if depth {
            self.handle_depth_snapshot(&data)
        } else {
            self.dispatch(key, &data)
        }
    }
    fn patch_ticker(
        &self,
        symbol: &str,
        patch: ModelPatch,
    ) -> Result<Option<Ticker>, ExchangeError> {
        let store = self.parent.store();
        let old = store.tickers_by_symbol().get(symbol).cloned();
        let Some(old) = old else {
            return Ok(None);
        };
        // Retain the updated source referent across synchronous Store observers.
        // Native events own this snapshot: later subscriber mutations of the
        // same source object are an explicit identity boundary.
        let mut retained = old.clone();
        for (field, value) in &patch.0 {
            let number = match value {
                safe_cex_store::model_patch::PatchValue::Number(n) => *n,
                safe_cex_store::model_patch::PatchValue::Json(Value::Number(n)) => {
                    safe_cex_num::number_from_string(&n.to_string())
                }
                safe_cex_store::model_patch::PatchValue::Json(value) => {
                    self.parent.log(
                        &format!(
                            "AsterDEX ticker {symbol} typed field {field} requires a number; received {value}"
                        ),
                        safe_cex_exchange::runtime::LogSeverity::Error,
                    )?;
                    return Ok(None);
                }
                _ => return Err(ExchangeError::new("Non-number public ticker patch")),
            };
            match field.as_str() {
                "bid" => retained.bid = number,
                "ask" => retained.ask = number,
                "last" => retained.last = number,
                "mark" => retained.mark = number,
                "index" => retained.index = Some(number),
                "percentage" => retained.percentage = number,
                "volume" => retained.volume = number,
                "quoteVolume" => retained.quote_volume = number,
                "fundingRate" => retained.funding_rate = number,
                "fundingTime" => retained.funding_time = number,
                _ => unreachable!("Only public-handler fields enter this patch"),
            }
        }
        store
            .update_ticker(&old.id, &patch)
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        Ok(Some(retained))
    }

    pub fn handle_ticker_update(&self, data: &Value) -> Result<(), ExchangeError> {
        let store = self.parent.store();
        let tickers = store.tickers().to_vec();
        let entries: Vec<_> = tickers.iter().map(|t| (t.symbol.as_str(), t)).collect();
        let Some(update) = ws_helpers::public_ticker_update(data, &entries).map_err(coerce)? else {
            return Ok(());
        };
        let mut patch = ModelPatch::default();
        for (key, value) in [
            ("last", update.patch.last),
            ("percentage", update.patch.percentage),
            ("volume", update.patch.volume),
            ("quoteVolume", update.patch.quote_volume),
        ] {
            if let Some(v) = value {
                patch.set_number(key, v);
            }
        }
        if let Some(ticker) = self.patch_ticker(&string(data.get("s"))?, patch)? {
            self.parent.emit_ticker("ticker", &update.symbol, &ticker)?;
        }
        Ok(())
    }
    pub fn handle_mark_price_update(&self, data: &Value) -> Result<(), ExchangeError> {
        let store = self.parent.store();
        let tickers = store.tickers().to_vec();
        let entries: Vec<_> = tickers.iter().map(|t| (t.symbol.as_str(), t)).collect();
        let Some(update) = ws_helpers::public_mark_price_update(data, &entries).map_err(coerce)?
        else {
            return Ok(());
        };
        let mut patch = ModelPatch::default();
        for (key, value) in [
            ("mark", update.patch.mark),
            ("index", update.patch.index),
            ("fundingRate", update.patch.funding_rate),
        ] {
            if let Some(v) = value {
                patch.set_number(key, v);
            }
        }
        if let Some(value) = update.patch.funding_time {
            patch.set_json("fundingTime", value);
        }
        if let Some(ticker) = self.patch_ticker(&string(data.get("s"))?, patch)? {
            self.parent
                .emit_ticker("markPrice", &update.symbol, &ticker)?;
        }
        Ok(())
    }
    pub fn handle_trade_update(&self, data: &Value) -> Result<(), ExchangeError> {
        if data.is_null() {
            return Err(coerce(safe_cex_num::JsCoercionError {
                name: "TypeError",
                message: "Cannot read properties of null (reading 's')",
            }));
        }
        let symbol = data.get("s").and_then(Value::as_str).ok_or_else(|| {
            ExchangeError::new("Non-string trade symbol cannot enter native trade event")
        })?;
        let price = safe_cex_num::parse_float(&string(data.get("p"))?);
        let qty = safe_cex_num::parse_float(&string(data.get("q"))?);
        let timestamp = data.get("T").and_then(Value::as_f64).ok_or_else(|| {
            ExchangeError::new("Non-number trade timestamp cannot enter native trade event")
        })?;
        let event = PublicStreamTradeEvent {
            symbol: symbol.into(),
            price,
            qty,
            timestamp,
            side: if js_truthy(data.get("m")) {
                "SELL"
            } else {
                "BUY"
            }
            .into(),
        };
        let mut patch = ModelPatch::default();
        patch.set_number("last", price);
        self.patch_ticker(symbol, patch)?;
        self.parent.emit_trade(&event)
    }

    pub fn handle_depth_snapshot(&self, data: &Value) -> Result<(), ExchangeError> {
        if data.is_null() {
            return Err(coerce(safe_cex_num::JsCoercionError {
                name: "TypeError",
                message: "Cannot read properties of null (reading 'b')",
            }));
        }
        let Some(book) = ws_helpers::public_depth_snapshot(data) else {
            return Ok(());
        };
        let symbol = string(data.get("s"))?;
        let mut patch = ModelPatch::default();
        patch.set_number("bid", book.bids[0].price);
        patch.set_number("ask", book.asks[0].price);
        self.patch_ticker(&symbol, patch)?;
        let lower = data
            .get("s")
            .and_then(Value::as_str)
            .ok_or_else(|| ExchangeError::new("data.s.toLowerCase is not a function"))?
            .to_lowercase();
        self.subscriptions
            .deliver_book(&format!("{lower}@depth20@100ms"), &book)
    }
    pub fn handle_kline_update(&self, data: &Value) -> Result<(), ExchangeError> {
        let keys = self.subscriptions.state().klines;
        let keys: Vec<_> = keys.iter().map(String::as_str).collect();
        if let Some(update) = ws_helpers::public_kline_update(data, &keys).map_err(coerce)? {
            self.subscriptions
                .deliver_kline(&update.stream, update.candle)?;
        }
        Ok(())
    }
    pub fn dispose(&self) {
        self.subscriptions.dispose();
    }
}

/// These delegates share the exchange's replaceable public-websocket slot. A
/// cleanup consults the current slot, as the source closure does at call time.
#[derive(Default)]
pub struct PublicDataListeners {
    pub websocket: RefCell<Option<Rc<PublicStream>>>,
}
impl PublicDataListeners {
    // ts: AsterDEXExchange.listenOHLCV
    pub fn listen_ohlcv(
        self: &Rc<Self>,
        symbol: &str,
        interval: AnyTimeframe,
        callback: CandleCallback,
    ) -> Result<BookCleanup, ExchangeError> {
        let stream = format!(
            "{}@kline_{}",
            symbol.to_lowercase(),
            crate::exchange_helpers::map_timeframe(interval)
        );
        let socket = self.websocket.borrow().clone();
        if let Some(socket) = socket {
            socket.subscriptions.subscribe_to_kline(&stream, callback)?;
        }
        let owner = self.clone();
        Ok(Rc::new(move || {
            let socket = owner.websocket.borrow().clone();
            if let Some(socket) = socket {
                socket.subscriptions.unsubscribe_from_kline(&stream)?;
            }
            Ok(())
        }))
    }
    // ts: AsterDEXExchange.listenOrderBook
    pub fn listen_order_book(
        &self,
        symbol: &str,
        callback: BookCallback,
    ) -> Result<BookCleanup, ExchangeError> {
        let socket = self.websocket.borrow().clone().ok_or_else(|| {
            ExchangeError::new("Cannot read properties of undefined (reading 'listenOrderBook')")
        })?;
        socket.subscriptions.listen_order_book(symbol, callback)
    }
}
