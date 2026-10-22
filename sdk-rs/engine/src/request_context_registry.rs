//! Engine-issued logical context identities. Physical attempts and transport
//! resources are registered only after this immutable logical bound is valid.
use super::{CommandId, Cx};
use crate::request_context::{Context, Error, Owner, Policy, RetryEvidence, Scope};
use std::collections::HashMap;

const CAPACITY: usize = 256;

struct CommandOwner {
    generation: u64,
    /// Owner retention only. This never extends any issued logical deadline.
    last_deadline: f64,
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
        self.commands.retain(|_, old| old.last_deadline > now);
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
            self.commands.insert(
                id,
                CommandOwner {
                    generation,
                    last_deadline,
                },
            );
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
