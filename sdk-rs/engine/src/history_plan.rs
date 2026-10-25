//! Command Read-Plan history ownership; completion is through guarded history.
use super::*;
use crate::request_context::Plan;

/// Unique history binding, separate from the cloneable Plan send handle.
/// Dropping it grants no terminal; the existing Plan expiry remains strict.
pub struct PlanReadCompletion {
    plan: Plan,
}

impl PlanReadCompletion {
    pub fn from_plan<P>(cx: &mut Cx<'_, P>, plan: &Plan) -> Result<Self, ContextError> {
        Ok(Self {
            plan: cx.bind_read_plan_history(plan)?,
        })
    }

    pub fn command(&self) -> CommandId {
        let Owner::Command { id, .. } = self.plan.owner() else {
            unreachable!("constructor admits only command Plans")
        };
        id
    }
}

use crate::request_context::plan::Slot;

/// Accepted output and the unvalidated physical page have one Plan owner.
pub struct PlanHistoryRows<T> {
    completion: PlanReadCompletion,
    accepted: Vec<T>,
    pending: Vec<T>,
    page: Option<(Context, Attempt)>,
    incomplete: Option<String>,
}

struct AcceptedPlanPrefix {
    plan: Plan,
}
impl AcceptedPlanPrefix {
    fn from_rows<T>(rows: &PlanHistoryRows<T>) -> Option<Self> {
        (!rows.accepted.is_empty()).then(|| Self {
            plan: rows.completion.plan.clone(),
        })
    }
}

impl<T> PlanHistoryRows<T> {
    pub fn new(completion: PlanReadCompletion) -> Self {
        Self {
            completion,
            accepted: Vec::new(),
            pending: Vec::new(),
            page: None,
            incomplete: None,
        }
    }
    pub fn command(&self) -> CommandId {
        self.completion.command()
    }
    pub fn accepted_len(&self) -> usize {
        self.accepted.len()
    }
    pub fn staged_len(&self) -> usize {
        self.pending.len()
    }
    pub fn discard_staged_page(&mut self) {
        self.pending.clear();
        self.page = None;
    }
    fn check_page(&self, context: &Context, attempt: Attempt) -> Result<(), ContextError> {
        if self
            .page
            .as_ref()
            .is_some_and(|(bound, current)| bound != context || *current != attempt)
        {
            return Err(ContextError::State);
        }
        Ok(())
    }
    pub fn admit<'a, 'io, P>(
        &'a mut self,
        cx: &'a mut Cx<'io, P>,
        slot: &'a Slot,
        context: &'a Context,
    ) -> Result<PlanRowsAdmission<'a, 'io, T, P>, ContextError> {
        let attempt = cx.validate_read_history_data(&self.completion.plan, slot, context)?;
        self.check_page(context, attempt)?;
        Ok(PlanRowsAdmission {
            rows: self,
            cx,
            slot,
            context,
            attempt,
        })
    }
    pub fn admit_failure<'a, 'io, P>(
        &'a mut self,
        cx: &'a mut Cx<'io, P>,
        slot: &'a Slot,
        context: &'a Context,
    ) -> Result<PlanFailureAdmission<'a, 'io, T, P>, ContextError> {
        let attempt =
            cx.validate_read_plan_history_failure(&self.completion.plan, slot, context)?;
        self.check_page(context, attempt)?;
        Ok(PlanFailureAdmission {
            rows: self,
            cx,
            slot,
            context,
        })
    }
    pub fn reject<P>(&mut self, cx: &mut Cx<'_, P>, error: VenueError) -> Result<(), ContextError> {
        if cx.read_history_expiry_is_current(&self.completion.plan)? {
            return Ok(());
        }
        let prepared = cx.prepare_read_history_terminal(
            &self.completion.plan,
            None,
            false,
            Some(error.clone()),
            None,
        )?;
        cx.begin_read_history_commit();
        self.pending.clear();
        self.accepted.clear();
        self.page = None;
        cx.commit_read_history_terminal(prepared, Err(error));
        Ok(())
    }
}

/// The mutable Cx loan has no accessor and cannot escape this physical page.
pub struct PlanRowsAdmission<'a, 'io, T, P> {
    rows: &'a mut PlanHistoryRows<T>,
    cx: &'a mut Cx<'io, P>,
    slot: &'a Slot,
    context: &'a Context,
    attempt: Attempt,
}
impl<T, P> PlanRowsAdmission<'_, '_, T, P> {
    pub fn stage(&mut self, row: T) -> Result<(), ContextError> {
        self.rows
            .pending
            .try_reserve(1)
            .map_err(|_| ContextError::Capacity)?;
        if self.rows.page.is_none() {
            self.rows.page = Some((self.context.clone(), self.attempt));
        }
        self.rows.pending.push(row);
        Ok(())
    }
    pub fn incomplete(&mut self, message: impl Into<String>) {
        self.rows.incomplete.get_or_insert_with(|| message.into());
    }
    pub fn accept_page(self) -> Result<(), ContextError> {
        self.rows
            .accepted
            .try_reserve(self.rows.pending.len())
            .map_err(|_| ContextError::Capacity)?;
        let prepared = self.cx.prepare_read_history_page(
            &self.rows.completion.plan,
            self.slot,
            self.context,
        )?;
        self.cx.begin_read_history_commit();
        self.rows.accepted.append(&mut self.rows.pending);
        self.rows.page = None;
        self.cx.commit_read_history_page(prepared);
        Ok(())
    }
    pub fn finish_with(
        self,
        encode: impl FnOnce(Vec<T>) -> Result<String, VenueError>,
    ) -> Result<(), ContextError> {
        self.rows
            .accepted
            .try_reserve(self.rows.pending.len())
            .map_err(|_| ContextError::Capacity)?;
        let prepared = self.cx.prepare_read_history_terminal(
            &self.rows.completion.plan,
            Some((self.slot, self.context)),
            false,
            None,
            self.rows.incomplete.clone(),
        )?;
        self.cx.begin_read_history_commit();
        self.rows.accepted.append(&mut self.rows.pending);
        self.rows.page = None;
        let result = encode(std::mem::take(&mut self.rows.accepted));
        self.cx.commit_read_history_terminal(prepared, result);
        Ok(())
    }
}

/// Only an exact engine-released failure may recover a nonempty accepted Vec.
pub struct PlanFailureAdmission<'a, 'io, T, P> {
    rows: &'a mut PlanHistoryRows<T>,
    cx: &'a mut Cx<'io, P>,
    slot: &'a Slot,
    context: &'a Context,
}
impl<T, P> PlanFailureAdmission<'_, '_, T, P> {
    pub fn finish_with(
        self,
        error: VenueError,
        encode: impl FnOnce(Vec<T>) -> Result<String, VenueError>,
    ) -> Result<(), ContextError> {
        let Some(prefix) = AcceptedPlanPrefix::from_rows(self.rows) else {
            return self.rows.reject(self.cx, error);
        };
        let marker = self
            .rows
            .incomplete
            .clone()
            .unwrap_or_else(|| format!("history is INCOMPLETE: {}", error.message));
        let prepared = self.cx.prepare_read_history_terminal(
            &prefix.plan,
            Some((self.slot, self.context)),
            true,
            None,
            Some(marker),
        )?;
        self.cx.begin_read_history_commit();
        self.rows.pending.clear();
        self.rows.page = None;
        let result = encode(std::mem::take(&mut self.rows.accepted));
        self.cx.commit_read_history_terminal(prepared, result);
        Ok(())
    }
}
