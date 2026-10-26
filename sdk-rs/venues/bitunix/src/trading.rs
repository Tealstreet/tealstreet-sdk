//! Trading command jobs: each command owns a Preflight root, then one serial
//! Plan. Jobs run concurrently under the engine's per-order locks; every call
//! signs Primary, and write outcomes ride the journal.
use crate::account::{Axis, NativeOrder};
use crate::sign::Params;
use crate::wire::Str;
use crate::{BitunixFutures, Pending, codes, error};
use safe_cex_engine::classify::{BatchItem, Intent, Native, NativeCode, WriteFact};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::plan::{SerialCall, SerialPolicy, Slot};
use safe_cex_engine::request_context::{
    Context, Credential, HttpOwnerRetirement, Operation, Plan, Policy, Preflight, PreflightPolicy,
    PreflightPurpose, ReconciliationData, RetryPolicy, WriteResult,
};
use safe_cex_engine::signing::Identity as SigningIdentity;
use safe_cex_engine::{Command, Cx, ErrorKind, Method, Outcome, Response, VenueError, num};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
const PLACE: &str = "/api/v1/futures/trade/place_order";
const BATCH: &str = "/api/v1/futures/trade/batch_order";
const TPSL: &str = "/api/v1/futures/tpsl/place_order";
const FULL_TPSL: &str = "/api/v1/futures/tpsl/position/place_order";
const CANCEL: &str = "/api/v1/futures/trade/cancel_orders";
const CANCEL_TPSL: &str = "/api/v1/futures/tpsl/cancel_order";
const AMEND: &str = "/api/v1/futures/trade/modify_order";
const AMEND_TPSL: &str = "/api/v1/futures/tpsl/modify_order";
const AMEND_FULL: &str = "/api/v1/futures/tpsl/position/modify_order";

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trade_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effect: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_stop_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_order_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_order_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_stop_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_order_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_order_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_qty: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Batch<'a> {
    symbol: &'a str,
    order_list: Vec<Payload>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cancel<'a> {
    symbol: &'a str,
    order_list: Vec<Identity<'a>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity<'a> {
    order_id: &'a str,
}

/// Each call's own bound; the streamed Plan has no whole-job deadline.
pub(crate) const CALL_MS: u32 = 20_000;

pub struct Request {
    pub command: u32,
    step: Step,
    slot: Slot,
    context: Context,
    resent: bool,
}
impl Request {
    /// Bounded correlation for a released write: its endpoint.
    pub(crate) fn reconciliation(
        &self,
    ) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
        ReconciliationData::new(self.step.path.to_owned())
    }
}
enum Call {
    Place {
        clients: Vec<String>,
        batch: bool,
    },
    Cancel {
        ids: Vec<String>,
        require_ack: bool,
    },
    ReconcileReplacement {
        parent: String,
        id: String,
        leg: Option<&'static str>,
        payload: Box<Payload>,
    },
    Amend {
        id: String,
        parent: String,
        leg: Option<&'static str>,
        fallback: Option<Box<Payload>>,
    },
    Leverage {
        symbol: String,
        value: f64,
    },
    Margin {
        symbol: String,
        isolated: bool,
    },
    Mode {
        hedged: bool,
    },
}
struct Step {
    path: &'static str,
    body: String,
    call: Call,
    delay: f64,
}
impl Step {
    fn read(&self) -> bool {
        matches!(self.call, Call::ReconcileReplacement { .. })
    }
    fn single_place(&self) -> bool {
        matches!(self.call, Call::Place { batch: false, .. })
    }
    fn setting(&self) -> bool {
        matches!(
            self.call,
            Call::Leverage { .. } | Call::Margin { .. } | Call::Mode { .. }
        )
    }
}
#[derive(Default)]
struct Job {
    steps: VecDeque<Step>,
    ids: Vec<String>,
    optimistic: Vec<String>,
    restore: Vec<OrderStatus>,
    recover: Option<Vec<PlaceOrderOpts>>,
    created: Vec<String>,
}
struct Prepare {
    orders: Vec<PlaceOrderOpts>,
    stage: u8,
    retried: bool,
}
/// One command's authority: its Preflight root, then its serial Plan.
struct Live {
    preflight: Preflight,
    plan: Option<Plan>,
    /// The delivered call the next one continues from.
    current: Option<(Slot, Context)>,
    job: Job,
    prepare: Option<Prepare>,
    /// The settings scope this job holds (per symbol, or the account's mode).
    settings: Option<String>,
    /// A paced call waiting for the Plan timer.
    waiting: Option<Step>,
    /// A batch chunk's refusal: later chunks still go out, then the job fails.
    failure: Option<VenueError>,
    /// Native text that follows the journal's selected error.
    note: Option<String>,
    /// A settings write is released without a definitive answer yet.
    settings_released: bool,
    /// An amend job: its fallback replacement is fenced as a placement.
    amend: bool,
}
#[derive(Default)]
pub struct TradingState {
    live: HashMap<u32, Live>,
    settings: HashSet<String>,
    /// Settings scopes whose change may still execute: held until a read
    /// issued at or after the serial lands (`None`: that read is not issued).
    held: HashMap<String, Option<u64>>,
    /// Jobs the engine sealed itself; their rows restore at the next callback.
    retired: Vec<Job>,
}
impl TradingState {
    pub(crate) fn plan(&self, command: u32) -> Option<&Plan> {
        self.live.get(&command)?.plan.as_ref()
    }
    pub(crate) fn preflight(&self, command: u32) -> Option<&Preflight> {
        self.live.get(&command).map(|live| &live.preflight)
    }
    /// Takes the continuation point for a prerequisite Read call.
    pub(crate) fn take_current(&mut self, command: u32) -> Option<(Slot, Context)> {
        self.live.get_mut(&command)?.current.take()
    }
    pub(crate) fn live(&self, command: u32) -> bool {
        self.live.contains_key(&command)
    }
    /// The hold's read failed: the next refused change issues another.
    pub(crate) fn settings_unread(&mut self, key: &str) {
        if let Some(issued) = self.held.get_mut(key) {
            *issued = None;
        }
    }
    /// A fresh settings read for `key` landed: release an uncertain hold.
    pub(crate) fn settings_observed(&mut self, key: &str, serial: u64) {
        if let Some(Some(threshold)) = self.held.get(key)
            && serial >= *threshold
        {
            self.held.remove(key);
        }
    }
}
fn text<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).expect("finite request fields")
}
fn empty_list() -> &'static serde_json::value::RawValue {
    serde_json::from_str("[]").expect("literal empty array")
}
fn ack_rows(
    raw: &serde_json::value::RawValue,
) -> Result<Vec<&serde_json::value::RawValue>, String> {
    serde_json::from_str(raw.get()).map_err(|error| error.to_string())
}
fn empty_id() -> &'static serde_json::value::RawValue {
    serde_json::from_str(r#""""#).expect("literal empty string")
}
/// Extract fields independently: malformed ancillary metadata cannot erase a native ID.
fn ack_identity(raw: &str, allow_client: bool) -> Result<(Option<String>, bool), String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Identity<'a> {
        #[serde(borrow, default = "empty_id", alias = "id")]
        order_id: &'a serde_json::value::RawValue,
        #[serde(borrow, default = "empty_id")]
        client_id: &'a serde_json::value::RawValue,
    }
    let row: Identity<'_> = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    let order = serde_json::from_str::<Str<'_>>(row.order_id.get());
    let client = serde_json::from_str::<Str<'_>>(row.client_id.get());
    let malformed = order.is_err()
        || client.is_err()
        || order
            .as_ref()
            .is_ok_and(|id| !id.is_empty() && id.trim().is_empty())
        || client
            .as_ref()
            .is_ok_and(|id| !id.is_empty() && id.trim().is_empty());
    let id = order
        .ok()
        .filter(|id| !id.trim().is_empty())
        .map(|id| id.into_owned())
        .or_else(|| {
            allow_client
                .then(|| {
                    client
                        .ok()
                        .filter(|id| !id.trim().is_empty())
                        .map(|id| id.into_owned())
                })
                .flatten()
        });
    Ok((id, malformed))
}
fn stop(t: Option<StopTriggerType>) -> &'static str {
    if t == Some(StopTriggerType::MarkPrice) {
        "MARK_PRICE"
    } else {
        "LAST_PRICE"
    }
}
fn effect(t: Option<TimeInForce>) -> &'static str {
    match t.unwrap_or(TimeInForce::GoodTillCancel) {
        TimeInForce::GoodTillCancel => "GTC",
        TimeInForce::ImmediateOrCancel => "IOC",
        TimeInForce::FillOrKill => "FOK",
        TimeInForce::PostOnly => "POST_ONLY",
    }
}
fn positive(v: f64, name: &str) -> Result<f64, String> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(format!("Bitunix {name} must be greater than zero"))
    }
}

fn split_quantities(
    amount: f64,
    step: f64,
    minimum: f64,
    maximum: f64,
) -> Result<Vec<f64>, String> {
    // Engine quantities are f64; native-unit counts must round-trip exactly.
    const MAX_TICKS: f64 = 9_007_199_254_740_991.0;
    let step = positive(step, "quantity step")?;
    let maximum = positive(maximum, "maximum quantity")?;
    let ticks = (amount / step).round();
    if !minimum.is_finite()
        || minimum < 0.0
        || !ticks.is_finite()
        || !(1.0..=MAX_TICKS).contains(&ticks)
    {
        return Err("Bitunix quantity cannot be represented in native units".into());
    }
    let ticks = ticks as u64;
    let mut max_ticks = (maximum / step).floor().min(MAX_TICKS) as u64;
    // A decimal limit such as 0.3 / 0.1 may divide just below an integer.
    // Admit the next tick only when its serialized decimal stays within max.
    let candidate = (maximum / step).ceil().min(MAX_TICKS) as u64;
    if num::adjust(candidate as f64 * step, step) <= maximum {
        max_ticks = candidate;
    }
    if max_ticks == 0 {
        return Err("Bitunix maximum quantity is smaller than one native unit".into());
    }
    let count = ticks.div_ceil(max_ticks);
    if count > 10_000 {
        return Err("Bitunix order exceeds split limit".into());
    }
    let quantity = |units: u64| -> Result<f64, String> {
        let value = num::adjust(units as f64 * step, step);
        if !value.is_finite()
            || value < minimum
            || value <= 0.0
            || value > maximum
            || (value / step).round() != units as f64
        {
            return Err("Bitunix split quantity is outside native lot limits".into());
        }
        Ok(value)
    };
    let mut sizes = Vec::with_capacity(count as usize);
    for _ in 0..ticks / max_ticks {
        sizes.push(quantity(max_ticks)?);
    }
    let remainder = ticks % max_ticks;
    if remainder != 0 {
        sizes.push(quantity(remainder)?);
    }
    Ok(sizes)
}

