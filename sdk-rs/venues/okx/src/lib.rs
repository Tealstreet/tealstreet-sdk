//! OKX swaps and EEA X-Perps on the sans-I/O engine.
mod account;
mod closed;
/// Opaque canonical recipes signed by the injected dispatch provider.
#[path = "sign/dispatch.rs"]
pub mod dispatch_signing;
mod history;
mod sign;
mod socket_codec;
mod trading;
mod wire;

use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::request_context::{
    Context, Credential, Operation, Policy, RetryPolicy, Scope,
};
use safe_cex_engine::selected_read::{BackgroundReadError, ReadRequest, ReadRole, SelectedRead};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::socket::Config;
use safe_cex_engine::startup::{DiscoveryRead, PublicRetries};
use safe_cex_engine::{
    Command, CommandId, CredentialSlot, Cx, Method, PublicFeedPolicy, Response, SocketId, TimerId,
    Venue, num,
};
use std::collections::{HashMap, HashSet};

const PUBLIC: usize = 0;
const BUSINESS: usize = 2;

#[derive(Clone, Default, Debug)]
pub struct Options {
    pub testnet: bool,
    pub eea: bool,
    pub public_data_only: bool,
    pub credentials: Option<Identity>,
    pub read_credentials: Option<Identity>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InstrumentType {
    Swap,
    Futures,
}
impl InstrumentType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Swap => "SWAP",
            Self::Futures => "FUTURES",
        }
    }
}
struct Product {
    kind: InstrumentType,
    underlying: String,
    settle_ccy: String,
}

struct Connection {
    feed: Option<safe_cex_engine::socket_lifecycle::FeedId>,
}
impl Connection {
    fn new(_login: bool) -> Self {
        Self { feed: None }
    }
}

#[derive(Default)]
struct BookState {
    subscribers: u32,
    sequence: Option<i64>,
    recovering: bool,
    book: Book,
}

pub enum Pending {
    SocketRefresh {
        reopened: bool,
    },
    Account(account::Request),
    History(Box<history::Request>),
    Closed(Box<closed::Request>),
    Trade(trading::Request),
    Markets {
        /// This physical discovery read's Background Context, once admitted.
        context: Option<Context>,
        kind: InstrumentType,
        rows: Vec<Market>,
        scope_start: usize,
        incomplete: bool,
        invalid: bool,
    },
    Tickers {
        context: Option<Context>,
        kind: InstrumentType,
        rows: Vec<Ticker>,
        scope_start: usize,
        incomplete: bool,
        invalid: bool,
        seeded: u32,
    },
    /// One physical candle read (`leg`) under the command's admitted `root`.
    Candles {
        root: Context,
        leg: Context,
        rows: Vec<Candle>,
        invalid: bool,
        read: CandleRead,
    },
}

pub struct CandleRead {
    instrument: String,
    interval: Interval,
    limit: u32,
    before: f64,
    after: f64,
    historical: bool,
    force_history: bool,
}

pub struct Okx {
    options: Options,
    credentials: Option<Identity>,
    account: account::AccountState,
    trading: trading::TradingState,
    history: history::HistoryState,
    closed: closed::ClosedState,
    connections: [Connection; 3],
    books: HashMap<String, BookState>,
    marks: HashSet<String>,
    indexes: HashSet<String>,
    candles: HashMap<(String, Interval), u32>,
    products: HashMap<String, Product>,
    underlying_products: HashMap<String, Vec<String>>,
    price_tickers: HashMap<String, Option<u32>>,
    startup: PublicRetries,
    /// Discovery retries wait out a rate-limit window the venue announced.
    discovery_paused_until: f64,
    catalogue_incomplete: bool,
    ticker_reading: bool,
    ticker_refresh_pending: bool,
    live_tickers: safe_cex_engine::startup::LiveTickers,
    skipped_instruments: HashSet<String>,
    /// Instruments skipped as malformed by the catalogue read in flight.
    skipping_instruments: HashSet<String>,
    catalogue_partial: bool,
}

