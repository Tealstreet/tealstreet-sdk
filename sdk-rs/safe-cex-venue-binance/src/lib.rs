//! Binance USD-M futures on the sans-I/O engine.
mod account;
mod history;
mod sign;
mod trading;
mod wire;

use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue, num,
};
use sign::{Credentials, Query};
use std::collections::HashMap;
use wire::*;

const MAX_RECONNECT_DELAY_MS: f64 = 30_000.0;
const DEPTH_SNAPSHOT_LIMIT: u32 = 1000;
/// Every socket is checked this often; one silent for `SILENCE_LIMIT_MS` is
/// treated as dead (a half-open socket never reports a close).
const WATCHDOG_INTERVAL_MS: f64 = 5_000.0;
const SILENCE_LIMIT_MS: f64 = 20_000.0;
/// Background reads pause at 80% of the 2,400 request-weight budget.
const WEIGHT_PAUSE_THRESHOLD: f64 = 1_920.0;
const WEIGHT_PAUSE_MS: f64 = 60_000.0;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub testnet: bool,
    /// Markets, tickers, books and candles only; no account.
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}

struct Urls {
    rest: &'static str,
    public_ws: &'static str,
    market_ws: &'static str,
    private_ws: &'static str,
}

impl Urls {
    fn for_options(options: &Options) -> Self {
        if options.testnet {
            Self {
                rest: "https://demo-fapi.binance.com",
                public_ws: "wss://demo-fstream.binance.com/public/ws",
                market_ws: "wss://demo-fstream.binance.com/market/ws",
                private_ws: "wss://demo-fstream.binance.com/private/ws",
            }
        } else {
            Self {
                rest: "https://fapi.binance.com",
                public_ws: "wss://fstream.binance.com/public/ws",
                market_ws: "wss://fstream.binance.com/market/ws",
                private_ws: "wss://fstream.binance.com/private/ws",
            }
        }
    }
}

/// Startup lists and klines stream in element by element (see
/// `Cx::request_streamed`); their continuations collect the decoded rows.
pub enum Pending {
    ExchangeInfo(Vec<Market>),
    Daily,
    BookTickers,
    PremiumIndex,
    DepthSnapshot {
        symbol: String,
        generation: u32,
    },
    Klines {
        command: CommandId,
        candles: Vec<Candle>,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Channel {
    Public,
    Market,
    Private,
}

struct Connection {
    socket: Option<SocketId>,
    open: bool,
    attempts: u32,
    reconnect: Option<TimerId>,
    watchdog: Option<TimerId>,
    last_received: f64,
    next_request_id: u64,
}

impl Connection {
    fn new() -> Self {
        Self {
            socket: None,
            open: false,
            attempts: 0,
            reconnect: None,
            watchdog: None,
            last_received: 0.0,
            next_request_id: 10,
        }
    }
}

/// A depth update held until the REST snapshot arrives.
struct BufferedDepth {
    first: u64,
    last: u64,
    previous: Option<u64>,
    bids: Vec<(f64, f64)>,
    asks: Vec<(f64, f64)>,
}

enum Sync {
    /// Waiting for the first diff before requesting a snapshot.
    Idle,
    Buffering(Vec<BufferedDepth>),
    /// `awaiting_first` until a diff has been applied on top of the snapshot:
    /// that first diff is checked by its id range, later ones by `pu`.
    Synced {
        last_applied: u64,
        awaiting_first: bool,
    },
    /// Backing off after a failed or discontinuous snapshot.
    Cooldown(TimerId),
}

struct BookState {
    subscribers: u32,
    book: Book,
    sync: Sync,
    generation: u32,
    failures: u32,
}

/// Diffs buffered while a snapshot is in flight; beyond this the snapshot is stale.
const MAX_BUFFERED_DIFFS: usize = 2_000;
const RATE_LIMIT_PAUSE_MS: f64 = 10_000.0;
const IP_BAN_PAUSE_MS: f64 = 120_000.0;
const MAX_SNAPSHOT_BACKOFF_MS: f64 = 60_000.0;

/// Tickers filled in place, one per market, by the three REST lists as they
/// stream in. A market becomes a ticker once all three have reported it.
#[derive(Default)]
struct TickerSeed {
    tickers: Vec<Ticker>,
    /// Which sources have reported each market.
    reported: Vec<u8>,
    /// Which sources have finished.
    finished: u8,
    failed: bool,
}

mod source {
    pub const DAILY: u8 = 1;
    pub const BOOK: u8 = 2;
    pub const PREMIUM: u8 = 4;
    pub const ALL: u8 = DAILY | BOOK | PREMIUM;
}

pub struct BinanceFutures {
    options: Options,
    urls: Urls,
    credentials: Option<Credentials>,
    public: Connection,
    market: Connection,
    private: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, String), u32>,
    ticker_seed: TickerSeed,
    /// Depth snapshots are paused until this time after a 418 or 429.
    rest_paused_until: f64,
    /// Background account reads pause until this time near the weight limit.
    background_paused_until: f64,
    account: account::AccountState,
    trading: trading::TradingState,
    history: history::HistoryState,
}

impl BinanceFutures {
    pub fn new(options: Options) -> Self {
        let urls = Urls::for_options(&options);
        let credentials = match (&options.key, &options.secret) {
            (Some(key), Some(secret)) if !options.public_data_only && !key.is_empty() => {
                Some(Credentials {
                    key: key.clone(),
                    secret: secret.clone(),
                })
            }
            _ => None,
        };
        Self {
            options,
            urls,
            credentials,
            public: Connection::new(),
            market: Connection::new(),
            private: Connection::new(),
            books: HashMap::new(),
            candles: HashMap::new(),
            ticker_seed: TickerSeed::default(),
            rest_paused_until: 0.0,
            background_paused_until: 0.0,
            account: account::AccountState::default(),
            trading: trading::TradingState::default(),
            history: history::HistoryState::default(),
        }
    }

