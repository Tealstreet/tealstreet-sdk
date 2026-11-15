//! Bitget perpetual futures: a deterministic, sans-I/O venue engine.
mod account;
mod book;
mod history;
mod identity;
mod leverage;
mod paths;
mod protective;
mod sign;
mod telemetry;
mod trading;
mod venue_state;
mod wire;

use book::BookState;
use safe_cex_engine::Level;
use safe_cex_engine::book::Book;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Context, Credential, Operation, Policy, RetryPolicy};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::startup::{DiscoveryRead, PublicRetries};
use safe_cex_engine::{
    Command, Cx, Method, PublicFeedPolicy, Response, SocketId, SubscriptionTopic, TimerId, Venue,
    num,
};
use serde::{Serialize, de::IgnoredAny};
use std::collections::HashMap;
use wire::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Product {
    #[default]
    Usdt,
    Usdc,
    Inverse,
}

#[derive(Clone, Default, Debug)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::signing::Identity>,
    pub testnet: bool,
    pub public_data_only: bool,
    pub credentials: Option<safe_cex_engine::signing::Identity>,
    pub product: Product,
}

pub enum Pending {
    Identity(Context),
    Leverage(leverage::Read),
    History(Box<history::Request>),
    Trade(Box<trading::Request>),
    Account {
        generation: u64,
        context: Context,
        request: account::Request,
    },
    /// A contracts or tickers read under its own Background Context.
    Discovery {
        context: Context,
        rows: Discovered,
    },
    Candles {
        root: Context,
        rows: Vec<Candle>,
        invalid: bool,
    },
}

impl Pending {
    fn reads_account_list(&self) -> bool {
        matches!(
            self,
            Self::Account {
                request: account::Request::Orders(_) | account::Request::Positions { .. },
                ..
            }
        )
    }
}

pub enum Discovered {
    Markets(Vec<Market>, HashMap<String, (f64, f64)>),
    Tickers(Vec<Ticker>),
}

const DISCOVERY_HEADERS: &[(&str, &str)] = &[("X-CHANNEL-API-CODE", "Tealstreet")];

fn discovery_context(pending: &Pending) -> Option<&Context> {
    match pending {
        Pending::Discovery { context, .. } => Some(context),
        _ => None,
    }
}

/// A discovery read decodes and publishes only its own live Background delivery.
fn discovery_delivered(cx: &Cx<'_, Pending>, pending: &Pending) -> bool {
    discovery_context(pending)
        .is_none_or(|context| cx.validate_background_read_http_delivery(context).is_ok())
}

/// A failure, including the read's own deadline, always retires its Context.
fn retire_discovery(cx: &mut Cx<'_, Pending>, pending: &Pending) {
    if let Some(context) = discovery_context(pending) {
        let _ = cx.finish_http_context(context);
    }
}

// The private connection (index 3) is supervised by `account.supervisor`.
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    ping_at: f64,
    supervisor: Supervisor,
}

impl Default for Connection {
    fn default() -> Self {
        Self {
            socket: None,
            open: false,
            ping_at: 0.0,
            supervisor: Supervisor::new(Config {
                connect_timeout_ms: 20_000.0,
                idle_timeout_ms: 20_000.0,
                ping_interval_ms: Some(5_000.0),
                ..Config::default()
            }),
        }
    }
}

pub struct BitgetFutures {
    options: Options,
    discovery: PublicRetries,
    account: account::Account,
    trading: trading::Trading,
    telemetry: telemetry::Telemetry,
    history: history::History,
    sockets: [Connection; 4],
    ticker_index: Vec<Option<u32>>,
    live_tickers: safe_cex_engine::startup::LiveTickers,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, Interval), usize>,
    /// Topics Bitget answered with event:error, by (channel, instId): retries made, pending timer.
    refused_topics: HashMap<(String, String), (u32, Option<TimerId>)>,
    /// When an unreadable public message was last logged; at most one per minute.
    public_parse_logged_at: Option<f64>,
    order_caps: HashMap<String, OrderCaps>,
}

#[derive(Clone, Copy)]
struct OrderCaps {
    limit: f64,
    market: f64,
}

impl OrderCaps {
    fn from_wire(row: &Contract<'_>, inverse: bool) -> Self {
        let limit = if inverse {
            row.max_trade_num.0.filter(|n| *n > 0.0)
        } else {
            [
                row.max_order_qty,
                row.max_market_order_qty,
                row.max_trade_num,
            ]
            .iter()
            .find_map(|n| n.0.filter(|n| *n > 0.0))
        }
        .unwrap_or(f64::INFINITY);
        Self {
            limit,
            market: if inverse {
                limit
            } else {
                row.max_market_order_qty
                    .0
                    .filter(|n| *n > 0.0)
                    .unwrap_or(limit)
            },
        }
    }

    fn shared(self) -> f64 {
        self.limit.min(self.market)
    }

    fn for_order(self, shared: f64, market: bool) -> f64 {
        // Expand only our own conservative publication for limits. A tighter
        // store override or source frame remains a bound for either kind.
        if market {
            shared.min(self.shared())
        } else if shared == self.shared() {
            self.limit
        } else {
            shared
        }
    }
}

/// A refused subscription is sent again after 1 s, 2 s and 4 s, then left refused.
const SUBSCRIBE_RETRIES: u32 = 3;
const PUBLIC_PARSE_LOG_MS: f64 = 60_000.0;

fn discovery_error(response: &Response<'_>) -> safe_cex_engine::VenueError {
    let envelope = serde_json::from_str::<Envelope<IgnoredAny>>(response.body).ok();
    let rejection = envelope
        .as_ref()
        .filter(|e| e.code.0.is_some_and(|code| code != 0.0));
    let code = rejection.map(|e| e.code.get().to_string());
    safe_cex_engine::startup::public_response_error(
        response.status,
        code.as_deref(),
        rejection.map(|e| e.msg.as_str()),
    )
}

