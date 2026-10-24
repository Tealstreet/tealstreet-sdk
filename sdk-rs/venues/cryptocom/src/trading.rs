//! Commands are preflighted plans, then sent once in sequence.
use crate::account::id;
use crate::sign::Params;
use crate::wire::RestEnvelope;
use crate::{Cryptocom, Pending};
use safe_cex_engine::Level;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, CommandId, Cx, ErrorKind, Outcome, TimerId, VenueError, num};
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};

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
pub struct Request {
    pub command: CommandId,
    step: Step,
    retried: bool,
}
struct Job {
    steps: VecDeque<Step>,
    ids: Vec<String>,
    best_effort: bool,
    first_error: Option<VenueError>,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
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
            first_error: None,
            effective: false,
        })
    }

    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let (command, plan) = match command {
            Command::PlaceOrders { command, orders } => (command, self.plan_place(cx, orders)),
            Command::CancelOrders { command, ids } => {
                if let Some(id) = ids.iter().find(|id| cx.store.order(id).is_none()) {
                    cx.fail(
                        command,
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
                        first_error: None,
                        effective: false,
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
                    cx.fail(command, not_sent(format!("Order not found: {id}")));
                    return;
                };
                let price = price.unwrap_or(order.price);
                let amount = amount.unwrap_or(order.amount);
                if !price.is_finite() || price <= 0.0 || !amount.is_finite() || amount <= 0.0 {
                    cx.fail(
                        command,
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
                    cx.fail(
                        command,
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
                command,
                symbol,
                isolated,
            } => {
                cx.fail(command, not_sent(format!("Crypto.com does not support switching {symbol} to {} margin through Exchange v1", if isolated { "isolated" } else { "cross" })).kind(ErrorKind::Unsupported));
                return;
            }
            Command::SetPositionMode { command, .. } => {
                cx.notice(
                    Level::Warn,
                    code::COMMAND_UNSUPPORTED,
                    "Position mode is always hedged on Crypto.com",
                );
                cx.done(command, "[]".into());
                return;
            }
            _ => return,
        };
        if self.credentials.is_none() {
            cx.fail(
                command,
                not_sent("Crypto.com credentials are required").kind(ErrorKind::AuthInvalid),
            );
            return;
        }
        match plan {
            Ok(job) => {
                self.trading.jobs.insert(command, job);
                self.trading.queue.push_back(command);
                self.drive_trading(cx);
            }
            Err(error) => {
                cx.command_error(command, &error);
                cx.fail(command, not_sent(error));
            }
        }
    }
    pub(crate) fn drive_trading(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.active || self.trading.timer.is_some() {
            return;
        }
        while let Some(command) = self.trading.queue.pop_front() {
            let Some(job) = self.trading.jobs.get_mut(&command) else {
                continue;
            };
            if let Some(step) = job.steps.pop_front() {
                self.trading.active = true;
                if let Call::Place(Some(receipt)) = &step.call {
                    cx.command_progress(command, vec![receipt.clone()]);
                }
                self.signed(
                    cx,
                    step.path,
                    &step.params,
                    Pending::Trade(Request {
                        command,
                        step: step.clone(),
                        retried: false,
                    }),
                    false,
                );
                return;
            }
            let job = self.trading.jobs.remove(&command).unwrap();
            match (job.best_effort, job.first_error) {
                (false, Some(mut error)) => {
                    if (job.effective || !job.ids.is_empty())
                        && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
                    {
                        error.outcome = None;
                    }
                    cx.fail(command, error);
                }
                _ => cx.done(command, serde_json::to_string(&job.ids).unwrap()),
            }
        }
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Result<&str, &str>,
        status: Option<u16>,
    ) {
        let result = response.and_then(|body| {
            serde_json::from_str::<RestEnvelope>(body).map_err(|_| "invalid Crypto.com response")
        });
        if let Ok(envelope) = &result
            && envelope.code == 40101
            && !request.retried
        {
            request.retried = true;
            self.signed(
                cx,
                request.step.path,
                &request.step.params,
                Pending::Trade(Request {
                    command: request.command,
                    step: request.step.clone(),
                    retried: true,
                }),
                false,
            );
            return;
        }
        // Set when the failure comes from the envelope's own error code.
        let mut code = None;
        let result = result.map_err(str::to_owned).and_then(|envelope| {
            if envelope.code != 0 {
                code = Some(envelope.code);
                return Err(format!(
                    "Crypto.com API Error {}: {}",
                    envelope.code, envelope.message
                ));
            }
            match request.step.call {
                Call::Place(_) | Call::Amend => {
                    #[derive(Deserialize)]
                    struct Created<'a> {
                        #[serde(borrow)]
                        order_id: &'a serde_json::value::RawValue,
                    }
                    let row: Created =
                        serde_json::from_str(envelope.result.ok_or("missing order result")?.get())
                            .map_err(|_| "missing order id")?;
                    let id = id(row.order_id);
                    if id.is_empty() || id == "null" || !id.bytes().all(|b| b.is_ascii_digit()) {
                        return Err("invalid order id".into());
                    }
                    Ok(Some(id))
                }
                Call::LeverageAccount { leverage } => {
                    #[derive(Deserialize)]
                    struct AccountId<'a> {
                        account_id: &'a str,
                    }
                    #[derive(Deserialize)]
                    struct BalanceData<'a> {
                        #[serde(borrow)]
                        data: Vec<AccountId<'a>>,
                    }
                    let data: BalanceData = serde_json::from_str(
                        envelope.result.ok_or("missing balance result")?.get(),
                    )
                    .map_err(|_| "missing account id in balance")?;
                    let account = data.data.first().ok_or("missing account id in balance")?;
                    let step = Step {
                        path: "private/change-account-leverage",
                        params: Params::default()
                            .text("account_id", account.account_id)
                            .number("leverage", leverage as u64),
                        call: Call::Leverage { leverage },
                    };
                    self.trading
                        .jobs
                        .get_mut(&request.command)
                        .unwrap()
                        .steps
                        .push_front(step);
                    Ok(None)
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
                    Ok(None)
                }
                _ => Ok(None),
            }
        });
        // Only validated accepted order mutations supersede older joined reads.
        // Retain partial-batch acceptance even if a later sibling is refused.
        if result.is_ok()
            && let Call::Place(_) | Call::Amend | Call::Cancel | Call::CancelSymbol =
                request.step.call
        {
            cx.store.invalidate_order_snapshots();
        }
        // The balance read that precedes a leverage change sends nothing.
        let read = matches!(request.step.call, Call::LeverageAccount { .. });
        let job = self.trading.jobs.get_mut(&request.command).unwrap();
        match result {
            Ok(Some(id)) => {
                job.effective = true;
                job.ids.push(id);
            }
            Ok(None) => job.effective |= !read,
            Err(error) => {
                cx.command_error(request.command, &error);
                let mut error = classify(error, status, code);
                if read {
                    error.outcome = Some(Outcome::NotSent);
                }
                job.first_error.get_or_insert(error);
                if !job.best_effort {
                    job.steps.clear();
                }
            }
        }
        self.trading.active = false;
        self.trading.queue.push_front(request.command);
        // Aggregate writes stay below the TS client's 50 requests/second ceiling.
        self.trading.timer = Some(cx.set_timer(20.0));
    }
}
fn single(step: Step) -> Job {
    Job {
        steps: VecDeque::from([step]),
        ids: vec![],
        best_effort: false,
        first_error: None,
        effective: false,
    }
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// A failed write. Transport loss (no status), 408/5xx and malformed answers
/// do not prove a refusal; documented Crypto.com codes are classified.
fn classify(message: String, status: Option<u16>, code: Option<i64>) -> VenueError {
    let error = VenueError::new(message);
    let rejected = |error: VenueError, kind| error.kind(kind).outcome(Outcome::Rejected);
    let Some(code) = code else {
        return match status {
            None => error.kind(ErrorKind::Network).outcome(Outcome::Unknown),
            Some(401) => rejected(error, ErrorKind::AuthInvalid),
            Some(403) => rejected(error, ErrorKind::AuthPermission),
            Some(429) => rejected(error, ErrorKind::RateLimited),
            Some(400..=407 | 409..=499) => rejected(error, ErrorKind::Rejected),
            _ => error.outcome(Outcome::Unknown),
        };
    };
    let error = error.venue_code(code.to_string());
    // Exchange v1 reason codes only; codes from other API versions stay unset.
    match code {
        40101 | 40102 => rejected(error, ErrorKind::AuthInvalid),
        40103 | 40104 => rejected(error, ErrorKind::AuthPermission),
        42901 => rejected(error, ErrorKind::RateLimited),
        40801 | 50001 => error.outcome(Outcome::Unknown),
        306 | 308 | 309 | 40401 => rejected(error, ErrorKind::Rejected),
        _ => error,
    }
}
