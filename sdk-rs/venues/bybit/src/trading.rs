//! Order entry. Each command becomes a job of sequential steps; a step's
//! requests go out together and the next step starts when they all answer.
//! Attached stop-loss and take-profit ride on the order itself; stops against
//! an open position and trailing stops go through `/v5/position/trading-stop`.
// Shared with Hyro's adoption (hyro.rs).
pub(crate) mod acknowledgement;
pub(crate) mod codes;
pub(crate) mod ordinary;

use crate::sign::Body;
use crate::wire::*;
use crate::{BybitLinear, Category, Pending};
use safe_cex_engine::model::{
    Order, OrderIdentityKind, OrderIdentityReceipt, OrderSide, OrderStatus, OrderType,
    PlaceOrderOpts, PositionSide, StopTriggerType, TimeInForce,
};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{Command, CommandId, Cx, ErrorKind, Outcome, SocketId, VenueError, num};
use std::collections::{HashMap, VecDeque};

/// Orders per `create-batch` request (Bybit's non-VIP limit).
const BATCH_LIMIT: usize = 10;
/// Pacing between batches, inside the per-second order limits.
const PLACE_BATCH_DELAY_MS: f64 = 1_200.0;
const CANCEL_BATCH_DELAY_MS: f64 = 1_000.0;
const CANCEL_BATCH_LIMIT: usize = 10;
const AGREEMENT_CODES: [i64; 2] = [110_123, 110_125];
const STOP_LOSS_SUFFIX: &str = "__stop_loss";
const TAKE_PROFIT_SUFFIX: &str = "__take_profit";

struct LocalWrite {
    path: &'static str,
    body: String,
    before: Vec<Order>,
    socket: Option<SocketId>,
    snapshot: u32,
}

/// What a response means for its job.
enum Call {
    PlaceOne,
    /// Single-order bodies to place one by one if the venue refuses the
    /// whole batch (a refusal places nothing, so this cannot double-place).
    PlaceBatch {
        fallback: Vec<(String, Vec<OrderIdentityReceipt>)>,
    },
    TradingStop,
    PartialStop,
    Amend,
    Cancel {
        ids: Vec<String>,
    },
    CancelPartialStop {
        id: String,
        batch: bool,
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
    receipts: Vec<OrderIdentityReceipt>,
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
    /// Run every step even after a failure (independent orders of one batch).
    continue_on_error: bool,
}

#[derive(Default)]
pub struct TradingState {
    ordinary: HashMap<CommandId, ordinary::Ordinary>,
    mode_version: u64,
    /// Settles cancel-alls whose order read has landed.
    unlisted_timer: Option<safe_cex_engine::TimerId>,
    pub(crate) hyro: crate::hyro::State,
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

fn stop_trigger_price(opts: &PlaceOrderOpts, precision: f64) -> Result<f64, String> {
    opts.trigger_price
        .filter(|price| *price != 0.0)
        .or(opts.price.filter(|price| *price != 0.0))
        .map(|price| num::adjust(price, precision))
        .filter(|price| price.is_finite() && *price > 0.0)
        .ok_or_else(|| {
            format!(
                "A stop needs a finite positive trigger price ({})",
                opts.symbol
            )
        })
}

fn validate_stop_triggers(cx: &Cx<'_, Pending>, orders: &[PlaceOrderOpts]) -> Result<(), String> {
    for opts in orders {
        if matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit) {
            let market = cx
                .store
                .market(&opts.symbol)
                .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
            stop_trigger_price(opts, market.precision.price)?;
        }
    }
    Ok(())
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

fn is_virtual(id: &str) -> bool {
    id.ends_with(STOP_LOSS_SUFFIX) || id.ends_with(TAKE_PROFIT_SUFFIX)
}

fn single(path: &'static str, body: String, call: Call) -> Job {
    Job {
        steps: VecDeque::from([Step {
            delay_ms: 0.0,
            requests: vec![Planned {
                path,
                body,
                call,
                receipts: Vec::new(),
            }],
        }]),
        ..Job::default()
    }
}

/// One order's fields, for a batch item and, with `category`, a single create.
pub(crate) struct Lot {
    receipts: Vec<OrderIdentityReceipt>,
    category: Category,
    item: Body,
    pub(crate) single: Body,
}

impl BybitLinear {
    fn local_order_lifecycle(&self) -> bool {
        self.variant == crate::Variant::Demo || self.variant.is_hyro() || self.brazil_site
    }

    fn variant_control(&self, command: &Command) -> Option<(CommandId, Result<(), String>)> {
        if self.variant.is_spot() || self.category == Category::Option {
            let id = match command {
                Command::SetLeverage { command, .. }
                | Command::SetMarginType { command, .. }
                | Command::SetPositionMode { command, .. } => *command,
                _ => return None,
            };
            let message = if self.category == Category::Option {
                "Bybit options have no leverage, margin or position-mode settings"
            } else {
                "Bybit spot has no derivative position settings"
            };
            return Some((id, Err(message.into())));
        }
        if self.variant.is_hyro() {
            match command {
                Command::SetPositionMode {
                    command,
                    hedged,
                    symbol: _,
                } => {
                    return Some((
                        *command,
                        if *hedged {
                            Err("HyroTrader supports one-way mode only".into())
                        } else {
                            Ok(())
                        },
                    ));
                }
                Command::SetMarginType {
                    command, isolated, ..
                } => {
                    return Some((
                        *command,
                        if *isolated {
                            Err("HyroTrader supports cross margin only".into())
                        } else {
                            Ok(())
                        },
                    ));
                }
                _ => {}
            }
        }
        None
    }

