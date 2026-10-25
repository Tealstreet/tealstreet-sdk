//! Commands are preflighted plans, then sent once in sequence.
use crate::account::id;
use crate::sign::Params;
use crate::wire::RestEnvelope;
use crate::{Cryptocom, Pending};
use safe_cex_engine::Level;
use safe_cex_engine::classify::{
    CodeTable, Intent, Key, Native, NativeCode, Row, Verdict, WriteFact,
};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::plan::{CallBound, CallTerms, RetryWindow, Slot, StepBound};
use safe_cex_engine::request_context::{
    Context, HttpOwnerRetirement, Operation, Plan, PlanPolicy, Policy, Preflight, PreflightPolicy,
    PreflightPurpose, RetryEvidence, RetryPolicy,
};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Outcome, Response, TimerId, VenueError, num,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};

fn local_mutation_policy() -> Policy {
    Policy {
        operation: Operation::Mutation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

#[derive(Clone)]
pub enum Call {
    Place(Option<OrderIdentityReceipt>),
    Amend,
    Cancel,
    CancelSymbol,
    LeverageAccount { leverage: f64 },
    Leverage { leverage: f64 },
}
#[derive(Clone)]
pub struct Step {
    path: &'static str,
    params: Params,
    call: Call,
}
#[derive(Clone)]
enum Authority {
    Preflight(Preflight),
    Plan(Plan),
}
#[derive(Clone)]
enum Physical {
    Prerequisite {
        preflight: Preflight,
        context: Context,
    },
    Write {
        plan: Plan,
        slot: Slot,
        context: Context,
    },
}
pub struct Request {
    pub command: CommandId,
    step: Step,
    retried: bool,
    physical: Physical,
    identity: Identity,
}
impl Request {
    pub(crate) fn delivered(&self, cx: &Cx<'_, Pending>) -> bool {
        match &self.physical {
            Physical::Prerequisite { preflight, context } => {
                cx.validate_preflight_delivery(preflight, context).is_ok()
            }
            Physical::Write { plan, context, .. } => {
                cx.validate_plan_delivery(plan, context).is_ok()
            }
        }
    }
    fn context(&self) -> &Context {
        match &self.physical {
            Physical::Prerequisite { context, .. } | Physical::Write { context, .. } => context,
        }
    }
}
struct Job {
    steps: VecDeque<Step>,
    ids: Vec<String>,
    best_effort: bool,
    authority: Option<Authority>,
    identity: Option<Identity>,
}
#[derive(Default)]
pub struct Trading {
    jobs: HashMap<CommandId, Job>,
    queue: VecDeque<CommandId>,
    active: bool,
    pub timer: Option<TimerId>,
}

fn advanced(kind: OrderType) -> bool {
    matches!(kind, OrderType::StopLoss | OrderType::TakeProfit)
}
fn trigger_name(trigger: Option<StopTriggerType>) -> &'static str {
    match trigger {
        Some(StopTriggerType::IndexPrice) => "INDEX_PRICE",
        Some(StopTriggerType::LastPrice) => "LAST_PRICE",
        _ => "MARK_PRICE",
    }
}
fn tif_name(tif: TimeInForce) -> &'static str {
    match tif {
        TimeInForce::GoodTillCancel => "GOOD_TILL_CANCEL",
        TimeInForce::ImmediateOrCancel => "IMMEDIATE_OR_CANCEL",
        TimeInForce::FillOrKill => "FILL_OR_KILL",
        TimeInForce::PostOnly => "GOOD_TILL_CANCEL",
    }
}

fn validate_id(opts: &PlaceOrderOpts, ids: &mut HashSet<String>) -> Result<(), String> {
    let Some(id) = &opts.client_order_id else {
        return Ok(());
    };
    if id.is_empty()
        || id.len() > 36
        || id
            .chars()
            .any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
    {
        return Err("Crypto.com clientOrderId must match /^.{1,36}$/".into());
    }
    if !matches!(opts.kind, OrderType::Market | OrderType::Limit)
        || opts.trigger_price.is_some()
        || opts.take_profit.is_some()
        || opts.stop_loss.is_some()
    {
        return Err(
            "Crypto.com clientOrderId can be honored only on plain Limit/Market orders".into(),
        );
    }
    if !ids.insert(id.clone()) {
        return Err("Crypto.com duplicate clientOrderId in batch".into());
    }
    Ok(())
}

