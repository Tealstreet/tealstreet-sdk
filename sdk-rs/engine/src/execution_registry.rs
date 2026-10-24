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
    /// Background read Context rooting each background execution.
    backgrounds: HashMap<u64, Context>,
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
        if !matches!(&owner.phase, CommandPhase::Context { root: issued, entry_owned: true, .. } if issued == root)
        {
            return Err(Error::Revoked);
        }
        if owner.pending != 0
            || owner.deliveries != 0
            || owner.waits != 0
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
        self.check_context_capacity(
            self.contexts
                .len()
                .checked_add(self.reserved_context_count())
                .and_then(|n| n.checked_add(1)),
            None,
        )?;
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
        let member = self.observer_member(founder, key, now, false)?;
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
        let planned = self.executions.groups.planned(execution);
        let member = self.observer_member(root, key, now, planned)?;
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
            .and_then(|(identity, _)| {
                self.owner_entry(Owner::Execution {
                    generation: *identity,
                })
            })
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

    /// Whether `owner` is the live physical Plan that exactly `founder`
    /// (command and generation) founded; never another execution's member.
    #[cfg(feature = "http-plans")]
    pub(in crate::engine) fn founded_by(&self, owner: Owner, founder: Owner) -> bool {
        let (
            Owner::Execution {
                generation: identity,
            },
            Owner::Command { id, generation },
        ) = (owner, founder)
        else {
            return false;
        };
        self.physical_owner(owner)
            && self
                .executions
                .roots
                .get(&identity)
                .is_some_and(|execution| {
                    self.executions.groups.planned(*execution)
                        && self.executions.groups.member(id) == Some((*execution, generation))
                })
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
        fanout: &mut [(Result<String, VenueError>, Vec<String>)],
        sealed: &mut Vec<Sealed>,
    ) {
        let Owner::Execution {
            generation: identity,
        } = plan.owner()
        else {
            return;
        };
        self.contexts.remove(&identity);
        let Some(execution) = self.executions.roots.get(&identity).copied() else {
            return;
        };
        let mut prepared = fanout.iter_mut();
        let _ = self.executions.groups.complete_into(
            execution,
            |_| {
                let (terminal, _) = prepared
                    .next()
                    .expect("one prepared terminal per surviving observer");
                std::mem::replace(terminal, Ok(String::new()))
            },
            sealed,
        );
    }

    /// Surviving observers of a physical Plan's execution, for preparation.
    #[cfg(feature = "http-plans")]
    pub(in crate::engine) fn planned_observer_count(
        &self,
        plan: &crate::request_context::Plan,
    ) -> usize {
        let Owner::Execution {
            generation: identity,
        } = plan.owner()
        else {
            return 0;
        };
        self.executions.roots.get(&identity).map_or(0, |execution| {
            self.executions.groups.observer_count(*execution)
        })
    }

    /// A live physical Plan whose execution has no observer left; the engine
    /// fails it outside any delivery so nothing more is sent or published.
    #[cfg(feature = "http-plans")]
    pub(in crate::engine) fn orphaned_plan(&self) -> Option<crate::request_context::Plan> {
        self.physical.iter().find_map(|(identity, owner)| {
            if owner.physical
                != Some(Owner::Execution {
                    generation: *identity,
                })
            {
                return None;
            }
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