/// A private send's body, or the array its streamed response decodes.
impl BitgetFutures {
    fn public_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        tickers: bool,
        error: safe_cex_engine::VenueError,
    ) {
        self.discovery.failed_after(
            cx,
            if tickers {
                DiscoveryRead::Tickers
            } else {
                DiscoveryRead::Markets
            },
            0.0,
            error,
        );
    }

    /// Retires a discovery read's Context, then reports it on the native backoff.
    #[inline(never)]
    fn discovery_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: &Pending,
        error: safe_cex_engine::VenueError,
    ) {
        retire_discovery(cx, pending);
        let tickers = matches!(
            pending,
            Pending::Discovery {
                rows: Discovered::Tickers(_),
                ..
            }
        );
        self.public_failed(cx, tickers, error);
    }

    pub fn new(options: Options) -> Self {
        Self {
            options,
            discovery: Default::default(),
            account: account::Account::new(),
            trading: trading::Trading::default(),
            telemetry: telemetry::Telemetry::default(),
            history: history::History::default(),
            sockets: Default::default(),
            ticker_index: Vec::new(),
            live_tickers: Default::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            refused_topics: HashMap::new(),
            public_parse_logged_at: None,
            order_caps: HashMap::new(),
        }
    }

    /// One Background managed Read per discovery read; a refusal is a prompt
    /// pre-send failure so the native backoff applies.
    fn discovery_read(&mut self, cx: &mut Cx<'_, Pending>, tickers: bool) {
        if tickers {
            self.live_tickers.begin();
        }
        let scope = safe_cex_engine::request_context::Scope::Background;
        let issue = if tickers {
            Cx::http_live_context
        } else {
            Cx::http_context
        };
        let context = match issue(cx, scope, read_policy(None)) {
            Ok(context) => context,
            Err(reason) => {
                let error = reason.read_venue_error();
                cx.startup_admission_refused(
                    if tickers {
                        safe_cex_engine::startup::Sections::TICKERS
                    } else {
                        safe_cex_engine::startup::Sections::MARKETS
                    },
                    &error,
                );
                return self.public_failed(cx, tickers, error);
            }
        };
        let path = if tickers {
            paths::TICKERS
        } else {
            paths::CONTRACTS
        };
        let url = format!("{}{path}?productType={}", paths::REST, self.product());
        let pending = Pending::Discovery {
            context: context.clone(),
            rows: if tickers {
                Discovered::Tickers(vec![])
            } else {
                Discovered::Markets(vec![], HashMap::new())
            },
        };
        match cx.request_managed_with_headers(
            context,
            None,
            Method::Get,
            url,
            DISCOVERY_HEADERS,
            None,
            pending,
        ) {
            Ok(id) => cx.stream_response(id, ArrayAt::Envelope(&[&["data"]])),
            Err((pending, reason)) => {
                self.discovery_failed(cx, &pending, reason.read_venue_error())
            }
        }
    }
    fn product(&self) -> &'static str {
        match (self.options.product, self.options.testnet) {
            (Product::Usdt, false) => "usdt-futures",
            (Product::Usdt, true) => "susdt-futures",
            (Product::Usdc, false) => "usdc-futures",
            (Product::Usdc, true) => "susdc-futures",
            (Product::Inverse, false) => "coin-futures",
            (Product::Inverse, true) => "scoin-futures",
        }
    }

    pub(crate) fn coin(&self) -> &'static str {
        match (self.options.product, self.options.testnet) {
            (Product::Usdt, false) => "USDT",
            (Product::Usdt, true) => "SUSDT",
            (Product::Usdc, false) => "USDC",
            (Product::Usdc, true) => "SUSDC",
            (Product::Inverse, _) => "BTC",
        }
    }

    pub(crate) fn inverse(&self) -> bool {
        self.options.product == Product::Inverse
    }

    fn quote(&self) -> &'static str {
        if self.inverse() { "USD" } else { self.coin() }
    }

    pub(crate) fn margin_coin<'a>(
        &self,
        cx: &'a Cx<'_, Pending>,
        symbol: &str,
    ) -> Result<&'a str, String> {
        if self.inverse() {
            cx.store
                .market(symbol)
                .map(|m| m.base.as_str())
                .ok_or_else(|| format!("unknown Bitget market {symbol}"))
        } else {
            Ok(self.coin())
        }
    }

    fn connect(&mut self, cx: &mut Cx<'_, Pending>, index: usize) {
        if index == 3
            && (self.account.startup_rejected
                || self.account.route_exhausted
                || self.account.supervisor.is_waiting())
        {
            return;
        }
        let url = if index == 3 {
            if self.account.uta {
                paths::UTA_PRIVATE
            } else {
                paths::PRIVATE
            }
        } else {
            paths::PUBLIC
        };
        let recover =
            index == 3 && std::mem::take(&mut self.account.needs_recovery) && !self.account.uta;
        // Composes with `recover`: the host leaves the route, recovery still runs.
        let reject_route = index == 3 && std::mem::take(&mut self.account.reject_route);
        let connection = &mut self.sockets[index];
        if let Some(socket) = connection.socket.take() {
            cx.close(socket);
        }
        let mut spec = safe_cex_engine::SocketSpec::new(url)
            .channel(if index == 3 {
                safe_cex_engine::SocketChannel::Private
            } else {
                safe_cex_engine::SocketChannel::Public
            })
            .feed(
                ["primary", "secondary-1", "secondary-2", "private"][index],
                recover,
            );
        if reject_route {
            spec = spec.reject_route();
        }
        connection.socket = Some(cx.connect(spec));
        connection.open = false;
        if index == 3 {
            self.account.authenticated = false;
            self.account.supervisor.connecting(cx);
        } else {
            // Suppressed public feeds never open; supervising them would only churn retries.
            if cx.public_feed_policy() == PublicFeedPolicy::Direct {
                connection.supervisor.connecting(cx);
            }
        }
    }

    fn open_feed(&mut self, cx: &mut Cx<'_, Pending>, index: usize) {
        self.connect(cx, index);
    }

    fn channel(&self, socket: SocketId) -> Option<usize> {
        self.sockets.iter().position(|c| c.socket == Some(socket))
    }

    fn subscribe(&self, cx: &mut Cx<'_, Pending>, op: &str, topics: &[(String, String)]) {
        if self.sockets[0].open
            && let Some(socket) = self.sockets[0].socket
        {
            self.send_topics(cx, socket, op, topics);
        }
    }

    /// Re-sends one live book or candle channel on the public socket; counts,
    /// refused-topic retries and reconnect resubscription are left untouched.
    fn repair(&self, cx: &mut Cx<'_, Pending>, topic: &SubscriptionTopic) -> Result<(), String> {
        let live = match topic {
            SubscriptionTopic::Candles { symbol, interval } => self
                .candles
                .contains_key(&(symbol.clone(), *interval))
                .then(|| candle_topic(symbol.clone(), *interval))
                .flatten(),
            SubscriptionTopic::OrderBook { symbol } => self
                .books
                .contains_key(symbol)
                .then(|| book_topic(symbol.clone())),
        };
        let Some(native) = live else {
            return Err(format!("{topic} is not subscribed"));
        };
        let connection = &self.sockets[0];
        let Some(socket) = connection.socket.filter(|_| connection.open) else {
            return Err(format!(
                "the public socket carrying {} {} is not open",
                native.0, native.1
            ));
        };
        let topics = [native];
        self.send_topics(cx, socket, "unsubscribe", &topics);
        self.send_topics(cx, socket, "subscribe", &topics);
        Ok(())
    }

    fn send_topics(
        &self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        op: &str,
        topics: &[(String, String)],
    ) {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Topic<'a> {
            inst_type: &'a str,
            channel: &'a str,
            inst_id: &'a str,
        }
        #[derive(Serialize)]
        struct Subscription<'a> {
            op: &'a str,
            args: Vec<Topic<'a>>,
        }
        let product = self.product().to_uppercase();
        for chunk in topics.chunks(100) {
            let args = chunk
                .iter()
                .map(|(channel, symbol)| Topic {
                    inst_type: &product,
                    channel,
                    inst_id: symbol,
                })
                .collect();
            cx.send(
                socket,
                serde_json::to_string(&Subscription { op, args }).expect("subscription"),
            );
        }
    }

    fn disconnected(&mut self, cx: &mut Cx<'_, Pending>, index: usize) {
        let connection = &mut self.sockets[index];
        let opened = std::mem::replace(&mut connection.open, false);
        connection.socket = None;
        if index == 3 {
            self.private_closed(cx, opened);
            self.account.authenticated = false;
            self.account.supervisor.retry(cx);
            return;
        }
        connection.supervisor.retry(cx);
        if index == 0 {
            for state in self.books.values_mut() {
                state.reset();
            }
        }
    }

    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let message = match serde_json::from_str::<PublicMessage<'_>>(text) {
            Ok(message) => message,
            Err(error) => {
                if self
                    .public_parse_logged_at
                    .is_none_or(|at| cx.now - at >= PUBLIC_PARSE_LOG_MS)
                {
                    self.public_parse_logged_at = Some(cx.now);
                    let head: String = text.chars().take(200).collect();
                    cx.notice(
                        Level::Warn,
                        code::STREAM_MALFORMED,
                        format!("Bitget public message unreadable ({error}): {head}"),
                    );
                }
                if let Ok(header) = serde_json::from_str::<PublicHeader<'_>>(text)
                    && header.arg.channel == "books"
                {
                    let symbol = header.arg.inst_id.as_ref();
                    if let Some(state) = self.books.get_mut(symbol) {
                        state.reset();
                        let topics = [book_topic(symbol.to_owned())];
                        self.subscribe(cx, "unsubscribe", &topics);
                        self.subscribe(cx, "subscribe", &topics);
                    }
                }
                return;
            }
        };
        if message.event == "error" {
            let topic = (
                message.arg.channel.to_string(),
                message.arg.inst_id.to_string(),
            );
            let retry = (!topic.0.is_empty() && !topic.1.is_empty())
                .then(|| {
                    self.refused_topics
                        .entry(topic.clone())
                        .or_insert((0, None))
                })
                .filter(|(made, timer)| *made < SUBSCRIBE_RETRIES && timer.is_none());
            let detail = match retry {
                Some((made, timer)) => {
                    *timer = Some(cx.set_timer(1_000.0 * f64::from(1u32 << *made)));
                    *made += 1;
                    format!(
                        "{} {}: retry {made} of {SUBSCRIBE_RETRIES}",
                        topic.0, topic.1
                    )
                }
                None if !topic.0.is_empty() => format!("{} {}: not retried", topic.0, topic.1),
                None => String::new(),
            };
            cx.notice(
                Level::Error,
                code::SUBSCRIPTION_REJECTED,
                if detail.is_empty() {
                    format!("Bitget WS: {}", message.msg)
                } else {
                    format!("Bitget WS: {} ({detail})", message.msg)
                },
            );
            return;
        }
        if !self.refused_topics.is_empty()
            && let Some((_, timer)) = self.refused_topics.remove(&(
                message.arg.channel.to_string(),
                message.arg.inst_id.to_string(),
            ))
            && let Some(timer) = timer
        {
            cx.clear_timer(timer);
        }
        match message.data {
            PublicData::Tickers(rows) => {
                for row in rows {
                    let symbol = if row.inst_id.is_empty() {
                        &row.symbol
                    } else {
                        &row.inst_id
                    };
                    let Some(market) = cx.store.market_index(symbol) else {
                        continue;
                    };
                    let index = match self.ticker_index.get(market as usize).copied().flatten() {
                        Some(index) => index,
                        None => {
                            if !row.has_values() {
                                continue;
                            }
                            let Some(index) = cx.store.ensure_ticker(symbol) else {
                                continue;
                            };
                            self.ticker_index.resize(cx.store.markets().len(), None);
                            self.ticker_index[market as usize] = Some(index);
                            index
                        }
                    };
                    cx.store
                        .update_ticker_at_observed(index, row.update_mask(), |ticker| {
                            update_ticker(ticker, &row)
                        });
                    self.observe_protective_quote(
                        symbol,
                        row.value(TickerField::LastPr),
                        row.value(TickerField::MarkPrice),
                        cx.now,
                    );
                    self.live_tickers
                        .record(cx.store, symbol, row.update_mask());
                }
            }
            PublicData::Books(rows) => {
                let symbol = message.arg.inst_id.as_ref();
                let Some(state) = self.books.get_mut(symbol) else {
                    return;
                };
                for row in rows {
                    match state.apply(&message.action, &row) {
                        Ok(true) => cx.book_changed(symbol),
                        Ok(false) => {}
                        Err(error) => {
                            state.reset();
                            cx.notice(Level::Warn, code::STREAM_MALFORMED, error);
                            let topics = [book_topic(symbol.to_owned())];
                            self.subscribe(cx, "unsubscribe", &topics);
                            self.subscribe(cx, "subscribe", &topics);
                            break;
                        }
                    }
                }
            }
            PublicData::Candles(rows) => {
                let symbol = message.arg.inst_id.as_ref();
                let native = message.arg.channel.strip_prefix("candle").unwrap_or("");
                if message.action == "update"
                    && let Some((_, value)) = self
                        .candles
                        .keys()
                        .find(|(s, i)| s == symbol && interval(*i) == Some(native))
                {
                    for row in rows {
                        if row.iter().all(|n| n.0.is_some()) {
                            cx.candle(symbol, *value, candle(&row));
                        } else {
                            cx.notice(Level::Warn, code::STREAM_MALFORMED, "invalid Bitget candle");
                        }
                    }
                }
            }
            PublicData::None => {}
        }
    }
}

