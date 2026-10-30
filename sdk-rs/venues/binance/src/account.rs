//! The private side: listenKey and user data stream, the account snapshot
//! reads and the reconciliation that keeps them honest.
use crate::sign::Query;
use crate::wire::*;
use crate::{BinanceFutures, Channel, Pending};
use safe_cex_engine::Level;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::coverage::SnapshotCoverage;
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Balance, CoinBalance, CurrencyBalance, MarginType, Market, Order, OrderExtra, OrderSide,
    OrderStatus, OrderType, Position, PositionMode, PositionSide, RejectReason, StopTriggerType,
    TimeInForce,
};
use safe_cex_engine::socket_lifecycle::RetireReason;
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Cx, Fill, Method, Response, TimerId, VenueError, json, num};
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};

/// Binance expires a listenKey 60 minutes after the last keepalive.
const LISTEN_KEY_REFRESH_MS: f64 = 30.0 * 60_000.0;
const LISTEN_KEY_RETRY_MAX_MS: f64 = 60_000.0;
/// 5 s doubling to the 60 s keepalive interval.
const LISTEN_KEY_RETRY: Backoff = Backoff::new(5_000.0, 4, LISTEN_KEY_RETRY_MAX_MS);
const HEALTH_CHECK_MS: f64 = 3_000.0;
const VENUE_STATE_VERSION: u8 = 1;
const ACCOUNT_INTERVAL_MS: f64 = 25_000.0;
const DEGRADED_ACCOUNT_INTERVAL_MS: f64 = 3_000.0;
const POSITION_RISK_INTERVAL_MS: f64 = 75_000.0;
/// `ACCOUNT_UPDATE` omits leverage, margin type and liquidation price; the
/// REST repair that fills them in runs at most this often.
const POSITION_REPAIR_THROTTLE_MS: f64 = 10_000.0;
/// Order events trigger a full REST order snapshot this long after the last one.
const ORDERS_REFRESH_DEBOUNCE_MS: f64 = 10_000.0;
const ORDERS_RETRY_MS: f64 = 15_000.0;
/// Retain execution identities across reconnects without an unbounded event log.
const DERIVATIVE_EXECUTION_WINDOW: usize = 16_384;
const SPOT_EXECUTION_WINDOW: usize = 16_384;
/// After the ticker seed, a tradable pair that still has no quote stops holding
/// the wallet's USD total after this long; the asset is then valued at 0.
const VALUATION_WAIT_MS: f64 = 30_000.0;
const STABLECOINS: [&str; 7] = ["USDT", "USDC", "BUSD", "FDUSD", "BFUSD", "LDUSDT", "BNFCR"];
const SKIP_LOG_LIMIT: usize = 4_096;

pub enum Request {
    /// Collected while streaming: max leverage by symbol.
    LeverageBrackets(HashMap<String, f64>),
    ListenKey,
    Keepalive,
    PositionMode,
    PositionRisk {
        generation: u32,
        rows: Vec<Position>,
        marks: Vec<(String, f64)>,
        skipped: SnapshotCoverage<String>,
    },
    SymbolConfig {
        generation: u32,
        configs: Vec<(String, MarketSetting)>,
    },
    Account {
        version: u32,
    },
    OpenOrders {
        generation: u32,
    },
    OpenAlgoOrders {
        generation: u32,
    },
}

struct PositionsJoin {
    generation: u32,
    /// Position-state version when the reads were issued; a newer stream
    /// event makes the snapshot stale.
    version: u32,
    rows: Option<Vec<Position>>,
    skipped: SnapshotCoverage<String>,
    configs: Option<Vec<(String, MarketSetting)>>,
    failed: bool,
}

struct OrdersJoin {
    generation: u32,
    token: safe_cex_engine::OrderSnapshotToken,
    /// Order-uncertainty epoch before the first read: a census spans one or two contexts.
    epoch: u64,
    native_orders: Vec<(String, String, u64, f64)>,
    native_strategies: Vec<(String, String, u64)>,
    regular: Option<Vec<Order>>,
    algo: Option<Vec<Order>>,
    skipped: SnapshotCoverage<String>,
    /// Symbols of the rows rejected under each id; "" when the row's symbol is unreadable.
    rejected_symbols: HashMap<String, Vec<String>>,
    failed: bool,
}

struct WalletAsset {
    asset: String,
    wallet: f64,
    available: f64,
    initial: f64,
    unrealized: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PmOrderSource {
    Regular(u64),
    Strategy(u64),
}

#[derive(Clone)]
struct PmOrderOwner {
    symbol: String,
    source: PmOrderSource,
}

#[derive(Default)]
struct SpotDelivery {
    bootstrap_floor: f64,
    delivered_cumulative: f64,
}

#[derive(Default)]
pub struct AccountState {
    started: bool,
    listen_key_failures: u32,
    listen_key_timer: Option<TimerId>,
    listen_key_session: Option<safe_cex_engine::request_context::Session>,
    keepalive_timer: Option<TimerId>,
    keepalive_failing: bool,
    has_opened: bool,
    pub resync_orders_on_open: bool,
    tick_timer: Option<TimerId>,
    last_account_at: f64,
    account_in_flight: bool,
    account_read_predates_event: bool,
    account_version: u32,
    spot_balances: HashMap<String, (f64, f64)>,
    native_executed: HashMap<(String, u64), (f64, bool)>,
    derivative_trades: HashSet<(String, u64, i64)>,
    derivative_trade_order: VecDeque<(String, u64, i64)>,
    derivative_fill_watermarks: HashMap<(String, u64), f64>,
    terminal_algo_ids: HashSet<String>,
    /// Orders whose native type executes at a limit price once triggered.
    pub(crate) limit_stops: HashSet<String>,
    strategy_client_native: HashMap<(String, String), u64>,
    terminal_strategies: HashSet<(String, u64)>,
    pm_order_owners: HashMap<String, PmOrderOwner>,
    unquantified_cancels: HashSet<(String, u64)>,
    spot_client_native: HashMap<(String, String), u64>,
    spot_delivery: HashMap<(String, u64), SpotDelivery>,
    spot_trades: HashSet<(String, u64, i64)>,
    spot_trade_order: VecDeque<(String, u64, i64)>,
    spot_subscribe_id: Option<String>,
    spot_subscribe_at: Option<f64>,
    spot_subscription: Option<u64>,
    spot_heartbeat_id: Option<String>,
    spot_heartbeat_at: Option<f64>,
    last_risk_at: f64,
    position_version: u32,
    pending_position_events: bool,
    repair_timer: Option<TimerId>,
    last_repair_at: Option<f64>,
    positions_generation: u32,
    positions: Option<PositionsJoin>,
    orders_generation: u32,
    orders: Option<OrdersJoin>,
    orders_refresh_timer: Option<TimerId>,
    orders_stale: bool,
    /// A reconcile asked while a snapshot was in flight: read again once it settles.
    orders_owed: bool,
    last_orders_retry_at: f64,
    logged_balance: bool,
    wallet_assets: Option<Vec<WalletAsset>>,
    valuation_pending: bool,
    valuation_refresh_required: bool,
    valuation_warned: bool,
    valuation_repair_pending: bool,
    valuation_wait_since: Option<f64>,
    /// Funded assets valued at 0 for want of a price; each is warned about once.
    zero_valued_assets: HashSet<String>,
    native_asset_marks: HashMap<String, f64>,
    /// Position ids (or "" for an unidentified row) whose skipped row was already logged.
    position_skip_logged: HashMap<String, bool>,
    /// Order ids whose skipped row was already logged.
    order_skip_logged: HashMap<String, bool>,
}

impl AccountState {
    pub(crate) fn observe_snapshot_orders(&mut self, orders: &[Order]) {
        for order in orders {
            if let Ok(native) = order.id.parse::<u64>() {
                let evidence = self
                    .native_executed
                    .entry((order.symbol.clone(), native))
                    .or_default();
                evidence.0 = evidence.0.max(order.filled);
                evidence.1 |= matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled);
            }
        }
    }

    pub(crate) fn order_terminal(&self, order: &Order) -> bool {
        self.terminal_algo_ids.contains(&order.id)
            || order.id.parse().is_ok_and(|native: u64| {
                self.native_executed
                    .get(&(order.symbol.clone(), native))
                    .is_some_and(|(_, terminal)| *terminal)
            })
    }
}

/// Logs a skipped row once per key, allowing one Error upgrade if its kept copy is lost.
fn log_skip(
    cx: &mut Cx<'_, Pending>,
    logged: &mut HashMap<String, bool>,
    key: String,
    level: Level,
    message: impl Into<String>,
) {
    if logged.len() >= SKIP_LOG_LIMIT {
        logged.clear();
    }
    let error = level == Level::Error;
    if logged.get(&key).is_some_and(|&errored| errored || !error) {
        return;
    }
    logged.insert(key, error);
    cx.notice(level, code::ROW_SKIPPED, message);
}

pub fn order_type(text: &str) -> Option<OrderType> {
    Some(match text {
        "LIMIT" | "LIMIT_MAKER" => OrderType::Limit,
        "MARKET" => OrderType::Market,
        "STOP_MARKET" | "STOP" | "STOP_LOSS" | "STOP_LOSS_LIMIT" => OrderType::StopLoss,
        "TAKE_PROFIT_MARKET" | "TAKE_PROFIT" | "TAKE_PROFIT_LIMIT" => OrderType::TakeProfit,
        "TRAILING_STOP_MARKET" => OrderType::TrailingStopLoss,
        _ => return None,
    })
}

/// Native stop types that execute at a limit price once triggered; this venue
/// re-places stops as market-execution orders, so it must not amend these.
/// Spot `STOP_LOSS`/`TAKE_PROFIT` execute at market; futures `STOP`/`TAKE_PROFIT` do not.
pub fn executes_at_limit(native: &str, spot: bool) -> bool {
    match native {
        "STOP_LOSS_LIMIT" | "TAKE_PROFIT_LIMIT" => true,
        "STOP" | "TAKE_PROFIT" => !spot,
        _ => false,
    }
}

pub fn order_side(text: &str) -> OrderSide {
    if text == "SELL" {
        OrderSide::Sell
    } else {
        OrderSide::Buy
    }
}

pub fn time_in_force(text: &str) -> Option<TimeInForce> {
    Some(match text {
        "GTC" => TimeInForce::GoodTillCancel,
        "IOC" => TimeInForce::ImmediateOrCancel,
        "FOK" => TimeInForce::FillOrKill,
        "GTX" => TimeInForce::PostOnly,
        _ => return None,
    })
}

/// TP/SL orders carry their trigger; other order types have none.
fn trigger_type(kind: OrderType, working_type: Option<&str>) -> Option<StopTriggerType> {
    if !matches!(kind, OrderType::StopLoss | OrderType::TakeProfit) {
        return None;
    }
    match working_type? {
        "MARK_PRICE" => Some(StopTriggerType::MarkPrice),
        "CONTRACT_PRICE" => Some(StopTriggerType::LastPrice),
        _ => None,
    }
}

/// `createPositionId`: one-way positions are keyed by symbol alone.
pub fn position_id(symbol: &str, side: PositionSide, mode: PositionMode) -> String {
    match mode {
        PositionMode::Oneway => symbol.to_owned(),
        PositionMode::Hedged => format!(
            "{symbol}:{}",
            match side {
                PositionSide::Long => "long",
                PositionSide::Short => "short",
            }
        ),
    }
}

fn position_side(ps: &str, signed_contracts: f64) -> PositionSide {
    match ps {
        "LONG" => PositionSide::Long,
        "SHORT" => PositionSide::Short,
        _ if signed_contracts >= 0.0 => PositionSide::Long,
        _ => PositionSide::Short,
    }
}

/// The leg a rejected positionRisk row names; an unknown side or symbol names none.
fn rejected_position_key(symbol: &str, ps: &str) -> Option<String> {
    let mode = match ps {
        "BOTH" => PositionMode::Oneway,
        "LONG" | "SHORT" => PositionMode::Hedged,
        _ => return None,
    };
    (!symbol.is_empty()).then(|| position_id(symbol, position_side(ps, 0.0), mode))
}

fn opt(value: &Option<std::borrow::Cow<'_, str>>) -> f64 {
    value.as_deref().map_or(0.0, num::parse)
}

fn nonnegative_number(value: &str) -> bool {
    value
        .parse::<f64>()
        .is_ok_and(|n| n.is_finite() && n >= 0.0)
}

/// A failed private read, classified by the Binance table (rules 2-6). The
/// message stays the venue's text; the code goes in the native evidence.
pub(crate) fn read_error(response: &Response<'_>) -> VenueError {
    use safe_cex_engine::classify::{self, Native, NativeCode, Released};
    let (code, message) =
        venue_error(response).unwrap_or_else(|| (0, format!("HTTP {}", response.status)));
    let native = (code != 0).then(|| Native {
        code: NativeCode::Int(code),
        message: &message,
    });
    classify::read(
        &crate::trading::BINANCE,
        Released::Response(response),
        native.as_ref(),
        message.clone(),
    )
    .error
}

