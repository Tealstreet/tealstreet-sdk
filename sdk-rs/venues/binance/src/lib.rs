//! Binance market and account families on the sans-I/O engine.
mod account;
mod candles;
mod history;
mod identity;
mod options;
mod options_products;
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
use safe_cex_engine::socket::{Config, Timeout};
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, ControlPolicy, EncodedBatch, FeedId, FeedPolicy, Interest,
    Invalidation, LifecycleEvent, MAX_FRAME_BYTES, RetireReason, SocketCapabilities, SocketError,
    TrafficPolicy,
};
use safe_cex_engine::{Command, Cx, PublicFeedPolicy, Response, SocketId, TimerId, Venue, num};
use sign::{Credentials, Query};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use wire::*;

const MIN_RECONNECT_DELAY_MS: f64 = 500.0;
const MAX_RECONNECT_DELAY_MS: f64 = 30_000.0;
const DEPTH_SNAPSHOT_LIMIT: u32 = 1000;
/// Probed sockets get a cheap request this often; any socket silent for
/// `SILENCE_LIMIT_MS` is treated as dead (a half-open socket never reports a close).
const PROBE_INTERVAL_MS: f64 = 5_000.0;
const SILENCE_LIMIT_MS: f64 = 20_000.0;
/// Spot's native server takes five inbound messages per second, each at most
/// `SPOT_FRAME_TOPICS` streams and below `SPOT_FRAME_BYTES`.
const SPOT_SUBSCRIBE_PACE_MS: f64 = 400.0;
const SPOT_FRAME_TOPICS: usize = 128;
const SPOT_FRAME_BYTES: usize = 4096;
/// Envelope, method, request ID and punctuation of a subscribe frame.
const FRAME_ENVELOPE_BYTES: usize = 128;
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
    Options,
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
    Options(Box<options::Request>),
    OptionsProduct(Box<options_products::Request>),
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

/// One managed feed; the engine owns its epochs, timers, retries and pacing.
struct Connection {
    feed: Option<FeedId>,
    /// The current epoch's socket, from connect until retirement.
    socket: Option<SocketId>,
    open: bool,
    /// Retired: only the engine's Reconnect (never, once stopped) connects it again.
    parked: bool,
    probed: bool,
    /// Never reset, so frame ids stay unique across reconnects.
    next_request_id: Cell<u64>,
    /// Spot subscribe frame ids awaiting their acknowledgement.
    subscriptions: RefCell<Vec<u64>>,
}

impl Connection {
    fn new(probed: bool) -> Self {
        Self {
            feed: None,
            socket: None,
            open: false,
            parked: false,
            probed,
            next_request_id: Cell::new(10),
            subscriptions: RefCell::new(Vec::new()),
        }
    }

    /// Neither connected nor waiting for the engine's retry.
    fn idle(&self) -> bool {
        self.socket.is_none() && !self.parked
    }
}

