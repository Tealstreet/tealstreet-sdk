//! Bybit V5 products and compatible HyroTrader accounts on the sans-I/O engine.
mod account;
mod history;
mod hyro;
mod paths;
mod sign;
mod trading;
mod wire;

use paths::Paths;
pub use paths::{Endpoints, Variant};
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue,
};
use sign::{Credentials, Query};
use std::collections::{HashMap, VecDeque};
use wire::*;

const MAX_RECONNECT_DELAY_MS: f64 = 30_000.0;
/// Every socket is checked this often. One quiet for `PING_AFTER_MS` is
/// pinged (Bybit answers on public and private sockets alike); one silent for
/// `SILENCE_LIMIT_MS` is treated as dead, since a half-open socket never closes.
const WATCHDOG_INTERVAL_MS: f64 = 5_000.0;
const PING_AFTER_MS: f64 = 10_000.0;
const SILENCE_LIMIT_MS: f64 = 20_000.0;
/// A book that skipped an update is unsubscribed, then subscribed again for a
/// fresh snapshot this long after.
const RESUBSCRIBE_DELAY_MS: f64 = 100.0;
/// Bybit closes a public socket that subscribes too fast: ten topics per
/// message, one message per pace interval.
const TOPICS_PER_SUBSCRIBE: usize = 10;
const SUBSCRIBE_PACE_MS: f64 = 10.0;
const MARKETS_PAGE_LIMIT: u32 = 1000;
const STARTUP_RETRY_MIN_MS: f64 = 1_000.0;
const STARTUP_RETRY_MAX_MS: f64 = 60_000.0;
const MARKETS_PATH: &[&str] = &["result", "list"];

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub variant: Variant,
    pub endpoints: Option<Endpoints>,
    pub testnet: bool,
    /// Markets, tickers, books and candles only; no account.
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}

/// One REST request as sent, kept so a rejected signature can be resent.
pub struct Rest {
    method: Method,
    path: &'static str,
    /// The query string of a GET, the JSON body of a POST.
    payload: String,
    stream: bool,
    signed: bool,
    resent: bool,
}

impl Rest {
    fn get(path: &'static str, query: Query) -> Self {
        Self {
            method: Method::Get,
            path,
            payload: query.into_string(),
            stream: false,
            signed: false,
            resent: false,
        }
    }

    /// A GET whose `result.list` streams in element by element.
    fn streamed(mut self) -> Self {
        self.stream = true;
        self
    }

    fn signed(mut self) -> Self {
        self.signed = true;
        self
    }

    fn post(path: &'static str, body: String) -> Self {
        Self {
            method: Method::Post,
            path,
            payload: body,
            stream: false,
            signed: true,
            resent: false,
        }
    }
}

/// What to do with a response. Startup lists and klines stream in element by
/// element (see `Cx::request_streamed`); their continuations collect the rows.
pub enum Then {
    Markets {
        prelaunch: bool,
        markets: Vec<Market>,
    },
    Fees(HashMap<String, (f64, f64)>),
    Tickers(Vec<Ticker>),
    Klines {
        command: CommandId,
        candles: Vec<Candle>,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
}

pub struct Pending {
    rest: Rest,
    then: Then,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Channel {
    Public,
    Private,
}

struct Connection {
    socket: Option<SocketId>,
    open: bool,
    attempts: u32,
    reconnect: Option<TimerId>,
    watchdog: Option<TimerId>,
    last_received: f64,
    last_ping: f64,
    /// Subscribe messages paced out one per `SUBSCRIBE_PACE_MS`.
    outbox: VecDeque<String>,
    outbox_timer: Option<TimerId>,
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
            last_ping: 0.0,
            outbox: VecDeque::new(),
            outbox_timer: None,
        }
    }
}

enum Sync {
    /// Subscribed; waiting for the snapshot.
    Waiting,
    Synced {
        last_update: u64,
    },
    /// Unsubscribed after a gap; subscribes again when the timer fires.
    Resubscribing(TimerId),
}

struct BookState {
    subscribers: u32,
    book: Book,
    sync: Sync,
}

/// The lists startup waits for before markets and tickers go into the store.
#[derive(Default)]
struct Startup {
    trading: Option<Vec<Market>>,
    prelaunch: Option<Vec<Market>>,
    /// Pages of a list whose later pages are still in flight.
    trading_pages: Vec<Market>,
    prelaunch_pages: Vec<Market>,
    fees: Option<HashMap<String, (f64, f64)>>,
    tickers: Option<Vec<Ticker>>,
    markets_loaded: bool,
    tickers_loaded: bool,
    trading_in_flight: bool,
    prelaunch_in_flight: bool,
    fees_in_flight: bool,
    tickers_in_flight: bool,
    account_info: bool,
    retry_ms: f64,
    retry: Option<TimerId>,
}

