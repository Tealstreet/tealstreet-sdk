//! Bitunix USDT futures: sans-I/O public and account state machines.
mod account;
mod codes;
mod error;
mod history;
mod recovery;
#[cfg(test)]
mod secondary_tests;
mod sign;
mod sockets;
mod trading;
mod wire;
use safe_cex_engine::classify::{self, ReadFailure, ReadRetry, Released};
use safe_cex_engine::credential_selection::{SlotPolicy, Slots};
use safe_cex_engine::log::code;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::socket_lifecycle::FeedId;
use safe_cex_engine::startup::{DiscoveryRead, PublicRetries};
use safe_cex_engine::{
    Command, Cx, Method, Response, SocketChannel, SocketId, TimerId, Venue, json, num,
};
use safe_cex_engine::{ErrorKind, Level, VenueError};
use safe_cex_engine::{
    book::{Book, BookSide},
    model::*,
};
use std::collections::{HashMap, HashSet};
use wire::*;

const REST: &str = "https://fapi.bitunix.com";
const PUBLIC: &str = "wss://fapi.bitunix.com/public/";
const FUTURES: &str = "wss://api.bitunix.com/ws-futures/?from=contract&compress=2";
const PRIVATE: &str = "wss://fapi.bitunix.com/private/";
const MARKETS: &str = "/api/v1/futures/market/trading_pairs";
const TICKERS: &str = "/api/v1/futures/market/tickers";
const KLINE: &str = "/api/v1/futures/market/kline";

/// Opaque trusted-setup identities; signing material stays with the provider.
#[derive(Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub credentials: Option<Identity>,
    pub read_credentials: Option<Identity>,
}

pub enum Pending {
    Private(account::Read),
    Trade(trading::Request),
    History(history::Request),
    Markets {
        context: Context,
        rows: Vec<Market>,
        invalid: bool,
    },
    Tickers {
        context: Context,
        rows: Vec<Ticker>,
        invalid: bool,
    },
    Candles {
        context: Context,
    },
    SymbolSettings {
        symbol: String,
        write_revision: u64,
        serial: u64,
        custody: Option<account::ReadCustody>,
    },
}
impl Pending {
    pub(crate) fn with_custody(self, custody: account::ReadCustody) -> Self {
        match self {
            Self::Private(mut read) => {
                read.custody = Some(custody);
                Self::Private(read)
            }
            Self::SymbolSettings {
                symbol,
                write_revision,
                serial,
                ..
            } => Self::SymbolSettings {
                symbol,
                write_revision,
                serial,
                custody: Some(custody),
            },
            other => other,
        }
    }
    fn custody(&self) -> Option<&account::ReadCustody> {
        match self {
            Self::Private(read) => read.custody.as_ref(),
            Self::SymbolSettings { custody, .. } => custody.as_ref(),
            _ => None,
        }
    }
    /// Settle a Background read's authority; idempotent. Command-owned
    /// prerequisite reads settle through their Preflight or Plan instead.
    pub(crate) fn release(&mut self, cx: &mut Cx<'_, Pending>) {
        let custody = match self {
            Self::Private(read) => &mut read.custody,
            Self::SymbolSettings { custody, .. } => custody,
            _ => return,
        };
        if matches!(custody, Some(account::ReadCustody::Background(_)))
            && let Some(custody) = custody.take()
        {
            let _ = cx.finish_http_context(custody.context());
        }
    }
    /// Exact delivery precedes any decoding or Store effect.
    fn delivered(&self, cx: &Cx<'_, Pending>) -> bool {
        self.custody().is_none_or(|custody| custody.delivered(cx))
    }
}
#[derive(Clone)]
enum Feed {
    Private,
    Futures,
    Public {
        channel: &'static str,
        range: std::ops::Range<usize>,
    },
}
/// A registered managed feed; the engine owns its epochs and supervision.
struct Socket {
    feed: Feed,
    id: FeedId,
    /// The next connect's routing context: true after a timeout retirement.
    recover: bool,
}
#[derive(Default)]
struct BookState {
    levels: Book,
    initialized: bool,
}

