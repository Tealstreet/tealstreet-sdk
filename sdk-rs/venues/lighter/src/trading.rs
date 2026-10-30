//! Serialized nonce allocation and submissions. Transport failures never replay writes.
use crate::{Lighter, Pending, sign};
use safe_cex_engine::classify::{CodeTable, Key, Native, NativeCode, Row, Verdict, WriteFact};
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::plan::{CallAuth, SerialCall, SerialPolicy, Slot};
use safe_cex_engine::request_context::{
    Context, Operation, Plan, Policy, Preflight, PreflightPolicy, PreflightPurpose, RetryPolicy,
};
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, TimerId, VenueError, num,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};

const TIER_MESSAGE: &str = "Lighter requires Plus or Premium account tier for Tealstreet partner-attributed orders with non-zero integrator fees. Switch this account to Plus or Premium, then retry.";
const APPROVAL_MESSAGE: &str = "Tealstreet partner attribution is not yet approved for this Lighter account. Open Account Settings, edit this Lighter account, then click \"Approve Builder Fee\" and retry.";
const POSITION_READ_TIMEOUT_MS: f64 = 10_000.0;
// Provisional: no recorded Lighter private tape measures the ACK-to-stream gap,
// so the group proof bound matches POSITION_READ_TIMEOUT_MS; refine at live smoke.
const GROUP_PROOF_BOUND_MS: u32 = 10_000;
const GROUP_PENDING_MESSAGE: &str = "is awaiting confirmation of its grouped placement";
const RATE_MESSAGE: &str = "Lighter rate limit is active; wait before retrying. Limits can apply to IP, account or transaction quota.";
const MAX_CLIENT_INDEX: u64 = (1 << 48) - 1;

