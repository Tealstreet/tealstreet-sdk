//! Trading command jobs: each command owns a Preflight root and one streamed
//! serial Plan. A lost write response is never retried.
use super::*;
use safe_cex_engine::classify::{
    BatchItem, CodeTable, Intent, Key, Native, NativeCode, Row, SatisfiedIntent, Verdict,
    WriteFact, decode_int,
};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::plan::{SerialCall, SerialPolicy, Slot};
use safe_cex_engine::request_context::{
    HttpOwnerRetirement, Plan, Preflight, PreflightPolicy, PreflightPurpose, ReconciliationData,
};
use safe_cex_engine::setting_holds::SettingKey;
use safe_cex_engine::{CommandWait, ErrorKind, Outcome, VenueError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
const BROKER: &str = "b3cdedc0a20880ba";
/// Each call's own bound; the streamed Plan has no whole-job deadline.
const CALL_MS: u32 = 20_000;

/// BloFin's documented error codes (docs.blofin.com, "Errors", read
/// 2026-10-26). Cited refusals are pre-execution; anything unlisted proves
/// nothing. "Gone" is satisfied only for a cancel. BloFin returns its 400/401
/// codes in a 200 body; only 403 (firewall) and 429 are documented HTTP statuses.
const CODES: CodeTable = CodeTable {
    rows: &[
        // "Bad Request" and "Invalid signature" body codes.
        refused(Key::Int(400), ErrorKind::InvalidInput),
        refused(Key::Int(401), ErrorKind::AuthInvalid),
        // Request parameter validation.
        refused(Key::Range(152001, 152015), ErrorKind::InvalidInput),
        refused(Key::Int(152420), ErrorKind::InvalidInput),
        refused(Key::Int(152423), ErrorKind::InvalidInput),
        // Order, price and size rules.
        refused(Key::Int(542), ErrorKind::Rejected),
        refused(Key::Int(102002), ErrorKind::Rejected),
        refused(Key::Int(102014), ErrorKind::Rejected),
        refused(Key::Int(102015), ErrorKind::Rejected),
        refused(Key::Int(102022), ErrorKind::Rejected),
        refused(Key::Range(102037, 102055), ErrorKind::Rejected),
        refused(Key::Int(102064), ErrorKind::Rejected),
        refused(Key::Int(102065), ErrorKind::Rejected),
        refused(Key::Int(102089), ErrorKind::Rejected),
        refused(Key::Int(102167), ErrorKind::Rejected),
        // Native: Multi-Position mode refuses the whole placement.
        refused(Key::Int(102143), ErrorKind::Unsupported),
        // Balance and margin.
        refused(Key::Int(103003), ErrorKind::Rejected),
        refused(Key::Int(150004), ErrorKind::Rejected),
        // Duplicate client order ID.
        refused(Key::Int(150003), ErrorKind::InvalidInput),
        refused(Key::Int(152030), ErrorKind::InvalidInput),
        // Leverage and margin-mode settings blocked.
        refused(Key::Int(110006), ErrorKind::Rejected),
        refused(Key::Int(110019), ErrorKind::Rejected),
        // Authentication.
        refused(Key::Int(152401), ErrorKind::AuthInvalid),
        refused(Key::Int(152402), ErrorKind::AuthExpired),
        refused(Key::Int(152406), ErrorKind::AuthPermission),
        refused(Key::Int(152408), ErrorKind::AuthInvalid),
        refused(Key::Int(152409), ErrorKind::AuthInvalid),
        // Internal error: proves nothing.
        Row {
            key: Key::Int(103013),
            verdict: Verdict::Unknown(Some(ErrorKind::Internal)),
            only: None,
        },
        // "The order has been canceled" and "Cancel failed: does not exist".
        gone(1000),
        gone(102068),
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[
        // "Network Firewall Restrictions" and "Rate Limits".
        (403, Verdict::Refused(ErrorKind::AuthPermission)),
        (429, Verdict::Refused(ErrorKind::RateLimited)),
    ],
};

const fn refused(key: Key, kind: ErrorKind) -> Row {
    Row {
        key,
        verdict: Verdict::Refused(kind),
        only: None,
    }
}

const fn gone(code: i64) -> Row {
    Row {
        key: Key::Int(code),
        verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
        only: Some(Intent::Cancel),
    }
}

#[derive(Default)]
pub(crate) struct Trading {
    acknowledged: HashMap<String, Target>,
    /// Commands behind the active job, each a Preflight parked with no I/O.
    queue: VecDeque<Queued>,
    active: Option<Active>,
    /// A job whose Plan the engine sealed itself: restore its Store rows and
    /// reconcile at the next callback that holds Cx.
    retired: Option<Job>,
}
struct Queued {
    preflight: Preflight,
    command: Command,
    /// The caller's wait, when it set one.
    deadline: Option<f64>,
}
struct Active {
    preflight: Preflight,
    plan: Option<Plan>,
    job: Job,
}
pub struct Request {
    command: CommandId,
    slot: Slot,
    context: Context,
    step: Step,
}
/// Where the active job is driven from: entry custody, the current call's
/// DATA, or a Plan-ready callback holding the next slot.
enum At<'a> {
    Entry,
    Data(&'a Slot, &'a Context),
    Ready(Slot),
}
struct Job {
    command: CommandId,
    intent: Intent,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    /// Store rows issued so far, restored on every terminal.
    creating: Vec<String>,
    canceling: Vec<String>,
    replacement_targets: Vec<Target>,
    commit: Commit,
    /// The hedged leverage legs: a failure refreshes the account, as native's join did.
    join: bool,
}
struct Step {
    receipts: Vec<OrderIdentityReceipt>,
    /// Creating rows this placement call adds.
    orders: Vec<Order>,
    path: &'static str,
    body: String,
    effect: Effect,
    delay: f64,
}
enum Effect {
    Place {
        family: u8,
        count: usize,
        inst_id: String,
    },
    Cancel(Vec<Target>),
    Verify(Target),
    Setting,
}
#[derive(Default)]
enum Commit {
    #[default]
    None,
    Leverage {
        symbol: String,
        leverage: f64,
    },
    Margin(MarginType),
    Mode(bool),
}
#[derive(Clone)]
struct Target {
    id: String,
    inst_id: String,
    family: u8,
    store_ids: Vec<String>,
    filled: f64,
    filled_native: f64,
}
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Protection {
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_order_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_price_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_order_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_price_type: Option<String>,
}
impl Protection {
    fn merge_leg(&mut self, order: &Order, source: Option<&account::Source>) {
        let (execution, reference) = source.map_or((-1.0, "last"), |s| {
            if order.kind == OrderType::StopLoss {
                (s.sl_order_price, s.sl_type.as_str())
            } else {
                (s.tp_order_price, s.tp_type.as_str())
            }
        });
        let execution = if execution == 0.0 { -1.0 } else { execution };
        let reference = if reference.is_empty() {
            "last"
        } else {
            reference
        };
        if order.kind == OrderType::StopLoss {
            self.sl_trigger_price = Some(num::format(order.price));
            self.sl_order_price = Some(num::format(execution));
            self.sl_trigger_price_type = Some(reference.into());
        } else {
            self.tp_trigger_price = Some(num::format(order.price));
            self.tp_order_price = Some(num::format(execution));
            self.tp_trigger_price_type = Some(reference.into());
        }
    }
    fn retain_leg(&mut self, kind: OrderType) {
        if kind == OrderType::StopLoss {
            self.tp_trigger_price = None;
            self.tp_order_price = None;
            self.tp_trigger_price_type = None;
        } else {
            self.sl_trigger_price = None;
            self.sl_order_price = None;
            self.sl_trigger_price_type = None;
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(untagged)]
enum Reduce {
    Boolean(bool),
    Text(&'static str),
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Placement {
    inst_id: String,
    margin_mode: String,
    position_side: String,
    side: &'static str,
    size: String,
    client_order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    broker_id: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<Reduce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_price: Option<String>,
    #[serde(flatten)]
    protection: Protection,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attach_algo_orders: Vec<Protection>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cancel<'a> {
    inst_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tpsl_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    algo_id: Option<&'a str>,
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ResultRow<'a> {
    #[serde(borrow, default)]
    order_id: Str<'a>,
    #[serde(borrow, default)]
    tpsl_id: Str<'a>,
    #[serde(borrow, default)]
    algo_id: Str<'a>,
    #[serde(borrow, default)]
    code: Str<'a>,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    msg: Str<'a>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum ResultRows<'a> {
    Many(#[serde(borrow)] Vec<ResultRow<'a>>),
    One(#[serde(borrow)] ResultRow<'a>),
}
impl<'a> ResultRows<'a> {
    fn rows(self) -> Vec<ResultRow<'a>> {
        match self {
            Self::Many(v) => v,
            Self::One(v) => vec![v],
        }
    }
}
#[derive(Deserialize)]
struct Reply<'a> {
    #[serde(borrow)]
    code: Str<'a>,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    msg: Str<'a>,
    #[serde(borrow, default)]
    data: Option<ResultRows<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderDetail<'a> {
    #[serde(borrow)]
    inst_id: Str<'a>,
    #[serde(borrow)]
    order_id: Str<'a>,
    #[serde(borrow)]
    state: Str<'a>,
    #[serde(borrow)]
    filled_size: Str<'a>,
}
fn minted(cx: &Cx<'_, Pending>) -> Result<String, String> {
    Ok(format!(
        "{BROKER}{}",
        safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?
    )
    .chars()
    .take(32)
    .collect())
}
fn trigger(value: Option<StopTriggerType>) -> &'static str {
    match value {
        Some(StopTriggerType::MarkPrice) => "mark",
        Some(StopTriggerType::IndexPrice) => "index",
        _ => "last",
    }
}
fn native_side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "buy"
    } else {
        "sell"
    }
}
fn margin(value: MarginType) -> &'static str {
    if value == MarginType::Isolated {
        "isolated"
    } else {
        "cross"
    }
}
fn positive(value: f64, label: &str) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("BloFin {label} must be positive"))
    }
}
impl Job {
    fn new(command: CommandId, intent: Intent) -> Self {
        Self {
            command,
            intent,
            steps: VecDeque::new(),
            ids: vec![],
            creating: vec![],
            canceling: vec![],
            replacement_targets: vec![],
            commit: Commit::None,
            join: false,
        }
    }
}
impl Step {
    fn read(&self) -> bool {
        matches!(self.effect, Effect::Verify(_))
    }
    fn is_place(&self) -> bool {
        matches!(self.effect, Effect::Place { .. })
    }
    fn intent(&self) -> Intent {
        match self.effect {
            Effect::Place { .. } => Intent::Place,
            Effect::Cancel(_) => Intent::Cancel,
            Effect::Verify(_) => Intent::Read,
            Effect::Setting => Intent::Setting,
        }
    }
}
impl Blofin {
    fn placement(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
        source: Option<&account::Source>,
    ) -> Result<Vec<(u8, Placement, Order)>, String> {
        let m = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Unknown market {}", opts.symbol))?;
        if !self.account.mode_loaded {
            return Err("BloFin position mode is not loaded".into());
        }
        let margin_mode = cx
            .store
            .setting(&opts.symbol)
            .and_then(|s| s.margin_type)
            .or(self.account.margin)
            .ok_or("BloFin margin mode is not loaded")?;
        let protective = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
        if opts.time_in_force.is_some_and(|t| {
            t != TimeInForce::GoodTillCancel
                && (t != TimeInForce::PostOnly || opts.kind != OrderType::Limit)
        }) {
            return Err("Unsupported BloFin time in force".into());
        }
        let conditional = opts.kind == OrderType::Conditional;
        if opts.kind == OrderType::TrailingStopLoss {
            return Err("BloFin trailing stops are unsupported".into());
        }
        if conditional
            && (opts.reduce_only.unwrap_or(false)
                || opts.close.unwrap_or(false)
                || opts
                    .time_in_force
                    .is_some_and(|v| v != TimeInForce::GoodTillCancel))
        {
            return Err("Invalid BloFin opening trigger shape".into());
        }
        if !opts.amount.is_finite() || opts.amount < 0.0 || (!protective && opts.amount == 0.0) {
            return Err("BloFin amount must be positive".into());
        }
        let amount = num::adjust(opts.amount, m.precision.amount);
        if !protective || opts.amount > 0.0 {
            positive(amount, "amount")?;
        }
        let mut price = opts.price.map(|p| num::adjust(p, m.precision.price));
        if opts.kind == OrderType::Limit {
            positive(price.unwrap_or(0.0), "price")?;
        }
        if protective {
            positive(
                price
                    .or(if opts.kind == OrderType::StopLoss {
                        opts.stop_loss
                    } else {
                        opts.take_profit
                    })
                    .unwrap_or(0.0),
                "trigger price",
            )?;
        }
        if let Some(value) = opts.price {
            positive(value, "price")?;
        }
        for p in [opts.take_profit, opts.stop_loss, opts.trigger_price]
            .into_iter()
            .flatten()
        {
            positive(p, "trigger price")?;
        }
        if conditional {
            positive(
                num::adjust(opts.trigger_price.unwrap_or(0.0), m.precision.price),
                "trigger price",
            )?;
        }
        if let Some(id) = &opts.client_order_id
            && (id.is_empty()
                || id.len() > 32
                || !id.bytes().all(|c| c.is_ascii_alphanumeric())
                || !matches!(opts.kind, OrderType::Limit | OrderType::Market)
                || opts.trigger_price.is_some()
                || opts.stop_loss.is_some()
                || opts.take_profit.is_some()
                || opts.amount > m.limits.amount.max)
        {
            return Err("BloFin cannot honor caller clientOrderId on this order".into());
        }
        let reduce = protective || opts.reduce_only.unwrap_or(false);
        let side = if !cx.store.hedged() {
            "net"
        } else if (opts.side == OrderSide::Buy) ^ reduce {
            "long"
        } else {
            "short"
        };
        let tick = |p: f64| num::adjust(p, m.precision.price);
        let tp = if opts.kind == OrderType::TakeProfit {
            price.or(opts.take_profit.map(tick))
        } else {
            opts.take_profit.map(tick)
        };
        let sl = if opts.kind == OrderType::StopLoss {
            price.or(opts.stop_loss.map(tick))
        } else {
            opts.stop_loss.map(tick)
        };
        for p in [tp, sl].into_iter().flatten() {
            positive(p, "trigger price")?;
        }
        let protection = Protection {
            tp_trigger_price: tp.map(num::format),
            tp_order_price: tp.map(|_| "-1".into()),
            tp_trigger_price_type: if conditional || protective {
                tp.map(|_| trigger(opts.tp_trigger_type).into())
            } else {
                None
            },
            sl_trigger_price: sl.map(num::format),
            sl_order_price: sl.map(|_| "-1".into()),
            sl_trigger_price_type: if conditional || protective {
                sl.map(|_| trigger(opts.sl_trigger_type).into())
            } else {
                None
            },
        };
        let mut payload = Placement {
            broker_id: self.options.broker_attribution.then_some(BROKER),
            inst_id: m.id.clone(),
            margin_mode: margin(margin_mode).into(),
            position_side: side.into(),
            side: native_side(opts.side),
            size: num::format(if protective && amount == 0.0 {
                -1.0
            } else {
                amount
            }),
            client_order_id: opts
                .client_order_id
                .clone()
                .map(Ok)
                .unwrap_or_else(|| minted(cx))?,
            order_type: if protective {
                None
            } else if conditional {
                Some("trigger")
            } else if opts.kind == OrderType::Market {
                Some("market")
            } else if opts.time_in_force == Some(TimeInForce::PostOnly) {
                Some("post_only")
            } else {
                Some("limit")
            },
            price: if opts.kind == OrderType::Limit {
                price.take().map(num::format)
            } else {
                None
            },
            reduce_only: if protective {
                Some(Reduce::Text("true"))
            } else if reduce {
                Some(Reduce::Boolean(true))
            } else {
                None
            },
            trigger_price: if conditional {
                opts.trigger_price
                    .map(|p| num::format(num::adjust(p, m.precision.price)))
            } else {
                None
            },
            trigger_price_type: conditional.then(|| "last".into()),
            order_price: conditional.then(|| {
                num::format(
                    opts.price
                        .map(|p| num::adjust(p, m.precision.price))
                        .unwrap_or(-1.0),
                )
            }),
            protection: if conditional {
                Protection::default()
            } else {
                protection.clone()
            },
            attach_algo_orders: if !protective && (tp.is_some() || sl.is_some()) {
                vec![protection]
            } else {
                vec![]
            },
        };
        if let Some(source) = source {
            if source.inst_id != m.id
                || !matches!(source.margin_mode.as_str(), "cross" | "isolated")
                || !matches!(source.position_side.as_str(), "net" | "long" | "short")
            {
                if conditional {
                    return Err("BloFin replacement provenance is incomplete".into());
                }
            } else {
                payload.margin_mode = source.margin_mode.clone();
                payload.position_side = source.position_side.clone();
            }
            if !conditional {
                if payload.protection.tp_trigger_price.is_some() {
                    payload.protection.tp_order_price =
                        Some(num::format(if source.tp_order_price == 0.0 {
                            -1.0
                        } else {
                            source.tp_order_price
                        }));
                }
                if payload.protection.sl_trigger_price.is_some() {
                    payload.protection.sl_order_price =
                        Some(num::format(if source.sl_order_price == 0.0 {
                            -1.0
                        } else {
                            source.sl_order_price
                        }));
                }
                if payload.protection.tp_trigger_price.is_some()
                    && matches!(source.tp_type.as_str(), "last" | "mark" | "index")
                {
                    payload.protection.tp_trigger_price_type = Some(source.tp_type.clone());
                }
                if payload.protection.sl_trigger_price.is_some()
                    && matches!(source.sl_type.as_str(), "last" | "mark" | "index")
                {
                    payload.protection.sl_trigger_price_type = Some(source.sl_type.clone());
                }
            }
            if !conditional && !protective && (!payload.attach_algo_orders.is_empty()) {
                payload.attach_algo_orders = vec![payload.protection.clone()];
            }
            if conditional {
                if !matches!(
                    source.trigger_price_type.as_str(),
                    "last" | "mark" | "index"
                ) || !(source.order_price == -1.0 || source.order_price > 0.0)
                {
                    return Err("BloFin trigger replacement provenance is incomplete".into());
                }
                payload.trigger_price_type = Some(source.trigger_price_type.clone());
                payload.order_price = Some(num::format(source.order_price));
                if source.tp > 0.0 || source.sl > 0.0 {
                    payload.attach_algo_orders = vec![Protection {
                        tp_trigger_price: (source.tp > 0.0).then(|| num::format(source.tp)),
                        sl_trigger_price: (source.sl > 0.0).then(|| num::format(source.sl)),
                        tp_order_price: (source.tp > 0.0).then(|| {
                            num::format(if source.tp_order_price == 0.0 {
                                -1.0
                            } else {
                                source.tp_order_price
                            })
                        }),
                        sl_order_price: (source.sl > 0.0).then(|| {
                            num::format(if source.sl_order_price == 0.0 {
                                -1.0
                            } else {
                                source.sl_order_price
                            })
                        }),
                        tp_trigger_price_type: (source.tp > 0.0).then(|| {
                            if source.tp_type.is_empty() {
                                "last".into()
                            } else {
                                source.tp_type.clone()
                            }
                        }),
                        sl_trigger_price_type: (source.sl > 0.0).then(|| {
                            if source.sl_type.is_empty() {
                                "last".into()
                            } else {
                                source.sl_type.clone()
                            }
                        }),
                    }];
                }
            }
        }
        let family = if conditional {
            2
        } else if protective {
            1
        } else {
            0
        };
        let quantities = if family == 0 && amount > m.limits.amount.max {
            positive(m.limits.amount.max, "maximum size")?;
            let lots = (amount / m.limits.amount.max).ceil() as usize;
            if lots > 1000 {
                return Err("BloFin order requires too many splits".into());
            }
            let step = m.precision.amount;
            let units = (amount / step).round() as u64;
            let each = units / lots as u64;
            let remainder = units % lots as u64;
            (0..lots)
                .map(|i| (each + u64::from((i as u64) < remainder)) as f64 * step)
                .collect::<Vec<_>>()
        } else {
            vec![amount]
        };
        let mut result = Vec::new();
        for quantity in quantities {
            let mut p = payload.clone();
            if !result.is_empty() {
                p.client_order_id = minted(cx)?;
            }
            p.size = num::format(if protective && quantity == 0.0 {
                -1.0
            } else {
                num::adjust(quantity, m.precision.amount)
            });
            let base_amount = quantity * m.contract_size.unwrap_or(1.0);
            result.push((
                family,
                p.clone(),
                Order {
                    id: format!("creating:{}", p.client_order_id),
                    parent_id: None,
                    status: OrderStatus::Creating,
                    symbol: opts.symbol.clone(),
                    kind: opts.kind,
                    side: opts.side,
                    price: if conditional {
                        opts.trigger_price.unwrap_or(0.0)
                    } else {
                        opts.price.unwrap_or(0.0)
                    },
                    amount: base_amount,
                    filled: 0.0,
                    remaining: base_amount,
                    reduce_only: reduce,
                    close: None,
                    client_order_id: Some(p.client_order_id),
                    time_in_force: opts.time_in_force,
                    last_update: cx.now,
                    trigger_type: opts.trigger_type,
                    take_profit: tp,
                    stop_loss: sl,
                    extra: None,
                },
            ));
        }
        Ok(result)
    }
    fn add_placements(job: &mut Job, rows: Vec<(u8, Placement, Order)>) {
        if !rows.is_empty() {
            job.replacement_targets = job
                .steps
                .iter()
                .filter_map(|step| {
                    if let Effect::Cancel(targets) = &step.effect {
                        Some(targets.clone())
                    } else {
                        None
                    }
                })
                .flatten()
                .collect();
            for target in job.replacement_targets.iter().filter(|t| t.family == 0) {
                job.steps.push_back(Step {
                    receipts: Vec::new(),
                    orders: Vec::new(),
                    path: "/api/v1/trade/order-detail",
                    body: String::new(),
                    effect: Effect::Verify(target.clone()),
                    delay: 0.0,
                });
            }
        }
        let mut normal = Vec::new();
        let mut normal_receipts = Vec::new();
        let mut normal_orders = Vec::new();
        let mut special = Vec::new();
        for (family, p, order) in rows {
            let receipt = OrderIdentityReceipt {
                symbol: order.symbol.clone(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: p.client_order_id.clone(),
            };
            if family == 0 {
                normal.push(p);
                normal_receipts.push(receipt);
                normal_orders.push(order);
            } else {
                special.push((family, p, receipt, order));
            }
        }
        // Keep caller order while respecting BloFin's homogeneous-batch rule.
        let mut start = 0;
        while start < normal.len() {
            let first = &normal[start];
            let count = normal[start..]
                .iter()
                .take(20)
                .take_while(|p| p.inst_id == first.inst_id && p.margin_mode == first.margin_mode)
                .count();
            let batch = &normal[start..start + count];
            let receipts = normal_receipts[start..start + count].to_vec();
            let orders = normal_orders[start..start + count].to_vec();
            start += count;
            job.steps.push_back(Step {
                receipts,
                orders,
                path: "/api/v1/trade/batch-orders",
                body: serde_json::to_string(batch).unwrap(),
                effect: Effect::Place {
                    family: 0,
                    count: batch.len(),
                    inst_id: first.inst_id.clone(),
                },
                delay: 350.0,
            });
        }
        for (family, payload, receipt, order) in special {
            job.steps.push_back(Step {
                receipts: vec![receipt],
                orders: vec![order],
                path: if family == 1 {
                    "/api/v1/trade/order-tpsl"
                } else {
                    "/api/v1/trade/order-algo"
                },
                body: serde_json::to_string(&payload).unwrap(),
                effect: Effect::Place {
                    family,
                    count: 1,
                    inst_id: payload.inst_id.clone(),
                },
                delay: 350.0,
            });
        }
    }
    fn cancel_targets(&self, cx: &Cx<'_, Pending>, ids: &[String]) -> Result<Vec<Target>, String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for id in ids {
            let Some(order) = cx.store.order(id) else {
                let target = self.trading.acknowledged.get(id).ok_or_else(|| {
                    format!("Order {id} not found; cancellation requires a reconciled order")
                })?;
                if seen.insert((target.family, target.id.clone())) {
                    out.push(target.clone());
                }
                continue;
            };
            if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                continue;
            }
            let m = cx
                .store
                .market(&order.symbol)
                .ok_or("Unknown cancel market")?;
            let family = if order.kind == OrderType::Conditional {
                2
            } else if matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit) {
                1
            } else {
                0
            };
            let real = id.split("__").next().unwrap_or(id);
            let source = self.account.sources.get(id);
            if family == 2 && source.is_none_or(|s| s.algo_id != *id || s.inst_id != m.id) {
                return Err("BloFin conditional cancel requires authoritative provenance".into());
            }
            // Inline fallback legs share the normal parent's native cancel target.
            let family = if family == 1
                && source.is_some_and(|s| s.tpsl_id.is_empty() && !s.order_id.is_empty())
            {
                0
            } else {
                family
            };
            if seen.insert((family, real.to_string())) {
                let store_ids = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.id == real || o.id.split("__").next() == Some(real))
                    .map(|o| o.id.clone())
                    .collect();
                let native_order = cx.store.order(real).unwrap_or(order);
                let filled_native = native_order
                    .extra
                    .as_ref()
                    .and_then(|e| e.filled_in_placement_units)
                    .unwrap_or(native_order.filled / m.contract_size.unwrap_or(1.0));
                out.push(Target {
                    id: real.to_string(),
                    inst_id: m.id.clone(),
                    family,
                    store_ids,
                    filled: native_order.filled,
                    filled_native,
                });
            }
        }
        Ok(out)
    }
    fn add_cancels(job: &mut Job, targets: Vec<Target>) {
        for family in 0..3 {
            let rows: Vec<_> = targets
                .iter()
                .filter(|t| t.family == family)
                .cloned()
                .collect();
            for batch in rows.chunks(if family == 0 {
                20
            } else if family == 1 {
                10
            } else {
                1
            }) {
                let payload: Vec<_> = batch
                    .iter()
                    .map(|t| Cancel {
                        inst_id: &t.inst_id,
                        order_id: (family == 0).then_some(t.id.as_str()),
                        tpsl_id: (family == 1).then_some(t.id.as_str()),
                        algo_id: (family == 2).then_some(t.id.as_str()),
                    })
                    .collect();
                job.steps.push_back(Step {
                    receipts: Vec::new(),
                    orders: Vec::new(),
                    path: match family {
                        0 => "/api/v1/trade/cancel-batch-orders",
                        1 => "/api/v1/trade/cancel-tpsl",
                        _ => "/api/v1/trade/cancel-algo",
                    },
                    body: if family == 2 {
                        serde_json::to_string(&payload[0]).unwrap()
                    } else {
                        serde_json::to_string(&payload).unwrap()
                    },
                    effect: Effect::Cancel(batch.to_vec()),
                    delay: if family == 1 { 1000.0 } else { 350.0 },
                });
            }
        }
    }
    fn replacement(
        &self,
        cx: &Cx<'_, Pending>,
        order: &Order,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Vec<(u8, Placement, Order)>, String> {
        if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
            return Err("Cannot amend a terminal BloFin order".into());
        }
        if amount.is_some_and(|v| !v.is_finite() || v < 0.0) {
            return Err("BloFin replacement amount is invalid".into());
        }
        if let Some(price) = price {
            positive(price, "amend price")?;
        }
        let m = cx
            .store
            .market(&order.symbol)
            .ok_or("Unknown replacement market")?;
        if let Some(price) = price {
            positive(num::adjust(price, m.precision.price), "amend price")?;
        }
        let source = self.account.sources.get(&order.id);
        if matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && let Some(parent) = source
                .filter(|s| s.tpsl_id.is_empty() && !s.order_id.is_empty())
                .and_then(|s| cx.store.order(&s.order_id))
        {
            // Editing attached protection cannot resize its entry.
            let mut rows = self.replacement(cx, parent, None, None)?;
            for (_, p, _) in &mut rows {
                let value =
                    num::format(num::adjust(price.unwrap_or(order.price), m.precision.price));
                if order.kind == OrderType::StopLoss {
                    p.protection.sl_trigger_price = Some(value);
                    p.protection
                        .sl_order_price
                        .get_or_insert_with(|| "-1".into());
                } else {
                    p.protection.tp_trigger_price = Some(value);
                    p.protection
                        .tp_order_price
                        .get_or_insert_with(|| "-1".into());
                }
                p.attach_algo_orders = vec![p.protection.clone()];
            }
            return Ok(rows);
        }
        if order.kind == OrderType::Conditional && source.is_none() {
            return Err("BloFin trigger replacement requires provenance".into());
        }
        if let Some(price) = price {
            positive(num::adjust(price, m.precision.price), "amend price")?;
        }
        let qty = amount
            .or_else(|| order.extra.as_ref().and_then(|e| e.remaining_native))
            .unwrap_or(order.remaining / m.contract_size.unwrap_or(1.0));
        let raw = PlaceOrderOpts {
            symbol: order.symbol.clone(),
            kind: order.kind,
            side: order.side,
            amount: if amount.is_none()
                && order.amount <= 0.0
                && matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
            {
                0.0
            } else {
                qty
            },
            price: if order.kind == OrderType::Conditional {
                source
                    .filter(|s| s.order_price > 0.0)
                    .map(|s| s.order_price)
            } else {
                price.or(Some(order.price))
            },
            trigger_price: (order.kind == OrderType::Conditional)
                .then(|| price.unwrap_or(order.price)),
            stop_loss: source.filter(|s| s.sl > 0.0).map(|s| s.sl),
            take_profit: source.filter(|s| s.tp > 0.0).map(|s| s.tp),
            reduce_only: Some(order.reduce_only),
            time_in_force: order.time_in_force,
            trigger_type: order.trigger_type,
            tp_trigger_type: None,
            sl_trigger_type: None,
            client_order_id: None,
            close: None,
            stp_mode: None,
            max_slippage_pct: None,
            bybit_bbo: None,
            bybit_slippage_tolerance_ticks: None,
            implied_volatility: None,
        };
        self.placement(cx, &raw, source)
    }
    pub(crate) fn trading_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        // The owed refresh allocates I/O, so it waits until this command has
        // used its own admission.
        let retired = self.restore_retired(cx);
        self.enter_trade(cx, command);
        if let Some(join) = retired {
            self.refresh_retired(cx, join);
        }
    }
    /// Root first: the Preflight exists before validation, so every refusal is
    /// its own NotSent terminal. A command behind a live job parks in FIFO order.
    fn enter_trade(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.trading.acknowledged.retain(|id, _| {
            !cx.store
                .orders()
                .iter()
                .any(|o| o.id == *id || o.parent_id.as_ref() == Some(id))
        });
        let policy = PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: self
                .trade_identity(CredentialRole::Read { scope: None })
                .map(|(credential, _)| credential),
            write_credential: self
                .trade_identity(CredentialRole::Mutation)
                .map(|(credential, _)| credential),
        };
        // A parked command keeps its caller's wait; it is checked at its wake.
        let deadline = match cx.command_wait() {
            Some(CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Mutation);
                return;
            }
            Some(CommandWait::Remaining(ms)) => Some(cx.now + f64::from(ms)),
            None => None,
        };
        let Ok(preflight) = cx.begin_preflight(policy) else {
            return;
        };
        if let (_, Err(error)) = self.plan_trade(cx, command.clone()) {
            let _ = cx.fail_preflight(&preflight, not_sent(error).kind(ErrorKind::InvalidInput));
            return;
        }
        if self.trading.active.is_some() || !self.trading.queue.is_empty() {
            if let Err(reason) = cx.park_preflight(&preflight) {
                let _ = cx.fail_preflight(&preflight, not_sent(reason.venue_error().message));
                return;
            }
            self.trading.queue.push_back(Queued {
                preflight,
                command,
                deadline,
            });
            return;
        }
        self.start_job(cx, preflight, command);
    }
    fn plan_trade(
        &self,
        cx: &Cx<'_, Pending>,
        command: Command,
    ) -> (CommandId, Result<Job, String>) {
        let command_id = match &command {
            Command::PlaceOrders { command, .. }
            | Command::CancelOrders { command, .. }
            | Command::AmendOrder { command, .. }
            | Command::CancelSymbolOrders { command, .. }
            | Command::SetLeverage { command, .. }
            | Command::SetMarginType { command, .. }
            | Command::SetPositionMode { command, .. } => *command,
            _ => return (0, Err("Unsupported trading command".into())),
        };
        let plan = (|| {
            if !self.signed() {
                return Err("BloFin trading requires API credentials".into());
            }
            if !self.account.mode_loaded || self.account.margin.is_none() {
                return Err("BloFin account settings are not loaded".into());
            }
            let intent = match &command {
                Command::PlaceOrders { .. } => Intent::Place,
                Command::AmendOrder { .. } => Intent::Amend,
                Command::CancelOrders { .. } | Command::CancelSymbolOrders { .. } => Intent::Cancel,
                _ => Intent::Setting,
            };
            let mut job = Job::new(command_id, intent);
            match command {
                Command::PlaceOrders { orders, .. } => {
                    if self.trading.acknowledged.len() >= 4096 {
                        return Err("BloFin unobserved placement capacity exceeded; reconcile before placing".into());
                    }
                    if orders.iter().any(|order| {
                        order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some()
                    }) {
                        return Err(
                            "Bybit BBO and TickSize controls are not supported by BloFin".into(),
                        );
                    }
                    let mut seen = HashSet::new();
                    let mut rows = Vec::new();
                    for opts in &orders {
                        if let Some(id) = &opts.client_order_id
                            && !seen.insert(id)
                        {
                            return Err("Duplicate caller clientOrderId".into());
                        }
                        rows.extend(self.placement(cx, opts, None)?);
                    }
                    // Combine an existing opposite full-position leg before writing.
                    let mut cancel_ids = Vec::new();
                    for (family, p, order) in &mut rows {
                        if *family != 1 || order.amount != 0.0 {
                            continue;
                        }
                        for opposite in cx.store.orders().iter().filter(|o| {
                            !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                                && o.symbol == order.symbol
                                && o.side == order.side
                                && o.amount <= 0.0
                                && matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
                                && o.kind != order.kind
                        }) {
                            p.protection
                                .merge_leg(opposite, self.account.sources.get(&opposite.id));
                            cancel_ids.push(opposite.id.clone());
                        }
                    }
                    Self::add_cancels(&mut job, self.cancel_targets(cx, &cancel_ids)?);
                    Self::add_placements(&mut job, rows);
                }
                Command::CancelOrders { ids, .. } => {
                    let mut preserved = Vec::new();
                    let mut seen = HashSet::new();
                    for id in &ids {
                        if let Some(order) = cx.store.order(id)
                            && !matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled)
                            && order.amount <= 0.0
                            && matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
                        {
                            let real = id.split("__").next().unwrap_or(id);
                            for sibling in cx.store.orders().iter().filter(|o| {
                                !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                                    && o.id != *id
                                    && !ids.contains(&o.id)
                                    && o.id.split("__").next() == Some(real)
                                    && o.amount <= 0.0
                            }) {
                                if seen.insert(sibling.id.clone()) {
                                    let mut rows =
                                        self.replacement(cx, sibling, None, Some(0.0))?;
                                    for (_, p, _) in &mut rows {
                                        p.protection.retain_leg(sibling.kind);
                                        p.attach_algo_orders.clear();
                                    }
                                    preserved.extend(rows);
                                }
                            }
                        }
                    }
                    Self::add_cancels(&mut job, self.cancel_targets(cx, &ids)?);
                    Self::add_placements(&mut job, preserved);
                }
                Command::CancelSymbolOrders { symbol, .. } => {
                    let ids = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| {
                            o.symbol == symbol
                                && !matches!(
                                    o.status,
                                    OrderStatus::Closed
                                        | OrderStatus::Canceled
                                        | OrderStatus::Creating
                                )
                        })
                        .map(|o| o.id.clone())
                        .chain(
                            self.trading
                                .acknowledged
                                .iter()
                                .filter(|(_, t)| {
                                    cx.store.market(&symbol).is_some_and(|m| m.id == t.inst_id)
                                })
                                .map(|(id, _)| id.clone()),
                        )
                        .collect::<Vec<_>>();
                    Self::add_cancels(&mut job, self.cancel_targets(cx, &ids)?);
                }
                Command::AmendOrder {
                    id, price, amount, ..
                } => {
                    let order = cx.store.order(&id).ok_or("Unknown order to amend")?;
                    let mut rows = self.replacement(cx, order, price, amount)?;
                    let real = id.split("__").next().unwrap_or(&id);
                    // Preserve every other protection on a shared native target.
                    for sibling in cx.store.orders().iter().filter(|o| {
                        !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                            && o.id != id
                            && o.id.split("__").next() == Some(real)
                            && matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
                    }) {
                        for (family, p, _) in &mut rows {
                            if *family == 0 || *family == 2 {
                                let protection = if *family == 2 {
                                    if p.attach_algo_orders.is_empty() {
                                        p.attach_algo_orders.push(Protection::default());
                                    }
                                    &mut p.attach_algo_orders[0]
                                } else {
                                    &mut p.protection
                                };
                                protection
                                    .merge_leg(sibling, self.account.sources.get(&sibling.id));
                                if *family == 0 {
                                    p.attach_algo_orders = vec![p.protection.clone()];
                                }
                            } else if sibling.kind != order.kind {
                                p.protection
                                    .merge_leg(sibling, self.account.sources.get(&sibling.id));
                            }
                        }
                    }
                    Self::add_cancels(&mut job, self.cancel_targets(cx, &[id])?);
                    Self::add_placements(&mut job, rows);
                }
                Command::SetLeverage {
                    symbol, leverage, ..
                } => {
                    positive(leverage, "leverage")?;
                    let market = cx.store.market(&symbol).ok_or("Unknown leverage market")?;
                    let leverage = num::clamp_leverage(
                        leverage,
                        market.limits.leverage.min,
                        market.limits.leverage.max,
                    )?;
                    let mode = self.account.margin.unwrap();
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Leverage<'a> {
                        inst_id: &'a str,
                        leverage: String,
                        margin_mode: &'static str,
                        #[serde(skip_serializing_if = "Option::is_none")]
                        position_side: Option<&'static str>,
                    }
                    let sides = if mode == MarginType::Isolated && cx.store.hedged() {
                        vec![Some("long"), Some("short")]
                    } else {
                        vec![None]
                    };
                    job.join = sides.len() == 2;
                    for side in sides {
                        job.steps.push_back(Step {
                            receipts: Vec::new(),
                            orders: Vec::new(),
                            path: "/api/v1/account/set-leverage",
                            body: serde_json::to_string(&Leverage {
                                inst_id: &market.id,
                                leverage: num::format(leverage),
                                margin_mode: margin(mode),
                                position_side: side,
                            })
                            .unwrap(),
                            effect: Effect::Setting,
                            delay: 0.0,
                        });
                    }
                    job.commit = Commit::Leverage { symbol, leverage };
                }
                Command::SetMarginType { isolated, .. } => {
                    let mode = if isolated {
                        MarginType::Isolated
                    } else {
                        MarginType::Cross
                    };
                    job.steps.push_back(Step {
                        receipts: Vec::new(),
                        orders: Vec::new(),
                        path: "/api/v1/account/set-margin-mode",
                        body: format!("{{\"marginMode\":\"{}\"}}", margin(mode)),
                        effect: Effect::Setting,
                        delay: 0.0,
                    });
                    job.commit = Commit::Margin(mode);
                }
                Command::SetPositionMode { hedged, .. } => {
                    job.steps.push_back(Step {
                        receipts: Vec::new(),
                        orders: Vec::new(),
                        path: "/api/v1/account/set-position-mode",
                        body: format!(
                            "{{\"positionMode\":\"{}\"}}",
                            if hedged {
                                "long_short_mode"
                            } else {
                                "net_mode"
                            }
                        ),
                        effect: Effect::Setting,
                        delay: 0.0,
                    });
                    job.commit = Commit::Mode(hedged);
                }
                _ => unreachable!(),
            }
            Ok(job)
        })();
        (command_id, plan)
    }
    /// Plans against the current Store, then admits the first call.
    fn start_job(&mut self, cx: &mut Cx<'_, Pending>, preflight: Preflight, command: Command) {
        let job = match self.plan_trade(cx, command).1 {
            Ok(job) => job,
            Err(error) => {
                let _ =
                    cx.fail_preflight(&preflight, not_sent(error).kind(ErrorKind::InvalidInput));
                return self.next_job(cx);
            }
        };
        if matches!(job.commit, Commit::Margin(_))
            && (cx.store.positions().iter().any(|p| p.contracts > 0.0)
                || cx
                    .store
                    .orders()
                    .iter()
                    .any(|o| o.status == OrderStatus::Open))
        {
            cx.notice(
                Level::Error,
                code::SETTING_CAUTION,
                "Changing margin type with open positions / orders is allowed but can cause unexpected behavior. Symbols with open positions will remain synced to previous mode until position is closed.",
            );
        }
        if job.steps.is_empty() {
            let _ = cx.done_empty_plan(&preflight, "[]".into());
            return self.next_job(cx);
        }
        let active = Active {
            preflight,
            plan: None,
            job,
        };
        self.drive(cx, active, At::Entry);
    }
    /// Advances the active job from `at`: abort on execution progress, admit
    /// or continue the next call, or settle.
    fn drive(&mut self, cx: &mut Cx<'_, Pending>, mut active: Active, at: At<'_>) {
        let current = match &at {
            At::Data(slot, context) => Some((*slot, *context)),
            _ => None,
        };
        let Some(step) = active.job.steps.pop_front() else {
            return self.finish(cx, active, current);
        };
        if matches!(step.effect, Effect::Place { .. } | Effect::Verify(_))
            && active.job.replacement_targets.iter().any(|target| {
                self.filled_native(&target.id) > target.filled_native
                    || cx.store.order(&target.id).is_some_and(|o| {
                        o.status == OrderStatus::Closed || o.filled > target.filled
                    })
            })
        {
            let error = VenueError::new("BloFin replacement aborted after execution progress");
            return self.end(cx, active, current, error);
        }
        let next = serial(&step);
        let slot = match at {
            At::Entry => {
                let Some((credential, _)) = self.trade_identity(CredentialRole::Mutation) else {
                    return self.end(
                        cx,
                        active,
                        None,
                        not_sent("BloFin requires API credentials"),
                    );
                };
                let policy = SerialPolicy {
                    credential: Some(credential),
                    pacing: None,
                    refusals_complete: true,
                };
                match cx.admit_serial_plan(&active.preflight, next, policy) {
                    Ok((plan, slot)) => {
                        active.plan = Some(plan);
                        slot
                    }
                    Err(reason) => {
                        let error = not_sent(reason.venue_error().message);
                        return self.end(cx, active, None, error);
                    }
                }
            }
            At::Data(slot, context) => {
                let plan = active.plan.clone().expect("live Plan");
                match cx.continue_serial_plan(&plan, slot, context, next) {
                    Ok(slot) => slot,
                    Err(reason) => {
                        let error = VenueError::new(reason.venue_error().message);
                        return self.end(cx, active, current, error);
                    }
                }
            }
            At::Ready(slot) => slot,
        };
        self.issue(cx, active, slot, step);
    }
    /// Queues one Plan call, applying native optimistic status after admission
    /// and before release.
    fn issue(&mut self, cx: &mut Cx<'_, Pending>, mut active: Active, slot: Slot, step: Step) {
        let plan = active.plan.clone().expect("admitted Plan");
        let narrowed = matches!(
            (active.job.intent, step.intent()),
            (Intent::Place, Intent::Cancel) | (Intent::Amend, Intent::Place)
        );
        let context = if narrowed {
            cx.plan_leg_context_with_intent(&plan, &slot, step.intent())
        } else {
            cx.plan_leg_context(&plan, &slot)
        };
        let context = match context {
            Ok(context) => context,
            Err(reason) => {
                let error = VenueError::new(reason.venue_error().message);
                return self.end(cx, active, None, error);
            }
        };
        let role = if step.read() {
            CredentialRole::Read { scope: None }
        } else {
            CredentialRole::Mutation
        };
        let Some((_, identity)) = self.trade_identity(role) else {
            return self.end(
                cx,
                active,
                None,
                not_sent("BloFin requires API credentials"),
            );
        };
        match &step.effect {
            Effect::Place { .. } => {
                cx.store.invalidate_order_snapshots();
                for order in &step.orders {
                    cx.store.upsert_order(order.clone());
                    active.job.creating.push(order.id.clone());
                }
            }
            Effect::Cancel(targets) => {
                cx.store.invalidate_order_snapshots();
                for id in targets.iter().flat_map(|t| &t.store_ids) {
                    cx.store.set_order_status(id, OrderStatus::Canceling);
                    active.job.canceling.push(id.clone());
                }
            }
            Effect::Verify(_) | Effect::Setting => {}
        }
        let (method, path) = if let Effect::Verify(target) = &step.effect {
            (
                Method::Get,
                format!(
                    "{}?instId={}&orderId={}",
                    step.path, target.inst_id, target.id
                ),
            )
        } else {
            (Method::Post, step.path.to_owned())
        };
        let body = (method == Method::Post).then(|| step.body.clone());
        let recipe = crate::dispatch_signing::Http::new(
            identity,
            self.options.testnet,
            method,
            path.clone(),
            body.clone(),
        );
        cx.command_progress(active.job.command, step.receipts.clone());
        let url = format!("{}{}", self.host(), path);
        let request = Request {
            command: active.job.command,
            slot,
            context: context.clone(),
            step,
        };
        self.trading.active = Some(active);
        if let Err((pending, reason)) = cx.request_signed_managed(
            context,
            None,
            method,
            url,
            body,
            Box::new(recipe),
            Pending::Trade(Box::new(request)),
        ) && let Pending::Trade(request) = pending
        {
            self.trade_call_failed(cx, *request, reason.venue_error(), false);
        }
    }
    /// Restores the job's optimistic rows: Creating removed, Canceling reopened.
    fn cleanup_job(&mut self, cx: &mut Cx<'_, Pending>, job: &Job) {
        let ids: Vec<_> = job.creating.iter().map(String::as_str).collect();
        cx.store.remove_orders(&ids);
        for id in &job.canceling {
            if cx
                .store
                .order(id)
                .is_some_and(|o| o.status == OrderStatus::Canceling)
            {
                cx.store.set_order_status(id, OrderStatus::Open);
            }
        }
    }
    /// Fails the job under its owner (the journal projects the outcome), then
    /// wakes the next queued command.
    fn end(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        active: Active,
        current: Option<(&Slot, &Context)>,
        error: VenueError,
    ) {
        self.cleanup_job(cx, &active.job);
        match &active.plan {
            None => {
                let failed = cx.fail_preflight(&active.preflight, error);
                debug_assert!(failed.is_ok(), "Preflight terminal stranded");
            }
            Some(plan) => {
                if let Some((slot, context)) = current {
                    let finished = cx.finish_plan_call(plan, slot, context);
                    debug_assert!(finished.is_ok(), "Plan call seal stranded");
                }
                let failed = cx.fail_plan(plan, error);
                debug_assert!(failed.is_ok(), "Plan terminal stranded");
                if active.job.join {
                    self.refresh_account(cx);
                }
            }
        }
        self.next_job(cx);
    }
    fn finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        active: Active,
        current: Option<(&Slot, &Context)>,
    ) {
        self.cleanup_job(cx, &active.job);
        let plan = active.plan.as_ref().expect("a job with calls has a Plan");
        if let Some((slot, context)) = current {
            let finished = cx.finish_plan_call(plan, slot, context);
            debug_assert!(finished.is_ok(), "Plan call seal stranded");
        }
        let ids = serde_json::to_string(&active.job.ids).expect("order ids");
        if cx.done_plan(plan, ids).is_ok() {
            self.commit(cx, &active.job.commit);
        } else {
            let failed = cx.fail_plan(plan, VenueError::new("BloFin job ended without its call"));
            debug_assert!(failed.is_ok(), "Plan terminal stranded");
        }
        self.next_job(cx);
    }
    /// Frame-fed: keep a confirmed margin mode over stale source frames until
    /// the source reports it (`safe_cex_engine::setting_holds`).
    pub(crate) fn hold_margin_over_source(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        ours: Option<MarginType>,
    ) {
        if !cx.setting_held(&margin_hold()) {
            return;
        }
        if self.account.margin == ours {
            cx.release_setting(&margin_hold());
        } else {
            self.account.margin = ours;
        }
    }
    /// Frame-fed: a held margin mode outlasts the source's per-symbol settings.
    pub(crate) fn reapply_held_margin(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(mode) = self
            .account
            .margin
            .filter(|_| cx.setting_held(&margin_hold()))
        {
            margin_on_flat_symbols(cx, mode);
        }
    }
    /// Commit effects apply only after a successful terminal.
    fn commit(&mut self, cx: &mut Cx<'_, Pending>, commit: &Commit) {
        match commit {
            Commit::None => {}
            Commit::Leverage { symbol, leverage } => {
                self.account.settings_revision += 1;
                self.account.positions_revision += 1;
                cx.store.update_setting(symbol, |s| {
                    s.leverage = *leverage;
                    s.buy_leverage = Some(*leverage);
                    s.sell_leverage = Some(*leverage);
                });
                cx.store.update_positions(|p| {
                    if p.symbol == *symbol {
                        p.leverage = *leverage;
                    }
                });
                cx.hold_setting(SettingKey::Leverage(symbol.clone()));
            }
            Commit::Margin(mode) => {
                self.account.settings_revision += 1;
                self.account.positions_revision += 1;
                self.account.margin = Some(*mode);
                margin_on_flat_symbols(cx, *mode);
                cx.hold_setting(margin_hold());
                self.refresh_account(cx);
            }
            Commit::Mode(hedged) => {
                self.account.settings_revision += 1;
                self.account.positions_revision += 1;
                cx.store.set_hedged(*hedged);
                cx.hold_setting(SettingKey::Hedged);
            }
        }
    }
    /// The active job ended: wake the oldest queued Preflight (FIFO).
    fn next_job(&mut self, cx: &mut Cx<'_, Pending>) {
        while self.trading.active.is_none()
            && let Some(queued) = self.trading.queue.front()
        {
            let Err(reason) = cx.request_preflight_ready(&queued.preflight) else {
                return;
            };
            // A front that cannot wake must not stall the FIFO.
            let queued = self.trading.queue.pop_front().expect("queued front");
            let _ = cx.fail_preflight(&queued.preflight, not_sent(reason.venue_error().message));
        }
    }
    pub(crate) fn trading_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &Preflight,
    ) {
        let front = self.trading.active.is_none()
            && self
                .trading
                .queue
                .front()
                .is_some_and(|queued| queued.preflight.owner() == preflight.owner());
        if !front {
            // Woken out of turn: park again, still in FIFO order.
            let _ = cx.park_preflight(preflight);
            return;
        }
        let queued = self.trading.queue.pop_front().expect("queued front");
        if queued.deadline.is_some_and(|deadline| cx.now >= deadline) {
            let error = safe_cex_engine::request_context::Error::TimedOut
                .venue_error()
                .outcome(Outcome::NotSent);
            let _ = cx.fail_preflight(&queued.preflight, error);
            return self.next_job(cx);
        }
        self.start_job(cx, queued.preflight, queued.command);
    }
    /// A native pause ended: issue the next call on the slot it handed back.
    pub(crate) fn trading_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        let Some(active) = self.trading.active.take() else {
            return;
        };
        let slot = active
            .plan
            .as_ref()
            .filter(|own| own.owner() == plan.owner())
            .and_then(|_| cx.plan_slots(plan).ok())
            .and_then(|slots| slots.into_iter().next());
        match slot {
            Some(slot) => self.drive(cx, active, At::Ready(slot)),
            None => self.trading.active = Some(active),
        }
    }
    /// The engine retired a trading owner itself: forget it, keep an active
    /// job for restoration, and name the queued Preflight to wake.
    pub(crate) fn trading_retired(&mut self, owner: &HttpOwnerRetirement) -> Option<Preflight> {
        self.trading
            .queue
            .retain(|queued| !owner.matches_preflight(&queued.preflight));
        if self
            .trading
            .active
            .as_ref()
            .is_some_and(|active| owner.matches_preflight(&active.preflight))
        {
            let active = self.trading.active.take().expect("retired job");
            self.trading.retired = Some(active.job);
        }
        if self.trading.active.is_some() {
            return None;
        }
        self.trading
            .queue
            .front()
            .map(|queued| queued.preflight.clone())
    }
    /// Native restore and reconcile for a job the engine sealed as Unknown.
    pub(crate) fn settle_retired_trade(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(join) = self.restore_retired(cx) {
            self.refresh_retired(cx, join);
        }
    }
    /// The I/O-free half of the settle. `Some(join)` when a refresh is owed.
    fn restore_retired(&mut self, cx: &mut Cx<'_, Pending>) -> Option<bool> {
        let job = self.trading.retired.take()?;
        self.cleanup_job(cx, &job);
        cx.store.invalidate_order_snapshots();
        Some(job.join)
    }
    /// A half-applied hedged join refreshes the account, as native's join did.
    fn refresh_retired(&mut self, cx: &mut Cx<'_, Pending>, join: bool) {
        self.reconcile_orders(cx);
        if join {
            self.refresh_account(cx);
        }
    }
    /// A call failed before or after release; a released write is Unknown.
    pub(crate) fn trade_call_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
        released: bool,
    ) {
        let Some(active) = self.trading.active.take() else {
            return;
        };
        let Some(plan) = active
            .plan
            .clone()
            .filter(|_| active.job.command == request.command)
        else {
            self.trading.active = Some(active);
            return;
        };
        if !request.step.read() {
            if released {
                error.outcome = Some(Outcome::Unknown);
                if request.step.is_place() {
                    cx.store.invalidate_order_snapshots();
                }
            }
            let _ = cx.finish_plan_failed_call(&plan, &request.slot, &request.context);
        }
        self.end(cx, active, None, error);
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let Some(mut active) = self.trading.active.take() else {
            return;
        };
        let delivered = active.job.command == request.command
            && active
                .plan
                .as_ref()
                .is_some_and(|plan| cx.validate_plan_delivery(plan, &request.context).is_ok());
        if !delivered {
            self.trading.active = Some(active);
            return;
        }
        let current = Some((&request.slot, &request.context));
        if let Effect::Verify(target) = &request.step.effect {
            let verified = (200..300).contains(&response.status)
                && check(&response).is_ok()
                && serde_json::from_str::<Data<OneOrMany<OrderDetail<'_>>>>(response.body)
                    .ok()
                    .is_some_and(|reply| {
                        let row = match &reply.data {
                            OneOrMany::One(row) => Some(row),
                            OneOrMany::Many(rows) if rows.len() == 1 => rows.first(),
                            _ => None,
                        };
                        row.is_some_and(|row| {
                            row.inst_id == target.inst_id
                                && row.order_id == target.id
                                && row.state == "canceled"
                                && row.filled_size.parse::<f64>().ok().is_some_and(|filled| {
                                    filled.is_finite()
                                        && filled >= 0.0
                                        && filled == target.filled_native
                                })
                        })
                    });
            return if verified {
                self.drive(cx, active, At::Data(&request.slot, &request.context))
            } else {
                let error = VenueError::new("BloFin replacement final order verification failed");
                self.end(cx, active, current, error)
            };
        }
        if let Err(error) = self.write_answer(cx, &mut active.job, &request, &response) {
            if request.step.is_place() {
                cx.store.invalidate_order_snapshots();
            }
            return self.end(cx, active, current, error);
        }
        // The step's write fact is recorded; a native pause follows only an
        // accepted call with more to send.
        let delay = request.step.delay as u32;
        if delay > 0
            && let Some(next) = active.job.steps.front()
        {
            let plan = active.plan.clone().expect("live Plan");
            let next = serial(next);
            return match cx.continue_serial_plan_after(
                &plan,
                &request.slot,
                &request.context,
                delay,
                next,
            ) {
                Ok(()) => self.trading.active = Some(active),
                Err(reason) => {
                    let error = VenueError::new(reason.venue_error().message);
                    self.end(cx, active, current, error)
                }
            };
        }
        self.drive(cx, active, At::Data(&request.slot, &request.context))
    }
    /// Decodes one write answer, records its journal fact exactly once and
    /// applies native Store effects. `Err` ends the job.
    fn write_answer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        job: &mut Job,
        request: &Request,
        response: &Response<'_>,
    ) -> Result<(), VenueError> {
        let context = &request.context;
        let parsed = serde_json::from_str::<Reply<'_>>(response.body);
        if !(200..300).contains(&response.status) {
            let native = parsed
                .as_ref()
                .ok()
                .filter(|reply| !reply.code.is_empty())
                .map(|reply| native(&reply.code, &reply.msg));
            let error = http_failure(response.status, response.body);
            record_failed(cx, context, native, &error.message);
            return Err(error);
        }
        let reply = match parsed {
            Ok(reply) => reply,
            Err(error) => {
                let message = error.to_string();
                let _ = cx.http_write_unproven(context, message.clone());
                return Err(unknown(message));
            }
        };
        let rows = reply.data.map(ResultRows::rows).unwrap_or_default();
        match &request.step.effect {
            Effect::Place {
                family,
                count,
                inst_id,
            } => {
                let multi_position = reply.code == MULTI_POSITION_CODE
                    || rows.iter().any(|row| row.code == MULTI_POSITION_CODE);
                let message = if multi_position {
                    MULTI_POSITION_MESSAGE.to_owned()
                } else {
                    format!("BloFin placement {}: {}", reply.code, reply.msg)
                };
                if rows.len() != *count {
                    if reply.code != "0" && rows.is_empty() {
                        record_failed(cx, context, Some(native(&reply.code, &reply.msg)), &message);
                        return Err(VenueError::new(message));
                    }
                    let _ = cx.http_write_unproven(context, message.clone());
                    return Err(unknown(message));
                }
                let mut items = Vec::with_capacity(rows.len());
                let mut seen = HashSet::new();
                let mut error = None;
                let mut unproven = false;
                for row in &rows {
                    let id = match family {
                        0 => &row.order_id,
                        1 => &row.tpsl_id,
                        _ => &row.algo_id,
                    };
                    let text = row.msg.trim().to_ascii_lowercase();
                    // Only a non-zero row code refuses; a cancel-only "gone" code
                    // or an unexpected success message proves nothing here.
                    let refused = !row.code.is_empty() && row.code != "0";
                    let gone = matches!(&*row.code, "1000" | "102068");
                    let odd = !matches!(text.as_str(), "" | "success" | "order placed");
                    if refused && !gone {
                        items.push(BatchItem::Native(native(&row.code, &row.msg)));
                        error = Some(format!("BloFin placement rejected: {}", row.msg));
                    } else if refused || id.is_empty() || !seen.insert(id.to_string()) || odd {
                        unproven = true;
                        error = Some(format!("BloFin placement rejected: {}", row.msg));
                    } else {
                        items.push(BatchItem::Accepted(id));
                    }
                }
                let accepted: Vec<String> = items
                    .iter()
                    .filter_map(|item| match item {
                        BatchItem::Accepted(id) => Some((*id).to_owned()),
                        BatchItem::Native(_) => None,
                    })
                    .collect();
                let refused = items.len() != accepted.len();
                // A failed envelope over all-accepted rows is a contradiction.
                unproven |= reply.code != "0" && !refused;
                if multi_position {
                    error = Some(MULTI_POSITION_MESSAGE.to_owned());
                }
                if unproven {
                    let message = error.clone().unwrap_or_else(|| message.clone());
                    let _ = cx.http_write_unproven(context, message);
                } else {
                    let message = error.clone().unwrap_or_default();
                    let _ = cx.http_write_batch(context, &CODES, *count, &items, message, None);
                }
                for id in accepted {
                    if cx.store.order(&id).is_none() {
                        self.trading.acknowledged.insert(
                            id.clone(),
                            Target {
                                id: id.clone(),
                                inst_id: inst_id.clone(),
                                family: *family,
                                store_ids: vec![],
                                filled: 0.0,
                                filled_native: 0.0,
                            },
                        );
                    }
                    job.ids.push(id);
                }
                let error = error.or_else(|| unproven.then_some(message));
                match error {
                    Some(error) if unproven => Err(unknown(error)),
                    Some(error) => Err(VenueError::new(error)),
                    None => Ok(()),
                }
            }
            Effect::Cancel(targets) => {
                if reply.code != "0" && rows.is_empty() {
                    let message = format!("BloFin cancellation rejected: {}", reply.msg);
                    record_failed(cx, context, Some(native(&reply.code, &reply.msg)), &message);
                    return Err(VenueError::new(message));
                }
                let mut matched: Vec<Option<&ResultRow<'_>>> = vec![None; targets.len()];
                let mut unproven = rows.len() != targets.len();
                let mut error =
                    unproven.then(|| "BloFin cancellation response count mismatch".to_owned());
                for row in &rows {
                    let found = targets.iter().position(|t| match t.family {
                        0 => row.order_id == t.id,
                        1 => row.tpsl_id == t.id || row.algo_id == t.id,
                        _ => row.algo_id == t.id,
                    });
                    match found {
                        None => {
                            unproven = true;
                            error =
                                Some("BloFin cancellation returned an unrelated identity".into());
                        }
                        Some(index) if matched[index].is_some() => {
                            unproven = true;
                            error =
                                Some("BloFin cancellation returned duplicate identities".into());
                        }
                        Some(index) => matched[index] = Some(row),
                    }
                }
                let mut items = Vec::with_capacity(targets.len());
                for (target, row) in targets.iter().zip(&matched) {
                    let Some(row) = row else {
                        continue;
                    };
                    // Code-only: the "does not exist" text is 102068's own message.
                    let gone = matches!(&*row.code, "1000" | "102068");
                    let accepted = reply.code == "0" && (row.code.is_empty() || row.code == "0");
                    if gone && !job.replacement_targets.is_empty() {
                        error = Some("BloFin replacement requires a confirmed cancellation".into());
                    }
                    if accepted {
                        items.push(BatchItem::Accepted(&target.id));
                    } else {
                        items.push(BatchItem::Native(native(&row.code, &row.msg)));
                    }
                    if gone || accepted {
                        self.trading.acknowledged.remove(&target.id);
                        cx.store.invalidate_order_snapshots();
                        if target.family != 1 {
                            self.remember_terminal(&target.id);
                        }
                        for id in &target.store_ids {
                            self.remember_terminal(id);
                        }
                        let ids = target
                            .store_ids
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>();
                        if gone {
                            cx.store.remove_orders(&ids);
                        } else {
                            for id in &target.store_ids {
                                if !cx
                                    .store
                                    .order(id)
                                    .is_some_and(|o| o.status == OrderStatus::Closed)
                                {
                                    cx.store.set_order_status(id, OrderStatus::Canceled);
                                }
                            }
                        }
                        for id in &target.store_ids {
                            self.account.sources.remove(id);
                        }
                    } else {
                        error = Some(format!("BloFin cancellation rejected: {}", row.msg));
                    }
                }
                if unproven {
                    let message = error.clone().unwrap_or_default();
                    let _ = cx.http_write_unproven(context, message);
                } else {
                    let message = error.clone().unwrap_or_default();
                    let _ =
                        cx.http_write_batch(context, &CODES, targets.len(), &items, message, None);
                }
                match error {
                    Some(error) if unproven => Err(unknown(error)),
                    Some(error) => Err(VenueError::new(error)),
                    None => Ok(()),
                }
            }
            Effect::Setting => {
                let refused = rows.iter().find(|r| !r.code.is_empty() && r.code != "0");
                if reply.code != "0" || refused.is_some() {
                    let message = format!("BloFin setting rejected: {}", reply.msg);
                    let evidence = match refused {
                        Some(row) if reply.code == "0" => native(&row.code, &row.msg),
                        _ => native(&reply.code, &reply.msg),
                    };
                    record_failed(cx, context, Some(evidence), &message);
                    return Err(VenueError::new(message));
                }
                let accepted = WriteFact::Accepted { order_ids: vec![] };
                let _ = cx.http_write_fact(context, accepted, None);
                Ok(())
            }
            Effect::Verify(_) => unreachable!("Verify is a Read call"),
        }
    }
}

