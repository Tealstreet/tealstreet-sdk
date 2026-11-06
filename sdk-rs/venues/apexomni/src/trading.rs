//! Sequenced trading jobs; preflight and signatures complete before any placement.
use crate::{ApexOmni, Pending, sign};
use safe_cex_engine::classify::{
    CodeTable, Intent, Key, Native, NativeCode, Row, Verdict, WriteFact,
};
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::plan::{
    CallBound, RetryWindow, SerialCall, SerialPolicy, Slot, StepBound,
};
use safe_cex_engine::request_context::{
    Context, Credential, HttpOwnerRetirement, Operation, Plan, PlanPolicy, Preflight,
    PreflightPolicy, PreflightPurpose, RetryPolicy, UnprovenEvidence,
};
use safe_cex_engine::{
    Command, CommandId, CredentialSlot, Cx, ErrorKind, Outcome, Response, VenueError, num,
};
use serde::Deserialize;

const APEX: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Int(20016),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        },
        // "Requires Login: Your API key is wrong": api-docs.pro.apex.exchange, Errors.
        Row {
            key: Key::Int(10002),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(401),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(429),
            verdict: Verdict::Refused(ErrorKind::RateLimited),
            only: None,
        },
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[],
};
use std::collections::{HashSet, VecDeque};

#[derive(Clone)]
enum Accepted {
    Place,
    Cancel(Vec<String>),
    AmendCancel,
    /// A read of the amended order: its cancel's terminal proof.
    ConfirmCancel,
    CancelSymbol(String),
    Leverage(String, f64),
}
enum Step {
    Build(Vec<PlaceOrderOpts>),
    Amend(String, Option<f64>, Option<f64>),
    Write(
        sign::Request,
        Accepted,
        Vec<OrderIdentityReceipt>,
        Vec<[u8; 36]>,
    ),
    Delay,
}
struct Job {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    placements: bool,
    preflight: Preflight,
    plan: Option<Plan>,
    amending: bool,
    /// The amended order: its replacement's refusal leaves it gone.
    replaces: Option<Replaced>,
    /// An amended stop whose cancel landed: its put-back, if one is provable.
    protect: Option<Box<StopProtect>>,
    /// The put-back of a refused stop replacement is in flight.
    restoring: bool,
}
pub struct Request {
    command: CommandId,
    accepted: Accepted,
    plan: Plan,
    slot: Slot,
    context: Context,
    unsigned: sign::Request,
    contracts: Vec<[u8; 36]>,
    legacy: bool,
    late: LateTrade,
}
#[derive(serde::Serialize, Deserialize)]
struct LateTrade {
    cancel_ids: Vec<String>,
    clients: Vec<String>,
}
/// The order an amend asked for: a virtual leg's own kind and closing side, not its parent's.
type Replaced = (String, OrderSide, OrderType);
/// How a message names an amended order of `kind`.
fn what(kind: OrderType) -> &'static str {
    match kind {
        OrderType::StopLoss => "stop",
        OrderType::TakeProfit => "take profit",
        _ => "order",
    }
}
fn gone_text((symbol, side, kind): &Replaced, unknown: bool) -> String {
    if matches!(kind, OrderType::StopLoss | OrderType::TakeProfit) {
        safe_cex_engine::error::stop_gone_text(symbol, *side, *kind, unknown)
    } else {
        safe_cex_engine::error::order_gone_text(symbol, unknown)
    }
}
/// Cancel-proof reads before an amend's replacement is unproven: Unknown.
const CONFIRM_POLLS: u32 = 30;
const CONFIRM_MS: u32 = 100;
/// Put-back order ids kept for a later cancel naming the original.
const RESTORED_KEPT: usize = 64;

/// The last check before an amended stop's cancel: a refusal leaves it untouched.
#[derive(Clone)]
struct StopGuard {
    order: Order,
    parent: Option<Order>,
    trigger: f64,
    reference: StopTriggerType,
    /// The side a live trigger must stay on; None while nothing can fire at
    /// once (a leg of an unfilled entry).
    below: Option<bool>,
    /// A reduce-only or closing stop needs a position on the side it closes.
    closes: Option<PositionSide>,
}
impl StopGuard {
    /// A refusal is typed NotSent and leaves the order untouched.
    fn validate(&self, store: &safe_cex_engine::Store, now: f64) -> Result<(), VenueError> {
        use safe_cex_engine::protective_amend::{fresh_reference, refused};
        let unchanged = || format!("Your {} order was not changed", self.order.symbol);
        let same = |planned: &Order| store.order(&planned.id) == Some(planned);
        if !same(&self.order) || self.parent.as_ref().is_some_and(|parent| !same(parent)) {
            return Err(refused(Some(format!(
                "{}: it changed or filled before cancellation",
                unchanged()
            ))));
        }
        if let Some(side) = self.closes
            && !store
                .positions()
                .iter()
                .any(|p| p.symbol == self.order.symbol && p.side == side && p.contracts > 0.0)
        {
            return Err(refused(Some(format!(
                "{}: no position for it to protect",
                unchanged()
            ))));
        }
        let label = match self.reference {
            StopTriggerType::MarkPrice => "mark",
            StopTriggerType::IndexPrice => "index",
            StopTriggerType::LastPrice => "last",
        };
        let Some(price) = fresh_reference(store, &self.order.symbol, self.reference, now) else {
            return Err(refused(Some(format!(
                "{}: the {label} price reference is stale",
                unchanged()
            ))));
        };
        if let Some(below) = self.below
            && ((below && self.trigger >= price) || (!below && self.trigger <= price))
        {
            return Err(refused(Some(format!(
                "{}: the new trigger has already crossed the {label} price",
                unchanged()
            ))));
        }
        Ok(())
    }
}
/// An amended stop's final check and the exact original to put back.
struct StopProtect {
    guard: StopGuard,
    restore: PlaceOrderOpts,
    /// False once anything filled: only an untouched original is put back.
    restorable: bool,
}

struct Amendment {
    id: String,
    replaced: Replaced,
    order: PlaceOrderOpts,
    total_amount: f64,
    baseline_filled: f64,
    filled: f64,
    fill_total: f64,
    fills: Vec<(f64, f64)>,
    reported: f64,
    reported_at: f64,
    overflow: bool,
    terminal: bool,
    started: f64,
    stop: Option<Box<StopProtect>>,
    /// A cancel, fill or expiry of this order was seen: the cancel's proof.
    closed: bool,
    polls: u32,
    read_retried: bool,
}
impl Amendment {
    fn observe(&mut self) {
        let after_report = self
            .fills
            .iter()
            .filter(|(timestamp, _)| *timestamp > self.reported_at)
            .map(|(_, amount)| amount)
            .sum::<f64>();
        self.filled = self
            .filled
            .max(self.baseline_filled + self.fill_total)
            .max(self.reported + after_report);
    }
}
#[derive(Default)]
pub struct Trading {
    queue: VecDeque<Job>,
    active: Option<Job>,
    waiting: bool,
    amendment: Option<Amendment>,
    /// A put-back's original id and the ids standing in for it.
    restored: VecDeque<(String, Vec<String>)>,
}

/// An amend's write: its one native resend of a proven refusal stays.
fn mutation_call() -> SerialCall {
    SerialCall {
        auth: Default::default(),
        operation: Operation::Mutation,
        timeout_ms: 20_000,
        attempts: 2,
        pacing: None,
    }
}
/// A cancel-proof read; its one failure retry is the serial Read retry.
fn read_call() -> SerialCall {
    SerialCall {
        operation: Operation::Read,
        attempts: 1,
        ..mutation_call()
    }
}
fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
fn side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "BUY"
    } else {
        "SELL"
    }
}
fn pair(key: &str, value: impl Into<String>) -> (String, String) {
    (key.into(), value.into())
}
fn mint_id(
    account: &str,
    now: f64,
    issued: &mut HashSet<String>,
    mut entropy: impl FnMut() -> Result<String, String>,
) -> Result<String, String> {
    for _ in 0..16 {
        let hash = safe_cex_crypto::sha256(entropy()?.as_bytes());
        let suffix = 100_000 + u32::from_be_bytes(hash[..4].try_into().unwrap()) % 900_000;
        let id = format!("apexomni-{account}-{}-{suffix}", num::format(now.floor()));
        if issued.insert(id.clone()) {
            return Ok(id);
        }
    }
    Err("ApeX Omni could not allocate a unique client order ID".into())
}
fn generated_id(
    cx: &Cx<'_, Pending>,
    account: &str,
    now: f64,
    issued: &mut HashSet<String>,
) -> Result<String, String> {
    mint_id(account, now, issued, || {
        safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))
    })
}
fn parent_id(id: &str) -> &str {
    id.strip_suffix("__stop_loss")
        .or_else(|| id.strip_suffix("__take_profit"))
        .unwrap_or(id)
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && !id.chars().any(|ch| ch.is_whitespace() || ch == '\u{feff}')
}

