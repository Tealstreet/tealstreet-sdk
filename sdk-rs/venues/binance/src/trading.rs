//! Order entry. Each command becomes a job of sequential steps; a step's
//! requests go out together and the next step starts when they all answer.
//! Stops, take-profits and trailing stops use the Algo Order API.
use crate::account::{PmOrderSource, venue_error};
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
    PlaceBatch {
        expected: usize,
        strict: bool,
    },
    PlaceAlgo {
        client_id: Option<String>,
    },
    /// Regular orders by id, or a whole symbol's regular or algo orders;
    /// these rows are marked canceled once the venue accepts.
    Cancel {
        ids: Vec<String>,
        proof: Option<CancelProof>,
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
    amendment: Option<Amendment>,
}

struct Amendment {
    id: String,
    target_total: f64,
    replacement: PlaceOrderOpts,
}

struct CancelTarget {
    id: String,
    symbol: String,
    native: Option<u64>,
}

enum CancelProof {
    One(CancelTarget),
    Algo(CancelTarget),
    Strategy(CancelTarget),
    Batch(Vec<CancelTarget>),
    SpotAll {
        symbol: String,
        targets: Vec<CancelTarget>,
    },
    CoinAll(Vec<CancelTarget>),
    SymbolAll {
        targets: Vec<CancelTarget>,
        strategy: bool,
    },
}

