//! MEXC USDT/USDC perpetual futures, driven by the sans-I/O engine.
mod account;
mod history;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue,
};
use std::collections::{BTreeSet, HashMap};
use wire::*;

const REST: &str = "https://api.mexc.com";
const WS: &str = "wss://contract.mexc.com/edge";
const HEARTBEAT_MS: f64 = 8_000.0;
const SILENCE_MS: f64 = 20_000.0;
const MAX_BUFFERED: usize = 2_000;

#[derive(Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}

pub enum Pending {
    History(history::Request),
    Trade(trading::Request),
    Account(account::Request),
    Markets { rows: Vec<Market>, failed: bool },
    Tickers { rows: Vec<Ticker>, failed: bool },
    Depth { symbol: String, generation: u32 },
    Candles { command: CommandId, limit: usize },
}

#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    heartbeat: Option<TimerId>,
    reconnect: Option<TimerId>,
    last_received: f64,
    attempts: u32,
}

struct BookState {
    count: u32,
    generation: u32,
    book: Book,
    version: Option<u64>,
    buffering: Option<Vec<Depth>>,
    fetching: bool,
    retry: Option<TimerId>,
    failures: u32,
}

pub struct MexcFutures {
    options: Options,
    credentials: Option<sign::Credentials>,
    account: account::Account,
    trading: trading::Trading,
    public: Connection,
    books: HashMap<String, BookState>,
    volume_units: HashMap<String, f64>,
    ticker_symbols: Vec<String>,
    ticker_subscriptions: BTreeSet<String>,
    candles: HashMap<(String, String), u32>,
    next_book_generation: u32,
    startup_retry: Option<TimerId>,
    startup_attempts: u8,
    depth_paused_until: f64,
}

