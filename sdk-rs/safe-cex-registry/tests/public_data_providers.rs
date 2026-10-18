use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_registry::{external_public_data::*, in_process_public_data::*, public_data_host::*};
use safe_cex_types::{Candle, OHLCVOptions, OrderBook, StoreDataLoaded, Ticker};
use std::{
    cell::{Cell, RefCell},
    num::NonZeroU64,
    rc::Rc,
};

fn fixture<T: serde::de::DeserializeOwned>(key: &str) -> T {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/public-data.json")).unwrap();
    serde_json::from_value(data[key].clone()).unwrap()
}
fn loaded() -> StoreDataLoaded {
    StoreDataLoaded {
        balance: false,
        orders: false,
        markets: false,
        tickers: false,
        positions: false,
    }
}
struct Parent {
    now: Cell<f64>,
    data: RefCell<PublicDataSnapshot>,
    updates: RefCell<Vec<(bool, bool, bool)>>,
    logs: RefCell<Vec<(String, LogSeverity)>>,
    fail_update: Cell<bool>,
    after_update: RefCell<Option<Rc<dyn Fn()>>>,
    fail_log: Cell<bool>,
}
impl Parent {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            now: Cell::new(1000.0),
            data: RefCell::new(PublicDataSnapshot {
                tickers: Rc::default(),
                markets: Rc::default(),
                loaded: Rc::new(RefCell::new(loaded())),
            }),
            updates: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            fail_update: Cell::new(false),
            after_update: RefCell::new(None),
            fail_log: Cell::new(false),
        })
    }
}
impl PublicDataParent for Parent {
    fn name(&self) -> String {
        "bybit".into()
    }
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn loaded(&self) -> StoreDataLoaded {
        self.data.borrow().loaded.borrow().clone()
    }
    fn tickers(&self) -> TickerBatch {
        self.data.borrow().tickers.clone()
    }
    fn adapt_markets(&self, markets: MarketBatch) -> Result<MarketBatch, ExchangeError> {
        let mut output = markets.borrow().clone();
        for market in &mut output {
            market.symbol = format!("ADAPT:{}", market.symbol);
        }
        Ok(Rc::new(RefCell::new(output)))
    }
    fn update(&self, update: PublicDataUpdate) -> Result<(), ExchangeError> {
        if self.fail_update.get() {
            return Err(ExchangeError::new("parent update refusal"));
        }
        self.updates.borrow_mut().push((
            update.tickers.is_some(),
            update.markets.is_some(),
            update.loaded.is_some(),
        ));
        let mut data = self.data.borrow_mut();
        if let Some(tickers) = update.tickers {
            data.tickers = tickers;
        }
        if let Some(markets) = update.markets {
            data.markets = markets;
        }
        if let Some(loaded) = update.loaded {
            *data.loaded.borrow_mut() = loaded;
        }
        drop(data);
        let callback = self.after_update.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
        Ok(())
    }
    fn emit_log(&self, message: String, severity: LogSeverity) -> Result<(), ExchangeError> {
        if self.fail_log.get() {
            return Err(ExchangeError::new("parent log refusal"));
        }
        self.logs.borrow_mut().push((message, severity));
        Ok(())
    }
}
struct Source {
    data: RefCell<PublicDataSnapshot>,
    callbacks: RefCell<Vec<PublicDataObserver>>,
    cleanups: Rc<Cell<u32>>,
    cleanup_error: Rc<Cell<bool>>,
    delegated: RefCell<Vec<String>>,
}
impl Source {
    fn new() -> Rc<Self> {
        let mut flags = loaded();
        flags.tickers = true;
        flags.markets = true;
        Rc::new(Self {
            data: RefCell::new(PublicDataSnapshot {
                tickers: Rc::new(RefCell::new(vec![fixture("ticker")])),
                markets: Rc::new(RefCell::new(vec![fixture("market")])),
                loaded: Rc::new(RefCell::new(flags)),
            }),
            callbacks: RefCell::new(vec![]),
            cleanups: Rc::new(Cell::new(0)),
            cleanup_error: Rc::new(Cell::new(false)),
            delegated: RefCell::new(vec![]),
        })
    }
    fn cleanup(&self) -> PublicDataCleanup {
        let count = self.cleanups.clone();
        let fail = self.cleanup_error.clone();
        Rc::new(move || {
            if fail.get() {
                return Err(ExchangeError::new("cleanup refusal"));
            }
            count.set(count.get() + 1);
            Ok(())
        })
    }
    fn notify(&self, data: PublicDataSnapshot) -> Result<(), ExchangeError> {
        let callbacks = self.callbacks.borrow().clone();
        for callback in callbacks {
            callback(data.clone())?;
        }
        Ok(())
    }
}
impl InProcessSource for Source {
    fn snapshot(&self) -> Result<PublicDataSnapshot, ExchangeError> {
        Ok(self.data.borrow().clone())
    }
    fn subscribe(&self, callback: PublicDataObserver) -> Result<PublicDataCleanup, ExchangeError> {
        self.callbacks.borrow_mut().push(callback);
        Ok(self.cleanup())
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: OrderbookObserver,
    ) -> Result<PublicDataCleanup, ExchangeError> {
        self.delegated.borrow_mut().push(symbol);
        callback(&fixture("orderbook"))?;
        Ok(self.cleanup())
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: CandleObserver,
    ) -> Result<PublicDataCleanup, ExchangeError> {
        assert_eq!(options.from, Some(300.0));
        assert_eq!(options.to, Some(900.0));
        assert_eq!(options.limit, Some(17.0));
        self.delegated.borrow_mut().push(options.symbol);
        callback(&fixture("candle"))?;
        Ok(self.cleanup())
    }
}
fn assert_book(value: &OrderBook) {
    assert_eq!(
        (
            value.bids[0].price,
            value.bids[0].amount,
            value.bids[0].total
        ),
        (91.0, 7.0, 637.0),
        "PUBLIC native decoded bid fields"
    );
    assert_eq!(
        (
            value.asks[0].price,
            value.asks[0].amount,
            value.asks[0].total
        ),
        (93.0, 11.0, 1023.0),
        "PUBLIC native decoded ask fields"
    );
}
fn assert_candle(value: &Candle) {
    assert_eq!(
        (
            value.timestamp,
            value.open,
            value.high,
            value.low,
            value.close,
            value.volume
        ),
        (1700.0, 201.0, 223.0, 197.0, 211.0, 31.0),
        "PUBLIC native decoded candle fields"
    );
}
fn source_gate() {
    // All Rust native assertions in the caller precede this source/wire witness.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public-data.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "PUBLIC actual source assertions: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observation: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(observation["inProcess"]["cleanupCount"], 3);
    assert_eq!(observation["gateway"]["nativeBid"], 163);
    assert_eq!(observation["gateway"]["nativeAsk"], 167);
}
#[test]
fn in_process_native_alias_filter_health_delegation_and_disposal() {
    let parent = Parent::new();
    let source = Source::new();
    let provider = InProcessPublicDataProvider::new(parent.clone(), source.clone());
    assert!(!provider.is_connected());
    assert!(!provider.is_initialized());
    assert!(!provider.is_healthy());
    assert_eq!(provider.last_message_timestamp(), None);
    provider.connect().unwrap();
    assert_eq!(
        *parent.updates.borrow(),
        [(true, false, true), (false, true, true)],
        "PUBLIC native seed order"
    );
    assert!(provider.is_connected());
    assert!(provider.is_initialized());
    assert!(provider.is_healthy());
    assert!(
        Rc::ptr_eq(&parent.tickers(), &source.data.borrow().tickers),
        "PUBLIC native seed array identity"
    );
    let market = parent.data.borrow().markets.borrow()[0].clone();
    assert_eq!(market.symbol, "ADAPT:BTC:PERP");
    assert_eq!(market.base, "BTC");
    assert_eq!(market.quote, "USD");
    assert_eq!(market.precision.amount, 0.01);
    assert_eq!(market.precision.price, 0.1);
    parent.now.set(2000.0);
    source.notify(source.data.borrow().clone()).unwrap();
    assert_eq!(parent.updates.borrow().len(), 3);
    source.data.borrow().tickers.borrow_mut()[0].bid = 151.0;
    parent.now.set(3000.0);
    source.notify(source.data.borrow().clone()).unwrap();
    assert_eq!(
        parent.updates.borrow().len(),
        3,
        "PUBLIC native in-place delta filtered"
    );
    assert_eq!(
        parent.tickers().borrow()[0].bid,
        151.0,
        "PUBLIC native alias delta visible without update"
    );
    assert_eq!(
        provider.last_message_timestamp(),
        Some(3000.0),
        "PUBLIC native liveness before filter"
    );
    let mut second: Ticker = fixture("ticker");
    second.id = "ETH".into();
    second.symbol = "ETH".into();
    source.data.borrow().tickers.borrow_mut().push(second);
    parent.now.set(4000.0);
    source.notify(source.data.borrow().clone()).unwrap();
    assert_eq!(
        parent.updates.borrow().len(),
        3,
        "PUBLIC native same-array length mutation filtered"
    );
    assert_eq!(
        parent.tickers().borrow().len(),
        2,
        "PUBLIC native aliased length visible"
    );
    parent.now.set(4500.0);
    source
        .notify(PublicDataSnapshot {
            tickers: Rc::default(),
            markets: Rc::default(),
            loaded: Rc::new(RefCell::new(loaded())),
        })
        .unwrap();
    assert_eq!(provider.last_message_timestamp(), Some(4000.0));
    let mut replacement = source.data.borrow().clone();
    let copied = replacement.tickers.borrow().clone();
    replacement.tickers = Rc::new(RefCell::new(copied));
    parent.now.set(5000.0);
    source.notify(replacement).unwrap();
    assert_eq!(parent.updates.borrow().len(), 4);
    assert_eq!(
        parent.updates.borrow()[3],
        (true, false, true),
        "PUBLIC native replacement full update"
    );
    parent.now.set(25000.0);
    assert!(
        !provider.is_healthy(),
        "PUBLIC native exact stale threshold"
    );
    parent.now.set(24999.0);
    assert!(provider.is_healthy());
    let unsub = provider
        .subscribe_orderbook(
            "BTC:PERP".into(),
            Rc::new(|value| {
                assert_book(value);
                Ok(())
            }),
        )
        .unwrap();
    unsub().unwrap();
    unsub().unwrap();
    provider
        .subscribe_candle(
            fixture("candleOptions"),
            Rc::new(|value| {
                assert_candle(value);
                Ok(())
            }),
        )
        .unwrap();
    assert_eq!(*source.delegated.borrow(), ["BTC:PERP", "BTC:PERP"]);
    provider.connect().unwrap();
    assert_eq!(
        source.callbacks.borrow().len(),
        2,
        "PUBLIC native repeated connect retains prior subscription"
    );
    provider.dispose().unwrap();
    provider.dispose().unwrap();
    assert_eq!(source.cleanups.get(), 3);
    assert!(!provider.is_connected());
    assert!(provider.is_initialized());
    parent.now.set(30000.0);
    source.notify(source.data.borrow().clone()).unwrap();
    assert_eq!(provider.last_message_timestamp(), Some(24999.0));
    source_gate();
}
#[test]
fn in_process_native_failure_points_and_zero_timestamp() {
    let parent = Parent::new();
    let source = Source::new();
    let provider = InProcessPublicDataProvider::new(parent.clone(), source.clone());
    parent.fail_update.set(true);
    assert_eq!(
        provider.connect().unwrap_err().message,
        "parent update refusal"
    );
    assert!(provider.is_initialized());
    assert!(!provider.is_connected());
    assert_eq!(provider.last_message_timestamp(), Some(1000.0));
    assert!(source.callbacks.borrow().is_empty());
    parent.fail_update.set(false);
    parent.now.set(0.0);
    provider.connect().unwrap();
    assert!(provider.is_initialized());
    assert!(!provider.is_healthy());
    assert_eq!(provider.last_message_timestamp(), None);
    source.cleanup_error.set(true);
    assert_eq!(provider.dispose().unwrap_err().message, "cleanup refusal");
    assert!(!provider.is_connected());
    source.cleanup_error.set(false);
    provider.dispose().unwrap();
    assert_eq!(
        source.cleanups.get(),
        1,
        "PUBLIC native cleanup retained after refusal"
    );
}

