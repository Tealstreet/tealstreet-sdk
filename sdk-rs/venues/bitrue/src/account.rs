//! Futures account snapshots and the listen-key stream.
use crate::wire::{Number, Text};
use crate::{Bitrue, Job, Pending};
use safe_cex_engine::Level;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::coverage::SnapshotBuilder;
use safe_cex_engine::execution::{
    Cursor, ExecutionRounding, IntervalDelivery, IntervalHistory, ObservedProgress, PricedInterval,
    QuantityInterval, Rejection, scaled_quantity,
};
use safe_cex_engine::fence::StaleGuard;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Balance, CurrencyBalance, Order, OrderExtra, OrderSide, OrderStatus, OrderType, Position,
    PositionMode, PositionSide, StopTriggerType, TimeInForce,
};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Fill, Method, Outcome, Response, SocketChannel, SocketId,
    TimerId, VenueError,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};

const LISTEN_URL: &str = "https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey";
const PRIVATE_URL: &str = "wss://fapiws.bitrue.com/stream";
const REFRESH_MS: f64 = 30.0 * 60_000.0;
const HISTORY_LIMIT: usize = 500;
const EXECUTION_ROUNDING: ExecutionRounding = ExecutionRounding::Significant15;
/// Consecutive event-fenced snapshots discarded before one commits anyway.
const STALE_SNAPSHOT_LIMIT: u8 = 2;

/// Delay before re-reading a symbol whose open-order read failed `failures` times in a row.
const ORDER_RETRY: Backoff = Backoff::new(500.0, 16, 30_000.0);
const SKIP_LOG_LIMIT: usize = 4_096;
/// A committed snapshot's settings and the position rows it rejected.
type SnapshotSettings<'a> = (
    &'a [(String, MarketSetting)],
    &'a SnapshotBuilder<String, Position>,
);

/// A skipped row is an error only when the store has no copy for it to keep.
fn skip_level(stored: bool) -> Level {
    if stored { Level::Warn } else { Level::Error }
}
/// Logs a skipped row once per `key` until it maps again; polls would otherwise repeat it.
fn log_skip(
    cx: &mut Cx<'_, Pending>,
    logged: &mut HashSet<String>,
    key: &str,
    level: Level,
    message: impl Into<String>,
) {
    if logged.len() >= SKIP_LOG_LIMIT {
        logged.clear();
    }
    if logged.insert(key.to_owned()) {
        cx.notice(level, code::ROW_SKIPPED, message);
    }
}
/// Every order id a malformed open-order row names, its children included, and
/// whether some child's id is unreadable (it could be any cached child).
fn order_tree_ids(item: &str) -> (Vec<String>, bool) {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Identity<'a> {
        #[serde(borrow, default)]
        order_id: Option<Text<'a>>,
        #[serde(borrow, default)]
        child_orders: Option<Vec<Identity<'a>>>,
    }
    fn collect(row: Identity<'_>, ids: &mut Vec<String>, unnamed: &mut bool) {
        match row.order_id.filter(|id| !id.is_empty()) {
            Some(id) => ids.push(id.to_string()),
            None => *unnamed = true,
        }
        for child in row.child_orders.into_iter().flatten() {
            collect(child, ids, unnamed);
        }
    }
    let mut ids = Vec::new();
    let mut unnamed = false;
    match serde_json::from_str::<Identity<'_>>(item) {
        Ok(row) if row.order_id.as_ref().is_some_and(|id| !id.is_empty()) => {
            collect(row, &mut ids, &mut unnamed)
        }
        _ => {}
    }
    (ids, unnamed)
}
enum OrderRead {
    Reconciled,
    Superseded,
    Failed(String),
}

fn matches_lookup(order: &Order, symbol: &str, client_order_id: &str) -> bool {
    !matches!(client_order_id, "" | "0" | "undefined")
        && order.symbol == symbol
        && (order.client_order_id.as_deref() == Some(client_order_id)
            || order.id == client_order_id
            || order.parent_id.as_deref() == Some(client_order_id))
}

pub enum Request {
    Brackets {
        command: CommandId,
        symbol: String,
    },
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
        lookup: Option<(CommandId, String)>,
        rows: SnapshotBuilder<String, Order>,
        quotes: HashMap<String, f64>,
        /// Duplicate ids or an oversized lookup: the whole read is discarded.
        invalid: bool,
    },
}
impl Request {
    pub(crate) fn reset(&mut self) {
        match self {
            Self::Snapshot { data, .. } => *data = Snapshot::default(),
            Self::Orders {
                rows,
                quotes,
                invalid,
                ..
            } => {
                *rows = SnapshotBuilder::default();
                quotes.clear();
                *invalid = false;
            }
            _ => {}
        }
    }
}
fn local_leverage_read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

