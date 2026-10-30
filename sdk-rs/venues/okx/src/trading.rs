//! Each command is one Preflight and one streamed serial Plan: ordered,
//! non-retried writes, each call bounded on its own.
use crate::{Okx, Pending, sign, wire};
use safe_cex_engine::classify::{
    BatchItem, CodeTable, Key, Native, NativeCode, Row, Verdict, WriteFact,
};
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::plan::{SerialCall, SerialPolicy, Slot};
use safe_cex_engine::request_context::{
    Context, Credential, HttpOwnerRetirement, Operation, Plan, Policy, Preflight, PreflightPolicy,
    PreflightPurpose, ReconciliationData, RetryPolicy, WriteResult,
};
use safe_cex_engine::setting_holds::SettingKey;
use safe_cex_engine::{
    Command, CommandId, CredentialSlot, Cx, ErrorKind, Method, Outcome, Response, VenueError, num,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

const BROKER: &str = "e847386590ce4dBC";

/// Every write is one call with its own 20 s bound; only a 50113 signature
/// refusal, which executed nothing, is re-signed and sent once more.
const SERIAL: SerialCall = SerialCall {
    auth: safe_cex_engine::request_context::plan::CallAuth::Preflight,
    operation: Operation::Mutation,
    timeout_ms: 20_000,
    attempts: 2,
    pacing: None,
};

/// OKX's native write codes. 51xxx are trading-rule refusals, and the listed
/// 50xxx are documented pre-processing refusals. Unavailable, timeout, busy and
/// system errors prove nothing, and an unlisted code is not proof of refusal.
pub(crate) const CODES: CodeTable = CodeTable {
    // Rows match first to last, so the exact codes below win over the 51xxx range.
    rows: &[
        refused(51000, ErrorKind::InvalidInput),
        // Balance and margin refusals (place, amend, and account-wide).
        refused(51008, ErrorKind::InsufficientBalance),
        refused(51119, ErrorKind::InsufficientBalance),
        refused(51127, ErrorKind::InsufficientBalance),
        refused(51131, ErrorKind::InsufficientBalance),
        refused(51502, ErrorKind::InsufficientBalance),
        refused(59103, ErrorKind::InsufficientBalance),
        refused(59108, ErrorKind::InsufficientBalance),
        refused(59200, ErrorKind::InsufficientBalance),
        // Order rules the order itself broke: unknown instrument, price out of
        // its limit, size below the minimum or off the lot size, market order
        // value or amount over its cap.
        refused(51001, ErrorKind::InvalidInput),
        refused(51006, ErrorKind::InvalidInput),
        refused(51007, ErrorKind::InvalidInput),
        refused(51020, ErrorKind::InvalidInput),
        refused(51100, ErrorKind::InvalidInput),
        refused(51116, ErrorKind::InvalidInput),
        refused(51118, ErrorKind::InvalidInput),
        refused(51120, ErrorKind::InvalidInput),
        refused(51121, ErrorKind::InvalidInput),
        refused(51122, ErrorKind::InvalidInput),
        refused(51137, ErrorKind::InvalidInput),
        refused(51138, ErrorKind::InvalidInput),
        refused(51201, ErrorKind::InvalidInput),
        refused(51202, ErrorKind::InvalidInput),
        refused(51203, ErrorKind::InvalidInput),
        Row {
            key: Key::Range(51000, 51999),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        refused(50014, ErrorKind::InvalidInput),
        refused(50016, ErrorKind::InvalidInput),
        refused(50011, ErrorKind::RateLimited),
        refused(50061, ErrorKind::RateLimited),
        // The read side's documented key refusals (docs-v5 "Error Code", 501xx),
        // pinned equal by account::write_and_read_key_refusals_agree.
        refused(50101, ErrorKind::AuthInvalid),
        refused(50103, ErrorKind::AuthInvalid),
        refused(50104, ErrorKind::AuthInvalid),
        refused(50105, ErrorKind::AuthInvalid),
        refused(50111, ErrorKind::AuthInvalid),
        Row {
            key: Key::Int(50113),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        },
        refused(50114, ErrorKind::AuthInvalid),
        refused(50119, ErrorKind::AuthInvalid),
        refused(50030, ErrorKind::AuthPermission),
        refused(50100, ErrorKind::AuthPermission),
        refused(50110, ErrorKind::AuthPermission),
        refused(50120, ErrorKind::AuthPermission),
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[
        (401, Verdict::Refused(ErrorKind::AuthInvalid)),
        (403, Verdict::Refused(ErrorKind::AuthPermission)),
        (429, Verdict::Refused(ErrorKind::RateLimited)),
    ],
};
const fn refused(code: i64, kind: ErrorKind) -> Row {
    Row {
        key: Key::Int(code),
        verdict: Verdict::Refused(kind),
        only: None,
    }
}
pub(crate) fn native<'a>(code: &'a str, message: &'a str) -> Native<'a> {
    Native {
        code: code.parse().map_or(NativeCode::Text(code), NativeCode::Int),
        message,
    }
}

/// One released Plan call, bound to its exact slot and leg.
pub struct Request {
    command: CommandId,
    slot: Slot,
    context: Context,
    path: &'static str,
}
struct Step {
    receipts: Vec<OrderIdentityReceipt>,
    path: &'static str,
    body: String,
    expected: usize,
    collect_ids: bool,
}
#[derive(Default)]
pub struct TradingState {
    jobs: HashMap<CommandId, Job>,
    pub acknowledged: HashMap<String, Acknowledged>,
}
pub struct Acknowledged {
    symbol: String,
    inst_id: String,
    algo: bool,
    pub version: u64,
}
struct Job {
    /// Set at genuine entry; the Plan follows its admission.
    preflight: Option<Preflight>,
    plan: Option<Plan>,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    finish: Finish,
    optimistic: Vec<(String, OrderStatus)>,
    continue_cancel_errors: bool,
    /// A step took effect, so a later rejection cannot mean nothing happened.
    effective: bool,
}
enum Finish {
    Ids,
    Amend(String),
    Empty,
    Leverage(String, f64),
    Mode(bool),
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OrderPayload {
    #[serde(skip)]
    symbol: String,
    inst_id: String,
    td_mode: &'static str,
    tag: &'static str,
    side: OrderSide,
    ord_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pos_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cl_ord_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    algo_cl_ord_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sz: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    close_fraction: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_px_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_ord_px: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_px_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_ord_px: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_px_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_px: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_ratio: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_spread: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cancel<'a> {
    inst_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ord_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    algo_id: Option<&'a str>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ack<'a> {
    #[serde(default)]
    ord_id: &'a str,
    #[serde(default)]
    algo_id: &'a str,
    #[serde(default)]
    s_code: &'a str,
    #[serde(default)]
    s_msg: &'a str,
}

impl Okx {
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let (id, result) = match command {
            Command::PlaceOrders { command, orders } => (command, self.plan_place(cx, orders)),
            Command::CancelOrders { command, ids } => (command, self.plan_cancel(cx, ids)),
            Command::CancelSymbolOrders { command, symbol } => {
                let mut ids: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        o.symbol == symbol
                            && matches!(
                                o.status,
                                OrderStatus::Open | OrderStatus::Creating | OrderStatus::Updating
                            )
                    })
                    .map(|o| o.id.clone())
                    .collect();
                ids.extend(
                    self.trading
                        .acknowledged
                        .iter()
                        .filter(|(_, order)| order.symbol == symbol)
                        .map(|(id, _)| id.clone()),
                );
                (command, self.plan_cancel(cx, ids))
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (command, self.plan_amend(cx, &id, price, amount)),
            // The engine binds the command when the read is admitted.
            Command::FetchLeverage { command: _, symbol } => {
                self.fetch_leverage(cx, symbol);
                return;
            }
            Command::SetMarginType { .. } => {
                self.refuse_entry(
                    cx,
                    not_sent("Only cross mode is supported").kind(ErrorKind::Unsupported),
                );
                return;
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Leverage<'a> {
                    inst_id: &'a str,
                    lever: String,
                    mgn_mode: &'static str,
                }
                let result = (|| {
                    let m = cx
                        .store
                        .market(&symbol)
                        .ok_or_else(|| format!("unknown market {symbol}"))?;
                    if !leverage.is_finite() {
                        return Err("leverage must be finite".into());
                    }
                    let leverage = num::clamp_leverage(
                        leverage,
                        m.limits.leverage.min,
                        m.limits.leverage.max,
                    )?;
                    let step = step(
                        "/api/v5/account/set-leverage",
                        &Leverage {
                            inst_id: &m.id,
                            lever: num::format(leverage),
                            mgn_mode: "cross",
                        },
                        0,
                        false,
                    )?;
                    Ok(job(vec![step], Finish::Leverage(symbol, leverage)))
                })();
                (command, result)
            }
            Command::SetPositionMode {
                command,
                hedged,
                symbol: _,
            } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Mode {
                    pos_mode: &'static str,
                }
                let result = if cx.store.positions().iter().any(|p| p.contracts > 0.0) {
                    Err("Please close all positions before switching position mode".into())
                } else {
                    step(
                        "/api/v5/account/set-position-mode",
                        &Mode {
                            pos_mode: if hedged {
                                "long_short_mode"
                            } else {
                                "net_mode"
                            },
                        },
                        0,
                        false,
                    )
                    .map(|s| job(vec![s], Finish::Mode(hedged)))
                };
                (command, result)
            }
            _ => return,
        };
        if self.credentials.is_none() || !self.account.ready {
            self.refuse_entry(cx, not_sent("OKX account is not ready"));
            return;
        }
        match result {
            Ok(job) => self.start_job(cx, id, job),
            Err(error) => self.refuse_entry(cx, not_sent(error).kind(ErrorKind::InvalidInput)),
        }
    }

    fn primary_credential(&self) -> Option<Credential> {
        self.credentials.as_ref().map(|identity| Credential {
            slot: CredentialSlot::Primary,
            reference: identity.credential.clone(),
        })
    }

    /// Genuine entry, then one serial Plan per command. Native optimistic
    /// status follows a successful admission, never a refused one.
    fn start_job(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, mut job: Job) {
        let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: None,
            write_credential: self.primary_credential(),
        }) else {
            return;
        };
        if job.steps.is_empty() {
            let _ = match self.finish_effects(cx, &mut job) {
                Ok(data) => cx.done_empty_plan(&preflight, data),
                Err(error) => cx.fail_preflight(&preflight, VenueError::new(error)),
            };
            return;
        }
        let Some(credential) = self.primary_credential() else {
            let _ = cx.fail_preflight(&preflight, not_sent("OKX account is not ready"));
            return;
        };
        let policy = SerialPolicy {
            credential: Some(credential),
            pacing: None,
            refusals_complete: true,
        };
        let (plan, slot) = match cx.admit_serial_plan(&preflight, SERIAL, policy) {
            Ok(admitted) => admitted,
            Err(reason) => {
                let _ = cx.fail_preflight(&preflight, not_sent(reason.venue_error().message));
                return;
            }
        };
        for (order, status) in &mut job.optimistic {
            if let Some(current) = cx.store.order(order) {
                *status = current.status;
            }
            cx.store.set_order_status(
                order,
                if matches!(job.finish, Finish::Amend(_)) {
                    OrderStatus::Updating
                } else {
                    OrderStatus::Canceling
                },
            );
        }
        job.preflight = Some(preflight);
        job.plan = Some(plan);
        self.trading.jobs.insert(command, job);
        self.issue(cx, command, slot);
    }

    /// A write refused before I/O admits a terminal-only Mutation owner, so the
    /// engine caps it at NotSent. A rejected admission already failed the command.
    fn refuse_entry(&self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        let credential = self.credentials.as_ref().map(|identity| Credential {
            slot: CredentialSlot::Primary,
            reference: identity.credential.clone(),
        });
        if let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Mutation,
            credential,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) {
            let _ = cx.fail_context(&context, error);
        }
    }

    fn plan_place(&self, cx: &Cx<'_, Pending>, orders: Vec<PlaceOrderOpts>) -> Result<Job, String> {
        if orders.iter().any(|order| {
            order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some()
        }) {
            return Err("Bybit execution controls are supported only on Bybit".into());
        }
        if orders
            .iter()
            .any(|order| order.implied_volatility.is_some())
        {
            return Err("Implied volatility is supported only for options".into());
        }
        let mut seen = HashSet::new();
        for o in &orders {
            let m = cx
                .store
                .market(&o.symbol)
                .ok_or_else(|| format!("Market {} not found on OKX", o.symbol))?;
            if let Some(id) = &o.client_order_id {
                if id.is_empty()
                    || id.len() > 32
                    || !id.bytes().all(|c| c.is_ascii_alphanumeric())
                    || !seen.insert(id)
                {
                    return Err(format!("Invalid or duplicated OKX client order ID {id}"));
                }
                if !matches!(o.kind, OrderType::Limit | OrderType::Market)
                    || o.trigger_price.is_some()
                    || o.stop_loss.is_some()
                    || o.take_profit.is_some()
                    || o.amount > m.limits.amount.max
                {
                    return Err(
                        "Caller client order ID requires exactly one plain native order".into(),
                    );
                }
            }
        }
        let mut regular = Vec::new();
        let mut brackets = Vec::new();
        let mut algos = Vec::new();
        for o in orders {
            if is_algo(o.kind) {
                algos.push(o);
                continue;
            }
            let m = cx.store.market(&o.symbol).expect("preflight market");
            if !m.limits.amount.max.is_finite()
                || m.limits.amount.max <= 0.0
                || m.precision.amount <= 0.0
            {
                return Err("invalid market size limits".into());
            }
            let amount = adjusted(o.amount, m.precision.amount, false)?;
            let max = lot_floor(m.limits.amount.max, m.precision.amount)?;
            if o.client_order_id.is_some() && amount > max {
                return Err(
                    "Caller client order ID requires exactly one plain native order".into(),
                );
            }
            if max <= 0.0 || amount / max > 10_000.0 {
                return Err("order exceeds safe split bounds".into());
            }
            let bracket =
                o.stop_loss.is_some_and(|v| v != 0.0) || o.take_profit.is_some_and(|v| v != 0.0);
            let mut remaining = amount;
            let mut count = 0;
            while remaining > 0.0 {
                let size = remaining.min(max);
                let payload = self.order_payload(cx, &o, size)?;
                count += 1;
                if bracket {
                    // Protect each accepted lot before another entry can fail.
                    brackets.push(placement_step(
                        "/api/v5/trade/batch-orders",
                        &[payload],
                        1,
                        true,
                    )?);
                    for (kind, price) in [
                        (OrderType::StopLoss, o.stop_loss),
                        (OrderType::TakeProfit, o.take_profit),
                    ] {
                        if let Some(price) = price.filter(|v| *v != 0.0) {
                            let mut leg = o.clone();
                            leg.kind = kind;
                            leg.side = o.side.opposite();
                            leg.amount = size;
                            leg.price = Some(price);
                            leg.client_order_id = None;
                            leg.stop_loss = None;
                            leg.take_profit = None;
                            leg.reduce_only = Some(true);
                            let payload = self.order_payload(cx, &leg, size)?;
                            brackets.push(placement_step(
                                "/api/v5/trade/order-algo",
                                &payload,
                                1,
                                true,
                            )?);
                        }
                    }
                } else {
                    regular.push(payload);
                }
                remaining = num::adjust(remaining - size, m.precision.amount);
            }
            if o.client_order_id.is_some() && count != 1 {
                return Err(
                    "Caller client order ID requires exactly one plain native order".into(),
                );
            }
        }
        algos.sort_by_key(|o| match o.kind {
            OrderType::StopLoss => 0,
            OrderType::TrailingStopLoss => 1,
            OrderType::TakeProfit => 2,
            _ => 3,
        });
        // Build every payload before issuing a write, including protective legs.
        let algos = algos
            .into_iter()
            .map(|o| self.order_payload(cx, &o, o.amount))
            .collect::<Result<Vec<_>, _>>()?;
        let mut steps = Vec::new();
        for batch in regular.chunks(20) {
            steps.push(placement_step(
                "/api/v5/trade/batch-orders",
                batch,
                batch.len(),
                true,
            )?);
        }
        steps.extend(brackets);
        for payload in algos {
            steps.push(placement_step(
                "/api/v5/trade/order-algo",
                &payload,
                1,
                true,
            )?);
        }
        Ok(job(steps, Finish::Ids))
    }

    fn order_payload(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        amount: f64,
    ) -> Result<OrderPayload, String> {
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| format!("unknown market {}", o.symbol))?;
        let algo = is_algo(o.kind);
        let size = adjusted(
            amount,
            m.precision.amount,
            algo && o.kind != OrderType::Conditional,
        )?;
        let protective = matches!(
            o.kind,
            OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
        );
        let closing =
            protective || (o.kind != OrderType::Conditional && o.reduce_only.unwrap_or(false));
        let pos_side = if cx.store.hedged() {
            Some(if (o.side == OrderSide::Buy) != closing {
                "long"
            } else {
                "short"
            })
        } else {
            None
        };
        let mut p = OrderPayload {
            symbol: o.symbol.clone(),
            inst_id: m.id.clone(),
            td_mode: "cross",
            tag: BROKER,
            side: o.side,
            ord_type: match o.kind {
                OrderType::Limit if o.time_in_force == Some(TimeInForce::PostOnly) => "post_only",
                OrderType::Limit => "limit",
                OrderType::Market => "market",
                OrderType::Conditional => "trigger",
                OrderType::TrailingStopLoss => "move_order_stop",
                _ => "conditional",
            },
            pos_side,
            ..empty_payload()
        };
        let generated = || {
            safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))
                .map(|id| format!("{BROKER}{id}").chars().take(32).collect::<String>())
        };
        if algo {
            p.algo_cl_ord_id = Some(generated()?);
        } else {
            p.cl_ord_id = Some(match &o.client_order_id {
                Some(id) => id.clone(),
                None => generated()?,
            });
        }
        if !algo {
            p.sz = Some(num::format(size));
            p.reduce_only = o.reduce_only.filter(|b| *b);
            if o.kind == OrderType::Limit {
                p.px = Some(num::format(adjusted(
                    o.price.unwrap_or(0.0),
                    m.precision.price,
                    false,
                )?));
            }
        }
        match o.kind {
            OrderType::Conditional => {
                p.sz = Some(num::format(size));
                p.trigger_px = Some(num::format(adjusted(
                    o.trigger_price.unwrap_or(0.0),
                    m.precision.price,
                    false,
                )?));
                // Like TS, a zero price means market execution once triggered.
                p.order_px = Some(if let Some(price) = o.price.filter(|p| *p != 0.0) {
                    num::format(adjusted(price, m.precision.price, false)?)
                } else {
                    "-1".into()
                });
                p.trigger_px_type = Some(trigger_name(o.trigger_type));
            }
            OrderType::StopLoss | OrderType::TakeProfit => {
                if size > 0.0 {
                    p.sz = Some(num::format(size));
                } else {
                    p.close_fraction = Some("1");
                }
                p.reduce_only = Some(true);
                let price =
                    num::format(adjusted(o.price.unwrap_or(0.0), m.precision.price, false)?);
                if o.kind == OrderType::StopLoss {
                    p.sl_trigger_px = Some(price);
                    p.sl_ord_px = Some("-1");
                    p.sl_trigger_px_type = Some(trigger_name(
                        o.sl_trigger_type.or(Some(StopTriggerType::MarkPrice)),
                    ));
                } else {
                    p.tp_trigger_px = Some(price);
                    p.tp_ord_px = Some("-1");
                    p.tp_trigger_px_type = Some(trigger_name(o.tp_trigger_type));
                }
            }
            OrderType::TrailingStopLoss => {
                p.active_px = Some("");
                p.callback_ratio = Some("");
                p.callback_spread = Some(num::format(adjusted(
                    o.price.unwrap_or(0.0),
                    m.precision.price,
                    false,
                )?));
                p.close_fraction = Some("1");
            }
            _ => {}
        }
        Ok(p)
    }

    fn plan_cancel(&self, cx: &Cx<'_, Pending>, ids: Vec<String>) -> Result<Job, String> {
        let mut regular = Vec::new();
        let mut algos = Vec::new();
        let mut seen = HashSet::new();
        let mut optimistic = Vec::new();
        for id in &ids {
            let (inst_id, native, algo) = if let Some(o) = cx.store.order(id) {
                let m = cx
                    .store
                    .market(&o.symbol)
                    .ok_or_else(|| "unknown market".to_owned())?;
                optimistic.push((id.clone(), o.status));
                (m.id.as_str(), native_id(id), is_algo(o.kind))
            } else if let Some(order) = self.trading.acknowledged.get(id) {
                (order.inst_id.as_str(), id.as_str(), order.algo)
            } else {
                return Err(format!(
                    "Order {id} not found; cancellation requires a reconciled order"
                ));
            };
            if !seen.insert((inst_id, native)) {
                continue;
            }
            if algo {
                algos.push(Cancel {
                    inst_id,
                    ord_id: None,
                    algo_id: Some(native),
                });
            } else {
                regular.push(Cancel {
                    inst_id,
                    ord_id: Some(id),
                    algo_id: None,
                });
            }
        }
        let mut steps = Vec::new();
        for batch in regular.chunks(20) {
            steps.push(step(
                "/api/v5/trade/cancel-batch-orders",
                batch,
                batch.len(),
                false,
            )?);
        }
        for batch in algos.chunks(10) {
            steps.push(step(
                "/api/v5/trade/cancel-algos",
                batch,
                batch.len(),
                false,
            )?);
        }
        let mut result = job(steps, Finish::Empty);
        result.optimistic = optimistic;
        result.continue_cancel_errors = true;
        Ok(result)
    }

    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Job, String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Amend<'a> {
            inst_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            ord_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            algo_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sz: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_trigger_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_ord_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_trigger_px_type: Option<&'static str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sl_trigger_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sl_ord_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sl_trigger_px_type: Option<&'static str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_tp_trigger_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_tp_ord_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_tp_trigger_px_type: Option<&'static str>,
        }
        let o = cx
            .store
            .order(id)
            .ok_or_else(|| format!("unknown order {id}"))?;
        if o.kind == OrderType::Market
            || !matches!(o.status, OrderStatus::Open | OrderStatus::Updating)
        {
            return Err("Only active limit or algo orders can be amended".into());
        }
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| "unknown market".to_owned())?;
        let oco = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && cx.store.orders().iter().any(|other| {
                other.id != o.id
                    && other.symbol == o.symbol
                    && native_id(&other.id) == native_id(id)
                    && matches!(other.kind, OrderType::StopLoss | OrderType::TakeProfit)
            });
        if !oco
            && matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && o.extra
                .as_ref()
                .and_then(|e| e.conditional_order_price)
                .is_some_and(|p| p > 0.0)
        {
            return Err("Cannot amend a standalone protective limit order without preserving its execution price".into());
        }
        let mut a = Amend {
            inst_id: &m.id,
            ord_id: None,
            algo_id: None,
            new_px: None,
            new_sz: None,
            new_trigger_px: None,
            new_ord_px: None,
            new_trigger_px_type: None,
            new_sl_trigger_px: None,
            new_sl_ord_px: None,
            new_sl_trigger_px_type: None,
            new_tp_trigger_px: None,
            new_tp_ord_px: None,
            new_tp_trigger_px_type: None,
        };
        if let Some(amount) = amount.filter(|_| o.amount != 0.0) {
            a.new_sz = Some(num::format(adjusted(amount, m.precision.amount, false)?));
        }
        let mut result = if o.kind == OrderType::Limit {
            a.ord_id = Some(id);
            if let Some(price) = price {
                if !price.is_finite() || price <= 0.0 {
                    return Err("invalid order price".into());
                }
                a.new_px = Some(num::format(price));
            }
            job(
                vec![step("/api/v5/trade/amend-order", &a, 1, false)?],
                Finish::Amend(id.into()),
            )
        } else if o.kind == OrderType::Conditional {
            a.algo_id = Some(native_id(id));
            a.new_trigger_px = Some(num::format(adjusted(
                price.unwrap_or(o.price),
                m.precision.price,
                false,
            )?));
            let limit = o
                .extra
                .as_ref()
                .and_then(|e| e.conditional_order_price)
                .unwrap_or(-1.0);
            a.new_ord_px = Some(if limit > 0.0 {
                num::format(num::adjust(limit, m.precision.price))
            } else {
                "-1".into()
            });
            a.new_trigger_px_type = Some(trigger_name(o.trigger_type));
            job(
                vec![step("/api/v5/trade/amend-algos", &a, 1, false)?],
                Finish::Amend(id.into()),
            )
        } else if oco {
            a.algo_id = Some(native_id(id));
            let price = num::format(adjusted(
                price.unwrap_or(o.price),
                m.precision.price,
                false,
            )?);
            let order_price = o
                .extra
                .as_ref()
                .and_then(|e| e.conditional_order_price)
                .filter(|p| *p > 0.0)
                .map_or_else(
                    || "-1".into(),
                    |p| num::format(num::adjust(p, m.precision.price)),
                );
            if o.kind == OrderType::StopLoss {
                a.new_sl_trigger_px = Some(price);
                a.new_sl_ord_px = Some(order_price);
                a.new_sl_trigger_px_type = Some(trigger_name(o.trigger_type));
            } else {
                a.new_tp_trigger_px = Some(price);
                a.new_tp_ord_px = Some(order_price);
                a.new_tp_trigger_px_type = Some(trigger_name(o.trigger_type));
            }
            job(
                vec![step("/api/v5/trade/amend-algos", &a, 1, false)?],
                Finish::Amend(id.into()),
            )
        } else {
            let group: Vec<_> = cx
                .store
                .orders()
                .iter()
                .filter(|c| native_id(&c.id) == native_id(id))
                .map(|c| c.id.clone())
                .collect();
            let mut result = self.plan_cancel(cx, group)?;
            let replacement = self.plan_place(
                cx,
                vec![PlaceOrderOpts {
                    symbol: o.symbol.clone(),
                    kind: o.kind,
                    side: o.side,
                    amount: amount.unwrap_or(o.amount),
                    price: Some(price.unwrap_or(o.price)),
                    trigger_price: None,
                    stop_loss: None,
                    take_profit: None,
                    reduce_only: Some(o.reduce_only),
                    time_in_force: o.time_in_force,
                    trigger_type: o.trigger_type,
                    sl_trigger_type: o.trigger_type,
                    tp_trigger_type: o.trigger_type,
                    client_order_id: None,
                    close: o.close,
                    stp_mode: None,
                    max_slippage_pct: None,
                    bybit_bbo: None,
                    bybit_slippage_tolerance_ticks: None,
                    implied_volatility: None,
                }],
            )?;
            result.steps.extend(replacement.steps);
            result.continue_cancel_errors = false;
            result.finish = Finish::Ids;
            result
        };
        if result.optimistic.is_empty() {
            result.optimistic.push((id.into(), o.status));
        }
        Ok(result)
    }

    /// Mints the next call's leg from its slot and releases it signed. Progress
    /// receipts publish after admission and before the physical release.
    fn issue(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, slot: Slot) {
        let Some(job) = self.trading.jobs.get(&command) else {
            return;
        };
        let plan = job.plan.clone().expect("admitted Plan");
        let context = match cx.plan_leg_context(&plan, &slot) {
            Ok(context) => context,
            Err(reason) => {
                let error = VenueError::new(reason.venue_error().message);
                return self.end(cx, command, None, error, false);
            }
        };
        let step = job.steps.front().expect("issued step");
        cx.command_progress(command, step.receipts.clone());
        self.release(cx, command, slot, context);
    }

    /// Signs and releases the job's front step on `context`.
    fn release(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        slot: Slot,
        context: Context,
    ) {
        let Some(job) = self.trading.jobs.get(&command) else {
            return;
        };
        let step = job.steps.front().expect("issued step");
        let recipe =
            sign::Http::new(self.credentials.clone(), step.path.into()).demo(self.options.testnet);
        let url = format!("{}{}", self.rest(), step.path);
        let body = Some(step.body.clone());
        let request = Request {
            command,
            slot,
            context: context.clone(),
            path: step.path,
        };
        if let Err((pending, reason)) = cx.request_signed_managed(
            context,
            None,
            Method::Post,
            url,
            body,
            Box::new(recipe),
            Pending::Trade(request),
        ) && let Pending::Trade(request) = pending
        {
            self.call_failed(cx, request, not_sent(reason.venue_error().message), false);
        }
    }

    /// Native completion effects and result, once every call has settled.
    fn finish_effects(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        job: &mut Job,
    ) -> Result<String, String> {
        for (id, status) in std::mem::take(&mut job.optimistic) {
            if cx
                .store
                .order(&id)
                .is_some_and(|o| matches!(o.status, OrderStatus::Updating | OrderStatus::Canceling))
            {
                cx.store.set_order_status(&id, status);
            }
        }
        match &job.finish {
            Finish::Leverage(symbol, leverage) => {
                *self
                    .account
                    .leverage_versions
                    .entry(symbol.clone())
                    .or_default() += 1;
                let leverage = *leverage;
                cx.store.update_setting(symbol, |s| {
                    s.leverage = leverage;
                    s.margin_type = Some(MarginType::Cross);
                });
                cx.hold_setting(SettingKey::Leverage(symbol.clone()));
                self.account_after_settings(cx);
                Ok("[]".into())
            }
            Finish::Mode(hedged) => {
                self.account.mode_version += 1;
                cx.store.set_hedged(*hedged);
                cx.hold_setting(SettingKey::Hedged);
                self.account_after_settings(cx);
                Ok("[]".into())
            }
            Finish::Amend(id) => serde_json::to_string(&[id]).map_err(|e| e.to_string()),
            Finish::Ids => serde_json::to_string(&job.ids).map_err(|e| e.to_string()),
            Finish::Empty => Ok("[]".into()),
        }
    }

    fn finish(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let Some(mut job) = self.trading.jobs.remove(&request.command) else {
            return;
        };
        let plan = job.plan.clone().expect("admitted Plan");
        let _ = cx.finish_plan_call(&plan, &request.slot, &request.context);
        let _ = match self.finish_effects(cx, &mut job) {
            Ok(data) => match cx.done_plan(&plan, data) {
                Ok(()) => Ok(()),
                Err(_) => cx.fail_plan(&plan, VenueError::new("OKX job ended without its call")),
            },
            Err(error) => cx.fail_plan(&plan, VenueError::new(error)),
        };
    }

    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let Some(plan) = self
            .trading
            .jobs
            .get(&request.command)
            .and_then(|job| job.plan.clone())
        else {
            return;
        };
        // Exact released DATA custody before any decode, Store or job change.
        if cx.validate_plan_delivery(&plan, &request.context).is_err() {
            return;
        }
        // OKX 50004 explicitly leaves execution unknown, including HTTP 200/400 replies.
        let parsed = serde_json::from_str::<wire::Envelope<'_>>(response.body);
        if response.status == 408
            || (500..600).contains(&response.status)
            || parsed.as_ref().is_ok_and(|reply| reply.code == "50004")
        {
            let message = format!("OKX HTTP {}: {}", response.status, response.body);
            return self.unproven(cx, request, &message);
        }
        if !(200..300).contains(&response.status) {
            let code = parsed.as_ref().ok().map(|reply| reply.code);
            let message = format!("OKX HTTP {}: {}", response.status, response.body);
            return self.failure(cx, request, code, message, response.status);
        }
        let envelope = match parsed {
            Ok(envelope) => envelope,
            Err(e) => return self.unproven(cx, request, &e.to_string()),
        };
        let Some(acks) = envelope
            .data
            .and_then(|data| serde_json::from_str::<Vec<Ack<'_>>>(data.get()).ok())
        else {
            if matches!(envelope.code, "0" | "2") {
                return self.unproven(cx, request, "Malformed OKX write response");
            }
            let message = format!("OKX {}: {}", envelope.code, envelope.msg);
            return self.failure(cx, request, Some(envelope.code), message, response.status);
        };
        // A refused envelope with no failing rows is one whole native failure.
        if !matches!(envelope.code, "0" | "2")
            && acks
                .iter()
                .all(|a| matches!(a.s_code, "" | "0") && a.s_msg.is_empty())
        {
            let message = format!("OKX {}: {}", envelope.code, envelope.msg);
            return self.failure(cx, request, Some(envelope.code), message, response.status);
        }
        let Some(job) = self.trading.jobs.get_mut(&request.command) else {
            return;
        };
        let step = job.steps.front().expect("inflight step");
        if step.collect_ids && matches!(envelope.code, "0" | "2") {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Identity {
                inst_id: String,
            }
            let identities = if step.path == "/api/v5/trade/batch-orders" {
                serde_json::from_str::<Vec<Identity>>(&step.body)
            } else {
                serde_json::from_str::<Identity>(&step.body).map(|row| vec![row])
            };
            let identities = match identities {
                Ok(rows) if rows.len() == acks.len() => rows,
                _ => {
                    return self.unproven(
                        cx,
                        request,
                        "OKX accepted order identity is unavailable",
                    );
                }
            };
            self.account.revision += 1;
            for (ack, identity) in acks.iter().zip(identities) {
                if (!ack.s_code.is_empty() && ack.s_code != "0") || !ack.s_msg.is_empty() {
                    continue;
                }
                let id = if ack.algo_id.is_empty() {
                    ack.ord_id
                } else {
                    ack.algo_id
                };
                if id.is_empty() {
                    return self.unproven(cx, request, "OKX placement response missing order ID");
                }
                if !self.account.order_final(id)
                    && let Some(market) = cx.store.market_by_id(&identity.inst_id)
                {
                    self.trading.acknowledged.insert(
                        id.into(),
                        Acknowledged {
                            symbol: market.symbol.clone(),
                            inst_id: identity.inst_id,
                            algo: !ack.algo_id.is_empty(),
                            version: self.account.revision,
                        },
                    );
                }
                job.ids.push(id.into());
            }
        }
        if acks.iter().any(|ack| ack.s_code == "50004") {
            return self.unproven(cx, request, "OKX 50004: write execution status is unknown");
        }
        if step.expected > 0
            && acks.iter().any(|ack| {
                matches!(ack.s_code, "" | "0") && ack.ord_id.is_empty() && ack.algo_id.is_empty()
            })
        {
            return self.unproven(
                cx,
                request,
                "OKX success acknowledgement omitted native order identity",
            );
        }
        if step.expected > 0 && acks.len() != step.expected {
            return self.unproven(cx, request, "OKX write response row count mismatch");
        }
        if step.collect_ids
            && acks.len() == step.expected
            && acks.iter().all(|ack| {
                if ack.s_code.is_empty() || ack.s_code == "0" {
                    (!ack.ord_id.is_empty() || !ack.algo_id.is_empty()) && ack.s_msg.is_empty()
                } else {
                    // 51xxx are explicit trading-rule rejections; transport/server
                    // timeout codes do not prove that the venue rejected a write.
                    ack.s_code.starts_with("51")
                }
            })
        {
            cx.placement_resolved(request.command);
        }
        let failed = acks
            .iter()
            .filter(|a| (!a.s_code.is_empty() && a.s_code != "0") || !a.s_msg.is_empty());
        let first_code = failed.clone().next().map(|a| a.s_code);
        // Only 51xxx proves a refused leg, and an accepted leg means something happened.
        let definitive = failed.clone().all(|a| a.s_code.starts_with("51"))
            && !acks.iter().any(|a| {
                matches!(a.s_code, "" | "0")
                    && a.s_msg.is_empty()
                    && (!a.ord_id.is_empty() || !a.algo_id.is_empty())
            });
        let errors: Vec<_> = failed
            .map(|a| format!("{} (code {})", a.s_msg, a.s_code))
            .collect();
        let (expected, continue_cancel_errors) = (step.expected, job.continue_cancel_errors);
        if expected == 0 && envelope.code != "0" {
            let message = format!("OKX {}: {}", envelope.code, envelope.msg);
            return self.failure(cx, request, Some(envelope.code), message, response.status);
        }
        // Record the call's fact before any continuation or terminal.
        let recorded = if expected > 0 {
            let items: Vec<_> = acks
                .iter()
                .map(|ack| {
                    if matches!(ack.s_code, "" | "0") && ack.s_msg.is_empty() {
                        BatchItem::Accepted(if ack.algo_id.is_empty() {
                            ack.ord_id
                        } else {
                            ack.algo_id
                        })
                    } else {
                        BatchItem::Native(native(ack.s_code, ack.s_msg))
                    }
                })
                .collect();
            cx.http_write_batch(
                &request.context,
                &CODES,
                expected,
                &items,
                errors.join("; "),
                None,
            )
        } else {
            let accepted = WriteFact::Accepted { order_ids: vec![] };
            cx.http_write_fact(&request.context, accepted, None)
                .map(|_| WriteResult::Accepted { order_ids: vec![] })
        };
        let unproven = match &recorded {
            Ok(WriteResult::PartialUnknown(_)) | Err(_) => true,
            Ok(WriteResult::Failed(error)) => error.outcome == Some(Outcome::Unknown),
            Ok(_) => false,
        };
        let refused = |mut error: VenueError| {
            if !definitive {
                error.outcome = None;
            }
            error
        };
        if unproven {
            let message = if errors.is_empty() {
                format!("OKX {}: {}", envelope.code, envelope.msg)
            } else {
                errors.join("; ")
            };
            let error = VenueError::new(message).outcome(Outcome::Unknown);
            return self.end(cx, request.command, Some(&request), error, true);
        }
        if envelope.code != "0"
            && !(continue_cancel_errors && envelope.code == "2" && !errors.is_empty())
        {
            let error = if errors.is_empty() {
                let error = format!("OKX {}: {}", envelope.code, envelope.msg);
                rejection(error, Some(envelope.code), response.status)
            } else {
                refused(rejection(errors.join("; "), first_code, response.status))
            };
            return self.end(cx, request.command, Some(&request), error, false);
        }
        if !errors.is_empty() {
            if !continue_cancel_errors {
                let error = refused(rejection(errors.join("; "), first_code, response.status));
                return self.end(cx, request.command, Some(&request), error, false);
            }
            for error in errors {
                cx.command_error(request.command, error);
            }
        }
        let Some(job) = self.trading.jobs.get_mut(&request.command) else {
            return;
        };
        job.steps.pop_front();
        job.effective = true;
        if job.steps.is_empty() {
            return self.finish(cx, request);
        }
        match cx.continue_serial_plan(&plan, &request.slot, &request.context, SERIAL) {
            Ok(slot) => self.issue(cx, request.command, slot),
            Err(reason) => {
                let error = VenueError::new(reason.venue_error().message);
                self.end(cx, request.command, Some(&request), error, false);
            }
        }
    }

    /// A delivered answer that cannot prove the write: recorded unproven, then
    /// Unknown. Optimistic status stays for the engine's reconcile snapshot.
    fn unproven(&mut self, cx: &mut Cx<'_, Pending>, request: Request, message: &str) {
        let _ = cx.http_write_unproven(&request.context, message);
        let error = VenueError::new(message).outcome(Outcome::Unknown);
        self.end(cx, request.command, Some(&request), error, true);
    }

    /// One whole native failure. A code the table does not prove refused ends
    /// Unknown rather than Rejected.
    fn failure(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        code: Option<&str>,
        message: String,
        status: u16,
    ) {
        let evidence = code
            .filter(|code| !code.is_empty())
            .map(|code| native(code, &message));
        let failure = cx.http_write_failure(
            &request.context,
            &CODES,
            evidence.as_ref(),
            message.clone(),
            None,
        );
        let (verdict, grant) = match failure {
            Ok((verdict, grant)) => (Ok(verdict), grant),
            Err(error) => (Err(error), None),
        };
        // One re-signed resend of a 50113 refusal; the journal refuses a second.
        if let Some(grant) = grant {
            let plan = self
                .trading
                .jobs
                .get(&request.command)
                .and_then(|job| job.plan.clone());
            if let Some(plan) = plan
                && let Ok(context) =
                    cx.plan_resend_context(&plan, &request.slot, &request.context, grant)
            {
                self.release(cx, request.command, request.slot, context);
                return;
            }
        }
        let error = rejection(message, code, status);
        if matches!(verdict, Ok(Verdict::Refused(_) | Verdict::RefusedResend(_))) {
            self.end(cx, request.command, Some(&request), error, false);
        } else {
            let error = error.outcome(Outcome::Unknown);
            self.end(cx, request.command, Some(&request), error, true);
        }
    }

    pub(crate) fn trade_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        self.call_failed(cx, request, error, false);
    }

    pub(crate) fn trade_lost(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        let error = VenueError::new(error).outcome(Outcome::Unknown);
        self.call_failed(cx, request, error, true);
    }

    /// The call failed without a delivered answer: settle it, then end the job.
    fn call_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
        uncertain: bool,
    ) {
        if let Some(plan) = self
            .trading
            .jobs
            .get(&request.command)
            .and_then(|job| job.plan.clone())
        {
            let _ = cx.finish_plan_failed_call(&plan, &request.slot, &request.context);
        }
        self.end(cx, request.command, None, error, uncertain);
    }

    /// Fails the job under its owner. After a step took effect a later
    /// rejection never means nothing happened, and an uncertain write keeps its
    /// optimistic status for the reconcile snapshot.
    fn end(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        current: Option<&Request>,
        mut error: VenueError,
        uncertain: bool,
    ) {
        let Some(job) = self.trading.jobs.remove(&command) else {
            return;
        };
        if job.effective && !uncertain {
            error.outcome = None;
        }
        for (id, status) in job.optimistic {
            if !uncertain
                && cx.store.order(&id).is_some_and(|o| {
                    matches!(o.status, OrderStatus::Updating | OrderStatus::Canceling)
                })
            {
                cx.store.set_order_status(&id, status);
            }
        }
        let _ = match (&job.plan, &job.preflight) {
            (Some(plan), _) => {
                if let Some(request) = current {
                    let _ = cx.finish_plan_call(plan, &request.slot, &request.context);
                }
                cx.fail_plan(plan, error)
            }
            (None, Some(preflight)) => cx.fail_preflight(preflight, error),
            (None, None) => Ok(()),
        };
    }

    /// The engine retired a write owner itself (for example an unproven 408/5xx):
    /// forget the job. Its optimistic status stays for the reconcile snapshot.
    pub(crate) fn retire_trading(&mut self, owner: &HttpOwnerRetirement) {
        self.trading.jobs.retain(|_, job| {
            !job.preflight
                .as_ref()
                .is_some_and(|preflight| owner.matches_preflight(preflight))
        });
    }
}

