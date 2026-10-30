//! Bybit V5 products and compatible HyroTrader accounts on the sans-I/O engine.
mod account;
mod candles;
mod closed;
mod history;
pub mod hosted_signing;
mod hyro;
mod identity;
mod options;
mod paths;
mod policy;
mod refresh;
mod sign;
mod trading;
mod wire;

use paths::Paths;
pub use paths::{Category, Endpoints, Variant};
use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::credential_selection::{CredentialRole, Selection, SlotPolicy, Slots};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Candle, DerivativeProduct, InstrumentClassification, Interval, Limits, Market, MinMax,
    Precision, ProductKind, Ticker,
};
use safe_cex_engine::socket::{Config, Timeout};
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, Control, ControlPolicy, EncodedBatch, FeedId, FeedPolicy, Interest,
    Invalidation, LifecycleEvent, MAX_INTERESTS, RetireReason, SocketCapabilities, SocketError,
    TrafficPolicy,
};
use safe_cex_engine::time;
use safe_cex_engine::{Command, Cx, Method, PublicFeedPolicy, Response, SocketId, TimerId, Venue};
use safe_cex_engine::{ErrorKind, VenueError};
use sign::{Credentials, Query};
use std::collections::{HashMap, HashSet};
use wire::*;

/// What a signed request's key is for; `BybitLinear::select_key` turns it
/// into an engine `Slots::select` role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyUse {
    /// Any configured key, the read key first.
    Read,
    /// The read key only beside a primary (option Greeks, ordinary Plan reads).
    AccountRead,
    Write,
}

/// Bybit answers pings on public and private sockets alike; a socket silent
/// for the idle timeout is treated as dead, since a half-open socket never closes.
const SOCKET: Config = Config {
    connect_timeout_ms: 10_000.0,
    auth_timeout_ms: None,
    idle_timeout_ms: 20_000.0,
    ping_interval_ms: Some(10_000.0),
    backoff_base_ms: 500.0,
    backoff_cap_ms: 30_000.0,
    stable_ms: 30_000.0,
};
/// A book that skipped an update is unsubscribed, then subscribed again for a
/// fresh snapshot this long after.
const RESUBSCRIBE_DELAY_MS: f64 = 100.0;
/// Bybit closes a public socket that subscribes too fast: ten topics per
/// message, one message per pace interval.
const TOPICS_PER_SUBSCRIBE: usize = 10;
const SUBSCRIBE_PACE_MS: f64 = 10.0;
const MARKETS_PAGE_LIMIT: u32 = 1000;
const PRELAUNCH_STARTUP_GRACE_MS: f64 = 1_500.0;
const MARKETS_PATH: &[&str] = &["result", "list"];
/// Diagnostics must not leave a definitive startup rejection waiting on another host.
const ENVIRONMENT_PROBE_TIMEOUT_MS: u32 = 2_000;

#[derive(Clone, Default)]
pub struct Options {
    pub signing_policy: safe_cex_engine::signing_policy::RemoteSigningPolicy,
    /// Optional tighter signing receive window in milliseconds.
    pub recv_window: Option<u32>,
    pub read_credentials: Option<safe_cex_engine::signing::Identity>,
    pub variant: Variant,
    pub endpoints: Option<Endpoints>,
    pub testnet: bool,
    /// Regional accounts must never send an auth diagnostic to a global environment.
    pub bybit_site_id: Option<String>,
    /// Cross-environment diagnostics are limited to ordinary local signing.
    pub remote_signing: bool,
    pub enable_inverse_markets: bool,
    pub enable_option_markets: bool,
    /// Markets, tickers, books and candles only; no account.
    pub public_data_only: bool,
    pub credentials: Option<safe_cex_engine::signing::Identity>,
}
safe_cex_crypto::redacted_debug!(Options);
/// One REST request as sent, kept so a rejected signature can be re-read.
pub struct Rest {
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
}

/// What to do with a response. Startup lists and klines stream in element by
/// element (see `Cx::request_streamed`); their continuations collect the rows.
pub enum Then {
    Identity(safe_cex_engine::request_context::Context),
    Markets {
        prelaunch: bool,
        markets: Vec<Market>,
        rejected: bool,
    },
    Fees(HashMap<String, (f64, f64)>),
    RefreshMarkets(refresh::Page),
    /// Diagnostic only: bypass ordinary retries, startup and account success handling.
    EnvironmentProbe,
    Tickers(Vec<Ticker>),
    OptionSeed {
        base_coin: String,
        tickers: Vec<Ticker>,
    },
    Options(options::Request),
    Candles(candles::Read),
    Account(account::Request),
    /// A Hyro link-filtered reconcile read.
    Hyro(hyro::Lookup),
    /// An ordinary write or mode read on the engine's Preflight/Plan core.
    Ordinary(trading::ordinary::Request),
    History(history::Request),
    Closed(Box<closed::Request>),
}

pub struct Pending {
    category: Category,
    rest: Rest,
    then: Then,
    /// Background Read authority of a managed discovery request.
    read: Option<safe_cex_engine::request_context::Context>,
}

struct EnvironmentProbe {
    category: Category,
    /// Background Read authority with the probe's native 2 s window.
    context: safe_cex_engine::request_context::Context,
    message: String,
}

// Require native integer success before attributing a key to another environment.
#[derive(serde::Deserialize)]
struct EnvironmentProbeStatus {
    #[serde(rename = "retCode")]
    ret_code: i64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Channel {
    Public,
    Private,
}

/// One managed feed; the engine owns its epochs, timers, retries and pacing.
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    feed: Option<FeedId>,
    /// Whether this Ready's ticker cap was already reported.
    tickers_capped: bool,
}

impl Connection {
    fn new() -> Self {
        Self {
            socket: None,
            open: false,
            feed: None,
            tickers_capped: false,
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
    trading_rejected: bool,
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
    backoff: safe_cex_engine::startup::Backoff<()>,
}

struct ProductState {
    category: Category,
    public: Connection,
    startup: Startup,
    account: account::AccountState,
}

pub struct BybitLinear {
    /// A Bybit Brazil (`BRA_BTL`) account: REST writes publish their own order
    /// rows, as on Demo, until its proxied private WS is venue-validated.
    brazil_site: bool,
    signing_policy: safe_cex_engine::signing_policy::RemoteSigningPolicy,
    recv_window: Option<u32>,
    category: Category,
    primary_category: Category,
    products: Vec<ProductState>,
    catalogs: HashMap<Category, Vec<Market>>,
    ticker_seeds: HashMap<Category, Vec<Ticker>>,
    catalog_ready: bool,
    tickers_ready: bool,
    /// The optional option product failed to load; the others publish without it.
    option_degraded: bool,
    published_catalogs: usize,
    published_seeds: usize,
    classification: HashMap<String, InstrumentClassification>,
    derivatives: HashMap<String, DerivativeProduct>,
    wallet: Option<account::Wallet>,
    orders_loaded: HashSet<Category>,
    positions_loaded: HashSet<Category>,
    variant: Variant,
    paths: Paths,
    config_error: Option<String>,
    credentials: Option<Credentials>,
    read_credentials: Option<safe_cex_engine::signing::Identity>,
    /// Set once a startup read proved the key unusable; startup never retries it.
    credentials_rejected: bool,
    environment_probe_origin: Option<&'static str>,
    environment_probe_attempted: bool,
    environment_probe: Option<EnvironmentProbe>,
    public: Connection,
    private: Connection,
    books: HashMap<String, BookState>,
    book_levels: BookLevels,
    /// Kline subscriptions by (symbol, interval), counted.
    candles: HashMap<(String, Interval), u32>,
    /// Live bar per subscription, extended by trades between native klines.
    live_candles: HashMap<(String, Interval), Candle>,
    trade_subscribers: HashMap<String, u32>,
    startup: Startup,
    /// Discovery retries wait out a rate-limit or IP ban across all products.
    discovery_paused_until: f64,
    account: account::AccountState,
    trading: trading::TradingState,
    history: history::HistoryState,
    /// The raw first account of the last good REST wallet read: TS `balance.info`.
    balance_info: Option<String>,
    market_refresh: refresh::State,
}

/// Bybit kline interval codes.
fn interval_code(interval: Interval) -> Option<&'static str> {
    use Interval::*;
    Some(match interval {
        M1 => "1",
        M3 => "3",
        M5 => "5",
        M15 => "15",
        M30 => "30",
        H1 => "60",
        H2 => "120",
        H4 => "240",
        H6 => "360",
        H12 => "720",
        D1 => "D",
        W1 => "W",
        Mo1 => "M",
        _ => return None,
    })
}

/// Seconds intervals have no native kline; they are built from trades alone.
fn seconds_interval(interval: Interval) -> Option<f64> {
    use Interval::*;
    matches!(interval, S1 | S5 | S15 | S30).then(|| interval.ms() / 1000.0)
}

/// The bar start (seconds) holding `time`: Monday weeks and UTC calendar months
/// for `1w`/`1M`, matching the native `W`/`M` klines; epoch floors otherwise.
fn candle_start(interval: Interval, time: f64) -> f64 {
    const DAY: f64 = 86_400.0;
    match interval {
        Interval::W1 => ((time - 4.0 * DAY) / (7.0 * DAY)).floor() * 7.0 * DAY + 4.0 * DAY,
        Interval::Mo1 => {
            let (year, month, _) = time::civil_from_days((time / DAY).floor() as i64);
            time::days_from_civil(year, month, 1) as f64 * DAY
        }
        _ => {
            let step = interval.ms() / 1000.0;
            (time / step).floor() * step
        }
    }
}

/// Definitive key rejections: invalid key, bad signature after the re-signed
/// resend, IP not allowed, expired key. 10005 (permission) is left out: a key
/// lacking only this read's scope may still trade.
const CREDENTIAL_REJECTIONS: [i64; 4] = [10003, 10004, 10010, 33004];
/// Bybit's 10010 "Unmatched IP" refusal: the key's IP allowlist excludes this egress.
const IP_REFUSED_HINT: &str = "Bybit refused this API key from this IP address: the key is restricted to other IPs. Add the current Tealstreet IPs to the key's IP allowlist on Bybit, or create a new key";

/// A command read's native refusal, classified so it keeps its kind.
pub(crate) fn read_refusal(response: &Response<'_>, prefix: &str) -> Option<VenueError> {
    use safe_cex_engine::classify::{self, Native, NativeCode, Released};
    let (code, message) = venue_error(response)?;
    // Only a parsed retCode is native evidence; a bare HTTP error is not.
    let parsed = serde_json::from_str::<Status>(response.body)
        .ok()
        .and_then(|status| status.ret_code)
        .is_some();
    let native = parsed.then_some(Native {
        code: NativeCode::Int(code),
        message: &message,
    });
    Some(
        classify::read(
            &trading::codes::BYBIT,
            Released::Response(response),
            native.as_ref(),
            format!("{prefix}{message}"),
        )
        .error,
    )
}

/// A page failure that reached the public dispatch as text.
fn page_failure(response: Result<Response<'_>, &str>) -> VenueError {
    match response {
        Err(error) => VenueError::new(error).kind(ErrorKind::Network),
        Ok(_) => VenueError::new("Bybit page answer lost its read leg").kind(ErrorKind::Internal),
    }
}

/// Each page of a command read has its own window inside the caller's.
pub(crate) const PAGE_WINDOW_MS: u32 = 20_000;

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
            // Signature diagnostics can contain the complete signed origin string.
            Some((code, signing_rejection(code, &message).unwrap_or(message)))
        }
        None => Some((
            i64::from(response.status),
            format!("HTTP {}", response.status),
        )),
    }
}

