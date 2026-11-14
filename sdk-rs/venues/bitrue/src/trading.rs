//! Trading command jobs: each command owns a Preflight root. Placements,
//! amends and leverage run as one streamed serial Plan, and a cancel batch as
//! one complete Plan of independent calls. Every call signs with the Primary.
//! Replacements require final cancellation proof; a lost write never resends.
use crate::codes;
use crate::wire::{Number, Str, Text, check};
use crate::{Bitrue, Pending};
use safe_cex_engine::classify::{Intent, Native, NativeCode, WriteFact, decode_int};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::plan::{
    CallBound, RetryWindow, SerialCall, SerialPolicy, Slot, StepBound,
};
use safe_cex_engine::request_context::{
    Context, Credential, HttpOwnerRetirement, Operation, Plan, PlanPolicy, Preflight,
    PreflightPolicy, PreflightPurpose, ReconciliationData, RetryPolicy,
};
use safe_cex_engine::signing::Identity as SigningIdentity;
use safe_cex_engine::url::encode;
use safe_cex_engine::{
    Command, CommandId, CommandWait, Cx, ErrorKind, Method, Outcome, Response, VenueError,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Each call's own bound; the streamed Plan has no whole-job deadline.
const CALL_MS: u32 = 20_000;
const CENSUS_UNCERTAIN: &str =
    "Bitrue protective placement outcome is uncertain; complete fresh order census is required";
/// Native's Network/Timeout retries on a read, chained per call (D3).
const READ_RETRIES: u8 = 3;
/// An amend's cancel is re-read this long for a terminal status, then unproven.
const VERIFY_MS: f64 = 3_000.0;
const VERIFY_POLL_MS: u32 = 250;

/// The last check before an amended stop's cancel: a refusal leaves it untouched.
struct StopGuard {
    order: Order,
    trigger: f64,
    reference: StopTriggerType,
    /// The side of the reference the trigger stays on, from planning.
    below: bool,
    /// A closing stop needs a position on the side it closes.
    closes: Option<PositionSide>,
}
impl StopGuard {
    /// A refusal is typed NotSent and leaves the order untouched.
    fn validate(&self, store: &safe_cex_engine::Store, now: f64) -> Result<(), VenueError> {
        use safe_cex_engine::protective_amend::{fresh_reference, refused};
        let unchanged = || format!("Your {} order was not changed", self.order.symbol);
        // Its own cancel marks the row Canceling before the claim.
        let same = store.order(&self.order.id).is_some_and(|order| {
            Order {
                status: self.order.status,
                ..order.clone()
            } == self.order
                && matches!(order.status, OrderStatus::Canceling)
                || order == &self.order
        });
        if !same {
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
        if (self.below && self.trigger >= price) || (!self.below && self.trigger <= price) {
            return Err(refused(Some(format!(
                "{}: the new trigger has already crossed the {label} price",
                unchanged()
            ))));
        }
        Ok(())
    }
}
/// The amended order as its Inspect read showed it.
struct Original {
    amount: f64,
    filled: f64,
    trigger: f64,
}
/// An amended stop's final check and, while nothing has filled, the exact
/// original to put back once.
struct StopProtect {
    guard: StopGuard,
    restore: Option<Vec<(Placement, Order)>>,
}

fn local_mutation_policy() -> safe_cex_engine::request_context::Policy {
    use safe_cex_engine::request_context::{Policy, RetryPolicy};
    Policy {
        operation: Operation::Mutation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

#[derive(Default)]
pub(crate) struct Trading {
    acknowledged: HashMap<String, Target>,
    queue: VecDeque<Queued>,
    active: Option<Active>,
    /// The engine sealed this job; its rows restore at the reconcile.
    retired: Option<Work>,
    unresolved_protection: HashMap<String, u64>,
}
struct Queued {
    preflight: Preflight,
    command: Command,
    deadline: Option<f64>,
}
struct Active {
    preflight: Preflight,
    plan: Option<Plan>,
    work: Work,
}
enum At<'a> {
    Entry,
    Data(&'a Slot, &'a Context),
    /// A delayed call's slot, readied by the Plan timer.
    Ready(Slot),
}
enum Answer {
    Accepted,
    Resend(safe_cex_engine::request_context::ResendGrant),
    Failed(VenueError),
    /// The engine refused the fact: the call is no longer this job's.
    Stopped,
}
/// One released call and the custody it answers under.
pub struct Request {
    command: CommandId,
    slot: Slot,
    context: Context,
    step: Step,
    protection: Option<ProtectionFence>,
    resent: bool,
    /// The released attempt's stamp: a re-sign stamps strictly after it.
    pub(crate) floor: Option<safe_cex_engine::StampFloor>,
    /// A census call's streamed open-order snapshot.
    census: Option<Box<crate::account::Request>>,
}
struct ProtectionFence {
    symbol: String,
    revision: u64,
    previous: Option<u64>,
}
struct Work {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    symbols: Vec<String>,
    creating: Vec<Order>,
    affected: Vec<String>,
    guard: Option<Target>,
    refresh_result: bool,
    protections: Vec<String>,
    census_started: bool,
    /// The amended order's side and kind: what its lost replacement costs.
    amend: Option<(OrderSide, OrderType)>,
    cancel_batch: bool,
    cancel_errors: Vec<CancelFailure>,
    /// A cancel batch's native aggregate, appended to the projected terminal.
    note: Option<String>,
    read_retries: u8,
    /// An amended stop or take profit: its final check and put-back.
    stop: Option<Box<StopProtect>>,
    /// The put-back of a refused stop replacement is in flight.
    restoring: bool,
    /// When this amend's cancel was first read back still live.
    verify_since: Option<f64>,
}
#[derive(Serialize)]
struct CancelFailure {
    id: String,
    error: String,
}
struct Step {
    method: Method,
    path: String,
    body: Option<String>,
    effect: Effect,
    receipts: Vec<OrderIdentityReceipt>,
}
enum Effect {
    Place {
        conditional: bool,
        protective: bool,
        target: Target,
    },
    Cancel(Target),
    Inspect(Box<Amend>),
    Verify(Target),
    Leverage {
        symbol: String,
        leverage: f64,
    },
    /// One protected symbol's fresh open-order snapshot (D4).
    Census(String),
}
#[derive(Clone)]
struct Target {
    id: String,
    contract: String,
    conditional: bool,
    filled: f64,
    amount: f64,
    store_ids: Vec<String>,
}
struct Amend {
    target: Target,
    order: Order,
    price: Option<f64>,
    amount: Option<f64>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Protection {
    client_order_id: String,
    trigger_type: u8,
    trigger_price_type: u8,
    trigger_price: f64,
    #[serde(rename = "type")]
    kind: &'static str,
    price: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Placement {
    contract_name: String,
    client_order_id: String,
    leverage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount: Option<f64>,
    #[serde(rename = "type")]
    kind: &'static str,
    side: &'static str,
    open: &'static str,
    position_type: u8,
    future_model: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    condition_order: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price_type: Option<u8>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    trigger_order_create_params: Vec<Protection>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cancellation<'a> {
    contract_name: &'a str,
    order_id: &'a str,
    condition_order: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Identity<'a> {
    #[serde(borrow)]
    order_id: Text<'a>,
}
#[derive(Deserialize)]
struct Reply<T> {
    data: T,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}
impl<T> OneOrMany<T> {
    fn one(self) -> Result<T, String> {
        match self {
            Self::One(row) => Ok(row),
            Self::Many(mut rows) if rows.len() == 1 => Ok(rows.remove(0)),
            _ => Err("Bitrue response did not identify exactly one order".into()),
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Detail<'a> {
    #[serde(borrow)]
    order_id: Text<'a>,
    #[serde(borrow, default)]
    contract_name: Option<Str<'a>>,
    #[serde(borrow)]
    status: Str<'a>,
    #[serde(default)]
    orig_qty: Option<Number>,
    #[serde(default)]
    executed_qty: Option<Number>,
    #[serde(borrow, default)]
    side: Str<'a>,
    #[serde(borrow, default, rename = "type")]
    kind: Str<'a>,
    #[serde(borrow, default, alias = "open")]
    action: Str<'a>,
    #[serde(default)]
    condition_order: Option<bool>,
    #[serde(default)]
    trigger_price: Option<Number>,
    #[serde(default)]
    trigger_type: Option<Number>,
    #[serde(default)]
    trigger_price_type: Option<Number>,
    #[serde(borrow, default)]
    child_orders: Option<Vec<Detail<'a>>>,
}
fn positive(value: f64, label: &str) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("Bitrue {label} must be positive"))
    }
}
fn terminal(status: OrderStatus) -> bool {
    matches!(status, OrderStatus::Closed | OrderStatus::Canceled)
}
fn native_side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "BUY"
    } else {
        "SELL"
    }
}
fn trigger(value: Option<StopTriggerType>, default: StopTriggerType) -> Result<u8, String> {
    match value.unwrap_or(default) {
        StopTriggerType::LastPrice => Ok(1),
        StopTriggerType::MarkPrice => Ok(2),
        StopTriggerType::IndexPrice => Err("Bitrue index-price triggers are unsupported".into()),
    }
}
fn minted(cx: &Cx<'_, Pending>) -> Result<String, String> {
    Ok(
        safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?
            .chars()
            .take(31)
            .collect(),
    )
}
fn caller_id(value: &str) -> bool {
    (1..=31).contains(&value.chars().count())
        && !value
            .chars()
            .any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
}
impl Target {
    fn lookup(&self, effect: Effect) -> Step {
        Step {
            receipts: Vec::new(),
            method: Method::Get,
            path: format!(
                "/fapi/v2/order?contractName={}&orderId={}&conditionOrder={}",
                encode(&self.contract),
                encode(&self.id),
                self.conditional
            ),
            body: None,
            effect,
        }
    }
    fn cancel(&self) -> Step {
        Step {
            receipts: Vec::new(),
            method: Method::Post,
            path: "/fapi/v2/cancel".into(),
            body: Some(
                serde_json::to_string(&Cancellation {
                    contract_name: &self.contract,
                    order_id: &self.id,
                    condition_order: self.conditional,
                })
                .expect("cancellation"),
            ),
            effect: Effect::Cancel(self.clone()),
        }
    }
    fn quantities(&self, detail: &Detail<'_>) -> Result<(f64, f64), String> {
        if detail.order_id.to_string() != self.id
            || detail
                .contract_name
                .as_ref()
                .is_some_and(|s| s.as_ref() != self.contract)
            || detail
                .condition_order
                .is_some_and(|v| v != self.conditional)
        {
            return Err("Bitrue order detail returned a different order".into());
        }
        let amount = detail
            .orig_qty
            .ok_or("Bitrue order detail has no original quantity")?
            .0;
        let filled = detail
            .executed_qty
            .ok_or("Bitrue order detail has no filled quantity")?
            .0;
        positive(amount, "original quantity")?;
        if !filled.is_finite() || filled < 0.0 || filled > amount {
            return Err("Bitrue order detail has invalid filled quantity".into());
        }
        Ok((amount, filled))
    }
}
impl Work {
    fn new(command: CommandId) -> Self {
        Self {
            command,
            steps: VecDeque::new(),
            ids: vec![],
            symbols: vec![],
            creating: vec![],
            affected: vec![],
            guard: None,
            refresh_result: false,
            protections: vec![],
            census_started: false,
            amend: None,
            cancel_batch: false,
            cancel_errors: vec![],
            note: None,
            read_retries: 0,
            stop: None,
            restoring: false,
            verify_since: None,
        }
    }
    fn symbol(&mut self, symbol: &str) {
        if !self.symbols.iter().any(|s| s == symbol) {
            self.symbols.push(symbol.into());
        }
    }
    fn placements(&mut self, rows: Vec<(Placement, Order)>) {
        for (payload, order) in rows {
            self.refresh_result |= payload.condition_order.unwrap_or(false)
                || !payload.trigger_order_create_params.is_empty();
            self.symbol(&order.symbol);
            let mut receipts = vec![OrderIdentityReceipt {
                symbol: order.symbol.clone(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: payload.client_order_id.clone(),
            }];
            receipts.extend(payload.trigger_order_create_params.iter().map(|child| {
                OrderIdentityReceipt {
                    symbol: order.symbol.clone(),
                    kind: OrderIdentityKind::ClientOrderId,
                    identity: child.client_order_id.clone(),
                }
            }));
            if (payload.condition_order == Some(true)
                || !payload.trigger_order_create_params.is_empty())
                && !self.protections.contains(&order.symbol)
            {
                self.protections.push(order.symbol.clone());
            }
            let target = Target {
                id: String::new(),
                contract: payload.contract_name.clone(),
                conditional: payload.condition_order.unwrap_or(false),
                filled: 0.0,
                amount: order.amount,
                store_ids: vec![],
            };
            self.creating.push(order);
            self.steps.push_back(Step {
                receipts,
                method: Method::Post,
                path: "/fapi/v2/order".into(),
                body: Some(serde_json::to_string(&payload).expect("placement")),
                effect: Effect::Place {
                    conditional: payload.condition_order.unwrap_or(false),
                    protective: payload.condition_order.unwrap_or(false)
                        || !payload.trigger_order_create_params.is_empty(),
                    target,
                },
            });
        }
    }
}
impl Bitrue {
    fn placement(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Vec<(Placement, Order)>, String> {
        if opts.bybit_bbo.is_some() || opts.bybit_slippage_tolerance_ticks.is_some() {
            return Err("Bybit BBO and TickSize controls are not supported by Bitrue".into());
        }
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or("Unknown Bitrue market")?;
        if !market.active {
            return Err("Bitrue market is not active".into());
        }
        let protective = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
        if !matches!(
            opts.kind,
            OrderType::Limit | OrderType::Market | OrderType::StopLoss | OrderType::TakeProfit
        ) {
            return Err("Bitrue conditional and trailing orders are unsupported".into());
        }
        if opts
            .time_in_force
            .is_some_and(|t| t != TimeInForce::GoodTillCancel)
        {
            return Err("Unsupported Bitrue time in force".into());
        }
        if (opts.trigger_price.is_some() && !opts.redundant_protective_trigger())
            || opts.trigger_type.is_some()
        {
            return Err("Unsupported Bitrue generic trigger shape".into());
        }
        if protective && (opts.stop_loss.is_some() || opts.take_profit.is_some()) {
            return Err("Bitrue attached protection requires a regular parent".into());
        }
        positive(opts.amount, "amount")?;
        positive(market.precision.amount, "amount step")?;
        let amount = safe_cex_engine::num::adjust(opts.amount, market.precision.amount);
        positive(amount, "rounded amount")?;
        if amount < market.limits.amount.min {
            return Err("Bitrue amount is below the minimum".into());
        }
        let price = match opts.price {
            Some(value) => {
                positive(value, "price")?;
                let value = safe_cex_engine::num::adjust(value, market.precision.price);
                positive(value, "rounded price")?;
                Some(value)
            }
            None if opts.kind == OrderType::Limit || protective => {
                return Err("Bitrue order requires a price".into());
            }
            None => None,
        };
        let position = cx
            .store
            .positions()
            .iter()
            .find(|p| p.symbol == opts.symbol && p.contracts > 0.0);
        if protective && position.is_none() {
            return Err("Bitrue protection requires an open position".into());
        }
        if protective
            && position.is_some_and(|p| {
                (p.side == PositionSide::Long && opts.side != OrderSide::Sell)
                    || (p.side == PositionSide::Short && opts.side != OrderSide::Buy)
            })
        {
            return Err("Bitrue protection side does not close the position".into());
        }
        let setting = cx.store.setting(&opts.symbol);
        let leverage = position
            .map(|p| p.leverage)
            .or(setting.map(|s| s.leverage).filter(|v| *v > 0.0))
            .ok_or("Bitrue leverage metadata is not loaded")?;
        let margin = position
            .and_then(|p| p.margin_type)
            .or(setting.and_then(|s| s.margin_type))
            .unwrap_or(MarginType::Cross);
        positive(leverage, "leverage metadata")?;
        if leverage.fract() != 0.0 || leverage > 125.0 {
            return Err("Invalid Bitrue leverage metadata".into());
        }
        let maximum = if opts.kind == OrderType::Limit {
            self.contracts
                .get(&opts.symbol)
                .map(|c| c.max_limit_volume)
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(market.limits.amount.max)
        } else {
            market.limits.amount.max
        };
        positive(maximum, "maximum amount")?;
        let step = market.precision.amount;
        // Legacy TS: bitrue.exchange.ts:1180-1196. Keep its equal rounded lots
        // and separate modulo tail, including any under/over-allocation.
        let lots = if amount > maximum {
            (amount / maximum).ceil()
        } else {
            1.0
        };
        let rest = if amount > maximum {
            safe_cex_engine::num::adjust(amount % maximum, step)
        } else {
            0.0
        };
        let lot_size = safe_cex_engine::num::adjust((amount - rest) / lots, step);
        let parts = lots
            .to_string()
            .parse::<usize>()
            .map_err(|_| "Bitrue lot count exceeds addressable storage")?;
        if let Some(id) = &opts.client_order_id
            && (!caller_id(id)
                || protective
                || parts != 1
                || rest != 0.0
                || opts.take_profit.is_some()
                || opts.stop_loss.is_some())
        {
            return Err("Bitrue cannot honor caller clientOrderId on this order".into());
        }
        let protection = |value: f64, kind: u8, reference| -> Result<Protection, String> {
            positive(value, "attached trigger price")?;
            let value = safe_cex_engine::num::adjust(value, market.precision.price);
            positive(value, "rounded attached trigger price")?;
            Ok(Protection {
                client_order_id: minted(cx)?,
                trigger_type: kind,
                trigger_price_type: trigger(
                    reference,
                    if kind == 1 {
                        StopTriggerType::MarkPrice
                    } else {
                        StopTriggerType::LastPrice
                    },
                )?,
                trigger_price: value,
                kind: "MARKET",
                price: 0.0,
            })
        };
        let mut attached = Vec::new();
        if let Some(value) = opts.stop_loss {
            attached.push(protection(value, 1, opts.sl_trigger_type)?);
        }
        if let Some(value) = opts.take_profit {
            attached.push(protection(value, 2, opts.tp_trigger_type)?);
        }
        let reference = if opts.kind == OrderType::StopLoss {
            trigger(opts.sl_trigger_type, StopTriggerType::MarkPrice)?
        } else {
            trigger(opts.tp_trigger_type, StopTriggerType::LastPrice)?
        };
        let mut result = Vec::new();
        for quantity in std::iter::repeat_n(lot_size, parts).chain((rest != 0.0).then_some(rest)) {
            let id = opts
                .client_order_id
                .clone()
                .map(Ok)
                .unwrap_or_else(|| minted(cx))?;
            let reduce =
                protective || opts.reduce_only.unwrap_or(false) || opts.close.unwrap_or(false);
            let mut payload = Placement {
                contract_name: market.id.clone(),
                client_order_id: id.clone(),
                leverage,
                price: if protective {
                    Some(0.0)
                } else if opts.kind == OrderType::Limit {
                    price
                } else {
                    None
                },
                trigger_price: protective.then_some(price.unwrap_or_default()),
                volume: Some(safe_cex_engine::num::format(quantity)),
                amount: None,
                kind: if opts.kind == OrderType::Limit {
                    "LIMIT"
                } else {
                    "MARKET"
                },
                side: native_side(opts.side),
                open: if reduce { "CLOSE" } else { "OPEN" },
                position_type: if margin == MarginType::Cross { 1 } else { 2 },
                future_model: "normal",
                condition_order: protective.then_some(true),
                trigger_type: protective.then_some(if opts.kind == OrderType::StopLoss {
                    1
                } else {
                    2
                }),
                trigger_price_type: protective.then_some(reference),
                trigger_order_create_params: attached.clone(),
            };
            // Each split is an independent parent, including its protection identities.
            for child in &mut payload.trigger_order_create_params {
                child.client_order_id = minted(cx)?;
            }
            if opts.kind == OrderType::Market && !reduce {
                let ticker = cx
                    .store
                    .ticker(&opts.symbol)
                    .ok_or("Bitrue market order has no reference price")?;
                let reference = [ticker.mark, ticker.last, ticker.bid, ticker.ask]
                    .into_iter()
                    .find(|v| v.is_finite() && *v > 0.0)
                    .ok_or("Bitrue market order has no reference price")?;
                let minimum = self
                    .contracts
                    .get(&opts.symbol)
                    .map_or(0.0, |c| c.min_order_money);
                if !minimum.is_finite() || minimum < 0.0 {
                    return Err("Invalid Bitrue minimum order money".into());
                }
                let quote =
                    safe_cex_engine::num::adjust((quantity + step / 3.0) * reference, 0.0001)
                        .max(minimum);
                positive(quote, "market quote amount")?;
                payload.volume = None;
                payload.amount = Some(quote);
            }
            result.push((
                payload,
                Order {
                    id: format!("creating:{id}"),
                    parent_id: None,
                    status: OrderStatus::Creating,
                    symbol: opts.symbol.clone(),
                    kind: opts.kind,
                    side: opts.side,
                    price: price.unwrap_or_default(),
                    amount: quantity,
                    filled: 0.0,
                    remaining: quantity,
                    reduce_only: reduce,
                    close: opts.close,
                    client_order_id: Some(id),
                    time_in_force: Some(TimeInForce::GoodTillCancel),
                    last_update: cx.now,
                    trigger_type: None,
                    take_profit: opts.take_profit,
                    stop_loss: opts.stop_loss,
                    extra: Some(OrderExtra {
                        remaining_native: Some(quantity),
                        filled_in_placement_units: Some(0.0),
                        ..OrderExtra::default()
                    }),
                },
            ));
        }
        Ok(result)
    }
    /// For a stop or take-profit edit, from its Inspect read: the final
    /// pre-cancel check, checked now too, and the untouched original to put back.
    fn stop_protect(
        &self,
        cx: &Cx<'_, Pending>,
        amend: &Amend,
        edited: &PlaceOrderOpts,
        reference: StopTriggerType,
        original: Original,
    ) -> Result<Box<StopProtect>, String> {
        let order = &amend.order;
        let guard = StopGuard {
            order: order.clone(),
            trigger: edited.price.ok_or("Bitrue amendment has no trigger")?,
            reference,
            // A stop sells below (buys above) the market; a take profit the reverse.
            below: (order.kind == OrderType::StopLoss) == (order.side == OrderSide::Sell),
            // Bitrue places every protective order as closing (CLOSE), so the
            // position on its closing side must still be held at the claim.
            closes: Some(if order.side == OrderSide::Sell {
                PositionSide::Long
            } else {
                PositionSide::Short
            }),
        };
        guard.validate(cx.store, cx.now).map_err(|e| e.message)?;
        // Only an untouched original is put back, exactly as the Inspect read
        // showed it: never the edit's size or trigger.
        let restore = if original.filled == 0.0
            && order.parent_id.is_none()
            && order.take_profit.is_none()
            && order.stop_loss.is_none()
            && !cx
                .store
                .orders()
                .iter()
                .any(|leg| leg.parent_id.as_ref() == Some(&order.id))
        {
            let precision = cx
                .store
                .market(&order.symbol)
                .map_or(0.0, |m| m.precision.amount);
            let reference = Some(reference);
            let put_back = PlaceOrderOpts {
                symbol: order.symbol.clone(),
                kind: order.kind,
                side: order.side,
                amount: safe_cex_engine::num::adjust(original.amount, precision),
                price: Some(original.trigger),
                trigger_price: None,
                stop_loss: None,
                take_profit: None,
                reduce_only: Some(order.reduce_only),
                time_in_force: order.time_in_force,
                trigger_type: None,
                tp_trigger_type: reference.filter(|_| order.kind == OrderType::TakeProfit),
                sl_trigger_type: reference.filter(|_| order.kind == OrderType::StopLoss),
                client_order_id: None,
                close: order.close,
                stp_mode: None,
                max_slippage_pct: None,
                bybit_bbo: None,
                bybit_slippage_tolerance_ticks: None,
                implied_volatility: None,
            };
            let rows = self.placement(cx, &put_back)?;
            // Current precision/limits may differ from the original venue order.
            // Never guess a rounded quantity/trigger or a split/grouped put-back.
            let exact = matches!(rows.as_slice(), [(payload, restored)]
                if restored.kind == order.kind
                    && restored.amount == original.amount
                    && payload.volume.as_ref().and_then(|value| value.parse::<f64>().ok()) == Some(original.amount)
                    && payload.trigger_price == Some(original.trigger)
                    && payload.trigger_order_create_params.is_empty());
            exact.then_some(rows)
        } else {
            None
        };
        Ok(Box::new(StopProtect { guard, restore }))
    }
    /// The final-claim re-check of an amended stop's cancel, every send of it.
    pub(crate) fn trade_release_veto(
        &self,
        store: &safe_cex_engine::Store,
        request: &Request,
        release: &safe_cex_engine::request_context::HttpRelease,
    ) -> Result<(), VenueError> {
        let Effect::Cancel(_) = &request.step.effect else {
            return Ok(());
        };
        match self
            .trading
            .active
            .as_ref()
            .filter(|active| active.work.command == request.command && !active.work.cancel_batch)
            .and_then(|active| active.work.stop.as_ref())
        {
            Some(stop) => stop.guard.validate(store, release.time.raw_ms),
            None => Ok(()),
        }
    }
    fn cancel_target(&self, cx: &Cx<'_, Pending>, order: &Order) -> Result<Target, String> {
        if terminal(order.status) || order.status == OrderStatus::Creating {
            return Err("Cannot cancel a terminal or creating Bitrue order".into());
        }
        let market = cx
            .store
            .market(&order.symbol)
            .ok_or("Unknown Bitrue cancellation market")?;
        Ok(Target {
            id: order.id.clone(),
            contract: market.id.clone(),
            conditional: matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit),
            filled: order.filled,
            amount: order.amount,
            store_ids: cx
                .store
                .orders()
                .iter()
                .filter(|o| o.id == order.id || o.parent_id.as_ref() == Some(&order.id))
                .map(|o| o.id.clone())
                .collect(),
        })
    }
    fn trade_plan(&self, cx: &Cx<'_, Pending>, command: Command) -> Result<Work, String> {
        let id = match &command {
            Command::PlaceOrders { command, .. }
            | Command::CancelOrders { command, .. }
            | Command::CancelSymbolOrders { command, .. }
            | Command::AmendOrder { command, .. }
            | Command::SetLeverage { command, .. } => *command,
            _ => return Err("Unsupported Bitrue trading command".into()),
        };
        if self.options.public_data_only || self.options.credentials.is_none() {
            return Err("Bitrue trading requires API credentials".into());
        }
        let mut work = Work::new(id);
        match command {
            Command::PlaceOrders { orders, .. } => {
                if self.trading.acknowledged.len() >= 4096 {
                    return Err(
                        "Bitrue unobserved placement capacity exceeded; reconcile before placing"
                            .into(),
                    );
                }
                let mut seen = HashSet::new();
                let mut rows = Vec::new();
                for opts in orders {
                    if let Some(id) = &opts.client_order_id
                        && !seen.insert(id.clone())
                    {
                        return Err("Duplicate Bitrue caller clientOrderId".into());
                    }
                    rows.extend(self.placement(cx, &opts)?);
                }
                work.placements(rows);
            }
            Command::AmendOrder {
                id, price, amount, ..
            } => {
                let order = cx.store.order(&id).ok_or("Unknown Bitrue order to amend")?;
                if terminal(order.status) {
                    return Err("Cannot amend a terminal Bitrue order".into());
                }
                if order.parent_id.is_some()
                    && matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
                {
                    return Err("Bitrue attached protection was not changed: direct leg edits are unsupported; re-place protection through the parent order".into());
                }
                if price.is_none() && amount.is_none() {
                    return Err("Bitrue amendment requires a price or amount".into());
                }
                if let Some(value) = price {
                    positive(value, "amendment price")?;
                }
                if let Some(value) = amount {
                    positive(value, "amendment amount")?;
                }
                let target = self.cancel_target(cx, order)?;
                let amend = Amend {
                    target: target.clone(),
                    order: order.clone(),
                    price,
                    amount,
                };
                work.amend = Some((order.side, order.kind));
                work.symbol(&order.symbol);
                work.steps
                    .push_back(target.lookup(Effect::Inspect(Box::new(amend))));
            }
            Command::CancelOrders { ids, .. } => {
                work.cancel_batch = true;
                let mut seen = HashSet::new();
                for id in &ids {
                    if !seen.insert(id.clone()) {
                        continue;
                    }
                    let Some(order) = cx.store.order(id) else {
                        if let Some(target) = self.trading.acknowledged.get(id) {
                            if let Some(market) = cx.store.market_by_id(&target.contract) {
                                work.symbol(&market.symbol);
                            }
                            work.steps.push_back(target.cancel());
                            continue;
                        }
                        work.cancel_errors.push(CancelFailure {
                            id: id.clone(),
                            error: "Order not found; cancellation requires a reconciled order"
                                .into(),
                        });
                        continue;
                    };
                    if terminal(order.status) {
                        continue;
                    }
                    let target = match self.cancel_target(cx, order) {
                        Ok(target) => target,
                        Err(error) => {
                            work.cancel_errors.push(CancelFailure {
                                id: id.clone(),
                                error,
                            });
                            continue;
                        }
                    };
                    work.symbol(&order.symbol);
                    work.affected.extend(target.store_ids.clone());
                    work.steps.push_back(target.cancel());
                }
            }
            Command::CancelSymbolOrders { symbol, .. } => {
                if self.trading.unresolved_protection.contains_key(&symbol) {
                    return Err("Bitrue protective placement outcome is uncertain; reconcile before cancel-all".into());
                }
                work.cancel_batch = true;
                let orders = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == symbol && o.status == OrderStatus::Open);
                let mut targets = Vec::new();
                for order in orders {
                    match self.cancel_target(cx, order) {
                        Ok(target) => targets.push(target),
                        Err(error) => work.cancel_errors.push(CancelFailure {
                            id: order.id.clone(),
                            error,
                        }),
                    }
                }
                if let Some(market) = cx.store.market(&symbol) {
                    targets.extend(
                        self.trading
                            .acknowledged
                            .values()
                            .filter(|t| {
                                t.contract == market.id
                                    && !targets.iter().any(|other| other.id == t.id)
                            })
                            .cloned()
                            .collect::<Vec<_>>(),
                    );
                }
                work.symbol(&symbol);
                for target in targets {
                    work.affected.extend(target.store_ids.clone());
                    work.steps.push_back(target.cancel());
                }
            }
            Command::SetLeverage {
                symbol, leverage, ..
            } => {
                positive(leverage, "leverage")?;
                if leverage.fract() != 0.0 {
                    return Err("Bitrue leverage must be an integer".into());
                }
                let market = cx
                    .store
                    .market(&symbol)
                    .ok_or("Unknown Bitrue leverage market")?;
                if leverage > 125.0 {
                    return Err("Bitrue leverage exceeds the venue ceiling of 125".into());
                }
                if let Some(max) = self.account.bracket_limits.get(&symbol)
                    && leverage > *max
                {
                    return Err(format!("Bitrue maximum leverage does not exceed {max}"));
                }
                // The startup market limit is conservative, not authoritative. Submit
                // the exact requested leverage and let the venue accept or reject it.
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Leverage<'a> {
                    contract_name: &'a str,
                    leverage: f64,
                }
                work.symbol(&symbol);
                work.steps.push_back(Step {
                    receipts: Vec::new(),
                    method: Method::Post,
                    path: "/fapi/v2/level_edit".into(),
                    body: Some(
                        serde_json::to_string(&Leverage {
                            contract_name: &market.id,
                            leverage,
                        })
                        .expect("leverage"),
                    ),
                    effect: Effect::Leverage { symbol, leverage },
                });
            }
            _ => unreachable!(),
        }
        Ok(work)
    }
    /// The Plan's single admitted credential: Primary for every trading call,
    /// in-job reads included (D3).
    fn trade_identity(&self) -> Option<(Credential, SigningIdentity)> {
        let slots = self.slots();
        let (selection, identity) = slots.select(CredentialRole::Mutation).ok()?;
        Some((
            selection.credential(identity.credential.clone()),
            identity.clone(),
        ))
    }
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) -> bool {
        self.trading.acknowledged.retain(|id, _| {
            !cx.store
                .orders()
                .iter()
                .any(|o| o.id == *id || o.parent_id.as_ref() == Some(id))
        });
        match &command {
            Command::PlaceOrders { .. }
            | Command::CancelOrders { .. }
            | Command::CancelSymbolOrders { .. }
            | Command::AmendOrder { .. }
            | Command::SetLeverage { .. } => {}
            Command::SetMarginType { .. } => {
                let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                    return true;
                };
                let _ = cx.fail_context(
                    &context,
                    not_sent("Bitrue margin mode cannot be changed through the API")
                        .kind(ErrorKind::Unsupported),
                );
                return true;
            }
            Command::SetPositionMode { hedged, .. } => {
                if *hedged {
                    let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                        return true;
                    };
                    let _ = cx.fail_context(
                        &context,
                        not_sent("Bitrue supports one-way positions only")
                            .kind(ErrorKind::Unsupported),
                    );
                } else {
                    let _ = cx.done_local_setting("[]".into(), |store| store.set_hedged(false));
                }
                return true;
            }
            _ => return false,
        };
        self.enter_trade(cx, command);
        true
    }
    /// Root first: the Preflight exists before validation, so every refusal is
    /// its own NotSent terminal. A command behind a live job parks in FIFO order.
    fn enter_trade(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        // Keyless: no write root exists, so the local refusal authority answers.
        if self.options.public_data_only || self.trade_identity().is_none() {
            let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                not_sent("Bitrue trading requires API credentials"),
            );
            return;
        }
        let credential = self.trade_identity().map(|(credential, _)| credential);
        let policy = PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: credential.clone(),
            write_credential: credential,
        };
        // A parked command keeps its caller's wait; it is checked at its wake.
        let deadline = match cx.command_wait() {
            Some(CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Mutation);
                return;
            }
            Some(CommandWait::Remaining(ms)) => Some(cx.now + f64::from(ms)),
            None => None,
        };
        let Ok(preflight) = cx.begin_preflight(policy) else {
            return;
        };
        if let Err(error) = self.trade_plan(cx, command.clone()) {
            let _ = cx.fail_preflight(&preflight, not_sent(error));
            return;
        }
        if self.trading.active.is_some() || !self.trading.queue.is_empty() {
            if let Err(reason) = cx.park_preflight(&preflight) {
                let _ = cx.fail_preflight(&preflight, not_sent(reason.venue_error().message));
                return;
            }
            self.trading.queue.push_back(Queued {
                preflight,
                command,
                deadline,
            });
            return;
        }
        self.start_job(cx, preflight, command);
    }
    /// Plans against the current Store, then admits and issues the first call
    /// in this one callback.
    fn start_job(&mut self, cx: &mut Cx<'_, Pending>, preflight: Preflight, command: Command) {
        let work = match self.trade_plan(cx, command) {
            Ok(work) => work,
            Err(error) => {
                let _ = cx.fail_preflight(&preflight, not_sent(error));
                return self.next_job(cx);
            }
        };
        self.account.orders_revision += 1;
        for order in &work.creating {
            cx.store.upsert_order(order.clone());
        }
        for id in &work.affected {
            cx.store.set_order_status(id, OrderStatus::Canceling);
        }
        let active = Active {
            preflight,
            plan: None,
            work,
        };
        if active.work.cancel_batch {
            self.start_batch(cx, active);
        } else {
            self.drive(cx, active, At::Entry);
        }
    }

    /// A cancel batch is one complete Plan of independent calls, one per target.
    fn start_batch(&mut self, cx: &mut Cx<'_, Pending>, mut active: Active) {
        // Every target already closed: no admission, nothing manufactured.
        let open = active.work.steps.iter().any(|step| match &step.effect {
            Effect::Cancel(target) => cx
                .store
                .order(&target.id)
                .is_none_or(|o| !terminal(o.status)),
            _ => true,
        });
        if !open {
            for step in std::mem::take(&mut active.work.steps) {
                if let Effect::Cancel(target) = &step.effect
                    && cx
                        .store
                        .order(&target.id)
                        .is_some_and(|o| o.status == OrderStatus::Canceled)
                {
                    active.work.ids.push(target.id.clone());
                }
            }
            if active.work.cancel_errors.is_empty() {
                self.cleanup(cx, &active.work);
                let _ = cx.done_empty_plan(&active.preflight, "[]".into());
                return self.next_job(cx);
            }
            let error = not_sent(cancellation_text(&active.work));
            return self.end(cx, active, None, error);
        }
        let Some((credential, _)) = self.trade_identity() else {
            let error = not_sent("Bitrue trading requires API credentials");
            return self.end(cx, active, None, error);
        };
        const CALL: [CallBound; 1] = [CallBound {
            timeout_ms: CALL_MS,
            attempts: 2,
            retry_window: RetryWindow::FreshAttempt,
            after_refusal: &[],
        }];
        let steps: Vec<_> = (0..active.work.steps.len())
            .map(|_| StepBound {
                wait_ms: 0,
                calls: &CALL,
            })
            .collect();
        match cx.admit_plan(
            &active.preflight,
            &steps,
            PlanPolicy {
                operation: Operation::Mutation,
                credential: Some(credential),
                pacing: None,
                retry: RetryPolicy::DefinitiveRefusal,
                independent_calls: true,
            },
        ) {
            Ok(plan) => {
                active.plan = Some(plan);
                self.batch_next(cx, active);
            }
            Err(reason) => {
                let error = not_sent(reason.venue_error().message);
                self.end(cx, active, None, error)
            }
        }
    }
    /// Issue the batch's next target, retiring each child already closed by
    /// its parent's ACK or the stream (Q1), then settle once all are done.
    fn batch_next(&mut self, cx: &mut Cx<'_, Pending>, mut active: Active) {
        let plan = active.plan.clone().expect("admitted batch");
        while let Some(step) = active.work.steps.pop_front() {
            let Ok(Some(slot)) = cx.plan_slots(&plan).map(|slots| slots.into_iter().next()) else {
                let error = VenueError::new("Bitrue cancellation batch lost its schedule");
                return self.end(cx, active, None, error);
            };
            if let Effect::Cancel(target) = &step.effect
                && let Some(order) = cx.store.order(&target.id)
                && terminal(order.status)
            {
                if order.status == OrderStatus::Canceled {
                    active.work.ids.push(target.id.clone());
                }
                if cx.retire_unissued_plan_call(&plan, &slot).is_err() {
                    let error = VenueError::new("Bitrue cancellation batch lost its schedule");
                    return self.end(cx, active, None, error);
                }
                continue;
            }
            if let Effect::Cancel(target) = &step.effect {
                for id in &target.store_ids {
                    if cx
                        .store
                        .order(id)
                        .is_some_and(|o| o.status == OrderStatus::Open)
                    {
                        cx.store.set_order_status(id, OrderStatus::Canceling);
                    }
                }
            }
            return self.issue_call(cx, active, slot, step, false, None);
        }
        if active.work.cancel_errors.is_empty() {
            return self.finish(cx, active, None);
        }
        active.work.note = Some(cancellation_text(&active.work));
        let error = VenueError::new("Bitrue cancellation failed for one or more orders");
        self.end(cx, active, None, error)
    }

    /// Advances a serial job from `at`: abort on execution progress, admit or
    /// continue the next call, or settle.
    fn drive(&mut self, cx: &mut Cx<'_, Pending>, mut active: Active, at: At<'_>) {
        let current = match &at {
            At::Data(slot, context) => Some((*slot, *context)),
            At::Entry | At::Ready(_) => None,
        };
        // The protective census is the job's own final Read calls (D4).
        if active.work.steps.is_empty() && !active.work.census_started {
            active.work.census_started = true;
            for symbol in active.work.protections.clone() {
                active.work.steps.push_back(census(&symbol));
            }
        }
        let Some(step) = active.work.steps.pop_front() else {
            return self.finish(cx, active, current);
        };
        if matches!(step.effect, Effect::Place { .. } | Effect::Verify(_))
            && active.work.guard.as_ref().is_some_and(|target| {
                self.filled_native(&target.id) > target.filled
                    || cx.store.order(&target.id).is_some_and(|o| {
                        o.status == OrderStatus::Closed || o.filled > target.filled
                    })
            })
        {
            let error = VenueError::new("Bitrue replacement aborted after execution progress");
            return self.end(cx, active, current, error);
        }
        let next = serial(&step);
        let slot = match at {
            At::Entry => {
                let Some((credential, _)) = self.trade_identity() else {
                    let error = not_sent("Bitrue trading requires API credentials");
                    return self.end(cx, active, None, error);
                };
                // Native Bitrue fails a refused job; it never completes `done []`.
                let policy = SerialPolicy {
                    credential: Some(credential),
                    pacing: None,
                    refusals_complete: false,
                };
                match cx.admit_serial_plan(&active.preflight, next, policy) {
                    Ok((plan, slot)) => {
                        active.plan = Some(plan);
                        slot
                    }
                    Err(reason) => {
                        let error = not_sent(reason.venue_error().message);
                        return self.end(cx, active, None, error);
                    }
                }
            }
            At::Data(slot, context) => {
                let plan = active.plan.clone().expect("live Plan");
                match cx.continue_serial_plan(&plan, slot, context, next) {
                    Ok(slot) => slot,
                    Err(reason) => {
                        let error = VenueError::new(reason.venue_error().message);
                        return self.end(cx, active, current, error);
                    }
                }
            }
            At::Ready(slot) => slot,
        };
        self.issue_call(cx, active, slot, step, false, None);
    }
    /// Queues one Plan call. An amend's replacement is fenced before release,
    /// since a lost create ACK may still leave native protection live.
    fn issue_call(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        active: Active,
        slot: Slot,
        step: Step,
        resent: bool,
        floor: Option<safe_cex_engine::StampFloor>,
    ) {
        let plan = active.plan.clone().expect("admitted Plan");
        let narrowed = active.work.amend.is_some() && matches!(step.effect, Effect::Place { .. });
        let context = if narrowed {
            cx.plan_leg_context_with_intent(&plan, &slot, Intent::Place)
        } else {
            cx.plan_leg_context(&plan, &slot)
        };
        let context = match context {
            Ok(context) => context,
            Err(reason) => {
                let error = VenueError::new(reason.venue_error().message);
                return self.end(cx, active, None, error);
            }
        };
        let Some((_, identity)) = self.trade_identity() else {
            let error = not_sent("Bitrue trading requires API credentials");
            return self.end(cx, active, None, error);
        };
        if !resent {
            cx.command_progress(active.work.command, step.receipts.clone());
        }
        // A 408 or 5xx on a put-back is sealed before this venue sees it:
        // say what is unknown rather than repeat the replacement's refusal.
        if active.work.restoring
            && step.is_place()
            && let Some(stop) = &active.work.stop
        {
            let order = &stop.guard.order;
            let words = format!(
                "Your {} {}'s replacement was refused and its put-back's outcome is unknown. Check Open Orders",
                order.symbol,
                what(order.kind)
            );
            let _ = cx.present_plan_failure(&plan, words);
        }
        let protection = match &step.effect {
            Effect::Place {
                protective: true,
                target,
                ..
            } if !resent => {
                let symbol = cx
                    .store
                    .market_by_id(&target.contract)
                    .map_or_else(|| target.contract.clone(), |market| market.symbol.clone());
                self.account.orders_revision += 1;
                let revision = self.account.orders_revision;
                let previous = self
                    .trading
                    .unresolved_protection
                    .insert(symbol.clone(), revision);
                Some(ProtectionFence {
                    symbol,
                    revision,
                    previous,
                })
            }
            _ => None,
        };
        let census = match &step.effect {
            Effect::Census(symbol) => Some(Box::new(crate::account::Request::Orders {
                symbol: symbol.clone(),
                revision: self.account.orders_revision,
                lookup: None,
                rows: Default::default(),
                quotes: HashMap::new(),
                invalid: false,
                primary: true,
            })),
            _ => None,
        };
        let url = format!("{}{}", crate::REST, step.path);
        let recipe = crate::sign::dispatch::Http::new(
            identity,
            step.method,
            url.clone(),
            step.body.clone(),
            self.options.recv_window,
            floor,
        );
        let (method, body) = (step.method, step.body.clone());
        let stream = census.is_some();
        let pending = Pending::trade(
            method,
            step.path.clone(),
            body.clone(),
            Request {
                command: active.work.command,
                slot,
                context: context.clone(),
                step,
                protection,
                resent,
                floor: None,
                census,
            },
        );
        self.trading.active = Some(active);
        match cx.request_signed_managed(context, None, method, url, body, Box::new(recipe), pending)
        {
            Ok(id) => {
                if stream {
                    cx.stream_response(id, ArrayAt::Field("data"));
                }
            }
            Err((pending, reason)) => {
                if let Some(request) = pending.into_trade() {
                    self.trade_call_failed(cx, request, reason.venue_error(), false);
                }
            }
        }
    }
    /// The amend's cancel read back: CANCELED with the planned quantities
    /// proves it; FILLED means the stop fired; anything else is re-read until
    /// `VERIFY_MS`, then the cancel is unproven.
    fn verify_answer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut active: Active,
        request: Request,
        target: &Target,
        response: &Response<'_>,
    ) {
        let current = Some((&request.slot, &request.context));
        let read = check(response).and_then(|()| {
            let Reply { data } =
                serde_json::from_str::<Reply<OneOrMany<Detail<'_>>>>(response.body)
                    .map_err(|e| format!("Malformed Bitrue order detail: {e}"))?;
            let detail = data.one()?;
            let quantities = target.quantities(&detail)?;
            Ok((detail.status.to_string(), quantities))
        });
        let Ok((status, (amount, filled))) = read else {
            return self.cancel_unproven(cx, active, current);
        };
        let guarded = active.work.stop.is_some();
        match status.as_str() {
            "CANCELED" | "CANCELLED" if amount == target.amount && filled == target.filled => {
                self.drive(cx, active, At::Data(&request.slot, &request.context))
            }
            "FILLED" if guarded => {
                let order = &active.work.stop.as_ref().expect("guarded").guard.order;
                let mut error = VenueError::new(format!(
                    "Your {} {} was not changed: it filled before its cancel landed",
                    order.symbol,
                    what(order.kind)
                ));
                error.presents = true;
                self.end(cx, active, current, error)
            }
            "CANCELED" | "CANCELLED" | "FILLED" if guarded => {
                // It partly filled during its cancel: nothing replaces it.
                let order = &active.work.stop.as_ref().expect("guarded").guard.order;
                let mut error = VenueError::new(format!(
                    "Your {} {} was removed: it partly filled during its cancel, please re-place it",
                    order.symbol,
                    what(order.kind)
                ));
                error.protection = Some(safe_cex_engine::error::Protection::Unprotected);
                error.presents = true;
                self.end(cx, active, current, error)
            }
            "CANCELED" | "CANCELLED" | "FILLED" => {
                let error = VenueError::new(
                    "Bitrue replacement final cancellation or filled quantity was not confirmed",
                );
                self.end(cx, active, current, error)
            }
            _ => {
                let since = *active.work.verify_since.get_or_insert(cx.now);
                if cx.now - since >= VERIFY_MS {
                    return self.cancel_unproven(cx, active, current);
                }
                let plan = active.plan.clone().expect("live Plan");
                let step = target.lookup(Effect::Verify(target.clone()));
                let call = serial(&step);
                active.work.steps.push_front(step);
                match cx.continue_serial_plan_after(
                    &plan,
                    &request.slot,
                    &request.context,
                    VERIFY_POLL_MS,
                    call,
                ) {
                    Ok(()) => self.trading.active = Some(active),
                    Err(reason) => {
                        let error = VenueError::new(reason.venue_error().message);
                        self.end(cx, active, current, error)
                    }
                }
            }
        }
    }
    /// The amend's cancel was taken but never proven: nothing replaces it,
    /// and only here is its standing unknown.
    fn cancel_unproven(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        active: Active,
        current: Option<(&Slot, &Context)>,
    ) {
        if let Some(plan) = &active.plan {
            let _ = cx.inherit_plan_uncertainty(plan);
        }
        let symbol = active.work.symbols.first().cloned().unwrap_or_default();
        let what = active.work.amend.map_or("order", |(_, kind)| what(kind));
        let mut error = VenueError::new(format!(
            "Your {symbol} {what} was not replaced: its cancel was sent but could not be confirmed, so whether it still stands is unknown. Check Open Orders"
        ));
        error.presents = true;
        self.end(cx, active, current, error)
    }
    /// An amended stop's replacement or put-back failed. A definite refusal of
    /// the replacement puts the untouched original back once.
    fn guarded_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut active: Active,
        request: Request,
        mut error: VenueError,
        data: bool,
    ) {
        // Documented codes and 401/403 are definite; 408, 5xx, 429 and
        // transport stay unknown for the put-back and protection decisions.
        let definite =
            error.outcome == Some(Outcome::Rejected) && error.kind != Some(ErrorKind::RateLimited);
        let unsent = error.outcome == Some(Outcome::NotSent);
        let restoring = active.work.restoring;
        // The put-back's rows are taken once: it is never sent twice.
        if definite
            && data
            && let Some(rows) = active
                .work
                .stop
                .as_mut()
                .and_then(|stop| stop.restore.take())
        {
            let plan = active.plan.clone().expect("live Plan");
            let orders: Vec<Order> = rows.iter().map(|(_, order)| order.clone()).collect();
            active.work.steps.clear();
            active.work.placements(rows);
            let step = active.work.steps.pop_front().expect("a put-back");
            if let Ok(slot) =
                cx.continue_serial_plan(&plan, &request.slot, &request.context, serial(&step))
            {
                active.work.restoring = true;
                for order in orders {
                    cx.store.upsert_order(order);
                }
                return self.issue_call(cx, active, slot, step, false, None);
            }
        }
        let current = data.then_some((&request.slot, &request.context));
        let order = &active.work.stop.as_ref().expect("guarded").guard.order;
        let (symbol, what) = (order.symbol.clone(), what(order.kind));
        let (message, protection) = match (restoring, definite, unsent) {
            (true, true, _) => (
                format!(
                    "Your {symbol} {what} was removed: its replacement and the put-back were both refused, please re-place it"
                ),
                Some(safe_cex_engine::error::Protection::Unprotected),
            ),
            (true, _, true) => (
                format!(
                    "Your {symbol} {what} was removed: its replacement was refused and the put-back could not be sent, please re-place it"
                ),
                Some(safe_cex_engine::error::Protection::Unprotected),
            ),
            (true, false, false) => (
                format!(
                    "Your {symbol} {what}'s replacement was refused and its put-back's outcome is unknown. Check Open Orders"
                ),
                None,
            ),
            (false, true, _) | (false, _, true) => (
                format!(
                    "Your {symbol} {what} was removed: its replacement was refused, please re-place it"
                ),
                Some(safe_cex_engine::error::Protection::Unprotected),
            ),
            (false, false, false) => {
                Self::note_replacement_lost(&mut active.work, &request.step.effect, true);
                return self.end(cx, active, current, error);
            }
        };
        error.message = format!("{message} ({})", error.message);
        error.protection = protection;
        error.presents = true;
        self.end(cx, active, current, error)
    }
    /// The Plan timer readied a delayed call: issue it.
    pub(crate) fn trading_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        let Some(active) = self.trading.active.take() else {
            return;
        };
        let slot = active
            .plan
            .as_ref()
            .filter(|own| own.owner() == plan.owner())
            .and_then(|_| cx.plan_slots(plan).ok())
            .and_then(|slots| slots.into_iter().next());
        match slot {
            Some(slot) => self.drive(cx, active, At::Ready(slot)),
            None => self.trading.active = Some(active),
        }
    }
    /// Restores the job's optimistic rows: Creating removed, Canceling reopened.
    fn cleanup(&mut self, cx: &mut Cx<'_, Pending>, work: &Work) {
        let creating: Vec<_> = work.creating.iter().map(|o| o.id.as_str()).collect();
        cx.store.remove_orders(&creating);
        for id in &work.affected {
            if cx
                .store
                .order(id)
                .is_some_and(|o| o.status == OrderStatus::Canceling)
            {
                cx.store.set_order_status(id, OrderStatus::Open);
            }
        }
    }
    /// Fails the job under its owner (the journal projects the outcome), then
    /// wakes the next queued command.
    /// An amend's replacement failed after its cancel landed: the order no longer rests.
    fn note_replacement_lost(work: &mut Work, effect: &Effect, unknown: bool) {
        // The accepted cancel's own ID is already in `ids`.
        let placed = work
            .ids
            .iter()
            .any(|id| work.guard.as_ref().is_none_or(|guard| guard.id != *id));
        if let Some((side, kind)) = work.amend
            && !placed
            && matches!(effect, Effect::Place { .. })
            && let Some(symbol) = work.symbols.first()
        {
            work.note = Some(
                if matches!(kind, OrderType::StopLoss | OrderType::TakeProfit) {
                    safe_cex_engine::error::stop_gone_text(symbol, side, kind, unknown)
                } else {
                    safe_cex_engine::error::order_gone_text(symbol, unknown)
                },
            );
        }
    }
    fn end(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        active: Active,
        current: Option<(&Slot, &Context)>,
        error: VenueError,
    ) {
        self.cleanup(cx, &active.work);
        match &active.plan {
            None => {
                let failed = cx.fail_preflight(&active.preflight, error);
                debug_assert!(failed.is_ok(), "Preflight terminal stranded");
            }
            Some(plan) => {
                if let Some((slot, context)) = current {
                    let finished = cx.finish_plan_call(plan, slot, context);
                    debug_assert!(finished.is_ok(), "Plan call seal stranded");
                }
                // The journal selects the leg's error; native's aggregate follows.
                let failed = match &active.work.note {
                    Some(note) => cx.fail_plan_noted(plan, error, note),
                    None => cx.fail_plan(plan, error),
                };
                debug_assert!(failed.is_ok(), "Plan terminal stranded");
            }
        }
        self.refresh_after(cx, &active.work);
        self.next_job(cx);
    }
    fn finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        active: Active,
        current: Option<(&Slot, &Context)>,
    ) {
        self.cleanup(cx, &active.work);
        let plan = active.plan.as_ref().expect("a job with calls has a Plan");
        if let Some((slot, context)) = current {
            let finished = cx.finish_plan_call(plan, slot, context);
            debug_assert!(finished.is_ok(), "Plan call seal stranded");
        }
        if active.work.restoring
            && let Some(stop) = &active.work.stop
        {
            let order = &stop.guard.order;
            let mut error = VenueError::new(format!(
                "Your {} {} is unchanged: its replacement was refused, so the original was put back as a new order",
                order.symbol,
                what(order.kind)
            ));
            error.protection = Some(safe_cex_engine::error::Protection::Restored);
            error.presents = true;
            let failed = cx.fail_plan(plan, error.partial_ids(&active.work.ids));
            debug_assert!(failed.is_ok(), "Plan terminal stranded");
            self.refresh_after(cx, &active.work);
            return self.next_job(cx);
        }
        let result = if active.work.refresh_result || active.work.cancel_batch {
            "[]".to_owned()
        } else {
            serde_json::to_string(&active.work.ids).expect("order ids")
        };
        if cx.done_plan(plan, result).is_err() {
            let failed = cx.fail_plan(plan, VenueError::new("Bitrue job ended without its call"));
            debug_assert!(failed.is_ok(), "Plan terminal stranded");
        }
        self.refresh_after(cx, &active.work);
        self.next_job(cx);
    }
    /// Native refreshes the touched symbols once a job settles.
    fn refresh_after(&mut self, cx: &mut Cx<'_, Pending>, work: &Work) {
        if !work.symbols.is_empty() {
            self.refresh_orders(cx, work.symbols.clone());
            self.refresh_account(cx);
        }
    }
    /// The active job ended: wake the oldest queued Preflight (FIFO).
    fn next_job(&mut self, cx: &mut Cx<'_, Pending>) {
        while self.trading.active.is_none()
            && let Some(queued) = self.trading.queue.front()
        {
            let Err(reason) = cx.request_preflight_ready(&queued.preflight) else {
                return;
            };
            // A front that cannot wake must not stall the FIFO.
            let queued = self.trading.queue.pop_front().expect("queued front");
            let _ = cx.fail_preflight(&queued.preflight, not_sent(reason.venue_error().message));
        }
    }
    pub(crate) fn trading_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &Preflight,
    ) {
        let front = self.trading.active.is_none()
            && self
                .trading
                .queue
                .front()
                .is_some_and(|queued| queued.preflight.owner() == preflight.owner());
        if !front {
            // Woken out of turn: park again, still in FIFO order.
            let _ = cx.park_preflight(preflight);
            return;
        }
        let queued = self.trading.queue.pop_front().expect("queued front");
        if queued.deadline.is_some_and(|deadline| cx.now >= deadline) {
            let error = safe_cex_engine::request_context::Error::TimedOut
                .venue_error()
                .outcome(Outcome::NotSent);
            let _ = cx.fail_preflight(&queued.preflight, error);
            return self.next_job(cx);
        }
        self.start_job(cx, queued.preflight, queued.command);
    }
    /// The engine retired a trading owner itself: forget it, keep an active
    /// job for restoration, and name the queued Preflight to wake.
    pub(crate) fn trading_retired(&mut self, owner: &HttpOwnerRetirement) -> Option<Preflight> {
        self.trading
            .queue
            .retain(|queued| !owner.matches_preflight(&queued.preflight));
        if self
            .trading
            .active
            .as_ref()
            .is_some_and(|active| owner.matches_preflight(&active.preflight))
        {
            let active = self.trading.active.take().expect("retired job");
            self.trading.retired = Some(active.work);
        }
        if self.trading.active.is_some() {
            return None;
        }
        self.trading
            .queue
            .front()
            .map(|queued| queued.preflight.clone())
    }
    /// Native restore for a job the engine sealed, then a Primary reconcile.
    pub(crate) fn reconcile_trading(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbols: Option<&[String]>,
    ) {
        if let Some(work) = self.trading.retired.take() {
            self.cleanup(cx, &work);
            self.account.orders_revision = self.account.orders_revision.wrapping_add(1);
        }
        self.reconcile_orders(cx, symbols);
    }
    /// A clean census of `revision` or later clears that symbol's fence.
    pub(crate) fn protection_observed(&mut self, symbol: &str, revision: u64) {
        if self
            .trading
            .unresolved_protection
            .get(symbol)
            .is_some_and(|minimum| revision >= *minimum)
        {
            self.trading.unresolved_protection.remove(symbol);
        }
    }
    pub(crate) fn trade_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        if let Some(census) = request.census.as_mut() {
            self.account_item(cx, census, item);
        }
    }
    /// A call failed before or after release. A released Read keeps native's
    /// three retries (D3); a released write is Unknown.
    pub(crate) fn trade_call_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        mut error: VenueError,
        released: bool,
    ) {
        let Some(mut active) = self.trading.active.take() else {
            return;
        };
        let Some(plan) = active
            .plan
            .clone()
            .filter(|_| active.work.command == request.command)
        else {
            self.trading.active = Some(active);
            return;
        };
        if request.step.is_place() {
            self.account.orders_revision = self.account.orders_revision.wrapping_add(1);
        }
        if request.step.read() {
            if released
                && matches!(error.kind, Some(ErrorKind::Network | ErrorKind::Timeout))
                && active.work.read_retries < READ_RETRIES
                && let Ok(slot) = cx.continue_serial_plan(
                    &plan,
                    &request.slot,
                    &request.context,
                    serial(&request.step),
                )
            {
                active.work.read_retries += 1;
                request.census = None;
                return self.issue_call(cx, active, slot, request.step, false, request.floor);
            }
            if let Effect::Verify(_) = &request.step.effect {
                return self.cancel_unproven(cx, active, None);
            }
            // Transport text stays behind native's private diagnostic boundary.
            if let Effect::Census(symbol) = &request.step.effect {
                cx.command_error(
                    active.work.command,
                    format!("Bitrue order read for {symbol} failed"),
                );
                error = VenueError::new(CENSUS_UNCERTAIN);
                active.work.note = Some(CENSUS_UNCERTAIN.into());
                // D4: the accepted placement's protection may or may not be live.
                let _ = cx.inherit_plan_uncertainty(&plan);
            }
            return self.end(cx, active, None, error);
        }
        if released {
            error.outcome = Some(Outcome::Unknown);
        }
        let _ = cx.finish_plan_failed_call(&plan, &request.slot, &request.context);
        if request.step.is_place() && active.work.stop.is_some() {
            return self.guarded_failed(cx, active, request, error, false);
        }
        Self::note_replacement_lost(
            &mut active.work,
            &request.step.effect,
            error.outcome == Some(Outcome::Unknown),
        );
        if active.work.cancel_batch
            && let Effect::Cancel(target) = &request.step.effect
        {
            active.work.cancel_errors.push(CancelFailure {
                id: target.id.clone(),
                error: error.message,
            });
            for id in &target.store_ids {
                if cx
                    .store
                    .order(id)
                    .is_some_and(|o| o.status == OrderStatus::Canceling)
                {
                    cx.store.set_order_status(id, OrderStatus::Open);
                }
            }
            return self.batch_next(cx, active);
        }
        self.end(cx, active, None, error);
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        let Some(mut active) = self.trading.active.take() else {
            return;
        };
        let delivered = active.work.command == request.command
            && active
                .plan
                .as_ref()
                .is_some_and(|plan| cx.validate_plan_delivery(plan, &request.context).is_ok());
        if !delivered {
            self.trading.active = Some(active);
            return;
        }
        let plan = active.plan.clone().expect("delivered Plan");
        let current = Some((&request.slot, &request.context));
        if request.step.read() {
            // Native's one signature re-sign covers in-job reads too.
            if !request.resent
                && codes::resend(&response)
                && let Ok(slot) = cx.continue_serial_plan(
                    &plan,
                    &request.slot,
                    &request.context,
                    serial(&request.step),
                )
            {
                request.census = None;
                return self.issue_call(cx, active, slot, request.step, true, request.floor);
            }
            active.work.read_retries = 0;
            if let Effect::Verify(target) = &request.step.effect {
                let target = target.clone();
                return self.verify_answer(cx, active, request, &target, &response);
            }
            let result = match request.census.take() {
                Some(census) => self.census_answer(cx, *census, &response),
                None => check(&response).and_then(|()| {
                    self.trade_detail(cx, &mut active.work, &request.step, response.body)
                }),
            };
            return match result {
                Ok(()) => self.drive(cx, active, At::Data(&request.slot, &request.context)),
                Err(error) => {
                    // A stream failure's owned read text may lead; the census
                    // uncertainty always follows it.
                    if request.step.is_census() {
                        active.work.note = Some(CENSUS_UNCERTAIN.into());
                        // D4: the accepted placement's protection may or may not be live.
                        let _ = cx.inherit_plan_uncertainty(&plan);
                    }
                    self.end(cx, active, current, VenueError::new(error))
                }
            };
        }
        match self.write_answer(cx, &mut active.work, &request, &response) {
            Answer::Accepted => {
                if active.work.cancel_batch {
                    let _ = cx.finish_plan_call(&plan, &request.slot, &request.context);
                    self.batch_next(cx, active)
                } else {
                    self.drive(cx, active, At::Data(&request.slot, &request.context))
                }
            }
            Answer::Resend(grant) => {
                match cx.plan_resend_context(&plan, &request.slot, &request.context, grant) {
                    Ok(context) => {
                        request.context = context;
                        let slot = request.slot.clone();
                        self.issue_resend(cx, active, slot, request)
                    }
                    Err(reason) => {
                        let error = VenueError::new(reason.venue_error().message);
                        self.end(cx, active, None, error)
                    }
                }
            }
            Answer::Failed(error) => {
                if active.work.cancel_batch
                    && let Effect::Cancel(target) = &request.step.effect
                {
                    active.work.cancel_errors.push(CancelFailure {
                        id: target.id.clone(),
                        error: error.message,
                    });
                    for id in &target.store_ids {
                        if cx
                            .store
                            .order(id)
                            .is_some_and(|o| o.status == OrderStatus::Canceling)
                        {
                            cx.store.set_order_status(id, OrderStatus::Open);
                        }
                    }
                    let _ = cx.finish_plan_call(&plan, &request.slot, &request.context);
                    return self.batch_next(cx, active);
                }
                if request.step.is_place() && active.work.stop.is_some() {
                    return self.guarded_failed(cx, active, request, error, true);
                }
                Self::note_replacement_lost(
                    &mut active.work,
                    &request.step.effect,
                    error.outcome == Some(Outcome::Unknown),
                );
                self.end(cx, active, current, error)
            }
            Answer::Stopped => self.trading.active = Some(active),
        }
    }
    /// The one classified resend: the same call bytes under the resend Context.
    fn issue_resend(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        active: Active,
        slot: Slot,
        request: Request,
    ) {
        let Some((_, identity)) = self.trade_identity() else {
            let error = not_sent("Bitrue trading requires API credentials");
            return self.end(cx, active, None, error);
        };
        let step = request.step;
        let url = format!("{}{}", crate::REST, step.path);
        let recipe = crate::sign::dispatch::Http::new(
            identity,
            step.method,
            url.clone(),
            step.body.clone(),
            self.options.recv_window,
            request.floor,
        );
        let (method, body, context) = (step.method, step.body.clone(), request.context.clone());
        let pending = Pending::trade(
            method,
            step.path.clone(),
            body.clone(),
            Request {
                command: request.command,
                slot,
                context: context.clone(),
                step,
                protection: request.protection,
                resent: true,
                floor: None,
                census: None,
            },
        );
        self.trading.active = Some(active);
        if let Err((pending, reason)) =
            cx.request_signed_managed(context, None, method, url, body, Box::new(recipe), pending)
            && let Some(request) = pending.into_trade()
        {
            self.trade_call_failed(cx, request, reason.venue_error(), false);
        }
    }
    /// One census page: a clean, current snapshot certifies the symbol.
    fn census_answer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        census: crate::account::Request,
        response: &Response<'_>,
    ) -> Result<(), String> {
        let crate::account::Request::Orders {
            symbol,
            revision,
            rows,
            quotes,
            invalid,
            ..
        } = census
        else {
            return Err(CENSUS_UNCERTAIN.into());
        };
        #[derive(Deserialize)]
        struct OrdersEnvelope {
            data: Vec<serde::de::IgnoredAny>,
        }
        let schema = (200..300).contains(&response.status)
            && check(response).is_ok()
            && serde_json::from_str::<OrdersEnvelope>(response.body)
                .is_ok_and(|v| v.data.is_empty());
        let (_, clean) = self.apply_orders(cx, &symbol, revision, rows, quotes, schema && !invalid);
        if clean {
            Ok(())
        } else {
            Err(CENSUS_UNCERTAIN.into())
        }
    }
    /// Decodes one write answer, records its journal fact exactly once and
    /// applies native Store effects.
    fn write_answer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        work: &mut Work,
        request: &Request,
        response: &Response<'_>,
    ) -> Answer {
        let context = &request.context;
        if let Err(message) = check(response) {
            // Only documented, unambiguous pre-execution rejections undo this
            // write's fence. Unknown/server outcomes require a fresh census.
            #[derive(Deserialize)]
            struct Rejected<'a> {
                #[serde(borrow)]
                code: Text<'a>,
                #[serde(borrow)]
                data: Option<&'a serde_json::value::RawValue>,
            }
            let parsed = serde_json::from_str::<Rejected<'_>>(response.body).ok();
            let guarded =
                work.stop.is_some() && matches!(request.step.effect, Effect::Place { .. });
            let no_order_data = parsed
                .as_ref()
                .is_some_and(|reply| reply.data.is_none_or(|data| data.get() == "null"));
            let mut rejected = (200..500).contains(&response.status)
                && response.status != 408
                && parsed.as_ref().is_some_and(|reply| {
                    // Futures error list: signature, mandatory parameter, contract, balance.
                    matches!(&*reply.code, "-1022" | "-1102" | "-1121" | "-2017")
                        && reply.data.is_none_or(|data| data.get() == "null")
                });
            if let Effect::Leverage { symbol, .. } = &request.step.effect {
                self.learn_leverage_limit(cx, symbol, response.body);
            }
            let code = parsed
                .as_ref()
                .and_then(|reply| decode_int(&reply.code))
                .filter(|_| {
                    if guarded {
                        // A rate status cannot become a refusal through its body.
                        !matches!(response.status, 418 | 429)
                    } else {
                        rejected
                    }
                });
            let native = code.map(|code| Native {
                code: NativeCode::Int(code),
                message: "",
            });
            let mut error = answer_failure(response, message, rejected);
            let table = if guarded {
                &codes::GUARDED_REPLACEMENT
            } else {
                &codes::BITRUE
            };
            let answer =
                match cx.classify_write(context, table, native.as_ref(), error.message.clone()) {
                    Ok(fact) => {
                        let fact = if guarded && !no_order_data {
                            fact.unproven()
                        } else {
                            fact
                        };
                        if guarded {
                            rejected = matches!(
                                fact.verdict(),
                                safe_cex_engine::classify::Verdict::Refused(_)
                                    | safe_cex_engine::classify::Verdict::RefusedResend(_)
                            );
                            error = fact.error().clone();
                        }
                        match cx.http_write_fact(
                            context,
                            WriteFact::Failed {
                                fact,
                                known_effects: false,
                            },
                            None,
                        ) {
                            Ok(Some(grant)) if !request.resent => Answer::Resend(grant),
                            Ok(_) => Answer::Failed(error),
                            Err(_) => Answer::Stopped,
                        }
                    }
                    Err(_) => match cx.http_write_unproven(context, error.message.clone()) {
                        Ok(_) => Answer::Failed(error),
                        Err(_) => Answer::Stopped,
                    },
                };
            // A resent placement keeps its fence until its own answer: a lost
            // resend may still leave native protection live.
            if rejected
                && matches!(answer, Answer::Failed(_))
                && let Some(fence) = &request.protection
                && self.trading.unresolved_protection.get(&fence.symbol) == Some(&fence.revision)
            {
                if let Some(previous) = fence.previous {
                    self.trading
                        .unresolved_protection
                        .insert(fence.symbol.clone(), previous);
                } else {
                    self.trading.unresolved_protection.remove(&fence.symbol);
                }
            }
            return answer;
        }
        let accepted = match &request.step.effect {
            Effect::Place {
                conditional,
                protective,
                target,
            } => match serde_json::from_str::<Reply<Identity<'_>>>(response.body) {
                Ok(reply) if !reply.data.order_id.is_empty() => {
                    let id = reply.data.order_id.to_string();
                    if !protective && cx.store.order(&id).is_none() {
                        let mut target = target.clone();
                        target.id = id.clone();
                        self.trading.acknowledged.insert(id.clone(), target);
                    }
                    if !conditional {
                        work.ids.push(id.clone());
                    }
                    Ok(vec![id])
                }
                Ok(_) => Err("Bitrue placement returned no order identity".to_owned()),
                Err(error) => Err(format!("Malformed Bitrue placement: {error}")),
            },
            Effect::Cancel(target) => {
                let reply = serde_json::from_str::<Reply<Identity<'_>>>(response.body);
                if reply.is_ok_and(|r| r.data.order_id.to_string() == target.id) {
                    self.trading.acknowledged.remove(&target.id);
                    self.account.orders_revision += 1;
                    for id in &target.store_ids {
                        self.remember_terminal(id);
                        if !cx
                            .store
                            .order(id)
                            .is_some_and(|o| o.status == OrderStatus::Closed)
                        {
                            cx.store.set_order_status(id, OrderStatus::Canceled);
                        }
                    }
                    self.remember_terminal(&target.id);
                    if work.cancel_batch {
                        work.ids.push(target.id.clone());
                    }
                    Ok(vec![target.id.clone()])
                } else {
                    Err("Bitrue cancellation did not confirm the requested order identity".into())
                }
            }
            Effect::Leverage { symbol, leverage } => {
                self.account.settings_revision += 1;
                cx.store.update_setting(symbol, |s| {
                    s.leverage = *leverage;
                    s.buy_leverage = Some(*leverage);
                    s.sell_leverage = Some(*leverage);
                    s.margin_type.get_or_insert(MarginType::Cross);
                });
                cx.store.update_positions(|p| {
                    if p.symbol == *symbol {
                        p.leverage = *leverage;
                    }
                });
                Ok(vec![])
            }
            Effect::Inspect(_) | Effect::Verify(_) | Effect::Census(_) => unreachable!(),
        };
        match accepted {
            Ok(order_ids) => {
                match cx.http_write_fact(context, WriteFact::Accepted { order_ids }, None) {
                    Ok(_) => Answer::Accepted,
                    Err(_) => Answer::Stopped,
                }
            }
            // A 2xx that does not prove the write is Unknown.
            Err(message) => match cx.http_write_unproven(context, message.clone()) {
                Ok(_) => Answer::Failed(VenueError::new(message).outcome(Outcome::Unknown)),
                Err(_) => Answer::Stopped,
            },
        }
    }
    /// A refused leverage write names the venue's ceiling; learn it.
    fn learn_leverage_limit(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, body: &str) {
        let Ok(reply) = serde_json::from_str::<crate::wire::Envelope<'_>>(body) else {
            return;
        };
        let message = reply.msg.to_ascii_lowercase();
        if let Some((_, suffix)) = message.split_once("maximum leverage does not exceed") {
            let token: String = suffix
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(max) = token.parse::<f64>()
                && max > 0.0
                && max <= 125.0
                && max.fract() == 0.0
            {
                self.account.bracket_limits.insert(symbol.to_owned(), max);
                cx.store.update_markets(|market| {
                    if market.symbol == symbol {
                        market.limits.leverage.max = max;
                    }
                });
            }
        }
    }
    fn trade_detail(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        work: &mut Work,
        step: &Step,
        body: &str,
    ) -> Result<(), String> {
        let Reply { data } = serde_json::from_str::<Reply<OneOrMany<Detail<'_>>>>(body)
            .map_err(|e| format!("Malformed Bitrue order detail: {e}"))?;
        let detail = data.one()?;
        match &step.effect {
            Effect::Verify(target) => {
                let (amount, filled) = target.quantities(&detail)?;
                if !matches!(detail.status.as_ref(), "CANCELED" | "CANCELLED")
                    || amount != target.amount
                    || filled != target.filled
                {
                    return Err("Bitrue replacement final cancellation or filled quantity was not confirmed".into());
                }
            }
            Effect::Inspect(amend) => {
                let (amount, filled) = amend.target.quantities(&detail)?;
                if !matches!(
                    detail.status.as_ref(),
                    "NEW" | "INIT" | "PARTIALLY_FILLED" | "PART_FILLED" | "PENDING_CREATE"
                ) || detail.side != native_side(amend.order.side)
                    || detail.action
                        != if amend.order.reduce_only {
                            "CLOSE"
                        } else {
                            "OPEN"
                        }
                    || filled < amend.order.filled
                    || filled < self.filled_native(&amend.target.id)
                    || cx
                        .store
                        .order(&amend.target.id)
                        .is_none_or(|o| terminal(o.status))
                {
                    return Err("Bitrue amendment read is stale or terminal".into());
                }
                let protective = amend.target.conditional;
                if protective && detail.kind != "MARKET" {
                    return Err("Bitrue protection amendment requires a market execution".into());
                }
                if !protective && detail.kind != "LIMIT" {
                    return Err("Bitrue only amends resting limit orders".into());
                }
                let mut opts = PlaceOrderOpts {
                    symbol: amend.order.symbol.clone(),
                    kind: amend.order.kind,
                    side: amend.order.side,
                    amount: amend
                        .amount
                        .map(|total| safe_cex_engine::num::left_of_total(total, filled))
                        .transpose()?
                        .unwrap_or_else(|| {
                            safe_cex_engine::num::adjust(
                                amount - filled,
                                cx.store
                                    .market(&amend.order.symbol)
                                    .map_or(0.0, |m| m.precision.amount),
                            )
                        }),
                    price: amend.price.or(Some(amend.order.price)),
                    trigger_price: None,
                    stop_loss: None,
                    take_profit: None,
                    reduce_only: Some(amend.order.reduce_only),
                    time_in_force: amend.order.time_in_force,
                    trigger_type: None,
                    tp_trigger_type: None,
                    sl_trigger_type: None,
                    client_order_id: None,
                    close: amend.order.close,
                    stp_mode: None,
                    max_slippage_pct: None,
                    bybit_bbo: None,
                    bybit_slippage_tolerance_ticks: None,
                    implied_volatility: None,
                };
                let mut source = None;
                if protective {
                    if detail.trigger_type.map(|n| n.0)
                        != Some(if opts.kind == OrderType::StopLoss {
                            1.0
                        } else {
                            2.0
                        })
                    {
                        return Err(
                            "Bitrue protective replacement has incomplete trigger provenance"
                                .into(),
                        );
                    }
                    let reference = detail
                        .trigger_price_type
                        .map(|n| n.0)
                        .ok_or("Bitrue protective replacement requires its trigger reference")?;
                    let reference = match reference {
                        1.0 => StopTriggerType::LastPrice,
                        2.0 => StopTriggerType::MarkPrice,
                        _ => return Err("Unsupported Bitrue replacement trigger reference".into()),
                    };
                    if opts.kind == OrderType::StopLoss {
                        opts.sl_trigger_type = Some(reference);
                    } else {
                        opts.tp_trigger_type = Some(reference);
                    }
                    source = Some(reference);
                }
                let mut seen = HashSet::new();
                if let Some(children) = &detail.child_orders {
                    for child in children {
                        if matches!(
                            child.status.as_ref(),
                            "CANCELED" | "CANCELLED" | "FILLED" | "REJECTED" | "EXPIRED"
                        ) {
                            continue;
                        }
                        if child.kind != "MARKET" {
                            return Err("Unsupported Bitrue attached protection execution".into());
                        }
                        let kind = child
                            .trigger_type
                            .map(|n| n.0)
                            .ok_or("Bitrue attached protection is missing its type")?;
                        if !seen.insert(kind as u8) || !matches!(kind, 1.0 | 2.0) {
                            return Err("Ambiguous Bitrue attached protection".into());
                        }
                        let price = child
                            .trigger_price
                            .ok_or("Bitrue attached protection has no trigger price")?
                            .0;
                        let reference = match child.trigger_price_type.map(|n| n.0) {
                            Some(1.0) => StopTriggerType::LastPrice, Some(2.0) => StopTriggerType::MarkPrice,
                            _ => return Err("Bitrue attached replacement requires authoritative trigger references".into()),
                        };
                        if kind == 1.0 {
                            opts.stop_loss = Some(price);
                            opts.sl_trigger_type = Some(reference);
                        } else {
                            opts.take_profit = Some(price);
                            opts.tp_trigger_type = Some(reference);
                        }
                    }
                }
                let known_protection_missing = (amend.order.stop_loss.is_some()
                    && !seen.contains(&1))
                    || (amend.order.take_profit.is_some() && !seen.contains(&2))
                    || cx.store.orders().iter().any(|o| {
                        o.parent_id.as_ref() == Some(&amend.target.id)
                            && !terminal(o.status)
                            && !match o.kind {
                                OrderType::StopLoss => seen.contains(&1),
                                OrderType::TakeProfit => seen.contains(&2),
                                _ => false,
                            }
                    });
                if known_protection_missing {
                    return Err("Bitrue replacement cannot verify attached protection".into());
                }
                let rows = self.placement(cx, &opts)?;
                if let Some(reference) = source {
                    let original = Original {
                        amount,
                        filled,
                        trigger: detail.trigger_price.map_or(amend.order.price, |n| n.0),
                    };
                    work.stop = Some(self.stop_protect(cx, amend, &opts, reference, original)?);
                }
                let mut target = amend.target.clone();
                target.amount = amount;
                target.filled = filled;
                work.guard = Some(target.clone());
                work.affected = target.store_ids.clone();
                for id in &target.store_ids {
                    cx.store.set_order_status(id, OrderStatus::Canceling);
                }
                work.steps.push_back(target.cancel());
                work.steps
                    .push_back(target.lookup(Effect::Verify(target.clone())));
                for (_, order) in &rows {
                    cx.store.upsert_order(order.clone());
                }
                work.placements(rows);
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}

/// How a message names an amended order of `kind`.
fn what(kind: OrderType) -> &'static str {
    match kind {
        OrderType::StopLoss => "stop",
        OrderType::TakeProfit => "take profit",
        _ => "order",
    }
}
/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// A refused write answer. Only Bitrue's documented pre-execution codes and
/// auth/rate-limit statuses prove a refusal; 408/5xx do not prove anything.
fn answer_failure(response: &Response<'_>, message: String, documented: bool) -> VenueError {
    let error = VenueError::new(message);
    let rejected = |error: VenueError, kind| error.kind(kind).outcome(Outcome::Rejected);
    let code = serde_json::from_str::<crate::wire::Envelope<'_>>(response.body)
        .ok()
        .map(|e| e.code.to_string())
        .filter(|code| !code.is_empty());
    let error = match &code {
        Some(code) => error.venue_code(code.clone()),
        None => error,
    };
    match response.status {
        401 => rejected(error, ErrorKind::AuthInvalid),
        403 => rejected(error, ErrorKind::AuthPermission),
        429 => rejected(error, ErrorKind::RateLimited),
        408 | 500.. => error.outcome(Outcome::Unknown),
        _ if documented && code.as_deref() == Some("-1022") => {
            rejected(error, ErrorKind::AuthInvalid)
        }
        _ if documented => rejected(error, ErrorKind::Rejected),
        _ => error,
    }
}

fn serial(step: &Step) -> SerialCall {
    let read = step.read();
    SerialCall {
        auth: safe_cex_engine::request_context::plan::CallAuth::Preflight,
        operation: if read {
            Operation::Read
        } else {
            Operation::Mutation
        },
        timeout_ms: CALL_MS,
        // A write's one -1022 RefusedResend.
        attempts: if read { 1 } else { 2 },
        pacing: None,
    }
}
fn census(symbol: &str) -> Step {
    Step {
        receipts: Vec::new(),
        method: Method::Get,
        path: format!("/fapi/v2/openOrders?contractName={}", encode(symbol)),
        body: None,
        effect: Effect::Census(symbol.to_owned()),
    }
}
impl Step {
    fn is_place(&self) -> bool {
        matches!(self.effect, Effect::Place { .. })
    }
    fn is_census(&self) -> bool {
        matches!(self.effect, Effect::Census(_))
    }
    fn read(&self) -> bool {
        matches!(
            self.effect,
            Effect::Inspect(_) | Effect::Verify(_) | Effect::Census(_)
        )
    }
}
/// Native's aggregated text; the journal owns the outcome and kind.
fn cancellation_text(work: &Work) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct CancellationErrors<'a> {
        message: &'static str,
        canceled_order_ids: &'a [String],
        failed_orders: &'a [CancelFailure],
    }
    serde_json::to_string(&CancellationErrors {
        message: "Bitrue cancellation failed for one or more orders",
        canceled_order_ids: &work.ids,
        failed_orders: &work.cancel_errors,
    })
    .expect("cancellation errors")
}
/// Bounded correlation for a released write: its endpoint.
pub(crate) fn trade_reconciliation(
    request: &Request,
) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
    ReconciliationData::new(request.step.path.clone())
}

#[cfg(test)]
mod serial_auth_tests {
    use super::*;
    use safe_cex_engine::request_context::plan::CallAuth;

    #[test]
    fn serial_reads_and_writes_keep_the_declared_preflight_credential_policy() {
        let read = census("BTCUSDT");
        let write = Step {
            method: Method::Post,
            path: "/fapi/v2/leverage".into(),
            body: None,
            effect: Effect::Leverage {
                symbol: "BTC/USDT:USDT".into(),
                leverage: 2.,
            },
            receipts: vec![],
        };
        for (step, operation, attempts) in [
            (&read, Operation::Read, 1),
            (&write, Operation::Mutation, 2),
        ] {
            let call = serial(step);
            assert_eq!(call.auth, CallAuth::Preflight);
            assert_eq!(call.operation, operation);
            assert_eq!(call.attempts, attempts);
            assert_eq!(call.timeout_ms, CALL_MS);
        }
    }
}
