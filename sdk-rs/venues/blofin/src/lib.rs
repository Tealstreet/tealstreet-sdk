//! BloFin linear perpetuals on the sans-I/O engine.
mod account;
mod closed;
#[path = "sign/dispatch.rs"]
pub mod dispatch_signing;
mod history;
mod socket_codec;
mod trading;
mod wire;
use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::startup::{DiscoveryRead, PublicRetries};
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, PublicFeedPolicy, Response, SocketId, TimerId,
    Venue, VenueError, num,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use wire::*;

#[derive(Clone, Default)]
pub struct Options {
    pub testnet: bool,
    /// TS enables payload attribution in production builds or with tealstreetProxy.
    pub broker_attribution: bool,
    pub public_data_only: bool,
    /// Opaque refs from trusted setup; the provider holds the key material.
    pub identities: safe_cex_engine::signing::Identities,
}
safe_cex_crypto::redacted_debug!(Options);
pub enum Pending {
    Managed(Context, Box<Pending>),
    PublicTrades(history::PublicTradesRequest),
    History(history::Request),
    Closed(closed::Request),
    Trade(Box<trading::Request>),
    Account(
        safe_cex_engine::selected_read::AdmittedRead,
        account::Request,
    ),
    Leverage(account::Leverage),
    Markets {
        rows: Vec<Market>,
        malformed: HashSet<String>,
    },
    Tickers {
        rows: Vec<Ticker>,
        failed: bool,
    },
    Candles {
        rows: Vec<Candle>,
        failed: bool,
    },
}
#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    feed: Option<safe_cex_engine::socket_lifecycle::FeedId>,
    pending: Option<(u64, socket_codec::Change)>,
    queued: std::collections::VecDeque<socket_codec::Change>,
}
#[derive(Default)]
struct DepthState {
    book: Book,
    sequence: Option<u64>,
    subscribers: u32,
    resync: bool,
}
#[derive(Default)]
struct CandleState {
    subscribers: u32,
    last: Option<Candle>,
}
pub struct Blofin {
    options: Options,
    account: account::Account,
    trading: trading::Trading,
    public: Connection,
    startup: PublicRetries,
    // Instrument ids rejected as malformed; owned exposure there holds the snapshot.
    malformed_markets: HashSet<String>,
    reread_stalls: u8,
    live_tickers: safe_cex_engine::startup::LiveTickers,
    books: HashMap<String, DepthState>,
    candles: HashMap<(String, Interval), CandleState>,
}
#[derive(Serialize)]
struct Topic<'a> {
    channel: &'a str,
    #[serde(rename = "instId")]
    inst_id: &'a str,
}
fn interval(value: Interval) -> Option<&'static str> {
    use Interval::*;
    Some(match value {
        M1 => "1m",
        M3 => "3m",
        M5 => "5m",
        M15 => "15m",
        M30 => "30m",
        H1 => "1H",
        H2 => "2H",
        H4 => "4H",
        H6 => "6H",
        H12 => "12H",
        D1 => "1D",
        W1 => "1W",
        _ => return None,
    })
}
fn candle(row: &[Number]) -> Option<Candle> {
    if row.len() < 6 {
        return None;
    }
    Some(Candle {
        timestamp: row[0].0 / 1000.0,
        open: row[1].0,
        high: row[2].0,
        low: row[3].0,
        close: row[4].0,
        volume: row.get(7).unwrap_or(&row[5]).0,
    })
}
fn round_usd(v: f64) -> f64 {
    num::adjust(v, 0.01)
}
fn discovery_error(response: &Response<'_>) -> VenueError {
    let envelope = serde_json::from_str::<wire::Envelope<'_>>(response.body).ok();
    let rejection = envelope.as_ref().filter(|e| e.code != "0");
    safe_cex_engine::startup::public_response_error(
        response.status,
        rejection.map(|e| e.code.as_ref()),
        rejection.map(|e| e.msg.as_ref()),
    )
}

