//! Engine-issued logical context identities. Physical attempts and transport
//! resources are registered only after this immutable logical bound is valid.
use super::{CommandId, Cx, RequestId};
use crate::VenueError;
use crate::request_context::{
    Attempt, CommandWrites, Context, Egress, Error, Operation, Owner, Policy, RejectionReports,
    ResendGrant, RetryEvidence, Scope, WriteResult,
};
#[cfg(feature = "http-plans")]
use crate::request_context::{
    HttpOwnerRetirement, Plan, PlanPolicy, Preflight, PreflightPolicy, PreflightPurpose,
};
#[cfg(feature = "http-plans")]
use std::collections::VecDeque;

const CAPACITY: usize = 256;

#[path = "local_setting.rs"]
mod local_setting;

#[path = "session_context.rs"]
pub(super) mod session;

#[cfg(all(test, feature = "http-plans"))]
#[path = "plan_receipt_cap_tests.rs"]
mod receipt_cap_tests;

#[cfg(all(test, feature = "http-plans"))]
#[path = "request_context_preflight_tests.rs"]
mod preflight_tests;

#[cfg(all(test, feature = "http-plans"))]
#[path = "request_context_preflight_entry_tests.rs"]
mod preflight_entry_tests;

#[cfg(test)]
#[path = "request_context_replacement_registry_tests.rs"]
mod replacement_tests;

#[cfg(test)]
#[path = "request_context_read_delivery_tests.rs"]
mod read_delivery_tests;

#[path = "execution_registry.rs"]
mod execution;

#[cfg(test)]
#[path = "request_context_background_delivery_tests.rs"]
mod background_delivery_tests;

struct CommandOwner {
    generation: u64,
    /// Exact logical completion authority survives expired-handle cleanup.
    phase: CommandPhase,
    /// Owner retention only. This never extends any issued logical deadline.
    last_deadline: f64,
    writes: Option<CommandWrites>,
    mutation_declared: bool,
    pending: usize,
    deliveries: usize,
    /// Session waiters keep completion authority until the command finishes:
    /// a drained Failed waiter must still be able to fail its command.
    waits: usize,
    /// The logical command's intent, captured when its first context is
    /// issued during command dispatch. `None`: issued outside dispatch.
    intent: Option<crate::classify::Intent>,
    /// Engine-validated narrowing for individual legs (see `http_write_leg_intent`).
    leg_intents: Vec<(u64, crate::classify::Intent)>,
    /// Some Context or declared policy of this owner named a credential; only
    /// then can its auth-kind failure be credential-health evidence.
    credentialed: bool,
    /// An earlier attempt of this intent may have reached the venue. Every
    /// journal this owner creates projects its failure at least Unknown.
    inherited_uncertain: bool,
}

enum CommandPhase {
    LocalSetting,
    Context {
        root: Context,
        entry_owned: bool,
        /// A caller-wait Read root the engine expires at its deadline.
        tracked: bool,
        /// Past its deadline: no send, publication or venue completion.
        expired: bool,
    },
    #[cfg(feature = "http-plans")]
    Preflight(preflight::State),
    #[cfg(feature = "http-plans")]
    Planned(planned::State),
    #[cfg(feature = "http-plans")]
    Retiring(HttpOwnerRetirement),
}

impl CommandOwner {
    /// One policy for issued Contexts and prospective Plan legs. Callers hold
    /// the exact live owner and commit synchronously, without a callback gap.
    /// `protection_cancel` is the one declared amend-cancel exception (see
    /// `Cx::http_write_protection_cancel_leg`); every other caller passes false.
    fn prepare_leg_intent(
        &self,
        context: &Context,
        intent: crate::classify::Intent,
        protection_cancel: bool,
    ) -> Result<bool, Error> {
        use crate::classify::Intent;
        if self
            .leg_intents
            .iter()
            .any(|(leg, _)| *leg == context.leg())
        {
            return Err(Error::Duplicate);
        }
        if self
            .writes
            .as_ref()
            .is_some_and(|writes| writes.latest(context.leg()).is_some())
        {
            return Err(Error::State);
        }
        if self.intent == Some(intent) && context.operation() == Operation::Mutation {
            return Ok(false);
        }
        let allowed = match (self.intent, intent) {
            (Some(Intent::Amend), Intent::Cancel) => protection_cancel,
            (Some(Intent::Amend), Intent::Place | Intent::Setting)
            | (Some(Intent::Place), Intent::Cancel | Intent::Setting) => !protection_cancel,
            _ => false,
        };
        if !allowed || context.operation() != Operation::Mutation {
            return Err(Error::Invalid);
        }
        if self.leg_intents.len() >= CAPACITY {
            return Err(Error::Capacity);
        }
        Ok(true)
    }

    fn context_root(&self) -> Option<&Context> {
        match &self.phase {
            CommandPhase::Context { root, .. } => Some(root),
            CommandPhase::LocalSetting => None,
            #[cfg(feature = "http-plans")]
            _ => None,
        }
    }

    fn retained(&self) -> bool {
        match &self.phase {
            CommandPhase::Context { entry_owned, .. } => *entry_owned,
            CommandPhase::LocalSetting => true,
            #[cfg(feature = "http-plans")]
            CommandPhase::Preflight(_) | CommandPhase::Planned(_) | CommandPhase::Retiring(_) => {
                true
            }
        }
    }
}

#[cfg(feature = "http-plans")]
#[path = "request_context_owned_call.rs"]
mod owned_call;
#[cfg(feature = "http-plans")]
#[path = "request_context_preflight.rs"]
mod preflight;
#[cfg(feature = "http-plans")]
pub(super) use owned_call::Admission as OwnedAdmission;
#[cfg(feature = "http-plans")]
pub(super) use owned_call::ReleasedReadFailure;

#[cfg(feature = "http-plans")]
#[path = "request_context_planned.rs"]
mod planned;
#[cfg(feature = "http-plans")]
#[cfg(feature = "http-mutation-barriers")]
pub(in crate::engine) use planned::MutationGroupKey;

#[cfg(not(feature = "http-plans"))]
#[path = "request_context_without_plans.rs"]
mod without_plans;

#[derive(Clone, Debug)]
pub(super) struct Delivery {
    pub context: Context,
    pub attempt: Attempt,
}

impl PartialEq for Delivery {
    fn eq(&self, other: &Self) -> bool {
        self.context.same_issuer(&other.context)
            && self.context == other.context
            && self.attempt == other.attempt
    }
}

pub(super) enum Seal {
    Legacy(Option<VenueError>),
    Stale,
    Success,
    Failure(VenueError, Vec<String>),
}

#[derive(Default)]
pub(super) struct ContextRegistry {
    issuer: std::sync::Arc<()>,
    contexts: crate::IdMap<u64, Context>,
    commands: std::collections::HashMap<CommandId, CommandOwner>,
    sessions: session::Sessions,
    /// Session generation each Bearer-bound leg carries; same-leg retries and
    /// resends inherit it. Leg identities are never reused.
    bearer_legs: std::collections::HashMap<u64, u64>,
    /// The command being dispatched to the venue and its intent.
    pub(super) dispatch: Option<(CommandId, crate::classify::Intent)>,
    executions: execution::Executions,
    #[cfg(feature = "http-plans")]
    retirements: VecDeque<HttpOwnerRetirement>,
    #[cfg(feature = "http-plans")]
    preflight_ready: VecDeque<Preflight>,
    #[cfg(feature = "http-plans")]
    preflight_initial: Vec<Preflight>,
    /// Scoped delivery loans outlive temporary marker removal and even owner
    /// cleanup. Readiness may run only after the true outermost scope exits.
    #[cfg(feature = "http-plans")]
    delivery_depth: usize,
    /// Logical identities promised to admitted plans but not yet issued.
    #[cfg(feature = "http-plans")]
    reserved_contexts: usize,
    // Only admitted Plan owners; capacity is reserved before ID commitment.
    #[cfg(feature = "http-plans")]
    plan_index: Vec<Owner>,
    /// Plan-backed executions' physical owners, keyed by execution identity.
    /// They hold the Plan, its slots and journal; no command observes them.
    #[cfg(feature = "http-plans")]
    physical: crate::IdMap<u64, CommandOwner>,
    #[cfg(all(feature = "http-plans", any(test, feature = "test-support")))]
    plan_scan_visits: std::cell::Cell<usize>,
    /// Order-read epoch at each live owner's first released Read: one logical
    /// read (pages, coins, products as legs) is only as fresh as its oldest leg.
    #[cfg(feature = "order-locks")]
    read_marks: Vec<(Owner, u64)>,
}

impl ContextRegistry {
    /// The order-read token for a Read released now under `owner`: the epoch of
    /// its earliest released Read. A mark only ever keeps a fence longer.
    #[cfg(feature = "order-locks")]
    pub(super) fn read_mark(&mut self, owner: Owner, epoch: u64) -> u64 {
        if let Some((_, mark)) = self.read_marks.iter().find(|(held, _)| *held == owner) {
            return *mark;
        }
        self.read_marks.push((owner, epoch));
        epoch
    }
    #[cfg(all(test, feature = "order-locks"))]
    pub(super) fn read_mark_count(&self) -> usize {
        self.read_marks.len()
    }
    /// A mark lives while its owner does: a live context, or a command owner in
    /// any phase (a serial Plan between steps has no current context).
    #[cfg(feature = "order-locks")]
    fn forget_dead_read_marks(&mut self) {
        if self.read_marks.is_empty() {
            return;
        }
        let (contexts, commands) = (&self.contexts, &self.commands);
        self.read_marks.retain(|(owner, _)| {
            matches!(owner, Owner::Command { id, generation }
                if commands.get(id).is_some_and(|live| live.generation == *generation))
                || contexts.values().any(|context| context.owner() == *owner)
        });
    }
    fn forget_read_mark(&mut self, _owner: Owner) {
        #[cfg(feature = "order-locks")]
        self.read_marks.retain(|(held, _)| *held != _owner);
    }
    pub(super) fn prune(&mut self, now: f64) {
        self.contexts.retain(|_, old| old.deadline() > now);
        self.commands.retain(|_, old| {
            old.retained()
                || old.last_deadline > now
                || old.writes.is_some()
                || old.pending > 0
                || old.deliveries > 0
                || old.waits > 0
        });
        #[cfg(feature = "order-locks")]
        self.forget_dead_read_marks();
    }
    pub(super) fn command_entry_unowned(&self, command: CommandId) -> bool {
        !self.commands.contains_key(&command)
    }

    pub(super) fn prepare_command_entry(&mut self, command: CommandId, now: f64) -> bool {
        let Some(owner) = self.commands.get(&command) else {
            return true;
        };
        if !now.is_finite()
            || owner.retained()
            || now < owner.last_deadline
            || owner.writes.is_some()
            || owner.pending != 0
            || owner.deliveries != 0
            || owner.waits != 0
        {
            return false;
        }
        let old = Owner::Command {
            id: command,
            generation: owner.generation,
        };
        self.commands.remove(&command);
        self.contexts.retain(|_, context| context.owner() != old);
        self.forget_read_mark(old);
        true
    }

    pub(super) fn command_live(&self, id: CommandId, now: f64) -> bool {
        self.commands.get(&id).is_some_and(|owner| {
            !now.is_finite()
                || owner.retained()
                || now < owner.last_deadline
                || owner.writes.is_some()
                || owner.pending > 0
                || owner.deliveries > 0
                || owner.waits > 0
        })
    }

    pub(super) fn retain_command_entry(&mut self, context: &Context) {
        assert!(
            context.issued_by(&self.issuer),
            "foreign command entry root"
        );
        let Owner::Command { id, generation } = context.owner() else {
            panic!("command entry must issue a command root");
        };
        let owner = self.commands.get_mut(&id).expect("issued command entry");
        assert_eq!(owner.generation, generation);
        let (root, entry_owned) = match &mut owner.phase {
            CommandPhase::Context {
                root, entry_owned, ..
            } => (root, entry_owned),
            _ => panic!("command entry must retain a Context root"),
        };
        // Plain comparisons: release WASM carries no Debug for Context or Owner.
        assert!(root == context, "command entry retains its own root");
        assert!(!*entry_owned, "command entry already retained");
        *entry_owned = true;
    }

    /// Whether this command's auth-kind failure is credential-health evidence:
    /// a tracked owner only when a Context or declared policy named a
    /// credential; an untracked (legacy) command keeps counting.
    pub(in super::super) fn owner_credentialed(&self, command: CommandId) -> bool {
        self.commands
            .get(&command)
            .is_none_or(|owner| owner.credentialed)
    }

    #[cfg(test)]
    fn issue(
        &mut self,
        identity: u64,
        scope: Scope,
        policy: Policy,
        now: f64,
    ) -> Result<Context, Error> {
        self.issue_with_egress(identity, scope, policy, now, None, None)
    }

    /// Egress is captured here, before binding, so the registered context and
    /// completion root hold it exactly. Legs inherit their parent's egress.
    fn issue_with_egress(
        &mut self,
        identity: u64,
        scope: Scope,
        policy: Policy,
        now: f64,
        parent: Option<&Context>,
        egress: Option<Egress>,
    ) -> Result<Context, Error> {
        self.issue_until(identity, scope, policy, now, parent, egress, None)
    }

