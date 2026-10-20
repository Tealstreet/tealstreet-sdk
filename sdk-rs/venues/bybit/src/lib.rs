//! Bybit V5 products and compatible HyroTrader accounts on the sans-I/O engine.
mod account;
mod history;
mod hyro;
mod options;
mod paths;
mod sign;
mod trading;
mod wire;

use paths::Paths;
pub use paths::{Category, Endpoints, Variant};
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{
    Candle, DerivativeProduct, InstrumentClassification, Limits, Market, MinMax, Precision,
    ProductKind, Ticker,
};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue,
};
use sign::{Credentials, Query};
use std::collections::{HashMap, HashSet, VecDeque};
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
const PRELAUNCH_STARTUP_GRACE_MS: f64 = 1_500.0;
const MARKETS_PATH: &[&str] = &["result", "list"];

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub variant: Variant,
    pub endpoints: Option<Endpoints>,
    pub testnet: bool,
    pub enable_inverse_markets: bool,
    pub enable_option_markets: bool,
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
    OptionSeed {
        base_coin: String,
        tickers: Vec<Ticker>,
    },
    Options(options::Request),
    Klines {
        command: CommandId,
        candles: Vec<Candle>,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
}

pub struct Pending {
    category: Category,
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
    prelaunch_grace: Option<TimerId>,
    fees_in_flight: bool,
    tickers_in_flight: bool,
    account_info: bool,
    option_tickers_pending: HashSet<String>,
    option_tickers: HashMap<String, Vec<Ticker>>,
    retry_ms: f64,
    retry: Option<TimerId>,
}

struct ProductState {
    category: Category,
    public: Connection,
    startup: Startup,
    account: account::AccountState,
}

