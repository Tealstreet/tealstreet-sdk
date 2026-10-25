//! Background-session ownership: listen keys, WS tokens and session logins.
//! Sessions are engine-issued owners beside commands. They join no command
//! journal, change no orders and hold no key or token value.
use super::super::{Action, Engine, TimerId, Venue};
use super::{ContextRegistry, Cx, RequestId, Scope};
use crate::request_context::{
    Context, Egress, Error, Operation, Owner, Policy, Session, SessionEgress, SessionPolicy,
    Waiter, WriteResult,
};
use crate::{ErrorKind, Outcome, VenueError};

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
    writes: crate::IdMap<u64, Write>,
    /// A released write had no proven outcome. Only retirement clears it.
    unresolved: bool,
}

#[derive(Default)]
pub(super) struct Sessions {
    live: crate::IdMap<u64, State>,
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
        // Signer iff credential: a signed session needs no unsigned egress.
        if policy.private_egress.is_some() && policy.credential.is_some() {
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
                writes: Default::default(),
                unresolved: false,
            },
        );
        Ok(())
    }

    pub(super) fn live(&self, generation: u64) -> bool {
        self.live.contains_key(&generation)
    }

    pub(super) fn egress(&self, generation: u64) -> Option<Egress> {
        self.live.get(&generation)?.policy.private_egress?;
        Some(Egress::Session(SessionEgress::new(generation)))
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
    pub(super) fn prune(&mut self, contexts: &crate::IdMap<u64, Context>) {
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

/// What a waiter resumes: a live owner Context, or a parked Plan Slot that
/// spends no attempt until its leg is issued at resume.
enum Unit {
    Owner(Context),
    #[cfg(feature = "http-plans")]
    Slot {
        plan: crate::request_context::Plan,
        slot: crate::request_context::plan::Slot,
        intent: Option<crate::classify::Intent>,
    },
    /// A failed Read call's declared retry, selected only at resume.
    #[cfg(feature = "http-plans")]
    Retry {
        plan: crate::request_context::Plan,
        slot: crate::request_context::plan::Slot,
        context: Context,
    },
    /// A retained Preflight's next read, issued only at resume. Its wait is
    /// bounded by the caller's receipt cap, if any.
    #[cfg(feature = "http-plans")]
    Preflight {
        handle: crate::request_context::Preflight,
        read: Box<(Policy, crate::request_context::plan::CallTerms)>,
        cap: f64,
    },
    /// A retained Preflight read's declared retry, selected only at resume.
    #[cfg(feature = "http-plans")]
    PreflightRetry {
        handle: crate::request_context::Preflight,
        context: Context,
        cap: f64,
    },
}

impl Unit {
    fn deadline(&self) -> f64 {
        match self {
            Unit::Owner(owner) => owner.deadline(),
            #[cfg(feature = "http-plans")]
            Unit::Slot { plan, .. } | Unit::Retry { plan, .. } => plan.deadline(),
            #[cfg(feature = "http-plans")]
            Unit::Preflight { cap, .. } | Unit::PreflightRetry { cap, .. } => *cap,
        }
    }

    #[cfg(feature = "http-plans")]
    fn plan(&self) -> Option<&crate::request_context::Plan> {
        match self {
            Unit::Slot { plan, .. } | Unit::Retry { plan, .. } => Some(plan),
            _ => None,
        }
    }

    #[cfg(feature = "http-plans")]
    fn preflight(&self) -> Option<&crate::request_context::Preflight> {
        match self {
            Unit::Preflight { handle, .. } | Unit::PreflightRetry { handle, .. } => Some(handle),
            _ => None,
        }
    }

    /// Plan and Preflight units are held for a disposition after the callback.
    #[cfg(feature = "http-plans")]
    fn held(&self) -> bool {
        !matches!(self, Unit::Owner(_))
    }
}

struct Wait<P> {
    session: u64,
    unit: Unit,
    /// An owner waits on its own deadline timer. A parked Slot has none: its
    /// Plan's root expiry seals first, and the sealed sweep returns it.
    timer: Option<TimerId>,
    pending: P,
}

/// Continuations parked behind a session. Each holds a live owner Context or
/// a reserved Plan Slot, so the registry's 256 live contexts bound this list.
pub(in crate::engine) struct Waits<P> {
    entries: Vec<Wait<P>>,
    /// Parked Slots whose Session retired or failed, delivered after the
    /// callback in that Plan's ready custody. Storage is reserved at park.
    #[cfg(feature = "http-plans")]
    ended: Vec<(
        crate::request_context::Plan,
        crate::request_context::plan::Slot,
        P,
        VenueError,
    )>,
    /// Parked Preflight reads whose Session ended, delivered after the callback
    /// in that Preflight's ready custody. Storage is reserved at park.
    #[cfg(feature = "http-plans")]
    preflight_ended: Vec<(crate::request_context::Preflight, P, VenueError)>,
}

impl<P> Default for Waits<P> {
    fn default() -> Self {
        Self {
            entries: vec![],
            #[cfg(feature = "http-plans")]
            ended: vec![],
            #[cfg(feature = "http-plans")]
            preflight_ended: vec![],
        }
    }
}

#[cfg(feature = "http-plans")]
impl<P> Waits<P> {
    pub(in crate::engine) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(in crate::engine) fn parked_units(&self) -> usize {
        self.entries.iter().filter(|wait| wait.unit.held()).count()
    }

    pub(in crate::engine) fn reserve_ended(&mut self) -> Result<(), Error> {
        let parked = self.parked_units();
        // Additional to the current length: every parked unit, plus this one.
        self.ended
            .try_reserve(parked + 1)
            .map_err(|_| Error::Capacity)?;
        self.preflight_ended
            .try_reserve(parked + 1)
            .map_err(|_| Error::Capacity)
    }

    pub(in crate::engine) fn park_slot(
        &mut self,
        session: u64,
        plan: crate::request_context::Plan,
        slot: crate::request_context::plan::Slot,
        intent: Option<crate::classify::Intent>,
        pending: P,
    ) {
        self.entries.push(Wait {
            session,
            unit: Unit::Slot { plan, slot, intent },
            timer: None,
            pending,
        });
    }

    pub(in crate::engine) fn park_retry(
        &mut self,
        session: u64,
        plan: crate::request_context::Plan,
        slot: crate::request_context::plan::Slot,
        context: Context,
        pending: P,
    ) {
        self.entries.push(Wait {
            session,
            unit: Unit::Retry {
                plan,
                slot,
                context,
            },
            timer: None,
            pending,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::engine) fn park_preflight_read(
        &mut self,
        session: u64,
        handle: crate::request_context::Preflight,
        read: (Policy, crate::request_context::plan::CallTerms),
        cap: f64,
        timer: Option<TimerId>,
        pending: P,
    ) {
        self.entries.push(Wait {
            session,
            unit: Unit::Preflight {
                handle,
                read: Box::new(read),
                cap,
            },
            timer,
            pending,
        });
    }

    pub(in crate::engine) fn park_preflight_retry(
        &mut self,
        session: u64,
        handle: crate::request_context::Preflight,
        context: Context,
        cap: f64,
        timer: Option<TimerId>,
        pending: P,
    ) {
        self.entries.push(Wait {
            session,
            unit: Unit::PreflightRetry {
                handle,
                context,
                cap,
            },
            timer,
            pending,
        });
    }

    #[cfg(test)]
    pub(in crate::engine) fn ended_capacity(&self) -> usize {
        self.ended.capacity()
    }

    fn has_slot_work(&self) -> bool {
        !self.ended.is_empty()
            || !self.preflight_ended.is_empty()
            || self.entries.iter().any(|wait| wait.unit.held())
    }
}

fn not_sent(error: VenueError) -> VenueError {
    error.outcome(Outcome::NotSent)
}

fn owner_error(reason: Error) -> VenueError {
    match reason {
        Error::TimedOut => VenueError::new("Session wait timed out").kind(ErrorKind::Timeout),
        _ => VenueError::new("Session waiter owner is no longer active")
            .kind(ErrorKind::InvalidInput),
    }
    .outcome(Outcome::NotSent)
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
    #[cfg(feature = "session-bearer")]
    pub(in crate::engine) fn session_generation_live(&self, generation: u64) -> bool {
        self.sessions.live(generation)
    }

    /// Issuer plus liveness only; binding a Bearer takes no session scope.
    pub(in crate::engine) fn validate_session(&self, session: &Session) -> Result<u64, Error> {
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
    /// Session lifetimes belong to session, socket and timer callbacks. A
    /// retained Preflight's or a Plan's readiness scope may not mint, retire
    /// or drain them.
    pub(super) fn session_scope(&self) -> Result<(), Error> {
        #[cfg(feature = "http-plans")]
        if self.io.core.http_preflight_ready.is_some() || self.io.core.http_plan_ready.is_some() {
            return Err(Error::State);
        }
        Ok(())
    }

    /// Issue a session owner. It reserves no HTTP, timer or socket and has no
    /// deadline; each operation captures its own bound in `session_context`.
    pub fn open_session(&mut self, policy: SessionPolicy) -> Result<Session, Error> {
        self.session_scope()?;
        let generation = self.io.reserve_ids(1).ok_or(Error::Capacity)?.start;
        self.io
            .core
            .request_contexts
            .sessions
            .open(generation, policy)?;
        Ok(Session::new(
            &self.io.core.request_contexts.issuer,
            generation,
        ))
    }

    /// A logical session operation with its own original raw bound. Mutations
    /// on a session that is not repeat-safe wait for every earlier write to be
    /// proven; an unresolved write blocks them until the session is retired.
    /// Record a write's codec fact before `finish_http_context`: finishing
    /// first discards the attempt, and its outcome stays unknown.
    pub fn session_context(&mut self, session: &Session, policy: Policy) -> Result<Context, Error> {
        self.session_scope()?;
        let generation = self.io.core.request_contexts.validate_session(session)?;
        let identity = self.io.reserve_ids(1).ok_or(Error::Capacity)?.start;
        self.io
            .core
            .request_contexts
            .issue_session(identity, generation, policy, self.now, None)
    }

    /// A new physical leg of a session operation, keeping its exact raw bound.
    /// A Read leg can never become a Mutation; any other owner is refused.
    pub fn session_leg_context(
        &mut self,
        parent: &Context,
        policy: Policy,
    ) -> Result<Context, Error> {
        self.io.core.request_contexts.validate_legacy(parent)?;
        self.io
            .core
            .request_contexts
            .validate_send(parent, self.now)?;
        let Owner::Session { generation } = parent.owner() else {
            return Err(Error::Invalid);
        };
        self.session_scope()?;
        let identity = self.io.reserve_ids(1).ok_or(Error::Capacity)?.start;
        self.io.core.request_contexts.issue_session(
            identity,
            generation,
            policy,
            self.now,
            Some(parent),
        )
    }

    /// Work owned by a command or the background that carries this session's
    /// login or token: it rides the session's private egress, and retirement
    /// fences its sends. Routing only: the scope still decides write authority.
    pub fn session_scoped_context(
        &mut self,
        session: &Session,
        scope: Scope,
        policy: Policy,
    ) -> Result<Context, Error> {
        self.session_scope()?;
        let generation = self.io.core.request_contexts.validate_session(session)?;
        let egress = self
            .io
            .core
            .request_contexts
            .sessions
            .egress(generation)
            .ok_or(Error::Invalid)?;
        self.make_http_context(scope, policy, None, Some(egress))
    }

    /// Discard the session's work and revoke it, with no venue callback in
    /// between: unreleased attempts emit no HTTP, and late responses find no
    /// managed entry, so they never reach venue callbacks or reducers. Its
    /// waiters come back to the caller as NotSent failures. Unreleased packets
    /// other owners bound to its Bearer fail after the current callback.
    #[must_use = "returned waiters must be failed or re-dispatched"]
    pub fn retire_session(
        &mut self,
        session: &Session,
    ) -> Result<Vec<(Context, P, VenueError)>, Error> {
        self.session_scope()?;
        let generation = self.io.core.request_contexts.validate_session(session)?;
        #[cfg(feature = "session-bearer")]
        if self
            .io
            .core
            .bearer_fenced
            .try_reserve(self.io.bearer_bound_len(generation))
            .is_err()
        {
            return Err(Error::Capacity);
        }
        let waiters = self.drain_waits(generation, VenueError::new("Session retired"));
        self.io.retire_http_owner(session.owner());
        let registry = &mut self.io.core.request_contexts;
        registry.sessions.retire(generation);
        registry
            .contexts
            .retain(|_, context| context.owner() != session.owner());
        #[cfg(feature = "session-bearer")]
        self.io.fence_bearer_generation(generation);
        Ok(waiters)
    }

    /// Park a live owner's continuation until the venue drains the session.
    /// One engine timer at the owner's existing deadline; no HTTP is queued.
    pub fn session_wait(
        &mut self,
        session: &Session,
        owner: &Context,
        pending: P,
    ) -> Result<(), (P, Error)> {
        let checked = (|| {
            self.session_scope()?;
            let generation = self.io.core.request_contexts.validate_session(session)?;
            self.io.core.request_contexts.validate(owner, self.now)?;
            // An owner waiting on its own session could never become ready.
            if matches!(owner.owner(), Owner::Session { .. }) {
                return Err(Error::Invalid);
            }
            // An execution observer re-dispatches only through its execution.
            if matches!(owner.owner(), Owner::Command { id, .. }
                if self.io.core.request_contexts.is_observer(id))
            {
                return Err(Error::Revoked);
            }
            if self
                .io
                .core
                .request_contexts
                .resend_blocked(owner, self.now)
            {
                return Err(Error::State);
            }
            // Planned and preflight commands sequence their own legs.
            if !self.io.core.request_contexts.context_phase(owner) {
                return Err(Error::State);
            }
            if self.io.session_waits.entries.iter().any(
                |wait| matches!(&wait.unit, Unit::Owner(parked) if parked.leg() == owner.leg()),
            ) {
                return Err(Error::Duplicate);
            }
            if self.io.session_waits.entries.len() >= super::CAPACITY {
                return Err(Error::Capacity);
            }
            Ok(generation)
        })();
        let generation = match checked {
            Ok(generation) => generation,
            Err(error) => return Err((pending, error)),
        };
        let Some(ids) = self.io.reserve_ids(1) else {
            return Err((pending, Error::Capacity));
        };
        let timer = TimerId(ids.start);
        self.io.core.timers.insert(timer, ());
        self.io.core.actions.push(Action::SetTimer {
            timer,
            at_ms: owner.deadline(),
        });
        self.io.core.request_contexts.hold_wait(owner);
        self.io.session_waits.entries.push(Wait {
            session: generation,
            unit: Unit::Owner(owner.clone()),
            timer: Some(timer),
            pending,
        });
        Ok(())
    }

    /// Drain this session's waiters in arrival order. Live owners are Ready;
    /// expired or revoked owners come back Failed so venue bookkeeping in
    /// their continuation is never stranded. A foreign handle drains nothing.
    #[must_use = "returned waiters must be failed or re-dispatched"]
    pub fn session_ready(&mut self, session: &Session) -> Vec<Waiter<P>> {
        if self.session_scope().is_err() {
            return vec![];
        }
        let Ok(generation) = self.io.core.request_contexts.validate_session(session) else {
            return vec![];
        };
        // Also announces the bundle: parked Bind calls re-check after the
        // callback. Without room to record it they stay parked to their window.
        #[cfg(feature = "session-bearer")]
        if !self.io.core.bearer_woken.contains(&generation)
            && self.io.core.bearer_woken.try_reserve(1).is_ok()
        {
            self.io.core.bearer_woken.push(generation);
        }
        // A Ready owner is live by its own deadline; a Failed one keeps its
        // hold until the venue fails the command, which removes the owner.
        // A parked Slot is issued now, with this drain's delivery as its
        // predecessor; if it cannot be, it ends after the callback.
        let mut waiters = Vec::new();
        for wait in self.take_waits(generation) {
            match wait.unit {
                Unit::Owner(owner) => {
                    match self.io.core.request_contexts.validate(&owner, self.now) {
                        Ok(()) => {
                            self.io.core.request_contexts.release_wait(&owner);
                            waiters.push(Waiter::Ready(owner, wait.pending));
                        }
                        Err(reason) => {
                            waiters.push(Waiter::Failed(owner, wait.pending, owner_error(reason)));
                        }
                    }
                }
                #[cfg(feature = "http-plans")]
                Unit::Slot { plan, slot, intent } => {
                    match self.resume_plan_slot(&plan, &slot, intent) {
                        Ok(leg) => waiters.push(Waiter::Ready(leg, wait.pending)),
                        // The Slot stays reserved until its ended disposition,
                        // so no ordinary readiness can issue it first.
                        Err(reason) => self.io.session_waits.ended.push((
                            plan,
                            slot,
                            wait.pending,
                            owner_error(reason),
                        )),
                    }
                }
                #[cfg(feature = "http-plans")]
                Unit::Retry {
                    plan,
                    slot,
                    context,
                } => match self.resume_plan_retry(&plan, &slot, &context) {
                    Ok(leg) => waiters.push(Waiter::Ready(leg, wait.pending)),
                    Err(reason) => self.io.session_waits.ended.push((
                        plan,
                        slot,
                        wait.pending,
                        owner_error(reason),
                    )),
                },
                #[cfg(feature = "http-plans")]
                Unit::Preflight { handle, read, .. } => {
                    let (policy, terms) = *read;
                    match self.resume_preflight_read(&handle, policy, terms) {
                        Ok(read) => waiters.push(Waiter::Ready(read, wait.pending)),
                        Err(reason) => self.io.session_waits.preflight_ended.push((
                            handle,
                            wait.pending,
                            owner_error(reason),
                        )),
                    }
                }
                #[cfg(feature = "http-plans")]
                Unit::PreflightRetry {
                    handle, context, ..
                } => match self.resume_preflight_retry(&handle, &context) {
                    Ok(read) => waiters.push(Waiter::Ready(read, wait.pending)),
                    Err(reason) => self.io.session_waits.preflight_ended.push((
                        handle,
                        wait.pending,
                        owner_error(reason),
                    )),
                },
            }
        }
        waiters
    }

    /// The prerequisite failed: every waiter comes back NotSent with `error`.
    #[must_use = "returned waiters must be failed"]
    pub fn session_failed(
        &mut self,
        session: &Session,
        error: VenueError,
    ) -> Vec<(Context, P, VenueError)> {
        if self.session_scope().is_err() {
            return vec![];
        }
        match self.io.core.request_contexts.validate_session(session) {
            Ok(generation) => self.drain_waits(generation, error),
            Err(_) => vec![],
        }
    }

    fn drain_waits(&mut self, generation: u64, error: VenueError) -> Vec<(Context, P, VenueError)> {
        let error = not_sent(error);
        let mut owners = Vec::new();
        for wait in self.take_waits(generation) {
            match wait.unit {
                Unit::Owner(owner) => owners.push((owner, wait.pending, error.clone())),
                // A parked Slot ends after this callback, in its Plan's ready
                // custody, where the venue can re-park it or fail the Plan.
                #[cfg(feature = "http-plans")]
                Unit::Slot { plan, slot, .. } | Unit::Retry { plan, slot, .. } => {
                    self.io
                        .session_waits
                        .ended
                        .push((plan, slot, wait.pending, error.clone()));
                }
                // A parked Preflight read ends the same way, in its own
                // Preflight-ready custody.
                #[cfg(feature = "http-plans")]
                Unit::Preflight { handle, .. } | Unit::PreflightRetry { handle, .. } => {
                    self.io.session_waits.preflight_ended.push((
                        handle,
                        wait.pending,
                        error.clone(),
                    ));
                }
            }
        }
        owners
    }

    fn take_waits(&mut self, generation: u64) -> Vec<Wait<P>> {
        let (taken, kept) = std::mem::take(&mut self.io.session_waits.entries)
            .into_iter()
            .partition::<Vec<_>, _>(|wait| wait.session == generation);
        self.io.session_waits.entries = kept;
        for timer in taken.iter().filter_map(|wait| wait.timer) {
            self.io.core.timers.remove(&timer);
            self.io.core.actions.push(Action::ClearTimer { timer });
        }
        taken
    }

    pub fn session_live(&self, session: &Session) -> bool {
        self.io
            .core
            .request_contexts
            .validate_session(session)
            .is_ok()
    }
}

impl crate::engine::CoreCx<'_> {}

impl<V: Venue> Engine<V> {
    /// A waiter's owner deadline passed before the venue drained it. Early
    /// timer delivery re-arms at the unchanged deadline.
    pub(in crate::engine) fn expire_session_wait(&mut self, now: f64, timer: TimerId) -> bool {
        let entries = &mut self.io.session_waits.entries;
        let Some(index) = entries.iter().position(|wait| wait.timer == Some(timer)) else {
            return false;
        };
        if now.is_finite() && now < entries[index].unit.deadline() {
            let at_ms = entries[index].unit.deadline();
            self.io.core.actions.push(Action::SetTimer { timer, at_ms });
            return true;
        }
        let wait = entries.remove(index);
        self.io.core.timers.remove(&timer);
        self.begin_event_scope();
        // A parked Preflight read reached its caller's receipt cap: the
        // Preflight's terminal comes first, then the pending returns once.
        #[cfg(feature = "http-plans")]
        if let Some(handle) = wait.unit.preflight() {
            Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            }
            .expire_parked_preflight(handle);
        }
        self.venue.on_session_wait_expired(
            &mut Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            },
            wait.pending,
            owner_error(Error::TimedOut),
        );
        self.finish_event_scope(now);
        true
    }
}

#[cfg(feature = "http-plans")]
impl<V: Venue> Engine<V> {
    /// After the callback: a parked Slot whose Plan sealed returns its pending
    /// once with no custody (Timeout if the root expired); one whose Session
    /// ended runs the ended hook in its Plan's ready custody. Callbacks here may
    /// queue more, so this runs until both queues are empty, in place, keeping
    /// the storage reserved at park.
    pub(in crate::engine) fn flush_plan_session_waits(&mut self, now: f64) {
        loop {
            if !self.io.session_waits.has_slot_work()
                || !now.is_finite()
                || self.io.core.http_delivery.is_some()
                || self.io.core.http_plan_ready.is_some()
                || self.io.core.http_plan_expiring.is_some()
                || self.io.core.http_preflight_ready.is_some()
                || !self.io.core.request_contexts.plan_scope_clear()
            {
                return;
            }
            let registry = &self.io.core.request_contexts;
            let sealed = self.io.session_waits.entries.iter().position(|wait| {
                wait.unit
                    .plan()
                    .is_some_and(|plan| !registry.plan_registered(plan))
                    || wait
                        .unit
                        .preflight()
                        .is_some_and(|handle| !registry.preflight_registered(handle))
            });
            if let Some(index) = sealed {
                let wait = self.io.session_waits.entries.remove(index);
                if let Some(timer) = wait.timer {
                    self.io.core.timers.remove(&timer);
                    self.io.core.actions.push(Action::ClearTimer { timer });
                }
                let reason = if now >= wait.unit.deadline() {
                    Error::TimedOut
                } else {
                    Error::Revoked
                };
                self.venue.on_session_wait_expired(
                    &mut Cx {
                        now,
                        store: &mut self.store,
                        io: &mut self.io,
                    },
                    wait.pending,
                    owner_error(reason),
                );
            } else if !self.io.session_waits.preflight_ended.is_empty() {
                let (handle, pending, error) = self.io.session_waits.preflight_ended.remove(0);
                self.end_preflight_session_wait(now, handle, pending, error);
            } else if self.io.session_waits.ended.is_empty() {
                return;
            } else {
                let (plan, slot, pending, error) = self.io.session_waits.ended.remove(0);
                // The Slot was held until now; it is released only for this
                // disposition, where the venue re-parks it or fails the Plan.
                self.io.core.request_contexts.unpark_plan_slot(&plan, &slot);
                if !self.io.core.request_contexts.plan_registered(&plan) || now >= plan.deadline() {
                    self.venue.on_session_wait_expired(
                        &mut Cx {
                            now,
                            store: &mut self.store,
                            io: &mut self.io,
                        },
                        pending,
                        error,
                    );
                } else {
                    self.io.core.http_plan_ready = Some(plan.clone());
                    self.io.core.http_plan_poison = true;
                    #[cfg(feature = "order-locks")]
                    let order_response = self.io.core.order_response_token.take();
                    self.venue.on_plan_session_wait_ended(
                        &mut Cx {
                            now,
                            store: &mut self.store,
                            io: &mut self.io,
                        },
                        &plan,
                        pending,
                        error,
                    );
                    #[cfg(feature = "order-locks")]
                    {
                        self.io.core.order_response_token = order_response;
                    }
                    self.io.core.http_plan_ready = None;
                    self.io.core.http_plan_poison = false;
                    ContextRegistry::rearm_live_plan(&mut self.io, &plan);
                }
            }
            self.assert_live();
            self.flush_http_owner_retirements();
            // An expired-wait callback has no ready custody and may retire a
            // Session; its fence drains before the next pass can return.
            self.flush_bearer_fences(now);
        }
    }
}

#[cfg(test)]
#[path = "session_context_tests.rs"]
mod tests;