#[derive(Default)]
pub struct Trading {
    queue: VecDeque<Job>,
    active: Option<Job>,
    inflight: bool,
    waiting_limits: bool,
    /// Bounded by the signer's safe-integer nonce range.
    next_nonce: safe_cex_engine::nonce::VenueCounter<{ (1 << 53) - 1 }>,
    /// Captured with the in-flight nextNonce request.
    nonce_ticket: safe_cex_engine::nonce::SeedTicket,
    next_id: u64,
    timer: Option<TimerId>,
    position_wait: Option<PositionWait>,
    retired_snapshot: Option<u64>,
    retired_effects: Vec<Effect>,
    uncertain_settings: HashMap<String, u64>,
}
struct PositionWait {
    command: CommandId,
    serial: u64,
    confirmed: Option<bool>,
}
struct Job {
    preflight: Preflight,
    plan: Option<Plan>,
    slot: Option<Slot>,
    live: Option<Context>,
    delay_ms: u32,
    waiting_plan: bool,
    condition: Option<safe_cex_engine::request_context::ConditionWait>,
    effect: Option<Effect>,
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    builder_stage: u8,
    retried_nonce: bool,
    amend: Option<Order>,
    amend_filled: bool,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
    /// Every grouped-order leg this job prepared; held against cancel/amend.
    group_legs: Vec<String>,
    /// Legs of acknowledged groups that the stream or a snapshot must prove.
    group_accepted: Vec<String>,
    group_seen: HashSet<String>,
    /// The plan is parked in a settle-only wait for group proof.
    settled: bool,
}
impl Job {
    /// Every acknowledged group leg has been seen by its own id.
    fn group_proven(&self) -> bool {
        self.group_accepted
            .iter()
            .all(|id| self.group_seen.contains(id))
    }
}
enum Step {
    Position {
        symbol: String,
        side: OrderSide,
        requested: bool,
    },
    Place(Vec<Prepared>),
    /// One create_grouped_orders transaction: parent, then SL, then TP.
    Group {
        grouping: u8,
        orders: Vec<Prepared>,
    },
    Cancel(Box<Order>),
    Setting {
        symbol: String,
        leverage: Option<f64>,
        isolated: Option<bool>,
    },
}
struct Prepared {
    native: sign::Order,
    order: Order,
}
pub enum Request {
    Approval,
    Nonce,
    Submit(Submission),
}
pub struct Submission {
    effect: Effect,
    count: usize,
    /// A grouped placement is never resent, even on a nonce refusal.
    grouped: bool,
}
#[derive(Clone)]
enum Effect {
    Place,
    Cancel,
    Setting {
        symbol: String,
        leverage: f64,
        isolated: bool,
    },
}
#[derive(Deserialize, Default)]
struct Reply {
    #[serde(default)]
    code: Option<Number>,
    #[serde(default)]
    message: String,
    #[serde(default)]
    nonce: Option<Number>,
    #[serde(default)]
    tx_hash: Option<Hashes>,
    #[serde(default, rename = "txHash")]
    tx_hash_camel: Option<Hashes>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Hashes {
    One(String),
    Many(Vec<String>),
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Number {
    Text(String),
    Integer(u64),
}
impl Number {
    fn integer(&self) -> Option<u64> {
        match self {
            Self::Text(s) => s.parse().ok(),
            Self::Integer(n) => Some(*n),
        }
    }
}
#[derive(Deserialize, Default)]
struct ApprovalReply {
    #[serde(default)]
    code: Option<Number>,
    #[serde(default)]
    accounts: Vec<ApprovalAccount>,
    #[serde(default)]
    account: Option<ApprovalAccount>,
}
#[derive(Deserialize, Default)]
struct ApprovalAccount {
    #[serde(default)]
    approved_integrators: Vec<Integrator>,
}
#[derive(Deserialize)]
struct Integrator {
    #[serde(alias = "accountIndex")]
    account_index: Number,
    #[serde(alias = "approvalExpiry")]
    approval_expiry: Number,
    #[serde(default, alias = "maxPerpsMakerFee")]
    max_perps_maker_fee: Option<Number>,
    #[serde(default, alias = "maxPerpsTakerFee")]
    max_perps_taker_fee: Option<Number>,
}

impl Lighter {
    /// An authoritative stream row. `client_index` is the row's own client order
    /// index when present; only it can prove a grouped leg.
    pub(crate) fn trade_order_observed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: &str,
        client_index: Option<&str>,
        filled: f64,
        status: OrderStatus,
    ) {
        for job in self
            .trading
            .active
            .iter_mut()
            .chain(self.trading.queue.iter_mut())
        {
            if job.amend.as_ref().is_some_and(|old| {
                old.id == id && (filled > old.filled || status == OrderStatus::Closed)
            }) {
                job.amend_filled = true;
            }
        }
        if let Some(client_index) = client_index {
            self.trade_group_observed(cx, client_index);
        }
    }

    /// Group legs are proven only in their own namespace: a row's client order
    /// index. Native order indexes can coincide with our minted indexes.
    pub(crate) fn trade_group_observed(&mut self, cx: &mut Cx<'_, Pending>, client_index: &str) {
        for job in self
            .trading
            .active
            .iter_mut()
            .chain(self.trading.queue.iter_mut())
        {
            if job.group_legs.iter().any(|own| own == client_index) {
                job.group_seen.insert(client_index.to_string());
            }
        }
        if let Some(job) = self.trading.active.as_ref()
            && job.settled
            && job.group_proven()
            && let Some(wait) = &job.condition
        {
            let _ = cx.release_condition_wait(wait);
        }
    }

    /// The private stream is the only proof source; losing it while a group is
    /// settling ends the wait now, and the ready handler reports Unknown.
    pub(crate) fn trade_stream_lost(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(job) = self.trading.active.as_ref()
            && job.settled
            && let Some(wait) = &job.condition
        {
            let _ = cx.release_condition_wait(wait);
        }
    }

    /// After every step: park in a settle-only wait until the stream proves
    /// each acknowledged leg, or fail Unknown with every id.
    fn trade_settle_group(&mut self, cx: &mut Cx<'_, Pending>) {
        // Frame-fed: no private stream, and ingested orders carry native ids,
        // not the client order indexes that prove legs. Unknown now, never resent.
        if cx.frame_fed() {
            self.trade_group_unconfirmed(cx);
            return;
        }
        let job = self.trading.active.as_mut().unwrap();
        let (Some(plan), Some(slot), Some(context)) =
            (job.plan.clone(), job.slot.take(), job.live.take())
        else {
            self.trade_group_unconfirmed(cx);
            return;
        };
        match cx.serial_plan_settle_wait(&plan, &slot, &context, GROUP_PROOF_BOUND_MS) {
            Ok(wait) => {
                job.condition = Some(wait);
                job.waiting_plan = true;
                job.settled = true;
            }
            Err(_) => {
                job.slot = Some(slot);
                job.live = Some(context);
                self.trade_group_unconfirmed(cx);
            }
        }
    }

    /// Unknown with every accepted id kept and the plan's uncertainty inherited;
    /// an orders refresh follows so the next complete snapshot reconciles. No replay.
    fn trade_group_unconfirmed(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(plan) = self
            .trading
            .active
            .as_ref()
            .and_then(|job| job.plan.clone())
        {
            let _ = cx.inherit_plan_uncertainty(&plan);
        }
        self.trading.next_nonce.invalidate();
        self.refresh_orders(cx);
        let message = if cx.frame_fed() {
            "Lighter accepted the grouped order but this session cannot confirm its legs; check open orders before retrying"
        } else {
            "Lighter accepted the grouped order but the account stream did not confirm every leg; check open orders before retrying"
        };
        self.trade_finish(cx, uncertain(message));
    }

    pub(crate) fn trade_execution_observed(&mut self, id: &str, timestamp: f64) {
        for job in self
            .trading
            .active
            .iter_mut()
            .chain(self.trading.queue.iter_mut())
        {
            if job
                .amend
                .as_ref()
                .is_some_and(|old| old.id == id && timestamp >= old.last_update)
            {
                job.amend_filled = true;
            }
        }
    }

    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, cmd: Command) {
        let mut amend = None;
        let (command, steps) = match cmd {
            Command::PlaceOrders { command, orders } => {
                // This venue cannot honor a caller ID, including on a later batch item.
                if orders.iter().any(|o| o.client_order_id.is_some()) {
                    refuse(
                        cx,
                        not_sent("Lighter does not support caller client order IDs"),
                    );
                    return;
                }
                let steps = match self.prepare_orders(cx, orders) {
                    Ok(steps) => steps,
                    Err(error) => {
                        refuse(cx, not_sent(error));
                        return;
                    }
                };
                (command, steps)
            }
            Command::CancelOrders { command, ids } => {
                let mut steps = VecDeque::new();
                for id in ids {
                    if self.group_pending(&id) {
                        refuse(
                            cx,
                            not_sent(format!("Lighter order {id} {GROUP_PENDING_MESSAGE}")),
                        );
                        return;
                    }
                    if id.parse::<u64>().is_err() {
                        refuse(
                            cx,
                            not_sent("Cannot cancel Lighter order: expected a numeric order index"),
                        );
                        return;
                    }
                    let Some(order) = cx.store.order(&id).cloned() else {
                        refuse(cx, not_sent(format!("Lighter order {id} not found")));
                        return;
                    };
                    if let Err(error) = market_id(cx, &order.symbol) {
                        refuse(cx, not_sent(error));
                        return;
                    }
                    steps.push_back(Step::Cancel(Box::new(order)));
                }
                (command, steps)
            }
            Command::CancelSymbolOrders { command, symbol } => {
                let orders: Vec<Order> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        o.symbol == symbol
                            && matches!(o.status, OrderStatus::Open | OrderStatus::Creating)
                    })
                    .cloned()
                    .collect();
                if let Some(order) = orders.iter().find(|o| self.group_pending(&o.id)) {
                    refuse(
                        cx,
                        not_sent(format!(
                            "Lighter order {} {GROUP_PENDING_MESSAGE}",
                            order.id
                        )),
                    );
                    return;
                }
                (
                    command,
                    orders
                        .into_iter()
                        .map(|order| Step::Cancel(Box::new(order)))
                        .collect(),
                )
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => {
                if self
                    .trading
                    .active
                    .iter()
                    .chain(self.trading.queue.iter())
                    .any(|j| j.amend.as_ref().is_some_and(|o| o.id == id))
                {
                    refuse(
                        cx,
                        not_sent("Lighter order already has an amendment pending"),
                    );
                    return;
                }
                if self.group_pending(&id) {
                    refuse(
                        cx,
                        not_sent(format!("Lighter order {id} {GROUP_PENDING_MESSAGE}")),
                    );
                    return;
                }
                let Some(old) = cx.store.order(&id).cloned() else {
                    refuse(cx, not_sent("Lighter order not found"));
                    return;
                };
                if !matches!(old.status, OrderStatus::Open | OrderStatus::Creating) {
                    refuse(cx, not_sent("Lighter order is not open for amendment"));
                    return;
                }
                let kind = protective_kind(cx, &old, price.unwrap_or(old.price));
                let opts = PlaceOrderOpts {
                    symbol: old.symbol.clone(),
                    kind,
                    side: old.side,
                    amount: amount.unwrap_or(old.remaining),
                    price: Some(price.unwrap_or(old.price)),
                    trigger_price: None,
                    stop_loss: None,
                    take_profit: None,
                    reduce_only: Some(old.reduce_only),
                    time_in_force: old.time_in_force,
                    trigger_type: old.trigger_type,
                    tp_trigger_type: None,
                    sl_trigger_type: None,
                    client_order_id: None,
                    close: old.close,
                    max_slippage_pct: None,
                    stp_mode: None,
                    bybit_bbo: None,
                    bybit_slippage_tolerance_ticks: None,
                    implied_volatility: None,
                };
                let mut steps = match self.prepare_orders(cx, vec![opts]) {
                    Ok(steps) => steps,
                    Err(error) => {
                        refuse(cx, not_sent(error));
                        return;
                    }
                };
                steps.push_front(Step::Cancel(Box::new(old.clone())));
                amend = Some(old);
                (command, steps)
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => (
                command,
                VecDeque::from([Step::Setting {
                    symbol,
                    leverage: Some(leverage),
                    isolated: None,
                }]),
            ),
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => (
                command,
                VecDeque::from([Step::Setting {
                    symbol,
                    leverage: None,
                    isolated: Some(isolated),
                }]),
            ),
            Command::SetPositionMode {
                command: _,
                hedged,
                symbol: _,
            } => {
                if hedged {
                    refuse(
                        cx,
                        not_sent("Lighter supports one-way position mode only")
                            .kind(ErrorKind::Unsupported),
                    );
                } else {
                    if let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
                        purpose: PreflightPurpose::WriteAfterReads,
                        read_credential: None,
                        write_credential: self
                            .account
                            .identity
                            .as_ref()
                            .map(crate::primary_credential),
                    }) {
                        cx.store.set_hedged(false);
                        let _ = cx.done_empty_plan(&preflight, "null".into());
                    }
                }
                return;
            }
            _ => return,
        };
        let group_legs: Vec<String> = steps
            .iter()
            .filter_map(|step| match step {
                Step::Group { orders, .. } => Some(orders.iter().map(|p| p.order.id.clone())),
                _ => None,
            })
            .flatten()
            .collect();
        let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: None,
            write_credential: self
                .account
                .identity
                .as_ref()
                .map(crate::primary_credential),
        }) else {
            return;
        };
        let queued = self.trading.active.is_some() || !self.trading.queue.is_empty();
        if queued && cx.park_preflight_for(&preflight, None).is_err() {
            let _ = cx.fail_preflight(&preflight, "Lighter FIFO admission failed");
            return;
        }
        self.trading.queue.push_back(Job {
            preflight,
            plan: None,
            slot: None,
            live: None,
            delay_ms: 0,
            waiting_plan: false,
            condition: None,
            effect: None,
            command,
            steps,
            ids: vec![],
            builder_stage: 0,
            retried_nonce: false,
            amend,
            amend_filled: false,
            effective: false,
            group_legs,
            group_accepted: Vec::new(),
            group_seen: HashSet::new(),
            settled: false,
        });
        if !queued {
            self.trade_kick(cx);
        }
    }

    /// A leg of a grouped placement whose job has not finished.
    fn group_pending(&self, id: &str) -> bool {
        self.trading
            .active
            .iter()
            .chain(self.trading.queue.iter())
            .any(|job| job.group_legs.iter().any(|leg| leg == id))
    }

    fn builder(&self) -> Option<sign::Builder> {
        self.options
            .integrator_account_index
            .or(if self.options.robinhood {
                Some(47_296)
            } else if self.options.testnet {
                None
            } else {
                Some(158_385)
            })
            .map(|account_index| sign::Builder {
                account_index,
                maker_fee: 0,
                taker_fee: 250,
            })
    }

    pub(crate) fn trade_kick(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.inflight
            || self.trading.timer.is_some()
            || self.trading.position_wait.is_some()
        {
            return;
        }
        if self.trading.active.is_none() {
            self.trading.active = self.trading.queue.pop_front();
        }
        let Some(job) = self.trading.active.as_ref() else {
            return;
        };
        if job.waiting_plan {
            return;
        }
        if job.steps.is_empty() {
            if !job.group_proven() {
                self.trade_settle_group(cx);
                return;
            }
            let job = self.trading.active.take().unwrap();
            if let Some(plan) = job.plan {
                if let (Some(slot), Some(context)) = (&job.slot, &job.live) {
                    let _ = cx.finish_plan_call(&plan, slot, context);
                }
                let _ = cx.done_plan(&plan, serde_json::to_string(&job.ids).unwrap());
            } else {
                let _ =
                    cx.done_empty_plan(&job.preflight, serde_json::to_string(&job.ids).unwrap());
            }
            self.trade_wake_next(cx);
            return;
        }
        if self.account.identity.is_none() {
            self.trade_finish(
                cx,
                not_sent("Lighter signer unavailable; execution is disabled"),
            );
            return;
        }
        if let Some(Step::Position {
            symbol,
            side,
            requested,
        }) = job.steps.front()
        {
            let present = has_position(cx, symbol, *side);
            if present {
                self.trading.active.as_mut().unwrap().steps.pop_front();
                self.trade_kick(cx);
            } else if *requested || cx.frame_fed() {
                // Frame-fed positions are the data source's; there is no read to wait for.
                self.trade_finish(
                    cx,
                    not_sent("No open position available for this Lighter reduce-only limit order"),
                );
            } else {
                if let Some(Step::Position { requested, .. }) =
                    self.trading.active.as_mut().unwrap().steps.front_mut()
                {
                    *requested = true;
                }
                let command = self.trading.active.as_ref().unwrap().command;
                self.refresh_account(cx);
                let Some(serial) = self.account_snapshot_serial() else {
                    self.trade_finish(
                        cx,
                        not_sent(
                            "Lighter account read unavailable; reduce-only order was not sent",
                        ),
                    );
                    return;
                };
                if self.trading.retired_snapshot == Some(serial) {
                    self.trade_finish(cx, not_sent("Previous Lighter account read timed out; wait for a fresh snapshot before placing this reduce-only order"));
                    return;
                }
                self.trading.position_wait = Some(PositionWait {
                    command,
                    serial,
                    confirmed: None,
                });
                let preflight = self.trading.active.as_ref().unwrap().preflight.clone();
                let job = self.trading.active.as_mut().unwrap();
                let result = if let Some(plan) = &job.plan {
                    match (job.slot.take(), job.live.take()) {
                        (Some(slot), Some(context)) => cx
                            .serial_plan_condition_wait(
                                plan,
                                &slot,
                                &context,
                                serial_call(Operation::Mutation),
                                Some(POSITION_READ_TIMEOUT_MS as u32),
                            )
                            .map(|wait| {
                                job.condition = Some(wait);
                                job.waiting_plan = true;
                            }),
                        _ => Err(safe_cex_engine::request_context::Error::State),
                    }
                } else {
                    cx.park_preflight_for(&preflight, Some(POSITION_READ_TIMEOUT_MS as u32))
                };
                if let Err(error) = result {
                    self.trade_finish(cx, error.venue_error());
                }
            }
            return;
        }
        if let Some(old) = job.amend.as_ref()
            && job.ids.is_empty()
            && matches!(job.steps.front(), Some(Step::Place(_)))
            && (job.amend_filled
                || cx
                    .store
                    .order(&old.id)
                    .is_some_and(|o| o.filled > old.filled || o.status == OrderStatus::Closed))
        {
            self.trade_finish(
                cx,
                not_sent(
                    "Lighter source order filled during amendment; replacement was not placed",
                ),
            );
            return;
        }
        if self.account.account_index.is_none() || self.account.api_key_index.is_none() {
            self.trade_finish(cx, not_sent("Lighter account is not loaded yet"));
            return;
        }
        if self.builder().is_some()
            && job
                .steps
                .iter()
                .any(|s| matches!(s, Step::Place(_) | Step::Group { .. }))
            && job.builder_stage < 2
        {
            if job.builder_stage == 0 {
                if !self.refresh_limits(cx) {
                    self.trading.waiting_limits = true;
                    let preflight = self.trading.active.as_ref().unwrap().preflight.clone();
                    if let Err(error) = cx.park_preflight_for(&preflight, None) {
                        self.trade_finish(cx, error.venue_error());
                    }
                    return;
                }
                if self.limits_standard() {
                    self.trade_finish(cx, not_sent(TIER_MESSAGE));
                    return;
                }
                // Frame-fed: no approval read; the venue refuses an unapproved
                // integrator (21149), mapped to APPROVAL_MESSAGE.
                self.trading.active.as_mut().unwrap().builder_stage =
                    if cx.frame_fed() { 2 } else { 1 };
                self.trade_kick(cx);
                return;
            }
            let account = self.account.account_index.unwrap();
            self.trading.inflight = true;
            self.trade_read(
                cx,
                format!("{}/api/v1/account?by=index&value={account}", self.host()),
                Request::Approval,
            );
            return;
        }
        if !self.trading.next_nonce.seeded() {
            let account = self.account.account_index.unwrap();
            let key = self.account.api_key_index.unwrap();
            self.trading.inflight = true;
            self.trading.nonce_ticket = self.trading.next_nonce.ticket();
            self.trade_read(
                cx,
                format!(
                    "{}/api/v1/nextNonce?account_index={account}&api_key_index={key}",
                    self.host()
                ),
                Request::Nonce,
            );
            return;
        }
        let result = self.build_submission(cx);
        let (txs, effect) = match result {
            Ok(v) => v,
            Err(error) => {
                self.trade_finish(cx, not_sent(error));
                return;
            }
        };
        let count = txs.len();
        let intent = match &effect {
            Effect::Place => Some(safe_cex_engine::classify::Intent::Place),
            Effect::Setting { .. } => Some(safe_cex_engine::classify::Intent::Setting),
            Effect::Cancel => None,
        };
        let Some(context) = self.trade_context(cx, Operation::Mutation, intent) else {
            return;
        };
        let mut reserved = Vec::with_capacity(count);
        for tx in txs {
            let Some(nonce) = self.trading.next_nonce.take() else {
                self.trading.next_nonce.invalidate();
                self.trade_finish(cx, not_sent("Lighter transaction nonce exhausted"));
                return;
            };
            reserved.push((tx, nonce));
        }
        let recipe = (|| {
            let account = sign::Account::new(
                self.chain_id(),
                self.account
                    .account_index
                    .ok_or("Lighter account is not loaded yet")?,
                self.account
                    .api_key_index
                    .ok_or("Invalid Lighter API key index")?,
            )?;
            sign::TxRecipe::new(
                self.account
                    .identity
                    .clone()
                    .ok_or("Lighter signer unavailable")?,
                account,
                reserved,
            )
        })();
        let recipe = match recipe {
            Ok(recipe) => recipe,
            Err(error) => {
                self.trading.next_nonce.invalidate();
                self.trade_finish(cx, not_sent(error));
                return;
            }
        };
        let path = if count == 1 { "sendTx" } else { "sendTxBatch" };
        if let Effect::Setting { symbol, .. } = &effect {
            self.account_setting_write(symbol);
        }
        let job = self.trading.active.as_ref().unwrap();
        let grouped = matches!(job.steps.front(), Some(Step::Group { .. }));
        if let Some(Step::Place(orders) | Step::Group { orders, .. }) = job.steps.front() {
            cx.command_progress(
                job.command,
                orders
                    .iter()
                    .map(|prepared| OrderIdentityReceipt {
                        symbol: prepared.order.symbol.clone(),
                        kind: OrderIdentityKind::ClientOrderIndex,
                        identity: prepared.native.order_id.to_string(),
                    })
                    .collect(),
            );
        }
        self.trading.active.as_mut().unwrap().effect = Some(effect.clone());
        self.trading.inflight = true;
        if let Err((_, error)) = cx.request_signed_managed(
            context,
            None,
            Method::Post,
            format!("{}/api/v1/{path}", self.host()),
            None,
            Box::new(sign::FormRecipe(recipe)),
            Pending::Trade(Request::Submit(Submission {
                effect,
                count,
                grouped,
            })),
        ) {
            self.trading.inflight = false;
            self.trade_finish(cx, error.venue_error());
        }
    }

    fn build_submission(
        &self,
        cx: &Cx<'_, Pending>,
    ) -> Result<(Vec<sign::Transaction>, Effect), String> {
        match self.trading.active.as_ref().unwrap().steps.front().unwrap() {
            Step::Position { .. } => unreachable!(),
            Step::Place(orders) => {
                let builder = self.builder();
                Ok((
                    orders
                        .iter()
                        .map(|p| sign::Transaction::Create {
                            order: p.native.clone(),
                            builder,
                        })
                        .collect(),
                    Effect::Place,
                ))
            }
            Step::Group { grouping, orders } => Ok((
                vec![sign::Transaction::Grouped {
                    grouping: *grouping,
                    orders: orders.iter().map(|p| p.native.clone()).collect(),
                    builder: self.builder(),
                }],
                Effect::Place,
            )),
            Step::Cancel(order) => Ok((
                vec![sign::Transaction::Cancel {
                    market_id: market_id(cx, &order.symbol)?,
                    order_id: order
                        .id
                        .parse()
                        .map_err(|_| "Invalid Lighter order index")?,
                }],
                Effect::Cancel,
            )),
            Step::Setting {
                symbol,
                leverage,
                isolated,
            } => {
                if self
                    .trading
                    .uncertain_settings
                    .get(symbol)
                    .is_some_and(|epoch| self.account_setting_epoch(symbol) <= *epoch)
                {
                    return Err("Lighter account settings are awaiting reconciliation after an uncertain transaction".into());
                }
                let market = cx
                    .store
                    .market(symbol)
                    .ok_or_else(|| format!("Market {symbol} not found"))?;
                let setting = cx.store.setting(symbol);
                let previous_leverage = setting
                    .map(|s| s.leverage)
                    .filter(|n| n.is_finite() && *n > 0.0)
                    .unwrap_or(market.limits.leverage.max);
                let leverage = leverage.unwrap_or(previous_leverage);
                if !leverage.is_finite() || leverage <= 0.0 {
                    return Err("Invalid Lighter leverage".into());
                }
                let leverage = snap_leverage(num::clamp_leverage(
                    leverage,
                    market.limits.leverage.min,
                    market.limits.leverage.max,
                )?);
                let isolated = isolated.unwrap_or_else(|| {
                    setting
                        .and_then(|s| s.margin_type)
                        .map(|m| m == MarginType::Isolated)
                        .unwrap_or_else(|| {
                            self.metadata
                                .get(symbol)
                                .is_some_and(|m| m.margin_mode == 1)
                        })
                });
                Ok((
                    vec![sign::Transaction::Leverage {
                        market_id: market_id(cx, symbol)?,
                        leverage,
                        margin_mode: u8::from(isolated),
                    }],
                    Effect::Setting {
                        symbol: symbol.clone(),
                        leverage,
                        isolated,
                    },
                ))
            }
        }
    }

    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let Some(context) = self
            .trading
            .active
            .as_ref()
            .and_then(|job| job.live.clone())
        else {
            return;
        };
        let valid = if let Some(plan) = self
            .trading
            .active
            .as_ref()
            .and_then(|job| job.plan.as_ref())
        {
            cx.validate_plan_delivery(plan, &context)
        } else {
            cx.validate_preflight_delivery(
                &self.trading.active.as_ref().unwrap().preflight,
                &context,
            )
        };
        if valid.is_err() {
            return;
        }
        self.trading.inflight = false;
        if matches!(response.status, 405 | 429) && !matches!(&request, Request::Submit(_)) {
            let parsed = serde_json::from_str::<Reply>(response.body).ok();
            let mut error = not_sent(RATE_MESSAGE).kind(ErrorKind::RateLimited);
            if let Some(reply) = parsed {
                if let Some(code) = reply.code.as_ref().and_then(Number::integer) {
                    error = error.venue_code(code.to_string());
                }
                if !reply.message.is_empty() {
                    error.message = format!("{}; {RATE_MESSAGE}", reply.message);
                    error = error.venue_message(reply.message);
                }
            }
            self.trade_finish(cx, error);
            return;
        }
        match request {
            Request::Approval => {
                if response.status < 300
                    && let Ok(approval) = serde_json::from_str::<ApprovalReply>(response.body)
                    && approval.code.as_ref().and_then(Number::integer) == Some(200)
                {
                    let builder = self.builder().unwrap();
                    let valid = approval
                        .accounts
                        .iter()
                        .chain(approval.account.iter())
                        .flat_map(|a| &a.approved_integrators)
                        .any(|a| {
                            a.account_index.integer() == Some(builder.account_index)
                                && a.approval_expiry
                                    .integer()
                                    .is_some_and(|expiry| expiry as f64 > cx.now)
                                && a.max_perps_maker_fee
                                    .as_ref()
                                    .and_then(Number::integer)
                                    .unwrap_or(0)
                                    >= u64::from(builder.maker_fee)
                                && a.max_perps_taker_fee
                                    .as_ref()
                                    .and_then(Number::integer)
                                    .is_some_and(|fee| fee >= u64::from(builder.taker_fee))
                        });
                    if !valid {
                        self.trade_finish(cx, not_sent(APPROVAL_MESSAGE));
                        return;
                    }
                }
                self.trading.active.as_mut().unwrap().builder_stage = 2;
                self.trade_read_finished(cx, &context);
                self.trade_kick(cx);
            }
            Request::Nonce => {
                let ticket = self.trading.nonce_ticket;
                let reply = serde_json::from_str::<Reply>(response.body);
                match reply {
                    Ok(reply)
                        if response.status < 300
                            && reply.code.as_ref().and_then(Number::integer) == Some(200) =>
                    {
                        let seeded = reply
                            .nonce
                            .as_ref()
                            .and_then(Number::integer)
                            .is_some_and(|n| self.trading.next_nonce.seed(ticket, n));
                        if !seeded {
                            self.trade_finish(
                                cx,
                                not_sent("Lighter returned an invalid transaction nonce"),
                            );
                        } else {
                            self.trade_read_finished(cx, &context);
                            self.trade_kick(cx);
                        }
                    }
                    _ => {
                        self.trade_finish(cx, not_sent("Failed to fetch Lighter transaction nonce"))
                    }
                }
            }
            Request::Submit(submission) => {
                let parsed = serde_json::from_str::<Reply>(response.body);
                let reply = match parsed {
                    Ok(reply) => reply,
                    Err(_) => {
                        let _ = cx.http_write_unproven(&context, UNCERTAIN_RESULT);
                        self.submission_uncertain(cx, &submission.effect);
                        let error = if matches!(response.status, 405 | 429) {
                            uncertain(format!("{UNCERTAIN_RESULT}; {RATE_MESSAGE}"))
                                .kind(ErrorKind::RateLimited)
                        } else {
                            uncertain(UNCERTAIN_RESULT)
                        };
                        self.trade_finish(cx, error);
                        return;
                    }
                };
                let code = reply.code.as_ref().and_then(Number::integer);
                if submission.count == 1
                    && !submission.grouped
                    && code == Some(21104)
                    && response.status < 500
                    && !matches!(response.status, 405 | 429)
                    && !self.trading.active.as_ref().unwrap().retried_nonce
                {
                    self.trading.active.as_mut().unwrap().retried_nonce = true;
                    self.trading.next_nonce.invalidate();
                    if !self.trade_refusal(cx, &context, code.unwrap(), &reply.message) {
                        return;
                    }
                    self.trading.active.as_mut().unwrap().delay_ms = 150;
                    self.trade_kick(cx);
                    return;
                }
                let hashes = reply.tx_hash.as_ref().or(reply.tx_hash_camel.as_ref());
                let hash_count = match hashes {
                    Some(Hashes::One(s)) if !s.is_empty() => 1,
                    Some(Hashes::Many(v)) if v.iter().all(|s| !s.is_empty()) => v.len(),
                    _ => 0,
                };
                if response.status >= 300 || code != Some(200) || hash_count != submission.count {
                    self.trading.next_nonce.invalidate();
                    // The sequencer may still accept individual transactions of a refused batch.
                    let unknown = response.status >= 500
                        || (submission.grouped && (response.status == 408 || code == Some(21104)))
                        || code.is_none()
                        || code == Some(200)
                        || submission.count > 1
                        || (reply.message.is_empty()
                            && !matches!(code, Some(21520 | 21149 | 21132 | 21613)));
                    if unknown {
                        self.submission_uncertain(cx, &submission.effect);
                    } else if matches!(submission.effect, Effect::Setting { .. })
                        || (code == Some(21613) && matches!(submission.effect, Effect::Place))
                    {
                        self.refresh_account(cx);
                    }
                    let native_message = reply.message.clone();
                    let mut message = match code {
                        Some(21520) => TIER_MESSAGE.into(),
                        Some(21149) => APPROVAL_MESSAGE.into(),
                        Some(21613) => format!("Lighter rejected invalid margin mode; refreshing account state: {}", reply.message),
                        Some(21132) => "Lighter rejected margin mode switch: close open positions and orders on this market first".into(),
                        _ if !reply.message.is_empty() => reply.message,
                        _ => UNCERTAIN_RESULT.into(),
                    };
                    self.refresh_orders(cx);
                    let explained = message != UNCERTAIN_RESULT;
                    let rate_limited = matches!(response.status, 405 | 429);
                    if rate_limited {
                        message.push_str("; ");
                        message.push_str(RATE_MESSAGE);
                    }
                    let mut error = VenueError::new(message);
                    if let Some(code) = code {
                        error = error.venue_code(code.to_string());
                    }
                    if !native_message.is_empty() {
                        error = error.venue_message(native_message);
                    }
                    let error = match code {
                        _ if unknown => error.outcome(Outcome::Unknown),
                        // A bare code with no reason keeps the uncertain text; claim nothing.
                        Some(code) if !explained => {
                            error.venue_code(code.to_string()).outcome(Outcome::Unknown)
                        }
                        // Lighter's numeric codes on a sub-500 answer are explicit refusals.
                        Some(code) => error
                            .venue_code(code.to_string())
                            .kind(if rate_limited {
                                ErrorKind::RateLimited
                            } else {
                                ErrorKind::Rejected
                            })
                            .outcome(Outcome::Rejected),
                        None => error,
                    };
                    let error = if rate_limited {
                        error.kind(ErrorKind::RateLimited)
                    } else {
                        error
                    };
                    if unknown {
                        let _ = cx.http_write_unproven(&context, &error.message);
                    } else if !self.trade_refusal(cx, &context, code.unwrap(), &error.message) {
                        return;
                    }
                    self.trade_finish(cx, error);
                    return;
                }
                let ids: Vec<String> = match self.trading.active.as_ref().unwrap().steps.front() {
                    Some(Step::Place(orders) | Step::Group { orders, .. }) => {
                        orders.iter().map(|p| p.order.id.clone()).collect()
                    }
                    _ => Vec::new(),
                };
                self.trading
                    .active
                    .as_mut()
                    .unwrap()
                    .ids
                    .extend(ids.iter().cloned());
                if cx
                    .http_write_fact(&context, WriteFact::Accepted { order_ids: ids }, None)
                    .is_err()
                    || cx.validate_http_write_effects(&context).is_err()
                {
                    self.trade_finish(cx, uncertain("Invalid Lighter write evidence"));
                    return;
                }
                let step = self
                    .trading
                    .active
                    .as_mut()
                    .unwrap()
                    .steps
                    .pop_front()
                    .unwrap();
                if let Step::Group { orders, .. } = &step {
                    let job = self.trading.active.as_mut().unwrap();
                    job.group_accepted
                        .extend(orders.iter().map(|p| p.order.id.clone()));
                }
                match step {
                    Step::Position { .. } => unreachable!(),
                    Step::Place(orders) | Step::Group { orders, .. } => {
                        for mut p in orders {
                            p.order.last_update = cx.now;
                            self.record_placement(p.order.clone());
                            // A live terminal event can overtake the HTTP acknowledgement.
                            if !self.account_order_terminal(&p.order.id)
                                && !cx.store.order(&p.order.id).is_some_and(|o| {
                                    matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                                        || o.filled > 0.0
                                })
                            {
                                cx.store.upsert_order(p.order);
                            }
                        }
                    }
                    Step::Cancel(order) => {
                        if let Some(job) = self.trading.active.as_mut()
                            && job.amend.as_ref().is_some_and(|o| o.id == order.id)
                            && cx.store.order(&order.id).is_some_and(|o| {
                                o.filled > order.filled || o.status == OrderStatus::Closed
                            })
                        {
                            job.amend_filled = true;
                        }
                        self.record_cancel(&order.id, cx.now);
                        cx.store.remove_orders(&[&order.id]);
                    }
                    Step::Setting { .. } => {
                        if let Effect::Setting {
                            symbol,
                            leverage,
                            isolated,
                        } = submission.effect
                        {
                            self.account_setting_write(&symbol);
                            self.trading.uncertain_settings.remove(&symbol);
                            let margin = if isolated {
                                MarginType::Isolated
                            } else {
                                MarginType::Cross
                            };
                            cx.store.update_setting(&symbol, |s| {
                                s.leverage = leverage;
                                s.margin_type = Some(margin);
                            });
                            cx.store.update_positions(|p| {
                                if p.symbol == symbol {
                                    p.leverage = leverage;
                                    p.margin_type = Some(margin);
                                }
                            });
                            self.refresh_account(cx);
                        }
                    }
                }
                let job = self.trading.active.as_mut().unwrap();
                job.effective = true;
                job.effect = None;
                self.refresh_orders(cx);
                if self
                    .trading
                    .active
                    .as_ref()
                    .is_some_and(|job| !job.steps.is_empty())
                {
                    self.trading.active.as_mut().unwrap().delay_ms = 100;
                    self.trade_kick(cx);
                } else {
                    self.trade_kick(cx);
                }
            }
        }
    }

    pub(crate) fn trade_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        if self.trading.active.is_none() {
            return;
        }
        self.trading.inflight = false;
        match request {
            Request::Approval => {
                self.trade_finish(cx, not_sent(error).kind(ErrorKind::Network));
            }
            Request::Nonce => self.trade_finish(cx, not_sent(error).kind(ErrorKind::Network)),
            Request::Submit(s) => {
                self.submission_uncertain(cx, &s.effect);
                let error = uncertain(format!(
                    "Uncertain Lighter transaction result: {error}; check open orders before retrying"
                ));
                self.trade_finish(cx, error.kind(ErrorKind::Network));
            }
        }
    }

    fn submission_uncertain(&mut self, cx: &mut Cx<'_, Pending>, effect: &Effect) {
        self.trading.next_nonce.invalidate();
        // Frame-fed settings come from the data source, which reconciles them.
        if let Effect::Setting { symbol, .. } = effect
            && !cx.frame_fed()
        {
            self.account_setting_write(symbol);
            self.trading
                .uncertain_settings
                .insert(symbol.clone(), self.account_setting_epoch(symbol));
        }
        self.refresh_account(cx);
        self.refresh_orders(cx);
    }

    pub(crate) fn trade_limits_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.waiting_limits
            && let Some(job) = &self.trading.active
        {
            let _ = cx.request_preflight_ready(&job.preflight);
        }
    }

    pub(crate) fn trade_position_symbol(&self, serial: u64) -> Option<&String> {
        let wait = self
            .trading
            .position_wait
            .as_ref()
            .filter(|w| w.serial == serial)?;
        let job = self
            .trading
            .active
            .as_ref()
            .filter(|j| j.command == wait.command)?;
        match job.steps.front()? {
            Step::Position { symbol, .. } => Some(symbol),
            _ => None,
        }
    }
    pub(crate) fn trade_account_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        serial: u64,
        confirmed: bool,
    ) {
        if self.trading.position_wait.is_none() {
            return;
        }
        if self.trade_position_symbol(serial).is_none() {
            return;
        }
        if let Some(wait) = self.trading.position_wait.as_mut() {
            wait.confirmed = Some(confirmed);
        }
        if let Some(job) = &self.trading.active {
            if let Some(wait) = &job.condition {
                let _ = cx.release_condition_wait(wait);
            } else {
                let _ = cx.request_preflight_ready(&job.preflight);
            }
        }
    }
    fn trade_finish(&mut self, cx: &mut Cx<'_, Pending>, error: impl Into<VenueError>) {
        if let Some(wait) = self.trading.position_wait.take() {
            self.trading.retired_snapshot = Some(wait.serial);
        }
        self.trading.waiting_limits = false;
        if let Some(job) = self.trading.active.take() {
            if job.effect.is_some() {
                self.trading.next_nonce.invalidate();
            }
            let mut error = error.into();
            if !job.ids.is_empty() {
                let indexes = job.ids.join(",");
                error.message = format!(
                    "{}; previously placed order indexes: {indexes}",
                    error.message
                );
            }
            if (job.effective || !job.ids.is_empty())
                && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
            {
                error.outcome = None;
            }
            if let Some(plan) = &job.plan {
                if let (Some(slot), Some(context)) = (&job.slot, &job.live) {
                    let _ = cx.finish_plan_call(plan, slot, context);
                }
                let _ = cx.fail_plan(plan, error);
            } else {
                if let Some(context) = &job.live {
                    let _ = cx.finish_http_context(context);
                }
                let _ = cx.fail_preflight(&job.preflight, error);
            }
        }
        self.trade_wake_next(cx);
    }

    pub(crate) fn trade_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.trading.timer != Some(timer) {
            return false;
        }
        self.trading.timer = None;
        self.trade_kick(cx);
        true
    }

    fn mint_id(&mut self, cx: &Cx<'_, Pending>) -> Result<u64, String> {
        if self.trading.next_id >= MAX_CLIENT_INDEX {
            return Err("Lighter client order index namespace exhausted.".into());
        }
        let mut random = [0; 2];
        cx.fill_entropy(&mut random)?;
        let candidate = ((u128::from(cx.now as u64) * 100
            + u128::from(u16::from_le_bytes(random) % 100))
            % u128::from(MAX_CLIENT_INDEX)) as u64;
        // Every lot, bracket leg and batch shares this instance reservation.
        // Failed or uncertain sends never release an already prepared index.
        self.trading.next_id = candidate.max(self.trading.next_id + 1);
        Ok(self.trading.next_id)
    }

    fn prepare_orders(
        &mut self,
        cx: &Cx<'_, Pending>,
        orders: Vec<PlaceOrderOpts>,
    ) -> Result<VecDeque<Step>, String> {
        if orders
            .iter()
            .any(|o| o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some())
        {
            return Err("Bybit BBO and TickSize controls are not supported by Lighter".into());
        }
        let batch = orders.len() > 1
            && orders.iter().all(|o| {
                o.symbol == orders[0].symbol
                    && o.kind == OrderType::Limit
                    && !o.reduce_only.unwrap_or(false)
                    && o.stop_loss.is_none()
                    && o.take_profit.is_none()
                    && cx
                        .store
                        .market(&o.symbol)
                        .is_some_and(|m| o.amount <= m.limits.amount.max)
            });
        let mut steps = VecDeque::new();
        let mut batched = Vec::new();
        for opts in orders {
            if opts.kind == OrderType::Limit
                && opts.reduce_only.unwrap_or(false)
                && !has_position(cx, &opts.symbol, opts.side)
            {
                steps.push_back(Step::Position {
                    symbol: opts.symbol.clone(),
                    side: opts.side,
                    requested: false,
                });
            }
            if opts.kind == OrderType::Limit
                && !opts.reduce_only.unwrap_or(false)
                && (opts.stop_loss.is_some() || opts.take_profit.is_some())
            {
                steps.push_back(self.prepare_group(cx, &opts)?);
                continue;
            }
            let bracket = matches!(opts.kind, OrderType::Limit | OrderType::Market)
                && !opts.reduce_only.unwrap_or(false);
            let prepared = self.prepare_one(cx, &opts)?;
            if batch {
                batched.extend(prepared);
            } else {
                steps.extend(prepared.into_iter().map(|p| Step::Place(vec![p])));
            }
            if bracket {
                for (kind, trigger) in [
                    (OrderType::StopLoss, opts.stop_loss),
                    (OrderType::TakeProfit, opts.take_profit),
                ] {
                    if let Some(trigger) = trigger {
                        let mut child = opts.clone();
                        child.kind = kind;
                        child.side = opts.side.opposite();
                        child.price = Some(trigger);
                        child.trigger_price = Some(trigger);
                        child.stop_loss = None;
                        child.take_profit = None;
                        child.reduce_only = Some(true);
                        child.time_in_force = Some(TimeInForce::ImmediateOrCancel);
                        steps.extend(
                            self.prepare_one(cx, &child)?
                                .into_iter()
                                .map(|p| Step::Place(vec![p])),
                        );
                    }
                }
            }
        }
        if batch && !batched.is_empty() {
            steps.push_back(Step::Place(batched));
        }
        Ok(steps)
    }

    /// TS d94e9d744f1: a limit entry with protections is one grouped transaction,
    /// OTO (1) with one child or OTOCO (3) with both, parent then SL then TP.
    fn prepare_group(
        &mut self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Step, String> {
        const SPLIT: &str =
            "Lighter attached TP/SL limit orders cannot be split into multiple native lots.";
        let mut orders = self.prepare_one(cx, opts)?;
        if orders.len() != 1 {
            return Err(SPLIT.into());
        }
        for (kind, trigger) in [
            (OrderType::StopLoss, opts.stop_loss),
            (OrderType::TakeProfit, opts.take_profit),
        ] {
            let Some(trigger) = trigger else {
                continue;
            };
            let mut child = opts.clone();
            child.kind = kind;
            child.side = opts.side.opposite();
            child.price = Some(trigger);
            child.trigger_price = Some(trigger);
            child.stop_loss = None;
            child.take_profit = None;
            child.reduce_only = Some(true);
            child.time_in_force = Some(TimeInForce::ImmediateOrCancel);
            let mut leg = self.prepare_one(cx, &child)?;
            if leg.len() != 1 {
                return Err(SPLIT.into());
            }
            let mut leg = leg.pop().unwrap();
            // Children close the filled parent: native amount zero.
            leg.native.amount = 0;
            orders.push(leg);
        }
        Ok(Step::Group {
            grouping: if orders.len() == 3 { 3 } else { 1 },
            orders,
        })
    }

    fn prepare_one(
        &mut self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Vec<Prepared>, String> {
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let meta = self
            .metadata
            .get(&opts.symbol)
            .ok_or("Lighter market metadata missing")?;
        let market_id = market_id(cx, &opts.symbol)?;
        if !opts.amount.is_finite() || opts.amount <= 0.0 {
            return Err("Invalid Lighter order amount".into());
        }
        let amount = num::adjust(opts.amount, market.precision.amount);
        let conditional = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
        let is_market = opts.kind == OrderType::Market;
        if opts.kind == OrderType::Conditional {
            return Err("Lighter opening conditional orders are unsupported".into());
        }
        if opts.kind == OrderType::TrailingStopLoss {
            return Err("Lighter trailing stops are unsupported".into());
        }
        let trigger = if conditional {
            opts.trigger_price.or(opts.price).unwrap_or(0.0)
        } else {
            0.0
        };
        if conditional && (!trigger.is_finite() || trigger <= 0.0) {
            return Err("Trigger price is required for conditional Lighter orders".into());
        }
        let trigger = num::adjust(trigger, market.precision.price);
        let price = if is_market {
            let ticker = cx
                .store
                .ticker(&opts.symbol)
                .ok_or("No live Lighter price available for market-style order")?;
            let (side, other) = if opts.side == OrderSide::Buy {
                (ticker.ask, ticker.bid)
            } else {
                (ticker.bid, ticker.ask)
            };
            let midpoint = if ticker.ask > 0.0 && ticker.bid > 0.0 {
                (ticker.ask + ticker.bid) / 2.0
            } else {
                0.0
            };
            [
                side,
                ticker.mark,
                ticker.last,
                ticker.index,
                midpoint,
                other,
            ]
            .into_iter()
            .find(|p| p.is_finite() && *p > 0.0)
            .ok_or("No live Lighter price available for market-style order")?
                * if opts.side == OrderSide::Buy {
                    1.02
                } else {
                    0.98
                }
        } else if conditional {
            let slip = opts
                .max_slippage_pct
                .filter(|v| v.is_finite() && *v >= 0.0)
                .or_else(|| {
                    (self.options.stop_slippage_pct.is_finite()
                        && self.options.stop_slippage_pct >= 0.0)
                        .then_some(self.options.stop_slippage_pct)
                })
                .unwrap_or(1.0)
                .min(50.0);
            trigger
                * if opts.side == OrderSide::Buy {
                    1.0 + slip / 100.0
                } else {
                    1.0 - slip / 100.0
                }
        } else {
            opts.price.unwrap_or(0.0)
        };
        let price = num::adjust(price, market.precision.price);
        if !price.is_finite() || price <= 0.0 || amount <= 0.0 {
            return Err("Invalid Lighter order price or amount".into());
        }
        let size_decimals = meta.size_decimals;
        let price_decimals = meta.price_decimals;
        let min_quote = meta.min_quote;
        let max = market.limits.amount.max;
        let min = market.limits.amount.min;
        if !max.is_finite() || max <= 0.0 {
            return Err("Invalid Lighter market maximum size".into());
        }
        let count = (amount / max).ceil().max(1.0) as usize;
        if count > 10_000 {
            return Err("Lighter order would create too many lots".into());
        }
        let remainder = if amount > max {
            num::adjust(amount % max, market.precision.amount)
        } else {
            0.0
        };
        let lot = num::adjust((amount - remainder) / count as f64, market.precision.amount);
        let mut amounts = vec![lot; count];
        if remainder > 0.0 {
            amounts.push(remainder);
        }
        if !is_market && amounts.len() > 1 && min > 0.0 && *amounts.last().unwrap() < min {
            let last = amounts.pop().unwrap();
            *amounts.last_mut().unwrap() += last;
        }
        let tif = if conditional {
            Some(TimeInForce::ImmediateOrCancel)
        } else {
            opts.time_in_force
        };
        let time_in_force = if is_market
            || matches!(
                tif,
                Some(TimeInForce::ImmediateOrCancel | TimeInForce::FillOrKill)
            ) {
            0
        } else if tif == Some(TimeInForce::PostOnly) {
            2
        } else {
            1
        };
        let kind = match opts.kind {
            OrderType::Limit => 0,
            OrderType::Market => 1,
            OrderType::StopLoss => 2,
            OrderType::TakeProfit => 4,
            OrderType::Conditional | OrderType::TrailingStopLoss => unreachable!(),
        };
        let scaled_price = u32::try_from(scale(price, price_decimals)?)
            .map_err(|_| "Lighter price exceeds uint32 range")?;
        let scaled_trigger = u32::try_from(scale(trigger, price_decimals)?)
            .map_err(|_| "Lighter trigger exceeds uint32 range")?;
        let mut out = Vec::new();
        for amount in amounts {
            if amount <= 0.0 || (!is_market && (amount < min || amount * price < min_quote)) {
                return Err(format!(
                    "Lighter order lot is below exchange minimums for {}",
                    opts.symbol
                ));
            }
            let scaled_amount = scale(amount, size_decimals)?;
            if scaled_amount == 0 {
                return Err("Lighter amount rounds to zero".into());
            }
            let id = self.mint_id(cx)?;
            let order = Order {
                id: id.to_string(),
                parent_id: None,
                status: OrderStatus::Open,
                symbol: opts.symbol.clone(),
                kind: opts.kind,
                side: opts.side,
                price: if conditional { trigger } else { price },
                amount,
                filled: 0.0,
                remaining: amount,
                reduce_only: opts.reduce_only.unwrap_or(false),
                close: opts.close,
                client_order_id: None,
                time_in_force: tif,
                last_update: cx.now,
                trigger_type: opts.trigger_type,
                take_profit: None,
                stop_loss: None,
                extra: None,
            };
            out.push(Prepared {
                native: sign::Order {
                    order_id: id,
                    market_id,
                    is_buy: opts.side == OrderSide::Buy,
                    price: scaled_price,
                    amount: scaled_amount,
                    order_type: kind,
                    time_in_force,
                    reduce_only: order.reduce_only,
                    trigger_price: scaled_trigger,
                    order_expiry: None,
                },
                order,
            });
        }
        Ok(out)
    }
}

fn market_id(cx: &Cx<'_, Pending>, symbol: &str) -> Result<u32, String> {
    cx.store
        .market(symbol)
        .and_then(|m| m.id.parse::<u32>().ok())
        .filter(|n| *n <= 32767 && *n != 255)
        .ok_or_else(|| format!("Invalid Lighter perps market {symbol}"))
}
fn has_position(cx: &Cx<'_, Pending>, symbol: &str, side: OrderSide) -> bool {
    cx.store.positions().iter().any(|p| {
        p.symbol == symbol
            && p.contracts > 0.0
            && ((p.side == PositionSide::Long) == (side == OrderSide::Sell))
    })
}
fn scale(value: f64, decimals: u32) -> Result<u64, String> {
    let n = (value * 10_f64.powi(i32::try_from(decimals).map_err(|_| "Invalid Lighter decimals")?))
        .round();
    if !n.is_finite() || !(0.0..=9_007_199_254_740_991.0).contains(&n) {
        return Err("Lighter scaled number exceeds safe integer range".into());
    }
    Ok(n as u64)
}
fn snap_leverage(requested: f64) -> f64 {
    [1.0, 2.0, 3.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 50.0]
        .into_iter()
        .min_by(|a, b| (requested - a).abs().total_cmp(&(requested - b).abs()))
        .unwrap()
}
fn protective_kind(cx: &Cx<'_, Pending>, order: &Order, price: f64) -> OrderType {
    if !order.reduce_only || order.kind != OrderType::Limit {
        return order.kind;
    }
    let Some(reference) = cx.store.ticker(&order.symbol).and_then(|t| {
        [t.mark, t.last, t.index]
            .into_iter()
            .find(|n| n.is_finite() && *n > 0.0)
    }) else {
        return order.kind;
    };
    if (order.side == OrderSide::Sell && price <= reference)
        || (order.side == OrderSide::Buy && price >= reference)
    {
        OrderType::StopLoss
    } else {
        OrderType::Limit
    }
}
const UNCERTAIN_RESULT: &str =
    "Uncertain Lighter transaction result; check open orders before retrying";

/// Refused before any transaction was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}
/// The submission may have executed; callers must reconcile before resending.
fn uncertain(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}

/// A native pre-I/O refusal settles only the genuine Mutation entry.
fn refuse(cx: &mut Cx<'_, Pending>, error: VenueError) {
    cx.refuse_command(Operation::Mutation, error);
}

pub(crate) fn serial_call(operation: Operation) -> SerialCall {
    SerialCall {
        auth: CallAuth::Preflight,
        operation,
        timeout_ms: 20_000,
        attempts: 1,
        pacing: None,
    }
}
impl Lighter {
    fn trade_context(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        operation: Operation,
        intent: Option<safe_cex_engine::classify::Intent>,
    ) -> Option<Context> {
        let job = self.trading.active.as_mut()?;
        let result = if let Some(plan) = &job.plan {
            if job.live.is_some() {
                let context = job.live.take().unwrap();
                let slot = job.slot.take().unwrap();
                if job.delay_ms > 0 {
                    let delay = std::mem::take(&mut job.delay_ms);
                    match cx.continue_serial_plan_after(
                        plan,
                        &slot,
                        &context,
                        delay,
                        serial_call(operation),
                    ) {
                        Ok(()) => {
                            job.waiting_plan = true;
                            self.trading.inflight = false;
                            return None;
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    cx.continue_serial_plan(plan, &slot, &context, serial_call(operation))
                        .and_then(|slot| {
                            let context = Self::trade_plan_context(cx, plan, &slot, intent)?;
                            job.slot = Some(slot);
                            Ok(context)
                        })
                }
            } else if let Some(slot) = &job.slot {
                Self::trade_plan_context(cx, plan, slot, intent)
            } else {
                return None;
            }
        } else if operation == Operation::Read {
            cx.preflight_read_context(
                &job.preflight,
                Policy {
                    operation,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
                safe_cex_engine::request_context::plan::CallTerms {
                    timeout_ms: 20_000,
                    attempts: 1,
                    wait_ms: 0,
                    retry_window: safe_cex_engine::request_context::plan::RetryWindow::FreshAttempt,
                },
            )
        } else {
            let credential = crate::primary_credential(self.account.identity.as_ref()?);
            cx.admit_serial_plan(
                &job.preflight,
                serial_call(operation),
                SerialPolicy {
                    credential: Some(credential),
                    pacing: None,
                    refusals_complete: true,
                },
            )
            .and_then(|(plan, slot)| {
                let context = Self::trade_plan_context(cx, &plan, &slot, intent)?;
                job.plan = Some(plan);
                job.slot = Some(slot);
                Ok(context)
            })
        };
        match result {
            Ok(context) => {
                self.trading.active.as_mut().unwrap().live = Some(context.clone());
                Some(context)
            }
            Err(error) => {
                self.trade_finish(cx, error.venue_error());
                None
            }
        }
    }
    fn trade_plan_context(
        cx: &mut Cx<'_, Pending>,
        plan: &Plan,
        slot: &Slot,
        intent: Option<safe_cex_engine::classify::Intent>,
    ) -> Result<Context, safe_cex_engine::request_context::Error> {
        match intent {
            Some(intent) => cx.plan_leg_context_with_intent(plan, slot, intent),
            None => cx.plan_leg_context(plan, slot),
        }
    }
    fn trade_read(&mut self, cx: &mut Cx<'_, Pending>, url: String, request: Request) {
        let Some(context) = self.trade_context(cx, Operation::Read, None) else {
            return;
        };
        self.trading.inflight = true;
        if let Err((_, error)) = cx.request_managed(
            context,
            None,
            Method::Get,
            url,
            None,
            Pending::Trade(request),
        ) {
            self.trading.inflight = false;
            self.trade_finish(cx, error.venue_error());
        }
    }
    fn trade_read_finished(&mut self, cx: &mut Cx<'_, Pending>, context: &Context) {
        if self
            .trading
            .active
            .as_ref()
            .is_some_and(|job| job.plan.is_none())
        {
            let preflight = &self.trading.active.as_ref().unwrap().preflight;
            if let Err(error) = cx.finish_preflight_call(preflight, context) {
                self.trade_finish(cx, error.venue_error());
                return;
            }
            self.trading.active.as_mut().unwrap().live = None;
        }
    }
    fn trade_refusal(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        code: u64,
        message: &str,
    ) -> bool {
        match Self::record_refusal(cx, context, code, message) {
            Ok(()) => true,
            Err(error) => {
                self.trade_finish(cx, error.venue_error());
                false
            }
        }
    }

    pub(crate) fn record_refusal(
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        code: u64,
        message: &str,
    ) -> Result<(), safe_cex_engine::request_context::Error> {
        let table = CodeTable {
            rows: &[Row {
                key: Key::Range(0, i64::MAX),
                verdict: Verdict::Refused(ErrorKind::Rejected),
                only: None,
            }],
            success: &[],
            status: &[],
            unlisted: Verdict::Unknown(None),
        };
        let native = Native {
            code: NativeCode::Int(code as i64),
            message,
        };
        cx.classify_write(context, &table, Some(&native), message)
            .and_then(|fact| {
                cx.http_write_fact(
                    context,
                    WriteFact::Failed {
                        fact,
                        known_effects: false,
                    },
                    None,
                )
            })
            .map(|_| ())
    }

    fn trade_wake_next(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(job) = self.trading.queue.front() {
            let _ = cx.request_preflight_ready(&job.preflight);
        }
    }
    pub(crate) fn trade_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &Preflight,
    ) {
        self.settle_retired_trades(cx);
        if self.trading.active.is_none() {
            if !self
                .trading
                .queue
                .front()
                .is_some_and(|job| job.preflight.owner() == preflight.owner())
            {
                let _ = cx.park_preflight_for(preflight, None);
                return;
            }
            self.trading.active = self.trading.queue.pop_front();
        }
        if !self
            .trading
            .active
            .as_ref()
            .is_some_and(|job| job.preflight.owner() == preflight.owner())
        {
            let _ = cx.park_preflight_for(preflight, None);
            return;
        }
        if std::mem::take(&mut self.trading.waiting_limits) {
            if self.limits_standard() {
                self.trade_finish(cx, not_sent(TIER_MESSAGE));
                return;
            }
            self.trading.active.as_mut().unwrap().builder_stage = 1;
        }
        if let Some(wait) = self.trading.position_wait.take()
            && wait.confirmed != Some(true)
        {
            self.trade_finish(cx, not_sent("Lighter account snapshot could not confirm reduce-only exposure; order was not sent"));
            return;
        }
        self.trade_kick(cx);
    }
    pub(crate) fn trade_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        self.settle_retired_trades(cx);
        let Some(job) = self.trading.active.as_mut().filter(|job| {
            job.plan
                .as_ref()
                .is_some_and(|own| own.owner() == plan.owner())
        }) else {
            return;
        };
        if job.settled {
            // A released settled Plan has no bound left: every path seals it.
            job.condition = None;
            job.waiting_plan = false;
            if !job.group_proven() {
                self.trade_group_unconfirmed(cx);
                return;
            }
            let job = self.trading.active.take().unwrap();
            let plan = job.plan.clone().unwrap();
            if cx
                .done_plan(&plan, serde_json::to_string(&job.ids).unwrap())
                .is_err()
            {
                self.trading.active = Some(job);
                self.trade_group_unconfirmed(cx);
                return;
            }
            self.trade_wake_next(cx);
            return;
        }
        job.slot = cx
            .plan_slots(plan)
            .ok()
            .and_then(|slots| slots.into_iter().next());
        job.waiting_plan = false;
        job.condition = None;
        if let Some(wait) = self.trading.position_wait.take()
            && wait.confirmed != Some(true)
        {
            self.trade_finish(cx, not_sent("Lighter account snapshot could not confirm reduce-only exposure; order was not sent"));
            return;
        }
        self.trade_kick(cx);
    }
}

impl Lighter {
    pub(crate) fn trade_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) -> Option<Preflight> {
        self.trading
            .queue
            .retain(|job| !owner.matches_preflight(&job.preflight));
        if self
            .trading
            .active
            .as_ref()
            .is_some_and(|job| owner.matches_preflight(&job.preflight))
        {
            let job = self.trading.active.take().unwrap();
            if let Some(effect) = job.effect {
                self.trading.retired_effects.push(effect);
            }
            self.trading.inflight = false;
            self.trading.waiting_limits = false;
            self.trading.position_wait = None;
            return self.trading.queue.front().map(|job| job.preflight.clone());
        }
        None
    }
    pub(crate) fn restore_retired_trades(&mut self, frame_fed: bool) -> bool {
        let effects = std::mem::take(&mut self.trading.retired_effects);
        let refresh = !effects.is_empty();
        for effect in effects {
            self.trading.next_nonce.invalidate();
            // Frame-fed settings come from the data source, which reconciles them.
            if let Effect::Setting { symbol, .. } = effect
                && !frame_fed
            {
                self.account_setting_write(&symbol);
                self.trading
                    .uncertain_settings
                    .insert(symbol.clone(), self.account_setting_epoch(&symbol));
            }
        }
        refresh
    }
    pub(crate) fn settle_retired_trades(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.restore_retired_trades(cx.frame_fed()) {
            self.refresh_account(cx);
            self.refresh_orders(cx);
        }
    }
    pub(crate) fn trade_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        if self.trading.active.is_none() {
            return;
        }
        self.trading.inflight = false;
        if let Request::Submit(submission) = request
            && error.outcome != Some(Outcome::NotSent)
        {
            self.submission_uncertain(cx, &submission.effect);
        }
        self.trade_finish(cx, error);
    }
}