fn lots(order: &PlaceOrderOpts, market: &Market) -> Result<Vec<f64>, String> {
    let amount = num::adjust(order.amount, market.precision.amount);
    if !positive(order.amount) || !positive(amount) || amount < market.limits.amount.min {
        return Err("ApeX Omni order size is below the market minimum".into());
    }
    let maximum = market.limits.amount.max;
    if !positive(maximum) || !positive(market.precision.amount) {
        return Err("ApeX Omni market has invalid lot limits".into());
    }
    if amount <= maximum {
        return Ok(vec![amount]);
    }
    if order.stop_loss.is_some() || order.take_profit.is_some() {
        return Err("ApeX Omni cannot atomically attach TP/SL to split orders".into());
    }
    // Allocate whole ticks so splitting cannot create or lose a tradable unit.
    let ticks = (amount / market.precision.amount).round();
    let max_ticks = (maximum / market.precision.amount).floor();
    let min_ticks = (market.limits.amount.min / market.precision.amount)
        .ceil()
        .max(1.0);
    let count = (ticks / max_ticks).ceil();
    if !count.is_finite() || !(1.0..=1000.0).contains(&count) || ticks > 9_007_199_254_740_991.0 {
        return Err("ApeX Omni order requires too many lots".into());
    }
    let base = (ticks / count).floor();
    if base < min_ticks {
        return Err("ApeX Omni split size is below the market minimum".into());
    }
    let remainder = (ticks - base * count) as usize;
    Ok((0..count as usize)
        .map(|i| {
            num::adjust(
                (base + f64::from(i < remainder)) * market.precision.amount,
                market.precision.amount,
            )
        })
        .collect())
}

fn validate(orders: &[PlaceOrderOpts], cx: &Cx<'_, Pending>) -> Result<(), String> {
    let mut ids = HashSet::new();
    for order in orders {
        if order.stp_mode.is_some() {
            return Err("ApeX Omni does not support per-order self-trade prevention".into());
        }
        if order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some() {
            return Err("Bybit BBO and TickSize controls are not supported by ApeX Omni".into());
        }
        let market = cx
            .store
            .market(&order.symbol)
            .ok_or_else(|| format!("ApeX Omni market {} not found", order.symbol))?;
        if let Some(id) = &order.client_order_id {
            if !valid_id(id) {
                return Err(
                    "ApeX Omni client order ID must be non-empty without whitespace".into(),
                );
            }
            if !ids.insert(id) {
                return Err("ApeX Omni batch contains duplicate client order IDs".into());
            }
            if !matches!(order.kind, OrderType::Limit | OrderType::Market)
                || order.trigger_price.is_some()
                || order.stop_loss.is_some()
                || order.take_profit.is_some()
                || order.amount > market.limits.amount.max
            {
                return Err(
                    "ApeX Omni caller client order ID requires one plain Limit/Market order".into(),
                );
            }
        }
        if matches!(
            order.kind,
            OrderType::Conditional | OrderType::TrailingStopLoss
        ) {
            return Err(
                "ApeX Omni does not support opening conditional or trailing-stop orders".into(),
            );
        }
        if order.kind == OrderType::Limit && !order.price.is_some_and(positive) {
            return Err("ApeX Omni limit order requires a positive price".into());
        }
        if matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && !order.trigger_price.or(order.price).is_some_and(positive)
        {
            return Err("ApeX Omni trigger order requires a positive trigger price".into());
        }
        if [
            order.price,
            order.trigger_price,
            order.stop_loss,
            order.take_profit,
        ]
        .into_iter()
        .flatten()
        .any(|v| !positive(v))
        {
            return Err("ApeX Omni order prices must be finite and positive".into());
        }
        lots(order, market)?;
    }
    Ok(())
}

fn replacement(
    cx: &Cx<'_, Pending>,
    id: &str,
    price: Option<f64>,
    amount: Option<f64>,
) -> Result<PlaceOrderOpts, String> {
    let target = cx
        .store
        .order(id)
        .ok_or_else(|| format!("ApeX Omni order {id} not found"))?;
    let base_id = parent_id(id);
    let parent = cx
        .store
        .order(base_id)
        .ok_or_else(|| format!("ApeX Omni parent order {base_id} not found"))?;
    if !matches!(parent.status, OrderStatus::Open | OrderStatus::Creating)
        || parent.remaining <= 0.0
    {
        return Err("ApeX Omni cannot amend a terminal order".into());
    }
    let tp = cx.store.order(&format!("{base_id}__take_profit"));
    let sl = cx.store.order(&format!("{base_id}__stop_loss"));
    let mut order = PlaceOrderOpts {
        symbol: parent.symbol.clone(),
        kind: parent.kind,
        side: parent.side,
        amount: amount
            .map(|total| safe_cex_engine::num::left_of_total(total, parent.filled))
            .transpose()?
            .unwrap_or(parent.remaining),
        price: Some(parent.price),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: sl.map(|o| o.price).or(parent.stop_loss),
        take_profit: tp.map(|o| o.price).or(parent.take_profit),
        reduce_only: Some(parent.reduce_only),
        time_in_force: parent.time_in_force,
        trigger_type: parent.trigger_type,
        tp_trigger_type: tp
            .and_then(|o| o.trigger_type)
            .or(parent.extra.as_ref().and_then(|e| e.tp_trigger_type)),
        sl_trigger_type: sl
            .and_then(|o| o.trigger_type)
            .or(parent.extra.as_ref().and_then(|e| e.sl_trigger_type)),
        client_order_id: None,
        close: parent.close,
        stp_mode: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    };
    if let Some(price) = price {
        if target.parent_id.is_some() || id != base_id {
            if id.ends_with("__take_profit") {
                order.take_profit = Some(price);
            } else {
                order.stop_loss = Some(price);
            }
        } else {
            order.price = Some(price);
        }
    }
    Ok(order)
}

/// For a stop or take-profit edit: its final pre-cancel check, checked now
/// too, and the exact original to put back (`order` is the edited replacement).
fn stop_protect(
    cx: &Cx<'_, Pending>,
    id: &str,
    order: &PlaceOrderOpts,
) -> Result<Option<Box<StopProtect>>, VenueError> {
    let (Some(target), Some(parent)) = (cx.store.order(id), cx.store.order(parent_id(id))) else {
        return Err(not_sent(format!("ApeX Omni order {id} not found")));
    };
    if !matches!(target.kind, OrderType::StopLoss | OrderType::TakeProfit) {
        return Ok(None);
    }
    let market = cx
        .store
        .market(&parent.symbol)
        .ok_or_else(|| not_sent("ApeX Omni market not found"))?;
    let leg = target.id != parent.id;
    let stops = target.kind == OrderType::StopLoss;
    let trigger = if !leg {
        order.trigger_price.or(order.price)
    } else if stops {
        order.stop_loss
    } else {
        order.take_profit
    }
    .map(|price| num::adjust(price, market.precision.price))
    .ok_or_else(|| not_sent("ApeX Omni amendment has no trigger"))?;
    // A leg acts only once its entry fills; a standalone stop acts at once.
    let live = !leg || parent.filled > 0.0;
    let closes = (!leg && (parent.reduce_only || parent.close == Some(true))).then(|| {
        if target.side == OrderSide::Sell {
            PositionSide::Long
        } else {
            PositionSide::Short
        }
    });
    // The source prepare_order sends: a stop loss on INDEX unless asked for
    // last, a take profit on MARKET (last) unless asked for index.
    let reference = match (stops, order.sl_trigger_type, order.tp_trigger_type) {
        (true, Some(StopTriggerType::LastPrice), _) => StopTriggerType::LastPrice,
        (true, ..) => StopTriggerType::IndexPrice,
        (false, _, Some(StopTriggerType::IndexPrice)) => StopTriggerType::IndexPrice,
        (false, ..) => StopTriggerType::LastPrice,
    };
    let guard = StopGuard {
        order: target.clone(),
        parent: leg.then(|| parent.clone()),
        trigger,
        reference,
        below: live.then_some(stops == (target.side == OrderSide::Sell)),
        closes,
    };
    guard.validate(cx.store, cx.now)?;
    Ok(Some(Box::new(StopProtect {
        guard,
        restore: replacement(cx, id, None, None).map_err(not_sent)?,
        restorable: parent.filled == 0.0,
    })))
}

