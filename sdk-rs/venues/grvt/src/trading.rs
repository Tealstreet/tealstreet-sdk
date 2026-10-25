//! Preflighted sequential writes. A cancellation ACK alone never permits replacement.
use crate::account::{BUILDER_MAINNET, BUILDER_TESTNET, BuilderState, decode_order};
use crate::provider::{Create, Leg, Metadata, Placement, Signature, Trigger, TriggerDetails};
use crate::sign;
use crate::wire::{Number, Str, Text};
use crate::{Grvt, Job, Pending};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    BuilderAuthorization, BuilderAuthorizationReason, Command, CommandId, Cx, ErrorKind, Outcome,
    Response, VenueError,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet, VecDeque};

const CREATE: &str = "/lite/v1/create_order";
const CANCEL: &str = "/lite/v1/cancel_order";
const CANCEL_ALL: &str = "/lite/v1/cancel_all_orders";
const GET: &str = "/lite/v1/order";
const OPEN: &str = "/lite/v1/open_orders";
const LEVERAGE: &str = "/lite/v1/set_initial_leverage";

#[derive(Default)]
pub(crate) struct Trading {
    queue: VecDeque<Command>,
    busy: bool,
    // A builder check finished after the queued front command started waiting on it.
    builder_checked: bool,
}
pub struct Request {
    plan: Box<Plan>,
    step: Step,
}
impl Request {
    pub(crate) fn is_read(&self) -> bool {
        matches!(
            self.step.effect,
            Effect::Inspect(_) | Effect::Verify(_) | Effect::ReadSymbol(_)
        )
    }
    pub(crate) fn is_symbol_read(&self) -> bool {
        matches!(self.step.effect, Effect::ReadSymbol(_))
    }
    pub(crate) fn array_at(&self) -> Option<ArrayAt> {
        self.is_symbol_read()
            .then_some(ArrayAt::Envelope(&[&["r"], &["result"]]))
    }
    pub(crate) fn reset(&mut self) {
        if let Effect::ReadSymbol(read) = &mut self.step.effect {
            read.targets.clear();
            read.native_ids.clear();
            read.client_ids.clear();
            read.failed = None;
        }
    }
}
struct Plan {
    command: CommandId,
    steps: VecDeque<Step>,
    created: Vec<String>,
    affected: Vec<String>,
    guard: Option<Target>,
    scope: Option<CancelScope>,
    cancel_attempts: Vec<String>,
    /// A write step took effect, so a later refusal cannot mean nothing happened.
    effective: bool,
}
impl Plan {
    fn new(command: CommandId) -> Self {
        Self {
            command,
            steps: VecDeque::new(),
            created: vec![],
            affected: vec![],
            guard: None,
            scope: None,
            cancel_attempts: vec![],
            effective: false,
        }
    }
}
struct Step {
    path: &'static str,
    body: String,
    effect: Effect,
}
enum Effect {
    Place(Box<Order>),
    Cancel(String),
    CancelSymbol(String),
    ReadSymbol(Box<CancelRead>),
    CancelExact(Target),
    Inspect(Box<Amend>),
    Verify(Target),
    Leverage { symbol: String, leverage: f64 },
}
struct CancelScope {
    instrument: String,
    account: String,
    account_id: String,
    revision: u64,
    executions: u64,
    evidence: HashMap<String, (Option<String>, Option<u64>)>,
}
#[derive(Default)]
struct CancelRead {
    targets: Vec<Target>,
    native_ids: HashSet<u128>,
    client_ids: HashSet<u64>,
    failed: Option<String>,
    instrument: String,
    account: String,
    account_id: Option<String>,
    revision: u64,
    executions: u64,
    evidence: HashMap<String, (Option<String>, Option<u64>)>,
}

// open_orders is an authoritative complete snapshot, with no cursor/limit API.
// Unknown envelope fields can signal truncation; destructive reads fail closed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenSnapshot<'a> {
    #[serde(rename = "r", alias = "result")]
    _rows: [serde::de::IgnoredAny; 0],
    #[serde(borrow, rename = "ri", alias = "request_id", default)]
    _request_id: Option<Text<'a>>,
}
#[derive(Deserialize)]
struct ScopedOrder<'a> {
    #[serde(
        rename = "a",
        alias = "ack",
        default,
        deserialize_with = "field_present"
    )]
    has_ack: bool,
    #[serde(
        rename = "e",
        alias = "error",
        default,
        deserialize_with = "field_present"
    )]
    error: bool,
    #[serde(
        rename = "c",
        alias = "code",
        default,
        deserialize_with = "field_present"
    )]
    code: bool,
    #[serde(borrow, rename = "sa", alias = "sub_account_id")]
    account: Str<'a>,
    #[serde(borrow, rename = "oi", alias = "order_id", default)]
    native: Option<Str<'a>>,
    #[serde(borrow, rename = "l", alias = "legs")]
    legs: Vec<ScopedLeg<'a>>,
    #[serde(borrow, rename = "m", alias = "metadata", default)]
    metadata: Option<ScopedMetadata<'a>>,
}
fn field_present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    serde::de::IgnoredAny::deserialize(deserializer)?;
    Ok(true)
}
#[derive(Deserialize)]
struct ScopedLeg<'a> {
    #[serde(borrow, rename = "i", alias = "instrument")]
    instrument: Str<'a>,
}
#[derive(Deserialize)]
struct ScopedMetadata<'a> {
    #[serde(borrow, rename = "co", alias = "client_order_id", default)]
    client: Option<Str<'a>>,
}
#[derive(Clone)]
struct Target {
    id: String,
    native: Option<String>,
    client: Option<String>,
    filled: f64,
    amount: f64,
    created_ns: Option<u64>,
    execution_revision: u64,
    inspected_updated_ns: u64,
}
struct Amend {
    target: Target,
    source: Order,
    price: Option<f64>,
    amount: Option<f64>,
}
#[derive(Serialize)]
struct Identity<'a> {
    sa: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    oi: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    co: Option<&'a str>,
}
#[derive(Deserialize)]
struct Reply<'a> {
    #[serde(borrow, rename = "r", alias = "result")]
    result: &'a RawValue,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancellationReply<'a> {
    #[serde(borrow, rename = "r", alias = "result")]
    result: &'a RawValue,
    #[serde(borrow, rename = "ri", alias = "request_id", default)]
    _request_id: Option<Text<'a>>,
}
#[derive(Deserialize)]
struct Created<'a> {
    #[serde(borrow, rename = "oi", alias = "order_id", default)]
    id: Text<'a>,
    #[serde(borrow, rename = "m", alias = "metadata", default)]
    metadata: Option<CreatedMetadata<'a>>,
}
#[derive(Deserialize)]
struct CreatedMetadata<'a> {
    #[serde(borrow, rename = "co", alias = "client_order_id", default)]
    id: Text<'a>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Truth<'a> {
    Bool(bool),
    #[serde(borrow)]
    Text(Str<'a>),
}
impl Truth<'_> {
    fn yes(&self) -> bool {
        matches!(self, Self::Bool(true)) || matches!(self, Self::Text(text) if text == "true")
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ack<'a> {
    #[serde(borrow, rename = "a", alias = "ack")]
    ack: Truth<'a>,
}
#[derive(Deserialize)]
struct Success<'a> {
    #[serde(borrow, rename = "s", alias = "success")]
    success: Truth<'a>,
}
#[derive(Deserialize)]
struct Semantics<'a> {
    #[serde(rename = "im", alias = "is_market", default)]
    market: bool,
    #[serde(rename = "l", alias = "legs")]
    legs: Vec<SemanticLeg>,
    #[serde(borrow, rename = "m", alias = "metadata", default)]
    metadata: Option<SemanticMetadata<'a>>,
}
#[derive(Deserialize)]
struct SemanticLeg {
    #[serde(rename = "lp", alias = "limit_price", default)]
    price: Number,
}
#[derive(Deserialize)]
struct SemanticMetadata<'a> {
    #[serde(borrow, rename = "t", alias = "trigger", default)]
    trigger: Option<SemanticTrigger<'a>>,
}
#[derive(Deserialize)]
struct SemanticTrigger<'a> {
    #[serde(borrow, rename = "tt", alias = "trigger_type")]
    kind: Str<'a>,
    #[serde(rename = "t", alias = "tpsl")]
    details: SemanticTriggerDetails,
}
#[derive(Deserialize)]
struct SemanticTriggerDetails {
    #[serde(rename = "is", alias = "is_split_position")]
    split: bool,
}

