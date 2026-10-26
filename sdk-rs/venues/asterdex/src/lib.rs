//! Aster perpetuals: typed public streams and versioned REST on the sans-I/O engine.
mod account;
#[cfg(test)]
#[path = "../tests/support/fixture.rs"]
mod fixture;
mod history;
mod request;
pub(crate) use request::command_read_root;
pub mod sign;
mod trading;
mod wire;

#[cfg(test)]
use Options as VenueOptions;
use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::credential_selection::{CredentialRole, Selection, SlotPolicy, Slots};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, ControlPolicy, EncodedBatch, FeedId, FeedPolicy,
    Interest as SocketInterest, Invalidation, LifecycleEvent, MAX_FRAME_BYTES, SocketCapabilities,
    SocketError, TrafficPolicy,
};
use safe_cex_engine::{
    Command, CredentialSlot, Cx, ErrorKind, PublicFeedPolicy, Response, SocketChannel, SocketId,
    TimerId, Venue, VenueError, json, num,
};
use serde_json::value::RawValue;
use sign::Version;
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use wire::*;

const INTEREST_MS: f64 = 15_000.0;

fn local_read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

#[derive(Clone, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    /// Provider refs; trusted setup owns the API key and signing material.
    pub credentials: Option<Identity>,
    /// Read agent refs. V3 setup may alias the primary wallet key for signing.
    pub read_credentials: Option<Identity>,
    /// Resolved by trusted setup from the key material; signing never guesses it.
    pub api_version: Option<Version>,
    /// V3 main wallet: `auth.userAddress`, else `applicationId`.
    pub user_address: Option<String>,
    /// V3 API wallet: `auth.signerAddress`, else the API key.
    pub signer_address: Option<String>,
    /// V3 read agent wallet: `auth.signerAddress`, else the read key.
    pub read_signer_address: Option<String>,
    pub builder_address: Option<String>,
    pub builder_fee_rate_maker: Option<String>,
    pub builder_fee_rate_taker: Option<String>,
}
safe_cex_crypto::redacted_debug!(Options);

struct Urls {
    rest: &'static str,
    public_ws: &'static str,
}

