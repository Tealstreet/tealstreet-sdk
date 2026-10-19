//! The private side: socket auth and topics, the REST snapshots of orders,
//! positions and the unified wallet, and the reconciliation that keeps them
//! honest against the stream.
use crate::sign::Query;
use crate::wire::*;
use crate::{BybitLinear, Channel, Pending, Rest, Then, venue_error};
use safe_cex_engine::model::{
    Balance, MarginType, Order, OrderExtra, OrderSide, OrderStatus, OrderType, Position,
    PositionMode, PositionSide, RejectReason, StopTriggerType, TimeInForce,
};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Cx, Fill, LogLevel, Response, SocketId, Store, TimerId, num};
use std::collections::{HashMap, HashSet};

/// Positions and the wallet are re-read this often; the stream carries
/// changes in between, so this only repairs what a dropped frame missed.
const RECONCILE_INTERVAL_MS: f64 = 5_000.0;
/// A failed order snapshot is retried this often until one succeeds.
const ORDERS_RETRY_MS: f64 = 15_000.0;
const ORDERS_PAGE_LIMIT: u32 = 50;
const POSITIONS_PAGE_LIMIT: u32 = 200;
/// Reject reasons that are the user's own cancel, not an error.
const QUIET_REJECT_REASONS: [&str; 2] = ["EC_PerCancelRequest", "EC_NoError"];

pub enum Request {
    AccountInfo,
    Orders {
        generation: u32,
        coin: &'static str,
        cursor: String,
        rows: Vec<Mapped>,
    },
    Positions {
        generation: u32,
        coin: &'static str,
        cursor: String,
        rows: Vec<MappedPosition>,
    },
    Wallet,
}

/// One order row as the store holds it: the order and its attached
/// stop-loss and take-profit, which Bybit keeps on the order itself.
pub struct Mapped {
    pub orders: Vec<Order>,
    /// A position TP/SL (`tpslMode` Full or Partial), rather than an
    /// opening trigger. Full stops have zero amount in the store.
    pub position_bound: bool,
}

pub struct MappedPosition {
    pub position: Position,
    /// Placeholder rows repeat a symbol; the most informative one wins.
    score: u32,
    updated: f64,
}

struct PositionsJoin {
    generation: u32,
    /// Position-event count when the reads were issued; a newer stream event
    /// makes the snapshot stale.
    version: u32,
    pending: usize,
    rows: Vec<MappedPosition>,
    failed: bool,
}

struct OrdersJoin {
    generation: u32,
    issued_at: f64,
    pending: usize,
    rows: Vec<Mapped>,
    failed: bool,
}

/// The unified wallet's account-wide figures, kept so the balance can be
/// recomputed when positions change.
#[derive(Clone, Copy, Default)]
pub struct Wallet {
    equity: Option<f64>,
    wallet_balance: Option<f64>,
    margin_balance: Option<f64>,
    available: Option<f64>,
    initial_margin: Option<f64>,
    maintenance_margin: Option<f64>,
    used_from_coins: Option<f64>,
    available_from_coins: Option<f64>,
}

#[derive(Default)]
pub struct AccountState {
    started: bool,
    pub authed: bool,
    pub margin_type: Option<MarginType>,
    /// `unifiedMarginStatus` 1 is a classic account: USDT only, CONTRACT wallet.
    classic: bool,
    tick_timer: Option<TimerId>,
    position_version: u32,
    positions_generation: u32,
    positions: Option<PositionsJoin>,
    positions_follow_up: bool,
    orders_generation: u32,
    orders: Option<OrdersJoin>,
    orders_follow_up: bool,
    orders_stale: bool,
    last_orders_retry_at: f64,
    wallet_in_flight: bool,
    wallet_follow_up: bool,
    wallet: Option<Wallet>,
    /// Bybit permits different position modes on different symbols.
    pub position_modes: HashMap<String, bool>,
    pub position_bound: HashSet<String>,
}

fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

/// `a - b` rounded to the larger decimal count of the two, as the TS
/// `safe-math` subtract does, so float noise never reaches the store.
pub fn subtract(a: f64, b: f64) -> f64 {
    let factor = 10f64.powi(num::decimals(a).max(num::decimals(b)) as i32);
    num::round_half_up((a - b) * factor) / factor
}

pub fn order_side(text: Option<&str>) -> Option<OrderSide> {
    match text? {
        "Buy" => Some(OrderSide::Buy),
        "Sell" => Some(OrderSide::Sell),
        _ => None,
    }
}