pub struct BitunixFutures {
    options: Options,
    slots: Slots<Identity>,
    account: account::AccountState,
    trading: trading::TradingState,
    sockets: Vec<Socket>,
    books: HashMap<String, BookState>,
    candles: HashSet<(String, Interval)>,
    discovery: PublicRetries,
    funding_times: HashMap<String, f64>,
}
impl BitunixFutures {
    pub fn new(options: Options) -> Self {
        Self {
            slots: if options.public_data_only {
                Slots::default()
            } else {
                Slots::new(
                    options.credentials.clone(),
                    options.read_credentials.clone(),
                    SlotPolicy {
                        secondary_requires_primary: true,
                    },
                )
            },
            account: account::AccountState::default(),
            trading: trading::TradingState::default(),
            options,
            sockets: Vec::new(),
            books: HashMap::new(),
            candles: HashSet::new(),
            discovery: PublicRetries::default(),
            funding_times: HashMap::new(),
        }
    }
    fn read_policy() -> Policy {
        Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }
    }
    fn public_headers(&self) -> &'static [(&'static str, &'static str)] {
        if self.options.testnet {
            &[("x-simulated-trading", "1")]
        } else {
            &[]
        }
    }
    fn markets(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public_seed(cx, DiscoveryRead::Markets);
    }
    fn tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public_seed(cx, DiscoveryRead::Tickers);
    }
    fn public_seed(&mut self, cx: &mut Cx<'_, Pending>, read: DiscoveryRead) {
        let context = match cx.http_context(Scope::Background, Self::read_policy()) {
            Ok(context) => context,
            Err(error) => {
                self.public_failed(cx, read, error.venue_error(), 0.0);
                return;
            }
        };
        let (path, pending) = match read {
            DiscoveryRead::Markets => (
                MARKETS,
                Pending::Markets {
                    context: context.clone(),
                    rows: Vec::new(),
                    invalid: false,
                },
            ),
            DiscoveryRead::Tickers => (
                TICKERS,
                Pending::Tickers {
                    context: context.clone(),
                    rows: Vec::new(),
                    invalid: false,
                },
            ),
        };
        match cx.request_managed_with_headers(
            context,
            None,
            Method::Get,
            format!("{REST}{path}"),
            self.public_headers(),
            None,
            pending,
        ) {
            Ok(request) => cx.stream_response(request, json::ArrayAt::EnvelopeField("data")),
            Err((pending, error)) => self.on_http_failed(cx, pending, error.venue_error()),
        }
    }
    fn public_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: DiscoveryRead,
        error: VenueError,
        pause: f64,
    ) {
        cx.retried_failure("bitunix.lib:1", &error.message);
        self.discovery.failed_after(cx, read, pause, error);
    }
    fn public_response_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: &Response<'_>,
    ) {
        let failure = codes::read_failure(response, "Malformed Bitunix public seed");
        let floor = if failure.error.kind == Some(ErrorKind::RateLimited) {
            if matches!(response.status, 418 | 429) {
                safe_cex_engine::startup::rate_limit_pause_ms(response, 60_000.0)
            } else {
                60_000.0
            }
        } else {
            0.0
        };
        self.public_read_failed(cx, pending, failure, floor);
    }
    fn public_read_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        failure: ReadFailure,
        floor: f64,
    ) {
        let (context, read) = match pending {
            Pending::Markets { context, .. } => (context, DiscoveryRead::Markets),
            Pending::Tickers { context, .. } => (context, DiscoveryRead::Tickers),
            Pending::Candles { context } => {
                let _ = cx.fail_context(&context, failure.error);
                return;
            }
            other => {
                self.on_request_failed(cx, other, &failure.error.message);
                return;
            }
        };
        if cx.finish_http_context(&context).is_err() {
            return;
        }
        match failure.retry {
            Some(retry) => {
                let pause = match retry {
                    ReadRetry::After(ms) => floor.max(ms),
                    _ => floor,
                };
                self.public_failed(cx, read, failure.error, pause);
            }
            None => cx.retried_failure("bitunix.lib:1", &failure.error.message),
        }
    }
    fn public_typed_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: VenueError,
    ) {
        let (context, read) = match pending {
            Pending::Markets { context, .. } => (context, DiscoveryRead::Markets),
            Pending::Tickers { context, .. } => (context, DiscoveryRead::Tickers),
            Pending::Candles { context } => {
                let _ = cx.fail_context(&context, error);
                return;
            }
            other => {
                self.on_request_failed(cx, other, &error.message);
                return;
            }
        };
        if cx.finish_http_context(&context).is_ok() {
            self.public_failed(cx, read, error, 0.0);
        }
    }
    fn socket_spec(feed: &Feed, recover: bool) -> safe_cex_engine::SocketSpec {
        let (url, channel, identity) = match feed {
            Feed::Futures => (FUTURES, SocketChannel::Business, "futures".into()),
            Feed::Private => (PRIVATE, SocketChannel::Private, "private".into()),
            Feed::Public { channel, range } => (
                PUBLIC,
                SocketChannel::Public,
                format!("{channel}:{}", range.start),
            ),
        };
        safe_cex_engine::SocketSpec::new(url)
            .channel(channel)
            .feed(identity, recover)
    }
    fn futures_topic(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Option<String> {
        let m = cx.store.market(symbol)?;
        Some(format!(
            "market_{}_depthop_{}",
            symbol.to_ascii_lowercase(),
            num::format(m.precision.price)
        ))
    }
    fn subscriptions(&self, cx: &Cx<'_, Pending>) -> Vec<String> {
        self.books
            .keys()
            .filter_map(|s| self.futures_topic(cx, s))
            .chain(self.candles.iter().map(|(s, i)| {
                format!("market_{}_klineop_{}", s.to_ascii_lowercase(), interval(*i))
            }))
            .collect()
    }
    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, m: &Message<'_>) -> Result<(), String> {
        let fallback;
        let public = if let Some(data) = &m.public {
            Some(data)
        } else if let Some(raw) = m.data {
            fallback = PublicData::parse(&m.ch, raw.get())?;
            fallback.as_ref()
        } else {
            None
        };
        match public {
            Some(PublicData::Tick(d)) => {
                cx.store.update_ticker(&m.symbol, |t| {
                    t.last = d.la.0;
                    t.percentage = (d.la.0 - d.o.0) / d.o.0 * 100.0;
                    t.volume = d.b.0;
                    t.quote_volume = d.q.0;
                });
            }
            Some(PublicData::Price(d)) => {
                let funding = if d.nft.is_empty() { &d.ft } else { &d.nft };
                let funding_time = if let Some(v) = self.funding_times.get(funding.as_ref()) {
                    *v
                } else {
                    let v = time_ms(funding);
                    if self.funding_times.len() >= 64 {
                        self.funding_times.clear();
                    }
                    self.funding_times.insert(funding.to_string(), v);
                    v
                };
                cx.store.update_ticker(&m.symbol, |t| {
                    t.mark = d.mp.0;
                    t.index = d.ip.0;
                    t.funding_rate = d.fr.0;
                    t.funding_time = funding_time;
                });
            }
            Some(PublicData::Top(d)) => {
                cx.store.update_ticker(&m.symbol, |t| {
                    t.bid = d.b.first().map_or(0.0, |v| v.0);
                    t.ask = d.a.first().map_or(0.0, |v| v.0);
                });
            }
            _ => {}
        }
        let Some(raw) = m.data else {
            return Ok(());
        };
        let text = raw.get();
        if let Some(rest) = m.channel.strip_prefix("market_") {
            if let Some((symbol, _)) = rest.split_once("_depthop_") {
                let symbol = symbol.to_ascii_uppercase();
                if let Some(book) = self.books.get_mut(&symbol) {
                    let d: Depth = serde_json::from_str(text).map_err(|e| e.to_string())?;
                    // Legacy depthop frames are complete sorted snapshots, without sequence ids.
                    book.levels
                        .load_sorted(BookSide::Bids, d.bids.into_iter().map(|v| (v.0, v.1)));
                    book.levels
                        .load_sorted(BookSide::Asks, d.asks.into_iter().map(|v| (v.0, v.1)));
                    book.initialized = true;
                    cx.book_changed(&symbol);
                }
            } else if let Some((symbol, native)) = rest.split_once("_klineop_") {
                let symbol = symbol.to_ascii_uppercase();
                let intervals: Vec<_> = self
                    .candles
                    .iter()
                    .filter(|(s, i)| s == &symbol && interval(*i) == native)
                    .map(|(_, i)| *i)
                    .collect();
                if !intervals.is_empty() {
                    let d: Kline = serde_json::from_str(text).map_err(|e| e.to_string())?;
                    cx.store.update_ticker(&symbol, |t| t.last = d.close.0);
                    for i in intervals {
                        cx.candle(&symbol, i, candle(&d, false));
                    }
                }
            }
        }
        Ok(())
    }
}
fn topic_frame(topic: &str, subscribe: bool) -> String {
    format!(
        r#"{{"event":"{}","params":{{"channel":"{topic}"}}}}"#,
        if subscribe { "sub" } else { "unsub" }
    )
}
fn interval(i: Interval) -> &'static str {
    match i {
        Interval::M1 => "1min",
        Interval::M3 => "3min",
        Interval::M5 => "5min",
        Interval::M15 => "15min",
        Interval::M30 => "30min",
        Interval::H1 => "60min",
        Interval::D1 => "1day",
        Interval::W1 => "1week",
        _ => i.as_str(),
    }
}
fn interval_ms(i: Interval) -> Option<f64> {
    use Interval::*;
    matches!(
        i,
        M1 | M3 | M5 | M15 | M30 | H1 | H2 | H4 | H6 | H12 | D1 | W1
    )
    .then(|| i.ms())
}
fn candle(k: &Kline, rest: bool) -> Candle {
    Candle {
        timestamp: if rest { k.time.0 / 1000.0 } else { k.time.0 },
        open: k.open.0,
        high: k.high.0,
        low: k.low.0,
        close: k.close.0,
        volume: if rest { k.quote_vol.0 } else { k.volume.0 },
    }
}
impl Venue for BitunixFutures {
    fn guard_pending_placements(&self) -> bool {
        true
    }
    // reconcile_uncertain starts the regular and tpsl census reads at once.
    fn fence_uncertain_placements(&self) -> bool {
        true
    }
    /// One mutation per order for its whole job, continuations included.
    fn guard_order_mutations(&self) -> bool {
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
            Pending::Trade(request) => request.reconciliation(),
            _ => Err(safe_cex_engine::request_context::Error::Invalid),
        }
    }
    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        plan: &safe_cex_engine::request_context::Plan,
    ) {
        self.trading_plan_ready(cx, plan);
    }
    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.trading_retired(owner);
    }
    /// An uncertain seal: restore sealed jobs' rows, then Primary order
    /// snapshots report through `orders_reconciled`.
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _symbols: Option<&[String]>) {
        self.reconcile_trading(cx);
    }
    fn account_auth_outcome(
        &self,
        pending: &Pending,
        response: &Response<'_>,
        _now: f64,
        store: &safe_cex_engine::Store,
    ) -> Option<safe_cex_engine::AuthOutcome> {
        self.account_auth(pending, response, store)
    }
    type Pending = Pending;
    fn managed_sockets() -> bool {
        true
    }
    fn socket_capabilities() -> safe_cex_engine::socket_lifecycle::SocketCapabilities {
        safe_cex_engine::socket_lifecycle::SocketCapabilities { handoff: false }
    }
    fn socket_control(
        &self,
        _: &safe_cex_engine::Store,
        feed: FeedId,
        _: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        self.control(feed, text)
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.socket_event(cx, event)
    }
    fn encode_socket_interests(
        &self,
        _: &safe_cex_engine::Store,
        feed: FeedId,
        subscribe: bool,
        interests: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.encode_interests(feed, subscribe, interests, emit)
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            closed_positions: true,
            position_history: safe_cex_engine::PositionHistory::Bitunix,
            closed_position_format: safe_cex_engine::ClosedPositionFormat::Bitunix,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn on_venue_action(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: safe_cex_engine::CommandId,
        action: safe_cex_engine::VenueAction,
    ) {
        match action {
            safe_cex_engine::VenueAction::FetchClosedPositions {
                symbol,
                from_ms,
                to_ms,
            } => self.closed_positions(cx, command, symbol, from_ms, to_ms),
            _ => safe_cex_engine::unsupported_venue_action(cx, command),
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && read.validate().is_err()
        {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                safe_cex_engine::ReadCredentials::status(false).unwrap_err(),
            );
            return;
        }
        self.markets(cx);
    }
    /// Managed sockets deliver opens and closes as lifecycle events.
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(private) = self
            .socket_feed(cx, socket)
            .map(|feed| matches!(feed, Feed::Private))
        else {
            return;
        };
        match serde_json::from_str::<Message<'_>>(text) {
            Ok(m) => {
                if private {
                    self.private_message(cx, &m);
                    return;
                }
                if let Err(e) = self.public_message(cx, &m) {
                    cx.notice(Level::Warn, code::STREAM_MALFORMED, e);
                }
            }
            Err(e) => cx.notice(
                Level::Warn,
                code::STREAM_MALFORMED,
                format!("invalid Bitunix frame: {e}"),
            ),
        }
    }
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        if let Pending::Markets { context, .. } | Pending::Tickers { context, .. } = pending
            && cx.validate_http_context(context).is_err()
        {
            return;
        }
        if !pending.delivered(cx) {
            return;
        }
        match pending {
            Pending::Private(read) => self.account_item(cx, read, item),
            Pending::History(read) => self.history_item(cx, read, item),
            Pending::Markets { rows, invalid, .. } => {
                match serde_json::from_str::<Pair<'_>>(item) {
                    Ok(m) => {
                        if m.symbol_status == "OPEN"
                            && m.quote == "USDT"
                            && m.is_api_supported.as_ref().is_none_or(Supported::allowed)
                        {
                            rows.push(Market {
                                id: format!("{}/{}", m.base, m.quote),
                                symbol: m.symbol.into_owned(),
                                base: m.base.into_owned(),
                                quote: m.quote.into_owned(),
                                active: true,
                                precision: Precision {
                                    amount: 10f64.powi(m.base_precision.0 as i32).recip(),
                                    price: 10f64.powi(m.quote_precision.0 as i32).recip(),
                                },
                                limits: Limits {
                                    amount: MinMax {
                                        min: 0.0,
                                        max: m.max_limit_order_volume.0,
                                    },
                                    leverage: MinMax {
                                        min: 1.0,
                                        max: 100.0,
                                    },
                                    price: None,
                                },
                                pretty_symbol: None,
                                contract_size: None,
                                is_expiring: None,
                                is_inverse: None,
                                is_spot: None,
                                is_perpetual: None,
                                is_read_only: None,
                                taker_fee: None,
                                maker_fee: None,
                            });
                        }
                    }
                    Err(_) => {
                        *invalid = true;
                        safe_cex_engine::startup::skipped_row(cx);
                    }
                }
            }
            Pending::Tickers { rows, invalid, .. } => match serde_json::from_str::<Daily<'_>>(item)
            {
                Ok(t) => {
                    if let Some(m) = cx.store.market(&t.symbol) {
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            bid: t.last.0,
                            ask: t.last.0,
                            last: t.last.0,
                            mark: t.mark_price.0,
                            index: t.mark_price.0,
                            percentage: if t.open.0 == 0.0 {
                                0.0
                            } else {
                                (t.last.0 - t.open.0) / t.open.0 * 100.0
                            },
                            volume: t.base_vol.0,
                            quote_volume: t.quote_vol.0,
                            ..Ticker::default()
                        });
                    }
                }
                Err(_) => {
                    *invalid = true;
                    safe_cex_engine::startup::skipped_row(cx);
                }
            },
            _ => {}
        }
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        response: Response<'_>,
    ) {
        if pending.custody().is_some() {
            let delivered = pending.delivered(cx);
            pending.release(cx);
            if !delivered {
                return;
            }
        }
        let pending = match pending {
            Pending::History(read) => {
                self.history_response(cx, read, response);
                return;
            }
            Pending::Private(mut read) => {
                self.order_overlap(&mut read);
                let after = read.after;
                let success = (200..300).contains(&response.status)
                    && recovery::success(&response)
                    && !read.invalid
                    && !read.position_rejected.any()
                    && !read.partial_orders()
                    && (!matches!(read.axis, account::Axis::Orders | account::Axis::Tpsl)
                        || read.orders_current(cx.store))
                    && self.account.generations[read.axis.index()] == read.generation
                    && self.account.serials[match read.axis {
                        account::Axis::Balance => 0,
                        account::Axis::Positions => 1,
                        account::Axis::Orders => 2,
                        account::Axis::Tpsl => 3,
                    }] == read.serial;
                let custody = read.custody.take();
                self.account_response(cx, read, response);
                if let Some(command) = after {
                    self.prepared(cx, command, success, custody);
                }
                return;
            }
            Pending::Trade(request) => {
                self.trade_response(cx, request, response);
                return;
            }
            Pending::SymbolSettings {
                symbol,
                write_revision,
                serial,
                ..
            } => {
                self.symbol_settings_response(cx, &symbol, write_revision, serial, response);
                return;
            }
            other => other,
        };
        match &pending {
            Pending::Markets { context, .. } | Pending::Tickers { context, .. } => {
                if cx.validate_http_context(context).is_err() {
                    return;
                }
                if !codes::seed_success(&response) {
                    self.public_response_failed(cx, pending, &response);
                    return;
                }
            }
            Pending::Candles { context } => {
                if cx.validate_read_http_delivery(context, context).is_err() {
                    return;
                }
                if !(200..300).contains(&response.status) || data(response.body).is_err() {
                    let failure =
                        codes::read_failure(&response, "Malformed Bitunix candle response");
                    self.public_read_failed(cx, pending, failure, 0.0);
                    return;
                }
            }
            _ => {}
        }
        match pending {
            Pending::Markets {
                context, mut rows, ..
            } if !rows.is_empty() => {
                rows.shrink_to_fit();
                cx.store.set_markets(rows);
                if cx.finish_http_context(&context).is_err() {
                    return;
                }
                self.discovery.succeeded(cx, DiscoveryRead::Markets);
                self.connect_feed(cx, Feed::Futures);
                self.tickers(cx);
                self.private_start(cx);
            }
            Pending::Tickers {
                context, mut rows, ..
            } if !rows.is_empty() => {
                rows.shrink_to_fit();
                cx.store.set_tickers(rows);
                if cx.finish_http_context(&context).is_err() {
                    return;
                }
                self.discovery.succeeded(cx, DiscoveryRead::Tickers);
                let count = cx.store.markets().len();
                for start in (0..count).step_by(300) {
                    for channel in ["ticker", "price"] {
                        self.connect_feed(
                            cx,
                            Feed::Public {
                                channel,
                                range: start..(start + 300).min(count),
                            },
                        );
                    }
                }
            }
            Pending::Candles { context } => {
                let result = data(response.body).and_then(|text| {
                    let mut rows = Vec::new();
                    json::each::<Kline>(text, |k| rows.push(candle(&k, true)))
                        .map_err(|e| e.to_string())?;
                    serde_json::to_string(&rows).map_err(|e| e.to_string())
                });
                match result {
                    Ok(data) => {
                        let _ = cx.done_context(&context, data);
                    }
                    Err(message) => {
                        let _ = cx
                            .fail_context(&context, codes::read_failure(&response, &message).error);
                    }
                };
            }
            pending @ (Pending::Markets { .. } | Pending::Tickers { .. }) => {
                let invalid = match &pending {
                    Pending::Markets { invalid, .. } | Pending::Tickers { invalid, .. } => *invalid,
                    _ => false,
                };
                let message = if invalid {
                    "Malformed Bitunix public seed contains no usable rows"
                } else {
                    "Bitunix public seed contains no eligible rows"
                };
                let failure = codes::read_failure(&response, message);
                self.public_read_failed(cx, pending, failure, 0.0);
            }
            other => self.on_request_failed(cx, other, "malformed Bitunix list"),
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        response: Response<'_>,
    ) {
        pending.release(cx);
        let pending = match pending {
            Pending::History(read) => {
                self.history_stream_error(cx, read, &response);
                return;
            }
            Pending::Private(read) => {
                self.account_failed(cx, read, Some(&response));
                return;
            }
            Pending::SymbolSettings {
                symbol,
                write_revision,
                serial,
                ..
            } => {
                self.settings_failed(cx, &symbol, write_revision, serial, Some(&response));
                return;
            }
            pending => pending,
        };
        if matches!(pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
            if let Pending::Markets { context, .. } | Pending::Tickers { context, .. } = &pending
                && cx.validate_http_context(context).is_err()
            {
                return;
            }
            self.public_response_failed(cx, pending, &response);
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
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        status: u16,
        headers: &str,
        error: &str,
    ) {
        pending.release(cx);
        let response = Response {
            status,
            headers,
            body: "",
        };
        let pending = match pending {
            Pending::Private(read) => {
                self.account_failed(cx, read, Some(&response));
                return;
            }
            Pending::SymbolSettings {
                symbol,
                write_revision,
                serial,
                ..
            } => {
                self.settings_failed(cx, &symbol, write_revision, serial, Some(&response));
                return;
            }
            pending => pending,
        };
        if matches!(pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
            if let Pending::Markets { context, .. } | Pending::Tickers { context, .. } = &pending
                && cx.validate_http_context(context).is_err()
            {
                return;
            }
            self.public_response_failed(cx, pending, &response);
        } else {
            self.on_http_failed(
                cx,
                pending,
                VenueError::new(error).kind(ErrorKind::Internal),
            );
        }
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.public_typed_failed(cx, pending, error);
    }
    /// A provider refusal never released the request: a write is NotSent,
    /// and a read keeps its native failure path.
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::Trade(request) => self.trade_error(cx, request, error),
            Pending::History(read) => self.history_sign_failed(cx, read, &error.message),
            pending => self.public_typed_failed(cx, pending, error),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending, error: &str) {
        pending.release(cx);
        match pending {
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::History(read) => self.history_failed(cx, read, error),
            Pending::Private(read) => self.account_failed(cx, read, None),
            Pending::Candles { context } => {
                let _ = cx.fail_context(
                    &context,
                    classify::read(&codes::BITUNIX, Released::Transport, None, error).error,
                );
            }
            Pending::SymbolSettings {
                symbol,
                write_revision,
                serial,
                ..
            } => self.settings_failed(cx, &symbol, write_revision, serial, None),
            pending @ (Pending::Markets { .. } | Pending::Tickers { .. }) => {
                let failure = classify::read(&codes::BITUNIX, Released::Transport, None, error);
                self.public_read_failed(cx, pending, failure, 0.0);
            }
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account_timer(cx, timer) {
            return;
        }
        if let Some(read) = self.discovery.on_timer(timer) {
            match read {
                DiscoveryRead::Markets => self.markets(cx),
                DiscoveryRead::Tickers => self.tickers(cx),
            }
        }
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                if self.books.contains_key(&symbol) {
                    return;
                }
                self.books.insert(symbol, BookState::default());
                self.sync_futures(cx);
            }
            Command::UnsubscribeOrderBook { symbol } => {
                self.books.remove(&symbol);
                self.sync_futures(cx);
            }
            Command::SubscribeCandles {
                symbol,
                interval: i,
            } => {
                if interval_ms(i).is_none() {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        "Unsupported Bitunix candle interval",
                    );
                    return;
                }
                self.read_symbol_settings(cx, &symbol);
                if self.candles.insert((symbol, i)) {
                    self.sync_futures(cx);
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: i,
            } => {
                if interval_ms(i).is_none() {
                    return;
                }
                self.candles.remove(&(symbol, i));
                self.sync_futures(cx);
            }
            Command::FetchCandles {
                command: _,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => {
                let Ok(context) = cx.admit_http_command(Self::read_policy()) else {
                    return;
                };
                let Some(duration) = interval_ms(interval) else {
                    let _ = cx.fail_context(
                        &context,
                        VenueError::new("Unsupported Bitunix candle interval")
                            .kind(ErrorKind::Unsupported)
                            .outcome(safe_cex_engine::Outcome::NotSent),
                    );
                    return;
                };
                let explicit_limit = limit;
                let limit = limit.unwrap_or(200).clamp(1, 200);
                let end = to_ms.unwrap_or(cx.now);
                let derived = end - duration * f64::from(limit);
                let start = if explicit_limit.is_some() {
                    derived
                } else {
                    from_ms.unwrap_or(derived)
                };
                let limit_query = if explicit_limit.is_some() {
                    format!("&limit={limit}")
                } else {
                    String::new()
                };
                if let Err((pending, error)) = cx.request_managed_with_headers(context.clone(), None, Method::Get,
                    format!("{REST}{KLINE}?symbol={symbol}&interval={}&startTime={start}&endTime={end}{limit_query}", interval.as_str()),
                    self.public_headers(), None, Pending::Candles { context }) {
                    self.on_http_failed(cx, pending, error.venue_error());
                }
            }
            Command::Query { command, query } => self.query(cx, command, query),
            other => self.trading_command(cx, other),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|b| b.initialized)
            .map(|b| &b.levels)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, b)| b.initialized)
                .map(|(s, b)| (s.as_str(), &b.levels)),
        )
    }
}
