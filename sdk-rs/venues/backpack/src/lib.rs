//! Backpack perpetuals on the sans-I/O engine.
mod account;
mod depth_query;
mod history;
mod identity;
pub mod sign;
mod socket_codec;
mod trading;
mod wire;

use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Limits, Market, Min, MinMax, Precision, Ticker};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Command, CommandWait, Cx, Method, PublicFeedPolicy, Response, SocketId, TimerId, Venue,
};
use socket_codec::Connection;
use std::collections::HashMap;
use wire::*;

const REST: &str = "https://api.backpack.exchange";
const WS: &str = "wss://ws.backpack.exchange";
const MAX_BUFFER: usize = 2000;
const PUBLIC_READ_HEADERS: &[(&str, &str)] = &[("X-Broker-Id", "1200")];

#[derive(Default)]
pub struct Options {
    pub identity: Option<safe_cex_engine::signing::Identity>,
    pub read_identity: Option<safe_cex_engine::signing::Identity>,
    pub read_credentials_status: Option<Result<(), sign::ReadCredentialsError>>,
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub recv_window: Option<f64>,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
        if let Some(read) = self.read_credentials.as_mut() {
            read.erase_secrets();
        }
    }
}

pub enum Pending {
    SocketSnapshot(String),
    #[cfg(test)]
    SigningProbe,
    Identity(identity::Request),
    BookQuery {
        context: Context,
    },
    Account(Box<account::Request>),
    Trade(trading::Request),
    History(Box<history::Request>),
    Markets {
        context: Context,
        rows: Vec<Market>,
        skipped: Vec<String>,
    },
    Daily {
        context: Context,
        rows: Vec<Ticker>,
        skipped: Vec<String>,
    },
    Marks {
        context: Context,
        rows: Vec<(String, [f64; 4])>,
        skipped: Vec<String>,
    },
    Depth {
        context: Context,
        symbol: String,
        generation: u64,
    },
    Candles {
        context: Context,
        rows: Vec<Candle>,
        invalid: bool,
    },
}

impl Pending {
    fn seed(&self) -> Option<(SeedRead, &Context)> {
        match self {
            Self::Markets { context, .. } => Some((SeedRead::Markets, context)),
            Self::Daily { context, .. } => Some((SeedRead::Daily, context)),
            Self::Marks { context, .. } => Some((SeedRead::Marks, context)),
            _ => None,
        }
    }
}

struct Delta {
    first: u64,
    last: u64,
    bids: Vec<Level>,
    asks: Vec<Level>,
}

enum Sync {
    Waiting(Vec<Delta>),
    Live { last: u64, bridge: bool },
    Retry(TimerId, Vec<Delta>),
    Failed,
}

struct BookState {
    book: Book,
    sync: Sync,
    subscribers: u32,
    generation: u64,
    recoveries: u32,
}

#[derive(Default)]
struct Seed {
    daily: Option<Vec<Ticker>>,
    marks: MarkSeed,
    /// Tickers are committed once; later mark reads update them in place.
    seeded: bool,
    backoff: safe_cex_engine::startup::Backoff<SeedRead>,
    live: safe_cex_engine::startup::LiveTickers,
    requests: [Option<Context>; 3],
}

/// Startup waits for the first mark read; a failed one lets startup proceed
/// with last-price marks while the retry keeps running.
#[derive(Default)]
enum MarkSeed {
    #[default]
    Waiting,
    Unavailable,
    Loaded(Vec<(String, [f64; 4])>),
}

#[derive(Clone, Copy, PartialEq)]
enum SeedRead {
    Markets,
    Daily,
    Marks,
}

pub struct Backpack {
    options: safe_cex_crypto::secrets::Sensitive<Options>,
    public: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, Interval), u32>,
    seed: Seed,
    /// Perp markets whose catalogue row was skipped, and whether that was reported yet.
    skipped_markets: HashMap<String, bool>,
    unlisted_logged: std::collections::HashSet<String>,
    reread_stalls: u8,
    account: account::Account,
    trading: trading::Trading,
    generation: u64,
}