fn discovery_read_policy() -> safe_cex_engine::selected_read::ReadPolicy {
    safe_cex_engine::selected_read::ReadPolicy {
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

fn discovery_context(pending: &Pending) -> Option<Context> {
    match pending {
        Pending::Markets { context, .. } | Pending::Tickers { context, .. } => context.clone(),
        _ => None,
    }
}

/// A discovery read decodes and publishes only its own exact Background
/// delivery; without an admitted Context it has no authority (fail closed).
fn discovery_delivered(cx: &Cx<'_, Pending>, pending: &Pending) -> bool {
    match pending {
        Pending::Markets { context, .. } | Pending::Tickers { context, .. } => context
            .as_ref()
            .is_some_and(|context| cx.validate_background_read_http_delivery(context).is_ok()),
        _ => true,
    }
}

fn with_discovery_context(mut pending: Pending, admitted: Context) -> Pending {
    if let Pending::Markets { context, .. } | Pending::Tickers { context, .. } = &mut pending {
        *context = Some(admitted);
    }
    pending
}

fn discovery_error(response: &Response<'_>) -> safe_cex_engine::VenueError {
    let envelope = serde_json::from_str::<wire::Envelope<'_>>(response.body).ok();
    let rejection = envelope.as_ref().filter(|e| e.code != "0");
    safe_cex_engine::startup::public_response_error(
        response.status,
        rejection.map(|e| e.code),
        rejection.map(|e| e.msg),
    )
}

impl Okx {
    fn public_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        markets: bool,
        error: safe_cex_engine::VenueError,
    ) {
        if !markets {
            self.ticker_reading = false;
            self.ticker_refresh_pending = false;
        }
        let pause = self.discovery_paused_until - cx.now;
        self.startup.failed_after(
            cx,
            if markets {
                DiscoveryRead::Markets
            } else {
                DiscoveryRead::Tickers
            },
            pause,
            error,
        );
    }

    pub fn new(options: Options) -> Self {
        let credentials = options
            .credentials
            .clone()
            .filter(|identity| !options.public_data_only && identity.validate().is_ok());
        let login = credentials.is_some();
        Self {
            options,
            credentials,
            account: account::AccountState::default(),
            trading: trading::TradingState::default(),
            history: history::HistoryState::default(),
            closed: closed::ClosedState::default(),
            connections: std::array::from_fn(|channel| Connection::new(channel != PUBLIC && login)),
            books: HashMap::new(),
            marks: HashSet::new(),
            indexes: HashSet::new(),
            candles: HashMap::new(),
            products: HashMap::new(),
            underlying_products: HashMap::new(),
            price_tickers: HashMap::new(),
            startup: Default::default(),
            discovery_paused_until: 0.0,
            catalogue_incomplete: false,
            ticker_reading: false,
            ticker_refresh_pending: false,
            live_tickers: Default::default(),
            skipped_instruments: HashSet::new(),
            skipping_instruments: HashSet::new(),
            catalogue_partial: false,
        }
    }

    pub(crate) fn instrument_types(&self) -> &'static [InstrumentType] {
        if self.options.eea {
            &[InstrumentType::Swap, InstrumentType::Futures]
        } else {
            &[InstrumentType::Swap]
        }
    }

    pub(crate) fn instrument_type(&self, id: &str) -> &'static str {
        self.products.get(id).map_or("SWAP", |p| p.kind.as_str())
    }

    fn ticker_seed(&mut self, cx: &mut Cx<'_, Pending>, catalogue_changed: bool) {
        if self.ticker_reading {
            self.ticker_refresh_pending |= catalogue_changed;
            return;
        }
        self.ticker_reading = true;
        self.live_tickers.begin();
        self.load_tickers(cx, InstrumentType::Swap, Vec::new(), false, 0);
    }

    fn next_instrument_type(&self, kind: InstrumentType) -> Option<InstrumentType> {
        (self.options.eea && kind == InstrumentType::Swap).then_some(InstrumentType::Futures)
    }

    fn reference_price(&self, cx: &Cx<'_, Pending>, market: &Market, reference: f64) -> f64 {
        if reference.is_finite() && reference > 0.0 {
            return reference;
        }
        cx.store.ticker(&market.symbol).map_or(0.0, |t| {
            if t.mark.is_finite() && t.mark > 0.0 {
                t.mark
            } else if t.last.is_finite() && t.last > 0.0 {
                t.last
            } else {
                0.0
            }
        })
    }

    pub(crate) fn contracts_to_base(
        &self,
        cx: &Cx<'_, Pending>,
        market: &Market,
        native: f64,
        reference: f64,
    ) -> f64 {
        let value = scaled(native, market.contract_size.unwrap_or(1.0));
        if market.is_inverse == Some(true) {
            let price = self.reference_price(cx, market, reference);
            if price > 0.0 { value / price } else { 0.0 }
        } else {
            value
        }
    }

    fn rest(&self) -> &'static str {
        match (self.options.eea, self.options.testnet) {
            (true, _) => "https://eea.okx.com",
            (false, true) => "https://openapi.okx.com",
            (false, false) => "https://www.okx.com",
        }
    }

    fn socket_url(&self, channel: usize) -> String {
        let host = match (self.options.eea, self.options.testnet) {
            (true, true) => "wseeapap.okx.com",
            (true, false) => "wseea.okx.com",
            (false, true) => "wspap.okx.com",
            (false, false) => "ws.okx.com",
        };
        let path = ["public", "private", "business"][channel];
        format!("wss://{host}/ws/v5/{path}")
    }

    /// One physical discovery read under its own Background Read: public reads
    /// unsigned with the native static headers, the EEA account catalogue on the
    /// selected key. A refused admission or queue is a pre-send discovery failure.
    fn discovery_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        private: bool,
        pending: Pending,
    ) {
        let markets = matches!(pending, Pending::Markets { .. });
        let url = format!("{}{path}", self.rest());
        let stream = ArrayAt::EnvelopeField("data");
        if private {
            let slots = self.read_slots(SlotPolicy::default());
            let selected = SelectedRead::select(&slots, ReadRole::Read { scope: None });
            let read = match SelectedRead::background(cx, selected, discovery_read_policy()) {
                Ok(read) => read,
                Err(BackgroundReadError::Selection(_)) => {
                    let error = safe_cex_engine::VenueError::new(
                        "OKX account catalogue requires credentials",
                    );
                    self.public_failed(cx, markets, error);
                    return;
                }
                Err(BackgroundReadError::Admission(error)) => {
                    self.public_failed(cx, markets, error.read_venue_error());
                    return;
                }
            };
            let pending = with_discovery_context(pending, read.context().clone());
            let demo = self.options.testnet;
            let queued = read.queue(
                cx,
                ReadRequest {
                    method: Method::Get,
                    url,
                    body: None,
                    stream: Some(stream),
                },
                |identity| {
                    Box::new(sign::Http::new(Some(identity.clone()), path.into()).demo(demo))
                },
                |_| pending,
            );
            // The refused page's Context retires before the native failure path.
            if let Err((pending, error)) = queued {
                Self::retire_discovery(cx, &pending);
                self.public_failed(cx, markets, error.read_venue_error());
            }
            return;
        }
        let policy = Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        };
        let context = match cx.http_context(Scope::Background, policy) {
            Ok(context) => context,
            Err(error) => {
                self.public_failed(cx, markets, error.read_venue_error());
                return;
            }
        };
        let pending = with_discovery_context(pending, context.clone());
        let headers: &'static [(&'static str, &'static str)] = if self.options.testnet {
            &[("x-simulated-trading", "1")]
        } else {
            &[]
        };
        match cx.request_managed_with_headers(
            context.clone(),
            None,
            Method::Get,
            url,
            headers,
            None,
            pending,
        ) {
            Ok(id) => cx.stream_response(id, stream),
            Err((_, error)) => {
                let _ = cx.finish_http_context(&context);
                self.public_failed(cx, markets, error.read_venue_error());
            }
        }
    }

    /// Retire a failed discovery read's Context before the native failure path.
    /// A failure (including its own deadline) always belongs to that read; only
    /// successful deliveries are gated on live custody.
    fn retire_discovery(cx: &mut Cx<'_, Pending>, pending: &Pending) {
        if let Some(context) = discovery_context(pending) {
            let _ = cx.finish_http_context(&context);
        }
    }

    /// Product chains carry earlier products' rows so a later failure republishes nothing.
    fn load_markets(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        kind: InstrumentType,
        rows: Vec<Market>,
        incomplete: bool,
    ) {
        let private = self.options.eea && self.credentials.is_some();
        let scope = if private { "account" } else { "public" };
        self.discovery_read(
            cx,
            &format!("/api/v5/{scope}/instruments?instType={}", kind.as_str()),
            private,
            Pending::Markets {
                context: None,
                kind,
                scope_start: rows.len(),
                incomplete,
                rows,
                invalid: false,
            },
        );
    }

    fn load_catalogue(&mut self, cx: &mut Cx<'_, Pending>) {
        self.skipping_instruments.clear();
        self.catalogue_partial = false;
        self.load_markets(cx, InstrumentType::Swap, Vec::new(), false);
    }

    fn load_tickers(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        kind: InstrumentType,
        rows: Vec<Ticker>,
        incomplete: bool,
        seeded: u32,
    ) {
        self.discovery_read(
            cx,
            &format!("/api/v5/market/tickers?instType={}", kind.as_str()),
            false,
            Pending::Tickers {
                context: None,
                kind,
                scope_start: rows.len(),
                incomplete,
                rows,
                invalid: false,
                seeded,
            },
        );
    }

    /// Both keys behind one role-based selection; each caller passes its native gate.
    pub(crate) fn read_slots(&self, policy: SlotPolicy) -> Slots<Identity> {
        Slots::new(
            self.credentials.clone(),
            self.options.read_credentials.clone(),
            policy,
        )
    }

    /// Private reads prefer the read-only key; the primary key is the fallback.
    pub(crate) fn read_credential(&self) -> Option<Credential> {
        self.read_slots(SlotPolicy::default())
            .select(CredentialRole::Read { scope: None })
            .ok()
            .map(|(selection, identity)| selection.credential(identity.credential.clone()))
    }

    /// One signed private GET under engine-issued Read authority. A queue
    /// refusal is a pre-send failure, as a provider refusal is.
    pub(crate) fn managed_get(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        path: &str,
        pending: Pending,
    ) {
        let identity = match context.credential().map(|c| c.slot) {
            Some(CredentialSlot::Secondary) => self.options.read_credentials.clone(),
            _ => self.credentials.clone(),
        };
        let recipe = sign::Http::new(identity, path.into()).demo(self.options.testnet);
        if let Err((pending, error)) = cx.request_signed_managed(
            context,
            None,
            Method::Get,
            format!("{}{path}", self.rest()),
            None,
            Box::new(recipe),
            pending,
        ) {
            self.on_sign_error(cx, pending, error.venue_error());
        }
    }

    /// The recent page is sent on the admitted root; the history fallback on
    /// a leg of it, so both share the command's original deadline and a
    /// command holds at most two of the registry's shared Context slots.
    fn history_candles(&mut self, cx: &mut Cx<'_, Pending>, root: Context, read: CandleRead) {
        match cx.http_leg_context(&root, candle_policy()) {
            Ok(leg) => self.send_candles(cx, root, leg, read),
            // The recent page already went out: keep the native soft shape.
            Err(reason) => {
                let message = reason.venue_error().message;
                if cx.command_error_context(&root, message).is_ok() {
                    let _ = cx.done_context(&root, "[]".into());
                }
            }
        }
    }

    /// Candles are public: unsigned on every alias, demo header on demo.
    fn send_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        root: Context,
        leg: Context,
        read: CandleRead,
    ) {
        let url = format!("{}{}", self.rest(), read.path());
        let pending = Pending::Candles {
            root,
            leg: leg.clone(),
            rows: Vec::new(),
            invalid: false,
            read,
        };
        let sent = if self.options.testnet {
            cx.request_managed_with_headers(
                leg,
                None,
                Method::Get,
                url,
                sign::DEMO_HEADERS,
                None,
                pending,
            )
        } else {
            cx.request_managed(leg, None, Method::Get, url, None, pending)
        };
        match sent {
            Ok(id) => cx.stream_response(id, ArrayAt::EnvelopeField("data")),
            Err((pending, reason)) => self.on_sign_error(cx, pending, reason.venue_error()),
        }
    }

    fn ensure_feed(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        if let Err(error) = self.managed_connect(cx, channel) {
            cx.notice(
                Level::Error,
                code::STREAM_UNAVAILABLE,
                format!("Managed socket connect refused: {error:?}"),
            );
        }
    }
    fn channel(&self, cx: &Cx<'_, Pending>, socket: SocketId) -> Option<usize> {
        self.connections.iter().position(|c| {
            c.feed
                .and_then(|f| cx.socket_epoch(f))
                .is_some_and(|e| e.socket() == socket)
        })
    }
    fn topics(&self, cx: &mut Cx<'_, Pending>, channel: usize, _op: &str, _args: &[Topic<'_>]) {
        self.subscribe(cx, channel);
    }
    fn subscribe(&self, cx: &mut Cx<'_, Pending>, channel: usize) {
        if let Err(error) = self.propose_interests(cx, channel) {
            cx.notice(
                Level::Error,
                code::SUBSCRIPTION_REJECTED,
                format!("Managed socket interest admission refused: {error:?}"),
            );
            if let Some(feed) = self.connections[channel].feed {
                cx.stop_socket_feed(feed);
            }
        }
    }
    fn propose_interests(
        &self,
        cx: &mut Cx<'_, Pending>,
        channel: usize,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        if let Some(feed) = self.connections[channel].feed {
            let desired = self.desired_socket_interests(cx.store, channel);
            cx.set_socket_interests(feed, desired)?;
        }
        Ok(())
    }

    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, message: wire::Message<'_>) {
        let Some(data) = message.data else {
            return;
        };
        let channel = message.arg.channel;
        if channel == "books" {
            let Some(m) = cx.store.market_by_id(message.arg.inst_id) else {
                return;
            };
            let symbol = m.symbol.clone();
            // The TS book contract scales by lotSz, unlike orders/fills (ctVal).
            let step = m.precision.amount;
            let Some(state) = self.books.get_mut(&symbol) else {
                return;
            };
            if state.recovering && message.action != "snapshot" {
                return;
            }
            let decoded = serde_json::from_str::<Vec<wire::Depth<'_>>>(data.get());
            let valid = decoded.as_ref().is_ok_and(|rows| {
                !rows.is_empty()
                    && rows.iter().all(|d| {
                        d.bids.iter().chain(&d.asks).all(|row| {
                            row.len() >= 2
                                && row[0]
                                    .parse::<f64>()
                                    .is_ok_and(|v| v.is_finite() && v > 0.0)
                                && row[1]
                                    .parse::<f64>()
                                    .is_ok_and(|v| v.is_finite() && v >= 0.0)
                        })
                    })
            });
            let mut gap = !valid || !matches!(message.action, "snapshot" | "update");
            if !gap {
                for d in decoded.expect("validated depth") {
                    if message.action == "snapshot" {
                        state.book.clear();
                        state.sequence = Some(d.seq_id);
                        state.recovering = false;
                    } else if state.recovering {
                        return;
                    } else if state.sequence.is_none() || d.prev_seq_id != state.sequence {
                        gap = true;
                        break;
                    } else {
                        state.sequence = Some(d.seq_id);
                    }
                    for (side, rows) in [(BookSide::Bids, d.bids), (BookSide::Asks, d.asks)] {
                        for row in rows {
                            state.book.apply(
                                side,
                                num::parse(row[0]),
                                scaled(num::parse(row[1]), step),
                            );
                        }
                    }
                    cx.book_changed(&symbol);
                }
            }
            if gap {
                state.sequence = None;
                state.recovering = true;
            }
            if gap {
                let id = message.arg.inst_id;
                if let Some(feed) = self.connections[PUBLIC].feed {
                    let key =
                        serde_json::to_string(&Topic::instrument("books", id)).expect("topic");
                    if let Err(error) = cx.repair_socket_interests(feed, vec![key]) {
                        cx.notice(
                            Level::Error,
                            code::SUBSCRIPTION_REJECTED,
                            format!("Managed socket repair refused: {error:?}"),
                        );
                        cx.stop_socket_feed(feed);
                    }
                }
            }
            return;
        }
        if let Some(interval) = channel.strip_prefix("candle") {
            let Some(interval) = interval_for_bar(interval) else {
                return;
            };
            if let Some(m) = cx.store.market_by_id(message.arg.inst_id) {
                let symbol = m.symbol.clone();
                if !self.candles.contains_key(&(symbol.clone(), interval)) {
                    return;
                }
                let _ = json::each::<Vec<&str>>(data.get(), |row| {
                    if let Some(c) = wire::candle(&row) {
                        cx.candle(&symbol, interval, c);
                    }
                });
            }
            return;
        }
        let _ = json::each::<wire::Prices<'_>>(data.get(), |p| {
            if channel == "tickers"
                && [
                    p.last,
                    p.open24h,
                    p.bid_px,
                    p.ask_px,
                    p.vol_ccy24h,
                    p.vol24h,
                ]
                .iter()
                .any(|value| {
                    !value.is_empty()
                        && value
                            .parse::<f64>()
                            .ok()
                            .is_none_or(|value| !value.is_finite())
                })
            {
                return;
            }
            let mut update = |id: &str| {
                let Some(index) = self.price_tickers.get(id).copied() else {
                    return;
                };
                if channel == "mark-price" && !self.marks.contains(id) {
                    self.marks.insert(id.to_owned());
                }
                if channel == "index-tickers" && !self.indexes.contains(id) {
                    self.indexes.insert(id.to_owned());
                }
                // Like TS, live prices only update rows the REST ticker read listed.
                let Some(index) = index else {
                    return;
                };
                cx.store.update_ticker_at(index, |t| match channel {
                    "tickers" => {
                        t.bid = num::parse(p.bid_px);
                        t.ask = num::parse(p.ask_px);
                        if !p.last.is_empty() {
                            t.last = num::parse(p.last);
                        }
                        if !p.last.is_empty() && !p.open24h.is_empty() {
                            t.percentage = percentage(p.last, p.open24h);
                        }
                        t.volume = num::parse(p.vol_ccy24h);
                        t.quote_volume = num::parse(p.vol24h);
                    }
                    // A blank or null live value keeps the last good one.
                    "mark-price" if !p.mark_px.is_empty() => t.mark = num::parse(p.mark_px),
                    "index-tickers" if !p.idx_px.is_empty() => t.index = num::parse(p.idx_px),
                    "open-interest" if !p.oi_ccy.is_empty() => {
                        t.open_interest = num::parse(p.oi_ccy)
                    }
                    "funding-rate" => {
                        if !p.funding_rate.is_empty() {
                            t.funding_rate = num::parse(p.funding_rate);
                        }
                        if !p.funding_time.is_empty() {
                            t.funding_time = num::parse(p.funding_time);
                        }
                    }
                    _ => {}
                });
                let mask = match channel {
                    "tickers" => {
                        3 | (1 << 9)
                            | (1 << 10)
                            | if !p.last.is_empty() { 1 << 2 } else { 0 }
                            | if !p.last.is_empty() && !p.open24h.is_empty() {
                                1 << 5
                            } else {
                                0
                            }
                    }
                    "mark-price" => 1 << 3,
                    "index-tickers" => 1 << 4,
                    "open-interest" => 1 << 6,
                    "funding-rate" => (1 << 7) | (1 << 8),
                    _ => 0,
                };
                if let Some(ticker) = cx.store.tickers().get(index as usize) {
                    self.live_tickers.record(cx.store, &ticker.symbol, mask);
                }
            };
            if channel == "index-tickers" {
                if let Some(ids) = self.underlying_products.get(p.inst_id) {
                    for id in ids {
                        update(id);
                    }
                }
            } else {
                update(p.inst_id);
            }
        });
        self.account_prices(cx);
    }

    fn request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Pending::Account(request) = pending {
            let error =
                safe_cex_engine::VenueError::new(error).kind(safe_cex_engine::ErrorKind::Network);
            self.account_failed(cx, request, error);
        } else if let Pending::Trade(request) = pending {
            self.trade_lost(cx, request, error);
        } else if let Pending::History(request) = pending {
            self.history_failed(cx, *request, error);
        } else if let Pending::Closed(request) = pending {
            Self::closed_failed(cx, *request, error);
        } else if let Pending::Candles { root, .. } = pending {
            // Native soft shape for any post-send failure, deadline included.
            // Not gated on read delivery: an expired leg would never finish.
            if cx.command_error_context(&root, error).is_ok() {
                let _ = cx.done_context(&root, "[]".into());
            }
        } else {
            self.public_failed(
                cx,
                matches!(pending, Pending::Markets { .. }),
                safe_cex_engine::VenueError::new(error).kind(safe_cex_engine::ErrorKind::Network),
            );
        }
    }
}

