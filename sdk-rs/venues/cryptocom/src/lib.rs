//! Crypto.com Exchange v1 perpetual derivatives on the sans-I/O engine.
mod account;
mod history;
pub mod sign;
mod trading;
mod wire;

use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::model::{Balance, Candle, Market, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue, num,
};
use std::collections::{BTreeMap, HashMap};
use wire::*;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}

pub enum Pending {
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    Markets {
        rows: Vec<Market>,
        indexes: HashMap<String, String>,
        invalid: bool,
    },
    Tickers {
        rows: Vec<Ticker>,
        invalid: bool,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        invalid: bool,
    },
}

#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    last: f64,
    ready: Option<TimerId>,
    reconnect: Option<TimerId>,
}

struct BookState {
    book: Book,
    subscribers: u32,
    synced: bool,
}

pub struct Cryptocom {
    options: Options,
    rest: &'static str,
    sockets: [Connection; 2],
    watchdog: Option<TimerId>,
    discovery: safe_cex_engine::startup::Backoff<bool>,
    subscriptions: BTreeMap<String, u32>,
    indexes: HashMap<String, String>,
    books: HashMap<String, BookState>,
    message_id: u64,
    credentials: Option<sign::Credentials>,
    account: account::Account,
    trading: trading::Trading,
    history: history::History,
}

pub(crate) fn interval(value: &str) -> Result<&str, String> {
    match value {
        "1m" | "5m" | "15m" | "30m" | "1h" | "2h" | "4h" | "12h" => Ok(value),
        "1d" => Ok("1D"),
        "1w" => Ok("7D"),
        _ => Err(format!(
            "Unsupported Crypto.com candlestick interval: {value}"
        )),
    }
}

fn quoted(s: &str) -> String {
    serde_json::to_string(s).expect("string")
}

pub(crate) fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

impl Cryptocom {
    fn discovery_read(&self, cx: &mut Cx<'_, Pending>, tickers: bool) {
        self.get(
            cx,
            if tickers {
                "/public/get-tickers"
            } else {
                "/public/get-instruments"
            },
            if tickers {
                Pending::Tickers {
                    rows: vec![],
                    invalid: false,
                }
            } else {
                Pending::Markets {
                    rows: vec![],
                    indexes: HashMap::new(),
                    invalid: false,
                }
            },
        );
    }
    pub fn new(options: Options) -> Self {
        let credentials = if options.public_data_only {
            None
        } else {
            options
                .key
                .as_ref()
                .filter(|k| !k.is_empty())
                .zip(options.secret.as_ref().filter(|s| !s.is_empty()))
                .map(|(key, secret)| sign::Credentials {
                    key: key.clone(),
                    secret: secret.clone(),
                })
        };
        Self {
            credentials,
            account: Default::default(),
            trading: Default::default(),
            history: Default::default(),
            rest: if options.testnet {
                "https://uat-api.3ona.co/exchange/v1"
            } else {
                "https://api.crypto.com/exchange/v1"
            },
            options,
            sockets: Default::default(),
            watchdog: None,
            discovery: Default::default(),
            subscriptions: BTreeMap::new(),
            indexes: HashMap::new(),
            books: HashMap::new(),
            message_id: 0,
        }
    }

    fn connect(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        let root = if self.options.testnet {
            "wss://uat-stream.3ona.co"
        } else {
            "wss://stream.crypto.com"
        };
        let kind = if channel == 0 { "market" } else { "user" };
        self.sockets[channel].socket = Some(cx.connect_channel(
            format!("{root}/exchange/v1/{kind}"),
            if channel == 0 {
                safe_cex_engine::SocketChannel::Public
            } else {
                safe_cex_engine::SocketChannel::Private
            },
        ));
    }

    fn get(&self, cx: &mut Cx<'_, Pending>, path: &str, pending: Pending) {
        cx.request_streamed(
            Method::Get,
            format!("{}{path}", self.rest),
            vec![],
            ArrayAt::Path(&["result", "data"]),
            pending,
        );
    }

    fn subscribe(&mut self, cx: &mut Cx<'_, Pending>, channels: &[String], method: &str) {
        if !self.sockets[0].open || self.sockets[0].ready.is_some() {
            return;
        }
        for chunk in channels.chunks(100) {
            self.message_id += 1;
            cx.send(
                self.sockets[0].socket.unwrap(),
                format!(
                    "{{\"id\":{},\"method\":{},\"params\":{{\"channels\":{}}},\"nonce\":{}}}",
                    self.message_id,
                    quoted(method),
                    serde_json::to_string(chunk).unwrap(),
                    cx.now as u64
                ),
            );
        }
    }

