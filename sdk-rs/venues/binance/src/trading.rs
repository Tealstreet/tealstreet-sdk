//! Order entry. Each command becomes a job of sequential steps; a step's
//! requests go out together and the next step starts when they all answer.
//! Stops, take-profits and trailing stops use the Algo Order API.
use crate::account::{PmOrderSource, venue_error};
use crate::sign::Query;
use crate::wire::{BatchItem, PlacedOrder};
use crate::{BinanceFutures, Pending};
use safe_cex_engine::Level;
use safe_cex_engine::classify::{
    CodeTable, Intent, Key, Native, NativeCode, Row, Verdict, WriteFact,
};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::json::{ArrayAt, ArraySplitter, ItemCheck};
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Order, OrderIdentityKind, OrderIdentityReceipt, OrderSide, OrderStatus, OrderType,
    PlaceOrderOpts, PositionSide, StopTriggerType, TimeInForce,
};
use safe_cex_engine::request_context::{
    Context, Operation, Owner, Policy, RejectionReports, RetryPolicy, UnprovenEvidence,
};
use safe_cex_engine::setting_holds::SettingKey;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, VenueError, num,
};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};

const BROKER_PREFIX: &str = "x-s3vFFRQp";
/// Errors callers handle themselves; they are not shown to the user.
const SILENT_CODES: [i64; 4] = [-2011, -2013, -4059, -5022];
const UNKNOWN_ORDER: i64 = -2011;
const POSITION_MODE_ALREADY_SET: i64 = -4059;
const POST_ONLY_REJECTED: i64 = -5022;

pub struct Request {
    command: CommandId,
    context: Context,
    call: Call,
    resend: Resend,
}

/// What a signature-rejected write needs to be re-signed and sent once more.
struct Resend {
    method: Method,
    url: String,
    query: Query,
}

/// What a response means for its job.
#[derive(Clone)]
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

/// Borrow native facts only. Wrong or duplicate known fields invalidate a row;
/// the caller's intended identity is never a substitute for a native echo.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnprovenOrder<'a> {
    code: Option<i64>,
    #[serde(borrow)]
    msg: Option<Cow<'a, str>>,
    order_id: Option<u64>,
    algo_id: Option<u64>,
    #[serde(borrow)]
    client_order_id: Option<Cow<'a, str>>,
    strategy_id: Option<u64>,
    #[serde(borrow)]
    new_client_strategy_id: Option<Cow<'a, str>>,
}

impl<'a> UnprovenOrder<'a> {
    fn parse(body: &'a str) -> Option<Self> {
        // Derive also accepts positional sequences. Native placement facts
        // require an object, never an array interpreted as struct fields.
        if !body
            .trim_start_matches([' ', '\t', '\r', '\n'])
            .starts_with('{')
        {
            return None;
        }
        serde_json::from_str(body).ok()
    }

    fn capture_diagnostics(&self, evidence: &mut UnprovenEvidence) {
        if let Some(code) = self.code {
            evidence.venue_code(&code.to_string());
        }
        if let Some(message) = self.msg.as_deref() {
            evidence.venue_message(message);
        }
    }
}

struct Planned {
    receipts: Vec<OrderIdentityReceipt>,
    method: Method,
    path: &'static str,
    query: Query,
    call: Call,
}

#[derive(Default)]
struct Job {
    context: Option<Context>,
    steps: VecDeque<Vec<Planned>>,
    outstanding: usize,
    /// Run every step even after a failure (cancel-all's two families).
    continue_on_error: bool,
    ids: Vec<String>,
    /// Native failure controls planning only; execution facts live in the engine.
    stop: bool,
    planning_error: Option<VenueError>,
    amendment: Option<Amendment>,
}

struct Amendment {
    id: String,
    target_total: f64,
    replacement: PlaceOrderOpts,
}

#[derive(Clone)]
struct CancelTarget {
    id: String,
    symbol: String,
    native: Option<u64>,
}

