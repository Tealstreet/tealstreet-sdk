//! Lighter perpetuals: deterministic public streams and engine-owned recovery.
mod account;
mod history;
mod identity;
mod limits;
pub mod sign;
mod trading;
mod venue_state;
mod wire;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Candle, Interval, Limits, Market, MinMax, Position, PositionSide, Precision, Ticker,
};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, PublicFeedPolicy, Response, SocketId, TimerId,
    Venue, VenueError,
};
use std::collections::{HashMap, HashSet};
use wire::*;

fn revalue_position(position: &mut Position, price: f64) {
    if position.contracts != 0.0 {
        let sign = if position.side == PositionSide::Long {
            1.0
        } else {
            -1.0
        };
        position.unrealized_pnl = sign * position.contracts * (price - position.entry_price);
    }
}

#[derive(Clone)]
pub struct Options {
    pub testnet: bool,
    pub robinhood: bool,
    pub public_data_only: bool,
    pub identity: Option<safe_cex_engine::signing::Identity>,
    pub l1_address: Option<String>,
    pub remote_signer: bool,
    pub signer_provider: Option<String>,
    pub application_id: Option<String>,
    pub api_key_index: Option<String>,
    pub integrator_account_index: Option<u64>,
    pub stop_slippage_pct: f64,
}
safe_cex_crypto::redacted_debug!(Options);