/// A read lost in transport: `network`, by rule 2.
fn transport_error(message: String) -> VenueError {
    use safe_cex_engine::classify::{self, Released};
    classify::read(&crate::trading::BINANCE, Released::Transport, None, message).error
}

fn prefixed(mut error: VenueError, prefix: &str) -> VenueError {
    error.message = format!("{prefix}{}", error.message);
    error
}

/// The body's Binance error, if the request failed.
pub fn venue_error(response: &Response<'_>) -> Option<(i64, String)> {
    let body = response.body;
    let error = body
        .trim_start()
        .starts_with('{')
        .then(|| serde_json::from_str::<ErrorBody>(body).ok())
        .flatten();
    if (200..300).contains(&response.status) && error.as_ref().is_none_or(|e| e.code >= 0) {
        return None;
    }
    match error {
        Some(error) => Some((error.code, error.msg.into_owned())),
        None => Some((0, format!("HTTP {}: {}", response.status, body))),
    }
}

impl BinanceFutures {
    pub(crate) fn request_leverage_brackets(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.is_spot() {
            return;
        }
        self.background_signed_read(
            cx,
            self.paths.leverage_bracket,
            Query::new(),
            Some(safe_cex_engine::json::ArrayAt::Root),
            Pending::Account(Request::LeverageBrackets(HashMap::new())),
        );
    }