    fn connection(&mut self, channel: Channel) -> &mut Connection {
        match channel {
            Channel::Public => &mut self.public,
            Channel::Market => &mut self.market,
            Channel::Private => &mut self.private,
        }
    }

    fn channel_of(&self, socket: SocketId) -> Option<Channel> {
        [Channel::Public, Channel::Market, Channel::Private]
            .into_iter()
            .find(|&channel| self.connection_ref(channel).socket == Some(socket))
    }

    fn connection_ref(&self, channel: Channel) -> &Connection {
        match channel {
            Channel::Public => &self.public,
            Channel::Market => &self.market,
            Channel::Private => &self.private,
        }
    }

    fn get(&self, cx: &mut Cx<'_, Pending>, path: &str, pending: Pending) {
        cx.request(
            Method::Get,
            format!("{}{}", self.urls.rest, path),
            Vec::new(),
            None,
            pending,
        );
    }

    fn get_streamed(&self, cx: &mut Cx<'_, Pending>, path: &str, at: ArrayAt, pending: Pending) {
        cx.request_streamed(
            Method::Get,
            format!("{}{}", self.urls.rest, path),
            Vec::new(),
            at,
            pending,
        );
    }

    /// A signed request; every parameter travels in the query string.
    fn signed(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        query: Query,
        pending: Pending,
    ) {
        self.signed_request(cx, method, path, query, None, pending);
    }

    /// A signed GET whose array streams in element by element.
    fn signed_streamed(
        &self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        query: Query,
        pending: Pending,
    ) {
        self.signed_request(cx, Method::Get, path, query, Some(ArrayAt::Root), pending);
    }

    fn signed_request(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        query: Query,
        stream: Option<ArrayAt>,
        pending: Pending,
    ) {
        let Some(credentials) = &self.credentials else {
            return;
        };
        let query = query.signed(credentials, cx.now);
        let url = format!("{}{path}?{query}", self.urls.rest);
        let headers = vec![("X-MBX-APIKEY", credentials.key.clone())];
        match stream {
            Some(at) => cx.request_streamed(method, url, headers, at, pending),
            None => cx.request(method, url, headers, None, pending),
        };
    }

    fn connect_url(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel, url: &str) {
        let socket = cx.connect(url);
        let connection = self.connection(channel);
        connection.socket = Some(socket);
        connection.open = false;
    }