    /// `until` is an exact absolute deadline that replaces the policy timeout
    /// (the caller's raw receipt), never rounded through whole milliseconds.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn issue_until(
        &mut self,
        identity: u64,
        scope: Scope,
        policy: Policy,
        now: f64,
        parent: Option<&Context>,
        egress: Option<Egress>,
        until: Option<f64>,
    ) -> Result<Context, Error> {
        if let Some(parent) = parent {
            self.validate_send(parent, now)?;
            if parent.operation() == Operation::Read && policy.operation == Operation::Mutation {
                return Err(Error::Invalid);
            }
        }
        let egress = match (egress, parent.and_then(Context::egress)) {
            (Some(own), Some(inherited)) if own != inherited => return Err(Error::Invalid),
            (own, inherited) => own.or(inherited),
        };
        if let Some(Egress::Session(session)) = egress
            && !self.sessions.live(session.generation())
        {
            return Err(Error::Revoked);
        }
        if scope == Scope::Background && policy.operation == Operation::Mutation {
            return Err(Error::Invalid);
        }
        // A member's only physical work is its execution's.
        if matches!(scope, Scope::Command(id) if self.is_observer(id)) {
            return Err(Error::Revoked);
        }
        if (scope == Scope::Execution)
            != parent.is_some_and(|parent| matches!(parent.owner(), Owner::Execution { .. }))
        {
            return Err(Error::Invalid);
        }
        // A Plan-backed execution's physical work is its Plan slots only.
        if parent.is_some_and(|parent| self.planned_execution_root(parent)) {
            return Err(Error::Revoked);
        }
        if !now.is_finite() || policy.timeout_ms == 0 {
            return Err(Error::Invalid);
        }
        // Abandoned handles must not permanently exhaust the bounded registry.
        // Retiring expired metadata is safe even if the new policy is refused.
        // A delayed original timer must not erase sent evidence or retained
        // childless generations when unrelated work is issued.
        self.prune(now);
        if matches!(scope, Scope::Command(id) if self.commands.get(&id)
            .is_some_and(|owner| !matches!(owner.phase, CommandPhase::Context { .. }) || ((owner.writes.is_some() || owner.pending > 0 || owner.deliveries > 0 || owner.waits > 0) && now >= owner.last_deadline)))
        {
            return Err(Error::State);
        }
        // A tracked caller-wait root stays plain Read: a Timeout seal must
        // never erase a released write.
        if policy.operation == Operation::Mutation
            && matches!(scope, Scope::Command(id) if self.tracked(id))
        {
            return Err(Error::Invalid);
        }
        let owner = match parent.map(Context::owner) {
            Some(owner) => owner,
            None => match scope {
                Scope::Command(id) => Owner::Command {
                    id,
                    generation: self
                        .commands
                        .get(&id)
                        .map_or(identity, |old| old.generation),
                },
                Scope::Background => Owner::Background {
                    generation: identity,
                },
                Scope::Execution => return Err(Error::Invalid),
            },
        };
        self.commit(owner, identity, policy, now, parent, egress, until)
    }

    fn issue_session(
        &mut self,
        identity: u64,
        generation: u64,
        policy: Policy,
        now: f64,
        parent: Option<&Context>,
    ) -> Result<Context, Error> {
        if let Some(parent) = parent {
            self.validate_send(parent, now)?;
            if parent.operation() == Operation::Read && policy.operation == Operation::Mutation {
                return Err(Error::Invalid);
            }
            // A leg never moves to another session, nor takes its egress.
            if parent.owner() != (Owner::Session { generation }) {
                return Err(Error::Invalid);
            }
        }
        if !now.is_finite() || policy.timeout_ms == 0 {
            return Err(Error::Invalid);
        }
        self.sessions.admit(generation, &policy)?;
        self.contexts.retain(|_, old| old.deadline() > now);
        self.sessions.prune(&self.contexts);
        let egress = self.sessions.egress(generation);
        self.commit(
            Owner::Session { generation },
            identity,
            policy,
            now,
            parent,
            egress,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn commit(
        &mut self,
        owner: Owner,
        identity: u64,
        policy: Policy,
        now: f64,
        parent: Option<&Context>,
        egress: Option<Egress>,
        until: Option<f64>,
    ) -> Result<Context, Error> {
        let deadline = until.unwrap_or(now + f64::from(policy.timeout_ms));
        let deadline = parent.map_or(deadline, |parent| parent.deadline().min(deadline));
        if let Owner::Command { id, .. } = owner
            && let Some(root) = self.commands.get(&id).and_then(|owner| match &owner.phase {
                CommandPhase::Context {
                    root,
                    entry_owned: true,
                    ..
                } => Some(root),
                _ => None,
            })
        {
            if now >= root.deadline() {
                return Err(Error::State);
            }
            if deadline > root.deadline() {
                return Err(Error::Invalid);
            }
        }
        // Validate before committing an owner, context, pending slot or action.
        let mut context = Context::new(
            owner,
            identity,
            policy.operation,
            policy.credential,
            now,
            deadline,
            policy.pacing,
        )?
        .with_retry(policy.retry)?
        .with_egress(egress)?;
        if self.contexts.len() + self.reserved_context_count() >= CAPACITY
            || matches!(owner, Owner::Command { id, .. }
                if !self.commands.contains_key(&id) && self.commands.len() >= CAPACITY)
        {
            return Err(Error::Capacity);
        }
        if self.contexts.contains_key(&identity) {
            return Err(Error::Duplicate);
        }
        self.contexts
            .try_reserve(1 + self.reserved_context_count())
            .map_err(|_| Error::Capacity)?;
        if matches!(owner, Owner::Command { id, .. } if !self.commands.contains_key(&id)) {
            self.commands.try_reserve(1).map_err(|_| Error::Capacity)?;
        }
        context.bind_issuer(&self.issuer);
        if let Owner::Command { id, generation } = owner {
            let last_deadline = self.commands.get(&id).map_or(context.deadline(), |old| {
                old.last_deadline.max(context.deadline())
            });
            self.commands
                .entry(id)
                .and_modify(|owner| {
                    owner.last_deadline = last_deadline;
                    owner.mutation_declared |= context.operation() == Operation::Mutation;
                    owner.credentialed |= context.credential().is_some();
                })
                .or_insert(CommandOwner {
                    generation,
                    phase: CommandPhase::Context {
                        root: context.clone(),
                        entry_owned: false,
                        tracked: false,
                        expired: false,
                    },
                    last_deadline,
                    writes: None,
                    mutation_declared: context.operation() == Operation::Mutation,
                    pending: 0,
                    deliveries: 0,
                    waits: 0,
                    intent: self
                        .dispatch
                        .filter(|(command, _)| *command == id)
                        .map(|(_, intent)| intent),
                    leg_intents: Vec::new(),
                    credentialed: context.credential().is_some(),
                    inherited_uncertain: false,
                });
        }
        self.contexts.insert(identity, context.clone());
        Ok(context)
    }

    fn replace(
        &mut self,
        parent: &Context,
        delivery: &Delivery,
        first: u64,
        policies: &[Policy],
        now: f64,
    ) -> Result<Vec<Context>, Error> {
        self.validate_send(parent, now)?;
        if parent.operation() != Operation::Mutation || delivery.context != *parent {
            return Err(Error::Revoked);
        }
        parent.retry(RetryEvidence::DefinitiveRefusal, now)?;
        let Owner::Command { id, generation } = parent.owner() else {
            return Err(Error::Invalid);
        };
        if policies.is_empty() {
            return Err(Error::Invalid);
        }
        if policies.len() > CAPACITY
            || first == 0
            || first
                .checked_add(policies.len() as u64)
                .is_none_or(|end| end > crate::resource_ids::MAX_RESOURCE_ID + 1)
            || self
                .contexts
                .values()
                .filter(|context| context.deadline() > now)
                .count()
                + policies.len()
                + self.reserved_context_count()
                > CAPACITY
        {
            return Err(Error::Capacity);
        }
        let owner = self.commands.get(&id).ok_or(Error::Revoked)?;
        let writes = owner.writes.as_ref().ok_or(Error::State)?;
        if owner.generation != generation
            || writes.latest(parent.leg()) != Some(delivery.attempt.request)
        {
            return Err(Error::Revoked);
        }
        let mut children = Vec::new();
        children
            .try_reserve_exact(policies.len())
            .map_err(|_| Error::Capacity)?;
        let mut legs = Vec::new();
        legs.try_reserve_exact(policies.len())
            .map_err(|_| Error::Capacity)?;
        for (index, policy) in policies.iter().enumerate() {
            if policy.operation != Operation::Mutation
                || policy.credential.as_ref() != parent.credential()
                || policy.pacing != parent.pacing()
                || policy.timeout_ms == 0
            {
                return Err(Error::Invalid);
            }
            let leg = first + index as u64;
            if self.contexts.contains_key(&leg) {
                return Err(Error::Duplicate);
            }
            let mut child = Context::new(
                parent.owner(),
                leg,
                policy.operation,
                policy.credential.clone(),
                now,
                parent.deadline().min(now + f64::from(policy.timeout_ms)),
                policy.pacing,
            )?
            .with_retry(policy.retry)?
            .with_egress(parent.egress())?;
            child.bind_issuer(&self.issuer);
            legs.push(leg);
            children.push(child);
        }
        // Reserve the pre-cleanup peak. Nothing visible has changed yet, and
        // journal.replace is the final fallible operation in the transaction.
        self.contexts
            .try_reserve(children.len() + self.reserved_context_count())
            .map_err(|_| Error::Capacity)?;
        self.commands
            .get_mut(&id)
            .expect("validated owner")
            .writes
            .as_mut()
            .expect("validated journal")
            .replace(delivery.attempt.request, &legs)?;
        self.contexts.retain(|_, context| context.deadline() > now);
        #[cfg(feature = "order-locks")]
        self.forget_dead_read_marks();
        self.contexts.extend(
            children
                .iter()
                .cloned()
                .map(|context| (context.leg(), context)),
        );
        Ok(children)
    }

    fn superseded(&self, context: &Context) -> bool {
        let Owner::Command { id, .. } = context.owner() else {
            return false;
        };
        self.commands
            .get(&id)
            .and_then(|owner| owner.writes.as_ref())
            .is_some_and(|writes| writes.superseded(context.leg()))
    }

    pub(super) fn leg_bearer(&self, context: &Context) -> Option<u64> {
        self.bearer_legs.get(&context.leg()).copied()
    }

    /// Prune bindings of retired legs, then reserve one slot before any effect.
    pub(super) fn reserve_leg_bearer(&mut self) -> Result<(), Error> {
        if self.bearer_legs.len() >= CAPACITY {
            let contexts = &self.contexts;
            self.bearer_legs.retain(|leg, _| contexts.contains_key(leg));
        }
        self.bearer_legs.try_reserve(1).map_err(|_| Error::Capacity)
    }

    pub(super) fn bind_leg_bearer(&mut self, leg: u64, generation: u64) {
        self.bearer_legs.insert(leg, generation);
    }

    pub(super) fn validate(&self, context: &Context, now: f64) -> Result<(), Error> {
        if !now.is_finite() {
            return Err(Error::Invalid);
        }
        if now >= context.deadline() {
            return Err(Error::TimedOut);
        }
        self.validate_handle(context)?;
        if self.superseded(context) {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    /// `validate` plus the session-egress retirement fence, for issuance and
    /// physical send only. Delivery, streaming and write facts of an attempt
    /// sent earlier use plain `validate`, so its outcome stays recordable.
    pub(super) fn validate_send(&self, context: &Context, now: f64) -> Result<(), Error> {
        self.validate(context, now)?;
        self.egress_live(context)
    }

    /// The session-egress retirement fence alone.
    pub(super) fn egress_live(&self, context: &Context) -> Result<(), Error> {
        if let Some(Egress::Session(session)) = context.egress()
            && !self.sessions.live(session.generation())
        {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    /// Retirement authority only; expiry cannot create fresh send permission.
    fn validate_handle(&self, context: &Context) -> Result<(), Error> {
        if matches!(context.owner(), Owner::Command { id, .. } if self.expired(id)) {
            return Err(Error::Revoked);
        }
        if !context.issued_by(&self.issuer)
            || self.contexts.get(&context.leg()) != Some(context)
            || matches!(context.owner(), Owner::Command {id, generation}
                if self.commands.get(&id).is_none_or(|owner| owner.generation != generation))
            || matches!(context.owner(), Owner::Session { generation }
                if !self.sessions.live(generation))
            || (matches!(context.owner(), Owner::Execution { .. }) && !self.execution_live(context))
        {
            return Err(Error::Revoked);
        }
        self.validate_owned_context(context)?;
        Ok(())
    }

    /// The phase/journal entry of an issued owner: a command generation, or a
    /// Plan-backed execution's physical owner. Other owners have none.
    fn owner_entry(&self, owner: Owner) -> Option<&CommandOwner> {
        match owner {
            Owner::Command { id, generation } => self
                .commands
                .get(&id)
                .filter(|owner| owner.generation == generation),
            #[cfg(feature = "http-plans")]
            Owner::Execution { generation } => self.physical.get(&generation),
            _ => None,
        }
    }

    fn owner_entry_mut(&mut self, owner: Owner) -> Option<&mut CommandOwner> {
        match owner {
            Owner::Command { id, generation } => self
                .commands
                .get_mut(&id)
                .filter(|owner| owner.generation == generation),
            #[cfg(feature = "http-plans")]
            Owner::Execution { generation } => self.physical.get_mut(&generation),
            _ => None,
        }
    }

    /// Whether `owner` is a Plan-backed execution's physical owner.
    fn physical_owner(&self, owner: Owner) -> bool {
        #[cfg(feature = "http-plans")]
        if let Owner::Execution { generation } = owner {
            return self.physical.contains_key(&generation);
        }
        let _ = owner;
        false
    }

    fn command_mut(&mut self, context: &Context) -> Result<&mut CommandOwner, Error> {
        if !context.issued_by(&self.issuer) {
            return Err(Error::Revoked);
        }
        if !matches!(context.owner(), Owner::Command { .. })
            && !self.physical_owner(context.owner())
        {
            return Err(Error::Invalid);
        }
        self.owner_entry_mut(context.owner()).ok_or(Error::Revoked)
    }

    pub(super) fn register_attempt(
        &mut self,
        context: &Context,
        request: RequestId,
        now: f64,
    ) -> Result<(), Error> {
        self.validate(context, now)?;
        // A member's only physical work is its execution's, including on the
        // entry root it already holds.
        if matches!(context.owner(), Owner::Command { id, .. } if self.is_observer(id)) {
            return Err(Error::Revoked);
        }
        if matches!(context.owner(), Owner::Background { .. })
            && context.operation() == Operation::Read
        {
            return Ok(());
        }
        if matches!(context.owner(), Owner::Session { .. }) {
            return self.sessions.register(context, request);
        }
        if matches!(context.owner(), Owner::Execution { .. })
            && !self.physical_owner(context.owner())
        {
            return self.execution_leg_issued(context);
        }
        let owner = self.command_mut(context)?;
        let pending = owner.pending.checked_add(1).ok_or(Error::Capacity)?;
        if context.operation() == Operation::Mutation {
            #[cfg(feature = "http-plans")]
            let reserved = matches!(owner.phase, CommandPhase::Planned(_));
            #[cfg(not(feature = "http-plans"))]
            let reserved = false;
            if reserved {
                if owner
                    .writes
                    .as_ref()
                    .and_then(|writes| writes.latest(context.leg()))
                    != Some(request)
                {
                    return Err(Error::Revoked);
                }
            } else if let Some(writes) = owner.writes.as_mut() {
                if let Some(previous) = writes.latest(context.leg()) {
                    context.retry(RetryEvidence::DefinitiveRefusal, now)?;
                    writes.retry(previous, request)?;
                } else {
                    writes.register(request, context.leg())?;
                }
            } else {
                let mut writes =
                    CommandWrites::new(CAPACITY)?.inheriting(owner.inherited_uncertain);
                writes.register(request, context.leg())?;
                owner.writes = Some(writes);
            }
        }
        owner.pending = pending;
        Ok(())
    }

    pub(super) fn retire_attempt(&mut self, context: &Context, request: RequestId) {
        if matches!(context.owner(), Owner::Session { .. }) {
            self.sessions.retire_attempt(context, request);
            return;
        }
        if matches!(context.owner(), Owner::Execution { .. })
            && !self.physical_owner(context.owner())
        {
            self.execution_leg_retired(context);
            return;
        }
        if let Ok(owner) = self.command_mut(context) {
            owner.pending = owner
                .pending
                .checked_sub(1)
                .expect("registered physical attempt retired once");
        }
    }

    pub(super) fn enter_delivery(&mut self, delivery: &Delivery) {
        #[cfg(feature = "http-plans")]
        if delivery.context.issued_by(&self.issuer) {
            self.delivery_depth = self
                .delivery_depth
                .checked_add(1)
                .expect("bounded engine delivery nesting");
        }
        if let Ok(owner) = self.command_mut(&delivery.context) {
            owner.deliveries = owner
                .deliveries
                .checked_add(1)
                .expect("bounded engine delivery nesting");
        }
    }

    pub(super) fn exit_delivery(&mut self, delivery: &Delivery) {
        #[cfg(feature = "http-plans")]
        if delivery.context.issued_by(&self.issuer) {
            self.delivery_depth = self
                .delivery_depth
                .checked_sub(1)
                .expect("scoped engine delivery exited once");
        }
        if let Ok(owner) = self.command_mut(&delivery.context) {
            owner.deliveries = owner
                .deliveries
                .checked_sub(1)
                .expect("scoped engine delivery exited once");
        }
    }

    pub(super) fn release_flag(
        &mut self,
        context: &Context,
        request: RequestId,
    ) -> Result<Option<&mut bool>, Error> {
        if context.operation() != Operation::Mutation
            || matches!(context.owner(), Owner::Session { .. })
        {
            return Ok(None);
        }
        self.command_mut(context)?
            .writes
            .as_mut()
            .ok_or(Error::State)?
            .release_flag(request)
            .map(Some)
    }

    /// A parked owner must be able to re-send on Ready: an earlier attempt
    /// needs both the context's refusal policy and the journal's admission.
    pub(super) fn resend_blocked(&self, context: &Context, now: f64) -> bool {
        let Owner::Command { id, .. } = context.owner() else {
            return false;
        };
        let Some(writes) = self
            .commands
            .get(&id)
            .and_then(|owner| owner.writes.as_ref())
        else {
            return false;
        };
        context.operation() == Operation::Mutation
            && writes.latest(context.leg()).is_some()
            && (writes.resend_blocked(context.leg())
                || context
                    .retry(RetryEvidence::DefinitiveRefusal, now)
                    .is_err())
    }

    /// A command owner outside a Plan or Preflight; non-command owners pass.
    pub(super) fn context_phase(&self, context: &Context) -> bool {
        let Owner::Command { id, .. } = context.owner() else {
            return true;
        };
        self.commands
            .get(&id)
            .is_none_or(|owner| matches!(owner.phase, CommandPhase::Context { .. }))
    }

    pub(super) fn hold_wait(&mut self, context: &Context) {
        if let Ok(owner) = self.command_mut(context) {
            owner.waits += 1;
        }
    }

    pub(super) fn release_wait(&mut self, context: &Context) {
        if let Ok(owner) = self.command_mut(context) {
            owner.waits = owner.waits.saturating_sub(1);
        }
    }

    /// Session writes record release separately: they have no command journal.
    pub(super) fn session_released(&mut self, context: &Context, request: RequestId) {
        if context.operation() == Operation::Mutation {
            self.sessions.released(context, request);
        }
    }

    pub(super) fn result(&mut self, delivery: &Delivery, result: WriteResult) -> Result<(), Error> {
        if delivery.context.operation() != Operation::Mutation {
            return Err(Error::Invalid);
        }
        if matches!(delivery.context.owner(), Owner::Session { .. }) {
            self.validate_handle(&delivery.context)?;
            return self
                .sessions
                .result(&delivery.context, delivery.attempt.request, &result);
        }
        self.command_mut(&delivery.context)?
            .writes
            .as_mut()
            .ok_or(Error::State)?
            .result(delivery.attempt.request, result)
    }

    pub(super) fn latest_write(&self, context: &Context) -> Result<Option<RequestId>, Error> {
        if !matches!(context.owner(), Owner::Command { .. })
            && !self.physical_owner(context.owner())
        {
            return Err(Error::Invalid);
        }
        let owner = self
            .owner_entry(context.owner())
            .filter(|_| context.issued_by(&self.issuer))
            .ok_or(Error::Revoked)?;
        Ok(owner
            .writes
            .as_ref()
            .and_then(|writes| writes.latest(context.leg())))
    }

    /// One classified resend per logical leg: a refused resend is final.
    pub(super) fn grant_resend(&mut self, delivery: &Delivery) -> Result<(), Error> {
        if delivery.context.operation() != Operation::Mutation {
            return Err(Error::Invalid);
        }
        let writes = self
            .command_mut(&delivery.context)?
            .writes
            .as_mut()
            .ok_or(Error::State)?;
        if writes.leg_attempts(delivery.context.leg()) > 1 {
            return Err(Error::RetryForbidden);
        }
        writes.grant_resend(delivery.attempt.request)
    }

    pub(super) fn unproven_result(
        &mut self,
        delivery: &Delivery,
        status: u16,
        capture: impl FnOnce(&mut crate::request_context::UnprovenEvidence),
    ) -> Result<(), Error> {
        self.unproven_writes(delivery)?
            .unproven_result(delivery.attempt.request, status, capture)
    }

    fn unproven_writes(&mut self, delivery: &Delivery) -> Result<&mut CommandWrites, Error> {
        self.validate_handle(&delivery.context)?;
        if delivery.context.operation() != Operation::Mutation {
            return Err(Error::Invalid);
        }
        let writes = self
            .command_mut(&delivery.context)?
            .writes
            .as_mut()
            .ok_or(Error::State)?;
        if writes.latest(delivery.context.leg()) != Some(delivery.attempt.request) {
            return Err(Error::Revoked);
        }
        Ok(writes)
    }

    fn result_with_reports(
        &mut self,
        delivery: &Delivery,
        result: WriteResult,
        reports: RejectionReports,
    ) -> Result<(), Error> {
        if delivery.context.operation() != Operation::Mutation {
            return Err(Error::Invalid);
        }
        self.command_mut(&delivery.context)?
            .writes
            .as_mut()
            .ok_or(Error::State)?
            .result_with_reports(delivery.attempt.request, result, reports)
    }

    fn validate_completion(
        &self,
        command: CommandId,
        delivery: Option<&Delivery>,
        authority: Option<&Context>,
    ) -> Result<(), Error> {
        self.validate_completion_as(command, delivery, authority, false)
    }

    fn validate_completion_as(
        &self,
        command: CommandId,
        delivery: Option<&Delivery>,
        authority: Option<&Context>,
        observer: bool,
    ) -> Result<(), Error> {
        // An execution observer is sealed only by its execution, never directly.
        if observer != self.is_observer(command) || (observer && delivery.is_some()) {
            return Err(Error::Revoked);
        }
        if self.expired(command)
            || self
                .commands
                .get(&command)
                .is_some_and(|owner| !matches!(owner.phase, CommandPhase::Context { .. }))
        {
            return Err(Error::Revoked);
        }
        // Managed completion always has explicit generation-bearing authority.
        // Numeric calls remain legacy-only; a live journal/managed delivery
        // cannot be finalized by that bridge.
        if let Some(authority) = authority {
            if !authority.issued_by(&self.issuer)
                || !matches!(authority.owner(), Owner::Command { id, generation } if id == command
                    && self.commands.get(&id).is_some_and(|owner| owner.generation == generation))
            {
                return Err(Error::Revoked);
            }
            // An original logical timer may seal after its root was used as a
            // replaced physical parent. This grants no send permission and
            // never bypasses the delivery validation below.
            if self.superseded(authority)
                && self
                    .commands
                    .get(&command)
                    .is_none_or(|owner| owner.context_root() != Some(authority))
            {
                return Err(Error::Revoked);
            }
        } else if delivery.is_some()
            || self
                .commands
                .get(&command)
                .is_some_and(|owner| owner.retained() || owner.mutation_declared)
        {
            return Err(Error::Revoked);
        }
        if let Some(delivery) = delivery {
            let Owner::Command { id, generation } = delivery.context.owner() else {
                return Err(Error::Revoked);
            };
            if id != command
                || !delivery.context.issued_by(&self.issuer)
                || self
                    .commands
                    .get(&id)
                    .is_none_or(|owner| owner.generation != generation)
                || (delivery.context.operation() == Operation::Mutation
                    && self
                        .commands
                        .get(&id)
                        .and_then(|owner| owner.writes.as_ref())
                        .and_then(|writes| writes.latest(delivery.context.leg()))
                        != Some(delivery.attempt.request))
            {
                return Err(Error::Revoked);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn seal(
        &mut self,
        command: CommandId,
        delivery: Option<&Delivery>,
        authority: Option<&Context>,
        error: Option<VenueError>,
    ) -> Seal {
        self.seal_as(command, delivery, authority, error, false)
    }

    /// A Plan-backed execution's observer takes the Plan journal's projection
    /// verbatim; it has no journal of its own that could rewrite the outcome.
    pub(super) fn seal_projected(
        &mut self,
        command: CommandId,
        root: &Context,
        result: Result<String, VenueError>,
    ) -> (Option<String>, Seal) {
        if self
            .validate_completion_as(command, None, Some(root), true)
            .is_err()
            || self
                .commands
                .get(&command)
                .is_some_and(|owner| owner.writes.is_some())
        {
            return (None, Seal::Stale);
        }
        let declared = self
            .commands
            .get(&command)
            .is_some_and(|owner| owner.mutation_declared);
        match result {
            Ok(result) if declared => (Some(result), Seal::Success),
            Ok(result) => (Some(result), Seal::Legacy(None)),
            Err(error) if declared => (None, Seal::Failure(error, Vec::new())),
            Err(error) => (None, Seal::Legacy(Some(error))),
        }
    }

    /// `observer` seals an execution member through its own root, never
    /// through a physical delivery.
    pub(super) fn seal_as(
        &mut self,
        command: CommandId,
        delivery: Option<&Delivery>,
        authority: Option<&Context>,
        error: Option<VenueError>,
        observer: bool,
    ) -> Seal {
        if self
            .validate_completion_as(command, delivery, authority, observer)
            .is_err()
        {
            return Seal::Stale;
        }
        let (declared, inherited) = self.commands.get(&command).map_or((false, false), |owner| {
            (owner.mutation_declared, owner.inherited_uncertain)
        });
        let mut writes = match self
            .commands
            .get_mut(&command)
            .and_then(|owner| owner.writes.take())
        {
            Some(writes) => writes,
            None if declared => CommandWrites::new(CAPACITY)
                .expect("fixed positive journal capacity")
                .inheriting(inherited),
            None => return Seal::Legacy(error),
        };
        if error.is_none() && writes.succeed().is_ok() {
            return Seal::Success;
        }
        let error = error
            .unwrap_or_else(|| VenueError::new("Command writes are not all confirmed accepted"));
        match writes.fail(error) {
            Ok(error) => Seal::Failure(error, writes.reports().map(str::to_owned).collect()),
            Err(_) => Seal::Stale,
        }
    }

    fn seal_replacement_failure(
        &mut self,
        root: &Context,
        child: &Context,
        delivery: &Delivery,
        now: f64,
        error: VenueError,
    ) -> Result<Seal, Error> {
        if root.operation() != Operation::Mutation || child.operation() != Operation::Mutation {
            return Err(Error::Invalid);
        }
        self.validate(child, now)?;
        self.validate_handle(&delivery.context)?;
        let Owner::Command { id, generation } = root.owner() else {
            return Err(Error::Invalid);
        };
        if !root.issued_by(&self.issuer)
            || child.owner() != root.owner()
            || delivery.context.owner() != root.owner()
        {
            return Err(Error::Revoked);
        }
        let owner = self.commands.get_mut(&id).ok_or(Error::Revoked)?;
        if owner.generation != generation || owner.context_root() != Some(root) {
            return Err(Error::Revoked);
        }
        let writes = owner.writes.as_mut().ok_or(Error::State)?;
        if !writes.unreleased_replacement_child(
            delivery.attempt.request,
            delivery.context.leg(),
            child.leg(),
        ) {
            return Err(Error::Revoked);
        }
        // Every authority check precedes sealing/removal. This path can only
        // derive failure from the existing journal, never success or new I/O.
        let error = writes.fail(error)?;
        let reports = writes.reports().map(str::to_owned).collect();
        owner.writes = None;
        Ok(Seal::Failure(error, reports))
    }

    /// Retire a physical Plan owner after its seal: no command completes here.
    #[cfg(feature = "http-plans")]
    pub(super) fn finish_physical(&mut self, owner: Owner) -> bool {
        let Owner::Execution {
            generation: identity,
        } = owner
        else {
            return false;
        };
        let Some(entry) = self.physical.get(&identity) else {
            return false;
        };
        let CommandPhase::Planned(state) = &entry.phase else {
            return false;
        };
        if entry.pending != 0 || self.retirements.len() >= self.retirements.capacity() {
            return false;
        }
        let Some(index) = self.plan_index.iter().position(|plan| *plan == owner) else {
            return false;
        };
        self.plan_index.remove(index);
        self.reserved_contexts -= state.unissued();
        let token = HttpOwnerRetirement::new(Preflight::for_owner(&self.issuer, owner));
        self.physical
            .get_mut(&identity)
            .expect("checked above")
            .phase = CommandPhase::Retiring(token.clone());
        self.contexts.retain(|_, context| context.owner() != owner);
        self.retirements.push_back(token);
        true
    }

    /// Effect-free: a new command root would fit the bounded registry, and
    /// its storage is reserved, so issuance after the ID spend cannot fail.
    pub(super) fn reserve_command_root(&mut self, command: CommandId) -> Result<(), Error> {
        let new_owner = !self.commands.contains_key(&command);
        if self.contexts.len() + self.reserved_context_count() >= CAPACITY
            || (new_owner && self.commands.len() >= CAPACITY)
        {
            return Err(Error::Capacity);
        }
        self.contexts
            .try_reserve(1 + self.reserved_context_count())
            .map_err(|_| Error::Capacity)?;
        if new_owner {
            self.commands.try_reserve(1).map_err(|_| Error::Capacity)?;
        }
        Ok(())
    }

    /// Whether `root`'s owner is a tracked caller-wait Read root.
    pub(super) fn tracked_owner(&self, root: &Context) -> bool {
        matches!(root.owner(), Owner::Command { id, .. } if self.tracked(id))
    }

    fn tracked(&self, command: CommandId) -> bool {
        self.commands
            .get(&command)
            .is_some_and(|owner| matches!(owner.phase, CommandPhase::Context { tracked: true, .. }))
    }

    fn expired(&self, command: CommandId) -> bool {
        self.commands
            .get(&command)
            .is_some_and(|owner| matches!(owner.phase, CommandPhase::Context { expired: true, .. }))
    }

    /// Track this command's just-retained entry root for engine expiry.
    pub(super) fn track_read_root(&mut self, root: &Context) {
        let Owner::Command { id, generation } = root.owner() else {
            panic!("a tracked root is a command root");
        };
        let owner = self.commands.get_mut(&id).expect("issued tracked root");
        assert_eq!(owner.generation, generation);
        let CommandPhase::Context {
            root: issued,
            entry_owned: true,
            tracked,
            ..
        } = &mut owner.phase
        else {
            panic!("a tracked root is a retained Context root");
        };
        assert!(issued == root && root.operation() == Operation::Read);
        *tracked = true;
    }

    /// Mark this exact tracked root expired; `false` when it is already gone.
    pub(super) fn expire_read_root(&mut self, command: CommandId, generation: u64) -> bool {
        let Some(owner) = self.commands.get_mut(&command) else {
            return false;
        };
        match &mut owner.phase {
            CommandPhase::Context {
                tracked: true,
                expired,
                ..
            } if owner.generation == generation => {
                *expired = true;
                true
            }
            _ => false,
        }
    }

    /// The engine's own Timeout seal for an expired tracked Read root. No
    /// venue path reaches it: the expired flag refuses every other seal.
    pub(super) fn seal_expired_root(&mut self, command: CommandId, generation: u64) -> Seal {
        let sealable = self.commands.get(&command).is_some_and(|owner| {
            owner.generation == generation
                && owner.writes.is_none()
                && !owner.mutation_declared
                && matches!(
                    owner.phase,
                    CommandPhase::Context {
                        tracked: true,
                        expired: true,
                        ..
                    }
                )
        });
        if !sealable {
            return Seal::Stale;
        }
        Seal::Legacy(Some(Error::TimedOut.read_venue_error()))
    }

    pub(super) fn finish(&mut self, command: CommandId) -> bool {
        #[cfg(feature = "http-plans")]
        if let Some(owner) = self.commands.get_mut(&command) {
            match &owner.phase {
                #[cfg(feature = "http-plans")]
                CommandPhase::Preflight(_) | CommandPhase::Planned(_) => {
                    if owner.pending != 0 || self.retirements.len() >= self.retirements.capacity() {
                        return false;
                    }
                    if let CommandPhase::Planned(state) = &owner.phase {
                        let Some(index) = self.plan_index.iter().position(
                            |plan| matches!(plan, Owner::Command { id, .. } if *id == command),
                        ) else {
                            return false;
                        };
                        self.plan_index.remove(index);
                        self.reserved_contexts -= state.unissued();
                    }
                    let preflight = Preflight::new(&self.issuer, command, owner.generation);
                    let token = HttpOwnerRetirement::new(preflight);
                    owner.phase = CommandPhase::Retiring(token.clone());
                    self.contexts
                        .retain(|_, context| context.owner() != token.preflight().owner());
                    self.forget_read_mark(token.preflight().owner());
                    self.retirements.push_back(token);
                    return true;
                }
                #[cfg(feature = "http-plans")]
                CommandPhase::Retiring(_) => return false,
                CommandPhase::Context { .. } | CommandPhase::LocalSetting => {}
            }
        }
        self.forget_observer(command);
        if let Some(owner) = self.commands.remove(&command) {
            let retired = Owner::Command {
                id: command,
                generation: owner.generation,
            };
            self.contexts
                .retain(|_, context| context.owner() != retired);
            self.forget_read_mark(retired);
        }
        true
    }
}

impl<P> Cx<'_, P> {
    /// Atomically replace a definitively refused physical write with bounded
    /// engine-issued child legs. This queues no HTTP and extends no deadline.
    pub fn http_replacement_contexts(
        &mut self,
        parent: &Context,
        policies: &[Policy],
    ) -> Result<Vec<Context>, Error> {
        self.io.request_contexts.validate_legacy(parent)?;
        if policies.is_empty() {
            return Err(Error::Invalid);
        }
        if policies.len() > CAPACITY {
            return Err(Error::Capacity);
        }
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::State)?.clone();
        // Reserve the complete child range before registry/journal mutation.
        // Failed admission may leave monotonic gaps, never reused identities.
        let first = self
            .io
            .reserve_ids(policies.len() as u64)
            .ok_or(Error::Capacity)?
            .start;
        self.io
            .request_contexts
            .replace(parent, &delivery, first, policies, self.now)
    }

    /// Abort an unreleased replacement child during its exact superseded
    /// parent's callback. Normal old-parent completion stays revoked. This is
    /// failure-only and grants no result, send, retry or success authority.
    pub fn fail_replacement_context(
        &mut self,
        root: &Context,
        child: &Context,
        error: impl Into<VenueError>,
    ) -> Result<(), Error> {
        let Owner::Command { id, .. } = root.owner() else {
            return Err(Error::Invalid);
        };
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::State)?;
        let seal = self.io.request_contexts.seal_replacement_failure(
            root,
            child,
            delivery,
            self.now,
            error.into(),
        )?;
        self.finish_sealed_command(id, None, seal)
    }

    /// Capture the logical raw deadline and engine-owned owner/leg generation.
    /// This neither queues HTTP nor reserves admission, a transport or pacing.
    pub fn http_context(&mut self, scope: Scope, policy: Policy) -> Result<Context, Error> {
        self.make_http_context(scope, policy, None, None)
    }

    /// As `http_context`, with egress declared at issuance. Only unsigned
    /// reads declare egress; a credentialed context may only confirm Primary.
    pub fn http_context_with_egress(
        &mut self,
        scope: Scope,
        policy: Policy,
        egress: Egress,
    ) -> Result<Context, Error> {
        // Session egress is minted only from a live session.
        if matches!(egress, Egress::Session(_)) {
            return Err(Error::Invalid);
        }
        self.make_http_context(scope, policy, None, Some(egress))
    }

    /// A new physical leg under the same logical authority. Preserve its exact
    /// raw deadline; integer timeout recomputation cannot round the bound up.
    /// Session legs use `session_leg_context`, so families without Sessions
    /// never link Session issuance.
    pub fn http_leg_context(&mut self, parent: &Context, policy: Policy) -> Result<Context, Error> {
        self.io.request_contexts.validate_legacy(parent)?;
        self.io.request_contexts.validate_send(parent, self.now)?;
        let scope = match parent.owner() {
            Owner::Command { id, .. } => Scope::Command(id),
            Owner::Background { .. } => Scope::Background,
            Owner::Session { .. } => return Err(Error::Invalid),
            Owner::Execution { .. } => Scope::Execution,
        };
        self.make_http_context(scope, policy, Some(parent), None)
    }

    fn make_http_context(
        &mut self,
        scope: Scope,
        policy: Policy,
        parent: Option<&Context>,
        egress: Option<Egress>,
    ) -> Result<Context, Error> {
        #[cfg(feature = "http-plans")]
        if self.io.http_preflight_ready.is_some() {
            return Err(Error::Revoked);
        }
        if !self.io.entry_context_allowed(scope) {
            return Err(Error::Revoked);
        }
        // A tracked caller-wait root stays plain Read: refused before any spend.
        if policy.operation == Operation::Mutation
            && matches!(scope, Scope::Command(id) if self.io.request_contexts.tracked(id))
        {
            return Err(Error::Invalid);
        }
        let identity = self.io.reserve_ids(1).ok_or(Error::Capacity)?.start;
        let context = self
            .io
            .request_contexts
            .issue_with_egress(identity, scope, policy, self.now, parent, egress)?;
        #[cfg(feature = "order-locks")]
        if context.operation() == Operation::Mutation
            && let Owner::Command { id, .. } = context.owner()
        {
            self.io.order_locks.manage(id);
        }
        Ok(context)
    }

    /// Validate context provenance and retry policy without extending its bound.
    /// This is not permission to resend a write: physical attempt registration
    /// must separately verify the prior attempt's definitive native facts.
    pub fn http_retry_context(
        &self,
        context: &Context,
        evidence: RetryEvidence,
    ) -> Result<Context, Error> {
        self.io.request_contexts.validate_legacy(context)?;
        if evidence == RetryEvidence::DefinitiveRefusal {
            return Err(Error::RetryForbidden);
        }
        self.io.request_contexts.validate_send(context, self.now)?;
        context.retry(evidence, self.now)
    }

    /// Check live issuance before presenting progress for already reserved work.
    /// This allocates nothing and grants no send, retry or completion rights.
    pub fn validate_http_context(&self, context: &Context) -> Result<(), Error> {
        self.io.request_contexts.validate_legacy(context)?;
        self.io.request_contexts.validate(context, self.now)
    }

    /// Read-only companion-event authority. A Read handle cannot publish a
    /// completion marker once its command has declared any mutation sibling.
    /// Complete synchronously after validation; this grants no send permission.
    pub fn validate_read_completion_context(&self, context: &Context) -> Result<(), Error> {
        self.validate_completion_context(context)?;
        let Owner::Command { id, .. } = context.owner() else {
            return Err(Error::Invalid);
        };
        let owner = self
            .io
            .request_contexts
            .commands
            .get(&id)
            .ok_or(Error::Revoked)?;
        if context.operation() != Operation::Read
            || owner.mutation_declared
            || owner.writes.is_some()
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }

    /// Bind a read-only completion owner to its exact original root, including
    /// constructors and no-I/O terminal paths. This grants no send permission.
    /// Expiry follows the existing completion rules, never renews a request.
    pub fn validate_read_root_context(&self, root: &Context) -> Result<(), Error> {
        self.validate_read_completion_context(root)?;
        let Owner::Command { id, .. } = root.owner() else {
            return Err(Error::Invalid);
        };
        if self
            .io
            .request_contexts
            .commands
            .get(&id)
            .is_none_or(|owner| owner.context_root() != Some(root))
        {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    /// Validate a Background Read in this exact managed HTTP callback. Publish
    /// synchronously after validation, before retiring the Context. Native
    /// response validity and publication freshness remain separate requirements.
    /// Failure callbacks also carry delivery identity, but prove no response data.
    /// This grants no send, retry, session or command completion authority.
    pub fn validate_background_read_http_delivery(&self, context: &Context) -> Result<(), Error> {
        self.validate_http_context(context)?;
        if context.operation() != Operation::Read
            || !matches!(context.owner(), Owner::Background { .. })
        {
            return Err(Error::Invalid);
        }
        if self
            .io
            .http_delivery
            .as_ref()
            .is_none_or(|delivery| delivery.context != *context)
        {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    /// Validate read publication in this exact managed HTTP callback. Native
    /// response decoding and snapshot freshness remain separate requirements.
    /// Failure callbacks also have delivery ownership; this does not prove a
    /// response exists or authorize publishing an unvalidated response body.
    /// Publish synchronously after validation; this grants no send or retry rights.
    pub fn validate_read_http_delivery(&self, root: &Context, leg: &Context) -> Result<(), Error> {
        self.validate_read_completion_context(root)?;
        self.validate_http_context(leg)?;
        if leg.operation() != Operation::Read {
            return Err(Error::Invalid);
        }
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::Revoked)?;
        let Owner::Command { id, .. } = root.owner() else {
            return Err(Error::Invalid);
        };
        if delivery.context != *leg
            || leg.owner() != root.owner()
            || leg.deadline() > root.deadline()
            || self
                .io
                .request_contexts
                .commands
                .get(&id)
                .is_none_or(|owner| owner.context_root() != Some(root))
        {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    /// Read-only identity of this exact validated physical delivery. History
    /// staging binds it alongside the logical leg; it grants no new rights.
    pub(crate) fn read_http_delivery_attempt(
        &self,
        root: &Context,
        leg: &Context,
    ) -> Result<Attempt, Error> {
        self.validate_read_http_delivery(root, leg)?;
        self.io
            .http_delivery
            .as_ref()
            .map(|delivery| delivery.attempt)
            .ok_or(Error::Revoked)
    }

    /// Validate a synchronous managed completion before publishing its companion
    /// event. This is completion authority only: no send permission or deadline
    /// extension. The caller must finish in the same callback without yielding.
    pub(crate) fn validate_completion_context(&self, context: &Context) -> Result<(), Error> {
        self.validate_preflight_ready_owner(context.owner())?;
        let Owner::Command { id, .. } = context.owner() else {
            return Err(Error::Invalid);
        };
        self.io.request_contexts.validate_completion(
            id,
            super::completion_delivery(&self.io.http_delivery),
            Some(context),
        )
    }

    /// Generation-bearing finalization for callbacks and logical command timers.
    /// Physical-attempt timers must retain/check their attempt token separately.
    pub fn done_context(&mut self, context: &Context, result: String) -> Result<(), Error> {
        let Owner::Command { id, .. } = context.owner() else {
            return Err(Error::Invalid);
        };
        self.validate_completion_context(context)?;
        self.finish_command(id, Ok(result), Some(context))
    }

    /// Publish a correlated diagnostic only for live completion authority.
    /// The caller completes synchronously; this neither issues nor renews HTTP.
    pub fn command_error_context(
        &mut self,
        context: &Context,
        message: impl Into<String>,
    ) -> Result<(), Error> {
        self.validate_completion_context(context)?;
        let Owner::Command { id, .. } = context.owner() else {
            return Err(Error::Invalid);
        };
        self.command_error(id, message);
        Ok(())
    }

    pub fn fail_context(
        &mut self,
        context: &Context,
        error: impl Into<VenueError>,
    ) -> Result<(), Error> {
        let Owner::Command { id, .. } = context.owner() else {
            return Err(Error::Invalid);
        };
        self.finish_command(id, Err(error.into()), Some(context))
    }

    /// A physical timer carries its original attempt token. A logical Context
    /// alone cannot distinguish an old physical timeout from its newer retry.
    pub fn fail_http_attempt(
        &mut self,
        context: &Context,
        attempt: Attempt,
        error: impl Into<VenueError>,
    ) -> Result<(), Error> {
        let delivery = self
            .io
            .managed_http
            .get(&attempt.request)
            .ok_or(Error::Revoked)?
            .delivery();
        if delivery.attempt != attempt
            || delivery.context != *context
            || !context.issued_by(&self.io.request_contexts.issuer)
        {
            return Err(Error::Revoked);
        }
        let previous = self.io.http_delivery.replace(delivery);
        let result = self.fail_context(context, error);
        self.io.http_delivery = previous;
        result
    }

    /// Record only native codec facts for the exact currently delivered attempt.
    /// Transport and signing failures are recorded by the engine, never inferred
    /// from venue exception text or a caller-provided numeric request id.
    #[cfg(test)]
    pub(crate) fn http_write_result(
        &mut self,
        context: &Context,
        result: WriteResult,
    ) -> Result<(), Error> {
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::State)?;
        if delivery.context != *context || !context.issued_by(&self.io.request_contexts.issuer) {
            return Err(Error::Revoked);
        }
        self.io.request_contexts.result(delivery, result)
    }

    /// Retain bounded native diagnostics/IDs when a delivered write ACK cannot
    /// prove its effects. Fixed Internal/Unknown; no Store, retry or auth rights.
    /// Capture runs only for the exact live, released, unresolved attempt.
    pub fn http_write_unproven_result(
        &mut self,
        context: &Context,
        capture: impl FnOnce(&mut crate::request_context::UnprovenEvidence),
    ) -> Result<(), Error> {
        self.validate_write_delivery_phase(context)?;
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::State)?;
        if delivery.context != *context {
            return Err(Error::Revoked);
        }
        self.io
            .request_contexts
            .unproven_writes(delivery)?
            .unproven_native_result(delivery.attempt.request, capture)
    }

    /// Check normalized native facts before synchronous codec-selected effects.
    /// This repeatable read-only check grants no send, retry or completion rights;
    /// native decoding, row attribution and snapshot freshness remain separate.
    pub fn validate_http_write_effects(&self, context: &Context) -> Result<(), Error> {
        let (writes, request) = self.validate_http_write_record(context)?;
        writes.validate_effects(request)
    }

    /// Exact response authority for synchronous nonfinancial terminal-row
    /// housekeeping only. The codec must separately prove the captured rows were
    /// already terminal and remain terminal on a validated native success reply.
    /// Failed evidence never authorizes financial effects or native tombstones.
    pub fn validate_http_write_housekeeping(&self, context: &Context) -> Result<(), Error> {
        let (writes, request) = self.validate_http_write_record(context)?;
        writes.validate_housekeeping(request)
    }

    fn validate_write_delivery_phase(&self, context: &Context) -> Result<(), Error> {
        #[cfg(feature = "http-plans")]
        if let Some(plan) = self.io.request_contexts.plan_from_context(context) {
            return self.validate_plan_delivery(&plan, context);
        }
        self.validate_http_context(context)
    }

    fn validate_http_write_record(
        &self,
        context: &Context,
    ) -> Result<(&CommandWrites, RequestId), Error> {
        self.validate_write_delivery_phase(context)?;
        if context.operation() != Operation::Mutation {
            return Err(Error::Invalid);
        }
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::State)?;
        if delivery.context != *context {
            return Err(Error::Revoked);
        }
        if !matches!(context.owner(), Owner::Command { .. })
            && !self.io.request_contexts.physical_owner(context.owner())
        {
            return Err(Error::Invalid);
        }
        let writes = self
            .io
            .request_contexts
            .owner_entry(context.owner())
            .and_then(|owner| owner.writes.as_ref())
            .ok_or(Error::State)?;
        Ok((writes, delivery.attempt.request))
    }

    /// Native reports are published centrally with the terminal logical legs.
    /// Capacity refusal leaves facts untouched: retry with empty reports to keep
    /// native facts/partial IDs. Never bypass a provenance or state refusal.
    #[cfg(test)]
    pub(crate) fn http_write_result_with_reports(
        &mut self,
        context: &Context,
        result: WriteResult,
        reports: RejectionReports,
    ) -> Result<(), Error> {
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::State)?;
        if delivery.context != *context || !context.issued_by(&self.io.request_contexts.issuer) {
            return Err(Error::Revoked);
        }
        self.io
            .request_contexts
            .result_with_reports(delivery, result, reports)
    }

    /// Classify the delivered write reply. The engine supplies the status,
    /// headers and the command's intent; the codec supplies native evidence.
    pub fn classify_write(
        &self,
        context: &Context,
        table: &crate::classify::CodeTable,
        native: Option<&crate::classify::Native<'_>>,
        message: impl Into<String>,
    ) -> Result<crate::classify::Fact, Error> {
        let (status, headers) = self.io.http_write_reply.as_ref().ok_or(Error::State)?;
        let response = crate::Response {
            status: *status,
            headers,
            body: "",
        };
        let delivery = self.current_write_delivery(context)?;
        Ok(crate::classify::fact(
            table,
            self.write_intent(context)?,
            crate::classify::Released::Response(&response),
            native,
            message,
        )
        .for_attempt(delivery.attempt.request))
    }

    /// Record the delivered write attempt's native answer. Refusals exist only
    /// as classifier facts; a whole `RefusedResend` refusal returns the only
    /// grant a resend accepts (one per logical leg).
    pub fn http_write_fact(
        &mut self,
        context: &Context,
        fact: crate::classify::WriteFact,
        reports: Option<RejectionReports>,
    ) -> Result<Option<ResendGrant>, Error> {
        let delivery = self.current_write_delivery(context)?;
        if !fact.belongs_to(delivery.attempt.request) {
            return Err(Error::Revoked);
        }
        let resendable = fact.resendable();
        let absent = matches!(&fact, crate::classify::WriteFact::Failed { fact, .. }
            if fact.verdict() == crate::classify::Verdict::Satisfied(crate::classify::SatisfiedIntent::Cancel));
        let result = match fact.into_result() {
            // A placement acknowledgement must name what it placed.
            WriteResult::Accepted { order_ids }
                if order_ids.is_empty()
                    && self.write_intent(context)? == crate::classify::Intent::Place =>
            {
                WriteResult::Failed(
                    VenueError::new("Placement acknowledgement has no native order ID")
                        .kind(crate::ErrorKind::Internal)
                        .outcome(crate::Outcome::Unknown),
                )
            }
            result => result,
        };
        // Accepted facts carry no rejection reports.
        let reports = reports.filter(|_| !matches!(result, WriteResult::Accepted { .. }));
        self.record_write(&delivery, result, reports)?;
        if absent {
            self.fence_absent_orders(context);
        }
        // Session writes get one attempt per operation: no resend grant.
        if !resendable || matches!(context.owner(), Owner::Session { .. }) {
            return Ok(None);
        }
        match self.io.request_contexts.grant_resend(&delivery) {
            Ok(()) => Ok(Some(ResendGrant {
                attempt: delivery.attempt.request,
            })),
            Err(Error::RetryForbidden) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Classify and record one whole native failure for the delivered attempt.
    /// `reports: None` keeps the default one-message rejection reporting.
    pub fn http_write_failure(
        &mut self,
        context: &Context,
        table: &crate::classify::CodeTable,
        native: Option<&crate::classify::Native<'_>>,
        message: impl Into<String>,
        reports: Option<RejectionReports>,
    ) -> Result<(crate::classify::Verdict, Option<ResendGrant>), Error> {
        let fact = self.classify_write(context, table, native, message)?;
        let verdict = fact.verdict();
        let failed = crate::classify::WriteFact::Failed {
            fact,
            known_effects: false,
        };
        Ok((verdict, self.http_write_fact(context, failed, reports)?))
    }

    /// A 2xx whose success envelope or acknowledgement does not prove the write.
    pub fn http_write_unproven(
        &mut self,
        context: &Context,
        message: impl Into<String>,
    ) -> Result<(), Error> {
        let failed = crate::classify::WriteFact::Failed {
            fact: crate::classify::unproven_reply(message),
            known_effects: false,
        };
        self.http_write_fact(context, failed, None).map(|_| ())
    }

    /// One batch acknowledgement, items in caller order. The engine checks the
    /// item count, joins accepted IDs and chooses Accepted, Rejected, KnownPartial,
    /// PartialUnknown or Failed. `message` is the venue's existing text for a failed batch.
    pub fn http_write_batch(
        &mut self,
        context: &Context,
        table: &crate::classify::CodeTable,
        expected: usize,
        items: &[crate::classify::BatchItem<'_>],
        message: impl Into<String>,
        reports: Option<RejectionReports>,
    ) -> Result<WriteResult, Error> {
        let delivery = self.current_write_delivery(context)?;
        let (status, headers) = self.io.http_write_reply.as_ref().ok_or(Error::State)?;
        let response = crate::Response {
            status: *status,
            headers,
            body: "",
        };
        let intent = self.write_intent(context)?;
        let result = crate::classify::batch(table, intent, &response, expected, items, message)?;
        let absent = !matches!(result, WriteResult::Failed(_))
            && items.iter().any(|item| {
                matches!(item, crate::classify::BatchItem::Native(native)
                    if table.verdict(native, intent)
                        == crate::classify::Verdict::Satisfied(crate::classify::SatisfiedIntent::Cancel))
            });
        let reports = reports.filter(|_| !matches!(result, WriteResult::Accepted { .. }));
        self.record_write(&delivery, result.clone(), reports)?;
        if absent {
            self.fence_absent_orders(context);
        }
        Ok(result)
    }

    /// A Satisfied(Cancel) answer is success without a store transition: fence
    /// the command's symbols and ask the venue for exactly one fresh order read.
    fn fence_absent_orders(&mut self, context: &Context) {
        #[cfg(feature = "order-locks")]
        {
            let command = match context.owner() {
                Owner::Command { id, .. } => Some(id),
                _ => None,
            };
            let scope = command.and_then(|id| self.io.order_locks.reconcile_scope(id).flatten());
            self.io.order_locks.fence(scope.as_deref());
            // A held placing scope would make a read now unable to clear the
            // fence; finish queues it instead.
            if !command.is_some_and(|id| self.io.order_locks.defer_absent(id)) {
                self.io.queue_reconcile(scope, self.now);
            }
        }
        #[cfg(not(feature = "order-locks"))]
        let _ = context;
    }

    /// The intent the engine recorded for this leg's logical command.
    /// Background (session) writes classify as placements; a command owner
    /// created outside dispatch has no trusted intent.
    fn write_intent(&self, context: &Context) -> Result<crate::classify::Intent, Error> {
        // Session writes (listen keys, logins) change account state, not orders.
        if matches!(context.owner(), Owner::Session { .. }) {
            return Ok(crate::classify::Intent::Setting);
        }
        if !matches!(context.owner(), Owner::Command { .. })
            && !self.io.request_contexts.physical_owner(context.owner())
        {
            return Ok(crate::classify::Intent::Place);
        }
        let owner = self
            .io
            .request_contexts
            .owner_entry(context.owner())
            .ok_or(Error::Revoked)?;
        let leg = owner
            .leg_intents
            .iter()
            .find(|(leg, _)| *leg == context.leg())
            .map(|(_, intent)| *intent);
        leg.or(owner.intent).ok_or(Error::State)
    }

    /// Narrow one leg's intent, before its first attempt, where the command
    /// implies it: an amend's replacement is a Place and its attached TP/SL
    /// update a Setting; a placement may first cancel conflicting orders or set
    /// a protection setting. An amend's own cancel leg never becomes a Cancel
    /// here (see `http_write_protection_cancel_leg`). The command's own intent is
    /// a no-op.
    pub fn http_write_leg_intent(
        &mut self,
        context: &Context,
        intent: crate::classify::Intent,
    ) -> Result<(), Error> {
        self.narrow_leg(context, intent, false)
    }

    /// The one declared exception for an amend's cancel leg: the old order of a
    /// cancel-then-recreate whose replacement is position-bound protection
    /// (reduce-only / close-on-trigger), so an absent old order is safe to
    /// replace. Never declare it for any other replacement.
    pub fn http_write_protection_cancel_leg(&mut self, context: &Context) -> Result<(), Error> {
        self.narrow_leg(context, crate::classify::Intent::Cancel, true)
    }

    fn narrow_leg(
        &mut self,
        context: &Context,
        intent: crate::classify::Intent,
        protection_cancel: bool,
    ) -> Result<(), Error> {
        self.validate_preflight_ready_owner(context.owner())?;
        self.io.request_contexts.validate(context, self.now)?;
        let Owner::Command { id, generation } = context.owner() else {
            return Err(Error::Invalid);
        };
        let owner = self
            .io
            .request_contexts
            .commands
            .get_mut(&id)
            .filter(|owner| owner.generation == generation)
            .ok_or(Error::Revoked)?;
        if owner.prepare_leg_intent(context, intent, protection_cancel)? {
            owner.leg_intents.push((context.leg(), intent));
        }
        Ok(())
    }

    /// A resend of a write refused with a `RefusedResend` verdict. The journal
    /// still refuses a second resend of the same logical leg.
    pub fn http_resend_context(
        &self,
        context: &Context,
        grant: ResendGrant,
    ) -> Result<Context, Error> {
        self.io.request_contexts.validate_send(context, self.now)?;
        if self.io.request_contexts.latest_write(context)? != Some(grant.attempt) {
            return Err(Error::Revoked);
        }
        context.retry(RetryEvidence::DefinitiveRefusal, self.now)
    }

    fn current_write_delivery(&self, context: &Context) -> Result<Delivery, Error> {
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::State)?;
        if delivery.context != *context || !context.issued_by(&self.io.request_contexts.issuer) {
            return Err(Error::Revoked);
        }
        Ok(delivery.clone())
    }

    fn record_write(
        &mut self,
        delivery: &Delivery,
        result: WriteResult,
        reports: Option<RejectionReports>,
    ) -> Result<(), Error> {
        match reports {
            Some(reports) => self
                .io
                .request_contexts
                .result_with_reports(delivery, result, reports),
            None => self.io.request_contexts.result(delivery, result),
        }
    }

    /// Retire a fully completed logical read/page join or write leg. Do not call
    /// for each physical page: all remaining attempts must retain their owner.
    /// Command generation stays live until done/fail or its final bound expires.
    /// Expired current handles may retire; this grants no send/retry permission.
    pub fn finish_http_context(&mut self, context: &Context) -> Result<(), Error> {
        self.validate_preflight_ready_owner(context.owner())?;
        self.io.request_contexts.validate_legacy(context)?;
        self.io.request_contexts.validate_handle(context)?;
        self.io.retire_http_leg(context);
        self.io.request_contexts.contexts.remove(&context.leg());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_context::{Operation, RetryPolicy};
    use crate::{Notice, Store};

    #[test]
    fn native_unproven_public_capture_refuses_invalid_authority_before_callback() {
        for case in 0..11 {
            let mut io = super::super::Io::<()>::default();
            let mut store = Store::default();
            let mut cx = Cx {
                now: 0.,
                store: &mut store,
                io: &mut io,
            };
            let mut mutation = policy();
            mutation.operation = Operation::Mutation;
            mutation.retry = RetryPolicy::DefinitiveRefusal;
            let context = cx
                .http_context(Scope::Command(7), mutation.clone())
                .unwrap();
            let request = RequestId(99);
            cx.io
                .request_contexts
                .register_attempt(&context, request, 0.)
                .unwrap();
            if case != 1 {
                *cx.io
                    .request_contexts
                    .release_flag(&context, request)
                    .unwrap()
                    .unwrap() = true;
            }
            cx.io.http_delivery = Some(Delivery {
                context: context.clone(),
                attempt: Attempt {
                    request,
                    generation: 100,
                },
            });
            let mut argument = context.clone();
            match case {
                0 | 1 => {}
                2 => {
                    cx.io.http_delivery = None;
                }
                3 => {
                    argument = cx.http_leg_context(&context, mutation.clone()).unwrap();
                }
                4 => {
                    argument = ContextRegistry::default()
                        .issue(1, Scope::Command(7), mutation.clone(), 0.)
                        .unwrap();
                }
                5 => {
                    cx.now = context.deadline();
                }
                6 => {
                    cx.now = f64::NAN;
                }
                7 => {
                    cx.io.request_contexts.contexts.remove(&context.leg());
                }
                8 => {
                    cx.io
                        .request_contexts
                        .commands
                        .get_mut(&7)
                        .unwrap()
                        .generation += 1;
                }
                9 => {
                    cx.http_write_result(&context, WriteResult::Accepted { order_ids: vec![] })
                        .unwrap();
                }
                10 => {
                    cx.http_write_result(
                        &context,
                        WriteResult::Rejected(VenueError::new("refused")),
                    )
                    .unwrap();
                    cx.http_replacement_contexts(&context, &[mutation]).unwrap();
                }
                _ => unreachable!(),
            }
            let mut captures = 0;
            let result = cx.http_write_unproven_result(&argument, |e| {
                captures += 1;
                assert!(e.order_id("native"));
            });
            assert_eq!(result.is_ok(), case == 0, "case {case}");
            assert_eq!(captures, usize::from(case == 0), "case {case}");
            assert!(cx.io.pending.is_empty());
            assert!(cx.io.actions.is_empty());
            assert!(cx.io.timers.is_empty());
        }
    }

    #[test]
    fn effect_authority_is_bound_to_live_current_delivery_and_has_no_side_effects() {
        for case in 0..10 {
            let mut io = super::super::Io::<()>::default();
            let mut store = Store::default();
            let mut cx = Cx {
                now: 0.,
                store: &mut store,
                io: &mut io,
            };
            let mut mutation = policy();
            mutation.operation = Operation::Mutation;
            mutation.retry = RetryPolicy::DefinitiveRefusal;
            let context = cx
                .http_context(Scope::Command(7), mutation.clone())
                .unwrap();
            let request = RequestId(99);
            cx.io
                .request_contexts
                .register_attempt(&context, request, 0.)
                .unwrap();
            *cx.io
                .request_contexts
                .release_flag(&context, request)
                .unwrap()
                .unwrap() = true;
            cx.io.http_delivery = Some(Delivery {
                context: context.clone(),
                attempt: Attempt {
                    request,
                    generation: 100,
                },
            });
            cx.http_write_result(&context, WriteResult::Accepted { order_ids: vec![] })
                .unwrap();
            let mut argument = context.clone();
            match case {
                0 => {}
                1 => {
                    cx.io.http_delivery = None;
                }
                2 => {
                    argument = cx.http_leg_context(&context, mutation.clone()).unwrap();
                }
                3 => {
                    argument = ContextRegistry::default()
                        .issue(1, Scope::Command(7), mutation, 0.)
                        .unwrap();
                }
                4 => {
                    cx.now = context.deadline();
                }
                5 => {
                    cx.now = f64::NAN;
                }
                6 => {
                    cx.io.request_contexts.contexts.remove(&context.leg());
                }
                7 => {
                    cx.io
                        .request_contexts
                        .commands
                        .get_mut(&7)
                        .unwrap()
                        .generation += 1;
                }
                8 => {
                    cx.io.http_delivery.as_mut().unwrap().attempt.request = RequestId(100);
                }
                9 => {
                    cx.done_context(&context, "done".into()).unwrap();
                }
                _ => unreachable!(),
            }
            let counts = (
                cx.io.actions.len(),
                cx.io.pending.len(),
                cx.io.timers.len(),
                cx.io.request_contexts.contexts.len(),
                cx.io.request_contexts.commands.len(),
            );
            for _ in 0..2 {
                assert_eq!(
                    cx.validate_http_write_effects(&argument).is_ok(),
                    case == 0,
                    "case {case}"
                );
                assert_eq!(
                    cx.validate_http_write_housekeeping(&argument).is_ok(),
                    case == 0,
                    "housekeeping case {case}"
                );
                assert_eq!(
                    (
                        cx.io.actions.len(),
                        cx.io.pending.len(),
                        cx.io.timers.len(),
                        cx.io.request_contexts.contexts.len(),
                        cx.io.request_contexts.commands.len()
                    ),
                    counts
                );
                assert!(!cx.store.hedged());
            }
        }
    }

    #[test]
    fn child_contexts_keep_exact_parent_deadline_owner_and_no_transport_resources() {
        for scope in [Scope::Command(7), Scope::Background] {
            let mut io = super::super::Io::<()>::default();
            let mut store = Store::default();
            let mut cx = Cx {
                now: 100.125,
                store: &mut store,
                io: &mut io,
            };
            let parent = cx.http_context(scope, policy()).unwrap();
            cx.now = 599.9999;
            let child = cx.http_leg_context(&parent, policy()).unwrap();
            assert_eq!(child.owner(), parent.owner());
            assert_ne!(child.leg(), parent.leg());
            assert_eq!(child.deadline(), 600.125);
            let grandchild = cx.http_leg_context(&child, policy()).unwrap();
            assert_eq!(grandchild.deadline(), parent.deadline());
            assert_eq!(grandchild.owner(), parent.owner());
            assert!(cx.io.pending.is_empty());
            assert!(cx.io.timers.is_empty());
            assert!(cx.io.actions.is_empty());
            let count = cx.io.request_contexts.contexts.len();
            cx.now = parent.deadline();
            assert_eq!(cx.http_leg_context(&parent, policy()), Err(Error::TimedOut));
            assert_eq!(cx.io.request_contexts.contexts.len(), count);
        }
    }

    #[test]
    fn a_child_can_shorten_but_foreign_retired_or_reused_authority_cannot_issue_it() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 100.125,
            store: &mut store,
            io: &mut io,
        };
        let parent = cx.http_context(Scope::Command(7), policy()).unwrap();
        let mut shorter = policy();
        shorter.timeout_ms = 1;
        cx.now = 100.25;
        let child = cx.http_leg_context(&parent, shorter).unwrap();
        assert_eq!(child.deadline(), 101.25);
        let mut other = ContextRegistry::default();
        let foreign = other
            .issue(1, Scope::Command(7), policy(), 100.125)
            .unwrap();
        assert_eq!(cx.http_leg_context(&foreign, policy()), Err(Error::Revoked));
        cx.finish_http_context(&parent).unwrap();
        assert_eq!(cx.http_leg_context(&parent, policy()), Err(Error::Revoked));
        assert_eq!(cx.io.request_contexts.validate(&child, cx.now), Ok(()));
        cx.done_context(&child, "[]".into()).unwrap();
        let replacement = cx.http_context(Scope::Command(7), policy()).unwrap();
        assert_ne!(replacement.owner(), child.owner());
        assert_eq!(cx.http_leg_context(&child, policy()), Err(Error::Revoked));
        assert!(cx.io.pending.is_empty());
        assert!(cx.io.timers.is_empty());
    }

    #[test]
    fn read_authority_cannot_issue_a_mutation_or_create_background_write_authority() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        let parent = cx.http_context(Scope::Command(7), policy()).unwrap();
        let mut mutation = policy();
        mutation.operation = Operation::Mutation;
        mutation.retry = RetryPolicy::Never;
        assert_eq!(
            cx.http_leg_context(&parent, mutation.clone()),
            Err(Error::Invalid)
        );
        assert_eq!(
            cx.http_context(Scope::Background, mutation),
            Err(Error::Invalid)
        );
        assert_eq!(cx.io.request_contexts.contexts.len(), 1);
        assert_eq!(cx.io.request_contexts.commands.len(), 1);
        assert!(cx.io.pending.is_empty());
        assert!(cx.io.actions.is_empty());
        assert!(cx.io.timers.is_empty());
    }

    #[test]
    fn context_diagnostics_require_exact_completion_authority() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        let original = cx.http_context(Scope::Command(7), policy()).unwrap();
        let background = cx.http_context(Scope::Background, policy()).unwrap();
        let foreign = ContextRegistry::default()
            .issue(1, Scope::Command(7), policy(), 0.)
            .unwrap();
        assert_eq!(
            cx.command_error_context(&background, "background"),
            Err(Error::Invalid)
        );
        assert_eq!(
            cx.command_error_context(&foreign, "foreign"),
            Err(Error::Revoked)
        );
        assert!(cx.io.actions.is_empty());
        cx.done_context(&original, "[]".into()).unwrap();
        cx.io.actions.clear();
        let current = cx.http_context(Scope::Command(7), policy()).unwrap();
        assert_eq!(
            cx.command_error_context(&original, "old"),
            Err(Error::Revoked)
        );
        assert!(cx.io.actions.is_empty());
        cx.command_error_context(&current, "current").unwrap();
        assert!(
            matches!(cx.io.actions.as_slice(), [crate::Action::Notice(crate::Notice::CommandError {command:7,message})] if message == "current")
        );
    }

    #[test]
    fn completion_validation_is_read_only_and_does_not_grant_post_deadline_send_permission() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        let context = cx.http_context(Scope::Command(7), policy()).unwrap();
        let background = cx.http_context(Scope::Background, policy()).unwrap();
        let foreign = ContextRegistry::default()
            .issue(1, Scope::Command(7), policy(), 0.)
            .unwrap();
        assert_eq!(
            cx.validate_completion_context(&background),
            Err(Error::Invalid)
        );
        assert_eq!(
            cx.validate_completion_context(&foreign),
            Err(Error::Revoked)
        );
        cx.now = context.deadline();
        assert_eq!(cx.validate_completion_context(&context), Ok(()));
        assert_eq!(cx.validate_completion_context(&context), Ok(()));
        assert_eq!(
            cx.http_retry_context(&context, RetryEvidence::ReadFailure),
            Err(Error::TimedOut)
        );
        assert!(cx.io.actions.is_empty());
        assert!(cx.io.timers.is_empty());
        cx.done_context(&context, "[]".into()).unwrap();
        assert_eq!(
            cx.validate_completion_context(&context),
            Err(Error::Revoked)
        );
        let replacement = cx.http_context(Scope::Command(7), policy()).unwrap();
        assert_eq!(
            cx.validate_completion_context(&context),
            Err(Error::Revoked)
        );
        assert_eq!(cx.validate_completion_context(&replacement), Ok(()));
        assert_eq!(cx.io.actions.len(), 1);
    }

    #[test]
    fn live_validation_cannot_borrow_completion_expiry_tolerance() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        let root = cx.http_context(Scope::Command(7), policy()).unwrap();
        assert_eq!(cx.validate_completion_context(&root), Ok(()));
        cx.now = root.deadline();
        assert_eq!(cx.validate_http_context(&root), Err(Error::TimedOut));
        assert!(cx.io.actions.is_empty());
        assert!(cx.io.pending.is_empty());
        assert!(cx.io.timers.is_empty());
    }

    #[test]
    fn read_companion_cannot_publish_after_a_sibling_declares_mutation() {
        for install_attempt in [false, true] {
            let mut io = super::super::Io::<()>::default();
            let mut store = Store::default();
            let mut cx = Cx {
                now: 0.,
                store: &mut store,
                io: &mut io,
            };
            let root = cx.http_context(Scope::Command(7), policy()).unwrap();
            assert_eq!(cx.validate_read_completion_context(&root), Ok(()));
            let mutation = cx
                .http_context(
                    Scope::Command(7),
                    Policy {
                        operation: Operation::Mutation,
                        retry: RetryPolicy::Never,
                        ..policy()
                    },
                )
                .unwrap();
            if install_attempt {
                cx.io
                    .request_contexts
                    .register_attempt(&mutation, RequestId(99), cx.now)
                    .unwrap();
            }
            assert_eq!(
                cx.validate_read_completion_context(&root),
                Err(Error::Invalid)
            );
            assert!(cx.io.actions.is_empty());
            assert!(cx.io.pending.is_empty());
            assert!(cx.io.timers.is_empty());
            assert!(cx.io.request_contexts.commands.contains_key(&7));
        }
    }

    /// A re-mark keeps the owner's earliest epoch; other owners are separate.
    #[cfg(feature = "order-locks")]
    #[test]
    fn a_read_mark_keeps_its_owners_earliest_epoch() {
        let mut registry = ContextRegistry::default();
        let first = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
        let other = registry.issue(2, Scope::Background, policy(), 0.).unwrap();
        assert_eq!(registry.read_mark(first.owner(), 5), 5);
        assert_eq!(registry.read_mark(first.owner(), 9), 5);
        assert_eq!(registry.read_mark(other.owner(), 9), 9);
        assert_eq!(registry.read_mark_count(), 2);
    }

    /// A sweep keeps live owners in any phase and drops dead or regenerated
    /// ones; forgetting one owner leaves the others.
    #[cfg(feature = "order-locks")]
    #[test]
    fn read_marks_live_exactly_as_long_as_their_owners() {
        let mut registry = ContextRegistry::default();
        let command = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
        let background = registry.issue(2, Scope::Background, policy(), 0.).unwrap();
        let Owner::Command { id, generation } = command.owner() else {
            panic!("a command scope has a command owner");
        };
        registry.read_mark(command.owner(), 1);
        registry.read_mark(background.owner(), 1);
        registry.read_mark(
            Owner::Command {
                id,
                generation: generation + 1,
            },
            1,
        );
        registry.read_mark(
            Owner::Background {
                generation: u64::MAX,
            },
            1,
        );
        registry.forget_dead_read_marks();
        assert_eq!(registry.read_mark_count(), 2, "dead and regenerated swept");
        registry.forget_read_mark(background.owner());
        assert_eq!(registry.read_mark_count(), 1);
        assert_eq!(
            registry.read_mark(command.owner(), 9),
            1,
            "the live mark stays"
        );
    }

    fn policy() -> Policy {
        Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 500,
            pacing: None,
            retry: RetryPolicy::ReadFailure,
        }
    }

    #[test]
    fn issuance_captures_egress_that_legs_and_retries_cannot_widen() {
        use crate::request_context::{Credential, RoutingIntent};
        use crate::signing::KeyRef;
        use crate::{CredentialSlot, RequestRoute};
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 1.,
            store: &mut store,
            io: &mut io,
        };
        let signed = |slot| Policy {
            credential: Some(Credential {
                slot,
                reference: KeyRef("slot.credential".into()),
            }),
            ..policy()
        };
        // Keyless primary egress (Bitrue catalog) without a fabricated credential.
        let keyless = cx
            .http_context_with_egress(Scope::Background, policy(), Egress::Primary)
            .unwrap();
        assert_eq!(keyless.routing_intent(), RoutingIntent::Primary);
        assert_eq!(keyless.route(), None);
        assert!(keyless.credential().is_none());
        cx.io.request_contexts.validate(&keyless, cx.now).unwrap();
        // Retries clone it; legs inherit it and may not change or widen it.
        let retry = cx
            .http_retry_context(&keyless, RetryEvidence::ReadFailure)
            .unwrap();
        assert_eq!(retry.egress(), Some(Egress::Primary));
        let leg = cx.http_leg_context(&keyless, policy()).unwrap();
        assert_eq!(leg.routing_intent(), RoutingIntent::Primary);
        assert_eq!(
            cx.http_leg_context(&keyless, signed(CredentialSlot::Secondary)),
            Err(Error::Invalid)
        );
        // A keyed leg under a keyless primary root keeps the pin for its own legs.
        let keyed = cx
            .http_leg_context(&keyless, signed(CredentialSlot::Primary))
            .unwrap();
        assert_eq!(keyed.egress(), Some(Egress::Primary));
        assert_eq!(
            cx.http_leg_context(&keyed, policy())
                .map(|c| c.routing_intent()),
            Ok(RoutingIntent::Primary)
        );
        assert_eq!(
            cx.http_leg_context(&keyed, signed(CredentialSlot::Secondary)),
            Err(Error::Invalid)
        );
        // Public CORS and direct egress (TxFlow candles) project an explicit route.
        for (egress, route) in [
            (Egress::PublicCors, RequestRoute::PublicCors),
            (Egress::Direct, RequestRoute::Direct),
        ] {
            let public = cx
                .http_context_with_egress(Scope::Background, policy(), egress)
                .unwrap();
            assert_eq!(
                (public.routing_intent(), public.route()),
                (RoutingIntent::Public, Some(route))
            );
            let leg = cx.http_leg_context(&public, policy()).unwrap();
            assert_eq!(leg.route(), Some(route));
            // A credential can never ride a public egress.
            assert_eq!(
                cx.http_leg_context(&public, signed(CredentialSlot::Primary)),
                Err(Error::Invalid)
            );
            assert_eq!(
                cx.http_context_with_egress(
                    Scope::Background,
                    signed(CredentialSlot::Primary),
                    egress
                ),
                Err(Error::Invalid)
            );
        }
        // A credentialed context may only confirm a primary slot.
        let confirmed = cx
            .http_context_with_egress(
                Scope::Background,
                signed(CredentialSlot::Primary),
                Egress::Primary,
            )
            .unwrap();
        assert_eq!(confirmed.routing_intent(), RoutingIntent::Primary);
        assert_eq!(
            cx.http_context_with_egress(
                Scope::Background,
                signed(CredentialSlot::Secondary),
                Egress::Primary
            ),
            Err(Error::Invalid)
        );
        // Keyless egress is for reads only.
        let write = Policy {
            operation: Operation::Mutation,
            retry: RetryPolicy::Never,
            ..policy()
        };
        for egress in [Egress::Primary, Egress::PublicCors, Egress::Direct] {
            assert_eq!(
                cx.http_context_with_egress(Scope::Command(9), write.clone(), egress),
                Err(Error::Invalid)
            );
        }
        // A command's completion root holds the egress; its retry still validates.
        let root = cx
            .http_context_with_egress(Scope::Command(7), policy(), Egress::PublicCors)
            .unwrap();
        let retry = cx
            .http_retry_context(&root, RetryEvidence::ReadFailure)
            .unwrap();
        cx.io.request_contexts.validate(&retry, cx.now).unwrap();
        assert_eq!(retry, root);
        // No declaration keeps the credential-derived intent and host routing.
        let plain = cx.http_context(Scope::Background, policy()).unwrap();
        assert_eq!((plain.egress(), plain.route()), (None, None));
    }

    #[test]
    fn execution_observers_record_the_execution_credential() {
        for signed in [false, true] {
            let mut io = super::super::Io::<()>::default();
            let founder = io
                .request_contexts
                .issue(1, Scope::Command(7), policy(), 0.)
                .unwrap();
            io.request_contexts.retain_command_entry(&founder);
            assert!(!io.request_contexts.owner_credentialed(7));
            let mut execution = policy();
            execution.credential = signed.then(|| crate::request_context::Credential {
                slot: crate::CredentialSlot::Primary,
                reference: crate::signing::KeyRef("fixture".into()),
            });
            io.request_contexts
                .begin_execution(
                    100,
                    &founder,
                    execution,
                    crate::execution_group::Window::UntilTerminal,
                    "k",
                    0.,
                )
                .unwrap();
            assert_eq!(io.request_contexts.owner_credentialed(7), signed);
        }
    }

    #[cfg(feature = "http-plans")]
    #[test]
    fn preflight_owners_count_auth_failures_only_with_a_declared_credential() {
        use crate::request_context::{PreflightPolicy, PreflightPurpose};
        use crate::{CredentialSlot, ErrorKind, VenueError};
        let credential = || {
            Some(crate::request_context::Credential {
                slot: CredentialSlot::Primary,
                reference: crate::signing::KeyRef("fixture".into()),
            })
        };
        for (read, write, counted) in [(false, false, 0), (true, false, 1), (true, true, 1)] {
            let mut io = super::super::Io::<()>::default();
            let mut store = Store::default();
            let mut cx = Cx {
                now: 100.,
                store: &mut store,
                io: &mut io,
            };
            let preflight = cx
                .issue_preflight(
                    7,
                    PreflightPolicy {
                        purpose: if write {
                            PreflightPurpose::WriteAfterReads
                        } else {
                            PreflightPurpose::ReadOnly
                        },
                        read_credential: if read { credential() } else { None },
                        write_credential: if write { credential() } else { None },
                    },
                )
                .unwrap();
            let error = VenueError::new("route refused").kind(ErrorKind::AuthInvalid);
            cx.fail_preflight(&preflight, error).unwrap();
            assert_eq!(
                cx.io.health.auth_failures, counted,
                "read={read} write={write}"
            );
        }
    }

    #[test]
    fn only_owners_that_carried_a_credential_count_auth_failures() {
        use crate::{CredentialSlot, ErrorKind, Notice, VenueError};
        let credential = || {
            Some(crate::request_context::Credential {
                slot: CredentialSlot::Primary,
                reference: crate::signing::KeyRef("fixture".into()),
            })
        };
        for kind in [
            ErrorKind::AuthInvalid,
            ErrorKind::AuthExpired,
            ErrorKind::AuthPermission,
        ] {
            // (root credential, leg credential, counted)
            for (root, leg, counted) in [
                (false, None, 0),
                (true, None, 1),
                (false, Some(false), 0),
                (false, Some(true), 1),
            ] {
                let mut io = super::super::Io::<()>::default();
                let mut store = Store::default();
                let mut cx = Cx {
                    now: 100.,
                    store: &mut store,
                    io: &mut io,
                };
                let mut root_policy = policy();
                root_policy.credential = if root { credential() } else { None };
                let first = cx.http_context(Scope::Command(7), root_policy).unwrap();
                let failed = match leg {
                    Some(signed) => {
                        let mut leg_policy = policy();
                        leg_policy.credential = if signed { credential() } else { None };
                        cx.http_context(Scope::Command(7), leg_policy).unwrap()
                    }
                    None => first,
                };
                // A credential-free route can still refuse with an auth kind.
                let error = VenueError::new("route refused")
                    .kind(kind)
                    .venue_code("route");
                cx.fail_context(&failed, error).unwrap();
                assert_eq!(
                    cx.io.health.auth_failures, counted,
                    "{kind:?} root={root} leg={leg:?}"
                );
                // The typed terminal keeps its metadata either way.
                assert!(cx.io.actions.iter().any(|action| matches!(action,
                    crate::Action::Notice(Notice::CommandFailed { command: 7, error })
                        if error.kind == Some(kind) && error.venue_code.as_deref() == Some("route"))));
            }
        }
    }

    #[test]
    fn engine_issues_unique_legs_and_generations_while_a_command_shares_its_owner() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 100.,
            store: &mut store,
            io: &mut io,
        };
        let first = cx.http_context(Scope::Command(7), policy()).unwrap();
        let second = cx.http_context(Scope::Command(7), policy()).unwrap();
        let background = cx.http_context(Scope::Background, policy()).unwrap();
        assert_eq!(first.owner(), second.owner());
        assert_ne!(first.leg(), second.leg());
        assert_ne!(first.leg(), background.leg());
        assert_eq!(first.deadline(), 600.);
        assert!(cx.io.pending.is_empty());
        assert!(cx.io.actions.is_empty());
        assert!(cx.io.timers.is_empty());
        cx.done(7, "null".into());
        assert!(matches!(
            cx.io.actions.as_slice(),
            [crate::Action::Notice(Notice::CommandDone {
                command: 7,
                ..
            })]
        ));
        let reused = cx.http_context(Scope::Command(7), policy()).unwrap();
        assert_ne!(reused.owner(), first.owner());
        assert_eq!(
            cx.http_retry_context(&first, RetryEvidence::ReadFailure),
            Err(Error::Revoked)
        );
        assert!(
            cx.http_retry_context(&reused, RetryEvidence::ReadFailure)
                .is_ok()
        );
    }

    #[test]
    fn invalid_policy_and_full_registry_create_no_owner_or_pending_resource() {
        let mut registry = ContextRegistry::default();
        let mut invalid = policy();
        invalid.operation = Operation::Mutation;
        assert_eq!(
            registry.issue(1, Scope::Command(7), invalid, 0.),
            Err(Error::RetryForbidden)
        );
        assert!(registry.contexts.is_empty());
        assert!(registry.commands.is_empty());
        for id in 1..=CAPACITY as u32 {
            registry
                .issue(u64::from(id), Scope::Background, policy(), 0.)
                .unwrap();
        }
        assert_eq!(
            registry.issue(300, Scope::Command(7), policy(), 1.),
            Err(Error::Capacity)
        );
        assert!(registry.commands.is_empty());
        assert_eq!(registry.contexts.len(), CAPACITY);
        // Expired logical contexts release capacity; their original handles
        // remain expired rather than silently acquiring the new deadline.
        let old = registry.contexts.get(&1).unwrap().clone();
        registry
            .issue(301, Scope::Command(7), policy(), 500.)
            .unwrap();
        assert_eq!(registry.contexts.len(), 1);
        assert_eq!(registry.validate(&old, 500.), Err(Error::TimedOut));
    }

    #[test]
    fn retry_retains_original_deadline_and_rejects_unissued_or_changed_context() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 100.,
            store: &mut store,
            io: &mut io,
        };
        let context = cx.http_context(Scope::Command(7), policy()).unwrap();
        cx.now = 599.;
        let retry = cx
            .http_retry_context(&context, RetryEvidence::ReadFailure)
            .unwrap();
        assert_eq!(retry, context);
        let altered = context.clone().with_retry(RetryPolicy::Never).unwrap();
        assert_eq!(
            cx.http_retry_context(&altered, RetryEvidence::ReadFailure),
            Err(Error::Revoked)
        );
        let unissued = Context::new(
            context.owner(),
            9_000,
            Operation::Read,
            None,
            100.,
            600.,
            None,
        )
        .unwrap()
        .with_retry(RetryPolicy::ReadFailure)
        .unwrap();
        assert_eq!(
            cx.http_retry_context(&unissued, RetryEvidence::ReadFailure),
            Err(Error::Revoked)
        );
        cx.now = 600.;
        assert_eq!(
            cx.http_retry_context(&retry, RetryEvidence::ReadFailure),
            Err(Error::TimedOut)
        );
    }

    #[test]
    fn failure_retires_context_before_reused_command_gets_a_fresh_generation() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 100.,
            store: &mut store,
            io: &mut io,
        };
        let first = cx.http_context(Scope::Command(7), policy()).unwrap();
        cx.fail(7, "failed");
        assert_eq!(
            cx.http_retry_context(&first, RetryEvidence::ReadFailure),
            Err(Error::Revoked)
        );
        let second = cx.http_context(Scope::Command(7), policy()).unwrap();
        assert_ne!(first.owner(), second.owner());
    }

    #[test]
    fn identical_local_counters_cannot_authorize_another_engines_context() {
        let mut first = ContextRegistry::default();
        let mut second = ContextRegistry::default();
        let left = first.issue(1, Scope::Command(7), policy(), 100.).unwrap();
        let right = second.issue(1, Scope::Command(7), policy(), 100.).unwrap();
        assert_eq!(left.leg(), right.leg());
        assert_eq!(left.owner(), right.owner());
        assert_eq!(first.validate(&left, 101.), Ok(()));
        assert_eq!(second.validate(&right, 101.), Ok(()));
        assert_eq!(first.validate(&right, 101.), Err(Error::Revoked));
        assert_eq!(second.validate(&left, 101.), Err(Error::Revoked));
    }

    #[test]
    fn expired_abandoned_command_owners_release_registry_capacity() {
        let mut registry = ContextRegistry::default();
        for id in 1..=CAPACITY as u32 {
            registry
                .issue(u64::from(id), Scope::Command(id), policy(), 0.)
                .unwrap();
        }
        assert_eq!(registry.commands.len(), CAPACITY);
        let fresh = registry
            .issue(301, Scope::Command(301), policy(), 500.)
            .unwrap();
        assert_eq!(fresh.deadline(), 1_000.);
        assert_eq!(registry.contexts.len(), 1);
        assert_eq!(registry.commands.len(), 1);
    }

    #[test]
    fn expired_cleanup_retires_only_the_registered_handle_without_send_rights() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 100.,
            store: &mut store,
            io: &mut io,
        };
        let mut foreign = super::super::Io::<()>::default();
        let mut foreign_store = Store::default();
        let mut other = Cx {
            now: 100.,
            store: &mut foreign_store,
            io: &mut foreign,
        };
        let foreign_handle = other.http_context(Scope::Background, policy()).unwrap();
        for _ in 0..CAPACITY * 2 {
            let context = cx.http_context(Scope::Background, policy()).unwrap();
            cx.now = context.deadline();
            assert_eq!(
                cx.http_retry_context(&context, RetryEvidence::ReadFailure),
                Err(Error::TimedOut)
            );
            assert_eq!(cx.finish_http_context(&foreign_handle), Err(Error::Revoked));
            cx.finish_http_context(&context).unwrap();
            assert_eq!(cx.finish_http_context(&context), Err(Error::Revoked));
            assert!(cx.io.request_contexts.contexts.is_empty());
        }
        let old = cx.http_context(Scope::Command(7), policy()).unwrap();
        cx.now = old.deadline();
        let replacement = cx.http_context(Scope::Command(7), policy()).unwrap();
        assert_ne!(old.owner(), replacement.owner());
        assert_eq!(cx.finish_http_context(&old), Err(Error::Revoked));
        assert!(
            cx.io
                .request_contexts
                .validate(&replacement, cx.now)
                .is_ok()
        );
    }

    #[test]
    fn retiring_background_read_frees_capacity_before_followup_admission() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 100.,
            store: &mut store,
            io: &mut io,
        };
        let mut contexts = Vec::new();
        for _ in 0..CAPACITY {
            contexts.push(cx.http_context(Scope::Background, policy()).unwrap());
        }
        assert_eq!(
            cx.http_context(Scope::Background, policy()),
            Err(Error::Capacity)
        );
        cx.finish_http_context(&contexts.remove(0)).unwrap();
        let replacement = cx.http_context(Scope::Background, policy()).unwrap();
        assert_eq!(cx.io.request_contexts.contexts.len(), CAPACITY);
        cx.finish_http_context(&replacement).unwrap();
        for context in contexts {
            cx.finish_http_context(&context).unwrap();
        }
        assert!(cx.io.request_contexts.contexts.is_empty());
    }

    #[test]
    fn logical_completion_releases_background_capacity_without_waiting_for_timeout() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 100.,
            store: &mut store,
            io: &mut io,
        };
        for _ in 0..CAPACITY * 2 {
            let context = cx.http_context(Scope::Background, policy()).unwrap();
            cx.finish_http_context(&context).unwrap();
            assert_eq!(cx.finish_http_context(&context), Err(Error::Revoked));
            assert_eq!(
                cx.http_retry_context(&context, RetryEvidence::ReadFailure),
                Err(Error::Revoked)
            );
        }
        assert!(cx.io.request_contexts.contexts.is_empty());
        assert!(cx.io.request_contexts.commands.is_empty());
        assert!(cx.io.pending.is_empty());
        assert!(cx.io.actions.is_empty());
    }

    #[test]
    fn completing_one_leg_retains_owner_and_never_extends_a_siblings_original_bound() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 100.,
            store: &mut store,
            io: &mut io,
        };
        let first = cx.http_context(Scope::Command(7), policy()).unwrap();
        cx.finish_http_context(&first).unwrap();
        cx.now = 200.;
        let sibling = cx.http_context(Scope::Command(7), policy()).unwrap();
        assert_eq!(first.owner(), sibling.owner());
        assert_eq!(first.deadline(), 600.);
        assert_eq!(sibling.deadline(), 700.);
        cx.now = 600.;
        let next = cx.http_context(Scope::Command(7), policy()).unwrap();
        assert_eq!(next.owner(), sibling.owner());
        assert_eq!(sibling.deadline(), 700.);
        assert_eq!(
            cx.http_retry_context(&first, RetryEvidence::ReadFailure),
            Err(Error::TimedOut)
        );
        assert!(
            cx.http_retry_context(&sibling, RetryEvidence::ReadFailure)
                .is_ok()
        );
        cx.now = 1_100.;
        let reused = cx.http_context(Scope::Command(7), policy()).unwrap();
        assert_ne!(reused.owner(), sibling.owner());
    }
}