#[derive(Default)]
pub struct Snapshot {
    balances: HashMap<String, Balance>,
    positions: SnapshotBuilder<String, Position>,
    settings: Vec<(String, MarketSetting)>,
    /// A structural failure or owned exposure on a catalogue-malformed contract.
    invalid: bool,
    valuation_pending: bool,
    uncatalogued: Vec<String>,
}
#[derive(Default)]
pub struct Account {
    started: bool,
    pub(crate) loaded: bool,
    pub(crate) settings_revision: u64,
    pub(crate) orders_revision: u64,
    pub(crate) bracket_limits: HashMap<String, f64>,
    pub(crate) socket: Option<SocketId>,
    generation: u64,
    listen_key: Option<String>,
    listen_pending: bool,
    supervisor: Supervisor,
    keepalive: Option<TimerId>,
    tick: Option<TimerId>,
    fast: Option<TimerId>,
    snapshot_pending: bool,
    snapshot_again: bool,
    snapshot_guard: StaleGuard,
    order_queue: VecDeque<String>,
    order_active: Option<String>,
    order_timer: Option<TimerId>,
    initial_orders: HashSet<String>,
    order_backoff: HashMap<String, OrderBackoff>,
    progress: HashMap<String, OrderProgress>,
    quotes: HashMap<String, (f64, f64)>,
    execution_fills: HashMap<String, f64>,
    execution_quotes: HashMap<String, (f64, f64)>,
    execution_intervals: HashMap<String, IntervalHistory>,
    leverage_queries: Vec<(CommandId, String)>,
    progress_queue: VecDeque<String>,
    terminal: HashMap<String, OrderStatus>,
    terminal_queue: VecDeque<String>,
    /// Contracts (empty when unidentified) whose skipped position row was already logged.
    position_skips: HashSet<String>,
    /// `symbol id` (id empty when unidentified) of open-order rows already logged as skipped.
    order_skips: HashSet<String>,
}
#[derive(Default)]
struct OrderBackoff {
    failures: u32,
    error: String,
    retry: Option<TimerId>,
}
#[derive(Clone, Copy, Default)]
struct OrderProgress {
    filled: f64,
    amount: f64,
    time: f64,
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
    #[serde(default)]
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
    avg_price: Number,
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
    child_orders: Option<Vec<OrderRow<'a>>>,
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
    l: Option<Number>,
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
    #[serde(default, rename = "conditionOrder")]
    condition_order: bool,
    #[serde(default, rename = "triggerType")]
    trigger_type: Number,
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
    fn execution(
        &mut self,
        id: &str,
        proof: PricedInterval,
    ) -> Result<Option<IntervalDelivery>, Rejection> {
        let baseline = self.execution_fills.get(id).copied().unwrap_or(0.0);
        let history = match self.execution_intervals.entry(id.to_owned()) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(IntervalHistory::new(baseline, HISTORY_LIMIT)?)
            }
        };
        Ok(history.prepare(proof)?.commit())
    }
    fn progress(&mut self, id: &str, filled: f64, amount: f64, time: f64) {
        if !self.progress.contains_key(id) {
            self.progress_queue.push_back(id.to_owned());
        }
        self.progress
            .entry(id.to_owned())
            .and_modify(|v| {
                v.filled = v.filled.max(filled);
                v.amount = v.amount.max(amount);
                v.time = v.time.max(time);
            })
            .or_insert(OrderProgress {
                filled,
                amount,
                time,
            });
        while self.progress_queue.len() > HISTORY_LIMIT {
            if let Some(old) = self.progress_queue.pop_front() {
                self.progress.remove(&old);
                self.quotes.remove(&old);
                self.execution_fills.remove(&old);
                self.execution_quotes.remove(&old);
                self.execution_intervals.remove(&old);
            }
        }
    }
    fn remember_terminal(&mut self, id: &str, status: OrderStatus) {
        if !self.terminal.contains_key(id) {
            self.terminal_queue.push_back(id.to_owned());
        }
        self.terminal
            .entry(id.to_owned())
            .and_modify(|previous| {
                if *previous != OrderStatus::Closed {
                    *previous = status;
                }
            })
            .or_insert(status);
        while self.terminal_queue.len() > HISTORY_LIMIT {
            if let Some(old) = self.terminal_queue.pop_front() {
                self.terminal.remove(&old);
            }
        }
    }
}
impl Bitrue {
    pub(crate) fn remember_terminal(&mut self, id: &str) {
        self.account.remember_terminal(id, OrderStatus::Canceled);
    }
    /// Bitrue's REST placement/detail quantities are base units.
    pub(crate) fn filled_native(&self, id: &str) -> f64 {
        self.account.progress.get(id).map_or(0.0, |v| v.filled)
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
        self.account.supervisor = Supervisor::new(Config {
            idle_timeout_ms: 20_000.0,
            ..Config::default()
        });
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
        self.account.tick = Some(cx.set_poll_timer(2_000.0));
    }
    fn acquire_listen_key(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.listen_pending {
            return;
        }
        self.account.listen_pending = true;
        self.account.supervisor.connecting(cx);
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
        if let Some(timer) = self.account.keepalive.take() {
            cx.clear_timer(timer);
        }
        self.account.generation += 1;
        self.account.listen_pending = false;
        self.account.listen_key = None;
        self.account.supervisor.retry(cx);
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
            // An explicit refresh replaces a pending backoff retry; failures keep counting.
            if let Some(timer) = self
                .account
                .order_backoff
                .get_mut(&symbol)
                .and_then(|b| b.retry.take())
            {
                cx.clear_timer(timer);
            }
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
                lookup: None,
                rows: SnapshotBuilder::default(),
                quotes: HashMap::new(),
                invalid: false,
            }),
        );
    }
    fn order_finished(&mut self, cx: &mut Cx<'_, Pending>, symbol: String, read: OrderRead) {
        self.account.order_active = None;
        // Loaded means the initial sweep finished, including visibly failed reads.
        // A failed symbol retains its snapshot and retries on its own backoff.
        self.account.initial_orders.remove(&symbol);
        let superseded = matches!(read, OrderRead::Superseded);
        match read {
            OrderRead::Reconciled => {
                if let Some(backoff) = self.account.order_backoff.remove(&symbol) {
                    cx.notice(
                        Level::Info,
                        code::REQUEST_FAILED,
                        format!(
                            "Bitrue order read for {symbol} recovered after {} failures",
                            backoff.failures
                        ),
                    );
                }
            }
            OrderRead::Superseded => {
                if !self.account.order_queue.contains(&symbol) {
                    self.account.order_queue.push_back(symbol.clone());
                }
            }
            OrderRead::Failed(error) => {
                let backoff = self
                    .account
                    .order_backoff
                    .entry(symbol.clone())
                    .or_default();
                backoff.failures += 1;
                if backoff.error != error {
                    // Already reported once per distinct error and reset on recovery.
                    cx.notice(
                        Level::Error,
                        code::REQUEST_FAILED,
                        format!("Bitrue order read for {symbol} failed: {error}"),
                    );
                    backoff.error = error;
                }
                if let Some(timer) = backoff.retry.take() {
                    cx.clear_timer(timer);
                }
                let delay = cx.jittered(ORDER_RETRY.delay(backoff.failures.saturating_sub(1)));
                backoff.retry = Some(cx.set_timer(delay));
            }
        }
        if self.account.initial_orders.is_empty() {
            cx.store.set_orders(cx.store.orders().to_vec());
        }
        if !self.account.order_queue.is_empty() && self.account.order_timer.is_none() {
            self.account.order_timer = Some(cx.set_timer(if superseded { 500.0 } else { 200.0 }));
        }
    }

    pub(crate) fn lookup_order(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: String,
        client_order_id: String,
    ) {
        if cx
            .store
            .orders()
            .iter()
            .any(|order| matches_lookup(order, &symbol, &client_order_id))
            || !self.authenticated()
        {
            Self::finish_lookup(cx, command, &symbol, &client_order_id, None);
            return;
        }
        self.send(
            cx,
            Method::Get,
            &format!(
                "/fapi/v2/openOrders?contractName={}",
                safe_cex_engine::url::encode(&symbol)
            ),
            None,
            Some(ArrayAt::Field("data")),
            Job::Account(Request::Orders {
                symbol,
                revision: self.account.orders_revision,
                lookup: Some((command, client_order_id)),
                rows: SnapshotBuilder::default(),
                quotes: HashMap::new(),
                invalid: false,
            }),
        );
    }

    fn finish_lookup(
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: &str,
        client_order_id: &str,
        rows: Option<&[Order]>,
    ) {
        #[derive(Serialize)]
        struct Lookup {
            found: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            order: Option<Order>,
            #[serde(skip_serializing_if = "Option::is_none")]
            definitive: Option<bool>,
        }
        let mut order = rows
            .unwrap_or_else(|| cx.store.orders())
            .iter()
            .find(|order| matches_lookup(order, symbol, client_order_id))
            .cloned();
        if let Some(order) = &mut order {
            order
                .time_in_force
                .get_or_insert(TimeInForce::GoodTillCancel);
            if let Some(extra) = &mut order.extra {
                // Bitrue uses base units for both placement and the public order.
                extra.remaining_native = None;
                extra.filled_in_placement_units = None;
                if *extra == OrderExtra::default() {
                    order.extra = None;
                }
            }
        }
        let definitive = order.is_none().then_some(false);
        let result = serde_json::to_string(&Lookup {
            found: order.is_some(),
            order,
            definitive,
        })
        .expect("client order lookup");
        cx.done(command, result);
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
        self.account.supervisor.opened(cx);
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
        match self.account.supervisor.on_timer(cx, timer) {
            Tick::Reconnect => {
                self.acquire_listen_key(cx);
                return true;
            }
            Tick::TimedOut(_) => {
                self.restart_private(cx);
                return true;
            }
            Tick::Handled | Tick::Ping => return true,
            Tick::Unhandled => {}
        }
        if self.account.keepalive == Some(timer) {
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
            self.account.tick = Some(cx.set_poll_timer(2_000.0));
            self.refresh_account(cx);
        } else if self.account.fast == Some(timer) {
            self.account.fast = None;
            self.account.snapshot_again = false;
            self.refresh_account(cx);
        } else if self.account.order_timer == Some(timer) {
            self.account.order_timer = None;
            self.next_orders(cx);
        } else if let Some(symbol) = self
            .account
            .order_backoff
            .iter()
            .find_map(|(symbol, b)| (b.retry == Some(timer)).then(|| symbol.clone()))
        {
            if let Some(backoff) = self.account.order_backoff.get_mut(&symbol) {
                backoff.retry = None;
            }
            self.refresh_orders(cx, vec![symbol]);
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
        self.account.supervisor.received(cx.now);
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
            cx.notice(
                Level::Error,
                code::SUBSCRIPTION_REJECTED,
                private_diagnostic(
                    &format!("Bitrue private subscription failed ({event})"),
                    message.code.parse::<i64>().ok(),
                ),
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
        let existing = cx.store.orders().iter().find(|o| o.id == id).cloned();
        let Some(mut side) = side(&row.side) else {
            return;
        };
        let protection = if row.condition_order {
            match row.trigger_type.0 {
                1.0 => Some(OrderType::StopLoss),
                2.0 => Some(OrderType::TakeProfit),
                _ => None,
            }
        } else {
            None
        };
        let Some(mut kind) = protection.or_else(|| order_type(&row.o)) else {
            return;
        };
        if let Some(previous) = existing
            .as_ref()
            .filter(|o| matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit))
        {
            kind = previous.kind;
            if previous.parent_id.is_some() {
                side = previous.side;
            }
        }
        let Some(status) = order_status(&row.status)
            .or_else(|| (row.x == "CANCEL").then_some(OrderStatus::Canceled))
        else {
            return;
        };
        let scale = market.contract_size.unwrap_or(market.precision.amount);
        let amount = if row.q.0 > 0.0 {
            row.q.0
        } else {
            let Ok(amount) = scaled_quantity(row.volume.0, scale, ExecutionRounding::Binary) else {
                return;
            };
            amount
        };
        let Ok(filled) = scaled_quantity(row.z.0, scale, EXECUTION_ROUNDING) else {
            return;
        };
        if amount < 0.0 || filled < 0.0 || !amount.is_finite() || !filled.is_finite() {
            return;
        }
        let previous = self
            .filled_native(&id)
            .max(existing.as_ref().map_or(0.0, |o| o.filled));
        let watermark = self.account.progress.get(&id).copied().unwrap_or_default();
        let time = time.max(row.time.0);
        let amount = amount
            .max(existing.as_ref().map_or(0.0, |o| o.amount))
            .max(watermark.amount)
            .max(filled);
        // REST progress describes order state, not executions already delivered to the caller.
        let reported = self
            .account
            .execution_fills
            .get(&id)
            .copied()
            .unwrap_or(0.0);
        // Beyond a blind REST interval, only the new suffix has an execution-price proof.
        let Ok(observed) = ObservedProgress::new(previous, Some(filled)) else {
            return;
        };
        let Ok(execution_previous) = observed.delivery_start(reported) else {
            return;
        };
        let execution_quotes = if execution_previous > reported {
            &self.account.quotes
        } else {
            &self.account.execution_quotes
        };
        let prior_quote = execution_quotes
            .get(&id)
            .filter(|(qty, _)| *qty == execution_previous)
            .map(|(_, value)| *value);
        let basis = Cursor::seed(execution_previous, prior_quote).ok();
        let suffix = QuantityInterval::new(execution_previous, filled).ok();
        // Retain presence separately: a rejected last quantity is not missing evidence.
        let last_quantity = row
            .l
            .map(|quantity| scaled_quantity(quantity.0, scale, EXECUTION_ROUNDING));
        let covers_suffix = match last_quantity {
            Some(Ok(last)) => {
                last > 0.0
                    && last <= filled
                    && suffix.is_some_and(|interval| last >= interval.length())
            }
            Some(Err(_)) => false,
            None => true,
        };
        let quote = if row.ap.0 > 0.0 {
            Cursor::seed_with_average(filled, None, Some(row.ap.0)).ok()
        } else if row.execution_price.0 > 0.0 && last_quantity.is_some() && covers_suffix {
            basis.and_then(|basis| basis.direct_suffix(filled, row.execution_price.0).ok())
        } else {
            None
        };
        let proof = if row.execution_price.0 > 0.0 && last_quantity.is_some() {
            last_quantity.and_then(Result::ok).and_then(|last| {
                QuantityInterval::suffix(filled, last, EXECUTION_ROUNDING)
                    .and_then(|interval| {
                        PricedInterval::uniform(interval, row.execution_price.0, EXECUTION_ROUNDING)
                    })
                    .ok()
            })
        } else if covers_suffix {
            basis
                .zip(quote)
                .and_then(|(from, to)| PricedInterval::quotes(from, to, EXECUTION_ROUNDING).ok())
        } else {
            None
        };
        let mut delivery_known = false;
        if row.x == "TRADE"
            && let Some(proof) = proof
            && let Ok(delivery) = self.account.execution(&id, proof)
        {
            delivery_known = true;
            if let Some(delivery) = delivery {
                cx.fill(Fill {
                    id: None,
                    order_id: id.clone(),
                    symbol: market.symbol.clone(),
                    side,
                    price: delivery.price(),
                    amount: delivery.amount(),
                });
            }
        }
        // A no-delivery correction can supply a missing quote at the exact
        // delivered endpoint. It cannot consume quantity or overwrite proof.
        if row.x == "TRADE"
            && let Some(anchor) = basis
                .zip(quote)
                .and_then(|(basis, quote)| basis.enrich_quote(quote))
            && let Some(value) = anchor.value()
        {
            self.account
                .execution_quotes
                .insert(id.clone(), (anchor.quantity(), value));
        }
        if row.x == "TRADE"
            && delivery_known
            && filled >= execution_previous
            && let Some(value) = quote.and_then(Cursor::value)
        {
            self.account
                .execution_quotes
                .insert(id.clone(), (filled, value));
        }
        if row.x == "TRADE" && delivery_known {
            self.account
                .execution_fills
                .insert(id.clone(), filled.max(execution_previous));
        }
        if filled >= previous
            && let Some(value) = quote.and_then(Cursor::value)
        {
            self.account.quotes.insert(id.clone(), (filled, value));
        }
        self.account.progress(&id, filled, amount, time);
        if terminal(status) {
            self.account.remember_terminal(&id, status);
        }
        if !terminal(status)
            && (self.account.terminal.contains_key(&id)
                || filled < previous
                || time > 0.0 && time < watermark.time
                || existing
                    .as_ref()
                    .is_some_and(|o| terminal(o.status) || o.last_update > time && time > 0.0))
        {
            return;
        }
        if (self.account.terminal.get(&id) == Some(&OrderStatus::Closed)
            || existing
                .as_ref()
                .is_some_and(|o| o.status == OrderStatus::Closed))
            && status == OrderStatus::Canceled
        {
            return;
        }
        self.account.orders_revision += 1;
        let filled = filled.max(previous);
        let amount = amount.max(filled);
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
            parent_id: existing.as_ref().and_then(|o| o.parent_id.clone()),
            status,
            symbol: market.symbol.clone(),
            kind,
            side,
            price,
            amount,
            filled,
            remaining: (amount - filled).max(0.0),
            reduce_only: existing.as_ref().is_some_and(|o| o.parent_id.is_some())
                || row.reduce.unwrap_or_else(|| {
                    if row.open.is_empty() {
                        existing.as_ref().is_some_and(|o| o.reduce_only)
                    } else {
                        row.open == "CLOSE"
                    }
                }),
            close: None,
            client_order_id: [row.c.as_ref(), row.client_order_id.as_ref()]
                .into_iter()
                .find(|s| !s.is_empty())
                .map(str::to_owned)
                .or_else(|| existing.as_ref().and_then(|o| o.client_order_id.clone())),
            time_in_force: match &*row.o {
                "IOC" => Some(TimeInForce::ImmediateOrCancel),
                "FOK" => Some(TimeInForce::FillOrKill),
                "POST_ONLY" => Some(TimeInForce::PostOnly),
                _ => None,
            },
            last_update: time,
            trigger_type: trigger(row.trigger_price_type.0)
                .or_else(|| existing.as_ref().and_then(|o| o.trigger_type)),
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
        &mut self,
        cx: &mut Cx<'_, Pending>,
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
                self.reject_position(cx, data, None);
                continue;
            }
            let Some(symbol) = cx
                .store
                .market(&group.contract_name)
                .map(|m| m.symbol.clone())
            else {
                // Only owned exposure on a contract skipped as malformed blocks the snapshot.
                if group.positions.iter().all(|row| row.volume.0 == 0.0) {
                    continue;
                }
                if self
                    .malformed_markets
                    .contains(group.contract_name.as_ref())
                {
                    return Err(());
                }
                data.uncatalogued.push(group.contract_name.to_string());
                continue;
            };
            for row in group.positions {
                // A row that cannot be read keeps this contract's stored position.
                let side = match &*row.side {
                    "BUY" => Some(PositionSide::Long),
                    "SELL" => Some(PositionSide::Short),
                    _ => None,
                };
                let margin_type = match row.position_type.0 {
                    1.0 => Some(safe_cex_engine::model::MarginType::Cross),
                    2.0 => Some(safe_cex_engine::model::MarginType::Isolated),
                    _ => None,
                };
                let leverage = if row.leverage_level.0 > 0.0 {
                    row.leverage_level.0
                } else {
                    cx.store.setting(&symbol).map_or(0.0, |s| s.leverage)
                };
                let (Some(side), Some(margin_type)) = (side, margin_type) else {
                    self.reject_position(cx, data, Some(&symbol));
                    continue;
                };
                if row.volume.0 < 0.0
                    || row.open_price.0 < 0.0
                    || row.leverage_level.0 < 0.0
                    || (row.volume.0 > 0.0 && leverage <= 0.0)
                {
                    self.reject_position(cx, data, Some(&symbol));
                    continue;
                }
                let setting = MarketSetting {
                    leverage,
                    margin_type: Some(margin_type),
                    ..MarketSetting::default()
                };
                if data
                    .settings
                    .iter()
                    .any(|(prior_symbol, prior)| prior_symbol == &symbol && *prior != setting)
                {
                    return Err(());
                }
                data.settings.push((symbol.clone(), setting));
                if row.volume.0 == 0.0 {
                    continue;
                }
                if data.positions.rows().iter().any(|p| p.symbol == symbol) {
                    return Err(());
                }
                let existing = cx
                    .store
                    .positions()
                    .iter()
                    .find(|p| p.symbol == symbol && p.side == side);
                let liquidation = [row.force_liquidation_price, row.reduce_price]
                    .into_iter()
                    .flatten()
                    .map(|n| n.0)
                    .find(|n| *n > 0.0)
                    .unwrap_or_else(|| existing.map_or(0.0, |p| p.liquidation_price));
                let mut valuation_pending = false;
                let unrealized_pnl = cx
                    .store
                    .ticker(&symbol)
                    .map(|ticker| ticker.last)
                    .filter(|last| last.is_finite() && *last > 0.0)
                    .map(|last| {
                        (if side == PositionSide::Long {
                            1.0
                        } else {
                            -1.0
                        }) * (last - row.open_price.0)
                            * row.volume.0
                    })
                    .filter(|value| value.is_finite())
                    .unwrap_or_else(|| {
                        valuation_pending = true;
                        existing.map_or(0.0, |p| p.unrealized_pnl)
                    });
                if !unrealized_pnl.is_finite() {
                    self.reject_position(cx, data, Some(&symbol));
                    continue;
                }
                data.valuation_pending |= valuation_pending;
                self.account.position_skips.remove(&symbol);
                data.positions.push(Position {
                    id: symbol.clone(),
                    symbol: symbol.clone(),
                    side,
                    entry_price: row.open_price.0,
                    notional: row.position_balance.0,
                    leverage,
                    margin_type: Some(margin_type),
                    position_mode: Some(PositionMode::Oneway),
                    unrealized_pnl,
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
    /// Rejects a position row by contract; an unnamed group protects every stored position.
    fn reject_position(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        data: &mut Snapshot,
        symbol: Option<&str>,
    ) {
        let skips = &mut self.account.position_skips;
        match symbol {
            Some(symbol) => {
                let level = skip_level(cx.store.position(symbol).is_some());
                let message = format!("Bitrue skipped malformed position row {symbol}");
                log_skip(cx, skips, symbol, level, message);
            }
            None => {
                let level = skip_level(!cx.store.positions().is_empty());
                let message = "Bitrue skipped malformed position group without a contract";
                log_skip(cx, skips, "", level, message);
            }
        }
        data.positions.reject(symbol.map(str::to_owned));
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
        if row.child_orders.is_some() {
            order.stop_loss = None;
            order.take_profit = None;
        }
        for child in row.child_orders.iter().flatten() {
            let child_rows = self.map_order_tree(cx, symbol, child, Some(&order))?;
            for mapped in child_rows {
                if mapped.id == order.id || children.iter().any(|o: &Order| o.id == mapped.id) {
                    return Err(());
                }
                if !terminal(mapped.status) {
                    if mapped.kind == OrderType::StopLoss {
                        order.stop_loss = Some(mapped.price);
                    }
                    if mapped.kind == OrderType::TakeProfit {
                        order.take_profit = Some(mapped.price);
                    }
                }
                children.push(mapped);
            }
        }
        let mut rows = vec![order];
        rows.extend(children);
        Ok(rows)
    }
    pub(crate) fn account_command(&mut self, cx: &mut Cx<'_, Pending>, command: &Command) -> bool {
        if let Command::FetchMaxLeverage { command, symbol } = command {
            let Some(symbol) = self.resolve_symbol(cx, symbol) else {
                let Ok(context) = cx.admit_http_command(local_leverage_read_policy()) else {
                    return true;
                };
                let _ = cx.fail_context(
                    &context,
                    VenueError::new("Unknown Bitrue market")
                        .kind(ErrorKind::InvalidInput)
                        .outcome(Outcome::NotSent),
                );
                return true;
            };
            if !self.authenticated() {
                let Ok(context) = cx.admit_http_command(local_leverage_read_policy()) else {
                    return true;
                };
                let _ = cx.fail_context(
                    &context,
                    VenueError::new("Bitrue private credentials required")
                        .kind(ErrorKind::InvalidInput)
                        .outcome(Outcome::NotSent),
                );
                return true;
            }
            let contract = &cx.store.market(&symbol).expect("resolved market").id;
            self.send(
                cx,
                Method::Get,
                &format!(
                    "/fapi/v2/leverageBracket?contractName={}",
                    safe_cex_engine::url::encode(contract)
                ),
                None,
                None,
                Job::Account(Request::Brackets {
                    command: *command,
                    symbol,
                }),
            );
            return true;
        }
        let Command::FetchLeverage { command, symbol } = command else {
            return false;
        };
        let Some(symbol) = self.resolve_symbol(cx, symbol) else {
            let Ok(context) = cx.admit_http_command(local_leverage_read_policy()) else {
                return true;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Unknown Bitrue market")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return true;
        };
        if !self.authenticated() {
            let Ok(context) = cx.admit_http_command(local_leverage_read_policy()) else {
                return true;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Bitrue private credentials required")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return true;
        }
        self.account.leverage_queries.push((*command, symbol));
        self.account.settings_revision += 1;
        self.refresh_account(cx);
        true
    }
    fn report_leverage_error(&self, cx: &mut Cx<'_, Pending>, message: impl Into<String>) {
        let message = message.into();
        if self.account.leverage_queries.is_empty() {
            cx.retried_failure("bitrue.account:1401", message);
        } else {
            for (command, _) in &self.account.leverage_queries {
                cx.command_error(*command, message.clone());
            }
        }
    }
    fn finish_leverage(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        settings: Option<SnapshotSettings<'_>>,
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
                // A rejected row leaves its contract's leverage unknown.
                .filter(|(_, positions)| positions.allows_absence(&symbol))
                .and_then(|(rows, _)| {
                    rows.iter()
                        .find(|(s, _)| *s == symbol)
                        .map(|(_, s)| *s)
                        .or_else(|| cx.store.setting(&symbol))
                })
                .filter(|s| s.leverage > 0.0)
                .and_then(|s| {
                    s.margin_type.map(|margin_type| Leverage {
                        buy_leverage: s.leverage,
                        sell_leverage: s.leverage,
                        margin_type,
                        position_mode: PositionMode::Oneway,
                    })
                });
            match result
                .map(|r| serde_json::to_string(&r).unwrap())
                .ok_or_else(|| error.to_owned())
            {
                Ok(data) => cx.done(command, data),
                Err(error) => cx.fail(command, error),
            };
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
                quotes,
                invalid,
                lookup,
                ..
            } => {
                let mapped = serde_json::from_str::<OrderRow<'_>>(item)
                    .map_err(|_| ())
                    .and_then(|row| {
                        if row.avg_price.0 > 0.0
                            && !row.condition_order
                            && !matches!(&*row.kind, "STOP_MARKET" | "TAKE_PROFIT_MARKET")
                            && let Ok(anchor) = Cursor::seed_with_average(
                                row.executed_qty.0,
                                None,
                                Some(row.avg_price.0),
                            )
                            && let Some(value) = anchor.value()
                        {
                            quotes.insert(row.order_id.to_string(), value);
                        }
                        self.map_order_tree(cx, symbol, &row, None)
                    });
                let Ok(mapped) = mapped else {
                    let (mut ids, unnamed_child) = order_tree_ids(item);
                    if unnamed_child && let Some(parent) = ids.first().cloned() {
                        // An unnamed child could be any cached child of its parent.
                        ids.extend(
                            cx.store
                                .orders()
                                .iter()
                                .filter(|o| o.parent_id.as_deref() == Some(parent.as_str()))
                                .map(|o| o.id.clone()),
                        );
                    }
                    let skips = &mut self.account.order_skips;
                    if let Some(id) = ids.first() {
                        let level = skip_level(ids.iter().any(|id| cx.store.order(id).is_some()));
                        let message = format!("Bitrue skipped malformed order row {id}");
                        log_skip(cx, skips, &format!("{symbol} {id}"), level, message);
                    } else {
                        let level =
                            skip_level(!cx.store.orders().iter().any(|o| o.symbol == *symbol));
                        let message =
                            format!("Bitrue skipped malformed {symbol} order row without an id");
                        log_skip(cx, skips, &format!("{symbol} "), level, message);
                        rows.reject(None);
                    }
                    for id in ids {
                        rows.reject(Some(id));
                    }
                    return;
                };
                if mapped
                    .iter()
                    .all(|row| !rows.rows().iter().any(|o| o.id == row.id))
                    && (lookup.is_none() || rows.rows().len() + mapped.len() <= 20_000)
                {
                    for row in &mapped {
                        self.account
                            .order_skips
                            .remove(&format!("{symbol} {}", row.id));
                    }
                    for row in mapped {
                        rows.push(row);
                    }
                } else {
                    *invalid = true;
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
            let code = serde_json::from_str::<crate::wire::Envelope<'_>>(response.body)
                .ok()
                .and_then(|envelope| envelope.code.parse::<i64>().ok())
                .or(Some(i64::from(response.status)));
            self.account_failed_code(cx, request, code);
            return;
        }
        match request {
            Request::Brackets { command, symbol } => {
                #[derive(Deserialize)]
                struct Reply {
                    data: BracketData,
                }
                #[derive(Deserialize)]
                #[serde(untagged)]
                enum BracketData {
                    One(Bracket),
                    Many(Vec<Bracket>),
                }
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Bracket {
                    contract_name: String,
                    brackets: Vec<Tier>,
                }
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Tier {
                    initial_leverage: Number,
                }
                let parsed = serde_json::from_str::<Reply>(response.body)
                    .ok()
                    .and_then(|reply| {
                        let rows = match reply.data {
                            BracketData::One(row) => vec![row],
                            BracketData::Many(rows) => rows,
                        };
                        let contract = &cx.store.market(&symbol)?.id;
                        let mut matching = rows
                            .into_iter()
                            .filter(|row| row.contract_name == *contract);
                        let row = matching.next()?;
                        if matching.next().is_some() || row.brackets.is_empty() {
                            return None;
                        }
                        row.brackets.into_iter().try_fold(0.0_f64, |max, tier| {
                            let value = tier.initial_leverage.0;
                            (value > 0.0 && value <= 125.0 && value.fract() == 0.0)
                                .then_some(max.max(value))
                        })
                    });
                if let Some(max) = parsed {
                    self.account.bracket_limits.insert(symbol.clone(), max);
                    cx.store.update_markets(|market| {
                        if market.symbol == symbol {
                            market.limits.leverage.max = max;
                        }
                    });
                    cx.done(command, safe_cex_engine::num::format(max));
                } else {
                    cx.fail(
                        command,
                        "Malformed Bitrue leverage brackets; retaining known limit",
                    );
                }
            }
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
                        self.listen_failed(cx, None, true);
                        return;
                    }
                    self.account.listen_key = Some(key.to_owned());
                    self.account.socket = Some(
                        cx.connect(
                            safe_cex_engine::SocketSpec::new(format!(
                                "{PRIVATE_URL}?listenKey={key}"
                            ))
                            .channel(SocketChannel::Private),
                        ),
                    );
                    self.account.keepalive = Some(cx.set_timer(REFRESH_MS));
                } else {
                    self.listen_failed(cx, None, true);
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
                let malformed = !schema || data.invalid || data.balances.is_empty();
                let current = revision == self.account.settings_revision;
                // A leverage command waits for a snapshot issued after its write.
                let admit = !malformed
                    && if self.account.leverage_queries.is_empty() {
                        self.account
                            .snapshot_guard
                            .admit(current, STALE_SNAPSHOT_LIMIT)
                    } else {
                        current
                    };
                if malformed {
                    self.report_leverage_error(
                        cx,
                        "Malformed Bitrue account snapshot; retaining previous state",
                    );
                    self.finish_leverage(cx, None, "Malformed Bitrue account snapshot");
                } else if admit {
                    if data.valuation_pending {
                        cx.notice(
                            Level::Warn,
                            code::VALUATION_UNAVAILABLE,
                            "Bitrue ticker price unavailable; retaining prior position valuation",
                        );
                    }
                    for contract in &data.uncatalogued {
                        cx.notice(
                            Level::Warn,
                            code::ROW_SKIPPED,
                            format!("Skipping Bitrue position on uncatalogued contract {contract}"),
                        );
                    }
                    if data.positions.rejected() == 0 {
                        self.account.position_skips.clear();
                    }
                    self.finish_leverage(
                        cx,
                        Some((&data.settings, &data.positions)),
                        "Bitrue account did not report leverage metadata for this symbol",
                    );
                    // A rejected row cannot prove its position flat; keep the stored copy.
                    let positions = data
                        .positions
                        .commit(cx.store.positions().iter().cloned(), |p| p.id.clone());
                    let closed: Vec<String> = cx
                        .store
                        .positions()
                        .iter()
                        .filter(|p| !positions.iter().any(|n| n.id == p.id))
                        .map(|p| p.symbol.clone())
                        .collect();
                    for (symbol, setting) in data.settings {
                        cx.store.update_setting(&symbol, |s| *s = setting);
                    }
                    let balance = data
                        .balances
                        .get("USDT")
                        .or_else(|| data.balances.get("USDC"))
                        .cloned()
                        .unwrap();
                    let mut currencies = data
                        .balances
                        .into_iter()
                        .map(|(currency, balance)| CurrencyBalance { currency, balance })
                        .collect::<Vec<_>>();
                    currencies.sort_by(|a, b| a.currency.cmp(&b.currency));
                    cx.store.set_balance(balance);
                    cx.store.set_currency_balances(currencies);
                    cx.store.set_positions(positions);
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
                lookup,
                rows,
                quotes,
                invalid,
            } => {
                #[derive(Deserialize)]
                struct OrdersEnvelope {
                    data: Vec<serde::de::IgnoredAny>,
                }
                let schema = serde_json::from_str::<OrdersEnvelope>(response.body)
                    .is_ok_and(|v| v.data.is_empty());
                if let Some((command, client_order_id)) = lookup {
                    // TS only checks open orders. A failed read falls
                    // back to the cache, and every miss remains non-definitive.
                    if !schema || invalid {
                        cx.notice(
                            Level::Warn,
                            code::STREAM_MALFORMED,
                            "Malformed Bitrue open-order lookup; using cache",
                        );
                    }
                    Self::finish_lookup(
                        cx,
                        command,
                        &symbol,
                        &client_order_id,
                        (schema && !invalid && rows.rejected() == 0).then_some(rows.rows()),
                    );
                    return;
                }
                let complete = schema && !invalid;
                let success = complete && revision == self.account.orders_revision;
                // A rejected row may be the order a census is looking for.
                let census = success && rows.rejected() == 0;
                if success {
                    let mut retained = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| o.symbol != symbol)
                        .cloned()
                        .collect::<Vec<_>>();
                    if census {
                        let prefix = format!("{symbol} ");
                        self.account
                            .order_skips
                            .retain(|key| !key.starts_with(&prefix));
                    }
                    let accepted = rows.rows().len();
                    let mut rows = rows.commit(
                        cx.store
                            .orders()
                            .iter()
                            .filter(|o| o.symbol == symbol)
                            .cloned(),
                        |o| o.id.clone(),
                    );
                    // Stored copies kept for rejected rows are not new observations.
                    retained.extend(rows.split_off(accepted));
                    for row in rows {
                        if self.account.terminal.contains_key(&row.id) && !terminal(row.status)
                            || self.account.terminal.get(&row.id) == Some(&OrderStatus::Closed)
                                && row.status == OrderStatus::Canceled
                        {
                            continue;
                        }
                        if row.filled < self.filled_native(&row.id) {
                            if let Some(old) = cx.store.orders().iter().find(|o| o.id == row.id) {
                                retained.push(old.clone());
                            }
                            continue;
                        }
                        if self.account.initial_orders.contains(&symbol) {
                            self.account
                                .execution_fills
                                .entry(row.id.clone())
                                .or_insert(row.filled);
                            if let Some(value) = quotes.get(&row.id) {
                                self.account
                                    .execution_quotes
                                    .entry(row.id.clone())
                                    .or_insert((row.filled, *value));
                            }
                        }
                        self.account
                            .progress(&row.id, row.filled, row.amount, row.last_update);
                        if let Some(value) = quotes.get(&row.id) {
                            self.account
                                .quotes
                                .insert(row.id.clone(), (row.filled, *value));
                        }
                        if terminal(row.status) {
                            self.account.remember_terminal(&row.id, row.status);
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
                }
                let read = if self.trade_census(cx, &symbol, revision, census) {
                    cx.orders_reconciled(Some(&symbol));
                    OrderRead::Reconciled
                } else if success {
                    OrderRead::Reconciled
                } else if complete {
                    OrderRead::Superseded
                } else {
                    OrderRead::Failed(
                        "incomplete open-order snapshot; retaining previous state".into(),
                    )
                };
                self.order_finished(cx, symbol, read);
            }
        }
    }
    fn listen_failed(&mut self, cx: &mut Cx<'_, Pending>, code: Option<i64>, malformed: bool) {
        cx.notice(
            Level::Error,
            code::STREAM_UNAVAILABLE,
            private_diagnostic(
                if malformed {
                    "Bitrue futures private stream: malformed listen-key response"
                } else {
                    "Bitrue futures private stream: request rejected or unavailable"
                },
                code,
            ),
        );
        self.restart_private(cx);
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        let body = error
            .strip_prefix("response did not contain the expected list: ")
            .unwrap_or(error);
        let code = serde_json::from_str::<crate::wire::Envelope<'_>>(body)
            .ok()
            .and_then(|envelope| envelope.code.parse::<i64>().ok());
        self.account_failed_code(cx, request, code);
    }
    fn account_failed_code(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        code: Option<i64>,
    ) {
        match request {
            Request::Brackets { command, .. } => cx.fail(
                command,
                private_diagnostic("Bitrue leverage bracket read failed", code),
            ),
            Request::Listen { generation } if generation == self.account.generation => {
                self.listen_failed(cx, code, false)
            }
            Request::Keepalive { generation } if generation == self.account.generation => {
                cx.notice(
                    Level::Error,
                    code::STREAM_UNAVAILABLE,
                    private_diagnostic("Bitrue listen key refresh failed", code),
                );
                self.restart_private(cx);
            }
            Request::Snapshot { .. } => {
                let error = private_diagnostic("Bitrue account read failed", code);
                self.report_leverage_error(cx, error.clone());
                self.finish_leverage(cx, None, &error);
                self.account.snapshot_pending = false;
                if std::mem::take(&mut self.account.snapshot_again) {
                    self.refresh_account(cx);
                }
            }
            Request::Orders {
                symbol,
                revision,
                lookup,
                ..
            } => {
                let message =
                    private_diagnostic(&format!("Bitrue order read for {symbol} failed"), code);
                // Backoff dedupes and logs this reason; it must not carry transport text.
                let reason = private_diagnostic("request failed", code);
                if let Some((command, client_order_id)) = lookup {
                    cx.command_error(command, message);
                    Self::finish_lookup(cx, command, &symbol, &client_order_id, None);
                } else {
                    // A census owned by a placement fails that command, which reports it;
                    // otherwise order_finished logs the failure once per change.
                    if let Some(command) = self.census_owner(&symbol, revision) {
                        cx.command_error(command, message);
                        self.account
                            .order_backoff
                            .entry(symbol.clone())
                            .or_default()
                            .error = reason.clone();
                    }
                    self.trade_census(cx, &symbol, revision, false);
                    self.order_finished(cx, symbol, OrderRead::Failed(reason));
                }
            }
            _ => {}
        }
    }
}

// Only bounded numeric venue codes may cross this private diagnostic boundary.
fn private_diagnostic(message: &str, code: Option<i64>) -> String {
    code.map_or_else(|| message.into(), |code| format!("{message} (code {code})"))
}
