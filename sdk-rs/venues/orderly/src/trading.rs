use crate::{
    Orderly, Pending,
    wire::{Id, N, Rest, checked},
};
use safe_cex_engine::Level;
use safe_cex_engine::classify::{
    CodeTable, Intent, Key, Native, NativeCode, Row, Verdict, WriteFact,
};
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::plan::{
    CallBound, RetryWindow, SerialCall, SerialPolicy, Slot, StepBound,
};
use safe_cex_engine::request_context::{
    ConditionOwner, ConditionWait, Context, HttpOwnerRetirement, Operation, Plan, PlanPolicy,
    Policy, Preflight, PreflightPolicy, PreflightPurpose, ReconciliationData, RetryPolicy,
};
use safe_cex_engine::{
    Command, CommandId, CommandWait, Cx, ErrorKind, Method, Outcome, Response, VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

/// A typed NotSent refusal under the command's own Mutation Context, for a
/// command refused before it can own a Preflight.
fn refuse_command(cx: &mut Cx<'_, Pending>, error: VenueError) {
    match cx.command_wait() {
        Some(CommandWait::Remaining(_)) => {}
        Some(CommandWait::Expired) => {
            let _ = cx.reject_expired_command(Operation::Mutation);
            return;
        }
        None => {
            let _ = cx.reject_unstamped_command(Operation::Mutation);
            return;
        }
    }
    let Ok(context) = cx.admit_http_command(Policy {
        operation: Operation::Mutation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }) else {
        return;
    };
    let _ = cx.fail_context(&context, error);
}

/// Each call's own bound; the streamed Plan has no whole-job deadline.
const CALL_MS: u32 = 20_000;
/// Native's protective-confirmation bound after a positional pair write.
const PAIR_WAIT_MS: u32 = 20_000;

/// Orderly's documented error codes (orderly.network/docs/build-on-omnichain/
/// evm-api/error-codes, read 2026-10-26). Cited refusals are pre-execution;
/// -1000/-1011/-1012 and the -1007 duplicate prove nothing, and anything
/// unlisted proves nothing.
pub(crate) const CODES: CodeTable = CodeTable {
    rows: &[
        refused(Key::Int(-1001), ErrorKind::AuthInvalid),
        refused(Key::Int(-1002), ErrorKind::AuthInvalid),
        refused(Key::Int(-1003), ErrorKind::RateLimited),
        refused(Key::Range(-1006, -1004), ErrorKind::Rejected),
        refused(Key::Range(-1009, -1008), ErrorKind::Rejected),
        refused(Key::Range(-1105, -1101), ErrorKind::Rejected),
        refused(Key::Range(-1202, -1201), ErrorKind::Rejected),
        unproven(-1000),
        unproven(-1007),
        unproven(-1011),
        unproven(-1012),
    ],
    success: &[],
    unlisted: Verdict::Unknown(None),
    // The documented HTTP statuses of the refusals above.
    status: &[
        (400, Verdict::Refused(ErrorKind::Rejected)),
        (401, Verdict::Refused(ErrorKind::AuthInvalid)),
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
const fn unproven(code: i64) -> Row {
    Row {
        key: Key::Int(code),
        verdict: Verdict::Unknown(None),
        only: None,
    }
}

pub struct Request {
    pub command: CommandId,
    step: Step,
    plan: Plan,
    slot: Slot,
    context: Context,
}
impl Request {
    /// A read (FetchLeverage, a protective verification) has no write outcome.
    pub(crate) fn is_read(&self) -> bool {
        self.step.method == Method::Get
    }
}
impl Step {
    fn is_verify(&self) -> bool {
        matches!(self.effect, Effect::Verify(_))
    }
}
#[derive(Clone)]
struct Step {
    method: Method,
    path: String,
    body: Option<String>,
    effect: Effect,
}
#[derive(Clone)]
enum Effect {
    Place(Order),
    Pair {
        legs: usize,
        symbol: String,
        side: OrderSide,
        mode: MarginType,
    },
    Cancel {
        orders: Vec<Order>,
        parent: Option<String>,
    },
    Amend {
        id: String,
    },
    Leverage {
        symbol: String,
        mode: MarginType,
        value: f64,
    },
    Margin {
        symbol: String,
        mode: MarginType,
    },
    FetchLeverage {
        symbol: String,
        mode: MarginType,
    },
    /// The REST snapshot that settles a job ending on a confirmed pair.
    Verify(Waiting),
}
struct Job {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    /// Bulk cancellation's two families are independent writes.
    keep_going: bool,
    /// The first failure names the job's error; the journal owns its outcome.
    error: Option<VenueError>,
    collect: bool,
    /// A placement job: its replacement cancels narrow to Cancel legs.
    place: bool,
    scopes: HashSet<String>,
    superseded: bool,
}
/// The running job: its Preflight root, its Plan once admitted, a native pause
/// between calls and the call in flight (restored if the engine retires it).
struct Active {
    preflight: Preflight,
    plan: Option<Plan>,
    job: Job,
    parked: Option<(ConditionWait, Waiting)>,
    flight: Option<Step>,
}
struct Queued {
    preflight: Preflight,
    command: Command,
    /// The caller's wait, when it set one; checked at the Preflight's wake.
    deadline: Option<f64>,
    /// Bulk cancellation superseded it: it fails NotSent at its own wake.
    superseded: bool,
}
enum At<'a> {
    Entry,
    Data(&'a Slot, &'a Context),
    Ready(Slot),
    /// An independent call failed; the next one still goes.
    Next,
}
/// What a delivered, accepted answer asks of the job.
enum Answer {
    Next,
    Wait(Waiting),
    Done(String),
}
#[derive(Default)]
pub struct Trading {
    active: Option<Active>,
    queue: VecDeque<Queued>,
    detached: Vec<(Waiting, HashSet<String>)>,
    /// A call the engine sealed itself, with its job's scopes: restore it as
    /// lost at the next callback.
    retired: Option<(Step, HashSet<String>)>,
}
fn mode(m: MarginType) -> &'static str {
    if m == MarginType::Isolated {
        "ISOLATED"
    } else {
        "CROSS"
    }
}
fn side_name(s: OrderSide) -> &'static str {
    if s == OrderSide::Buy { "BUY" } else { "SELL" }
}
fn trade_id(command: &Command) -> Option<CommandId> {
    match command {
        Command::PlaceOrders { command, .. }
        | Command::CancelOrders { command, .. }
        | Command::AmendOrder { command, .. }
        | Command::CancelSymbolOrders { command, .. }
        | Command::SetLeverage { command, .. }
        | Command::SetMarginType { command, .. }
        | Command::SetPositionMode { command, .. }
        | Command::FetchLeverage { command, .. } => Some(*command),
        _ => None,
    }
}
fn live(order: &Order) -> bool {
    matches!(
        order.status,
        OrderStatus::Open | OrderStatus::Creating | OrderStatus::Updating | OrderStatus::Canceling
    )
}
fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}
fn client_id_valid(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && id.len() <= 36
        && id.is_ascii()
        && !id
            .chars()
            .any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
}
fn native_id(id: &str) -> Result<(), String> {
    if id.starts_with('0') || id.parse::<u64>().is_err() {
        Err("Orderly requires an exact positive decimal order ID".into())
    } else {
        Ok(())
    }
}
#[derive(Serialize)]
struct Entry<'a> {
    symbol: &'a str,
    order_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    side: &'a str,
    margin_mode: &'a str,
    order_tag: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_order_id: Option<&'a str>,
    order_quantity: f64,
    broker_id: &'a str,
}
#[derive(Serialize)]
struct Child {
    reduce_only: bool,
    algo_type: &'static str,
    r#type: &'static str,
    side: &'static str,
    trigger_price: String,
}
#[derive(Serialize)]
struct Pair<'a> {
    reduce_only: bool,
    symbol: &'a str,
    algo_type: &'static str,
    margin_mode: &'a str,
    child_orders: Vec<Child>,
    #[serde(rename = "brokerId")]
    broker_id: &'a str,
}