impl ApexOmni {
    fn prepare_order(
        &self,
        cx: &Cx<'_, Pending>,
        order: &PlaceOrderOpts,
        issued: &mut HashSet<String>,
    ) -> Result<Vec<Step>, String> {
        if !self.account.initialized {
            return Err("ApeX Omni account signing state is unavailable".into());
        }
        let market = cx
            .store
            .market(&order.symbol)
            .ok_or("ApeX Omni market not found")?;
        let config = self
            .contracts
            .get(&market.id)
            .or_else(|| self.contracts.get(&market.symbol))
            .ok_or("ApeX Omni contract signing config is unavailable")?;
        let tick = market.precision.price;
        let price = match order.kind {
            OrderType::Limit => num::adjust(order.price.unwrap_or(0.0), tick),
            OrderType::Market => {
                let mark = cx.store.ticker(&market.symbol).map_or(0.0, |t| t.mark);
                if !positive(mark) {
                    return Err("ApeX Omni market order requires a current mark price".into());
                }
                num::adjust(
                    mark * if order.side == OrderSide::Buy {
                        1.02
                    } else {
                        0.98
                    },
                    tick,
                )
            }
            _ => num::adjust(
                num::adjust(order.trigger_price.or(order.price).unwrap_or(0.0), tick)
                    * if order.side == OrderSide::Buy {
                        1.005
                    } else {
                        0.995
                    },
                tick,
            ),
        };
        if !positive(price) {
            return Err("ApeX Omni execution price rounds to zero".into());
        }
        let expiration =
            (((cx.now + 30.0 * 86_400_000.0) / 3_600_000.0).ceil() + 168.0) * 3_600_000.0;
        let taker_fee = self
            .account
            .taker_fee
            .parse::<f64>()
            .map_err(|_| "ApeX Omni invalid taker fee")?;
        if !taker_fee.is_finite() || taker_fee < 0.0 {
            return Err("ApeX Omni invalid taker fee".into());
        }
        let mut steps = Vec::new();
        for amount in lots(order, market)? {
            let id =
                order.client_order_id.clone().map(Ok).unwrap_or_else(|| {
                    generated_id(cx, &self.account.account_id, cx.now, issued)
                })?;
            let mut receipts = vec![OrderIdentityReceipt {
                symbol: order.symbol.clone(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: id.clone(),
            }];
            let size = num::format(amount);
            let price_text = num::format(price);
            let mut contracts = vec![sign::contract_payload(
                &id,
                &self.account.account_id,
                config.pair_id,
                &size,
                &price_text,
                &self.account.maker_fee,
                &self.account.taker_fee,
                order.side == OrderSide::Buy,
            )?];
            let signature = sign::zk_placeholder(0);
            let mut fields = vec![
                pair("symbol", market.id.clone()),
                pair("side", side(order.side)),
                pair(
                    "type",
                    match order.kind {
                        OrderType::Limit => "LIMIT",
                        OrderType::Market => "MARKET",
                        OrderType::StopLoss => "STOP_LIMIT",
                        OrderType::TakeProfit => "TAKE_PROFIT_LIMIT",
                        _ => unreachable!(),
                    },
                ),
                pair("size", size.clone()),
                pair("price", price_text),
                pair(
                    "timeInForce",
                    match order.time_in_force.unwrap_or(TimeInForce::GoodTillCancel) {
                        TimeInForce::GoodTillCancel => "GOOD_TIL_CANCEL",
                        TimeInForce::ImmediateOrCancel => "IMMEDIATE_OR_CANCEL",
                        TimeInForce::FillOrKill => "FILL_OR_KILL",
                        TimeInForce::PostOnly => "POST_ONLY",
                    },
                ),
                pair("brokerId", "6587"),
                pair("clientId", id.clone()),
                pair("clientOrderId", id),
                pair("signature", signature),
                pair("expiration", num::format(expiration)),
                pair(
                    "limitFee",
                    sign::limit_fee(&num::format(price), &size, &self.account.taker_fee)?,
                ),
            ];
            // Placement's caller serializes once with falsy filtering before API auth.
            if order.reduce_only.unwrap_or(false) {
                fields.push(pair("reduceOnly", "true"));
            }
            if matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit) {
                let trigger = num::adjust(order.trigger_price.or(order.price).unwrap(), tick);
                fields.push(pair("triggerPrice", num::format(trigger)));
                let kind = if order.kind == OrderType::StopLoss {
                    order.sl_trigger_type
                } else {
                    order.tp_trigger_type
                };
                let trigger_type = if order.kind == OrderType::StopLoss {
                    if kind == Some(StopTriggerType::LastPrice) {
                        "MARKET"
                    } else {
                        "INDEX"
                    }
                } else if kind == Some(StopTriggerType::IndexPrice) {
                    "INDEX"
                } else {
                    "MARKET"
                };
                fields.push(pair("triggerPriceType", trigger_type));
            }
            if order.take_profit.is_some() || order.stop_loss.is_some() {
                fields.push(pair("isOpenTpslOrder", "true"));
            }
            for (prefix, trigger, trigger_type) in [
                ("tp", order.take_profit, order.tp_trigger_type),
                ("sl", order.stop_loss, order.sl_trigger_type),
            ] {
                let Some(trigger) = trigger else {
                    continue;
                };
                let trigger = num::adjust(trigger, tick);
                let opposite = order.side.opposite();
                let leg_price = num::adjust(
                    trigger
                        * if opposite == OrderSide::Buy {
                            1.005
                        } else {
                            0.995
                        },
                    tick,
                );
                if !positive(leg_price) || !positive(trigger) {
                    return Err("ApeX Omni bracket price rounds to zero".into());
                }
                let leg_id = generated_id(cx, &self.account.account_id, cx.now, issued)?;
                receipts.push(OrderIdentityReceipt {
                    symbol: order.symbol.clone(),
                    kind: OrderIdentityKind::ClientOrderId,
                    identity: leg_id.clone(),
                });
                let signature = sign::zk_placeholder(contracts.len());
                contracts.push(sign::contract_payload(
                    &leg_id,
                    &self.account.account_id,
                    config.pair_id,
                    &size,
                    &num::format(leg_price),
                    &self.account.maker_fee,
                    &self.account.taker_fee,
                    opposite == OrderSide::Buy,
                )?);
                fields.push(pair(
                    if prefix == "tp" {
                        "isSetOpenTp"
                    } else {
                        "isSetOpenSl"
                    },
                    "true",
                ));
                let trigger_kind = if prefix == "tp" {
                    if trigger_type == Some(StopTriggerType::IndexPrice) {
                        "INDEX"
                    } else {
                        "MARKET"
                    }
                } else if trigger_type == Some(StopTriggerType::LastPrice) {
                    "MARKET"
                } else {
                    "INDEX"
                };
                for (key, value) in [
                    ("ClientOrderId", leg_id),
                    ("Expiration", num::format(expiration)),
                    (
                        "LimitFee",
                        sign::limit_fee(&num::format(leg_price), &size, &self.account.taker_fee)?,
                    ),
                    ("Price", num::format(leg_price)),
                    ("Side", side(opposite).into()),
                    ("Signature", signature),
                    ("Size", size.clone()),
                    ("TriggerPrice", num::format(trigger)),
                    ("TriggerPriceType", trigger_kind.into()),
                ] {
                    fields.push(pair(&format!("{prefix}{key}"), value));
                }
            }
            steps.push(Step::Write(
                sign::Request::raw("/v3/order", sign::query(&fields)),
                Accepted::Place,
                receipts,
                contracts,
            ));
        }
        Ok(steps)
    }