impl Blofin {
    fn public_failed(&mut self, cx: &mut Cx<'_, Pending>, markets: bool, error: VenueError) {
        self.startup.failed_after(
            cx,
            if markets {
                DiscoveryRead::Markets
            } else {
                DiscoveryRead::Tickers
            },
            0.0,
            error,
        );
    }

    pub fn new(options: Options) -> Self {
        Self {
            options,
            account: account::Account::new(),
            trading: trading::Trading::default(),
            public: Connection::default(),
            startup: Default::default(),
            malformed_markets: HashSet::new(),
            reread_stalls: 0,
            live_tickers: Default::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
        }
    }
    fn host(&self) -> &'static str {
        if self.options.testnet {
            "https://demo-trading-openapi.blofin.com"
        } else {
            "https://openapi.blofin.com"
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
    fn read(&mut self, cx: &mut Cx<'_, Pending>, path: &str, pending: Pending) {
        let context = match cx.http_context(Scope::Background, Self::read_policy()) {
            Ok(context) => context,
            Err(_) => {
                self.public_failed(
                    cx,
                    matches!(pending, Pending::Markets { .. }),
                    VenueError::new("BloFin read lifecycle is unavailable")
                        .kind(ErrorKind::Internal),
                );
                return;
            }
        };
        self.managed_read(cx, path, context, pending);
    }
    fn managed_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        context: Context,
        pending: Pending,
    ) {
        if matches!(&pending, Pending::Tickers { .. }) {
            self.live_tickers.begin();
        }
        match cx.request_managed(
            context.clone(),
            None,
            Method::Get,
            format!("{}{path}", self.host()),
            None,
            Pending::Managed(context, Box::new(pending)),
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::EnvelopeField("data")),
            Err((pending, _)) => self.on_sign_error(
                cx,
                pending,
                VenueError::new("BloFin read preparation is unavailable").kind(ErrorKind::Internal),
            ),
        }
    }
    fn public_failure(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        pending: Pending,
        error: VenueError,
    ) {
        if matches!(&pending, Pending::Candles { .. }) {
            let _ = cx.fail_context(&context, error);
        } else {
            let _ = cx.finish_http_context(&context);
            self.public_failed(cx, matches!(pending, Pending::Markets { .. }), error);
        }
    }
    fn public_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        pending: Pending,
        response: Response<'_>,
    ) {
        if !matches!(&pending, Pending::Candles { .. }) {
            let _ = cx.finish_http_context(&context);
        }
        if let Err(error) = check(&response) {
            if matches!(pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
                self.public_failed(
                    cx,
                    matches!(pending, Pending::Markets { .. }),
                    discovery_error(&response),
                );
                return;
            }
            self.public_failure(cx, context, pending, VenueError::new(error));
            return;
        }
        match pending {
            Pending::Markets { rows, malformed } if !rows.is_empty() => {
                let refresh = cx.store.loaded().markets;
                let rows = if refresh {
                    keep_listed(cx.store.markets(), rows)
                } else {
                    rows
                };
                cx.store.set_markets(rows);
                let before = self.malformed_markets.len();
                self.malformed_markets.extend(malformed);
                self.malformed_markets
                    .retain(|id| cx.store.market_by_id(id).is_none());
                let after = self.malformed_markets.len();
                // A partial catalogue commits so private reads start, then re-reads.
                if reread_partial(cx, &mut self.reread_stalls, refresh, before, after) {
                    self.public_failed(
                        cx,
                        true,
                        VenueError::new("Incomplete BloFin catalogue")
                            .kind(ErrorKind::VenueUnavailable),
                    );
                } else {
                    self.startup.succeeded(cx, DiscoveryRead::Markets);
                }
                // Frame-fed: tickers and the account arrive as frames.
                if refresh || cx.frame_fed() {
                    return;
                }
                if let Err(error) = self.connect_feed(cx, false) {
                    Self::socket_notice(cx, error);
                }
                self.read(
                    cx,
                    "/api/v1/market/tickers",
                    Pending::Tickers {
                        rows: vec![],
                        failed: false,
                    },
                );
                self.private_start(cx);
            }
            Pending::Tickers { mut rows, failed } if !failed || !rows.is_empty() => {
                self.live_tickers.finish(cx.store, &mut rows);
                cx.store.set_tickers(rows);
                self.startup.succeeded(cx, DiscoveryRead::Tickers);
            }
            Pending::Candles {
                mut rows,
                failed: false,
            } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                let _ = cx.done_context(&context, serde_json::to_string(&rows).unwrap());
            }
            other @ (Pending::Markets { .. } | Pending::Tickers { .. }) => self.public_failed(
                cx,
                matches!(other, Pending::Markets { .. }),
                discovery_error(&response),
            ),
            other => self.public_failure(
                cx,
                context,
                other,
                VenueError::new("Malformed BloFin response"),
            ),
        }
    }
    fn ticker(&mut self, cx: &mut Cx<'_, Pending>, row: &Tick<'_>, funding: bool) {
        let Some(m) = cx.store.market_by_id(&row.inst_id) else {
            return;
        };
        let symbol = m.symbol.clone();
        if cx.store.loaded().tickers && cx.store.ticker(&symbol).is_none() {
            // A malformed initial row must be able to recover on its next
            // valid event; update_ticker alone cannot add a missing symbol.
            let mut tickers = cx.store.tickers().to_vec();
            tickers.push(Ticker {
                id: m.id.clone(),
                symbol: symbol.clone(),
                ..Ticker::default()
            });
            cx.store.set_tickers(tickers);
        }
        cx.store.update_ticker(&symbol, |t| {
            if funding {
                t.funding_rate = row.funding_rate.0;
                t.funding_time = row.funding_time.0;
            } else {
                t.bid = row.bid_price.0;
                t.ask = row.ask_price.0;
                t.last = row.last.0;
                t.mark = t.last;
                t.index = t.last;
                t.open_interest = 0.0;
                t.percentage = round_usd((t.last - row.open24h.0) / row.open24h.0 * 100.0);
                t.volume = row.vol_currency24h.0;
                t.quote_volume = row.vol24h.0;
            }
        });
        let mask = if funding {
            (1 << 7) | (1 << 8)
        } else {
            ((1 << 7) - 1) | (1 << 9) | (1 << 10)
        };
        self.live_tickers.record(cx.store, &symbol, mask);
    }
    fn depth(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, message: &Message<'_>, d: &Depth) {
        if message.arg.channel == "books5" {
            for ((s, i), state) in &mut self.candles {
                if s != symbol {
                    continue;
                }
                if let (Some(last), Some(bid), Some(ask)) =
                    (state.last.as_mut(), d.bids.first(), d.asks.first())
                {
                    if bid.is_empty() || ask.is_empty() {
                        continue;
                    }
                    let step = i.ms() / 1000.0;
                    let time = (d.ts.0 / 1000.0 / step).floor() * step;
                    if last.timestamp == time {
                        let mut prices = [bid[0].0, last.close, ask[0].0];
                        prices.sort_by(f64::total_cmp);
                        if last.close == prices[1] {
                            continue;
                        }
                        last.close = prices[1];
                        last.high = last.high.max(last.close);
                        last.low = last.low.min(last.close);
                        cx.candle(s, *i, *last);
                    }
                }
            }
        }
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        let secondary = message.arg.channel == "books5";
        if !secondary
            && message.action == "update"
            && (state.sequence.is_none()
                || d.prev_seq_id.map(|n| n.0) != state.sequence
                || d.seq_id.is_none())
        {
            state.book.clear();
            state.sequence = None;
            let request = !state.resync;
            state.resync = true;
            if request {
                self.repair_book_interest(cx, symbol);
            }
            return;
        }
        if !secondary && message.action == "snapshot" {
            state.book.clear();
            state.resync = false;
        }
        if !secondary {
            state.sequence = d.seq_id.map(|n| n.0);
        }
        for (side, rows) in [(BookSide::Bids, &d.bids), (BookSide::Asks, &d.asks)] {
            if secondary {
                let old = if side == BookSide::Bids {
                    state.book.bids()
                } else {
                    state.book.asks()
                };
                let mut levels: Vec<_> = rows
                    .iter()
                    .filter(|r| r.len() >= 2 && r[1].0 != 0.0)
                    .map(|r| (r[0].0, r[1].0))
                    .collect();
                if levels.len() >= 5
                    && let Some(boundary) = levels.last().map(|(price, _)| *price)
                {
                    levels.extend(old.iter().copied().filter(|(price, _)| {
                        if side == BookSide::Bids {
                            *price < boundary
                        } else {
                            *price > boundary
                        }
                    }));
                }
                state.book.load_sorted(side, levels);
            } else {
                for row in rows {
                    if row.len() >= 2 {
                        state.book.apply(side, row[0].0, row[1].0);
                    }
                }
            }
        }
        if !secondary
            && let Some(last) = cx.store.ticker(symbol).map(|t| t.last).filter(|v| *v > 0.0)
        {
            let bids = state
                .book
                .bids()
                .iter()
                .copied()
                .filter(|(p, _)| *p <= last)
                .collect::<Vec<_>>();
            let asks = state
                .book
                .asks()
                .iter()
                .copied()
                .filter(|(p, _)| *p >= last)
                .collect::<Vec<_>>();
            state.book.load_sorted(BookSide::Bids, bids);
            state.book.load_sorted(BookSide::Asks, asks);
        }
        cx.book_changed(symbol);
    }
}
impl Blofin {
    /// A delivered error (a stream-shape or malformed body) carries a response,
    /// so a managed read validates its custody first, as `on_response` does: a
    /// stale or superseded leg only closes its Context and changes nothing.
    fn delivered_failure(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Account(read, request) => {
                let current = cx
                    .validate_background_read_http_delivery(read.context())
                    .is_ok();
                let _ = cx.finish_http_context(read.context());
                if current {
                    self.account_failed(cx, request, error);
                }
            }
            Pending::History(mut request) => {
                if request.valid_delivery(cx) {
                    self.history_failed(cx, request, VenueError::new(error));
                } else {
                    request.release(cx);
                }
            }
            Pending::Closed(mut request) => {
                if request.valid_delivery(cx) {
                    Self::closed_failed(cx, request, VenueError::new(error));
                } else {
                    request.release(cx);
                }
            }
            other => self.on_request_failed(cx, other, error),
        }
    }
}