    /// Private startup once the catalogue supplies native market identities.
    pub(crate) fn start_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if std::mem::replace(&mut self.account.started, true) {
            return;
        }
        self.account.listen_key_failures = 0;
        self.request_listen_key(cx);
        if !self.is_spot() {
            self.background_signed_read(
                cx,
                self.paths.position_mode,
                Query::new(),
                None,
                Pending::Account(Request::PositionMode),
            );
        }
        self.fetch_positions(cx);
        self.fetch_account(cx);
        self.fetch_orders(cx);
        self.account.last_risk_at = cx.now;
        self.account.last_account_at = cx.now;
        self.account.tick_timer = Some(cx.set_poll_timer(HEALTH_CHECK_MS));
    }

    pub(crate) fn request_listen_key(&mut self, cx: &mut Cx<'_, Pending>) {
        if !cx.private_feed_policy().streaming() {
            return;
        }
        if self.is_spot() {
            self.reset_spot_subscription(cx);
            self.connect_url(cx, Channel::Private, self.urls.private_ws);
            return;
        }
        self.listen_key_request(cx, Method::Post, Request::ListenKey);
    }

    /// Create opens a new session generation; keepalive uses the current one.
    fn listen_key_request(&mut self, cx: &mut Cx<'_, Pending>, method: Method, request: Request) {
        if !cx.private_feed_policy().streaming() {
            return;
        }
        let mut session = self.account.listen_key_session.take();
        self.listen_key_session_request(cx, &mut session, method, request);
        self.account.listen_key_session = session;
    }

    fn fetch_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.is_spot() {
            cx.store.set_positions(Vec::new());
            return;
        }
        if self.account.positions.is_some() {
            return;
        }
        self.account.positions_generation += 1;
        let generation = self.account.positions_generation;
        let symbol_config = self.paths.symbol_config;
        self.account.positions = Some(PositionsJoin {
            generation,
            version: self.account.position_version,
            rows: None,
            skipped: SnapshotCoverage::default(),
            // Without a symbol config endpoint, settings come from the rows.
            configs: symbol_config.is_none().then(Vec::new),
            failed: false,
        });
        self.account.pending_position_events = false;
        self.background_signed_read(
            cx,
            self.paths.position_risk,
            Query::new(),
            Some(safe_cex_engine::json::ArrayAt::Root),
            Pending::Account(Request::PositionRisk {
                generation,
                rows: Vec::new(),
                marks: Vec::new(),
                skipped: SnapshotCoverage::default(),
            }),
        );
        if let Some(path) = symbol_config {
            self.background_signed_read(
                cx,
                path,
                Query::new(),
                Some(safe_cex_engine::json::ArrayAt::Root),
                Pending::Account(Request::SymbolConfig {
                    generation,
                    configs: Vec::new(),
                }),
            );
        }
    }

    fn fetch_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if std::mem::replace(&mut self.account.account_in_flight, true) {
            return;
        }
        self.account.account_read_predates_event = false;
        self.background_signed_read(
            cx,
            self.paths.balance,
            Query::new(),
            None,
            Pending::Account(Request::Account {
                version: self.account.account_version,
            }),
        );
    }

    /// Fresh open-orders read after an uncertain placement. Joins an in-flight
    /// snapshot by owing a new one behind it, since that read predates the mark;
    /// a re-ask (no new uncertain write) joins without owing another.
    pub(crate) fn reconcile_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.orders.is_some() {
            self.account.orders_owed |= !cx.reconcile_reask();
        } else {
            self.fetch_orders(cx);
        }
    }

    pub(crate) fn fetch_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.is_options() {
            self.options_read(cx, crate::options::Kind::Orders);
            return;
        }
        if self.account.orders.is_some() {
            return;
        }
        let Some(token) = cx.store.begin_orders_snapshot() else {
            self.account.orders_stale = true;
            cx.notice(
                Level::Error,
                code::SNAPSHOT_DISCARDED,
                "Binance order snapshot admission exhausted",
            );
            return;
        };
        self.account.orders_generation += 1;
        let generation = self.account.orders_generation;
        self.account.orders = Some(OrdersJoin {
            generation,
            token,
            epoch: cx.order_read_epoch(),
            native_orders: Vec::new(),
            native_strategies: Vec::new(),
            regular: None,
            algo: (self.is_spot() || self.is_inverse()).then(Vec::new),
            skipped: SnapshotCoverage::default(),
            rejected_symbols: HashMap::new(),
            failed: false,
        });
        self.background_signed_read(
            cx,
            self.paths.open_orders,
            Query::new(),
            None,
            Pending::Account(Request::OpenOrders { generation }),
        );
        if self.is_spot() || self.is_inverse() {
            return;
        }
        let conditional = if self.options.portfolio_margin {
            Query::new()
        } else {
            Query::new().str("algoType", "CONDITIONAL")
        };
        self.background_signed_read(
            cx,
            self.paths.open_conditional_orders,
            conditional,
            None,
            Pending::Account(Request::OpenAlgoOrders { generation }),
        );
    }

    /// Coalesce bursts of order events into one REST snapshot.
    pub(crate) fn schedule_orders_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(timer) = self.account.orders_refresh_timer.take() {
            cx.clear_timer(timer);
        }
        self.account.orders_refresh_timer = Some(cx.set_timer(ORDERS_REFRESH_DEBOUNCE_MS));
    }

    fn schedule_position_repair(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.background_paused(cx.now) {
            self.account.pending_position_events = false;
            return;
        }
        if self.account.repair_timer.is_some() {
            return;
        }
        let wait = self
            .account
            .last_repair_at
            .map_or(0.0, |at| POSITION_REPAIR_THROTTLE_MS - (cx.now - at));
        if wait <= 0.0 && self.account.positions.is_none() {
            self.account.last_repair_at = Some(cx.now);
            self.fetch_positions(cx);
        } else {
            self.account.repair_timer = Some(cx.set_timer(wait.max(1.0)));
        }
    }

    pub(crate) fn on_private_open(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.is_options() {
            self.options_refresh(cx);
            return;
        }
        if self.is_spot() {
            self.subscribe_spot_account(cx);
            return;
        }
        self.private_ready(cx);
    }

    fn private_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        // Order events while no stream was open were never delivered.
        if self.account.resync_orders_on_open || self.account.has_opened {
            self.account.resync_orders_on_open = false;
            self.schedule_orders_refresh(cx);
        }
        self.account.has_opened = true;
    }

    pub(crate) fn on_account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        let account = &mut self.account;
        if account.listen_key_timer == Some(timer) {
            account.listen_key_timer = None;
            if self.private.idle() {
                self.request_listen_key(cx);
            }
        } else if account.keepalive_timer == Some(timer) {
            account.keepalive_timer = None;
            self.listen_key_request(cx, Method::Put, Request::Keepalive);
        } else if account.orders_refresh_timer == Some(timer) {
            account.orders_refresh_timer = None;
            self.fetch_orders(cx);
        } else if account.repair_timer == Some(timer) {
            account.repair_timer = None;
            if account.pending_position_events && !self.background_paused(cx.now) {
                if self.account.positions.is_some() {
                    self.schedule_position_repair(cx);
                } else {
                    self.account.last_repair_at = Some(cx.now);
                    self.fetch_positions(cx);
                }
            }
        } else if account.tick_timer == Some(timer) {
            self.tick(cx);
        }
    }

    fn tick(&mut self, cx: &mut Cx<'_, Pending>) {
        let now = cx.now;
        self.account.tick_timer = Some(cx.set_poll_timer(HEALTH_CHECK_MS));
        self.revalue_pending_wallet(cx);
        if self.account.orders_stale && now - self.account.last_orders_retry_at >= ORDERS_RETRY_MS {
            self.account.last_orders_retry_at = now;
            self.fetch_orders(cx);
        }
        if self.background_paused(now) {
            return;
        }
        if now - self.account.last_risk_at >= POSITION_RISK_INTERVAL_MS {
            self.account.last_risk_at = now;
            self.fetch_positions(cx);
        }
        let interval = if self.private.open && !self.options.portfolio_margin {
            ACCOUNT_INTERVAL_MS
        } else {
            DEGRADED_ACCOUNT_INTERVAL_MS
        };
        if self.account.valuation_refresh_required || now - self.account.last_account_at >= interval
        {
            self.account.last_account_at = now;
            self.fetch_account(cx);
        }
    }

    pub(crate) fn on_account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        self.account_request_failed(
            cx,
            request,
            transport_error(format!("network error: {error}")),
        );
    }

    pub(crate) fn account_request_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        match request {
            Request::ListenKey => {
                let error = prefixed(error, "Binance user data stream unavailable: ");
                // One toast per failure streak; later attempts only log.
                let level = if self.account.listen_key_failures == 0 {
                    Level::Error
                } else {
                    Level::Warn
                };
                cx.notice_error(level, code::STREAM_UNAVAILABLE, &error);
                self.account.resync_orders_on_open = true;
                let delay = cx.jittered(LISTEN_KEY_RETRY.delay(self.account.listen_key_failures));
                self.account.listen_key_failures += 1;
                self.account.listen_key_timer = Some(cx.set_timer(delay));
            }
            Request::Keepalive => {
                let error = prefixed(error, "Binance user data stream keepalive failed: ");
                let level = if self.account.keepalive_failing {
                    Level::Warn
                } else {
                    Level::Error
                };
                cx.notice_error(level, code::STREAM_UNAVAILABLE, &error);
                self.account.keepalive_failing = true;
                self.account.keepalive_timer = Some(cx.set_timer(LISTEN_KEY_RETRY_MAX_MS));
            }
            Request::PositionRisk { generation, .. } | Request::SymbolConfig { generation, .. } => {
                cx.retried_failure_error(
                    "binance.account:615",
                    &prefixed(error, "Binance positions: "),
                );
                if let Some(join) = &mut self.account.positions
                    && join.generation == generation
                {
                    join.failed = true;
                }
                self.finish_positions(cx);
            }
            Request::Account { .. } => {
                self.account.account_in_flight = false;
                cx.retried_failure_error(
                    "binance.account:629",
                    &prefixed(error, "Binance account: "),
                );
            }
            Request::OpenOrders { generation } | Request::OpenAlgoOrders { generation } => {
                cx.retried_failure_error(
                    "binance.account:636",
                    &prefixed(error, "Binance fetchOrders error: "),
                );
                if let Some(join) = &mut self.account.orders
                    && join.generation == generation
                {
                    join.failed = true;
                }
                self.finish_orders(cx);
            }
            Request::LeverageBrackets(_) | Request::PositionMode => {
                cx.retried_failure_error("binance.account:649", &prefixed(error, "Binance: "));
            }
        }
    }

    /// One element of a streamed account list.
    pub(crate) fn on_account_item(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::LeverageBrackets(max_by_symbol) => {
                if let Ok(b) = serde_json::from_str::<LeverageBracket>(item)
                    && let Some(first) = b.brackets.first()
                    && first.initial_leverage > 0.0
                {
                    max_by_symbol.insert(b.symbol.into_owned(), first.initial_leverage);
                }
            }
            Request::PositionRisk {
                rows,
                generation,
                marks,
                skipped,
            } => {
                let Ok(p) = serde_json::from_str::<PositionSnapshot>(item) else {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Identity<'a> {
                        #[serde(borrow, default)]
                        symbol: Cow<'a, str>,
                        #[serde(borrow, default)]
                        position_side: Cow<'a, str>,
                    }
                    let key = serde_json::from_str::<Identity>(item)
                        .ok()
                        .and_then(|row| rejected_position_key(&row.symbol, &row.position_side));
                    let message = match &key {
                        Some(id) => format!("Binance skipped malformed positionRisk row {id}"),
                        None => "Binance skipped malformed positionRisk row without an identity"
                            .to_owned(),
                    };
                    self.reject_position_row(_cx, *generation, skipped, key, message);
                    return;
                };
                if self.is_inverse()
                    && num::parse(&p.position_amt) != 0.0
                    && _cx.store.market(&p.symbol).is_none()
                {
                    // A clean catalogue excludes delivery/non-trading contracts by design.
                    if !self.catalogue_partial {
                        return;
                    }
                    // Discovery is retrying the catalogue; hold the snapshot until it settles.
                    if let Some(join) = &mut self.account.positions
                        && join.generation == *generation
                    {
                        join.failed = true;
                    }
                    _cx.notice(Level::Warn, code::ROW_SKIPPED, "Binance inverse owned position market unavailable; preserving previous snapshot");
                    return;
                }
                if let (Some(leverage), Some(join)) = (p.leverage, &mut self.account.positions)
                    && join.generation == *generation
                    && let Some(configs) = &mut join.configs
                    && self.paths.symbol_config.is_none()
                {
                    let setting = MarketSetting {
                        leverage,
                        margin_type: Some(if p.margin_type.eq_ignore_ascii_case("isolated") {
                            MarginType::Isolated
                        } else {
                            MarginType::Cross
                        }),
                        ..MarketSetting::default()
                    };
                    configs.push((p.symbol.to_string(), setting));
                }
                if let Some(mark) = p
                    .mark_price
                    .as_deref()
                    .and_then(|price| price.parse::<f64>().ok())
                    .filter(|price| price.is_finite() && *price > 0.0)
                    && let Some(market) = _cx.store.market(&p.symbol)
                    && (market.quote == "USD" || STABLECOINS.contains(&market.quote.as_str()))
                {
                    marks.push((market.base.clone(), mark));
                }
                // An unreadable size is not flat: keep this leg's last known state.
                let Some(signed) = num::parse_decimal(&p.position_amt) else {
                    let key = rejected_position_key(&p.symbol, &p.position_side);
                    let message = format!(
                        "Binance positionRisk row {} has an unreadable positionAmt",
                        p.symbol
                    );
                    self.reject_position_row(_cx, *generation, skipped, key, message);
                    return;
                };
                if signed == 0.0 && !self.is_inverse() {
                    return;
                }
                let side = position_side(&p.position_side, signed);
                let mode = if p.position_side == "BOTH" {
                    PositionMode::Oneway
                } else {
                    PositionMode::Hedged
                };
                let entry_price = num::parse(&p.entry_price);
                let reference = self.inverse_reference_price(
                    _cx.store,
                    &p.symbol,
                    if opt(&p.mark_price) > 0.0 {
                        opt(&p.mark_price)
                    } else {
                        entry_price
                    },
                );
                let contract_size = _cx
                    .store
                    .market(&p.symbol)
                    .map_or(1.0, |m| m.contract_size.unwrap_or(1.0));
                let notional = if self.is_inverse() {
                    signed.abs() * contract_size
                } else {
                    opt(&p.notional).abs()
                };
                let contracts = if self.is_inverse() {
                    if reference > 0.0 {
                        notional / reference
                    } else {
                        0.0
                    }
                } else {
                    signed.abs()
                };
                let isolated = opt(&p.isolated_margin);
                let margin_size = if isolated > 0.0 {
                    isolated
                } else if self.is_inverse() {
                    contracts / p.leverage.unwrap_or(1.0).max(1.0)
                } else {
                    0.0
                };
                rows.push(Position {
                    id: position_id(&p.symbol, side, mode),
                    symbol: p.symbol.into_owned(),
                    side,
                    entry_price,
                    notional,
                    leverage: 0.0,
                    margin_type: None,
                    position_mode: Some(mode),
                    unrealized_pnl: num::parse(&p.un_realized_profit),
                    realized_pnl: None,
                    contracts,
                    liquidation_price: num::parse(&p.liquidation_price),
                    margin_size: Some(margin_size),
                    last_update: (p.update_time > 0.0).then_some(p.update_time),
                });
            }
            Request::SymbolConfig { configs, .. } => {
                if let Ok(c) = serde_json::from_str::<SymbolConfig>(item) {
                    let margin_type = if c.margin_type == "ISOLATED" {
                        MarginType::Isolated
                    } else {
                        MarginType::Cross
                    };
                    configs.push((
                        c.symbol.into_owned(),
                        MarketSetting {
                            leverage: c.leverage,
                            margin_type: Some(margin_type),
                            ..MarketSetting::default()
                        },
                    ));
                }
            }
            _ => {}
        }
    }

    pub(crate) fn on_account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if venue_error(&response).is_some() {
            self.account_request_failed(cx, request, read_error(&response));
            return;
        }
        let body = response.body;
        match request {
            Request::LeverageBrackets(max_by_symbol) => {
                cx.store.update_markets(|market| {
                    if let Some(max) = max_by_symbol.get(&market.symbol) {
                        market.limits.leverage.max = *max;
                    }
                });
            }
            Request::ListenKey => match serde_json::from_str::<ListenKey>(body) {
                Ok(key) => {
                    self.account.listen_key_failures = 0;
                    if let Some(timer) = self.account.keepalive_timer.take() {
                        cx.clear_timer(timer);
                    }
                    self.account.keepalive_timer = Some(cx.set_timer(LISTEN_KEY_REFRESH_MS));
                    let mut url = String::from(self.urls.private_ws);
                    url.push_str(if self.paths.listen_key_in_path {
                        "/"
                    } else {
                        "?listenKey="
                    });
                    url.push_str(&safe_cex_engine::url::encode(&key.listen_key));
                    self.connect_url(cx, Channel::Private, &url);
                }
                Err(error) => self.account_request_failed(
                    cx,
                    Request::ListenKey,
                    VenueError::new(error.to_string()),
                ),
            },
            Request::Keepalive => {
                self.account.keepalive_failing = false;
                self.account.keepalive_timer = Some(cx.set_timer(LISTEN_KEY_REFRESH_MS));
            }
            Request::PositionMode => {
                if let Ok(mode) = serde_json::from_str::<PositionModeBody>(body) {
                    cx.store.set_hedged(mode.dual_side_position);
                }
            }
            Request::PositionRisk {
                generation,
                rows,
                marks,
                skipped,
            } => {
                if let Some(join) = &mut self.account.positions
                    && join.generation == generation
                {
                    if !join.failed {
                        self.account.native_asset_marks.extend(marks);
                    }
                    join.rows = Some(rows);
                    join.skipped = skipped;
                }
                self.revalue_pending_wallet(cx);
                self.finish_positions(cx);
            }
            Request::SymbolConfig {
                generation,
                configs,
            } => {
                if let Some(join) = &mut self.account.positions
                    && join.generation == generation
                {
                    join.configs = Some(configs);
                }
                self.finish_positions(cx);
            }
            Request::Account { version } if self.options.portfolio_margin => {
                if version != self.account.account_version {
                    self.account.account_in_flight = false;
                    return;
                }
                self.account.account_in_flight = false;
                let mut balance = Balance::default();
                let parsed = json::each::<PmBalance>(body, |row| {
                    if !matches!(row.asset.as_ref(), "USDT" | "USDC" | "FDUSD") {
                        return;
                    }
                    let total = opt(&row.total_wallet_balance);
                    let free = opt(&row.cross_margin_free);
                    balance.total += total;
                    balance.free += free;
                    balance.used += total - free;
                    balance.upnl += opt(&row.um_unrealized_pnl);
                });
                match parsed {
                    Ok(()) => cx.store.set_balance(balance),
                    Err(error) => cx.notice(
                        Level::Error,
                        code::STREAM_MALFORMED,
                        format!("Binance balance: {error}"),
                    ),
                }
            }
            Request::Account { version } => {
                self.account.account_in_flight = false;
                if version != self.account.account_version {
                    if self.is_spot() && self.account.spot_subscription.is_some() {
                        self.fetch_account(cx);
                    } else if !self.is_spot() {
                        self.account.valuation_refresh_required = true;
                    }
                    return;
                }
                if self.is_spot() {
                    self.on_spot_account_snapshot(cx, body);
                    return;
                }
                match serde_json::from_str::<Account>(body) {
                    Ok(account) => {
                        self.account.valuation_refresh_required = false;
                        self.account.wallet_assets = Some(
                            account
                                .assets
                                .iter()
                                .map(|asset| WalletAsset {
                                    asset: asset.asset.to_string(),
                                    wallet: opt(&asset.wallet_balance),
                                    available: opt(&asset.available_balance),
                                    initial: opt(&asset.initial_margin),
                                    unrealized: opt(&asset.unrealized_profit),
                                })
                                .collect(),
                        );
                        self.account.valuation_pending = true;
                        self.account.valuation_warned = false;
                        self.revalue_pending_wallet(cx);
                        if std::mem::take(&mut self.account.account_read_predates_event)
                            && self.account.valuation_pending
                        {
                            self.account.wallet_assets = None;
                            self.account.valuation_pending = false;
                            self.account.valuation_refresh_required = true;
                            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "Binance pending wallet valuation superseded by private update; awaiting fresh wallet");
                        }
                    }
                    Err(error) => cx.notice(
                        Level::Error,
                        code::STREAM_MALFORMED,
                        format!("Binance account: {error}"),
                    ),
                }
            }
            Request::OpenOrders { generation } => {
                let now = cx.now;
                let mut orders = Vec::new();
                let mut native_orders = Vec::new();
                let mut rejected = Vec::new();
                let mut limit_stops = Vec::new();
                let client_keyed = self.options.portfolio_margin || self.is_spot();
                let spot = self.is_spot();
                let parsed = json::each::<OpenOrder>(body, |o| {
                    let id = match (&o.client_order_id, client_keyed) {
                        (Some(client), true) => client.to_string(),
                        _ => o.order_id.to_string(),
                    };
                    let Some(kind) = order_type(&o.kind) else {
                        let message = format!("Binance open order {id} has an unsupported type");
                        rejected.push((id, o.symbol.to_string(), message));
                        return;
                    };
                    limit_stops.push((id.clone(), executes_at_limit(&o.kind, spot)));
                    if client_keyed
                        && (!matches!(o.side.as_ref(), "BUY" | "SELL")
                            || [&o.price, &o.orig_qty, &o.executed_qty]
                                .iter()
                                .any(|n| !nonnegative_number(n))
                            || o.stop_price
                                .as_deref()
                                .is_some_and(|n| !nonnegative_number(n))
                            || num::parse(&o.executed_qty) > num::parse(&o.orig_qty))
                    {
                        let message = format!("Binance open order {id} has invalid economics");
                        rejected.push((id, o.symbol.to_string(), message));
                        return;
                    }
                    let price = num::parse(&o.price);
                    let amount = num::parse(&o.orig_qty);
                    let filled = num::parse(&o.executed_qty);
                    native_orders.push((o.symbol.to_string(), id.clone(), o.order_id, filled));
                    orders.push(Order {
                        id,
                        parent_id: None,
                        status: OrderStatus::Open,
                        symbol: o.symbol.into_owned(),
                        kind,
                        side: order_side(&o.side),
                        price: if price != 0.0 {
                            price
                        } else {
                            opt(&o.stop_price)
                        },
                        amount,
                        filled,
                        remaining: amount - filled,
                        reduce_only: o.reduce_only,
                        close: Some(o.close_position),
                        client_order_id: o.client_order_id.map(|c| c.into_owned()),
                        time_in_force: if self.is_spot() && o.kind == "LIMIT_MAKER" {
                            Some(TimeInForce::PostOnly)
                        } else {
                            o.time_in_force.as_deref().and_then(time_in_force)
                        },
                        last_update: now,
                        trigger_type: trigger_type(kind, o.working_type.as_deref()),
                        take_profit: None,
                        stop_loss: None,
                        extra: None,
                    });
                });
                self.note_limit_stops(limit_stops);
                if self.is_inverse() {
                    for order in &mut orders {
                        order.extra = Some(OrderExtra {
                            remaining_native: Some(order.remaining),
                            ..OrderExtra::default()
                        });
                    }
                }
                if let Some(join) = &mut self.account.orders
                    && join.generation == generation
                {
                    match parsed {
                        Ok(()) => {
                            join.regular = Some(orders);
                            join.native_orders = native_orders;
                            self.reject_orders(cx, generation, rejected);
                        }
                        Err(_) => join.failed = true,
                    }
                }
                self.finish_orders(cx);
            }
            Request::OpenAlgoOrders { generation } if self.options.portfolio_margin => {
                let now = cx.now;
                let mut orders = Vec::new();
                let mut native_strategies = Vec::new();
                let mut rejected = Vec::new();
                let mut limit_stops = Vec::new();
                let parsed = json::each::<PmConditionalOrder>(body, |o| {
                    let id = o.new_client_strategy_id.as_ref();
                    let Some(kind) = order_type(&o.strategy_type) else {
                        let message =
                            format!("Binance conditional order {id} has an unsupported type");
                        rejected.push((id.to_owned(), o.symbol.to_string(), message));
                        return;
                    };
                    limit_stops.push((id.to_owned(), executes_at_limit(&o.strategy_type, false)));
                    if !matches!(o.side.as_ref(), "BUY" | "SELL")
                        || [&o.price, &o.orig_qty, &o.stop_price]
                            .iter()
                            .any(|n| n.as_deref().is_some_and(|n| !nonnegative_number(n)))
                    {
                        let message =
                            format!("Binance conditional order {id} has invalid economics");
                        rejected.push((id.to_owned(), o.symbol.to_string(), message));
                        return;
                    }
                    let price = opt(&o.price);
                    let amount = opt(&o.orig_qty);
                    if let Some(native) = o.strategy_id {
                        native_strategies.push((
                            o.symbol.to_string(),
                            o.new_client_strategy_id.to_string(),
                            native,
                        ));
                    }
                    orders.push(Order {
                        id: o.new_client_strategy_id.into_owned(),
                        parent_id: None,
                        status: OrderStatus::Open,
                        symbol: o.symbol.into_owned(),
                        kind,
                        side: order_side(&o.side),
                        price: if price != 0.0 {
                            price
                        } else {
                            opt(&o.stop_price)
                        },
                        amount,
                        filled: 0.0,
                        remaining: amount,
                        reduce_only: o.reduce_only,
                        close: Some(false),
                        client_order_id: None,
                        time_in_force: None,
                        last_update: now,
                        trigger_type: trigger_type(kind, o.working_type.as_deref()),
                        take_profit: None,
                        stop_loss: None,
                        extra: None,
                    });
                });
                self.note_limit_stops(limit_stops);
                if let Some(join) = &mut self.account.orders
                    && join.generation == generation
                {
                    match parsed {
                        Ok(()) => {
                            join.algo = Some(orders);
                            join.native_strategies = native_strategies;
                            self.reject_orders(cx, generation, rejected);
                        }
                        Err(_) => join.failed = true,
                    }
                }
                self.finish_orders(cx);
            }
            Request::OpenAlgoOrders { generation } => {
                let now = cx.now;
                let mut orders = Vec::new();
                let mut rejected = Vec::new();
                let mut limit_stops = Vec::new();
                let parsed = json::each::<OpenAlgoOrder>(body, |o| {
                    let Some(kind) = order_type(&o.order_type) else {
                        let id = o.algo_id.to_string();
                        let message = format!("Binance open order {id} has an unsupported type");
                        rejected.push((id, o.symbol.to_string(), message));
                        return;
                    };
                    limit_stops.push((
                        o.algo_id.to_string(),
                        executes_at_limit(&o.order_type, false),
                    ));
                    let amount = opt(&o.quantity);
                    orders.push(Order {
                        id: o.algo_id.to_string(),
                        parent_id: None,
                        status: OrderStatus::Open,
                        symbol: o.symbol.into_owned(),
                        kind,
                        side: order_side(&o.side),
                        price: opt(&o.trigger_price),
                        amount,
                        filled: 0.0,
                        remaining: amount,
                        reduce_only: o.reduce_only,
                        close: Some(o.close_position),
                        client_order_id: None,
                        time_in_force: None,
                        last_update: now,
                        trigger_type: trigger_type(kind, o.working_type.as_deref()),
                        take_profit: None,
                        stop_loss: None,
                        extra: None,
                    });
                });
                self.note_limit_stops(limit_stops);
                if let Some(join) = &mut self.account.orders
                    && join.generation == generation
                {
                    match parsed {
                        Ok(()) => {
                            join.algo = Some(orders);
                            self.reject_orders(cx, generation, rejected);
                        }
                        Err(_) => join.failed = true,
                    }
                }
                self.finish_orders(cx);
            }
        }
    }

    /// Commit the positions snapshot once both halves are in.
    fn finish_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(join) = &self.account.positions else {
            return;
        };
        if !join.failed && (join.rows.is_none() || join.configs.is_none()) {
            return;
        }
        let join = self.account.positions.take().expect("checked above");
        if !join.failed {
            let configs = join.configs.unwrap_or_default();
            let settings: std::collections::HashMap<&str, MarketSetting> =
                configs.iter().map(|(s, c)| (s.as_str(), *c)).collect();
            if join.version == self.account.position_version {
                let mut positions: Vec<Position> = join
                    .rows
                    .unwrap_or_default()
                    .into_iter()
                    .map(|mut p| {
                        if let Some(config) = settings.get(p.symbol.as_str()) {
                            p.leverage = config.leverage;
                            p.margin_type = config.margin_type;
                        } else {
                            p.margin_type = Some(MarginType::Cross);
                        }
                        let isolated = p.margin_size.unwrap_or(0.0);
                        p.margin_size = if isolated > 0.0 {
                            Some(isolated)
                        } else if p.leverage > 0.0 {
                            Some(if self.is_inverse() {
                                p.contracts / p.leverage
                            } else {
                                p.notional / p.leverage
                            })
                        } else {
                            None
                        };
                        p
                    })
                    .collect();
                // A clean snapshot re-arms every skip log; otherwise only the legs it mapped.
                if join.skipped.rejected() == 0 {
                    self.account.position_skip_logged.clear();
                } else {
                    for p in &positions {
                        self.account.position_skip_logged.remove(&p.id);
                    }
                }
                for current in cx.store.positions() {
                    if !join.skipped.allows_absence(&current.id)
                        && !positions.iter().any(|p| p.id == current.id)
                    {
                        positions.push(current.clone());
                    }
                }
                cx.store.set_positions(positions);
            } else {
                // A stream event landed after these reads were issued.
                self.account.pending_position_events = true;
            }
            let rows: Vec<(u32, MarketSetting)> = configs
                .iter()
                .filter_map(|(symbol, setting)| Some((cx.store.market_index(symbol)?, *setting)))
                .collect();
            cx.store.set_settings(rows);
        }
        if std::mem::take(&mut self.account.valuation_repair_pending) {
            self.fetch_positions(cx);
        } else if self.account.pending_position_events {
            self.schedule_position_repair(cx);
        }
    }

    pub(crate) fn client_order_row_is_foreign(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        client: &str,
    ) -> bool {
        cx.store
            .order(client)
            .is_some_and(|order| order.symbol != symbol)
    }

    fn known_client_binding_is_foreign(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        client: &str,
        native: u64,
        strategy: bool,
    ) -> bool {
        let known = if strategy {
            self.pm_strategy_native_order_id(symbol, client)
        } else {
            self.spot_native_order_id(symbol, client)
        };
        known == Some(native) && self.client_order_row_is_foreign(cx, symbol, client)
    }

    /// Commit the orders snapshot once both halves are in. A failed half is
    /// not authoritative: the store keeps its orders and the tick retries.
    fn finish_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(join) = &self.account.orders else {
            return;
        };
        if !join.failed && (join.regular.is_none() || join.algo.is_none()) {
            return;
        }
        let join = self.account.orders.take().expect("checked above");
        self.settle_orders(cx, join);
        if std::mem::take(&mut self.account.orders_owed) {
            self.fetch_orders(cx);
        }
    }

    fn settle_orders(&mut self, cx: &mut Cx<'_, Pending>, join: OrdersJoin) {
        if join.failed {
            self.account.orders_stale = true;
            return;
        }
        if !cx.store.orders_snapshot_current(&join.token) {
            self.account.orders_stale = true;
            self.schedule_orders_refresh(cx);
            return;
        }
        if join
            .native_orders
            .iter()
            .any(|(symbol, client, native, filled)| {
                self.account
                    .native_executed
                    .get(&(symbol.clone(), *native))
                    .is_some_and(|(seen, terminal)| *terminal || *seen > *filled)
                    || ((self.is_spot() || self.options.portfolio_margin)
                        && self.known_client_binding_is_foreign(cx, symbol, client, *native, false))
                    || (self.is_spot()
                        && self
                            .account
                            .spot_client_native
                            .get(&(symbol.clone(), client.clone()))
                            .is_some_and(|current| *current > *native))
            })
            || join
                .native_strategies
                .iter()
                .any(|(symbol, client, native)| {
                    self.account
                        .terminal_strategies
                        .contains(&(symbol.clone(), *native))
                        || self.known_client_binding_is_foreign(cx, symbol, client, *native, true)
                })
            || join
                .algo
                .iter()
                .flatten()
                .any(|order| self.account.terminal_algo_ids.contains(&order.id))
        {
            self.account.orders_stale = true;
            self.schedule_orders_refresh(cx);
            return;
        }
        if self.options.portfolio_margin {
            let mut clients = HashSet::new();
            let ambiguous = join
                .regular
                .iter()
                .flatten()
                .chain(join.algo.iter().flatten())
                .any(|order| !clients.insert(order.id.as_str()));
            let idless_conflict = join.algo.iter().flatten().any(|order| {
                !join
                    .native_strategies
                    .iter()
                    .any(|(symbol, client, _)| symbol == &order.symbol && client == &order.id)
                    && self
                        .account
                        .pm_order_owners
                        .get(&order.id)
                        .is_some_and(|owner| {
                            owner.symbol != order.symbol
                                || !matches!(owner.source, PmOrderSource::Strategy(_))
                        })
            });
            if ambiguous || idless_conflict {
                self.account.orders_stale = true;
                self.schedule_orders_refresh(cx);
                return;
            }
        }
        let clean = join.skipped.rejected() == 0;
        if !cx
            .store
            .commit_orders_snapshot([join.token], |current_rows| {
                self.account.orders_stale = false;
                let mut orders = join.regular.unwrap_or_default();
                orders.extend(join.algo.unwrap_or_default());
                // A clean snapshot re-arms every skip log; otherwise only the ids it mapped.
                if join.skipped.rejected() == 0 {
                    self.account.order_skip_logged.clear();
                } else {
                    for order in &orders {
                        self.account.order_skip_logged.remove(&order.id);
                    }
                }
                for (symbol, client, native, filled) in join.native_orders {
                    if self.is_spot() {
                        // Only the first accepted REST observation establishes history.
                        // Live events create a zero floor; ACKs never touch delivery.
                        self.account
                            .spot_delivery
                            .entry((symbol.clone(), native))
                            .or_insert(SpotDelivery {
                                bootstrap_floor: filled,
                                delivered_cumulative: filled,
                            });
                    }
                    let entry = self
                        .account
                        .native_executed
                        .entry((symbol.clone(), native))
                        .or_default();
                    entry.0 = entry.0.max(filled);
                    if self.is_spot() || self.options.portfolio_margin {
                        self.bind_regular_native(&symbol, &client, native);
                    }
                }
                for (symbol, client, native) in join.native_strategies {
                    self.bind_strategy_native(&symbol, &client, native);
                }
                for current in current_rows {
                    let terminal = self.account.order_terminal(current);
                    // A rejected row on another symbol is a different order reusing the id.
                    let same_order = join
                        .rejected_symbols
                        .get(&current.id)
                        .is_none_or(|symbols| {
                            symbols
                                .iter()
                                .any(|symbol| symbol.is_empty() || *symbol == current.symbol)
                        });
                    if current.status == OrderStatus::Open
                        && !terminal
                        && !join.skipped.allows_absence(&current.id)
                        && same_order
                        && !orders.iter().any(|o| o.id == current.id)
                    {
                        orders.push(current.clone());
                    }
                }
                orders
            })
        {
            self.account.orders_stale = true;
            self.schedule_orders_refresh(cx);
        } else if clean {
            // Published and fully mapped: this read may resolve an uncertain placement.
            cx.orders_reconciled_since(None, join.epoch);
        }
    }

    /// Keeps the last known state of listed orders that could not be mapped.
    fn reject_orders(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        generation: u32,
        rows: Vec<(String, String, String)>,
    ) {
        if self
            .account
            .orders
            .as_ref()
            .is_none_or(|j| j.generation != generation)
        {
            return;
        }
        for (id, symbol, message) in rows {
            let kept = cx.store.orders().iter().any(|o| {
                o.id == id
                    && (symbol.is_empty() || o.symbol == symbol)
                    && o.status == OrderStatus::Open
                    && !self.account.order_terminal(o)
            });
            let level = if kept { Level::Warn } else { Level::Error };
            log_skip(
                cx,
                &mut self.account.order_skip_logged,
                id.clone(),
                level,
                message,
            );
            let Some(join) = self.account.orders.as_mut() else {
                return;
            };
            join.rejected_symbols
                .entry(id.clone())
                .or_default()
                .push(symbol);
            join.skipped.reject(Some(id));
        }
    }

    /// Records an unmappable positionRisk row. With no stored copy to keep, it holds the
    /// snapshot while discovery is unsettled and is otherwise reported as an error.
    fn reject_position_row(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        generation: u32,
        skipped: &mut SnapshotCoverage<String>,
        key: Option<String>,
        message: String,
    ) {
        let stored = match &key {
            Some(id) => cx.store.position(id).is_some(),
            None => !cx.store.positions().is_empty(),
        };
        if !stored && !self.discovery_settled() {
            if let Some(join) = &mut self.account.positions
                && join.generation == generation
            {
                join.failed = true;
            }
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                "Binance positionRisk row unmappable during discovery; preserving previous snapshot",
            );
            return;
        }
        let level = if stored { Level::Warn } else { Level::Error };
        let logged = &mut self.account.position_skip_logged;
        log_skip(cx, logged, key.clone().unwrap_or_default(), level, message);
        skipped.reject(key);
    }

    /// USD totals over every asset: stablecoins at par, others priced by their
    /// USDT/USDC/BUSD market, or through BTC.
    pub(crate) fn catalogue_wallet_repair(&mut self, cx: &mut Cx<'_, Pending>) {
        self.revalue_pending_wallet(cx);
        if !self.is_spot() && self.account.valuation_pending && self.account.wallet_assets.is_some()
        {
            if self.account.positions.is_some() {
                self.account.valuation_repair_pending = true;
            } else {
                self.fetch_positions(cx);
            }
        }
    }
    pub(crate) fn revalue_pending_wallet(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.valuation_pending {
            return;
        }
        if self.is_spot() {
            self.commit_spot_balance(cx);
            return;
        }
        let Some(assets) = &self.account.wallet_assets else {
            return;
        };
        let mut missing_price = false;
        let mut zero_valued = Vec::new();
        let mut native_price_used = false;
        let mut balance = Balance::default();
        let mut details = Vec::new();
        let mut coins = Vec::new();
        let mut currencies = Vec::new();
        for asset in assets {
            let wallet = asset.wallet;
            let available = asset.available;
            let initial = asset.initial;
            let unrealized = asset.unrealized;
            if wallet == 0.0 && available == 0.0 && initial == 0.0 && unrealized == 0.0 {
                continue;
            }
            let price = if STABLECOINS.contains(&asset.asset.as_str()) {
                1.0
            } else {
                let public = self.public_asset_usd_price(cx.store, &asset.asset);
                if public.is_finite() && public > 0.0 {
                    public
                } else {
                    let private = self
                        .account
                        .native_asset_marks
                        .get(&asset.asset)
                        .copied()
                        .unwrap_or(0.0);
                    native_price_used |= private > 0.0;
                    private
                }
            };
            if price.is_finite() && price > 0.0 {
                balance.total += wallet * price;
                balance.free += available * price;
                balance.used += initial * price;
                balance.upnl += unrealized * price;
            } else if self.awaits_valuation_price(cx.store, cx.now, &asset.asset) {
                missing_price = true;
                cx.store.want_ticker_for_base(&asset.asset);
            } else {
                zero_valued.push(asset.asset.clone());
            }
            if self.is_inverse() {
                coins.push(CoinBalance {
                    base: asset.asset.to_string(),
                    amount: wallet,
                    notional: if price.is_finite() && price > 0.0 {
                        wallet * price
                    } else {
                        cx.store
                            .coin_balances()
                            .and_then(|coins| coins.iter().find(|coin| coin.base == asset.asset))
                            .map_or(0.0, |coin| coin.notional)
                    },
                });
                // Coin units, as TS's per-coin `currencies`; inverse markets read their margin coin.
                currencies.push(CurrencyBalance {
                    currency: asset.asset.to_string(),
                    balance: Balance {
                        total: wallet,
                        free: available,
                        used: initial,
                        upnl: unrealized,
                    },
                });
            }
            if wallet > 0.0 {
                details.push(format!("{}({wallet} @ {price})", asset.asset));
            }
        }
        if self.is_inverse() {
            cx.store.set_coin_balances(coins);
            currencies.sort_by(|a: &CurrencyBalance, b| a.currency.cmp(&b.currency));
            cx.store.set_currency_balances(currencies);
        }
        self.note_valuation_wait(cx, missing_price, zero_valued);
        if missing_price {
            if !self.account.valuation_warned {
                cx.notice(
                    Level::Warn,
                    code::VALUATION_UNAVAILABLE,
                    "Binance wallet valuation price unavailable; preserving previous USD balance",
                );
                self.account.valuation_warned = true;
            }
            return;
        }
        self.account.valuation_pending = native_price_used;
        cx.store.set_balance(balance.clone());
        if !self.account.logged_balance && !details.is_empty() {
            self.account.logged_balance = true;
            cx.notice(Level::Debug, code::BALANCE_SUMMARY,
                format!(
                    "Balance aggregation: {}; Total: ${:.2}, Free: ${:.2}, Used: ${:.2}, UPnL: ${:.2}",
                    details.join(", "),
                    balance.total,
                    balance.free,
                    balance.used,
                    balance.upnl
                ),
            );
        }
    }

    pub(crate) fn on_user_event(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        if self.is_spot() {
            self.on_spot_user_event(cx, text);
            return;
        }
        match event_type(text) {
            Some("ORDER_TRADE_UPDATE") => {
                if let Ok(event) = serde_json::from_str::<OrderUpdateEvent>(text) {
                    self.on_order_update(cx, event.o);
                    self.schedule_orders_refresh(cx);
                }
            }
            Some("CONDITIONAL_ORDER_TRADE_UPDATE") => {
                if let Ok(event) = serde_json::from_str::<ConditionalUpdateEvent>(text) {
                    self.on_conditional_update(cx, event.so);
                    self.schedule_orders_refresh(cx);
                }
            }
            Some("ALGO_UPDATE") => {
                if let Ok(event) = serde_json::from_str::<AlgoUpdateEvent>(text) {
                    self.on_algo_update(cx, event.o);
                    self.schedule_orders_refresh(cx);
                }
            }
            Some("ACCOUNT_UPDATE") => {
                if let Ok(event) = serde_json::from_str::<AccountUpdateEvent>(text)
                    && !(self.options.portfolio_margin && event.fs.as_deref() == Some("CM"))
                {
                    self.on_account_update(cx, event.a);
                }
            }
            Some("ACCOUNT_CONFIG_UPDATE") => {
                if let Ok(event) = serde_json::from_str::<ConfigUpdateEvent>(text)
                    && let Some(config) = event.ac
                {
                    let leverage = config.l;
                    cx.store
                        .update_setting(&config.s, |setting| setting.leverage = leverage);
                    let symbol = config.s.to_string();
                    cx.store.update_positions(|p| {
                        if p.symbol == symbol {
                            p.leverage = leverage;
                        }
                    });
                }
            }
            // The listen key is the stream's credential; the reconnect fetches a fresh one.
            Some("listenKeyExpired") => {
                self.retire(cx, Channel::Private, RetireReason::Authentication)
            }
            _ => {}
        }
    }

    fn on_order_update(&mut self, cx: &mut Cx<'_, Pending>, o: OrderUpdate<'_>) {
        cx.store.invalidate_order_snapshots();
        // Portfolio margin identifies orders by client order id.
        let id = match (&o.c, self.options.portfolio_margin) {
            (Some(client), true) => client.to_string(),
            _ => o.i.to_string(),
        };
        let status = o.status.as_ref();
        let kind = o.ot.as_deref().or(o.o.as_deref()).and_then(order_type);
        if let Some(native) = o.ot.as_deref().or(o.o.as_deref()) {
            self.note_limit_stops([(id.clone(), executes_at_limit(native, false))]);
        }
        if status == "NEW" && kind.is_none() {
            return;
        }
        let Some(amount) =
            o.q.parse::<f64>()
                .ok()
                .filter(|value| value.is_finite() && *value >= 0.0)
        else {
            return;
        };
        let Some(filled) =
            o.z.parse::<f64>()
                .ok()
                .filter(|value| value.is_finite() && *value >= 0.0)
        else {
            return;
        };
        if status == "NEW"
            && (filled > amount
                || !matches!(o.side.as_ref(), "BUY" | "SELL")
                || !nonnegative_number(&o.p)
                || o.sp.as_deref().is_some_and(|p| !nonnegative_number(p)))
        {
            return;
        }
        let inverse = self.is_inverse();
        let key = (o.s.to_string(), o.i);
        let (previous, native_terminal) = self
            .account
            .native_executed
            .get(&key)
            .copied()
            .unwrap_or_default();
        let foreign_row = self.client_order_row_is_foreign(cx, &o.s, &id);
        let source = PmOrderSource::Regular(o.i);
        let stale_incarnation = self.options.portfolio_margin
            && ((self.pm_owner_conflicts(&o.s, &id, source)
                && (native_terminal || status != "NEW"))
                || (foreign_row
                    && (status != "NEW" || self.spot_native_order_id(&o.s, &id) == Some(o.i))));
        if self.options.portfolio_margin && !stale_incarnation {
            self.bind_regular_native(&o.s, &id, o.i);
        }
        let terminal = native_terminal;
        let current_filled = previous.max(filled);
        let last_quantity = opt(&o.l);
        let price = o.last_price.as_deref().map_or(0.0, num::parse);
        // Execution identity is independent of lifecycle ordering: a cancellation
        // or stop-trigger EXPIRED can arrive before its final TRADE.
        // CALCULATED is a liquidation execution and carries its own trade id.
        let fresh_execution = if matches!(o.x.as_deref(), Some("TRADE" | "CALCULATED"))
            && last_quantity.is_finite()
            && last_quantity > 0.0
            && price.is_finite()
            && price > 0.0
        {
            match o.t {
                Some(trade) if trade >= 0 => {
                    let execution = (o.s.to_string(), o.i, trade);
                    let fresh = self.account.derivative_trades.insert(execution.clone());
                    if fresh {
                        self.account.derivative_trade_order.push_back(execution);
                        if self.account.derivative_trade_order.len() > DERIVATIVE_EXECUTION_WINDOW
                            && let Some(oldest) = self.account.derivative_trade_order.pop_front()
                        {
                            self.account.derivative_trades.remove(&oldest);
                        }
                    }
                    fresh
                }
                Some(_) => false,
                None => {
                    filled
                        > self
                            .account
                            .derivative_fill_watermarks
                            .get(&key)
                            .copied()
                            .unwrap_or(previous)
                }
            }
        } else {
            false
        };
        if fresh_execution {
            self.account
                .derivative_fill_watermarks
                .entry(key.clone())
                .and_modify(|seen| *seen = seen.max(filled))
                .or_insert(filled);
            cx.fill(Fill {
                id: None,
                order_id: id.clone(),
                symbol: o.s.to_string(),
                side: order_side(&o.side),
                price,
                amount: last_quantity,
            });
        }
        let is_terminal = matches!(
            status,
            "FILLED" | "CANCELED" | "CANCELLED" | "EXPIRED" | "EXPIRED_IN_MATCH" | "REJECTED"
        );
        self.account
            .native_executed
            .insert(key, (current_filled, terminal || is_terminal));
        if stale_incarnation {
            return;
        }
        if matches!(status, "PARTIALLY_FILLED" | "FILLED") && filled >= previous {
            let now = cx.now;
            cx.store.update_order(&id, |order| {
                order.filled = current_filled;
                order.remaining = (amount - current_filled).max(0.0);
                if inverse {
                    order
                        .extra
                        .get_or_insert_with(OrderExtra::default)
                        .remaining_native = Some(order.remaining);
                }
                order.last_update = now;
            });
        }
        if !terminal
            && filled >= previous
            && (status == "NEW"
                || (inverse
                    && cx.store.order(&id).is_none()
                    && matches!(status, "PARTIALLY_FILLED" | "FILLED")))
            && let Some(kind) = kind
        {
            let price = num::parse(&o.p);
            cx.store.upsert_order(Order {
                id: id.clone(),
                parent_id: None,
                status: OrderStatus::Open,
                symbol: o.s.to_string(),
                kind,
                side: order_side(&o.side),
                price: if price != 0.0 { price } else { opt(&o.sp) },
                amount,
                filled,
                remaining: amount - filled,
                reduce_only: o.reduce_only,
                close: Some(o.close_position),
                client_order_id: o.c.as_deref().map(str::to_owned),
                time_in_force: o.f.as_deref().and_then(time_in_force),
                last_update: cx.now,
                trigger_type: trigger_type(kind, o.wt.as_deref()),
                take_profit: None,
                stop_loss: None,
                extra: inverse.then(|| OrderExtra {
                    remaining_native: Some((amount - filled).max(0.0)),
                    ..OrderExtra::default()
                }),
            });
        }
        if matches!(
            status,
            "CANCELED" | "CANCELLED" | "EXPIRED" | "EXPIRED_IN_MATCH" | "REJECTED"
        ) && cx
            .store
            .order(&id)
            .is_none_or(|order| order.status != OrderStatus::Closed)
        {
            let code = o.r.as_deref().unwrap_or_default().to_ascii_uppercase();
            let post_only =
                code.contains("POST_ONLY") || code.contains("GTX") || code.contains("MAKER");
            cx.store.update_order(&id, |order| {
                order.status = OrderStatus::Canceled;
                if post_only {
                    order
                        .extra
                        .get_or_insert_with(OrderExtra::default)
                        .reject_reason = Some(RejectReason::PostOnly);
                }
            });
        }
        if inverse
            && self
                .account
                .unquantified_cancels
                .contains(&(o.s.to_string(), o.i))
            && status != "FILLED"
        {
            cx.store.set_order_status(&id, OrderStatus::Canceled);
        }
        if status == "FILLED" && filled >= previous {
            cx.store.set_order_status(&id, OrderStatus::Closed);
            if inverse {
                self.account
                    .unquantified_cancels
                    .remove(&(o.s.to_string(), o.i));
            }
        }
    }

    /// Portfolio margin's stop, take-profit and trailing orders.
    fn on_conditional_update(&mut self, cx: &mut Cx<'_, Pending>, o: ConditionalUpdate<'_>) {
        let id = o.c.to_string();
        cx.store.invalidate_order_snapshots();
        if o.os == "NEW"
            && (order_type(&o.st).is_none()
                || !matches!(o.side.as_ref(), "BUY" | "SELL")
                || [&o.q, &o.p, &o.sp]
                    .iter()
                    .any(|n| n.as_deref().is_some_and(|n| !nonnegative_number(n))))
        {
            return;
        }
        let is_terminal = matches!(
            o.os.as_ref(),
            "TRIGGERED" | "FINISHED" | "FILLED" | "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED"
        );
        if let Some(native) = o.si {
            let key = (o.s.to_string(), native);
            let terminal = self.account.terminal_strategies.contains(&key);
            let current = self
                .account
                .strategy_client_native
                .get(&(o.s.to_string(), id.clone()))
                .copied();
            if is_terminal {
                self.account.terminal_strategies.insert(key);
            }
            if (terminal && o.os == "NEW")
                || (self.pm_owner_conflicts(&o.s, &id, PmOrderSource::Strategy(native))
                    && (terminal || o.os != "NEW"))
                || (self.client_order_row_is_foreign(cx, &o.s, &id)
                    && (o.os != "NEW" || current == Some(native)))
            {
                return;
            }
            self.bind_strategy_native(&o.s, &id, native);
        } else {
            // An idless packet cannot prove ownership of a newer native row.
            if self.account.pm_order_owners.get(&id).is_some_and(|owner| {
                owner.symbol != o.s || !matches!(owner.source, PmOrderSource::Strategy(_))
            }) {
                return;
            }
            if is_terminal {
                self.observe_strategy_terminal(cx, &o.s, &id);
            }
        }
        match o.os.as_ref() {
            "TRIGGERED" | "FINISHED" | "FILLED" => {
                cx.store.set_order_status(&id, OrderStatus::Closed);
            }
            "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => {
                cx.store.set_order_status(&id, OrderStatus::Canceled);
            }
            "NEW" => {
                let Some(kind) = order_type(&o.st) else {
                    return;
                };
                self.note_limit_stops([(id.clone(), executes_at_limit(&o.st, false))]);
                let price = opt(&o.p);
                let amount = opt(&o.q);
                cx.store.upsert_order(Order {
                    id,
                    parent_id: None,
                    status: OrderStatus::Open,
                    symbol: o.s.to_string(),
                    kind,
                    side: order_side(&o.side),
                    price: if price != 0.0 { price } else { opt(&o.sp) },
                    amount,
                    filled: 0.0,
                    remaining: amount,
                    reduce_only: o.reduce_only,
                    close: Some(false),
                    client_order_id: None,
                    time_in_force: None,
                    last_update: cx.now,
                    trigger_type: trigger_type(kind, o.wt.as_deref()),
                    take_profit: None,
                    stop_loss: None,
                    extra: None,
                });
            }
            _ => {}
        }
    }

    fn on_algo_update(&mut self, cx: &mut Cx<'_, Pending>, o: AlgoUpdate<'_>) {
        let id = o.aid.to_string();
        cx.store.invalidate_order_snapshots();
        if matches!(
            o.status.as_ref(),
            "TRIGGERED"
                | "TRIGGERING"
                | "FINISHED"
                | "CANCELED"
                | "CANCELLED"
                | "EXPIRED"
                | "REJECTED"
        ) {
            self.account.terminal_algo_ids.insert(id.clone());
        } else if o.status == "NEW" && self.account.terminal_algo_ids.contains(&id) {
            return;
        }
        match o.status.as_ref() {
            "TRIGGERED" | "TRIGGERING" | "FINISHED" => {
                cx.store.set_order_status(&id, OrderStatus::Closed);
            }
            "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => {
                cx.store.set_order_status(&id, OrderStatus::Canceled);
            }
            "NEW" => {
                let Some(kind) = order_type(&o.o) else {
                    return;
                };
                self.note_limit_stops([(id.clone(), executes_at_limit(&o.o, false))]);
                let amount = opt(&o.q);
                cx.store.upsert_order(Order {
                    id,
                    parent_id: None,
                    status: OrderStatus::Open,
                    symbol: o.s.to_string(),
                    kind,
                    side: order_side(&o.side),
                    price: opt(&o.tp),
                    amount,
                    filled: 0.0,
                    remaining: amount,
                    reduce_only: o.reduce_only,
                    close: Some(o.cp),
                    client_order_id: None,
                    time_in_force: None,
                    last_update: cx.now,
                    trigger_type: trigger_type(kind, o.wt.as_deref()),
                    take_profit: None,
                    stop_loss: None,
                    extra: None,
                });
            }
            _ => {}
        }
    }

    /// Quantity and PnL apply at once; leverage, margin type and liquidation
    /// price come from the REST repair this schedules.
    fn on_account_update(&mut self, cx: &mut Cx<'_, Pending>, update: AccountUpdate<'_>) {
        // A pending valuation must never replay an older wallet after a newer
        // private balance/position event. Reconcile at the existing health tick;
        // settled accounts keep their usual REST cadence.
        let changed = !update.positions.is_empty()
            || update
                .balances
                .is_some_and(|rows| !matches!(rows.get().trim(), "[]" | "null"));
        if changed && self.account.account_in_flight && !self.account.valuation_pending {
            // Applied at once like any settled read; only a pending cache is fenced.
            self.account.account_read_predates_event = true;
        } else if changed && self.account.valuation_pending {
            self.account.account_version += 1;
            self.account.wallet_assets = None;
            self.account.valuation_pending = false;
            self.account.valuation_refresh_required = true;
            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "Binance pending wallet valuation superseded by private update; awaiting fresh wallet");
        }
        if update.positions.is_empty() {
            return;
        }
        if self.is_inverse()
            && self.catalogue_partial
            && update.positions.iter().any(|position| {
                num::parse(&position.pa) != 0.0 && cx.store.market(&position.s).is_none()
            })
        {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "Binance inverse owned position market unavailable; preserving previous snapshot",
            );
            return;
        }
        self.account.position_version += 1;
        self.account.pending_position_events = true;
        for p in &update.positions {
            if self.is_inverse() && cx.store.market(&p.s).is_none() {
                continue;
            }
            let signed = num::parse(&p.pa);
            let mode = if p.ps == "BOTH" {
                PositionMode::Oneway
            } else {
                PositionMode::Hedged
            };
            let event_side = position_side(&p.ps, signed);
            let id = position_id(&p.s, event_side, mode);
            let existing = cx.store.position(&id).cloned();
            if existing.is_none() && signed == 0.0 {
                continue;
            }
            let setting = cx.store.setting(&p.s);
            let entry_price = num::parse(&p.ep);
            let upnl = num::parse(&p.up);
            let side = match (&existing, signed == 0.0) {
                (Some(existing), true) => existing.side,
                _ => event_side,
            };
            let notional = if self.is_inverse() {
                signed.abs()
                    * cx.store
                        .market(&p.s)
                        .map_or(1.0, |m| m.contract_size.unwrap_or(1.0))
            } else {
                (signed * entry_price + upnl).abs()
            };
            let reference = self.inverse_reference_price(cx.store, &p.s, entry_price);
            let contracts = if self.is_inverse() {
                if reference > 0.0 {
                    notional / reference
                } else {
                    0.0
                }
            } else {
                signed.abs()
            };
            let leverage = existing
                .as_ref()
                .map(|e| e.leverage)
                .or(setting.map(|s| s.leverage))
                .unwrap_or(0.0);
            let isolated_wallet = opt(&p.iw);
            let margin_size = if isolated_wallet > 0.0 {
                Some(isolated_wallet)
            } else if leverage > 0.0 {
                Some(if self.is_inverse() {
                    contracts / leverage
                } else {
                    notional / leverage
                })
            } else {
                existing.as_ref().and_then(|e| e.margin_size)
            };
            cx.store.upsert_position(Position {
                id,
                symbol: p.s.to_string(),
                side,
                entry_price,
                notional,
                leverage,
                margin_type: existing
                    .as_ref()
                    .and_then(|e| e.margin_type)
                    .or(setting.and_then(|s| s.margin_type))
                    .or(Some(MarginType::Cross)),
                position_mode: Some(mode),
                unrealized_pnl: upnl,
                realized_pnl: existing.as_ref().and_then(|e| e.realized_pnl),
                contracts,
                liquidation_price: existing.as_ref().map_or(0.0, |e| e.liquidation_price),
                margin_size,
                last_update: Some(cx.now),
            });
        }
        self.schedule_position_repair(cx);
    }
}

