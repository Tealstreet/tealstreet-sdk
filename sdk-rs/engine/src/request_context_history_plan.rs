//! Prepared page and terminal transactions for a uniquely bound Read history.
use super::*;
use crate::engine::http_lifecycle::PreparedReadCleanup;
use crate::{Level, Value};

pub(crate) struct PreparedHistoryPage {
    plan: Plan,
    slot: Slot,
    context: Context,
    cleanup: Vec<PreparedReadCleanup>,
}
pub(crate) struct PreparedHistoryTerminal {
    plan: Plan,
    seal: conditional::PreparedPlanSeal,
    page: Option<(Slot, Context)>,
    cleanup: Vec<PreparedReadCleanup>,
    marker: Option<crate::log::LogEvent>,
}

impl<P> Cx<'_, P> {
    pub(crate) fn read_history_expiry_is_current(&self, plan: &Plan) -> Result<bool, Error> {
        if self
            .io
            .core
            .http_plan_expiring
            .as_ref()
            .is_some_and(|current| current.same_plan(plan))
        {
            self.io.core.request_contexts.validate_plan_seal_origin(
                plan,
                self.now,
                self.io.core.http_delivery.as_ref(),
            )?;
            return Ok(true);
        }
        Ok(false)
    }
    pub(crate) fn validate_read_history_data(
        &self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<Attempt, Error> {
        if self.io.core.http_owner_poison
            || self.io.core.http_plan_poison
            || self.io.core.http_plan_expiring.is_some()
        {
            return Err(Error::Revoked);
        }
        self.validate_plan_delivery(plan, context)?;
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        if !body.history_bound
            || body.policy.operation != Operation::Read
            || body.receipt_cap.is_none()
            || !matches!(plan.owner(), Owner::Command { .. })
        {
            return Err(Error::Invalid);
        }
        let index = body.schedule.ordinal(slot)?;
        let delivery = self.io.core.http_delivery.as_ref().ok_or(Error::Revoked)?;
        if !body.entries[index].call.as_ref().is_some_and(|call| {
            call.context.same_issuer(context) && call.context == *context && call.matches(delivery)
        }) {
            return Err(Error::Revoked);
        }
        Ok(delivery.attempt)
    }
    pub(crate) fn prepare_read_history_page(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<PreparedHistoryPage, Error> {
        self.validate_read_history_data(plan, slot, context)?;
        let cleanup = self
            .io
            .prepare_read_history_cleanup(plan.owner(), Some(context.leg()), 2)?;
        Ok(PreparedHistoryPage {
            plan: plan.clone(),
            slot: slot.clone(),
            context: context.clone(),
            cleanup,
        })
    }
    pub(crate) fn begin_read_history_commit(&mut self) {
        assert!(!self.io.core.http_owner_poison && !self.io.core.http_plan_poison);
        self.io.core.http_owner_poison = true;
    }
    fn settle_read_history_page(&mut self, plan: &Plan, slot: &Slot, context: &Context) {
        self.io
            .core
            .request_contexts
            .contexts
            .remove(&context.leg());
        let body = self
            .io
            .core
            .request_contexts
            .plan_body_mut(plan)
            .expect("prepared history Plan");
        let index = body.schedule.ordinal(slot).expect("prepared history Slot");
        body.schedule
            .settle(slot)
            .expect("prepared active history Slot");
        body.entries[index].call = None;
        body.entries[index].credit = Credit::Released;
        body.handoff = self.io.core.http_delivery.clone();
        body.refresh_availability(self.now);
        self.rearm_plan(plan);
    }
    pub(crate) fn commit_read_history_page(&mut self, prepared: PreparedHistoryPage) {
        self.io.commit_read_history_cleanup(prepared.cleanup);
        self.settle_read_history_page(&prepared.plan, &prepared.slot, &prepared.context);
        self.io.core.http_owner_poison = false;
    }
    pub(crate) fn prepare_read_history_terminal(
        &mut self,
        plan: &Plan,
        page: Option<(&Slot, &Context)>,
        released_failure: bool,
        error: Option<VenueError>,
        marker: Option<String>,
    ) -> Result<PreparedHistoryTerminal, Error> {
        if self.io.core.http_owner_poison
            || self.io.core.http_plan_poison
            || self.io.core.http_plan_expiring.is_some()
        {
            return Err(Error::Revoked);
        }
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        if !body.history_bound
            || !matches!(plan.owner(), Owner::Command { .. })
            || body.policy.operation != Operation::Read
            || body.receipt_cap.is_none()
        {
            return Err(Error::Invalid);
        }
        if let Some((slot, context)) = page {
            if released_failure {
                self.validate_read_plan_history_failure(plan, slot, context)?;
            } else {
                self.validate_read_history_data(plan, slot, context)?;
            }
            let index = body.schedule.ordinal(slot)?;
            if body.entries.iter().enumerate().any(|(other, entry)| {
                other != index && (entry.call.is_some() || entry.credit == Credit::Issued)
            }) {
                return Err(Error::State);
            }
        } else if error.is_none() {
            return Err(Error::Invalid);
        }
        self.io
            .core
            .request_contexts
            .retirements
            .try_reserve(1)
            .map_err(|_| Error::Capacity)?;
        let seal = self.prepare_plan_terminal(plan, error, None, None)?;
        let cleanup = self
            .io
            .prepare_read_history_cleanup(plan.owner(), None, 4)?;
        let Owner::Command { id, .. } = plan.owner() else {
            unreachable!()
        };
        let marker = marker.map(|message| crate::log::LogEvent {
            level: Level::Warn,
            code: crate::log::code::HISTORY_INCOMPLETE,
            message,
            fields: vec![("command", Value::Num(f64::from(id)))],
        });
        Ok(PreparedHistoryTerminal {
            plan: plan.clone(),
            seal,
            page: page
                .filter(|_| !released_failure)
                .map(|(slot, context)| (slot.clone(), context.clone())),
            cleanup,
            marker,
        })
    }
    pub(crate) fn commit_read_history_terminal(
        &mut self,
        mut prepared: PreparedHistoryTerminal,
        result: Result<String, VenueError>,
    ) {
        let result = match result {
            Ok(result) => Some(result),
            Err(mut error) => {
                if error.kind.is_none() {
                    error.kind = Some(crate::ErrorKind::Internal);
                }
                prepared.seal.legacy = Some(error);
                prepared.marker = None;
                prepared.page = None;
                None
            }
        };
        self.io.commit_read_history_cleanup(prepared.cleanup);
        if let Some((slot, context)) = prepared.page {
            self.settle_read_history_page(&prepared.plan, &slot, &context);
        }
        if let Some(marker) = prepared.marker {
            super::super::super::push_event(
                &mut self.io.core.actions,
                &mut self.io.core.health,
                self.io.core.min_level,
                marker,
            );
        }
        self.commit_plan_terminal(&prepared.plan, result, prepared.seal)
            .expect("prepared history terminal");
        self.io.core.http_owner_poison = false;
    }
}

#[cfg(test)]
#[path = "request_context_history_plan_tests.rs"]
mod tests;