fn ticker_index(store: &safe_cex_engine::Store) -> Vec<Option<u32>> {
    let mut indexes = vec![None; store.markets().len()];
    // Store's symbol lookup selects the last duplicate ticker row.
    for (index, ticker) in store.tickers().iter().enumerate() {
        if let Some(market) = store.market_index(&ticker.symbol) {
            indexes[market as usize] = Some(index as u32);
        }
    }
    indexes
}

fn update_ticker(t: &mut Ticker, row: &TickerRow<'_>) {
    for (target, value) in [
        (&mut t.bid, row.value(TickerField::BidPr)),
        (&mut t.ask, row.value(TickerField::AskPr)),
        (&mut t.last, row.value(TickerField::LastPr)),
        (&mut t.mark, row.value(TickerField::MarkPrice)),
        (&mut t.index, row.value(TickerField::IndexPrice)),
        (&mut t.funding_rate, row.value(TickerField::FundingRate)),
        (&mut t.funding_time, row.value(TickerField::NextFundingTime)),
        (&mut t.volume, row.value(TickerField::BaseVolume)),
        (&mut t.quote_volume, row.value(TickerField::QuoteVolume)),
    ] {
        if let Some(value) = value {
            *target = value;
        }
    }
    if let Some(change) = row
        .value(TickerField::Change24h)
        .or(row.value(TickerField::ChangeUtc24h))
    {
        t.percentage = change * 100.0;
    }
}