impl Backpack {
    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.public.started
            && cx.public_feed_policy() == PublicFeedPolicy::Direct
            && let Err(error) = self.connect_feed(cx, false)
        {
            Self::socket_notice(cx, error);
        }
    }
    fn seed_read(&mut self, cx: &mut Cx<'_, Pending>, kind: SeedRead) {
        if let Some(old) = self.seed.requests[kind as usize].take() {
            let _ = cx.finish_http_context(&old);
        }
        let context = match cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(context) => context,
            Err(reason) => {
                if reason != safe_cex_engine::request_context::Error::Invalid {
                    self.retry_seed(cx, kind, &reason.to_string());
                }
                return;
            }
        };
        self.seed.requests[kind as usize] = Some(context.clone());
        if !matches!(kind, SeedRead::Markets) && !self.seed.seeded {
            self.seed.live.begin();
        }
        let (path, pending) = match kind {
            SeedRead::Markets => (
                "/api/v1/markets",
                Pending::Markets {
                    context: context.clone(),
                    rows: vec![],
                    skipped: vec![],
                },
            ),
            SeedRead::Daily => (
                "/api/v1/tickers",
                Pending::Daily {
                    context: context.clone(),
                    rows: vec![],
                    skipped: vec![],
                },
            ),
            SeedRead::Marks => (
                "/api/v1/markPrices",
                Pending::Marks {
                    context: context.clone(),
                    rows: vec![],
                    skipped: vec![],
                },
            ),
        };
        match cx.request_managed_with_headers(
            context,
            None,
            Method::Get,
            format!("{REST}{path}"),
            PUBLIC_READ_HEADERS,
            None,
            pending,
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::Root),
            Err((pending, reason)) => {
                self.on_http_preparation_failed(cx, pending, reason, reason.read_venue_error())
            }
        }
    }

    fn finish_seed_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        kind: SeedRead,
        context: &Context,
    ) -> bool {
        let current = self.seed.requests[kind as usize].as_ref() == Some(context);
        if current {
            self.seed.requests[kind as usize] = None;
        }
        let _ = cx.finish_http_context(context);
        current
    }

    fn seed_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.on_request_failed(
                cx,
                pending,
                &format!("Backpack HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        if let Pending::Markets { skipped, .. }
        | Pending::Daily { skipped, .. }
        | Pending::Marks { skipped, .. } = &pending
            && !skipped.is_empty()
        {
            cx.notice(
                safe_cex_engine::Level::Warn,
                code::ROW_SKIPPED,
                format!(
                    "Skipped {} malformed Backpack {} rows: {}",
                    skipped.len(),
                    match pending {
                        Pending::Markets { .. } => "market",
                        Pending::Daily { .. } => "ticker",
                        _ => "mark price",
                    },
                    skipped.join(", ")
                ),
            );
        }
        match pending {
            Pending::Markets {
                context,
                rows,
                skipped,
            } => {
                if rows.is_empty() {
                    self.finish_seed_request(cx, SeedRead::Markets, &context);
                    self.seed.backoff.failed(cx, SeedRead::Markets);
                    return;
                }
                let refresh = cx.store.loaded().markets;
                let rows = if refresh {
                    keep_listed(cx.store.markets(), rows)
                } else {
                    rows
                };
                cx.store.set_markets(rows);
                let previous = std::mem::take(&mut self.skipped_markets);
                self.skipped_markets = skipped
                    .into_iter()
                    .filter(|s| s.ends_with("_PERP") && cx.store.market(s).is_none())
                    .map(|s| {
                        let reported = previous.get(&s).copied().unwrap_or(false);
                        (s, reported)
                    })
                    .collect();
                let after = self.skipped_markets.len();
                self.finish_seed_request(cx, SeedRead::Markets, &context);
                // Only named malformed perps make a catalogue partial and re-read it.
                if reread_partial(cx, &mut self.reread_stalls, refresh, previous.len(), after) {
                    self.seed.backoff.failed(cx, SeedRead::Markets);
                } else {
                    self.seed.backoff.succeeded(cx, &SeedRead::Markets);
                }
                if refresh {
                    return;
                }
                self.seed.live.begin();
                self.connect_public(cx);
                self.account_start(cx);
                if self.options.public_data_only
                    || self.options.key.as_ref().is_none_or(String::is_empty)
                {
                    cx.store.set_orders(vec![]);
                    cx.store.set_positions(vec![]);
                    cx.store.set_balance(Default::default());
                }

                self.seed_read(cx, SeedRead::Daily);
                self.seed_read(cx, SeedRead::Marks);
            }
            Pending::Daily {
                context,
                mut rows,
                skipped,
            } => {
                if rows.is_empty() {
                    self.finish_seed_request(cx, SeedRead::Daily, &context);
                    self.retry_seed(
                        cx,
                        SeedRead::Daily,
                        "Backpack ticker list has no usable rows",
                    );
                    return;
                }
                // A malformed row keeps an empty ticker so socket updates can fill it.
                for symbol in skipped {
                    if let Some(m) = cx.store.market(&symbol)
                        && !rows.iter().any(|t| t.symbol == symbol)
                    {
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            ..Default::default()
                        });
                    }
                }
                self.seed.daily = Some(rows);
                self.finish_seed(cx);
                self.finish_seed_request(cx, SeedRead::Daily, &context);
                self.seed.backoff.succeeded(cx, &SeedRead::Daily);
            }
            Pending::Marks { context, rows, .. } => {
                if rows.is_empty()
                    && !serde_json::from_str::<Vec<serde::de::IgnoredAny>>(response.body)
                        .is_ok_and(|rows| rows.is_empty())
                {
                    self.finish_seed_request(cx, SeedRead::Marks, &context);
                    self.retry_seed(cx, SeedRead::Marks, "Backpack mark list has no usable rows");
                    return;
                }
                if self.seed.seeded {
                    Self::apply_marks(cx, rows);
                } else {
                    self.seed.marks = MarkSeed::Loaded(rows);
                    self.finish_seed(cx);
                }
                self.finish_seed_request(cx, SeedRead::Marks, &context);
                self.seed.backoff.succeeded(cx, &SeedRead::Marks);
            }
            _ => unreachable!("seed response requires a seed pending"),
        }
    }

    fn retry_seed(&mut self, cx: &mut Cx<'_, Pending>, kind: SeedRead, error: &str) {
        cx.notice(safe_cex_engine::Level::Warn, code::REQUEST_FAILED, error);
        if kind == SeedRead::Marks && matches!(self.seed.marks, MarkSeed::Waiting) {
            self.seed.marks = MarkSeed::Unavailable;
            self.finish_seed(cx);
        }
        if cx.now.is_finite() {
            self.seed.backoff.failed(cx, kind);
        }
    }
    pub fn new(mut options: Options) -> Self {
        if options.read_credentials_status.is_none() {
            options.read_credentials_status = options
                .read_credentials
                .as_ref()
                .map(|read| sign::validate_read_credentials(read.validate(), &read.secret));
        }
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        Self {
            options,
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            seed: Seed::default(),
            skipped_markets: Default::default(),
            unlisted_logged: Default::default(),
            reread_stalls: 0,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            generation: 0,
        }
    }

    fn finish_seed(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.seed.seeded
            || self.seed.daily.is_none()
            || matches!(self.seed.marks, MarkSeed::Waiting)
        {
            return;
        }
        self.seed.seeded = true;
        let mut tickers = self.seed.daily.take().unwrap();
        self.seed.live.finish(cx.store, &mut tickers);
        cx.store.set_tickers(tickers);
        if let MarkSeed::Loaded(marks) = std::mem::take(&mut self.seed.marks) {
            Self::apply_marks(cx, marks);
        }
        self.relabel_unpriced_triggers(cx);
        self.connect_public(cx);
    }

    /// Account rows on a market skipped as malformed hold their snapshot; say so once
    /// per symbol. False means the market was filtered or never listed.
    pub(crate) fn report_skipped_market(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) -> bool {
        let Some(reported) = self.skipped_markets.get_mut(symbol) else {
            return false;
        };
        if !std::mem::replace(reported, true) {
            cx.notice(
                safe_cex_engine::Level::Error,
                code::ROW_SKIPPED,
                format!("Backpack account data for {symbol} is held: its market row was malformed"),
            );
        }
        true
    }

    /// Account rows on a filtered or delisted market are skipped; say so once per symbol.
    pub(crate) fn report_unlisted_market(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if self.unlisted_logged.insert(symbol.into()) {
            cx.notice(
                safe_cex_engine::Level::Warn,
                code::ROW_SKIPPED,
                format!("Backpack {symbol} is not a listed market; its account rows are skipped"),
            );
        }
    }

    fn apply_marks(cx: &mut Cx<'_, Pending>, marks: Vec<(String, [f64; 4])>) {
        for (symbol, m) in marks {
            cx.store.update_ticker(&symbol, |t| {
                t.mark = m[0];
                t.index = m[1];
                t.funding_rate = m[2];
                t.funding_time = m[3];
            });
        }
    }

    fn snapshot(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        self.generation += 1;
        state.generation = self.generation;
        let prior = std::mem::replace(&mut state.sync, Sync::Failed);
        let buffer = match prior {
            Sync::Retry(_, buffer) | Sync::Waiting(buffer) => buffer,
            _ => vec![],
        };
        state.sync = Sync::Waiting(buffer);
        let context = match cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(context) => context,
            Err(reason) => {
                if reason != safe_cex_engine::request_context::Error::Invalid {
                    self.recover_book(cx, symbol);
                }
                return;
            }
        };
        let params = [("symbol".into(), symbol.into())].into();
        let url = format!("{REST}/api/v1/depth?{}", sign::query(&params));
        if let Err((pending, reason)) = cx.request_managed_with_headers(
            context.clone(),
            None,
            Method::Get,
            url,
            PUBLIC_READ_HEADERS,
            None,
            Pending::Depth {
                context,
                symbol: symbol.into(),
                generation: self.generation,
            },
        ) {
            safe_cex_engine::Venue::on_http_preparation_failed(
                self,
                cx,
                pending,
                reason,
                reason.read_venue_error(),
            );
        }
    }

    fn apply_depth_seed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        generation: u64,
        response: Response<'_>,
    ) {
        let Some(state) = self.books.get_mut(&symbol) else {
            return;
        };
        if state.generation != generation || !matches!(state.sync, Sync::Waiting(_)) {
            return;
        }
        let Ok(depth) = serde_json::from_str::<Depth>(response.body) else {
            self.recover_book(cx, &symbol);
            return;
        };
        let Sync::Waiting(buffer) = std::mem::replace(
            &mut state.sync,
            Sync::Live {
                last: depth.last_update_id,
                bridge: true,
            },
        ) else {
            unreachable!()
        };
        state.book.load_sorted_owned(BookSide::Bids, depth.bids);
        state.book.load_sorted_owned(BookSide::Asks, depth.asks);
        let mut iter = buffer.into_iter();
        let mut gap = None;
        while let Some(d) = iter.next() {
            let valid = match state.sync {
                Sync::Live { last, bridge } => {
                    d.last <= last
                        || last.checked_add(1).is_some_and(|expected| {
                            d.first <= expected
                                && (bridge || d.first == expected)
                                && d.first <= d.last
                        })
                }
                _ => false,
            };
            if valid {
                Self::apply(state, d);
            } else {
                gap = Some(std::iter::once(d).chain(iter).collect());
                break;
            }
        }
        if let Some(buffer) = gap {
            state.sync = Sync::Waiting(buffer);
            self.recover_book(cx, &symbol);
        } else {
            state.recoveries = 0;
            cx.book_changed(&symbol);
        }
    }

    fn recover_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        if matches!(state.sync, Sync::Failed) {
            return;
        }
        state.recoveries += 1;
        if state.recoveries > 3 {
            state.sync = Sync::Failed;
            state.book.clear();
            cx.book_changed(symbol);
            cx.notice(
                safe_cex_engine::Level::Error,
                code::STREAM_UNAVAILABLE,
                format!("Backpack depth recovery exhausted for {symbol}"),
            );
            return;
        }
        let prior = std::mem::replace(&mut state.sync, Sync::Failed);
        let buffer = match prior {
            Sync::Waiting(buffer) | Sync::Retry(_, buffer) => buffer,
            _ => vec![],
        };
        state.sync = Sync::Retry(cx.set_timer(1000.0), buffer);
    }

    fn apply(state: &mut BookState, delta: Delta) -> bool {
        let Sync::Live { last, bridge } = &mut state.sync else {
            return false;
        };
        if delta.last <= *last {
            return true;
        }
        let Some(expected) = last.checked_add(1) else {
            return false;
        };
        if delta.first > expected
            || (!*bridge && delta.first != expected)
            || delta.first > delta.last
        {
            return false;
        }
        for Level(p, q) in delta.bids {
            state.book.apply(BookSide::Bids, p, q);
        }
        for Level(p, q) in delta.asks {
            state.book.apply(BookSide::Asks, p, q);
        }
        *last = delta.last;
        *bridge = false;
        true
    }

    fn depth(&mut self, cx: &mut Cx<'_, Pending>, event: DepthEvent<'_>) {
        let Some(state) = self.books.get_mut(event.s.as_ref()) else {
            return;
        };
        let delta = Delta {
            first: event.first,
            last: event.last,
            bids: event.b,
            asks: event.a,
        };
        match &mut state.sync {
            Sync::Waiting(buffer) | Sync::Retry(_, buffer) if buffer.len() < MAX_BUFFER => {
                buffer.push(delta);
            }
            Sync::Waiting(_) | Sync::Retry(_, _) => {
                state.sync = Sync::Failed;
                state.book.clear();
                cx.book_changed(&event.s);
                cx.notice(
                    safe_cex_engine::Level::Error,
                    code::STREAM_UNAVAILABLE,
                    format!("Backpack depth buffer overflow for {}", event.s),
                );
            }
            Sync::Live { .. } => {
                if delta.last
                    <= match state.sync {
                        Sync::Live { last, .. } => last,
                        _ => unreachable!(),
                    }
                {
                    return;
                }
                let valid = match state.sync {
                    Sync::Live { last, bridge } => last.checked_add(1).is_some_and(|expected| {
                        delta.first <= expected
                            && (bridge || delta.first == expected)
                            && delta.first <= delta.last
                    }),
                    _ => false,
                };
                if valid {
                    Self::apply(state, delta);
                    cx.book_changed(&event.s);
                } else {
                    state.sync = Sync::Waiting(vec![delta]);
                    self.recover_book(cx, &event.s);
                }
            }
            Sync::Failed => {}
        }
    }

    fn public_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        stream: &str,
        text: &str,
    ) -> Result<(), String> {
        let topic = stream.split('.').next().unwrap_or("");
        match topic {
            "depth" => self.depth(cx, serde_json::from_str(text).map_err(|e| e.to_string())?),
            "ticker" => {
                let d: TickerEvent<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
                let mut mask = (1 << 2) | (1 << 9) | (1 << 10);
                cx.store.update_ticker(&d.s, |t| {
                    t.last = d.c;
                    if let Some(change) = d
                        .percentage
                        .map(|n| n.0)
                        .or_else(|| d.o.filter(|n| n.0 > 0.0).map(|n| (d.c - n.0) / n.0))
                    {
                        t.percentage = change * 100.0;
                        mask |= 1 << 5;
                    }
                    t.volume = d.v;
                    t.quote_volume = d.q;
                });
                self.seed.live.record(cx.store, &d.s, mask);
            }
            "trade" => {
                let d: TradeEvent<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
                cx.store.update_ticker(&d.s, |t| t.last = d.p);
                self.seed.live.record(cx.store, &d.s, 1 << 2);
            }
            "bookTicker" => {
                let d: BookEvent<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
                cx.store.update_ticker(&d.s, |t| {
                    t.bid = d.b;
                    t.ask = d.a;
                });
                self.seed.live.record(cx.store, &d.s, 3);
            }
            "kline" => {
                let d: KlineEvent<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
                if let (Some(o), Some(h), Some(l), Some(c), Some(v)) = (d.o, d.h, d.l, d.c, d.v)
                    && let Some(interval) = stream.split('.').nth(1).and_then(Interval::parse)
                    && self.candles.contains_key(&(d.s.to_string(), interval))
                {
                    cx.candle(
                        &d.s,
                        interval,
                        Candle {
                            timestamp: timestamp(&d.t)?,
                            open: finite(&o)?,
                            high: finite(&h)?,
                            low: finite(&l)?,
                            close: finite(&c)?,
                            volume: finite(&v)?,
                        },
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl Venue for Backpack {
    fn cancel_needs_stored_order(&self) -> bool {
        true
    }

    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            order_book_query: true,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(Err(error)) = self.options.read_credentials_status {
            cx.notice(
                safe_cex_engine::Level::Error,
                code::CONFIG_INVALID,
                error.message(),
            );
            return;
        }
        if let Err(error) = self.register_sockets(cx) {
            Self::socket_notice(cx, error);
            return;
        }
        self.seed_read(cx, SeedRead::Markets);
    }
    fn on_open(&mut self, _cx: &mut Cx<'_, Pending>, _socket: SocketId) {}
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.managed_socket_event(cx, event)
    }
    fn on_socket_interests_released(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        epoch: safe_cex_engine::socket_lifecycle::Epoch,
        pending: Pending,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.released_socket_snapshots(cx, epoch, pending)
    }
    fn encode_socket_interests(
        &self,
        _store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        items: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.encode_public_interests(feed, subscribe, items, emit)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.socket.socket == Some(socket) {
            if let Err(e) = self.private_message(cx, text) {
                cx.notice(safe_cex_engine::Level::Error, code::STREAM_UNAVAILABLE, e);
            }
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        let result = (|| {
            let envelope: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
            if let Some(error) = envelope.error {
                if error.code == 4002 {
                    cx.store.set_latency((cx.now - self.public.ping_at) / 2.0);
                    return Ok(());
                }
                return Err(format!("Backpack socket {}: {}", error.code, error.message));
            }
            if let Some(data) = envelope.data {
                self.public_message(cx, &envelope.stream, data.get())?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            cx.notice(safe_cex_engine::Level::Error, code::STREAM_MALFORMED, e);
        }
    }
    fn on_close(&mut self, _cx: &mut Cx<'_, Pending>, _socket: SocketId, _code: u16) {}
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        if let Some((kind, context)) = pending.seed()
            && (self.seed.requests[kind as usize].as_ref() != Some(context)
                || cx.validate_background_read_http_delivery(context).is_err())
        {
            return;
        }
        if let Pending::Candles { context, .. } = &*pending
            && cx.validate_read_http_delivery(context, context).is_err()
        {
            return;
        }
        let result = match pending {
            Pending::SocketSnapshot(_) | Pending::Identity(_) => return,
            Pending::History(request) => {
                self.history_item(cx, request, item);
                return;
            }
            Pending::Account(request) => {
                self.account_item(cx, request, item);
                return;
            }
            Pending::Markets { rows, skipped, .. } => {
                let r = serde_json::from_str::<MarketRow<'_>>(item).map(|r| {
                    if r.visible == Some(false)
                        || r.market_type != "PERP"
                        || !r.symbol.contains('_')
                    {
                        return true;
                    }
                    let mut split = r.symbol.split('_');
                    let base = split.next().unwrap_or("").to_owned();
                    let quote = split.next().unwrap_or("").to_owned();
                    let max = r.imf_function.map_or(1.0, |f| {
                        let n = safe_cex_engine::num::parse(&f.base);
                        if n > 0.0 { (1.0 / n).round() } else { 1.0 }
                    });
                    let market = Market {
                        id: r.symbol.to_string(),
                        symbol: r.symbol.into_owned(),
                        pretty_symbol: None,
                        base,
                        quote,
                        active: true,
                        precision: Precision {
                            amount: r
                                .quantity_increment
                                .or(r.filters.quantity.step_size)
                                .map_or(0.001, |n| n.0),
                            price: r
                                .price_increment
                                .or(r.filters.price.tick_size)
                                .map_or(0.01, |n| n.0),
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: r
                                    .min_quantity
                                    .or(r.filters.quantity.min_quantity)
                                    .map_or(0.0, |n| n.0),
                                max: r
                                    .max_quantity
                                    .or(r.filters.quantity.max_quantity)
                                    .map_or(0.0, |n| n.0),
                            },
                            leverage: MinMax { min: 1.0, max },
                            price: Some(Min {
                                min: r.filters.price.min_price.map_or(0.0, |n| n.0),
                            }),
                        },
                        contract_size: Some(1.0),
                        is_expiring: Some(false),
                        is_inverse: Some(false),
                        is_spot: Some(false),
                        is_perpetual: Some(true),
                        is_read_only: None,
                        taker_fee: None,
                        maker_fee: None,
                    };
                    let tradable = market.precision.is_tradable();
                    if tradable {
                        rows.push(market);
                    }
                    tradable
                });
                if !r.unwrap_or(false) {
                    safe_cex_engine::startup::skipped_row(cx);
                    skipped.push(row_symbol(item));
                }
                return;
            }
            Pending::Daily { rows, skipped, .. } => {
                let r = serde_json::from_str::<Daily<'_>>(item).map(|d| {
                    if cx.store.market(&d.symbol).is_some() {
                        rows.push(Ticker {
                            id: d.symbol.to_string(),
                            symbol: d.symbol.into_owned(),
                            last: d.last_price,
                            mark: d.last_price,
                            index: d.last_price,
                            percentage: d.price_change_percent * 100.0,
                            volume: d.volume,
                            quote_volume: d.quote_volume,
                            ..Default::default()
                        });
                    }
                });
                if r.is_err() {
                    safe_cex_engine::startup::skipped_row(cx);
                    skipped.push(row_symbol(item));
                }
                return;
            }
            Pending::Marks { rows, skipped, .. } => {
                let r = serde_json::from_str::<Mark<'_>>(item).map(|d| {
                    rows.push((
                        d.symbol.into_owned(),
                        [
                            d.mark_price,
                            d.index_price,
                            d.funding_rate,
                            d.next_funding_timestamp,
                        ],
                    ))
                });
                if r.is_err() {
                    safe_cex_engine::startup::skipped_row(cx);
                    skipped.push(row_symbol(item));
                }
                return;
            }
            Pending::Candles { rows, invalid, .. } => {
                let r = serde_json::from_str::<Kline<'_>>(item)
                    .and_then(|d| candle(d).map_err(serde::de::Error::custom))
                    .map(|c| rows.push(c));
                if r.is_err() {
                    *invalid = true;
                }
                r
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            cx.notice(
                safe_cex_engine::Level::Error,
                code::STREAM_MALFORMED,
                format!("Backpack decode: {e}"),
            );
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Depth {
            context,
            symbol,
            generation,
        } = pending
        {
            if cx.validate_background_read_http_delivery(&context).is_err() {
                let _ = cx.finish_http_context(&context);
                return;
            }
            if !(200..300).contains(&response.status) {
                self.on_request_failed(
                    cx,
                    Pending::Depth {
                        context,
                        symbol,
                        generation,
                    },
                    &format!("Backpack HTTP {}: {}", response.status, response.body),
                );
                return;
            }
            self.apply_depth_seed(cx, symbol, generation, response);
            let _ = cx.finish_http_context(&context);
            return;
        }
        if let Pending::Candles { context, .. } = &pending
            && cx.validate_read_http_delivery(context, context).is_err()
        {
            return;
        }
        if let Pending::BookQuery { context } = pending {
            self.book_query_response(cx, &context, response);
            return;
        }
        if let Pending::Identity(request) = pending {
            self.identity_response(cx, request, response);
            return;
        }
        if let Pending::History(request) = pending {
            self.history_response(cx, *request, response);
            return;
        }
        if let Pending::Trade(request) = pending {
            self.trade_response(cx, request, response);
            return;
        }
        if let Pending::Account(request) = pending {
            self.account_response(cx, request, response);
            return;
        }
        if let Some((kind, context)) = pending
            .seed()
            .map(|(kind, context)| (kind, context.clone()))
        {
            if self.seed.requests[kind as usize].as_ref() == Some(&context)
                && cx.validate_background_read_http_delivery(&context).is_ok()
            {
                self.seed_response(cx, pending, response);
            } else {
                self.finish_seed_request(cx, kind, &context);
            }
            return;
        }
        if !(200..300).contains(&response.status) {
            self.on_request_failed(
                cx,
                pending,
                &format!("Backpack HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        match pending {
            Pending::Candles {
                context,
                rows,
                invalid: false,
            } => {
                let _ = cx.done_context(&context, serde_json::to_string(&rows).unwrap());
            }
            Pending::Candles { context, .. } => {
                let _ = cx.fail_context(&context, "Backpack invalid candles");
            }
            _ => cx.notice(
                safe_cex_engine::Level::Error,
                code::STREAM_MALFORMED,
                "Backpack invalid REST list",
            ),
        }
    }
    fn unproven_http_evidence(
        &self,
        pending: &Pending,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        if let Pending::Trade(request) = pending {
            request.unproven_evidence(response, evidence);
        }
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Account(request) => self.account_http_failed(cx, request, error),
            Pending::History(request) => {
                // Native history presents a released transport failure as Internal.
                let error = if error.kind == Some(safe_cex_engine::ErrorKind::Network) {
                    error.kind(safe_cex_engine::ErrorKind::Internal)
                } else {
                    error
                };
                self.history_failed(cx, *request, error);
            }
            Pending::Identity(request) => self.identity_failed(cx, request, error),
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::Candles { context, .. } => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::BookQuery { context } => {
                // Preserve the public query's existing error presentation from #1680.
                let _ = cx.fail_context(&context, error.outcome(safe_cex_engine::Outcome::NotSent));
            }
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: safe_cex_engine::request_context::Error,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Account(request) => self.account_precommit_failed(cx, request, reason, error),
            Pending::History(request) => self.history_failed(cx, *request, error),
            Pending::Identity(request) => self.identity_failed(cx, request, error),
            Pending::Candles { context, .. } => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::BookQuery { context } => {
                let _ = cx.fail_context(&context, error.outcome(safe_cex_engine::Outcome::NotSent));
            }
            pending => self.on_sign_error(cx, pending, error),
        }
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Account(request) => self.account_precommit_failed(
                cx,
                request,
                safe_cex_engine::request_context::Error::Invalid,
                error,
            ),
            Pending::History(request) => self.history_failed(cx, *request, error),
            Pending::Identity(request) => self.identity_failed(cx, request, error),
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }
    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        plan: &safe_cex_engine::request_context::Plan,
    ) {
        self.trade_plan_ready(cx, plan);
    }
    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.trade_retired(owner);
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
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Pending::Identity(request) = pending {
            self.identity_failed(cx, request, safe_cex_engine::VenueError::new(error));
            return;
        }
        match pending {
            Pending::BookQuery { context } => {
                let _ = cx.fail_context(
                    &context,
                    depth_query::failure(error, safe_cex_engine::ErrorKind::Network),
                );
            }
            Pending::Account(request) => self.account_failed(cx, request, error),
            Pending::Trade(request) => {
                let error = safe_cex_engine::VenueError::new(error)
                    .kind(safe_cex_engine::ErrorKind::Network)
                    .outcome(safe_cex_engine::Outcome::Unknown);
                self.trade_failed(cx, request, error)
            }
            Pending::History(request) => {
                self.history_failed(cx, *request, safe_cex_engine::VenueError::new(error))
            }
            Pending::Depth {
                context,
                symbol,
                generation,
            } => {
                let _ = cx.finish_http_context(&context);
                if self
                    .books
                    .get(&symbol)
                    .is_some_and(|s| s.generation == generation)
                {
                    self.recover_book(cx, &symbol);
                }
            }
            Pending::Candles { context, .. } => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::Markets { context, .. } => {
                if self.finish_seed_request(cx, SeedRead::Markets, &context) {
                    self.retry_seed(cx, SeedRead::Markets, error);
                }
            }
            Pending::Daily { context, .. } => {
                if self.finish_seed_request(cx, SeedRead::Daily, &context) {
                    self.retry_seed(cx, SeedRead::Daily, error);
                }
            }
            Pending::Marks { context, .. } => {
                if self.finish_seed_request(cx, SeedRead::Marks, &context) {
                    self.retry_seed(cx, SeedRead::Marks, error);
                }
            }
            _ => cx.retried_failure("backpack.lib:990", error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.socket_interest_timer(cx, timer) {
            return;
        }
        if self.account_timer(cx, timer) {
            return;
        }
        if let Some(kind) = self.seed.backoff.on_timer(timer) {
            self.seed_read(cx, kind);
            return;
        }
        if let Some(symbol) = self
            .books
            .iter()
            .find_map(|(s, b)| matches!(b.sync,Sync::Retry(t, _) if t==timer).then(|| s.clone()))
        {
            self.request_book_snapshot(cx, symbol);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::FetchAccount { command } = command {
            self.fetch_identity(cx, command);
            return;
        }
        match command {
            Command::SubscribeOrderBook { symbol } => self.subscribe_book(cx, symbol),
            Command::UnsubscribeOrderBook { symbol } => self.unsubscribe_book(cx, symbol),
            Command::SubscribeCandles { symbol, interval } => {
                if !supported(interval) {
                    cx.notice(
                        safe_cex_engine::Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("Backpack"),
                    );
                    return;
                }
                self.subscribe_candles(cx, symbol, interval);
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                self.unsubscribe_candles(cx, symbol, interval);
            }
            Command::FetchCandles {
                symbol,
                interval,
                from_ms,
                to_ms,
                ..
            } => {
                if !supported(interval) {
                    match cx.command_wait() {
                        Some(CommandWait::Remaining(_)) => {}
                        Some(CommandWait::Expired) => {
                            let _ = cx.reject_expired_command(Operation::Read);
                            return;
                        }
                        None => {
                            let _ = cx.reject_unstamped_command(Operation::Read);
                            return;
                        }
                    }
                    let Ok(context) = cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    }) else {
                        return;
                    };
                    let _ = cx.fail_context(&context, interval.unsupported("Backpack"));
                    return;
                }
                let mut params: sign::Params = [
                    ("symbol".into(), symbol),
                    ("interval".into(), interval.to_string()),
                ]
                .into();
                if let Some(from) = from_ms {
                    params.insert(
                        "startTime".into(),
                        format!("{:.0}", (from / 1000.0).floor()),
                    );
                }
                if let Some(to) = to_ms {
                    params.insert("endTime".into(), format!("{:.0}", (to / 1000.0).floor()));
                }
                let Ok(context) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return;
                };
                let url = format!("{REST}/api/v1/klines?{}", sign::query(&params));
                match cx.request_managed_with_headers(
                    context.clone(),
                    None,
                    Method::Get,
                    url,
                    PUBLIC_READ_HEADERS,
                    None,
                    Pending::Candles {
                        context,
                        rows: vec![],
                        invalid: false,
                    },
                ) {
                    Ok(request) => cx.stream_response(request, ArrayAt::Root),
                    Err((pending, reason)) => {
                        self.on_http_preparation_failed(cx, pending, reason, reason.venue_error())
                    }
                }
            }
            Command::FetchOrderBook { symbol, .. } => {
                let Ok(context) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return;
                };
                let params = [("symbol".into(), symbol)].into();
                let url = format!("{REST}/api/v1/depth?{}", sign::query(&params));
                if let Err((pending, error)) = cx.request_managed_with_headers(
                    context.clone(),
                    None,
                    Method::Get,
                    url,
                    PUBLIC_READ_HEADERS,
                    None,
                    Pending::BookQuery { context },
                ) {
                    self.on_http_preparation_failed(cx, pending, error, error.venue_error());
                }
            }
            Command::Query { command, query } => self.history_command(cx, command, query),
            command => self.trade_command(cx, command),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|s| matches!(s.sync, Sync::Live { .. } | Sync::Failed))
            .map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

/// A refresh keeps every listed market the new catalogue omits or filters out, unchanged.
fn keep_listed(
    listed: &[safe_cex_engine::model::Market],
    rows: Vec<safe_cex_engine::model::Market>,
) -> Vec<safe_cex_engine::model::Market> {
    let mut fresh: Vec<_> = rows.into_iter().map(Some).collect();
    let mut index = std::collections::HashMap::new();
    for (i, market) in fresh.iter().enumerate() {
        if let Some(market) = market {
            index.entry(market.symbol.clone()).or_insert(i);
        }
    }
    let mut merged: Vec<_> = listed
        .iter()
        .map(|old| {
            index
                .get(&old.symbol)
                .and_then(|&i| fresh[i].take())
                .unwrap_or_else(|| old.clone())
        })
        .collect();
    merged.extend(fresh.into_iter().flatten());
    merged
}

/// Whether a partial catalogue re-reads; five refreshes in a row that leave its
/// malformed set no smaller end the retry.
fn reread_partial<P>(
    cx: &mut safe_cex_engine::Cx<'_, P>,
    stalls: &mut u8,
    refresh: bool,
    before: usize,
    after: usize,
) -> bool {
    if after == 0 {
        *stalls = 0;
        return false;
    }
    *stalls = if refresh && after >= before {
        stalls.saturating_add(1)
    } else {
        0
    };
    if *stalls == 5 {
        cx.notice(
            safe_cex_engine::Level::Warn,
            code::REQUEST_FAILED,
            format!("Backpack stops re-reading a catalogue with {after} malformed markets"),
        );
    }
    *stalls < 5
}

#[cfg(test)]
mod secondary_tests;

// Backpack kline codes are the TS spellings for these intervals.
fn supported(interval: Interval) -> bool {
    use Interval::*;
    matches!(
        interval,
        M1 | M3 | M5 | M15 | M30 | H1 | H2 | H4 | H6 | H8 | H12 | D1 | D3 | W1
    )
}
