//! Immutable top-level barrier membership; no release or callback authority.
use super::*;
use crate::request_context::SendPacing;
use crate::request_context::plan::MutationBarrierBound;

#[cfg(test)]
#[path = "request_context_group_admission_tests.rs"]
mod tests;

pub(super) fn normalize(
    steps: &[StepBound<'_>],
    policy: &PlanPolicy,
    declarations: &[MutationBarrierBound],
    pacing: &SendPacing,
) -> Result<Vec<Range<usize>>, Error> {
    if !declarations.is_empty() {
        if policy.operation != Operation::Mutation {
            return Err(Error::Invalid);
        }
        pacing.validate_mutation_group(policy.pacing)?;
    }
    if declarations.len() > CAPACITY {
        return Err(Error::Capacity);
    }
    let mut groups = Vec::new();
    groups
        .try_reserve_exact(declarations.len())
        .map_err(|_| Error::Capacity)?;
    for declaration in declarations {
        let step = steps.get(declaration.step).ok_or(Error::Invalid)?;
        if step.calls.len() < 2
            || step
                .calls
                .iter()
                .any(|c| c.attempts != 1 || !c.after_refusal.is_empty())
        {
            return Err(Error::Invalid);
        }
        // Schedule emits all root calls before its queued fallback sequences.
        let start = steps[..declaration.step].iter().try_fold(0usize, |n, s| {
            n.checked_add(s.calls.len()).ok_or(Error::Capacity)
        })?;
        let end = start.checked_add(step.calls.len()).ok_or(Error::Capacity)?;
        let range = start..end;
        if groups.contains(&range) {
            return Err(Error::Duplicate);
        }
        groups.push(range);
    }
    Ok(groups)
}

impl<P> Cx<'_, P> {
    #[inline]
    pub fn admit_plan_with_barriers(
        &mut self,
        preflight: &Preflight,
        steps: &[StepBound<'_>],
        policy: PlanPolicy,
        barriers: &[MutationBarrierBound],
    ) -> Result<Plan, Error> {
        self.core()
            .admit_plan_with_barriers(preflight, steps, policy, barriers)
    }
}

impl crate::engine::CoreCx<'_> {
    /// Barrier declarations are immutable and share the canonical transaction.
    pub fn admit_plan_with_barriers(
        &mut self,
        preflight: &Preflight,
        steps: &[StepBound<'_>],
        policy: PlanPolicy,
        barriers: &[MutationBarrierBound],
    ) -> Result<Plan, Error> {
        self.admit_declared_plan(
            preflight,
            AdmissionDeclaration {
                steps,
                policy,
                terminals: &[],
                barriers,
            },
        )
    }
}

/// Private metadata from an exact live selected Context; never a send token.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::engine) struct MutationGroupKey {
    pub owner: Owner,
    pub anchor: u64,
    pub ordinal: usize,
    pub count: usize,
}
impl MutationGroupKey {
    pub(in crate::engine) fn same_group(self, other: Self) -> bool {
        self.owner == other.owner && self.anchor == other.anchor && self.count == other.count
    }
}
impl ContextRegistry {
    pub(in crate::engine) fn mutation_group_key(
        &self,
        context: &Context,
    ) -> Result<Option<MutationGroupKey>, Error> {
        self.validate_handle(context)?;
        let Some(owner) = self.owner_entry(context.owner()) else {
            return Ok(None);
        };
        let CommandPhase::Planned(state) = &owner.phase else {
            return Ok(None);
        };
        let body = &state.body[0];
        if body.barriers.is_empty() {
            return Ok(None);
        }
        let index = body
            .entries
            .iter()
            .position(|entry| {
                entry
                    .call
                    .as_ref()
                    .is_some_and(|call| call.context == *context)
            })
            .ok_or(Error::Revoked)?;
        let Some(range) = body.barriers.iter().find(|range| range.contains(&index)) else {
            return Ok(None);
        };
        Ok(Some(MutationGroupKey {
            owner: context.owner(),
            anchor: body.entries[range.start].logical,
            ordinal: index - range.start,
            count: range.len(),
        }))
    }

    pub(in crate::engine) fn validate_mutation_group_submission(
        &self,
        context: &Context,
    ) -> Result<Option<MutationGroupKey>, Error> {
        let group = self.mutation_group_key(context)?;
        let Some(group) = group else {
            return Ok(None);
        };
        let owner = self.owner_entry(context.owner()).ok_or(Error::Revoked)?;
        let CommandPhase::Planned(state) = &owner.phase else {
            return Err(Error::Revoked);
        };
        let body = &state.body[0];
        let range = body
            .barriers
            .iter()
            .find(|range| body.entries[range.start].logical == group.anchor)
            .ok_or(Error::Revoked)?;
        if body.entries[range.start..range.start + group.ordinal]
            .iter()
            .any(|entry| {
                !entry
                    .call
                    .as_ref()
                    .is_some_and(|call| matches!(call.stage, Stage::Active { .. }))
            })
        {
            return Err(Error::State);
        }
        Ok(Some(group))
    }
}
