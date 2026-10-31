//! The private side: socket auth and topics, the REST snapshots of orders,
//! positions and the unified wallet, and the reconciliation that keeps them
//! honest against the stream.
use crate::sign::Query;
use crate::wire::*;
use crate::{BybitLinear, Category, Channel, Pending, Rest, Then, venue_error};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::{RowAdmission, RowRepeat, RowReports, row_skipped};
use safe_cex_engine::execution::{Arithmetic, CumulativePrice, Cursor, Rejection, Report};
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Balance, CoinBalance, MarginType, OptionGreeks, Order, OrderExtra, OrderSide, OrderStatus,
    OrderType, Position, PositionGreeks, PositionMode, PositionSide, RejectReason, StopTriggerType,
    TimeInForce,
};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Cx, ErrorKind, Fill, Response, SocketId, Store, TimerId, VenueError, num};
use std::collections::{HashMap, HashSet, VecDeque};

/// Positions and the wallet are re-read this often; the stream carries
/// changes in between, so this only repairs what a dropped frame missed.
const RECONCILE_INTERVAL_MS: f64 = 5_000.0;
/// While the private topics are live, positions (with no position open) and the
/// wallet come from them; REST re-reads them only this often as a safety
/// reconcile (TS polled every tick).
const SAFETY_RECONCILE_MS: f64 = 60_000.0;
/// Consecutive Hyro reads the private stream may overtake before one is admitted.
const HYRO_OVERTAKEN_READS: u8 = 2;
const FILL_REPAIR_GRACE_MS: f64 = 8_000.0;
/// A failed order snapshot is retried this often until one succeeds.
const ORDERS_RETRY_MS: f64 = 15_000.0;
const ORDERS_PAGE_LIMIT: u32 = 50;
/// The window for a whole paged account read; each page keeps its own 20 s.
const ACCOUNT_READ_WINDOW_MS: u32 = 60_000;
const POSITIONS_PAGE_LIMIT: u32 = 200;
const EXECUTION_CACHE_LIMIT: usize = 4096;
const TERMINAL_ORDER_CACHE_LIMIT: usize = 4096;
/// Reject reasons that are the user's own cancel, not an error.
const QUIET_REJECT_REASONS: [&str; 2] = ["EC_PerCancelRequest", "EC_NoError"];

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RejectedIdentity {
    #[serde(default)]
    order_id: Option<String>,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    category: Option<String>,
}
fn rejected_identity(item: &str) -> Option<RejectedIdentity> {
    serde_json::from_str(item).ok()
}
fn parent_order_id(id: &str) -> &str {
    id.strip_suffix("__stop_loss")
        .or_else(|| id.strip_suffix("__take_profit"))
        .unwrap_or(id)
}

fn admitted_order_numbers(row: &OrderRow<'_>, hyro: bool) -> bool {
    !(hyro && !valid_hyro_order(row))
        && float_of(&row.qty).is_finite()
        && float_of(&row.qty) >= 0.0
        && order_status(&row.order_status).is_some()
        && [
            &row.price,
            &row.cum_exec_qty,
            &row.cum_exec_value,
            &row.leaves_qty,
            &row.avg_price,
            &row.trigger_price,
            &row.stop_loss,
            &row.take_profit,
        ]
        .into_iter()
        .all(optional_numeric)
}
fn admitted_position_numbers(row: &PositionRow<'_>, hyro: bool) -> bool {
    let size = float_of(&row.size);
    !((hyro && !valid_hyro_position(row))
        || !size.is_finite()
        || size < 0.0
        || (size > 0.0 && order_side(row.side.as_deref()).is_none())
        || !valid_position_greeks(row)
        || [
            &row.avg_price,
            &row.entry_price,
            &row.mark_price,
            &row.leverage,
            &row.unrealised_pnl,
            &row.position_value,
            &row.position_im,
            &row.cur_realised_pnl,
            &row.liq_price,
        ]
        .into_iter()
        .any(|value| !optional_numeric(value)))
}

pub enum Request {
    AccountInfo,
    Orders {
        generation: u32,
        coin: &'static str,
        cursor: String,
        rows: Vec<Mapped>,
        unsupported: Vec<String>,
        invalid: bool,
        rejected: RowAdmission<String>,
    },
    Positions {
        generation: u32,
        coin: &'static str,
        cursor: String,
        rows: Vec<MappedPosition>,
        invalid: bool,
        rejected: RowAdmission<String>,
    },
    Wallet {
        session: u32,
        epoch: u32,
    },
    /// Executions for one Demo reduce-only limit order whose stream events may never come.
    FillRepair {
        order_id: String,
        symbol: String,
        placed_at: f64,
        session: u32,
        cursor: Option<String>,
        filled: f64,
        pages: u8,
    },
}

/// One order row as the store holds it: the order and its attached
/// stop-loss and take-profit, which Bybit keeps on the order itself.
pub struct Mapped {
    pub orders: Vec<Order>,
    /// A position TP/SL (`tpslMode` Full or Partial), rather than an
    /// opening trigger. Full stops have zero amount in the store.
    pub position_bound: bool,
    fill_cursor: Option<Cursor>,
    terminal: bool,
    /// A spot TP/SL classified with neither a trigger direction nor a price
    /// to compare against; reclassified once tickers are seeded.
    unpriced_tpsl: bool,
}

pub struct MappedPosition {
    pub position: Position,
    greeks: Option<OptionGreeks>,
    /// Placeholder rows repeat a symbol; the most informative one wins.
    score: u32,
    updated: f64,
}

struct PositionsJoin {
    /// One Background owner for every page and settle coin of this read.
    root: Option<safe_cex_engine::request_context::Context>,
    /// Its issued page legs, finished with the root so none outlives it.
    legs: Vec<safe_cex_engine::request_context::Context>,
    generation: u32,
    /// Position-event count when the reads were issued; a newer stream event
    /// makes the snapshot stale.
    version: u32,
    pending: usize,
    rows: Vec<MappedPosition>,
    failed: bool,
    rejected: RowAdmission<String>,
    cursors: HashSet<String>,
}

struct OrdersJoin {
    /// One Background owner for every page and settle coin of this read.
    root: Option<safe_cex_engine::request_context::Context>,
    /// Its issued page legs, finished with the root so none outlives it.
    legs: Vec<safe_cex_engine::request_context::Context>,
    generation: u32,
    revision: u32,
    issued_at: f64,
    pending: usize,
    rows: Vec<Mapped>,
    unsupported: Vec<String>,
    failed: bool,
    rejected: RowAdmission<String>,
    cursors: HashSet<String>,
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

struct HyroSnapshot {
    revision: u32,
    orders: Option<Vec<Mapped>>,
    positions: Option<Vec<MappedPosition>>,
    wallet: Option<Wallet>,
    coins: Vec<CoinBalance>,
}

#[derive(Default)]
pub struct AccountState {
    started: bool,
    unpriced_tpsl: HashSet<String>,
    unsupported_logged: RowReports<String>,
    /// Demo reduce-only repair reads due: order id, symbol, placement time.
    fill_repairs: HashMap<TimerId, (String, String, f64)>,
    /// A Hyro reconciliation read failed in transport or at the venue, not on content.
    hyro_transient: bool,
    hyro_transient_streak: u8,
    /// This snapshot had a read refused locally (engine admission or queue);
    /// it never reached Hyro, so it is no evidence against the session.
    hyro_local_refusal: bool,
    /// Bumped to orphan an in-flight wallet read without ending the session.
    wallet_epoch: u32,
    /// Per-coin holdings; `None` until a wallet read or push is accepted.
    coins: Option<Vec<CoinBalance>>,
    pub authed: bool,
    pub session_version: u32,
    pub orders_snapshot_version: u32,
    fill_cursors: HashMap<String, Cursor>,
    terminal_fills: VecDeque<String>,
    executions: HashSet<(String, String)>,
    recent_executions: VecDeque<(String, String)>,
    terminal_orders: HashSet<String>,
    recent_terminal_orders: VecDeque<String>,
    recent_fill_cursors: VecDeque<String>,
    ready: bool,
    revision: u32,
    handshake: Option<&'static str>,
    handshake_timer: Option<TimerId>,
    /// Admits the third read in a row that the private stream overtook.
    snapshot_guard: safe_cex_engine::fence::StaleGuard,
    snapshot: Option<HyroSnapshot>,
    pub margin_type: Option<MarginType>,
    /// `unifiedMarginStatus` 1 is a classic account: USDT only, CONTRACT wallet.
    classic: bool,
    tick_timer: Option<TimerId>,
    partial_stop_refresh: Option<TimerId>,
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
    /// The private topics are subscribed on an authenticated socket.
    topics_live: bool,
    /// When positions and the wallet were last read over REST.
    last_account_read_at: f64,
    /// Bybit permits different position modes on different symbols.
    pub position_modes: HashMap<String, bool>,
    pub position_bound: HashSet<String>,
    /// Frame-fed: the mode this engine switched to, held over the data source
    /// until its VENUE_STATE agrees (`SettingKey::Venue(MODE_HOLD, category)`).
    pub held_mode: Option<bool>,
}

impl AccountState {
    pub(crate) fn new() -> Self {
        Self {
            unsupported_logged: RowReports::unbounded(RowRepeat::Once),
            ..Self::default()
        }
    }

    pub(crate) fn has_hedged(&self) -> bool {
        self.position_modes.values().any(|hedged| *hedged)
    }
}

/// The engine reports a valid envelope missing its list through the failure path;
/// a Bybit error envelope there is a venue refusal, not bad content.
fn malformed_stream(message: &str) -> bool {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Refusal {
        ret_code: i64,
    }
    message
        .strip_prefix("response did not contain the expected list: ")
        .is_some_and(|body| {
            serde_json::from_str::<Refusal>(body).map_or(true, |refusal| refusal.ret_code == 0)
        })
}

fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

fn valid_order(order: &Order) -> bool {
    [order.price, order.amount, order.filled, order.remaining]
        .into_iter()
        .all(|v| v.is_finite() && v >= 0.0)
}

fn optional_numeric(value: &Option<Str<'_>>) -> bool {
    present(value).is_none_or(|value| float(value).is_finite())
}

fn valid_position_greeks(row: &PositionRow<'_>) -> bool {
    [&row.delta, &row.gamma, &row.vega, &row.theta]
        .into_iter()
        .all(optional_numeric)
}

pub(crate) fn valid_hyro_order(row: &OrderRow<'_>) -> bool {
    row.symbol.ends_with("USDT")
        && present(&row.category).is_none_or(|category| category == "linear")
        && !row.order_id.is_empty()
        && order_status(&row.order_status).is_some()
        && [&row.qty, &row.price]
            .into_iter()
            .all(|value| float_of(value).is_finite())
        && [
            &row.cum_exec_qty,
            &row.cum_exec_value,
            &row.leaves_qty,
            &row.avg_price,
            &row.trigger_price,
            &row.stop_loss,
            &row.take_profit,
        ]
        .into_iter()
        .all(optional_numeric)
}

fn valid_hyro_position(row: &PositionRow<'_>) -> bool {
    let size = float_of(&row.size);
    row.symbol.ends_with("USDT")
        && present(&row.category).is_none_or(|category| category == "linear")
        && size.is_finite()
        && size >= 0.0
        && row.position_idx.unwrap_or(0) == 0
        && float_of(&row.leverage) > 0.0
        && float_of(&row.leverage).is_finite()
        && [
            &row.avg_price,
            &row.mark_price,
            &row.unrealised_pnl,
            &row.position_value,
            &row.position_im,
            &row.cur_realised_pnl,
            &row.liq_price,
        ]
        .into_iter()
        .all(optional_numeric)
        && (size == 0.0
            || (order_side(row.side.as_deref()).is_some()
                && [&row.avg_price, &row.mark_price, &row.unrealised_pnl]
                    .into_iter()
                    .all(|value| float_of(value).is_finite())))
}

/// The first account of a wallet-balance body, verbatim.
fn raw_wallet_account(body: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Envelope<'a> {
        #[serde(borrow)]
        result: Option<Result<'a>>,
    }
    #[derive(serde::Deserialize)]
    struct Result<'a> {
        #[serde(borrow)]
        list: Vec<&'a serde_json::value::RawValue>,
    }
    let envelope: Envelope<'_> = serde_json::from_str(body).ok()?;
    Some(envelope.result?.list.first()?.get().to_owned())
}

