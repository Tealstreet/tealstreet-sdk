//! Registry custody for execution groups: roots, observers and leg accounting.
//! Observers are entry-owned command roots; legs are `Owner::Execution`.
use super::*;
use crate::execution_group::{Execution, Groups, Joined, Member, Observer, Root, Sealed, Window};

#[derive(Default)]
pub(super) struct Executions {
    groups: Groups,
    /// Execution root identity (its `Owner::Execution` generation) to group.
    roots: HashMap<u64, Execution>,
    observers: HashMap<CommandId, Observer>,
}

impl ContextRegistry {
    /// Cheap per-event guard: most engines never hold an execution.
    /// Every expirable owner, queued owner, leg count and deadline lives in a
    /// group, so an empty group map means nothing can expire or be revoked.
    pub(in crate::engine) fn has_executions(&self) -> bool {
        !self.executions.groups.is_empty()
    }

    /// With no group left every root is stale; drop them without a scan.
    pub(in crate::engine) fn clear_retired_roots(&mut self) {
        if self.executions.groups.is_empty() && !self.executions.roots.is_empty() {
            self.executions.roots.clear();
        }
    }

    #[cfg(test)]
    pub(in crate::engine) fn root_count(&self) -> usize {
        self.executions.roots.len()
    }

    pub(in crate::engine) fn is_observer(&self, command: CommandId) -> bool {
        self.executions.observers.contains_key(&command)
    }

    fn execution_of(&self, context: &Context) -> Option<Execution> {
        let Owner::Execution { generation } = context.owner() else {
            return None;
        };
        self.executions.roots.get(&generation).copied()
    }

    /// Live send/publication authority for an execution-owned handle.
    pub(in crate::engine) fn execution_live(&self, context: &Context) -> bool {
        self.execution_of(context)
            .is_some_and(|execution| self.executions.groups.may_publish(execution))
    }

    /// Exclusive whole-command composition: an entry-owned root with no other
    /// issued handle, attempt, delivery or journal.
    fn observer_member(
        &self,
        root: &Context,
        key: &str,
        now: f64,
        planned: bool,
    ) -> Result<Member, Error> {
        self.validate(root, now)?;
        let Owner::Command { id, generation } = root.owner() else {
            return Err(Error::Invalid);
        };
        let owner = self.commands.get(&id).ok_or(Error::Revoked)?;
        if !matches!(&owner.phase, CommandPhase::Context { root: issued, entry_owned: true } if issued == root)
        {
            return Err(Error::Revoked);
        }
        if owner.pending != 0
            || owner.deliveries != 0
            || owner.writes.is_some()
            // A Plan seal projects the founder's journal; the member has none.
            || (owner.mutation_declared && !planned)
            || self
                .contexts
                .values()
                .any(|context| context.owner() == root.owner() && context != root)
        {
            return Err(Error::State);
        }
        Ok(Member {
            command: id,
            generation,
            operation: root.operation(),
            deadline: root.deadline(),
            key: key.to_owned(),
        })
    }

    pub(in crate::engine) fn begin_execution(
        &mut self,
        identity: u64,
        founder: &Context,
        policy: Policy,
        window: Window,
        key: &str,
        now: f64,
    ) -> Result<Context, Error> {
        // Mutation executions need the shared write journal (Hyro phase).
        if policy.operation != Operation::Read || policy.timeout_ms == 0 || !now.is_finite() {
            return Err(Error::Invalid);
        }
        let member = self.observer_member(founder, key, now, false)?;
        self.contexts.retain(|_, old| old.deadline() > now);
        if self
            .contexts
            .len()
            .checked_add(self.reserved_context_count())
            .is_none_or(|total| total >= CAPACITY)
        {
            return Err(Error::Capacity);
        }
        if self.contexts.contains_key(&identity) {
            return Err(Error::Duplicate);
        }
        self.contexts
            .try_reserve(1 + self.reserved_context_count())
            .map_err(|_| Error::Capacity)?;
        // Validate the root before committing any group state.
        let deadline = founder.deadline().min(now + f64::from(policy.timeout_ms));
        let mut root = Context::new(
            Owner::Execution {
                generation: identity,
            },
            identity,
            Operation::Read,
            policy.credential,
            now,
            deadline,
            policy.pacing,
        )?
        .with_retry(policy.retry)?;
        let command = member.command;
        let (execution, observer) = self.executions.groups.begin(
            Root::Command(member),
            Operation::Read,
            window,
            policy.timeout_ms,
            now,
        )?;
        debug_assert_eq!(self.executions.groups.deadline(execution), Some(deadline));
        root.bind_issuer(&self.issuer);
        self.contexts.insert(identity, root.clone());
        self.executions.roots.insert(identity, execution);
        self.executions
            .observers
            .insert(command, observer.expect("command-rooted execution"));
        Ok(root)
    }

