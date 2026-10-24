//! Market discovery and one quote socket with reference-counted consumers.
use crate::{ApexOmni, Pending, public_wire as wire};
use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Candle, Interval, Limits, Market, MinMax, PositionSide, Precision, Ticker,
};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryEvidence, RetryPolicy};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::{
    Command, Cx, Method, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId, VenueError,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct ContractConfig {
    pub pair_id: u16,
    pub initial_margin_rate: f64,
    pub default_initial_margin_rate: f64,
    pub max_market_price_range: f64,
}
pub struct Candles {
    context: Context,
    symbol: String,
    url: String,
    retries: u8,
    signature_retried: bool,
}

pub enum Request {
    Markets {
        rows: Vec<Market>,
        contracts: HashMap<String, ContractConfig>,
        /// Native and display names of malformed market rows.
        skipped: Vec<(String, String)>,
    },
}
impl Request {
    pub(crate) fn reset(&mut self) {
        let Self::Markets {
            rows,
            contracts,
            skipped,
        } = self;
        rows.clear();
        contracts.clear();
        skipped.clear();
    }
}
pub struct Depth {
    pub book: Book,
    sequence: Option<u64>,
    count: u32,
}
#[derive(Default)]
struct CandleState {
    count: u32,
    last: Option<Candle>,
}
pub struct Public {
    pub books: HashMap<String, Depth>,
    candles: HashMap<(String, Interval), CandleState>,
    socket: Option<SocketId>,
    open: bool,
    ping_at: f64,
    seed_retry: safe_cex_engine::startup::Backoff<()>,
    unlisted: HashSet<String>,
    unlisted_reported: HashSet<String>,
    supervisor: Supervisor,
}
impl Default for Public {
    fn default() -> Self {
        Self {
            books: HashMap::new(),
            candles: HashMap::new(),
            socket: None,
            open: false,
            ping_at: 0.0,
            seed_retry: safe_cex_engine::startup::Backoff::default(),
            unlisted: HashSet::new(),
            unlisted_reported: HashSet::new(),
            supervisor: Supervisor::new(Config {
                connect_timeout_ms: 30_000.0,
                idle_timeout_ms: 30_000.0,
                ping_interval_ms: Some(10_000.0),
                backoff_base_ms: 1_000.0,
                backoff_cap_ms: 16_000.0,
                ..Config::default()
            }),
        }
    }
}
impl Public {
    pub(crate) fn has_unlisted(&self) -> bool {
        !self.unlisted.is_empty()
    }
    pub fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).map(|d| &d.book)
    }
    pub fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, d)| (s.as_str(), &d.book)))
    }
}
/// ApeX's native code and the interval in seconds.
pub(crate) fn interval(interval: Interval) -> Option<(&'static str, f64)> {
    use Interval::*;
    let native = match interval {
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
        _ => return None,
    };
    Some((native, interval.ms() / 1000.0))
}
fn valid_candle(c: &Candle) -> bool {
    c.timestamp >= 0.0
        && c.open > 0.0
        && c.close > 0.0
        && c.low > 0.0
        && c.high >= c.low
        && c.volume >= 0.0
}
impl ApexOmni {
    fn revalue_positions(cx: &mut Cx<'_, Pending>, symbol: &str, reference: f64) {
        if !reference.is_finite() || reference <= 0.0 {
            return;
        }
        if !cx
            .store
            .positions()
            .iter()
            .any(|p| p.symbol == symbol && p.contracts != 0.0)
        {
            return;
        }
        cx.store.update_positions(|p| {
            if p.symbol == symbol && p.contracts != 0.0 {
                let pnl = (reference - p.entry_price)
                    * p.contracts
                    * if p.side == PositionSide::Long {
                        1.0
                    } else {
                        -1.0
                    };
                if pnl.is_finite() {
                    p.unrealized_pnl = pnl;
                }
            }
        });
    }

