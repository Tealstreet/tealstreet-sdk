//! The private side: listenKey and user data stream, the account snapshot
//! reads and the reconciliation that keeps them honest.
use crate::sign::Query;
use crate::wire::*;
use crate::{BinanceFutures, Channel, Pending};
use safe_cex_engine::model::{
    Balance, MarginType, Order, OrderExtra, OrderSide, OrderStatus, OrderType, Position,
    PositionMode, PositionSide, RejectReason, StopTriggerType, TimeInForce,
};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, TimerId, json, num};

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
const STABLECOINS: [&str; 7] = ["USDT", "USDC", "BUSD", "FDUSD", "BFUSD", "LDUSDT", "BNFCR"];

pub enum Request {
    LeverageBrackets,
    ListenKey,
    Keepalive,
    PositionMode,
    PositionRisk { generation: u32 },
    SymbolConfig { generation: u32 },
    Account,
    OpenOrders { generation: u32 },
    OpenAlgoOrders { generation: u32 },
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
        "LIMIT" => OrderType::Limit,
        "MARKET" => OrderType::Market,
        "STOP_MARKET" | "STOP" => OrderType::StopLoss,
        "TAKE_PROFIT_MARKET" | "TAKE_PROFIT" => OrderType::TakeProfit,
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
    let looks_like_error = body.trim_start().starts_with('{') && body.contains("\"code\":-");
    if (200..300).contains(&response.status) && !looks_like_error {
        return None;
    }
    match serde_json::from_str::<ErrorBody>(body) {
        Ok(error) => Some((error.code, error.msg.into_owned())),
        Err(_) => Some((0, format!("HTTP {}: {}", response.status, body))),
    }
}

impl BinanceFutures {
    pub(crate) fn request_leverage_brackets(&self, cx: &mut Cx<'_, Pending>) {
        self.signed(
            cx,
            Method::Get,
            "/fapi/v1/leverageBracket",
            Query::new(),
            Pending::Account(Request::LeverageBrackets),
        );
    }