#[cfg(test)]
mod write_tests {
    use super::*;
    use crate::{Notice, Outcome, Store};

    fn policy() -> Policy {
        Policy {
            operation: Operation::Mutation,
            credential: None,
            timeout_ms: 100,
            pacing: None,
            retry: crate::request_context::RetryPolicy::DefinitiveRefusal,
        }
    }
    fn delivery(context: &Context, id: u64) -> Delivery {
        Delivery {
            context: context.clone(),
            attempt: Attempt {
                request: RequestId(id),
                generation: id,
            },
        }
    }
    fn rejected() -> WriteResult {
        WriteResult::Rejected(VenueError::new("native refusal").venue_code("10002"))
    }
    fn accepted() -> WriteResult {
        WriteResult::Accepted {
            order_ids: vec!["accepted-id".into()],
        }
    }

    #[test]
    fn write_facts_are_bound_to_their_attempt_intent_and_placement_ids() {
        use crate::classify::{self, CodeTable, Intent, Native, NativeCode, Verdict, WriteFact};
        const REFUSE: CodeTable = CodeTable {
            rows: &[],
            success: &[],
            unlisted: Verdict::Refused(crate::ErrorKind::Rejected),
            status: &[],
        };
        let native = Native {
            code: NativeCode::Int(-1),
            message: "refused",
        };
        let released = |intent: Option<Intent>| {
            let mut io = super::super::Io::<()>::default();
            io.request_contexts.dispatch = intent.map(|intent| (7, intent));
            let context = io
                .request_contexts
                .issue(1, Scope::Command(7), policy(), 0.)
                .unwrap();
            io.request_contexts.dispatch = None;
            for id in [11, 12] {
                let _ = io
                    .request_contexts
                    .register_attempt(&context, RequestId(id), 0.);
            }
            *io.request_contexts
                .release_flag(&context, RequestId(11))
                .unwrap()
                .unwrap() = true;
            io.http_delivery = Some(delivery(&context, 11));
            io.http_write_reply = Some((400, String::new()));
            (io, context)
        };

        // A fact classified for one attempt cannot be recorded on another.
        let (mut io, context) = released(Some(Intent::Place));
        let mut store = Store::default();
        let mut cx = Cx {
            now: 1.,
            store: &mut store,
            io: &mut io,
        };
        let fact = cx
            .classify_write(&context, &REFUSE, Some(&native), "x")
            .unwrap();
        cx.io.http_delivery = Some(delivery(&context, 12));
        let failed = WriteFact::Failed {
            fact: fact.clone(),
            known_effects: false,
        };
        assert_eq!(
            cx.http_write_fact(&context, failed, None).unwrap_err(),
            Error::Revoked
        );
        // An attempt-free fact may only be an unproven answer.
        cx.io.http_delivery = Some(delivery(&context, 11));
        let (_, error) = classify::write(
            &REFUSE,
            Intent::Place,
            classify::Released::Transport,
            None,
            "x",
        );
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        let forged = classify::fact(
            &REFUSE,
            Intent::Place,
            classify::Released::Response(&crate::Response {
                status: 400,
                headers: "",
                body: "",
            }),
            Some(&native),
            "x",
        );
        let forged = WriteFact::Failed {
            fact: forged,
            known_effects: false,
        };
        assert_eq!(
            cx.http_write_fact(&context, forged, None).unwrap_err(),
            Error::Revoked
        );
        // A placement acknowledgement without native IDs proves nothing.
        let empty = WriteFact::Accepted {
            order_ids: Vec::new(),
        };
        cx.http_write_fact(&context, empty, None).unwrap();
        let Seal::Failure(error, _) = cx.io.request_contexts.seal(
            7,
            Some(&delivery(&context, 11)),
            Some(&context),
            Some(VenueError::new("fallback")),
        ) else {
            panic!("an empty placement ACK cannot seal success");
        };
        assert_eq!(error.outcome, Some(Outcome::Unknown));

        // Owners issued outside command dispatch have no trusted intent.
        let (mut io, context) = released(None);
        let mut store = Store::default();
        let cx = Cx {
            now: 1.,
            store: &mut store,
            io: &mut io,
        };
        assert_eq!(
            cx.classify_write(&context, &REFUSE, Some(&native), "x")
                .unwrap_err(),
            Error::State
        );
    }