impl Urls {
    fn new(testnet: bool) -> Self {
        if testnet {
            Self {
                rest: "https://fapi.asterdex-testnet.com",
                public_ws: "wss://fstream5.asterdex-testnet.com/stream",
            }
        } else {
            Self {
                rest: "https://fapi.asterdex.com",
                public_ws: "wss://fstream.asterdex.com/stream",
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct MarketRule {
    pub tick_size: f64,
    pub max_algo_orders: Option<u32>,
}

pub enum Pending {
    DepthRead {
        context: Context,
    },
    MarketBrackets {
        rows: HashMap<String, f64>,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    Signed(Box<request::SignedRequest>),
    Clock(request::ClockRequest),
    ExchangeInfo {
        rows: Vec<(Market, MarketRule)>,
        skipped: Vec<String>,
    },
    Daily {
        rows: Vec<Ticker>,
        skipped: Vec<String>,
    },
    OpenInterest {
        rows: HashMap<String, f64>,
        initial: bool,
        single: bool,
    },
    Klines {
        context: Context,
        rows: Vec<Candle>,
        /// The first row that did not decode.
        invalid: Option<String>,
    },
}

/// Both feeds keep the legacy supervisor timings: 20 s idle, 5 s protocol ping.
fn feed_policy(role: SocketChannel, backoff_base_ms: f64) -> FeedPolicy {
    FeedPolicy {
        role,
        supervision: Config {
            idle_timeout_ms: 20_000.0,
            ping_interval_ms: Some(5_000.0),
            backoff_base_ms,
            ..Config::default()
        },
        // The listen key in the URL is the private credential; neither feed logs in.
        auth: AuthMode::None,
        recovery: AuthRecovery::Reconnect,
        permanent_refusal_group: None,
        traffic: TrafficPolicy::AnyText,
        controls: ControlPolicy::None,
        invalidation: Invalidation::Required,
        subscription_interval_ms: 0.0,
    }
}

fn socket_failed(cx: &mut Cx<'_, Pending>, error: SocketError) {
    cx.notice(
        Level::Error,
        code::SUBSCRIPTION_REJECTED,
        format!("AsterDEX socket admission failed: {error:?}"),
    );
}

#[derive(Default)]
struct Connection {
    feed: Option<FeedId>,
    /// The current epoch's socket, from connect until retirement.
    socket: Option<SocketId>,
    /// Subscribe frame ids; never reset, so ids stay unique across reconnects.
    next_request_id: Cell<u64>,
}

#[derive(Default)]
struct BookState {
    subscribers: u32,
    book: Book,
    last_id: Option<u64>,
}

pub struct AsterDex {
    options: Options,
    version: Version,
    credentials: Slots<Identity>,
    /// N1: a slot whose configured V3 wallet differs from the address its
    /// signing key proved at setup is refused, never silently re-addressed.
    signer_mismatch: [Option<String>; 2],
    /// `start` latched the V3 slots from the engine's sealed identities.
    signing_sealed: bool,
    account: account::AccountState,
    trading: trading::TradingState,
    history: history::HistoryState,
    requests: request::RequestState,
    urls: Urls,
    public: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, Interval), u32>,
    market_rules: HashMap<String, MarketRule>,
    interest_timer: Option<TimerId>,
    startup: safe_cex_engine::startup::Backoff<bool>,
    interest_cursor: usize,
    interest_fallback: VecDeque<String>,
}

fn slot_index(slot: CredentialSlot) -> usize {
    match slot {
        CredentialSlot::Primary => 0,
        CredentialSlot::Secondary => 1,
    }
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

impl AsterDex {
    /// Signing slots with today's filters: an incomplete identity cannot
    /// sign, and a read key keeps signing reads for public-data-only accounts.
    fn slots(
        options: &Options,
        primary: Option<Identity>,
        secondary: Option<Identity>,
    ) -> Slots<Identity> {
        let valid = |identity: Option<Identity>| identity.filter(|id| id.validate().is_ok());
        Slots::new(
            valid(primary).filter(|_| !options.public_data_only),
            valid(secondary),
            SlotPolicy::default(),
        )
    }

    pub fn new(options: Options) -> Self {
        // Construction clones answer presence only; V3 signs with the
        // engine's sealed identities, installed at start.
        let credentials = Self::slots(
            &options,
            options.credentials.clone(),
            options.read_credentials.clone(),
        );
        Self {
            signer_mismatch: [None, None],
            signing_sealed: false,
            urls: Urls::new(options.testnet),
            version: options.api_version.unwrap_or_default(),
            options,
            credentials,
            account: account::AccountState::default(),
            trading: trading::TradingState::default(),
            history: history::HistoryState::default(),
            requests: request::RequestState::default(),
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            market_rules: HashMap::new(),
            interest_timer: None,
            startup: Default::default(),
            interest_cursor: 0,
            interest_fallback: VecDeque::new(),
        }
    }

    /// Setup must resolve the version, and the primary needs both V3 wallets.
    /// A read slot's error fails its own requests, as before, not startup.
    fn signing_config_error(&self) -> Option<String> {
        if !self.credentials.has_primary() && !self.credentials.secondary_supplied() {
            return None;
        }
        if self.options.api_version.is_none() {
            return Some("Aster signing requires a resolved API version".into());
        }
        let (selection, _) = self.credentials.select(CredentialRole::Mutation).ok()?;
        self.v3_fields(selection.slot()).err()
    }

    fn configured_signer(&self, slot: CredentialSlot) -> Option<&str> {
        nonempty(match slot {
            CredentialSlot::Primary => self.options.signer_address.as_deref(),
            CredentialSlot::Secondary => self.options.read_signer_address.as_deref(),
        })
    }

    /// The V3 main wallet, after checking the slot's configured wallet fields.
    fn v3_fields(&self, slot: CredentialSlot) -> Result<String, String> {
        if self.version == Version::V1 {
            return Ok(String::new());
        }
        let user = nonempty(self.options.user_address.as_deref());
        let (Some(user), Some(signer)) = (user, self.configured_signer(slot)) else {
            return Err("Missing required Aster v3 auth fields: Main Wallet Address, API Wallet Address, and Generated Private Key".into());
        };
        safe_cex_crypto::ethereum::get_address(user)?;
        safe_cex_crypto::ethereum::get_address(signer)?;
        Ok(user.into())
    }

    /// Checks a role's signing contract without reserving a nonce. A mismatch
    /// is a configuration refusal, never an authentication failure.
    fn check_signing(&self, role: CredentialRole) -> Result<(), (String, ErrorKind)> {
        let (selection, _) = self.credentials.select(role).map_err(|_| {
            (
                "Missing required Aster credentials".to_owned(),
                ErrorKind::AuthInvalid,
            )
        })?;
        if let Some(error) = &self.signer_mismatch[slot_index(selection.slot())] {
            return Err((error.clone(), ErrorKind::InvalidInput));
        }
        self.v3_fields(selection.slot())
            .map(drop)
            .map_err(|error| (error, ErrorKind::AuthInvalid))
    }

    /// The selected identity and its recipe contract. V3 mints a nonce claim
    /// against the wallet address the slot's signing key proved at setup.
    fn signing(&self, role: CredentialRole) -> Result<(Selection, Identity, sign::Auth), String> {
        self.check_signing(role).map_err(|(error, _)| error)?;
        let (selection, identity) = self
            .credentials
            .select(role)
            .map_err(|_| "Missing required Aster credentials".to_owned())?;
        if self.version == Version::V1 {
            return Ok((selection, identity.clone(), sign::Auth::V1));
        }
        let user = self.v3_fields(selection.slot())?;
        let (selection, identity, claim) = self
            .credentials
            .select_with_nonce(role, &safe_cex_engine::nonce::ASTERDEX_V3)
            .map_err(|error| error.sign_error().to_string())?;
        Ok((selection, identity.clone(), sign::Auth::V3 { user, claim }))
    }

    /// Rebuilds V3 slots from the sealed identities and records any slot
    /// whose configured wallet differs from the address its key proved.
    /// Before start, setup may still be initialized or reinitialized, so each
    /// call reads the engine's current identities; only `start` latches them.
    fn seal_signing(&mut self, cx: &mut Cx<'_, Pending>, at_start: bool) {
        if self.version != Version::V3 || self.signing_sealed {
            return;
        }
        self.signing_sealed = at_start;
        self.signer_mismatch = [None, None];
        let sealed = cx.address_nonce_identities().clone();
        self.credentials = Self::slots(
            &self.options,
            sealed.primary.clone(),
            sealed.secondary.clone(),
        );
        for (slot, identity) in [
            (CredentialSlot::Primary, &sealed.primary),
            (CredentialSlot::Secondary, &sealed.secondary),
        ] {
            let (Some(identity), Some(configured)) = (identity, self.configured_signer(slot))
            else {
                continue;
            };
            let (Ok(configured), Ok(proved)) = (
                safe_cex_crypto::ethereum::get_address(configured),
                identity.verified_signing_address(),
            ) else {
                continue;
            };
            let Ok(proved) = safe_cex_crypto::ethereum::get_address(&format!(
                "0x{}",
                safe_cex_crypto::hex(&proved)
            )) else {
                continue;
            };
            if configured != proved {
                let error = format!(
                    "Aster V3 API wallet address {configured} does not match the signing key's address {proved}"
                );
                if at_start {
                    cx.notice(Level::Error, code::CONFIG_INVALID, error.clone());
                }
                self.signer_mismatch[slot_index(slot)] = Some(error);
            }
        }
    }

    pub fn is_private(&self) -> bool {
        self.credentials.has_primary()
    }

    fn path(&self, endpoint: &str) -> String {
        format!(
            "{}/{}",
            self.version.prefix(),
            endpoint.trim_start_matches('/')
        )
    }

    fn get(&mut self, cx: &mut Cx<'_, Pending>, path: &str, pending: Pending) {
        self.public_read(cx, path, pending, None);
    }

    fn get_streamed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        at: ArrayAt,
        pending: Pending,
    ) {
        self.public_read(cx, path, pending, Some(at));
    }

    fn bootstrap(&mut self, cx: &mut Cx<'_, Pending>) {
        self.get_streamed(
            cx,
            &self.path("exchangeInfo"),
            ArrayAt::Field("symbols"),
            Pending::ExchangeInfo {
                rows: Vec::new(),
                skipped: Vec::new(),
            },
        );
    }

    fn seed_daily(&mut self, cx: &mut Cx<'_, Pending>) {
        self.get_streamed(
            cx,
            &self.path("ticker/24hr"),
            ArrayAt::Root,
            Pending::Daily {
                rows: Vec::new(),
                skipped: Vec::new(),
            },
        );
    }
    fn public_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        self.ensure_public_feed(cx);
    }

    /// Registers the public feed once and connects it when it has no epoch.
    /// Suppressed public feeds never register, so they never churn retries.
    fn ensure_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return;
        }
        let feed = match self.public.feed {
            Some(feed) if cx.socket_epoch(feed).is_some() => return,
            Some(feed) => feed,
            None => match cx.register_socket_feed(feed_policy(SocketChannel::Public, 500.0)) {
                Ok(feed) => {
                    self.public.feed = Some(feed);
                    self.sync_public_interests(cx);
                    feed
                }
                Err(error) => return socket_failed(cx, error),
            },
        };
        match cx.connect_socket(feed, self.urls.public_ws) {
            Ok(epoch) => self.public.socket = Some(epoch.socket()),
            Err(error) => socket_failed(cx, error),
        }
    }

    /// The desired public topics; the engine replays them on every open and
    /// encodes only the deltas while connected.
    fn public_interests(&self) -> Vec<SocketInterest> {
        let mut topics = vec!["!ticker@arr".to_owned(), "!markPrice@arr@1s".to_owned()];
        topics.extend(
            self.books
                .keys()
                .map(|symbol| format!("{}@depth20@100ms", symbol.to_ascii_lowercase())),
        );
        topics.extend(
            self.candles.keys().map(|(symbol, interval)| {
                format!("{}@kline_{interval}", symbol.to_ascii_lowercase())
            }),
        );
        topics
            .into_iter()
            .map(SocketInterest::from_wire_args)
            .collect()
    }

    fn sync_public_interests(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(feed) = self.public.feed else {
            return;
        };
        if let Err(error) = cx.set_socket_interests(feed, self.public_interests()) {
            socket_failed(cx, error);
        }
    }

    fn public_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Opened(epoch) => {
                self.public.socket = Some(epoch.socket());
                for state in self.books.values_mut() {
                    state.last_id = None;
                }
                // Sent before the engine replays the subscriptions, as legacy did.
                cx.send(
                    epoch.socket(),
                    "{\"id\":-1,\"method\":\"LIST_SUBSCRIPTIONS\"}",
                );
            }
            LifecycleEvent::Ping(epoch) => {
                cx.send(epoch.socket(), r#"{"id":-1,"method":"LIST_SUBSCRIPTIONS"}"#)
            }
            LifecycleEvent::Reconnect(_) => self.ensure_public_feed(cx),
            LifecycleEvent::Retired { .. } => {
                self.public.socket = None;
                for (symbol, state) in &mut self.books {
                    state.book.clear();
                    state.last_id = None;
                    cx.book_changed(symbol);
                }
            }
            // A refused set is reported; the feed keeps serving its other topics.
            LifecycleEvent::InterestsRefused { error, .. } => socket_failed(cx, error),
            _ => {}
        }
        Ok(())
    }