    pub(crate) fn check_ids(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[PlaceOrderOpts],
    ) -> Result<(), String> {
        if self.variant.is_hyro() && orders.iter().any(|o| o.client_order_id.is_some()) {
            return Err("HyroTrader does not support caller client order IDs".into());
        }
        check_client_order_ids(cx, orders)
    }

    pub(crate) fn on_trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        // Bybit cancels by order id only; refuse rather than leave it open.
        if matches!(command, Command::CancelOrdersWithSymbols { .. }) {
            let policy = Policy {
                operation: Operation::Mutation,
                credential: None,
                timeout_ms: 1_000,
                pacing: None,
                retry: RetryPolicy::Never,
            };
            if let Ok(root) = cx.admit_http_command(policy) {
                let refusal = not_sent("Bybit has no symbol-qualified cancel; use cancelOrders")
                    .kind(ErrorKind::Unsupported);
                let _ = cx.fail_context(&root, refusal);
            }
            return;
        }
        if self.ordinary_command(&command) {
            self.start_ordinary(cx, command);
        }
    }

    /// The native job for one trade command once its preconditions hold;
    /// `None` for commands Bybit trading does not plan.
    fn plan_job(&self, cx: &Cx<'_, Pending>, command: &Command) -> Option<Result<Job, String>> {
        Some(match command {
            Command::PlaceOrders { orders, .. } => self.plan_place(cx, orders),
            Command::CancelOrders { ids, .. } => ids
                .iter()
                .map(|id| {
                    cx.store.order(id).cloned().ok_or_else(|| {
                        format!("Order {id} not found; cancellation requires its symbol")
                    })
                })
                .collect::<Result<Vec<Order>, String>>()
                .map(|orders| self.plan_cancel(&orders)),
            Command::AmendOrder {
                id, price, amount, ..
            } => self.plan_amend(cx, id, *price, *amount),
            Command::CancelSymbolOrders { symbol, .. } => {
                if self.variant.is_hyro() {
                    let orders: Vec<Order> = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| o.symbol == *symbol)
                        .cloned()
                        .collect();
                    return Some(Ok(self.plan_cancel(&orders)));
                }
                if self.variant.is_spot() {
                    let mut job = Job {
                        continue_on_error: true,
                        ..Job::default()
                    };
                    // Spot defaults to ordinary orders; every trigger family
                    // needs its own cancel, even when absent from our snapshot.
                    for filter in ["Order", "StopOrder", "tpslOrder"] {
                        let ids = cx
                            .store
                            .orders()
                            .iter()
                            .filter(|o| {
                                o.symbol == *symbol
                                    && !is_virtual(&o.id)
                                    && spot_order_filter(o.kind) == filter
                            })
                            .map(|o| o.id.clone())
                            .collect();
                        job.steps.push_back(Step {
                            delay_ms: 0.0,
                            requests: vec![Planned {
                                receipts: Vec::new(),
                                path: "/v5/order/cancel-all",
                                body: Body::new()
                                    .str("category", self.paths.category)
                                    .str("symbol", symbol)
                                    .str("orderFilter", filter)
                                    .finish(),
                                call: Call::CancelAll { ids },
                            }],
                        });
                    }
                    Ok(job)
                } else {
                    let ids = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| o.symbol == *symbol && !is_virtual(&o.id))
                        .map(|o| o.id.clone())
                        .collect();
                    let body = Body::new()
                        .str("category", self.paths.category)
                        .str("symbol", symbol)
                        .finish();
                    Ok(single(
                        "/v5/order/cancel-all",
                        body,
                        Call::CancelAll { ids },
                    ))
                }
            }
            Command::SetLeverage {
                symbol, leverage, ..
            } => plan_leverage(cx, self.paths.category, symbol.clone(), *leverage),
            Command::SetMarginType { isolated, .. } => {
                let mode = if *isolated {
                    "ISOLATED_MARGIN"
                } else {
                    "REGULAR_MARGIN"
                };
                let body = Body::new().str("setMarginMode", mode).finish();
                let call = Call::MarginMode {
                    isolated: *isolated,
                };
                Ok(single("/v5/account/set-margin-mode", body, call))
            }
            Command::SetPositionMode { hedged, .. } => {
                let hedged = *hedged;
                if self.category == Category::Inverse {
                    let mut coins = std::collections::BTreeSet::new();
                    for market in cx.store.markets() {
                        coins.insert(market.base.clone());
                    }
                    let mut job = Job::default();
                    for coin in coins {
                        let body = Body::new()
                            .str("category", self.paths.category)
                            .str("coin", &coin)
                            .int("mode", if hedged { 3 } else { 0 })
                            .finish();
                        job.steps.push_back(Step {
                            delay_ms: 0.0,
                            requests: vec![Planned {
                                receipts: Vec::new(),
                                path: "/v5/position/switch-mode",
                                body,
                                call: Call::PositionMode { hedged },
                            }],
                        });
                    }
                    if job.steps.is_empty() {
                        return Some(Err(
                            "No inverse markets loaded for position-mode switch".into()
                        ));
                    }
                    return Some(Ok(job));
                }
                // Linear modes switch per settle coin; USDT carries every perp but USDC's.
                let body = Body::new()
                    .str("category", self.paths.category)
                    .str("coin", "USDT")
                    .int("mode", if hedged { 3 } else { 0 })
                    .finish();
                Ok(single(
                    "/v5/position/switch-mode",
                    body,
                    Call::PositionMode { hedged },
                ))
            }
            _ => return None,
        })
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
        if self.variant.is_spot() || self.variant.is_hyro() {
            return 0;
        }
        if !self.symbol_mode(symbol).unwrap_or(false) {
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

    fn full_position_opposite(&self, cx: &Cx<'_, Pending>, opts: &PlaceOrderOpts) -> bool {
        let side = match opts.side {
            OrderSide::Buy => PositionSide::Short,
            OrderSide::Sell => PositionSide::Long,
        };
        cx.store.positions().iter().any(|p| {
            p.symbol == opts.symbol
                && p.side == side
                && p.contracts != 0.0
                && if self.category_for(&opts.symbol) == Category::Inverse {
                    p.notional
                } else {
                    p.contracts
                } >= opts.amount
        })
    }

    fn plan_place(&self, cx: &Cx<'_, Pending>, orders: &[PlaceOrderOpts]) -> Result<Job, String> {
        self.check_ids(cx, orders)?;
        let mut job = Job {
            continue_on_error: true,
            ..Job::default()
        };
        let mut lots = Vec::new();
        for opts in orders {
            let category = self.category_for(&opts.symbol);
            if !self.category_enabled(category) {
                return Err("Bybit product is not enabled".into());
            }
            if category == Category::Option {
                for lot in self.format_option(cx, opts)? {
                    job.steps.push_back(Step {
                        delay_ms: 0.0,
                        requests: vec![Planned {
                            receipts: lot.receipts,
                            path: "/v5/order/create",
                            body: lot.single.finish(),
                            call: Call::PlaceOne,
                        }],
                    });
                }
                continue;
            }
            execution_params(opts, self.variant.is_spot(), self.variant.is_hyro())?;
            if !opts.amount.is_finite() || opts.amount < 0.0 {
                return Err("Order quantity must be finite and nonnegative".into());
            }
            let stop = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
            if self.variant.is_spot() && opts.kind == OrderType::TrailingStopLoss {
                return Err("Bybit spot has no trailing-position stops".into());
            }
            if !self.variant.is_spot() && stop && self.full_position_opposite(cx, opts) {
                let body = self.position_stop(cx, opts)?;
                job.steps.push_back(trading_stop_step(body));
            } else if opts.kind == OrderType::TrailingStopLoss {
                let body = self.trailing_stop(cx, opts)?;
                job.steps.push_back(trading_stop_step(body));
            } else if opts.max_slippage_pct.is_some()
                || opts.bybit_bbo.is_some()
                || opts.bybit_slippage_tolerance_ticks.is_some()
            {
                for lot in self.format_lots(cx, opts)? {
                    job.steps.push_back(Step {
                        delay_ms: 0.0,
                        requests: vec![Planned {
                            receipts: lot.receipts,
                            path: "/v5/order/create",
                            body: lot.single.finish(),
                            call: Call::PlaceOne,
                        }],
                    });
                }
            } else {
                lots.extend(self.format_lots(cx, opts)?);
            }
        }
        if !self.paths.batch_orders {
            for lot in lots {
                job.steps.push_back(Step {
                    delay_ms: 0.0,
                    requests: vec![Planned {
                        receipts: lot.receipts,
                        path: "/v5/order/create",
                        body: lot.single.finish(),
                        call: Call::PlaceOne,
                    }],
                });
            }
            return Ok(job);
        }
        for category in self.categories() {
            let category_lots: Vec<&Lot> =
                lots.iter().filter(|lot| lot.category == category).collect();
            for (index, chunk) in category_lots.chunks(BATCH_LIMIT).enumerate() {
                let items: Vec<String> =
                    chunk.iter().map(|lot| lot.item.clone().finish()).collect();
                let body = Body::new()
                    .str("category", category.as_str())
                    .raw("request", &format!("[{}]", items.join(",")))
                    .finish();
                let fallback = chunk
                    .iter()
                    .map(|lot| (lot.single.clone().finish(), lot.receipts.clone()))
                    .collect();
                job.steps.push_back(Step {
                    delay_ms: if index > 0 { PLACE_BATCH_DELAY_MS } else { 0.0 },
                    requests: vec![Planned {
                        receipts: chunk.iter().flat_map(|lot| lot.receipts.clone()).collect(),
                        path: "/v5/order/create-batch",
                        body,
                        call: Call::PlaceBatch { fallback },
                    }],
                });
            }
        }
        Ok(job)
    }

    /// One order as create bodies, split into lots above the market's
    /// maximum size; only the first lot carries the attached TP/SL.
    pub(crate) fn hyro_stop(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Option<String>, String> {
        execution_params(opts, false, true)?;
        if !opts.amount.is_finite() || opts.amount < 0.0 {
            return Err("Order quantity must be finite and nonnegative".into());
        }
        if opts.kind == OrderType::TrailingStopLoss {
            return self.trailing_stop(cx, opts).map(Some);
        }
        if matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && self.full_position_opposite(cx, opts)
        {
            return self.position_stop(cx, opts).map(Some);
        }
        Ok(None)
    }

    pub(crate) fn format_lots(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Vec<Lot>, String> {
        let category = self.category_for(&opts.symbol);
        if !self.category_enabled(category) {
            return Err("Bybit product is not enabled".into());
        }
        if category == Category::Option {
            return self.format_option(cx, opts);
        }
        if opts.implied_volatility.is_some() {
            return Err("impliedVolatility is supported only on Bybit options".into());
        }
        let execution = execution_params(opts, self.variant.is_spot(), self.variant.is_hyro())?;
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let p_price = market.precision.price;
        let p_amount = market.precision.amount;
        let adjust_price = |p: f64| num::adjust(p, p_price);
        let amount = if self.category_for(&opts.symbol) == Category::Inverse {
            opts.amount.round()
        } else {
            num::adjust(opts.amount, p_amount)
        };
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
        let trigger_price = if stop {
            Some(stop_trigger_price(opts, p_price)?)
        } else {
            opts.trigger_price.filter(|p| *p != 0.0).map(adjust_price)
        };
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
        let limit_price = (execution.bbo_side.is_none()
            && (opts.kind == OrderType::Limit || (conditional && price.is_some())))
        .then_some(price)
        .flatten();
        let direction = match trigger_price.filter(|_| !self.variant.is_spot()) {
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
        if self.variant.is_spot() && opts.reduce_only == Some(true) {
            return Err("Bybit spot does not support reduceOnly".into());
        }
        if self.variant.is_spot()
            && (opts.stop_loss.is_some() || opts.take_profit.is_some())
            && opts.kind != OrderType::Limit
        {
            return Err("Bybit spot attached TP/SL requires a limit order".into());
        }
        let reduce_only = !conditional && opts.reduce_only == Some(true);
        let position_idx = self.position_idx(cx, &opts.symbol, opts.side, reduce_only);
        let stop_loss = opts.stop_loss.filter(|p| *p != 0.0).map(adjust_price);
        let take_profit = opts.take_profit.filter(|p| *p != 0.0).map(adjust_price);
        let tif = (limit_price.is_some() || execution.bbo_side.is_some())
            .then(|| tif_text(opts.time_in_force.unwrap_or(TimeInForce::GoodTillCancel)));

        let max = market.limits.amount.max;
        if !max.is_finite() || max <= 0.0 {
            return Err("Market has no valid maximum order quantity".into());
        }
        if self.category_for(&opts.symbol) == Category::Inverse && amount < market.limits.amount.min
        {
            return Err("Inverse order quantity is below the market minimum".into());
        }
        if [price, trigger_price, stop_loss, take_profit]
            .into_iter()
            .flatten()
            .any(|p| !p.is_finite() || p <= 0.0)
        {
            return Err("Order prices must be finite and positive".into());
        }
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
                body = body.str("category", self.category_for(&opts.symbol).as_str());
            }
            let sl = stop_loss.filter(|_| brackets);
            let tp = take_profit.filter(|_| brackets);
            let body = body
                .str("symbol", &opts.symbol)
                .str("side", side_text(opts.side))
                .str("orderType", order_type)
                .str("qty", &number(qty))
                .opt_str("price", limit_price.map(number).as_deref())
                .opt_str("stopLoss", sl.map(number).as_deref())
                .opt_str("takeProfit", tp.map(number).as_deref());
            let body = if self.variant.is_spot() {
                body
            } else {
                body.bool("reduceOnly", reduce_only)
            };
            let body = body
                .opt_str("triggerPrice", trigger_price.map(number).as_deref())
                .opt_str("triggerDirection", direction)
                .opt_str(
                    "triggerBy",
                    (!self.variant.is_spot())
                        .then(|| trigger_by(opts))
                        .flatten(),
                )
                .opt_str(
                    "slTriggerBy",
                    sl.filter(|_| !self.variant.is_spot())
                        .map(|_| sl_trigger_by(opts)),
                )
                .opt_str(
                    "tpTriggerBy",
                    tp.filter(|_| !self.variant.is_spot())
                        .map(|_| tp_trigger_by(opts)),
                )
                .opt_str("timeInForce", tif)
                .opt_str("bboSideType", execution.bbo_side)
                .opt_str("bboLevel", execution.bbo_level.as_deref())
                .opt_str("slippageToleranceType", execution.slippage_kind)
                .opt_str("slippageTolerance", execution.slippage.as_deref());
            let body = if self.variant.is_spot() {
                body.opt_str("marketUnit", (order_type == "Market").then_some("baseCoin"))
                    .opt_str(
                        "orderFilter",
                        trigger_price.map(|_| if stop { "tpslOrder" } else { "StopOrder" }),
                    )
                    .opt_str("slOrderType", sl.map(|_| "Market"))
                    .opt_str("tpOrderType", tp.map(|_| "Market"))
            } else {
                body.bool("closeOnTrigger", stop)
                    .int("positionIdx", position_idx)
            };
            body.opt_str("orderLinkId", opts.client_order_id.as_deref())
        };
        Ok(sizes
            .iter()
            .enumerate()
            .map(|(index, &qty)| Lot {
                receipts: opts
                    .client_order_id
                    .iter()
                    .map(|identity| OrderIdentityReceipt {
                        symbol: opts.symbol.clone(),
                        kind: OrderIdentityKind::ClientOrderId,
                        identity: identity.clone(),
                    })
                    .collect(),
                category: self.category_for(&opts.symbol),
                item: lot(qty, index == 0, false),
                single: lot(qty, index == 0, true),
            })
            .collect())
    }

    fn format_option(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Vec<Lot>, String> {
        if !self.category_enabled(Category::Option) {
            return Err("Options are not enabled on this Bybit account".into());
        }
        if !matches!(opts.kind, OrderType::Limit | OrderType::Market)
            || opts.trigger_price.is_some()
            || opts.stop_loss.is_some()
            || opts.take_profit.is_some()
            || opts.max_slippage_pct.is_some()
            || opts.bybit_bbo.is_some()
            || opts.bybit_slippage_tolerance_ticks.is_some()
            || opts.close.is_some()
            || opts.stp_mode.is_some()
        {
            return Err("Bybit options support plain limit/market orders without execution controls or protection".into());
        }
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let amount = num::adjust(opts.amount, market.precision.amount);
        if !amount.is_finite()
            || amount < market.limits.amount.min
            || amount <= 0.0
            || amount > market.limits.amount.max
        {
            return Err("Option quantity is outside the market limits".into());
        }
        if opts
            .implied_volatility
            .is_some_and(|iv| !iv.is_finite() || iv <= 0.0)
        {
            return Err("Implied volatility must be finite and positive".into());
        }
        if opts.kind == OrderType::Market && opts.implied_volatility.is_some() {
            return Err("Bybit option implied volatility requires a limit order".into());
        }
        let price = opts
            .price
            .map(|price| num::adjust(price, market.precision.price));
        if opts.kind == OrderType::Limit
            && opts.implied_volatility.is_none()
            && price.is_none_or(|price| !price.is_finite() || price <= 0.0)
        {
            return Err("A limit option needs a price or implied volatility".into());
        }
        let generated = if opts.client_order_id.is_none() {
            Some(safe_cex_crypto::uuid::uuid_with_entropy(|bytes| {
                cx.fill_entropy(bytes)
            })?)
        } else {
            None
        };
        let body = Body::new()
            .str("category", "option")
            .str("symbol", &opts.symbol)
            .str("side", side_text(opts.side))
            .str(
                "orderType",
                if opts.kind == OrderType::Limit {
                    "Limit"
                } else {
                    "Market"
                },
            )
            .str("qty", &number(amount))
            .opt_str(
                "price",
                price
                    .filter(|_| opts.kind == OrderType::Limit && opts.implied_volatility.is_none())
                    .map(number)
                    .as_deref(),
            )
            .opt_str("orderIv", opts.implied_volatility.map(number).as_deref())
            .opt_str(
                "timeInForce",
                (opts.kind == OrderType::Limit)
                    .then(|| tif_text(opts.time_in_force.unwrap_or(TimeInForce::GoodTillCancel))),
            )
            .bool("reduceOnly", opts.reduce_only == Some(true))
            .str(
                "orderLinkId",
                opts.client_order_id
                    .as_deref()
                    .or(generated.as_deref())
                    .expect("option identity"),
            );
        Ok(vec![Lot {
            receipts: vec![OrderIdentityReceipt {
                symbol: opts.symbol.clone(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: opts
                    .client_order_id
                    .as_ref()
                    .or(generated.as_ref())
                    .expect("option identity")
                    .clone(),
            }],
            category: Category::Option,
            item: body.clone(),
            single: body,
        }])
    }

    /// A TP/SL on the whole (or part of the) open position it closes.
    fn position_stop(&self, cx: &Cx<'_, Pending>, opts: &PlaceOrderOpts) -> Result<String, String> {
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let price = stop_trigger_price(opts, market.precision.price)?;
        let amount = num::adjust(opts.amount, market.precision.amount);
        let partial = amount != 0.0;
        let mut body = Body::new()
            .str("category", self.category_for(&opts.symbol).as_str())
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
            .str("category", self.category_for(&opts.symbol).as_str())
            .str("symbol", &opts.symbol)
            .int(
                "positionIdx",
                self.stop_position_idx(cx, &opts.symbol, opts.side),
            )
            .str("trailingStop", &number(distance))
            .finish())
    }

    fn plan_partial_stop_amend(
        &self,
        cx: &Cx<'_, Pending>,
        order: &Order,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Job, String> {
        if order.status != OrderStatus::Open {
            return Err("Only an open Partial position stop can be amended".into());
        }
        if price.is_some_and(|value| !value.is_finite() || value <= 0.0)
            || amount.is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err("Amended stop price must be positive and quantity nonnegative".into());
        }
        let mark = cx
            .store
            .ticker(&order.symbol)
            .map(|ticker| ticker.mark)
            .filter(|mark| mark.is_finite() && *mark > 0.0);
        let base = || {
            Body::new()
                .str("category", self.category_for(&order.symbol).as_str())
                .str("symbol", &order.symbol)
        };
        if let Some(mark) =
            mark.filter(|_| self.variant != crate::Variant::Inverse || price.is_some())
        {
            // Projection and the replacement price deliberately use the raw drag value.
            let price = price.unwrap_or(order.price);
            if !price.is_finite() || price <= 0.0 {
                return Err("A Partial position stop needs a positive replacement price".into());
            }
            let kind = match order.side {
                OrderSide::Sell if price > mark => OrderType::TakeProfit,
                OrderSide::Buy if price <= mark => OrderType::TakeProfit,
                _ => OrderType::StopLoss,
            };
            let market = cx
                .store
                .market(&order.symbol)
                .ok_or_else(|| format!("Market {} not found", order.symbol))?;
            let amount = num::adjust(amount.unwrap_or(order.amount), market.precision.amount);
            if !amount.is_finite() || amount < 0.0 {
                return Err(
                    "A Partial position stop needs a nonnegative replacement quantity".into(),
                );
            }
            let partial = amount != 0.0;
            let trigger = (kind == order.kind).then_some(order.trigger_type).flatten();
            let (level, by, size, reference) = if kind == OrderType::StopLoss {
                (
                    "stopLoss",
                    "slTriggerBy",
                    "slSize",
                    if trigger == Some(StopTriggerType::LastPrice) {
                        "LastPrice"
                    } else {
                        "MarkPrice"
                    },
                )
            } else {
                (
                    "takeProfit",
                    "tpTriggerBy",
                    "tpSize",
                    if trigger == Some(StopTriggerType::MarkPrice) {
                        "MarkPrice"
                    } else {
                        "LastPrice"
                    },
                )
            };
            let body = base()
                .int(
                    "positionIdx",
                    self.stop_position_idx(cx, &order.symbol, order.side),
                )
                .str("tpslMode", if partial { "Partial" } else { "Full" })
                .str(level, &number(price))
                .str(by, reference)
                .opt_str(size, partial.then(|| number(amount)).as_deref())
                .finish();
            let mut job = self.plan_cancel(std::slice::from_ref(order));
            for planned in job.steps.iter_mut().flat_map(|step| &mut step.requests) {
                planned.call = Call::CancelPartialStop {
                    id: order.id.clone(),
                    batch: self.paths.batch_orders,
                };
            }
            job.steps.push_back(Step {
                delay_ms: 0.0,
                requests: vec![Planned {
                    receipts: Vec::new(),
                    path: "/v5/position/trading-stop",
                    body,
                    call: Call::PartialStop,
                }],
            });
            return Ok(job);
        }
        // Missing mark (or inverse amount-only): TS updates the original leg directly.
        let level = if order.kind == OrderType::StopLoss {
            "stopLoss"
        } else {
            "takeProfit"
        };
        let size = if order.kind == OrderType::StopLoss {
            "slSize"
        } else {
            "tpSize"
        };
        let mut body = base().int(
            "positionIdx",
            self.position_idx(cx, &order.symbol, order.side, order.reduce_only),
        );
        if self.variant != crate::Variant::Inverse {
            body = body.str("tpslMode", "Partial");
        }
        body = body.opt_str(level, price.map(number).as_deref());
        if self.variant != crate::Variant::Inverse {
            body = body.str(size, &number(amount.unwrap_or(order.amount)));
        }
        Ok(trading_stop_job(body.finish()))
    }

    /// Partial position stops cancel/recreate; ordinary orders amend in place.
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
        let stop = matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit);
        if stop
            && !is_virtual(id)
            && self.category_for(&order.symbol) != Category::Spot
            && self.category_for(&order.symbol) != Category::Option
            && self.account.position_bound.contains(id)
            && order.amount != 0.0
        {
            return self.plan_partial_stop_amend(cx, order, price, amount);
        }
        let price = price.map(|p| num::adjust(p, market.precision.price));
        let amount = amount.map(|a| {
            if self.category_for(&order.symbol) == Category::Inverse {
                a.round()
            } else {
                num::adjust(a, market.precision.amount)
            }
        });
        if amount.is_some_and(|a| !a.is_finite() || a <= 0.0)
            || price.is_some_and(|p| !p.is_finite() || p <= 0.0)
        {
            return Err("Amended price and quantity must be finite and positive".into());
        }
        let base = || {
            Body::new()
                .str("category", self.category_for(&order.symbol).as_str())
                .str("symbol", &order.symbol)
        };
        let amend = |body: Body| Ok(single("/v5/order/amend", body.finish(), Call::Amend));
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
                implied_volatility: None,
                max_slippage_pct: None,
                bybit_bbo: None,
                bybit_slippage_tolerance_ticks: None,
            };
            return Ok(trading_stop_job(self.trailing_stop(cx, &opts)?));
        }
        if stop && self.account.position_bound.contains(id) && order.amount == 0.0 {
            // Full stops keep their existing position update path.
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
                .str("category", self.category_for(&order.symbol).as_str())
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
            receipts: Vec::new(),
            path: "/v5/position/trading-stop",
            body,
            call: Call::TradingStop,
        }],
    }
}