    fn connect(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        let url = match channel {
            Channel::Public => self.urls.public_ws,
            Channel::Market => self.urls.market_ws,
            Channel::Private => return self.request_listen_key(cx),
        };
        self.connect_url(cx, channel, url);
    }

    /// Note request weight; near the limit, or on a 418/429, pause background reads.
    fn observe_weight(&mut self, cx: &mut Cx<'_, Pending>, response: &Response<'_>) {
        let pause = if matches!(response.status, 418 | 429) {
            Some(
                response
                    .header("retry-after")
                    .map(num::parse)
                    .filter(|seconds| *seconds > 0.0)
                    .map_or(WEIGHT_PAUSE_MS, |seconds| seconds * 1_000.0),
            )
        } else {
            response
                .header("x-mbx-used-weight-1m")
                .map(num::parse)
                .filter(|weight| *weight >= WEIGHT_PAUSE_THRESHOLD)
                .map(|_| WEIGHT_PAUSE_MS)
        };
        if let Some(pause) = pause {
            self.background_paused_until = self.background_paused_until.max(cx.now + pause);
        }
    }

    fn background_paused(&self, now: f64) -> bool {
        now < self.background_paused_until
    }

    fn watch(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        let timer = cx.set_timer(WATCHDOG_INTERVAL_MS);
        let connection = self.connection(channel);
        if let Some(previous) = connection.watchdog.replace(timer) {
            cx.clear_timer(previous);
        }
    }

    /// A socket silent too long is closed and reconnected; the private socket
    /// is pinged first because it can be quiet for good reason.
    fn on_watchdog(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        let now = cx.now;
        let connection = self.connection(channel);
        connection.watchdog = None;
        let (Some(socket), true) = (connection.socket, connection.open) else {
            return;
        };
        if now - connection.last_received > SILENCE_LIMIT_MS {
            cx.close(socket);
            self.on_socket_closed(cx, channel, 4000);
            return;
        }
        if channel == Channel::Private {
            cx.send(socket, "{\"id\":42,\"method\":\"LIST_SUBSCRIPTIONS\"}");
        }
        self.watch(cx, channel);
    }

    fn on_socket_closed(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel, code: u16) {
        let connection = self.connection(channel);
        connection.socket = None;
        connection.open = false;
        connection.attempts += 1;
        if let Some(watchdog) = connection.watchdog.take() {
            cx.clear_timer(watchdog);
        }
        let delay = (250.0 * 2f64.powi(connection.attempts as i32)).min(MAX_RECONNECT_DELAY_MS);
        let timer = cx.set_timer(delay);
        self.connection(channel).reconnect = Some(timer);
        if channel == Channel::Private {
            self.account.resync_orders_on_open = true;
        }
        cx.log(
            LogLevel::Warning,
            format!("Binance socket closed ({code}); reconnecting in {delay} ms"),
        );
    }

    fn send_subscribe(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        channel: Channel,
        method: &str,
        params: &[String],
    ) {
        let connection = self.connection(channel);
        let (Some(socket), true) = (connection.socket, connection.open) else {
            return;
        };
        if params.is_empty() {
            return;
        }
        connection.next_request_id += 1;
        let id = connection.next_request_id;
        let params = params
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(",");
        cx.send(
            socket,
            format!("{{\"method\":\"{method}\",\"params\":[{params}],\"id\":{id}}}"),
        );
    }

    fn depth_topics(&self) -> Vec<String> {
        self.books
            .keys()
            .map(|symbol| format!("{}@depth", symbol.to_lowercase()))
            .collect()
    }

    fn kline_topics(&self) -> Vec<String> {
        self.candles
            .keys()
            .map(|(symbol, interval)| format!("{}@kline_{interval}", symbol.to_lowercase()))
            .collect()
    }

