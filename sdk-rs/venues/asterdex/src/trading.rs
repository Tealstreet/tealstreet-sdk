//! Aster order entry uses sequential individual requests. Conditional orders
//! share the ordinary order endpoint; amendments cancel before replacing.
use crate::account::venue_error;
use crate::sign::{Query, Version};
use crate::{AsterDex, Options, Pending};
use safe_cex_engine::classify::{
    CodeTable, Intent, Key, Native, NativeCode, Row, Verdict, WriteFact,
};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::model::{
    CancelOrderRef, MarginType, Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts,
    PositionSide, StopTriggerType, StpMode, TimeInForce,
};
use safe_cex_engine::request_context::plan::{SerialCall, SerialPolicy, Slot};
use safe_cex_engine::request_context::{
    Context, Operation, Plan, Policy, Preflight, PreflightPolicy, PreflightPurpose, RetryPolicy,
};
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, VenueError, num,
};
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};

pub struct Request {
    command: CommandId,
    call: Call,
    /// A write's plan and its slot; the STP read's caller root instead.
    plan: Option<(Plan, Slot)>,
    pub(crate) root: Option<Context>,
}

impl Request {
    /// Bounded native correlation for the engine journal: no credential or packet.
    pub(crate) fn reconciliation(
        &self,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        let data = match &self.call {
            Call::Place { symbol, .. } => format!("place:{symbol}"),
            Call::Cancel { id, .. } => format!("cancel:{id}"),
            _ => "setting".to_owned(),
        };
        safe_cex_engine::request_context::ReconciliationData::new(data)
    }

    /// A resend after a resync is the plan's next serial call.
    pub(crate) fn resume(&mut self, plan: Plan, slot: Slot) {
        self.plan = Some((plan, slot));
    }

    pub(crate) fn plan(&self) -> Option<&(Plan, Slot)> {
        self.plan.as_ref()
    }

    pub(crate) fn intent(&self) -> Intent {
        match self.call {
            Call::Place { .. } => Intent::Place,
            Call::Cancel {
                filled_at_amend: Some(_),
                ..
            } => Intent::Amend,
            Call::Cancel { .. } => Intent::Cancel,
            _ => Intent::Setting,
        }
    }
}

/// The native `failure` table: definitive refusals and codes that leave a
/// write unknown.
pub(crate) const ASTER: CodeTable = CodeTable {
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
        Row {
            key: Key::Int(-1022),
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
        Row {
            key: Key::Int(-5025),
            verdict: Verdict::Refused(ErrorKind::AuthPermission),
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
    success: &[Key::Int(200)],
    unlisted: Verdict::Refused(ErrorKind::Rejected),
    status: &[],
};

/// Only while a write still has its single resend: V1 -1022 and -1021.
pub(crate) const ASTER_RESEND: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Int(-1022),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(-1021),
            verdict: Verdict::RefusedResend(ErrorKind::Rejected),
            only: None,
        },
    ],
    success: &[Key::Int(200)],
    unlisted: Verdict::Refused(ErrorKind::Rejected),
    status: &[],
};
enum Call {
    Place {
        symbol: String,
        position_side: Option<PositionSide>,
    },
    Cancel {
        id: String,
        filled_at_amend: Option<f64>,
    },
    Leverage(String, f64),
    Margin(String, bool),
    Mode(bool),
    StpWrite,
    StpRead,
}

impl Call {
    /// The STP setting read is the only read in a trading job.
    fn role(&self) -> CredentialRole {
        match self {
            Self::StpRead => CredentialRole::Read { scope: None },
            _ => CredentialRole::Mutation,
        }
    }
}
struct Planned {
    method: Method,
    endpoint: &'static str,
    query: Query,
    call: Call,
}
#[derive(Default)]
struct Job {
    plan: Option<Plan>,
    /// The next serial call's slot, from admission or the last continuation.
    slot: Option<Slot>,
    reserved_places: usize,
    steps: VecDeque<Planned>,
    ids: Vec<String>,
    successes: usize,
    error: Option<VenueError>,
    continue_on_error: bool,
    result: Option<String>,
}
// Above the venue open-order limit; never evict cancellation routing for a live ACK.
const MAX_ACKNOWLEDGED_ORDERS: usize = 10_000;

#[derive(Default)]
pub struct TradingState {
    jobs: HashMap<CommandId, Job>,
    /// Valid jobs whose entry waits for the clock; admitted when ready.
    parked: Vec<(Preflight, CommandId, Job)>,
    // Placement ACKs can precede both REST reconciliation and the private NEW.
    // Keep routing provenance only until Store/terminal sees the ID or cancellation.
    acknowledged: HashMap<String, String>,
}

/// TS's exact decimal policy: plain decimals, at most 18 fractional digits,
/// nonnegative and capped at 0.001. No float conversion enters signed bytes.
fn fee_rate(input: &str) -> Result<String, String> {
    let input = input.trim();
    let (integer, fraction) = input.split_once('.').unwrap_or((input, ""));
    if integer.is_empty()
        || (integer.len() > 1 && integer.starts_with('0'))
        || !integer.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 18
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || (input.contains('.') && fraction.is_empty())
    {
        return Err("Invalid Aster builder fee rate: expected a decimal string".into());
    }
    if integer != "0" {
        return Err("Aster builder fee rate exceeds 0.001".into());
    }
    let padded = format!("{fraction:0<18}");
    if padded.as_str() > "001000000000000000" {
        return Err("Aster builder fee rate exceeds 0.001".into());
    }
    let fraction = fraction.trim_end_matches('0');
    Ok(if fraction.is_empty() {
        "0".into()
    } else {
        format!("0.{fraction}")
    })
}

