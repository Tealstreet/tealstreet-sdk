//! Atomic private snapshots, socket healing and execution identity.
use crate::{ApexOmni, Pending, private_wire as wire, read, sign};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::SnapshotBuilder;
use safe_cex_engine::credential_selection::CredentialUnavailable;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Balance, MarginType, Order, OrderExtra, OrderSide, OrderStatus, OrderType, Position,
    PositionMode, PositionSide, StopTriggerType, TimeInForce,
};
use safe_cex_engine::request_context::{CommandEntryError, Context, Operation, RetryPolicy};
use safe_cex_engine::selected_read::{
    BackgroundReadError, CommandReadError, LocalRefusal, ReadPolicy, ReadRole, SelectedRead,
};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::store::Store;
use safe_cex_engine::{Command, Cx, Fill, Response, SocketChannel, SocketId, TimerId, VenueError};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

// Preserve native operand precision without snapping public sizes to lot steps.
pub(crate) fn subtract(a: f64, b: f64) -> f64 {
    let difference = a - b;
    let factor =
        10f64.powi(safe_cex_engine::num::decimals(a).max(safe_cex_engine::num::decimals(b)) as i32);
    let scaled = difference * factor;
    if !scaled.is_finite() {
        return difference;
    }
    let floor = scaled.floor();
    // Adding 0.5 can alter an already-integral float above 2^52.
    let rounded = if scaled - floor < 0.5 {
        floor
    } else {
        floor + 1.0
    };
    rounded / factor
}

pub enum Request {
    /// FetchAccount: its terminal publishes through the admitted command context.
    Identity,
    Positions {
        generation: Option<u64>,
        revision: u64,
        rows: SnapshotBuilder<String, Position>,
    },
    Orders {
        generation: Option<u64>,
        revision: u64,
        rows: SnapshotBuilder<String, Order>,
    },
    Balance,
}

/// Rejects a row the snapshot could not map; its stored copies keep their last known state.
fn reject<T>(
    cx: &mut Cx<'_, Pending>,
    rows: &mut SnapshotBuilder<String, T>,
    kind: &str,
    key: &str,
    ids: Vec<String>,
) {
    if key.trim().is_empty() {
        cx.notice(Level::Warn, code::ROW_SKIPPED,
            format!("ApeX skipped malformed {kind} row without an identity; keeping the last known state"),
        );
        rows.reject(None);
        return;
    }
    cx.notice(
        Level::Warn,
        code::ROW_SKIPPED,
        format!("ApeX skipped malformed {kind} row {key}; keeping its last known state"),
    );
    for id in ids {
        rows.reject(Some(id));
    }
}

#[cfg(test)]
mod tests {
    use super::{Account, subtract};
    use safe_cex_engine::model::OrderStatus;

    #[test]
    fn subtraction_keeps_large_integral_values_and_finite_scaling_overflow() {
        let integral = 4_503_599_627_370_497.0;
        assert_eq!(subtract(integral, 0.0), integral);
        assert_eq!(subtract(1e308, 1e-10), 1e308);
        assert_eq!(subtract(5e-324, 0.0), 5e-324);
    }

