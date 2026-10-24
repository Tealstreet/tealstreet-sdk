//! MEXC USDT/USDC perpetual futures, driven by the sans-I/O engine.
mod account;
mod depth_query;
mod history;
mod identity;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::Level;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::socket::{self, Config, Supervisor};
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, PublicFeedPolicy, Response, SocketId,
    TimerId, Venue, VenueError,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use wire::*;

const REST: &str = "https://api.mexc.com";
const WS: &str = "wss://contract.mexc.com/edge";
const PING: &str = r#"{"method":"ping"}"#;
const MAX_BUFFERED: usize = 2_000;

#[derive(Default)]
pub struct Options {
    /// Optional tighter signing receive window in milliseconds.
    pub recv_window: Option<u32>,
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
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
    History(history::Request),
    Identity(CommandId),
    Trade(trading::Request),
    Account(account::Request),
    Markets {
        context: Context,
        rows: Vec<Market>,
        malformed: HashSet<String>,
    },
    Tickers {
        context: Context,
        rows: Vec<Ticker>,
        failed: bool,
    },
    Depth {
        context: Context,
        symbol: String,
        generation: u32,
    },
    BookQuery {
        context: Context,
        size: f64,
        limit: usize,
    },
    Candles {
        context: Context,
        limit: usize,
    },
}

impl Pending {
    fn public_context(&self) -> Option<&Context> {
        match self {
            Self::Markets { context, .. }
            | Self::Tickers { context, .. }
            | Self::Depth { context, .. } => Some(context),
            _ => None,
        }
    }
}

#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
}

struct BookState {
    count: u32,
    generation: u32,
    book: Book,
    version: Option<u64>,
    buffering: Option<Vec<Depth>>,
    retry: Option<TimerId>,
    failures: u32,
}

pub struct MexcFutures {
    options: safe_cex_crypto::secrets::Sensitive<Options>,
    credentials: Option<sign::Credentials>,
    account: account::Account,
    /// Regular, plan and stop order reads share the orders scope.
    orders_auth: safe_cex_engine::MergedScopeAuth,
    trading: trading::Trading,
    public: Connection,
    public_supervisor: Supervisor,
    books: HashMap<String, BookState>,
    volume_units: HashMap<String, f64>,
    ticker_symbols: Vec<String>,
    ticker_subscriptions: BTreeSet<String>,
    candles: HashMap<(String, Interval), u32>,
    next_book_generation: u32,
    discovery: safe_cex_engine::startup::Backoff<bool>,
    // Catalogue rows rejected as malformed; owned exposure there holds the snapshot.
    malformed_markets: HashSet<String>,
    reread_stalls: u8,
    depth_paused_until: f64,
}