fn usd_price(store: &safe_cex_engine::Store, asset: &str) -> f64 {
    let last = |symbol: &str| store.ticker(symbol).map_or(0.0, |t| t.last);
    let direct = store
        .markets()
        .iter()
        .find(|m| m.base == asset && matches!(m.quote.as_str(), "USDT" | "USDC" | "BUSD"))
        .map_or(0.0, |m| last(&m.symbol));
    if direct > 0.0 || asset == "BTC" {
        return direct;
    }
    let Some(btc_market) = store
        .markets()
        .iter()
        .find(|m| m.base == asset && m.quote == "BTC")
    else {
        return 0.0;
    };
    let btc = last("BTCUSDT").max(0.0);
    let btc = if btc > 0.0 { btc } else { last("BTCUSDC") };
    let in_btc = last(&btc_market.symbol);
    if in_btc > 0.0 && btc > 0.0 {
        in_btc * btc
    } else {
        0.0
    }
}

/// COIN-M omits USD-M's `notional`; its quantity is a count of USD contracts.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionSnapshot<'a> {
    #[serde(borrow)]
    symbol: Cow<'a, str>,
    #[serde(borrow)]
    position_side: Cow<'a, str>,
    #[serde(borrow)]
    position_amt: Cow<'a, str>,
    #[serde(borrow)]
    entry_price: Cow<'a, str>,
    #[serde(borrow)]
    un_realized_profit: Cow<'a, str>,
    #[serde(borrow)]
    liquidation_price: Cow<'a, str>,
    #[serde(borrow, default)]
    notional: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    mark_price: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    isolated_margin: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    margin_type: Cow<'a, str>,
    #[serde(
        default,
        deserialize_with = "safe_cex_engine::de::lenient_optional_number"
    )]
    leverage: Option<f64>,
    #[serde(default)]
    update_time: f64,
}

