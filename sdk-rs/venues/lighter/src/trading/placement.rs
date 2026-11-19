//! A sendTx acknowledgement means the sequencer queued the transaction, not that
//! the order rests or filled. Each ordinary placement stays pending until
//! authoritative venue state (`accountOrders`, or a failed `tx`) proves it.
use super::*;
use safe_cex_engine::request_context::{PendingPlacement, Scope};

/// Ten seconds bounds the proof; past it the outcome is reported Unknown.
pub(super) const PROOF_BOUND_MS: u32 = 10_000;
const POLL_MS: [f64; 5] = [250.0, 500.0, 1_000.0, 1_500.0, 2_000.0];
/// `accountOrders` accepts at most 20 client order indexes per read.
const ROWS_PER_READ: usize = 20;
const REASON_BYTES: usize = 160;
/// The legacy pace between a proven placement and the command's next write.
const NEXT_WRITE_PACE_MS: f64 = 100.0;

pub(super) struct Proof {
    pub receipt: Option<PendingPlacement>,
    /// Client order indexes, in transaction order, paired with `hashes`.
    pub ids: Vec<String>,
    pub hashes: Vec<String>,
    pub deadline: f64,
    pub token: u64,
    polls: usize,
    inflight: bool,
    verdicts: HashMap<String, Verdict>,
    pub outcome: Option<Result<(), VenueError>>,
}
#[derive(Clone)]
enum Verdict {
    Live,
    /// The native reason: an order status or a failed transaction's text.
    Refused(String),
}
pub enum ProofRead {
    Orders { token: u64 },
    Tx { token: u64, id: String },
}
impl ProofRead {
    fn token(&self) -> u64 {
        match self {
            Self::Orders { token } | Self::Tx { token, .. } => *token,
        }
    }
}
impl Proof {
    pub(super) fn new(
        receipt: PendingPlacement,
        ids: Vec<String>,
        hashes: Vec<String>,
        deadline: f64,
        token: u64,
    ) -> Self {
        Self {
            receipt: Some(receipt),
            ids,
            hashes,
            deadline,
            token,
            polls: 0,
            inflight: false,
            verdicts: HashMap::new(),
            outcome: None,
        }
    }
    pub(super) fn unresolved(&self) -> Vec<String> {
        self.ids
            .iter()
            .filter(|id| !self.verdicts.contains_key(*id))
            .cloned()
            .collect()
    }
}

#[derive(Deserialize)]
struct OrdersReply {
    #[serde(default)]
    code: Option<Number>,
    #[serde(default)]
    orders: Vec<OrderRow>,
}
#[derive(Deserialize)]
struct OrderRow {
    #[serde(default)]
    client_order_index: Option<Number>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    filled_base_amount: Option<Amount>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Amount {
    Text(String),
    Number(f64),
}
impl Amount {
    fn positive(&self) -> bool {
        match self {
            Self::Text(text) => text.parse::<f64>().is_ok_and(|n| n > 0.0),
            Self::Number(n) => *n > 0.0,
        }
    }
}
#[derive(Deserialize)]
struct TxReply {
    #[serde(default)]
    code: Option<Number>,
    #[serde(default)]
    status: Option<Number>,
    #[serde(default)]
    event_info: String,
    #[serde(default)]
    message: String,
}

/// Native order statuses that refuse an order before it ever rests or fills.
/// Plain `canceled`, expiry and OCO/child/liquidation cancels follow acceptance.
fn refusal_text(status: &str) -> Option<&'static str> {
    Some(match status {
        "canceled-margin-not-allowed" => "not enough margin",
        "canceled-post-only" => "the post-only order would have crossed the book",
        "canceled-reduce-only" => "the reduce-only order would not reduce the position",
        "canceled-position-not-allowed" => "the position is not allowed",
        "canceled-too-much-slippage" => "too much slippage",
        "canceled-not-enough-liquidity" => "not enough liquidity",
        "canceled-self-trade" => "self-trade prevention",
        "canceled-invalid-balance" => "invalid balance",
        _ => return None,
    })
}
fn verdict(row: &OrderRow) -> Option<Verdict> {
    if row
        .filled_base_amount
        .as_ref()
        .is_some_and(Amount::positive)
    {
        return Some(Verdict::Live);
    }
    let status = row.status.as_str();
    if refusal_text(status).is_some() {
        return Some(Verdict::Refused(status.into()));
    }
    (matches!(status, "open" | "pending" | "in-progress" | "filled")
        || status.starts_with("canceled")
        || status.starts_with("cancelled"))
    .then_some(Verdict::Live)
}
/// The text fields a failed transaction's `event_info` has been seen to carry.
#[derive(Deserialize)]
struct EventInfo {
    #[serde(default)]
    ae: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    err: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}