impl Lighter {
    pub(crate) fn trade_condition_expired(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        owner: &safe_cex_engine::request_context::ConditionOwner,
        error: VenueError,
    ) {
        let matches = self.trading.active.as_ref().is_some_and(|job| match owner {
            safe_cex_engine::request_context::ConditionOwner::Preflight(preflight) => {
                preflight.owner() == job.preflight.owner()
            }
            safe_cex_engine::request_context::ConditionOwner::Plan(plan) => job
                .plan
                .as_ref()
                .is_some_and(|own| own.owner() == plan.owner()),
        });
        if !matches {
            return;
        }
        if self.trading.active.as_ref().is_some_and(|job| job.settled) {
            self.trading.active.as_mut().unwrap().condition = None;
            self.trade_group_unconfirmed(cx);
        } else {
            self.trade_finish(cx, error);
        }
    }
}

#[cfg(test)]
mod client_index_parity_tests {
    use super::*;
    use safe_cex_engine::test_support::{ProbeVenue, seeded_engine};

    #[test]
    fn client_index_maximum_is_reserved_once_then_exhaustion_never_wraps() {
        let mut venue = Lighter::new(crate::Options::default());
        venue.trading.next_id = MAX_CLIENT_INDEX - 1;
        let mut engine = seeded_engine(ProbeVenue::new(venue, |venue, cx| {
            assert_eq!(venue.mint_id(cx), Ok(MAX_CLIENT_INDEX));
            assert_eq!(
                venue.mint_id(cx).unwrap_err(),
                "Lighter client order index namespace exhausted."
            );
            assert_eq!(venue.trading.next_id, MAX_CLIENT_INDEX);
            assert!(venue.mint_id(cx).is_err());
            assert_eq!(venue.trading.next_id, MAX_CLIENT_INDEX);
        }));
        engine.start(1000.);
        assert!(!engine.drain_actions().any(|action| matches!(
            action,
            safe_cex_engine::Action::Http(_) | safe_cex_engine::Action::Sign { .. }
        )));
    }