fn discovery_rate_limit_pause_ms(response: &Response<'_>, now: f64) -> f64 {
    use safe_cex_engine::startup::{MAX_RATE_LIMIT_PAUSE_MS, rate_limit_pause_ms};

    // HTTP403 is Bybit's ten-minute IP ban; 10006 is its API rate-limit code.
    // https://bybit-exchange.github.io/docs/v5/rate-limit
    let http_pause = if response.status == 403 {
        600_000.0
    } else {
        rate_limit_pause_ms(response, 10_000.0)
    };
    if !matches!(venue_error(response), Some((10006, _))) {
        return http_pause;
    }
    let reset = response
        .header("x-bapi-limit-reset-timestamp")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|at| at.is_finite() && *at > now)
        .map(|at| (at - now).min(MAX_RATE_LIMIT_PAUSE_MS));
    // Reuse the HTTP rate-limit parser for an envelope-level limit without
    // changing the original response passed to the venue's continuation.
    let limited = Response {
        status: 429,
        headers: response.headers,
        body: response.body,
    };
    rate_limit_pause_ms(&limited, reset.unwrap_or(10_000.0))
        .max(reset.unwrap_or(0.0))
        .max(http_pause)
}

pub(crate) fn signing_rejection(code: i64, message: &str) -> Option<String> {
    let reason = if code == 10004 || message.to_ascii_lowercase().contains("error sign") {
        "request signature rejected"
    } else if code == 10002 || message.to_ascii_lowercase().contains("recv_window") {
        "request timestamp outside receive window"
    } else {
        return None;
    };
    Some(format!("Bybit {reason} (retCode {code})"))
}