/// BloFin refuses placements in Multi-Position mode with this code.
const MULTI_POSITION_CODE: &str = "102143";
const MULTI_POSITION_MESSAGE: &str = "BloFin Multi-Position mode is not supported in Tealstreet yet. Turn off Multi-Position mode on BloFin, then retry from Tealstreet.";

/// A non-2xx answer's native text. Its outcome and kind are the journal's,
/// from the recorded fact (`CODES`).
fn http_failure(status: u16, body: &str) -> VenueError {
    VenueError::new(format!("BloFin HTTP {status}: {body}"))
}

/// The write may have executed; reconcile before resending.
fn unknown(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// One call of the job's streamed Plan; BloFin never resends a write.
fn serial(step: &Step) -> SerialCall {
    SerialCall {
        auth: Default::default(),
        operation: if step.read() {
            Operation::Read
        } else {
            Operation::Mutation
        },
        timeout_ms: CALL_MS,
        attempts: 1,
        pacing: None,
    }
}

/// BloFin codes are decimal strings; a non-integer stays text evidence.
fn native<'a>(code: &'a str, message: &'a str) -> Native<'a> {
    Native {
        code: decode_int(code).map_or(NativeCode::Text(code), NativeCode::Int),
        message,
    }
}

/// Records one whole failed answer for the delivered write.
fn record_failed(
    cx: &mut Cx<'_, Pending>,
    context: &Context,
    native: Option<Native<'_>>,
    message: &str,
) {
    match cx.classify_write(context, &CODES, native.as_ref(), message.to_owned()) {
        Ok(fact) => {
            let failed = WriteFact::Failed {
                fact,
                known_effects: false,
            };
            let _ = cx.http_write_fact(context, failed, None);
        }
        // A released write never ends factless.
        Err(_) => {
            let _ = cx.http_write_unproven(context, message.to_owned());
        }
    }
}

/// Bounded correlation for a released write: its endpoint. BloFin has no late
/// reconcile codec; the order snapshot reconciles.
pub(crate) fn trade_reconciliation(
    request: &Request,
) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
    ReconciliationData::new(request.step.path.to_owned())
}