fn trigger(text: Option<&str>) -> Option<StopTriggerType> {
    match text? {
        "MarkPrice" => Some(StopTriggerType::MarkPrice),
        "LastPrice" => Some(StopTriggerType::LastPrice),
        "IndexPrice" => Some(StopTriggerType::IndexPrice),
        _ => None,
    }
}

pub fn order_type(text: &str) -> Option<OrderType> {
    Some(match text {
        "Limit" => OrderType::Limit,
        "Market" => OrderType::Market,
        "Stop" | "StopLoss" | "PartialStopLoss" => OrderType::StopLoss,
        "TakeProfit" | "PartialTakeProfit" => OrderType::TakeProfit,
        "TrailingStop" => OrderType::TrailingStopLoss,
        _ => return None,
    })
}

pub fn order_status(text: &str) -> Option<OrderStatus> {
    Some(match text {
        "Created" | "New" | "Active" | "Untriggered" | "PartiallyFilled" => OrderStatus::Open,
        "Rejected" | "Filled" | "Deactivated" | "Triggered" => OrderStatus::Closed,
        "PendingCancel" | "Cancelled" => OrderStatus::Canceled,
        _ => return None,
    })
}

fn time_in_force(text: Option<&str>) -> Option<TimeInForce> {
    Some(match text? {
        "GTC" => TimeInForce::GoodTillCancel,
        "IOC" => TimeInForce::ImmediateOrCancel,
        "FOK" => TimeInForce::FillOrKill,
        // RPI is maker-only; Tealstreet never places it.
        "PostOnly" | "RPI" => TimeInForce::PostOnly,
        _ => return None,
    })
}

/// `BTCUSD`, `ETHUSDZ26`: coin-settled contracts this connection does not trade.
fn is_inverse_symbol(symbol: &str) -> bool {
    let Some(at) = symbol.find("USD") else {
        return false;
    };
    let (base, rest) = (&symbol[..at], &symbol[at + 3..]);
    let expiry = rest.len() == 3
        && "FGHJKMNQUVXZ".contains(&rest[..1])
        && rest[1..].bytes().all(|b| b.is_ascii_digit());
    !base.is_empty()
        && base
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && (rest.is_empty() || expiry)
}

/// The shared private socket carries every category on the account; this
/// connection keeps linear rows only.
fn is_linear(category: &Option<Str<'_>>, symbol: &str) -> bool {
    match present(category) {
        Some(category) => category == "linear",
        None => !is_inverse_symbol(symbol) && symbol.matches('-').count() < 3,
    }
}

/// The store row id of a position.
pub fn position_id(symbol: &str, side: PositionSide, mode: Option<PositionMode>) -> String {
    match mode {
        Some(PositionMode::Oneway) => symbol.to_owned(),
        _ => format!(
            "{symbol}:{}",
            match side {
                PositionSide::Long => "long",
                PositionSide::Short => "short",
            }
        ),
    }
}

