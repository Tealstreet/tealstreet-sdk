//! Foreground writes retain legacy TS sequencing; WritePlan observes outcomes.
//! An uncertain physical write is never resent.
use crate::account::Next;
use crate::sign::Params;
use crate::{Backpack, Pending, REST};
use safe_cex_engine::Level;
use safe_cex_engine::classify::{
    BatchItem, CodeTable, Intent, Native, NativeCode, Verdict, WriteFact,
};
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{
    Context, Credential, HttpOwnerRetirement, Operation, Plan, PlanPolicy, Policy, Preflight,
    PreflightPolicy, PreflightPurpose, ReconciliationData, RetryPolicy, WriteResult,
    plan::{CallBound, RetryWindow, SerialCall, SerialPolicy, Slot, StepBound},
};
use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, WritePlan};
use safe_cex_engine::{
    Command, CommandId, CommandWait, Cx, ErrorKind, Method, Outcome, Response, VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub(crate) const UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

fn with_unknown_guidance(mut error: VenueError) -> VenueError {
    if error.outcome == Some(Outcome::Unknown) {
        error.message = UNKNOWN_TEXT.into();
    }
    error
}

const BACKPACK: CodeTable = CodeTable {
    rows: &[],
    success: &[],
    unlisted: Verdict::Unknown(None),
    status: &[],
};
/// Only a trigger amend's replacement refused with a coded 4xx (`definite_refusal`)
/// is journaled refused, so its proof read and put-back may follow.
const AMEND_REFUSAL: CodeTable = CodeTable {
    rows: &[],
    success: &[],
    unlisted: Verdict::Refused(ErrorKind::Rejected),
    status: &[],
};

#[derive(Clone)]
struct ManagedWrite {
    plan: Plan,
    slot: Slot,
    context: Context,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeOrder {
    pub(crate) symbol: String,
    pub(crate) side: &'static str,
    order_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_in_force: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_trigger_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_trigger_by: Option<&'static str>,
}
impl NativeOrder {
    fn identity(&self, store: &safe_cex_engine::Store) -> Option<OrderIdentityReceipt> {
        Some(OrderIdentityReceipt {
            symbol: store
                .market_by_id(&self.symbol)
                .map_or_else(|| self.symbol.clone(), |market| market.symbol.clone()),
            kind: OrderIdentityKind::ClientOrderId,
            identity: self.client_id?.to_string(),
        })
    }

    pub(crate) fn params(&self) -> Params {
        let mut p: Params = [
            ("symbol".into(), self.symbol.clone()),
            ("side".into(), self.side.into()),
            ("orderType".into(), self.order_type.into()),
        ]
        .into();
        macro_rules! field {
            ($key:literal,$value:expr) => {
                if let Some(value) = $value {
                    p.insert($key.into(), value.to_string());
                }
            };
        }
        field!("price", self.price.as_deref());
        field!("quantity", self.quantity.as_deref());
        field!("triggerPrice", self.trigger_price.as_deref());
        field!("triggerQuantity", self.trigger_quantity.as_deref());
        field!("triggerBy", self.trigger_by);
        field!("timeInForce", self.time_in_force);
        field!("postOnly", self.post_only);
        field!("reduceOnly", self.reduce_only);
        field!("clientId", self.client_id);
        field!(
            "takeProfitTriggerPrice",
            self.take_profit_trigger_price.as_deref()
        );
        field!("takeProfitTriggerBy", self.take_profit_trigger_by);
        field!(
            "stopLossTriggerPrice",
            self.stop_loss_trigger_price.as_deref()
        );
        field!("stopLossTriggerBy", self.stop_loss_trigger_by);
        p
    }
}
#[derive(Clone)]
pub enum Step {
    Place {
        order: Box<NativeOrder>,
        optional: bool,
    },
    Pair(Vec<NativeOrder>),
    Cancel {
        symbol: String,
        id: String,
        optional: bool,
    },
    CancelAll {
        symbol: String,
    },
    Leverage(f64),
    OpenOrders {
        symbol: String,
    },
}
impl Step {
    fn places(&self) -> bool {
        matches!(self, Self::Place { .. })
    }

    fn reads_proof(&self) -> bool {
        matches!(self, Self::OpenOrders { .. })
    }

    fn is_entry(&self) -> bool {
        matches!(
            self,
            Self::Place {
                optional: false,
                ..
            }
        )
    }
}

fn write_plan(steps: &[Step]) -> (WritePlan, Vec<Vec<usize>>) {
    let mut journal = vec![];
    let mut index = 0;
    let writes = steps
        .iter()
        .map(|step| {
            if matches!(step, Step::OpenOrders { .. }) {
                return vec![];
            }
            let orders: &[NativeOrder] = match step {
                Step::Place { order, .. } => std::slice::from_ref(order.as_ref()),
                Step::Pair(orders) => orders,
                _ => &[],
            };
            let mut elements: Vec<_> = orders
                .iter()
                .map(|order| {
                    let element = Element {
                        index,
                        client_order_id: order.client_id.map(|id| id.to_string()),
                    };
                    index += 1;
                    element
                })
                .collect();
            if elements.is_empty() {
                elements.push(Element {
                    index,
                    client_order_id: None,
                });
                index += 1;
            }
            let indices = elements.iter().map(|element| element.index).collect();
            journal.extend(elements);
            indices
        })
        .collect();
    (WritePlan::new(journal), writes)
}

pub struct Request {
    managed: Option<ManagedWrite>,
    write: usize,
    command: u64,
    step: Step,
    previous: Option<OrderStatus>,
    targets: Vec<(String, f64)>,
    leverage_revision: u64,
}
pub struct Repair {
    pub(crate) old_id: String,
    pub(crate) order: NativeOrder,
    pub(crate) amount: f64,
    pub(crate) high_water: Option<f64>,
    pub(crate) uncertain: bool,
    pub(crate) terminal: bool,
}
struct Job {
    managed: Option<Plan>,
    write_plan: WritePlan,
    steps: Vec<Step>,
    elements: Vec<Vec<usize>>,
    next: usize,
    continue_rows: bool,
    skip_children: bool,
    failure: Option<VenueError>,
    waiting: bool,
    cancel_latches: Vec<String>,
    ids: Vec<String>,
    collect_ids: bool,
    amendment: bool,
    /// The amended order's side and kind: what its lost replacement costs.
    amended: Option<(OrderSide, OrderType)>,
    protect: Option<StopProtect>,
    refused: Option<Box<NativeOrder>>,
    restoring: bool,
}

struct StopProtect {
    original: Order,
    restore: Box<NativeOrder>,
    restorable: bool,
}

fn append_step(job: &mut Job, step: Step) {
    let outcomes = job.write_plan.outcomes();
    job.steps.push(step);
    let (plan, elements) = write_plan(&job.steps);
    job.write_plan = plan;
    job.elements = elements;
    for outcome in outcomes {
        let _ = job.write_plan.record(outcome.element.index, outcome.result);
    }
}

fn record_failure(plan: &mut WritePlan, indices: &[usize], error: &VenueError) {
    for index in indices {
        let result = if matches!(error.outcome, Some(Outcome::NotSent | Outcome::Rejected)) {
            ElementResult::NotSent {
                error: Some(Box::new(error.clone())),
            }
        } else {
            ElementResult::Unknown {
                error: Box::new(VenueError {
                    message: UNKNOWN_TEXT.into(),
                    ..error.clone().outcome(Outcome::Unknown)
                }),
            }
        };
        let _ = plan.record(*index, result);
    }
}

#[derive(Default)]
pub struct Trading {
    jobs: HashMap<u64, Job>,
    next_client: u32,
    cancels: HashMap<String, (usize, OrderStatus)>,
}

/// A trigger amend's write: one attempt, as every Backpack write.
fn mutation_call() -> SerialCall {
    SerialCall {
        auth: Default::default(),
        operation: Operation::Mutation,
        timeout_ms: 20000,
        attempts: 1,
        pacing: None,
    }
}
fn serial_call(step: &Step) -> SerialCall {
    SerialCall {
        operation: if matches!(step, Step::OpenOrders { .. }) {
            Operation::Read
        } else {
            Operation::Mutation
        },
        ..mutation_call()
    }
}
/// How a message names an amended trigger order of `kind`.
fn what(kind: OrderType) -> &'static str {
    match kind {
        OrderType::TakeProfit => "take profit",
        OrderType::StopLoss => "stop",
        _ => "trigger order",
    }
}
/// A definite refusal of an order write: a 4xx with a JSON code, except a
/// timeout, conflict or rate limit. Backpack documents no code table.
fn definite_refusal(status: u16, body: &str) -> bool {
    #[derive(Deserialize)]
    struct Failure<'a> {
        #[serde(borrow)]
        code: Option<&'a serde_json::value::RawValue>,
    }
    (400..500).contains(&status)
        && !matches!(status, 408 | 409 | 429)
        && serde_json::from_str::<Failure<'_>>(body).is_ok_and(|failure| failure.code.is_some())
}