#[derive(Deserialize)]
struct SpotAccount<'a> {
    #[serde(borrow)]
    balances: Vec<SpotBalance<'a>>,
}
#[derive(Deserialize)]
struct SpotBalance<'a> {
    #[serde(borrow)]
    asset: Cow<'a, str>,
    #[serde(borrow)]
    free: Cow<'a, str>,
    #[serde(borrow)]
    locked: Cow<'a, str>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotSubscription {
    subscription_id: u64,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum SpotReply {
    Subscription(SpotSubscription),
    Subscriptions(Vec<SpotSubscription>),
    Other(serde::de::IgnoredAny),
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotEnvelope<'a> {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    result: Option<SpotReply>,
    #[serde(default)]
    subscription_id: Option<u64>,
    #[serde(borrow, default)]
    event: Option<SpotEvent<'a>>,
}
/// Both Spot execution and account records share the same envelope.
#[derive(Deserialize)]
struct SpotEvent<'a> {
    #[serde(borrow)]
    e: Cow<'a, str>,
    #[serde(rename = "B", borrow, default)]
    balances: Vec<SpotEventBalance<'a>>,
    #[serde(default)]
    i: Option<u64>,
    #[serde(default)]
    t: Option<i64>,
    #[serde(borrow, default)]
    c: Cow<'a, str>,
    #[serde(rename = "C", borrow, default)]
    original_client: Cow<'a, str>,
    #[serde(borrow, default)]
    s: Cow<'a, str>,
    #[serde(rename = "S", borrow, default)]
    side: Cow<'a, str>,
    #[serde(borrow, default)]
    o: Cow<'a, str>,
    #[serde(rename = "X", borrow, default)]
    status: Cow<'a, str>,
    #[serde(borrow, default)]
    x: Cow<'a, str>,
    #[serde(borrow, default)]
    p: Cow<'a, str>,
    #[serde(borrow, default)]
    q: Cow<'a, str>,
    #[serde(borrow, default)]
    z: Cow<'a, str>,
    #[serde(borrow, default)]
    l: Cow<'a, str>,
    #[serde(rename = "L", borrow, default)]
    last_price: Cow<'a, str>,
    #[serde(rename = "P", borrow, default)]
    stop_price: Cow<'a, str>,
    #[serde(borrow, default)]
    f: Cow<'a, str>,
}
#[derive(Deserialize)]
struct SpotEventBalance<'a> {
    #[serde(borrow)]
    a: Cow<'a, str>,
    #[serde(borrow)]
    f: Cow<'a, str>,
    #[serde(borrow)]
    l: Cow<'a, str>,
}