/// Map an order row: the order, plus its attached stop-loss and take-profit
/// as `{id}__stop_loss` / `{id}__take_profit` rows. `None` for a row the
/// store cannot hold (an unknown type or no side).
pub fn map_order(o: &OrderRow<'_>, store: &Store, now: f64) -> Option<Mapped> {
    let stop_order_type = present(&o.stop_order_type);
    let tpsl_mode = present(&o.tpsl_mode);
    let position_bound = matches!(tpsl_mode, Some("Full" | "Partial"));
    let create_type = present(&o.create_type);
    let from_create_type = position_bound
        .then(|| match create_type? {
            "CreateByTakeProfit" | "CreateByPartialTakeProfit" => Some("TakeProfit"),
            "CreateByStopLoss" | "CreateByPartialStopLoss" => Some("StopLoss"),
            "CreateByTrailingStop" => Some("TrailingStop"),
            _ => None,
        })
        .flatten();
    let is_stop = stop_order_type.is_some_and(|t| t != "UNKNOWN") || from_create_type.is_some();
    let trigger_price = present(&o.trigger_price);
    let price_text = if is_stop {
        trigger_price.or(o.price.as_deref())
    } else {
        o.price.as_deref()
    };
    let wire_type = from_create_type.or(if is_stop {
        stop_order_type
    } else {
        present(&o.order_type)
    })?;
    let opening_trigger = is_stop
        && !o.reduce_only
        && !o.close_on_trigger
        && !position_bound
        && (trigger_price.map_or(0.0, float) > 0.0 || create_type == Some("CreateByStopOrder"));
    let kind = if opening_trigger {
        OrderType::Conditional
    } else {
        order_type(wire_type)?
    };
    let protective = o.reduce_only
        || o.close_on_trigger
        || position_bound
        || matches!(
            kind,
            OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
        );
    let side = order_side(o.side.as_deref())
        .or_else(|| protective.then(|| protective_side(o, store)).flatten())?;
    let qty = or_zero(&o.qty);
    let filled = or_zero(&o.cum_exec_qty);
    let remaining = match present(&o.leaves_qty) {
        Some(leaves) => float(leaves),
        None => subtract(qty, filled),
    };
    let amount = if matches!(wire_type, "StopLoss" | "TakeProfit") && tpsl_mode == Some("Full") {
        0.0
    } else {
        qty
    };
    let trigger_type = match kind {
        OrderType::Conditional | OrderType::StopLoss | OrderType::TakeProfit => {
            trigger(o.trigger_by.as_deref())
        }
        _ => None,
    };
    let reject = present(&o.reject_reason);
    let extra = (reject == Some("EC_PostOnlyWillTakeLiquidity")).then(|| OrderExtra {
        reject_reason: Some(RejectReason::PostOnly),
        ..OrderExtra::default()
    });
    let id = o.order_id.to_string();
    let parent = Order {
        id: id.clone(),
        parent_id: None,
        status: order_status(&o.order_status).unwrap_or(OrderStatus::Canceled),
        symbol: o.symbol.to_string(),
        kind,
        side,
        price: price_text.map_or(f64::NAN, float),
        amount,
        filled,
        remaining,
        reduce_only: o.reduce_only,
        close: Some(o.close_on_trigger),
        client_order_id: present(&o.order_link_id).map(str::to_owned),
        time_in_force: time_in_force(present(&o.time_in_force)),
        last_update: now,
        trigger_type,
        take_profit: None,
        stop_loss: None,
        extra,
    };
    let mut orders = Vec::with_capacity(3);
    for (suffix, kind, level, by) in [
        (
            "__stop_loss",
            OrderType::StopLoss,
            &o.stop_loss,
            &o.sl_trigger_by,
        ),
        (
            "__take_profit",
            OrderType::TakeProfit,
            &o.take_profit,
            &o.tp_trigger_by,
        ),
    ] {
        let level = float_of(level);
        if level > 0.0 {
            orders.push(Order {
                id: format!("{id}{suffix}"),
                parent_id: Some(id.clone()),
                kind,
                side: side.opposite(),
                price: level,
                filled: 0.0,
                remaining: amount,
                client_order_id: None,
                trigger_type: trigger(present(by)).or(parent.trigger_type),
                extra: None,
                ..parent.clone()
            });
        }
    }
    orders.insert(0, parent);
    Some(Mapped {
        orders,
        position_bound,
    })
}

/// A protective row without a side closes its position: the hedge leg says
/// which, or else the symbol's single open position.
fn protective_side(o: &OrderRow<'_>, store: &Store) -> Option<OrderSide> {
    match o.position_idx {
        Some(1) => return Some(OrderSide::Sell),
        Some(2) => return Some(OrderSide::Buy),
        _ => {}
    }
    let mut open = store
        .positions()
        .iter()
        .filter(|p| p.symbol == o.symbol && p.contracts.abs() > 0.0);
    let only = open.next()?;
    if open.next().is_some() {
        return None;
    }
    Some(match only.side {
        PositionSide::Long => OrderSide::Sell,
        PositionSide::Short => OrderSide::Buy,
    })
}