impl Venue for Okx {
    fn managed_sockets() -> bool {
        true
    }
    fn socket_control(
        &self,
        _store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        authenticated: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        self.managed_control(feed, authenticated, text)
    }
    fn socket_startup_ready(&self, _store: &safe_cex_engine::Store) -> bool {
        self.socket_account_is_ready()
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.managed_event(cx, event)
    }
    fn on_socket_interests_released(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        _epoch: safe_cex_engine::socket_lifecycle::Epoch,
        pending: Pending,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        if let Pending::SocketRefresh { reopened } = pending {
            self.socket_account_ready(cx, reopened);
        }
        Ok(())
    }

    fn encode_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        items: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.managed_encode(store, feed, subscribe, items, emit)
    }

    fn guard_pending_placements(&self) -> bool {
        true
    }
    // reconcile_uncertain reads the uncertain symbols' orders at once.
    fn fence_uncertain_placements(&self) -> bool {
        true
    }

    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        match pending {
            Pending::Trade(request) => trading::trade_reconciliation(request),
            _ => Err(safe_cex_engine::request_context::Error::Invalid),
        }
    }

    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.retire_trading(owner);
    }

    /// The engine asks once per uncertain seal; the fresh account snapshot
    /// reports through `orders_reconciled`.
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _symbols: Option<&[String]>) {
        self.account_after_uncertain_write(cx);
    }

    // Frame-fed: the data source reads the config before its orders and
    // positions, so once both have arrived the hedged flag is the account's.
    fn refresh_settings(&mut self, cx: &mut Cx<'_, Pending>, _symbol: Option<&str>) {
        self.account_refresh_settings(cx);
    }
    fn on_frame_ingested(&mut self, cx: &mut Cx<'_, Pending>) {
        let loaded = cx.store.loaded();
        self.account.ready = loaded.orders && loaded.positions;
        // An acknowledged placement the store now holds cancels through the store.
        let stored: std::collections::HashSet<&str> = cx
            .store
            .orders()
            .iter()
            .map(|order| order.id.as_str())
            .collect();
        self.trading
            .acknowledged
            .retain(|id, _| !stored.contains(id.as_str()));
    }

    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            position_history: safe_cex_engine::PositionHistory::Okx,
            closed_position_format: safe_cex_engine::ClosedPositionFormat::Okx,
            quantity_units: safe_cex_engine::QuantityUnits::ContractCount,
            cancel_all: safe_cex_engine::CancelAll::EachPlacedSymbol,
            client_order_lookup: true,
            leverage_query: safe_cex_engine::LeverageQuery::OnRead,
            closed_positions: true,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account_prices(cx);
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.live_tickers.begin();
        let validation = self
            .credentials
            .iter()
            .chain(self.options.read_credentials.iter())
            .try_for_each(|identity| {
                identity.validate().and_then(|_| {
                    identity
                        .passphrase
                        .as_ref()
                        .filter(|r| !r.0.is_empty())
                        .map(|_| ())
                        .ok_or("OKX signing credentials require a passphrase")
                })
            });
        if let Err(error) = validation {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            self.account.startup_rejected = true;
            cx.startup_failed(
                safe_cex_engine::VenueError::new(error)
                    .kind(safe_cex_engine::ErrorKind::InvalidInput),
            );
            return;
        }
        self.load_catalogue(cx);
    }

    fn on_open(&mut self, _cx: &mut Cx<'_, Pending>, _socket: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(channel) = self.channel(cx, socket) else {
            return;
        };
        let Ok(message) = serde_json::from_str::<wire::Message<'_>>(text) else {
            return;
        };
        if message.event == "error" {
            cx.notice(
                Level::Error,
                code::SUBSCRIPTION_REJECTED,
                format!("OKX {}: {}", message.code, message.msg),
            );
            return;
        }
        if channel != PUBLIC
            && (message.event == "login"
                || matches!(
                    message.arg.channel,
                    "positions" | "orders" | "orders-algo" | "algo-advance" | "account"
                ))
        {
            self.private_message(cx, channel, message);
        } else {
            self.public_message(cx, message);
        }
    }

    fn on_close(&mut self, _cx: &mut Cx<'_, Pending>, _socket: SocketId, _code: u16) {}
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        // A discovery row decodes only for its own exact Background delivery.
        if !discovery_delivered(cx, pending) {
            return;
        }
        match pending {
            Pending::SocketRefresh { .. } => (),
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::Trade(_) => {}
            Pending::History(request) => self.history_item(cx, request, item),
            Pending::Closed(request) => Self::closed_item(cx, request, item),
            Pending::Markets {
                kind,
                rows,
                invalid,
                ..
            } => match serde_json::from_str::<wire::Instrument<'_>>(item) {
                Ok(m) => {
                    if m.inst_id.is_empty() {
                        *invalid = true;
                        safe_cex_engine::startup::skipped_row(cx);
                        return;
                    }
                    if (!m.ct_type.is_empty() && !matches!(m.ct_type, "linear" | "inverse"))
                        || (!m.inst_type.is_empty() && m.inst_type != kind.as_str())
                        || (*kind == InstrumentType::Futures
                            && !m.rule_type.is_empty()
                            && m.rule_type != "xperp")
                    {
                        return;
                    }
                    // A blank or null classifier is malformed, not a different product.
                    if m.inst_type.is_empty()
                        || m.ct_type.is_empty()
                        || (*kind == InstrumentType::Futures && m.rule_type.is_empty())
                    {
                        *invalid = true;
                        safe_cex_engine::startup::skipped_row(cx);
                        self.skipping_instruments.insert(m.inst_id.into());
                        return;
                    }
                    let inverse = m.ct_type == "inverse";
                    let underlying = if !m.uly.is_empty() {
                        m.uly
                    } else if !m.inst_family.is_empty() {
                        m.inst_family
                    } else {
                        m.inst_id.strip_suffix("-SWAP").unwrap_or(m.inst_id)
                    };
                    let mut family = if !m.uly.is_empty() {
                        m.uly
                    } else {
                        m.inst_family
                    }
                    .split('-');
                    let family_base = family.next().unwrap_or_default();
                    let family_quote = family.next().unwrap_or_default();
                    let base = if !m.base_ccy.is_empty() {
                        m.base_ccy
                    } else if inverse {
                        m.settle_ccy
                    } else {
                        m.ct_val_ccy
                    };
                    let quote = if !m.quote_ccy.is_empty() {
                        m.quote_ccy
                    } else if inverse {
                        m.ct_val_ccy
                    } else {
                        m.settle_ccy
                    };
                    let base = if base.is_empty() { family_base } else { base };
                    let quote = if quote.is_empty() {
                        family_quote
                    } else {
                        quote
                    };
                    if m.inst_id.is_empty()
                        || rows.iter().any(|row| row.id == m.inst_id)
                        || base.is_empty()
                        || quote.is_empty()
                        || (*kind == InstrumentType::Swap && !m.inst_id.ends_with("-SWAP"))
                        || [m.ct_val, m.lot_sz, m.tick_sz, m.min_sz, m.lever]
                            .iter()
                            .any(|n| {
                                n.parse::<f64>()
                                    .ok()
                                    .is_none_or(|value| !value.is_finite() || value <= 0.0)
                            })
                    {
                        {
                            *invalid = true;
                            safe_cex_engine::startup::skipped_row(cx);
                        };
                        self.skipping_instruments.insert(m.inst_id.into());
                        return;
                    }
                    let contract_size = num::parse(m.ct_val);
                    let caps = [
                        m.max_iceberg_sz,
                        m.max_lmt_sz,
                        m.max_mkt_sz,
                        m.max_stop_sz,
                        m.max_trigger_sz,
                        m.max_twap_sz,
                    ];
                    if caps.iter().any(|cap| {
                        !cap.is_empty()
                            && cap
                                .parse::<f64>()
                                .ok()
                                .is_none_or(|value| !value.is_finite() || value <= 0.0)
                    }) {
                        {
                            *invalid = true;
                            safe_cex_engine::startup::skipped_row(cx);
                        };
                        self.skipping_instruments.insert(m.inst_id.into());
                        return;
                    }
                    let max = caps
                        .into_iter()
                        .filter(|cap| !cap.is_empty())
                        .map(num::parse)
                        .fold(f64::INFINITY, f64::min);
                    if max <= 0.0 || !max.is_finite() {
                        {
                            *invalid = true;
                            safe_cex_engine::startup::skipped_row(cx);
                        };
                        self.skipping_instruments.insert(m.inst_id.into());
                        return;
                    }
                    self.products.insert(
                        m.inst_id.into(),
                        Product {
                            kind: *kind,
                            underlying: underlying.into(),
                            settle_ccy: m.settle_ccy.into(),
                        },
                    );
                    rows.push(Market {
                        id: m.inst_id.into(),
                        symbol: if *kind == InstrumentType::Futures {
                            m.inst_id.replace('-', "")
                        } else {
                            m.inst_id
                                .strip_suffix("-SWAP")
                                .unwrap_or(m.inst_id)
                                .replace('-', "")
                        },
                        pretty_symbol: (*kind == InstrumentType::Futures)
                            .then(|| format!("{base}/{quote} XPERP")),
                        base: base.to_uppercase(),
                        quote: quote.to_uppercase(),
                        active: m.state == "live",
                        precision: Precision {
                            amount: num::parse(m.lot_sz),
                            price: num::parse(m.tick_sz),
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: scaled(num::parse(m.min_sz), contract_size),
                                max,
                            },
                            leverage: MinMax {
                                min: 1.0,
                                max: num::parse(m.lever),
                            },
                            price: None,
                        },
                        contract_size: Some(contract_size),
                        is_inverse: Some(inverse),
                        is_expiring: None,
                        is_spot: None,
                        is_perpetual: None,
                        is_read_only: None,
                        taker_fee: None,
                        maker_fee: None,
                    });
                }
                Err(_) => {
                    *invalid = true;
                    safe_cex_engine::startup::skipped_row(cx);
                    if let Ok(row) = serde_json::from_str::<wire::InstrumentId<'_>>(item)
                        && !row.inst_id.is_empty()
                    {
                        self.skipping_instruments.insert(row.inst_id.into());
                    }
                }
            },
            Pending::Tickers {
                kind,
                rows,
                invalid,
                seeded,
                ..
            } => match serde_json::from_str::<wire::Prices<'_>>(item) {
                Ok(p) => {
                    if let Some(m) = cx.store.market_by_id(p.inst_id) {
                        if self.instrument_type(&m.id) != kind.as_str() {
                            return;
                        }
                        // Untraded rows carry blank quotes; seed unusable values as zero
                        // so the market keeps a row for live updates to patch.
                        if [
                            p.last,
                            p.open24h,
                            p.bid_px,
                            p.ask_px,
                            p.vol_ccy24h,
                            p.vol24h,
                        ]
                        .iter()
                        .any(|value| {
                            !value.is_empty()
                                && safe_cex_engine::startup::number_or_zero(value).is_none()
                        }) {
                            *seeded += 1;
                        }
                        let last = num::parse(p.last);
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            bid: num::parse(p.bid_px),
                            ask: num::parse(p.ask_px),
                            last,
                            mark: last,
                            index: last,
                            percentage: percentage(p.last, p.open24h),
                            volume: num::parse(p.vol_ccy24h),
                            quote_volume: num::parse(p.vol24h),
                            ..Ticker::default()
                        });
                    } else {
                        *invalid = true;
                        safe_cex_engine::startup::skipped_row(cx);
                    }
                }
                Err(_) => {
                    *invalid = true;
                    safe_cex_engine::startup::skipped_row(cx);
                }
            },
            Pending::Candles {
                root,
                leg,
                rows,
                invalid,
                ..
            } => {
                if cx.validate_read_http_delivery(root, leg).is_err() {
                    return;
                }
                match serde_json::from_str::<Vec<&str>>(item)
                    .ok()
                    .and_then(|r| wire::candle(&r))
                {
                    Some(c) => rows.push(c),
                    None => *invalid = true,
                }
            }
        }
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Closed(read) = &pending
            && !read.valid_delivery(cx)
        {
            return;
        }
        if let Pending::History(read) = &pending
            && !read.valid_delivery(cx)
        {
            return;
        }
        if let Pending::Account(account::Request::Config { context, .. }) = &pending {
            let _ = cx.finish_http_context(context);
        }
        // An account-cycle page passes custody and generation before any codec,
        // publishes, then retires; a stale or unauthorized one only retires.
        let page = match &pending {
            Pending::Account(request) => request.page_context().cloned(),
            _ => None,
        };
        if let Pending::Account(request) = &pending
            && !self.account_delivery_current(cx, request)
        {
            account::retire(cx, page.as_ref());
            return;
        }
        if let Pending::Account(account::Request::Config { .. }) = &pending
            && self.config_rejected(cx, response.body)
        {
            return;
        }
        if let Pending::Trade(request) = pending {
            self.trade_response(cx, request, response);
            return;
        }
        let pending = match pending {
            Pending::History(request) if request.is_lookup() => {
                self.history_lookup_response(cx, *request, response);
                return;
            }
            other => other,
        };
        if matches!(pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
            let pause = safe_cex_engine::startup::rate_limit_pause_ms(&response, 10_000.0);
            self.discovery_paused_until = self.discovery_paused_until.max(cx.now + pause);
        }
        // Gate before the envelope; the Context retires after publication below.
        let discovery = discovery_context(&pending);
        if !discovery_delivered(cx, &pending) {
            Self::retire_discovery(cx, &pending);
            return;
        }
        if let Pending::Account(account::Request::Config { .. }) = &pending {
            self.account.config_pause_ms =
                safe_cex_engine::startup::rate_limit_pause_ms(&response, 10_000.0);
        }
        let result = if (200..300).contains(&response.status) {
            wire::envelope(response.body).map(|_| ())
        } else {
            Err(format!("OKX HTTP {}: {}", response.status, response.body))
        };
        if let Err(error) = result {
            if let Some(context) = &discovery {
                let _ = cx.finish_http_context(context);
            }
            if matches!(pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
                self.public_failed(
                    cx,
                    matches!(pending, Pending::Markets { .. }),
                    discovery_error(&response),
                );
                return;
            }
            // Account reads take the table's kind from the reply itself.
            match pending {
                Pending::Account(request) => {
                    let error = account::read_error(&response, error);
                    self.account_failed(cx, request, error);
                }
                pending => self.request_failed(cx, pending, &error),
            }
            return;
        }
        match pending {
            Pending::SocketRefresh { .. } => (),
            Pending::Account(request) => {
                self.account_response(cx, request, response);
                account::retire(cx, page.as_ref());
            }
            Pending::Trade(request) => self.trade_response(cx, request, response),
            Pending::History(request) => self.history_response(cx, *request, response),
            Pending::Closed(request) => self.closed_response(cx, *request),
            Pending::Markets {
                kind,
                rows,
                invalid,
                scope_start,
                incomplete,
                ..
            } => 'settle: {
                let incomplete = incomplete || (invalid && rows.len() == scope_start);
                self.catalogue_partial |= invalid;
                if let Some(next) = self.next_instrument_type(kind) {
                    // The settled product retires before the next one is admitted.
                    if let Some(context) = &discovery {
                        let _ = cx.finish_http_context(context);
                    }
                    self.load_markets(cx, next, rows, incomplete);
                    break 'settle;
                }
                self.catalogue_incomplete = incomplete;
                if rows.is_empty() && !cx.store.loaded().markets {
                    if let Some(context) = &discovery {
                        let _ = cx.finish_http_context(context);
                    }
                    self.public_failed(
                        cx,
                        true,
                        safe_cex_engine::VenueError::new("Empty OKX catalogue")
                            .kind(safe_cex_engine::ErrorKind::VenueUnavailable),
                    );
                    break 'settle;
                }
                // A partial catalogue commits so private reads start, and is re-read.
                if incomplete || self.catalogue_partial {
                    self.public_failed(
                        cx,
                        true,
                        safe_cex_engine::VenueError::new("Incomplete OKX catalogue")
                            .kind(safe_cex_engine::ErrorKind::VenueUnavailable),
                    );
                } else {
                    self.startup.succeeded(cx, DiscoveryRead::Markets);
                }
                if rows.is_empty() && incomplete {
                    break 'settle;
                }
                let was_loaded = cx.store.loaded().markets;
                let mut merged = cx.store.markets().to_vec();
                let mut added = false;
                for row in rows {
                    if let Some(old) = merged.iter_mut().find(|old| old.id == row.id) {
                        *old = row;
                    } else {
                        merged.push(row);
                        added = true;
                    }
                }
                cx.store.set_markets(merged);
                self.startup.ready(cx);
                let mut skipped = std::mem::take(&mut self.skipping_instruments);
                skipped.retain(|id| cx.store.market_by_id(id).is_none());
                self.skipped_instruments = skipped;
                self.products
                    .retain(|id, _| cx.store.market_by_id(id).is_some());
                self.price_tickers = price_ticker_indexes(cx.store);
                self.underlying_products.clear();
                for market in cx.store.markets() {
                    if let Some(product) = self.products.get(&market.id) {
                        self.underlying_products
                            .entry(product.underlying.clone())
                            .or_default()
                            .push(market.id.clone());
                    }
                }
                // A frame-fed engine loads public instrument metadata only.
                if cx.frame_fed() {
                } else if !was_loaded {
                    self.ticker_seed(cx, false);
                    self.account_start(cx);
                } else if added {
                    self.ticker_seed(cx, true);
                    if self.connections[PUBLIC]
                        .feed
                        .and_then(|f| cx.socket_epoch(f))
                        .is_some()
                    {
                        self.subscribe(cx, PUBLIC);
                    }
                    if self.connections[BUSINESS]
                        .feed
                        .and_then(|f| cx.socket_epoch(f))
                        .is_some()
                    {
                        self.subscribe(cx, BUSINESS);
                    }
                }
            }
            Pending::Tickers {
                kind,
                rows,
                invalid,
                scope_start,
                incomplete,
                seeded,
                ..
            } => 'settle: {
                let incomplete = incomplete || (invalid && rows.len() == scope_start);
                if let Some(next) = self.next_instrument_type(kind) {
                    if let Some(context) = &discovery {
                        let _ = cx.finish_http_context(context);
                    }
                    self.load_tickers(cx, next, rows, incomplete, seeded);
                    break 'settle;
                }
                if seeded > 0 {
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        format!("OKX seeded {seeded} malformed ticker rows as zero"),
                    );
                }
                self.ticker_reading = false;
                let refresh = std::mem::take(&mut self.ticker_refresh_pending);
                if incomplete {
                    self.public_failed(
                        cx,
                        false,
                        safe_cex_engine::VenueError::new("Incomplete OKX tickers")
                            .kind(safe_cex_engine::ErrorKind::VenueUnavailable),
                    );
                } else {
                    self.startup.succeeded(cx, DiscoveryRead::Tickers);
                }
                if rows.is_empty() && incomplete {
                    if refresh {
                        self.ticker_seed(cx, false);
                    }
                    break 'settle;
                }
                let mut rows = rows;
                if incomplete || self.catalogue_incomplete || refresh {
                    for old in cx.store.tickers() {
                        if !rows.iter().any(|row| row.id == old.id) {
                            rows.push(old.clone());
                        }
                    }
                    self.live_tickers.overlay(cx.store, &mut rows);
                } else {
                    self.live_tickers.finish(cx.store, &mut rows);
                }
                cx.store.set_tickers(rows);
                self.startup.ready(cx);
                self.price_tickers = price_ticker_indexes(cx.store);
                self.account_prices(cx);
                if self.connections[PUBLIC].feed.is_none() {
                    self.ensure_feed(cx, PUBLIC);
                }
                if self.connections[BUSINESS].feed.is_none() {
                    self.ensure_feed(cx, BUSINESS);
                }
                if refresh {
                    self.ticker_seed(cx, false);
                }
            }
            Pending::Candles {
                root,
                leg,
                mut rows,
                invalid: false,
                mut read,
            } => {
                if cx.validate_read_http_delivery(&root, &leg).is_err() {
                    return;
                }
                if !read.historical && (rows.is_empty() || read.force_history) {
                    read.historical = true;
                    self.history_candles(cx, root, read);
                    return;
                }
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                let _ = match serde_json::to_string(&rows) {
                    Ok(data) => cx.done_context(&root, data),
                    Err(error) => cx.fail_context(&root, error.to_string()),
                };
            }
            other => self.on_request_failed(cx, other, "malformed OKX list item"),
        }
        if let Some(context) = &discovery {
            let _ = cx.finish_http_context(context);
        }
    }

    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        status: u16,
        _headers: &str,
        error: &str,
    ) {
        if matches!(pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
            Self::retire_discovery(cx, &pending);
            self.public_failed(
                cx,
                matches!(pending, Pending::Markets { .. }),
                safe_cex_engine::startup::public_response_error(status, None, None),
            );
        } else {
            self.on_request_failed(cx, pending, error);
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        if matches!(pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
            let pause = safe_cex_engine::startup::rate_limit_pause_ms(&response, 10_000.0);
            self.discovery_paused_until = self.discovery_paused_until.max(cx.now + pause);
            Self::retire_discovery(cx, &pending);
            self.public_failed(
                cx,
                matches!(pending, Pending::Markets { .. }),
                discovery_error(&response),
            );
        } else {
            self.on_request_failed(
                cx,
                pending,
                &format!(
                    "response did not contain the expected list: {}",
                    response.body
                ),
            );
        }
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Trade(request) => self.trade_sign_failed(cx, request, error),
            Pending::Closed(request) => Self::closed_sign_failed(cx, *request, error),
            Pending::Account(account::Request::Leverage { context, .. }) => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::Candles { root, .. } => {
                let _ = cx.fail_context(&root, error);
            }
            Pending::History(read) => Self::history_sign_failed(cx, *read, error),
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }

    fn read_root_retired(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
        _pending: Vec<Pending>,
    ) {
        self.retire_history(owner);
        self.retire_closed(owner);
    }

    /// The managed leverage read keeps the engine's typed failure, e.g. Timeout.
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::History(read) => self.history_failed(cx, *read, error),
            Pending::Closed(read) => Self::closed_http_failed(cx, *read, error),
            Pending::Account(account::Request::Leverage { context, .. }) => {
                let _ = cx.fail_context(&context, error);
            }
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        // A failed discovery read retires first, then takes the native failure path.
        if matches!(pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
            Self::retire_discovery(cx, &pending);
        }
        if let Pending::Account(account::Request::Config { context, .. }) = &pending {
            let _ = cx.finish_http_context(context);
            self.config_unanswered();
        }
        self.request_failed(cx, pending, error);
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(read) = self.startup.on_timer(timer) {
            if read == DiscoveryRead::Markets && cx.store.loaded().markets {
                self.load_catalogue(cx);
            } else if read == DiscoveryRead::Markets {
                self.start(cx);
            } else if !cx.frame_fed() {
                self.ticker_seed(cx, false);
            }
            return;
        }
        if self.history_timer(cx, timer) || self.closed_timer(cx, timer) {
            return;
        }
        self.account_timer(cx, timer);
    }

    fn on_venue_action(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        action: safe_cex_engine::VenueAction,
    ) {
        match action {
            safe_cex_engine::VenueAction::FetchClosedPositions {
                symbol,
                from_ms,
                to_ms,
            } => self.start_closed_positions(cx, command, symbol, from_ms, to_ms),
            _ => safe_cex_engine::unsupported_venue_action(cx, command),
        }
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                let state = self.books.entry(symbol.clone()).or_default();
                state.subscribers += 1;
                if state.subscribers == 1
                    && let Some(m) = cx.store.market(&symbol)
                {
                    let id = m.id.clone();
                    self.topics(cx, PUBLIC, "subscribe", &[Topic::instrument("books", &id)]);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers = state.subscribers.saturating_sub(1);
                    if state.subscribers > 0 {
                        return;
                    }
                }
                self.books.remove(&symbol);
                if let Some(m) = cx.store.market(&symbol) {
                    let id = m.id.clone();
                    self.topics(
                        cx,
                        PUBLIC,
                        "unsubscribe",
                        &[Topic::instrument("books", &id)],
                    );
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                let Some(bar) = bar(interval) else {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("OKX"),
                    );
                    return;
                };
                let count = self.candles.entry((symbol.clone(), interval)).or_default();
                *count += 1;
                if *count == 1
                    && let Some(m) = cx.store.market(&symbol)
                {
                    let id = m.id.clone();
                    let name = format!("candle{bar}");
                    self.topics(cx, BUSINESS, "subscribe", &[Topic::instrument(&name, &id)]);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                if let Some(count) = self.candles.get_mut(&(symbol.clone(), interval)) {
                    *count = count.saturating_sub(1);
                    if *count > 0 {
                        return;
                    }
                }
                self.candles.remove(&(symbol.clone(), interval));
                if let Some(m) = cx.store.market(&symbol)
                    && let Some(bar) = bar(interval)
                {
                    let id = m.id.clone();
                    let name = format!("candle{bar}");
                    self.topics(
                        cx,
                        BUSINESS,
                        "unsubscribe",
                        &[Topic::instrument(&name, &id)],
                    );
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
                // A rejected admission already published the command's typed failure.
                let Ok(root) = cx.admit_http_command(candle_policy()) else {
                    return;
                };
                let Some(m) = cx.store.market(&symbol) else {
                    let _ = cx.done_context(&root, "[]".into());
                    return;
                };
                if [from_ms, to_ms]
                    .into_iter()
                    .flatten()
                    .any(|v| !v.is_finite())
                {
                    let _ = cx.fail_context(&root, "invalid candle time bounds");
                    return;
                }
                let explicit_limit = limit.filter(|n| *n > 0);
                let limit = explicit_limit.unwrap_or(300).min(300);
                let end = to_ms.unwrap_or(cx.now);
                let Some(default_start) = subtract_bars(end, interval, limit) else {
                    let _ = cx.fail_context(&root, interval.unsupported("OKX"));
                    return;
                };
                let before = if explicit_limit.is_none() {
                    from_ms.unwrap_or(default_start)
                } else {
                    default_start
                };
                if before > end {
                    let _ = cx.fail_context(&root, "invalid candle time bounds");
                    return;
                }
                let read = CandleRead {
                    instrument: m.id.clone(),
                    interval,
                    limit,
                    before,
                    after: end,
                    historical: false,
                    force_history: to_ms.is_some_and(|to| {
                        subtract_bars(cx.now, interval, 1440).is_some_and(|oldest| to < oldest)
                    }),
                };
                self.send_candles(cx, root.clone(), root, read);
            }
            Command::Query { command, query } => self.start_query(cx, command, query),
            other => self.trade_command(cx, other),
        }
    }

    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|b| b.sequence.is_some())
            .map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

