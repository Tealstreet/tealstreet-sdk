//! Order entry. Each command becomes a job of sequential steps; a step's
//! requests go out together and the next step starts when they all answer.
//! Attached stop-loss and take-profit ride on the order itself; stops against
//! an open position and trailing stops go through `/v5/position/trading-stop`.
use crate::sign::Body;
use crate::wire::*;
use crate::{BybitLinear, Pending, Rest, Then, venue_error};
use safe_cex_engine::model::{
    MarginType, Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, PositionSide,
    StopTriggerType, TimeInForce,
};
use safe_cex_engine::{Command, CommandId, Cx, LogLevel, Response, TimerId, num};
use std::collections::{HashMap, VecDeque};

/// Orders per `create-batch` request (Bybit's non-VIP limit).
const BATCH_LIMIT: usize = 10;
/// Pacing between batches, inside the per-second order limits.
const PLACE_BATCH_DELAY_MS: f64 = 1_200.0;
const CANCEL_BATCH_DELAY_MS: f64 = 1_000.0;
const CANCEL_BATCH_LIMIT: usize = 10;
const LEVERAGE_NOT_MODIFIED: i64 = 110_043;
const POSITION_MODE_NOT_MODIFIED: i64 = 110_025;
const AGREEMENT_CODES: [i64; 2] = [110_123, 110_125];
const STOP_LOSS_SUFFIX: &str = "__stop_loss";
const TAKE_PROFIT_SUFFIX: &str = "__take_profit";

pub struct Request {
    command: CommandId,
    call: Call,
}

/// What a response means for its job.
enum Call {
    ResolveMode {
        symbol: String,
        command: Box<Command>,
        version: u64,
    },
    PlaceOne,
    /// Single-order bodies to place one by one if the venue refuses the
    /// whole batch (a refusal places nothing, so this cannot double-place).
    PlaceBatch {
        fallback: Vec<String>,
    },
    TradingStop,
    Amend,
    Cancel {
        ids: Vec<String>,
    },
    CancelAll {
        ids: Vec<String>,
    },
    Leverage {
        symbol: String,
        leverage: f64,
    },
    MarginMode {
        isolated: bool,
    },
    PositionMode {
        hedged: bool,
    },
}

struct Planned {
    path: &'static str,
    body: String,
    call: Call,
}

struct Step {
    /// Wait this long before sending (batch pacing).
    delay_ms: f64,
    requests: Vec<Planned>,
}

#[derive(Default)]
struct Job {
    steps: VecDeque<Step>,
    outstanding: usize,
    /// Run every step even after a failure (independent orders of one batch).
    continue_on_error: bool,
    ids: Vec<String>,
    error: Option<String>,
    delay: Option<TimerId>,
}

#[derive(Default)]
pub struct TradingState {
    jobs: HashMap<CommandId, Job>,
    mode_version: u64,
}

fn side_text(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "Buy",
        OrderSide::Sell => "Sell",
    }
}

fn tif_text(tif: TimeInForce) -> &'static str {
    match tif {
        TimeInForce::GoodTillCancel => "GTC",
        TimeInForce::ImmediateOrCancel => "IOC",
        TimeInForce::FillOrKill => "FOK",
        TimeInForce::PostOnly => "PostOnly",
    }
}

fn number(value: f64) -> String {
    num::format(value)
}

/// `^[A-Za-z0-9_-]{1,36}$`, Bybit's `orderLinkId` rule.
fn valid_client_order_id(id: &str) -> bool {
    (1..=36).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

/// The source price a trigger compares against, and its V5 name.
fn trigger_by(opts: &PlaceOrderOpts) -> Option<&'static str> {
    Some(match opts.kind {
        OrderType::Conditional => match opts.trigger_type {
            Some(StopTriggerType::MarkPrice) => "MarkPrice",
            Some(StopTriggerType::IndexPrice) => "IndexPrice",
            _ => "LastPrice",
        },
        OrderType::StopLoss => sl_trigger_by(opts),
        OrderType::TakeProfit => tp_trigger_by(opts),
        _ => return None,
    })
}

/// Stop-losses trigger on mark unless asked for last; take-profits the reverse.
fn sl_trigger_by(opts: &PlaceOrderOpts) -> &'static str {
    match opts.sl_trigger_type {
        Some(StopTriggerType::LastPrice) => "LastPrice",
        _ => "MarkPrice",
    }
}

