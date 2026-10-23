//! Background-session ownership: listen keys, WS tokens and session logins.
//! Sessions are engine-issued owners beside commands. They join no command
//! journal, change no orders and hold no key or token value.
use super::{ContextRegistry, Cx, RequestId};
use crate::request_context::{
    Context, Error, Operation, Owner, Policy, Session, SessionPolicy, WriteResult,
};
use std::collections::HashMap;

const SESSIONS: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Write {
    Registered(RequestId),
    Released(RequestId),
    Resolved,
}

struct State {
    policy: SessionPolicy,
    /// Mutation legs by logical identity: one physical attempt per operation.
    writes: HashMap<u64, Write>,
    /// A released write had no proven outcome. Only retirement clears it.
    unresolved: bool,
}

#[derive(Default)]
pub(super) struct Sessions {
    live: HashMap<u64, State>,
}

impl Sessions {
    pub(super) fn open(&mut self, generation: u64, policy: SessionPolicy) -> Result<(), Error> {
        if policy
            .credential
            .as_ref()
            .is_some_and(|c| c.reference.0.is_empty() || c.reference.0.len() > 256)
        {
            return Err(Error::Invalid);
        }
        if self.live.len() >= SESSIONS {
            return Err(Error::Capacity);
        }
        if self.live.contains_key(&generation) {
            return Err(Error::Duplicate);
        }
        self.live.insert(
            generation,
            State {
                policy,
                writes: HashMap::new(),
                unresolved: false,
            },
        );
        Ok(())
    }

    pub(super) fn live(&self, generation: u64) -> bool {
        self.live.contains_key(&generation)
    }

    pub(super) fn retire(&mut self, generation: u64) -> bool {
        self.live.remove(&generation).is_some()
    }

    /// Validate before any registry commit; this reserves nothing.
    pub(super) fn admit(&self, generation: u64, policy: &Policy) -> Result<(), Error> {
        let state = self.live.get(&generation).ok_or(Error::Revoked)?;
        if policy.credential != state.policy.credential {
            return Err(Error::Invalid);
        }
        if policy.operation == Operation::Mutation
            && !state.policy.repeat_safe
            && (state.unresolved
                || state
                    .writes
                    .values()
                    .any(|write| matches!(write, Write::Released(_))))
        {
            return Err(Error::State);
        }
        Ok(())
    }

    pub(super) fn register(&mut self, context: &Context, request: RequestId) -> Result<(), Error> {
        let state = self.state(context)?;
        if context.operation() != Operation::Mutation {
            return Ok(());
        }
        if state.writes.contains_key(&context.leg()) {
            return Err(Error::RetryForbidden);
        }
        state
            .writes
            .insert(context.leg(), Write::Registered(request));
        Ok(())
    }

    pub(super) fn released(&mut self, context: &Context, request: RequestId) {
        if let Ok(state) = self.state(context)
            && let Some(write) = state.writes.get_mut(&context.leg())
            && *write == Write::Registered(request)
        {
            *write = Write::Released(request);
        }
    }

    /// Native codec facts for the delivered attempt. Accepted and definitive
    /// refusal resolve the operation; a failure leaves it unresolved.
    pub(super) fn result(
        &mut self,
        context: &Context,
        request: RequestId,
        result: &WriteResult,
    ) -> Result<(), Error> {
        let state = self.state(context)?;
        let write = state.writes.get_mut(&context.leg()).ok_or(Error::State)?;
        if !matches!(*write, Write::Released(id) | Write::Registered(id) if id == request) {
            return Err(Error::Revoked);
        }
        match result {
            WriteResult::Accepted { .. } | WriteResult::Rejected(_) => {
                if matches!(write, Write::Released(_)) {
                    *write = Write::Resolved;
                }
                Ok(())
            }
            WriteResult::Failed(_) => Ok(()),
            WriteResult::KnownPartial(_) => Err(Error::Invalid),
        }
    }

    /// Physical attempt settled or was discarded. An unproven released write
    /// marks the session unresolved; an unreleased one leaves no trace.
    pub(super) fn retire_attempt(&mut self, context: &Context, request: RequestId) {
        let Ok(state) = self.state(context) else {
            return;
        };
        match state.writes.get(&context.leg()) {
            Some(Write::Released(id)) if *id == request => state.unresolved = true,
            Some(Write::Registered(id)) if *id == request => {}
            _ => return,
        }
        state.writes.insert(context.leg(), Write::Resolved);
    }

    /// Bound per-operation records by live logical handles. In-flight attempts
    /// keep theirs until the physical attempt settles.
    pub(super) fn prune(&mut self, contexts: &HashMap<u64, Context>) {
        for state in self.live.values_mut() {
            state.writes.retain(|leg, write| {
                contexts.contains_key(leg) || !matches!(write, Write::Resolved)
            });
        }
    }

    fn state(&mut self, context: &Context) -> Result<&mut State, Error> {
        let Owner::Session { generation } = context.owner() else {
            return Err(Error::Invalid);
        };
        self.live.get_mut(&generation).ok_or(Error::Revoked)
    }
}

impl ContextRegistry {
    fn validate_session(&self, session: &Session) -> Result<u64, Error> {
        let Owner::Session { generation } = session.owner() else {
            unreachable!("session handles always carry a session owner");
        };
        if !session.issued_by(&self.issuer) || !self.sessions.live(generation) {
            return Err(Error::Revoked);
        }
        Ok(generation)
    }
}

impl<P> Cx<'_, P> {
    /// Issue a session owner. It reserves no HTTP, timer or socket and has no
    /// deadline; each operation captures its own bound in `session_context`.
    pub fn open_session(&mut self, policy: SessionPolicy) -> Result<Session, Error> {
        let generation = self.io.reserve_ids(1).ok_or(Error::Capacity)?.start;
        self.io.request_contexts.sessions.open(generation, policy)?;
        Ok(Session::new(&self.io.request_contexts.issuer, generation))
    }

    /// A logical session operation with its own original raw bound. Mutations
    /// on a session that is not repeat-safe wait for every earlier write to be
    /// proven; an unresolved write blocks them until the session is retired.
    pub fn session_context(&mut self, session: &Session, policy: Policy) -> Result<Context, Error> {
        let generation = self.io.request_contexts.validate_session(session)?;
        let identity = self.io.reserve_ids(1).ok_or(Error::Capacity)?.start;
        self.io
            .request_contexts
            .issue_session(identity, generation, policy, self.now, None)
    }

    /// Discard the session's work and revoke it, with no venue callback in
    /// between: unreleased attempts emit no HTTP, and late responses find no
    /// managed entry, so they never reach venue callbacks or reducers.
    pub fn retire_session(&mut self, session: &Session) -> Result<(), Error> {
        let generation = self.io.request_contexts.validate_session(session)?;
        self.io.retire_http_owner(session.owner());
        let registry = &mut self.io.request_contexts;
        registry.sessions.retire(generation);
        registry
            .contexts
            .retain(|_, context| context.owner() != session.owner());
        Ok(())
    }

    pub fn session_live(&self, session: &Session) -> bool {
        self.io.request_contexts.validate_session(session).is_ok()
    }
}

#[cfg(test)]
#[path = "session_context_tests.rs"]
mod tests;
