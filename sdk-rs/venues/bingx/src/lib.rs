//! BingX stablecoin-margined perpetual swaps on the sans-I/O engine.
pub mod account;
pub mod account_wire;
mod history;
mod identity;
mod managed;
pub mod paths;
pub mod sign;
mod socket_codec;
mod trading;
mod wire;

use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::credential_selection::{SlotPolicy, Slots};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::selected_read::ReadRole;
use safe_cex_engine::{
    Command, CommandId, Cx, Method, PublicFeedPolicy, Response, SocketId, TimerId, Venue, num,
};
use serde::Deserialize;
use sign::{Credentials, Params};
use socket_codec::Connection;
use std::collections::HashMap;
use wire::*;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::signing::Identity>,
    pub public_data_only: bool,
    pub credentials: Option<safe_cex_engine::signing::Identity>,
}

pub enum Pending {
    Identity(safe_cex_engine::selected_read::AdmittedRead),
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    Markets {
        rows: Vec<Market>,
        skipped: Vec<String>,
    },
    Daily {
        rows: Vec<Ticker>,
        skipped: Vec<String>,
    },
    Premium {
        skipped: Vec<String>,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        valid: bool,
    },
    Signed(Box<Signed>),
    /// One FetchLeverage read under its engine-admitted metadata Plan.
    Metadata(Box<account::MetadataLeg>),
}

impl Pending {
    pub(crate) fn into_trade(self) -> Option<trading::Request> {
        match self {
            Self::Trade(request) => Some(request),
            _ => None,
        }
    }
}

/// A signed request kept re-signable, so a signature rejection is resent once.
pub struct Signed {
    inner: Pending,
    method: Method,
    path: &'static str,
    params: Params,
    at: Option<ArrayAt>,
    resent: bool,
    custody: Option<managed::Custody>,
}
impl Signed {
    fn context(&self) -> Option<&safe_cex_engine::request_context::Context> {
        self.custody.as_ref().map(managed::Custody::context)
    }
}

pub(crate) fn signature_rejected(body: &str) -> bool {
    serde_json::from_str::<Rejection<'_>>(body).is_ok_and(|r| {
        r.code == Some(100_001)
            || r.msg
                .to_ascii_lowercase()
                .contains("signature verification failed")
    })
}

/// Logs a read's skipped rows once, by identity only.
pub(crate) fn log_skipped(cx: &mut Cx<'_, Pending>, kind: &str, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    let shown = ids
        .iter()
        .take(10)
        .map(|id| if id.is_empty() { "unidentified" } else { id })
        .collect::<Vec<_>>();
    let more = ids.len().saturating_sub(shown.len());
    cx.notice(
        Level::Warn,
        code::ROW_SKIPPED,
        format!(
            "BingX skipped {} malformed {kind} row(s): {}{}",
            ids.len(),
            shown.join(", "),
            if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            }
        ),
    );
}

#[derive(Clone, Copy, PartialEq)]
enum List {
    Markets,
    Daily,
    Premium,
}

#[derive(Default)]
struct BookState {
    subscribers: u32,
    book: Book,
    ready: bool,
}

pub struct Bingx {
    config_error: Option<&'static str>,
    account: account::Account,
    trading: trading::Trading,
    history: history::History,
    credentials: Option<Credentials>,
    public: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, Interval), u32>,
    seed: Vec<Ticker>,
    seed_index: Vec<Option<usize>>,
    seed_rows_index: Vec<Option<usize>>,
    live_tickers: safe_cex_engine::startup::LiveTickers,
    seed_reported: Vec<bool>,
    sequence: std::cell::Cell<u64>,
    retries: safe_cex_engine::startup::Backoff<List>,
    reread_stalls: u8,
    // A failed or empty funding read commits tickers without it and retries.
    tickers_committed: bool,
    // Markets the live stream has priced; a late funding retry never overwrites them.
    live_priced: Vec<bool>,
    // Rows created only to hold live fields before the REST seed commits.
    staged_tickers: Vec<String>,
}

impl Bingx {
    pub fn new(options: Options) -> Self {
        let config_error = options
            .read_credentials
            .as_ref()
            .and_then(|read| read.validate().err());
        let credentials = if options.public_data_only {
            None
        } else {
            options
                .credentials
                .filter(|identity| identity.validate().is_ok())
                .map(|primary| Credentials {
                    primary,
                    read: options.read_credentials,
                })
        };
        Self {
            config_error,
            account: account::Account::with_slots(Slots::new(
                credentials
                    .as_ref()
                    .map(|c: &Credentials| c.primary.clone()),
                credentials
                    .as_ref()
                    .and_then(|c: &Credentials| c.read.clone()),
                // The read key serves reads only beside a configured primary.
                SlotPolicy {
                    secondary_requires_primary: true,
                },
            )),
            trading: trading::Trading::default(),
            history: history::History::default(),
            credentials,
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            seed: Vec::new(),
            seed_index: Vec::new(),
            seed_rows_index: Vec::new(),
            live_tickers: Default::default(),
            seed_reported: Vec::new(),
            sequence: std::cell::Cell::new(0),
            retries: Default::default(),
            reread_stalls: 0,
            tickers_committed: false,
            live_priced: Vec::new(),
            staged_tickers: Vec::new(),
        }
    }