    #[test]
    fn leg_intents_only_narrow_what_the_command_implies() {
        use crate::classify::Intent;
        for (commanded, leg, allowed) in [
            (Intent::Amend, Intent::Place, true),
            // An amend's attached TP/SL update is a setting.
            (Intent::Amend, Intent::Setting, true),
            (Intent::Place, Intent::Setting, true),
            // A placement's conflicting-order cancel is a real cancel.
            (Intent::Place, Intent::Cancel, true),
            // An amend's own cancel leg never borrows cancel success generically.
            (Intent::Amend, Intent::Cancel, false),
            (Intent::Cancel, Intent::Place, false),
            (Intent::Setting, Intent::Cancel, false),
        ] {
            let mut io = super::super::Io::<()>::default();
            let mut store = Store::default();
            io.request_contexts.dispatch = Some((7, commanded));
            let context = io
                .request_contexts
                .issue(1, Scope::Command(7), policy(), 0.)
                .unwrap();
            io.request_contexts.dispatch = None;
            let mut cx = Cx {
                now: 1.,
                store: &mut store,
                io: &mut io,
            };
            let narrowed = cx.http_write_leg_intent(&context, leg);
            assert_eq!(narrowed.is_ok(), allowed, "{commanded:?} -> {leg:?}");
            if allowed {
                assert_eq!(cx.write_intent(&context), Ok(leg));
                assert_eq!(
                    cx.http_write_leg_intent(&context, leg),
                    Err(Error::Duplicate)
                );
            } else {
                assert_eq!(cx.write_intent(&context), Ok(commanded));
            }
        }
        // The command's own intent is a no-op; nothing narrows after a send.
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        io.request_contexts.dispatch = Some((7, Intent::Place));
        let context = io
            .request_contexts
            .issue(1, Scope::Command(7), policy(), 0.)
            .unwrap();
        io.request_contexts.dispatch = None;
        io.request_contexts
            .register_attempt(&context, RequestId(11), 0.)
            .unwrap();
        let mut cx = Cx {
            now: 1.,
            store: &mut store,
            io: &mut io,
        };
        assert_eq!(
            cx.http_write_leg_intent(&context, Intent::Place),
            Err(Error::State)
        );
        assert_eq!(
            cx.http_write_leg_intent(&context, Intent::Cancel),
            Err(Error::State)
        );
        assert_eq!(cx.write_intent(&context), Ok(Intent::Place));

        // The one declared amend-cancel exception: only an amend's leg, once,
        // before any attempt; any other command refuses it.
        for (commanded, allowed) in [
            (Intent::Amend, true),
            (Intent::Place, false),
            (Intent::Cancel, false),
            (Intent::Setting, false),
        ] {
            let mut io = super::super::Io::<()>::default();
            let mut store = Store::default();
            io.request_contexts.dispatch = Some((7, commanded));
            let context = io
                .request_contexts
                .issue(1, Scope::Command(7), policy(), 0.)
                .unwrap();
            io.request_contexts.dispatch = None;
            let mut cx = Cx {
                now: 1.,
                store: &mut store,
                io: &mut io,
            };
            if commanded == Intent::Cancel {
                // A Cancel command's own intent needs no exception.
                assert_eq!(cx.http_write_protection_cancel_leg(&context), Ok(()));
                continue;
            }
            assert_eq!(
                cx.http_write_protection_cancel_leg(&context).is_ok(),
                allowed,
                "{commanded:?}"
            );
            let expected = if allowed { Intent::Cancel } else { commanded };
            assert_eq!(cx.write_intent(&context), Ok(expected));
        }
    }