fn gone_text(symbol: &str, side: OrderSide, kind: OrderType, unknown: bool) -> String {
    if matches!(kind, OrderType::StopLoss | OrderType::TakeProfit) {
        safe_cex_engine::error::stop_gone_text(symbol, side, kind, unknown)
    } else {
        safe_cex_engine::error::order_gone_text(symbol, unknown)
    }
}

fn client_id(id: &str) -> Result<u32, String> {
    let n = id.parse::<u32>().map_err(|_| {
        "Backpack clientOrderId must be a canonical uint32 decimal string".to_owned()
    })?;
    if n == 0 || n.to_string() != id {
        return Err("Backpack clientOrderId must be a canonical uint32 decimal string".into());
    }
    Ok(n)
}
fn trigger(kind: Option<StopTriggerType>) -> &'static str {
    match kind.unwrap_or(StopTriggerType::MarkPrice) {
        StopTriggerType::MarkPrice => "MarkPrice",
        StopTriggerType::LastPrice => "LastPrice",
        StopTriggerType::IndexPrice => "IndexPrice",
    }
}
fn formatted(n: f64, step: f64) -> Result<String, String> {
    if !n.is_finite() || n <= 0.0 || !step.is_finite() || step <= 0.0 {
        return Err("Backpack order amount/price must be positive and finite".into());
    }
    let adjusted = num::adjust(n, step);
    if adjusted <= 0.0 {
        return Err("Backpack order rounds to zero".into());
    }
    Ok(format!("{:.*}", num::decimals(step) as usize, adjusted))
}
fn ordinary(o: &PlaceOrderOpts) -> bool {
    o.kind == OrderType::Limit
        && o.trigger_price.is_none()
        && o.stop_loss.is_none()
        && o.take_profit.is_none()
        && o.close != Some(true)
        && o.client_order_id.is_none()
}