fn market(row: &Contract<'_>) -> Market {
    Market {
        id: row.symbol.to_string(),
        symbol: row.symbol.to_string(),
        pretty_symbol: None,
        base: row.base_coin.to_string(),
        quote: row.quote_coin.to_string(),
        active: matches!(
            row.symbol_status.as_ref(),
            "normal" | "listed" | "limit_open"
        ),
        precision: Precision {
            amount: row.size_multiplier.get(),
            price: if row.price_end_step.get().fract() == 0.0
                && row.price_end_step.get() <= 9_007_199_254_740_991.0
            {
                row.price_end_step.get() / 10f64.powi(row.price_place.get() as i32)
            } else {
                format!("{}e-{}", row.price_end_step.get(), row.price_place.get())
                    .parse::<f64>()
                    .unwrap_or(0.0)
            },
        },
        limits: Limits {
            amount: MinMax {
                min: row.min_trade_num.get(),
                max: OrderCaps::from_wire(row, false).shared(),
            },
            leverage: MinMax {
                min: row.min_lever.0.filter(|n| *n != 0.0).unwrap_or(1.0),
                max: row.max_lever.0.filter(|n| *n != 0.0).unwrap_or(20.0),
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
    }
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
        H6 => "6Hutc",
        H12 => "12Hutc",
        D1 => "1Dutc",
        W1 => "1Wutc",
        _ => return None,
    })
}

/// The (channel, instId) every book subscribe, unsubscribe and repair names.
fn book_topic(symbol: String) -> (String, String) {
    ("books".to_owned(), symbol)
}

/// The (channel, instId) every candle subscribe, unsubscribe and repair names.
fn candle_topic(symbol: String, value: Interval) -> Option<(String, String)> {
    Some((format!("candle{}", interval(value)?), symbol))
}

fn candle(row: &CandleRow) -> Candle {
    Candle {
        timestamp: row[0].get() / 1000.0,
        open: row[1].get(),
        high: row[2].get(),
        low: row[3].get(),
        close: row[4].get(),
        volume: row[6].get(),
    }
}