/// Bounded correlation for a released write: its endpoint. The order snapshot
/// reconciles; OKX has no late reconcile codec.
pub(crate) fn trade_reconciliation(
    request: &Request,
) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
    ReconciliationData::new(request.path.to_owned())
}

/// A definitive OKX refusal. Its kind comes from the table (the native code,
/// else a documented status row), `rejected` when neither names one.
fn rejection(message: String, code: Option<&str>, status: u16) -> VenueError {
    let by_code = code.filter(|code| !code.is_empty()).and_then(|code| {
        CODES
            .verdict(&native(code, ""), safe_cex_engine::classify::Intent::Place)
            .kind()
    });
    let by_status = || {
        CODES
            .status
            .iter()
            .find(|(documented, _)| *documented == status)
            .and_then(|(_, verdict)| verdict.kind())
    };
    let kind = by_code.or_else(by_status).unwrap_or(ErrorKind::Rejected);
    // System errors and busy replies do not prove the venue refused the write.
    let outcome = if matches!(code, Some("50001" | "50013" | "50026")) {
        Outcome::Unknown
    } else {
        Outcome::Rejected
    };
    let mut error = VenueError::new(message).kind(kind).outcome(outcome);
    if let Some(code) = code.filter(|code| !code.is_empty()) {
        error = error.venue_code(code);
    }
    error
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

fn empty_payload() -> OrderPayload {
    OrderPayload {
        symbol: String::new(),
        inst_id: String::new(),
        td_mode: "cross",
        tag: BROKER,
        side: OrderSide::Buy,
        ord_type: "limit",
        pos_side: None,
        cl_ord_id: None,
        algo_cl_ord_id: None,
        sz: None,
        px: None,
        reduce_only: None,
        close_fraction: None,
        trigger_px: None,
        order_px: None,
        trigger_px_type: None,
        sl_trigger_px: None,
        sl_ord_px: None,
        sl_trigger_px_type: None,
        tp_trigger_px: None,
        tp_ord_px: None,
        tp_trigger_px_type: None,
        active_px: None,
        callback_ratio: None,
        callback_spread: None,
    }
}
fn step<T: Serialize + ?Sized>(
    path: &'static str,
    body: &T,
    expected: usize,
    collect_ids: bool,
) -> Result<Step, String> {
    Ok(Step {
        receipts: Vec::new(),
        path,
        body: serde_json::to_string(body).map_err(|e| e.to_string())?,
        expected,
        collect_ids,
    })
}
fn job(steps: Vec<Step>, finish: Finish) -> Job {
    Job {
        preflight: None,
        plan: None,
        steps: steps.into(),
        ids: Vec::new(),
        finish,
        optimistic: Vec::new(),
        continue_cancel_errors: false,
        effective: false,
    }
}
fn is_algo(kind: OrderType) -> bool {
    !matches!(kind, OrderType::Limit | OrderType::Market)
}
fn native_id(id: &str) -> &str {
    id.strip_suffix("_sl")
        .or_else(|| id.strip_suffix("_tp"))
        .or_else(|| id.strip_suffix("_tsl"))
        .unwrap_or(id)
}
fn trigger_name(value: Option<StopTriggerType>) -> &'static str {
    match value {
        Some(StopTriggerType::MarkPrice) => "mark",
        Some(StopTriggerType::IndexPrice) => "index",
        _ => "last",
    }
}
fn adjusted(value: f64, precision: f64, allow_zero: bool) -> Result<f64, String> {
    if !value.is_finite() || value < 0.0 || !precision.is_finite() || precision <= 0.0 {
        return Err("order amount/price must be finite".into());
    }
    let result = num::adjust(value, precision);
    if result < 0.0 || (result == 0.0 && (!allow_zero || value > 0.0)) {
        Err("Order amount or price is too small".into())
    } else {
        Ok(result)
    }
}