#[derive(Default)]
pub struct TradingState {
    jobs: HashMap<CommandId, Job>,
    #[cfg(test)]
    id_generator: Option<fn() -> Result<String, String>>,
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

fn generated_id_with(uuid: impl FnOnce() -> Result<String, String>) -> Result<String, String> {
    let id =
        uuid().map_err(|error| format!("Binance client order ID generation failed: {error}"))?;
    Ok(format!("{BROKER_PREFIX}{id}"))
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
    fn generated_id(&self) -> Result<String, String> {
        #[cfg(test)]
        if let Some(generator) = self.trading.id_generator {
            return generated_id_with(generator);
        }
        generated_id_with(safe_cex_crypto::uuid::uuid)
    }

    fn client_id(&self, input: &Option<String>) -> Result<String, String> {
        match input {
            Some(id) => Ok(id.clone()),
            None => self.generated_id(),
        }
    }

    pub(crate) fn on_trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::FetchStpMode { command }
        | Command::AmendOrderWithSymbol { command, .. }
        | Command::SetStpMode { command, .. }
        | Command::CancelOrdersWithSymbols { command, .. }
        | Command::FetchAccount { command }
        | Command::FetchOrderBook { command, .. } = &command
        {
            cx.done(
                *command,
                Err("This Binance command is not supported".into()),
            );
            return;
        }
        if self.is_spot() {
            let id = match &command {
                Command::SetLeverage { command, .. }
                | Command::FetchLeverage { command, .. }
                | Command::SetMarginType { command, .. }
                | Command::SetPositionMode { command, .. } => Some(*command),
                _ => None,
            };
            if let Some(id) = id {
                cx.done(
                    id,
                    Err("Binance Spot does not support derivative settings".into()),
                );
                return;
            }
        }
        let (id, plan) = match command {
            Command::PlaceOrders { command, orders } => (command, self.plan_place(cx, &orders)),
            Command::CancelOrders { command, ids } => {
                let orders = ids
                    .iter()
                    .map(|id| {
                        cx.store.order(id).cloned().ok_or_else(|| {
                            format!("Order {id} not found; cancellation requires its symbol")
                        })
                    })
                    .collect::<Result<Vec<Order>, String>>();
                (command, orders.map(|orders| self.plan_cancel(&orders)))
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
                    .partition(|o| self.order_uses_strategy(o));
                let ids = |orders: Vec<&Order>| orders.iter().map(|o| o.id.clone()).collect();
                let mut step = vec![
                    Planned {
                        method: Method::Delete,
                        path: self.paths.cancel_all,
                        query: Query::new().str("symbol", &symbol),
                        call: Call::Cancel {
                            proof: Some(CancelProof::SymbolAll {
                                targets: regular
                                    .iter()
                                    .map(|order| self.cancel_target(order))
                                    .collect(),
                                strategy: false,
                            }),
                            ids: ids(regular),
                        },
                    },
                    Planned {
                        method: Method::Delete,
                        path: self.paths.cancel_all_conditional,
                        query: Query::new().str("symbol", &symbol),
                        call: Call::Cancel {
                            proof: Some(CancelProof::SymbolAll {
                                targets: algo
                                    .iter()
                                    .map(|order| self.cancel_target(order))
                                    .collect(),
                                strategy: true,
                            }),
                            ids: ids(algo),
                        },
                    },
                ];
                if self.is_spot() || self.is_inverse() {
                    step.truncate(1);
                    if let Call::Cancel { ids, proof } = &mut step[0].call {
                        *proof = Some(if self.is_spot() {
                            CancelProof::SpotAll {
                                symbol: symbol.clone(),
                                targets: cx
                                    .store
                                    .orders()
                                    .iter()
                                    .filter(|order| order.symbol == symbol)
                                    .map(|order| self.cancel_target(order))
                                    .collect(),
                            }
                        } else {
                            CancelProof::CoinAll(
                                cx.store
                                    .orders()
                                    .iter()
                                    .filter(|order| order.symbol == symbol)
                                    .map(|order| self.cancel_target(order))
                                    .collect(),
                            )
                        });
                        *ids = cx
                            .store
                            .orders()
                            .iter()
                            .filter(|o| o.symbol == symbol)
                            .map(|o| o.id.clone())
                            .collect();
                    }
                }
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
                side: _,
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
            Command::SetPositionMode {
                command,
                hedged,
                symbol: _,
            } => {
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
        if step.is_none()
            && !stop
            && let Some(mut amendment) = job.amendment.take()
        {
            let Some(order) = cx.store.order(&amendment.id) else {
                if let Some(job) = self.trading.jobs.get_mut(&command) {
                    job.error =
                        Some("Order disappeared during cancellation; replacement fenced".into());
                }
                self.advance(cx, command);
                return;
            };
            let filled = order.filled;
            let full_close =
                amendment.target_total == 0.0 && amendment.replacement.close == Some(true);
            if order.status == OrderStatus::Closed
                || (!full_close && filled >= amendment.target_total)
            {
                self.advance(cx, command);
                return;
            }
            amendment.replacement.amount = if full_close {
                0.0
            } else {
                amendment.target_total - filled
            };
            let plan = self.plan_place(cx, std::slice::from_ref(&amendment.replacement));
            if let Some(job) = self.trading.jobs.get_mut(&command) {
                match plan {
                    Ok(plan) => job.steps.extend(plan.steps),
                    Err(error) => job.error = Some(error),
                }
            }
            self.advance(cx, command);
            return;
        }
        let Some(step) = step.filter(|step| !step.is_empty()) else {
            let job = self.trading.jobs.remove(&command).expect("present");
            let result = match job.error {
                Some(error) if self.is_inverse() && !job.ids.is_empty() => {
                    #[derive(serde::Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Partial<'a> {
                        message: &'a str,
                        partial_order_ids: &'a [String],
                    }
                    Err(serde_json::to_string(&Partial {
                        message: &error,
                        partial_order_ids: &job.ids,
                    })
                    .unwrap_or(error))
                }
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
                Ok(body),
            ) if self.is_spot() => match serde_json::from_str::<PlacedOrder>(body) {
                Ok(placed) if placed.order_id.is_some() => {
                    ids.push(id);
                    None
                }
                _ => Some((
                    0,
                    "Binance Spot placement response has no native order ID".into(),
                )),
            },
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
                    Ok(placed) => match placed.order_id.or(placed.algo_id) {
                        Some(id) => {
                            ids.push(id.to_string());
                            None
                        }
                        None => Some((
                            0,
                            "Binance placement response has no native order ID".into(),
                        )),
                    },
                    Err(error) => Some((0, error.to_string())),
                }
            }
            (Call::PlaceBatch { expected, strict }, Ok(body)) => {
                let mut rejection = None;
                if let Ok(items) = serde_json::from_str::<Vec<BatchItem>>(body) {
                    if strict && items.len() != expected {
                        rejection = Some((
                            0,
                            "Binance batch response count does not match the request".into(),
                        ));
                    }
                    for item in items {
                        match (item.order_id, item.code) {
                            (Some(id), None) => ids.push(id.to_string()),
                            (_, code) => {
                                let code = code.unwrap_or(0);
                                if strict {
                                    rejection.get_or_insert((
                                        code,
                                        item.msg
                                            .as_deref()
                                            .unwrap_or("Binance batch order rejected")
                                            .to_owned(),
                                    ));
                                }
                                if code != POST_ONLY_REJECTED {
                                    let msg = item.msg.as_deref().unwrap_or("batch order rejected");
                                    cx.log(LogLevel::Error, msg.to_owned());
                                }
                            }
                        }
                    }
                } else if strict {
                    rejection = Some((0, "Binance batch response malformed".into()));
                }
                rejection
            }
            (
                Call::Cancel {
                    ids: canceled,
                    proof,
                },
                Ok(body),
            ) => match proof {
                Some(proof) => self.confirm_cancel(cx, proof, body),
                None => {
                    self.mark_canceled(cx, &canceled);
                    None
                }
            },
            (
                Call::Cancel {
                    ids: canceled,
                    proof,
                },
                Err((UNKNOWN_ORDER, message)),
            ) => {
                if self
                    .trading
                    .jobs
                    .get(&command)
                    .is_some_and(|job| job.amendment.is_some())
                {
                    Some((
                        UNKNOWN_ORDER,
                        format!("Cancellation was not confirmed: {message}"),
                    ))
                } else {
                    match proof {
                        Some(CancelProof::One(target)) => {
                            self.mark_captured_canceled(cx, &[target], false);
                        }
                        Some(CancelProof::Algo(target) | CancelProof::Strategy(target)) => {
                            self.mark_captured_canceled(cx, &[target], true);
                        }
                        Some(CancelProof::SymbolAll { targets, strategy }) => {
                            self.mark_captured_canceled(cx, &targets, strategy);
                        }
                        Some(CancelProof::Batch(targets) | CancelProof::CoinAll(targets))
                        | Some(CancelProof::SpotAll { targets, .. }) => {
                            self.mark_captured_canceled(cx, &targets, false);
                        }
                        None => self.mark_canceled(cx, &canceled),
                    }
                    None
                }
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
        if orders.iter().any(|order| order.stp_mode.is_some()) {
            return Err("Binance self-trade prevention controls are not supported".into());
        }
        check_client_order_ids(orders)?;
        for opts in orders {
            if let Some(id) = &opts.client_order_id {
                if self.is_inverse() {
                    return Err("Binance COIN-M does not support caller client order IDs".into());
                }
                if self.options.portfolio_margin && id.len() > 32 {
                    return Err("Binance PM client order IDs must be at most 32 characters".into());
                }
                if self.is_spot()
                    && !id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                {
                    return Err(
                        "Binance Spot client order IDs must contain only letters, digits, _ or -"
                            .into(),
                    );
                }
            }
        }
        if orders
            .iter()
            .any(|o| o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some())
        {
            return Err("Bybit execution controls are supported only on Bybit".into());
        }
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
            let (r, a) = if self.is_spot() {
                (self.format_spot(cx, opts)?, Vec::new())
            } else if self.is_inverse() {
                (self.format_inverse(cx, opts)?, Vec::new())
            } else {
                self.format_order(cx, opts)?
            };
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
                        client_id: (pm || self.is_spot())
                            .then(|| only.get("newClientOrderId"))
                            .flatten(),
                    },
                },
                (_, Some(path)) => {
                    let batch: Vec<String> = lot.iter().map(Params::json).collect();
                    Planned {
                        method: Method::Post,
                        path,
                        query: Query::new().str("batchOrders", &format!("[{}]", batch.join(","))),
                        call: Call::PlaceBatch {
                            expected: lot.len(),
                            strict: self.is_inverse(),
                        },
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
        if amount != 0.0 && (self.options.portfolio_margin || opts.close != Some(true)) {
            return Vec::new();
        }
        cx.store
            .orders()
            .iter()
            .filter(|o| {
                o.symbol == opts.symbol
                    && o.status == OrderStatus::Open
                    && o.close == Some(true)
                    && o.amount == 0.0
                    && o.kind == opts.kind
                    && o.side == opts.side
            })
            .cloned()
            .collect()
    }

    fn order_uses_strategy(&self, order: &Order) -> bool {
        if self.options.portfolio_margin
            && let Some(source) = self.pm_order_source(&order.symbol, &order.id)
        {
            return matches!(source, PmOrderSource::Strategy(_));
        }
        is_algo(order.kind)
    }

    fn plan_cancel(&self, orders: &[Order]) -> Job {
        let mut job = Job::default();
        if self.options.portfolio_margin || self.is_spot() {
            // One at a time, by the client ids the orders were placed with.
            for order in orders {
                let (path, field) = if !self.is_spot() && self.order_uses_strategy(order) {
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
                        proof: Some(if !self.is_spot() && self.order_uses_strategy(order) {
                            CancelProof::Strategy(self.cancel_target(order))
                        } else {
                            CancelProof::One(self.cancel_target(order))
                        }),
                    },
                }]);
            }
            return job;
        }
        let mut by_symbol: Vec<(String, Vec<String>)> = Vec::new();
        for order in orders
            .iter()
            .filter(|o| self.is_inverse() || !is_algo(o.kind))
        {
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
                    call: Call::Cancel {
                        ids: ids.clone(),
                        proof: Some({
                            CancelProof::One(CancelTarget {
                                id: id.clone(),
                                symbol: symbol.clone(),
                                native: id.parse().ok(),
                            })
                        }),
                    },
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
                    call: Call::Cancel {
                        ids: lot.to_vec(),
                        proof: Some({
                            CancelProof::Batch(
                                lot.iter()
                                    .map(|id| CancelTarget {
                                        id: id.clone(),
                                        symbol: symbol.clone(),
                                        native: id.parse().ok(),
                                    })
                                    .collect(),
                            )
                        }),
                    },
                }]);
            }
        }
        for order in orders
            .iter()
            .filter(|o| !self.is_inverse() && is_algo(o.kind))
        {
            job.steps.push_back(vec![Planned {
                method: Method::Delete,
                path: self.paths.conditional_order,
                query: Query::new().str("algoId", &order.id),
                call: Call::Cancel {
                    ids: vec![order.id.clone()],
                    proof: Some(CancelProof::Algo(self.cancel_target(order))),
                },
            }]);
        }
        job
    }

    fn format_spot(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Vec<Params>, String> {
        if opts.reduce_only == Some(true) || opts.close == Some(true) {
            return Err("Binance Spot does not support reduceOnly or closePosition".into());
        }
        if matches!(
            opts.kind,
            OrderType::TrailingStopLoss | OrderType::Conditional
        ) {
            return Err("Binance Spot trailing/conditional placement requires unsupported native parameters".into());
        }
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let amount = num::adjust(opts.amount, market.precision.amount);
        if !amount.is_finite() || amount <= 0.0 {
            return Err("Binance Spot orders require a positive explicit quantity".into());
        }
        let mut params = Params::default();
        params.set("symbol", opts.symbol.clone());
        params.set("side", side_text(opts.side));
        let kind = match opts.kind {
            OrderType::Limit if opts.time_in_force == Some(TimeInForce::PostOnly) => "LIMIT_MAKER",
            OrderType::Limit => "LIMIT",
            OrderType::Market => "MARKET",
            OrderType::StopLoss => "STOP_LOSS",
            OrderType::TakeProfit => "TAKE_PROFIT",
            _ => unreachable!(),
        };
        params.set("type", kind);
        if matches!(opts.kind, OrderType::Limit | OrderType::Market) && opts.trigger_price.is_some()
        {
            return Err("Binance Spot plain orders cannot carry triggerPrice".into());
        }
        if opts.kind == OrderType::Limit {
            params.set(
                "price",
                number(positive_price(opts.price, market.precision.price)?),
            );
            if kind != "LIMIT_MAKER" {
                params.set(
                    "timeInForce",
                    tif_text(opts.time_in_force.unwrap_or(TimeInForce::GoodTillCancel)),
                );
            }
        } else if matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit) {
            params.set(
                "stopPrice",
                number(positive_price(
                    opts.trigger_price.or(opts.price),
                    market.precision.price,
                )?),
            );
        } else if opts.trigger_price.is_some() {
            return Err("Binance Spot plain orders cannot carry triggerPrice".into());
        }
        let mut out = sized_payloads(
            params,
            amount,
            market.limits.amount.max,
            market.precision.amount,
        )?;
        if opts.client_order_id.is_some() && out.len() != 1 {
            return Err("Client order ID requires one native order".into());
        }
        for p in &mut out {
            p.set("newClientOrderId", self.client_id(&opts.client_order_id)?);
        }
        for (kind, trigger) in [
            ("STOP_LOSS", opts.stop_loss),
            ("TAKE_PROFIT", opts.take_profit),
        ] {
            if let Some(trigger) = trigger {
                let mut p = Params::default();
                p.set("symbol", opts.symbol.clone());
                p.set(
                    "side",
                    side_text(if opts.side == OrderSide::Buy {
                        OrderSide::Sell
                    } else {
                        OrderSide::Buy
                    }),
                );
                p.set("type", kind);
                p.set(
                    "stopPrice",
                    number(positive_price(Some(trigger), market.precision.price)?),
                );
                for mut part in
                    sized_payloads(p, amount, market.limits.amount.max, market.precision.amount)?
                {
                    part.set("newClientOrderId", self.generated_id()?);
                    out.push(part);
                }
            }
        }
        Ok(out)
    }

    fn format_inverse(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Vec<Params>, String> {
        if opts.kind == OrderType::Conditional {
            return Err("Binance COIN-M conditional orders require a supported stop type".into());
        }
        if matches!(opts.kind, OrderType::Limit | OrderType::Market) && opts.trigger_price.is_some()
        {
            return Err("Binance COIN-M plain orders cannot carry triggerPrice".into());
        }
        if !opts.amount.is_finite() || opts.amount < 0.0 {
            return Err("Binance COIN-M quantity must be finite and nonnegative".into());
        }
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let amount = num::adjust(opts.amount, market.precision.amount);
        if !amount.is_finite() || amount < 0.0 {
            return Err("Binance COIN-M quantity must be finite and nonnegative".into());
        }
        if opts.amount != 0.0 && amount == 0.0 && opts.close != Some(true) {
            return Err("Binance COIN-M nonzero quantity rounds to zero native contracts".into());
        }
        let stop = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
        let closing_side = if opts.side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let position = cx
            .store
            .positions()
            .iter()
            .find(|p| p.symbol == opts.symbol && p.side == closing_side && p.contracts > 0.0);
        let close = stop && (opts.close == Some(true) || amount == 0.0) && position.is_some();
        if !close && amount <= 0.0 {
            return Err(
                "Binance COIN-M orders require a quantity or an existing position for a full stop"
                    .into(),
            );
        }
        let mut p = Params::default();
        p.set("symbol", opts.symbol.clone());
        p.set(
            "positionSide",
            self.position_side(cx, opts.side, opts.kind, opts.reduce_only == Some(true)),
        );
        p.set("side", side_text(opts.side));
        p.set("type", type_text(opts.kind));
        if close {
            p.set("closePosition", "true");
        } else if !cx.store.hedged() && opts.reduce_only == Some(true) {
            p.set("reduceOnly", "true");
        }
        p.set("workingType", working_type(opts));
        if opts.kind == OrderType::Limit {
            p.set(
                "price",
                number(positive_price(opts.price, market.precision.price)?),
            );
            p.set(
                "timeInForce",
                tif_text(opts.time_in_force.unwrap_or(TimeInForce::GoodTillCancel)),
            );
        } else if stop {
            p.set(
                "stopPrice",
                number(positive_price(
                    opts.trigger_price.or(opts.price),
                    market.precision.price,
                )?),
            );
        } else if opts.kind == OrderType::TrailingStopLoss {
            let target = positive_price(opts.price, market.precision.price)?;
            let last = cx.store.ticker(&opts.symbol).map_or(0.0, |t| t.last);
            if !last.is_finite() || last <= 0.0 {
                return Err("Binance COIN-M trailing orders require a valid ticker".into());
            }
            let rate = ((last - target).abs() * 1000.0 / last).round() / 10.0;
            p.set("callbackRate", number(rate.clamp(0.1, 10.0)));
            p.set("activationPrice", number(target));
        }
        let mut out = if close {
            vec![p]
        } else {
            sized_payloads(p, amount, market.limits.amount.max, market.precision.amount)?
        };
        for p in &mut out {
            p.set("newClientOrderId", self.generated_id()?);
        }
        for (kind, trigger) in [
            (OrderType::StopLoss, opts.stop_loss),
            (OrderType::TakeProfit, opts.take_profit),
        ] {
            if let Some(trigger) = trigger {
                let mut bracket = opts.clone();
                bracket.kind = kind;
                bracket.side = if opts.side == OrderSide::Buy {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                };
                bracket.price = Some(trigger);
                bracket.trigger_price = None;
                bracket.stop_loss = None;
                bracket.take_profit = None;
                bracket.reduce_only = Some(true);
                bracket.close = Some(false);
                out.extend(self.format_inverse(cx, &bracket)?);
            }
        }
        Ok(out)
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
        if self.trading.jobs.values().any(|job| {
            job.amendment
                .as_ref()
                .is_some_and(|amendment| amendment.id == id)
        }) {
            return Err("An amendment is already pending for this order".into());
        }
        let order = cx
            .store
            .order(id)
            .cloned()
            .ok_or_else(|| format!("Order {id} not found"))?;
        if order.status != OrderStatus::Open {
            return Err("Only an open order can be amended".into());
        }
        let replacement = PlaceOrderOpts {
            symbol: order.symbol.clone(),
            kind: order.kind,
            side: order.side,
            amount: amount.unwrap_or(order.amount),
            price: Some(price.unwrap_or(order.price)),
            trigger_price: None,
            stop_loss: None,
            take_profit: None,
            reduce_only: Some(order.reduce_only),
            time_in_force: order.time_in_force,
            trigger_type: None,
            tp_trigger_type: (order.kind == OrderType::TakeProfit)
                .then_some(order.trigger_type)
                .flatten(),
            sl_trigger_type: (order.kind == OrderType::StopLoss)
                .then_some(order.trigger_type)
                .flatten(),
            client_order_id: None,
            max_slippage_pct: None,
            bybit_bbo: None,
            bybit_slippage_tolerance_ticks: None,
            implied_volatility: None,
            close: Some(order.close.unwrap_or(false)),
            stp_mode: None,
        };
        // Validate before canceling; build the final payload after its cumulative fill is known.
        self.plan_place(cx, std::slice::from_ref(&replacement))?;
        let mut job = self.plan_cancel(std::slice::from_ref(&order));
        let step = cx
            .store
            .market(&order.symbol)
            .map_or(1.0, |market| market.precision.amount);
        let target_total = num::adjust(replacement.amount, step);
        job.amendment = Some(Amendment {
            id: id.to_owned(),
            target_total,
            replacement,
        });
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
        let wants_close = stop_or_tp
            && (amount == 0.0 || (!self.options.portfolio_margin && opts.close == Some(true)));
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
            params.set("clientAlgoId", self.generated_id()?);
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
                let id = self.client_id(&opts.client_order_id)?;
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
            params.set("clientAlgoId", self.generated_id()?);
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
        params.set("clientAlgoId", self.generated_id()?);
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

fn positive_price(value: Option<f64>, precision: f64) -> Result<f64, String> {
    let price = value.map(|v| num::adjust(v, precision)).unwrap_or(0.0);
    if !price.is_finite() || price <= 0.0 {
        return Err("Binance price/trigger must be finite and positive after rounding".into());
    }
    Ok(price)
}

fn sized_payloads(params: Params, amount: f64, max: f64, step: f64) -> Result<Vec<Params>, String> {
    let max = if max.is_finite() && max > 0.0 {
        num::adjust(max, step)
    } else {
        amount
    };
    if !max.is_finite() || max <= 0.0 {
        return Err("Binance quantity limit must permit a positive lot".into());
    }
    let count = (amount / max).floor();
    if count > 10000.0 {
        return Err("Binance order requires too many native lots".into());
    }
    let mut out = Vec::new();
    let at = params.0.len();
    for _ in 0..count as usize {
        out.push(with_quantity(&params, at, max));
    }
    let rest = num::adjust(amount - count * max, step);
    if rest > 0.0 {
        out.push(with_quantity(&params, at, rest));
    }
    Ok(out)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelAck {
    order_id: u64,
    symbol: String,
    status: String,
    orig_qty: String,
    executed_qty: String,
    #[serde(default)]
    client_order_id: Option<String>,
    #[serde(default)]
    orig_client_order_id: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum CancelCode {
    Number(u16),
    Text(String),
}

impl CancelCode {
    fn success(&self) -> bool {
        match self {
            Self::Number(code) => *code == 200,
            Self::Text(code) => code == "200",
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum CancelRow {
    Order(CancelAck),
    Error { code: i64, msg: String },
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum SpotCancelReply {
    Order(CancelAck),
    List(CancelList),
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelList {
    order_list_id: u64,
    symbol: String,
    list_status_type: String,
    list_order_status: String,
    orders: Vec<CancelListMember>,
    order_reports: Vec<CancelAck>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelListMember {
    symbol: String,
    order_id: u64,
    client_order_id: String,
}

fn spot_cancel_rows(body: &str, symbol: &str) -> Result<Vec<CancelAck>, ()> {
    let rows = serde_json::from_str::<Vec<SpotCancelReply>>(body).map_err(|_| ())?;
    let mut flat = Vec::new();
    for row in rows {
        match row {
            SpotCancelReply::Order(ack) => flat.push(ack),
            SpotCancelReply::List(list) => {
                let _ = list.order_list_id;
                if list.symbol != symbol
                    || list.list_status_type != "ALL_DONE"
                    || list.list_order_status != "ALL_DONE"
                    || list.orders.is_empty()
                    || list.orders.len() != list.order_reports.len()
                {
                    return Err(());
                }
                let mut members = std::collections::HashSet::new();
                for member in &list.orders {
                    if member.symbol != symbol
                        || !members.insert(member.order_id)
                        || !list.order_reports.iter().any(|ack| {
                            ack.order_id == member.order_id
                                && (ack.orig_client_order_id.as_deref()
                                    == Some(member.client_order_id.as_str())
                                    || ack.client_order_id.as_deref()
                                        == Some(member.client_order_id.as_str()))
                        })
                    {
                        return Err(());
                    }
                }
                flat.extend(list.order_reports);
            }
        }
    }
    Ok(flat)
}

fn cancel_quantities(ack: &CancelAck) -> Option<(f64, f64)> {
    let amount = ack.orig_qty.parse::<f64>().ok()?;
    let filled = ack.executed_qty.parse::<f64>().ok()?;
    (amount.is_finite() && filled.is_finite() && amount >= 0.0 && filled >= 0.0 && filled <= amount)
        .then_some((amount, filled))
}

fn canceled_status(status: &str) -> bool {
    matches!(
        status,
        "CANCELED" | "CANCELLED" | "FILLED" | "EXPIRED" | "EXPIRED_IN_MATCH"
    )
}

fn mark_canceled_preserving_closed(cx: &mut Cx<'_, Pending>, ids: &[String]) {
    for id in ids {
        if cx
            .store
            .order(id)
            .is_some_and(|order| order.status != OrderStatus::Closed)
        {
            cx.store.set_order_status(id, OrderStatus::Canceled);
        }
    }
}

impl BinanceFutures {
    fn mark_canceled(&mut self, cx: &mut Cx<'_, Pending>, ids: &[String]) {
        for id in ids {
            if let Some(order) = cx.store.order(id) {
                let symbol = order.symbol.clone();
                let filled = order.filled;
                let native_regular = self.is_inverse() || !self.order_uses_strategy(order);
                let closed = order.status == OrderStatus::Closed;
                if !native_regular {
                    self.observe_strategy_terminal(&symbol, id);
                } else {
                    let native = if self.is_spot() || self.options.portfolio_margin {
                        self.spot_native_order_id(&symbol, id)
                    } else {
                        id.parse().ok()
                    };
                    if let Some(native) = native {
                        if closed {
                            self.observe_native_terminal(&symbol, native, filled);
                        } else {
                            self.observe_unquantified_cancel(&symbol, native, filled);
                        }
                    }
                }
            }
        }
        mark_canceled_preserving_closed(cx, ids);
        self.fence_order_snapshots();
    }

    fn cancel_target_is_current(
        &self,
        cx: &Cx<'_, Pending>,
        target: &CancelTarget,
        strategy: bool,
    ) -> bool {
        let Some(native) = target.native else {
            return false;
        };
        if self.options.portfolio_margin
            && let Some(current) = self.pm_owner_matches(
                &target.symbol,
                &target.id,
                if strategy {
                    PmOrderSource::Strategy(native)
                } else {
                    PmOrderSource::Regular(native)
                },
            )
        {
            return current && !self.client_order_row_is_foreign(cx, &target.symbol, &target.id);
        }
        let current = if self.options.portfolio_margin && strategy {
            self.pm_strategy_native_order_id(&target.symbol, &target.id)
        } else if self.is_spot() || self.options.portfolio_margin {
            self.spot_native_order_id(&target.symbol, &target.id)
        } else {
            target.id.parse().ok()
        };
        target.native.is_some()
            && current == target.native
            && !self.client_order_row_is_foreign(cx, &target.symbol, &target.id)
            && cx.store.order(&target.id).is_none_or(|order| {
                (!(self.is_spot() || self.is_inverse()) && is_algo(order.kind)) == strategy
            })
    }

    fn observe_captured_cancel(
        &mut self,
        cx: &Cx<'_, Pending>,
        target: &CancelTarget,
        strategy: bool,
        current: bool,
    ) {
        let Some(native) = target.native else { return };
        if strategy {
            if self.options.portfolio_margin {
                self.observe_native_strategy_terminal(&target.symbol, native);
            } else {
                self.observe_strategy_terminal(&target.symbol, &target.id);
            }
        } else {
            let order = current.then(|| cx.store.order(&target.id)).flatten();
            let filled = self
                .known_native_filled(&target.symbol, native)
                .max(order.map_or(0.0, |order| order.filled));
            if order.is_some_and(|order| order.status == OrderStatus::Closed) {
                self.observe_native_terminal(&target.symbol, native, filled);
            } else {
                self.observe_unquantified_cancel(&target.symbol, native, filled);
            }
        }
    }

    fn mark_captured_canceled(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        targets: &[CancelTarget],
        strategy: bool,
    ) {
        let mut unchanged = Vec::new();
        for target in targets {
            let current = self.cancel_target_is_current(cx, target, strategy);
            self.observe_captured_cancel(cx, target, strategy, current);
            if current {
                unchanged.push(target.id.clone());
            }
        }
        mark_canceled_preserving_closed(cx, &unchanged);
        self.fence_order_snapshots();
    }

    fn cancel_target(&self, order: &Order) -> CancelTarget {
        CancelTarget {
            id: order.id.clone(),
            symbol: order.symbol.clone(),
            native: if self.options.portfolio_margin && self.order_uses_strategy(order) {
                self.pm_strategy_native_order_id(&order.symbol, &order.id)
            } else if self.is_spot() || self.options.portfolio_margin {
                self.spot_native_order_id(&order.symbol, &order.id)
            } else {
                order.id.parse().ok()
            },
        }
    }

    fn valid_cancel_ack(&self, target: &CancelTarget, ack: &CancelAck) -> bool {
        ack.order_id > 0
            && ack.symbol == target.symbol
            && target.native.is_none_or(|native| native == ack.order_id)
            && (!(self.is_spot() || self.options.portfolio_margin)
                || ack.orig_client_order_id.as_deref() == Some(target.id.as_str())
                || ack.client_order_id.as_deref() == Some(target.id.as_str()))
            && canceled_status(&ack.status)
            && cancel_quantities(ack).is_some()
    }

    fn apply_cancel_ack(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        target: &CancelTarget,
        ack: &CancelAck,
    ) -> bool {
        let (amount, filled) = cancel_quantities(ack).expect("validated cancellation quantities");
        let now = cx.now;
        let inverse = self.is_inverse();
        let mut final_filled = filled.max(self.known_native_filled(&ack.symbol, ack.order_id));
        let current = self.cancel_target_is_current(cx, target, false);
        if current {
            cx.store.update_order(&target.id, |order| {
                order.filled = order.filled.max(final_filled);
                final_filled = order.filled;
                order.remaining = (amount - order.filled).max(0.0);
                if inverse {
                    order
                        .extra
                        .get_or_insert_with(Default::default)
                        .remaining_native = Some(order.remaining);
                }
                if order.status != OrderStatus::Closed {
                    order.status =
                        if ack.status == "FILLED" || (amount > 0.0 && order.filled >= amount) {
                            OrderStatus::Closed
                        } else {
                            OrderStatus::Canceled
                        };
                }
                order.last_update = now;
            });
        }
        self.observe_native_terminal(&ack.symbol, ack.order_id, final_filled);
        current
    }

    fn remember_cancel_all_row(&mut self, cx: &mut Cx<'_, Pending>, target: &CancelTarget) {
        let current = self.cancel_target_is_current(cx, target, false);
        self.observe_captured_cancel(cx, target, false, current);
    }

    fn confirm_cancel(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        proof: CancelProof,
        body: &str,
    ) -> Option<(i64, String)> {
        let invalid = || {
            Some((
                0,
                "Binance cancellation acknowledgement malformed or mismatched".to_owned(),
            ))
        };
        match proof {
            CancelProof::One(target) => {
                let Ok(ack) = serde_json::from_str::<CancelAck>(body) else {
                    return invalid();
                };
                if !self.valid_cancel_ack(&target, &ack) {
                    return invalid();
                }
                if !self.apply_cancel_ack(cx, &target, &ack) {
                    return invalid();
                }
            }
            CancelProof::Algo(target) => {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Ack {
                    algo_id: u64,
                    code: CancelCode,
                }
                let Ok(ack) = serde_json::from_str::<Ack>(body) else {
                    return invalid();
                };
                if ack.algo_id == 0 || target.native != Some(ack.algo_id) || !ack.code.success() {
                    return invalid();
                }
                let current = self.cancel_target_is_current(cx, &target, true);
                self.mark_captured_canceled(cx, &[target], true);
                if !current {
                    return invalid();
                }
            }
            CancelProof::Strategy(target) => {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Ack {
                    symbol: String,
                    strategy_id: u64,
                    new_client_strategy_id: String,
                    strategy_status: String,
                }
                let Ok(ack) = serde_json::from_str::<Ack>(body) else {
                    return invalid();
                };
                if ack.strategy_id == 0
                    || target
                        .native
                        .is_some_and(|native| native != ack.strategy_id)
                    || ack.symbol != target.symbol
                    || ack.new_client_strategy_id != target.id
                    || ack.strategy_status != "CANCELED"
                {
                    return invalid();
                }
                let current = self.cancel_target_is_current(cx, &target, true);
                self.mark_captured_canceled(cx, &[target], true);
                if !current {
                    return invalid();
                }
            }
            CancelProof::Batch(targets) => {
                let Ok(rows) = serde_json::from_str::<Vec<CancelRow>>(body) else {
                    return invalid();
                };
                let mut error = (rows.len() != targets.len()).then(|| {
                    (
                        0,
                        "Binance batch cancellation response count does not match request"
                            .to_owned(),
                    )
                });
                let mut seen = std::collections::HashSet::new();
                for row in rows {
                    match row {
                        CancelRow::Order(ack) => {
                            let Some(target) = targets
                                .iter()
                                .find(|target| target.native == Some(ack.order_id))
                            else {
                                error.get_or_insert_with(|| invalid().unwrap());
                                continue;
                            };
                            if !seen.insert(ack.order_id) || !self.valid_cancel_ack(target, &ack) {
                                error.get_or_insert_with(|| invalid().unwrap());
                                continue;
                            }
                            if !self.apply_cancel_ack(cx, target, &ack) {
                                error.get_or_insert_with(|| invalid().unwrap());
                            }
                        }
                        CancelRow::Error { code, msg } => {
                            error.get_or_insert((code, msg));
                        }
                    }
                }
                if seen.len() != targets.len() && error.is_none() {
                    return invalid();
                }
                return error;
            }
            CancelProof::SpotAll { symbol, targets } => {
                let Ok(rows) = spot_cancel_rows(body, &symbol) else {
                    return invalid();
                };
                let mut seen = std::collections::HashSet::new();
                for ack in &rows {
                    if targets.iter().any(|target| {
                        target.native == Some(ack.order_id) && !self.valid_cancel_ack(target, ack)
                    }) {
                        return invalid();
                    }
                    if ack.order_id == 0
                        || ack.symbol != symbol
                        || !canceled_status(&ack.status)
                        || cancel_quantities(ack).is_none()
                        || !seen.insert(ack.order_id)
                    {
                        return invalid();
                    }
                }
                for ack in &rows {
                    if let Some(target) = targets
                        .iter()
                        .find(|target| target.native == Some(ack.order_id))
                        && self.valid_cancel_ack(target, ack)
                    {
                        self.apply_cancel_ack(cx, target, ack);
                    }
                }
                let unchanged: Vec<String> = targets
                    .iter()
                    .filter(|target| self.cancel_target_is_current(cx, target, false))
                    .map(|target| target.id.clone())
                    .collect();
                for target in &targets {
                    if target.native.is_some_and(|native| !seen.contains(&native)) {
                        self.remember_cancel_all_row(cx, target);
                    }
                }
                mark_canceled_preserving_closed(cx, &unchanged);
                self.fence_order_snapshots();
            }
            CancelProof::SymbolAll { targets, strategy } => {
                #[derive(serde::Deserialize)]
                struct Success {
                    code: CancelCode,
                }
                if !matches!(serde_json::from_str::<Success>(body), Ok(ack) if ack.code.success()) {
                    return invalid();
                }
                self.mark_captured_canceled(cx, &targets, strategy);
            }
            CancelProof::CoinAll(targets) => {
                #[derive(serde::Deserialize)]
                struct Success {
                    code: i64,
                }
                if !matches!(
                    serde_json::from_str::<Success>(body),
                    Ok(Success { code: 200 })
                ) {
                    return invalid();
                }
                self.mark_captured_canceled(cx, &targets, false);
            }
        }
        None
    }
}

#[cfg(test)]
mod audit_tests {
    use super::generated_id_with;
    use crate::{BinanceFutures, Options, Variant, market_from_wire, wire::SymbolInfo};
    use safe_cex_engine::model::{
        OrderSide, OrderType, PlaceOrderOpts, Position, PositionSide, Ticker,
    };
    use safe_cex_engine::{Action, Command, Engine, Event, Notice};
    use std::cell::Cell;

    thread_local! {
        static FAIL_AT: Cell<usize> = const { Cell::new(1) };
        static IDS: Cell<usize> = const { Cell::new(0) };
    }

    fn injected_uuid() -> Result<String, String> {
        let next = IDS.get() + 1;
        IDS.set(next);
        if next == FAIL_AT.get() {
            Err("entropy unavailable".into())
        } else {
            Ok(format!("fresh-{next}"))
        }
    }

    fn limit() -> PlaceOrderOpts {
        PlaceOrderOpts {
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            amount: 2.0,
            price: Some(100.0),
            trigger_price: None,
            stop_loss: None,
            take_profit: None,
            reduce_only: None,
            time_in_force: None,
            trigger_type: None,
            tp_trigger_type: None,
            sl_trigger_type: None,
            client_order_id: None,
            max_slippage_pct: None,
            bybit_bbo: None,
            bybit_slippage_tolerance_ticks: None,
            implied_volatility: None,
            close: None,
            stp_mode: None,
        }
    }

    fn engine(variant: Variant, fail_at: usize) -> Engine<BinanceFutures> {
        FAIL_AT.set(fail_at);
        IDS.set(0);
        let mut venue = BinanceFutures::new(Options {
            variant,
            key: Some("api-key".into()),
            secret: Some("secret".into()),
            ..Options::default()
        });
        venue.trading.id_generator = Some(injected_uuid);
        let mut engine = Engine::new(venue);
        let info: SymbolInfo<'_> = serde_json::from_str(r#"{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}]}"#).unwrap();
        engine
            .store_mut()
            .set_markets(vec![market_from_wire(&info, variant)]);
        engine.store_mut().set_tickers(vec![Ticker {
            id: "BTCUSDT".into(),
            symbol: "BTCUSDT".into(),
            last: 100.0,
            ..Ticker::default()
        }]);
        engine.store_mut().upsert_position(Position {
            id: "BTCUSDT".into(),
            symbol: "BTCUSDT".into(),
            side: PositionSide::Short,
            entry_price: 100.0,
            notional: 200.0,
            leverage: 1.0,
            margin_type: None,
            position_mode: None,
            unrealized_pnl: 0.0,
            realized_pnl: None,
            contracts: 2.0,
            liquidation_price: 0.0,
            margin_size: None,
            last_update: None,
        });
        engine
    }

    #[test]
    fn uuid_failures_on_every_native_order_family_reject_before_any_write() {
        for variant in [
            Variant::UsdM,
            Variant::PortfolioMargin,
            Variant::CoinM,
            Variant::Spot,
        ] {
            let mut cases = vec![(limit(), 1)];
            let mut bracketed = limit();
            bracketed.stop_loss = Some(90.0);
            bracketed.take_profit = Some(110.0);
            cases.extend([(bracketed.clone(), 2), (bracketed, 3)]);
            for kind in [
                OrderType::StopLoss,
                OrderType::TakeProfit,
                OrderType::TrailingStopLoss,
            ] {
                if variant == Variant::Spot && kind == OrderType::TrailingStopLoss {
                    continue;
                }
                cases.push((PlaceOrderOpts { kind, ..limit() }, 1));
            }
            for (order, fail_at) in cases {
                let kind = order.kind;
                let mut engine = engine(variant, fail_at);
                engine.handle(
                    1000.0,
                    Event::Command(Command::PlaceOrders {
                        command: 91,
                        orders: vec![order],
                    }),
                );
                let actions: Vec<_> = engine.drain_actions().collect();
                assert!(
                    !actions
                        .iter()
                        .any(|action| matches!(action, Action::Http(_))),
                    "{variant:?} {kind:?} fail_at={fail_at}"
                );
                assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 91, result: Err(error) }) if error.contains("entropy unavailable"))), "{variant:?} {kind:?} fail_at={fail_at}: {actions:?}");
            }
        }
    }

    #[test]
    fn entropy_failure_never_falls_back_to_the_shared_broker_prefix() {
        let result = generated_id_with(|| {
            safe_cex_crypto::uuid::uuid_with_entropy(|_| Err("entropy unavailable".into()))
        });
        assert_eq!(
            result,
            Err("Binance client order ID generation failed: entropy unavailable".into())
        );
    }
}
