//! Authenticated account snapshots and monotonic execution reconciliation.
use crate::wire::{Envelope, Str, id, number, optional_number};
use crate::{Connection, Pending, Woo, compact, native};
use safe_cex_engine::Level;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::coverage::{RowKeys, RowRepeat, RowReports};
use safe_cex_engine::execution::{Cursor, ObservedProgress};
use safe_cex_engine::fence::StaleGuard;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Balance, MarginType, Order, OrderExtra, OrderSide, OrderStatus, OrderType, Position,
    PositionMode, PositionSide, StopTriggerType, TimeInForce,
};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::{
    Command, CommandId, Cx, Fill, Method, OrderSnapshotToken, Response, SocketChannel, SocketId,
    TimerId, num,
};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::HashMap;

const STALE_SNAPSHOT_LIMIT: u8 = 2;

pub enum Request {
    Info {
        revision: u64,
    },
    Positions {
        revision: u64,
        rows: Vec<Position>,
        skipped: Skipped,
        bad: bool,
    },
    Orders {
        generation: u64,
        algo: bool,
        page: u32,
        rows: Vec<Order>,
        classifications: Vec<(String, bool)>,
        skipped: Skipped,
        bad: bool,
    },
    Leverage {
        symbol: String,
        command: Option<CommandId>,
        revision: u64,
    },
}
/// Rows a snapshot could not map; known ids keep their stored copy, and a row
/// without an id keeps every stored row of the list.
#[derive(Default)]
pub struct Skipped(RowKeys);
impl Skipped {
    pub(crate) fn add(&mut self, id: Option<&str>) {
        self.0.add(id);
    }
    pub(crate) fn ids(&self) -> &[String] {
        self.0.ids()
    }
    fn into_ids(self) -> Vec<String> {
        self.0.into_ids()
    }
    pub(crate) fn anonymous(&self) -> bool {
        self.0.anonymous() > 0
    }
    pub(crate) fn summary(&self, kind: &str) -> Option<String> {
        self.0.summary(|rows| {
            format!(
                "WOO {kind}: skipped {} unmappable row(s) [{}]",
                rows.count(),
                rows.ids().join(", ")
            )
        })
    }
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct RowIds<'a> {
    #[serde(borrow)]
    symbol: Str<'a>,
    #[serde(deserialize_with = "id")]
    order_id: String,
    #[serde(deserialize_with = "id", rename = "orderId")]
    order_id_camel: String,
    #[serde(deserialize_with = "id", alias = "algoOrderId")]
    algo_order_id: String,
    #[serde(deserialize_with = "id")]
    id: String,
    #[serde(borrow, rename = "childOrders")]
    children: Option<Vec<RowIds<'a>>>,
    #[serde(borrow)]
    holding: Option<Lenient<'a>>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Lenient<'a> {
    Number(f64),
    Text(#[serde(borrow)] Str<'a>),
    Other(serde::de::IgnoredAny),
}
impl Lenient<'_> {
    fn number(&self) -> Option<f64> {
        match self {
            Lenient::Number(n) => Some(*n),
            Lenient::Text(s) => s.parse().ok(),
            Lenient::Other(_) => None,
        }
        .filter(|n| n.is_finite())
    }
}
impl RowIds<'_> {
    fn key(&self, algo: bool) -> &str {
        [
            if algo {
                &self.algo_order_id
            } else {
                &self.order_id
            },
            &self.order_id,
            &self.order_id_camel,
            &self.id,
        ]
        .into_iter()
        .find(|id| !id.is_empty())
        .map_or("", String::as_str)
    }
}
#[derive(Clone, Default)]
pub(crate) struct AlgoMeta {
    pub positional: bool,
}
struct OrdersJoin {
    token: OrderSnapshotToken,
    generation: u64,
    revision: u64,
    regular: Vec<Order>,
    algo: Vec<Order>,
    classifications: Vec<(String, bool)>,
    skipped: Vec<String>,
    /// A page skipped a row without an id: absence removes no stored order.
    unidentified: bool,
    regular_done: bool,
    algo_done: bool,
}
#[derive(Default)]
pub struct Account {
    /// An algo STOP was skipped because no price existed to classify it.
    stop_awaiting_price: bool,
    pub(crate) supervisor: Supervisor,
    authenticated: bool,
    connection: Connection,
    started: bool,
    ready: bool,
    timer: Option<TimerId>,
    tick: u32,
    pause_until: f64,
    info_pending: bool,
    positions_pending: bool,
    positions_guard: StaleGuard,
    orders: Option<OrdersJoin>,
    generation: u64,
    revision: u64,
    versions: HashMap<String, u64>,
    writes: HashMap<String, u64>,
    terminal: HashMap<String, (OrderStatus, f64)>,
    fills: HashMap<String, f64>,
    cumulative: HashMap<String, f64>,
    execution_cumulative: HashMap<String, f64>,
    orders_seeded: bool,
    equity: Option<f64>,
    metadata_at: HashMap<String, f64>,
    expected: HashMap<String, (f64, f64)>,
    refresh: Option<TimerId>,
    refresh_attempt: u32,
    pub(crate) algos: HashMap<String, AlgoMeta>,
    skip_logged: RowReports<&'static str>,
    hidden: std::collections::HashSet<String>,
    isolated_warned: std::collections::HashSet<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Info<'a> {
    #[serde(borrow)]
    account_mode: Str<'a>,
    #[serde(borrow)]
    position_mode: Str<'a>,
    #[serde(deserialize_with = "number")]
    free_collateral: f64,
    #[serde(deserialize_with = "number")]
    total_collateral: f64,
    #[serde(deserialize_with = "number")]
    total_account_value: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionRow<'a> {
    #[serde(borrow)]
    symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    holding: f64,
    #[serde(deserialize_with = "number")]
    average_open_price: f64,
    // GET /v3/positions: https://docs.woox.io/#get-all-position-info-new
    #[serde(default, deserialize_with = "optional_number")]
    mark_price: Option<f64>,
    #[serde(deserialize_with = "number")]
    leverage: f64,
    #[serde(default, deserialize_with = "number")]
    est_liq_price: f64,
    #[serde(borrow)]
    margin_mode: Str<'a>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct OrderRow<'a> {
    #[serde(borrow)]
    symbol: Str<'a>,
    #[serde(deserialize_with = "id")]
    order_id: String,
    #[serde(deserialize_with = "id", rename = "orderId")]
    order_id_camel: String,
    #[serde(deserialize_with = "id", alias = "algoOrderId")]
    algo_order_id: String,
    #[serde(deserialize_with = "id", alias = "rootAlgoOrderId")]
    root_algo_order_id: String,
    #[serde(deserialize_with = "id")]
    id: String,
    #[serde(borrow)]
    status: Str<'a>,
    #[serde(borrow, rename = "algoStatus")]
    algo_status: Str<'a>,
    #[serde(borrow)]
    side: Str<'a>,
    #[serde(borrow, rename = "type")]
    kind: Str<'a>,
    #[serde(borrow, alias = "orderType")]
    order_type: Str<'a>,
    #[serde(borrow, rename = "algoType")]
    algo_type: Str<'a>,
    #[serde(deserialize_with = "optional_number")]
    quantity: Option<f64>,
    #[serde(deserialize_with = "optional_number", alias = "orderQuantity")]
    order_quantity: Option<f64>,
    #[serde(deserialize_with = "optional_number")]
    executed: Option<f64>,
    #[serde(deserialize_with = "optional_number", rename = "totalExecutedQuantity")]
    total_executed: Option<f64>,
    #[serde(deserialize_with = "optional_number")]
    price: Option<f64>,
    #[serde(deserialize_with = "optional_number", alias = "orderPrice")]
    order_price: Option<f64>,
    #[serde(deserialize_with = "optional_number", rename = "triggerPrice")]
    trigger_price: Option<f64>,
    #[serde(deserialize_with = "optional_number", rename = "extremePrice")]
    extreme_price: Option<f64>,
    #[serde(deserialize_with = "optional_number", rename = "callbackValue")]
    callback_value: Option<f64>,
    #[serde(borrow, rename = "triggerPriceType")]
    trigger_price_type: Str<'a>,
    #[serde(alias = "reduceOnly")]
    reduce_only: Option<bool>,
    #[serde(deserialize_with = "optional_number", alias = "clientOrderId")]
    client_order_id: Option<f64>,
    #[serde(deserialize_with = "optional_number", rename = "executedPrice")]
    executed_price: Option<f64>,
    #[serde(deserialize_with = "optional_number", rename = "averageExecutedPrice")]
    average_executed_price: Option<f64>,
    #[serde(deserialize_with = "optional_number", rename = "executedQuantity")]
    executed_quantity: Option<f64>,
    #[serde(deserialize_with = "id", alias = "tradeId")]
    trade_id: String,
    #[serde(borrow, rename = "childOrders")]
    children: Option<Vec<OrderRow<'a>>>,
}
#[derive(Deserialize, Default)]
struct Page {
    meta: Option<Meta>,
    data: Option<Box<Page>>,
}
#[derive(Deserialize)]
struct Meta {
    total: u64,
    records_per_page: u64,
    current_page: u32,
}
#[derive(Deserialize)]
struct Leverage<'a> {
    #[serde(borrow)]
    symbol: Str<'a>,
    #[serde(borrow)]
    margin_mode: Str<'a>,
    #[serde(borrow)]
    position_mode: Str<'a>,
    #[serde(borrow)]
    details: Vec<LeverageDetail<'a>>,
}
#[derive(Deserialize)]
struct LeverageDetail<'a> {
    #[serde(borrow, default)]
    position_side: Str<'a>,
    #[serde(deserialize_with = "number")]
    leverage: f64,
}
fn rounded(v: f64) -> f64 {
    num::round_half_up(v * 100.0) / 100.0
}
fn position_id(symbol: &str, side: PositionSide, hedged: bool) -> String {
    if hedged {
        format!(
            "{symbol}:{}",
            if side == PositionSide::Long {
                "long"
            } else {
                "short"
            }
        )
    } else {
        symbol.into()
    }
}
fn side(s: &str) -> Option<OrderSide> {
    match s {
        "BUY" => Some(OrderSide::Buy),
        "SELL" => Some(OrderSide::Sell),
        _ => None,
    }
}
fn terminal(s: &str) -> Option<OrderStatus> {
    match s {
        "FILLED" => Some(OrderStatus::Closed),
        "CANCELLED" | "CANCELED" | "REJECTED" | "EXPIRED" => Some(OrderStatus::Canceled),
        _ => None,
    }
}
fn order_type(s: &str) -> Option<OrderType> {
    match s {
        "LIMIT" | "POST_ONLY" | "IOC" | "FOK" => Some(OrderType::Limit),
        "MARKET" | "ASK" | "BID" => Some(OrderType::Market),
        "STOP" | "STOP_LOSS" => Some(OrderType::StopLoss),
        "TAKE_PROFIT" => Some(OrderType::TakeProfit),
        "TRAILING_STOP" => Some(OrderType::TrailingStopLoss),
        _ => None,
    }
}
fn trigger(s: &str) -> Option<StopTriggerType> {
    match s {
        "MARK_PRICE" => Some(StopTriggerType::MarkPrice),
        "MARKET_PRICE" => Some(StopTriggerType::LastPrice),
        _ => None,
    }
}
fn holding_side(holding: f64) -> PositionSide {
    if holding > 0.0 {
        PositionSide::Long
    } else {
        PositionSide::Short
    }
}
// Algo children may omit side, status and reduceOnly that the parent carries.
fn leaf_side<'r>(r: &'r OrderRow<'_>, parent: Option<&'r OrderRow<'_>>) -> &'r str {
    match parent {
        Some(p) if r.side.is_empty() => &p.side,
        _ => &r.side,
    }
}
fn leaf_status<'r>(r: &'r OrderRow<'_>, algo: bool, parent: Option<&'r OrderRow<'_>>) -> &'r str {
    if !algo {
        return &r.status;
    }
    match parent {
        // A filled parent cancels the sibling that never executed.
        Some(p)
            if r.algo_status.is_empty()
                && p.algo_status == "FILLED"
                && r.total_executed.or(r.executed).unwrap_or(0.0) <= 0.0 =>
        {
            "CANCELLED"
        }
        Some(p) if r.algo_status.is_empty() => &p.algo_status,
        _ => &r.algo_status,
    }
}
fn row_id<'r>(r: &'r OrderRow<'_>, algo: bool) -> &'r str {
    let regular = if r.order_id.is_empty() {
        &r.order_id_camel
    } else {
        &r.order_id
    };
    [
        if algo { &r.algo_order_id } else { regular },
        regular,
        &r.id,
    ]
    .into_iter()
    .find(|id| !id.is_empty())
    .map_or("", String::as_str)
}
fn leaf_reduce_only(r: &OrderRow<'_>, parent: Option<&OrderRow<'_>>) -> Option<bool> {
    r.reduce_only.or_else(|| parent.and_then(|p| p.reduce_only))
}