    pub(crate) fn trading_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Result<(), Command> {
        let read_only = matches!(
            &command,
            Command::SetMarginType { .. }
                | Command::SetPositionMode { .. }
                | Command::FetchLeverage { .. }
        );
        if !read_only
            && !matches!(
                &command,
                Command::PlaceOrders { .. }
                    | Command::AmendOrder { .. }
                    | Command::CancelOrders { .. }
                    | Command::CancelSymbolOrders { .. }
                    | Command::SetLeverage { .. }
            )
        {
            return Err(command);
        }
        let credential = self.options.identity.as_ref().map(|identity| Credential {
            slot: CredentialSlot::Primary,
            reference: identity.credential.clone(),
        });
        // An amend reads its cancel's proof with the same (only) ApeX key.
        let amend = matches!(&command, Command::AmendOrder { .. });
        let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
            purpose: if read_only {
                PreflightPurpose::ReadOnly
            } else {
                PreflightPurpose::WriteAfterReads
            },
            read_credential: credential.clone().filter(|_| amend),
            write_credential: if read_only { None } else { credential },
        }) else {
            return Ok(());
        };
        let (command, steps, placements) = match command {
            Command::PlaceOrders { command, orders } => {
                if let Err(error) = validate(&orders, cx) {
                    cx.command_error(command, &error);
                    let _ = cx
                        .fail_preflight(&preflight, not_sent(error).kind(ErrorKind::InvalidInput));
                    return Ok(());
                }
                (command, vec![Step::Build(orders)], true)
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => {
                if price.is_none() && amount.is_none() {
                    let _ = cx.fail_preflight(
                        &preflight,
                        not_sent("ApeX Omni amendment requires a price or amount"),
                    );
                    return Ok(());
                }
                (command, vec![Step::Amend(id, price, amount)], true)
            }
            Command::CancelOrders { command, mut ids } => {
                // A screen still showing a put-back's original cancels what stands for it.
                for (old, new) in &self.trading.restored {
                    if ids.iter().any(|id| parent_id(id) == old) {
                        ids.retain(|id| parent_id(id) != old);
                        ids.extend(new.iter().cloned());
                    }
                }
                let mut unique = HashSet::new();
                let mut steps = vec![];
                for id in ids {
                    let id = parent_id(&id).to_owned();
                    if !valid_id(&id) {
                        let error = "ApeX Omni cancel requires an order ID".to_owned();
                        cx.command_error(command, &error);
                        let _ = cx.fail_preflight(
                            &preflight,
                            not_sent(error).kind(ErrorKind::InvalidInput),
                        );
                        return Ok(());
                    }
                    if unique.insert(id.clone()) {
                        if unique.len() > 1 && (unique.len() - 1) % 10 == 0 {
                            steps.push(Step::Delay);
                        }
                        steps.push(Step::Write(
                            sign::Request::form("/v3/delete-order", vec![pair("id", id.clone())]),
                            Accepted::Cancel(vec![id]),
                            Vec::new(),
                            Vec::new(),
                        ));
                    }
                }
                (command, steps, false)
            }
            Command::CancelSymbolOrders { command, symbol } => {
                let Some(market) = cx.store.market(&symbol) else {
                    let _ = cx.fail_preflight(
                        &preflight,
                        not_sent(format!("ApeX Omni market {symbol} not found")),
                    );
                    return Ok(());
                };
                (
                    command,
                    vec![Step::Write(
                        sign::Request::form(
                            "/v3/delete-open-orders",
                            vec![pair("symbol", market.id.clone())],
                        ),
                        Accepted::CancelSymbol(symbol),
                        Vec::new(),
                        Vec::new(),
                    )],
                    false,
                )
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => {
                let Some(market) = cx.store.market(&symbol) else {
                    let _ = cx.fail_preflight(
                        &preflight,
                        not_sent(format!("ApeX Omni market {symbol} not found")),
                    );
                    return Ok(());
                };
                if !positive(leverage) {
                    let _ = cx.fail_preflight(
                        &preflight,
                        not_sent("ApeX Omni leverage must be finite and positive"),
                    );
                    return Ok(());
                }
                let leverage =
                    leverage.clamp(market.limits.leverage.min, market.limits.leverage.max);
                (
                    command,
                    vec![Step::Write(
                        sign::Request::multipart(
                            "/v3/set-initial-margin-rate",
                            vec![
                                pair("symbol", market.id.clone()),
                                pair("token", market.quote.clone()),
                                pair("initialMarginRate", format!("{:.5}", 1.0 / leverage)),
                            ],
                        ),
                        Accepted::Leverage(symbol, leverage),
                        Vec::new(),
                        Vec::new(),
                    )],
                    false,
                )
            }
            Command::SetMarginType {
                command: _,
                isolated,
                ..
            } => {
                if isolated {
                    let error = not_sent("ApeX Omni supports cross margin only");
                    let _ = cx.fail_preflight(&preflight, error.kind(ErrorKind::Unsupported));
                } else {
                    let _ = cx.done_preflight(&preflight, "null".into());
                }
                return Ok(());
            }
            Command::SetPositionMode {
                command: _,
                hedged,
                symbol: _,
            } => {
                if hedged {
                    let _ = cx.done_preflight(&preflight, "null".into());
                } else {
                    let error = not_sent("ApeX Omni supports hedged position mode only");
                    let _ = cx.fail_preflight(&preflight, error.kind(ErrorKind::Unsupported));
                }
                return Ok(());
            }
            Command::FetchLeverage { command: _, symbol } => {
                let result: Result<String, String> = cx
                    .store
                    .setting(&symbol)
                    .map(|s| num::format(s.leverage))
                    .ok_or_else(|| "ApeX Omni leverage is unavailable".into());
                match result {
                    Ok(data) => {
                        let _ = cx.done_preflight(&preflight, data);
                    }
                    Err(error) => {
                        let _ = cx.fail_preflight(&preflight, error);
                    }
                };
                return Ok(());
            }
            command => return Err(command),
        };
        if self.options.public_data_only {
            let _ = cx.fail_preflight(
                &preflight,
                not_sent("ApeX Omni private commands require API credentials")
                    .kind(ErrorKind::AuthInvalid),
            );
            return Ok(());
        }
        if self.trading.active.is_some() {
            if let Err(error) = cx.park_preflight(&preflight) {
                let _ = cx.fail_preflight(&preflight, error.venue_error());
                return Ok(());
            }
            self.trading.queue.push_back(Job {
                command,
                steps: steps.into(),
                ids: vec![],
                placements,
                preflight,
                plan: None,
                amending: false,
                replaces: None,
                protect: None,
                restoring: false,
            });
        } else {
            self.trading.active = Some(Job {
                command,
                steps: steps.into(),
                ids: vec![],
                placements,
                preflight,
                plan: None,
                amending: false,
                replaces: None,
                protect: None,
                restoring: false,
            });
            self.drive_trading(cx);
        }
        Ok(())
    }

    fn prepare_trading(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        job: &mut Job,
    ) -> Result<(), VenueError> {
        let mut prepared = VecDeque::new();
        while let Some(step) = job.steps.pop_front() {
            match step {
                Step::Build(orders) => {
                    let mut issued = cx
                        .store
                        .orders()
                        .iter()
                        .filter_map(|o| o.client_order_id.clone())
                        .chain(orders.iter().filter_map(|o| o.client_order_id.clone()))
                        .collect();
                    for order in orders {
                        prepared.extend(
                            self.prepare_order(cx, &order, &mut issued)
                                .map_err(not_sent)?,
                        );
                    }
                }
                Step::Amend(id, price, amount) => {
                    job.amending = true;
                    let order = replacement(cx, &id, price, amount).map_err(not_sent)?;
                    validate(std::slice::from_ref(&order), cx).map_err(not_sent)?;
                    let mut issued = cx
                        .store
                        .orders()
                        .iter()
                        .filter_map(|o| o.client_order_id.clone())
                        .collect();
                    let replacement = self
                        .prepare_order(cx, &order, &mut issued)
                        .map_err(not_sent)?;
                    let parent = cx
                        .store
                        .order(parent_id(&id))
                        .expect("replacement validated parent");
                    let amended = cx.store.order(&id).expect("replacement validated target");
                    let replaced = (amended.symbol.clone(), amended.side, amended.kind);
                    let stop = stop_protect(cx, &id, &order)?;
                    job.replaces = Some(replaced.clone());
                    prepared.push_back(Step::Write(
                        sign::Request::form(
                            "/v3/delete-order",
                            vec![pair("id", parent.id.clone())],
                        ),
                        Accepted::AmendCancel,
                        vec![],
                        vec![],
                    ));
                    prepared.extend(replacement);
                    self.trading.amendment = Some(Amendment {
                        id: parent.id.clone(),
                        replaced,
                        order,
                        total_amount: parent.amount,
                        baseline_filled: parent.filled,
                        filled: parent.filled,
                        fill_total: 0.0,
                        fills: vec![],
                        reported: parent.filled,
                        reported_at: cx.now,
                        overflow: false,
                        terminal: false,
                        started: cx.now,
                        stop,
                        closed: false,
                        polls: 0,
                        read_retried: false,
                    });
                }
                step => prepared.push_back(step),
            }
        }
        if prepared.is_empty() {
            cx.done_empty_plan(&job.preflight, "[]".into())
                .map_err(|e| e.venue_error())?;
            return Ok(());
        }
        let needs = if job.placements {
            sign::CONTRACT_NEEDS
        } else {
            sign::REST_NEEDS
        };
        cx.can_sign(CredentialSlot::Primary, needs)
            .map_err(|error| error.venue_error())?;
        let identity = self
            .options
            .identity
            .as_ref()
            .expect("preflight requires credential");
        let credential = Credential {
            slot: CredentialSlot::Primary,
            reference: identity.credential.clone(),
        };
        // An amend streams: its cancel's proof is read before the replacement,
        // and a refused stop replacement may be followed by one put-back.
        if self.trading.amendment.is_some() {
            let (plan, _) = cx
                .admit_serial_plan(
                    &job.preflight,
                    mutation_call(),
                    SerialPolicy {
                        credential: Some(credential),
                        pacing: None,
                        refusals_complete: false,
                    },
                )
                .map_err(|error| error.venue_error())?;
            job.steps = prepared;
            job.plan = Some(plan);
            return Ok(());
        }
        let mut waits = Vec::new();
        let mut wait = 0;
        for step in &prepared {
            match step {
                Step::Delay => wait = 1000,
                Step::Write(..) => {
                    waits.push(wait);
                    wait = 0;
                }
                _ => unreachable!("native preparation expanded"),
            }
        }
        let calls: Vec<_> = waits
            .iter()
            .map(|_| {
                [CallBound {
                    timeout_ms: 20_000,
                    attempts: 2,
                    retry_window: RetryWindow::FreshAttempt,
                    after_refusal: &[],
                }]
            })
            .collect();
        let steps: Vec<_> = calls
            .iter()
            .zip(waits)
            .map(|(calls, wait_ms)| StepBound { wait_ms, calls })
            .collect();
        let plan = cx
            .admit_plan(
                &job.preflight,
                &steps,
                PlanPolicy {
                    independent_calls: false,
                    operation: Operation::Mutation,
                    credential: Some(credential),
                    pacing: None,
                    retry: RetryPolicy::DefinitiveRefusal,
                },
            )
            .map_err(|error| error.venue_error())?;
        prepared.retain(|step| !matches!(step, Step::Delay));
        job.steps = prepared;
        job.plan = Some(plan);
        Ok(())
    }

    pub(crate) fn drive_trading(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.waiting {
            return;
        }
        let Some(mut job) = self.trading.active.take() else {
            return;
        };
        if job.plan.is_none() {
            if let Err(error) = self.prepare_trading(cx, &mut job) {
                let _ = cx.fail_preflight(&job.preflight, error);
                self.trading.active = Some(job);
                return;
            }
            if job.plan.is_none() {
                self.trading.active = Some(job);
                return;
            }
        }
        let plan = job.plan.as_ref().unwrap().clone();
        let slot = match cx.plan_slots(&plan) {
            Ok(slots) => slots.into_iter().next(),
            Err(error) => {
                let _ = cx.fail_plan(&plan, error.venue_error());
                self.trading.active = Some(job);
                return;
            }
        };
        let Some(slot) = slot else {
            self.trading.active = Some(job);
            return;
        };
        let Some(Step::Write(unsigned, accepted, receipts, contracts)) = job.steps.pop_front()
        else {
            self.trading.active = Some(job);
            let _ = cx.fail_plan(
                &plan,
                VenueError::new("ApeX Omni native plan has no next request")
                    .kind(ErrorKind::Internal),
            );
            return;
        };
        let intent = match &accepted {
            Accepted::Place => Some(Intent::Place),
            Accepted::Cancel(_) | Accepted::CancelSymbol(_) => Some(Intent::Cancel),
            Accepted::AmendCancel => Some(Intent::Amend),
            Accepted::ConfirmCancel => None,
            Accepted::Leverage(..) => Some(Intent::Setting),
        };
        let context = match intent {
            Some(intent) => cx.plan_leg_context_with_intent(&plan, &slot, intent),
            None => cx.plan_leg_context(&plan, &slot),
        };
        let context = match context {
            Ok(context) => context,
            Err(error) => {
                self.trading.active = Some(job);
                let _ = cx.fail_plan(&plan, error.venue_error());
                return;
            }
        };
        let cancel_ids = match &accepted {
            Accepted::Cancel(ids) => ids.clone(),
            Accepted::AmendCancel => self
                .trading
                .amendment
                .as_ref()
                .map(|amendment| vec![amendment.id.clone()])
                .unwrap_or_default(),
            Accepted::CancelSymbol(symbol) => cx
                .store
                .orders()
                .iter()
                .filter(|order| &order.symbol == symbol)
                .map(|order| parent_id(&order.id).to_owned())
                .collect(),
            _ => vec![],
        };
        let late = LateTrade {
            cancel_ids,
            clients: receipts
                .iter()
                .map(|receipt| receipt.identity.clone())
                .collect(),
        };
        // A 408 or 5xx on a put-back is sealed before this venue sees it:
        // say what is unknown rather than repeat the replacement's refusal.
        if job.restoring
            && matches!(accepted, Accepted::Place)
            && let Some((symbol, _, kind)) = &job.replaces
        {
            let words = format!(
                "Your {symbol} {}'s replacement was refused and its put-back's outcome is unknown. Check Open Orders",
                what(*kind)
            );
            let _ = cx.present_plan_failure(&plan, words);
        }
        cx.command_progress(job.command, receipts);
        let delivery = Request {
            command: job.command,
            accepted,
            plan,
            slot,
            context,
            unsigned,
            contracts,
            legacy: false,
            late,
        };
        self.trading.active = Some(job);
        self.send_trading(cx, delivery);
    }

    fn send_trading(&mut self, cx: &mut Cx<'_, Pending>, delivery: Request) {
        let identity = self
            .options
            .identity
            .as_ref()
            .expect("admitted identity")
            .clone();
        let query = sign::query(&delivery.unsigned.params);
        let url = format!(
            "{}{}{}{}",
            self.rest,
            delivery.unsigned.path,
            if query.is_empty() { "" } else { "?" },
            query
        );
        let recipe = sign::Http::with_contracts(
            identity,
            delivery.unsigned.clone(),
            delivery.legacy,
            delivery.contracts.clone(),
        );
        self.trading.waiting = true;
        if let Err((Pending::Trade(delivery), error)) = cx.request_signed_managed(
            delivery.context.clone(),
            None,
            delivery.unsigned.method,
            url,
            None,
            Box::new(recipe),
            Pending::Trade(delivery),
        ) {
            self.trading_failed(cx, delivery, error.venue_error());
        }
    }

    pub(crate) fn trading_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        delivery: Request,
        error: VenueError,
    ) {
        if matches!(delivery.accepted, Accepted::ConfirmCancel) {
            return self.confirm_failed(cx, &delivery, false);
        }
        // A released call that failed without a refusal may have executed.
        let unknown = !matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent));
        let _ = self.fail_delivery(cx, &delivery, error, unknown);
    }

    /// Fails a call's Plan, saying so when it was an amend's replacement.
    fn fail_delivery(
        &self,
        cx: &mut Cx<'_, Pending>,
        delivery: &Request,
        error: impl Into<VenueError>,
        unknown: bool,
    ) -> Result<(), safe_cex_engine::request_context::Error> {
        let job = self
            .trading
            .active
            .as_ref()
            .filter(|job| matches!(delivery.accepted, Accepted::Place) && job.ids.is_empty());
        let gone = job
            .and_then(|job| job.replaces.as_ref())
            .map(|replaced| gone_text(replaced, unknown));
        let mut error = error.into();
        // A stop's cancel landed and nothing stands in its place.
        if let Some(job) = job.filter(|job| job.protect.is_some())
            && let Some((symbol, _, kind)) = &job.replaces
        {
            let what = what(*kind);
            error.presents = true;
            error.message = match (unknown, job.restoring) {
                (false, false) => format!(
                    "Your {symbol} {what} was removed: its replacement was refused, please re-place it ({})",
                    error.message
                ),
                (false, true) => format!(
                    "Your {symbol} {what} was removed: its replacement and the put-back were both refused, please re-place it ({})",
                    error.message
                ),
                (true, true) => format!(
                    "Your {symbol} {what}'s replacement was refused and its put-back's outcome is unknown. Check Open Orders ({})",
                    error.message
                ),
                (true, false) => error.message,
            };
            if !unknown {
                error.protection = Some(safe_cex_engine::error::Protection::Unprotected);
            }
        }
        match gone {
            Some(gone) => cx.fail_plan_noted(&delivery.plan, error, &gone),
            None => cx.fail_plan(&delivery.plan, error),
        }
    }

    /// The final-claim re-check of an amended stop's cancel, every send of it.
    pub(crate) fn trading_release_veto(
        &self,
        store: &safe_cex_engine::Store,
        pending: &Pending,
        release: &safe_cex_engine::request_context::HttpRelease,
    ) -> Result<(), VenueError> {
        let Pending::Trade(Request {
            accepted: Accepted::AmendCancel,
            ..
        }) = pending
        else {
            return Ok(());
        };
        match self
            .trading
            .amendment
            .as_ref()
            .and_then(|a| a.stop.as_ref())
        {
            Some(stop) => stop.guard.validate(store, release.time.raw_ms),
            None => Ok(()),
        }
    }

    pub(crate) fn trading_retired(&mut self, owner: &HttpOwnerRetirement) -> Option<Preflight> {
        self.trading
            .queue
            .retain(|job| !owner.matches_preflight(&job.preflight));
        if self
            .trading
            .active
            .as_ref()
            .is_some_and(|job| owner.matches_preflight(&job.preflight))
        {
            self.trading.active = None;
            self.trading.waiting = false;
            self.trading.amendment = None;
            return self.trading.queue.front().map(|job| job.preflight.clone());
        }
        None
    }

    pub(crate) fn trading_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        if self
            .trading
            .active
            .as_ref()
            .and_then(|job| job.plan.as_ref())
            .is_some_and(|active| active.owner() == plan.owner())
        {
            self.drive_trading(cx);
        }
    }

    pub(crate) fn trading_ready(&mut self, cx: &mut Cx<'_, Pending>, preflight: &Preflight) {
        if self.trading.active.is_none()
            && self
                .trading
                .queue
                .front()
                .is_some_and(|job| job.preflight.owner() == preflight.owner())
        {
            self.trading.active = self.trading.queue.pop_front();
            self.drive_trading(cx);
        }
    }

    pub(crate) fn trading_order_progress(
        &mut self,
        id: &str,
        filled: f64,
        terminal: bool,
        closed: bool,
        timestamp: f64,
    ) {
        if let Some(amendment) = self.trading.amendment.as_mut()
            && amendment.id == id
            && filled.is_finite()
            && filled >= 0.0
        {
            amendment.closed |= closed;
            if filled > amendment.reported && timestamp.is_finite() {
                amendment.reported = filled;
                amendment.reported_at = timestamp;
            }
            amendment.filled = amendment.filled.max(filled);
            amendment.observe();
            amendment.terminal |= terminal;
        }
    }
    pub(crate) fn trading_fill_progress(&mut self, id: &str, amount: f64, timestamp: f64) {
        if let Some(amendment) = self.trading.amendment.as_mut()
            && amendment.id == id
            && positive(amount)
            && timestamp.is_finite()
            && timestamp >= amendment.started
        {
            if amendment.fills.len() == 4096 {
                amendment.overflow = true;
                return;
            }
            amendment.fill_total += amount;
            amendment.fills.push((timestamp, amount));
            // Only executions after a cumulative report's watermark add to it.
            amendment.observe();
        }
    }
    /// The amended order's cancel is proven: size and queue its replacement.
    /// False once the Plan has failed.
    fn replace_after_cancel(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) -> bool {
        let Some(mut amendment) = self.trading.amendment.take() else {
            let _ = cx.fail_plan(plan, "ApeX Omni amendment state is unavailable");
            return false;
        };
        if let Some(mut stop) = amendment.stop.take()
            && let Some(job) = self.trading.active.as_mut()
        {
            stop.restorable &= amendment.filled <= amendment.baseline_filled;
            job.protect = Some(stop);
        }
        if amendment.overflow {
            let error = "ApeX Omni amendment exceeded the observed fill limit";
            let _ = cx.fail_plan(plan, error);
            return false;
        }
        // The amended total less fills: those since planning come off too.
        let additional =
            crate::account::subtract(amendment.filled, amendment.baseline_filled).max(0.0);
        amendment.order.amount = crate::account::subtract(amendment.order.amount, additional);
        if !amendment.terminal
            && amendment.filled < amendment.total_amount
            && amendment.order.amount > 0.0
        {
            let prepared = if amendment.filled > amendment.baseline_filled {
                validate(std::slice::from_ref(&amendment.order), cx).and_then(|()| {
                    let mut issued = cx
                        .store
                        .orders()
                        .iter()
                        .filter_map(|o| o.client_order_id.clone())
                        .collect();
                    self.prepare_order(cx, &amendment.order, &mut issued)
                })
            } else {
                Ok(self
                    .trading
                    .active
                    .as_mut()
                    .unwrap()
                    .steps
                    .drain(..)
                    .collect())
            };
            match prepared {
                Ok(prepared) => {
                    if let Some(job) = self.trading.active.as_mut() {
                        job.steps = prepared.into();
                    }
                }
                Err(error) => {
                    // The cancel landed: a replacement that cannot be built is refused.
                    let gone = gone_text(&amendment.replaced, false);
                    let _ = cx.fail_plan_noted(plan, error, &gone);
                    return false;
                }
            }
        } else if let Some(job) = self.trading.active.as_mut() {
            job.steps.clear();
        }
        true
    }

    /// Reads the amended order again after a pause: its cancel's proof.
    fn confirm_next(&mut self, cx: &mut Cx<'_, Pending>, delivery: &Request, id: &str) {
        if let Some(job) = self.trading.active.as_mut() {
            job.steps.push_front(Step::Write(
                sign::Request::get("/v3/order", vec![pair("id", id)]),
                Accepted::ConfirmCancel,
                vec![],
                vec![],
            ));
        }
        self.trading.waiting = false;
        if let Err(error) = cx.continue_serial_plan_after(
            &delivery.plan,
            &delivery.slot,
            &delivery.context,
            CONFIRM_MS,
            read_call(),
        ) {
            let _ = cx.fail_plan(&delivery.plan, error.venue_error());
        }
    }

    /// A cancel-proof read: CANCELED, FILLED or EXPIRED proves the cancel landed.
    fn confirm_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        delivery: Request,
        response: Response<'_>,
    ) {
        #[derive(Deserialize)]
        struct Envelope<'a> {
            #[serde(borrow)]
            data: Option<crate::private_wire::OrderRow<'a>>,
        }
        let Some(amendment) = self.trading.amendment.as_mut() else {
            let _ = cx.fail_plan(&delivery.plan, "ApeX Omni amendment state is unavailable");
            return;
        };
        let row = serde_json::from_str::<Envelope<'_>>(response.body)
            .ok()
            .filter(|_| (200..300).contains(&response.status))
            .and_then(|envelope| envelope.data)
            .filter(|row| row.id == amendment.id);
        let Some(row) = row else {
            return self.confirm_failed(cx, &delivery, true);
        };
        if matches!(row.status.as_ref(), "CANCELED" | "FILLED" | "EXPIRED") {
            amendment.filled = amendment
                .filled
                .max(row.cum_success_fill_size.unwrap_or(0.0));
            amendment.terminal |= row.status != "CANCELED";
            amendment.closed = true;
        }
        if !amendment.closed {
            if amendment.polls >= CONFIRM_POLLS {
                return self.cancel_unproven(cx, &delivery, true);
            }
            amendment.polls += 1;
            let id = amendment.id.clone();
            return self.confirm_next(cx, &delivery, &id);
        }
        if self.replace_after_cancel(cx, &delivery.plan) {
            self.continue_amend(cx, &delivery);
        }
    }

    /// A failed cancel-proof read is retried once; then the cancel is unproven.
    fn confirm_failed(&mut self, cx: &mut Cx<'_, Pending>, delivery: &Request, data: bool) {
        match self.trading.amendment.as_mut() {
            Some(amendment) if !amendment.read_retried => {
                amendment.read_retried = true;
                let id = amendment.id.clone();
                self.confirm_next(cx, delivery, &id);
            }
            _ => self.cancel_unproven(cx, delivery, data),
        }
    }

    /// The cancel was taken but never proven: nothing replaces or restores it.
    fn cancel_unproven(&mut self, cx: &mut Cx<'_, Pending>, delivery: &Request, data: bool) {
        self.trading.waiting = false;
        self.trading.amendment = None;
        let what = self
            .trading
            .active
            .as_ref()
            .and_then(|job| job.replaces.as_ref())
            .map_or_else(
                || "order".to_owned(),
                |(symbol, _, kind)| format!("{symbol} {}", what(*kind)),
            );
        let mut error = unknown(format!(
            "Your {what} was not replaced: its cancel was sent but could not be confirmed, so whether it still stands is unknown. Check Open Orders"
        ));
        error.presents = true;
        // Only here is it unknown whether the original stands: reconcile it.
        let _ = cx.inherit_plan_uncertainty(&delivery.plan);
        if data {
            let _ = cx.finish_plan_call(&delivery.plan, &delivery.slot, &delivery.context);
        }
        let _ = cx.fail_plan(&delivery.plan, error);
    }

    /// An amend's accepted call: the next one, or its terminal.
    fn continue_amend(&mut self, cx: &mut Cx<'_, Pending>, delivery: &Request) {
        self.trading.waiting = false;
        let job = self.trading.active.as_ref().expect("an active amend");
        if !job.steps.is_empty() {
            match cx.continue_serial_plan(
                &delivery.plan,
                &delivery.slot,
                &delivery.context,
                mutation_call(),
            ) {
                Ok(_) => self.drive_trading(cx),
                Err(error) => {
                    let _ = self.fail_delivery(cx, delivery, error.venue_error(), false);
                }
            }
            return;
        }
        let _ = cx.finish_plan_call(&delivery.plan, &delivery.slot, &delivery.context);
        if !job.restoring {
            let _ = cx.done_plan(&delivery.plan, serde_json::to_string(&job.ids).unwrap());
            return;
        }
        let (symbol, kind) = job
            .replaces
            .as_ref()
            .map_or((String::new(), OrderType::StopLoss), |(symbol, _, kind)| {
                (symbol.clone(), *kind)
            });
        let mut error = VenueError::new(format!(
            "Your {symbol} {} is unchanged: its replacement was refused, so the original was put back as a new order",
            what(kind)
        ));
        error.protection = Some(safe_cex_engine::error::Protection::Restored);
        error.presents = true;
        let ids = job.ids.clone();
        if let Some(protect) = &job.protect {
            let guard = &protect.guard;
            let old = guard.parent.as_ref().unwrap_or(&guard.order).id.clone();
            if self.trading.restored.len() >= RESTORED_KEPT {
                self.trading.restored.pop_front();
            }
            self.trading.restored.push_back((old, ids.clone()));
        }
        let _ = cx.fail_plan(&delivery.plan, error.partial_ids(&ids));
    }

    /// A definite refusal of a stop's replacement proves nothing replaced it:
    /// put the untouched original back, once.
    fn put_back(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        delivery: &Request,
        error: &VenueError,
    ) -> bool {
        let Some(job) = self.trading.active.as_ref() else {
            return false;
        };
        let Some(restore) = job
            .protect
            .as_ref()
            .filter(|protect| protect.restorable)
            .map(|protect| protect.restore.clone())
        else {
            return false;
        };
        if !matches!(delivery.accepted, Accepted::Place)
            || error.outcome != Some(Outcome::Rejected)
            || job.restoring
            || !job.ids.is_empty()
        {
            return false;
        }
        let mut issued = cx
            .store
            .orders()
            .iter()
            .filter_map(|o| o.client_order_id.clone())
            .collect();
        let Ok(steps) = self.prepare_order(cx, &restore, &mut issued) else {
            return false;
        };
        // One attempt: no native resend can send the put-back twice. A refused
        // continuation leaves the job as it was: the stop was removed.
        let put_back_call = SerialCall {
            attempts: 1,
            ..mutation_call()
        };
        if cx
            .continue_serial_plan(
                &delivery.plan,
                &delivery.slot,
                &delivery.context,
                put_back_call,
            )
            .is_err()
        {
            return false;
        }
        let job = self.trading.active.as_mut().expect("an active amend");
        job.steps = steps.into();
        job.restoring = true;
        self.trading.waiting = false;
        self.drive_trading(cx);
        true
    }

    pub(crate) fn trading_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        delivery: Request,
        response: Response<'_>,
    ) {
        if cx
            .validate_plan_delivery(&delivery.plan, &delivery.context)
            .is_err()
        {
            return;
        }
        if !self
            .trading
            .active
            .as_ref()
            .is_some_and(|job| job.command == delivery.command)
        {
            return;
        }
        if matches!(delivery.accepted, Accepted::ConfirmCancel) {
            return self.confirm_response(cx, delivery, response);
        }
        let envelope = serde_json::from_str::<crate::Envelope<'_>>(response.body).ok();
        let code = envelope
            .as_ref()
            .and_then(|e| e.code.as_ref())
            .and_then(strict_code);
        // ApeX answers success with no `code`: `{data, timeCost}`, a bare `{timeCost}`
        // (cancel-all, leverage) or an empty body (a TP/SL cancel). A `code` must be 0.
        let succeeded = response.body.trim().is_empty()
            || envelope
                .as_ref()
                .is_some_and(|e| e.code.is_none() || code == Some(0));
        if !succeeded || !(200..300).contains(&response.status) {
            let native = envelope.as_ref().map(|e| Native {
                code: code.map_or(NativeCode::None, NativeCode::Int),
                message: e.msg.as_ref(),
            });
            let message = envelope
                .as_ref()
                .map_or("ApeX Omni malformed write response", |e| e.msg.as_ref());
            let message = crate::failure_text(code.unwrap_or(i64::from(response.status)), message);
            let Ok(fact) = cx.classify_write(&delivery.context, &APEX, native.as_ref(), message)
            else {
                return;
            };
            let error = fact.error().clone();
            let grant = cx.http_write_fact(
                &delivery.context,
                WriteFact::Failed {
                    fact,
                    known_effects: false,
                },
                None,
            );
            if let Ok(Some(grant)) = grant {
                match cx.plan_resend_context(
                    &delivery.plan,
                    &delivery.slot,
                    &delivery.context,
                    grant,
                ) {
                    Ok(context) => {
                        let mut delivery = delivery;
                        delivery.context = context;
                        delivery.legacy = true;
                        self.send_trading(cx, delivery);
                    }
                    Err(_) => {
                        let _ = self.fail_delivery(cx, &delivery, error, false);
                    }
                }
            } else if !self.put_back(cx, &delivery, &error) {
                let unknown = error.outcome == Some(Outcome::Unknown);
                let _ = self.fail_delivery(cx, &delivery, error, unknown);
            }
            return;
        }
        #[derive(Deserialize)]
        struct Envelope {
            data: Option<Data>,
        }
        #[derive(Deserialize)]
        struct Data {
            id: String,
        }
        let placed = if matches!(delivery.accepted, Accepted::Place) {
            serde_json::from_str::<Envelope>(response.body)
                .ok()
                .and_then(|e| e.data)
                .filter(|d| !d.id.is_empty())
                .map(|d| d.id)
        } else {
            None
        };
        if matches!(delivery.accepted, Accepted::Place) && placed.is_none() {
            let message = "ApeX Omni placement response has no order ID";
            let _ = cx.http_write_unproven(&delivery.context, message);
            let error = VenueError::new(message).kind(ErrorKind::Internal);
            let _ = self.fail_delivery(cx, &delivery, error, true);
            return;
        }
        if cx
            .http_write_fact(
                &delivery.context,
                WriteFact::Accepted {
                    order_ids: if matches!(delivery.accepted, Accepted::Place) {
                        placed.iter().cloned().collect()
                    } else {
                        delivery.late.cancel_ids.clone()
                    },
                },
                None,
            )
            .is_err()
        {
            return;
        }
        match delivery.accepted.clone() {
            Accepted::Place => {
                self.trading
                    .active
                    .as_mut()
                    .unwrap()
                    .ids
                    .push(placed.unwrap());
                self.heal_orders(cx);
            }
            Accepted::Cancel(ids) => self.note_cancel(cx, &ids),
            Accepted::AmendCancel => {
                let Some(amendment) = self.trading.amendment.as_mut() else {
                    let _ =
                        cx.fail_plan(&delivery.plan, "ApeX Omni amendment state is unavailable");
                    return;
                };
                if let Some(parent) = cx.store.order(&amendment.id) {
                    amendment.filled = amendment.filled.max(parent.filled);
                    amendment.terminal |=
                        parent.remaining <= 0.0 || parent.status == OrderStatus::Closed;
                }
                amendment.closed |= amendment.terminal;
                let (id, closed) = (amendment.id.clone(), amendment.closed);
                self.note_cancel(cx, std::slice::from_ref(&id));
                // A 2xx only says the cancel was taken: its proof is read first.
                if !closed {
                    return self.confirm_next(cx, &delivery, &id);
                }
                if !self.replace_after_cancel(cx, &delivery.plan) {
                    return;
                }
            }
            Accepted::ConfirmCancel => unreachable!("cancel proof is read apart"),
            Accepted::CancelSymbol(symbol) => {
                let ids = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == symbol)
                    .map(|o| parent_id(&o.id).to_owned())
                    .collect::<Vec<_>>();
                self.note_cancel(cx, &ids);
            }
            Accepted::Leverage(symbol, leverage) => {
                self.account.note_leverage(&symbol, leverage, cx.now);
                if let Some(index) = cx.store.market_index(&symbol) {
                    let mut setting = cx.store.setting(&symbol).unwrap_or_default();
                    setting.leverage = leverage;
                    setting.buy_leverage = Some(leverage);
                    setting.sell_leverage = Some(leverage);
                    setting.margin_type = Some(MarginType::Cross);
                    cx.store.set_settings([(index, setting)]);
                }
                cx.store.update_positions(|position| {
                    if position.symbol == symbol {
                        position.leverage = leverage;
                    }
                });
                self.refresh_positions(cx);
            }
        }
        self.trading.waiting = false;
        let job = self.trading.active.as_ref().unwrap();
        if job.amending {
            return self.continue_amend(cx, &delivery);
        }
        if job.steps.is_empty() {
            let result = if job.placements {
                serde_json::to_string(&job.ids).unwrap()
            } else {
                "[]".into()
            };
            if cx
                .finish_plan_call(&delivery.plan, &delivery.slot, &delivery.context)
                .is_ok()
            {
                let _ = cx.done_plan(&delivery.plan, result);
            }
        } else if cx
            .finish_plan_call(&delivery.plan, &delivery.slot, &delivery.context)
            .is_ok()
        {
            self.drive_trading(cx);
        }
    }
}

