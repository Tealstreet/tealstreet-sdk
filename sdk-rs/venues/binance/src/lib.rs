//! Binance market and account families on the sans-I/O engine.
mod account;
mod candles;
mod history;
mod identity;
mod paths;
mod reads;
mod session;
mod sign;
pub use sign::hosted_signing;
mod trading;
pub mod wire;

use safe_cex_engine::Level;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::credential_selection::{SlotPolicy, Slots};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::request_context::{Context, Operation};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::{Command, Cx, PublicFeedPolicy, Response, SocketId, TimerId, Venue, num};
use sign::{Credentials, Query};
use std::collections::{HashMap, HashSet, VecDeque};
use wire::*;

const MIN_RECONNECT_DELAY_MS: f64 = 500.0;
const MAX_RECONNECT_DELAY_MS: f64 = 30_000.0;
const DEPTH_SNAPSHOT_LIMIT: u32 = 1000;
/// Probed sockets get a cheap request this often; any socket silent for
/// `SILENCE_LIMIT_MS` is treated as dead (a half-open socket never reports a close).
const PROBE_INTERVAL_MS: f64 = 5_000.0;
const SILENCE_LIMIT_MS: f64 = 20_000.0;
/// Background reads pause at 80% of the 2,400 request-weight budget.
const WEIGHT_PAUSE_THRESHOLD: f64 = 1_920.0;
const WEIGHT_PAUSE_MS: f64 = 60_000.0;
/// Binance IP bans can last hours; a longer or overflowing `Retry-After` is capped
/// so background reads resume instead of stopping for good.
const MAX_BAN_PAUSE_MS: f64 = 86_400_000.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Variant {
    #[default]
    UsdM,
    PortfolioMargin,
    CoinM,
    Spot,
}

#[derive(Clone, Default)]
pub struct Options {
    pub signing_policy: safe_cex_engine::signing_policy::RemoteSigningPolicy,
    pub read_credentials: Option<Credentials>,
    pub testnet: bool,
    pub variant: Variant,
    /// Markets, tickers, books and candles only; no account.
    pub public_data_only: bool,
    pub credentials: Option<Credentials>,
    pub recv_window: Option<f64>,
    /// A portfolio margin account (`papi`) rather than USD-M futures.
    pub portfolio_margin: bool,
}
safe_cex_crypto::redacted_debug!(Options);

struct Urls {
    rest: &'static str,
    public_ws: &'static str,
    market_ws: &'static str,
    /// The account's REST host and private stream.
    account_rest: &'static str,
    private_ws: &'static str,
}

impl Urls {
    fn for_options(options: &Options, paths: &paths::Paths) -> Self {
        if options.testnet {
            Self {
                rest: paths.public_rest_testnet,
                public_ws: paths.public_ws_testnet,
                market_ws: paths.market_ws_testnet,
                account_rest: paths.rest_testnet,
                private_ws: paths.private_ws_testnet,
            }
        } else {
            Self {
                rest: paths.public_rest,
                public_ws: paths.public_ws,
                market_ws: paths.market_ws,
                account_rest: paths.rest,
                private_ws: paths.private_ws,
            }
        }
    }
}