pub struct BybitLinear {
    variant: Variant,
    paths: Paths,
    config_error: Option<String>,
    credentials: Option<Credentials>,
    public: Connection,
    private: Connection,
    books: HashMap<String, BookState>,
    /// Kline subscriptions by (symbol, interval), counted.
    candles: HashMap<(String, String), u32>,
    startup: Startup,
    /// Server clock minus ours, learned from a `recv_window` rejection.
    clock_offset_ms: f64,
    account: account::AccountState,
    trading: trading::TradingState,
    history: history::HistoryState,
}

/// Bybit kline interval codes.
fn interval_code(interval: &str) -> Option<&'static str> {
    Some(match interval {
        "1m" => "1",
        "3m" => "3",
        "5m" => "5",
        "15m" => "15",
        "30m" => "30",
        "1h" => "60",
        "2h" => "120",
        "4h" => "240",
        "6h" => "360",
        "12h" => "720",
        "1d" => "D",
        "1w" => "W",
        "1M" => "M",
        _ => return None,
    })
}

/// Bybit's message for a rejected request, from any envelope or body.
pub(crate) fn venue_error(response: &Response<'_>) -> Option<(i64, String)> {
    let status = serde_json::from_str::<Status>(response.body).ok();
    match status.as_ref().and_then(|s| s.ret_code) {
        Some(0) if (200..300).contains(&response.status) => None,
        Some(code) => {
            let message = status
                .and_then(|s| s.ret_msg.map(|m| m.into_owned()))
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| format!("retCode {code}"));
            Some((code, message))
        }
        None => Some((
            i64::from(response.status),
            format!("HTTP {}: {}", response.status, response.body),
        )),
    }
}

/// A rejection that is safe to resend once, re-signed: a bad signature
/// (10004), or a timestamp outside the receive window. Bybit names its own
/// clock in the latter, which corrects ours.
fn resendable(text: &str) -> Option<Option<f64>> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("\"retcode\":10004") || lower.contains("error sign") {
        return Some(None);
    }
    if !lower.contains("recv_window") {
        return None;
    }
    let server = text
        .find("server_timestamp[")
        .map(|at| &text[at + 17..])
        .and_then(|rest| rest.split(']').next())
        .and_then(|digits| digits.parse::<f64>().ok());
    Some(server)
}