    fn observe_orders(&mut self, cx: &Cx<'_, Pending>) {
        for (order, kind, side, attached, native_status) in self.account.drain_order_meta() {
            self.trading
                .observe_order(&order, &kind, &side, attached, &native_status);
        }
        self.trading.retain_open_orders(cx.store);
    }

    /// Market-indexed side tables follow their symbols across a catalogue refresh.
    fn remap_market_slots(&mut self, cx: &Cx<'_, Pending>, previous: &[String]) {
        fn remap<T: Copy + Default>(
            store: &safe_cex_engine::store::Store,
            previous: &[String],
            slots: &mut Vec<T>,
        ) {
            let mut moved = vec![T::default(); store.markets().len()];
            for (symbol, value) in previous.iter().zip(slots.iter()) {
                if let Some(index) = store.market_index(symbol) {
                    moved[index as usize] = *value;
                }
            }
            *slots = moved;
        }
        remap(cx.store, previous, &mut self.seed_index);
        remap(cx.store, previous, &mut self.seed_rows_index);
        remap(cx.store, previous, &mut self.live_priced);
    }

    fn retry_list(&mut self, cx: &mut Cx<'_, Pending>, kind: List) {
        if matches!(kind, List::Premium) {
            self.seed_reported.fill(false);
        }
        self.retries.failed(cx, kind);
    }

    fn candle_response_context(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: &safe_cex_engine::request_context::Context,
        response: Response<'_>,
        rows: Vec<Candle>,
        valid: bool,
    ) {
        let result = success(response).and_then(|_| {
            if valid {
                candle_data(rows)
            } else {
                Err("malformed BingX list row".into())
            }
        });
        match result {
            Ok(data) => {
                let _ = cx.done_context(context, data);
            }
            Err(error) => {
                let _ = cx.command_error_context(context, error.clone());
                let _ = cx.fail_context(context, error);
            }
        }
    }

    fn read(&mut self, cx: &mut Cx<'_, Pending>, path: &'static str, pending: Pending) {
        if !self.tickers_committed
            && matches!(&pending, Pending::Daily { .. } | Pending::Premium { .. })
        {
            self.live_tickers.begin();
        }
        if self.credentials.is_some() {
            if let Err((pending, error)) = managed::read(
                cx,
                &self.account.slots,
                managed::Read {
                    method: Method::Get,
                    path,
                    params: Params::new(),
                    at: Some(ArrayAt::EnvelopeField("data")),
                },
                pending,
                safe_cex_engine::request_context::Scope::Background,
                ReadRole::Read { scope: None },
            ) {
                self.on_sign_error(cx, *pending, error);
            }
        } else if let Err((pending, error)) = managed::public(
            cx,
            managed::Read {
                method: Method::Get,
                path,
                params: Params::new(),
                at: Some(ArrayAt::EnvelopeField("data")),
            },
            pending,
            safe_cex_engine::request_context::Scope::Background,
        ) {
            self.on_sign_error(cx, *pending, error);
        }
    }

    fn resend_managed(&mut self, cx: &mut Cx<'_, Pending>, mut signed: Signed) {
        // A refused envelope cannot contribute rows or decode status to a
        // later successful physical attempt of the same logical candle read.
        if let Pending::Candles { rows, valid, .. } = &mut signed.inner {
            rows.clear();
            *valid = true;
        }
        // The retry keeps its Selection, so account evidence is tagged again.
        if let Err((pending, error)) = managed::retry(cx, signed) {
            self.on_sign_error(cx, *pending, error);
        }
    }

    /// The seed's ticker slot for a native symbol; after commit, the store's.
    fn ticker_slot(&self, cx: &Cx<'_, Pending>, id: &str) -> Option<usize> {
        let market = cx
            .store
            .market_by_id(id)
            .and_then(|m| cx.store.market_index(&m.symbol))? as usize;
        let slots = if self.tickers_committed {
            &self.seed_index
        } else {
            &self.seed_rows_index
        };
        slots.get(market).copied().flatten()
    }

    /// Publishes the seeded tickers over any live fields received meanwhile.
    /// Without a funding read every daily ticker is kept; with one, only
    /// reported ones. The private side already started at markets.
    fn commit_tickers(&mut self, cx: &mut Cx<'_, Pending>, keep_unreported: bool) {
        let reported = std::mem::take(&mut self.seed_reported);
        if !keep_unreported {
            let mut index = 0;
            self.seed.retain(|_| {
                let keep = reported[index];
                index += 1;
                keep
            });
        }
        let mut rows = std::mem::take(&mut self.seed);
        let seeded = rows.len();
        self.live_tickers.finish(cx.store, &mut rows);
        // TS publishes only seeded tickers; a live-only staged row is dropped.
        let staged = std::mem::take(&mut self.staged_tickers);
        let mut index = 0;
        rows.retain(|row| {
            index += 1;
            index <= seeded || !staged.contains(&row.symbol)
        });
        self.seed_index.resize(cx.store.markets().len(), None);
        self.seed_index.fill(None);
        for (i, ticker) in rows.iter().enumerate() {
            if let Some(market) = cx.store.market_index(&ticker.symbol) {
                self.seed_index[market as usize] = Some(i);
            }
        }
        cx.store.set_tickers(rows);
        self.tickers_committed = true;
    }