pub fn map_position(
    p: &PositionRow<'_>,
    store: &Store,
    margin_type: MarginType,
    now: f64,
) -> MappedPosition {
    let entry_price = present(&p.avg_price)
        .or(present(&p.entry_price))
        .map_or(0.0, float);
    let contracts = or_zero(&p.size);
    let side = match p.position_idx {
        Some(1) => PositionSide::Long,
        Some(2) => PositionSide::Short,
        _ if p.side.as_deref() == Some("Sell") => PositionSide::Short,
        _ => PositionSide::Long,
    };
    let mode = p.position_idx.map(|idx| {
        if idx == 0 {
            PositionMode::Oneway
        } else {
            PositionMode::Hedged
        }
    });
    let mut unrealized = or_zero(&p.unrealised_pnl);
    // USDC contracts report PnL settled differently; TS values them at mark.
    if !p.symbol.ends_with("USDT")
        && store.market(&p.symbol).is_some_and(|m| m.quote == "USDC")
        && let Some(ticker) = store.ticker(&p.symbol)
    {
        let direction = if side == PositionSide::Long {
            1.0
        } else {
            -1.0
        };
        unrealized = direction * (ticker.mark * contracts - entry_price * contracts);
    }
    let margin = or_zero(&p.position_im);
    let updated = or_zero(&p.updated_time);
    let mut score = 0;
    if contracts != 0.0 {
        score += 1000;
    }
    if updated > 0.0 {
        score += 100;
    }
    if p.seq.is_some_and(|seq| seq >= 0) {
        score += 50;
    }
    if present(&p.position_status).is_some() {
        score += 20;
    }
    if present(&p.avg_price).is_some() {
        score += 5;
    }
    MappedPosition {
        position: Position {
            id: position_id(&p.symbol, side, mode),
            symbol: p.symbol.to_string(),
            side,
            entry_price,
            notional: or_zero(&p.position_value),
            leverage: float_of(&p.leverage),
            margin_type: Some(margin_type),
            position_mode: mode,
            unrealized_pnl: unrealized,
            realized_pnl: Some(or_zero(&p.cur_realised_pnl)),
            contracts,
            liquidation_price: or_zero(&p.liq_price),
            margin_size: (margin > 0.0).then_some(margin),
            last_update: Some(now),
        },
        score,
        updated,
    }
}

impl Wallet {
    fn from_wire(account: &WalletAccount<'_>) -> Self {
        let mut used = None::<f64>;
        let mut available = None::<f64>;
        for coin in &account.coin {
            for value in [&coin.total_position_im, &coin.total_order_im] {
                if let Some(value) = finite(float_of(value)) {
                    used = Some(used.unwrap_or(0.0) + value);
                }
            }
            let value = finite(float_of(&coin.usd_value))
                .or(finite(float_of(&coin.wallet_balance)))
                .or(finite(float_of(&coin.equity)));
            if let Some(value) = value {
                let held = [
                    &coin.total_position_im,
                    &coin.total_order_im,
                    &coin.locked,
                    &coin.bonus,
                ]
                .into_iter()
                .filter_map(|v| finite(float_of(v)))
                .sum::<f64>();
                available = Some(available.unwrap_or(0.0) + (value - held).max(0.0));
            }
        }
        Self {
            equity: finite(float_of(&account.total_equity)),
            wallet_balance: finite(float_of(&account.total_wallet_balance)),
            margin_balance: finite(float_of(&account.total_margin_balance)),
            available: finite(float_of(&account.total_available_balance)),
            initial_margin: finite(float_of(&account.total_initial_margin)),
            maintenance_margin: finite(float_of(&account.total_maintenance_margin)),
            used_from_coins: used,
            available_from_coins: available,
        }
    }

    /// The account balance against `upnl`, the open positions' unrealized
    /// PnL. Demo accounts can leave the aggregate margin fields blank; the
    /// coin rows stand in for them.
    fn balance(&self, upnl: f64) -> Balance {
        let used_top = (self.initial_margin.is_some() || self.maintenance_margin.is_some())
            .then(|| self.maintenance_margin.unwrap_or(0.0) + self.initial_margin.unwrap_or(0.0));
        let available_top = self.margin_balance.map(|margin| match self.initial_margin {
            Some(initial) => (margin - initial).max(0.0),
            None => margin,
        });
        Balance {
            total: subtract(self.equity.or(self.wallet_balance).unwrap_or(0.0), upnl),
            upnl,
            used: used_top.or(self.used_from_coins).unwrap_or(0.0),
            free: self
                .available
                .or(available_top)
                .or(self.available_from_coins)
                .or(self.wallet_balance)
                .or(self.equity)
                .unwrap_or(0.0),
        }
    }
}

fn open_upnl(store: &Store) -> f64 {
    store.positions().iter().map(|p| p.unrealized_pnl).sum()
}