    #[test]
    fn classification_takes_intent_from_the_command_and_status_from_the_engine() {
        use crate::classify::{
            CodeTable, Intent, Key, Native, NativeCode, Row, SatisfiedIntent, Verdict,
        };
        const GONE: CodeTable = CodeTable {
            rows: &[Row {
                key: Key::Int(-2011),
                verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
                only: Some(Intent::Cancel),
            }],
            success: &[],
            unlisted: Verdict::Unknown(None),
            status: &[],
        };
        let native = Native {
            code: NativeCode::Int(-2011),
            message: "Unknown order sent.",
        };
        for (intent, expected) in [
            (
                Some(Intent::Cancel),
                Verdict::Satisfied(SatisfiedIntent::Cancel),
            ),
            // An amend's cancel leg can never inherit cancel success.
            (Some(Intent::Amend), Verdict::Unknown(None)),
            // A venue action classifies conservatively as a placement.
            (Some(Intent::Place), Verdict::Unknown(None)),
        ] {
            let mut io = super::super::Io::<()>::default();
            let mut store = Store::default();
            io.request_contexts.dispatch = intent.map(|intent| (7, intent));
            let context = io
                .request_contexts
                .issue(1, Scope::Command(7), policy(), 0.)
                .unwrap();
            io.request_contexts.dispatch = None;
            io.http_delivery = Some(delivery(&context, 11));
            let cx = Cx {
                now: 1.,
                store: &mut store,
                io: &mut io,
            };
            assert_eq!(
                cx.classify_write(&context, &GONE, Some(&native), "x"),
                Err(Error::State),
                "no delivered reply"
            );
            cx.io.http_write_reply = Some((400, String::new()));
            let fact = cx
                .classify_write(&context, &GONE, Some(&native), "x")
                .unwrap();
            assert_eq!(fact.verdict(), expected, "{intent:?}");
            cx.io.http_write_reply = Some((503, String::new()));
            let fact = cx
                .classify_write(&context, &GONE, Some(&native), "x")
                .unwrap();
            assert!(matches!(fact.verdict(), Verdict::Unknown(Some(_))));
        }
    }