struct Builder {
    address: String,
    maker: String,
    taker: String,
}
fn builder(options: &Options) -> Result<Option<Builder>, String> {
    let maker = fee_rate(
        options
            .builder_fee_rate_maker
            .as_deref()
            .unwrap_or("0.0001"),
    )?;
    let taker = fee_rate(
        options
            .builder_fee_rate_taker
            .as_deref()
            .unwrap_or("0.0001"),
    )?;
    if maker == "0" && taker == "0" {
        return Ok(None);
    }
    let default = if options.testnet {
        "REPLACE_WITH_TESTNET_BUILDER_ADDRESS"
    } else {
        "0x79a52a6658c23cF7e7Fe4fE43Ff91c5195Eb86Ca"
    };
    let address = options
        .builder_address
        .as_deref()
        .filter(|v| !v.is_empty())
        .unwrap_or(default)
        .trim();
    if address.is_empty() || address.starts_with("REPLACE_WITH_") {
        return Ok(None);
    }
    let address = safe_cex_crypto::ethereum::get_address(address)
        .map_err(|_| "Invalid Aster builder address")?;
    if address == "0x0000000000000000000000000000000000000000" {
        return Err("Invalid Aster builder address: zero address".into());
    }
    Ok(Some(Builder {
        address,
        maker,
        taker,
    }))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Id {
    Number(u64),
    Text(String),
}
impl Id {
    fn text(self) -> String {
        match self {
            Self::Number(n) => n.to_string(),
            Self::Text(s) => s,
        }
    }
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PlaceAck {
    order_id: Option<Id>,
    data: Option<PlaceData>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceData {
    order_id: Option<Id>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelAck {
    executed_qty: Option<String>,
    status: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StpResponse {
    stp_mode: StpMode,
}

impl AsterDex {
    pub(crate) fn on_trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let prune_on_refusal = matches!(&command, Command::CancelSymbolOrders { .. });
        let (id, job) = match command {
            Command::PlaceOrders { command, .. } | Command::AmendOrder { command, .. }
                if self.position_mode_pending() =>
            {
                (
                    command,
                    Err("Aster position mode has not loaded yet; order placement is held until it does".into()),
                )
            }
            Command::PlaceOrders { command, orders } => (command, self.plan_place(cx, &orders)),
            Command::CancelOrders { command, ids } => {
                let mut error = None;
                let orders = ids
                    .iter()
                    .filter_map(|id| {
                        let symbol = cx
                            .store
                            .order(id)
                            .map(|order| order.symbol.clone())
                            .or_else(|| self.trading.acknowledged.get(id).cloned());
                        if let Some(symbol) = symbol {
                            Some(CancelOrderRef {
                                id: id.clone(),
                                symbol,
                            })
                        } else {
                            error = Some(format!(
                                "Order {id} not found; cancellation requires its symbol"
                            ));
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                (
                    command,
                    self.plan_cancel_refs(&orders).map(|mut job| {
                        job.error = error.map(not_sent);
                        job
                    }),
                )
            }
            Command::CancelOrdersWithSymbols { command, orders } => {
                (command, self.plan_cancel_refs(&orders))
            }
            Command::CancelSymbolOrders { command, symbol } => {
                let mut orders = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == symbol && o.status == OrderStatus::Open)
                    .map(|o| CancelOrderRef {
                        id: o.id.clone(),
                        symbol: o.symbol.clone(),
                    })
                    .collect::<Vec<_>>();
                orders.extend(
                    self.trading
                        .acknowledged
                        .iter()
                        .filter(|(id, s)| {
                            **s == symbol
                                && cx.store.order(id).is_none()
                                && !self.account.terminal.contains_key(*id)
                        })
                        .map(|(id, symbol)| CancelOrderRef {
                            id: id.clone(),
                            symbol: symbol.clone(),
                        }),
                );
                (command, self.plan_cancel_refs(&orders))
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (command, self.plan_amend(cx, &id, price, amount)),
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => {
                let plan = cx
                    .store
                    .market(&symbol)
                    .ok_or_else(|| format!("Market {symbol} not found"))
                    .and_then(|market| {
                        if !leverage.is_finite() {
                            return Err("Invalid Aster leverage".into());
                        }
                        let leverage = leverage.round().clamp(
                            market.limits.leverage.min.max(1.0),
                            market.limits.leverage.max.max(1.0),
                        );
                        Ok(single(
                            Method::Post,
                            "leverage",
                            Query::new()
                                .str("symbol", &symbol)
                                .str("leverage", &num::format(leverage)),
                            Call::Leverage(symbol, leverage),
                        ))
                    });
                (command, plan)
            }
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => {
                let query = Query::new()
                    .str("symbol", &symbol)
                    .str("marginType", if isolated { "ISOLATED" } else { "CROSSED" });
                (
                    command,
                    Ok(single(
                        Method::Post,
                        "marginType",
                        query,
                        Call::Margin(symbol, isolated),
                    )),
                )
            }
            Command::SetPositionMode {
                command,
                hedged,
                symbol: _,
            } => (
                command,
                Ok(single(
                    Method::Post,
                    "positionSide/dual",
                    Query::new().str("dualSidePosition", if hedged { "true" } else { "false" }),
                    Call::Mode(hedged),
                )),
            ),
            Command::FetchStpMode { command } => {
                self.fetch_stp_mode(cx, command);
                return;
            }
            Command::SetStpMode { command, mode } => {
                let plan = if self.version != Version::V3 {
                    Err("Aster self-trade prevention requires V3".into())
                } else {
                    Ok(single(
                        Method::Post,
                        "stpMode",
                        Query::new().str("stpMode", mode.as_str()),
                        Call::StpWrite,
                    ))
                };
                (command, plan)
            }
            _ => return,
        };
        self.start_trade(cx, id, Operation::Mutation, prune_on_refusal, job);
    }

    /// A read: its native local refusals first, then a read under the root.
    pub(crate) fn fetch_stp_mode(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let plan = if self.version != Version::V3 {
            Err("Aster self-trade prevention requires V3".into())
        } else {
            Ok(single(Method::Get, "stpMode", Query::new(), Call::StpRead))
        };
        self.start_trade(cx, command, Operation::Read, false, plan);
    }

    fn read_stp_mode(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(root) = crate::command_read_root(cx) else {
            return;
        };
        let path = self.path("stpMode");
        let pending = Pending::Trade(Request {
            command,
            call: Call::StpRead,
            plan: None,
            root: Some(root),
        });
        self.signed(
            cx,
            Call::StpRead.role(),
            Method::Get,
            &path,
            Query::new(),
            pending,
        );
    }
    fn start_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        operation: Operation,
        prune_on_refusal: bool,
        job: Result<Job, String>,
    ) {
        let job = job
            .map_err(|error| {
                (VenueError::new(error).kind(ErrorKind::InvalidInput), prune_on_refusal)
            })
            .and_then(|mut job| {
                // Validate the shared auth shape before reserving any split step.
                if let Some(step) = job.steps.front() {
                    let valid = self
                        .credentials
                        .select(step.call.role())
                        .map_err(|_| "Missing required Aster credentials".to_owned())
                        .and_then(|(_, slot)| self.auth(slot).map(drop));
                    if let Err(error) = valid {
                        return Err((VenueError::new(error).kind(ErrorKind::AuthInvalid), prune_on_refusal));
                    }
                }
                job.reserved_places = job
                    .steps
                    .iter()
                    .filter(|step| matches!(step.call, Call::Place { .. }))
                    .count();
                if job.reserved_places > 0 {
                    let reserved = self
                        .trading
                        .jobs
                        .values()
                        .map(|job| job.reserved_places)
                        .sum::<usize>();
                    let acknowledged = self.trading.acknowledged.keys().filter(|id| {
                        cx.store.order(id).is_none() && !self.account.terminal.contains_key(*id)
                    }).count();
                    if acknowledged + reserved + job.reserved_places > MAX_ACKNOWLEDGED_ORDERS
                    {
                        return Err((VenueError::new("Aster placement routing capacity reached; wait for order reconciliation or cancel existing acknowledged orders"), true));
                    }
                }
                Ok(job)
            });
        // A local refusal owns its command Context before any diagnostic.
        let job = match job {
            Ok(job) => job,
            Err((mut error, prune)) => {
                let Ok(context) = cx.admit_http_command(Policy {
                    operation,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return;
                };
                if prune {
                    self.prune_trade_acknowledged(cx);
                }
                if operation == Operation::Mutation {
                    error.outcome = Some(Outcome::NotSent);
                }
                let _ = cx.command_error_context(&context, error.message.clone());
                let _ = cx.fail_context(&context, error);
                return;
            }
        };
        if operation == Operation::Read {
            self.read_stp_mode(cx, id);
            return;
        }
        // A valid job's plan needs the write credential the job validated.
        let credential = self
            .credentials
            .select(CredentialRole::Mutation)
            .ok()
            .map(|(selection, slot)| selection.credential(slot.identity.credential.clone()));
        let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: None,
            write_credential: credential,
        }) else {
            return;
        };
        // Nothing signs before a fresh clock: the entry parks until it settles.
        if !self.clock_fresh(cx.now) {
            if let Err(error) = cx.park_preflight(&preflight) {
                let _ = cx.fail_preflight(&preflight, error.venue_error());
                return;
            }
            self.trading.parked.push((preflight, id, job));
            self.await_clock(cx);
            return;
        }
        self.admit_trade(cx, id, &preflight, job);
    }

    pub(crate) fn wake_parked_trades(&mut self, cx: &mut Cx<'_, Pending>) {
        for (preflight, _, _) in &self.trading.parked {
            let _ = cx.request_preflight_ready(preflight);
        }
    }

    pub(crate) fn on_trade_preflight_ready(&mut self, cx: &mut Cx<'_, Pending>, ready: &Preflight) {
        let Some(index) = self
            .trading
            .parked
            .iter()
            .position(|(preflight, _, _)| preflight.owner() == ready.owner())
        else {
            return;
        };
        let (preflight, id, job) = self.trading.parked.remove(index);
        self.admit_trade(cx, id, &preflight, job);
    }

    fn admit_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        preflight: &Preflight,
        mut job: Job,
    ) {
        let credential = self
            .credentials
            .select(CredentialRole::Mutation)
            .ok()
            .map(|(selection, slot)| selection.credential(slot.identity.credential.clone()));
        // A job with no native step sends nothing: it ends on its preflight.
        if job.steps.is_empty() {
            let _ = match job.error {
                Some(error) => {
                    cx.command_error(id, error.message.clone());
                    cx.fail_preflight(preflight, error)
                }
                None => cx.done_empty_plan(
                    preflight,
                    job.result
                        .unwrap_or_else(|| serde_json::to_string(&job.ids).unwrap_or_default()),
                ),
            };
            return;
        }
        let Some(credential) = credential else {
            let error = not_sent("Missing required Aster credentials").kind(ErrorKind::AuthInvalid);
            let _ = cx.fail_preflight(preflight, error);
            return;
        };
        let policy = SerialPolicy {
            credential: Some(credential),
            pacing: None,
            refusals_complete: true,
        };
        match cx.admit_serial_plan(preflight, serial(), policy) {
            Ok((plan, slot)) => {
                job.plan = Some(plan);
                job.slot = Some(slot);
            }
            Err(error) => {
                let _ = cx.fail_preflight(preflight, error.venue_error());
                return;
            }
        }
        self.prune_trade_acknowledged(cx);
        if let Some(error) = &job.error {
            cx.command_error(id, error.message.clone());
        }
        self.trading.jobs.insert(id, job);
        self.advance_trade(cx, id);
    }

    fn prune_trade_acknowledged(&mut self, cx: &Cx<'_, Pending>) {
        self.trading.acknowledged.retain(|id, _| {
            cx.store.order(id).is_none() && !self.account.terminal.contains_key(id)
        });
    }
    fn advance_trade(&mut self, cx: &mut Cx<'_, Pending>, id: CommandId) {
        let Some(job) = self.trading.jobs.get_mut(&id) else {
            return;
        };
        let next = if job.error.is_some() && !job.continue_on_error {
            None
        } else {
            job.steps.pop_front()
        };
        let plan = job.plan.clone().expect("an admitted job holds its plan");
        if let Some(step) = next {
            let Some(slot) = job.slot.take() else {
                return;
            };
            let path = self.path(step.endpoint);
            // The request layer consumes the pending request and reports any
            // signing failure through on_trade_response, including synchronous
            // failures. Do not complete or advance the same request twice.
            self.signed(
                cx,
                step.call.role(),
                step.method,
                &path,
                step.query,
                Pending::Trade(Request {
                    command: id,
                    call: step.call,
                    plan: Some((plan, slot)),
                    root: None,
                }),
            );
            return;
        }
        let job = self.trading.jobs.remove(&id).expect("present");
        let _ = match job.error {
            Some(mut error) if !job.continue_on_error || job.successes == 0 => {
                // A later rejection after a step took effect cannot mean nothing happened.
                if job.successes > 0 && error.outcome == Some(Outcome::Rejected) {
                    error.outcome = None;
                }
                cx.fail_plan(&plan, error)
            }
            _ => cx.done_plan(
                &plan,
                job.result
                    .unwrap_or_else(|| serde_json::to_string(&job.ids).unwrap_or_default()),
            ),
        };
    }
    /// `leg` is the plan write's live Context: its fact is reported here,
    /// once, unless the request layer already reported it (`reported`).
    pub(crate) fn on_trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
        leg: Option<&Context>,
        reported: bool,
    ) {
        let command = request.command;
        let root = request.root.clone();
        let plan = request.plan.clone();
        let call = &request.call;
        let was_place = matches!(call, Call::Place { .. });
        let cancelled = match &request.call {
            Call::Cancel { id, .. } => vec![id.clone()],
            _ => vec![],
        };
        let transport = response.is_err();
        // A gateway failure is unproven whatever body it carries.
        let unproven = response
            .as_ref()
            .is_ok_and(|r| r.status == 408 || r.status >= 500);
        let outcome = match response {
            Ok(ref response) => venue_error(response).map_or(Ok(response.body), Err),
            Err(error) => Err((0, error.to_owned())),
        };
        let outcome = match outcome {
            Err((-2011, ref message))
                if matches!(
                    call,
                    Call::Cancel {
                        filled_at_amend: None,
                        ..
                    }
                ) && message.to_ascii_lowercase().contains("unknown order") =>
            {
                Ok("")
            }
            outcome => outcome,
        };
        let replied = outcome.is_ok();
        let mut placed = None;
        let mut result = None;
        let outcome = match (request.call, outcome) {
            (
                Call::Place {
                    symbol,
                    position_side,
                },
                Ok(body),
            ) => {
                let ack = serde_json::from_str::<PlaceAck>(body).map_err(|e| (0, e.to_string()));
                ack.and_then(|ack| {
                    ack.order_id
                        .or_else(|| ack.data.and_then(|d| d.order_id))
                        .map(Id::text)
                        .filter(|id| !id.is_empty())
                        .ok_or_else(|| (0, "Aster order response did not include orderId".into()))
                })
                .map(|id| {
                    self.trading.acknowledged.retain(|id, _| {
                        cx.store.order(id).is_none() && !self.account.terminal.contains_key(id)
                    });
                    if cx.store.order(&id).is_none() && !self.account.terminal.contains_key(&id) {
                        self.trading.acknowledged.insert(id.clone(), symbol);
                    }
                    if let Some(side) = position_side {
                        self.record_order_position_side(&id, side);
                    }
                    placed = Some(id);
                    self.account.order_revision = self.account.order_revision.wrapping_add(1);
                    self.request_refresh_orders(cx);
                })
            }
            (
                Call::Cancel {
                    id,
                    filled_at_amend,
                },
                Ok(body),
            ) => {
                // The cancel response can report a fill before its WebSocket
                // event arrives. Its cumulative quantity is authoritative too.
                if let Ok(ack) = serde_json::from_str::<CancelAck>(body) {
                    let filled = ack
                        .executed_qty
                        .as_deref()
                        .and_then(|s| s.parse::<f64>().ok())
                        .filter(|n| n.is_finite() && *n >= 0.0);
                    if let Some(mut order) = cx.store.order(&id).cloned().or_else(|| {
                        self.account
                            .terminal
                            .get(&id)
                            .map(|(_, order)| order.clone())
                    }) {
                        let closed = ack.status.as_deref() == Some("FILLED");
                        if closed || filled.is_some_and(|n| n > order.filled) {
                            order.filled = filled.unwrap_or(order.filled).max(order.filled);
                            order.remaining = (order.amount - order.filled).max(0.0);
                            order.last_update = cx.now;
                            if closed {
                                order.status = OrderStatus::Closed;
                                order.remaining = 0.0;
                            }
                            self.account.order_revision =
                                self.account.order_revision.wrapping_add(1);
                            if closed {
                                self.account
                                    .terminal
                                    .insert(id.clone(), (cx.now, order.clone()));
                            }
                            cx.store.upsert_order(order);
                        }
                    }
                }
                let filled_during_cancel = filled_at_amend.is_some_and(|before| {
                    cx.store
                        .order(&id)
                        .into_iter()
                        .chain(self.account.terminal.get(&id).map(|(_, order)| order))
                        .any(|order| order.status == OrderStatus::Closed || order.filled > before)
                });
                self.trading.acknowledged.remove(&id);
                self.mark_order_canceled(cx, &id);
                if filled_during_cancel {
                    Err((0,"Aster order filled while cancellation was in flight; replacement was not submitted".into()))
                } else {
                    Ok(())
                }
            }
            (Call::Leverage(symbol, leverage), Ok(_)) => {
                self.invalidate_position_settings();
                cx.store.update_setting(&symbol, |s| s.leverage = leverage);
                cx.store.update_positions(|p| {
                    if p.symbol == symbol {
                        p.leverage = leverage;
                    }
                });
                Ok(())
            }
            (Call::Margin(symbol, isolated), Ok(_)) => {
                self.invalidate_position_settings();
                cx.store.update_setting(&symbol, |s| {
                    s.margin_type = Some(if isolated {
                        MarginType::Isolated
                    } else {
                        MarginType::Cross
                    })
                });
                Ok(())
            }
            (Call::Mode(hedged), Ok(_)) => {
                self.on_mode_changed(cx, hedged);
                Ok(())
            }
            (Call::StpRead, Ok(body)) => serde_json::from_str::<StpResponse>(body)
                .map_err(|e| (0, e.to_string()))
                .map(|row| {
                    result = serde_json::to_string(&row.stp_mode).ok();
                }),
            (Call::StpWrite, Ok(_)) => Ok(()),
            (_, Err(error)) => Err(error),
        };
        if let Some(root) = root {
            let _ = match outcome {
                Ok(()) => cx.done_context(&root, result.unwrap_or_default()),
                Err((code, message)) => {
                    // A read carries no money outcome.
                    let mut error = failure(code, message);
                    error.outcome = None;
                    cx.fail_context(&root, error)
                }
            };
            return;
        }
        if let Some(leg) = leg.filter(|_| !reported) {
            let _ = match &outcome {
                _ if replied && !(was_place && placed.is_none()) => cx.http_write_fact(
                    leg,
                    WriteFact::Accepted {
                        order_ids: placed.iter().cloned().chain(cancelled).collect(),
                    },
                    None,
                ),
                Err((code, message)) if !transport && !unproven && !replied && *code != 0 => {
                    let native = Native {
                        code: NativeCode::Int(*code),
                        message,
                    };
                    match cx.classify_write(leg, &ASTER, Some(&native), message.clone()) {
                        Ok(fact) => cx.http_write_fact(
                            leg,
                            WriteFact::Failed {
                                fact,
                                known_effects: false,
                            },
                            None,
                        ),
                        Err(error) => Err(error),
                    }
                }
                Err((_, message)) => cx.http_write_unproven(leg, message.clone()).map(|_| None),
                Ok(()) => cx
                    .http_write_unproven(leg, "Aster write reply was not proven")
                    .map(|_| None),
            };
        }
        // The fact is recorded: continue the serial plan into the next native
        // step, or settle this last call before the terminal.
        if let (Some(leg), Some((plan, slot))) = (leg, &plan) {
            let continues = self.trading.jobs.get(&command).is_some_and(|job| {
                !job.steps.is_empty() && (job.continue_on_error || outcome.is_ok())
            });
            if continues {
                match cx.continue_serial_plan(plan, slot, leg, serial()) {
                    Ok(next) => {
                        if let Some(job) = self.trading.jobs.get_mut(&command) {
                            job.slot = Some(next);
                        }
                    }
                    Err(error) => {
                        if let Some(job) = self.trading.jobs.get_mut(&command) {
                            job.steps.clear();
                            job.error
                                .get_or_insert(VenueError::new(error.venue_error().message));
                        }
                    }
                }
            } else {
                let _ = cx.finish_plan_call(plan, slot, leg);
            }
        }
        if was_place && outcome.is_err() {
            self.account.order_revision = self.account.order_revision.wrapping_add(1);
        }
        if let Some(job) = self.trading.jobs.get_mut(&command) {
            if was_place {
                job.reserved_places = job.reserved_places.saturating_sub(1);
            }
            match outcome {
                Ok(()) => job.successes += 1,
                Err((code, message)) => {
                    cx.command_error(command, message.clone());
                    let mut error = failure(code, message);
                    // An earlier leg that may have executed keeps the command unknown.
                    if unproven
                        || job
                            .error
                            .as_ref()
                            .is_some_and(|e| e.outcome == Some(Outcome::Unknown))
                    {
                        error.outcome = Some(Outcome::Unknown);
                    }
                    job.error = Some(error);
                }
            }
            if let Some(id) = placed {
                job.ids.push(id);
            }
            if let Some(result) = result {
                job.result = Some(result);
            }
        }
        self.advance_trade(cx, command);
    }

    fn plan_cancel_refs(&self, orders: &[CancelOrderRef]) -> Result<Job, String> {
        if orders
            .iter()
            .any(|order| order.id.is_empty() || order.symbol.is_empty())
        {
            return Err("Aster cancellation requires a nonempty order ID and symbol".into());
        }
        Ok(Job {
            steps: orders
                .iter()
                .map(|order| Planned {
                    method: Method::Delete,
                    endpoint: "order",
                    query: Query::new()
                        .str("symbol", &order.symbol)
                        .str("orderId", &order.id),
                    call: Call::Cancel {
                        id: order.id.clone(),
                        filled_at_amend: None,
                    },
                })
                .collect(),
            continue_on_error: true,
            ..Job::default()
        })
    }
    fn plan_cancel(&self, orders: &[Order], continue_on_error: bool) -> Job {
        Job {
            steps: orders
                .iter()
                .map(|order| Planned {
                    method: Method::Delete,
                    endpoint: "order",
                    query: Query::new()
                        .str("symbol", &order.symbol)
                        .str("orderId", &order.id),
                    call: Call::Cancel {
                        id: order.id.clone(),
                        filled_at_amend: (!continue_on_error).then_some(order.filled),
                    },
                })
                .collect(),
            continue_on_error,
            ..Job::default()
        }
    }
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
        if order.status != OrderStatus::Open {
            return Err(format!("Aster order {id} is no longer open"));
        }
        let conditional = matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit);
        // Hedge rows report reduceOnly=false, so re-deriving the leg could open the opposite side.
        let position_side = if cx.store.hedged() {
            Some(
                self.order_position_side(id)
                    .ok_or_else(|| format!("Aster order {id} has no known hedge position side"))?,
            )
        } else {
            None
        };
        let replacement = PlaceOrderOpts {
            symbol: order.symbol.clone(),
            kind: order.kind,
            side: order.side,
            amount: amount.unwrap_or(order.amount),
            price: (!conditional).then_some(price.unwrap_or(order.price)),
            trigger_price: conditional.then_some(price.unwrap_or(order.price)),
            reduce_only: Some(order.reduce_only),
            time_in_force: order.time_in_force,
            trigger_type: order.trigger_type,
            ..empty_order()
        };
        // Format and validate before canceling the live order.
        let place = self.plan_place_inner(cx, &[replacement], Some(id), position_side)?;
        if let Some(error) = place.error {
            return Err(error.message);
        }
        let mut job = self.plan_cancel(std::slice::from_ref(order), false);
        job.steps.extend(place.steps);
        Ok(job)
    }
    fn plan_place(&self, cx: &Cx<'_, Pending>, orders: &[PlaceOrderOpts]) -> Result<Job, String> {
        self.plan_place_inner(cx, orders, None, None)
    }

    fn plan_place_inner(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[PlaceOrderOpts],
        replacing: Option<&str>,
        position_side: Option<PositionSide>,
    ) -> Result<Job, String> {
        // This venue does not honor caller IDs. Check the entire batch before I/O.
        if orders.iter().any(|o| o.client_order_id.is_some()) {
            return Err("AsterDEX does not support caller client order IDs".into());
        }
        if orders
            .iter()
            .any(|o| o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some())
        {
            return Err("Bybit BBO and TickSize controls are not supported by AsterDEX".into());
        }
        let builder = if self.version == Version::V3 {
            builder(&self.options)?
        } else {
            None
        };
        let mut job = Job {
            continue_on_error: true,
            ..Job::default()
        };
        for opts in orders {
            if opts.kind == OrderType::TrailingStopLoss {
                job.error = Some(not_sent("AsterDEX trailing stop-loss orders are unsupported because callbackRate is unavailable").kind(ErrorKind::Unsupported));
                continue;
            }
            if opts.stp_mode.is_some() && self.version != Version::V3 {
                job.error = Some(
                    not_sent("Aster per-order self-trade prevention requires V3")
                        .kind(ErrorKind::Unsupported),
                );
                continue;
            }
            let mut children = vec![opts.clone()];
            children[0].stop_loss = None;
            children[0].take_profit = None;
            if matches!(opts.kind, OrderType::Limit | OrderType::Market) {
                for (kind, trigger) in [
                    (OrderType::StopLoss, opts.stop_loss),
                    (OrderType::TakeProfit, opts.take_profit),
                ] {
                    if let Some(trigger) = trigger.filter(|v| *v != 0.0) {
                        children.push(PlaceOrderOpts {
                            symbol: opts.symbol.clone(),
                            kind,
                            side: if opts.side == OrderSide::Buy {
                                OrderSide::Sell
                            } else {
                                OrderSide::Buy
                            },
                            amount: opts.amount,
                            trigger_price: Some(trigger),
                            reduce_only: Some(true),
                            tp_trigger_type: opts.tp_trigger_type,
                            sl_trigger_type: opts.sl_trigger_type,
                            ..empty_order()
                        });
                    }
                }
            }
            let count = children
                .iter()
                .filter(|o| matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit))
                .count();
            let max = self
                .market_rules
                .get(&opts.symbol)
                .and_then(|r| r.max_algo_orders)
                .unwrap_or(0) as usize;
            let existing = cx
                .store
                .orders()
                .iter()
                .filter(|o| {
                    o.symbol == opts.symbol
                        && Some(o.id.as_str()) != replacing
                        && o.status == OrderStatus::Open
                        && matches!(
                            o.kind,
                            OrderType::StopLoss
                                | OrderType::TakeProfit
                                | OrderType::TrailingStopLoss
                        )
                })
                .count();
            if max > 0 && existing + count > max {
                job.error = Some(not_sent(format!(
                    "AsterDEX max algo order limit reached for {}: {existing}/{max} open, {count} new requested",
                    opts.symbol
                )));
                continue;
            }
            for child in children {
                let position_side = hedge_position_side(cx, &child, position_side);
                match self.format_order(cx, &child, builder.as_ref(), position_side) {
                    Ok(queries) => {
                        for query in queries {
                            job.steps.push_back(Planned {
                                method: Method::Post,
                                endpoint: "order",
                                query,
                                call: Call::Place {
                                    symbol: child.symbol.clone(),
                                    position_side,
                                },
                            });
                        }
                    }
                    Err(error) => job.error = Some(not_sent(error).kind(ErrorKind::InvalidInput)),
                }
            }
        }
        Ok(job)
    }
    fn format_order(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
        builder: Option<&Builder>,
        position_side: Option<PositionSide>,
    ) -> Result<Vec<Query>, String> {
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let conditional = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
        if !opts.amount.is_finite() || opts.amount < 0.0 || (!conditional && opts.amount == 0.0) {
            return Err("Invalid Aster order amount".into());
        }
        let kind = match opts.kind {
            OrderType::Limit => "LIMIT",
            OrderType::Market => "MARKET",
            OrderType::StopLoss => "STOP_MARKET",
            OrderType::TakeProfit => "TAKE_PROFIT_MARKET",
            _ => return Err("Unsupported Aster order type".into()),
        };
        let mut base = Query::new()
            .str("symbol", &opts.symbol)
            .str(
                "side",
                if opts.side == OrderSide::Buy {
                    "BUY"
                } else {
                    "SELL"
                },
            )
            .str("type", kind);
        if let Some(mode) = opts.stp_mode {
            base = base.str("stpMode", mode.as_str());
        }
        if let Some(builder) = builder {
            let maker =
                opts.kind == OrderType::Limit && opts.time_in_force == Some(TimeInForce::PostOnly);
            let fee = if maker {
                &builder.maker
            } else {
                &builder.taker
            };
            if fee != "0" {
                base = base.str("builder", &builder.address).str("feeRate", fee);
            }
        }
        if let Some(side) = position_side {
            let side = if side == PositionSide::Long {
                "LONG"
            } else {
                "SHORT"
            };
            base = base.str("positionSide", side);
        }
        let amount = num::adjust(opts.amount, market.precision.amount);
        if opts.amount > 0.0 && amount <= 0.0 {
            return Err("Aster order amount rounds to zero".into());
        }
        let close = conditional && opts.amount == 0.0;
        let max = num::adjust(market.limits.amount.max, market.precision.amount);
        let mut quantities = Vec::new();
        let mut remaining = amount;
        if !conditional && max > 0.0 && remaining > max {
            // Bound planning rather than allowing an unbounded allocation.
            if remaining / max > 10000.0 {
                return Err("Aster order requires too many split requests".into());
            }
            while remaining > max {
                quantities.push(max);
                remaining = num::adjust(remaining - max, market.precision.amount);
            }
        }
        quantities.push(remaining);
        let tick = self
            .market_rules
            .get(&opts.symbol)
            .map_or(market.precision.price, |rule| rule.tick_size);
        quantities
            .into_iter()
            .map(|quantity| {
                let mut query = base.clone();
                if close {
                    query = query.str("closePosition", "true");
                } else {
                    query = query.str("quantity", &num::format(quantity));
                    if opts.reduce_only == Some(true) && !cx.store.hedged() {
                        query = query.str("reduceOnly", "true");
                    }
                }
                if opts.kind == OrderType::Limit {
                    let price = opts
                        .price
                        .filter(|p| p.is_finite() && *p > 0.0)
                        .ok_or("Aster limit order needs a positive price")?;
                    let price = num::adjust(price, tick);
                    if price <= 0.0 {
                        return Err("Aster limit price rounds to zero".into());
                    }
                    query = query
                        .str(
                            "timeInForce",
                            tif(opts.time_in_force.unwrap_or(TimeInForce::GoodTillCancel)),
                        )
                        .str("price", &num::format(price));
                }
                if conditional {
                    let trigger = opts
                        .trigger_price
                        .or(opts.price)
                        .filter(|p| p.is_finite() && *p > 0.0)
                        .ok_or("Aster conditional order needs a positive trigger price")?;
                    let trigger = num::adjust(trigger, tick);
                    if trigger <= 0.0 {
                        return Err("Aster trigger price rounds to zero".into());
                    }
                    query = query
                        .str("stopPrice", &num::format(trigger))
                        .str("workingType", working_type(opts));
                }
                Ok(query)
            })
            .collect()
    }
}
/// The hedge leg an order targets; an amend passes the replaced order's leg.
fn hedge_position_side(
    cx: &Cx<'_, Pending>,
    opts: &PlaceOrderOpts,
    replaced: Option<PositionSide>,
) -> Option<PositionSide> {
    if !cx.store.hedged() {
        return None;
    }
    let closes = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit)
        || opts.reduce_only == Some(true);
    Some(replaced.unwrap_or(match (opts.side, closes) {
        (OrderSide::Buy, false) | (OrderSide::Sell, true) => PositionSide::Long,
        _ => PositionSide::Short,
    }))
}
fn empty_order() -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: String::new(),
        kind: OrderType::Market,
        side: OrderSide::Buy,
        amount: 0.0,
        price: None,
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
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    }
}
/// One native step: a Mutation with its one classified V1 -1022 resend.
pub(crate) fn serial() -> SerialCall {
    SerialCall {
        auth: Default::default(),
        operation: Operation::Mutation,
        timeout_ms: 20_000,
        attempts: 2,
        pacing: None,
    }
}

