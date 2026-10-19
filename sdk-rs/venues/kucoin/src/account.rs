use crate::wire::{Envelope, Number};
use crate::{Kucoin, Pending};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Store, TimerId};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet, VecDeque};

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
    order_books: HashMap<String, bool>,
    terminal: HashSet<String>,
    position_versions: HashMap<String, u64>,
    closures: HashMap<String, f64>,
    executions: HashSet<String>,
    execution_queue: VecDeque<String>,
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
    reduce_only: bool,
    #[serde(default)]
    close_order: bool,
    #[serde(default)]
    time_in_force: &'a str,
    #[serde(default)]
    post_only: bool,
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
            reduce_only: self.reduce_only,
            close: Some(self.close_order),
            client_order_id: self.client_oid.map(str::to_owned),
            time_in_force: Some(if self.post_only {
                TimeInForce::PostOnly
            } else {
                match self.time_in_force {
                    "IOC" => TimeInForce::ImmediateOrCancel,
                    "FOK" => TimeInForce::FillOrKill,
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
    order_margin: Number,
    account_equity: Number,
    #[serde(rename = "unrealisedPNL")]
    unrealised_pnl: Number,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata<'a> {
    #[serde(default)]
    margin_mode: &'a str,
    #[serde(default, borrow)]
    position_mode: Option<&'a RawValue>,
    #[serde(default)]
    leverage: Number,
}

impl Kucoin {
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
        self.account.timer = None;
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
                Ok(row) => request.positions.push(row.position(cx.store)),
                Err(_) => request.invalid = true,
            },
            Read::Orders(_) => match serde_json::from_str::<&RawValue>(item)
                .map_err(|e| e.to_string())
                .and_then(|r| map_order(cx.store, r, cx.now))
            {
                Ok(orders) => {
                    if request.native_ids.insert(orders[0].id.clone()) {
                        request.orders.extend(orders);
                    } else {
                        request.invalid = true;
                    }
                }
                Err(_) => request.invalid = true,
            },
            _ => request.invalid = true,
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
            cx.log(
                LogLevel::Error,
                format!("KuCoin private read failed: {}", request.path(cx.store)),
            );
            if let Read::Margin(symbol) | Read::Leverage(symbol) = &request.read {
                self.account.metadata.remove(symbol);
            }
            self.account_finished(cx);
            return;
        }
        let envelope = envelope.unwrap();
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
                cx.store.set_positions(positions);
            }
            Read::Orders(stop) => {
                let page = envelope
                    .data
                    .and_then(|r| serde_json::from_str::<Page>(r.get()).ok());
                let Some(page) = page else {
                    cx.log(LogLevel::Error, "KuCoin order page missing metadata");
                    self.account_finished(cx);
                    return;
                };
                if page.current_page != request.page {
                    cx.log(LogLevel::Error, "KuCoin order page mismatch");
                    self.account_finished(cx);
                    return;
                }
                if request.page < page.total_page {
                    if request.page >= 10000 {
                        cx.log(LogLevel::Error, "KuCoin order page limit");
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
                    cx.log(LogLevel::Error, "KuCoin incomplete order snapshot");
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
                    {
                        self.account.terminal.insert(old.id.clone());
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
                    })
                    .cloned()
                    .collect();
                for mut row in request.orders {
                    if self.account.terminal.contains(&row.id)
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
                            row = old.clone();
                        } else if old.remaining < row.remaining {
                            row.remaining = old.remaining;
                            if let (Some(a), Some(b)) = (&mut row.extra, &old.extra) {
                                a.remaining_native = b.remaining_native;
                            }
                        }
                    }
                    self.account.order_books.insert(row.id.clone(), stop);
                    if row.status == OrderStatus::Open && !orders.iter().any(|o| o.id == row.id) {
                        orders.push(row);
                    }
                }
                cx.store.set_orders(orders);
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
                if request.revision < self.account.settings_revision {
                    self.account.metadata.remove(&symbol);
                    self.account_finished(cx);
                    return;
                }
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
            self.account.timer = Some(cx.set_timer(500.0));
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
            let orders = row.orders(cx.store, cx.now);
            if orders[0].id.is_empty() {
                return;
            }
            if row.match_size.get() > 0.0 && row.match_price.get() > 0.0 {
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
                            row.match_price.get()
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
                        order_id: orders[0].id.clone(),
                        symbol: orders[0].symbol.clone(),
                        side: orders[0].side,
                        price: row.match_price.get(),
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
                for suffix in ["", "__stop_loss", "__take_profit"] {
                    let id = format!("{}{suffix}", orders[0].id);
                    self.account
                        .order_versions
                        .insert(id.clone(), self.account.revision);
                    self.account.terminal.insert(id.clone());
                    cx.store.remove_orders(&[&id]);
                }
                return;
            }
            for order in orders {
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
                    cx.store.upsert_order(order);
                }
            }
            return;
        }
        if topic.starts_with("/contract/position")
            && let Ok(row) = serde_json::from_str::<PositionRow>(data.get())
        {
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