impl Orderly {
    fn broker_id(&self) -> &'static str {
        if self.options.testnet {
            "e708a644-9ce5-46c6-b50c-74b15f62d8ca"
        } else {
            "ab82cb09-cfec-4473-80a3-b740779d0644"
        }
    }
    fn resolve_mode(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
        protective: bool,
    ) -> Result<MarginType, String> {
        if !protective && opts.reduce_only != Some(true) {
            return Ok(self.selected_mode(cx, &opts.symbol));
        }
        let closing = if opts.side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let rows: Vec<_> = cx
            .store
            .positions()
            .iter()
            .filter(|p| p.symbol == opts.symbol && p.side == closing && p.contracts > 0.0)
            .collect();
        if let Some(selected) = self.account.defaults.get(&opts.symbol)
            && rows
                .iter()
                .filter(|p| p.margin_type == Some(*selected))
                .count()
                == 1
        {
            return Ok(*selected);
        }
        if rows.len() > 1 {
            return Err("Orderly reduce-only order is ambiguous across Cross and Isolated positions; select a margin mode first".into());
        }
        Ok(rows
            .first()
            .and_then(|p| p.margin_type)
            .unwrap_or(self.selected_mode(cx, &opts.symbol)))
    }
    fn pair(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        side: OrderSide,
        mode_: MarginType,
        stop: Option<f64>,
        take: Option<f64>,
    ) -> Result<Step, String> {
        let market = cx.store.market(symbol).ok_or("Orderly unknown market")?;
        let mut children = vec![];
        for (kind, price) in [("STOP_LOSS", stop), ("TAKE_PROFIT", take)] {
            if let Some(price) = price {
                let adjusted = num::adjust(price, market.precision.price);
                if !price.is_finite() || adjusted <= 0.0 {
                    return Err("Orderly protective price must be finite and positive".into());
                }
                children.push(Child {
                    reduce_only: true,
                    algo_type: kind,
                    r#type: "CLOSE_POSITION",
                    side: side_name(side),
                    trigger_price: num::format(adjusted),
                });
            }
        }
        if children.is_empty() {
            return Err("Orderly positional order requires a protective price".into());
        }
        let legs = children.len();
        Ok(Step {
            method: Method::Post,
            path: "/v1/algo/order".into(),
            body: Some(json(&Pair {
                reduce_only: true,
                symbol: &self.native[symbol],
                algo_type: "POSITIONAL_TP_SL",
                margin_mode: mode(mode_),
                child_orders: children,
                broker_id: self.broker_id(),
            })),
            effect: Effect::Pair {
                legs,
                symbol: symbol.into(),
                side,
                mode: mode_,
            },
        })
    }
    fn replace_pair(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        side: OrderSide,
        mode_: MarginType,
        mut stop: Option<f64>,
        mut take: Option<f64>,
    ) -> Result<VecDeque<Step>, String> {
        let mut siblings = vec![];
        for o in cx.store.orders().iter().filter(|o| {
            o.symbol == symbol
                && o.side == side
                && live(o)
                && matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
        }) {
            let proof = self
                .account
                .proofs
                .get(&o.id)
                .ok_or("Orderly protective order lacks margin provenance")?;
            // Quantity TP_SL and other external algos are independent protections.
            // Only proven full-position legs participate in a parent rebuild.
            if !proof.positional_repair {
                if proof.close_compatible
                    || o.parent_id
                        .as_ref()
                        .is_some_and(|root| self.account.created_pairs.contains_key(root))
                {
                    return Err(
                        "Orderly positional parent lacks consistent full-position provenance"
                            .into(),
                    );
                }
                continue;
            }
            if o.parent_id
                .as_ref()
                .and_then(|root| self.account.created_pairs.get(root))
                .is_some_and(|(_, _, mode)| *mode != proof.mode)
            {
                return Err(
                    "Orderly protective margin provenance conflicts with accepted creation".into(),
                );
            }
            if proof.mode == mode_ {
                siblings.push(o.clone());
            }
        }
        let parents: HashSet<_> = siblings
            .iter()
            .filter_map(|o| o.parent_id.clone())
            .collect();
        if parents.len() > 1
            || siblings.len() > 2
            || siblings
                .iter()
                .any(|o| o.parent_id.is_none() || o.kind == OrderType::TrailingStopLoss)
        {
            return Err(
                "Orderly positional pair is ambiguous; cancel existing parents first".into(),
            );
        }
        if cx.store.orders().iter().any(|order| {
            live(order)
                && order
                    .parent_id
                    .as_ref()
                    .is_some_and(|parent| parents.contains(parent))
                && !siblings.iter().any(|sibling| sibling.id == order.id)
        }) {
            return Err(
                "Orderly cannot rebuild a parent containing unrelated protective siblings".into(),
            );
        }
        let mut seen = HashSet::new();
        for o in &siblings {
            let key = if o.kind == OrderType::StopLoss { 0 } else { 1 };
            if !seen.insert(key) {
                return Err("Orderly duplicate protective sibling".into());
            }
            if key == 0 && stop.is_none() {
                stop = Some(o.price);
            } else if key == 1 && take.is_none() {
                take = Some(o.price);
            }
        }
        let mut steps = VecDeque::new();
        if !siblings.is_empty() {
            steps.push_back(self.cancel(siblings, parents.into_iter().next())?);
        }
        steps.push_back(self.pair(cx, symbol, side, mode_, stop, take)?);
        Ok(steps)
    }
    fn cancel(&self, orders: Vec<Order>, parent: Option<String>) -> Result<Step, String> {
        let first = orders
            .first()
            .ok_or("Orderly missing cancellation identity")?;
        let id = parent.as_deref().unwrap_or(&first.id);
        native_id(id)?;
        let path = format!(
            "/v1/{}order?order_id={id}&symbol={}",
            if parent.is_some() { "algo/" } else { "" },
            self.native[&first.symbol]
        );
        Ok(Step {
            method: Method::Delete,
            path,
            body: None,
            effect: Effect::Cancel { orders, parent },
        })
    }
    fn validate_order(&self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<(), String> {
        let market = cx.store.market(&o.symbol).ok_or("Orderly unknown symbol")?;
        if (matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
            || o.stop_loss.is_some()
            || o.take_profit.is_some())
            && self
                .trading
                .detached
                .iter()
                .any(|(_, scopes)| scopes.contains(&o.symbol))
        {
            return Err(
                "Orderly protective state is still reconciling after an interrupted command".into(),
            );
        }
        if !market.active
            || !o.amount.is_finite()
            || o.amount <= 0.0
            || num::adjust(o.amount, market.precision.amount) < market.limits.amount.min
        {
            return Err("Orderly amount must meet the market minimum".into());
        }
        if matches!(o.kind, OrderType::TrailingStopLoss | OrderType::Conditional) {
            return Err(
                "Orderly opening triggers and trailing-stop placement are unsupported".into(),
            );
        }
        if o.trigger_price.is_some()
            && !(matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
                && o.trigger_price == o.price)
        {
            return Err("Orderly positional triggerPrice must equal price on SL/TP orders".into());
        }
        if o.close == Some(true) && matches!(o.kind, OrderType::Limit | OrderType::Market) {
            return Err("Orderly close flag is unsupported on ordinary orders".into());
        }
        for trigger in [o.trigger_type, o.tp_trigger_type, o.sl_trigger_type]
            .into_iter()
            .flatten()
        {
            if trigger != StopTriggerType::MarkPrice {
                return Err("Orderly only supports MarkPrice protective triggers".into());
            }
        }
        if o.kind != OrderType::Limit
            && o.time_in_force
                .is_some_and(|t| t != TimeInForce::GoodTillCancel)
        {
            return Err("Orderly time-in-force requires a limit order".into());
        }
        if matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && (o.stop_loss.is_some() || o.take_profit.is_some())
        {
            return Err("Orderly stop orders cannot carry another bracket".into());
        }
        for price in [
            if o.kind == OrderType::Market {
                None
            } else {
                o.price
            },
            o.stop_loss,
            o.take_profit,
        ]
        .into_iter()
        .flatten()
        {
            if !price.is_finite() || num::adjust(price, market.precision.price) <= 0.0 {
                return Err("Orderly price must be finite and positive".into());
            }
        }
        if o.kind != OrderType::Market && o.price.is_none() {
            return Err("Orderly price is required".into());
        }
        if let Some(id) = &o.client_order_id
            && (!client_id_valid(id)
                || !matches!(o.kind, OrderType::Limit | OrderType::Market)
                || o.stop_loss.is_some()
                || o.take_profit.is_some()
                || o.amount > market.limits.amount.max)
        {
            return Err("Orderly clientOrderId must be up to36 characters, cannot begin with -, and must identify one ordinary native order".into());
        }
        self.resolve_mode(
            cx,
            o,
            matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit),
        )?;
        Ok(())
    }
    fn plan_orders(
        &self,
        cx: &Cx<'_, Pending>,
        orders: Vec<PlaceOrderOpts>,
    ) -> Result<VecDeque<Step>, String> {
        let mut ids = HashSet::new();
        let mut positional = HashSet::new();
        for o in &orders {
            self.validate_order(cx, o)?;
            if let Some(id) = &o.client_order_id
                && !ids.insert(id.clone())
            {
                return Err("Orderly duplicate caller clientOrderId".into());
            }
        }
        let mut steps = VecDeque::new();
        for o in orders {
            let market = cx.store.market(&o.symbol).unwrap();
            let protective = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
            let mode_ = self.resolve_mode(cx, &o, protective)?;
            if protective {
                if !positional.insert((o.symbol.clone(), mode_ == MarginType::Isolated)) {
                    return Err(
                        "Orderly batch cannot replace the same positional pair twice".into(),
                    );
                }
                steps.extend(self.replace_pair(
                    cx,
                    &o.symbol,
                    o.side,
                    mode_,
                    (o.kind == OrderType::StopLoss).then_some(o.price.unwrap()),
                    (o.kind == OrderType::TakeProfit).then_some(o.price.unwrap()),
                )?);
                continue;
            }
            let amount = num::adjust(o.amount, market.precision.amount);
            let units = (amount / market.precision.amount).round();
            let cap = (market.limits.amount.max / market.precision.amount).floor();
            if !units.is_finite()
                || !cap.is_finite()
                || cap < 1.0
                || units > 9007199254740991.0
                || (units / cap).ceil() > 100.0
            {
                return Err("Orderly order exceeds supported split bounds".into());
            }
            let mut remaining = units;
            let mut index = 0;
            while remaining > 0.0 {
                let lot = remaining.min(cap);
                let qty = num::adjust(lot * market.precision.amount, market.precision.amount);
                if qty < market.limits.amount.min {
                    return Err("Orderly split leaves a below-minimum remainder".into());
                }
                let price = (o.kind == OrderType::Limit)
                    .then(|| num::adjust(o.price.unwrap(), market.precision.price));
                let order_type = if o.kind == OrderType::Market {
                    "MARKET"
                } else {
                    match o.time_in_force.unwrap_or(TimeInForce::GoodTillCancel) {
                        TimeInForce::GoodTillCancel => "LIMIT",
                        TimeInForce::PostOnly => "POST_ONLY",
                        TimeInForce::ImmediateOrCancel => "IOC",
                        TimeInForce::FillOrKill => "FOK",
                    }
                };
                let body = json(&Entry {
                    symbol: &self.native[&o.symbol],
                    order_type,
                    order_price: price,
                    reduce_only: o.reduce_only,
                    side: side_name(o.side),
                    margin_mode: mode(mode_),
                    order_tag: self.options.broker.order_tag(),
                    client_order_id: o.client_order_id.as_deref(),
                    order_quantity: qty,
                    broker_id: self.broker_id(),
                });
                let order = Order {
                    id: format!("pending:{index}"),
                    parent_id: None,
                    status: OrderStatus::Creating,
                    symbol: o.symbol.clone(),
                    kind: o.kind,
                    side: o.side,
                    price: price.unwrap_or(0.0),
                    amount: qty,
                    filled: 0.0,
                    remaining: qty,
                    reduce_only: o.reduce_only.unwrap_or(false),
                    close: None,
                    client_order_id: o.client_order_id.clone(),
                    time_in_force: (o.kind == OrderType::Limit)
                        .then_some(o.time_in_force.unwrap_or(TimeInForce::GoodTillCancel)),
                    last_update: cx.now,
                    trigger_type: None,
                    take_profit: None,
                    stop_loss: None,
                    extra: None,
                };
                steps.push_back(Step {
                    method: Method::Post,
                    path: "/v1/order".into(),
                    body: Some(body),
                    effect: Effect::Place(order),
                });
                remaining -= lot;
                index += 1;
            }
            if o.stop_loss.is_some() || o.take_profit.is_some() {
                if !positional.insert((o.symbol.clone(), mode_ == MarginType::Isolated)) {
                    return Err(
                        "Orderly batch cannot replace the same positional pair twice".into(),
                    );
                }
                steps.extend(self.replace_pair(
                    cx,
                    &o.symbol,
                    o.side.opposite(),
                    mode_,
                    o.stop_loss,
                    o.take_profit,
                )?);
            }
        }
        Ok(steps)
    }
    fn plan_cancel(
        &self,
        cx: &Cx<'_, Pending>,
        ids: Vec<String>,
    ) -> Result<VecDeque<Step>, String> {
        let mut unique = HashSet::new();
        let ids: Vec<_> = ids
            .into_iter()
            .filter(|id| unique.insert(id.clone()))
            .collect();
        let requested: HashSet<_> = ids.iter().cloned().collect();
        let mut steps = VecDeque::new();
        let mut parents = HashSet::new();
        for id in &ids {
            let order = cx
                .store
                .orders()
                .iter()
                .find(|o| &o.id == id && live(o))
                .or_else(|| {
                    cx.store
                        .orders()
                        .iter()
                        .find(|o| o.parent_id.as_ref() == Some(id) && live(o))
                })
                .or_else(|| {
                    cx.store
                        .orders()
                        .iter()
                        .find(|o| &o.id == id || o.parent_id.as_ref() == Some(id))
                })
                .ok_or("Orderly cancel requires a reconciled order")?;
            if !live(order) {
                continue;
            }
            if let Some(parent) = &order.parent_id {
                if !parents.insert(parent.clone()) {
                    continue;
                }
                let siblings: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        o.parent_id.as_ref() == Some(parent) && o.symbol == order.symbol && live(o)
                    })
                    .cloned()
                    .collect();
                let mode_ = self
                    .account
                    .proofs
                    .get(&order.id)
                    .ok_or("Orderly algo cancellation requires margin provenance")?
                    .mode;
                let survivors: Vec<_> = siblings
                    .iter()
                    .filter(|o| !requested.contains(parent) && !requested.contains(&o.id))
                    .collect();
                let mut stop = None;
                let mut take = None;
                for o in survivors {
                    if self
                        .account
                        .proofs
                        .get(&o.id)
                        .is_none_or(|p| p.mode != mode_ || !p.positional_repair)
                        || o.side != order.side
                    {
                        return Err("Orderly cannot safely recreate this sibling; cancel the whole parent instead".into());
                    }
                    match o.kind {
                        OrderType::StopLoss => {
                            if stop.replace(o.price).is_some() {
                                return Err("Orderly duplicate protective sibling".into());
                            }
                        }
                        OrderType::TakeProfit => {
                            if take.replace(o.price).is_some() {
                                return Err("Orderly duplicate protective sibling".into());
                            }
                        }
                        _ => return Err("Orderly cannot safely recreate this algo sibling".into()),
                    }
                }
                steps.push_back(self.cancel(siblings, Some(parent.clone()))?);
                if stop.is_some() || take.is_some() {
                    steps.push_back(self.pair(cx, &order.symbol, order.side, mode_, stop, take)?);
                }
            } else {
                steps.push_back(self.cancel(vec![order.clone()], None)?);
            }
        }
        Ok(steps)
    }
    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: String,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<VecDeque<Step>, String> {
        let o = cx
            .store
            .orders()
            .iter()
            .find(|o| o.id == id)
            .ok_or("Orderly amend requires one reconciled order")?;
        if !live(o) || price.is_some() == amount.is_some() {
            return Err("Orderly amend requires one real price or quantity change".into());
        }
        native_id(&id)?;
        let market = cx.store.market(&o.symbol).unwrap();
        let mut steps = VecDeque::new();
        if let Some(parent) = &o.parent_id {
            native_id(parent)?;
            let p = price.ok_or("Orderly positional stops cannot amend quantity")?;
            if !matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
                || !p.is_finite()
                || p <= 0.0
                || num::adjust(p, market.precision.price) != p
                || p == o.price
            {
                return Err(
                    "Orderly algo amendment requires a new aligned protective price".into(),
                );
            }
            #[derive(Serialize)]
            struct Child<'a> {
                order_id: &'a str,
                trigger_price: f64,
            }
            #[derive(Serialize)]
            struct Edit<'a> {
                order_id: u64,
                child_orders: [Child<'a>; 1],
            }
            steps.push_back(Step {
                method: Method::Put,
                path: "/v1/algo/order".into(),
                body: Some(json(&Edit {
                    order_id: parent.parse().unwrap(),
                    child_orders: [Child {
                        order_id: &id,
                        trigger_price: p,
                    }],
                })),
                effect: Effect::Amend { id },
            });
            return Ok(steps);
        }
        let proof = self
            .account
            .proofs
            .get(&id)
            .ok_or("Orderly native amend requires complete REST provenance")?;
        if !proof.amend
            || o.kind != OrderType::Limit
            || o.status != OrderStatus::Open
            || o.filled != 0.0
            || o.remaining != o.amount
            || o.time_in_force != Some(TimeInForce::GoodTillCancel)
        {
            return Err("Orderly native amend requires a REST-proven pending zero-fill ordinary GTC LIMIT order".into());
        }
        let (value, old, step) = if let Some(p) = price {
            (p, o.price, market.precision.price)
        } else {
            (amount.unwrap(), o.amount, market.precision.amount)
        };
        if !value.is_finite()
            || value <= 0.0
            || value == old
            || num::adjust(value, step) != value
            || amount.is_some_and(|q| q < market.limits.amount.min || q > market.limits.amount.max)
        {
            return Err("Orderly amendment must be new, aligned and within market limits".into());
        }
        #[derive(Serialize)]
        struct Edit<'a> {
            order_id: &'a str,
            symbol: &'a str,
            order_type: &'a str,
            side: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            order_price: Option<f64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            order_quantity: Option<f64>,
        }
        steps.push_back(Step {
            method: Method::Put,
            path: "/v1/order".into(),
            body: Some(json(&Edit {
                order_id: &id,
                symbol: &self.native[&o.symbol],
                order_type: "LIMIT",
                side: side_name(o.side),
                order_price: price,
                order_quantity: amount,
            })),
            effect: Effect::Amend { id },
        });
        Ok(steps)
    }
    fn plan_trade(&self, cx: &Cx<'_, Pending>, command: Command) -> Result<Job, String> {
        let id = trade_id(&command).unwrap();
        let scopes: HashSet<String> = match &command {
            Command::PlaceOrders { orders, .. } => {
                orders.iter().map(|o| o.symbol.clone()).collect()
            }
            Command::CancelOrders { ids, .. } => ids
                .iter()
                .filter_map(|id| {
                    cx.store
                        .orders()
                        .iter()
                        .find(|o| o.id == *id || o.parent_id.as_ref() == Some(id))
                        .map(|o| o.symbol.clone())
                })
                .collect(),
            Command::AmendOrder { id, .. } => cx
                .store
                .orders()
                .iter()
                .find(|o| o.id == *id)
                .map(|o| HashSet::from([o.symbol.clone()]))
                .unwrap_or_default(),
            Command::CancelSymbolOrders { symbol, .. }
            | Command::SetLeverage { symbol, .. }
            | Command::SetMarginType { symbol, .. }
            | Command::FetchLeverage { symbol, .. } => HashSet::from([symbol.clone()]),
            _ => HashSet::from([String::new()]),
        };

        let mut keep_going = false;
        let place = matches!(command, Command::PlaceOrders { .. });
        let collect = matches!(
            command,
            Command::PlaceOrders { .. } | Command::AmendOrder { .. }
        );
        let steps = match command {
            Command::PlaceOrders { orders, .. } => self.plan_orders(cx, orders)?,
            Command::CancelOrders { ids, .. } => self.plan_cancel(cx, ids)?,
            Command::AmendOrder {
                id, price, amount, ..
            } => self.plan_amend(cx, id, price, amount)?,
            Command::CancelSymbolOrders { symbol, .. } => {
                if !symbol.is_empty() && !self.native.contains_key(&symbol) {
                    return Err("Orderly unknown cancel symbol".into());
                }
                keep_going = true;
                [false, true]
                    .into_iter()
                    .map(|algo| {
                        let orders = cx
                            .store
                            .orders()
                            .iter()
                            .filter(|o| {
                                (symbol.is_empty() || o.symbol == symbol)
                                    && o.parent_id.is_some() == algo
                                    && live(o)
                            })
                            .cloned()
                            .collect();
                        Step {
                            method: Method::Delete,
                            path: format!(
                                "/v1/{}orders{}",
                                if algo { "algo/" } else { "" },
                                if symbol.is_empty() {
                                    String::new()
                                } else {
                                    format!("?symbol={}", self.native[&symbol])
                                }
                            ),
                            body: None,
                            effect: Effect::Cancel {
                                orders,
                                parent: None,
                            },
                        }
                    })
                    .collect()
            }
            Command::SetLeverage {
                symbol, leverage, ..
            } => {
                if !leverage.is_finite() {
                    return Err("Orderly leverage must be finite".into());
                }
                let (lo, hi, mode_) = if symbol.is_empty() {
                    (1.0, 100.0, MarginType::Cross)
                } else {
                    let m = cx.store.market(&symbol).ok_or("Orderly unknown symbol")?;
                    (
                        m.limits.leverage.min,
                        m.limits.leverage.max,
                        self.selected_mode(cx, &symbol),
                    )
                };
                let value = num::round_half_up(num::clamp_leverage(leverage, lo, hi)?);
                #[derive(Serialize)]
                struct Set<'a> {
                    leverage: f64,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    symbol: Option<&'a str>,
                    margin_mode: &'a str,
                }
                VecDeque::from([Step {
                    method: Method::Post,
                    path: "/v1/client/leverages".into(),
                    body: Some(json(&Set {
                        leverage: value,
                        symbol: self.native.get(&symbol).map(String::as_str),
                        margin_mode: mode(mode_),
                    })),
                    effect: Effect::Leverage {
                        symbol,
                        mode: mode_,
                        value,
                    },
                }])
            }
            Command::SetMarginType {
                symbol, isolated, ..
            } => {
                let native = self
                    .native
                    .get(&symbol)
                    .ok_or("Orderly margin type requires a known symbol")?;
                let mode_ = if isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                #[derive(Serialize)]
                struct Set<'a> {
                    symbol: &'a str,
                    default_margin_mode: &'a str,
                }
                VecDeque::from([Step {
                    method: Method::Post,
                    path: "/v1/client/margin_mode".into(),
                    body: Some(json(&Set {
                        symbol: native,
                        default_margin_mode: mode(mode_),
                    })),
                    effect: Effect::Margin {
                        symbol,
                        mode: mode_,
                    },
                }])
            }
            Command::SetPositionMode { hedged, .. } => {
                if hedged {
                    return Err("Orderly position mode is fixed OneWay".into());
                }
                VecDeque::new()
            }
            Command::FetchLeverage { symbol, .. } => {
                let native = self
                    .native
                    .get(&symbol)
                    .ok_or("Orderly unknown leverage symbol")?;
                let mode_ = self.selected_mode(cx, &symbol);
                VecDeque::from([Step {
                    method: Method::Get,
                    path: format!(
                        "/v1/client/leverage?symbol={native}&margin_mode={}",
                        mode(mode_)
                    ),
                    body: None,
                    effect: Effect::FetchLeverage {
                        symbol,
                        mode: mode_,
                    },
                }])
            }
            _ => return Err("Orderly invalid trading command".into()),
        };
        if steps
            .iter()
            .any(|s| matches!(s.effect, Effect::Pair { .. }))
            && (self.trading.detached.len() >= 100
                || self
                    .trading
                    .detached
                    .iter()
                    .any(|(_, blocked)| scopes.iter().any(|s| blocked.contains(s))))
        {
            return Err(
                "Orderly protective state requires reconciliation before another write".into(),
            );
        }
        Ok(Job {
            command: id,
            steps,
            ids: vec![],
            keep_going,
            error: None,
            collect,
            place,
            scopes,
            superseded: false,
        })
    }
    /// The Primary trades; Orderly has one account key.
    fn trade_identity(
        &self,
    ) -> Option<(
        safe_cex_engine::request_context::Credential,
        safe_cex_engine::signing::Identity,
    )> {
        let slots = Slots::new(
            self.options
                .identities
                .primary
                .clone()
                .filter(|_| !self.options.public_data_only),
            None,
            SlotPolicy {
                secondary_requires_primary: true,
            },
        );
        let (selection, identity) = slots.select(CredentialRole::Mutation).ok()?;
        Some((
            selection.credential(identity.credential.clone()),
            identity.clone(),
        ))
    }
    /// Root first: every trading command owns a Preflight before validation,
    /// so each refusal is its own NotSent terminal. A command behind a live
    /// job parks in FIFO order.
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.settle_retired(cx);
        if trade_id(&command).is_none() {
            return;
        }
        if let Command::PlaceOrders { orders, .. } = &command
            && orders
                .iter()
                .any(|o| o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some())
        {
            let error = not_sent("Bybit BBO and TickSize controls are not supported by Orderly")
                .kind(ErrorKind::Unsupported);
            return refuse_command(cx, error);
        }
        // A keyless account has no credential to own a write Preflight.
        let Some((credential, _)) = self.trade_identity() else {
            return refuse_command(cx, not_sent("Orderly private account is not ready"));
        };
        let deadline = match cx.command_wait() {
            Some(CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Mutation);
                return;
            }
            Some(CommandWait::Remaining(ms)) => Some(cx.now + f64::from(ms)),
            None => None,
        };
        // FetchLeverage keeps its native FIFO place, as a read.
        let read = matches!(command, Command::FetchLeverage { .. });
        let policy = PreflightPolicy {
            purpose: if read {
                PreflightPurpose::ReadOnly
            } else {
                PreflightPurpose::WriteAfterReads
            },
            read_credential: Some(credential.clone()),
            write_credential: (!read).then_some(credential),
        };
        let Ok(preflight) = cx.begin_preflight(policy) else {
            return;
        };
        if !self.signed()
            || !cx.store.loaded().orders
            || !cx.store.loaded().positions
            || !cx.store.loaded().balance
        {
            let _ = cx.fail_preflight(&preflight, not_sent("Orderly private account is not ready"));
            return;
        }
        if let Command::CancelSymbolOrders { symbol, .. } = &command {
            let symbol = symbol.clone();
            self.supersede_queued(cx, &symbol);
            if let Some(active) = self.trading.active.take() {
                return self.supersede_active(cx, active, &symbol, preflight, command, deadline);
            }
        }
        if self.trading.queue.len() >= 100 {
            let _ = cx.fail_preflight(&preflight, not_sent("Orderly trading queue is full"));
            return;
        }
        if self.trading.active.is_some() || !self.trading.queue.is_empty() {
            return self.park(cx, preflight, command, deadline, false);
        }
        self.start_job(cx, preflight, command);
    }
    fn park(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: Preflight,
        command: Command,
        deadline: Option<f64>,
        front: bool,
    ) {
        if let Err(reason) = cx.park_preflight(&preflight) {
            let _ = cx.fail_preflight(&preflight, not_sent(reason.venue_error().message));
            return;
        }
        let queued = Queued {
            preflight,
            command,
            deadline,
            superseded: false,
        };
        if front {
            self.trading.queue.push_front(queued);
        } else {
            self.trading.queue.push_back(queued);
        }
    }
    /// Bulk cancellation supersedes queued order mutations on its scope. A
    /// parked Preflight fails only in its own ready custody, so each is woken.
    fn supersede_queued(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        for queued in &mut self.trading.queue {
            let in_scope = |id: &String, children: bool| {
                symbol.is_empty()
                    || cx
                        .store
                        .orders()
                        .iter()
                        .find(|o| o.id == *id || (children && o.parent_id.as_ref() == Some(id)))
                        .is_none_or(|o| o.symbol == symbol)
            };
            let affected = match &queued.command {
                Command::PlaceOrders { orders, .. } => orders
                    .iter()
                    .any(|o| symbol.is_empty() || o.symbol == symbol),
                Command::CancelOrders { ids, .. } => ids.iter().any(|id| in_scope(id, true)),
                Command::AmendOrder { id, .. } => in_scope(id, false),
                _ => false,
            };
            if affected && !queued.superseded {
                queued.superseded = true;
                let _ = cx.request_preflight_ready(&queued.preflight);
            }
        }
    }
    /// The live job on the bulk cancellation's scope sends nothing more; the
    /// cancellation runs next, at the front of the FIFO.
    fn supersede_active(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut active: Active,
        symbol: &str,
        preflight: Preflight,
        command: Command,
        deadline: Option<f64>,
    ) {
        let affected = symbol.is_empty()
            || active.job.scopes.contains(symbol)
            || active.job.scopes.contains("");
        let mut ended = None;
        if affected && !active.job.keep_going {
            active.job.superseded = true;
            active.job.steps.clear();
            // The current call may already be in flight, so nothing is proven.
            active.job.error = Some(VenueError::new(
                "Orderly remaining writes were superseded by bulk cancellation",
            ));
            if let Some((_, waiting)) = active.parked.take() {
                self.trading
                    .detached
                    .push((waiting, active.job.scopes.clone()));
                ended = active.job.error.take();
            }
        }
        self.park(cx, preflight, command, deadline, true);
        match ended {
            Some(error) => self.end(cx, active, None, error),
            None => self.trading.active = Some(active),
        }
    }
    /// Plans against the current Store, then admits the first call.
    fn start_job(&mut self, cx: &mut Cx<'_, Pending>, preflight: Preflight, command: Command) {
        let id = trade_id(&command).expect("trading command");
        let job = match self.plan_trade(cx, command) {
            Ok(job) => job,
            Err(error) => {
                cx.command_error(id, &error);
                let _ = cx.fail_preflight(&preflight, not_sent(error));
                return self.next_job(cx);
            }
        };
        if job.steps.is_empty() {
            let _ = cx.done_empty_plan(&preflight, "[]".into());
            return self.next_job(cx);
        }
        let active = Active {
            preflight,
            plan: None,
            job,
            parked: None,
            flight: None,
        };
        self.drive(cx, active, At::Entry);
    }
    /// Advances the active job from `at`: admit or continue the next call, or settle.
    fn drive(&mut self, cx: &mut Cx<'_, Pending>, mut active: Active, at: At<'_>) {
        let current = match &at {
            At::Data(slot, context) => Some((*slot, *context)),
            _ => None,
        };
        let Some(step) = active.job.steps.pop_front() else {
            return self.finish(cx, active, current);
        };
        let independent = active.job.keep_going;
        let slot = match at {
            At::Entry => {
                let Some((credential, _)) = self.trade_identity() else {
                    let error = not_sent("Orderly private account is not ready");
                    return self.end(cx, active, None, error);
                };
                let admitted = if step.method == Method::Get {
                    // FetchLeverage: one Read call under its read-only Preflight.
                    admit_read(cx, &active.preflight, credential)
                        .and_then(|plan| first_slot(cx, &plan).map(|slot| (plan, slot)))
                } else if independent {
                    admit_independent(
                        cx,
                        &active.preflight,
                        credential,
                        active.job.steps.len() + 1,
                    )
                    .and_then(|plan| first_slot(cx, &plan).map(|slot| (plan, slot)))
                } else {
                    let policy = SerialPolicy {
                        credential: Some(credential),
                        pacing: None,
                        refusals_complete: false,
                    };
                    cx.admit_serial_plan(&active.preflight, serial(&step), policy)
                };
                match admitted {
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
                let next = if independent {
                    cx.finish_plan_call(&plan, slot, context)
                        .and_then(|()| first_slot(cx, &plan))
                } else {
                    cx.continue_serial_plan(&plan, slot, context, serial(&step))
                };
                match next {
                    Ok(slot) => slot,
                    Err(reason) => {
                        let error = VenueError::new(reason.venue_error().message);
                        return self.end(cx, active, None, error);
                    }
                }
            }
            At::Ready(slot) => slot,
            At::Next => {
                let plan = active.plan.clone().expect("live Plan");
                match first_slot(cx, &plan) {
                    Ok(slot) => slot,
                    Err(reason) => {
                        let error = VenueError::new(reason.venue_error().message);
                        return self.end(cx, active, None, error);
                    }
                }
            }
        };
        self.issue(cx, active, slot, step);
    }
    /// Queues one Plan call, applying native optimistic status after admission
    /// and before release.
    fn issue(&mut self, cx: &mut Cx<'_, Pending>, mut active: Active, slot: Slot, mut step: Step) {
        let plan = active.plan.clone().expect("admitted Plan");
        // A placement's conflicting-order cancel is a real cancel leg.
        let context = if active.job.place && matches!(step.effect, Effect::Cancel { .. }) {
            cx.plan_leg_context_with_intent(&plan, &slot, Intent::Cancel)
        } else {
            cx.plan_leg_context(&plan, &slot)
        };
        let identity = self.trade_identity().map(|(_, identity)| identity);
        let (context, identity) = match (context, identity) {
            (Ok(context), Some(identity)) => (context, identity),
            // A verification leg follows an accepted pair: never NotSent.
            _ if step.is_verify() => {
                let Effect::Verify(waiting) = step.effect else {
                    unreachable!()
                };
                return self.unconfirmed(cx, active, None, Some(waiting));
            }
            (Err(reason), _) => {
                let error = VenueError::new(reason.venue_error().message);
                return self.end(cx, active, None, error);
            }
            (Ok(_), None) => {
                let error = not_sent("Orderly private account is not ready");
                return self.end(cx, active, None, error);
            }
        };
        let command = active.job.command;
        match &mut step.effect {
            Effect::Place(order) => {
                order.id = format!("pending:{command}");
                cx.store.upsert_order(order.clone());
                self.account.order_revisions[0] += 1;
                self.account.positions_revision += 1;
                if let Some(identity) = &order.client_order_id {
                    cx.command_progress(
                        command,
                        vec![OrderIdentityReceipt {
                            symbol: order.symbol.clone(),
                            kind: OrderIdentityKind::ClientOrderId,
                            identity: identity.clone(),
                        }],
                    );
                }
            }
            Effect::Pair { .. } => {
                self.account.order_revisions[1] += 1;
            }
            Effect::Cancel { orders, .. } => {
                for o in orders {
                    cx.store.update_order(&o.id, |p| {
                        if live(p) {
                            p.status = OrderStatus::Canceling;
                        }
                    });
                }
                self.account.order_revisions[0] += 1;
                self.account.order_revisions[1] += 1;
            }
            Effect::Amend { id } => {
                cx.store
                    .update_order(id, |o| o.status = OrderStatus::Updating);
                self.account.order_revisions[0] += 1;
                self.account.order_revisions[1] += 1;
                self.account.proofs.remove(id);
            }
            Effect::Leverage { .. } | Effect::Margin { .. } => {
                self.account.settings_revision += 1;
                self.account.positions_revision += 1;
            }
            Effect::FetchLeverage { .. } | Effect::Verify(_) => {}
        }
        let origin = self.rest();
        let account = self.options.application_id.clone().unwrap_or_default();
        let recipe = crate::sign::Http::new(identity, &account, step.method, origin, &step.path);
        let (method, url, body) = (
            step.method,
            format!("{origin}{}", step.path),
            step.body.clone(),
        );
        active.flight = Some(step.clone());
        let request = Request {
            command,
            step,
            plan,
            slot,
            context: context.clone(),
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
    /// Fails the job under its owner (the journal projects the outcome), then
    /// wakes the next queued command.
    fn end(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        active: Active,
        current: Option<(&Slot, &Context)>,
        error: VenueError,
    ) {
        match &active.plan {
            None => {
                let failed = cx.fail_preflight(&active.preflight, error);
                debug_assert!(failed.is_ok(), "Preflight terminal stranded");
            }
            Some(plan) => {
                if let Some((slot, context)) = current {
                    let _ = cx.finish_plan_call(plan, slot, context);
                }
                let failed = cx.fail_plan(plan, error);
                debug_assert!(failed.is_ok(), "Plan terminal stranded");
            }
        }
        self.next_job(cx);
    }
    fn finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut active: Active,
        current: Option<(&Slot, &Context)>,
    ) {
        let plan = active.plan.clone().expect("a job with calls has a Plan");
        if let Some((slot, context)) = current {
            let finished = cx.finish_plan_call(&plan, slot, context);
            debug_assert!(finished.is_ok(), "Plan call seal stranded");
        }
        match active.job.error.take() {
            Some(error) => {
                let failed = cx.fail_plan(&plan, error);
                debug_assert!(failed.is_ok(), "Plan terminal stranded");
            }
            None => {
                let result = if active.job.collect {
                    json(&active.job.ids)
                } else {
                    "[]".into()
                };
                if cx.done_plan(&plan, result).is_err() {
                    let error = VenueError::new("Orderly job ended without its call");
                    let failed = cx.fail_plan(&plan, error);
                    debug_assert!(failed.is_ok(), "Plan terminal stranded");
                }
            }
        }
        self.next_job(cx);
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
        // A call the engine retired settles before anything plans against the Store.
        self.settle_retired(cx);
        if let Some(index) =
            self.trading.queue.iter().position(|queued| {
                queued.superseded && queued.preflight.owner() == preflight.owner()
            })
        {
            let queued = self.trading.queue.remove(index).expect("superseded entry");
            let _ = cx.fail_preflight(
                &queued.preflight,
                not_sent("Orderly queued order mutation was superseded by bulk cancellation"),
            );
            return self.next_job(cx);
        }
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
    /// A protective pause released: issue the next call on the slot it handed back.
    pub(crate) fn trading_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        // A call the engine retired settles before anything plans against the Store.
        self.settle_retired(cx);
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
    /// Native: an unconfirmed protective state after the bound sends no
    /// further write, and its scope stays fenced until reconciled.
    pub(crate) fn trading_condition_expired(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        owner: &ConditionOwner,
    ) {
        // A call the engine retired settles before anything plans against the Store.
        self.settle_retired(cx);
        let ours = self
            .trading
            .active
            .as_ref()
            .is_some_and(|active| match owner {
                ConditionOwner::Preflight(preflight) => {
                    active.preflight.owner() == preflight.owner()
                }
                ConditionOwner::Plan(plan) => active
                    .plan
                    .as_ref()
                    .is_some_and(|own| own.owner() == plan.owner()),
            });
        if !ours {
            return;
        }
        let active = self.trading.active.take().expect("active job");
        self.unconfirmed(cx, active, None, None);
    }
    /// The engine retired a trading owner itself: forget it, keep the call in
    /// flight for restoration, and name the queued Preflight to wake.
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
            let mut active = self.trading.active.take().expect("retired job");
            if let Some((_, waiting)) = active.parked.take() {
                self.trading
                    .detached
                    .push((waiting, active.job.scopes.clone()));
            }
            self.trading.retired = active.flight.map(|step| (step, active.job.scopes));
        }
        if self.trading.active.is_some() {
            return None;
        }
        self.trading
            .queue
            .front()
            .map(|queued| queued.preflight.clone())
    }
    /// A retired call settles as a lost one: its rows stay uncertain and the
    /// account reconciles, as native's lost write did.
    pub(crate) fn settle_retired(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some((step, scopes)) = self.trading.retired.take() {
            self.undo_step(cx, step, true, scopes);
            self.account_after_uncertain_write(cx);
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
        self.settle_retired(cx);
        let Some(mut active) = self.trading.active.take() else {
            return;
        };
        let ours = active.job.command == request.command
            && active
                .plan
                .as_ref()
                .is_some_and(|plan| plan.owner() == request.plan.owner());
        if !ours {
            self.trading.active = Some(active);
            return;
        }
        active.flight = None;
        if let Effect::Verify(waiting) = request.step.effect {
            return self.unconfirmed(cx, active, None, Some(waiting));
        }
        let write = !request.is_read();
        if !write {
            error.outcome = None;
        } else if released {
            error = error.kind(ErrorKind::Network).outcome(Outcome::Unknown);
        }
        if write {
            let _ = cx.finish_plan_failed_call(&request.plan, &request.slot, &request.context);
        }
        self.call_failed(cx, active, request.step, error, write && released, None);
    }
    /// Native failure bookkeeping, then the job's next move: an independent
    /// job continues, any other ends with its first error.
    fn call_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut active: Active,
        step: Step,
        error: VenueError,
        uncertain: bool,
        current: Option<(&Slot, &Context)>,
    ) {
        let lost = uncertain || error.outcome == Some(Outcome::Unknown);
        self.undo_step(cx, step, lost, active.job.scopes.clone());
        cx.command_error(active.job.command, &error.message);
        let first = active.job.error.get_or_insert(error);
        if lost {
            first.outcome = Some(Outcome::Unknown);
        }
        if lost {
            self.account_after_uncertain_write(cx);
        }
        if active.job.keep_going {
            let plan = active.plan.clone().expect("live Plan");
            if let Some((slot, context)) = current {
                let _ = cx.finish_plan_call(&plan, slot, context);
            }
            return self.drive(cx, active, At::Next);
        }
        let error = active.job.error.take().expect("first error");
        self.end(cx, active, current, error);
    }
    /// Native restoration of a failed call's optimistic rows.
    fn undo_step(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        step: Step,
        uncertain: bool,
        scopes: HashSet<String>,
    ) {
        match step.effect {
            Effect::Place(order) if !uncertain => cx.store.remove_orders(&[&order.id]),
            Effect::Place(order) => {
                self.account.uncertain_placements.insert(order.id);
            }
            Effect::Cancel { orders, .. } if !uncertain => {
                for original in orders {
                    cx.store.update_order(&original.id, |o| {
                        if o.status == OrderStatus::Canceling {
                            o.status = original.status;
                        }
                    });
                }
            }
            Effect::Pair { legs, .. } if uncertain => {
                // A lost response cannot prove the pair was rejected.
                self.trading.detached.push((
                    Waiting {
                        parent: String::new(),
                        ids: vec![],
                        legs,
                        present: true,
                        snapshot: self.account.algo_snapshots,
                    },
                    scopes,
                ));
                self.account.order_revisions[1] += 1;
                self.account_poll(cx, true);
            }
            Effect::Amend { id } if !uncertain => {
                cx.store.update_order(&id, |o| {
                    if o.status == OrderStatus::Updating {
                        o.status = OrderStatus::Open;
                    }
                });
            }
            _ => {}
        }
    }
    /// After the call's write fact: pause until the stream confirms the pair,
    /// then issue the next call (a final REST verification when none is left).
    fn pause(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut active: Active,
        request: &Request,
        waiting: Waiting,
    ) {
        if active.job.superseded {
            self.trading
                .detached
                .push((waiting, active.job.scopes.clone()));
            return self.drive(cx, active, At::Data(&request.slot, &request.context));
        }
        if active.job.steps.is_empty()
            && let Some(verify) = self.verify_step(&request.step, &waiting)
        {
            active.job.steps.push_back(verify);
        }
        let Some(next) = active.job.steps.front().map(serial) else {
            return self.drive(cx, active, At::Data(&request.slot, &request.context));
        };
        let plan = active.plan.clone().expect("live Plan");
        match cx.serial_plan_condition_wait(
            &plan,
            &request.slot,
            &request.context,
            next,
            Some(PAIR_WAIT_MS),
        ) {
            Ok(wait) => {
                active.parked = Some((wait, waiting));
                self.trading.active = Some(active);
                self.account_poll(cx, true);
            }
            // The pair or parent cancel was accepted: never a plain failure.
            Err(_) => self.unconfirmed(
                cx,
                active,
                Some((&request.slot, &request.context)),
                Some(waiting),
            ),
        }
    }
    /// An accepted pair whose legs cannot be confirmed: the Plan inherits
    /// uncertainty (Unknown, accepted IDs kept) and the scope stays fenced.
    fn unconfirmed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut active: Active,
        current: Option<(&Slot, &Context)>,
        waiting: Option<Waiting>,
    ) {
        if let Some(waiting) = waiting.or_else(|| active.parked.take().map(|(_, w)| w)) {
            self.trading
                .detached
                .push((waiting, active.job.scopes.clone()));
        }
        if let Some(plan) = &active.plan {
            let _ = cx.inherit_plan_uncertainty(plan);
        }
        self.account_after_uncertain_write(cx);
        let error = VenueError::new(
            "Orderly protective state is unconfirmed; no subsequent write was sent",
        )
        .outcome(Outcome::Unknown);
        self.end(cx, active, current, error);
    }
    /// The account's open-algo listing (the native poll's own read), read
    /// under the Plan after a pair write or a pair's parent cancel.
    fn verify_step(&self, step: &Step, waiting: &Waiting) -> Option<Step> {
        if !matches!(step.effect, Effect::Pair { .. } | Effect::Cancel { .. }) {
            return None;
        }
        Some(Step {
            method: Method::Get,
            path: "/v1/algo/orders?status=INCOMPLETE&page=1&size=500".into(),
            body: None,
            effect: Effect::Verify(waiting.clone()),
        })
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        self.settle_retired(cx);
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
        active.flight = None;
        let current = Some((&request.slot, &request.context));
        let write = !request.is_read();
        if write && (response.status == 408 || (500..600).contains(&response.status)) {
            let message = format!("Orderly HTTP {}: {}", response.status, response.body);
            let _ = cx.http_write_unproven(&request.context, message.clone());
            let error = VenueError::new(message).outcome(Outcome::Unknown);
            return self.call_failed(cx, active, request.step.clone(), error, true, current);
        }
        // A successful envelope with an unusable ACK cannot prove a write was rejected.
        // The IDs this answer adds, for its own write fact.
        let ids_before = active.job.ids.len();
        let unknown_ack = write
            && (200..300).contains(&response.status)
            && serde_json::from_str::<Rest<'_>>(response.body).map_or(true, |rest| rest.success);
        let result = (|| -> Result<Answer, String> {
            let rest = checked(response)?;
            let data = rest.data.map_or("{}", |d| d.get());
            match &request.step.effect {
                Effect::Place(order) => {
                    #[derive(Deserialize)]
                    struct Created {
                        order_id: Id,
                    }
                    let r: Created = serde_json::from_str(data).map_err(|e| e.to_string())?;
                    native_id(&r.order_id.0)?;
                    cx.store.remove_orders(&[&order.id]);
                    let mut placed = order.clone();
                    placed.id = r.order_id.0.clone();
                    placed.status = OrderStatus::Open;
                    if !self.order_terminal(&placed.id)
                        && !cx.store.orders().iter().any(|o| o.id == placed.id)
                    {
                        cx.store.upsert_order(placed);
                    }
                    self.account.order_revisions[0] += 1;
                    active.job.ids.push(r.order_id.0);
                }
                Effect::Pair {
                    legs,
                    symbol,
                    side,
                    mode,
                } => {
                    #[derive(Deserialize)]
                    struct Row {
                        #[serde(rename = "orderId")]
                        id: Id,
                    }
                    #[derive(Deserialize)]
                    struct Created {
                        order_id: Option<Id>,
                        #[serde(default)]
                        rows: Vec<Row>,
                    }
                    let r: Created = serde_json::from_str(data).map_err(|e| e.to_string())?;
                    let parent = r
                        .order_id
                        .as_ref()
                        .map(|id| id.0.clone())
                        .unwrap_or_default();
                    let ids = if let Some(id) = r.order_id {
                        vec![id.0]
                    } else {
                        r.rows.into_iter().map(|r| r.id.0).collect()
                    };
                    if ids.is_empty() {
                        return Err("Orderly algo success omitted native order IDs".into());
                    }
                    for id in &ids {
                        native_id(id)?;
                    }
                    if !parent.is_empty() {
                        if self.account.created_pairs.len() >= 1000 {
                            self.account.created_pairs.clear();
                        }
                        self.account
                            .created_pairs
                            .insert(parent.clone(), (symbol.clone(), *side, *mode));
                        for order in cx.store.orders().iter().filter(|o| {
                            o.parent_id.as_ref() == Some(&parent)
                                && o.symbol == *symbol
                                && o.side == *side
                                && matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
                        }) {
                            if let Some(proof) = self.account.proofs.get_mut(&order.id)
                                && proof.close_compatible
                                && (!proof.mode_known || proof.mode == *mode)
                            {
                                proof.mode = *mode;
                                proof.mode_known = true;
                                proof.positional_repair = true;
                            }
                        }
                    }
                    active.job.ids.extend(ids.clone());
                    self.account.order_revisions[1] += 1;
                    if !self.pair_known(cx, &parent, &ids, *legs) {
                        return Ok(Answer::Wait(Waiting {
                            parent,
                            ids,
                            legs: *legs,
                            present: true,
                            snapshot: self.account.algo_snapshots,
                        }));
                    }
                }
                Effect::Cancel { orders, parent } => {
                    #[derive(Deserialize)]
                    struct Ack<'a> {
                        #[serde(default, borrow)]
                        status: std::borrow::Cow<'a, str>,
                    }
                    let ack: Ack<'_> = serde_json::from_str(data).map_err(|e| e.to_string())?;
                    if !matches!(
                        ack.status.as_ref(),
                        "CANCEL_SENT" | "CANCEL_ALL_SENT" | "CANCELED" | "CANCELLED"
                    ) {
                        return Err(
                            "Orderly cancellation success omitted a valid native ACK".into()
                        );
                    }
                    self.account.order_revisions[0] += 1;
                    self.account.order_revisions[1] += 1;
                    if matches!(ack.status.as_ref(), "CANCELED" | "CANCELLED") {
                        if let Some(parent) = parent {
                            self.terminal_order(&format!("root:{parent}"));
                        }
                        for o in orders {
                            self.terminal_order(&o.id);
                            cx.store.update_order(&o.id, |p| {
                                if p.status != OrderStatus::Closed {
                                    p.status = OrderStatus::Canceled;
                                }
                            });
                        }
                    }
                    if let Some(parent) = parent
                        && active
                            .job
                            .steps
                            .front()
                            .is_some_and(|s| matches!(s.effect, Effect::Pair { .. }))
                        && !self.root_finished(cx, parent)
                    {
                        return Ok(Answer::Wait(Waiting {
                            parent: parent.clone(),
                            ids: vec![],
                            legs: 0,
                            present: false,
                            snapshot: self.account.algo_snapshots,
                        }));
                    }
                }
                Effect::Amend { id } => {
                    #[derive(Deserialize)]
                    struct Ack<'a> {
                        #[serde(default, borrow)]
                        status: std::borrow::Cow<'a, str>,
                    }
                    if request.step.path == "/v1/order"
                        && serde_json::from_str::<Ack<'_>>(data)
                            .map_err(|e| e.to_string())?
                            .status
                            != "EDIT_SENT"
                    {
                        return Err("Orderly native amend requires EDIT_SENT".into());
                    }
                    cx.store.update_order(id, |o| {
                        if o.status == OrderStatus::Updating {
                            o.status = OrderStatus::Open;
                        }
                    });
                    self.account.order_revisions[0] += 1;
                    self.account.order_revisions[1] += 1;
                    active.job.ids.push(id.clone());
                }
                Effect::Leverage {
                    symbol,
                    mode: mode_,
                    value,
                } => {
                    self.account.settings_revision += 1;
                    self.account.positions_revision += 1;
                    if symbol.is_empty() {
                        let previous = self.account.fallback_leverage;
                        self.account.fallback_leverage = *value;
                        for s in cx
                            .store
                            .markets()
                            .iter()
                            .map(|m| m.symbol.clone())
                            .collect::<Vec<_>>()
                        {
                            self.account
                                .leverage
                                .entry((s.clone(), true))
                                .or_insert(previous);
                            let m = cx.store.market(&s).unwrap();
                            let effective = match num::clamp_leverage(
                                *value,
                                m.limits.leverage.min,
                                m.limits.leverage.max,
                            ) {
                                Ok(effective) => effective,
                                Err(_) => {
                                    // The account write succeeded independently of market metadata.
                                    cx.notice(Level::Warn, code::SETTING_CAUTION, format!("Orderly invalid leverage limits for {s}; using confirmed account leverage"));
                                    *value
                                }
                            };
                            self.account.leverage.insert((s.clone(), false), effective);
                            self.sync_setting(cx, &s);
                        }
                    } else {
                        self.account
                            .leverage
                            .insert((symbol.clone(), *mode_ == MarginType::Isolated), *value);
                        self.sync_setting(cx, symbol);
                    }
                    cx.store.update_positions(|p| {
                        if (symbol.is_empty() || p.symbol == *symbol)
                            && p.margin_type == Some(*mode_)
                        {
                            p.leverage = self
                                .account
                                .leverage
                                .get(&(p.symbol.clone(), *mode_ == MarginType::Isolated))
                                .copied()
                                .unwrap_or(*value);
                        }
                    });
                }
                Effect::Margin {
                    symbol,
                    mode: mode_,
                } => {
                    self.account.settings_revision += 1;
                    self.account.defaults.insert(symbol.clone(), *mode_);
                    self.sync_setting(cx, symbol);
                }
                Effect::Verify(waiting) => {
                    #[derive(Deserialize)]
                    struct Node {
                        #[serde(default)]
                        algo_order_id: Option<Id>,
                        #[serde(default)]
                        order_id: Option<Id>,
                        #[serde(default)]
                        child_orders: Vec<Node>,
                    }
                    #[derive(Deserialize)]
                    struct Rows {
                        #[serde(default)]
                        rows: Vec<Node>,
                    }
                    fn walk(node: &Node, out: &mut HashSet<String>) {
                        out.extend(node.algo_order_id.iter().map(|id| id.0.clone()));
                        out.extend(node.order_id.iter().map(|id| id.0.clone()));
                        for child in &node.child_orders {
                            walk(child, out);
                        }
                    }
                    let rows: Rows = serde_json::from_str(data).map_err(|e| e.to_string())?;
                    let mut listed = HashSet::new();
                    for row in &rows.rows {
                        walk(row, &mut listed);
                    }
                    let confirmed = if !waiting.parent.is_empty() {
                        // Every placed leg: listed under the parent, or terminal.
                        let mut legs: HashSet<String> = rows
                            .rows
                            .iter()
                            .filter(|row| {
                                row.algo_order_id
                                    .as_ref()
                                    .is_some_and(|id| id.0 == waiting.parent)
                            })
                            .flat_map(|row| &row.child_orders)
                            .filter_map(|child| child.algo_order_id.as_ref().map(|id| id.0.clone()))
                            .collect();
                        legs.extend(
                            cx.store
                                .orders()
                                .iter()
                                .filter(|o| {
                                    o.parent_id.as_deref() == Some(waiting.parent.as_str())
                                        && self.order_terminal(&o.id)
                                })
                                .map(|o| o.id.clone()),
                        );
                        self.root_terminal(&waiting.parent) || legs.len() >= waiting.legs
                    } else {
                        waiting
                            .ids
                            .iter()
                            .all(|id| listed.contains(id) || self.order_terminal(id))
                    };
                    if !confirmed {
                        return Err(
                            "Orderly protective state is unconfirmed; no subsequent write was sent"
                                .into(),
                        );
                    }
                }
                Effect::FetchLeverage {
                    symbol,
                    mode: mode_,
                } => {
                    #[derive(Deserialize)]
                    struct Leverage {
                        #[serde(alias = "leverage")]
                        max_leverage: N,
                    }
                    let value = serde_json::from_str::<Leverage>(data)
                        .map_err(|e| e.to_string())?
                        .max_leverage
                        .0;
                    if value <= 0.0 {
                        return Err("Orderly invalid symbol leverage".into());
                    }
                    self.account
                        .leverage
                        .insert((symbol.clone(), *mode_ == MarginType::Isolated), value);
                    self.sync_setting(cx, symbol);
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Rails {
                        buy_leverage: f64,
                        sell_leverage: f64,
                    }
                    return Ok(Answer::Done(json(&Rails {
                        buy_leverage: value,
                        sell_leverage: value,
                    })));
                }
            }
            Ok(Answer::Next)
        })();
        match result {
            Ok(answer) => {
                if write {
                    let order_ids = active.job.ids[ids_before..].to_vec();
                    let accepted = WriteFact::Accepted { order_ids };
                    let _ = cx.http_write_fact(&request.context, accepted, None);
                }
                match answer {
                    Answer::Next => {
                        self.drive(cx, active, At::Data(&request.slot, &request.context))
                    }
                    Answer::Wait(waiting) => self.pause(cx, active, &request, waiting),
                    Answer::Done(result) => {
                        let plan = active.plan.clone().expect("live Plan");
                        let done = cx.done_read_plan_after_data(
                            &plan,
                            &request.slot,
                            &request.context,
                            result,
                        );
                        if done.is_err() {
                            let error = VenueError::new("Orderly leverage read could not complete");
                            return self.end(cx, active, current, error);
                        }
                        self.next_job(cx);
                    }
                }
            }
            Err(_) if request.step.is_verify() => {
                let Effect::Verify(waiting) = request.step.effect.clone() else {
                    unreachable!()
                };
                self.unconfirmed(cx, active, current, Some(waiting));
            }
            Err(e) if unknown_ack => {
                let _ = cx.http_write_unproven(&request.context, e.clone());
                let error = VenueError::new(e).outcome(Outcome::Unknown);
                self.call_failed(cx, active, request.step.clone(), error, true, current);
            }
            Err(e) => {
                let mut error = answer_failure(response, e);
                let uncertain = write && error.outcome != Some(Outcome::Rejected);
                if write {
                    record_failed(cx, &request.context, response, &error.message);
                }
                // The journal owns a write's outcome; a read has none.
                if !uncertain {
                    error.outcome = None;
                }
                self.call_failed(cx, active, request.step.clone(), error, uncertain, current);
            }
        }
    }
    fn root_finished(&self, _cx: &Cx<'_, Pending>, parent: &str) -> bool {
        self.root_terminal(parent)
    }
    fn pair_known(&self, cx: &Cx<'_, Pending>, parent: &str, ids: &[String], legs: usize) -> bool {
        if !parent.is_empty() {
            return self.root_terminal(parent)
                || cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.parent_id.as_deref() == Some(parent))
                    .count()
                    >= legs;
        }
        ids.len() >= legs
            && ids.iter().all(|id| {
                self.order_terminal(id)
                    || cx
                        .store
                        .orders()
                        .iter()
                        .any(|o| o.id == *id && o.parent_id.is_some())
            })
    }
    fn waiting_satisfied(&self, cx: &Cx<'_, Pending>, w: &Waiting, detached: bool) -> bool {
        if w.present && w.parent.is_empty() && w.ids.is_empty() {
            return detached && self.account.algo_snapshots > w.snapshot;
        }
        if w.present {
            self.pair_known(cx, &w.parent, &w.ids, w.legs)
                || (detached
                    && self.account.algo_snapshots > w.snapshot
                    && !cx.store.orders().iter().any(|o| {
                        live(o)
                            && (w.ids.contains(&o.id)
                                || o.parent_id.as_ref().is_some_and(|p| w.ids.contains(p)))
                    }))
        } else {
            self.root_terminal(&w.parent)
                || (self.account.algo_snapshots > w.snapshot
                    && !cx
                        .store
                        .orders()
                        .iter()
                        .any(|o| o.parent_id.as_deref() == Some(&w.parent) && live(o)))
        }
    }
    /// From a callback holding Cx: clear confirmed detached fences and release
    /// the active job's pause once the stream confirms it. Never issues I/O.
    pub(crate) fn trade_reconcile(&mut self, cx: &mut Cx<'_, Pending>) {
        self.settle_retired(cx);
        let detached = std::mem::take(&mut self.trading.detached);
        self.trading.detached = detached
            .into_iter()
            .filter(|(w, _)| !self.waiting_satisfied(cx, w, true))
            .collect();
        let satisfied = self
            .trading
            .active
            .as_ref()
            .and_then(|active| active.parked.as_ref())
            .is_some_and(|(_, waiting)| self.waiting_satisfied(cx, waiting, false));
        if satisfied
            && let Some(active) = self.trading.active.as_mut()
            && let Some((wait, _)) = &active.parked
            && cx.release_condition_wait(wait).is_ok()
        {
            active.parked = None;
        }
    }
}

