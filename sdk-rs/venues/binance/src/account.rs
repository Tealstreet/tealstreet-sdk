//! The private side: listenKey and user data stream, the account snapshot
//! reads and the reconciliation that keeps them honest.
use crate::sign::Query;
use crate::wire::*;
use crate::{BinanceFutures, Channel, Pending};
use safe_cex_engine::model::{
    Balance, CoinBalance, CurrencyBalance, MarginType, Order, OrderExtra, OrderSide, OrderStatus,
    OrderType, Position, PositionMode, PositionSide, RejectReason, StopTriggerType, TimeInForce,
};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, TimerId, json, num};
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};

/// Binance expires a listenKey 60 minutes after the last keepalive.
const LISTEN_KEY_REFRESH_MS: f64 = 30.0 * 60_000.0;
const LISTEN_KEY_RETRY_MIN_MS: f64 = 5_000.0;
const LISTEN_KEY_RETRY_MAX_MS: f64 = 60_000.0;
const HEALTH_CHECK_MS: f64 = 3_000.0;
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
const STABLECOINS: [&str; 7] = ["USDT", "USDC", "BUSD", "FDUSD", "BFUSD", "LDUSDT", "BNFCR"];

pub enum Request {
    /// Collected while streaming: max leverage by symbol.
    LeverageBrackets(HashMap<String, f64>),
    ListenKey,
    Keepalive,
    PositionMode,
    PositionRisk {
        generation: u32,
        rows: Vec<Position>,
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
    configs: Option<Vec<(String, MarketSetting)>>,
    failed: bool,
}

struct OrdersJoin {
    generation: u32,
    version: u32,
    native_orders: Vec<(String, String, u64, f64)>,
    native_strategies: Vec<(String, String, u64)>,
    regular: Option<Vec<Order>>,
    algo: Option<Vec<Order>>,
    failed: bool,
}

#[derive(Default)]
pub struct AccountState {
    started: bool,
    listen_key_retry_ms: f64,
    listen_key_timer: Option<TimerId>,
    keepalive_timer: Option<TimerId>,
    keepalive_failing: bool,
    has_opened: bool,
    pub resync_orders_on_open: bool,
    tick_timer: Option<TimerId>,
    last_account_at: f64,
    account_in_flight: bool,
    account_version: u32,
    spot_balances: HashMap<String, (f64, f64)>,
    native_executed: HashMap<(String, u64), (f64, bool)>,
    derivative_trades: HashSet<(String, u64, i64)>,
    derivative_trade_order: VecDeque<(String, u64, i64)>,
    derivative_fill_watermarks: HashMap<(String, u64), f64>,
    terminal_algo_ids: HashSet<String>,
    strategy_client_native: HashMap<(String, String), u64>,
    terminal_strategies: HashSet<(String, u64)>,
    unquantified_cancels: HashSet<(String, u64)>,
    spot_client_native: HashMap<(String, String), u64>,
    spot_subscribe_id: Option<String>,
    spot_subscribe_at: Option<f64>,
    spot_subscription: Option<u64>,
    spot_heartbeat_id: Option<String>,
    spot_heartbeat_at: Option<f64>,
    order_version: u32,
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
    last_orders_retry_at: f64,
    logged_balance: bool,
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

fn opt(value: &Option<std::borrow::Cow<'_, str>>) -> f64 {
    value.as_deref().map_or(0.0, num::parse)
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
    pub(crate) fn request_leverage_brackets(&self, cx: &mut Cx<'_, Pending>) {
        if self.is_spot() {
            return;
        }
        self.signed_streamed(
            cx,
            self.paths.leverage_bracket,
            Query::new(),
            Pending::Account(Request::LeverageBrackets(HashMap::new())),
        );
    }

    /// Private startup once markets and tickers are loaded.
    pub(crate) fn start_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if std::mem::replace(&mut self.account.started, true) {
            return;
        }
        self.account.listen_key_retry_ms = LISTEN_KEY_RETRY_MIN_MS;
        self.request_listen_key(cx);
        if !self.is_spot() {
            self.signed(
                cx,
                Method::Get,
                self.paths.position_mode,
                Query::new(),
                Pending::Account(Request::PositionMode),
            );
        }
        self.fetch_positions(cx);
        self.fetch_account(cx);
        self.fetch_orders(cx);
        self.account.last_risk_at = cx.now;
        self.account.last_account_at = cx.now;
        self.account.tick_timer = Some(cx.set_timer(HEALTH_CHECK_MS));
    }