    fn start_public(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.public_feed_policy() == PublicFeedPolicy::Direct
            && let Err(error) = self.connect_public(cx)
        {
            Self::socket_notice(cx, error);
        }
    }

    fn symbol_id(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Option<String> {
        cx.store.market(symbol).map(|m| m.id.clone())
    }

    fn topic_book(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Option<String> {
        Some(format!(
            "market.depth.{}.step0.level100",
            self.symbol_id(cx, symbol)?
        ))
    }
    fn topic_candle(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        interval: Interval,
    ) -> Option<String> {
        Some(format!(
            "market.kline.{}.{}.trade.utc+8",
            self.symbol_id(cx, symbol)?,
            stream_interval(interval)?
        ))
    }

    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Some(socket) = self.public.socket else {
            return;
        };
        if text == "Ping" {
            cx.send(socket, "Pong");
            return;
        }
        if text == "Pong" {
            return;
        }
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if message.ping.is_some() {
            cx.send(socket, "Pong");
            return;
        }
        if message.pong.is_some() {
            return;
        }
        let Some(data) = message.data else { return };
        if message.data_type == "market.contracts" {
            let result = safe_cex_engine::json::each_in_field::<LiveTicker<'_>>(
                data.get(),
                "contracts",
                |row| {
                    let Some(market) = cx.store.market_index_by_id(&row.symbol) else {
                        return;
                    };
                    // Before the seed commits, a staged row keeps live fields for its overlay;
                    // afterwards, like TS, markets the daily or funding read omitted stay absent.
                    let ticker = match self.seed_index.get(market as usize).copied().flatten() {
                        Some(ticker) => ticker,
                        None if !self.tickers_committed => {
                            let symbol = cx.store.markets()[market as usize].symbol.clone();
                            let Some(ticker) = cx.store.ensure_ticker(&symbol) else {
                                return;
                            };
                            if !self.staged_tickers.contains(&symbol) {
                                self.staged_tickers.push(symbol);
                            }
                            ticker as usize
                        }
                        None => return,
                    };
                    let step = cx.store.markets()[market as usize].precision.price;
                    let mut mask = (1 << 3) | (1 << 4) | (1 << 7);
                    if row.trade_price > 0.0 {
                        mask |= 7;
                    }
                    if row.change_percentage.is_some() {
                        mask |= 1 << 5;
                    }
                    if row.volume.is_some() {
                        mask |= 1 << 9;
                    }
                    if row.value.is_some() {
                        mask |= 1 << 10;
                    }
                    cx.store.update_ticker_at(ticker as u32, |t| {
                        if row.trade_price > 0. {
                            t.last = row.trade_price;
                            (t.bid, t.ask) = touch(row.trade_price, step)
                        }
                        t.mark = row.fair_price;
                        t.index = row.index_price;
                        t.funding_rate = row.funding_rate / 100.;
                        t.percentage = row.change_percentage.unwrap_or_default();
                        t.volume = row.volume.unwrap_or_default();
                        t.quote_volume = row.value.unwrap_or_default();
                        t.funding_time = 0.;
                    });
                    let symbol = &cx.store.markets()[market as usize].symbol;
                    self.live_tickers.record(cx.store, symbol, mask);
                    if self.live_priced.len() <= market as usize {
                        self.live_priced.resize(cx.store.markets().len(), false);
                    }
                    self.live_priced[market as usize] = true;
                },
            );
            if result.is_err() {
                cx.notice(
                    Level::Warn,
                    code::STREAM_MALFORMED,
                    "invalid BingX ticker frame",
                )
            }
            return;
        }
        if let Some(rest) = message.data_type.strip_prefix("market.depth.") {
            let Some(id) = rest.strip_suffix(".step0.level100") else {
                return;
            };
            let Some(m) = cx.store.market_by_id(id) else {
                return;
            };
            let symbol = m.symbol.clone();
            let Ok(Depth { mut bids, mut asks }) = serde_json::from_str::<Depth>(data.get()) else {
                return;
            };
            if let Some(state) = self.books.get_mut(&symbol) {
                bids.sort_unstable_by(|a, b| b.price.total_cmp(&a.price));
                asks.sort_unstable_by(|a, b| a.price.total_cmp(&b.price));
                state.book.load_sorted(
                    BookSide::Bids,
                    bids.into_iter().map(|l| (l.price, l.volume)),
                );
                state.book.load_sorted(
                    BookSide::Asks,
                    asks.into_iter().map(|l| (l.price, l.volume)),
                );
                state.ready = true;
                cx.book_changed(&symbol);
            }
        } else if let Some(rest) = message.data_type.strip_prefix("market.kline.") {
            let mut split = rest.split('.');
            let (Some(id), Some(native_interval)) = (split.next(), split.next()) else {
                return;
            };
            let Some(m) = cx.store.market_by_id(id) else {
                return;
            };
            let symbol = m.symbol.clone();
            let matches: Vec<_> = self
                .candles
                .keys()
                .filter(|(s, i)| s == &symbol && stream_interval(*i) == Some(native_interval))
                .cloned()
                .collect();
            let Ok(data) = serde_json::from_str::<CandleData>(data.get()) else {
                return;
            };
            if let Some(row) = data.kline_infos_vo.into_iter().next() {
                let candle = Candle::from(row);
                for (symbol, interval) in matches {
                    cx.candle(&symbol, interval, candle)
                }
            }
        }
    }
}