    #[test]
    fn native_report_api_preserves_delivery_authority_and_publishes_each_batch_message_once() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let first = io
            .request_contexts
            .issue(1, Scope::Command(7), policy(), 0.)
            .unwrap();
        let second = io
            .request_contexts
            .issue(2, Scope::Command(7), policy(), 0.)
            .unwrap();
        let foreign = ContextRegistry::default()
            .issue(1, Scope::Command(7), policy(), 0.)
            .unwrap();
        for (context, id) in [(&first, 11), (&second, 12)] {
            io.request_contexts
                .register_attempt(context, RequestId(id), 0.)
                .unwrap();
            *io.request_contexts
                .release_flag(context, RequestId(id))
                .unwrap()
                .unwrap() = true;
        }
        io.http_delivery = Some(delivery(&second, 12));
        let mut cx = Cx {
            now: 1.,
            store: &mut store,
            io: &mut io,
        };
        for context in [&first, &foreign] {
            assert_eq!(
                cx.http_write_result_with_reports(
                    context,
                    rejected(),
                    RejectionReports::new(vec!["wrong".into()]).unwrap()
                ),
                Err(Error::Revoked)
            );
        }
        cx.http_write_result_with_reports(
            &second,
            rejected(),
            RejectionReports::new(vec!["B1".into(), "B2".into()]).unwrap(),
        )
        .unwrap();
        assert_eq!(
            cx.http_write_result_with_reports(
                &second,
                rejected(),
                RejectionReports::new(vec!["duplicate".into()]).unwrap()
            ),
            Err(Error::State)
        );
        cx.io.http_delivery = Some(delivery(&first, 11));
        cx.http_write_result_with_reports(
            &first,
            rejected(),
            RejectionReports::new(vec!["A1".into(), "A2".into()]).unwrap(),
        )
        .unwrap();
        cx.fail_context(&first, "fallback").unwrap();
        assert_eq!(cx.fail_context(&first, "duplicate"), Err(Error::Revoked));
        let messages: Vec<_> = cx
            .io
            .actions
            .iter()
            .filter_map(|a| match a {
                crate::Action::Notice(Notice::CommandError { message, .. }) => {
                    Some(message.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(messages, ["A1", "A2", "B1", "B2"]);
        assert_eq!(cx.io.actions.iter().filter(|a| matches!(a,
            crate::Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(Outcome::Rejected))).count(), 1);
    }

    #[test]
    fn engine_done_cannot_erase_unsent_unresolved_native_refusal_or_known_partial() {
        for state in 0..5 {
            let mut io = super::super::Io::<()>::default();
            let mut store = Store::default();
            let first = io
                .request_contexts
                .issue(1, Scope::Command(7), policy(), 0.)
                .unwrap();
            io.request_contexts
                .register_attempt(&first, RequestId(11), 0.)
                .unwrap();
            if state != 0 {
                *io.request_contexts
                    .release_flag(&first, RequestId(11))
                    .unwrap()
                    .unwrap() = true;
            }
            if state >= 2 {
                io.request_contexts
                    .result(
                        &delivery(&first, 11),
                        if state == 2 { rejected() } else { accepted() },
                    )
                    .unwrap();
            }
            if state == 4 {
                let second = io
                    .request_contexts
                    .issue(2, Scope::Command(7), policy(), 0.)
                    .unwrap();
                io.request_contexts
                    .register_attempt(&second, RequestId(12), 0.)
                    .unwrap();
                *io.request_contexts
                    .release_flag(&second, RequestId(12))
                    .unwrap()
                    .unwrap() = true;
                io.request_contexts
                    .result(&delivery(&second, 12), rejected())
                    .unwrap();
            }
            io.http_delivery = Some(delivery(&first, 11));
            let mut cx = Cx {
                now: 1.,
                store: &mut store,
                io: &mut io,
            };
            cx.done_context(&first, "[]".into()).unwrap();
            cx.io.http_delivery = None;
            assert_eq!(
                cx.done_context(&first, "duplicate".into()),
                Err(Error::Revoked)
            );
            assert_eq!(cx.fail_context(&first, "late timer"), Err(Error::Revoked));
            let done = cx
                .io
                .actions
                .iter()
                .filter(|a| matches!(a, crate::Action::Notice(Notice::CommandDone { .. })))
                .count();
            let failed = cx
                .io
                .actions
                .iter()
                .filter_map(|a| match a {
                    crate::Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if state == 3 {
                assert_eq!(done, 1);
                assert!(failed.is_empty());
            } else {
                assert_eq!(done, 0);
                assert_eq!(failed.len(), 1);
                assert_eq!(
                    failed[0].outcome,
                    match state {
                        0 => Some(Outcome::NotSent),
                        1 => Some(Outcome::Unknown),
                        2 => Some(Outcome::Rejected),
                        _ => None,
                    }
                );
                if state == 4 {
                    assert_eq!(
                        failed[0].partial.as_ref().unwrap().order_ids,
                        ["accepted-id"]
                    );
                }
            }
        }
    }

    #[test]
    fn prior_send_evidence_survives_native_refusal_retry_and_signer_failure() {
        let mut registry = ContextRegistry::default();
        let context = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
        registry
            .register_attempt(&context, RequestId(11), 0.)
            .unwrap();
        *registry
            .release_flag(&context, RequestId(11))
            .unwrap()
            .unwrap() = true;
        registry
            .result(&delivery(&context, 11), rejected())
            .unwrap();
        registry.grant_resend(&delivery(&context, 11)).unwrap();
        registry
            .register_attempt(&context, RequestId(12), 1.)
            .unwrap();
        registry
            .result(
                &delivery(&context, 12),
                WriteResult::Failed(VenueError::new("provider refused").outcome(Outcome::NotSent)),
            )
            .unwrap();
        let Seal::Failure(error, _) = registry.seal(
            7,
            Some(&delivery(&context, 12)),
            Some(&context),
            Some(VenueError::new("fallback")),
        ) else {
            panic!("typed failure");
        };
        assert_eq!(error.outcome, Some(Outcome::Unknown));
    }

    #[test]
    fn expired_live_handles_and_sibling_issuance_do_not_erase_sent_journals() {
        let mut registry = ContextRegistry::default();
        let context = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
        registry
            .register_attempt(&context, RequestId(11), 0.)
            .unwrap();
        *registry
            .release_flag(&context, RequestId(11))
            .unwrap()
            .unwrap() = true;
        registry.issue(2, Scope::Command(7), policy(), 0.).unwrap();
        registry
            .issue(
                3,
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    ..policy()
                },
                101.,
            )
            .unwrap();
        assert_eq!(registry.validate(&context, 101.), Err(Error::TimedOut));
        assert_eq!(
            registry.issue(4, Scope::Command(7), policy(), 101.),
            Err(Error::State)
        );
        let Seal::Failure(error, _) =
            registry.seal(7, None, Some(&context), Some(VenueError::new("timeout")))
        else {
            panic!("sent journal retained");
        };
        assert_eq!(error.outcome, Some(Outcome::Unknown));
    }

    #[test]
    fn stale_delivery_cannot_settle_a_new_command_reusing_the_numeric_id() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let old = io
            .request_contexts
            .issue(1, Scope::Command(7), policy(), 0.)
            .unwrap();
        io.request_contexts.finish(7);
        let current = io
            .request_contexts
            .issue(2, Scope::Command(7), policy(), 0.)
            .unwrap();
        io.request_contexts
            .register_attempt(&current, RequestId(12), 0.)
            .unwrap();
        io.http_delivery = Some(delivery(&old, 11));
        let mut cx = Cx {
            now: 1.,
            store: &mut store,
            io: &mut io,
        };
        cx.done(7, "old success".into());
        cx.fail(7, "old failure");
        assert!(cx.io.actions.is_empty());
        assert_eq!(
            cx.http_write_result(&current, accepted()),
            Err(Error::Revoked)
        );
        assert!(cx.io.request_contexts.validate(&current, 1.).is_ok());
    }

    #[test]
    fn denied_release_and_unproven_retry_consume_no_release_or_new_attempt() {
        let mut registry = ContextRegistry::default();
        let context = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
        registry
            .register_attempt(&context, RequestId(11), 0.)
            .unwrap();
        let _uncommitted = registry.release_flag(&context, RequestId(11)).unwrap();
        assert_eq!(
            registry.result(&delivery(&context, 11), accepted()),
            Err(Error::State)
        );
        assert_eq!(
            registry.register_attempt(&context, RequestId(12), 1.),
            Err(Error::RetryForbidden)
        );
        let Seal::Failure(error, _) = registry.seal(
            7,
            None,
            Some(&context),
            Some(VenueError::new("refused before release")),
        ) else {
            panic!("failure");
        };
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }

    #[test]
    fn a_sibling_command_delivery_cannot_take_or_seal_another_command_journal() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let first = io
            .request_contexts
            .issue(1, Scope::Command(7), policy(), 0.)
            .unwrap();
        let second = io
            .request_contexts
            .issue(2, Scope::Command(8), policy(), 0.)
            .unwrap();
        io.request_contexts
            .register_attempt(&first, RequestId(11), 0.)
            .unwrap();
        io.request_contexts
            .register_attempt(&second, RequestId(12), 0.)
            .unwrap();
        *io.request_contexts
            .release_flag(&second, RequestId(12))
            .unwrap()
            .unwrap() = true;
        io.request_contexts
            .result(&delivery(&second, 12), accepted())
            .unwrap();
        io.http_delivery = Some(delivery(&first, 11));
        let mut cx = Cx {
            now: 1.,
            store: &mut store,
            io: &mut io,
        };
        assert_eq!(cx.validate_completion_context(&second), Err(Error::Revoked));
        assert_eq!(
            cx.done_context(&second, "wrong delivery".into()),
            Err(Error::Revoked)
        );
        assert_eq!(
            cx.fail_context(&second, "wrong delivery"),
            Err(Error::Revoked)
        );
        assert!(cx.io.actions.is_empty());
        cx.io.http_delivery = None;
        cx.done(8, "unqualified".into());
        cx.fail(8, "unqualified");
        assert!(
            cx.io.actions.is_empty(),
            "numeric finalizers cannot bypass an active managed journal"
        );
        cx.done_context(&second, "accepted".into()).unwrap();
        assert!(matches!(
            cx.io.actions.as_slice(),
            [crate::Action::Notice(Notice::CommandDone {
                command: 8,
                ..
            })]
        ));
        assert!(cx.io.request_contexts.validate(&first, 1.).is_ok());
    }

    #[test]
    fn a_historical_attempt_cannot_seal_the_newer_retry_of_the_same_command() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let context = io
            .request_contexts
            .issue(1, Scope::Command(7), policy(), 0.)
            .unwrap();
        io.request_contexts
            .register_attempt(&context, RequestId(11), 0.)
            .unwrap();
        *io.request_contexts
            .release_flag(&context, RequestId(11))
            .unwrap()
            .unwrap() = true;
        io.request_contexts
            .result(&delivery(&context, 11), rejected())
            .unwrap();
        io.request_contexts
            .grant_resend(&delivery(&context, 11))
            .unwrap();
        io.request_contexts
            .register_attempt(&context, RequestId(12), 1.)
            .unwrap();
        io.http_delivery = Some(delivery(&context, 11));
        let mut cx = Cx {
            now: 2.,
            store: &mut store,
            io: &mut io,
        };
        assert_eq!(
            cx.done_context(&context, "late old ACK".into()),
            Err(Error::Revoked)
        );
        assert_eq!(
            cx.fail_context(&context, "late old failure"),
            Err(Error::Revoked)
        );
        assert!(cx.io.actions.is_empty());
        cx.io.http_delivery = Some(delivery(&context, 12));
        cx.fail_context(&context, "new signer failed").unwrap();
        assert!(
            matches!(cx.io.actions.last(), Some(crate::Action::Notice(Notice::CommandFailed { error, .. })) if error.outcome == Some(Outcome::Unknown))
        );
    }

    #[test]
    fn a_retired_command_context_never_falls_back_to_legacy_without_a_delivery_marker() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let context = io
            .request_contexts
            .issue(1, Scope::Command(7), policy(), 0.)
            .unwrap();
        io.request_contexts
            .register_attempt(&context, RequestId(11), 0.)
            .unwrap();
        let mut cx = Cx {
            now: 1.,
            store: &mut store,
            io: &mut io,
        };
        cx.fail_context(&context, "before send").unwrap();
        assert_eq!(
            cx.done_context(&context, "late".into()),
            Err(Error::Revoked)
        );
        assert_eq!(
            cx.fail_context(&context, "late socket"),
            Err(Error::Revoked)
        );
        let current = cx
            .io
            .request_contexts
            .issue(2, Scope::Command(7), policy(), 1.)
            .unwrap();
        assert_eq!(
            cx.done_context(&context, "old reused id".into()),
            Err(Error::Revoked)
        );
        assert!(cx.io.request_contexts.validate(&current, 2.).is_ok());
        assert_eq!(cx.io.actions.len(), 1);
    }