fn single(method: Method, endpoint: &'static str, query: Query, call: Call) -> Job {
    Job {
        steps: VecDeque::from([Planned {
            method,
            endpoint,
            query,
            call,
        }]),
        ..Job::default()
    }
}
fn tif(tif: TimeInForce) -> &'static str {
    match tif {
        TimeInForce::GoodTillCancel => "GTC",
        TimeInForce::ImmediateOrCancel => "IOC",
        TimeInForce::FillOrKill => "FOK",
        TimeInForce::PostOnly => "GTX",
    }
}
fn working_type(opts: &PlaceOrderOpts) -> &'static str {
    match opts.kind {
        OrderType::TakeProfit => {
            if opts.tp_trigger_type.or(opts.trigger_type) == Some(StopTriggerType::MarkPrice) {
                "MARK_PRICE"
            } else {
                "CONTRACT_PRICE"
            }
        }
        _ => {
            if opts.sl_trigger_type.or(opts.trigger_type) == Some(StopTriggerType::LastPrice) {
                "CONTRACT_PRICE"
            } else {
                "MARK_PRICE"
            }
        }
    }
}

/// A trading failure classified by its Aster code; code 0 is a transport
/// failure, a non-JSON HTTP error or an unproven acknowledgement.
fn failure(code: i64, message: String) -> VenueError {
    let (kind, outcome) = match code {
        0 | -1000 => (None, Outcome::Unknown),
        -1007 => (Some(ErrorKind::Timeout), Outcome::Unknown),
        -1001 | -1006 => (Some(ErrorKind::VenueUnavailable), Outcome::Unknown),
        -1008 => (Some(ErrorKind::VenueUnavailable), Outcome::Rejected),
        -1003 | -1015 => (Some(ErrorKind::RateLimited), Outcome::Rejected),
        -1022 | -2014 | -2015 => (Some(ErrorKind::AuthInvalid), Outcome::Rejected),
        -1002 | -5025 => (Some(ErrorKind::AuthPermission), Outcome::Rejected),
        -1199..=-1100 | -4999..=-4000 => (Some(ErrorKind::InvalidInput), Outcome::Rejected),
        _ => (Some(ErrorKind::Rejected), Outcome::Rejected),
    };
    let mut error = VenueError::new(message.clone()).outcome(outcome);
    error.kind = kind;
    if code != 0 {
        error = error.venue_code(code.to_string()).venue_message(message);
    }
    error
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

#[cfg(test)]
mod tests {
    use super::fee_rate;
    #[test]
    fn builder_decimals_preserve_exact_signed_strings() {
        for (input, expected) in [
            (" 0.0001000 ", "0.0001"),
            ("0.001000000000000000", "0.001"),
            ("0.000000000000000001", "0.000000000000000001"),
            ("0.000", "0"),
        ] {
            assert_eq!(fee_rate(input).unwrap(), expected);
        }
        for input in [
            "1e-4",
            "-0.0001",
            "00.001",
            ".001",
            "0.",
            "0.001000000000000001",
            "0.0000000000000000001",
            "NaN",
        ] {
            assert!(fee_rate(input).is_err(), "{input}");
        }
    }
}

#[cfg(test)]
#[path = "../tests/support/local_trade_refusals.rs"]
mod local_trade_refusals;