    fn request_interest(&mut self, cx: &mut Cx<'_, Pending>, initial: bool, symbol: Option<&str>) {
        let mut path = self.path("openInterest");
        if let Some(symbol) = symbol {
            path.push_str(&format!("?symbol={symbol}"));
        }
        let pending = Pending::OpenInterest {
            rows: HashMap::new(),
            initial,
            single: symbol.is_some(),
        };
        if symbol.is_some() {
            self.get(cx, &path, pending);
        } else {
            self.get_streamed(cx, &path, ArrayAt::Envelope(&[&[], &["data"]]), pending);
        }
    }

    fn finish_interest(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut rows: HashMap<String, f64>,
        initial: bool,
        single: bool,
        response: Option<Response<'_>>,
    ) {
        if !response.is_some_and(|response| response.status == 200) {
            rows.clear();
        }
        if let Some(response) = response
            && response.status == 200
        {
            collect_interest(response.body, &mut rows);
        }
        for (symbol, value) in &rows {
            cx.store
                .update_ticker(symbol, |ticker| ticker.open_interest = *value);
        }
        if initial {
            self.public_ready(cx);
            self.interest_timer = Some(cx.set_timer(250.0));
            return;
        } else if rows.is_empty() && !single {
            let tickers = cx.store.tickers();
            let length = tickers.len();
            let count = length.min(25);
            for offset in 0..count {
                self.interest_fallback.push_back(
                    tickers[(self.interest_cursor + offset) % length]
                        .symbol
                        .clone(),
                );
            }
            if length > 0 {
                self.interest_cursor = (self.interest_cursor + count) % length;
            }
        }
        if let Some(symbol) = self.interest_fallback.pop_front() {
            self.request_interest(cx, false, Some(&symbol));
        } else {
            self.interest_timer = Some(cx.set_timer(INTEREST_MS));
        }
    }

