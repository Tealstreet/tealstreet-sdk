//! Each command is a preflighted sequence of writes. A lost write response is
//! terminal for that job: it is never replayed.
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
    Context, Credential, HttpOwnerRetirement, Operation, Plan, PlanPolicy, Preflight,
    PreflightPolicy, PreflightPurpose, ReconciliationData, RetryPolicy, WriteResult,
    plan::{CallBound, MutationTerminalBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

const BACKPACK: CodeTable = CodeTable {
    rows: &[],
    success: &[],
    unlisted: Verdict::Unknown(None),
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
    symbol: String,
    side: &'static str,
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
}
pub struct Request {
    managed: Option<ManagedWrite>,
    command: u64,
    step: Step,
    previous: Option<OrderStatus>,
    targets: Vec<(String, f64)>,
    leverage_revision: u64,
}
pub struct Repair {
    old_id: String,
    order: NativeOrder,
}
struct Job {
    managed: Option<Plan>,
    waiting: bool,
    cancel_latches: Vec<String>,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    collect_ids: bool,
    internal: bool,
    replaces: Option<String>,
    amendment: Option<f64>,
}
#[derive(Default)]
pub struct Trading {
    jobs: HashMap<u64, Job>,
    next_client: u32,
    cancels: HashMap<String, (usize, OrderStatus)>,
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
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
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
                read_credential: None,
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
                        .ok_or_else(|| format!("Order not found: {id}"))?;
                    if matches!(old.status, OrderStatus::Closed | OrderStatus::Canceled) {
                        return Err("Backpack cannot amend a terminal order".into());
                    }
                    if amount.unwrap_or(old.amount) <= old.filled {
                        return Err(
                            "Backpack amended total must exceed the already filled quantity".into(),
                        );
                    }
                    let o = PlaceOrderOpts {
                        symbol: old.symbol.clone(),
                        kind: old.kind,
                        side: old.side,
                        amount: (amount.unwrap_or(old.amount) - old.filled).max(0.0),
                        price: Some(
                            old.extra
                                .as_ref()
                                .and_then(|e| e.conditional_order_price)
                                .unwrap_or(price.unwrap_or(old.price)),
                        ),
                        trigger_price: old
                            .extra
                            .as_ref()
                            .and_then(|e| e.conditional_order_price)
                            .map(|_| price.unwrap_or(old.price)),
                        stop_loss: None,
                        take_profit: None,
                        reduce_only: Some(old.reduce_only),
                        time_in_force: old.time_in_force,
                        trigger_type: old.trigger_type,
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
            Command::SetMarginType { command, .. } => {
                let error = not_sent("Backpack only supports cross margin");
                cx.fail(command, error.kind(ErrorKind::Unsupported));
                return;
            }
            Command::SetPositionMode { command, .. } => {
                let error = not_sent("Backpack position mode is fixed");
                cx.fail(command, error.kind(ErrorKind::Unsupported));
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
                let amendment = if collect_ids {
                    steps.first().and_then(|step| match step {
                        Step::Cancel { id, .. } => cx.store.order(id).map(|o| o.filled),
                        _ => None,
                    })
                } else {
                    None
                };
                let preflight = preflight.as_ref().unwrap();
                if steps.is_empty() {
                    let _ = cx.done_empty_plan(preflight, "[]".into());
                    return;
                }
                let plan = match self.admit_trade(cx, preflight, &steps, amendment.is_some()) {
                    Ok(plan) => plan,
                    Err(error) => {
                        let _ = cx.fail_preflight(preflight, error.venue_error());
                        return;
                    }
                };
                self.trading.jobs.insert(
                    u64::from(id),
                    Job {
                        managed: Some(plan),
                        waiting: false,
                        cancel_latches: vec![],
                        steps: steps.into(),
                        ids: vec![],
                        collect_ids,
                        internal: false,
                        replaces: None,
                        amendment,
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
    fn admit_trade(
        &self,
        cx: &mut Cx<'_, Pending>,
        preflight: &Preflight,
        steps: &[Step],
        amendment: bool,
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
        let terminals = if amendment {
            vec![MutationTerminalBound { after_step: 0 }]
        } else {
            vec![]
        };
        cx.admit_plan_with_terminals(
            preflight,
            &bounds,
            PlanPolicy {
                independent_calls: false,
                operation: Operation::Mutation,
                credential: self.options.identity.as_ref().map(|identity| Credential {
                    slot: safe_cex_engine::CredentialSlot::Primary,
                    reference: identity.credential.clone(),
                }),
                pacing: None,
                retry: RetryPolicy::Never,
            },
            &terminals,
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
        if job.steps.is_empty()
            && let Some(plan) = &job.managed
        {
            let result = if job.collect_ids {
                serde_json::to_string(&job.ids).unwrap()
            } else {
                "[]".into()
            };
            let _ = cx.done_plan(plan, result);
            return;
        }
        let managed = if let Some(plan) = &job.managed {
            let Some(slot) = cx
                .plan_slots(plan)
                .ok()
                .and_then(|slots| slots.into_iter().next())
            else {
                return;
            };
            let intent = match job.steps.front() {
                Some(Step::Place { .. } | Step::Pair(..)) => Intent::Place,
                Some(Step::Cancel { .. }) if job.amendment.is_some() => Intent::Amend,
                Some(Step::Cancel { .. } | Step::CancelAll { .. }) => Intent::Cancel,
                Some(Step::Leverage(..)) => Intent::Setting,
                None => return,
            };
            let context = match cx.plan_leg_context_with_intent(plan, &slot, intent) {
                Ok(context) => context,
                Err(error) => {
                    let _ = cx.fail_plan(plan, error.venue_error());
                    return;
                }
            };
            job.waiting = true;
            Some(ManagedWrite {
                plan: plan.clone(),
                slot,
                context,
            })
        } else {
            None
        };
        let Some(step) = job.steps.pop_front() else {
            self.trading.jobs.remove(&command);
            return;
        };
        let mut previous = None;
        let mut targets = vec![];
        if !job.internal {
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
                let managed = managed.expect("paired writes are foreground plans");
                let params = rows.iter().map(NativeOrder::params).collect();
                let body = serde_json::to_string(rows).unwrap();
                let delivery = Request {
                    managed: Some(managed),
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
        if let Some(managed) = managed {
            let delivery = Request {
                managed: Some(managed),
                command,
                step,
                previous,
                targets,
                leverage_revision: self.account.leverage_revision,
            };
            self.send_managed_trade(cx, delivery, method, path, vec![params], body);
            return;
        }
        let result = self.signed(
            cx,
            method,
            path,
            params,
            Some(body),
            Pending::Trade(Request {
                managed: None,
                command,
                step,
                previous,
                targets,
                leverage_revision: self.account.leverage_revision,
            }),
            false,
        );
        if let Err(error) = result {
            if let Some(job) = self.trading.jobs.remove(&command) {
                for id in job.cancel_latches {
                    self.finish_cancel(&id);
                }
            }
            cx.notice(Level::Warn, code::ORDER_REJECTED, error);
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
            method,
            format!("{REST}{path}"),
            Some(body),
            Box::new(recipe),
            Pending::Trade(delivery),
        ) {
            self.trade_failed(cx, delivery, error.venue_error());
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
            if let Step::Pair(orders) = &request.step
                && (200..300).contains(&response.status)
                && !refused_in_body(response.body)
            {
                match pair_fact(cx, managed, orders, response.body) {
                    Ok(WriteResult::Accepted { .. }) => recorded = true,
                    Ok(_) => {
                        self.managed_trade_failed(
                            cx,
                            request,
                            VenueError::new("Backpack invalid batch response identity"),
                            true,
                        );
                        return;
                    }
                    Err(error) => {
                        let _ = cx.fail_plan(&managed.plan, error.venue_error());
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
                    cx.store.invalidate_order_snapshots();
                    if let Some(job) = self.trading.jobs.get_mut(&request.command) {
                        job.ids.push(row.id.into());
                        if let Some(old_id) = job.replaces.as_deref() {
                            let native: crate::wire::OrderRow<'_> =
                                serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                            let mapped = crate::account::order(native, cx.store)?;
                            cx.store.remove_orders(&[old_id]);
                            if cx.store.order(&mapped.id).is_none() {
                                cx.store.upsert_order(mapped);
                            }
                        }
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
                            job.ids.push(ids.remove(&o.client_id.unwrap()).unwrap());
                        }
                    }
                }
                Step::Cancel { id, .. } => {
                    if let Some(baseline) = self
                        .trading
                        .jobs
                        .get(&request.command)
                        .and_then(|job| job.amendment.as_ref())
                    {
                        #[derive(Deserialize)]
                        #[serde(rename_all = "camelCase")]
                        struct Canceled<'a> {
                            id: &'a str,
                            status: &'a str,
                            executed_quantity: crate::wire::Number,
                        }
                        let proof: Canceled<'_> = serde_json::from_str(response.body).map_err(
                            |_| "Backpack amend requires a complete cancel acknowledgement",
                        )?;
                        if proof.id != id
                            || !matches!(proof.status, "Cancelled" | "Filled")
                            || !proof.executed_quantity.0.is_finite()
                            || proof.executed_quantity.0 < *baseline
                        {
                            return Err(
                                "Backpack amend cancel acknowledgement is not definitive".into()
                            );
                        }
                        accept_trade(cx, &request, vec![id.clone()])?;
                        recorded = request.managed.is_some();
                        let filled = proof
                            .executed_quantity
                            .0
                            .max(cx.store.order(id).map_or(*baseline, |o| o.filled));
                        let extra = filled - baseline;
                        let job = self.trading.jobs.get_mut(&request.command).unwrap();
                        if extra > 0.0 {
                            let mut exhausted = false;
                            for step in &mut job.steps {
                                if let Step::Place { order, .. } = step {
                                    let value = order
                                        .quantity
                                        .as_mut()
                                        .or(order.trigger_quantity.as_mut())
                                        .ok_or("Backpack amend replacement quantity missing")?;
                                    let market = cx
                                        .store
                                        .market(&order.symbol)
                                        .ok_or("Backpack amend market missing")?;
                                    let remaining = num::adjust(
                                        value.parse::<f64>().map_err(|e| e.to_string())? - extra,
                                        market.precision.amount,
                                    );
                                    if remaining <= 0.0 {
                                        exhausted = true;
                                        break;
                                    }
                                    if remaining < market.limits.amount.min {
                                        return Err(
                                            "Backpack amend remainder is below market minimum"
                                                .into(),
                                        );
                                    }
                                    *value = num::format(remaining);
                                }
                            }
                            if exhausted {
                                job.steps.clear();
                            }
                        }
                    }
                    if !recorded {
                        if request.managed.is_some() {
                            validate_ack(response.body)?;
                        }
                        accept_trade(cx, &request, vec![id.clone()])?;
                        recorded = request.managed.is_some();
                    }
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
                    accept_trade(
                        cx,
                        &request,
                        request.targets.iter().map(|(id, _)| id.clone()).collect(),
                    )?;
                    recorded = request.managed.is_some();
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
                Step::Leverage(leverage) => {
                    if request.managed.is_some() {
                        validate_ack(response.body)?;
                    }
                    accept_trade(cx, &request, vec![])?;
                    recorded = request.managed.is_some();
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
                if let Some(managed) = &request.managed {
                    let step = &request.step;
                    let conditional = matches!(step, Step::Cancel { .. })
                        && self
                            .trading
                            .jobs
                            .get(&request.command)
                            .is_some_and(|job| job.amendment.is_some() && job.steps.is_empty());
                    if conditional {
                        let result = self
                            .trading
                            .jobs
                            .get(&request.command)
                            .map(|job| serde_json::to_string(&job.ids).unwrap())
                            .unwrap_or_else(|| "[]".into());
                        let _ = cx.done_mutation_plan_after_data(
                            &managed.plan,
                            &managed.slot,
                            &managed.context,
                            result,
                        );
                        return;
                    }
                    if let Err(error) =
                        cx.finish_plan_call(&managed.plan, &managed.slot, &managed.context)
                    {
                        let _ = cx.fail_plan(&managed.plan, error.venue_error());
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
                        let _ = cx.fail_plan(&managed.plan, VenueError::new(e));
                        return;
                    }
                    let fact = if (200..300).contains(&response.status)
                        && !refused_in_body(response.body)
                    {
                        cx.http_write_unproven(&managed.context, &e)
                    } else {
                        native_failure(cx, &managed.context, response.body, &e)
                    };
                    if let Err(error) = fact {
                        let _ = cx.fail_plan(&managed.plan, error.venue_error());
                        return;
                    }
                    self.managed_trade_failed(cx, request, VenueError::new(e), true);
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

    fn managed_trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
        data: bool,
    ) {
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
        if !optional {
            let _ = cx.fail_plan(&managed.plan, error);
            return;
        }
        let settled = if data {
            cx.finish_plan_call(&managed.plan, &managed.slot, &managed.context)
        } else {
            cx.finish_plan_failed_call(&managed.plan, &managed.slot, &managed.context)
        };
        if let Err(reason) = settled {
            let _ = cx.fail_plan(&managed.plan, reason.venue_error());
            return;
        }
        if let Some(job) = self.trading.jobs.get_mut(&request.command) {
            job.waiting = false;
        }
        cx.notice(Level::Error, code::CHILD_LEG_FAILED, &error.message);
        self.run_trade(cx, request.command);
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
    ) -> Result<Repair, String> {
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
    pub(crate) fn start_repair(&mut self, cx: &mut Cx<'_, Pending>, repair: Repair) {
        if self.account.credentials.is_none() {
            cx.notice(
                Level::Warn,
                code::ORDER_REJECTED,
                "Backpack automatic trigger repair awaits managed account signing",
            );
            return;
        }
        let command = (u64::from(u32::MAX) + 1..u64::MAX)
            .rev()
            .find(|id| !self.trading.jobs.contains_key(id))
            .unwrap();
        let steps = vec![
            Step::Cancel {
                symbol: repair.order.symbol.clone(),
                id: repair.old_id.clone(),
                optional: false,
            },
            Step::Place {
                order: Box::new(repair.order),
                optional: false,
            },
        ];
        self.trading.jobs.insert(
            command,
            Job {
                managed: None,
                waiting: false,
                cancel_latches: vec![],
                steps: steps.into(),
                ids: vec![],
                collect_ids: false,
                internal: true,
                replaces: Some(repair.old_id),
                amendment: None,
            },
        );
        self.run_trade(cx, command);
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
fn http_failure(status: u16, message: String) -> VenueError {
    let error = VenueError::new(message);
    match status {
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
    }
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
    body: &str,
    message: &str,
) -> Result<(), safe_cex_engine::request_context::Error> {
    #[derive(Deserialize)]
    struct Failure<'a> {
        #[serde(borrow)]
        code: Option<&'a serde_json::value::RawValue>,
        #[serde(borrow)]
        message: Option<&'a str>,
    }
    let failure = serde_json::from_str::<Failure<'_>>(body).ok();
    let raw = failure.as_ref().and_then(|failure| failure.code);
    let text = raw.and_then(|raw| serde_json::from_str::<String>(raw.get()).ok());
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
            .unwrap_or(""),
    };
    cx.http_write_failure(context, &BACKPACK, Some(&native), message, None)
        .map(|_| ())
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
) -> Result<WriteResult, safe_cex_engine::request_context::Error> {
    let (rows, count) = pair_rows(body);
    let mut items = vec![];
    for expected in orders {
        let Some(row) = correlated_pair_row(expected, &rows) else {
            continue;
        };
        if row.operation == "Ok"
            && !row.id.trim().is_empty()
            && rows.iter().filter(|other| other.id == row.id).count() == 1
        {
            items.push(BatchItem::Accepted(&row.id));
        } else {
            items.push(BatchItem::Native(Native {
                code: NativeCode::None,
                message: "Backpack invalid batch response identity",
            }));
        }
    }
    if count > orders.len() && items.len() == orders.len() {
        // A real extra native row invalidates the complete-batch proof.
        items.push(BatchItem::Native(Native {
            code: NativeCode::None,
            message: "Backpack unexpected batch response row",
        }));
    }
    cx.http_write_batch(
        &managed.context,
        &BACKPACK,
        orders.len(),
        &items,
        "Backpack invalid batch response identity",
        None,
    )
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
            identity: Some(Identity {
                credential: KeyRef("credential".into()),
                signing: KeyRef("signing".into()),
                passphrase: None,
                stable_id: None,
                extra: Default::default(),
            }),
            ..Default::default()
        });
        let probe = ProbeVenue::new(venue, |venue: &mut Backpack, cx: &mut Cx<'_, Pending>| {
            venue.start_repair(
                cx,
                Repair {
                    old_id: "old".into(),
                    order: NativeOrder::default(),
                },
            );
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
