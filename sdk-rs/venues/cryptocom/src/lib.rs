//! Crypto.com Exchange v1 perpetual derivatives on the sans-I/O engine.
mod account;
mod depth_query;
mod history;
mod identity;
mod public_trades;
mod reads;
pub mod sign;
mod trading;
mod wire;

use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Balance, Candle, Interval, Market, Ticker};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
pub(crate) use safe_cex_engine::url::encode;
use safe_cex_engine::{
    Command, Cx, Method, PublicFeedPolicy, Response, SocketId, TimerId, Venue, num,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use wire::*;

fn read_policy() -> Policy {
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
    pub credentials: Option<safe_cex_engine::signing::Identity>,
    pub secondary_validation: Option<Result<(), &'static str>>,
    pub testnet: bool,
    pub public_data_only: bool,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        self.credentials = None;
        self.secondary_validation = None;
    }
}

pub enum Pending {
    BookQuery {
        context: safe_cex_engine::request_context::Context,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    PublicTrades(Box<public_trades::Request>),
    Identity(identity::Request),
    Markets {
        context: Context,
        rows: Vec<Market>,
        indexes: HashMap<String, String>,
        skipped: Skipped,
        malformed: HashSet<String>,
    },
    Tickers {
        context: Context,
        rows: Vec<Ticker>,
        skipped: Skipped,
    },
    Candles {
        context: Context,
        rows: Vec<Candle>,
        invalid: bool,
    },
}

impl Pending {
    fn discovery_context(&self) -> Option<&Context> {
        match self {
            Self::Markets { context, .. } | Self::Tickers { context, .. } => Some(context),
            _ => None,
        }
    }
}

struct Connection {
    socket: Option<SocketId>,
    open: bool,
    authenticated: bool,
    ready: Option<TimerId>,
    supervisor: Supervisor,
}
impl Connection {
    fn new(private: bool) -> Self {
        Self {
            socket: None,
            open: false,
            authenticated: false,
            ready: None,
            supervisor: Supervisor::new(Config {
                idle_timeout_ms: 90_000.0,
                auth_timeout_ms: private.then_some(10_000.0),
                ..Config::default()
            }),
        }
    }
}

struct BookState {
    book: Book,
    subscribers: u32,
    synced: bool,
}

pub struct Cryptocom {
    options: Options,
    rest: &'static str,
    sockets: [Connection; 2],
    discovery: safe_cex_engine::startup::Backoff<bool>,
    // Instruments rejected as malformed; owned exposure there holds the snapshot.
    malformed_markets: HashSet<String>,
    reread_stalls: u8,
    subscriptions: BTreeMap<String, u32>,
    indexes: HashMap<String, String>,
    books: HashMap<String, BookState>,
    message_id: u64,
    credentials: Option<safe_cex_engine::signing::Identity>,
    slots: safe_cex_engine::credential_selection::Slots<safe_cex_engine::signing::Identity>,
    account: account::Account,
    trading: trading::Trading,
    history: history::History,
    public_trades: public_trades::State,
}

const TIMEFRAMES: [(Interval, &str); 10] = [
    (Interval::M1, "1m"),
    (Interval::M5, "5m"),
    (Interval::M15, "15m"),
    (Interval::M30, "30m"),
    (Interval::H1, "1h"),
    (Interval::H2, "2h"),
    (Interval::H4, "4h"),
    (Interval::H12, "12h"),
    (Interval::D1, "1D"),
    (Interval::W1, "7D"),
];
pub(crate) fn interval(value: Interval) -> Result<&'static str, String> {
    TIMEFRAMES
        .iter()
        .find(|(i, _)| *i == value)
        .map(|(_, native)| *native)
        .ok_or_else(|| value.unsupported("Crypto.com"))
}

fn quoted(s: &str) -> String {
    serde_json::to_string(s).expect("string")
}