    pub(crate) fn request_listen_key(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.is_spot() {
            self.reset_spot_subscription(cx);
            self.connect_url(cx, Channel::Private, self.urls.private_ws);
            return;
        }
        self.listen_key_request(cx, Method::Post, Request::ListenKey);
    }

    /// Livenet listenKey requests carry only the API key; testnet signs them.
    fn listen_key_request(&mut self, cx: &mut Cx<'_, Pending>, method: Method, request: Request) {
        let Some(credentials) = &self.credentials else {
            return;
        };
        if self.options.testnet {
            self.signed(
                cx,
                method,
                self.paths.listen_key,
                Query::new(),
                Pending::Account(request),
            );
        } else {
            cx.request(
                method,
                format!("{}{}", self.urls.account_rest, self.paths.listen_key),
                vec![("X-MBX-APIKEY", credentials.key.clone())],
                None,
                Pending::Account(request),
            );
        }
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
            // Without a symbol config endpoint, settings come from the rows.
            configs: symbol_config.is_none().then(Vec::new),
            failed: false,
        });
        self.account.pending_position_events = false;
        self.signed_streamed(
            cx,
            self.paths.position_risk,
            Query::new(),
            Pending::Account(Request::PositionRisk {
                generation,
                rows: Vec::new(),
            }),
        );
        if let Some(path) = symbol_config {
            self.signed_streamed(
                cx,
                path,
                Query::new(),
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
        self.signed(
            cx,
            Method::Get,
            self.paths.balance,
            Query::new(),
            Pending::Account(Request::Account {
                version: self.account.account_version,
            }),
        );
    }

    pub(crate) fn fetch_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.orders.is_some() {
            return;
        }
        self.account.orders_generation += 1;
        let generation = self.account.orders_generation;
        self.account.orders = Some(OrdersJoin {
            generation,
            version: self.account.order_version,
            native_orders: Vec::new(),
            native_strategies: Vec::new(),
            regular: None,
            algo: (self.is_spot() || self.is_inverse()).then(Vec::new),
            failed: false,
        });
        self.signed(
            cx,
            Method::Get,
            self.paths.open_orders,
            Query::new(),
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
        self.signed(
            cx,
            Method::Get,
            self.paths.open_conditional_orders,
            conditional,
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
            if self.private.socket.is_none() && self.private.reconnect.is_none() {
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
        self.account.tick_timer = Some(cx.set_timer(HEALTH_CHECK_MS));
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
        if now - self.account.last_account_at >= interval {
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
        self.account_request_failed(cx, request, format!("network error: {error}"));
    }

    fn account_request_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        message: String,
    ) {
        match request {
            Request::ListenKey => {
                let message = format!("Binance user data stream unavailable: {message}");
                // One toast per failure streak; later attempts only log.
                let level = if self.account.listen_key_retry_ms <= LISTEN_KEY_RETRY_MIN_MS {
                    LogLevel::Error
                } else {
                    LogLevel::Warning
                };
                cx.log(level, message);
                self.account.resync_orders_on_open = true;
                let delay = self
                    .account
                    .listen_key_retry_ms
                    .max(LISTEN_KEY_RETRY_MIN_MS);
                self.account.listen_key_retry_ms = (delay * 2.0).min(LISTEN_KEY_RETRY_MAX_MS);
                self.account.listen_key_timer = Some(cx.set_timer(delay));
            }
            Request::Keepalive => {
                let message = format!("Binance user data stream keepalive failed: {message}");
                let level = if self.account.keepalive_failing {
                    LogLevel::Warning
                } else {
                    LogLevel::Error
                };
                cx.log(level, message);
                self.account.keepalive_failing = true;
                self.account.keepalive_timer = Some(cx.set_timer(LISTEN_KEY_RETRY_MAX_MS));
            }
            Request::PositionRisk { generation, .. } | Request::SymbolConfig { generation, .. } => {
                cx.log(LogLevel::Error, format!("Binance positions: {message}"));
                if let Some(join) = &mut self.account.positions
                    && join.generation == generation
                {
                    join.failed = true;
                }
                self.finish_positions(cx);
            }
            Request::Account { .. } => {
                self.account.account_in_flight = false;
                cx.log(LogLevel::Error, format!("Binance account: {message}"));
            }
            Request::OpenOrders { generation } | Request::OpenAlgoOrders { generation } => {
                cx.log(
                    LogLevel::Error,
                    format!("Binance fetchOrders error: {message}"),
                );
                if let Some(join) = &mut self.account.orders
                    && join.generation == generation
                {
                    join.failed = true;
                }
                self.finish_orders(cx);
            }
            Request::LeverageBrackets(_) | Request::PositionMode => {
                cx.log(LogLevel::Error, format!("Binance: {message}"));
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
            Request::PositionRisk { rows, generation } => {
                let Ok(p) = serde_json::from_str::<PositionSnapshot>(item) else {
                    if let Some(join) = &mut self.account.positions
                        && join.generation == *generation
                    {
                        join.failed = true;
                    }
                    _cx.log(LogLevel::Error, "Binance positionRisk row malformed");
                    return;
                };
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
                let signed = num::parse(&p.position_amt);
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
        if let Some((_, message)) = venue_error(&response) {
            self.account_request_failed(cx, request, message);
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
                    self.account.listen_key_retry_ms = LISTEN_KEY_RETRY_MIN_MS;
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
                Err(error) => {
                    self.account_request_failed(cx, Request::ListenKey, error.to_string())
                }
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
            Request::PositionRisk { generation, rows } => {
                if let Some(join) = &mut self.account.positions
                    && join.generation == generation
                {
                    join.rows = Some(rows);
                }
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
                    Err(error) => cx.log(LogLevel::Error, format!("Binance balance: {error}")),
                }
            }
            Request::Account { version } => {
                self.account.account_in_flight = false;
                if version != self.account.account_version {
                    if self.is_spot() && self.account.spot_subscription.is_some() {
                        self.fetch_account(cx);
                    }
                    return;
                }
                if self.is_spot() {
                    self.on_spot_account_snapshot(cx, body);
                    return;
                }
                match serde_json::from_str::<Account>(body) {
                    Ok(account) => {
                        let balance = self.aggregate_balance(cx, &account);
                        cx.store.set_balance(balance);
                    }
                    Err(error) => cx.log(LogLevel::Error, format!("Binance account: {error}")),
                }
            }
            Request::OpenOrders { generation } => {
                let now = cx.now;
                let mut orders = Vec::new();
                let mut native_orders = Vec::new();
                let parsed = json::each::<OpenOrder>(body, |o| {
                    let Some(kind) = order_type(&o.kind) else {
                        return;
                    };
                    let price = num::parse(&o.price);
                    let amount = num::parse(&o.orig_qty);
                    let filled = num::parse(&o.executed_qty);
                    let id = match (
                        &o.client_order_id,
                        self.options.portfolio_margin || self.is_spot(),
                    ) {
                        (Some(client), true) => client.to_string(),
                        _ => o.order_id.to_string(),
                    };
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
                let parsed = json::each::<PmConditionalOrder>(body, |o| {
                    let Some(kind) = order_type(&o.strategy_type) else {
                        return;
                    };
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
                if let Some(join) = &mut self.account.orders
                    && join.generation == generation
                {
                    match parsed {
                        Ok(()) => {
                            join.algo = Some(orders);
                            join.native_strategies = native_strategies;
                        }
                        Err(_) => join.failed = true,
                    }
                }
                self.finish_orders(cx);
            }
            Request::OpenAlgoOrders { generation } => {
                let now = cx.now;
                let mut orders = Vec::new();
                let parsed = json::each::<OpenAlgoOrder>(body, |o| {
                    let Some(kind) = order_type(&o.order_type) else {
                        return;
                    };
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
                if let Some(join) = &mut self.account.orders
                    && join.generation == generation
                {
                    match parsed {
                        Ok(()) => join.algo = Some(orders),
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
                let positions = join
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
        if self.account.pending_position_events {
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
        if join.failed {
            self.account.orders_stale = true;
            return;
        }
        if join.version != self.account.order_version {
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
        self.account.orders_stale = false;
        let mut orders = join.regular.unwrap_or_default();
        orders.extend(join.algo.unwrap_or_default());
        for (symbol, client, native, filled) in join.native_orders {
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
        cx.store.set_orders(orders);
    }

    /// USD totals over every asset: stablecoins at par, others priced by their
    /// USDT/USDC/BUSD market, or through BTC.
    fn aggregate_balance(&mut self, cx: &mut Cx<'_, Pending>, account: &Account<'_>) -> Balance {
        let mut balance = Balance::default();
        let mut details = Vec::new();
        let mut coins = Vec::new();
        for asset in &account.assets {
            let wallet = opt(&asset.wallet_balance);
            let available = opt(&asset.available_balance);
            let initial = opt(&asset.initial_margin);
            let unrealized = opt(&asset.unrealized_profit);
            if wallet == 0.0 && available == 0.0 && initial == 0.0 && unrealized == 0.0 {
                continue;
            }
            let price = if STABLECOINS.contains(&asset.asset.as_ref()) {
                1.0
            } else {
                self.asset_usd_price(cx.store, &asset.asset)
            };
            if price > 0.0 {
                balance.total += wallet * price;
                balance.free += available * price;
                balance.used += initial * price;
                balance.upnl += unrealized * price;
            }
            if self.is_inverse() {
                coins.push(CoinBalance {
                    base: asset.asset.to_string(),
                    amount: wallet,
                    notional: wallet * price,
                });
            }
            if wallet > 0.0 {
                details.push(format!("{}({wallet} @ {price})", asset.asset));
            }
        }
        if self.is_inverse() {
            cx.store.set_coin_balances(coins);
        }
        if !self.account.logged_balance && !details.is_empty() {
            self.account.logged_balance = true;
            cx.log(
                LogLevel::Info,
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
        balance
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
                if let Ok(event) = serde_json::from_str::<AccountUpdateEvent>(text) {
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
            Some("listenKeyExpired") => {
                if let Some(socket) = self.private.socket {
                    cx.close(socket);
                    self.on_socket_closed(cx, Channel::Private, 4001);
                }
            }
            _ => {}
        }
    }

    fn on_order_update(&mut self, cx: &mut Cx<'_, Pending>, o: OrderUpdate<'_>) {
        self.account.order_version += 1;
        // Portfolio margin identifies orders by client order id.
        let id = match (&o.c, self.options.portfolio_margin) {
            (Some(client), true) => client.to_string(),
            _ => o.i.to_string(),
        };
        let status = o.status.as_ref();
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
        let inverse = self.is_inverse();
        let key = (o.s.to_string(), o.i);
        let (previous, native_terminal) = self
            .account
            .native_executed
            .get(&key)
            .copied()
            .unwrap_or_default();
        let foreign_row = self.client_order_row_is_foreign(cx, &o.s, &id);
        let stale_incarnation = self.options.portfolio_margin
            && ((native_terminal
                && self
                    .account
                    .spot_client_native
                    .get(&(o.s.to_string(), id.clone()))
                    .is_some_and(|native| *native != o.i))
                || (foreign_row
                    && (status != "NEW"
                        || self
                            .account
                            .spot_client_native
                            .get(&(o.s.to_string(), id.clone()))
                            == Some(&o.i))));
        if self.options.portfolio_margin && !stale_incarnation {
            self.bind_regular_native(&o.s, &id, o.i);
        }
        let terminal = native_terminal;
        let current_filled = previous.max(filled);
        let last_quantity = opt(&o.l);
        let price = o.last_price.as_deref().map_or(0.0, num::parse);
        // Execution identity is independent of lifecycle ordering: a cancellation
        // or stop-trigger EXPIRED can arrive before its final TRADE.
        let fresh_execution = if o.x.as_deref() == Some("TRADE")
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
            && let Some(kind) = o.ot.as_deref().or(o.o.as_deref()).and_then(order_type)
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
        self.account.order_version += 1;
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
            if (terminal && (o.os == "NEW" || current.is_some_and(|current| current != native)))
                || (self.client_order_row_is_foreign(cx, &o.s, &id)
                    && (o.os != "NEW" || current == Some(native)))
            {
                return;
            }
            self.bind_strategy_native(&o.s, &id, native);
        } else if is_terminal {
            self.observe_strategy_terminal(&o.s, &id);
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
        self.account.order_version += 1;
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
        if update.positions.is_empty() {
            return;
        }
        self.account.position_version += 1;
        self.account.pending_position_events = true;
        for p in &update.positions {
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
    #[serde(default, deserialize_with = "optional_leverage")]
    leverage: Option<f64>,
    #[serde(default)]
    update_time: f64,
}

fn optional_leverage<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Scalar<'a> {
        Bare(f64),
        Text(#[serde(borrow)] Cow<'a, str>),
    }
    Ok(Option::<Scalar<'_>>::deserialize(d)?.map(|s| match s {
        Scalar::Bare(n) => n,
        Scalar::Text(s) => num::parse(&s),
    }))
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
    fn asset_usd_price(&self, store: &safe_cex_engine::Store, asset: &str) -> f64 {
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
        usd_price(store, asset)
    }
    pub(crate) fn reset_spot_subscription(&mut self, _cx: &mut Cx<'_, Pending>) {
        if !self.is_spot() {
            return;
        }
        self.account.spot_subscribe_id = None;
        self.account.spot_subscribe_at = None;
        self.account.spot_heartbeat_at = None;
        self.account.spot_subscription = None;
        self.account.spot_heartbeat_id = None;
        self.account.account_version += 1;
        self.account.order_version += 1;
    }
    fn next_spot_request_id(&mut self, prefix: &str) -> String {
        let id = self.private.next_request_id;
        self.private.next_request_id += 1;
        format!("{prefix}-{id}")
    }
    fn subscribe_spot_account(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(socket) = self.private.socket else {
            return;
        };
        let id = self.next_spot_request_id("tealstreet-user-data");
        let Some(credentials) = &self.credentials else {
            return;
        };
        let timestamp = cx.now.floor();
        let payload = format!(
            "apiKey={}&recvWindow=5000&timestamp={}",
            credentials.key,
            num::format(timestamp)
        );
        let signature = safe_cex_crypto::hmac_sha256(
            credentials.secret.as_bytes(),
            payload.as_bytes(),
            safe_cex_crypto::Encoding::Hex,
        );
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Params<'a> {
            api_key: &'a str,
            recv_window: u32,
            timestamp: f64,
            signature: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Subscription<'a> {
            id: &'a str,
            method: &'a str,
            params: Params<'a>,
        }
        let message = serde_json::to_string(&Subscription {
            id: &id,
            method: "userDataStream.subscribe.signature",
            params: Params {
                api_key: &credentials.key,
                recv_window: 5000,
                timestamp,
                signature: &signature,
            },
        })
        .expect("finite timestamp");
        self.account.spot_subscription = None;
        self.account.spot_heartbeat_id = None;
        self.account.spot_subscribe_id = Some(id);
        self.account.spot_subscribe_at = Some(cx.now);
        cx.send(socket, message);
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
        cx.log(
            LogLevel::Error,
            format!("Binance Spot user-data subscription {reason}"),
        );
        if let Some(socket) = self.private.socket {
            cx.close(socket);
            self.on_socket_closed(cx, Channel::Private, 4001);
        }
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
                self.account.order_version += 1;
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
            Err(error) => cx.log(LogLevel::Error, format!("Binance Spot account: {error}")),
        }
    }
    fn commit_spot_balance(&mut self, cx: &mut Cx<'_, Pending>) {
        let mut balance = Balance::default();
        let mut coins = Vec::with_capacity(self.account.spot_balances.len());
        let mut currencies = Vec::with_capacity(self.account.spot_balances.len());
        for (asset, &(free, used)) in &self.account.spot_balances {
            let price = self.asset_usd_price(cx.store, asset);
            let total = free + used;
            balance.free += free * price;
            balance.used += used * price;
            balance.total += total * price;
            coins.push(CoinBalance {
                base: asset.clone(),
                amount: total,
                notional: total * price,
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
        cx.store.set_balance(balance);
    }
    fn on_spot_order(&mut self, cx: &mut Cx<'_, Pending>, event: SpotEvent<'_>) {
        if event.c.is_empty() || event.s.is_empty() {
            return;
        }
        self.account.order_version += 1;
        let Some(native) = event.i else {
            return;
        };
        let id = if !event.original_client.is_empty()
            && matches!(
                event.status.as_ref(),
                "CANCELED" | "EXPIRED" | "EXPIRED_IN_MATCH"
            ) {
            event.original_client.to_string()
        } else {
            event.c.to_string()
        };
        let key = (event.s.to_string(), native);
        let client_key = (event.s.to_string(), id.clone());
        if self
            .account
            .spot_client_native
            .get(&client_key)
            .is_some_and(|current| *current > native)
            || (self.client_order_row_is_foreign(cx, &event.s, &id)
                && (event.status != "NEW"
                    || self.account.spot_client_native.get(&client_key) == Some(&native)))
        {
            // An unquantified cancel-all can precede the old order's final fill,
            // even after its caller ID has been reused for another native order.
            let (previous, terminal) = self
                .account
                .native_executed
                .get(&key)
                .copied()
                .unwrap_or_default();
            let filled = num::parse(&event.z);
            let terminal = terminal
                || matches!(
                    event.status.as_ref(),
                    "FILLED" | "CANCELED" | "REJECTED" | "EXPIRED" | "EXPIRED_IN_MATCH"
                );
            if event.x == "TRADE" && filled > previous && num::parse(&event.l) > 0.0 {
                cx.fill(Fill {
                    order_id: id,
                    symbol: event.s.to_string(),
                    side: order_side(&event.side),
                    price: num::parse(&event.last_price),
                    amount: num::parse(&event.l),
                });
                self.account
                    .native_executed
                    .insert(key.clone(), (filled.max(previous), terminal));
                if event.status == "FILLED" {
                    self.account.unquantified_cancels.remove(&key);
                }
            }
            // Lifecycle totals do not prove that their trades were delivered.
            self.account
                .native_executed
                .entry(key)
                .and_modify(|seen| seen.1 |= terminal)
                .or_insert((previous, terminal));
            return;
        }
        let amount = num::parse(&event.q);
        let filled = num::parse(&event.z);
        let (previous, terminal) = self
            .account
            .native_executed
            .get(&key)
            .copied()
            .unwrap_or_default();
        let unquantified = self.account.unquantified_cancels.contains(&key);
        if (terminal
            && !(unquantified
                && event.x == "TRADE"
                && filled > previous
                && num::parse(&event.l) > 0.0))
            || filled < previous
        {
            return;
        }
        let terminal = terminal
            || matches!(
                event.status.as_ref(),
                "FILLED" | "CANCELED" | "REJECTED" | "EXPIRED" | "EXPIRED_IN_MATCH"
            );
        self.account.native_executed.insert(key, (filled, terminal));
        let new_incarnation =
            self.account.spot_client_native.insert(client_key, native) != Some(native);
        if event.status == "NEW"
            || ((new_incarnation || cx.store.order(&id).is_none())
                && matches!(event.status.as_ref(), "PARTIALLY_FILLED" | "FILLED"))
        {
            let Some(kind) = order_type(&event.o) else {
                return;
            };
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
                filled,
                remaining: (amount - filled).max(0.0),
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
        if event.x == "TRADE" && filled > previous && num::parse(&event.l) > 0.0 {
            // `p` is the limit price, which is zero on market orders; `L` is execution price.
            cx.fill(Fill {
                order_id: id.clone(),
                symbol: event.s.to_string(),
                side: order_side(&event.side),
                price: num::parse(&event.last_price),
                amount: num::parse(&event.l),
            });
        }
        let now = cx.now;
        cx.store.update_order(&id, |order| {
            order.filled = filled;
            order.remaining = (amount - filled).max(0.0);
            order.last_update = now;
        });
        if unquantified && event.status != "FILLED" {
            cx.store.set_order_status(&id, OrderStatus::Canceled);
        }
        if event.status == "FILLED" {
            cx.store.set_order_status(&id, OrderStatus::Closed);
            self.account
                .unquantified_cancels
                .remove(&(event.s.to_string(), native));
        }
        if matches!(
            event.status.as_ref(),
            "CANCELED" | "REJECTED" | "EXPIRED" | "EXPIRED_IN_MATCH"
        ) {
            cx.store.set_order_status(&id, OrderStatus::Canceled);
        }
    }
}

impl BinanceFutures {
    fn bind_regular_native(&mut self, symbol: &str, client: &str, native: u64) {
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

    pub(crate) fn observe_native_strategy_terminal(&mut self, symbol: &str, native: u64) {
        self.account
            .terminal_strategies
            .insert((symbol.to_owned(), native));
        self.account.order_version += 1;
    }

    pub(crate) fn observe_strategy_terminal(&mut self, symbol: &str, client: &str) {
        if let Some(native) = self.pm_strategy_native_order_id(symbol, client) {
            self.observe_native_strategy_terminal(symbol, native);
        } else if !self.options.portfolio_margin {
            self.account.terminal_algo_ids.insert(client.to_owned());
            self.account.order_version += 1;
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

    pub(crate) fn observe_unquantified_cancel(&mut self, symbol: &str, native: u64, filled: f64) {
        self.observe_native_terminal(symbol, native, filled);
        self.account
            .unquantified_cancels
            .insert((symbol.to_owned(), native));
    }

    pub(crate) fn fence_order_snapshots(&mut self) {
        self.account.order_version += 1;
    }

    pub(crate) fn known_native_filled(&self, symbol: &str, native: u64) -> f64 {
        self.account
            .native_executed
            .get(&(symbol.to_owned(), native))
            .map_or(0.0, |(filled, _)| *filled)
    }

    pub(crate) fn observe_native_terminal(&mut self, symbol: &str, native: u64, filled: f64) {
        let execution = self
            .account
            .native_executed
            .entry((symbol.to_owned(), native))
            .or_default();
        execution.0 = execution.0.max(filled);
        execution.1 = true;
        self.account.order_version += 1;
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
}