impl BinanceFutures {
    fn inverse_reference_price(
        &self,
        store: &safe_cex_engine::Store,
        symbol: &str,
        fallback: f64,
    ) -> f64 {
        store
            .ticker(symbol)
            .map(|t| if t.mark > 0.0 { t.mark } else { t.last })
            .filter(|p| *p > 0.0)
            .unwrap_or(fallback)
    }
    /// An unpriced funded asset holds the USD total only while a quote is
    /// plausible: a tradable route exists and its bounded wait has not run out.
    fn awaits_valuation_price(
        &self,
        store: &safe_cex_engine::Store,
        now: f64,
        asset: &str,
    ) -> bool {
        if self.catalogue_partial {
            return true;
        }
        let expired = self
            .account
            .valuation_wait_since
            .is_some_and(|since| now - since >= VALUATION_WAIT_MS);
        !expired
            && !self.account.zero_valued_assets.contains(asset)
            && self.asset_has_valuation_market(store, asset)
    }
    fn note_valuation_wait(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        missing_price: bool,
        zero_valued: Vec<String>,
    ) {
        if !missing_price {
            self.account.valuation_wait_since = None;
        } else if cx.store.loaded().tickers && self.account.valuation_wait_since.is_none() {
            self.account.valuation_wait_since = Some(cx.now);
        }
        // An asset that regained a price gets a fresh wait if it loses it again.
        self.account
            .zero_valued_assets
            .retain(|asset| zero_valued.contains(asset));
        for asset in zero_valued {
            if !STABLECOINS.contains(&asset.as_str())
                && self.account.zero_valued_assets.insert(asset.clone())
            {
                cx.notice(Level::Warn, code::VALUATION_UNAVAILABLE,
                    format!("Binance wallet asset {asset} has no supported valuation market; valued at 0, native amount retained"),
                );
            }
        }
    }
    /// Before the ticker seed any catalogued route may still be quoted; after
    /// it, only a TRADING market can produce a price (BREAK/settling cannot).
    fn asset_has_valuation_market(&self, store: &safe_cex_engine::Store, asset: &str) -> bool {
        if self.catalogue_partial {
            return true;
        }
        let seeded = store.loaded().tickers;
        let current = |market: &&Market| {
            (market.active || !seeded) && self.catalogue_symbols.contains(&market.symbol)
        };
        if self.is_spot() {
            return store.markets().iter().filter(current).any(|market| {
                market.base == asset && matches!(market.quote.as_str(), "USD" | "USDT" | "USDC")
            });
        }
        let direct =
            |market: &Market| market.quote == "USD" || STABLECOINS.contains(&market.quote.as_str());
        let btc_route = store
            .markets()
            .iter()
            .filter(current)
            .any(|market| matches!(market.symbol.as_str(), "BTCUSDT" | "BTCUSDC"));
        store.markets().iter().filter(current).any(|market| {
            market.base == asset
                && (direct(market) || (!self.is_inverse() && market.quote == "BTC" && btc_route))
        })
    }