/// Unmappable rows of one read or frame, reported once as a summary; the rest still commits.
#[derive(Default)]
pub struct Skipped(safe_cex_engine::coverage::RowCount);
impl Skipped {
    pub(crate) fn add(&mut self, id: Option<&str>) {
        self.0.add(id);
    }
    pub(crate) fn any(&self) -> bool {
        self.0.any()
    }
    pub(crate) fn report(&self, cx: &mut Cx<'_, Pending>, kind: &str) {
        safe_cex_engine::coverage::row_summary(
            cx,
            Level::Warn,
            self.0.summary(|rows| {
                let first = rows
                    .first()
                    .expect("nonempty group")
                    .unwrap_or("without an id");
                format!(
                    "Crypto.com skipped {} unmappable {kind} rows (first: {first})",
                    rows.count()
                )
            }),
        );
    }
}

impl Cryptocom {
    fn discovery_read(&mut self, cx: &mut Cx<'_, Pending>, tickers: bool) {
        let context = match cx.http_context(Scope::Background, read_policy()) {
            Ok(context) => context,
            Err(_) => {
                self.discovery.failed(cx, tickers);
                return;
            }
        };
        let path = if tickers {
            "/public/get-tickers"
        } else {
            "/public/get-instruments"
        };
        let pending = if tickers {
            Pending::Tickers {
                context: context.clone(),
                rows: vec![],
                skipped: Skipped::default(),
            }
        } else {
            Pending::Markets {
                context: context.clone(),
                rows: vec![],
                indexes: HashMap::new(),
                skipped: Skipped::default(),
                malformed: HashSet::new(),
            }
        };
        match cx.request_managed(
            context,
            None,
            Method::Get,
            format!("{}{path}", self.rest),
            None,
            pending,
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::Path(&["result", "data"])),
            Err((pending, error)) => {
                self.on_request_failed(cx, pending, &error.venue_error().message)
            }
        }
    }
    pub fn new(options: Options) -> Self {
        let credentials = options
            .credentials
            .as_ref()
            .filter(|identity| !options.public_data_only && identity.validate().is_ok())
            .cloned();
        let slots = safe_cex_engine::credential_selection::Slots::new(
            credentials.clone(),
            None,
            Default::default(),
        );
        Self {
            slots,
            credentials,
            account: Default::default(),
            trading: Default::default(),
            history: Default::default(),
            public_trades: Default::default(),
            rest: if options.testnet {
                "https://uat-api.3ona.co/exchange/v1"
            } else {
                "https://api.crypto.com/exchange/v1"
            },
            options,
            sockets: [Connection::new(false), Connection::new(true)],
            discovery: Default::default(),
            malformed_markets: HashSet::new(),
            reread_stalls: 0,
            subscriptions: BTreeMap::new(),
            indexes: HashMap::new(),
            books: HashMap::new(),
            message_id: 0,
        }
    }

    fn connect(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        if self.sockets[channel].socket.is_some() {
            return;
        }
        // Suppressed public feeds never open; supervising them would only churn retries.
        if channel != 0 || cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.sockets[channel].supervisor.connecting(cx);
        }
        let root = if self.options.testnet {
            "wss://uat-stream.3ona.co"
        } else {
            "wss://stream.crypto.com"
        };
        let kind = if channel == 0 { "market" } else { "user" };
        self.sockets[channel].socket = Some(cx.connect(
            safe_cex_engine::SocketSpec::new(format!("{root}/exchange/v1/{kind}")).channel(
                if channel == 0 {
                    safe_cex_engine::SocketChannel::Public
                } else {
                    safe_cex_engine::SocketChannel::Private
                },
            ),
        ));
    }

    fn subscribe(&mut self, cx: &mut Cx<'_, Pending>, channels: &[String], method: &str) {
        if !self.sockets[0].open || self.sockets[0].ready.is_some() {
            return;
        }
        for chunk in channels.chunks(100) {
            self.message_id += 1;
            cx.send(
                self.sockets[0].socket.unwrap(),
                format!(
                    "{{\"id\":{},\"method\":{},\"params\":{{\"channels\":{}}},\"nonce\":{}}}",
                    self.message_id,
                    quoted(method),
                    serde_json::to_string(chunk).unwrap(),
                    cx.now as u64
                ),
            );
        }
    }

    fn add_channel(&mut self, cx: &mut Cx<'_, Pending>, channel: String) {
        let count = self.subscriptions.entry(channel.clone()).or_default();
        *count += 1;
        if *count == 1 {
            self.subscribe(cx, &[channel], "subscribe");
        }
    }

    fn remove_channel(&mut self, cx: &mut Cx<'_, Pending>, channel: String) {
        if let Some(count) = self.subscriptions.get_mut(&channel) {
            *count -= 1;
            if *count == 0 {
                self.subscriptions.remove(&channel);
                self.subscribe(cx, &[channel], "unsubscribe");
            }
        }
    }

    fn market_channels(&mut self, cx: &mut Cx<'_, Pending>) {
        let mut seeded = self
            .subscriptions
            .keys()
            .filter(|channel| {
                ["ticker.", "mark.", "index.", "funding."]
                    .iter()
                    .any(|prefix| channel.starts_with(prefix))
            })
            .count();
        let markets = cx.store.markets();
        let channels =
            markets
                .iter()
                .map(|m| format!("ticker.{}", m.id))
                .chain(markets.iter().flat_map(|m| {
                    let index = self.indexes.get(&m.id).filter(|i| i.ends_with("-INDEX"));
                    [
                        Some(format!("mark.{}", m.id)),
                        index.map(|i| format!("index.{i}")),
                        Some(format!("funding.{}", m.id)),
                    ]
                    .into_iter()
                    .flatten()
                }));
        for channel in channels {
            if self.subscriptions.contains_key(&channel) {
                continue;
            }
            if seeded >= 350 {
                break;
            }
            self.subscriptions.insert(channel, 1);
            seeded += 1;
        }
    }

    fn message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        message: Message<'_>,
    ) -> serde_json::Result<()> {
        let symbol = message.instrument_name;
        match message.channel {
            "ticker" => {
                let mut first = true;
                json::each::<TickerRow>(message.data.get(), |row| {
                    if first {
                        cx.store.update_ticker(symbol, |t| row.apply(t));
                        first = false;
                    }
                })?;
            }
            "book" => {
                let Some(state) = self.books.get_mut(symbol) else {
                    return Ok(());
                };
                let mut first = true;
                json::each::<BookRow>(message.data.get(), |row| {
                    if !first {
                        return;
                    }
                    first = false;
                    state.book.clear();
                    for (side, rows) in [(BookSide::Bids, row.bids), (BookSide::Asks, row.asks)] {
                        state.book.load_sorted(
                            side,
                            rows.into_iter()
                                .map(|(p, q, _)| (num::parse(p), num::parse(q))),
                        );
                    }
                    state.synced = true;
                    let bid = state.book.best_bid().unwrap_or(0.0);
                    let ask = state.book.best_ask().unwrap_or(0.0);
                    cx.store.update_ticker(symbol, |t| {
                        t.bid = bid;
                        t.ask = ask;
                    });
                    cx.book_changed(symbol);
                })?;
            }
            "mark" | "index" | "funding" => {
                let mut first = true;
                json::each::<Valuation>(message.data.get(), |row| {
                    if !first {
                        return;
                    }
                    first = false;
                    let Ok(value) = row.v.parse::<f64>() else {
                        return;
                    };
                    if !value.is_finite() || (message.channel != "funding" && value <= 0.0) {
                        return;
                    }
                    if message.channel == "index" {
                        for (market, index) in &self.indexes {
                            if index == symbol {
                                cx.store.update_ticker(market, |t| t.index = value);
                            }
                        }
                    } else {
                        cx.store.update_ticker(symbol, |t| {
                            if message.channel == "mark" {
                                t.mark = value
                            } else {
                                t.funding_rate = value
                            }
                        });
                    }
                })?;
            }
            "candlestick" => {
                let timeframe = message
                    .subscription
                    .strip_prefix("candlestick.")
                    .and_then(|s| s.split('.').next())
                    .unwrap_or("");
                let Some(timeframe) = TIMEFRAMES
                    .iter()
                    .find(|(_, native)| *native == timeframe)
                    .map(|(i, _)| *i)
                else {
                    return Ok(());
                };
                if !self.subscriptions.contains_key(message.subscription) {
                    return Ok(());
                }
                json::each::<CandleRow>(message.data.get(), |row| {
                    cx.candle(symbol, timeframe, row.candle())
                })?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Venue for Cryptocom {
    fn http_pacing(&self) -> &[safe_cex_engine::request_context::Pacing] {
        &[public_trades::PACING]
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            order_book_query: true,
            ..safe_cex_engine::Capabilities::BASE
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
        // Native late replies have no independent Store authority; normal
        // fenced account polling reconciles unknown effects.
        safe_cex_engine::request_context::ReconciliationData::new(String::new())
    }
    fn cancel_needs_stored_order(&self) -> bool {
        true
    }
    // reconcile_uncertain starts a fresh Orders+AdvancedOrders read at once.
    fn fence_uncertain_placements(&self) -> bool {
        true
    }
    /// The engine asks once per uncertain seal and re-asks while fenced; the fresh
    /// order read reports through `orders_reconciled`.
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _symbols: Option<&[String]>) {
        self.reconcile_uncertain_placement(cx);
    }

    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(Err(error)) = self.options.secondary_validation {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            return;
        }
        self.discovery_read(cx, false);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if let Some(state) = self.sockets.iter_mut().find(|s| s.socket == Some(socket)) {
            if state.open {
                return;
            }
            state.open = true;
            state.authenticated = false;
            state.supervisor.opened(cx);
            state.ready = Some(cx.set_timer(1_000.0));
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(channel) = self.sockets.iter().position(|s| s.socket == Some(socket)) else {
            return;
        };
        self.sockets[channel].supervisor.received(cx.now);
        if channel == 0
            && let Ok(push) = serde_json::from_str::<TickerPush>(text)
            && push.code == 0
        {
            let TickerChannel::Ticker = push.result.channel;
            if let Some(row) = push.result.data.first() {
                cx.store
                    .update_ticker(push.result.instrument_name, |t| row.apply(t));
            }
            return;
        }
        let result = (|| {
            let envelope: Envelope<'_> = serde_json::from_str(text)?;
            if envelope.method == "public/heartbeat" {
                cx.send(
                    socket,
                    format!(
                        "{{\"id\":{},\"method\":\"public/respond-heartbeat\"}}",
                        envelope.id
                    ),
                );
            } else if channel == 1 && envelope.method == "public/auth" {
                let code = serde_json::from_str::<AuthenticationReply>(text)?.code;
                if code != 0 {
                    cx.notice(
                        Level::Error,
                        code::AUTH_REJECTED,
                        format!(
                            "Crypto.com private login rejected: {} {}",
                            envelope.code, envelope.message
                        ),
                    );
                    cx.close(socket);
                    self.on_close(cx, socket, 1008);
                } else if !self.sockets[channel].authenticated {
                    self.sockets[channel].authenticated = true;
                    self.sockets[channel].supervisor.authenticated(cx);
                    self.authenticated(cx, &envelope);
                }
            } else if envelope.code != 0 {
                cx.notice(
                    Level::Error,
                    code::SUBSCRIPTION_REJECTED,
                    format!(
                        "Crypto.com API Error {}: {}",
                        envelope.code, envelope.message
                    ),
                );
            } else if let Some(result) = envelope.result {
                // Subscribe acknowledgements may carry a result without a channel.
                if let Ok(message) = serde_json::from_str::<Message>(result.get()) {
                    if channel == 1 {
                        self.private_message(cx, message)?;
                    } else {
                        self.message(cx, message)?;
                    }
                }
            }
            Ok::<_, serde_json::Error>(())
        })();
        if let Err(error) = result {
            cx.notice(Level::Error, code::STREAM_MALFORMED, error.to_string());
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if let Some(channel) = self.sockets.iter().position(|s| s.socket == Some(socket)) {
            let state = &mut self.sockets[channel];
            state.open = false;
            state.authenticated = false;
            state.socket = None;
            if let Some(timer) = state.ready.take() {
                cx.clear_timer(timer);
            }
            state.supervisor.retry(cx);
            if channel == 0 {
                for state in self.books.values_mut() {
                    state.synced = false;
                }
            }
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        if let Some(context) = pending.discovery_context()
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            return;
        }
        if let Pending::Candles { context, .. } = &*pending
            && cx.validate_read_http_delivery(context, context).is_err()
        {
            return;
        }
        if let Pending::Account(request) = &*pending
            && self.account_delivery(cx, request).is_err()
        {
            return;
        }
        match pending {
            Pending::History(request) => self.history_item(cx, request, item),
            Pending::PublicTrades(trade) => trade.item(cx, item),
            Pending::Trade(_) | Pending::Identity(_) | Pending::BookQuery { .. } => {}
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::Markets {
                rows,
                indexes,
                skipped,
                malformed,
                ..
            } => {
                // Other instrument types carry fields this venue never reads.
                let key = serde_json::from_str::<InstrumentKey>(item);
                if key
                    .as_ref()
                    .is_ok_and(|k| k.inst_type != Some("PERPETUAL_SWAP"))
                {
                    return;
                }
                match serde_json::from_str::<Instrument>(item) {
                    Ok(row) => match row.market() {
                        Some(market) if market.precision.is_tradable() => {
                            indexes.insert(
                                market.id.clone(),
                                row.underlying_symbol.unwrap_or_default().into(),
                            );
                            rows.push(market);
                        }
                        Some(market) => {
                            skipped.add(Some(row.symbol));
                            malformed.insert(market.symbol);
                        }
                        None => {}
                    },
                    Err(_) => {
                        let symbol = key.ok().and_then(|k| k.symbol);
                        skipped.add(symbol);
                        if let Some(symbol) = symbol.filter(|s| !s.is_empty()) {
                            malformed.insert(symbol.into());
                        }
                    }
                }
            }
            Pending::Tickers { rows, skipped, .. } => {
                // Spot rows may contain null prices; they are outside this venue.
                match serde_json::from_str::<TickerName>(item) {
                    Ok(name) if cx.store.market(name.i).is_none() => {}
                    Err(_) => skipped.add(None),
                    Ok(name) => match serde_json::from_str::<TickerRow>(item) {
                        Ok(row) => {
                            let mut ticker = Ticker {
                                id: row.i.into(),
                                symbol: row.i.into(),
                                ..Default::default()
                            };
                            row.apply(&mut ticker);
                            rows.push(ticker);
                        }
                        Err(_) => skipped.add(Some(name.i)),
                    },
                }
            }
            Pending::Candles { rows, invalid, .. } => match serde_json::from_str::<CandleRow>(item)
            {
                Ok(row) => rows.push(row.candle()),
                Err(_) => *invalid = true,
            },
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let private_delivery = match &pending {
            Pending::Account(request) => self.account_delivery(cx, request),
            Pending::Identity(request) if !request.delivered(cx) => {
                Err(safe_cex_engine::request_context::Error::Revoked)
            }
            Pending::History(request) if !request.delivered(cx) => {
                Err(safe_cex_engine::request_context::Error::Revoked)
            }
            Pending::Trade(request) if !request.delivered(cx) => {
                Err(safe_cex_engine::request_context::Error::Revoked)
            }
            _ => Ok(()),
        };
        if let Err(error) = private_delivery {
            if let Pending::Account(request) = pending {
                self.account_failed(
                    cx,
                    request,
                    &error.read_venue_error().message,
                    reads::FailureRecovery::Strict,
                );
            }
            return;
        }
        if let Pending::PublicTrades(trade) = &pending
            && !trade.delivered(cx)
        {
            return;
        }
        if let Some(context) = pending.discovery_context()
            && let Err(error) = cx.validate_background_read_http_delivery(context)
        {
            self.on_request_failed(cx, pending, &error.venue_error().message);
            return;
        }
        if let Pending::Candles { context, .. } = &pending
            && cx.validate_read_http_delivery(context, context).is_err()
        {
            return;
        }
        if let Pending::BookQuery { context } = pending {
            self.book_query_response(cx, context, response);
            return;
        }
        if let Pending::Identity(request) = pending {
            self.identity_response(cx, request, response);
            return;
        }
        if let Pending::Trade(request) = pending {
            self.trade_response(cx, request, response);
            return;
        }
        let error = match serde_json::from_str::<RestEnvelope>(response.body) {
            Ok(e) if e.code != 0 => Some(format!("Crypto.com API Error {}: {}", e.code, e.message)),
            _ if !(200..300).contains(&response.status) => {
                Some(format!("Crypto.com HTTP error {}", response.status))
            }
            Ok(_) => None,
            Err(e) => Some(e.to_string()),
        };
        if let Some(error) = error {
            match pending {
                Pending::Account(request) => {
                    self.account_failed(cx, request, &error, reads::FailureRecovery::Response)
                }
                Pending::History(request) => self.history_failed(
                    cx,
                    request,
                    reads::response_failure(response, error),
                    reads::FailureRecovery::Response,
                ),
                Pending::PublicTrades(trade) => {
                    self.public_trades_response(cx, *trade, response, Some(error))
                }
                pending => self.on_request_failed(cx, pending, &error),
            }
            return;
        }
        match pending {
            Pending::Trade(_) | Pending::Identity(_) | Pending::BookQuery { .. } => unreachable!(),
            Pending::History(request) => self.history_response(cx, request),
            Pending::PublicTrades(trade) => self.public_trades_response(cx, *trade, response, None),
            Pending::Account(request) => self.account_response(cx, request),
            Pending::Markets {
                context,
                rows,
                indexes,
                skipped,
                malformed,
            } => {
                skipped.report(cx, "instrument");
                if rows.is_empty() {
                    let _ = cx.finish_http_context(&context);
                    if skipped.any() {
                        cx.retried_failure(
                            "cryptocom.lib:649",
                            "no mappable Crypto.com instrument rows",
                        );
                    } else {
                        cx.notice(
                            Level::Warn,
                            code::REQUEST_FAILED,
                            "Crypto.com instrument list has no usable rows",
                        );
                    }
                    self.discovery.failed(cx, false);
                    return;
                }
                let refresh = cx.store.loaded().markets;
                let rows = if refresh {
                    let mut indexes = indexes;
                    for (market, index) in std::mem::take(&mut self.indexes) {
                        indexes.entry(market).or_insert(index);
                    }
                    self.indexes = indexes;
                    keep_listed(cx.store.markets(), rows)
                } else {
                    self.indexes = indexes;
                    rows
                };
                cx.store.set_markets(rows);
                let before = self.malformed_markets.len();
                self.malformed_markets.extend(malformed);
                self.malformed_markets
                    .retain(|symbol| cx.store.market(symbol).is_none());
                let after = self.malformed_markets.len();
                // A partial catalogue commits so private reads start, then re-reads.
                if reread_partial(cx, &mut self.reread_stalls, refresh, before, after) {
                    self.discovery.failed(cx, false);
                } else {
                    self.discovery.succeeded(cx, &false);
                }
                let _ = cx.finish_http_context(&context);
                if refresh {
                    return;
                }
                self.discovery_read(cx, true);
                if self.credentials.is_some() {
                    self.private_start(cx);
                } else {
                    cx.store.set_balance(Balance::default());
                    cx.store.set_positions(vec![]);
                    cx.store.set_orders(vec![]);
                }
            }
            Pending::Tickers {
                context,
                rows,
                skipped,
            } => {
                skipped.report(cx, "ticker");
                if rows.is_empty() {
                    let _ = cx.finish_http_context(&context);
                    if skipped.any() {
                        cx.retried_failure(
                            "cryptocom.lib:704",
                            "no mappable Crypto.com ticker rows",
                        );
                    }
                    self.discovery.failed(cx, true);
                    return;
                }
                self.discovery.succeeded(cx, &true);
                cx.store.set_tickers(rows);
                let _ = cx.finish_http_context(&context);
                self.market_channels(cx);
                self.connect(cx, 0);
            }
            Pending::Candles {
                context,
                rows,
                invalid,
            } => {
                let result: Result<String, String> = if invalid {
                    Err("invalid candle row".into())
                } else {
                    Ok(serde_json::to_string(&rows).unwrap())
                };
                match result {
                    Ok(data) => {
                        let _ = cx.done_context(&context, data);
                    }
                    Err(error) => {
                        let _ = cx.fail_context(&context, error);
                    }
                }
            }
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        let missing_list = || {
            safe_cex_engine::VenueError::new(format!(
                "response did not contain the expected list: {}",
                response.body
            ))
            .kind(safe_cex_engine::ErrorKind::Internal)
        };
        let recovery = if response.body.contains("40101") {
            reads::FailureRecovery::Response
        } else {
            reads::FailureRecovery::Strict
        };
        match pending {
            Pending::Identity(request) => self.identity_reject(cx, request, missing_list()),
            Pending::History(request) => self.history_failed(cx, request, missing_list(), recovery),
            Pending::Account(request) => {
                self.account_failed(cx, request, &missing_list().message, recovery)
            }
            Pending::PublicTrades(trade) => self.public_trades_stream_failed(cx, *trade, response),
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
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Identity(request) => self.identity_reject(cx, request, error),
            Pending::History(request) => self.history_reject(cx, request, error),
            Pending::Trade(request) => self.trade_reject(cx, request, error),
            Pending::Account(request) => {
                self.account_failed(cx, request, &error.message, reads::FailureRecovery::Strict)
            }
            Pending::PublicTrades(trade) => Self::public_trades_reject(cx, *trade, error),
            Pending::Candles { context, .. } => {
                let _ = cx.fail_context(&context, error.outcome(safe_cex_engine::Outcome::NotSent));
            }
            Pending::BookQuery { context } => {
                let _ = cx.fail_context(&context, error);
            }
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::Identity(request) => self.identity_failed(cx, request, error),
            Pending::History(request) => {
                let recovery = reads::FailureRecovery::released(&error);
                self.history_failed(cx, request, error, recovery);
            }
            Pending::Account(request) => {
                let recovery = reads::FailureRecovery::released(&error);
                self.account_failed(cx, request, &error.message, recovery);
            }
            Pending::PublicTrades(trade) => self.public_trades_failed(cx, *trade, error, true),
            Pending::BookQuery { context } | Pending::Candles { context, .. } => {
                let _ = cx.fail_context(&context, error.outcome(safe_cex_engine::Outcome::NotSent));
            }
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Some(context) = pending.discovery_context() {
            // An admitted failure keeps native retry even after sibling issue
            // has pruned expired Context retirement metadata.
            let _ = cx.finish_http_context(context);
        }
        match pending {
            Pending::BookQuery { context } => {
                let _ = cx.fail_context(
                    &context,
                    depth_query::failure(error, safe_cex_engine::ErrorKind::Network),
                );
            }
            Pending::Identity(request) => self.identity_reject(
                cx,
                request,
                safe_cex_engine::VenueError::new(error).kind(safe_cex_engine::ErrorKind::Internal),
            ),
            Pending::Trade(request) => self.trade_failed(
                cx,
                request,
                safe_cex_engine::VenueError::new(error).kind(safe_cex_engine::ErrorKind::Network),
            ),
            Pending::History(request) => self.history_reject(
                cx,
                request,
                safe_cex_engine::VenueError::new(error).kind(safe_cex_engine::ErrorKind::Internal),
            ),
            Pending::PublicTrades(trade) => self.public_trades_failed(
                cx,
                *trade,
                safe_cex_engine::VenueError::new(error).kind(safe_cex_engine::ErrorKind::Network),
                true,
            ),
            Pending::Account(request) => {
                self.account_failed(cx, request, error, reads::FailureRecovery::Strict)
            }
            Pending::Candles { context, .. } => {
                if cx.validate_read_http_delivery(&context, &context).is_ok() {
                    let _ = cx.fail_context(&context, error);
                }
            }
            Pending::Markets { .. } => self.discovery.failed(cx, false),
            Pending::Tickers { .. } => self.discovery.failed(cx, true),
        }
    }
    fn read_root_retired(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
        _pending: Vec<Pending>,
    ) {
        self.history
            .waiting
            .retain(|_, request| request.root().owner() != owner);
        self.public_trades
            .waiting
            .retain(|_, trade| trade.root().owner() != owner);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(trade) = self.public_trades.waiting.remove(&timer) {
            self.public_trades_request(cx, trade);
            return;
        }
        if let Some(tickers) = self.discovery.on_timer(timer) {
            self.discovery_read(cx, tickers);
            return;
        }
        if let Some(request) = self.history.waiting.remove(&timer) {
            self.history_request(cx, request);
            return;
        }
        if self.trading.timer == Some(timer) {
            self.trading.timer = None;
            self.drive_trading(cx);
            return;
        }
        if self.account.timer == Some(timer) {
            self.account.timer = None;
            self.reconcile(cx);
            return;
        }
        for channel in 0..2 {
            match self.sockets[channel].supervisor.on_timer(cx, timer) {
                Tick::Reconnect => {
                    self.connect(cx, channel);
                    return;
                }
                Tick::TimedOut(_) => {
                    if let Some(socket) = self.sockets[channel].socket {
                        cx.close(socket);
                        self.on_close(cx, socket, 1006);
                    }
                    return;
                }
                Tick::Handled | Tick::Ping => return,
                Tick::Unhandled => {}
            }
            if self.sockets[channel].ready == Some(timer) {
                self.sockets[channel].ready = None;
                if channel == 1 {
                    self.authenticate(cx);
                }
                if channel == 0 {
                    self.subscribe(
                        cx,
                        &self.subscriptions.keys().cloned().collect::<Vec<_>>(),
                        "subscribe",
                    );
                }
                return;
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::FetchOrderBook { symbol, .. } => {
                self.fetch_book(cx, &symbol);
            }
            Command::SubscribeOrderBook { symbol } => {
                let state = self
                    .books
                    .entry(symbol.clone())
                    .or_insert_with(|| BookState {
                        book: Book::default(),
                        subscribers: 0,
                        synced: false,
                    });
                state.subscribers += 1;
                self.add_channel(cx, format!("book.{symbol}.50"));
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers -= 1;
                    if state.subscribers == 0 {
                        self.books.remove(&symbol);
                    }
                }
                self.remove_channel(cx, format!("book.{symbol}.50"));
            }
            Command::SubscribeCandles {
                symbol,
                interval: timeframe,
            } => match interval(timeframe) {
                Ok(timeframe) => self.add_channel(cx, format!("candlestick.{timeframe}.{symbol}")),
                Err(error) => cx.notice(Level::Error, code::COMMAND_UNSUPPORTED, error),
            },
            Command::UnsubscribeCandles {
                symbol,
                interval: timeframe,
            } => {
                if let Ok(timeframe) = interval(timeframe) {
                    self.remove_channel(cx, format!("candlestick.{timeframe}.{symbol}"));
                }
            }
            Command::FetchCandles {
                symbol,
                interval: timeframe,
                from_ms,
                to_ms,
                ..
            } => {
                let Ok(context) = cx.admit_http_command(read_policy()) else {
                    return;
                };
                match interval(timeframe) {
                    Ok(timeframe) => {
                        let mut path = format!(
                            "/public/get-candlestick?instrument_name={}&timeframe={timeframe}",
                            encode(&symbol)
                        );
                        if let Some(from) = from_ms {
                            path.push_str(&format!("&start_ts={from}"));
                        }
                        if let Some(to) = to_ms {
                            path.push_str(&format!("&end_ts={to}"));
                        }
                        match cx.request_managed(
                            context.clone(),
                            None,
                            Method::Get,
                            format!("{}{path}", self.rest),
                            None,
                            Pending::Candles {
                                context,
                                rows: vec![],
                                invalid: false,
                            },
                        ) {
                            Ok(id) => cx.stream_response(id, ArrayAt::Path(&["result", "data"])),
                            Err((pending, reason)) => {
                                self.on_sign_error(cx, pending, reason.venue_error())
                            }
                        }
                    }
                    Err(error) => {
                        let _ = cx.fail_context(&context, error);
                    }
                }
            }
            Command::FetchAccount { command } => self.fetch_identity(cx, command),
            Command::Query { command, query } => self.query(cx, command, query),
            command => self.trade_command(cx, command),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|s| s.synced).map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .map(|(symbol, s)| (symbol.as_str(), &s.book)),
        )
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
            format!("Crypto.com stops re-reading a catalogue with {after} malformed markets"),
        );
    }
    *stalls < 5
}