fn valid_hyro_wallet(row: &WalletAccount<'_>) -> bool {
    row.account_type.as_deref() == Some("UNIFIED")
        && (float_of(&row.total_equity).is_finite()
            || float_of(&row.total_wallet_balance).is_finite())
        && [
            &row.total_equity,
            &row.total_wallet_balance,
            &row.total_margin_balance,
            &row.total_available_balance,
            &row.total_initial_margin,
            &row.total_maintenance_margin,
        ]
        .into_iter()
        .all(optional_numeric)
        && row.coin.iter().all(|coin| {
            present(&coin.coin).is_some()
                && [
                    &coin.equity,
                    &coin.wallet_balance,
                    &coin.usd_value,
                    &coin.total_position_im,
                    &coin.total_order_im,
                    &coin.locked,
                    &coin.bonus,
                ]
                .into_iter()
                .all(optional_numeric)
        })
}

pub(crate) fn valid_snapshot_list(body: &str, cursor: &str) -> bool {
    #[derive(serde::Deserialize)]
    struct Envelope {
        result: List,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct List {
        list: Vec<serde::de::IgnoredAny>,
        #[serde(default)]
        next_page_cursor: Option<String>,
    }
    serde_json::from_str::<Envelope>(body).is_ok_and(|envelope| {
        let _ = envelope.result.list;
        envelope
            .result
            .next_page_cursor
            .is_none_or(|next| next.is_empty() || next != cursor)
    })
}

/// `a - b` rounded to the larger decimal count of the two, as the TS
/// `safe-math` subtract does, so float noise never reaches the store.
pub fn subtract(a: f64, b: f64) -> f64 {
    num::decimal_subtract(a, b)
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
        "PendingCancel" | "Cancelled" | "PartiallyFilledCanceled" => OrderStatus::Canceled,
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
pub(crate) fn is_inverse_symbol(symbol: &str) -> bool {
    let Some(at) = symbol.find("USD") else {
        return false;
    };
    let (base, rest) = (&symbol[..at], &symbol[at + 3..]);
    let suffix = rest.as_bytes();
    let expiry = suffix.len() == 3
        && b"FGHJKMNQUVXZ".contains(&suffix[0])
        && suffix[1..].iter().all(u8::is_ascii_digit);
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
/// The spot TP/SL kind from its trigger against the current last price, or
/// `None` while there is no price or the order is not a TP/SL.
fn priced_tpsl_kind(order: &Order, store: &Store) -> Option<OrderType> {
    let last = store.ticker(&order.symbol).map_or(0.0, |t| t.last);
    if last <= 0.0 || !matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit) {
        return None;
    }
    let below = order.price < last;
    Some(if (order.side == OrderSide::Sell) == below {
        OrderType::StopLoss
    } else {
        OrderType::TakeProfit
    })
}

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
    let spot_tpsl = stop_order_type == Some("tpslOrder");
    let last = store.ticker(&o.symbol).map_or(0.0, |ticker| ticker.last);
    let unpriced_tpsl = spot_tpsl && o.trigger_direction.is_none() && last <= 0.0;
    let spot_tpsl_kind = spot_tpsl.then(|| {
        let below = match o.trigger_direction {
            Some(1) => false,
            Some(2) => true,
            _ => trigger_price.map_or(0.0, float) < last,
        };
        if (o.side.as_deref() == Some("Sell")) == below {
            "StopLoss"
        } else {
            "TakeProfit"
        }
    });
    let price_text = if is_stop {
        trigger_price.or(o.price.as_deref())
    } else {
        o.price.as_deref()
    };
    let wire_type = from_create_type.or(spot_tpsl_kind).or(if is_stop {
        stop_order_type
    } else {
        present(&o.order_type)
    })?;
    let opening_trigger = is_stop
        && !spot_tpsl
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
    let inverse = is_inverse_symbol(&o.symbol);
    let extra = (inverse || reject == Some("EC_PostOnlyWillTakeLiquidity")).then(|| OrderExtra {
        remaining_native: inverse.then_some(remaining),
        remaining_notional: inverse.then_some(remaining),
        reject_reason: (reject == Some("EC_PostOnlyWillTakeLiquidity"))
            .then_some(RejectReason::PostOnly),
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
        remaining: if inverse {
            inverse_remaining(remaining, price_text.map_or(0.0, float))
        } else {
            remaining
        },
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
                remaining: if inverse {
                    inverse_remaining(remaining, level)
                } else {
                    amount
                },
                client_order_id: None,
                trigger_type: trigger(present(by)).or(parent.trigger_type),
                extra: inverse.then(|| OrderExtra {
                    remaining_native: Some(remaining),
                    remaining_notional: Some(remaining),
                    ..OrderExtra::default()
                }),
                ..parent.clone()
            });
        }
    }
    orders.insert(0, parent);
    Some(Mapped {
        orders,
        position_bound,
        unpriced_tpsl,
        terminal: matches!(
            o.order_status.as_ref(),
            "Cancelled" | "Filled" | "Deactivated" | "Rejected" | "PartiallyFilledCanceled"
        ),
        fill_cursor: Cursor::seed_with_average(
            filled,
            finite(float_of(&o.cum_exec_value)),
            finite(float_of(&o.avg_price)),
        )
        .ok(),
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
    let native = or_zero(&p.size);
    let inverse = is_inverse_symbol(&p.symbol);
    let option = Category::for_symbol(&p.symbol) == Category::Option;
    let ticker = store.ticker(&p.symbol);
    let reference = ticker
        .map(|ticker| ticker.mark)
        .filter(|mark| mark.is_finite() && *mark > 0.0)
        .or_else(|| {
            present(&p.mark_price)
                .map(float)
                .filter(|mark| mark.is_finite() && *mark > 0.0)
        })
        .or_else(|| {
            ticker
                .map(|ticker| ticker.last)
                .filter(|last| last.is_finite() && *last > 0.0)
        })
        .unwrap_or(entry_price);
    let contracts = if inverse && reference > 0.0 {
        num::round_half_up(native / reference * 1e8) / 1e8
    } else {
        native
    };
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
    if !option
        && !p.symbol.ends_with("USDT")
        && store.market(&p.symbol).is_some_and(|m| m.quote == "USDC")
        && let Some(ticker) = store.ticker(&p.symbol)
        && ticker.mark.is_finite()
        && ticker.mark > 0.0
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
        greeks: option
            .then(|| crate::options::greeks(&p.delta, &p.gamma, &p.vega, &p.theta))
            .flatten(),
        position: Position {
            id: position_id(&p.symbol, side, mode),
            symbol: p.symbol.to_string(),
            side,
            entry_price,
            notional: if inverse {
                native
            } else {
                or_zero(&p.position_value)
            },
            leverage: if option { 1.0 } else { float_of(&p.leverage) },
            margin_type: Some(margin_type),
            position_mode: mode,
            unrealized_pnl: unrealized,
            realized_pnl: Some(or_zero(&p.cur_realised_pnl)),
            contracts,
            liquidation_price: if option { 0.0 } else { or_zero(&p.liq_price) },
            margin_size: (margin > 0.0).then_some(margin),
            last_update: Some(now),
        },
        score,
        updated,
    }
}