    #[test]
    fn preparation_failure_before_any_attempt_registration_is_still_not_sent() {
        let mut io = super::super::Io::<()>::default();
        let mut store = Store::default();
        let context = io
            .request_contexts
            .issue(1, Scope::Command(7), policy(), 0.)
            .unwrap();
        assert!(
            io.request_contexts.commands[&7].writes.is_none(),
            "no phantom physical attempt"
        );
        let mut read = policy();
        read.operation = Operation::Read;
        read.retry = crate::request_context::RetryPolicy::Never;
        let read = io
            .request_contexts
            .issue(2, Scope::Command(7), read, 0.)
            .unwrap();
        let mut cx = Cx {
            now: 1.,
            store: &mut store,
            io: &mut io,
        };
        cx.done(7, "unqualified".into());
        assert!(cx.io.actions.is_empty());
        cx.fail_context(&read, "recipe refused before registration")
            .unwrap();
        assert!(
            matches!(cx.io.actions.as_slice(), [crate::Action::Notice(Notice::CommandFailed { error, .. })] if error.outcome == Some(Outcome::NotSent))
        );
        assert_eq!(
            cx.done_context(&context, "duplicate".into()),
            Err(Error::Revoked)
        );
    }

    #[test]
    fn expired_read_preflight_keeps_declared_write_completion_without_a_phantom_write() {
        let mut registry = ContextRegistry::default();
        let mutation = registry.issue(1, Scope::Command(7), policy(), 0.).unwrap();
        let mut read = policy();
        read.operation = Operation::Read;
        read.retry = crate::request_context::RetryPolicy::Never;
        let preflight = registry.issue(2, Scope::Command(7), read, 0.).unwrap();
        registry
            .register_attempt(&preflight, RequestId(12), 0.)
            .unwrap();
        assert!(registry.commands[&7].writes.is_none());
        let callback = delivery(&preflight, 12);
        registry.enter_delivery(&callback);
        registry.retire_attempt(&preflight, RequestId(12));
        registry
            .issue(
                3,
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    ..policy()
                },
                101.,
            )
            .unwrap();
        assert_eq!(registry.validate(&mutation, 101.), Err(Error::TimedOut));
        let Seal::Failure(error, _) = registry.seal(
            7,
            Some(&callback),
            Some(&mutation),
            Some(VenueError::new("preflight timed out")),
        ) else {
            panic!("pending completion retained");
        };
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        registry.finish(7);
        registry.exit_delivery(&callback);
        assert!(!registry.commands.contains_key(&7));
    }

    #[test]
    fn abandoned_unqueued_mutation_intents_still_release_capacity_at_deadline() {
        let mut registry = ContextRegistry::default();
        for id in 1..=CAPACITY as u32 {
            registry
                .issue(u64::from(id), Scope::Command(id), policy(), 0.)
                .unwrap();
        }
        assert_eq!(registry.commands.len(), CAPACITY);
        registry
            .issue(1000, Scope::Command(1000), policy(), 101.)
            .unwrap();
        assert_eq!(registry.commands.len(), 1);
        assert_eq!(registry.contexts.len(), 1);
    }
}