    fn on_public_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        if text.trim_start().starts_with('[') {
            let _ = json::each::<&RawValue>(text, |item| self.on_public_frame(cx, item.get()));
            return;
        }
        let Ok(envelope) = serde_json::from_str::<Envelope>(text) else {
            return;
        };
        if envelope.id.is_some() {
            return;
        }
        let stream = envelope.stream.as_deref();
        let body = envelope.data.map_or(text, RawValue::get);
        match stream {
            Some("!ticker@arr") => {
                let _ = json::each::<TickerEvent>(body, |event| update_ticker(cx, event));
            }
            Some("!markPrice@arr@1s" | "!markPrice@arr") => {
                let _ = json::each::<MarkEvent>(body, |event| update_mark(cx, event));
            }
            Some(stream) if is_partial_stream(stream) => self.on_depth(cx, stream, body),
            Some(stream) if stream.contains("@depth") => {}
            Some(stream) if stream.contains("@kline_") => self.on_kline(cx, body),
            Some(stream) if stream.ends_with("@aggTrade") => self.on_trade(cx, body),
            _ => match envelope.e.as_deref() {
                Some("24hrTicker") => {
                    if let Ok(event) = serde_json::from_str::<TickerEvent>(body) {
                        update_ticker(cx, event);
                    }
                }
                Some("markPriceUpdate") => {
                    if let Ok(event) = serde_json::from_str::<MarkEvent>(body) {
                        update_mark(cx, event);
                    }
                }
                Some("kline") => self.on_kline(cx, body),
                Some("aggTrade") => self.on_trade(cx, body),
                _ => {}
            },
        }
    }

    fn on_kline(&self, cx: &mut Cx<'_, Pending>, body: &str) {
        if let Ok(event) = serde_json::from_str::<KlineEvent>(body) {
            let k = event.k;
            if let Some(interval) = Interval::parse(&k.i)
                && self.candles.contains_key(&(k.s.to_string(), interval))
            {
                cx.candle(&k.s, interval, candle(k.t, &k.o, &k.h, &k.l, &k.c, &k.v));
            }
        }
    }

    fn on_trade(&self, cx: &mut Cx<'_, Pending>, body: &str) {
        if let Ok(event) = serde_json::from_str::<TradeEvent>(body) {
            let price = num::parse(&event.p);
            cx.store
                .update_ticker(&event.s, |ticker| ticker.last = price);
        }
    }

    fn on_depth(&mut self, cx: &mut Cx<'_, Pending>, stream: &str, body: &str) {
        let Ok(event) = serde_json::from_str::<PartialDepth>(body) else {
            return;
        };
        let symbol = stream
            .split('@')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if event.s.as_deref().is_some_and(|s| s != symbol) {
            return;
        }
        let Some(state) = self.books.get_mut(&symbol) else {
            return;
        };
        if event
            .u
            .zip(state.last_id)
            .is_some_and(|(next, last)| next <= last)
        {
            return;
        }
        let levels = |rows: Vec<[std::borrow::Cow<'_, str>; 2]>| -> Option<Vec<(f64, f64)>> {
            rows.into_iter()
                .map(|[price, size]| {
                    let price: f64 = price.trim().parse().ok()?;
                    let size: f64 = size.trim().parse().ok()?;
                    (price.is_finite() && size.is_finite() && price > 0.0 && size > 0.0)
                        .then_some((price, size))
                })
                .collect()
        };
        let (Some(mut bids), Some(mut asks)) = (levels(event.b), levels(event.a)) else {
            return;
        };
        if bids.is_empty() || asks.is_empty() {
            return;
        }
        bids.sort_by(|a, b| b.0.total_cmp(&a.0));
        asks.sort_by(|a, b| a.0.total_cmp(&b.0));
        if bids[0].0 >= asks[0].0 {
            return;
        }
        cx.store.update_ticker(&symbol, |ticker| {
            ticker.bid = bids[0].0;
            ticker.ask = asks[0].0;
        });
        state.book.load_sorted_owned(BookSide::Bids, bids);
        state.book.load_sorted_owned(BookSide::Asks, asks);
        state.last_id = event.u.or(state.last_id);
        cx.book_changed(&symbol);
    }
}

fn is_partial_stream(stream: &str) -> bool {
    let Some((_, depth)) = stream.split_once('@') else {
        return false;
    };
    matches!(
        depth,
        "depth5"
            | "depth10"
            | "depth20"
            | "depth5@100ms"
            | "depth5@250ms"
            | "depth5@500ms"
            | "depth10@100ms"
            | "depth10@250ms"
            | "depth10@500ms"
            | "depth20@100ms"
            | "depth20@250ms"
            | "depth20@500ms"
    )
}

fn update_ticker(cx: &mut Cx<'_, Pending>, event: TickerEvent<'_>) {
    cx.store.update_ticker(&event.s, |ticker| {
        if let Some(value) = event.c.as_deref().filter(|s| !s.is_empty()) {
            ticker.last = num::parse(value);
        }
        if let Some(value) = event.percentage.as_deref().filter(|s| !s.is_empty()) {
            ticker.percentage = num::parse(value);
        }
        if let Some(value) = event.v.as_deref().filter(|s| !s.is_empty()) {
            ticker.volume = num::parse(value);
        }
        if let Some(value) = event.q.as_deref().filter(|s| !s.is_empty()) {
            ticker.quote_volume = num::parse(value);
        }
    });
}

