//! Declared mutation terminals and shared synchronous seal preparation.
use super::*;
use crate::request_context::PreparedWriteSeal;
use crate::request_context::plan::MutationTerminalBound;

pub(super) struct AdmissionDeclaration<'s, 'd, 't> {
    pub steps: &'s [StepBound<'d>],
    pub policy: PlanPolicy,
    pub terminals: &'t [MutationTerminalBound],
}

pub(super) struct PreparedPlanSeal {
    /// The Plan's physical owner: a command generation or an execution.
    pub owner: Owner,
    pub journal: Option<PreparedWriteSeal>,
    pub legacy: Option<VenueError>,
    pub reports: Vec<String>,
    /// One prepared terminal and report set per surviving execution observer,
    /// cloned before commit so the no-fail commit only moves them.
    pub fanout: Vec<(Result<String, VenueError>, Vec<String>)>,
    /// Reserved for exactly those observers' seals.
    pub sealed: Vec<crate::execution_group::Sealed>,
    /// The physical owner's live requests, listed before the seal.
    pub retired: Vec<RequestId>,
}

pub(super) fn terminal_ordinals(
    steps: &[StepBound<'_>],
    policy: &PlanPolicy,
    terminals: &[MutationTerminalBound],
) -> Result<Vec<usize>, Error> {
    if !terminals.is_empty() && policy.operation != Operation::Mutation {
        return Err(Error::Invalid);
    }
    if terminals.len() > CAPACITY {
        return Err(Error::Capacity);
    }
    let mut ordinals = Vec::new();
    ordinals
        .try_reserve_exact(terminals.len())
        .map_err(|_| Error::Capacity)?;
    for terminal in terminals {
        let step = steps.get(terminal.after_step).ok_or(Error::Invalid)?;
        if step.calls.len() != 1 {
            return Err(Error::Invalid);
        }
        let ordinal = steps[..terminal.after_step]
            .iter()
            .try_fold(0usize, |n, step| {
                n.checked_add(step.calls.len()).ok_or(Error::Capacity)
            })?;
        if ordinals.contains(&ordinal) {
            return Err(Error::Invalid);
        }
        ordinals.push(ordinal);
    }
    Ok(ordinals)
}

impl<P> Cx<'_, P> {
    /// Admit immutable optional mutation terminals in the same complete native
    /// Plan transaction. No declaration may be changed after admission.
    pub fn admit_plan_with_terminals(
        &mut self,
        preflight: &Preflight,
        steps: &[StepBound<'_>],
        policy: PlanPolicy,
        terminals: &[MutationTerminalBound],
    ) -> Result<Plan, Error> {
        self.admit_declared_plan(
            preflight,
            AdmissionDeclaration {
                steps,
                policy,
                terminals,
            },
        )
    }

    /// Complete at a declared native terminal from exact current DATA. Retire
    /// only unissued remainder; journal uncertainty still projects failure.
    pub fn done_mutation_plan_after_data(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        result: String,
    ) -> Result<(), Error> {
        let body = self.io.request_contexts.validate_plan(plan, self.now)?;
        if body.policy.operation != Operation::Mutation {
            return Err(Error::Invalid);
        }
        self.validate_plan_delivery(plan, context)?;
        let index = body.schedule.ordinal(slot)?;
        if !body.terminals.contains(&index) {
            return Err(Error::Invalid);
        }
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| call.context == *context)
            .ok_or(Error::Revoked)?;
        if body.entries.iter().enumerate().any(|(other, entry)| {
            other != index && (entry.call.is_some() || entry.credit == Credit::Issued)
        }) {
            return Err(Error::State);
        }
        self.io
            .request_contexts
            .owner_entry(plan.owner())
            .ok_or(Error::Revoked)?
            .writes
            .as_ref()
            .ok_or(Error::State)?
            .validate_housekeeping(call.admission.attempt.request)?;
        // No journal or structural mutation before all fallible projection and
        // report storage preparation succeeds. No callback may intervene.
        let prepared = self.prepare_plan_terminal(plan, None, Some(&result))?;
        self.finish_plan_call(plan, slot, context)
            .expect("validated conditional DATA settlement");
        self.commit_plan_terminal(plan, Some(result), prepared)
    }
}