    pub(crate) fn revalue_public_positions(&self, cx: &mut Cx<'_, Pending>) {
        // Shared quotes cannot establish a private snapshot or mark it loaded.
        if !cx.store.loaded().positions {
            return;
        }
        let references: Vec<_> = cx
            .store
            .tickers()
            .iter()
            .filter_map(|ticker| {
                // The direct mapper already folds an absent mark into index.
                // An explicit zero mark falls back to last, even with a live index.
                [ticker.mark, ticker.last]
                    .into_iter()
                    .find(|price| price.is_finite() && *price > 0.0)
                    .map(|price| (ticker.symbol.clone(), price))
            })
            .collect();
        for (symbol, reference) in references {
            Self::revalue_positions(cx, &symbol, reference);
        }
    }

    /// Account rows on a market dropped as malformed are invisible; say so once.
    pub(crate) fn note_unlisted(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, kind: &str) {
        if self.public.unlisted.contains(symbol)
            && self.public.unlisted_reported.insert(symbol.to_owned())
        {
            cx.notice(
                Level::Error,
                code::ROW_SKIPPED,
                format!(
                    "ApeX {kind} on {symbol} is hidden because its market metadata is malformed"
                ),
            );
        }
    }

    pub(crate) fn start_public(&mut self, cx: &mut Cx<'_, Pending>) {
        let request = Pending::Public(Request::Markets {
            rows: vec![],
            contracts: HashMap::new(),
            skipped: vec![],
        });
        if let Err(error) = self.signed(
            cx,
            crate::sign::Request::get("/v3/symbols", vec![]),
            request,
            Some(ArrayAt::Path(&[
                "data",
                "contractConfig",
                "perpetualContract",
            ])),
        ) {
            cx.notice(Level::Warn, code::REQUEST_FAILED, error);
            self.public.seed_retry.failed(cx, ());
        }
    }
    pub(crate) fn public_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        let Request::Markets {
            rows,
            contracts,
            skipped,
        } = request;
        {
            let Ok(raw) = serde_json::from_str::<wire::Contract<'_>>(item) else {
                safe_cex_engine::startup::skipped_row(cx);
                let key = serde_json::from_str::<wire::ContractKey<'_>>(item).unwrap_or_default();
                skipped.push((key.symbol.into_owned(), key.cross_symbol_name.into_owned()));
                return;
            };
            if !raw.enable_trade {
                return;
            }
            if raw.symbol.is_empty()
                || raw.cross_symbol_name.is_empty()
                || raw.base_token_id.is_empty()
                || raw.settle_asset_id.is_empty()
                || raw.step_size <= 0.0
                || raw.tick_size <= 0.0
                || raw.min_order_size < 0.0
                || raw.max_order_size < raw.min_order_size
                || raw.display_min_leverage <= 0.0
                || raw.display_max_leverage < raw.display_min_leverage
                || raw.l2_pair_id < 0.0
                || raw.l2_pair_id > u16::MAX as f64
                || raw.l2_pair_id.fract() != 0.0
                || raw.default_initial_margin_rate <= 0.0
            {
                safe_cex_engine::startup::skipped_row(cx);
                skipped.push((raw.symbol.into_owned(), raw.cross_symbol_name.into_owned()));
                return;
            }
            contracts.insert(
                raw.cross_symbol_name.to_string(),
                ContractConfig {
                    pair_id: raw.l2_pair_id as u16,
                    initial_margin_rate: raw.initial_margin_rate,
                    default_initial_margin_rate: raw.default_initial_margin_rate,
                    max_market_price_range: raw.max_market_price_range,
                },
            );
            rows.push(Market {
                id: raw.symbol.into_owned(),
                symbol: raw.cross_symbol_name.into_owned(),
                base: raw.base_token_id.into_owned(),
                quote: raw.settle_asset_id.into_owned(),
                active: true,
                pretty_symbol: None,
                precision: Precision {
                    amount: raw.step_size,
                    price: raw.tick_size,
                },
                limits: Limits {
                    amount: MinMax {
                        min: raw.min_order_size,
                        max: raw.max_order_size,
                    },
                    leverage: MinMax {
                        min: raw.display_min_leverage,
                        max: raw.display_max_leverage,
                    },
                    price: None,
                },
                contract_size: None,
                is_expiring: Some(false),
                is_inverse: None,
                is_spot: None,
                is_perpetual: None,
                is_read_only: None,
                taker_fee: None,
                maker_fee: None,
            });
        }
    }
    pub(crate) fn public_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let envelope = serde_json::from_str::<wire::Envelope<'_>>(response.body).ok();
        let success = (200..300).contains(&response.status)
            && envelope
                .as_ref()
                .is_some_and(|e| e.code.unwrap_or(0.0) == 0.0 && e.data.is_some());
        match request {
            Request::Markets {
                rows,
                contracts,
                skipped,
            } => {
                if !success || rows.is_empty() {
                    self.public_failed(
                        cx,
                        Request::Markets {
                            rows,
                            contracts,
                            skipped,
                        },
                        "invalid market response",
                    );
                    return;
                }
                if !skipped.is_empty() {
                    let labels = skipped
                        .iter()
                        .map(|(id, name)| match (id.as_str(), name.as_str()) {
                            ("", "") => "unidentified",
                            ("", name) => name,
                            (id, _) => id,
                        })
                        .collect::<Vec<_>>();
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        format!("ApeX skipped malformed market rows ({})", labels.join(", ")),
                    );
                }
                self.public.unlisted = skipped
                    .into_iter()
                    .flat_map(|(id, name)| [id, name])
                    .filter(|name| !name.is_empty())
                    .collect();
                let tickers = rows
                    .iter()
                    .map(|m| Ticker {
                        id: m.id.clone(),
                        symbol: m.symbol.clone(),
                        ..Default::default()
                    })
                    .collect();
                self.public.seed_retry.succeeded(cx, &());
                self.contracts = contracts;
                cx.store.set_markets(rows);
                cx.store.set_tickers(tickers);
                self.connect_public(cx);
                self.start_private(cx);
            }
        }
    }
    pub(crate) fn public_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        match request {
            Request::Markets { .. } => {
                cx.notice(
                    Level::Warn,
                    code::REQUEST_FAILED,
                    format!("ApeX markets: {error}"),
                );
                self.public.seed_retry.failed(cx, ());
            }
        }
    }
    pub(crate) fn candle_failed(cx: &mut Cx<'_, Pending>, request: Candles, error: VenueError) {
        let _ = cx.fail_context(&request.context, error);
    }

    fn queue_candles(&mut self, cx: &mut Cx<'_, Pending>, request: Candles) {
        if let Err((pending, error)) = cx.request_managed(
            request.context.clone(),
            None,
            Method::Get,
            request.url.clone(),
            None,
            Pending::Candles(request),
        ) {
            self.failed(cx, pending, error.venue_error());
        }
    }

    fn start_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        params: Vec<(String, String)>,
    ) {
        let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::ReadFailure,
        }) else {
            return;
        };
        let url = format!("{}/v3/klines?{}", self.rest, crate::sign::query(&params));
        self.queue_candles(
            cx,
            Candles {
                context,
                symbol,
                url,
                retries: 0,
                signature_retried: false,
            },
        );
    }

    fn retry_candles(&mut self, cx: &mut Cx<'_, Pending>, mut request: Candles) {
        match cx.http_retry_context(&request.context, RetryEvidence::ReadFailure) {
            Ok(context) => {
                request.context = context;
                self.queue_candles(cx, request);
            }
            Err(error) => Self::candle_failed(cx, request, error.venue_error()),
        }
    }

    pub(crate) fn candle_transport_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Candles,
        error: VenueError,
    ) {
        if let Err(error) = cx.validate_http_context(&request.context) {
            Self::candle_failed(cx, request, error.venue_error());
        } else if request.retries < 3 {
            request.retries += 1;
            self.retry_candles(cx, request);
        } else {
            Self::candle_failed(cx, request, error);
        }
    }

    pub(crate) fn candle_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Candles,
        response: Response<'_>,
    ) {
        if cx
            .validate_read_http_delivery(&request.context, &request.context)
            .is_err()
        {
            return;
        }
        match crate::api_response(response) {
            crate::ApiResponse::SignatureRefusal(_) if !request.signature_retried => {
                request.signature_retried = true;
                self.retry_candles(cx, request);
                return;
            }
            crate::ApiResponse::Failed(error) | crate::ApiResponse::SignatureRefusal(error) => {
                // Public reads retain the legacy message-only native error contract.
                Self::candle_failed(cx, request, VenueError::new(error.message));
                return;
            }
            crate::ApiResponse::Success => {}
        }
        let Candles {
            context, symbol, ..
        } = request;
        let envelope = serde_json::from_str::<wire::Envelope<'_>>(response.body).ok();
        let success = (200..300).contains(&response.status)
            && envelope
                .as_ref()
                .is_some_and(|e| e.code.unwrap_or(0.0) == 0.0 && e.data.is_some());
        if !success {
            let _ = cx.fail_context(&context, "ApeX candle request failed");
            return;
        }
        let rows = envelope
            .and_then(|e| e.data)
            .and_then(|data| {
                serde_json::from_str::<HashMap<String, Vec<wire::Kline>>>(data.get()).ok()
            })
            .and_then(|mut rows| rows.remove(&symbol));
        let Some(rows) = rows else {
            let _ = cx.fail_context(&context, "Malformed ApeX candle response");
            return;
        };
        if rows.iter().any(|row| row.t < 0.0 || row.t.fract() != 0.0) {
            let _ = cx.fail_context(&context, "Invalid ApeX candle timestamp");
            return;
        }
        let received = rows.len();
        let mut rows: Vec<_> = rows
            .into_iter()
            .map(|r| Candle {
                timestamp: r.t / 1000.0,
                open: r.o,
                high: r.h,
                low: r.l,
                close: r.c,
                volume: r.v,
            })
            .collect();
        rows.retain(|row| {
            if valid_candle(row) {
                return true;
            }
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                format!("ApeX skipped invalid {symbol} candle at {}", row.timestamp),
            );
            false
        });
        if received > 0 && rows.is_empty() {
            let _ = cx.fail_context(&context, "No usable ApeX candles");
            return;
        }
        rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
        {
            let result: Result<String, String> =
                serde_json::to_string(&rows).map_err(|e| e.to_string());
            match result {
                Ok(data) => {
                    let _ = cx.done_context(&context, data);
                }
                Err(error) => {
                    let _ = cx.fail_context(&context, error);
                }
            }
        };
    }

    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        let host = if self.options.testnet {
            "qa-quote.omni.apex.exchange"
        } else {
            "quote.omni.apex.exchange"
        };
        // Suppressed public feeds never open; supervising them would only churn retries.
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public.supervisor.connecting(cx);
        }
        self.public.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new(format!(
                    "wss://{host}/realtime_public?v=2&timestamp={}&requestId=rust-{}",
                    cx.now as u64, cx.now as u64
                ))
                .channel(SocketChannel::Public),
            ),
        );
    }
    fn public_topics(&self) -> Vec<String> {
        let mut topics = vec!["instrumentInfo.all".to_owned()];
        topics.extend(
            self.public
                .books
                .keys()
                .map(|s| format!("orderBook200.H.{s}")),
        );
        let mut trades = HashSet::new();
        for (s, i) in self.public.candles.keys() {
            if let Some((i, _)) = interval(*i) {
                topics.push(format!("candle.{i}.{s}"));
            }
            if !self.options.public_data_only {
                trades.insert(format!("recentlyTrade.H.{s}"));
            }
        }
        topics.extend(trades);
        topics.sort();
        topics
    }
    fn public_subscribe(&self, cx: &mut Cx<'_, Pending>, op: &str, args: &[String]) {
        if !self.public.open {
            return;
        }
        let Some(socket) = self.public.socket else {
            return;
        };
        for args in args.chunks(50) {
            cx.send(
                socket,
                serde_json::to_string(&Subscription { op, args }).expect("string subscription"),
            );
        }
    }
    pub(crate) fn public_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.public.socket != Some(socket) {
            return false;
        }
        self.public.open = true;
        self.public.supervisor.opened(cx);
        self.public_subscribe(cx, "subscribe", &self.public_topics());
        cx.send(socket, r#"{"op":"ping"}"#);
        self.public.ping_at = cx.now;
        true
    }
    pub(crate) fn public_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.public.socket != Some(socket) {
            return false;
        }
        self.public.socket = None;
        self.public.open = false;
        for (symbol, depth) in &mut self.public.books {
            if !depth.book.is_empty() {
                depth.book.clear();
                cx.book_changed(symbol);
            }
            depth.sequence = None;
        }
        self.public.supervisor.retry(cx);
        true
    }
    pub(crate) fn public_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.public.seed_retry.on_timer(timer).is_some() {
            self.start_public(cx);
            return true;
        }
        match self.public.supervisor.on_timer(cx, timer) {
            Tick::Unhandled => return false,
            Tick::Reconnect => self.connect_public(cx),
            Tick::Ping => {
                if let Some(socket) = self.public.socket {
                    cx.send(socket, r#"{"op":"ping"}"#);
                    self.public.ping_at = cx.now;
                }
            }
            Tick::TimedOut(_) => {
                if let Some(socket) = self.public.socket {
                    cx.close(socket);
                    self.public_close(cx, socket);
                }
            }
            Tick::Handled => (),
        }
        true
    }
    pub(crate) fn public_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.public.socket != Some(socket) {
            return false;
        }
        self.public.supervisor.received(cx.now);
        let Ok(message) = serde_json::from_str::<wire::Message<'_>>(text) else {
            return true;
        };
        if message.op == "ping" {
            cx.send(
                socket,
                format!(r#"{{"op":"pong","args":["{}"]}}"#, cx.now as u64),
            );
            return true;
        }
        if message.op == "pong" || (message.topic.is_empty() && text.contains("pong")) {
            cx.store
                .set_latency(((cx.now - self.public.ping_at).max(0.0) / 2.0).round());
            return true;
        }
        let Some(data) = message.data else {
            return true;
        };
        if message.topic == "instrumentInfo.all" {
            if let Ok(rows) = serde_json::from_str::<Vec<wire::Tick<'_>>>(data.get()) {
                for row in rows {
                    let symbol = cx
                        .store
                        .market(&row.s)
                        .map(|m| m.symbol.clone())
                        .or_else(|| cx.store.market_by_id(&row.s).map(|m| m.symbol.clone()));
                    let Some(symbol) = symbol else {
                        continue;
                    };
                    cx.store.update_ticker(&symbol, |t| {
                        if let Some(n) = row.p {
                            t.last = n;
                        }
                        if let Some(n) = row.mp.or(row.xp) {
                            t.mark = n;
                        }
                        if let Some(n) = row.xp {
                            t.index = n;
                        }
                        if let Some(n) = row.pr {
                            t.percentage = n * 100.0;
                        }
                        if let Some(n) = row.o {
                            t.open_interest = n;
                        }
                        if let Some(n) = row.fr {
                            t.funding_rate = n;
                        }
                        if let Some(n) = row.v {
                            t.volume = n;
                        }
                        if let Some(n) = row.turnover {
                            t.quote_volume = n;
                        }
                    });
                    if let Some(reference) = row
                        .mp
                        .or(row.xp)
                        .filter(|n| *n != 0.0)
                        .or(row.p)
                        .filter(|n| *n != 0.0)
                    {
                        Self::revalue_positions(cx, &symbol, reference);
                    }
                }
            }
        } else if let Some(symbol) = message.topic.strip_prefix("orderBook200.H.") {
            let Some(depth) = self.public.books.get_mut(symbol) else {
                return true;
            };
            let Ok(rows) = serde_json::from_str::<wire::Depth>(data.get()) else {
                return true;
            };
            if !rows
                .a
                .iter()
                .chain(&rows.b)
                .all(|p| p.0 > 0.0 && p.1 >= 0.0)
            {
                return true;
            }
            if message.kind == "snapshot" {
                depth.book.clear();
            } else if message.kind == "delta" {
                let Some(previous) = depth.sequence else {
                    return true;
                };
                if rows.u <= previous {
                    return true;
                }
                if previous.checked_add(1) != Some(rows.u) {
                    depth.book.clear();
                    depth.sequence = None;
                    cx.book_changed(symbol);
                    self.public_subscribe(cx, "unsubscribe", &[message.topic.to_string()]);
                    self.public_subscribe(cx, "subscribe", &[message.topic.to_string()]);
                    return true;
                }
            } else {
                return true;
            }
            depth.sequence = Some(rows.u);
            for (side, rows) in [(BookSide::Asks, rows.a), (BookSide::Bids, rows.b)] {
                for wire::Level(price, amount) in rows {
                    depth.book.apply(side, price, amount);
                }
            }
            cx.book_changed(symbol);
        } else if let Some(topic) = message.topic.strip_prefix("candle.") {
            let Some((native, symbol)) = topic.split_once('.') else {
                return true;
            };
            let Ok(rows) = serde_json::from_str::<Vec<wire::Candle>>(data.get()) else {
                return true;
            };
            let Some(row) = rows.into_iter().next() else {
                return true;
            };
            let candle = Candle {
                timestamp: row.start / 1000.0,
                open: row.open,
                high: row.high,
                low: row.low,
                close: row.close,
                volume: row.volume,
            };
            if !valid_candle(&candle) {
                return true;
            }
            for ((s, i), state) in &mut self.public.candles {
                if s == symbol
                    && interval(*i).is_some_and(|(n, _)| n == native)
                    && state
                        .last
                        .as_ref()
                        .is_none_or(|c| c.timestamp <= candle.timestamp)
                {
                    state.last = Some(candle);
                    cx.candle(s, *i, candle);
                }
            }
        } else if let Some(symbol) = message.topic.strip_prefix("recentlyTrade.H.") {
            if self.options.public_data_only {
                return true;
            }
            let Ok(rows) = serde_json::from_str::<Vec<wire::Trade>>(data.get()) else {
                return true;
            };
            let Some(row) = rows.into_iter().next() else {
                return true;
            };
            if row.p.value <= 0.0 || row.v.value < 0.0 {
                return true;
            }
            let decimals = row.v.scale.max(row.p.scale).max(8);
            let factor = 10f64.powi(decimals as i32);
            let volume =
                safe_cex_engine::num::round_half_up(row.v.value / row.p.value * factor) / factor;
            if !volume.is_finite() {
                return true;
            }
            for ((s, i), state) in &mut self.public.candles {
                if s != symbol {
                    continue;
                }
                let Some((_, seconds)) = interval(*i) else {
                    continue;
                };
                let Some(last) = state.last.as_mut() else {
                    continue;
                };
                let time = (row.time / 1000.0 / seconds).floor() * seconds;
                if time < last.timestamp {
                    continue;
                }
                if time > last.timestamp {
                    *last = Candle {
                        timestamp: time,
                        open: last.close,
                        high: last.close,
                        low: last.close,
                        close: last.close,
                        volume: 0.0,
                    };
                }
                last.high = last.high.max(row.p.value);
                last.low = last.low.min(row.p.value);
                last.close = row.p.value;
                let factor = 10f64.powi(
                    safe_cex_engine::num::decimals(last.volume)
                        .max(safe_cex_engine::num::decimals(volume))
                        .min(308) as i32,
                );
                last.volume =
                    safe_cex_engine::num::round_half_up((last.volume + volume) * factor) / factor;
                cx.candle(s, *i, *last);
            }
        }
        true
    }
    pub(crate) fn public_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Result<(), Command> {
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => {
                let depth = self
                    .public
                    .books
                    .entry(symbol.clone())
                    .or_insert_with(|| Depth {
                        count: 0,
                        book: Book::default(),
                        sequence: None,
                    });
                depth.count += 1;
                if depth.count == 1 {
                    self.public_subscribe(cx, "subscribe", &[format!("orderBook200.H.{symbol}")]);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(depth) = self.public.books.get_mut(&symbol) {
                    depth.count = depth.count.saturating_sub(1);
                    if depth.count == 0 {
                        self.public.books.remove(&symbol);
                        self.public_subscribe(
                            cx,
                            "unsubscribe",
                            &[format!("orderBook200.H.{symbol}")],
                        );
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: i,
            } => {
                let Some((native, _)) = interval(i) else {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        i.unsupported("ApeX"),
                    );
                    return Ok(());
                };
                let was_trades = self.public.candles.keys().any(|(s, _)| s == &symbol);
                let state = self.public.candles.entry((symbol.clone(), i)).or_default();
                state.count += 1;
                if state.count == 1 {
                    self.public_subscribe(cx, "subscribe", &[format!("candle.{native}.{symbol}")]);
                    if !self.options.public_data_only && !was_trades {
                        self.public_subscribe(
                            cx,
                            "subscribe",
                            &[format!("recentlyTrade.H.{symbol}")],
                        );
                    }
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: i,
            } => {
                if let Some(state) = self.public.candles.get_mut(&(symbol.clone(), i)) {
                    state.count = state.count.saturating_sub(1);
                    if state.count == 0 {
                        self.public.candles.remove(&(symbol.clone(), i));
                        if let Some((native, _)) = interval(i) {
                            self.public_subscribe(
                                cx,
                                "unsubscribe",
                                &[format!("candle.{native}.{symbol}")],
                            );
                        }
                        if !self.options.public_data_only
                            && !self.public.candles.keys().any(|(s, _)| s == &symbol)
                        {
                            self.public_subscribe(
                                cx,
                                "unsubscribe",
                                &[format!("recentlyTrade.H.{symbol}")],
                            );
                        }
                    }
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval: i,
                limit,
                from_ms,
                to_ms,
            } => {
                let Some((native, seconds)) = interval(i) else {
                    cx.fail(command, i.unsupported("ApeX"));
                    return Ok(());
                };
                let limit = limit.filter(|n| *n > 0);
                let count = limit.unwrap_or(500).min(1500) as f64;
                let end = to_ms.filter(|n| n.is_finite()).unwrap_or(cx.now) / 1000.0;
                let start = if limit.is_none() {
                    from_ms
                        .filter(|n| n.is_finite())
                        .map(|n| n / 1000.0)
                        .unwrap_or(end - seconds * count)
                } else {
                    end - seconds * count
                };
                let mut params = vec![
                    ("symbol".into(), symbol.clone()),
                    ("start".into(), (start.floor() as i64).to_string()),
                    ("interval".into(), native.into()),
                ];
                if native != "D" {
                    params.push(("end".into(), (end.floor() as i64).to_string()));
                }
                self.start_candles(cx, symbol, params);
            }
            other => return Err(other),
        }
        Ok(())
    }
}
#[derive(serde::Serialize)]
struct Subscription<'a> {
    op: &'a str,
    args: &'a [String],
}