    pub(in crate::engine) fn join_execution(
        &mut self,
        execution_root: &Context,
        root: &Context,
        key: &str,
        now: f64,
    ) -> Result<Joined, Error> {
        self.validate(execution_root, now)?;
        let execution = self.execution_of(execution_root).ok_or(Error::Revoked)?;
        let planned = self.executions.groups.planned(execution);
        let member = self.observer_member(root, key, now, planned)?;
        let command = member.command;
        let (observer, joined) = self.executions.groups.join(execution, member, now)?;
        self.executions.observers.insert(command, observer);
        Ok(joined)
    }

    pub(in crate::engine) fn execution_leg_issued(
        &mut self,
        context: &Context,
    ) -> Result<(), Error> {
        let execution = self.execution_of(context).ok_or(Error::Revoked)?;
        self.executions.groups.leg_issued(execution)
    }

    pub(in crate::engine) fn execution_leg_released(&mut self, context: &Context) {
        if let Some(execution) = self.execution_of(context) {
            let _ = self.executions.groups.leg_released(execution);
        }
    }

    pub(in crate::engine) fn execution_leg_retired(&mut self, context: &Context) {
        if let Some(execution) = self.execution_of(context) {
            let _ = self.executions.groups.leg_retired(execution);
        }
    }

    /// A planned observer leaving before the Plan seal is told what the shared
    /// journal knows now, without sealing or charging it: its accepted IDs,
    /// and Unknown while a peer remains (more declared work may still send).
    #[cfg(feature = "http-plans")]
    fn project_early(&self, sealed: &mut Sealed) {
        let (Some(execution), Err(error)) = (sealed.early, &mut sealed.result) else {
            return;
        };
        let Some(writes) = self
            .executions
            .roots
            .iter()
            .find(|(_, root)| **root == execution)
            .and_then(|(identity, _)| self.physical.get(identity))
            .and_then(|owner| owner.writes.as_ref())
        else {
            return;
        };
        if let Ok(projected) = writes.project_failure(error.clone()) {
            // The observer keeps its own reason; the journal supplies the
            // write outcome and known IDs. A live peer keeps declared work
            // open, so no current answer can rule out a later effect.
            error.outcome = if self.executions.groups.may_publish(execution) {
                Some(crate::Outcome::Unknown)
            } else {
                projected.outcome
            };
            error.partial = projected.partial;
        }
    }

    #[cfg(not(feature = "http-plans"))]
    fn project_early(&self, _: &mut Sealed) {}

    pub(in crate::engine) fn detach_observer(
        &mut self,
        root: &Context,
        error: VenueError,
    ) -> Result<Sealed, Error> {
        let Owner::Command { id, .. } = root.owner() else {
            return Err(Error::Invalid);
        };
        if !root.issued_by(&self.issuer)
            || self
                .commands
                .get(&id)
                .is_none_or(|owner| owner.context_root() != Some(root))
        {
            return Err(Error::Revoked);
        }
        let observer = *self.executions.observers.get(&id).ok_or(Error::Revoked)?;
        let mut sealed = self.executions.groups.detach(observer, error)?;
        self.project_early(&mut sealed);
        Ok(sealed)
    }

    /// Whether `context` is a Plan-backed execution root: its only physical
    /// work is the Plan's and only the Plan seal completes it.
    pub(in crate::engine) fn planned_execution_root(&self, context: &Context) -> bool {
        self.execution_of(context)
            .is_some_and(|execution| self.executions.groups.planned(execution))
    }