/// The account margin mode lives in VENUE_STATE, so it holds under a venue key.
fn margin_hold() -> SettingKey {
    SettingKey::Venue("margin", String::new())
}

/// A margin-mode change applies to every symbol without an open position.
fn margin_on_flat_symbols(cx: &mut Cx<'_, Pending>, mode: MarginType) {
    let symbols = cx
        .store
        .markets()
        .iter()
        .map(|m| m.symbol.clone())
        .collect::<Vec<_>>();
    for symbol in symbols {
        if !cx
            .store
            .positions()
            .iter()
            .any(|p| p.symbol == symbol && p.contracts > 0.0)
        {
            cx.store
                .update_setting(&symbol, |s| s.margin_type = Some(mode));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(code: &str, intent: Intent) -> Verdict {
        CODES.verdict(&native(code, ""), intent)
    }

    #[test]
    fn the_code_table_holds_the_shared_invariants() {
        safe_cex_engine::classify::assert_table(&CODES);
    }

    #[test]
    fn cited_codes_refuse_and_unlisted_codes_prove_nothing() {
        for code in [
            "152001", "152015", "102015", "102037", "102055", "103003", "110019",
        ] {
            assert!(verdict(code, Intent::Place).is_refusal(), "{code}");
        }
        assert_eq!(
            verdict("152401", Intent::Setting),
            Verdict::Refused(ErrorKind::AuthInvalid)
        );
        for code in ["102036", "102056", "152016", "999999", "1", "2"] {
            assert_eq!(
                verdict(code, Intent::Place),
                Verdict::Unknown(None),
                "{code}"
            );
        }
        assert_eq!(
            verdict("103013", Intent::Cancel),
            Verdict::Unknown(Some(ErrorKind::Internal))
        );
    }

    #[test]
    fn gone_is_satisfied_only_for_a_cancel() {
        for code in ["1000", "102068"] {
            assert_eq!(
                verdict(code, Intent::Cancel),
                Verdict::Satisfied(SatisfiedIntent::Cancel)
            );
            for intent in [Intent::Place, Intent::Amend, Intent::Setting] {
                assert_eq!(verdict(code, intent), Verdict::Unknown(None), "{code}");
            }
        }
    }

    #[test]
    fn no_exact_row_lies_inside_a_range_of_its_scope() {
        // So first-match order can never decide a verdict (assert_table also
        // refuses overlap; this states the reason the table relies on).
        for row in CODES.rows {
            let Key::Int(code) = row.key else { continue };
            for other in CODES.rows {
                if let Key::Range(low, high) = other.key {
                    assert!(
                        row.only != other.only || !(low..=high).contains(&code),
                        "{code} inside {low}-{high}"
                    );
                }
            }
        }
    }

    #[test]
    fn exact_rows_beside_ranges_keep_their_own_kinds() {
        // 102064 sits just past the 102037-102055 range; 102143 is the native
        // Multi-Position refusal, typed Unsupported rather than the range's kind.
        assert_eq!(
            verdict("102064", Intent::Place),
            Verdict::Refused(ErrorKind::Rejected)
        );
        assert_eq!(
            verdict("102143", Intent::Place),
            Verdict::Refused(ErrorKind::Unsupported)
        );
        assert_eq!(
            verdict("152420", Intent::Place),
            Verdict::Refused(ErrorKind::InvalidInput)
        );
    }
}
