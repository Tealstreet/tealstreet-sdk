//! Order entry. Each command becomes a job of sequential steps; a step's
//! requests go out together and the next step starts when they all answer.
//! Stops, take-profits and trailing stops use the Algo Order API.
use crate::account::venue_error;
use crate::sign::Query;
use crate::wire::{BatchItem, PlacedOrder};
use crate::{BinanceFutures, Pending};
use safe_cex_engine::model::{
    Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, PositionSide, StopTriggerType,
    TimeInForce,
};
use safe_cex_engine::{Command, CommandId, Cx, LogLevel, Method, Response, num};
use std::collections::{HashMap, VecDeque};

const BROKER_PREFIX: &str = "x-s3vFFRQp";
/// Errors callers handle themselves; they are not shown to the user.
const SILENT_CODES: [i64; 4] = [-2011, -2013, -4059, -5022];
const UNKNOWN_ORDER: i64 = -2011;
const POSITION_MODE_ALREADY_SET: i64 = -4059;
const POST_ONLY_REJECTED: i64 = -5022;

pub struct Request {
    command: CommandId,
    call: Call,
}

/// What a response means for its job.
enum Call {
    /// Portfolio margin answers with our client id, which is the order's id.
    PlaceOne {
        client_id: Option<String>,
    },
    PlaceBatch,
    PlaceAlgo {
        client_id: Option<String>,
    },
    /// Regular orders by id, or a whole symbol's regular or algo orders;
    /// these rows are marked canceled once the venue accepts.
    Cancel {
        ids: Vec<String>,
    },
    Leverage {
        symbol: String,
        leverage: f64,
    },
    MarginType {
        symbol: String,
        isolated: bool,
    },
    PositionMode {
        hedged: bool,
    },
}

struct Planned {
    method: Method,
    path: &'static str,
    query: Query,
    call: Call,
}

#[derive(Default)]
struct Job {
    steps: VecDeque<Vec<Planned>>,
    outstanding: usize,
    /// Run every step even after a failure (cancel-all's two families).
    continue_on_error: bool,
    ids: Vec<String>,
    error: Option<String>,
}

#[derive(Default)]
pub struct TradingState {
    jobs: HashMap<CommandId, Job>,
}

/// Request parameters in order, rendered as a query or as a batch JSON object.
#[derive(Clone, Default)]
struct Params(Vec<(&'static str, String)>);

impl Params {
    fn set(&mut self, name: &'static str, value: impl Into<String>) {
        self.0.push((name, value.into()));
    }

    fn get(&self, name: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
    }

    fn query(&self) -> Query {
        self.0
            .iter()
            .fold(Query::new(), |query, (name, value)| query.str(name, value))
    }

    fn json(&self) -> String {
        let fields: Vec<String> = self
            .0
            .iter()
            .map(|(name, value)| {
                format!(
                    "\"{name}\":{}",
                    serde_json::to_string(value).unwrap_or_default()
                )
            })
            .collect();
        format!("{{{}}}", fields.join(","))
    }
}

fn side_text(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "BUY",
        OrderSide::Sell => "SELL",
    }
}

fn type_text(kind: OrderType) -> &'static str {
    match kind {
        OrderType::Limit => "LIMIT",
        OrderType::Market => "MARKET",
        OrderType::StopLoss => "STOP_MARKET",
        OrderType::TakeProfit => "TAKE_PROFIT_MARKET",
        OrderType::TrailingStopLoss => "TRAILING_STOP_MARKET",
        OrderType::Conditional => "STOP_MARKET",
    }
}

fn tif_text(tif: TimeInForce) -> &'static str {
    match tif {
        TimeInForce::GoodTillCancel => "GTC",
        TimeInForce::ImmediateOrCancel => "IOC",
        TimeInForce::FillOrKill => "FOK",
        TimeInForce::PostOnly => "GTX",
    }
}

fn is_algo(kind: OrderType) -> bool {
    matches!(
        kind,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    )
}

fn generated_id() -> String {
    let id = safe_cex_crypto::uuid::uuid().unwrap_or_default();
    format!("{BROKER_PREFIX}{id}")
}