pub(crate) fn capture_reconciliation(
    delivery: &Request,
) -> Result<
    safe_cex_engine::request_context::ReconciliationData,
    safe_cex_engine::request_context::Error,
> {
    let data = serde_json::to_string(&delivery.late)
        .map_err(|_| safe_cex_engine::request_context::Error::Invalid)?;
    safe_cex_engine::request_context::ReconciliationData::new(data)
}
pub(crate) fn capture_unproven(
    delivery: &Request,
    response: Response<'_>,
    evidence: &mut UnprovenEvidence,
) {
    if let Ok(envelope) = serde_json::from_str::<crate::Envelope<'_>>(response.body) {
        if let Some(code) = envelope.code.as_ref().and_then(strict_code) {
            evidence.venue_code(&code.to_string());
        }
        evidence.venue_message(envelope.msg.as_ref());
    }
    if matches!(delivery.accepted, Accepted::Place) {
        #[derive(Deserialize)]
        struct Envelope {
            data: Option<Data>,
        }
        #[derive(Deserialize)]
        struct Data {
            id: String,
        }
        if let Ok(Envelope { data: Some(data) }) = serde_json::from_str(response.body) {
            evidence.order_id(&data.id);
        }
    }
}

fn strict_code(code: &crate::Code) -> Option<i64> {
    match code {
        crate::Code::Integer(value) => Some(*value),
        crate::Code::Text(value) => safe_cex_engine::classify::decode_int(value),
        _ => None,
    }
}