impl MexcFutures {
    pub fn new(options: Options) -> Self {
        let credentials = match (&options.key, &options.secret) {
            (Some(key), Some(secret)) if !options.public_data_only && !key.trim().is_empty() => {
                Some(sign::Credentials {
                    key: key.trim().to_owned(),
                    secret: secret.clone(),
                })
            }
            _ => None,
        };
        Self {
            options,
            credentials,
            account: account::Account::new(),
            trading: trading::Trading::default(),
            public: Connection::default(),
            books: HashMap::new(),
            volume_units: HashMap::new(),
            ticker_symbols: vec![],
            ticker_subscriptions: BTreeSet::new(),
            candles: HashMap::new(),
            next_book_generation: 0,
            startup_retry: None,
            startup_attempts: 0,
            depth_paused_until: 0.0,
        }
    }
    fn get(&self, cx: &mut Cx<'_, Pending>, path: &str, pending: Pending) {
        cx.request(Method::Get, format!("{REST}{path}"), vec![], None, pending);
    }
    fn stream(&self, cx: &mut Cx<'_, Pending>, path: &str, pending: Pending) {
        cx.request_streamed(
            Method::Get,
            format!("{REST}{path}"),
            vec![],
            ArrayAt::EnvelopeField("data"),
            pending,
        );
    }
    fn send(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: &str,
        symbol: &str,
        extra: Option<(&str, &str)>,
    ) {
        if !self.public.open {
            return;
        }
        let Some(socket) = self.public.socket else {
            return;
        };
        #[derive(serde::Serialize)]
        struct Subscription<'a> {
            method: &'a str,
            param: std::collections::BTreeMap<&'a str, &'a str>,
        }
        let mut param = std::collections::BTreeMap::from([("symbol", symbol)]);
        if let Some((k, v)) = extra {
            param.insert(k, v);
        }
        cx.send(
            socket,
            serde_json::to_string(&Subscription { method, param }).unwrap(),
        );
    }
    fn reconcile_tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.public.open {
            return;
        }
        let ids: Vec<_> = if self.ticker_symbols.is_empty() {
            cx.store
                .markets()
                .iter()
                .take(100)
                .map(|m| m.id.clone())
                .collect()
        } else {
            self.ticker_symbols
                .iter()
                .filter_map(|symbol| cx.store.market(symbol).map(|m| m.id.clone()))
                .collect()
        };
        let desired: BTreeSet<_> = ids.iter().cloned().collect();
        for id in self.ticker_subscriptions.difference(&desired) {
            self.send(cx, "unsub.ticker", id, None);
        }
        // Keep market/request order on the wire while deduplicating ids.
        self.ticker_subscriptions.retain(|id| desired.contains(id));
        for id in ids {
            if self.ticker_subscriptions.insert(id.clone()) {
                self.send(cx, "sub.ticker", &id, None);
            }
        }
    }
    fn mini_ticker(&self, cx: &mut Cx<'_, Pending>, item: &str) {
        #[derive(serde::Deserialize)]
        struct Identity<'a> {
            #[serde(borrow)]
            symbol: Str<'a>,
        }
        let Ok(identity) = serde_json::from_str::<Identity<'_>>(item) else {
            cx.log(
                LogLevel::Warning,
                "Skipping malformed MEXC all-market ticker identity",
            );
            return;
        };
        let symbol = compact(&identity.symbol);
        let Some(market) = cx.store.market(&symbol) else {
            return;
        };
        let size = market.contract_size.unwrap_or(1.0);
        let Ok(raw) = serde_json::from_str::<MiniTick>(item) else {
            cx.log(
                LogLevel::Warning,
                "Skipping malformed MEXC all-market ticker row",
            );
            return;
        };
        if [raw.last_price, raw.fair_price, raw.index_price]
            .into_iter()
            .flatten()
            .any(|n| n < 0.0)
            || [raw.volume24, raw.amount24]
                .into_iter()
                .flatten()
                .any(|n| n < 0.0)
        {
            cx.log(
                LogLevel::Warning,
                "Skipping invalid MEXC all-market ticker row",
            );
            return;
        }
        cx.store.update_ticker(&symbol, |t| {
            // A zero price is an unavailable quote (seen in the public feed),
            // not a reason to discard valid mark/index/volume fields.
            if let Some(last) = raw.last_price.filter(|n| *n > 0.0) {
                t.last = last;
            }
            if let Some(mark) = raw.fair_price.filter(|n| *n > 0.0) {
                // The all-market feed omits holdVol: retain the known quantity,
                // but value its quote notional at the new mark.
                if t.mark > 0.0 {
                    t.open_interest *= mark / t.mark;
                }
                t.mark = mark;
            }
            if let Some(index) = raw.index_price.filter(|n| *n > 0.0) {
                t.index = index;
            }
            if let Some(rate) = raw.rise_fall_rate {
                t.percentage = if rate.abs() <= 1.0 {
                    rate * 100.0
                } else {
                    rate
                };
            }
            if let Some(volume) = raw.volume24 {
                t.volume = volume * size;
            }
            if let Some(volume) = raw.amount24 {
                t.quote_volume = volume;
            }
        });
    }
    fn subscribe_depth(&self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if let (Some(socket), Some(market)) = (self.public.socket, cx.store.market(symbol))
            && self.public.open
        {
            cx.send(
                socket,
                format!(
                    r#"{{"method":"sub.depth","param":{{"symbol":"{}","compress":false}}}}"#,
                    market.id
                ),
            );
        }
    }
    fn snapshot(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if !self.public.open {
            return;
        }
        let Some(market) = cx.store.market(symbol) else {
            return;
        };
        let path = format!("/api/v1/contract/depth/{}?limit=1000", market.id);
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        if state.fetching || state.retry.is_some() {
            return;
        }
        if cx.now < self.depth_paused_until {
            state.retry = Some(cx.set_timer(self.depth_paused_until - cx.now));
            return;
        }
        self.next_book_generation = self.next_book_generation.wrapping_add(1);
        state.generation = self.next_book_generation;
        state.version = None;
        state.buffering.get_or_insert_default();
        state.fetching = true;
        let generation = state.generation;
        self.get(
            cx,
            &path,
            Pending::Depth {
                symbol: symbol.into(),
                generation,
            },
        );
    }
    fn resync(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        self.retry_depth(cx, symbol, false);
    }
    fn retry_depth(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, retain: bool) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        state.version = None;
        state.fetching = false;
        if !retain {
            state.buffering = None;
        }
        state.generation = state.generation.wrapping_add(1);
        state.failures += 1;
        if let Some(timer) = state.retry.take() {
            cx.clear_timer(timer);
        }
        state.retry =
            Some(cx.set_timer((250.0 * 2f64.powi(state.failures.min(7) as i32)).min(30_000.0)));
    }
    fn apply_depth(book: &mut Book, depth: &Depth, size: f64) {
        for (side, rows) in [(BookSide::Bids, &depth.bids), (BookSide::Asks, &depth.asks)] {
            for Level(price, amount, _) in rows {
                if *price > 0.0 && *amount >= 0.0 {
                    book.apply(side, *price, *amount * size);
                }
            }
        }
    }
    fn depth(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, depth: Depth) {
        if depth
            .bids
            .iter()
            .chain(&depth.asks)
            .any(|Level(p, v, _)| *p <= 0.0 || *v < 0.0)
        {
            self.resync(cx, symbol);
            return;
        }
        let size = cx
            .store
            .market(symbol)
            .and_then(|m| m.contract_size)
            .unwrap_or(1.0);
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        if state.version.is_none() {
            let buffer = state.buffering.get_or_insert_default();
            buffer.push(depth);
            if buffer.len() > MAX_BUFFERED {
                self.resync(cx, symbol);
            }
            return;
        }
        let Some(version) = state.version else {
            return;
        };
        if depth.version <= version {
            return;
        }
        if depth.version != version + 1 {
            state.buffering = Some(vec![depth]);
            self.retry_depth(cx, symbol, true);
            return;
        }
        Self::apply_depth(&mut state.book, &depth, size);
        state.version = Some(depth.version);
        state.failures = 0;
        cx.book_changed(symbol);
    }
    fn ticker(raw: Tick<'_>, market: &Market) -> Ticker {
        let size = market.contract_size.unwrap_or(1.0);
        let mark = if raw.fair_price == 0.0 {
            raw.last_price
        } else {
            raw.fair_price
        };
        Ticker {
            id: market.id.clone(),
            symbol: market.symbol.clone(),
            bid: if raw.bid1 == 0.0 {
                raw.last_price
            } else {
                raw.bid1
            },
            ask: if raw.ask1 == 0.0 {
                raw.last_price
            } else {
                raw.ask1
            },
            last: raw.last_price,
            mark,
            index: if raw.index_price == 0.0 {
                raw.last_price
            } else {
                raw.index_price
            },
            percentage: if raw.rise_fall_rate.abs() <= 1.0 {
                raw.rise_fall_rate * 100.0
            } else {
                raw.rise_fall_rate
            },
            open_interest: raw.hold_vol * size * mark,
            funding_rate: raw.funding_rate,
            funding_time: millis(raw.funding_time),
            volume: raw.volume24 * size,
            quote_volume: raw.amount24,
            mid_price: None,
        }
    }
    fn market(raw: Contract<'_>) -> Option<Market> {
        let settle = if raw.settle_coin.is_empty() {
            &raw.quote_coin
        } else {
            &raw.settle_coin
        };
        if !matches!(settle.as_ref(), "USDT" | "USDC")
            || raw.api_allowed == Some(false)
            || raw.is_hidden == Some(true)
            || raw.state != 0.0
        {
            return None;
        }
        if raw.contract_size <= 0.0 || raw.vol_unit <= 0.0 || raw.price_unit <= 0.0 {
            return None;
        }
        let step = (raw.contract_size * raw.vol_unit).max(raw.contract_size);
        Some(Market {
            symbol: compact(&raw.symbol),
            pretty_symbol: Some(raw.symbol.to_string()),
            id: raw.symbol.into_owned(),
            base: raw.base_coin.into_owned(),
            quote: raw.quote_coin.into_owned(),
            active: true,
            precision: Precision {
                amount: step,
                price: raw.price_unit,
            },
            limits: Limits {
                amount: MinMax {
                    min: (raw.min_vol * raw.contract_size).max(step),
                    max: raw.max_vol * raw.contract_size,
                },
                leverage: MinMax {
                    min: raw.min_leverage,
                    max: raw.max_leverage,
                },
                price: None,
            },
            contract_size: Some(raw.contract_size),
            maker_fee: Some(raw.maker_fee_rate),
            taker_fee: Some(raw.taker_fee_rate),
            is_inverse: None,
            is_perpetual: Some(true),
            is_expiring: None,
            is_spot: None,
            is_read_only: None,
        })
    }
    fn candle(raw: Kline<'_>) -> Candle {
        Candle {
            timestamp: (millis(raw.t) / 1000.0).floor(),
            open: raw.o,
            high: raw.h,
            low: raw.l,
            close: raw.c,
            volume: raw.q,
        }
    }
    fn public_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.socket = Some(cx.connect_channel(WS, safe_cex_engine::SocketChannel::Public));
        self.account_start(cx);
        if self.options.public_data_only || self.options.key.as_ref().is_none_or(|s| s.is_empty()) {
            cx.store.set_orders(vec![]);
            cx.store.set_positions(vec![]);
            cx.store.set_balance(Default::default());
        }
    }
    fn retry_startup(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.startup_retry.is_none() && self.startup_attempts < 3 {
            self.startup_attempts += 1;
            self.startup_retry = Some(cx.set_timer(1000.0 * f64::from(self.startup_attempts)));
        }
    }
    fn failure(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Depth { symbol, generation } => {
                if self
                    .books
                    .get(&symbol)
                    .is_some_and(|s| s.generation == generation)
                {
                    self.retry_depth(cx, &symbol, true);
                }
            }
            Pending::Candles { command, .. } => cx.done(command, Err(error.into())),
            Pending::Account(request) => self.account_failed(cx, request, error),
            Pending::Trade(request) => self.trade_failed(cx, request.command, error),
            Pending::History(request) => self.query_failed(cx, request, error),
            Pending::Markets { .. } | Pending::Tickers { .. } => {
                cx.log(LogLevel::Error, error);
                self.retry_startup(cx);
            }
        }
    }
}