impl Cryptocom {
    fn plan_place(&self, cx: &Cx<'_, Pending>, orders: Vec<PlaceOrderOpts>) -> Result<Job, String> {
        if orders.iter().any(|order| {
            order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some()
        }) {
            return Err("Bybit BBO and TickSize controls are not supported by Crypto.com".into());
        }
        let mut ids = HashSet::new();
        for order in &orders {
            validate_id(order, &mut ids)?;
        }
        let mut steps = VecDeque::new();
        for order in orders {
            if cx.store.market(&order.symbol).is_none() {
                return Err(format!("Market not found: {}", order.symbol));
            }
            if !order.amount.is_finite() || order.amount <= 0.0 {
                return Err("A positive quantity is required".into());
            }
            if order.stop_loss.is_some() || order.take_profit.is_some() {
                return Err("Crypto.com attached brackets are not supported".into());
            }
            if !matches!(
                order.kind,
                OrderType::Market | OrderType::Limit | OrderType::StopLoss | OrderType::TakeProfit
            ) {
                return Err("Unsupported Crypto.com order type".into());
            }
            let is_trigger = advanced(order.kind);
            if !is_trigger && order.trigger_price.is_some() {
                return Err("Crypto.com opening trigger orders are not supported".into());
            }
            let mut params = Params::default()
                .text("instrument_name", &order.symbol)
                .text(
                    "side",
                    if order.side == OrderSide::Buy {
                        "BUY"
                    } else {
                        "SELL"
                    },
                )
                .text(
                    "type",
                    match order.kind {
                        OrderType::Market => "MARKET",
                        OrderType::StopLoss => "STOP_LOSS",
                        OrderType::TakeProfit => "TAKE_PROFIT",
                        _ => "LIMIT",
                    },
                )
                .text("quantity", num::format(order.amount));
            let receipt = order
                .client_order_id
                .as_ref()
                .map(|id| OrderIdentityReceipt {
                    symbol: order.symbol.clone(),
                    kind: OrderIdentityKind::ClientOrderId,
                    identity: id.clone(),
                });
            if let Some(id) = order.client_order_id {
                params = params.text("client_oid", id);
            }
            if is_trigger {
                let trigger = order
                    .trigger_price
                    .or(order.price)
                    .filter(|n| n.is_finite() && *n > 0.0)
                    .ok_or("A positive trigger price is required")?;
                params = params.text("ref_price", num::format(trigger)).text(
                    "ref_price_type",
                    trigger_name(if order.kind == OrderType::StopLoss {
                        order.sl_trigger_type
                    } else {
                        order.tp_trigger_type
                    }),
                );
            } else if order.kind == OrderType::Limit {
                let price = order
                    .price
                    .filter(|n| n.is_finite() && *n > 0.0)
                    .ok_or("A positive limit price is required")?;
                params = params.text("price", num::format(price));
                if let Some(tif) = order.time_in_force {
                    params = params.text("time_in_force", tif_name(tif));
                }
            }
            let mut instructions = Vec::new();
            if order.reduce_only == Some(true) {
                instructions.push("REDUCE_ONLY".into());
            }
            if order.kind == OrderType::Limit && order.time_in_force == Some(TimeInForce::PostOnly)
            {
                instructions.push("POST_ONLY".into());
            }
            if !instructions.is_empty() {
                params = params.strings("exec_inst", instructions);
            }
            steps.push_back(Step {
                path: if is_trigger {
                    "private/advanced/create-order"
                } else {
                    "private/create-order"
                },
                params,
                call: Call::Place(receipt),
            });
        }
        Ok(Job {
            steps,
            ids: vec![],
            best_effort: true,
            authority: None,
            identity: None,
        })
    }

    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let (command, plan) = match command {
            Command::PlaceOrders { command, orders } => (command, self.plan_place(cx, orders)),
            Command::CancelOrders { command, ids } => {
                if let Some(id) = ids.iter().find(|id| cx.store.order(id).is_none()) {
                    let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                        return;
                    };
                    let _ = cx.fail_context(
                        &context,
                        not_sent(format!(
                            "Order {id} not found; cancellation requires a reconciled order"
                        )),
                    );
                    return;
                }
                let mut steps = VecDeque::new();
                for id in ids {
                    let order = cx.store.order(&id).expect("cancel preflight");
                    let mut params = Params::default().text("order_id", &id);
                    if !advanced(order.kind) {
                        params = params.text("instrument_name", &order.symbol);
                    }
                    steps.push_back(Step {
                        path: if advanced(order.kind) {
                            "private/advanced/cancel-order"
                        } else {
                            "private/cancel-order"
                        },
                        params,
                        call: Call::Cancel,
                    });
                }
                (
                    command,
                    Ok(Job {
                        steps,
                        ids: vec![],
                        best_effort: true,
                        authority: None,
                        identity: None,
                    }),
                )
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => {
                let Some(order) = cx.store.order(&id) else {
                    let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                        return;
                    };
                    let _ = cx.fail_context(&context, not_sent(format!("Order not found: {id}")));
                    return;
                };
                let price = price.unwrap_or(order.price);
                let amount = amount.unwrap_or(order.amount);
                if !price.is_finite() || price <= 0.0 || !amount.is_finite() || amount <= 0.0 {
                    let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                        return;
                    };
                    let _ = cx.fail_context(
                        &context,
                        not_sent("A positive price and quantity are required"),
                    );
                    return;
                }
                let mut params = Params::default()
                    .text("order_id", &id)
                    .text("new_quantity", num::format(amount));
                let path = if advanced(order.kind) {
                    params = params.text("new_ref_price", num::format(price));
                    "private/advanced/amend-order"
                } else {
                    params = params
                        .text("instrument_name", &order.symbol)
                        .text("new_price", num::format(price));
                    "private/amend-order"
                };
                (
                    command,
                    Ok(single(Step {
                        path,
                        params,
                        call: Call::Amend,
                    })),
                )
            }
            Command::CancelSymbolOrders { command, symbol } => (
                command,
                Ok(single(Step {
                    path: "private/cancel-all-orders",
                    params: if symbol.is_empty() {
                        Params::default()
                    } else {
                        Params::default().text("instrument_name", symbol)
                    },
                    call: Call::CancelSymbol,
                })),
            ),
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => {
                if cx.store.market(&symbol).is_none()
                    || !(1.0..=100.0).contains(&leverage)
                    || leverage.fract() != 0.0
                {
                    let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                        return;
                    };
                    let _ = cx.fail_context(
                        &context,
                        not_sent("Crypto.com account leverage must be an integer from 1 to 100"),
                    );
                    return;
                }
                (
                    command,
                    Ok(single(Step {
                        path: "private/user-balance",
                        params: Params::default(),
                        call: Call::LeverageAccount { leverage },
                    })),
                )
            }
            Command::SetMarginType {
                command: _,
                symbol,
                isolated,
            } => {
                let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                    return;
                };
                let _ = cx.fail_context(&context, not_sent(format!("Crypto.com does not support switching {symbol} to {} margin through Exchange v1", if isolated { "isolated" } else { "cross" })).kind(ErrorKind::Unsupported));
                return;
            }
            Command::SetPositionMode { .. } => {
                if cx.done_local_setting("[]".into(), |_| {}).is_ok() {
                    cx.notice(
                        Level::Warn,
                        code::COMMAND_UNSUPPORTED,
                        "Position mode is always hedged on Crypto.com",
                    );
                }
                return;
            }
            _ => return,
        };
        if self.credentials.is_none() {
            let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                not_sent("Crypto.com credentials are required").kind(ErrorKind::AuthInvalid),
            );
            return;
        }
        match plan {
            Ok(mut job) => {
                let Ok((selection, identity)) = self.slots.select(CredentialRole::Mutation) else {
                    return;
                };
                let credential = selection.credential(identity.credential.clone());
                let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: Some(credential.clone()),
                    write_credential: Some(credential),
                }) else {
                    return;
                };
                job.authority = Some(Authority::Preflight(preflight.clone()));
                job.identity = Some(identity.clone());
                let waiting = !self.trading.queue.is_empty()
                    || self.trading.active
                    || self.trading.timer.is_some();
                if waiting && let Err(error) = cx.park_preflight(&preflight) {
                    let _ = cx.fail_preflight(&preflight, error.venue_error());
                    return;
                }
                self.trading.jobs.insert(command, job);
                self.trading.queue.push_back(command);
                if !waiting {
                    self.start_trade(cx, command);
                }
            }
            Err(error) => {
                let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                    return;
                };
                let _ = cx.command_error_context(&context, &error);
                let _ = cx.fail_context(&context, not_sent(error));
            }
        }
    }
    fn start_trade(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(job) = self.trading.jobs.get(&command) else {
            return;
        };
        let Some(Authority::Preflight(preflight)) = job.authority.clone() else {
            return;
        };
        if job.steps.is_empty() {
            let _ = cx.done_empty_plan(&preflight, "[]".into());
            self.remove_trade(command);
            self.wake_next_trade(cx);
            return;
        }
        if let Some(Step {
            call: Call::LeverageAccount { .. },
            ..
        }) = job.steps.front()
        {
            let identity = job.identity.clone().expect("admitted primary");
            let credential = self
                .slots
                .select(CredentialRole::Read { scope: None })
                .map(|(selection, _)| selection.credential(identity.credential.clone()));
            let Ok(credential) = credential else {
                let _ =
                    cx.fail_preflight(&preflight, not_sent("Crypto.com credentials are required"));
                return;
            };
            let context = match cx.preflight_read_context(
                &preflight,
                Policy {
                    operation: Operation::Read,
                    credential: Some(credential),
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::ReadFailure,
                },
                CallTerms {
                    wait_ms: 0,
                    timeout_ms: 20_000,
                    attempts: 2,
                    retry_window: RetryWindow::FreshAttempt,
                },
            ) {
                Ok(context) => context,
                Err(error) => {
                    let _ = cx.fail_preflight(&preflight, error.venue_error());
                    return;
                }
            };
            let step = self
                .trading
                .jobs
                .get_mut(&command)
                .unwrap()
                .steps
                .pop_front()
                .unwrap();
            self.trading.active = true;
            self.queue_trade(
                cx,
                Request {
                    command,
                    step,
                    retried: false,
                    identity,
                    physical: Physical::Prerequisite { preflight, context },
                },
            );
        } else {
            self.admit_trade_plan(cx, command, &preflight, false);
        }
    }
    fn admit_trade_plan(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        preflight: &Preflight,
        after_read: bool,
    ) {
        let Some(job) = self.trading.jobs.get(&command) else {
            return;
        };
        let call = [CallBound {
            timeout_ms: 20_000,
            attempts: 2,
            retry_window: RetryWindow::FreshAttempt,
            after_refusal: &[],
        }];
        let steps: Vec<_> = (0..job.steps.len())
            .map(|index| StepBound {
                wait_ms: if index == 0 && !after_read { 0 } else { 20 },
                calls: &call,
            })
            .collect();
        let identity = job.identity.as_ref().expect("admitted primary");
        let credential = self
            .slots
            .select(CredentialRole::Mutation)
            .map(|(selection, _)| selection.credential(identity.credential.clone()));
        let Ok(credential) = credential else {
            let _ = cx.fail_preflight(preflight, not_sent("Crypto.com credentials are required"));
            return;
        };
        match cx.admit_plan(
            preflight,
            &steps,
            PlanPolicy {
                operation: Operation::Mutation,
                credential: Some(credential),
                pacing: None,
                retry: RetryPolicy::DefinitiveRefusal,
                independent_calls: job.best_effort,
            },
        ) {
            Ok(plan) => {
                self.trading.jobs.get_mut(&command).unwrap().authority =
                    Some(Authority::Plan(plan.clone()));
                self.trading.active = false;
                if !after_read {
                    self.send_trade_step(cx, command, &plan);
                }
            }
            Err(error) => {
                let _ = cx.fail_preflight(preflight, error.venue_error());
            }
        }
    }
    fn send_trade_step(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, plan: &Plan) {
        if self.trading.active {
            return;
        }
        let Some(job) = self.trading.jobs.get(&command) else {
            return;
        };
        let Some(step) = job.steps.front().cloned() else {
            return;
        };
        let slot = match cx.plan_slots(plan) {
            Ok(slots) => match slots.into_iter().next() {
                Some(slot) => slot,
                None => return,
            },
            Err(error) => {
                let _ = cx.fail_plan(plan, error.venue_error());
                return;
            }
        };
        let context = match cx.plan_leg_context_with_intent(plan, &slot, step.call.intent()) {
            Ok(context) => context,
            Err(error) => {
                let _ = cx.fail_plan(plan, error.venue_error());
                return;
            }
        };
        let job = self.trading.jobs.get_mut(&command).unwrap();
        job.steps.pop_front();
        let identity = job.identity.clone().expect("admitted primary");
        if let Call::Place(Some(receipt)) = &step.call {
            cx.command_progress(command, vec![receipt.clone()]);
        }
        self.trading.active = true;
        self.queue_trade(
            cx,
            Request {
                command,
                step,
                retried: false,
                identity,
                physical: Physical::Write {
                    plan: plan.clone(),
                    slot,
                    context,
                },
            },
        );
    }
    fn queue_trade(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let path = request.step.path;
        let url = format!("{}/{path}", self.rest);
        let recipe = crate::sign::Envelope::http(
            request.identity.clone(),
            request.step.path,
            request.step.params.clone(),
            &url,
        );
        if let Err((Pending::Trade(request), error)) = cx.request_signed_managed(
            request.context().clone(),
            safe_cex_engine::Method::Post,
            url,
            None,
            Box::new(recipe),
            Pending::Trade(request),
        ) {
            self.trade_reject(cx, request, error.venue_error());
        }
    }
    pub(crate) fn trade_reject(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        match &request.physical {
            Physical::Prerequisite { preflight, .. } => {
                let _ = cx.fail_preflight(preflight, error);
            }
            Physical::Write { plan, .. } => {
                let _ = cx.fail_plan(plan, error);
            }
        }
    }
    pub(crate) fn trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        let Physical::Write {
            plan,
            slot,
            context,
        } = &request.physical
        else {
            self.trade_reject(cx, request, error);
            return;
        };
        if self
            .trading
            .jobs
            .get(&request.command)
            .is_some_and(|job| job.best_effort)
            && cx.finish_plan_failed_call(plan, slot, context).is_ok()
        {
            self.trade_settled(cx);
        } else {
            let _ = cx.fail_plan(plan, error);
        }
    }
    fn trade_settled(&mut self, cx: &mut Cx<'_, Pending>) {
        self.trading.active = false;
        if let Some(timer) = self.trading.timer.take() {
            cx.clear_timer(timer);
        }
        self.trading.timer = Some(cx.set_timer(20.));
    }
    pub(crate) fn drive_trading(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.active || self.trading.timer.is_some() {
            return;
        }
        let Some(command) = self.trading.queue.front().copied() else {
            return;
        };
        let Some(job) = self.trading.jobs.get(&command) else {
            return;
        };
        if !job.steps.is_empty() {
            return;
        }
        let Some(Authority::Plan(plan)) = job.authority.clone() else {
            return;
        };
        let result = serde_json::to_string(&job.ids).unwrap();
        if cx.done_plan(&plan, result).is_ok() {
            self.remove_trade(command);
            self.wake_next_trade(cx);
        }
    }
    fn remove_trade(&mut self, command: CommandId) {
        self.trading.jobs.remove(&command);
        self.trading.queue.retain(|id| *id != command);
        self.trading.active = false;
    }
    fn wake_next_trade(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(job) = self
            .trading
            .queue
            .front()
            .and_then(|id| self.trading.jobs.get(id))
            && let Some(Authority::Preflight(preflight)) = &job.authority
        {
            let _ = cx.request_preflight_ready(preflight);
        }
    }
    pub(crate) fn trading_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &Preflight,
    ) {
        let command = self.trading.queue.front().copied().filter(|id| self.trading.jobs.get(id).is_some_and(|job|
            matches!(&job.authority, Some(Authority::Preflight(own)) if own.owner()==preflight.owner())));
        if let Some(command) = command {
            self.start_trade(cx, command);
        }
    }
    pub(crate) fn trading_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        let command = self.trading.queue.front().copied().filter(|id| {
            self.trading.jobs.get(id).is_some_and(|job|
            matches!(&job.authority, Some(Authority::Plan(own)) if own.owner()==plan.owner()))
        });
        if let Some(command) = command {
            if let Some(timer) = self.trading.timer.take() {
                cx.clear_timer(timer);
            }
            self.send_trade_step(cx, command, plan);
        }
    }
    pub(crate) fn trading_retired(
        &mut self,
        retirement: &HttpOwnerRetirement,
    ) -> Option<Preflight> {
        let command = self
            .trading
            .jobs
            .iter()
            .find_map(|(id, job)| match &job.authority {
                Some(Authority::Preflight(own)) if retirement.matches_preflight(own) => Some(*id),
                Some(Authority::Plan(own)) if retirement.matches_plan(own) => Some(*id),
                _ => None,
            })?;
        let active = self.trading.queue.front() == Some(&command);
        self.trading.jobs.remove(&command);
        self.trading.queue.retain(|id| *id != command);
        if active {
            self.trading.active = false;
            self.trading.timer = None;
            self.trading
                .queue
                .front()
                .and_then(|id| self.trading.jobs.get(id))
                .and_then(|job| match &job.authority {
                    Some(Authority::Preflight(own)) => Some(own.clone()),
                    _ => None,
                })
        } else {
            None
        }
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if !request.delivered(cx) {
            return;
        }
        let envelope = serde_json::from_str::<RestEnvelope>(response.body);
        if let Physical::Prerequisite { preflight, context } = &request.physical {
            if let Ok(envelope) = &envelope
                && envelope.code == 40101
                && !request.retried
            {
                match cx.preflight_retry_context(preflight, context, RetryEvidence::ReadFailure) {
                    Ok(next) => {
                        request.physical = Physical::Prerequisite {
                            preflight: preflight.clone(),
                            context: next,
                        };
                        request.retried = true;
                        self.queue_trade(cx, request);
                    }
                    Err(error) => {
                        let _ = cx.fail_preflight(preflight, error.venue_error());
                    }
                }
                return;
            }
            let result = (|| {
                let envelope = envelope.map_err(|_| "invalid Crypto.com response".to_owned())?;
                if envelope.code != 0 || !(200..300).contains(&response.status) {
                    return Err(format!(
                        "Crypto.com API Error {}: {}",
                        envelope.code, envelope.message
                    ));
                }
                #[derive(Deserialize)]
                struct AccountId<'a> {
                    account_id: &'a str,
                }
                #[derive(Deserialize)]
                struct BalanceData<'a> {
                    #[serde(borrow)]
                    data: Vec<AccountId<'a>>,
                }
                let data: BalanceData =
                    serde_json::from_str(envelope.result.ok_or("missing balance result")?.get())
                        .map_err(|_| "missing account id in balance")?;
                let account = data.data.first().ok_or("missing account id in balance")?;
                let Call::LeverageAccount { leverage } = request.step.call else {
                    return Err("invalid leverage prerequisite".into());
                };
                Ok(Step {
                    path: "private/change-account-leverage",
                    params: Params::default()
                        .text("account_id", account.account_id)
                        .number("leverage", leverage as u64),
                    call: Call::Leverage { leverage },
                })
            })();
            match result {
                Ok(step) => {
                    if let Some(job) = self.trading.jobs.get_mut(&request.command) {
                        job.steps.push_front(step);
                    }
                    self.trading.active = false;
                    self.admit_trade_plan(cx, request.command, preflight, true);
                }
                Err(error) => {
                    let _ = cx.fail_preflight(preflight, VenueError::new(error));
                }
            }
            return;
        }
        let Physical::Write {
            plan,
            slot,
            context,
        } = &request.physical
        else {
            unreachable!()
        };
        let parsed = match &envelope {
            Ok(envelope) if envelope.code == 0 && (200..300).contains(&response.status) => {
                if let Call::Place(_) | Call::Amend = request.step.call {
                    #[derive(Deserialize)]
                    struct Created<'a> {
                        #[serde(borrow)]
                        order_id: &'a serde_json::value::RawValue,
                    }
                    envelope
                        .result
                        .and_then(|r| serde_json::from_str::<Created>(r.get()).ok())
                        .map(|r| id(r.order_id))
                        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
                        .map(Some)
                        .ok_or_else(|| "invalid order id".to_owned())
                } else {
                    Ok(None)
                }
            }
            Ok(envelope) => Err(format!(
                "Crypto.com API Error {}: {}",
                envelope.code, envelope.message
            )),
            Err(_) => Err("invalid Crypto.com response".into()),
        };
        let fact = match parsed {
            Ok(order_id) => {
                if let Some(id) = &order_id
                    && let Some(job) = self.trading.jobs.get_mut(&request.command)
                {
                    job.ids.push(id.clone());
                }
                WriteFact::Accepted {
                    order_ids: order_id.into_iter().collect(),
                }
            }
            Err(message) => {
                let native = envelope.as_ref().ok().map(|e| Native {
                    code: NativeCode::Int(e.code),
                    message: e.message,
                });
                let Ok(fact) = cx.classify_write(context, &ERRORS, native.as_ref(), message) else {
                    return;
                };
                WriteFact::Failed {
                    fact,
                    known_effects: false,
                }
            }
        };
        let accepted = matches!(&fact, WriteFact::Accepted { .. });
        let diagnostic = match &fact {
            WriteFact::Failed { fact, .. } => Some(fact.error().message.clone()),
            _ => None,
        };
        match cx.http_write_fact(context, fact, None) {
            Ok(Some(grant)) if !request.retried => {
                match cx.plan_resend_context(plan, slot, context, grant) {
                    Ok(next) => {
                        request.physical = Physical::Write {
                            plan: plan.clone(),
                            slot: slot.clone(),
                            context: next,
                        };
                        request.retried = true;
                        self.queue_trade(cx, request);
                    }
                    Err(error) => {
                        let _ = cx.fail_plan(plan, error.venue_error());
                    }
                }
                return;
            }
            Ok(_) => {}
            Err(_) => return,
        }
        if let Some(message) = diagnostic {
            let _ = cx.command_error_context(context, message);
        }
        if accepted {
            match request.step.call {
                Call::Place(_) | Call::Amend | Call::Cancel | Call::CancelSymbol => {
                    cx.store.invalidate_order_snapshots()
                }
                Call::Leverage { leverage } => {
                    let settings: Vec<_> = cx
                        .store
                        .markets()
                        .iter()
                        .map(|m| (m.symbol.clone(), leverage.min(m.limits.leverage.max)))
                        .collect();
                    for (symbol, leverage) in settings {
                        cx.store.update_setting(&symbol, |s| s.leverage = leverage);
                    }
                }
                Call::LeverageAccount { .. } => unreachable!(),
            }
        }
        if cx.finish_plan_call(plan, slot, context).is_ok() {
            self.trade_settled(cx);
        }
    }
}
impl Call {
    fn intent(&self) -> Intent {
        match self {
            Self::Place(_) => Intent::Place,
            Self::Amend => Intent::Amend,
            Self::Cancel | Self::CancelSymbol => Intent::Cancel,
            Self::Leverage { .. } => Intent::Setting,
            Self::LeverageAccount { .. } => Intent::Read,
        }
    }
}
pub(crate) const ERRORS: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Int(40101),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(40102),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(40103),
            verdict: Verdict::Refused(ErrorKind::AuthPermission),
            only: None,
        },
        Row {
            key: Key::Int(40104),
            verdict: Verdict::Refused(ErrorKind::AuthPermission),
            only: None,
        },
        Row {
            key: Key::Int(42901),
            verdict: Verdict::Refused(ErrorKind::RateLimited),
            only: None,
        },
        Row {
            key: Key::Int(40801),
            verdict: Verdict::Unknown(None),
            only: None,
        },
        Row {
            key: Key::Int(50001),
            verdict: Verdict::Unknown(None),
            only: None,
        },
        Row {
            key: Key::Int(306),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(308),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(309),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(40401),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[
        (400, Verdict::Refused(ErrorKind::Rejected)),
        (401, Verdict::Refused(ErrorKind::AuthInvalid)),
        (403, Verdict::Refused(ErrorKind::AuthPermission)),
        (429, Verdict::Refused(ErrorKind::RateLimited)),
    ],
};
fn single(step: Step) -> Job {
    Job {
        steps: VecDeque::from([step]),
        ids: vec![],
        best_effort: false,
        authority: None,
        identity: None,
    }
}
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}
