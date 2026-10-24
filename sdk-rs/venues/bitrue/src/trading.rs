//! Preflighted, serialized writes; replacements require final cancellation proof.
use crate::wire::{Number, Str, Text, check};
use crate::{Bitrue, Job, Pending};
use safe_cex_engine::model::*;
use safe_cex_engine::url::encode;
use safe_cex_engine::{Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, VenueError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

fn local_mutation_policy() -> safe_cex_engine::request_context::Policy {
    use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
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
    queue: VecDeque<Command>,
    busy: bool,
    census: Option<(Plan, u64)>,
    unresolved_protection: HashMap<String, u64>,
}
pub struct Request {
    plan: Box<Plan>,
    step: Step,
    protection: Option<ProtectionFence>,
}
struct ProtectionFence {
    symbol: String,
    revision: u64,
    previous: Option<u64>,
}
struct Plan {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    symbols: Vec<String>,
    creating: Vec<Order>,
    affected: Vec<String>,
    guard: Option<Target>,
    refresh_result: bool,
    protections: Vec<String>,
    cancel_batch: bool,
    cancel_errors: Vec<CancelFailure>,
    /// A cancellation leg's outcome is not known.
    cancel_unknown: bool,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
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
impl Plan {
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
            cancel_batch: false,
            cancel_errors: vec![],
            cancel_unknown: false,
            effective: false,
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
        let max_units = (maximum / step + 1e-9).floor();
        let units = (amount / step).round();
        if max_units < 1.0 || units > (u64::MAX / 2) as f64 {
            return Err("Bitrue order size is not representable".into());
        }
        let parts = (units / max_units).ceil() as usize;
        if parts > 1000 {
            return Err("Bitrue order requires too many splits".into());
        }
        if let Some(id) = &opts.client_order_id
            && (!caller_id(id)
                || protective
                || parts != 1
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
        let units = units as u64;
        let each = units / parts as u64;
        let remainder = units % parts as u64;
        for index in 0..parts {
            let quantity = safe_cex_engine::num::adjust(
                (each + u64::from((index as u64) < remainder)) as f64 * step,
                step,
            );
            if quantity < market.limits.amount.min {
                return Err("Bitrue split is below the minimum".into());
            }
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
    fn trade_plan(&self, cx: &Cx<'_, Pending>, command: Command) -> Result<Plan, String> {
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
        let mut plan = Plan::new(id);
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
                plan.placements(rows);
            }
            Command::AmendOrder {
                id, price, amount, ..
            } => {
                let order = cx.store.order(&id).ok_or("Unknown Bitrue order to amend")?;
                if terminal(order.status) {
                    return Err("Cannot amend a terminal Bitrue order".into());
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
                plan.symbol(&order.symbol);
                plan.steps
                    .push_back(target.lookup(Effect::Inspect(Box::new(amend))));
            }
            Command::CancelOrders { ids, .. } => {
                plan.cancel_batch = true;
                let mut seen = HashSet::new();
                for id in &ids {
                    if !seen.insert(id.clone()) {
                        continue;
                    }
                    let Some(order) = cx.store.order(id) else {
                        if let Some(target) = self.trading.acknowledged.get(id) {
                            if let Some(market) = cx.store.market_by_id(&target.contract) {
                                plan.symbol(&market.symbol);
                            }
                            plan.steps.push_back(target.cancel());
                            continue;
                        }
                        plan.cancel_errors.push(CancelFailure {
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
                            plan.cancel_errors.push(CancelFailure {
                                id: id.clone(),
                                error,
                            });
                            continue;
                        }
                    };
                    plan.symbol(&order.symbol);
                    plan.affected.extend(target.store_ids.clone());
                    plan.steps.push_back(target.cancel());
                }
            }
            Command::CancelSymbolOrders { symbol, .. } => {
                if self.trading.unresolved_protection.contains_key(&symbol) {
                    return Err("Bitrue protective placement outcome is uncertain; reconcile before cancel-all".into());
                }
                plan.cancel_batch = true;
                let orders = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == symbol && o.status == OrderStatus::Open);
                let mut targets = Vec::new();
                for order in orders {
                    match self.cancel_target(cx, order) {
                        Ok(target) => targets.push(target),
                        Err(error) => plan.cancel_errors.push(CancelFailure {
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
                plan.symbol(&symbol);
                for target in targets {
                    plan.affected.extend(target.store_ids.clone());
                    plan.steps.push_back(target.cancel());
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
                plan.symbol(&symbol);
                plan.steps.push_back(Step {
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
        Ok(plan)
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
        if let Err(error) = self.trade_plan(cx, command.clone()) {
            let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                return true;
            };
            let _ = cx.fail_context(&context, not_sent(error));
            return true;
        }
        self.trading.queue.push_back(command);
        self.trade_next(cx);
        true
    }
    fn trade_next(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.busy {
            return;
        }
        let Some(command) = self.trading.queue.pop_front() else {
            return;
        };
        let id = match &command {
            Command::PlaceOrders { command, .. }
            | Command::CancelOrders { command, .. }
            | Command::CancelSymbolOrders { command, .. }
            | Command::AmendOrder { command, .. }
            | Command::SetLeverage { command, .. } => *command,
            _ => unreachable!(),
        };
        match self.trade_plan(cx, command) {
            Ok(plan) => {
                self.trading.busy = true;
                self.account.orders_revision += 1;
                for order in &plan.creating {
                    cx.store.upsert_order(order.clone());
                }
                for id in &plan.affected {
                    cx.store.set_order_status(id, OrderStatus::Canceling);
                }
                self.trade_step(cx, plan);
            }
            Err(error) => {
                cx.fail(id, not_sent(error));
                self.trade_next(cx);
            }
        }
    }
    fn trade_step(&mut self, cx: &mut Cx<'_, Pending>, mut plan: Plan) {
        let step = loop {
            let Some(step) = plan.steps.pop_front() else {
                self.trade_finish(cx, plan, None);
                return;
            };
            if plan.cancel_batch
                && let Effect::Cancel(target) = &step.effect
            {
                // A confirmed parent cancellation can close its native children.
                // Keep each child as a fallback until that success is known.
                if let Some(order) = cx.store.order(&target.id)
                    && terminal(order.status)
                {
                    if order.status == OrderStatus::Canceled {
                        plan.ids.push(target.id.clone());
                    }
                    continue;
                }
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
            break step;
        };
        if matches!(step.effect, Effect::Place { .. } | Effect::Verify(_))
            && plan.guard.as_ref().is_some_and(|target| {
                self.filled_native(&target.id) > target.filled
                    || cx.store.order(&target.id).is_some_and(|o| {
                        o.status == OrderStatus::Closed || o.filled > target.filled
                    })
            })
        {
            self.trade_failed(
                cx,
                Request {
                    plan: Box::new(plan),
                    step,
                    protection: None,
                },
                not_sent("Bitrue replacement aborted after execution progress"),
            );
            return;
        }
        cx.command_progress(plan.command, step.receipts.clone());
        // An amend is not an engine placement scope. Fence its replacement before
        // dispatch, since a lost create ACK may still leave native protection live.
        let protection = if let Effect::Place {
            protective: true,
            target,
            ..
        } = &step.effect
        {
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
        } else {
            None
        };
        let path = step.path.clone();
        let body = step.body.clone();
        self.send(
            cx,
            step.method,
            &path,
            body,
            None,
            Job::Trade(Request {
                plan: Box::new(plan),
                step,
                protection,
            }),
        );
    }
    fn trade_finish(&mut self, cx: &mut Cx<'_, Pending>, plan: Plan, error: Option<VenueError>) {
        let creating: Vec<_> = plan.creating.iter().map(|o| o.id.as_str()).collect();
        cx.store.remove_orders(&creating);
        for id in &plan.affected {
            if cx
                .store
                .order(id)
                .is_some_and(|o| o.status == OrderStatus::Canceling)
            {
                cx.store.set_order_status(id, OrderStatus::Open);
            }
        }
        if error.is_none() && !plan.protections.is_empty() {
            // Protective ACK IDs and echoed client IDs are unreliable. A complete
            // post-write census, including conditionals, owns the native open set.
            self.account.orders_revision += 1;
            for symbol in &plan.protections {
                self.trading
                    .unresolved_protection
                    .insert(symbol.clone(), self.account.orders_revision);
            }
            let symbols = plan.symbols.clone();
            self.trading.census = Some((plan, self.account.orders_revision));
            self.refresh_orders(cx, symbols);
            self.refresh_account(cx);
            return;
        }
        self.trading.busy = false;
        let error = if plan.cancel_errors.is_empty() {
            error.map(|mut error| {
                if (plan.effective || !plan.ids.is_empty())
                    && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
                {
                    error.outcome = None;
                }
                error
            })
        } else {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct CancellationErrors<'a> {
                message: &'static str,
                canceled_order_ids: &'a [String],
                failed_orders: &'a [CancelFailure],
            }
            let error = VenueError::new(
                serde_json::to_string(&CancellationErrors {
                    message: "Bitrue cancellation failed for one or more orders",
                    canceled_order_ids: &plan.ids,
                    failed_orders: &plan.cancel_errors,
                })
                .expect("cancellation errors"),
            );
            // Legs may differ; only an unknown leg is worth stating.
            Some(if plan.cancel_unknown {
                error.outcome(Outcome::Unknown)
            } else {
                error
            })
        };
        match error {
            Some(error) => cx.fail(plan.command, error),
            None => cx.done(
                plan.command,
                if plan.refresh_result || plan.cancel_batch {
                    "[]".to_owned()
                } else {
                    serde_json::to_string(&plan.ids).expect("order ids")
                },
            ),
        }
        if !plan.symbols.is_empty() {
            self.refresh_orders(cx, plan.symbols);
            self.refresh_account(cx);
        }
        self.trade_next(cx);
    }
    /// The command whose protective placement this order read would certify.
    pub(crate) fn census_owner(&self, symbol: &str, revision: u64) -> Option<CommandId> {
        self.trading.census.as_ref().and_then(|(plan, minimum)| {
            (revision >= *minimum && plan.protections.iter().any(|s| s == symbol))
                .then_some(plan.command)
        })
    }
    pub(crate) fn trade_census(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        revision: u64,
        success: bool,
    ) -> bool {
        let Some((mut plan, minimum)) = self.trading.census.take() else {
            if success
                && self
                    .trading
                    .unresolved_protection
                    .get(symbol)
                    .is_some_and(|minimum| revision >= *minimum)
            {
                self.trading.unresolved_protection.remove(symbol);
            }
            return success;
        };
        if revision < minimum || !plan.protections.iter().any(|s| s == symbol) {
            self.trading.census = Some((plan, minimum));
            return success;
        }
        if !success {
            self.trade_finish(
                cx,
                plan,
                Some(VenueError::new("Bitrue protective placement outcome is uncertain; complete fresh order census is required").outcome(Outcome::Unknown)),
            );
            return false;
        }
        self.trading.unresolved_protection.remove(symbol);
        plan.protections.retain(|s| s != symbol);
        if plan.protections.is_empty() {
            self.trade_finish(cx, plan, None);
        } else {
            self.trading.census = Some((plan, minimum));
        }
        true
    }
    pub(crate) fn trade_item(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        _request: &mut Request,
        _item: &str,
    ) {
    }
    pub(crate) fn trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        error: VenueError,
    ) {
        if matches!(request.step.effect, Effect::Place { .. }) {
            self.account.orders_revision = self.account.orders_revision.wrapping_add(1);
        }
        if request.plan.cancel_batch
            && let Effect::Cancel(target) = &request.step.effect
        {
            request.plan.cancel_unknown |= error.outcome == Some(Outcome::Unknown);
            request.plan.cancel_errors.push(CancelFailure {
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
            self.trade_step(cx, *request.plan);
        } else {
            self.trade_finish(cx, *request.plan, Some(error));
        }
    }
    pub(crate) fn trade_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
        prior_attempt: bool,
    ) {
        let read = matches!(request.step.effect, Effect::Inspect(_) | Effect::Verify(_));
        if (prior_attempt && !read) || request.plan.effective || !request.plan.ids.is_empty() {
            error.outcome = Some(Outcome::Unknown);
        }
        self.trade_failed(cx, request, error);
    }
    /// A request that never answered. Reads precede a write that was never sent.
    pub(crate) fn trade_lost(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        let read = matches!(request.step.effect, Effect::Inspect(_) | Effect::Verify(_));
        let error = VenueError::new(error)
            .kind(ErrorKind::Network)
            .outcome(if read {
                Outcome::NotSent
            } else {
                Outcome::Unknown
            });
        self.trade_failed(cx, request, error);
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if let Err(error) = check(&response) {
            // Only documented, unambiguous pre-execution rejections undo this
            // write's fence. Unknown/server outcomes require a fresh census.
            #[derive(Deserialize)]
            struct Rejected<'a> {
                #[serde(borrow)]
                code: Text<'a>,
                #[serde(borrow)]
                data: Option<&'a serde_json::value::RawValue>,
            }
            let rejected = (200..500).contains(&response.status)
                && response.status != 408
                && serde_json::from_str::<Rejected<'_>>(response.body).is_ok_and(|reply| {
                    // Futures error list: signature, mandatory parameter, contract, balance.
                    matches!(&*reply.code, "-1022" | "-1102" | "-1121" | "-2017")
                        && reply.data.is_none_or(|data| data.get() == "null")
                });
            if rejected
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
            if let Effect::Leverage { symbol, .. } = &request.step.effect
                && let Ok(reply) = serde_json::from_str::<crate::wire::Envelope<'_>>(response.body)
            {
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
                        self.account.bracket_limits.insert(symbol.clone(), max);
                        cx.store.update_markets(|market| {
                            if market.symbol == *symbol {
                                market.limits.leverage.max = max;
                            }
                        });
                    }
                }
            }
            let read = matches!(request.step.effect, Effect::Inspect(_) | Effect::Verify(_));
            let mut error = answer_failure(&response, error, rejected);
            if read {
                error.outcome = Some(Outcome::NotSent);
            }
            self.trade_failed(cx, request, error);
            return;
        }
        let result = match &request.step.effect {
            Effect::Place {
                conditional,
                protective,
                target,
            } => {
                let reply = serde_json::from_str::<Reply<Identity<'_>>>(response.body)
                    .map_err(|e| format!("Malformed Bitrue placement: {e}"));
                match reply {
                    Ok(reply) if !reply.data.order_id.is_empty() => {
                        let id = reply.data.order_id.to_string();
                        if !protective && cx.store.order(&id).is_none() {
                            let mut target = target.clone();
                            target.id = id.clone();
                            self.trading.acknowledged.insert(id, target);
                        }
                        if !conditional {
                            request.plan.ids.push(reply.data.order_id.to_string());
                        }
                        Ok(())
                    }
                    Ok(_) => Err("Bitrue placement returned no order identity".into()),
                    Err(error) => Err(error),
                }
            }
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
                    if request.plan.cancel_batch {
                        request.plan.ids.push(target.id.clone());
                    }
                    Ok(())
                } else {
                    Err("Bitrue cancellation did not confirm the requested order identity".into())
                }
            }
            Effect::Inspect(_) | Effect::Verify(_) => {
                self.trade_detail(cx, &mut request, response.body)
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
                Ok(())
            }
        };
        if let Err(error) = result {
            // Reads precede a write that was never sent; other 2xx answers are unproven.
            let outcome = if matches!(request.step.effect, Effect::Inspect(_) | Effect::Verify(_)) {
                Outcome::NotSent
            } else {
                Outcome::Unknown
            };
            self.trade_failed(cx, request, VenueError::new(error).outcome(outcome));
        } else {
            if !matches!(request.step.effect, Effect::Inspect(_) | Effect::Verify(_)) {
                request.plan.effective = true;
            }
            self.trade_step(cx, *request.plan);
        }
    }
    fn trade_detail(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        body: &str,
    ) -> Result<(), String> {
        let Reply { data } = serde_json::from_str::<Reply<OneOrMany<Detail<'_>>>>(body)
            .map_err(|e| format!("Malformed Bitrue order detail: {e}"))?;
        let detail = data.one()?;
        match &request.step.effect {
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
                    amount: amend.amount.unwrap_or_else(|| {
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
                let mut target = amend.target.clone();
                target.amount = amount;
                target.filled = filled;
                request.plan.guard = Some(target.clone());
                request.plan.affected = target.store_ids.clone();
                for id in &target.store_ids {
                    cx.store.set_order_status(id, OrderStatus::Canceling);
                }
                request.plan.steps.push_back(target.cancel());
                request
                    .plan
                    .steps
                    .push_back(target.lookup(Effect::Verify(target.clone())));
                for (_, order) in &rows {
                    cx.store.upsert_order(order.clone());
                }
                request.plan.placements(rows);
            }
            _ => unreachable!(),
        }
        Ok(())
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

impl Request {
    pub(crate) fn credential_role(&self) -> safe_cex_engine::credential_selection::CredentialRole {
        use safe_cex_engine::credential_selection::CredentialRole;
        match self.step.effect {
            Effect::Inspect(_) | Effect::Verify(_) => CredentialRole::Read { scope: None },
            _ => CredentialRole::Mutation,
        }
    }
}
