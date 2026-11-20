//! Readiness is prerequisite evidence, never cancellation or replay authority.
use super::{
    BybitLinear, CALL_TIMEOUT_MS, Call, CommandId, Context, Cx, Job, Method, Ordinary, Pending,
    Stage, VenueError, not_sent, partial,
};
use safe_cex_engine::Response;
use safe_cex_engine::request_context::plan::{CallTerms, RetryWindow};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};

/// The check before a protective edit lost its own state; nothing was sent.
const UNFINISHED: &str = "Couldn't finish checking your order with Bybit, so the edit wasn't sent. Nothing changed; try again.";

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
            entry.protection = guard
                .cloned()
                .map(|guard| Box::new(partial::Readiness::new(guard)));
        }
        let Some(readiness) = &entry.protection else {
            return Ok(None);
        };
        if !guard.is_some_and(|guard| readiness.matches(guard)) {
            let what = readiness.what();
            return Err(not_sent(format!(
                "Your {what} edit changed while it was being checked, so it wasn't sent. Your {what} is unchanged; try again."
            )));
        }
        // Revalidate the captured order and position after every deferred read.
        readiness.validate(cx.store, cx.now)?;
        let Some(planned) = readiness.planned() else {
            return Ok(None);
        };
        self.select_category(self.category_for(readiness.symbol()));
        let what = readiness.what();
        // The read never reached Bybit: keep this refusal's kind unset, as before.
        let refused = || {
            not_sent(format!(
                "Couldn't check your current {what} with Bybit, so the edit wasn't sent. Your {what} is unchanged; try again."
            ))
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
            .ok_or_else(|| not_sent(UNFINISHED))
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
            let _ = cx.fail_preflight(&entry.preflight, not_sent(UNFINISHED));
            return;
        }
        self.park(id, entry);
        self.resume_ordinary(cx, id);
    }
}
