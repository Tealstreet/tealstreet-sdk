//! Engine-issued logical context identities. Physical attempts and transport
//! resources are registered only after this immutable logical bound is valid.
use super::{CommandId, Cx, RequestId};
use crate::VenueError;
use crate::request_context::{
    Attempt, CommandWrites, Context, Error, Operation, Owner, Policy, RetryEvidence, Scope,
    WriteResult,
};
use std::collections::HashMap;

const CAPACITY: usize = 256;

struct CommandOwner {
    generation: u64,
    /// Owner retention only. This never extends any issued logical deadline.
    last_deadline: f64,
    writes: Option<CommandWrites>,
    mutation_declared: bool,
    pending: usize,
    deliveries: usize,
}

#[derive(Clone)]
pub(super) struct Delivery {
    pub context: Context,
    pub attempt: Attempt,
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
    contexts: HashMap<u64, Context>,
    commands: HashMap<CommandId, CommandOwner>,
}

impl ContextRegistry {
    fn issue(
        &mut self,
        identity: u32,
        scope: Scope,
        policy: Policy,
        now: f64,
    ) -> Result<Context, Error> {
        if !now.is_finite() || policy.timeout_ms == 0 {
            return Err(Error::Invalid);
        }
        // Abandoned handles must not permanently exhaust the bounded registry.
        // Retiring expired metadata is safe even if the new policy is refused.
        self.contexts.retain(|_, old| old.deadline() > now);
        // A delayed original timer must not let unrelated new work erase sent
        // evidence. Active journals are bounded and survive live-handle expiry.
        self.commands.retain(|_, old| {
            old.last_deadline > now || old.writes.is_some() || old.pending > 0 || old.deliveries > 0
        });
        if matches!(scope, Scope::Command(id) if self.commands.get(&id)
            .is_some_and(|owner| (owner.writes.is_some() || owner.pending > 0 || owner.deliveries > 0) && now >= owner.last_deadline))
        {
            return Err(Error::State);
        }
        let identity = u64::from(identity);
        let owner = match scope {
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
        };
        // Validate before committing an owner, context, pending slot or action.
        let mut context = Context::new(
            owner,
            identity,
            policy.operation,
            policy.credential,
            now,
            now + f64::from(policy.timeout_ms),
            policy.pacing,
        )?
        .with_retry(policy.retry)?;
        if self.contexts.len() >= CAPACITY
            || matches!(scope, Scope::Command(id)
                if !self.commands.contains_key(&id) && self.commands.len() >= CAPACITY)
        {
            return Err(Error::Capacity);
        }
        if self.contexts.contains_key(&identity) {
            return Err(Error::Duplicate);
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
                })
                .or_insert(CommandOwner {
                    generation,
                    last_deadline,
                    writes: None,
                    mutation_declared: context.operation() == Operation::Mutation,
                    pending: 0,
                    deliveries: 0,
                });
        }
        self.contexts.insert(identity, context.clone());
        Ok(context)
    }

    pub(super) fn validate(&self, context: &Context, now: f64) -> Result<(), Error> {
        if !now.is_finite() {
            return Err(Error::Invalid);
        }
        if now >= context.deadline() {
            return Err(Error::TimedOut);
        }
        if !context.issued_by(&self.issuer)
            || self.contexts.get(&context.leg()) != Some(context)
            || matches!(context.owner(), Owner::Command {id, generation}
                if self.commands.get(&id).is_none_or(|owner| owner.generation != generation))
        {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    fn command_mut(&mut self, context: &Context) -> Result<&mut CommandOwner, Error> {
        if !context.issued_by(&self.issuer) {
            return Err(Error::Revoked);
        }
        let Owner::Command { id, generation } = context.owner() else {
            return Err(Error::Invalid);
        };
        self.commands
            .get_mut(&id)
            .filter(|owner| owner.generation == generation)
            .ok_or(Error::Revoked)
    }

    pub(super) fn register_attempt(
        &mut self,
        context: &Context,
        request: RequestId,
        now: f64,
    ) -> Result<(), Error> {
        self.validate(context, now)?;
        if matches!(context.owner(), Owner::Background { .. })
            && context.operation() == Operation::Read
        {
            return Ok(());
        }
        let owner = self.command_mut(context)?;
        let pending = owner.pending.checked_add(1).ok_or(Error::Capacity)?;
        if context.operation() == Operation::Mutation {
            if let Some(writes) = owner.writes.as_mut() {
                if let Some(previous) = writes.latest(context.leg()) {
                    context.retry(RetryEvidence::DefinitiveRefusal, now)?;
                    writes.retry(previous, request)?;
                } else {
                    writes.register(request, context.leg())?;
                }
            } else {
                let mut writes = CommandWrites::new(CAPACITY)?;
                writes.register(request, context.leg())?;
                owner.writes = Some(writes);
            }
        }
        owner.pending = pending;
        Ok(())
    }

    pub(super) fn retire_attempt(&mut self, context: &Context) {
        if let Ok(owner) = self.command_mut(context) {
            owner.pending = owner
                .pending
                .checked_sub(1)
                .expect("registered physical attempt retired once");
        }
    }

    pub(super) fn enter_delivery(&mut self, delivery: &Delivery) {
        if let Ok(owner) = self.command_mut(&delivery.context) {
            owner.deliveries = owner
                .deliveries
                .checked_add(1)
                .expect("bounded engine delivery nesting");
        }
    }

    pub(super) fn exit_delivery(&mut self, delivery: &Delivery) {
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
        if context.operation() != Operation::Mutation {
            return Ok(None);
        }
        self.command_mut(context)?
            .writes
            .as_mut()
            .ok_or(Error::State)?
            .release_flag(request)
            .map(Some)
    }

    pub(super) fn result(&mut self, delivery: &Delivery, result: WriteResult) -> Result<(), Error> {
        if delivery.context.operation() != Operation::Mutation {
            return Err(Error::Invalid);
        }
        self.command_mut(&delivery.context)?
            .writes
            .as_mut()
            .ok_or(Error::State)?
            .result(delivery.attempt.request, result)
    }

    pub(super) fn seal(
        &mut self,
        command: CommandId,
        delivery: Option<&Delivery>,
        authority: Option<&Context>,
        error: Option<VenueError>,
    ) -> Seal {
        // Managed completion always has explicit generation-bearing authority.
        // Numeric calls remain legacy-only; a live journal/managed delivery
        // cannot be finalized by that bridge.
        if let Some(authority) = authority {
            if !authority.issued_by(&self.issuer)
                || !matches!(authority.owner(), Owner::Command { id, generation } if id == command
                    && self.commands.get(&id).is_some_and(|owner| owner.generation == generation))
            {
                return Seal::Stale;
            }
        } else if delivery.is_some()
            || self
                .commands
                .get(&command)
                .is_some_and(|owner| owner.mutation_declared)
        {
            return Seal::Stale;
        }
        if let Some(delivery) = delivery {
            let Owner::Command { id, generation } = delivery.context.owner() else {
                return Seal::Stale;
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
                return Seal::Stale;
            }
        }
        let declared = self
            .commands
            .get(&command)
            .is_some_and(|owner| owner.mutation_declared);
        let mut writes = match self
            .commands
            .get_mut(&command)
            .and_then(|owner| owner.writes.take())
        {
            Some(writes) => writes,
            None if declared => {
                CommandWrites::new(CAPACITY).expect("fixed positive journal capacity")
            }
            None => return Seal::Legacy(error),
        };
        if error.is_none() && writes.succeed().is_ok() {
            return Seal::Success;
        }
        let error = error
            .unwrap_or_else(|| VenueError::new("Command writes are not all confirmed accepted"));
        match writes.fail(error) {
            Ok(error) => Seal::Failure(
                error,
                writes
                    .rejections()
                    .map(|(_, error)| error.message.clone())
                    .collect(),
            ),
            Err(_) => Seal::Stale,
        }
    }

    pub(super) fn finish(&mut self, command: CommandId) {
        if let Some(owner) = self.commands.remove(&command) {
            self.contexts.retain(|_, context| {
                context.owner()
                    != Owner::Command {
                        id: command,
                        generation: owner.generation,
                    }
            });
        }
    }
}

impl<P> Cx<'_, P> {
    /// Capture the logical raw deadline and engine-owned owner/leg generation.
    /// This neither queues HTTP nor reserves admission, a transport or pacing.
    pub fn http_context(&mut self, scope: Scope, policy: Policy) -> Result<Context, Error> {
        let identity = self.io.next();
        self.io
            .request_contexts
            .issue(identity, scope, policy, self.now)
    }

    /// Validate context provenance and retry policy without extending its bound.
    /// This is not permission to resend a write: physical attempt registration
    /// must separately verify the prior attempt's definitive native facts.
    pub fn http_retry_context(
        &self,
        context: &Context,
        evidence: RetryEvidence,
    ) -> Result<Context, Error> {
        self.io.request_contexts.validate(context, self.now)?;
        context.retry(evidence, self.now)
    }

    /// Generation-bearing finalization for callbacks and logical command timers.
    /// Physical-attempt timers must retain/check their attempt token separately.
    pub fn done_context(&mut self, context: &Context, result: String) -> Result<(), Error> {
        let Owner::Command { id, .. } = context.owner() else {
            return Err(Error::Invalid);
        };
        self.finish_command(id, Ok(result), Some(context))
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
    pub fn http_write_result(
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

    /// Retire a fully completed logical read/page join or write leg. Do not call
    /// for each physical page: all remaining attempts must retain their owner.
    /// Command generation stays live until done/fail or its final bound expires.
    pub fn finish_http_context(&mut self, context: &Context) -> Result<(), Error> {
        self.io.request_contexts.validate(context, self.now)?;
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
            registry.issue(id, Scope::Background, policy(), 0.).unwrap();
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
                .issue(id, Scope::Command(id), policy(), 0.)
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
    fn delivery(context: &Context, id: u32) -> Delivery {
        Delivery {
            context: context.clone(),
            attempt: Attempt {
                request: RequestId(id),
                generation: u64::from(id),
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
            .issue(3, Scope::Background, policy(), 101.)
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
        registry.retire_attempt(&preflight);
        registry
            .issue(3, Scope::Background, policy(), 101.)
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
                .issue(id, Scope::Command(id), policy(), 0.)
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
