//! Trading commands are preflighted completely, then issued one write at a time:
//! ordered writes on a streamed serial Plan, native continue-on-error cancels as
//! independent calls of a complete Plan.
use crate::Pending;
#[path = "trading/context.rs"]
mod context;
#[path = "trading/reference.rs"]
mod reference;
pub use reference::ReferenceRead;
#[path = "trading/protection.rs"]
pub(crate) mod protection;
#[path = "trading/venue_state.rs"]
mod venue_state;
#[path = "trading/write_plan.rs"]
mod write_plan;

/// BingX key refusals (common codes): bad signature, key without the permission,
/// incorrect key, IP not whitelisted. Reads and writes share this one list.
pub(crate) const KEY_REFUSALS: [i64; 4] = [100001, 100004, 100413, 100419];
use crate::sign::{Credentials, Params};
use context::{Entry, SERIAL, TABLE};
use safe_cex_engine::classify::{Intent, Native, NativeCode, Verdict, WriteFact};
use safe_cex_engine::model::{
    MarginType, Order, OrderIdentityKind, OrderIdentityReceipt, OrderSide, OrderStatus, OrderType,
    PlaceOrderOpts, PositionSide, StopTriggerType, TimeInForce,
};
use safe_cex_engine::request_context::plan::Slot;
use safe_cex_engine::request_context::{
    Context, Error, HttpOwnerRetirement, Plan, ReconciliationData,
};
use safe_cex_engine::setting_holds::SettingKey;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, VenueError, num,
};
use serde::Deserialize;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
};
use write_plan::Journals;

const ORDER: &str = "/openApi/swap/v2/trade/order";
const CANCEL_ALL: &str = "/openApi/swap/v2/trade/allOpenOrders";
const AMEND: &str = "/openApi/swap/v1/trade/amend";
const REPLACE: &str = "/openApi/swap/v1/trade/cancelReplace";
const UNKNOWN_WORDS: &str = "The outcome is unknown. Check Open Orders";
const MISSING_ORDER_WORDS: &str = "BingX order was already filled or cancelled; check Open Orders";

pub struct Request {
    write: usize,
    command: CommandId,
    call: Call,
    plan: Plan,
    slot: Slot,
    context: Context,
    method: Method,
    path: &'static str,
    params: Params,
    protective: bool,
    restoring: bool,
}
impl Request {
    pub fn settings_scope(&self) -> Option<Option<&str>> {
        match &self.call {
            Call::Leverage { symbol, .. } | Call::Margin { symbol, .. } => Some(Some(symbol)),
            Call::Mode { .. } => Some(None),
            _ => None,
        }
    }
    pub(crate) fn validate(&self, cx: &Cx<'_, Pending>) -> bool {
        cx.validate_plan_delivery(&self.plan, &self.context).is_ok()
    }
    pub(crate) fn reconciliation(&self) -> Result<ReconciliationData, Error> {
        ReconciliationData::new(self.command.to_string())
    }

    pub fn changes_settings(&self) -> bool {
        matches!(
            self.call,
            Call::Leverage { .. } | Call::Margin { .. } | Call::Mode { .. }
        )
    }
    pub fn accepts_placement(&self, response: Response<'_>) -> bool {
        self.places_order() && validated_placement(response).is_ok()
    }
    pub fn reconciles_orders(&self) -> bool {
        matches!(self.call, Call::Amend { .. } | Call::Replace { .. })
    }
    pub fn places_order(&self) -> bool {
        matches!(self.call, Call::Place)
    }
}
enum Call {
    Place,
    Cancel {
        ids: Vec<String>,
        symbol: String,
        absent_ok: bool,
    },
    Amend {
        id: String,
        symbol: String,
        quantity: f64,
        step: f64,
    },
    Replace {
        old_id: String,
    },
    Leverage {
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
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
    receipts: Vec<OrderIdentityReceipt>,
    method: Method,
    path: &'static str,
    params: Params,
    call: Call,
}
#[derive(Default)]
struct Job {
    steps: VecDeque<Step>,
    continue_on_error: bool,
    continue_after_refusal: bool,
    sets_both_hedged_sides: bool,
    // A trigger move cancels before placing; a failed placement must say so.
    replaces_cancelled: Option<(String, OrderSide, OrderType)>,
    // A limit's cancelReplace may cancel and then refuse the placement.
    replaces_order: Option<String>,
    guard: Option<protection::Guard>,
    restore: Option<Params>,
    restoring: bool,
    cancelled: bool,
    // A failed bracket leaves the placed entry without the protection the user asked for.
    entry_with_brackets: Option<(String, usize)>,
}
struct Run {
    job: Job,
    writes: Journals,
    plan: Plan,
    /// A serial Plan's next admitted call; independent Plans list their slots.
    next: Option<Slot>,
    native_ids: Vec<String>,
    error: Option<VenueError>,
}
/// Earlier writes that a later failure cannot roll back.
fn partial_failure_note(run: &Run, request: &Request, may_be_live: bool) -> Option<String> {
    if run.job.sets_both_hedged_sides
        && let Call::Leverage {
            symbol,
            side: Some(PositionSide::Short),
            ..
        } = &request.call
    {
        return Some(format!(
            "LONG leverage was updated for {symbol}, but SHORT leverage update was not confirmed"
        ));
    }
    if !request.places_order() {
        return None;
    }
    if let Some((symbol, side, kind)) = &run.job.replaces_cancelled {
        return Some(safe_cex_engine::error::stop_gone_text(
            symbol,
            *side,
            *kind,
            may_be_live,
        ));
    }
    let (symbol, lots) = run.job.entry_with_brackets.as_ref()?;
    let placed = run.writes.placed_count(*lots);
    if placed == 0 {
        return None;
    }
    let placement = if placed == *lots {
        "placed"
    } else {
        "partially placed"
    };
    let state = if may_be_live { "may not be" } else { "is not" };
    Some(format!(
        "Your {symbol} entry was {placement}, but its stop or take profit {state} in place"
    ))
}
struct Observation {
    order: Order,
    native_kind: String,
    position_side: String,
    attached: bool,
    native_status: String,
    original: Option<protection::Original>,
}
#[derive(Default)]
pub struct Trading {
    jobs: HashMap<CommandId, Run>,
    references: HashMap<String, protection::References>,
    /// Retired settings writes whose metadata custody finishes at the next callback.
    retired_settings: Vec<CommandId>,
    observed: HashMap<String, Observation>,
    /// Frame-fed: source observations whose order has not been ingested yet.
    pending_observed: HashMap<String, (String, String, bool, String, Option<protection::Original>)>,
    /// Frame-fed: the data source knows the position mode (VENUE_STATE).
    mode_known: bool,
    /// tradeMinUSDT by contract id. Market limits carry no value minimum.
    min_order_value: HashMap<String, f64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Id<'a> {
    Text(#[serde(borrow)] Cow<'a, str>),
    Number(u64),
}
impl Id<'_> {
    fn text(&self) -> String {
        match self {
            Self::Text(s) => s.to_string(),
            Self::Number(n) => n.to_string(),
        }
    }
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data<'a> {
    #[serde(default, borrow)]
    order_id: Option<Id<'a>>,
    #[serde(default, borrow, rename = "orderID")]
    order_id_text: Option<Id<'a>>,
    #[serde(default, borrow)]
    symbol: Cow<'a, str>,
    #[serde(default, borrow)]
    status: Cow<'a, str>,
    #[serde(default, borrow)]
    quantity: Option<Number<'a>>,
    #[serde(default, borrow, alias = "filled")]
    executed_qty: Option<Number<'a>>,
    #[serde(default, borrow, deserialize_with = "object_order")]
    order: Option<Box<Data<'a>>>,
    #[serde(default, borrow)]
    new_order_id: Option<Id<'a>>,
    #[serde(default, borrow, deserialize_with = "object_order")]
    new_order_response: Option<Box<Data<'a>>>,
    #[serde(default, borrow)]
    cancel_result: Cow<'a, str>,
    #[serde(default, borrow)]
    cancel_order_id: Option<Id<'a>>,
    #[serde(default, borrow)]
    new_order_result: Cow<'a, str>,
    #[serde(default, borrow)]
    replace_result: Cow<'a, str>,
    #[serde(default)]
    failed: Option<Vec<serde::de::IgnoredAny>>,
}
impl Data<'_> {
    fn has_order_claim(&self) -> bool {
        self.cancel_order_id.is_some()
            || self.order_id.is_some()
            || self.order_id_text.is_some()
            || self.new_order_id.is_some()
            || self.order.is_some()
            || self.new_order_response.is_some()
            || !self.status.is_empty()
            || self.quantity.is_some()
            || !self.cancel_result.is_empty()
            || !self.new_order_result.is_empty()
            || !self.replace_result.is_empty()
    }
    fn evidence_ids(&self, ids: &mut Vec<String>) {
        for id in self
            .order_id
            .iter()
            .chain(self.order_id_text.iter())
            .chain(self.new_order_id.iter())
            .chain(self.cancel_order_id.iter())
        {
            let id = id.text();
            if !id.trim().is_empty() {
                ids.push(id);
            }
        }
        for nested in self.order.iter().chain(self.new_order_response.iter()) {
            nested.evidence_ids(ids);
        }
    }