impl BybitLinear {
    pub fn new(options: Options) -> Self {
        let paths = Paths::for_options(&options);
        let config_error = options.validate().err();
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
            variant: options.variant,
            paths,
            config_error,
            credentials,
            public: Connection::new(),
            private: Connection::new(),
            books: HashMap::new(),
            candles: HashMap::new(),
            startup: Startup {
                retry_ms: STARTUP_RETRY_MIN_MS,
                prelaunch: (options.variant.is_spot() || options.variant.is_hyro()).then(Vec::new),
                fees: (options.variant.is_spot() || options.variant.is_hyro()).then(HashMap::new),
                ..Startup::default()
            },
            clock_offset_ms: 0.0,
            account: account::AccountState::default(),
            trading: trading::TradingState::default(),
            history: history::HistoryState::default(),
        }
    }

    fn connection(&mut self, channel: Channel) -> &mut Connection {
        match channel {
            Channel::Public => &mut self.public,
            Channel::Private => &mut self.private,
        }
    }

    fn channel_of(&self, socket: SocketId) -> Option<Channel> {
        if self.public.socket == Some(socket) {
            Some(Channel::Public)
        } else if self.private.socket == Some(socket) {
            Some(Channel::Private)
        } else {
            None
        }
    }

    /// Send one REST request; a signed one needs credentials.
    fn send(&self, cx: &mut Cx<'_, Pending>, rest: Rest, then: Then) {
        if self.config_error.is_some() {
            return;
        }
        let mut url = format!("{}{}", self.paths.rest, rest.path);
        if rest.method == Method::Get && !rest.payload.is_empty() {
            url.push('?');
            url.push_str(&rest.payload);
        }
        let headers = match (&self.credentials, rest.signed) {
            (Some(credentials), true) => {
                credentials.headers(cx.now + self.clock_offset_ms, &rest.payload)
            }
            (None, true) => return,
            _ => Vec::new(),
        };
        let body = (rest.method != Method::Get).then(|| rest.payload.clone());
        let (method, stream) = (rest.method, rest.stream);
        let pending = Pending { rest, then };
        if stream {
            cx.request_streamed(method, url, headers, ArrayAt::Path(MARKETS_PATH), pending);
        } else {
            cx.request(method, url, headers, body, pending);
        }
    }

    /// Resend a rejected request once with a fresh signature; false when it
    /// was already resent or the rejection is not one a resend can fix.
    fn resend(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: &mut Option<Pending>,
        text: &str,
    ) -> bool {
        let Some(server_time) = resendable(text) else {
            return false;
        };
        let Some(mut taken) = pending.take_if(|p| p.rest.signed && !p.rest.resent) else {
            return false;
        };
        if let Some(server) = server_time {
            self.clock_offset_ms = server - cx.now;
        }
        cx.log(
            LogLevel::Warning,
            format!("Bybit rejected a signed request; resending it once: {text}"),
        );
        taken.rest.resent = true;
        self.send(cx, taken.rest, taken.then);
        true
    }

    fn connect(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        let url = match channel {
            Channel::Public => self.paths.public_ws.as_str(),
            Channel::Private => self.paths.private_ws.as_str(),
        };
        let socket = cx.connect(url);
        let connection = self.connection(channel);
        connection.socket = Some(socket);
        connection.open = false;
    }

    fn watch(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        let timer = cx.set_timer(WATCHDOG_INTERVAL_MS);
        if let Some(previous) = self.connection(channel).watchdog.replace(timer) {
            cx.clear_timer(previous);
        }
    }

    /// Ping a quiet socket; close and reconnect a silent one.
    fn on_watchdog(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        let now = cx.now;
        let connection = self.connection(channel);
        connection.watchdog = None;
        let (Some(socket), true) = (connection.socket, connection.open) else {
            return;
        };
        let quiet = now - connection.last_received;
        if quiet > SILENCE_LIMIT_MS {
            cx.close(socket);
            self.on_socket_closed(cx, channel, 4000);
            return;
        }
        if quiet >= PING_AFTER_MS && now - connection.last_ping >= PING_AFTER_MS {
            connection.last_ping = now;
            cx.send(socket, r#"{"op":"ping"}"#);
        }
        self.watch(cx, channel);
    }

    fn on_socket_closed(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel, code: u16) {
        let connection = self.connection(channel);
        connection.socket = None;
        connection.open = false;
        connection.attempts += 1;
        connection.outbox.clear();
        let timers = [connection.watchdog.take(), connection.outbox_timer.take()];
        let delay = (250.0 * 2f64.powi(connection.attempts as i32)).min(MAX_RECONNECT_DELAY_MS);
        for timer in timers.into_iter().flatten() {
            cx.clear_timer(timer);
        }
        let timer = cx.set_timer(delay);
        self.connection(channel).reconnect = Some(timer);
        if channel == Channel::Private {
            self.on_private_closed(cx);
        }
        cx.log(
            LogLevel::Warning,
            format!("Bybit socket closed ({code}); reconnecting in {delay} ms"),
        );
    }

    /// Queue `op` messages for `topics`, ten per message, paced.
    fn subscribe(&mut self, cx: &mut Cx<'_, Pending>, op: &str, topics: &[String]) {
        let connection = &mut self.public;
        if connection.socket.is_none() || !connection.open {
            return;
        }
        for chunk in topics.chunks(TOPICS_PER_SUBSCRIBE) {
            let args = serde_json::to_string(chunk).unwrap_or_default();
            connection
                .outbox
                .push_back(format!(r#"{{"op":"{op}","args":{args}}}"#));
        }
        if connection.outbox_timer.is_none() {
            self.drain_outbox(cx);
        }
    }

    fn drain_outbox(&mut self, cx: &mut Cx<'_, Pending>) {
        let connection = &mut self.public;
        connection.outbox_timer = None;
        let (Some(socket), Some(message)) = (connection.socket, connection.outbox.pop_front())
        else {
            return;
        };
        cx.send(socket, message);
        if !connection.outbox.is_empty() {
            connection.outbox_timer = Some(cx.set_timer(SUBSCRIBE_PACE_MS));
        }
    }

    fn book_topic(depth: u32, symbol: &str) -> String {
        format!("orderbook.{depth}.{symbol}")
    }

    fn kline_topic(symbol: &str, interval: &str) -> Option<String> {
        Some(format!("kline.{}.{symbol}", interval_code(interval)?))
    }

    fn request_markets(&self, cx: &mut Cx<'_, Pending>, prelaunch: bool, cursor: Option<&str>) {
        let mut query = Query::new().str("category", self.paths.category);
        if !self.variant.is_spot() {
            query = query
                .num("limit", f64::from(MARKETS_PAGE_LIMIT))
                .opt_str("status", prelaunch.then_some("PreLaunch"))
                .opt_str("cursor", cursor);
        }
        let markets = Vec::new();
        self.send(
            cx,
            Rest::get("/v5/market/instruments-info", query).streamed(),
            Then::Markets { prelaunch, markets },
        );
    }

    fn request_startup(&mut self, cx: &mut Cx<'_, Pending>) {
        self.startup.retry = None;
        if !self.startup.markets_loaded
            && self.startup.trading.is_none()
            && !self.startup.trading_in_flight
        {
            self.startup.trading_in_flight = true;
            self.request_markets(cx, false, None);
        }
        if !self.startup.markets_loaded
            && self.startup.prelaunch.is_none()
            && !self.startup.prelaunch_in_flight
        {
            self.startup.prelaunch_in_flight = true;
            self.request_markets(cx, true, None);
        }
        if !self.startup.tickers_loaded
            && self.startup.tickers.is_none()
            && !self.startup.tickers_in_flight
        {
            self.startup.tickers_in_flight = true;
            let query = Query::new().str("category", self.paths.category);
            self.send(
                cx,
                Rest::get("/v5/market/tickers", query).streamed(),
                Then::Tickers(Vec::new()),
            );
        }
        if !self.startup.markets_loaded
            && self.credentials.is_some()
            && self.startup.fees.is_none()
            && !self.startup.fees_in_flight
        {
            self.startup.fees_in_flight = true;
            let query = Query::new().str("category", self.paths.category);
            self.send(
                cx,
                Rest::get("/v5/account/fee-rate", query).streamed().signed(),
                Then::Fees(HashMap::new()),
            );
        }
    }

    /// A required startup list failed: try the missing ones again, backing off.
    fn retry_startup(&mut self, cx: &mut Cx<'_, Pending>, what: &str, error: &str) {
        cx.log(
            LogLevel::Error,
            format!("Failed to fetch Bybit {what}: {error}"),
        );
        if self.startup.retry.is_some() {
            return;
        }
        let delay = self.startup.retry_ms;
        self.startup.retry_ms = (delay * 2.0).min(STARTUP_RETRY_MAX_MS);
        self.startup.retry = Some(cx.set_timer(delay));
    }

    fn on_markets(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        prelaunch: bool,
        mut markets: Vec<Market>,
        response: Result<Response<'_>, &str>,
    ) {
        let body = match response.map(|r| (venue_error(&r), r.body)) {
            Ok((None, body)) => body,
            Ok((Some((_, message)), _)) => return self.markets_failed(cx, prelaunch, &message),
            Err(error) => return self.markets_failed(cx, prelaunch, error),
        };
        let cursor = serde_json::from_str::<PageEnvelope>(body)
            .ok()
            .and_then(|page| page.result)
            .and_then(|result| result.next_page_cursor)
            .filter(|cursor| !cursor.is_empty())
            .map(|cursor| cursor.into_owned());
        let pages = if prelaunch {
            &mut self.startup.prelaunch_pages
        } else {
            &mut self.startup.trading_pages
        };
        pages.append(&mut markets);
        if let Some(cursor) = cursor.filter(|_| !self.variant.is_spot()) {
            self.request_markets(cx, prelaunch, Some(&cursor));
            return;
        }
        let list = std::mem::take(pages);
        if prelaunch {
            self.startup.prelaunch_in_flight = false;
            self.startup.prelaunch = Some(list);
        } else {
            self.startup.trading_in_flight = false;
            self.startup.trading = Some(list);
        }
        self.try_load_markets(cx);
    }

    /// Trading markets are required; PreLaunch markets are optional.
    fn markets_failed(&mut self, cx: &mut Cx<'_, Pending>, prelaunch: bool, error: &str) {
        if prelaunch {
            self.startup.prelaunch_in_flight = false;
            self.startup.prelaunch_pages.clear();
            cx.log(
                LogLevel::Warning,
                format!("Failed to fetch Bybit PreLaunch markets, continuing with Trading markets: {error}"),
            );
            self.startup.prelaunch = Some(Vec::new());
            self.try_load_markets(cx);
        } else {
            self.startup.trading_in_flight = false;
            self.startup.trading_pages.clear();
            self.retry_startup(cx, "Trading markets", error);
        }
    }

    fn try_load_markets(&mut self, cx: &mut Cx<'_, Pending>) {
        let startup = &mut self.startup;
        let fees_ready = self.credentials.is_none() || startup.fees.is_some();
        if startup.markets_loaded
            || startup.trading.is_none()
            || startup.prelaunch.is_none()
            || !fees_ready
        {
            return;
        }
        startup.markets_loaded = true;
        let fees = startup.fees.take().unwrap_or_default();
        let mut markets = startup.trading.take().unwrap_or_default();
        markets.append(&mut startup.prelaunch.take().unwrap_or_default());
        for market in &mut markets {
            if let Some(&(taker, maker)) = fees.get(&market.symbol) {
                market.taker_fee = Some(taker);
                market.maker_fee = Some(maker);
            }
        }
        cx.log(
            LogLevel::Info,
            format!("Loaded {} Bybit markets", markets.len()),
        );
        cx.store.set_markets(markets);
        self.try_load_tickers(cx);
    }

    fn try_load_tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.startup.markets_loaded || self.startup.tickers_loaded {
            return;
        }
        let Some(rows) = self.startup.tickers.take() else {
            return;
        };
        self.startup.tickers_loaded = true;
        let tickers: Vec<Ticker> = rows
            .into_iter()
            .filter_map(|mut ticker| {
                let market = cx.store.market(&ticker.symbol)?;
                if market.is_expiring == Some(true) {
                    ticker.funding_rate = 0.0;
                    ticker.funding_time = 0.0;
                }
                if self.variant.is_spot() {
                    ticker.funding_rate = 0.0;
                    ticker.funding_time = 0.0;
                }
                ticker.id = market.id.clone();
                Some(ticker)
            })
            .collect();
        cx.store.set_tickers(tickers);
        // Subscribe only after the seed exists: ticker deltas omit unchanged fields.
        self.connect(cx, Channel::Public);
        if self.credentials.is_none() {
            cx.store.set_orders(Vec::new());
            cx.store.set_positions(Vec::new());
            cx.store.set_balance(Default::default());
            return;
        }
        self.try_start_account(cx);
    }

    fn on_public_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Some(topic) = topic(text) else {
            if let Ok(frame) = serde_json::from_str::<OpFrame>(text)
                && frame.success == Some(false)
            {
                let op = frame.op.as_deref().unwrap_or_default();
                let message = frame.ret_msg.as_deref().unwrap_or_default();
                cx.log(LogLevel::Error, format!("[WS]{op}:{message}"));
            }
            return;
        };
        if topic.starts_with("tickers.") {
            if let Ok(event) = serde_json::from_str::<TickerEvent>(text) {
                on_ticker(cx, &event.data, self.variant);
            }
        } else if topic.starts_with("orderbook.") {
            if let Ok(event) = serde_json::from_str::<BookEvent>(text) {
                self.on_book(cx, event);
            }
        } else if topic.starts_with("kline.")
            && let Ok(event) = serde_json::from_str::<KlineEvent>(text)
        {
            self.on_kline(cx, &event);
        }
    }

    fn on_book(&mut self, cx: &mut Cx<'_, Pending>, event: BookEvent<'_>) {
        let BookEvent { kind, data } = event;
        let Some(state) = self.books.get_mut(data.s.as_ref()) else {
            return;
        };
        let levels = |side: &[[Str<'_>; 2]]| {
            side.iter()
                .map(|l| (float(&l[0]), float(&l[1])))
                .collect::<Vec<_>>()
        };
        match (kind.as_ref(), &state.sync) {
            (_, Sync::Resubscribing(_)) => {}
            ("snapshot", _) => {
                state.book.load_sorted(BookSide::Bids, levels(&data.b));
                state.book.load_sorted(BookSide::Asks, levels(&data.a));
                state.sync = Sync::Synced {
                    last_update: data.u,
                };
                cx.book_changed(&data.s);
            }
            ("delta", Sync::Synced { last_update }) => {
                let last = *last_update;
                if data.u <= last {
                    return;
                }
                if data.u > last + 1 {
                    // A skipped update: drop the book and subscribe again for a snapshot.
                    state.book.clear();
                    state.sync = Sync::Resubscribing(cx.set_timer(RESUBSCRIBE_DELAY_MS));
                    let topic = Self::book_topic(self.paths.book_depth, &data.s);
                    self.subscribe(cx, "unsubscribe", &[topic]);
                    return;
                }
                for (price, amount) in levels(&data.b) {
                    state.book.apply(BookSide::Bids, price, amount);
                }
                for (price, amount) in levels(&data.a) {
                    state.book.apply(BookSide::Asks, price, amount);
                }
                state.sync = Sync::Synced {
                    last_update: data.u,
                };
                cx.book_changed(&data.s);
            }
            _ => {}
        }
    }

    fn on_kline(&mut self, cx: &mut Cx<'_, Pending>, event: &KlineEvent<'_>) {
        let mut parts = event.topic.splitn(3, '.');
        let (Some(_), Some(code), Some(symbol)) = (parts.next(), parts.next(), parts.next()) else {
            return;
        };
        let Some(interval) = self
            .candles
            .keys()
            .find(|(s, i)| s == symbol && interval_code(i) == Some(code))
            .map(|(_, interval)| interval.clone())
        else {
            return;
        };
        for k in &event.data {
            cx.candle(
                symbol,
                &interval,
                Candle {
                    timestamp: k.start / 1000.0,
                    open: float(&k.open),
                    high: float(&k.high),
                    low: float(&k.low),
                    close: float(&k.close),
                    volume: float(&k.volume),
                },
            );
        }
    }

    fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let state = self
            .books
            .entry(symbol.clone())
            .or_insert_with(|| BookState {
                subscribers: 0,
                book: Book::default(),
                sync: Sync::Waiting,
            });
        state.subscribers += 1;
        if state.subscribers == 1 {
            self.subscribe(
                cx,
                "subscribe",
                &[Self::book_topic(self.paths.book_depth, &symbol)],
            );
        }
    }

    fn unsubscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        state.subscribers -= 1;
        if state.subscribers == 0 {
            if let Some(BookState {
                sync: Sync::Resubscribing(timer),
                ..
            }) = self.books.remove(symbol)
            {
                cx.clear_timer(timer);
            }
            self.subscribe(
                cx,
                "unsubscribe",
                &[Self::book_topic(self.paths.book_depth, symbol)],
            );
        }
    }

    fn on_candle_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        interval: String,
        add: bool,
    ) {
        let Some(topic) = Self::kline_topic(&symbol, &interval) else {
            cx.log(
                LogLevel::Warning,
                format!("Bybit has no {interval} candles"),
            );
            return;
        };
        let key = (symbol, interval);
        if add {
            let count = self.candles.entry(key).or_insert(0);
            *count += 1;
            if *count == 1 {
                self.subscribe(cx, "subscribe", &[topic]);
            }
        } else if let Some(count) = self.candles.get_mut(&key) {
            *count -= 1;
            if *count == 0 {
                self.candles.remove(&key);
                self.subscribe(cx, "unsubscribe", &[topic]);
            }
        }
    }

    fn fetch_candles(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: &str,
        interval: &str,
        (limit, from_ms, to_ms): (Option<u32>, Option<f64>, Option<f64>),
    ) {
        let Some(code) = interval_code(interval) else {
            cx.done(command, Err(format!("Bybit has no {interval} candles")));
            return;
        };
        let query = Query::new()
            .str("category", self.paths.category)
            .str("symbol", symbol)
            .str("interval", code)
            .opt_num("start", from_ms)
            .opt_num("end", to_ms)
            .num("limit", f64::from(limit.unwrap_or(500).min(1000)));
        let candles = Vec::new();
        self.send(
            cx,
            Rest::get("/v5/market/kline", query).streamed(),
            Then::Klines { command, candles },
        );
    }
}