impl MexcFutures {
    fn fail_local_read(cx: &mut Cx<'_, Pending>, error: &str) {
        let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) else {
            return;
        };
        let _ = cx.fail_context(&context, error);
    }

    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        let credentials = match (&options.key, &options.secret) {
            (Some(key), Some(secret)) if !options.public_data_only && !key.trim().is_empty() => {
                Some(sign::Credentials {
                    key: key.trim().to_owned(),
                    secret: secret.clone(),
                })
            }
            _ => None,
        };
        Self {
            options,
            credentials,
            account: account::Account::new(),
            orders_auth: Default::default(),
            trading: trading::Trading::default(),
            public: Connection::default(),
            public_supervisor: Supervisor::new(Config {
                idle_timeout_ms: 20_000.0,
                ping_interval_ms: Some(8_000.0),
                ..Config::default()
            }),
            books: HashMap::new(),
            volume_units: HashMap::new(),
            ticker_symbols: vec![],
            ticker_subscriptions: BTreeSet::new(),
            candles: HashMap::new(),
            next_book_generation: 0,
            discovery: Default::default(),
            malformed_markets: HashSet::new(),
            reread_stalls: 0,
            depth_paused_until: 0.0,
        }
    }
    fn get(&mut self, cx: &mut Cx<'_, Pending>, path: &str, symbol: String, generation: u32) {
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
            Err(_) => {
                self.resync(cx, &symbol);
                return;
            }
        };
        let pending = Pending::Depth {
            context: context.clone(),
            symbol,
            generation,
        };
        if let Err((pending, error)) =
            cx.request_managed(context, Method::Get, format!("{REST}{path}"), None, pending)
        {
            self.failure(cx, pending, &error.venue_error().message);
        }
    }
    fn stream(&mut self, cx: &mut Cx<'_, Pending>, path: &str, tickers: bool) {
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
            Err(error) => {
                cx.notice(
                    Level::Warn,
                    code::REQUEST_FAILED,
                    error.venue_error().message,
                );
                self.discovery.failed(cx, tickers);
                return;
            }
        };
        let pending = if tickers {
            Pending::Tickers {
                context: context.clone(),
                rows: vec![],
                failed: false,
            }
        } else {
            Pending::Markets {
                context: context.clone(),
                rows: vec![],
                malformed: HashSet::new(),
            }
        };
        match cx.request_managed(context, Method::Get, format!("{REST}{path}"), None, pending) {
            Ok(request) => cx.stream_response(request, ArrayAt::EnvelopeField("data")),
            Err((pending, error)) => self.failure(cx, pending, &error.venue_error().message),
        }
    }
    fn send(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: &str,
        symbol: &str,
        extra: Option<(&str, &str)>,
    ) {
        if !self.public.open {
            return;
        }
        let Some(socket) = self.public.socket else {
            return;
        };
        #[derive(serde::Serialize)]
        struct Subscription<'a> {
            method: &'a str,
            param: std::collections::BTreeMap<&'a str, &'a str>,
        }
        let mut param = std::collections::BTreeMap::from([("symbol", symbol)]);
        if let Some((k, v)) = extra {
            param.insert(k, v);
        }
        cx.send(
            socket,
            serde_json::to_string(&Subscription { method, param }).unwrap(),
        );
    }
    fn reconcile_tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.public.open {
            return;
        }
        let ids: Vec<_> = if self.ticker_symbols.is_empty() {
            cx.store
                .markets()
                .iter()
                .take(100)
                .map(|m| m.id.clone())
                .collect()
        } else {
            self.ticker_symbols
                .iter()
                .filter_map(|symbol| cx.store.market(symbol).map(|m| m.id.clone()))
                .collect()
        };
        let desired: BTreeSet<_> = ids.iter().cloned().collect();
        for id in self.ticker_subscriptions.difference(&desired) {
            self.send(cx, "unsub.ticker", id, None);
        }
        // Keep market/request order on the wire while deduplicating ids.
        self.ticker_subscriptions.retain(|id| desired.contains(id));
        for id in ids {
            if self.ticker_subscriptions.insert(id.clone()) {
                self.send(cx, "sub.ticker", &id, None);
            }
        }
    }
    fn mini_ticker(&self, cx: &mut Cx<'_, Pending>, item: &str) {
        #[derive(serde::Deserialize)]
        struct Identity<'a> {
            #[serde(borrow)]
            symbol: Str<'a>,
        }
        let Ok(identity) = serde_json::from_str::<Identity<'_>>(item) else {
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                "Skipping malformed MEXC all-market ticker identity",
            );
            return;
        };
        let symbol = compact(&identity.symbol);
        let Some(market) = cx.store.market(&symbol) else {
            return;
        };
        let size = market.contract_size.unwrap_or(1.0);
        let Ok(raw) = serde_json::from_str::<MiniTick>(item) else {
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                "Skipping malformed MEXC all-market ticker row",
            );
            return;
        };
        if [raw.last_price, raw.fair_price, raw.index_price]
            .into_iter()
            .flatten()
            .any(|n| n < 0.0)
            || [raw.volume24, raw.amount24]
                .into_iter()
                .flatten()
                .any(|n| n < 0.0)
        {
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                "Skipping invalid MEXC all-market ticker row",
            );
            return;
        }
        cx.store.update_ticker(&symbol, |t| {
            // A zero price is an unavailable quote (seen in the public feed),
            // not a reason to discard valid mark/index/volume fields.
            if let Some(last) = raw.last_price.filter(|n| *n > 0.0) {
                t.last = last;
            }
            if let Some(mark) = raw.fair_price.filter(|n| *n > 0.0) {
                // The all-market feed omits holdVol: retain the known quantity,
                // but value its quote notional at the new mark.
                if t.mark > 0.0 {
                    t.open_interest *= mark / t.mark;
                }
                t.mark = mark;
            }
            if let Some(index) = raw.index_price.filter(|n| *n > 0.0) {
                t.index = index;
            }
            if let Some(rate) = raw.rise_fall_rate {
                t.percentage = if rate.abs() <= 1.0 {
                    rate * 100.0
                } else {
                    rate
                };
            }
            if let Some(volume) = raw.volume24 {
                t.volume = volume * size;
            }
            if let Some(volume) = raw.amount24 {
                t.quote_volume = volume;
            }
        });
    }
    fn subscribe_depth(&self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if let (Some(socket), Some(market)) = (self.public.socket, cx.store.market(symbol))
            && self.public.open
        {
            cx.send(
                socket,
                format!(
                    r#"{{"method":"sub.depth","param":{{"symbol":"{}","compress":false}}}}"#,
                    market.id
                ),
            );
        }
    }
    fn snapshot(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if !self.public.open {
            return;
        }
        let Some(market) = cx.store.market(symbol) else {
            return;
        };
        let path = format!("/api/v1/contract/depth/{}?limit=1000", market.id);
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        if state.buffering.is_some() || state.retry.is_some() {
            return;
        }
        if cx.now < self.depth_paused_until {
            state.retry = Some(cx.set_timer(self.depth_paused_until - cx.now));
            return;
        }
        self.next_book_generation = self.next_book_generation.wrapping_add(1);
        state.generation = self.next_book_generation;
        state.version = None;
        state.buffering = Some(vec![]);
        let generation = state.generation;
        self.get(cx, &path, symbol.into(), generation);
    }
    fn resync(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        state.version = None;
        state.buffering = None;
        state.generation = state.generation.wrapping_add(1);
        state.failures += 1;
        if let Some(timer) = state.retry.take() {
            cx.clear_timer(timer);
        }
        // Book resync stays on its nominal schedule: its timing decides which buffered
        // deltas apply to the snapshot, and it is per symbol, not a shared retry herd.
        state.retry = Some(cx.set_timer(Backoff::new(250.0, 7, 30_000.0).delay(state.failures)));
    }
    fn apply_depth(book: &mut Book, depth: &Depth, size: f64) {
        for (side, rows) in [(BookSide::Bids, &depth.bids), (BookSide::Asks, &depth.asks)] {
            for Level(price, amount, _) in rows {
                if *price > 0.0 && *amount >= 0.0 {
                    book.apply(side, *price, *amount * size);
                }
            }
        }
    }
    fn depth(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, depth: Depth) {
        if depth
            .bids
            .iter()
            .chain(&depth.asks)
            .any(|Level(p, v, _)| *p <= 0.0 || *v < 0.0)
        {
            self.resync(cx, symbol);
            return;
        }
        let size = cx
            .store
            .market(symbol)
            .and_then(|m| m.contract_size)
            .unwrap_or(1.0);
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        if let Some(buffer) = &mut state.buffering {
            buffer.push(depth);
            if buffer.len() > MAX_BUFFERED {
                self.resync(cx, symbol);
            }
            return;
        }
        let Some(version) = state.version else {
            return;
        };
        if depth.version <= version {
            return;
        }
        if depth.version != version + 1 {
            self.resync(cx, symbol);
            return;
        }
        Self::apply_depth(&mut state.book, &depth, size);
        state.version = Some(depth.version);
        state.failures = 0;
        cx.book_changed(symbol);
    }
    fn ticker(raw: Tick<'_>, market: &Market) -> Ticker {
        let size = market.contract_size.unwrap_or(1.0);
        let mark = if raw.fair_price == 0.0 {
            raw.last_price
        } else {
            raw.fair_price
        };
        Ticker {
            id: market.id.clone(),
            symbol: market.symbol.clone(),
            bid: if raw.bid1 == 0.0 {
                raw.last_price
            } else {
                raw.bid1
            },
            ask: if raw.ask1 == 0.0 {
                raw.last_price
            } else {
                raw.ask1
            },
            last: raw.last_price,
            mark,
            index: if raw.index_price == 0.0 {
                raw.last_price
            } else {
                raw.index_price
            },
            percentage: if raw.rise_fall_rate.abs() <= 1.0 {
                raw.rise_fall_rate * 100.0
            } else {
                raw.rise_fall_rate
            },
            open_interest: raw.hold_vol * size * mark,
            funding_rate: raw.funding_rate,
            funding_time: millis(raw.funding_time),
            volume: raw.volume24 * size,
            quote_volume: raw.amount24,
            mid_price: None,
        }
    }
    /// `Ok(None)` is a deliberately filtered contract; `Err` a malformed one.
    fn market(raw: Contract<'_>) -> Result<Option<Market>, ()> {
        let settle = if raw.settle_coin.is_empty() {
            &raw.quote_coin
        } else {
            &raw.settle_coin
        };
        if !matches!(settle.as_ref(), "USDT" | "USDC")
            || raw.api_allowed == Some(false)
            || raw.is_hidden == Some(true)
            || raw.state != 0.0
        {
            return Ok(None);
        }
        let step = (raw.contract_size * raw.vol_unit).max(raw.contract_size);
        let precision = Precision {
            amount: step,
            price: raw.price_unit,
        };
        if raw.contract_size <= 0.0 || raw.vol_unit <= 0.0 || !precision.is_tradable() {
            return Err(());
        }
        Ok(Some(Market {
            symbol: compact(&raw.symbol),
            pretty_symbol: Some(raw.symbol.to_string()),
            id: raw.symbol.into_owned(),
            base: raw.base_coin.into_owned(),
            quote: raw.quote_coin.into_owned(),
            active: true,
            precision,
            limits: Limits {
                amount: MinMax {
                    min: (raw.min_vol * raw.contract_size).max(step),
                    max: raw.max_vol * raw.contract_size,
                },
                leverage: MinMax {
                    min: raw.min_leverage,
                    max: raw.max_leverage,
                },
                price: None,
            },
            contract_size: Some(raw.contract_size),
            maker_fee: Some(raw.maker_fee_rate),
            taker_fee: Some(raw.taker_fee_rate),
            is_inverse: None,
            is_perpetual: Some(true),
            is_expiring: None,
            is_spot: None,
            is_read_only: None,
        }))
    }
    fn candle(raw: Kline<'_>) -> Candle {
        Candle {
            timestamp: (millis(raw.t) / 1000.0).floor(),
            open: raw.o,
            high: raw.h,
            low: raw.l,
            close: raw.c,
            volume: raw.q,
        }
    }
    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        // Suppressed public feeds never open; supervising them would only churn retries.
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public_supervisor.connecting(cx);
        }
        self.public.socket = Some(cx.connect(
            safe_cex_engine::SocketSpec::new(WS).channel(safe_cex_engine::SocketChannel::Public),
        ));
    }
    // The account already started when markets loaded, independent of tickers.
    fn public_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        self.connect_public(cx);
        if self.options.public_data_only || self.options.key.as_ref().is_none_or(|s| s.is_empty()) {
            cx.store.set_orders(vec![]);
            cx.store.set_positions(vec![]);
            cx.store.set_balance(Default::default());
        }
    }
    fn discovery_read(&mut self, cx: &mut Cx<'_, Pending>, tickers: bool) {
        self.stream(
            cx,
            if tickers {
                "/api/v1/contract/ticker"
            } else {
                "/api/v1/contract/detail"
            },
            tickers,
        );
    }
    fn failure(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Some(context) = pending.public_context()
            && cx.finish_http_context(context).is_err()
        {
            return;
        }
        match pending {
            Pending::Identity(command) => self.identity_failed(cx, command, error),
            Pending::Depth {
                symbol, generation, ..
            } => {
                if self
                    .books
                    .get(&symbol)
                    .is_some_and(|s| s.generation == generation)
                {
                    self.resync(cx, &symbol);
                }
            }
            Pending::Candles { context, .. } => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::BookQuery { context, .. } => {
                let _ = cx.fail_context(
                    &context,
                    VenueError::new(error)
                        .kind(ErrorKind::Network)
                        .outcome(Outcome::NotSent),
                );
            }
            Pending::Account(request) => self.account_failed(cx, request, error),
            Pending::Trade(request) => {
                let error = VenueError::new(error).kind(ErrorKind::Network);
                // The amend read precedes the replacement, which was never sent.
                let error = if matches!(request.call, trading::Call::AmendRead { .. }) {
                    error.outcome(Outcome::NotSent)
                } else {
                    error.outcome(Outcome::Unknown)
                };
                self.trade_failed(cx, request.command, error)
            }
            Pending::History(request) => self.query_failed(cx, request, error),
            Pending::Markets { .. } | Pending::Tickers { .. } => {
                cx.notice(Level::Warn, code::REQUEST_FAILED, error);
                self.discovery
                    .failed(cx, matches!(pending, Pending::Tickers { .. }));
            }
        }
    }
}

