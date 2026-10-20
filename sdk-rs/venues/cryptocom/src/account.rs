use crate::sign::Params;
use crate::wire::{Envelope, Message};
use crate::{Cryptocom, Pending, Skipped, quoted};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, TimerId, num};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

#[derive(Clone, Copy)]
pub enum Read {
    Positions,
    Balance,
    Orders,
    AdvancedOrders,
}
impl Read {
    pub fn path(self) -> &'static str {
        match self {
            Self::Positions => "private/get-positions",
            Self::Balance => "private/user-balance",
            Self::Orders => "private/get-open-orders",
            Self::AdvancedOrders => "private/advanced/get-open-orders",
        }
    }
}

pub enum Rows {
    Positions(Vec<Position>),
    Balance(Option<(Balance, Option<Vec<CurrencyBalance>>)>),
    Orders(Vec<Order>),
}
pub struct Request {
    pub read: Read,
    pub revision: u64,
    pub rows: Rows,
    pub invalid: bool,
    pub skipped: Skipped,
    /// Identities of unmappable snapshot rows; their known state is kept.
    pub kept: Vec<String>,
    pub retries: u8,
}

#[derive(Default)]
pub struct Account {
    pub timer: Option<TimerId>,
    pub busy: bool,
    pub revision: u64,
    pub count: u32,
    pub order_rows: Option<Vec<Order>>,
    pub order_revision: u64,
    order_versions: HashMap<String, u64>,
    position_versions: HashMap<String, u64>,
    symbol_closures: HashMap<String, (u64, f64)>,
    position_times: HashMap<String, f64>,
    position_closures: HashMap<String, f64>,
    terminal: HashSet<String>,
    balance_revision: u64,
    executions: HashSet<String>,
    execution_queue: VecDeque<String>,
}

#[derive(Deserialize)]
pub struct OrderRow<'a> {
    #[serde(borrow)]
    pub order_id: &'a RawValue,
    pub instrument_name: &'a str,
    pub order_type: &'a str,
    pub side: &'a str,
    pub quantity: &'a str,
    #[serde(default)]
    pub client_oid: Option<&'a str>,
    #[serde(default)]
    pub limit_price: Option<&'a str>,
    #[serde(default)]
    pub ref_price: Option<&'a str>,
    #[serde(default)]
    pub trigger_price: Option<&'a str>,
    #[serde(default)]
    pub cumulative_quantity: Option<&'a str>,
    #[serde(default)]
    pub status: Option<&'a str>,
    #[serde(default)]
    pub order_status: Option<&'a str>,
    #[serde(default)]
    pub time_in_force: Option<&'a str>,
    #[serde(default)]
    pub exec_inst: Option<Vec<&'a str>>,
    #[serde(default)]
    pub update_time: Option<f64>,
    #[serde(default)]
    pub time_updated: Option<f64>,
    #[serde(default)]
    pub create_time: Option<f64>,
    #[serde(default)]
    pub time_created: Option<f64>,
}

pub fn id(raw: &RawValue) -> String {
    serde_json::from_str::<String>(raw.get()).unwrap_or_else(|_| raw.get().to_owned())
}
pub fn side(value: &str) -> OrderSide {
    if value == "BUY" {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    }
}
pub fn kind(value: &str) -> OrderType {
    match value {
        "MARKET" => OrderType::Market,
        "STOP_LOSS" | "STOP_LIMIT" => OrderType::StopLoss,
        "TAKE_PROFIT" | "TAKE_PROFIT_LIMIT" => OrderType::TakeProfit,
        _ => OrderType::Limit,
    }
}
pub fn status(value: &str) -> OrderStatus {
    match value {
        "FILLED" => OrderStatus::Closed,
        "CANCELED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
        _ => OrderStatus::Open,
    }
}
impl OrderRow<'_> {
    /// Null optional fields keep the `known` order's values rather than resetting them.
    pub fn order(&self, now: f64, known: Option<&Order>) -> Order {
        let amount = num::parse(self.quantity);
        let filled = self
            .cumulative_quantity
            .map_or_else(|| known.map_or(0.0, |o| o.filled), num::parse);
        let exec_inst = self.exec_inst.as_deref().unwrap_or_default();
        Order {
            id: id(self.order_id),
            parent_id: None,
            symbol: self.instrument_name.into(),
            status: status(self.status.or(self.order_status).unwrap_or("ACTIVE")),
            kind: kind(self.order_type),
            side: side(self.side),
            price: num::parse(
                self.ref_price
                    .or(self.trigger_price)
                    .or(self.limit_price)
                    .unwrap_or("0"),
            ),
            amount,
            filled,
            remaining: (amount - filled).max(0.0),
            reduce_only: exec_inst.contains(&"REDUCE_ONLY"),
            close: None,
            client_order_id: self.client_oid.map(str::to_owned),
            time_in_force: if exec_inst.contains(&"POST_ONLY") {
                Some(TimeInForce::PostOnly)
            } else {
                match self.time_in_force {
                    Some("IMMEDIATE_OR_CANCEL") => Some(TimeInForce::ImmediateOrCancel),
                    Some("FILL_OR_KILL") => Some(TimeInForce::FillOrKill),
                    Some("POST_ONLY") => Some(TimeInForce::PostOnly),
                    None if self.exec_inst.is_none() => known
                        .and_then(|o| o.time_in_force)
                        .or(Some(TimeInForce::GoodTillCancel)),
                    _ => Some(TimeInForce::GoodTillCancel),
                }
            },
            last_update: self
                .update_time
                .or(self.time_updated)
                .or(self.create_time)
                .or(self.time_created)
                .unwrap_or(now),
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }
    }
}