    fn id(&self) -> Result<Option<String>, String> {
        let nested = self.order.as_ref().map(|o| o.id()).transpose()?.flatten();
        matching_id(
            self.order_id_text
                .as_ref()
                .map(Id::text)
                .into_iter()
                .chain(self.order_id.as_ref().map(Id::text))
                .chain(nested),
        )
    }
    fn matches_symbol(&self, symbol: &str) -> bool {
        (self.symbol.is_empty() || self.symbol.replace('-', "") == symbol)
            && self.order.as_ref().is_none_or(|o| o.matches_symbol(symbol))
            && self
                .new_order_response
                .as_ref()
                .is_none_or(|o| o.matches_symbol(symbol))
    }
    fn proves_cancel(&self, ids: &[String]) -> bool {
        ids.len() == 1
            && self.id().ok().flatten().as_deref() == Some(&ids[0])
            && (matches!(self.status.as_ref(), "CANCELED" | "CANCELLED" | "EXPIRED")
                || self.order.as_ref().is_some_and(|o| o.proves_cancel(ids)))
            && (self.executed_qty.as_ref().is_some_and(|n| n.value() == 0.0)
                || self.order.as_ref().is_some_and(|o| o.proves_cancel(ids)))
            && self.canceled().is_ok()
            && self.unfilled()
            && self.failed.as_ref().is_none_or(Vec::is_empty)
    }
    fn unfilled(&self) -> bool {
        self.executed_qty.as_ref().is_none_or(|n| n.value() == 0.0)
            && self.order.as_ref().is_none_or(|o| o.unfilled())
    }
    fn canceled(&self) -> Result<(), String> {
        if !self.status.is_empty()
            && !matches!(self.status.as_ref(), "CANCELED" | "CANCELLED" | "EXPIRED")
        {
            return Err(format!(
                "BingX cancellation acknowledgement reports order status {}",
                self.status
            ));
        }
        if let Some(order) = &self.order {
            order.canceled()?;
        }
        Ok(())
    }
    fn open_placement(&self) -> bool {
        let own = self.status.is_empty()
            || matches!(self.status.as_ref(), "NEW" | "OPEN" | "PENDING" | "WORKING");
        own && (matches!(self.status.as_ref(), "NEW" | "OPEN" | "PENDING" | "WORKING")
            || self.order.as_ref().is_some_and(|o| o.open_placement()))
            && self
                .order
                .as_ref()
                .is_none_or(|o| o.status.is_empty() || o.open_placement())
    }
    fn accepted(&self) -> Result<(), String> {
        if !self.status.is_empty()
            && !matches!(
                self.status.as_ref(),
                "NEW" | "OPEN" | "PENDING" | "WORKING" | "PARTIALLY_FILLED" | "FILLED"
            )
        {
            return Err(format!(
                "BingX acknowledgement reports order status {}",
                self.status
            ));
        }
        if let Some(order) = &self.order {
            order.accepted()?;
        }
        Ok(())
    }
}
fn matching_id(ids: impl Iterator<Item = String>) -> Result<Option<String>, String> {
    let mut result = None;
    for id in ids {
        if id.trim().is_empty() {
            return Err("BingX acknowledgement contains an empty order id".into());
        }
        if result.as_ref().is_some_and(|previous| previous != &id) {
            return Err("BingX acknowledgement contains contradictory order ids".into());
        }
        result = Some(id);
    }
    Ok(result)
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Number<'a> {
    Text(#[serde(borrow)] Cow<'a, str>),
    Bare(f64),
}
impl Number<'_> {
    fn value(&self) -> f64 {
        match self {
            Self::Text(s) => s.parse().unwrap_or(f64::NAN),
            Self::Bare(n) => *n,
        }
    }
}
#[derive(Deserialize)]
struct Envelope<'a> {
    code: i64,
    #[serde(default, borrow)]
    msg: Cow<'a, str>,
    #[serde(default, borrow, deserialize_with = "object_data")]
    data: Option<Data<'a>>,
}

fn object_data<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Data<'de>>, D::Error> {
    struct OptionalObject;
    struct Object;
    impl<'de> serde::de::Visitor<'de> for Object {
        type Value = Data<'de>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a BingX acknowledgement object")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
            Data::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    impl<'de> serde::de::Visitor<'de> for OptionalObject {
        type Value = Option<Data<'de>>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a BingX acknowledgement object or null")
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: serde::Deserializer<'de>>(
            self,
            value: D,
        ) -> Result<Self::Value, D::Error> {
            value.deserialize_map(Object).map(Some)
        }
    }
    deserializer.deserialize_option(OptionalObject)
}

fn object_order<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Box<Data<'de>>>, D::Error> {
    object_data(deserializer).map(|data| data.map(Box::new))
}
fn parse_envelope(body: &str) -> Result<Envelope<'_>, serde_json::Error> {
    if !body.trim_start().starts_with('{') {
        return Err(serde::de::Error::custom(
            "BingX acknowledgement is not an object",
        ));
    }
    serde_json::from_str(body)
}

/// The write may have executed; reconcile before resending.
fn unknown(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}
fn present_unknown(mut error: VenueError, note: Option<&str>) -> VenueError {
    if error.venue_message.is_none() {
        error.venue_message = Some(error.message.clone().into_boxed_str());
    }
    error.message = match note {
        Some(note) => format!("{UNKNOWN_WORDS}. {note}"),
        None => UNKNOWN_WORDS.into(),
    };
    error.outcome = Some(Outcome::Unknown);
    error.presents = true;
    error
}
/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}
fn placement_id(data: &Data<'_>) -> Result<String, VenueError> {
    data.accepted()?;
    data.id()
        .map_err(unknown)?
        .ok_or_else(|| unknown("BingX place response omitted the order id"))
}
fn validated_placement(response: Response<'_>) -> Result<String, VenueError> {
    let envelope: Envelope<'_> =
        parse_envelope(response.body).map_err(|e| unknown(e.to_string()))?;
    if envelope.code != 0 {
        return Err(unknown("BingX refused placement"));
    }
    placement_id(&envelope.data.unwrap_or_default())
}

impl Trading {
    /// Retirement has no Store authority: finish its metadata writes here.
    fn settle_retired(&mut self, cx: &mut Cx<'_, Pending>) {
        for command in self.retired_settings.drain(..) {
            cx.store.metadata_write_finished(command);
        }
    }

    pub fn observe_order(
        &mut self,
        order: &Order,
        native_kind: &str,
        position_side: &str,
        attached: bool,
        native_status: &str,
    ) {
        if order.status != OrderStatus::Open {
            self.observed.remove(&order.id);
            return;
        }
        self.observed.insert(
            order.id.clone(),
            Observation {
                order: order.clone(),
                native_kind: native_kind.into(),
                position_side: position_side.into(),
                attached,
                native_status: native_status.into(),
                original: None,
            },
        );
    }

    pub(crate) fn observe_original(&mut self, id: &str, original: Option<protection::Original>) {
        if let Some(seen) = self.observed.get_mut(id) {
            seen.original = original;
        }
    }

    pub fn retain_open_orders(&mut self, store: &safe_cex_engine::Store) {
        self.observed.retain(|id, _| {
            store
                .order(id)
                .is_some_and(|o| o.status == OrderStatus::Open)
        });
    }

    pub fn command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        command: Command,
        on_entry: impl FnOnce(&mut Cx<'_, Pending>),
    ) -> bool {
        self.settle_retired(cx);
        if !matches!(
            &command,
            Command::PlaceOrders { .. }
                | Command::CancelOrders { .. }
                | Command::AmendOrder { .. }
                | Command::CancelSymbolOrders { .. }
                | Command::SetLeverage { .. }
                | Command::SetMarginType { .. }
                | Command::SetPositionMode { .. }
        ) {
            return false;
        }
        let Some(entry) = Entry::begin(cx, credentials) else {
            return true;
        };
        let (id, result) = match command {
            Command::PlaceOrders { command, orders } => (command, self.place(cx, &orders)),
            Command::CancelOrders { command, ids } => (command, self.cancel(cx, &ids)),
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (command, self.amend(cx, &id, price, amount)),
            Command::CancelSymbolOrders { command, symbol } => {
                (command, self.cancel_all(cx, &symbol))
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side,
            } => (command, self.leverage(cx, &symbol, leverage, side)),
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => (command, self.margin(cx, &symbol, isolated)),
            Command::SetPositionMode {
                command,
                hedged,
                symbol: _,
            } => {
                let plan = if cx.store.positions().iter().any(|p| p.contracts > 0.0)
                    || cx
                        .store
                        .orders()
                        .iter()
                        .any(|o| o.status == OrderStatus::Open)
                {
                    Err(
                        "Close positions and cancel open orders before switching position mode"
                            .into(),
                    )
                } else {
                    Ok(one(
                        Method::Post,
                        "/openApi/swap/v1/positionSide/dual",
                        Params::new()
                            .str("dualSidePosition", if hedged { "true" } else { "false" }),
                        Call::Mode { hedged },
                    ))
                };
                (command, plan)
            }
            _ => return false,
        };
        if credentials.is_none() {
            let error = not_sent("BingX trading requires an API key and secret");
            entry.reject(cx, error.kind(ErrorKind::AuthInvalid));
            return true;
        }
        if self.jobs.contains_key(&id) {
            entry.reject(cx, not_sent("Command id is already in progress"));
            return true;
        }
        match result {
            Ok(job) => {
                if job
                    .guard
                    .as_ref()
                    .is_some_and(|guard| guard.needs_reference(cx.store, &self.references, cx.now))
                {
                    on_entry(cx);
                    self.read_reference(cx, entry, id, job);
                } else {
                    self.admit_job(cx, credentials, entry, id, job, on_entry);
                }
            }
            Err(error) => {
                on_entry(cx);
                entry.reject(cx, not_sent(error).kind(ErrorKind::InvalidInput));
            }
        }
        true
    }