fn trading_stop_job(body: String) -> Job {
    single("/v5/position/trading-stop", body, Call::TradingStop)
}

fn spot_order_filter(kind: OrderType) -> &'static str {
    match kind {
        OrderType::StopLoss | OrderType::TakeProfit => "tpslOrder",
        OrderType::Conditional => "StopOrder",
        _ => "Order",
    }
}

/// Cancel by id, ten per batch; attached TP/SL rows are not venue orders
/// and go with their parent.
impl BybitLinear {
    fn plan_cancel(&self, orders: &[Order]) -> Job {
        let mut job = Job::default();
        let real: Vec<&Order> = orders.iter().filter(|o| !is_virtual(&o.id)).collect();
        if !self.paths.batch_orders {
            for order in real {
                let filter = self
                    .variant
                    .is_spot()
                    .then_some(spot_order_filter(order.kind));
                let body = Body::new()
                    .str("category", self.category_for(&order.symbol).as_str())
                    .str("symbol", &order.symbol)
                    .str("orderId", &order.id)
                    .opt_str("orderFilter", filter)
                    .finish();
                job.steps.push_back(Step {
                    delay_ms: 0.0,
                    requests: vec![Planned {
                        receipts: Vec::new(),
                        path: "/v5/order/cancel",
                        body,
                        call: Call::Cancel {
                            ids: vec![order.id.clone()],
                        },
                    }],
                });
            }
            return job;
        }
        for category in self.categories() {
            let category_orders: Vec<&Order> = real
                .iter()
                .copied()
                .filter(|order| self.category_for(&order.symbol) == category)
                .collect();
            for (index, chunk) in category_orders.chunks(CANCEL_BATCH_LIMIT).enumerate() {
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
                    .str("category", category.as_str())
                    .raw("request", &format!("[{}]", items.join(",")))
                    .finish();
                job.steps.push_back(Step {
                    delay_ms: if index > 0 {
                        CANCEL_BATCH_DELAY_MS
                    } else {
                        0.0
                    },
                    requests: vec![Planned {
                        receipts: Vec::new(),
                        path: "/v5/order/cancel-batch",
                        body,
                        call: Call::Cancel {
                            ids: chunk.iter().map(|o| o.id.clone()).collect(),
                        },
                    }],
                });
            }
        }
        job
    }
}