    /// Private startup once markets and tickers are loaded.
    pub(crate) fn start_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if std::mem::replace(&mut self.account.started, true) {
            return;
        }
        self.account.listen_key_retry_ms = LISTEN_KEY_RETRY_MIN_MS;
        self.request_listen_key(cx);
        self.signed(
            cx,
            Method::Get,
            "/fapi/v1/positionSide/dual",
            Query::new(),
            Pending::Account(Request::PositionMode),
        );
        self.fetch_positions(cx);
        self.fetch_account(cx);
        self.fetch_orders(cx);
        self.account.last_risk_at = cx.now;
        self.account.last_account_at = cx.now;
        self.account.tick_timer = Some(cx.set_timer(HEALTH_CHECK_MS));
    }

    pub(crate) fn request_listen_key(&mut self, cx: &mut Cx<'_, Pending>) {
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
                "/fapi/v1/listenKey",
                Query::new(),
                Pending::Account(request),
            );
        } else {
            cx.request(
                method,
                format!("{}/fapi/v1/listenKey", self.urls.rest),
                vec![("X-MBX-APIKEY", credentials.key.clone())],
                None,
                Pending::Account(request),
            );
        }
    }

    fn fetch_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.positions.is_some() {
            return;
        }
        self.account.positions_generation += 1;
        let generation = self.account.positions_generation;
        self.account.positions = Some(PositionsJoin {
            generation,
            version: self.account.position_version,
            rows: None,
            configs: None,
            failed: false,
        });
        self.account.pending_position_events = false;
        self.signed(
            cx,
            Method::Get,
            "/fapi/v3/positionRisk",
            Query::new(),
            Pending::Account(Request::PositionRisk { generation }),
        );
        self.signed(
            cx,
            Method::Get,
            "/fapi/v1/symbolConfig",
            Query::new(),
            Pending::Account(Request::SymbolConfig { generation }),
        );
    }

    fn fetch_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if std::mem::replace(&mut self.account.account_in_flight, true) {
            return;
        }
        self.signed(
            cx,
            Method::Get,
            "/fapi/v3/account",
            Query::new(),
            Pending::Account(Request::Account),
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
            regular: None,
            algo: None,
            failed: false,
        });
        self.signed(
            cx,
            Method::Get,
            "/fapi/v1/openOrders",
            Query::new(),
            Pending::Account(Request::OpenOrders { generation }),
        );
        self.signed(
            cx,
            Method::Get,
            "/fapi/v1/openAlgoOrders",
            Query::new().str("algoType", "CONDITIONAL"),
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
        let interval = if self.private.open {
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
            Request::PositionRisk { generation } | Request::SymbolConfig { generation } => {
                cx.log(LogLevel::Error, format!("Binance positions: {message}"));
                if let Some(join) = &mut self.account.positions
                    && join.generation == generation
                {
                    join.failed = true;
                }
                self.finish_positions(cx);
            }
            Request::Account => {
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
            Request::LeverageBrackets | Request::PositionMode => {
                cx.log(LogLevel::Error, format!("Binance: {message}"));
            }
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
            Request::LeverageBrackets => {
                let mut max_by_symbol = std::collections::HashMap::new();
                let _ = json::each::<LeverageBracket>(body, |b| {
                    if let Some(first) = b.brackets.first()
                        && first.initial_leverage > 0.0
                    {
                        max_by_symbol.insert(b.symbol.into_owned(), first.initial_leverage);
                    }
                });
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
                    url.push_str("?listenKey=");
                    url.push_str(&crate::sign::encode(&key.listen_key));
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
            Request::PositionRisk { generation } => {
                let mut rows = Vec::new();
                let parsed = json::each::<PositionRisk>(body, |p| {
                    let signed = num::parse(&p.position_amt);
                    if signed == 0.0 {
                        return;
                    }
                    let side = position_side(&p.position_side, signed);
                    let mode = if p.position_side == "BOTH" {
                        PositionMode::Oneway
                    } else {
                        PositionMode::Hedged
                    };
                    rows.push(Position {
                        id: position_id(&p.symbol, side, mode),
                        symbol: p.symbol.into_owned(),
                        side,
                        entry_price: num::parse(&p.entry_price),
                        notional: num::parse(&p.notional).abs(),
                        leverage: 0.0,
                        margin_type: None,
                        position_mode: Some(mode),
                        unrealized_pnl: num::parse(&p.un_realized_profit),
                        realized_pnl: None,
                        contracts: signed.abs(),
                        liquidation_price: num::parse(&p.liquidation_price),
                        margin_size: Some(opt(&p.isolated_margin)),
                        last_update: (p.update_time > 0.0).then_some(p.update_time),
                    });
                });
                if let Some(join) = &mut self.account.positions
                    && join.generation == generation
                {
                    match parsed {
                        Ok(()) => join.rows = Some(rows),
                        Err(_) => join.failed = true,
                    }
                }
                self.finish_positions(cx);
            }
            Request::SymbolConfig { generation } => {
                let mut configs = Vec::new();
                let parsed = json::each::<SymbolConfig>(body, |c| {
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
                        },
                    ));
                });
                if let Some(join) = &mut self.account.positions
                    && join.generation == generation
                {
                    match parsed {
                        Ok(()) => join.configs = Some(configs),
                        Err(_) => join.failed = true,
                    }
                }
                self.finish_positions(cx);
            }
            Request::Account => {
                self.account.account_in_flight = false;
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
                let parsed = json::each::<OpenOrder>(body, |o| {
                    let Some(kind) = order_type(&o.kind) else {
                        return;
                    };
                    let price = num::parse(&o.price);
                    let amount = num::parse(&o.orig_qty);
                    let filled = num::parse(&o.executed_qty);
                    orders.push(Order {
                        id: o.order_id.to_string(),
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
                        time_in_force: o.time_in_force.as_deref().and_then(time_in_force),
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
                        Ok(()) => join.regular = Some(orders),
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
                            Some(p.notional / p.leverage)
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
        self.account.orders_stale = false;
        let mut orders = join.regular.unwrap_or_default();
        orders.extend(join.algo.unwrap_or_default());
        cx.store.set_orders(orders);
    }

    /// USD totals over every asset: stablecoins at par, others priced by their
    /// USDT/USDC/BUSD market, or through BTC.
    fn aggregate_balance(&mut self, cx: &mut Cx<'_, Pending>, account: &Account<'_>) -> Balance {
        let mut balance = Balance::default();
        let mut details = Vec::new();
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
                usd_price(cx.store, &asset.asset)
            };
            if price > 0.0 {
                balance.total += wallet * price;
                balance.free += available * price;
                balance.used += initial * price;
                balance.upnl += unrealized * price;
            }
            if wallet > 0.0 {
                details.push(format!("{}({wallet} @ {price})", asset.asset));
            }
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
        match event_type(text) {
            Some("ORDER_TRADE_UPDATE") => {
                if let Ok(event) = serde_json::from_str::<OrderUpdateEvent>(text) {
                    self.on_order_update(cx, event.o);
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
        let id = o.i.to_string();
        let status = o.status.as_ref();
        let amount = num::parse(&o.q);
        let filled = num::parse(&o.z);
        if matches!(status, "PARTIALLY_FILLED" | "FILLED") {
            // One execution: its own quantity at its own price.
            let price = o
                .last_price
                .as_deref()
                .map(num::parse)
                .filter(|p| *p > 0.0)
                .unwrap_or_else(|| opt(&o.ap));
            cx.fill(Fill {
                order_id: id.clone(),
                symbol: o.s.to_string(),
                side: order_side(&o.side),
                price,
                amount: opt(&o.l),
            });
            let now = cx.now;
            cx.store.update_order(&id, |order| {
                order.filled = filled;
                order.remaining = amount - filled;
                order.last_update = now;
            });
        }
        if status == "NEW"
            && let Some(kind) = order_type(&o.ot)
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
                extra: None,
            });
        }
        if matches!(status, "CANCELED" | "CANCELLED" | "EXPIRED") {
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
        if status == "FILLED" {
            cx.store.set_order_status(&id, OrderStatus::Closed);
        }
    }

    fn on_algo_update(&mut self, cx: &mut Cx<'_, Pending>, o: AlgoUpdate<'_>) {
        let id = o.aid.to_string();
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
            let notional = (signed * entry_price + upnl).abs();
            let leverage = existing
                .as_ref()
                .map(|e| e.leverage)
                .or(setting.map(|s| s.leverage))
                .unwrap_or(0.0);
            let isolated_wallet = opt(&p.iw);
            let margin_size = if isolated_wallet > 0.0 {
                Some(isolated_wallet)
            } else if leverage > 0.0 {
                Some(notional / leverage)
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
                contracts: signed.abs(),
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