/// Public candle reads: one physical attempt per leg inside one 20 s command budget.
fn candle_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

fn price_ticker_indexes(store: &safe_cex_engine::Store) -> HashMap<String, Option<u32>> {
    // Match Store's last duplicate symbol lookup, independently of either table's order.
    let symbols: HashMap<&str, u32> = store
        .tickers()
        .iter()
        .enumerate()
        .map(|(index, ticker)| (ticker.symbol.as_str(), index as u32))
        .collect();
    store
        .markets()
        .iter()
        .map(|market| {
            (
                market.id.clone(),
                symbols.get(market.symbol.as_str()).copied(),
            )
        })
        .collect()
}

#[derive(serde::Serialize)]
struct Topic<'a> {
    channel: &'a str,
    #[serde(rename = "instId", skip_serializing_if = "Option::is_none")]
    inst_id: Option<&'a str>,
    #[serde(rename = "instType", skip_serializing_if = "Option::is_none")]
    inst_type: Option<&'a str>,
}
impl<'a> Topic<'a> {
    fn instrument(channel: &'a str, id: &'a str) -> Self {
        Self {
            channel,
            inst_id: Some(id),
            inst_type: None,
        }
    }
}

const BARS: [(Interval, &str); 14] = [
    (Interval::S1, "1s"),
    (Interval::M1, "1m"),
    (Interval::M3, "3m"),
    (Interval::M5, "5m"),
    (Interval::M15, "15m"),
    (Interval::M30, "30m"),
    (Interval::H1, "1H"),
    (Interval::H2, "2H"),
    (Interval::H4, "4H"),
    (Interval::H6, "6H"),
    (Interval::H12, "12H"),
    (Interval::D1, "1D"),
    (Interval::W1, "1W"),
    (Interval::Mo1, "1Mutc"),
];
fn bar(interval: Interval) -> Option<&'static str> {
    BARS.iter()
        .find(|(i, _)| *i == interval)
        .map(|(_, bar)| *bar)
}
fn interval_for_bar(value: &str) -> Option<Interval> {
    BARS.iter().find(|(_, bar)| *bar == value).map(|(i, _)| *i)
}
fn percentage(last: &str, open: &str) -> f64 {
    let open = num::parse(open);
    match last.parse::<f64>() {
        Ok(last) if last.is_finite() && open != 0.0 => usd((last - open) / open * 100.0),
        _ => 0.0,
    }
}
fn usd(value: f64) -> f64 {
    num::round_half_up(value * 100.0) / 100.0
}
fn scaled(value: f64, factor: f64) -> f64 {
    let digits = num::decimals(value) + num::decimals(factor);
    let scale = 10f64.powi(digits.min(15) as i32);
    (value * factor * scale).round() / scale
}