fn on_item_public(then: &mut Then, item: &str, variant: Variant) {
    {
        match then {
            Then::Markets { markets, .. } => {
                if let Ok(m) = serde_json::from_str::<Instrument>(item)
                    && (!variant.is_hyro()
                        || (m.quote_coin == "USDT"
                            && (m.contract_type.is_empty()
                                || m.contract_type == "LinearPerpetual")))
                {
                    markets.push(market_from_wire(&m, variant));
                }
            }
            Then::Fees(fees) => {
                if let Ok(f) = serde_json::from_str::<FeeRate>(item) {
                    fees.insert(
                        f.symbol.into_owned(),
                        (float_of(&f.taker_fee_rate), float_of(&f.maker_fee_rate)),
                    );
                }
            }
            Then::Tickers(tickers) => {
                if let Ok(t) = serde_json::from_str::<TickerRow>(item) {
                    tickers.push(ticker_from_rest(&t));
                }
            }
            Then::Klines { candles, .. } => {
                if let Ok(KlineRow(row)) = serde_json::from_str::<KlineRow>(item)
                    && row.len() >= 6
                {
                    candles.push(Candle {
                        timestamp: float(&row[0]) / 1000.0,
                        open: float(&row[1]),
                        high: float(&row[2]),
                        low: float(&row[3]),
                        close: float(&row[4]),
                        volume: float(&row[5]),
                    });
                }
            }
            _ => {}
        }
    }
}

