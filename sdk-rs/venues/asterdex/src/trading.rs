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
use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, WritePlan};
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
    write: Option<usize>,
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
            Call::CancelSymbol(symbol) => format!("cancel-all:{symbol}"),
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
            Call::Cancel { .. } | Call::CancelSymbol(_) => Intent::Cancel,
            _ => Intent::Setting,
        }
    }
}

pub(crate) const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

pub(crate) fn write_failure_text(error: &VenueError) -> String {
    if error.outcome == Some(Outcome::Unknown) {
        // Projection retains uncertainty context without its presentation marker.
        if error.message.contains(ENGINE_UNKNOWN_TEXT) {
            error.message.clone()
        } else if error.presents || error.message.contains("Check Open Orders") {
            format!("{}: {ENGINE_UNKNOWN_TEXT}", error.message)
        } else {
            ENGINE_UNKNOWN_TEXT.to_owned()
        }
    } else {
        error.message.clone()
    }
}

fn failed_element(mut error: VenueError) -> ElementResult {
    if error
        .partial
        .as_ref()
        .is_some_and(|p| !p.order_ids.is_empty())
    {
        error.outcome = Some(Outcome::Unknown);
    }
    match error.outcome {
        Some(Outcome::NotSent | Outcome::Rejected) => ElementResult::NotSent {
            error: Some(Box::new(error)),
        },
        _ => {
            error.outcome = Some(Outcome::Unknown);
            error.message = write_failure_text(&error);
            ElementResult::Unknown {
                error: Box::new(error),
            }
        }
    }
}

const NO_AGENT: &str = "No agent found";
const BAD_SIGNATURE: &str = "Signature check failed";
const DEVICE_TIME: &str = "Your device time must match the actual time";
/// Aster's -1000 refusals: the fixed text of one, whatever follows it.
fn refusal_1000(message: &str) -> Option<&'static str> {
    [NO_AGENT, BAD_SIGNATURE, DEVICE_TIME]
        .into_iter()
        .find(|fixed| message.starts_with(fixed))
}
/// The native `failure` table: definitive refusals and codes that leave a
/// write unknown.
pub(crate) const ASTER: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::IntMessage(-1000, NO_AGENT),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::IntMessage(-1000, BAD_SIGNATURE),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::IntMessage(-1000, DEVICE_TIME),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
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
        // Aster futures v3 "20xx - Processing Issues": the order was rejected.
        Row {
            key: Key::Int(-2010),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(-2016),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Range(-2028, -2018),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(-2031),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: None,
        },
    ],
    success: &[Key::Int(200)],
    unlisted: Verdict::Refused(ErrorKind::Rejected),
    status: ASTER_STATUS,
};

/// A stop's replacement or put-back: an undocumented code may have placed the
/// order, so it is unknown, never a refusal that earns a put-back or a resend.
pub(crate) const ASTER_PROTECTIVE: CodeTable = CodeTable {
    rows: ASTER.rows,
    success: ASTER.success,
    unlisted: Verdict::Unknown(None),
    status: ASTER_STATUS,
};