#[derive(Clone)]
struct Waiting {
    parent: String,
    ids: Vec<String>,
    legs: usize,
    present: bool,
    snapshot: u64,
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// One call of the job's streamed Plan; Orderly never resends a write.
fn serial(step: &Step) -> SerialCall {
    SerialCall {
        operation: if step.method == Method::Get {
            Operation::Read
        } else {
            Operation::Mutation
        },
        timeout_ms: CALL_MS,
        attempts: 1,
    }
}

/// Native bulk cancellation: two independent family writes.
fn admit_independent(
    cx: &mut Cx<'_, Pending>,
    preflight: &Preflight,
    credential: safe_cex_engine::request_context::Credential,
    count: usize,
) -> Result<Plan, safe_cex_engine::request_context::Error> {
    let call = [CallBound {
        timeout_ms: CALL_MS,
        attempts: 1,
        retry_window: RetryWindow::FreshAttempt,
        after_refusal: &[],
    }];
    let steps = (0..count)
        .map(|_| StepBound {
            wait_ms: 0,
            calls: &call,
        })
        .collect::<Vec<_>>();
    cx.admit_plan(
        preflight,
        &steps,
        PlanPolicy {
            operation: Operation::Mutation,
            credential: Some(credential),
            pacing: None,
            retry: RetryPolicy::Never,
            independent_calls: true,
        },
    )
}

/// FetchLeverage's one Read call.
fn admit_read(
    cx: &mut Cx<'_, Pending>,
    preflight: &Preflight,
    credential: safe_cex_engine::request_context::Credential,
) -> Result<Plan, safe_cex_engine::request_context::Error> {
    let call = [CallBound {
        timeout_ms: CALL_MS,
        attempts: 1,
        retry_window: RetryWindow::FreshAttempt,
        after_refusal: &[],
    }];
    cx.admit_plan(
        preflight,
        &[StepBound {
            wait_ms: 0,
            calls: &call,
        }],
        PlanPolicy {
            operation: Operation::Read,
            credential: Some(credential),
            pacing: None,
            retry: RetryPolicy::Never,
            independent_calls: false,
        },
    )
}

fn first_slot(
    cx: &Cx<'_, Pending>,
    plan: &Plan,
) -> Result<Slot, safe_cex_engine::request_context::Error> {
    cx.plan_slots(plan)?
        .into_iter()
        .next()
        .ok_or(safe_cex_engine::request_context::Error::State)
}

fn native_code(body: &str) -> Option<i64> {
    #[derive(Deserialize)]
    struct Code {
        code: Option<i64>,
    }
    serde_json::from_str::<Code>(body).ok().and_then(|c| c.code)
}

/// The table's verdict for one delivered failure, by code or else by status.
fn verdict(status: u16, code: Option<i64>) -> Verdict {
    let listed = |key: &Key, code: i64| match *key {
        Key::Int(value) => value == code,
        Key::Range(low, high) => (low..=high).contains(&code),
        _ => false,
    };
    match code {
        Some(code) => CODES
            .rows
            .iter()
            .find(|row| listed(&row.key, code))
            .map_or(CODES.unlisted, |row| row.verdict),
        None => CODES
            .status
            .iter()
            .find(|(listed, _)| *listed == status)
            .map_or(CODES.unlisted, |(_, verdict)| *verdict),
    }
}

/// A delivered failure: native text and code, the table's kind, and Rejected
/// only for a proven pre-execution refusal; anything else is Unknown.
fn answer_failure(response: Response<'_>, message: String) -> VenueError {
    let code = native_code(response.body);
    let mut error = VenueError::new(message);
    if let Some(code) = code {
        error = error.venue_code(code.to_string());
    }
    match verdict(response.status, code) {
        Verdict::Refused(kind) | Verdict::RefusedResend(kind) => {
            error.kind(kind).outcome(Outcome::Rejected)
        }
        Verdict::Unknown(kind) => {
            let error = error.outcome(Outcome::Unknown);
            match kind {
                Some(kind) => error.kind(kind),
                None => error,
            }
        }
        Verdict::Satisfied(_) => error,
    }
}

/// Records one whole failed answer for the delivered write.
fn record_failed(
    cx: &mut Cx<'_, Pending>,
    context: &Context,
    response: Response<'_>,
    message: &str,
) {
    let native = native_code(response.body).map(|code| Native {
        code: NativeCode::Int(code),
        message,
    });
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

/// Bounded correlation for a released write: its endpoint. Orderly has no late
/// reconcile codec; the account snapshot reconciles.
pub(crate) fn trade_reconciliation(
    request: &Request,
) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
    ReconciliationData::new(request.step.path.clone())
}

#[cfg(test)]
mod tests {
    #[test]
    fn orderly_table_is_well_formed() {
        safe_cex_engine::classify::assert_table(&super::CODES);
    }
}