    fn admit_job(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        entry: Entry,
        id: CommandId,
        job: Job,
        on_entry: impl FnOnce(&mut Cx<'_, Pending>),
    ) {
        let count = job.steps.len();
        let writes = Journals::new(&job.steps);
        let admitted = if job.continue_on_error {
            entry.admit_independent(cx, count).map(|plan| (plan, None))
        } else {
            entry
                .admit_serial(cx, count)
                .map(|(plan, slot)| (plan, Some(slot)))
        };
        if let Some((plan, next)) = admitted {
            on_entry(cx);
            self.retain_open_orders(cx.store);
            self.jobs.insert(
                id,
                Run {
                    job,
                    writes,
                    plan,
                    next,
                    native_ids: Vec::new(),
                    error: None,
                },
            );
            self.advance(cx, credentials, id);
        }
    }

    fn advance(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        command: CommandId,
    ) {
        let Some(run) = self.jobs.get_mut(&command) else {
            return;
        };
        if run.error.is_some()
            && !run.job.continue_on_error
            && !run.job.continue_after_refusal
            && !run.job.restoring
        {
            let plan = run.plan.clone();
            let error = run
                .error
                .clone()
                .unwrap_or_else(|| unknown("BingX write failed"));
            let error = run.writes.failure(error);
            let _ = cx.fail_plan(&plan, error);
            return;
        }
        if let Some(step) = run.job.steps.pop_front() {
            let write = run.writes.begin(&step);
            let plan = run.plan.clone();
            let next = match run.next.take() {
                Some(slot) => Ok(slot),
                None => cx
                    .plan_slots(&plan)
                    .and_then(|slots| slots.into_iter().next().ok_or(Error::State)),
            };
            let slot = match next {
                Ok(slot) => slot,
                Err(error) => {
                    let error = error.venue_error().outcome(Outcome::NotSent);
                    run.writes.record_failure(write, &step.call, error.clone());
                    let error = run.writes.failure(error);
                    let _ = cx.fail_plan(&plan, error);
                    return;
                }
            };
            let intent = match &step.call {
                Call::Place => Some(Intent::Place),
                Call::Leverage { .. } | Call::Margin { .. } | Call::Mode { .. } => {
                    Some(Intent::Setting)
                }
                _ => None,
            };
            let issued = match intent {
                Some(intent) => cx.plan_leg_context_with_intent(&plan, &slot, intent),
                None => cx.plan_leg_context(&plan, &slot),
            };
            let context = match issued {
                Ok(context) => context,
                Err(error) => {
                    let error = error.venue_error().outcome(Outcome::NotSent);
                    run.writes.record_failure(write, &step.call, error.clone());
                    let error = run.writes.failure(error);
                    let _ = cx.fail_plan(&plan, error);
                    return;
                }
            };
            if matches!(
                &step.call,
                Call::Leverage { .. } | Call::Margin { .. } | Call::Mode { .. }
            ) {
                let scope = match &step.call {
                    Call::Leverage { symbol, .. } | Call::Margin { symbol, .. } => {
                        Some(symbol.as_str())
                    }
                    _ => None,
                };
                if let Err(error) = cx.store.metadata_write_started(command, scope) {
                    let error = error.outcome(Outcome::NotSent);
                    run.writes.record_failure(write, &step.call, error.clone());
                    let error = run.writes.failure(error);
                    let _ = cx.fail_plan(&plan, error);
                    return;
                }
            }
            let words = run
                .job
                .unknown_words()
                .unwrap_or_else(|| UNKNOWN_WORDS.into());
            let _ = cx.present_plan_failure(&plan, words);
            let protective = run.job.guard.is_some()
                && (matches!(step.call, Call::Cancel { .. } | Call::Replace { .. })
                    || run.job.restoring);
            let restoring = run.job.restoring;
            cx.command_progress(command, step.receipts);
            self.queue(
                cx,
                credentials,
                Request {
                    write,
                    command,
                    call: step.call,
                    plan,
                    slot,
                    context,
                    method: step.method,
                    path: step.path,
                    params: step.params,
                    protective,
                    restoring,
                },
            );
        } else {
            let plan = run.plan.clone();
            if let Some(error) = run.error.clone() {
                let error = run.writes.failure(error);
                let _ = cx.fail_plan(&plan, error);
                return;
            }
            let result = serde_json::to_string(&run.native_ids).unwrap_or_default();
            let _ = cx.done_plan(&plan, result);
        }
    }

    fn queue(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        request: Request,
    ) {
        let Some(credentials) = credentials else {
            let plan = request.plan.clone();
            let error = not_sent("BingX credentials unavailable").kind(ErrorKind::AuthInvalid);
            let error = if let Some(run) = self.jobs.get_mut(&request.command) {
                run.writes
                    .record_failure(request.write, &request.call, error.clone());
                run.writes.failure(error)
            } else {
                error
            };
            let _ = cx.fail_plan(&plan, error);
            return;
        };
        let recipe = crate::sign::dispatch::Http::new(
            credentials.primary.clone(),
            request.method,
            request.path,
            request.params.clone(),
        );
        let (url, _, body) = recipe.unsigned();
        let plan = request.plan.clone();
        if let Err((pending, error)) = cx.request_signed_managed(
            request.context.clone(),
            None,
            request.method,
            url,
            body,
            Box::new(recipe),
            Pending::Trade(request),
        ) {
            if let Some(request) = pending.into_trade() {
                self.on_sign_failed(cx, Some(credentials), request, error.venue_error());
            } else {
                let _ = cx.fail_plan(&plan, error.venue_error());
            }
        }
    }

    pub fn on_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        request: Request,
        error: VenueError,
    ) {
        self.settle_retired(cx);
        cx.store.metadata_write_finished(request.command);
        let Some(run) = self
            .jobs
            .get_mut(&request.command)
            .filter(|run| run.plan.owner() == request.plan.owner())
        else {
            return;
        };
        // A transport loss arrives here too; only a call never sent is certainly not live.
        let note = partial_failure_note(run, &request, error.outcome != Some(Outcome::NotSent));
        let error = if !matches!(error.outcome, Some(Outcome::NotSent | Outcome::Rejected))
            && run.job.guard.is_none()
        {
            present_unknown(error, None)
        } else if run.job.cancelled {
            run.job.protection_error(error, false)
        } else {
            error
        };
        run.writes
            .record_failure(request.write, &request.call, error.clone());
        let _ = run.error.get_or_insert(error.clone());
        if run.job.continue_on_error
            && error.outcome == Some(Outcome::NotSent)
            && cx
                .finish_plan_failed_call(&request.plan, &request.slot, &request.context)
                .is_ok()
        {
            self.advance(cx, credentials, request.command);
        } else if let Some(note) = note {
            let error = run.writes.failure(error);
            let _ = cx.fail_plan_noted(&request.plan, error, &note);
        } else {
            let error = run.writes.failure(error);
            let _ = cx.fail_plan(&request.plan, error);
        }
    }