fn tp_trigger_by(opts: &PlaceOrderOpts) -> &'static str {
    match opts.tp_trigger_type {
        Some(StopTriggerType::MarkPrice) => "MarkPrice",
        _ => "LastPrice",
    }
}

/// Bybit's message, with the Trading Terms refusal made actionable.
fn describe((code, message): (i64, String)) -> String {
    if !AGREEMENT_CODES.contains(&code) {
        return message;
    }
    format!(
        "Bybit Trading Terms not accepted for this contract. Sign the agreement from the Bybit master account before retrying. Original Bybit error: {} (retCode {code}).",
        message.trim_end_matches(['.', ' '])
    )
}

fn not_modified(message: &str) -> bool {
    message.to_ascii_lowercase().contains("not modified")
}

/// A cancel that raced a fill or another cancel: the order is gone either way.
fn already_gone(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("order not exists") || message.contains("too late to cancel")
}

fn is_virtual(id: &str) -> bool {
    id.ends_with(STOP_LOSS_SUFFIX) || id.ends_with(TAKE_PROFIT_SUFFIX)
}

fn single(path: &'static str, body: String, call: Call) -> Job {
    Job {
        steps: VecDeque::from([Step {
            delay_ms: 0.0,
            requests: vec![Planned { path, body, call }],
        }]),
        ..Job::default()
    }
}

/// One order's fields, for a batch item and, with `category`, a single create.
struct Lot {
    item: Body,
    single: Body,
}