/// Aster's fapi documents Binance's statuses: 4XX for malformed requests, 418 an
/// IP auto-ban and 429 a rate limit. A bare edge 403 may be another block, so it
/// is a plain refusal. They apply only where no native code was parsed.
const ASTER_STATUS: &[(u16, Verdict)] = &[
    (400, Verdict::Refused(ErrorKind::InvalidInput)),
    (401, Verdict::Refused(ErrorKind::AuthInvalid)),
    (403, Verdict::Refused(ErrorKind::Rejected)),
    (418, Verdict::Refused(ErrorKind::RateLimited)),
    (429, Verdict::Refused(ErrorKind::RateLimited)),
];

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
    status: ASTER_STATUS,
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
    /// Reaches orders the store has not seen; their rows follow from the stream.
    CancelSymbol(String),
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
    guard: Option<StopGuard>,
    restore: Option<Planned>,
    restoring: bool,
    steps: VecDeque<Planned>,
    journal: Option<WritePlan>,
    next_element: usize,
    ids: Vec<String>,
    successes: usize,
    error: Option<VenueError>,
    continue_on_error: bool,
    /// The cancel-all sweep was refused: orders may rest whatever the row cancels did.
    sweep_failed: bool,
    result: Option<String>,
    /// A refused replacement here leaves the position without its stop.
    replacing_stop: Option<(String, OrderSide, OrderType)>,
    /// A refused replacement here leaves nothing resting of this symbol's order.
    replacing_order: Option<String>,
}
impl Job {
    fn failure(&self, error: VenueError) -> VenueError {
        match &self.journal {
            Some(journal) => journal.attach(error),
            None => error,
        }
    }
    /// What a failed replacement cost, once its cancel has landed.
    fn gone_text(&self, unknown: bool) -> Option<String> {
        if let Some((symbol, side, kind)) = &self.replacing_stop {
            return Some(safe_cex_engine::error::stop_gone_text(
                symbol, *side, *kind, unknown,
            ));
        }
        self.replacing_order
            .as_deref()
            .filter(|_| self.ids.is_empty())
            .map(|symbol| safe_cex_engine::error::order_gone_text(symbol, unknown))
    }
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
    pub(crate) protections: HashMap<String, NativeProtection>,
}

#[derive(Clone)]
pub(crate) struct NativeProtection {
    pub(crate) order: Order,
    pub(crate) query: Query,
    pub(crate) reference: StopTriggerType,
    pub(crate) price_protect: bool,
}

struct StopGuard {
    order: Order,
    price: f64,
    reference: StopTriggerType,
}
impl StopGuard {
    fn validate(&self, store: &safe_cex_engine::Store, now: f64) -> Result<(), VenueError> {
        let unchanged = || format!("Your {} stop was not changed", self.order.symbol);
        if store.order(&self.order.id) != Some(&self.order) {
            return Err(safe_cex_engine::protective_amend::refused(Some(format!(
                "{}: the original order changed before cancellation",
                unchanged()
            ))));
        }
        let current = store
            .reference_seen_at(&self.order.symbol, self.reference)
            .filter(|seen| *seen <= now)
            .and_then(|_| {
                safe_cex_engine::protective_amend::fresh_reference(
                    store,
                    &self.order.symbol,
                    self.reference,
                    now,
                )
            });
        let label = if self.reference == StopTriggerType::MarkPrice {
            "mark"
        } else {
            "last"
        };
        let Some(current) = current else {
            return Err(safe_cex_engine::protective_amend::refused(Some(format!(
                "{}: a fresh {label} price is required before cancellation",
                unchanged()
            ))));
        };
        let below =
            (self.order.kind == OrderType::StopLoss) == (self.order.side == OrderSide::Sell);
        if (below && self.price >= current) || (!below && self.price <= current) {
            return Err(safe_cex_engine::protective_amend::refused(Some(format!(
                "{}: the replacement would trigger immediately against the {label} price",
                unchanged()
            ))));
        }
        Ok(())
    }
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
/// Aster documents the sweep's success as
/// `{"code": "200", "msg": "The operation of cancel all open order is done."}`.
#[derive(Deserialize)]
struct SweepAck {
    code: Id,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StpResponse {
    stp_mode: StpMode,
}

impl AsterDex {
    pub(crate) fn trade_unproven_evidence(
        &self,
        pending: &Pending,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        let Pending::Signed(signed) = pending else {
            return;
        };
        let Pending::Trade(request) = &signed.pending else {
            return;
        };
        if let Some(journal) = self
            .trading
            .jobs
            .get(&request.command)
            .and_then(|job| job.journal.as_ref())
        {
            evidence.write_plan(journal);
        }
    }