fn socket_failed(cx: &mut Cx<'_, Pending>, error: SocketError) {
    cx.notice(
        Level::Error,
        code::SUBSCRIPTION_REJECTED,
        format!("Binance socket admission failed: {error:?}"),
    );
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
    option_state: options::State,
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
        if options.variant == Variant::Options && options.recv_window.is_none() {
            options.recv_window = Some(5_000.);
        }
        let paths = match options.variant {
            Variant::UsdM => &paths::USD_M,
            Variant::PortfolioMargin => &paths::PORTFOLIO_MARGIN,
            Variant::CoinM => &paths::COIN_M,
            Variant::Spot => &paths::SPOT,
            Variant::Options => &paths::OPTIONS,
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
            option_state: options::State::default(),
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

    fn channels(&self) -> impl Iterator<Item = Channel> + use<> {
        [Channel::Public, Channel::Market, Channel::Private]
            .into_iter()
            .chain((0..self.spot_tickers.len()).map(Channel::SpotTicker))
    }

    fn channel_of(&self, socket: SocketId) -> Option<Channel> {
        self.channels()
            .find(|&channel| self.connection_ref(channel).socket == Some(socket))
    }

    fn channel_of_feed(&self, feed: FeedId) -> Option<Channel> {
        self.channels()
            .find(|&channel| self.connection_ref(channel).feed == Some(feed))
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

    /// The legacy supervisor's timings: 20 s silence, a 5 s probe where probed,
    /// a 10 s connect bound and a 500 ms to 30 s backoff.
    fn feed_policy(&self, channel: Channel) -> FeedPolicy {
        let paced = self.is_spot() && channel != Channel::Private;
        FeedPolicy {
            role: if channel == Channel::Private {
                safe_cex_engine::SocketChannel::Private
            } else {
                safe_cex_engine::SocketChannel::Public
            },
            supervision: Config {
                idle_timeout_ms: SILENCE_LIMIT_MS,
                ping_interval_ms: self
                    .connection_ref(channel)
                    .probed
                    .then_some(PROBE_INTERVAL_MS),
                backoff_base_ms: MIN_RECONNECT_DELAY_MS,
                backoff_cap_ms: MAX_RECONNECT_DELAY_MS,
                ..Config::default()
            },
            // Listen keys and the Spot user-data subscription are venue-owned; no feed logs in.
            auth: AuthMode::None,
            recovery: AuthRecovery::Reconnect,
            permanent_refusal_group: None,
            traffic: TrafficPolicy::AnyText,
            controls: ControlPolicy::None,
            invalidation: Invalidation::Required,
            subscription_interval_ms: if paced { SPOT_SUBSCRIBE_PACE_MS } else { 0.0 },
            close_floors: &[],
        }
    }

    fn connect_url(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel, url: &str) {
        if channel == Channel::Private && !cx.private_feed_policy().streaming() {
            return;
        }
        // Suppressed public feeds never register, so they never churn retries.
        if channel != Channel::Private && cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return;
        }
        let feed = match self.connection_ref(channel).feed {
            Some(feed) => feed,
            None => match cx.register_socket_feed(self.feed_policy(channel)) {
                Ok(feed) => {
                    self.connection(channel).feed = Some(feed);
                    feed
                }
                Err(error) => return socket_failed(cx, error),
            },
        };
        if self.connection_ref(channel).parked || cx.socket_epoch(feed).is_some() {
            return;
        }
        let mut spec = safe_cex_engine::SocketSpec::new(url);
        if self.is_spot() && channel != Channel::Private {
            let name = match channel {
                Channel::SpotTicker(index) => format!("spot-ticker-{index}"),
                _ => "spot-books".to_owned(),
            };
            spec = spec.feed(name, false);
        }
        match cx.connect_socket(feed, spec) {
            Ok(epoch) => {
                let connection = self.connection(channel);
                connection.socket = Some(epoch.socket());
                connection.open = false;
            }
            Err(error) => socket_failed(cx, error),
        }
    }

    /// Retire the channel's live epoch; the engine then backs off and reconnects.
    fn retire(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel, reason: RetireReason) {
        if let Some(feed) = self.connection_ref(channel).feed {
            cx.retire_socket(feed, reason);
        }
    }

    fn open_feed(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
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

    fn on_socket_retired(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        channel: Channel,
        reason: RetireReason,
        close_code: Option<u16>,
    ) {
        let connection = self.connection(channel);
        connection.socket = None;
        connection.open = false;
        connection.parked = true;
        if channel == Channel::Private {
            self.reset_spot_subscription(cx);
            self.account.resync_orders_on_open = true;
        }
        let message = match (reason, close_code) {
            (RetireReason::Closed, Some(code)) => {
                format!("Binance socket closed ({code}); reconnecting")
            }
            (RetireReason::Timeout(Timeout::Idle), _) => {
                "Binance socket closed (idle timeout); reconnecting".into()
            }
            (RetireReason::Timeout(_), _) => "Binance socket timed out; reconnecting".into(),
            (RetireReason::Authentication, _) => {
                "Binance socket authentication failed; reconnecting".into()
            }
            (RetireReason::Closed, None) => "Binance socket closed; reconnecting".into(),
            (RetireReason::Stopped, _) => "Binance socket stopped".into(),
            (RetireReason::RouteExhausted, _) => "Binance socket refused on every route".into(),
            (RetireReason::RouteRefused, _) => {
                "Binance socket route refused; trying the next route".into()
            }
        };
        cx.notice(Level::Warn, code::WS_DISCONNECTED, message);
    }

    /// The channel's desired streams; the engine replays them on every open
    /// and sends only the deltas while connected.
    fn interests(&self, cx: &Cx<'_, Pending>, channel: Channel) -> Vec<Interest> {
        let mut topics = Vec::new();
        match channel {
            Channel::Public => {
                if self.is_spot() {
                    topics.extend(self.kline_topics());
                } else {
                    if !self.options.public_data_only {
                        topics.push("!bookTicker".to_owned());
                    }
                    if self.shared_public_socket() {
                        topics.push("!ticker@arr".to_owned());
                        topics.push("!markPrice@arr@1s".to_owned());
                        topics.extend(self.kline_topics());
                    }
                }
                topics.extend(self.depth_topics());
            }
            Channel::Market => {
                topics.push("!ticker@arr".to_owned());
                topics.push("!markPrice@arr@1s".to_owned());
                topics.extend(self.kline_topics());
            }
            Channel::SpotTicker(index) => topics.extend(
                cx.store
                    .markets()
                    .iter()
                    .skip(index * 1_024)
                    .take(1_024)
                    .map(|market| format!("{}@ticker", market.symbol.to_lowercase())),
            ),
            Channel::Private => {}
        }
        topics.into_iter().map(Interest::from_wire_args).collect()
    }

    fn sync_interests(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        let Some(feed) = self.connection_ref(channel).feed else {
            return;
        };
        let interests = self.interests(cx, channel);
        if let Err(error) = cx.set_socket_interests(feed, interests) {
            socket_failed(cx, error);
        }
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
        // A frame-fed engine loads public instrument metadata only: its data
        // source supplies the account, leverage brackets and tickers in frames.
        if cx.frame_fed() {
            return;
        }
        if self.credentials.has_primary() {
            self.start_account(cx);
        }
        if was_loaded {
            self.catalogue_wallet_repair(cx);
            // New markets join their existing shards; later shards connect after the seed.
            if self.is_spot() && previous_len < cx.store.markets().len() {
                for shard in previous_len / 1_024..self.spot_tickers.len() {
                    self.sync_interests(cx, Channel::SpotTicker(shard));
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
        if self.public.idle() {
            self.open_feed(cx, Channel::Public);
        }
        if self.is_spot() {
            while self.spot_tickers.len() < cx.store.markets().len().div_ceil(1_024) {
                let index = self.spot_tickers.len();
                self.spot_tickers.push(Connection::new(true));
                self.open_feed(cx, Channel::SpotTicker(index));
            }
        } else if !self.shared_public_socket() && self.market.idle() {
            self.open_feed(cx, Channel::Market);
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
            self.sync_interests(cx, Channel::Public);
        }
    }

    fn unsubscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        state.subscribers -= 1;
        if state.subscribers == 0 {
            self.books.remove(symbol);
            self.sync_interests(cx, Channel::Public);
        }
    }

    fn on_klines(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        candles: Vec<Candle>,
        response: Response<'_>,
    ) {
        if response.status != 200 {
            // The table gives the kind and native code; the text stays as it was.
            let mut error = account::read_error(&response);
            error.message = format!("Binance klines HTTP {}: {}", response.status, response.body);
            let _ = cx.fail_context(&context, error);
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
    fn managed_sockets() -> bool {
        true
    }
    // An Unknown placement fences its symbol until a fresh, clean open-orders
    // snapshot (`reconcile_orders`) proves what landed.
    fn export_venue_state_for(&self, store: &safe_cex_engine::Store) -> Option<Vec<u8>> {
        Some(self.export_venue_state(store))
    }
    fn on_frame_ingested(&mut self, cx: &mut Cx<'_, Pending>) {
        self.prune_venue_state(cx.store);
    }
    fn apply_venue_state(&mut self, cx: &mut Cx<'_, Pending>, state: &[u8]) {
        if !BinanceFutures::apply_venue_state(self, state) {
            cx.venue_state_unreadable("Unreadable Binance venue state from the data source");
        }
    }
    fn fence_uncertain_placements(&self) -> bool {
        true
    }
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _symbols: Option<&[String]>) {
        if self.is_options() {
            self.options_reconcile(cx);
        } else {
            self.reconcile_orders(cx);
        }
    }
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
            Variant::Options => safe_cex_engine::Capabilities {
                options: true,
                fill_history: true,
                order_history: true,
                income_history: false,
                fills_by_order_id: true,
                ..safe_cex_engine::Capabilities::NO_HISTORY
            },
            Variant::CoinM => safe_cex_engine::Capabilities {
                quantity_units: safe_cex_engine::QuantityUnits::InverseContractSized,
                balance_info: true,
                ..safe_cex_engine::Capabilities::NO_HISTORY
            },
        }
    }

    fn on_venue_action(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: safe_cex_engine::CommandId,
        action: safe_cex_engine::VenueAction,
    ) {
        match action {
            // TS marks a COIN-M balance coin-margined, so an inverse market whose
            // margin coin has no row reads zero instead of the USD total.
            safe_cex_engine::VenueAction::FetchBalanceInfo
                if self.options.variant == Variant::CoinM =>
            {
                let Ok(context) = Self::read_context(cx, None) else {
                    return;
                };
                let _ = cx.done_context(&context, r#"{"coinMargined":true}"#.into());
            }
            _ => safe_cex_engine::unsupported_venue_action(cx, command),
        }
    }

    fn refresh_settings(&mut self, cx: &mut Cx<'_, Pending>, symbol: Option<&str>) {
        self.reread_settings(cx, symbol);
    }

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.is_options() {
            self.start_options(cx);
            return;
        }
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
        if self.is_options() {
            self.options_observed(cx);
        }
        self.revalue_pending_wallet(cx);
    }

    // Every feed is engine-managed: opens, closes and liveness arrive as socket events.
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}

    /// The engine delivers only frames of an open, current, unexpired epoch.
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(channel) = self.channel_of(socket) else {
            return;
        };
        if self.is_spot() && channel != Channel::Private {
            if event_type(text) == Some("serverShutdown") {
                self.retire(cx, channel, RetireReason::Closed);
                return;
            }
            if event_type(text).is_none()
                && let Ok(ack) = serde_json::from_str::<SubscriptionAck>(text)
                && let Some(id) = ack.id
            {
                let pending = self.connection(channel).subscriptions.get_mut();
                if let Some(at) = pending.iter().position(|request| *request == id) {
                    pending.swap_remove(at);
                    if ack.code.is_some_and(|code| code != 0) {
                        cx.notice(
                            Level::Error,
                            code::SUBSCRIPTION_REJECTED,
                            format!("Binance Spot subscription rejected: {}", ack.msg),
                        );
                        self.retire(cx, channel, RetireReason::Closed);
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

    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}

    fn socket_capabilities() -> SocketCapabilities {
        // No Binance work waits on a subscription release; continuations compile out.
        SocketCapabilities { handoff: false }
    }

    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        let feed = match &event {
            LifecycleEvent::Opened(epoch)
            | LifecycleEvent::Ready { epoch, .. }
            | LifecycleEvent::Authenticated { epoch, .. }
            | LifecycleEvent::AuthFailure { epoch, .. }
            | LifecycleEvent::Retired { epoch, .. }
            | LifecycleEvent::Ping(epoch)
            | LifecycleEvent::RetryLogin(epoch) => epoch.feed(),
            LifecycleEvent::Reconnect(feed)
            | LifecycleEvent::InterestsCommitted { feed, .. }
            | LifecycleEvent::InterestsRefused { feed, .. } => *feed,
        };
        let Some(channel) = self.channel_of_feed(feed) else {
            return Err(SocketError::Stale);
        };
        match event {
            LifecycleEvent::Opened(epoch) => {
                let connection = self.connection(channel);
                connection.socket = Some(epoch.socket());
                connection.open = true;
                connection.subscriptions.get_mut().clear();
                match channel {
                    Channel::Public => {
                        for state in self.books.values_mut() {
                            state.book.clear();
                            state.sync = Sync::Idle;
                        }
                    }
                    Channel::Private => self.on_private_open(cx),
                    Channel::Market | Channel::SpotTicker(_) => {}
                }
            }
            // The engine has replayed the last desired set; this adds what changed.
            LifecycleEvent::Ready { .. } if channel != Channel::Private => {
                self.sync_interests(cx, channel)
            }
            LifecycleEvent::Ping(_) => self.probe(cx, channel),
            LifecycleEvent::Reconnect(_) => {
                self.connection(channel).parked = false;
                self.open_feed(cx, channel);
            }
            LifecycleEvent::Retired {
                reason, close_code, ..
            } => self.on_socket_retired(cx, channel, reason, close_code),
            // A refused set is reported; the feed keeps serving its other streams.
            LifecycleEvent::InterestsRefused {
                error: SocketError::InvalidInterest,
                ..
            } if self.is_spot() => cx.notice(
                Level::Error,
                code::SUBSCRIPTION_REJECTED,
                "Binance Spot stream name exceeds the control frame limit",
            ),
            LifecycleEvent::InterestsRefused { error, .. } => socket_failed(cx, error),
            _ => {}
        }
        Ok(())
    }

    /// `{"method","params","id"}` frames: Spot's within its native control
    /// frame bound, the others within the engine's.
    fn encode_socket_interests(
        &self,
        _store: &safe_cex_engine::Store,
        feed: FeedId,
        subscribe: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        let connection = self
            .channel_of_feed(feed)
            .map(|channel| self.connection_ref(channel))
            .ok_or(SocketError::Stale)?;
        let method = if subscribe {
            "SUBSCRIBE"
        } else {
            "UNSUBSCRIBE"
        };
        let (max_topics, max_bytes) = if self.is_spot() {
            (SPOT_FRAME_TOPICS, SPOT_FRAME_BYTES)
        } else {
            (usize::MAX, MAX_FRAME_BYTES)
        };
        let mut start = 0;
        while start < interests.len() {
            let mut end = start;
            let mut bytes = FRAME_ENVELOPE_BYTES;
            while end < interests.len() && end - start < max_topics {
                let topic = serde_json::to_string(interests[end].wire_args.as_ref())
                    .map_err(|_| SocketError::InvalidInterest)?;
                if topic.len() + FRAME_ENVELOPE_BYTES >= max_bytes {
                    return Err(SocketError::InvalidInterest);
                }
                if end > start && bytes + topic.len() + 1 >= max_bytes {
                    break;
                }
                bytes += topic.len() + 1;
                end += 1;
            }
            let chunk = &interests[start..end];
            let id = connection
                .next_request_id
                .get()
                .checked_add(1)
                .ok_or(SocketError::Capacity)?;
            connection.next_request_id.set(id);
            if self.is_spot() {
                connection.subscriptions.borrow_mut().push(id);
            }
            let params: Vec<&str> = chunk.iter().map(|i| i.wire_args.as_ref()).collect();
            emit(EncodedBatch {
                text: format!(
                    "{{\"method\":\"{method}\",\"params\":{},\"id\":{id}}}",
                    serde_json::to_string(&params).map_err(|_| SocketError::InvalidInterest)?
                ),
                covered_keys: chunk.iter().map(|i| i.key.clone()).collect(),
            })?;
            start = end;
        }
        Ok(())
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Background(context, pending) = pending {
            let _ = cx.finish_http_context(&context);
            self.on_response(cx, *pending, response);
            return;
        }
        if let Pending::Options(request) = pending {
            self.options_response(cx, *request, response);
            return;
        }
        if let Pending::OptionsProduct(request) = pending {
            self.options_product_response(cx, *request, response);
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
            Pending::Identity(_) | Pending::Options(_) | Pending::OptionsProduct(_) => {
                unreachable!()
            }
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
            Pending::Options(request) => self.options_failed(cx, *request, error),
            Pending::OptionsProduct(request) => {
                self.options_product_failed(cx, *request, error.into())
            }
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
        if self.is_options() {
            self.options_timer(cx, timer);
        }
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
        self.on_account_timer(cx, timer);
        self.on_history_timer(cx, timer);
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if self.is_options() {
            self.options_observed(cx);
        }
        let command = if self.is_options() {
            match self.options_product_command(cx, command) {
                Ok(()) => return,
                Err(command) => command,
            }
        } else {
            command
        };
        if self.is_options()
            && matches!(
                command,
                Command::SubscribeOrderBook { .. }
                    | Command::SubscribeCandles { .. }
                    | Command::UnsubscribeOrderBook { .. }
                    | Command::UnsubscribeCandles { .. }
            )
        {
            cx.notice(
                Level::Error,
                code::COMMAND_UNSUPPORTED,
                "Binance Options streaming is not supported; use REST queries",
            );
            return;
        }
        if let Command::FetchAccount { .. } = command {
            if self.is_options() {
                cx.refuse_command(
                    Operation::Read,
                    safe_cex_engine::VenueError::new(
                        "Binance Options account identity lookup is unsupported",
                    )
                    .kind(safe_cex_engine::ErrorKind::Unsupported),
                );
                return;
            }
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
                    self.sync_interests(cx, self.candle_channel());
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol, interval);
                if let Some(count) = self.candles.get_mut(&key) {
                    *count -= 1;
                    if *count == 0 {
                        self.candles.remove(&key);
                        self.sync_interests(cx, self.candle_channel());
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
            Command::Query { command: _, query } if self.is_options() => {
                self.options_query(cx, query)
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