impl BybitLinear {
    pub(crate) fn on_trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        // A settle-coin snapshot omits flat symbols. Resolve their mode before
        // planning any writes, since Bybit modes can differ by symbol.
        let unresolved = match &command {
            Command::PlaceOrders { command, orders } => orders
                .iter()
                .find(|o| !self.account.position_modes.contains_key(&o.symbol))
                .map(|o| (*command, o.symbol.clone())),
            Command::AmendOrder { command, id, .. } => cx
                .store
                .order(id)
                .filter(|o| !self.account.position_modes.contains_key(&o.symbol))
                .map(|o| (*command, o.symbol.clone())),
            _ => None,
        };
        if let Some((id, symbol)) = unresolved {
            if self.credentials.is_none() {
                cx.done(id, Err("Bybit trading needs an API key".into()));
                return;
            }
            if let Command::PlaceOrders { orders, .. } = &command
                && let Err(message) = check_client_order_ids(cx, orders)
            {
                cx.done(id, Err(message));
                return;
            }
            self.send(
                cx,
                Rest::get(
                    "/v5/position/list",
                    crate::sign::Query::new()
                        .str("category", "linear")
                        .str("symbol", &symbol),
                )
                .signed(),
                Then::Trade(Request {
                    command: id,
                    call: Call::ResolveMode {
                        symbol,
                        command: Box::new(command),
                        version: self.trading.mode_version,
                    },
                }),
            );
            return;
        }
        let (id, plan) = match command {
            Command::PlaceOrders { command, orders } => (command, self.plan_place(cx, &orders)),
            Command::CancelOrders { command, ids } => {
                let orders: Vec<Order> = ids
                    .iter()
                    .filter_map(|id| cx.store.order(id).cloned())
                    .collect();
                (command, Ok(plan_cancel(&orders)))
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (command, self.plan_amend(cx, &id, price, amount)),
            Command::CancelSymbolOrders { command, symbol } => {
                let ids = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == symbol)
                    .map(|o| o.id.clone())
                    .collect();
                let body = Body::new()
                    .str("category", "linear")
                    .str("symbol", &symbol)
                    .finish();
                let call = Call::CancelAll { ids };
                (command, Ok(single("/v5/order/cancel-all", body, call)))
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
            } => (command, plan_leverage(cx, symbol, leverage)),
            Command::SetMarginType {
                command, isolated, ..
            } => {
                let mode = if isolated {
                    "ISOLATED_MARGIN"
                } else {
                    "REGULAR_MARGIN"
                };
                let body = Body::new().str("setMarginMode", mode).finish();
                let call = Call::MarginMode { isolated };
                (
                    command,
                    Ok(single("/v5/account/set-margin-mode", body, call)),
                )
            }
            Command::SetPositionMode { command, hedged } => {
                // Linear modes switch per settle coin; USDT carries every perp but USDC's.
                let body = Body::new()
                    .str("category", "linear")
                    .str("coin", "USDT")
                    .int("mode", if hedged { 3 } else { 0 })
                    .finish();
                let call = Call::PositionMode { hedged };
                (command, Ok(single("/v5/position/switch-mode", body, call)))
            }
            _ => return,
        };
        if self.credentials.is_none() {
            cx.done(id, Err("Bybit trading needs an API key".into()));
            return;
        }
        match plan {
            Ok(job) => {
                self.trading.jobs.insert(id, job);
                self.advance(cx, id);
            }
            Err(message) => {
                cx.log(LogLevel::Error, message.clone());
                cx.done(id, Err(message));
            }
        }
    }

    /// Send the job's next step, or finish it.
    fn advance(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(job) = self.trading.jobs.get_mut(&command) else {
            return;
        };
        let stop = job.error.is_some() && !job.continue_on_error;
        let next = if stop { None } else { job.steps.front() };
        match next {
            Some(step) if step.delay_ms > 0.0 => {
                let delay = step.delay_ms;
                job.steps[0].delay_ms = 0.0;
                job.delay = Some(cx.set_timer(delay));
            }
            Some(_) => {
                let step = job.steps.pop_front().expect("checked above");
                job.outstanding = step.requests.len();
                for planned in step.requests {
                    let request = Request {
                        command,
                        call: planned.call,
                    };
                    self.send(
                        cx,
                        Rest::post(planned.path, planned.body),
                        Then::Trade(request),
                    );
                }
            }
            None => {
                let job = self.trading.jobs.remove(&command).expect("present");
                // A batch reports its failures as logs; it fails only when nothing went through.
                let result = match job.error {
                    Some(error) if job.ids.is_empty() || !job.continue_on_error => Err(error),
                    _ => Ok(serde_json::to_string(&job.ids).unwrap_or_default()),
                };
                cx.done(command, result);
            }
        }
    }

    pub(crate) fn on_trade_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        let due = self
            .trading
            .jobs
            .iter_mut()
            .find(|(_, job)| job.delay == Some(timer))
            .map(|(command, job)| {
                job.delay = None;
                *command
            });
        if let Some(command) = due {
            self.advance(cx, command);
        }
    }

    pub(crate) fn on_trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let Request { command, call } = request;
        let call = match call {
            Call::ResolveMode {
                symbol,
                command: deferred,
                version,
            } => {
                if version != self.trading.mode_version {
                    self.on_trade_command(cx, *deferred);
                    return;
                }
                let mode = response
                    .as_ref()
                    .map_err(|e| (*e).to_owned())
                    .and_then(|response| {
                        if let Some(error) = venue_error(response) {
                            return Err(describe(error));
                        }
                        let page: Listed<PositionRow<'_>> =
                            serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                        let rows = page.result.ok_or("Missing position mode result")?.list;
                        let mut mode = None;
                        for row in rows.iter().filter(|row| row.symbol == symbol) {
                            let next = match row.position_idx {
                                Some(0) => false,
                                Some(1 | 2) => true,
                                _ => return Err("Invalid position mode".into()),
                            };
                            if mode.is_some_and(|current| current != next) {
                                return Err("Conflicting position modes".into());
                            }
                            mode = Some(next);
                        }
                        mode.ok_or_else(|| format!("Position mode missing for {symbol}"))
                    });
                match mode {
                    Ok(mode) => {
                        self.account.position_modes.entry(symbol).or_insert(mode);
                        self.on_trade_command(cx, *deferred);
                    }
                    Err(message) => {
                        cx.log(LogLevel::Error, message.clone());
                        cx.done(command, Err(message));
                    }
                }
                return;
            }
            call => call,
        };
        let rejected_before_acceptance = response.as_ref().ok().is_some_and(|response| {
            (200..300).contains(&response.status)
                && serde_json::from_str::<Status>(response.body)
                    .ok()
                    .and_then(|s| s.ret_code)
                    .is_some_and(|code| code == 10001)
        });
        let outcome = match &response {
            Ok(response) => match venue_error(response) {
                None => Ok(response.body),
                Some(error) => Err(error),
            },
            Err(error) => Err((0, (*error).to_owned())),
        };
        let mut ids = Vec::new();
        let mut errors = Vec::new();
        let mut fallback_steps = Vec::new();
        match (call, outcome) {
            (Call::PlaceOne | Call::Amend, Ok(body)) => {
                if let Ok(placed) = serde_json::from_str::<Placed>(body) {
                    ids.extend(
                        placed
                            .result
                            .and_then(|r| r.order_id)
                            .filter(|id| !id.is_empty())
                            .map(|id| id.into_owned()),
                    );
                }
            }
            (Call::Amend | Call::TradingStop, Err((_, message))) if not_modified(&message) => {}
            (Call::TradingStop, Ok(_)) => {
                // No order id comes back; the stop's row arrives on the stream.
            }
            (Call::PlaceBatch { .. }, Ok(body)) => {
                let batch = serde_json::from_str::<Batch>(body).ok();
                let (results, statuses) = batch
                    .map(|b| {
                        (
                            b.result.map(|r| r.list).unwrap_or_default(),
                            b.ret_ext_info.map(|s| s.list).unwrap_or_default(),
                        )
                    })
                    .unwrap_or_default();
                for (index, status) in statuses.iter().enumerate() {
                    if status.code != 0 {
                        let message = status.msg.as_deref().unwrap_or("batch order rejected");
                        errors.push(format!(
                            "Order {}: {}",
                            index + 1,
                            describe((status.code, message.to_owned()))
                        ));
                    }
                }
                ids.extend(
                    results
                        .into_iter()
                        .filter_map(|r| r.order_id)
                        .filter(|id| !id.is_empty())
                        .map(|id| id.into_owned()),
                );
            }
            (Call::PlaceBatch { fallback }, Err(error)) if rejected_before_acceptance => {
                // Refused whole: nothing was placed, so each order may go alone.
                errors.push(format!("Failed to place batch orders: {}", describe(error)));
                fallback_steps = fallback;
            }
            (Call::Cancel { ids: requested }, Ok(body)) => {
                let statuses = serde_json::from_str::<Batch>(body)
                    .ok()
                    .and_then(|b| b.ret_ext_info)
                    .map(|s| s.list)
                    .unwrap_or_default();
                let mut canceled = Vec::new();
                for (index, id) in requested.iter().enumerate() {
                    match statuses.get(index) {
                        Some(status)
                            if status.code != 0
                                && !already_gone(status.msg.as_deref().unwrap_or_default()) =>
                        {
                            errors.push(
                                status
                                    .msg
                                    .as_deref()
                                    .unwrap_or("cancel rejected")
                                    .to_owned(),
                            );
                        }
                        _ => canceled.push(id.clone()),
                    }
                }
                mark_canceled(cx, &canceled);
            }
            (Call::Cancel { ids } | Call::CancelAll { ids }, Err((_, message)))
                if already_gone(&message) =>
            {
                mark_canceled(cx, &ids);
            }
            (Call::CancelAll { ids }, Ok(_)) => mark_canceled(cx, &ids),
            (Call::Leverage { symbol, leverage }, outcome)
                if outcome
                    .as_ref()
                    .map_or_else(|(code, _)| *code == LEVERAGE_NOT_MODIFIED, |_| true) =>
            {
                cx.store.update_setting(&symbol, |s| s.leverage = leverage);
                cx.store.update_positions(|p| {
                    if p.symbol == symbol {
                        p.leverage = leverage;
                    }
                });
            }
            (Call::MarginMode { isolated }, outcome) => {
                let body = match &outcome {
                    Ok(body) => *body,
                    Err(_) => response.as_ref().map_or("", |r| r.body),
                };
                let reasons: Vec<String> = serde_json::from_str::<MarginModeResult>(body)
                    .ok()
                    .and_then(|r| r.result)
                    .map(|r| r.reasons)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|r| r.reason_msg.map(|m| m.into_owned()))
                    .filter(|m| !m.is_empty())
                    .collect();
                // Success is retCode 0 with no reasons; the message is not "OK".
                match (outcome, reasons.is_empty()) {
                    (Ok(_), true) => {
                        let margin = if isolated {
                            MarginType::Isolated
                        } else {
                            MarginType::Cross
                        };
                        self.account.margin_type = Some(margin);
                        cx.store.update_positions(|p| p.margin_type = Some(margin));
                        self.fetch_positions(cx);
                    }
                    (outcome, _) if !reasons.is_empty() || outcome.is_ok() => {
                        errors.push(reasons.join(", "));
                    }
                    (Err(error), _) => errors.push(describe(error)),
                    (Ok(_), _) => {}
                }
            }
            (Call::PositionMode { hedged }, outcome)
                if outcome
                    .as_ref()
                    .map_or_else(|(code, _)| *code == POSITION_MODE_NOT_MODIFIED, |_| true) =>
            {
                cx.store.set_hedged(hedged);
                self.trading.mode_version += 1;
                self.refresh_after_mode_change(cx);
            }
            (_, Err(error)) => errors.push(describe(error)),
            (_, Ok(_)) => {}
        }
        for message in &errors {
            cx.log(LogLevel::Error, message.clone());
        }
        let Some(job) = self.trading.jobs.get_mut(&command) else {
            return;
        };
        if let Some(first) = errors.into_iter().next() {
            job.error.get_or_insert(first);
        }
        job.ids.extend(ids);
        if !fallback_steps.is_empty() {
            let steps = fallback_steps.into_iter().rev().map(|body| Step {
                delay_ms: 0.0,
                requests: vec![Planned {
                    path: "/v5/order/create",
                    body,
                    call: Call::PlaceOne,
                }],
            });
            for step in steps {
                job.steps.push_front(step);
            }
        }
        job.outstanding = job.outstanding.saturating_sub(1);
        if job.outstanding == 0 {
            self.advance(cx, command);
        }
    }

    /// `0` in one-way mode; in hedge mode the leg the order opens, or for a
    /// reducing order the leg it closes.
    fn position_idx(
        &self,
        _cx: &Cx<'_, Pending>,
        symbol: &str,
        side: OrderSide,
        reduce: bool,
    ) -> i64 {
        if !self
            .account
            .position_modes
            .get(symbol)
            .copied()
            .unwrap_or(false)
        {
            return 0;
        }
        match (side, reduce) {
            (OrderSide::Buy, false) | (OrderSide::Sell, true) => 1,
            (OrderSide::Sell, false) | (OrderSide::Buy, true) => 2,
        }
    }

    /// A position stop sits on the leg its order side closes.
    fn stop_position_idx(&self, cx: &Cx<'_, Pending>, symbol: &str, side: OrderSide) -> i64 {
        self.position_idx(cx, symbol, side, true)
    }

    fn full_position_opposite(cx: &Cx<'_, Pending>, opts: &PlaceOrderOpts) -> bool {
        let side = match opts.side {
            OrderSide::Buy => PositionSide::Short,
            OrderSide::Sell => PositionSide::Long,
        };
        cx.store.positions().iter().any(|p| {
            p.symbol == opts.symbol
                && p.side == side
                && p.contracts != 0.0
                && p.contracts >= opts.amount
        })
    }

    fn plan_place(&self, cx: &Cx<'_, Pending>, orders: &[PlaceOrderOpts]) -> Result<Job, String> {
        check_client_order_ids(cx, orders)?;
        let mut job = Job {
            continue_on_error: true,
            ..Job::default()
        };
        let mut lots = Vec::new();
        for opts in orders {
            let stop = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
            if stop && Self::full_position_opposite(cx, opts) {
                let body = self.position_stop(cx, opts)?;
                job.steps.push_back(trading_stop_step(body));
            } else if opts.kind == OrderType::TrailingStopLoss {
                let body = self.trailing_stop(cx, opts)?;
                job.steps.push_back(trading_stop_step(body));
            } else {
                lots.extend(self.format_lots(cx, opts)?);
            }
        }
        for (index, chunk) in lots.chunks(BATCH_LIMIT).enumerate() {
            let items: Vec<String> = chunk.iter().map(|lot| lot.item.clone().finish()).collect();
            let body = Body::new()
                .str("category", "linear")
                .raw("request", &format!("[{}]", items.join(",")))
                .finish();
            let fallback = chunk
                .iter()
                .map(|lot| lot.single.clone().finish())
                .collect();
            job.steps.push_back(Step {
                delay_ms: if index > 0 { PLACE_BATCH_DELAY_MS } else { 0.0 },
                requests: vec![Planned {
                    path: "/v5/order/create-batch",
                    body,
                    call: Call::PlaceBatch { fallback },
                }],
            });
        }
        Ok(job)
    }

    /// One order as create bodies, split into lots above the market's
    /// maximum size; only the first lot carries the attached TP/SL.
    fn format_lots(&self, cx: &Cx<'_, Pending>, opts: &PlaceOrderOpts) -> Result<Vec<Lot>, String> {
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let p_price = market.precision.price;
        let p_amount = market.precision.amount;
        let adjust_price = |p: f64| num::adjust(p, p_price);
        let amount = num::adjust(opts.amount, p_amount);
        if !amount.is_finite() || amount <= 0.0 {
            return Err("Order quantity must be finite and positive".into());
        }
        let stop = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
        if stop && amount == 0.0 {
            return Err(
                "Cannot place full stop order without existing position or specified amount".into(),
            );
        }
        let price = opts.price.filter(|p| *p != 0.0).map(adjust_price);
        // A stop with no position to attach to is a reduce-only conditional
        // market order triggering at its price.
        let trigger_price = opts
            .trigger_price
            .filter(|p| *p != 0.0)
            .or(if stop { price } else { None })
            .map(adjust_price);
        let conditional = opts.kind == OrderType::Conditional;
        if conditional && trigger_price.is_none() {
            return Err(format!(
                "Trigger price is required for conditional Bybit orders ({})",
                opts.symbol
            ));
        }
        let order_type = match opts.kind {
            OrderType::Limit => "Limit",
            OrderType::Conditional if price.is_some() => "Limit",
            _ => "Market",
        };
        let limit_price = (opts.kind == OrderType::Limit || (conditional && price.is_some()))
            .then_some(price)
            .flatten();
        let direction = match trigger_price {
            Some(trigger) => {
                let ticker = cx
                    .store
                    .ticker(&opts.symbol)
                    .ok_or_else(|| format!("Ticker {} not found", opts.symbol))?;
                let compare = match trigger_by(opts) {
                    Some("MarkPrice") => ticker.mark,
                    Some("IndexPrice") => ticker.index,
                    _ => ticker.last,
                };
                Some(if trigger > compare { "1" } else { "2" })
            }
            None => None,
        };
        let reduce_only = !conditional && opts.reduce_only == Some(true);
        let position_idx = self.position_idx(cx, &opts.symbol, opts.side, reduce_only);
        let stop_loss = opts.stop_loss.filter(|p| *p != 0.0).map(adjust_price);
        let take_profit = opts.take_profit.filter(|p| *p != 0.0).map(adjust_price);
        let tif = (limit_price.is_some())
            .then(|| tif_text(opts.time_in_force.unwrap_or(TimeInForce::GoodTillCancel)));

        let max = market.limits.amount.max;
        let mut sizes = Vec::new();
        let mut remaining = amount;
        while max > 0.0 && remaining > max {
            sizes.push(max);
            remaining = num::adjust(remaining - max, p_amount);
        }
        sizes.push(remaining);
        let lot = |qty: f64, brackets: bool, category: bool| {
            let mut body = Body::new();
            if category {
                body = body.str("category", "linear");
            }
            let sl = stop_loss.filter(|_| brackets);
            let tp = take_profit.filter(|_| brackets);
            body.str("symbol", &opts.symbol)
                .str("side", side_text(opts.side))
                .str("orderType", order_type)
                .str("qty", &number(qty))
                .opt_str("price", limit_price.map(number).as_deref())
                .opt_str("stopLoss", sl.map(number).as_deref())
                .opt_str("takeProfit", tp.map(number).as_deref())
                .bool("reduceOnly", reduce_only)
                .opt_str("triggerPrice", trigger_price.map(number).as_deref())
                .opt_str("triggerDirection", direction)
                .opt_str("triggerBy", trigger_by(opts))
                .opt_str("slTriggerBy", sl.map(|_| sl_trigger_by(opts)))
                .opt_str("tpTriggerBy", tp.map(|_| tp_trigger_by(opts)))
                .opt_str("timeInForce", tif)
                .bool("closeOnTrigger", stop)
                .int("positionIdx", position_idx)
                .opt_str("orderLinkId", opts.client_order_id.as_deref())
        };
        Ok(sizes
            .iter()
            .enumerate()
            .map(|(index, &qty)| Lot {
                item: lot(qty, index == 0, false),
                single: lot(qty, index == 0, true),
            })
            .collect())
    }

    /// A TP/SL on the whole (or part of the) open position it closes.
    fn position_stop(&self, cx: &Cx<'_, Pending>, opts: &PlaceOrderOpts) -> Result<String, String> {
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let price = opts
            .trigger_price
            .or(opts.price)
            .map(|p| num::adjust(p, market.precision.price))
            .ok_or_else(|| format!("A stop needs a price ({})", opts.symbol))?;
        let amount = num::adjust(opts.amount, market.precision.amount);
        let partial = amount != 0.0;
        let mut body = Body::new()
            .str("category", "linear")
            .str("symbol", &opts.symbol)
            .int(
                "positionIdx",
                self.stop_position_idx(cx, &opts.symbol, opts.side),
            )
            .str("tpslMode", if partial { "Partial" } else { "Full" });
        body = if opts.kind == OrderType::StopLoss {
            body.str("stopLoss", &number(price))
                .str("slTriggerBy", sl_trigger_by(opts))
                .opt_str("slSize", partial.then(|| number(amount)).as_deref())
        } else {
            body.str("takeProfit", &number(price))
                .str("tpTriggerBy", tp_trigger_by(opts))
                .opt_str("tpSize", partial.then(|| number(amount)).as_deref())
        };
        Ok(body.finish())
    }

    /// Trailing stops trail the whole position by the distance from last price.
    fn trailing_stop(&self, cx: &Cx<'_, Pending>, opts: &PlaceOrderOpts) -> Result<String, String> {
        let (Some(ticker), Some(market)) =
            (cx.store.ticker(&opts.symbol), cx.store.market(&opts.symbol))
        else {
            return Err(format!("Ticker {} not found", opts.symbol));
        };
        let target = opts
            .price
            .ok_or_else(|| format!("A trailing stop needs a price ({})", opts.symbol))?;
        let distance = num::adjust(
            ticker.last.max(target) - ticker.last.min(target),
            market.precision.price,
        );
        Ok(Body::new()
            .str("category", "linear")
            .str("symbol", &opts.symbol)
            .int(
                "positionIdx",
                self.stop_position_idx(cx, &opts.symbol, opts.side),
            )
            .str("trailingStop", &number(distance))
            .finish())
    }

    /// Bybit amends in place, so the order keeps its id and client id.
    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Job, String> {
        let order = cx
            .store
            .order(id)
            .ok_or_else(|| format!("Order {id} not found"))?;
        let market = cx
            .store
            .market(&order.symbol)
            .ok_or_else(|| format!("Market {} not found", order.symbol))?;
        let price = price.map(|p| num::adjust(p, market.precision.price));
        let amount = amount.map(|a| num::adjust(a, market.precision.amount));
        let base = || {
            Body::new()
                .str("category", "linear")
                .str("symbol", &order.symbol)
        };
        let amend = |body: Body| Ok(single("/v5/order/amend", body.finish(), Call::Amend));
        let stop = matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit);
        if is_virtual(id) {
            // An attached TP/SL is a field of its parent order.
            let price = price.ok_or("Only the price of an attached TP/SL can change")?;
            let parent = order.parent_id.as_deref().unwrap_or_default();
            let field = if id.ends_with(STOP_LOSS_SUFFIX) {
                "stopLoss"
            } else {
                "takeProfit"
            };
            return amend(base().str("orderId", parent).str(field, &number(price)));
        }
        if order.kind == OrderType::TrailingStopLoss {
            let target = price.ok_or("A trailing stop moves by price")?;
            let opts = PlaceOrderOpts {
                symbol: order.symbol.clone(),
                kind: order.kind,
                side: order.side,
                amount: order.amount,
                price: Some(target),
                trigger_price: None,
                stop_loss: None,
                take_profit: None,
                reduce_only: None,
                time_in_force: None,
                trigger_type: None,
                tp_trigger_type: None,
                sl_trigger_type: None,
                client_order_id: None,
                close: None,
                stp_mode: None,
            };
            return Ok(trading_stop_job(self.trailing_stop(cx, &opts)?));
        }
        if stop && self.account.position_bound.contains(id) && order.amount == 0.0 {
            // Full stops modify the position; Partial stops amend by order ID.
            if amount.is_some() {
                return Err("A full-position stop has no adjustable quantity".into());
            }
            let price = price.ok_or("A full-position stop moves by price")?;
            let (level, by) = if order.kind == OrderType::StopLoss {
                ("stopLoss", "slTriggerBy")
            } else {
                ("takeProfit", "tpTriggerBy")
            };
            let by_value = match order.trigger_type {
                Some(StopTriggerType::MarkPrice) => Some("MarkPrice"),
                Some(StopTriggerType::LastPrice) => Some("LastPrice"),
                Some(StopTriggerType::IndexPrice) => Some("IndexPrice"),
                None => None,
            };
            let body = Body::new()
                .str("category", "linear")
                .str("symbol", &order.symbol)
                .int(
                    "positionIdx",
                    self.stop_position_idx(cx, &order.symbol, order.side),
                )
                .str("tpslMode", "Full")
                .str(level, &number(price))
                .opt_str(by, by_value);
            return Ok(trading_stop_job(body.finish()));
        }
        let conditional = stop || order.kind == OrderType::Conditional;
        if order.kind != OrderType::Limit && !conditional {
            return Err(format!("A {:?} order cannot be amended", order.kind));
        }
        let price_field = if conditional { "triggerPrice" } else { "price" };
        amend(
            base()
                .str("orderId", &order.id)
                .opt_str("qty", amount.map(number).as_deref())
                .opt_str(price_field, price.map(number).as_deref()),
        )
    }
}