fn plan_leverage(
    cx: &Cx<'_, Pending>,
    category: &str,
    symbol: String,
    input: f64,
) -> Result<Job, String> {
    let market = cx
        .store
        .market(&symbol)
        .ok_or_else(|| format!("Market {symbol} not found"))?;
    let leverage = input
        .max(market.limits.leverage.min)
        .min(market.limits.leverage.max);
    // Flat hedged rails can differ, and the write sets both.
    if cx.store.setting(&symbol).is_some_and(|s| {
        s.buy_leverage.unwrap_or(s.leverage) == leverage
            && s.sell_leverage.unwrap_or(s.leverage) == leverage
    }) {
        return Ok(Job::default());
    }
    let value = number(leverage);
    let body = Body::new()
        .str("category", category)
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
fn mark_canceled(cx: &mut Cx<'_, Pending>, ids: &[String], retire: bool) {
    if retire {
        for id in ids {
            cx.store.remove_orders(&[
                id,
                &format!("{id}{STOP_LOSS_SUFFIX}"),
                &format!("{id}{TAKE_PROFIT_SUFFIX}"),
            ]);
        }
        return;
    }
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

#[derive(Default)]
struct ExecutionParams {
    bbo_side: Option<&'static str>,
    bbo_level: Option<String>,
    slippage_kind: Option<&'static str>,
    slippage: Option<String>,
}

fn execution_params(
    opts: &PlaceOrderOpts,
    spot: bool,
    hyro: bool,
) -> Result<ExecutionParams, String> {
    let bbo = opts.bybit_bbo.as_ref();
    let ticks = opts.bybit_slippage_tolerance_ticks;
    let percent = opts.max_slippage_pct;
    if hyro && (bbo.is_some() || ticks.is_some() || percent.is_some()) {
        return Err("HyroTrader does not support client-defined slippage or BBO controls".into());
    }
    let attached =
        opts.trigger_price.is_some() || opts.stop_loss.is_some() || opts.take_profit.is_some();
    if let Some(bbo) = bbo {
        if spot
            || opts.kind != OrderType::Limit
            || opts.price.is_some()
            || attached
            || ticks.is_some()
            || percent.is_some()
            || opts
                .time_in_force
                .is_some_and(|t| t != TimeInForce::GoodTillCancel)
        {
            return Err(
                "Bybit BBO requires a plain derivative GTC limit order without price or protection"
                    .into(),
            );
        }
        let side = match bbo.side_type.as_str() {
            "Queue" => "Queue",
            "Counterparty" => "Counterparty",
            _ => return Err("Bybit BBO sideType must be Queue or Counterparty".into()),
        };
        if !bbo.level.is_finite() || bbo.level.fract() != 0.0 || !(1.0..=5.0).contains(&bbo.level) {
            return Err("Bybit BBO level must be an integer from 1 to 5".into());
        }
        return Ok(ExecutionParams {
            bbo_side: Some(side),
            bbo_level: Some(number(bbo.level)),
            ..ExecutionParams::default()
        });
    }
    if let Some(ticks) = ticks {
        if opts.kind != OrderType::Market || opts.price.is_some() || attached || percent.is_some() {
            return Err("Bybit TickSize slippage requires a plain market order".into());
        }
        if !ticks.is_finite() || ticks.fract() != 0.0 || !(1.0..=10_000.0).contains(&ticks) {
            return Err("Bybit TickSize slippage must be an integer from 1 to 10000".into());
        }
        return Ok(ExecutionParams {
            slippage_kind: Some("TickSize"),
            slippage: Some(number(ticks)),
            ..ExecutionParams::default()
        });
    }
    if let Some(percent) = percent {
        if opts.kind != OrderType::Market
            || attached
            || !percent.is_finite()
            || !(0.01..=10.0).contains(&percent)
            || (percent * 100.0 - (percent * 100.0).round()).abs() >= 1e-9
        {
            return Err("Bybit maxSlippagePct requires a plain market order and 0.01 to 10 with at most two decimals".into());
        }
        return Ok(ExecutionParams {
            slippage_kind: Some("Percent"),
            slippage: Some(number(percent)),
            ..ExecutionParams::default()
        });
    }
    Ok(ExecutionParams::default())
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalOrder {
    symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_in_force: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_link_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    close_on_trigger: Option<bool>,
}

impl BybitLinear {
    fn apply_local_write(&mut self, cx: &mut Cx<'_, Pending>, local: &LocalWrite, response: &str) {
        // A later authoritative snapshot or private session supersedes an ack.
        if local.socket != self.private.socket
            || local.snapshot != self.account.orders_snapshot_version
        {
            return;
        }
        match local.path {
            "/v5/order/create" => {
                if let (Ok(input), Ok(reply)) = (
                    serde_json::from_str::<LocalOrder>(&local.body),
                    serde_json::from_str::<Placed>(response),
                ) && let Some(id) = reply.result.and_then(|r| r.order_id)
                {
                    self.publish_local_create(cx, &input, &id);
                }
            }
            "/v5/order/create-batch" => {
                #[derive(serde::Deserialize)]
                struct Inputs {
                    #[serde(rename = "request")]
                    orders: Vec<LocalOrder>,
                }
                if let (Ok(inputs), Ok(reply)) = (
                    serde_json::from_str::<Inputs>(&local.body),
                    serde_json::from_str::<Batch>(response),
                ) && let Some(results) = reply.result
                {
                    for (input, result) in inputs.orders.iter().zip(results.list) {
                        if let Some(id) = result.order_id {
                            self.publish_local_create(cx, input, &id);
                        }
                    }
                }
            }
            "/v5/order/amend" => {
                let Ok(input) = serde_json::from_str::<LocalOrder>(&local.body) else {
                    return;
                };
                let Some(id) = input.order_id.as_deref() else {
                    return;
                };
                let Some(before) = local.before.iter().find(|o| o.id == id) else {
                    return;
                };
                // A private event wins over a late acknowledgement.
                if cx.store.order(id) != Some(before) {
                    return;
                }
                let price = input
                    .price
                    .or(input.trigger_price)
                    .and_then(|p| p.parse().ok());
                let amount = input.qty.and_then(|a| a.parse().ok());
                cx.store.update_order(id, |o| {
                    if let Some(price) = price {
                        o.price = price;
                    }
                    if let Some(amount) = amount {
                        o.amount = amount;
                        o.remaining = (amount - o.filled).max(0.0);
                    }
                    o.last_update = cx.now;
                });
                for (suffix, level) in [
                    (STOP_LOSS_SUFFIX, input.stop_loss),
                    (TAKE_PROFIT_SUFFIX, input.take_profit),
                ] {
                    if let Some(level) = level.and_then(|p| p.parse().ok()) {
                        cx.store.update_order(&format!("{id}{suffix}"), |o| {
                            o.price = level;
                            o.last_update = cx.now;
                        });
                    }
                }
            }
            _ => {}
        }
    }

    /// Demo rows for a validated create-batch, one per accepted request.
    fn publish_local_rows(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        local: &LocalWrite,
        ids: &[Option<String>],
    ) {
        #[derive(serde::Deserialize)]
        struct Inputs {
            #[serde(rename = "request")]
            orders: Vec<LocalOrder>,
        }
        if local.socket != self.private.socket
            || local.snapshot != self.account.orders_snapshot_version
        {
            return;
        }
        let Ok(inputs) = serde_json::from_str::<Inputs>(&local.body) else {
            return;
        };
        for (input, id) in inputs.orders.iter().zip(ids) {
            if let Some(id) = id {
                self.publish_local_create(cx, input, id);
            }
        }
    }

    fn publish_local_create(&mut self, cx: &mut Cx<'_, Pending>, input: &LocalOrder, id: &str) {
        if id.is_empty()
            || cx.store.order(id).is_some()
            || (input.order_type.as_deref() == Some("Market") && input.trigger_price.is_none())
        {
            return;
        }
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct PlacedRow<'a> {
            #[serde(flatten)]
            input: &'a LocalOrder,
            order_id: &'a str,
            order_status: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            stop_order_type: Option<&'static str>,
        }
        let wire = PlacedRow {
            input,
            order_id: id,
            order_status: "New",
            stop_order_type: input.trigger_price.as_ref().map(|_| "Stop"),
        };
        if let Ok(body) = serde_json::to_string(&wire)
            && let Ok(row) = serde_json::from_str::<OrderRow>(&body)
            && let Some(mapped) = crate::account::map_order(&row, cx.store, cx.now)
        {
            for order in mapped.orders {
                cx.store.upsert_order(order);
            }
            if input.reduce_only == Some(true)
                && input.order_type.as_deref() == Some("Limit")
                && input.trigger_price.is_none()
            {
                self.schedule_fill_repair(cx, id, &input.symbol);
            }
        }
    }
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

fn trade_command_id(command: &Command) -> Option<CommandId> {
    match command {
        Command::PlaceOrders { command, .. }
        | Command::CancelOrders { command, .. }
        | Command::CancelOrdersWithSymbols { command, .. }
        | Command::AmendOrder { command, .. }
        | Command::CancelSymbolOrders { command, .. }
        | Command::SetLeverage { command, .. }
        | Command::SetMarginType { command, .. }
        | Command::SetPositionMode { command, .. } => Some(*command),
        _ => None,
    }
}
