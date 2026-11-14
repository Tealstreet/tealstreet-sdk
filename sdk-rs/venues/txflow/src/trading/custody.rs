//! The same engine-owned preflight and serial-plan boundary used by HL.
use super::*;
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::request_context::{
    Operation, Policy, Preflight, PreflightPolicy, PreflightPurpose, RetryPolicy,
    plan::{CallAuth, CallTerms, RetryWindow, SerialCall, SerialPolicy, Slot},
};
pub(super) struct Custody {
    preflight: Preflight,
    ready: bool,
    pub(super) wait: Option<safe_cex_engine::request_context::ConditionWait>,
    pub(super) pending_identity: Option<safe_cex_engine::request_context::PendingPlacement>,
    authorized: bool,
    checking: bool,
    pub(super) plan: Option<Plan>,
    pub(super) slot: Option<Slot>,
    pub(super) context: Option<Context>,
}
pub(crate) const PACING: safe_cex_engine::request_context::Pacing =
    safe_cex_engine::request_context::Pacing {
        group: 1,
        minimum_gap_ms: 110.0,
    };

pub(super) fn call() -> SerialCall {
    SerialCall {
        auth: CallAuth::Preflight,
        pacing: None,
        operation: Operation::Mutation,
        timeout_ms: 20_000,
        attempts: 1,
    }
}
impl TxFlow {
    pub(crate) fn trade_owner_retired(
        &mut self,
        retired: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) -> Option<Preflight> {
        self.trading
            .queue
            .retain(|job| !retired.matches_preflight(&job.custody.preflight));
        if self
            .trading
            .active
            .as_ref()
            .is_some_and(|job| retired.matches_preflight(&job.custody.preflight))
        {
            self.trading.active = self.trading.queue.pop_front();
            self.trading.inflight = false;
            self.trading.reconcile = None;
            self.trading.reconcile_count = 0;
        }
        self.trading
            .active
            .as_ref()
            .filter(|job| job.custody.plan.is_none())
            .map(|job| job.custody.preflight.clone())
    }
    pub(super) fn admit_trade(&self, cx: &mut Cx<'_, Pending>) -> Option<Custody> {
        if self.trading.queue.len() >= 256 {
            cx.refuse_command(
                Operation::Mutation,
                not_sent("TxFlow command queue is full"),
            );
            return None;
        }
        let authority = match self.credentials.select(CredentialRole::Mutation) {
            Ok((selection, identity)) if identity.verified_signing_address().is_ok() => {
                selection.authority(identity)
            }
            _ => {
                cx.refuse_command(
                    Operation::Mutation,
                    not_sent("TxFlow agent signing provider is required"),
                );
                return None;
            }
        };
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(authority),
            })
            .ok()?;
        let ready = self.trading.active.is_none() && self.trading.queue.is_empty();
        if !ready && cx.park_preflight_for(&preflight, None).is_err() {
            return None;
        }
        Some(Custody {
            preflight,
            ready,
            wait: None,
            pending_identity: None,
            authorized: false,
            checking: false,
            plan: None,
            slot: None,
            context: None,
        })
    }
    pub(super) fn trade_ready(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        let Some(job) = self.trading.active.as_mut() else {
            self.finish_trade(cx, Err(not_sent("TxFlow active trade is missing")));
            return false;
        };
        let custody = &mut job.custody;
        if custody.wait.is_some() {
            return false;
        }
        if custody.plan.is_none() && !custody.ready {
            let _ = cx.request_preflight_ready(&custody.preflight);
            return false;
        }
        if custody.authorized {
            return true;
        }
        if custody.checking {
            return false;
        }
        let context = match cx.preflight_read_context(
            &custody.preflight,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
            CallTerms {
                wait_ms: 0,
                timeout_ms: 20_000,
                attempts: 1,
                retry_window: RetryWindow::FreshAttempt,
            },
        ) {
            Ok(context) => context,
            Err(error) => {
                self.finish_trade(cx, Err(error.venue_error()));
                return false;
            }
        };
        custody.checking = true;
        let body = self.approval_body();
        if let Err((_, error)) = cx.request_managed_with_headers(
            context.clone(),
            None,
            Method::Post,
            format!("{}/info", self.rest()),
            &[
                ("Content-Type", "application/json"),
                ("x-api-version", "2026-08-16"),
            ],
            Some(body),
            Pending::Agent(crate::agent::Request { context }),
        ) {
            self.finish_trade(cx, Err(error.venue_error()));
        }
        false
    }
    pub(crate) fn trade_agent_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        result: Result<(), VenueError>,
    ) {
        let Some(job) = &mut self.trading.active else {
            return;
        };
        if cx
            .finish_preflight_call(&job.custody.preflight, &context)
            .is_err()
        {
            return;
        }
        job.custody.checking = false;
        match result {
            Ok(()) => {
                job.custody.authorized = true;
                self.pump_trades(cx);
            }
            Err(error) => self.finish_trade(cx, Err(error)),
        }
    }
    pub(crate) fn trade_preflight_ready(&mut self, cx: &mut Cx<'_, Pending>, owner: &Preflight) {
        if let Some(job) = &mut self.trading.active
            && job.custody.preflight.owner() == owner.owner()
        {
            job.custody.ready = true;
            self.pump_trades(cx);
        }
    }
    pub(crate) fn trade_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, owner: &Plan) {
        let Some(job) = &mut self.trading.active else {
            return;
        };
        if job
            .custody
            .plan
            .as_ref()
            .is_none_or(|plan| plan.owner() != owner.owner())
        {
            return;
        }
        if job.steps.is_empty() && job.waiting.is_empty() {
            self.finish_trade(cx, Ok(()));
            return;
        }
        let next = if let Some(context) = job.custody.context.take() {
            cx.continue_settled_serial_plan(owner, &context, call())
        } else {
            cx.plan_slots(owner).and_then(|slots| {
                slots
                    .into_iter()
                    .next()
                    .ok_or(safe_cex_engine::request_context::Error::State)
            })
        };
        match next {
            Ok(slot) => job.custody.slot = Some(slot),
            Err(error) => {
                self.finish_trade(cx, Err(error.venue_error()));
                return;
            }
        }
        self.pump_trades(cx);
    }
    pub(super) fn submit_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        action: &Action,
        effect: Effect,
        response_type: &'static str,
    ) -> Result<(), VenueError> {
        let (selection, identity, claim) = self
            .credentials
            .select_with_nonce(
                CredentialRole::Mutation,
                &safe_cex_engine::nonce::HYPERLIQUID_AGENT,
            )
            .map_err(|e| e.sign_error().venue_error())?;
        let recipe = sign::dispatch::Exchange::new(
            identity,
            claim,
            &TypedAction {
                r#type: action.kind().0,
                body: action,
            },
            self.options.vault.clone(),
            self.options
                .application_id
                .clone()
                .ok_or_else(|| not_sent("TxFlow owner address is missing"))?,
        )
        .map_err(|e| e.venue_error())?;
        let authority = selection.authority(identity);
        let job = self
            .trading
            .active
            .as_mut()
            .ok_or_else(|| not_sent("TxFlow active trade is missing"))?;
        let custody = &mut job.custody;
        if let Some(plan) = &custody.plan {
            if let Some(context) = custody.context.take() {
                custody.slot = Some(
                    cx.continue_serial_plan(
                        plan,
                        custody
                            .slot
                            .as_ref()
                            .ok_or_else(|| unknown("TxFlow trade slot is missing"))?,
                        &context,
                        call(),
                    )
                    .map_err(|e| e.venue_error())?,
                );
            }
        } else {
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
        let plan = custody
            .plan
            .as_ref()
            .ok_or_else(|| unknown("TxFlow trade plan is missing"))?
            .clone();
        let context = cx
            .plan_leg_context(
                &plan,
                custody
                    .slot
                    .as_ref()
                    .ok_or_else(|| unknown("TxFlow trade slot is missing"))?,
            )
            .map_err(|e| e.venue_error())?;
        custody.context = Some(context.clone());
        cx.request_signed_managed(
            context.clone(),
            None,
            Method::Post,
            recipe.url().into(),
            None,
            Box::new(recipe),
            Pending::Trade(Request {
                command: job.command,
                effect,
                response_type,
                plan,
                context,
            }),
        )
        .map(|_| ())
        .map_err(|(_, e)| e.venue_error())
    }
    pub(super) fn finish_trade_owner(
        &self,
        cx: &mut Cx<'_, Pending>,
        job: &Job,
        result: Result<String, VenueError>,
    ) {
        let c = &job.custody;
        if let Some(plan) = &c.plan {
            match result {
                Err(e) => {
                    let _ = cx.fail_plan(plan, e);
                }
                Ok(data) => {
                    if let (Some(slot), Some(context)) = (&c.slot, &c.context)
                        && let Err(e) = cx.finish_plan_call(plan, slot, context)
                    {
                        let _ = cx.fail_plan(plan, e.venue_error());
                        return;
                    }
                    let _ = cx.done_plan(plan, data);
                }
            }
        } else {
            match result {
                Err(e) => {
                    let _ = cx.fail_preflight(&c.preflight, e);
                }
                Ok(data) => {
                    let _ = cx.done_empty_plan(&c.preflight, data);
                }
            }
        }
    }
}
impl Request {
    pub(crate) fn reconciliation(
        &self,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        let identity: Vec<_> = match &self.effect {
            Effect::Place(rows) => rows
                .iter()
                .map(|(cloid, previous_id)| (previous_id.clone(), Some(cloid.clone())))
                .collect(),
            Effect::Amend { id, .. } => vec![(None, Some(id.clone()))],
            _ => vec![],
        };
        safe_cex_engine::request_context::ReconciliationData::new(
            serde_json::to_string(&identity)
                .map_err(|_| safe_cex_engine::request_context::Error::Invalid)?,
        )
    }
}