fn update_mark(cx: &mut Cx<'_, Pending>, event: MarkEvent<'_>) {
    cx.store.update_ticker(&event.s, |ticker| {
        if let Some(value) = event.p.as_deref().filter(|s| !s.is_empty()) {
            ticker.mark = num::parse(value);
        }
        if let Some(value) = event.i.as_deref().filter(|s| !s.is_empty()) {
            ticker.index = num::parse(value);
        }
        if let Some(value) = event.r.as_deref().filter(|s| !s.is_empty()) {
            ticker.funding_rate = num::parse(value);
        }
        if let Some(value) = event.funding_time.filter(|value| *value != 0.0) {
            ticker.funding_time = value;
        }
    });
}

fn collect_interest(body: &str, output: &mut HashMap<String, f64>) {
    if let Ok(row) = serde_json::from_str::<Interest>(body) {
        let value = serde_json::from_str::<f64>(row.open_interest.get())
            .ok()
            .or_else(|| {
                serde_json::from_str::<String>(row.open_interest.get())
                    .ok()?
                    .parse::<f64>()
                    .ok()
            });
        if let Some(value) = value.filter(|value| value.is_finite()) {
            output.insert(row.symbol.into_owned(), value);
        }
    } else if body.trim_start().starts_with('[') {
        let _ = json::each::<&RawValue>(body, |row| collect_interest(row.get(), output));
    } else if let Ok(envelope) = serde_json::from_str::<InterestEnvelope>(body) {
        collect_interest(envelope.data.get(), output);
    }
}

// AsterDEX kline codes are the TS spellings.
fn supported(interval: Interval) -> bool {
    use Interval::*;
    matches!(
        interval,
        M1 | M3 | M5 | M15 | M30 | H1 | H2 | H4 | H6 | H12 | D1 | W1
    )
}

fn candle(time: f64, o: &str, h: &str, l: &str, c: &str, v: &str) -> Candle {
    Candle {
        timestamp: time / 1000.0,
        open: num::parse(o),
        high: num::parse(h),
        low: num::parse(l),
        close: num::parse(c),
        volume: num::parse(v),
    }
}

fn market_from_wire(row: SymbolInfo<'_>) -> (Market, MarketRule) {
    let find = |kind: &str| row.filters.iter().find(|filter| filter.filter_type == kind);
    let lot = find("LOT_SIZE");
    let price = 10f64.powi(-row.price_precision);
    let rule = MarketRule {
        tick_size: find("PRICE_FILTER")
            .and_then(|filter| filter.tick_size.as_deref())
            .map(num::parse)
            .unwrap_or(price),
        max_algo_orders: find("MAX_NUM_ALGO_ORDERS").and_then(|filter| filter.limit),
    };
    let market = Market {
        id: row.symbol.to_string(),
        symbol: row.symbol.into_owned(),
        pretty_symbol: None,
        base: row.base_asset.into_owned(),
        quote: row.quote_asset.into_owned(),
        active: true,
        precision: Precision {
            price,
            amount: 10f64.powi(-row.quantity_precision),
        },
        limits: Limits {
            amount: MinMax {
                min: lot
                    .and_then(|filter| filter.min_qty.as_deref())
                    .map(num::parse)
                    .unwrap_or(0.0),
                max: lot
                    .and_then(|filter| filter.max_qty.as_deref())
                    .map(num::parse)
                    .unwrap_or(0.0),
            },
            leverage: MinMax {
                min: 1.0,
                max: 10.0,
            },
            price: None,
        },
        contract_size: Some(1.0),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    };
    (market, rule)
}

