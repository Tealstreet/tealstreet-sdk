use crate::wire::{Envelope, Number};
use crate::{Kucoin, Pending};
use safe_cex_engine::Level;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, Fill, Method, Store, TimerId};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet, VecDeque};
const SKIP_LOG_LIMIT: usize = 4_096;

#[derive(Clone)]
pub(crate) enum Read {
    Positions,
    Balance,
    Orders(bool),
    Mode,
    Margin(String),
    Leverage(String),
}
pub struct Request {
    read: Read,
    revision: u64,
    positions: Vec<Position>,
    orders: Vec<Order>,
    native_ids: HashSet<String>,
    /// Rows that failed to map; the store keeps its copy of these IDs/symbols.
    skipped_orders: HashSet<String>,
    skipped_symbols: HashSet<String>,
    /// A position row without a symbol: absence removes no stored position.
    unidentified: bool,
    page: u32,
    retries: u8,
    invalid: bool,
}
impl Request {
    fn new(read: Read, revision: u64) -> Self {
        Self {
            read,
            revision,
            positions: vec![],
            orders: vec![],
            native_ids: HashSet::new(),
            skipped_orders: HashSet::new(),
            skipped_symbols: HashSet::new(),
            unidentified: false,
            page: 1,
            retries: 0,
            invalid: false,
        }
    }
    fn path(&self, store: &Store) -> String {
        match &self.read {
            Read::Positions => "/api/v1/positions".into(),
            Read::Balance => "/api/v1/account-overview?currency=USDT".into(),
            Read::Orders(stop) => format!(
                "/api/v1/{}?status=active&currentPage={}&pageSize=100",
                if *stop { "stopOrders" } else { "orders" },
                self.page
            ),
            Read::Mode => "/api/v2/position/getPositionMode".into(),
            Read::Margin(symbol) => format!(
                "/api/v2/position/getMarginMode?symbol={}",
                crate::api_symbol(store, symbol)
            ),
            Read::Leverage(symbol) => format!(
                "/api/v2/getCrossUserLeverage?symbol={}",
                crate::api_symbol(store, symbol)
            ),
        }
    }
}
#[derive(Default)]
pub(crate) struct Account {
    timer: Option<TimerId>,
    refresh: Option<TimerId>,
    busy: usize,
    count: u32,
    revision: u64,
    settings_revision: u64,
    pub(crate) mode_known: bool,
    metadata: HashSet<String>,
    order_versions: HashMap<String, u64>,
    order_fills: HashMap<String, f64>,
    order_books: HashMap<String, bool>,
    initial_order_books: u8,
    terminal: HashSet<String>,
    order_absences: HashMap<String, u64>,
    incomplete_orders: HashSet<String>,
    // REST rows may omit submitted brackets; retain them until terminal proof.
    placed_orders: HashMap<String, Order>,
    position_versions: HashMap<String, u64>,
    closures: HashMap<String, f64>,
    executions: HashSet<String>,
    execution_queue: VecDeque<String>,
    /// Unified symbols (empty when unreadable) of rejected position rows already logged.
    position_skips: HashSet<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderRow<'a> {
    #[serde(default)]
    id: &'a str,
    #[serde(default)]
    order_id: &'a str,
    symbol: &'a str,
    #[serde(default)]
    client_oid: Option<&'a str>,
    #[serde(default)]
    order_type: &'a str,
    #[serde(default, rename = "type")]
    event: &'a str,
    #[serde(default)]
    status: &'a str,
    side: &'a str,
    #[serde(default)]
    price: Number,
    #[serde(default)]
    order_price: Number,
    #[serde(default)]
    size: Number,
    #[serde(default)]
    filled_size: Number,
    #[serde(default)]
    remain_size: Option<Number>,
    #[serde(default)]
    reduce_only: Option<bool>,
    #[serde(default)]
    close_order: Option<bool>,
    #[serde(default)]
    time_in_force: Option<&'a str>,
    #[serde(default)]
    post_only: Option<bool>,
    #[serde(default)]
    update_time: Number,
    #[serde(default)]
    ts: Number,
    #[serde(default)]
    order_time: Number,
    #[serde(default)]
    created_at: Number,
    #[serde(default, borrow)]
    stop: Option<&'a RawValue>,
    #[serde(default)]
    stop_type: &'a str,
    #[serde(default)]
    stop_price: Number,
    #[serde(default)]
    stop_price_type: &'a str,
    #[serde(default)]
    stop_loss: Number,
    #[serde(default)]
    take_profit: Number,
    #[serde(default)]
    trigger_stop_up_price: Number,
    #[serde(default)]
    trigger_stop_down_price: Number,
    #[serde(default)]
    match_size: Number,
    #[serde(default)]
    match_price: Number,
    #[serde(default)]
    trade_id: &'a str,
}
pub(crate) fn base_amount(lots: f64, multiplier: f64) -> f64 {
    safe_cex_engine::num::adjust(lots * multiplier, multiplier)
}
pub(crate) fn contract_size(store: &Store, symbol: &str) -> f64 {
    store
        .market(symbol)
        .and_then(|m| m.contract_size)
        .unwrap_or(1.0)
}
fn timestamp(value: f64) -> f64 {
    if value >= 1e17 {
        (value / 1e6).floor()
    } else if value >= 1e14 {
        (value / 1e3).floor()
    } else {
        value
    }
}
fn symbol(store: &Store, id: &str) -> String {
    store
        .market_by_id(id)
        .map(|m| m.symbol.clone())
        .unwrap_or_else(|| crate::internal_symbol(id))
}
impl OrderRow<'_> {
    fn complete(&self) -> bool {
        self.reduce_only.is_some()
            && self.close_order.is_some()
            && self.time_in_force.is_some()
            && self.post_only.is_some()
    }

    fn merge(&self, order: &mut Order, old: &Order) {
        if self.reduce_only.is_none() {
            order.reduce_only = old.reduce_only;
        }
        if self.close_order.is_none() {
            order.close = old.close;
        }
        if self.time_in_force.is_none() && self.post_only.is_none()
            || self.post_only.is_none() && old.time_in_force == Some(TimeInForce::PostOnly)
        {
            order.time_in_force = old.time_in_force;
        } else if self.time_in_force.is_none() && self.post_only != Some(true) {
            order.time_in_force = old
                .time_in_force
                .filter(|tif| *tif != TimeInForce::PostOnly)
                .or(Some(TimeInForce::GoodTillCancel));
        }
        if self.client_oid.is_none() {
            order.client_order_id.clone_from(&old.client_order_id);
        }
        if self.order_type.is_empty()
            && !matches!(self.event, "limit" | "market")
            && self.stop.is_none()
        {
            order.kind = old.kind;
        }
        if self.stop_price_type.is_empty() {
            order.trigger_type = old.trigger_type;
        }
    }

    fn orders(&self, store: &Store, now: f64) -> Vec<Order> {
        let symbol = symbol(store, self.symbol);
        let multiplier = contract_size(store, &symbol);
        let raw_type = if self.order_type.is_empty() {
            self.event
        } else {
            self.order_type
        };
        let side = if self.side == "buy" {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let mut kind = if raw_type == "market" {
            OrderType::Market
        } else {
            OrderType::Limit
        };
        let stop = self.stop.map(|s| s.get()).unwrap_or("false");
        let is_stop = stop != "false" && stop != "null" && stop != "\"\"";
        let mut price = if self.price.get() != 0.0 {
            self.price.get()
        } else {
            self.order_price.get()
        };
        if is_stop || raw_type == "stop" {
            let direction = if self.stop_type.is_empty() {
                stop.trim_matches('"')
            } else {
                self.stop_type
            };
            kind = match direction {
                "loss" | "sl" => OrderType::StopLoss,
                "profit" | "tp" => OrderType::TakeProfit,
                "up" => {
                    if side == OrderSide::Buy {
                        OrderType::StopLoss
                    } else {
                        OrderType::TakeProfit
                    }
                }
                "down" => {
                    if side == OrderSide::Sell {
                        OrderType::StopLoss
                    } else {
                        OrderType::TakeProfit
                    }
                }
                _ => kind,
            };
            price = self.stop_price.get();
        }
        if stop == "true" && self.stop_price_type == "TP" {
            kind = OrderType::TrailingStopLoss;
        }
        let remaining = self
            .remain_size
            .map(|n| n.get())
            .unwrap_or((self.size.get() - self.filled_size.get()).max(0.0));
        let stamp = [
            self.update_time.get(),
            self.ts.get(),
            self.order_time.get(),
            self.created_at.get(),
        ]
        .into_iter()
        .find(|n| *n > 0.0)
        .map(timestamp)
        .unwrap_or(now);
        let status = if ["canceled", "cancelled", "cancel"].contains(&self.status)
            || ["canceled", "cancel"].contains(&self.event)
        {
            OrderStatus::Canceled
        } else if self.event == "filled" || self.status == "done" && remaining == 0.0 {
            OrderStatus::Closed
        } else {
            OrderStatus::Open
        };
        let parent = Order {
            id: if self.id.is_empty() {
                self.order_id
            } else {
                self.id
            }
            .into(),
            parent_id: None,
            status,
            symbol,
            kind,
            side,
            price,
            amount: base_amount(self.size.get(), multiplier),
            filled: base_amount(self.filled_size.get(), multiplier),
            remaining: base_amount(remaining, multiplier),
            reduce_only: self.reduce_only.unwrap_or(false),
            close: self.close_order,
            client_order_id: self.client_oid.map(str::to_owned),
            time_in_force: Some(if self.post_only == Some(true) {
                TimeInForce::PostOnly
            } else {
                match self.time_in_force {
                    Some("IOC") => TimeInForce::ImmediateOrCancel,
                    Some("FOK") => TimeInForce::FillOrKill,
                    _ => TimeInForce::GoodTillCancel,
                }
            }),
            last_update: stamp,
            trigger_type: if is_stop || !self.stop_price_type.is_empty() {
                Some(match self.stop_price_type {
                    "MP" => StopTriggerType::MarkPrice,
                    "IP" => StopTriggerType::IndexPrice,
                    _ => StopTriggerType::LastPrice,
                })
            } else {
                None
            },
            take_profit: None,
            stop_loss: None,
            extra: Some(OrderExtra {
                remaining_native: Some(remaining),
                filled_in_placement_units: Some(self.filled_size.get()),
                ..Default::default()
            }),
        };
        let sl = if self.stop_loss.get() > 0.0 {
            self.stop_loss.get()
        } else if side == OrderSide::Buy {
            self.trigger_stop_down_price.get()
        } else {
            self.trigger_stop_up_price.get()
        };
        let tp = if self.take_profit.get() > 0.0 {
            self.take_profit.get()
        } else if side == OrderSide::Buy {
            self.trigger_stop_up_price.get()
        } else {
            self.trigger_stop_down_price.get()
        };
        let mut orders = vec![parent.clone()];
        for (value, suffix, kind) in [
            (sl, "stop_loss", OrderType::StopLoss),
            (tp, "take_profit", OrderType::TakeProfit),
        ] {
            if value > 0.0 {
                let mut leg = parent.clone();
                leg.id = format!("{}__{suffix}", parent.id);
                leg.side = side.opposite();
                leg.kind = kind;
                leg.price = value;
                leg.reduce_only = true;
                orders.push(leg);
            }
        }
        orders
    }
}
pub(crate) fn map_order(store: &Store, raw: &RawValue, now: f64) -> Result<Vec<Order>, String> {
    let row: OrderRow = serde_json::from_str(raw.get()).map_err(|e| e.to_string())?;
    if row.id.is_empty() && row.order_id.is_empty() {
        return Err("KuCoin order missing id".into());
    }
    Ok(row.orders(store, now))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionRow<'a> {
    symbol: &'a str,
    current_qty: Number,
    #[serde(default)]
    avg_entry_price: Option<Number>,
    #[serde(default)]
    mark_price: Option<Number>,
    #[serde(default)]
    leverage: Option<Number>,
    #[serde(default)]
    unrealised_pnl: Option<Number>,
    #[serde(default)]
    liquidation_price: Option<Number>,
    #[serde(default)]
    margin_mode: &'a str,
    #[serde(default)]
    cross_mode: Option<bool>,
    #[serde(default)]
    position_side: &'a str,
    #[serde(default, borrow)]
    position_mode: Option<&'a RawValue>,
    #[serde(default)]
    hold_mode: &'a str,
    #[serde(default)]
    current_timestamp: Number,
    #[serde(default)]
    ts: Number,
}
fn margin(value: &str, cross: Option<bool>) -> MarginType {
    if matches!(value, "ISOLATED" | "FIXED") || cross == Some(false) {
        MarginType::Isolated
    } else {
        MarginType::Cross
    }
}
fn mode(value: &str) -> bool {
    matches!(
        value.trim_matches('"'),
        "1" | "HEDGE_MODE" | "HEDGE" | "DOUBLE_HOLD"
    )
}
impl PositionRow<'_> {
    fn position(&self, store: &Store) -> Position {
        let symbol = symbol(store, self.symbol);
        let side = if self.position_side == "LONG"
            || self.position_side != "SHORT" && self.current_qty.get() >= 0.0
        {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        let hedged = (!self.position_side.is_empty() && self.position_side != "BOTH")
            || mode(
                self.position_mode
                    .map(|r| r.get())
                    .unwrap_or(self.hold_mode),
            )
            || self.position_side.is_empty()
                && self.position_mode.is_none()
                && self.hold_mode.is_empty()
                && store.hedged();
        let contracts = base_amount(self.current_qty.get().abs(), contract_size(store, &symbol));
        let stamp = timestamp(if self.current_timestamp.get() > 0.0 {
            self.current_timestamp.get()
        } else {
            self.ts.get()
        });
        Position {
            id: if hedged {
                format!(
                    "{symbol}:{}",
                    if side == PositionSide::Long {
                        "long"
                    } else {
                        "short"
                    }
                )
            } else {
                symbol.clone()
            },
            symbol,
            side,
            entry_price: self.avg_entry_price.unwrap_or_default().get(),
            notional: (contracts * self.mark_price.unwrap_or_default().get()).abs(),
            leverage: self
                .leverage
                .map(Number::get)
                .filter(|n| *n > 0.0)
                .unwrap_or(1.0)
                .floor(),
            margin_type: Some(margin(self.margin_mode, self.cross_mode)),
            position_mode: Some(if hedged {
                PositionMode::Hedged
            } else {
                PositionMode::Oneway
            }),
            unrealized_pnl: self.unrealised_pnl.unwrap_or_default().get(),
            realized_pnl: None,
            contracts,
            liquidation_price: self.liquidation_price.unwrap_or_default().get(),
            margin_size: None,
            last_update: if stamp > 0.0 { Some(stamp) } else { None },
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    current_page: u32,
    total_page: u32,
    total_num: u32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BalanceRow {
    available_balance: Number,
    // An omitted secondary field reads as zero, as TS; equity and free stay required.
    #[serde(default)]
    order_margin: Number,
    account_equity: Number,
    #[serde(default, rename = "unrealisedPNL")]
    unrealised_pnl: Number,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metadata<'a> {
    #[serde(default)]
    pub(crate) margin_mode: &'a str,
    #[serde(default, borrow)]
    position_mode: Option<&'a RawValue>,
    #[serde(default)]
    pub(crate) leverage: Number,
}

impl Kucoin {
    pub(crate) fn account_revision(&self) -> u64 {
        self.account.revision
    }
    pub(crate) fn account_settings_unchanged_since(&self, revision: u64) -> bool {
        revision >= self.account.settings_revision
    }
    pub(crate) fn mode_known(&self) -> bool {
        self.account.mode_known
    }
    pub(crate) fn account_order_complete(&self, id: &str) -> bool {
        !self.account.incomplete_orders.contains(id)
    }

    pub(crate) fn account_placement(&self, id: &str) -> Option<&Order> {
        self.account.placed_orders.get(id)
    }

    pub(crate) fn account_order_filled(&self, id: &str) -> f64 {
        self.account.order_fills.get(id).copied().unwrap_or(0.0)
    }

    pub(crate) fn account_placed_order(&mut self, cx: &mut Cx<'_, Pending>, id: &str, body: &str) {
        if self.account.terminal.contains(id) {
            return;
        }
        let Ok(row) = serde_json::from_str::<OrderRow>(body) else {
            return;
        };
        let mut orders = row.orders(cx.store, cx.now);
        let mut order = orders.remove(0);
        order.stop_loss = orders
            .iter()
            .find(|leg| leg.kind == OrderType::StopLoss)
            .map(|leg| leg.price);
        order.take_profit = orders
            .iter()
            .find(|leg| leg.kind == OrderType::TakeProfit)
            .map(|leg| leg.price);
        order.id = id.into();
        order.close = Some(row.close_order.unwrap_or(false));
        if let Some(old) = cx.store.order(id) {
            let mut hydrated = old.clone();
            hydrated.reduce_only = order.reduce_only;
            hydrated.close = order.close;
            hydrated.time_in_force = order.time_in_force;
            hydrated.kind = order.kind;
            hydrated.client_order_id.clone_from(&order.client_order_id);
            hydrated.trigger_type = order.trigger_type;
            cx.store.upsert_order(hydrated);
        }
        self.account.incomplete_orders.remove(id);
        self.account.placed_orders.insert(id.into(), order);
    }

    pub(crate) fn account_order_terminal(&mut self, cx: &mut Cx<'_, Pending>, parent: &str) {
        self.account.revision += 1;
        for suffix in ["", "__stop_loss", "__take_profit"] {
            let id = format!("{parent}{suffix}");
            self.account
                .order_versions
                .insert(id.clone(), self.account.revision);
            self.account.terminal.insert(id.clone());
            self.account.incomplete_orders.remove(&id);
            self.account.placed_orders.remove(&id);
            cx.store.remove_orders(&[&id]);
        }
    }

    pub(crate) fn account_settings_changed(&mut self) {
        self.account.revision += 1;
        self.account.settings_revision = self.account.revision;
    }
    fn account_issue(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let path = request.path(cx.store);
        match request.read {
            Read::Positions => {
                self.signed_streamed(
                    cx,
                    Method::Get,
                    &path,
                    None,
                    ArrayAt::EnvelopeField("data"),
                    Pending::Account(request),
                );
            }
            Read::Orders(_) => {
                self.signed_streamed(
                    cx,
                    Method::Get,
                    &path,
                    None,
                    ArrayAt::Path(&["data", "items"]),
                    Pending::Account(request),
                );
            }
            _ => {
                self.signed(cx, Method::Get, &path, None, Pending::Account(request));
            }
        }
    }
    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.busy != 0 {
            return;
        }
        if let Some(timer) = self.account.timer.take() {
            cx.clear_timer(timer);
        }
        let mut reads = vec![Read::Positions, Read::Orders(false), Read::Orders(true)];
        if self.account.count.is_multiple_of(4) {
            reads.push(Read::Balance);
        }
        if !self.account.mode_known {
            reads.push(Read::Mode);
        }
        self.account.count += 1;
        self.account.busy = reads.len();
        for read in reads {
            self.account_issue(cx, Request::new(read, self.account.revision));
        }
    }
    pub(crate) fn account_metadata_ready(&self, cx: &Cx<'_, Pending>, symbol: &str) -> bool {
        self.account.mode_known
            && cx.store.setting(symbol).is_some_and(|s| {
                s.margin_type.is_some() && s.leverage.is_finite() && s.leverage > 0.0
            })
    }
    pub(crate) fn account_metadata(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if self.account_metadata_ready(cx, symbol) || !self.account.metadata.insert(symbol.into()) {
            return;
        }
        self.account.busy += 1;
        self.account_issue(
            cx,
            Request::new(Read::Margin(symbol.into()), self.account.revision),
        );
        if !self.account.mode_known {
            self.account.busy += 1;
            self.account_issue(cx, Request::new(Read::Mode, self.account.revision));
        }
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request.read {
            Read::Positions => match serde_json::from_str::<PositionRow>(item) {
                Ok(row) => {
                    let unified = symbol(cx.store, row.symbol);
                    if row.current_qty.get() == 0.0 || cx.store.market(&unified).is_some() {
                        request.positions.push(row.position(cx.store));
                    } else if self.malformed_markets.contains(&unified) {
                        request.invalid = true;
                        cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "KuCoin owned position market unavailable; preserving previous snapshot");
                    } else {
                        cx.notice(
                            Level::Warn,
                            code::ROW_SKIPPED,
                            format!(
                                "Skipping KuCoin position on uncatalogued market {}",
                                row.symbol
                            ),
                        );
                    }
                }
                Err(_) => match row_identity(item, "symbol") {
                    Some(symbol) => {
                        // Stored positions carry the unified symbol, not the contract id.
                        let unified = self::symbol(cx.store, &symbol);
                        let stored = cx.store.positions().iter().any(|p| p.symbol == unified);
                        let message = format!("KuCoin skipped unmappable position {symbol}");
                        self.log_position_skip(cx, unified.clone(), stored, message);
                        request.skipped_symbols.insert(unified);
                    }
                    None => {
                        let stored = !cx.store.positions().is_empty();
                        let message = "KuCoin skipped position row without a symbol";
                        self.log_position_skip(cx, String::new(), stored, message);
                        request.unidentified = true;
                    }
                },
            },
            Read::Orders(_) => match serde_json::from_str::<&RawValue>(item)
                .map_err(|e| e.to_string())
                .and_then(|r| map_order(cx.store, r, cx.now))
            {
                Ok(orders) => {
                    let uncatalogued = orders
                        .iter()
                        .find(|o| {
                            o.status == OrderStatus::Open && cx.store.market(&o.symbol).is_none()
                        })
                        .map(|o| o.symbol.clone());
                    if !request.native_ids.insert(orders[0].id.clone()) {
                        request.invalid = true;
                    } else if let Some(symbol) = uncatalogued {
                        // Without catalogue sizing, a malformed market's order withholds the
                        // snapshot; one on a filtered market is skipped but still counted.
                        if self.malformed_markets.contains(&symbol) {
                            request.invalid = true;
                            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "KuCoin working order market unavailable; preserving previous orders");
                        } else {
                            cx.notice(
                                Level::Warn,
                                code::ROW_SKIPPED,
                                format!("Skipping KuCoin order on uncatalogued market {symbol}"),
                            );
                        }
                    } else {
                        request.orders.extend(orders);
                    }
                }
                Err(error) => match row_identity(item, "id") {
                    // Still counted toward the page census so the snapshot can commit.
                    Some(id) if request.native_ids.insert(id.clone()) => {
                        cx.notice(
                            Level::Warn,
                            code::ROW_SKIPPED,
                            format!("KuCoin skipped unmappable order {id}: {error}"),
                        );
                        // The row also carried any attached TP/SL legs; keep them too.
                        for suffix in ["__stop_loss", "__take_profit"] {
                            request.skipped_orders.insert(format!("{id}{suffix}"));
                        }
                        request.skipped_orders.insert(id);
                    }
                    _ => request.invalid = true,
                },
            },
            _ => request.invalid = true,
        }
    }
    /// Logs a rejected position row once per symbol until it maps again; at Error when no
    /// stored copy is kept, as the row then commits as flat.
    fn log_position_skip(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        stored: bool,
        message: impl Into<String>,
    ) {
        let skips = &mut self.account.position_skips;
        if skips.len() >= SKIP_LOG_LIMIT {
            skips.clear();
        }
        if skips.insert(symbol) {
            let level = if stored { Level::Warn } else { Level::Error };
            cx.notice(level, code::ROW_SKIPPED, message);
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        status: u16,
        body: &str,
    ) {
        let envelope = serde_json::from_str::<Envelope>(body);
        let good = (200..300).contains(&status)
            && !request.invalid
            && envelope.as_ref().is_ok_and(|e| e.code == "200000");
        let rejected_signature = envelope.as_ref().is_ok_and(|e| e.code == "400005");
        if !good {
            if request.retries < if rejected_signature { 1 } else { 2 } {
                request.retries += 1;
                request.invalid = false;
                // A failed page cannot leave rows staged from that page.
                request.skipped_orders.clear();
                request.skipped_symbols.clear();
                request.unidentified = false;
                if matches!(request.read, Read::Positions) {
                    request.positions.clear();
                } else if matches!(request.read, Read::Orders(_)) {
                    request.orders.clear();
                    request.native_ids.clear();
                    request.page = 1;
                }
                self.account_issue(cx, request);
                return;
            }
            cx.retried_failure(
                "kucoin.account:773",
                format!("KuCoin private read failed: {}", request.path(cx.store)),
            );
            if let Read::Margin(symbol) | Read::Leverage(symbol) = &request.read {
                self.account.metadata.remove(symbol);
            }
            self.account_finished(cx);
            return;
        }
        let envelope = envelope.unwrap();
        if request.revision < self.account.settings_revision {
            match &request.read {
                Read::Mode => {
                    if !self.account.mode_known {
                        self.account_issue(cx, Request::new(Read::Mode, self.account.revision));
                        return;
                    }
                }
                Read::Margin(symbol) | Read::Leverage(symbol) => {
                    let symbol = symbol.clone();
                    self.account.metadata.remove(&symbol);
                    self.account_finished(cx);
                    self.account_metadata(cx, &symbol);
                    return;
                }
                _ => {}
            }
        }
        match request.read.clone() {
            Read::Positions => {
                let mut positions = Vec::new();
                for mut row in request.positions {
                    let version = self
                        .account
                        .position_versions
                        .get(&row.id)
                        .copied()
                        .unwrap_or(0);
                    if version > request.revision {
                        continue;
                    }
                    if let Some(existing) = cx.store.position(&row.id)
                        && row.last_update.unwrap_or(0.0) < existing.last_update.unwrap_or(0.0)
                    {
                        row = existing.clone();
                    }
                    if self
                        .account
                        .closures
                        .get(&row.id)
                        .is_some_and(|stamp| row.last_update.unwrap_or(0.0) <= *stamp)
                    {
                        continue;
                    }
                    if request.revision >= self.account.settings_revision {
                        cx.store.update_setting(&row.symbol, |s| {
                            s.leverage = row.leverage;
                            s.margin_type = row.margin_type;
                        });
                        self.account.mode_known = true;
                        cx.store
                            .set_hedged(row.position_mode == Some(PositionMode::Hedged));
                    }
                    if row.contracts != 0.0 {
                        positions.push(row);
                    } else {
                        self.account
                            .closures
                            .insert(row.id, row.last_update.unwrap_or(cx.now));
                    }
                }
                for old in cx.store.positions() {
                    if self
                        .account
                        .position_versions
                        .get(&old.id)
                        .copied()
                        .unwrap_or(0)
                        > request.revision
                        || request.unidentified
                        || request.skipped_symbols.contains(&old.symbol)
                    {
                        if !positions.iter().any(|p| p.id == old.id) {
                            positions.push(old.clone());
                        }
                    } else if !positions.iter().any(|p| p.id == old.id) {
                        self.account
                            .closures
                            .insert(old.id.clone(), old.last_update.unwrap_or(cx.now));
                    }
                }
                // A skip log re-arms once its row stops failing.
                self.account.position_skips.retain(|symbol| {
                    if symbol.is_empty() {
                        request.unidentified
                    } else {
                        request.skipped_symbols.contains(symbol)
                    }
                });
                cx.store.set_positions(positions);
            }
            Read::Orders(stop) => {
                let page = envelope
                    .data
                    .and_then(|r| serde_json::from_str::<Page>(r.get()).ok());
                let Some(page) = page else {
                    cx.notice(
                        Level::Error,
                        code::SNAPSHOT_DISCARDED,
                        "KuCoin order page missing metadata",
                    );
                    self.account_finished(cx);
                    return;
                };
                if page.current_page != request.page {
                    cx.notice(
                        Level::Error,
                        code::SNAPSHOT_DISCARDED,
                        "KuCoin order page mismatch",
                    );
                    self.account_finished(cx);
                    return;
                }
                if request.page < page.total_page {
                    if request.page >= 10000 {
                        cx.notice(
                            Level::Error,
                            code::SNAPSHOT_DISCARDED,
                            "KuCoin order page limit",
                        );
                        self.account_finished(cx);
                        return;
                    }
                    request.page += 1;
                    request.retries = 0;
                    self.account_issue(cx, request);
                    return;
                }
                if page.total_num as usize != request.native_ids.len()
                    || page.total_page == 0 && !request.orders.is_empty()
                {
                    cx.notice(
                        Level::Error,
                        code::SNAPSHOT_DISCARDED,
                        "KuCoin incomplete order snapshot",
                    );
                    self.account_finished(cx);
                    return;
                }
                for old in cx.store.orders() {
                    if self
                        .account
                        .order_books
                        .get(&old.id)
                        .copied()
                        .unwrap_or(matches!(
                            old.kind,
                            OrderType::StopLoss
                                | OrderType::TakeProfit
                                | OrderType::TrailingStopLoss
                        ))
                        == stop
                        && self
                            .account
                            .order_versions
                            .get(&old.id)
                            .copied()
                            .unwrap_or(0)
                            <= request.revision
                        && !request.orders.iter().any(|row| row.id == old.id)
                        && !request.skipped_orders.contains(&old.id)
                    {
                        self.account.revision += 1;
                        self.account
                            .order_absences
                            .insert(old.id.clone(), self.account.revision);
                    }
                }
                let mut orders: Vec<Order> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        self.account
                            .order_books
                            .get(&o.id)
                            .copied()
                            .unwrap_or(matches!(
                                o.kind,
                                OrderType::StopLoss
                                    | OrderType::TakeProfit
                                    | OrderType::TrailingStopLoss
                            ))
                            != stop
                            || self.account.order_versions.get(&o.id).copied().unwrap_or(0)
                                > request.revision
                            || request.skipped_orders.contains(&o.id)
                    })
                    .cloned()
                    .collect();
                for mut row in request.orders {
                    if let Some(filled) = row
                        .extra
                        .as_ref()
                        .and_then(|extra| extra.filled_in_placement_units)
                    {
                        let watermark = self.account.order_fills.entry(row.id.clone()).or_default();
                        *watermark = watermark.max(filled);
                    }
                    if self.account.terminal.contains(&row.id)
                        || self
                            .account
                            .order_absences
                            .get(&row.id)
                            .is_some_and(|revision| *revision > request.revision)
                        || self
                            .account
                            .order_versions
                            .get(&row.id)
                            .copied()
                            .unwrap_or(0)
                            > request.revision
                    {
                        continue;
                    }
                    if let Some(old) = cx.store.order(&row.id) {
                        if old.last_update > row.last_update {
                            let mut newer = old.clone();
                            newer.reduce_only = row.reduce_only;
                            newer.close = row.close;
                            newer.time_in_force = row.time_in_force;
                            newer.client_order_id = row.client_order_id;
                            row = newer;
                        } else if old.remaining < row.remaining {
                            row.remaining = old.remaining;
                            if let (Some(a), Some(b)) = (&mut row.extra, &old.extra) {
                                a.remaining_native = b.remaining_native;
                            }
                        }
                    }
                    self.account.order_books.insert(row.id.clone(), stop);
                    self.account.incomplete_orders.remove(&row.id);
                    if row.status == OrderStatus::Open && !orders.iter().any(|o| o.id == row.id) {
                        orders.push(row);
                    }
                }
                self.account.initial_order_books |= if stop { 2 } else { 1 };
                if cx.store.loaded().orders || self.account.initial_order_books == 3 {
                    cx.store.set_orders(orders);
                } else {
                    // Publish the valid book without claiming the other book was read.
                    let removed: Vec<_> = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|old| !orders.iter().any(|order| order.id == old.id))
                        .map(|old| old.id.clone())
                        .collect();
                    cx.store
                        .remove_orders(&removed.iter().map(String::as_str).collect::<Vec<_>>());
                    for order in orders {
                        cx.store.upsert_order(order);
                    }
                }
            }
            Read::Balance => {
                if let Some(row) = envelope
                    .data
                    .and_then(|r| serde_json::from_str::<BalanceRow>(r.get()).ok())
                {
                    cx.store.set_balance(Balance {
                        free: row.available_balance.get(),
                        used: row.order_margin.get(),
                        total: row.account_equity.get(),
                        upnl: row.unrealised_pnl.get(),
                    });
                }
            }
            Read::Mode => {
                if let Some(row) = envelope
                    .data
                    .and_then(|r| serde_json::from_str::<Metadata>(r.get()).ok())
                    && let Some(value) = row.position_mode
                    && request.revision >= self.account.settings_revision
                {
                    self.account.mode_known = true;
                    cx.store.set_hedged(mode(value.get()));
                }
            }
            Read::Margin(symbol) => {
                let Some(row) = envelope
                    .data
                    .and_then(|r| serde_json::from_str::<Metadata>(r.get()).ok())
                else {
                    self.account.metadata.remove(&symbol);
                    self.account_finished(cx);
                    return;
                };
                if !matches!(row.margin_mode, "CROSS" | "ISOLATED") {
                    self.account.metadata.remove(&symbol);
                    self.account_finished(cx);
                    return;
                }
                let margin_type = margin(row.margin_mode, None);
                cx.store.update_setting(&symbol, |s| {
                    s.margin_type = Some(margin_type);
                    if margin_type == MarginType::Isolated {
                        s.leverage = if s.leverage.is_finite() && s.leverage > 0.0 {
                            s.leverage
                        } else {
                            1.0
                        };
                    }
                });
                if margin_type == MarginType::Cross {
                    self.account_issue(
                        cx,
                        Request::new(Read::Leverage(symbol), self.account.revision),
                    );
                    return;
                }
                self.account.metadata.remove(&symbol);
            }
            Read::Leverage(symbol) => {
                if let Some(row) = envelope
                    .data
                    .and_then(|r| serde_json::from_str::<Metadata>(r.get()).ok())
                    && row.leverage.get() > 0.0
                    && request.revision >= self.account.settings_revision
                {
                    cx.store
                        .update_setting(&symbol, |s| s.leverage = row.leverage.get());
                }
                self.account.metadata.remove(&symbol);
            }
        }
        self.account_finished(cx);
    }
    fn account_finished(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.busy = self.account.busy.saturating_sub(1);
        if self.account.busy == 0 && self.account.timer.is_none() {
            self.account.timer = Some(cx.set_poll_timer(500.0));
        }
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.timer == Some(timer) {
            self.account.timer = None;
            self.account_start(cx);
            return true;
        }
        if self.account.refresh == Some(timer) {
            self.account.refresh = None;
            if self.account.busy == 0 {
                self.account_start(cx);
            }
            return true;
        }
        false
    }
    pub(crate) fn account_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        topic: &str,
        data: &RawValue,
    ) {
        if topic.starts_with("/contractMarket/tradeOrders")
            || topic.starts_with("/contractMarket/advancedOrders")
        {
            let Ok(row) = serde_json::from_str::<OrderRow>(data.get()) else {
                return;
            };
            if cx.store.market(&symbol(cx.store, row.symbol)).is_none() {
                cx.notice(
                    Level::Warn,
                    code::SNAPSHOT_DISCARDED,
                    "KuCoin working order market unavailable; preserving previous orders",
                );
                return;
            }
            let orders = row.orders(cx.store, cx.now);
            if orders[0].id.is_empty() {
                return;
            }
            let watermark = self
                .account
                .order_fills
                .entry(orders[0].id.clone())
                .or_default();
            *watermark = watermark.max(row.filled_size.get());
            // Like TS, a match without matchPrice executes at the order price.
            let match_price = if row.match_price.get() > 0.0 {
                row.match_price.get()
            } else {
                row.price.get()
            };
            if row.match_size.get() > 0.0 && match_price > 0.0 {
                let key = if row.trade_id.is_empty() {
                    #[derive(Deserialize)]
                    struct Stamp<'a> {
                        #[serde(default, borrow)]
                        ts: Option<&'a RawValue>,
                    }
                    let stamp = serde_json::from_str::<Stamp>(data.get())
                        .ok()
                        .and_then(|s| s.ts);
                    stamp.map(|s| {
                        format!(
                            "{}:{}:{}:{}:{}",
                            orders[0].id,
                            s.get().trim_matches('"'),
                            row.filled_size.get(),
                            row.match_size.get(),
                            match_price
                        )
                    })
                } else {
                    Some(format!("{}:{}", orders[0].id, row.trade_id))
                };
                if let Some(key) = key
                    && self.account.executions.insert(key.clone())
                {
                    self.account.execution_queue.push_back(key);
                    if self.account.execution_queue.len() > 4096
                        && let Some(old) = self.account.execution_queue.pop_front()
                    {
                        self.account.executions.remove(&old);
                    }
                    cx.fill(Fill {
                        id: None,
                        order_id: orders[0].id.clone(),
                        symbol: orders[0].symbol.clone(),
                        side: orders[0].side,
                        price: match_price,
                        amount: base_amount(
                            row.match_size.get(),
                            contract_size(cx.store, &orders[0].symbol),
                        ),
                    });
                }
            }
            self.account.revision += 1;
            if matches!(
                orders[0].status,
                OrderStatus::Canceled | OrderStatus::Closed
            ) {
                self.account_order_terminal(cx, &orders[0].id);
                return;
            }
            let parent_id = orders[0].id.clone();
            let incomplete = !row.complete()
                && (self.account.incomplete_orders.contains(&parent_id)
                    || cx.store.order(&parent_id).is_none())
                && !self.account.placed_orders.contains_key(&parent_id);
            if incomplete {
                self.account.incomplete_orders.insert(parent_id.clone());
                if let Some(timer) = self.account.refresh.take() {
                    cx.clear_timer(timer);
                }
                self.account.refresh = Some(cx.set_timer(500.0));
            } else {
                self.account.incomplete_orders.remove(&parent_id);
            }
            for mut order in orders {
                self.account
                    .order_versions
                    .insert(order.id.clone(), self.account.revision);
                self.account.order_books.insert(
                    order.id.clone(),
                    topic.starts_with("/contractMarket/advancedOrders"),
                );
                if matches!(order.status, OrderStatus::Canceled | OrderStatus::Closed) {
                    self.account.terminal.insert(order.id.clone());
                    cx.store.remove_orders(&[&order.id]);
                } else if !self.account.terminal.contains(&order.id)
                    && cx
                        .store
                        .order(&order.id)
                        .is_none_or(|old| old.last_update <= order.last_update)
                {
                    if let Some(old) = cx
                        .store
                        .order(&order.id)
                        .or_else(|| self.account.placed_orders.get(&order.id))
                    {
                        row.merge(&mut order, old);
                    }
                    cx.store.upsert_order(order);
                }
            }
            return;
        }
        if topic == "/contract/marginMode" {
            if let Ok(rows) = serde_json::from_str::<HashMap<&str, &str>>(data.get()) {
                for (id, value) in rows {
                    if !matches!(value, "CROSS" | "ISOLATED") {
                        continue;
                    }
                    let symbol = symbol(cx.store, id);
                    self.account_settings_changed();
                    let margin_type = margin(value, None);
                    cx.store.update_setting(&symbol, |s| {
                        if s.margin_type != Some(margin_type) {
                            s.leverage = f64::NAN;
                        }
                        s.margin_type = Some(margin_type);
                    });
                }
            }
        } else if topic == "/contract/crossLeverage"
            && let Ok(rows) = serde_json::from_str::<HashMap<&str, Metadata>>(data.get())
        {
            for (id, value) in rows {
                if value.leverage.get() <= 0.0 {
                    continue;
                }
                let symbol = symbol(cx.store, id);
                self.account_settings_changed();
                cx.store
                    .update_setting(&symbol, |s| s.leverage = value.leverage.get());
            }
        }
        if topic.starts_with("/contract/position")
            && let Ok(row) = serde_json::from_str::<PositionRow>(data.get())
        {
            if row.current_qty.get() != 0.0
                && cx.store.market(&symbol(cx.store, row.symbol)).is_none()
            {
                cx.notice(
                    Level::Warn,
                    code::SNAPSHOT_DISCARDED,
                    "KuCoin owned position market unavailable; preserving previous snapshot",
                );
                return;
            }
            if cx.store.hedged() && row.position_side.is_empty() {
                if let Some(timer) = self.account.refresh.take() {
                    cx.clear_timer(timer);
                }
                self.account.refresh = Some(cx.set_timer(500.0));
                return;
            }
            let mut position = row.position(cx.store);
            if let Some(old) = cx.store.position(&position.id) {
                if row.avg_entry_price.is_none() {
                    position.entry_price = old.entry_price;
                }
                if row.mark_price.is_none() {
                    position.notional = old.notional / old.contracts * position.contracts;
                }
                if row.leverage.is_none() {
                    position.leverage = old.leverage;
                }
                if row.unrealised_pnl.is_none() {
                    position.unrealized_pnl = old.unrealized_pnl;
                }
                if row.liquidation_price.is_none() {
                    position.liquidation_price = old.liquidation_price;
                }
                if row.margin_mode.is_empty() && row.cross_mode.is_none() {
                    position.margin_type = old.margin_type;
                }
                if row.position_side.is_empty()
                    && row.position_mode.is_none()
                    && row.hold_mode.is_empty()
                {
                    position.position_mode = old.position_mode;
                }
            } else if position.contracts != 0.0
                && (row.avg_entry_price.is_none()
                    || row.mark_price.is_none()
                    || row.leverage.is_none()
                    || row.liquidation_price.is_none())
            {
                if let Some(timer) = self.account.refresh.take() {
                    cx.clear_timer(timer);
                }
                self.account.refresh = Some(cx.set_timer(500.0));
                return;
            }
            let stale = cx.store.position(&position.id).is_some_and(|old| {
                old.last_update.unwrap_or(0.0) > position.last_update.unwrap_or(cx.now)
            }) || position.contracts != 0.0
                && self
                    .account
                    .closures
                    .get(&position.id)
                    .is_some_and(|stamp| position.last_update.unwrap_or(0.0) <= *stamp);
            if stale {
                return;
            }
            self.account.revision += 1;
            self.account
                .position_versions
                .insert(position.id.clone(), self.account.revision);
            self.account.settings_revision = self.account.revision;
            cx.store.update_setting(&position.symbol, |s| {
                if row.leverage.is_some() {
                    s.leverage = position.leverage;
                }
                if !row.margin_mode.is_empty() || row.cross_mode.is_some() {
                    s.margin_type = position.margin_type;
                }
            });
            if !row.position_side.is_empty()
                || row.position_mode.is_some()
                || !row.hold_mode.is_empty()
            {
                cx.store
                    .set_hedged(position.position_mode == Some(PositionMode::Hedged));
                self.account.mode_known = true;
            }
            if position.contracts == 0.0 {
                self.account
                    .closures
                    .insert(position.id.clone(), position.last_update.unwrap_or(cx.now));
            }
            if cx.store.position(&position.id).is_none_or(|old| {
                old.last_update.unwrap_or(0.0) <= position.last_update.unwrap_or(cx.now)
            }) {
                cx.store.upsert_position(position);
            }
        }
        if let Some(timer) = self.account.refresh.take() {
            cx.clear_timer(timer);
        }
        self.account.refresh = Some(cx.set_timer(500.0));
    }
}
/// The identity field of a REST row that failed to map.
fn row_identity(item: &str, field: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Row<'a> {
        #[serde(borrow, default)]
        id: Option<std::borrow::Cow<'a, str>>,
        #[serde(borrow, default)]
        symbol: Option<std::borrow::Cow<'a, str>>,
    }
    let row: Row<'_> = serde_json::from_str(item).ok()?;
    let value = if field == "id" { row.id } else { row.symbol }?;
    (!value.is_empty()).then(|| value.into_owned())
}