impl Backpack {
    fn native(&self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<NativeOrder, String> {
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| format!("Market not found: {}", o.symbol))?;
        if !m.active || m.is_read_only == Some(true) {
            return Err("Backpack market is not placeable".into());
        }
        if matches!(o.kind, OrderType::TrailingStopLoss | OrderType::Conditional) {
            return Err("Backpack unsupported order type".into());
        }
        if let Some(id) = &o.client_order_id {
            client_id(id)?;
            if !matches!(o.kind, OrderType::Limit | OrderType::Market)
                || o.trigger_price.is_some()
                || o.stop_loss.is_some()
                || o.take_profit.is_some()
                || o.close == Some(true)
                || (m.limits.amount.max > 0.0 && o.amount > m.limits.amount.max)
            {
                return Err("Backpack clientOrderId requires one ordinary native order".into());
            }
        }
        let stop = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
        let triggered = stop || o.trigger_price.is_some();
        let limit = o.kind == OrderType::Limit;
        let qty = formatted(o.amount, m.precision.amount)?;
        if o.amount < m.limits.amount.min
            || (m.limits.amount.max > 0.0 && o.amount > m.limits.amount.max)
        {
            return Err("Backpack order amount is outside market bounds".into());
        }
        let mut native = NativeOrder {
            symbol: m.id.clone(),
            side: if o.side == OrderSide::Buy {
                "Bid"
            } else {
                "Ask"
            },
            order_type: if limit { "Limit" } else { "Market" },
            price: None,
            quantity: None,
            trigger_price: None,
            trigger_quantity: None,
            trigger_by: None,
            time_in_force: None,
            post_only: None,
            reduce_only: (stop || o.reduce_only == Some(true)).then_some(true),
            client_id: o
                .client_order_id
                .as_ref()
                .map(|id| client_id(id))
                .transpose()?,
            take_profit_trigger_price: None,
            take_profit_trigger_by: None,
            stop_loss_trigger_price: None,
            stop_loss_trigger_by: None,
        };
        if limit {
            native.price = Some(formatted(
                o.price.ok_or("Backpack limit order needs price")?,
                m.precision.price,
            )?);
            native.post_only = Some(o.time_in_force == Some(TimeInForce::PostOnly));
            native.time_in_force = Some(match o.time_in_force {
                Some(TimeInForce::ImmediateOrCancel) => "IOC",
                Some(TimeInForce::FillOrKill) => "FOK",
                _ => "GTC",
            });
        }
        if triggered {
            native.trigger_price = Some(formatted(
                o.trigger_price
                    .or(o.price)
                    .ok_or("Backpack trigger order needs trigger price")?,
                m.precision.price,
            )?);
            native.trigger_quantity = Some(qty);
            native.trigger_by = Some(trigger(if o.kind == OrderType::TakeProfit {
                o.tp_trigger_type.or(o.trigger_type)
            } else {
                o.sl_trigger_type.or(o.trigger_type)
            }));
        } else {
            native.quantity = Some(qty);
        }
        if !limit {
            if let Some(price) = o.take_profit {
                let p = formatted(price, m.precision.price)?;
                native.take_profit_trigger_price = Some(p);
                native.take_profit_trigger_by = Some(trigger(o.tp_trigger_type));
            }
            if let Some(price) = o.stop_loss {
                let p = formatted(price, m.precision.price)?;
                native.stop_loss_trigger_price = Some(p);
                native.stop_loss_trigger_by = Some(trigger(o.sl_trigger_type));
            }
        }
        Ok(native)
    }
    fn placement_steps(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
    ) -> Result<Vec<Step>, String> {
        let main = self.native(cx, o)?;
        let mut steps = vec![Step::Place {
            order: Box::new(main.clone()),
            optional: false,
        }];
        if o.kind == OrderType::Limit {
            let m = cx.store.market(&o.symbol).unwrap();
            for (price, kind) in [
                (o.take_profit, o.tp_trigger_type),
                (o.stop_loss, o.sl_trigger_type),
            ] {
                if let Some(price) = price {
                    let mut order = main.clone();
                    order.side = if o.side == OrderSide::Buy {
                        "Ask"
                    } else {
                        "Bid"
                    };
                    order.order_type = "Market";
                    order.price = None;
                    order.quantity = None;
                    order.time_in_force = None;
                    order.post_only = None;
                    order.client_id = None;
                    order.reduce_only = Some(true);
                    order.trigger_price = Some(formatted(price, m.precision.price)?);
                    order.trigger_quantity = Some(formatted(o.amount, m.precision.amount)?);
                    order.trigger_by = Some(trigger(kind));
                    steps.push(Step::Place {
                        order: Box::new(order),
                        optional: true,
                    });
                }
            }
        }
        Ok(steps)
    }
    fn generated_client(
        &mut self,
        cx: &Cx<'_, Pending>,
        used: &HashSet<u32>,
    ) -> Result<u32, String> {
        if self.trading.next_client == 0 {
            let random = safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?;
            self.trading.next_client = u32::from_le_bytes(
                safe_cex_crypto::sha256(random.as_bytes())[..4]
                    .try_into()
                    .unwrap(),
            )
            .max(1);
        }
        for _ in 0..4096 {
            let id = self.trading.next_client;
            self.trading.next_client = self.trading.next_client.wrapping_add(1).max(1);
            if !used.contains(&id)
                && !cx.store.orders().iter().any(|o| {
                    o.client_order_id
                        .as_ref()
                        .is_some_and(|s| s == &id.to_string())
                })
            {
                return Ok(id);
            }
        }
        Err("Backpack cannot mint unique batch IDs".into())
    }
    fn prepare_place(
        &mut self,
        cx: &Cx<'_, Pending>,
        orders: Vec<PlaceOrderOpts>,
    ) -> Result<Vec<Step>, String> {
        if orders.iter().any(|order| {
            order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some()
        }) {
            return Err("Bybit BBO and TickSize controls are not supported by Backpack".into());
        }
        let mut caller = HashSet::new();
        let mut prepared = Vec::new();
        for o in &orders {
            if let Some(id) = &o.client_order_id
                && !caller.insert(client_id(id)?)
            {
                return Err("Backpack duplicated caller clientOrderId".into());
            }
            prepared.push(self.placement_steps(cx, o)?);
        }
        let eligible = |o: &PlaceOrderOpts| {
            ordinary(o)
                && cx
                    .store
                    .market(&o.symbol)
                    .is_some_and(|m| m.limits.amount.max > 0.0 && o.amount <= m.limits.amount.max)
        };
        let mut steps = Vec::new();
        let mut i = 0;
        while i < orders.len() {
            if i + 1 < orders.len() && eligible(&orders[i]) && eligible(&orders[i + 1]) {
                let mut pair = vec![];
                for p in &prepared[i..i + 2] {
                    let Step::Place { order, .. } = &p[0] else {
                        unreachable!()
                    };
                    let mut order = (**order).clone();
                    let id = self.generated_client(cx, &caller)?;
                    caller.insert(id);
                    order.client_id = Some(id);
                    pair.push(order);
                }
                steps.push(Step::Pair(pair));
                i += 2;
            } else {
                steps.append(&mut prepared[i]);
                i += 1;
            }
        }
        Ok(steps)
    }
    fn reject_fixed_setting(cx: &mut Cx<'_, Pending>, message: &str) {
        match cx.command_wait() {
            Some(CommandWait::Remaining(_)) => {}
            Some(CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Mutation);
                return;
            }
            None => {
                let _ = cx.reject_unstamped_command(Operation::Mutation);
                return;
            }
        }
        let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Mutation,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) else {
            return;
        };
        let _ = cx.fail_context(&context, not_sent(message).kind(ErrorKind::Unsupported));
    }

    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let continue_rows = matches!(
            &command,
            Command::PlaceOrders { .. } | Command::CancelOrders { .. }
        );
        let managed = matches!(
            &command,
            Command::PlaceOrders { .. }
                | Command::CancelOrders { .. }
                | Command::CancelSymbolOrders { .. }
                | Command::AmendOrder { .. }
                | Command::SetLeverage { .. }
        );
        let preflight = if managed {
            let credential = self.options.identity.as_ref().map(|identity| Credential {
                slot: safe_cex_engine::CredentialSlot::Primary,
                reference: identity.credential.clone(),
            });
            let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: credential
                    .clone()
                    .filter(|_| matches!(&command, Command::AmendOrder { .. })),
                write_credential: credential,
            }) else {
                return;
            };
            if self.options.public_data_only {
                let _ = cx.fail_preflight(
                    &preflight,
                    not_sent("Backpack writes are disabled in public-data-only mode")
                        .kind(ErrorKind::AuthInvalid),
                );
                return;
            }
            if self.options.identity.is_none() {
                let _ = cx.fail_preflight(
                    &preflight,
                    not_sent("Backpack managed signing identity unavailable")
                        .kind(ErrorKind::AuthInvalid),
                );
                return;
            }
            Some(preflight)
        } else {
            None
        };
        let mut protect = None;
        let (id, plan) = match command {
            Command::PlaceOrders { command, orders } => {
                (command, self.prepare_place(cx, orders).map(|s| (s, true)))
            }
            Command::CancelOrders { command, ids } => {
                let steps = ids
                    .into_iter()
                    .map(|id| {
                        let order = cx
                            .store
                            .order(&id)
                            .ok_or_else(|| format!("Order not found: {id}"))?;
                        Ok(Step::Cancel {
                            symbol: order.symbol.clone(),
                            id,
                            optional: true,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>();
                (command, steps.map(|s| (s, false)))
            }
            Command::CancelSymbolOrders { command, symbol } => (
                command,
                if cx.store.market(&symbol).is_some() {
                    Ok((vec![Step::CancelAll { symbol }], false))
                } else {
                    Err(format!("Market not found: {symbol}"))
                },
            ),
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => {
                let plan = (|| {
                    let old = cx
                        .store
                        .order(&id)
                        .ok_or_else(|| format!("Order not found: {id}"))?
                        .clone();
                    if matches!(old.status, OrderStatus::Closed | OrderStatus::Canceled) {
                        return Err("Backpack cannot amend a terminal order".into());
                    }
                    if amount.unwrap_or(old.amount) <= 0. {
                        return Err("Backpack amended total must be positive".into());
                    }
                    let edited = |price: Option<f64>, amount: Option<f64>| PlaceOrderOpts {
                        symbol: old.symbol.clone(),
                        kind: old.kind,
                        side: old.side,
                        amount: amount.unwrap_or(old.amount),
                        price: Some(price.unwrap_or(old.price)),
                        trigger_price: None,
                        stop_loss: None,
                        take_profit: None,
                        reduce_only: Some(old.reduce_only),
                        time_in_force: old.time_in_force,
                        trigger_type: None,
                        tp_trigger_type: None,
                        sl_trigger_type: None,
                        client_order_id: None,
                        close: None,
                        stp_mode: None,
                        max_slippage_pct: None,
                        bybit_bbo: None,
                        bybit_slippage_tolerance_ticks: None,
                        implied_volatility: None,
                    };
                    let o = edited(price, amount);
                    let execution_price = old
                        .extra
                        .as_ref()
                        .and_then(|extra| extra.conditional_order_price);
                    if matches!(old.kind, OrderType::StopLoss | OrderType::TakeProfit)
                        || execution_price.is_some()
                    {
                        let mut original = edited(None, None);
                        original.trigger_type = old.trigger_type;
                        if let Some(price) = execution_price {
                            original.price = Some(price);
                            original.trigger_price = Some(old.price);
                        }
                        protect = Some(StopProtect {
                            original: old.clone(),
                            restore: Box::new(self.native(cx, &original)?),
                            restorable: old.filled == 0.0,
                        });
                    }
                    let mut steps = self.placement_steps(cx, &o)?;
                    steps.insert(
                        0,
                        Step::Cancel {
                            symbol: old.symbol.clone(),
                            id,
                            optional: false,
                        },
                    );
                    Ok((steps, true))
                })();
                (command, plan)
            }
            Command::SetLeverage {
                command, leverage, ..
            } => (
                command,
                if leverage.is_finite() && leverage > 0.0 {
                    Ok((vec![Step::Leverage(leverage)], false))
                } else {
                    Err("Backpack leverage must be positive and finite".into())
                },
            ),
            Command::SetMarginType { .. } => {
                Self::reject_fixed_setting(cx, "Backpack only supports cross margin");
                return;
            }
            Command::SetPositionMode { .. } => {
                Self::reject_fixed_setting(cx, "Backpack position mode is fixed");
                return;
            }
            _ => return,
        };
        if self.trading.jobs.contains_key(&u64::from(id)) {
            let _ = cx.fail_preflight(
                preflight.as_ref().unwrap(),
                not_sent("Backpack command is already active"),
            );
            return;
        }
        match plan {
            Ok((steps, collect_ids)) => {
                let amendment = collect_ids && matches!(steps.first(), Some(Step::Cancel { .. }));
                let amended = steps.first().and_then(|step| match step {
                    Step::Cancel { id, .. } if collect_ids => {
                        cx.store.order(id).map(|o| (o.side, o.kind))
                    }
                    _ => None,
                });
                let preflight = preflight.as_ref().unwrap();
                let (write_plan, elements) = write_plan(&steps);
                if steps.is_empty() {
                    let _ = cx.done_empty_plan(preflight, "[]".into());
                    return;
                }
                let admitted = if protect.is_some() {
                    self.admit_serial(cx, preflight)
                } else {
                    self.admit_trade(cx, preflight, &steps, continue_rows)
                };
                let plan = match admitted {
                    Ok(plan) => plan,
                    Err(error) => {
                        let mut error = error.venue_error();
                        error = write_plan.attach(error);
                        let _ = cx.fail_preflight(preflight, error);
                        return;
                    }
                };
                self.trading.jobs.insert(
                    u64::from(id),
                    Job {
                        managed: Some(plan),
                        write_plan,
                        steps,
                        elements,
                        next: 0,
                        continue_rows,
                        skip_children: false,
                        failure: None,
                        waiting: false,
                        cancel_latches: vec![],
                        ids: vec![],
                        collect_ids,
                        amendment,
                        amended,
                        protect,
                        refused: None,
                        restoring: false,
                    },
                );
                self.run_trade(cx, u64::from(id));
            }
            Err(e) => {
                let _ = cx.fail_preflight(
                    preflight.as_ref().unwrap(),
                    not_sent(e).kind(ErrorKind::InvalidInput),
                );
            }
        }
    }
    fn admit_serial(
        &self,
        cx: &mut Cx<'_, Pending>,
        preflight: &Preflight,
    ) -> Result<Plan, safe_cex_engine::request_context::Error> {
        cx.admit_serial_plan(
            preflight,
            mutation_call(),
            SerialPolicy {
                credential: self.options.identity.as_ref().map(|identity| Credential {
                    slot: safe_cex_engine::CredentialSlot::Primary,
                    reference: identity.credential.clone(),
                }),
                pacing: None,
                refusals_complete: false,
            },
        )
        .map(|(plan, _)| plan)
    }
    fn admit_trade(
        &self,
        cx: &mut Cx<'_, Pending>,
        preflight: &Preflight,
        steps: &[Step],
        independent_calls: bool,
    ) -> Result<Plan, safe_cex_engine::request_context::Error> {
        let calls: Vec<_> = steps
            .iter()
            .map(|_| CallBound {
                timeout_ms: 20000,
                attempts: 1,
                retry_window: RetryWindow::Shared,
                after_refusal: &[],
            })
            .collect();
        let bounds: Vec<_> = calls
            .iter()
            .map(|call| StepBound {
                wait_ms: 0,
                calls: std::slice::from_ref(call),
            })
            .collect();
        cx.admit_plan_with_terminals(
            preflight,
            &bounds,
            PlanPolicy {
                independent_calls,
                operation: Operation::Mutation,
                credential: self.options.identity.as_ref().map(|identity| Credential {
                    slot: safe_cex_engine::CredentialSlot::Primary,
                    reference: identity.credential.clone(),
                }),
                pacing: None,
                retry: RetryPolicy::Never,
            },
            &[],
        )
    }

    pub(crate) fn trade_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        let command = self
            .trading
            .jobs
            .iter()
            .find(|(_, job)| {
                job.managed
                    .as_ref()
                    .is_some_and(|own| own.owner() == plan.owner())
            })
            .map(|(id, _)| *id);
        if let Some(command) = command {
            self.run_trade(cx, command);
        }
    }

    fn run_trade(&mut self, cx: &mut Cx<'_, Pending>, command: u64) {
        let Some(job) = self.trading.jobs.get_mut(&command) else {
            return;
        };
        if job.waiting {
            return;
        }
        let complete = job.next == job.steps.len();
        if complete && let Some(plan) = &job.managed {
            if let Some(error) = job.failure.take() {
                let error = job.write_plan.attach(error);
                let _ = cx.fail_plan(plan, error);
                return;
            }
            let result = if job.collect_ids {
                serde_json::to_string(&job.ids).unwrap()
            } else {
                "[]".into()
            };
            let _ = cx.done_plan(plan, result);
            return;
        }
        let Some(plan) = &job.managed else { return };
        let Some(slot) = cx
            .plan_slots(plan)
            .ok()
            .and_then(|slots| slots.into_iter().next())
        else {
            return;
        };
        let write = job.next;
        let Some(step) = job.steps.get(write).cloned() else {
            return;
        };
        job.next += 1;
        if job.skip_children && matches!(step, Step::Place { optional: true, .. }) {
            if let Err(reason) = cx.retire_unissued_plan_call(plan, &slot) {
                let error = job.write_plan.attach(reason.venue_error());
                let _ = cx.fail_plan(plan, error);
                return;
            }
            self.run_trade(cx, command);
            return;
        }
        job.skip_children = false;
        let intent = match &step {
            Step::Place { .. } | Step::Pair(..) => Intent::Place,
            Step::Cancel { .. } if job.amendment => Intent::Amend,
            Step::Cancel { .. } | Step::CancelAll { .. } => Intent::Cancel,
            Step::Leverage(..) => Intent::Setting,
            Step::OpenOrders { .. } => Intent::Read,
        };
        let context = match if matches!(step, Step::OpenOrders { .. }) {
            cx.plan_leg_context(plan, &slot)
        } else {
            cx.plan_leg_context_with_intent(plan, &slot, intent)
        } {
            Ok(context) => context,
            Err(error) => {
                let error = error.venue_error().outcome(Outcome::NotSent);
                record_failure(&mut job.write_plan, &job.elements[write], &error);
                let error = job.write_plan.attach(error);
                let _ = cx.fail_plan(plan, error);
                return;
            }
        };
        record_failure(
            &mut job.write_plan,
            &job.elements[write],
            &VenueError::new("Backpack write awaiting response").outcome(Outcome::Unknown),
        );
        job.waiting = true;
        let managed = ManagedWrite {
            plan: plan.clone(),
            slot,
            context,
        };
        let mut previous = None;
        let mut targets = vec![];
        {
            let receipts = match &step {
                Step::Place { order, .. } => order.identity(cx.store).into_iter().collect(),
                Step::Pair(orders) => orders
                    .iter()
                    .filter_map(|order| order.identity(cx.store))
                    .collect(),
                _ => vec![],
            };
            cx.command_progress(command as CommandId, receipts);
        }
        let (method, path, params, body) = match &step {
            Step::Place { order, .. } => (
                Method::Post,
                "/api/v1/order",
                order.params(),
                serde_json::to_string(order).unwrap(),
            ),
            Step::Pair(rows) => {
                let params = rows.iter().map(NativeOrder::params).collect();
                let body = serde_json::to_string(rows).unwrap();
                let delivery = Request {
                    managed: Some(managed),
                    write,
                    command,
                    step,
                    previous,
                    targets,
                    leverage_revision: self.account.leverage_revision,
                };
                self.send_managed_trade(cx, delivery, Method::Post, "/api/v1/orders", params, body);
                return;
            }
            Step::Cancel { symbol, id, .. } => {
                cx.store.invalidate_order_snapshots();
                previous = cx.store.order(id).map(|o| o.status);
                if let Some(order) = cx.store.order(id) {
                    job.cancel_latches.push(id.clone());
                    targets.push((id.clone(), self.order_stream_time(id)));
                    let state = self
                        .trading
                        .cancels
                        .entry(id.clone())
                        .or_insert((0, order.status));
                    state.0 += 1;
                    previous = Some(state.1);
                }
                if cx.store.order(id).is_some_and(|o| {
                    !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                }) {
                    cx.store.set_order_status(id, OrderStatus::Canceling);
                }
                let p: Params = [
                    ("symbol".into(), symbol.clone()),
                    ("orderId".into(), id.clone()),
                ]
                .into();
                (
                    Method::Delete,
                    "/api/v1/order",
                    p.clone(),
                    serde_json::to_string(&p).unwrap(),
                )
            }
            Step::CancelAll { symbol } => {
                cx.store.invalidate_order_snapshots();
                targets = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        o.symbol == *symbol
                            && !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                    })
                    .map(|o| (o.id.clone(), self.order_stream_time(&o.id)))
                    .collect();
                let p: Params = [("symbol".into(), symbol.clone())].into();
                (
                    Method::Delete,
                    "/api/v1/orders",
                    p.clone(),
                    serde_json::to_string(&p).unwrap(),
                )
            }
            Step::OpenOrders { symbol } => {
                let params: Params = [("symbol".into(), symbol.clone())].into();
                let delivery = Request {
                    managed: Some(managed),
                    write,
                    command,
                    step,
                    previous,
                    targets,
                    leverage_revision: self.account.leverage_revision,
                };
                self.send_managed_read(cx, delivery, params);
                return;
            }
            Step::Leverage(leverage) => {
                self.account.leverage_revision += 1;
                let p: Params = [("leverageLimit".into(), num::format(*leverage))].into();
                (
                    Method::Patch,
                    "/api/v1/account",
                    p.clone(),
                    serde_json::to_string(&p).unwrap(),
                )
            }
        };
        let delivery = Request {
            managed: Some(managed),
            write,
            command,
            step,
            previous,
            targets,
            leverage_revision: self.account.leverage_revision,
        };
        self.send_managed_trade(cx, delivery, method, path, vec![params], body);
    }

    fn send_managed_read(&mut self, cx: &mut Cx<'_, Pending>, delivery: Request, params: Params) {
        let identity = self
            .options
            .identity
            .as_ref()
            .expect("admitted identity")
            .clone();
        let query = crate::sign::query(&params);
        let recipe = crate::sign::Http::new(
            identity,
            crate::sign::Request {
                method: Method::Get,
                path: "/api/v1/orders".into(),
                rows: vec![params],
                body: None,
                window: self
                    .options
                    .recv_window
                    .filter(|window| window.is_finite() && *window > 0.)
                    .map_or(60000, |window| window.min(60000.) as u32),
            },
        );
        if let Err((Pending::Trade(delivery), error)) = cx.request_signed_managed(
            delivery.managed.as_ref().unwrap().context.clone(),
            None,
            Method::Get,
            format!("{REST}/api/v1/orders?{query}"),
            None,
            Box::new(recipe),
            Pending::Trade(delivery),
        ) {
            self.trade_failed(cx, delivery, error.venue_error());
        }
    }
    fn send_managed_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        delivery: Request,
        method: Method,
        path: &str,
        rows: Vec<Params>,
        body: String,
    ) {
        let identity = self
            .options
            .identity
            .as_ref()
            .expect("admitted identity")
            .clone();
        let window = self
            .options
            .recv_window
            .filter(|window| window.is_finite() && *window > 0.)
            .map_or(60000, |window| window.min(60000.) as u32);
        let recipe = crate::sign::Http::new(
            identity,
            crate::sign::Request {
                method,
                path: path.into(),
                rows,
                body: Some(body.clone()),
                window,
            },
        );
        if let Err((Pending::Trade(delivery), error)) = cx.request_signed_managed(
            delivery.managed.as_ref().unwrap().context.clone(),
            None,
            method,
            format!("{REST}{path}"),
            Some(body),
            Box::new(recipe),
            Pending::Trade(delivery),
        ) {
            let error = error.venue_error().outcome(Outcome::NotSent);
            self.trade_failed(cx, delivery, error);
        }
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let mut recorded = false;
        if let Some(managed) = &request.managed {
            if cx
                .validate_plan_delivery(&managed.plan, &managed.context)
                .is_err()
            {
                return;
            }
            if let Some(job) = self.trading.jobs.get_mut(&request.command) {
                job.waiting = false;
            }
            if request.step.reads_proof() {
                return self.proof_response(cx, request, response);
            }
            if let Step::Pair(orders) = &request.step
                && (200..300).contains(&response.status)
                && !refused_in_body(response.body)
            {
                match pair_fact(cx, managed, orders, response.body) {
                    Ok((result, elements)) => {
                        self.record_write(&request, elements);
                        match result {
                            WriteResult::Accepted { .. } => recorded = true,
                            WriteResult::Rejected(error)
                            | WriteResult::KnownPartial(error)
                            | WriteResult::PartialUnknown(error)
                            | WriteResult::Failed(error) => {
                                if self.continue_reported_failure(
                                    cx,
                                    &request,
                                    error.clone(),
                                    false,
                                    true,
                                ) {
                                    return;
                                }
                                let error = self.plan_failure(cx, &request, error, false);
                                let _ = cx.fail_plan(&managed.plan, error);
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        let error = self.plan_failure(cx, &request, error.venue_error(), true);
                        let _ = cx.fail_plan(&managed.plan, error);
                        return;
                    }
                }
            }
        }
        let result = (|| {
            if !(200..300).contains(&response.status) {
                return Err(format!(
                    "Backpack HTTP {}: {}",
                    response.status, response.body
                ));
            }
            // Some API errors are returned with HTTP 200.
            if refused_in_body(response.body) {
                return Err(format!("Backpack write rejected: {}", response.body));
            }
            match &request.step {
                Step::Place { .. } => {
                    #[derive(Deserialize)]
                    struct Placed<'a> {
                        #[serde(borrow)]
                        id: &'a str,
                    }
                    let row: Placed<'_> =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    if row.id.trim().is_empty() {
                        return Err("Backpack placement missing order ID".into());
                    }
                    accept_trade(cx, &request, vec![row.id.into()])?;
                    recorded = request.managed.is_some();
                    self.record_write(
                        &request,
                        vec![ElementResult::Placed {
                            order_ids: vec![row.id.into()],
                        }],
                    );
                    cx.store.invalidate_order_snapshots();
                    if let Some(job) = self.trading.jobs.get_mut(&request.command) {
                        job.ids.push(row.id.into());
                    }
                }
                Step::Pair(orders) => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Row<'a> {
                        #[serde(borrow)]
                        operation: &'a str,
                        #[serde(borrow)]
                        id: &'a str,
                        client_id: u32,
                        #[serde(borrow)]
                        symbol: &'a str,
                        #[serde(borrow)]
                        side: &'a str,
                        #[serde(borrow)]
                        order_type: &'a str,
                    }
                    let rows: Vec<Row<'_>> =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    if rows.len() != orders.len() {
                        return Err("Backpack batch response count mismatch".into());
                    }
                    let mut ids = HashMap::new();
                    let mut unique = HashSet::new();
                    for r in rows {
                        let expected = orders
                            .iter()
                            .find(|o| o.client_id == Some(r.client_id))
                            .ok_or("Backpack unknown batch clientId")?;
                        if r.operation != "Ok"
                            || r.id.trim().is_empty()
                            || expected.symbol != r.symbol
                            || expected.side != r.side
                            || expected.order_type != r.order_type
                            || ids.insert(r.client_id, r.id.to_owned()).is_some()
                            || !unique.insert(r.id)
                        {
                            return Err("Backpack invalid batch response identity".into());
                        }
                    }
                    // Only a fully validated batch acknowledgement supersedes reads.
                    cx.store.invalidate_order_snapshots();
                    if let Some(job) = self.trading.jobs.get_mut(&request.command) {
                        for o in orders {
                            let client = o.client_id.ok_or("Backpack missing planned client ID")?;
                            job.ids
                                .push(ids.remove(&client).ok_or("Backpack missing batch ID")?);
                        }
                    }
                }
                Step::Cancel { id, .. } => {
                    if let Some(protect) = self
                        .trading
                        .jobs
                        .get_mut(&request.command)
                        .and_then(|job| job.protect.as_mut())
                    {
                        #[derive(Deserialize)]
                        #[serde(rename_all = "camelCase")]
                        struct Proof {
                            status: String,
                            executed_quantity: crate::wire::Number,
                        }
                        protect.restorable &= serde_json::from_str::<Proof>(response.body)
                            .is_ok_and(|proof| {
                                proof.status == "Cancelled"
                                    && proof.executed_quantity.0 <= protect.original.filled
                            });
                    }
                    validate_ack(response.body)?;
                    accept_trade(cx, &request, vec![])?;
                    recorded = request.managed.is_some();
                    self.record_write(
                        &request,
                        vec![ElementResult::Applied {
                            effect: AppliedEffect::Cancel,
                            target_order_id: Some(id.clone()),
                            position_key: None,
                        }],
                    );
                    cx.store.invalidate_order_snapshots();
                    self.finish_trade_cancel(request.command, id);
                    let unchanged = request
                        .targets
                        .first()
                        .is_some_and(|(_, stamp)| *stamp == self.order_stream_time(id));
                    cx.store.update_order(id, |o| {
                        if unchanged
                            && !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                        {
                            o.status = OrderStatus::Canceled;
                        }
                    });
                }
                Step::CancelAll { .. } => {
                    if request.managed.is_some() {
                        validate_ack(response.body)?;
                    }
                    accept_trade(cx, &request, vec![])?;
                    recorded = request.managed.is_some();
                    self.record_write(
                        &request,
                        vec![ElementResult::Applied {
                            effect: AppliedEffect::Cancel,
                            target_order_id: None,
                            position_key: None,
                        }],
                    );
                    cx.store.invalidate_order_snapshots();
                    for (id, stamp) in &request.targets {
                        let unchanged = self.order_stream_time(id) == *stamp;
                        cx.store.update_order(id, |o| {
                            if unchanged
                                && !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                            {
                                o.status = OrderStatus::Canceled;
                            }
                        });
                    }
                }
                Step::OpenOrders { .. } => unreachable!("proof reads are answered apart"),
                Step::Leverage(leverage) => {
                    // Documented: PATCH /api/v1/account answers 200 with no content.
                    if request.managed.is_some() && !response.body.trim().is_empty() {
                        validate_ack(response.body)?;
                    }
                    accept_trade(cx, &request, vec![])?;
                    recorded = request.managed.is_some();
                    self.record_write(
                        &request,
                        vec![ElementResult::Applied {
                            effect: AppliedEffect::Settings,
                            target_order_id: None,
                            position_key: None,
                        }],
                    );
                    if request.leverage_revision == self.account.leverage_revision {
                        self.account.leverage_revision += 1;
                        self.account.leverage = *leverage;
                        self.account.positions_revision += 1;
                        self.leverage_settings(cx);
                        cx.store.update_positions(|p| p.leverage = *leverage);
                        self.positions_read(cx, Next::None);
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                if self
                    .trading
                    .jobs
                    .get(&request.command)
                    .is_some_and(|job| job.protect.is_some())
                {
                    return self.continue_guarded(cx, &request);
                }
                if let Some(managed) = &request.managed {
                    if let Err(error) =
                        cx.finish_plan_call(&managed.plan, &managed.slot, &managed.context)
                    {
                        let error = self.plan_failure(cx, &request, error.venue_error(), false);
                        let _ = cx.fail_plan(&managed.plan, error);
                        return;
                    }
                    self.run_trade(cx, request.command);
                    return;
                }
                self.run_trade(cx, request.command)
            }
            Err(e) => {
                if let Some(managed) = &request.managed {
                    if recorded {
                        // An amend's cancel landed but its remainder cannot be re-placed.
                        let gone = match &request.step {
                            Step::Cancel { id, .. }
                                if self
                                    .trading
                                    .jobs
                                    .get(&request.command)
                                    .is_some_and(|job| job.amendment) =>
                            {
                                cx.store
                                    .order(id)
                                    .map(|o| gone_text(&o.symbol, o.side, o.kind, false))
                            }
                            _ => None,
                        };
                        let error = self.plan_failure(cx, &request, VenueError::new(e), false);
                        let _ = match &gone {
                            Some(gone) => cx.fail_plan_noted(&managed.plan, error, gone),
                            None => cx.fail_plan(&managed.plan, error),
                        };
                        return;
                    }
                    let unproven =
                        (200..300).contains(&response.status) && !refused_in_body(response.body);
                    let amend_refusal = !unproven
                        && request.step.places()
                        && definite_refusal(response.status, response.body)
                        && self
                            .trading
                            .jobs
                            .get(&request.command)
                            .is_some_and(|job| job.protect.is_some() && job.ids.is_empty());
                    let fact = if unproven {
                        let error =
                            with_unknown_guidance(VenueError::new(e).outcome(Outcome::Unknown));
                        cx.http_write_unproven(&managed.context, &error.message)
                            .map(|()| error)
                    } else {
                        native_failure(
                            cx,
                            &managed.context,
                            if amend_refusal {
                                &AMEND_REFUSAL
                            } else {
                                &BACKPACK
                            },
                            response.body,
                            &e,
                        )
                    };
                    let error = match fact {
                        Ok(error) => error,
                        Err(error) => {
                            let error = self.plan_failure(cx, &request, error.venue_error(), true);
                            let _ = cx.fail_plan(&managed.plan, error);
                            return;
                        }
                    };
                    if amend_refusal
                        && let Step::Place { order, .. } = &request.step
                        && let Some(job) = self.trading.jobs.get_mut(&request.command)
                    {
                        job.refused = Some(order.clone());
                    }
                    self.managed_trade_failed(cx, request, error, true);
                    return;
                }
                let error = if !(200..300).contains(&response.status) {
                    http_failure(response.status, e)
                } else if refused_in_body(response.body) {
                    // Backpack's error codes are undocumented here; leave the outcome unset.
                    VenueError::new(e)
                } else {
                    // The write was answered with an acknowledgement that does not prove it.
                    VenueError::new(e).outcome(Outcome::Unknown)
                };
                self.trade_failed(cx, request, error)
            }
        }
    }
    pub(crate) fn trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        if request.managed.is_some() {
            self.managed_trade_failed(cx, request, error, false);
            return;
        }
        let error = with_unknown_guidance(error);
        if let Step::Cancel { id, .. } = &request.step
            && self.finish_trade_cancel(request.command, id)
            && let Some(previous) = request.previous
        {
            let unchanged = request
                .targets
                .first()
                .is_some_and(|(_, stamp)| *stamp == self.order_stream_time(id));
            cx.store.update_order(id, |o| {
                if o.status == OrderStatus::Canceling && unchanged {
                    o.status = previous;
                }
            });
        }
        self.trading.jobs.remove(&request.command);
        cx.notice(Level::Warn, code::ORDER_REJECTED, &error.message);
    }

    fn continue_guarded(&mut self, cx: &mut Cx<'_, Pending>, request: &Request) {
        let managed = request.managed.as_ref().expect("a streamed amend");
        let job = self
            .trading
            .jobs
            .get(&request.command)
            .expect("an active amend");
        let protect = job.protect.as_ref().expect("a guarded amend");
        let symbol = protect.original.symbol.clone();
        let what = what(protect.original.kind);
        if let Some(next) = job.steps.get(job.next).filter(|_| !job.restoring) {
            match cx.continue_serial_plan(
                &managed.plan,
                &managed.slot,
                &managed.context,
                serial_call(next),
            ) {
                Ok(_) => self.run_trade(cx, request.command),
                Err(reason) => {
                    let _ = cx.fail_plan(&managed.plan, reason.venue_error());
                }
            }
            return;
        }
        let _ = cx.finish_plan_call(&managed.plan, &managed.slot, &managed.context);
        if job.restoring {
            let mut error = VenueError::new(format!(
                "Your {symbol} {what} is unchanged: its replacement was refused, so the original was put back as a new order"
            ));
            error.protection = Some(safe_cex_engine::error::Protection::Restored);
            error.presents = true;
            let _ = cx.fail_plan(
                &managed.plan,
                job.write_plan.attach(error.partial_ids(&job.ids)),
            );
        } else {
            let _ = cx.done_plan(&managed.plan, serde_json::to_string(&job.ids).unwrap());
        }
    }

    fn guarded_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
        data: bool,
    ) {
        let managed = request.managed.as_ref().expect("a streamed amend");
        let job = self
            .trading
            .jobs
            .get(&request.command)
            .expect("an active amend");
        let protect = job.protect.as_ref().expect("a guarded amend");
        let (symbol, side, kind) = (
            protect.original.symbol.clone(),
            protect.original.side,
            protect.original.kind,
        );
        let what = what(kind);
        let refused = job.refused.is_some();
        // A definite refusal is put back only once an open-orders read shows
        // nothing like the replacement resting.
        if request.step.places()
            && refused
            && !job.restoring
            && job.ids.is_empty()
            && protect.restorable
            && data
        {
            let next = Step::OpenOrders {
                symbol: protect.original.symbol.clone(),
            };
            let call = serial_call(&next);
            if cx
                .continue_serial_plan(&managed.plan, &managed.slot, &managed.context, call)
                .is_ok()
            {
                let job = self.trading.jobs.get_mut(&request.command).unwrap();
                append_step(job, next);
                return self.run_trade(cx, request.command);
            }
        }
        let mut error = error;
        error.presents = true;
        let removed = |detail: &str| {
            format!("Your {symbol} {what} was removed: {detail}, please re-place it")
        };
        let (message, protection) = match (&request.step, job.restoring, refused) {
            (Step::OpenOrders { .. }, ..) => (
                removed("its replacement was refused and nothing could prove it absent"),
                Some(safe_cex_engine::error::Protection::Unprotected),
            ),
            (_, true, true) => (
                removed("its replacement and the put-back were both refused"),
                Some(safe_cex_engine::error::Protection::Unprotected),
            ),
            (_, true, false) => (
                format!(
                    "Your {symbol} {what}'s replacement was refused and its put-back's outcome is unknown. Check Open Orders"
                ),
                None,
            ),
            (_, false, true) => (
                removed("its replacement was refused"),
                Some(safe_cex_engine::error::Protection::Unprotected),
            ),
            (_, false, false) => (gone_text(&symbol, side, kind, true), None),
        };
        error.message = format!("{message} ({})", error.message);
        error.protection = protection;
        if data {
            let _ = cx.finish_plan_call(&managed.plan, &managed.slot, &managed.context);
        }
        let _ = cx.fail_plan(&managed.plan, job.write_plan.attach(error));
    }

    fn proof_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Open {
            #[serde(default)]
            client_id: Option<u32>,
            #[serde(default)]
            side: String,
            #[serde(default)]
            trigger_price: Option<crate::wire::Number>,
            #[serde(default)]
            trigger_quantity: Option<crate::wire::Number>,
        }
        let rows = (200..300)
            .contains(&response.status)
            .then(|| serde_json::from_str::<Vec<Open>>(response.body).ok())
            .flatten();
        let Some(rows) = rows else {
            let error = VenueError::new("Backpack open-orders proof read failed");
            return self.guarded_failed(cx, request, error, true);
        };
        let job = self
            .trading
            .jobs
            .get(&request.command)
            .expect("an active amend");
        let replacement = job.refused.as_ref().expect("a refused replacement");
        let number = |text: &Option<String>| text.as_deref().and_then(|t| t.parse::<f64>().ok());
        let resting = rows.iter().any(|row| match replacement.client_id {
            Some(id) => row.client_id == Some(id),
            None => {
                row.side == replacement.side
                    && row.trigger_price.map(|n| n.0) == number(&replacement.trigger_price)
                    && row.trigger_quantity.map(|n| n.0) == number(&replacement.trigger_quantity)
            }
        });
        let managed = request.managed.as_ref().expect("a streamed amend");
        if resting {
            let protect = job.protect.as_ref().expect("a guarded amend");
            let order = &protect.original;
            let mut error = VenueError::new(gone_text(&order.symbol, order.side, order.kind, true));
            error.presents = true;
            let _ = cx.finish_plan_call(&managed.plan, &managed.slot, &managed.context);
            let _ = cx.fail_plan(&managed.plan, job.write_plan.attach(error));
            return;
        }
        let protect = job.protect.as_ref().expect("a guarded amend");
        let restore = Step::Place {
            order: protect.restore.clone(),
            optional: false,
        };
        // A 408 or 5xx on the put-back is sealed before this venue sees it:
        // say what is unknown rather than repeat the replacement's refusal.
        let words = format!(
            "Your {} {}'s replacement was refused and its put-back's outcome is unknown. Check Open Orders",
            protect.original.symbol,
            what(protect.original.kind)
        );
        if let Err(reason) = cx.present_plan_failure(&managed.plan, words) {
            let error = VenueError::new(reason.venue_error().message);
            return self.guarded_failed(cx, request, error, true);
        }
        if let Err(reason) = cx.continue_serial_plan(
            &managed.plan,
            &managed.slot,
            &managed.context,
            serial_call(&restore),
        ) {
            let error = VenueError::new(reason.venue_error().message);
            return self.guarded_failed(cx, request, error, true);
        }
        let job = self.trading.jobs.get_mut(&request.command).unwrap();
        append_step(job, restore);
        job.restoring = true;
        // The put-back is judged by its own answer, not the replacement's.
        job.refused = None;
        self.run_trade(cx, request.command);
    }

    fn record_write(&mut self, request: &Request, results: Vec<ElementResult>) {
        let Some(job) = self.trading.jobs.get_mut(&request.command) else {
            return;
        };
        let Some(indices) = job.elements.get(request.write) else {
            return;
        };
        for (index, mut result) in indices.iter().zip(results) {
            if let ElementResult::Unknown { error } = &mut result {
                error.message = UNKNOWN_TEXT.into();
            }
            let _ = job.write_plan.record(*index, result);
        }
    }

    fn plan_failure(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        mut error: VenueError,
        pending: bool,
    ) -> VenueError {
        error = with_unknown_guidance(error);
        if let Some(job) = self.trading.jobs.get_mut(&request.command) {
            if pending {
                record_failure(&mut job.write_plan, &job.elements[request.write], &error);
            }
            error = job.write_plan.attach(error);
        }
        if error.outcome == Some(Outcome::Unknown) {
            if let Some(managed) = &request.managed {
                // The venue path has already composed its own user-facing
                // message for an unknown write; keep it verbatim. The local
                // outcome gate here is intentional and must not be dropped.
                let _ = cx.override_plan_failure_message(&managed.plan, &error.message);
            }
            cx.notice(Level::Warn, "backpack.write.unknown", &error.message);
        }
        error
    }

    pub(crate) fn trade_unproven_evidence(
        &self,
        request: &Request,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        request.unproven_evidence(response, evidence);
        if let Some(plan) = self
            .trading
            .jobs
            .get(&request.command)
            .map(|job| &job.write_plan)
        {
            evidence.write_plan(plan);
        }
    }

    fn managed_trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
        data: bool,
    ) {
        let mut error = error;
        if !data && error.outcome.is_none() {
            error.outcome = Some(Outcome::Unknown);
        }
        error = with_unknown_guidance(error);
        let managed = request.managed.as_ref().expect("foreground delivery");
        let optional = matches!(
            request.step,
            Step::Place { optional: true, .. } | Step::Cancel { optional: true, .. }
        );
        if let Step::Cancel { id, .. } = &request.step {
            let last = self.finish_trade_cancel(request.command, id);
            if last
                && error.outcome == Some(Outcome::NotSent)
                && let Some(previous) = request.previous
                && request
                    .targets
                    .first()
                    .is_some_and(|(_, stamp)| *stamp == self.order_stream_time(id))
            {
                cx.store.update_order(id, |order| {
                    if order.status == OrderStatus::Canceling {
                        order.status = previous;
                    }
                });
            }
        }
        if self
            .trading
            .jobs
            .get(&request.command)
            .is_some_and(|job| job.protect.is_some())
            && (request.step.places() || request.step.reads_proof())
        {
            if let Some(job) = self.trading.jobs.get_mut(&request.command) {
                record_failure(&mut job.write_plan, &job.elements[request.write], &error);
            }
            return self.guarded_failed(cx, request, error, data);
        }
        let gone = match &request.step {
            Step::Place { order, .. } => self
                .trading
                .jobs
                .get(&request.command)
                .filter(|job| job.ids.is_empty())
                .and_then(|job| job.amended)
                .map(|(side, kind)| {
                    gone_text(
                        &order.symbol,
                        side,
                        kind,
                        error.outcome != Some(Outcome::NotSent),
                    )
                }),
            _ => None,
        };
        if optional {
            cx.notice(Level::Error, code::CHILD_LEG_FAILED, &error.message);
        }
        if self.continue_reported_failure(cx, &request, error.clone(), true, data) {
            return;
        }
        let error = self.plan_failure(cx, &request, error, true);
        let _ = match &gone {
            Some(gone) => cx.fail_plan_noted(&managed.plan, error, gone),
            None => cx.fail_plan(&managed.plan, error),
        };
    }

    fn continue_reported_failure(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        error: VenueError,
        pending: bool,
        data: bool,
    ) -> bool {
        if !matches!(error.outcome, Some(Outcome::NotSent | Outcome::Rejected)) {
            return false;
        }
        let Some(job) = self
            .trading
            .jobs
            .get_mut(&request.command)
            .filter(|job| job.continue_rows)
        else {
            return false;
        };
        let Some(managed) = &request.managed else {
            return false;
        };
        job.waiting = false;
        if pending {
            record_failure(&mut job.write_plan, &job.elements[request.write], &error);
        }
        job.skip_children = request.step.is_entry();
        if job.failure.is_none() {
            job.failure = Some(error);
        }
        let finished = if data {
            cx.finish_plan_call(&managed.plan, &managed.slot, &managed.context)
        } else {
            cx.finish_plan_failed_call(&managed.plan, &managed.slot, &managed.context)
        };
        if let Err(reason) = finished {
            let error = job.write_plan.attach(reason.venue_error());
            let _ = cx.fail_plan(&managed.plan, error);
            return true;
        }
        self.run_trade(cx, request.command);
        true
    }

    pub(crate) fn trade_retired(&mut self, owner: &HttpOwnerRetirement) {
        let command = self
            .trading
            .jobs
            .iter()
            .find(|(_, job)| {
                job.managed
                    .as_ref()
                    .is_some_and(|plan| owner.matches_plan(plan))
            })
            .map(|(command, _)| *command);
        if let Some(command) = command
            && let Some(job) = self.trading.jobs.remove(&command)
        {
            for id in job.cancel_latches {
                self.finish_cancel(&id);
            }
        }
    }

    fn finish_trade_cancel(&mut self, command: u64, id: &str) -> bool {
        if let Some(job) = self.trading.jobs.get_mut(&command)
            && let Some(index) = job.cancel_latches.iter().position(|own| own == id)
        {
            job.cancel_latches.remove(index);
            return self.finish_cancel(id);
        }
        false
    }

    fn finish_cancel(&mut self, id: &str) -> bool {
        let Some(state) = self.trading.cancels.get_mut(id) else {
            return false;
        };
        state.0 -= 1;
        if state.0 == 0 {
            self.trading.cancels.remove(id);
            true
        } else {
            false
        }
    }
}

