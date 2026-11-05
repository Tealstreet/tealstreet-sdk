//! Native FIFO planning carries genuine engine owners; only their callbacks can issue work.
use super::*;
use safe_cex_engine::CredentialSlot;
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::request_context::{
    ConditionWait, Context, HttpOwnerRetirement, Plan, Preflight, PreflightPolicy,
    PreflightPurpose,
    plan::{CallAuth, SerialCall, SerialPolicy, Slot},
};
use safe_cex_engine::signing::{Algorithm, KeyUse, Need};

pub(crate) const PACING: safe_cex_engine::request_context::Pacing =
    safe_cex_engine::request_context::Pacing {
        group: 1,
        minimum_gap_ms: 334.0,
    };
fn call() -> SerialCall {
    SerialCall {
        auth: CallAuth::Preflight,
        pacing: None,
        operation: Operation::Mutation,
        timeout_ms: 20_000,
        attempts: 1,
    }
}
pub(super) struct Custody {
    preflight: Preflight,
    // Captured once from the genuine caller receipt, before queue or read waits.
    intent_deadline: f64,
    intent_timer: Option<TimerId>,
    intent_expired: bool,
    plan: Option<Plan>,
    slot: Option<Slot>,
    context: Option<Context>,
    ready: bool,
    wait: Option<ConditionWait>,
}
impl Hyperliquid {
    pub(crate) fn sealed_agent(&self) -> Option<String> {
        let (_, identity) = self.credentials.select(CredentialRole::Mutation).ok()?;
        let address = identity.verified_signing_address().ok()?;
        Some(format!("0x{}", safe_cex_crypto::hex(&address)))
    }
    pub(super) fn enqueue_trade(&mut self, cx: &mut Cx<'_, Pending>, mut job: Job) {
        if self.trading.queue.len() >= 256 {
            cx.refuse_command(
                Operation::Mutation,
                safe_cex_engine::request_context::Error::Capacity.venue_error(),
            );
            return;
        }
        let selected = self.credentials.select(CredentialRole::Mutation);
        let authority = match selected {
            Ok((selection, identity)) if !self.options.public_data_only => {
                if let Err(error) = identity.verified_signing_address() {
                    cx.refuse_command(Operation::Mutation, error.venue_error());
                    return;
                }
                selection.authority(identity)
            }
            _ => {
                cx.refuse_command(
                    Operation::Mutation,
                    not_sent("Hyperliquid signing key is missing").kind(ErrorKind::AuthInvalid),
                );
                return;
            }
        };
        if let Err(error) = cx.can_sign(
            CredentialSlot::Primary,
            &[Need {
                key: KeyUse::Signing,
                algorithm: Algorithm::Secp256k1Digest,
            }],
        ) {
            cx.refuse_command(Operation::Mutation, error.venue_error());
            return;
        }
        let remaining = match cx.command_wait() {
            Some(safe_cex_engine::CommandWait::Remaining(ms)) => ms,
            Some(safe_cex_engine::CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Mutation);
                return;
            }
            None => {
                cx.refuse_command(
                    Operation::Mutation,
                    not_sent("Hyperliquid command has no caller deadline")
                        .kind(ErrorKind::Internal),
                );
                return;
            }
        };
        let intent_deadline = cx.now + f64::from(remaining);
        let preflight = match cx.begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: None,
            write_credential: Some(authority),
        }) {
            Ok(owner) => owner,
            Err(_) => return,
        };
        let ready = self.trading.active.is_none() && self.trading.queue.is_empty();
        let intent_timer = cx.set_timer(f64::from(remaining));
        if !ready && let Err(error) = cx.park_preflight_for(&preflight, None) {
            cx.clear_timer(intent_timer);
            let _ = cx.fail_preflight(&preflight, error.venue_error());
            return;
        }
        job.custody = Some(Custody {
            preflight,
            intent_deadline,
            intent_timer: Some(intent_timer),
            intent_expired: false,
            plan: None,
            slot: None,
            context: None,
            ready,
            wait: None,
        });
        self.trading.queue.push_back(job);
        self.pump_trades(cx);
    }
    pub(crate) fn trade_preflight_ready(&mut self, cx: &mut Cx<'_, Pending>, owner: &Preflight) {
        if self.trading.active.is_none() {
            self.trading.active = self.take_next_trade();
        }
        let Some(custody) = self
            .trading
            .active
            .as_mut()
            .and_then(|job| job.custody.as_mut())
        else {
            return;
        };
        if custody.preflight.owner() != owner.owner() || custody.plan.is_some() {
            return;
        }
        custody.ready = true;
        self.pump_trades(cx);
    }
    pub(crate) fn trade_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, owner: &Plan) {
        let Some(custody) = self
            .trading
            .active
            .as_mut()
            .and_then(|job| job.custody.as_mut())
        else {
            return;
        };
        if custody
            .plan
            .as_ref()
            .is_none_or(|plan| plan.owner() != owner.owner())
        {
            return;
        }
        match cx.plan_slots(owner).and_then(|slots| {
            slots
                .into_iter()
                .next()
                .ok_or(safe_cex_engine::request_context::Error::State)
        }) {
            Ok(slot) => custody.slot = Some(slot),
            Err(error) => {
                self.finish_trade(cx, Err(error.venue_error()));
                return;
            }
        }
        self.pump_trades(cx);
    }
    pub(super) fn trade_entry_ready(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        if self.trading.active.as_ref().is_some_and(|job| {
            !job.effective
                && job.custody.as_ref().is_some_and(|custody| {
                    custody.intent_expired || cx.now >= custody.intent_deadline
                })
        }) {
            self.finish_trade(cx, Err(intent_expired()));
            return false;
        }
        let custody = self
            .trading
            .active
            .as_ref()
            .unwrap()
            .custody
            .as_ref()
            .unwrap();
        if custody.wait.is_some() {
            return false;
        }
        if custody.plan.is_none() && !custody.ready {
            if let Err(error) = cx.request_preflight_ready(&custody.preflight) {
                self.finish_trade(cx, Err(error.venue_error()));
            }
            return false;
        }
        true
    }
    pub(super) fn park_trade_builder(
        &mut self,
        cx: &mut Cx<'_, Pending>,
    ) -> Result<(), VenueError> {
        let job = self.trading.active.as_ref().unwrap();
        if !job.effective {
            let remaining = (job.custody.as_ref().unwrap().intent_deadline - cx.now).floor();
            if remaining < 1. || job.custody.as_ref().unwrap().intent_expired {
                return Err(intent_expired());
            }
        }
        let custody = self
            .trading
            .active
            .as_mut()
            .unwrap()
            .custody
            .as_mut()
            .unwrap();
        if custody.plan.is_none() && custody.ready {
            cx.park_preflight_for(&custody.preflight, None)
                .map_err(|e| e.venue_error())?;
            custody.ready = false;
        }
        if let (Some(plan), Some(slot), Some(context)) =
            (&custody.plan, &custody.slot, &custody.context)
        {
            custody.wait = Some(
                cx.serial_plan_condition_wait(plan, slot, context, call(), None)
                    .map_err(|e| e.venue_error())?,
            );
            custody.context = None;
            custody.slot = None;
        }
        Ok(())
    }
    pub(super) fn release_trade_builder(&mut self, cx: &mut Cx<'_, Pending>) {
        let wait = self
            .trading
            .active
            .as_mut()
            .and_then(|job| job.custody.as_mut())
            .and_then(|custody| custody.wait.take());
        if let Some(wait) = wait {
            if let Err(error) = cx.release_condition_wait(&wait) {
                self.finish_trade(cx, Err(error.venue_error()));
            }
        } else {
            self.pump_trades(cx);
        }
    }
    pub(super) fn submit_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        action: &Action,
        effect: Effect,
    ) -> Result<(), VenueError> {
        let (_, identity, claim) = self
            .credentials
            .select_with_nonce(
                CredentialRole::Mutation,
                &safe_cex_engine::nonce::HYPERLIQUID_AGENT,
            )
            .map_err(|e| e.sign_error().venue_error())?;
        let recipe = sign::dispatch::Exchange::new(
            identity,
            claim,
            action,
            // As legacy, an empty saved vault is no vault.
            self.options.vault.clone().filter(|vault| !vault.is_empty()),
            self.options.testnet,
        )
        .map_err(|e| e.venue_error())?;
        let url = recipe.url().to_owned();
        let authority = self
            .credentials
            .select(CredentialRole::Mutation)
            .map(|(selection, identity)| selection.authority(identity))
            .map_err(|_| {
                not_sent("Hyperliquid signing key is missing").kind(ErrorKind::AuthInvalid)
            })?;
        let spot = self.spot
            || matches!(&effect, Effect::Place(records)
            if records.first().is_some_and(|record| self.spot_market(cx, &record.order.symbol)));
        let order_reply = OrderReply::for_action(action, spot);
        let job = self.trading.active.as_mut().unwrap();
        let custody = job.custody.as_mut().unwrap();
        match custody.plan.as_ref() {
            None => {
                let (plan, slot) = cx
                    .admit_serial_plan(
                        &custody.preflight,
                        call(),
                        SerialPolicy {
                            credential: Some(authority),
                            pacing: None,
                            refusals_complete: false,
                        },
                    )
                    .map_err(|e| e.venue_error())?;
                custody.plan = Some(plan);
                custody.slot = Some(slot);
            }
            Some(plan) => {
                if let Some(context) = custody.context.take() {
                    custody.slot = Some(
                        cx.continue_serial_plan(
                            plan,
                            custody.slot.as_ref().unwrap(),
                            &context,
                            call(),
                        )
                        .map_err(|e| e.venue_error())?,
                    );
                }
            }
        }
        let plan = custody.plan.as_ref().unwrap().clone();
        let slot = custody.slot.as_ref().unwrap().clone();
        let context = cx
            .plan_leg_context_with_intent(
                &plan,
                &slot,
                match &effect {
                    Effect::Place(_) if order_reply == OrderReply::FullPosition => {
                        safe_cex_engine::classify::Intent::Setting
                    }
                    Effect::ScheduleCancel | Effect::Leverage { .. } => {
                        safe_cex_engine::classify::Intent::Setting
                    }
                    Effect::Cancel(_) if job.amend_source.is_none() => {
                        safe_cex_engine::classify::Intent::Cancel
                    }
                    Effect::Cancel(_) | Effect::Place(_) if job.amend_source.is_some() => {
                        safe_cex_engine::classify::Intent::Amend
                    }
                    _ => safe_cex_engine::classify::Intent::Place,
                },
            )
            .map_err(|e| e.venue_error())?;
        custody.context = Some(context.clone());
        let committed_effect = effect.clone();
        let command = job.command;
        let intent_deadline = (!job.effective).then_some(custody.intent_deadline);
        self.publish_trade_receipts(cx, command, &committed_effect);
        let pending = Pending::Trade(Request {
            command,
            effect,
            plan,
            context: context.clone(),
            order_reply,
            intent_deadline,
        });
        match cx.request_signed_managed(
            context,
            None,
            Method::Post,
            url,
            None,
            Box::new(recipe),
            pending,
        ) {
            Ok(_) => {
                self.trading.inflight = true;
                self.remember_trade_effect(cx, command, &committed_effect);
                Ok(())
            }
            Err((Pending::Trade(request), error)) => {
                self.restore_cancel(cx, &request.effect);
                Err(error.venue_error())
            }
            Err(_) => unreachable!("trade pending retained"),
        }
    }
    pub(super) fn trade_valid(&self, cx: &Cx<'_, Pending>, request: &Request) -> bool {
        self.trading.active.as_ref().is_some_and(|job| {
            job.command == request.command
                && job.custody.as_ref().and_then(|c| c.context.as_ref()) == Some(&request.context)
        }) && cx
            .validate_plan_delivery(&request.plan, &request.context)
            .is_ok()
    }
    pub(super) fn finish_trade_owner(
        &self,
        cx: &mut Cx<'_, Pending>,
        job: &Job,
        result: Result<String, VenueError>,
    ) {
        let custody = job.custody.as_ref().unwrap();
        if let Some(timer) = custody.intent_timer {
            cx.clear_timer(timer);
        }
        if let Some(plan) = &custody.plan {
            match result {
                Err(error) => {
                    let _ = match &job.stop_note {
                        Some(note) => cx.fail_plan_noted(plan, error, note),
                        None => cx.fail_plan(plan, error),
                    };
                }
                Ok(data) => {
                    if let (Some(slot), Some(context)) = (&custody.slot, &custody.context)
                        && let Err(error) = cx.finish_plan_call(plan, slot, context)
                    {
                        let _ = cx.fail_plan(plan, error.venue_error());
                        return;
                    }
                    let _ = cx.done_plan(plan, data);
                }
            }
        } else {
            let _ = match result {
                Err(error) => cx.fail_preflight(&custody.preflight, error),
                Ok(data) => cx.done_empty_plan(&custody.preflight, data),
            };
        }
    }
    pub(crate) fn check_trade_intent(
        &self,
        request: &Request,
        now: f64,
    ) -> Result<(), safe_cex_engine::request_context::Error> {
        request.check_intent_deadline(now)?;
        if request.intent_deadline.is_some()
            && self.trading.active.as_ref().is_some_and(|job| {
                job.command == request.command
                    && job.custody.as_ref().is_some_and(|custody| {
                        custody.context.as_ref() == Some(&request.context) && custody.intent_expired
                    })
            })
        {
            return Err(safe_cex_engine::request_context::Error::TimedOut);
        }
        Ok(())
    }
    pub(super) fn expire_trade_intent(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some(index) = self.trading.queue.iter().position(|job| {
            job.custody
                .as_ref()
                .is_some_and(|custody| custody.intent_timer == Some(timer))
        }) {
            let mut job = self.trading.queue.remove(index).unwrap();
            job.custody.as_mut().unwrap().intent_timer = None;
            self.finish_trade_owner(cx, &job, Err(intent_expired()));
            return true;
        }
        let Some(job) = self.trading.active.as_mut().filter(|job| {
            job.custody
                .as_ref()
                .is_some_and(|custody| custody.intent_timer == Some(timer))
        }) else {
            return false;
        };
        let custody = job.custody.as_mut().unwrap();
        custody.intent_timer = None;
        custody.intent_expired = true;
        if !job.effective && !self.trading.inflight {
            self.finish_trade(cx, Err(intent_expired()));
        }
        true
    }
    pub(crate) fn trade_owner_retired(
        &mut self,
        retired: &HttpOwnerRetirement,
    ) -> Option<Preflight> {
        self.trading.queue.retain(|job| {
            !job.custody
                .as_ref()
                .is_some_and(|c| retired.matches_preflight(&c.preflight))
        });
        if self
            .trading
            .active
            .as_ref()
            .and_then(|job| job.custody.as_ref())
            .is_some_and(|c| retired.matches_preflight(&c.preflight))
        {
            self.trading.active = None;
            self.trading.inflight = false;
            self.trading.builder_wait = None;
            self.trading.amend_wait = None;
        }
        if self.trading.active.is_none() {
            self.trading.active = self.take_next_trade();
        }
        self.trading
            .active
            .as_ref()
            .and_then(|job| job.custody.as_ref())
            .filter(|c| c.plan.is_none())
            .map(|c| c.preflight.clone())
    }
}