    pub(crate) fn discard_trade_owner(&mut self, owner: safe_cex_engine::request_context::Owner) {
        self.trading
            .jobs
            .retain(|_, job| !job.plan.as_ref().is_some_and(|plan| plan.owner() == owner));
    }

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
                let mut job = self.plan_cancel_refs(&orders);
                if let Ok(job) = &mut job
                    && !symbol.is_empty()
                {
                    job.steps.push_back(Planned {
                        method: Method::Delete,
                        endpoint: "allOpenOrders",
                        query: Query::new().str("symbol", &symbol),
                        call: Call::CancelSymbol(symbol.clone()),
                    });
                }
                (command, job)
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
            write: None,
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
                job.journal = Some(WritePlan::new(job
                    .steps
                    .iter()
                    .chain(job.restore.iter())
                    .enumerate()
                    .map(|(index, step)| Element {
                        index,
                        client_order_id: step.query.value("newClientOrderId").map(str::to_owned),
                    })
                    .collect()));
                // Validate the shared auth shape before reserving any split step.
                if let Some(step) = job.steps.front()
                    && let Err((error, kind)) = self.check_signing(step.call.role())
                {
                    return Err((job.failure(VenueError::new(error).kind(kind)), prune_on_refusal));
                }
                job.reserved_places = job
                    .steps
                    .iter()
                    .filter(|step| matches!(step.call, Call::Place { .. }))
                    .count() + usize::from(job.restore.is_some());
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
            .map(|(selection, identity)| selection.credential(identity.credential.clone()));
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
                let _ = cx.fail_preflight(&preflight, job.failure(error.venue_error()));
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
            .map(|(selection, identity)| selection.credential(identity.credential.clone()));
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
            let _ = cx.fail_preflight(preflight, job.failure(error));
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
                let _ = cx.fail_preflight(preflight, job.failure(error.venue_error()));
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
        let next = if job.error.is_some()
            && !job.continue_on_error
            && !(job.restoring && !job.steps.is_empty())
        {
            None
        } else {
            job.steps.pop_front()
        };
        let Some(plan) = job.plan.clone() else {
            return;
        };
        if let Some(step) = next {
            let Some(slot) = job.slot.take() else {
                return;
            };
            // A 408 or 5xx on a put-back is sealed before this venue sees it:
            // say what is unknown rather than repeat the replacement's refusal.
            if job.restoring
                && let Some((symbol, ..)) = &job.replacing_stop
            {
                let words = format!(
                    "Your {symbol} stop's replacement was refused and its put-back's outcome is unknown. Check Open Orders"
                );
                let _ = cx.present_plan_failure(&plan, words);
            }
            let write = job.next_element;
            job.next_element += 1;
            if let Some(journal) = &mut job.journal {
                let _ = journal.record(
                    write,
                    failed_element(VenueError::new(ENGINE_UNKNOWN_TEXT).outcome(Outcome::Unknown)),
                );
            }
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
                    write: Some(write),
                }),
            );
            return;
        }
        let Some(mut job) = self.trading.jobs.remove(&id) else {
            return;
        };
        let _ = match job.error.take() {
            Some(mut error)
                if !job.continue_on_error
                    || job.sweep_failed
                    || job.successes == 0
                    || error.outcome == Some(Outcome::Unknown) =>
            {
                // A later rejection after a step took effect cannot mean nothing happened.
                if job.successes > 0 && error.outcome == Some(Outcome::Rejected) {
                    error.outcome = None;
                }
                if error.outcome == Some(Outcome::Unknown) {
                    cx.event(
                        safe_cex_engine::Level::Warn,
                        "write.unknown",
                        ENGINE_UNKNOWN_TEXT,
                        vec![("command", id.into()), ("outcome", "unknown".into())],
                    );
                }
                cx.fail_plan(&plan, job.failure(error))
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
        response: Result<Response<'_>, VenueError>,
        leg: Option<&Context>,
        reported: bool,
    ) {
        let command = request.command;
        let root = request.root.clone();
        let plan = request.plan.clone();
        let call = &request.call;
        let was_place = matches!(call, Call::Place { .. });
        let was_cancel = matches!(call, Call::Cancel { .. });
        let was_sweep = matches!(call, Call::CancelSymbol(_));
        let cancelled = match &request.call {
            Call::Cancel { id, .. } => vec![id.clone()],
            _ => vec![],
        };
        let transport_error = response.as_ref().err().cloned();
        let transport = transport_error.is_some();
        let applied = match &request.call {
            Call::Cancel { id, .. } => Some((AppliedEffect::Cancel, Some(id.clone()), None)),
            Call::CancelSymbol(symbol) => Some((AppliedEffect::Cancel, None, Some(symbol.clone()))),
            Call::Leverage(symbol, _) | Call::Margin(symbol, _) => {
                Some((AppliedEffect::Settings, None, Some(symbol.clone())))
            }
            Call::Mode(_) | Call::StpWrite => Some((AppliedEffect::Settings, None, None)),
            _ => None,
        };
        let mut cancel_applied = false;
        // A gateway failure is unproven whatever body it carries.
        let unproven = response
            .as_ref()
            .is_ok_and(|r| r.status == 408 || r.status >= 500);
        let status_429 = response.as_ref().is_ok_and(|r| r.status == 429);
        let outcome = match response {
            Ok(ref response) => venue_error(response).map_or(Ok(response.body), Err),
            Err(error) => Err((0, error.message)),
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
        // A 429 or a documented rate-limit code (-1003 TOO_MANY_REQUESTS, -1015
        // TOO_MANY_ORDERS) may have placed the order: a guarded leg's outcome is unknown.
        let rate_limited = status_429 || matches!(outcome, Err((-1003 | -1015, _)));
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
            // An amend's cancel leg must prove the order ended: a reply that says nothing
            // readable leaves it maybe live, and a replacement could double it.
            (
                Call::Cancel {
                    filled_at_amend: Some(_),
                    ..
                },
                Ok(body),
            ) if !serde_json::from_str::<CancelAck>(body).is_ok_and(|ack| ack.status.is_some()) => {
                Err((
                    0,
                    "Aster cancel answer was unreadable; replacement was not submitted".into(),
                ))
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
                cancel_applied = true;
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
            (Call::CancelSymbol(_), Ok(body)) => {
                self.account.order_revision = self.account.order_revision.wrapping_add(1);
                self.request_refresh_orders(cx);
                match serde_json::from_str::<SweepAck>(body).map(|ack| ack.code.text()) {
                    Ok(code) if code == "200" => Ok(()),
                    _ => Err((0, format!("Aster cancel-all was not confirmed: {body}"))),
                }
            }
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
        // Keep the engine's classified kind and outcome in the logical journal.
        let mut classified_error = None;
        let protective = was_place
            && self
                .trading
                .jobs
                .get(&command)
                .is_some_and(|job| job.replacing_stop.is_some());
        if let Some(leg) = leg.filter(|_| {
            !reported
                && !transport_error
                    .as_ref()
                    .is_some_and(|e| e.outcome == Some(Outcome::NotSent))
        }) {
            let _ = match &outcome {
                _ if replied
                    && !(was_place && placed.is_none())
                    && !(was_sweep && outcome.is_err()) =>
                {
                    cx.http_write_fact(
                        leg,
                        WriteFact::Accepted {
                            order_ids: placed.iter().cloned().chain(cancelled).collect(),
                        },
                        None,
                    )
                }
                Err((code, message)) if !transport && !unproven && !replied => {
                    let native = Native {
                        code: NativeCode::Int(*code),
                        message: refusal_1000(message)
                            .filter(|_| *code == -1000)
                            .unwrap_or(message),
                    };
                    let native = (*code != 0).then_some(&native);
                    let told = match self
                        .trading
                        .jobs
                        .get(&command)
                        .filter(|_| was_place)
                        .and_then(|job| {
                            job.gone_text(
                                rate_limited
                                    || protective
                                        && protective_failure(*code, message.clone()).outcome
                                            == Some(Outcome::Unknown),
                            )
                        }) {
                        Some(gone) => format!("{gone}: {message}"),
                        None => message.clone(),
                    };
                    let table = if protective {
                        &ASTER_PROTECTIVE
                    } else {
                        &ASTER
                    };
                    match cx.classify_write(leg, table, native, told) {
                        Ok(fact) => {
                            classified_error = Some(fact.error().clone());
                            cx.http_write_fact(
                                leg,
                                WriteFact::Failed {
                                    fact,
                                    known_effects: false,
                                },
                                None,
                            )
                        }
                        Err(error) => Err(error),
                    }
                }
                Err((_, message)) => cx.http_write_unproven(leg, message.clone()).map(|_| None),
                Ok(()) => cx
                    .http_write_unproven(leg, "Aster write reply was not proven")
                    .map(|_| None),
            };
        }
        if was_place && let Some(job) = self.trading.jobs.get_mut(&command) {
            if outcome.is_err() && !job.restoring && !transport && !unproven && !replied {
                let definitive = outcome.as_ref().err().is_some_and(|(code, message)| {
                    matches!(
                        protective_failure(*code, message.clone()).outcome,
                        Some(Outcome::Rejected)
                    )
                });
                if definitive
                    && !rate_limited
                    && let Some(restore) = job.restore.take()
                {
                    job.steps.clear();
                    job.steps.push_back(restore);
                    job.restoring = true;
                }
            } else if outcome.is_ok()
                && job.restoring
                && let Some(error) = &mut job.error
            {
                error.protection = Some(safe_cex_engine::error::Protection::Restored);
                error.presents = true;
                error.message = format!(
                    "Your {} stop is unchanged: its replacement was refused, so the old stop was put back as a new order",
                    job.replacing_stop.as_ref().map_or("", |(s, _, _)| s)
                );
            }
        }
        // The fact is recorded: continue the serial plan into the next native
        // step, or settle this last call before the terminal.
        if let (Some(leg), Some((plan, slot))) = (leg, &plan) {
            let continues = self.trading.jobs.get(&command).is_some_and(|job| {
                !job.steps.is_empty() && (job.continue_on_error || outcome.is_ok() || job.restoring)
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
            let mut element_error = None;
            match &outcome {
                Ok(()) if was_cancel && job.replacing_order.is_some() => {
                    // TS cancels first, then runs its continuing placement batch.
                    job.continue_on_error = true;
                }
                Ok(()) => job.successes += 1,
                Err((code, message)) => {
                    cx.command_error(command, message.clone());
                    let mut error = transport_error
                        .or(classified_error)
                        .unwrap_or_else(|| failure(*code, message.clone()));
                    if unproven || transport && error.outcome != Some(Outcome::NotSent) {
                        error.outcome = Some(Outcome::Unknown);
                    }
                    let guarded = was_place && job.gone_text(true).is_some();
                    if guarded && rate_limited
                        || protective
                            && !transport
                            && protective_failure(*code, message.clone()).outcome
                                == Some(Outcome::Unknown)
                    {
                        error.outcome = Some(Outcome::Unknown);
                    }
                    element_error = Some(error.clone());
                    // Earlier ambiguity belongs to the aggregate, not this row.
                    if job
                        .error
                        .as_ref()
                        .is_some_and(|e| e.outcome == Some(Outcome::Unknown))
                    {
                        error.outcome = Some(Outcome::Unknown);
                    }
                    let unknown = error.outcome == Some(Outcome::Unknown);
                    job.sweep_failed |= was_sweep;
                    if was_place && let Some(gone) = job.gone_text(unknown) {
                        error.protection =
                            (!unknown).then_some(safe_cex_engine::error::Protection::Unprotected);
                        error.presents = true;
                        // A status-only refusal already carries the gone text it was classified with.
                        let detail = if rate_limited {
                            message
                        } else {
                            &error.message
                        };
                        error.message = format!("{gone}: {detail}");
                    }
                    job.error = Some(error);
                }
            }
            if let (Some(index), Some(journal)) = (request.write, &mut job.journal) {
                let result = if outcome.is_ok() || cancel_applied {
                    if let Some(id) = &placed {
                        ElementResult::Placed {
                            order_ids: vec![id.clone()],
                        }
                    } else if let Some((effect, target_order_id, position_key)) = applied {
                        ElementResult::Applied {
                            effect,
                            target_order_id,
                            position_key,
                        }
                    } else {
                        failed_element(VenueError::new("Aster write acceptance is unproven"))
                    }
                } else {
                    failed_element(
                        element_error
                            .unwrap_or_else(|| VenueError::new("Aster write outcome is unknown")),
                    )
                };
                let _ = journal.record(index, result);
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
    // Only the destructive cancel is guarded; the final claim reads current
    // Store state synchronously and a veto follows the engine's normal unwind.
    pub(crate) fn stop_release_veto(
        &self,
        store: &safe_cex_engine::Store,
        pending: &Pending,
        release: &safe_cex_engine::request_context::HttpRelease,
    ) -> Result<(), VenueError> {
        let Pending::Signed(signed) = pending else {
            return Ok(());
        };
        let Pending::Trade(request) = &signed.pending else {
            return Ok(());
        };
        if let Call::Cancel {
            filled_at_amend: Some(_),
            ..
        } = &request.call
            && let Some(guard) = self
                .trading
                .jobs
                .get(&request.command)
                .filter(|job| {
                    request.plan.as_ref().is_some_and(|(plan, _)| {
                        job.plan
                            .as_ref()
                            .is_some_and(|own| own.owner() == plan.owner())
                    })
                })
                .and_then(|job| job.guard.as_ref())
        {
            guard.validate(store, release.time.raw_ms)?;
        }
        Ok(())
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
        // Size is what is left to fill: the venue treats a replacement's size as new.
        let size = match amount {
            Some(total) if total <= order.filled => {
                return Err(format!(
                    "Aster amendment size must exceed the {} already filled",
                    order.filled
                ));
            }
            Some(total) => total - order.filled,
            None => order.remaining,
        };
        // A replacement below the value minimum would be refused after the old order is gone.
        let value = size * price.unwrap_or(order.price);
        let minimum = self
            .market_rules
            .get(&order.symbol)
            .map_or(0.0, |r| r.min_notional);
        if !conditional && !order.reduce_only && value > 0.0 && value < minimum {
            return Err(format!(
                "Aster replacement worth {value} is below the {minimum} minimum; the order is unchanged"
            ));
        }
        let replacement = PlaceOrderOpts {
            symbol: order.symbol.clone(),
            kind: order.kind,
            side: order.side,
            amount: size,
            price: (!conditional).then_some(price.unwrap_or(order.price)),
            trigger_price: conditional.then_some(price.unwrap_or(order.price)),
            reduce_only: Some(order.reduce_only),
            time_in_force: order.time_in_force,
            trigger_type: order.trigger_type,
            ..empty_order()
        };
        // Format and validate before canceling the live order.
        let mut place = self.plan_place_inner(cx, &[replacement], Some(id), position_side)?;
        if let Some(error) = place.error {
            return Err(error.message);
        }
        let mut job = self.plan_cancel(std::slice::from_ref(order), false);
        if conditional {
            let native = self.trading.protections.get(id)
                .filter(|native| native.order == *order)
                .ok_or_else(|| format!("Your {} stop was not changed: its exact native parameters must be refreshed before cancellation", order.symbol))?;
            // STOP and TAKE_PROFIT rest a limit: the replacement places only market stops, and
            // moving the trigger alone would leave a stale limit, so they are not edited at all.
            if matches!(native.query.value("type"), Some("STOP" | "TAKE_PROFIT")) {
                return Err(format!(
                    "Your {} stop-limit was not changed: AsterDEX limit stops cannot be edited without losing their limit price; cancel it and place a new one",
                    order.symbol
                ));
            }
            if order.filled != 0.0 {
                return Err(format!(
                    "Your {} stop was not changed: it already has fills",
                    order.symbol
                ));
            }
            let step = place
                .steps
                .front()
                .ok_or("Aster replacement has no native order")?;
            let trigger = step
                .query
                .value("stopPrice")
                .and_then(|p| p.parse::<f64>().ok())
                .ok_or("Aster replacement has no trigger")?;
            let guard = StopGuard {
                order: order.clone(),
                price: trigger,
                reference: native.reference,
            };
            guard.validate(cx.store, cx.now).map_err(|e| e.message)?;
            // The replacement keeps the original's price protection, as its put-back does.
            if let Some(step) = place.steps.front_mut() {
                step.query = std::mem::replace(&mut step.query, Query::new()).str(
                    "priceProtect",
                    if native.price_protect {
                        "TRUE"
                    } else {
                        "FALSE"
                    },
                );
            }
            job.guard = Some(guard);
            job.restore = Some(Planned {
                method: Method::Post,
                endpoint: "order",
                query: native.query.clone(),
                call: Call::Place {
                    symbol: order.symbol.clone(),
                    position_side,
                },
            });
        }
        job.steps.extend(place.steps);
        job.replacing_stop = conditional.then(|| (order.symbol.clone(), order.side, order.kind));
        job.replacing_order = (!conditional).then(|| order.symbol.clone());
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
            let max = usize::try_from(
                self.market_rules
                    .get(&opts.symbol)
                    .and_then(|r| r.max_algo_orders)
                    .unwrap_or(0),
            )
            .unwrap_or(usize::MAX);
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
        // A MARKET order answers to MARKET_LOT_SIZE; every other order to LOT_SIZE.
        let market_lot = self
            .market_rules
            .get(&opts.symbol)
            .map(|rule| &rule.market_lot)
            .filter(|_| opts.kind == OrderType::Market);
        let step = market_lot.map_or(market.precision.amount, |lot| lot.step);
        let amount = num::adjust(opts.amount, step);
        if opts.amount > 0.0 && amount <= 0.0 {
            return Err("Aster order amount rounds to zero".into());
        }
        let close = conditional && opts.amount == 0.0;
        let max = market_lot
            .map(|lot| lot.max)
            .filter(|max| *max > 0.0)
            .unwrap_or(market.limits.amount.max);
        let max = num::adjust(max, step);
        let min = market_lot.map_or(0.0, |lot| lot.min);
        let mut quantities = Vec::new();
        let mut remaining = amount;
        if !conditional && max > 0.0 && remaining > max {
            // Bound planning rather than allowing an unbounded allocation.
            if remaining / max > 10000.0 {
                return Err("Aster order requires too many split requests".into());
            }
            while remaining > max {
                quantities.push(max);
                remaining = num::adjust(remaining - max, step);
            }
            // A remainder under the minimum borrows it from the chunk before.
            if remaining < min
                && let Some(last) = quantities.last_mut()
            {
                let short = num::adjust(min - remaining, step);
                *last = num::adjust(*last - short, step);
                remaining = num::adjust(remaining + short, step);
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

/// `failure` for a stop's replacement or put-back: a code `ASTER` does not list as a
/// refusal leaves the write unknown, as `ASTER_PROTECTIVE` classifies it.
fn protective_failure(code: i64, message: String) -> VenueError {
    let listed = {
        let native = Native {
            code: NativeCode::Int(code),
            message: refusal_1000(&message)
                .filter(|_| code == -1000)
                .unwrap_or(&message),
        };
        // Every refusal verdict is listed; only an unknown or satisfied answer is not.
        !matches!(
            ASTER_PROTECTIVE.verdict(&native, Intent::Place),
            Verdict::Unknown(_) | Verdict::Satisfied(_)
        )
    };
    let mut error = failure(code, message);
    if !listed && error.outcome == Some(Outcome::Rejected) {
        error.outcome = Some(Outcome::Unknown);
    }
    error
}
/// A trading failure classified by its Aster code; code 0 is a transport
/// failure, a non-JSON HTTP error or an unproven acknowledgement.
fn failure(code: i64, message: String) -> VenueError {
    let (kind, outcome) = match code {
        -1000 if refusal_1000(&message) == Some(DEVICE_TIME) => {
            (Some(ErrorKind::Rejected), Outcome::Rejected)
        }
        -1000 if refusal_1000(&message).is_some() => {
            (Some(ErrorKind::AuthInvalid), Outcome::Rejected)
        }
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