    fn on_markets(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        markets: Vec<Market>,
        response: Response<'_>,
    ) {
        if response.status != 200 {
            cx.log(
                LogLevel::Error,
                format!(
                    "Binance exchangeInfo HTTP {}: {}",
                    response.status, response.body
                ),
            );
            return;
        }
        self.ticker_seed = TickerSeed {
            tickers: markets
                .iter()
                .map(|m| Ticker {
                    id: m.id.clone(),
                    symbol: m.symbol.clone(),
                    ..Ticker::default()
                })
                .collect(),
            reported: vec![0; markets.len()],
            finished: 0,
            failed: false,
        };
        cx.store.set_markets(markets);
        if self.credentials.is_some() {
            self.request_leverage_brackets(cx);
        }
        self.get_streamed(cx, "/fapi/v1/ticker/24hr", ArrayAt::Root, Pending::Daily);
        self.get_streamed(
            cx,
            "/fapi/v1/ticker/bookTicker",
            ArrayAt::Root,
            Pending::BookTickers,
        );
        self.get_streamed(
            cx,
            "/fapi/v1/premiumIndex",
            ArrayAt::Root,
            Pending::PremiumIndex,
        );
    }

    /// Fill one market's ticker from one source.
    fn seed(
        &mut self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        from: u8,
        fill: impl FnOnce(&mut Ticker),
    ) {
        let Some(index) = cx.store.market_index(symbol) else {
            return;
        };
        let seed = &mut self.ticker_seed;
        if let Some(ticker) = seed.tickers.get_mut(index as usize) {
            fill(ticker);
            seed.reported[index as usize] |= from;
        }
    }

    /// One source finished (or failed: TS then loads no tickers at all).
    fn source_finished(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        from: u8,
        response: Result<Response<'_>, &str>,
    ) {
        let failure = match response {
            Ok(response) if response.status == 200 => None,
            Ok(response) => Some(format!("HTTP {}: {}", response.status, response.body)),
            Err(error) => Some(error.to_owned()),
        };
        if let Some(failure) = failure {
            cx.log(LogLevel::Error, format!("Binance tickers: {failure}"));
            self.ticker_seed.failed = true;
        }
        self.ticker_seed.finished |= from;
        self.try_seed_tickers(cx);
    }

    fn try_seed_tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.ticker_seed.finished != source::ALL {
            return;
        }
        let seed = std::mem::take(&mut self.ticker_seed);
        let tickers: Vec<Ticker> = if seed.failed {
            Vec::new()
        } else {
            seed.tickers
                .into_iter()
                .zip(seed.reported)
                .filter(|(_, reported)| *reported == source::ALL)
                .map(|(ticker, _)| ticker)
                .collect()
        };
        cx.log(
            LogLevel::Info,
            format!(
                "Loaded {} Binance markets",
                tickers.len().min(cx.store.markets().len())
            ),
        );
        cx.store.set_tickers(tickers);
        if self.credentials.is_some() {
            self.start_account(cx);
        } else {
            cx.store.set_orders(Vec::new());
            cx.store.set_positions(Vec::new());
            cx.store.set_balance(Default::default());
        }
        self.connect(cx, Channel::Public);
        self.connect(cx, Channel::Market);
    }