impl BybitLinear {
    fn settle_coins(&self) -> &'static [&'static str] {
        if self.account.classic {
            &["USDT"]
        } else {
            &["USDT", "USDC"]
        }
    }

    fn margin_type(&self) -> MarginType {
        self.account.margin_type.unwrap_or(MarginType::Cross)
    }

    pub(crate) fn request_account_info(&self, cx: &mut Cx<'_, Pending>) {
        self.send(
            cx,
            Rest::get("/v5/account/info", Query::new()).signed(),
            Then::Account(Request::AccountInfo),
        );
    }

    pub(crate) fn try_start_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.startup.tickers_loaded
            || !self.startup.account_info
            || std::mem::replace(&mut self.account.started, true)
        {
            return;
        }
        self.connect(cx, Channel::Private);
        self.fetch_orders(cx);
        self.fetch_positions(cx);
        self.fetch_wallet(cx);
        self.account.tick_timer = Some(cx.set_timer(RECONCILE_INTERVAL_MS));
    }

    fn page_query(coin: &str, limit: u32, cursor: &str) -> Query {
        Query::new()
            .str("category", "linear")
            .str("settleCoin", coin)
            .num("limit", f64::from(limit))
            .str("cursor", cursor)
    }

    fn request_orders_page(
        &self,
        cx: &mut Cx<'_, Pending>,
        generation: u32,
        coin: &'static str,
        cursor: String,
    ) {
        let query = Self::page_query(coin, ORDERS_PAGE_LIMIT, &cursor);
        let request = Request::Orders {
            generation,
            coin,
            cursor,
            rows: Vec::new(),
        };
        self.send(
            cx,
            Rest::get("/v5/order/realtime", query).streamed().signed(),
            Then::Account(request),
        );
    }

    fn request_positions_page(
        &self,
        cx: &mut Cx<'_, Pending>,
        generation: u32,
        coin: &'static str,
        cursor: String,
    ) {
        let query = Self::page_query(coin, POSITIONS_PAGE_LIMIT, &cursor);
        let request = Request::Positions {
            generation,
            coin,
            cursor,
            rows: Vec::new(),
        };
        self.send(
            cx,
            Rest::get("/v5/position/list", query).streamed().signed(),
            Then::Account(request),
        );
    }

    pub(crate) fn fetch_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.orders.is_some() {
            return;
        }
        self.account.orders_generation += 1;
        let generation = self.account.orders_generation;
        let coins = self.settle_coins();
        self.account.orders = Some(OrdersJoin {
            generation,
            issued_at: cx.now,
            pending: coins.len(),
            rows: Vec::new(),
            failed: false,
        });
        for coin in coins {
            self.request_orders_page(cx, generation, coin, String::new());
        }
    }

    pub(crate) fn fetch_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.positions.is_some() {
            return;
        }
        self.account.positions_generation += 1;
        let generation = self.account.positions_generation;
        let coins = self.settle_coins();
        self.account.positions = Some(PositionsJoin {
            generation,
            version: self.account.position_version,
            pending: coins.len(),
            rows: Vec::new(),
            failed: false,
        });
        for coin in coins {
            self.request_positions_page(cx, generation, coin, String::new());
        }
    }

    fn fetch_wallet(&mut self, cx: &mut Cx<'_, Pending>) {
        if std::mem::replace(&mut self.account.wallet_in_flight, true) {
            return;
        }
        let account_type = if self.account.classic {
            "CONTRACT"
        } else {
            "UNIFIED"
        };
        let query = Query::new().str("accountType", account_type);
        self.send(
            cx,
            Rest::get("/v5/account/wallet-balance", query).signed(),
            Then::Account(Request::Wallet),
        );
    }

    pub(crate) fn on_account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account.tick_timer != Some(timer) {
            return;
        }
        let now = cx.now;
        self.account.tick_timer = Some(cx.set_timer(RECONCILE_INTERVAL_MS));
        if self.account.orders_stale && now - self.account.last_orders_retry_at >= ORDERS_RETRY_MS {
            self.account.last_orders_retry_at = now;
            self.fetch_orders(cx);
        }
        self.fetch_positions(cx);
        self.fetch_wallet(cx);
    }

    pub(crate) fn on_private_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if let Some(credentials) = &self.credentials {
            cx.send(socket, credentials.ws_auth(cx.now + self.clock_offset_ms));
        }
    }

    pub(crate) fn on_private_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        match topic(text) {
            Some("order") => {
                if let Ok(frame) = serde_json::from_str::<TopicFrame<OrderRow>>(text) {
                    for row in frame
                        .data
                        .iter()
                        .filter(|r| is_linear(&r.category, &r.symbol))
                    {
                        self.on_order_row(cx, row);
                    }
                }
            }
            Some("execution") => {
                if let Ok(frame) = serde_json::from_str::<TopicFrame<ExecutionRow>>(text) {
                    for row in frame
                        .data
                        .iter()
                        .filter(|r| is_linear(&r.category, &r.symbol))
                    {
                        on_execution(cx, row);
                    }
                }
            }
            Some("position") => {
                if let Ok(frame) = serde_json::from_str::<TopicFrame<PositionRow>>(text) {
                    self.on_position_rows(cx, &frame.data);
                }
            }
            Some("wallet") => {
                if let Ok(frame) = serde_json::from_str::<TopicFrame<WalletAccount>>(text)
                    && let Some(account) = frame.data.first()
                {
                    self.account.wallet = Some(Wallet::from_wire(account));
                    self.commit_balance(cx);
                }
            }
            Some(_) => {}
            None => self.on_private_op(cx, text),
        }
    }

    fn on_private_op(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Ok(frame) = serde_json::from_str::<OpFrame>(text) else {
            return;
        };
        let op = frame.op.as_deref().unwrap_or_default();
        match (op, frame.success) {
            ("auth", Some(true)) => {
                self.account.authed = true;
                if let Some(socket) = self.private.socket {
                    cx.send(
                        socket,
                        r#"{"op":"subscribe","args":["order","execution","position","wallet"]}"#,
                    );
                }
            }
            ("subscribe", Some(true)) if self.account.authed => {
                // Cover the gap before topics became live, including the first connection.
                self.account.orders_follow_up = self.account.orders.is_some();
                self.account.positions_follow_up = self.account.positions.is_some();
                self.account.wallet_follow_up = self.account.wallet_in_flight;
                self.fetch_orders(cx);
                self.fetch_positions(cx);
                self.fetch_wallet(cx);
            }
            (_, Some(false)) => {
                let message = frame.ret_msg.as_deref().unwrap_or_default();
                cx.log(LogLevel::Error, format!("[WS]{op}:{message}"));
                // An unauthenticated socket answers pings but carries no topics.
                if op == "auth"
                    && let Some(socket) = self.private.socket
                {
                    cx.close(socket);
                    self.on_socket_closed(cx, Channel::Private, 4001);
                }
            }
            _ => {}
        }
    }

    fn on_order_row(&mut self, cx: &mut Cx<'_, Pending>, row: &OrderRow<'_>) {
        let status = row.order_status.as_ref();
        // A replaced order reports a partial fill of nothing at no price.
        if status == "PartiallyFilled" && or_zero(&row.price) <= 0.0 && or_zero(&row.qty) <= 0.0 {
            return;
        }
        let Some(mut mapped) = map_order(row, cx.store, cx.now) else {
            return;
        };
        self.note_position_bound(&mapped);
        match status {
            "Cancelled" | "Filled" | "Deactivated" | "Rejected" => {
                if let Some(reason) = present(&row.reject_reason)
                    && status != "Filled"
                    && !QUIET_REJECT_REASONS.contains(&reason)
                {
                    cx.log(LogLevel::Error, reason.to_owned());
                }
                let final_status = if status == "Filled" {
                    OrderStatus::Closed
                } else {
                    OrderStatus::Canceled
                };
                for order in mapped.orders.drain(..) {
                    cx.store.upsert_order(Order {
                        status: final_status,
                        ..order
                    });
                }
            }
            "New" | "Untriggered" | "PartiallyFilled" => {
                for order in mapped.orders.drain(..) {
                    cx.store.upsert_order(order);
                }
            }
            _ => {}
        }
    }

    fn note_position_bound(&mut self, mapped: &Mapped) {
        let id = &mapped.orders[0].id;
        if mapped.position_bound {
            self.account.position_bound.insert(id.clone());
        } else if !self.account.position_bound.is_empty() {
            self.account.position_bound.remove(id);
        }
    }

    fn on_position_rows(&mut self, cx: &mut Cx<'_, Pending>, rows: &[PositionRow<'_>]) {
        let margin_type = self.margin_type();
        let mut changed = false;
        for row in rows.iter().filter(|r| is_linear(&r.category, &r.symbol)) {
            let mapped = map_position(row, cx.store, margin_type, cx.now);
            let position = mapped.position;
            if let Some(mode) = position.position_mode {
                self.account
                    .position_modes
                    .insert(position.symbol.clone(), mode == PositionMode::Hedged);
            }
            if position.leverage > 0.0 {
                let leverage = position.leverage;
                cx.store.update_setting(&position.symbol, |s| {
                    s.leverage = leverage;
                    s.margin_type = Some(margin_type);
                });
            }
            cx.store.upsert_position(position);
            changed = true;
        }
        if !changed {
            return;
        }
        cx.store
            .set_hedged(self.account.position_modes.values().any(|hedged| *hedged));
        self.account.position_version += 1;
        // Only the PnL follows the stream; the total waits for the wallet.
        if cx.store.loaded().balance {
            let balance = Balance {
                upnl: open_upnl(cx.store),
                ..cx.store.balance().clone()
            };
            cx.store.set_balance(balance);
        }
    }

    fn commit_balance(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(wallet) = self.account.wallet {
            cx.store.set_balance(wallet.balance(open_upnl(cx.store)));
        }
    }

    pub(crate) fn on_account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Orders { rows, .. } => {
                if let Ok(row) = serde_json::from_str::<OrderRow>(item)
                    && let Some(mapped) = map_order(&row, cx.store, cx.now)
                {
                    rows.push(mapped);
                }
            }
            Request::Positions { rows, .. } => {
                if let Ok(row) = serde_json::from_str::<PositionRow>(item) {
                    rows.push(map_position(&row, cx.store, self.margin_type(), cx.now));
                }
            }
            _ => {}
        }
    }

    pub(crate) fn on_account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let outcome = match &response {
            Ok(response) => match venue_error(response) {
                None => Ok(response.body),
                Some((_, message)) => Err(message),
            },
            Err(error) => Err((*error).to_owned()),
        };
        match request {
            Request::AccountInfo => {
                match outcome {
                    Ok(body) => {
                        if let Ok(info) = serde_json::from_str::<AccountInfoEnvelope>(body)
                            && let Some(info) = info.result
                        {
                            self.account.margin_type =
                                Some(if info.margin_mode.as_deref() == Some("ISOLATED_MARGIN") {
                                    MarginType::Isolated
                                } else {
                                    MarginType::Cross
                                });
                            self.account.classic = info.unified_margin_status == Some(1);
                        }
                    }
                    // Public data still loads; trading reports its own errors.
                    Err(message) => cx.log(
                        LogLevel::Warning,
                        format!("Bybit market data is ready, but the private API was rejected ({message}) — trading is unavailable"),
                    ),
                }
                self.startup.account_info = true;
                self.try_start_account(cx);
            }
            Request::Orders {
                generation,
                coin,
                cursor,
                rows,
            } => {
                let next = outcome
                    .as_ref()
                    .ok()
                    .and_then(|body| next_cursor(body, &cursor, rows.is_empty()));
                let Some(join) = self
                    .account
                    .orders
                    .as_mut()
                    .filter(|j| j.generation == generation)
                else {
                    return;
                };
                match outcome {
                    Ok(_) => {
                        join.rows.extend(rows);
                        if let Some(next) = next {
                            self.request_orders_page(cx, generation, coin, next);
                            return;
                        }
                        join.pending -= 1;
                    }
                    Err(message) => {
                        cx.log(LogLevel::Error, format!("Bybit orders: {message}"));
                        join.failed = true;
                    }
                }
                self.finish_orders(cx);
            }
            Request::Positions {
                generation,
                coin,
                cursor,
                rows,
            } => {
                let next = outcome
                    .as_ref()
                    .ok()
                    .and_then(|body| next_cursor(body, &cursor, false));
                let Some(join) = self
                    .account
                    .positions
                    .as_mut()
                    .filter(|j| j.generation == generation)
                else {
                    return;
                };
                match outcome {
                    Ok(_) => {
                        join.rows.extend(rows);
                        if let Some(next) = next {
                            self.request_positions_page(cx, generation, coin, next);
                            return;
                        }
                        join.pending -= 1;
                    }
                    Err(message) => {
                        cx.log(LogLevel::Error, format!("Bybit positions: {message}"));
                        join.failed = true;
                    }
                }
                self.finish_positions(cx);
            }
            Request::Wallet => {
                self.account.wallet_in_flight = false;
                if std::mem::take(&mut self.account.wallet_follow_up) {
                    self.fetch_wallet(cx);
                }
                let account = outcome.and_then(|body| {
                    serde_json::from_str::<WalletEnvelope>(body)
                        .ok()
                        .and_then(|w| w.result)
                        .and_then(|r| r.list.into_iter().next())
                        .map(|account| Wallet::from_wire(&account))
                        .ok_or_else(|| "Invalid or empty balance data received".to_owned())
                });
                match account {
                    Ok(wallet) => {
                        self.account.wallet = Some(wallet);
                        self.commit_balance(cx);
                    }
                    Err(message) => cx.log(LogLevel::Error, format!("Bybit balance: {message}")),
                }
            }
        }
    }

    /// Commit the order snapshot once every settle coin is in. A failed read
    /// is not authoritative: the store keeps its orders and the tick retries.
    fn finish_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(join) = &self.account.orders else {
            return;
        };
        if !join.failed && join.pending > 0 {
            return;
        }
        let join = self.account.orders.take().expect("checked above");
        if std::mem::take(&mut self.account.orders_follow_up) {
            self.fetch_orders(cx);
        }
        if join.failed {
            self.account.orders_stale = true;
            return;
        }
        self.account.orders_stale = false;
        self.account.position_bound.clear();
        let mut orders = Vec::new();
        for mapped in join.rows {
            if mapped.position_bound {
                self.account
                    .position_bound
                    .insert(mapped.orders[0].id.clone());
            }
            orders.extend(mapped.orders);
        }
        // Rows the stream or a command touched after the reads went out are
        // newer than the snapshot.
        let fresh: HashMap<&str, &Order> = cx
            .store
            .orders()
            .iter()
            .filter(|o| o.last_update > join.issued_at)
            .map(|o| (o.id.as_str(), o))
            .collect();
        let mut merged: Vec<Order> = orders
            .into_iter()
            .map(|o| match fresh.get(o.id.as_str()) {
                Some(newer) => (*newer).clone(),
                None => o,
            })
            .collect();
        for (id, order) in &fresh {
            if !merged.iter().any(|o| o.id == *id) {
                merged.push((*order).clone());
            }
        }
        cx.store.set_orders(merged);
    }

    /// Commit the positions snapshot once every settle coin is in.
    fn finish_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(join) = &self.account.positions else {
            return;
        };
        if !join.failed && join.pending > 0 {
            return;
        }
        let join = self.account.positions.take().expect("checked above");
        if std::mem::take(&mut self.account.positions_follow_up) {
            self.fetch_positions(cx);
        }
        if join.failed {
            return;
        }
        if join.version != self.account.position_version {
            // A stream event landed after these reads went out; it is newer.
            return;
        }
        let mut best: Vec<MappedPosition> = Vec::new();
        for row in join.rows {
            match best.iter_mut().find(|b| b.position.id == row.position.id) {
                Some(existing) => {
                    if row.score > existing.score
                        || (row.score == existing.score && row.updated > existing.updated)
                    {
                        *existing = row;
                    }
                }
                None => best.push(row),
            }
        }
        if !best.is_empty() {
            for row in &best {
                if let Some(mode) = row.position.position_mode {
                    self.account
                        .position_modes
                        .insert(row.position.symbol.clone(), mode == PositionMode::Hedged);
                }
            }
            cx.store
                .set_hedged(self.account.position_modes.values().any(|hedged| *hedged));
        }
        let settings: Vec<(u32, MarketSetting)> = best
            .iter()
            .filter(|r| r.position.leverage > 0.0)
            .filter_map(|r| {
                Some((
                    cx.store.market_index(&r.position.symbol)?,
                    MarketSetting {
                        leverage: r.position.leverage,
                        margin_type: r.position.margin_type,
                    },
                ))
            })
            .collect();
        cx.store.set_settings(settings);
        cx.store.set_positions(
            best.into_iter()
                .map(|r| r.position)
                .filter(|p| p.contracts != 0.0)
                .collect(),
        );
        self.commit_balance(cx);
    }
}