impl Default for Options {
    fn default() -> Self {
        Self {
            testnet: false,
            robinhood: false,
            public_data_only: false,
            identity: None,
            l1_address: None,
            remote_signer: false,
            signer_provider: None,
            application_id: None,
            api_key_index: None,
            integrator_account_index: None,
            stop_slippage_pct: 1.0,
        }
    }
}
pub enum Pending {
    Account {
        context: Context,
        request: account::Request,
    },
    History(history::Request),
    Identity(identity::Request),
    Trade(trading::Request),
    Tier {
        plan: safe_cex_engine::request_context::Plan,
        slot: safe_cex_engine::request_context::plan::Slot,
        context: Context,
    },
    Markets {
        context: Context,
        fallback: bool,
        rows: Vec<Market>,
        metadata: HashMap<String, MarketMeta>,
        failed: bool,
    },
    Tickers {
        context: Context,
        rows: Vec<(String, Ticker)>,
        failed: bool,
        revision: u64,
    },
    Depth {
        context: Context,
        symbol: String,
        revision: u64,
        generation: u64,
    },
    Candles {
        context: Context,
        command: Option<CommandId>,
        symbol: String,
        interval: Interval,
        rows: Vec<Candle>,
        failed: bool,
        revision: u64,
        generation: Option<u64>,
    },
}
#[derive(Clone, Debug, Default)]
pub struct MarketMeta {
    pub size_decimals: u32,
    pub price_decimals: u32,
    pub min_quote: f64,
    pub margin_mode: u8,
    pub initial_margin_fraction: f64,
    pub maintenance_margin_fraction: f64,
    pub initial_last_price: f64,
}
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    supervisor: Supervisor,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            socket: None,
            open: false,
            supervisor: Supervisor::new(Config {
                connect_timeout_ms: 120_000.0,
                idle_timeout_ms: 120_000.0,
                ping_interval_ms: Some(10_000.0),
                backoff_base_ms: 1_000.0,
                backoff_cap_ms: 16_000.0,
                ..Config::default()
            }),
        }
    }
}
#[derive(Default)]
struct DepthState {
    generation: u64,
    book: Book,
    nonce: Option<u64>,
    subscribers: u32,
    revision: u64,
    last_received: f64,
    waiting: bool,
    in_flight: bool,
    rest_retry_at: f64,
}
#[derive(Default)]
struct CandleState {
    generation: u64,
    revision: u64,
    subscribers: u32,
    last: Option<Candle>,
    last_received: f64,
    in_flight: bool,
}
pub struct Lighter {
    options: Options,
    account: account::Account,
    trading: trading::Trading,
    limits: limits::Limits,
    metadata: HashMap<String, MarketMeta>,
    symbols: HashMap<String, String>,
    /// Native ids and symbols of spot markets, whose private rows belong to another product.
    spot_markets: HashSet<String>,
    public: Connection,
    books: HashMap<String, DepthState>,
    candles: HashMap<(String, Interval), CandleState>,
    // Pending reads must distinguish subscriptions whose revisions restart at zero.
    subscription_generation: u64,
    ticker_revision: u64,
    ticker_sources: HashMap<String, u64>,
    ticker_in_flight: bool,
    startup: safe_cex_engine::startup::Backoff<bool>,
    poll: Option<TimerId>,
}
// Lighter's resolutions are the TS spellings.
fn supported(interval: Interval) -> bool {
    use Interval::*;
    matches!(interval, M1 | M5 | M15 | M30 | H1 | H4 | H12 | D1)
}
impl Lighter {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            limits: limits::Limits::default(),
            metadata: HashMap::new(),
            symbols: HashMap::new(),
            spot_markets: HashSet::new(),
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            subscription_generation: 0,
            ticker_revision: 0,
            ticker_sources: HashMap::new(),
            ticker_in_flight: false,
            startup: Default::default(),
            poll: None,
        }
    }
    fn host(&self) -> &'static str {
        match (self.options.robinhood, self.options.testnet) {
            (false, false) => "https://mainnet.zklighter.elliot.ai",
            (false, true) => "https://testnet-api.lighter.xyz",
            (true, false) => "https://api.rh.lighter.xyz",
            (true, true) => "https://api.rh-testnet.lighter.xyz",
        }
    }
    fn chain_id(&self) -> u32 {
        sign::chain_id(self.options.robinhood, self.options.testnet)
    }
    fn ws_url(&self) -> &'static str {
        match (self.options.robinhood, self.options.testnet) {
            (false, false) => "wss://mainnet.zklighter.elliot.ai/stream",
            (false, true) => "wss://testnet.zklighter.elliot.ai/stream",
            (true, false) => "wss://api.rh.lighter.xyz/stream",
            (true, true) => "wss://api.rh-testnet.lighter.xyz/stream",
        }
    }
    fn quote(&self) -> &'static str {
        if self.options.robinhood {
            "USDG"
        } else {
            "USDC"
        }
    }
    fn spot_market(&self, id_or_symbol: &str) -> bool {
        !id_or_symbol.is_empty() && self.spot_markets.contains(id_or_symbol)
    }
    fn symbol_for_id(&self, id: &str) -> Option<&str> {
        self.symbols.get(id).map(String::as_str)
    }
    fn markets(&mut self, cx: &mut Cx<'_, Pending>, fallback: bool) {
        let Ok(context) = cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) else {
            self.startup.failed(cx, true);
            return;
        };
        let path = if fallback {
            "orderBooks"
        } else {
            "orderBookDetails"
        };
        match cx.request_managed(
            context.clone(),
            None,
            Method::Get,
            format!("{}/api/v1/{path}", self.host()),
            None,
            Pending::Markets {
                context,
                fallback,
                rows: vec![],
                metadata: HashMap::new(),
                failed: false,
            },
        ) {
            Ok(request) => cx.stream_response(
                request,
                ArrayAt::Envelope(&[&["order_book_details"], &["order_books"]]),
            ),
            Err((pending, _)) => {
                self.public_failed(cx, pending, "Lighter catalog preparation failed")
            }
        }
    }
    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        // Suppressed public feeds never open; supervising them would only churn retries.
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public.supervisor.connecting(cx);
        }
        self.public.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new(self.ws_url())
                    .channel(safe_cex_engine::SocketChannel::Public),
            ),
        );
        self.public.open = false;
    }
    fn topic(&self, cx: &mut Cx<'_, Pending>, op: &str, channel: &str) {
        if let Some(socket) = self.public.socket.filter(|_| self.public.open) {
            #[derive(serde::Serialize)]
            struct Subscription<'a> {
                #[serde(rename = "type")]
                kind: &'a str,
                channel: &'a str,
            }
            cx.send(
                socket,
                serde_json::to_string(&Subscription { kind: op, channel }).unwrap(),
            );
        }
    }
    fn ticker_seed(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.ticker_in_flight || self.startup.waiting(&false) {
            return;
        }
        let Ok(context) = cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) else {
            self.startup.failed(cx, false);
            return;
        };
        self.ticker_in_flight = true;
        let pending = Pending::Tickers {
            context: context.clone(),
            rows: vec![],
            failed: false,
            revision: self.ticker_revision,
        };
        match cx.request_managed(
            context,
            None,
            Method::Get,
            format!("{}/api/v1/exchangeStats", self.host()),
            None,
            pending,
        ) {
            Ok(request) => cx.stream_response(
                request,
                ArrayAt::Envelope(&[
                    &["order_book_stats"],
                    &["data", "order_book_stats"],
                    &["result", "order_book_stats"],
                    &["tickers"],
                    &["data", "tickers"],
                    &["result", "tickers"],
                ]),
            ),
            Err((pending, error)) => {
                self.public_failed(cx, pending, &error.venue_error().message);
            }
        }
    }
    fn depth_seed(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(market) = cx.store.market(symbol) else {
            return;
        };
        let id = market.id.clone();
        let host = self.host();
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        if state.in_flight || cx.now < state.rest_retry_at {
            return;
        }
        let Ok(context) = cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) else {
            state.rest_retry_at = cx.now + 2_000.0;
            return;
        };
        state.in_flight = true;
        let pending = Pending::Depth {
            context: context.clone(),
            symbol: symbol.into(),
            revision: state.revision,
            generation: state.generation,
        };
        if let Err((pending, error)) = cx.request_managed(
            context,
            None,
            Method::Get,
            format!("{}/api/v1/orderBookOrders?market_id={}&limit=100", host, id),
            None,
            pending,
        ) {
            self.public_failed(cx, pending, &error.venue_error().message);
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn fetch_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Option<CommandId>,
        symbol: String,
        interval: Interval,
        limit: Option<u32>,
        from: Option<f64>,
        to: Option<f64>,
    ) {
        let generation = self
            .candles
            .get(&(symbol.clone(), interval))
            .map(|state| state.generation);
        let revision = self
            .candles
            .get(&(symbol.clone(), interval))
            .map_or(0, |state| state.revision);
        let policy = Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        };
        let context = if command.is_some() {
            let Ok(context) = cx.admit_http_command(policy) else {
                return;
            };
            context
        } else {
            match cx.http_context(Scope::Background, policy) {
                Ok(context) => context,
                Err(_) => {
                    if let Some(state) = self
                        .candles
                        .get_mut(&(symbol, interval))
                        .filter(|state| Some(state.generation) == generation)
                    {
                        state.in_flight = false;
                    }
                    return;
                }
            }
        };
        let result = (|| {
            if !supported(interval) {
                return Err(interval.unsupported("Lighter"));
            }
            let duration = interval.ms();
            let market = cx.store.market(&symbol).ok_or("Market not found")?;
            if from
                .into_iter()
                .chain(to)
                .any(|t| !t.is_finite() || t < 0.0)
            {
                return Err("Invalid candle time range".to_owned());
            }
            let count = limit.filter(|n| *n > 0).unwrap_or(500).min(1500);
            let end = to.unwrap_or(cx.now);
            let start = if limit.is_none() {
                from.unwrap_or(end - duration * f64::from(count))
            } else {
                end - duration * f64::from(count)
            }
            .max(0.0);
            if start > end {
                return Err("Invalid candle time range".to_owned());
            }
            Ok(format!(
                "{}/api/v1/candles?market_id={}&resolution={interval}&start_timestamp={start:.0}&end_timestamp={end:.0}&count_back={count}",
                self.host(),
                market.id
            ))
        })();
        let pending = Pending::Candles {
            context: context.clone(),
            command,
            symbol,
            interval,
            rows: vec![],
            failed: false,
            revision,
            generation,
        };
        match result {
            Ok(url) => match cx.request_managed(context, None, Method::Get, url, None, pending) {
                Ok(request) => cx.stream_response(
                    request,
                    ArrayAt::Envelope(&[&["c"], &["candles"], &["candlesticks"]]),
                ),
                Err((pending, _)) => self.on_sign_error(
                    cx,
                    pending,
                    VenueError::new("Lighter candle preparation is unavailable")
                        .kind(ErrorKind::Internal),
                ),
            },
            Err(error) => self.on_sign_error(cx, pending, error.into()),
        }
    }
    fn apply_stats(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        stats: &Stats<'_>,
        channel_id: Option<&str>,
        is_stats: bool,
    ) {
        let id = stats.market_id.map(|n| format!("{n:.0}"));
        let symbol = if cx.store.ticker(&stats.symbol).is_some() {
            Some(stats.symbol.as_ref())
        } else {
            id.as_deref()
                .or(channel_id)
                .and_then(|id| self.symbol_for_id(id))
        };
        let Some(symbol) = symbol.map(str::to_owned) else {
            return;
        };
        self.ticker_revision += 1;
        self.ticker_sources
            .insert(symbol.clone(), self.ticker_revision);
        cx.store.update_ticker(&symbol, |t| {
            if let Some(v) = stats.mark_price {
                t.mark = v;
            }
            if let Some(v) = stats.index_price {
                t.index = v;
                if !is_stats && stats.mark_price.is_none() {
                    t.mark = v;
                }
            }
            if let Some(v) = stats.last_trade_price {
                t.last = v;
            }
            if let Some(v) = stats
                .best_bid_price
                .as_ref()
                .or(stats.b.as_ref())
                .and_then(BestPrice::price)
            {
                t.bid = v;
            }
            if let Some(v) = stats
                .best_ask_price
                .as_ref()
                .or(stats.a.as_ref())
                .and_then(BestPrice::price)
            {
                t.ask = v;
            }
            if let Some(v) = stats.open_interest.or(stats.open_interest_usd) {
                t.open_interest = v;
            }
            if let Some(v) = stats.current_funding_rate.or(stats.funding_rate) {
                t.funding_rate = if v == 0.0012 { 0.0001 } else { v * 8.0 / 100.0 };
            }
            if let Some(v) = stats.daily_base_token_volume {
                t.volume = v;
            }
            if let Some(v) = stats.daily_quote_token_volume {
                t.quote_volume = v;
            }
            if let Some(v) = stats.daily_price_change {
                t.percentage = v;
            }
        });
        if !is_stats {
            let reference = stats
                .mark_price
                .or(stats.index_price)
                .filter(|p| *p > 0.0)
                .or(stats.last_trade_price.filter(|p| *p > 0.0));
            if let Some(price) = reference {
                cx.store.update_positions(|position| {
                    if position.symbol == symbol {
                        revalue_position(position, price);
                    }
                });
            }
        }
    }
    fn stats_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        #[derive(serde::Deserialize)]
        struct Nested<'a> {
            #[serde(borrow)]
            market_stats: &'a serde_json::value::RawValue,
        }
        if let Ok(nested) = serde_json::from_str::<Nested<'_>>(text) {
            self.stats_message(cx, nested.market_stats.get());
            return;
        }
        // Distinguish keyed maps before decoding the optional-field stats record.
        let probe = serde_json::from_str::<Stats<'_>>(text).ok();
        if let Some(stats) = probe.filter(|s| !s.symbol.is_empty() || s.market_id.is_some()) {
            self.apply_stats(cx, &stats, None, true);
            return;
        }
        let _ = wire::each_stats_map(text, |id, row| {
            if let Ok(stats) = row {
                self.apply_stats(cx, &stats, Some(id), true);
            }
        });
    }
    fn apply_depth(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        depth: Depth,
        snapshot: bool,
    ) {
        if depth.code.is_some_and(|code| code != 0 && code != 200) {
            return;
        }
        let Some(state) = self.books.get_mut(symbol).filter(|s| s.subscribers > 0) else {
            return;
        };
        if !snapshot {
            if let (Some(old), Some(new)) = (state.nonce, depth.nonce)
                && new <= old
            {
                return;
            }
            if state.waiting
                || matches!((state.nonce,depth.begin_nonce),(Some(old),Some(begin))if old!=begin)
            {
                state.waiting = true;
                state.revision += 1;
                state.nonce = None;
                state.book.clear();
                cx.book_changed(symbol);
                let id = cx.store.market(symbol).unwrap().id.clone();
                self.topic(cx, "unsubscribe", &format!("order_book/{id}"));
                self.topic(cx, "subscribe", &format!("order_book/{id}"));
                return;
            }
        } else {
            state.book.clear();
            state.waiting = depth.nonce.is_none();
        }
        let amount_scale = self
            .metadata
            .get(symbol)
            .map(|m| 10_f64.powi(m.size_decimals as i32));
        for (side, levels) in [(BookSide::Bids, &depth.bids), (BookSide::Asks, &depth.asks)] {
            if snapshot {
                let mut pairs: Vec<(f64, f64)> = levels
                    .iter()
                    .map(Level::pair)
                    .filter(|(p, n)| *p > 0.0 && *n > 0.0)
                    .collect();
                pairs.sort_unstable_by(|a, b| match side {
                    BookSide::Bids => b.0.total_cmp(&a.0),
                    BookSide::Asks => a.0.total_cmp(&b.0),
                });
                let mut count = 0;
                for i in 0..pairs.len() {
                    if count > 0 && pairs[count - 1].0 == pairs[i].0 {
                        pairs[count - 1].1 += pairs[i].1;
                    } else {
                        pairs[count] = pairs[i];
                        count += 1;
                    }
                }
                pairs.truncate(count);
                if let Some(scale) = amount_scale {
                    for (_, amount) in &mut pairs {
                        let lots = *amount * scale;
                        if lots.is_finite() {
                            *amount = lots.round() / scale;
                        }
                    }
                }
                state.book.load_sorted_owned(side, pairs);
            } else {
                for level in levels {
                    let (p, n) = level.pair();
                    if p > 0.0 && n >= 0.0 {
                        // Deltas replace the complete native size; only snapshots add sizes.
                        state.book.apply(side, p, n);
                    }
                }
            }
        }
        state.nonce = depth.nonce;
        state.revision += 1;
        state.last_received = cx.now;
        cx.book_changed(symbol);
    }
    fn emit_candle(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        interval: Interval,
        candle: Candle,
    ) {
        if let Some(state) = self
            .candles
            .get_mut(&(symbol.into(), interval))
            .filter(|s| s.subscribers > 0)
        {
            if state
                .last
                .is_some_and(|last| last.timestamp > candle.timestamp)
            {
                return;
            }
            state.last_received = cx.now;
            state.revision += 1;
            if state.last != Some(candle) {
                state.last = Some(candle);
                cx.candle(symbol, interval, candle);
            }
        }
    }
    fn public_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Identity(request) => self.identity_failed(cx, request, error),
            Pending::Account { context, request } => {
                if cx.validate_http_context(&context).is_ok() {
                    let _ = cx.finish_http_context(&context);
                    self.account_failed(cx, request, error);
                }
            }
            Pending::Tier { plan, .. } => {
                let _ = cx.fail_plan(&plan, error);
            }
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::History(request) => self.history_failed(cx, request, error),
            Pending::Markets {
                context, fallback, ..
            } => {
                let _ = cx.finish_http_context(&context);
                if fallback {
                    self.startup.failed(cx, true);
                } else {
                    self.markets(cx, true);
                }
            }
            Pending::Tickers { context, .. } => {
                let _ = cx.finish_http_context(&context);
                self.ticker_in_flight = false;
                self.startup.failed(cx, false);
            }
            Pending::Depth {
                context,
                symbol,
                generation,
                ..
            } => {
                let _ = cx.finish_http_context(&context);
                if let Some(s) = self
                    .books
                    .get_mut(&symbol)
                    .filter(|s| s.generation == generation)
                {
                    s.in_flight = false;
                    s.rest_retry_at = cx.now + 2_000.0;
                }
            }
            pending @ Pending::Candles { .. } => self.on_sign_error(cx, pending, error.into()),
        }
    }
}
impl Venue for Lighter {
    fn on_http_owner_retired_preflight(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) -> Option<safe_cex_engine::request_context::Preflight> {
        self.trade_retired(owner)
    }
    fn on_condition_wait_expired(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        owner: &safe_cex_engine::request_context::ConditionOwner,
        error: VenueError,
    ) {
        self.trade_condition_expired(cx, owner, error);
    }
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _: Option<&[String]>) {
        self.settle_retired_trades(cx);
        self.refresh_account(cx);
        self.reconcile_orders(cx);
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        if !matches!(
            pending,
            Pending::Trade(trading::Request::Submit(_)) | Pending::Tier { .. }
        ) {
            return Err(safe_cex_engine::request_context::Error::Invalid);
        }
        safe_cex_engine::request_context::ReconciliationData::new(String::new())
    }
    fn cancel_needs_stored_order(&self) -> bool {
        true
    }
    /// `None` until the account lookup or a tier read has something to carry.
    fn export_venue_state(&self) -> Option<Vec<u8>> {
        let state = venue_state::VenueState {
            account_index: self.account.account_index,
            tier: self.limits_tier_code(),
        };
        (state != venue_state::VenueState::default()).then(|| state.encode())
    }
    fn apply_venue_state(&mut self, cx: &mut Cx<'_, Pending>, bytes: &[u8]) {
        let Some(state) = venue_state::VenueState::decode(bytes) else {
            return;
        };
        if let Some(account) = state.account_index {
            self.apply_account_index(cx, account);
        }
        self.apply_limits_tier_code(state.tier);
    }

    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            fill_history_time: safe_cex_engine::FillHistoryTime::SecondsOrMilliseconds,
            account_tier: true,
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn guard_order_mutations(&self) -> bool {
        true
    }
    // An unproven (Unknown) placement may have rested; the engine fences its
    // symbol until a complete orders snapshot reconciles it.
    fn guard_pending_placements(&self) -> bool {
        true
    }
    // reconcile_uncertain reads all active orders at once, so a placement or
    // amend may wait for that read instead of duplicating an unproven one.
    fn fence_uncertain_placements(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn on_http_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &safe_cex_engine::request_context::Preflight,
    ) {
        self.trade_preflight_ready(cx, preflight);
    }
    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        plan: &safe_cex_engine::request_context::Plan,
    ) {
        self.trade_plan_ready(cx, plan);
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        if !cx.store.loaded().positions {
            return;
        }
        let references: HashMap<_, _> = cx
            .store
            .tickers()
            .iter()
            .filter_map(|ticker| {
                // Direct tickers fold index into mark only when mark is absent.
                // An explicit zero mark falls through to last, not index.
                [ticker.mark, ticker.last]
                    .into_iter()
                    .find(|price| *price > 0.0)
                    .map(|price| (ticker.symbol.clone(), price))
            })
            .collect();
        if !references.is_empty() {
            cx.store.update_positions(|position| {
                if let Some(price) = references.get(&position.symbol) {
                    revalue_position(position, *price);
                }
            });
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.markets(cx, false);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account_open(cx, socket) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.supervisor.opened(cx);
        self.topic(cx, "subscribe", "market_stats/all");
        for market in cx.store.markets().to_vec() {
            self.topic(cx, "subscribe", &format!("ticker/{}", market.id));
        }
        let symbols: Vec<_> = self
            .books
            .iter()
            .filter_map(|(symbol, state)| {
                (state.subscribers > 0 && cx.store.market(symbol).is_some())
                    .then_some(symbol.clone())
            })
            .collect();
        for symbol in symbols {
            let id = cx.store.market(&symbol).unwrap().id.clone();
            self.topic(cx, "subscribe", &format!("order_book/{id}"));
            self.depth_seed(cx, &symbol);
        }
        for ((symbol, interval), s) in &self.candles {
            if s.subscribers > 0 {
                let id = cx.store.market(symbol).unwrap().id.clone();
                self.topic(cx, "subscribe", &format!("candle/{id}/{interval}"));
                if !self.options.public_data_only {
                    self.topic(cx, "subscribe", &format!("trade/{id}"));
                }
            }
        }
        cx.send(socket, r#"{"type":"ping"}"#);
        self.ticker_seed(cx);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account_message(cx, socket, text) {
            return;
        }
        if self.public.socket != Some(socket) || !self.public.open {
            return;
        }
        self.public.supervisor.received(cx.now);
        let Ok(frame) = serde_json::from_str::<Envelope<'_>>(text) else {
            return;
        };
        if frame.kind == "ping" {
            cx.send(socket, r#"{"type":"pong"}"#);
            return;
        }
        if let Some(error) = frame.error {
            cx.notice(
                safe_cex_engine::Level::Warn,
                code::SUBSCRIPTION_REJECTED,
                format!("Lighter WS {}: {}", error.code, error.message),
            );
            return;
        }
        let mut parts = frame.channel.split([':', '/']);
        let channel = parts.next().unwrap_or("");
        let id = parts.next();
        match channel {
            "market_stats" => {
                if let Some(raw) = frame.market_stats.or(frame.data) {
                    self.stats_message(cx, raw.get());
                }
            }
            "ticker" => {
                if let Ok(stats) = serde_json::from_str::<Stats<'_>>(
                    frame.ticker.or(frame.data).map_or(text, |v| v.get()),
                ) {
                    self.apply_stats(cx, &stats, id, false);
                }
            }
            "tickers" => {
                if let Some(data) = frame.data {
                    let _ = safe_cex_engine::json::each::<Stats<'_>>(data.get(), |s| {
                        self.apply_stats(cx, &s, None, false)
                    });
                }
            }
            "order_book" => {
                if let (Some(symbol), Some(raw)) = (
                    id.and_then(|v| self.symbol_for_id(v)).map(str::to_owned),
                    frame.order_book.or(frame.data),
                ) && let Ok(depth) = serde_json::from_str::<Depth>(raw.get())
                {
                    let snapshot =
                        frame.kind.contains("subscribed") || frame.kind.contains("snapshot");
                    self.apply_depth(cx, &symbol, depth, snapshot);
                }
            }
            "candle" => {
                if let (Some(symbol), Some(interval)) = (
                    id.and_then(|v| self.symbol_for_id(v)).map(str::to_owned),
                    parts.next().and_then(Interval::parse),
                ) && let Ok(rows) =
                    serde_json::from_str::<CandleMessage>(frame.data.map_or(text, |v| v.get()))
                {
                    for row in rows.candles.into_iter().chain(rows.candle) {
                        self.emit_candle(cx, &symbol, interval, row.candle());
                    }
                }
            }
            "trade" => {
                if let Some(symbol) = id.and_then(|v| self.symbol_for_id(v)).map(str::to_owned)
                    && let Ok(rows) =
                        serde_json::from_str::<TradeMessage>(frame.data.map_or(text, |v| v.get()))
                {
                    for trade in rows.trades.into_iter().chain(rows.trade) {
                        let time = if trade.timestamp > 10_000_000_000.0 {
                            trade.timestamp / 1000.0
                        } else {
                            trade.timestamp
                        };
                        let updates: Vec<_> = self
                            .candles
                            .iter()
                            .filter(|((s, _), state)| *s == symbol && state.subscribers > 0)
                            .filter_map(|((_, i), state)| {
                                let mut c = state.last?;
                                let duration = i.ms() / 1000.0;
                                let start = (time / duration).floor() * duration;
                                if start < c.timestamp || trade.price <= 0.0 || trade.size < 0.0 {
                                    return None;
                                }
                                if start > c.timestamp {
                                    c = Candle {
                                        timestamp: start,
                                        open: c.close,
                                        high: c.close,
                                        low: c.close,
                                        close: c.close,
                                        volume: 0.0,
                                    };
                                }
                                c.high = c.high.max(trade.price);
                                c.low = c.low.min(trade.price);
                                c.close = trade.price;
                                c.volume += trade.size;
                                Some((*i, c))
                            })
                            .collect();
                        for (i, c) in updates {
                            self.emit_candle(cx, &symbol, i, c);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account_close(cx, socket, _code) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.socket = None;
        self.public.open = false;
        for (symbol, state) in &mut self.books {
            state.waiting = true;
            state.revision += 1;
            state.nonce = None;
            state.book.clear();
            cx.book_changed(symbol);
        }
        self.public.supervisor.retry(cx);
        let symbols: Vec<_> = self.books.keys().cloned().collect();
        for symbol in symbols {
            if self.books[&symbol].subscribers > 0 {
                self.depth_seed(cx, &symbol);
            }
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Tier { .. } => {}
            Pending::Account { context, request } => {
                if cx.validate_background_read_http_delivery(context).is_ok() {
                    self.account_item(cx, request, item);
                }
            }
            Pending::Trade(_) | Pending::Identity(_) => {}
            Pending::History(request) => self.history_item(cx, request, item),
            Pending::Markets {
                context,
                rows,
                metadata,
                ..
            } => {
                if cx.validate_background_read_http_delivery(context).is_err() {
                    return;
                }
                match serde_json::from_str::<Instrument<'_>>(item) {
                    Ok(m) => {
                        if m.market_type.eq_ignore_ascii_case("spot") {
                            self.spot_markets.insert(format!("{:.0}", m.market_id));
                            self.spot_markets.insert(m.symbol.to_string());
                            return;
                        }
                        if m.status != "active" {
                            return;
                        }
                        let size = m
                            .supported_size_decimals
                            .filter(|n| *n >= 0.0)
                            .unwrap_or(3.0) as u32;
                        let price = m
                            .supported_price_decimals
                            .filter(|n| *n >= 0.0)
                            .unwrap_or(2.0) as u32;
                        if size > 18
                            || price > 18
                            || m.market_id < 0.0
                            || m.market_id > 32767.0
                            || m.market_id == 255.0
                            || m.market_id.fract() != 0.0
                        {
                            safe_cex_engine::startup::skipped_row(cx);
                            return;
                        }
                        let fraction = m
                            .min_initial_margin_fraction
                            .or(m.default_initial_margin_fraction)
                            .unwrap_or(0.0);
                        let (base, quote) = if let Some((b, q)) = m.symbol.split_once('/') {
                            (
                                b,
                                q.split('/')
                                    .next()
                                    .filter(|q| !q.is_empty())
                                    .unwrap_or(self.quote()),
                            )
                        } else if let Some(q) = ["USDT", "USDC", "USDG"]
                            .into_iter()
                            .find(|q| m.symbol.ends_with(q))
                        {
                            (&m.symbol[..m.symbol.len() - q.len()], q)
                        } else {
                            (m.symbol.as_ref(), self.quote())
                        };
                        let symbol = m.symbol.to_string();
                        metadata.insert(
                            symbol.clone(),
                            MarketMeta {
                                initial_last_price: m.last_trade_price.unwrap_or(0.0),
                                size_decimals: size,
                                price_decimals: price,
                                min_quote: m.min_quote_amount,
                                margin_mode: m.market_config.map_or(0, |m| m.market_margin_mode),
                                initial_margin_fraction: fraction / 10000.0,
                                maintenance_margin_fraction: m.maintenance_margin_fraction
                                    / 10000.0,
                            },
                        );
                        rows.push(Market {
                            id: format!("{:.0}", m.market_id),
                            symbol: symbol.clone(),
                            base: base.into(),
                            quote: quote.into(),
                            pretty_symbol: None,
                            active: true,
                            precision: Precision {
                                amount: 10_f64.powi(-(size as i32)),
                                price: 10_f64.powi(-(price as i32)),
                            },
                            limits: Limits {
                                amount: MinMax {
                                    min: m.min_base_amount,
                                    max: m.order_quote_limit.unwrap_or(1_000_000.0),
                                },
                                leverage: MinMax {
                                    min: 1.0,
                                    max: if fraction > 0.0 {
                                        10_000.0 / fraction
                                    } else {
                                        50.0
                                    },
                                },
                                price: None,
                            },
                            contract_size: None,
                            is_expiring: Some(false),
                            is_inverse: None,
                            is_spot: Some(false),
                            is_perpetual: None,
                            is_read_only: None,
                            taker_fee: None,
                            maker_fee: None,
                        });
                    }
                    Err(_) => safe_cex_engine::startup::skipped_row(cx),
                }
            }
            Pending::Tickers {
                context,
                rows,
                failed,
                ..
            } => {
                if cx.validate_http_context(context).is_err() {
                    return;
                }
                match serde_json::from_str::<Stats<'_>>(item) {
                    Ok(stats) => {
                        let id = stats.market_id.map(|v| format!("{v:.0}"));
                        if let Some(symbol) = self
                            .symbol_for_id(id.as_deref().unwrap_or(""))
                            .or_else(|| cx.store.ticker(&stats.symbol).map(|t| t.symbol.as_str()))
                        {
                            let mut ticker = cx.store.ticker(symbol).cloned().unwrap();
                            if let Some(v) = stats.mark_price {
                                ticker.mark = v;
                            }
                            if let Some(v) = stats.index_price {
                                ticker.index = v;
                                if stats.mark_price.is_none() {
                                    ticker.mark = v;
                                }
                            }
                            if let Some(v) = stats.last_trade_price {
                                ticker.last = v;
                            }
                            if let Some(v) = stats
                                .best_bid_price
                                .as_ref()
                                .or(stats.b.as_ref())
                                .and_then(BestPrice::price)
                            {
                                ticker.bid = v;
                            }
                            if let Some(v) = stats
                                .best_ask_price
                                .as_ref()
                                .or(stats.a.as_ref())
                                .and_then(BestPrice::price)
                            {
                                ticker.ask = v;
                            }
                            if let Some(v) = stats.open_interest.or(stats.open_interest_usd) {
                                ticker.open_interest = v;
                            }
                            if let Some(v) = stats.current_funding_rate.or(stats.funding_rate) {
                                ticker.funding_rate =
                                    if v == 0.0012 { 0.0001 } else { v * 8.0 / 100.0 };
                            }
                            if let Some(v) = stats.daily_base_token_volume {
                                ticker.volume = v;
                            }
                            if let Some(v) = stats.daily_quote_token_volume {
                                ticker.quote_volume = v;
                            }
                            if let Some(v) = stats.daily_price_change {
                                ticker.percentage = v;
                            }
                            rows.push((symbol.into(), ticker));
                        } else {
                            *failed = true;
                            safe_cex_engine::startup::skipped_row(cx);
                        }
                    }
                    Err(_) => {
                        *failed = true;
                        safe_cex_engine::startup::skipped_row(cx);
                    }
                }
            }
            Pending::Candles { rows, failed, .. } => {
                match serde_json::from_str::<CandleRow>(item) {
                    Ok(row) => rows.push(row.candle()),
                    Err(_) => *failed = true,
                }
            }
            _ => {}
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Markets { context, .. } = &pending
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            return;
        }
        let pending = match pending {
            Pending::Identity(request) => {
                self.identity_response(cx, request, response);
                return;
            }
            Pending::Account { context, request } => {
                if cx.validate_background_read_http_delivery(&context).is_ok() {
                    self.account_response(cx, request, response);
                }
                let _ = cx.finish_http_context(&context);
                return;
            }
            Pending::Tier {
                plan,
                slot,
                context,
            } => {
                self.tier_changed(cx, plan, slot, context, response);
                return;
            }
            Pending::Trade(request) => {
                self.trade_response(cx, request, response);
                return;
            }
            Pending::History(request) => {
                self.history_response(cx, request, response);
                return;
            }
            other => other,
        };
        let status = serde_json::from_str::<Status<'_>>(response.body);
        if !(200..300).contains(&response.status) || status.as_ref().is_ok_and(|s| s.code != 200) {
            let error = status.ok().map_or_else(
                || format!("Lighter HTTP {}", response.status),
                |s| format!("Lighter {}: {}", s.code, s.message),
            );
            self.public_failed(cx, pending, &error);
            return;
        }
        match pending {
            Pending::Account { .. }
            | Pending::Tier { .. }
            | Pending::Trade(_)
            | Pending::History(_)
            | Pending::Identity(_) => unreachable!(),
            Pending::Markets {
                context,
                fallback,
                rows,
                metadata,
                failed,
            } => {
                if failed || rows.is_empty() {
                    self.public_failed(
                        cx,
                        Pending::Markets {
                            context,
                            fallback,
                            rows,
                            metadata,
                            failed,
                        },
                        "Invalid Lighter market snapshot",
                    );
                    return;
                }
                self.startup.succeeded(cx, &true);
                self.symbols = rows
                    .iter()
                    .map(|m| (m.id.clone(), m.symbol.clone()))
                    .collect();
                self.metadata = metadata;
                cx.store.set_tickers(
                    rows.iter()
                        .map(|m| Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            last: self.metadata[&m.symbol].initial_last_price,
                            ..Ticker::default()
                        })
                        .collect(),
                );
                cx.store.set_markets(rows);
                let _ = cx.finish_http_context(&context);
                // Frame-fed engines load only the market catalogue: no ticker
                // seed, socket or poll; their data source streams the rest.
                if cx.frame_fed() {
                    self.account_start(cx);
                    return;
                }
                self.ticker_seed(cx);
                self.account_start(cx);
                self.connect(cx);
                self.poll = Some(cx.set_timer(2_000.0));
            }
            Pending::Tickers {
                context,
                rows,
                failed,
                revision,
            } => {
                let live = cx.validate_http_context(&context).is_ok();
                let _ = cx.finish_http_context(&context);
                self.ticker_in_flight = false;
                if !live || failed && rows.is_empty() {
                    self.startup.failed(cx, false);
                    return;
                }
                self.startup.succeeded(cx, &false);
                for (symbol, ticker) in rows {
                    if self.ticker_sources.get(&symbol).copied().unwrap_or(0) <= revision {
                        cx.store.update_ticker(&symbol, |t| *t = ticker);
                    }
                }
            }
            Pending::Depth {
                context,
                symbol,
                revision,
                generation,
            } => {
                let live = cx.validate_http_context(&context).is_ok();
                let _ = cx.finish_http_context(&context);
                let Some(s) = self
                    .books
                    .get_mut(&symbol)
                    .filter(|s| s.generation == generation)
                else {
                    return;
                };
                s.in_flight = false;
                s.rest_retry_at = cx.now + 2_000.0;
                if !live || s.revision != revision {
                    return;
                }
                #[derive(serde::Deserialize)]
                struct RestDepth {
                    bids: Vec<Level>,
                    asks: Vec<Level>,
                }
                if let Ok(depth) = serde_json::from_str::<RestDepth>(response.body) {
                    self.apply_depth(
                        cx,
                        &symbol,
                        Depth {
                            bids: depth.bids,
                            asks: depth.asks,
                            nonce: None,
                            begin_nonce: None,
                            code: None,
                        },
                        true,
                    );
                }
            }
            Pending::Candles {
                context,
                command,
                symbol,
                interval,
                mut rows,
                failed,
                revision,
                generation,
            } => {
                if command.is_some() {
                    if cx.validate_read_http_delivery(&context, &context).is_err() {
                        return;
                    }
                } else {
                    let _ = cx.finish_http_context(&context);
                }
                if command.is_none()
                    && !self
                        .candles
                        .get(&(symbol.clone(), interval))
                        .is_some_and(|s| Some(s.generation) == generation)
                {
                    return;
                }
                if failed {
                    self.public_failed(
                        cx,
                        Pending::Candles {
                            context,
                            command,
                            symbol,
                            interval,
                            rows,
                            failed,
                            revision,
                            generation,
                        },
                        "Invalid Lighter candles",
                    );
                    return;
                }
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                if command.is_some() {
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
                } else {
                    if let Some(s) = self.candles.get_mut(&(symbol.clone(), interval)) {
                        s.in_flight = false;
                    }
                    if self
                        .candles
                        .get(&(symbol.clone(), interval))
                        .is_some_and(|state| state.revision != revision)
                    {
                        return;
                    }
                    if let Some(c) = rows.last() {
                        self.emit_candle(cx, &symbol, interval, *c);
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
        if let Pending::Markets { context, .. } = &pending
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            return;
        }
        if let Pending::Account { context, request } = pending {
            if cx.validate_background_read_http_delivery(&context).is_ok() {
                self.account_stream_error(cx, request, response);
            }
            let _ = cx.finish_http_context(&context);
            return;
        }
        self.public_failed(
            cx,
            pending,
            &format!("Missing Lighter response list: {}", response.status),
        );
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.public_failed(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::Identity(request) => self.identity_failed(cx, request, error),
            Pending::Trade(request) => self.trade_error(cx, request, error),
            Pending::Account { context, request } => {
                let _ = cx.finish_http_context(&context);
                self.account_failed(cx, request, &error.message);
            }
            Pending::History(request) => self.history_transport_failed(cx, request, error),
            Pending::Tier { plan, .. } => {
                let _ = cx.fail_plan(&plan, error);
            }
            other => self.on_request_failed(cx, other, &error.message),
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        _reason: safe_cex_engine::request_context::Error,
        error: VenueError,
    ) {
        match pending {
            Pending::Identity(request) => self.identity_rejected(cx, request, error),
            other => self.on_sign_error(cx, other, error),
        }
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::Identity(request) => self.identity_rejected(cx, request, error),
            Pending::Trade(request) => self.trade_error(cx, request, error),
            Pending::Account { context, request } => {
                let _ = cx.finish_http_context(&context);
                self.account_failed(cx, request, &error.message);
            }
            Pending::History(request) => self.history_rejected(cx, request, error),
            Pending::Tier { plan, .. } => {
                let _ = cx.fail_plan(&plan, error);
            }
            Pending::Candles {
                context,
                command,
                symbol,
                interval,
                generation,
                ..
            } => {
                if command.is_some() {
                    let _ = cx.fail_context(&context, error);
                } else {
                    let _ = cx.finish_http_context(&context);
                    if let Some(state) = self
                        .candles
                        .get_mut(&(symbol, interval))
                        .filter(|state| Some(state.generation) == generation)
                    {
                        state.in_flight = false;
                    }
                }
            }
            pending => self.public_failed(cx, pending, &error.message),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.settle_retired_trades(cx);
        if let Some(markets) = self.startup.on_timer(timer) {
            if markets {
                self.markets(cx, false);
            } else {
                self.ticker_seed(cx);
            }
            return;
        }
        if self.account_timer(cx, timer) || self.trade_timer(cx, timer) {
            return;
        }
        match self.public.supervisor.on_timer(cx, timer) {
            Tick::Unhandled => {}
            Tick::Reconnect => return self.connect(cx),
            Tick::Ping => {
                if let Some(s) = self.public.socket.filter(|_| self.public.open) {
                    cx.send(s, r#"{"type":"ping"}"#);
                }
                return;
            }
            Tick::TimedOut(_) => {
                if let Some(s) = self.public.socket {
                    cx.close(s);
                    self.on_close(cx, s, 1006);
                }
                return;
            }
            Tick::Handled => return,
        }
        if self.poll == Some(timer) {
            self.poll = Some(cx.set_timer(2_000.0));
            if !self.public.open
                || !cx.store.tickers().iter().any(|t| {
                    [t.mark, t.last, t.index, t.bid, t.ask]
                        .iter()
                        .any(|v| *v > 0.0)
                })
            {
                self.ticker_seed(cx);
            }
            let symbols: Vec<_> = self
                .books
                .iter_mut()
                .filter_map(|(symbol, s)| {
                    if s.subscribers == 0 {
                        return None;
                    }
                    if cx.now - s.last_received > 90_000.0 {
                        s.waiting = true;
                        if !s.book.is_empty() {
                            s.book.clear();
                            s.nonce = None;
                            s.revision += 1;
                            cx.book_changed(symbol);
                        }
                    }
                    if s.waiting {
                        Some(symbol.clone())
                    } else {
                        None
                    }
                })
                .collect();
            for s in symbols {
                self.depth_seed(cx, &s);
            }
            let candles: Vec<_> = self
                .candles
                .iter_mut()
                .filter_map(|((s, i), state)| {
                    if state.subscribers > 0
                        && !state.in_flight
                        && cx.now - state.last_received >= 120_000.0
                    {
                        state.in_flight = true;
                        Some((s.clone(), *i))
                    } else {
                        None
                    }
                })
                .collect();
            for (s, i) in candles {
                self.fetch_candles(cx, None, s, i, Some(1), None, None);
            }
        }
    }
    fn on_venue_action(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: safe_cex_engine::CommandId,
        action: safe_cex_engine::VenueAction,
    ) {
        self.tier_action(cx, command, action);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let refresh = self.restore_retired_trades(cx.frame_fed());
        self.command(cx, command);
        if refresh {
            self.refresh_account(cx);
            self.refresh_orders(cx);
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}
impl Lighter {
    fn command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::FetchAccount { .. } => self.fetch_identity(cx),
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => {
                if cx.store.loaded().markets && cx.store.market(&symbol).is_none() {
                    cx.notice(
                        safe_cex_engine::Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        "Lighter order book market not found",
                    );
                    return;
                }
                let state = self.books.entry(symbol.clone()).or_default();
                state.subscribers += 1;
                if state.subscribers == 1 {
                    self.subscription_generation = self.subscription_generation.wrapping_add(1);
                    state.generation = self.subscription_generation;
                    state.waiting = true;
                    state.last_received = cx.now;
                    if let Some(market) = cx.store.market(&symbol) {
                        self.topic(cx, "subscribe", &format!("order_book/{}", market.id));
                        self.depth_seed(cx, &symbol);
                    }
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.subscribers = s.subscribers.saturating_sub(1);
                    if s.subscribers == 0 {
                        if let Some(market) = cx.store.market(&symbol) {
                            self.topic(cx, "unsubscribe", &format!("order_book/{}", market.id));
                        }
                        self.books.remove(&symbol);
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                if !supported(interval) {
                    cx.notice(
                        safe_cex_engine::Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("Lighter"),
                    );
                    return;
                }
                let Some(m) = cx.store.market(&symbol) else {
                    return;
                };
                let id = m.id.clone();
                let state = self.candles.entry((symbol.clone(), interval)).or_default();
                state.subscribers += 1;
                if state.subscribers == 1 {
                    self.subscription_generation = self.subscription_generation.wrapping_add(1);
                    state.generation = self.subscription_generation;
                    state.last_received = cx.now;
                    self.topic(cx, "subscribe", &format!("candle/{id}/{interval}"));
                    if !self.options.public_data_only {
                        self.topic(cx, "subscribe", &format!("trade/{id}"));
                    }
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                if let Some(s) = self.candles.get_mut(&(symbol.clone(), interval)) {
                    s.subscribers = s.subscribers.saturating_sub(1);
                    if s.subscribers == 0 {
                        let id = cx.store.market(&symbol).unwrap().id.clone();
                        self.topic(cx, "unsubscribe", &format!("candle/{id}/{interval}"));
                        self.candles.remove(&(symbol.clone(), interval));
                        if !self.candles.keys().any(|(s, _)| *s == symbol) {
                            self.topic(cx, "unsubscribe", &format!("trade/{id}"));
                        }
                    }
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => self.fetch_candles(cx, Some(command), symbol, interval, limit, from_ms, to_ms),
            Command::FetchLeverage { symbol, .. } => {
                let Ok(context) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return;
                };
                #[derive(serde::Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Leverage {
                    buy_leverage: f64,
                    sell_leverage: f64,
                }
                let result = cx
                    .store
                    .setting(&symbol)
                    .ok_or_else(|| "Lighter leverage is not loaded".to_owned())
                    .and_then(|setting| {
                        serde_json::to_string(&Leverage {
                            buy_leverage: setting.leverage,
                            sell_leverage: setting.leverage,
                        })
                        .map_err(|e| e.to_string())
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
            Command::Query { command, query } => self.history_query(cx, command, query),
            other => self.trade_command(cx, other),
        }
    }
}

#[cfg(test)]
mod deployment_tests {
    use super::{Lighter, Options};
    use crate::reference_signer::Signer;
    use crate::sign::Transaction;
    use safe_cex_crypto::lighter::{SchnorrNonce, public_key, schnorr_verify};

    #[test]
    fn deployment_transactions_bind_the_selected_chain_and_auth_tokens_do_not() {
        let key = format!("01{}", "00".repeat(39));
        let mut raw_key = [0; 40];
        raw_key[0] = 1;
        let public = public_key(&raw_key).unwrap();
        let mut tokens = Vec::new();
        for (robinhood, testnet, chain) in [
            (false, false, 304_u64),
            (false, true, 300),
            (true, false, 466_324),
            (true, true, 300),
        ] {
            let venue = Lighter::new(Options {
                robinhood,
                testnet,
                ..Options::default()
            });
            let signer = Signer::new(&key, venue.chain_id(), 42, 3).unwrap();
            let signed = signer
                .sign_with_nonce(
                    &Transaction::Cancel {
                        market_id: 1,
                        order_id: 99,
                    },
                    7,
                    1_700_000_000_000,
                    SchnorrNonce::from_bytes(&[66; 40]).unwrap(),
                )
                .unwrap();
            assert_eq!(
                u64::from_le_bytes(signed.data[..8].try_into().unwrap()),
                chain
            );
            let signature = signed
                .signature
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            assert!(schnorr_verify(&signed.data, &public, &signature).unwrap());
            for other_chain in [300_u64, 304, 466_324]
                .into_iter()
                .filter(|other| *other != chain)
            {
                let mut cross_chain = signed.data.clone();
                cross_chain[..8].copy_from_slice(&other_chain.to_le_bytes());
                assert!(!schnorr_verify(&cross_chain, &public, &signature).unwrap());
            }
            tokens.push(
                signer
                    .auth_token_with_nonce(
                        1_700_000_060,
                        SchnorrNonce::from_bytes(&[66; 40]).unwrap(),
                    )
                    .unwrap(),
            );
        }
        assert!(tokens.windows(2).all(|pair| pair[0] == pair[1]));
    }
}

fn primary_credential(
    identity: &safe_cex_engine::signing::Identity,
) -> safe_cex_engine::request_context::Credential {
    safe_cex_engine::request_context::Credential {
        slot: safe_cex_engine::CredentialSlot::Primary,
        reference: identity.credential.clone(),
    }
}

#[cfg(test)]
extern crate self as safe_cex_venue_lighter;
#[cfg(test)]
#[path = "../tests/support/reference_signer.rs"]
mod reference_signer;