    fn asset_usd_price(&self, store: &safe_cex_engine::Store, asset: &str) -> f64 {
        let public = self.public_asset_usd_price(store, asset);
        if public.is_finite() && public > 0.0 {
            public
        } else {
            self.account
                .native_asset_marks
                .get(asset)
                .copied()
                .unwrap_or(0.0)
        }
    }
    fn public_asset_usd_price(&self, store: &safe_cex_engine::Store, asset: &str) -> f64 {
        if self.is_spot() {
            if matches!(asset, "USD" | "USDT" | "USDC") {
                return 1.0;
            }
            for suffix in ["USDT", "USDC", "USD"] {
                if let Some(ticker) = store.ticker(&format!("{asset}{suffix}"))
                    && ticker.last > 0.0
                {
                    return ticker.last;
                }
            }
            return 0.0;
        }
        if asset == "USD" || STABLECOINS.contains(&asset) {
            return 1.0;
        }
        if self.is_inverse() {
            for suffix in ["USD_PERP", "USD", "USDT"] {
                if let Some(ticker) = store.ticker(&format!("{asset}{suffix}")) {
                    let price = if ticker.mark > 0.0 {
                        ticker.mark
                    } else {
                        ticker.last
                    };
                    if price > 0.0 {
                        return price;
                    }
                }
            }
            return 0.0;
        }
        let public = usd_price(store, asset);
        if public > 0.0 {
            return public;
        }
        for market in store.markets().iter().filter(|market| {
            market.base == asset
                && (market.quote == "USD" || STABLECOINS.contains(&market.quote.as_str()))
        }) {
            if let Some(ticker) = store.ticker(&market.symbol)
                && ticker.mark.is_finite()
                && ticker.mark > 0.0
            {
                return ticker.mark;
            }
        }
        0.0
    }
    pub(crate) fn reset_spot_subscription(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.is_spot() {
            return;
        }
        self.account.spot_subscribe_id = None;
        self.account.spot_subscribe_at = None;
        self.account.spot_heartbeat_at = None;
        self.account.spot_subscription = None;
        self.account.spot_heartbeat_id = None;
        self.account.account_version += 1;
        cx.store.invalidate_order_snapshots();
    }
    fn next_spot_request_id(&mut self, prefix: &str) -> String {
        let id = self.private.next_request_id.get();
        self.private.next_request_id.set(id + 1);
        format!("{prefix}-{id}")
    }
    fn subscribe_spot_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if !cx.private_feed_policy().streaming() {
            return;
        }
        let Some(socket) = self.private.socket else {
            return;
        };
        let id = self.next_spot_request_id("tealstreet-user-data");
        let Ok((_, credentials)) = self.credentials.select(CredentialRole::SessionAuth) else {
            return;
        };
        self.account.spot_subscription = None;
        self.account.spot_heartbeat_id = None;
        self.account.spot_subscribe_id = Some(id.clone());
        self.account.spot_subscribe_at = Some(cx.now);
        cx.send_signed(
            socket,
            String::new(),
            Box::new(crate::sign::SpotSubscribe::new(credentials.clone(), id)),
        );
    }
    pub(crate) fn ping_spot_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self
            .account
            .spot_subscribe_at
            .is_some_and(|at| cx.now - at >= crate::SILENCE_LIMIT_MS)
            || self
                .account
                .spot_heartbeat_at
                .is_some_and(|at| cx.now - at >= crate::SILENCE_LIMIT_MS)
        {
            self.fail_spot_stream(cx, "acknowledgement timed out");
            return;
        }
        let (Some(socket), Some(_)) = (self.private.socket, self.account.spot_subscription) else {
            return;
        };
        if self.account.spot_heartbeat_id.is_some() {
            return;
        }
        let id = self.next_spot_request_id("tealstreet-user-data-heartbeat");
        cx.send(
            socket,
            format!(r#"{{"id":"{id}","method":"session.subscriptions"}}"#),
        );
        self.account.spot_heartbeat_id = Some(id);
        self.account.spot_heartbeat_at = Some(cx.now);
    }
    fn fail_spot_stream(&mut self, cx: &mut Cx<'_, Pending>, reason: &str) {
        cx.notice(
            Level::Error,
            code::SUBSCRIPTION_REJECTED,
            format!("Binance Spot user-data subscription {reason}"),
        );
        self.retire(cx, Channel::Private, RetireReason::Authentication);
    }
    fn on_spot_user_event(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Ok(envelope) = serde_json::from_str::<SpotEnvelope<'_>>(text) else {
            return;
        };
        if let Some(id) = envelope.id {
            if self.account.spot_subscribe_id.as_deref() == Some(id.as_str()) {
                self.account.spot_subscribe_id = None;
                self.account.spot_subscribe_at = None;
                let Some(SpotReply::Subscription(subscription)) = envelope.result else {
                    self.fail_spot_stream(cx, "rejected");
                    return;
                };
                if envelope.status != Some(200) {
                    self.fail_spot_stream(cx, "rejected");
                    return;
                }
                self.account.spot_subscription = Some(subscription.subscription_id);
                self.private_ready(cx);
                self.account.account_version += 1;
                cx.store.invalidate_order_snapshots();
                self.account.orders_stale = true;
                if !self.account.account_in_flight {
                    self.fetch_account(cx);
                }
                if self.account.orders.is_none() {
                    self.fetch_orders(cx);
                }
                self.ping_spot_account(cx);
            } else if self.account.spot_heartbeat_id.as_deref() == Some(id.as_str()) {
                self.account.spot_heartbeat_id = None;
                self.account.spot_heartbeat_at = None;
                let valid = envelope.status == Some(200)
                    && matches!(envelope.result, Some(SpotReply::Subscriptions(ref subscriptions)) if subscriptions.iter().any(|s| Some(s.subscription_id) == self.account.spot_subscription));
                if !valid {
                    self.fail_spot_stream(cx, "is no longer active");
                }
            }
            return;
        }
        if self.account.spot_subscription.is_none()
            || envelope.subscription_id != self.account.spot_subscription
        {
            return;
        }
        let Some(event) = envelope.event else {
            return;
        };
        match event.e.as_ref() {
            "outboundAccountPosition" => {
                self.account.account_version += 1;
                for balance in event.balances {
                    self.account.spot_balances.insert(
                        balance.a.into_owned(),
                        (num::parse(&balance.f), num::parse(&balance.l)),
                    );
                }
                self.commit_spot_balance(cx);
            }
            "executionReport" => {
                self.on_spot_order(cx, event);
                self.schedule_orders_refresh(cx);
            }
            "eventStreamTerminated" => self.fail_spot_stream(cx, "terminated"),
            _ => {}
        }
    }
    fn on_spot_account_snapshot(&mut self, cx: &mut Cx<'_, Pending>, body: &str) {
        match serde_json::from_str::<SpotAccount<'_>>(body) {
            Ok(snapshot) => {
                self.account.spot_balances = snapshot
                    .balances
                    .into_iter()
                    .map(|b| {
                        (
                            b.asset.into_owned(),
                            (num::parse(&b.free), num::parse(&b.locked)),
                        )
                    })
                    .collect();
                self.commit_spot_balance(cx);
            }
            Err(error) => cx.notice(
                Level::Error,
                code::STREAM_MALFORMED,
                format!("Binance Spot account: {error}"),
            ),
        }
    }
    fn commit_spot_balance(&mut self, cx: &mut Cx<'_, Pending>) {
        let mut missing_price = false;
        let mut zero_valued = Vec::new();
        let mut balance = Balance::default();
        let mut coins = Vec::with_capacity(self.account.spot_balances.len());
        let mut currencies = Vec::with_capacity(self.account.spot_balances.len());
        for (asset, &(free, used)) in &self.account.spot_balances {
            let price = self.asset_usd_price(cx.store, asset);
            let total = free + used;
            if (free != 0.0 || used != 0.0) && !(price.is_finite() && price > 0.0) {
                if self.awaits_valuation_price(cx.store, cx.now, asset) {
                    cx.store.want_ticker_for_base(asset);
                    missing_price = true;
                } else {
                    zero_valued.push(asset.clone());
                }
            }
            balance.free += free * price;
            balance.used += used * price;
            balance.total += total * price;
            coins.push(CoinBalance {
                base: asset.clone(),
                amount: total,
                notional: if price.is_finite() && price > 0.0 {
                    total * price
                } else {
                    cx.store
                        .coin_balances()
                        .and_then(|coins| coins.iter().find(|coin| coin.base == *asset))
                        .map_or(0.0, |coin| coin.notional)
                },
            });
            currencies.push(CurrencyBalance {
                currency: asset.clone(),
                balance: Balance {
                    free,
                    used,
                    total,
                    upnl: 0.0,
                },
            });
        }
        coins.sort_by(|a, b| a.base.cmp(&b.base));
        currencies.sort_by(|a, b| a.currency.cmp(&b.currency));
        cx.store.set_coin_balances(coins);
        cx.store.set_currency_balances(currencies);
        self.note_valuation_wait(cx, missing_price, zero_valued);
        self.account.valuation_pending = missing_price;
        if missing_price {
            if !self.account.valuation_warned {
                cx.notice(Level::Warn, code::VALUATION_UNAVAILABLE, "Binance Spot wallet valuation price unavailable; preserving previous USD balance");
                self.account.valuation_warned = true;
            }
        } else {
            self.account.valuation_warned = false;
            cx.store.set_balance(balance);
        }
    }
    fn fresh_spot_execution(
        &mut self,
        key: &(String, u64),
        filled: f64,
        trade: Option<i64>,
    ) -> bool {
        let cursor = self.account.spot_delivery.entry(key.clone()).or_default();
        if filled <= cursor.bootstrap_floor {
            return false;
        }
        let fresh = match trade {
            Some(id) if id >= 0 => {
                let execution = (key.0.clone(), key.1, id);
                let fresh = self.account.spot_trades.insert(execution.clone());
                if fresh {
                    self.account.spot_trade_order.push_back(execution);
                    if self.account.spot_trade_order.len() > SPOT_EXECUTION_WINDOW
                        && let Some(oldest) = self.account.spot_trade_order.pop_front()
                    {
                        self.account.spot_trades.remove(&oldest);
                    }
                }
                fresh
            }
            Some(_) => false,
            // Legacy idless packets only support cumulative progress dedupe.
            None => filled > cursor.delivered_cumulative,
        };
        if fresh {
            cursor.delivered_cumulative = cursor.delivered_cumulative.max(filled);
        }
        fresh
    }

    /// A reused id takes its latest native type, so only current stop-limits refuse amend.
    fn note_limit_stops(&mut self, rows: impl IntoIterator<Item = (String, bool)>) {
        for (id, limit) in rows {
            if limit {
                self.account.limit_stops.insert(id);
            } else {
                self.account.limit_stops.remove(&id);
            }
        }
    }

    fn on_spot_order(&mut self, cx: &mut Cx<'_, Pending>, event: SpotEvent<'_>) {
        if event.c.is_empty() || event.s.is_empty() {
            return;
        }
        cx.store.invalidate_order_snapshots();
        let Some(native) = event.i else {
            return;
        };
        let kind = order_type(&event.o);
        if !matches!(event.side.as_ref(), "BUY" | "SELL")
            || !nonnegative_number(&event.q)
            || !nonnegative_number(&event.z)
            || (event.status == "NEW"
                && (kind.is_none()
                    || !nonnegative_number(&event.p)
                    || (!event.stop_price.is_empty() && !nonnegative_number(&event.stop_price))))
        {
            return;
        }
        let amount = num::parse(&event.q);
        let filled = num::parse(&event.z);
        if filled > amount {
            return;
        }
        let is_trade = event.x == "TRADE";
        let last_quantity = event
            .l
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n > 0.0);
        let last_price = event
            .last_price
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n > 0.0);
        if is_trade
            && (last_quantity.is_none()
                || last_price.is_none()
                || last_quantity.is_some_and(|last| last > filled)
                || event.t.is_some_and(|t| t < 0))
        {
            return;
        }
        let id = if !event.original_client.is_empty()
            && matches!(
                event.status.as_ref(),
                "CANCELED" | "EXPIRED" | "EXPIRED_IN_MATCH"
            ) {
            event.original_client.to_string()
        } else {
            event.c.to_string()
        };
        if event.status == "NEW" {
            self.note_limit_stops([(id.clone(), executes_at_limit(&event.o, true))]);
        }
        let key = (event.s.to_string(), native);
        let client_key = (event.s.to_string(), id.clone());
        // A valid stream observation is live even when its lifecycle total
        // already includes executions whose TRADE reports have not arrived.
        self.account.spot_delivery.entry(key.clone()).or_default();
        if is_trade && self.fresh_spot_execution(&key, filled, event.t) {
            cx.fill(Fill {
                id: None,
                order_id: id.clone(),
                symbol: event.s.to_string(),
                side: order_side(&event.side),
                price: last_price.expect("validated execution price"),
                amount: last_quantity.expect("validated execution quantity"),
            });
        }
        let (previous, was_terminal) = self
            .account
            .native_executed
            .get(&key)
            .copied()
            .unwrap_or_default();
        let terminal = was_terminal
            || matches!(
                event.status.as_ref(),
                "FILLED" | "CANCELED" | "REJECTED" | "EXPIRED" | "EXPIRED_IN_MATCH"
            );
        let current_filled = previous.max(filled);
        self.account
            .native_executed
            .insert(key.clone(), (current_filled, terminal));
        if event.status == "FILLED" {
            self.account.unquantified_cancels.remove(&key);
        }
        let historical = self
            .account
            .spot_client_native
            .get(&client_key)
            .is_some_and(|current| *current > native)
            || (self.client_order_row_is_foreign(cx, &event.s, &id)
                && (event.status != "NEW"
                    || self.account.spot_client_native.get(&client_key) == Some(&native)));
        // Native executions remain observable independently of lifecycle and
        // canonical client-ID ownership; old rows never steal a newer binding.
        if historical || filled < previous || (was_terminal && !is_trade) {
            return;
        }
        let new_incarnation = !was_terminal
            && self.account.spot_client_native.insert(client_key, native) != Some(native);
        if !was_terminal
            && (event.status == "NEW"
                || ((new_incarnation || cx.store.order(&id).is_none())
                    && matches!(event.status.as_ref(), "PARTIALLY_FILLED" | "FILLED")))
        {
            let Some(kind) = kind else { return };
            let price = num::parse(&event.p);
            cx.store.upsert_order(Order {
                id: id.clone(),
                parent_id: None,
                status: OrderStatus::Open,
                symbol: event.s.to_string(),
                kind,
                side: order_side(&event.side),
                price: if price > 0.0 {
                    price
                } else {
                    num::parse(&event.stop_price)
                },
                amount,
                filled: current_filled,
                remaining: (amount - current_filled).max(0.0),
                reduce_only: false,
                close: Some(false),
                client_order_id: Some(id.clone()),
                time_in_force: if event.o == "LIMIT_MAKER" {
                    Some(TimeInForce::PostOnly)
                } else {
                    time_in_force(&event.f)
                },
                last_update: cx.now,
                trigger_type: None,
                take_profit: None,
                stop_loss: None,
                extra: None,
            });
        }
        let now = cx.now;
        cx.store.update_order(&id, |order| {
            order.filled = order.filled.max(current_filled);
            order.remaining = (amount - order.filled).max(0.0);
            order.last_update = now;
            if event.status == "FILLED" {
                order.status = OrderStatus::Closed;
            } else if terminal && order.status != OrderStatus::Closed {
                order.status = OrderStatus::Canceled;
            }
        });
    }
}

