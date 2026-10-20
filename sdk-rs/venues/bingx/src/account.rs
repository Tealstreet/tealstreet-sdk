//! Private account state and its independently recovered listen key.
use crate::Pending;
use crate::account_wire::*;
use crate::sign::{Credentials, Params};
use safe_cex_engine::model::{
    Balance, MarginType, Order, OrderSide, OrderStatus, OrderType, Position, PositionMode,
    PositionSide, StopTriggerType, TimeInForce,
};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, SocketId, TimerId, json::ArrayAt};
use std::collections::{HashMap, HashSet, VecDeque};

const LISTEN_KEY: &str = "/openApi/user/auth/userDataStream";
const POSITION_MODE: &str = "/openApi/swap/v1/positionSide/dual";
const POSITIONS: &str = "/openApi/swap/v2/user/positions";
const BALANCE: &str = "/openApi/swap/v3/user/balance";
const OPEN_ORDERS: &str = "/openApi/swap/v2/trade/openOrders";
const REFRESH_MS: f64 = 30.0 * 60_000.0;
const RETRY_MS: f64 = 30_000.0;
const POLL_MS: f64 = 2_000.0;
// Keep recent terminal quantities across snapshots and reconnects. Evict one
// oldest identity at a time; clearing all state would replay every terminal fill.
const RETAINED_ORDERS: usize = 10_000;
const RETAINED_EXECUTIONS: usize = 20_000;
const PING_MS: f64 = 15_000.0;
const SILENCE_MS: f64 = 45_000.0;
const REPAIR_MS: f64 = 750.0;

pub enum Request {
    ListenKey {
        epoch: u64,
    },
    Keepalive {
        epoch: u64,
    },
    PositionMode {
        revision: u64,
    },
    Balance {
        revision: u64,
        first: Option<Balance>,
        usdt: Option<Balance>,
        failed: bool,
    },
    Positions {
        revision: u64,
        rows: Vec<Position>,
        settings: Vec<(String, MarketSetting)>,
        failed: bool,
    },
    OpenOrders {
        revision: u64,
        rows: Vec<Order>,
        metas: Vec<OrderMeta>,
        failed: bool,
    },
}

type OrderMeta = (Order, String, String, bool, String);

#[derive(Default)]
pub struct Account {
    started: bool,
    mode_known: bool,
    mode_in_flight: bool,
    epoch: u64,
    key_in_flight: bool,
    key: Option<String>,
    socket: Option<SocketId>,
    open: bool,
    received_at: f64,
    recovery: Option<TimerId>,
    keepalive: Option<TimerId>,
    heartbeat: Option<TimerId>,
    poll: Option<TimerId>,
    repair: Option<TimerId>,
    orders_revision: u64,
    positions_revision: u64,
    balance_revision: u64,
    orders_in_flight: Option<u64>,
    positions_in_flight: Option<u64>,
    balance_in_flight: Option<u64>,
    poll_count: u8,
    last_positions: f64,
    cumulative_fills: HashMap<String, f64>,
    terminal_orders: HashMap<String, f64>,
    seen_executions: HashSet<String>,
    // REST/z can cover executions never observed individually by this account.
    covered_history: HashSet<String>,
    retained_orders: VecDeque<String>,
    retained_terminals: VecDeque<String>,
    retained_executions: VecDeque<String>,
    observations: Vec<OrderMeta>,
}