/// A failed transaction's bounded native text; `event_info` has no schema.
fn tx_reason(reply: &TxReply) -> String {
    let text = serde_json::from_str::<EventInfo>(&reply.event_info)
        .ok()
        .and_then(|info| {
            [info.ae, info.error, info.err, info.message, info.reason]
                .into_iter()
                .flatten()
                .find(|text| !text.is_empty())
        })
        .or_else(|| (!reply.message.is_empty()).then(|| reply.message.clone()))
        .unwrap_or_else(|| "transaction failed".into());
    let mut end = text.len().min(REASON_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

impl Lighter {
    fn proof_mut(&mut self, token: u64) -> Option<&mut Proof> {
        self.trading
            .active
            .as_mut()?
            .proof
            .as_mut()
            .filter(|proof| proof.token == token && proof.outcome.is_none())
    }

    /// One read of every unresolved placement; the first runs at acknowledgement.
    pub(super) fn proof_poll(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(job) = self.trading.active.as_mut() else {
            return;
        };
        let Some(proof) = job.proof.as_mut().filter(|p| p.outcome.is_none()) else {
            return;
        };
        if proof.inflight {
            return;
        }
        let ids: Vec<String> = proof.unresolved().into_iter().take(ROWS_PER_READ).collect();
        let token = proof.token;
        proof.polls += 1;
        let (Some(account), Some(identity)) =
            (self.account.account_index, self.account.identity.clone())
        else {
            self.proof_schedule(cx);
            return;
        };
        let url = format!(
            "{}/api/v1/accountOrders?account_index={account}&client_order_indexes={}",
            self.host(),
            ids.join(",")
        );
        let credential = Some(crate::primary_credential(&identity));
        let Ok(recipe) = self.auth_recipe(identity, sign::AuthPlacement::Header) else {
            self.proof_schedule(cx);
            return;
        };
        self.proof_request(
            cx,
            url,
            credential,
            Some(recipe),
            ProofRead::Orders { token },
        );
    }

    fn proof_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        url: String,
        credential: Option<safe_cex_engine::request_context::Credential>,
        recipe: Option<Box<dyn safe_cex_engine::SignStep>>,
        read: ProofRead,
    ) {
        let token = read.token();
        let context = match cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(context) => context,
            Err(_) => {
                self.proof_schedule(cx);
                return;
            }
        };
        let pending = Pending::Proof {
            context: context.clone(),
            read,
        };
        let sent = match recipe {
            Some(recipe) => cx
                .request_signed_managed(context, None, Method::Get, url, None, recipe, pending)
                .map_err(drop),
            None => cx
                .request_managed(context, None, Method::Get, url, None, pending)
                .map_err(drop),
        };
        if sent.is_err() {
            self.proof_schedule(cx);
            return;
        }
        if let Some(proof) = self.proof_mut(token) {
            proof.inflight = true;
        }
    }

    /// The next read follows a short backoff, never past the bound.
    fn proof_schedule(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(proof) = self
            .trading
            .active
            .as_ref()
            .and_then(|job| job.proof.as_ref())
            .filter(|p| p.outcome.is_none())
        else {
            return;
        };
        let delay = POLL_MS[proof.polls.saturating_sub(1).min(POLL_MS.len() - 1)];
        if self.trading.proof_timer.is_none() && cx.now + delay < proof.deadline {
            self.trading.proof_timer = Some(cx.set_timer(delay));
        }
    }

    pub(crate) fn proof_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.trading.proof_timer != Some(timer) {
            return false;
        }
        self.trading.proof_timer = None;
        let job = self.trading.active.as_ref();
        if job
            .and_then(|job| job.proof.as_ref())
            .is_some_and(|p| p.outcome.is_some())
        {
            if let Some(wait) = job.and_then(|job| job.condition.as_ref()) {
                let _ = cx.release_condition_wait(wait);
            }
        } else {
            self.proof_poll(cx);
        }
        true
    }

    pub(crate) fn proof_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: ProofRead,
        response: Response<'_>,
    ) {
        let token = read.token();
        let Some(proof) = self.proof_mut(token) else {
            return;
        };
        proof.inflight = false;
        let ok = (200..300).contains(&response.status);
        match read {
            ProofRead::Orders { .. } => {
                let reply = serde_json::from_str::<OrdersReply>(response.body)
                    .ok()
                    .filter(|r| ok && r.code.as_ref().and_then(Number::integer) == Some(200));
                for row in reply.iter().flat_map(|r| &r.orders) {
                    let Some(id) = row.client_order_index.as_ref().and_then(Number::integer) else {
                        continue;
                    };
                    let id = id.to_string();
                    if proof.ids.contains(&id)
                        && !proof.verdicts.contains_key(&id)
                        && let Some(verdict) = verdict(row)
                    {
                        proof.verdicts.insert(id, verdict);
                    }
                }
                // A row that has not appeared may belong to a failed transaction.
                let missing = reply.is_some().then(|| {
                    proof.unresolved().into_iter().find_map(|id| {
                        let index = proof.ids.iter().position(|own| *own == id)?;
                        Some((id, proof.hashes.get(index)?.clone()))
                    })
                });
                if self.proof_conclude(cx, token) {
                    return;
                }
                if let Some(Some((id, hash))) = missing {
                    let url = format!("{}/api/v1/tx?by=hash&value={hash}", self.host());
                    self.proof_request(cx, url, None, None, ProofRead::Tx { token, id });
                    return;
                }
            }
            ProofRead::Tx { id, .. } => {
                if let Ok(reply) = serde_json::from_str::<TxReply>(response.body)
                    && ok
                    && reply.code.as_ref().and_then(Number::integer) == Some(200)
                    && reply.status.as_ref().and_then(Number::integer) == Some(0)
                {
                    proof
                        .verdicts
                        .insert(id, Verdict::Refused(tx_reason(&reply)));
                }
                if self.proof_conclude(cx, token) {
                    return;
                }
            }
        }
        self.proof_schedule(cx);
    }

    /// A reduce-only leg next may need a fresh account read. Only the parked
    /// proof can wait for it: a released settle has no call left to park on.
    fn proof_hold_for_position(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        let Some(job) = self.trading.active.as_mut() else {
            return false;
        };
        let command = job.command;
        let Some(Step::Position {
            symbol,
            side,
            requested,
        }) = job.steps.front_mut()
        else {
            return false;
        };
        if *requested || cx.frame_fed() || has_position(cx, symbol, *side) {
            return false;
        }
        *requested = true;
        self.refresh_account(cx);
        let Some(serial) = self.account_snapshot_serial() else {
            return false;
        };
        if self.trading.retired_snapshot == Some(serial) {
            return false;
        }
        self.trading.position_wait = Some(PositionWait {
            command,
            serial,
            confirmed: None,
        });
        true
    }

    pub(crate) fn proof_failed(&mut self, cx: &mut Cx<'_, Pending>, read: ProofRead) {
        let Some(proof) = self.proof_mut(read.token()) else {
            return;
        };
        proof.inflight = false;
        self.proof_schedule(cx);
    }

    /// Every placement proven: resolve or refuse the receipt and release the
    /// Plan. A mix of live and refused orders stays pending, hence Unknown.
    fn proof_conclude(&mut self, cx: &mut Cx<'_, Pending>, token: u64) -> bool {
        let Some(proof) = self.proof_mut(token) else {
            return true;
        };
        if !proof.unresolved().is_empty() {
            return false;
        }
        let refused: Vec<(String, String)> = proof
            .ids
            .iter()
            .filter_map(|id| match proof.verdicts.get(id) {
                Some(Verdict::Refused(reason)) => Some((id.clone(), reason.clone())),
                _ => None,
            })
            .collect();
        let ids = proof.ids.clone();
        let receipt = proof.receipt.take();
        let outcome = if refused.is_empty() {
            match receipt.map(|r| cx.resolve_placement_identity(r, ids.clone())) {
                Some(Ok(())) => Ok(()),
                _ => Err(uncertain("Invalid Lighter placement evidence")),
            }
        } else if refused.len() == ids.len() {
            let (_, native) = &refused[0];
            let reason = refusal_text(native).unwrap_or(native);
            let error = VenueError::new(format!("Lighter did not place the order: {reason}"))
                .kind(ErrorKind::Rejected)
                .outcome(Outcome::Rejected)
                .venue_message(native.clone());
            match receipt.map(|r| cx.reject_pending_placement(r, error.clone())) {
                Some(Ok(())) => Err(error),
                _ => Err(uncertain("Invalid Lighter placement evidence")),
            }
        } else {
            let listed: Vec<String> = refused
                .iter()
                .map(|(id, native)| format!("{id} ({native})"))
                .collect();
            Err(uncertain(format!(
                "Lighter refused some orders of this placement: {}",
                listed.join(", ")
            )))
        };
        if outcome.is_ok() {
            let job = self.trading.active.as_mut().unwrap();
            job.ids.extend(ids);
            job.effective = true;
        } else if refused.len() == ids.len() {
            for (id, _) in &refused {
                self.account.unnamed_placements.remove(id);
                self.record_cancel(id, cx.now);
                cx.store.remove_orders(&[id]);
            }
        }
        if let Some(timer) = self.trading.proof_timer.take() {
            cx.clear_timer(timer);
        }
        if outcome.is_err() {
            self.refresh_orders(cx);
        }
        let job = self.trading.active.as_mut().unwrap();
        let paced = outcome.is_ok() && !job.steps.is_empty();
        job.proof.as_mut().unwrap().outcome = Some(outcome);
        if paced && self.proof_hold_for_position(cx) {
            return true;
        }
        let job = self.trading.active.as_mut().unwrap();
        if paced {
            self.trading.proof_timer = Some(cx.set_timer(NEXT_WRITE_PACE_MS));
        } else if let Some(wait) = &job.condition {
            let _ = cx.release_condition_wait(wait);
        }
        true
    }
}