impl Backpack {
    pub(crate) fn trigger_repair(
        &self,
        cx: &Cx<'_, Pending>,
        row: &crate::wire::OrderRow<'_>,
        proof: Option<f64>,
    ) -> Result<Repair, String> {
        if matches!(
            crate::account::status(&row.status),
            OrderStatus::Closed | OrderStatus::Canceled
        ) {
            return Err("Backpack repair order is already terminal".into());
        }
        let market = cx
            .store
            .market(&row.symbol)
            .ok_or("Backpack repair market missing")?;
        let quantity = row.base_quantity()?;
        let amount = row.trigger_quantity.as_ref().map_or(quantity, |n| n.0);
        let trigger = formatted(
            row.trigger_price
                .as_ref()
                .ok_or("Backpack repair trigger missing")?
                .0,
            market.precision.price,
        )?;
        let quantity = formatted(amount, market.precision.amount)?;
        let limit = row.order_type.eq_ignore_ascii_case("Limit");
        let price = if limit {
            Some(formatted(row.price, market.precision.price)?)
        } else {
            None
        };
        Ok(Repair {
            amount,
            high_water: proof,
            uncertain: proof.is_none(),
            terminal: false,
            old_id: row.id.to_string(),
            order: NativeOrder {
                symbol: row.symbol.to_string(),
                side: if row.side == "Bid" { "Bid" } else { "Ask" },
                order_type: if limit { "Limit" } else { "Market" },
                price,
                trigger_price: Some(trigger),
                trigger_quantity: Some(quantity),
                trigger_by: Some("MarkPrice"),
                reduce_only: row.reduce_only,
                post_only: limit.then_some(row.post_only || row.time_in_force == "PO"),
                time_in_force: limit.then_some(match row.time_in_force.as_ref() {
                    "IOC" => "IOC",
                    "FOK" => "FOK",
                    _ => "GTC",
                }),
                ..Default::default()
            },
        })
    }
}