fn trading_stop_step(body: String) -> Step {
    Step {
        delay_ms: 0.0,
        requests: vec![Planned {
            path: "/v5/position/trading-stop",
            body,
            call: Call::TradingStop,
        }],
    }
}

fn trading_stop_job(body: String) -> Job {
    single("/v5/position/trading-stop", body, Call::TradingStop)
}

/// Cancel by id, ten per batch; attached TP/SL rows are not venue orders
/// and go with their parent.
fn plan_cancel(orders: &[Order]) -> Job {
    let mut job = Job::default();
    let real: Vec<&Order> = orders.iter().filter(|o| !is_virtual(&o.id)).collect();
    for (index, chunk) in real.chunks(CANCEL_BATCH_LIMIT).enumerate() {
        let items: Vec<String> = chunk
            .iter()
            .map(|o| {
                Body::new()
                    .str("symbol", &o.symbol)
                    .str("orderId", &o.id)
                    .finish()
            })
            .collect();
        let body = Body::new()
            .str("category", "linear")
            .raw("request", &format!("[{}]", items.join(",")))
            .finish();
        job.steps.push_back(Step {
            delay_ms: if index > 0 {
                CANCEL_BATCH_DELAY_MS
            } else {
                0.0
            },
            requests: vec![Planned {
                path: "/v5/order/cancel-batch",
                body,
                call: Call::Cancel {
                    ids: chunk.iter().map(|o| o.id.clone()).collect(),
                },
            }],
        });
    }
    job
}