/// USD-M's documented client order ID charset.
fn valid_client_order_id(id: &str) -> bool {
    (1..=36).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b':' | b'/' | b'_' | b'-'))
}

fn number(value: f64) -> String {
    num::format(value)
}

impl BinanceFutures {
    pub(crate) fn on_trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let (id, plan) = match command {
            Command::PlaceOrders { command, orders } => (command, self.plan_place(cx, &orders)),
            Command::CancelOrders { command, ids } => {
                let orders: Vec<Order> = ids
                    .iter()
                    .filter_map(|id| cx.store.order(id).cloned())
                    .collect();
                (command, Ok(self.plan_cancel(&orders)))
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (command, self.plan_amend(cx, &id, price, amount)),
            Command::CancelSymbolOrders { command, symbol } => {
                let (algo, regular): (Vec<&Order>, Vec<&Order>) = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == symbol)
                    .partition(|o| is_algo(o.kind));
                let ids = |orders: Vec<&Order>| orders.iter().map(|o| o.id.clone()).collect();
                let step = vec![
                    Planned {
                        method: Method::Delete,
                        path: self.paths.cancel_all,
                        query: Query::new().str("symbol", &symbol),
                        call: Call::Cancel { ids: ids(regular) },
                    },
                    Planned {
                        method: Method::Delete,
                        path: self.paths.cancel_all_conditional,
                        query: Query::new().str("symbol", &symbol),
                        call: Call::Cancel { ids: ids(algo) },
                    },
                ];
                let job = Job {
                    steps: VecDeque::from([step]),
                    continue_on_error: true,
                    ..Job::default()
                };
                (command, Ok(job))
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
            } => (command, self.plan_leverage(cx, symbol, leverage)),
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => {
                let query = Query::new()
                    .str("symbol", &symbol)
                    .str("marginType", if isolated { "ISOLATED" } else { "CROSSED" });
                let call = Call::MarginType { symbol, isolated };
                let plan = match self.paths.margin_type {
                    Some(path) => Ok(single(Method::Post, path, query, call)),
                    None => Err("Portfolio margin accounts are cross margin only".to_owned()),
                };
                (command, plan)
            }
            Command::SetPositionMode { command, hedged } => {
                let plan = if cx.store.positions().iter().any(|p| p.contracts > 0.0) {
                    Err("Please close all positions before switching position mode".to_owned())
                } else {
                    let query =
                        Query::new().str("dualSidePosition", if hedged { "true" } else { "false" });
                    let call = Call::PositionMode { hedged };
                    Ok(single(Method::Post, self.paths.position_mode, query, call))
                };
                (command, plan)
            }
            _ => return,
        };
        if self.credentials.is_none() {
            cx.done(id, Err("Binance trading needs an API key".into()));
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
        let step = if stop { None } else { job.steps.pop_front() };
        let Some(step) = step.filter(|step| !step.is_empty()) else {
            let job = self.trading.jobs.remove(&command).expect("present");
            let result = match job.error {
                Some(error) => Err(error),
                None => Ok(serde_json::to_string(&job.ids).unwrap_or_default()),
            };
            cx.done(command, result);
            return;
        };
        job.outstanding = step.len();
        for planned in step {
            self.signed(
                cx,
                planned.method,
                planned.path,
                planned.query,
                Pending::Trade(Request {
                    command,
                    call: planned.call,
                }),
            );
        }
    }

