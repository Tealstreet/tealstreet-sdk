//! Futures account snapshots and the listen-key stream.
use crate::wire::{Number, Text};
use crate::{Bitrue, Job, Pending};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{
    Balance, Order, OrderExtra, OrderSide, OrderStatus, OrderType, Position, PositionMode,
    PositionSide, StopTriggerType, TimeInForce,
};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{
    Command, CommandId, Cx, Fill, LogLevel, Method, Response, SocketId, TimerId,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};

const LISTEN_URL: &str = "https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey";
const PRIVATE_URL: &str = "wss://fapiws.bitrue.com/stream";
const REFRESH_MS: f64 = 30.0 * 60_000.0;
const HEALTH_MS: f64 = 20_000.0;
const HISTORY_LIMIT: usize = 500;

pub enum Request {
    Listen {
        generation: u64,
    },
    Keepalive {
        generation: u64,
    },
    Snapshot {
        revision: u64,
        data: Snapshot,
    },
    Orders {
        symbol: String,
        revision: u64,
        rows: Vec<Order>,
        invalid: bool,
    },
}
impl Request {
    pub(crate) fn reset(&mut self) {
        match self {
            Self::Snapshot { data, .. } => *data = Snapshot::default(),
            Self::Orders { rows, invalid, .. } => {
                rows.clear();
                *invalid = false;
            }
            _ => {}
        }
    }
}
#[derive(Default)]
pub struct Snapshot {
    balances: HashMap<String, Balance>,
    positions: Vec<Position>,
    settings: Vec<(String, MarketSetting)>,
    invalid: bool,
}
#[derive(Default)]
pub struct Account {
    started: bool,
    pub(crate) loaded: bool,
    pub(crate) settings_revision: u64,
    pub(crate) orders_revision: u64,
    pub(crate) socket: Option<SocketId>,
    generation: u64,
    listen_key: Option<String>,
    listen_pending: bool,
    listen_attempt: usize,
    reconnect: Option<TimerId>,
    keepalive: Option<TimerId>,
    health: Option<TimerId>,
    tick: Option<TimerId>,
    fast: Option<TimerId>,
    snapshot_pending: bool,
    snapshot_again: bool,
    order_queue: VecDeque<String>,
    order_active: Option<String>,
    order_timer: Option<TimerId>,
    initial_orders: HashSet<String>,
    progress: HashMap<String, f64>,
    quotes: HashMap<String, f64>,
    leverage_queries: Vec<(CommandId, String)>,
    progress_queue: VecDeque<String>,
    terminal: HashSet<String>,
    terminal_queue: VecDeque<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listen<'a> {
    #[serde(borrow, default)]
    code: Text<'a>,
    #[serde(borrow, default)]
    status: Cow<'a, str>,
    #[serde(borrow, default)]
    msg: Cow<'a, str>,
    #[serde(borrow, default)]
    listen_key: Cow<'a, str>,
    #[serde(borrow, default)]
    data: Option<ListenData<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListenData<'a> {
    #[serde(borrow, default)]
    listen_key: Cow<'a, str>,
}
impl Listen<'_> {
    fn success(&self) -> bool {
        matches!(&*self.code, "0" | "200") || self.status == "ok" || self.msg == "succ"
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Coin<'a> {
    #[serde(borrow)]
    margin_coin: Cow<'a, str>,
    total_equity: Number,
    account_normal: Number,
    #[serde(default)]
    account_lock: Number,
    #[serde(default)]
    total_position_normal: Number,
    #[serde(default)]
    total_cost: Number,
    #[serde(default)]
    sum_maintenance_margin: Option<Number>,
    #[serde(default)]
    sum_open_realized_amount: Number,
    #[serde(borrow)]
    position_vos: Option<Vec<ContractPositions<'a>>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContractPositions<'a> {
    #[serde(borrow)]
    contract_name: Cow<'a, str>,
    #[serde(borrow)]
    positions: Vec<PositionRow<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionRow<'a> {
    #[serde(borrow)]
    side: Cow<'a, str>,
    volume: Number,
    open_price: Number,
    leverage_level: Number,
    position_type: Number,
    #[serde(default)]
    position_balance: Number,
    #[serde(default)]
    force_liquidation_price: Option<Number>,
    #[serde(default)]
    reduce_price: Option<Number>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderRow<'a> {
    #[serde(borrow)]
    order_id: Text<'a>,
    #[serde(borrow, default)]
    client_order_id: Text<'a>,
    #[serde(borrow, default)]
    contract_name: Cow<'a, str>,
    #[serde(borrow, default, rename = "type")]
    kind: Cow<'a, str>,
    #[serde(borrow)]
    side: Cow<'a, str>,
    #[serde(borrow)]
    status: Cow<'a, str>,
    #[serde(borrow, default)]
    action: Cow<'a, str>,
    #[serde(default)]
    price: Number,
    #[serde(default)]
    orig_qty: Option<Number>,
    executed_qty: Number,
    #[serde(default)]
    trigger_price: Number,
    #[serde(default)]
    trigger_type: Number,
    #[serde(default)]
    trigger_price_type: Number,
    #[serde(default)]
    transact_time: Number,
    #[serde(default)]
    condition_order: bool,
    #[serde(borrow, default)]
    child_orders: Vec<OrderRow<'a>>,
}
#[derive(Deserialize)]
struct PrivateMessage<'a> {
    #[serde(borrow, default)]
    e: Cow<'a, str>,
    #[serde(borrow, default)]
    event: Cow<'a, str>,
    #[serde(rename = "E", default)]
    time: Number,
    #[serde(borrow, default)]
    ts: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    ping: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    o: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    status: Cow<'a, str>,
    #[serde(borrow, default)]
    code: Text<'a>,
}
#[derive(Deserialize)]
struct OrderEvent<'a> {
    #[serde(borrow, default)]
    s: Cow<'a, str>,
    #[serde(borrow, default)]
    i: Text<'a>,
    #[serde(borrow, default, rename = "orderId")]
    order_id: Text<'a>,
    #[serde(borrow, default)]
    c: Cow<'a, str>,
    #[serde(borrow, default, rename = "clientOrderId")]
    client_order_id: Cow<'a, str>,
    #[serde(borrow, default, rename = "S")]
    side: Text<'a>,
    #[serde(borrow, default)]
    o: Text<'a>,
    #[serde(borrow, default)]
    x: Cow<'a, str>,
    #[serde(borrow, default, rename = "X")]
    status: Cow<'a, str>,
    #[serde(default, alias = "origQty")]
    q: Number,
    #[serde(default)]
    volume: Number,
    #[serde(default, alias = "executedQty")]
    z: Number,
    #[serde(default)]
    p: Number,
    #[serde(default)]
    ap: Number,
    #[serde(default, rename = "L")]
    execution_price: Number,
    #[serde(default, rename = "R")]
    reduce: Option<bool>,
    #[serde(borrow, default)]
    open: Cow<'a, str>,
    #[serde(rename = "E", default)]
    time: Number,
    #[serde(default, rename = "triggerPrice")]
    trigger_price: Number,
    #[serde(default, rename = "triggerPriceType")]
    trigger_price_type: Number,
}