impl Venue for MexcFutures {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        self.stream(
            cx,
            "/api/v1/contract/detail",
            Pending::Markets {
                rows: vec![],
                failed: false,
            },
        );
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.private.socket == Some(socket) {
            self.account_open(cx, socket);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.attempts = 0;
        self.public.last_received = cx.now;
        cx.send(socket, r#"{"method":"ping"}"#);
        self.public.heartbeat = Some(cx.set_timer(HEARTBEAT_MS));
        // Sparse once-per-second prices cover every listed market. Detailed
        // subscriptions provide funding, index and open-interest quantities.
        cx.send(
            socket,
            r#"{"method":"sub.tickers","param":{},"gzip":false}"#,
        );
        self.reconcile_tickers(cx);
        let symbols: Vec<_> = self.books.keys().cloned().collect();
        for symbol in symbols {
            self.subscribe_depth(cx, &symbol);
            self.snapshot(cx, &symbol);
        }
        for (symbol, interval) in self.candles.keys() {
            if let (Some(m), Some(i)) = (cx.store.market(symbol), wire::interval(interval)) {
                let id = m.id.clone();
                self.send(cx, "sub.kline", &id, Some(("interval", i)));
            }
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.private.socket == Some(socket) {
            self.account_message(cx, text);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.last_received = cx.now;
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        let Some(data) = message.data else {
            return;
        };
        if message.success == Some(false) {
            cx.log(LogLevel::Warning, "MEXC websocket request failed");
            return;
        }
        let symbol = compact(&message.symbol);
        match message.channel.as_ref() {
            "push.tickers" => {
                let _ = safe_cex_engine::json::each::<&serde_json::value::RawValue>(
                    data.get(),
                    |row| self.mini_ticker(cx, row.get()),
                );
            }
            "push.ticker" => {
                if let (Ok(raw), Some(market)) = (
                    serde_json::from_str::<Tick<'_>>(data.get()),
                    cx.store.market(&symbol),
                ) {
                    let ticker = Self::ticker(raw, market);
                    cx.store.update_ticker(&symbol, |t| *t = ticker);
                }
            }
            "push.depth" => match serde_json::from_str(data.get()) {
                Ok(depth) => self.depth(cx, &symbol, depth),
                Err(_) => self.resync(cx, &symbol),
            },
            "push.kline" => {
                if let Ok(raw) = serde_json::from_str::<Kline<'_>>(data.get()) {
                    let interval = self
                        .candles
                        .keys()
                        .find(|(s, i)| {
                            s == &symbol && wire::interval(i) == Some(raw.interval.as_ref())
                        })
                        .map(|(_, i)| i.clone());
                    if let Some(interval) = interval {
                        cx.candle(&symbol, &interval, Self::candle(raw));
                    }
                }
            }
            _ => {}
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account.private.socket == Some(socket) {
            self.account_close(cx, socket);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.socket = None;
        self.public.open = false;
        self.ticker_subscriptions.clear();
        if let Some(timer) = self.public.heartbeat.take() {
            cx.clear_timer(timer);
        }
        self.public.attempts += 1;
        self.public.reconnect = Some(
            cx.set_timer((500.0 * 2f64.powi(self.public.attempts.min(6) as i32)).min(30_000.0)),
        );
        for state in self.books.values_mut() {
            state.version = None;
            state.buffering = None;
            state.fetching = false;
            state.generation = state.generation.wrapping_add(1);
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::History(request) => self.query_item(cx, request, item),
            Pending::Account(account::Request::Leverage { rows, bad, .. }) => {
                match serde_json::from_str::<wire::LeverageRow>(item) {
                    Ok(r) => rows.push(r),
                    Err(_) => *bad = true,
                }
            }
            Pending::Account(account::Request::Snapshot(snapshot)) => {
                self.account_item(cx, snapshot, item)
            }
            Pending::Markets { rows, failed } => match serde_json::from_str::<Contract<'_>>(item) {
                Ok(raw) => {
                    let unit = raw.vol_unit;
                    if let Some(m) = Self::market(raw) {
                        self.volume_units.insert(m.symbol.clone(), unit);
                        rows.push(m);
                    }
                }
                Err(_) => *failed = true,
            },
            Pending::Tickers { rows, failed } => match serde_json::from_str::<Tick<'_>>(item) {
                Ok(raw) => {
                    if let Some(m) = cx.store.market_by_id(&raw.symbol) {
                        rows.push(Self::ticker(raw, m));
                    }
                }
                Err(_) => *failed = true,
            },
            _ => {}
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Trade(request) = pending {
            self.trade_response(cx, request, response);
            return;
        }
        if matches!(&pending, Pending::Markets { .. } | Pending::Tickers { .. })
            && let Err(error) = Envelope::checked(response.body)
        {
            self.failure(cx, pending, &error);
            return;
        }
        if response.status == 429 {
            self.depth_paused_until = cx.now
                + response
                    .header("retry-after")
                    .and_then(|s| s.parse::<f64>().ok())
                    .map_or(10_000.0, |s| s * 1000.0);
        }
        if response.status == 429 {
            self.account.paused_until = self.depth_paused_until;
        }
        if response.status != 200 {
            self.failure(cx, pending, &format!("MEXC HTTP {}", response.status));
            return;
        }
        match pending {
            Pending::Account(request) => {
                self.account_response(cx, request, response);
                self.refresh_stop_orders(cx);
            }
            Pending::Trade(_) => unreachable!(),
            Pending::History(request) => self.query_response(cx, request, response),
            Pending::Markets { rows, failed } => {
                if failed {
                    cx.log(LogLevel::Error, "Malformed MEXC market list");
                    self.retry_startup(cx);
                    return;
                }
                self.startup_attempts = 0;
                cx.store.set_markets(rows);
                self.stream(
                    cx,
                    "/api/v1/contract/ticker",
                    Pending::Tickers {
                        rows: vec![],
                        failed: false,
                    },
                );
            }
            Pending::Tickers { mut rows, failed } => {
                if failed {
                    cx.log(LogLevel::Error, "Malformed MEXC ticker list");
                    self.retry_startup(cx);
                    return;
                }
                rows.sort_by_key(|t| cx.store.market_index(&t.symbol));
                self.startup_attempts = 0;
                cx.store.set_tickers(rows);
                self.public_ready(cx);
            }
            Pending::Depth { symbol, generation } => {
                let depth = Envelope::checked(response.body).and_then(|e| {
                    let snapshot: DepthSnapshot =
                        serde_json::from_str(e.payload()?).map_err(|e| e.to_string())?;
                    if snapshot
                        .bids
                        .iter()
                        .chain(&snapshot.asks)
                        .any(|Level(p, v, _)| *p <= 0.0 || *v < 0.0)
                    {
                        return Err("Invalid MEXC depth snapshot level".into());
                    }
                    Ok(Depth {
                        version: snapshot.version,
                        bids: snapshot.bids,
                        asks: snapshot.asks,
                    })
                });
                let Some(state) = self.books.get_mut(&symbol) else {
                    return;
                };
                if state.generation != generation || !state.fetching {
                    return;
                }
                let Ok(depth) = depth else {
                    self.resync(cx, &symbol);
                    return;
                };
                // REST can lag the live stream. Validate the complete retained
                // chain before touching the candidate book or publishing a prefix.
                let mut version = depth.version;
                let contiguous = state.buffering.iter().flatten().all(|update| {
                    if update.version <= version {
                        return true;
                    }
                    if version.checked_add(1) != Some(update.version) {
                        return false;
                    }
                    version = update.version;
                    true
                });
                if !contiguous {
                    self.retry_depth(cx, &symbol, true);
                    return;
                }
                let size = cx
                    .store
                    .market(&symbol)
                    .and_then(|m| m.contract_size)
                    .unwrap_or(1.0);
                state.book.clear();
                Self::apply_depth(&mut state.book, &depth, size);
                state.fetching = false;
                let mut applied = depth.version;
                let buffer = state.buffering.take().unwrap_or_default();
                for update in buffer {
                    if update.version > applied {
                        Self::apply_depth(&mut state.book, &update, size);
                        applied = update.version;
                    }
                }
                state.version = Some(version);
                state.failures = 0;
                cx.book_changed(&symbol);
            }
            Pending::Candles { command, limit } => {
                let result = Envelope::checked(response.body).and_then(|e| {
                    let k: Klines =
                        serde_json::from_str(e.payload()?).map_err(|e| e.to_string())?;
                    let n = k.time.len();
                    if [
                        k.open.len(),
                        k.high.len(),
                        k.low.len(),
                        k.close.len(),
                        k.vol.len(),
                    ]
                    .iter()
                    .any(|&l| l != n)
                    {
                        return Err("Malformed MEXC klines".into());
                    }
                    let rows: Vec<Candle> = (n.saturating_sub(limit)..n)
                        .map(|i| Candle {
                            timestamp: (millis(k.time[i]) / 1000.0).floor(),
                            open: k.open[i],
                            high: k.high[i],
                            low: k.low[i],
                            close: k.close[i],
                            volume: k.vol[i],
                        })
                        .collect();
                    serde_json::to_string(&rows).map_err(|e| e.to_string())
                });
                cx.done(command, result);
            }
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.failure(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.account_timer(cx, timer);
        self.trade_timer(cx, timer);
        if self.startup_retry == Some(timer) {
            self.startup_retry = None;
            if cx.store.loaded().markets {
                self.stream(
                    cx,
                    "/api/v1/contract/ticker",
                    Pending::Tickers {
                        rows: vec![],
                        failed: false,
                    },
                );
            } else {
                self.stream(
                    cx,
                    "/api/v1/contract/detail",
                    Pending::Markets {
                        rows: vec![],
                        failed: false,
                    },
                );
            }
        }
        if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.public.socket =
                Some(cx.connect_channel(WS, safe_cex_engine::SocketChannel::Public));
        }
        if self.public.heartbeat == Some(timer) {
            self.public.heartbeat = None;
            if let Some(socket) = self.public.socket {
                if cx.now - self.public.last_received >= SILENCE_MS {
                    cx.close(socket);
                    self.on_close(cx, socket, 1006);
                } else {
                    cx.send(socket, r#"{"method":"ping"}"#);
                    self.public.heartbeat = Some(cx.set_timer(HEARTBEAT_MS));
                }
            }
        }
        let symbols: Vec<_> = self
            .books
            .iter()
            .filter(|(_, s)| s.retry == Some(timer))
            .map(|(s, _)| s.clone())
            .collect();
        for symbol in symbols {
            self.books.get_mut(&symbol).unwrap().retry = None;
            self.snapshot(cx, &symbol);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SetTickerSymbols { symbols } => {
                self.ticker_symbols = symbols;
                self.reconcile_tickers(cx);
            }
            Command::Query { command, query } => self.start_query(cx, command, query),
            Command::FetchLeverage { command, symbol } => self.fetch_leverage(cx, command, symbol),
            Command::SubscribeOrderBook { symbol } => {
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.count += 1;
                    return;
                }
                self.books.insert(
                    symbol.clone(),
                    BookState {
                        count: 1,
                        generation: 0,
                        book: Book::default(),
                        version: None,
                        buffering: None,
                        fetching: false,
                        retry: None,
                        failures: 0,
                    },
                );
                self.subscribe_depth(cx, &symbol);
                self.snapshot(cx, &symbol);
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.count = s.count.saturating_sub(1);
                    if s.count > 0 {
                        return;
                    }
                }
                if let Some(s) = self.books.remove(&symbol)
                    && let Some(t) = s.retry
                {
                    cx.clear_timer(t);
                }
                if let Some(m) = cx.store.market(&symbol) {
                    let id = m.id.clone();
                    self.send(cx, "unsub.depth", &id, None);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval.clone());
                if let Some(count) = self.candles.get_mut(&key) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.candles.remove(&key);
                        if let (Some(m), Some(i)) =
                            (cx.store.market(&symbol), wire::interval(&interval))
                        {
                            let id = m.id.clone();
                            self.send(cx, "unsub.kline", &id, Some(("interval", i)));
                        }
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                let Some(i) = wire::interval(&interval) else {
                    cx.log(LogLevel::Error, "Unsupported MEXC candle interval");
                    return;
                };
                let key = (symbol.clone(), interval);
                let n = self.candles.entry(key).or_insert(0);
                *n += 1;
                if *n == 1
                    && let Some(m) = cx.store.market(&symbol)
                {
                    let id = m.id.clone();
                    self.send(cx, "sub.kline", &id, Some(("interval", i)));
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => {
                let (Some(m), Some(i)) = (cx.store.market(&symbol), wire::interval(&interval))
                else {
                    cx.done(
                        command,
                        Err("Unknown MEXC market or candle interval".into()),
                    );
                    return;
                };
                let mut path = format!("/api/v1/contract/kline/{}?interval={i}", m.id);
                if let Some(t) = from_ms {
                    path.push_str(&format!("&start={}", (t / 1000.0).floor()));
                }
                if let Some(t) = to_ms {
                    path.push_str(&format!("&end={}", (t / 1000.0).floor()));
                }
                self.get(
                    cx,
                    &path,
                    Pending::Candles {
                        command,
                        limit: limit.map_or(usize::MAX, |n| n as usize),
                    },
                );
            }
            trade => self.trade_command(cx, trade),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|s| s.version.is_some())
            .map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, s)| s.version.is_some())
                .map(|(s, b)| (s.as_str(), &b.book)),
        )
    }
}

#[cfg(test)]
mod secondary_tests;