// Integer decimal units avoid floor(0.3 / 0.1) losing an entire lot.
fn lot_floor(max: f64, step: f64) -> Result<f64, String> {
    let max_text = max.to_string();
    let step_text = step.to_string();
    let decimals = |text: &str| text.split_once('.').map_or(0, |(_, tail)| tail.len());
    let places = decimals(&max_text).max(decimals(&step_text));
    let units = |text: &str| -> Option<u128> {
        let value = text.replace('.', "").parse::<u128>().ok()?;
        value.checked_mul(10u128.checked_pow((places - decimals(text)).try_into().ok()?)?)
    };
    let max_units = units(&max_text).ok_or("invalid market size limits")?;
    let step_units = units(&step_text)
        .filter(|n| *n > 0)
        .ok_or("invalid market lot size")?;
    Ok(num::adjust((max_units / step_units) as f64 * step, step))
}

fn placement_step<T: Serialize + ?Sized + PlacementIdentities>(
    path: &'static str,
    body: &T,
    expected: usize,
    collect_ids: bool,
) -> Result<Step, String> {
    let mut write = step(path, body, expected, collect_ids)?;
    write.receipts = body.identities();
    Ok(write)
}
trait PlacementIdentities {
    fn identities(&self) -> Vec<OrderIdentityReceipt>;
}
impl PlacementIdentities for OrderPayload {
    fn identities(&self) -> Vec<OrderIdentityReceipt> {
        self.cl_ord_id
            .iter()
            .chain(self.algo_cl_ord_id.iter())
            .map(|identity| OrderIdentityReceipt {
                symbol: self.symbol.clone(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: identity.clone(),
            })
            .collect()
    }
}
impl<const N: usize> PlacementIdentities for [OrderPayload; N] {
    fn identities(&self) -> Vec<OrderIdentityReceipt> {
        self.as_slice().identities()
    }
}
impl PlacementIdentities for [OrderPayload] {
    fn identities(&self) -> Vec<OrderIdentityReceipt> {
        self.iter()
            .flat_map(PlacementIdentities::identities)
            .collect()
    }
}

#[cfg(test)]
mod code_table_tests {
    use super::*;
    use safe_cex_engine::classify::Intent;

