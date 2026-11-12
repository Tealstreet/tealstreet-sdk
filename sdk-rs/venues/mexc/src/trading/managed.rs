//! Protective FIFO custody and physical-release safety use the engine journal.
use super::*;
use safe_cex_engine::classify::{Intent, WriteFact};
use safe_cex_engine::credential_selection::{CredentialRole, Slots};
use safe_cex_engine::request_context::plan::{CallAuth, SerialCall, SerialPolicy, Slot};
use safe_cex_engine::request_context::{
    Context, Error, Plan, Preflight, PreflightPolicy, PreflightPurpose,
};
use safe_cex_engine::signing::Identity;

pub(super) enum OwnerError {
    MissingCredential,
    Entry,
}
pub struct Run {
    preflight: Preflight,
    ready: bool,
    plan: Option<Plan>,
    slot: Option<Slot>,
    current: Option<(Slot, Context)>,
    read: Identity,
    write: Identity,
    write_credential: safe_cex_engine::request_context::Credential,
}
pub struct ManagedRequest {
    pub native: Request,
    pub plan: Plan,
    pub slot: Slot,
    pub context: Context,
}
fn terms(operation: Operation) -> SerialCall {
    SerialCall {
        auth: CallAuth::Preflight,
        operation,
        timeout_ms: 20_000,
        attempts: 1,
        pacing: None,
    }
}
fn single_row(payload: &str) -> Option<ResultRow<'_>> {
    match serde_json::from_str::<ResultRows<'_>>(payload).ok()? {
        ResultRows::One(row) => Some(row),
        ResultRows::Many(mut rows) if rows.len() == 1 => rows.pop(),
        _ => None,
    }
}
fn accepted_place_id(payload: &str) -> Option<String> {
    let id = result_id(payload).ok()?;
    if id == "0" || id.chars().any(char::is_whitespace) {
        return None;
    }
    if payload.trim_start().starts_with(['{', '[']) {
        let row = single_row(payload)?;
        if row.error_code != 0.
            || [&row.id, &row.order_id]
                .iter()
                .any(|actual| !matches!(actual.as_ref(), "" | "0") && actual.as_ref() != id)
        {
            return None;
        }
    }
    Some(id)
}
fn accepted_native(payload: Option<&str>, id: &str) -> bool {
    let Some(payload) = payload else { return true };
    if payload.trim() == "null" {
        return true;
    }
    single_row(payload).is_some_and(|row| {
        row.error_code == 0.
            && [&row.id, &row.order_id]
                .iter()
                .all(|actual| matches!(actual.as_ref(), "" | "0") || actual.as_ref() == id)
    })
}
impl MexcFutures {
    pub(super) fn protective_owner(
        &self,
        cx: &mut Cx<'_, Pending>,
        parked: bool,
    ) -> Result<Run, OwnerError> {
        let slots = Slots::new(
            self.credentials.clone(),
            self.options.identities.secondary.clone(),
            Default::default(),
        );
        let (r, read) = slots
            .select(CredentialRole::Read { scope: None })
            .map_err(|_| OwnerError::MissingCredential)?;
        let (w, write) = slots
            .select(CredentialRole::Mutation)
            .map_err(|_| OwnerError::MissingCredential)?;
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: Some(r.authority(read)),
                write_credential: Some(w.authority(write)),
            })
            .map_err(|_| OwnerError::Entry)?;
        if parked && let Err(error) = cx.park_preflight(&preflight) {
            let _ = cx.fail_preflight(&preflight, error.venue_error());
            return Err(OwnerError::Entry);
        }
        Ok(Run {
            preflight,
            ready: !parked,
            plan: None,
            slot: None,
            current: None,
            read: read.clone(),
            write: write.clone(),
            write_credential: w.authority(write),
        })
    }
    pub(super) fn protective_fifo_ready(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        let Some(run) = self
            .trading
            .active
            .as_ref()
            .and_then(|job| job.managed.as_ref())
        else {
            return true;
        };
        if run.ready {
            return true;
        }
        let _ = cx.request_preflight_ready(&run.preflight);
        false
    }
    pub(crate) fn protective_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &Preflight,
    ) {
        let Some(run) = self
            .trading
            .active
            .as_mut()
            .and_then(|job| job.managed.as_mut())
            .filter(|run| run.preflight.owner() == preflight.owner())
        else {
            return;
        };
        run.ready = true;
        self.advance_trade(cx);
    }
    pub(crate) fn protective_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        let Some(run) = self
            .trading
            .active
            .as_mut()
            .and_then(|job| job.managed.as_mut())
            .filter(|run| run.plan.as_ref().is_some_and(|p| p.owner() == plan.owner()))
        else {
            return;
        };
        run.slot = cx
            .plan_slots(plan)
            .ok()
            .and_then(|slots| slots.into_iter().next());
        self.trading.waiting = false;
        self.advance_trade(cx);
    }
    pub(super) fn protective_delay(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        delay: f64,
        operation: Operation,
    ) -> Result<bool, Error> {
        let Some(run) = self
            .trading
            .active
            .as_mut()
            .and_then(|job| job.managed.as_mut())
        else {
            return Ok(false);
        };
        let (slot, context) = run.current.as_ref().ok_or(Error::State)?;
        cx.continue_serial_plan_after(
            run.plan.as_ref().ok_or(Error::State)?,
            slot,
            context,
            delay.ceil().max(1.) as u32,
            terms(operation),
        )?;
        run.current = None;
        self.trading.waiting = true;
        Ok(true)
    }
    pub(crate) fn queue_protective(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        payload: String,
        native: Request,
    ) {
        let command = native.command;
        if let Err(error) = self.protective_send_guard(cx, &native) {
            self.trade_failed(cx, command, not_sent(error));
            return;
        }
        let result: Result<_, Error> = (|| {
            let job = self
                .trading
                .active
                .as_mut()
                .filter(|job| job.command == command)
                .ok_or(Error::Revoked)?;
            let run = job.managed.as_mut().ok_or(Error::State)?;
            let operation = if method == Method::Get {
                Operation::Read
            } else {
                Operation::Mutation
            };
            let slot = if let Some(slot) = run.slot.take() {
                slot
            } else if let Some(plan) = &run.plan {
                let (slot, context) = run.current.as_ref().ok_or(Error::State)?;
                let next = cx.continue_serial_plan(plan, slot, context, terms(operation))?;
                run.current = None;
                next
            } else {
                let (plan, slot) = cx.admit_serial_plan(
                    &run.preflight,
                    terms(operation),
                    SerialPolicy {
                        credential: Some(run.write_credential.clone()),
                        pacing: None,
                        refusals_complete: false,
                    },
                )?;
                run.plan = Some(plan);
                slot
            };
            let plan = run.plan.as_ref().ok_or(Error::State)?.clone();
            let intent = match native.call {
                Call::ProtectiveRead { .. } => Intent::Read,
                Call::Cancel { .. } => Intent::Cancel,
                Call::NativeEdit => Intent::Amend,
                Call::ProtectivePlace { .. } => Intent::Place,
                _ => return Err(Error::Invalid),
            };
            let position_bound = job
                .protection
                .as_ref()
                .is_some_and(|edit| edit.scope == Scope::Stop || edit.observed.reduce_only);
            let context = if operation == Operation::Read {
                cx.plan_leg_context(&plan, &slot)?
            } else if intent == Intent::Cancel && position_bound {
                cx.plan_protection_cancel_leg_context(&plan, &slot)?
            } else {
                cx.plan_leg_context_with_intent(&plan, &slot, intent)?
            };
            if operation == Operation::Mutation {
                let edit = job.protection.as_ref().ok_or(Error::State)?;
                let words = format!(
                    "The outcome of your {} protection update is unknown. Check Open Orders and your position before replacing protection.",
                    edit.observed.symbol
                );
                cx.present_plan_failure(&plan, words)?;
            }
            run.current = Some((slot.clone(), context.clone()));
            let identity = if operation == Operation::Read {
                run.read.clone()
            } else {
                run.write.clone()
            };
            let recipe = crate::sign::dispatch::Http::new(
                identity,
                method,
                path.into(),
                payload.clone(),
                self.options.recv_window,
            )
            .with_header_policy(self.options.signing_policy.header_policy());
            let url = recipe.url();
            Ok((context, plan, slot, recipe, url))
        })();
        let (context, plan, slot, recipe, url) = match result {
            Ok(ready) => ready,
            Err(error) => {
                self.trade_failed(cx, command, error.venue_error());
                return;
            }
        };
        let pending = Pending::Protective(Box::new(ManagedRequest {
            native,
            plan,
            slot,
            context: context.clone(),
        }));
        if let Err((Pending::Protective(request), error)) = cx.request_signed_managed(
            context,
            None,
            method,
            url,
            (method != Method::Get).then_some(payload),
            Box::new(recipe),
            pending,
        ) {
            self.protective_failed(cx, *request, error.venue_error());
        }
    }
    pub(crate) fn protective_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: ManagedRequest,
        response: Response<'_>,
    ) {
        if cx
            .validate_plan_delivery(&request.plan, &request.context)
            .is_err()
        {
            return;
        }
        if request.context.operation() == Operation::Mutation {
            let parsed = Envelope::checked(response.body);
            let message = parsed
                .as_ref()
                .err()
                .cloned()
                .or_else(|| {
                    parsed
                        .as_ref()
                        .ok()
                        .and_then(|e| e.payload().ok())
                        .and_then(row_refusal)
                })
                .unwrap_or_else(|| "MEXC protective write acknowledgement is unproven".into());
            let accepted = parsed
                .as_ref()
                .ok()
                .filter(|_| response.status == 200)
                .is_some_and(|e| match &request.native.call {
                    Call::ProtectivePlace { .. } => e
                        .payload()
                        .ok()
                        .is_some_and(|data| accepted_place_id(data).is_some()),
                    Call::NativeEdit => self
                        .trading
                        .active
                        .as_ref()
                        .and_then(|job| job.protection.as_ref())
                        .is_some_and(|edit| {
                            accepted_native(e.data.map(|data| data.get()), &edit.observed.id)
                        }),
                    Call::Cancel { ids, regular: true } => e
                        .payload()
                        .ok()
                        .and_then(|data| serde_json::from_str::<Vec<ResultRow<'_>>>(data).ok())
                        .is_some_and(|rows| {
                            rows.len() == ids.len()
                                && ids.iter().all(|id| {
                                    rows.iter()
                                        .filter(|row| {
                                            (&row.order_id == id || &row.id == id)
                                                && row.error_code == 0.
                                        })
                                        .count()
                                        == 1
                                })
                        }),
                    Call::Cancel { .. } => true,
                    _ => false,
                });
            let fact = if accepted {
                let ids = match &request.native.call {
                    Call::Cancel { ids, .. } => ids.clone(),
                    Call::NativeEdit => self
                        .trading
                        .active
                        .as_ref()
                        .and_then(|j| j.protection.as_ref())
                        .map(|edit| vec![edit.observed.id.clone()])
                        .unwrap_or_default(),
                    _ => parsed
                        .as_ref()
                        .ok()
                        .and_then(|e| e.payload().ok())
                        .and_then(accepted_place_id)
                        .into_iter()
                        .collect(),
                };
                WriteFact::Accepted { order_ids: ids }
            } else {
                let refused = parsed
                    .as_ref()
                    .ok()
                    .and_then(|e| e.payload().ok())
                    .is_some_and(|data| row_refusal(data).is_some());
                let error = write_failure(
                    response.status,
                    response.body,
                    message.clone(),
                    refused,
                    true,
                );
                let code = error
                    .venue_code
                    .as_deref()
                    .and_then(|code| code.parse::<i64>().ok());
                let native = code.map(|code| safe_cex_engine::classify::Native {
                    code: safe_cex_engine::classify::NativeCode::Int(code),
                    message: error.venue_message.as_deref().unwrap_or(""),
                });
                let Ok(mut fact) = cx.classify_write(
                    &request.context,
                    &WRITE_CODES,
                    native.as_ref(),
                    error.message.clone(),
                ) else {
                    return;
                };
                if error.outcome != Some(Outcome::Rejected)
                    || error.kind == Some(ErrorKind::RateLimited)
                {
                    fact = fact.unproven();
                }
                if error.outcome == Some(Outcome::Rejected)
                    && error.kind != Some(ErrorKind::RateLimited)
                {
                    let _ = cx.present_plan_failure(&request.plan, message);
                }
                WriteFact::Failed {
                    fact,
                    known_effects: false,
                }
            };
            let failure = match &fact {
                WriteFact::Failed { fact, .. } if !fact.verdict().is_refusal() => {
                    Some(fact.error().clone())
                }
                _ => None,
            };
            if cx.http_write_fact(&request.context, fact, None).is_err() {
                return;
            }
            if let Some(error) = failure {
                self.trade_failed(cx, request.native.command, error);
                return;
            }
        }
        self.trade_response(cx, request.native, response);
    }
    pub(crate) fn protective_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: ManagedRequest,
        error: VenueError,
    ) {
        if error.outcome == Some(Outcome::NotSent) {
            // The venue path's composed message for a definite transport
            // refusal on a protective amend; override because the outcome
            // is `NotSent`, which the gated entry point would drop.
            let _ = cx.override_plan_failure_message(&request.plan, error.message.clone());
        }
        let _ = cx.finish_plan_failed_call(&request.plan, &request.slot, &request.context);
        if let Some(run) = self
            .trading
            .active
            .as_mut()
            .filter(|job| job.command == request.native.command)
            .and_then(|job| job.managed.as_mut())
        {
            run.current = None;
        }
        self.trade_failed(cx, request.native.command, error);
    }
    pub(super) fn finish_protective(
        cx: &mut Cx<'_, Pending>,
        run: Run,
        result: Result<String, VenueError>,
    ) {
        if let Some(plan) = run.plan {
            if let Some((slot, context)) = run.current {
                let _ = cx.finish_plan_call(&plan, &slot, &context);
            }
            match result {
                Ok(data) => {
                    let _ = cx.done_plan(&plan, data);
                }
                Err(error) => {
                    let _ = cx.fail_plan(&plan, error);
                }
            }
        } else if let Err(error) = result {
            let _ = cx.fail_preflight(&run.preflight, error);
        }
    }
}
impl MexcFutures {
    pub(crate) fn trading_is_protective(&self, command: CommandId) -> bool {
        self.trading
            .active
            .as_ref()
            .is_some_and(|job| job.command == command && job.managed.is_some())
    }
    pub(crate) fn protective_release(
        &self,
        store: &safe_cex_engine::Store,
        request: &ManagedRequest,
        now: f64,
    ) -> Result<(), VenueError> {
        let job = self
            .trading
            .active
            .as_ref()
            .filter(|job| {
                job.command == request.native.command
                    && job.managed.as_ref().is_some_and(|run| {
                        run.plan
                            .as_ref()
                            .is_some_and(|plan| plan.owner() == request.plan.owner())
                    })
            })
            .ok_or_else(|| not_sent("MEXC protective owner is retired"))?;
        if self
            .account
            .order_updates
            .get(
                &job.protection
                    .as_ref()
                    .ok_or_else(|| not_sent("MEXC protection provenance missing"))?
                    .observed
                    .id,
            )
            .is_some_and(|revision| *revision > request.native.revision)
        {
            return Err(not_sent(
                "MEXC native protection changed after preparation; stop not changed",
            ));
        }
        let edit = job
            .protection
            .as_ref()
            .ok_or_else(|| not_sent("MEXC protection provenance missing"))?;
        match request.native.call {
            Call::Cancel { .. } | Call::NativeEdit => {
                protection::guard_at(store, now, &self.references, edit, false).map_err(not_sent)
            }
            Call::ProtectivePlace { restore: true } => {
                protection::guard_at(store, now, &self.references, edit, true).map_err(not_sent)
            }
            Call::ProtectivePlace { restore: false } => {
                if !edit.cancel_proven {
                    return Err(not_sent(
                        "MEXC cancellation is not proven; replacement withheld",
                    ));
                }
                protection::replacement_guard_at(store, now, &self.references, edit)
                    .map_err(not_sent)
            }
            _ => Ok(()),
        }
    }
    pub(crate) fn protective_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) -> Option<Preflight> {
        self.trading.queue.retain(|job| {
            job.managed
                .as_ref()
                .is_none_or(|run| !owner.matches_preflight(&run.preflight))
        });
        if self
            .trading
            .active
            .as_ref()
            .and_then(|job| job.managed.as_ref())
            .is_some_and(|run| {
                owner.matches_preflight(&run.preflight)
                    || run
                        .plan
                        .as_ref()
                        .is_some_and(|plan| owner.matches_plan(plan))
            })
        {
            self.trading.retired_managed = true;
            self.trading.active = None;
            self.trading.waiting = false;
            self.trading.active = self.trading.queue.pop_front();
            return self
                .trading
                .active
                .as_ref()
                .and_then(|job| job.managed.as_ref())
                .map(|run| run.preflight.clone());
        }
        None
    }
}

impl MexcFutures {
    pub(crate) fn settle_protective_retirement(&mut self, cx: &mut Cx<'_, Pending>) {
        if !std::mem::take(&mut self.trading.retired_managed) {
            return;
        }
        self.advance_trade(cx);
        self.refresh_account(cx, true);
    }
}