    fn journal_failure(&mut self, request: &Request, error: VenueError) -> VenueError {
        let Some(run) = self
            .jobs
            .get_mut(&request.command)
            .filter(|run| run.plan.owner() == request.plan.owner())
        else {
            return error;
        };
        run.writes
            .record_failure(request.write, &request.call, error.clone());
        run.writes.failure(error)
    }
    pub fn on_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        request: Request,
        error: &str,
    ) {
        self.on_sign_failed(
            cx,
            credentials,
            request,
            unknown(error).kind(ErrorKind::Network),
        );
    }
    pub(crate) fn retired(&mut self, retired: &HttpOwnerRetirement) {
        let commands = self
            .jobs
            .iter()
            .filter_map(|(command, run)| retired.matches_plan(&run.plan).then_some(*command))
            .collect::<Vec<_>>();
        for command in commands {
            self.jobs.remove(&command);
            self.retired_settings.push(command);
        }
    }

    pub(crate) fn unproven_evidence(
        &self,
        request: &Request,
        body: &str,
        proof: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        evidence(body, proof);
        if let Some(run) = self
            .jobs
            .get(&request.command)
            .filter(|run| run.plan.owner() == request.plan.owner())
        {
            run.writes.evidence(proof);
        }
    }

    pub fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        mut request: Request,
        response: Response<'_>,
        on_effects: impl FnOnce(&mut Cx<'_, Pending>, &Request),
    ) {
        self.settle_retired(cx);
        if !request.validate(cx) {
            return;
        }
        let envelope = parse_envelope(response.body);
        let data = envelope.as_ref().ok().and_then(|e| e.data.as_ref());
        let mut evidence = Vec::new();
        if let Some(data) = data {
            data.evidence_ids(&mut evidence);
        }
        let call = &request.call;
        let guarded = self
            .jobs
            .get(&request.command)
            .is_some_and(|r| r.job.guard.is_some());
        let restoring = self
            .jobs
            .get(&request.command)
            .is_some_and(|r| r.job.restoring);
        let rate_limited = guarded
            && (response.status == 429 || envelope.as_ref().is_ok_and(|e| e.code == 100410));
        let consistent_symbol = self
            .jobs
            .get(&request.command)
            .and_then(|r| r.job.guard.as_ref())
            .is_none_or(|g| data.is_none_or(|d| d.matches_symbol(&g.order.symbol)));
        let current_unfilled = self
            .jobs
            .get(&request.command)
            .and_then(|r| r.job.guard.as_ref())
            .is_none_or(|g| {
                cx.store.order(&g.order.id).is_some_and(|o| {
                    let mut current = o.clone();
                    current.status = g.order.status;
                    current.last_update = g.order.last_update;
                    matches!(o.status, OrderStatus::Open | OrderStatus::Canceled)
                        && o.filled == 0.0
                        && current == g.order
                })
            });
        let cancel_proven = consistent_symbol
            && current_unfilled
            && match call {
                Call::Replace { old_id } => data.is_some_and(|d| {
                    d.cancel_order_id.as_ref().map(Id::text).as_deref() == Some(old_id)
                        && matches!(d.cancel_result.as_ref(), "SUCCESS" | "true" | "TRUE")
                        && d.unfilled()
                        && d.failed.as_ref().is_none_or(Vec::is_empty)
                }),
                Call::Cancel { ids, .. } => data.is_some_and(|d| d.proves_cancel(ids)),
                _ => false,
            };
        let missing_cancel_proof = guarded
            && envelope.as_ref().is_ok_and(|e| e.code == 0)
            && matches!(call, Call::Cancel { .. } | Call::Replace { .. })
            && !cancel_proven;
        let half_refused = guarded
            && !rate_limited
            && (200..300).contains(&response.status)
            && cancel_proven
            && matches!(call, Call::Replace { .. })
            && data.is_some_and(|d| {
                d.new_order_result == "FAILED"
                    && (d.replace_result.is_empty() || d.replace_result == "FAILED")
                    && d.new_order_id.is_none()
                    && d.new_order_response.is_none()
                    && d.order_id.is_none()
                    && d.order_id_text.is_none()
                    && d.order.is_none()
                    && d.status.is_empty()
            });
        let missing_order = ((200..300).contains(&response.status) || response.status == 400)
            && matches!(
                call,
                Call::Cancel { .. } | Call::Amend { .. } | Call::Replace { .. }
            )
            && envelope
                .as_ref()
                .is_ok_and(|e| context::missing_order(e.code, &e.msg));
        if missing_order {
            let _ = cx.present_plan_failure(&request.plan, MISSING_ORDER_WORDS);
        }
        let absent = envelope
            .as_ref()
            .is_ok_and(|e| e.code == 80016 || missing_order)
            && !guarded
            && matches!(
                call,
                Call::Cancel {
                    absent_ok: true,
                    ..
                }
            );
        let canceled = (!guarded || cancel_proven)
            && matches!(call, Call::Replace { .. })
            && data.is_some_and(|data| {
                matches!(data.cancel_result.as_ref(), "SUCCESS" | "true" | "TRUE")
            });
        let decoded = envelope
            .as_ref()
            .map_err(|e| unknown(format!("Invalid BingX trading response: {e}")))
            .and_then(|e| {
                if e.code != 0 && !absent {
                    return Err(unknown(if missing_order {
                        MISSING_ORDER_WORDS.into()
                    } else {
                        format!("BingX {}: {}", e.code, e.msg)
                    }));
                }
                decode(
                    &request.call,
                    e.data.as_ref().unwrap_or(&Data::default()),
                    absent,
                )
            });
        let decoded = if restoring
            && decoded.is_ok()
            && (!data.is_some_and(Data::open_placement)
                || decoded.as_ref().is_ok_and(|ids| {
                    self.jobs
                        .get(&request.command)
                        .and_then(|r| r.job.guard.as_ref())
                        .is_some_and(|g| ids.iter().any(|id| id == &g.order.id))
                })) {
            Err(unknown(
                "BingX put-back did not prove a fresh open protection order",
            ))
        } else {
            decoded
        };
        let decoded = if guarded && (rate_limited || half_refused || missing_cancel_proof) {
            Err(unknown(if rate_limited {
                "BingX outcome unknown, check Open Orders"
            } else if half_refused {
                "BingX replacement was refused after its target was canceled"
            } else {
                "BingX cancellation did not prove the target; outcome unknown, check Open Orders"
            }))
        } else {
            decoded
        };
        let decoded = if guarded
            && matches!(call, Call::Replace { .. })
            && data.is_some_and(|d| {
                let success = |s: &str| matches!(s, "SUCCESS" | "true" | "TRUE");
                !d.new_order_result.is_empty()
                    && !d.replace_result.is_empty()
                    && success(&d.new_order_result) != success(&d.replace_result)
            }) {
            Err(unknown(
                "BingX replacement results contradict each other; check Open Orders",
            ))
        } else {
            decoded
        };
        let result = if let (true, Ok(ids)) = (
            envelope.as_ref().is_ok_and(|e| e.code == 0),
            decoded.as_ref(),
        ) {
            let ids = ids.clone();
            match cx.http_write_fact(
                &request.context,
                WriteFact::Accepted {
                    order_ids: ids.clone(),
                },
                None,
            ) {
                Ok(_) => Ok(ids),
                Err(error) => {
                    let error = self
                        .journal_failure(&request, error.venue_error().outcome(Outcome::Unknown));
                    let _ = cx.fail_plan(&request.plan, error);
                    return;
                }
            }
        } else {
            let mut message = decoded
                .as_ref()
                .err()
                .map(|error| error.message.clone())
                .unwrap_or_else(|| "BingX order is already absent".into());
            let venue_refused = envelope.as_ref().is_ok_and(|e| {
                e.code != 0
                    && matches!(
                        TABLE.verdict(
                            &Native {
                                code: NativeCode::Int(e.code),
                                message: &e.msg,
                            },
                            Intent::Place,
                        ),
                        Verdict::Refused(_) | Verdict::RefusedResend(_)
                    )
            });
            if let Some(note) = self
                .jobs
                .get(&request.command)
                .and_then(|run| partial_failure_note(run, &request, !venue_refused))
            {
                message = format!("{note}: {message}");
            }
            if canceled
                && let Some(symbol) = self
                    .jobs
                    .get(&request.command)
                    .and_then(|run| run.job.replaces_order.as_ref())
            {
                let refused = envelope.as_ref().is_ok_and(|e| e.code != 0)
                    || data.is_some_and(|data| {
                        !data.new_order_result.is_empty()
                            && !matches!(
                                data.new_order_result.as_ref(),
                                "SUCCESS" | "true" | "TRUE"
                            )
                    });
                let lost = safe_cex_engine::error::order_gone_text(symbol, !refused);
                message = format!("{lost}: {message}");
            }
            let native = if half_refused {
                Some(Native {
                    code: NativeCode::Text("FAILED"),
                    message: "newOrderResult",
                })
            } else {
                envelope.as_ref().ok().map(|e| Native {
                    code: NativeCode::Int(e.code),
                    message: &e.msg,
                })
            };
            let timestamp = crate::sign::timestamp::refused(response.body);
            let table = if half_refused {
                &protection::REPLACEMENT_FAILED
            } else if (restoring || (request.write > 0 && !matches!(call, Call::Cancel { .. })))
                && (timestamp || envelope.as_ref().is_ok_and(|e| e.code == 100001))
            {
                &protection::RESTORE_AUTH
            } else if native
                .as_ref()
                .is_some_and(|n| context::names_duplicate_client_id(n.message))
            {
                &context::DUPLICATE_TABLE
            } else if timestamp {
                &context::TIMESTAMP_TABLE
            } else if missing_order {
                &context::MISSING_ORDER_TABLE
            } else {
                &TABLE
            };
            let fact = match cx.classify_write(&request.context, table, native.as_ref(), message) {
                Ok(fact) => fact,
                Err(error) => {
                    let error = self
                        .journal_failure(&request, error.venue_error().outcome(Outcome::Unknown));
                    let _ = cx.fail_plan(&request.plan, error);
                    return;
                }
            };
            let fact = if half_refused {
                fact
            } else if rate_limited
                || (guarded && matches!(call, Call::Replace { .. }))
                || missing_cancel_proof
                || data.is_some_and(Data::has_order_claim)
                || canceled
                || envelope.as_ref().is_ok_and(|e| e.code == 0)
                || (matches!(fact.verdict(), Verdict::Satisfied(_))
                    && (!absent || decoded.is_err()))
            {
                fact.unproven().with_order_ids(&evidence)
            } else {
                fact
            };
            let satisfied = matches!(fact.verdict(), Verdict::Satisfied(_));
            // STOP_ON_FAILURE may cancel before refusing the placement: native
            // leaves a refused cancelReplace without an outcome, and never resends it.
            let replace_refused = matches!(call, Call::Replace { .. })
                && matches!(
                    fact.verdict(),
                    Verdict::Refused(_) | Verdict::RefusedResend(_)
                );
            let error = fact.error().clone();
            let grant = match cx.http_write_fact(
                &request.context,
                WriteFact::Failed {
                    fact,
                    known_effects: canceled || replace_refused,
                },
                None,
            ) {
                Ok(grant) => grant,
                Err(error) => {
                    let error = self
                        .journal_failure(&request, error.venue_error().outcome(Outcome::Unknown));
                    let _ = cx.fail_plan(&request.plan, error);
                    return;
                }
            };
            if let Some(grant) = grant {
                match cx.plan_resend_context(&request.plan, &request.slot, &request.context, grant)
                {
                    Ok(context) => {
                        request.context = context;
                        if timestamp {
                            crate::sign::timestamp::note_resend(cx);
                        }
                        self.queue(cx, credentials, request);
                    }
                    Err(error) => {
                        let error = self.journal_failure(&request, error.venue_error());
                        let _ = cx.fail_plan(&request.plan, error);
                    }
                }
                return;
            }
            if satisfied {
                Ok(Vec::new())
            } else {
                let mut error = error;
                if error.outcome == Some(Outcome::Unknown) && !guarded {
                    let note = self
                        .jobs
                        .get(&request.command)
                        .and_then(|run| partial_failure_note(run, &request, true));
                    error = present_unknown(error, note.as_deref());
                }
                Err(error)
            }
        };
        cx.store.metadata_write_finished(request.command);
        if !absent
            && (result.is_ok() || canceled)
            && cx.validate_http_write_effects(&request.context).is_ok()
        {
            apply(self, cx, &request.call, canceled);
            on_effects(cx, &request);
        }
        let Some(run) = self
            .jobs
            .get_mut(&request.command)
            .filter(|run| run.plan.owner() == request.plan.owner())
        else {
            let _ = cx.finish_plan_call(&request.plan, &request.slot, &request.context);
            return;
        };
        if guarded
            && cancel_proven
            && (canceled || (matches!(call, Call::Cancel { .. }) && result.is_ok()))
        {
            run.job.cancelled = true;
        }
        run.writes.settle(&request, &result, canceled);
        let uncertain = result
            .as_ref()
            .is_err_and(|error| error.outcome == Some(Outcome::Unknown));
        match result {
            Ok(ids) => {
                run.native_ids.extend(ids);
                if restoring {
                    if let Some(error) = run.error.take() {
                        run.error = Some(run.job.protection_error(error, true));
                    }
                    run.job.restoring = false;
                }
            }
            Err(error) => {
                let definite = error.outcome == Some(Outcome::Rejected) && !rate_limited;
                let error = if run.job.cancelled {
                    run.job.protection_error(error, false)
                } else {
                    error
                };
                if guarded || uncertain || run.error.is_none() {
                    run.error = Some(error);
                }
                if guarded || uncertain {
                    run.job.steps.clear();
                }
                if guarded
                    && run.job.cancelled
                    && definite
                    && !restoring
                    && let Some(params) = run.job.restore.take()
                {
                    let restore = Step {
                        receipts: Vec::new(),
                        method: Method::Post,
                        path: ORDER,
                        params,
                        call: Call::Place,
                    };
                    run.writes.restore(&restore);
                    run.job.steps.push_back(restore);
                    run.job.restoring = true;
                } else {
                    run.job.restoring = false;
                }
            }
        }
        // An ordered write continues only from this call's accepted DATA.
        if uncertain {
            let error = run
                .writes
                .failure(run.error.clone().unwrap_or_else(|| unknown(UNKNOWN_WORDS)));
            let _ = cx.fail_plan(&request.plan, error);
            return;
        }
        let settled = if !run.job.continue_on_error
            && (run.error.is_none() || run.job.restoring || run.job.continue_after_refusal)
            && !run.job.steps.is_empty()
        {
            cx.continue_serial_plan(
                &request.plan,
                &request.slot,
                &request.context,
                safe_cex_engine::request_context::plan::SerialCall {
                    attempts: 1,
                    ..SERIAL
                },
            )
            .map(|slot| run.next = Some(slot))
        } else {
            cx.finish_plan_call(&request.plan, &request.slot, &request.context)
        };
        if settled.is_err() {
            let error = run
                .writes
                .failure(unknown("BingX write settlement refused"));
            let _ = cx.fail_plan(&request.plan, error);
            return;
        }
        self.advance(cx, credentials, request.command);
    }

    fn place(&self, cx: &Cx<'_, Pending>, orders: &[PlaceOrderOpts]) -> Result<Job, String> {
        // Position side and reduce-only follow the mode; never guess one-way.
        if cx.frame_fed() && !self.mode_known {
            return Err("BingX position mode is not loaded yet".into());
        }
        if orders.iter().any(|order| {
            order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some()
        }) {
            return Err("Bybit BBO and TickSize controls are not supported by BingX".into());
        }
        let mut seen = HashSet::new();
        for o in orders {
            if let Some(id) = &o.client_order_id {
                if !(1..=40).contains(&id.chars().count()) || id.chars().any(|c| c.is_uppercase()) {
                    return Err(
                        "BingX client order id requires 1–40 characters with no uppercase letters"
                            .into(),
                    );
                }
                if !matches!(o.kind, OrderType::Limit | OrderType::Market)
                    || o.stop_loss.is_some()
                    || o.take_profit.is_some()
                    || (o.trigger_price.is_some() && !o.redundant_protective_trigger())
                    || o.close == Some(true)
                {
                    return Err("Client order id requires one plain LIMIT or MARKET order".into());
                }
                if !seen.insert(id) {
                    return Err("Duplicate client order id in batch".into());
                }
            }
        }
        let mut job = Job {
            continue_after_refusal: true,
            ..Job::default()
        };
        for o in orders {
            let steps = self.format(cx, o)?;
            if orders.len() == 1 && (o.stop_loss.is_some() || o.take_profit.is_some()) {
                let lots = steps.len()
                    - usize::from(o.stop_loss.is_some())
                    - usize::from(o.take_profit.is_some());
                job.entry_with_brackets = Some((o.symbol.clone(), lots));
            }
            job.steps.extend(steps);
        }
        Ok(job)
    }

    pub fn set_min_order_value(&mut self, id: &str, value: f64) {
        self.min_order_value.insert(id.to_owned(), value);
    }

    fn format(&self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<Vec<Step>, String> {
        let market = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| format!("Market {} not found", o.symbol))?;
        if !market.active {
            return Err("Market is not active".into());
        }
        if [o.trigger_type, o.tp_trigger_type, o.sl_trigger_type]
            .contains(&Some(StopTriggerType::IndexPrice))
        {
            return Err("BingX supports mark-price and last-price triggers".into());
        }
        if (o.trigger_price.is_some() && !o.redundant_protective_trigger())
            || o.kind == OrderType::Conditional
        {
            return Err("BingX does not support this conditional order shape".into());
        }
        if !o.amount.is_finite() || o.amount < 0.0 {
            return Err("Order quantity must be finite and nonnegative".into());
        }
        let amount = num::adjust(o.amount, market.precision.amount);
        if !amount.is_finite() {
            return Err("Normalized order quantity must be finite".into());
        }
        let contract = market
            .id
            .split('-')
            .next()
            .unwrap_or(&market.base)
            .to_ascii_uppercase();
        if !cx.store.hedged()
            && contract.ends_with("USD")
            && !contract.ends_with("USDT")
            && !contract.ends_with("USDC")
        {
            return Err(
                "BingX OpenAPI requires Hedged position mode for non-crypto symbols".into(),
            );
        }
        let stop = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
        let full = stop && (o.close == Some(true) || amount == 0.0);
        let closing = stop || o.kind == OrderType::TrailingStopLoss || o.reduce_only == Some(true);
        let pside = position_side(cx.store.hedged(), o.side, closing);
        let opposite = if o.side == OrderSide::Sell {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        let position = cx.store.positions().iter().find(|p| {
            p.symbol == o.symbol
                && p.side == opposite
                && p.contracts.is_finite()
                && p.contracts > 0.0
        });
        let base = || {
            Params::new()
                .str("symbol", &market.id)
                .str("side", side(o.side))
                .str("positionSide", pside)
        };
        if o.kind == OrderType::TrailingStopLoss {
            if o.stop_loss.is_some() || o.take_profit.is_some() {
                return Err("Trailing stop cannot carry attached brackets".into());
            }
            let pos = position.ok_or("Trailing stop requires an opposite-side position")?;
            let ticker = cx
                .store
                .ticker(&o.symbol)
                .filter(|t| t.last.is_finite() && t.last > 0.0)
                .ok_or("Trailing stop requires a valid last price")?;
            let target = positive(o.price, "Trailing stop price")?;
            let distance = num::adjust((ticker.last - target).abs(), market.precision.price);
            let rate = num::round_half_up(distance / ticker.last * 10_000.0) / 10_000.0;
            if rate <= 0.0 || rate > 1.0 {
                return Err(
                    "Trailing callback rate must be greater than zero and at most one".into(),
                );
            }
            return Ok(vec![place_step(
                base()
                    .str("type", "TRAILING_STOP_MARKET")
                    .num("quantity", pos.contracts)
                    .num("priceRate", rate)
                    .str("priceProtect", "true"),
            )]);
        }
        if full && position.is_none() {
            return Err("Full-position stop requires an opposite-side position".into());
        }
        // A close stays out, as below: dust left by a partial close must still be closable.
        if !full && (amount <= 0.0 || (!closing && amount < market.limits.amount.min)) {
            return Err("Order quantity is below the market minimum".into());
        }
        let price = if o.kind != OrderType::Market {
            Some(num::adjust(
                positive(o.price, "Order price")?,
                market.precision.price,
            ))
        } else {
            None
        };
        if price.is_some_and(|p| !p.is_finite() || p <= 0.0) {
            return Err("Order price rounds to zero".into());
        }
        // Closes stay out: nothing says the venue holds a reduce to this, and a stuck close is worse.
        let min_value = self.min_order_value.get(&market.id).copied().unwrap_or(0.0);
        let reference = price.or_else(|| cx.store.ticker(&o.symbol).map(|t| t.last));
        if !closing && reference.is_some_and(|p| p > 0.0 && amount * p < min_value) {
            return Err(format!(
                "Order value is below the BingX minimum of {min_value} USDT"
            ));
        }
        let kind = match o.kind {
            OrderType::Market => "MARKET",
            OrderType::Limit => "LIMIT",
            OrderType::StopLoss if position.is_some() => "STOP_MARKET",
            OrderType::TakeProfit if position.is_some() => "TAKE_PROFIT_MARKET",
            _ => "TRIGGER_MARKET",
        };
        let mut params = base().str("type", kind);
        if let Some(p) = price {
            params = params.num(if stop { "stopPrice" } else { "price" }, p);
        }
        if stop {
            params = params.str("workingType", working(o));
        }
        if o.kind == OrderType::Limit {
            params = params.str(
                "timeInForce",
                tif(o.time_in_force.unwrap_or(TimeInForce::GoodTillCancel)),
            );
        }
        if full {
            params = params.str("closePosition", "true");
        } else if !cx.store.hedged() && (kind == "TRIGGER_MARKET" || o.reduce_only == Some(true)) {
            params = params.boolean("reduceOnly", true);
        }
        let size = if full {
            position.expect("checked position").contracts
        } else {
            amount
        };
        let lots = if full {
            vec![size]
        } else {
            split(size, market.limits.amount.max, market.precision.amount)?
        };
        if !full && !closing && lots.iter().any(|lot| *lot < market.limits.amount.min) {
            return Err("Split order lot is below the market minimum".into());
        }
        if o.client_order_id.is_some() && lots.len() != 1 {
            return Err("Client order id requires exactly one native order".into());
        }
        let mut out = Vec::new();
        for &lot in &lots {
            let mut p = params.clone().num("quantity", lot);
            let mut receipts = Vec::new();
            if matches!(o.kind, OrderType::Limit | OrderType::Market) {
                let id = match &o.client_order_id {
                    Some(id) => id.clone(),
                    None => generated_id(cx)?,
                };
                p = p.str("clientOrderId", &id);
                receipts.push(OrderIdentityReceipt {
                    symbol: o.symbol.clone(),
                    kind: OrderIdentityKind::ClientOrderId,
                    identity: id,
                });
            }
            let mut step = place_step(p);
            step.receipts = receipts;
            out.push(step);
        }
        for (kind, trigger) in [
            (OrderType::StopLoss, o.stop_loss),
            (OrderType::TakeProfit, o.take_profit),
        ] {
            let Some(trigger) = trigger else { continue };
            if stop || full || o.reduce_only == Some(true) {
                return Err("Attached brackets require a plain entry order".into());
            }
            let trigger = num::adjust(
                positive(Some(trigger), "Bracket trigger")?,
                market.precision.price,
            );
            if !trigger.is_finite() || trigger <= 0.0 {
                return Err("Bracket trigger rounds to zero".into());
            }
            let source = match kind {
                OrderType::StopLoss => o.sl_trigger_type,
                OrderType::TakeProfit => o.tp_trigger_type,
                _ => unreachable!("bracket leg kind"),
            }
            .or(o.trigger_type)
            .unwrap_or(StopTriggerType::MarkPrice);
            let mut bracket = Params::new()
                .str("symbol", &market.id)
                .str(
                    "positionSide",
                    position_side(cx.store.hedged(), o.side.opposite(), true),
                )
                .str("side", side(o.side.opposite()))
                .str("type", "TRIGGER_MARKET")
                .num("stopPrice", trigger)
                .str("workingType", working_type(source)?)
                .str("timeInForce", "GTC");
            if !cx.store.hedged() {
                bracket = bracket.boolean("reduceOnly", true);
            }
            out.push(place_step(bracket.num("quantity", amount)));
        }
        Ok(out)
    }

    fn cancel(&self, cx: &Cx<'_, Pending>, ids: &[String]) -> Result<Job, String> {
        let mut job = Job {
            continue_on_error: true,
            ..Job::default()
        };
        for id in ids {
            let o = cx
                .store
                .order(id)
                .ok_or_else(|| format!("Order {id} not found"))?;
            let market = cx.store.market(&o.symbol).ok_or("Order market not found")?;
            job.steps.push_back(Step {
                receipts: Vec::new(),
                method: Method::Delete,
                path: ORDER,
                params: Params::new().str("symbol", &market.id).str("orderId", id),
                call: Call::Cancel {
                    ids: vec![id.clone()],
                    symbol: o.symbol.clone(),
                    absent_ok: true,
                },
            });
        }
        Ok(job)
    }
    fn cancel_all(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Result<Job, String> {
        let market = cx.store.market(symbol).ok_or("Market not found")?;
        let ids = cx
            .store
            .orders()
            .iter()
            .filter(|o| o.symbol == symbol && o.status == OrderStatus::Open)
            .map(|o| o.id.clone())
            .collect();
        Ok(one(
            Method::Delete,
            CANCEL_ALL,
            Params::new().str("symbol", &market.id),
            Call::Cancel {
                ids,
                symbol: symbol.into(),
                absent_ok: false,
            },
        ))
    }

    fn amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Job, String> {
        if price.is_some() == amount.is_some() {
            return Err("Amend requires exactly one price or amount change".into());
        }
        let o = cx.store.order(id).ok_or("Order not found")?;
        let meta = self
            .observed
            .get(id)
            .ok_or("Order has not been reconciled")?;
        if meta.order != *o
            || o.status != OrderStatus::Open
            || o.filled != 0.0
            || o.remaining != o.amount
            || !["BOTH", "LONG", "SHORT"].contains(&meta.position_side.as_str())
            || !["NEW", "OPEN", "PENDING", "WORKING"].contains(&meta.native_status.as_str())
        {
            return Err("Amend requires an exactly reconciled unfilled order".into());
        }
        let market = cx.store.market(&o.symbol).ok_or("Market not found")?;
        if matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit) {
            if amount.is_some()
                || !matches!(
                    meta.native_kind.as_str(),
                    "STOP_MARKET" | "TAKE_PROFIT_MARKET" | "TRIGGER_MARKET"
                )
            {
                return Err(
                    "Trigger amendment supports only reconciled STOP/TP price changes".into(),
                );
            }
            let next = num::adjust(positive(price, "Trigger price")?, market.precision.price);
            if !next.is_finite() || next <= 0.0 || next == o.price {
                return Err("Trigger price must change after rounding".into());
            }
            // TRIGGER_MARKET (attached brackets) has no native direction; the
            // mapped kind is its SL/TP side relative to mark.
            let kind = match meta.native_kind.as_str() {
                "STOP_MARKET" => OrderType::StopLoss,
                "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
                _ => o.kind,
            };
            let source = o
                .trigger_type
                .ok_or("Trigger amendment requires a reconciled trigger price source")?;
            working_type(source)?;
            let opts = PlaceOrderOpts {
                symbol: o.symbol.clone(),
                kind,
                side: o.side,
                amount: o.amount,
                price: Some(next),
                trigger_price: None,
                stop_loss: None,
                take_profit: None,
                reduce_only: Some(o.reduce_only),
                close: o.close,
                client_order_id: None,
                time_in_force: None,
                trigger_type: o.trigger_type,
                tp_trigger_type: o.trigger_type,
                sl_trigger_type: o.trigger_type,
                stp_mode: None,
                max_slippage_pct: None,
                bybit_bbo: None,
                bybit_slippage_tolerance_ticks: None,
                implied_volatility: None,
            };
            let mut replacement = self.format(cx, &opts)?;
            for step in &mut replacement {
                let params =
                    std::mem::take(&mut step.params).str("positionSide", &meta.position_side);
                // Like TS, a moved bracket takes the type placement picks today,
                // but a reduce-only bracket stays reduce-only whatever that type.
                step.params = if meta.native_kind == "TRIGGER_MARKET" {
                    if o.reduce_only && !cx.store.hedged() && o.close != Some(true) {
                        params.boolean("reduceOnly", true)
                    } else {
                        params
                    }
                } else {
                    params.str("type", &meta.native_kind)
                };
            }
            let guard = protection::Guard {
                order: o.clone(),
                trigger: Some(next),
                kind,
                source: Some(source),
                closing: o.reduce_only
                    || o.close == Some(true)
                    || matches!(
                        (meta.position_side.as_str(), o.side),
                        ("LONG", OrderSide::Sell) | ("SHORT", OrderSide::Buy)
                    ),
            };
            // Validate the original before reading; the full guard still gates admission
            // after the requested reference arrives and every destructive final Claim.
            let mut initial = guard.clone();
            if guard.needs_reference(cx.store, &self.references, cx.now) {
                initial.trigger = None;
            }
            initial
                .check(cx.store, &self.references, cx.now, false)
                .map_err(|e| e.message)?;
            if !meta.attached
                && matches!(
                    meta.native_kind.as_str(),
                    "STOP_MARKET" | "TAKE_PROFIT_MARKET"
                )
                && let Some(original) = meta
                    .original
                    .as_ref()
                    .filter(|v| v.matches(o, &meta.native_kind, &meta.position_side))
                && let Some(mut params) = original.params()
            {
                params = params
                    .num("stopPrice", next)
                    .str("cancelReplaceMode", "STOP_ON_FAILURE")
                    .integer(
                        "cancelOrderId",
                        id.parse()
                            .map_err(|_| "Order id is not a numeric int64; stop not changed")?,
                    )
                    .str(
                        "cancelRestrictions",
                        if meta.native_status == "PENDING" {
                            "ONLY_PENDING"
                        } else {
                            "ONLY_NEW"
                        },
                    );
                let mut job = one(
                    Method::Post,
                    REPLACE,
                    params,
                    Call::Replace { old_id: id.into() },
                );
                job.guard = Some(guard);
                job.restore = original.params();
                return Ok(job);
            }
            if let Some(original) = meta
                .original
                .as_ref()
                .filter(|v| v.matches(o, &meta.native_kind, &meta.position_side))
            {
                let params = original.params().ok_or(
                    "Exact native protection parameters are unavailable; stop not changed",
                )?;
                if replacement.len() != 1 {
                    return Err(
                        "Cannot split an exact native protective edit; stop not changed".into(),
                    );
                }
                replacement[0].params = params.num("stopPrice", next);
            }
            let mut job = self.cancel(cx, &[id.into()])?;
            job.guard = Some(guard);
            job.continue_on_error = false;
            job.replaces_cancelled = Some((opts.symbol.clone(), opts.side, opts.kind));
            if let Some(Step {
                call: Call::Cancel { absent_ok, .. },
                ..
            }) = job.steps.front_mut()
            {
                *absent_ok = false;
            }
            job.steps.extend(replacement);
            return Ok(job);
        }
        if o.kind != OrderType::Limit
            || meta.native_kind != "LIMIT"
            || o.close == Some(true)
            || !matches!(
                o.time_in_force,
                Some(TimeInForce::GoodTillCancel | TimeInForce::PostOnly)
            )
        {
            return Err("BingX amends only an open GTC or PostOnly LIMIT order".into());
        }
        // As legacy: a size change keeps the order and its TP/SL; a price move replaces the
        // order, and BingX's cancel-replace cannot carry attached TP/SL across.
        let protected = meta.attached || o.stop_loss.is_some() || o.take_profit.is_some();
        if amount.is_some() && price.is_some() {
            return Err("BingX changes a limit order's size or its price, not both at once".into());
        }
        if amount.is_none() && protected {
            return Err("BingX cannot move this limit order's price without dropping its TP/SL. Cancel it and place it again at the new price with its TP/SL, or change its size instead".into());
        }
        let base = Params::new().str("symbol", &market.id);
        if let Some(amount) = amount {
            let quantity =
                num::adjust(positive(Some(amount), "Quantity")?, market.precision.amount);
            if !quantity.is_finite()
                || quantity <= 0.0
                || quantity < market.limits.amount.min
                || quantity > market.limits.amount.max
                || quantity == o.amount
            {
                return Err("Amended quantity must change and remain within market limits".into());
            }
            return Ok(one(
                Method::Post,
                AMEND,
                base.str("orderId", id).num("quantity", quantity),
                Call::Amend {
                    id: id.into(),
                    symbol: o.symbol.clone(),
                    quantity,
                    step: market.precision.amount,
                },
            ));
        }
        let price = num::adjust(positive(price, "Price")?, market.precision.price);
        if !price.is_finite() || price <= 0.0 || price == o.price {
            return Err("Replacement price must change after rounding".into());
        }
        let mut params = base
            .str("side", side(o.side))
            .str("positionSide", &meta.position_side)
            .str("type", "LIMIT")
            .num("quantity", o.amount)
            .num("price", price)
            .str("timeInForce", tif(o.time_in_force.expect("checked TIF")))
            .str("cancelReplaceMode", "STOP_ON_FAILURE");
        // As legacy: a WORKING or OPEN order may still be PENDING at the venue.
        params = match meta.native_status.as_ref() {
            "PENDING" => params.str("cancelRestrictions", "ONLY_PENDING"),
            "NEW" => params.str("cancelRestrictions", "ONLY_NEW"),
            _ => params,
        };
        if let Some(client) = o.client_order_id.as_deref().filter(|id| !id.is_empty()) {
            params = params.str("cancelClientOrderId", client);
        } else {
            params = params.integer(
                "cancelOrderId",
                id.parse().map_err(|_| "Order id is not a numeric int64")?,
            );
        }
        let identity = generated_id(cx)?;
        params = params.str("clientOrderId", &identity);
        if !cx.store.hedged() && o.reduce_only {
            params = params.boolean("reduceOnly", true);
        }
        let mut job = one(
            Method::Post,
            REPLACE,
            params,
            Call::Replace {
                old_id: o.id.clone(),
            },
        );
        job.steps.front_mut().unwrap().receipts = vec![OrderIdentityReceipt {
            symbol: o.symbol.clone(),
            kind: OrderIdentityKind::ClientOrderId,
            identity,
        }];
        job.replaces_order = Some(o.symbol.clone());
        if o.reduce_only {
            let guard = protection::Guard {
                order: o.clone(),
                trigger: None,
                kind: o.kind,
                source: None,
                closing: true,
            };
            guard
                .check(cx.store, &self.references, cx.now, false)
                .map_err(|e| e.message)?;
            job.guard = Some(guard);
            job.restore = meta
                .original
                .as_ref()
                .and_then(protection::Original::params);
        }
        Ok(job)
    }
    fn leverage(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        input: f64,
        side: Option<PositionSide>,
    ) -> Result<Job, String> {
        let market = cx.store.market(symbol).ok_or("Market not found")?;
        let input = positive(Some(input), "Leverage")?;
        if !market.limits.leverage.min.is_finite()
            || !market.limits.leverage.max.is_finite()
            || market.limits.leverage.min > market.limits.leverage.max
        {
            return Err("Market leverage limits are invalid".into());
        }
        let leverage = input.clamp(market.limits.leverage.min, market.limits.leverage.max);
        let step = |side| Step {
            receipts: Vec::new(),
            method: Method::Post,
            path: crate::paths::LEVERAGE,
            params: Params::new()
                .str("symbol", &market.id)
                .str(
                    "side",
                    match side {
                        Some(PositionSide::Long) => "LONG",
                        Some(PositionSide::Short) => "SHORT",
                        None => "BOTH",
                    },
                )
                .num("leverage", leverage),
            call: Call::Leverage {
                symbol: symbol.into(),
                leverage,
                side,
            },
        };
        let mut job = Job::default();
        if cx.store.hedged() && side.is_none() {
            job.sets_both_hedged_sides = true;
            job.steps.push_back(step(Some(PositionSide::Long)));
            job.steps.push_back(step(Some(PositionSide::Short)));
        } else {
            job.steps
                .push_back(step(if cx.store.hedged() { side } else { None }));
        }
        Ok(job)
    }
    fn margin(&self, cx: &Cx<'_, Pending>, symbol: &str, isolated: bool) -> Result<Job, String> {
        let market = cx.store.market(symbol).ok_or("Market not found")?;
        Ok(one(
            Method::Post,
            "/openApi/swap/v2/trade/marginType",
            Params::new()
                .str("symbol", &market.id)
                .str("marginType", if isolated { "ISOLATED" } else { "CROSSED" }),
            Call::Margin {
                symbol: symbol.into(),
                isolated,
            },
        ))
    }
}
fn one(method: Method, path: &'static str, params: Params, call: Call) -> Job {
    Job {
        steps: VecDeque::from([Step {
            receipts: Vec::new(),
            method,
            path,
            params,
            call,
        }]),
        ..Job::default()
    }
}
fn place_step(params: Params) -> Step {
    Step {
        receipts: Vec::new(),
        method: Method::Post,
        path: ORDER,
        params,
        call: Call::Place,
    }
}
fn positive(value: Option<f64>, field: &str) -> Result<f64, String> {
    value
        .filter(|n| n.is_finite() && *n > 0.0)
        .ok_or_else(|| format!("{field} must be finite and greater than zero"))
}
fn generated_id(cx: &Cx<'_, Pending>) -> Result<String, String> {
    Ok(format!(
        "ts{}",
        safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?
            .to_ascii_lowercase()
    ))
}
fn side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "BUY"
    } else {
        "SELL"
    }
}
fn position_side(hedged: bool, side: OrderSide, closing: bool) -> &'static str {
    if !hedged {
        "BOTH"
    } else {
        match (side, closing) {
            (OrderSide::Buy, false) | (OrderSide::Sell, true) => "LONG",
            _ => "SHORT",
        }
    }
}
fn tif(value: TimeInForce) -> &'static str {
    match value {
        TimeInForce::GoodTillCancel => "GTC",
        TimeInForce::ImmediateOrCancel => "IOC",
        TimeInForce::FillOrKill => "FOK",
        TimeInForce::PostOnly => "PostOnly",
    }
}
fn working(o: &PlaceOrderOpts) -> &'static str {
    match o.kind {
        OrderType::TakeProfit => {
            if o.tp_trigger_type.or(o.trigger_type) == Some(StopTriggerType::MarkPrice) {
                "MARK_PRICE"
            } else {
                "CONTRACT_PRICE"
            }
        }
        _ => {
            if o.sl_trigger_type.or(o.trigger_type) == Some(StopTriggerType::LastPrice) {
                "CONTRACT_PRICE"
            } else {
                "MARK_PRICE"
            }
        }
    }
}
fn working_type(source: StopTriggerType) -> Result<&'static str, String> {
    match source {
        StopTriggerType::MarkPrice => Ok("MARK_PRICE"),
        StopTriggerType::LastPrice => Ok("CONTRACT_PRICE"),
        StopTriggerType::IndexPrice => {
            Err("BingX supports mark-price and last-price triggers".into())
        }
    }
}
fn split(amount: f64, max: f64, step: f64) -> Result<Vec<f64>, String> {
    if max <= 0.0 || max.is_nan() {
        return Err("Invalid market maximum quantity".into());
    }
    if amount <= max {
        return Ok(vec![amount]);
    }
    // Legacy TS: bingx.exchange.ts:1544-1564. Round equal lots and the native
    // remainder independently; do not redistribute their rounding difference.
    let lots = (amount / max).ceil();
    if lots > 10_000.0 {
        return Err("Order would require too many lots".into());
    }
    let rest = num::adjust(amount % max, step);
    let size = num::adjust((amount - rest) / lots, step);
    if !size.is_finite() || size <= 0.0 {
        return Err("Split order rounds to zero".into());
    }
    let mut result = vec![size; lots as usize];
    if rest > 0.0 {
        result.push(rest);
    }
    Ok(result)
}