/// TS `formatBalance`: amount is equity (else wallet balance), notional its USD value.
fn coin_balances(account: &WalletAccount<'_>) -> Vec<CoinBalance> {
    account
        .coin
        .iter()
        .filter_map(|coin| {
            let base = present(&coin.coin)?;
            let amount =
                finite(float_of(&coin.equity)).or(finite(float_of(&coin.wallet_balance)))?;
            Some(CoinBalance {
                base: base.to_owned(),
                amount,
                notional: finite(float_of(&coin.usd_value)).unwrap_or(amount),
            })
        })
        .collect()
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

fn inverse_remaining(native: f64, price: f64) -> f64 {
    if price > 0.0 {
        num::round_half_up(native / price * 1e8) / 1e8
    } else {
        native
    }
}

fn open_upnl(store: &Store) -> f64 {
    store
        .positions()
        .iter()
        .map(|p| {
            if is_inverse_symbol(&p.symbol) && p.contracts > 0.0 {
                p.unrealized_pnl * p.notional / p.contracts
            } else {
                p.unrealized_pnl
            }
        })
        .sum()
}

impl BybitLinear {
    fn remember_hyro_fill_cursor(&mut self, id: &str) {
        self.account.recent_fill_cursors.retain(|known| known != id);
        self.account.recent_fill_cursors.push_back(id.to_owned());
        while self.account.recent_fill_cursors.len() > 500 {
            self.account.recent_fill_cursors.pop_front();
        }
    }

    fn retain_hyro_fill_cursors(&mut self, active: &HashSet<&str>) {
        self.account.fill_cursors.retain(|id, _| {
            active.contains(id.as_str()) || self.account.recent_fill_cursors.contains(id)
        });
        self.account
            .terminal_fills
            .retain(|id| self.account.fill_cursors.contains_key(id));
    }

    fn store_hyro_fill_cursor(&mut self, cx: &Cx<'_, Pending>, id: &str, cursor: Cursor) {
        self.account.fill_cursors.insert(id.to_owned(), cursor);
        self.remember_hyro_fill_cursor(id);
        let active = cx
            .store
            .orders()
            .iter()
            .filter(|order| order.status == OrderStatus::Open)
            .map(|order| order.id.as_str())
            .collect();
        self.retain_hyro_fill_cursors(&active);
    }

    fn on_hyro_fill(&mut self, cx: &mut Cx<'_, Pending>, row: &OrderRow<'_>) -> bool {
        let quantity = or_zero(&row.cum_exec_qty);
        let previous = self
            .account
            .fill_cursors
            .get(row.order_id.as_ref())
            .copied()
            .unwrap_or_default();
        if quantity < previous.quantity()
            || (quantity == previous.quantity()
                && self
                    .account
                    .terminal_fills
                    .iter()
                    .any(|id| id == row.order_id.as_ref())
                && order_status(&row.order_status) == Some(OrderStatus::Open))
        {
            return false;
        }
        if matches!(
            row.order_status.as_ref(),
            "Filled" | "Cancelled" | "Deactivated" | "PartiallyFilledCanceled"
        ) {
            self.account.terminal_fills.retain(|id| id != &row.order_id);
            self.account
                .terminal_fills
                .push_back(row.order_id.to_string());
            while self.account.terminal_fills.len() > 500 {
                self.account.terminal_fills.pop_front();
            }
        }
        let report = Report::Cumulative {
            quantity,
            price: CumulativePrice::Value {
                cumulative: finite(float_of(&row.cum_exec_value)),
                fallback: finite(float_of(&row.avg_price))
                    .filter(|price| *price > 0.0)
                    .or_else(|| finite(float_of(&row.price)).filter(|price| *price > 0.0)),
            },
        };
        let prepared = match previous.prepare(report, Arithmetic::DecimalRounded) {
            Ok(Some(prepared)) => prepared,
            Ok(None) => {
                self.store_hyro_fill_cursor(cx, &row.order_id, previous);
                return true;
            }
            Err(Rejection::ValueRegressed) => {
                self.fail_hyro(cx, "private fill value regressed while quantity increased");
                return false;
            }
            Err(Rejection::QuantityRegressed) => return false,
            // A valid observation can update native order/protocol state while
            // leaving this unpriced execution eligible for a corrected report.
            Err(_) => return true,
        };
        let Some(side) = order_side(row.side.as_deref()) else {
            return true;
        };
        cx.fill(Fill {
            id: None,
            order_id: row.order_id.to_string(),
            symbol: row.symbol.to_string(),
            side,
            price: prepared.price(),
            amount: prepared.amount(),
        });
        self.store_hyro_fill_cursor(cx, &row.order_id, prepared.next());
        true
    }

    pub(crate) fn note_hyro_write(&mut self) {
        if self.variant.is_hyro() {
            self.account.revision += 1;
        }
    }

    pub(crate) fn schedule_partial_stop_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.variant == crate::Variant::Inverse {
            return;
        }
        if let Some(timer) = self.account.partial_stop_refresh.take() {
            cx.clear_timer(timer);
        }
        self.account.partial_stop_refresh = Some(cx.set_timer(250.0));
    }

    pub(crate) fn note_order_terminal(&mut self, ids: &[String]) {
        // Terminal rows can be removed, or share the snapshot issue timestamp.
        self.account.revision = self.account.revision.wrapping_add(1);
        for id in ids {
            self.remember_terminal_order(id);
        }
    }

    fn remember_terminal_order(&mut self, id: &str) {
        if !(self.variant == crate::Variant::Demo || self.variant.is_hyro() || self.brazil_site) {
            return;
        }
        self.cache_terminal_order(id);
    }

    pub(crate) fn note_partial_stop_canceled(&mut self, id: &str) {
        self.account.revision = self.account.revision.wrapping_add(1);
        // The replacement refresh can read a replica still serving the canceled leg.
        self.cache_terminal_order(id);
    }

    fn cache_terminal_order(&mut self, id: &str) {
        if !self.account.terminal_orders.insert(id.to_owned()) {
            return;
        }
        self.account.recent_terminal_orders.push_back(id.to_owned());
        while self.account.recent_terminal_orders.len() > TERMINAL_ORDER_CACHE_LIMIT {
            if let Some(old) = self.account.recent_terminal_orders.pop_front() {
                self.account.terminal_orders.remove(&old);
            }
        }
    }

    fn stale_terminal_snapshot(&mut self, mapped: &Mapped) -> bool {
        let parent = &mapped.orders[0];
        if mapped.terminal {
            self.remember_terminal_order(&parent.id);
            false
        } else {
            parent.status == OrderStatus::Open && self.account.terminal_orders.contains(&parent.id)
        }
    }

    pub(crate) fn private_ready(&self) -> bool {
        !self.variant.is_hyro() || self.account.ready
    }

    fn trades_row(&self, category: &Option<Str<'_>>, symbol: &str) -> bool {
        if let Some(category) = present(category) {
            return category == self.paths.category
                && (!self.variant.is_hyro() || symbol.ends_with("USDT"));
        }
        if self.variant.is_spot() {
            return false;
        }
        if self.category == Category::Option {
            return Category::for_symbol(symbol) == Category::Option;
        }
        if self.category == Category::Inverse {
            return is_inverse_symbol(symbol);
        }
        is_linear(category, symbol) && (!self.variant.is_hyro() || symbol.ends_with("USDT"))
    }

    fn map_account_position(
        &self,
        row: &PositionRow<'_>,
        store: &Store,
        now: f64,
    ) -> MappedPosition {
        let mut mapped = map_position(row, store, self.margin_type(), now);
        if self.variant.is_hyro() {
            mapped.position.id = row.symbol.to_string();
            mapped.position.position_mode = Some(PositionMode::Oneway);
        }
        mapped
    }

    pub(crate) fn on_private_closed(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(timer) = self.account.partial_stop_refresh.take() {
            cx.clear_timer(timer);
        }
        self.account.authed = false;
        self.account.topics_live = false;
        if !self.variant.is_hyro() {
            self.account.session_version += 1;
            self.account.orders_generation += 1;
            self.account.positions_generation += 1;
            self.drop_account_reads(cx);
            self.account.orders_follow_up = false;
            self.account.positions_follow_up = false;
            self.account.orders_stale = true;
            self.account.wallet_in_flight = false;
            self.account.wallet_follow_up = false;
            return;
        }
        self.account.session_version += 1;
        self.account.ready = false;
        self.account.handshake = None;
        self.account.snapshot = None;
        self.drop_account_reads(cx);
        self.account.wallet_in_flight = false;
        if let Some(timer) = self.account.tick_timer.take() {
            cx.clear_timer(timer);
        }
        self.account.orders_generation += 1;
        self.account.positions_generation += 1;
        if let Some(timer) = self.account.handshake_timer.take() {
            cx.clear_timer(timer);
        }
    }

    /// A periodic read that failed in transport keeps the healthy stream and the
    /// last good state; the next tick retries. Bad content still reconnects.
    fn hyro_read_failed(&mut self, cx: &mut Cx<'_, Pending>, message: &str) {
        let disposition = hyro_failure(
            self.account.ready,
            std::mem::take(&mut self.account.hyro_transient),
            self.account.hyro_local_refusal,
            self.account.hyro_transient_streak,
        );
        if let HyroFailure::Retry { counts } = disposition {
            if counts {
                self.account.hyro_transient_streak += 1;
            }
            cx.notice(
                Level::Warn,
                code::REQUEST_FAILED,
                format!("HyroTrader {message}; retrying"),
            );
            // Orphan this snapshot's other reads so they cannot fill the next one.
            self.account.snapshot = None;
            self.account.snapshot_guard = Default::default();
            self.drop_account_reads(cx);
            self.account.orders_generation += 1;
            self.account.positions_generation += 1;
            self.account.wallet_in_flight = false;
            self.account.wallet_epoch = self.account.wallet_epoch.wrapping_add(1);
            return;
        }
        self.fail_hyro(cx, message);
    }

    /// A read the engine refused before any I/O (admission or queue).
    pub(crate) fn note_local_read_refusal(&mut self) {
        if self.variant.is_hyro() {
            self.account.hyro_local_refusal = true;
        }
    }

    fn fail_hyro(&mut self, cx: &mut Cx<'_, Pending>, message: &str) {
        self.account.hyro_transient = false;
        self.account.hyro_transient_streak = 0;
        cx.notice(
            Level::Error,
            code::STREAM_UNAVAILABLE,
            format!("HyroTrader {message}"),
        );
        if let Some(feed) = self.private.feed {
            cx.retire_socket(
                feed,
                safe_cex_engine::socket_lifecycle::RetireReason::Authentication,
            );
        }
    }

    fn on_hyro_op(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Ok(frame) = serde_json::from_str::<OpFrame>(text) else {
            return;
        };
        let Some(expected) = self.account.handshake else {
            return;
        };
        let id = format!("hyro-{}-{expected}", self.account.session_version);
        if frame.op.as_deref() != Some(expected) || frame.req_id.as_deref() != Some(&id) {
            return;
        }
        if let Some(timer) = self.account.handshake_timer.take() {
            cx.clear_timer(timer);
        }
        if frame.success != Some(true) {
            self.fail_hyro(cx, "private handshake rejected");
            return;
        }
        if expected == "auth" {
            self.account.authed = true;
            self.account.handshake = Some("subscribe");
            self.account.handshake_timer = Some(cx.set_timer(10_000.0));
            if let Some(socket) = self.private.socket {
                cx.send(socket, format!(r#"{{"op":"subscribe","req_id":"hyro-{}-subscribe","args":["order","position"]}}"#, self.account.session_version));
            }
        } else {
            self.account.handshake = None;
            self.account.snapshot_guard = Default::default();
            self.start_hyro_snapshot(cx);
            self.account.tick_timer = Some(cx.set_poll_timer(RECONCILE_INTERVAL_MS));
        }
    }

    /// Reconcile an uncertain Hyro write with a fresh atomic snapshot; reads
    /// already out predate the write, so they are abandoned, not joined.
    pub(crate) fn reconcile_hyro_snapshot(&mut self, cx: &mut Cx<'_, Pending>) {
        // Before the subscribe ack, its own snapshot follows and reconciles.
        if !self.account.authed || self.account.handshake.is_some() {
            return;
        }
        self.drop_account_reads(cx);
        self.account.wallet_in_flight = false;
        self.account.wallet_epoch = self.account.wallet_epoch.wrapping_add(1);
        self.start_hyro_snapshot(cx);
    }

    fn start_hyro_snapshot(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.hyro_local_refusal = false;
        self.account.snapshot = Some(HyroSnapshot {
            revision: self.account.revision,
            orders: None,
            positions: None,
            wallet: None,
            coins: Vec::new(),
        });
        self.fetch_orders(cx);
        self.fetch_positions(cx);
        self.fetch_wallet(cx);
    }

    fn finish_hyro_snapshot(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(snapshot) = &self.account.snapshot else {
            return;
        };
        if snapshot.orders.is_none() || snapshot.positions.is_none() || snapshot.wallet.is_none() {
            return;
        }
        let snapshot = self.account.snapshot.take().expect("checked above");
        let moved = snapshot.revision != self.account.revision;
        if !self
            .account
            .snapshot_guard
            .admit(!moved, HYRO_OVERTAKEN_READS)
        {
            if self.account.ready {
                // Hyro streams no wallet, so order/position frames cannot make it stale.
                self.account.wallet = snapshot.wallet;
                self.account.coins = Some(snapshot.coins);
                self.account.hyro_transient_streak = 0;
                self.commit_balance(cx);
            }
            self.start_hyro_snapshot(cx);
            return;
        }
        // Steady frames must not turn reconciliation into a reconnect loop: the guard
        // admits the third overtaken read; the next tick (not an immediate re-read,
        // which steady frames would turn into continuous polling) corrects it.
        if moved {
            cx.notice(Level::Warn, code::REQUEST_FAILED,
                "HyroTrader private stream kept moving during reconciliation; using the latest snapshot",
            );
        }
        let mut orders = Vec::new();
        self.account.position_bound.clear();
        for mapped in snapshot.orders.expect("checked above") {
            if self.stale_terminal_snapshot(&mapped) {
                continue;
            }
            let id = mapped.orders[0].id.clone();
            // Recovery establishes the baseline. Healthy polls cannot consume
            // executions whose order events have not arrived yet.
            if !self.account.ready {
                let previous = self.account.fill_cursors.entry(id.clone()).or_default();
                // Admission rejects an invalid recovery seed before this commit.
                if let Some(seed) = mapped.fill_cursor {
                    *previous = previous.merge_seed(seed);
                }
                if mapped.orders[0].status != OrderStatus::Open {
                    self.account.terminal_fills.retain(|known| known != &id);
                    self.account.terminal_fills.push_back(id.clone());
                    while self.account.terminal_fills.len() > 500 {
                        self.account.terminal_fills.pop_front();
                    }
                }
            }
            self.remember_hyro_fill_cursor(&id);
            if mapped.position_bound {
                self.account
                    .position_bound
                    .insert(mapped.orders[0].id.clone());
            }
            orders.extend(mapped.orders);
        }
        let active: HashSet<_> = orders
            .iter()
            .filter(|order| order.status == OrderStatus::Open)
            .map(|order| order.id.as_str())
            .collect();
        self.retain_hyro_fill_cursors(&active);
        let positions = best_positions(snapshot.positions.expect("checked above"));
        let settings: Vec<_> = positions
            .iter()
            .filter_map(|row| {
                Some((
                    cx.store.market_index(&row.position.symbol)?,
                    MarketSetting {
                        leverage: row.position.leverage,
                        margin_type: Some(MarginType::Cross),
                        ..Default::default()
                    },
                ))
            })
            .collect();
        cx.store.set_settings(settings);
        cx.store.set_orders(orders);
        cx.store.set_positions(
            positions
                .into_iter()
                .map(|row| row.position)
                .filter(|position| position.contracts > 0.0)
                .collect(),
        );
        cx.store.set_hedged(false);
        self.account.wallet = snapshot.wallet;
        self.account.coins = Some(snapshot.coins);
        self.account.hyro_transient_streak = 0;
        self.commit_balance(cx);
        self.account.orders_snapshot_version += 1;
        self.account.ready = true;
        // A complete committed snapshot is the fresh order read for the fence.
        cx.orders_reconciled(None);
    }

    fn settle_coins(&self) -> &'static [&'static str] {
        if self.variant.is_spot() || matches!(self.category, Category::Inverse | Category::Option) {
            &[""]
        } else if self.account.classic || self.variant.is_hyro() {
            &["USDT"]
        } else {
            &["USDT", "USDC"]
        }
    }

    fn margin_type(&self) -> MarginType {
        self.account.margin_type.unwrap_or(MarginType::Cross)
    }

    pub(crate) fn request_account_info(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.variant.is_hyro() {
            self.account.margin_type = Some(MarginType::Cross);
            self.startup.account_info = true;
            return;
        }
        self.send_read(
            cx,
            Rest::get("/v5/account/info", Query::new()).signed(),
            Then::Account(Request::AccountInfo),
        );
    }

    pub(crate) fn try_start_account(&mut self, cx: &mut Cx<'_, Pending>) {
        // A key the venue refused is never used for the private feed or account reads.
        if self.credentials_rejected
            || !self.catalog_ready
            || !self.startup.account_info
            || std::mem::replace(&mut self.account.started, true)
        {
            return;
        }
        if self.category == self.primary_category {
            self.open_feed(cx, Channel::Private);
        }
        if self.variant.is_hyro() {
            return;
        }
        self.fetch_orders(cx);
        self.fetch_positions(cx);
        self.fetch_wallet(cx);
        self.account.last_account_read_at = cx.now;
        self.account.tick_timer = Some(cx.set_poll_timer(RECONCILE_INTERVAL_MS));
    }

    fn page_query(&self, coin: &str, limit: u32, cursor: &str) -> Query {
        let query = Query::new().str("category", self.paths.category);
        let query = if coin.is_empty() {
            query
        } else {
            query.str("settleCoin", coin)
        };
        query.num("limit", f64::from(limit)).str("cursor", cursor)
    }

    fn request_orders_page(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        generation: u32,
        coin: &'static str,
        cursor: String,
    ) {
        let query = self.page_query(coin, ORDERS_PAGE_LIMIT, &cursor);
        let request = Request::Orders {
            generation,
            coin,
            cursor,
            rows: Vec::new(),
            unsupported: Vec::new(),
            invalid: false,
            rejected: RowAdmission::default(),
        };
        let rest = Rest::get("/v5/order/realtime", query).streamed().signed();
        self.send_page(cx, generation, rest, Then::Account(request));
    }

    fn request_positions_page(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        generation: u32,
        coin: &'static str,
        cursor: String,
    ) {
        let limit = if self.variant.is_hyro() {
            ORDERS_PAGE_LIMIT
        } else {
            POSITIONS_PAGE_LIMIT
        };
        let query = self.page_query(coin, limit, &cursor);
        let request = Request::Positions {
            generation,
            coin,
            cursor,
            rows: Vec::new(),
            invalid: false,
            rejected: RowAdmission::default(),
        };
        let rest = Rest::get("/v5/position/list", query).streamed().signed();
        self.send_page(cx, generation, rest, Then::Account(request));
    }

    /// A Background root for one paged read: its legs share one owner, so the
    /// engine counts the read only from its earliest released page.
    fn account_read_root(
        &self,
        cx: &mut Cx<'_, Pending>,
    ) -> Option<safe_cex_engine::request_context::Context> {
        let policy = self.read_policy(true, ACCOUNT_READ_WINDOW_MS)?;
        cx.http_context(safe_cex_engine::request_context::Scope::Background, policy)
            .ok()
    }

    /// One page as a leg of its join's root; a refused leg fails the page.
    fn send_page(&mut self, cx: &mut Cx<'_, Pending>, generation: u32, rest: Rest, then: Then) {
        let root = match &then {
            Then::Account(Request::Orders { .. }) => self
                .account
                .orders
                .as_ref()
                .filter(|join| join.generation == generation)
                .and_then(|join| join.root.clone()),
            _ => self
                .account
                .positions
                .as_ref()
                .filter(|join| join.generation == generation)
                .and_then(|join| join.root.clone()),
        };
        let leg = root
            .zip(self.read_policy(true, crate::policy::read_policy().timeout_ms))
            .and_then(|(root, policy)| cx.http_leg_context(&root, policy).ok());
        match leg {
            Some(leg) => {
                let legs = match &then {
                    Then::Account(Request::Orders { .. }) => {
                        self.account.orders.as_mut().map(|join| &mut join.legs)
                    }
                    _ => self.account.positions.as_mut().map(|join| &mut join.legs),
                };
                if let Some(legs) = legs {
                    legs.push(leg.clone());
                }
                self.queue_read(cx, leg, rest, then)
            }
            None => {
                self.note_local_read_refusal();
                self.on_public_response(cx, then, Err("Bybit account read was not admitted"));
            }
        }
    }

    /// Drop the in-flight snapshots, releasing their read authority.
    fn drop_account_reads(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(join) = self.account.orders.take() {
            Self::finish_read_root(cx, join.root, &join.legs);
        }
        if let Some(join) = self.account.positions.take() {
            Self::finish_read_root(cx, join.root, &join.legs);
        }
    }

    /// Finish a read's page legs, then its root. A leg already retired after
    /// its page was handled is refused harmlessly.
    fn finish_read_root(
        cx: &mut Cx<'_, Pending>,
        root: Option<safe_cex_engine::request_context::Context>,
        legs: &[safe_cex_engine::request_context::Context],
    ) {
        for leg in legs {
            let _ = cx.finish_http_context(leg);
        }
        if let Some(root) = root {
            let _ = cx.finish_http_context(&root);
        }
    }

    /// Reconcile an uncertain write with a fresh order read: an older read
    /// still out predates the write, so it is abandoned rather than joined.
    pub(crate) fn reconcile_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(old) = self.account.orders.take() {
            Self::finish_read_root(cx, old.root, &old.legs);
        }
        self.account.orders_follow_up = false;
        self.fetch_orders(cx);
    }

    pub(crate) fn fetch_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.orders.is_some() {
            return;
        }
        self.account.orders_generation += 1;
        let generation = self.account.orders_generation;
        let coins = self.settle_coins();
        let root = self.account_read_root(cx);
        self.account.orders = Some(OrdersJoin {
            root,
            legs: Vec::new(),
            generation,
            revision: self.account.revision,
            issued_at: cx.now,
            pending: coins.len(),
            rows: Vec::new(),
            unsupported: Vec::new(),
            failed: false,
            rejected: RowAdmission::default(),
            cursors: HashSet::new(),
        });
        for coin in coins {
            self.request_orders_page(cx, generation, coin, String::new());
        }
    }

    pub(crate) fn fetch_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        // Frame-fed: positions arrive in the data source's frames, never read here.
        if cx.frame_fed() {
            return;
        }
        if self.variant.is_spot() {
            cx.store.set_positions(Vec::new());
            return;
        }
        if self.account.positions.is_some() {
            return;
        }
        self.account.positions_generation += 1;
        let generation = self.account.positions_generation;
        let coins = self.settle_coins();
        let root = self.account_read_root(cx);
        self.account.positions = Some(PositionsJoin {
            root,
            legs: Vec::new(),
            generation,
            version: self.account.position_version,
            pending: coins.len(),
            rows: Vec::new(),
            failed: false,
            rejected: RowAdmission::default(),
            cursors: HashSet::new(),
        });
        for coin in coins {
            self.request_positions_page(cx, generation, coin, String::new());
        }
    }

    fn fetch_wallet(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.category != self.primary_category {
            return;
        }
        if std::mem::replace(&mut self.account.wallet_in_flight, true) {
            return;
        }
        let account_type = if self.account.classic {
            "CONTRACT"
        } else {
            "UNIFIED"
        };
        let query = Query::new().str("accountType", account_type);
        self.send_read(
            cx,
            Rest::get("/v5/account/wallet-balance", query).signed(),
            Then::Account(Request::Wallet {
                session: self.account.session_version,
                epoch: self.account.wallet_epoch,
            }),
        );
    }

    /// TS repairs Demo's dropped order frames for reduce-only limits by asking for
    /// that order's executions at 1s, 3s and just after the 8s placement grace.
    pub(crate) fn schedule_fill_repair(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: &str,
        symbol: &str,
    ) {
        if self.variant != crate::Variant::Demo && !self.brazil_site {
            return;
        }
        for delay in [1_000.0, 3_000.0, FILL_REPAIR_GRACE_MS + 250.0] {
            let timer = cx.set_timer(delay);
            self.account
                .fill_repairs
                .insert(timer, (id.to_owned(), symbol.to_owned(), cx.now));
        }
    }

    fn request_fill_repair(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: String,
        symbol: String,
        placed_at: f64,
    ) {
        if cx.store.order(&id).is_none() {
            return;
        }
        self.request_fill_page(cx, id, symbol, placed_at, None, 0.0, 0);
    }

    #[allow(clippy::too_many_arguments)]
    fn request_fill_page(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: String,
        symbol: String,
        placed_at: f64,
        cursor: Option<String>,
        filled: f64,
        pages: u8,
    ) {
        let query = Query::new()
            .str("category", self.paths.category)
            .str("symbol", &symbol)
            .str("orderId", &id)
            .num("limit", 100.0)
            .opt_str("cursor", cursor.as_deref());
        self.send_read(
            cx,
            Rest::get("/v5/execution/list", query).signed(),
            Then::Account(Request::FillRepair {
                order_id: id,
                symbol,
                placed_at,
                session: self.account.session_version,
                cursor,
                filled,
                pages,
            }),
        );
    }

    /// Bybit returns a cursor even on a short page; walk it as TS does (20 pages).
    #[allow(clippy::too_many_arguments)]
    fn on_fill_repair(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: String,
        symbol: String,
        placed_at: f64,
        cursor: Option<String>,
        mut filled: f64,
        pages: u8,
        body: &str,
    ) {
        let Some(order) = cx.store.order(&id).cloned() else {
            return;
        };
        let page = serde_json::from_str::<Listed<ExecutionRow>>(body)
            .ok()
            .and_then(|l| l.result);
        let mut complete = false;
        if let Some(page) = page {
            filled += page
                .list
                .iter()
                .filter(|e| e.order_id.as_ref() == id)
                .map(|e| float_of(&e.exec_qty))
                .filter(|q| q.is_finite() && *q > 0.0)
                .sum::<f64>();
            let next = page.next_page_cursor.filter(|c| !c.is_empty());
            match next {
                Some(next) if !page.list.is_empty() && cursor.as_ref() != Some(&next) => {
                    if pages + 1 < 20 {
                        self.request_fill_page(
                            cx,
                            id,
                            symbol,
                            placed_at,
                            Some(next),
                            filled,
                            pages + 1,
                        );
                        return;
                    }
                }
                _ => complete = true,
            }
        }
        let target = order.amount.max(order.remaining + order.filled);
        // Only a complete, readable walk can prove a full fill.
        if complete && target > 0.0 && filled >= target - (target * 1e-8).max(1e-12) {
            self.remember_terminal_order(&id);
            let children = [format!("{id}__stop_loss"), format!("{id}__take_profit")];
            cx.store.remove_orders(&[&id, &children[0], &children[1]]);
            return;
        }
        if cx.now - placed_at >= FILL_REPAIR_GRACE_MS {
            self.account.orders_follow_up |= self.account.orders.is_some();
            self.fetch_orders(cx);
        }
    }

    pub(crate) fn on_account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some((id, symbol, placed_at)) = self.account.fill_repairs.remove(&timer) {
            self.request_fill_repair(cx, id, symbol, placed_at);
            return;
        }
        if self.account.partial_stop_refresh == Some(timer) {
            self.account.partial_stop_refresh = None;
            if self.variant.is_hyro() {
                if self.account.authed && self.account.handshake.is_none() {
                    if self.account.snapshot.is_some() {
                        self.account.revision = self.account.revision.wrapping_add(1);
                    } else {
                        self.start_hyro_snapshot(cx);
                    }
                }
            } else {
                self.account.orders_follow_up |= self.account.orders.is_some();
                self.fetch_orders(cx);
            }
            return;
        }
        if self.account.handshake_timer == Some(timer) {
            self.account.handshake_timer = None;
            self.fail_hyro(cx, "private handshake acknowledgement timed out");
            return;
        }
        if self.account.tick_timer != Some(timer) {
            return;
        }
        let now = cx.now;
        self.account.tick_timer = Some(cx.set_poll_timer(RECONCILE_INTERVAL_MS));
        if self.variant.is_hyro() {
            if self.account.authed
                && self.account.handshake.is_none()
                && self.account.snapshot.is_none()
            {
                self.start_hyro_snapshot(cx);
            }
            return;
        }
        if self.account.orders_stale && now - self.account.last_orders_retry_at >= ORDERS_RETRY_MS {
            self.account.last_orders_retry_at = now;
            self.fetch_orders(cx);
        }
        let reconcile = self.account_reads_due(now);
        // Position unrealized PnL is venue-reported and the position topic may push
        // only on position changes, so an open position keeps its per-tick read.
        if reconcile
            || cx
                .store
                .positions()
                .iter()
                .any(|position| position.contracts > 0.0)
        {
            self.fetch_positions(cx);
        }
        if reconcile {
            self.account.last_account_read_at = now;
            self.fetch_wallet(cx);
        }
    }

    /// Positions and the wallet stream on the private topics; REST re-reads them
    /// every tick only while those topics are down, else as the safety reconcile.
    pub(crate) fn account_reads_due(&self, now: f64) -> bool {
        !self.account.topics_live || now - self.account.last_account_read_at >= SAFETY_RECONCILE_MS
    }

    pub(crate) fn on_private_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.variant.is_hyro() {
            self.account.session_version += 1;
            self.account.handshake = Some("auth");
            self.account.handshake_timer = Some(cx.set_timer(10_000.0));
        }
        if let Some(credentials) = &self.credentials {
            let request_id = self
                .variant
                .is_hyro()
                .then(|| format!("hyro-{}-auth", self.account.session_version));
            cx.send_signed(
                socket,
                String::new(),
                Box::new(crate::sign::WsAuth::new(
                    credentials.clone(),
                    request_id,
                    self.paths.private_ws.clone(),
                )),
            );
        }
    }

    pub(crate) fn on_private_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let wire_topic = topic(text);
        if self.variant.is_hyro() && wire_topic == Some("execution") {
            return;
        }
        if self.variant.is_hyro() && wire_topic.is_some() {
            self.account.revision += 1;
            if !self.account.ready {
                return;
            }
        }
        match wire_topic.map(|topic| topic.split('.').next().unwrap_or(topic)) {
            Some("order") if !self.variant.is_hyro() => {
                let Ok(frame) =
                    serde_json::from_str::<TopicFrame<&serde_json::value::RawValue>>(text)
                else {
                    self.account.orders_stale = true;
                    if let Some(join) = &mut self.account.orders {
                        join.rejected.reject(None);
                    }
                    self.account.orders_follow_up |= self.account.orders.is_some();
                    row_skipped(
                        cx,
                        Level::Warn,
                        "Bybit malformed order frame; reconciliation scheduled",
                    );
                    return;
                };
                let mut rejected = RowAdmission::default();
                let mut admitted = Vec::new();
                for raw in &frame.data {
                    let Ok(row) = serde_json::from_str::<OrderRow>(raw.get()) else {
                        self.reject_order_item(cx, &mut rejected, raw.get());
                        continue;
                    };
                    if row.symbol.is_empty() || row.order_id.is_empty() {
                        self.reject_order_item(cx, &mut rejected, raw.get());
                        continue;
                    }
                    if !self.trades_row(&row.category, &row.symbol) {
                        continue;
                    }
                    // Existing replacement placeholders carry no financial update.
                    if row.order_status.as_ref() == "PartiallyFilled"
                        && or_zero(&row.price) <= 0.0
                        && or_zero(&row.qty) <= 0.0
                    {
                        continue;
                    }
                    let mapped = map_order(&row, cx.store, cx.now);
                    if !admitted_order_numbers(&row, false)
                        || mapped
                            .as_ref()
                            .is_none_or(|m| m.orders.iter().any(|o| !valid_order(o)))
                    {
                        self.reject_order_item(cx, &mut rejected, raw.get());
                        continue;
                    }
                    admitted.push((row, mapped.expect("validated mapping")));
                }
                if rejected.any() {
                    if let Some(join) = &mut self.account.orders {
                        join.rejected.extend(rejected.clone());
                    }
                    self.account.orders_follow_up |= self.account.orders.is_some();
                }
                for (row, mapped) in admitted {
                    if !rejected.conflicts(row.order_id.as_ref()) {
                        self.apply_order_row(cx, &row, mapped);
                    }
                }
            }

            Some("order") => {
                if let Ok(frame) = serde_json::from_str::<TopicFrame<OrderRow>>(text) {
                    if self.variant.is_hyro()
                        && frame.data.iter().any(|row| {
                            !valid_hyro_order(row)
                                || map_order(row, cx.store, cx.now).is_none_or(|mapped| {
                                    mapped.orders.iter().any(|order| !valid_order(order))
                                })
                        })
                    {
                        self.fail_hyro(cx, "malformed private order event");
                        return;
                    }
                    let rows: Vec<_> = frame
                        .data
                        .iter()
                        .filter(|r| self.trades_row(&r.category, &r.symbol))
                        .collect();
                    for row in rows {
                        self.on_order_row(cx, row);
                    }
                } else if self.variant.is_hyro() {
                    self.fail_hyro(cx, "malformed private order event");
                }
            }
            Some("execution") => {
                if self.variant.is_hyro() {
                    return;
                }
                if let Ok(frame) = serde_json::from_str::<TopicFrame<ExecutionRow>>(text) {
                    let rows: Vec<_> = frame
                        .data
                        .iter()
                        .filter(|r| self.trades_row(&r.category, &r.symbol))
                        .collect();
                    for row in rows {
                        on_execution(cx, row, &mut self.account);
                    }
                }
            }
            Some("position") if !self.variant.is_hyro() => {
                let Ok(frame) =
                    serde_json::from_str::<TopicFrame<&serde_json::value::RawValue>>(text)
                else {
                    if let Some(join) = &mut self.account.positions {
                        join.rejected.reject(None);
                    }
                    row_skipped(
                        cx,
                        Level::Warn,
                        "Bybit malformed position frame; awaiting reconciliation",
                    );
                    return;
                };
                let mut rejected = RowAdmission::default();
                let mut admitted = Vec::new();
                for raw in &frame.data {
                    let Ok(row) = serde_json::from_str::<PositionRow>(raw.get()) else {
                        self.reject_position_item(cx, &mut rejected, raw.get());
                        continue;
                    };
                    if row.symbol.is_empty() {
                        self.reject_position_item(cx, &mut rejected, raw.get());
                        continue;
                    }
                    if !self.trades_row(&row.category, &row.symbol) {
                        continue;
                    }
                    if !admitted_position_numbers(&row, false) {
                        self.reject_position_item(cx, &mut rejected, raw.get());
                        continue;
                    }
                    admitted.push(row);
                }
                if rejected.any()
                    && let Some(join) = &mut self.account.positions
                {
                    join.rejected.extend(rejected.clone());
                }
                admitted.retain(|row| {
                    !rejected.conflicts(row.symbol.as_ref())
                        && (!rejected.unidentified()
                            || !cx.store.positions().iter().any(|old| {
                                let mode = row.position_idx.map(|idx| {
                                    if idx == 0 {
                                        PositionMode::Oneway
                                    } else {
                                        PositionMode::Hedged
                                    }
                                });
                                old.symbol == row.symbol.as_ref() && old.position_mode != mode
                            }))
                });
                self.on_position_rows(cx, &admitted);
            }

            Some("position") => {
                if let Ok(frame) = serde_json::from_str::<TopicFrame<PositionRow>>(text) {
                    if self.variant.is_hyro()
                        && frame.data.iter().any(|row| !valid_hyro_position(row))
                    {
                        self.fail_hyro(cx, "malformed private position event");
                        return;
                    }
                    self.on_position_rows(cx, &frame.data);
                } else if self.variant.is_hyro() {
                    self.fail_hyro(cx, "malformed private position event");
                }
            }
            Some("wallet") => {
                if let Ok(frame) = serde_json::from_str::<TopicFrame<WalletAccount>>(text)
                    && let Some(account) = frame.data.first()
                {
                    if self.variant.is_hyro() && !valid_hyro_wallet(account) {
                        self.fail_hyro(cx, "malformed private wallet event");
                        return;
                    }
                    self.account.wallet = Some(Wallet::from_wire(account));
                    // A push may list only changed coins; merge, keeping explicit zeros.
                    let mut coins = self.account.coins.take().unwrap_or_default();
                    for coin in coin_balances(account) {
                        match coins.iter_mut().find(|c| c.base == coin.base) {
                            Some(known) => *known = coin,
                            None => coins.push(coin),
                        }
                    }
                    self.account.coins = Some(coins);
                    self.commit_balance(cx);
                } else if self.variant.is_hyro() {
                    self.fail_hyro(cx, "malformed private wallet event");
                }
            }
            Some(_) => {}
            None => self.on_private_op(cx, text),
        }
    }

    fn on_private_op(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        if self.variant.is_hyro() {
            self.on_hyro_op(cx, text);
            return;
        }
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
                        if self.variant.is_spot() { r#"{"op":"subscribe","args":["order.spot","execution.spot","wallet"]}"# } else if self.category == Category::Inverse { r#"{"op":"subscribe","args":["order.inverse","execution.inverse","position.inverse","wallet"]}"# } else { r#"{"op":"subscribe","args":["order","execution","position","wallet"]}"# },
                    );
                }
            }
            ("subscribe", Some(true)) if self.account.authed => {
                self.account.topics_live = true;
                self.account.last_account_read_at = cx.now;
                // Cover the gap before topics became live, including the first connection.
                self.account.orders_follow_up = self.account.orders.is_some();
                self.account.positions_follow_up = self.account.positions.is_some();
                self.account.wallet_follow_up = self.account.wallet_in_flight;
                self.fetch_orders(cx);
                self.fetch_positions(cx);
                self.fetch_wallet(cx);
                let category = self.category;
                for other in self
                    .categories()
                    .into_iter()
                    .filter(|other| *other != category)
                    .collect::<Vec<_>>()
                {
                    self.select_category(other);
                    self.account.authed = true;
                    self.account.orders_follow_up = self.account.orders.is_some();
                    self.account.positions_follow_up = self.account.positions.is_some();
                    self.fetch_orders(cx);
                    self.fetch_positions(cx);
                }
                self.select_category(category);
            }
            (_, Some(false)) => {
                let message = frame.ret_msg.as_deref().unwrap_or_default();
                cx.notice(
                    Level::Error,
                    if op == "auth" {
                        code::AUTH_REJECTED
                    } else {
                        code::SUBSCRIPTION_REJECTED
                    },
                    format!("[WS]{op}:{message}"),
                );
                // An unauthenticated socket answers pings but carries no topics.
                if op == "auth"
                    && let Some(feed) = self.private.feed
                {
                    cx.retire_socket(
                        feed,
                        safe_cex_engine::socket_lifecycle::RetireReason::Authentication,
                    );
                }
            }
            _ => {}
        }
    }

    fn on_order_row(&mut self, cx: &mut Cx<'_, Pending>, row: &OrderRow<'_>) {
        if self.variant.is_hyro() {
            self.observe_hyro_order(row);
            if !self.on_hyro_fill(cx, row) {
                return;
            }
        }
        let status = row.order_status.as_ref();
        // A replaced order reports a partial fill of nothing at no price.
        if status == "PartiallyFilled" && or_zero(&row.price) <= 0.0 && or_zero(&row.qty) <= 0.0 {
            return;
        }
        let Some(mapped) = map_order(row, cx.store, cx.now) else {
            return;
        };
        self.apply_order_row(cx, row, mapped);
    }

    fn apply_order_row(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        row: &OrderRow<'_>,
        mut mapped: Mapped,
    ) {
        let status = row.order_status.as_ref();
        self.note_unpriced_tpsl(&mapped);
        if self.stale_terminal_snapshot(&mapped) {
            return;
        }
        self.note_position_bound(&mapped);
        if mapped.terminal {
            let ids = mapped
                .orders
                .iter()
                .map(|order| order.id.clone())
                .collect::<Vec<_>>();
            self.note_order_terminal(&ids);
        }
        match status {
            "Cancelled" | "Filled" | "Deactivated" | "Rejected" | "PartiallyFilledCanceled" => {
                if let Some(reason) = present(&row.reject_reason)
                    && status != "Filled"
                    && !QUIET_REJECT_REASONS.contains(&reason)
                {
                    cx.notice(Level::Error, code::ORDER_REJECTED, reason.to_owned());
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

    fn note_unpriced_tpsl(&mut self, mapped: &Mapped) {
        // The spot TP/SL itself is the parent row; legs carry their own kind.
        let Some(order) = mapped.orders.first() else {
            return;
        };
        if mapped.unpriced_tpsl {
            self.account.unpriced_tpsl.insert(order.id.clone());
        } else {
            self.account.unpriced_tpsl.remove(&order.id);
        }
    }

    /// Spot TP/SL rows read before the ticker seed were labelled against a
    /// zero price; relabel them from their trigger now that one exists.
    pub(crate) fn reclassify_unpriced_tpsl(&mut self, cx: &mut Cx<'_, Pending>) {
        for id in std::mem::take(&mut self.account.unpriced_tpsl) {
            if let Some(kind) = cx
                .store
                .order(&id)
                .and_then(|o| priced_tpsl_kind(o, cx.store))
            {
                cx.store.update_order(&id, |order| order.kind = kind);
            }
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
        if self.variant.is_spot() {
            return;
        }
        let margin_type = self.margin_type();
        let mut changed = false;
        let rows: Vec<_> = rows
            .iter()
            .filter(|r| self.trades_row(&r.category, &r.symbol))
            .collect();
        for row in rows {
            // A malformed supplied Greek is not an authoritative zero. Validate
            // before changing either the position or its exact-ID metadata.
            if self.category == Category::Option && !valid_position_greeks(row) {
                continue;
            }
            let mapped = self.map_account_position(row, cx.store, cx.now);
            let position = mapped.position;
            let mut greek_rows = cx.store.position_greeks().unwrap_or_default().to_vec();
            greek_rows.retain(|row| row.position_id != position.id);
            if position.contracts != 0.0
                && let Some(greeks) = mapped.greeks
            {
                greek_rows.push(PositionGreeks {
                    position_id: position.id.clone(),
                    greeks,
                });
            }
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
            if self.category == Category::Option {
                cx.store.set_position_greeks(greek_rows);
            }
            changed = true;
        }
        if !changed {
            return;
        }
        cx.store.set_hedged(self.any_hedged());
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
        if self.category == self.primary_category {
            self.wallet = self.account.wallet;
            if let Some(coins) = &self.account.coins {
                cx.store.set_coin_balances(coins.clone());
            }
        }
        if let Some(wallet) = self.wallet {
            cx.store.set_balance(wallet.balance(open_upnl(cx.store)));
        }
    }

    fn reject_order_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        rejected: &mut RowAdmission<String>,
        item: &str,
    ) {
        let identity = rejected_identity(item);
        if let Some(identity) = &identity
            && let Some(symbol) = identity.symbol.as_deref().filter(|v| !v.is_empty())
            && !self.trades_row(&identity.category.as_deref().map(Into::into), symbol)
        {
            return;
        }
        let key = identity.and_then(|r| {
            r.symbol
                .filter(|v| !v.is_empty())
                .and(r.order_id.filter(|v| !v.is_empty()))
        });
        rejected.skip(cx, key, Level::Warn, "Bybit malformed order row; cached identity/protection retained and reconciliation scheduled");
        self.account.orders_stale = true;
    }
    fn reject_position_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        rejected: &mut RowAdmission<String>,
        item: &str,
    ) {
        let identity = rejected_identity(item);
        if let Some(identity) = &identity
            && let Some(symbol) = identity.symbol.as_deref().filter(|v| !v.is_empty())
            && !self.trades_row(&identity.category.as_deref().map(Into::into), symbol)
        {
            return;
        }
        rejected.skip(
            cx,
            identity.and_then(|r| r.symbol.filter(|v| !v.is_empty())),
            Level::Warn,
            "Bybit malformed position row; cached symbol exposure retained until reconciliation",
        );
    }

    pub(crate) fn on_account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Orders {
                rows,
                unsupported,
                invalid,
                rejected,
                ..
            } => {
                let Ok(row) = serde_json::from_str::<OrderRow>(item) else {
                    if self.variant.is_hyro() {
                        *invalid = true;
                    } else {
                        self.reject_order_item(cx, rejected, item);
                    }
                    return;
                };
                if row.symbol.is_empty() || row.order_id.is_empty() {
                    if self.variant.is_hyro() {
                        *invalid = true;
                    } else {
                        self.reject_order_item(cx, rejected, item);
                    }
                    return;
                }
                if !self.trades_row(&row.category, &row.symbol) {
                    return;
                }
                let mapped = map_order(&row, cx.store, cx.now);
                let well_formed = admitted_order_numbers(&row, self.variant.is_hyro());
                // A sound row of a type the store cannot hold must not block every
                // other order; finish_orders keeps its last known state and says so.
                if well_formed && mapped.is_none() && !self.variant.is_hyro() {
                    rejected.reject(Some(row.order_id.to_string()));
                    unsupported.push(row.order_id.to_string());
                } else if !well_formed
                    || mapped.as_ref().is_none_or(|mapped| {
                        mapped.orders.iter().any(|order| !valid_order(order))
                            || (self.variant.is_hyro() && mapped.fill_cursor.is_none())
                    })
                {
                    if self.variant.is_hyro() {
                        *invalid = true;
                    } else {
                        self.reject_order_item(cx, rejected, item);
                    }
                } else if let Some(mapped) = mapped {
                    self.note_unpriced_tpsl(&mapped);
                    rows.push(mapped);
                }
            }
            Request::Positions {
                rows,
                invalid,
                rejected,
                ..
            } => {
                let Ok(row) = serde_json::from_str::<PositionRow>(item) else {
                    if self.variant.is_hyro() {
                        *invalid = true;
                    } else {
                        self.reject_position_item(cx, rejected, item);
                    }
                    return;
                };
                if row.symbol.is_empty() {
                    if self.variant.is_hyro() {
                        *invalid = true;
                    } else {
                        self.reject_position_item(cx, rejected, item);
                    }
                    return;
                }
                if !self.trades_row(&row.category, &row.symbol) {
                    return;
                }
                if !admitted_position_numbers(&row, self.variant.is_hyro()) {
                    if self.variant.is_hyro() {
                        *invalid = true;
                    } else {
                        self.reject_position_item(cx, rejected, item);
                    }
                } else {
                    rows.push(self.map_account_position(&row, cx.store, cx.now));
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
        // The table's classification of a venue or transport failure; a reply
        // the codec rejects later carries none. Each notice keeps its own text.
        let classified = match &response {
            Ok(response) => crate::read_refusal(response, ""),
            Err(error) => Some(VenueError::new(*error).kind(ErrorKind::Network)),
        };
        let failed = |message: String| {
            let mut error = classified.clone().unwrap_or_else(|| VenueError::new(""));
            error.message = message;
            error
        };
        match request {
            Request::FillRepair {
                order_id,
                symbol,
                placed_at,
                session,
                cursor,
                filled,
                pages,
            } => {
                if session != self.account.session_version {
                    return;
                }
                match outcome {
                    Ok(body) => self.on_fill_repair(
                        cx, order_id, symbol, placed_at, cursor, filled, pages, body,
                    ),
                    Err(message) => {
                        cx.notice_error(
                            Level::Warn,
                            code::REQUEST_FAILED,
                            &failed(format!(
                                "Bybit fill check for order {order_id} failed: {message}"
                            )),
                        );
                        // After the grace a failed check still lets open orders correct the row.
                        if cx.now - placed_at >= FILL_REPAIR_GRACE_MS {
                            self.account.orders_follow_up |= self.account.orders.is_some();
                            self.fetch_orders(cx);
                        }
                    }
                }
            }
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
                    Err(message) => {
                        let refused = response
                            .as_ref()
                            .ok()
                            .and_then(venue_error)
                            .filter(|(code, _)| crate::CREDENTIAL_REJECTIONS.contains(code));
                        match refused {
                            // The key itself was refused (an IP allowlist, bad key or
                            // expiry): fail visibly instead of loading empty account data.
                            Some((code, venue_message)) => {
                                self.reject_credentials(cx, code, &venue_message);
                            }
                            // Public data still loads; the private failure is an Error.
                            None => cx.notice_error(Level::Error, code::REQUEST_FAILED,
                                &failed(format!("Bybit market data is ready, but the private API was rejected ({message}) — trading is unavailable")),
                            ),
                        }
                    }
                }
                self.startup.account_info = true;
                let classic = self.account.classic;
                let margin_type = self.account.margin_type;
                for state in &mut self.products {
                    state.account.classic = classic;
                    state.account.margin_type = margin_type;
                    state.startup.account_info = true;
                }
                let category = self.category;
                for other in self.categories() {
                    self.select_category(other);
                    self.try_start_account(cx);
                }
                self.select_category(category);
            }
            Request::Orders {
                generation,
                coin,
                cursor,
                rows,
                unsupported,
                invalid,
                rejected,
            } => {
                let malformed = invalid
                    || outcome
                        .as_ref()
                        .is_ok_and(|body| !valid_snapshot_list(body, &cursor));
                let outcome = if malformed {
                    Err("Malformed Bybit private snapshot".to_owned())
                } else {
                    outcome
                };
                let next = outcome
                    .as_ref()
                    .ok()
                    .and_then(|body| next_cursor(body, &cursor, false));
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
                        join.rejected.extend(rejected);
                        join.unsupported.extend(unsupported);
                        if let Some(next) = next {
                            if !join.cursors.insert(next.clone()) || join.cursors.len() > 100 {
                                join.failed = true;
                            } else {
                                self.request_orders_page(cx, generation, coin, next);
                                return;
                            }
                        }
                        join.pending -= 1;
                    }
                    Err(message) => {
                        cx.retried_failure_error(
                            "bybit.account:2216",
                            &failed(format!("Bybit orders: {message}")),
                        );
                        join.failed = true;
                        self.account.hyro_transient |= !malformed && !malformed_stream(&message);
                    }
                }
                self.finish_orders(cx);
                if self.account.orders.is_none() {
                    self.unlisted_read_landed(cx, !self.account.orders_stale);
                }
            }
            Request::Positions {
                generation,
                coin,
                cursor,
                rows,
                invalid,
                rejected,
            } => {
                let malformed = invalid
                    || outcome
                        .as_ref()
                        .is_ok_and(|body| !valid_snapshot_list(body, &cursor));
                let outcome = if malformed {
                    Err("Malformed Bybit private snapshot".to_owned())
                } else {
                    outcome
                };
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
                        join.rejected.extend(rejected);
                        if let Some(next) = next {
                            if !join.cursors.insert(next.clone()) || join.cursors.len() > 100 {
                                join.failed = true;
                            } else {
                                self.request_positions_page(cx, generation, coin, next);
                                return;
                            }
                        }
                        join.pending -= 1;
                    }
                    Err(message) => {
                        cx.retried_failure_error(
                            "bybit.account:2269",
                            &failed(format!("Bybit positions: {message}")),
                        );
                        join.failed = true;
                        self.account.hyro_transient |= !malformed && !malformed_stream(&message);
                    }
                }
                self.finish_positions(cx);
            }
            Request::Wallet { session, epoch } => {
                if session != self.account.session_version || epoch != self.account.wallet_epoch {
                    return;
                }
                self.account.wallet_in_flight = false;
                // Stale account info can call a migrated account classic; TS retries UNIFIED once.
                let retired = self.account.classic
                    && outcome
                        .as_ref()
                        .is_err_and(|m| m.contains("accountType only support UNIFIED"));
                if retired {
                    cx.notice(
                        Level::Warn,
                        code::REQUEST_FAILED,
                        "Bybit wallet refused CONTRACT; reading the UNIFIED wallet",
                    );
                    self.account.classic = false;
                    for state in &mut self.products {
                        state.account.classic = false;
                    }
                }
                if std::mem::take(&mut self.account.wallet_follow_up) || retired {
                    self.fetch_wallet(cx);
                }
                if retired {
                    // USDC-settled rows were never read under the classic settle coins.
                    self.account.orders_follow_up |= self.account.orders.is_some();
                    self.account.positions_follow_up |= self.account.positions.is_some();
                    self.fetch_orders(cx);
                    self.fetch_positions(cx);
                    return;
                }
                self.account.hyro_transient |= outcome.is_err();
                let outcome_body = outcome.as_ref().ok().copied();
                let account = outcome.and_then(|body| {
                    serde_json::from_str::<WalletEnvelope>(body)
                        .ok()
                        .and_then(|w| w.result)
                        .and_then(|r| r.list.into_iter().next())
                        .filter(|account| !self.variant.is_hyro() || valid_hyro_wallet(account))
                        .map(|account| (Wallet::from_wire(&account), coin_balances(&account)))
                        .ok_or_else(|| "Invalid or empty balance data received".to_owned())
                });
                match account {
                    Ok((wallet, coins)) => {
                        // TS keeps the venue's first account verbatim as `balance.info`.
                        self.balance_info = outcome_body
                            .and_then(raw_wallet_account)
                            .or(self.balance_info.take());
                        if self.variant.is_hyro() {
                            if let Some(snapshot) = &mut self.account.snapshot {
                                snapshot.wallet = Some(wallet);
                                snapshot.coins = coins;
                            }
                            self.finish_hyro_snapshot(cx);
                        } else {
                            self.account.wallet = Some(wallet);
                            self.account.coins = Some(coins);
                            self.commit_balance(cx);
                        }
                    }
                    Err(message) => {
                        cx.retried_failure_error(
                            "bybit.account:2342",
                            &failed(format!("Bybit balance: {message}")),
                        );
                        if self.variant.is_hyro() {
                            self.hyro_read_failed(cx, "wallet reconciliation failed");
                        }
                    }
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
        let mut join = self.account.orders.take().expect("checked above");
        Self::finish_read_root(cx, join.root.take(), &join.legs);
        if std::mem::take(&mut self.account.orders_follow_up) {
            self.fetch_orders(cx);
        }
        if join.failed {
            self.account.orders_stale = true;
            if self.variant.is_hyro() {
                self.hyro_read_failed(cx, "orders reconciliation failed");
            }
            return;
        }
        self.account.orders_stale = join.rejected.any();
        if self.variant.is_hyro() {
            if let Some(snapshot) = &mut self.account.snapshot {
                snapshot.orders = Some(join.rows);
            }
            self.finish_hyro_snapshot(cx);
            return;
        }
        if join.revision != self.account.revision {
            self.fetch_orders(cx);
            return;
        }
        self.account.orders_snapshot_version += 1;
        let admitted_parents: HashSet<String> = join
            .rows
            .iter()
            .map(|r| parent_order_id(&r.orders[0].id))
            .filter(|id| !join.rejected.conflicts(*id))
            .map(str::to_owned)
            .collect();
        self.account.position_bound.retain(|id| {
            join.rejected.holds(parent_order_id(id))
                && !admitted_parents.contains(parent_order_id(id))
        });
        let mut orders = Vec::new();
        for mut mapped in join.rows {
            if join
                .rejected
                .conflicts(parent_order_id(&mapped.orders[0].id))
            {
                continue;
            }
            if self.stale_terminal_snapshot(&mapped) {
                continue;
            }
            // Rows parsed before a seed that has since landed are relabelled here.
            if mapped.unpriced_tpsl
                && let Some(kind) = priced_tpsl_kind(&mapped.orders[0], cx.store)
            {
                mapped.orders[0].kind = kind;
                self.account.unpriced_tpsl.remove(&mapped.orders[0].id);
            }
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
            .filter(|o| self.owns_symbol(&o.symbol) && o.last_update > join.issued_at)
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
        for order in cx.store.orders().iter().filter(|o| {
            self.owns_symbol(&o.symbol)
                && join.rejected.holds(parent_order_id(&o.id))
                && !admitted_parents.contains(parent_order_id(&o.id))
        }) {
            if !merged.iter().any(|o| o.id == order.id) {
                merged.push(order.clone());
            }
        }
        for id in &join.unsupported {
            let known = cx.store.order(id).filter(|o| self.owns_symbol(&o.symbol));
            if let Some(order) = known
                && !merged.iter().any(|o| o.id == *id)
            {
                merged.push(order.clone());
            }
            self.account.unsupported_logged.report(
                cx,
                id.clone(),
                Level::Error,
                if known.is_some() {
                    format!(
                        "Bybit order {id} has an unsupported type; its last known state is kept"
                    )
                } else {
                    format!("Bybit order {id} has an unsupported type and is not shown")
                },
            );
        }
        let old: Vec<String> = cx
            .store
            .orders()
            .iter()
            .filter(|order| self.owns_symbol(&order.symbol))
            .map(|order| order.id.clone())
            .collect();
        let old_refs: Vec<&str> = old.iter().map(String::as_str).collect();
        cx.store.remove_orders(&old_refs);
        for order in merged.drain(..) {
            cx.store.upsert_order(order);
        }
        self.orders_loaded.insert(self.category);
        if self
            .required_categories()
            .iter()
            .all(|c| self.orders_loaded.contains(c))
        {
            cx.store.set_orders(cx.store.orders().to_vec());
        }
        // A committed, wholly admitted fresh read clears the engine's
        // uncertain-write fence once no product's order read is still out.
        if !join.rejected.any()
            && !self.account.orders_stale
            && self.account.orders.is_none()
            && self
                .products
                .iter()
                .all(|state| state.account.orders.is_none() && !state.account.orders_stale)
        {
            cx.orders_reconciled(None);
        }
    }

    /// A read issued before an acknowledged setting cannot publish old rails.
    pub(crate) fn note_leverage_ack(&mut self) {
        // Hyro owns an atomic snapshot guarded by note_hyro_write/revision.
        if self.variant.is_hyro() {
            return;
        }
        self.account.position_version = self.account.position_version.wrapping_add(1);
        self.account.positions_follow_up |= self.account.positions.is_some();
    }

    pub(crate) fn refresh_after_mode_change(&mut self, cx: &mut Cx<'_, Pending>) {
        // A response issued before the switch cannot restore the old mode.
        self.account.position_version += 1;
        if self.category == Category::Inverse {
            self.account.position_modes.clear();
        }
        self.account.positions_follow_up |= self.account.positions.is_some();
        self.account.position_modes.retain(|symbol, _| {
            cx.store
                .market(symbol)
                .is_some_and(|market| market.quote != "USDT")
        });
        self.fetch_positions(cx);
    }

    /// Commit the positions snapshot once every settle coin is in.
    fn finish_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(join) = &self.account.positions else {
            return;
        };
        if !join.failed && join.pending > 0 {
            return;
        }
        let mut join = self.account.positions.take().expect("checked above");
        Self::finish_read_root(cx, join.root.take(), &join.legs);
        if std::mem::take(&mut self.account.positions_follow_up) {
            self.fetch_positions(cx);
        }
        if join.failed {
            if self.variant.is_hyro() {
                self.hyro_read_failed(cx, "positions reconciliation failed");
            }
            return;
        }
        if self.variant.is_hyro() {
            if let Some(snapshot) = &mut self.account.snapshot {
                snapshot.positions = Some(join.rows);
            }
            self.finish_hyro_snapshot(cx);
            return;
        }
        if join.version != self.account.position_version {
            // A stream event landed after these reads went out; it is newer.
            return;
        }
        let mut best = best_positions(
            join.rows
                .into_iter()
                .filter(|r| !join.rejected.conflicts(&r.position.symbol))
                .collect(),
        );
        // Unknown neighbors cannot prove a same-symbol mode transition. Keep
        // that symbol coherent while unrelated authoritative symbols progress.
        if join.rejected.unidentified() {
            best.retain(|r| {
                !cx.store.positions().iter().any(|old| {
                    old.symbol == r.position.symbol && old.position_mode != r.position.position_mode
                })
            });
        }
        if !best.is_empty() {
            for row in &best {
                if let Some(mode) = row.position.position_mode {
                    self.account
                        .position_modes
                        .insert(row.position.symbol.clone(), mode == PositionMode::Hedged);
                }
            }
            cx.store.set_hedged(self.any_hedged());
        }
        for row in &best {
            if row.position.leverage > 0.0 {
                cx.store.update_setting(&row.position.symbol, |setting| {
                    setting.leverage = row.position.leverage;
                    setting.margin_type = row.position.margin_type;
                });
            }
        }
        let mut greek_rows = cx
            .store
            .position_greeks()
            .unwrap_or_default()
            .iter()
            .filter(|row| {
                cx.store.position(&row.position_id).is_some_and(|position| {
                    !self.owns_symbol(&position.symbol)
                        || (join.rejected.holds(&position.symbol)
                            && !best.iter().any(|r| r.position.id == position.id))
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        for row in &best {
            if row.position.contracts != 0.0
                && let Some(greeks) = &row.greeks
            {
                greek_rows.push(PositionGreeks {
                    position_id: row.position.id.clone(),
                    greeks: greeks.clone(),
                });
            }
        }
        let category = self.category;
        let spot = self.variant.is_spot();
        cx.store.update_positions(|position| {
            if (spot || Category::for_symbol(&position.symbol) == category)
                && !join.rejected.holds(&position.symbol)
            {
                position.contracts = 0.0;
            }
        });
        for row in best {
            cx.store.upsert_position(row.position);
        }
        if self.category_enabled(Category::Option) {
            cx.store.set_position_greeks(greek_rows);
        }
        self.positions_loaded.insert(self.category);
        if self
            .required_categories()
            .iter()
            .all(|c| self.positions_loaded.contains(c))
        {
            cx.store.set_positions(cx.store.positions().to_vec());
        }
        self.commit_balance(cx);
    }
}

fn best_positions(rows: Vec<MappedPosition>) -> Vec<MappedPosition> {
    let mut best: Vec<MappedPosition> = Vec::new();
    for row in rows {
        match best
            .iter_mut()
            .find(|existing| existing.position.id == row.position.id)
        {
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
    best
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
fn on_execution(cx: &mut Cx<'_, Pending>, row: &ExecutionRow<'_>, account: &mut AccountState) {
    if !matches!(
        row.exec_type.as_deref(),
        Some("Trade" | "AdlTrade" | "BustTrade")
    ) {
        return;
    }
    let amount = or_zero(&row.exec_qty);
    let price = or_zero(&row.exec_price);
    let Some(side) = order_side(row.side.as_deref())
        .filter(|_| amount.is_finite() && amount > 0.0 && price.is_finite() && price > 0.0)
    else {
        return;
    };
    // Legacy frames without an execution ID cannot be deduplicated safely.
    if let Some(id) = present(&row.exec_id) {
        let key = (row.symbol.to_string(), id.to_owned());
        if !account.executions.insert(key.clone()) {
            return;
        }
        account.recent_executions.push_back(key);
        while account.recent_executions.len() > EXECUTION_CACHE_LIMIT {
            if let Some(old) = account.recent_executions.pop_front() {
                account.executions.remove(&old);
            }
        }
    }
    cx.fill(Fill {
        id: None,
        order_id: row.order_id.to_string(),
        symbol: row.symbol.to_string(),
        side,
        price,
        amount,
    });
}

/// What one failed Hyro reconciliation read does to the private session.
#[derive(Debug, PartialEq, Eq)]
enum HyroFailure {
    /// Keep the stream; the next tick re-reads. `counts` spends the streak.
    Retry {
        counts: bool,
    },
    Reconnect,
}

/// `transient`: this failure was transport or venue, not content (a local
/// refusal's own failure is transient). `local`: this snapshot had a read the
/// engine refused before any I/O; it waives the streak, never content.
fn hyro_failure(ready: bool, transient: bool, local: bool, streak: u8) -> HyroFailure {
    if ready && transient && (local || streak < 3) {
        HyroFailure::Retry { counts: !local }
    } else {
        HyroFailure::Reconnect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyro_failure_retries_transients_until_the_streak_and_never_counts_local_refusals() {
        use HyroFailure::{Reconnect, Retry};
        // (ready, transient, local, streak)
        assert_eq!(hyro_failure(true, true, false, 0), Retry { counts: true });
        assert_eq!(hyro_failure(true, true, false, 3), Reconnect);
        assert_eq!(hyro_failure(true, true, true, 3), Retry { counts: false });
        assert_eq!(
            hyro_failure(true, false, false, 0),
            Reconnect,
            "content reconnects"
        );
        assert_eq!(
            hyro_failure(false, true, false, 0),
            Reconnect,
            "not ready yet"
        );
        assert_eq!(
            hyro_failure(false, true, true, 0),
            Reconnect,
            "not ready yet"
        );
    }

    /// dazzy3 on #2106: a local refusal earlier in the snapshot never turns
    /// this read's content failure into a retry; bad content reconnects now.
    #[test]
    fn hyro_content_failure_reconnects_even_after_a_local_refusal() {
        assert_eq!(hyro_failure(true, false, true, 0), HyroFailure::Reconnect);
    }

    #[test]
    fn recognizes_inverse_symbols() {
        assert!(is_inverse_symbol("BTCUSD"));
        assert!(is_inverse_symbol("ETHUSDZ26"));
        assert!(!is_inverse_symbol("BTCUSDT"));
        assert!(!is_inverse_symbol("ETHUSDC"));
        assert!(!is_inverse_symbol("BTC-26DEC25"));
        assert!(!is_inverse_symbol("BTCUSD中"));
        assert_eq!(subtract(1000.1, 0.2), 999.9);
    }
    #[test]
    fn usdc_without_a_positive_public_mark_retains_native_unrealised_pnl() {
        use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
        let market = Market {
            id: "ETHUSDC".into(),
            symbol: "ETHUSDC".into(),
            pretty_symbol: None,
            base: "ETH".into(),
            quote: "USDC".into(),
            active: true,
            precision: Precision {
                amount: 0.01,
                price: 0.01,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.01,
                    max: 1000.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: 100.0,
                },
                price: None,
            },
            contract_size: Some(1.0),
            is_expiring: Some(false),
            is_inverse: Some(false),
            is_spot: Some(false),
            is_perpetual: Some(true),
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        };
        let native: PositionRow<'_> = serde_json::from_str(r#"{"symbol":"ETHUSDC","size":"2","avgPrice":"100","markPrice":"110","positionValue":"220","unrealisedPnl":"7","positionIdx":0,"side":"Buy","leverage":"10"}"#).unwrap();
        for mark in [None, Some(0.0), Some(115.0)] {
            let mut store = Store::default();
            store.set_markets(vec![market.clone()]);
            if let Some(mark) = mark {
                store.set_tickers(vec![Ticker {
                    id: "ETHUSDC".into(),
                    symbol: "ETHUSDC".into(),
                    mark,
                    ..Default::default()
                }]);
            }
            let mapped = map_position(&native, &store, MarginType::Cross, 1000.0).position;
            assert_eq!(mapped.contracts, 2.0);
            assert_eq!(
                mapped.unrealized_pnl,
                if mark == Some(115.0) { 30.0 } else { 7.0 }
            );
            assert!(mapped.notional > 0.0);
        }
    }
}
