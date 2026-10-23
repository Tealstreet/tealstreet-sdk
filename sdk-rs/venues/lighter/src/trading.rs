//! Serialized nonce allocation and submissions. Transport failures never replay writes.
use crate::{Lighter, Pending, sign};
use safe_cex_engine::model::*;
use safe_cex_engine::url;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, TimerId, VenueError, num,
};
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};

const TIER_MESSAGE: &str = "Lighter requires Plus or Premium account tier for Tealstreet partner-attributed orders with non-zero integrator fees. Switch this account to Plus or Premium, then retry.";
const APPROVAL_MESSAGE: &str = "Tealstreet partner attribution is not yet approved for this Lighter account. Open Account Settings, edit this Lighter account, then click \"Approve Builder Fee\" and retry.";
const POSITION_READ_TIMEOUT_MS: f64 = 10_000.0;
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
    uncertain_settings: HashMap<String, u64>,
}
struct PositionWait {
    command: CommandId,
    serial: u64,
    timer: TimerId,
}
struct Job {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    builder_stage: u8,
    retried_nonce: bool,
    amend: Option<Order>,
    amend_filled: bool,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}
enum Step {
    Position {
        symbol: String,
        side: OrderSide,
        requested: bool,
    },
    Place(Vec<Prepared>),
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
}
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
    pub(crate) fn trade_order_observed(&mut self, id: &str, filled: f64, status: OrderStatus) {
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
                    cx.fail(
                        command,
                        not_sent("Lighter does not support caller client order IDs"),
                    );
                    return;
                }
                let steps = match self.prepare_orders(cx, orders) {
                    Ok(steps) => steps,
                    Err(error) => {
                        cx.fail(command, not_sent(error));
                        return;
                    }
                };
                (command, steps)
            }
            Command::CancelOrders { command, ids } => {
                let mut steps = VecDeque::new();
                for id in ids {
                    if id.parse::<u64>().is_err() {
                        cx.fail(
                            command,
                            not_sent("Cannot cancel Lighter order: expected a numeric order index"),
                        );
                        return;
                    }
                    let Some(order) = cx.store.order(&id).cloned() else {
                        cx.fail(command, not_sent(format!("Lighter order {id} not found")));
                        return;
                    };
                    if let Err(error) = market_id(cx, &order.symbol) {
                        cx.fail(command, not_sent(error));
                        return;
                    }
                    steps.push_back(Step::Cancel(Box::new(order)));
                }
                (command, steps)
            }
            Command::CancelSymbolOrders { command, symbol } => (
                command,
                cx.store
                    .orders()
                    .iter()
                    .filter(|o| {
                        o.symbol == symbol
                            && matches!(o.status, OrderStatus::Open | OrderStatus::Creating)
                    })
                    .cloned()
                    .map(|order| Step::Cancel(Box::new(order)))
                    .collect(),
            ),
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
                    cx.fail(
                        command,
                        not_sent("Lighter order already has an amendment pending"),
                    );
                    return;
                }
                let Some(old) = cx.store.order(&id).cloned() else {
                    cx.fail(command, not_sent("Lighter order not found"));
                    return;
                };
                if !matches!(old.status, OrderStatus::Open | OrderStatus::Creating) {
                    cx.fail(command, not_sent("Lighter order is not open for amendment"));
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
                        cx.fail(command, not_sent(error));
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
                command,
                hedged,
                symbol: _,
            } => {
                if hedged {
                    cx.fail(
                        command,
                        not_sent("Lighter supports one-way position mode only")
                            .kind(ErrorKind::Unsupported),
                    );
                } else {
                    cx.store.set_hedged(false);
                    cx.done(command, "null".into());
                }
                return;
            }
            _ => return,
        };
        self.trading.queue.push_back(Job {
            command,
            steps,
            ids: vec![],
            builder_stage: 0,
            retried_nonce: false,
            amend,
            amend_filled: false,
            effective: false,
        });
        self.trade_kick(cx);
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
        if job.steps.is_empty() {
            let job = self.trading.active.take().unwrap();
            cx.done(job.command, serde_json::to_string(&job.ids).unwrap());
            self.trade_kick(cx);
            return;
        }
        if self.account.signer.is_none() {
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
            } else if *requested {
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
                    timer: cx.set_timer(POSITION_READ_TIMEOUT_MS),
                });
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
        if self.builder().is_some()
            && job.steps.iter().any(|s| matches!(s, Step::Place(_)))
            && job.builder_stage < 2
        {
            if job.builder_stage == 0 {
                if !self.refresh_limits(cx) {
                    self.trading.waiting_limits = true;
                    return;
                }
                if self.limits_standard() {
                    self.trade_finish(cx, not_sent(TIER_MESSAGE));
                    return;
                }
                self.trading.active.as_mut().unwrap().builder_stage = 1;
                self.trade_kick(cx);
                return;
            }
            let account = self.account.account_index.unwrap();
            self.trading.inflight = true;
            cx.request(
                Method::Get,
                format!("{}/api/v1/account?by=index&value={account}", self.host()),
                vec![],
                None,
                Pending::Trade(Request::Approval),
            );
            return;
        }
        if !self.trading.next_nonce.seeded() {
            let account = self.account.account_index.unwrap();
            let key = self.account.api_key_index.unwrap();
            self.trading.inflight = true;
            self.trading.nonce_ticket = self.trading.next_nonce.ticket();
            cx.request(
                Method::Get,
                format!(
                    "{}/api/v1/nextNonce?account_index={account}&api_key_index={key}",
                    self.host()
                ),
                vec![],
                None,
                Pending::Trade(Request::Nonce),
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
        let mut signed = Vec::with_capacity(count);
        for tx in txs {
            let Some(nonce) = self.trading.next_nonce.take() else {
                self.trade_finish(cx, not_sent("Lighter transaction nonce exhausted"));
                return;
            };
            match self
                .account
                .signer
                .as_ref()
                .unwrap()
                .sign(&tx, nonce, cx.now as u64, |bytes| {
                    cx.fill_signing_entropy(bytes).map_err(|_| ())
                }) {
                Ok(tx) => signed.push(tx),
                Err(error) => {
                    self.trading.next_nonce.invalidate();
                    self.trade_finish(cx, not_sent(error));
                    return;
                }
            }
        }
        let (path, body) = if count == 1 {
            (
                "sendTx",
                format!(
                    "tx_type={}&tx_info={}",
                    signed[0].tx_type,
                    url::encode_form(&signed[0].tx_info)
                ),
            )
        } else {
            let types = signed.iter().map(|s| s.tx_type).collect::<Vec<_>>();
            let infos = signed
                .iter()
                .map(|s| s.tx_info.as_str())
                .collect::<Vec<_>>();
            (
                "sendTxBatch",
                format!(
                    "tx_types={}&tx_infos={}",
                    url::encode_form(&serde_json::to_string(&types).unwrap()),
                    url::encode_form(&serde_json::to_string(&infos).unwrap())
                ),
            )
        };
        if let Effect::Setting { symbol, .. } = &effect {
            self.account_setting_write(symbol);
        }
        let job = self.trading.active.as_ref().unwrap();
        if let Some(Step::Place(orders)) = job.steps.front() {
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
        self.trading.inflight = true;
        cx.request(
            Method::Post,
            format!("{}/api/v1/{path}", self.host()),
            vec![("Content-Type", "application/x-www-form-urlencoded".into())],
            Some(body),
            Pending::Trade(Request::Submit(Submission { effect, count })),
        );
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
                if code == Some(21104)
                    && response.status < 500
                    && !matches!(response.status, 405 | 429)
                    && !self.trading.active.as_ref().unwrap().retried_nonce
                {
                    self.trading.active.as_mut().unwrap().retried_nonce = true;
                    self.trading.next_nonce.invalidate();
                    self.trading.timer = Some(cx.set_timer(150.0));
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
                    self.trade_finish(cx, error);
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
                match step {
                    Step::Position { .. } => unreachable!(),
                    Step::Place(orders) => {
                        for mut p in orders {
                            p.order.last_update = cx.now;
                            self.record_placement(p.order.clone());
                            self.trading
                                .active
                                .as_mut()
                                .unwrap()
                                .ids
                                .push(p.order.id.clone());
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
                job.retried_nonce = false;
                job.effective = true;
                self.refresh_orders(cx);
                if self
                    .trading
                    .active
                    .as_ref()
                    .is_some_and(|job| !job.steps.is_empty())
                {
                    self.trading.timer = Some(cx.set_timer(100.0));
                } else {
                    self.trade_kick(cx);
                }
            }
        }
    }

    pub(crate) fn trade_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        self.trading.inflight = false;
        match request {
            Request::Approval => {
                self.trading.active.as_mut().unwrap().builder_stage = 2;
                self.trade_kick(cx);
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
        if let Effect::Setting { symbol, .. } = effect {
            self.account_setting_write(symbol);
            self.trading
                .uncertain_settings
                .insert(symbol.clone(), self.account_setting_epoch(symbol));
        }
        self.refresh_account(cx);
        self.refresh_orders(cx);
    }

    pub(crate) fn trade_limits_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        if std::mem::take(&mut self.trading.waiting_limits) {
            if self.limits_standard() {
                self.trade_finish(cx, not_sent(TIER_MESSAGE));
                return;
            }
            if let Some(job) = self.trading.active.as_mut() {
                job.builder_stage = 1;
            }
        }
        self.trade_kick(cx);
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
            self.trade_kick(cx);
            return;
        }
        if self.trade_position_symbol(serial).is_none() {
            return;
        }
        let wait = self.trading.position_wait.take().unwrap();
        cx.clear_timer(wait.timer);
        if !confirmed {
            self.trade_finish(cx, not_sent("Lighter account snapshot could not confirm reduce-only exposure; order was not sent"));
            return;
        }
        let job = self.trading.active.as_ref().unwrap();
        let Some(Step::Position { symbol, side, .. }) = job.steps.front() else {
            unreachable!()
        };
        if !has_position(cx, symbol, *side) {
            self.trade_finish(
                cx,
                not_sent("No open position available for this Lighter reduce-only limit order"),
            );
            return;
        }
        self.trading.active.as_mut().unwrap().steps.pop_front();
        self.trade_kick(cx);
    }
    fn trade_finish(&mut self, cx: &mut Cx<'_, Pending>, error: impl Into<VenueError>) {
        if let Some(wait) = self.trading.position_wait.take() {
            cx.clear_timer(wait.timer);
            self.trading.retired_snapshot = Some(wait.serial);
        }
        self.trading.waiting_limits = false;
        if let Some(job) = self.trading.active.take() {
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
            cx.fail(job.command, error);
        }
        self.trade_kick(cx);
    }

    pub(crate) fn trade_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self
            .trading
            .position_wait
            .as_ref()
            .is_some_and(|w| w.timer == timer)
        {
            let wait = self.trading.position_wait.take().unwrap();
            self.trading.retired_snapshot = Some(wait.serial);
            self.trade_finish(
                cx,
                not_sent("Timed out confirming Lighter reduce-only exposure; order was not sent"),
            );
            return true;
        }
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