#[derive(Clone)]
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
    fn generated_id(&self, cx: &Cx<'_, Pending>) -> Result<String, String> {
        #[cfg(test)]
        if let Some(generator) = self.trading.id_generator {
            return generated_id_with(generator);
        }
        generated_id_with(|| {
            safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))
        })
    }

    fn client_id(&self, cx: &Cx<'_, Pending>, input: &Option<String>) -> Result<String, String> {
        match input {
            Some(id) => Ok(id.clone()),
            None => self.generated_id(cx),
        }
    }

    pub(crate) fn on_trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        // Every refusal before admission is the command's own typed terminal.
        let operation = if matches!(
            command,
            Command::FetchStpMode { .. }
                | Command::FetchAccount { .. }
                | Command::FetchOrderBook { .. }
                | Command::FetchLeverage { .. }
        ) {
            Operation::Read
        } else {
            Operation::Mutation
        };
        if matches!(
            command,
            Command::FetchStpMode { .. }
                | Command::AmendOrderWithSymbol { .. }
                | Command::SetStpMode { .. }
                | Command::CancelOrdersWithSymbols { .. }
                | Command::FetchAccount { .. }
                | Command::FetchOrderBook { .. }
        ) {
            cx.refuse_command(
                operation,
                not_sent("This Binance command is not supported").kind(ErrorKind::Unsupported),
            );
            return;
        }
        if (self.is_spot() || self.is_options())
            && matches!(
                command,
                Command::SetLeverage { .. }
                    | Command::FetchLeverage { .. }
                    | Command::SetMarginType { .. }
                    | Command::SetPositionMode { .. }
            )
        {
            cx.refuse_command(
                operation,
                not_sent("Binance Spot does not support derivative settings")
                    .kind(ErrorKind::Unsupported),
            );
            return;
        }
        // A limit-execution stop would be re-placed as a market stop at its limit price.
        if let Command::AmendOrder { id, .. } = &command
            && self.account.limit_stops.contains(id)
        {
            cx.refuse_command(
                operation,
                not_sent("Binance stop-limit orders cannot be amended; cancel and place a new one")
                    .kind(ErrorKind::Unsupported),
            );
            return;
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
                        receipts: Vec::new(),
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
                        receipts: Vec::new(),
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
                if self.is_spot() || self.is_inverse() || self.is_options() {
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
        if !self.credentials.has_primary() {
            let error = not_sent("Binance trading needs an API key").kind(ErrorKind::AuthInvalid);
            cx.refuse_command(operation, error);
            return;
        }
        let plan = plan.and_then(|job| {
            // Reserve no partial prefix for a predictably oversized plan. The
            // root also occupies one of the engine's bounded context slots.
            let count: usize = job.steps.iter().map(Vec::len).sum();
            if count >= 256 {
                return Err("Binance command exceeds the HTTP lifecycle capacity".into());
            }
            Ok(job)
        });
        match plan {
            Ok(mut job) => {
                let count: usize = job.steps.iter().map(Vec::len).sum();
                let operation = if count == 0 && job.amendment.is_none() {
                    Operation::Read
                } else {
                    Operation::Mutation
                };
                let credential = (operation == Operation::Mutation).then(|| {
                    let (selection, credentials) = self
                        .credentials
                        .select(CredentialRole::Mutation)
                        .expect("checked above");
                    selection.credential(credentials.credential.clone())
                });
                job.context = Some(
                    match cx.admit_http_command(Policy {
                        operation,
                        credential,
                        timeout_ms: 120_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    }) {
                        Ok(context) => context,
                        Err(_) => return, // Engine owns initial admission rejection.
                    },
                );
                self.trading.jobs.insert(id, job);
                self.advance(cx, id);
            }
            Err(message) => {
                cx.command_error(id, message.clone());
                cx.refuse_command(operation, not_sent(message).kind(ErrorKind::InvalidInput));
            }
        }
    }

    /// Send the job's next step, or finish it.
    fn advance(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(job) = self.trading.jobs.get_mut(&command) else {
            return;
        };
        let stop = (job.stop || job.planning_error.is_some()) && !job.continue_on_error;
        let step = if stop { None } else { job.steps.pop_front() };
        if step.is_none()
            && !stop
            && let Some(mut amendment) = job.amendment.take()
        {
            let Some(order) = cx.store.order(&amendment.id) else {
                if let Some(job) = self.trading.jobs.get_mut(&command) {
                    job.planning_error =
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
                    Err(error) => job.planning_error = Some(error.into()),
                }
            }
            self.advance(cx, command);
            return;
        }
        let Some(step) = step.filter(|step| !step.is_empty()) else {
            let job = self.trading.jobs.remove(&command).expect("present");
            let context = job.context.expect("managed job has an authority");
            match job.planning_error {
                Some(error) => {
                    let _ = cx.fail_context(&context, error);
                }
                None => {
                    let _ = cx.done_context(
                        &context,
                        serde_json::to_string(&job.ids).unwrap_or_default(),
                    );
                }
            }
            return;
        };
        job.outstanding = step.len();
        let root = job.context.clone().expect("managed job has an authority");
        // A placement's conflicting-order cancel is a cancel leg; an
        // amendment's cancel stays part of the amend.
        let amending = job.amendment.is_some();
        for planned in step {
            let (selection, credentials) = self
                .credentials
                .select(CredentialRole::Mutation)
                .expect("checked before planning");
            let credentials = credentials.clone();
            let context = cx.http_leg_context(
                &root,
                Policy {
                    operation: Operation::Mutation,
                    credential: Some(selection.credential(credentials.credential.clone())),
                    timeout_ms: 20_000,
                    pacing: None,
                    // A -1022 signature rejection is re-signed and sent once more.
                    retry: RetryPolicy::DefinitiveRefusal,
                },
            );
            let context = context.and_then(|context| {
                if matches!(planned.call, Call::Cancel { .. }) && !amending {
                    cx.http_write_leg_intent(&context, Intent::Cancel)?;
                }
                Ok(context)
            });
            let Ok(context) = context else {
                self.trading.jobs.remove(&command);
                let _ = cx.fail_context(
                    &root,
                    VenueError::new("Binance HTTP lifecycle refused the next leg"),
                );
                return;
            };
            cx.command_progress(command, planned.receipts);
            let resend = Resend {
                method: planned.method,
                url: format!("{}{}", self.urls.account_rest, planned.path),
                query: planned.query,
            };
            if !self.send_trade(cx, command, context, planned.call, resend, credentials) {
                self.trading.jobs.remove(&command);
                let _ = cx.fail_context(
                    &root,
                    VenueError::new("Binance HTTP lifecycle refused the next leg"),
                );
                return;
            }
        }
    }

    /// Sign and send one write leg; false when the engine refuses it.
    fn send_trade(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        context: Context,
        call: Call,
        resend: Resend,
        credentials: crate::sign::Credentials,
    ) -> bool {
        cx.request_signed_managed(
            context.clone(),
            None,
            resend.method,
            resend.url.clone(),
            None,
            Box::new(self.signed_recipe(credentials, resend.query.clone())),
            Pending::Trade(Request {
                command,
                context,
                call,
                resend,
            }),
        )
        .is_ok()
    }

    pub(crate) fn capture_unproven_trade(
        &self,
        request: &Request,
        response: Response<'_>,
        evidence: &mut UnprovenEvidence,
    ) {
        if let Call::PlaceBatch { .. } = &request.call {
            // A batch-wide native refusal has an object envelope. Preserve its
            // bounded diagnostics, but a non-array can never supply batch IDs.
            if let Some(row) = UnprovenOrder::parse(response.body) {
                row.capture_diagnostics(evidence);
                return;
            }
            // Validate every row and the complete envelope before emitting a
            // prefix. Keep at most one row, never an intermediate result Vec.
            let mut validator = ArraySplitter::new(ArrayAt::Root);
            validator.feed(response.body.as_bytes(), |_| {});
            if !validator.finished() {
                return;
            }
            drop(validator);
            let mut rows = ArraySplitter::with_item_check(ArrayAt::Root, ItemCheck::Venue);
            rows.feed(response.body.as_bytes(), |row| {
                self.capture_unproven_trade_row(request, row, evidence);
            });
        } else {
            self.capture_unproven_trade_row(request, response.body, evidence);
        }
    }

    fn capture_unproven_trade_row(
        &self,
        request: &Request,
        body: &str,
        evidence: &mut UnprovenEvidence,
    ) {
        if self.is_options() {
            if matches!(request.call, Call::PlaceOne { .. })
                && let Some(id) = crate::options::placed_id(body)
            {
                evidence.order_id(&id.to_string());
            }
            return;
        }
        let Some(row) = UnprovenOrder::parse(body) else {
            return;
        };
        row.capture_diagnostics(evidence);
        // A refusal carrying an ID is contradictory native placement proof.
        if row.code.is_some() {
            return;
        }
        let positive = |id: Option<u64>| id.filter(|id| *id > 0);
        let futures = !self.is_spot() && !self.options.portfolio_margin;
        match &request.call {
            Call::PlaceOne { client_id: None } | Call::PlaceBatch { .. } if futures => {
                if let Some(id) = positive(row.order_id) {
                    evidence.order_id(&id.to_string());
                }
            }
            Call::PlaceAlgo { client_id: None } if futures && !self.is_inverse() => {
                if let Some(id) = positive(row.algo_id) {
                    evidence.order_id(&id.to_string());
                }
            }
            Call::PlaceOne {
                client_id: Some(expected),
            } if (self.is_spot() || self.options.portfolio_margin)
                && positive(row.order_id).is_some()
                && row.client_order_id.as_deref() == Some(expected.as_str()) =>
            {
                evidence.order_id(row.client_order_id.as_deref().expect("matched native echo"));
            }
            Call::PlaceAlgo {
                client_id: Some(expected),
            } if self.options.portfolio_margin
                && positive(row.strategy_id).is_some()
                && row.new_client_strategy_id.as_deref() == Some(expected.as_str()) =>
            {
                evidence.order_id(
                    row.new_client_strategy_id
                        .as_deref()
                        .expect("matched native echo"),
                );
            }
            _ => {}
        }
    }

    pub(crate) fn discard_unproven_trade(&mut self, owner: Owner) {
        let Owner::Command { id, .. } = owner else {
            return;
        };
        if self.trading.jobs.get(&id).is_some_and(|job| {
            job.context
                .as_ref()
                .is_some_and(|context| context.owner() == owner)
        }) {
            self.trading.jobs.remove(&id);
        }
    }

    pub(crate) fn on_trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        self.on_trade_result(cx, request, response);
    }
    pub(crate) fn on_trade_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        self.on_trade_result(cx, request, Err(&error.message));
    }
    fn on_trade_result(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let Request {
            command,
            context,
            call,
            resend,
        } = request;
        let native_response = response.is_ok();
        let resend_call = call.clone();
        let stopped = self.trading.jobs.get(&command).is_some_and(|job| job.stop);

        let mut reports = Vec::new();
        let mut batch_reports = false;
        let mut continue_after_item_refusal = false;
        // The shared engine filters unproven envelopes before this reducer.
        // Native malformed/mixed replies still supply their native facts here.
        let mut unproven = false;
        let outcome = match response {
            Ok(response) => match venue_error(&response) {
                Some(error) => Err(error),
                None => Ok(response.body),
            },
            Err(error) => Err((0, error.to_owned())),
        };
        let mut ids = Vec::new();
        // Native IDs on an ambiguous batch are reconciliation evidence only;
        // never feed them into accepted placement planning or caller bindings.
        let mut evidence_ids = Vec::new();
        let mut known_cancel_effects = false;
        let error = match (call, outcome) {
            (Call::PlaceOne { .. }, Ok(body)) if self.is_options() => {
                if let Some(id) = crate::options::placed_id(body) {
                    ids.push(id.to_string());
                    None
                } else {
                    Some((
                        0,
                        "Binance Options placement response has no valid native order ID".into(),
                    ))
                }
            }
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
                batch_reports = true;
                if let Ok(rows) = serde_json::from_str::<Vec<&RawValue>>(body) {
                    if rows.len() != expected {
                        unproven = true;
                        for row in rows {
                            if let Ok(item) = serde_json::from_str::<BatchItem>(row.get())
                                && let (Some(id), None) = (item.order_id, item.code)
                                && id > 0
                            {
                                evidence_ids.push(id.to_string());
                            }
                        }
                        rejection = Some((
                            0,
                            "Binance batch response count does not match the request".into(),
                        ));
                    } else if let Ok(items) = serde_json::from_str::<Vec<BatchItem>>(body) {
                        for item in items {
                            match (item.order_id, item.code) {
                                (Some(id), None) => ids.push(id.to_string()),
                                (_, code) => {
                                    let code = code.unwrap_or(0);
                                    unproven |= item_unknown(code);
                                    rejection.get_or_insert((
                                        code,
                                        item.msg
                                            .as_deref()
                                            .unwrap_or("Binance batch order rejected")
                                            .to_owned(),
                                    ));
                                    if code != POST_ONLY_REJECTED {
                                        let msg =
                                            item.msg.as_deref().unwrap_or("batch order rejected");
                                        reports.push(msg.to_owned());
                                    }
                                }
                            }
                        }
                    } else {
                        unproven = true;
                        rejection = Some((0, "Binance batch response malformed".into()));
                    }
                    continue_after_item_refusal = !strict && !unproven;
                } else {
                    unproven = true;
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
                Some(proof) => {
                    self.confirm_cancel(cx, proof, body, &mut known_cancel_effects, &mut unproven)
                }
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
            ) if !unproven => {
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
                cx.hold_setting(SettingKey::Leverage(symbol));
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
                cx.hold_setting(SettingKey::MarginType(symbol));
                None
            }
            // Binance's dualSidePosition is account-wide: the store's hedge flag.
            (Call::PositionMode { hedged }, Ok(_)) => {
                cx.store.set_hedged(hedged);
                cx.hold_setting(SettingKey::Hedged);
                None
            }
            (Call::PositionMode { hedged }, Err((POSITION_MODE_ALREADY_SET, _))) if !unproven => {
                cx.store.set_hedged(hedged);
                cx.hold_setting(SettingKey::Hedged);
                None
            }
            (_, Err(error)) => Some(error),
        };
        let mut native_fact = None;
        if let Some((code, message)) = error {
            if !batch_reports && !SILENT_CODES.contains(&code) {
                reports.push(message.clone());
            }
            if native_response {
                // Code 0 is no native code: a non-JSON HTTP error or an
                // unproven acknowledgement. The engine classifies both.
                let native = (code != 0).then_some(Native {
                    code: NativeCode::Int(code),
                    message: &message,
                });
                let Ok(fact) = cx.classify_write(&context, &BINANCE, native.as_ref(), &message)
                else {
                    return;
                };
                native_fact = Some(if unproven { fact.unproven() } else { fact });
            }
            if let Some(job) = self.trading.jobs.get_mut(&command)
                && !continue_after_item_refusal
            {
                job.stop = true;
            }
        }
        if native_response {
            let fact = match native_fact {
                Some(fact) => {
                    let evidence: Vec<_> = ids.iter().chain(&evidence_ids).cloned().collect();
                    WriteFact::Failed {
                        fact: if evidence.is_empty() {
                            fact
                        } else {
                            fact.with_order_ids(&evidence)
                        },
                        known_effects: known_cancel_effects,
                    }
                }
                None => WriteFact::Accepted {
                    order_ids: ids.clone(),
                },
            };
            let metadata = RejectionReports::new(reports).unwrap_or_default();
            let recorded = cx.http_write_fact(&context, fact.clone(), Some(metadata));
            let recorded = if matches!(
                recorded,
                Err(safe_cex_engine::request_context::Error::Capacity)
            ) {
                cx.http_write_fact(&context, fact, Some(RejectionReports::default()))
            } else {
                recorded
            };
            match recorded {
                Err(_) => return,
                // One re-signed resend of a definitive signature refusal; the
                // journal refuses a second, which then fails as recorded.
                // A job already stopping, or ended, never sends again.
                Ok(Some(grant))
                    if may_resend(stopped, self.trading.jobs.contains_key(&command)) =>
                {
                    // Undo the stop this refusal just set: the leg is resent, not failed.
                    if let Some(job) = self.trading.jobs.get_mut(&command) {
                        job.stop = stopped;
                    }
                    self.resend_trade(cx, command, &context, grant, resend_call, resend);
                    return;
                }
                Ok(_) => {}
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

    fn resend_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        context: &Context,
        grant: safe_cex_engine::request_context::ResendGrant,
        call: Call,
        resend: Resend,
    ) {
        let sent = self
            .credentials
            .select(CredentialRole::Mutation)
            .ok()
            .map(|(_, credentials)| credentials.clone())
            .and_then(|credentials| {
                let context = cx.http_resend_context(context, grant).ok()?;
                self.send_trade(cx, command, context, call, resend, credentials)
                    .then_some(())
            });
        if sent.is_none()
            && let Some(job) = self.trading.jobs.remove(&command)
            && let Some(root) = job.context
        {
            let _ = cx.fail_context(
                &root,
                VenueError::new("Binance HTTP lifecycle refused the signature resend"),
            );
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
        let replaces_close_orders = !pm && !self.is_spot() && !self.is_options();
        for opts in orders {
            if replaces_close_orders {
                for existing in self.conflicting_close_orders(cx, opts) {
                    if !replaced.iter().any(|o| o.id == existing.id) {
                        replaced.push(existing);
                    }
                }
            }
            let (r, a) = if self.is_options() {
                (self.format_option(cx, opts)?, Vec::new())
            } else if self.is_spot() {
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
            cx.notice(
                Level::Info,
                code::ORDERS_REPLACED,
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
                    receipts: identities(only),
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
                        receipts: lot.iter().flat_map(identities).collect(),
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
                receipts: identities(&params),
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
        if self.is_options() {
            for order in orders {
                job.steps.push_back(vec![Planned {
                    receipts: Vec::new(),
                    method: Method::Delete,
                    path: self.paths.order,
                    query: Query::new()
                        .str("symbol", &order.symbol)
                        .str("orderId", &order.id),
                    call: Call::Cancel {
                        ids: vec![order.id.clone()],
                        proof: Some(CancelProof::One(self.cancel_target(order))),
                    },
                }]);
            }
            return job;
        }
        if self.options.portfolio_margin || self.is_spot() {
            // One at a time, by the client ids the orders were placed with.
            for order in orders {
                let (path, field) = if !self.is_spot() && self.order_uses_strategy(order) {
                    (self.paths.conditional_order, "newClientStrategyId")
                } else {
                    (self.paths.order, "origClientOrderId")
                };
                job.steps.push_back(vec![Planned {
                    receipts: Vec::new(),
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
                    receipts: Vec::new(),
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
                    receipts: Vec::new(),
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
                receipts: Vec::new(),
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

    fn format_option(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Vec<Params>, String> {
        if opts.kind != OrderType::Limit
            || opts.implied_volatility.is_some()
            || opts.trigger_price.is_some()
            || opts.stop_loss.is_some()
            || opts.take_profit.is_some()
        {
            return Err("Binance Options requires a limit price and supports no trigger or implied-volatility order".into());
        }
        if let Some(id) = &opts.client_order_id
            && !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err("Invalid Binance Options client order ID".into());
        }
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let amount = num::adjust(opts.amount, market.precision.amount);
        if !amount.is_finite()
            || amount < market.limits.amount.min
            || amount <= 0.
            || amount > market.limits.amount.max
        {
            return Err("Binance Options quantity is outside its native lot bounds".into());
        }
        let price = positive_price(opts.price, market.precision.price)?;
        let post_only = opts.time_in_force == Some(TimeInForce::PostOnly);
        let mut params = Params::default();
        params.set("symbol", opts.symbol.clone());
        params.set("side", side_text(opts.side));
        params.set("type", "LIMIT");
        params.set("quantity", number(amount));
        params.set("price", number(price));
        params.set(
            "timeInForce",
            if post_only {
                "GTC"
            } else {
                tif_text(opts.time_in_force.unwrap_or(TimeInForce::GoodTillCancel))
            },
        );
        if post_only {
            params.set("postOnly", "true");
        }
        if opts.reduce_only == Some(true) {
            params.set("reduceOnly", "true");
        }
        params.set("clientOrderId", self.client_id(cx, &opts.client_order_id)?);
        Ok(vec![params])
    }

    fn format_spot(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Vec<Params>, String> {
        // Spot has no position to reduce, so the flags are dropped as in TS; a sell
        // beyond the balance fails at Binance.
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
            p.set(
                "newClientOrderId",
                self.client_id(cx, &opts.client_order_id)?,
            );
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
                    part.set("newClientOrderId", self.generated_id(cx)?);
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
            p.set("newClientOrderId", self.generated_id(cx)?);
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
            params.set("clientAlgoId", self.generated_id(cx)?);
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
                let id = self.client_id(cx, &opts.client_order_id)?;
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
            params.set("clientAlgoId", self.generated_id(cx)?);
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
        params.set("clientAlgoId", self.generated_id(cx)?);
        Ok(params)
    }
}

/// Binance native codes and documented statuses
/// (developers.binance.com/docs/derivatives/usds-margined-futures/error-code).
pub(crate) const BINANCE: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Int(-1000),
            verdict: Verdict::Unknown(None),
            only: None,
        },
        Row {
            key: Key::Int(-1007),
            verdict: Verdict::Unknown(Some(ErrorKind::Timeout)),
            only: None,
        },
        Row {
            key: Key::Int(-1001),
            verdict: Verdict::Unknown(Some(ErrorKind::VenueUnavailable)),
            only: None,
        },
        Row {
            key: Key::Int(-1006),
            verdict: Verdict::Unknown(Some(ErrorKind::VenueUnavailable)),
            only: None,
        },
        Row {
            key: Key::Int(-1008),
            verdict: Verdict::Refused(ErrorKind::VenueUnavailable),
            only: None,
        },
        Row {
            key: Key::Int(-1003),
            verdict: Verdict::Refused(ErrorKind::RateLimited),
            only: None,
        },
        Row {
            key: Key::Int(-1015),
            verdict: Verdict::Refused(ErrorKind::RateLimited),
            only: None,
        },
        // A rejected signature executed nothing: re-sign and send once more.
        Row {
            key: Key::Int(-1022),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(-2008),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(-2014),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(-2015),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(-1002),
            verdict: Verdict::Refused(ErrorKind::AuthPermission),
            only: None,
        },
        // BALANCE_NOT_SUFFICIENT, MARGIN_NOT_SUFFICIEN, and MIN_LEVERAGE_RATIO
        // ("Leverage is smaller than permitted: insufficient margin balance.").
        Row {
            key: Key::Int(-2018),
            verdict: Verdict::Refused(ErrorKind::InsufficientBalance),
            only: None,
        },
        Row {
            key: Key::Int(-2019),
            verdict: Verdict::Refused(ErrorKind::InsufficientBalance),
            only: None,
        },
        Row {
            key: Key::Int(-2028),
            verdict: Verdict::Refused(ErrorKind::InsufficientBalance),
            only: None,
        },
        // Order rules the order itself broke: UNABLE_TO_FILL,
        // ORDER_WOULD_IMMEDIATELY_TRIGGER, REDUCE_ONLY_REJECT, FOK_ORDER_REJECT
        // and GTX_ORDER_REJECT (post-only).
        Row {
            key: Key::Range(-2022, -2020),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: None,
        },
        Row {
            key: Key::Range(-5022, -5021),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: None,
        },
        Row {
            key: Key::Range(-1199, -1100),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: None,
        },
        Row {
            key: Key::Range(-4999, -4000),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: None,
        },
    ],
    // Non-negative codes are success envelopes, never failure evidence.
    success: &[Key::Range(0, i64::MAX)],
    // Every other documented code is a pre-execution refusal.
    unlisted: Verdict::Refused(ErrorKind::Rejected),
    // "4XX return codes are used for malformed requests"; 418 is an IP auto-ban
    // and 429 a rate limit. 403 is the WAF, but a bare edge 403 may be another
    // block, so it is a plain refusal. 409 (partial cancel-replace) is not one.
    status: &[
        (400, Verdict::Refused(ErrorKind::InvalidInput)),
        (401, Verdict::Refused(ErrorKind::AuthInvalid)),
        (403, Verdict::Refused(ErrorKind::Rejected)),
        (418, Verdict::Refused(ErrorKind::RateLimited)),
        (429, Verdict::Refused(ErrorKind::RateLimited)),
    ],
};

/// Whether a native batch-item code leaves its write unproven. Code 0 is a
/// missing item code, which proves nothing.
fn item_unknown(code: i64) -> bool {
    let native = Native {
        code: NativeCode::Int(code),
        message: "",
    };
    code == 0 || matches!(BINANCE.verdict(&native, Intent::Place), Verdict::Unknown(_))
}

#[cfg(test)]
#[test]
fn binance_code_table_satisfies_the_engine_invariants() {
    safe_cex_engine::classify::assert_table(&BINANCE);
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

fn single(method: Method, path: &'static str, query: Query, call: Call) -> Job {
    Job {
        steps: VecDeque::from([vec![Planned {
            receipts: Vec::new(),
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
    #[serde(deserialize_with = "crate::options::native_id")]
    order_id: u64,
    symbol: String,
    status: String,
    #[serde(alias = "quantity")]
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
                    self.observe_strategy_terminal(cx, &symbol, id);
                } else {
                    let native = if self.is_spot() || self.options.portfolio_margin {
                        self.spot_native_order_id(&symbol, id)
                    } else {
                        id.parse().ok()
                    };
                    if let Some(native) = native {
                        if closed {
                            self.observe_native_terminal(cx, &symbol, native, filled);
                        } else {
                            self.observe_unquantified_cancel(cx, &symbol, native, filled);
                        }
                    }
                }
            }
        }
        mark_canceled_preserving_closed(cx, ids);
        self.fence_order_snapshots(cx);
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
        cx: &mut Cx<'_, Pending>,
        target: &CancelTarget,
        strategy: bool,
        current: bool,
    ) {
        let Some(native) = target.native else { return };
        if strategy {
            if self.options.portfolio_margin {
                self.observe_native_strategy_terminal(cx, &target.symbol, native);
            } else {
                self.observe_strategy_terminal(cx, &target.symbol, &target.id);
            }
        } else {
            let order = current.then(|| cx.store.order(&target.id)).flatten();
            let filled = self
                .known_native_filled(&target.symbol, native)
                .max(order.map_or(0.0, |order| order.filled));
            if order.is_some_and(|order| order.status == OrderStatus::Closed) {
                self.observe_native_terminal(cx, &target.symbol, native, filled);
            } else {
                self.observe_unquantified_cancel(cx, &target.symbol, native, filled);
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
        self.fence_order_snapshots(cx);
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
        self.observe_native_terminal(cx, &ack.symbol, ack.order_id, final_filled);
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
        known_effects: &mut bool,
        unproven: &mut bool,
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
                let mismatched = rows.len() != targets.len();
                *unproven |= mismatched;
                let mut error = mismatched.then(|| {
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
                                *unproven = true;
                                error.get_or_insert_with(|| invalid().unwrap());
                                continue;
                            };
                            if !seen.insert(ack.order_id) || !self.valid_cancel_ack(target, &ack) {
                                *unproven = true;
                                error.get_or_insert_with(|| invalid().unwrap());
                                continue;
                            }
                            if self.apply_cancel_ack(cx, target, &ack) {
                                *known_effects = true;
                            } else {
                                *unproven = true;
                                error.get_or_insert_with(|| invalid().unwrap());
                            }
                        }
                        CancelRow::Error { code, msg } => {
                            *unproven |= item_unknown(code);
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
                self.fence_order_snapshots(cx);
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
                    Ok(Success { code }) if code == 200 || (self.is_options() && code == 0)
                ) {
                    return invalid();
                }
                self.mark_captured_canceled(cx, &targets, false);
            }
        }
        None
    }
}

fn identities(params: &Params) -> Vec<OrderIdentityReceipt> {
    let Some(symbol) = params.get("symbol") else {
        return Vec::new();
    };
    [
        "newClientOrderId",
        "clientAlgoId",
        "newClientStrategyId",
        "clientOrderId",
    ]
    .into_iter()
    .filter_map(|key| {
        params.get(key).map(|identity| OrderIdentityReceipt {
            symbol: symbol.clone(),
            kind: OrderIdentityKind::ClientOrderId,
            identity,
        })
    })
    .collect()
}

#[cfg(test)]
mod audit_tests {
    use super::generated_id_with;
    use crate::{BinanceFutures, Variant, market_from_wire, wire::SymbolInfo};
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
        let (mut venue, signer) =
            crate::test_config::keyed(variant, "api-key", "fixture-seed").build();
        venue.trading.id_generator = Some(injected_uuid);
        let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
        engine.set_signer(Some(signer)).unwrap();
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
                assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 91, error }) if error.message.contains("entropy unavailable") && error.outcome == Some(safe_cex_engine::Outcome::NotSent))), "{variant:?} {kind:?} fail_at={fail_at}: {actions:?}");
            }
        }
    }

    struct RefusingSigner;
    impl safe_cex_engine::signing::Signer for RefusingSigner {
        fn sign(
            &self,
            _: safe_cex_engine::signing::SignRequest,
        ) -> safe_cex_engine::signing::SignFuture {
            Box::pin(async { Err(safe_cex_engine::signing::SignError::Refused) })
        }
        fn immediate(&self) -> bool {
            true
        }
        fn sign_ready(
            &self,
            _: &safe_cex_engine::signing::SignRequest,
        ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
        {
            Some(Err(safe_cex_engine::signing::SignError::Refused))
        }
    }

    #[test]
    fn refused_signer_reports_not_sent_once_for_every_binance_family() {
        for variant in [
            Variant::UsdM,
            Variant::PortfolioMargin,
            Variant::CoinM,
            Variant::Spot,
        ] {
            let mut engine = engine(variant, usize::MAX);
            engine
                .set_signer(Some(std::sync::Arc::new(RefusingSigner)))
                .unwrap();
            engine.handle(
                1000.0,
                Event::Command(Command::PlaceOrders {
                    command: 92,
                    orders: vec![limit()],
                }),
            );
            let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 1000.);
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_))),
                "{variant:?}"
            );
            let failures: Vec<_> = actions
                .iter()
                .filter_map(|action| match action {
                    Action::Notice(Notice::CommandFailed { command: 92, error }) => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(failures.len(), 1, "{variant:?}: {actions:?}");
            assert_eq!(
                failures[0].outcome,
                Some(safe_cex_engine::Outcome::NotSent),
                "{variant:?}"
            );
            assert!(failures[0].message.to_lowercase().contains("sign"));
            assert_eq!(engine.drain_actions().count(), 0);
        }
    }

    struct HungSigner;
    impl safe_cex_engine::signing::Signer for HungSigner {
        fn sign(
            &self,
            _: safe_cex_engine::signing::SignRequest,
        ) -> safe_cex_engine::signing::SignFuture {
            Box::pin(std::future::pending())
        }
    }

    #[test]
    fn signing_failures_in_private_reads_never_become_successful_empty_history() {
        use safe_cex_engine::{ErrorKind, Outcome, Query};
        for variant in [Variant::UsdM, Variant::PortfolioMargin] {
            for hung in [false, true] {
                let mut commands = vec![Command::Query {
                    command: 94,
                    query: Query::OrderByClientId {
                        symbol: "BTCUSDT".into(),
                        client_order_id: "lookup".into(),
                    },
                }];
                if matches!(variant, Variant::UsdM | Variant::PortfolioMargin) {
                    commands.push(Command::FetchAccount { command: 94 });
                    commands.push(Command::Query {
                        command: 94,
                        query: Query::OrderHistory {
                            symbol: Some("BTCUSDT".into()),
                            from_ms: None,
                            to_ms: None,
                            limit: None,
                        },
                    });
                    commands.push(Command::Query {
                        command: 94,
                        query: Query::Fills {
                            symbol: Some("BTCUSDT".into()),
                            order_id: None,
                            from_ms: None,
                            to_ms: None,
                            limit: None,
                        },
                    });
                }
                for command in commands {
                    let mut engine = engine(variant, usize::MAX);
                    let signer: safe_cex_engine::signing::Provider = if hung {
                        std::sync::Arc::new(HungSigner)
                    } else {
                        std::sync::Arc::new(RefusingSigner)
                    };
                    engine.set_signer(Some(signer)).unwrap();
                    let label = format!("{variant:?} hung={hung} {command:?}");
                    engine.handle(1000.0, Event::Command(command));
                    let mut actions: Vec<_> = engine.drain_actions().collect();
                    let preparations: Vec<_> = actions
                        .iter()
                        .filter_map(|action| match action {
                            Action::ManagedHttp(action) => match action.as_ref() {
                                safe_cex_engine::request_context::HttpAction::Prepare {
                                    preparation,
                                    ..
                                } => Some(*preparation),
                                _ => None,
                            },
                            _ => None,
                        })
                        .collect();
                    for preparation in preparations {
                        engine.http_prepared(
                            1000.,
                            preparation,
                            safe_cex_engine::request_context::Prepared {
                                id: 1,
                                expires_at: None,
                            },
                        );
                    }
                    actions.extend(engine.drain_actions());
                    if hung {
                        let timers: Vec<_> = actions
                            .iter()
                            .filter_map(|a| match a {
                                Action::SetTimer { timer, .. } => Some(*timer),
                                _ => None,
                            })
                            .collect();
                        assert!(!timers.is_empty(), "{variant:?}");
                        for timer in timers {
                            engine.handle(100_000.0, Event::TimerFired { timer });
                        }
                        actions.extend(engine.drain_actions());
                    }
                    assert!(
                        !actions.iter().any(|a| matches!(
                            a,
                            Action::Http(_)
                                | Action::Notice(Notice::CommandDone { command: 94, .. })
                        )),
                        "{variant:?}: {actions:?}"
                    );
                    let failures: Vec<_> = actions
                        .iter()
                        .filter_map(|a| match a {
                            Action::Notice(Notice::CommandFailed { command: 94, error }) => {
                                Some(error)
                            }
                            _ => None,
                        })
                        .collect();
                    assert_eq!(failures.len(), 1, "{variant:?} hung={hung}: {actions:?}");
                    assert_eq!(
                        failures[0].outcome,
                        Some(Outcome::NotSent),
                        "{label}: {actions:?}"
                    );
                    assert_eq!(
                        failures[0].kind,
                        Some(if hung {
                            ErrorKind::Timeout
                        } else {
                            ErrorKind::InvalidInput
                        })
                    );
                }
            }
        }
    }

    struct FirstSignatureOnly {
        inner: safe_cex_engine::signing::Provider,
        calls: std::sync::atomic::AtomicUsize,
    }
    impl safe_cex_engine::signing::Signer for FirstSignatureOnly {
        fn sign(
            &self,
            _: safe_cex_engine::signing::SignRequest,
        ) -> safe_cex_engine::signing::SignFuture {
            unreachable!("immediate provider")
        }
        fn immediate(&self) -> bool {
            true
        }
        fn sign_ready(
            &self,
            request: &safe_cex_engine::signing::SignRequest,
        ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
        {
            if self
                .calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                == 0
            {
                self.inner.sign_ready(request)
            } else {
                Some(Err(safe_cex_engine::signing::SignError::Refused))
            }
        }
    }

    #[test]
    fn later_signer_refusal_cannot_label_an_earlier_dispatched_leg_not_sent() {
        for variant in [
            Variant::UsdM,
            Variant::PortfolioMargin,
            Variant::CoinM,
            Variant::Spot,
        ] {
            for accepted in [false, true] {
                let mut engine = engine(variant, usize::MAX);
                let signer = FirstSignatureOnly {
                    inner: engine.signer().unwrap(),
                    calls: std::sync::atomic::AtomicUsize::new(0),
                };
                engine
                    .set_signer(Some(std::sync::Arc::new(signer)))
                    .unwrap();
                engine.handle(
                    1000.0,
                    Event::Command(Command::PlaceOrders {
                        command: 93,
                        orders: vec![limit(); 6],
                    }),
                );
                let first: Vec<_> =
                    safe_cex_engine::test_support::prepared_actions(&mut engine, 1000.)
                        .into_iter()
                        .filter_map(|a| match a {
                            Action::Http(r) => Some(r),
                            _ => None,
                        })
                        .collect();
                assert_eq!(first.len(), 1, "{variant:?}");
                let request = &first[0];
                let body = if request.url.contains("batchOrders") {
                    r#"[{"orderId":1},{"orderId":2},{"orderId":3},{"orderId":4},{"orderId":5}]"#
                } else {
                    r#"{"orderId":1}"#
                };
                if accepted {
                    engine.handle(
                        1001.0,
                        Event::HttpResponse {
                            request: request.id,
                            response: safe_cex_engine::Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                } else {
                    engine.handle(
                        1001.0,
                        Event::HttpFailed {
                            request: request.id,
                            error: "connection closed",
                        },
                    );
                }
                let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 1001.);
                assert!(
                    !actions.iter().any(|a| matches!(a, Action::Http(_))),
                    "{variant:?}"
                );
                let failures: Vec<_> = actions
                    .iter()
                    .filter_map(|a| match a {
                        Action::Notice(Notice::CommandFailed { command: 93, error }) => Some(error),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    failures.len(),
                    1,
                    "{variant:?} accepted={accepted}: {actions:?}"
                );
                assert_eq!(
                    failures[0].outcome,
                    if accepted {
                        None
                    } else {
                        Some(safe_cex_engine::Outcome::Unknown)
                    },
                    "{variant:?} accepted={accepted}"
                );
                if accepted {
                    assert!(!failures[0].partial.as_ref().unwrap().order_ids.is_empty());
                }
            }
        }
    }

    #[test]
    fn unproven_native_job_discard_matches_only_the_sealed_owner_generation() {
        use safe_cex_engine::{Response, Venue};
        let mut engine = engine(Variant::UsdM, usize::MAX);
        engine.handle(
            1000.,
            Event::Command(Command::PlaceOrders {
                command: 98,
                orders: vec![limit(); 6],
            }),
        );
        let old = engine.venue().trading.jobs[&98].context.clone().unwrap();
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 1001.);
        let request = actions
            .iter()
            .find_map(|action| match action {
                Action::Http(http) => Some(http.id),
                _ => None,
            })
            .unwrap();
        engine.handle(
            1002.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 503,
                    headers: "",
                    body: "unproven",
                },
            },
        );
        let dropped = engine.venue().trading.jobs.is_empty();
        // The Unknown fences the symbol; a clean post-mark open-orders snapshot
        // releases it so the second placement exercises a new owner generation.
        for _ in 0..4 {
            let reads: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut engine, 1002.)
                .into_iter()
                .filter_map(|action| match action {
                    Action::Http(http)
                        if http.url.contains("openOrders")
                            || http.url.contains("openAlgoOrders") =>
                    {
                        Some(http.id)
                    }
                    _ => None,
                })
                .collect();
            if reads.is_empty() {
                break;
            }
            for request in reads {
                engine.handle(
                    1002.,
                    Event::HttpResponse {
                        request,
                        response: Response {
                            status: 200,
                            headers: "",
                            body: "[]",
                        },
                    },
                );
            }
        }
        engine.drain_actions().for_each(drop);
        engine.handle(
            1003.,
            Event::Command(Command::PlaceOrders {
                command: 98,
                orders: vec![limit()],
            }),
        );
        let current = engine.venue().trading.jobs[&98].context.clone().unwrap();
        assert_ne!(old.owner(), current.owner());
        let (mut native, _) = crate::test_config::Options::default().build();
        native.trading.jobs.insert(
            98,
            super::Job {
                context: Some(current.clone()),
                ..Default::default()
            },
        );
        native.discard_unproven_http_command(old.owner());
        assert!(
            native.trading.jobs.contains_key(&98),
            "old owner cannot drop current native plan"
        );
        native.discard_unproven_http_command(current.owner());
        let current_dropped = native.trading.jobs.is_empty();
        assert!(
            dropped,
            "central unproven seal must discard queued native work"
        );
        assert!(
            current_dropped,
            "matching generation must discard its native plan"
        );
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

/// A granted signature resend goes out only for a live job that was not
/// already stopping before this refusal.
fn may_resend(stopped_before: bool, job_live: bool) -> bool {
    job_live && !stopped_before
}

#[cfg(test)]
mod resend_guard_tests {
    #[test]
    fn a_stopping_or_ended_job_never_resends() {
        assert!(super::may_resend(false, true));
        assert!(
            !super::may_resend(true, true),
            "stopping before the refusal"
        );
        assert!(
            !super::may_resend(false, false),
            "the command already ended"
        );
    }
}