    fn on_public_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        match event_type(text) {
            Some("bookTicker") => {
                if let Ok(event) = serde_json::from_str::<BookTickerEvent>(text) {
                    let (bid, ask) = (num::parse(&event.b), num::parse(&event.a));
                    cx.store.update_ticker(&event.s, |t| {
                        t.bid = bid;
                        t.ask = ask;
                    });
                }
            }
            Some("depthUpdate") => {
                if let Ok(event) = serde_json::from_str::<DepthEvent>(text) {
                    self.on_depth(cx, event);
                }
            }
            _ => {}
        }
    }

    fn on_market_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        match event_type(text) {
            Some("24hrTicker") => {
                if let Ok(events) = serde_json::from_str::<Vec<TickerEvent>>(text) {
                    for event in events {
                        let last = num::parse(&event.c);
                        let percentage = num::parse(&event.percentage);
                        let volume = num::parse(&event.v);
                        let quote_volume = num::parse(&event.q);
                        cx.store.update_ticker(&event.s, |t| {
                            t.last = last;
                            t.percentage = percentage;
                            t.volume = volume;
                            t.quote_volume = quote_volume;
                        });
                    }
                }
            }
            Some("markPriceUpdate") => {
                if let Ok(events) = serde_json::from_str::<Vec<MarkPriceEvent>>(text) {
                    for event in events {
                        let mark = num::parse(&event.p);
                        let index = num::parse(&event.i);
                        let funding_rate = num::parse(&event.r);
                        cx.store.update_ticker(&event.s, |t| {
                            t.mark = mark;
                            t.index = index;
                            t.funding_rate = funding_rate;
                            t.funding_time = event.funding_time;
                        });
                    }
                }
            }
            Some("kline") => {
                if let Ok(event) = serde_json::from_str::<KlineEvent>(text) {
                    let k = &event.k;
                    if self
                        .candles
                        .contains_key(&(k.s.to_string(), k.i.to_string()))
                    {
                        cx.candle(&k.s, &k.i, kline_candle(k.t, &k.o, &k.h, &k.l, &k.c, &k.v));
                    }
                }
            }
            _ => {}
        }
    }

    fn on_depth(&mut self, cx: &mut Cx<'_, Pending>, event: DepthEvent<'_>) {
        let paused = cx.now < self.rest_paused_until;
        let Some(state) = self.books.get_mut(event.s.as_ref()) else {
            return;
        };
        match &mut state.sync {
            Sync::Cooldown(_) => {}
            Sync::Idle if paused => {}
            Sync::Idle => {
                state.sync = Sync::Buffering(vec![buffered(&event)]);
                state.generation += 1;
                let generation = state.generation;
                let symbol = event.s.to_string();
                let path = format!("/fapi/v1/depth?symbol={symbol}&limit={DEPTH_SNAPSHOT_LIMIT}");
                self.get(cx, &path, Pending::DepthSnapshot { symbol, generation });
            }
            Sync::Buffering(updates) if updates.len() >= MAX_BUFFERED_DIFFS => {
                state.sync = Sync::Idle;
            }
            Sync::Buffering(updates) => updates.push(buffered(&event)),
            Sync::Synced {
                last_applied,
                awaiting_first,
            } => {
                let last = *last_applied;
                if event.final_update_id <= last {
                    return;
                }
                let continuous = if *awaiting_first {
                    event.first_update_id <= last + 1
                } else {
                    event.pu == Some(last)
                };
                if !continuous {
                    // Gap: drop the book; the next diff starts a fresh snapshot.
                    state.book.clear();
                    state.sync = Sync::Idle;
                    return;
                }
                for level in &event.b {
                    state
                        .book
                        .apply(BookSide::Bids, num::parse(&level[0]), num::parse(&level[1]));
                }
                for level in &event.a {
                    state
                        .book
                        .apply(BookSide::Asks, num::parse(&level[0]), num::parse(&level[1]));
                }
                state.sync = Sync::Synced {
                    last_applied: event.final_update_id,
                    awaiting_first: false,
                };
                cx.book_changed(&event.s);
            }
        }
    }

    /// Back off this book's next snapshot attempt exponentially.
    fn snapshot_failed(cx: &mut Cx<'_, Pending>, state: &mut BookState, min_delay_ms: f64) {
        state.book.clear();
        state.failures += 1;
        let backoff = (1_000.0 * 2f64.powi(state.failures as i32 - 1)).min(MAX_SNAPSHOT_BACKOFF_MS);
        state.sync = Sync::Cooldown(cx.set_timer(backoff.max(min_delay_ms)));
    }

    fn on_depth_snapshot(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        generation: u32,
        status: u16,
        body: &str,
    ) {
        if matches!(status, 418 | 429) {
            let pause = if status == 418 {
                IP_BAN_PAUSE_MS
            } else {
                RATE_LIMIT_PAUSE_MS
            };
            self.rest_paused_until = self.rest_paused_until.max(cx.now + pause);
            cx.log(
                LogLevel::Warning,
                format!(
                    "Binance REST limited (HTTP {status}); pausing depth snapshots for {pause} ms"
                ),
            );
        }
        let pause_left = (self.rest_paused_until - cx.now).max(0.0);
        let Some(state) = self.books.get_mut(&symbol) else {
            return;
        };
        if state.generation != generation {
            return;
        }
        let snapshot = (status == 200)
            .then(|| serde_json::from_str::<DepthSnapshot>(body).ok())
            .flatten();
        let Some(snapshot) = snapshot else {
            Self::snapshot_failed(cx, state, pause_left);
            return;
        };
        let Sync::Buffering(updates) = std::mem::replace(&mut state.sync, Sync::Idle) else {
            return;
        };
        state.book.load_sorted(
            BookSide::Bids,
            snapshot
                .bids
                .iter()
                .map(|l| (num::parse(&l[0]), num::parse(&l[1]))),
        );
        state.book.load_sorted(
            BookSide::Asks,
            snapshot
                .asks
                .iter()
                .map(|l| (num::parse(&l[0]), num::parse(&l[1]))),
        );
        let mut last_applied = snapshot.last_update_id;
        let mut awaiting_first = true;
        for update in updates
            .into_iter()
            .filter(|u| u.last > snapshot.last_update_id)
        {
            let continuous = if awaiting_first {
                update.first <= last_applied + 1
            } else {
                update.previous == Some(last_applied)
            };
            if !continuous {
                Self::snapshot_failed(cx, state, pause_left);
                return;
            }
            awaiting_first = false;
            for (price, amount) in update.bids {
                state.book.apply(BookSide::Bids, price, amount);
            }
            for (price, amount) in update.asks {
                state.book.apply(BookSide::Asks, price, amount);
            }
            last_applied = update.last;
        }
        state.failures = 0;
        state.sync = Sync::Synced {
            last_applied,
            awaiting_first,
        };
        cx.book_changed(&symbol);
    }

    fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let state = self
            .books
            .entry(symbol.clone())
            .or_insert_with(|| BookState {
                subscribers: 0,
                book: Book::default(),
                sync: Sync::Idle,
                generation: 0,
                failures: 0,
            });
        state.subscribers += 1;
        if state.subscribers == 1 {
            let topic = format!("{}@depth", symbol.to_lowercase());
            self.send_subscribe(cx, Channel::Public, "SUBSCRIBE", &[topic]);
        }
    }

    fn unsubscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        state.subscribers -= 1;
        if state.subscribers == 0 {
            self.books.remove(symbol);
            let topic = format!("{}@depth", symbol.to_lowercase());
            self.send_subscribe(cx, Channel::Public, "UNSUBSCRIBE", &[topic]);
        }
    }

    fn on_klines(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        candles: Vec<Candle>,
        response: Response<'_>,
    ) {
        let Response { status, body, .. } = response;
        if status != 200 {
            cx.done(
                command,
                Err(format!("Binance klines HTTP {status}: {body}")),
            );
            return;
        }
        cx.done(
            command,
            Ok(serde_json::to_string(&candles).unwrap_or_default()),
        );
    }
}