    fn add_channel(&mut self, cx: &mut Cx<'_, Pending>, channel: String) {
        let count = self.subscriptions.entry(channel.clone()).or_default();
        *count += 1;
        if *count == 1 {
            self.subscribe(cx, &[channel], "subscribe");
        }
    }

    fn remove_channel(&mut self, cx: &mut Cx<'_, Pending>, channel: String) {
        if let Some(count) = self.subscriptions.get_mut(&channel) {
            *count -= 1;
            if *count == 0 {
                self.subscriptions.remove(&channel);
                self.subscribe(cx, &[channel], "unsubscribe");
            }
        }
    }

    fn market_channels(&mut self, cx: &mut Cx<'_, Pending>) {
        let mut seeded = self
            .subscriptions
            .keys()
            .filter(|channel| {
                ["ticker.", "mark.", "index.", "funding."]
                    .iter()
                    .any(|prefix| channel.starts_with(prefix))
            })
            .count();
        let markets = cx.store.markets();
        let channels =
            markets
                .iter()
                .map(|m| format!("ticker.{}", m.id))
                .chain(markets.iter().flat_map(|m| {
                    let index = self.indexes.get(&m.id).filter(|i| i.ends_with("-INDEX"));
                    [
                        Some(format!("mark.{}", m.id)),
                        index.map(|i| format!("index.{i}")),
                        Some(format!("funding.{}", m.id)),
                    ]
                    .into_iter()
                    .flatten()
                }));
        for channel in channels {
            if self.subscriptions.contains_key(&channel) {
                continue;
            }
            if seeded >= 350 {
                break;
            }
            self.subscriptions.insert(channel, 1);
            seeded += 1;
        }
    }