impl BybitLinear {
    pub fn new(options: Options) -> Self {
        let mut options = options;
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
                            prelaunch: (category == Category::Option).then(Vec::new),
                            fees: (category == Category::Option).then(HashMap::new),
                            ..Startup::default()
                        },
                        account: account::AccountState::new(),
                    });
                }
            }
        }
        let paths = Paths::for_options(&options);
        let config_error = options.validate().err();
        let environment_probe_origin = if !options.testnet
            && !options.remote_signing
            && options.endpoints.is_none()
            && options.bybit_site_id.as_deref().is_none_or(str::is_empty)
        {
            match (options.variant, paths.rest.as_str()) {
                (Variant::Linear, "https://api.bybit.com") => Some("https://api-demo.bybit.com"),
                (Variant::Demo, "https://api-demo.bybit.com") => Some("https://api.bybit.com"),
                _ => None,
            }
        } else {
            None
        };
        let credentials = (!options.public_data_only)
            .then(|| options.credentials.take())
            .flatten()
            .filter(|identity| identity.validate().is_ok());
        Self {
            signing_policy: options.signing_policy,
            category,
            primary_category: category,
            products,
            catalogs: HashMap::new(),
            ticker_seeds: HashMap::new(),
            catalog_ready: false,
            tickers_ready: false,
            option_degraded: false,
            published_catalogs: 0,
            published_seeds: 0,
            classification: HashMap::new(),
            derivatives: HashMap::new(),
            wallet: None,
            orders_loaded: HashSet::new(),
            positions_loaded: HashSet::new(),
            variant: options.variant,
            paths,
            config_error,
            recv_window: options.recv_window,
            credentials,
            read_credentials: options.read_credentials.take(),
            credentials_rejected: false,
            environment_probe_origin,
            brazil_site: options.bybit_site_id.as_deref() == Some("BRA_BTL"),
            environment_probe_attempted: false,
            environment_probe: None,
            public: Connection::new(),
            private: Connection::new(),
            books: HashMap::new(),
            book_levels: BookLevels::default(),
            candles: HashMap::new(),
            live_candles: HashMap::new(),
            trade_subscribers: HashMap::new(),
            startup: Startup {
                prelaunch: options.variant.is_spot().then(Vec::new),
                fees: (options.variant.is_spot() || options.variant.is_hyro()).then(HashMap::new),
                ..Startup::default()
            },
            discovery_paused_until: 0.0,
            account: account::AccountState::new(),
            trading: trading::TradingState::default(),
            history: history::HistoryState::default(),
            balance_info: None,
            market_refresh: refresh::State::default(),
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

    /// Public startup discovery on its own Background Read with a 20 s window.
    fn send_discovery(&mut self, cx: &mut Cx<'_, Pending>, rest: Rest, then: Then) {
        self.send_read(cx, rest, then);
    }

    /// One Background Read with a 20 s window: public, or signed with the read
    /// key for GETs (else the primary). A read that cannot be admitted (invalid
    /// options, no key, refused admission or queue) fails into the caller's path.
    fn send_read(&mut self, cx: &mut Cx<'_, Pending>, rest: Rest, then: Then) {
        use safe_cex_engine::request_context::Scope;
        if self.config_error.is_some() {
            return self.on_public_response(cx, then, Err("Bybit options are invalid"));
        }
        let Some(policy) = self.read_policy(rest.signed, policy::read_policy().timeout_ms) else {
            return self.on_public_response(
                cx,
                then,
                Err("Bybit has no API key for a signed read"),
            );
        };
        let context = match cx.http_context(Scope::Background, policy) {
            Ok(context) => context,
            Err(error) => {
                self.note_local_read_refusal();
                return self.on_public_response(cx, then, Err(&error.venue_error().message));
            }
        };
        self.queue_read(cx, context, rest, then);
    }

    /// Read authority: public, or signed with the read key for GETs (else the
    /// primary); `None` when a signed read has no credential. A signed read is
    /// re-read once after a native signature refusal.
    pub(crate) fn read_policy(
        &self,
        signed: bool,
        timeout_ms: u32,
    ) -> Option<safe_cex_engine::request_context::Policy> {
        use safe_cex_engine::request_context::{Policy, RetryPolicy};
        let credential = if signed {
            let (selection, identity) = self.select_key(KeyUse::Read)?;
            Some(selection.credential(identity.credential.clone()))
        } else {
            None
        };
        Some(Policy {
            credential,
            timeout_ms,
            retry: if signed {
                RetryPolicy::ReadFailure
            } else {
                RetryPolicy::Never
            },
            ..policy::read_policy()
        })
    }

    /// A command read's caller root: the receipt deadline bounds every page.
    /// `None` when admission already answered the command.
    pub(crate) fn caller_read_root(
        &self,
        cx: &mut Cx<'_, Pending>,
        signed: bool,
    ) -> Option<safe_cex_engine::request_context::Context> {
        use safe_cex_engine::request_context::CommandEntryError;
        let credential = self
            .read_policy(signed, 1)
            .and_then(|policy| policy.credential);
        match cx.admit_caller_read_root(credential, None) {
            Ok(root) => Some(root),
            Err(CommandEntryError::Rejected(_)) => None,
            Err(CommandEntryError::Revoked) => {
                let _ =
                    cx.reject_expired_command(safe_cex_engine::request_context::Operation::Read);
                None
            }
        }
    }

    /// One page of a command read: a 20 s managed leg of its caller root.
    pub(crate) fn page_leg(
        &self,
        cx: &mut Cx<'_, Pending>,
        root: &safe_cex_engine::request_context::Context,
        signed: bool,
    ) -> Result<safe_cex_engine::request_context::Context, VenueError> {
        let policy = self.read_policy(signed, PAGE_WINDOW_MS).ok_or_else(|| {
            VenueError::new("Bybit signed read has no credential").kind(ErrorKind::AuthInvalid)
        })?;
        cx.http_leg_context(root, policy)
            .map_err(|error| error.venue_error())
    }

    /// The read key signs GETs when one is set; the primary signs the rest.
    pub(crate) fn signing_identity(
        &self,
        method: Method,
    ) -> Option<&safe_cex_engine::signing::Identity> {
        let key = if method == Method::Get {
            KeyUse::Read
        } else {
            KeyUse::Write
        };
        self.select_key(key).map(|(_, identity)| identity)
    }

    /// The only key choice: the engine's slot rules over Bybit's two keys.
    pub(crate) fn select_key(
        &self,
        key: KeyUse,
    ) -> Option<(Selection, &safe_cex_engine::signing::Identity)> {
        let policy = SlotPolicy {
            secondary_requires_primary: key == KeyUse::AccountRead,
        };
        let role = match key {
            KeyUse::Read | KeyUse::AccountRead => CredentialRole::Read { scope: None },
            KeyUse::Write => CredentialRole::Mutation,
        };
        let slots = Slots::new(
            self.credentials.as_ref(),
            self.read_credentials.as_ref(),
            policy,
        );
        let (selection, identity) = slots.select(role).ok()?;
        Some((selection, *identity))
    }

    pub(crate) fn queue_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: safe_cex_engine::request_context::Context,
        rest: Rest,
        then: Then,
    ) {
        let mut url = format!("{}{}", self.paths.rest, rest.path);
        if !rest.payload.is_empty() {
            url.push('?');
            url.push_str(&rest.payload);
        }
        let signing = rest.signed.then(|| {
            let identity = self
                .signing_identity(Method::Get)
                .expect("a signed read was admitted with a credential");
            sign::Http::new(identity.clone(), rest.payload.clone(), self.recv_window)
                .with_header_policy(self.signing_policy.header_policy())
        });
        let stream = rest.stream;
        let pending = Pending {
            category: self.category,
            rest,
            then,
            read: Some(context.clone()),
        };
        let queued = match signing {
            Some(signing) => cx.request_signed_managed(
                context,
                None,
                Method::Get,
                url,
                None,
                Box::new(signing),
                pending,
            ),
            None => cx.request_managed_with_headers(
                context,
                None,
                Method::Get,
                url,
                &[("X-Referer", sign::BROKER_ID)],
                None,
                pending,
            ),
        };
        match queued {
            Ok(request) if stream => cx.stream_response(request, ArrayAt::Path(MARKETS_PATH)),
            Ok(_) => {}
            Err((pending, error)) => match pending.then {
                // Nothing was sent: the typed refusal is the command's.
                Then::Identity(context) => {
                    let error = error
                        .venue_error()
                        .outcome(safe_cex_engine::Outcome::NotSent);
                    self.identity_response(cx, context, Err(error));
                }
                // A page refused before any send keeps the engine's typed error.
                Then::History(request) => {
                    let error = error
                        .venue_error()
                        .outcome(safe_cex_engine::Outcome::NotSent);
                    self.on_history_failed(cx, request, error);
                }
                Then::Closed(request) => {
                    let error = error
                        .venue_error()
                        .outcome(safe_cex_engine::Outcome::NotSent);
                    self.on_closed_failed(cx, *request, error);
                }
                then => {
                    self.note_local_read_refusal();
                    self.on_public_response(cx, then, Err(&error.venue_error().message));
                }
            },
        }
    }

    /// Re-read a signed read once on a native 10002/10004 refusal, through the
    /// shared classifier; text alone never re-reads.
    fn reread(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: &mut Option<Pending>,
        response: &Response<'_>,
    ) -> bool {
        use safe_cex_engine::classify::{self, Native, NativeCode, ReadRetry, Released};
        use safe_cex_engine::request_context::RetryEvidence;
        let Some(current) = pending.as_ref().filter(|p| p.rest.signed && !p.rest.resent) else {
            return false;
        };
        let Some(read) = current.read.clone() else {
            return false;
        };
        let Some((code, message)) = venue_error(response) else {
            return false;
        };
        let parsed = serde_json::from_str::<Status>(response.body)
            .ok()
            .and_then(|status| status.ret_code)
            .is_some();
        let native = parsed.then_some(Native {
            code: NativeCode::Int(code),
            message: &message,
        });
        let failure = classify::read(
            &trading::codes::BYBIT,
            Released::Response(response),
            native.as_ref(),
            message.clone(),
        );
        if failure.retry != Some(ReadRetry::NativeResend) {
            return false;
        }
        let Ok(retry) = cx.http_retry_context(&read, RetryEvidence::ReadFailure) else {
            return false;
        };
        // Bybit names its own clock in the raw refusal, before any sanitizing.
        if let Some(server) = trading::ordinary::server_time(response.body) {
            cx.observe_protocol_time(server);
        }
        let mut taken = pending.take().expect("checked above");
        taken.rest.resent = true;
        self.queue_read(cx, retry, taken.rest, taken.then);
        true
    }

    fn open_feed(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        if channel == Channel::Private && !cx.private_feed_policy().streaming() {
            return;
        }
        if channel == Channel::Public && cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return;
        }
        let url = match channel {
            Channel::Public => self.paths.public_ws.clone(),
            Channel::Private => self.paths.private_ws.clone(),
        };
        let feed = match self.connection(channel).feed {
            Some(feed) => {
                if cx.socket_epoch(feed).is_some() {
                    return;
                }
                feed
            }
            None => {
                let registered = cx.register_socket_feed(FeedPolicy {
                    role: match channel {
                        Channel::Public => safe_cex_engine::SocketChannel::Public,
                        Channel::Private => safe_cex_engine::SocketChannel::Private,
                    },
                    supervision: SOCKET,
                    auth: AuthMode::None,
                    recovery: AuthRecovery::Reconnect,
                    permanent_refusal_group: None,
                    traffic: TrafficPolicy::AnyText,
                    controls: ControlPolicy::Native,
                    invalidation: Invalidation::Required,
                    subscription_interval_ms: SUBSCRIBE_PACE_MS,
                    close_floors: &[],
                });
                match registered {
                    Ok(feed) => {
                        self.connection(channel).feed = Some(feed);
                        feed
                    }
                    Err(error) => return self.socket_failed(cx, error),
                }
            }
        };
        if let Err(error) = cx.connect_socket(feed, url) {
            self.socket_failed(cx, error);
        }
    }

    fn socket_failed(&self, cx: &mut Cx<'_, Pending>, error: SocketError) {
        cx.notice(
            Level::Error,
            code::SUBSCRIPTION_REJECTED,
            format!("Bybit socket admission failed: {error:?}"),
        );
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
        if channel == Channel::Private {
            let category = self.category;
            for other in self.categories() {
                self.select_category(other);
                self.on_private_closed(cx);
            }
            self.select_category(category);
        }
        let message = match (reason, close_code) {
            (RetireReason::Closed, Some(code)) => {
                format!("Bybit socket closed ({code}); reconnecting")
            }
            (RetireReason::Timeout(Timeout::Idle), _) => {
                "Bybit socket closed (idle timeout); reconnecting".into()
            }
            (RetireReason::Timeout(_), _) => "Bybit socket timed out; reconnecting".into(),
            (RetireReason::Authentication, _) => {
                "Bybit socket authentication failed; reconnecting".into()
            }
            (RetireReason::Closed, None) => "Bybit socket closed; reconnecting".into(),
            (RetireReason::Stopped, _) => "Bybit socket stopped".into(),
            (RetireReason::RouteExhausted, _) => "Bybit socket refused on every route".into(),
            (RetireReason::RouteRefused, _) => {
                "Bybit socket route refused; trying the next route".into()
            }
        };
        cx.notice(Level::Warn, code::WS_DISCONNECTED, message);
    }

    /// The feed's channel, with its category selected.
    fn select_feed(&mut self, feed: FeedId) -> Option<Channel> {
        if self.private.feed == Some(feed) {
            self.select_category(self.primary_category);
            return Some(Channel::Private);
        }
        if self.public.feed == Some(feed) {
            return Some(Channel::Public);
        }
        let category = self
            .products
            .iter()
            .find(|state| state.public.feed == Some(feed))
            .map(|state| state.category)?;
        self.select_category(category);
        Some(Channel::Public)
    }

    /// The current category's ticker topics from the store.
    fn ticker_topics(&self, cx: &Cx<'_, Pending>) -> Vec<String> {
        cx.store
            .markets()
            .iter()
            .filter(|m| self.owns_symbol(&m.symbol))
            .map(|m| format!("tickers.{}", m.symbol))
            .collect()
    }

    /// The current category's public topics: live books, candles, trades, then
    /// tickers. Tickers are capped so the requested topics always fit the
    /// engine's interest bound; the bool reports a cap.
    fn public_interests(&self, cx: &Cx<'_, Pending>) -> (Vec<Interest>, bool) {
        let mut topics: Vec<String> = Vec::new();
        let spot = self.variant.is_spot();
        for (symbol, state) in &self.books {
            if (!spot && Category::for_symbol(symbol) != self.category)
                || matches!(state.sync, Sync::Resubscribing(_))
            {
                continue;
            }
            topics.push(Self::book_topic(self.paths.book_depth, symbol));
        }
        topics.extend(
            self.candles
                .keys()
                .filter(|(symbol, _)| self.owns_symbol(symbol))
                .filter_map(|(symbol, interval)| Self::kline_topic(symbol, *interval)),
        );
        topics.extend(
            self.trade_subscribers
                .keys()
                .filter(|symbol| self.owns_symbol(symbol))
                .map(|symbol| format!("publicTrade.{symbol}")),
        );
        let tickers = self.ticker_topics(cx);
        let room = MAX_INTERESTS.saturating_sub(topics.len());
        let capped = tickers.len() > room;
        topics.extend(tickers.into_iter().take(room));
        (
            topics.into_iter().map(Interest::from_wire_args).collect(),
            capped,
        )
    }

    fn sync_public_interests(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(feed) = self.public.feed.filter(|_| self.public.open) else {
            return;
        };
        let (interests, capped) = self.public_interests(cx);
        if capped && !self.public.tickers_capped {
            self.public.tickers_capped = true;
            cx.notice(
                Level::Warn,
                code::SUBSCRIPTION_REJECTED,
                "Bybit public ticker subscriptions exceed the socket interest limit; some tickers are not streamed",
            );
        }
        if let Err(error) = cx.set_socket_interests(feed, interests) {
            self.socket_failed(cx, error);
        }
    }

    /// Topic changes reach the venue as one desired set; the engine encodes
    /// and paces the subscribe/unsubscribe deltas.
    fn subscribe(&mut self, cx: &mut Cx<'_, Pending>, _op: &str, _topics: &[String]) {
        self.sync_public_interests(cx);
    }

    fn book_topic(depth: u32, symbol: &str) -> String {
        format!("orderbook.{depth}.{symbol}")
    }

    fn kline_topic(symbol: &str, interval: Interval) -> Option<String> {
        Some(format!("kline.{}.{symbol}", interval_code(interval)?))
    }

    fn request_markets(&mut self, cx: &mut Cx<'_, Pending>, prelaunch: bool, cursor: Option<&str>) {
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
        self.send_discovery(
            cx,
            Rest::get("/v5/market/instruments-info", query).streamed(),
            Then::Markets {
                prelaunch,
                markets,
                rejected: false,
            },
        );
    }

    fn request_startup(&mut self, cx: &mut Cx<'_, Pending>) {
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
        if cx.frame_fed() {
            return;
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
            self.send_discovery(
                cx,
                Rest::get("/v5/market/tickers", query).streamed(),
                Then::Tickers(Vec::new()),
            );
        }
        if !self.startup.markets_loaded
            && self.credentials.is_some()
            && !self.credentials_rejected
            && self.environment_probe.is_none()
            && self.startup.fees.is_none()
            && !self.startup.fees_in_flight
        {
            self.startup.fees_in_flight = true;
            let query = Query::new().str("category", self.paths.category);
            self.send_read(
                cx,
                Rest::get("/v5/account/fee-rate", query).streamed().signed(),
                Then::Fees(HashMap::new()),
            );
        }
    }

    /// The signed startup fee read proved the key unusable; retrying cannot
    /// fix it, so startup fails instead of waiting for an account that never loads.
    /// Only the trading key's own invalid, bad-signature or expired verdicts
    /// deactivate: an IP mismatch or a rejected read-only key leaves it usable.
    fn reject_credentials(&mut self, cx: &mut Cx<'_, Pending>, code: i64, message: &str) {
        let hint = (code == 10010).then_some(IP_REFUSED_HINT);
        self.reject_credentials_with_hint(cx, code, message, hint);
    }

    fn reject_credentials_with_hint(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        code: i64,
        message: &str,
        hint: Option<&str>,
    ) {
        if std::mem::replace(&mut self.credentials_rejected, true) {
            return;
        }
        if let Some(probe) = self.environment_probe.take() {
            let _ = cx.finish_http_context(&probe.context);
        }
        let kind = match code {
            33004 => ErrorKind::AuthExpired,
            10010 => ErrorKind::AuthPermission,
            _ => ErrorKind::AuthInvalid,
        };
        // Signed GETs use the read-only key when one is configured.
        let read_key = self.read_credentials.is_some();
        let which = if read_key {
            "read-only API key"
        } else {
            "API key"
        };
        let diagnostic =
            hint.map_or_else(|| message.to_owned(), |hint| format!("{hint} [{message}]"));
        let text = format!("Bybit rejected the {which} ({code}): {diagnostic}");
        cx.notice(Level::Error, code::AUTH_REJECTED, &text);
        cx.startup_failed(
            VenueError::new(text)
                .kind(kind)
                .venue_code(code.to_string())
                .venue_message(message),
        );
        if !read_key && code != 10010 {
            cx.deactivate();
        }
    }

    /// Probe only the credential used by the rejected startup GET. Never switch origins
    /// for normal reads or writes, and never log the probe's key-bearing response.
    fn probe_rejected_environment(&mut self, cx: &mut Cx<'_, Pending>, message: &str) -> bool {
        if self.environment_probe.is_some() {
            return true; // Concurrent product fee reads share the same rejected GET key.
        }
        if self.credentials_rejected || self.environment_probe_attempted {
            return false;
        }
        let Some(origin) = self.environment_probe_origin else {
            return false;
        };
        // Owned, so the probe can mark its attempt while it holds the key.
        let Some((selection, credentials)) = self
            .select_key(KeyUse::Read)
            .map(|(selection, identity)| (selection, identity.clone()))
        else {
            return false;
        };
        if credentials.validate().is_err() {
            return false;
        }
        let signing = sign::Http::new(credentials.clone(), String::new(), self.recv_window)
            .with_header_policy(self.signing_policy.header_policy());
        self.environment_probe_attempted = true;
        use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy, Scope};
        let Ok(context) = cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: Some(selection.credential(credentials.credential.clone())),
                timeout_ms: ENVIRONMENT_PROBE_TIMEOUT_MS,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) else {
            return false;
        };
        let pending = Pending {
            category: self.category,
            rest: Rest::get("/v5/user/query-api", Query::new()).signed(),
            then: Then::EnvironmentProbe,
            read: None,
        };
        if cx
            .request_signed_managed(
                context.clone(),
                None,
                Method::Get,
                format!("{origin}/v5/user/query-api"),
                None,
                Box::new(signing),
                pending,
            )
            .is_err()
        {
            let _ = cx.finish_http_context(&context);
            return false;
        }
        self.environment_probe = Some(EnvironmentProbe {
            category: self.category,
            context,
            message: message.to_owned(),
        });
        true
    }

    fn finish_environment_probe(&mut self, cx: &mut Cx<'_, Pending>, confirmed: bool) {
        // Taking the state retires timeout/late HTTP races before any terminal notice.
        let Some(probe) = self.environment_probe.take() else {
            return;
        };
        let _ = cx.finish_http_context(&probe.context);
        self.select_category(probe.category);
        let hint = confirmed.then_some(if self.variant == Variant::Demo {
            "Bybit API key environment mismatch: this key is valid for live Bybit, but this account is configured for Bybit Demo Trading. Create a Demo Trading API key or switch the account to live Bybit."
        } else {
            "Bybit API key environment mismatch: this key is valid for Bybit Demo Trading, but this account is configured for live Bybit. Create a live Bybit API key or switch the account to Bybit Demo Trading."
        });
        self.reject_credentials_with_hint(cx, 10003, &probe.message, hint);
    }

    /// A required startup list failed: try the missing ones again, backing off.
    fn retry_startup(&mut self, cx: &mut Cx<'_, Pending>, what: &str, error: &str) {
        cx.notice(
            Level::Warn,
            code::REQUEST_FAILED,
            format!("Failed to fetch Bybit {what}: {error}"),
        );
        if self.category == Category::Option && !self.option_degraded {
            self.option_degraded = true;
            // Publish the working products now; options join when they load.
            let category = self.category;
            for other in self.categories() {
                self.select_category(other);
                self.try_publish_catalogs(cx);
                self.try_load_tickers(cx);
            }
            self.select_category(category);
            // Seeds already loaded elsewhere skip try_load_tickers; publish them too.
            self.try_publish_tickers(cx);
        }
        let pause = self.discovery_paused_until - cx.now;
        self.startup.backoff.failed_after(cx, (), pause);
    }

    fn on_markets(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        prelaunch: bool,
        mut markets: Vec<Market>,
        rejected: bool,
        response: Result<Response<'_>, &str>,
    ) {
        if self.market_refresh.retired_startup {
            return;
        }
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
        if !prelaunch {
            self.startup.trading_rejected |= rejected;
        }
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
        if !prelaunch && list.is_empty() && self.startup.trading_rejected {
            self.markets_failed(cx, false, "Trading catalogue has no usable markets");
            return;
        }
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
            cx.notice(Level::Warn, code::REQUEST_FAILED,
                format!("Failed to fetch Bybit PreLaunch markets, continuing with Trading markets: {error}"),
            );
            self.startup.prelaunch = Some(Vec::new());
            self.try_load_markets(cx);
        } else {
            self.startup.trading_in_flight = false;
            self.startup.trading_pages.clear();
            self.startup.trading_rejected = false;
            self.retry_startup(cx, "Trading markets", error);
        }
    }

    fn try_load_markets(&mut self, cx: &mut Cx<'_, Pending>) {
        let startup = &mut self.startup;
        let fees_ready = self.credentials.is_none() || startup.fees.is_some() || cx.frame_fed();
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
        cx.notice(
            Level::Info,
            code::STARTUP_PROGRESS,
            format!("Loaded {} Bybit markets", markets.len()),
        );
        self.catalogs.insert(self.category, markets);
        if self.category == Category::Option && !cx.frame_fed() {
            self.request_option_seeds(cx);
        }
        self.try_publish_catalogs(cx);
    }

    /// Categories whose public data must be in before publishing: every enabled
    /// one, except a failed option product, which joins later.
    pub(crate) fn required_categories(&self) -> Vec<Category> {
        self.categories()
            .into_iter()
            .filter(|c| !(self.option_degraded && *c == Category::Option))
            .collect()
    }

    fn try_publish_catalogs(&mut self, cx: &mut Cx<'_, Pending>) {
        let complete = self.catalogs.len() == self.products.len() + 1;
        let usable = self
            .required_categories()
            .iter()
            .all(|c| self.catalogs.contains_key(c));
        // Option sorts last, so a late option catalog only appends market indexes.
        let late_option = self.catalog_ready && complete;
        if !usable
            || (self.catalog_ready && !late_option)
            || self.published_catalogs == self.catalogs.len()
        {
            return;
        }
        self.published_catalogs = self.catalogs.len();
        {
            let mut markets = Vec::new();
            for category in self.categories() {
                markets.extend(self.catalogs.get(&category).into_iter().flatten().cloned());
            }
            cx.store.set_markets(markets);
            publish_product_metadata(cx.store, &self.classification, &self.derivatives);
            self.catalog_ready = true;
            if cx.frame_fed() {
                return;
            }
            let category = self.category;
            for other in self.categories() {
                self.select_category(other);
                self.try_start_account(cx);
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
            self.send_discovery(
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
        self.startup.backoff.succeeded(cx, &());
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
        self.try_publish_tickers(cx);
    }

    fn try_publish_tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        let usable = self
            .required_categories()
            .iter()
            .all(|c| self.ticker_seeds.contains_key(c));
        let late_option = self.tickers_ready
            && self.ticker_seeds.len() == self.products.len() + 1
            && self.published_seeds < self.ticker_seeds.len();
        if !usable
            || (self.tickers_ready && !late_option)
            || self.published_seeds == self.ticker_seeds.len()
        {
            return;
        }
        self.published_seeds = self.ticker_seeds.len();
        if self.option_degraded && self.ticker_seeds.contains_key(&Category::Option) {
            self.option_degraded = false;
            cx.notice(
                Level::Info,
                code::STARTUP_PROGRESS,
                "Bybit option markets recovered",
            );
        }
        // Published categories keep their live rows; a late option only appends.
        let mut tickers = if late_option {
            cx.store.tickers().to_vec()
        } else {
            Vec::new()
        };
        for category in self.categories() {
            if late_option && category != Category::Option {
                continue;
            }
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
        self.reclassify_unpriced_tpsl(cx);
        let category = self.category;
        for other in self.categories() {
            // A category without seeded data waits; a connected one is already live.
            if !self.ticker_seeds.contains_key(&other) || (late_option && other != Category::Option)
            {
                continue;
            }
            self.select_category(other);
            self.open_feed(cx, Channel::Public);
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
                cx.notice(
                    Level::Error,
                    code::SUBSCRIPTION_REJECTED,
                    format!("[WS]{op}:{message}"),
                );
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
        } else if topic.starts_with("publicTrade.")
            && let Ok(event) = serde_json::from_str::<TradeEvent>(text)
        {
            self.on_trades(cx, &event);
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
            .find(|(s, i)| s == symbol && interval_code(*i) == Some(code))
            .map(|(_, interval)| *interval)
        else {
            return;
        };
        let key = (symbol.to_owned(), interval);
        for k in &event.data {
            let native = Candle {
                timestamp: k.start / 1000.0,
                open: float(&k.open),
                high: float(&k.high),
                low: float(&k.low),
                close: float(&k.close),
                volume: float(&k.volume),
            };
            let candle = match self.live_candles.get(&key) {
                // Never move a live bar backwards.
                Some(live) if live.timestamp > native.timestamp => continue,
                // Keep trade extremes the kline has not caught up with; its volume is cumulative.
                Some(live) if live.timestamp == native.timestamp => Candle {
                    high: live.high.max(native.high),
                    low: live.low.min(native.low),
                    ..native
                },
                _ => native,
            };
            self.live_candles.insert(key.clone(), candle);
            cx.candle(symbol, key.1, candle);
        }
    }

    /// Extend each live bar of this symbol with every trade in the packet, in order.
    fn on_trades(&mut self, cx: &mut Cx<'_, Pending>, event: &TradeEvent<'_>) {
        let Some(symbol) = event.topic.strip_prefix("publicTrade.") else {
            return;
        };
        let trades: Vec<(f64, f64, f64)> = event
            .data
            .iter()
            .map(|t| (t.time / 1000.0, float(&t.price), float(&t.size)))
            .collect();
        if trades.iter().any(|(time, price, size)| {
            !(time.is_finite() && price.is_finite() && *price > 0.0 && size.is_finite())
        }) {
            return;
        }
        let intervals: Vec<Interval> = self
            .candles
            .keys()
            .filter(|(s, _)| s == symbol)
            .map(|(_, interval)| *interval)
            .collect();
        for interval in intervals {
            let key = (symbol.to_owned(), interval);
            let seconds = seconds_interval(key.1).is_some();
            let mut live = self.live_candles.get(&key).copied();
            let before = live;
            for &(time, price, size) in &trades {
                let start = candle_start(key.1, time);
                let bar = match live {
                    // Minute and longer bars start from a native kline.
                    None if !seconds => continue,
                    None => Candle {
                        timestamp: start,
                        open: price,
                        high: price,
                        low: price,
                        close: price,
                        volume: 0.0,
                    },
                    Some(bar) if bar.timestamp > start => continue,
                    Some(bar) if bar.timestamp < start => Candle {
                        timestamp: start,
                        open: bar.close,
                        high: bar.close,
                        low: bar.close,
                        close: bar.close,
                        volume: 0.0,
                    },
                    Some(bar) => bar,
                };
                live = Some(Candle {
                    high: bar.high.max(price),
                    low: bar.low.min(price),
                    close: price,
                    volume: bar.volume + size,
                    ..bar
                });
            }
            if let Some(candle) = live.filter(|c| Some(*c) != before) {
                self.live_candles.insert(key.clone(), candle);
                cx.candle(symbol, key.1, candle);
            }
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
        interval: Interval,
        add: bool,
    ) {
        if self.category_for(&symbol) == Category::Option
            || !self.category_enabled(self.category_for(&symbol))
        {
            cx.notice(
                Level::Warn,
                code::COMMAND_UNSUPPORTED,
                "Bybit has no candles for this product",
            );
            return;
        }
        let trades = !self.variant.is_hyro();
        let kline = Self::kline_topic(&symbol, interval);
        // Hyro has no trade feed, so its seconds intervals stay unsupported.
        if kline.is_none() && !(trades && seconds_interval(interval).is_some()) {
            cx.notice(
                Level::Error,
                code::COMMAND_UNSUPPORTED,
                interval.unsupported("Bybit"),
            );
            return;
        }
        let key = (symbol.clone(), interval);
        if add {
            let count = self.candles.entry(key).or_insert(0);
            *count += 1;
            if *count == 1 {
                let mut topics: Vec<String> = kline.into_iter().collect();
                if trades {
                    let subscribers = self.trade_subscribers.entry(symbol.clone()).or_insert(0);
                    *subscribers += 1;
                    if *subscribers == 1 {
                        topics.push(format!("publicTrade.{symbol}"));
                    }
                }
                self.subscribe(cx, "subscribe", &topics);
            }
        } else if let Some(count) = self.candles.get_mut(&key) {
            *count -= 1;
            if *count == 0 {
                self.candles.remove(&key);
                self.live_candles.remove(&key);
                let mut topics: Vec<String> = kline.into_iter().collect();
                if trades && let Some(subscribers) = self.trade_subscribers.get_mut(&symbol) {
                    *subscribers -= 1;
                    if *subscribers == 0 {
                        self.trade_subscribers.remove(&symbol);
                        topics.push(format!("publicTrade.{symbol}"));
                    }
                }
                self.subscribe(cx, "unsubscribe", &topics);
            }
        }
    }
}

fn on_item_public(
    cx: &mut Cx<'_, Pending>,
    then: &mut Then,
    item: &str,
    variant: Variant,
    category: Category,
) {
    {
        match then {
            Then::Markets {
                markets, rejected, ..
            } => {
                let Ok(m) = decode_instrument(item) else {
                    *rejected = true;
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                };
                if (!variant.is_hyro()
                    || (m.quote_coin == "USDT"
                        && (m.contract_type.is_empty() || m.contract_type == "LinearPerpetual")))
                    && (category != Category::Option
                        || (m.status == "Trading" && derivative_from_wire(&m, category).is_some()))
                {
                    let market = market_from_wire(&m, variant, category);
                    if !market.precision.is_tradable() {
                        *rejected = true;
                        safe_cex_engine::startup::skipped_row(cx);
                        return;
                    }
                    markets.push(market);
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
                let Ok(t) = serde_json::from_str::<TickerRow>(item) else {
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                };
                {
                    tickers.push(ticker_from_rest(
                        &t,
                        variant == Variant::Linear && category == Category::Inverse,
                    ));
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
        if let Ok(response) = &response
            && matches!(
                then,
                Then::Markets { .. } | Then::Tickers(_) | Then::OptionSeed { .. }
            )
        {
            let pause = discovery_rate_limit_pause_ms(response, cx.now);
            if pause > 0.0 {
                self.discovery_paused_until = self.discovery_paused_until.max(cx.now + pause);
                let remaining = self.discovery_paused_until - cx.now;
                // Categories share this limit. Extend already-armed retries;
                // waiting entries retain their attempt count and longer waits.
                for startup in std::iter::once(&mut self.startup)
                    .chain(self.products.iter_mut().map(|product| &mut product.startup))
                {
                    if startup.backoff.waiting(&()) {
                        startup.backoff.failed_after(cx, (), remaining);
                    }
                }
            }
        }
        match then {
            Then::RefreshMarkets(page) => match response {
                Ok(response) => self.refresh_response(cx, page, response),
                Err(error) => self.refresh_failed(cx, page, error),
            },
            Then::EnvironmentProbe => self.finish_environment_probe(cx, false),
            Then::Identity(context) => {
                let response =
                    response.map_err(|error| VenueError::new(error).kind(ErrorKind::Internal));
                self.identity_response(cx, context, response)
            }
            Then::Markets {
                prelaunch,
                markets,
                rejected,
            } => self.on_markets(cx, prelaunch, markets, rejected, response),
            Then::Fees(fees) => {
                self.startup.fees_in_flight = false;
                let rejected = response
                    .as_ref()
                    .ok()
                    .and_then(venue_error)
                    .filter(|(code, _)| CREDENTIAL_REJECTIONS.contains(code));
                if let Some((code, message)) = rejected {
                    if code == 10003 && self.probe_rejected_environment(cx, &message) {
                        return;
                    }
                    return self.reject_credentials(cx, code, &message);
                }
                if let Some(error) = failure {
                    cx.notice(
                        Level::Warn,
                        code::REQUEST_FAILED,
                        format!("Bybit fee rates: {error}"),
                    );
                }
                self.startup.fees = Some(fees);
                self.try_load_markets(cx);
            }
            Then::Tickers(tickers) => {
                self.startup.tickers_in_flight = false;
                match failure {
                    Some(error) => self.retry_startup(cx, "tickers", &error),
                    None if tickers.is_empty() => {
                        self.retry_startup(cx, "tickers", "No usable ticker rows")
                    }
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
            // Managed option reads settle in on_options_response before this
            // public path; kept typed so no caller can leave one unanswered.
            Then::Options(request) => {
                let response =
                    response.map_err(|error| VenueError::new(error).kind(ErrorKind::Internal));
                self.on_options_response(cx, request, response)
            }
            // Managed kline reads never reach the legacy public path.
            Then::Candles(_) => {}
            Then::Account(request) => self.on_account_response(cx, request, response),
            Then::Hyro(read) => self.on_hyro_response(cx, read, response),
            // Ordinary responses and failures never reach the public path.
            Then::Ordinary(_) => {}
            // Answers arrive with their page leg in on_response; only failures here.
            Then::History(request) => self.on_history_failed(cx, request, page_failure(response)),
            Then::Closed(request) => self.on_closed_failed(cx, *request, page_failure(response)),
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

// Startup and explicit refresh use one decoder without sharing their admission policy.
#[inline(never)]
fn decode_instrument(item: &str) -> serde_json::Result<Instrument<'_>> {
    serde_json::from_str(item)
}

#[inline(never)]
fn classification_from_wire(row: &Instrument<'_>) -> InstrumentClassification {
    InstrumentClassification {
        market: 0,
        symbol_type: row.symbol_type.as_ref().map(ToString::to_string),
        market_region: row.market_region.as_ref().map(ToString::to_string),
        underlying_ticker: row.underlying_ticker.as_ref().map(ToString::to_string),
    }
}

// Keep metadata aligned with the just-published market table in both discovery paths.
#[inline(never)]
fn publish_product_metadata(
    store: &mut safe_cex_engine::Store,
    classifications: &HashMap<String, InstrumentClassification>,
    derivatives: &HashMap<String, DerivativeProduct>,
) {
    let classifications = store
        .markets()
        .iter()
        .enumerate()
        .filter_map(|(index, market)| {
            let mut row = classifications.get(&market.symbol)?.clone();
            row.market = index as u32;
            Some(row)
        })
        .collect();
    store.set_instrument_classifications(classifications);
    let derivatives = store
        .markets()
        .iter()
        .enumerate()
        .filter_map(|(index, market)| {
            let mut row = derivatives.get(&market.symbol)?.clone();
            row.market = index as u32;
            Some(row)
        })
        .collect();
    store.set_derivative_products(derivatives);
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
        if self.startup.backoff.on_timer(timer).is_some() {
            self.request_startup(cx);
        }
        if self.startup.prelaunch_grace == Some(timer) {
            self.startup.prelaunch_grace = None;
            self.startup.prelaunch_in_flight = false;
            self.startup.prelaunch_pages.clear();
            self.startup.prelaunch = Some(Vec::new());
            cx.notice(
                Level::Warn,
                code::REQUEST_FAILED,
                "Timed out waiting for Bybit PreLaunch markets, continuing with Trading markets",
            );
            self.try_load_markets(cx);
        }
        self.on_account_timer(cx, timer);
    }
}

impl Venue for BybitLinear {
    fn managed_sockets() -> bool {
        true
    }
    // Bybit never hands work to a subscription release; its continuation paths compile out.
    fn socket_capabilities() -> SocketCapabilities {
        SocketCapabilities { handoff: false }
    }
    fn on_http_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &safe_cex_engine::request_context::Preflight,
    ) {
        self.hyro_waiter_ready(cx, preflight);
    }

    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        plan: &safe_cex_engine::request_context::Plan,
    ) {
        self.on_ordinary_plan_ready(cx, plan);
    }

    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.on_ordinary_retired(owner);
    }

    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        self.ordinary_reconciliation(pending)
    }

    fn read_root_retired(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
        _pending: Vec<Pending>,
    ) {
        // Closed-position owners travel in their pending and drop with it.
        self.retire_history(owner);
    }

    fn reconcile_http(
        &self,
        _cx: &mut safe_cex_engine::request_context::Reconciliation<'_>,
        data: &safe_cex_engine::request_context::ReconciliationData,
        response: Response<'_>,
    ) {
        self.hyro_late_ack(data.as_str(), &response);
    }

    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending.then {
            Then::Ordinary(request) => self.on_ordinary_leg_failed(cx, request, error),
            Then::Candles(read) => Self::candles_failed(cx, read, error),
            Then::Identity(context) => self.identity_response(cx, context, Err(error)),
            Then::Options(request) => self.on_options_response(cx, request, Err(error)),
            Then::History(request) => self.on_history_failed(cx, request, error),
            Then::Closed(request) => self.on_closed_failed(cx, *request, error),
            then => self.on_request_failed(
                cx,
                Pending {
                    category: pending.category,
                    rest: pending.rest,
                    then,
                    read: pending.read,
                },
                &error.message,
            ),
        }
    }

    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _symbols: Option<&[String]>) {
        self.reconcile_ordinary(cx);
    }

    // reconcile_uncertain reads every product's orders at once; the engine
    // re-asks until a clean fresh read clears the fence. HyroTrader keeps its
    // own same-intent retry, which recovers identity through link reads.
    fn fence_uncertain_placements(&self) -> bool {
        !self.variant.is_hyro()
    }

    fn cancel_needs_stored_order(&self) -> bool {
        true
    }

    type Pending = Pending;

    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            position_history: if self.variant.is_spot() || self.variant == Variant::Inverse {
                safe_cex_engine::PositionHistory::Unsupported
            } else {
                safe_cex_engine::PositionHistory::Bybit
            },
            closed_position_format: if self.variant.is_spot() {
                safe_cex_engine::ClosedPositionFormat::Unsupported
            } else {
                safe_cex_engine::ClosedPositionFormat::Bybit
            },
            bybit_execution_controls: true,
            reject_client_slippage: self.variant.is_hyro(),
            inverse_closed_positions: self.variant == Variant::Inverse,
            options: self.category_enabled(Category::Option),
            // HyroTrader exposes no identity endpoint; `fetch_identity` refuses it.
            account: if self.variant.is_hyro() {
                safe_cex_engine::AccountLookup::Unsupported
            } else {
                safe_cex_engine::AccountLookup::Query
            },
            income_history: !self.variant.is_spot() && !self.variant.is_hyro(),
            closed_positions: !self.variant.is_spot(),
            balance_info: true,
            market_refresh: true,
            ..safe_cex_engine::Capabilities::BASE
        }
    }

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(error) = self
            .credentials
            .iter()
            .chain(self.read_credentials.iter())
            .find_map(|identity| identity.validate().err())
        {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            return;
        }
        if let Some(error) = &self.config_error {
            cx.notice(Level::Error, code::CONFIG_INVALID, error.clone());
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
        // A frame-fed engine loads public instrument metadata only.
        if self.credentials.is_some() && !cx.frame_fed() {
            self.request_account_info(cx);
        }
    }

    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}

    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.select_socket_category(socket);
        let Some(channel) = self.channel_of(socket) else {
            return;
        };
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

    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}

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
        let Some(channel) = self.select_feed(feed) else {
            return Err(SocketError::Stale);
        };
        match event {
            LifecycleEvent::Opened(epoch) => {
                let connection = self.connection(channel);
                connection.socket = Some(epoch.socket());
                connection.open = true;
                if channel == Channel::Private {
                    self.on_private_open(cx, epoch.socket());
                }
            }
            LifecycleEvent::Ready { .. } if channel == Channel::Public => {
                let category = self.category;
                let spot = self.variant.is_spot();
                for (symbol, state) in &mut self.books {
                    // Only the reopened feed's category resyncs; other feeds stay live.
                    if !spot && Category::for_symbol(symbol) != category {
                        continue;
                    }
                    state.book.clear();
                    if let Sync::Resubscribing(timer) = state.sync {
                        cx.clear_timer(timer);
                    }
                    state.sync = Sync::Waiting;
                }
                self.public.tickers_capped = false;
                self.sync_public_interests(cx);
            }
            LifecycleEvent::Ping(epoch) => cx.send(epoch.socket(), r#"{"op":"ping"}"#),
            LifecycleEvent::Reconnect(_) => self.open_feed(cx, channel),
            LifecycleEvent::Retired {
                reason, close_code, ..
            } => self.on_socket_retired(cx, channel, reason, close_code),
            LifecycleEvent::InterestsRefused { error, .. } => return Err(error),
            _ => {}
        }
        Ok(())
    }

    fn encode_socket_interests(
        &self,
        _store: &safe_cex_engine::Store,
        _feed: FeedId,
        subscribe: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        let op = if subscribe {
            "subscribe"
        } else {
            "unsubscribe"
        };
        for chunk in interests.chunks(TOPICS_PER_SUBSCRIBE) {
            let topics: Vec<&str> = chunk.iter().map(|i| i.wire_args.as_ref()).collect();
            emit(EncodedBatch {
                text: format!(
                    r#"{{"op":"{op}","args":{}}}"#,
                    serde_json::to_string(&topics).unwrap_or_default()
                ),
                covered_keys: chunk.iter().map(|i| i.key.clone()).collect(),
            })?;
        }
        Ok(())
    }

    fn socket_control(
        &self,
        _: &safe_cex_engine::Store,
        _: FeedId,
        _: bool,
        text: &str,
    ) -> Control {
        if text.contains(r#""op":"pong""#) || text.contains(r#""ret_msg":"pong""#) {
            Control::Pong
        } else {
            Control::Other
        }
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if matches!(&pending.then, Then::RefreshMarkets(_)) {
            self.select_category(pending.category);
            let Then::RefreshMarkets(page) = pending.then else {
                unreachable!()
            };
            self.refresh_response(cx, page, response);
            return;
        }
        if matches!(&pending.then, Then::EnvironmentProbe) {
            // Only the live probe's own delivery may attribute the key; any
            // other answer leaves it unconfirmed, never hanging startup.
            if self
                .environment_probe
                .as_ref()
                .is_none_or(|probe| cx.validate_http_context(&probe.context).is_err())
            {
                self.finish_environment_probe(cx, false);
                return;
            }
            let confirmed = (200..300).contains(&response.status)
                && serde_json::from_str::<EnvironmentProbeStatus>(response.body)
                    .is_ok_and(|status| status.ret_code == 0);
            self.finish_environment_probe(cx, confirmed);
            return;
        }
        self.select_category(pending.category);
        // Ordinary writes resend only on a classified grant, never on text.
        if let Then::Ordinary(request) = pending.then {
            self.on_ordinary_response(cx, request, response);
            return;
        }
        if let Then::Candles(read) = pending.then {
            Self::candles_response(cx, read, &response);
            return;
        }
        if let Then::Options(request) = pending.then {
            self.on_options_response(cx, request, Ok(response));
            return;
        }
        if matches!(pending.then, Then::History(_) | Then::Closed(_)) {
            // A page answers on its own leg; the owner validates that delivery.
            let Some(read) = pending.read.clone() else {
                return;
            };
            let mut pending = Some(pending);
            // A native re-read continues on this leg's retry attempt.
            if self.reread(cx, &mut pending, &response) {
                return;
            }
            match pending.map(|pending| pending.then) {
                Some(Then::History(request)) => {
                    self.on_history_response(cx, request, &read, response);
                }
                Some(Then::Closed(request)) => {
                    self.on_closed_response(cx, *request, &read, response);
                }
                _ => {}
            }
            // The page is handled; its leg retires so a long walk never fills capacity.
            let _ = cx.finish_http_context(&read);
            return;
        }
        if let Some(read) = pending.read.clone() {
            // Only a live Background read may publish; the engine never
            // delivers an answer for a retired attempt.
            if cx.validate_http_context(&read).is_err() {
                return;
            }
            let mut pending = Some(pending);
            if self.reread(cx, &mut pending, &response) {
                return;
            }
            let Some(pending) = pending else {
                return;
            };
            self.on_public_response(cx, pending.then, Ok(response));
            let _ = cx.finish_http_context(&read);
            return;
        }
        self.on_public_response(cx, pending.then, Ok(response));
    }

    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        // A native kline refusal is typed; a success without its list proves nothing.
        if let Then::Candles(read) = pending.then {
            if venue_error(&response).is_some() {
                Self::candles_response(cx, read, &response);
            } else {
                let error =
                    VenueError::new("Bybit klines: response did not contain the expected list")
                        .kind(ErrorKind::Internal);
                Self::candles_failed(cx, read, error);
            }
            return;
        }
        // A signed managed read's signature/timestamp refusal carries no list:
        // classify it (one re-read) and fail with the sanitized message.
        if pending.rest.signed
            && pending.read.is_some()
            && matches!(venue_error(&response), Some((10002 | 10004, _)))
        {
            return self.on_response(cx, pending, response);
        }
        // A native option refusal is classified; a success without its list is malformed.
        if let Then::Options(request) = pending.then {
            let response = if venue_error(&response).is_some() {
                Ok(response)
            } else {
                Err(
                    VenueError::new("Bybit options: response did not contain the expected list")
                        .kind(ErrorKind::Internal),
                )
            };
            self.on_options_response(cx, request, response);
            return;
        }
        // Fee, refresh and discovery-limit failures can carry `result: {}`.
        // Preserve typed limit evidence and headers; successful discovery still
        // requires its streamed list.
        if matches!(pending.then, Then::Fees(_) | Then::RefreshMarkets(_))
            || (matches!(
                pending.then,
                Then::Markets { .. } | Then::Tickers(_) | Then::OptionSeed { .. }
            ) && matches!(venue_error(&response), Some((10006, _))))
        {
            return self.on_response(cx, pending, response);
        }
        self.on_request_failed(
            cx,
            pending,
            &format!(
                "response did not contain the expected list: {}",
                response.body
            ),
        );
    }

    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.select_category(pending.category);
        if let Some(read) = &pending.read
            && cx.validate_http_context(read).is_err()
        {
            return;
        }
        if self.market_refresh.retired_startup && matches!(&pending.then, Then::Markets { .. }) {
            return;
        }
        match &mut pending.then {
            Then::Candles(read) => Self::candles_item(cx, read, item),
            Then::RefreshMarkets(page) => self.refresh_item(page, item),
            Then::Account(request) => self.on_account_item(cx, request, item),
            Then::Options(request) => {
                let context = request.context().clone();
                if cx.validate_read_http_delivery(&context, &context).is_ok() {
                    options::on_item(request, item)
                }
            }
            then => {
                if matches!(then, Then::Markets { .. })
                    && let Ok(row) = decode_instrument(item)
                {
                    self.classification
                        .insert(row.symbol.to_string(), classification_from_wire(&row));
                    if let Some(product) = derivative_from_wire(&row, self.category) {
                        self.derivatives.insert(row.symbol.to_string(), product);
                    }
                }
                on_item_public(cx, then, item, self.variant, self.category)
            }
        }
    }

    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        // The engine seals an ordinary preflight or plan unless an
        // independent-order leg retires alone.
        if let Then::Ordinary(request) = pending.then {
            self.on_ordinary_leg_failed(cx, request, error);
            return;
        }
        if let Then::Candles(read) = pending.then {
            Self::candles_failed(cx, read, error);
            return;
        }
        // A managed identity read (first or retried leg) keeps the engine's type.
        if let Then::Identity(context) = pending.then {
            self.identity_response(cx, context, Err(error));
            return;
        }
        // A page or its re-read keeps the signer's typed error.
        if let Then::History(request) = pending.then {
            self.on_history_failed(cx, request, error);
            return;
        }
        if let Then::Closed(request) = pending.then {
            self.on_closed_failed(cx, *request, error);
            return;
        }
        if pending.rest.resent {
            // This refusal concerns only the retry. The earlier dispatched
            // attempt keeps the existing conservative failure/recovery path.
            self.on_request_failed(cx, pending, "Bybit retry signing failed before dispatch");
            return;
        }
        self.select_category(pending.category);
        match pending.then {
            Then::Hyro(read) => self.on_hyro_response(cx, read, Err(&error.message)),
            Then::Identity(context) => self.identity_response(cx, context, Err(error)),
            Then::EnvironmentProbe => self.finish_environment_probe(cx, false),
            Then::Options(request) => self.on_options_response(cx, request, Err(error)),
            then => self.on_public_response(cx, then, Err(&error.message)),
        }
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Then::Ordinary(request) = pending.then {
            // Transport loss arrives typed through on_http_failed; this is a
            // pre-transport (signing) failure.
            let error = VenueError::new(error).outcome(safe_cex_engine::Outcome::NotSent);
            self.on_ordinary_leg_failed(cx, request, error);
            return;
        }
        if let Then::Candles(read) = pending.then {
            let error = VenueError::new(error).outcome(safe_cex_engine::Outcome::NotSent);
            Self::candles_failed(cx, read, error);
            return;
        }
        if matches!(&pending.then, Then::RefreshMarkets(_)) {
            let Then::RefreshMarkets(page) = pending.then else {
                unreachable!()
            };
            self.refresh_failed(cx, page, error);
            return;
        }
        if matches!(&pending.then, Then::EnvironmentProbe) {
            self.finish_environment_probe(cx, false);
            return;
        }
        self.select_category(pending.category);
        let Pending {
            rest, then, read, ..
        } = pending;
        // A rejected retry may arrive as a host error containing the signed origin URL.
        let safe_error = (rest.signed && rest.resent)
            .then_some("Bybit signed request failed after its signature retry");
        self.on_public_response(cx, then, Err(safe_error.unwrap_or(error)));
        if let Some(read) = read {
            let _ = cx.finish_http_context(&read);
        }
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        for category in self.categories() {
            self.select_category(category);
            self.on_category_timer(cx, timer);
        }
    }

    fn on_venue_action(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: safe_cex_engine::CommandId,
        action: safe_cex_engine::VenueAction,
    ) {
        match action {
            // The engine binds the command when the refresh is admitted.
            safe_cex_engine::VenueAction::RefreshMarkets => self.refresh_markets(cx),
            safe_cex_engine::VenueAction::FetchClosedPositions {
                symbol,
                from_ms,
                to_ms,
            } => self.start_closed_positions(cx, command, symbol, from_ms, to_ms),
            safe_cex_engine::VenueAction::FetchBalanceInfo => {
                // A cached answer still needs this command's own authority.
                let Ok(context) = cx.admit_http_command(policy::read_policy()) else {
                    return;
                };
                let info = self.balance_info.clone().unwrap_or_else(|| "null".into());
                let _ = cx.done_context(&context, info);
            }
            _ => safe_cex_engine::unsupported_venue_action(cx, command),
        }
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        // The engine binds the command when the read is admitted.
        if let Command::FetchAccount { command: _ } = command {
            self.fetch_identity(cx);
            return;
        }
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
        // An ordinary trade command fails through its own preflight.
        if !self.category_enabled(category) && self.ordinary_command(&command) {
            self.on_trade_command(cx, command);
            return;
        }
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
            if id.is_some() {
                // A refusal before any I/O still needs this command's authority.
                let operation = if matches!(
                    command,
                    Command::FetchCandles { .. } | Command::Query { .. }
                ) {
                    safe_cex_engine::request_context::Operation::Read
                } else {
                    safe_cex_engine::request_context::Operation::Mutation
                };
                if let Ok(context) = cx.admit_http_command(policy::entry_policy(operation)) {
                    // An account-config precondition, not an unavailable command.
                    let error = safe_cex_engine::VenueError::new("Bybit product is not enabled")
                        .kind(ErrorKind::InvalidInput)
                        .outcome(safe_cex_engine::Outcome::NotSent);
                    let _ = cx.fail_context(&context, error);
                }
            } else {
                cx.notice(
                    Level::Warn,
                    code::COMMAND_UNSUPPORTED,
                    "Bybit product is not enabled",
                );
            }
            return;
        }
        self.select_category(category);
        match command {
            // The engine binds the command when each read is admitted.
            Command::FetchOptionChain { command: _, query } => self.fetch_option_chain(cx, query),
            Command::FetchOptionExpiries {
                command: _,
                underlying,
            } => self.fetch_option_expiries(cx, &underlying),
            Command::FetchAccountGreeks { command: _ } => self.fetch_account_greeks(cx),
            Command::SubscribeOrderBook { symbol } => self.subscribe_book(cx, symbol),
            Command::UnsubscribeOrderBook { symbol } => self.unsubscribe_book(cx, &symbol),
            Command::SubscribeCandles { symbol, interval } => {
                self.on_candle_command(cx, symbol, interval, true)
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                self.on_candle_command(cx, symbol, interval, false)
            }
            // The engine binds the command when the read is admitted.
            Command::FetchCandles {
                command: _,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => self.fetch_candles(cx, &symbol, interval, (limit, from_ms, to_ms)),
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
    use safe_cex_engine::Event;

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
            let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
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
        let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
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

#[cfg(test)]
mod candle_start_tests {
    use super::candle_start;
    use safe_cex_engine::model::Interval;

    #[test]
    fn calendar_bars_match_native_week_and_month_boundaries() {
        // 2026-10-20 12:34:56 UTC is a Tuesday.
        let t = 1_792_499_696.0;
        assert_eq!(candle_start(Interval::W1, t), 1_792_368_000.0); // Monday 2026-10-19
        assert_eq!(candle_start(Interval::Mo1, t), 1_790_812_800.0); // 2026-10-01
        assert_eq!(
            candle_start(Interval::Mo1, 1_709_251_199.0),
            1_706_745_600.0
        ); // 2024-02-29 -> 02-01
        assert_eq!(
            candle_start(Interval::Mo1, 1_704_067_200.0),
            1_704_067_200.0
        ); // 2024-01-01
        assert_eq!(candle_start(Interval::D1, t), 1_792_454_400.0);
        assert_eq!(candle_start(Interval::H4, t), 1_792_497_600.0);
        assert_eq!(candle_start(Interval::S15, t), 1_792_499_685.0);
    }
}