#[derive(Default)]
struct Socket {
    ready: Cell<u16>,
    listeners: RefCell<Vec<(&'static str, GatewayListener)>>,
    sent: RefCell<Vec<String>>,
    closes: Cell<usize>,
    fail_send: Cell<bool>,
}
impl Socket {
    fn emit(&self, event: GatewayEvent) -> Result<(), ExchangeError> {
        let label = match &event {
            GatewayEvent::Open => "open",
            GatewayEvent::Message(_) => "message",
            GatewayEvent::Close => "close",
            GatewayEvent::Error(_) => "error",
        };
        let listener = self
            .listeners
            .borrow()
            .iter()
            .find(|(name, _)| *name == label)
            .unwrap()
            .1
            .clone();
        listener(event)
    }
    fn open(&self) -> Result<(), ExchangeError> {
        self.ready.set(1);
        self.emit(GatewayEvent::Open)
    }
    fn incoming(&self, text: &str) -> Result<(), ExchangeError> {
        self.emit(GatewayEvent::Message(text.into()))
    }
}
impl GatewaySocket for Socket {
    fn ready_state(&self) -> u16 {
        self.ready.get()
    }
    fn add_listener(
        &self,
        event: &'static str,
        listener: GatewayListener,
    ) -> Result<(), ExchangeError> {
        self.listeners.borrow_mut().push((event, listener));
        Ok(())
    }
    fn send(&self, text: String) -> Result<(), ExchangeError> {
        if self.fail_send.get() {
            return Err(ExchangeError::new("socket send refusal"));
        }
        self.sent.borrow_mut().push(text);
        Ok(())
    }
    fn close(&self) -> Result<(), ExchangeError> {
        self.closes.set(self.closes.get() + 1);
        self.ready.set(3);
        self.emit(GatewayEvent::Close)
    }
}
#[derive(Default)]
struct Host {
    sockets: RefCell<Vec<Rc<Socket>>>,
    timers: RefCell<Vec<(NonZeroU64, GatewayTimerCallback)>>,
    scheduled: RefCell<Vec<f64>>,
    cleared: RefCell<Vec<NonZeroU64>>,
    next: Cell<u64>,
}
impl Host {
    fn fire(&self) -> Result<(), ExchangeError> {
        let (_, callback) = self.timers.borrow_mut().remove(0);
        callback()
    }
    fn current(&self) -> Rc<Socket> {
        self.sockets.borrow().last().unwrap().clone()
    }
}
impl GatewayHost for Host {
    fn create_socket(&self, url: &str) -> Result<Rc<dyn GatewaySocket>, ExchangeError> {
        assert_eq!(url, "wss://fixture");
        let socket = Rc::new(Socket::default());
        self.sockets.borrow_mut().push(socket.clone());
        Ok(socket)
    }
    fn reconnect_delay_ms(&self) -> f64 {
        1500.0
    }
    fn set_timeout(
        &self,
        delay: f64,
        callback: GatewayTimerCallback,
    ) -> Result<NonZeroU64, ExchangeError> {
        self.next.set(self.next.get() + 1);
        let id = NonZeroU64::new(self.next.get()).unwrap();
        self.timers.borrow_mut().push((id, callback));
        self.scheduled.borrow_mut().push(delay);
        Ok(id)
    }
    fn clear_timeout(&self, id: NonZeroU64) -> Result<(), ExchangeError> {
        self.cleared.borrow_mut().push(id);
        self.timers.borrow_mut().retain(|(entry, _)| *entry != id);
        Ok(())
    }
}
fn gateway() -> (Rc<Parent>, Rc<Host>, Rc<ExternalPublicDataProvider>) {
    let parent = Parent::new();
    let host = Rc::new(Host::default());
    let provider = ExternalPublicDataProvider::new(
        parent.clone(),
        host.clone(),
        "wss://fixture".into(),
        "bybit".into(),
        Some("secret".into()),
    );
    (parent, host, provider)
}
fn incoming(socket: &Socket, channel: &str, symbol: Option<&str>, data: serde_json::Value) {
    let mut envelope =
        serde_json::json!({"type":"data","exchangeId":"bybit","channel":channel,"data":data});
    if let Some(symbol) = symbol {
        envelope["symbol"] = symbol.into();
    }
    if channel == "candle" {
        envelope["interval"] = "1m".into();
    }
    socket.incoming(&envelope.to_string()).unwrap();
}
fn payload(key: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(include_str!("fixtures/public-data.json")).unwrap()
        [key]
        .clone()
}
#[test]
fn gateway_native_decode_subscriptions_heartbeat_reconnect_and_late_events() {
    let (parent, host, provider) = gateway();
    assert!(!provider.is_connected());
    assert!(!provider.is_initialized());
    assert!(!provider.is_healthy());
    assert_eq!(provider.last_message_timestamp(), None);
    let seen = Rc::new(RefCell::new(vec![]));
    let books = seen.clone();
    provider
        .subscribe_orderbook(
            "BTC:PERP".into(),
            Rc::new(move |value| {
                assert_book(value);
                books.borrow_mut().push("book");
                Ok(())
            }),
        )
        .unwrap();
    let options = Rc::new(RefCell::new(fixture::<OHLCVOptions>("candleOptions")));
    let candles = seen.clone();
    let unsub = provider
        .subscribe_candle(
            options.clone(),
            Rc::new(move |value| {
                assert_candle(value);
                candles.borrow_mut().push("candle");
                Ok(())
            }),
        )
        .unwrap();
    provider.connect().unwrap();
    let socket = host.current();
    assert_eq!(
        socket
            .listeners
            .borrow()
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>(),
        ["open", "message", "close", "error"]
    );
    socket.open().unwrap();
    assert!(provider.is_connected());
    assert!(provider.is_initialized());
    assert!(provider.is_healthy());
    assert_eq!(provider.last_message_timestamp(), Some(1000.0));
    incoming(
        &socket,
        "tickers",
        None,
        serde_json::Value::Array(vec![payload("ticker")]),
    );
    let mut single = payload("ticker");
    single["bid"] = 163.into();
    single["ask"] = 167.into();
    incoming(&socket, "ticker", None, single);
    {
        let batch = parent.tickers();
        let rows = batch.borrow();
        let ticker = &rows[0];
        assert_eq!(
            (
                ticker.bid,
                ticker.ask,
                ticker.last,
                ticker.mark,
                ticker.index
            ),
            (163.0, 167.0, 107.0, 109.0, Some(113.0)),
            "PUBLIC native decoded ticker prices"
        );
        assert_eq!(
            (
                ticker.percentage,
                ticker.open_interest,
                ticker.funding_rate,
                ticker.funding_time,
                ticker.volume,
                ticker.quote_volume
            ),
            (2.0, 17.0, 0.25, 1500.0, 19.0, 23.0),
            "PUBLIC native decoded ticker auxiliary fields"
        );
    }
    let mut extra = payload("ticker");
    extra["symbol"] = "ETH".into();
    incoming(&socket, "ticker", None, extra);
    assert_eq!(
        parent.tickers().borrow().len(),
        2,
        "PUBLIC native single ticker append"
    );
    assert_eq!(
        parent.updates.borrow()[1],
        (true, false, false),
        "PUBLIC native single ticker leaves loaded unchanged"
    );
    incoming(
        &socket,
        "market",
        None,
        serde_json::Value::Array(vec![payload("market")]),
    );
    assert_eq!(
        parent.data.borrow().markets.borrow()[0].symbol,
        "ADAPT:BTC:PERP"
    );
    assert!(parent.loaded().tickers);
    assert!(parent.loaded().markets);
    incoming(&socket, "orderbook", Some("BTC:PERP"), payload("orderbook"));
    incoming(&socket, "candle", Some("BTC:PERP"), payload("candle"));
    assert_eq!(*seen.borrow(), ["book", "candle"]);
    incoming(
        &socket,
        "orderbook",
        Some("UNSUBSCRIBED"),
        serde_json::Value::Null,
    );
    incoming(
        &socket,
        "candle",
        Some("UNSUBSCRIBED"),
        serde_json::Value::Null,
    );
    for text in [
        r#"{"exchangeId":"bybit","type":"subscribed","channel":"orderbook","symbol":"BTC:PERP"}"#,
        r#"{"exchangeId":"bybit","type":"unsubscribed","channel":"candle"}"#,
        r#"{"exchangeId":"bybit","type":"error","error":"refusal"}"#,
        r#"{"exchangeId":"bybit","type":"pong"}"#,
    ] {
        socket.incoming(text).unwrap();
    }
    let logs = parent.logs.borrow();
    assert_eq!(
        logs[logs.len() - 3].0,
        "[bybit:Gateway] Subscribed to orderbook:BTC:PERP"
    );
    assert_eq!(
        logs[logs.len() - 2].0,
        "[bybit:Gateway] Unsubscribed from candle"
    );
    assert_eq!(
        logs[logs.len() - 1].0,
        "[bybit:Gateway] Gateway error: refusal"
    );
    drop(logs);
    parent.now.set(7000.0);
    host.fire().unwrap();
    assert_eq!(
        socket.sent.borrow().len(),
        5,
        "PUBLIC native ping exact threshold excluded"
    );
    parent.now.set(7001.0);
    host.fire().unwrap();
    assert_eq!(socket.sent.borrow().len(), 6, "PUBLIC native first ping");
    parent.now.set(8000.0);
    host.fire().unwrap();
    assert_eq!(
        socket.sent.borrow().len(),
        6,
        "PUBLIC native no repeated ping"
    );
    parent.now.set(9000.0);
    socket.incoming("bad json").unwrap();
    assert_eq!(provider.last_message_timestamp(), Some(9000.0));
    parent.now.set(10000.0);
    socket.incoming(r#"{"exchangeId":"other"}"#).unwrap();
    assert_eq!(
        provider.last_message_timestamp(),
        Some(10000.0),
        "PUBLIC native foreign traffic refreshes liveness"
    );
    parent.now.set(30000.0);
    assert!(!provider.is_healthy());
    host.fire().unwrap();
    assert_eq!(
        socket.closes.get(),
        0,
        "PUBLIC native heartbeat strict stale threshold"
    );
    parent.now.set(30001.0);
    host.fire().unwrap();
    assert_eq!(socket.closes.get(), 1);
    assert_eq!(provider.last_message_timestamp(), None);
    assert_eq!(
        *host.scheduled.borrow().last().unwrap(),
        1500.0,
        "PUBLIC native imported reconnect delay"
    );
    host.fire().unwrap();
    let replacement = host.current();
    assert_eq!(host.sockets.borrow().len(), 2);
    replacement.open().unwrap();
    assert_eq!(
        replacement.sent.borrow().len(),
        5,
        "PUBLIC native reconnect resubscription count"
    );
    options.borrow_mut().symbol = "CHANGED".into();
    options.borrow_mut().interval = serde_json::from_str("\"5m\"").unwrap();
    unsub().unwrap();
    unsub().unwrap();
    assert_eq!(
        replacement.sent.borrow().len(),
        6,
        "PUBLIC native reusable unsubscribe"
    );
    provider.dispose().unwrap();
    assert!(!provider.is_connected());
    assert!(provider.is_initialized());
    replacement.emit(GatewayEvent::Open).unwrap();
    assert!(
        !provider.is_connected(),
        "PUBLIC native late open no disposed guard"
    );
    assert_eq!(
        host.timers.borrow().len(),
        1,
        "PUBLIC native late open arms a timer"
    );
    host.fire().unwrap();
    assert!(host.timers.borrow().is_empty());
    replacement
        .emit(GatewayEvent::Error(Some("".into())))
        .unwrap();
    assert_eq!(
        parent.logs.borrow().last().unwrap().0,
        "[bybit:Gateway] Gateway WebSocket error: Unknown error"
    );
    // Every native model/lifecycle assertion above precedes outbound wire assertions.
    let wire: Vec<serde_json::Value> = socket
        .sent
        .borrow()
        .iter()
        .map(|text| serde_json::from_str(text).unwrap())
        .collect();
    assert_eq!(wire[0]["channel"], "auth");
    assert_eq!(wire[0]["data"]["token"], "secret");
    assert_eq!(wire[1]["channel"], "tickers");
    assert_eq!(wire[2]["channel"], "market");
    assert_eq!(wire[3]["symbol"], "BTC:PERP");
    assert_eq!(wire[4]["interval"], "1m");
    let wire: Vec<serde_json::Value> = replacement
        .sent
        .borrow()
        .iter()
        .map(|text| serde_json::from_str(text).unwrap())
        .collect();
    assert_eq!(wire[5]["type"], "unsubscribe");
    assert_eq!(wire[5]["symbol"], "CHANGED");
    assert_eq!(wire[5]["interval"], "5m");
    source_gate();
}
#[test]
fn gateway_native_live_set_iteration_duplicate_identity_and_first_failure() {
    let (_, host, provider) = gateway();
    provider.connect().unwrap();
    let socket = host.current();
    socket.open().unwrap();
    let seen = Rc::new(RefCell::new(vec![]));
    let third_seen = seen.clone();
    let third: OrderbookObserver = Rc::new(move |value| {
        assert_book(value);
        third_seen.borrow_mut().push("third");
        Ok(())
    });
    let cleanup: Rc<RefCell<Option<PublicDataCleanup>>> = Rc::default();
    let remove = cleanup.clone();
    let callback_provider = provider.clone();
    let first_seen = seen.clone();
    let third_ref = third.clone();
    let first: OrderbookObserver = Rc::new(move |_| {
        first_seen.borrow_mut().push("first");
        let remove = remove.borrow().as_ref().unwrap().clone();
        remove()?;
        callback_provider.subscribe_orderbook("LIVE".into(), third_ref.clone())?;
        Ok(())
    });
    provider
        .subscribe_orderbook("LIVE".into(), first.clone())
        .unwrap();
    provider.subscribe_orderbook("LIVE".into(), first).unwrap();
    let removed = seen.clone();
    *cleanup.borrow_mut() = Some(
        provider
            .subscribe_orderbook(
                "LIVE".into(),
                Rc::new(move |_| {
                    removed.borrow_mut().push("removed");
                    Ok(())
                }),
            )
            .unwrap(),
    );
    incoming(&socket, "orderbook", Some("LIVE"), payload("orderbook"));
    assert_eq!(
        *seen.borrow(),
        ["first", "third"],
        "PUBLIC native live Set add/remove/identity"
    );
    assert_eq!(
        socket.sent.borrow().len(),
        4,
        "PUBLIC native only first subscriber sends"
    );
    let later = Rc::new(Cell::new(false));
    provider
        .subscribe_orderbook(
            "FAIL".into(),
            Rc::new(|_| Err(ExchangeError::new("first callback refusal"))),
        )
        .unwrap();
    let called = later.clone();
    provider
        .subscribe_orderbook(
            "FAIL".into(),
            Rc::new(move |_| {
                called.set(true);
                Ok(())
            }),
        )
        .unwrap();
    let text=serde_json::json!({"type":"data","exchangeId":"bybit","channel":"orderbook","symbol":"FAIL","data":payload("orderbook")}).to_string();
    assert_eq!(
        socket.incoming(&text).unwrap_err().message,
        "first callback refusal"
    );
    assert!(!later.get(), "PUBLIC native callback failure aborts fanout");
}
#[test]
fn gateway_native_cap_reset_and_failure_before_rearm() {
    let (parent, host, provider) = gateway();
    provider.connect().unwrap();
    for _ in 0..99 {
        host.current().emit(GatewayEvent::Close).unwrap();
        host.fire().unwrap();
    }
    assert_eq!(
        host.sockets.borrow().len(),
        100,
        "PUBLIC native pre-reset reconnect enumeration floor 99"
    );
    host.current().incoming("bad json").unwrap();
    assert_eq!(provider.last_message_timestamp(), Some(1000.0));
    assert!(
        !provider.is_initialized(),
        "PUBLIC native inbound data does not imply open"
    );
    for _ in 0..100 {
        host.current().emit(GatewayEvent::Close).unwrap();
        assert_eq!(
            host.timers.borrow().len(),
            1,
            "PUBLIC native reconnect reset allows each turn"
        );
        host.fire().unwrap();
    }
    assert_eq!(
        host.sockets.borrow().len(),
        200,
        "PUBLIC native post-reset reconnect enumeration floor 100"
    );
    host.current().emit(GatewayEvent::Close).unwrap();
    assert!(
        host.timers.borrow().is_empty(),
        "PUBLIC native 101st close cap"
    );
    assert!(
        parent
            .logs
            .borrow()
            .last()
            .unwrap()
            .0
            .ends_with("Reconnect count exceeded, giving up")
    );
    let (parent, host, provider) = gateway();
    provider.connect().unwrap();
    let socket = host.current();
    socket.open().unwrap();
    parent.now.set(7001.0);
    socket.fail_send.set(true);
    assert_eq!(host.fire().unwrap_err().message, "socket send refusal");
    assert!(
        host.timers.borrow().is_empty(),
        "PUBLIC native send error prevents rearm"
    );
    parent.fail_log.set(true);
    assert_eq!(
        socket.emit(GatewayEvent::Close).unwrap_err().message,
        "parent log refusal"
    );
    assert_eq!(provider.last_message_timestamp(), None);
    assert!(
        host.timers.borrow().is_empty(),
        "PUBLIC native close log failure prevents reconnect"
    );
    source_gate();
}

const FRAME: &str = r#"{
        "type":"data","exchangeId":"bybit","channel":"tickers","data":[{
            "id":"BTC","symbol":"BTC:PERP","bid":101,"ask":103,"last":107,
            "mark":109,"index":113,"percentage":2,"openInterest":17,
            "fundingRate":0.25,"fundingTime":1500,"volume":19,"quoteVolume":23,
            "info":{"$serde_json::private::Number":"17"}
        }]
    }"#;

#[test]
fn gateway_declaration_retains_literal_info_object() {
    let message: safe_cex_types::gateway::GatewayMessage = serde_json::from_str(FRAME).unwrap();
    let data = message.data.unwrap();
    let info = &data[0]["info"];
    assert!(info.is_object(), "GATEWAY declaration literal object kind");
    assert_eq!(
        info.as_object().unwrap().len(),
        1,
        "GATEWAY declaration exact own key count"
    );
    assert_eq!(
        info["$serde_json::private::Number"], "17",
        "GATEWAY declaration literal string value"
    );
    assert_eq!(
        data[0]["bid"].as_f64().unwrap().to_bits(),
        101.0f64.to_bits(),
        "GATEWAY declaration sibling bid"
    );
}
#[test]
fn gateway_provider_retains_literal_info_object() {
    let (parent, host, provider) = gateway();
    provider.connect().unwrap();
    let socket = host.current();
    socket.open().unwrap();
    socket.incoming(FRAME).unwrap();
    let snapshot = parent.data.borrow();
    let tickers = snapshot.tickers.borrow();
    assert_eq!(
        tickers.len(),
        1,
        "GATEWAY provider independent ticker count"
    );
    let info = tickers[0].info.as_ref().unwrap();
    assert!(info.is_object(), "GATEWAY provider literal object kind");
    assert_eq!(
        info.as_object().unwrap().len(),
        1,
        "GATEWAY provider exact own key count"
    );
    assert_eq!(
        info.get("$serde_json::private::Number").unwrap().as_text(),
        Some("17".encode_utf16().collect::<Vec<_>>().as_slice()),
        "GATEWAY provider literal string value"
    );
    assert_eq!(
        tickers[0].bid.to_bits(),
        101.0f64.to_bits(),
        "GATEWAY provider sibling bid"
    );
    drop(tickers);
    drop(snapshot);
    source_gate();
}

#[test]
fn in_process_seed_rechecks_retained_loaded_flags_after_parent_notification() {
    let source = Source::new();
    source.data.borrow().loaded.borrow_mut().markets = false;
    let parent = Parent::new();
    let retained = source.clone();
    *parent.after_update.borrow_mut() = Some(Rc::new(move || {
        retained.data.borrow().loaded.borrow_mut().markets = true;
    }));
    let provider = InProcessPublicDataProvider::new(parent.clone(), source);
    provider.connect().unwrap();
    assert_eq!(
        parent.updates.borrow().len(),
        2,
        "PROVIDER native retained loaded flag reread after ticker notification"
    );
    assert_eq!(parent.updates.borrow()[1], (false, true, true));
    assert_eq!(
        parent.data.borrow().markets.borrow()[0].symbol,
        "ADAPT:BTC:PERP",
        "PROVIDER native adapted market after reentrant loaded mutation"
    );
}

#[test]
fn gateway_candles_preserve_every_worker_interval_through_reconnect_and_cleanup() {
    use serde_json::{Value, json};
    let intervals = [
        "1M", "1s", "5s", "15s", "30s", "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h",
        "12h", "1d", "1w", "3h", "5h", "7h", "8h", "9h", "10h", "11h", "24h", "2d", "3d",
    ];
    for interval in intervals {
        let (_, host, provider) = gateway();
        provider.connect().unwrap();
        let socket = host.current();
        socket.open().unwrap();
        socket.sent.borrow_mut().clear();
        let seen = Rc::new(Cell::new(0));
        let count = seen.clone();
        let options =
            serde_json::from_value(json!({"symbol":"BTC:PERP","interval":interval})).unwrap();
        let off = provider
            .subscribe_candle(
                Rc::new(RefCell::new(options)),
                Rc::new(move |candle| {
                    assert_candle(candle);
                    count.set(count.get() + 1);
                    Ok(())
                }),
            )
            .unwrap();
        let candle = payload("candle");
        socket.incoming(&json!({"type":"data","exchangeId":"bybit","channel":"candle","symbol":"BTC:PERP","interval":interval,"data":candle}).to_string()).unwrap();
        assert_eq!(seen.get(), 1, "GATEWAY-INTERVAL native delivery {interval}");
        socket.open().unwrap();
        off().unwrap();
        let frames: Vec<Value> = socket
            .sent
            .borrow()
            .iter()
            .map(|text| serde_json::from_str(text).unwrap())
            .filter(|frame: &Value| frame["channel"] == "candle")
            .collect();
        assert_eq!(
            frames.len(),
            3,
            "GATEWAY-INTERVAL native frame count {interval}"
        );
        for (frame, kind) in frames.iter().zip(["subscribe", "subscribe", "unsubscribe"]) {
            assert_eq!(frame["type"], kind);
            assert_eq!(frame["symbol"], "BTC:PERP");
            assert_eq!(
                frame["interval"], interval,
                "GATEWAY-INTERVAL native frame interval"
            );
            let typed: safe_cex_types::gateway::GatewayMessage =
                serde_json::from_value(frame.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(typed.interval).unwrap(),
                json!(interval)
            );
        }
        provider.dispose().unwrap();
    }
    let source = std::process::Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/gateway-interval-source.cjs"),
        )
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "GATEWAY-INTERVAL actual source: {}",
        String::from_utf8_lossy(&source.stderr)
    );
}