impl Venue for AsterDex {
    fn guard_pending_placements(&self) -> bool {
        true
    }

    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            stp_controls: self.version == Version::V3,
            isolated_book_subscriptions: true,
            cancel_all: safe_cex_engine::CancelAll::EachPlacedSymbol,
            account: safe_cex_engine::AccountLookup::Query,
            leverage_query: safe_cex_engine::LeverageQuery::OnRead,
            cancel_with_symbols: true,
            order_book_query: true,
            ..safe_cex_engine::Capabilities::BASE
        }
    }

    fn address_nonce_domain(&self) -> Option<&'static safe_cex_engine::nonce::NonceDomain> {
        (self.version == Version::V3).then_some(&safe_cex_engine::nonce::ASTERDEX_V3)
    }

    /// Only slots that sign V3 gain address setup; a public-data-only
    /// primary never signs.
    fn public_address_slots(&self) -> safe_cex_engine::AddressSetupSlots {
        use safe_cex_engine::AddressSetupSlots;
        match (self.version, self.options.public_data_only) {
            (Version::V1, _) => AddressSetupSlots::None,
            (Version::V3, true) => AddressSetupSlots::Secondary,
            (Version::V3, false) => AddressSetupSlots::Both,
        }
    }

    fn dispatch_nonce() -> bool {
        true
    }

    fn http_release() -> safe_cex_engine::HttpRelease<Self> {
        safe_cex_engine::HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
    }

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        // Configured fields are checked first; sealing then binds the slots.
        if let Some(error) = self.signing_config_error() {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            return;
        }
        self.seal_signing(cx, true);
        self.start_requests(cx);
    }

    // Both feeds are engine-managed: opens, closes and liveness arrive as socket events.
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}

    /// The engine delivers only frames of an open, current, unexpired epoch.
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.on_account_message(cx, socket, text) {
            return;
        }
        if self.public.socket == Some(socket) {
            self.on_public_frame(cx, text);
        }
    }

    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}

    fn socket_capabilities() -> SocketCapabilities {
        // No AsterDEX work waits on a subscription release; continuations compile out.
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
        if self.account_feed() == Some(feed) {
            self.account_socket_event(cx, event)
        } else if self.public.feed == Some(feed) {
            self.public_socket_event(cx, event)
        } else {
            Err(SocketError::Stale)
        }
    }

    /// One `{"method","params","id"}` frame per batch, split only to stay under
    /// the engine's frame bound; the private feed carries no interests.
    fn encode_socket_interests(
        &self,
        _store: &safe_cex_engine::Store,
        feed: FeedId,
        subscribe: bool,
        interests: &[SocketInterest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        if self.public.feed != Some(feed) || interests.is_empty() {
            return Ok(());
        }
        let method = if subscribe {
            "SUBSCRIBE"
        } else {
            "UNSUBSCRIBE"
        };
        let mut start = 0;
        while start < interests.len() {
            let mut end = start + 1;
            let frame = |chunk: &[SocketInterest], id: u64| {
                let params: Vec<&str> = chunk.iter().map(|i| i.wire_args.as_ref()).collect();
                format!(
                    "{{\"method\":\"{method}\",\"params\":{},\"id\":{id}}}",
                    serde_json::to_string(&params).unwrap_or_default()
                )
            };
            let id = self
                .public
                .next_request_id
                .get()
                .checked_add(1)
                .ok_or(SocketError::Capacity)?;
            while end < interests.len()
                && frame(&interests[start..=end], id).len() <= MAX_FRAME_BYTES
            {
                end += 1;
            }
            self.public.next_request_id.set(id);
            emit(EncodedBatch {
                text: frame(&interests[start..end], id),
                covered_keys: interests[start..end]
                    .iter()
                    .map(|i| i.key.clone())
                    .collect(),
            })?;
            start = end;
        }
        Ok(())
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        match pending {
            Pending::DepthRead { context } => {
                #[derive(serde::Serialize)]
                struct Level {
                    price: f64,
                    amount: f64,
                    total: f64,
                }
                #[derive(serde::Serialize)]
                struct Output {
                    bids: Vec<Level>,
                    asks: Vec<Level>,
                }
                let result = if let Some((_, error)) = account::venue_error(&response) {
                    Err(error)
                } else {
                    serde_json::from_str::<safe_cex_venue_binance::wire::DepthSnapshot>(
                        response.body,
                    )
                    .map_err(|e| e.to_string())
                    .and_then(|depth| {
                        let levels = |raw: Vec<[std::borrow::Cow<'_, str>; 2]>,
                                      bid: bool|
                         -> Result<Vec<Level>, String> {
                            let mut levels = raw
                                .into_iter()
                                .map(|l| {
                                    Ok(Level {
                                        price: l[0].parse::<f64>().map_err(|e| e.to_string())?,
                                        amount: l[1].parse::<f64>().map_err(|e| e.to_string())?,
                                        total: 0.0,
                                    })
                                })
                                .collect::<Result<Vec<_>, String>>()?;
                            if levels.iter().any(|l| {
                                !l.price.is_finite()
                                    || !l.amount.is_finite()
                                    || l.price <= 0.0
                                    || l.amount < 0.0
                            }) {
                                return Err("Malformed Aster depth level".into());
                            }
                            levels.sort_by(|a, b| {
                                if bid {
                                    b.price.total_cmp(&a.price)
                                } else {
                                    a.price.total_cmp(&b.price)
                                }
                            });
                            let mut total = 0.0;
                            for l in &mut levels {
                                total += l.price * l.amount;
                                l.total = total;
                            }
                            Ok(levels)
                        };
                        serde_json::to_string(&Output {
                            bids: levels(depth.bids, true)?,
                            asks: levels(depth.asks, false)?,
                        })
                        .map_err(|e| e.to_string())
                    })
                };
                let _ = match result {
                    Ok(data) => cx.done_context(&context, data),
                    Err(error) => cx.fail_context(&context, error),
                };
            }
            Pending::MarketBrackets { rows } => {
                if account::venue_error(&response).is_none() {
                    cx.store.update_markets(|m| {
                        if let Some(max) = rows.get(&m.symbol) {
                            m.limits.leverage.max = *max;
                        }
                    });
                }
                self.seed_daily(cx);
            }
            Pending::Signed(req) => self.on_signed_response(cx, req, Ok(response)),
            Pending::Clock(req) => self.on_clock_response(cx, req, Ok(response)),
            Pending::Account(req) => self.on_account_response(cx, req, Ok(response)),
            Pending::Trade(req) => self.on_trade_response(cx, req, Ok(response), None, false),
            Pending::History(req) => self.on_history_response(cx, req, Ok(response)),
            Pending::ExchangeInfo { rows, skipped } => {
                log_skipped_rows(cx, "market", &skipped);
                if response.status != 200 || (!skipped.is_empty() && rows.is_empty()) {
                    cx.retried_failure("asterdex.lib:823", "Aster market bootstrap failed");
                    self.startup.failed(cx, true);
                    return;
                }
                let mut markets = Vec::with_capacity(rows.len());
                for (market, rule) in rows {
                    self.market_rules.insert(market.symbol.clone(), rule);
                    markets.push(market);
                }
                self.startup.succeeded(cx, &true);
                cx.store.set_markets(markets);
                if self.is_private() {
                    self.start_account(cx);
                }
                let path = self.path("leverageBracket");
                let pending = Pending::MarketBrackets {
                    rows: HashMap::new(),
                };
                if self.is_private() {
                    self.signed_streamed(cx, &path, sign::Query::new(), pending);
                } else {
                    self.get_streamed(cx, &path, ArrayAt::Root, pending);
                }
            }
            Pending::Daily { rows, skipped } => {
                log_skipped_rows(cx, "ticker", &skipped);
                if response.status != 200 || (!skipped.is_empty() && rows.is_empty()) {
                    cx.retried_failure("asterdex.lib:854", "Aster ticker bootstrap failed");
                    self.startup.failed(cx, false);
                    return;
                }
                self.startup.succeeded(cx, &false);
                cx.store.set_tickers(rows);
                self.request_interest(cx, true, None);
            }
            Pending::OpenInterest {
                rows,
                initial,
                single,
            } => self.finish_interest(cx, rows, initial, single, Some(response)),
            Pending::Klines {
                context,
                rows,
                invalid,
            } => {
                let result = match invalid {
                    None if response.status == 200 => {
                        serde_json::to_string(&rows).map_err(|error| error.to_string())
                    }
                    None => Err(format!(
                        "Aster klines HTTP {} or malformed payload",
                        response.status
                    )),
                    Some(detail) => Err(format!(
                        "Aster klines HTTP {} or malformed payload: {detail}",
                        response.status
                    )),
                };
                let _ = match result {
                    Ok(data) => cx.done_context(&context, data),
                    Err(error) => cx.fail_context(&context, error),
                };
            }
        }
    }

    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::DepthRead { .. } => {}
            Pending::MarketBrackets { rows } => {
                if let Ok(row) =
                    serde_json::from_str::<safe_cex_venue_binance::wire::LeverageBracket>(item)
                    && let Some(bracket) = row.brackets.first()
                {
                    rows.insert(row.symbol.to_string(), bracket.initial_leverage);
                }
            }
            Pending::Signed(req) => self.on_signed_item(cx, req, item),
            Pending::Account(req) => self.on_account_item(cx, req, item),
            Pending::History(req) => self.on_history_item(cx, req, item),
            Pending::Clock(_) | Pending::Trade(_) => {}
            Pending::ExchangeInfo { rows, skipped } => {
                match serde_json::from_str::<SymbolInfo>(item) {
                    Ok(row) if row.status == "TRADING" => {
                        let (market, rule) = market_from_wire(row);
                        if market.id.is_empty()
                            || market.base.is_empty()
                            || market.quote.is_empty()
                            || !market.precision.price.is_finite()
                            || market.precision.price <= 0.0
                            || !market.precision.amount.is_finite()
                            || market.precision.amount <= 0.0
                            || !rule.tick_size.is_finite()
                            || rule.tick_size <= 0.0
                        {
                            skipped.push(row_symbol(item));
                            safe_cex_engine::startup::skipped_row(cx);
                        } else {
                            rows.push((market, rule));
                        }
                    }
                    Ok(_) => {}
                    Err(_) => {
                        skipped.push(row_symbol(item));
                        safe_cex_engine::startup::skipped_row(cx);
                    }
                }
            }
            Pending::Daily { rows, skipped } => match serde_json::from_str::<Daily>(item) {
                Ok(row)
                    if !row.symbol.trim().is_empty()
                        && [
                            &row.last_price,
                            &row.price_change_percent,
                            &row.volume,
                            &row.quote_volume,
                        ]
                        .iter()
                        .all(|text| safe_cex_engine::startup::number_or_zero(text).is_some()) =>
                {
                    rows.push(Ticker {
                        id: row.symbol.to_string(),
                        symbol: row.symbol.into_owned(),
                        last: num::parse(&row.last_price),
                        percentage: num::parse(&row.price_change_percent),
                        volume: num::parse(&row.volume),
                        quote_volume: num::parse(&row.quote_volume),
                        ..Ticker::default()
                    })
                }
                _ => {
                    skipped.push(row_symbol(item));
                    safe_cex_engine::startup::skipped_row(cx);
                }
            },
            Pending::OpenInterest { rows, .. } => collect_interest(item, rows),
            Pending::Klines { rows, invalid, .. } => match serde_json::from_str::<KlineRow>(item) {
                Ok(row) => rows.push(candle(row.0, &row.1, &row.2, &row.3, &row.4, &row.5)),
                Err(error) => {
                    let row = rows.len();
                    invalid.get_or_insert_with(|| format!("row {row}: {error}"));
                }
            },
        }
    }

    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        match pending {
            Pending::Signed(signed) => match &signed.pending {
                Pending::Trade(trade) => trade.reconciliation(),
                _ => Err(safe_cex_engine::request_context::Error::Invalid),
            },
            _ => Err(safe_cex_engine::request_context::Error::Invalid),
        }
    }

    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        plan: &safe_cex_engine::request_context::Plan,
    ) {
        self.on_resync_ready(cx, plan);
    }

    fn on_condition_wait_expired(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        owner: &safe_cex_engine::request_context::ConditionOwner,
        _error: VenueError,
    ) {
        if let safe_cex_engine::request_context::ConditionOwner::Plan(plan) = owner {
            self.on_resync_expired(plan);
        }
    }

    fn on_http_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &safe_cex_engine::request_context::Preflight,
    ) {
        self.on_trade_preflight_ready(cx, preflight);
    }

    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::Signed(req) => self.on_signed_failed(cx, req, error),
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }

    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        if let Pending::Signed(req) = pending {
            self.on_signed_stream_error(cx, req, response);
            return;
        }
        if let Pending::OpenInterest {
            rows,
            initial,
            single,
        } = pending
        {
            self.finish_interest(cx, rows, initial, single, Some(response));
        } else {
            self.on_request_failed(cx, pending, "missing expected response list");
        }
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::DepthRead { context } => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::MarketBrackets { .. } => self.seed_daily(cx),
            Pending::Signed(req) => self.on_signed_response(cx, req, Err(error)),
            Pending::Clock(req) => self.on_clock_response(cx, req, Err(error)),
            Pending::Account(req) => self.on_account_response(cx, req, Err(error)),
            Pending::Trade(req) => self.on_trade_response(cx, req, Err(error), None, false),
            Pending::History(req) => self.on_history_response(cx, req, Err(error)),
            Pending::OpenInterest {
                initial, single, ..
            } => self.finish_interest(cx, HashMap::new(), initial, single, None),
            Pending::Klines { context, .. } => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::ExchangeInfo { .. } => self.startup.failed(cx, true),
            Pending::Daily { .. } => self.startup.failed(cx, false),
        }
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(markets) = self.startup.on_timer(timer) {
            if markets {
                self.bootstrap(cx);
            } else {
                self.seed_daily(cx);
            }
            return;
        }
        if self.on_request_timer(cx, timer)
            || self.on_account_timer(cx, timer)
            || self.on_history_timer(cx, timer)
        {
            return;
        }
        if self.interest_timer == Some(timer) {
            self.interest_timer = None;
            self.request_interest(cx, false, None);
        }
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Some(local) = self.local_read(&command) {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let result = match local {
                history::LocalRead::History(result) => {
                    result.map(str::to_owned).map_err(str::to_owned)
                }
                history::LocalRead::Unsupported(interval) => Err(interval.unsupported("AsterDEX")),
                history::LocalRead::Leverage(symbol) => cx
                    .store
                    .setting(symbol)
                    .filter(|s| s.leverage.is_finite() && s.leverage > 0.0)
                    .map(|s| {
                        format!(
                            "{{\"buyLeverage\":{},\"sellLeverage\":{}}}",
                            num::format(s.leverage),
                            num::format(s.leverage)
                        )
                    })
                    .ok_or_else(|| format!("Aster leverage is not loaded for {symbol}")),
            };
            let _ = match result {
                Ok(data) => cx.done_context(&context, data),
                Err(error) => cx.fail_context(&context, error),
            };
            return;
        }
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => {
                let state = self.books.entry(symbol.clone()).or_default();
                state.subscribers += 1;
                if state.subscribers == 1 {
                    self.sync_public_interests(cx);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers -= 1;
                    if state.subscribers == 0 {
                        self.books.remove(&symbol);
                        self.sync_public_interests(cx);
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                if !supported(interval) {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("AsterDEX"),
                    );
                    return;
                }
                let count = self.candles.entry((symbol.clone(), interval)).or_default();
                *count += 1;
                if *count == 1 {
                    self.sync_public_interests(cx);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol, interval);
                if let Some(count) = self.candles.get_mut(&key) {
                    *count -= 1;
                    if *count == 0 {
                        self.candles.remove(&key);
                        self.sync_public_interests(cx);
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
                let mut path = format!(
                    "{}?symbol={symbol}&interval={interval}&limit={}",
                    self.path("klines"),
                    limit.unwrap_or(500)
                );
                if let Some(from) = from_ms {
                    path.push_str(&format!("&startTime={}", num::format(from)));
                }
                if let Some(to) = to_ms {
                    path.push_str(&format!("&endTime={}", num::format(to)));
                }
                let Ok(context) = cx.admit_http_command(request::public_read_policy()) else {
                    return;
                };
                self.get_streamed(
                    cx,
                    &path,
                    ArrayAt::Root,
                    Pending::Klines {
                        context,
                        rows: Vec::new(),
                        invalid: None,
                    },
                );
            }
            Command::FetchOrderBook {
                command: _,
                symbol,
                limit,
            } => {
                let path = format!(
                    "{}?{}",
                    self.path("depth"),
                    sign::Query::new()
                        .str("symbol", &symbol)
                        .num("limit", f64::from(limit.unwrap_or(100)))
                        .as_str()
                );
                let Ok(context) = cx.admit_http_command(request::public_read_policy()) else {
                    return;
                };
                self.get(cx, &path, Pending::DepthRead { context });
            }
            Command::FetchAccount { .. } => self.fetch_account_info(cx),
            Command::Query { query, .. } => self.start_query(cx, query),
            command => self.on_trade_command(cx, command),
        }
    }

    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|state| !state.book.is_empty())
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