impl BinanceFutures {
    /// VENUE_STATE: the stop-limit ids, whose amend a frame-fed engine refuses as
    /// LOCAL does. Version byte, u32 count, then sorted ids as u32 length and
    /// UTF-8 bytes (sorted, so unchanged state is not resent).
    pub(crate) fn export_limit_stops(&self) -> Vec<u8> {
        let mut ids: Vec<&String> = self.account.limit_stops.iter().collect();
        ids.sort();
        let mut out = vec![VENUE_STATE_VERSION];
        out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
        for id in ids {
            out.extend_from_slice(&(id.len() as u32).to_le_bytes());
            out.extend_from_slice(id.as_bytes());
        }
        out
    }

    /// Replace the stop-limit ids with a data source's; `false` leaves them
    /// unchanged when the bytes are not a version this build reads.
    pub(crate) fn apply_limit_stops(&mut self, bytes: &[u8]) -> bool {
        let Some((&VENUE_STATE_VERSION, mut rest)) = bytes.split_first() else {
            return false;
        };
        let mut take = |length: usize| -> Option<&[u8]> {
            let (head, tail) = rest.split_at_checked(length)?;
            rest = tail;
            Some(head)
        };
        let count = |bytes: &[u8]| u32::from_le_bytes(bytes.try_into().unwrap()) as usize;
        let decoded = (|| {
            let mut ids = HashSet::new();
            for _ in 0..count(take(4)?) {
                let length = count(take(4)?);
                ids.insert(std::str::from_utf8(take(length)?).ok()?.to_owned());
            }
            Some(ids)
        })();
        match decoded {
            Some(ids) if rest.is_empty() => {
                self.account.limit_stops = ids;
                true
            }
            _ => false,
        }
    }

    pub(crate) fn pm_order_source(&self, symbol: &str, client: &str) -> Option<PmOrderSource> {
        self.account
            .pm_order_owners
            .get(client)
            .filter(|owner| owner.symbol == symbol)
            .map(|owner| owner.source)
    }

    pub(crate) fn pm_owner_matches(
        &self,
        symbol: &str,
        client: &str,
        source: PmOrderSource,
    ) -> Option<bool> {
        self.account
            .pm_order_owners
            .get(client)
            .map(|owner| owner.symbol == symbol && owner.source == source)
    }

    fn pm_owner_conflicts(&self, symbol: &str, client: &str, source: PmOrderSource) -> bool {
        self.pm_owner_matches(symbol, client, source) == Some(false)
    }

    fn bind_pm_owner(&mut self, symbol: &str, client: &str, source: PmOrderSource) {
        if !self.options.portfolio_margin {
            return;
        }
        if let Some(previous) = self.account.pm_order_owners.insert(
            client.to_owned(),
            PmOrderOwner {
                symbol: symbol.to_owned(),
                source,
            },
        ) && (previous.symbol != symbol || previous.source != source)
        {
            // Canonical client IDs span both native namespaces and all symbols.
            match previous.source {
                PmOrderSource::Regular(native) => {
                    self.account
                        .native_executed
                        .entry((previous.symbol, native))
                        .or_default()
                        .1 = true;
                }
                PmOrderSource::Strategy(native) => {
                    self.account
                        .terminal_strategies
                        .insert((previous.symbol, native));
                }
            }
        }
    }

    fn bind_regular_native(&mut self, symbol: &str, client: &str, native: u64) {
        self.bind_pm_owner(symbol, client, PmOrderSource::Regular(native));
        if let Some(previous) = self
            .account
            .spot_client_native
            .insert((symbol.to_owned(), client.to_owned()), native)
            && previous != native
        {
            self.account
                .native_executed
                .entry((symbol.to_owned(), previous))
                .or_default()
                .1 = true;
        }
    }

    fn bind_strategy_native(&mut self, symbol: &str, client: &str, native: u64) {
        self.bind_pm_owner(symbol, client, PmOrderSource::Strategy(native));
        if let Some(previous) = self
            .account
            .strategy_client_native
            .insert((symbol.to_owned(), client.to_owned()), native)
            && previous != native
        {
            self.account
                .terminal_strategies
                .insert((symbol.to_owned(), previous));
        }
    }

    pub(crate) fn observe_native_strategy_terminal(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        native: u64,
    ) {
        self.account
            .terminal_strategies
            .insert((symbol.to_owned(), native));
        cx.store.invalidate_order_snapshots();
    }

    pub(crate) fn observe_strategy_terminal(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        client: &str,
    ) {
        if let Some(native) = self.pm_strategy_native_order_id(symbol, client) {
            self.observe_native_strategy_terminal(cx, symbol, native);
        } else if !self.options.portfolio_margin {
            self.account.terminal_algo_ids.insert(client.to_owned());
            cx.store.invalidate_order_snapshots();
        }
    }

    pub(crate) fn pm_strategy_native_order_id(&self, symbol: &str, client: &str) -> Option<u64> {
        self.account
            .strategy_client_native
            .get(&(symbol.to_owned(), client.to_owned()))
            .copied()
    }

    pub(crate) fn spot_native_order_id(&self, symbol: &str, client: &str) -> Option<u64> {
        self.account
            .spot_client_native
            .get(&(symbol.to_owned(), client.to_owned()))
            .copied()
    }

    pub(crate) fn observe_unquantified_cancel(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        native: u64,
        filled: f64,
    ) {
        self.observe_native_terminal(cx, symbol, native, filled);
        self.account
            .unquantified_cancels
            .insert((symbol.to_owned(), native));
    }

    pub(crate) fn fence_order_snapshots(&mut self, cx: &mut Cx<'_, Pending>) {
        cx.store.invalidate_order_snapshots();
    }

    pub(crate) fn known_native_filled(&self, symbol: &str, native: u64) -> f64 {
        self.account
            .native_executed
            .get(&(symbol.to_owned(), native))
            .map_or(0.0, |(filled, _)| *filled)
    }

    pub(crate) fn observe_native_terminal(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        native: u64,
        filled: f64,
    ) {
        let execution = self
            .account
            .native_executed
            .entry((symbol.to_owned(), native))
            .or_default();
        execution.0 = execution.0.max(filled);
        execution.1 = true;
        cx.store.invalidate_order_snapshots();
    }
}

#[cfg(test)]
mod tests {
    use super::venue_error;
    use safe_cex_engine::Response;

    #[test]
    fn successful_http_status_does_not_hide_whitespace_formatted_binance_errors() {
        for body in [
            r#"{"code":-2010,"msg":"rejected"}"#,
            "{ \"code\" : -2010, \"msg\": \"rejected\" }",
        ] {
            assert_eq!(
                venue_error(&Response {
                    status: 200,
                    headers: "",
                    body
                }),
                Some((-2010, "rejected".to_owned()))
            );
        }
        assert_eq!(
            venue_error(&Response {
                status: 200,
                headers: "",
                body: r#"{"code":200,"msg":"success"}"#
            }),
            None
        );
        assert_eq!(
            venue_error(&Response {
                status: 200,
                headers: "",
                body: r#"{"assets":[]}"#
            }),
            None
        );
    }

    // A blank or malformed leverage used to decode as Some(0) and overwrite
    // the market's leverage setting with 0.
    #[test]
    fn unreadable_leverage_is_absent_not_zero() {
        let row = |leverage: &str| {
            format!(
                r#"{{"symbol":"BTCUSD_PERP","positionSide":"BOTH","positionAmt":"1","entryPrice":"1",
                "unRealizedProfit":"0","liquidationPrice":"0","notional":"1","leverage":{leverage}}}"#
            )
        };
        for (leverage, expected) in [
            ("\"20\"", Some(20.0)),
            ("20", Some(20.0)),
            ("\"\"", None),
            ("\"x\"", None),
            ("null", None),
        ] {
            let text = row(leverage);
            let snapshot: super::PositionSnapshot<'_> = serde_json::from_str(&text).unwrap();
            assert_eq!(snapshot.leverage, expected, "{leverage}");
            let risk: crate::wire::PositionRisk<'_> = serde_json::from_str(&text).unwrap();
            assert_eq!(risk.leverage, expected, "{leverage}");
        }
    }
}