fn filter_value<'a>(filter: Option<&Filter<'a>>, pick: fn(&Filter<'a>) -> Option<f64>) -> f64 {
    filter.and_then(pick).unwrap_or(f64::NAN)
}

fn market_from_wire(m: &SymbolInfo<'_>) -> Market {
    let filter = |kind: &str| m.filters.iter().find(|f| f.filter_type == kind);
    let price = filter("PRICE_FILTER");
    let lot = filter("LOT_SIZE");
    let market_lot = filter("MARKET_LOT_SIZE");
    let min_qty: fn(&Filter<'_>) -> Option<f64> = |f| f.min_qty.as_deref().map(num::parse);
    let max_qty: fn(&Filter<'_>) -> Option<f64> = |f| f.max_qty.as_deref().map(num::parse);
    Market {
        id: format!("{}/{}:{}", m.base_asset, m.quote_asset, m.margin_asset),
        symbol: m.symbol.to_string(),
        pretty_symbol: None,
        base: m.base_asset.to_string(),
        quote: m.quote_asset.to_string(),
        active: m.status == "TRADING",
        precision: Precision {
            amount: filter_value(lot, |f| f.step_size.as_deref().map(num::parse)),
            price: filter_value(price, |f| f.tick_size.as_deref().map(num::parse)),
        },
        limits: Limits {
            amount: MinMax {
                min: filter_value(lot, min_qty).max(filter_value(market_lot, min_qty)),
                max: filter_value(lot, max_qty).min(filter_value(market_lot, max_qty)),
            },
            leverage: MinMax { min: 1.0, max: 1.0 },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}

fn kline_candle(open_time_ms: f64, o: &str, h: &str, l: &str, c: &str, v: &str) -> Candle {
    Candle {
        timestamp: open_time_ms / 1000.0,
        open: num::parse(o),
        high: num::parse(h),
        low: num::parse(l),
        close: num::parse(c),
        volume: num::parse(v),
    }
}

fn buffered(event: &DepthEvent<'_>) -> BufferedDepth {
    let levels = |side: &[[std::borrow::Cow<'_, str>; 2]]| {
        side.iter()
            .map(|l| (num::parse(&l[0]), num::parse(&l[1])))
            .collect()
    };
    BufferedDepth {
        first: event.first_update_id,
        last: event.final_update_id,
        previous: event.pu,
        bids: levels(&event.b),
        asks: levels(&event.a),
    }
}

impl Venue for BinanceFutures {
    type Pending = Pending;

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.get_streamed(
            cx,
            "/fapi/v1/exchangeInfo",
            ArrayAt::Field("symbols"),
            Pending::ExchangeInfo(Vec::new()),
        );
    }

    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let Some(channel) = self.channel_of(socket) else {
            return;
        };
        let now = cx.now;
        let connection = self.connection(channel);
        connection.open = true;
        connection.attempts = 0;
        connection.last_received = now;
        self.watch(cx, channel);
        match channel {
            Channel::Public => {
                let mut topics = if self.options.public_data_only {
                    Vec::new()
                } else {
                    vec!["!bookTicker".to_owned()]
                };
                for state in self.books.values_mut() {
                    state.book.clear();
                    state.sync = Sync::Idle;
                }
                topics.extend(self.depth_topics());
                self.send_subscribe(cx, Channel::Public, "SUBSCRIBE", &topics);
            }
            Channel::Market => {
                let mut topics = vec!["!ticker@arr".to_owned(), "!markPrice@arr@1s".to_owned()];
                topics.extend(self.kline_topics());
                self.send_subscribe(cx, Channel::Market, "SUBSCRIBE", &topics);
            }
            Channel::Private => self.on_private_open(cx),
        }
    }

    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(channel) = self.channel_of(socket) else {
            return;
        };
        self.connection(channel).last_received = cx.now;
        match channel {
            Channel::Public => self.on_public_frame(cx, text),
            Channel::Market => self.on_market_frame(cx, text),
            Channel::Private => self.on_user_event(cx, text),
        }
    }

    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        if let Some(channel) = self.channel_of(socket) {
            self.on_socket_closed(cx, channel, code);
        }
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.observe_weight(cx, &response);
        let Response { status, body, .. } = response;
        match pending {
            Pending::ExchangeInfo(markets) => self.on_markets(cx, markets, response),
            Pending::Daily => self.source_finished(cx, source::DAILY, Ok(response)),
            Pending::BookTickers => self.source_finished(cx, source::BOOK, Ok(response)),
            Pending::PremiumIndex => self.source_finished(cx, source::PREMIUM, Ok(response)),
            Pending::DepthSnapshot { symbol, generation } => {
                self.on_depth_snapshot(cx, symbol, generation, status, body)
            }
            Pending::Klines { command, candles } => self.on_klines(cx, command, candles, response),
            Pending::Account(request) => self.on_account_response(cx, request, response),
            Pending::Trade(request) => self.on_trade_response(cx, request, Ok(response)),
            Pending::History(request) => self.on_history_response(cx, request, Ok(response)),
        }
    }

    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::ExchangeInfo(markets) => {
                if let Ok(m) = serde_json::from_str::<SymbolInfo>(item)
                    && matches!(&*m.contract_type, "PERPETUAL" | "TRADIFI_PERPETUAL")
                    && matches!(&*m.margin_asset, "USDT" | "USDC")
                {
                    markets.push(market_from_wire(&m));
                }
            }
            Pending::Daily => {
                if let Ok(d) = serde_json::from_str::<Daily>(item) {
                    self.seed(cx, &d.symbol, source::DAILY, |t| {
                        t.last = num::parse(&d.last_price);
                        t.percentage = num::parse(&d.price_change_percent);
                        t.volume = num::parse(&d.volume);
                        t.quote_volume = num::parse(&d.quote_volume);
                    });
                }
            }
            Pending::BookTickers => {
                if let Ok(b) = serde_json::from_str::<BookTickerRest>(item) {
                    self.seed(cx, &b.symbol, source::BOOK, |t| {
                        t.bid = num::parse(&b.bid_price);
                        t.ask = num::parse(&b.ask_price);
                    });
                }
            }
            Pending::PremiumIndex => {
                if let Ok(p) = serde_json::from_str::<PremiumIndex>(item) {
                    self.seed(cx, &p.symbol, source::PREMIUM, |t| {
                        t.mark = num::parse(&p.mark_price);
                        t.index = num::parse(&p.index_price);
                        t.funding_rate = num::parse(&p.last_funding_rate);
                        t.funding_time = p.next_funding_time;
                    });
                }
            }
            Pending::Klines { candles, .. } => {
                if let Ok(row) = serde_json::from_str::<KlineRow>(item) {
                    candles.push(kline_candle(row.0, &row.1, &row.2, &row.3, &row.4, &row.5));
                }
            }
            Pending::Account(request) => self.on_account_item(cx, request, item),
            _ => {}
        }
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Klines { command, .. } => cx.done(command, Err(error.to_owned())),
            Pending::DepthSnapshot { symbol, generation } => {
                if let Some(state) = self.books.get_mut(&symbol)
                    && state.generation == generation
                {
                    Self::snapshot_failed(cx, state, 0.0);
                }
            }
            Pending::Account(request) => self.on_account_failed(cx, request, error),
            Pending::Daily => self.source_finished(cx, source::DAILY, Err(error)),
            Pending::BookTickers => self.source_finished(cx, source::BOOK, Err(error)),
            Pending::PremiumIndex => self.source_finished(cx, source::PREMIUM, Err(error)),
            Pending::Trade(request) => self.on_trade_response(cx, request, Err(error)),
            Pending::History(request) => self.on_history_response(cx, request, Err(error)),
            _ => cx.log(
                LogLevel::Error,
                format!("Binance startup request failed: {error}"),
            ),
        }
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        for state in self.books.values_mut() {
            if matches!(state.sync, Sync::Cooldown(t) if t == timer) {
                state.sync = Sync::Idle;
            }
        }
        for channel in [Channel::Public, Channel::Market, Channel::Private] {
            if self.connection(channel).reconnect == Some(timer) {
                self.connection(channel).reconnect = None;
                self.connect(cx, channel);
            }
            if self.connection(channel).watchdog == Some(timer) {
                self.on_watchdog(cx, channel);
            }
        }
        self.on_account_timer(cx, timer);
        self.on_history_timer(cx, timer);
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => self.subscribe_book(cx, symbol),
            Command::UnsubscribeOrderBook { symbol } => self.unsubscribe_book(cx, &symbol),
            Command::SubscribeCandles { symbol, interval } => {
                let count = self
                    .candles
                    .entry((symbol.clone(), interval.clone()))
                    .or_insert(0);
                *count += 1;
                if *count == 1 {
                    let topic = format!("{}@kline_{interval}", symbol.to_lowercase());
                    self.send_subscribe(cx, Channel::Market, "SUBSCRIBE", &[topic]);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol, interval);
                if let Some(count) = self.candles.get_mut(&key) {
                    *count -= 1;
                    if *count == 0 {
                        self.candles.remove(&key);
                        let topic = format!("{}@kline_{}", key.0.to_lowercase(), key.1);
                        self.send_subscribe(cx, Channel::Market, "UNSUBSCRIBE", &[topic]);
                    }
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
                let mut path = format!(
                    "/fapi/v1/klines?symbol={symbol}&interval={interval}&limit={}",
                    limit.unwrap_or(500)
                );
                if let Some(from) = from_ms {
                    path.push_str(&format!("&startTime={}", num::format(from)));
                }
                if let Some(to) = to_ms {
                    path.push_str(&format!("&endTime={}", num::format(to)));
                }
                let candles = Vec::new();
                self.get_streamed(
                    cx,
                    &path,
                    ArrayAt::Root,
                    Pending::Klines { command, candles },
                );
            }
            Command::Query { command, query } => self.start_query(cx, command, query),
            trade => self.on_trade_command(cx, trade),
        }
    }

    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|state| matches!(state.sync, Sync::Synced { .. }))
            .map(|state| &state.book)
    }
}
