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
