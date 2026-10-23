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
        if self.contexts.len() >= CAPACITY {
            return Err(Error::Capacity);
        }
        if self.contexts.contains_key(&identity) {
            return Err(Error::Duplicate);
        }
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
        // Deterministic notice order across executions expiring together.
        let mut roots = self.executions.roots.iter().collect::<Vec<_>>();
        roots.sort_unstable_by_key(|(identity, _)| **identity);
        for (_, execution) in roots {
            sealed.extend(self.executions.groups.expire(*execution, now));
        }
        let groups = &self.executions.groups;
        self.executions
            .roots
            .retain(|_, execution| groups.deadline(*execution).is_some());
        sealed
    }

    pub(in crate::engine) fn next_execution_deadline(&self) -> Option<f64> {
        self.executions
            .roots
            .values()
            .filter_map(|execution| self.executions.groups.timer(*execution))
            .min_by(f64::total_cmp)
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