impl BybitLinear {
    fn on_public_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        then: Then,
        response: Result<Response<'_>, &str>,
    ) {
        let failure = match &response {
            Ok(response) => venue_error(response).map(|(_, message)| message),
            Err(error) => Some((*error).to_owned()),
        };
        match then {
            Then::Markets { prelaunch, markets } => {
                self.on_markets(cx, prelaunch, markets, response)
            }
            Then::Fees(fees) => {
                self.startup.fees_in_flight = false;
                if let Some(error) = failure {
                    cx.log(LogLevel::Warning, format!("Bybit fee rates: {error}"));
                }
                self.startup.fees = Some(fees);
                self.try_load_markets(cx);
            }
            Then::Tickers(tickers) => {
                self.startup.tickers_in_flight = false;
                match failure {
                    Some(error) => self.retry_startup(cx, "tickers", &error),
                    None => {
                        self.startup.tickers = Some(tickers);
                        self.try_load_tickers(cx);
                    }
                }
            }
            Then::Klines {
                command,
                mut candles,
            } => match failure {
                Some(error) => cx.done(command, Err(format!("Bybit klines: {error}"))),
                None => {
                    // Bybit lists newest first.
                    candles.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                    cx.done(
                        command,
                        Ok(serde_json::to_string(&candles).unwrap_or_default()),
                    );
                }
            },
            Then::Account(request) => self.on_account_response(cx, request, response),
            Then::Trade(request) => self.on_trade_response(cx, request, response),
            Then::History(request) => self.on_history_response(cx, request, response),
        }
    }
}