fn row_symbol(item: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Identity<'a> {
        #[serde(borrow)]
        symbol: std::borrow::Cow<'a, str>,
    }
    serde_json::from_str::<Identity>(item)
        .map_or_else(|_| "<unidentified>".into(), |row| row.symbol.into_owned())
}

/// A malformed startup row is skipped, not fatal; one summary per read.
fn log_skipped_rows(cx: &mut Cx<'_, Pending>, kind: &str, skipped: &[String]) {
    if !skipped.is_empty() {
        cx.notice(
            Level::Warn,
            code::ROW_SKIPPED,
            format!(
                "Aster skipped {} malformed {kind} row(s): {}",
                skipped.len(),
                skipped.join(", ")
            ),
        );
    }
}

impl AsterDex {
    /// A managed read's typed failure: a command read keeps its error kind.
    pub(crate) fn on_read_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: VenueError,
    ) {
        match pending {
            Pending::DepthRead { context: root }
            | Pending::Klines { context: root, .. }
            | Pending::Account(account::Request::Info { root }) => {
                let _ = cx.fail_context(&root, error);
            }
            Pending::History(history) => {
                let _ = cx.fail_context(history.root(), error);
            }
            Pending::Trade(trading::Request {
                root: Some(root), ..
            }) => {
                let _ = cx.fail_context(&root, error);
            }
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }
}

fn reset_pending(pending: &mut Pending) {
    match pending {
        Pending::Account(account::Request::Positions { rows, settings, .. }) => {
            *rows = Default::default();
            settings.clear();
        }
        Pending::Account(account::Request::Orders { rows, .. }) => *rows = Default::default(),
        Pending::History(req) => req.reset(),
        Pending::MarketBrackets { rows } => rows.clear(),
        Pending::ExchangeInfo { rows, skipped } => {
            rows.clear();
            skipped.clear();
        }
        Pending::Daily { rows, skipped } => {
            rows.clear();
            skipped.clear();
        }
        Pending::Klines { rows, invalid, .. } => {
            rows.clear();
            *invalid = None;
        }
        Pending::OpenInterest { rows, .. } => rows.clear(),
        _ => {}
    }
}