impl Venue for Blofin {
    fn managed_sockets() -> bool {
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
    fn socket_control(
        &self,
        _: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        _: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        self.classify_control(feed, text)
    }
    fn encode_socket_interests(
        &self,
        _: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        interests: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.encode_interests(feed, subscribe, interests, emit)
    }
    fn fence_uncertain_placements(&self) -> bool {
        true
    }

    fn guard_pending_placements(&self) -> bool {
        true
    }
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
            Pending::Trade(request) => trading::trade_reconciliation(request),
            _ => Err(safe_cex_engine::request_context::Error::Invalid),
        }
    }
    fn on_http_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &safe_cex_engine::request_context::Preflight,
    ) {
        self.trading_preflight_ready(cx, preflight);
    }
    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        plan: &safe_cex_engine::request_context::Plan,
    ) {
        self.trading_plan_ready(cx, plan);
    }
    fn on_http_owner_retired_preflight(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) -> Option<safe_cex_engine::request_context::Preflight> {
        self.trading_retired(owner)
    }
    /// An uncertain or absent-order seal: a fresh order snapshot reports
    /// through `orders_reconciled`.
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _symbols: Option<&[String]>) {
        self.reconcile_uncertain_orders(cx);
    }

    type Pending = Pending;
    fn export_venue_state(&self) -> Option<Vec<u8>> {
        Some(self.account.export_venue_state())
    }
    fn apply_venue_state(&mut self, cx: &mut Cx<'_, Pending>, state: &[u8]) {
        let ours = self.account.margin;
        if self.account.apply_venue_state(state) {
            self.hold_margin_over_source(cx, ours);
        } else {
            cx.venue_state_unreadable("Unreadable BloFin venue state from the data source");
        }
    }
    fn on_frame_ingested(&mut self, cx: &mut Cx<'_, Pending>) {
        self.reapply_held_margin(cx);
    }
    fn refresh_settings(&mut self, cx: &mut Cx<'_, Pending>, _symbol: Option<&str>) {
        // The existing reads: margin and position mode, and positions (leverage).
        if cx.frame_fed() || !self.signed() {
            return;
        }
        self.read_setting(cx, account::StartupRead::Margin);
        self.read_setting(cx, account::StartupRead::Mode);
        self.refresh_account(cx);
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            position_history: safe_cex_engine::PositionHistory::Blofin,
            closed_position_format: safe_cex_engine::ClosedPositionFormat::Blofin,
            fill_history_time: safe_cex_engine::FillHistoryTime::SecondsOrMilliseconds,
            quantity_units: safe_cex_engine::QuantityUnits::NativePlacement,
            cancel_all: safe_cex_engine::CancelAll::EachPlacedSymbol,
            leverage_query: safe_cex_engine::LeverageQuery::OnRead,
            account_margin_type: true,
            closed_positions: true,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        // Trusted setup drops an empty effective passphrase, and a Secondary
        // inherits the Primary's, so a missing one refuses before discovery or
        // entropy, naming the slot that lacks it.
        let identities = &self.options.identities;
        let primary_lacks = !self.options.public_data_only
            && identities
                .primary
                .as_ref()
                .is_some_and(|primary| primary.passphrase.is_none());
        let secondary_lacks = identities
            .secondary
            .as_ref()
            .is_some_and(|read| read.passphrase.is_none());
        if primary_lacks || secondary_lacks {
            let error = if primary_lacks {
                "BloFin requires a passphrase"
            } else {
                "Secondary read credentials require a passphrase"
            };
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            self.account.startup_rejected = true;
            cx.startup_failed(VenueError::new(error).kind(ErrorKind::InvalidInput));
            return;
        }
        if let Err(error) = self.register_sockets(cx) {
            Self::socket_notice(cx, error);
            return;
        }
        self.read(
            cx,
            "/api/v1/market/instruments",
            Pending::Markets {
                rows: vec![],
                malformed: HashSet::new(),
            },
        );
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.connection.socket == Some(socket) {
            self.private_message(cx, socket, text);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        if text == "pong" {
            return;
        }
        if text == "ping" {
            cx.send(socket, "pong");
            return;
        }
        let Ok(m) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if !m.event.is_empty() {
            if m.event == "error" {
                cx.notice(
                    Level::Error,
                    code::SUBSCRIPTION_REJECTED,
                    format!("BloFin socket error {}", m.code),
                );
            }
            return;
        }
        match m.arg.channel.as_ref() {
            "tickers" | "funding-rate" => {
                if let Ok(rows) =
                    serde_json::from_str::<Data<Vec<&serde_json::value::RawValue>>>(text)
                {
                    for raw in rows.data {
                        match serde_json::from_str::<Tick<'_>>(raw.get()) {
                            Ok(row) => self.ticker(cx, &row, m.arg.channel == "funding-rate"),
                            Err(_) => cx.notice(
                                Level::Error,
                                code::ROW_SKIPPED,
                                "Malformed BloFin ticker row; retaining known ticker",
                            ),
                        }
                    }
                }
            }
            "books" | "books5" => {
                if let Ok(rows) = serde_json::from_str::<Data<OneOrMany<Depth>>>(text)
                    && let Some(d) = rows.data.first()
                    && let Some(market) = cx.store.market_by_id(&m.arg.inst_id)
                {
                    let symbol = market.symbol.clone();
                    self.depth(cx, &symbol, &m, d);
                }
            }
            channel if channel.starts_with("candle") => {
                if let Ok(rows) = serde_json::from_str::<Data<OneOrMany<CandleWire>>>(text)
                    && let Some(c) = rows.data.first().and_then(CandleWire::candle)
                    && let Some(market) = cx.store.market_by_id(&m.arg.inst_id)
                {
                    let symbol = market.symbol.clone();
                    for ((s, i), state) in &mut self.candles {
                        if *s == symbol
                            && interval(*i).is_some_and(|bar| format!("candle{bar}") == channel)
                            && state.last != Some(c)
                        {
                            state.last = Some(c);
                            cx.candle(s, *i, c);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Managed(_, pending) => self.on_item(cx, pending, item),
            Pending::Trade(_) => {}
            Pending::PublicTrades(request) => self.public_trades_item(cx, request, item),
            Pending::History(request) => self.history_item(cx, request, item),
            Pending::Closed(request) => Self::closed_item(cx, request, item),
            Pending::Account(read, request) => {
                if cx
                    .validate_background_read_http_delivery(read.context())
                    .is_ok()
                {
                    self.account_item(cx, request, item);
                }
            }
            Pending::Leverage(_) => {}
            Pending::Markets { rows, malformed } => {
                match serde_json::from_str::<Instrument<'_>>(item) {
                    Ok(m)
                        if m.inst_type.eq_ignore_ascii_case("SWAP")
                            && m.contract_type.eq_ignore_ascii_case("linear") =>
                    {
                        if m.inst_id.is_empty()
                            || m.base_currency.is_empty()
                            || m.quote_currency.is_empty()
                            || [
                                m.contract_value.0,
                                m.min_size.0,
                                m.tick_size.0,
                                m.max_limit_size.0,
                                m.max_market_size.0,
                                m.max_leverage.0,
                            ]
                            .iter()
                            .any(|number| !number.is_finite() || *number <= 0.0)
                        {
                            safe_cex_engine::startup::skipped_row(cx);
                            if !m.inst_id.is_empty() {
                                malformed.insert(m.inst_id.into_owned());
                            }
                            return;
                        }
                        rows.push(Market {
                            id: m.inst_id.to_string(),
                            symbol: m.inst_id.trim_end_matches("-SWAP").replace('-', ""),
                            pretty_symbol: None,
                            base: m.base_currency.into_owned(),
                            quote: m.quote_currency.into_owned(),
                            active: m.state == "live",
                            precision: Precision {
                                amount: if m.lot_size.0 > 0.0 {
                                    m.lot_size.0
                                } else {
                                    m.min_size.0
                                },
                                price: m.tick_size.0,
                            },
                            limits: Limits {
                                amount: MinMax {
                                    min: m.min_size.0 * m.contract_value.0,
                                    max: m.max_limit_size.0.min(m.max_market_size.0),
                                },
                                leverage: MinMax {
                                    min: 1.0,
                                    max: m.max_leverage.0,
                                },
                                price: None,
                            },
                            contract_size: Some(m.contract_value.0),
                            is_expiring: None,
                            is_inverse: None,
                            is_spot: None,
                            is_perpetual: None,
                            is_read_only: None,
                            taker_fee: None,
                            maker_fee: None,
                        })
                    }
                    Ok(_) => {}
                    Err(_) => {
                        safe_cex_engine::startup::skipped_row(cx);
                        if let Ok(key) = serde_json::from_str::<InstrumentKey<'_>>(item)
                            && !key.inst_id.is_empty()
                            && key
                                .inst_type
                                .as_deref()
                                .is_none_or(|t| t.eq_ignore_ascii_case("SWAP"))
                            && key
                                .contract_type
                                .as_deref()
                                .is_none_or(|t| t.eq_ignore_ascii_case("linear"))
                        {
                            malformed.insert(key.inst_id.into_owned());
                        }
                    }
                }
            }
            Pending::Tickers { rows, failed } => match serde_json::from_str::<Tick<'_>>(item) {
                Ok(t) => {
                    if let Some(m) = cx.store.market_by_id(&t.inst_id) {
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            bid: t.bid_price.0,
                            ask: t.ask_price.0,
                            last: t.last.0,
                            mark: t.last.0,
                            index: t.last.0,
                            percentage: if t.open24h.0 > 0.0 {
                                round_usd((t.last.0 - t.open24h.0) / t.open24h.0 * 100.0)
                            } else {
                                0.0
                            },
                            volume: t.vol_currency24h.0,
                            quote_volume: t.vol24h.0,
                            ..Ticker::default()
                        });
                    } else {
                        *failed = true;
                        safe_cex_engine::startup::skipped_row(cx);
                    }
                }
                Err(_) => {
                    *failed = true;
                    safe_cex_engine::startup::skipped_row(cx);
                    cx.notice(
                        Level::Error,
                        code::ROW_SKIPPED,
                        "Malformed BloFin ticker row; omitted from initial tickers",
                    );
                }
            },
            Pending::Candles { rows, failed, .. } => {
                match serde_json::from_str::<Vec<Number>>(item)
                    .ok()
                    .and_then(|r| candle(&r))
                {
                    Some(c) => rows.push(c),
                    None => *failed = true,
                }
            }
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        match pending {
            Pending::PublicTrades(request) => self.public_trades_response(cx, request, response),
            Pending::History(request) => {
                self.history_response(cx, request, response);
            }
            Pending::Closed(request) => {
                self.closed_response(cx, request, response);
            }
            Pending::Account(read, request) => {
                let current = cx
                    .validate_background_read_http_delivery(read.context())
                    .is_ok();
                // Every terminal finishes the leg: a stale delivery and an answer
                // the revision drops alike.
                let _ = cx.finish_http_context(read.context());
                if current {
                    self.account_response(cx, request, response);
                }
            }
            Pending::Leverage(leverage) => self.leverage_response(cx, leverage, response),
            Pending::Trade(request) => {
                self.settle_retired_trade(cx);
                self.trade_response(cx, *request, response);
            }
            Pending::Managed(context, pending) => {
                self.public_response(cx, context, *pending, response)
            }
            other => self.on_request_failed(cx, other, "Unmanaged BloFin public response"),
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
        match pending {
            Pending::Managed(context, pending) => {
                let error =
                    if matches!(&*pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
                        safe_cex_engine::startup::public_response_error(status, None, None)
                    } else {
                        VenueError::new(error)
                    };
                self.public_failure(cx, context, *pending, error);
            }
            other => self.delivered_failure(cx, other, error),
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        match pending {
            Pending::Managed(context, pending) => {
                let error =
                    if matches!(&*pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
                        discovery_error(&response)
                    } else {
                        VenueError::new(format!(
                            "response did not contain the expected list: {}",
                            response.body
                        ))
                    };
                self.public_failure(cx, context, *pending, error);
            }
            other => self.delivered_failure(
                cx,
                other,
                &format!(
                    "response did not contain the expected list: {}",
                    response.body
                ),
            ),
        }
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::PublicTrades(request) => self.public_trades_failed(cx, request, error),
            Pending::Managed(context, pending) => self.public_failure(cx, context, *pending, error),
            Pending::Leverage(leverage) => self.leverage_failed(cx, leverage, error),
            Pending::Closed(request) => Self::closed_failed(cx, request, error),
            Pending::History(request) => self.history_refused(cx, request, error),
            Pending::Trade(request) => self.trade_call_failed(cx, *request, error, false),
            other => self.on_request_failed(cx, other, &error.message),
        }
    }
    /// A failed delivery has no body to validate. History and closed positions
    /// keep one leg in flight per query, so the failure belongs to that leg.
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::PublicTrades(request) => self.public_trades_failed(cx, request, error),
            // The shared Network/Timeout kind stays; its text is the native message.
            Pending::Leverage(leverage) => self.leverage_failed(cx, leverage, error),
            Pending::Closed(request) => Self::closed_failed(cx, request, error),
            Pending::History(request) => self.history_failed(cx, request, error),
            Pending::Trade(request) => self.trade_call_failed(cx, *request, error, true),
            other => self.on_request_failed(cx, other, &error.message),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        let pending = match pending {
            Pending::PublicTrades(request) => {
                self.public_trades_failed(cx, request, VenueError::new(error));
                return;
            }
            Pending::Managed(context, pending) => {
                self.public_failure(
                    cx,
                    context,
                    *pending,
                    VenueError::new(error).kind(ErrorKind::Network),
                );
                return;
            }
            Pending::History(request) => {
                self.history_failed(cx, request, VenueError::new(error));
                return;
            }
            Pending::Closed(request) => {
                Self::closed_failed(cx, request, VenueError::new(error));
                return;
            }
            Pending::Leverage(leverage) => {
                self.leverage_failed(cx, leverage, VenueError::new(error));
                return;
            }
            other => other,
        };
        if let Pending::Account(read, request) = pending {
            let _ = cx.finish_http_context(read.context());
            self.account_failed(cx, request, error);
            return;
        }
        // Candle failures carry their Managed Context and return above.
        if matches!(pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
            self.public_failed(
                cx,
                matches!(pending, Pending::Markets { .. }),
                VenueError::new(error).kind(ErrorKind::Network),
            );
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(read) = self.startup.on_timer(timer) {
            if read == DiscoveryRead::Markets {
                self.start(cx);
            } else {
                self.read(
                    cx,
                    "/api/v1/market/tickers",
                    Pending::Tickers {
                        rows: vec![],
                        failed: false,
                    },
                );
            }
            return;
        }
        self.settle_retired_trade(cx);
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
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => self.subscribe_book(cx, symbol),
            Command::UnsubscribeOrderBook { symbol } => self.unsubscribe_book(cx, symbol),
            Command::SubscribeCandles {
                symbol,
                interval: i,
            } => {
                if interval(i).is_none() {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        i.unsupported("BloFin"),
                    );
                } else {
                    self.subscribe_candles(cx, symbol, i);
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: i,
            } => self.unsubscribe_candles(cx, symbol, i),
            Command::FetchCandles {
                command: _,
                symbol,
                interval: i,
                limit,
                from_ms,
                to_ms,
            } => {
                let context = match cx.admit_http_command(Self::read_policy()) {
                    Ok(context) => context,
                    Err(_) => return,
                };
                if let Some(m) = cx.store.market(&symbol) {
                    let Some(bar) = interval(i) else {
                        let _ = cx.fail_context(
                            &context,
                            VenueError::new(i.unsupported("BloFin"))
                                .kind(ErrorKind::Unsupported)
                                .outcome(safe_cex_engine::Outcome::NotSent),
                        );
                        return;
                    };
                    let step = i.ms();
                    let end = to_ms.unwrap_or(cx.now);
                    let count = limit.filter(|n| *n > 0).unwrap_or(1440).min(1440);
                    let start = if limit.is_none() {
                        from_ms.unwrap_or(end - step * f64::from(count))
                    } else {
                        end - step * f64::from(count)
                    };
                    let path = format!(
                        "/api/v1/market/candles?instId={}&bar={}&before={}&after={}",
                        m.id,
                        bar,
                        num::format(start),
                        num::format(end)
                    );
                    self.managed_read(
                        cx,
                        &path,
                        context,
                        Pending::Candles {
                            rows: vec![],
                            failed: false,
                        },
                    );
                } else {
                    let _ = cx.fail_context(
                        &context,
                        VenueError::new(format!("Unknown market {symbol}"))
                            .kind(ErrorKind::InvalidInput)
                            .outcome(safe_cex_engine::Outcome::NotSent),
                    );
                }
            }
            Command::FetchLeverage { symbol, .. } => self.fetch_leverage(cx, symbol),
            Command::Query { command, query } => self.start_query(cx, command, query),
            other => self.trading_command(cx, other),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).map(|s| &s.book)
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
            format!("BloFin stops re-reading a catalogue with {after} malformed markets"),
        );
    }
    *stalls < 5
}