fn on_ticker(cx: &mut Cx<'_, Pending>, d: &TickerRow<'_>, variant: Variant) {
    let field = |value: &Option<Str<'_>>| present(value).map(float);
    let bid = field(&d.bid1_price);
    let ask = field(&d.ask1_price);
    let last = field(&d.last_price);
    let mark = field(&d.mark_price);
    let index = field(&d.index_price);
    let percentage = field(&d.price24h_pcnt).map(|p| p * 100.0);
    let open_interest = field(&d.open_interest);
    let funding_rate = if variant.is_spot() {
        Some(0.0)
    } else {
        field(&d.funding_rate)
    };
    let funding_time = if variant.is_spot() {
        Some(0.0)
    } else {
        field(&d.next_funding_time)
    };
    let volume = field(&d.volume24h);
    let quote_volume = field(&d.turnover24h);
    cx.store.update_ticker(&d.symbol, |t| {
        let set = |slot: &mut f64, value: Option<f64>| {
            if let Some(value) = value {
                *slot = value;
            }
        };
        set(&mut t.bid, bid);
        set(&mut t.ask, ask);
        set(&mut t.last, last);
        set(&mut t.mark, mark);
        set(&mut t.index, index);
        set(&mut t.percentage, percentage);
        set(&mut t.open_interest, open_interest);
        set(&mut t.funding_rate, funding_rate);
        set(&mut t.funding_time, funding_time);
        set(&mut t.volume, volume);
        set(&mut t.quote_volume, quote_volume);
    });
}