pub struct BybitLinear {
    category: Category,
    primary_category: Category,
    products: Vec<ProductState>,
    catalogs: HashMap<Category, Vec<Market>>,
    ticker_seeds: HashMap<Category, Vec<Ticker>>,
    catalog_ready: bool,
    tickers_ready: bool,
    classification: HashMap<String, InstrumentClassification>,
    derivatives: HashMap<String, DerivativeProduct>,
    wallet: Option<account::Wallet>,
    orders_loaded: HashSet<Category>,
    positions_loaded: HashSet<Category>,
    variant: Variant,
    paths: Paths,
    config_error: Option<String>,
    credentials: Option<Credentials>,
    read_credentials: Option<safe_cex_engine::ReadCredentials>,
    public: Connection,
    private: Connection,
    books: HashMap<String, BookState>,
    book_levels: BookLevels,
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
        let category = match options.variant {
            Variant::Inverse => Category::Inverse,
            Variant::Spot => Category::Spot,
            _ => Category::Linear,
        };
        let mut products = Vec::new();
        if options.variant == Variant::Linear {
            for (enabled, category) in [
                (options.enable_inverse_markets, Category::Inverse),
                (options.enable_option_markets, Category::Option),
            ] {
                if enabled {
                    products.push(ProductState {
                        category,
                        public: Connection::new(),
                        startup: Startup {
                            retry_ms: STARTUP_RETRY_MIN_MS,
                            prelaunch: (category == Category::Option).then(Vec::new),
                            fees: (category == Category::Option).then(HashMap::new),
                            ..Startup::default()
                        },
                        account: account::AccountState::default(),
                    });
                }
            }
        }
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
            category,
            primary_category: category,
            products,
            catalogs: HashMap::new(),
            ticker_seeds: HashMap::new(),
            catalog_ready: false,
            tickers_ready: false,
            classification: HashMap::new(),
            derivatives: HashMap::new(),
            wallet: None,
            orders_loaded: HashSet::new(),
            positions_loaded: HashSet::new(),
            variant: options.variant,
            paths,
            config_error,
            credentials,
            read_credentials: options.read_credentials,
            public: Connection::new(),
            private: Connection::new(),
            books: HashMap::new(),
            book_levels: BookLevels::default(),
            candles: HashMap::new(),
            startup: Startup {
                retry_ms: STARTUP_RETRY_MIN_MS,
                prelaunch: options.variant.is_spot().then(Vec::new),
                fees: (options.variant.is_spot() || options.variant.is_hyro()).then(HashMap::new),
                ..Startup::default()
            },
            clock_offset_ms: 0.0,
            account: account::AccountState::default(),
            trading: trading::TradingState::default(),
            history: history::HistoryState::default(),
        }
    }

    fn categories(&self) -> Vec<Category> {
        let mut categories = vec![self.category];
        categories.extend(self.products.iter().map(|state| state.category));
        categories.sort_by_key(|category| match category {
            Category::Linear => 0,
            Category::Inverse => 1,
            Category::Spot => 2,
            Category::Option => 3,
        });
        categories
    }

    fn select_category(&mut self, category: Category) {
        if category == self.category {
            return;
        }
        let state = self
            .products
            .iter_mut()
            .find(|state| state.category == category)
            .expect("enabled category");
        std::mem::swap(&mut self.category, &mut state.category);
        std::mem::swap(&mut self.public, &mut state.public);
        std::mem::swap(&mut self.startup, &mut state.startup);
        std::mem::swap(&mut self.account, &mut state.account);
        self.paths.category = self.category.as_str();
        if let Some((prefix, _)) = self.paths.public_ws.rsplit_once('/') {
            self.paths.public_ws = format!("{prefix}/{}", self.paths.category);
        }
        self.paths.book_depth = if self.category == Category::Option {
            25
        } else if self.category == Category::Spot {
            200
        } else {
            1000
        };
    }

    fn category_for(&self, symbol: &str) -> Category {
        if self.variant.is_spot() {
            Category::Spot
        } else {
            Category::for_symbol(symbol)
        }
    }

    fn category_enabled(&self, category: Category) -> bool {
        self.category == category || self.products.iter().any(|state| state.category == category)
    }

    fn owns_symbol(&self, symbol: &str) -> bool {
        self.category_for(symbol) == self.category
    }

    fn any_hedged(&self) -> bool {
        self.account.has_hedged() || self.products.iter().any(|state| state.account.has_hedged())
    }

    fn symbol_mode(&self, symbol: &str) -> Option<bool> {
        let category = self.category_for(symbol);
        if category == Category::Option || category == Category::Spot {
            return Some(false);
        }
        if category == self.category {
            self.account.position_modes.get(symbol).copied()
        } else {
            self.products
                .iter()
                .find(|state| state.category == category)?
                .account
                .position_modes
                .get(symbol)
                .copied()
        }
    }

    fn connection(&mut self, channel: Channel) -> &mut Connection {
        match channel {
            Channel::Public => &mut self.public,
            Channel::Private => &mut self.private,
        }
    }

    fn select_socket_category(&mut self, socket: SocketId) {
        let category = if self.private.socket == Some(socket) {
            Some(self.primary_category)
        } else if self.public.socket == Some(socket) {
            Some(self.category)
        } else {
            self.products
                .iter()
                .find(|state| state.public.socket == Some(socket))
                .map(|state| state.category)
        };
        if let Some(category) = category {
            self.select_category(category);
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
        let read_credentials = self
            .read_credentials
            .as_ref()
            .filter(|_| rest.method == Method::Get)
            .map(|read| Credentials {
                key: read.key.clone(),
                secret: read.secret.clone(),
            });
        let credentials = read_credentials.as_ref().or(self.credentials.as_ref());
        let headers = match (credentials, rest.signed) {
            (Some(credentials), true) => {
                credentials.headers(cx.now + self.clock_offset_ms, &rest.payload)
            }
            (None, true) => return,
            _ => vec![("X-Referer", sign::BROKER_ID.into())],
        };
        let body = (rest.method != Method::Get).then(|| rest.payload.clone());
        let (method, stream) = (rest.method, rest.stream);
        let pending = Pending {
            category: self.category,
            rest,
            then,
        };
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
        let socket = cx.connect_channel(
            url,
            match channel {
                Channel::Public => safe_cex_engine::SocketChannel::Public,
                Channel::Private => safe_cex_engine::SocketChannel::Private,
            },
        );
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
            let category = self.category;
            for other in self.categories() {
                self.select_category(other);
                self.on_private_closed(cx);
            }
            self.select_category(category);
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
        if self.category == Category::Option {
            query = query.str("baseCoin", "All");
        }
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
        if self.category == Category::Option && self.startup.markets_loaded {
            self.request_option_seeds(cx);
        }
        if self.category != Category::Option
            && !self.startup.tickers_loaded
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
        if prelaunch && (self.startup.markets_loaded || self.startup.prelaunch.is_some()) {
            return;
        }
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
            if let Some(timer) = self.startup.prelaunch_grace.take() {
                cx.clear_timer(timer);
            }
            self.startup.prelaunch_in_flight = false;
            self.startup.prelaunch = Some(list);
        } else {
            self.startup.trading_in_flight = false;
            self.startup.trading = Some(list);
            // The optional read gets its grace after Trading completes, as in
            // the TS helper. A slow Trading request must not consume that grace.
            if self.startup.prelaunch.is_none() && self.startup.prelaunch_grace.is_none() {
                self.startup.prelaunch_grace = Some(cx.set_timer(PRELAUNCH_STARTUP_GRACE_MS));
            }
        }
        self.try_load_markets(cx);
    }

    /// Trading markets are required; PreLaunch markets are optional.
    fn markets_failed(&mut self, cx: &mut Cx<'_, Pending>, prelaunch: bool, error: &str) {
        if prelaunch {
            if self.startup.markets_loaded || self.startup.prelaunch.is_some() {
                return;
            }
            if let Some(timer) = self.startup.prelaunch_grace.take() {
                cx.clear_timer(timer);
            }
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
        if self.variant.is_hyro() {
            let mut indices = HashMap::new();
            let mut unique = Vec::with_capacity(markets.len());
            for market in markets {
                if let Some(&index) = indices.get(&market.symbol) {
                    unique[index] = market;
                } else {
                    indices.insert(market.symbol.clone(), unique.len());
                    unique.push(market);
                }
            }
            markets = unique;
        }
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
        self.catalogs.insert(self.category, markets);
        if self.category == Category::Option {
            self.request_option_seeds(cx);
        }
        if self.catalogs.len() == self.products.len() + 1 {
            let mut markets = Vec::new();
            for category in self.categories() {
                markets.extend(self.catalogs.get(&category).into_iter().flatten().cloned());
            }
            cx.store.set_markets(markets);
            let classifications = cx
                .store
                .markets()
                .iter()
                .enumerate()
                .filter_map(|(index, market)| {
                    let mut row = self.classification.get(&market.symbol)?.clone();
                    row.market = index as u32;
                    Some(row)
                })
                .collect();
            cx.store.set_instrument_classifications(classifications);
            let derivatives = cx
                .store
                .markets()
                .iter()
                .enumerate()
                .filter_map(|(index, market)| {
                    let mut row = self.derivatives.get(&market.symbol)?.clone();
                    row.market = index as u32;
                    Some(row)
                })
                .collect();
            cx.store.set_derivative_products(derivatives);
            self.catalog_ready = true;
            let category = self.category;
            for other in self.categories() {
                self.select_category(other);
                self.try_load_tickers(cx);
            }
            self.select_category(category);
        }
    }

    fn request_option_seeds(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.startup.tickers_loaded || self.startup.tickers.is_some() {
            return;
        }
        let mut bases = std::collections::BTreeSet::new();
        for market in self.catalogs.get(&Category::Option).into_iter().flatten() {
            bases.insert(market.base.clone());
        }
        if bases.is_empty() {
            self.startup.tickers = Some(Vec::new());
            self.try_load_tickers(cx);
            return;
        }
        for base_coin in bases {
            if self.startup.option_tickers.contains_key(&base_coin)
                || !self
                    .startup
                    .option_tickers_pending
                    .insert(base_coin.clone())
            {
                continue;
            }
            let query = Query::new()
                .str("category", "option")
                .str("baseCoin", &base_coin);
            self.send(
                cx,
                Rest::get("/v5/market/tickers", query).streamed(),
                Then::OptionSeed {
                    base_coin,
                    tickers: Vec::new(),
                },
            );
        }
    }

    fn try_load_tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.catalog_ready || !self.startup.markets_loaded || self.startup.tickers_loaded {
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
        self.ticker_seeds.insert(self.category, tickers);
        if self.ticker_seeds.len() != self.products.len() + 1 {
            return;
        }
        let mut tickers = Vec::new();
        for category in self.categories() {
            tickers.extend(
                self.ticker_seeds
                    .get(&category)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        }
        cx.store.set_tickers(tickers);
        self.tickers_ready = true;
        let category = self.category;
        for other in self.categories() {
            self.select_category(other);
            self.connect(cx, Channel::Public);
            if self.credentials.is_some() {
                self.try_start_account(cx);
            }
        }
        self.select_category(category);
        if self.credentials.is_none() {
            cx.store.set_orders(Vec::new());
            cx.store.set_positions(Vec::new());
            cx.store.set_balance(Default::default());
        }
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
        if topic
            .rsplit('.')
            .next()
            .is_some_and(|symbol| !self.owns_symbol(symbol))
        {
            return;
        }
        if topic.starts_with("tickers.") {
            if let Ok(event) = serde_json::from_str::<TickerEvent>(text) {
                on_ticker(cx, &event.data, self.variant);
            }
        } else if topic.starts_with("orderbook.") {
            if let Ok(event) = self.book_levels.decode(text) {
                self.on_book(cx, event);
            }
        } else if topic.starts_with("kline.")
            && let Ok(event) = serde_json::from_str::<KlineEvent>(text)
        {
            self.on_kline(cx, &event);
        }
    }

    fn on_book(&mut self, cx: &mut Cx<'_, Pending>, event: BookMessage<'_>) {
        let BookMessage {
            kind,
            symbol,
            update,
        } = event;
        let Some(state) = self.books.get_mut(symbol.as_ref()) else {
            return;
        };
        match (kind.as_ref(), &state.sync) {
            (_, Sync::Resubscribing(_)) => {}
            ("snapshot", _) => {
                state
                    .book
                    .load_sorted(BookSide::Bids, self.book_levels.bids.iter().copied());
                state
                    .book
                    .load_sorted(BookSide::Asks, self.book_levels.asks.iter().copied());
                state.sync = Sync::Synced {
                    last_update: update,
                };
                cx.book_changed(&symbol);
            }
            ("delta", Sync::Synced { last_update }) => {
                let last = *last_update;
                if update <= last {
                    return;
                }
                if update > last + 1 {
                    // A skipped update: drop the book and subscribe again for a snapshot.
                    state.book.clear();
                    state.sync = Sync::Resubscribing(cx.set_timer(RESUBSCRIBE_DELAY_MS));
                    let topic = Self::book_topic(self.paths.book_depth, &symbol);
                    self.subscribe(cx, "unsubscribe", &[topic]);
                    return;
                }
                for (price, amount) in self.book_levels.bids.iter().copied() {
                    state.book.apply(BookSide::Bids, price, amount);
                }
                for (price, amount) in self.book_levels.asks.iter().copied() {
                    state.book.apply(BookSide::Asks, price, amount);
                }
                state.sync = Sync::Synced {
                    last_update: update,
                };
                cx.book_changed(&symbol);
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
        if self.category_for(&symbol) == Category::Option
            || !self.category_enabled(self.category_for(&symbol))
        {
            cx.log(LogLevel::Warning, "Bybit has no candles for this product");
            return;
        }
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
        if self.category_for(symbol) == Category::Option {
            cx.done(command, Err("Bybit has no candles for options".into()));
            return;
        }
        if !self.category_enabled(self.category_for(symbol)) {
            cx.done(command, Err("Bybit product is not enabled".into()));
            return;
        }
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

fn on_item_public(then: &mut Then, item: &str, variant: Variant, category: Category) {
    {
        match then {
            Then::Markets { markets, .. } => {
                if let Ok(m) = serde_json::from_str::<Instrument>(item)
                    && (!variant.is_hyro()
                        || (m.quote_coin == "USDT"
                            && (m.contract_type.is_empty()
                                || m.contract_type == "LinearPerpetual")))
                    && (category != Category::Option
                        || (m.status == "Trading" && derivative_from_wire(&m, category).is_some()))
                {
                    markets.push(market_from_wire(&m, variant, category));
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
            Then::Tickers(tickers) | Then::OptionSeed { tickers, .. } => {
                if let Ok(t) = serde_json::from_str::<TickerRow>(item) {
                    tickers.push(ticker_from_rest(
                        &t,
                        variant == Variant::Linear && category == Category::Inverse,
                    ));
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
            Then::OptionSeed { base_coin, tickers } => {
                self.startup.option_tickers_pending.remove(&base_coin);
                if let Some(error) = failure {
                    self.retry_startup(cx, "option tickers", &error);
                } else {
                    self.startup.option_tickers.insert(base_coin, tickers);
                    let bases: HashSet<&str> = self
                        .catalogs
                        .get(&Category::Option)
                        .into_iter()
                        .flatten()
                        .map(|market| market.base.as_str())
                        .collect();
                    if bases
                        .iter()
                        .all(|base| self.startup.option_tickers.contains_key(*base))
                    {
                        self.startup.tickers = Some(
                            self.startup
                                .option_tickers
                                .values()
                                .flatten()
                                .cloned()
                                .collect(),
                        );
                        self.try_load_tickers(cx);
                    }
                }
            }
            Then::Options(request) => self.on_options_response(cx, request, response),
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

fn on_ticker(cx: &mut Cx<'_, Pending>, d: &TickerDelta<'_>, variant: Variant) {
    let percentage = d.price24h_pcnt.or(d.change24h).map(|p| p * 100.0);
    let open_interest = d.open_interest;
    let funding_rate = if variant.is_spot() {
        Some(0.0)
    } else {
        d.funding_rate
    };
    let funding_time = if variant.is_spot() {
        Some(0.0)
    } else {
        d.next_funding_time
    };
    let inverse =
        variant == Variant::Linear && Category::for_symbol(&d.symbol) == Category::Inverse;
    let volume = if inverse { d.turnover24h } else { d.volume24h };
    let quote_volume = if inverse { d.volume24h } else { d.turnover24h };
    cx.store.update_ticker(&d.symbol, |t| {
        let set = |slot: &mut f64, value: Option<f64>| {
            if let Some(value) = value {
                *slot = value;
            }
        };
        set(&mut t.bid, d.bid1_price.or(d.bid_price));
        set(&mut t.ask, d.ask1_price.or(d.ask_price));
        set(&mut t.last, d.last_price);
        set(&mut t.mark, d.mark_price);
        set(&mut t.index, d.index_price);
        set(&mut t.percentage, percentage);
        set(&mut t.open_interest, open_interest);
        set(&mut t.funding_rate, funding_rate);
        set(&mut t.funding_time, funding_time);
        set(&mut t.volume, volume);
        set(&mut t.quote_volume, quote_volume);
    });
}

fn ticker_from_rest(t: &TickerRow<'_>, inverse: bool) -> Ticker {
    let last = float_of(&t.last_price);
    let volume = if inverse {
        float_of(&t.turnover24h)
    } else {
        float_of(&t.volume24h)
    };
    Ticker {
        id: t.symbol.to_string(),
        symbol: t.symbol.to_string(),
        bid: float_of(&t.bid1_price),
        ask: float_of(&t.ask1_price),
        last,
        mark: float_of(&t.mark_price),
        index: float_of(&t.index_price),
        mid_price: None,
        percentage: present(&t.price24h_pcnt)
            .map_or_else(|| float_of(&t.change24h), |_| float_of(&t.price24h_pcnt))
            * 100.0,
        open_interest: float_of(&t.open_interest),
        funding_rate: float_of(&t.funding_rate),
        funding_time: float_of(&t.next_funding_time),
        volume,
        // REST reports turnover too, but TS seeds quote volume as volume x last
        // and the stream then replaces it with turnover; kept identical.
        quote_volume: if inverse {
            float_of(&t.volume24h)
        } else {
            volume * last
        },
    }
}

fn market_from_wire(m: &Instrument<'_>, variant: Variant, category: Category) -> Market {
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
                min: if variant.is_spot() || category == Category::Option {
                    1.0
                } else {
                    leverage.map_or(f64::NAN, |l| float_of(&l.min_leverage))
                },
                max: if variant.is_spot() || category == Category::Option {
                    1.0
                } else {
                    leverage.map_or(f64::NAN, |l| float_of(&l.max_leverage))
                },
            },
            price: if category == Category::Option {
                m.price_filter.as_ref().and_then(|price| {
                    let min = float_of(&price.min_price);
                    min.is_finite()
                        .then_some(safe_cex_engine::model::Min { min })
                })
            } else {
                None
            },
        },
        contract_size: (category == Category::Option).then_some(1.0),
        is_expiring: Some(expiring || category == Category::Option),
        is_inverse: (category == Category::Inverse).then_some(true),
        is_spot: variant.is_spot().then_some(true),
        is_perpetual: Some(!variant.is_spot() && category != Category::Option && !expiring),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}

fn derivative_from_wire(row: &Instrument<'_>, category: Category) -> Option<DerivativeProduct> {
    let kind = if category == Category::Option {
        match row.options_type.as_deref()? {
            "Call" => ProductKind::Call,
            "Put" => ProductKind::Put,
            _ => return None,
        }
    } else if row.contract_type.ends_with("Futures") {
        ProductKind::Future
    } else {
        return None;
    };
    let strike = if category == Category::Option {
        let strike = row
            .symbol
            .split('-')
            .nth(2)?
            .replace('d', ".")
            .parse::<f64>()
            .ok()?;
        if !strike.is_finite() || strike <= 0.0 {
            return None;
        }
        Some(strike)
    } else {
        None
    };
    Some(DerivativeProduct {
        market: 0,
        kind,
        underlying: row.base_coin.to_string(),
        expiry: present(&row.delivery_time)
            .map(float)
            .filter(|expiry| expiry.is_finite() && *expiry > 0.0),
        strike,
        settlement_asset: row.settle_coin.as_ref().map(ToString::to_string),
        premium_asset: (category == Category::Option).then(|| row.quote_coin.to_string()),
        style: (category == Category::Option).then(|| "european".into()),
        settlement: (category == Category::Option).then(|| "cash".into()),
    })
}

impl BybitLinear {
    fn on_category_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        let mut resubscribe = Vec::new();
        for (symbol, state) in &mut self.books {
            if !self.variant.is_spot() && Category::for_symbol(symbol) != self.category {
                continue;
            }
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
        if self.startup.prelaunch_grace == Some(timer) {
            self.startup.prelaunch_grace = None;
            self.startup.prelaunch_in_flight = false;
            self.startup.prelaunch_pages.clear();
            self.startup.prelaunch = Some(Vec::new());
            cx.log(
                LogLevel::Warning,
                "Timed out waiting for Bybit PreLaunch markets, continuing with Trading markets",
            );
            self.try_load_markets(cx);
        }
        for channel in [Channel::Public, Channel::Private] {
            if channel == Channel::Private && self.category != self.primary_category {
                continue;
            }
            if self.connection(channel).reconnect == Some(timer) {
                self.connection(channel).reconnect = None;
                self.connect(cx, channel);
            }
            if self.connection(channel).watchdog == Some(timer) {
                self.on_watchdog(cx, channel);
            }
        }
        self.on_account_timer(cx, timer);
        if self.category == self.primary_category {
            self.on_trade_timer(cx, timer);
        }
    }
}

impl Venue for BybitLinear {
    type Pending = Pending;

    fn supports_options(&self) -> bool {
        self.category_enabled(Category::Option)
    }

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.read_credentials
            && let Err(error) = read.validate()
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        if let Some(error) = &self.config_error {
            cx.log(LogLevel::Error, error.clone());
            return;
        }
        if self.category_enabled(Category::Option) {
            // Keep identity stable before an option position first receives Greeks.
            cx.store.retain_position_ids();
        }
        for category in self.categories() {
            self.select_category(category);
            self.request_startup(cx);
        }
        self.select_category(self.primary_category);
        if self.credentials.is_some() {
            self.request_account_info(cx);
        }
    }

    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.select_socket_category(socket);
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
                    .filter(|m| self.owns_symbol(&m.symbol))
                    .map(|m| format!("tickers.{}", m.symbol))
                    .collect();
                let category = self.category;
                let spot = self.variant.is_spot();
                for (symbol, state) in &mut self.books {
                    if !spot && Category::for_symbol(symbol) != category {
                        continue;
                    }
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
                        .filter(|(symbol, _)| self.owns_symbol(symbol))
                        .filter_map(|(symbol, interval)| Self::kline_topic(symbol, interval)),
                );
                self.subscribe(cx, "subscribe", &topics);
            }
            Channel::Private => self.on_private_open(cx, socket),
        }
    }

    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.select_socket_category(socket);
        let Some(channel) = self.channel_of(socket) else {
            return;
        };
        self.connection(channel).last_received = cx.now;
        match channel {
            Channel::Public => self.on_public_frame(cx, text),
            Channel::Private => {
                self.on_private_frame(cx, text);
                if topic(text).is_some_and(|topic| !topic.starts_with("wallet")) {
                    for category in self
                        .categories()
                        .into_iter()
                        .filter(|category| *category != self.primary_category)
                        .collect::<Vec<_>>()
                    {
                        self.select_category(category);
                        self.on_private_frame(cx, text);
                    }
                }
            }
        }
    }

    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.select_socket_category(socket);
        if let Some(channel) = self.channel_of(socket) {
            self.on_socket_closed(cx, channel, code);
        }
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.select_category(pending.category);
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
        self.select_category(pending.category);
        match &mut pending.then {
            Then::Account(request) => self.on_account_item(cx, request, item),
            Then::Options(request) => options::on_item(request, item),
            then => {
                if matches!(then, Then::Markets { .. })
                    && let Ok(row) = serde_json::from_str::<Instrument>(item)
                {
                    self.classification.insert(
                        row.symbol.to_string(),
                        InstrumentClassification {
                            market: 0,
                            symbol_type: row.symbol_type.as_ref().map(ToString::to_string),
                            market_region: row.market_region.as_ref().map(ToString::to_string),
                            underlying_ticker: row
                                .underlying_ticker
                                .as_ref()
                                .map(ToString::to_string),
                        },
                    );
                    if let Some(product) = derivative_from_wire(&row, self.category) {
                        self.derivatives.insert(row.symbol.to_string(), product);
                    }
                }
                on_item_public(then, item, self.variant, self.category)
            }
        }
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.select_category(pending.category);
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
        for category in self.categories() {
            self.select_category(category);
            self.on_category_timer(cx, timer);
        }
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let symbol = match &command {
            Command::SubscribeOrderBook { symbol }
            | Command::UnsubscribeOrderBook { symbol }
            | Command::SubscribeCandles { symbol, .. }
            | Command::UnsubscribeCandles { symbol, .. }
            | Command::FetchCandles { symbol, .. }
            | Command::CancelSymbolOrders { symbol, .. }
            | Command::SetLeverage { symbol, .. }
            | Command::SetMarginType { symbol, .. } => Some(symbol.as_str()),
            Command::AmendOrder { id, .. } => cx.store.order(id).map(|order| order.symbol.as_str()),
            Command::Query { query, .. } => match query {
                safe_cex_engine::Query::Fills { symbol, .. }
                | safe_cex_engine::Query::OrderHistory { symbol, .. }
                | safe_cex_engine::Query::Income { symbol, .. } => symbol.as_deref(),
                safe_cex_engine::Query::PublicTrades { symbol, .. }
                | safe_cex_engine::Query::OrderByClientId { symbol, .. } => Some(symbol.as_str()),
            },
            _ => None,
        };
        let category = symbol.map_or(self.primary_category, |symbol| self.category_for(symbol));
        if !self.category_enabled(category) {
            let id = match &command {
                Command::FetchCandles { command, .. }
                | Command::CancelSymbolOrders { command, .. }
                | Command::SetLeverage { command, .. }
                | Command::SetMarginType { command, .. }
                | Command::AmendOrder { command, .. }
                | Command::Query { command, .. } => Some(*command),
                _ => None,
            };
            if let Some(id) = id {
                cx.done(id, Err("Bybit product is not enabled".into()));
            } else {
                cx.log(LogLevel::Warning, "Bybit product is not enabled");
            }
            return;
        }
        self.select_category(category);
        match command {
            Command::FetchOptionChain { command, query } => {
                self.fetch_option_chain(cx, command, query)
            }
            Command::FetchOptionExpiries {
                command,
                underlying,
            } => self.fetch_option_expiries(cx, command, &underlying),
            Command::FetchAccountGreeks { command } => self.fetch_account_greeks(cx, command),
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
                .map(|(symbol, state)| (symbol.as_str(), &state.book)),
        )
    }
}

#[cfg(test)]
mod public_decoder_tests {
    use super::*;
    use safe_cex_engine::{Engine, Event};

    #[test]
    fn numeric_ticker_deltas_preserve_sparse_fields_and_product_conversions() {
        for (variant, category, symbol) in [
            (Variant::Linear, Category::Linear, "BTCUSDT"),
            (Variant::Linear, Category::Inverse, "BTCUSD"),
            (Variant::Linear, Category::Option, "BTC-27MAR26-100000-C"),
            (Variant::Inverse, Category::Inverse, "BTCUSD"),
            (Variant::Spot, Category::Spot, "BTCUSDT"),
            (Variant::Demo, Category::Linear, "BTCUSDT"),
            (Variant::Hyrotrader, Category::Linear, "BTCUSDT"),
        ] {
            let socket = SocketId(1);
            let mut venue = BybitLinear::new(Options {
                variant,
                enable_inverse_markets: category == Category::Inverse && variant == Variant::Linear,
                enable_option_markets: category == Category::Option,
                endpoints: variant.is_hyro().then(|| Endpoints {
                    rest: "https://hyro.example".into(),
                    public_ws: "wss://hyro.example/v5/public/linear".into(),
                    private_ws: "wss://hyro.example/v5/private".into(),
                }),
                public_data_only: true,
                ..Options::default()
            });
            venue.select_category(category);
            venue.public.socket = Some(socket);
            let mut engine = Engine::new(venue);
            engine.store_mut().set_tickers(vec![Ticker {
                id: symbol.into(),
                symbol: symbol.into(),
                bid: 9.0,
                ask: 11.0,
                last: 10.0,
                mark: 10.0,
                index: 10.0,
                percentage: 2.0,
                open_interest: 3.0,
                funding_rate: 0.01,
                funding_time: 123.0,
                volume: 4.0,
                quote_volume: 5.0,
                ..Ticker::default()
            }]);
            let delta = format!(
                r#"{{"topic":"tickers.{symbol}","data":{{"symbol":"{symbol}","lastPrice":"\u0031\u0032","bid1Price":null,"ask1Price":"","price24hPcnt":"0.015","volume24h":"30","turnover24h":"60","fundingRate":"0.02","nextFundingTime":"456"}}}}"#
            );
            engine.handle(
                0.0,
                Event::WsMessage {
                    socket,
                    text: &delta,
                },
            );
            let ticker = engine.store().ticker(symbol).unwrap();
            assert_eq!((ticker.bid, ticker.ask, ticker.last), (9.0, 11.0, 12.0));
            assert_eq!(
                (ticker.mark, ticker.index, ticker.open_interest),
                (10.0, 10.0, 3.0)
            );
            assert_eq!(ticker.percentage, 1.5);
            assert_eq!(
                (ticker.volume, ticker.quote_volume),
                if variant == Variant::Linear && category == Category::Inverse {
                    (60.0, 30.0)
                } else {
                    (30.0, 60.0)
                },
                "{variant:?}/{category:?}"
            );
            assert_eq!(
                (ticker.funding_rate, ticker.funding_time),
                if variant.is_spot() {
                    (0.0, 0.0)
                } else {
                    (0.02, 456.0)
                }
            );
            let before = ticker.clone();
            for data in [
                r#""lastPrice":"99","delta":false"#,
                r#""lastPrice":"99","lastPrice":"100""#,
                r#""lastPrice":"99","gamma":null,"gamma":null"#,
                r#""lastPrice":99"#,
            ] {
                let malformed = format!(
                    r#"{{"topic":"tickers.{symbol}","data":{{"symbol":"{symbol}",{data}}}}}"#
                );
                engine.handle(
                    1.0,
                    Event::WsMessage {
                        socket,
                        text: &malformed,
                    },
                );
                assert_eq!(engine.store().ticker(symbol), Some(&before), "{malformed}");
            }
            let whitespace = format!(
                r#"{{"topic":"tickers.{symbol}","data":{{"symbol":"{symbol}","lastPrice":" ","bid1Price":"bad"}}}}"#
            );
            engine.handle(
                2.0,
                Event::WsMessage {
                    socket,
                    text: &whitespace,
                },
            );
            let ticker = engine.store().ticker(symbol).unwrap();
            assert!(ticker.last.is_nan());
            assert!(ticker.bid.is_nan());
            assert_eq!(ticker.ask, 11.0);
        }
    }

    #[test]
    fn malformed_book_data_cannot_partially_change_the_book_or_advance_its_sequence() {
        let socket = SocketId(1);
        let mut venue = BybitLinear::new(Options {
            public_data_only: true,
            ..Options::default()
        });
        venue.public.socket = Some(socket);
        venue.books.insert(
            "BTCUSDT".into(),
            BookState {
                subscribers: 1,
                book: Book::default(),
                sync: Sync::Waiting,
            },
        );
        let mut engine = Engine::new(venue);
        engine.handle(0.0, Event::WsMessage { socket, text: r#"{"topic":"orderbook.1000.BTCUSDT","type":"snapshot","data":{"s":"BTCUSDT","b":[["100","1"]],"a":[["101","2"]],"u":1}}"# });
        for text in [
            r#"{"topic":"orderbook.1000.BTCUSDT","type":"delta","data":{"s":"BTCUSDT","b":[["100","99"]],"a":[["101",3]],"u":2}}"#,
            r#"{"topic":"orderbook.1000.BTCUSDT","type":"delta","data":{"s":"BTCUSDT","b":[["100","99"]],"a":[],"u":2}} {}"#,
        ] {
            engine.handle(1.0, Event::WsMessage { socket, text });
            let book = engine.book("BTCUSDT").unwrap();
            assert_eq!(book.bids(), [(100.0, 1.0)]);
            assert_eq!(book.asks(), [(101.0, 2.0)]);
        }
        engine.handle(2.0, Event::WsMessage { socket, text: r#"{"topic":"orderbook.1000.BTCUSDT","type":"delta","data":{"s":"BTCUSDT","b":[["100","3"]],"u":2}}"# });
        let book = engine.book("BTCUSDT").unwrap();
        assert_eq!(book.bids(), [(100.0, 3.0)]);
        assert_eq!(book.asks(), [(101.0, 2.0)]);
    }
}

#[cfg(test)]
mod secondary_tests;