fn plan_leverage(cx: &Cx<'_, Pending>, symbol: String, input: f64) -> Result<Job, String> {
    let market = cx
        .store
        .market(&symbol)
        .ok_or_else(|| format!("Market {symbol} not found"))?;
    let leverage = input
        .max(market.limits.leverage.min)
        .min(market.limits.leverage.max);
    if cx.store.setting(&symbol).map(|s| s.leverage) == Some(leverage) {
        return Ok(Job::default());
    }
    let value = number(leverage);
    let body = Body::new()
        .str("category", "linear")
        .str("symbol", &symbol)
        .str("buyLeverage", &value)
        .str("sellLeverage", &value)
        .finish();
    Ok(single(
        "/v5/position/set-leverage",
        body,
        Call::Leverage { symbol, leverage },
    ))
}

/// Mark accepted cancels; an order that already finished keeps its status.
fn mark_canceled(cx: &mut Cx<'_, Pending>, ids: &[String]) {
    let now = cx.now;
    for id in ids {
        for id in [
            id.clone(),
            format!("{id}{STOP_LOSS_SUFFIX}"),
            format!("{id}{TAKE_PROFIT_SUFFIX}"),
        ] {
            if cx
                .store
                .order(&id)
                .is_some_and(|o| o.status == OrderStatus::Open)
            {
                cx.store.update_order(&id, |order| {
                    order.status = OrderStatus::Canceled;
                    order.last_update = now;
                });
            }
        }
    }
}