fn decode(call: &Call, data: &Data<'_>, absent: bool) -> Result<Vec<String>, VenueError> {
    match call {
        Call::Place => Ok(vec![placement_id(data)?]),
        Call::Cancel { ids, .. } => {
            data.canceled()?;
            if !absent
                && let Some(id) = data.id().map_err(unknown)?
                && ids.len() == 1
                && id != ids[0]
            {
                return Err(unknown(
                    "BingX cancellation acknowledgement identifies another order",
                ));
            }
            if data
                .failed
                .as_ref()
                .is_some_and(|failed| !failed.is_empty())
            {
                return Err(unknown(
                    "BingX cancel response reports failed cancellations",
                ));
            }
            Ok(Vec::new())
        }
        Call::Amend {
            id,
            symbol,
            quantity,
            step,
        } => {
            data.accepted()?;
            if data.id().map_err(unknown)?.as_deref() != Some(id)
                || data.symbol.replace('-', "") != *symbol
                || data
                    .quantity
                    .as_ref()
                    .map(|v| num::adjust(v.value(), *step))
                    != Some(*quantity)
            {
                return Err(unknown(
                    "BingX amendment acknowledgement does not confirm order, symbol and quantity",
                ));
            }
            Ok(vec![id.clone()])
        }
        Call::Replace { old_id } => {
            let success = |s: &str| matches!(s, "SUCCESS" | "true" | "TRUE");
            if !success(&data.cancel_result) {
                return Err(unknown("BingX replacement did not confirm cancellation"));
            }
            if !success(&data.new_order_result) && !success(&data.replace_result) {
                return Err(unknown("BingX replacement did not confirm placement"));
            }
            let nested = if let Some(order) = &data.new_order_response {
                order.accepted()?;
                order.id().map_err(unknown)?
            } else {
                None
            };
            let id = matching_id(
                nested
                    .into_iter()
                    .chain(data.new_order_id.as_ref().map(Id::text)),
            )
            .map_err(unknown)?
            .filter(|id| id != old_id)
            .ok_or_else(|| unknown("BingX replacement response omitted a fresh order id"))?;
            Ok(vec![id])
        }
        Call::Leverage { .. } | Call::Margin { .. } | Call::Mode { .. } => Ok(Vec::new()),
    }
}
fn apply(trading: &mut Trading, cx: &mut Cx<'_, Pending>, call: &Call, canceled: bool) {
    match call {
        Call::Place => {}
        Call::Cancel { ids, .. } => {
            for id in ids {
                if cx
                    .store
                    .order(id)
                    .is_some_and(|o| o.status != OrderStatus::Closed)
                {
                    cx.store.set_order_status(id, OrderStatus::Canceled);
                }
                trading.observed.remove(id);
            }
        }
        Call::Amend { id, .. } => {
            trading.observed.remove(id);
        }
        Call::Replace { old_id } if canceled => {
            if cx
                .store
                .order(old_id)
                .is_some_and(|o| o.status != OrderStatus::Closed)
            {
                cx.store.set_order_status(old_id, OrderStatus::Canceled);
            }
            trading.observed.remove(old_id);
        }
        Call::Replace { .. } => {}
        Call::Leverage {
            symbol,
            leverage,
            side,
        } => {
            cx.store.update_setting(symbol, |s| match side {
                Some(PositionSide::Long) => {
                    s.leverage = *leverage;
                    s.buy_leverage = Some(*leverage);
                }
                Some(PositionSide::Short) => s.sell_leverage = Some(*leverage),
                None => {
                    s.leverage = *leverage;
                    s.buy_leverage = Some(*leverage);
                    s.sell_leverage = Some(*leverage);
                }
            });
            cx.store.update_positions(|p| {
                if p.symbol == *symbol && side.is_none_or(|side| p.side == side) {
                    p.leverage = *leverage;
                }
            });
            cx.hold_setting(SettingKey::Leverage(symbol.clone()));
        }
        Call::Margin { symbol, isolated } => {
            cx.store.update_setting(symbol, |s| {
                s.margin_type = Some(if *isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                })
            });
            cx.hold_setting(SettingKey::MarginType(symbol.clone()));
        }
        Call::Mode { hedged } => {
            cx.store.set_hedged(*hedged);
            cx.hold_setting(SettingKey::Hedged);
        }
    }
}

pub(crate) fn evidence(
    body: &str,
    evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
) {
    if let Ok(envelope) = parse_envelope(body)
        && let Some(data) = envelope.data
    {
        let mut ids = Vec::new();
        data.evidence_ids(&mut ids);
        for id in ids {
            evidence.order_id(&id);
        }
    }
}

#[cfg(test)]
mod reply_tests {
    #[test]
    fn a_cancel_all_with_a_null_failed_list_reads() {
        let data: super::Data<'_> = serde_json::from_str(r#"{"failed":null}"#).unwrap();
        assert!(data.failed.is_none());
    }
}