const NATIVE_ERROR: safe_cex_engine::classify::CodeTable = safe_cex_engine::classify::CodeTable {
    rows: &[
        refusal(WALLET_MISSING, ErrorKind::AuthInvalid),
        refusal(
            "Cannot switch leverage type with open position.",
            ErrorKind::Rejected,
        ),
        refusal(SCHEDULE_VOLUME, ErrorKind::Rejected),
    ],
    success: &[],
    unlisted: safe_cex_engine::classify::Verdict::Unknown(None),
    status: &[],
};
const WALLET_MISSING: &str = "User or API Wallet does not exist.";
const SCHEDULE_VOLUME: &str = "Cannot set scheduled cancel time until enough volume traded";
const POST_ONLY: &str = "Post only order would have immediately matched";
const fn refusal(message: &'static str, kind: ErrorKind) -> safe_cex_engine::classify::Row {
    safe_cex_engine::classify::Row {
        key: safe_cex_engine::classify::Key::Message(message),
        verdict: safe_cex_engine::classify::Verdict::Refused(kind),
        only: None,
    }
}
const fn place_refusal(message: &'static str) -> safe_cex_engine::classify::Row {
    safe_cex_engine::classify::Row {
        key: safe_cex_engine::classify::Key::Message(message),
        verdict: safe_cex_engine::classify::Verdict::Refused(ErrorKind::Rejected),
        only: Some(safe_cex_engine::classify::Intent::Place),
    }
}
/// The fixed text of a venue message: it appends " asset=N" and embeds live values.
fn table_message(message: &str) -> &str {
    let message = message
        .split_once(" asset=")
        .map_or(message, |(text, _)| text);
    for fixed in [POST_ONLY, SCHEDULE_VOLUME] {
        if message.starts_with(fixed) {
            return fixed;
        }
    }
    if message.starts_with("User or API Wallet ") && message.ends_with(" does not exist.") {
        return WALLET_MISSING;
    }
    message
}
// Official Error responses / PerpMargin is a definite per-row rejection.
// https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/error-responses
const BATCH: safe_cex_engine::classify::CodeTable = safe_cex_engine::classify::CodeTable {
    rows: &[
        place_refusal("Insufficient margin to place order."),
        place_refusal("Post only order would have immediately matched"),
        place_refusal("Reduce only order would increase position."),
        place_refusal("Order must have minimum value of $10."),
        place_refusal("Order could not immediately match against any resting orders."),
        place_refusal("Order has invalid price."),
        place_refusal("Order has invalid size."),
        place_refusal("Order has zero size."),
        place_refusal("Builder fee has not been approved."),
        place_refusal("Reduce-only is invalid for spot trading."),
    ],
    success: &[],
    unlisted: safe_cex_engine::classify::Verdict::Unknown(Some(ErrorKind::Rejected)),
    status: &[],
};
impl Request {
    pub(crate) fn reconciliation(
        &self,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        let entries: Vec<_> = match &self.effect {
            Effect::Place(rows) => rows
                .iter()
                .map(|row| (row.order.symbol.clone(), row.order.client_order_id.clone()))
                .collect(),
            Effect::Cancel(rows) => rows
                .iter()
                .map(|row| (row.symbol.clone(), Some(row.id.clone())))
                .collect(),
            Effect::Leverage { symbol, .. } => vec![(symbol.clone(), None)],
            Effect::ScheduleCancel => vec![],
        };
        safe_cex_engine::request_context::ReconciliationData::new(
            serde_json::to_string(&entries)
                .map_err(|_| safe_cex_engine::request_context::Error::Invalid)?,
        )
    }
}
impl Hyperliquid {
    pub(crate) fn trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
    ) {
        if self
            .trading
            .active
            .as_ref()
            .and_then(|job| job.custody.as_ref())
            .and_then(|c| c.context.as_ref())
            != Some(&request.context)
        {
            return;
        }
        self.restore_cancel(cx, &request.effect);
        if error.outcome.is_none()
            && matches!(error.kind, Some(ErrorKind::Network | ErrorKind::Timeout))
        {
            error.outcome = Some(Outcome::Unknown);
        }
        if error.outcome == Some(Outcome::Unknown) {
            self.mark_settings_uncertain(cx, &request.effect);
            self.schedule_orders(cx);
        } else if let Effect::Leverage { symbol, .. } = &request.effect {
            self.settings_reconciled(symbol);
        }
        self.finish_trade(cx, Err(error));
    }
    pub(crate) fn unproven_trade_evidence(
        &self,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        if let Ok(reply) = serde_json::from_str::<Reply<'_>>(response.body)
            && reply.status == "err"
            && let Ok(message) = serde_json::from_str::<String>(reply.response.get())
        {
            evidence.venue_message(&message);
        }
    }
    pub(super) fn report_trade_failure(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        error: &ReplyError,
        native_message: Option<&str>,
    ) -> Result<(), safe_cex_engine::request_context::Error> {
        let native = safe_cex_engine::classify::Native {
            code: safe_cex_engine::classify::NativeCode::None,
            message: table_message(native_message.unwrap_or(&error.message)),
        };
        cx.http_write_failure(
            &request.context,
            &NATIVE_ERROR,
            Some(&native),
            error.message.clone(),
            None,
        )
        .map(drop)
    }
    pub(super) fn report_trade_batch(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        statuses: &[Status],
        message: &str,
        idempotent_cancel: bool,
    ) -> Result<(), safe_cex_engine::request_context::Error> {
        use safe_cex_engine::classify::{BatchItem, Native, NativeCode, WriteFact};
        if let Effect::Leverage { .. } | Effect::ScheduleCancel = &request.effect {
            return cx
                .http_write_fact(
                    &request.context,
                    WriteFact::Accepted { order_ids: vec![] },
                    None,
                )
                .map(drop);
        }
        if request.order_reply == OrderReply::FullPosition
            && matches!(statuses, [Status::Text(text)] if text == "waitingForTrigger")
        {
            return cx
                .http_write_fact(
                    &request.context,
                    WriteFact::Accepted { order_ids: vec![] },
                    None,
                )
                .map(drop);
        }
        if request.order_reply == OrderReply::Attached
            && matches!(
                statuses.first(),
                Some(Status::Resting { .. } | Status::Filled { .. })
            )
            && statuses
                .iter()
                .skip(1)
                .any(|status| matches!(status, Status::Text(text) if deferred_child(text)))
        {
            return self.report_attached_order(cx, request, statuses, message);
        }
        let ids: Vec<_> = statuses
            .iter()
            .enumerate()
            .map(|(index, status)| match status {
                Status::Resting { resting } => resting.oid.to_string(),
                Status::Filled { filled } => filled.oid.to_string(),
                _ => match &request.effect {
                    Effect::Cancel(orders) => orders[index].id.clone(),
                    _ => String::new(),
                },
            })
            .collect();
        let items: Vec<_> = statuses
            .iter()
            .enumerate()
            .map(|(index, status)| match status {
                Status::Error { error } if !(idempotent_cancel && only_terminal_cancel(error)) => {
                    BatchItem::Native(Native {
                        code: NativeCode::None,
                        message: table_message(error),
                    })
                }
                Status::Text(text) => match &request.effect {
                    Effect::Cancel(_) => BatchItem::Accepted(ids[index].as_str()),
                    _ => BatchItem::Native(Native {
                        code: NativeCode::None,
                        message: if text == "waitingForFill" || text == "waitingForTrigger" {
                            "Hyperliquid pending order acknowledgement has no native order ID"
                        } else {
                            "Unknown Hyperliquid order status"
                        },
                    }),
                },
                _ => BatchItem::Accepted(ids[index].as_str()),
            })
            .collect();
        cx.http_write_batch(
            &request.context,
            &BATCH,
            statuses.len(),
            &items,
            if message.is_empty() && statuses.iter().any(|status| matches!(status, Status::Text(_))) {
                if statuses.iter().any(|status| matches!(status, Status::Text(text) if text != "waitingForFill" && text != "waitingForTrigger")) {
                    "Unknown Hyperliquid order status"
                } else { "Hyperliquid pending order acknowledgement has no native order ID" }
            } else { message },
            None,
        )
        .map(drop)
    }

    fn report_attached_order(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        statuses: &[Status],
        message: &str,
    ) -> Result<(), safe_cex_engine::request_context::Error> {
        use safe_cex_engine::classify::{Native, NativeCode, Verdict, WriteFact};
        let ids: Vec<_> = statuses
            .iter()
            .filter_map(|status| match status {
                Status::Resting { resting } => Some(resting.oid.to_string()),
                Status::Filled { filled } => Some(filled.oid.to_string()),
                _ => None,
            })
            .collect();
        let mut failure = None;
        for (index, status) in statuses.iter().enumerate() {
            let error = match status {
                Status::Error { error } => error.as_str(),
                Status::Text(text) if index > 0 && deferred_child(text) => continue,
                Status::Text(_) => "Unknown Hyperliquid order status",
                _ => continue,
            };
            let native = Native {
                code: NativeCode::None,
                message: table_message(error),
            };
            let fact = cx.classify_write(
                &request.context,
                &BATCH,
                Some(&native),
                if message.is_empty() { error } else { message },
            )?;
            let unknown = matches!(fact.verdict(), Verdict::Unknown(_));
            if failure.is_none() || unknown {
                failure = Some(fact);
            }
            if unknown {
                break;
            }
        }
        let fact = match failure {
            Some(fact) => WriteFact::Failed {
                fact: fact.with_order_ids(&ids),
                known_effects: true,
            },
            None => WriteFact::Accepted { order_ids: ids },
        };
        cx.http_write_fact(&request.context, fact, None).map(drop)
    }
}

/// A child still waiting on its parent: an unfilled parent's child says
/// waitingForFill, a filled parent's says waitingForTrigger.
fn deferred_child(text: &str) -> bool {
    text == "waitingForFill" || text == "waitingForTrigger"
}