impl Venue for MexcFutures {
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

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            quantity_units: safe_cex_engine::QuantityUnits::NativePlacement,
            leverage_query: safe_cex_engine::LeverageQuery::OnMetadata,
            order_book_query: true,
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            return;
        }
        self.discovery_read(cx, false);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.private.socket == Some(socket) {
            self.account_open(cx, socket);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public_supervisor.opened(cx);
        cx.send(socket, PING);
        // Sparse once-per-second prices cover every listed market. Detailed
        // subscriptions provide funding, index and open-interest quantities.
        cx.send(
            socket,
            r#"{"method":"sub.tickers","param":{},"gzip":false}"#,
        );
        self.reconcile_tickers(cx);
        let symbols: Vec<_> = self.books.keys().cloned().collect();
        for symbol in symbols {
            self.subscribe_depth(cx, &symbol);
            self.snapshot(cx, &symbol);
        }
        for (symbol, interval) in self.candles.keys() {
            if let (Some(m), Some(i)) = (cx.store.market(symbol), wire::interval(*interval)) {
                let id = m.id.clone();
                self.send(cx, "sub.kline", &id, Some(("interval", i)));
            }
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.private.socket == Some(socket) {
            self.account_message(cx, text);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public_supervisor.received(cx.now);
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        let Some(data) = message.data else {
            return;
        };
        if message.success == Some(false) {
            cx.notice(
                Level::Warn,
                code::SUBSCRIPTION_REJECTED,
                "MEXC websocket request failed",
            );
            return;
        }
        let symbol = compact(&message.symbol);
        match message.channel.as_ref() {
            "push.tickers" => {
                let _ = safe_cex_engine::json::each::<&serde_json::value::RawValue>(
                    data.get(),
                    |row| self.mini_ticker(cx, row.get()),
                );
            }
            "push.ticker" => {
                if let (Ok(raw), Some(market)) = (
                    serde_json::from_str::<Tick<'_>>(data.get()),
                    cx.store.market(&symbol),
                ) {
                    let ticker = Self::ticker(raw, market);
                    cx.store.update_ticker(&symbol, |t| *t = ticker);
                }
            }
            "push.depth" => match serde_json::from_str(data.get()) {
                Ok(depth) => self.depth(cx, &symbol, depth),
                Err(_) => self.resync(cx, &symbol),
            },
            "push.kline" => {
                if let Ok(raw) = serde_json::from_str::<Kline<'_>>(data.get()) {
                    let interval = self
                        .candles
                        .keys()
                        .find(|(s, i)| {
                            s == &symbol && wire::interval(*i) == Some(raw.interval.as_ref())
                        })
                        .map(|(_, i)| *i);
                    if let Some(interval) = interval {
                        cx.candle(&symbol, interval, Self::candle(raw));
                    }
                }
            }
            _ => {}
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account.private.socket == Some(socket) {
            self.account_close(cx, socket);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.socket = None;
        self.public.open = false;
        self.ticker_subscriptions.clear();
        self.public_supervisor.retry(cx);
        for state in self.books.values_mut() {
            state.version = None;
            state.buffering = None;
            state.generation = state.generation.wrapping_add(1);
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        if let Some(context) = pending.public_context()
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            return;
        }
        match pending {
            Pending::History(request) => self.query_item(cx, request, item),
            Pending::Account(account::Request::Leverage { rows, bad, .. }) => {
                match serde_json::from_str::<wire::LeverageRow>(item) {
                    Ok(r) => rows.push(r),
                    Err(_) => *bad = true,
                }
            }
            Pending::Account(account::Request::Snapshot(snapshot)) => {
                self.account_item(cx, snapshot, item)
            }
            Pending::Markets {
                rows, malformed, ..
            } => match serde_json::from_str::<Contract<'_>>(item) {
                Ok(raw) => {
                    let unit = raw.vol_unit;
                    let symbol = compact(&raw.symbol);
                    match Self::market(raw) {
                        Ok(Some(m)) => {
                            self.volume_units.insert(m.symbol.clone(), unit);
                            rows.push(m);
                        }
                        Ok(None) => {}
                        Err(()) => {
                            safe_cex_engine::startup::skipped_row(cx);
                            malformed.insert(symbol);
                        }
                    }
                }
                Err(_) => {
                    safe_cex_engine::startup::skipped_row(cx);
                    if let Ok(row) = serde_json::from_str::<ContractIdentity<'_>>(item)
                        && !row.symbol.is_empty()
                    {
                        malformed.insert(compact(&row.symbol));
                    }
                }
            },
            Pending::Tickers { rows, .. } => match serde_json::from_str::<Tick<'_>>(item) {
                Ok(raw) => {
                    if let Some(m) = cx.store.market_by_id(&raw.symbol) {
                        rows.push(Self::ticker(raw, m));
                    }
                }
                Err(_) => safe_cex_engine::startup::skipped_row(cx),
            },
            _ => {}
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Trade(request) = pending {
            self.trade_response(cx, request, response);
            return;
        }
        if let Pending::BookQuery {
            context,
            size,
            limit,
        } = pending
        {
            self.book_query_response(cx, context, size, limit, response);
            return;
        }
        if matches!(&pending, Pending::Markets { .. } | Pending::Tickers { .. })
            && let Err(error) = Envelope::checked(response.body)
        {
            self.failure(cx, pending, &error);
            return;
        }
        if response.status == 429 {
            self.depth_paused_until = cx.now
                + response
                    .header("retry-after")
                    .and_then(|s| s.parse::<f64>().ok())
                    .map_or(10_000.0, |s| s * 1000.0);
        }
        if response.status == 429 {
            self.account.paused_until = self.depth_paused_until;
        }
        if let Some(context) = pending.public_context()
            && let Err(error) = cx.validate_background_read_http_delivery(context)
        {
            self.failure(cx, pending, &error.venue_error().message);
            return;
        }
        // Account reads classify their own HTTP failures (a refused key ends startup).
        if response.status != 200 && !matches!(pending, Pending::Account(_)) {
            self.failure(cx, pending, &format!("MEXC HTTP {}", response.status));
            return;
        }
        let background = pending.public_context().cloned();
        (|| match pending {
            Pending::Identity(command) => self.identity_response(cx, command, response),
            Pending::Account(request) => {
                let ok = response.status == 200;
                self.account_response(cx, request, response);
                if ok {
                    self.refresh_stop_orders(cx);
                }
            }
            Pending::Trade(_) => unreachable!(),
            Pending::History(request) => self.query_response(cx, request, response),
            Pending::Markets {
                rows, malformed, ..
            } => {
                if rows.is_empty() {
                    cx.notice(
                        Level::Warn,
                        code::REQUEST_FAILED,
                        "Malformed MEXC market list",
                    );
                    self.discovery.failed(cx, false);
                    return;
                }
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
                    .retain(|symbol| cx.store.market(symbol).is_none());
                let after = self.malformed_markets.len();
                // A partial catalogue commits so private reads start, then re-reads.
                if reread_partial(cx, &mut self.reread_stalls, refresh, before, after) {
                    self.discovery.failed(cx, false);
                } else {
                    self.discovery.succeeded(cx, &false);
                }
                if refresh {
                    return;
                }
                self.account_start(cx);
                self.discovery_read(cx, true);
            }
            Pending::Tickers {
                mut rows, failed, ..
            } => {
                if failed || rows.is_empty() {
                    cx.notice(
                        Level::Warn,
                        code::REQUEST_FAILED,
                        "Malformed MEXC ticker list",
                    );
                    self.discovery.failed(cx, true);
                    return;
                }
                rows.sort_by_key(|t| cx.store.market_index(&t.symbol));
                self.discovery.succeeded(cx, &true);
                cx.store.set_tickers(rows);
                self.public_ready(cx);
            }
            Pending::Depth {
                symbol, generation, ..
            } => {
                let depth = Envelope::checked(response.body).and_then(|e| {
                    let snapshot: DepthSnapshot =
                        serde_json::from_str(e.payload()?).map_err(|e| e.to_string())?;
                    if snapshot
                        .bids
                        .iter()
                        .chain(&snapshot.asks)
                        .any(|Level(p, v, _)| *p <= 0.0 || *v < 0.0)
                    {
                        return Err("Invalid MEXC depth snapshot level".into());
                    }
                    Ok(Depth {
                        version: snapshot.version,
                        bids: snapshot.bids,
                        asks: snapshot.asks,
                    })
                });
                let Some(state) = self.books.get_mut(&symbol) else {
                    return;
                };
                if state.generation != generation {
                    return;
                }
                let Ok(depth) = depth else {
                    self.resync(cx, &symbol);
                    return;
                };
                let size = cx
                    .store
                    .market(&symbol)
                    .and_then(|m| m.contract_size)
                    .unwrap_or(1.0);
                state.book.clear();
                Self::apply_depth(&mut state.book, &depth, size);
                state.version = Some(depth.version);
                let buffer = state.buffering.take().unwrap_or_default();
                for update in buffer {
                    self.depth(cx, &symbol, update);
                }
                if self.book(&symbol).is_some() {
                    cx.book_changed(&symbol);
                }
            }
            Pending::BookQuery { .. } => unreachable!(),
            Pending::Candles { context, limit } => {
                let result = Envelope::checked(response.body).and_then(|e| {
                    let k: Klines =
                        serde_json::from_str(e.payload()?).map_err(|e| e.to_string())?;
                    let n = k.time.len();
                    if [
                        k.open.len(),
                        k.high.len(),
                        k.low.len(),
                        k.close.len(),
                        k.vol.len(),
                    ]
                    .iter()
                    .any(|&l| l != n)
                    {
                        return Err("Malformed MEXC klines".into());
                    }
                    let rows: Vec<Candle> = (n.saturating_sub(limit)..n)
                        .map(|i| Candle {
                            timestamp: (millis(k.time[i]) / 1000.0).floor(),
                            open: k.open[i],
                            high: k.high[i],
                            low: k.low[i],
                            close: k.close[i],
                            volume: k.vol[i],
                        })
                        .collect();
                    serde_json::to_string(&rows).map_err(|e| e.to_string())
                });
                match result {
                    Ok(data) => {
                        let _ = cx.done_context(&context, data);
                    }
                    Err(error) => {
                        let _ = cx.fail_context(&context, error);
                    }
                };
            }
        })();
        if let Some(context) = background {
            let _ = cx.finish_http_context(&context);
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        if let Pending::Account(request) = &pending
            && self.account_startup_refusal(cx, request, &response)
        {
            return;
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
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::Candles { context, .. } => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::BookQuery { context, .. } => {
                let _ = cx.fail_context(&context, error.outcome(Outcome::NotSent));
            }
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::Candles { context, .. } | Pending::BookQuery { context, .. } => {
                let _ = cx.fail_context(&context, error.outcome(Outcome::NotSent));
            }
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.failure(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.account_timer(cx, timer);
        self.trade_timer(cx, timer);
        if let Some(tickers) = self.discovery.on_timer(timer) {
            self.discovery_read(cx, tickers);
            return;
        }
        match self.public_supervisor.on_timer(cx, timer) {
            socket::Tick::Reconnect => self.connect_public(cx),
            socket::Tick::Ping => {
                if let Some(socket) = self.public.socket {
                    cx.send(socket, PING);
                }
            }
            socket::Tick::TimedOut(_) => {
                if let Some(socket) = self.public.socket {
                    cx.close(socket);
                    self.on_close(cx, socket, 1006);
                }
            }
            _ => (),
        }
        let symbols: Vec<_> = self
            .books
            .iter()
            .filter(|(_, s)| s.retry == Some(timer))
            .map(|(s, _)| s.clone())
            .collect();
        for symbol in symbols {
            self.books.get_mut(&symbol).unwrap().retry = None;
            self.snapshot(cx, &symbol);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::FetchAccount { command } => self.fetch_identity(cx, command),
            Command::SetTickerSymbols { symbols } => {
                self.ticker_symbols = symbols;
                self.reconcile_tickers(cx);
            }
            Command::Query { command, query } => self.start_query(cx, command, query),
            Command::FetchLeverage { command, symbol } => self.fetch_leverage(cx, command, symbol),
            Command::SubscribeOrderBook { symbol } => {
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.count += 1;
                    return;
                }
                self.books.insert(
                    symbol.clone(),
                    BookState {
                        count: 1,
                        generation: 0,
                        book: Book::default(),
                        version: None,
                        buffering: None,
                        retry: None,
                        failures: 0,
                    },
                );
                self.subscribe_depth(cx, &symbol);
                self.snapshot(cx, &symbol);
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.count = s.count.saturating_sub(1);
                    if s.count > 0 {
                        return;
                    }
                }
                if let Some(s) = self.books.remove(&symbol)
                    && let Some(t) = s.retry
                {
                    cx.clear_timer(t);
                }
                if let Some(m) = cx.store.market(&symbol) {
                    let id = m.id.clone();
                    self.send(cx, "unsub.depth", &id, None);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval);
                if let Some(count) = self.candles.get_mut(&key) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.candles.remove(&key);
                        if let (Some(m), Some(i)) =
                            (cx.store.market(&symbol), wire::interval(interval))
                        {
                            let id = m.id.clone();
                            self.send(cx, "unsub.kline", &id, Some(("interval", i)));
                        }
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                let Some(i) = wire::interval(interval) else {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("MEXC"),
                    );
                    return;
                };
                let key = (symbol.clone(), interval);
                let n = self.candles.entry(key).or_insert(0);
                *n += 1;
                if *n == 1
                    && let Some(m) = cx.store.market(&symbol)
                {
                    let id = m.id.clone();
                    self.send(cx, "sub.kline", &id, Some(("interval", i)));
                }
            }
            Command::FetchOrderBook {
                command: _,
                symbol,
                limit,
            } => self.fetch_book(cx, &symbol, limit),
            Command::FetchCandles {
                command: _,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => {
                let context = match cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) {
                    Ok(context) => context,
                    Err(_) => return,
                };
                let Some(i) = wire::interval(interval) else {
                    let _ = cx.fail_context(&context, interval.unsupported("MEXC"));
                    return;
                };
                let Some(m) = cx.store.market(&symbol) else {
                    let _ = cx.fail_context(&context, "Unknown MEXC market");
                    return;
                };
                let mut path = format!("/api/v1/contract/kline/{}?interval={i}", m.id);
                if let Some(t) = from_ms {
                    path.push_str(&format!("&start={}", (t / 1000.0).floor()));
                }
                if let Some(t) = to_ms {
                    path.push_str(&format!("&end={}", (t / 1000.0).floor()));
                }
                if let Err((pending, error)) = cx.request_managed(
                    context.clone(),
                    Method::Get,
                    format!("{REST}{path}"),
                    None,
                    Pending::Candles {
                        context,
                        limit: limit.map_or(usize::MAX, |n| n as usize),
                    },
                ) {
                    self.on_sign_error(cx, pending, error.venue_error());
                }
            }
            trade => self.trade_command(cx, trade),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|s| s.version.is_some())
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
            Level::Warn,
            code::REQUEST_FAILED,
            format!("MEXC stops re-reading a catalogue with {after} malformed markets"),
        );
    }
    *stalls < 5
}

#[cfg(test)]
mod secondary_tests;