#[derive(Deserialize)]
struct PositionRow<'a> {
    instrument_name: &'a str,
    quantity: &'a str,
    cost: &'a str,
    #[serde(default)]
    open_position_pnl: Option<&'a str>,
    #[serde(default)]
    isolation_type: Option<&'a str>,
    #[serde(default)]
    leverage: Option<&'a str>,
    #[serde(default)]
    target_leverage: Option<&'a str>,
    #[serde(default)]
    liquidation_price: Option<&'a str>,
    #[serde(default)]
    update_timestamp_ms: Option<f64>,
}
impl PositionRow<'_> {
    fn position(&self, now: f64) -> Position {
        let qty = num::parse(self.quantity);
        let contracts = qty.abs();
        let side = if qty < 0.0 {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let cost = num::parse(self.cost);
        Position {
            id: format!(
                "{}:{}",
                self.instrument_name,
                if side == PositionSide::Long {
                    "long"
                } else {
                    "short"
                }
            ),
            symbol: self.instrument_name.into(),
            side,
            entry_price: if contracts > 0.0 {
                cost / contracts
            } else {
                0.0
            },
            notional: cost.abs(),
            leverage: num::parse(self.leverage.or(self.target_leverage).unwrap_or("1")),
            unrealized_pnl: self.open_position_pnl.map_or(0.0, num::parse),
            contracts,
            liquidation_price: self.liquidation_price.map_or(0.0, num::parse),
            margin_type: Some(if self.isolation_type == Some("ISOLATED_MARGIN") {
                MarginType::Isolated
            } else {
                MarginType::Cross
            }),
            position_mode: Some(PositionMode::Hedged),
            realized_pnl: None,
            margin_size: None,
            last_update: Some(self.update_timestamp_ms.unwrap_or(now)),
        }
    }
}

#[derive(Deserialize)]
struct BalanceRow<'a> {
    total_margin_balance: &'a str,
    total_available_balance: &'a str,
    total_initial_margin: &'a str,
    total_session_unrealized_pnl: &'a str,
    #[serde(borrow, default)]
    position_balances: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct CoinRow<'a> {
    instrument_name: &'a str,
    quantity: &'a str,
    max_withdrawal_balance: &'a str,
}
#[derive(Deserialize)]
struct CoinKey<'a> {
    #[serde(default)]
    instrument_name: Option<&'a str>,
}
fn finite(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|v| v.is_finite())
}
impl BalanceRow<'_> {
    fn balance(&self) -> Balance {
        Balance {
            total: num::parse(self.total_margin_balance),
            free: num::parse(self.total_available_balance),
            used: num::parse(self.total_initial_margin),
            upnl: num::parse(self.total_session_unrealized_pnl),
        }
    }
    /// Per-coin rows are parsed apart from the summary so a bad coin never costs the aggregate;
    /// `None` (unreadable list) keeps the previously published coins.
    fn currencies(&self, cx: &mut Cx<'_, Pending>) -> Option<Vec<CurrencyBalance>> {
        let mut coins = BTreeMap::new();
        let mut skipped = Skipped::default();
        if let Some(raw) = self.position_balances.filter(|raw| raw.get() != "null") {
            json::each::<&RawValue>(raw.get(), |item| {
                let coin = serde_json::from_str::<CoinRow>(item.get())
                    .ok()
                    .and_then(|row| {
                        let total = finite(row.quantity)?;
                        let free = finite(row.max_withdrawal_balance)?;
                        Some((row.instrument_name, total, free))
                    });
                match coin {
                    Some((currency, total, free)) => {
                        coins.insert(
                            currency.to_owned(),
                            Balance {
                                total,
                                free,
                                used: (total - free).max(0.0),
                                upnl: 0.0,
                            },
                        );
                    }
                    None => skipped.add(
                        serde_json::from_str::<CoinKey>(item.get())
                            .ok()
                            .and_then(|k| k.instrument_name),
                    ),
                }
            })
            .ok()?;
        }
        skipped.report(cx, "currency balance");
        Some(
            coins
                .into_iter()
                .map(|(currency, balance)| CurrencyBalance { currency, balance })
                .collect(),
        )
    }
}

