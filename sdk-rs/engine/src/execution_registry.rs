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
    fn observer_member(&self, root: &Context, key: &str, now: f64) -> Result<Member, Error> {
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
            || owner.mutation_declared
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
        let member = self.observer_member(founder, key, now)?;
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
        let member = self.observer_member(root, key, now)?;
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
        self.executions.groups.detach(observer, error)
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
        self.executions.groups.complete(execution, project)
    }

    pub(in crate::engine) fn expire_executions(&mut self, now: f64) -> Vec<Sealed> {
        let mut sealed = Vec::new();
        // Iterate the groups themselves, in issue order, so expiry cannot
        // depend on root bookkeeping.
        for execution in self.executions.groups.executions() {
            sealed.extend(self.executions.groups.expire(execution, now));
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