fn touch(last: f64, step: f64) -> (f64, f64) {
    if last <= 0. {
        return (0., 0.);
    }
    // Native ticks are powers of ten. Recover common scales without formatting;
    // above 15 digits reciprocal rounding can lose an integer tick.
    let scale = step.recip().round();
    let factor = if (1. ..=1e15).contains(&scale) {
        scale
    } else {
        10f64.powi(num::decimals(step) as i32)
    };
    (
        ((last * factor - step * factor).round() / factor).max(0.),
        (last * factor + step * factor).round() / factor,
    )
}

fn stream_interval(interval: Interval) -> Option<&'static str> {
    use Interval::*;
    Some(match interval {
        M1 => "1min",
        M3 => "3min",
        M5 => "5min",
        M15 => "15min",
        M30 => "30min",
        H1 => "1hour",
        H2 => "2hour",
        H3 => "3hour",
        H4 => "4hour",
        H6 => "6hour",
        H12 => "12hour",
        D1 => "1day",
        W1 => "1week",
        Mo1 => "1month",
        _ => return None,
    })
}

fn refuse_candle_query(cx: &mut Cx<'_, Pending>, error: impl Into<safe_cex_engine::VenueError>) {
    use safe_cex_engine::CommandWait;
    use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};

    let remaining = match cx.command_wait() {
        Some(CommandWait::Remaining(remaining)) => remaining,
        Some(CommandWait::Expired) => {
            let _ = cx.reject_expired_command(Operation::Read);
            return;
        }
        None => {
            let _ = cx.reject_unstamped_command(Operation::Read);
            return;
        }
    };
    let Ok(context) = cx.admit_http_command(Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: remaining.min(20_000),
        pacing: None,
        retry: RetryPolicy::Never,
    }) else {
        return;
    };
    let _ = cx.fail_context(&context, error);
}

fn interval_ms(interval: Interval) -> Option<f64> {
    stream_interval(interval).map(|_| interval.ms())
}

pub(crate) fn success(response: Response<'_>) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Envelope {
        code: i64,
        #[serde(default)]
        msg: String,
    }
    if !(200..300).contains(&response.status) {
        return Err(format!("BingX HTTP {}: {}", response.status, response.body));
    }
    let envelope: Envelope =
        serde_json::from_str(response.body).map_err(|e| format!("invalid BingX response: {e}"))?;
    if envelope.code == 0 {
        Ok(())
    } else {
        Err(format!("BingX {}: {}", envelope.code, envelope.msg))
    }
}

impl Venue for Bingx {
    fn managed_sockets() -> bool {
        true
    }

    fn fence_uncertain_placements(&self) -> bool {
        true
    }

    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            leverage_query: safe_cex_engine::LeverageQuery::OnReadAndMetadata,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn account_auth_outcome(
        &self,
        pending: &Self::Pending,
        response: &Response<'_>,
        _now: f64,
        _store: &safe_cex_engine::Store,
    ) -> Option<safe_cex_engine::AuthOutcome> {
        self.account_auth(pending, response)
    }

    fn cancel_needs_stored_order(&self) -> bool {
        true
    }

    fn guard_order_mutations(&self) -> bool {
        true
    }

    fn socket_capabilities() -> safe_cex_engine::socket_lifecycle::SocketCapabilities {
        safe_cex_engine::socket_lifecycle::SocketCapabilities { handoff: false }
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
        _store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        items: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.encode_interests(feed, subscribe, items, emit)
    }

