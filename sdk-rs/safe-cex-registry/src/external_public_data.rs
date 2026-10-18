use crate::{
    in_process_public_data::{CandleObserver, OrderbookObserver},
    public_data::{
        EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS, EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS,
        get_candle_subscription_key, parse_candle_subscription_key,
    },
    public_data_host::{
        MarketBatch, PublicDataCleanup, PublicDataParent, PublicDataUpdate, TickerBatch,
    },
};
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_types::{
    AnyTimeframe, Candle, OHLCVOptions, OrderBook,
    gateway::{GatewayChannel, GatewayMessage, GatewayMessageType},
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    num::NonZeroU64,
    rc::Rc,
};

pub enum GatewayEvent {
    Open,
    Message(String),
    Close,
    Error(Option<String>),
}
pub type GatewayListener = Rc<dyn Fn(GatewayEvent) -> Result<(), ExchangeError>>;
pub type GatewayTimerCallback = Rc<dyn Fn() -> Result<(), ExchangeError>>;
/// Socket creation binds the source ENABLE_RECONNECTING_WEBSOCKET/tuning globals.
pub trait GatewaySocket {
    fn ready_state(&self) -> u16;
    fn add_listener(
        &self,
        event: &'static str,
        listener: GatewayListener,
    ) -> Result<(), ExchangeError>;
    fn send(&self, text: String) -> Result<(), ExchangeError>;
    fn close(&self) -> Result<(), ExchangeError>;
}
pub trait GatewayHost {
    fn create_socket(&self, url: &str) -> Result<Rc<dyn GatewaySocket>, ExchangeError>;
    /// Imported WS_MIN_RECONNECTION_DELAY, fixed at host module initialization.
    fn reconnect_delay_ms(&self) -> f64;
    /// Must enqueue callbacks, never invoke them inline (setTimeout contract).
    fn set_timeout(
        &self,
        delay_ms: f64,
        callback: GatewayTimerCallback,
    ) -> Result<NonZeroU64, ExchangeError>;
    fn clear_timeout(&self, timer: NonZeroU64) -> Result<(), ExchangeError>;
}
#[derive(Clone)]
enum Observer {
    Orderbook(OrderbookObserver),
    Candle(CandleObserver),
}
impl Observer {
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Orderbook(a), Self::Orderbook(b)) => Rc::ptr_eq(a, b),
            (Self::Candle(a), Self::Candle(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}
type ListenerSet = Rc<RefCell<Vec<Option<Observer>>>>;
type ListenerMap = RefCell<Vec<Option<(String, ListenerSet)>>>;
fn lookup(map: &ListenerMap, key: &str) -> Option<ListenerSet> {
    map.borrow()
        .iter()
        .flatten()
        .find(|(name, _)| name == key)
        .map(|(_, set)| set.clone())
}
fn remove(map: &ListenerMap, key: &str) {
    for entry in map.borrow_mut().iter_mut() {
        if entry.as_ref().is_some_and(|(name, _)| name == key) {
            *entry = None;
            break;
        }
    }
}
fn has_listener(map: &ListenerMap, key: &str) -> bool {
    lookup(map, key).is_some_and(|set| set.borrow().iter().any(Option::is_some))
}
fn timeframe(interval: &AnyTimeframe) -> String {
    serde_json::to_value(interval)
        .unwrap()
        .as_str()
        .unwrap()
        .into()
}
fn message(
    kind: GatewayMessageType,
    exchange: &str,
    channel: Option<GatewayChannel>,
    symbol: Option<String>,
    interval: Option<AnyTimeframe>,
) -> GatewayMessage {
    GatewayMessage {
        message_type: kind,
        exchange_id: exchange.into(),
        channel,
        symbol,
        interval,
        data: None,
        error: None,
        request_id: None,
    }
}
pub struct ExternalPublicDataProvider {
    parent: Rc<dyn PublicDataParent>,
    host: Rc<dyn GatewayHost>,
    url: String,
    exchange_id: String,
    auth_token: Option<String>,
    ws: RefCell<Option<Rc<dyn GatewaySocket>>>,
    reconnecting: Cell<bool>,
    initialized: Cell<bool>,
    last_received: Cell<f64>,
    last_ping_sent: Cell<f64>,
    heartbeat_timer: Cell<Option<NonZeroU64>>,
    reconnect_count: Cell<u64>,
    is_disposed: Cell<bool>,
    orderbook_callbacks: ListenerMap,
    candle_callbacks: ListenerMap,
}
impl ExternalPublicDataProvider {
    pub fn new(
        parent: Rc<dyn PublicDataParent>,
        host: Rc<dyn GatewayHost>,
        url: String,
        exchange_id: String,
        auth_token: Option<String>,
    ) -> Rc<Self> {
        Rc::new(Self {
            parent,
            host,
            url,
            exchange_id,
            auth_token,
            ws: RefCell::new(None),
            reconnecting: Cell::new(false),
            initialized: Cell::new(false),
            last_received: Cell::new(0.0),
            last_ping_sent: Cell::new(0.0),
            heartbeat_timer: Cell::new(None),
            reconnect_count: Cell::new(0),
            is_disposed: Cell::new(false),
            orderbook_callbacks: RefCell::new(vec![]),
            candle_callbacks: RefCell::new(vec![]),
        })
    }
    pub fn connect(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.is_disposed.get() {
            return Ok(());
        }
        self.log("Connecting to public data gateway...", LogSeverity::Info)?;
        let socket = self.host.create_socket(&self.url)?;
        *self.ws.borrow_mut() = Some(socket);
        for event in ["open", "message", "close", "error"] {
            let weak = Rc::downgrade(self);
            let listener: GatewayListener = Rc::new(move |event| {
                let Some(provider) = weak.upgrade() else {
                    return Ok(());
                };
                match event {
                    GatewayEvent::Open => provider.on_open(),
                    GatewayEvent::Message(data) => provider.on_message(&data),
                    GatewayEvent::Close => provider.on_close(),
                    GatewayEvent::Error(message) => provider.on_error(message.as_deref()),
                }
            });
            let current = self.ws.borrow().clone().expect("socket was assigned");
            current.add_listener(event, listener)?;
        }
        Ok(())
    }
    fn on_open(self: &Rc<Self>) -> Result<(), ExchangeError> {
        self.log("Connected to public data gateway", LogSeverity::Info)?;
        self.initialized.set(true);
        self.last_received.set(self.parent.now_ms());
        self.last_ping_sent.set(0.0);
        self.schedule_heartbeat_check()?;
        if let Some(token) = self.auth_token.as_ref().filter(|token| !token.is_empty()) {
            let mut auth = message(
                GatewayMessageType::Subscribe,
                &self.exchange_id,
                Some(GatewayChannel::Auth),
                None,
                None,
            );
            auth.data = Some(serde_json::json!({"token":token}));
            self.send(auth)?;
        }
        self.send(message(
            GatewayMessageType::Subscribe,
            &self.exchange_id,
            Some(GatewayChannel::Tickers),
            None,
            None,
        ))?;
        self.send(message(
            GatewayMessageType::Subscribe,
            &self.exchange_id,
            Some(GatewayChannel::Market),
            None,
            None,
        ))?;
        self.resubscribe_all()
    }
    fn on_message(&self, text: &str) -> Result<(), ExchangeError> {
        if self.is_disposed.get() {
            return Ok(());
        }
        self.last_received.set(self.parent.now_ms());
        self.last_ping_sent.set(0.0);
        self.reconnect_count.set(0);
        let Ok(data) =
            serde_json::from_str::<safe_cex_types::retained_json::RetainedJsonInput>(text)
                .and_then(|value| value.into_value())
        else {
            return Ok(());
        };
        if data.get("exchangeId").and_then(Value::as_str) != Some(self.exchange_id.as_str()) {
            return Ok(());
        }
        match data.get("type").and_then(Value::as_str) {
            Some("data") => self.handle_data_message(&data)?,
            Some(kind @ ("subscribed" | "unsubscribed")) => {
                let channel = data
                    .get("channel")
                    .and_then(Value::as_str)
                    .unwrap_or("undefined");
                let symbol = data
                    .get("symbol")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty());
                let label = if kind == "subscribed" {
                    "Subscribed to"
                } else {
                    "Unsubscribed from"
                };
                self.log(
                    &format!(
                        "{label} {channel}{}",
                        symbol.map_or(String::new(), |symbol| format!(":{symbol}"))
                    ),
                    LogSeverity::Info,
                )?;
            }
            Some("error") => self.log(
                &format!(
                    "Gateway error: {}",
                    data.get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("undefined")
                ),
                LogSeverity::Error,
            )?,
            _ => {}
        }
        Ok(())
    }
    fn handle_data_message(&self, message: &Value) -> Result<(), ExchangeError> {
        let payload = message.get("data").cloned().unwrap_or(Value::Null);
        let decode_error = |error: serde_json::Error| ExchangeError::new(error.to_string());
        match message.get("channel").and_then(Value::as_str) {
            Some("tickers") => self.handle_tickers(Rc::new(RefCell::new(
                serde_json::from_value(payload).map_err(decode_error)?,
            )))?,
            Some("ticker") => {
                self.handle_ticker(serde_json::from_value(payload).map_err(decode_error)?)?
            }
            Some("market") => self.handle_markets(Rc::new(RefCell::new(
                serde_json::from_value(payload).map_err(decode_error)?,
            )))?,
            Some("orderbook") => {
                if let Some(symbol) = message
                    .get("symbol")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    if has_listener(&self.orderbook_callbacks, symbol) {
                        self.handle_orderbook(
                            symbol,
                            &serde_json::from_value(payload).map_err(decode_error)?,
                        )?;
                    }
                }
            }
            Some("candle") => {
                if let (Some(symbol), Some(interval)) = (
                    message
                        .get("symbol")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty()),
                    message
                        .get("interval")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty()),
                ) {
                    if has_listener(
                        &self.candle_callbacks,
                        &get_candle_subscription_key(symbol, interval),
                    ) {
                        self.handle_candle(
                            symbol,
                            interval,
                            &serde_json::from_value(payload).map_err(decode_error)?,
                        )?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn handle_tickers(&self, tickers: TickerBatch) -> Result<(), ExchangeError> {
        let mut loaded = self.parent.loaded()?;
        loaded.tickers = true;
        self.parent.update(PublicDataUpdate {
            tickers: Some(tickers),
            markets: None,
            loaded: Some(loaded),
        })
    }
    fn handle_ticker(&self, ticker: safe_cex_types::Ticker) -> Result<(), ExchangeError> {
        let mut tickers = self.parent.tickers()?.borrow().clone();
        if let Some(index) = tickers
            .iter()
            .position(|value| value.symbol == ticker.symbol)
        {
            tickers[index] = ticker;
        } else {
            tickers.push(ticker);
        }
        self.parent.update(PublicDataUpdate {
            tickers: Some(Rc::new(RefCell::new(tickers))),
            markets: None,
            loaded: None,
        })
    }
    fn handle_orderbook(&self, symbol: &str, value: &OrderBook) -> Result<(), ExchangeError> {
        if let Some(set) = lookup(&self.orderbook_callbacks, symbol) {
            let mut index = 0;
            loop {
                let next = {
                    let set = set.borrow();
                    if index >= set.len() {
                        break;
                    }
                    set[index].clone()
                };
                index += 1;
                if let Some(Observer::Orderbook(callback)) = next {
                    callback(value)?;
                }
            }
        }
        Ok(())
    }
    fn handle_candle(
        &self,
        symbol: &str,
        interval: &str,
        value: &Candle,
    ) -> Result<(), ExchangeError> {
        if let Some(set) = lookup(
            &self.candle_callbacks,
            &get_candle_subscription_key(symbol, interval),
        ) {
            let mut index = 0;
            loop {
                let next = {
                    let set = set.borrow();
                    if index >= set.len() {
                        break;
                    }
                    set[index].clone()
                };
                index += 1;
                if let Some(Observer::Candle(callback)) = next {
                    callback(value)?;
                }
            }
        }
        Ok(())
    }
    fn handle_markets(&self, markets: MarketBatch) -> Result<(), ExchangeError> {
        let markets = self.parent.adapt_markets(markets)?;
        let mut loaded = self.parent.loaded()?;
        loaded.markets = true;
        self.parent.update(PublicDataUpdate {
            tickers: None,
            markets: Some(markets),
            loaded: Some(loaded),
        })
    }
    fn on_close(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.is_disposed.get() {
            return Ok(());
        }
        self.stop_heartbeat_check()?;
        self.last_received.set(0.0);
        self.last_ping_sent.set(0.0);
        self.log(
            "Disconnected from public data gateway",
            LogSeverity::Warning,
        )?;
        if self.reconnecting.get() {
            return Ok(());
        }
        self.reconnecting.set(true);
        self.reconnect_count.set(self.reconnect_count.get() + 1);
        if self.reconnect_count.get() > 100 {
            self.log("Reconnect count exceeded, giving up", LogSeverity::Error)?;
            return Ok(());
        }
        let weak = Rc::downgrade(self);
        self.host.set_timeout(
            self.host.reconnect_delay_ms(),
            Rc::new(move || {
                let Some(provider) = weak.upgrade() else {
                    return Ok(());
                };
                if provider.is_disposed.get() {
                    return Ok(());
                }
                provider.reconnecting.set(false);
                provider.connect()
            }),
        )?;
        Ok(())
    }
    fn on_error(&self, message: Option<&str>) -> Result<(), ExchangeError> {
        self.log(
            &format!(
                "Gateway WebSocket error: {}",
                message
                    .filter(|value| !value.is_empty())
                    .unwrap_or("Unknown error")
            ),
            LogSeverity::Error,
        )
    }
    pub fn subscribe_orderbook(
        self: &Rc<Self>,
        symbol: String,
        callback: OrderbookObserver,
    ) -> Result<PublicDataCleanup, ExchangeError> {
        if lookup(&self.orderbook_callbacks, &symbol).is_none() {
            self.orderbook_callbacks
                .borrow_mut()
                .push(Some((symbol.clone(), Rc::default())));
            self.send(message(
                GatewayMessageType::Subscribe,
                &self.exchange_id,
                Some(GatewayChannel::Orderbook),
                Some(symbol.clone()),
                None,
            ))?;
        }
        let set = lookup(&self.orderbook_callbacks, &symbol)
            .ok_or_else(|| ExchangeError::new("orderbook callback set removed during send"))?;
        let observer = Observer::Orderbook(callback);
        if !set
            .borrow()
            .iter()
            .flatten()
            .any(|entry| entry.same(&observer))
        {
            set.borrow_mut().push(Some(observer.clone()));
        }
        let weak = Rc::downgrade(self);
        Ok(Rc::new(move || {
            let Some(provider) = weak.upgrade() else {
                return Ok(());
            };
            if let Some(set) = lookup(&provider.orderbook_callbacks, &symbol) {
                for entry in set.borrow_mut().iter_mut() {
                    if entry.as_ref().is_some_and(|entry| entry.same(&observer)) {
                        *entry = None;
                        break;
                    }
                }
                if set.borrow().iter().all(Option::is_none) {
                    remove(&provider.orderbook_callbacks, &symbol);
                    provider.send(message(
                        GatewayMessageType::Unsubscribe,
                        &provider.exchange_id,
                        Some(GatewayChannel::Orderbook),
                        Some(symbol.clone()),
                        None,
                    ))?;
                }
            }
            Ok(())
        }))
    }
    pub fn subscribe_candle(
        self: &Rc<Self>,
        options: Rc<RefCell<OHLCVOptions>>,
        callback: CandleObserver,
    ) -> Result<PublicDataCleanup, ExchangeError> {
        let key = {
            let opts = options.borrow();
            get_candle_subscription_key(&opts.symbol, &timeframe(&opts.interval))
        };
        if lookup(&self.candle_callbacks, &key).is_none() {
            self.candle_callbacks
                .borrow_mut()
                .push(Some((key.clone(), Rc::default())));
            let request = {
                let opts = options.borrow();
                message(
                    GatewayMessageType::Subscribe,
                    &self.exchange_id,
                    Some(GatewayChannel::Candle),
                    Some(opts.symbol.clone()),
                    Some(opts.interval.clone()),
                )
            };
            self.send(request)?;
        }
        let set = lookup(&self.candle_callbacks, &key)
            .ok_or_else(|| ExchangeError::new("candle callback set removed during send"))?;
        let observer = Observer::Candle(callback);
        if !set
            .borrow()
            .iter()
            .flatten()
            .any(|entry| entry.same(&observer))
        {
            set.borrow_mut().push(Some(observer.clone()));
        }
        let weak = Rc::downgrade(self);
        Ok(Rc::new(move || {
            let Some(provider) = weak.upgrade() else {
                return Ok(());
            };
            if let Some(set) = lookup(&provider.candle_callbacks, &key) {
                for entry in set.borrow_mut().iter_mut() {
                    if entry.as_ref().is_some_and(|entry| entry.same(&observer)) {
                        *entry = None;
                        break;
                    }
                }
                if set.borrow().iter().all(Option::is_none) {
                    remove(&provider.candle_callbacks, &key);
                    let request = {
                        let opts = options.borrow();
                        message(
                            GatewayMessageType::Unsubscribe,
                            &provider.exchange_id,
                            Some(GatewayChannel::Candle),
                            Some(opts.symbol.clone()),
                            Some(opts.interval.clone()),
                        )
                    };
                    provider.send(request)?;
                }
            }
            Ok(())
        }))
    }
    fn resubscribe_all(&self) -> Result<(), ExchangeError> {
        for (map, candle) in [
            (&self.orderbook_callbacks, false),
            (&self.candle_callbacks, true),
        ] {
            let mut index = 0;
            loop {
                let key = {
                    let entries = map.borrow();
                    if index >= entries.len() {
                        break;
                    }
                    entries[index].as_ref().map(|(key, _)| key.clone())
                };
                index += 1;
                if let Some(key) = key {
                    let (symbol, interval) = if candle {
                        let (symbol, interval) = parse_candle_subscription_key(&key);
                        (
                            symbol,
                            Some(
                                serde_json::from_value(Value::String(interval))
                                    .map_err(|error| ExchangeError::new(error.to_string()))?,
                            ),
                        )
                    } else {
                        (key, None)
                    };
                    self.send(message(
                        GatewayMessageType::Subscribe,
                        &self.exchange_id,
                        Some(if candle {
                            GatewayChannel::Candle
                        } else {
                            GatewayChannel::Orderbook
                        }),
                        Some(symbol),
                        interval,
                    ))?;
                }
            }
        }
        Ok(())
    }
    fn send(&self, message: GatewayMessage) -> Result<(), ExchangeError> {
        let socket = self.ws.borrow().clone();
        if !socket
            .as_ref()
            .is_some_and(|socket| socket.ready_state() == 1)
        {
            return self.log("Cannot send - gateway not connected", LogSeverity::Warning);
        }
        socket.unwrap().send(
            serde_json::to_string(&message)
                .map_err(|error| ExchangeError::new(error.to_string()))?,
        )
    }
    fn schedule_heartbeat_check(self: &Rc<Self>) -> Result<(), ExchangeError> {
        self.stop_heartbeat_check()?;
        self.arm_heartbeat()
    }
    fn arm_heartbeat(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let weak = Rc::downgrade(self);
        let timer = self.host.set_timeout(
            1000.0,
            Rc::new(move || {
                let Some(provider) = weak.upgrade() else {
                    return Ok(());
                };
                if provider.is_disposed.get() {
                    return Ok(());
                }
                let now = provider.parent.now_ms();
                let elapsed = now - provider.last_received.get();
                if elapsed > EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS as f64 {
                    provider.log(
                        &format!(
                            "Gateway stale for {}s, reconnecting",
                            (elapsed / 1000.0).round()
                        ),
                        LogSeverity::Warning,
                    )?;
                    let socket = provider.ws.borrow().clone();
                    if let Some(socket) = socket {
                        socket.close()?;
                    }
                    return Ok(());
                }
                if elapsed > EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS as f64
                    && provider.last_ping_sent.get() == 0.0
                {
                    provider.last_ping_sent.set(now);
                    provider.send(message(
                        GatewayMessageType::Ping,
                        &provider.exchange_id,
                        None,
                        None,
                        None,
                    ))?;
                }
                provider.arm_heartbeat()
            }),
        )?;
        self.heartbeat_timer.set(Some(timer));
        Ok(())
    }
    fn stop_heartbeat_check(&self) -> Result<(), ExchangeError> {
        if let Some(timer) = self.heartbeat_timer.get() {
            self.host.clear_timeout(timer)?;
            self.heartbeat_timer.set(None);
        }
        Ok(())
    }
    fn log(&self, message: &str, severity: LogSeverity) -> Result<(), ExchangeError> {
        self.parent.emit_log(
            format!("[{}:Gateway] {message}", self.parent.name()),
            severity,
        )
    }
    pub fn is_connected(&self) -> bool {
        self.ws
            .borrow()
            .as_ref()
            .is_some_and(|socket| socket.ready_state() == 1)
    }
    pub fn is_initialized(&self) -> bool {
        self.initialized.get()
    }
    pub fn is_healthy(&self) -> bool {
        self.is_connected()
            && self.last_received.get() != 0.0
            && self.parent.now_ms() - self.last_received.get()
                < EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS as f64
    }
    pub fn last_message_timestamp(&self) -> Option<f64> {
        (self.last_received.get() > 0.0).then(|| self.last_received.get())
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        self.is_disposed.set(true);
        self.stop_heartbeat_check()?;
        // Retain iterator positions, as Map.clear() does for already-live iterators.
        for entry in self.orderbook_callbacks.borrow_mut().iter_mut() {
            *entry = None;
        }
        for entry in self.candle_callbacks.borrow_mut().iter_mut() {
            *entry = None;
        }
        let socket = self.ws.borrow().clone();
        if let Some(socket) = socket {
            socket.close()?;
        }
        *self.ws.borrow_mut() = None;
        Ok(())
    }
}