    /// assert_table covers the status rows (never Satisfied or RefusedResend) and
    /// shadowing (an exact row after its range panics); OKX adds two of its own.
    #[test]
    fn the_okx_table_satisfies_the_engine_invariants() {
        safe_cex_engine::classify::assert_table(&CODES);
        for row in CODES.rows {
            // Every row is unscoped, so rejection()'s Place lookup reads the
            // same row any write intent would.
            assert_eq!(row.only, None, "{row:?}");
            // 50113 (signature) is the one documented re-sign.
            if matches!(row.verdict, Verdict::RefusedResend(_)) {
                assert_eq!(row.key, Key::Int(50113), "{row:?}");
            }
            // No auth or permission code ever proves a write landed.
            if matches!(row.key, Key::Int(code) if (50100..=50199).contains(&code) || code == 50030)
            {
                assert!(!matches!(row.verdict, Verdict::Satisfied(_)), "{row:?}");
            }
        }
        // Every exact 51xxx row sits before the range it refines.
        let range = CODES
            .rows
            .iter()
            .position(|row| row.key == Key::Range(51000, 51999))
            .unwrap();
        for (index, row) in CODES.rows.iter().enumerate() {
            if let Key::Int(code) = row.key
                && (51000..=51999).contains(&code)
            {
                assert!(index < range, "{code} is shadowed by the 51xxx range");
            }
        }
    }