impl Venue for BitgetFutures {
    fn http_release_veto(
        &self,
        store: &safe_cex_engine::Store,
        pending: &Pending,
        release: &safe_cex_engine::request_context::HttpRelease,
    ) -> Result<(), safe_cex_engine::VenueError> {
        self.protective_release_veto(store, pending, release)
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            position_history: safe_cex_engine::PositionHistory::FillsAccount,
            cancel_all: safe_cex_engine::CancelAll::EachPlacedSymbol,
            quantity_units: if self.inverse() {
                safe_cex_engine::QuantityUnits::InverseNative
            } else {
                safe_cex_engine::QuantityUnits::Base
            },
            account: safe_cex_engine::AccountLookup::Query,
            leverage_query: safe_cex_engine::LeverageQuery::OnReadAndMetadata,
            fresh_positions: true,
            nuke_account_cancel: true,
            symbol_cancel_misses_unseen: true,
            // Every census commits its covers and answers a refresh ask.
            order_read_covers: true,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn guard_pending_placements(&self) -> bool {
        true
    }
    fn refresh_settings(&mut self, cx: &mut Cx<'_, Pending>, symbol: Option<&str>) {
        self.refresh_settings(cx, symbol);
    }
    fn on_venue_action(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: safe_cex_engine::CommandId,
        action: safe_cex_engine::VenueAction,
    ) {
        match action {
            safe_cex_engine::VenueAction::ReadPositions => self.read_positions(cx),
            safe_cex_engine::VenueAction::RegisterCancelChecks { orders } => {
                let root = match cx.admit_caller_read_root(None, None) {
                    Ok(root) => root,
                    Err(safe_cex_engine::request_context::CommandEntryError::Revoked) => {
                        let _ = cx.reject_expired_command(
                            safe_cex_engine::request_context::Operation::Read,
                        );
                        return;
                    }
                    Err(safe_cex_engine::request_context::CommandEntryError::Rejected(_)) => return,
                };
                self.register_cancel_checks(cx, &orders);
                let _ = cx.done_context(&root, "null".into());
            }
            action @ safe_cex_engine::VenueAction::CancelAccountOrders => {
                self.trade_command(cx, safe_cex_engine::Command::Venue { command, action })
            }
            _ => safe_cex_engine::unsupported_venue_action(cx, command),
        }
    }
    fn export_venue_state(&self) -> Option<Vec<u8>> {
        Some(venue_state::encode(
            self.account.detected,
            self.account.uta,
            &self.account.plans,
            &self.account.trailing,
            &self.account.execution_prices,
        ))
    }
    fn on_frame_ingested(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account_mode_event(cx);
        self.observe_framed_close_bookings(cx);
    }
    fn apply_venue_state(&mut self, cx: &mut Cx<'_, Pending>, state: &[u8]) {
        let Some(state) = venue_state::decode(state) else {
            cx.venue_state_unreadable("Unreadable Bitget venue state from the data source");
            return;
        };
        self.account.detected = state.detected;
        self.account.uta = state.uta;
        self.account.plans = state.plans;
        self.account.trailing = state.trailing;
        self.account.execution_prices = state.execution_prices;
        // Writes waiting on detection may now plan.
        self.wake_trading(cx);
    }
    fn guard_order_mutations(&self) -> bool {
        true
    }
    // reconcile_uncertain reads the orders at once; the engine re-asks until a
    // fresh read clears the fence.
    fn fence_uncertain_placements(&self) -> bool {
        true
    }
    fn enforces_reduce_only(&self, _store: &safe_cex_engine::Store, _symbol: &str) -> bool {
        true
    }
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _symbols: Option<&[String]>) {
        self.reconcile_orders(cx);
    }
    fn take_source_cancel_checks(&mut self) -> Vec<safe_cex_engine::model::CancelOrderRef> {
        self.take_retired_cancel_targets()
    }
    fn refreshes_orders(&self) -> bool {
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
    fn on_condition_wait_expired(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        owner: &safe_cex_engine::request_context::ConditionOwner,
        _: safe_cex_engine::VenueError,
    ) {
        self.trading_condition_expired(cx, owner);
    }
    fn on_http_owner_retired_preflight(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) -> Option<safe_cex_engine::request_context::Preflight> {
        self.trading_retired(owner)
    }

    type Pending = Pending;
    fn startup_reads(pending: &Pending) -> safe_cex_engine::startup::Sections {
        use safe_cex_engine::startup::Sections as S;
        match pending {
            Pending::Discovery {
                rows: Discovered::Markets(..),
                ..
            } => S::MARKETS,
            Pending::Discovery { .. } => S::TICKERS,
            Pending::Account { request, .. } => request.startup_sections(),
            _ => S::NONE,
        }
    }

    fn repair_subscription(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        topic: &SubscriptionTopic,
    ) -> Option<Result<(), String>> {
        Some(self.repair(cx, topic))
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate().and_then(|_| {
                read.passphrase
                    .as_ref()
                    .filter(|key| !key.0.is_empty())
                    .map(|_| ())
                    .ok_or("Bitget secondary signing credentials require a passphrase")
            })
        {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            self.account.startup_rejected = true;
            cx.startup_failed(
                safe_cex_engine::VenueError::new(error)
                    .kind(safe_cex_engine::ErrorKind::InvalidInput),
            );
            return;
        }
        self.discovery_read(cx, false);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let Some(index) = self.channel(socket) else {
            return;
        };
        let connection = &mut self.sockets[index];
        connection.open = true;
        connection.ping_at = cx.now;
        if index == 3 {
            self.account.login_answered = false;
            self.account.supervisor.opened(cx);
            self.private_open(cx, socket);
            cx.send(socket, "ping");
            return;
        }
        connection.supervisor.opened(cx);
        let mut topics: Vec<_> = cx
            .store
            .markets()
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 3 == index)
            .map(|(_, m)| ("ticker".to_owned(), m.symbol.clone()))
            .collect();
        if index == 0 {
            for (symbol, state) in &mut self.books {
                state.reset();
                topics.push(book_topic(symbol.clone()));
            }
            topics.extend(
                self.candles
                    .keys()
                    .filter_map(|(s, i)| candle_topic(s.clone(), *i)),
            );
        }
        self.send_topics(cx, socket, "subscribe", &topics);
        cx.send(socket, "ping");
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(index) = self.channel(socket) else {
            return;
        };
        if index == 3 {
            self.account.supervisor.received(cx.now);
        } else {
            self.sockets[index].supervisor.received(cx.now);
        }
        if text == "pong" {
            cx.store
                .set_latency((cx.now - self.sockets[index].ping_at) / 2.0);
            return;
        }
        if index == 3 {
            self.private_message(cx, text);
            self.account_mode_event(cx);
        } else {
            self.public_message(cx, text);
        }
    }
    #[inline]
    fn route_exhaustion() -> bool {
        true
    }
    fn on_route_exhausted(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.sockets[3].socket == Some(socket) {
            self.private_route_exhausted(cx);
        } else {
            self.on_close(cx, socket, safe_cex_engine::ROUTE_EXHAUSTED_CLOSE);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _: u16) {
        if let Some(index) = self.channel(socket) {
            self.disconnected(cx, index);
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Identity(_) | Pending::Leverage(_) => {}
            Pending::History(request) => self.query_item(cx, request, item),
            Pending::Trade(_) => {}
            Pending::Account {
                generation,
                context,
                request,
            } => {
                if *generation == self.account.generation
                    && cx.validate_background_read_http_delivery(context).is_ok()
                {
                    self.account_item(cx, request, item);
                }
            }
            Pending::Discovery { context, rows } => {
                if cx.validate_background_read_http_delivery(context).is_err() {
                    return;
                }
                match rows {
                    Discovered::Markets(rows, caps) => match serde_json::from_str::<Contract>(item)
                    {
                        Ok(row) if row.quote_coin == self.quote() => {
                            if row.symbol.is_empty()
                                || row.base_coin.is_empty()
                                || row.price_place.0.is_none()
                                || row.price_place.get().fract() != 0.0
                                || !(0.0..=18.0).contains(&row.price_place.get())
                                || row.price_end_step.get() <= 0.0
                                || row.size_multiplier.get() <= 0.0
                            {
                                safe_cex_engine::startup::skipped_row(cx);
                            } else {
                                self.trading
                                    .min_order_value
                                    .insert(row.symbol.to_string(), row.min_trade_usdt.get());
                                let mut mapped = market(&row);
                                if self.inverse() {
                                    if row.min_lever.get() == 0.0 {
                                        mapped.limits.leverage.min = 1.0;
                                    }
                                    mapped.is_inverse = Some(true);
                                    if row.max_lever.get() == 0.0 {
                                        mapped.limits.leverage.max = 20.0;
                                    }
                                }
                                if !mapped.precision.price.is_finite()
                                    || mapped.precision.price <= 0.0
                                {
                                    safe_cex_engine::startup::skipped_row(cx);
                                } else {
                                    let limits = OrderCaps::from_wire(&row, self.inverse());
                                    mapped.limits.amount.max = limits.shared();
                                    caps.insert(
                                        mapped.symbol.clone(),
                                        (limits.limit, limits.market),
                                    );
                                    rows.push(mapped);
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(_) => safe_cex_engine::startup::skipped_row(cx),
                    },
                    Discovered::Tickers(rows) => {
                        match serde_json::from_str::<TickerRow>(item) {
                            Ok(row) if row.has_values() => {
                                if let Some(m) = cx.store.market_by_id(&row.symbol) {
                                    let mut ticker = Ticker {
                                        id: m.id.clone(),
                                        symbol: m.symbol.clone(),
                                        mark: row.value(TickerField::LastPr).unwrap_or(0.0),
                                        ..Ticker::default()
                                    };
                                    update_ticker(&mut ticker, &row);
                                    ticker.mark = row.value(TickerField::LastPr).unwrap_or(0.0);
                                    // The REST seed uses the UTC change; the live stream uses change24h.
                                    ticker.percentage =
                                        row.value(TickerField::ChangeUtc24h).unwrap_or(0.0) * 100.0;
                                    ticker.funding_time = 0.0;
                                    rows.push(ticker);
                                }
                            }
                            Ok(_) | Err(_) => safe_cex_engine::startup::skipped_row(cx),
                        }
                    }
                }
            }
            Pending::Candles { rows, invalid, .. } => match serde_json::from_str::<CandleRow>(item)
            {
                Ok(row) if row.iter().all(|n| n.0.is_some()) => rows.push(candle(&row)),
                Ok(_) => *invalid = true,
                Err(_) => *invalid = true,
            },
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Identity(root) = pending {
            self.identity_response(cx, &root, response);
            return;
        }
        if let Pending::Leverage(read) = pending {
            self.leverage_response(cx, read, response);
            return;
        }
        if let Pending::History(request) = pending {
            self.query_response(cx, *request, response);
            return;
        }
        if let Pending::Trade(request) = pending {
            self.trade_response(cx, *request, response);
            return;
        }
        if let Pending::Account {
            generation,
            context,
            request,
        } = pending
        {
            // Publish under live custody, then retire; a stale page only retires.
            if generation == self.account.generation
                && cx.validate_background_read_http_delivery(&context).is_ok()
            {
                self.account_response(cx, request, response);
                self.account_mode_event(cx);
                self.wake_trading(cx);
            }
            let _ = cx.finish_http_context(&context);
            return;
        }
        // Contracts and tickers decode and publish only under live custody.
        if !discovery_delivered(cx, &pending) {
            retire_discovery(cx, &pending);
            return;
        }
        let valid = serde_json::from_str::<Envelope<IgnoredAny>>(response.body)
            .map_err(|e| e.to_string())
            .and_then(Envelope::checked);
        if valid.is_err() || response.status != 200 {
            if discovery_context(&pending).is_some() {
                return self.discovery_failed(cx, &pending, discovery_error(&response));
            }
            let error = match valid {
                Err(error) => error,
                Ok(_) => format!("Bitget HTTP {}: {}", response.status, response.body),
            };
            // A candles read takes the table's kind and native code from the reply.
            if let Pending::Candles { root, .. } = pending {
                let _ = cx.fail_context(&root, account::read_error(&response, error));
                return;
            }
            self.on_request_failed(cx, pending, &error);
            return;
        }
        match pending {
            Pending::Discovery { context, rows } => {
                let tickers = matches!(rows, Discovered::Tickers(_));
                let empty = match &rows {
                    Discovered::Markets(rows, _) => rows.is_empty(),
                    Discovered::Tickers(rows) => rows.is_empty(),
                };
                // A product with no contracts cannot appear by retrying.
                if empty && !tickers && self.inverse() {
                    let _ = cx.finish_http_context(&context);
                    let error =
                        safe_cex_engine::VenueError::new("Bitget lists no inverse contracts")
                            .kind(safe_cex_engine::ErrorKind::VenueUnavailable);
                    cx.notice_error(Level::Error, code::for_startup_error(&error), &error);
                    cx.startup_failed(error);
                    return;
                }
                if empty {
                    let _ = cx.finish_http_context(&context);
                    let message = if tickers {
                        "Empty Bitget tickers"
                    } else {
                        "Empty Bitget catalogue"
                    };
                    let error = safe_cex_engine::VenueError::new(message)
                        .kind(safe_cex_engine::ErrorKind::VenueUnavailable);
                    return self.public_failed(cx, tickers, error);
                }
                match rows {
                    Discovered::Markets(rows, caps) => {
                        self.order_caps = caps
                            .into_iter()
                            .map(|(symbol, (limit, market))| (symbol, OrderCaps { limit, market }))
                            .collect();
                        cx.store.set_markets(rows);
                    }
                    Discovered::Tickers(mut rows) => {
                        for row in &rows {
                            self.trading_seed_reference(&row.symbol, row.last, cx.now);
                        }
                        self.live_tickers.finish(cx.store, &mut rows);
                        cx.store.set_tickers(rows);
                    }
                }
                self.ticker_index = ticker_index(cx.store);
                let _ = cx.finish_http_context(&context);
                if tickers {
                    self.discovery.succeeded(cx, DiscoveryRead::Tickers);
                    self.open_feed(cx, 1);
                    self.open_feed(cx, 2);
                } else if cx.frame_fed() {
                    // The page engine loads instruments only; its data source streams.
                    self.discovery.succeeded(cx, DiscoveryRead::Markets);
                } else {
                    self.connect(cx, 0);
                    self.discovery.succeeded(cx, DiscoveryRead::Markets);
                    self.discovery_read(cx, true);
                    self.start_private(cx);
                }
            }
            Pending::Candles {
                root,
                rows,
                invalid: false,
            } => {
                if cx.validate_read_http_delivery(&root, &root).is_ok() {
                    let _ = cx.done_context(&root, serde_json::to_string(&rows).expect("candles"));
                }
            }
            pending => self.on_request_failed(cx, pending, "invalid Bitget REST record"),
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
        if discovery_context(&pending).is_some() {
            let error = safe_cex_engine::startup::public_response_error(status, None, None);
            self.discovery_failed(cx, &pending, error);
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
        if discovery_context(&pending).is_some() {
            if discovery_delivered(cx, &pending) {
                self.discovery_failed(cx, &pending, discovery_error(&response));
            } else {
                retire_discovery(cx, &pending);
            }
            return;
        }
        let refused = serde_json::from_str::<Envelope<IgnoredAny>>(response.body)
            .ok()
            .is_some_and(|e| e.code.0.is_some_and(|code| code != 0.0));
        let empty_account_list = pending.reads_account_list() && wire::null_list(response.body);
        let empty_history =
            matches!(&pending, Pending::History(_)) && wire::null_history(response.body);
        if refused || empty_account_list || empty_history {
            self.on_response(cx, pending, response);
        } else {
            cx.notice(
                Level::Warn,
                code::REQUEST_FAILED,
                format!("Bitget response has no expected list: {}", response.body),
            );
            self.on_request_failed(
                cx,
                pending,
                "Bitget successful response has no expected list",
            );
        }
    }
    fn on_signing_release(
        &mut self,
        target: safe_cex_engine::SigningTarget<'_, Pending>,
        release: safe_cex_engine::SigningRelease,
    ) {
        if let safe_cex_engine::SigningTarget::Http { pending, .. } = target {
            self.position_read_released(pending, release);
        }
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        cx.with_startup_request_failure(pending, error, |cx, pending, error| match pending {
            Pending::Identity(root) | Pending::Candles { root, .. } => {
                let _ = cx.fail_context(&root, error);
            }
            Pending::Leverage(read) => self.leverage_failed(cx, read, error),
            Pending::History(request) => self.query_strict_failed(cx, *request, error),
            Pending::Trade(request) => self.trade_call_failed(cx, *request, error, false),
            pending @ Pending::Discovery { .. } => self.discovery_failed(cx, &pending, error),
            Pending::Account {
                generation,
                context,
                request,
            } => {
                let _ = cx.finish_http_context(&context);
                if generation == self.account.generation {
                    self.account_sign_failed(cx, request, &error.message);
                    self.wake_trading(cx);
                }
            }
        });
    }

    /// History keeps typed failure causes separate from native response retries;
    /// managed candle and identity reads preserve their deadline handling.
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        let deadline = error.kind == Some(safe_cex_engine::ErrorKind::Timeout);
        match pending {
            Pending::Candles { root, .. } if deadline => {
                let _ = cx.fail_context(&root, error);
            }
            Pending::Identity(root) if deadline => self.identity_failed(cx, &root, error),
            Pending::Leverage(read) => self.leverage_failed(cx, read, error),
            Pending::History(request) => self.query_http_failed(cx, *request, error),
            Pending::Trade(request) => self.trade_call_failed(cx, *request, error, true),
            pending @ Pending::Discovery { .. } => self.discovery_failed(cx, &pending, error),
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Pending::Identity(root) = pending {
            self.identity_failed(cx, &root, safe_cex_engine::VenueError::new(error));
            return;
        }
        if let Pending::Leverage(read) = pending {
            self.leverage_failed(cx, read, safe_cex_engine::VenueError::new(error));
            return;
        }
        match pending {
            Pending::History(mut request) => {
                if request.delivered(cx) {
                    self.query_failed(cx, *request, error);
                } else {
                    let _ = request.retire_leg(cx);
                }
            }
            Pending::Trade(request) => {
                let error = safe_cex_engine::VenueError::new(error)
                    .kind(safe_cex_engine::ErrorKind::Network);
                self.trade_call_failed(cx, *request, error, true)
            }
            Pending::Candles { root, .. } => {
                let _ = cx.fail_context(&root, error);
            }
            Pending::Account {
                generation,
                context,
                request,
            } => {
                let _ = cx.finish_http_context(&context);
                if generation == self.account.generation {
                    self.account_transport_failed(cx, request, error);
                    self.wake_trading(cx);
                }
            }
            pending @ Pending::Discovery { .. } => {
                let error = safe_cex_engine::VenueError::new(error)
                    .kind(safe_cex_engine::ErrorKind::Network);
                self.discovery_failed(cx, &pending, error);
            }
            Pending::Identity(_) | Pending::Leverage(_) => unreachable!(),
        }
    }
    fn read_root_retired(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
        _pending: Vec<Pending>,
    ) {
        self.history_retired(owner);
        self.account.fresh_positions.retired(owner);
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.settle_retired_write(cx);
        if let Some(topic) = self
            .refused_topics
            .iter_mut()
            .find(|(_, (_, pending))| *pending == Some(timer))
            .map(|(topic, (_, pending))| {
                *pending = None;
                topic.clone()
            })
        {
            self.subscribe(cx, "subscribe", &[topic]);
            return;
        }
        if let Some(read) = self.discovery.on_timer(timer) {
            self.discovery_read(cx, read == DiscoveryRead::Tickers);
            return;
        }
        self.account_timer(cx, timer);
        self.history_timer(cx, timer);
        for index in 0..self.sockets.len() {
            let tick = if index == 3 {
                self.account.supervisor.on_timer(cx, timer)
            } else {
                self.sockets[index].supervisor.on_timer(cx, timer)
            };
            match tick {
                Tick::Unhandled => continue,
                Tick::Reconnect => self.open_feed(cx, index),
                Tick::Ping => {
                    if let Some(socket) = self.sockets[index].socket {
                        cx.send(socket, "ping");
                        self.sockets[index].ping_at = cx.now;
                    }
                }
                Tick::TimedOut(_) => {
                    if let Some(socket) = self.sockets[index].socket {
                        cx.close(socket);
                    }
                    self.disconnected(cx, index);
                }
                Tick::Handled => (),
            }
            return;
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        // The engine binds the command when the identity read is admitted.
        if let Command::FetchAccount { .. } = command {
            self.fetch_identity(cx);
            return;
        }
        if let Command::FetchLeverage { symbol, .. } = command {
            self.fetch_leverage(cx, symbol);
            return;
        }
        match command {
            Command::SubscribeOrderBook { symbol } => {
                let state = self.books.entry(symbol.clone()).or_default();
                state.subscribers += 1;
                if state.subscribers == 1 {
                    self.subscribe(cx, "subscribe", &[book_topic(symbol)]);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers -= 1;
                    if state.subscribers == 0 {
                        self.books.remove(&symbol);
                        self.subscribe(cx, "unsubscribe", &[book_topic(symbol)]);
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: value,
            } => {
                let Some(topic) = candle_topic(symbol.clone(), value) else {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        value.unsupported("Bitget"),
                    );
                    return;
                };
                let count = self.candles.entry((symbol, value)).or_default();
                *count += 1;
                if *count == 1 {
                    self.subscribe(cx, "subscribe", &[topic]);
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: value,
            } => {
                let key = (symbol, value);
                if let Some(count) = self.candles.get_mut(&key) {
                    *count -= 1;
                    if *count == 0 {
                        self.candles.remove(&key);
                        if let Some(topic) = candle_topic(key.0, key.1) {
                            self.subscribe(cx, "unsubscribe", &[topic]);
                        }
                    }
                }
            }
            Command::FetchCandles {
                command: _,
                symbol,
                interval: value,
                limit,
                from_ms,
                to_ms,
            } => {
                // A rejected admission already published the command's typed failure.
                let Ok(root) = cx.admit_http_command(read_policy(None)) else {
                    return;
                };
                let Some(native) = interval(value) else {
                    let _ = cx.fail_context(&root, value.unsupported("Bitget"));
                    return;
                };
                let limit = limit.unwrap_or(500).clamp(1, 1000);
                let mut end = to_ms.unwrap_or(cx.protocol_now());
                if let Some(start) = from_ms {
                    end = end.min(start + (value.ms() * f64::from(limit)).min(90.0 * 86_400_000.0));
                }
                let mut path = format!(
                    "{}?symbol={symbol}&granularity={native}&endTime={}&productType={}&limit={limit}",
                    paths::CANDLES,
                    num::format(end),
                    self.product()
                );
                if let Some(start) = from_ms.filter(|v| *v < end) {
                    path.push_str(&format!("&startTime={}", num::format(start)));
                }
                let pending = Pending::Candles {
                    root: root.clone(),
                    rows: vec![],
                    invalid: false,
                };
                match cx.request_managed_with_headers(
                    root,
                    None,
                    Method::Get,
                    format!("{}{path}", paths::REST),
                    CHANNEL,
                    None,
                    pending,
                ) {
                    Ok(id) => cx.stream_response(id, ArrayAt::Envelope(&[&["data"]])),
                    Err((pending, reason)) => self.on_sign_error(cx, pending, reason.venue_error()),
                }
            }
            Command::Query { command, query } => self.start_query(cx, command, query),
            trade => self.trade_command(cx, trade),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|b| b.synced).map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

/// Broker attribution, the only header a public Bitget read carries.
const CHANNEL: &[(&str, &str)] = &[("X-CHANNEL-API-CODE", "Tealstreet")];

/// Managed reads: one physical attempt inside a shared 20 s deadline.
pub(crate) fn read_policy(credential: Option<Credential>) -> Policy {
    Policy {
        operation: Operation::Read,
        credential,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

#[cfg(test)]
mod secondary_tests;