/// Backpack answers some refused writes with HTTP 200 and an error body.
fn refused_in_body(body: &str) -> bool {
    #[derive(Deserialize)]
    struct Failure<'a> {
        #[serde(borrow)]
        error: Option<&'a serde_json::value::RawValue>,
        #[serde(borrow)]
        code: Option<&'a serde_json::value::RawValue>,
    }
    body.trim_start().starts_with('{')
        && serde_json::from_str::<Failure<'_>>(body)
            .is_ok_and(|error| error.error.is_some() || error.code.is_some())
}

/// A rejected HTTP write: 408 and 5xx do not prove the venue refused it.
pub(crate) fn http_failure(status: u16, message: String) -> VenueError {
    let error = VenueError::new(message);
    with_unknown_guidance(match status {
        408 | 500.. => error.outcome(Outcome::Unknown),
        401 => error
            .kind(ErrorKind::AuthInvalid)
            .outcome(Outcome::Rejected),
        403 => error
            .kind(ErrorKind::AuthPermission)
            .outcome(Outcome::Rejected),
        429 => error
            .kind(ErrorKind::RateLimited)
            .outcome(Outcome::Rejected),
        _ => error.kind(ErrorKind::Rejected).outcome(Outcome::Rejected),
    })
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

fn validate_ack(body: &str) -> Result<(), String> {
    if !body.trim_start().starts_with(['{', '[']) {
        return Err("Backpack invalid write acknowledgement".into());
    }
    serde_json::from_str::<serde::de::IgnoredAny>(body)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn accept_trade(
    cx: &mut Cx<'_, Pending>,
    request: &Request,
    ids: Vec<String>,
) -> Result<(), String> {
    if let Some(managed) = &request.managed {
        cx.http_write_fact(
            &managed.context,
            WriteFact::Accepted { order_ids: ids },
            None,
        )
        .map_err(|error| error.venue_error().message)?;
    }
    Ok(())
}

fn native_failure(
    cx: &mut Cx<'_, Pending>,
    context: &Context,
    table: &CodeTable,
    body: &str,
    message: &str,
) -> Result<VenueError, safe_cex_engine::request_context::Error> {
    #[derive(Deserialize)]
    struct Failure<'a> {
        #[serde(borrow)]
        code: Option<&'a serde_json::value::RawValue>,
        #[serde(borrow)]
        message: Option<&'a str>,
        #[serde(borrow)]
        error: Option<&'a serde_json::value::RawValue>,
    }
    let failure = serde_json::from_str::<Failure<'_>>(body).ok();
    let raw = failure.as_ref().and_then(|failure| failure.code);
    let text = raw.and_then(|raw| serde_json::from_str::<String>(raw.get()).ok());
    let error_text = failure
        .as_ref()
        .and_then(|failure| failure.error)
        .and_then(|raw| serde_json::from_str::<String>(raw.get()).ok());
    let code = raw
        .and_then(|raw| safe_cex_engine::classify::decode_int(raw.get()))
        .map(NativeCode::Int)
        .or_else(|| text.as_deref().map(NativeCode::Text))
        .unwrap_or(NativeCode::None);
    let native = Native {
        code,
        message: failure
            .as_ref()
            .and_then(|failure| failure.message)
            .or(error_text.as_deref())
            .unwrap_or(""),
    };
    let fact = cx.classify_write(context, table, Some(&native), message)?;
    let error = with_unknown_guidance(fact.error().clone());
    cx.http_write_fact(
        context,
        WriteFact::Failed {
            fact,
            known_effects: false,
        },
        None,
    )?;
    Ok(error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairRow {
    operation: String,
    id: String,
    client_id: u32,
    symbol: String,
    side: String,
    order_type: String,
}
fn pair_rows(body: &str) -> (Vec<PairRow>, usize) {
    let raw: Vec<&serde_json::value::RawValue> = serde_json::from_str(body).unwrap_or_default();
    let rows = raw
        .iter()
        .filter_map(|raw| serde_json::from_str(raw.get()).ok())
        .collect();
    (rows, raw.len())
}
fn correlated_pair_row<'a>(expected: &NativeOrder, rows: &'a [PairRow]) -> Option<&'a PairRow> {
    let mut matched = rows
        .iter()
        .filter(|row| Some(row.client_id) == expected.client_id);
    let row = matched.next()?;
    if matched.next().is_some()
        || row.symbol != expected.symbol
        || row.side != expected.side
        || row.order_type != expected.order_type
    {
        return None;
    }
    Some(row)
}

fn pair_fact(
    cx: &mut Cx<'_, Pending>,
    managed: &ManagedWrite,
    orders: &[NativeOrder],
    body: &str,
) -> Result<(WriteResult, Vec<ElementResult>), safe_cex_engine::request_context::Error> {
    let (rows, count) = pair_rows(body);
    let mut items = vec![];
    let mut results = vec![];
    for expected in orders {
        let row = correlated_pair_row(expected, &rows);
        if let Some(row) = row
            && row.operation == "Ok"
            && !row.id.trim().is_empty()
            && rows.iter().filter(|other| other.id == row.id).count() == 1
        {
            items.push(BatchItem::Accepted(&row.id));
            results.push(ElementResult::Placed {
                order_ids: vec![row.id.clone()],
            });
        } else {
            let native = Native {
                code: NativeCode::None,
                message: "Backpack invalid batch response identity",
            };
            let fact =
                cx.classify_write(&managed.context, &BACKPACK, Some(&native), native.message)?;
            let error = Box::new(fact.error().clone());
            results.push(match error.outcome {
                Some(Outcome::Rejected | Outcome::NotSent) => {
                    ElementResult::NotSent { error: Some(error) }
                }
                _ => ElementResult::Unknown { error },
            });
            if row.is_some() {
                items.push(BatchItem::Native(native));
            }
        }
    }
    if count > orders.len() && items.len() == orders.len() {
        // A real extra native row invalidates the complete-batch proof.
        items.push(BatchItem::Native(Native {
            code: NativeCode::None,
            message: "Backpack unexpected batch response row",
        }));
    }
    let result = cx.http_write_batch(
        &managed.context,
        &BACKPACK,
        orders.len(),
        &items,
        "Backpack invalid batch response identity",
        None,
    )?;
    match &result {
        WriteResult::Accepted { .. } => {}
        WriteResult::Rejected(error)
        | WriteResult::KnownPartial(error)
        | WriteResult::PartialUnknown(error)
        | WriteResult::Failed(error) => {
            if error.outcome == Some(Outcome::Unknown) {
                let _ = cx.present_plan_failure(
                    &managed.plan,
                    with_unknown_guidance(error.clone()).message,
                );
            }
        }
    }
    Ok((result, results))
}

impl Request {
    pub(crate) fn unproven_evidence(
        &self,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        if self.managed.is_none() {
            return;
        }
        #[derive(Deserialize)]
        struct Envelope<'a> {
            #[serde(borrow)]
            code: Option<&'a serde_json::value::RawValue>,
            message: Option<&'a str>,
            id: Option<&'a str>,
        }
        if let Ok(row) = serde_json::from_str::<Envelope<'_>>(response.body) {
            if let Some(raw) = row.code {
                if let Ok(text) = serde_json::from_str::<String>(raw.get()) {
                    evidence.venue_code(&text);
                } else if let Some(code) = safe_cex_engine::classify::decode_int(raw.get()) {
                    evidence.venue_code(&code.to_string());
                }
            }
            if let Some(message) = row.message {
                evidence.venue_message(message);
            }
            if let Step::Place { .. } = self.step
                && let Some(id) = row.id.filter(|id| !id.trim().is_empty())
            {
                evidence.order_id(id);
            }
        }
        if let Step::Pair(orders) = &self.step {
            let (rows, _) = pair_rows(response.body);
            for order in orders {
                if let Some(row) = correlated_pair_row(order, &rows)
                    && row.operation == "Ok"
                    && !row.id.trim().is_empty()
                    && rows.iter().filter(|other| other.id == row.id).count() == 1
                    && !evidence.order_id(&row.id)
                {
                    break;
                }
            }
        }
    }

    pub(crate) fn reconciliation(
        &self,
    ) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
        if self.managed.is_none() {
            return Err(safe_cex_engine::request_context::Error::Invalid);
        }
        #[derive(Serialize)]
        struct Late<'a> {
            kind: &'static str,
            ids: Vec<&'a str>,
            clients: Vec<u32>,
            symbol: Option<&'a str>,
        }
        let (kind, ids, clients) = match &self.step {
            Step::Place { order, .. } => ("place", vec![], order.client_id.into_iter().collect()),
            Step::Pair(orders) => (
                "pair",
                vec![],
                orders.iter().filter_map(|order| order.client_id).collect(),
            ),
            Step::Cancel { id, .. } => ("cancel", vec![id.as_str()], vec![]),
            Step::CancelAll { .. } => ("cancelAll", vec![], vec![]),
            Step::Leverage(..) => ("leverage", vec![], vec![]),
            Step::OpenOrders { .. } => {
                return Err(safe_cex_engine::request_context::Error::Invalid);
            }
        };
        let symbol = match &self.step {
            Step::CancelAll { symbol } => Some(symbol.as_str()),
            _ => None,
        };
        let text = serde_json::to_string(&Late {
            kind,
            ids,
            clients,
            symbol,
        })
        .map_err(|_| safe_cex_engine::request_context::Error::Invalid)?;
        ReconciliationData::new(text)
    }
}