/// A caller's client order ID is honored verbatim on exactly one plain order,
/// or the whole batch is refused before anything is sent.
fn check_client_order_ids(cx: &Cx<'_, Pending>, orders: &[PlaceOrderOpts]) -> Result<(), String> {
    let mut seen = Vec::new();
    for opts in orders {
        let Some(id) = opts.client_order_id.as_deref() else {
            continue;
        };
        let plain = matches!(opts.kind, OrderType::Limit | OrderType::Market)
            && opts.trigger_price.is_none()
            && opts.stop_loss.is_none()
            && opts.take_profit.is_none();
        if !plain {
            return Err(format!(
                "Client order ID {id} can only be used on a plain limit or market order"
            ));
        }
        if !valid_client_order_id(id) {
            return Err(format!(
                "Client order ID {id} is not valid for Bybit (1-36 of A-Z a-z 0-9 _ -)"
            ));
        }
        if seen.contains(&id) {
            return Err(format!("Client order ID {id} is used twice in one batch"));
        }
        if let Some(market) = cx.store.market(&opts.symbol)
            && num::adjust(opts.amount, market.precision.amount) > market.limits.amount.max
        {
            return Err(format!(
                "Client order ID needs a single order; {} exceeds the maximum size",
                opts.amount
            ));
        }
        seen.push(id);
    }
    Ok(())
}