fn positive(value: f64, name: &str) -> Result<(), String> {
    if value.is_finite() && value > 0. {
        Ok(())
    } else {
        Err(format!("Invalid GRVT {name}"))
    }
}
pub(crate) fn valid_client(value: &str) -> bool {
    !value.starts_with('0')
        && value.bytes().all(|b| b.is_ascii_digit())
        && value
            .parse::<u64>()
            .is_ok_and(|n| n > 0 && n <= i64::MAX as u64)
}
fn nonce(cx: &Cx<'_, Pending>, signing: bool) -> Result<u32, String> {
    let entropy = safe_cex_crypto::uuid::uuid_with_entropy(|bytes| {
        if signing {
            cx.fill_signing_entropy(bytes)
        } else {
            cx.fill_entropy(bytes)
        }
    })?;
    Ok(u32::from_be_bytes(
        safe_cex_crypto::ethereum::keccak256(entropy.as_bytes())[..4]
            .try_into()
            .unwrap(),
    ))
}
fn command_id(command: &Command) -> Option<CommandId> {
    match command {
        Command::PlaceOrders { command, .. }
        | Command::CancelOrders { command, .. }
        | Command::CancelSymbolOrders { command, .. }
        | Command::AmendOrder { command, .. }
        | Command::SetLeverage { command, .. }
        | Command::SetMarginType { command, .. }
        | Command::SetPositionMode { command, .. } => Some(*command),
        _ => None,
    }
}
fn terminal(status: OrderStatus) -> bool {
    matches!(status, OrderStatus::Closed | OrderStatus::Canceled)
}
fn native_identity(text: &str) -> Result<Option<u128>, String> {
    if text == "0" {
        return Ok(None);
    }
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .filter(|digits| {
            !digits.is_empty()
                && digits.len() <= 32
                && digits.bytes().all(|b| b.is_ascii_hexdigit())
        })
        .ok_or("Invalid GRVT cancellation native identity")?;
    let value = u128::from_str_radix(digits, 16)
        .map_err(|_| "Invalid GRVT cancellation native identity")?;
    Ok((value > 0).then_some(value))
}
fn client_identity(text: &str) -> Result<Option<u64>, String> {
    let value = text
        .parse::<u64>()
        .ok()
        .filter(|v| v.to_string() == text)
        .ok_or("Invalid GRVT cancellation client identity")?;
    Ok((value > 0).then_some(value))
}
fn trigger_by(kind: Option<StopTriggerType>) -> &'static str {
    match kind.unwrap_or(StopTriggerType::LastPrice) {
        StopTriggerType::LastPrice => "LAST",
        StopTriggerType::MarkPrice => "MARK",
        StopTriggerType::IndexPrice => "INDEX",
    }
}
impl Grvt {
    fn exact_cancel_ack(
        &self,
        cx: &Cx<'_, Pending>,
        plan: &Plan,
        target: &Target,
        response: &Response<'_>,
    ) -> Result<(), String> {
        if response.status != 200 {
            return Err("Incomplete GRVT cancellation acknowledgement".into());
        }
        let reply: CancellationReply<'_> = serde_json::from_str(response.body)
            .map_err(|_| "Malformed GRVT cancellation acknowledgement")?;
        if let Ok(ack) = serde_json::from_str::<Ack<'_>>(reply.result.get()) {
            return if ack.ack.yes() {
                Ok(())
            } else {
                Err("GRVT cancellation was not acknowledged".into())
            };
        }
        // The official API also returns the affected Order. It proves identity,
        // not finality: keep local cancellation pending until reconciliation.
        let scope = plan
            .scope
            .as_ref()
            .ok_or("Missing GRVT cancellation scope")?;
        let proof: ScopedOrder<'_> = serde_json::from_str(reply.result.get())
            .map_err(|_| "Missing GRVT cancellation acknowledgement")?;
        if proof.has_ack
            || proof.error
            || proof.code
            || proof.account != scope.account
            || proof.legs.len() != 1
            || proof.legs[0].instrument != scope.instrument
        {
            return Err("GRVT cancellation proof has a different account or instrument".into());
        }
        let decoded = decode_order(reply.result.get(), cx.store, cx.now)?;
        let returned_native = proof
            .native
            .as_deref()
            .map(native_identity)
            .transpose()?
            .flatten();
        let returned_client = proof
            .metadata
            .as_ref()
            .and_then(|m| m.client.as_deref())
            .map(client_identity)
            .transpose()?
            .flatten();
        if target
            .native
            .as_deref()
            .is_some_and(|id| native_identity(id).ok().flatten() != returned_native)
            || (target.native.is_none()
                && target
                    .client
                    .as_deref()
                    .and_then(|id| id.parse::<u64>().ok())
                    != returned_client)
            || target
                .client
                .as_deref()
                .zip(returned_client)
                .is_some_and(|(id, value)| id.parse::<u64>().ok() != Some(value))
            || target
                .created_ns
                .is_some_and(|created| decoded.created_ns > 0 && decoded.created_ns != created)
        {
            return Err("GRVT cancellation proof returned a different order identity".into());
        }
        Ok(())
    }
    pub(crate) fn trade_item(&self, cx: &Cx<'_, Pending>, request: &mut Request, item: &str) {
        if let Effect::ReadSymbol(read) = &mut request.step.effect
            && read.failed.is_none()
            && let Err(error) = self.cancel_snapshot_item(cx, read, item)
        {
            read.failed = Some(error);
        }
    }
    fn cancel_snapshot_item(
        &self,
        cx: &Cx<'_, Pending>,
        read: &mut CancelRead,
        item: &str,
    ) -> Result<(), String> {
        let scoped: ScopedOrder<'_> =
            serde_json::from_str(item).map_err(|_| "Malformed GRVT cancellation order scope")?;
        if scoped.has_ack
            || scoped.account != read.account
            || scoped.error
            || scoped.code
            || scoped.legs.len() != 1
            || scoped.legs[0].instrument.is_empty()
        {
            return Err("Invalid GRVT cancellation account or instrument scope".into());
        }
        let native = scoped
            .native
            .as_deref()
            .map(native_identity)
            .transpose()?
            .flatten();
        let client = scoped
            .metadata
            .as_ref()
            .and_then(|m| m.client.as_deref())
            .map(client_identity)
            .transpose()?
            .flatten();
        if (native.is_none() && client.is_none())
            || native.is_some_and(|id| !read.native_ids.insert(id))
            || client.is_some_and(|id| !read.client_ids.insert(id))
        {
            return Err("Missing or duplicate GRVT cancellation identity".into());
        }
        let decoded = decode_order(item, cx.store, cx.now)?;
        if scoped.legs[0].instrument != read.instrument || terminal(decoded.order.status) {
            return Ok(());
        }
        read.targets.push(Target {
            id: decoded.order.id,
            native: native.and_then(|_| scoped.native.map(|s| s.into_owned())),
            client: client.map(|id| id.to_string()),
            filled: decoded.order.filled,
            amount: decoded.order.amount,
            created_ns: (decoded.created_ns > 0).then_some(decoded.created_ns),
            execution_revision: self.account.execution_revision,
            inspected_updated_ns: decoded.updated_ns,
        });
        Ok(())
    }
    fn cancel_snapshot(
        &self,
        read: &CancelRead,
        response: &Response<'_>,
    ) -> Result<(CancelScope, Vec<Step>, Vec<String>), String> {
        if response.status != 200
            || self.trade_account()? != read.account
            || read.account_id.as_deref().is_some_and(|expected| {
                self.account
                    .session
                    .as_ref()
                    .is_none_or(|s| s.account_id != expected)
            })
        {
            return Err("Incomplete GRVT cancellation snapshot or changed account".into());
        }
        let _: OpenSnapshot<'_> = serde_json::from_str(response.body)
            .map_err(|_| "Incomplete or malformed GRVT cancellation snapshot")?;
        if let Some(error) = &read.failed {
            return Err(error.clone());
        }
        let mut steps = Vec::new();
        let mut affected = Vec::new();
        for target in &read.targets {
            affected.push(target.id.clone());
            steps.push(Step {
                path: CANCEL,
                body: self.identity(target)?,
                effect: Effect::CancelExact(target.clone()),
            });
        }
        Ok((
            CancelScope {
                instrument: read.instrument.clone(),
                account: read.account.clone(),
                account_id: self
                    .account
                    .session
                    .as_ref()
                    .ok_or("GRVT cancellation session is unavailable")?
                    .account_id
                    .clone(),
                revision: read.revision,
                executions: read.executions,
                evidence: read.evidence.clone(),
            },
            steps,
            affected,
        ))
    }
    fn guard_exact_cancel(
        &self,
        cx: &Cx<'_, Pending>,
        plan: &Plan,
        target: &Target,
    ) -> Result<(), String> {
        let scope = plan
            .scope
            .as_ref()
            .ok_or("Missing GRVT cancellation scope")?;
        if self.trade_account()? != scope.account
            || self.account.session.as_ref().map(|s| &s.account_id) != Some(&scope.account_id)
        {
            return Err("GRVT cancellation account changed".into());
        }
        if target.native.is_none() {
            let expected = scope.evidence.get(&target.id).cloned().unwrap_or_default();
            let current = (
                self.account.native_ids.get(&target.id).cloned(),
                self.account.order_created(&target.id),
            );
            if self.account.orders_revision != scope.revision
                || self.account.execution_revision != scope.executions
                || current != expected
            {
                return Err(
                    "GRVT client cancellation identity changed during snapshot or authentication"
                        .into(),
                );
            }
        }
        if let Some(progress) = self.account.order_progress(&target.id)
            && (terminal(progress.status)
                || target.created_ns.is_some_and(|created| {
                    progress.created_ns > 0 && created != progress.created_ns
                })
                || target
                    .native
                    .as_deref()
                    .zip(progress.native_id.as_deref())
                    .is_some_and(|(wanted, actual)| {
                        native_identity(wanted).ok() != native_identity(actual).ok()
                    }))
        {
            return Err(
                "GRVT cancellation target lifecycle changed; reconcile before retrying".into(),
            );
        }
        if cx.store.order(&target.id).is_some_and(|o| {
            terminal(o.status)
                || cx
                    .store
                    .market_by_id(&scope.instrument)
                    .is_none_or(|m| m.symbol != o.symbol)
        }) {
            return Err("GRVT cancellation target became terminal".into());
        }
        Ok(())
    }
    fn trade_account(&self) -> Result<&str, String> {
        if self.options.public_data_only {
            return Err("GRVT trading requires a private account".into());
        }
        self.account
            .trading_account_id
            .as_deref()
            .ok_or_else(|| "GRVT trading account is not loaded".into())
    }
    fn target(&self, order: &Order) -> Result<Target, String> {
        let native = self
            .account
            .native_ids
            .get(&order.id)
            .cloned()
            .or_else(|| order.id.starts_with("0x").then(|| order.id.clone()));
        let client = order
            .client_order_id
            .clone()
            .or_else(|| valid_client(&order.id).then(|| order.id.clone()));
        if native.is_none() && client.is_none() {
            return Err("GRVT order has no native cancellation identity".into());
        }
        Ok(Target {
            id: order.id.clone(),
            native,
            client,
            filled: order.filled,
            amount: order.amount,
            created_ns: self.account.order_created(&order.id),
            execution_revision: self.account.execution_revision,
            inspected_updated_ns: 0,
        })
    }
    fn guard_amend(&self, cx: &Cx<'_, Pending>, target: &Target) -> Result<(), String> {
        if self.account.execution_revision != target.execution_revision {
            return Err(
                "GRVT replacement aborted after an account execution; reconcile before retrying"
                    .into(),
            );
        }
        if cx.store.order(&target.id).is_some_and(|order| {
            order.filled > target.filled || order.status == OrderStatus::Closed
        }) || self
            .account
            .order_progress(&target.id)
            .is_some_and(|progress| {
                progress.filled > target.filled
                    || progress.amount != target.amount
                    || progress.status == OrderStatus::Closed
                    || target.native.as_deref().is_some_and(|expected| {
                        progress
                            .native_id
                            .as_deref()
                            .is_some_and(|native| native != expected)
                    })
                    || target.created_ns.is_some_and(|expected| {
                        progress.created_ns > 0 && progress.created_ns != expected
                    })
            })
        {
            return Err(
                "GRVT replacement aborted after order execution or lifecycle progress".into(),
            );
        }
        Ok(())
    }
    pub(crate) fn trade_pending_guard(
        &self,
        cx: &Cx<'_, Pending>,
        request: &Request,
    ) -> Result<(), String> {
        match &request.step.effect {
            Effect::CancelExact(target) => self.guard_exact_cancel(cx, &request.plan, target),
            Effect::Inspect(amend) => self.guard_amend(cx, &amend.target),
            Effect::Place(_) | Effect::Verify(_) => request
                .plan
                .guard
                .as_ref()
                .map_or(Ok(()), |target| self.guard_amend(cx, target)),
            _ => Ok(()),
        }
    }
    fn identity(&self, target: &Target) -> Result<String, String> {
        serde_json::to_string(&Identity {
            sa: self.trade_account()?,
            oi: target.native.as_deref(),
            co: if target.native.is_none() {
                target.client.as_deref()
            } else {
                None
            },
        })
        .map_err(|e| e.to_string())
    }
    fn placement(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
        parent: Option<&str>,
        execution: Option<(bool, f64)>,
        used: &mut HashSet<String>,
    ) -> Result<Step, VenueError> {
        if opts.stp_mode.is_some() {
            return Err("Per-order self-trade prevention is unsupported on GRVT".into());
        }
        if opts.bybit_bbo.is_some() || opts.bybit_slippage_tolerance_ticks.is_some() {
            return Err("Bybit-specific order options are unsupported on GRVT".into());
        }
        if !matches!(
            opts.kind,
            OrderType::Market | OrderType::Limit | OrderType::StopLoss | OrderType::TakeProfit
        ) {
            return Err("Unsupported GRVT order type".into());
        }
        if opts.client_order_id.is_some()
            && (!matches!(opts.kind, OrderType::Limit | OrderType::Market)
                || opts.trigger_price.is_some()
                || opts.stop_loss.is_some()
                || opts.take_profit.is_some())
        {
            return Err("GRVT caller client order IDs require a plain Limit/Market order".into());
        }
        let subaccount = self.trade_account()?;
        let subaccount_int = subaccount
            .parse::<u64>()
            .map_err(|_| "Invalid GRVT uint64 trading account")?;
        let secret = self
            .options
            .secret
            .as_deref()
            .ok_or("GRVT private key is required for signing")?;
        let signer = sign::signer_address(secret)?;
        match self.account.builder {
            BuilderState::Authorized => {}
            BuilderState::CapTooLow(raw) => {
                return Err(VenueError::new(
                    "GRVT builder fee authorization cap is below 300; re-authorize builder",
                )
                .builder_authorization(BuilderAuthorization {
                    code: "GRVT_BUILDER_UNAUTHORIZED",
                    reason: BuilderAuthorizationReason::CapTooLow,
                    required_raw: 300,
                    authorized_raw: Some(raw),
                }));
            }
            BuilderState::Unauthorized => {
                return Err(VenueError::new(
                    "GRVT builder is not authorized; authorize builder before placing orders",
                )
                .builder_authorization(BuilderAuthorization {
                    code: "GRVT_BUILDER_UNAUTHORIZED",
                    reason: BuilderAuthorizationReason::Unauthorized,
                    required_raw: 300,
                    authorized_raw: None,
                }));
            }
            BuilderState::Unknown | BuilderState::Checking => {
                return Err(format!(
                    "GRVT builder authorization could not be checked ({}); retry the order",
                    self.account
                        .builder_error
                        .as_deref()
                        .unwrap_or("check pending")
                )
                .into());
            }
        }
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or("Unknown GRVT order market")?;
        if !market.active {
            return Err("GRVT market is inactive".into());
        }
        let info = self
            .metadata
            .get(&market.id)
            .ok_or("GRVT instrument signing metadata is unavailable")?;
        let decimals = info
            .base_decimals
            .ok_or("GRVT base decimals are unavailable")?;
        info.quote_decimals
            .ok_or("GRVT quote decimals are unavailable")?;
        positive(opts.amount, "order amount")?;
        let amount = safe_cex_engine::num::adjust(opts.amount, market.precision.amount);
        positive(amount, "rounded order amount")?;
        if amount < market.limits.amount.min
            || (market.limits.amount.max > 0. && amount > market.limits.amount.max)
        {
            return Err("GRVT order amount is outside market limits".into());
        }
        let conditional = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
        let is_market = execution.map_or(
            matches!(
                opts.kind,
                OrderType::Market | OrderType::StopLoss | OrderType::TakeProfit
            ),
            |x| x.0,
        );
        let price = if is_market {
            0.
        } else {
            safe_cex_engine::num::adjust(
                execution.map_or(opts.price.unwrap_or(0.), |x| x.1),
                market.precision.price,
            )
        };
        if !is_market {
            positive(price, "limit price")?;
        }
        let native_amount = sign::scaled_u64(&amount.to_string(), decimals)?;
        let native_price = sign::scaled_u64(&price.to_string(), 9)?;
        if native_amount == 0 {
            return Err("GRVT native order amount is zero".into());
        }
        let tif = opts.time_in_force.unwrap_or(TimeInForce::GoodTillCancel);
        let (ti, tif_code, post_only) = match tif {
            TimeInForce::GoodTillCancel => ("GOOD_TILL_TIME", 1, false),
            TimeInForce::PostOnly => ("GOOD_TILL_TIME", 1, true),
            TimeInForce::ImmediateOrCancel => ("IMMEDIATE_OR_CANCEL", 3, false),
            TimeInForce::FillOrKill => ("FILL_OR_KILL", 4, false),
        };
        if is_market && post_only {
            return Err("GRVT market orders cannot be post-only".into());
        }
        let n = nonce(cx, true)?;
        let client = if let Some(id) = &opts.client_order_id {
            if !valid_client(id) {
                return Err("GRVT client order ID must be a canonical integer between 1 and 9223372036854775807".into());
            }
            id.clone()
        } else {
            let mut generated = nonce(cx, false)?.max(1).to_string();
            for _ in 0..16 {
                if !used.contains(&generated) && cx.store.order(&generated).is_none() {
                    break;
                }
                generated = nonce(cx, false)?.max(1).to_string();
            }
            generated
        };
        if !used.insert(client.clone())
            || cx.store.orders().iter().any(|o| {
                !terminal(o.status)
                    && (o.id == client || o.client_order_id.as_deref() == Some(&client))
            })
        {
            return Err("GRVT client order ID is already active or repeated in batch".into());
        }
        if !cx.now.is_finite() || cx.now < 0. || cx.now > u64::MAX as f64 {
            return Err("Invalid GRVT signing clock".into());
        }
        let expiry = sign::expiration(cx.now as u64)?;
        let builder = if self.options.testnet {
            BUILDER_TESTNET
        } else {
            BUILDER_MAINNET
        };
        let signed_legs = [sign::OrderLeg {
            asset_id: sign::parse_asset_id(&info.hash)?,
            contract_size: native_amount,
            limit_price: native_price,
            is_buying: opts.side == OrderSide::Buy,
        }];
        let signed = sign::sign_order(
            &sign::Order {
                sub_account_id: subaccount_int,
                is_market,
                time_in_force: tif_code,
                post_only,
                reduce_only: opts.reduce_only.unwrap_or(false),
                legs: &signed_legs,
                builder: Some(sign::BuilderFee {
                    address: sign::parse_address(builder)?,
                    fee: sign::builder_fee("0.03")?,
                }),
                nonce: n,
                expiration: expiry,
            },
            secret,
            self.options.testnet,
        )?;
        let mut trigger_type = None;
        let mut trigger_price = price;
        let trigger = if conditional {
            let value = opts
                .trigger_price
                .or(opts.price)
                .ok_or("GRVT trigger price is required")?;
            positive(value, "trigger price")?;
            trigger_price = safe_cex_engine::num::adjust(value, market.precision.price);
            positive(trigger_price, "rounded trigger price")?;
            sign::scaled_u64(&trigger_price.to_string(), 9)?;
            trigger_type = Some(
                if opts.kind == OrderType::StopLoss {
                    opts.sl_trigger_type.or(opts.trigger_type)
                } else {
                    opts.tp_trigger_type.or(opts.trigger_type)
                }
                .unwrap_or(StopTriggerType::LastPrice),
            );
            Some(Trigger {
                tt: if opts.kind == OrderType::StopLoss {
                    "STOP_LOSS"
                } else {
                    "TAKE_PROFIT"
                },
                t: TriggerDetails {
                    tb: trigger_by(trigger_type),
                    tp: trigger_price.to_string(),
                    cp: opts.close.unwrap_or(false),
                    is: false,
                },
            })
        } else {
            None
        };
        let create_time = (cx.now as u64)
            .checked_mul(1_000_000)
            .ok_or("GRVT create time overflow")?;
        let body = serde_json::to_string(&Create {
            o: Placement {
                sa: subaccount.to_owned(),
                im: is_market,
                ti,
                po: post_only,
                ro: opts.reduce_only.unwrap_or(false),
                l: [Leg {
                    i: market.id.clone(),
                    s: amount.to_string(),
                    lp: (!is_market).then(|| price.to_string()),
                    ib: opts.side == OrderSide::Buy,
                }],
                s: Signature {
                    s: signer,
                    r: signed.r,
                    s1: signed.s,
                    v: signed.v,
                    e: expiry.to_string(),
                    n,
                    ci: if self.options.testnet { "326" } else { "325" },
                },
                m: Metadata {
                    co: client.clone(),
                    ct: create_time.to_string(),
                    t: trigger,
                },
                b: builder,
                bf: "0.03",
            },
        })
        .map_err(|e| e.to_string())?;
        Ok(Step {
            path: CREATE,
            body,
            effect: Effect::Place(Box::new(Order {
                id: client.clone(),
                parent_id: parent.map(str::to_owned),
                status: OrderStatus::Creating,
                symbol: market.symbol.clone(),
                kind: opts.kind,
                side: opts.side,
                price: trigger_price,
                amount,
                filled: 0.,
                remaining: amount,
                reduce_only: opts.reduce_only.unwrap_or(false),
                close: conditional.then_some(opts.close.unwrap_or(false)),
                client_order_id: Some(client),
                time_in_force: Some(tif),
                last_update: cx.now,
                trigger_type,
                take_profit: None,
                stop_loss: None,
                extra: None,
            })),
        })
    }
    fn trade_plan(&self, cx: &Cx<'_, Pending>, command: Command) -> Result<Plan, VenueError> {
        let mut plan = Plan::new(command_id(&command).ok_or("Unsupported GRVT trading command")?);
        self.trade_account()?;
        match command {
            Command::PlaceOrders { orders, .. } => {
                let mut used = HashSet::new();
                for opts in orders {
                    if matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit)
                        && (opts.take_profit.is_some() || opts.stop_loss.is_some())
                    {
                        return Err("GRVT trigger orders cannot carry attached brackets".into());
                    }
                    let entry = self.placement(cx, &opts, None, None, &mut used)?;
                    let Effect::Place(entry_order) = &entry.effect else {
                        unreachable!()
                    };
                    let parent = entry_order.id.clone();
                    plan.steps.push_back(entry);
                    for (kind, price, trigger_type) in [
                        (OrderType::StopLoss, opts.stop_loss, opts.sl_trigger_type),
                        (
                            OrderType::TakeProfit,
                            opts.take_profit,
                            opts.tp_trigger_type,
                        ),
                    ] {
                        if let Some(price) = price {
                            positive(price, "bracket trigger price")?;
                            let child = PlaceOrderOpts {
                                symbol: opts.symbol.clone(),
                                kind,
                                side: if opts.side == OrderSide::Buy {
                                    OrderSide::Sell
                                } else {
                                    OrderSide::Buy
                                },
                                amount: opts.amount,
                                price: Some(price),
                                trigger_price: Some(price),
                                stop_loss: None,
                                take_profit: None,
                                reduce_only: Some(true),
                                time_in_force: Some(TimeInForce::GoodTillCancel),
                                trigger_type,
                                tp_trigger_type: trigger_type,
                                sl_trigger_type: trigger_type,
                                client_order_id: None,
                                max_slippage_pct: None,
                                close: Some(false),
                                stp_mode: None,
                                bybit_bbo: None,
                                bybit_slippage_tolerance_ticks: None,
                                implied_volatility: None,
                            };
                            plan.steps.push_back(self.placement(
                                cx,
                                &child,
                                Some(&parent),
                                Some((true, 0.)),
                                &mut used,
                            )?);
                        }
                    }
                }
            }
            Command::CancelOrders { ids, .. } => {
                let mut seen = HashSet::new();
                for id in ids {
                    if !seen.insert(id.clone()) {
                        continue;
                    }
                    let order = cx.store.order(&id).ok_or("Unknown GRVT order to cancel")?;
                    if terminal(order.status) {
                        continue;
                    }
                    let target = self.target(order)?;
                    plan.affected.push(id.clone());
                    plan.steps.push_back(Step {
                        path: CANCEL,
                        body: self.identity(&target)?,
                        effect: Effect::Cancel(id),
                    });
                }
            }
            Command::CancelSymbolOrders { symbol, .. } => {
                #[derive(Serialize)]
                struct Account<'a> {
                    sa: &'a str,
                }
                let body = serde_json::to_string(&Account {
                    sa: self.trade_account()?,
                })
                .map_err(|e| e.to_string())?;
                if symbol.is_empty() {
                    plan.affected.extend(
                        cx.store
                            .orders()
                            .iter()
                            .filter(|o| !terminal(o.status))
                            .map(|o| o.id.clone()),
                    );
                    plan.steps.push_back(Step {
                        path: CANCEL_ALL,
                        body,
                        effect: Effect::CancelSymbol(symbol),
                    });
                } else {
                    let market = cx
                        .store
                        .market(&symbol)
                        .ok_or("Unknown GRVT cancellation market")?;
                    plan.steps.push_back(Step {
                        path: OPEN,
                        body,
                        effect: Effect::ReadSymbol(Box::new(CancelRead {
                            instrument: market.id.clone(),
                            account: self.trade_account()?.to_owned(),
                            ..CancelRead::default()
                        })),
                    });
                }
            }
            Command::AmendOrder {
                id, price, amount, ..
            } => {
                if price.is_none() && amount.is_none() {
                    return Err("GRVT amendment requires a price or amount".into());
                }
                if let Some(value) = price {
                    positive(value, "amendment price")?;
                }
                if let Some(value) = amount {
                    positive(value, "amendment amount")?;
                }
                let order = cx.store.order(&id).ok_or("Unknown GRVT order to amend")?;
                if terminal(order.status) {
                    return Err("Cannot amend terminal GRVT order".into());
                }
                if order.kind == OrderType::Market && price.is_some() {
                    return Err("Cannot change the price of a GRVT market order".into());
                }
                if cx
                    .store
                    .orders()
                    .iter()
                    .any(|o| o.parent_id.as_deref() == Some(&id) && !terminal(o.status))
                {
                    return Err(
                        "Cannot safely amend GRVT entry with active bracket children".into(),
                    );
                }
                let target = self.target(order)?;
                if target.native.is_none() && target.created_ns.is_none() {
                    return Err(
                        "Cannot safely amend GRVT order before its native identity is known".into(),
                    );
                }
                self.guard_amend(cx, &target)?;
                plan.steps.push_back(Step {
                    path: GET,
                    body: self.identity(&target)?,
                    effect: Effect::Inspect(Box::new(Amend {
                        target,
                        source: order.clone(),
                        price,
                        amount,
                    })),
                });
            }
            Command::SetLeverage {
                symbol, leverage, ..
            } => {
                positive(leverage, "leverage")?;
                let market = cx
                    .store
                    .market(&symbol)
                    .ok_or("Unknown GRVT leverage market")?;
                let leverage = leverage.clamp(
                    market.limits.leverage.min.max(1.),
                    market.limits.leverage.max.max(1.),
                );
                #[derive(Serialize)]
                struct Set<'a> {
                    sa: &'a str,
                    i: &'a str,
                    l: String,
                }
                plan.steps.push_back(Step {
                    path: LEVERAGE,
                    body: serde_json::to_string(&Set {
                        sa: self.trade_account()?,
                        i: &market.id,
                        l: leverage.to_string(),
                    })
                    .map_err(|e| e.to_string())?,
                    effect: Effect::Leverage { symbol, leverage },
                });
            }
            _ => return Err("Unsupported GRVT trading command".into()),
        }
        Ok(plan)
    }
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) -> bool {
        let Some(id) = command_id(&command) else {
            return false;
        };
        match &command {
            Command::SetMarginType { .. } => {
                let error = not_sent("GRVT margin mode cannot be changed through this integration");
                let Ok(context) = cx.admit_http_command(Policy {
                    operation: Operation::Mutation,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return true;
                };
                let _ = cx.fail_context(&context, error.kind(ErrorKind::Unsupported));
                return true;
            }
            Command::SetPositionMode { hedged, .. } => {
                if *hedged {
                    let error = not_sent("GRVT supports one-way positions only");
                    let Ok(context) = cx.admit_http_command(Policy {
                        operation: Operation::Mutation,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    }) else {
                        return true;
                    };
                    let _ = cx.fail_context(&context, error.kind(ErrorKind::Unsupported));
                } else {
                    let _ = cx.done_local_setting("[]".into(), |store| store.set_hedged(false));
                }
                return true;
            }
            _ => {}
        }
        if !self.awaits_builder(&command)
            && let Err(error) = self.trade_plan(cx, command.clone())
        {
            cx.fail(id, not_sent(error).kind(ErrorKind::InvalidInput));
            return true;
        }
        self.trading.queue.push_back(command);
        self.trade_next(cx);
        true
    }
    fn awaits_builder(&self, command: &Command) -> bool {
        matches!(
            command,
            Command::PlaceOrders { .. } | Command::AmendOrder { .. }
        ) && matches!(
            self.account.builder,
            BuilderState::Unknown | BuilderState::Checking
        ) && self.account.trading_account_id.is_some()
    }
    pub(crate) fn builder_checked(&mut self, cx: &mut Cx<'_, Pending>) {
        self.trading.builder_checked = !self.trading.queue.is_empty();
        self.trade_next(cx);
    }
    fn trade_next(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.busy {
            return;
        }
        let Some(front) = self.trading.queue.front() else {
            return;
        };
        if self.awaits_builder(front) && !self.trading.builder_checked {
            self.check_builder(cx);
            return;
        }
        self.trading.builder_checked = false;
        let Some(command) = self.trading.queue.pop_front() else {
            return;
        };
        let id = command_id(&command).unwrap();
        match self.trade_plan(cx, command) {
            Ok(plan) => {
                self.trading.busy = true;
                self.account.orders_revision += 1;
                self.trade_step(cx, plan);
            }
            Err(error) => {
                cx.fail(id, not_sent(error).kind(ErrorKind::InvalidInput));
                self.trade_next(cx);
            }
        }
    }
    fn trade_step(&mut self, cx: &mut Cx<'_, Pending>, mut plan: Plan) {
        let Some(mut step) = plan.steps.pop_front() else {
            self.trade_finish(cx, plan, None);
            return;
        };
        if matches!(step.effect, Effect::Place(_) | Effect::Verify(_))
            && let Some(target) = plan.guard.as_ref()
            && let Err(error) = self.guard_amend(cx, target)
        {
            self.trade_finish(cx, plan, Some(not_sent(error)));
            return;
        }
        if let Effect::CancelExact(target) = &step.effect
            && let Err(error) = self.guard_exact_cancel(cx, &plan, target)
        {
            self.trade_finish(cx, plan, Some(not_sent(error)));
            return;
        }
        if let Effect::ReadSymbol(read) = &mut step.effect {
            read.account_id = self.account.session.as_ref().map(|s| s.account_id.clone());
            read.revision = self.account.orders_revision;
            read.executions = self.account.execution_revision;
            read.evidence = self.account.cancellation_evidence();
        }
        match &step.effect {
            Effect::CancelExact(target) => {
                plan.cancel_attempts.push(target.id.clone());
                cx.store
                    .set_order_status(&target.id, OrderStatus::Canceling);
            }
            Effect::Place(order) => {
                if let Some(identity) = &order.client_order_id {
                    cx.command_progress(
                        plan.command,
                        vec![OrderIdentityReceipt {
                            symbol: order.symbol.clone(),
                            kind: OrderIdentityKind::ClientOrderId,
                            identity: identity.clone(),
                        }],
                    );
                }
                self.account
                    .record_identity(&order.id, None, order.last_update);
                cx.store.upsert_order(*order.clone());
            }
            Effect::Cancel(id) => {
                cx.store.set_order_status(id, OrderStatus::Canceling);
            }
            Effect::CancelSymbol(symbol) => {
                for id in &plan.affected {
                    if cx
                        .store
                        .order(id)
                        .is_some_and(|o| symbol.is_empty() || o.symbol == *symbol)
                    {
                        cx.store.set_order_status(id, OrderStatus::Canceling);
                    }
                }
            }
            _ => {}
        }
        self.request(
            cx,
            step.path,
            step.body.clone(),
            Job::Trade(Request {
                plan: Box::new(plan),
                step,
            }),
        );
    }
    fn trade_finish(&mut self, cx: &mut Cx<'_, Pending>, plan: Plan, error: Option<VenueError>) {
        for id in &plan.affected {
            if error.is_some()
                && plan.scope.is_none()
                && cx
                    .store
                    .order(id)
                    .is_some_and(|o| o.status == OrderStatus::Canceling)
            {
                cx.store.set_order_status(id, OrderStatus::Open);
            }
        }
        self.account.orders_revision += 1;
        self.trading.busy = false;
        match error {
            Some(mut error) => {
                if !plan.created.is_empty() {
                    let created = plan.created.join(",");
                    error.message = format!(
                        "{}; already created GRVT order IDs: {created}",
                        error.message
                    );
                } else if !plan.cancel_attempts.is_empty() {
                    let attempts = plan.cancel_attempts.join(",");
                    error.message = format!(
                        "{}; reconcile GRVT cancellation identities: {attempts}",
                        error.message
                    );
                }
                // A later refusal after a write took effect cannot mean nothing happened.
                let affected =
                    plan.effective || !plan.created.is_empty() || !plan.cancel_attempts.is_empty();
                if affected && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent)) {
                    error.outcome = None;
                }
                cx.fail(plan.command, error);
            }
            None => cx.done(
                plan.command,
                serde_json::to_string(&plan.created).expect("GRVT result IDs"),
            ),
        }
        self.refresh_orders(cx);
        self.refresh_positions(cx);
        self.trade_next(cx);
    }
    /// `outcome` is `Unknown` unless the write definitively did not reach the book.
    pub(crate) fn trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
        outcome: Outcome,
    ) {
        self.trade_failed_error(cx, request, VenueError::new(error), outcome);
    }
    fn trade_failed_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
        outcome: Outcome,
    ) {
        let message = if let Effect::Place(order) = &request.step.effect
            && outcome == Outcome::Unknown
        {
            format!(
                "{error}; reconcile GRVT client order ID {} before retrying",
                order.id
            )
        } else {
            error.message.clone()
        };
        error.message = message;
        self.trade_finish(cx, *request.plan, Some(error.outcome(outcome)));
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if let Err(error) = crate::wire::check(&response) {
            let outcome = crate::wire::write_outcome(&response);
            self.trade_failed(cx, request, &error, outcome);
            return;
        }
        let result = (|| -> Result<(), VenueError> {
            match &request.step.effect {
                Effect::ReadSymbol(read) => {
                    let (scope, steps, affected) = self.cancel_snapshot(read, &response)?;
                    request.plan.scope = Some(scope);
                    request.plan.affected = affected;
                    // Every row is validated before any new step can be dispatched.
                    for step in &steps {
                        if let Effect::CancelExact(target) = &step.effect {
                            self.guard_exact_cancel(cx, &request.plan, target)?;
                        }
                    }
                    request.plan.steps.extend(steps);
                }
                Effect::CancelExact(target) => {
                    if let Err(error) = self.exact_cancel_ack(cx, &request.plan, target, &response)
                    {
                        if error == "GRVT cancellation was not acknowledged"
                            && cx
                                .store
                                .order(&target.id)
                                .is_some_and(|o| o.status == OrderStatus::Canceling)
                        {
                            cx.store.set_order_status(&target.id, OrderStatus::Open);
                        }
                        return Err(error.into());
                    }
                    if let Some(scope) = &mut request.plan.scope {
                        scope.revision += 1;
                    }
                }
                Effect::Place(order) => {
                    let reply: Reply<'_> = serde_json::from_str(response.body)
                        .map_err(|_| "Malformed GRVT create response")?;
                    let created: Created<'_> = serde_json::from_str(reply.result.get())
                        .map_err(|_| "Malformed GRVT create identity")?;
                    if created.id.is_empty()
                        && created.metadata.as_ref().is_none_or(|m| m.id.is_empty())
                    {
                        return Err("GRVT create response has no identity".into());
                    }
                    if created
                        .metadata
                        .as_ref()
                        .is_some_and(|m| !m.id.is_empty() && *m.id != order.id)
                    {
                        return Err(
                            "GRVT create response returned a different client order ID".into()
                        );
                    }
                    if !created.id.is_empty() && !matches!(&*created.id, "0" | "0x00" | "0x0") {
                        self.account.record_identity(
                            &order.id,
                            Some(&created.id),
                            order.last_update,
                        );
                    }
                    if cx
                        .store
                        .order(&order.id)
                        .is_some_and(|o| o.status == OrderStatus::Creating)
                    {
                        cx.store.set_order_status(&order.id, OrderStatus::Open);
                    }
                    request.plan.created.push(order.id.clone());
                }
                Effect::Cancel(_) => {
                    require_ack(response.body)?;
                }
                Effect::CancelSymbol(_) => {
                    require_ack(response.body)?;
                }
                Effect::Leverage { symbol, leverage } => {
                    let success: Success<'_> = serde_json::from_str(response.body)
                        .map_err(|_| "Malformed GRVT leverage response")?;
                    if !success.success.yes() {
                        return Err("GRVT leverage update was rejected".into());
                    }
                    self.account.settings_revision += 1;
                    cx.store
                        .update_setting(symbol, |setting| setting.leverage = *leverage);
                }
                Effect::Inspect(amend) => {
                    self.guard_amend(cx, &amend.target)?;
                    let raw: Reply<'_> = serde_json::from_str(response.body)
                        .map_err(|_| "Malformed GRVT order lookup")?;
                    let decoded = decode_order(raw.result.get(), cx.store, cx.now)?;
                    let source = &decoded.order;
                    if self
                        .account
                        .order_progress(&amend.target.id)
                        .is_some_and(|progress| decoded.updated_ns < progress.updated_ns)
                    {
                        return Err(
                            "GRVT amendment inspection predates a known order update".into()
                        );
                    }
                    ensure_identity(
                        &amend.target,
                        source,
                        decoded.native_id.as_deref(),
                        decoded.created_ns,
                    )?;
                    if terminal(source.status)
                        || source.filled != amend.target.filled
                        || source.amount != amend.target.amount
                    {
                        return Err("GRVT order changed before amendment".into());
                    }
                    let semantics: Semantics<'_> = serde_json::from_str(raw.result.get())
                        .map_err(|_| "Incomplete GRVT replacement semantics")?;
                    if semantics.legs.len() != 1 {
                        return Err("Unsupported GRVT multi-leg replacement".into());
                    }
                    if source.kind != amend.source.kind
                        || source.side != amend.source.side
                        || source.symbol != amend.source.symbol
                    {
                        return Err("GRVT order semantics changed before amendment".into());
                    }
                    let tif = source
                        .time_in_force
                        .ok_or("Cannot amend GRVT order with unknown time-in-force")?;
                    if let Some(trigger) =
                        semantics.metadata.as_ref().and_then(|m| m.trigger.as_ref())
                    {
                        if !matches!(trigger.kind.as_ref(), "STOP_LOSS" | "TAKE_PROFIT")
                            || trigger.details.split
                        {
                            return Err("Cannot amend GRVT split-position trigger".into());
                        }
                        if source.close.is_none() || source.trigger_type.is_none() {
                            return Err(
                                "Cannot amend GRVT order with incomplete trigger metadata".into()
                            );
                        }
                    } else if matches!(source.kind, OrderType::StopLoss | OrderType::TakeProfit) {
                        return Err("Cannot amend GRVT order with missing trigger metadata".into());
                    }
                    let amount = amend.amount.unwrap_or(source.amount);
                    if amount <= source.filled {
                        return Err(
                            "GRVT amended amount must exceed already-filled quantity".into()
                        );
                    }
                    let conditional =
                        matches!(source.kind, OrderType::StopLoss | OrderType::TakeProfit);
                    let opts = PlaceOrderOpts {
                        symbol: source.symbol.clone(),
                        kind: source.kind,
                        side: source.side,
                        amount: amount - source.filled,
                        price: Some(amend.price.unwrap_or(source.price)),
                        trigger_price: conditional.then_some(amend.price.unwrap_or(source.price)),
                        stop_loss: None,
                        take_profit: None,
                        reduce_only: Some(source.reduce_only),
                        time_in_force: Some(tif),
                        trigger_type: source.trigger_type,
                        tp_trigger_type: source.trigger_type,
                        sl_trigger_type: source.trigger_type,
                        client_order_id: None,
                        max_slippage_pct: None,
                        close: source.close,
                        stp_mode: None,
                        bybit_bbo: None,
                        bybit_slippage_tolerance_ticks: None,
                        implied_volatility: None,
                    };
                    let replacement = self.placement(
                        cx,
                        &opts,
                        amend.source.parent_id.as_deref(),
                        conditional.then_some((semantics.market, semantics.legs[0].price.0)),
                        &mut HashSet::new(),
                    )?;
                    let mut target = amend.target.clone();
                    target.inspected_updated_ns = decoded.updated_ns;
                    if decoded.native_id.is_some() {
                        target.native = decoded.native_id;
                    }
                    request.plan.guard = Some(target.clone());
                    request.plan.affected.push(target.id.clone());
                    request.plan.steps.push_back(Step {
                        path: CANCEL,
                        body: self.identity(&target)?,
                        effect: Effect::Cancel(target.id.clone()),
                    });
                    request.plan.steps.push_back(Step {
                        path: GET,
                        body: self.identity(&target)?,
                        effect: Effect::Verify(target),
                    });
                    request.plan.steps.push_back(replacement);
                }
                Effect::Verify(target) => {
                    self.guard_amend(cx, target)?;
                    let raw: Reply<'_> = serde_json::from_str(response.body)
                        .map_err(|_| "Malformed GRVT cancellation confirmation")?;
                    let decoded = decode_order(raw.result.get(), cx.store, cx.now)?;
                    ensure_identity(
                        target,
                        &decoded.order,
                        decoded.native_id.as_deref(),
                        decoded.created_ns,
                    )?;
                    if decoded.updated_ns < target.inspected_updated_ns
                        || self
                            .account
                            .order_progress(&target.id)
                            .is_some_and(|progress| decoded.updated_ns < progress.updated_ns)
                    {
                        return Err("GRVT cancellation proof predates a known order update".into());
                    }
                    let state: CancelState<'_> = serde_json::from_str(raw.result.get())
                        .map_err(|_| "Missing GRVT cancellation state")?;
                    if state.state.status != "CANCELLED"
                        || decoded.order.filled != target.filled
                        || decoded.order.amount != target.amount
                    {
                        return Err("GRVT cancellation is not final or execution advanced; replacement aborted".into());
                    }
                    cx.store.set_order_status(&target.id, OrderStatus::Canceled);
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                if !request.is_read() {
                    self.account.orders_revision += 1;
                    request.plan.effective = true;
                }
                self.trade_step(cx, *request.plan);
            }
            Err(error) => self.trade_failed_error(cx, request, error, Outcome::Unknown),
        }
    }
}
#[derive(Deserialize)]
struct CancelState<'a> {
    #[serde(borrow, rename = "s1", alias = "state")]
    state: Status<'a>,
}
#[derive(Deserialize)]
struct Status<'a> {
    #[serde(borrow, rename = "s", alias = "status")]
    status: Str<'a>,
}
fn require_ack(body: &str) -> Result<(), String> {
    let reply: Reply<'_> =
        serde_json::from_str(body).map_err(|_| "Malformed GRVT cancellation acknowledgement")?;
    let ack: Ack<'_> = serde_json::from_str(reply.result.get())
        .map_err(|_| "Missing GRVT cancellation acknowledgement")?;
    if ack.ack.yes() {
        Ok(())
    } else {
        Err("GRVT cancellation was not acknowledged".into())
    }
}
fn ensure_identity(
    target: &Target,
    order: &Order,
    native: Option<&str>,
    created_ns: u64,
) -> Result<(), String> {
    if order.id != target.id
        || target
            .created_ns
            .is_some_and(|expected| expected != created_ns)
        || target
            .native
            .as_deref()
            .is_some_and(|expected| native != Some(expected))
        || target
            .client
            .as_deref()
            .is_some_and(|expected| order.client_order_id.as_deref() != Some(expected))
    {
        return Err("GRVT lookup returned a different order identity".into());
    }
    Ok(())
}

/// Refused before any request was sent.
fn not_sent(error: impl Into<VenueError>) -> VenueError {
    error.into().outcome(Outcome::NotSent)
}
