//! Registry custody for execution groups: roots, observers and leg accounting.
//! Observers are entry-owned command roots; legs are `Owner::Execution`.
use super::*;
use crate::execution_group::{Execution, Groups, Joined, Member, Observer, Root, Sealed, Window};

#[derive(Default)]
pub(super) struct Executions {
    groups: Groups,
    /// Execution root identity (its `Owner::Execution` generation) to group.
    roots: crate::IdMap<u64, Execution>,
    observers: crate::IdMap<CommandId, Observer>,
    /// Background read Context rooting each background execution.
    backgrounds: crate::IdMap<u64, Context>,
}

fn root_identity(context: &Context) -> Option<u64> {
    match context.owner() {
        Owner::Execution { generation } => Some(generation),
        _ => None,
    }
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
            self.executions.backgrounds.clear();
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

    /// Live send/publication authority for an execution-owned handle. A
    /// background root publishes only while its own Context is still issued;
    /// after that only surviving command observers keep the execution live.
    pub(in crate::engine) fn execution_live(&self, context: &Context) -> bool {
        let Some(execution) = self.execution_of(context) else {
            return false;
        };
        let groups = &self.executions.groups;
        groups.may_publish(execution)
            && root_identity(context)
                .and_then(|identity| self.executions.backgrounds.get(&identity))
                .is_none_or(|background| {
                    self.validate_handle(background).is_ok() || groups.has_observers(execution)
                })
    }

    /// One live execution per background Context.
    fn background_in_use(&self, background: &Context) -> bool {
        self.executions
            .backgrounds
            .iter()
            .any(|(identity, rooted)| {
                rooted == background
                    && self
                        .executions
                        .roots
                        .get(identity)
                        .is_some_and(|execution| self.executions.groups.may_publish(*execution))
            })
    }

    fn prune_roots(&mut self) {
        let groups = &self.executions.groups;
        self.executions
            .roots
            .retain(|_, execution| groups.deadline(*execution).is_some());
        let roots = &self.executions.roots;
        self.executions
            .backgrounds
            .retain(|identity, _| roots.contains_key(identity));
    }

    /// Exclusive whole-command composition: an entry-owned root with no other
    /// issued handle, attempt, delivery or journal.
    fn observer_member(&self, root: &Context, key: &str, now: f64) -> Result<Member, Error> {
        self.validate(root, now)?;
        let Owner::Command { id, generation } = root.owner() else {
            return Err(Error::Invalid);
        };
        let owner = self.commands.get(&id).ok_or(Error::Revoked)?;
        if !matches!(&owner.phase, CommandPhase::Context { root: issued, entry_owned: true, .. } if issued == root)
        {
            return Err(Error::Revoked);
        }
        if owner.pending != 0
            || owner.deliveries != 0
            || owner.waits != 0
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

    /// The engine-issued Owner::Execution root for a validated group bound.
    fn execution_root(
        &mut self,
        identity: u64,
        policy: &Policy,
        deadline: f64,
        now: f64,
    ) -> Result<Context, Error> {
        self.contexts.retain(|_, old| old.deadline() > now);
        // Plan-reserved identities are promised; a root may not take them.
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
        Context::new(
            Owner::Execution {
                generation: identity,
            },
            identity,
            Operation::Read,
            policy.credential.clone(),
            now,
            deadline,
            policy.pacing,
        )?
        .with_retry(policy.retry)
    }

    fn commit_root(&mut self, mut root: Context, execution: Execution) -> Context {
        root.bind_issuer(&self.issuer);
        self.contexts.insert(root.leg(), root.clone());
        self.executions.roots.insert(root.leg(), execution);
        root
    }

    /// A live background read becomes the root of a read execution. It keeps
    /// its own publication rights; command observers receive projections only.
    pub(in crate::engine) fn begin_background_execution(
        &mut self,
        identity: u64,
        background: &Context,
        policy: Policy,
        window: Window,
        now: f64,
    ) -> Result<Context, Error> {
        if policy.operation != Operation::Read
            || background.operation() != Operation::Read
            || !matches!(background.owner(), Owner::Background { .. })
            || policy.timeout_ms == 0
            || !now.is_finite()
        {
            return Err(Error::Invalid);
        }
        self.validate(background, now)?;
        if self.background_in_use(background) {
            return Err(Error::State);
        }
        let deadline = background
            .deadline()
            .min(now + f64::from(policy.timeout_ms));
        let root = self.execution_root(identity, &policy, deadline, now)?;
        let (execution, _) = self.executions.groups.begin(
            Root::Background {
                deadline: background.deadline(),
            },
            Operation::Read,
            window,
            policy.timeout_ms,
            now,
        )?;
        debug_assert_eq!(self.executions.groups.deadline(execution), Some(deadline));
        self.executions
            .backgrounds
            .insert(identity, background.clone());
        Ok(self.commit_root(root, execution))
    }

    /// The next read of the same background root adopts owners that joined
    /// after the previous read was released. Returns any already expired.
    pub(in crate::engine) fn begin_successor_execution(
        &mut self,
        identity: u64,
        previous: &Context,
        background: &Context,
        policy: Policy,
        now: f64,
    ) -> Result<(Context, Vec<Sealed>), Error> {
        if policy.operation != Operation::Read
            || !matches!(background.owner(), Owner::Background { .. })
            || background.operation() != Operation::Read
            || policy.timeout_ms == 0
            || !now.is_finite()
        {
            return Err(Error::Invalid);
        }
        if !previous.issued_by(&self.issuer) {
            return Err(Error::Revoked);
        }
        let old = self
            .execution_of(previous)
            .filter(|old| self.executions.groups.deadline(*old).is_some())
            .ok_or(Error::Revoked)?;
        // Queued owners need the same account's next snapshot.
        let rooted = self
            .executions
            .backgrounds
            .get(&root_identity(previous).expect("execution handle"))
            .ok_or(Error::Invalid)?;
        if rooted.credential() != background.credential() {
            return Err(Error::Invalid);
        }
        self.validate(background, now)?;
        if self.background_in_use(background) {
            return Err(Error::State);
        }
        let deadline = background
            .deadline()
            .min(now + f64::from(policy.timeout_ms));
        let root = self.execution_root(identity, &policy, deadline, now)?;
        let (execution, sealed) = self.executions.groups.begin_successor(
            old,
            background.deadline(),
            policy.timeout_ms,
            now,
        )?;
        self.executions
            .backgrounds
            .insert(identity, background.clone());
        let root = self.commit_root(root, execution);
        self.prune_roots();
        Ok((root, sealed))
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
        // Validate the root before committing any group state.
        let deadline = founder.deadline().min(now + f64::from(policy.timeout_ms));
        let root = self.execution_root(identity, &policy, deadline, now)?;
        let command = member.command;
        let (execution, observer) = self.executions.groups.begin(
            Root::Command(member),
            Operation::Read,
            window,
            policy.timeout_ms,
            now,
        )?;
        debug_assert_eq!(self.executions.groups.deadline(execution), Some(deadline));
        let root = self.commit_root(root, execution);
        self.executions
            .observers
            .insert(command, observer.expect("command-rooted execution"));
        self.observe_execution_credential(command, &root);
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
        self.observe_execution_credential(command, execution_root);
        Ok(joined)
    }

    /// An observer seals through the shared execution: a signed execution's
    /// auth-kind failure is credential evidence for every observing command.
    fn observe_execution_credential(&mut self, command: CommandId, execution_root: &Context) {
        if execution_root.credential().is_some()
            && let Some(owner) = self.commands.get_mut(&command)
        {
            owner.credentialed = true;
        }
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
        self.prune_roots();
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
