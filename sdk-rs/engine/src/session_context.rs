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
    /// Released without a proven outcome. Terminal: no later fact clears it.
    Unknown,
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
        if policy.operation == Operation::Mutation && state.blocks_writes() {
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
        // Rechecked here: a context issued earlier must not bypass the gate.
        if state.blocks_writes() {
            return Err(Error::State);
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

    /// One fact per attempt, as in the command journal. Accepted and
    /// definitive refusal resolve a released write; an engine or codec failure
    /// makes it terminally unknown. Unreleased writes take only failure.
    pub(super) fn result(
        &mut self,
        context: &Context,
        request: RequestId,
        result: &WriteResult,
    ) -> Result<(), Error> {
        let state = self.state(context)?;
        let write = state.writes.get_mut(&context.leg()).ok_or(Error::State)?;
        let next = match (*write, result) {
            (_, WriteResult::KnownPartial(_)) => return Err(Error::Invalid),
            (Write::Registered(id), WriteResult::Failed(_)) if id == request => return Ok(()),
            (Write::Released(id), WriteResult::Accepted { .. } | WriteResult::Rejected(_))
                if id == request =>
            {
                Write::Resolved
            }
            (Write::Released(id), WriteResult::Failed(_)) if id == request => {
                state.unresolved = true;
                Write::Unknown
            }
            (Write::Registered(id) | Write::Released(id), _) if id != request => {
                return Err(Error::Revoked);
            }
            _ => return Err(Error::State),
        };
        *write = next;
        Ok(())
    }

    /// Physical attempt settled or was discarded. An unproven released write
    /// marks the session unresolved; an unreleased one leaves no trace.
    pub(super) fn retire_attempt(&mut self, context: &Context, request: RequestId) {
        let Ok(state) = self.state(context) else {
            return;
        };
        match state.writes.get(&context.leg()) {
            Some(Write::Released(id)) if *id == request => {
                state.unresolved = true;
                state.writes.insert(context.leg(), Write::Unknown);
            }
            Some(Write::Registered(id)) if *id == request => {
                state.writes.insert(context.leg(), Write::Resolved);
            }
            _ => {}
        }
    }

    /// Bound per-operation records by live logical handles. In-flight attempts
    /// keep theirs until the physical attempt settles.
    pub(super) fn prune(&mut self, contexts: &HashMap<u64, Context>) {
        for state in self.live.values_mut() {
            state.writes.retain(|leg, write| {
                contexts.contains_key(leg)
                    || matches!(write, Write::Registered(_) | Write::Released(_))
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

impl State {
    /// A session that is not repeat-safe allows one write at a time, and none
    /// after an unknown outcome until it is retired.
    fn blocks_writes(&self) -> bool {
        !self.policy.repeat_safe
            && (self.unresolved
                || self
                    .writes
                    .values()
                    .any(|write| matches!(write, Write::Registered(_) | Write::Released(_))))
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
    /// Record a write's codec fact before `finish_http_context`: finishing
    /// first discards the attempt, and its outcome stays unknown.
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