    #[test]
    fn retiring_order_guards_bounds_memory_and_invalidates_preexisting_reads() {
        let mut account = Account::default();
        account.record_order("first-terminal", Some(OrderStatus::Canceled));
        let read_revision = account.revision;
        for index in 0..8192 {
            account.record_order(&format!("terminal-{index}"), Some(OrderStatus::Canceled));
        }
        assert_eq!(account.order_queue.len(), 8192);
        assert_eq!(account.order_versions.len(), 8192);
        assert_eq!(account.terminal.len(), 8192);
        assert!(!account.terminal.contains_key("first-terminal"));
        assert!(read_revision < account.snapshot_floor);
        assert_eq!(account.snapshot_floor, account.revision);
    }
}
impl Request {
    pub(crate) fn reset(&mut self) {
        match self {
            Self::Positions { rows, .. } => *rows = SnapshotBuilder::default(),
            Self::Orders { rows, .. } => *rows = SnapshotBuilder::default(),
            Self::Balance | Self::Identity => {}
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Execution {
    #[serde(skip)]
    pub(crate) identity: String,
    pub timestamp: f64,
    #[serde(skip)]
    pub(crate) timestamp_ms: f64,
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    pub price: f64,
    pub qty: f64,
    pub side: OrderSide,
    pub commission: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commission_asset: Option<String>,
    #[serde(skip)]
    synthetic: bool,
}

struct Snapshot {
    generation: u64,
    revision: u64,
    positions: Option<Vec<Position>>,
    orders: Option<Vec<Order>>,
    metadata: Option<(String, String, String)>,
    /// The orders leg was fresh and clean, so publishing it releases the uncertain fence.
    orders_release: bool,
}

#[derive(Default)]
struct Progress {
    topic: f64,
    reported: f64,
    synthetic: f64,
    fee: f64,
    at: f64,
}

#[derive(Default)]
pub struct Account {
    supervisor: Supervisor,
    authenticated: bool,
    pub(crate) account_id: String,
    pub(crate) maker_fee: String,
    pub(crate) taker_fee: String,
    pub(crate) initialized: bool,
    pub(crate) recent_executions: Vec<Execution>,
    started: bool,
    socket: Option<SocketId>,
    open: bool,
    opened_at: f64,
    ping_at: f64,
    tick: Option<TimerId>,
    orders_heal: Option<TimerId>,
    positions_heal: Option<TimerId>,
    balance_pending: bool,
    positions_pending: bool,
    orders_pending: bool,
    /// An uncertain placement asked for an orders read dispatched after its mark.
    reconcile_owed: bool,
    /// The orders read in flight started after every such ask, so it may release the fence.
    order_read_fresh: bool,
    snapshot: Option<Snapshot>,
    generation: u64,
    revision: u64,
    position_versions: HashMap<String, (u64, f64, Position)>,
    leverage_versions: HashMap<String, (u64, f64, f64)>,
    order_versions: HashMap<String, u64>,
    order_queue: VecDeque<(String, u64)>,
    snapshot_floor: u64,
    terminal: HashMap<String, (u64, OrderStatus)>,
    cancelling: HashMap<String, f64>,
    seen: HashSet<String>,
    seen_queue: VecDeque<String>,
    progress: HashMap<String, Progress>,
}

impl Account {
    pub(crate) fn new() -> Self {
        Self {
            supervisor: Supervisor::new(Config {
                auth_timeout_ms: Some(10_000.0),
                idle_timeout_ms: 30_000.0,
                ping_interval_ms: Some(5_000.0),
                ..Config::default()
            }),
            ..Self::default()
        }
    }

    pub(crate) fn note_leverage(&mut self, symbol: &str, leverage: f64, now: f64) {
        self.revision += 1;
        self.leverage_versions
            .insert(symbol.to_owned(), (self.revision, now, leverage));
    }
    /// An accepted cancel fences older reads and hides the order briefly; only
    /// a venue terminal event tombstones it, so an order the venue kept reappears.
    pub(crate) fn note_cancel(&mut self, id: &str, now: f64) {
        self.record_order(id, None);
        self.cancelling.retain(|_, at| now - *at < CANCEL_GRACE_MS);
        self.cancelling.insert(id.to_owned(), now);
    }

    fn cancelling(&self, id: &str, now: f64) -> bool {
        self.cancelling
            .get(id)
            .is_some_and(|at| now - at < CANCEL_GRACE_MS)
    }

    fn record_order(&mut self, id: &str, terminal: Option<OrderStatus>) {
        self.revision += 1;
        self.order_versions.insert(id.to_owned(), self.revision);
        if let Some(status) = terminal {
            self.terminal.insert(id.to_owned(), (self.revision, status));
        }
        self.order_queue.push_back((id.to_owned(), self.revision));
        if self.order_queue.len() > 8192
            && let Some((old, revision)) = self.order_queue.pop_front()
            && self.order_versions.get(&old) == Some(&revision)
        {
            self.order_versions.remove(&old);
            self.terminal.remove(&old);
            // Reads predating retired guards cannot become authoritative.
            self.snapshot_floor = self.revision;
        }
    }

    fn prune_progress(&mut self) {
        if self.progress.len() > 4096
            && let Some(old) = self
                .progress
                .iter()
                .min_by(|a, b| a.1.at.total_cmp(&b.1.at))
                .map(|(id, _)| id.clone())
        {
            self.progress.remove(&old);
        }
    }

    fn remember(&mut self, key: String) -> bool {
        if !self.seen.insert(key.clone()) {
            return false;
        }
        self.seen_queue.push_back(key);
        if self.seen_queue.len() > 8192
            && let Some(old) = self.seen_queue.pop_front()
        {
            self.seen.remove(&old);
        }
        true
    }

    fn cache(&mut self, execution: Execution) {
        self.recent_executions.insert(0, execution);
        self.recent_executions
            .sort_by(|a, b| b.timestamp_ms.total_cmp(&a.timestamp_ms));
        self.recent_executions.truncate(500);
    }
}

const CANCEL_GRACE_MS: f64 = 5000.0;

pub(crate) fn execution_milliseconds(time: f64) -> f64 {
    if time > 1e12 { time } else { time * 1000.0 }
}

pub(crate) fn side(value: &str) -> Option<OrderSide> {
    match value.to_ascii_uppercase().as_str() {
        "BUY" | "B" | "LONG" => Some(OrderSide::Buy),
        "SELL" | "S" | "SHORT" => Some(OrderSide::Sell),
        _ => None,
    }
}
fn trigger(value: &str) -> Option<StopTriggerType> {
    match value {
        "INDEX" => Some(StopTriggerType::IndexPrice),
        "MARKET" => Some(StopTriggerType::LastPrice),
        _ => None,
    }
}
fn position_id(symbol: &str, side: PositionSide) -> String {
    format!(
        "{symbol}:{}",
        if side == PositionSide::Long {
            "long"
        } else {
            "short"
        }
    )
}
fn base_id(id: &str) -> &str {
    id.strip_suffix("__stop_loss")
        .or_else(|| id.strip_suffix("__take_profit"))
        .unwrap_or(id)
}

pub(crate) fn map_order(
    store: &Store,
    raw: &wire::OrderRow<'_>,
    now: f64,
) -> Result<Vec<Order>, String> {
    if raw.symbol.trim().is_empty() || raw.id.trim().is_empty() {
        return Err("missing order symbol or ID".into());
    }
    let Some(market) = store
        .market_by_id(&raw.symbol)
        .or_else(|| store.market(&raw.symbol))
    else {
        return Ok(vec![]);
    };
    let status = match raw.status.as_ref() {
        "PENDING" | "OPEN" | "UNTRIGGERED" => OrderStatus::Open,
        "FILLED" | "EXPIRED" => OrderStatus::Closed,
        "CANCELED" => OrderStatus::Canceled,
        _ => return Err("unknown order status".into()),
    };
    let kind = match raw.kind.as_ref() {
        "LIMIT" => OrderType::Limit,
        "MARKET" => OrderType::Market,
        "STOP_LIMIT" | "STOP_MARKET" => OrderType::StopLoss,
        "TAKE_PROFIT_LIMIT" | "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        _ => return Err("unknown order type".into()),
    };
    let side = side(&raw.side).ok_or("unknown order side")?;
    let amount = raw.size.ok_or("missing order size")?;
    let filled = raw.cum_success_fill_size.unwrap_or(0.0);
    let price = raw
        .trigger_price
        .filter(|n| *n > 0.0)
        .or(raw.price)
        .ok_or("missing order price")?;
    if raw.id.is_empty() || amount < 0.0 || filled < 0.0 || filled > amount || price < 0.0 {
        return Err("invalid order quantities".into());
    }
    let time_in_force = if raw.post_only {
        Some(TimeInForce::PostOnly)
    } else {
        match raw.time_in_force.as_ref() {
            "GOOD_TIL_CANCEL" => Some(TimeInForce::GoodTillCancel),
            "IMMEDIATE_OR_CANCEL" => Some(TimeInForce::ImmediateOrCancel),
            "FILL_OR_KILL" => Some(TimeInForce::FillOrKill),
            "POST_ONLY" => Some(TimeInForce::PostOnly),
            _ => None,
        }
    };
    let client_order_id = if !raw.client_order_id.is_empty() {
        Some(raw.client_order_id.to_string())
    } else if !raw.client_id.is_empty() {
        Some(raw.client_id.to_string())
    } else {
        None
    };
    let order = Order {
        id: raw.id.clone(),
        parent_id: None,
        status,
        symbol: market.symbol.clone(),
        kind,
        side,
        price,
        amount,
        filled,
        remaining: subtract(amount, filled),
        reduce_only: raw.reduce_only,
        close: None,
        client_order_id,
        time_in_force,
        last_update: now,
        trigger_type: if matches!(kind, OrderType::StopLoss | OrderType::TakeProfit) {
            trigger(&raw.trigger_price_type)
        } else {
            None
        },
        take_profit: None,
        stop_loss: None,
        extra: Some(OrderExtra {
            sl_trigger_type: if kind == OrderType::StopLoss {
                trigger(&raw.trigger_price_type)
            } else {
                None
            },
            tp_trigger_type: if kind == OrderType::TakeProfit {
                trigger(&raw.trigger_price_type)
            } else {
                None
            },
            ..Default::default()
        }),
    };
    let mut rows = vec![order];
    for (leg, kind, suffix) in [
        (
            raw.open_sl_params.as_ref().or(raw.open_sl_param.as_ref()),
            OrderType::StopLoss,
            "__stop_loss",
        ),
        (
            raw.open_tp_params.as_ref().or(raw.open_tp_param.as_ref()),
            OrderType::TakeProfit,
            "__take_profit",
        ),
    ] {
        if let Some(leg) = leg
            && let Some(price) = leg.trigger_price.filter(|n| *n > 0.0)
        {
            let mut child = rows[0].clone();
            child.id = format!("{}{suffix}", raw.id);
            child.parent_id = Some(raw.id.clone());
            child.kind = kind;
            child.side = side.opposite();
            child.price = price;
            child.filled = 0.0;
            child.remaining = amount;
            child.trigger_type = trigger(&leg.trigger_price_type);
            child.extra = Some(OrderExtra {
                sl_trigger_type: if kind == OrderType::StopLoss {
                    child.trigger_type
                } else {
                    None
                },
                tp_trigger_type: if kind == OrderType::TakeProfit {
                    child.trigger_type
                } else {
                    None
                },
                ..Default::default()
            });
            rows.push(child);
        }
    }
    Ok(rows)
}

impl ApexOmni {
    fn map_position(
        &self,
        store: &Store,
        raw: &wire::PositionRow<'_>,
        now: f64,
    ) -> Result<Option<Position>, String> {
        if raw.symbol.trim().is_empty() {
            return Err("missing position symbol".into());
        }
        let Some(market) = store
            .market_by_id(&raw.symbol)
            .or_else(|| store.market(&raw.symbol))
        else {
            return if raw.size == Some(0.0) {
                Ok(None)
            } else {
                Err("ApeX owned position has unavailable market metadata".into())
            };
        };
        let side = match raw.side.as_ref() {
            "LONG" => PositionSide::Long,
            "SHORT" => PositionSide::Short,
            _ => return Err("unknown position side".into()),
        };
        let id = position_id(&market.symbol, side);
        let contracts = raw.size.ok_or("missing position size")?;
        let entry_price = raw
            .avg_price
            .filter(|n| *n != 0.0)
            .or(raw.entry_price)
            .unwrap_or(0.0);
        if contracts < 0.0 || entry_price < 0.0 || !(contracts * entry_price).is_finite() {
            return Err("invalid position quantities".into());
        }
        let rate = raw
            .custom_imr
            .filter(|n| *n > 0.0)
            .or(raw.custom_initial_margin_rate.filter(|n| *n > 0.0));
        let leverage = rate
            .map(|r| 1.0 / r)
            .or_else(|| store.position(&id).map(|p| p.leverage))
            .or_else(|| {
                store
                    .setting(&market.symbol)
                    .and_then(|setting| match side {
                        PositionSide::Long => setting.buy_leverage,
                        PositionSide::Short => setting.sell_leverage,
                    })
            })
            .unwrap_or_else(|| {
                self.contracts
                    .get(&market.symbol)
                    .map(|c| 1.0 / c.default_initial_margin_rate)
                    .unwrap_or(0.0)
            })
            .round();
        if !leverage.is_finite() {
            return Err("invalid position leverage".into());
        }
        let unrealized_pnl = raw
            .unrealised_pnl
            .or_else(|| store.position(&id).map(|p| p.unrealized_pnl))
            .filter(|value| value.is_finite())
            .unwrap_or(0.0);
        Ok(Some(Position {
            id,
            symbol: market.symbol.clone(),
            side,
            entry_price,
            notional: contracts * entry_price,
            leverage,
            margin_type: Some(MarginType::Cross),
            position_mode: Some(PositionMode::Hedged),
            unrealized_pnl,
            realized_pnl: Some(raw.cur_realised_pnl.or(raw.realized_pnl).unwrap_or(0.0)),
            contracts,
            liquidation_price: raw.liq_price.unwrap_or(0.0),
            margin_size: None,
            last_update: Some(now),
        }))
    }

    fn placeholders(&self, store: &Store, positions: &mut Vec<Position>) {
        for market in store.markets() {
            let default_leverage = self
                .contracts
                .get(&market.symbol)
                .map(|c| 1.0 / c.initial_margin_rate)
                .filter(|n| n.is_finite() && *n > 0.0)
                .unwrap_or(10.0);
            for side in [PositionSide::Long, PositionSide::Short] {
                let leverage = store
                    .setting(&market.symbol)
                    .and_then(|setting| match side {
                        PositionSide::Long => setting.buy_leverage,
                        PositionSide::Short => setting.sell_leverage,
                    })
                    .filter(|leverage| leverage.is_finite() && *leverage > 0.0)
                    .unwrap_or(default_leverage);
                let id = position_id(&market.symbol, side);
                if !positions.iter().any(|p| p.id == id) {
                    positions.push(Position {
                        id,
                        symbol: market.symbol.clone(),
                        side,
                        entry_price: 0.0,
                        notional: 0.0,
                        leverage,
                        margin_type: Some(MarginType::Cross),
                        position_mode: Some(PositionMode::Hedged),
                        unrealized_pnl: 0.0,
                        realized_pnl: None,
                        contracts: 0.0,
                        liquidation_price: 0.0,
                        margin_size: None,
                        last_update: None,
                    });
                }
            }
        }
    }

    pub(crate) fn start_private(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started {
            return;
        }
        self.account.started = true;
        if self.options.public_data_only || self.options.identity.is_none() {
            cx.store.set_positions(vec![]);
            cx.store.set_orders(vec![]);
            cx.store.set_balance(Balance::default());
            return;
        }
        // A missing or unusable zkLink role only blocks order signing (each
        // order's capability check refuses); monitoring and cancels still work.
        // Trusted setup already enforced the exact 65-byte seed.
        if let Err(error) = cx
            .can_sign(
                safe_cex_engine::CredentialSlot::Primary,
                &sign::CONTRACT_NEEDS[1..],
            )
            .map_err(|_| "Invalid OmniKey seed: expected 65-byte Ethereum signature")
        {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                format!(
                    "{error}; ApeX order placement is disabled until the OmniKey seed is fixed"
                ),
            );
        }
        if self
            .options
            .identity
            .as_ref()
            .is_none_or(|identity| identity.passphrase.is_none())
        {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "ApeX private account requires an API passphrase",
            );
            return;
        }
        cx.store.set_hedged(true);
        self.connect_private(cx);
        self.refresh_account(cx);
        self.account.tick = Some(cx.set_poll_timer(self.tick_interval()));
    }

    fn tick_interval(&self) -> f64 {
        self.options
            .tick_interval_ms
            .filter(|n| n.is_finite() && *n > 0.0)
            .unwrap_or(2000.0)
    }

    fn connect_private(&mut self, cx: &mut Cx<'_, Pending>) {
        let host = if self.options.testnet {
            "qa-quote.omni.apex.exchange"
        } else {
            "quote.omni.apex.exchange"
        };
        self.account.generation += 1;
        self.account.authenticated = false;
        self.account.supervisor.connecting(cx);
        self.account.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new(format!(
                    "wss://{host}/realtime_private?v=2&timestamp={}&requestId=rust-{}",
                    cx.now as u64, self.account.generation
                ))
                .channel(SocketChannel::Private),
            ),
        );
    }

    pub(crate) fn account_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.supervisor.opened(cx);
        let Some(identity) = self.options.identity.clone() else {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "ApeX API key, signing key and passphrase are required",
            );
            cx.close(socket);
            self.account_close(cx, socket);
            return true;
        };
        self.account.open = true;
        self.account.opened_at = cx.now;
        // The provider signs the login at dispatch; it precedes the sends below.
        let host = if self.options.testnet {
            "qa-quote.omni.apex.exchange"
        } else {
            "quote.omni.apex.exchange"
        };
        cx.send_signed(
            socket,
            String::new(),
            Box::new(sign::WsLogin::new(
                identity,
                format!("wss://{host}/realtime_private"),
            )),
        );
        // The production proxy expects all three subscriptions before a login ACK.
        cx.send(
            socket,
            r#"{"op":"subscribe","args":["ws_zk_accounts_v3","ws_user_v1","ws_notify_v1"]}"#,
        );
        cx.send(socket, r#"{"op":"ping"}"#);
        self.account.ping_at = cx.now;
        self.refresh_account(cx);
        true
    }

    pub(crate) fn account_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.socket = None;
        self.account.open = false;
        self.account.supervisor.retry(cx);
        true
    }

    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        match self.account.supervisor.on_timer(cx, timer) {
            Tick::Unhandled => (),
            Tick::Reconnect => {
                self.connect_private(cx);
                return true;
            }
            Tick::Ping => {
                if let Some(socket) = self.account.socket {
                    cx.send(socket, r#"{"op":"ping"}"#);
                    self.account.ping_at = cx.now;
                }
                return true;
            }
            Tick::TimedOut(_) => {
                if let Some(socket) = self.account.socket {
                    cx.close(socket);
                    self.account_close(cx, socket);
                }
                return true;
            }
            Tick::Handled => return true,
        }
        if self.account.tick == Some(timer) {
            self.account.tick = None;
            self.refresh_account(cx);
            self.account.tick = Some(cx.set_poll_timer(self.tick_interval()));
            return true;
        }
        if self.account.orders_heal == Some(timer) {
            self.account.orders_heal = None;
            self.heal_orders(cx);
            return true;
        }
        if self.account.positions_heal == Some(timer) {
            self.account.positions_heal = None;
            self.refresh_positions(cx);
            return true;
        }
        false
    }

    pub(crate) fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.snapshot.is_some()
            || self.account.positions_pending
            || self.account.orders_pending
        {
            return;
        }
        self.account.generation += 1;
        let generation = self.account.generation;
        self.account.snapshot = Some(Snapshot {
            generation,
            revision: self.account.revision,
            positions: None,
            orders: None,
            metadata: None,
            orders_release: false,
        });
        self.request_positions(cx, Some(generation));
        self.request_orders(cx, Some(generation));
    }

    pub(crate) fn refresh_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.positions_pending {
            self.request_positions(cx, None);
        }
    }
    /// Idempotent: the engine re-asks while a placement stays uncertain. A read already in
    /// flight may predate the mark, so it can no longer release; one fresh read is owed.
    pub(crate) fn reconcile_uncertain_placement(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.reconcile_owed = true;
        self.account.order_read_fresh = false;
        self.heal_orders(cx);
    }
    pub(crate) fn heal_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.orders_pending {
            self.request_orders(cx, None);
        }
    }

    fn request_positions(&mut self, cx: &mut Cx<'_, Pending>, generation: Option<u64>) {
        self.account.positions_pending = true;
        let pending = Request::Positions {
            generation,
            revision: self.account.revision,
            rows: SnapshotBuilder::default(),
        };
        self.snapshot_read(
            cx,
            "/v3/account",
            Some(ArrayAt::Path(&["data", "positions"])),
            pending,
        );
    }

    fn request_orders(&mut self, cx: &mut Cx<'_, Pending>, generation: Option<u64>) {
        self.account.orders_pending = true;
        self.account.order_read_fresh = std::mem::take(&mut self.account.reconcile_owed);
        let pending = Request::Orders {
            generation,
            revision: self.account.revision,
            rows: SnapshotBuilder::default(),
        };
        self.snapshot_read(cx, "/v3/open-orders", Some(ArrayAt::Field("data")), pending);
    }

    pub(crate) fn request_balance(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.balance_pending {
            return;
        }
        self.account.balance_pending = true;
        self.snapshot_read(cx, "/v3/account-balance", None, Request::Balance);
    }

    /// One Background Read per snapshot leg; the native generation stays the join.
    /// A refused selection, admission or queue settles the leg before any send.
    fn snapshot_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        at: Option<ArrayAt>,
        request: Request,
    ) {
        let selected = SelectedRead::select(&self.slots, ReadRole::Read { scope: None });
        let policy = ReadPolicy {
            timeout_ms: read::TIMEOUT_MS,
            pacing: None,
            retry: RetryPolicy::ReadFailure,
        };
        let read = match SelectedRead::background(cx, selected, policy) {
            Ok(read) => read,
            Err(error) => {
                let error = match error {
                    BackgroundReadError::Selection(_) => read::MISSING.to_owned(),
                    BackgroundReadError::Admission(error) => error.read_venue_error().message,
                };
                self.snapshot_refused(cx, request, error);
                return;
            }
        };
        self.send_read(
            cx,
            read::Custody::Admitted(read),
            sign::Request::get(path, vec![]),
            at,
            read::Native::Account(request),
        );
    }

    fn snapshot_refused(&mut self, cx: &mut Cx<'_, Pending>, leg: Request, error: String) {
        let snapshot = match leg {
            Request::Positions { .. } => {
                self.account.positions_pending = false;
                true
            }
            Request::Orders { .. } => {
                self.account.orders_pending = false;
                true
            }
            Request::Balance => {
                self.account.balance_pending = false;
                false
            }
            Request::Identity => return,
        };
        if snapshot {
            self.account.snapshot = None;
        }
        cx.notice(Level::Error, code::CONFIG_INVALID, error);
    }

    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        if self.public.has_unlisted() {
            let kind = match request {
                Request::Positions { .. } => "position",
                _ => "order",
            };
            self.note_unlisted(cx, &wire::row_key(item).symbol, kind);
        }
        match request {
            Request::Positions { rows, .. } => {
                match serde_json::from_str::<wire::PositionRow<'_>>(item)
                    .map_err(|e| e.to_string())
                    .and_then(|raw| {
                        if raw.size.is_some_and(|size| size > 0.0) && raw.unrealised_pnl.is_none() {
                            cx.notice(Level::Warn, code::VALUATION_UNAVAILABLE,
                                "ApeX position PnL unavailable; retaining prior valuation",
                            );
                        }
                        self.map_position(cx.store, &raw, cx.now)
                    }) {
                    Ok(Some(row)) => rows.push(row),
                    Ok(None) => {}
                    Err(_) => {
                        let key = wire::row_key(item);
                        let symbol = cx
                            .store
                            .market_by_id(&key.symbol)
                            .or_else(|| cx.store.market(&key.symbol))
                            .map_or(key.symbol.as_str(), |market| market.symbol.as_str())
                            .to_owned();
                        let legs = [PositionSide::Long, PositionSide::Short]
                            .map(|side| position_id(&symbol, side))
                            .into();
                        reject(cx, rows, "position", &symbol, legs);
                    }
                }
            }
            Request::Orders { rows, .. } => {
                match serde_json::from_str::<wire::OrderRow<'_>>(item)
                    .map_err(|e| e.to_string())
                    .and_then(|raw| {
                        if cx.store.market_by_id(&raw.symbol)
                            .or_else(|| cx.store.market(&raw.symbol)).is_none() {
                            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "ApeX owned order has unavailable market metadata; preserving previous orders");
                            return Err("unknown owned order market".into());
                        }
                        map_order(cx.store, &raw, cx.now)
                    })
                {
                    Ok(mapped) => mapped.into_iter().for_each(|order| rows.push(order)),
                    Err(_) => {
                        let id = wire::row_key(item).id;
                        let ids = vec![id.clone()];
                        reject(cx, rows, "order", &id, ids);
                    }
                }
            }
            Request::Balance | Request::Identity => {}
        }
    }

    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        context: &Context,
        response: Response<'_>,
    ) {
        let data = serde_json::from_str::<wire::Envelope<'_>>(response.body)
            .ok()
            .map(|e| e.data);
        match request {
            Request::Identity => {
                let id = data
                    .and_then(|data| serde_json::from_str::<wire::Identity>(data.get()).ok())
                    .map(|identity| identity.id)
                    .filter(|id| !id.is_empty());
                match id {
                    Some(id) => {
                        #[derive(Serialize)]
                        #[serde(rename_all = "camelCase")]
                        struct Identity {
                            user_id: String,
                            affiliate_id: Option<String>,
                        }
                        {
                            let result: Result<String, String> = serde_json::to_string(&Identity {
                                user_id: id,
                                affiliate_id: None,
                            })
                            .map_err(|e| e.to_string());
                            let _ = match result {
                                Ok(data) => cx.done_context(context, data),
                                Err(error) => cx.fail_context(context, error),
                            };
                        };
                    }
                    None => {
                        let _ = cx.fail_context(
                            context,
                            "ApeX account identity payload missing or invalid",
                        );
                    }
                }
            }
            Request::Positions {
                generation,
                revision,
                rows,
            } => {
                self.account.positions_pending = false;
                let metadata = data
                    .and_then(|data| serde_json::from_str::<wire::Metadata<'_>>(data.get()).ok());
                let Some(metadata) =
                    metadata.filter(|m| m.positions.get().starts_with('[') && !m.id.is_empty())
                else {
                    self.snapshot_failed(cx, generation, "invalid account snapshot");
                    return;
                };
                let rows = rows.commit(cx.store.positions().iter().cloned(), |p| p.id.clone());
                let fees = metadata
                    .contract_account
                    .map(|f| (f.maker_fee_rate, f.taker_fee_rate))
                    .unwrap_or_else(|| {
                        (
                            self.account.maker_fee.clone(),
                            self.account.taker_fee.clone(),
                        )
                    });
                if let Some(generation) = generation {
                    if let Some(snapshot) = self
                        .account
                        .snapshot
                        .as_mut()
                        .filter(|s| s.generation == generation)
                    {
                        snapshot.positions = Some(rows);
                        snapshot.metadata = Some((metadata.id, fees.0, fees.1));
                    }
                    self.commit_snapshot(cx);
                } else {
                    self.commit_positions(cx, rows, revision);
                    self.account.account_id = metadata.id;
                    self.account.maker_fee = fees.0;
                    self.account.taker_fee = fees.1;
                }
            }
            Request::Orders {
                generation,
                revision,
                rows,
            } => {
                self.account.orders_pending = false;
                if revision < self.account.snapshot_floor {
                    self.snapshot_failed(
                        cx,
                        generation,
                        "order snapshot predates retired terminal guards",
                    );
                    return;
                }
                if !data.is_some_and(|data| data.get().starts_with('[')) {
                    self.snapshot_failed(cx, generation, "invalid orders snapshot");
                    return;
                }
                // A clean read started after the mark releases the fence once published.
                let release =
                    rows.rejected() == 0 && std::mem::take(&mut self.account.order_read_fresh);
                // Bracket children share their parent's identity.
                let rows = rows.commit(cx.store.orders().iter().cloned(), |o| {
                    base_id(&o.id).to_owned()
                });
                if let Some(generation) = generation {
                    if let Some(snapshot) = self
                        .account
                        .snapshot
                        .as_mut()
                        .filter(|s| s.generation == generation)
                    {
                        snapshot.orders = Some(rows);
                        snapshot.orders_release = release;
                    }
                    self.commit_snapshot(cx);
                } else {
                    self.commit_orders(cx, rows, revision);
                    if release {
                        cx.orders_reconciled(None);
                    }
                }
                if self.account.reconcile_owed {
                    self.heal_orders(cx);
                }
            }
            Request::Balance => {
                self.account.balance_pending = false;
                let Some(balance) =
                    data.and_then(|data| serde_json::from_str::<wire::BalanceRow>(data.get()).ok())
                else {
                    cx.notice(
                        Level::Warn,
                        code::STREAM_MALFORMED,
                        "ApeX balance payload missing or invalid; keeping last balance",
                    );
                    return;
                };
                // Live last price values positions as in TS (native PnL lags); only a
                // missing, zero or non-finite quote falls back to reported position PnL.
                let upnl = cx.store.positions().iter().try_fold(0.0, |sum, p| {
                    let pnl = cx
                        .store
                        .ticker(&p.symbol)
                        .map(|t| t.last)
                        .filter(|last| last.is_finite() && *last > 0.0)
                        .map_or(p.unrealized_pnl, |last| {
                            let direction = if p.side == PositionSide::Long {
                                1.0
                            } else {
                                -1.0
                            };
                            (last - p.entry_price) * p.contracts * direction
                        });
                    let value = sum + pnl;
                    value.is_finite().then_some(value)
                });
                let Some(upnl) = upnl else {
                    cx.notice(
                        Level::Warn,
                        code::VALUATION_UNAVAILABLE,
                        "ApeX position PnL invalid; retaining previous balance",
                    );
                    return;
                };
                cx.store.set_balance(Balance {
                    total: balance.total_equity_value - upnl,
                    used: balance.maintenance_margin,
                    free: balance.total_equity_value - balance.initial_margin,
                    upnl,
                });
            }
        }
    }

    fn snapshot_failed(&mut self, cx: &mut Cx<'_, Pending>, generation: Option<u64>, error: &str) {
        if generation.is_some_and(|generation| {
            self.account
                .snapshot
                .as_ref()
                .is_some_and(|s| s.generation == generation)
        }) {
            self.account.snapshot = None;
        }
        cx.notice(
            Level::Warn,
            code::REQUEST_FAILED,
            format!("ApeX private reconciliation: {error}; retaining last state"),
        );
    }

    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        context: &Context,
        error: VenueError,
    ) {
        let generation = match request {
            Request::Identity => {
                let _ = cx.fail_context(context, error);
                return;
            }
            Request::Positions { generation, .. } => {
                self.account.positions_pending = false;
                generation
            }
            Request::Orders { generation, .. } => {
                self.account.orders_pending = false;
                generation
            }
            Request::Balance => {
                self.account.balance_pending = false;
                None
            }
        };
        self.snapshot_failed(cx, generation, &error.message);
    }

    fn commit_snapshot(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self
            .account
            .snapshot
            .as_ref()
            .is_some_and(|s| s.positions.is_some() && s.orders.is_some())
        {
            return;
        }
        let snapshot = self.account.snapshot.take().expect("complete snapshot");
        if snapshot.revision < self.account.snapshot_floor {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "ApeX snapshot predates retired terminal guards; retaining last state",
            );
            return;
        }
        self.commit_positions(
            cx,
            snapshot.positions.expect("positions"),
            snapshot.revision,
        );
        self.commit_orders(cx, snapshot.orders.expect("orders"), snapshot.revision);
        if snapshot.orders_release {
            cx.orders_reconciled(None);
        }
        if let Some((id, maker, taker)) = snapshot.metadata {
            self.account.account_id = id;
            self.account.maker_fee = maker;
            self.account.taker_fee = taker;
        }
        self.account.initialized = true;
        // Queued only after the completing snapshot leg retires.
        self.follow_ups.push(crate::read::FollowUp::Balance);
    }

    fn commit_positions(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut rows: Vec<Position>,
        revision: u64,
    ) {
        for row in &mut rows {
            if let Some((version, at, last)) = self.account.position_versions.get(&row.id)
                && (*version > revision || cx.now - at < 1000.0)
            {
                *row = cx.store.position(&row.id).unwrap_or(last).clone();
            }
        }
        for current in cx.store.positions() {
            if !rows.iter().any(|row| row.id == current.id)
                && self
                    .account
                    .position_versions
                    .get(&current.id)
                    .is_some_and(|(version, at, _)| *version > revision || cx.now - at < 1000.0)
            {
                rows.push(current.clone());
            }
        }
        self.placeholders(cx.store, &mut rows);
        for row in &mut rows {
            if let Some((version, at, leverage)) = self.account.leverage_versions.get(&row.symbol)
                && (*version > revision || cx.now - at < 5000.0)
            {
                row.leverage = *leverage;
            }
        }
        Self::apply_positions(cx, rows);
    }

    fn apply_positions(cx: &mut Cx<'_, Pending>, rows: Vec<Position>) {
        let mut symbols = HashSet::new();
        for position in &rows {
            let first = symbols.insert(&position.symbol);
            cx.store.update_setting(&position.symbol, |setting| {
                if first {
                    setting.leverage = position.leverage;
                }
                setting.margin_type = Some(MarginType::Cross);
                match position.side {
                    PositionSide::Long => setting.buy_leverage = Some(position.leverage),
                    PositionSide::Short => setting.sell_leverage = Some(position.leverage),
                }
            });
        }
        cx.store.set_positions(
            rows.into_iter()
                .filter(|position| position.contracts > 0.0)
                .collect(),
        );
    }

    fn commit_orders(&mut self, cx: &mut Cx<'_, Pending>, mut rows: Vec<Order>, revision: u64) {
        let versions = &self.account.order_versions;
        let newer = |id: &str| versions.get(base_id(id)).is_some_and(|v| *v > revision);
        // Rows read before a later local change never win, even if the store dropped the id.
        rows.retain(|row| !newer(&row.id));
        for current in cx.store.orders() {
            if newer(&current.id) && current.status != OrderStatus::Canceled {
                rows.push(current.clone());
            }
        }
        let now = cx.now;
        let lapsed = self
            .account
            .cancelling
            .extract_if(|id, at| now - *at >= CANCEL_GRACE_MS && !newer(id))
            .map(|(id, _)| id)
            .collect::<HashSet<_>>();
        for id in lapsed
            .iter()
            .filter(|id| rows.iter().any(|row| row.id == **id))
        {
            cx.notice(
                Level::Warn,
                code::ORDER_REJECTED,
                format!("ApeX order {id} is still open after its cancel was accepted"),
            );
        }
        rows.retain(|row| {
            let id = base_id(&row.id);
            !self.account.terminal.contains_key(id) && !self.account.cancelling.contains_key(id)
        });
        cx.store.set_orders(rows);
    }

    pub(crate) fn note_cancel(&mut self, cx: &mut Cx<'_, Pending>, ids: &[String]) {
        let mut remove = vec![];
        for id in ids {
            let id = base_id(id);
            self.account.note_cancel(id, cx.now);
            remove.extend([
                id.to_owned(),
                format!("{id}__stop_loss"),
                format!("{id}__take_profit"),
            ]);
        }
        cx.store
            .remove_orders(&remove.iter().map(String::as_str).collect::<Vec<_>>());
    }

    pub(crate) fn account_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Result<(), Command> {
        // The admitted context carries the command; its ID is not needed here.
        let Command::FetchAccount { .. } = command else {
            return Err(command);
        };
        let selected = if self.options.public_data_only {
            Err(CredentialUnavailable::Missing)
        } else {
            SelectedRead::select(&self.slots, ReadRole::Read { scope: None })
        };
        let policy = ReadPolicy {
            timeout_ms: read::TIMEOUT_MS,
            pacing: None,
            retry: RetryPolicy::ReadFailure,
        };
        let missing = LocalRefusal {
            diagnostic: None,
            error: VenueError::new("ApeX account identity requires API credentials"),
        };
        match SelectedRead::command(cx, selected, policy, missing) {
            Ok(read) => {
                self.send_read(
                    cx,
                    read::Custody::Admitted(read),
                    sign::Request::get("/v3/account", vec![]),
                    None,
                    read::Native::Account(Request::Identity),
                );
            }
            Err(CommandReadError::Entry(CommandEntryError::Revoked)) => {
                let _ = cx.reject_expired_command(Operation::Read);
            }
            Err(_) => {}
        }
        Ok(())
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
        self.account.supervisor.received(cx.now);
        let Ok(message) = serde_json::from_str::<wire::Message<'_>>(text) else {
            return true;
        };
        if message.op == "ping" || message.ping.is_some() {
            let args = message
                .args
                .map(|args| args.get())
                .filter(|args| args.starts_with('[') && *args != "[]")
                .map(str::to_owned)
                .unwrap_or_else(|| format!(r#"["{}"]"#, cx.now as u64));
            cx.send(socket, format!(r#"{{"op":"pong","args":{args}}}"#));
        }
        if message.op == "pong" || message.ret_msg == "pong" || message.pong.is_some() {
            cx.store
                .set_latency(((cx.now - self.account.ping_at).max(0.0) / 2.0).round());
        }
        if message.success == Some(false) {
            let rejected = if message.op == "login" {
                code::AUTH_REJECTED
            } else {
                code::SUBSCRIPTION_REJECTED
            };
            cx.notice(
                Level::Warn,
                rejected,
                format!("ApeX private socket rejected request: {}", message.ret_msg),
            );
            if message.op == "login" {
                cx.close(socket);
                self.account_close(cx, socket);
            }
            return true;
        }
        if message.op == "login" && message.success == Some(true) {
            self.account.authenticated = true;
            self.account.supervisor.authenticated(cx);
        }
        if message.topic != "ws_zk_accounts_v3" {
            return true;
        }
        let Some(contents) = message.contents else {
            return true;
        };
        if !self.account.authenticated
            && matches!(message.kind.as_ref(), "snapshot" | "delta")
            && self.private_auth_payload(cx.store, &contents, cx.now)
        {
            self.account.authenticated = true;
            self.account.supervisor.authenticated(cx);
        }
        let snapshot = message.kind == "snapshot";
        for (kind, skipped) in [
            ("fill", contents.fills.as_ref().map(|rows| &rows.skipped)),
            ("order", contents.orders.as_ref().map(|rows| &rows.skipped)),
            (
                "position",
                contents.positions.as_ref().map(|rows| &rows.skipped),
            ),
        ] {
            if let Some(skipped) = skipped.filter(|skipped| !skipped.is_empty()) {
                cx.notice(
                    Level::Warn,
                    code::ROW_SKIPPED,
                    format!(
                        "ApeX private stream skipped malformed {kind} rows ({})",
                        skipped.join(", ")
                    ),
                );
            }
        }
        if let Some(fills) = contents.fills {
            for fill in fills.rows {
                self.note_unlisted(cx, &fill.symbol, "fill");
                self.account_fill(cx, &fill, snapshot);
            }
        }
        if let Some(orders) = contents.orders {
            let canceled = orders.iter().any(|o| o.status == "CANCELED");
            for raw in orders.rows {
                self.note_unlisted(cx, &raw.symbol, "order");
                self.account_order(cx, &raw, snapshot);
            }
            if !canceled {
                if let Some(timer) = self.account.orders_heal.take() {
                    cx.clear_timer(timer);
                }
                self.account.orders_heal = Some(cx.set_timer(1000.0));
            }
        }
        if let Some(positions) = contents.positions {
            let mut rows = cx.store.positions().to_vec();
            for raw in positions.rows {
                self.note_unlisted(cx, &raw.symbol, "position");
                if raw.size.is_some_and(|size| size > 0.0) && raw.unrealised_pnl.is_none() {
                    cx.notice(
                        Level::Warn,
                        code::VALUATION_UNAVAILABLE,
                        "ApeX position PnL unavailable; retaining prior valuation",
                    );
                }
                let mapped = self.map_position(cx.store, &raw, cx.now);
                if let Err(error) = &mapped {
                    cx.notice(Level::Warn, code::ROW_SKIPPED, error);
                }
                if let Ok(Some(mut position)) = mapped {
                    if let Some((_, at, leverage)) =
                        self.account.leverage_versions.get(&position.symbol)
                        && cx.now - at < 5000.0
                    {
                        position.leverage = *leverage;
                    }
                    self.account.revision += 1;
                    self.account.position_versions.insert(
                        position.id.clone(),
                        (self.account.revision, cx.now, position.clone()),
                    );
                    if let Some(current) = rows.iter_mut().find(|row| row.id == position.id) {
                        *current = position;
                    } else {
                        rows.push(position);
                    }
                }
            }
            self.placeholders(cx.store, &mut rows);
            Self::apply_positions(cx, rows);
            if let Some(timer) = self.account.positions_heal.take() {
                cx.clear_timer(timer);
            }
            self.account.positions_heal = Some(cx.set_timer(1000.0));
        }
        true
    }

    fn fill_order_progress(&mut self, order_id: &str, qty: f64, now: f64) {
        let progress = self
            .account
            .progress
            .entry(order_id.to_owned())
            .or_default();
        let covered = progress.synthetic.min(qty);
        progress.synthetic -= covered;
        progress.topic += qty;
        progress.at = now;
        let mut remaining = covered;
        for execution in &mut self.account.recent_executions {
            if execution.synthetic
                && execution.order_id.as_deref() == Some(order_id)
                && remaining > 0.0
            {
                let reduction = execution.qty.min(remaining);
                execution.commission *= (execution.qty - reduction) / execution.qty;
                execution.qty -= reduction;
                remaining -= reduction;
            }
        }
        self.account
            .recent_executions
            .retain(|execution| execution.qty > 0.0);
    }

    fn private_auth_payload(&self, store: &Store, contents: &wire::Contents<'_>, now: f64) -> bool {
        let has_rows = contents.fills.is_some()
            || contents.orders.is_some()
            || contents.positions.is_some()
            || contents.wallets.is_some();
        let skipped = [
            contents.fills.as_ref().map(|rows| rows.skipped.len()),
            contents.orders.as_ref().map(|rows| rows.skipped.len()),
            contents.positions.as_ref().map(|rows| rows.skipped.len()),
            contents.wallets.as_ref().map(|rows| rows.skipped.len()),
        ]
        .into_iter()
        .flatten()
        .any(|count| count > 0);
        has_rows
            && !skipped
            && contents
                .wallets
                .as_ref()
                .is_none_or(|rows| rows.iter().all(wire::WalletRow::valid))
            && contents.orders.as_ref().is_none_or(|rows| {
                rows.iter().all(|row| {
                    !row.id.trim().is_empty()
                        && !row.symbol.trim().is_empty()
                        && side(&row.side).is_some()
                        && row.size.is_some_and(|size| size >= 0.0)
                        && row
                            .price
                            .or(row.trigger_price)
                            .is_some_and(|price| price >= 0.0)
                        && matches!(
                            row.status.as_ref(),
                            "PENDING" | "OPEN" | "UNTRIGGERED" | "FILLED" | "EXPIRED" | "CANCELED"
                        )
                        && matches!(
                            row.kind.as_ref(),
                            "LIMIT"
                                | "MARKET"
                                | "STOP_LIMIT"
                                | "STOP_MARKET"
                                | "TAKE_PROFIT_LIMIT"
                                | "TAKE_PROFIT_MARKET"
                        )
                        && map_order(store, row, now).is_ok()
                })
            })
            && contents.positions.as_ref().is_none_or(|rows| {
                rows.iter().all(|row| {
                    !row.symbol.trim().is_empty()
                        && matches!(row.side.as_ref(), "LONG" | "SHORT")
                        && row.size.is_some_and(|size| size >= 0.0)
                        && self.map_position(store, row, now).is_ok()
                })
            })
            && contents.fills.as_ref().is_none_or(|rows| {
                rows.iter().all(|row| {
                    !row.symbol.trim().is_empty()
                        && !row.order_id.trim().is_empty()
                        && side(&row.side).is_some()
                        && row.price.is_some_and(|price| price > 0.0)
                        && row.size.is_some_and(|size| size > 0.0)
                        && row
                            .created_at
                            .or(row.updated_at)
                            .is_some_and(|time| time > 0.0)
                })
            })
    }

    fn account_fill(&mut self, cx: &mut Cx<'_, Pending>, raw: &wire::FillRow<'_>, snapshot: bool) {
        let Some(market) = cx
            .store
            .market_by_id(&raw.symbol)
            .or_else(|| cx.store.market(&raw.symbol))
        else {
            return;
        };
        let (Some(side), Some(price), Some(qty), Some(time)) = (
            side(&raw.side),
            raw.price,
            raw.size,
            raw.created_at.or(raw.updated_at),
        ) else {
            return;
        };
        if price <= 0.0 || qty <= 0.0 || time <= 0.0 {
            return;
        }
        let id = if !raw.id.is_empty() {
            Some(raw.id.clone())
        } else if !raw.match_fill_id.is_empty() {
            Some(raw.match_fill_id.clone())
        } else {
            None
        };
        let key = id
            .as_ref()
            .map(|id| format!("fill:{id}"))
            .unwrap_or_else(|| format!("fill:{}:{time}:{price}:{qty}:{side:?}", raw.order_id));
        if !self.account.remember(key.clone()) {
            return;
        }
        let symbol = market.symbol.clone();
        let asset = if !raw.token.is_empty() {
            raw.token.to_string()
        } else if !raw.fee_currency.is_empty() {
            raw.fee_currency.to_string()
        } else {
            market.quote.clone()
        };
        // A fill without an order ID cannot be attributed to order progress.
        if !raw.order_id.is_empty() {
            self.fill_order_progress(&raw.order_id, qty, cx.now);
            if !snapshot {
                self.trading_fill_progress(&raw.order_id, qty, execution_milliseconds(time));
            }
        }
        self.account.cache(Execution {
            identity: key,
            timestamp: if time > 1e12 {
                (time / 1000.0).floor()
            } else {
                time.floor()
            },
            timestamp_ms: execution_milliseconds(time),
            symbol: symbol.clone(),
            order_id: (!raw.order_id.is_empty()).then(|| raw.order_id.clone()),
            price,
            qty,
            side,
            commission: raw.fee.unwrap_or(0.0).abs(),
            commission_asset: Some(asset),
            synthetic: false,
        });
        if !snapshot {
            cx.fill(Fill {
                id: None,
                order_id: raw.order_id.clone(),
                symbol,
                side,
                price,
                amount: qty,
            });
        }
        self.account.prune_progress();
    }

    fn account_order(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        raw: &wire::OrderRow<'_>,
        snapshot: bool,
    ) {
        let rows = match map_order(cx.store, raw, cx.now) {
            Ok(rows) => rows,
            Err(error) => {
                cx.notice(Level::Warn, code::ROW_SKIPPED, error);
                return;
            }
        };
        let Some(order) = rows.first() else {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "ApeX owned order has unavailable market metadata; preserving previous orders",
            );
            return;
        };
        if raw.status == "FILLED"
            && raw.price.unwrap_or(0.0) <= 0.0
            && raw.size.unwrap_or(0.0) <= 0.0
        {
            return;
        }
        self.trading_order_progress(
            &raw.id,
            raw.cum_success_fill_size.unwrap_or(0.0),
            matches!(raw.status.as_ref(), "FILLED" | "EXPIRED"),
            raw.updated_at
                .or(raw.updated_time)
                .map(execution_milliseconds)
                .unwrap_or(cx.now),
        );
        if self.account.terminal.contains_key(&raw.id) {
            return;
        }
        self.account.record_order(
            &raw.id,
            matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled)
                .then_some(order.status),
        );
        let cumulative = raw.cum_success_fill_size.unwrap_or(0.0);
        let progress = self.account.progress.entry(raw.id.clone()).or_default();
        let delta = (cumulative - progress.reported.max(progress.topic)).max(0.0);
        let fee = raw.cum_success_fill_fee.unwrap_or(0.0);
        let fee_delta = (fee - progress.fee).max(0.0);
        progress.reported = progress.reported.max(cumulative);
        progress.fee = fee;
        progress.at = cx.now;
        if snapshot {
            progress.topic = progress.topic.max(cumulative);
        } else if delta > 0.0 {
            progress.synthetic += delta;
        }
        if !snapshot && delta > 0.0 {
            let price = raw
                .latest_match_fill_price
                .filter(|n| *n > 0.0)
                .or(raw.average_price.filter(|n| *n > 0.0))
                .or(raw.price)
                .unwrap_or(0.0);
            if price > 0.0 {
                let time = raw
                    .updated_at
                    .or(raw.updated_time)
                    .or(raw.created_at)
                    .unwrap_or(cx.now);
                self.account.cache(Execution {
                    identity: format!("order:{}:{cumulative}:{time}", raw.id),
                    timestamp: if time > 1e12 {
                        (time / 1000.0).floor()
                    } else {
                        time.floor()
                    },
                    timestamp_ms: execution_milliseconds(time),
                    symbol: order.symbol.clone(),
                    order_id: Some(raw.id.clone()),
                    price,
                    qty: delta,
                    side: order.side,
                    commission: fee_delta,
                    commission_asset: cx.store.market(&order.symbol).map(|m| m.quote.clone()),
                    synthetic: true,
                });
            }
        }
        match order.status {
            OrderStatus::Canceled => {
                self.account.terminal.insert(
                    raw.id.clone(),
                    (self.account.revision, OrderStatus::Canceled),
                );
                let ids = [
                    raw.id.clone(),
                    format!("{}__stop_loss", raw.id),
                    format!("{}__take_profit", raw.id),
                ];
                cx.store
                    .remove_orders(&ids.iter().map(String::as_str).collect::<Vec<_>>());
                if !snapshot
                    && cx.now - self.account.opened_at > 5000.0
                    && raw.updated_at.is_some_and(|at| cx.now - at <= 5000.0)
                    && !matches!(
                        raw.cancel_reason.as_ref(),
                        "" | "UNKNOWN_ORDER_CANCEL_REASON" | "INTERNAL_FAILED" | "USER_CANCELED"
                    )
                {
                    cx.notice(
                        Level::Error,
                        code::ORDER_REJECTED,
                        raw.cancel_reason.as_ref(),
                    );
                }
            }
            OrderStatus::Closed => {
                self.account
                    .terminal
                    .insert(raw.id.clone(), (self.account.revision, OrderStatus::Closed));
                for id in [
                    raw.id.clone(),
                    format!("{}__stop_loss", raw.id),
                    format!("{}__take_profit", raw.id),
                ] {
                    cx.store.set_order_status(&id, OrderStatus::Closed);
                }
                for row in rows {
                    cx.store.upsert_order(row);
                }
            }
            // An open echo while a cancel is settling must not resurrect the order.
            _ if self.account.cancelling(&raw.id, cx.now) => {}
            _ => {
                for row in rows {
                    cx.store.upsert_order(row);
                }
            }
        }
        self.account.prune_progress();
    }
}