impl CandleRead {
    fn path(&self) -> String {
        let endpoint = if self.historical {
            "history-candles"
        } else {
            "candles"
        };
        format!(
            "/api/v5/market/{endpoint}?instId={}&bar={}&limit={}&before={}&after={}",
            self.instrument,
            bar(self.interval).unwrap_or_default(),
            if self.historical {
                self.limit.min(100)
            } else {
                self.limit
            },
            num::format(self.before),
            num::format(self.after)
        )
    }
}

fn subtract_bars(end: f64, interval: Interval, count: u32) -> Option<f64> {
    bar(interval)?;
    let duration = match interval {
        Interval::Mo1 => {
            return Some(safe_cex_engine::time::sub_months(end.floor() as i64, count) as f64);
        }
        _ => interval.ms(),
    };
    Some(end - duration * f64::from(count))
}

#[cfg(test)]
mod price_index_tests {
    use super::*;
    use safe_cex_engine::{Action, Engine, Event};

    const MARKET: &str = r#"{"instId":"BTC-USDT-SWAP","instType":"SWAP","ctType":"linear","ctVal":"0.01","ctValCcy":"BTC","settleCcy":"USDT","lotSz":"0.01","tickSz":"0.1","minSz":"0.01","maxIcebergSz":"1000","maxLmtSz":"1000","maxMktSz":"1000","maxStopSz":"1000","maxTriggerSz":"1000","maxTwapSz":"1000","lever":"100"}"#;
    fn request(e: &mut Engine<Okx>) -> safe_cex_engine::HttpRequest {
        safe_cex_engine::test_support::prepared_actions(e, 1.0)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .expect("seed request")
    }
    fn reply(e: &mut Engine<Okx>, request: safe_cex_engine::HttpRequest, rows: &str) {
        e.handle(
            1.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &format!(r#"{{"code":"0","data":[{rows}]}}"#),
                },
            },
        );
    }
    fn engine() -> Engine<Okx> {
        safe_cex_engine::test_support::seeded_engine(Okx::new(Options {
            public_data_only: true,
            ..Options::default()
        }))
    }

    #[test]
    fn successful_catalog_and_ticker_commits_each_rebuild_price_identity() {
        let mut e = engine();
        e.store_mut().set_tickers(vec![
            Ticker {
                symbol: "OTHER".into(),
                ..Ticker::default()
            },
            Ticker {
                symbol: "BTCUSDT".into(),
                ..Ticker::default()
            },
        ]);
        e.start(0.0);
        let r = request(&mut e);
        reply(&mut e, r, MARKET);
        assert_eq!(e.venue().price_tickers.get("BTC-USDT-SWAP"), Some(&Some(1)));
        let r = request(&mut e);
        reply(
            &mut e,
            r,
            r#"{"instId":"BTC-USDT-SWAP","bidPx":"1","askPx":"3","last":"2","open24h":"1","volCcy24h":"4","vol24h":"5"}"#,
        );
        assert_eq!(e.venue().price_tickers.get("BTC-USDT-SWAP"), Some(&Some(0)));
    }

    #[test]
    fn live_prices_after_the_seed_never_create_tickers_the_rest_read_omitted() {
        let mut e = engine();
        e.start(0.0);
        let r = request(&mut e);
        reply(&mut e, r, MARKET);
        let r = request(&mut e);
        reply(&mut e, r, "");
        assert_eq!(e.venue().price_tickers.get("BTC-USDT-SWAP"), Some(&None));
        let socket = e
            .drain_actions()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(socket),
                _ => None,
            })
            .unwrap();
        e.handle(2.0, Event::WsOpened { socket });
        for _ in 0..2 {
            e.handle(3.0, Event::WsMessage { socket, text: r#"{"arg":{"channel":"mark-price"},"data":[{"instId":"BTC-USDT-SWAP","markPx":"2"},{"instId":"UNKNOWN-SWAP","markPx":"3"}]}"# });
            e.handle(3.0, Event::WsMessage { socket, text: r#"{"arg":{"channel":"index-tickers"},"data":[{"instId":"BTC-USDT","idxPx":"4"},{"instId":"UNKNOWN","idxPx":"5"}]}"# });
        }
        assert_eq!(e.venue().marks.len(), 1);
        assert_eq!(e.venue().indexes.len(), 1);
        assert!(e.venue().marks.contains("BTC-USDT-SWAP"));
        assert!(e.venue().indexes.contains("BTC-USDT-SWAP"));
        // TS updates only REST-seeded tickers (gate: CRMLUSDT, MPUSDT index-only rows).
        assert!(e.store().tickers().is_empty());
        assert_eq!(e.venue().price_tickers.get("BTC-USDT-SWAP"), Some(&None));
    }

    #[test]
    fn index_fan_out_after_a_partial_seed_updates_only_rest_listed_tickers() {
        let eth = MARKET.replace("BTC", "ETH");
        let mut e = engine();
        e.start(0.0);
        let r = request(&mut e);
        reply(&mut e, r, &format!("{MARKET},{eth}"));
        let r = request(&mut e);
        reply(
            &mut e,
            r,
            r#"{"instId":"BTC-USDT-SWAP","bidPx":"1","askPx":"3","last":"2","open24h":"1","volCcy24h":"4","vol24h":"5"}"#,
        );
        let socket = e
            .drain_actions()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(socket),
                _ => None,
            })
            .unwrap();
        e.handle(2.0, Event::WsOpened { socket });
        e.handle(3.0, Event::WsMessage { socket, text: r#"{"arg":{"channel":"index-tickers"},"data":[{"instId":"BTC-USDT","idxPx":"4"},{"instId":"ETH-USDT","idxPx":"39.02"}]}"# });
        e.handle(3.0, Event::WsMessage { socket, text: r#"{"arg":{"channel":"mark-price"},"data":[{"instId":"ETH-USDT-SWAP","markPx":"39"}]}"# });
        assert_eq!(e.store().tickers().len(), 1);
        assert_eq!(e.store().ticker("BTCUSDT").unwrap().index, 4.);
        assert!(e.store().ticker("ETHUSDT").is_none());
        assert_eq!(e.venue().price_tickers.get("ETH-USDT-SWAP"), Some(&None));
    }

    #[test]
    fn incomplete_or_failed_catalog_does_not_publish_staged_price_identity() {
        let mut e = engine();
        e.start(0.0);
        let r = request(&mut e);
        e.handle(
            1.0,
            Event::HttpStart {
                request: r.id,
                status: 200,
                headers: "",
            },
        );
        let chunk = format!(r#"{{"code":"50113","data":[{MARKET},"#);
        e.handle(
            2.0,
            Event::HttpChunk {
                request: r.id,
                data: chunk.as_bytes(),
            },
        );
        assert!(e.venue().price_tickers.is_empty());
        assert!(e.store().markets().is_empty());
        e.handle(
            3.0,
            Event::HttpChunk {
                request: r.id,
                data: b"{}]}",
            },
        );
        e.handle(4.0, Event::HttpEnd { request: r.id });
        assert!(e.venue().price_tickers.is_empty());
        assert!(e.store().markets().is_empty());
    }
}
