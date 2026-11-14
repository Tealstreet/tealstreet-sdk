//! Derivatives commands over drb. A label is correlation, never a resend grant.
//! Wire contracts: Coinbase technical migration guide and OpenAPI 2.1.1.
use crate::{Coinbase, Pending, drb};
use safe_cex_engine::{
    Command, Cx, ErrorKind, Outcome, Response, TimerId, VenueError,
    classify::{self, Native, NativeCode, Verdict, WriteFact},
    model::*,
    request_context::{Context, Operation, Owner, Policy, ReconciliationData, RetryPolicy, Scope},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet, VecDeque};

const MAX_JOBS: usize = 64;
const MAX_LABELS: usize = 4096;

/// Closed command vocabulary; each wire name is classified by S0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Method {
    Buy,
    Sell,
    Edit,
    Cancel,
    CancelInstrument,
    CancelAll,
    SetLeverage,
    GetLeverage,
}
impl Method {
    fn name(self) -> &'static str {
        match self {
            Self::Buy => "private/buy",
            Self::Sell => "private/sell",
            Self::Edit => "private/edit",
            Self::Cancel => "private/cancel",
            Self::CancelInstrument => "private/cancel_all_by_instrument",
            Self::CancelAll => "private/cancel_all",
            Self::SetLeverage => "private/set_leverage",
            Self::GetLeverage => "private/get_leverage",
        }
    }
    /// Authority comes only from S0's closed classifier. An unclassified name
    /// stays Mutation here and is refused by the transport before release.
    fn operation(self) -> Operation {
        match drb::method_intent(self.name()) {
            Ok(classify::Intent::Read) => Operation::Read,
            _ => Operation::Mutation,
        }
    }
}

#[derive(Clone)]
struct Call {
    method: Method,
    params: Value,
    symbol: String,
    instrument: String,
    label: Option<String>,
    id: Option<String>,
}
impl Call {
    fn key(&self) -> String {
        match (&self.label, &self.id) {
            (Some(label), _) => format!("label:{label}"),
            (_, Some(id)) => format!("order:{id}"),
            _ => format!("method:{}:{}", self.method.name(), self.instrument),
        }
    }
}
struct Job {
    root: Context,
    calls: VecDeque<Call>,
    ids: Vec<String>,
    collect: bool,
    data: Option<String>,
}
struct Recovery {
    call: Call,
    owner: Owner,
    uncertain: bool,
}
pub struct Request {
    context: Context,
    kind: RequestKind,
}
enum RequestKind {
    Command { job: Box<Job>, call: Call },
    Recovery { key: String, call: Call, epoch: u64 },
}

#[derive(Default)]
pub(crate) struct Trading {
    expiry_timer: Option<TimerId>,
    recovery_timer: Option<TimerId>,
    waiting_timer: Option<TimerId>,
    waiting: VecDeque<Job>,
    rpc: BTreeMap<u64, (drb::RpcId, f64)>,
    used_labels: HashSet<String>,
    /// Retained until native read-back proves the result; never a resend queue.
    recovery: BTreeMap<String, Recovery>,
    recovering: HashSet<String>,
}
fn policy(operation: Operation) -> Policy {
    Policy {
        operation,
        credential: None,
        timeout_ms: drb::REQUEST_TIMEOUT_MS,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
fn invalid(message: impl Into<String>) -> VenueError {
    VenueError::new(message)
        .kind(ErrorKind::InvalidInput)
        .outcome(Outcome::NotSent)
}
fn uncertain(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}
fn valid_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}
fn positive(n: f64) -> Result<f64, VenueError> {
    if n.is_finite() && n > 0. {
        Ok(n)
    } else {
        Err(invalid("drb price and size must be finite and positive"))
    }
}
fn trigger(t: Option<StopTriggerType>) -> &'static str {
    match t.unwrap_or(StopTriggerType::LastPrice) {
        StopTriggerType::MarkPrice => "mark_price",
        StopTriggerType::IndexPrice => "index_price",
        StopTriggerType::LastPrice => "last_price",
    }
}
fn derivatives(m: &Market) -> bool {
    m.is_spot != Some(true)
        && m.is_expiring != Some(true)
        && (m.id.ends_with("_USDC-PERPETUAL") || m.id.ends_with("-PERP-INTX"))
}