    type Pending = Pending;
    fn export_venue_state(&self) -> Option<Vec<u8>> {
        Some(self.trading.export_venue_state(self.account.mode_known()))
    }
    fn apply_venue_state(&mut self, cx: &mut Cx<'_, Pending>, state: &[u8]) {
        if !self.trading.apply_venue_state(state, cx.store) {
            cx.notice(
                Level::Warn,
                code::STREAM_MALFORMED,
                "Unreadable BingX venue state from the data source",
            );
        }
    }
    fn on_frame_ingested(&mut self, cx: &mut Cx<'_, Pending>) {
        self.trading.refresh_observed(cx.store);
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(error) = self.config_error {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            return;
        }
        if let Err(error) = self.register_sockets(cx) {
            Self::socket_notice(cx, error);
            return;
        }
        self.read(
            cx,
            paths::MARKETS,
            Pending::Markets {
                rows: Vec::with_capacity(1536),
                skipped: Vec::new(),
            },
        );
        if self.credentials.is_none() {
            cx.store.set_balance(Default::default());
            cx.store.set_orders(Vec::new());
            cx.store.set_positions(Vec::new());
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.handles_socket(socket) {
            if let Some(c) = &self.credentials {
                self.account.on_message(cx, text, c);
            }
            self.observe_orders(cx);
        } else if self.public.socket == Some(socket) {
            self.public_message(cx, text)
        }
    }
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Identity(_) | Pending::Metadata(_) => {}
            Pending::Signed(signed) => {
                if signed
                    .context()
                    .is_some_and(|context| cx.validate_http_context(context).is_err())
                {
                    return;
                }
                self.on_item(cx, &mut signed.inner, item);
            }
            Pending::Account(request) => self.account.on_item(cx, request, item),
            Pending::Trade(_) => {}
            Pending::History(request) => self.history.on_item(cx, request, item),
            Pending::Markets { rows, skipped } => {
                if rows.len() == rows.capacity() {
                    rows.reserve_exact(128)
                }
                let market = serde_json::from_str::<Contract<'_>>(item).map(|m| Market {
                    id: m.symbol.to_string(),
                    symbol: m.symbol.replace('-', ""),
                    pretty_symbol: None,
                    base: m.asset.into_owned(),
                    quote: m.currency.into_owned(),
                    active: m.status == 1.,
                    precision: Precision {
                        amount: 10f64.powi(m.quantity_precision as i32).recip(),
                        price: 10f64.powi(m.price_precision as i32).recip(),
                    },
                    limits: Limits {
                        amount: MinMax {
                            min: m.trade_min_limit,
                            max: f64::INFINITY,
                        },
                        leverage: MinMax {
                            min: 1.,
                            max: if m.max_long_leverage > 0. {
                                m.max_long_leverage
                            } else {
                                100.
                            },
                        },
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
                });
                match market {
                    Ok(market) if market.precision.is_tradable() => rows.push(market),
                    _ => {
                        safe_cex_engine::startup::skipped_row(cx);
                        skipped.push(row_symbol(item).unwrap_or_default().into_owned())
                    }
                }
            }
            Pending::Daily { rows, skipped } => match serde_json::from_str::<Daily<'_>>(item) {
                Ok(d) => {
                    if let Some(m) = cx.store.market_by_id(&d.symbol) {
                        let (bid, ask) = touch(d.last_price, m.precision.price);
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            bid,
                            ask,
                            last: d.last_price,
                            percentage: d.price_change_percent,
                            volume: d.volume,
                            quote_volume: d.quote_volume,
                            ..Ticker::default()
                        })
                    }
                }
                Err(_) => {
                    safe_cex_engine::startup::skipped_row(cx);
                    skipped.push(row_symbol(item).unwrap_or_default().into_owned())
                }
            },
            Pending::Premium { skipped } => match serde_json::from_str::<Premium<'_>>(item) {
                Ok(p) => {
                    let Some(i) = self.ticker_slot(cx, &p.symbol) else {
                        return;
                    };
                    let apply = |t: &mut Ticker| {
                        t.mark = p.mark_price;
                        t.index = p.index_price;
                        t.funding_rate = p.last_funding_rate / 100.;
                        t.funding_time = p.next_funding_time;
                    };
                    if self.tickers_committed {
                        let live = cx
                            .store
                            .market_index_by_id(&p.symbol)
                            .and_then(|m| self.live_priced.get(m as usize).copied())
                            .unwrap_or(false);
                        if !live {
                            cx.store.update_ticker_at(i as u32, apply);
                        }
                    } else {
                        apply(&mut self.seed[i]);
                        self.seed_reported[i] = true
                    }
                }
                Err(_) => {
                    safe_cex_engine::startup::skipped_row(cx);
                    let symbol = row_symbol(item);
                    // A malformed funding row keeps its daily ticker, without funding.
                    if !self.tickers_committed
                        && let Some(i) = symbol.as_deref().and_then(|s| self.ticker_slot(cx, s))
                    {
                        self.seed_reported[i] = true
                    }
                    skipped.push(symbol.unwrap_or_default().into_owned())
                }
            },
            Pending::Candles { rows, valid, .. } => match serde_json::from_str::<Kline>(item) {
                Ok(k) => rows.push(k.into()),
                Err(_) => *valid = false,
            },
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Metadata(leg) = pending {
            match &self.credentials {
                Some(credentials) => {
                    self.account
                        .metadata_response(cx, credentials, *leg, response)
                }
                None => {
                    account::fail_metadata_plan(cx, &leg, "BingX metadata requires credentials")
                }
            }
            return;
        }
        if let Pending::Identity(read) = pending {
            self.identity_response(cx, read, response);
            return;
        }
        let pending = match pending {
            Pending::Signed(signed) => {
                if signed
                    .context()
                    .is_some_and(|context| cx.validate_http_context(context).is_err())
                {
                    return;
                }
                if !signed.resent && signature_rejected(response.body) {
                    self.resend_managed(cx, *signed);
                } else {
                    let context = signed.context().cloned();
                    match (&context, signed.inner) {
                        (Some(context), Pending::Candles { rows, valid, .. }) => {
                            self.candle_response_context(cx, context, response, rows, valid)
                        }
                        (_, pending) => self.on_response(cx, pending, response),
                    }
                    managed::finish(cx, context);
                }
                return;
            }
            Pending::History(request) => {
                self.history.on_response(cx, request, response);
                return;
            }
            Pending::Trade(request) => {
                let account = &mut self.account;
                self.trading.on_response(
                    cx,
                    self.credentials.as_ref(),
                    request,
                    response,
                    |cx, request| {
                        if request.changes_settings() {
                            account.settings_changed();
                        }
                        if let Some(scope) = request.settings_scope() {
                            cx.store.invalidate_metadata(scope);
                        }
                        account.bump_orders(cx.store);
                        account.bump_positions();
                        if request.accepts_placement(response) {
                            account.accept_placed_response(cx, response.body);
                        }
                        if (request.changes_settings() || request.reconciles_orders())
                            && success(response).is_ok()
                            && let Some(credentials) = &self.credentials
                        {
                            account.reconcile(cx, credentials);
                        }
                    },
                );
                self.observe_orders(cx);
                return;
            }
            Pending::Account(request) => {
                if let Some(c) = &self.credentials {
                    self.account.on_response(cx, request, response, c);
                }
                self.observe_orders(cx);
                return;
            }
            pending => pending,
        };
        if let Err(error) = success(response) {
            self.on_request_failed(cx, pending, &error);
            return;
        }
        match pending {
            Pending::Markets { rows, skipped } => {
                log_skipped(cx, "market", &skipped);
                if rows.is_empty() {
                    self.retry_list(cx, List::Markets);
                    return;
                }
                let refresh = cx.store.loaded().markets;
                let previous: Vec<String> = if refresh {
                    cx.store
                        .markets()
                        .iter()
                        .map(|m| m.symbol.clone())
                        .collect()
                } else {
                    Vec::new()
                };
                let mut rows = if refresh {
                    keep_listed(cx.store.markets(), rows)
                } else {
                    rows
                };
                rows.shrink_to_fit();
                cx.store.set_markets(rows);
                let (before, after) = self
                    .account
                    .set_skipped_markets(cx.store, skipped.iter().map(|id| id.replace('-', "")));
                // A partial catalogue commits so private reads start, then re-reads.
                if reread_partial(cx, &mut self.reread_stalls, refresh, before, after) {
                    self.retry_list(cx, List::Markets);
                } else {
                    self.retries.succeeded(cx, &List::Markets);
                }
                if refresh {
                    self.remap_market_slots(cx, &previous);
                    return;
                }
                // Frame-fed: tickers and the account arrive as frames.
                if cx.frame_fed() {
                    return;
                }
                if let Some(c) = &self.credentials {
                    self.account.start(cx, c);
                }
                self.start_public(cx);
                self.read(
                    cx,
                    paths::DAILY,
                    Pending::Daily {
                        rows: Vec::with_capacity(cx.store.markets().len()),
                        skipped: Vec::new(),
                    },
                )
            }
            Pending::Daily { mut rows, skipped } => {
                log_skipped(cx, "ticker", &skipped);
                // A market whose daily row was malformed still gets a slot, filled
                // later by funding and the ticker stream.
                let reported = rows.len();
                for id in &skipped {
                    if let Some(m) = cx.store.market_by_id(id)
                        && !rows.iter().any(|t| t.id == m.id)
                    {
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            ..Ticker::default()
                        });
                    }
                }
                // Neither a valid row nor an identifiable placeholder: never publish empty.
                if rows.is_empty() {
                    self.retry_list(cx, List::Daily);
                    return;
                }
                self.retries.succeeded(cx, &List::Daily);
                self.seed_rows_index = vec![None; cx.store.markets().len()];
                for (i, ticker) in rows.iter().enumerate() {
                    if let Some(index) = cx.store.market_index(&ticker.symbol) {
                        self.seed_rows_index[index as usize] = Some(i)
                    }
                }
                self.seed_reported = (0..rows.len()).map(|i| i >= reported).collect();
                self.seed = rows;
                self.read(
                    cx,
                    paths::PREMIUM,
                    Pending::Premium {
                        skipped: Vec::new(),
                    },
                )
            }
            Pending::Premium { skipped } => {
                log_skipped(cx, "funding", &skipped);
                if self.tickers_committed {
                    self.retries.succeeded(cx, &List::Premium);
                } else if self.seed_reported.iter().any(|reported| *reported) {
                    self.retries.succeeded(cx, &List::Premium);
                    self.commit_tickers(cx, false);
                } else {
                    // No funding row named any seeded market: keep the daily tickers
                    // rather than publishing an empty set, and retry funding.
                    self.on_request_failed(
                        cx,
                        Pending::Premium { skipped },
                        "BingX funding read named no seeded market",
                    );
                }
            }
            pending => self.on_request_failed(cx, pending, "malformed BingX list row"),
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        match pending {
            Pending::Signed(signed) if !signed.resent && signature_rejected(response.body) => {
                self.resend_managed(cx, *signed);
            }
            Pending::Signed(signed) => {
                if signed.context().is_some() && matches!(&signed.inner, Pending::Candles { .. }) {
                    self.on_request_failed(
                        cx,
                        Pending::Signed(signed),
                        &format!(
                            "response did not contain the expected list: {}",
                            response.body
                        ),
                    );
                } else {
                    let context = signed.context().cloned();
                    self.on_stream_error(cx, signed.inner, response);
                    managed::finish(cx, context);
                }
            }
            Pending::History(request) => self.history.on_stream_error(cx, request, response),
            pending => self.on_request_failed(
                cx,
                pending,
                &format!(
                    "response did not contain the expected list: {}",
                    response.body
                ),
            ),
        }
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Identity(read) => self.identity_failed(cx, read, error),
            Pending::History(request) => self.history.on_http_failed(cx, request, error),
            Pending::Trade(request) => {
                self.trading
                    .on_sign_failed(cx, self.credentials.as_ref(), request, error)
            }
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }
    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.trading.retired(owner);
    }
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _: Option<&[String]>) {
        if let Some(credentials) = &self.credentials {
            self.account.reconcile_orders(cx, credentials);
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
            Pending::Trade(request) => request.reconciliation(),
            _ => Err(safe_cex_engine::request_context::Error::Invalid),
        }
    }
    fn unproven_http_evidence(
        &self,
        pending: &Pending,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        if matches!(pending, Pending::Trade(_)) {
            trading::evidence(response.body, evidence);
        }
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        mut error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Signed(signed) => {
                if let Some(context) = signed.context()
                    && matches!(&signed.inner, Pending::Candles { .. })
                {
                    let _ = cx.command_error_context(context, error.message.clone());
                    let _ = cx.fail_context(context, error);
                    managed::finish(cx, signed.context().cloned());
                    return;
                }
                if signed.resent && matches!(&signed.inner, Pending::Trade(_)) {
                    // Only the retry was refused before release. The original
                    // write reached the venue, so reconciliation is still required.
                    error.outcome = Some(safe_cex_engine::Outcome::Unknown);
                }
                let context = signed.context().cloned();
                self.on_sign_error(cx, signed.inner, error);
                managed::finish(cx, context);
            }
            Pending::Identity(read) => self.identity_failed(cx, read, error),
            Pending::History(request) => self.history.on_sign_failed(cx, request, error),
            Pending::Trade(request) => {
                self.trading
                    .on_sign_failed(cx, self.credentials.as_ref(), request, error)
            }
            Pending::Account(request) => {
                if let Some(credentials) = &self.credentials {
                    self.account.on_sign_failed(cx, request, error, credentials);
                }
            }
            // Provider refusal is typed and is never venue-auth evidence.
            Pending::Metadata(leg) => account::fail_metadata_plan(cx, &leg, error),
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        let discovery = matches!(
            &pending,
            Pending::Markets { .. } | Pending::Daily { .. } | Pending::Premium { .. }
        );
        match pending {
            Pending::Metadata(leg) => {
                // A failing private read surfaces as an Error at the retried-failure cadence.
                cx.retried_failure(
                    "bingx.lib:private",
                    format!("BingX private request failed: {error}"),
                );
                account::fail_metadata_plan(cx, &leg, account::metadata_error(error));
                return;
            }
            Pending::Signed(signed) => {
                if let Some(context) = signed.context()
                    && matches!(&signed.inner, Pending::Candles { .. })
                {
                    let _ = cx.command_error_context(context, error.to_owned());
                    let _ = cx.fail_context(context, error);
                    managed::finish(cx, signed.context().cloned());
                    return;
                }
                let context = signed.context().cloned();
                self.on_request_failed(cx, signed.inner, error);
                managed::finish(cx, context);
                return;
            }
            Pending::Identity(read) => {
                self.identity_failed(cx, read, error.into());
                return;
            }
            Pending::History(request) => {
                self.history.on_failed(cx, request, error);
                return;
            }
            Pending::Trade(request) => {
                self.trading
                    .on_failed(cx, self.credentials.as_ref(), request, error);
                return;
            }
            Pending::Account(request) => {
                if let Some(c) = &self.credentials {
                    self.account.on_request_failed(cx, request, error, c);
                }
                return;
            }
            // Only a refused read admission leaves candles without custody.
            Pending::Candles { command, .. } => {
                cx.command_error(command, error);
                refuse_candle_query(cx, error);
                return;
            }
            Pending::Markets { .. } => self.retry_list(cx, List::Markets),
            Pending::Daily { .. } => self.retry_list(cx, List::Daily),
            Pending::Premium { .. } => {
                if !self.tickers_committed {
                    cx.notice(
                        Level::Warn,
                        code::REQUEST_FAILED,
                        "BingX funding read failed; starting without funding rates",
                    );
                    self.commit_tickers(cx, true);
                }
                self.retry_list(cx, List::Premium)
            }
        }
        cx.notice(
            if discovery { Level::Warn } else { Level::Error },
            code::REQUEST_FAILED,
            error,
        );
    }
    fn read_root_retired(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
        _pending: Vec<Pending>,
    ) {
        self.history.retire(owner);
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.history.on_timer(cx, timer);
        if self.account.handles_timer(timer) {
            if let Some(c) = &self.credentials {
                self.account.on_timer(cx, timer, c);
            }
            return;
        }
        if let Some(kind) = self.retries.on_timer(timer) {
            let (path, pending) = match kind {
                List::Markets => (
                    paths::MARKETS,
                    Pending::Markets {
                        rows: Vec::new(),
                        skipped: Vec::new(),
                    },
                ),
                List::Daily => (
                    paths::DAILY,
                    Pending::Daily {
                        rows: Vec::with_capacity(cx.store.markets().len()),
                        skipped: Vec::new(),
                    },
                ),
                List::Premium => (
                    paths::PREMIUM,
                    Pending::Premium {
                        skipped: Vec::new(),
                    },
                ),
            };
            self.read(cx, path, pending);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if matches!(
            command,
            Command::SetLeverage { .. }
                | Command::SetMarginType { .. }
                | Command::SetPositionMode { .. }
        ) {
            self.account.settings_changed();
        }
        match &command {
            Command::SetLeverage { symbol, .. } | Command::SetMarginType { symbol, .. } => {
                cx.store.invalidate_metadata(Some(symbol))
            }
            Command::SetPositionMode { .. } => cx.store.invalidate_metadata(None),
            _ => {}
        }
        if let Command::FetchAccount { .. } = command {
            self.fetch_identity(cx);
            return;
        }
        match command {
            Command::FetchLeverage { command, symbol } => {
                let publish = !cx.store.metadata_write_pending(&symbol);
                self.account.fetch_metadata(
                    cx,
                    self.credentials.as_ref(),
                    command,
                    symbol,
                    publish,
                );
            }
            Command::Query { command, query } => {
                self.history
                    .query(cx, self.credentials.as_ref(), command, query)
            }
            Command::SubscribeOrderBook { symbol } => self.subscribe_book(cx, symbol),
            Command::UnsubscribeOrderBook { symbol } => self.unsubscribe_book(cx, symbol),
            Command::SubscribeCandles { symbol, interval } => {
                if stream_interval(interval).is_none() {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("BingX"),
                    );
                    return;
                }
                self.subscribe_candles(cx, symbol, interval);
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                self.unsubscribe_candles(cx, symbol, interval)
            }
            Command::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => {
                let Some(market) = cx.store.market(&symbol) else {
                    refuse_candle_query(cx, format!("unknown market {symbol}"));
                    return;
                };
                let Some(step) = interval_ms(interval) else {
                    refuse_candle_query(cx, interval.unsupported("BingX"));
                    return;
                };
                let end = to_ms.unwrap_or(cx.now);
                let start = if limit.is_none() {
                    from_ms.unwrap_or(end - step * 500.)
                } else {
                    end - step * f64::from(limit.unwrap_or(500).min(1500))
                };
                if !start.is_finite() || !end.is_finite() || start > end {
                    refuse_candle_query(cx, "invalid candle time range");
                    return;
                }
                let params = Params::new()
                    .str("symbol", &market.id)
                    .str("interval", interval.as_str())
                    .num("startTime", start)
                    .num("endTime", end);
                let pending = Pending::Candles {
                    command,
                    rows: Vec::new(),
                    valid: true,
                };
                if self.credentials.is_some() {
                    if let Err((pending, error)) = managed::read(
                        cx,
                        &self.account.slots,
                        managed::Read {
                            method: Method::Get,
                            path: paths::KLINES,
                            params,
                            at: Some(ArrayAt::EnvelopeField("data")),
                        },
                        pending,
                        safe_cex_engine::request_context::Scope::Command(command),
                        ReadRole::Read { scope: None },
                    ) {
                        self.on_sign_error(cx, *pending, error);
                    }
                } else if let Err((pending, error)) = managed::public(
                    cx,
                    managed::Read {
                        method: Method::Get,
                        path: paths::KLINES,
                        params,
                        at: Some(ArrayAt::EnvelopeField("data")),
                    },
                    pending,
                    safe_cex_engine::request_context::Scope::Command(command),
                ) {
                    self.on_sign_error(cx, *pending, error);
                }
            }
            other => {
                let changes_settings = matches!(
                    &other,
                    Command::SetLeverage { .. }
                        | Command::SetMarginType { .. }
                        | Command::SetPositionMode { .. }
                );
                let account = &mut self.account;
                if !self
                    .trading
                    .command(cx, self.credentials.as_ref(), other, |cx| {
                        if changes_settings {
                            account.settings_changed();
                        }
                        account.bump_orders(cx.store);
                        account.bump_positions();
                    })
                {
                    cx.notice(
                        Level::Warn,
                        code::COMMAND_UNSUPPORTED,
                        "unsupported BingX command",
                    );
                }
            }
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|b| b.ready).map(|b| &b.book)
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
            Level::Warn,
            code::REQUEST_FAILED,
            format!("BingX stops re-reading a catalogue with {after} malformed markets"),
        );
    }
    *stalls < 5
}

fn candle_data(mut rows: Vec<Candle>) -> Result<String, String> {
    rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
    serde_json::to_string(&rows).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_tick_scales_preserve_decimal_touch_prices() {
        for precision in 0..=18 {
            let factor = 10f64.powi(precision);
            let step = factor.recip();
            for last in [0., step, 0.000_001_234_567_89, 1.234_567_89, 90_000.123_456] {
                let expected = if last <= 0. {
                    (0., 0.)
                } else {
                    (
                        ((last * factor - step * factor).round() / factor).max(0.),
                        (last * factor + step * factor).round() / factor,
                    )
                };
                assert_eq!(
                    super::touch(last, step),
                    expected,
                    "precision={precision}, last={last}"
                );
            }
        }
    }
}