pub(crate) fn order_status(value: &str) -> Option<OrderStatus> {
    Some(match value {
        "INIT" | "NEW" | "PENDING_CREATE" | "PART_FILLED" | "PARTIALLY_FILLED" => OrderStatus::Open,
        "PENDING_CANCEL" => OrderStatus::Canceling,
        "FILLED" => OrderStatus::Closed,
        "CANCELED" | "CANCELLED" | "REJECTED" | "EXPIRED" => OrderStatus::Canceled,
        _ => return None,
    })
}
pub(crate) fn order_type(value: &str) -> Option<OrderType> {
    Some(match value {
        "1" | "LIMIT" | "IOC" | "FOK" | "POST_ONLY" => OrderType::Limit,
        "2" | "MARKET" => OrderType::Market,
        "3" | "STOP_MARKET" => OrderType::StopLoss,
        "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        _ => return None,
    })
}
fn decimal(value: f64) -> f64 {
    format!("{value:.14e}").parse().unwrap_or(value)
}
fn terminal(status: OrderStatus) -> bool {
    matches!(status, OrderStatus::Closed | OrderStatus::Canceled)
}
fn side(value: &str) -> Option<OrderSide> {
    match value {
        "BUY" | "1" => Some(OrderSide::Buy),
        "SELL" | "2" => Some(OrderSide::Sell),
        _ => None,
    }
}
fn trigger(value: f64) -> Option<StopTriggerType> {
    match value {
        1.0 => Some(StopTriggerType::LastPrice),
        2.0 => Some(StopTriggerType::MarkPrice),
        _ => None,
    }
}
impl Account {
    fn progress(&mut self, id: &str, amount: f64) {
        if !self.progress.contains_key(id) {
            self.progress_queue.push_back(id.to_owned());
        }
        self.progress
            .entry(id.to_owned())
            .and_modify(|v| *v = v.max(amount))
            .or_insert(amount);
        while self.progress_queue.len() > HISTORY_LIMIT {
            if let Some(old) = self.progress_queue.pop_front() {
                self.progress.remove(&old);
                self.quotes.remove(&old);
            }
        }
    }
    pub(crate) fn remember_terminal(&mut self, id: &str) {
        if self.terminal.insert(id.to_owned()) {
            self.terminal_queue.push_back(id.to_owned());
        }
        while self.terminal_queue.len() > HISTORY_LIMIT {
            if let Some(old) = self.terminal_queue.pop_front() {
                self.terminal.remove(&old);
            }
        }
    }
}
impl Bitrue {
    pub(crate) fn remember_terminal(&mut self, id: &str) {
        self.account.remember_terminal(id);
    }
    /// Bitrue's REST placement/detail quantities are base units.
    pub(crate) fn filled_native(&self, id: &str) -> f64 {
        self.account.progress.get(id).copied().unwrap_or(0.0)
    }
    pub(crate) fn start_private(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started
            || self.options.public_data_only
            || self.options.key.as_ref().is_none_or(String::is_empty)
            || self.options.secret.as_ref().is_none_or(String::is_empty)
        {
            return;
        }
        self.account.started = true;
        cx.store.set_hedged(false);
        self.acquire_listen_key(cx);
        self.refresh_account(cx);
        self.account.initial_orders = cx
            .store
            .markets()
            .iter()
            .map(|m| m.symbol.clone())
            .collect();
        self.refresh_orders(cx, vec![]);
        self.account.tick = Some(cx.set_timer(2_000.0));
    }
    fn acquire_listen_key(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.listen_pending {
            return;
        }
        self.account.listen_pending = true;
        self.send(
            cx,
            Method::Post,
            LISTEN_URL,
            None,
            None,
            Job::Account(Request::Listen {
                generation: self.account.generation,
            }),
        );
    }
    fn restart_private(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(socket) = self.account.socket.take() {
            cx.close(socket);
        }
        for timer in [
            self.account.health.take(),
            self.account.keepalive.take(),
            self.account.reconnect.take(),
        ]
        .into_iter()
        .flatten()
        {
            cx.clear_timer(timer);
        }
        self.account.generation += 1;
        self.account.listen_pending = false;
        self.account.listen_key = None;
        self.account.reconnect = Some(cx.set_timer(1_000.0));
    }
    pub(crate) fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.snapshot_pending {
            self.account.snapshot_again = true;
            return;
        }
        self.account.snapshot_pending = true;
        self.send(
            cx,
            Method::Get,
            "/fapi/v2/account",
            None,
            Some(ArrayAt::Path(&["data", "account"])),
            Job::Account(Request::Snapshot {
                revision: self.account.settings_revision,
                data: Snapshot::default(),
            }),
        );
    }
    pub(crate) fn refresh_orders(&mut self, cx: &mut Cx<'_, Pending>, symbols: Vec<String>) {
        let symbols = if symbols.is_empty() {
            cx.store
                .markets()
                .iter()
                .map(|m| m.symbol.clone())
                .collect()
        } else {
            symbols
        };
        for symbol in symbols {
            if cx.store.market(&symbol).is_some()
                && !self.account.order_queue.contains(&symbol)
                && self.account.order_active.as_ref() != Some(&symbol)
            {
                self.account.order_queue.push_back(symbol);
            }
        }
        if self.account.order_active.is_none() && self.account.order_timer.is_none() {
            self.next_orders(cx);
        }
    }
    fn next_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(symbol) = self.account.order_queue.pop_front() else {
            return;
        };
        self.account.order_active = Some(symbol.clone());
        self.send(
            cx,
            Method::Get,
            &format!("/fapi/v2/openOrders?contractName={symbol}"),
            None,
            Some(ArrayAt::Field("data")),
            Job::Account(Request::Orders {
                symbol,
                revision: self.account.orders_revision,
                rows: vec![],
                invalid: false,
            }),
        );
    }
    fn order_finished(&mut self, cx: &mut Cx<'_, Pending>, symbol: String, success: bool) {
        self.account.order_active = None;
        if success {
            self.account.initial_orders.remove(&symbol);
        } else if !self.account.order_queue.contains(&symbol) {
            self.account.order_queue.push_back(symbol);
        }
        if self.account.initial_orders.is_empty() {
            cx.store.set_orders(cx.store.orders().to_vec());
        }
        if !self.account.order_queue.is_empty() {
            self.account.order_timer = Some(cx.set_timer(if success { 200.0 } else { 500.0 }));
        }
    }
    fn fast_refresh(&mut self, cx: &mut Cx<'_, Pending>, symbol: Option<String>) {
        self.account.settings_revision += 1;
        self.account.snapshot_again = true;
        if let Some(symbol) = symbol {
            self.refresh_orders(cx, vec![symbol]);
        }
        if let Some(timer) = self.account.fast.take() {
            cx.clear_timer(timer);
        }
        self.account.fast = Some(cx.set_timer(350.0));
    }
    pub(crate) fn account_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        for channel in [
            "user_account_update",
            "user_order_update",
            "user_balance_update",
        ] {
            cx.send(
                socket,
                format!("{{\"event\":\"sub\",\"params\":{{\"channel\":\"{channel}\"}}}}"),
            );
        }
        self.account.health = Some(cx.set_timer(HEALTH_MS));
        self.refresh_account(cx);
        self.refresh_orders(cx, vec![]);
        true
    }
    pub(crate) fn account_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.restart_private(cx);
        true
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.health == Some(timer) {
            self.account.health = None;
            self.restart_private(cx);
        } else if self.account.reconnect == Some(timer) {
            self.account.reconnect = None;
            self.acquire_listen_key(cx);
        } else if self.account.keepalive == Some(timer) {
            self.account.keepalive = None;
            if let Some(key) = &self.account.listen_key {
                self.send(
                    cx,
                    Method::Put,
                    &format!("{LISTEN_URL}/{key}"),
                    None,
                    None,
                    Job::Account(Request::Keepalive {
                        generation: self.account.generation,
                    }),
                );
            }
        } else if self.account.tick == Some(timer) {
            self.account.tick = Some(cx.set_timer(2_000.0));
            self.refresh_account(cx);
        } else if self.account.fast == Some(timer) {
            self.account.fast = None;
            self.account.snapshot_again = false;
            self.refresh_account(cx);
        } else if self.account.order_timer == Some(timer) {
            self.account.order_timer = None;
            self.next_orders(cx);
        } else {
            return false;
        }
        true
    }
    pub(crate) fn account_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        let Ok(message) = serde_json::from_str::<PrivateMessage<'_>>(text) else {
            return true;
        };
        if let Some(timer) = self.account.health.take() {
            cx.clear_timer(timer);
        }
        self.account.health = Some(cx.set_timer(HEALTH_MS));
        let event = if message.e.is_empty() {
            &*message.event
        } else {
            &*message.e
        };
        if let Some(ping) = message.ping {
            cx.send(socket, format!("{{\"pong\":{}}}", ping.get()));
        } else if event == "ping" {
            cx.send(
                socket,
                message.ts.map_or_else(
                    || "{\"event\":\"pong\"}".to_owned(),
                    |ts| format!("{{\"event\":\"pong\",\"ts\":{}}}", ts.get()),
                ),
            );
        } else if matches!(event, "BALANCE" | "ACCOUNT_UPDATE") {
            self.fast_refresh(cx, None);
        } else if matches!(event, "ORDER" | "ORDER_TRADE_UPDATE" | "CANCEL") {
            let raw = message.o.map_or(text, |o| o.get());
            if let Ok(row) = serde_json::from_str::<OrderEvent<'_>>(raw) {
                self.apply_order_event(cx, row, message.time.0);
            }
        } else if matches!(
            event,
            "user_account_update" | "user_order_update" | "user_balance_update"
        ) && message.status != "ok"
            && !matches!(&*message.code, "0" | "200")
        {
            cx.log(
                LogLevel::Error,
                format!("Bitrue private subscription failed: {text}"),
            );
            self.restart_private(cx);
        }
        true
    }
    fn apply_order_event(&mut self, cx: &mut Cx<'_, Pending>, row: OrderEvent<'_>, time: f64) {
        let raw = row.s.to_uppercase();
        let Some(market) = cx
            .store
            .markets()
            .iter()
            .find(|m| {
                m.symbol == raw
                    || m.id == raw
                    || m.symbol.replace(['-', '_'], "") == raw.replace(['-', '_'], "")
                    || m.symbol.trim_start_matches("E-").replace('-', "") == raw.replace('_', "")
            })
            .cloned()
        else {
            return;
        };
        let id = if !row.i.is_empty() {
            row.i.to_string()
        } else {
            row.order_id.to_string()
        };
        if id.is_empty() {
            return;
        }
        let Some(side) = side(&row.side) else {
            return;
        };
        let Some(kind) = order_type(&row.o) else {
            return;
        };
        let Some(status) = order_status(&row.status)
            .or_else(|| (row.x == "CANCEL").then_some(OrderStatus::Canceled))
        else {
            return;
        };
        let scale = market.contract_size.unwrap_or(market.precision.amount);
        let amount = if row.q.0 > 0.0 {
            row.q.0
        } else {
            row.volume.0 * scale
        };
        let filled = decimal(row.z.0 * scale);
        if amount < 0.0 || filled < 0.0 || !amount.is_finite() || !filled.is_finite() {
            return;
        }
        let existing = cx.store.orders().iter().find(|o| o.id == id).cloned();
        let previous = self
            .filled_native(&id)
            .max(existing.as_ref().map_or(0.0, |o| o.filled));
        let quote = (row.ap.0 > 0.0).then_some(filled * row.ap.0);
        let execution_price = if row.execution_price.0 > 0.0 {
            row.execution_price.0
        } else if filled > previous && (previous == 0.0 || self.account.quotes.contains_key(&id)) {
            quote.map_or(0.0, |value| {
                (value - self.account.quotes.get(&id).copied().unwrap_or(0.0)) / (filled - previous)
            })
        } else {
            0.0
        };
        if row.x == "TRADE"
            && filled > previous
            && execution_price.is_finite()
            && execution_price > 0.0
        {
            cx.fill(Fill {
                order_id: id.clone(),
                symbol: market.symbol.clone(),
                side,
                price: decimal(execution_price),
                amount: decimal(filled - previous),
            });
        }
        if filled >= previous
            && let Some(quote) = quote.filter(|v| v.is_finite())
        {
            self.account.quotes.insert(id.clone(), quote);
        }
        self.account.progress(&id, filled);
        if terminal(status) {
            self.account.remember_terminal(&id);
        }
        if !terminal(status)
            && (self.account.terminal.contains(&id)
                || filled < previous
                || existing.as_ref().is_some_and(|o| {
                    terminal(o.status)
                        || o.last_update > time.max(row.time.0) && time.max(row.time.0) > 0.0
                }))
        {
            return;
        }
        if existing
            .as_ref()
            .is_some_and(|o| o.status == OrderStatus::Closed)
            && status == OrderStatus::Canceled
        {
            return;
        }
        self.account.orders_revision += 1;
        let filled = filled.max(previous);
        let amount = amount
            .max(existing.as_ref().map_or(0.0, |o| o.amount))
            .max(filled);
        let price = if matches!(kind, OrderType::StopLoss | OrderType::TakeProfit) {
            if row.trigger_price.0 > 0.0 {
                row.trigger_price.0
            } else {
                existing.as_ref().map_or(row.p.0, |o| o.price)
            }
        } else if row.p.0 > 0.0 {
            row.p.0
        } else {
            row.ap.0
        };
        cx.store.upsert_order(Order {
            id: id.clone(),
            parent_id: None,
            status,
            symbol: market.symbol.clone(),
            kind,
            side,
            price,
            amount,
            filled,
            remaining: (amount - filled).max(0.0),
            reduce_only: row.reduce.unwrap_or(row.open == "CLOSE"),
            close: None,
            client_order_id: [row.c.as_ref(), row.client_order_id.as_ref()]
                .into_iter()
                .find(|s| !s.is_empty())
                .map(str::to_owned),
            time_in_force: match &*row.o {
                "IOC" => Some(TimeInForce::ImmediateOrCancel),
                "FOK" => Some(TimeInForce::FillOrKill),
                "POST_ONLY" => Some(TimeInForce::PostOnly),
                _ => None,
            },
            last_update: time.max(row.time.0),
            trigger_type: trigger(row.trigger_price_type.0),
            take_profit: existing.as_ref().and_then(|o| o.take_profit),
            stop_loss: existing.as_ref().and_then(|o| o.stop_loss),
            extra: Some(OrderExtra {
                filled_in_placement_units: Some(filled),
                remaining_native: Some((amount - filled).max(0.0)),
                ..OrderExtra::default()
            }),
        });
        if row.x == "TRADE" {
            self.fast_refresh(cx, Some(market.symbol));
        }
    }
    fn snapshot_item(
        &self,
        cx: &Cx<'_, Pending>,
        data: &mut Snapshot,
        item: &str,
    ) -> Result<(), ()> {
        let coin: Coin<'_> = serde_json::from_str(item).map_err(|_| ())?;
        if !matches!(&*coin.margin_coin, "USDT" | "USDC") {
            return Ok(());
        }
        let groups = coin.position_vos.ok_or(())?;
        let balance = Balance {
            total: coin.total_equity.0,
            free: coin.account_normal.0,
            used: (coin.total_equity.0 - coin.account_normal.0)
                .max(coin.account_lock.0)
                .max(coin.total_position_normal.0)
                .max(coin.total_cost.0)
                .max(coin.sum_maintenance_margin.map_or(0.0, |n| n.0))
                .max(0.0),
            upnl: coin.sum_open_realized_amount.0,
        };
        data.balances.insert(coin.margin_coin.to_string(), balance);
        for group in groups {
            if group.contract_name.is_empty() {
                return Err(());
            }
            let Some(market) = cx.store.market(&group.contract_name) else {
                continue;
            };
            for row in group.positions {
                let side = match &*row.side {
                    "BUY" => PositionSide::Long,
                    "SELL" => PositionSide::Short,
                    _ => return Err(()),
                };
                let margin_type = match row.position_type.0 {
                    1.0 => safe_cex_engine::model::MarginType::Cross,
                    2.0 => safe_cex_engine::model::MarginType::Isolated,
                    _ => return Err(()),
                };
                if row.volume.0 < 0.0 || row.open_price.0 < 0.0 || row.leverage_level.0 <= 0.0 {
                    return Err(());
                }
                let setting = MarketSetting {
                    leverage: row.leverage_level.0,
                    margin_type: Some(margin_type),
                    ..MarketSetting::default()
                };
                if data
                    .settings
                    .iter()
                    .any(|(symbol, prior)| symbol == &market.symbol && *prior != setting)
                {
                    return Err(());
                }
                data.settings.push((market.symbol.clone(), setting));
                if row.volume.0 == 0.0 {
                    continue;
                }
                if data.positions.iter().any(|p| p.symbol == market.symbol) {
                    return Err(());
                }
                let existing = cx
                    .store
                    .positions()
                    .iter()
                    .find(|p| p.symbol == market.symbol);
                let liquidation = [row.force_liquidation_price, row.reduce_price]
                    .into_iter()
                    .flatten()
                    .map(|n| n.0)
                    .find(|n| *n > 0.0)
                    .unwrap_or_else(|| existing.map_or(0.0, |p| p.liquidation_price));
                let last = cx.store.ticker(&market.symbol).map_or(0.0, |t| t.last);
                data.positions.push(Position {
                    id: market.symbol.clone(),
                    symbol: market.symbol.clone(),
                    side,
                    entry_price: row.open_price.0,
                    notional: row.position_balance.0,
                    leverage: row.leverage_level.0,
                    margin_type: Some(margin_type),
                    position_mode: Some(PositionMode::Oneway),
                    unrealized_pnl: (if side == PositionSide::Long {
                        1.0
                    } else {
                        -1.0
                    }) * (last - row.open_price.0)
                        * row.volume.0,
                    realized_pnl: None,
                    contracts: row.volume.0,
                    liquidation_price: liquidation,
                    margin_size: None,
                    last_update: None,
                });
            }
        }
        Ok(())
    }
    fn map_rest_order(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        row: &OrderRow<'_>,
        child: bool,
    ) -> Result<Order, ()> {
        if row.order_id.is_empty() || (!row.contract_name.is_empty() && row.contract_name != symbol)
        {
            return Err(());
        }
        let protection = match row.trigger_type.0 {
            1.0 => Some(OrderType::StopLoss),
            2.0 => Some(OrderType::TakeProfit),
            _ => None,
        };
        let kind = if row.condition_order || child || row.kind.is_empty() {
            protection.or_else(|| order_type(&row.kind))
        } else {
            order_type(&row.kind)
        }
        .ok_or(())?;
        let conditional = matches!(kind, OrderType::StopLoss | OrderType::TakeProfit);
        let amount = if conditional && !child && row.executed_qty.0 > 0.0 {
            row.executed_qty.0
        } else {
            row.orig_qty.ok_or(())?.0
        };
        let filled = if conditional { 0.0 } else { row.executed_qty.0 };
        if amount < 0.0 || filled < 0.0 || filled > amount || row.price.0 < 0.0 {
            return Err(());
        }
        let status = order_status(&row.status).ok_or(())?;
        let existing = cx
            .store
            .orders()
            .iter()
            .find(|o| o.id == row.order_id.as_ref());
        Ok(Order {
            id: row.order_id.to_string(),
            parent_id: None,
            status,
            symbol: symbol.to_owned(),
            kind,
            side: side(&row.side).ok_or(())?,
            price: if conditional {
                row.trigger_price.0
            } else {
                row.price.0
            },
            amount,
            filled,
            remaining: amount - filled,
            reduce_only: row.action == "CLOSE",
            close: None,
            client_order_id: (!row.client_order_id.is_empty())
                .then(|| row.client_order_id.to_string()),
            time_in_force: match &*row.kind {
                "IOC" => Some(TimeInForce::ImmediateOrCancel),
                "FOK" => Some(TimeInForce::FillOrKill),
                "POST_ONLY" => Some(TimeInForce::PostOnly),
                _ => None,
            },
            last_update: row.transact_time.0,
            trigger_type: trigger(row.trigger_price_type.0)
                .or_else(|| existing.and_then(|o| o.trigger_type)),
            take_profit: existing.and_then(|o| o.take_profit),
            stop_loss: existing.and_then(|o| o.stop_loss),
            extra: Some(OrderExtra {
                filled_in_placement_units: Some(filled),
                remaining_native: Some(amount - filled),
                ..OrderExtra::default()
            }),
        })
    }
    fn map_order_tree(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        row: &OrderRow<'_>,
        parent: Option<&Order>,
    ) -> Result<Vec<Order>, ()> {
        let mut order = self.map_rest_order(cx, symbol, row, parent.is_some())?;
        if let Some(parent) = parent {
            if !matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit) {
                return Err(());
            }
            order.parent_id = Some(parent.id.clone());
            order.reduce_only = true;
            if row.action != "CLOSE" {
                order.side = if parent.side == OrderSide::Buy {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                };
            }
        }
        let mut children = Vec::new();
        for child in &row.child_orders {
            let child_rows = self.map_order_tree(cx, symbol, child, Some(&order))?;
            for mapped in child_rows {
                if mapped.id == order.id || children.iter().any(|o: &Order| o.id == mapped.id) {
                    return Err(());
                }
                if mapped.kind == OrderType::StopLoss {
                    order.stop_loss = Some(mapped.price);
                }
                if mapped.kind == OrderType::TakeProfit {
                    order.take_profit = Some(mapped.price);
                }
                children.push(mapped);
            }
        }
        let mut rows = vec![order];
        rows.extend(children);
        Ok(rows)
    }
    pub(crate) fn account_command(&mut self, cx: &mut Cx<'_, Pending>, command: &Command) -> bool {
        let Command::FetchLeverage { command, symbol } = command else {
            return false;
        };
        let Some(symbol) = self.resolve_symbol(cx, symbol) else {
            cx.done(*command, Err("Unknown Bitrue market".into()));
            return true;
        };
        if !self.authenticated() {
            cx.done(*command, Err("Bitrue private credentials required".into()));
            return true;
        }
        self.account.leverage_queries.push((*command, symbol));
        self.account.settings_revision += 1;
        self.refresh_account(cx);
        true
    }
    fn finish_leverage(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        settings: Option<&[(String, MarketSetting)]>,
        error: &str,
    ) {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Leverage {
            buy_leverage: f64,
            sell_leverage: f64,
            margin_type: safe_cex_engine::model::MarginType,
            position_mode: PositionMode,
        }
        for (command, symbol) in std::mem::take(&mut self.account.leverage_queries) {
            let result = settings
                .and_then(|rows| rows.iter().find(|(s, _)| *s == symbol))
                .and_then(|(_, s)| {
                    s.margin_type.map(|margin_type| Leverage {
                        buy_leverage: s.leverage,
                        sell_leverage: s.leverage,
                        margin_type,
                        position_mode: PositionMode::Oneway,
                    })
                });
            cx.done(
                command,
                result
                    .map(|r| serde_json::to_string(&r).unwrap())
                    .ok_or_else(|| error.to_owned()),
            );
        }
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Snapshot { data, .. } => {
                if self.snapshot_item(cx, data, item).is_err() {
                    data.invalid = true;
                }
            }
            Request::Orders {
                symbol,
                rows,
                invalid,
                ..
            } => {
                let mapped = serde_json::from_str::<OrderRow<'_>>(item)
                    .map_err(|_| ())
                    .and_then(|row| self.map_order_tree(cx, symbol, &row, None));
                match mapped {
                    Ok(mapped)
                        if mapped
                            .iter()
                            .all(|row| !rows.iter().any(|o| o.id == row.id)) =>
                    {
                        rows.extend(mapped)
                    }
                    _ => *invalid = true,
                }
            }
            _ => {}
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let valid = if matches!(request, Request::Listen { .. } | Request::Keepalive { .. }) {
            (200..300).contains(&response.status)
                && serde_json::from_str::<Listen<'_>>(response.body).is_ok_and(|p| {
                    p.success()
                        || (matches!(request, Request::Listen { .. })
                            && p.code.is_empty()
                            && (!p.listen_key.is_empty()
                                || p.data.as_ref().is_some_and(|d| !d.listen_key.is_empty())))
                })
        } else {
            crate::wire::check(&response).is_ok()
        };
        if !valid {
            self.account_failed(
                cx,
                request,
                &format!("Bitrue account request rejected: {}", response.body),
            );
            return;
        }
        match request {
            Request::Listen { generation } => {
                if generation != self.account.generation {
                    return;
                }
                self.account.listen_pending = false;
                let parsed = serde_json::from_str::<Listen<'_>>(response.body).ok();
                let key = parsed
                    .as_ref()
                    .map(|p| {
                        p.data
                            .as_ref()
                            .map_or(p.listen_key.as_ref(), |d| d.listen_key.as_ref())
                    })
                    .filter(|s| !s.is_empty());
                if let Some(key) = key {
                    let encoded = key
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
                    if !encoded {
                        self.listen_failed(cx, "Malformed Bitrue listen key");
                        return;
                    }
                    self.account.listen_key = Some(key.to_owned());
                    self.account.listen_attempt = 0;
                    self.account.socket =
                        Some(cx.connect(format!("{PRIVATE_URL}?listenKey={key}")));
                    self.account.keepalive = Some(cx.set_timer(REFRESH_MS));
                    self.account.health = Some(cx.set_timer(HEALTH_MS));
                } else {
                    self.listen_failed(cx, "Malformed Bitrue futures listen key response");
                }
            }
            Request::Keepalive { generation } => {
                if generation == self.account.generation {
                    self.account.keepalive = Some(cx.set_timer(REFRESH_MS));
                }
            }
            Request::Snapshot { revision, data } => {
                self.account.snapshot_pending = false;
                #[derive(Deserialize)]
                struct SnapshotEnvelope {
                    data: SnapshotData,
                }
                #[derive(Deserialize)]
                struct SnapshotData {
                    account: Vec<serde::de::IgnoredAny>,
                }
                let schema = serde_json::from_str::<SnapshotEnvelope>(response.body)
                    .is_ok_and(|v| v.data.account.is_empty());
                if !schema || data.invalid || data.balances.is_empty() {
                    self.finish_leverage(cx, None, "Malformed Bitrue account snapshot");
                    cx.log(
                        LogLevel::Error,
                        "Malformed Bitrue account snapshot; retaining previous state",
                    );
                } else if revision == self.account.settings_revision {
                    let closed: Vec<String> = cx
                        .store
                        .positions()
                        .iter()
                        .filter(|p| !data.positions.iter().any(|n| n.id == p.id))
                        .map(|p| p.symbol.clone())
                        .collect();
                    self.finish_leverage(
                        cx,
                        Some(&data.settings),
                        "Bitrue account did not report leverage metadata for this symbol",
                    );
                    for (symbol, setting) in data.settings {
                        cx.store.update_setting(&symbol, |s| *s = setting);
                    }
                    let balance = data
                        .balances
                        .get("USDT")
                        .or_else(|| data.balances.get("USDC"))
                        .cloned()
                        .unwrap();
                    cx.store.set_balance(balance);
                    cx.store.set_positions(data.positions);
                    cx.store.set_hedged(false);
                    self.account.loaded = true;
                    self.account.settings_revision += 1;
                    if !closed.is_empty() {
                        self.refresh_orders(cx, closed);
                    }
                } else {
                    self.account.snapshot_again = true;
                }
                if std::mem::take(&mut self.account.snapshot_again) {
                    self.refresh_account(cx);
                }
            }
            Request::Orders {
                symbol,
                revision,
                rows,
                invalid,
            } => {
                #[derive(Deserialize)]
                struct OrdersEnvelope {
                    data: Vec<serde::de::IgnoredAny>,
                }
                let schema = serde_json::from_str::<OrdersEnvelope>(response.body)
                    .is_ok_and(|v| v.data.is_empty());
                let success = schema && !invalid && revision == self.account.orders_revision;
                if success {
                    let mut retained = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| o.symbol != symbol)
                        .cloned()
                        .collect::<Vec<_>>();
                    for row in rows {
                        if self.account.terminal.contains(&row.id) && !terminal(row.status) {
                            continue;
                        }
                        if row.filled < self.filled_native(&row.id) {
                            if let Some(old) = cx.store.orders().iter().find(|o| o.id == row.id) {
                                retained.push(old.clone());
                            }
                            continue;
                        }
                        self.account.progress(&row.id, row.filled);
                        if terminal(row.status) {
                            self.account.remember_terminal(&row.id);
                        }
                        retained.push(row);
                    }
                    let remove = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| o.symbol == symbol)
                        .map(|o| o.id.clone())
                        .collect::<Vec<_>>();
                    cx.store
                        .remove_orders(&remove.iter().map(String::as_str).collect::<Vec<_>>());
                    for order in retained {
                        cx.store.upsert_order(order);
                    }
                } else {
                    cx.log(
                        LogLevel::Warning,
                        "Bitrue order snapshot incomplete or superseded; retaining previous state",
                    );
                }
                self.order_finished(cx, symbol, success);
            }
        }
    }
    fn listen_failed(&mut self, cx: &mut Cx<'_, Pending>, error: &str) {
        self.account.listen_pending = false;
        let delay = [250.0, 500.0, 1_000.0]
            .get(self.account.listen_attempt)
            .copied()
            .unwrap_or(5_000.0);
        self.account.listen_attempt += 1;
        cx.log(
            LogLevel::Error,
            format!("Bitrue futures private stream: {error}"),
        );
        self.account.reconnect = Some(cx.set_timer(delay));
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        match request {
            Request::Listen { generation } if generation == self.account.generation => {
                self.listen_failed(cx, error)
            }
            Request::Keepalive { generation } if generation == self.account.generation => {
                cx.log(
                    LogLevel::Error,
                    format!("Bitrue listen key refresh failed: {error}"),
                );
                self.restart_private(cx);
            }
            Request::Snapshot { .. } => {
                self.finish_leverage(cx, None, error);
                self.account.snapshot_pending = false;
                cx.log(
                    LogLevel::Error,
                    format!("Bitrue account read failed: {error}"),
                );
                if std::mem::take(&mut self.account.snapshot_again) {
                    self.refresh_account(cx);
                }
            }
            Request::Orders { symbol, .. } => {
                cx.log(
                    LogLevel::Error,
                    format!("Bitrue order read failed: {error}"),
                );
                self.order_finished(cx, symbol, false);
            }
            _ => {}
        }
    }
}