impl Coinbase {
    fn drb_trading_market(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
    ) -> Result<(String, Market), VenueError> {
        let m = cx
            .store
            .market(symbol)
            .filter(|m| derivatives(m))
            .ok_or_else(|| invalid("Unknown drb perpetual market"))?;
        // New discovery carries the exact name. The sole legacy alias is S0's mapping.
        let instrument = if m.id.ends_with("_USDC-PERPETUAL") {
            m.id.clone()
        } else {
            drb::to_instrument(&m.id).map_err(invalid)?
        };
        Ok((instrument, m.clone()))
    }
    fn drb_place(&self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<Call, VenueError> {
        let (instrument, m) = self.drb_trading_market(cx, &o.symbol)?;
        self.drb_require_reconciled(&o.symbol)?;
        if !m.active || m.is_read_only == Some(true) || !m.precision.is_tradable() {
            return Err(invalid("drb market is not tradable"));
        }
        if o.max_slippage_pct.is_some()
            || o.bybit_bbo.is_some()
            || o.bybit_slippage_tolerance_ticks.is_some()
            || o.stp_mode.is_some()
            || o.implied_volatility.is_some()
        {
            return Err(invalid(
                "drb cannot express these slippage, BBO, STP or volatility controls",
            ));
        }
        let amount = positive(safe_cex_engine::num::adjust(
            positive(o.amount)?,
            m.precision.amount,
        ))?;
        if amount < m.limits.amount.min || o.amount > m.limits.amount.max {
            return Err(invalid("drb size is outside instrument limits"));
        }
        let label = match &o.client_order_id {
            Some(id) if valid_id(id) && id.len() <= 64 => id.clone(),
            Some(_) => {
                return Err(invalid(
                    "drb client order ID requires 1..64 non-whitespace bytes",
                ));
            }
            None => format!(
                "tealstreet-{}",
                safe_cex_crypto::uuid::uuid_with_entropy(|b| cx.fill_entropy(b))
                    .map_err(invalid)?
            ),
        };
        if self.trading_drb.used_labels.contains(&label) {
            return Err(invalid(
                "drb client order ID was already used; reconcile the original order instead",
            ));
        }
        let tif = o.time_in_force.unwrap_or(TimeInForce::GoodTillCancel);
        let reduce = o.reduce_only == Some(true) || o.close == Some(true);
        let mut p = json!({"instrument_name":instrument, "amount":amount, "label":label,
        "reduce_only":reduce, "post_only":tif == TimeInForce::PostOnly,
        "time_in_force":match tif {
            TimeInForce::GoodTillCancel | TimeInForce::PostOnly => "good_til_cancelled",
            TimeInForce::ImmediateOrCancel => "immediate_or_cancel", TimeInForce::FillOrKill => "fill_or_kill",
        }});
        match o.kind {
            OrderType::Limit => {
                p["type"] = json!("limit");
                p["price"] = json!(positive(safe_cex_engine::num::adjust(
                    positive(
                        o.price
                            .ok_or_else(|| invalid("drb limit price is required"))?
                    )?,
                    m.precision.price
                ))?);
                if tif == TimeInForce::PostOnly {
                    p["reject_post_only"] = json!(true);
                }
            }
            OrderType::Market => {
                if tif == TimeInForce::PostOnly {
                    return Err(invalid("drb market orders cannot be post-only"));
                }
                p["type"] = json!("market");
                if tif == TimeInForce::GoodTillCancel {
                    p["time_in_force"] = json!("immediate_or_cancel");
                }
            }
            OrderType::StopLoss | OrderType::TakeProfit => {
                if !reduce
                    || tif != TimeInForce::GoodTillCancel
                    || o.stop_loss.is_some()
                    || o.take_profit.is_some()
                {
                    return Err(invalid(
                        "drb standalone protection requires reduce-only GTC and no attached bracket",
                    ));
                }
                if o.trigger_price.is_some() && o.price.is_some() && o.trigger_price != o.price {
                    return Err(invalid(
                        "drb protective market order requires a single trigger price",
                    ));
                }
                p["type"] = json!(if o.kind == OrderType::StopLoss {
                    "stop_market"
                } else {
                    "take_market"
                });
                p["trigger_price"] = json!(positive(safe_cex_engine::num::adjust(
                    positive(
                        o.trigger_price
                            .or(o.price)
                            .ok_or_else(|| invalid("drb trigger price is required"))?
                    )?,
                    m.precision.price
                ))?);
                p["trigger"] = json!(trigger(o.trigger_type));
            }
            _ => {
                return Err(invalid(
                    "drb cannot express this order type; GTT and generic conditional orders are unsupported",
                ));
            }
        }
        if matches!(o.kind, OrderType::Market | OrderType::Limit)
            && (o.trigger_price.is_some() || o.trigger_type.is_some())
        {
            return Err(invalid(
                "drb ordinary entry cannot carry a standalone trigger",
            ));
        }
        if o.tp_trigger_type.is_some() && o.take_profit.is_none()
            || o.sl_trigger_type.is_some() && o.stop_loss.is_none()
        {
            return Err(invalid("drb bracket trigger type requires its exit price"));
        }
        let mut legs = vec![];
        for (price, kind, t) in [
            (o.stop_loss, "stop_market", o.sl_trigger_type),
            (o.take_profit, "take_market", o.tp_trigger_type),
        ] {
            if let Some(price) = price {
                if reduce {
                    return Err(invalid(
                        "drb cannot attach opening brackets to a reducing entry",
                    ));
                }
                legs.push(json!({"type":kind,"direction":if o.side == OrderSide::Buy { "sell" } else { "buy" },
                    "amount":amount,"trigger_price":positive(safe_cex_engine::num::adjust(positive(price)?,m.precision.price))?,
                    "trigger":trigger(t),"reduce_only":true,"time_in_force":"good_til_cancelled"}));
            }
        }
        if !legs.is_empty() {
            p["linked_order_type"] = json!(if legs.len() == 2 {
                "one_triggers_one_cancels_other"
            } else {
                "one_triggers_other"
            });
            p["trigger_fill_condition"] = json!("incremental");
            p["otoco_config"] = json!(legs);
        }
        Ok(Call {
            method: if o.side == OrderSide::Buy {
                Method::Buy
            } else {
                Method::Sell
            },
            params: p,
            symbol: o.symbol.clone(),
            instrument,
            label: Some(label),
            id: None,
        })
    }
    /// Intercept perpetual commands only; spot and CFM retain their existing codec.
    pub(crate) fn drb_trading_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Option<Command> {
        let is_symbol = |s: &str| {
            cx.store.market(s).is_some_and(derivatives)
                || s.ends_with("-PERP-INTX")
                || s.ends_with("_USDC-PERPETUAL")
        };
        #[cfg(test)]
        if self.account.legacy_protocol_fixture {
            return Some(command);
        }
        let applies = match &command {
            Command::PlaceOrders { orders, .. } => orders.iter().any(|o| is_symbol(&o.symbol)),
            Command::CancelSymbolOrders { symbol, .. }
            | Command::SetLeverage { symbol, .. }
            | Command::FetchLeverage { symbol, .. }
            | Command::SetMarginType { symbol, .. } => is_symbol(symbol),
            Command::SetPositionMode { symbol, .. } => {
                symbol.as_deref().is_some_and(is_symbol) || self.account.is_intx()
            }
            Command::AmendOrder { id, .. } => {
                cx.store.order(id).is_some_and(|o| is_symbol(&o.symbol)) || self.account.is_intx()
            }
            Command::CancelOrders { ids, .. } => {
                ids.iter()
                    .any(|id| cx.store.order(id).is_some_and(|o| is_symbol(&o.symbol)))
                    || self.account.is_intx()
            }
            _ => false,
        };
        if !applies {
            return Some(command);
        }
        let operation = if matches!(command, Command::FetchLeverage { .. }) {
            Operation::Read
        } else {
            Operation::Mutation
        };
        let Ok(root) = cx.admit_http_command(policy(operation)) else {
            return None;
        };
        let result = self.drb_calls(cx, command);
        match result {
            Ok((calls, collect)) => self.drb_submit(
                cx,
                Job {
                    root,
                    calls,
                    ids: vec![],
                    collect,
                    data: None,
                },
            ),
            Err(e) => {
                let _ = cx.fail_context(&root, e);
            }
        }
        None
    }
    fn drb_calls(
        &self,
        cx: &Cx<'_, Pending>,
        command: Command,
    ) -> Result<(VecDeque<Call>, bool), VenueError> {
        let mut calls = VecDeque::new();
        let mut collect = false;
        match command {
            Command::PlaceOrders { orders, .. } => {
                if orders.is_empty() || orders.len() > 100 { return Err(invalid("drb placement batch requires 1..100 orders")); }
                let mut labels = HashSet::new();
                for o in orders {
                    let call = self.drb_place(cx, &o)?;
                    if !labels.insert(call.label.clone()) { return Err(invalid("drb batch contains duplicate client IDs")); }
                    calls.push_back(call);
                }
                collect = true;
            }
            Command::CancelOrders { ids, .. } => {
                if ids.is_empty() || ids.len() > 100 { return Err(invalid("drb cancel batch requires 1..100 IDs")); }
                let mut seen = HashSet::new();
                for id in ids {
                    if !valid_id(&id) || !seen.insert(id.clone()) { return Err(invalid("drb cancel IDs must be valid and unique")); }
                    let symbol = cx.store.order(&id).map(|o|o.symbol.clone()).unwrap_or_default();
                    let instrument = if symbol.is_empty() {String::new()} else {self.drb_trading_market(cx, &symbol)?.0};
                    calls.push_back(Call { method: Method::Cancel, params: json!({"order_id":id}),
                        symbol, instrument, label: None, id: Some(id) });
                }
            }
            Command::AmendOrder { id, price, amount, .. } => {
                let order = cx.store.order(&id).ok_or_else(|| invalid("drb edit requires a reconciled order"))?;
                self.drb_require_reconciled(&order.symbol)?;
                if !valid_id(&id) || order.status != OrderStatus::Open || price.is_none() && amount.is_none() {
                    return Err(invalid("drb native edit requires an open order and price or size"));
                }
                let (instrument, m) = self.drb_trading_market(cx, &order.symbol)?;
                if !m.active || m.is_read_only == Some(true) || !m.precision.is_tradable() {return Err(invalid("drb edit market is not tradable"));}
                if order.kind != OrderType::Limit || order.stop_loss.is_some() || order.take_profit.is_some() {
                    return Err(invalid("drb edit supports ordinary limits; linked or protective order edits require native leg identity"));
                }
                let amount = positive(safe_cex_engine::num::adjust(positive(amount.unwrap_or(order.amount))?, m.precision.amount))?;
                if amount <= order.filled || amount < m.limits.amount.min || amount > m.limits.amount.max { return Err(invalid("drb edit amount is outside limits or not above filled size")); }
                let price = positive(safe_cex_engine::num::adjust(positive(price.unwrap_or(order.price))?,m.precision.price))?;
                let mut params = json!({"order_id":id,"amount":amount,"price":price,"reduce_only":order.reduce_only,
                    "post_only":order.time_in_force == Some(TimeInForce::PostOnly)});
                if order.time_in_force == Some(TimeInForce::PostOnly) { params["reject_post_only"] = json!(true); }
                calls.push_back(Call { method: Method::Edit, params, symbol: order.symbol.clone(), instrument, label: None, id: Some(id) }); collect = true;
            }
            Command::CancelSymbolOrders { symbol, .. } => {
                let (instrument, _) = self.drb_trading_market(cx, &symbol)?;
                calls.push_back(Call { method: Method::CancelInstrument, params: json!({"instrument_name":instrument,"type":"all"}), symbol, instrument, label: None, id: None });
            }
            Command::SetLeverage { symbol, leverage, side, .. } => {
                let (instrument, m) = self.drb_trading_market(cx, &symbol)?;
                if side.is_some() || positive(leverage)? > m.limits.leverage.max || leverage < m.limits.leverage.min {
                    return Err(invalid("drb leverage is instrument-wide and must be within market limits"));
                }
                calls.push_back(Call { method: Method::SetLeverage, params: json!({"instrument_name":instrument,"leverage":leverage}), symbol, instrument, label: None, id: None });
            }
            Command::FetchLeverage { symbol, .. } => {
                let (instrument, _) = self.drb_trading_market(cx, &symbol)?;
                calls.push_back(Call { method: Method::GetLeverage, params: json!({"instrument_name":instrument}), symbol, instrument, label: None, id: None });
            }
            Command::SetMarginType { .. } => return Err(VenueError::new("drb margin model changes are account-wide (cross/segregated SM/PM); a per-symbol isolated/cross toggle cannot express them safely").kind(ErrorKind::Unsupported)),
            Command::SetPositionMode { .. } => return Err(VenueError::new("drb exposes net positions and no hedged/one-way position-mode setter").kind(ErrorKind::Unsupported)),
            _ => return Err(invalid("Unsupported drb trading command")),
        }
        Ok((calls, collect))
    }
    pub(crate) fn drb_cancel_account(&mut self, cx: &mut Cx<'_, Pending>) {
        let Ok(root) = cx.admit_http_command(policy(Operation::Mutation)) else {
            return;
        };
        if self.account.kind == Some(crate::ProductKind::Cfm)
            || !cx.store.markets().iter().any(derivatives)
        {
            let _ = cx.fail_context(
                &root,
                invalid("drb account cancel requires a gateway derivatives account"),
            );
            return;
        }
        let call = Call {
            method: Method::CancelAll,
            params: json!({}),
            symbol: String::new(),
            instrument: String::new(),
            label: None,
            id: None,
        };
        self.drb_submit(
            cx,
            Job {
                root,
                calls: VecDeque::from([call]),
                ids: vec![],
                collect: false,
                data: None,
            },
        );
    }
    fn drb_submit(&mut self, cx: &mut Cx<'_, Pending>, job: Job) {
        if !self.signers.has_primary()
            || self.options.public_data_only
            || self.options.spot_only
            || self.options.testnet
            || self.trading_drb.recovery.len() >= 256
            || self.trading_drb.waiting.len() + self.trading_drb.rpc.len() >= MAX_JOBS
            || self.trading_drb.used_labels.len() + job.calls.len() > MAX_LABELS
        {
            let _ = cx.fail_context(
                &job.root,
                invalid("drb trading credentials, capacity or client-ID reservation unavailable"),
            );
            return;
        }
        // Reserve the whole batch before any I/O. Labels never become reusable on rejection or loss.
        for call in &job.calls {
            if let Some(label) = &call.label {
                self.trading_drb.used_labels.insert(label.clone());
            }
        }
        self.drb_ensure_http(cx);
        if self.drb_http_session().is_none() {
            self.trading_drb.waiting.push_back(job);
            self.drb_wait_deadline(cx);
        } else {
            self.drb_next(cx, job);
        }
    }
    pub(crate) fn drb_trading_auth_failed(&mut self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        if let Some(timer) = self.trading_drb.waiting_timer.take() {
            cx.clear_timer(timer);
        }
        for job in self.trading_drb.waiting.drain(..) {
            let _ = cx.fail_context(&job.root, error.clone());
        }
    }
    pub(crate) fn drb_trading_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(timer) = self.trading_drb.waiting_timer.take() {
            cx.clear_timer(timer);
        }
        let waiting = std::mem::take(&mut self.trading_drb.waiting);
        for job in waiting {
            self.drb_next(cx, job);
        }
        self.drb_reconcile(cx);
    }
    fn drb_next(&mut self, cx: &mut Cx<'_, Pending>, mut job: Job) {
        if cx.validate_http_context(&job.root).is_err() {
            let _ = cx.fail_context(
                &job.root,
                VenueError::new("drb command deadline elapsed").kind(ErrorKind::Timeout),
            );
            return;
        }
        self.drb_ensure_http(cx);
        if self.drb_http_session().is_none() {
            self.trading_drb.waiting.push_back(job);
            self.drb_wait_deadline(cx);
            return;
        }
        let Some(call) = job.calls.pop_front() else {
            let data = job.data.unwrap_or_else(|| {
                if job.collect {
                    json!(job.ids).to_string()
                } else {
                    "null".into()
                }
            });
            let _ = cx.done_context(&job.root, data);
            return;
        };
        // A new attempt cannot acquire the same recovery reservation. In
        // particular, its later NotSent/refusal must not erase an older Unknown.
        if call.method.operation() == Operation::Mutation
            && self.trading_drb.recovery.contains_key(&call.key())
        {
            let _ = cx.fail_context(
                &job.root,
                invalid("drb write already pending or awaiting native read-back"),
            );
            return;
        }
        let context = match cx.http_leg_context(&job.root, policy(call.method.operation())) {
            Ok(c) => c,
            Err(e) => {
                let _ = cx.fail_context(&job.root, invalid(e.to_string()));
                return;
            }
        };
        if let (Some(label), Owner::Command { id, .. }) = (&call.label, job.root.owner()) {
            cx.command_progress(
                id,
                vec![OrderIdentityReceipt {
                    symbol: call.symbol.clone(),
                    kind: OrderIdentityKind::ClientOrderId,
                    identity: label.clone(),
                }],
            );
        }
        let key = call.key();
        if call.method.operation() == Operation::Mutation {
            self.trading_drb.recovery.insert(
                key.clone(),
                Recovery {
                    call: call.clone(),
                    owner: job.root.owner(),
                    uncertain: false,
                },
            );
        }
        let root = job.root.clone();
        let pending = Pending::TradingDrb(Request {
            context: context.clone(),
            kind: RequestKind::Command {
                job: Box::new(job),
                call: call.clone(),
            },
        });
        let session = self.drb_http_session().cloned();
        match self.drb.http(
            cx,
            &context,
            call.method.name(),
            call.params.clone(),
            session.as_ref(),
            pending,
        ) {
            Ok((rpc, _)) => {
                self.trading_drb.rpc.insert(context.leg(), (rpc, cx.now));
                self.drb_expiry(cx);
            }
            Err(e) => {
                self.trading_drb.recovery.remove(&key);
                let _ = cx.finish_http_context(&context);
                let _ = cx.fail_context(&root, invalid(e));
            }
        }
    }
    fn drb_expiry(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading_drb.expiry_timer.is_none() {
            self.trading_drb.expiry_timer = Some(cx.set_timer(f64::from(drb::REQUEST_TIMEOUT_MS)));
        }
    }
    pub(crate) fn drb_trading_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.trading_drb.waiting_timer == Some(timer) {
            self.trading_drb.waiting_timer = None;
            let waiting = std::mem::take(&mut self.trading_drb.waiting);
            for job in waiting {
                if cx.now >= job.root.deadline() {
                    let _ = cx.fail_context(
                        &job.root,
                        VenueError::new("drb authentication wait exceeded the command deadline")
                            .kind(ErrorKind::Timeout),
                    );
                } else {
                    self.trading_drb.waiting.push_back(job);
                }
            }
            self.drb_wait_deadline(cx);
            return true;
        }
        if self.trading_drb.recovery_timer == Some(timer) {
            self.trading_drb.recovery_timer = None;
            self.drb_reconcile(cx);
            return true;
        }
        if self.trading_drb.expiry_timer != Some(timer) {
            return false;
        }
        self.trading_drb.expiry_timer = None;
        // Retire only S3 IDs. The public and private streams own their timers.
        let expired: Vec<_> = self
            .trading_drb
            .rpc
            .iter()
            .filter(|(_, (_, started))| cx.now - started >= f64::from(drb::REQUEST_TIMEOUT_MS))
            .map(|(leg, (rpc, _))| (*leg, *rpc))
            .collect();
        for (leg, rpc) in expired {
            self.trading_drb.rpc.remove(&leg);
            let _ = self.drb.failed(rpc, ErrorKind::Timeout);
        }
        if !self.trading_drb.rpc.is_empty() {
            self.drb_expiry(cx);
        }
        true
    }
    pub(crate) fn drb_capture(
        &self,
        request: &Request,
    ) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
        let RequestKind::Command { call, .. } = &request.kind else {
            return Err(safe_cex_engine::request_context::Error::Invalid);
        };
        // Correlation only: no outbound params, JWT, bearer or continuation.
        ReconciliationData::new(
            json!({"instrument":call.instrument,"label":call.label,"order_id":call.id}).to_string(),
        )
    }
    pub(crate) fn drb_trading_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        if let Some((id, _)) = self.trading_drb.rpc.remove(&request.context.leg()) {
            let _ = self
                .drb
                .failed(id, error.kind.unwrap_or(ErrorKind::Network));
        }
        match request.kind {
            RequestKind::Command { job, call } => {
                if error.outcome == Some(Outcome::NotSent) {
                    self.trading_drb.recovery.remove(&call.key());
                } else if let Some(recovery) = self.trading_drb.recovery.get_mut(&call.key()) {
                    recovery.uncertain = true;
                }
                let _ = cx.fail_context(&job.root, error);
            }
            RequestKind::Recovery { key, .. } => {
                self.trading_drb.recovering.remove(&key);
                let _ = cx.finish_http_context(&request.context);
                self.drb_recovery_later(cx);
            }
        }
    }
    pub(crate) fn drb_trading_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let rpc = self.trading_drb.rpc.remove(&request.context.leg());
        let result = rpc.map(|(rpc, _)| {
            self.drb
                .response_at(rpc, response.status, response.body, cx.now)
        });
        match request.kind {
            RequestKind::Command { mut job, call } => {
                if cx.validate_http_context(&request.context).is_err() {
                    return;
                }
                match result {
                    Some(Ok(value)) => match self.drb_ack(cx, &call, &value) {
                        Ok((ids, data)) => {
                            if call.method.operation() == Operation::Mutation {
                                if cx
                                    .http_write_fact(
                                        &request.context,
                                        WriteFact::Accepted {
                                            order_ids: ids.clone(),
                                        },
                                        None,
                                    )
                                    .is_err()
                                {
                                    return;
                                }
                                if cx.validate_http_write_effects(&request.context).is_err() {
                                    return;
                                }
                            } else if cx
                                .validate_read_http_delivery(&job.root, &request.context)
                                .is_err()
                            {
                                return;
                            }
                            if call.method.operation() == Operation::Mutation {
                                cx.store.invalidate_order_snapshots();
                            }
                            self.drb_trading_publish(cx, &call, &value);
                            if call.method.operation() == Operation::Mutation {
                                self.drb_poll(cx);
                            }
                            self.trading_drb.recovery.remove(&call.key());
                            job.ids.extend(ids);
                            job.data = data;
                            let _ = cx.finish_http_context(&request.context);
                            self.drb_next(cx, *job);
                        }
                        Err(e) => {
                            if let Some(recovery) = self.trading_drb.recovery.get_mut(&call.key()) {
                                recovery.uncertain = true;
                            }
                            if call.method.operation() == Operation::Mutation {
                                let _ = cx.http_write_unproven(&request.context, &e.message);
                            }
                            let _ = cx.fail_context(&job.root, e);
                        }
                    },
                    failure => {
                        let error = match failure {
                            Some(Err(f)) => {
                                let message = f.code.map_or_else(
                                    || {
                                        "drb write outcome is unproven; read back before retrying"
                                            .into()
                                    },
                                    |c| format!("drb RPC refusal/error {c}"),
                                );
                                if call.method.operation() == Operation::Mutation {
                                    let native = f.rpc_error.as_ref().map(|e| Native {
                                        code: NativeCode::Int(e.code),
                                        message: &e.message,
                                    });
                                    let _ = cx.http_write_failure(
                                        &request.context,
                                        &drb::CODES,
                                        native.as_ref(),
                                        &message,
                                        None,
                                    );
                                }
                                if matches!(f.verdict, Verdict::Refused(_)) {
                                    self.trading_drb.recovery.remove(&call.key());
                                } else if let Some(recovery) =
                                    self.trading_drb.recovery.get_mut(&call.key())
                                {
                                    recovery.uncertain = true;
                                }
                                if matches!(
                                    f.verdict,
                                    Verdict::Refused(
                                        ErrorKind::AuthExpired | ErrorKind::AuthInvalid
                                    )
                                ) {
                                    self.drb_trading_auth_failed(
                                        cx,
                                        VenueError::new(
                                            "drb shared HTTP authentication was refused",
                                        ),
                                    );
                                }
                                VenueError::new(message)
                            }
                            _ => {
                                let _ = cx.http_write_unproven(
                                    &request.context,
                                    "drb correlation is unavailable",
                                );
                                uncertain("drb correlation is unavailable")
                            }
                        };
                        let _ = cx.fail_context(&job.root, error);
                    }
                }
            }
            RequestKind::Recovery { key, call, epoch } => {
                self.trading_drb.recovering.remove(&key);
                if cx
                    .validate_background_read_http_delivery(&request.context)
                    .is_ok()
                    && let Some(Ok(value)) = result
                {
                    if self.drb_apply_recovery(cx, &call, value).is_ok() {
                        self.trading_drb.recovery.remove(&key);
                        if !self.trading_drb.recovery.values().any(|r| {
                            r.uncertain
                                && (call.symbol.is_empty()
                                    || r.call.symbol.is_empty()
                                    || r.call.symbol == call.symbol)
                        }) {
                            cx.orders_reconciled_since(
                                (!call.symbol.is_empty()).then_some(call.symbol.as_str()),
                                epoch,
                            );
                        }
                    } else {
                        self.drb_recovery_later(cx);
                    }
                } else {
                    self.drb_recovery_later(cx);
                }
                let _ = cx.finish_http_context(&request.context);
            }
        }
    }
    fn drb_ack(
        &self,
        cx: &Cx<'_, Pending>,
        call: &Call,
        value: &Value,
    ) -> Result<(Vec<String>, Option<String>), VenueError> {
        match call.method {
            Method::Buy | Method::Sell | Method::Edit | Method::Cancel => {
                let row = if call.method == Method::Cancel {
                    value
                } else {
                    value
                        .get("order")
                        .ok_or_else(|| uncertain("drb write ACK omitted order"))?
                };
                let selected = self.drb_order_context(cx, call, row)?;
                let order = decode_order(&selected, row)?;
                if matches!(call.method, Method::Buy | Method::Sell | Method::Edit) {
                    let wanted_amount = call.params["amount"].as_f64();
                    let reduced_size =
                        order.reduce_only && wanted_amount.is_some_and(|n| order.amount <= n);
                    if (!reduced_size && Some(order.amount) != wanted_amount)
                        || Some(order.reduce_only) != call.params["reduce_only"].as_bool()
                        || row["post_only"] != call.params["post_only"]
                        || call
                            .params
                            .get("type")
                            .is_some_and(|kind| row["order_type"] != *kind)
                        || call
                            .params
                            .get("price")
                            .is_some_and(|price| row["price"] != *price)
                        || call
                            .params
                            .get("time_in_force")
                            .is_some_and(|tif| row["time_in_force"] != *tif)
                        || call
                            .params
                            .get("trigger_price")
                            .is_some_and(|price| row["trigger_price"].as_f64() != price.as_f64())
                        || call
                            .params
                            .get("trigger")
                            .is_some_and(|basis| row["trigger"] != *basis)
                    {
                        return Err(uncertain(
                            "drb native order ACK differs from requested size or controls",
                        ));
                    }
                }
                if call.method == Method::Cancel && order.status != OrderStatus::Canceled {
                    return Err(uncertain("drb cancel ACK did not prove cancellation"));
                }
                Ok((vec![order.id], None))
            }
            Method::CancelAll | Method::CancelInstrument => {
                if value.as_u64().is_none() {
                    return Err(uncertain(
                        "drb bulk cancel ACK omitted the cancellation count",
                    ));
                }
                Ok((vec![], None))
            }
            Method::SetLeverage => {
                let leverage = value
                    .as_f64()
                    .filter(|v| v.is_finite() && *v > 0.)
                    .ok_or_else(|| uncertain("drb leverage ACK is invalid"))?;
                if Some(leverage) != call.params["leverage"].as_f64() {
                    return Err(uncertain(
                        "drb leverage ACK differs from requested leverage",
                    ));
                }
                Ok((vec![], None))
            }
            Method::GetLeverage => {
                if value["instrument_name"] != call.instrument {
                    return Err(uncertain("drb leverage read instrument mismatch"));
                }
                let leverage = value["leverage"]
                    .as_f64()
                    .filter(|n| n.is_finite() && *n > 0.)
                    .ok_or_else(|| uncertain("drb leverage is unavailable"))?;
                Ok((
                    vec![],
                    Some(json!({"buyLeverage":leverage,"sellLeverage":leverage}).to_string()),
                ))
            }
        }
    }
    fn drb_trading_publish(&mut self, cx: &mut Cx<'_, Pending>, call: &Call, value: &Value) {
        match call.method {
            Method::Buy | Method::Sell | Method::Edit | Method::Cancel => {
                let row = if call.method == Method::Cancel {
                    value
                } else {
                    &value["order"]
                };
                if let Ok(selected) = self.drb_order_context(cx, call, row)
                    && let Ok(mut order) = decode_order(&selected, row)
                {
                    if cx.store.market(&order.symbol).is_none() {
                        return;
                    }
                    if !order_is_newer(cx, &order) {
                        return;
                    }
                    if let Some(old) = cx.store.order(&order.id) {
                        order.stop_loss = old.stop_loss;
                        order.take_profit = old.take_profit;
                    }
                    order.stop_loss = call.params["otoco_config"]
                        .as_array()
                        .and_then(|legs| legs.iter().find(|l| l["type"] == "stop_market"))
                        .and_then(|l| l["trigger_price"].as_f64())
                        .or(order.stop_loss);
                    order.take_profit = call.params["otoco_config"]
                        .as_array()
                        .and_then(|legs| legs.iter().find(|l| l["type"] == "take_market"))
                        .and_then(|l| l["trigger_price"].as_f64())
                        .or(order.take_profit);
                    cx.store.upsert_order(order);
                }
            }
            Method::SetLeverage | Method::GetLeverage => {
                let leverage = if call.method == Method::SetLeverage {
                    value.as_f64()
                } else {
                    value["leverage"].as_f64()
                };
                if let Some(n) = leverage {
                    cx.store.update_setting(&call.symbol, |s| s.leverage = n);
                }
            }
            _ => {}
        }
    }
    pub(crate) fn drb_reconcile(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.trading_drb.recovery.values().any(|r| r.uncertain) {
            return;
        }
        self.drb_ensure_http(cx);
        if self.drb_http_session().is_none() {
            return;
        }
        let calls: Vec<_> = self
            .trading_drb
            .recovery
            .iter()
            .filter(|(key, r)| r.uncertain && !self.trading_drb.recovering.contains(*key))
            .map(|(key, r)| (key.clone(), r.call.clone()))
            .collect();
        for (key, call) in calls {
            let (method, params) = if let Some(label) = &call.label {
                (
                    "private/get_order_state_by_label",
                    json!({"currency":"USDC","label":label}),
                )
            } else if let Some(id) = &call.id {
                ("private/get_order_state", json!({"order_id":id}))
            } else {
                match call.method {
                    Method::SetLeverage => (
                        "private/get_leverage",
                        json!({"instrument_name":call.instrument}),
                    ),
                    Method::CancelInstrument => (
                        "private/get_open_orders_by_instrument",
                        json!({"instrument_name":call.instrument}),
                    ),
                    Method::CancelAll => (
                        "private/get_open_orders",
                        json!({"currency":"USDC","kind":"future"}),
                    ),
                    _ => continue,
                }
            };
            let Some(session) = self.drb_http_session().cloned() else {
                return;
            };
            let Ok(context) = cx.http_live_context(Scope::Background, policy(Operation::Read))
            else {
                continue;
            };
            let pending = Pending::TradingDrb(Request {
                context: context.clone(),
                kind: RequestKind::Recovery {
                    key: key.clone(),
                    call,
                    epoch: cx.order_read_epoch(),
                },
            });
            match self
                .drb
                .http(cx, &context, method, params, Some(&session), pending)
            {
                Ok((rpc, _)) => {
                    self.trading_drb.rpc.insert(context.leg(), (rpc, cx.now));
                    self.trading_drb.recovering.insert(key);
                    self.drb_expiry(cx);
                }
                Err(_) => {
                    let _ = cx.finish_http_context(&context);
                    self.drb_recovery_later(cx);
                }
            }
        }
    }
    pub(crate) fn drb_discard(&mut self, owner: Owner) {
        for recovery in self
            .trading_drb
            .recovery
            .values_mut()
            .filter(|r| r.owner == owner)
        {
            recovery.uncertain = true;
        }
        self.trading_drb
            .waiting
            .retain(|job| job.root.owner() != owner);
    }
    fn drb_require_reconciled(&self, symbol: &str) -> Result<(), VenueError> {
        if self
            .trading_drb
            .recovery
            .values()
            .any(|r| r.uncertain && (r.call.symbol == symbol || r.call.symbol.is_empty()))
        {
            Err(invalid(
                "drb previous write outcome is uncertain; wait for native read-back before placing or editing",
            ))
        } else {
            Ok(())
        }
    }
    fn drb_recovery_later(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading_drb.recovery_timer.is_none()
            && self.trading_drb.recovery.values().any(|r| r.uncertain)
        {
            self.trading_drb.recovery_timer = Some(cx.set_timer(5_000.));
        }
    }
    fn drb_wait_deadline(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(old) = self.trading_drb.waiting_timer.take() {
            cx.clear_timer(old);
        }
        let deadline = self
            .trading_drb
            .waiting
            .iter()
            .map(|j| j.root.deadline())
            .min_by(f64::total_cmp);
        if let Some(at) = deadline {
            self.trading_drb.waiting_timer = Some(cx.set_timer((at - cx.now).max(0.)));
        }
    }
    fn drb_apply_recovery(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        call: &Call,
        value: Value,
    ) -> Result<(), VenueError> {
        if call.method == Method::SetLeverage {
            if value["instrument_name"] != call.instrument {
                return Err(uncertain("drb leverage read instrument mismatch"));
            }
            let leverage = value["leverage"]
                .as_f64()
                .or_else(|| value["max_leverage"].as_f64())
                .filter(|v| v.is_finite() && *v > 0.)
                .ok_or_else(|| uncertain("drb invalid leverage read-back"))?;
            cx.store
                .update_setting(&call.symbol, |s| s.leverage = leverage);
        } else if matches!(call.method, Method::CancelAll | Method::CancelInstrument) {
            let native = value
                .as_array()
                .filter(|rows| rows.len() <= 10_000)
                .ok_or_else(|| uncertain("drb invalid open-orders read-back"))?;
            let mut orders = vec![];
            let mut ids = HashSet::new();
            for row in native {
                let mut selected = call.clone();
                if call.method == Method::CancelAll {
                    let instrument = row["instrument_name"]
                        .as_str()
                        .ok_or_else(|| uncertain("drb open order omitted instrument"))?;
                    let m = cx
                        .store
                        .markets()
                        .iter()
                        .find(|m| m.id == instrument && derivatives(m))
                        .ok_or_else(|| uncertain("drb open order instrument is not discovered"))?;
                    selected.instrument = instrument.into();
                    selected.symbol = m.symbol.clone();
                }
                let order = decode_order(&selected, row)?;
                if order.status != OrderStatus::Open || !ids.insert(order.id.clone()) {
                    return Err(uncertain("drb open-orders snapshot is conflicting"));
                }
                orders.push(order);
            }
            let symbols: Vec<_> = cx
                .store
                .markets()
                .iter()
                .filter(|m| derivatives(m) && (call.symbol.is_empty() || m.symbol == call.symbol))
                .map(|m| m.symbol.clone())
                .collect();
            let borrowed: Vec<_> = symbols.iter().map(String::as_str).collect();
            let unproven: Vec<_> = self
                .trading_drb
                .recovery
                .values()
                .filter(|r| r.uncertain && r.call.label.is_some())
                .map(|r| r.call.symbol.clone())
                .collect();
            let refused = cx
                .commit_orders_read(
                    safe_cex_engine::order_read_cover::OrderReadScope::Symbols(&borrowed),
                    |_| true,
                    orders,
                    &unproven,
                )
                .ok_or_else(|| uncertain("drb open-orders snapshot lost freshness"))?;
            if !refused.is_empty() || !unproven.is_empty() {
                return Err(uncertain(
                    "drb open-orders snapshot cannot resolve every uncertain write",
                ));
            }
        } else {
            let rows = value.as_array().cloned().unwrap_or_else(|| vec![value]);
            // Labels are not unique server-side; absence or duplicates grant no authority.
            if rows.len() != 1 {
                return Err(uncertain("drb label read-back is absent or ambiguous"));
            }
            let selected = self.drb_order_context(cx, call, &rows[0])?;
            let order = decode_order(&selected, &rows[0])?;
            if call.method == Method::Cancel && order.status == OrderStatus::Open {
                return Err(uncertain("drb cancellation is still unproven by read-back"));
            }
            if call.method == Method::Edit
                && order.status == OrderStatus::Open
                && (call.params["amount"].as_f64() != Some(order.amount)
                    || call.params["price"].as_f64() != Some(order.price))
            {
                return Err(uncertain("drb read-back still shows the pre-edit order"));
            }
            if matches!(call.method, Method::Buy | Method::Sell) {
                self.drb_ack(cx, call, &json!({"order":rows[0]}))?;
            }
            if order_is_newer(cx, &order) {
                cx.store.upsert_order(order);
            }
        }
        Ok(())
    }
    fn drb_order_context(
        &self,
        cx: &Cx<'_, Pending>,
        call: &Call,
        row: &Value,
    ) -> Result<Call, VenueError> {
        let mut selected = call.clone();
        if selected.instrument.is_empty() {
            let instrument = row["instrument_name"]
                .as_str()
                .ok_or_else(|| uncertain("drb cancel ACK omitted instrument"))?;
            let alias = drb::from_instrument(instrument).map_err(uncertain)?;
            selected.instrument = instrument.into();
            selected.symbol = cx
                .store
                .market_by_id(instrument)
                .map_or(alias, |m| m.symbol.clone());
        }
        Ok(selected)
    }
}