impl Account {
    pub(crate) fn new() -> Self {
        Self {
            skip_logged: RowReports::unbounded(RowRepeat::ChangedMessage),
            supervisor: Supervisor::new(Config {
                auth_timeout_ms: Some(10_000.0),
                idle_timeout_ms: 20_000.0,
                ping_interval_ms: Some(6_000.0),
                ..Config::default()
            }),
            ..Self::default()
        }
    }
}

impl Woo {
    fn account_skipped(&mut self, cx: &mut Cx<'_, Pending>, kind: &'static str, s: &Skipped) {
        self.account
            .skip_logged
            .report_summary(cx, kind, Level::Warn, s.summary(kind));
    }
    /// Whether exposure on an unlisted native symbol sits on a catalogue row skipped as
    /// malformed. An anonymous skipped row names no market, so it holds nothing.
    fn catalogue_may_list(&self, native: &str) -> bool {
        native.starts_with("PERP_") && self.skipped_markets.contains(native)
    }
    // Exposure the store cannot show must be visible once, not every poll.
    fn account_hidden(&mut self, cx: &mut Cx<'_, Pending>, kind: &str, symbol: &str) {
        if self.account.hidden.insert(format!("{kind}:{symbol}")) {
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                format!("WOO {kind} on {symbol} cannot be shown: market or ticker unavailable"),
            );
        }
    }
    pub(crate) fn account_revision(&self, key: &str) -> u64 {
        self.account.versions.get(key).copied().unwrap_or(0)
    }
    fn account_stamp(&mut self, key: &str) {
        self.account.revision += 1;
        self.account
            .versions
            .insert(key.into(), self.account.revision);
    }
    pub(crate) fn account_config_write_started(&mut self, key: &str) -> u64 {
        self.account_stamp(key);
        let revision = self.account_revision(key);
        self.account.writes.insert(key.into(), revision);
        revision
    }
    pub(crate) fn account_config_write_finished(&mut self, key: &str, revision: u64) {
        if self.account.writes.get(key) == Some(&revision) {
            self.account.writes.remove(key);
        }
    }
    pub(crate) fn account_algo_positional(&self, id: &str) -> bool {
        self.account.algos.get(id).is_some_and(|m| m.positional)
    }
    pub(crate) fn account_upsert(&mut self, cx: &mut Cx<'_, Pending>, mut row: Order) {
        if let Some(old) = cx.store.order(&row.id) {
            row.filled = row.filled.max(old.filled);
            row.remaining = (row.amount - row.filled).max(0.0);
            if old.status == OrderStatus::Closed
                || old.status == OrderStatus::Canceled && row.status != OrderStatus::Closed
            {
                row.status = old.status;
            }
            if row.extra.is_none() {
                row.extra = old.extra.clone();
            }
            if row.trigger_type.is_none() {
                row.trigger_type = old.trigger_type;
            }
        }
        if let Some((status, _)) = self.account.terminal.get(&row.id)
            && (*status == OrderStatus::Closed || row.status != OrderStatus::Closed)
        {
            row.status = *status;
        }
        if matches!(row.status, OrderStatus::Closed | OrderStatus::Canceled) {
            self.account
                .terminal
                .insert(row.id.clone(), (row.status, cx.now));
        }
        self.account_stamp(&row.id);
        cx.store.upsert_order(row);
    }
    pub(crate) fn account_leverage_applied(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        leverage: f64,
        revision: u64,
    ) {
        if self.account_revision(symbol) > revision {
            return;
        }
        self.account_config_write_finished(symbol, revision);
        self.account_stamp(symbol);
        self.account.metadata_at.insert(symbol.into(), cx.now);
        cx.store.update_setting(symbol, |s| {
            s.leverage = leverage;
            s.margin_type = Some(MarginType::Cross);
            s.buy_leverage = Some(leverage);
            s.sell_leverage = Some(leverage);
        });
        cx.store.update_positions(|p| {
            if p.symbol == symbol {
                p.leverage = leverage;
            }
        });
    }
    pub(crate) fn account_mode_applied(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        hedged: bool,
        revision: u64,
    ) {
        if self.account_revision("$mode") > revision {
            return;
        }
        self.account_config_write_finished("$mode", revision);
        self.account_stamp("$mode");
        cx.store.set_hedged(hedged);
        self.account.metadata_at.clear();
    }
    fn account_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        let host = if self.options.testnet {
            "wss://wss.staging.woox.io/v2/ws/private/stream/"
        } else {
            "wss://wss.woox.io/v2/ws/private/stream/"
        };
        self.account.authenticated = false;
        self.account.supervisor.connecting(cx);
        self.account.connection.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new(format!(
                    "{host}{}",
                    self.options
                        .application_id
                        .as_deref()
                        .unwrap_or("undefined")
                ))
                .channel(SocketChannel::Private),
            ),
        );
    }
    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if std::mem::replace(&mut self.account.started, true) {
            return;
        }
        self.account_connect(cx);
        self.account_refresh(cx);
        self.account.timer = Some(cx.set_poll_timer(2000.0));
    }
    fn account_info_read(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.info_pending {
            return;
        }
        self.account.info_pending = true;
        let req = Request::Info {
            revision: self.account.revision,
        };
        if let Err(e) = self.signed(
            cx,
            Method::Get,
            "/v3/accountinfo",
            vec![],
            None,
            Pending::Account(req),
        ) {
            self.account.info_pending = false;
            cx.retried_failure("woo.account:579", e);
        }
    }
    fn account_positions_read(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.positions_pending || !self.account.ready {
            return;
        }
        self.account.positions_pending = true;
        let req = Request::Positions {
            revision: self.account.revision,
            rows: vec![],
            skipped: Skipped::default(),
            bad: false,
        };
        match self.signed(
            cx,
            Method::Get,
            "/v3/positions",
            vec![],
            None,
            Pending::Account(req),
        ) {
            Ok(id) => cx.stream_response(id, ArrayAt::Path(&["data", "positions"])),
            Err(e) => {
                self.account.positions_pending = false;
                cx.retried_failure("woo.account:604", e);
            }
        }
    }
    fn account_orders_page(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        generation: u64,
        algo: bool,
        page: u32,
    ) {
        let path = if algo {
            "/v3/algo/orders"
        } else {
            "/v1/orders"
        };
        let req = Request::Orders {
            generation,
            algo,
            page,
            rows: vec![],
            classifications: vec![],
            skipped: Skipped::default(),
            bad: false,
        };
        match self.signed(
            cx,
            Method::Get,
            path,
            vec![
                ("status".into(), "INCOMPLETE".into()),
                ("page".into(), page.to_string()),
            ],
            None,
            Pending::Account(req),
        ) {
            Ok(id) => cx.stream_response(
                id,
                if algo {
                    ArrayAt::Path(&["data", "rows"])
                } else {
                    ArrayAt::Field("rows")
                },
            ),
            Err(e) => {
                self.account.orders = None;
                cx.retried_failure("woo.account:650", e);
            }
        }
    }
    fn account_orders_read(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.orders.is_some() {
            return;
        }
        let Some(token) = cx.store.begin_orders_snapshot() else {
            cx.notice(
                Level::Error,
                code::SNAPSHOT_DISCARDED,
                "WOO order snapshot authority exhausted; preserving orders",
            );
            return;
        };
        self.account.generation += 1;
        let generation = self.account.generation;
        self.account.orders = Some(OrdersJoin {
            token,
            generation,
            revision: self.account.revision,
            regular: vec![],
            algo: vec![],
            classifications: vec![],
            skipped: vec![],
            unidentified: false,
            regular_done: false,
            algo_done: false,
        });
        self.account_orders_page(cx, generation, false, 1);
        self.account_orders_page(cx, generation, true, 1);
    }
    pub(crate) fn account_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.started || cx.now < self.account.pause_until {
            return;
        }
        self.account_info_read(cx);
        self.account_positions_read(cx);
        self.account_orders_read(cx);
    }
    fn account_leverage_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        command: Option<CommandId>,
    ) {
        let Some(m) = cx.store.market(symbol) else {
            if let Some(id) = command {
                cx.fail(id, "Market not found");
            }
            return;
        };
        let req = Request::Leverage {
            symbol: symbol.into(),
            command,
            revision: self.account.revision,
        };
        if let Err(e) = self.signed(
            cx,
            Method::Get,
            "/v1/client/futures_leverage",
            vec![
                ("symbol".into(), native(m)),
                ("position_mode".into(), "ONE_WAY".into()),
                ("margin_mode".into(), "CROSS".into()),
            ],
            None,
            Pending::Account(req),
        ) && let Some(id) = command
        {
            cx.fail(id, e);
        }
    }
    pub(crate) fn account_command(&mut self, cx: &mut Cx<'_, Pending>, command: &Command) -> bool {
        if let Command::FetchLeverage { command, symbol } = command {
            self.account_leverage_read(cx, symbol, Some(*command));
            true
        } else {
            false
        }
    }
    pub(crate) fn account_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.connection.socket != Some(socket) {
            return false;
        }
        let c = &mut self.account.connection;
        c.open = true;
        c.ping_at = cx.now;
        self.account.supervisor.opened(cx);
        let Some(text) = crate::sign::websocket_auth(&self.options, cx.now) else {
            return true;
        };
        cx.send(socket, text);
        self.account_refresh(cx);
        true
    }
    pub(crate) fn account_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.connection.socket != Some(socket) {
            return false;
        }
        let c = &mut self.account.connection;
        c.socket = None;
        c.open = false;
        self.account.supervisor.retry(cx);
        true
    }
    fn account_classification(
        &self,
        id: &str,
        row: &OrderRow<'_>,
        parent: Option<&OrderRow<'_>>,
    ) -> bool {
        parent.is_some_and(|p| p.algo_type == "POSITIONAL_TP_SL")
            || row.algo_type == "POSITIONAL_TP_SL"
            || self.account.algos.get(id).is_some_and(|m| m.positional)
    }
    fn account_map_order(
        &self,
        cx: &Cx<'_, Pending>,
        r: &OrderRow<'_>,
        algo: bool,
        parent: Option<&OrderRow<'_>>,
    ) -> Option<Order> {
        let id = row_id(r, algo);
        if id.is_empty() {
            return None;
        }
        let old = cx.store.order(id);
        let symbol = if r.symbol.is_empty() {
            parent
                .map(|p| compact(&p.symbol))
                .or_else(|| old.map(|o| o.symbol.clone()))?
        } else {
            compact(&r.symbol)
        };
        cx.store.market(&symbol)?;
        let kind = if algo && r.algo_type == "STOP" {
            if let Some(kind) = old
                .map(|o| o.kind)
                .filter(|k| matches!(k, OrderType::StopLoss | OrderType::TakeProfit))
            {
                Some(kind)
            } else {
                let price = r.trigger_price?;
                let ticker = cx.store.ticker(&symbol)?;
                let reference =
                    if trigger(&r.trigger_price_type) == Some(StopTriggerType::LastPrice) {
                        ticker.last
                    } else {
                        ticker.mark
                    };
                if price <= 0.0 || reference <= 0.0 {
                    return None;
                }
                Some(
                    if match side(leaf_side(r, parent))? {
                        OrderSide::Sell => price > reference,
                        OrderSide::Buy => price < reference,
                    } {
                        OrderType::TakeProfit
                    } else {
                        OrderType::StopLoss
                    },
                )
            }
        } else if algo {
            order_type(&r.algo_type)
        } else {
            order_type(if r.kind.is_empty() {
                &r.order_type
            } else {
                &r.kind
            })
        }
        .or_else(|| old.map(|o| o.kind))?;
        let quantity = r
            .quantity
            .or(r.order_quantity)
            .or_else(|| old.map(|o| o.amount))?;
        let filled = r
            .total_executed
            .or(r.executed)
            .unwrap_or_else(|| old.map_or(0.0, |o| o.filled));
        if quantity < 0.0 || filled < 0.0 {
            return None;
        }
        let parent_id = if algo {
            Some(
                parent
                    .map(|p| p.root_algo_order_id.as_str())
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        (!r.root_algo_order_id.is_empty()).then_some(r.root_algo_order_id.as_str())
                    })
                    .or_else(|| old.and_then(|o| o.parent_id.as_deref()))
                    .unwrap_or(id)
                    .into(),
            )
        } else {
            None
        };
        let source = trigger(&r.trigger_price_type);
        let price = if algo {
            r.trigger_price.filter(|v| *v > 0.0).or_else(|| {
                r.extreme_price.zip(r.callback_value).map(|(a, b)| {
                    if side(leaf_side(r, parent)).or_else(|| old.map(|o| o.side))
                        == Some(OrderSide::Buy)
                    {
                        a + b
                    } else {
                        a - b
                    }
                })
            })
        } else {
            r.price.or(r.order_price)
        }
        .or_else(|| old.map(|o| o.price))
        .unwrap_or(0.0);
        let extra = source.map(|s| OrderExtra {
            tp_trigger_type: (kind == OrderType::TakeProfit).then_some(s),
            sl_trigger_type: (kind != OrderType::TakeProfit).then_some(s),
            ..Default::default()
        });
        Some(Order {
            id: id.into(),
            parent_id,
            status: terminal(leaf_status(r, algo, parent)).unwrap_or(OrderStatus::Open),
            symbol,
            kind,
            side: side(leaf_side(r, parent)).or_else(|| old.map(|o| o.side))?,
            price,
            amount: quantity,
            filled,
            remaining: (quantity - filled).max(0.0),
            reduce_only: leaf_reduce_only(r, parent)
                .unwrap_or_else(|| old.is_some_and(|o| o.reduce_only)),
            close: None,
            client_order_id: r
                .client_order_id
                .filter(|n| *n > 0.0)
                .map(num::format)
                .or_else(|| old.and_then(|o| o.client_order_id.clone())),
            time_in_force: if !algo && r.kind.is_empty() {
                old.and_then(|o| o.time_in_force)
                    .or(Some(TimeInForce::GoodTillCancel))
            } else {
                (!algo).then_some(if r.kind == "POST_ONLY" {
                    TimeInForce::PostOnly
                } else {
                    TimeInForce::GoodTillCancel
                })
            },
            last_update: cx.now,
            trigger_type: source,
            take_profit: None,
            stop_loss: None,
            extra,
        })
    }
    pub(crate) fn account_item(&mut self, cx: &mut Cx<'_, Pending>, req: &mut Request, item: &str) {
        match req {
            Request::Positions {
                rows, skipped, bad, ..
            } => {
                let Ok(r) = serde_json::from_str::<PositionRow<'_>>(item) else {
                    match serde_json::from_str::<RowIds<'_>>(item) {
                        Ok(ids) if !ids.symbol.is_empty() => {
                            let symbol = compact(&ids.symbol);
                            let holding = ids.holding.as_ref().and_then(Lenient::number);
                            if holding == Some(0.0) {
                                return;
                            }
                            if cx.store.market(&symbol).is_none() {
                                if self.catalogue_may_list(&ids.symbol) {
                                    *bad = true;
                                }
                                return;
                            }
                            // An unreadable side keeps every stored leg of the symbol.
                            let key = holding.map_or_else(
                                || symbol.clone(),
                                |h| position_id(&symbol, holding_side(h), cx.store.hedged()),
                            );
                            skipped.add(Some(&key));
                        }
                        _ => skipped.add(None),
                    }
                    return;
                };
                let symbol = compact(&r.symbol);
                if r.holding == 0.0 {
                    return;
                }
                if cx.store.market(&symbol).is_none() {
                    // Only exposure that may sit on a skipped catalogue row is ambiguous.
                    if self.catalogue_may_list(&r.symbol) {
                        *bad = true;
                        cx.notice(
                            Level::Warn,
                            code::SNAPSHOT_DISCARDED,
                            "WOO owned position market unavailable; preserving previous snapshot",
                        );
                    } else if r.symbol.starts_with("PERP_") {
                        self.account_hidden(cx, "position", &r.symbol);
                    }
                    return;
                }
                let side = holding_side(r.holding);
                let hedged = cx.store.hedged();
                if r.average_open_price <= 0.0 || r.leverage <= 0.0 {
                    skipped.add(Some(&position_id(&symbol, side, hedged)));
                    return;
                }
                // The private response publishes its own mark. Public startup may
                // still be retrying, so no entry-price or zero-price valuation.
                let mark = r.mark_price.filter(|price| *price > 0.0).or_else(|| {
                    cx.store.ticker(&symbol).and_then(|ticker| {
                        [ticker.mark, ticker.last]
                            .into_iter()
                            .find(|price| price.is_finite() && *price > 0.0)
                    })
                });
                let Some(mark) = mark else {
                    *bad = true;
                    cx.notice(Level::Error, code::VALUATION_UNAVAILABLE, "WOO position mark unavailable; preserving previous positions and valuation");
                    return;
                };
                let contracts = r.holding.abs();
                rows.push(Position {
                    id: position_id(&symbol, side, hedged),
                    symbol,
                    side,
                    entry_price: r.average_open_price,
                    notional: contracts * mark,
                    leverage: r.leverage,
                    margin_type: Some(if r.margin_mode == "CROSS" {
                        MarginType::Cross
                    } else {
                        MarginType::Isolated
                    }),
                    position_mode: Some(if hedged {
                        PositionMode::Hedged
                    } else {
                        PositionMode::Oneway
                    }),
                    unrealized_pnl: rounded((mark - r.average_open_price) * r.holding),
                    realized_pnl: None,
                    contracts,
                    liquidation_price: r.est_liq_price,
                    margin_size: None,
                    last_update: None,
                });
            }
            Request::Orders {
                rows,
                classifications,
                skipped,
                bad,
                algo,
                ..
            } => {
                let Ok(r) = serde_json::from_str::<OrderRow<'_>>(item) else {
                    match serde_json::from_str::<RowIds<'_>>(item) {
                        Ok(ids) => match ids.children.as_deref().filter(|_| *algo) {
                            Some(children) => {
                                children.iter().for_each(|c| skipped.add(Some(c.key(true))));
                            }
                            None => skipped.add(Some(ids.key(*algo))),
                        },
                        Err(_) => skipped.add(None),
                    }
                    return;
                };
                let candidates: Vec<_> = if *algo {
                    r.children
                        .as_ref()
                        .map(|children| children.iter().map(|child| (child, Some(&r))).collect())
                        .unwrap_or_else(|| vec![(&r, None)])
                } else {
                    vec![(&r, None)]
                };
                for (leaf, parent) in candidates {
                    let symbol = if leaf.symbol.is_empty() {
                        parent.map_or_else(String::new, |p| compact(&p.symbol))
                    } else {
                        compact(&leaf.symbol)
                    };
                    if cx.store.market(&symbol).is_none() {
                        let supported = !*algo
                            || matches!(
                                leaf.algo_type.as_ref(),
                                "STOP" | "STOP_LOSS" | "TAKE_PROFIT" | "TRAILING_STOP"
                            );
                        let terminal = terminal(if *algo {
                            &leaf.algo_status
                        } else {
                            &leaf.status
                        })
                        .is_some();
                        let raw = if leaf.symbol.is_empty() {
                            parent.map_or("", |p| &p.symbol)
                        } else {
                            &leaf.symbol
                        };
                        if raw.starts_with("PERP_")
                            && supported
                            && !terminal
                            && self.catalogue_may_list(raw)
                        {
                            *bad = true;
                            cx.notice(
                                Level::Warn,
                                code::SNAPSHOT_DISCARDED,
                                "WOO working order market unavailable; preserving previous orders",
                            );
                        } else if raw.starts_with("PERP_") {
                            self.account_hidden(cx, "order", raw);
                        }
                        continue;
                    }
                    let relevant = !*algo
                        || matches!(
                            leaf.algo_type.as_ref(),
                            "STOP" | "STOP_LOSS" | "TAKE_PROFIT" | "TRAILING_STOP"
                        );
                    if !relevant {
                        continue;
                    }
                    if *algo
                        && if leaf.algo_type == "TRAILING_STOP" {
                            leaf.extreme_price
                                .zip(leaf.callback_value)
                                .is_none_or(|(a, b)| {
                                    let price =
                                        if side(leaf_side(leaf, parent)) == Some(OrderSide::Buy) {
                                            a + b
                                        } else {
                                            a - b
                                        };
                                    a <= 0.0 || b <= 0.0 || !price.is_finite() || price <= 0.0
                                })
                        } else {
                            leaf.trigger_price.is_none_or(|v| v <= 0.0)
                        }
                    {
                        skipped.add(Some(row_id(leaf, *algo)));
                        continue;
                    }
                    if !leaf.order_id.is_empty()
                        && !leaf.order_id_camel.is_empty()
                        && leaf.order_id != leaf.order_id_camel
                    {
                        skipped.add(Some(row_id(leaf, *algo)));
                        continue;
                    }
                    if leaf
                        .quantity
                        .or(leaf.order_quantity)
                        .is_none_or(|n| n < 0.0)
                        || leaf
                            .total_executed
                            .or(leaf.executed)
                            .is_some_and(|n| n < 0.0)
                    {
                        skipped.add(Some(row_id(leaf, *algo)));
                        continue;
                    }
                    let order = self.account_map_order(cx, leaf, *algo, parent);
                    if order.is_none()
                        && *algo
                        && leaf.algo_type == "STOP"
                        && !cx.store.loaded().tickers
                    {
                        self.account.stop_awaiting_price = true;
                    }
                    if let Some(order) = order {
                        if *algo {
                            classifications.push((
                                order.id.clone(),
                                self.account_classification(&order.id, leaf, parent),
                            ));
                        }
                        rows.push(order);
                    } else {
                        skipped.add(Some(row_id(leaf, *algo)));
                    }
                }
            }
            _ => (),
        }
    }
    fn account_balance_pnl(&mut self, cx: &mut Cx<'_, Pending>) {
        let upnl = rounded(cx.store.positions().iter().map(|p| p.unrealized_pnl).sum());
        let mut b = cx.store.balance().clone();
        b.upnl = upnl;
        if let Some(equity) = self.account.equity {
            b.total = equity - upnl;
        }
        cx.store.set_balance(b);
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        req: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            if response.status == 429 {
                self.account.pause_until = cx.now + 10_000.0;
            }
            self.account_failed(cx, req, "WOO account HTTP read rejected");
            return;
        }
        let envelope = match Envelope::checked(response.body) {
            Ok(v) => v,
            Err(e) => {
                self.account_failed(cx, req, &e);
                return;
            }
        };
        match req {
            Request::Info { revision } => {
                self.account.info_pending = false;
                let Ok(info) = envelope
                    .payload()
                    .and_then(|s| serde_json::from_str::<Info<'_>>(s).map_err(|e| e.to_string()))
                else {
                    cx.notice(
                        Level::Error,
                        code::STREAM_MALFORMED,
                        "Invalid WOO account information",
                    );
                    return;
                };
                if info.account_mode != "FUTURES" {
                    cx.notice(
                        Level::Error,
                        code::CONFIG_INVALID,
                        format!(
                            "WOO X account is in {} mode, only FUTURES mode is supported",
                            info.account_mode
                        ),
                    );
                    return;
                }
                if info.position_mode != "ONE_WAY" && info.position_mode != "HEDGE_MODE" {
                    cx.notice(
                        Level::Error,
                        code::STREAM_MALFORMED,
                        "Invalid WOO position mode",
                    );
                    return;
                }
                if !self.account.writes.contains_key("$mode") {
                    self.account_mode_applied(cx, info.position_mode == "HEDGE_MODE", revision);
                }
                self.account.ready = true;
                self.account.equity = Some(rounded(info.total_account_value));
                cx.store.set_balance(Balance {
                    free: rounded(info.free_collateral),
                    used: rounded(info.total_collateral - info.free_collateral),
                    total: rounded(info.total_account_value),
                    upnl: cx.store.balance().upnl,
                });
                self.account_balance_pnl(cx);
                self.account_positions_read(cx);
            }
            Request::Positions {
                revision,
                mut rows,
                skipped,
                bad,
            } => {
                self.account.positions_pending = false;
                self.account_skipped(cx, "positions", &skipped);
                if bad {
                    cx.notice(
                        Level::Error,
                        code::SNAPSHOT_DISCARDED,
                        "Invalid WOO positions page",
                    );
                    return;
                }
                if self.account_revision("$mode") > revision
                    || self.account.writes.contains_key("$mode")
                {
                    return;
                }
                // A fill after this read began takes precedence, including fills on newly-opened
                // symbols. One-way fill estimates stay protected by `expected` below; hedged
                // positions come only from REST, so an admitted stale read is re-read at once.
                let current = self.account_revision("$positions") <= revision;
                if !self
                    .account
                    .positions_guard
                    .admit(current, STALE_SNAPSHOT_LIMIT)
                {
                    return;
                }
                for p in &mut rows {
                    if (self.account_revision(&p.symbol) > revision
                        || self.account.writes.contains_key(&p.symbol))
                        && let Some(setting) = cx.store.setting(&p.symbol)
                    {
                        p.leverage = match p.side {
                            PositionSide::Long => setting.buy_leverage,
                            PositionSide::Short => setting.sell_leverage,
                        }
                        .unwrap_or(setting.leverage);
                        p.margin_type = setting.margin_type;
                    }
                }
                let mut retry_symbols = std::collections::HashSet::new();
                for (symbol, (expected, until)) in &self.account.expected {
                    let actual = rows
                        .iter()
                        .filter(|p| &p.symbol == symbol)
                        .map(|p| {
                            p.contracts
                                * if p.side == PositionSide::Long {
                                    1.0
                                } else {
                                    -1.0
                                }
                        })
                        .sum::<f64>();
                    if cx.now < *until && (actual - expected).abs() > 1e-10 {
                        rows.retain(|p| &p.symbol != symbol);
                        rows.extend(
                            cx.store
                                .positions()
                                .iter()
                                .filter(|p| &p.symbol == symbol)
                                .cloned(),
                        );
                        retry_symbols.insert(symbol.clone());
                    }
                }
                self.account
                    .expected
                    .retain(|symbol, _| retry_symbols.contains(symbol));
                let mut settings = HashMap::new();
                for p in &rows {
                    if !self.account.writes.contains_key(&p.symbol)
                        && self.account_revision(&p.symbol) <= revision
                    {
                        let s = settings
                            .entry(p.symbol.clone())
                            .or_insert_with(|| cx.store.setting(&p.symbol).unwrap_or_default());
                        s.leverage = p.leverage;
                        s.margin_type = p.margin_type;
                        match p.side {
                            PositionSide::Long => s.buy_leverage = Some(p.leverage),
                            PositionSide::Short => s.sell_leverage = Some(p.leverage),
                        }
                    }
                }
                for (symbol, setting) in settings {
                    self.account_stamp(&symbol);
                    cx.store.update_setting(&symbol, |s| *s = setting);
                }
                let kept: Vec<_> = cx
                    .store
                    .positions()
                    .iter()
                    .filter(|p| {
                        (skipped.anonymous()
                            || skipped.ids().contains(&p.id)
                            || skipped.ids().contains(&p.symbol))
                            && !rows.iter().any(|r| r.id == p.id)
                    })
                    .cloned()
                    .collect();
                rows.extend(kept);
                for position in &rows {
                    if position.margin_type == Some(MarginType::Isolated)
                        && self.account.isolated_warned.insert(position.symbol.clone())
                    {
                        cx.notice(Level::Warn, code::CONFIG_INVALID, format!("WOO {} uses isolated margin; changing isolated margin settings is unsupported", position.symbol));
                    }
                }
                cx.store.set_positions(rows);
                self.account_balance_pnl(cx);
                self.account_metadata(cx);
                if !retry_symbols.is_empty() {
                    self.account_schedule_fill_refresh(cx);
                }
                if !current && cx.now >= self.account.pause_until {
                    self.account_positions_read(cx);
                }
            }
            Request::Orders {
                generation,
                algo,
                page,
                rows,
                classifications,
                skipped,
                bad,
            } => {
                if self
                    .account
                    .orders
                    .as_ref()
                    .is_none_or(|j| j.generation != generation)
                {
                    return;
                }
                if bad {
                    self.account.orders = None;
                    cx.notice(
                        Level::Error,
                        code::SNAPSHOT_DISCARDED,
                        "Invalid WOO open-order page",
                    );
                    return;
                }
                self.account_skipped(
                    cx,
                    if algo { "algo orders" } else { "open orders" },
                    &skipped,
                );
                let Ok(meta) = serde_json::from_str::<Page>(response.body) else {
                    self.account.orders = None;
                    cx.notice(
                        Level::Error,
                        code::SNAPSHOT_DISCARDED,
                        "Invalid WOO open-order envelope",
                    );
                    return;
                };
                let meta = if algo {
                    meta.data.and_then(|page| page.meta)
                } else {
                    meta.meta
                };
                let Some(meta) =
                    meta.filter(|meta| meta.records_per_page > 0 && meta.current_page == page)
                else {
                    self.account.orders = None;
                    cx.notice(
                        Level::Error,
                        code::SNAPSHOT_DISCARDED,
                        "Invalid WOO open-order page metadata",
                    );
                    return;
                };
                let more = meta.total > meta.records_per_page.saturating_mul(u64::from(page));
                let join = self.account.orders.as_mut().unwrap();
                let dest = if algo {
                    &mut join.algo
                } else {
                    &mut join.regular
                };
                let before = dest.len();
                for row in rows {
                    if !dest.iter().any(|o| o.id == row.id) {
                        dest.push(row);
                    }
                }
                let added = dest.len() != before
                    || skipped.ids().iter().any(|id| !join.skipped.contains(id));
                let anonymous = skipped.anonymous();
                join.skipped.extend(skipped.into_ids());
                join.unidentified |= anonymous;
                join.classifications
                    .extend(dest[before..].iter().filter_map(|row| {
                        classifications
                            .iter()
                            .find(|(id, _)| id == &row.id)
                            .cloned()
                    }));
                if more {
                    if !added || page >= 1000 {
                        self.account.orders = None;
                        cx.notice(
                            Level::Error,
                            code::SNAPSHOT_DISCARDED,
                            "WOO order pagination did not advance",
                        );
                        return;
                    }
                    self.account_orders_page(cx, generation, algo, page + 1);
                    return;
                }
                if algo {
                    join.algo_done = true;
                } else {
                    join.regular_done = true;
                }
                if join.algo_done && join.regular_done {
                    let join = self.account.orders.take().unwrap();
                    let admitted = cx.store.commit_orders_snapshot([join.token], |current| {
                        for (id, positional) in join.classifications {
                            if self.account_revision(&id) <= join.revision {
                                self.account.algos.insert(id, AlgoMeta { positional });
                            }
                        }
                        let mut combined = join.regular;
                        combined.extend(join.algo);
                        for old in current {
                            if (join.unidentified || join.skipped.contains(&old.id))
                                && !combined.iter().any(|r| r.id == old.id)
                            {
                                combined.push(old.clone());
                            }
                        }
                        for old in current {
                            if self.account_revision(&old.id) > join.revision {
                                combined.retain(|r| r.id != old.id);
                                combined.push(old.clone());
                            }
                        }
                        for row in &mut combined {
                            if let Some(old) = current.iter().find(|old| old.id == row.id) {
                                row.filled = row.filled.max(old.filled);
                                row.remaining = (row.amount - row.filled).max(0.0);
                                if matches!(old.status, OrderStatus::Closed | OrderStatus::Canceled)
                                {
                                    row.status = old.status;
                                }
                            }
                            if let Some((status, _)) = self.account.terminal.get(&row.id) {
                                row.status = *status;
                            }
                        }
                        if !self.account.orders_seeded {
                            for order in &combined {
                                self.account
                                    .execution_cumulative
                                    .entry(order.id.clone())
                                    .or_insert(order.filled);
                            }
                            self.account.orders_seeded = true;
                        }
                        combined
                    });
                    if !admitted {
                        return;
                    }
                    self.account_metadata(cx);
                    // A read built before the ticker seed skipped its stops; once
                    // prices exist, re-read at once instead of at the next poll.
                    if std::mem::take(&mut self.account.stop_awaiting_price)
                        && cx.store.loaded().tickers
                    {
                        self.account_orders_read(cx);
                    }
                }
            }
            Request::Leverage {
                symbol,
                command,
                revision,
            } => {
                let parsed = envelope.payload().and_then(|s| {
                    serde_json::from_str::<Leverage<'_>>(s).map_err(|e| e.to_string())
                });
                let result = match parsed {
                    Ok(data) => {
                        let scoped = data.symbol == native(cx.store.market(&symbol).unwrap())
                            && data.margin_mode == "CROSS"
                            && data.position_mode == "ONE_WAY"
                            && data.details.len() == 1;
                        let selected = scoped
                            .then(|| data.details.iter().find(|d| d.position_side == "BOTH"))
                            .flatten();
                        if let Some(detail) = selected.filter(|d| d.leverage > 0.0) {
                            if !self.account.writes.contains_key(&symbol) {
                                self.account_leverage_applied(
                                    cx,
                                    &symbol,
                                    detail.leverage,
                                    revision,
                                );
                            }
                            let setting = cx.store.setting(&symbol).unwrap_or_default();
                            Ok(format!(
                                r#"{{"buyLeverage":{},"sellLeverage":{},"marginType":"cross"}}"#,
                                num::format(setting.buy_leverage.unwrap_or(setting.leverage)),
                                num::format(setting.sell_leverage.unwrap_or(setting.leverage))
                            ))
                        } else {
                            Err("Invalid WOO futures leverage".into())
                        }
                    }
                    Err(e) => Err(e),
                };
                if let Some(command) = command {
                    match result {
                        Ok(data) => cx.done(command, data),
                        Err(error) => cx.fail(command, error),
                    };
                } else if let Err(e) = result {
                    cx.notice(Level::Warn, code::REQUEST_FAILED, e);
                }
            }
        }
    }
    fn account_metadata(&mut self, cx: &mut Cx<'_, Pending>) {
        let symbols: Vec<_> = cx
            .store
            .orders()
            .iter()
            .filter(|o| o.status == OrderStatus::Open)
            .map(|o| o.symbol.clone())
            .chain(cx.store.positions().iter().map(|p| p.symbol.clone()))
            .collect();
        let mut seen = std::collections::HashSet::new();
        for symbol in symbols {
            let positions: Vec<_> = cx
                .store
                .positions()
                .iter()
                .filter(|p| p.symbol == symbol)
                .collect();
            let need = positions.is_empty() || cx.store.hedged() && positions.len() < 2;
            if need
                && seen.insert(symbol.clone())
                && self
                    .account
                    .metadata_at
                    .get(&symbol)
                    .is_none_or(|at| cx.now - at >= 30_000.0)
            {
                self.account.metadata_at.insert(symbol.clone(), cx.now);
                self.account_leverage_read(cx, &symbol, None);
            }
        }
    }
    pub(crate) fn account_failed(&mut self, cx: &mut Cx<'_, Pending>, req: Request, error: &str) {
        match req {
            Request::Info { .. } => self.account.info_pending = false,
            Request::Positions { .. } => self.account.positions_pending = false,
            Request::Orders { generation, .. } => {
                if self
                    .account
                    .orders
                    .as_ref()
                    .is_some_and(|j| j.generation == generation)
                {
                    self.account.orders = None;
                }
            }
            Request::Leverage {
                command, symbol, ..
            } => {
                self.account.metadata_at.remove(&symbol);
                if let Some(id) = command {
                    cx.fail(id, error);
                    return;
                }
            }
        }
        cx.retried_failure("woo.account:1597", error);
    }
    fn account_schedule_fill_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.refresh.is_some() || self.account.refresh_attempt >= 4 {
            return;
        }
        let delay = if self.account.refresh_attempt == 0 {
            0.0
        } else {
            Backoff::new(150.0, u32::MAX, f64::INFINITY)
                .delay(self.account.refresh_attempt.saturating_sub(1))
        };
        self.account.refresh = Some(cx.set_timer(delay));
    }
    fn account_optimistic_fill(&mut self, cx: &mut Cx<'_, Pending>, fill: &Fill, amount: f64) {
        self.account_stamp("$positions");
        if !cx.store.hedged() {
            let old = cx
                .store
                .positions()
                .iter()
                .find(|p| p.symbol == fill.symbol)
                .cloned();
            let previous = old.as_ref().map_or(0.0, |p| {
                p.contracts
                    * if p.side == PositionSide::Long {
                        1.0
                    } else {
                        -1.0
                    }
            });
            let signed = amount
                * if fill.side == OrderSide::Buy {
                    1.0
                } else {
                    -1.0
                };
            let next = previous + signed;
            let mut positions: Vec<_> = cx
                .store
                .positions()
                .iter()
                .filter(|p| p.symbol != fill.symbol)
                .cloned()
                .collect();
            if next.abs() > 1e-10 {
                let side = if next > 0.0 {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                };
                let same = previous.signum() == next.signum() && previous != 0.0;
                let entry = if same && next.abs() <= previous.abs() {
                    old.as_ref().unwrap().entry_price
                } else if same {
                    (old.as_ref().unwrap().entry_price * previous.abs() + fill.price * amount)
                        / next.abs()
                } else {
                    fill.price
                };
                let mark = cx.store.ticker(&fill.symbol).map_or(fill.price, |t| {
                    if t.mark > 0.0 { t.mark } else { fill.price }
                });
                let setting = cx.store.setting(&fill.symbol);
                positions.push(Position {
                    id: fill.symbol.clone(),
                    symbol: fill.symbol.clone(),
                    side,
                    entry_price: entry,
                    notional: next.abs() * mark,
                    leverage: old
                        .as_ref()
                        .map_or_else(|| setting.map_or(1.0, |s| s.leverage), |p| p.leverage),
                    margin_type: Some(MarginType::Cross),
                    position_mode: Some(PositionMode::Oneway),
                    unrealized_pnl: old.as_ref().map_or(0.0, |p| p.unrealized_pnl),
                    realized_pnl: None,
                    contracts: next.abs(),
                    liquidation_price: old.as_ref().map_or(0.0, |p| p.liquidation_price),
                    margin_size: None,
                    last_update: None,
                });
            }
            cx.store.set_positions(positions);
            self.account.expected.insert(
                fill.symbol.clone(),
                (if next.abs() < 1e-10 { 0.0 } else { next }, cx.now + 1500.0),
            );
        }
        self.account.refresh_attempt = 0;
        self.account_schedule_fill_refresh(cx);
    }
    fn account_execution(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        r: &OrderRow<'_>,
        algo: bool,
        parent: Option<&OrderRow<'_>>,
    ) {
        let status = leaf_status(r, algo, parent);
        let Some(mut order) = self.account_map_order(cx, r, algo, parent) else {
            if let Some(status) = terminal(status) {
                let regular_id = if r.order_id.is_empty() {
                    &r.order_id_camel
                } else {
                    &r.order_id
                };
                let id = if algo { &r.algo_order_id } else { regular_id };
                let id = if id.is_empty() { &r.id } else { id };
                if !id.is_empty() {
                    self.account.terminal.insert(id.clone(), (status, cx.now));
                    self.account_stamp(id);
                    let children: Vec<_> = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| o.parent_id.as_deref() == Some(id.as_str()))
                        .map(|o| o.id.clone())
                        .collect();
                    for child in children {
                        self.account
                            .terminal
                            .insert(child.clone(), (status, cx.now));
                        self.account_stamp(&child);
                        cx.store.set_order_status(&child, status);
                    }
                }
            }
            return;
        };
        if algo {
            let positional = self.account_classification(&order.id, r, parent);
            self.account
                .algos
                .insert(order.id.clone(), AlgoMeta { positional });
        }
        let baseline = self
            .account
            .cumulative
            .get(&order.id)
            .copied()
            .unwrap_or(0.0)
            .max(cx.store.order(&order.id).map_or(0.0, |o| o.filled));
        let cumulative = r.total_executed.or(r.executed);
        let observed = ObservedProgress::new(baseline, cumulative).ok();
        // REST progress describes order state, not executions already delivered to the caller.
        let execution_baseline = self
            .account
            .execution_cumulative
            .get(&order.id)
            .copied()
            .unwrap_or(0.0);
        let key = if r.trade_id.is_empty() {
            None
        } else {
            Some(format!("{}:{}", order.id, r.trade_id))
        };
        let fresh = key
            .as_ref()
            .is_none_or(|k| !self.account.fills.contains_key(k));
        let price = r
            .executed_price
            .or(r.average_executed_price)
            .unwrap_or(order.price);
        let prepared =
            if fresh && matches!(status, "FILLED" | "PARTIAL_FILLED" | "PARTIALLY_FILLED") {
                observed.and_then(|observed| {
                    let delivered = Cursor::seed(execution_baseline, None).ok()?;
                    observed
                        .prepare(delivered, key.as_ref().and(r.executed_quantity), price)
                        .ok()
                        .flatten()
                })
            } else {
                None
            };
        if cumulative.is_none()
            && let Some(prepared) = prepared
        {
            order.filled = prepared.observed();
            order.remaining = (order.amount - order.filled).max(0.0);
            self.account
                .cumulative
                .insert(order.id.clone(), order.filled);
        }
        if cumulative.is_some()
            && let Some(observed) = observed
        {
            self.account
                .cumulative
                .insert(order.id.clone(), observed.quantity());
        }
        self.account_upsert(cx, order.clone());
        if let Some(prepared) = prepared {
            if let Some(key) = key {
                self.account.fills.insert(key, cx.now);
            }
            let delivery = prepared.delivery();
            self.account
                .execution_cumulative
                .insert(order.id.clone(), delivery.next().quantity());
            let fill = Fill {
                id: None,
                order_id: order.id,
                symbol: order.symbol,
                side: order.side,
                price: delivery.price(),
                amount: delivery.amount(),
            };
            // REST may already include an execution whose stream report arrived late.
            let position_amount = prepared.position_amount();
            if position_amount > 0.0 {
                self.account_optimistic_fill(cx, &fill, position_amount);
            } else {
                self.account.refresh_attempt = 0;
                self.account_schedule_fill_refresh(cx);
            }
            cx.fill(fill);
        }
    }
    pub(crate) fn account_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.account.connection.socket != Some(socket) {
            return false;
        }
        self.account.supervisor.received(cx.now);
        #[derive(Deserialize)]
        struct Message<'a> {
            #[serde(borrow, default)]
            event: Str<'a>,
            #[serde(borrow, default)]
            topic: Str<'a>,
            success: Option<bool>,
            #[serde(borrow)]
            data: Option<&'a RawValue>,
            #[serde(borrow, default)]
            message: Str<'a>,
        }
        let Ok(msg) = serde_json::from_str::<Message<'_>>(text) else {
            return true;
        };
        match msg.event.as_ref() {
            "ping" => {
                cx.send(socket, r#"{"event":"pong"}"#);
                return true;
            }
            "pong" => return true,
            "auth" => {
                if msg.success == Some(true) {
                    if self.account.authenticated {
                        return true;
                    }
                    self.account.authenticated = true;
                    self.account.supervisor.authenticated(cx);
                    for topic in ["executionreport", "algoexecutionreportv2"] {
                        cx.send(
                            socket,
                            format!(r#"{{"topic":"{topic}","event":"subscribe"}}"#),
                        );
                    }
                    cx.send(socket, r#"{"event":"ping"}"#);
                } else {
                    cx.notice(
                        Level::Error,
                        code::AUTH_REJECTED,
                        if msg.message.is_empty() {
                            "WOO X private websocket authentication failed".into()
                        } else {
                            msg.message.into_owned()
                        },
                    );
                    cx.close(socket);
                    self.account_close(cx, socket);
                }
                return true;
            }
            _ => (),
        }
        let Some(data) = msg.data else {
            return true;
        };
        if msg.topic == "executionreport" {
            if let Ok(row) = serde_json::from_str::<OrderRow<'_>>(data.get()) {
                self.account_execution(cx, &row, false, None);
            }
        } else if msg.topic == "algoexecutionreportv2" {
            if data.get().trim_start().starts_with('[') {
                if let Ok(rows) = serde_json::from_str::<Vec<OrderRow<'_>>>(data.get()) {
                    for row in rows {
                        if let Some(children) = &row.children {
                            for child in children {
                                self.account_execution(cx, child, true, Some(&row));
                            }
                        } else {
                            self.account_execution(cx, &row, true, None);
                        }
                    }
                }
            } else if let Ok(row) = serde_json::from_str::<OrderRow<'_>>(data.get()) {
                if let Some(children) = &row.children {
                    for child in children {
                        self.account_execution(cx, child, true, Some(&row));
                    }
                } else {
                    self.account_execution(cx, &row, true, None);
                }
            }
        }
        true
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        match self.account.supervisor.on_timer(cx, timer) {
            Tick::Unhandled => (),
            Tick::Reconnect => {
                self.account_connect(cx);
                return true;
            }
            Tick::Ping => {
                if let Some(socket) = self.account.connection.socket {
                    cx.send(socket, r#"{"event":"ping"}"#);
                    self.account.connection.ping_at = cx.now;
                }
                return true;
            }
            Tick::TimedOut(_) => {
                if let Some(socket) = self.account.connection.socket {
                    cx.close(socket);
                    self.account_close(cx, socket);
                }
                return true;
            }
            Tick::Handled => return true,
        }
        if self.account.refresh == Some(timer) {
            self.account.refresh = None;
            self.account.refresh_attempt += 1;
            if cx.now >= self.account.pause_until {
                self.account_positions_read(cx);
            }
            return true;
        }
        if self.account.timer == Some(timer) {
            self.account.timer = Some(cx.set_poll_timer(2000.0));
            self.account.tick += 1;
            self.account
                .terminal
                .retain(|_, (_, at)| cx.now - *at < 300_000.0);
            self.account.fills.retain(|_, at| cx.now - *at < 300_000.0);
            let mut retained: std::collections::HashSet<_> = cx
                .store
                .orders()
                .iter()
                .map(|o| o.id.clone())
                .chain(self.account.terminal.keys().cloned())
                .collect();
            if let Some(join) = &self.account.orders {
                retained.extend(join.regular.iter().chain(&join.algo).map(|o| o.id.clone()));
            }
            self.account
                .cumulative
                .retain(|id, _| retained.contains(id));
            self.account
                .execution_cumulative
                .retain(|id, _| retained.contains(id));
            self.account.algos.retain(|id, _| retained.contains(id));
            self.account.versions.retain(|key, _| {
                key.starts_with('$') || cx.store.market(key).is_some() || retained.contains(key)
            });
            if cx.now >= self.account.pause_until {
                if !self.account.ready || self.account.tick.is_multiple_of(5) {
                    self.account_info_read(cx);
                }
                self.account_positions_read(cx);
                if !cx.store.loaded().orders || self.account.tick % 10 == 5 {
                    self.account_orders_read(cx);
                }
            }
            return true;
        }
        false
    }
}