    pub(in crate::engine) fn seal_execution(
        &mut self,
        execution_root: &Context,
        project: impl FnMut(&str) -> Result<String, VenueError>,
    ) -> Result<Vec<Sealed>, Error> {
        if !execution_root.issued_by(&self.issuer)
            || self.contexts.get(&execution_root.leg()) != Some(execution_root)
        {
            return Err(Error::Revoked);
        }
        let execution = self.execution_of(execution_root).ok_or(Error::Revoked)?;
        if self.executions.groups.planned(execution) {
            return Err(Error::Invalid);
        }
        self.executions.groups.complete(execution, project)
    }

    pub(in crate::engine) fn expire_executions(&mut self, now: f64) -> Vec<Sealed> {
        let mut sealed = Vec::new();
        // Iterate the groups themselves, in issue order, so expiry cannot
        // depend on root bookkeeping.
        for execution in self.executions.groups.executions() {
            sealed.extend(self.executions.groups.expire(execution, now));
        }
        for seal in &mut sealed {
            self.project_early(seal);
        }
        let groups = &self.executions.groups;
        self.executions
            .roots
            .retain(|_, execution| groups.deadline(*execution).is_some());
        sealed
    }

    pub(in crate::engine) fn next_execution_deadline(&self) -> Option<f64> {
        self.executions.groups.next_timer()
    }

    /// The observer's own completion root, only for its sealed generation.
    pub(in crate::engine) fn observer_authority(&self, sealed: &Sealed) -> Option<Context> {
        self.commands
            .get(&sealed.command)
            .filter(|owner| owner.generation == sealed.generation)
            .and_then(|owner| owner.context_root().cloned())
    }

    /// Check and reserve everything the planned execution adds beyond the
    /// Plan itself (group membership, root and physical-owner storage, one
    /// more retirement) so the commit after Plan admission cannot fail.
    #[cfg(feature = "http-plans")]
    pub(in crate::engine) fn prepare_planned_execution(
        &mut self,
        founder: CommandId,
        key: &str,
    ) -> Result<(), Error> {
        self.executions.groups.admissible(founder, key)?;
        self.executions
            .roots
            .try_reserve(1)
            .map_err(|_| Error::Capacity)?;
        self.physical.try_reserve(1).map_err(|_| Error::Capacity)?;
        let retained = self
            .commands
            .values()
            .chain(self.physical.values())
            .filter(|owner| owner.retained())
            .count()
            + 2;
        self.retirements
            .try_reserve_exact(retained.saturating_sub(self.retirements.len()))
            .map_err(|_| Error::Capacity)
    }

    /// Commit the group whose physical owner is this admitted Plan; the
    /// founder's observer root is its first member. Bound: the Plan root.
    #[cfg(feature = "http-plans")]
    pub(in crate::engine) fn begin_planned_execution(
        &mut self,
        plan: &crate::request_context::Plan,
        policy: &crate::request_context::PlanPolicy,
        founder: &Context,
        key: &str,
        now: f64,
    ) -> Result<Context, Error> {
        let Owner::Execution {
            generation: identity,
        } = plan.owner()
        else {
            return Err(Error::Invalid);
        };
        let Owner::Command { id, generation } = founder.owner() else {
            return Err(Error::Invalid);
        };
        let mut root = Context::new(
            plan.owner(),
            identity,
            policy.operation,
            policy.credential.clone(),
            now,
            plan.deadline(),
            policy.pacing,
        )?;
        let member = Member {
            command: id,
            generation,
            operation: founder.operation(),
            deadline: founder.deadline(),
            key: key.to_owned(),
        };
        let (execution, observer) = self.executions.groups.begin(
            Root::Planned {
                deadline: plan.deadline(),
                founder: member,
            },
            policy.operation,
            Window::UntilTerminal,
            1,
            now,
        )?;
        root.bind_issuer(&self.issuer);
        self.contexts.insert(identity, root.clone());
        self.executions.roots.insert(identity, execution);
        self.executions
            .observers
            .insert(id, observer.expect("founder observer"));
        Ok(root)
    }

    /// The commands observing a physical Plan owner's execution.
    #[cfg(all(feature = "http-plans", feature = "order-locks"))]
    pub(in crate::engine) fn physical_observers(&self, owner: Owner) -> Vec<CommandId> {
        let Owner::Execution {
            generation: identity,
        } = owner
        else {
            return Vec::new();
        };
        self.executions
            .roots
            .get(&identity)
            .map_or_else(Vec::new, |execution| {
                self.executions.groups.observer_commands(*execution)
            })
    }