#[derive(Deserialize, Serialize)]
struct NativeOrder {
    order_id: String,
    instrument_name: String,
    #[serde(default)]
    label: String,
    direction: String,
    order_type: String,
    order_state: String,
    amount: f64,
    #[serde(default)]
    filled_amount: f64,
    price: Value,
    post_only: bool,
    reduce_only: bool,
    time_in_force: String,
    last_update_timestamp: f64,
    trigger_price: Option<f64>,
    trigger: Option<StopTriggerType>,
}
fn decode_order(call: &Call, value: &Value) -> Result<Order, VenueError> {
    let n: NativeOrder = serde_json::from_value(value.clone())
        .map_err(|_| uncertain("drb malformed native order ACK"))?;
    if !valid_id(&n.order_id)
        || n.instrument_name != call.instrument
        || call.id.as_ref().is_some_and(|id| id != &n.order_id)
        || call.label.as_ref().is_some_and(|l| l != &n.label)
        || !n.amount.is_finite()
        || n.amount <= 0.
        || !n.filled_amount.is_finite()
        || n.filled_amount < 0.
        || n.filled_amount > n.amount
        || !n.last_update_timestamp.is_finite()
        || n.last_update_timestamp < 0.
    {
        return Err(uncertain(
            "drb native order ACK identity or quantities are unproven",
        ));
    }
    let side = match n.direction.as_str() {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        _ => return Err(uncertain("drb unknown order direction")),
    };
    if call.method == Method::Buy && side != OrderSide::Buy
        || call.method == Method::Sell && side != OrderSide::Sell
    {
        return Err(uncertain("drb native order ACK side mismatch"));
    }
    let kind = match n.order_type.as_str() {
        "limit" | "market_limit" => OrderType::Limit,
        "market" => OrderType::Market,
        "stop_market" | "stop_limit" => OrderType::StopLoss,
        "take_market" | "take_limit" => OrderType::TakeProfit,
        "trailing_stop" => OrderType::TrailingStopLoss,
        _ => {
            return Err(uncertain(
                "drb native order type is not supported by this codec",
            ));
        }
    };
    let status = match n.order_state.as_str() {
        "open" | "untriggered" => OrderStatus::Open,
        "filled" if n.filled_amount == n.amount => OrderStatus::Closed,
        "cancelled" => OrderStatus::Canceled,
        _ => return Err(uncertain("drb native order state is unproven")),
    };
    let price = if let Some(price) = n.price.as_f64() {
        positive(price).map_err(|_| uncertain("drb invalid native order price"))?
    } else if n.price == "market_price" {
        n.trigger_price.unwrap_or(0.)
    } else {
        return Err(uncertain("drb invalid native order price"));
    };
    let tif = match n.time_in_force.as_str() {
        "good_til_cancelled" => {
            if n.post_only {
                TimeInForce::PostOnly
            } else {
                TimeInForce::GoodTillCancel
            }
        }
        "immediate_or_cancel" => TimeInForce::ImmediateOrCancel,
        "fill_or_kill" => TimeInForce::FillOrKill,
        _ => return Err(uncertain("drb unknown native time-in-force")),
    };
    Ok(Order {
        id: n.order_id,
        parent_id: None,
        client_order_id: Some(n.label),
        symbol: call.symbol.clone(),
        kind,
        side,
        status,
        price,
        amount: n.amount,
        filled: n.filled_amount,
        remaining: (n.amount - n.filled_amount).max(0.),
        reduce_only: n.reduce_only,
        close: None,
        time_in_force: Some(tif),
        last_update: n.last_update_timestamp,
        trigger_type: n.trigger,
        take_profit: None,
        stop_loss: None,
        extra: Some(OrderExtra {
            remaining_native: Some(n.amount - n.filled_amount),
            filled_in_placement_units: Some(n.filled_amount),
            ..Default::default()
        }),
    })
}

fn order_is_newer(cx: &Cx<'_, Pending>, order: &Order) -> bool {
    cx.store.order(&order.id).is_none_or(|old| {
        old.last_update <= order.last_update
            && old.filled <= order.filled
            && !(matches!(old.status, OrderStatus::Closed | OrderStatus::Canceled)
                && order.status == OrderStatus::Open)
    })
}

#[cfg(test)]
#[path = "../tests/support/trading_drb.rs"]
mod tests;