/// A failed ApeX Omni delivery: transport, a gateway status or a coded reply.
/// No ApeX code table is documented, so other codes stay unclassified.
const INVALID_CREDENTIAL: i64 = 10002;
pub(crate) fn failure(code: Option<i64>, status: u16, message: String) -> VenueError {
    let error = VenueError::new(message);
    let error = match (code, status) {
        (_, 408 | 500..) | (None, 200..=299) => error.outcome(Outcome::Unknown),
        (Some(401), _) | (_, 401) => error
            .kind(ErrorKind::AuthInvalid)
            .outcome(Outcome::Rejected),
        (Some(403), _) | (_, 403) => error
            .kind(ErrorKind::AuthPermission)
            .outcome(Outcome::Rejected),
        // "Api credential is invalid": the key will not start working on a retry.
        (Some(INVALID_CREDENTIAL), _) => error
            .kind(ErrorKind::AuthInvalid)
            .outcome(Outcome::Rejected),
        (Some(429), _) | (None, 429) => error
            .kind(ErrorKind::RateLimited)
            .outcome(Outcome::Rejected),
        (None, _) => error.kind(ErrorKind::Rejected).outcome(Outcome::Rejected),
        (Some(_), _) => error,
    };
    match code {
        Some(code) => error.venue_code(code.to_string()),
        None => error,
    }
}

/// The write may have executed; reconcile before resending.
pub(crate) fn unknown(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}

/// Refused before any delivery was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_ids_retry_entropy_collisions_before_any_write() {
        let mut issued = HashSet::new();
        let first = mint_id("account", 1.0, &mut issued, || Ok("first".into())).unwrap();
        let mut draws = ["first", "second"].into_iter();
        let second = mint_id("account", 1.0, &mut issued, || {
            Ok(draws.next().unwrap().into())
        })
        .unwrap();
        assert_ne!(first, second);
        assert_eq!(issued.len(), 2);
        assert!(mint_id("account", 1.0, &mut issued, || Ok("first".into())).is_err());
        assert_eq!(issued.len(), 2);
    }
}