    fn message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        message: Message<'_>,
    ) -> serde_json::Result<()> {
        let symbol = message.instrument_name;
        match message.channel {
            "ticker" => {
                let mut first = true;
                json::each::<TickerRow>(message.data.get(), |row| {
                    if first {
                        cx.store.update_ticker(symbol, |t| row.apply(t));
                        first = false;
                    }
                })?;
            }
            "book" => {
                let Some(state) = self.books.get_mut(symbol) else {
                    return Ok(());
                };
                let mut first = true;
                json::each::<BookRow>(message.data.get(), |row| {
                    if !first {
                        return;
                    }
                    first = false;
                    state.book.clear();
                    for (side, rows) in [(BookSide::Bids, row.bids), (BookSide::Asks, row.asks)] {
                        state.book.load_sorted(
                            side,
                            rows.into_iter()
                                .map(|(p, q, _)| (num::parse(p), num::parse(q))),
                        );
                    }
                    state.synced = true;
                    let bid = state.book.best_bid().unwrap_or(0.0);
                    let ask = state.book.best_ask().unwrap_or(0.0);
                    cx.store.update_ticker(symbol, |t| {
                        t.bid = bid;
                        t.ask = ask;
                    });
                    cx.book_changed(symbol);
                })?;
            }
            "mark" | "index" | "funding" => {
                let mut first = true;
                json::each::<Valuation>(message.data.get(), |row| {
                    if !first {
                        return;
                    }
                    first = false;
                    let Ok(value) = row.v.parse::<f64>() else {
                        return;
                    };
                    if !value.is_finite() || (message.channel != "funding" && value <= 0.0) {
                        return;
                    }
                    if message.channel == "index" {
                        for (market, index) in &self.indexes {
                            if index == symbol {
                                cx.store.update_ticker(market, |t| t.index = value);
                            }
                        }
                    } else {
                        cx.store.update_ticker(symbol, |t| {
                            if message.channel == "mark" {
                                t.mark = value
                            } else {
                                t.funding_rate = value
                            }
                        });
                    }
                })?;
            }
            "candlestick" => {
                let timeframe = message
                    .subscription
                    .strip_prefix("candlestick.")
                    .and_then(|s| s.split('.').next())
                    .unwrap_or("");
                let timeframe = match timeframe {
                    "1D" => "1d",
                    "7D" => "1w",
                    other => other,
                };
                if !self.subscriptions.contains_key(message.subscription) {
                    return Ok(());
                }
                json::each::<CandleRow>(message.data.get(), |row| {
                    cx.candle(symbol, timeframe, row.candle())
                })?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Venue for Cryptocom {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        self.discovery_read(cx, false);
        self.watchdog = Some(cx.set_timer(5_000.0));
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if let Some(state) = self.sockets.iter_mut().find(|s| s.socket == Some(socket)) {
            state.open = true;
            state.last = cx.now;
            state.ready = Some(cx.set_timer(1_000.0));
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(channel) = self.sockets.iter().position(|s| s.socket == Some(socket)) else {
            return;
        };
        self.sockets[channel].last = cx.now;
        if channel == 0
            && let Ok(push) = serde_json::from_str::<TickerPush>(text)
            && push.code == 0
        {
            let TickerChannel::Ticker = push.result.channel;
            if let Some(row) = push.result.data.first() {
                cx.store
                    .update_ticker(push.result.instrument_name, |t| row.apply(t));
            }
            return;
        }
        let result = (|| {
            let envelope: Envelope<'_> = serde_json::from_str(text)?;
            if envelope.method == "public/heartbeat" {
                cx.send(
                    socket,
                    format!(
                        "{{\"id\":{},\"method\":\"public/respond-heartbeat\"}}",
                        envelope.id
                    ),
                );
            } else if channel == 1 && envelope.method == "public/auth" && envelope.code == 0 {
                self.authenticated(cx, &envelope);
            } else if envelope.code != 0 {
                cx.log(
                    LogLevel::Error,
                    format!(
                        "Crypto.com API Error {}: {}",
                        envelope.code, envelope.message
                    ),
                );
            } else if let Some(result) = envelope.result {
                // Subscribe acknowledgements may carry a result without a channel.
                if let Ok(message) = serde_json::from_str::<Message>(result.get()) {
                    if channel == 1 {
                        self.private_message(cx, message)?;
                    } else {
                        self.message(cx, message)?;
                    }
                }
            }
            Ok::<_, serde_json::Error>(())
        })();
        if let Err(error) = result {
            cx.log(LogLevel::Error, error.to_string());
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if let Some(channel) = self.sockets.iter().position(|s| s.socket == Some(socket)) {
            let state = &mut self.sockets[channel];
            state.open = false;
            state.socket = None;
            if let Some(timer) = state.ready.take() {
                cx.clear_timer(timer);
            }
            if state.reconnect.is_none() {
                state.reconnect = Some(cx.set_timer(1_000.0));
            }
            if channel == 0 {
                for state in self.books.values_mut() {
                    state.synced = false;
                }
            }
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::History(request) => self.history_item(cx, request, item),
            Pending::Trade(_) => {}
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::Markets {
                rows,
                indexes,
                invalid: _,
            } => match serde_json::from_str::<Instrument>(item) {
                Ok(row) => {
                    if let Some(market) = row.market() {
                        indexes.insert(market.id.clone(), row.underlying_symbol.into());
                        rows.push(market);
                    }
                }
                Err(_) => safe_cex_engine::startup::skipped_row(cx),
            },
            Pending::Tickers { rows, invalid: _ } => {
                // Spot rows may contain null prices; they are outside this venue.
                match serde_json::from_str::<TickerName>(item) {
                    Ok(name) if cx.store.market(name.i).is_none() => {}
                    Err(_) => safe_cex_engine::startup::skipped_row(cx),
                    Ok(_) => match serde_json::from_str::<TickerRow>(item) {
                        Ok(row) => {
                            let mut ticker = Ticker {
                                id: row.i.into(),
                                symbol: row.i.into(),
                                ..Default::default()
                            };
                            row.apply(&mut ticker);
                            rows.push(ticker);
                        }
                        Err(_) => safe_cex_engine::startup::skipped_row(cx),
                    },
                }
            }
            Pending::Candles { rows, invalid, .. } => match serde_json::from_str::<CandleRow>(item)
            {
                Ok(row) => rows.push(row.candle()),
                Err(_) => *invalid = true,
            },
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Trade(request) = pending {
            self.trade_response(
                cx,
                request,
                if (200..300).contains(&response.status)
                    || serde_json::from_str::<RestEnvelope>(response.body)
                        .is_ok_and(|e| e.code != 0)
                {
                    Ok(response.body)
                } else {
                    Err("Crypto.com HTTP write failed")
                },
            );
            return;
        }
        let error = match serde_json::from_str::<RestEnvelope>(response.body) {
            Ok(e) if e.code != 0 => Some(format!("Crypto.com API Error {}: {}", e.code, e.message)),
            _ if !(200..300).contains(&response.status) => {
                Some(format!("Crypto.com HTTP error {}", response.status))
            }
            Ok(_) => None,
            Err(e) => Some(e.to_string()),
        };
        if let Some(error) = error {
            match pending {
                Pending::Account(request) => self.account_failed(cx, request, &error, false),
                Pending::History(request) => self.history_failed(cx, request, &error, false),
                pending => self.on_request_failed(cx, pending, &error),
            }
            return;
        }
        match pending {
            Pending::Trade(_) => unreachable!(),
            Pending::History(request) => self.history_response(cx, request),
            Pending::Account(request) => self.account_response(cx, request),
            Pending::Markets {
                rows,
                indexes,
                invalid,
            } => {
                if invalid || rows.is_empty() {
                    cx.log(LogLevel::Warning, "invalid instrument row");
                    self.discovery.failed(cx, false);
                    return;
                }
                self.indexes = indexes;
                cx.store.set_markets(rows);
                self.discovery.succeeded(cx, &false);
                self.discovery_read(cx, true);
                if self.credentials.is_some() {
                    self.private_start(cx);
                } else {
                    cx.store.set_balance(Balance::default());
                    cx.store.set_positions(vec![]);
                    cx.store.set_orders(vec![]);
                }
            }
            Pending::Tickers { rows, invalid } => {
                if invalid || rows.is_empty() {
                    self.discovery.failed(cx, true);
                    return;
                }
                self.discovery.succeeded(cx, &true);
                cx.store.set_tickers(rows);
                self.market_channels(cx);
                self.connect(cx, 0);
            }
            Pending::Candles {
                command,
                rows,
                invalid,
            } => cx.done(
                command,
                if invalid {
                    Err("invalid candle row".into())
                } else {
                    Ok(serde_json::to_string(&rows).unwrap())
                },
            ),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Trade(request) => self.trade_response(cx, request, Err(error)),
            Pending::History(request) => self.history_failed(cx, request, error, true),
            Pending::Account(request) => self.account_failed(cx, request, error, true),
            Pending::Candles { command, .. } => cx.done(command, Err(error.into())),
            Pending::Markets { .. } => self.discovery.failed(cx, false),
            Pending::Tickers { .. } => self.discovery.failed(cx, true),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(tickers) = self.discovery.on_timer(timer) {
            self.discovery_read(cx, tickers);
            return;
        }
        if let Some(request) = self.history.waiting.remove(&timer) {
            self.history_request(cx, request);
            return;
        }
        if self.trading.timer == Some(timer) {
            self.trading.timer = None;
            self.drive_trading(cx);
            return;
        }
        if self.account.timer == Some(timer) {
            self.account.timer = None;
            self.reconcile(cx);
            return;
        }
        if self.watchdog == Some(timer) {
            for channel in 0..2 {
                let state = &self.sockets[channel];
                if state.open && cx.now - state.last >= 90_000.0 {
                    let socket = state.socket.unwrap();
                    cx.close(socket);
                    self.on_close(cx, socket, 1006);
                }
            }
            self.watchdog = Some(cx.set_timer(5_000.0));
            return;
        }
        for channel in 0..2 {
            if self.sockets[channel].reconnect == Some(timer) {
                self.sockets[channel].reconnect = None;
                self.connect(cx, channel);
                return;
            }
            if self.sockets[channel].ready == Some(timer) {
                self.sockets[channel].ready = None;
                if channel == 1 {
                    self.authenticate(cx);
                }
                if channel == 0 {
                    self.subscribe(
                        cx,
                        &self.subscriptions.keys().cloned().collect::<Vec<_>>(),
                        "subscribe",
                    );
                }
                return;
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                let state = self
                    .books
                    .entry(symbol.clone())
                    .or_insert_with(|| BookState {
                        book: Book::default(),
                        subscribers: 0,
                        synced: false,
                    });
                state.subscribers += 1;
                self.add_channel(cx, format!("book.{symbol}.50"));
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers -= 1;
                    if state.subscribers == 0 {
                        self.books.remove(&symbol);
                    }
                }
                self.remove_channel(cx, format!("book.{symbol}.50"));
            }
            Command::SubscribeCandles {
                symbol,
                interval: timeframe,
            } => match interval(&timeframe) {
                Ok(timeframe) => self.add_channel(cx, format!("candlestick.{timeframe}.{symbol}")),
                Err(error) => cx.log(LogLevel::Error, error),
            },
            Command::UnsubscribeCandles {
                symbol,
                interval: timeframe,
            } => {
                if let Ok(timeframe) = interval(&timeframe) {
                    self.remove_channel(cx, format!("candlestick.{timeframe}.{symbol}"));
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval: timeframe,
                from_ms,
                to_ms,
                ..
            } => match interval(&timeframe) {
                Ok(timeframe) => {
                    let mut path = format!(
                        "/public/get-candlestick?instrument_name={}&timeframe={timeframe}",
                        encode(&symbol)
                    );
                    if let Some(from) = from_ms {
                        path.push_str(&format!("&start_ts={from}"));
                    }
                    if let Some(to) = to_ms {
                        path.push_str(&format!("&end_ts={to}"));
                    }
                    self.get(
                        cx,
                        &path,
                        Pending::Candles {
                            command,
                            rows: vec![],
                            invalid: false,
                        },
                    );
                }
                Err(error) => cx.done(command, Err(error)),
            },
            Command::Query { command, query } => self.query(cx, command, query),
            command => self.trade_command(cx, command),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|s| s.synced).map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, s)| s.synced)
                .map(|(symbol, s)| (symbol.as_str(), &s.book)),
        )
    }
}

#[cfg(test)]
mod secondary_tests;