pub fn order_type(text: &str) -> Option<OrderType> {
    Some(match text {
        "LIMIT" => OrderType::Limit,
        "MARKET" => OrderType::Market,
        "STOP" | "STOP_MARKET" | "TRIGGER_MARKET" | "TRIGGER_LIMIT" => OrderType::StopLoss,
        "TAKE_PROFIT" | "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        "TRAILING_STOP_MARKET" | "TRAILING_TP_SL" => OrderType::TrailingStopLoss,
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
pub fn position_id(symbol: &str, side: PositionSide, mode: PositionMode) -> String {
    match mode {
        PositionMode::Oneway => symbol.to_owned(),
        PositionMode::Hedged => format!(
            "{symbol}:{}",
            if side == PositionSide::Long {
                "long"
            } else {
                "short"
            }
        ),
    }
}

fn margin_type(text: &str) -> MarginType {
    if text.eq_ignore_ascii_case("isolated") {
        MarginType::Isolated
    } else {
        MarginType::Cross
    }
}
fn mode(cx: &Cx<'_, Pending>) -> PositionMode {
    if cx.store.hedged() {
        PositionMode::Hedged
    } else {
        PositionMode::Oneway
    }
}
fn position_side(text: &str, quantity: f64, mode: PositionMode) -> PositionSide {
    if mode == PositionMode::Oneway {
        if quantity < 0.0 {
            PositionSide::Short
        } else {
            PositionSide::Long
        }
    } else if text == "SHORT" {
        PositionSide::Short
    } else {
        PositionSide::Long
    }
}
fn time_in_force(text: &str) -> Option<TimeInForce> {
    Some(match text {
        "GTC" => TimeInForce::GoodTillCancel,
        "IOC" => TimeInForce::ImmediateOrCancel,
        "FOK" => TimeInForce::FillOrKill,
        "PostOnly" | "POST_ONLY" | "GTX" => TimeInForce::PostOnly,
        _ => return None,
    })
}

/// A venue error is a failed snapshot, including errors transported as HTTP 200.
fn response_error(response: Response<'_>, allow_missing_code: bool) -> Option<String> {
    if allow_missing_code
        && response.body.trim().is_empty()
        && (200..300).contains(&response.status)
    {
        return None;
    }
    let status = serde_json::from_str::<Status<'_>>(response.body);
    if let Ok(s) = &status {
        if s.code != 0 {
            return Some(format!("{} ({})", s.msg, s.code));
        }
    } else if !allow_missing_code || serde_json::from_str::<KeyStatus>(response.body).is_err() {
        return Some("malformed BingX response envelope".into());
    }
    if !(200..300).contains(&response.status) {
        return Some(format!("HTTP {}", response.status));
    }
    None
}

impl Account {
    pub fn handles_socket(&self, socket: SocketId) -> bool {
        self.socket == Some(socket)
    }
    pub fn handles_timer(&self, timer: TimerId) -> bool {
        [
            self.recovery,
            self.keepalive,
            self.heartbeat,
            self.poll,
            self.repair,
        ]
        .contains(&Some(timer))
    }
    pub fn bump_orders(&mut self) {
        self.orders_revision = self.orders_revision.wrapping_add(1);
    }
    pub fn bump_positions(&mut self) {
        self.positions_revision = self.positions_revision.wrapping_add(1);
    }
    pub fn drain_order_meta(&mut self) -> impl Iterator<Item = OrderMeta> + '_ {
        self.observations.drain(..)
    }

    pub fn start(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if std::mem::replace(&mut self.started, true) {
            return;
        }
        self.fetch_key(cx, credentials);
        self.fetch_mode(cx, credentials);
        self.fetch_balance(cx, credentials);
        self.fetch_orders(cx, credentials);
        self.poll_count = 1;
        self.poll = Some(cx.set_poll_timer(POLL_MS));
    }

    fn request(
        cx: &mut Cx<'_, Pending>,
        credentials: &Credentials,
        method: Method,
        path: &str,
        params: Params,
        request: Request,
        stream: bool,
    ) {
        let (url, headers, body) = params.into_request(method, path, Some(credentials), cx.now);
        if stream {
            let at = if path == OPEN_ORDERS {
                ArrayAt::EnvelopeFieldOrNested("data", "orders")
            } else {
                ArrayAt::EnvelopeField("data")
            };
            cx.request_streamed(method, url, headers, at, Pending::Account(request));
        } else {
            cx.request(method, url, headers, body, Pending::Account(request));
        }
    }

    fn fetch_key(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if self.key_in_flight || self.socket.is_some() {
            return;
        }
        self.key_in_flight = true;
        Self::request(
            cx,
            credentials,
            Method::Post,
            LISTEN_KEY,
            Params::new(),
            Request::ListenKey { epoch: self.epoch },
            false,
        );
    }
    fn fetch_mode(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if self.mode_in_flight {
            return;
        }
        self.mode_in_flight = true;
        Self::request(
            cx,
            credentials,
            Method::Get,
            POSITION_MODE,
            Params::new(),
            Request::PositionMode {
                revision: self.positions_revision,
            },
            false,
        );
    }
    fn fetch_balance(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if self.balance_in_flight.is_some() {
            return;
        }
        self.balance_in_flight = Some(self.balance_revision);
        Self::request(
            cx,
            credentials,
            Method::Get,
            BALANCE,
            Params::new(),
            Request::Balance {
                revision: self.balance_revision,
                first: None,
                usdt: None,
                failed: false,
            },
            true,
        );
    }
    fn fetch_positions(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if !self.mode_known || self.positions_in_flight.is_some() {
            return;
        }
        self.last_positions = cx.now;
        self.positions_in_flight = Some(self.positions_revision);
        Self::request(
            cx,
            credentials,
            Method::Get,
            POSITIONS,
            Params::new(),
            Request::Positions {
                revision: self.positions_revision,
                rows: Vec::new(),
                settings: Vec::new(),
                failed: false,
            },
            true,
        );
    }
    fn fetch_orders(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if self.orders_in_flight.is_some() {
            return;
        }
        self.orders_in_flight = Some(self.orders_revision);
        Self::request(
            cx,
            credentials,
            Method::Get,
            OPEN_ORDERS,
            Params::new(),
            Request::OpenOrders {
                revision: self.orders_revision,
                rows: Vec::new(),
                metas: Vec::new(),
                failed: false,
            },
            true,
        );
    }

    pub fn reconcile(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        self.fetch_orders(cx, credentials);
        self.fetch_positions(cx, credentials);
        self.fetch_balance(cx, credentials);
    }

    fn clear_private_timers(&mut self, cx: &mut Cx<'_, Pending>) {
        for timer in [&mut self.recovery, &mut self.keepalive, &mut self.heartbeat] {
            if let Some(timer) = timer.take() {
                cx.clear_timer(timer);
            }
        }
    }
    fn invalidate_private(&mut self, cx: &mut Cx<'_, Pending>) {
        self.epoch = self.epoch.wrapping_add(1);
        self.key_in_flight = false;
        self.key = None;
        self.open = false;
        self.clear_private_timers(cx);
        if let Some(socket) = self.socket.take() {
            cx.close(socket);
        }
        self.bump_orders();
        self.bump_positions();
    }
    fn fresh_key(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        self.invalidate_private(cx);
        self.fetch_key(cx, credentials);
    }

    pub fn on_open(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        self.open = true;
        self.received_at = cx.now;
        if let Some(timer) = self.heartbeat.take() {
            cx.clear_timer(timer);
        }
        self.heartbeat = Some(cx.set_timer(PING_MS));
        if let Some(timer) = self.keepalive.take() {
            cx.clear_timer(timer);
        }
        self.keepalive = Some(cx.set_timer(REFRESH_MS));
        if let Some(socket) = self.socket {
            cx.send(socket, "Ping");
        }
        // Reconnects have a blind interval; any old snapshots are now stale.
        self.bump_orders();
        self.bump_positions();
        self.reconcile(cx, credentials);
    }
    pub fn on_close(&mut self, cx: &mut Cx<'_, Pending>, _credentials: &Credentials) {
        // The parent only routes the current socket. Late callbacks cannot
        // invalidate its replacement or install a second recovery timer.
        self.socket = None;
        self.invalidate_private(cx);
        self.recovery = Some(cx.set_timer(1_000.0));
    }

    pub fn on_timer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        timer: TimerId,
        credentials: &Credentials,
    ) {
        if self.recovery == Some(timer) {
            self.recovery = None;
            self.fetch_key(cx, credentials);
        } else if self.keepalive == Some(timer) {
            self.keepalive = None;
            if let Some(key) = self.key.as_deref() {
                Self::request(
                    cx,
                    credentials,
                    Method::Put,
                    LISTEN_KEY,
                    Params::new().str("listenKey", key),
                    Request::Keepalive { epoch: self.epoch },
                    false,
                );
            } else {
                self.fresh_key(cx, credentials);
            }
        } else if self.heartbeat == Some(timer) {
            self.heartbeat = None;
            if cx.now - self.received_at >= SILENCE_MS {
                cx.log(
                    LogLevel::Warning,
                    "BingX private stream timed out; refreshing listen key",
                );
                self.fresh_key(cx, credentials);
            } else {
                if self.open
                    && let Some(socket) = self.socket
                {
                    cx.send(socket, "Ping");
                }
                self.heartbeat = Some(cx.set_timer(PING_MS));
            }
        } else if self.repair == Some(timer) {
            self.repair = None;
            self.fetch_positions(cx, credentials);
        } else if self.poll == Some(timer) {
            self.poll = Some(cx.set_poll_timer(POLL_MS));
            if !self.mode_known {
                self.fetch_mode(cx, credentials);
            }
            self.fetch_positions(cx, credentials);
            let count = self.poll_count;
            self.poll_count = (count + 1) % 4;
            if count == 2 {
                self.fetch_orders(cx, credentials);
            }
            if count == 0 {
                self.fetch_balance(cx, credentials);
            }
        }
    }

    fn schedule_repair(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.repair.is_none() {
            self.repair = Some(cx.set_timer((REPAIR_MS - (cx.now - self.last_positions)).max(1.0)));
        }
    }

    pub fn on_item(&mut self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        match request {
            Request::Balance {
                first,
                usdt,
                failed,
                ..
            } => match serde_json::from_str::<BalanceRow<'_>>(item) {
                Ok(row) => {
                    let b = Balance {
                        total: row.balance,
                        free: row.available_margin,
                        used: row.used_margin,
                        upnl: row.unrealized_profit,
                    };
                    if first.is_none() {
                        *first = Some(b.clone());
                    }
                    if row.asset == "USDT" {
                        *usdt = Some(b);
                    }
                }
                Err(_) => *failed = true,
            },
            Request::Positions {
                rows,
                settings,
                failed,
                ..
            } => {
                let Ok(row) = serde_json::from_str::<PositionRow<'_>>(item) else {
                    *failed = true;
                    return;
                };
                let symbol = row.symbol.replace('-', "");
                if cx.store.market(&symbol).is_none() {
                    return;
                }
                if row.leverage <= 0.0 {
                    *failed = true;
                    return;
                }
                let mode = mode(cx);
                let side = position_side(&row.position_side, row.position_amt, mode);
                let margin = if row.isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                // The common setting has one leverage; retain long leverage
                // there and preserve side-specific leverage on each position.
                if side == PositionSide::Long || !settings.iter().any(|(s, _)| s == &symbol) {
                    settings.retain(|(s, _)| s != &symbol);
                    settings.push((
                        symbol.clone(),
                        MarketSetting {
                            leverage: row.leverage,
                            margin_type: Some(margin),
                            ..MarketSetting::default()
                        },
                    ));
                }
                if row.position_amt == 0.0 {
                    return;
                }
                let notional = row.position_amt.abs() * row.avg_price + row.unrealized_profit;
                let margin_size = if row.isolated_margin > 0.0 {
                    row.isolated_margin
                } else if row.initial_margin > 0.0 {
                    row.initial_margin
                } else {
                    notional / row.leverage
                };
                rows.push(Position {
                    id: position_id(&symbol, side, mode),
                    symbol,
                    side,
                    entry_price: row.avg_price,
                    notional,
                    leverage: row.leverage,
                    margin_type: Some(margin),
                    position_mode: Some(mode),
                    unrealized_pnl: row.unrealized_profit,
                    realized_pnl: None,
                    contracts: row.position_amt.abs(),
                    liquidation_price: row.liquidation_price,
                    margin_size: Some(margin_size),
                    last_update: None,
                });
            }
            Request::OpenOrders {
                rows,
                metas,
                failed,
                ..
            } => {
                let Ok(native) = serde_json::from_str::<NativeOrder<'_>>(item) else {
                    *failed = true;
                    return;
                };
                let Some(order) = map_order(&native, cx) else {
                    *failed = true;
                    return;
                };
                if cx.store.market(&order.symbol).is_none()
                    || self.terminal_orders.contains_key(&order.id)
                {
                    return;
                }
                if native.complete_metadata() {
                    metas.push(observation(order.clone(), &native));
                }
                rows.push(order);
            }
            _ => {}
        }
    }

    pub fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
        credentials: &Credentials,
    ) {
        if matches!(&request,Request::ListenKey{epoch}|Request::Keepalive{epoch} if *epoch!=self.epoch)
        {
            return;
        }
        let is_key = matches!(
            request,
            Request::ListenKey { .. } | Request::Keepalive { .. }
        );
        if let Some(error) = response_error(response, is_key) {
            if matches!(request, Request::Keepalive { .. })
                && (response.status == 404 || {
                    let m = error.to_ascii_lowercase();
                    m.contains("listen")
                        && (m.contains("expired")
                            || m.contains("invalid")
                            || m.contains("does not exist"))
                })
            {
                cx.log(
                    LogLevel::Warning,
                    "BingX listen key expired; obtaining a fresh key",
                );
                self.fresh_key(cx, credentials);
            } else {
                self.on_request_failed(cx, request, &error, credentials);
            }
            return;
        }
        match request {
            Request::ListenKey { .. } => {
                self.key_in_flight = false;
                let parsed = serde_json::from_str::<ListenKey<'_>>(response.body);
                let key = parsed.as_ref().ok().and_then(|p| p.key()).filter(|k| {
                    !k.is_empty()
                        && k.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                });
                let Some(key) = key else {
                    self.on_request_failed(
                        cx,
                        Request::ListenKey { epoch: self.epoch },
                        "listen key missing or malformed",
                        credentials,
                    );
                    return;
                };
                self.key = Some(key.into());
                self.socket = Some(cx.connect_channel(
                    format!("{}?listenKey={key}", crate::paths::PRIVATE_WS),
                    safe_cex_engine::SocketChannel::Private,
                ));
                self.received_at = cx.now;
                self.heartbeat = Some(cx.set_timer(PING_MS));
            }
            Request::Keepalive { .. } => {
                if let Some(timer) = self.keepalive.take() {
                    cx.clear_timer(timer);
                }
                self.keepalive = Some(cx.set_timer(REFRESH_MS));
            }
            Request::PositionMode { revision } => {
                self.mode_in_flight = false;
                if revision != self.positions_revision {
                    self.fetch_mode(cx, credentials);
                    return;
                }
                match serde_json::from_str::<ModeEnvelope>(response.body) {
                    Ok(row) => {
                        if !self.mode_known || cx.store.hedged() != row.data.dual {
                            self.bump_positions();
                        }
                        self.mode_known = true;
                        cx.store.set_hedged(row.data.dual);
                        self.fetch_positions(cx, credentials);
                    }
                    Err(_) => cx.log(LogLevel::Error, "BingX malformed position mode response"),
                }
            }
            Request::Balance {
                revision,
                first,
                usdt,
                failed,
            } => {
                self.balance_in_flight = None;
                if failed {
                    cx.log(LogLevel::Error, "BingX malformed balance snapshot");
                    return;
                }
                if revision == self.balance_revision {
                    if let Some(balance) = usdt.or(first) {
                        cx.store.set_balance(balance);
                    } else {
                        cx.log(
                            LogLevel::Error,
                            "BingX balance snapshot contained no settlement asset",
                        );
                    }
                } else {
                    self.fetch_balance(cx, credentials);
                }
            }
            Request::Positions {
                revision,
                rows,
                settings,
                failed,
            } => {
                self.positions_in_flight = None;
                if failed {
                    cx.log(LogLevel::Error, "BingX malformed position snapshot");
                    return;
                }
                if revision == self.positions_revision {
                    cx.store.set_positions(rows);
                    for (symbol, value) in settings {
                        cx.store.update_setting(&symbol, |s| *s = value);
                    }
                } else {
                    self.schedule_repair(cx);
                }
            }
            Request::OpenOrders {
                revision,
                rows,
                metas,
                failed,
            } => {
                self.orders_in_flight = None;
                if revision != self.orders_revision {
                    self.fetch_orders(cx, credentials);
                    return;
                }
                if failed {
                    cx.log(
                        LogLevel::Error,
                        "BingX invalid open order; retaining previous snapshot",
                    );
                    return;
                }
                for order in &rows {
                    if order.filled > self.cumulative_fills.get(&order.id).copied().unwrap_or(0.0) {
                        self.covered_history.insert(order.id.clone());
                    }
                    self.record_cumulative(&order.id, order.filled);
                }
                self.observations.extend(metas);
                cx.store.set_orders(rows);
            }
        }
    }

    pub fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
        _credentials: &Credentials,
    ) {
        if matches!(&request,Request::ListenKey{epoch}|Request::Keepalive{epoch} if *epoch!=self.epoch)
        {
            return;
        }
        cx.log(
            LogLevel::Warning,
            format!("BingX private request failed: {error}"),
        );
        match request {
            Request::ListenKey { .. } => {
                self.key_in_flight = false;
                if let Some(timer) = self.recovery.take() {
                    cx.clear_timer(timer);
                }
                self.recovery = Some(cx.set_timer(RETRY_MS));
            }
            Request::Keepalive { .. } => {
                if let Some(timer) = self.keepalive.take() {
                    cx.clear_timer(timer);
                }
                self.keepalive = Some(cx.set_timer(RETRY_MS));
            }
            Request::PositionMode { .. } => self.mode_in_flight = false,
            Request::Balance { .. } => self.balance_in_flight = None,
            Request::Positions { .. } => self.positions_in_flight = None,
            Request::OpenOrders { .. } => self.orders_in_flight = None,
        }
    }

    pub fn on_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str, credentials: &Credentials) {
        self.received_at = cx.now;
        if text == "Ping" {
            if let Some(socket) = self.socket {
                cx.send(socket, "Pong");
            }
            return;
        }
        if text == "Pong" {
            return;
        }
        let Ok(event) = serde_json::from_str::<UserEvent<'_>>(text) else {
            return;
        };
        if event.ping.is_some() {
            if let Some(socket) = self.socket {
                cx.send(socket, "Pong");
            }
            return;
        }
        if event.pong.is_some() {
            return;
        }
        match event.e.as_ref() {
            "listenKeyExpired" => self.fresh_key(cx, credentials),
            "ACCOUNT_UPDATE" => {
                if !event.a.balances.is_empty() {
                    self.balance_revision = self.balance_revision.wrapping_add(1);
                    self.fetch_balance(cx, credentials);
                }
                let candidates = [
                    &event.a.positions,
                    &event.a.balances,
                    &event.a.legacy_positions,
                ];
                if let Some(rows) = candidates
                    .into_iter()
                    .find(|rows| rows.iter().any(|p| !p.s.is_empty() && p.pa.is_some()))
                {
                    self.bump_positions();
                    for p in rows {
                        self.on_position(cx, p);
                    }
                    self.schedule_repair(cx);
                }
            }
            "ACCOUNT_CONFIG_UPDATE" => {
                if let Some(config) = event.ac {
                    self.bump_positions();
                    let symbol = config.s.replace('-', "");
                    if config.l > 0.0 {
                        cx.store.update_setting(&symbol, |s| {
                            s.leverage = config.l;
                            if let Some(m) = &config.mt {
                                s.margin_type = Some(margin_type(m));
                            }
                        });
                        cx.store.update_positions(|p| {
                            if p.symbol == symbol {
                                p.leverage = if p.side == PositionSide::Short && config.short > 0.0
                                {
                                    config.short
                                } else {
                                    config.l
                                };
                                if let Some(m) = &config.mt {
                                    p.margin_type = Some(margin_type(m));
                                }
                            }
                        });
                    }
                    self.schedule_repair(cx);
                }
            }
            "ORDER_TRADE_UPDATE" | "TRADE_UPDATE" => {
                if let Some(native) = event.o {
                    self.on_order(cx, native, event.time);
                }
            }
            _ => {}
        }
    }

    fn record_cumulative(&mut self, id: &str, quantity: f64) {
        if !self.cumulative_fills.contains_key(id) {
            self.retained_orders.push_back(id.to_owned());
            while self.retained_orders.len() > RETAINED_ORDERS {
                if let Some(old) = self.retained_orders.pop_front() {
                    self.cumulative_fills.remove(&old);
                    if !self.terminal_orders.contains_key(&old) {
                        self.covered_history.remove(&old);
                    }
                }
            }
        }
        let previous = self.cumulative_fills.entry(id.to_owned()).or_insert(0.0);
        *previous = previous.max(quantity);
    }

    fn record_terminal(&mut self, id: &str) {
        // Terminal tombstones have their own budget: a large open-order
        // snapshot must not evict all recent terminal replay protection.
        if !self.terminal_orders.contains_key(id) {
            self.retained_terminals.push_back(id.to_owned());
            while self.retained_terminals.len() > RETAINED_ORDERS {
                if let Some(old) = self.retained_terminals.pop_front() {
                    self.terminal_orders.remove(&old);
                    if !self.cumulative_fills.contains_key(&old) {
                        self.covered_history.remove(&old);
                    }
                }
            }
        }
        let quantity = self.cumulative_fills.get(id).copied().unwrap_or(0.0);
        let previous = self.terminal_orders.entry(id.to_owned()).or_insert(0.0);
        *previous = previous.max(quantity);
    }

    fn first_execution(&mut self, identity: String) -> bool {
        if !self.seen_executions.insert(identity.clone()) {
            return false;
        }
        self.retained_executions.push_back(identity);
        while self.retained_executions.len() > RETAINED_EXECUTIONS {
            if let Some(old) = self.retained_executions.pop_front() {
                self.seen_executions.remove(&old);
            }
        }
        true
    }

    pub fn accept_placed_response(&mut self, cx: &mut Cx<'_, Pending>, body: &str) {
        if !serde_json::from_str::<Status<'_>>(body).is_ok_and(|s| s.code == 0) {
            return;
        }
        if let Ok(placed) = serde_json::from_str::<PlacedEnvelope<'_>>(body) {
            let native = match placed.data {
                Placed::Nested { order } | Placed::Direct(order) => order,
            };
            self.accept_placed(cx, &native);
        }
    }

    pub fn accept_placed(&mut self, cx: &mut Cx<'_, Pending>, native: &NativeOrder<'_>) {
        if let Some(order) = map_order(native, cx)
            && order.status == OrderStatus::Open
            && order.kind != OrderType::Market
            && cx.store.order(&order.id).is_none()
            && !self.terminal_orders.contains_key(&order.id)
        {
            // A stream row is authoritative even when its terminal row has
            // since disappeared from the latest open-order snapshot.
            if native.complete_metadata() {
                self.observations.push(observation(order.clone(), native));
            }
            cx.store.upsert_order(order);
        }
    }

    fn on_order(&mut self, cx: &mut Cx<'_, Pending>, native: NativeOrder<'_>, event_time: f64) {
        // LIQUIDATION and future execution types need not map to an ordinary
        // resting OrderType. Process their executions before order conversion.
        let Some(id) = native.id() else {
            return;
        };
        if !matches!(native.side.as_ref(), "BUY" | "SELL") || native.symbol.is_empty() {
            return;
        }
        let symbol = native.symbol.replace('-', "");
        self.bump_orders();
        let filled = native.filled.unwrap_or(0.0);
        let previous = self
            .cumulative_fills
            .get(id)
            .copied()
            .unwrap_or(0.0)
            .max(self.terminal_orders.get(id).copied().unwrap_or(0.0));
        let mut effective_filled = filled.max(previous);
        let trade_status = matches!(
            native.status.as_deref(),
            Some("PARTIALLY_FILLED" | "FILLED")
        );
        let mut emitted = 0.0;
        if trade_status {
            // Both event names can carry one execution with or without z.
            let fresh = if native.last_quantity > 0.0 {
                let identity = if !native.trade_id.is_empty() {
                    format!("{id}:trade:{}", native.trade_id)
                } else {
                    format!(
                        "{id}:time:{}:{}:{}",
                        if native.time > 0.0 {
                            native.time
                        } else {
                            event_time
                        },
                        native.last_quantity,
                        native.last_price
                    )
                };
                self.first_execution(identity)
            } else {
                true
            };
            // Cumulative growth proves a new fill when heuristic same-ms keys collide.
            let duplicate = !fresh && (!native.trade_id.is_empty() || native.filled.is_none());
            let delta = if duplicate {
                0.0
            } else if native.filled.is_some() {
                let increase = (filled - previous).max(0.0);
                // Only l belongs at this execution's price after a blind interval.
                if native.last_quantity > 0.0 {
                    increase.min(native.last_quantity)
                } else {
                    increase
                }
            } else if self.terminal_orders.contains_key(id)
                && (native.trade_id.is_empty() || self.covered_history.contains(id))
            {
                0.0
            } else {
                native.last_quantity.max(0.0)
            };
            emitted = delta;
            if native.filled.is_none() {
                effective_filled = previous + delta;
            }
            if delta > 0.0 {
                let price = if native.last_price > 0.0 {
                    native.last_price
                } else if native.avg_price > 0.0 {
                    native.avg_price
                } else {
                    native.price
                };
                cx.fill(Fill {
                    order_id: id.to_owned(),
                    symbol: symbol.clone(),
                    side: order_side(&native.side),
                    price,
                    amount: delta,
                });
                self.bump_positions();
                self.schedule_repair(cx);
            }
            self.record_cumulative(id, effective_filled);
        }
        if matches!(
            native.status.as_deref(),
            Some("FILLED" | "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED")
        ) {
            self.record_cumulative(id, effective_filled);
            self.record_terminal(id);
        }
        if native.filled.is_some()
            && filled > previous + emitted
            && self.cumulative_fills.contains_key(id)
        {
            self.covered_history.insert(id.to_owned());
        }
        let Some(mut order) = map_order(&native, cx) else {
            return;
        };
        order.filled = effective_filled.max(cx.store.order(id).map_or(0.0, |order| order.filled));
        if native.status.as_deref() == Some("FILLED") && native.filled.is_none() {
            order.filled = order.filled.max(order.amount);
        }
        order.remaining = (order.amount - order.filled).max(0.0);
        if native.complete_metadata() {
            self.observations.push(observation(order.clone(), &native));
        }
        if order.status == OrderStatus::Open && order.kind != OrderType::Market {
            let existing = cx.store.order(&order.id);
            if !self.terminal_orders.contains_key(&order.id)
                && !existing
                    .is_some_and(|o| o.status != OrderStatus::Open || o.filled > order.filled)
            {
                cx.store.upsert_order(order);
            }
        } else if order.status != OrderStatus::Open {
            cx.store.update_order(&order.id, |o| {
                o.status = order.status;
                o.filled = o.filled.max(order.filled);
                o.remaining = (o.amount - o.filled).max(0.0);
                o.last_update = cx.now;
            });
        }
    }

    fn on_position(&mut self, cx: &mut Cx<'_, Pending>, p: &PositionUpdate<'_>) {
        let Some(quantity) = p.pa else {
            return;
        };
        if p.s.is_empty() {
            return;
        }
        let symbol = p.s.replace('-', "");
        if cx.store.market(&symbol).is_none() {
            return;
        }
        let current_mode = if self.mode_known {
            mode(cx)
        } else if p.ps == "BOTH" {
            PositionMode::Oneway
        } else {
            PositionMode::Hedged
        };
        let side = position_side(&p.ps, quantity, current_mode);
        let expected = position_id(&symbol, side, current_mode);
        let existing = cx
            .store
            .position(&expected)
            .or_else(|| {
                cx.store.positions().iter().find(|e| {
                    e.symbol == symbol && (current_mode == PositionMode::Oneway || e.side == side)
                })
            })
            .cloned();
        if quantity == 0.0 && existing.is_none() {
            return;
        }
        let setting = cx.store.setting(&symbol);
        let entry =
            p.ep.or_else(|| existing.as_ref().map(|p| p.entry_price))
                .unwrap_or(0.0);
        let upnl =
            p.up.or_else(|| existing.as_ref().map(|p| p.unrealized_pnl))
                .unwrap_or(0.0);
        let leverage = existing
            .as_ref()
            .map(|p| p.leverage)
            .or(setting.map(|s| s.leverage))
            .filter(|l| l.is_finite() && *l > 0.0)
            .unwrap_or(1.0);
        let notional = quantity.abs() * entry + upnl;
        cx.store.upsert_position(Position {
            id: existing.as_ref().map(|p| p.id.clone()).unwrap_or(expected),
            symbol,
            side: if quantity == 0.0 {
                existing.as_ref().map_or(side, |p| p.side)
            } else {
                side
            },
            entry_price: entry,
            notional,
            leverage,
            margin_type: p
                .mt
                .as_deref()
                .map(margin_type)
                .or_else(|| existing.as_ref().and_then(|p| p.margin_type))
                .or(Some(MarginType::Cross)),
            position_mode: Some(current_mode),
            unrealized_pnl: upnl,
            realized_pnl: existing.as_ref().and_then(|p| p.realized_pnl),
            contracts: quantity.abs(),
            liquidation_price: p
                .liqp
                .or_else(|| existing.as_ref().map(|p| p.liquidation_price))
                .unwrap_or(0.0),
            margin_size: existing
                .as_ref()
                .and_then(|p| p.margin_size)
                .or(Some(notional / leverage)),
            last_update: Some(cx.now),
        });
    }
}