#[derive(Deserialize)]
struct Execution<'a> {
    #[serde(borrow)]
    trade_id: &'a RawValue,
    #[serde(borrow)]
    order_id: &'a RawValue,
    instrument_name: &'a str,
    side: &'a str,
    traded_price: &'a str,
    traded_quantity: &'a str,
}

#[derive(Deserialize)]
struct RowKey<'a> {
    #[serde(borrow, default)]
    order_id: Option<&'a RawValue>,
    #[serde(borrow, default)]
    trade_id: Option<&'a RawValue>,
    #[serde(default)]
    instrument_name: Option<&'a str>,
}
fn row_key(raw: &str) -> Option<RowKey<'_>> {
    serde_json::from_str(raw).ok()
}
fn row_label(key: &RowKey<'_>) -> Option<String> {
    key.trade_id
        .or(key.order_id)
        .map(id)
        .or(key.instrument_name.map(str::to_owned))
}
// Rows for instruments outside the loaded markets are ignored whether or not they map.
fn foreign(cx: &Cx<'_, Pending>, key: Option<&RowKey<'_>>) -> bool {
    key.and_then(|k| k.instrument_name)
        .is_some_and(|symbol| cx.store.market(symbol).is_none())
}

/// Applies each mappable row of a pushed frame; bad rows are skipped with one summary per frame.
fn each_row<'a, T: Deserialize<'a>>(
    cx: &mut Cx<'_, Pending>,
    kind: &str,
    data: &'a RawValue,
    mut apply: impl FnMut(&mut Cx<'_, Pending>, T),
) -> serde_json::Result<()> {
    let mut skipped = Skipped::default();
    json::each::<&RawValue>(data.get(), |raw| {
        match serde_json::from_str::<T>(raw.get()) {
            Ok(row) => apply(cx, row),
            Err(_) => {
                let key = row_key(raw.get());
                if !foreign(cx, key.as_ref()) {
                    skipped.add(key.as_ref().and_then(row_label).as_deref());
                }
            }
        }
    })?;
    skipped.report(cx, kind);
    Ok(())
}

impl Cryptocom {
    pub(crate) fn private_start(&mut self, cx: &mut Cx<'_, Pending>) {
        cx.store.set_hedged(true);
        for symbol in cx
            .store
            .markets()
            .iter()
            .map(|m| m.symbol.clone())
            .collect::<Vec<_>>()
        {
            cx.store.update_setting(&symbol, |s| {
                s.leverage = 1.0;
                s.margin_type = Some(MarginType::Cross);
            });
        }
        self.connect(cx, 1);
        self.reconcile(cx);
    }
    pub(crate) fn signed(
        &self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        params: &Params,
        pending: Pending,
        streamed: bool,
    ) {
        let Some(credentials) = &self.credentials else {
            return;
        };
        let body = credentials.envelope(path, params, cx.now, false);
        let request = cx.request(
            Method::Post,
            format!("{}/{path}", self.rest),
            vec![("Content-Type", "application/json".into())],
            Some(body),
            pending,
        );
        if streamed {
            cx.stream_response(request, ArrayAt::Path(&["result", "data"]));
        }
    }
    pub(crate) fn reconcile(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.busy {
            return;
        }
        self.account.busy = true;
        self.read_account(cx, Read::Positions, 0);
    }
    fn read_account(&self, cx: &mut Cx<'_, Pending>, read: Read, retries: u8) {
        let rows = match read {
            Read::Positions => Rows::Positions(vec![]),
            Read::Balance => Rows::Balance(None),
            _ => Rows::Orders(vec![]),
        };
        self.signed(
            cx,
            read.path(),
            &Params::default(),
            Pending::Account(Request {
                read,
                revision: self.account.revision,
                rows,
                invalid: false,
                skipped: Skipped::default(),
                kept: vec![],
                retries,
            }),
            true,
        );
    }
    fn schedule_reconcile(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.busy = false;
        if let Some(timer) = self.account.timer.take() {
            cx.clear_timer(timer);
        }
        self.account.timer = Some(cx.set_timer(10_000.0));
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
        transport: bool,
    ) {
        if (error.contains("40101") && request.retries < 1)
            || (transport && !error.contains("40101") && request.retries < 3)
        {
            self.read_account(cx, request.read, request.retries + 1);
            return;
        }
        self.account.order_rows = None;
        cx.log(LogLevel::Error, error);
        self.schedule_reconcile(cx);
    }
    pub(crate) fn account_item(&self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        let result = match &mut request.rows {
            Rows::Positions(rows) => serde_json::from_str::<PositionRow>(item).map(|row| {
                if cx.store.market(row.instrument_name).is_some() {
                    rows.push(row.position(cx.now));
                }
            }),
            Rows::Orders(rows) => serde_json::from_str::<OrderRow>(item).map(|row| {
                if cx.store.market(row.instrument_name).is_some() {
                    rows.push(row.order(cx.now, cx.store.order(&id(row.order_id))));
                }
            }),
            Rows::Balance(balance) => serde_json::from_str::<BalanceRow>(item).map(|row| {
                if balance.is_none() {
                    *balance = Some((row.balance(), row.currencies(cx)));
                }
            }),
        };
        if result.is_ok() {
            return;
        }
        let key = row_key(item);
        if foreign(cx, key.as_ref()) {
            return;
        }
        // A row without its identity cannot be kept, so the read fails as a whole.
        let identity = match &request.rows {
            Rows::Positions(_) => key.and_then(|k| k.instrument_name.map(str::to_owned)),
            Rows::Orders(_) => key.and_then(|k| k.order_id.map(id)),
            Rows::Balance(Some(_)) => {
                request.skipped.add(None);
                return;
            }
            Rows::Balance(None) => None,
        };
        let Some(identity) = identity else {
            request.invalid = true;
            return;
        };
        request.skipped.add(Some(&identity));
        request.kept.push(identity);
    }
    pub(crate) fn account_response(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        if request.invalid {
            self.account_failed(cx, request, "invalid account row", false);
            return;
        }
        let fresh_balance = request.revision >= self.account.balance_revision;
        request.skipped.report(
            cx,
            match request.rows {
                Rows::Positions(_) => "position",
                Rows::Balance(_) => "balance",
                Rows::Orders(_) => "order",
            },
        );
        let skipped = request.kept;
        match request.rows {
            Rows::Positions(rows) => {
                let mut rows = rows;
                rows.extend(
                    cx.store
                        .positions()
                        .iter()
                        .filter(|p| {
                            skipped.contains(&p.symbol) && !rows.iter().any(|row| row.id == p.id)
                        })
                        .cloned()
                        .collect::<Vec<_>>(),
                );
                // Resolve cached rows before applying closure fences: a newer live
                // reopen must survive an old snapshot row from before the closure.
                for row in &mut rows {
                    if let Some(live) = cx.store.position(&row.id)
                        && live.last_update > row.last_update
                    {
                        *row = live.clone();
                    }
                }
                rows.retain(|p| {
                    if self
                        .account
                        .position_closures
                        .get(&p.id)
                        .is_some_and(|time| p.last_update.unwrap_or(0.0) <= *time)
                    {
                        return false;
                    }
                    if self
                        .account
                        .symbol_closures
                        .get(&p.symbol)
                        .is_some_and(|(_, time)| p.last_update.unwrap_or(0.0) <= *time)
                    {
                        return false;
                    }
                    self.account
                        .position_versions
                        .get(&p.id)
                        .is_none_or(|v| *v <= request.revision)
                        && self
                            .account
                            .symbol_closures
                            .get(&p.symbol)
                            .is_none_or(|(v, _)| *v <= request.revision)
                });
                rows.extend(
                    cx.store
                        .positions()
                        .iter()
                        .filter(|p| {
                            self.account
                                .position_versions
                                .get(&p.id)
                                .is_some_and(|v| *v > request.revision)
                        })
                        .cloned(),
                );
                for row in &mut rows {
                    // Recovery rows omit live leverage/liquidation fields.
                    if let Some(live) = cx.store.position(&row.id) {
                        if live.last_update > row.last_update {
                            *row = live.clone();
                        }
                        if row.leverage == 1.0 {
                            row.leverage = live.leverage;
                        }
                        if row.liquidation_price == 0.0 {
                            row.liquidation_price = live.liquidation_price;
                        }
                    }
                    self.account
                        .position_times
                        .entry(row.id.clone())
                        .and_modify(|t| *t = t.max(row.last_update.unwrap_or(0.0)))
                        .or_insert(row.last_update.unwrap_or(0.0));
                    cx.store.update_setting(&row.symbol, |s| {
                        s.leverage = row.leverage;
                        s.margin_type = row.margin_type;
                    });
                }
                for live in cx.store.positions() {
                    if !rows.iter().any(|p| p.id == live.id && p.contracts > 0.0) {
                        let time = self
                            .account
                            .position_times
                            .get(&live.id)
                            .copied()
                            .unwrap_or(live.last_update.unwrap_or(0.0));
                        self.account
                            .position_closures
                            .entry(live.id.clone())
                            .and_modify(|t| *t = t.max(time))
                            .or_insert(time);
                    }
                }
                cx.store
                    .set_positions(rows.into_iter().filter(|p| p.contracts > 0.0).collect());
                self.read_account(cx, Read::Balance, 0);
            }
            Rows::Balance(balance) => {
                if let Some((mut balance, currencies)) = balance {
                    if fresh_balance {
                        balance.upnl = cx.store.positions().iter().map(|p| p.unrealized_pnl).sum();
                        cx.store.set_balance(balance);
                        if let Some(currencies) = currencies {
                            cx.store.set_currency_balances(currencies);
                        }
                    }
                } else {
                    cx.log(LogLevel::Error, "missing balance summary");
                }
                if self.account.count.is_multiple_of(4) {
                    self.account.order_revision = self.account.revision;
                    self.read_account(cx, Read::Orders, 0);
                } else {
                    self.account.count += 1;
                    self.schedule_reconcile(cx);
                }
            }
            Rows::Orders(mut rows) => {
                rows.extend(
                    cx.store
                        .orders()
                        .iter()
                        .filter(|o| skipped.contains(&o.id))
                        .cloned()
                        .collect::<Vec<_>>(),
                );
                if matches!(request.read, Read::Orders) {
                    self.account.order_rows = Some(rows);
                    self.read_account(cx, Read::AdvancedOrders, 0);
                } else {
                    if let Some(mut regular) = self.account.order_rows.take() {
                        regular.extend(rows);
                        regular.retain(|o| {
                            !self.account.terminal.contains(&o.id)
                                && self
                                    .account
                                    .order_versions
                                    .get(&o.id)
                                    .is_none_or(|v| *v <= self.account.order_revision)
                        });
                        regular.extend(
                            cx.store
                                .orders()
                                .iter()
                                .filter(|o| {
                                    self.account
                                        .order_versions
                                        .get(&o.id)
                                        .is_some_and(|v| *v > self.account.order_revision)
                                })
                                .cloned(),
                        );
                        for row in &mut regular {
                            if let Some(live) = cx.store.order(&row.id)
                                && live.last_update > row.last_update
                            {
                                *row = live.clone();
                            }
                        }
                        cx.store.set_orders(regular);
                    }
                    self.account.count += 1;
                    self.schedule_reconcile(cx);
                }
            }
        }
    }
    pub(crate) fn authenticate(&self, cx: &mut Cx<'_, Pending>) {
        if let (Some(credentials), Some(socket)) = (&self.credentials, self.sockets[1].socket) {
            cx.send(
                socket,
                credentials.envelope("public/auth", &Params::default(), cx.now, true),
            );
        }
    }
    pub(crate) fn authenticated(&mut self, cx: &mut Cx<'_, Pending>, envelope: &Envelope<'_>) {
        if envelope.code != 0 {
            return;
        }
        if let Some(socket) = self.sockets[1].socket {
            self.message_id += 1;
            // user.trade supplies execution identity and price, independent of order totals.
            let channels = [
                "user.order",
                "user.advance.order",
                "user.positions",
                "user.balance",
                "user.trade",
            ];
            cx.send(
                socket,
                format!(
                    "{{\"id\":{},\"method\":{},\"params\":{{\"channels\":{}}},\"nonce\":{}}}",
                    self.message_id,
                    quoted("subscribe"),
                    serde_json::to_string(&channels).unwrap(),
                    cx.now as u64
                ),
            );
            self.reconcile(cx);
        }
    }
    pub(crate) fn private_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        message: Message<'_>,
    ) -> serde_json::Result<()> {
        match message.channel {
            channel
                if channel.starts_with("user.order")
                    || channel.starts_with("user.advance.order") =>
            {
                each_row(cx, "order", message.data, |cx, row: OrderRow| {
                    if cx.store.market(row.instrument_name).is_none() {
                        return;
                    }
                    let order = row.order(cx.now, cx.store.order(&id(row.order_id)));
                    if cx
                        .store
                        .order(&order.id)
                        .is_some_and(|o| o.last_update > order.last_update)
                    {
                        return;
                    }
                    if self.account.terminal.contains(&order.id)
                        && order.status == OrderStatus::Open
                    {
                        return;
                    }
                    self.account.revision += 1;
                    self.account
                        .order_versions
                        .insert(order.id.clone(), self.account.revision);
                    if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                        self.account.terminal.insert(order.id.clone());
                    }
                    cx.store.upsert_order(order);
                })?;
            }
            "user.positions" => {
                each_row(cx, "position", message.data, |cx, row: PositionRow| {
                    if cx.store.market(row.instrument_name).is_none() {
                        return;
                    }
                    let position = row.position(cx.now);
                    let time = position.last_update.unwrap_or(cx.now);
                    let closed_at = self
                        .account
                        .symbol_closures
                        .get(&position.symbol)
                        .map_or(f64::NEG_INFINITY, |(_, t)| *t);
                    let last = if position.contracts == 0.0 {
                        self.account
                            .position_times
                            .iter()
                            .filter(|(id, _)| id.starts_with(&format!("{}:", position.symbol)))
                            .map(|(_, t)| *t)
                            .fold(closed_at, f64::max)
                    } else {
                        self.account
                            .position_times
                            .get(&position.id)
                            .copied()
                            .unwrap_or(f64::NEG_INFINITY)
                            .max(closed_at)
                    };
                    let closed_position = self
                        .account
                        .position_closures
                        .get(&position.id)
                        .copied()
                        .unwrap_or(f64::NEG_INFINITY);
                    if time < last
                        || (position.contracts > 0.0 && time <= closed_at.max(closed_position))
                    {
                        return;
                    }
                    self.account.revision += 1;
                    if position.contracts == 0.0 {
                        self.account
                            .symbol_closures
                            .insert(position.symbol.clone(), (self.account.revision, time));
                    } else {
                        self.account
                            .position_times
                            .insert(position.id.clone(), time);
                        self.account
                            .position_versions
                            .insert(position.id.clone(), self.account.revision);
                    }
                    if position.contracts > 0.0 {
                        cx.store.update_setting(&position.symbol, |s| {
                            s.leverage = position.leverage;
                            s.margin_type = position.margin_type;
                        });
                    }
                    if position.contracts == 0.0 {
                        cx.store.update_positions(|p| {
                            if p.symbol == position.symbol {
                                p.contracts = 0.0;
                            }
                        });
                    } else {
                        cx.store.upsert_position(position);
                    }
                })?;
            }
            "user.balance" => {
                let mut first = true;
                each_row(cx, "balance", message.data, |cx, row: BalanceRow| {
                    if first {
                        first = false;
                        self.account.revision += 1;
                        self.account.balance_revision = self.account.revision;
                        cx.store.set_balance(row.balance());
                        if let Some(currencies) = row.currencies(cx) {
                            cx.store.set_currency_balances(currencies);
                        }
                    }
                })?;
            }
            "user.trade" => {
                each_row(cx, "execution", message.data, |cx, row: Execution| {
                    if cx.store.market(row.instrument_name).is_none() {
                        return;
                    }
                    let trade = id(row.trade_id);
                    if !self.account.executions.insert(trade.clone()) {
                        return;
                    }
                    self.account.execution_queue.push_back(trade);
                    if self.account.execution_queue.len() > 4096
                        && let Some(old) = self.account.execution_queue.pop_front()
                    {
                        self.account.executions.remove(&old);
                    }
                    cx.fill(Fill {
                        order_id: id(row.order_id),
                        symbol: row.instrument_name.into(),
                        side: side(row.side),
                        price: num::parse(row.traded_price),
                        amount: num::parse(row.traded_quantity),
                    });
                })?;
            }
            _ => {}
        }
        Ok(())
    }
}