    pub(crate) fn on_trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let Request { command, call } = request;
        let outcome = match response {
            Ok(response) => match venue_error(&response) {
                None => Ok(response.body),
                Some(error) => Err(error),
            },
            Err(error) => Err((0, error.to_owned())),
        };
        let mut ids = Vec::new();
        let error = match (call, outcome) {
            (
                Call::PlaceOne {
                    client_id: Some(id),
                },
                Ok(_),
            )
            | (
                Call::PlaceAlgo {
                    client_id: Some(id),
                },
                Ok(_),
            ) => {
                ids.push(id);
                None
            }
            (Call::PlaceOne { .. }, Ok(body)) | (Call::PlaceAlgo { .. }, Ok(body)) => {
                match serde_json::from_str::<PlacedOrder>(body) {
                    Ok(placed) => {
                        ids.extend(placed.order_id.or(placed.algo_id).map(|id| id.to_string()));
                        None
                    }
                    Err(error) => Some((0, error.to_string())),
                }
            }
            (Call::PlaceBatch, Ok(body)) => {
                if let Ok(items) = serde_json::from_str::<Vec<BatchItem>>(body) {
                    for item in items {
                        match (item.order_id, item.code) {
                            (Some(id), None) => ids.push(id.to_string()),
                            (_, code) => {
                                let code = code.unwrap_or(0);
                                if code != POST_ONLY_REJECTED {
                                    let msg = item.msg.as_deref().unwrap_or("batch order rejected");
                                    cx.log(LogLevel::Error, msg.to_owned());
                                }
                            }
                        }
                    }
                }
                None
            }
            (Call::Cancel { ids: canceled }, Ok(_)) => {
                mark_canceled(cx, &canceled);
                None
            }
            (Call::Cancel { ids: canceled }, Err((UNKNOWN_ORDER, _))) => {
                mark_canceled(cx, &canceled);
                None
            }
            (Call::Leverage { symbol, leverage }, Ok(_)) => {
                cx.store.update_setting(&symbol, |s| s.leverage = leverage);
                cx.store.update_positions(|p| {
                    if p.symbol == symbol {
                        p.leverage = leverage;
                    }
                });
                None
            }
            (Call::MarginType { symbol, isolated }, Ok(_)) => {
                let margin = if isolated {
                    safe_cex_engine::model::MarginType::Isolated
                } else {
                    safe_cex_engine::model::MarginType::Cross
                };
                cx.store
                    .update_setting(&symbol, |s| s.margin_type = Some(margin));
                None
            }
            (Call::PositionMode { hedged }, Ok(_))
            | (Call::PositionMode { hedged }, Err((POSITION_MODE_ALREADY_SET, _))) => {
                cx.store.set_hedged(hedged);
                None
            }
            (_, Err(error)) => Some(error),
        };
        if let Some((code, message)) = error {
            if !SILENT_CODES.contains(&code) {
                cx.log(LogLevel::Error, message.clone());
            }
            if let Some(job) = self.trading.jobs.get_mut(&command) {
                job.error.get_or_insert(message);
            }
        }
        let Some(job) = self.trading.jobs.get_mut(&command) else {
            return;
        };
        job.ids.extend(ids);
        job.outstanding = job.outstanding.saturating_sub(1);
        if job.outstanding == 0 {
            self.advance(cx, command);
        }
    }

    fn plan_place(
        &self,
        cx: &mut Cx<'_, Pending>,
        orders: &[PlaceOrderOpts],
    ) -> Result<Job, String> {
        check_client_order_ids(orders)?;
        let mut job = Job::default();
        let mut regular = Vec::new();
        let mut algo = Vec::new();
        // Binance allows one closePosition order per direction per symbol.
        let mut replaced: Vec<Order> = Vec::new();
        let pm = self.options.portfolio_margin;
        for opts in orders {
            if !pm {
                for existing in self.conflicting_close_orders(cx, opts) {
                    if !replaced.iter().any(|o| o.id == existing.id) {
                        replaced.push(existing);
                    }
                }
            }
            let (r, a) = self.format_order(cx, opts)?;
            regular.extend(r);
            if pm {
                algo.extend(
                    a.into_iter()
                        .map(|params| self.conditional_params(cx, params)),
                );
            } else {
                algo.extend(a);
            }
        }
        if !replaced.is_empty() {
            cx.log(
                LogLevel::Info,
                format!(
                    "Canceling {} existing closePosition order(s) before placing new one",
                    replaced.len()
                ),
            );
            job.steps.extend(self.plan_cancel(&replaced).steps);
        }
        let lot_size = if self.paths.batch_orders.is_some() {
            5
        } else {
            1
        };
        for lot in regular.chunks(lot_size) {
            let step = match (lot, self.paths.batch_orders) {
                ([only], _) => Planned {
                    method: Method::Post,
                    path: self.paths.order,
                    query: only.query(),
                    call: Call::PlaceOne {
                        client_id: pm.then(|| only.get("newClientOrderId")).flatten(),
                    },
                },
                (_, Some(path)) => {
                    let batch: Vec<String> = lot.iter().map(Params::json).collect();
                    Planned {
                        method: Method::Post,
                        path,
                        query: Query::new().str("batchOrders", &format!("[{}]", batch.join(","))),
                        call: Call::PlaceBatch,
                    }
                }
                (_, None) => unreachable!("lots are single orders without a batch endpoint"),
            };
            job.steps.push_back(vec![step]);
        }
        for params in algo {
            job.steps.push_back(vec![Planned {
                method: Method::Post,
                path: self.paths.conditional_order,
                query: params.query(),
                call: Call::PlaceAlgo {
                    client_id: pm.then(|| params.get("newClientStrategyId")).flatten(),
                },
            }]);
        }
        Ok(job)
    }

    fn conflicting_close_orders(&self, cx: &Cx<'_, Pending>, opts: &PlaceOrderOpts) -> Vec<Order> {
        if !matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit) {
            return Vec::new();
        }
        let Some(market) = cx.store.market(&opts.symbol) else {
            return Vec::new();
        };
        let amount = num::adjust(opts.amount, market.precision.amount);
        if !(opts.close == Some(true) || amount == 0.0) {
            return Vec::new();
        }
        cx.store
            .orders()
            .iter()
            .filter(|o| {
                o.symbol == opts.symbol
                    && o.close == Some(true)
                    && o.amount == 0.0
                    && o.kind == opts.kind
                    && o.side == opts.side
            })
            .cloned()
            .collect()
    }

    fn plan_cancel(&self, orders: &[Order]) -> Job {
        let mut job = Job::default();
        if self.options.portfolio_margin {
            // One at a time, by the client ids the orders were placed with.
            for order in orders {
                let (path, field) = if is_algo(order.kind) {
                    (self.paths.conditional_order, "newClientStrategyId")
                } else {
                    (self.paths.order, "origClientOrderId")
                };
                job.steps.push_back(vec![Planned {
                    method: Method::Delete,
                    path,
                    query: Query::new()
                        .str("symbol", &order.symbol)
                        .str(field, &order.id),
                    call: Call::Cancel {
                        ids: vec![order.id.clone()],
                    },
                }]);
            }
            return job;
        }
        let mut by_symbol: Vec<(String, Vec<String>)> = Vec::new();
        for order in orders.iter().filter(|o| !is_algo(o.kind)) {
            match by_symbol
                .iter_mut()
                .find(|(symbol, _)| *symbol == order.symbol)
            {
                Some((_, ids)) => ids.push(order.id.clone()),
                None => by_symbol.push((order.symbol.clone(), vec![order.id.clone()])),
            }
        }
        let batch = self.paths.batch_orders.expect("USD-M has a batch endpoint");
        for (symbol, ids) in by_symbol {
            if let [id] = ids.as_slice() {
                let query = Query::new().str("symbol", &symbol).str("orderId", id);
                job.steps.push_back(vec![Planned {
                    method: Method::Delete,
                    path: self.paths.order,
                    query,
                    call: Call::Cancel { ids: ids.clone() },
                }]);
                continue;
            }
            for lot in ids.chunks(10) {
                // LIST<LONG>: ids stay digits so no precision is lost.
                let query = Query::new()
                    .str("symbol", &symbol)
                    .str("orderIdList", &format!("[{}]", lot.join(",")));
                job.steps.push_back(vec![Planned {
                    method: Method::Delete,
                    path: batch,
                    query,
                    call: Call::Cancel { ids: lot.to_vec() },
                }]);
            }
        }
        for order in orders.iter().filter(|o| is_algo(o.kind)) {
            job.steps.push_back(vec![Planned {
                method: Method::Delete,
                path: self.paths.conditional_order,
                query: Query::new().str("algoId", &order.id),
                call: Call::Cancel {
                    ids: vec![order.id.clone()],
                },
            }]);
        }
        job
    }

    /// Portfolio margin's conditional orders name their fields differently and
    /// have no closePosition: a full-position stop carries the position's size.
    fn conditional_params(&self, cx: &Cx<'_, Pending>, params: Params) -> Params {
        let symbol = params.get("symbol").unwrap_or_default();
        let side = params.get("side");
        let mut out = Params::default();
        for (name, value) in params.0 {
            match name {
                "algoType" => {}
                "type" => out.set("strategyType", value),
                "triggerPrice" => out.set("stopPrice", value),
                "clientAlgoId" => out.set("newClientStrategyId", value),
                "closePosition" => {
                    let closes = match side.as_deref() {
                        Some("SELL") => PositionSide::Long,
                        _ => PositionSide::Short,
                    };
                    let size = cx
                        .store
                        .positions()
                        .iter()
                        .find(|p| p.symbol == symbol && p.side == closes)
                        .map_or(0.0, |p| p.contracts);
                    out.set("quantity", number(size));
                    if !cx.store.hedged() {
                        out.set("reduceOnly", "true");
                    }
                }
                _ => out.set(name, value),
            }
        }
        out
    }

    /// Binance has no amend for these orders: cancel, then place the replacement.
    fn plan_amend(
        &self,
        cx: &mut Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Job, String> {
        let order = cx
            .store
            .order(id)
            .cloned()
            .ok_or_else(|| format!("Order {id} not found"))?;
        let replacement = PlaceOrderOpts {
            max_slippage_pct: None,
            symbol: order.symbol.clone(),
            kind: order.kind,
            side: order.side,
            amount: amount.unwrap_or(order.amount),
            price: Some(price.unwrap_or(order.price)),
            trigger_price: None,
            stop_loss: None,
            take_profit: None,
            reduce_only: Some(order.reduce_only),
            time_in_force: None,
            trigger_type: None,
            tp_trigger_type: (order.kind == OrderType::TakeProfit)
                .then_some(order.trigger_type)
                .flatten(),
            sl_trigger_type: (order.kind == OrderType::StopLoss)
                .then_some(order.trigger_type)
                .flatten(),
            client_order_id: None,
            close: Some(order.close.unwrap_or(false)),
        };
        let mut job = self.plan_cancel(std::slice::from_ref(&order));
        let place = self.plan_place(cx, std::slice::from_ref(&replacement))?;
        job.steps.extend(place.steps);
        Ok(job)
    }

    fn plan_leverage(
        &self,
        cx: &Cx<'_, Pending>,
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
        if cx.store.setting(&symbol).map(|s| s.leverage) == Some(leverage) {
            return Ok(Job::default());
        }
        let query = Query::new()
            .str("symbol", &symbol)
            .str("leverage", &number(leverage));
        Ok(single(
            Method::Post,
            self.paths.leverage,
            query,
            Call::Leverage { symbol, leverage },
        ))
    }

    /// `BOTH` in one-way mode; in hedge mode the side of the position the order
    /// opens, or for reducing orders the side it closes.
    fn position_side(
        &self,
        cx: &Cx<'_, Pending>,
        side: OrderSide,
        kind: OrderType,
        reduce: bool,
    ) -> &'static str {
        if !cx.store.hedged() {
            return "BOTH";
        }
        let closes = is_algo(kind) || reduce;
        match (side, closes) {
            (OrderSide::Buy, false) | (OrderSide::Sell, true) => "LONG",
            (OrderSide::Sell, false) | (OrderSide::Buy, true) => "SHORT",
        }
    }

    /// Payloads for one order: regular orders (split above the market's max
    /// size) and algo orders (the order itself if a stop, plus attached TP/SL).
    fn format_order(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<(Vec<Params>, Vec<Params>), String> {
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        if opts.kind == OrderType::TrailingStopLoss {
            return Ok((Vec::new(), vec![self.format_trailing(cx, opts)?]));
        }
        let mut regular = Vec::new();
        let mut algo = Vec::new();
        let p_price = market.precision.price;
        let p_amount = market.precision.amount;
        let amount = num::adjust(opts.amount, p_amount);
        let price = opts
            .price
            .filter(|p| *p != 0.0 && opts.kind != OrderType::Market)
            .map(|p| num::adjust(p, p_price));
        let hedged = cx.store.hedged();
        let reduce_only = !hedged && opts.reduce_only == Some(true);
        let stop_or_tp = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
        let opposite = match opts.side {
            OrderSide::Buy => PositionSide::Short,
            OrderSide::Sell => PositionSide::Long,
        };
        let has_position = cx
            .store
            .positions()
            .iter()
            .any(|p| p.symbol == opts.symbol && p.side == opposite && p.contracts.abs() > 0.0);
        let wants_close = stop_or_tp && (opts.close == Some(true) || amount == 0.0);
        let use_close = wants_close && has_position;
        let position_side =
            self.position_side(cx, opts.side, opts.kind, opts.reduce_only == Some(true));

        if stop_or_tp {
            if wants_close && !has_position && amount == 0.0 {
                return Err(
                    "Cannot place full stop order without existing position or specified amount"
                        .into(),
                );
            }
            let mut params = Params::default();
            params.set("algoType", "CONDITIONAL");
            params.set("symbol", opts.symbol.clone());
            params.set("positionSide", position_side);
            params.set("side", side_text(opts.side));
            params.set("type", type_text(opts.kind));
            if !use_close && amount != 0.0 {
                params.set("quantity", number(amount));
            }
            if let Some(price) = price {
                params.set("triggerPrice", number(price));
            }
            if use_close {
                params.set("closePosition", "true");
            } else if reduce_only {
                params.set("reduceOnly", "true");
            }
            params.set("workingType", working_type(opts));
            params.set("clientAlgoId", generated_id());
            algo.push(params);
        } else {
            let mut params = Params::default();
            params.set("symbol", opts.symbol.clone());
            params.set("positionSide", position_side);
            params.set("side", side_text(opts.side));
            params.set("type", type_text(opts.kind));
            let quantity_at = params.0.len();
            if amount != 0.0 {
                params.set("quantity", number(amount));
            }
            if let Some(price) = price {
                params.set("price", number(price));
            }
            if opts.kind == OrderType::Limit {
                let tif = opts.time_in_force.unwrap_or(TimeInForce::GoodTillCancel);
                params.set("timeInForce", tif_text(tif));
            }
            if reduce_only {
                params.set("reduceOnly", "true");
            }
            let max = market.limits.amount.max;
            let (lots, rest) = if amount > max && max > 0.0 {
                (
                    (amount / max).ceil() as usize,
                    num::adjust(amount % max, p_amount),
                )
            } else {
                (1, 0.0)
            };
            let lot_size = num::adjust((amount - rest) / lots as f64, p_amount);
            let mut payloads: Vec<Params> = (0..lots)
                .map(|_| with_quantity(&params, quantity_at, lot_size))
                .collect();
            if rest != 0.0 {
                payloads.push(with_quantity(&params, quantity_at, rest));
            }
            if opts.client_order_id.is_some() && payloads.len() > 1 {
                return Err(format!(
                    "Client order ID needs a single order; {} exceeds the maximum size",
                    opts.amount
                ));
            }
            for mut payload in payloads {
                let id = opts.client_order_id.clone().unwrap_or_else(generated_id);
                payload.set("newClientOrderId", id);
                regular.push(payload);
            }
        }

        // Attached brackets: the position does not exist yet, so quantity +
        // reduceOnly rather than closePosition (reduceOnly is refused in hedge mode).
        let close_side = match opts.side {
            OrderSide::Buy => OrderSide::Sell,
            OrderSide::Sell => OrderSide::Buy,
        };
        for (kind, trigger) in [
            (OrderType::StopLoss, opts.stop_loss),
            (OrderType::TakeProfit, opts.take_profit),
        ] {
            let Some(trigger) = trigger.filter(|t| *t != 0.0) else {
                continue;
            };
            let mut params = Params::default();
            params.set("algoType", "CONDITIONAL");
            params.set("symbol", opts.symbol.clone());
            params.set(
                "positionSide",
                self.position_side(cx, close_side, kind, true),
            );
            params.set("side", side_text(close_side));
            params.set("type", type_text(kind));
            params.set("triggerPrice", number(num::adjust(trigger, p_price)));
            if amount != 0.0 {
                params.set("quantity", number(amount));
            }
            if !hedged {
                params.set("reduceOnly", "true");
            }
            let bracket = PlaceOrderOpts {
                kind,
                ..opts.clone()
            };
            params.set("workingType", working_type(&bracket));
            params.set("clientAlgoId", generated_id());
            algo.push(params);
        }
        Ok((regular, algo))
    }

    /// Trailing stops trail the whole position by the distance from last price.
    fn format_trailing(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Params, String> {
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let ticker = cx
            .store
            .ticker(&opts.symbol)
            .ok_or_else(|| format!("Ticker {} not found", opts.symbol))?;
        let side = match opts.side {
            OrderSide::Buy => PositionSide::Short,
            OrderSide::Sell => PositionSide::Long,
        };
        let position = cx
            .store
            .positions()
            .iter()
            .find(|p| p.symbol == opts.symbol && p.side == side)
            .ok_or_else(|| format!("Position {} and side {side:?} not found", opts.symbol))?;
        let target = opts.price.unwrap_or(0.0);
        let distance = num::adjust(
            ticker.last.max(target) - ticker.last.min(target),
            market.precision.price,
        );
        let rate = (distance * 100.0 / ticker.last * 10.0).round() / 10.0;
        let mut params = Params::default();
        params.set("algoType", "CONDITIONAL");
        params.set("symbol", opts.symbol.clone());
        params.set(
            "positionSide",
            self.position_side(cx, opts.side, opts.kind, opts.reduce_only == Some(true)),
        );
        params.set("side", side_text(opts.side));
        params.set("type", type_text(OrderType::TrailingStopLoss));
        params.set("quantity", number(position.contracts));
        params.set("callbackRate", number(rate));
        params.set("priceProtect", "TRUE");
        params.set("clientAlgoId", generated_id());
        Ok(params)
    }
}

fn single(method: Method, path: &'static str, query: Query, call: Call) -> Job {
    Job {
        steps: VecDeque::from([vec![Planned {
            method,
            path,
            query,
            call,
        }]]),
        ..Job::default()
    }
}

fn with_quantity(params: &Params, at: usize, quantity: f64) -> Params {
    let mut out = params.clone();
    match out.0.get_mut(at) {
        Some((name, value)) if *name == "quantity" => *value = number(quantity),
        _ => out.0.insert(at, ("quantity", number(quantity))),
    }
    out
}

/// TPs trigger on last price unless asked for mark; SLs on mark unless asked for last.
fn working_type(opts: &PlaceOrderOpts) -> &'static str {
    match opts.kind {
        OrderType::TakeProfit => match opts.tp_trigger_type.or(opts.trigger_type) {
            Some(StopTriggerType::MarkPrice) => "MARK_PRICE",
            _ => "CONTRACT_PRICE",
        },
        _ => match opts.sl_trigger_type.or(opts.trigger_type) {
            Some(StopTriggerType::LastPrice) => "CONTRACT_PRICE",
            _ => "MARK_PRICE",
        },
    }
}

fn mark_canceled(cx: &mut Cx<'_, Pending>, ids: &[String]) {
    for id in ids {
        cx.store.set_order_status(id, OrderStatus::Canceled);
    }
}

/// A caller's client order ID is honored verbatim on exactly one plain order,
/// or the whole batch is refused before anything is sent.
fn check_client_order_ids(orders: &[PlaceOrderOpts]) -> Result<(), String> {
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
                "Client order ID {id} is not valid for Binance (1-36 of A-Z a-z 0-9 . : / _ -)"
            ));
        }
        if seen.contains(&id) {
            return Err(format!("Client order ID {id} is used twice in one batch"));
        }
        seen.push(id);
    }
    Ok(())
}