    #[cfg(feature = "http-plans")]
    pub(in crate::engine) fn has_planned_executions(&self) -> bool {
        !self.physical.is_empty()
    }

    /// Seal a physical owner's execution once with the Plan's projection.
    #[cfg(feature = "http-plans")]
    pub(in crate::engine) fn seal_planned_execution(
        &mut self,
        plan: &crate::request_context::Plan,
        projection: &Result<String, VenueError>,
    ) -> Vec<Sealed> {
        let Owner::Execution {
            generation: identity,
        } = plan.owner()
        else {
            return Vec::new();
        };
        self.contexts.remove(&identity);
        let Some(execution) = self.executions.roots.get(&identity).copied() else {
            return Vec::new();
        };
        self.executions
            .groups
            .complete(execution, |_| projection.clone())
            .unwrap_or_default()
    }

    /// A live physical Plan whose execution has no observer left; the engine
    /// fails it outside any delivery so nothing more is sent or published.
    #[cfg(feature = "http-plans")]
    pub(in crate::engine) fn orphaned_plan(&self) -> Option<crate::request_context::Plan> {
        self.physical.iter().find_map(|(identity, owner)| {
            let orphan = self
                .executions
                .roots
                .get(identity)
                .is_none_or(|execution| !self.executions.groups.may_publish(*execution));
            match &owner.phase {
                CommandPhase::Planned(state) if orphan => Some(state.handle()),
                _ => None,
            }
        })
    }

    #[cfg(all(test, feature = "http-plans"))]
    pub(in crate::engine) fn execution_count(&self) -> usize {
        self.executions.roots.len()
    }

    pub(super) fn forget_observer(&mut self, command: CommandId) {
        self.executions.observers.remove(&command);
    }
}

#[cfg(all(test, feature = "http-plans"))]
#[path = "execution_plan_capacity_tests.rs"]
mod plan_capacity_tests;

#[cfg(all(test, feature = "http-plans"))]
#[path = "execution_plan_composition_tests.rs"]
mod plan_composition_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_group::Joined;

    fn member(command: CommandId, deadline: f64) -> Member {
        Member {
            command,
            generation: 1,
            operation: Operation::Read,
            deadline,
            key: String::new(),
        }
    }

    /// Seed a group directly, with no registry root, so the sweep cannot
    /// lean on root bookkeeping.
    fn orphan(registry: &mut ContextRegistry) -> Execution {
        let (execution, _) = registry
            .executions
            .groups
            .begin(
                Root::Background { deadline: 1_000. },
                Operation::Read,
                Window::BeforeRelease,
                1_000,
                0.,
            )
            .unwrap();
        assert!(registry.executions.roots.is_empty());
        execution
    }

    #[test]
    fn rootless_observers_queued_owners_and_legs_still_expire() {
        let mut registry = ContextRegistry::default();
        assert!(!registry.has_executions());
        let execution = orphan(&mut registry);
        let groups = &mut registry.executions.groups;
        groups.join(execution, member(1, 100.), 1.).unwrap();
        groups.leg_issued(execution).unwrap();
        groups.leg_released(execution).unwrap();
        let (_, queued) = groups.join(execution, member(2, 200.), 2.).unwrap();
        assert_eq!(queued, Joined::Pending);
        assert!(registry.has_executions());
        assert_eq!(registry.next_execution_deadline(), Some(100.));
        let sealed = registry.expire_executions(100.);
        assert_eq!(sealed.iter().map(|s| s.command).collect::<Vec<_>>(), [1]);
        assert_eq!(registry.next_execution_deadline(), Some(200.));
        let sealed = registry.expire_executions(200.);
        assert_eq!(sealed.iter().map(|s| s.command).collect::<Vec<_>>(), [2]);
        // The leg still holds the group, and the execution bound still fires.
        assert!(registry.has_executions());
        assert!(registry.expire_executions(1_000.).is_empty());
        registry.executions.groups.leg_retired(execution).unwrap();
        assert!(!registry.has_executions());
    }
}