#[cfg(test)]
mod managed_repair_gate_tests {
    use super::*;
    #[test]
    fn identity_only_account_repair_stops_before_job_creation_or_write() {
        use safe_cex_engine::signing::{Identity, KeyRef};
        use safe_cex_engine::test_support::ProbeVenue;
        let venue = Backpack::new(crate::Options {
            identity: Some(Identity::new(
                KeyRef("credential".into()),
                Some(KeyRef("signing".into())),
            )),
            ..Default::default()
        });
        let probe = ProbeVenue::new(venue, |venue: &mut Backpack, cx: &mut Cx<'_, Pending>| {
            let reason = venue
                .repair_primary()
                .expect_err("installed identity is not native readiness");
            venue.repair_local_refusal(cx, reason);
            assert!(venue.repairs_is_empty());
            assert!(venue.trading.jobs.is_empty());
            assert!(venue.trading.cancels.is_empty());
        });
        let mut e = safe_cex_engine::test_support::seeded_engine(probe);
        e.start(1000.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(actions.iter().any(|action| matches!(action,
            safe_cex_engine::Action::Notice(safe_cex_engine::Notice::Event(event))
            if event.message == "Backpack automatic trigger repair awaits managed account signing")));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, safe_cex_engine::Action::ManagedHttp(_)))
        );
        assert!(!actions.iter().any(|action| matches!(action,
            safe_cex_engine::Action::Http(http) if http.method != Method::Get)));
    }
}