    #[test]
    fn exact_rows_win_over_their_range_and_unlisted_codes_prove_nothing() {
        let verdict = |code| CODES.verdict(&native(code, "native"), Intent::Place);
        assert_eq!(verdict("51000"), Verdict::Refused(ErrorKind::InvalidInput));
        assert_eq!(
            verdict("51008"),
            Verdict::Refused(ErrorKind::InsufficientBalance)
        );
        for code in ["51119", "51127", "51131", "51502", "59200"] {
            assert_eq!(
                verdict(code),
                Verdict::Refused(ErrorKind::InsufficientBalance),
                "{code}"
            );
        }
        for code in [
            "51001", "51006", "51020", "51121", "51122", "51137", "51203",
        ] {
            assert_eq!(
                verdict(code),
                Verdict::Refused(ErrorKind::InvalidInput),
                "{code}"
            );
        }
        // State and risk refusals in the range stay plain rejections.
        assert_eq!(verdict("51004"), Verdict::Refused(ErrorKind::Rejected));
        assert_eq!(verdict("51999"), Verdict::Refused(ErrorKind::Rejected));
        assert_eq!(verdict("50011"), Verdict::Refused(ErrorKind::RateLimited));
        assert_eq!(
            verdict("50113"),
            Verdict::RefusedResend(ErrorKind::AuthInvalid)
        );
        for unproven in ["50001", "50004", "50013", "50026", "52000", "59999"] {
            assert_eq!(verdict(unproven), Verdict::Unknown(None), "{unproven}");
        }
    }
}