impl BitunixFutures {
    /// The Primary credential and identity every trading call signs with.
    pub(crate) fn trade_identity(&self) -> Option<(Credential, SigningIdentity)> {
        let (selection, identity) = self.slots.select(CredentialRole::Mutation).ok()?;
        Some((
            selection.credential(identity.credential.clone()),
            identity.clone(),
        ))
    }
    /// A validation refusal before any write: the command's one NotSent.
    fn refuse(&mut self, cx: &mut Cx<'_, Pending>, command: u32, error: String) {
        self.end(cx, command, error::not_sent(error, ErrorKind::InvalidInput));
    }
    fn preflight(&self, cx: &Cx<'_, Pending>, orders: &[PlaceOrderOpts]) -> Result<(), String> {
        let mut ids = HashSet::new();
        for o in orders {
            if o.stp_mode.is_some() {
                return Err("Bitunix does not support self-trade prevention controls".into());
            }
            if o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some() {
                return Err("Bybit BBO and TickSize controls are not supported by Bitunix".into());
            }
            let m = cx
                .store
                .market(&o.symbol)
                .ok_or_else(|| format!("Market {} not found", o.symbol))?;
            if let Some(id) = &o.client_order_id {
                if id.is_empty() || id.chars().any(char::is_whitespace) || !ids.insert(id) {
                    return Err(
                        "Bitunix clientOrderId must be nonempty, unique and contain no whitespace"
                            .into(),
                    );
                }
                if !matches!(o.kind, OrderType::Limit | OrderType::Market)
                    || o.trigger_price.is_some()
                    || o.stop_loss.is_some()
                    || o.take_profit.is_some()
                    || o.amount > m.limits.amount.max
                {
                    return Err(
                        "Bitunix clientOrderId requires exactly one plain native order".into(),
                    );
                }
            }
            // The app sends SL/TP with triggerPrice equal to price; TS places them from price.
            if matches!(o.kind, OrderType::TrailingStopLoss | OrderType::Conditional)
                || (o.trigger_price.is_some() && !o.redundant_protective_trigger())
            {
                return Err("Bitunix conditional and trailing orders are not supported".into());
            }
            if o.amount < 0.0 || !o.amount.is_finite() {
                return Err("Bitunix order amount is invalid".into());
            }
            if matches!(o.kind, OrderType::Limit | OrderType::Market) {
                positive(o.amount, "quantity")?;
            }
            if o.kind != OrderType::Market {
                positive(o.price.unwrap_or(0.0), "price")?;
            }
            for v in [o.take_profit, o.stop_loss].into_iter().flatten() {
                positive(v, "protection trigger")?;
            }
        }
        Ok(())
    }
    /// Root first: each command opens its Preflight before validation, so
    /// every refusal is that root's NotSent. Jobs stay concurrent (no FIFO);
    /// the engine's order lock holds each order for its whole job.
    pub(crate) fn trading_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let settings = match &command {
            Command::PlaceOrders { .. }
            | Command::CancelOrders { .. }
            | Command::CancelSymbolOrders { .. }
            | Command::AmendOrder { .. } => None,
            Command::SetLeverage { symbol, .. } | Command::SetMarginType { symbol, .. } => {
                Some(format!("symbol:{symbol}"))
            }
            Command::SetPositionMode { .. } => Some("mode".to_owned()),
            _ => return,
        };
        // Keyless: no write root exists, so the local refusal authority answers.
        let Some((credential, _)) = self.trade_identity() else {
            let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                error::not_sent("Bitunix credentials required", ErrorKind::AuthInvalid),
            );
            return;
        };
        let policy = PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: Some(credential.clone()),
            write_credential: Some(credential),
        };
        let Ok(preflight) = cx.begin_preflight(policy) else {
            return;
        };
        let id = match &command {
            Command::PlaceOrders { command, .. }
            | Command::CancelOrders { command, .. }
            | Command::CancelSymbolOrders { command, .. }
            | Command::AmendOrder { command, .. }
            | Command::SetLeverage { command, .. }
            | Command::SetMarginType { command, .. }
            | Command::SetPositionMode { command, .. } => *command,
            _ => return,
        };
        self.trading.live.insert(
            id,
            Live {
                preflight,
                plan: None,
                current: None,
                job: Job::default(),
                prepare: None,
                settings: None,
                waiting: None,
                failure: None,
                note: None,
                settings_released: false,
                amend: matches!(command, Command::AmendOrder { .. }),
            },
        );
        self.restore_retired(cx);
        // (b): one settings change per scope; a second is refused, not queued.
        if let Some(key) = settings {
            if self.trading.settings.contains(&key) {
                return self.end(
                    cx,
                    id,
                    error::not_sent(
                        "Bitunix setting change is already in flight",
                        ErrorKind::InvalidInput,
                    ),
                );
            }
            // An uncertain change may still execute: wait for a fresh read
            // (issued once; a failed one is issued again here).
            if self.trading.held.contains_key(&key) {
                self.reread_held(cx);
                return self.end(
                    cx,
                    id,
                    error::not_sent(
                        "Bitunix setting change outcome is unresolved; a fresh settings read is pending",
                        ErrorKind::InvalidInput,
                    ),
                );
            }
            self.trading.settings.insert(key.clone());
            self.trading.live.get_mut(&id).expect("live job").settings = Some(key);
        }
        let result = match command {
            Command::PlaceOrders { command, orders } => {
                if let Err(e) = self.preflight(cx, &orders) {
                    return self.refuse(cx, command, e);
                }
                return self.start_place(cx, command, orders, false);
            }
            Command::CancelOrders { ids, .. } => self.plan_cancel(cx, &ids),
            Command::CancelSymbolOrders { symbol, .. } => {
                let ids: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == symbol && o.status == OrderStatus::Open)
                    .map(|o| o.id.clone())
                    .collect();
                self.plan_cancel(cx, &ids)
            }
            Command::AmendOrder {
                id, price, amount, ..
            } => self.plan_amend(cx, &id, price, amount),
            Command::SetLeverage {
                symbol, leverage, ..
            } => (|| {
                let m = cx
                    .store
                    .market(&symbol)
                    .ok_or_else(|| format!("Market {symbol} not found"))?;
                positive(leverage, "leverage")?;
                let value = leverage.clamp(m.limits.leverage.min, m.limits.leverage.max);
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Body<'a> {
                    margin_coin: &'static str,
                    symbol: &'a str,
                    leverage: f64,
                }
                Ok(Job {
                    steps: VecDeque::from([Step {
                        path: "/api/v1/futures/account/change_leverage",
                        body: text(&Body {
                            margin_coin: "USDT",
                            symbol: &symbol,
                            leverage: value,
                        }),
                        call: Call::Leverage { symbol, value },
                        delay: 0.0,
                    }]),
                    ..Job::default()
                })
            })(),
            Command::SetMarginType {
                symbol, isolated, ..
            } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Body<'a> {
                    symbol: &'a str,
                    margin_mode: &'static str,
                    margin_coin: &'static str,
                }
                if cx.store.market(&symbol).is_none() {
                    Err(format!("Market {symbol} not found"))
                } else {
                    Ok(Job {
                        steps: VecDeque::from([Step {
                            path: "/api/v1/futures/account/change_margin_mode",
                            body: text(&Body {
                                symbol: &symbol,
                                margin_mode: if isolated { "ISOLATION" } else { "CROSS" },
                                margin_coin: "USDT",
                            }),
                            call: Call::Margin { symbol, isolated },
                            delay: 0.0,
                        }]),
                        ..Job::default()
                    })
                }
            }
            Command::SetPositionMode { hedged, .. } => {
                if !cx.store.positions().is_empty() {
                    Err("Please close all positions before switching position mode".into())
                } else {
                    Ok(Job {
                        steps: VecDeque::from([Step {
                            path: "/api/v1/futures/account/change_position_mode",
                            body: format!(
                                r#"{{"positionMode":"{}"}}"#,
                                if hedged { "HEDGE" } else { "ONE_WAY" }
                            ),
                            call: Call::Mode { hedged },
                            delay: 0.0,
                        }]),
                        ..Job::default()
                    })
                }
            }
            _ => return,
        };
        match result {
            Ok(job) => {
                self.trading.live.get_mut(&id).expect("live job").job = job;
                self.advance(cx, id);
            }
            Err(e) => self.refuse(cx, id, e),
        }
    }
    fn start_place(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: u32,
        orders: Vec<PlaceOrderOpts>,
        retried: bool,
    ) {
        let needs_read = orders.iter().any(|o| {
            let tpsl = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
            (tpsl && o.amount == 0.0)
                || ((tpsl || o.reduce_only == Some(true))
                    && self.resolve_position(cx, o, tpsl).is_err())
        });
        if needs_read {
            let Some(live) = self.trading.live.get_mut(&command) else {
                return;
            };
            live.prepare = Some(Prepare {
                orders,
                stage: 0,
                retried,
            });
            self.read_axis_for(cx, Axis::Positions, Some(command));
            return;
        }
        self.place_prepared(cx, command, orders, retried);
    }
    /// A prerequisite read answered (positions, then orders, then TP/SL). Its
    /// delivery is the continuation point for the next read or the first write.
    pub(crate) fn prepared(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: u32,
        success: bool,
        custody: Option<crate::account::ReadCustody>,
    ) {
        let Some(live) = self.trading.live.get_mut(&command) else {
            return;
        };
        // The delivery hands off to the next read or the first write.
        let retired = match custody {
            Some(crate::account::ReadCustody::Plan(_, slot, context)) => {
                live.current = Some((slot, context));
                true
            }
            Some(crate::account::ReadCustody::Preflight(preflight, context)) => {
                cx.finish_preflight_call(&preflight, &context).is_ok()
            }
            _ => true,
        };
        if !success || !retired {
            live.prepare = None;
            return self.end(
                cx,
                command,
                error::not_sent(
                    "Bitunix preflight reconciliation failed",
                    ErrorKind::InvalidInput,
                ),
            );
        }
        let Some(p) = live.prepare.as_mut() else {
            return;
        };
        if p.stage < 2 {
            p.stage += 1;
            let axis = if p.stage == 1 {
                Axis::Orders
            } else {
                Axis::Tpsl
            };
            self.read_axis_for(cx, axis, Some(command));
        } else {
            let p = live.prepare.take().expect("prepared stage");
            self.place_prepared(cx, command, p.orders, p.retried);
        }
    }
    fn resolve_position(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        tpsl: bool,
    ) -> Result<String, String> {
        let side = if o.side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let side_text = if side == PositionSide::Long {
            "long"
        } else {
            "short"
        };
        if !cx
            .store
            .positions()
            .iter()
            .any(|p| p.symbol == o.symbol && p.side == side && p.contracts > 0.0)
        {
            return Err(format!(
                "No {side_text} position open for {} - nothing to reduce",
                o.symbol
            ));
        }
        let key = format!("{}:{side_text}", o.symbol);
        let rest = self.account.position_ids.get(&key);
        let ws = self.account.ws_position_ids.get(&key);
        if tpsl { ws.or(rest) } else { rest.or(ws) }
            .cloned()
            .ok_or_else(|| format!("Missing positionId for {}", o.symbol))
    }
    fn payloads(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
    ) -> Result<Vec<(&'static str, Payload)>, String> {
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| format!("Market {} not found", o.symbol))?;
        let tpsl = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
        let mut p = Payload {
            symbol: Some(o.symbol.clone()),
            ..Payload::default()
        };
        if tpsl {
            let position_id = self.resolve_position(cx, o, true)?;
            let full = o.amount == 0.0;
            p.position_id = Some(position_id.clone());
            let price = num::format(positive(
                num::adjust(o.price.unwrap_or(0.0), m.precision.price),
                "TP/SL price",
            )?);
            let qty = num::format(
                positive(num::adjust(o.amount, m.precision.amount), "TP/SL quantity")
                    .or_else(|e| if full { Ok(0.0) } else { Err(e) })?,
            );
            if o.kind == OrderType::TakeProfit {
                p.tp_price = Some(price);
                p.tp_stop_type = Some(stop(o.tp_trigger_type));
                if !full {
                    p.tp_qty = Some(qty);
                    p.tp_order_type = Some("MARKET".into());
                }
            } else {
                p.sl_price = Some(price);
                p.sl_stop_type = Some(stop(o.sl_trigger_type));
                if !full {
                    p.sl_qty = Some(qty);
                    p.sl_order_type = Some("MARKET".into());
                }
            }
            if full {
                for row in cx.store.orders().iter().filter(|r| {
                    r.symbol == o.symbol && r.status == OrderStatus::Open && r.kind != o.kind
                }) {
                    if self
                        .account
                        .native
                        .get(&row.id)
                        .is_some_and(|n| !n.attached && n.position_id == position_id)
                    {
                        if row.kind == OrderType::StopLoss {
                            p.sl_price = Some(num::format(row.price));
                            p.sl_stop_type = Some(stop(row.trigger_type));
                        } else if row.kind == OrderType::TakeProfit {
                            p.tp_price = Some(num::format(row.price));
                            p.tp_stop_type = Some(stop(row.trigger_type));
                        }
                    }
                }
            }
            return Ok(vec![(if full { FULL_TPSL } else { TPSL }, p)]);
        }
        let amount = positive(num::adjust(o.amount, m.precision.amount), "quantity")?;
        let reduce = o.reduce_only.unwrap_or(false);
        p.reduce_only = Some(reduce);
        p.side = Some(if o.side == OrderSide::Buy {
            "BUY"
        } else {
            "SELL"
        });
        p.trade_side = Some(if reduce { "CLOSE" } else { "OPEN" });
        if reduce {
            p.position_id = Some(self.resolve_position(cx, o, false)?);
        }
        p.order_type = Some(if o.kind == OrderType::Market {
            "MARKET"
        } else {
            "LIMIT"
        });
        p.effect = Some(effect(o.time_in_force));
        if o.kind != OrderType::Market {
            p.price = Some(num::format(positive(
                num::adjust(o.price.unwrap_or(0.0), m.precision.price),
                "price",
            )?));
        }
        if let Some(v) = o.take_profit {
            p.tp_price = Some(num::format(v));
            p.tp_stop_type = Some(stop(o.tp_trigger_type));
            p.tp_order_type = Some("MARKET".into());
        }
        if let Some(v) = o.stop_loss {
            p.sl_price = Some(num::format(v));
            p.sl_stop_type = Some(stop(o.sl_trigger_type));
            p.sl_order_type = Some("MARKET".into());
        }
        let sizes = split_quantities(
            amount,
            m.precision.amount,
            m.limits.amount.min,
            m.limits.amount.max,
        )?;
        if o.client_order_id.is_some() && sizes.len() != 1 {
            return Err("Bitunix clientOrderId cannot be split".into());
        }
        sizes
            .into_iter()
            .map(|qty| {
                positive(qty, "split quantity")?;
                let mut payload = p.clone();
                payload.qty = Some(num::format(qty));
                payload.client_id = Some(if let Some(id) = &o.client_order_id {
                    id.clone()
                } else {
                    format!(
                        "ts_{}",
                        safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?
                    )
                });
                Ok((PLACE, payload))
            })
            .collect()
    }
    fn place_prepared(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: u32,
        orders: Vec<PlaceOrderOpts>,
        retried: bool,
    ) {
        let result = (|| {
            let mut plan: Vec<(&'static str, Payload)> = vec![];
            for o in &orders {
                for (path, p) in self.payloads(cx, o)? {
                    if path == FULL_TPSL
                        && let Some((_, existing)) = plan.iter_mut().find(|(kind, row)| {
                            *kind == FULL_TPSL
                                && row.symbol == p.symbol
                                && row.position_id == p.position_id
                        })
                    {
                        // Each payload carries an old sibling; merge only the requested leg.
                        if o.kind == OrderType::TakeProfit {
                            existing.tp_price = p.tp_price;
                            existing.tp_stop_type = p.tp_stop_type;
                        } else {
                            existing.sl_price = p.sl_price;
                            existing.sl_stop_type = p.sl_stop_type;
                        }
                    } else {
                        plan.push((path, p));
                    }
                }
            }
            let mut job = Job::default();
            if orders.len() == 1 && !retried {
                job.recover = Some(orders.clone());
            }
            for path in [FULL_TPSL, TPSL] {
                for (_, p) in plan.iter().filter(|(kind, _)| *kind == path) {
                    job.steps.push_back(Step {
                        path,
                        body: text(p),
                        call: Call::Place {
                            clients: vec![],
                            batch: false,
                        },
                        delay: 0.0,
                    });
                }
            }
            let mut grouped: Vec<(String, Vec<Payload>)> = vec![];
            for (_, p) in plan.into_iter().filter(|(path, _)| *path == PLACE) {
                let symbol = p.symbol.clone().unwrap();
                if let Some((_, rows)) = grouped.iter_mut().find(|(s, _)| *s == symbol) {
                    rows.push(p);
                } else {
                    grouped.push((symbol, vec![p]));
                }
            }
            for (symbol, payloads) in grouped {
                for (index, chunk) in payloads.chunks(5).enumerate() {
                    let clients: Vec<_> =
                        chunk.iter().filter_map(|p| p.client_id.clone()).collect();
                    for p in chunk {
                        if let Some(id) = &p.client_id {
                            self.account
                                .tifs
                                .insert(id.clone(), crate::account::tif(p.effect.unwrap_or("GTC")));
                            self.account
                                .reduces
                                .insert(id.clone(), p.reduce_only.unwrap_or(false));
                        }
                    }
                    let batch = chunk.len() > 1;
                    let body = if batch {
                        text(&Batch {
                            symbol: &symbol,
                            order_list: chunk
                                .iter()
                                .map(|p| {
                                    let mut p = p.clone();
                                    p.symbol = None;
                                    p
                                })
                                .collect(),
                        })
                    } else {
                        text(&chunk[0])
                    };
                    job.steps.push_back(Step {
                        path: if batch { BATCH } else { PLACE },
                        body,
                        call: Call::Place { clients, batch },
                        delay: if index > 0 { 1000.0 } else { 0.0 },
                    });
                }
            }
            if job.steps.len() != 1
                || !matches!(
                    job.steps.front().map(|s| &s.call),
                    Some(Call::Place { batch: false, .. })
                )
            {
                job.recover = None;
            }
            Ok::<_, String>(job)
        })();
        match result {
            Ok(mut job) => {
                for (index, order) in orders
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| matches!(o.kind, OrderType::Market | OrderType::Limit))
                {
                    let id = format!("creating:{command}:{index}");
                    cx.store.upsert_order(Order {
                        id: id.clone(),
                        parent_id: None,
                        status: OrderStatus::Creating,
                        symbol: order.symbol.clone(),
                        kind: order.kind,
                        side: order.side,
                        price: order.price.unwrap_or(0.0),
                        amount: order.amount,
                        filled: 0.0,
                        remaining: order.amount,
                        reduce_only: order.reduce_only.unwrap_or(false),
                        close: None,
                        client_order_id: order.client_order_id.clone(),
                        time_in_force: order.time_in_force,
                        last_update: cx.now,
                        trigger_type: None,
                        take_profit: order.take_profit,
                        stop_loss: order.stop_loss,
                        extra: None,
                    });
                    job.created.push(id);
                }
                self.account.generations[2] += 1;
                cx.store.invalidate_order_snapshots();
                let Some(live) = self.trading.live.get_mut(&command) else {
                    return Self::cleanup(cx, &job);
                };
                live.job = job;
                self.advance(cx, command);
            }
            Err(e) => self.refuse(cx, command, e),
        }
    }
    fn plan_cancel(&self, cx: &Cx<'_, Pending>, ids: &[String]) -> Result<Job, String> {
        let mut job = Job::default();
        let mut regular: Vec<(String, Vec<String>)> = vec![];
        let mut seen = HashSet::new();
        let selected: HashSet<_> = ids.iter().map(String::as_str).collect();
        let mut replacements = vec![];
        for id in ids {
            let o = cx.store.order(id).ok_or_else(|| {
                format!("Bitunix order {id} not found; cancellation requires a reconciled order")
            })?;
            let n = self
                .account
                .native
                .get(id)
                .ok_or_else(|| format!("Bitunix order {id} is not reconciled"))?;
            if !seen.insert(n.raw_id.clone()) {
                continue;
            }
            if n.attached {
                let parent = cx
                    .store
                    .order(&n.raw_id)
                    .ok_or("Bitunix attached parent missing")?;
                let keep_tp =
                    n.tp.is_some() && !selected.contains(format!("{}__tp", parent.id).as_str());
                let keep_sl =
                    n.sl.is_some() && !selected.contains(format!("{}__sl", parent.id).as_str());
                if !selected.contains(parent.id.as_str()) && (keep_tp || keep_sl) {
                    let mut metadata = n.clone();
                    if !keep_tp {
                        metadata.tp = None;
                        metadata.tp_execution.clear();
                    }
                    if !keep_sl {
                        metadata.sl = None;
                        metadata.sl_execution.clear();
                    }
                    if let Some(p) = self.replacement(cx, parent, &metadata)? {
                        replacements.push((parent.id.clone(), p));
                    }
                }
            }
            let standalone =
                matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit) && !n.attached;
            let affected: Vec<_> = cx
                .store
                .orders()
                .iter()
                .filter(|r| {
                    r.id == n.raw_id
                        || self
                            .account
                            .native
                            .get(&r.id)
                            .is_some_and(|m| m.raw_id == n.raw_id)
                })
                .map(|r| r.id.clone())
                .collect();
            if standalone {
                let p = Payload {
                    symbol: Some(o.symbol.clone()),
                    order_id: Some(n.raw_id.clone()),
                    ..Default::default()
                };
                job.steps.push_back(Step {
                    path: CANCEL_TPSL,
                    body: text(&p),
                    call: Call::Cancel {
                        ids: affected,
                        require_ack: false,
                    },
                    delay: 0.0,
                });
            } else {
                if let Some((_, rows)) = regular.iter_mut().find(|(s, _)| s == &o.symbol) {
                    rows.push(n.raw_id.clone());
                } else {
                    regular.push((o.symbol.clone(), vec![n.raw_id.clone()]));
                }
            }
            for id in affected_ids(cx, n) {
                if let Some(row) = cx.store.order(&id) {
                    job.optimistic.push(id);
                    job.restore.push(row.status);
                }
            }
        }
        for (symbol, ids) in regular {
            let affected: Vec<_> = ids
                .iter()
                .flat_map(|id| {
                    cx.store
                        .orders()
                        .iter()
                        .filter(|o| {
                            o.id == *id
                                || self
                                    .account
                                    .native
                                    .get(&o.id)
                                    .is_some_and(|n| n.raw_id == *id)
                        })
                        .map(|o| o.id.clone())
                })
                .collect();
            job.steps.push_back(Step {
                path: CANCEL,
                body: text(&Cancel {
                    symbol: &symbol,
                    order_list: ids.iter().map(|id| Identity { order_id: id }).collect(),
                }),
                call: Call::Cancel {
                    ids: affected,
                    require_ack: true,
                },
                delay: 0.0,
            });
        }
        for (parent, p) in replacements {
            job.steps.push_back(Step {
                path: "/api/v1/futures/trade/get_order_detail",
                body: String::new(),
                call: Call::ReconcileReplacement {
                    parent: parent.clone(),
                    id: parent,
                    leg: None,
                    payload: Box::new(p),
                },
                delay: 0.0,
            });
        }
        Ok(job)
    }
    fn replacement(
        &self,
        cx: &Cx<'_, Pending>,
        parent: &Order,
        n: &NativeOrder,
    ) -> Result<Option<Payload>, String> {
        if n.tp_execution == "LIMIT" || n.sl_execution == "LIMIT" {
            return Err("Bitunix cannot recreate attached LIMIT-execution protection".into());
        }
        let o = PlaceOrderOpts {
            symbol: parent.symbol.clone(),
            kind: parent.kind,
            side: parent.side,
            amount: parent.remaining,
            price: Some(parent.price),
            trigger_price: None,
            max_slippage_pct: None,
            stp_mode: None,
            bybit_bbo: None,
            bybit_slippage_tolerance_ticks: None,
            implied_volatility: None,
            stop_loss: n.sl,
            take_profit: n.tp,
            reduce_only: Some(parent.reduce_only),
            time_in_force: parent.time_in_force,
            trigger_type: None,
            tp_trigger_type: n.tp_stop,
            sl_trigger_type: n.sl_stop,
            client_order_id: None,
            close: None,
        };
        Ok(self.payloads(cx, &o)?.into_iter().next().map(|(_, p)| p))
    }
    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Job, String> {
        let o = cx
            .store
            .order(id)
            .ok_or_else(|| format!("Bitunix order {id} not found"))?;
        if o.status != OrderStatus::Open {
            return Err("Bitunix only pending orders can be amended".into());
        }
        let n = self
            .account
            .native
            .get(id)
            .ok_or("Bitunix order identity not reconciled")?;
        let m = cx.store.market(&o.symbol).ok_or("Bitunix market missing")?;
        let attached = n.attached;
        let leg = if matches!(o.kind, OrderType::TakeProfit | OrderType::StopLoss) {
            Some(if o.kind == OrderType::TakeProfit {
                "tp"
            } else {
                "sl"
            })
        } else {
            None
        };
        let full = n.full_position || o.amount == 0.0;
        if (attached || full) && amount.is_some() {
            return Err(
                "Bitunix full-position or attached TP/SL quantity cannot be amended".into(),
            );
        }
        let mut p = Payload::default();
        let mut fallback = None;
        let path = if !attached && leg.is_some() {
            if full {
                if n.position_id.is_empty() {
                    return Err("Missing positionId for Bitunix full-position amend".into());
                }
                p.symbol = Some(o.symbol.clone());
                p.position_id = Some(n.position_id.clone());
            } else {
                p.order_id = Some(n.raw_id.clone());
            }
            for kind in [OrderType::TakeProfit, OrderType::StopLoss] {
                let row = if kind == o.kind {
                    Some(o)
                } else {
                    cx.store
                        .orders()
                        .iter()
                        .filter(|r| {
                            r.symbol == o.symbol && r.kind == kind && r.status == OrderStatus::Open
                        })
                        .find(|r| {
                            self.account
                                .native
                                .get(&r.id)
                                .is_some_and(|meta| !meta.attached && meta.raw_id == n.raw_id)
                        })
                        .or_else(|| {
                            cx.store
                                .orders()
                                .iter()
                                .filter(|r| {
                                    r.symbol == o.symbol
                                        && r.kind == kind
                                        && r.status == OrderStatus::Open
                                })
                                .find(|r| {
                                    self.account.native.get(&r.id).is_some_and(|meta| {
                                        !meta.attached
                                            && !n.position_id.is_empty()
                                            && meta.position_id == n.position_id
                                    })
                                })
                        })
                };
                if let Some(row) = row {
                    let meta = self.account.native.get(&row.id).unwrap();
                    let tp = kind == OrderType::TakeProfit;
                    let v = positive(
                        num::adjust(
                            if row.id == id {
                                price.unwrap_or(row.price)
                            } else {
                                row.price
                            },
                            m.precision.price,
                        ),
                        "TP/SL price",
                    )?;
                    let qty = positive(
                        num::adjust(
                            if row.id == id {
                                amount.unwrap_or(row.amount)
                            } else {
                                row.amount
                            },
                            m.precision.amount,
                        ),
                        "TP/SL quantity",
                    );
                    append_protection(
                        &mut p,
                        tp,
                        v,
                        row.trigger_type,
                        (
                            if tp {
                                &meta.tp_execution
                            } else {
                                &meta.sl_execution
                            },
                            if tp { meta.tp_limit } else { meta.sl_limit },
                        ),
                        if full { None } else { Some(qty?) },
                        !full,
                    )?;
                }
            }
            if full { AMEND_FULL } else { AMEND_TPSL }
        } else {
            let parent = if attached {
                cx.store
                    .order(&n.raw_id)
                    .ok_or("Bitunix attached parent not found")?
            } else {
                o
            };
            if parent.kind != OrderType::Limit {
                return Err("Bitunix native amend requires a pending LIMIT order".into());
            }
            p.symbol = Some(parent.symbol.clone());
            p.order_id = Some(parent.id.clone());
            p.qty = Some(num::format(positive(
                num::adjust(amount.unwrap_or(parent.amount), m.precision.amount),
                "quantity",
            )?));
            if num::parse(p.qty.as_deref().unwrap()) <= parent.filled {
                return Err("Bitunix amended quantity must exceed filled quantity".into());
            }
            p.price = Some(num::format(positive(
                num::adjust(
                    if attached {
                        parent.price
                    } else {
                        price.unwrap_or(parent.price)
                    },
                    m.precision.price,
                ),
                "price",
            )?));
            for (tp, trigger, stop_type, execution, limit) in [
                (true, n.tp, n.tp_stop, &n.tp_execution, n.tp_limit),
                (false, n.sl, n.sl_stop, &n.sl_execution, n.sl_limit),
            ] {
                let value = if attached && leg == Some(if tp { "tp" } else { "sl" }) {
                    price
                } else {
                    trigger
                };
                if let Some(value) = value {
                    append_protection(
                        &mut p,
                        tp,
                        positive(num::adjust(value, m.precision.price), "protection trigger")?,
                        stop_type,
                        (execution, limit),
                        None,
                        true,
                    )?;
                }
            }
            if attached {
                if price.is_none() {
                    return Err("Bitunix attached amend requires trigger price".into());
                }
                if let Ok(Some(mut replace)) = self.replacement(cx, parent, n) {
                    replace.tp_price = p.tp_price.clone();
                    replace.sl_price = p.sl_price.clone();
                    fallback = Some(Box::new(replace));
                }
            }
            AMEND
        };
        Ok(Job {
            steps: VecDeque::from([Step {
                path,
                body: text(&p),
                call: Call::Amend {
                    id: id.into(),
                    parent: n.raw_id.clone(),
                    leg: if attached { leg } else { None },
                    fallback,
                },
                delay: 0.0,
            }]),
            ..Job::default()
        })
    }
    /// The serial Plan's next call: admitted at the first call, else continued
    /// from the delivery that decided it.
    fn next_slot(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: u32,
        next: SerialCall,
    ) -> Result<Slot, VenueError> {
        let Some((credential, _)) = self.trade_identity() else {
            return Err(error::not_sent(
                "Bitunix credentials required",
                ErrorKind::AuthInvalid,
            ));
        };
        let live = self.trading.live.get_mut(&command).expect("live job");
        match live.plan.clone() {
            None => {
                let policy = SerialPolicy {
                    credential: Some(credential),
                    pacing: None,
                    // Native fails a refused job; it never completes `done []`.
                    refusals_complete: false,
                };
                let (plan, slot) = cx
                    .admit_serial_plan(&live.preflight, next, policy)
                    .map_err(|reason| {
                        error::not_sent(reason.venue_error().message, ErrorKind::Internal)
                    })?;
                live.plan = Some(plan);
                Ok(slot)
            }
            Some(plan) => {
                let Some((slot, current)) = live.current.take() else {
                    return Err(VenueError::new("Bitunix job lost its continuation"));
                };
                cx.continue_serial_plan(&plan, &slot, &current, next)
                    .map_err(|reason| VenueError::new(reason.venue_error().message))
            }
        }
    }
    fn advance(&mut self, cx: &mut Cx<'_, Pending>, command: u32) {
        let Some(live) = self.trading.live.get_mut(&command) else {
            return;
        };
        let Some(mut step) = live.job.steps.pop_front() else {
            return self.finish(cx, command);
        };
        // Native's gap between batch chunks: the Plan timer issues the call.
        if step.delay > 0.0
            && let Some(plan) = live.plan.clone()
            && let Some((slot, current)) = live.current.take()
        {
            let delay = step.delay as u32;
            step.delay = 0.0;
            let next = serial(&step);
            live.waiting = Some(step);
            if let Err(reason) = cx.continue_serial_plan_after(&plan, &slot, &current, delay, next)
            {
                let error = VenueError::new(reason.venue_error().message);
                return self.end(cx, command, error);
            }
            return;
        }
        let slot = match self.next_slot(cx, command, serial(&step)) {
            Ok(slot) => slot,
            Err(error) => return self.end(cx, command, error),
        };
        self.issue(cx, command, slot, step, false);
    }
    /// A paced continuation's Plan timer fired: issue its waiting call.
    pub(crate) fn trading_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        let Some(command) = self.trading.live.iter().find_map(|(command, live)| {
            live.plan
                .as_ref()
                .filter(|own| own.owner() == plan.owner() && live.waiting.is_some())
                .map(|_| *command)
        }) else {
            return;
        };
        let slot = cx
            .plan_slots(plan)
            .ok()
            .and_then(|slots| slots.into_iter().next());
        let step = self
            .trading
            .live
            .get_mut(&command)
            .and_then(|live| live.waiting.take());
        match (slot, step) {
            (Some(slot), Some(step)) => self.issue(cx, command, slot, step, false),
            _ => self.end(
                cx,
                command,
                VenueError::new("Bitunix job lost its schedule"),
            ),
        }
    }
    /// Queues one Plan call; every call signs Primary.
    fn issue(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: u32,
        slot: Slot,
        step: Step,
        resent: bool,
    ) {
        let Some(live) = self.trading.live.get(&command) else {
            return;
        };
        let Some(plan) = live.plan.clone() else {
            return;
        };
        // An amend fallback's replacement may leave a live order on a lost
        // ACK: fence it as a placement, not as the amended order.
        let context = if live.amend && step.path == PLACE {
            cx.plan_leg_context_with_intent(&plan, &slot, Intent::Place)
        } else {
            cx.plan_leg_context(&plan, &slot)
        };
        let context = match context {
            Ok(context) => context,
            Err(reason) => {
                let error = VenueError::new(reason.venue_error().message);
                return self.end(cx, command, error);
            }
        };
        let Some((_, identity)) = self.trade_identity() else {
            let error = error::not_sent("Bitunix credentials required", ErrorKind::AuthInvalid);
            return self.end(cx, command, error);
        };
        if let Call::Cancel { ids, .. } = &step.call
            && !resent
        {
            self.account.generations[2] += 1;
            let live = self.trading.live.get_mut(&command).expect("live job");
            for id in ids {
                let Some(row) = cx.store.order(id) else {
                    continue;
                };
                if matches!(row.status, OrderStatus::Closed | OrderStatus::Canceled) {
                    continue;
                }
                if !live.job.optimistic.contains(id) {
                    live.job.optimistic.push(id.clone());
                    live.job.restore.push(row.status);
                }
                cx.store.set_order_status(id, OrderStatus::Canceling);
            }
        }
        let (method, params, body) = match &step.call {
            Call::ReconcileReplacement { parent, .. } => {
                (Method::Get, Params::default().str("orderId", parent), None)
            }
            _ => (Method::Post, Params::default(), Some(step.body.clone())),
        };
        if !step.read() {
            cx.store.invalidate_order_snapshots();
        }
        let query = params.query();
        let url = format!(
            "{}{}{}{}",
            crate::REST,
            step.path,
            if query.is_empty() { "" } else { "?" },
            query
        );
        let recipe = crate::sign::dispatch::Http::new(
            identity,
            method,
            url.clone(),
            params.signing(),
            body.clone(),
            self.options.testnet,
        );
        if step.setting()
            && let Some(live) = self.trading.live.get_mut(&command)
        {
            live.settings_released = true;
        }
        let pending = Pending::Trade(Request {
            command,
            step,
            slot,
            context: context.clone(),
            resent,
        });
        if let Err((pending, reason)) =
            cx.request_signed_managed(context, None, method, url, body, Box::new(recipe), pending)
            && let Pending::Trade(request) = pending
        {
            self.trade_error(cx, request, reason.venue_error());
        }
    }
    /// Restores the job's optimistic rows: Creating removed, Canceling reopened.
    fn cleanup(cx: &mut Cx<'_, Pending>, job: &Job) {
        cx.store.invalidate_order_snapshots();
        cx.store
            .remove_orders(&job.created.iter().map(String::as_str).collect::<Vec<_>>());
        for (id, status) in job.optimistic.iter().zip(&job.restore) {
            if cx
                .store
                .order(id)
                .is_some_and(|o| o.status == OrderStatus::Canceling)
            {
                cx.store.set_order_status(id, *status);
            }
        }
    }
    /// Ends a job's authority. A settings scope whose write may still execute
    /// stays held until a fresh read; any other ends with the job.
    fn take_live(&mut self, command: u32) -> Option<Live> {
        let live = self.trading.live.remove(&command)?;
        if let Some(key) = &live.settings {
            self.trading.settings.remove(key);
            if live.settings_released {
                self.trading.held.entry(key.clone()).or_insert(None);
            }
        }
        Some(live)
    }
    /// Issues the read that can release each hold not yet waiting on one.
    fn reread_held(&mut self, cx: &mut Cx<'_, Pending>) {
        let keys: Vec<_> = self
            .trading
            .held
            .iter()
            .filter(|(_, issued)| issued.is_none())
            .map(|(key, _)| key.clone())
            .collect();
        for key in keys {
            self.reread_settings(cx, &key);
        }
    }
    /// The symbol's settings read, or the account read for position mode.
    fn reread_settings(&mut self, cx: &mut Cx<'_, Pending>, key: &str) {
        let threshold = if let Some(symbol) = key.strip_prefix("symbol:") {
            let threshold = self.account.recovery.settings_sequence() + 1;
            self.account.symbol_settings.remove(symbol);
            self.read_symbol_settings(cx, symbol);
            threshold
        } else {
            let threshold = self.account.serials[0] + 1;
            self.read_axis(cx, Axis::Balance);
            threshold
        };
        self.trading.held.insert(key.to_owned(), Some(threshold));
    }
    /// Restores rows of jobs the engine sealed, at the first callback after.
    pub(crate) fn restore_retired(&mut self, cx: &mut Cx<'_, Pending>) {
        for job in std::mem::take(&mut self.trading.retired) {
            Self::cleanup(cx, &job);
        }
        self.reread_held(cx);
    }
    /// Fails the job under its owner; the journal projects the outcome.
    fn end(&mut self, cx: &mut Cx<'_, Pending>, command: u32, error: VenueError) {
        let Some(live) = self.take_live(command) else {
            return;
        };
        Self::cleanup(cx, &live.job);
        self.reread_held(cx);
        match &live.plan {
            None => {
                let failed = cx.fail_preflight(&live.preflight, error);
                debug_assert!(failed.is_ok(), "Preflight terminal stranded");
            }
            Some(plan) => {
                if let Some((slot, context)) = &live.current {
                    let _ = cx.finish_plan_call(plan, slot, context);
                }
                let failed = match &live.note {
                    Some(note) => cx.fail_plan_noted(plan, error, note),
                    None => cx.fail_plan(plan, error),
                };
                debug_assert!(failed.is_ok(), "Plan terminal stranded");
            }
        }
        self.debounce(cx, Axis::Orders);
    }
    fn finish(&mut self, cx: &mut Cx<'_, Pending>, command: u32) {
        if let Some(error) = self
            .trading
            .live
            .get_mut(&command)
            .and_then(|live| live.failure.take())
        {
            return self.end(cx, command, error);
        }
        let Some(live) = self.take_live(command) else {
            return;
        };
        Self::cleanup(cx, &live.job);
        let result = text(&live.job.ids);
        match &live.plan {
            // Every cancel target was already closed: nothing was sent.
            None => {
                let _ = cx.done_empty_plan(&live.preflight, result);
            }
            Some(plan) => {
                if let Some((slot, context)) = &live.current {
                    let _ = cx.finish_plan_call(plan, slot, context);
                }
                if cx.done_plan(plan, result).is_err() {
                    let error = VenueError::new("Bitunix job ended without its call");
                    let failed = cx.fail_plan(plan, error);
                    debug_assert!(failed.is_ok(), "Plan terminal stranded");
                }
            }
        }
    }
    /// The engine retired a trading owner itself (no Cx here); its rows and
    /// any settings read follow at the next callback.
    pub(crate) fn trading_retired(&mut self, owner: &HttpOwnerRetirement) {
        let commands: Vec<u32> = self
            .trading
            .live
            .iter()
            .filter(|(_, live)| owner.matches_preflight(&live.preflight))
            .map(|(command, _)| *command)
            .collect();
        for command in commands {
            if let Some(live) = self.take_live(command) {
                self.trading.retired.push(live.job);
            }
        }
    }
    /// Native restore for sealed jobs, then Primary order snapshots.
    pub(crate) fn reconcile_trading(&mut self, cx: &mut Cx<'_, Pending>) {
        self.restore_retired(cx);
        self.account.reconcile = true;
        self.account.reconciled = [false, false];
        self.read_axis(cx, Axis::Orders);
        self.read_axis(cx, Axis::Tpsl);
        self.account.reconcile = false;
    }
    /// A call failed in transport; a released write is Unknown.
    pub(crate) fn trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        message: &str,
    ) {
        let write = !request.step.read();
        if let Some(plan) = self.trading.plan(request.command).cloned() {
            let _ = cx.finish_plan_failed_call(&plan, &request.slot, &request.context);
        }
        // A lost in-job read leaves earlier facts selected; its text follows.
        if !write && let Some(live) = self.trading.live.get_mut(&request.command) {
            live.note = Some(message.to_owned());
        }
        self.end(cx, request.command, error::transport(message, write));
    }
    /// A call refused before release (signing): NotSent for this call.
    pub(crate) fn trade_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        // Never released: a settings scope ends with the job.
        if let Some(live) = self.trading.live.get_mut(&request.command) {
            live.settings_released = false;
        }
        // An unreleased call the engine already stopped leaves it the terminal.
        if let Some(plan) = self.trading.plan(request.command).cloned()
            && cx
                .finish_plan_failed_call(&plan, &request.slot, &request.context)
                .is_err()
        {
            if let Some(live) = self.take_live(request.command) {
                Self::cleanup(cx, &live.job);
            }
            return self.debounce(cx, Axis::Orders);
        }
        self.end(cx, request.command, error);
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let command = request.command;
        let Some(plan) = self.trading.plan(command).cloned() else {
            return;
        };
        if cx.validate_plan_delivery(&plan, &request.context).is_err() {
            return;
        }
        if request.step.read() {
            return self.replacement_answer(cx, request, response);
        }
        // Fence before parsing: a partial ACK can apply accepted effects and
        // then stop on a rejected or malformed sibling.
        cx.store.invalidate_order_snapshots();
        let data = match error::response_data(&response, true) {
            Ok(data) => data,
            Err(error) => return self.write_refused(cx, request, &response, error),
        };
        let accepted = match &request.step.call {
            Call::Place {
                clients,
                batch: false,
            } => self.place_answer(cx, &request, data, clients),
            Call::Place {
                clients,
                batch: true,
            } => self.batch_answer(cx, &request, data, clients.len()),
            Call::Cancel { ids, require_ack } => {
                self.cancel_answer(cx, &request, data, ids, *require_ack)
            }
            Call::Amend {
                id, parent, leg, ..
            } => self.amend_answer(cx, &request, data, id, parent, *leg),
            Call::Leverage { symbol, value } => {
                self.account.generations[Axis::Positions.index()] += 1;
                self.account.position_write_revision += 1;
                cx.store.update_setting(symbol, |s| s.leverage = *value);
                cx.store.update_positions(|p| {
                    if p.symbol == *symbol {
                        p.leverage = *value;
                    }
                });
                Ok(())
            }
            Call::Margin { symbol, isolated } => {
                self.account.generations[Axis::Positions.index()] += 1;
                self.account.position_write_revision += 1;
                let margin = if *isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                cx.store
                    .update_setting(symbol, |s| s.margin_type = Some(margin));
                cx.store.update_positions(|p| {
                    if p.symbol == *symbol {
                        p.margin_type = Some(margin);
                    }
                });
                Ok(())
            }
            Call::Mode { hedged } => {
                self.account.balance_write_revision += 1;
                self.account.generations[Axis::Balance.index()] += 1;
                self.account.generations[Axis::Positions.index()] += 1;
                self.account.position_write_revision += 1;
                cx.store.set_hedged(*hedged);
                self.debounce(cx, Axis::Positions);
                Ok(())
            }
            Call::ReconcileReplacement { .. } => unreachable!("a Read call"),
        };
        // Settings writes record their acceptance here; order writes did above.
        if accepted.is_ok()
            && matches!(
                request.step.call,
                Call::Leverage { .. } | Call::Margin { .. } | Call::Mode { .. }
            )
            && cx
                .http_write_fact(
                    &request.context,
                    WriteFact::Accepted {
                        order_ids: Vec::new(),
                    },
                    None,
                )
                .is_err()
        {
            return;
        }
        if request.step.setting()
            && let Some(live) = self.trading.live.get_mut(&command)
        {
            live.settings_released = false;
        }
        self.account.generations[2] += 1;
        self.debounce(cx, Axis::Orders);
        match accepted {
            Ok(()) => {
                if let Some(live) = self.trading.live.get_mut(&command) {
                    live.current = Some((request.slot, request.context));
                }
                self.advance(cx, command);
            }
            Err(Some(error)) => {
                if let Some(live) = self.trading.live.get_mut(&command) {
                    live.current = Some((request.slot, request.context));
                }
                self.end(cx, command, error);
            }
            // The journal refused the fact; the engine owns this job now.
            Err(None) => {}
        }
    }
    /// A refused write: the classified fact, then native's recoveries on a
    /// definitive refusal, else the job ends.
    fn write_refused(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: &Response<'_>,
        error: VenueError,
    ) {
        let command = request.command;
        // HTTP status owns classification; a native code decides only a 2xx.
        let code = error
            .venue_code
            .as_deref()
            .filter(|_| (200..300).contains(&response.status))
            .and_then(|code| code.parse::<i64>().ok());
        let native = code.map(|code| Native {
            code: NativeCode::Int(code),
            message: "",
        });
        let mut refused = false;
        let grant = match cx.classify_write(
            &request.context,
            &codes::BITUNIX,
            native.as_ref(),
            error.message.clone(),
        ) {
            Ok(fact) => {
                refused = fact.verdict().is_refusal();
                cx.http_write_fact(
                    &request.context,
                    WriteFact::Failed {
                        fact,
                        known_effects: false,
                    },
                    None,
                )
            }
            Err(_) => cx
                .http_write_unproven(&request.context, error.message.clone())
                .map(|()| None),
        };
        let grant = match grant {
            Ok(grant) => grant,
            Err(_) => return,
        };
        // A proven refusal settles a settings write; a resend re-releases it.
        if refused
            && grant.is_none()
            && let Some(live) = self.trading.live.get_mut(&command)
        {
            live.settings_released = false;
        }
        let plan = self.trading.plan(command).cloned().expect("delivered Plan");
        // 10007: the one re-signed resend, with a fresh nonce.
        if let Some(grant) = grant
            && !request.resent
        {
            return match cx.plan_resend_context(&plan, &request.slot, &request.context, grant) {
                Ok(context) => {
                    let request = Request { context, ..request };
                    self.issue_resent(cx, request)
                }
                Err(reason) => self.end(cx, command, VenueError::new(reason.venue_error().message)),
            };
        }
        let definitive = (200..300).contains(&response.status)
            && error.outcome == Some(Outcome::Rejected)
            && error.venue_code.is_some();
        let message = error.message.to_ascii_lowercase();
        if let Some(live) = self.trading.live.get_mut(&command) {
            live.current = Some((request.slot.clone(), request.context.clone()));
        }
        if definitive
            && message.contains("position does not exist")
            && request.step.single_place()
            && let Some(orders) = self
                .trading
                .live
                .get_mut(&command)
                .filter(|live| live.job.ids.is_empty())
                .and_then(|live| live.job.recover.take())
        {
            // Native `recover`: forget the stale position IDs, re-read and
            // re-place once, as continuations of this Plan.
            if let Some(live) = self.trading.live.get_mut(&command) {
                Self::cleanup(cx, &live.job);
                live.job.created.clear();
            }
            for o in &orders {
                self.account
                    .ws_position_ids
                    .remove(&format!("{}:long", o.symbol));
                self.account
                    .ws_position_ids
                    .remove(&format!("{}:short", o.symbol));
            }
            if let Some(live) = self.trading.live.get_mut(&command) {
                live.prepare = Some(Prepare {
                    orders,
                    stage: 0,
                    retried: true,
                });
            }
            return self.read_axis_for(cx, Axis::Positions, Some(command));
        }
        if definitive
            && message.contains("order does not support this operation")
            && let Call::Amend {
                fallback: Some(p),
                parent,
                id,
                leg,
            } = &request.step.call
        {
            // Native fallback: cancel, prove the cancel final, then replace.
            let symbol = p.symbol.clone().unwrap_or_default();
            let ids: Vec<_> = cx
                .store
                .orders()
                .iter()
                .filter(|o| {
                    o.id == *parent
                        || self
                            .account
                            .native
                            .get(&o.id)
                            .is_some_and(|n| n.raw_id == *parent)
                })
                .map(|o| o.id.clone())
                .collect();
            if let Some(live) = self.trading.live.get_mut(&command) {
                live.job.steps.push_front(Step {
                    path: "/api/v1/futures/trade/get_order_detail",
                    body: String::new(),
                    call: Call::ReconcileReplacement {
                        id: id.clone(),
                        parent: parent.clone(),
                        leg: *leg,
                        payload: p.clone(),
                    },
                    delay: 0.0,
                });
                live.job.steps.push_front(Step {
                    path: CANCEL,
                    body: text(&Cancel {
                        symbol: &symbol,
                        order_list: vec![Identity { order_id: parent }],
                    }),
                    call: Call::Cancel {
                        ids,
                        require_ack: true,
                    },
                    delay: 0.0,
                });
            }
            return self.advance(cx, command);
        }
        self.end(cx, command, error);
    }
    /// The same call bytes under the resend Context, re-signed.
    fn issue_resent(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let Request {
            command,
            step,
            slot,
            context,
            ..
        } = request;
        let Some((_, identity)) = self.trade_identity() else {
            let error = error::not_sent("Bitunix credentials required", ErrorKind::AuthInvalid);
            return self.end(cx, command, error);
        };
        let url = format!("{}{}", crate::REST, step.path);
        let body = Some(step.body.clone());
        let recipe = crate::sign::dispatch::Http::new(
            identity,
            Method::Post,
            url.clone(),
            String::new(),
            body.clone(),
            self.options.testnet,
        );
        let pending = Pending::Trade(Request {
            command,
            step,
            slot,
            context: context.clone(),
            resent: true,
        });
        if let Err((pending, reason)) = cx.request_signed_managed(
            context,
            None,
            Method::Post,
            url,
            body,
            Box::new(recipe),
            pending,
        ) && let Pending::Trade(request) = pending
        {
            self.trade_error(cx, request, reason.venue_error());
        }
    }
    /// Records an accepted or unproven write fact; `Err(None)` means the
    /// journal refused it and the engine owns the job.
    fn record(
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        accepted: Result<Vec<String>, String>,
    ) -> Result<(), Option<VenueError>> {
        match accepted {
            Ok(order_ids) => cx
                .http_write_fact(&request.context, WriteFact::Accepted { order_ids }, None)
                .map(drop)
                .map_err(|_| None),
            // A 2xx that does not prove the write is Unknown.
            Err(message) => match cx.http_write_unproven(&request.context, message.clone()) {
                Ok(()) => Err(Some(error::malformed(message, true))),
                Err(_) => Err(None),
            },
        }
    }
    fn place_answer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        data: &str,
        clients: &[String],
    ) -> Result<(), Option<VenueError>> {
        let accepted = match ack_identity(data, true) {
            Ok((Some(id), false)) => Ok(id),
            // code=0 proves acceptance; keep the client-ID identity fallback.
            Ok((None, false)) => clients
                .first()
                .cloned()
                .ok_or_else(|| "Bitunix place response has no order ID".to_owned()),
            Ok((_, true)) => Err("Malformed Bitunix order acknowledgement".to_owned()),
            Err(error) => Err(error),
        };
        if let (Ok(id), Some(live)) = (&accepted, self.trading.live.get_mut(&request.command)) {
            live.job.ids.push(id.clone());
        }
        Self::record(cx, request, accepted.map(|id| vec![id]))
    }
    /// A batch ACK: accepted rows ride the journal; a refused or malformed
    /// row ends the job with the accepted IDs kept.
    fn batch_answer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        data: &str,
        expected: usize,
    ) -> Result<(), Option<VenueError>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct BatchResult<'a> {
            #[serde(borrow, default = "empty_list")]
            success_list: &'a serde_json::value::RawValue,
            #[serde(borrow, default = "empty_list")]
            failure_list: &'a serde_json::value::RawValue,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Failure<'a> {
            #[serde(borrow, default)]
            error_msg: Str<'a>,
            #[serde(default, deserialize_with = "error::present_code")]
            error_code: Option<error::Code>,
        }
        let Ok(rows) = serde_json::from_str::<BatchResult<'_>>(data) else {
            return Self::record(cx, request, Err("Malformed Bitunix batch response".into()));
        };
        let (successes, mut malformed) = match ack_rows(rows.success_list) {
            Ok(rows) => (rows, false),
            Err(_) => (Vec::new(), true),
        };
        let rejected = ack_rows(rows.failure_list).unwrap_or_else(|_| {
            malformed = true;
            Vec::new()
        });
        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        // Decode independently: a malformed sibling cannot erase observed IDs,
        // and a missing one leaves the count short (Unknown).
        for raw in successes {
            match ack_identity(raw.get(), true) {
                Ok((Some(id), bad)) => {
                    malformed |= bad;
                    if seen.insert(id.clone()) {
                        ids.push(id);
                    }
                }
                _ => malformed = true,
            }
        }
        let failures: Vec<_> = rejected
            .iter()
            .filter_map(|raw| {
                let failure = serde_json::from_str::<Failure<'_>>(raw.get()).ok();
                malformed |= failure.is_none();
                failure
            })
            .collect();
        let messages: Vec<_> = failures
            .iter()
            .map(|f| {
                format!(
                    "{} ({})",
                    f.error_msg,
                    f.error_code.map_or(0, |code| code.0)
                )
            })
            .collect();
        let mut items: Vec<_> = ids.iter().map(|id| BatchItem::Accepted(id)).collect();
        items.extend(failures.iter().map(|f| {
            BatchItem::Native(Native {
                code: NativeCode::Int(f.error_code.map_or(0, |code| code.0)),
                message: &f.error_msg,
            })
        }));
        // An unreadable row proves nothing: the empty identity makes it Unknown.
        if malformed {
            items.push(BatchItem::Accepted(""));
        }
        let message = if messages.is_empty() {
            "Bitunix batch response has incomplete order acknowledgements".to_owned()
        } else {
            messages.join("; ")
        };
        if let Some(live) = self.trading.live.get_mut(&request.command) {
            live.job.ids.extend(ids.iter().cloned());
        }
        // Native: a refused chunk never holds back later chunks; the job fails
        // once they have gone out. An uncertain chunk ends the job here.
        match cx.http_write_batch(
            &request.context,
            &codes::BITUNIX,
            expected,
            &items,
            message,
            None,
        ) {
            Ok(WriteResult::Accepted { .. }) => Ok(()),
            Ok(WriteResult::Rejected(error) | WriteResult::KnownPartial(error)) => {
                if let Some(live) = self.trading.live.get_mut(&request.command) {
                    live.failure.get_or_insert(error);
                }
                Ok(())
            }
            Ok(WriteResult::PartialUnknown(error) | WriteResult::Failed(error)) => Err(Some(error)),
            Err(_) => Err(None),
        }
    }
    fn cancel_answer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        data: &str,
        ids: &[String],
        require_ack: bool,
    ) -> Result<(), Option<VenueError>> {
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase")]
        struct Item<'a> {
            #[serde(borrow, default, alias = "id")]
            order_id: Str<'a>,
            #[serde(borrow, default)]
            error_msg: Str<'a>,
            #[serde(default, deserialize_with = "error::present_code")]
            error_code: Option<error::Code>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Canceled<'a> {
            #[serde(borrow, default = "empty_list")]
            success_list: &'a serde_json::value::RawValue,
            #[serde(borrow, default = "empty_list")]
            failure_list: &'a serde_json::value::RawValue,
        }
        let ack = if data.trim() == "null" {
            Ok(Canceled {
                success_list: empty_list(),
                failure_list: empty_list(),
            })
        } else {
            serde_json::from_str::<Canceled<'_>>(data)
        };
        let Ok(ack) = ack else {
            return Self::record(cx, request, Err("Malformed Bitunix cancellation".into()));
        };
        let raw_id = |id: &String| {
            self.account
                .native
                .get(id)
                .map_or(id.as_str(), |n| n.raw_id.as_str())
                .to_owned()
        };
        let (successes, rows_valid) = match ack_rows(ack.success_list) {
            Ok(rows) => (rows, true),
            Err(_) => (Vec::new(), false),
        };
        let (rejected, failures_valid) = match ack_rows(ack.failure_list) {
            Ok(rows) => (rows, true),
            Err(_) => (Vec::new(), false),
        };
        let succeeded: HashSet<String> = successes
            .iter()
            .filter_map(|raw| match ack_identity(raw.get(), false) {
                Ok((Some(id), false)) if serde_json::from_str::<Item<'_>>(raw.get()).is_ok() => {
                    Some(id)
                }
                _ => None,
            })
            .collect();
        let failures: Vec<_> = rejected
            .iter()
            .filter_map(|raw| serde_json::from_str::<Item<'_>>(raw.get()).ok())
            .collect();
        let malformed = !rows_valid
            || !failures_valid
            || successes.len() != succeeded.len()
            || failures.len() != rejected.len();
        // Native: a clean code-0 answer cancels every target unless an ACK is
        // required; otherwise only the echoed IDs are proven.
        let all = !require_ack && !malformed && rejected.is_empty();
        let mut canceled = Vec::new();
        for id in ids {
            if all || succeeded.contains(&raw_id(id)) {
                canceled.push(id.clone());
                cx.store.update_order(id, |row| {
                    if !matches!(row.status, OrderStatus::Closed | OrderStatus::Canceled) {
                        row.status = OrderStatus::Canceled;
                    }
                });
            }
        }
        if all {
            return Self::record(cx, request, Ok(canceled));
        }
        let raws: Vec<_> = canceled.iter().map(raw_id).collect();
        let mut items: Vec<_> = raws.iter().map(|id| BatchItem::Accepted(id)).collect();
        items.extend(failures.iter().map(|f| {
            BatchItem::Native(Native {
                code: NativeCode::Int(f.error_code.map_or(0, |code| code.0)),
                message: &f.error_msg,
            })
        }));
        if malformed {
            items.push(BatchItem::Accepted(""));
        }
        let message = failures
            .iter()
            .map(|f| {
                if f.error_msg.is_empty() {
                    format!("Bitunix cancel failed for {}", f.order_id)
                } else {
                    f.error_msg.to_string()
                }
            })
            .collect::<Vec<_>>();
        let message = if require_ack && canceled.len() != ids.len() {
            "Bitunix cancellation not acknowledged for every order; replacement was not placed"
                .to_owned()
        } else if message.is_empty() {
            "Malformed Bitunix cancellation acknowledgement".to_owned()
        } else {
            message.join("; ")
        };
        match cx.http_write_batch(
            &request.context,
            &codes::BITUNIX,
            ids.len(),
            &items,
            message,
            None,
        ) {
            Ok(WriteResult::Accepted { .. }) => Ok(()),
            Ok(
                WriteResult::Rejected(error)
                | WriteResult::KnownPartial(error)
                | WriteResult::PartialUnknown(error)
                | WriteResult::Failed(error),
            ) => Err(Some(error)),
            Err(_) => Err(None),
        }
    }
    fn amend_answer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        data: &str,
        id: &str,
        parent: &str,
        leg: Option<&'static str>,
    ) -> Result<(), Option<VenueError>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Ack<'a> {
            #[serde(borrow, default, alias = "id")]
            order_id: Str<'a>,
        }
        let accepted = match serde_json::from_str::<Ack<'_>>(data) {
            Ok(ack) => {
                let raw = if ack.order_id.is_empty() {
                    parent
                } else {
                    ack.order_id.as_ref()
                };
                Ok(if let Some(leg) = leg {
                    format!("{raw}__{leg}")
                } else if id == parent {
                    raw.to_owned()
                } else {
                    id.to_owned()
                })
            }
            Err(error) => Err(error.to_string()),
        };
        if let (Ok(id), Some(live)) = (&accepted, self.trading.live.get_mut(&request.command)) {
            live.job.ids.push(id.clone());
        }
        Self::record(cx, request, accepted.map(|id| vec![id]))
    }
    /// The amend fallback's Read: the canceled parent must be final before
    /// its remainder is re-placed.
    fn replacement_answer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let command = request.command;
        let Call::ReconcileReplacement {
            parent,
            id,
            leg,
            payload,
        } = &request.step.call
        else {
            return;
        };
        let result = (|| {
            let body = error::response_data(&response, false).map_err(|e| e.message)?;
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct FinalOrder<'a> {
                #[serde(borrow)]
                order_id: Str<'a>,
                #[serde(borrow)]
                symbol: Str<'a>,
                #[serde(borrow, alias = "orderStatus")]
                status: Str<'a>,
                qty: crate::wire::StrictNum,
                trade_qty: crate::wire::StrictNum,
            }
            let row: FinalOrder<'_> = serde_json::from_str(body).map_err(|e| e.to_string())?;
            if row.order_id != parent.as_str()
                || row.symbol != payload.symbol.as_deref().unwrap_or("")
            {
                return Err("Bitunix canceled order identity is not reconciled".to_owned());
            }
            let status = row.status.to_ascii_uppercase();
            if !matches!(status.as_str(), "CANCELED" | "CANCELLED" | "FILLED") {
                return Err("Bitunix cancel not final; replacement was not placed".into());
            }
            let m = cx
                .store
                .market(&row.symbol)
                .ok_or("Bitunix market missing")?;
            if status == "FILLED" {
                return Ok(None);
            }
            if row.qty.0 <= 0.0
                || row.trade_qty.0 < 0.0
                || row.trade_qty.0 > row.qty.0
                || self
                    .account
                    .filled
                    .get(parent)
                    .is_some_and(|(filled, _)| row.trade_qty.0 < *filled)
            {
                return Err("Bitunix canceled fill total is stale or invalid".into());
            }
            let remaining = num::adjust(row.qty.0 - row.trade_qty.0, m.precision.amount);
            if !remaining.is_finite() || remaining < 0.0 {
                return Err("Bitunix canceled quantity is not reconciled".into());
            }
            if remaining == 0.0 {
                return Ok(None);
            }
            let mut p = (**payload).clone();
            p.qty = Some(num::format(remaining));
            Ok(Some(Step {
                path: PLACE,
                body: text(&p),
                call: if leg.is_some() {
                    Call::Amend {
                        id: id.clone(),
                        parent: parent.clone(),
                        leg: *leg,
                        fallback: None,
                    }
                } else {
                    Call::Place {
                        clients: p.client_id.into_iter().collect(),
                        batch: false,
                    }
                },
                delay: 0.0,
            }))
        })();
        let Some(live) = self.trading.live.get_mut(&command) else {
            return;
        };
        live.current = Some((request.slot, request.context));
        match result {
            Ok(replacement) => {
                if let Some(step) = replacement {
                    live.job.steps.push_front(step);
                }
                self.advance(cx, command);
            }
            Err(message) => self.end(cx, command, VenueError::new(message)),
        }
    }
}
fn serial(step: &Step) -> SerialCall {
    let read = step.read();
    SerialCall {
        operation: if read {
            Operation::Read
        } else {
            Operation::Mutation
        },
        timeout_ms: CALL_MS,
        // A write's one 10007 RefusedResend.
        attempts: if read { 1 } else { 2 },
    }
}
fn local_mutation_policy() -> Policy {
    Policy {
        operation: Operation::Mutation,
        credential: None,
        timeout_ms: CALL_MS,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
fn affected_ids(cx: &Cx<'_, Pending>, n: &NativeOrder) -> Vec<String> {
    cx.store
        .orders()
        .iter()
        .filter(|o| o.id == n.raw_id || o.id.starts_with(&format!("{}__", n.raw_id)))
        .map(|o| o.id.clone())
        .collect()
}
fn append_protection(
    p: &mut Payload,
    tp: bool,
    price: f64,
    trigger: Option<StopTriggerType>,
    execution: (&str, f64),
    qty: Option<f64>,
    with_execution: bool,
) -> Result<(), String> {
    let (execution, limit) = execution;
    let execution = if execution.is_empty() {
        "MARKET"
    } else {
        execution
    };
    if !matches!(execution, "MARKET" | "LIMIT") {
        return Err("Bitunix protection execution type is not reconciled".into());
    }
    let limit = if with_execution && execution == "LIMIT" {
        Some(num::format(positive(limit, "protection LIMIT price")?))
    } else {
        None
    };
    if tp {
        p.tp_price = Some(num::format(price));
        p.tp_stop_type = Some(stop(trigger));
        p.tp_qty = qty.map(num::format);
        if with_execution {
            p.tp_order_type = Some(execution.into());
            p.tp_order_price = limit;
        }
    } else {
        p.sl_price = Some(num::format(price));
        p.sl_stop_type = Some(stop(trigger));
        p.sl_qty = qty.map(num::format);
        if with_execution {
            p.sl_order_type = Some(execution.into());
            p.sl_order_price = limit;
        }
    }
    Ok(())
}

#[cfg(test)]
mod split_tests {
    use super::split_quantities;

    #[test]
    fn native_ticks_handle_awkward_limits_minimums_and_exact_split_count() {
        assert_eq!(
            split_quantities(2.0, 0.1, 0.1, 0.95).unwrap(),
            [0.9, 0.9, 0.2]
        );
        assert_eq!(
            split_quantities(1.0, 0.1, 0.1, 0.3).unwrap(),
            [0.3, 0.3, 0.3, 0.1]
        );
        assert!(split_quantities(1.25, 0.25, 0.5, 1.0).is_err());
        assert_eq!(
            split_quantities(9_999.5, 0.1, 0.1, 1.0).unwrap().len(),
            10_000
        );
        assert_eq!(
            split_quantities(10_000.0, 0.1, 0.1, 1.0).unwrap().len(),
            10_000
        );
        assert!(split_quantities(10_000.1, 0.1, 0.1, 1.0).is_err());
        assert!(split_quantities(1e16, 1.0, 1.0, 1e16).is_err());
        assert!(split_quantities(1.0, 0.0, 0.0, 1.0).is_err());
        assert!(split_quantities(1.0, 0.25, 0.0, 0.1).is_err());
    }
}