/// The next page's cursor, if the venue named one and this page was not empty.
fn next_cursor(body: &str, current: &str, empty_page: bool) -> Option<String> {
    if empty_page {
        return None;
    }
    serde_json::from_str::<PageEnvelope>(body)
        .ok()?
        .result?
        .next_page_cursor
        .map(|c| c.into_owned())
        .filter(|c| !c.is_empty() && c != current)
}

/// One execution is one fill: its own quantity at its own price.
fn on_execution(cx: &mut Cx<'_, Pending>, row: &ExecutionRow<'_>) {
    if !matches!(
        row.exec_type.as_deref(),
        Some("Trade" | "AdlTrade" | "BustTrade")
    ) {
        return;
    }
    let amount = or_zero(&row.exec_qty);
    let Some(side) = order_side(row.side.as_deref()).filter(|_| amount > 0.0) else {
        return;
    };
    cx.fill(Fill {
        order_id: row.order_id.to_string(),
        symbol: row.symbol.to_string(),
        side,
        price: or_zero(&row.exec_price),
        amount,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_inverse_symbols() {
        assert!(is_inverse_symbol("BTCUSD"));
        assert!(is_inverse_symbol("ETHUSDZ26"));
        assert!(!is_inverse_symbol("BTCUSDT"));
        assert!(!is_inverse_symbol("ETHUSDC"));
        assert!(!is_inverse_symbol("BTC-26DEC25"));
        assert_eq!(subtract(1000.1, 0.2), 999.9);
    }
}