fn observation(order: Order, native: &NativeOrder<'_>) -> OrderMeta {
    (
        order,
        native.kind.to_string(),
        native.position_side.to_string(),
        native
            .take_profit
            .as_ref()
            .is_some_and(Protection::meaningful)
            || native
                .stop_loss
                .as_ref()
                .is_some_and(Protection::meaningful),
        native.status.as_deref().unwrap_or_default().to_owned(),
    )
}

pub(crate) fn map_order(native: &NativeOrder<'_>, cx: &Cx<'_, Pending>) -> Option<Order> {
    if !matches!(native.side.as_ref(), "BUY" | "SELL") || native.symbol.is_empty() {
        return None;
    }
    let id = native.id()?;
    let quantity = native.quantity.or(native.close_position.then_some(0.0))?;
    let mut kind = order_type(&native.kind)?;
    let symbol = native.symbol.replace('-', "");
    let side = order_side(&native.side);
    let status = match native.status.as_deref().unwrap_or("NEW") {
        "NEW" | "OPEN" | "PENDING" | "WORKING" | "PARTIALLY_FILLED" => OrderStatus::Open,
        "FILLED" => OrderStatus::Closed,
        "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
        _ => return None,
    };
    let price = if native.price > 0.0 {
        native.price
    } else {
        native.stop_price
    };
    if kind == OrderType::StopLoss
        && let Some(ticker) = cx.store.ticker(&symbol)
        && ticker.mark > 0.0
        && ((side == OrderSide::Buy && price < ticker.mark)
            || (side == OrderSide::Sell && price > ticker.mark))
    {
        kind = OrderType::TakeProfit;
    }
    let filled = native.filled.unwrap_or(0.0);
    if quantity < 0.0 || filled < 0.0 {
        return None;
    }
    let existing = cx.store.order(id);
    let tif = if native.post_only {
        Some(TimeInForce::PostOnly)
    } else {
        native
            .time_in_force
            .as_deref()
            .and_then(time_in_force)
            .or_else(|| existing.and_then(|o| o.time_in_force))
    };
    let trigger_type = if matches!(kind, OrderType::StopLoss | OrderType::TakeProfit) {
        match native.working_type.as_deref() {
            Some("MARK_PRICE") => Some(StopTriggerType::MarkPrice),
            Some("CONTRACT_PRICE") => Some(StopTriggerType::LastPrice),
            _ => None,
        }
    } else {
        None
    };
    Some(Order {
        id: id.to_owned(),
        parent_id: None,
        status,
        symbol,
        kind,
        side,
        price,
        amount: quantity,
        filled,
        remaining: (quantity - filled).max(0.0),
        reduce_only: native.reduce_only,
        close: Some(native.close_position),
        client_order_id: native.client_order_id.as_deref().map(str::to_owned),
        time_in_force: tif,
        last_update: cx.now,
        trigger_type,
        take_profit: None,
        stop_loss: None,
        extra: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::{Action, Command, Engine, Event, Venue, book::Book, wire};

    #[derive(Default)]
    struct PlacementVenue {
        account: Account,
    }
    impl Venue for PlacementVenue {
        type Pending = Pending;
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            cx.request(
                Method::Post,
                "fixture".into(),
                Vec::new(),
                None,
                Pending::Account(Request::Keepalive { epoch: 0 }),
            );
        }
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
        fn on_message(&mut self, cx: &mut Cx<'_, Pending>, _: SocketId, text: &str) {
            self.account.on_message(
                cx,
                text,
                &Credentials {
                    read: None,
                    key: "fixture".into(),
                    secret: "fixture".into(),
                },
            );
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Pending>, _: Pending, response: Response<'_>) {
            self.account.accept_placed_response(cx, response.body);
        }
        fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
        fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: Command) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    #[test]
    fn placement_acknowledgements_do_not_suppress_first_stream_execution() {
        for (kind, status, cumulative) in [
            ("MARKET", "FILLED", true),
            ("MARKET", "FILLED", false),
            ("LIMIT", "PARTIALLY_FILLED", true),
            ("LIMIT", "PARTIALLY_FILLED", false),
        ] {
            let mut engine = Engine::new(PlacementVenue::default());
            engine.start(0.0);
            let request = engine
                .drain_actions()
                .find_map(|a| {
                    if let Action::Http(r) = a {
                        Some(r.id)
                    } else {
                        None
                    }
                })
                .unwrap();
            let ack = format!(
                r#"{{"code":0,"data":{{"order":{{"orderId":123,"type":"{kind}","symbol":"BTC-USDT","side":"BUY","status":"{status}","quantity":"0.2","executedQty":"0.1","price":"60000"}}}}}}"#
            );
            engine.handle(
                1.0,
                Event::HttpResponse {
                    request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: &ack,
                    },
                },
            );
            assert!(!engine.venue().account.cumulative_fills.contains_key("123"));
            let cumulative_field = if cumulative { r#""z":"0.1","# } else { "" };
            let event = format!(
                r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"i":123,"s":"BTC-USDT","o":"{kind}","S":"BUY","X":"{status}","q":"0.2",{cumulative_field}"l":"0.1","L":"60000"}}}}"#
            );
            engine.handle(
                2.0,
                Event::WsMessage {
                    socket: SocketId(0),
                    text: &event,
                },
            );
            let frame = engine.take_market_frame(2.0).unwrap();
            let (_, fills) = wire::sections(frame)
                .find(|(kind, _)| *kind == wire::section::FILLS)
                .expect("WS execution must emit fill after placement ack");
            assert_eq!(f64::from_le_bytes(fills[24..32].try_into().unwrap()), 0.1);
            engine.handle(
                3.0,
                Event::WsMessage {
                    socket: SocketId(0),
                    text: &event,
                },
            );
            assert!(engine.take_market_frame(3.0).is_none());
        }
    }

    #[test]
    fn late_placement_ack_preserves_private_partial_and_terminal_tombstone() {
        for terminal in [false, true] {
            let mut engine = Engine::new(PlacementVenue::default());
            engine.start(0.0);
            let request = engine
                .drain_actions()
                .find_map(|a| {
                    if let Action::Http(r) = a {
                        Some(r.id)
                    } else {
                        None
                    }
                })
                .unwrap();
            engine.handle(1.0,Event::WsMessage{socket:SocketId(0),text:r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":123,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"NEW","q":"0.2","z":"0","p":"60000"}}"#});
            let status = if terminal {
                "FILLED"
            } else {
                "PARTIALLY_FILLED"
            };
            let quantity = if terminal { 0.2 } else { 0.1 };
            let event = format!(
                r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"i":123,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"{status}","q":"0.2","z":"{quantity}","l":"{quantity}","L":"60000"}}}}"#
            );
            engine.handle(
                2.0,
                Event::WsMessage {
                    socket: SocketId(0),
                    text: &event,
                },
            );
            if terminal {
                engine.store_mut().set_orders(Vec::new());
            }
            let body = r#"{"code":0,"data":{"order":{"orderId":123,"type":"LIMIT","symbol":"BTC-USDT","side":"BUY","status":"NEW","quantity":"0.2","executedQty":"0","price":"60000"}}}"#;
            engine.handle(
                3.0,
                Event::HttpResponse {
                    request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
            if terminal {
                assert!(engine.store().orders().is_empty());
            } else {
                assert_eq!(engine.store().order("123").unwrap().filled, 0.1);
            }
            assert!(engine.venue().account.observations.is_empty());
        }
    }

    #[test]
    fn execution_retention_evicts_individually_and_keeps_recent_terminals() {
        let mut account = Account::default();
        for i in 0..RETAINED_ORDERS + 100 {
            let id = i.to_string();
            account.record_cumulative(&id, 1.0);
            account.record_terminal(&id);
            account.covered_history.insert(id);
        }
        assert_eq!(account.cumulative_fills.len(), RETAINED_ORDERS);
        assert_eq!(account.terminal_orders.len(), RETAINED_ORDERS);
        assert!(!account.cumulative_fills.contains_key("0"));
        assert_eq!(account.covered_history.len(), RETAINED_ORDERS);
        assert!(!account.covered_history.contains("0"));
        assert!(
            account
                .terminal_orders
                .contains_key(&(RETAINED_ORDERS + 99).to_string())
        );
        for i in 0..RETAINED_ORDERS + 100 {
            account.record_cumulative(&format!("active-{i}"), 0.0);
        }
        assert_eq!(account.covered_history.len(), RETAINED_ORDERS);
        assert!(
            account
                .terminal_orders
                .contains_key(&(RETAINED_ORDERS + 99).to_string())
        );
        for i in 0..RETAINED_EXECUTIONS + 100 {
            assert!(account.first_execution(i.to_string()));
        }
        assert_eq!(account.seen_executions.len(), RETAINED_EXECUTIONS);
        assert!(!account.first_execution((RETAINED_EXECUTIONS + 99).to_string()));
        assert!(account.first_execution("0".into()));
        assert_eq!(account.seen_executions.len(), RETAINED_EXECUTIONS);
    }
}