fn ticker_from_rest(t: &TickerRow<'_>) -> Ticker {
    let last = float_of(&t.last_price);
    let volume = float_of(&t.volume24h);
    Ticker {
        id: t.symbol.to_string(),
        symbol: t.symbol.to_string(),
        bid: float_of(&t.bid1_price),
        ask: float_of(&t.ask1_price),
        last,
        mark: float_of(&t.mark_price),
        index: float_of(&t.index_price),
        mid_price: None,
        percentage: float_of(&t.price24h_pcnt) * 100.0,
        open_interest: float_of(&t.open_interest),
        funding_rate: float_of(&t.funding_rate),
        funding_time: float_of(&t.next_funding_time),
        volume,
        // REST reports turnover too, but TS seeds quote volume as volume x last
        // and the stream then replaces it with turnover; kept identical.
        quote_volume: volume * last,
    }
}

fn market_from_wire(m: &Instrument<'_>, variant: Variant) -> Market {
    let lot = m.lot_size_filter.as_ref();
    let leverage = m.leverage_filter.as_ref();
    let expiring = !variant.is_spot()
        && m.delivery_time
            .as_deref()
            .map(float)
            .is_some_and(|t| t != 0.0 && !t.is_nan());
    Market {
        id: m.symbol.to_string(),
        symbol: m.symbol.to_string(),
        pretty_symbol: None,
        base: m.base_coin.to_string(),
        quote: m.quote_coin.to_string(),
        active: m.status == "Trading" || (!variant.is_spot() && m.status == "PreLaunch"),
        precision: Precision {
            amount: lot.map_or(f64::NAN, |l| {
                if variant.is_spot() {
                    present(&l.qty_step)
                        .or_else(|| present(&l.base_precision))
                        .map_or(f64::NAN, float)
                } else {
                    float_of(&l.qty_step)
                }
            }),
            price: m
                .price_filter
                .as_ref()
                .map_or(f64::NAN, |p| float_of(&p.tick_size)),
        },
        limits: Limits {
            amount: MinMax {
                min: lot.map_or(f64::NAN, |l| float_of(&l.min_order_qty)),
                max: lot.map_or(f64::NAN, |l| {
                    if variant.is_spot() {
                        [
                            &l.max_limit_order_qty,
                            &l.max_market_order_qty,
                            &l.max_order_qty,
                        ]
                        .into_iter()
                        .filter_map(|v| present(v).map(float))
                        .filter(|n| n.is_finite() && *n > 0.0)
                        .reduce(f64::min)
                        .unwrap_or(f64::NAN)
                    } else {
                        float_of(&l.max_order_qty)
                    }
                }),
            },
            leverage: MinMax {
                min: if variant.is_spot() {
                    1.0
                } else {
                    leverage.map_or(f64::NAN, |l| float_of(&l.min_leverage))
                },
                max: if variant.is_spot() {
                    1.0
                } else {
                    leverage.map_or(f64::NAN, |l| float_of(&l.max_leverage))
                },
            },
            price: None,
        },
        contract_size: None,
        is_expiring: Some(expiring),
        is_inverse: variant.is_inverse().then_some(true),
        is_spot: variant.is_spot().then_some(true),
        is_perpetual: Some(!variant.is_spot() && !expiring),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}

impl Venue for BybitLinear {
    type Pending = Pending;

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(error) = &self.config_error {
            cx.log(LogLevel::Error, error.clone());
            return;
        }
        self.request_startup(cx);
        if self.credentials.is_some() {
            self.request_account_info(cx);
        }
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
        connection.last_ping = now;
        self.watch(cx, channel);
        match channel {
            Channel::Public => {
                let mut topics: Vec<String> = cx
                    .store
                    .markets()
                    .iter()
                    .filter(|m| {
                        self.variant.is_inverse()
                            || self.variant.is_spot()
                            || matches!(m.quote.as_str(), "USDT" | "USDC")
                    })
                    .map(|m| format!("tickers.{}", m.symbol))
                    .collect();
                for (symbol, state) in &mut self.books {
                    state.book.clear();
                    if let Sync::Resubscribing(timer) = state.sync {
                        cx.clear_timer(timer);
                    }
                    state.sync = Sync::Waiting;
                    topics.push(Self::book_topic(self.paths.book_depth, symbol));
                }
                topics.extend(
                    self.candles
                        .keys()
                        .filter_map(|(symbol, interval)| Self::kline_topic(symbol, interval)),
                );
                self.subscribe(cx, "subscribe", &topics);
            }
            Channel::Private => self.on_private_open(cx, socket),
        }
    }

    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(channel) = self.channel_of(socket) else {
            return;
        };
        self.connection(channel).last_received = cx.now;
        match channel {
            Channel::Public => self.on_public_frame(cx, text),
            Channel::Private => self.on_private_frame(cx, text),
        }
    }

    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        if let Some(channel) = self.channel_of(socket) {
            self.on_socket_closed(cx, channel, code);
        }
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let mut pending = Some(pending);
        if self.resend(cx, &mut pending, response.body) {
            return;
        }
        let Some(Pending { then, .. }) = pending else {
            return;
        };
        self.on_public_response(cx, then, Ok(response));
    }

    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match &mut pending.then {
            Then::Account(request) => self.on_account_item(cx, request, item),
            then => on_item_public(then, item, self.variant),
        }
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        let mut pending = Some(pending);
        if self.resend(cx, &mut pending, error) {
            return;
        }
        let Some(Pending { then, .. }) = pending else {
            return;
        };
        self.on_public_response(cx, then, Err(error));
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        let mut resubscribe = Vec::new();
        for (symbol, state) in &mut self.books {
            if matches!(state.sync, Sync::Resubscribing(t) if t == timer) {
                state.sync = Sync::Waiting;
                resubscribe.push(Self::book_topic(self.paths.book_depth, symbol));
            }
        }
        if !resubscribe.is_empty() {
            self.subscribe(cx, "subscribe", &resubscribe);
        }
        if self.public.outbox_timer == Some(timer) {
            self.drain_outbox(cx);
        }
        if self.startup.retry == Some(timer) {
            self.request_startup(cx);
        }
        for channel in [Channel::Public, Channel::Private] {
            if self.connection(channel).reconnect == Some(timer) {
                self.connection(channel).reconnect = None;
                self.connect(cx, channel);
            }
            if self.connection(channel).watchdog == Some(timer) {
                self.on_watchdog(cx, channel);
            }
        }
        self.on_account_timer(cx, timer);
        self.on_trade_timer(cx, timer);
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => self.subscribe_book(cx, symbol),
            Command::UnsubscribeOrderBook { symbol } => self.unsubscribe_book(cx, &symbol),
            Command::SubscribeCandles { symbol, interval } => {
                self.on_candle_command(cx, symbol, interval, true)
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                self.on_candle_command(cx, symbol, interval, false)
            }
            Command::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => self.fetch_candles(cx, command, &symbol, &interval, (limit, from_ms, to_ms)),
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

    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, state)| matches!(state.sync, Sync::Synced { .. }))
                .map(|(symbol, state)| (symbol.as_str(), &state.book)),
        )
    }
}