    #[test]
    fn a_zero_timestamp_and_entropy_candidate_still_reserves_positive_indices() {
        let entropy = safe_cex_engine::entropy::Entropy::new(
            Box::new(|bytes| {
                bytes.fill(0);
                Ok(())
            }),
            Box::new(|bytes| {
                bytes.fill(0);
                Ok(())
            }),
        );
        let venue = Lighter::new(crate::Options::default());
        let mut engine = safe_cex_engine::Engine::new(
            ProbeVenue::new(venue, |venue, cx| {
                assert_eq!(venue.mint_id(cx), Ok(1));
                assert_eq!(venue.mint_id(cx), Ok(2));
            }),
            Some(entropy),
        )
        .unwrap();
        engine.start(0.);
    }

    #[test]
    fn each_reservation_reconsiders_the_current_timestamp_candidate() {
        let mut venue = Lighter::new(crate::Options::default());
        venue.trading.next_id = 7;
        let mut engine = seeded_engine(ProbeVenue::new(venue, |venue, cx| {
            let first = venue.mint_id(cx).unwrap();
            assert!((170_000_000_000_000..=170_000_000_000_099).contains(&first));
            let next = venue.mint_id(cx).unwrap();
            assert!(next > first && next <= MAX_CLIENT_INDEX);
        }));
        engine.start(1_700_000_000_000.);
    }
}
