//! Readiness is prerequisite evidence, never cancellation or replay authority.
use super::{
    BybitLinear, CALL_TIMEOUT_MS, Call, CommandId, Context, Cx, Job, Method, Ordinary, Pending,
    Stage, VenueError, not_sent, partial,
};
use safe_cex_engine::Response;
use safe_cex_engine::request_context::plan::{CallTerms, RetryWindow};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};

impl BybitLinear {
    pub(super) fn prepare_protection(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        entry: &mut Ordinary,
        job: &Job,
    ) -> Result<Option<Context>, VenueError> {
        if !cx.frame_fed() {
            return Ok(None);
        }
        let guard = job
            .steps
            .iter()
            .flat_map(|step| &step.requests)
            .find_map(|planned| match &planned.call {
                Call::CancelPartialStop { guard, .. } => Some(guard.as_ref()),
                _ => None,
            });
        if entry.protection.is_none() {
            entry.protection = guard.cloned().map(partial::Readiness::new);
        }
        let Some(readiness) = &entry.protection else {
            return Ok(None);
        };
        if !guard.is_some_and(|guard| readiness.matches(guard)) {
            return Err(not_sent(
                "The protective edit changed during readiness; your protection is unchanged",
            ));
        }
        // Revalidate the captured order and position after every deferred read.
        readiness.validate(cx.store, cx.now)?;
        let Some(planned) = readiness.planned() else {
            return Ok(None);
        };
        self.select_category(self.category_for(readiness.symbol()));
        let refused = || {
            not_sent("Protective-edit readiness could not be checked; your protection is unchanged")
        };
        let context = cx
            .preflight_read_context(
                &entry.preflight,
                Policy {
                    operation: Operation::Read,
                    credential: entry.read_credential.clone(),
                    timeout_ms: CALL_TIMEOUT_MS,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
                CallTerms {
                    wait_ms: 0,
                    timeout_ms: CALL_TIMEOUT_MS,
                    attempts: 1,
                    retry_window: RetryWindow::FreshAttempt,
                },
            )
            .map_err(|_| refused())?;
        if !self.send_ordinary(
            cx,
            id,
            context.clone(),
            Method::Get,
            planned.path,
            planned.body,
        ) {
            return Err(refused());
        }
        Ok(Some(context))
    }

    pub(super) fn on_protective_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        mut entry: Ordinary,
        response: &Response<'_>,
    ) {
        let Stage::ProtectiveRead { context } = std::mem::replace(&mut entry.stage, Stage::Idle)
        else {
            return;
        };
        if cx
            .validate_preflight_delivery(&entry.preflight, &context)
            .is_err()
        {
            entry.stage = Stage::ProtectiveRead { context };
            self.park(id, entry);
            return;
        }
        let result = entry
            .protection
            .as_mut()
            .ok_or_else(|| {
                not_sent("Protective-edit readiness lost its owner; your protection is unchanged")
            })
            .and_then(|readiness| readiness.observe(response));
        if let Err(error) = result {
            cx.command_error(id, error.message.clone());
            let _ = cx.fail_preflight(&entry.preflight, error);
            return;
        }
        if cx
            .finish_preflight_call(&entry.preflight, &context)
            .is_err()
        {
            let _ = cx.fail_preflight(
                &entry.preflight,
                not_sent(
                    "Protective-edit readiness lost its custody; your protection is unchanged",
                ),
            );
            return;
        }
        self.park(id, entry);
        self.resume_ordinary(cx, id);
    }
}