/// Startup lists and klines stream in element by element (see
/// `Cx::request_streamed`); their continuations collect the decoded rows.
pub enum Pending {
    Background(Context, Box<Pending>),
    Identity(Context),
    ExchangeInfo {
        rows: Vec<Market>,
        rejected: bool,
    },
    Daily,
    BookTickers,
    PremiumIndex,
    DepthSnapshot {
        symbol: String,
        generation: u64,
    },
    Klines {
        context: Context,
        candles: Vec<Candle>,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(Box<history::Request>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Channel {
    Public,
    Market,
    Private,
    SpotTicker(usize),
}

struct Connection {
    socket: Option<SocketId>,
    open: bool,
    supervisor: Supervisor,
    next_request_id: u64,
    subscriptions: Vec<u64>,
    send_queue: VecDeque<(String, Vec<String>)>,
    send_timer: Option<TimerId>,
    next_send_at: f64,
}

impl Connection {
    fn new(probed: bool) -> Self {
        Self {
            socket: None,
            open: false,
            supervisor: Supervisor::new(Config {
                idle_timeout_ms: SILENCE_LIMIT_MS,
                ping_interval_ms: probed.then_some(PROBE_INTERVAL_MS),
                backoff_base_ms: MIN_RECONNECT_DELAY_MS,
                backoff_cap_ms: MAX_RECONNECT_DELAY_MS,
                ..Config::default()
            }),
            next_request_id: 10,
            subscriptions: Vec::new(),
            send_queue: VecDeque::new(),
            send_timer: None,
            next_send_at: 0.0,
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
    generation: u64,
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
    paths: &'static paths::Paths,
    /// Primary trading key and optional read key; chosen per request by role.
    credentials: Slots<Credentials>,
    public: Connection,
    market: Connection,
    private: Connection,
    spot_tickers: Vec<Connection>,
    books: HashMap<String, BookState>,
    /// Removing and re-adding a book must not reuse an in-flight snapshot ID.
    snapshot_generation: u64,
    candles: HashMap<(String, Interval), u32>,
    ticker_seed: TickerSeed,
    discovery: safe_cex_engine::startup::Backoff<u8>,
    catalogue_partial: bool,
    catalogue_symbols: HashSet<String>,
    ticker_reading: bool,
    ticker_refresh_pending: bool,
    live_tickers: safe_cex_engine::startup::LiveTickers,
    /// Depth snapshots are paused until this time after a 418 or 429.
    rest_paused_until: f64,
    /// Background account reads pause until this time near the weight limit.
    background_paused_until: f64,
    account: account::AccountState,
    trading: trading::TradingState,
    history: history::HistoryState,
}

impl BinanceFutures {
    /// The catalogue is complete and no discovery read awaits a retry.
    pub(crate) fn discovery_settled(&self) -> bool {
        !self.catalogue_partial && self.discovery.idle()
    }

    pub fn new(options: Options) -> Self {
        let mut options = options;
        if options.variant == Variant::UsdM && options.portfolio_margin {
            options.variant = Variant::PortfolioMargin;
        }
        options.portfolio_margin = options.variant == Variant::PortfolioMargin;
        let paths = match options.variant {
            Variant::UsdM => &paths::USD_M,
            Variant::PortfolioMargin => &paths::PORTFOLIO_MARGIN,
            Variant::CoinM => &paths::COIN_M,
            Variant::Spot => &paths::SPOT,
        };
        let urls = Urls::for_options(&options, paths);
        let spot = options.variant == Variant::Spot;
        let credentials = Slots::new(
            options
                .credentials
                .clone()
                .filter(|id| !options.public_data_only && id.validate().is_ok()),
            options.read_credentials.clone(),
            SlotPolicy::default(),
        );
        Self {
            options,
            urls,
            paths,
            credentials,
            public: Connection::new(spot),
            market: Connection::new(false),
            private: Connection::new(true),
            spot_tickers: Vec::new(),
            books: HashMap::new(),
            snapshot_generation: 0,
            candles: HashMap::new(),
            ticker_seed: TickerSeed::default(),
            discovery: Default::default(),
            catalogue_partial: false,
            catalogue_symbols: HashSet::new(),
            ticker_reading: false,
            ticker_refresh_pending: false,
            live_tickers: Default::default(),
            rest_paused_until: 0.0,
            background_paused_until: 0.0,
            account: account::AccountState::default(),
            trading: trading::TradingState::default(),
            history: history::HistoryState::default(),
        }
    }

    fn is_spot(&self) -> bool {
        self.options.variant == Variant::Spot
    }

    /// Binance kline codes are the TS spellings; only Spot serves `1s`.
    fn supports(&self, interval: Interval) -> bool {
        use Interval::*;
        match interval {
            S1 => self.is_spot(),
            M1 | M3 | M5 | M15 | M30 | H1 | H2 | H4 | H6 | H8 | H12 | D1 | D3 | W1 | Mo1 => true,
            _ => false,
        }
    }

    fn is_inverse(&self) -> bool {
        self.options.variant == Variant::CoinM
    }

    fn shared_public_socket(&self) -> bool {
        self.urls.public_ws == self.urls.market_ws
    }

    fn candle_channel(&self) -> Channel {
        if self.shared_public_socket() {
            Channel::Public
        } else {
            Channel::Market
        }
    }

    fn connection(&mut self, channel: Channel) -> &mut Connection {
        match channel {
            Channel::Public => &mut self.public,
            Channel::Market => &mut self.market,
            Channel::Private => &mut self.private,
            Channel::SpotTicker(index) => &mut self.spot_tickers[index],
        }
    }

    fn channel_of(&self, socket: SocketId) -> Option<Channel> {
        [Channel::Public, Channel::Market, Channel::Private]
            .into_iter()
            .chain((0..self.spot_tickers.len()).map(Channel::SpotTicker))
            .find(|&channel| self.connection_ref(channel).socket == Some(socket))
    }

    fn connection_ref(&self, channel: Channel) -> &Connection {
        match channel {
            Channel::Public => &self.public,
            Channel::Market => &self.market,
            Channel::Private => &self.private,
            Channel::SpotTicker(index) => &self.spot_tickers[index],
        }
    }

    fn get(&mut self, cx: &mut Cx<'_, Pending>, path: &str, pending: Pending) {
        self.background_read(
            cx,
            reads::Read {
                url: format!("{}{}", self.urls.rest, path),
                credentials: None,
                query: Query::new(),
                stream: None,
            },
            None,
            pending,
        );
    }

    fn get_streamed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        at: ArrayAt,
        pending: Pending,
    ) {
        self.background_read(
            cx,
            reads::Read {
                url: format!("{}{}", self.urls.rest, path),
                credentials: None,
                query: Query::new(),
                stream: Some(at),
            },
            None,
            pending,
        );
    }

    /// A signed request; every parameter travels in the query string.
    fn signed_recipe(&self, credentials: Credentials, query: Query) -> sign::Http {
        sign::Http::new(credentials, query, self.options.recv_window, true)
            .with_header_policy(self.options.signing_policy.header_policy())
    }

    fn connect_url(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel, url: &str) {
        if channel == Channel::Private && !cx.private_feed_policy().streaming() {
            return;
        }
        let role = if matches!(channel, Channel::Private) {
            safe_cex_engine::SocketChannel::Private
        } else {
            safe_cex_engine::SocketChannel::Public
        };
        // Suppressed public feeds never open; supervising them would only churn retries.
        if channel == Channel::Private || cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.connection(channel).supervisor.connecting(cx);
        }
        let socket = if self.is_spot() && channel != Channel::Private {
            let feed = match channel {
                Channel::SpotTicker(index) => format!("spot-ticker-{index}"),
                _ => "spot-books".to_owned(),
            };
            cx.connect(
                safe_cex_engine::SocketSpec::new(url)
                    .channel(role)
                    .feed(feed, false),
            )
        } else {
            cx.connect(safe_cex_engine::SocketSpec::new(url).channel(role))
        };
        let connection = self.connection(channel);
        connection.socket = Some(socket);
        connection.open = false;
    }

    fn connect(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        let url = match channel {
            Channel::Public | Channel::SpotTicker(_) => self.urls.public_ws,
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
                    .map_or(WEIGHT_PAUSE_MS, |seconds| {
                        (seconds * 1_000.0).min(MAX_BAN_PAUSE_MS)
                    }),
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

    fn on_supervisor_tick(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel, tick: Tick) {
        match tick {
            Tick::Reconnect => self.connect(cx, channel),
            Tick::Ping => self.probe(cx, channel),
            Tick::TimedOut(_) => {
                if let Some(socket) = self.connection(channel).socket {
                    cx.close(socket);
                }
                self.on_socket_closed(cx, channel, 4000);
            }
            Tick::Handled | Tick::Unhandled => {}
        }
    }

    /// Spot market sockets and the private socket are probed; the latter can be
    /// quiet for good reason.
    fn probe(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        let Some(socket) = self.connection(channel).socket else {
            return;
        };
        match channel {
            Channel::Private if self.is_spot() => self.ping_spot_account(cx),
            Channel::Private => cx.send(socket, "{\"id\":42,\"method\":\"LIST_SUBSCRIPTIONS\"}"),
            _ => cx.send(socket, "{\"id\":0,\"method\":\"LIST_SUBSCRIPTIONS\"}"),
        }
    }

    fn on_socket_closed(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel, code: u16) {
        let connection = self.connection(channel);
        connection.socket = None;
        connection.open = false;
        connection.subscriptions.clear();
        connection.send_queue.clear();
        if let Some(timer) = connection.send_timer.take() {
            cx.clear_timer(timer);
        }
        connection.next_send_at = 0.0;
        connection.supervisor.retry(cx);
        if channel == Channel::Private {
            self.reset_spot_subscription(cx);
            self.account.resync_orders_on_open = true;
        }
        cx.notice(
            Level::Warn,
            code::WS_DISCONNECTED,
            format!("Binance socket closed ({code}); reconnecting"),
        );
    }

    /// Keep Spot control frames below the native WebSocket server's payload
    /// limit and its five inbound messages per second budget.
    fn send_subscribe(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        channel: Channel,
        method: &str,
        params: &[String],
    ) {
        if !self.is_spot() || channel == Channel::Private {
            self.send_subscription(cx, channel, method, params);
            return;
        }
        if !self.connection_ref(channel).open {
            return;
        }
        let mut chunk = Vec::new();
        let mut bytes = 128; // Envelope, method, request ID and punctuation.
        for topic in params {
            let encoded = serde_json::to_string(topic).expect("string");
            if encoded.len() + 128 >= 4096 {
                cx.notice(
                    Level::Error,
                    code::SUBSCRIPTION_REJECTED,
                    "Binance Spot stream name exceeds the control frame limit",
                );
                return;
            }
            if chunk.len() == 128 || bytes + encoded.len() + 1 >= 4096 {
                self.connection(channel)
                    .send_queue
                    .push_back((method.to_owned(), std::mem::take(&mut chunk)));
                bytes = 128;
            }
            bytes += encoded.len() + 1;
            chunk.push(topic.clone());
        }
        if !chunk.is_empty() {
            self.connection(channel)
                .send_queue
                .push_back((method.to_owned(), chunk));
        }
        self.pump_subscriptions(cx, channel);
    }

    fn pump_subscriptions(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        let now = cx.now;
        let connection = self.connection(channel);
        if !connection.open || connection.send_timer.is_some() || connection.send_queue.is_empty() {
            return;
        }
        if now < connection.next_send_at {
            connection.send_timer = Some(cx.set_timer(connection.next_send_at - now));
            return;
        }
        let (method, params) = connection.send_queue.pop_front().expect("nonempty queue");
        connection.next_send_at = now + 400.0;
        self.send_subscription(cx, channel, &method, &params);
        let connection = self.connection(channel);
        if !connection.send_queue.is_empty() {
            connection.send_timer = Some(cx.set_timer(400.0));
        }
    }

    fn send_subscription(
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
        if self.options.variant == Variant::Spot {
            self.connection(channel).subscriptions.push(id);
        }
        let params = serde_json::to_string(params).expect("string list");
        cx.send(
            socket,
            format!("{{\"method\":\"{method}\",\"params\":{params},\"id\":{id}}}"),
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
        rejected: bool,
    ) {
        if response.status != 200 || markets.is_empty() {
            cx.notice(
                Level::Warn,
                code::REQUEST_FAILED,
                format!(
                    "Binance exchangeInfo read unavailable (HTTP {})",
                    response.status
                ),
            );
            self.discovery
                .failed_after(cx, 0, self.background_paused_until - cx.now);
            return;
        }
        let was_loaded = cx.store.loaded().markets;
        let previous_len = cx.store.markets().len();
        self.catalogue_symbols = markets.iter().map(|market| market.symbol.clone()).collect();
        if !was_loaded {
            cx.store.set_markets(markets);
        } else {
            let mut merged = cx.store.markets().to_vec();
            let mut indices: HashMap<String, usize> = merged
                .iter()
                .enumerate()
                .map(|(index, market)| (market.symbol.clone(), index))
                .collect();
            for mut market in markets {
                if let Some(index) = indices.get(&market.symbol).copied() {
                    // Leverage brackets are private native metadata, fetched once.
                    market.limits.leverage = merged[index].limits.leverage;
                    merged[index] = market;
                } else {
                    indices.insert(market.symbol.clone(), merged.len());
                    merged.push(market);
                }
            }
            cx.store.set_markets(merged);
        }
        self.catalogue_partial = rejected;
        if rejected {
            self.discovery
                .failed_after(cx, 0, self.background_paused_until - cx.now);
        } else {
            self.discovery.succeeded(cx, &0);
        }
        if self.credentials.has_primary() {
            self.start_account(cx);
        }
        if was_loaded {
            self.catalogue_wallet_repair(cx);
            if self.is_spot() {
                for index in previous_len..cx.store.markets().len() {
                    let shard = index / 1_024;
                    if self
                        .spot_tickers
                        .get(shard)
                        .is_some_and(|connection| connection.open)
                    {
                        let topic =
                            format!("{}@ticker", cx.store.markets()[index].symbol.to_lowercase());
                        self.send_subscribe(cx, Channel::SpotTicker(shard), "SUBSCRIBE", &[topic]);
                    }
                }
            }
        }
        if !was_loaded && self.credentials.has_primary() && !self.is_spot() {
            self.request_leverage_brackets(cx);
        }
        self.start_ticker_seed(cx);
    }

    fn start_ticker_seed(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.ticker_reading {
            self.ticker_refresh_pending = true;
            return;
        }
        self.ticker_reading = true;
        self.live_tickers.begin();
        self.ticker_seed = TickerSeed {
            tickers: cx
                .store
                .markets()
                .iter()
                .map(|market| Ticker {
                    id: market.id.clone(),
                    symbol: market.symbol.clone(),
                    ..Ticker::default()
                })
                .collect(),
            reported: vec![0; cx.store.markets().len()],
            finished: 0,
            failed: false,
        };
        self.get_streamed(cx, self.paths.daily, ArrayAt::Root, Pending::Daily);
        if self.is_spot() {
            return;
        }
        self.get_streamed(
            cx,
            self.paths.book_tickers,
            ArrayAt::Root,
            Pending::BookTickers,
        );
        self.get_streamed(
            cx,
            self.paths.premium_index,
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
            Ok(response) if response.status == 200 => (!self
                .ticker_seed
                .reported
                .iter()
                .any(|mask| mask & from != 0))
            .then(|| "Ticker source has no usable rows".to_string()),
            Ok(response) => Some(format!("HTTP {}: {}", response.status, response.body)),
            Err(error) => Some(error.to_owned()),
        };
        if let Some(failure) = failure {
            cx.notice(
                Level::Warn,
                code::REQUEST_FAILED,
                format!("Binance tickers: {failure}"),
            );
            for reported in &mut self.ticker_seed.reported {
                *reported &= !from;
            }
            self.ticker_seed.finished &= !from;
            self.discovery
                .failed_after(cx, from, self.background_paused_until - cx.now);
            return;
        }
        self.discovery.succeeded(cx, &from);
        self.ticker_seed.finished |= from;
        self.try_seed_tickers(cx);
    }

    fn try_seed_tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        let required = if self.is_spot() {
            source::DAILY
        } else {
            source::ALL
        };
        if self.ticker_seed.finished != required {
            return;
        }
        let seed = std::mem::take(&mut self.ticker_seed);
        self.ticker_reading = false;
        let refresh = std::mem::take(&mut self.ticker_refresh_pending);
        let mut tickers: Vec<Ticker> = if seed.failed {
            Vec::new()
        } else {
            seed.tickers
                .into_iter()
                .zip(seed.reported)
                .filter(|(_, reported)| *reported == required)
                .map(|(ticker, _)| ticker)
                .collect()
        };
        cx.notice(
            Level::Info,
            code::STARTUP_PROGRESS,
            format!(
                "Loaded {} Binance markets",
                tickers.len().min(cx.store.markets().len())
            ),
        );
        self.live_tickers.finish(cx.store, &mut tickers);
        cx.store.set_tickers(tickers);
        self.revalue_pending_wallet(cx);
        if !self.credentials.has_primary() {
            cx.store.set_orders(Vec::new());
            cx.store.set_positions(Vec::new());
            cx.store.set_balance(Default::default());
        }
        if self.public.socket.is_none() && !self.public.supervisor.is_waiting() {
            self.connect(cx, Channel::Public);
        }
        if self.is_spot() {
            while self.spot_tickers.len() < cx.store.markets().len().div_ceil(1_024) {
                let index = self.spot_tickers.len();
                self.spot_tickers.push(Connection::new(true));
                self.connect(cx, Channel::SpotTicker(index));
            }
        } else if !self.shared_public_socket()
            && self.market.socket.is_none()
            && !self.market.supervisor.is_waiting()
        {
            self.connect(cx, Channel::Market);
        }
        if refresh {
            self.start_ticker_seed(cx);
        }
    }

    fn on_public_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        match event_type(text) {
            Some("bookTicker") | None if self.is_spot() || event_type(text).is_some() => {
                if let Ok(event) = serde_json::from_str::<BookTickerEvent>(text) {
                    let (bid, ask) = (num::parse(&event.b), num::parse(&event.a));
                    cx.store.update_ticker(&event.s, |t| {
                        t.bid = bid;
                        t.ask = ask;
                    });
                    self.live_tickers.record(cx.store, &event.s, 3);
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

    fn apply_ticker(&mut self, cx: &mut Cx<'_, Pending>, event: TickerEvent<'_>) {
        let last = num::parse(&event.c);
        let percentage = num::parse(&event.percentage);
        let volume = num::parse(&event.v);
        let quote_volume = num::parse(
            event
                .q
                .as_deref()
                .or(event.base_volume.as_deref())
                .unwrap_or("0"),
        );
        cx.store.update_ticker(&event.s, |t| {
            t.last = last;
            if self.is_spot() {
                t.mark = last;
                t.index = last;
                if let Some(bid) = event.bid.as_deref() {
                    t.bid = num::parse(bid);
                }
                if let Some(ask) = event.ask.as_deref() {
                    t.ask = num::parse(ask);
                }
            }
            t.percentage = percentage;
            t.volume = volume;
            t.quote_volume = quote_volume;
        });
        let mut mask = (1 << 2) | (1 << 5) | (1 << 9) | (1 << 10);
        if self.is_spot() {
            mask |= (1 << 3) | (1 << 4);
            if event.bid.is_some() {
                mask |= 1;
            }
            if event.ask.is_some() {
                mask |= 2;
            }
        }
        self.live_tickers.record(cx.store, &event.s, mask);
    }

    fn on_market_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        match event_type(text) {
            Some("24hrTicker") => {
                if self.is_spot() && !text.trim_start().starts_with('[') {
                    if let Ok(event) = serde_json::from_str::<TickerEvent>(text) {
                        self.apply_ticker(cx, event);
                    }
                } else if let Ok(events) = serde_json::from_str::<Vec<TickerEvent>>(text) {
                    for event in events {
                        self.apply_ticker(cx, event);
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
                        self.live_tickers.record(
                            cx.store,
                            &event.s,
                            (1 << 3) | (1 << 4) | (1 << 7) | (1 << 8),
                        );
                    }
                }
            }
            Some("kline") => {
                if let Ok(event) = serde_json::from_str::<KlineEvent>(text) {
                    let k = &event.k;
                    if let Some(interval) = Interval::parse(&k.i)
                        && self.candles.contains_key(&(k.s.to_string(), interval))
                    {
                        cx.candle(
                            &k.s,
                            interval,
                            kline_candle(k.t, &k.o, &k.h, &k.l, &k.c, &k.v),
                        );
                    }
                }
            }
            _ => {}
        }
        self.revalue_pending_wallet(cx);
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
                self.snapshot_generation += 1;
                state.generation = self.snapshot_generation;
                let generation = state.generation;
                let symbol = event.s.to_string();
                let path = format!(
                    "{}?symbol={symbol}&limit={DEPTH_SNAPSHOT_LIMIT}",
                    self.paths.depth
                );
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
                let continuous = if *awaiting_first || self.options.variant == Variant::Spot {
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
        let backoff = Backoff::new(1_000.0, u32::MAX, MAX_SNAPSHOT_BACKOFF_MS)
            .delay(state.failures.saturating_sub(1));
        let delay = cx.jittered(backoff).max(min_delay_ms);
        state.sync = Sync::Cooldown(cx.set_timer(delay));
    }

    fn on_depth_snapshot(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        generation: u64,
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
            cx.notice(
                Level::Warn,
                code::REQUEST_PAUSED,
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
            let continuous = if awaiting_first || self.options.variant == Variant::Spot {
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
        if self.is_spot()
            && !self.books.contains_key(&symbol)
            && self.books.len() + self.candles.len() >= 1_024
        {
            cx.notice(
                Level::Error,
                code::SUBSCRIPTION_REJECTED,
                "Binance Spot supports at most 1024 combined book/candle streams per account",
            );
            return;
        }
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
        context: Context,
        candles: Vec<Candle>,
        response: Response<'_>,
    ) {
        let Response { status, body, .. } = response;
        if status != 200 {
            let _ = cx.fail_context(&context, format!("Binance klines HTTP {status}: {body}"));
            return;
        }
        let _ = cx.done_context(
            &context,
            serde_json::to_string(&candles).unwrap_or_default(),
        );
    }
}

fn filter_value<'a>(filter: Option<&Filter<'a>>, pick: fn(&Filter<'a>) -> Option<f64>) -> f64 {
    filter.and_then(pick).unwrap_or(f64::NAN)
}

fn market_from_wire(m: &SymbolInfo<'_>, variant: Variant) -> Market {
    let spot = variant == Variant::Spot;
    let inverse = variant == Variant::CoinM;
    let filter = |kind: &str| m.filters.iter().find(|f| f.filter_type == kind);
    let price = filter("PRICE_FILTER");
    let lot = filter("LOT_SIZE");
    let market_lot = filter("MARKET_LOT_SIZE");
    let min_qty: fn(&Filter<'_>) -> Option<f64> = |f| f.min_qty.as_deref().map(num::parse);
    let max_qty: fn(&Filter<'_>) -> Option<f64> = |f| f.max_qty.as_deref().map(num::parse);
    Market {
        id: if spot {
            format!("{}/{}", m.base_asset, m.quote_asset)
        } else {
            format!("{}/{}:{}", m.base_asset, m.quote_asset, m.margin_asset)
        },
        symbol: m.symbol.to_string(),
        pretty_symbol: None,
        base: m.base_asset.to_string(),
        quote: m.quote_asset.to_string(),
        active: m.status == "TRADING",
        precision: Precision {
            amount: {
                let step = filter_value(lot, |f| f.step_size.as_deref().map(num::parse));
                if step > 0.0 {
                    step
                } else if spot {
                    10f64.powi(-m.base_asset_precision)
                } else if inverse {
                    1.0
                } else {
                    step
                }
            },
            price: {
                let step = filter_value(price, |f| f.tick_size.as_deref().map(num::parse));
                if spot && (!step.is_finite() || step <= 0.0) {
                    10f64.powi(-m.quote_asset_precision)
                } else {
                    step
                }
            },
        },
        limits: Limits {
            amount: MinMax {
                min: if spot {
                    filter_value(lot, min_qty).max(0.0)
                } else {
                    filter_value(lot, min_qty).max(filter_value(market_lot, min_qty))
                },
                max: if spot {
                    let max = filter_value(lot, max_qty);
                    if max > 0.0 { max } else { 999_999_999.0 }
                } else {
                    filter_value(lot, max_qty).min(filter_value(market_lot, max_qty))
                },
            },
            leverage: MinMax { min: 1.0, max: 1.0 },
            price: None,
        },
        contract_size: inverse.then_some(if m.contract_size > 0.0 {
            m.contract_size
        } else {
            1.0
        }),
        is_expiring: None,
        is_inverse: inverse.then_some(true),
        is_spot: spot.then_some(true),
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
    fn unproven_http_evidence(
        &self,
        pending: &Pending,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        if let Pending::Trade(request) = pending {
            self.capture_unproven_trade(request, response, evidence);
        }
    }

    fn discard_unproven_http_command(&mut self, owner: safe_cex_engine::request_context::Owner) {
        self.discard_unproven_trade(owner);
    }

    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        if !matches!(pending, Pending::Trade(_)) {
            return Err(safe_cex_engine::request_context::Error::Invalid);
        }
        // Initial native late codec intentionally has no mutation authority.
        // Unknown effects reconcile through ordinary fenced account reads.
        safe_cex_engine::request_context::ReconciliationData::new(String::new())
    }

    fn cancel_needs_stored_order(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        match self.options.variant {
            Variant::UsdM | Variant::PortfolioMargin => safe_cex_engine::Capabilities {
                position_history: if self.options.variant == Variant::UsdM {
                    safe_cex_engine::PositionHistory::FillsSymbol
                } else {
                    safe_cex_engine::PositionHistory::Unsupported
                },
                client_order_lookup: true,
                account: safe_cex_engine::AccountLookup::Query,
                ..safe_cex_engine::Capabilities::BASE
            },
            Variant::Spot => safe_cex_engine::Capabilities::NO_HISTORY,
            Variant::CoinM => safe_cex_engine::Capabilities {
                quantity_units: safe_cex_engine::QuantityUnits::InverseContractSized,
                ..safe_cex_engine::Capabilities::NO_HISTORY
            },
        }
    }

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            return;
        }
        self.get_streamed(
            cx,
            self.paths.exchange_info,
            ArrayAt::Field("symbols"),
            Pending::ExchangeInfo {
                rows: Vec::new(),
                rejected: false,
            },
        );
    }

    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        self.revalue_pending_wallet(cx);
    }

    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let Some(channel) = self.channel_of(socket) else {
            return;
        };
        let connection = self.connection(channel);
        connection.open = true;
        connection.supervisor.opened(cx);
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
                if self.is_spot() {
                    topics.clear();
                    topics.extend(self.kline_topics());
                } else if self.shared_public_socket() {
                    topics.push("!ticker@arr".to_owned());
                    topics.push("!markPrice@arr@1s".to_owned());
                    topics.extend(self.kline_topics());
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
            Channel::SpotTicker(index) => {
                let topics: Vec<_> = cx
                    .store
                    .markets()
                    .iter()
                    .skip(index * 1_024)
                    .take(1_024)
                    .map(|market| format!("{}@ticker", market.symbol.to_lowercase()))
                    .collect();
                self.send_subscribe(cx, channel, "SUBSCRIBE", &topics);
            }
        }
    }

    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(channel) = self.channel_of(socket) else {
            return;
        };
        self.connection(channel).supervisor.received(cx.now);
        if self.is_spot() && channel != Channel::Private {
            if event_type(text) == Some("serverShutdown") {
                cx.close(socket);
                self.on_socket_closed(cx, channel, 4000);
                return;
            }
            if event_type(text).is_none()
                && let Ok(ack) = serde_json::from_str::<SubscriptionAck>(text)
                && let Some(id) = ack.id
            {
                let pending = &mut self.connection(channel).subscriptions;
                if let Some(at) = pending.iter().position(|request| *request == id) {
                    pending.swap_remove(at);
                    if ack.code.is_some_and(|code| code != 0) {
                        cx.notice(
                            Level::Error,
                            code::SUBSCRIPTION_REJECTED,
                            format!("Binance Spot subscription rejected: {}", ack.msg),
                        );
                        cx.close(socket);
                        self.on_socket_closed(cx, channel, 4000);
                    }
                    return;
                }
            }
        }
        match channel {
            Channel::Public => {
                self.on_public_frame(cx, text);
                if self.shared_public_socket() {
                    self.on_market_frame(cx, text);
                }
            }
            Channel::Market | Channel::SpotTicker(_) => self.on_market_frame(cx, text),
            Channel::Private => self.on_user_event(cx, text),
        }
    }

    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        if let Some(channel) = self.channel_of(socket) {
            self.on_socket_closed(cx, channel, code);
        }
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Background(context, pending) = pending {
            let _ = cx.finish_http_context(&context);
            self.on_response(cx, *pending, response);
            return;
        }
        self.observe_weight(cx, &response);
        if let Pending::Identity(context) = pending {
            self.identity_response(cx, context, response);
            return;
        }
        let Response { status, body, .. } = response;
        match pending {
            Pending::Background(..) => unreachable!(),
            Pending::Identity(_) => unreachable!(),
            Pending::ExchangeInfo { rows, rejected } => {
                self.on_markets(cx, rows, response, rejected)
            }
            Pending::Daily => self.source_finished(cx, source::DAILY, Ok(response)),
            Pending::BookTickers => self.source_finished(cx, source::BOOK, Ok(response)),
            Pending::PremiumIndex => self.source_finished(cx, source::PREMIUM, Ok(response)),
            Pending::DepthSnapshot { symbol, generation } => {
                self.on_depth_snapshot(cx, symbol, generation, status, body)
            }
            Pending::Klines { context, candles } => self.on_klines(cx, context, candles, response),
            Pending::Account(request) => self.on_account_response(cx, request, response),
            Pending::Trade(request) => self.on_trade_response(cx, request, Ok(response)),
            Pending::History(request) => self.on_history_response(cx, *request, response),
        }
    }

    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Background(_, pending) => self.on_item(cx, pending, item),
            Pending::ExchangeInfo {
                rows: markets,
                rejected,
            } => {
                let Ok(m) = serde_json::from_str::<SymbolInfo>(item) else {
                    *rejected = true;
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                };
                if self.is_spot()
                    || if self.is_inverse() {
                        m.contract_type == "PERPETUAL" && m.status == "TRADING"
                    } else {
                        matches!(&*m.contract_type, "PERPETUAL" | "TRADIFI_PERPETUAL")
                            && matches!(&*m.margin_asset, "USDT" | "USDC")
                    }
                {
                    let market = market_from_wire(&m, self.options.variant);
                    if !market.precision.is_tradable() {
                        *rejected = true;
                        safe_cex_engine::startup::skipped_row(cx);
                        return;
                    }
                    markets.push(market);
                }
            }
            Pending::Daily => {
                let Ok(d) = serde_json::from_str::<Daily>(item) else {
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                };
                {
                    let inverse = self.is_inverse();
                    let spot = self.is_spot();
                    self.seed(cx, &d.symbol, source::DAILY, |t| {
                        t.last = num::parse(&d.last_price);
                        t.percentage = num::parse(&d.price_change_percent);
                        t.volume = num::parse(&d.volume);
                        t.quote_volume = num::parse(if inverse {
                            &d.base_volume
                        } else {
                            &d.quote_volume
                        });
                        if spot {
                            t.bid = num::parse(&d.bid_price);
                            t.ask = num::parse(&d.ask_price);
                            t.mark = t.last;
                            t.index = t.last;
                        }
                    });
                }
            }
            Pending::BookTickers => {
                let Ok(b) = serde_json::from_str::<BookTickerRest>(item) else {
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                };
                {
                    self.seed(cx, &b.symbol, source::BOOK, |t| {
                        t.bid = num::parse(&b.bid_price);
                        t.ask = num::parse(&b.ask_price);
                    });
                }
            }
            Pending::PremiumIndex => {
                let Ok(p) = serde_json::from_str::<PremiumIndex>(item) else {
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                };
                {
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

    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Background(context, pending) => {
                let _ = cx.finish_http_context(&context);
                self.on_sign_error(cx, *pending, error);
            }
            Pending::Trade(request) => self.on_trade_sign_failed(cx, request, error),
            Pending::Identity(context) => self.identity_error(cx, &context, error),
            Pending::Klines { context, .. } => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::History(request) => self.history_failed(cx, *request, error, true),
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }

    /// Managed history keeps the typed failure, such as a page Timeout.
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::History(request) => self.history_failed(cx, *request, error, false),
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }

    fn read_root_retired(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
        _pending: Vec<Pending>,
    ) {
        self.retire_history(owner);
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Pending::Background(context, pending) = pending {
            let _ = cx.finish_http_context(&context);
            self.on_request_failed(cx, *pending, error);
            return;
        }
        if let Pending::Identity(context) = pending {
            self.identity_error(cx, &context, error.into());
            return;
        }
        match pending {
            Pending::Background(..) => unreachable!(),
            Pending::Klines { context, .. } => {
                let _ = cx.fail_context(&context, error.to_owned());
            }
            Pending::DepthSnapshot { symbol, generation } => {
                if let Some(state) = self.books.get_mut(&symbol)
                    && state.generation == generation
                {
                    Self::snapshot_failed(cx, state, 0.0);
                }
            }
            Pending::Account(request) => self.on_account_failed(cx, request, error),
            Pending::ExchangeInfo { .. } => {
                self.discovery
                    .failed_after(cx, 0, self.background_paused_until - cx.now)
            }
            Pending::Daily => self.source_finished(cx, source::DAILY, Err(error)),
            Pending::BookTickers => self.source_finished(cx, source::BOOK, Err(error)),
            Pending::PremiumIndex => self.source_finished(cx, source::PREMIUM, Err(error)),
            Pending::Trade(request) => self.on_trade_response(cx, request, Err(error)),
            Pending::History(request) => self.history_failed(
                cx,
                *request,
                safe_cex_engine::VenueError::new(error).kind(safe_cex_engine::ErrorKind::Network),
                false,
            ),
            Pending::Identity(_) => unreachable!(),
        }
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(source) = self.discovery.on_timer(timer) {
            match source {
                0 => self.get_streamed(
                    cx,
                    self.paths.exchange_info,
                    ArrayAt::Field("symbols"),
                    Pending::ExchangeInfo {
                        rows: Vec::new(),
                        rejected: false,
                    },
                ),
                source::DAILY => {
                    self.get_streamed(cx, self.paths.daily, ArrayAt::Root, Pending::Daily)
                }
                source::BOOK => self.get_streamed(
                    cx,
                    self.paths.book_tickers,
                    ArrayAt::Root,
                    Pending::BookTickers,
                ),
                source::PREMIUM => self.get_streamed(
                    cx,
                    self.paths.premium_index,
                    ArrayAt::Root,
                    Pending::PremiumIndex,
                ),
                _ => unreachable!(),
            }
            return;
        }

        for state in self.books.values_mut() {
            if matches!(state.sync, Sync::Cooldown(t) if t == timer) {
                state.sync = Sync::Idle;
            }
        }
        let channels = [Channel::Public, Channel::Market, Channel::Private]
            .into_iter()
            .chain((0..self.spot_tickers.len()).map(Channel::SpotTicker))
            .collect::<Vec<_>>();
        for channel in channels {
            let tick = self.connection(channel).supervisor.on_timer(cx, timer);
            self.on_supervisor_tick(cx, channel, tick);
            if self.connection(channel).send_timer == Some(timer) {
                self.connection(channel).send_timer = None;
                self.pump_subscriptions(cx, channel);
            }
        }
        self.on_account_timer(cx, timer);
        self.on_history_timer(cx, timer);
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::FetchAccount { .. } = command {
            self.fetch_identity(cx);
            return;
        }
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => self.subscribe_book(cx, symbol),
            Command::UnsubscribeOrderBook { symbol } => self.unsubscribe_book(cx, &symbol),
            Command::SubscribeCandles { symbol, interval } => {
                if !self.supports(interval) {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("Binance"),
                    );
                    return;
                }
                if self.is_spot()
                    && !self.candles.contains_key(&(symbol.clone(), interval))
                    && self.books.len() + self.candles.len() >= 1_024
                {
                    cx.notice(Level::Error, code::SUBSCRIPTION_REJECTED, "Binance Spot supports at most 1024 combined book/candle streams per account");
                    return;
                }
                let count = self.candles.entry((symbol.clone(), interval)).or_insert(0);
                *count += 1;
                if *count == 1 {
                    let topic = format!("{}@kline_{interval}", symbol.to_lowercase());
                    self.send_subscribe(cx, self.candle_channel(), "SUBSCRIBE", &[topic]);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol, interval);
                if let Some(count) = self.candles.get_mut(&key) {
                    *count -= 1;
                    if *count == 0 {
                        self.candles.remove(&key);
                        let topic = format!("{}@kline_{}", key.0.to_lowercase(), key.1);
                        self.send_subscribe(cx, self.candle_channel(), "UNSUBSCRIBE", &[topic]);
                    }
                }
            }
            Command::FetchCandles {
                command: _,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => {
                if !self.supports(interval) {
                    // Typed Unsupported, never a bare message (better than TS).
                    let error = safe_cex_engine::VenueError::new(interval.unsupported("Binance"))
                        .kind(safe_cex_engine::ErrorKind::Unsupported)
                        .outcome(safe_cex_engine::Outcome::NotSent);
                    cx.refuse_command(Operation::Read, error);
                    return;
                }
                let query = match candles::query(&symbol, interval, limit, from_ms, to_ms, cx.now) {
                    Ok(query) => query,
                    Err(error) => {
                        let error = safe_cex_engine::VenueError::new(error)
                            .kind(safe_cex_engine::ErrorKind::InvalidInput)
                            .outcome(safe_cex_engine::Outcome::NotSent);
                        cx.refuse_command(Operation::Read, error);
                        return;
                    }
                };
                // Initial issuance refusal is already correlated and completed
                // by the engine entry transaction; never fall back numerically.
                let Ok(context) = Self::read_context(cx, None) else {
                    return;
                };
                let path = format!("{}?{}", self.paths.klines, query.into_string());
                self.managed_read(
                    cx,
                    context.clone(),
                    reads::Read {
                        url: format!("{}{}", self.urls.rest, path),
                        credentials: None,
                        query: Query::new(),
                        stream: Some(ArrayAt::Root),
                    },
                    Pending::Klines {
                        context,
                        candles: Vec::new(),
                    },
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

    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .map(|(symbol, state)| (symbol.as_str(), &state.book)),
        )
    }
}

#[cfg(test)]
mod secondary_tests;

#[cfg(test)]
extern crate self as safe_cex_venue_binance;
#[cfg(test)]
#[path = "../tests/support/config.rs"]
mod test_config;
