//! Managed HTTP preparation and the engine's synchronous final claim.
use super::*;
use crate::request_context::{
    Attempt, Claim, Context, Error, HttpAction, HttpRelease, Lifecycle, Operation, Owner, Permit,
    Preparation, Prepared, ReleaseTime, ResponseProof, RoutingIntent, Session, SessionBearer,
    SessionHeaders,
};

#[cfg(test)]
#[path = "http_unsigned_headers_tests.rs"]
mod unsigned_headers_tests;

#[cfg(feature = "http-mutation-barriers")]
#[path = "http_group.rs"]
mod mutation_group;

enum ManagedRecipe {
    Unsigned(&'static [(&'static str, &'static str)]),
    Signed(Box<dyn crate::SignStep>),
}

pub(super) struct Managed {
    lifecycle: Lifecycle,
    original: HttpRequest,
    pub(super) timer: TimerId,
    response_started: bool,
    signing_attempt: Option<RequestId>,
    recipe: Option<crate::signing_dispatch::Pending>,
    /// Decided at issue from the binding; reprepare never re-derives it.
    intent: RoutingIntent,
    bearer: Option<Bearer>,
    #[cfg(feature = "http-mutation-barriers")]
    group: Option<Box<mutation_group::Member>>,
    waiting_route: Option<Prepared>,
}

/// A Session-bound packet's generation and the revision its header carries.
/// The engine keeps no token: only the Bind packet holds a copy.
#[derive(Clone, Copy)]
struct Bearer {
    generation: u64,
    revision: Option<u64>,
}

impl Bearer {
    /// Claim-time fence: no token copy, clone or allocation.
    fn current<V: Venue>(
        &self,
        venue: &V,
        registry: &request_context_registry::ContextRegistry,
    ) -> Result<Option<u64>, Error> {
        if !registry.session_generation_live(self.generation) {
            return Err(Error::Revoked);
        }
        Ok(venue.session_bearer_revision(self.generation))
    }

    /// The Bind-site read: the one copy of the token, recorded by revision.
    fn read<V: Venue>(
        &mut self,
        venue: &V,
        registry: &request_context_registry::ContextRegistry,
    ) -> Result<SessionHeaders, Error> {
        if !registry.session_generation_live(self.generation) {
            return Err(Error::Revoked);
        }
        match venue.session_bearer(self.generation) {
            SessionBearer::Ready { headers, revision } => {
                self.revision = Some(revision);
                Ok(headers)
            }
            SessionBearer::NotReady => Err(Error::State),
        }
    }
}

fn bearer_refused(reason: Error) -> VenueError {
    VenueError::new(match reason {
        Error::Revoked => "Session retired",
        Error::State => "Session bearer is not ready",
        _ => "Session bearer header is refused",
    })
}

/// Insert the Bind packet's one copy of each session header, then re-check the
/// limits over the whole packet (recipe, signer and session headers). A name
/// the packet already carries or an overflow is refused, inserted copies wiped.
fn attach_session_headers(
    headers: &mut Vec<(&'static str, String)>,
    signer_headers: &[(String, String)],
    session: &SessionHeaders,
) -> Result<(), Error> {
    let taken = |name: &str| {
        headers
            .iter()
            .any(|(owned, _)| owned.eq_ignore_ascii_case(name))
            || signer_headers
                .iter()
                .any(|(owned, _)| owned.eq_ignore_ascii_case(name))
    };
    if session.entries().iter().any(|(name, _)| taken(name)) {
        return Err(Error::Invalid);
    }
    let inserted = session.entries().len();
    for (index, (name, value)) in session.entries().iter().enumerate() {
        headers.insert(index, (*name, value.expose().to_owned()));
    }
    if crate::signing_headers::validate_packet(headers, signer_headers).is_err() {
        for (_, mut value) in headers.drain(..inserted) {
            zeroize::Zeroize::zeroize(&mut value);
        }
        return Err(Error::Invalid);
    }
    Ok(())
}

fn is_authorization(name: &str) -> bool {
    name.eq_ignore_ascii_case("Authorization")
}

impl Managed {
    fn validate(
        &self,
        registry: &request_context_registry::ContextRegistry,
        now: f64,
    ) -> Result<(), Error> {
        if !now.is_finite() {
            return Err(Error::Invalid);
        }
        if now >= self.lifecycle.deadline() {
            return Err(Error::TimedOut);
        }
        registry.validate(self.lifecycle.context(), now)
    }

    /// `validate` plus the session-egress retirement fence, for physical
    /// send steps only; responses and facts keep plain `validate`.
    fn validate_send(
        &self,
        registry: &request_context_registry::ContextRegistry,
        now: f64,
    ) -> Result<(), Error> {
        self.validate(registry, now)?;
        registry.egress_live(self.lifecycle.context())
    }

    pub(super) fn delivery(&self) -> request_context_registry::Delivery {
        request_context_registry::Delivery {
            context: self.lifecycle.context().clone(),
            attempt: self.lifecycle.attempt(),
        }
    }

    #[cfg(feature = "http-plans")]
    pub(super) fn released_delivery(&self, delivery: &request_context_registry::Delivery) -> bool {
        self.delivery() == *delivery
            && self.lifecycle.release().is_some()
            && self.lifecycle.accepts_response()
    }
}

impl<P> Cx<'_, P> {
    /// Queue semantic, unsigned HTTP work. Private recipes use the signed
    /// entry point; a missing signing recipe must never send an unsigned account
    /// request. Failure preserves the caller's continuation.
    pub fn request_managed(
        &mut self,
        context: Context,
        bearer: Option<&Session>,
        method: Method,
        url: String,
        body: Option<String>,
        pending: P,
    ) -> Result<RequestId, (P, Error)> {
        self.queue_managed(
            context,
            bearer,
            method,
            url,
            body,
            ManagedRecipe::Unsigned(&[]),
            pending,
        )
    }

    /// Queue unsigned HTTP with static public codec headers. The existing
    /// managed boundary validates and snapshots them before allocating IDs.
    /// Credential-bearing contexts still require the signed entry point.
    #[allow(clippy::too_many_arguments)]
    pub fn request_managed_with_headers(
        &mut self,
        context: Context,
        bearer: Option<&Session>,
        method: Method,
        url: String,
        headers: &'static [(&'static str, &'static str)],
        body: Option<String>,
        pending: P,
    ) -> Result<RequestId, (P, Error)> {
        self.queue_managed(
            context,
            bearer,
            method,
            url,
            body,
            ManagedRecipe::Unsigned(headers),
            pending,
        )
    }

    /// Retain an unsigned recipe at creation; canonicalisation waits for route
    /// readiness. Mutations require command authority and a reconciliation codec
    /// before physical release; the engine owns their write evidence.
    #[allow(clippy::too_many_arguments)]
    pub fn request_signed_managed(
        &mut self,
        context: Context,
        bearer: Option<&Session>,
        method: Method,
        url: String,
        body: Option<String>,
        step: Box<dyn crate::SignStep>,
        pending: P,
    ) -> Result<RequestId, (P, Error)> {
        self.queue_managed(
            context,
            bearer,
            method,
            url,
            body,
            ManagedRecipe::Signed(step),
            pending,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn queue_managed(
        &mut self,
        context: Context,
        bearer: Option<&Session>,
        method: Method,
        url: String,
        body: Option<String>,
        recipe: ManagedRecipe,
        pending: P,
    ) -> Result<RequestId, (P, Error)> {
        let leg = context.leg();
        #[cfg(feature = "http-plans")]
        let admission = match self.owned_http_admission(&context) {
            Ok(admission) => admission,
            Err(error) => return Err((pending, error)),
        };
        // After admission, so a refused binding fails an owning Plan like any
        // other issue refusal.
        let resolved = self.resolve_bearer(&context, bearer);
        let generation = resolved.as_ref().ok().copied().flatten();
        let result = match resolved {
            Ok(bearer) => self.queue_managed_admitted(
                context,
                bearer,
                method,
                url,
                body,
                recipe,
                pending,
                #[cfg(feature = "http-plans")]
                admission.as_ref(),
            ),
            Err(error) => Err((pending, error)),
        };
        #[cfg(feature = "http-plans")]
        if let (Some(admission), Err((_, error))) = (admission.as_ref(), &result) {
            self.fail_owned_admission(admission, *error);
        }
        match (generation, &result) {
            (Some(generation), Ok(_)) => self.io.request_contexts.bind_leg_bearer(leg, generation),
            (Some(generation), Err(_)) => self.io.drop_empty_bearer_bound(generation),
            _ => {}
        }
        result
    }

    /// Binding grants nothing: issuer and liveness only, never session scope.
    /// A same-leg retry or resend inherits the leg's binding. Index storage is
    /// reserved here, before any effect.
    fn resolve_bearer(
        &mut self,
        context: &Context,
        bearer: Option<&Session>,
    ) -> Result<Option<u64>, Error> {
        let registry = &self.io.request_contexts;
        let generation = match bearer {
            Some(session) => Some(registry.validate_session(session)?),
            None => match registry.leg_bearer(context) {
                Some(generation) if !registry.session_generation_live(generation) => {
                    return Err(Error::Revoked);
                }
                generation => generation,
            },
        };
        if let Some(generation) = generation {
            self.io.request_contexts.reserve_leg_bearer()?;
            self.io.reserve_bearer_bound(generation)?;
        }
        Ok(generation)
    }

    #[allow(clippy::too_many_arguments)]
    fn queue_managed_admitted(
        &mut self,
        context: Context,
        bearer: Option<u64>,
        method: Method,
        url: String,
        body: Option<String>,
        recipe: ManagedRecipe,
        pending: P,
        #[cfg(feature = "http-plans")] admission: Option<&request_context_registry::OwnedAdmission>,
    ) -> Result<RequestId, (P, Error)> {
        let (step, public_headers) = match recipe {
            ManagedRecipe::Unsigned(headers) => (None, headers),
            ManagedRecipe::Signed(step) => (Some(step), &[][..]),
        };
        if let Err(error) = self.io.request_contexts.validate_send(&context, self.now) {
            return Err((pending, error));
        }
        #[cfg(feature = "http-mutation-barriers")]
        let group = match self
            .io
            .request_contexts
            .validate_mutation_group_submission(&context)
        {
            Ok(key) => key.map(|key| Box::new(mutation_group::Member::new(key))),
            Err(error) => return Err((pending, error)),
        };
        // A group claims its members together; until that claim checks every
        // member's Bearer (R11), a Bearer-bound group member is refused.
        #[cfg(feature = "http-mutation-barriers")]
        if bearer.is_some() && group.is_some() {
            return Err((pending, Error::Invalid));
        }
        if let Err(error) = self.io.http_pacing.check(context.pacing(), self.now) {
            return Err((pending, error));
        }
        if context.credential().is_some() != step.is_some() {
            return Err((pending, Error::Invalid));
        }
        if self.io.managed_http.len() >= 256 {
            return Err((pending, Error::Capacity));
        }
        // Public codec declarations are captured exactly once. Validate borrowed
        // bytes before allocating IDs or publishing any preparation resource.
        let headers = step
            .as_ref()
            .map_or(public_headers, |step| step.unsigned_http_headers());
        // On a bound request the engine alone sets `Authorization`, and it is
        // never routed to explicit public egress. Unbound requests are unchanged.
        if crate::signing_headers::validate_unsigned(headers).is_err()
            || (bearer.is_some()
                && (headers.iter().any(|(name, _)| is_authorization(name))
                    || context.route().is_some()))
        {
            return Err((pending, Error::Invalid));
        }
        let headers = headers
            .iter()
            .map(|&(name, value)| (name, value.to_owned()))
            .collect();
        // Preflight both namespaces before committing either. An admission
        // refusal must not partially allocate a request, generation or timer.
        #[cfg(feature = "http-plans")]
        let needs_ids = admission.is_none();
        #[cfg(not(feature = "http-plans"))]
        let needs_ids = true;
        if needs_ids
            && (!self.io.next_id.available(3)
                || (step.is_some() && !self.io.signing.available_id()))
        {
            return Err((pending, Error::Capacity));
        }
        #[cfg(feature = "http-plans")]
        let (request, generation, timer, deadline) = if let Some(admission) = admission {
            if !self.now.is_finite() || self.now >= admission.deadline {
                return Err((pending, Error::TimedOut));
            }
            (
                admission.attempt.request,
                admission.attempt.generation,
                admission.timer,
                admission.deadline,
            )
        } else {
            let ids = self.io.reserve_ids(3).expect("preflighted HTTP identities");
            (
                RequestId(ids.start),
                ids.start + 1,
                TimerId(ids.start + 2),
                context.deadline(),
            )
        };
        #[cfg(not(feature = "http-plans"))]
        let (request, generation, timer, deadline) = {
            let ids = self.io.reserve_ids(3).expect("preflighted HTTP identities");
            (
                RequestId(ids.start),
                ids.start + 1,
                TimerId(ids.start + 2),
                context.deadline(),
            )
        };
        let http = HttpRequest {
            id: request,
            method,
            route: context.route(),
            timeout_ms: Some((deadline - self.now).ceil().max(1.) as u32),
            url,
            headers,
            signer_headers: vec![],
            body,
        };
        let signing_attempt = if let Some(step) = step {
            #[cfg(feature = "http-plans")]
            let attempt = admission
                .and_then(|admission| admission.signing)
                .unwrap_or_else(|| self.io.signing.next_id());
            #[cfg(not(feature = "http-plans"))]
            let attempt = self.io.signing.next_id();
            if let Err(error) = self.io.signing.stage_managed(
                attempt,
                Action::Http(http.clone()),
                step,
                self.now,
                deadline,
            ) {
                return Err((
                    pending,
                    if error == crate::signing::SignError::InvalidRequest {
                        Error::Invalid
                    } else {
                        Error::Capacity
                    },
                ));
            }
            Some(attempt)
        } else {
            None
        };
        if let Err(error) = self
            .io
            .request_contexts
            .register_attempt(&context, request, self.now)
        {
            if signing_attempt.is_some() {
                self.io.signing.remove_request(request);
            }
            return Err((pending, error));
        }
        #[cfg(feature = "http-plans")]
        if let Some(admission) = admission {
            self.io.request_contexts.commit_owned_admission(admission);
        }
        let intent = match (bearer, context.routing_intent()) {
            (Some(_), RoutingIntent::Public) => RoutingIntent::Primary,
            (_, intent) => intent,
        };
        let lifecycle = Lifecycle::with_deadline(
            context,
            Attempt {
                request,
                generation,
            },
            self.now,
            deadline,
        )
        .expect("validated physical HTTP window");
        let preparation = lifecycle.preparation();
        self.io.managed_http.insert(
            request,
            Managed {
                lifecycle,
                original: http.clone(),
                timer,
                response_started: false,
                signing_attempt,
                recipe: None,
                intent,
                bearer: bearer.map(|generation| Bearer {
                    generation,
                    revision: None,
                }),
                #[cfg(feature = "http-mutation-barriers")]
                group,
                waiting_route: None,
            },
        );
        if let Some(generation) = bearer
            && let Some(bound) = self.io.bearer_bound_mut(generation)
        {
            bound.push(request);
        }
        self.io.pending.insert(
            request,
            PendingHttp {
                value: pending,
                auth: None,
            },
        );
        self.io.timers.insert(timer, ());
        self.io.actions.push(Action::SetTimer {
            timer,
            at_ms: deadline,
        });
        // A leg in flight owns the execution deadline through its own timer.
        self.io.rearm_execution_timer();
        self.io
            .actions
            .push(Action::ManagedHttp(Box::new(HttpAction::Prepare {
                preparation,
                intent,
                request: http,
            })));
        Ok(request)
    }
}

impl<P> Io<P> {
    pub(super) fn managed_response_live(
        &self,
        request: RequestId,
        token: Preparation,
        now: f64,
    ) -> bool {
        self.managed_http.get(&request).is_some_and(|managed| {
            managed.lifecycle.preparation() == token
                && managed.lifecycle.accepts_response()
                && managed.validate(&self.request_contexts, now).is_ok()
        })
    }

    pub(super) fn retire_http_leg(&mut self, context: &Context) {
        let requests = self
            .managed_http
            .iter()
            .filter_map(|(request, managed)| {
                (managed.lifecycle.context().leg() == context.leg()
                    && managed.lifecycle.context().owner() == context.owner())
                .then_some(*request)
            })
            .collect::<Vec<_>>();
        for request in requests {
            self.discard_managed(request);
        }
    }

    pub(super) fn managed_preparation(&self, request: RequestId) -> Option<Preparation> {
        Some(self.managed_http.get(&request)?.lifecycle.preparation())
    }

    pub(super) fn retire_http_command(&mut self, command: CommandId) {
        #[cfg(feature = "http-plans")]
        if let Some(timer) = self.request_contexts.plan_timer(command) {
            self.timers.remove(&timer);
            self.actions.retain(|action| !matches!(action, Action::SetTimer { timer: previous, .. } if *previous == timer));
            self.actions.push(Action::ClearTimer { timer });
        }
        let requests = self.managed_http.iter().filter_map(|(request, managed)| {
            matches!(managed.lifecycle.context().owner(), Owner::Command {id, ..} if id == command)
                .then_some(*request)
        }).collect::<Vec<_>>();
        for request in requests {
            self.discard_managed(request);
        }
    }

    /// A sealed physical Plan owner: clear its timer and unsent work.
    #[cfg(feature = "http-plans")]
    /// The physical owner's live requests, listed and their cleanup actions
    /// reserved before its seal (each: clear, cancel, timer rearm), in one
    /// reservation together with `notices` further actions.
    pub(super) fn prepare_owner_retirement(
        &mut self,
        owner: Owner,
        notices: usize,
    ) -> Result<Vec<RequestId>, crate::request_context::Error> {
        let capacity = crate::request_context::Error::Capacity;
        let count = self
            .managed_http
            .values()
            .filter(|managed| managed.lifecycle.context().owner() == owner)
            .count();
        let mut requests = Vec::new();
        requests.try_reserve_exact(count).map_err(|_| capacity)?;
        requests.extend(
            self.managed_http
                .iter()
                .filter(|(_, managed)| managed.lifecycle.context().owner() == owner)
                .map(|(request, _)| *request),
        );
        let actions = count
            .checked_mul(4)
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_add(notices))
            .ok_or(capacity)?;
        self.actions.try_reserve(actions).map_err(|_| capacity)?;
        Ok(requests)
    }

    #[cfg(feature = "http-plans")]
    /// A sealed physical Plan owner: clear its timer and the requests listed
    /// before the seal (one already settled since then is simply skipped).
    pub(super) fn retire_physical_owner(&mut self, owner: Owner, requests: Vec<RequestId>) {
        if let Some(timer) = self.request_contexts.physical_plan_timer(owner) {
            self.timers.remove(&timer);
            self.actions.retain(|action| !matches!(action, Action::SetTimer { timer: previous, .. } if *previous == timer));
            self.actions.push(Action::ClearTimer { timer });
        }
        for request in requests {
            self.discard_managed(request);
        }
    }

    pub(super) fn retire_http_owner(&mut self, owner: Owner) {
        let requests = self
            .managed_http
            .iter()
            .filter_map(|(request, managed)| {
                (managed.lifecycle.context().owner() == owner).then_some(*request)
            })
            .collect::<Vec<_>>();
        for request in requests {
            self.discard_managed(request);
        }
    }

    /// Unsent legs of an execution with no live owner get no send right.
    pub(super) fn revoke_unsent_execution_legs(&mut self) {
        let revoked = self
            .managed_http
            .iter()
            .filter(|(_, managed)| {
                matches!(
                    managed.lifecycle.context().owner(),
                    crate::request_context::Owner::Execution { .. }
                ) && managed.lifecycle.release().is_none()
                    && !self
                        .request_contexts
                        .execution_live(managed.lifecycle.context())
            })
            .map(|(request, _)| *request)
            .collect::<Vec<_>>();
        for request in revoked {
            self.discard_managed(request);
        }
    }

    fn bearer_bound_mut(&mut self, generation: u64) -> Option<&mut Vec<RequestId>> {
        self.bearer_bound
            .iter_mut()
            .find(|(bound, _)| *bound == generation)
            .map(|(_, requests)| requests)
    }

    pub(super) fn bearer_bound_len(&self, generation: u64) -> usize {
        self.bearer_bound
            .iter()
            .find(|(bound, _)| *bound == generation)
            .map_or(0, |(_, requests)| requests.len())
    }

    fn reserve_bearer_bound(&mut self, generation: u64) -> Result<(), Error> {
        if self.bearer_bound_mut(generation).is_none() {
            self.bearer_bound
                .try_reserve(1)
                .map_err(|_| Error::Capacity)?;
            self.bearer_bound.push((generation, Vec::new()));
        }
        self.bearer_bound_mut(generation)
            .expect("reserved above")
            .try_reserve(1)
            .map_err(|_| Error::Capacity)
    }

    fn drop_empty_bearer_bound(&mut self, generation: u64) {
        self.bearer_bound
            .retain(|(bound, requests)| *bound != generation || !requests.is_empty());
    }

    /// Queue every unreleased packet bound to a retired generation for a typed
    /// failure after the current callback; released packets still deliver.
    pub(super) fn fence_bearer_generation(&mut self, generation: u64) {
        let Some(index) = self
            .bearer_bound
            .iter()
            .position(|(bound, _)| *bound == generation)
        else {
            return;
        };
        for request in self.bearer_bound.swap_remove(index).1 {
            if let Some(managed) = self.managed_http.get(&request)
                && managed.lifecycle.release().is_none()
            {
                self.bearer_fenced.push(managed.lifecycle.preparation());
            }
        }
    }

    fn take_managed_with_cancel(
        &mut self,
        request: RequestId,
        cancel: Option<Action>,
    ) -> Option<Managed> {
        let managed = self.managed_http.remove(&request)?;
        if let Some(bearer) = managed.bearer
            && let Some(bound) = self.bearer_bound_mut(bearer.generation)
        {
            bound.retain(|id| *id != request);
            self.drop_empty_bearer_bound(bearer.generation);
        }
        #[cfg(feature = "http-mutation-barriers")]
        if managed.group.is_some() {
            self.actions.retain(|action| !matches!(action,
                Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::BindGroup { members, .. }
                    if members.iter().any(|member| member.preparation.attempt().request == request))));
        }
        self.request_contexts
            .retire_attempt(managed.lifecycle.context(), request);
        self.rearm_execution_timer();
        self.signing.remove_request(request);
        self.timers.remove(&managed.timer);
        self.actions.push(Action::ClearTimer {
            timer: managed.timer,
        });
        self.actions.push(cancel.unwrap_or_else(|| {
            Action::ManagedHttp(Box::new(HttpAction::Cancel {
                preparation: managed.lifecycle.preparation(),
                signing: managed.signing_attempt,
            }))
        }));
        Some(managed)
    }

    pub(super) fn discard_managed(&mut self, request: RequestId) -> Option<PendingHttp<P>> {
        self.discard_managed_with_cancel(request, None)
    }

    fn discard_managed_with_cancel(
        &mut self,
        request: RequestId,
        cancel: Option<Action>,
    ) -> Option<PendingHttp<P>> {
        let managed = self.take_managed_with_cancel(request, cancel)?;
        if managed.lifecycle.context().operation() == Operation::Mutation
            && managed.lifecycle.release().is_some()
        {
            self.http_responses.retire(request);
        } else {
            self.http_responses.remove(request);
        }
        self.streams.remove(&request);
        self.incoming.remove(&request);
        #[cfg(feature = "order-locks")]
        self.order_read_tokens.remove(&request);
        self.pending.remove(&request)
    }
}

#[cfg(not(feature = "http-mutation-barriers"))]
impl<V: Venue> Engine<V> {
    pub fn http_group_pending(&self, _: &crate::request_context::GroupPermit) -> bool {
        false
    }
    pub fn claim_http_group(
        &mut self,
        _: f64,
        _: crate::request_context::GroupPermit,
    ) -> Result<crate::request_context::GroupClaim, Error> {
        Err(Error::Invalid)
    }
}

impl<V: Venue> Engine<V> {
    /// Hosts check this before inserting an asynchronous prepared resource.
    /// A duplicate/late completion must not overwrite its replacement.
    pub fn http_preparation_pending(&self, token: Preparation) -> bool {
        if self.failed() {
            return false;
        }
        self.io
            .managed_http
            .get(&token.attempt().request)
            .is_some_and(|managed| managed.lifecycle.expects_preparation(token))
    }

    /// Preparation is a host resource, never evidence of transport dispatch.
    pub fn http_prepared(&mut self, now: f64, token: Preparation, prepared: Prepared) {
        self.assert_live();
        self.io.dispatch_now = now;
        self.expire_read_roots(now);
        let request = token.attempt().request;
        let Some(managed) = self.io.managed_http.get_mut(&request) else {
            return;
        };
        if managed.lifecycle.preparation() != token {
            return;
        }
        let prepared_result = managed
            .validate_send(&self.io.request_contexts, now)
            .and_then(|()| {
                if prepared.expires_at.is_some_and(|at| at <= now) {
                    return Err(Error::TimedOut);
                }
                managed.lifecycle.prepared(token, prepared)
            });
        #[cfg(feature = "http-mutation-barriers")]
        if managed.group.is_some() {
            match prepared_result {
                Ok(()) => self.pump_mutation_group(now, request),
                Err(Error::State) => {}
                Err(error) => self.http_preparation_failed(
                    now,
                    token,
                    error,
                    VenueError::new("HTTP group route failed"),
                ),
            }
            return;
        }
        if prepared_result.is_ok() && managed.signing_attempt.is_some() {
            self.resume_prepared_signing(now, token, prepared);
            return;
        }
        let prepared_result = prepared_result
            .and_then(|()| managed.lifecycle.unsigned())
            .and_then(|()| managed.lifecycle.permit());
        match prepared_result {
            Ok(permit) => {
                let mut request = managed.original.clone();
                request.timeout_ms =
                    Some((managed.lifecycle.deadline() - now).ceil().max(1.) as u32);
                if let Some(bearer) = managed.bearer.as_mut() {
                    match bearer
                        .read(&self.venue, &self.io.request_contexts)
                        .and_then(|session| {
                            attach_session_headers(&mut request.headers, &[], &session)
                        }) {
                        Ok(()) => {}
                        Err(reason) => {
                            self.http_preparation_failed(
                                now,
                                token,
                                reason,
                                bearer_refused(reason),
                            );
                            return;
                        }
                    }
                }
                self.io
                    .actions
                    .push(Action::ManagedHttp(Box::new(HttpAction::Bind {
                        preparation: token,
                        permit,
                        request,
                    })));
            }
            Err(Error::State) => {} // Duplicate readiness cannot replace an issued permit.
            Err(error) => self.http_preparation_failed(
                now,
                token,
                error,
                VenueError::new("HTTP preparation is no longer valid"),
            ),
        }
    }

    fn resume_prepared_signing(&mut self, now: f64, token: Preparation, prepared: Prepared) {
        let request = token.attempt().request;
        let Some(managed) = self.io.managed_http.get_mut(&request) else {
            return;
        };
        if managed.lifecycle.preparation() != token {
            return;
        }
        if let Err(error) = managed.validate(&self.io.request_contexts, now) {
            self.http_preparation_failed(
                now,
                token,
                error,
                VenueError::new("HTTP signing window expired"),
            );
            return;
        }
        if prepared.expires_at.is_some_and(|at| now >= at) {
            self.reprepare_http(now, token);
            return;
        }
        match self
            .io
            .http_pacing
            .check(managed.lifecycle.context().pacing(), now)
        {
            Err(error) => {
                self.http_preparation_failed(
                    now,
                    token,
                    error,
                    VenueError::new("Invalid HTTP signing pacing"),
                );
                return;
            }
            Ok(Some(at)) => {
                managed.waiting_route = Some(prepared);
                self.io.actions.push(Action::SetTimer {
                    timer: managed.timer,
                    at_ms: at
                        .min(managed.lifecycle.deadline())
                        .min(prepared.expires_at.unwrap_or(f64::MAX)),
                });
                return;
            }
            Ok(None) => {}
        }
        let was_waiting = managed.waiting_route.take().is_some();
        let Some(attempt) = managed.signing_attempt else {
            return;
        };
        // The token is read at the signed Bind; refuse before provider work.
        if let Some(bearer) = managed.bearer {
            let ready = match bearer.current(&self.venue, &self.io.request_contexts) {
                Ok(Some(_)) => Ok(()),
                Ok(None) => Err(Error::State),
                Err(reason) => Err(reason),
            };
            if let Err(reason) = ready {
                self.http_preparation_failed(now, token, reason, bearer_refused(reason));
                return;
            }
        }
        let ready = managed.lifecycle.begin_signing().and_then(|_| {
            self.io
                .signing
                .activate_managed(attempt, managed.lifecycle.deadline())
                .map_err(|_| Error::State)
        });
        if let Err(error) = ready {
            self.http_preparation_failed(
                now,
                token,
                error,
                VenueError::new("HTTP signing preparation failed"),
            );
        } else if was_waiting {
            self.io.actions.push(Action::SetTimer {
                timer: managed.timer,
                at_ms: managed.lifecycle.deadline(),
            });
        }
    }

    pub(super) fn managed_sign_completed(
        &mut self,
        now: f64,
        completion: crate::signing_dispatch::Completion,
    ) {
        use crate::signing_dispatch::Completion;
        let (pending, mut outbound, error, stale) = match completion {
            Completion::Ready(pending, outbound) => (pending, Some(outbound), None, false),
            Completion::Stale(pending) => (pending, None, None, true),
            Completion::Failed(pending, error) => (pending, None, Some(error), false),
        };
        let Some(request) = pending.request_id() else {
            return;
        };
        let Some(managed) = self.io.managed_http.get_mut(&request) else {
            if let Some(action) = outbound.as_mut() {
                crate::signing_dispatch::wipe(action);
            }
            return;
        };
        let token = managed.lifecycle.preparation();
        if let Err(reason) = managed.validate_send(&self.io.request_contexts, now) {
            if let Some(action) = outbound.as_mut() {
                crate::signing_dispatch::wipe(action);
            }
            self.http_preparation_failed(
                now,
                token,
                reason,
                if reason == Error::TimedOut {
                    crate::signing::SignError::TimedOut.venue_error()
                } else {
                    VenueError::new("HTTP signing result is no longer valid")
                },
            );
            return;
        }
        if let Some(error) = error {
            let reason = if error == crate::signing::SignError::TimedOut {
                Error::TimedOut
            } else {
                Error::State
            };
            self.http_preparation_failed(now, token, reason, error.venue_error());
            return;
        }
        if stale {
            managed.recipe = Some(pending);
            self.reprepare_http(now, token);
            return;
        }
        let prepared = pending
            .managed_freshness()
            .ok_or(Error::State)
            .and_then(|(revision, until)| {
                managed
                    .lifecycle
                    .signed(token, pending.receipt, revision, until)
            })
            .and_then(|()| managed.lifecycle.permit());
        match (prepared, outbound) {
            (Ok(permit), Some(Action::Http(mut packet))) if packet.id == request => {
                // Added after signing, so the venue signature never covers it.
                if let Some(bearer) = managed.bearer.as_mut() {
                    let read = bearer
                        .read(&self.venue, &self.io.request_contexts)
                        .and_then(|session| {
                            attach_session_headers(
                                &mut packet.headers,
                                &packet.signer_headers,
                                &session,
                            )
                        });
                    match read {
                        Ok(()) => {}
                        Err(reason) => {
                            crate::signing_dispatch::wipe(&mut Action::Http(packet));
                            self.http_preparation_failed(
                                now,
                                token,
                                reason,
                                bearer_refused(reason),
                            );
                            return;
                        }
                    }
                }
                packet.timeout_ms = Some(
                    (managed.lifecycle.deadline().min(pending.deadline) - now)
                        .ceil()
                        .max(1.) as u32,
                );
                managed.recipe = Some(pending);
                #[cfg(feature = "http-mutation-barriers")]
                if let Some(member) = &mut managed.group {
                    member.packet = Some(packet);
                    member.permit = Some(permit);
                    self.pump_mutation_group(now, request);
                    return;
                }
                self.io
                    .actions
                    .push(Action::ManagedHttp(Box::new(HttpAction::Bind {
                        preparation: token,
                        permit,
                        request: packet,
                    })));
            }
            (result, mut outbound) => {
                if let Some(action) = outbound.as_mut() {
                    crate::signing_dispatch::wipe(action);
                }
                self.http_preparation_failed(
                    now,
                    token,
                    result.err().unwrap_or(Error::Invalid),
                    VenueError::new("HTTP signing result is no longer valid"),
                );
            }
        }
    }

    /// A typed pre-claim failure is distinct from a response transport failure.
    /// Old preparation failures cannot remove or settle a replacement resource.
    pub fn http_preparation_failed(
        &mut self,
        now: f64,
        token: Preparation,
        reason: Error,
        mut error: VenueError,
    ) {
        self.assert_live();
        self.io.dispatch_now = now;
        self.expire_read_roots(now);
        let request = token.attempt().request;
        #[cfg(feature = "http-plans")]
        if self
            .io
            .managed_http
            .get(&request)
            .is_some_and(|managed| managed.lifecycle.preparation() == token)
            && self.expire_plan_before_http_failure(now, request)
        {
            return;
        }
        let Some(managed) = self.io.managed_http.get(&request) else {
            return;
        };
        if managed.lifecycle.preparation() != token || managed.lifecycle.release().is_some() {
            return;
        }
        if reason == Error::Stale {
            self.reprepare_http(now, token);
            return;
        }
        let delivery = managed.delivery();
        error.outcome = Some(crate::Outcome::NotSent);
        if error.kind.is_none() {
            error.kind = Some(reason.kind());
        }
        if delivery.context.operation() == Operation::Mutation {
            let _ = self.io.request_contexts.result(
                &delivery,
                crate::request_context::WriteResult::Failed(error.clone()),
            );
        }
        self.io
            .request_contexts
            .record_owned_read_failure(&delivery);
        self.io.request_contexts.enter_delivery(&delivery);
        if let Some(pending) = self.io.discard_managed(request) {
            self.begin_event_scope();
            #[cfg(feature = "http-plans")]
            {
                self.io.http_history_failure = None;
            }
            let previous = self.io.http_delivery.replace(delivery.clone());
            self.venue.on_http_preparation_failed(
                &mut Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                },
                pending.value,
                reason,
                error.clone(),
            );
            Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            }
            .finish_owned_read_failure(&delivery, error);
            self.io.http_delivery = previous;
        }
        self.io.request_contexts.exit_delivery(&delivery);
        self.finish_event_scope(now);
        self.sweep_executions(now);
        self.flush_preflight_readiness(now);
        if self.venue.cancel_needs_stored_order() {
            self.release_held(now);
        }
    }

    /// Fail the attempts a Session retire fenced, each through the common
    /// preparation-failure path after the retiring callback has returned.
    /// Runs until the queue is empty, in place to keep the storage reserved at
    /// retire: a failure callback that retires another Session is drained here.
    pub(super) fn flush_bearer_fences(&mut self, now: f64) {
        while !self.io.bearer_fenced.is_empty() {
            let token = self.io.bearer_fenced.remove(0);
            self.http_preparation_failed(
                now,
                token,
                Error::Revoked,
                bearer_refused(Error::Revoked),
            );
        }
    }

    /// The host checks its bound resource, calls this, and immediately invokes
    /// its transport API on Released, with no intervening await or actor hop.
    pub fn claim_http(&mut self, now: f64, permit: Permit) -> Result<Claim, Error> {
        self.assert_live();
        #[cfg(feature = "http-mutation-barriers")]
        if self
            .io
            .managed_http
            .get(&permit.attempt().request)
            .is_some_and(|managed| managed.group.is_some())
        {
            return Err(Error::Revoked);
        }
        self.io.dispatch_now = now;
        self.expire_read_roots(now);
        let request = permit.attempt().request;
        let managed = self
            .io
            .managed_http
            .get_mut(&request)
            .ok_or(Error::Revoked)?;
        if !managed.lifecycle.is_current_permit(permit) {
            return Err(Error::Revoked);
        }
        let preparation = managed.lifecycle.preparation();
        if let Err(error) = managed.validate_send(&self.io.request_contexts, now) {
            self.http_preparation_failed(
                now,
                preparation,
                error,
                VenueError::new("HTTP request context is no longer active"),
            );
            return Err(error);
        }
        if let Some(bearer) = managed.bearer {
            match bearer.current(&self.venue, &self.io.request_contexts) {
                Err(reason) => {
                    self.http_preparation_failed(now, preparation, reason, bearer_refused(reason));
                    return Err(reason);
                }
                Ok(revision) if revision.is_none() || revision != bearer.revision => {
                    self.http_preparation_failed(
                        now,
                        preparation,
                        Error::Stale,
                        VenueError::new("Session bearer revision moved"),
                    );
                    return Err(Error::Stale);
                }
                Ok(_) => {}
            }
        }
        let release_flag = self
            .io
            .request_contexts
            .release_flag(managed.lifecycle.context(), request)?;
        let pending = self.io.pending.get_mut(&request).ok_or(Error::Revoked)?;
        // Session writes change no orders: no reconciliation record and no
        // write-revision fence over unrelated command reconciliation.
        let journal = managed.lifecycle.context().operation() == Operation::Mutation
            && !matches!(managed.lifecycle.context().owner(), Owner::Session { .. });
        if journal {
            let capture = if self.io.streams.contains_key(&request) {
                Err(Error::Invalid)
            } else {
                self.venue.capture_http_reconciliation(&pending.value)
            }
            .and_then(|data| {
                self.io.http_responses.insert(
                    crate::request_context::ReconciliationRecord::new(
                        managed.lifecycle.context().owner(),
                        managed.lifecycle.context().leg(),
                        managed.lifecycle.attempt(),
                        managed.lifecycle.deadline(),
                        data,
                    ),
                    now,
                )
            });
            if let Err(error) = capture {
                self.http_preparation_failed(
                    now,
                    preparation,
                    error,
                    VenueError::new("HTTP mutation reconciliation is unavailable"),
                );
                return Err(error);
            }
        }
        let time = ReleaseTime {
            raw_ms: now,
            protocol_ms: self.io.protocol_clock.at(now),
            revision: self.io.protocol_clock.revision(),
        };
        let mut signing_release = None;
        let result =
            managed
                .lifecycle
                .claim(permit, time, &mut self.io.http_pacing, &mut |release| {
                    signing_release =
                        Some(crate::SigningRelease::new(release.signature, release.time)?);
                    self.venue.on_http_release(&mut pending.value, release)
                });
        if let Ok(Claim::Released(release)) = result {
            if let Some(released) = release_flag {
                *released = true;
            }
            #[cfg(feature = "http-plans")]
            self.io
                .request_contexts
                .record_read_plan_release(&managed.delivery());
            self.io
                .request_contexts
                .execution_leg_released(managed.lifecycle.context());
            managed.recipe = None;
            managed.signing_attempt = None;
            self.io
                .request_contexts
                .session_released(managed.lifecycle.context(), request);
            if journal {
                self.io.http_write_revision = self.io.http_write_revision.saturating_add(1);
                let record = self
                    .io
                    .http_responses
                    .get_mut(request)
                    .expect("reconciliation reserved before admission");
                record.release = Some(release);
                record.order_revision = self.store.orders_revision();
                record.write_revision = self.io.http_write_revision;
            }
            #[cfg(feature = "order-locks")]
            {
                if managed.lifecycle.context().operation() == Operation::Mutation {
                    match managed.lifecycle.context().owner() {
                        Owner::Command { id, .. } => self.io.order_locks.note_command_write(id),
                        // A physical Plan write is sent for every observer.
                        #[cfg(feature = "http-plans")]
                        owner @ Owner::Execution { .. } => {
                            for id in self.io.request_contexts.physical_observers(owner) {
                                self.io.order_locks.note_command_write(id);
                            }
                        }
                        _ => {}
                    }
                }
                if managed.lifecycle.context().operation() == Operation::Read {
                    let owner = managed.lifecycle.context().owner();
                    let token = self
                        .io
                        .request_contexts
                        .read_mark(owner, self.io.order_locks.token());
                    self.io.order_read_tokens.insert(request, token);
                }
            }
            self.venue.on_signing_release(
                SigningTarget::Http {
                    request,
                    pending: &mut pending.value,
                },
                signing_release.expect("release metadata checked before admission"),
            );
        }
        if journal && !matches!(result, Ok(Claim::Released(_))) {
            self.io.http_responses.remove(request);
        }
        if let Err(error) = result {
            self.http_preparation_failed(
                now,
                preparation,
                error,
                VenueError::new("HTTP request was not released"),
            );
        }
        result
    }

    fn reprepare_http(&mut self, now: f64, token: Preparation) {
        let request = token.attempt().request;
        let Some(managed) = self.io.managed_http.get_mut(&request) else {
            return;
        };
        if managed.lifecycle.preparation() != token || managed.lifecycle.release().is_some() {
            return;
        }
        if let Err(error) = managed.validate_send(&self.io.request_contexts, now) {
            self.http_preparation_failed(
                now,
                token,
                error,
                VenueError::new("HTTP preparation window expired"),
            );
            return;
        }
        #[cfg(feature = "http-mutation-barriers")]
        if managed
            .group
            .as_ref()
            .is_some_and(|member| member.announced && !member.bound)
        {
            self.http_preparation_failed(
                now,
                token,
                Error::State,
                VenueError::new("HTTP group binding failed before claim"),
            );
            return;
        }
        if managed.waiting_route.take().is_some() {
            self.io.actions.push(Action::SetTimer {
                timer: managed.timer,
                at_ms: managed.lifecycle.deadline(),
            });
        }
        // Do not advance a lifecycle generation or take its retained recipe
        // unless a fresh signing attempt can also be allocated.
        if managed.signing_attempt.is_some() && !self.io.signing.available_id() {
            self.http_preparation_failed(
                now,
                token,
                Error::Capacity,
                VenueError::new("HTTP signing identity capacity exhausted"),
            );
            return;
        }
        let next = managed.lifecycle.reprepare();
        if let Err(error) = next {
            self.http_preparation_failed(
                now,
                token,
                error,
                VenueError::new("HTTP request cannot be prepared again"),
            );
            return;
        }
        #[cfg(feature = "http-mutation-barriers")]
        if let Some(member) = &mut managed.group {
            member.reset();
        }
        let old_signing = managed.signing_attempt;
        if let Some(recipe) = managed
            .recipe
            .take()
            .or_else(|| self.io.signing.remove_request(request))
        {
            let next = self.io.signing.next_id();
            self.io.signing.retry(next, recipe);
            managed.signing_attempt = Some(next);
        }
        self.io
            .actions
            .push(Action::ManagedHttp(Box::new(HttpAction::Cancel {
                preparation: token,
                signing: old_signing,
            })));
        if let Some(bearer) = managed.bearer.as_mut() {
            bearer.revision = None;
        }
        let mut http = managed.original.clone();
        http.timeout_ms = Some((managed.lifecycle.deadline() - now).ceil().max(1.) as u32);
        self.io
            .actions
            .push(Action::ManagedHttp(Box::new(HttpAction::Prepare {
                preparation: managed.lifecycle.preparation(),
                intent: managed.intent,
                request: http,
            })));
    }

    /// Admission metadata for a host resource or reducer. Reading it grants no
    /// permission to send and does not renew any response lease.
    pub fn http_release(&self, request: RequestId) -> Option<HttpRelease> {
        if self.failed() {
            return None;
        }
        self.io.managed_http.get(&request)?.lifecycle.release()
    }

    pub(super) fn expire_http_timer(&mut self, now: f64, event: &Event<'_>) -> bool {
        let Event::TimerFired { timer } = event else {
            return false;
        };
        let Some((request, managed)) = self
            .io
            .managed_http
            .iter()
            .find(|(_, managed)| managed.timer == *timer)
        else {
            return self.expire_session_wait(now, *timer);
        };
        let request = *request;
        if now.is_finite() && now < managed.lifecycle.deadline() {
            #[cfg(feature = "http-mutation-barriers")]
            if managed
                .group
                .as_ref()
                .is_some_and(|member| member.pacing_wait)
            {
                let timer = managed.timer;
                let deadline = managed.lifecycle.deadline();
                self.io
                    .managed_http
                    .get_mut(&request)
                    .unwrap()
                    .group
                    .as_mut()
                    .unwrap()
                    .pacing_wait = false;
                self.io.actions.push(Action::SetTimer {
                    timer,
                    at_ms: deadline,
                });
                self.pump_mutation_group(now, request);
                return true;
            }
            if let Some(prepared) = managed.waiting_route {
                let token = managed.lifecycle.preparation();
                self.resume_prepared_signing(now, token, prepared);
                return true;
            }
            self.io.actions.push(Action::SetTimer {
                timer: *timer,
                at_ms: managed.lifecycle.deadline(),
            });
            return true;
        }
        #[cfg(feature = "http-plans")]
        if self.expire_plan_before_http_failure(now, request) {
            return true;
        }
        let managed = &self.io.managed_http[&request];
        if managed.lifecycle.release().is_some() {
            let kind = std::mem::replace(&mut self.io.http_failure_kind, ErrorKind::Timeout);
            self.handle_event_with_read_timeout(
                now,
                Event::HttpFailed {
                    request,
                    error: "HTTP request timed out",
                },
                true,
            );
            self.io.http_failure_kind = kind;
            self.io.discard_managed(request);
        } else {
            self.http_preparation_failed(
                now,
                managed.lifecycle.preparation(),
                Error::TimedOut,
                VenueError::new("HTTP request timed out"),
            );
        }
        true
    }

    pub(super) fn admit_http_event(&mut self, now: f64, event: &Event<'_>) -> bool {
        let request = match event {
            Event::HttpResponse { request, .. }
            | Event::HttpStart { request, .. }
            | Event::HttpChunk { request, .. }
            | Event::HttpEnd { request }
            | Event::HttpFailed { request, .. } => *request,
            _ => return true,
        };
        let Some(managed) = self.io.managed_http.get(&request) else {
            return true;
        };
        if !now.is_finite() || now >= managed.lifecycle.deadline() {
            let timer = managed.timer;
            self.expire_http_timer(now, &Event::TimerFired { timer });
            return false;
        }
        if !managed.lifecycle.accepts_response() {
            return false;
        }
        let started = managed.response_started;
        match event {
            Event::HttpResponse { .. } | Event::HttpStart { .. } if started => false,
            Event::HttpChunk { .. } | Event::HttpEnd { .. } if !started => false,
            Event::HttpStart { .. } => {
                self.io
                    .managed_http
                    .get_mut(&request)
                    .expect("managed request exists")
                    .response_started = true;
                true
            }
            _ => true,
        }
    }

    pub(super) fn settle_http_event(&mut self, request: RequestId) {
        self.io.discard_managed(request);
    }

    /// Active mutations and late responses share one bounded raw collector.
    /// No mutation array is streamed into ordinary reducers before completion.
    pub(super) fn collect_write_response(
        &mut self,
        now: f64,
        event: &Event<'_>,
        admitted: bool,
    ) -> bool {
        let request = match event {
            Event::HttpResponse { request, .. }
            | Event::HttpStart { request, .. }
            | Event::HttpChunk { request, .. }
            | Event::HttpEnd { request }
            | Event::HttpFailed { request, .. } => *request,
            _ => return false,
        };
        if !self.io.http_responses.contains(request) {
            return false;
        }
        if !now.is_finite() {
            self.fail_write_collection(now, request, Error::Invalid);
            return true;
        }
        // Invalid duplicate/order events for an active request are ignored by
        // the ordinary admission state machine. Only retirement during timeout
        // may turn a denied current event into a valid late response.
        if !admitted && self.io.managed_http.contains_key(&request) {
            return true;
        }
        let collected = match event {
            Event::HttpResponse { response, .. } => self
                .io
                .http_responses
                .start(request, response.status, response.headers)
                .and_then(|()| {
                    self.io
                        .http_responses
                        .chunk(request, response.body.as_bytes())
                }),
            Event::HttpStart {
                status, headers, ..
            } => self.io.http_responses.start(request, *status, headers),
            Event::HttpChunk { data, .. } => self.io.http_responses.chunk(request, data),
            Event::HttpEnd { .. } => Ok(()),
            Event::HttpFailed { .. } => {
                // A live request still needs the ordinary failure callback;
                // a retired failure only consumes its late-response custody.
                self.io.http_responses.remove(request);
                return !self.io.managed_http.contains_key(&request);
            }
            _ => unreachable!(),
        };
        if let Err(error) = collected {
            self.fail_write_collection(now, request, error);
            return true;
        }
        if !matches!(event, Event::HttpResponse { .. } | Event::HttpEnd { .. }) {
            return true;
        }
        let record = match self.io.http_responses.complete(request) {
            Ok(record) => record,
            Err(error) => {
                self.fail_write_collection(now, request, error);
                return true;
            }
        };
        let response = Response {
            status: record.status.expect("complete response has a status"),
            headers: &record.headers,
            body: std::str::from_utf8(&record.body).expect("completion validated UTF-8"),
        };
        let ordinary = !record.late;
        let proof = ResponseProof::from_status(response.status);
        if record.late {
            if proof == ResponseProof::NativeCodec {
                self.venue.reconcile_http(
                    &mut crate::request_context::Reconciliation::new(
                        &record,
                        &mut self.store,
                        self.io.http_write_revision,
                    ),
                    &record.data,
                    response,
                );
            }
        } else if let Some(pending) = self.io.pending.remove(&request) {
            self.begin_event_scope();
            self.count_http_status(response.status);
            #[cfg(feature = "order-locks")]
            {
                self.io.order_read_tokens.remove(&request);
            }
            let delivery = self
                .io
                .managed_http
                .get(&request)
                .expect("active capture has a managed request")
                .delivery();
            self.io.request_contexts.enter_delivery(&delivery);
            #[cfg(feature = "http-plans")]
            {
                self.io.http_history_failure = None;
            }
            let previous = self.io.http_delivery.replace(delivery.clone());
            if let Some(error) = proof.error() {
                let venue = &self.venue;
                let recorded = self.io.request_contexts.unproven_result(
                    &delivery,
                    response.status,
                    |evidence| venue.unproven_http_evidence(&pending.value, response, evidence),
                );
                if recorded.is_ok() {
                    #[cfg(feature = "http-plans")]
                    let plan = self
                        .io
                        .request_contexts
                        .plan_from_context(&delivery.context);
                    let mut cx = Cx {
                        now,
                        store: &mut self.store,
                        io: &mut self.io,
                    };
                    #[cfg(feature = "http-plans")]
                    let sealed = if let Some(failure) = cx
                        .io
                        .request_contexts
                        .independent_plan_failure(&delivery, error.clone())
                    {
                        cx.with_independent_plan_failure(
                            plan.as_ref()
                                .expect("independent failure belongs to a Plan"),
                            failure,
                            |cx, failure| self.venue.on_http_failed(cx, pending.value, failure),
                        )
                    } else if let Some(plan) = &plan {
                        cx.fail_plan_owned(plan, error)
                    } else {
                        cx.fail_context(&delivery.context, error)
                    };
                    #[cfg(feature = "http-plans")]
                    let context_sealed = sealed.is_ok() && plan.is_none();
                    #[cfg(not(feature = "http-plans"))]
                    let context_sealed = cx.fail_context(&delivery.context, error).is_ok();
                    if context_sealed {
                        self.venue
                            .discard_unproven_http_command(delivery.context.owner());
                    }
                }
            } else {
                observe_account_auth(
                    &self.venue,
                    &pending,
                    &response,
                    true,
                    now,
                    &self.store,
                    &mut self.io.actions,
                );
                self.io.http_write_reply = Some((response.status, response.headers.to_owned()));
                self.venue.on_response(
                    &mut Cx {
                        now,
                        store: &mut self.store,
                        io: &mut self.io,
                    },
                    pending.value,
                    response,
                );
                self.io.http_write_reply = None;
            }
            self.io.http_delivery = previous;
            self.io.request_contexts.exit_delivery(&delivery);
            self.io.discard_managed(request);
        }
        drop(record);
        self.io.http_responses.remove(request);
        // Only active ordinary delivery owns event bookkeeping. Late codecs
        // never drive auth/retry/deferred commands or tickers.
        if ordinary {
            self.finish_event_scope(now);
            if self.venue.cancel_needs_stored_order() {
                self.release_held(now);
            }
        }
        true
    }

    fn fail_write_collection(&mut self, now: f64, request: RequestId, _reason: Error) {
        self.io.http_responses.remove(request);
        if self.io.managed_http.contains_key(&request) {
            // The venue answered (non-UTF-8 or oversize reply): never transport loss.
            let kind = std::mem::replace(&mut self.io.http_failure_kind, ErrorKind::Internal);
            self.handle_event(
                now,
                Event::HttpFailed {
                    request,
                    error: "HTTP mutation response failed integrity bounds",
                },
            );
            self.io.http_failure_kind = kind;
            self.io.discard_managed(request);
            if self.venue.cancel_needs_stored_order() {
                self.release_held(now);
            }
        }
    }
}

#[cfg(feature = "http-plans")]
pub(in crate::engine) struct PreparedReadCleanup {
    request: RequestId,
    preparation: Preparation,
    cancel: Action,
}

#[cfg(feature = "http-plans")]
impl<P> Io<P> {
    pub(super) fn prepare_read_history_cleanup(
        &mut self,
        owner: Owner,
        leg: Option<u64>,
        notices: usize,
    ) -> Result<Vec<PreparedReadCleanup>, Error> {
        let matches = |managed: &Managed| {
            managed.lifecycle.context().owner() == owner
                && leg.is_none_or(|leg| managed.lifecycle.context().leg() == leg)
        };
        let count = self
            .managed_http
            .values()
            .filter(|managed| matches(managed))
            .count();
        let mut cleanup = Vec::new();
        cleanup
            .try_reserve_exact(count)
            .map_err(|_| Error::Capacity)?;
        // Four per request cover Cancel, its timer clear, and execution-timer clear/rearm.
        // Additional notices cover Plan timer actions, the terminal and optional marker.
        self.actions
            .try_reserve(
                count
                    .checked_mul(4)
                    .and_then(|n| n.checked_add(notices))
                    .ok_or(Error::Capacity)?,
            )
            .map_err(|_| Error::Capacity)?;
        for (request, managed) in &self.managed_http {
            if matches(managed) {
                cleanup.push(PreparedReadCleanup {
                    request: *request,
                    preparation: managed.lifecycle.preparation(),
                    cancel: Action::ManagedHttp(Box::new(HttpAction::Cancel {
                        preparation: managed.lifecycle.preparation(),
                        signing: managed.signing_attempt,
                    })),
                });
            }
        }
        Ok(cleanup)
    }
    pub(super) fn commit_read_history_cleanup(&mut self, cleanup: Vec<PreparedReadCleanup>) {
        for item in cleanup {
            assert!(
                self.managed_http
                    .get(&item.request)
                    .is_some_and(|managed| managed.lifecycle.preparation() == item.preparation)
            );
            self.discard_managed_with_cancel(item.request, Some(item.cancel));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_context::{Admission, Pacing, Policy, RetryPolicy, Scope};
    #[cfg(feature = "http-plans")]
    include!("request_context_preflight_call_tests.rs");
    #[cfg(feature = "http-plans")]
    include!("request_context_planned_tests.rs");
    #[cfg(feature = "http-plans")]
    include!("execution_planned_tests.rs");
    #[cfg(feature = "http-plans")]
    include!("request_context_reconciliation_tests.rs");
    #[cfg(feature = "http-plans")]
    include!("request_context_plan_expiry_tests.rs");
    #[cfg(feature = "http-plans")]
    include!("request_context_plan_independence_tests.rs");

    use crate::request_context::SecretHeaderValue;
    use crate::signing_headers::{MAX_HEADERS, MAX_VALUE_BYTES};

    fn session(names: &[&'static str], value: &str) -> SessionHeaders {
        SessionHeaders::new(
            names
                .iter()
                .map(|name| (*name, SecretHeaderValue::new(value.into()).unwrap()))
                .collect(),
        )
        .unwrap()
    }
    fn signer(count: usize, value: &str) -> Vec<(String, String)> {
        (0..count)
            .map(|index| (format!("x-s{index}"), value.to_owned()))
            .collect()
    }
    const RECIPE_NAMES: [&str; 29] = [
        "x-r0", "x-r1", "x-r2", "x-r3", "x-r4", "x-r5", "x-r6", "x-r7", "x-r8", "x-r9", "x-r10",
        "x-r11", "x-r12", "x-r13", "x-r14", "x-r15", "x-r16", "x-r17", "x-r18", "x-r19", "x-r20",
        "x-r21", "x-r22", "x-r23", "x-r24", "x-r25", "x-r26", "x-r27", "x-r28",
    ];
    fn recipe(count: usize, value: &str) -> Vec<(&'static str, String)> {
        RECIPE_NAMES[..count]
            .iter()
            .map(|name| (*name, value.to_owned()))
            .collect()
    }

    /// The final packet is recipe + signer + session headers; its count limit
    /// spans all three and a refusal leaves both vectors exactly as they were.
    #[test]
    fn session_headers_count_against_the_whole_packet_including_signer_headers() {
        let signer_headers = signer(3, "v");
        let mut headers = recipe(28, "v");
        attach_session_headers(&mut headers, &signer_headers, &session(&["Cookie"], "c"))
            .expect("exactly 32 headers");
        assert_eq!(headers.len() + signer_headers.len(), MAX_HEADERS);

        let original = recipe(29, "v");
        let mut headers = original.clone();
        assert_eq!(
            attach_session_headers(&mut headers, &signer_headers, &session(&["Cookie"], "c")),
            Err(Error::Invalid)
        );
        assert_eq!(headers, original);
        assert_eq!(signer_headers, signer(3, "v"));
    }

    /// Bytes likewise: each vector fits the budget alone; together they don't.
    #[test]
    fn session_headers_byte_budget_spans_recipe_and_signer_headers() {
        let big = "x".repeat(MAX_VALUE_BYTES);
        let original = recipe(1, &big);
        let signer_headers = signer(1, &big);
        let mut headers = original.clone();
        assert_eq!(
            attach_session_headers(
                &mut headers,
                &signer_headers,
                &session(&["Cookie", "X-Id"], &big)
            ),
            Err(Error::Invalid)
        );
        assert_eq!(headers, original);

        let fits = "x".repeat(MAX_VALUE_BYTES - 64);
        let mut headers = recipe(1, &fits);
        attach_session_headers(
            &mut headers,
            &signer(1, &fits),
            &session(&["Cookie", "X-Id"], &fits),
        )
        .expect("four values under the shared budget");
    }

    #[derive(Debug)]
    struct Pending(Context);
    #[derive(Clone, Copy, Debug)]
    enum EffectFact {
        Missing,
        Accepted,
        KnownPartial,
        PartialUnknown,
        Rejected,
        Failed,
        Overflow,
    }

    // The original control used only live issuance, not recorded-fact authority.
    fn write_effect_authority<P>(cx: &Cx<'_, P>, context: &Context) -> Result<(), Error> {
        cx.validate_http_write_effects(context)
    }
    #[derive(Default)]
    struct Probe {
        count: usize,
        command: Option<CommandId>,
        deny: bool,
        responses: usize,
        items: usize,
        failures: Vec<(Error, Option<crate::Outcome>)>,
        failure_messages: Vec<String>,
        releases: Vec<HttpRelease>,
        receipts: usize,
        finish_in_item: Option<bool>,
        signed: bool,
        reconciliations: std::cell::RefCell<Vec<String>>,
        refuse_capture: bool,
        auth_reject: bool,
        retry_refusal: bool,
        hostile_response_patch: bool,
        command_calls: Vec<CommandId>,
        evidence_owners: std::cell::RefCell<Vec<crate::request_context::Owner>>,
        discarded_owners: Vec<crate::request_context::Owner>,
        capture_auth_details: bool,
        auth_observations: std::cell::Cell<usize>,
        hold_write_fact: Option<bool>,
        native_unproven: bool,
        effect_fact: Option<EffectFact>,
        native_effects: usize,
        housekeeping_checks: usize,
        reconciles: Vec<Option<Vec<String>>>,
        // The command intent a test mutation is issued under (default Place).
        mutation_intent: Option<crate::classify::Intent>,
        // Whether a reconcile hook ever saw borrowed delivery/read authority.
        #[cfg(feature = "order-locks")]
        reconcile_custody: bool,
        // Later reads are legs of the first read's owner (one logical read).
        legs: bool,
        // A delivered read reports itself as a complete order read.
        #[cfg(feature = "order-locks")]
        reconcile_reads: bool,
    }
    const GROUP: Pacing = Pacing {
        group: 1,
        minimum_gap_ms: 450.,
    };
    impl Venue for Probe {
        type Pending = Pending;
        fn unproven_http_evidence(
            &self,
            pending: &Pending,
            _: Response<'_>,
            evidence: &mut crate::request_context::UnprovenEvidence,
        ) {
            self.evidence_owners.borrow_mut().push(pending.0.owner());
            if self.capture_auth_details {
                evidence.venue_code("10004");
                evidence.venue_message("authentication refused");
                assert!(evidence.order_id("native-evidence"));
            }
        }
        fn discard_unproven_http_command(&mut self, owner: crate::request_context::Owner) {
            self.discarded_owners.push(owner);
        }
        fn account_auth_outcome(
            &self,
            _: &Pending,
            _: &Response<'_>,
            _: f64,
            _: &Store,
        ) -> Option<AuthOutcome> {
            if self.native_unproven {
                self.auth_observations.set(self.auth_observations.get() + 1);
                return None;
            }
            if self.capture_auth_details {
                self.auth_observations.set(self.auth_observations.get() + 1);
                Some(AuthOutcome::Rejected)
            } else {
                None
            }
        }
        fn capture_http_reconciliation(
            &self,
            _: &Pending,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            if self.refuse_capture {
                return Err(Error::Invalid);
            }
            crate::request_context::ReconciliationData::new("native-correlation".into())
        }
        fn reconcile_http(
            &self,
            cx: &mut crate::request_context::Reconciliation<'_>,
            data: &crate::request_context::ReconciliationData,
            response: Response<'_>,
        ) {
            assert_eq!(data.as_str(), "native-correlation");
            self.reconciliations.borrow_mut().push(response.body.into());
            if response.body == "ok" {
                cx.set_order_status("one", crate::model::OrderStatus::Canceled);
                cx.set_order_status("two", crate::model::OrderStatus::Canceled);
            }
        }
        fn http_pacing(&self) -> &[Pacing] {
            &[GROUP]
        }
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            let mut root: Option<Context> = None;
            for _ in 0..self.count {
                let scope = self.command.map_or(Scope::Background, Scope::Command);
                let policy = Policy {
                    operation: Operation::Read,
                    credential: self.signed.then(|| crate::request_context::Credential {
                        slot: CredentialSlot::Primary,
                        reference: crate::signing::KeyRef("fixture".into()),
                    }),
                    timeout_ms: 1_000,
                    pacing: Some(GROUP),
                    retry: RetryPolicy::Never,
                };
                let context = match root.as_ref().filter(|_| self.legs) {
                    Some(root) => cx.http_leg_context(root, policy),
                    None => cx.http_context(scope, policy),
                }
                .unwrap();
                root.get_or_insert_with(|| context.clone());
                let request = if self.signed {
                    cx.request_signed_managed(
                        context.clone(),
                        None,
                        Method::Post,
                        "https://venue.test/read".into(),
                        Some("{}".into()),
                        Box::new(Recipe),
                        Pending(context),
                    )
                } else {
                    cx.request_managed(
                        context.clone(),
                        None,
                        Method::Post,
                        "https://venue.test/read".into(),
                        Some("{}".into()),
                        Pending(context),
                    )
                }
                .unwrap();
                cx.stream_response(request, ArrayAt::Root);
            }
        }
        fn on_http_release(
            &mut self,
            _: &mut Pending,
            release: &HttpRelease,
        ) -> Result<Admission, Error> {
            self.releases.push(*release);
            Ok(if self.deny {
                Admission::Denied
            } else {
                Admission::Unscheduled
            })
        }
        fn on_http_preparation_failed(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            pending: Pending,
            reason: Error,
            error: VenueError,
        ) {
            self.failures.push((reason, error.outcome));
            self.failure_messages.push(error.message.clone());
            if pending.0.operation() == Operation::Mutation {
                cx.fail_context(&pending.0, error).unwrap();
            }
        }
        fn on_signing_receipt(
            &mut self,
            _: SigningTarget<'_, Pending>,
            _: crate::signing::SignReceipt,
        ) {
            self.receipts += 1;
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: Response<'_>) {
            self.responses += 1;
            if self.native_unproven {
                cx.http_write_unproven_result(&pending.0, |evidence| {
                    evidence.venue_code("10004");
                    evidence.venue_message("invalid api signature in malformed native proof");
                    assert!(evidence.order_id("evidence-only"));
                })
                .unwrap();
                assert_eq!(
                    cx.http_write_unproven_result(&pending.0, |_| panic!("duplicate capture")),
                    Err(Error::State)
                );
                cx.fail_context(&pending.0, "invalid native ACK").unwrap();
                return;
            }
            if let Some(fact) = self.effect_fact {
                use crate::request_context::WriteResult;
                let result = match fact {
                    EffectFact::Missing => None,
                    EffectFact::Accepted => Some(WriteResult::Accepted {
                        order_ids: vec!["admitted".into()],
                    }),
                    EffectFact::KnownPartial => Some(WriteResult::KnownPartial(VenueError::new(
                        "known partial without native IDs",
                    ))),
                    EffectFact::PartialUnknown => Some(WriteResult::PartialUnknown(
                        VenueError::new("native subset with unresolved remainder"),
                    )),
                    EffectFact::Rejected => {
                        Some(WriteResult::Rejected(VenueError::new("native refused")))
                    }
                    EffectFact::Failed => Some(WriteResult::Failed(VenueError::new("unproven"))),
                    EffectFact::Overflow => Some(WriteResult::Accepted {
                        order_ids: vec!["n".repeat(1025)],
                    }),
                };
                if let Some(result) = result {
                    cx.http_write_result(&pending.0, result).unwrap();
                }
                if write_effect_authority(cx, &pending.0).is_ok() {
                    self.native_effects += 1;
                    cx.store.set_hedged(true);
                }
                if cx.validate_http_write_housekeeping(&pending.0).is_ok() {
                    self.housekeeping_checks += 1;
                }
                if matches!(fact, EffectFact::Accepted) {
                    cx.done_context(&pending.0, "ok".into()).unwrap();
                } else {
                    cx.fail_context(&pending.0, "finish native effect probe")
                        .unwrap();
                }
                return;
            }
            if self.hostile_response_patch {
                cx.store
                    .set_order_status("one", crate::model::OrderStatus::Canceled);
            }
            if pending.0.operation() == Operation::Mutation {
                #[cfg(feature = "order-locks")]
                assert!(
                    cx.io.order_response_token.is_none(),
                    "write cannot inherit a prior read fence"
                );
                if self.retry_refusal && self.responses == 1 {
                    use crate::classify::{CodeTable, Key, Native, NativeCode, Row, Verdict};
                    const TIMESTAMP: CodeTable = CodeTable {
                        rows: &[Row {
                            key: Key::Int(10002),
                            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
                            only: None,
                        }],
                        success: &[],
                        unlisted: Verdict::Unknown(None),
                        status: &[],
                    };
                    let native = Native {
                        code: NativeCode::Int(10002),
                        message: "invalid timestamp",
                    };
                    let (_, grant) = cx
                        .http_write_failure(
                            &pending.0,
                            &TIMESTAMP,
                            Some(&native),
                            "native timestamp refusal",
                            None,
                        )
                        .unwrap();
                    let context = cx.http_resend_context(&pending.0, grant.unwrap()).unwrap();
                    cx.request_signed_managed(
                        context.clone(),
                        None,
                        Method::Post,
                        "https://venue.test/write".into(),
                        Some("{}".into()),
                        Box::new(Recipe),
                        Pending(context),
                    )
                    .unwrap();
                    return;
                }
                if let Some(partial) = self.hold_write_fact {
                    let fact = if partial {
                        crate::request_context::WriteResult::KnownPartial(
                            VenueError::new("known partial").partial_ids(&["prior-native".into()]),
                        )
                    } else {
                        crate::request_context::WriteResult::Accepted {
                            order_ids: vec!["prior-native".into()],
                        }
                    };
                    cx.http_write_result(&pending.0, fact).unwrap();
                    return;
                }
                if self.auth_reject {
                    let error = VenueError::new("native authentication refusal")
                        .kind(ErrorKind::AuthInvalid);
                    cx.http_write_result(
                        &pending.0,
                        crate::request_context::WriteResult::Rejected(error.clone()),
                    )
                    .unwrap();
                    cx.fail_context(&pending.0, error).unwrap();
                    return;
                }
                cx.http_write_result(
                    &pending.0,
                    crate::request_context::WriteResult::Accepted { order_ids: vec![] },
                )
                .unwrap();
                cx.done_context(&pending.0, "ok".into()).unwrap();
            } else {
                #[cfg(feature = "order-locks")]
                if self.reconcile_reads {
                    cx.orders_reconciled(None);
                }
                cx.finish_http_context(&pending.0).unwrap();
            }
        }
        fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, _: &str) {
            self.items += 1;
            if let Some(fail) = self.finish_in_item {
                if fail {
                    cx.fail_context(&pending.0, "stopped during streaming")
                        .unwrap();
                } else {
                    cx.done_context(&pending.0, "null".into()).unwrap();
                }
            }
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
            if pending.0.operation() == Operation::Mutation {
                cx.fail_context(&pending.0, error).unwrap();
            } else {
                panic!("unexpected transport failure");
            }
        }
        fn reconcile_uncertain(&mut self, _cx: &mut Cx<'_, Pending>, symbols: Option<&[String]>) {
            self.reconciles.push(symbols.map(<[String]>::to_vec));
            #[cfg(feature = "order-locks")]
            {
                self.reconcile_custody |= _cx_has_custody(_cx);
            }
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
            if let Command::FetchLeverage { command, .. }
            | Command::CancelSymbolOrders { command, .. } = &command
            {
                self.command_calls.push(*command);
            }
            if let Command::FetchLeverage { command, .. } = command {
                cx.done(command, "null".into());
            }
        }
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {
            panic!("engine timer leaked to venue");
        }
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    struct Recipe;
    impl crate::SignStep for Recipe {
        fn prepare(
            &mut self,
            outbound: &mut Action,
            context: &crate::DispatchContext<'_>,
        ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
            let Action::Http(http) = outbound else {
                return Err(crate::signing::SignError::InvalidRequest);
            };
            http.url = format!("https://venue.test/read?stamp={}", context.stamp_ms());
            Ok(crate::signing::SignRequest {
                receipt: crate::signing::SignReceipt {
                    stamp_ms: Some(context.stamp_ms()),
                    expiry_ms: Some(context.stamp_ms() + 3_000.),
                },
                context: Default::default(),
                operations: vec![crate::signing::Operation::Present {
                    key: crate::signing::KeyRef("fixture".into()),
                }],
            })
        }
        fn apply(
            &mut self,
            outbound: &mut Action,
            result: &crate::signing::SignResult,
        ) -> Result<(), crate::signing::SignError> {
            let Action::Http(http) = outbound else {
                return Err(crate::signing::SignError::InvalidRequest);
            };
            http.signer_headers.push((
                "x-credential".into(),
                String::from_utf8(result.values[0].clone())
                    .map_err(|_| crate::signing::SignError::InvalidResult)?,
            ));
            Ok(())
        }
        fn freshness_ms(&self) -> f64 {
            3_000.
        }
    }

    fn route(engine: &mut Engine<Probe>, token: Preparation, at: f64) -> Vec<Action> {
        engine.http_prepared(
            at,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        engine.drain_actions().collect()
    }
    fn work(actions: &[Action]) -> (RequestId, crate::signing::SignRequest) {
        actions
            .iter()
            .find_map(|action| match action {
                Action::Sign { attempt, work, .. } => Some((*attempt, work.clone())),
                _ => None,
            })
            .expect("signing work after route readiness")
    }
    fn signed(
        engine: &mut Engine<Probe>,
        at: f64,
        attempt: RequestId,
        work: crate::signing::SignRequest,
    ) -> Vec<Action> {
        engine.handle(
            at,
            Event::Signed {
                attempt,
                result: Ok(crate::signing::SignResult {
                    values: vec![b"value".to_vec()],
                    receipt: work.receipt,
                    headers: vec![],
                }),
            },
        );
        engine.drain_actions().collect()
    }
    fn bound(actions: &[Action]) -> Permit {
        actions
            .iter()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match action.as_ref() {
                    HttpAction::Bind {
                        permit, request, ..
                    } => {
                        assert_eq!(
                            request.signer_headers,
                            [("x-credential".into(), "value".into())]
                        );
                        Some(*permit)
                    }
                    _ => None,
                },
                _ => None,
            })
            .expect("bound signed packet")
    }

    struct WsRecipe;
    impl crate::SignStep for WsRecipe {
        fn prepare(
            &mut self,
            _: &mut Action,
            _: &crate::DispatchContext<'_>,
        ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
            Ok(crate::signing::SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![crate::signing::Operation::Present {
                    key: crate::signing::KeyRef("fixture".into()),
                }],
            })
        }
        fn apply(
            &mut self,
            outbound: &mut Action,
            _: &crate::signing::SignResult,
        ) -> Result<(), crate::signing::SignError> {
            let Action::WsSend { text, .. } = outbound else {
                return Err(crate::signing::SignError::InvalidRequest);
            };
            *text = "signed auth".into();
            Ok(())
        }
        fn freshness_ms(&self) -> f64 {
            3_000.
        }
    }

    #[test]
    fn managed_prepare_carries_the_issued_public_egress_route_through_repreparation() {
        use crate::request_context::{Egress, RoutingIntent};
        for (egress, route) in [
            (Egress::PublicCors, crate::RequestRoute::PublicCors),
            (Egress::Direct, crate::RequestRoute::Direct),
        ] {
            let (mut engine, _) = start(Probe::default());
            engine.drain_actions().for_each(drop);
            let mut cx = Cx {
                now: 0.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            let context = cx
                .http_context_with_egress(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 1_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                    egress,
                )
                .unwrap();
            cx.request_managed(
                context.clone(),
                None,
                Method::Post,
                "https://venue.test/candles".into(),
                None,
                Pending(context),
            )
            .unwrap();
            let prepared = |engine: &mut Engine<Probe>| {
                engine
                    .drain_actions()
                    .find_map(|a| match a {
                        Action::ManagedHttp(a) => match *a {
                            HttpAction::Prepare {
                                preparation,
                                intent,
                                request,
                            } => Some((preparation, intent, request.route)),
                            _ => None,
                        },
                        _ => None,
                    })
                    .unwrap()
            };
            let (first, intent, declared) = prepared(&mut engine);
            assert_eq!((intent, declared), (RoutingIntent::Public, Some(route)));
            // A stale route re-prepares the same original packet, route included.
            bind(&mut engine, first, 0.5);
            engine.http_preparation_failed(1., first, Error::Stale, VenueError::new("expired"));
            let (second, intent, declared) = prepared(&mut engine);
            assert_ne!(second, first);
            assert_eq!((intent, declared), (RoutingIntent::Public, Some(route)));
        }
    }

    #[test]
    fn managed_http_keeps_signed_ws_and_plain_subscription_barriers_with_cancellation() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            signed: true,
            ..Probe::default()
        });
        let socket = SocketId(99);
        {
            let mut cx = Cx {
                now: 0.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            cx.send_signed(socket, "auth".into(), Box::new(WsRecipe));
            cx.send(socket, "subscription");
            let context = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: Some(crate::request_context::Credential {
                            slot: CredentialSlot::Primary,
                            reference: crate::signing::KeyRef("fixture".into()),
                        }),
                        timeout_ms: 1_000,
                        pacing: Some(GROUP),
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_signed_managed(
                context.clone(),
                None,
                Method::Post,
                "https://venue.test/read".into(),
                None,
                Box::new(Recipe),
                Pending(context),
            )
            .unwrap();
        }
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(
            actions
                .iter()
                .all(|a| !matches!(a, Action::Sign { .. } | Action::WsSend { .. }))
        );
        let third = actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        // C is ready, but cannot cross signed WS + subscription sequence.
        assert!(route(&mut engine, third, 1.).is_empty());
        let (second, input) = work(&route(&mut engine, tokens[1], 2.));
        let actions = signed(&mut engine, 3., second, input);
        bound(&actions);
        assert!(
            actions
                .iter()
                .all(|a| !matches!(a, Action::Sign { .. } | Action::WsSend { .. }))
        );
        let (first, _) = work(&route(&mut engine, tokens[0], 4.));
        let context = engine
            .io
            .pending
            .get(&tokens[0].attempt().request)
            .unwrap()
            .value
            .0
            .clone();
        {
            let mut cx = Cx {
                now: 5.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            cx.finish_http_context(&context).unwrap();
        }
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Cancel { signing: Some(id), .. } if *id == first))));
        let (ws, input) = work(&actions);
        assert_ne!(ws, second);
        assert_ne!(ws, first);
        let actions = signed(&mut engine, 6., ws, input);
        let auth = actions
            .iter()
            .position(|a| matches!(a, Action::WsSend { text, .. } if text == "signed auth"))
            .unwrap();
        let subscription = actions
            .iter()
            .position(|a| matches!(a, Action::WsSend { text, .. } if text == "subscription"))
            .unwrap();
        let after = actions
            .iter()
            .position(|a| matches!(a, Action::Sign { .. }))
            .unwrap();
        assert!(auth < subscription && subscription < after);
        // Ready B keeps only its original unsigned recipe, never presented bytes.
        let b = engine
            .io
            .managed_http
            .get(&tokens[1].attempt().request)
            .unwrap();
        assert!(b.original.signer_headers.is_empty());
        let Action::Http(unsigned) = &b.recipe.as_ref().unwrap().outbound else {
            panic!("HTTP recipe");
        };
        assert!(unsigned.signer_headers.is_empty());
        assert!(!unsigned.url.contains("stamp="));
    }

    #[test]
    fn invalid_recipe_and_full_signing_queue_preserve_continuation_without_resources() {
        struct Invalid;
        impl crate::SignStep for Invalid {
            fn prepare(
                &mut self,
                _: &mut Action,
                _: &crate::DispatchContext<'_>,
            ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
                panic!("invalid recipe must fail before preparation");
            }
            fn apply(
                &mut self,
                _: &mut Action,
                _: &crate::signing::SignResult,
            ) -> Result<(), crate::signing::SignError> {
                panic!("invalid recipe");
            }
            fn freshness_ms(&self) -> f64 {
                f64::NAN
            }
        }
        for full in [false, true] {
            let mut engine = crate::test_support::seeded_engine(Probe::default());
            if full {
                for n in 0..256 {
                    let id = engine.io.signing.next_id();
                    engine
                        .io
                        .signing
                        .stage(
                            id,
                            Action::Http(HttpRequest {
                                id: RequestId(10_000 + n),
                                method: Method::Get,
                                route: None,
                                timeout_ms: None,
                                url: "https://venue.test/read".into(),
                                headers: vec![],
                                signer_headers: vec![],
                                body: None,
                            }),
                            Box::new(Recipe),
                            TimerId(0),
                            0.,
                        )
                        .unwrap();
                }
            }
            let mut cx = Cx {
                now: 0.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            let context = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: Some(crate::request_context::Credential {
                            slot: CredentialSlot::Primary,
                            reference: crate::signing::KeyRef("fixture".into()),
                        }),
                        timeout_ms: 1_000,
                        pacing: Some(GROUP),
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            let step: Box<dyn crate::SignStep> = if full {
                Box::new(Recipe)
            } else {
                Box::new(Invalid)
            };
            let result = cx.request_signed_managed(
                context.clone(),
                None,
                Method::Get,
                "https://venue.test/read".into(),
                None,
                step,
                Pending(context.clone()),
            );
            let expected = if full {
                Error::Capacity
            } else {
                Error::Invalid
            };
            assert!(
                matches!(result, Err((Pending(returned), error)) if error == expected && returned == context)
            );
            assert!(cx.io.pending.is_empty());
            assert!(cx.io.managed_http.is_empty());
            assert!(cx.io.timers.is_empty());
            assert!(cx.io.actions.is_empty());
        }
    }

    // These controls exercise downstream narrower-window custody only. Initial
    // queue resource/packet capture is verified by owned admission separately.
    fn narrow_physical_window(engine: &mut Engine<Probe>, token: Preparation, deadline: f64) {
        let managed = engine
            .io
            .managed_http
            .get_mut(&token.attempt().request)
            .unwrap();
        assert!(deadline < managed.lifecycle.deadline());
        managed.lifecycle = Lifecycle::with_deadline(
            managed.lifecycle.context().clone(),
            token.attempt(),
            0.,
            deadline,
        )
        .unwrap();
        assert_eq!(managed.lifecycle.preparation(), token);
    }

    #[test]
    fn physical_window_unsigned_bind_uses_remaining_cap_and_retains_payload() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            ..Probe::default()
        });
        let token = tokens[0];
        narrow_physical_window(&mut engine, token, 50.);
        let actions = route(&mut engine, token, 10.);
        assert!(actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { request, .. } if request.timeout_ms == Some(40) && request.url == "https://venue.test/read" && request.body.as_deref() == Some("{}")))));
    }

    /// The Bind packet's Bearer copy is wiped with the packet: on a signed
    /// refusal, and for any action the host never drained when the engine drops.
    #[test]
    fn bind_wipe_zeroizes_the_bearer_copy() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            ..Probe::default()
        });
        let mut bind = route(&mut engine, tokens[0], 10.)
            .into_iter()
            .find(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. })))
            .expect("bind");
        let Action::ManagedHttp(managed) = &mut bind else {
            unreachable!()
        };
        let HttpAction::Bind { request, .. } = managed.as_mut() else {
            unreachable!()
        };
        request
            .headers
            .insert(0, ("Authorization", "Bearer jwt-secret".into()));
        crate::signing_dispatch::wipe(&mut bind);
        let Action::ManagedHttp(managed) = &bind else {
            unreachable!()
        };
        let HttpAction::Bind { request, .. } = managed.as_ref() else {
            unreachable!()
        };
        assert!(request.headers.iter().all(|(_, value)| value.is_empty()));
        assert!(request.url.is_empty());
    }

    #[test]
    fn physical_window_default_timeout_keeps_existing_signing_diagnostic() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            ..Probe::default()
        });
        let token = tokens[0];
        let (attempt, input) = work(&route(&mut engine, token, 10.));
        let actions = signed(&mut engine, 1_000., attempt, input);
        assert!(!actions.iter().any(
            |a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. }))
        ));
        assert_eq!(
            engine.venue.failure_messages,
            ["Signing timed out before dispatch"]
        );
        assert_eq!(
            engine.venue.failures,
            [(Error::TimedOut, Some(crate::Outcome::NotSent))]
        );
    }

    #[test]
    fn physical_window_refuses_route_preparation_at_exact_expiry() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            ..Probe::default()
        });
        let token = tokens[0];
        narrow_physical_window(&mut engine, token, 50.);
        let actions = route(&mut engine, token, 50.);
        assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. })));
        assert_eq!(engine.venue.receipts, 0);
        assert_eq!(
            engine.venue.failures,
            [(Error::TimedOut, Some(crate::Outcome::NotSent))]
        );
    }

    #[test]
    fn physical_window_delayed_drain_never_prepares_provider_work_after_cap() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            ..Probe::default()
        });
        let token = tokens[0];
        narrow_physical_window(&mut engine, token, 50.);
        engine.http_prepared(
            49.,
            token,
            Prepared {
                id: 7,
                expires_at: None,
            },
        );
        engine.prepare_dispatch(50.);
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. })));
        assert_eq!(engine.venue.receipts, 0);
        assert_eq!(
            engine.venue.failures,
            [(Error::TimedOut, Some(crate::Outcome::NotSent))]
        );
    }

    #[test]
    fn physical_window_timeout_adjustment_cannot_renew_activated_provider_work() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            ..Probe::default()
        });
        let token = tokens[0];
        narrow_physical_window(&mut engine, token, 50.);
        engine.http_prepared(
            49.,
            token,
            Prepared {
                id: 7,
                expires_at: None,
            },
        );
        Cx {
            now: 49.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .request_timeout(token.attempt().request, 1_000);
        engine.prepare_dispatch(50.);
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. })));
        assert_eq!(engine.venue.receipts, 0);
        assert_eq!(
            engine.venue.failures,
            [(Error::TimedOut, Some(crate::Outcome::NotSent))]
        );
    }

    #[test]
    fn physical_window_tighter_timeout_survives_signed_bind() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            ..Probe::default()
        });
        let token = tokens[0];
        narrow_physical_window(&mut engine, token, 50.);
        Cx {
            now: 0.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .request_timeout(token.attempt().request, 20);
        let actions = route(&mut engine, token, 1.);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Sign { timeout_ms: 19, .. }))
        );
        let (attempt, input) = work(&actions);
        let actions = signed(&mut engine, 10., attempt, input);
        bound(&actions);
        assert!(actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { request, .. } if request.timeout_ms == Some(10) && request.url == "https://venue.test/read?stamp=1" && request.body.as_deref() == Some("{}")))));
    }

    #[test]
    fn physical_window_caps_sign_and_bind_timeouts_without_changing_signed_payload() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            ..Probe::default()
        });
        let token = tokens[0];
        narrow_physical_window(&mut engine, token, 50.);
        let actions = route(&mut engine, token, 10.);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Sign { timeout_ms: 40, .. }))
        );
        let (attempt, input) = work(&actions);
        let actions = signed(&mut engine, 30., attempt, input);
        bound(&actions);
        assert!(actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { request, .. } if request.timeout_ms == Some(20) && request.url == "https://venue.test/read?stamp=10" && request.body.as_deref() == Some("{}")))));
    }

    #[test]
    fn physical_window_retires_parked_provider_at_exact_cap_without_bind() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            ..Probe::default()
        });
        let token = tokens[0];
        narrow_physical_window(&mut engine, token, 50.);
        let (attempt, input) = work(&route(&mut engine, token, 10.));
        let actions = signed(&mut engine, 50., attempt, input);
        assert!(!actions.iter().any(
            |a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. }))
        ));
        assert_eq!(
            engine.venue.failures,
            [(Error::TimedOut, Some(crate::Outcome::NotSent))]
        );
        assert!(engine.io.managed_http.is_empty());
    }

    #[test]
    fn physical_window_expiry_precedes_provider_refusal_and_stale_revision() {
        for stale in [false, true] {
            let (mut engine, tokens) = start(Probe {
                count: 1,
                signed: true,
                ..Probe::default()
            });
            let token = tokens[0];
            narrow_physical_window(&mut engine, token, 50.);
            let (attempt, _) = work(&route(&mut engine, token, 10.));
            if stale {
                assert!(engine.io.protocol_clock.observe(20., 520.));
            }
            engine.handle(
                50.,
                Event::Signed {
                    attempt,
                    result: Err(crate::signing::SignError::Refused),
                },
            );
            let actions = engine.drain_actions().collect::<Vec<_>>();
            assert_eq!(
                engine.venue.failures,
                [(Error::TimedOut, Some(crate::Outcome::NotSent))]
            );
            assert_eq!(
                engine.venue.failure_messages,
                ["Signing timed out before dispatch"]
            );
            assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. }) || matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
            assert_eq!(engine.venue.releases.len(), 0);
        }
    }

    #[test]
    fn physical_window_expiry_precedes_reprepare_identity_capacity() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            ..Probe::default()
        });
        let token = tokens[0];
        narrow_physical_window(&mut engine, token, 50.);
        let (attempt, input) = work(&route(&mut engine, token, 10.));
        bound(&signed(&mut engine, 11., attempt, input));
        engine
            .io
            .signing
            .set_test_ids(crate::resource_ids::ResourceIds::starting_at(
                crate::resource_ids::MAX_RESOURCE_ID + 1,
            ));
        assert!(!engine.io.signing.available_id());
        assert!(!engine.failed());
        engine.reprepare_http(50., token);
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert_eq!(
            engine.venue.failures,
            [(Error::TimedOut, Some(crate::Outcome::NotSent))]
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
        assert!(!engine.failed());
    }

    #[test]
    fn physical_window_reprepare_retains_cap_and_early_timer_rearms_to_it() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            ..Probe::default()
        });
        let old = tokens[0];
        narrow_physical_window(&mut engine, old, 500.);
        let timer = engine.io.managed_http[&old.attempt().request].timer;
        let (attempt, input) = work(&route(&mut engine, old, 10.));
        let permit = bound(&signed(&mut engine, 11., attempt, input));
        assert!(engine.io.protocol_clock.observe(12., 512.));
        assert_eq!(engine.claim_http(12., permit), Err(Error::Stale));
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { request, preparation, .. } if request.timeout_ms == Some(488) && *preparation != old))));
        engine.handle(20., Event::TimerFired { timer });
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { timer: id, at_ms: 500. } if *id == timer))
        );
        assert!(engine.venue.failures.is_empty());
    }

    #[test]
    fn physical_window_write_ack_at_cap_is_unknown_before_ordinary_codec() {
        for split in [false, true] {
            for at in [49., 50.] {
                let mut engine = crate::test_support::seeded_engine(Probe::default());
                let (_, token, timer) = mutation(&mut engine, 610, 0.);
                narrow_physical_window(&mut engine, token, 50.);
                let request = token.attempt().request;
                let permit = bind(&mut engine, token, 1.);
                assert!(matches!(
                    engine.claim_http(1., permit),
                    Ok(Claim::Released(_))
                ));
                assert!(engine.io.managed_response_live(request, token, 49.));
                assert!(!engine.io.managed_response_live(request, token, 50.));
                engine.drain_actions().for_each(drop);
                if split {
                    engine.handle(
                        10.,
                        Event::HttpStart {
                            request,
                            status: 200,
                            headers: "",
                        },
                    );
                    engine.handle(
                        11.,
                        Event::HttpChunk {
                            request,
                            data: b"o",
                        },
                    );
                    assert_eq!(engine.venue.responses, 0);
                }
                engine.handle(25., Event::TimerFired { timer });
                let actions = engine.drain_actions().collect::<Vec<_>>();
                assert!(actions.iter().any(
                    |a| matches!(a, Action::SetTimer { timer: id, at_ms: 50. } if *id == timer)
                ));
                if split {
                    engine.handle(
                        at,
                        Event::HttpChunk {
                            request,
                            data: b"k",
                        },
                    );
                    assert_eq!(engine.venue.responses, 0);
                    engine.handle(at, Event::HttpEnd { request });
                } else {
                    engine.handle(
                        at,
                        Event::HttpResponse {
                            request,
                            response: Response {
                                status: 200,
                                headers: "",
                                body: "ok",
                            },
                        },
                    );
                }
                let actions = engine.drain_actions().collect::<Vec<_>>();
                if at < 50. {
                    assert_eq!(engine.venue.responses, 1);
                    assert!(actions.iter().any(|a| matches!(
                        a,
                        Action::Notice(Notice::CommandDone { command: 610, .. })
                    )));
                } else {
                    assert_eq!(engine.venue.responses, 0);
                    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 610, error }) if error.outcome == Some(crate::Outcome::Unknown))));
                    assert_eq!(engine.venue.reconciliations.borrow().as_slice(), &["ok"]);
                }
            }
        }
    }

    #[test]
    fn signed_reads_wait_for_routes_and_parked_provider_does_not_block_independent_ready_http() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            signed: true,
            ..Probe::default()
        });
        assert!(engine.io.signing.next_work().is_none());
        // Later route/provider can complete first; neither consumes admission.
        let (second, second_work) = work(&route(&mut engine, tokens[1], 100.));
        let (first, first_work) = work(&route(&mut engine, tokens[0], 101.));
        assert_eq!(first_work.receipt.stamp_ms, Some(101.));
        let second_packet = signed(&mut engine, 700., second, second_work);
        let second_permit = bound(&second_packet);
        assert_eq!(engine.venue().receipts, 0);
        assert!(matches!(
            engine.claim_http(700., second_permit),
            Ok(Claim::Released(_))
        ));
        assert_eq!(engine.venue().receipts, 1);
        response(&mut engine, tokens[1], 700.);
        let first_packet = signed(&mut engine, 701., first, first_work);
        let first_permit = bound(&first_packet);
        assert_eq!(engine.claim_http(701., first_permit), Err(Error::TimedOut));
        // The original 1000ms bound wins over a 1150ms pacing wake.
        assert_eq!(engine.venue().releases.len(), 1);
        assert_eq!(
            engine.venue().failures,
            [(Error::TimedOut, Some(crate::Outcome::NotSent))]
        );
    }

    #[test]
    fn calibration_change_reprepares_with_new_provider_id_and_same_original_deadline() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            ..Probe::default()
        });
        let old = tokens[0];
        let (attempt, input) = work(&route(&mut engine, old, 100.));
        let permit = bound(&signed(&mut engine, 101., attempt, input.clone()));
        assert!(engine.io.protocol_clock.observe(102., 602.));
        assert_eq!(engine.claim_http(102., permit), Err(Error::Stale));
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Cancel { preparation, signing: Some(id) } if *preparation == old && *id == attempt))));
        let next = actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        assert_eq!(request.timeout_ms, Some(898));
                        Some(*preparation)
                    }
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        assert_ne!(old, next);
        assert!(signed(&mut engine, 103., attempt, input).iter().all(|a| !matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. }))));
        let (new_attempt, input) = work(&route(&mut engine, next, 104.));
        assert_ne!(new_attempt, attempt);
        assert_eq!(input.receipt.stamp_ms, Some(604.));
        let permit = bound(&signed(&mut engine, 105., new_attempt, input));
        let Ok(Claim::Released(release)) = engine.claim_http(106., permit) else {
            panic!("fresh release");
        };
        assert_eq!(release.time.raw_ms, 106.);
        assert_eq!(release.time.protocol_ms, 606.);
        assert_eq!(release.signature.stamp_ms, Some(604.));
        assert_eq!(release.signature_valid_until, Some(1000.));
        assert_eq!(engine.venue().receipts, 1);
    }

    #[test]
    fn unsolicited_provider_failure_before_route_preparation_cannot_retire_a_managed_read() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            ..Probe::default()
        });
        let attempt = engine
            .io
            .managed_http
            .get(&tokens[0].attempt().request)
            .unwrap()
            .signing_attempt
            .unwrap();
        assert!(!engine.signing_pending(attempt));
        engine.handle(
            1.,
            Event::Signed {
                attempt,
                result: Err(crate::signing::SignError::Refused),
            },
        );
        assert!(engine.http_preparation_pending(tokens[0]));
        assert!(engine.venue().failures.is_empty());
        let (id, input) = work(&route(&mut engine, tokens[0], 2.));
        assert_eq!(id, attempt);
        assert!(engine.signing_pending(id));
        let permit = bound(&signed(&mut engine, 3., id, input));
        assert!(!engine.signing_pending(id));
        assert!(matches!(
            engine.claim_http(4., permit),
            Ok(Claim::Released(_))
        ));
    }

    #[test]
    fn cancelled_hung_signed_read_emits_provider_cancellation_and_late_success_cannot_escape() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            signed: true,
            command: Some(7),
            ..Probe::default()
        });
        let (attempt, input) = work(&route(&mut engine, tokens[0], 10.));
        // Completion is an existing authority's operation, not a duplicate
        // wire command borrowing its numeric ID.
        let context = engine
            .io
            .managed_http
            .get(&tokens[0].attempt().request)
            .unwrap()
            .delivery()
            .context;
        Cx {
            now: 11.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .done_context(&context, "null".into())
        .unwrap();
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Cancel { signing: Some(id), .. } if *id == attempt))));
        assert!(engine.io.managed_http.is_empty());
        assert!(engine.io.signing.next_work().is_none());
        let late = signed(&mut engine, 12., attempt, input);
        assert!(
            late.iter()
                .all(|a| !matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
        assert_eq!(engine.venue().receipts, 0);
        assert_eq!(engine.venue().responses, 0);
    }

    fn start(probe: Probe) -> (Engine<Probe>, Vec<Preparation>) {
        let mut engine = crate::test_support::seeded_engine(probe);
        engine.start(0.);
        let tokens = engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        (engine, tokens)
    }
    fn bind(engine: &mut Engine<Probe>, token: Preparation, at: f64) -> Permit {
        engine.http_prepared(
            at,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        engine
            .drain_actions()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .unwrap()
    }

    /// Use the public queue API; inspect only the resulting timer/token.
    fn mutation(
        engine: &mut Engine<Probe>,
        command: CommandId,
        at: f64,
    ) -> (Context, Preparation, TimerId) {
        queue_mutation(engine, command, at, false)
    }

    fn queue_mutation(
        engine: &mut Engine<Probe>,
        command: CommandId,
        at: f64,
        signed: bool,
    ) -> (Context, Preparation, TimerId) {
        let retry = if engine.venue.retry_refusal {
            RetryPolicy::DefinitiveRefusal
        } else {
            RetryPolicy::Never
        };
        // Issued as if dispatched from a placement command (or the chosen intent).
        let intent = engine
            .venue
            .mutation_intent
            .unwrap_or(crate::classify::Intent::Place);
        engine.io.request_contexts.dispatch = Some((command, intent));
        let mut cx = Cx {
            now: at,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        let context = cx
            .http_context(
                Scope::Command(command),
                Policy {
                    operation: Operation::Mutation,
                    credential: signed.then(|| crate::request_context::Credential {
                        slot: CredentialSlot::Primary,
                        reference: crate::signing::KeyRef("fixture".into()),
                    }),
                    timeout_ms: 100,
                    pacing: None,
                    retry,
                },
            )
            .unwrap();
        cx.io.request_contexts.dispatch = None;
        let request = if signed {
            cx.request_signed_managed(
                context.clone(),
                None,
                Method::Post,
                "https://venue.test/write".into(),
                Some("{}".into()),
                Box::new(Recipe),
                Pending(context.clone()),
            )
        } else {
            cx.request_managed(
                context.clone(),
                None,
                Method::Post,
                "https://venue.test/write".into(),
                Some("{}".into()),
                Pending(context.clone()),
            )
        }
        .expect("public managed mutation must queue");
        let managed = cx.io.managed_http.get(&request).unwrap();
        (context, managed.lifecycle.preparation(), managed.timer)
    }

    fn fixture_order(id: &str) -> crate::model::Order {
        use crate::model::*;
        Order {
            id: id.into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "X".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 1.,
            amount: 1.,
            filled: 0.,
            remaining: 1.,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }
    }

    fn fail_mutation(engine: &mut Engine<Probe>, context: &Context, at: f64) {
        Cx {
            now: at,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .fail_context(context, "original command deadline")
        .unwrap();
    }

    #[test]
    fn retired_mutation_retains_mid_body_and_reconciles_once_without_reopening_command() {
        let (mut engine, _) = start(Probe::default());
        engine
            .store
            .set_orders(vec![fixture_order("one"), fixture_order("two")]);
        let (context, token, timer) = mutation(&mut engine, 7, 0.);
        let permit = bind(&mut engine, token, 1.);
        assert!(matches!(
            engine.claim_http(2., permit),
            Ok(Claim::Released(_))
        ));
        engine.handle(
            3.,
            Event::HttpStart {
                request: token.attempt().request,
                status: 200,
                headers: "header",
            },
        );
        engine.handle(
            4.,
            Event::HttpChunk {
                request: token.attempt().request,
                data: b"o",
            },
        );
        fail_mutation(&mut engine, &context, 5.);
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(crate::Outcome::Unknown))).count(), 1);
        assert!(engine.io.pending.is_empty());
        assert!(engine.io.managed_http.is_empty());
        assert!(engine.io.incoming.is_empty());
        engine.handle(
            6.,
            Event::HttpChunk {
                request: token.attempt().request,
                data: b"k",
            },
        );
        engine.handle(
            7.,
            Event::HttpEnd {
                request: token.attempt().request,
            },
        );
        engine.handle(
            8.,
            Event::HttpEnd {
                request: token.attempt().request,
            },
        );
        engine.handle(101., Event::TimerFired { timer });
        assert_eq!(&*engine.venue.reconciliations.borrow(), &["ok"]);
        assert_eq!(
            engine.store.order("one").unwrap().status,
            crate::model::OrderStatus::Canceled
        );
        assert_eq!(
            engine.store.order("two").unwrap().status,
            crate::model::OrderStatus::Canceled
        );
        assert_eq!(engine.venue.responses, 0);
        assert!(engine.drain_actions().all(|a| !matches!(
            a,
            Action::Http(_) | Action::Notice(_) | Action::SetTimer { .. }
        )));
        assert!(!engine.io.http_responses.contains(token.attempt().request));
    }

    #[test]
    fn newer_order_or_physical_write_fences_late_reconciliation() {
        for newer_write in [false, true] {
            let (mut engine, _) = start(Probe::default());
            engine
                .store
                .set_orders(vec![fixture_order("one"), fixture_order("two")]);
            let (context, token, _) = mutation(&mut engine, 7, 0.);
            let permit = bind(&mut engine, token, 1.);
            engine.claim_http(2., permit).unwrap();
            fail_mutation(&mut engine, &context, 3.);
            if newer_write {
                let (_, replacement, _) = mutation(&mut engine, 7, 4.);
                let permit = bind(&mut engine, replacement, 5.);
                engine.claim_http(6., permit).unwrap();
            } else {
                engine.store.upsert_order(fixture_order("unrelated"));
            }
            engine.drain_actions().for_each(drop);
            engine.handle(
                7.,
                Event::HttpResponse {
                    request: token.attempt().request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: "ok",
                    },
                },
            );
            assert_eq!(&*engine.venue.reconciliations.borrow(), &["ok"]);
            assert_eq!(
                engine.store.order("one").unwrap().status,
                crate::model::OrderStatus::Open
            );
            assert_eq!(
                engine.store.order("two").unwrap().status,
                crate::model::OrderStatus::Open
            );
            assert_eq!(engine.venue.responses, 0);
            assert!(engine.drain_actions().next().is_none());
        }
    }

    #[test]
    fn response_proof_guard_precedes_store_capable_codec_for_whole_and_split_replies() {
        let mut observed = Vec::new();
        for status in [408, 500, 503, 600, u16::MAX, 200] {
            for split in [false, true] {
                let (mut engine, _) = start(Probe {
                    hostile_response_patch: true,
                    ..Probe::default()
                });
                engine.store.set_orders(vec![fixture_order("one")]);
                let (_, token, _) = mutation(&mut engine, 7, 0.);
                let permit = bind(&mut engine, token, 1.);
                assert!(matches!(
                    engine.claim_http(2., permit),
                    Ok(Claim::Released(_))
                ));
                engine.drain_actions().for_each(drop);
                let request = token.attempt().request;
                if split {
                    engine.handle(
                        3.,
                        Event::HttpStart {
                            request,
                            status,
                            headers: "",
                        },
                    );
                    engine.handle(
                        4.,
                        Event::HttpChunk {
                            request,
                            data: b"o",
                        },
                    );
                    engine.handle(
                        5.,
                        Event::HttpChunk {
                            request,
                            data: b"k",
                        },
                    );
                    engine.handle(6., Event::HttpEnd { request });
                } else {
                    engine.handle(
                        6.,
                        Event::HttpResponse {
                            request,
                            response: Response {
                                status,
                                headers: "",
                                body: "ok",
                            },
                        },
                    );
                }
                let actions = engine.drain_actions().collect::<Vec<_>>();
                let unknown = actions
                    .iter()
                    .filter(|a| {
                        matches!(a,
                    Action::Notice(Notice::CommandFailed { command: 7, error })
                    if error.outcome == Some(crate::Outcome::Unknown))
                    })
                    .count();
                let done = actions
                    .iter()
                    .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
                    .count();
                observed.push((
                    status,
                    split,
                    engine.store.order("one").unwrap().status,
                    engine.venue.responses,
                    unknown,
                    done,
                ));
                assert!(engine.io.pending.is_empty());
                assert!(engine.io.managed_http.is_empty());
            }
        }
        let mut expected = Vec::new();
        for status in [408, 500, 503, 600, u16::MAX] {
            for split in [false, true] {
                expected.push((status, split, crate::model::OrderStatus::Open, 0, 1, 0));
            }
        }
        for split in [false, true] {
            expected.push((200, split, crate::model::OrderStatus::Canceled, 1, 0, 1));
        }
        assert_eq!(
            observed, expected,
            "unproven status must be intercepted before any Store-capable native callback"
        );
    }

    #[test]
    fn response_proof_guard_keeps_late_unproven_replies_out_of_reconciliation() {
        let mut observed = Vec::new();
        for status in [408, 500, 503, 600, u16::MAX, 200] {
            let (mut engine, _) = start(Probe::default());
            engine
                .store
                .set_orders(vec![fixture_order("one"), fixture_order("two")]);
            let (context, token, timer) = mutation(&mut engine, 7, 0.);
            let permit = bind(&mut engine, token, 1.);
            assert!(matches!(
                engine.claim_http(2., permit),
                Ok(Claim::Released(_))
            ));
            fail_mutation(&mut engine, &context, 3.);
            let actions = engine.drain_actions().collect::<Vec<_>>();
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command: 7, error })
                if error.outcome == Some(crate::Outcome::Unknown)))
                    .count(),
                1
            );
            engine.handle(
                4.,
                Event::HttpResponse {
                    request: token.attempt().request,
                    response: Response {
                        status,
                        headers: "",
                        body: "ok",
                    },
                },
            );
            engine.handle(101., Event::TimerFired { timer });
            observed.push((
                status,
                engine.store.order("one").unwrap().status,
                engine.store.order("two").unwrap().status,
                engine.venue.reconciliations.borrow().len(),
            ));
            assert_eq!(engine.venue.responses, 0);
            assert!(engine.venue.evidence_owners.borrow().is_empty());
            assert!(engine.venue.discarded_owners.is_empty());
            assert!(engine.drain_actions().next().is_none());
            assert!(engine.io.pending.is_empty());
            assert!(engine.io.managed_http.is_empty());
        }
        let mut expected = [408, 500, 503, 600, u16::MAX]
            .map(|status| {
                (
                    status,
                    crate::model::OrderStatus::Open,
                    crate::model::OrderStatus::Open,
                    0,
                )
            })
            .to_vec();
        expected.push((
            200,
            crate::model::OrderStatus::Canceled,
            crate::model::OrderStatus::Canceled,
            1,
        ));
        assert_eq!(
            observed, expected,
            "retired unproven response cannot gain reconciliation Store authority"
        );
    }

    #[test]
    fn response_proof_capture_is_once_and_never_authentication_authority() {
        for status in [408, 503, 200] {
            let (mut engine, _) = start(Probe {
                capture_auth_details: true,
                ..Probe::default()
            });
            let (context, token, _) = mutation(&mut engine, 7, 0.);
            Cx {
                now: 0.,
                store: &mut engine.store,
                io: &mut engine.io,
            }
            .tag_account_read(
                token.attempt().request,
                CredentialSlot::Primary,
                AccountReadScope::Orders,
            );
            let permit = bind(&mut engine, token, 1.);
            engine.claim_http(2., permit).unwrap();
            engine.drain_actions().for_each(drop);
            for at in [3., 4.] {
                engine.handle(
                    at,
                    Event::HttpResponse {
                        request: token.attempt().request,
                        response: Response {
                            status,
                            headers: "",
                            body: "authentication refused",
                        },
                    },
                );
            }
            let actions = engine.drain_actions().collect::<Vec<_>>();
            if status == 200 {
                assert!(engine.venue.evidence_owners.borrow().is_empty());
                assert!(engine.venue.discarded_owners.is_empty());
                assert_eq!(engine.venue.auth_observations.get(), 1);
                assert_eq!(engine.venue.responses, 1);
                assert_eq!(
                    actions
                        .iter()
                        .filter(|action| matches!(
                            action,
                            Action::Notice(Notice::AccountAuth { .. })
                        ))
                        .count(),
                    1
                );
            } else {
                assert_eq!(*engine.venue.evidence_owners.borrow(), [context.owner()]);
                assert_eq!(engine.venue.discarded_owners, [context.owner()]);
                assert_eq!(engine.venue.auth_observations.get(), 0);
                assert_eq!(engine.health().auth_failures, 0);
                assert_eq!(engine.venue.responses, 0);
                assert!(!actions.iter().any(|action| matches!(
                    action,
                    Action::Notice(Notice::AccountAuth { .. } | Notice::CommandDone { .. })
                )));
                let errors: Vec<_> = actions
                    .iter()
                    .filter_map(|action| match action {
                        Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                        _ => None,
                    })
                    .collect();
                assert_eq!(errors.len(), 1);
                let error = errors[0];
                assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
                assert_eq!(
                    error.kind,
                    Some(if status == 408 {
                        ErrorKind::Timeout
                    } else {
                        ErrorKind::VenueUnavailable
                    })
                );
                assert_eq!(error.venue_code.as_deref(), Some("10004"));
                assert_eq!(
                    error.venue_message.as_deref(),
                    Some("authentication refused")
                );
                let partial = error.partial.as_ref().unwrap();
                assert_eq!(partial.order_ids, ["native-evidence"]);
                assert_eq!(partial.total, None);
                assert_eq!(partial.failed_index, None);
            }
            assert!(engine.io.http_delivery.is_none());
            assert!(engine.io.pending.is_empty());
            assert!(engine.io.managed_http.is_empty());
        }
    }

    #[test]
    fn unproven_seal_keeps_prior_facts_and_retires_siblings_without_touching_reused_owner() {
        for partial in [false, true] {
            let (mut engine, _) = start(Probe {
                hold_write_fact: Some(partial),
                capture_auth_details: true,
                ..Probe::default()
            });
            let (original, prior, _) = mutation(&mut engine, 7, 0.);
            let permit = bind(&mut engine, prior, 1.);
            engine.claim_http(2., permit).unwrap();
            engine.handle(
                3.,
                Event::HttpResponse {
                    request: prior.attempt().request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: "ok",
                    },
                },
            );
            let (_, trigger, _) = mutation(&mut engine, 7, 4.);
            let permit = bind(&mut engine, trigger, 5.);
            engine.claim_http(6., permit).unwrap();
            let (_, sibling, _) = mutation(&mut engine, 7, 7.);
            let permit = bind(&mut engine, sibling, 8.);
            engine.claim_http(9., permit).unwrap();
            let (_, queued, _) = mutation(&mut engine, 7, 10.);
            let (_, parked, _) = queue_mutation(&mut engine, 7, 11., true);
            let (attempt, signing_work) = work(&route(&mut engine, parked, 12.));
            engine.drain_actions().for_each(drop);
            engine.handle(
                13.,
                Event::HttpResponse {
                    request: trigger.attempt().request,
                    response: Response {
                        status: 503,
                        headers: "",
                        body: "unproven",
                    },
                },
            );
            let actions = engine.drain_actions().collect::<Vec<_>>();
            let errors: Vec<_> = actions
                .iter()
                .filter_map(|action| match action {
                    Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].outcome, Some(crate::Outcome::Unknown));
            let ids = &errors[0].partial.as_ref().unwrap().order_ids;
            assert_eq!(ids.len(), 2);
            assert!(ids.iter().any(|id| id == "prior-native"));
            assert!(ids.iter().any(|id| id == "native-evidence"));
            assert_eq!(*engine.venue.evidence_owners.borrow(), [original.owner()]);
            assert_eq!(engine.venue.discarded_owners, [original.owner()]);
            for token in [queued, parked] {
                assert!(actions.iter().any(|action| matches!(action,
                    Action::ManagedHttp(action) if matches!(action.as_ref(),
                    HttpAction::Cancel { preparation, .. } if *preparation == token))));
            }
            assert!(engine.io.pending.is_empty());
            assert!(engine.io.managed_http.is_empty());
            assert_eq!(engine.venue.releases.len(), 3);
            // Reusing the numeric command gives a new owner generation. Old
            // responses and provider completions cannot discard or seal it.
            let (current, fresh, _) = mutation(&mut engine, 7, 14.);
            assert_ne!(current.owner(), original.owner());
            let _ = signed(&mut engine, 15., attempt, signing_work);
            engine.handle(
                16.,
                Event::HttpResponse {
                    request: sibling.attempt().request,
                    response: Response {
                        status: 503,
                        headers: "",
                        body: "late",
                    },
                },
            );
            assert_eq!(*engine.venue.evidence_owners.borrow(), [original.owner()]);
            assert_eq!(engine.venue.discarded_owners, [original.owner()]);
            assert!(engine.venue.reconciliations.borrow().is_empty());
            assert_eq!(engine.io.managed_http.len(), 1);
            assert!(
                engine
                    .io
                    .managed_http
                    .contains_key(&fresh.attempt().request)
            );
            assert!(!engine.drain_actions().any(|action| matches!(
                action,
                Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
            )));
            fail_mutation(&mut engine, &current, 17.);
        }
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn unproven_seal_rejects_deferred_uncertain_cancel_once_without_native_codec() {
        let (mut engine, _) = start(Probe::default());
        engine
            .io
            .order_locks
            .acquire(
                &Command::PlaceOrders {
                    command: 7,
                    orders: vec![],
                },
                &engine.store,
                true,
            )
            .unwrap();
        assert!(engine.io.order_locks.defer(
            &Command::CancelSymbolOrders {
                command: 8,
                symbol: String::new(),
            },
            None
        ));
        let (_, token, _) = mutation(&mut engine, 7, 0.);
        let permit = bind(&mut engine, token, 1.);
        engine.claim_http(2., permit).unwrap();
        for at in [3., 4.] {
            engine.handle(
                at,
                Event::HttpResponse {
                    request: token.attempt().request,
                    response: Response {
                        status: 503,
                        headers: "",
                        body: "unproven",
                    },
                },
            );
        }
        assert!(engine.venue.command_calls.is_empty());
        assert_eq!(engine.venue.responses, 0);
        assert!(engine.io.http_delivery.is_none());
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action,
            Action::Notice(Notice::CommandFailed { command: 7, error })
            if error.outcome == Some(crate::Outcome::Unknown)))
                .count(),
            1
        );
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action,
            Action::Notice(Notice::CommandFailed { command: 8, error })
            if error.message == "Placement outcome is uncertain; reconcile before cancel-all"))
                .count(),
            1
        );
    }

    #[test]
    fn timer_before_response_is_unknown_and_response_before_timer_succeeds_once() {
        for timer_first in [true, false] {
            let (mut engine, _) = start(Probe::default());
            let (_, token, timer) = mutation(&mut engine, 7, 0.);
            let permit = bind(&mut engine, token, 1.);
            engine.claim_http(2., permit).unwrap();
            if timer_first {
                engine.handle(100., Event::TimerFired { timer });
            }
            engine.handle(
                if timer_first { 101. } else { 3. },
                Event::HttpResponse {
                    request: token.attempt().request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: "ok",
                    },
                },
            );
            engine.handle(102., Event::TimerFired { timer });
            let actions = engine.drain_actions().collect::<Vec<_>>();
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(
                        a,
                        Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
                    ))
                    .count(),
                1
            );
            assert_eq!(engine.venue.responses, usize::from(!timer_first));
            assert_eq!(
                engine.venue.reconciliations.borrow().len(),
                usize::from(timer_first)
            );
            assert!(engine.io.pending.is_empty());
            assert!(engine.io.managed_http.is_empty());
        }
    }

    #[test]
    fn mutation_capture_and_streaming_refusals_precede_admission_and_physical_send() {
        for refuse_capture in [false, true] {
            let (mut engine, _) = start(Probe {
                refuse_capture,
                ..Probe::default()
            });
            let (_, token, _) = mutation(&mut engine, 7, 0.);
            let permit = bind(&mut engine, token, 1.);
            if !refuse_capture {
                engine.io.streams.insert(
                    token.attempt().request,
                    (ArrayAt::Root, ItemCheck::Splitter),
                );
            }
            assert_eq!(engine.claim_http(2., permit), Err(Error::Invalid));
            assert!(engine.venue.releases.is_empty());
            assert!(!engine.io.http_responses.contains(token.attempt().request));
            assert!(engine.drain_actions().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.outcome == Some(crate::Outcome::NotSent))));
        }
    }

    #[test]
    fn active_mutation_resets_auth_scope_and_counts_http_status_but_late_ack_does_neither() {
        let (mut engine, _) = start(Probe {
            auth_reject: true,
            ..Probe::default()
        });
        for (command, status) in [(7, 429), (8, 503)] {
            // A native auth refusal is credential evidence only on a signed write.
            let (_, token, _) = queue_mutation(&mut engine, command, 0., true);
            let (attempt, input) = work(&route(&mut engine, token, 1.));
            let permit = bound(&signed(&mut engine, 1., attempt, input));
            engine.claim_http(2., permit).unwrap();
            engine.io.auth_failure_counted = true;
            #[cfg(feature = "order-locks")]
            {
                engine.io.order_response_token = Some(123);
            }
            engine.handle(
                3.,
                Event::HttpResponse {
                    request: token.attempt().request,
                    response: Response {
                        status,
                        headers: "",
                        body: "denied",
                    },
                },
            );
            assert_eq!(engine.health().auth_failures, 1);
            if status == 503 {
                assert!(!engine.io.auth_failure_counted);
                let actions = engine.drain_actions().collect::<Vec<_>>();
                assert!(actions.iter().any(|action| matches!(action,
                    Action::Notice(Notice::CommandFailed { command: 8, error })
                    if error.kind == Some(ErrorKind::VenueUnavailable)
                        && error.outcome == Some(crate::Outcome::Unknown))));
            }
        }
        assert_eq!(engine.health().auth_failures, 1);
        assert_eq!(engine.health().rate_limited, 1);
        assert_eq!(engine.health().http_failures, 1);
        let (context, token, _) = mutation(&mut engine, 9, 4.);
        let permit = bind(&mut engine, token, 5.);
        engine.claim_http(6., permit).unwrap();
        fail_mutation(&mut engine, &context, 7.);
        engine.drain_actions().for_each(drop);
        let health = engine.health().clone();
        engine.handle(
            8.,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 429,
                    headers: "",
                    body: "denied",
                },
            },
        );
        assert_eq!(engine.health(), &health);
        assert_eq!(engine.venue.responses, 1);
        assert_eq!(engine.venue.reconciliations.borrow().len(), 1);
        assert!(engine.drain_actions().next().is_none());
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn active_mutation_completion_drains_deferred_order_locks_without_an_unrelated_event() {
        let (mut engine, _) = start(Probe::default());
        let placing = Command::PlaceOrders {
            command: 7,
            orders: vec![],
        };
        engine
            .io
            .order_locks
            .acquire(&placing, &engine.store, true)
            .unwrap();
        assert!(engine.io.order_locks.defer(
            &Command::CancelSymbolOrders {
                command: 8,
                symbol: String::new()
            },
            None
        ));
        let (_, token, _) = mutation(&mut engine, 7, 0.);
        let permit = bind(&mut engine, token, 1.);
        engine.claim_http(2., permit).unwrap();
        engine.handle(
            3.,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "ok",
                },
            },
        );
        assert_eq!(engine.venue.command_calls, vec![8]);
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn managed_read_callback_cannot_suppress_an_engine_owned_deferred_uncertainty_failure() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            command: Some(9),
            ..Probe::default()
        });
        engine
            .io
            .order_locks
            .acquire(
                &Command::PlaceOrders {
                    command: 7,
                    orders: vec![],
                },
                &engine.store,
                true,
            )
            .unwrap();
        engine.io.order_locks.note_write();
        assert!(engine.io.order_locks.defer(
            &Command::CancelSymbolOrders {
                command: 8,
                symbol: String::new()
            },
            None
        ));
        engine.io.order_locks.finish(7, true);
        let permit = bind(&mut engine, tokens[0], 1.);
        engine.claim_http(2., permit).unwrap();
        engine.handle(
            3.,
            Event::HttpResponse {
                request: tokens[0].attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "[]",
                },
            },
        );
        assert_eq!(
            engine
                .drain_actions()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, .. })))
                .count(),
            1
        );
        assert_eq!(engine.venue.responses, 1);
        assert!(engine.io.http_delivery.is_none());
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn pre_send_refusal_releases_a_deferred_command_without_waiting_for_an_event() {
        let (mut engine, _) = start(Probe {
            refuse_capture: true,
            ..Probe::default()
        });
        engine
            .io
            .order_locks
            .acquire(
                &Command::PlaceOrders {
                    command: 7,
                    orders: vec![],
                },
                &engine.store,
                true,
            )
            .unwrap();
        assert!(engine.io.order_locks.defer(
            &Command::CancelSymbolOrders {
                command: 8,
                symbol: String::new()
            },
            None
        ));
        let (_, token, _) = mutation(&mut engine, 7, 0.);
        let permit = bind(&mut engine, token, 1.);
        assert_eq!(engine.claim_http(2., permit), Err(Error::Invalid));
        assert_eq!(engine.venue.command_calls, vec![8]);
        assert!(engine.venue.releases.is_empty());
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn managed_lock_uncertainty_uses_the_sealed_journal_not_the_presence_of_an_error() {
        use crate::request_context::WriteResult;
        for state in 0..4 {
            let (mut engine, _) = start(Probe::default());
            engine
                .io
                .order_locks
                .acquire(
                    &Command::PlaceOrders {
                        command: 7,
                        orders: vec![],
                    },
                    &engine.store,
                    true,
                )
                .unwrap();
            let (first, first_token, _) = mutation(&mut engine, 7, 0.);
            if state != 0 {
                let permit = bind(&mut engine, first_token, 1.);
                engine.claim_http(2., permit).unwrap();
            }
            if state == 1 || state == 2 {
                let delivery = engine
                    .io
                    .managed_http
                    .get(&first_token.attempt().request)
                    .unwrap()
                    .delivery();
                engine
                    .io
                    .request_contexts
                    .result(
                        &delivery,
                        if state == 1 {
                            WriteResult::Rejected(VenueError::new("native refusal"))
                        } else {
                            WriteResult::Accepted {
                                order_ids: vec!["accepted".into()],
                            }
                        },
                    )
                    .unwrap();
            }
            if state == 2 {
                let (_, second, _) = mutation(&mut engine, 7, 3.);
                let permit = bind(&mut engine, second, 4.);
                engine.claim_http(5., permit).unwrap();
                let delivery = engine
                    .io
                    .managed_http
                    .get(&second.attempt().request)
                    .unwrap()
                    .delivery();
                engine
                    .io
                    .request_contexts
                    .result(
                        &delivery,
                        WriteResult::Rejected(VenueError::new("native refusal")),
                    )
                    .unwrap();
            }
            // A legacy venue helper is not managed write evidence. In
            // particular it cannot clear the unresolved fourth case's fence.
            Cx {
                now: 6.,
                store: &mut engine.store,
                io: &mut engine.io,
            }
            .placement_resolved(7);
            fail_mutation(&mut engine, &first, 6.);
            assert_eq!(engine.io.order_locks.uncertain(""), state == 3);
            let error = engine
                .drain_actions()
                .find_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                error.outcome,
                [
                    Some(crate::Outcome::NotSent),
                    Some(crate::Outcome::Rejected),
                    None,
                    Some(crate::Outcome::Unknown)
                ][state]
            );
        }
    }

    #[cfg(feature = "order-locks")]
    fn _cx_has_custody(cx: &Cx<'_, Pending>) -> bool {
        cx.io.http_delivery.is_some()
            || cx.io.order_response_token.is_some()
            || cx.io.command_entry.is_some()
    }

    /// Seal `command` (held under `lock`) as an uncertain write at `at`.
    #[cfg(feature = "order-locks")]
    fn seal_uncertain(engine: &mut Engine<Probe>, lock: &Command, at: f64, error: VenueError) {
        let id = lock.completion_id().unwrap();
        engine
            .io
            .order_locks
            .acquire(lock, &engine.store, false)
            .unwrap();
        let (context, token, _) = mutation(engine, id, at);
        let permit = bind(engine, token, at);
        engine.claim_http(at, permit).unwrap();
        Cx {
            now: at,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .fail_context(&context, error)
        .unwrap();
        engine.finish_event_scope(at);
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn unlocked_uncertain_writes_full_read_and_waits_are_bounded() {
        // A managed write the order lock never held still reconciles in full.
        let (mut engine, _) = start(Probe::default());
        let (context, token, _) = mutation(&mut engine, 7, 0.);
        let permit = bind(&mut engine, token, 1.);
        engine.claim_http(1., permit).unwrap();
        Cx {
            now: 6.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .fail_context(&context, VenueError::new("lost"))
        .unwrap();
        engine.finish_event_scope(6.);
        assert_eq!(engine.venue.reconciles, vec![None]);

        // A venue-supplied wait is capped at the engine maximum.
        let (mut engine, _) = start(Probe::default());
        let (context, token, _) = mutation(&mut engine, 7, 0.);
        let permit = bind(&mut engine, token, 1.);
        engine.claim_http(1., permit).unwrap();
        let huge = VenueError::new("slow down")
            .kind(crate::ErrorKind::RateLimited)
            .retry_after_ms(1e12);
        Cx {
            now: 6.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .fail_context(&context, huge)
        .unwrap();
        engine.finish_event_scope(6.);
        let (_, at) = engine.reconcile_timer.unwrap();
        assert_eq!(at, 6. + crate::startup::MAX_RATE_LIMIT_PAUSE_MS);

        // A non-finite seal time is due at the next drain, never wedged.
        engine.io.queue_reconcile(None, f64::NAN);
        engine.finish_event_scope(7.);
        assert_eq!(engine.venue.reconciles, vec![None]);
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn an_absent_order_cancel_fences_and_reconciles_exactly_once() {
        use crate::classify::{
            CodeTable, Intent, Key, Native, NativeCode, Row, SatisfiedIntent, Verdict,
        };
        const GONE: CodeTable = CodeTable {
            rows: &[Row {
                key: Key::Int(110001),
                verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
                only: Some(Intent::Cancel),
            }],
            success: &[],
            unlisted: Verdict::Refused(crate::ErrorKind::Rejected),
            status: &[],
        };
        let native = Native {
            code: NativeCode::Int(110001),
            message: "order not exists",
        };
        for intent in [Intent::Cancel, Intent::Place] {
            let (mut engine, _) = start(Probe {
                mutation_intent: Some(intent),
                ..Probe::default()
            });
            let (context, token, _) = mutation(&mut engine, 7, 0.);
            let permit = bind(&mut engine, token, 1.);
            engine.claim_http(1., permit).unwrap();
            let delivery = engine
                .io
                .managed_http
                .get(&token.attempt().request)
                .unwrap()
                .delivery();
            engine.io.http_delivery = Some(delivery);
            engine.io.http_write_reply = Some((200, String::new()));
            let (verdict, _) = Cx {
                now: 2.,
                store: &mut engine.store,
                io: &mut engine.io,
            }
            .http_write_failure(&context, &GONE, Some(&native), "order not exists", None)
            .unwrap();
            engine.io.http_write_reply = None;
            engine.io.http_delivery = None;
            engine.finish_event_scope(2.);
            if intent != Intent::Cancel {
                // Outside a cancel the absent-order row never applies.
                assert_eq!(verdict, Verdict::Refused(crate::ErrorKind::Rejected));
                assert!(engine.venue.reconciles.is_empty());
                assert!(!engine.io.order_locks.uncertain("BTCUSDT"));
                continue;
            }
            assert_eq!(verdict, Verdict::Satisfied(SatisfiedIntent::Cancel));
            assert_eq!(engine.venue.reconciles, vec![None], "exactly one read");
            assert!(
                engine.io.order_locks.uncertain("BTCUSDT"),
                "fenced, not cancelled"
            );
            engine.finish_event_scope(3.);
            assert_eq!(engine.venue.reconciles.len(), 1, "never asked twice");
            // Only a fresh order read clears the fence.
            engine.io.order_response_token = Some(engine.io.order_locks.token());
            Cx {
                now: 4.,
                store: &mut engine.store,
                io: &mut engine.io,
            }
            .orders_reconciled(None);
            engine.io.order_response_token = None;
            assert!(!engine.io.order_locks.uncertain("BTCUSDT"));
        }
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn an_absent_order_under_a_held_placement_reconciles_after_it_finishes() {
        use crate::classify::{
            CodeTable, Intent, Key, Native, NativeCode, Row, SatisfiedIntent, Verdict,
        };
        const GONE: CodeTable = CodeTable {
            rows: &[Row {
                key: Key::Int(110001),
                verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
                only: Some(Intent::Cancel),
            }],
            success: &[],
            unlisted: Verdict::Refused(crate::ErrorKind::Rejected),
            status: &[],
        };
        // A placement's conflict-cancel leg (classified as Cancel) under its
        // own held placing lock on BTCUSDT.
        let (mut engine, _) = start(Probe {
            mutation_intent: Some(Intent::Cancel),
            ..Probe::default()
        });
        let place = Command::PlaceOrders {
            command: 7,
            orders: vec![
                serde_json::from_str(
                    r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":1}"#,
                )
                .unwrap(),
            ],
        };
        engine
            .io
            .order_locks
            .acquire(&place, &engine.store, false)
            .unwrap();
        let (context, token, _) = mutation(&mut engine, 7, 0.);
        let permit = bind(&mut engine, token, 1.);
        engine.claim_http(1., permit).unwrap();
        let delivery = engine
            .io
            .managed_http
            .get(&token.attempt().request)
            .unwrap()
            .delivery();
        engine.io.http_delivery = Some(delivery);
        engine.io.http_write_reply = Some((200, String::new()));
        let native = Native {
            code: NativeCode::Int(110001),
            message: "order not exists",
        };
        Cx {
            now: 2.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .http_write_failure(&context, &GONE, Some(&native), "order not exists", None)
        .unwrap();
        engine.io.http_write_reply = None;
        engine.io.http_delivery = None;
        engine.finish_event_scope(2.);
        assert!(engine.io.order_locks.uncertain("BTCUSDT"), "fenced at once");
        assert!(
            engine.venue.reconciles.is_empty(),
            "a read now could not clear the held placement's fence"
        );
        Cx {
            now: 3.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .fail_context(&context, "placement finished")
        .unwrap();
        engine.finish_event_scope(3.);
        assert_eq!(
            engine.venue.reconciles,
            vec![Some(vec!["BTCUSDT".to_owned()])],
            "exactly one read once the placement finished"
        );
        engine.io.order_response_token = Some(engine.io.order_locks.token());
        Cx {
            now: 4.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .orders_reconciled(Some("BTCUSDT"));
        engine.io.order_response_token = None;
        assert!(
            !engine.io.order_locks.uncertain("BTCUSDT"),
            "the read clears it"
        );
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn an_overflowing_reconcile_queue_never_cuts_a_wait_short() {
        let btc = || Some(vec!["BTCUSDT".to_owned()]);
        // 64 deferred entries with unequal waits, then one immediate entry.
        for immediate in [true, false] {
            let (mut engine, _) = start(Probe::default());
            for n in 0..64 {
                engine.io.queue_reconcile(btc(), 10_000. + f64::from(n));
            }
            engine
                .io
                .queue_reconcile(btc(), if immediate { 0. } else { 5_000. });
            engine.finish_event_scope(1.);
            assert!(
                engine.venue.reconciles.is_empty(),
                "no hook before the bound"
            );
            let (timer, at) = engine.reconcile_timer.unwrap();
            assert_eq!(at, 10_063., "waits for the latest merged deadline");
            engine.handle(
                at - 1.,
                Event::TimerFired {
                    timer: TimerId(timer.0 + 99),
                },
            );
            assert!(
                engine.venue.reconciles.is_empty(),
                "a stale timer is ignored"
            );
            engine.handle(at, Event::TimerFired { timer });
            assert_eq!(engine.venue.reconciles, vec![None], "exactly one full read");
            assert!(engine.reconcile_timer.is_none());
        }
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn an_uncertain_write_asks_the_venue_once_per_seal_after_any_rate_wait() {
        let place = |command, symbol: &str| Command::PlaceOrders {
            command,
            orders: vec![
                serde_json::from_str(&format!(
                    r#"{{"symbol":"{symbol}","type":"limit","side":"buy","amount":1,"price":1}}"#
                ))
                .unwrap(),
            ],
        };
        // Each uncertain seal asks again, even for an already-uncertain symbol.
        let (mut engine, _) = start(Probe::default());
        seal_uncertain(
            &mut engine,
            &place(7, "BTCUSDT"),
            6.,
            VenueError::new("lost"),
        );
        seal_uncertain(
            &mut engine,
            &place(8, "BTCUSDT"),
            7.,
            VenueError::new("lost"),
        );
        let btc = Some(vec!["BTCUSDT".to_owned()]);
        assert_eq!(engine.venue.reconciles, vec![btc.clone(), btc]);
        assert!(!engine.venue.reconcile_custody);

        // Scope follows the sealed command: routed cancels are symbols,
        // unrouted IDs and an all-symbol cancel are full reads.
        for (lock, expected) in [
            (
                Command::CancelOrdersWithSymbols {
                    command: 9,
                    orders: vec![crate::model::CancelOrderRef {
                        id: "x".into(),
                        symbol: "ETHUSDT".into(),
                    }],
                },
                Some(vec!["ETHUSDT".to_owned()]),
            ),
            (
                Command::CancelOrders {
                    command: 9,
                    ids: vec!["unrouted".into()],
                },
                None,
            ),
            (
                Command::CancelSymbolOrders {
                    command: 9,
                    symbol: String::new(),
                },
                None,
            ),
        ] {
            let (mut engine, _) = start(Probe::default());
            seal_uncertain(&mut engine, &lock, 6., VenueError::new("lost"));
            assert_eq!(engine.venue.reconciles, vec![expected], "{lock:?}");
        }

        // A rate limit waits its Retry-After; a credential refusal the default.
        for (error, wait) in [
            (
                VenueError::new("slow down")
                    .kind(crate::ErrorKind::RateLimited)
                    .retry_after_ms(3_000.),
                3_000.,
            ),
            (
                VenueError::new("bad key").kind(crate::ErrorKind::AuthInvalid),
                crate::classify::DEFAULT_RATE_LIMIT_DEFERRAL_MS,
            ),
        ] {
            let (mut engine, _) = start(Probe::default());
            seal_uncertain(&mut engine, &place(7, "BTCUSDT"), 6., error);
            assert!(engine.venue.reconciles.is_empty(), "waits");
            let (timer, at) = engine.reconcile_timer.unwrap();
            assert_eq!(at, 6. + wait);
            engine.handle(
                at - 1.,
                Event::TimerFired {
                    timer: TimerId(timer.0 + 99),
                },
            );
            assert!(
                engine.venue.reconciles.is_empty(),
                "a stale timer is ignored"
            );
            engine.handle(at, Event::TimerFired { timer });
            assert_eq!(engine.venue.reconciles.len(), 1);
            assert!(engine.reconcile_timer.is_none());
        }

        // A duplicate (stale) seal of a finished command asks nothing more.
        let (mut engine, _) = start(Probe::default());
        seal_uncertain(
            &mut engine,
            &place(7, "BTCUSDT"),
            6.,
            VenueError::new("lost"),
        );
        Cx {
            now: 7.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .fail(7, "late duplicate");
        engine.finish_event_scope(7.);
        assert_eq!(engine.venue.reconciles.len(), 1);
    }

    #[test]
    fn oversize_released_response_seals_unknown_and_cannot_reopen_on_a_late_ack() {
        let (mut engine, _) = start(Probe::default());
        let (_, token, _) = mutation(&mut engine, 7, 0.);
        let permit = bind(&mut engine, token, 1.);
        engine.claim_http(2., permit).unwrap();
        engine.handle(
            3.,
            Event::HttpStart {
                request: token.attempt().request,
                status: 200,
                headers: "",
            },
        );
        engine.handle(
            4.,
            Event::HttpChunk {
                request: token.attempt().request,
                data: &vec![b'x'; 256 * 1024 + 1],
            },
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(crate::Outcome::Unknown) && error.kind == Some(ErrorKind::Internal))).count(), 1);
        assert_eq!(engine.venue.responses, 0);
        assert!(engine.io.pending.is_empty());
        assert!(engine.io.managed_http.is_empty());
        engine.handle(
            5.,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "ok",
                },
            },
        );
        assert_eq!(engine.venue.responses, 0);
        assert!(engine.venue.reconciliations.borrow().is_empty());
        assert!(
            engine
                .drain_actions()
                .all(|a| !matches!(a, Action::Notice(_)))
        );
    }
    fn response(engine: &mut Engine<Probe>, token: Preparation, at: f64) {
        engine.handle(
            at,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "[1]",
                },
            },
        );
    }

    #[test]
    fn preclaim_responses_cannot_reach_streams_auth_or_reducers_and_post_reads_stay_reads() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            ..Probe::default()
        });
        let token = tokens[0];
        response(&mut engine, token, 1.);
        assert_eq!(engine.venue().responses, 0);
        assert_eq!(engine.venue().items, 0);
        let permit = bind(&mut engine, token, 2.);
        assert_eq!(engine.venue().receipts, 0);
        response(&mut engine, token, 3.);
        assert_eq!(engine.venue().responses, 0);
        #[cfg(feature = "order-locks")]
        let before = engine.io.order_locks.token();
        assert!(matches!(
            engine.claim_http(4., permit),
            Ok(Claim::Released(_))
        ));
        #[cfg(feature = "order-locks")]
        assert_eq!(engine.io.order_locks.token(), before);
        assert_eq!(engine.venue().receipts, 1);
        response(&mut engine, token, 5.);
        assert_eq!(engine.venue().responses, 1);
        assert_eq!(engine.venue().items, 1);
        response(&mut engine, token, 6.);
        assert_eq!(engine.venue().responses, 1);
        assert!(engine.io.pending.is_empty());
        assert!(engine.io.managed_http.is_empty());
    }

    #[test]
    fn final_claim_starts_gap_and_repeated_waits_consume_no_admission() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            ..Probe::default()
        });
        let first = bind(&mut engine, tokens[0], 20.);
        let second = bind(&mut engine, tokens[1], 30.);
        assert!(engine.venue().releases.is_empty());
        assert!(matches!(
            engine.claim_http(500., first),
            Ok(Claim::Released(_))
        ));
        for now in [500., 600., 949.] {
            assert_eq!(engine.claim_http(now, second), Ok(Claim::WaitUntil(950.)));
        }
        assert_eq!(engine.venue().releases.len(), 1);
        response(&mut engine, tokens[0], 501.);
        assert!(matches!(
            engine.claim_http(950., second),
            Ok(Claim::Released(_))
        ));
        assert_eq!(engine.venue().releases.len(), 2);
        assert_eq!(engine.claim_http(951., second), Err(Error::Revoked));
    }

    #[test]
    fn denial_and_typed_setup_failure_settle_once_without_response_failure_or_gap() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            deny: true,
            ..Probe::default()
        });
        let first = bind(&mut engine, tokens[0], 1.);
        assert_eq!(engine.claim_http(2., first), Err(Error::Revoked));
        assert_eq!(
            engine.venue().failures,
            [(Error::Revoked, Some(crate::Outcome::NotSent))]
        );
        engine.http_preparation_failed(3., tokens[0], Error::Invalid, VenueError::new("late"));
        assert_eq!(engine.venue().failures.len(), 1);
        engine.venue.deny = false;
        let second = bind(&mut engine, tokens[1], 3.);
        assert!(matches!(
            engine.claim_http(3., second),
            Ok(Claim::Released(_))
        ));
    }

    #[test]
    fn old_physical_timer_cannot_finish_a_new_attempt_with_the_same_logical_context() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            command: Some(7),
            ..Probe::default()
        });
        let old = tokens[0].attempt();
        let context = engine.io.pending.get(&old.request).unwrap().value.0.clone();
        let replacement = {
            let mut cx = Cx {
                now: 1.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            cx.request_managed(
                context.clone(),
                None,
                Method::Post,
                "https://venue.test/read".into(),
                None,
                Pending(context.clone()),
            )
            .unwrap()
        };
        engine.io.discard_managed(old.request);
        engine.drain_actions().for_each(drop);
        let mut cx = Cx {
            now: 2.,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        assert_eq!(
            cx.fail_http_attempt(&context, old, "old timer"),
            Err(Error::Revoked)
        );
        assert!(cx.io.actions.is_empty());
        assert!(cx.io.managed_http.contains_key(&replacement));
        assert!(cx.io.request_contexts.validate(&context, 2.).is_ok());
        assert!(cx.io.http_delivery.is_none());
    }

    #[test]
    fn already_expired_readiness_settles_without_repreparation_or_signing() {
        for signed in [false, true] {
            let (mut engine, tokens) = start(Probe {
                count: 1,
                signed,
                ..Probe::default()
            });
            engine.http_prepared(
                10.,
                tokens[0],
                Prepared {
                    id: 1,
                    expires_at: Some(10.),
                },
            );
            let actions = engine.drain_actions().collect::<Vec<_>>();
            assert!(actions.iter().all(|action| !matches!(action, Action::Sign { .. }) &&
                !matches!(action, Action::ManagedHttp(action) if matches!(action.as_ref(),
                    HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
            assert_eq!(
                engine.venue().failures,
                [(Error::TimedOut, Some(crate::Outcome::NotSent))]
            );
            assert!(engine.venue().releases.is_empty());
            assert!(!engine.http_preparation_pending(tokens[0]));
        }
    }

    #[test]
    fn stale_route_reprepares_without_extending_deadline_and_old_cleanup_cannot_win() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            ..Probe::default()
        });
        let old = tokens[0];
        let old_permit = bind(&mut engine, old, 1.);
        engine.http_preparation_failed(100., old, Error::Stale, VenueError::new("route expired"));
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, Action::ManagedHttp(action)
            if matches!(**action, HttpAction::Cancel {preparation, ..} if preparation == old)))
        );
        let replacement = actions
            .into_iter()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        assert_eq!(request.timeout_ms, Some(900));
                        Some(preparation)
                    }
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        assert_ne!(replacement, old);
        assert!(!engine.http_preparation_pending(old));
        assert!(engine.http_preparation_pending(replacement));
        engine.http_preparation_failed(101., old, Error::Invalid, VenueError::new("old failure"));
        let permit = bind(&mut engine, replacement, 102.);
        assert_eq!(engine.claim_http(103., old_permit), Err(Error::Revoked));
        assert!(matches!(
            engine.claim_http(104., permit),
            Ok(Claim::Released(_))
        ));
        assert!(engine.venue().failures.is_empty());
    }

    #[test]
    fn original_timer_bounds_lost_preparation_and_permit_callbacks() {
        for ready in [false, true] {
            let (mut engine, tokens) = start(Probe {
                count: 1,
                ..Probe::default()
            });
            let token = tokens[0];
            if ready {
                bind(&mut engine, token, 1.);
            }
            let timer = engine
                .io
                .managed_http
                .get(&token.attempt().request)
                .unwrap()
                .timer;
            engine.handle(1_000., Event::TimerFired { timer });
            assert_eq!(
                engine.venue().failures,
                [(Error::TimedOut, Some(crate::Outcome::NotSent))]
            );
            engine.handle(1_001., Event::TimerFired { timer });
            engine.http_prepared(
                1_001.,
                token,
                Prepared {
                    id: 9,
                    expires_at: None,
                },
            );
            response(&mut engine, token, 1_002.);
            assert_eq!(engine.venue().failures.len(), 1);
            assert_eq!(engine.venue().responses, 0);
            assert!(engine.io.pending.is_empty());
            assert!(engine.io.managed_http.is_empty());
        }
    }

    #[test]
    fn duplicate_venue_command_preserves_the_live_managed_owner() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            command: Some(7),
            ..Probe::default()
        });
        engine.handle(
            1.,
            Event::Command(Command::Venue {
                command: 7,
                action: crate::VenueAction::FetchBalanceInfo,
            }),
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::Event(event)) if event.code == crate::log::code::COMMAND_DUPLICATE)));
        assert!(!actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
        )));
        assert!(engine.http_preparation_pending(tokens[0]));
        assert_eq!(engine.io.pending.len(), 1);
    }

    #[test]
    fn command_completion_cancels_unreleased_children_without_a_second_result() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            command: Some(7),
            ..Probe::default()
        });
        let permit = bind(&mut engine, tokens[0], 1.);
        // Completion is an existing authority's operation, not a duplicate
        // wire command borrowing its numeric ID.
        let context = engine
            .io
            .managed_http
            .get(&tokens[0].attempt().request)
            .unwrap()
            .delivery()
            .context;
        Cx {
            now: 2.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .done_context(&context, "null".into())
        .unwrap();
        assert_eq!(engine.claim_http(3., permit), Err(Error::Revoked));
        assert!(!engine.http_preparation_pending(tokens[1]));
        assert!(engine.io.pending.is_empty());
        assert!(engine.io.managed_http.is_empty());
        assert!(engine.venue().failures.is_empty());
        let results = engine
            .drain_actions()
            .filter(|action| {
                matches!(
                    action,
                    Action::Notice(Notice::CommandDone { command: 7, .. })
                )
            })
            .count();
        assert_eq!(results, 1);
    }

    #[test]
    fn retirement_in_first_streamed_row_stops_same_chunk_and_cannot_resurrect_pending() {
        for fail in [false, true] {
            let (mut engine, tokens) = start(Probe {
                count: 1,
                command: Some(7),
                finish_in_item: Some(fail),
                ..Probe::default()
            });
            let permit = bind(&mut engine, tokens[0], 1.);
            assert!(matches!(
                engine.claim_http(2., permit),
                Ok(Claim::Released(_))
            ));
            engine.handle(
                3.,
                Event::HttpResponse {
                    request: tokens[0].attempt().request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: "[1,2]",
                    },
                },
            );
            assert_eq!(engine.venue().items, 1);
            assert_eq!(engine.venue().responses, 0);
            assert!(engine.io.pending.is_empty());
            assert!(engine.io.incoming.is_empty());
            assert!(engine.io.managed_http.is_empty());
            response(&mut engine, tokens[0], 4.);
            assert_eq!(engine.venue().items, 1);
            assert_eq!(engine.venue().responses, 0);
            let terminal = engine
                .drain_actions()
                .filter(|action| {
                    matches!(
                        action,
                        Action::Notice(
                            Notice::CommandDone { command: 7, .. }
                                | Notice::CommandFailed { command: 7, .. }
                        )
                    )
                })
                .count();
            assert_eq!(terminal, 1);
        }
    }

    #[test]
    fn managed_response_order_rejects_early_end_and_duplicate_start_without_losing_pending() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            ..Probe::default()
        });
        let token = tokens[0];
        let request = token.attempt().request;
        let permit = bind(&mut engine, token, 1.);
        assert!(matches!(
            engine.claim_http(2., permit),
            Ok(Claim::Released(_))
        ));
        engine.handle(3., Event::HttpEnd { request });
        assert!(
            engine.io.pending.contains_key(&request),
            "early end must not consume continuation"
        );
        assert!(engine.io.managed_http.contains_key(&request));
        engine.handle(
            4.,
            Event::HttpChunk {
                request,
                data: b"[99]",
            },
        );
        assert_eq!(engine.venue().items, 0);
        engine.handle(
            5.,
            Event::HttpStart {
                request,
                status: 200,
                headers: "first: yes",
            },
        );
        engine.handle(
            6.,
            Event::HttpChunk {
                request,
                data: b"[1,",
            },
        );
        assert_eq!(engine.venue().items, 1);
        engine.handle(
            7.,
            Event::HttpStart {
                request,
                status: 500,
                headers: "duplicate: no",
            },
        );
        engine.handle(
            8.,
            Event::HttpChunk {
                request,
                data: b"2]",
            },
        );
        assert_eq!(engine.venue().items, 2);
        engine.handle(9., Event::HttpEnd { request });
        assert_eq!(engine.venue().responses, 1);
        engine.handle(10., Event::HttpEnd { request });
        assert_eq!(engine.venue().responses, 1);
        assert!(engine.io.pending.is_empty());
        assert!(engine.io.managed_http.is_empty());
    }

    #[test]
    fn managed_mutation_requires_matching_credential_recipe_and_preserves_continuation() {
        for credential in [false, true] {
            let mut io = Io::<Pending>::default();
            let mut store = Store::default();
            let mut cx = Cx {
                now: 0.,
                io: &mut io,
                store: &mut store,
            };
            let context = cx
                .http_context(
                    Scope::Command(7),
                    Policy {
                        operation: Operation::Mutation,
                        credential: credential.then(|| crate::request_context::Credential {
                            slot: CredentialSlot::Primary,
                            reference: crate::signing::KeyRef("fixture".into()),
                        }),
                        timeout_ms: 1_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            let result = if credential {
                cx.request_managed(
                    context.clone(),
                    None,
                    Method::Post,
                    "https://venue.test/write".into(),
                    None,
                    Pending(context.clone()),
                )
            } else {
                cx.request_signed_managed(
                    context.clone(),
                    None,
                    Method::Post,
                    "https://venue.test/write".into(),
                    None,
                    Box::new(Recipe),
                    Pending(context.clone()),
                )
            };
            assert!(
                matches!(result, Err((Pending(returned), Error::Invalid)) if returned == context)
            );
            assert!(cx.io.pending.is_empty());
            assert!(cx.io.managed_http.is_empty());
            assert!(cx.io.actions.is_empty());
            assert!(cx.io.timers.is_empty());
            assert!(cx.io.signing.next_work().is_none());
            cx.fail_context(&context, "recipe mismatch").unwrap();
            assert!(
                matches!(cx.io.actions.last(), Some(Action::Notice(Notice::CommandFailed { error, .. })) if error.outcome == Some(crate::Outcome::NotSent))
            );
        }
    }

    #[test]
    fn prepared_fixture_claims_signed_packets_and_preserves_progress_before_sibling_sends() {
        struct Immediate;
        impl crate::signing::Signer for Immediate {
            fn sign(&self, _: crate::signing::SignRequest) -> crate::signing::SignFuture {
                panic!("immediate fixture cannot park a provider");
            }
            fn immediate(&self) -> bool {
                true
            }
            fn sign_ready(
                &self,
                work: &crate::signing::SignRequest,
            ) -> Option<Result<crate::signing::SignResult, crate::signing::SignError>> {
                Some(Ok(crate::signing::SignResult::new(
                    vec![b"value".to_vec()],
                    work.receipt,
                )))
            }
        }
        // Already queued legacy signing must use the same supplied drain time,
        // rather than the last engine event's stale dispatch clock.
        {
            let mut engine = crate::test_support::seeded_engine(Probe::default());
            engine
                .set_signer(Some(std::sync::Arc::new(Immediate)))
                .unwrap();
            engine.start(0.);
            let mut cx = Cx {
                now: 0.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            let context = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 100,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            let request = cx.request(
                Method::Get,
                "https://venue.test/read".into(),
                vec![],
                None,
                Pending(context),
            );
            cx.sign_http(request, Box::new(Recipe));
            let actions = crate::test_support::prepared_actions(&mut engine, 9.);
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(http) if http.url.ends_with("stamp=9"))),
                "queued legacy signing must use the fixture drain clock"
            );
        }
        for deny in [false, true] {
            let mut engine = crate::test_support::seeded_engine(Probe {
                deny,
                ..Probe::default()
            });
            engine
                .set_signer(Some(std::sync::Arc::new(Immediate)))
                .unwrap();
            engine.start(0.);
            for command in [7, 8] {
                Cx {
                    now: 0.,
                    store: &mut engine.store,
                    io: &mut engine.io,
                }
                .command_progress(
                    command,
                    vec![crate::model::OrderIdentityReceipt {
                        symbol: "X".into(),
                        kind: crate::model::OrderIdentityKind::ClientOrderId,
                        identity: format!("intent-{command}"),
                    }],
                );
                queue_mutation(&mut engine, command, 0., true);
            }
            let actions = crate::test_support::prepared_actions(&mut engine, 1.);
            let released: Vec<_> = actions
                .iter()
                .enumerate()
                .filter_map(|(index, a)| match a {
                    Action::Http(http) => {
                        assert_eq!(
                            http.signer_headers,
                            [("x-credential".into(), "value".into())]
                        );
                        assert!(http.url.ends_with("stamp=1"));
                        Some(index)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(released.len(), if deny { 0 } else { 2 });
            assert!(
                actions
                    .iter()
                    .all(|a| !matches!(a, Action::ManagedHttp(_) | Action::Sign { .. }))
            );
            if !deny {
                let first = actions
                    .iter()
                    .position(|a| {
                        matches!(
                            a,
                            Action::Notice(Notice::CommandProgress { command: 7, .. })
                        )
                    })
                    .unwrap();
                let second = actions
                    .iter()
                    .position(|a| {
                        matches!(
                            a,
                            Action::Notice(Notice::CommandProgress { command: 8, .. })
                        )
                    })
                    .unwrap();
                assert!(first < released[0] && released[0] < second && second < released[1]);
                assert_eq!(engine.venue.receipts, 2);
            } else {
                assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.outcome == Some(crate::Outcome::NotSent))).count(), 2);
                assert_eq!(engine.venue.releases.len(), 2);
                assert_eq!(engine.venue.receipts, 0);
            }
        }
    }

    #[test]
    fn default_venue_without_reconciliation_codec_cannot_release_a_managed_mutation() {
        // ProbeVenue deliberately does not override the trait capture hook.
        let venue = crate::test_support::ProbeVenue::new(Probe::default(), |_, cx| {
            let context = cx
                .http_context(
                    Scope::Command(7),
                    Policy {
                        operation: Operation::Mutation,
                        credential: None,
                        timeout_ms: 100,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_managed(
                context.clone(),
                None,
                Method::Post,
                "https://venue.test/write".into(),
                None,
                Pending(context),
            )
            .unwrap();
        });
        let mut engine = crate::test_support::seeded_engine(venue);
        engine.start(0.);
        let preparation = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        engine.http_prepared(
            1.,
            preparation,
            Prepared {
                id: 1,
                expires_at: None,
            },
        );
        let permit = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        assert_eq!(engine.claim_http(2., permit), Err(Error::Invalid));
        assert!(engine.io.pending.is_empty());
        assert!(engine.io.managed_http.is_empty());
        assert!(
            !engine
                .io
                .http_responses
                .contains(preparation.attempt().request)
        );
        assert!(
            engine
                .drain_actions()
                .all(|a| !matches!(a, Action::Http(_)))
        );
    }

    fn assert_managed_effect_fact(fact: EffectFact, may_publish: bool) {
        let (mut engine, _) = start(Probe {
            effect_fact: Some(fact),
            ..Probe::default()
        });
        let (_, first, _) = queue_mutation(&mut engine, 7, 0., true);
        let (attempt, input) = work(&route(&mut engine, first, 1.));
        let permit = bound(&signed(&mut engine, 2., attempt, input));
        assert!(matches!(
            engine.claim_http(3., permit),
            Ok(Claim::Released(_))
        ));
        engine.handle(
            4.,
            Event::HttpResponse {
                request: first.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "native fixture",
                },
            },
        );
        assert_eq!(engine.venue.responses, 1);
        assert_eq!(
            engine.venue.native_effects,
            usize::from(may_publish),
            "{fact:?}"
        );
        assert_eq!(engine.store.hedged(), may_publish, "{fact:?}");
        assert_eq!(
            engine.venue.housekeeping_checks,
            usize::from(!matches!(fact, EffectFact::Missing | EffectFact::Overflow)),
            "{fact:?}"
        );
        let completions = engine
            .drain_actions()
            .filter(|a| {
                matches!(
                    a,
                    Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(completions.len(), 1);
        if matches!(fact, EffectFact::PartialUnknown) {
            assert!(
                matches!(&completions[0], Action::Notice(Notice::CommandFailed { error, .. })
                if error.outcome == Some(crate::Outcome::Unknown))
            );
        }
    }
    #[test]
    fn postfact_publication_rejects_absent_refused_and_unproven_native_facts() {
        for fact in [
            EffectFact::Missing,
            EffectFact::Rejected,
            EffectFact::Failed,
        ] {
            assert_managed_effect_fact(fact, false);
        }
    }
    #[test]
    fn postfact_publication_rejects_native_identity_overflow_before_effects() {
        assert_managed_effect_fact(EffectFact::Overflow, false);
    }
    #[test]
    fn postfact_publication_accepts_recorded_known_effects_even_without_native_ids() {
        for fact in [
            EffectFact::Accepted,
            EffectFact::KnownPartial,
            EffectFact::PartialUnknown,
        ] {
            assert_managed_effect_fact(fact, true);
        }
    }

    #[test]
    fn native_unproven_fact_uses_real_managed_handoff_and_finishes_once_without_effects() {
        let (mut engine, _) = start(Probe {
            native_unproven: true,
            ..Probe::default()
        });
        let (_, first, _) = queue_mutation(&mut engine, 7, 0., true);
        // This is a financial write, not a tagged private account read. Native
        // auth-looking evidence must not invent an account-read observation.
        assert!(engine.io.pending[&first.attempt().request].auth.is_none());
        let (attempt, input) = work(&route(&mut engine, first, 1.));
        let permit = bound(&signed(&mut engine, 2., attempt, input));
        assert!(matches!(
            engine.claim_http(3., permit),
            Ok(Claim::Released(_))
        ));
        for at in [4., 5.] {
            engine.handle(
                at,
                Event::HttpResponse {
                    request: first.attempt().request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: "malformed-native-proof",
                    },
                },
            );
        }
        assert_eq!(engine.venue.responses, 1);
        assert_eq!(engine.venue.receipts, 1);
        assert_eq!(engine.venue.auth_observations.get(), 0);
        assert_eq!(engine.health().auth_failures, 0);
        assert!(engine.store.orders().is_empty());
        let failures = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                Action::Notice(Notice::CommandDone { .. }) => {
                    panic!("unproven native proof cannot succeed")
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(failures.len(), 1);
        let error = &failures[0];
        assert_eq!(error.kind, Some(ErrorKind::Internal));
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
        assert_eq!(error.venue_code.as_deref(), Some("10004"));
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["evidence-only"]);
    }

    #[test]
    fn released_native_refusal_then_retry_preparation_failure_is_unknown_not_not_sent() {
        for provider in [false, true] {
            let (mut engine, _) = start(Probe {
                retry_refusal: true,
                ..Probe::default()
            });
            let (_, first, _) = queue_mutation(&mut engine, 7, 0., true);
            let (attempt, input) = work(&route(&mut engine, first, 1.));
            let permit = bound(&signed(&mut engine, 2., attempt, input));
            assert!(matches!(
                engine.claim_http(3., permit),
                Ok(Claim::Released(_))
            ));
            engine.handle(
                4.,
                Event::HttpResponse {
                    request: first.attempt().request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: "native10002",
                    },
                },
            );
            let actions = engine.drain_actions().collect::<Vec<_>>();
            assert!(actions.iter().all(|a| !matches!(
                a,
                Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
            )));
            let retry = actions
                .iter()
                .find_map(|a| match a {
                    Action::ManagedHttp(a) => match a.as_ref() {
                        HttpAction::Prepare { preparation, .. } => Some(*preparation),
                        _ => None,
                    },
                    _ => None,
                })
                .expect("native refusal staged retry");
            assert_ne!(retry.attempt(), first.attempt());
            if provider {
                let (attempt, _) = work(&route(&mut engine, retry, 5.));
                engine.handle(
                    6.,
                    Event::Signed {
                        attempt,
                        result: Err(crate::signing::SignError::Refused),
                    },
                );
            } else {
                engine.http_preparation_failed(
                    5.,
                    retry,
                    Error::State,
                    VenueError::new("network lost; error sign"),
                );
            }
            let actions = engine.drain_actions().collect::<Vec<_>>();
            assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(crate::Outcome::Unknown))).count(), 1);
            assert_eq!(engine.venue.releases.len(), 1);
            assert!(engine.io.managed_http.is_empty());
            assert!(engine.io.pending.is_empty());
        }
    }

    #[test]
    fn parked_mutation_provider_refusal_or_deadline_seals_not_sent_once_and_late_success_is_inert()
    {
        for timeout in [false, true] {
            let (mut engine, _) = start(Probe::default());
            let (_, token, timer) = queue_mutation(&mut engine, 7, 0., true);
            let (attempt, input) = work(&route(&mut engine, token, 1.));
            assert!(engine.venue.releases.is_empty());
            if timeout {
                engine.handle(100., Event::TimerFired { timer });
            } else {
                engine.handle(
                    2.,
                    Event::Signed {
                        attempt,
                        result: Err(crate::signing::SignError::Refused),
                    },
                );
            }
            let actions = engine.drain_actions().collect::<Vec<_>>();
            assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(crate::Outcome::NotSent))).count(), 1);
            assert!(actions.iter().all(|a| !matches!(a, Action::Http(_)) && !matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. }))));
            assert!(engine.io.pending.is_empty());
            assert!(engine.io.managed_http.is_empty());
            assert!(!engine.signing_pending(attempt));
            assert!(engine.venue.releases.is_empty());
            assert_eq!(engine.venue.receipts, 0);
            let late = signed(&mut engine, 101., attempt, input);
            assert!(late.iter().all(|a| !matches!(
                a,
                Action::Http(_)
                    | Action::ManagedHttp(_)
                    | Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
            )));
            assert_eq!(engine.venue.receipts, 0);
        }
    }

    /// Release `token` at `at` (bound and claimed), as the host would.
    #[cfg(feature = "order-locks")]
    fn release(engine: &mut Engine<Probe>, token: Preparation, at: f64) {
        let permit = bind(engine, token, at);
        engine.claim_http(at, permit).unwrap();
    }

    #[cfg(feature = "order-locks")]
    fn fenced(engine: &mut Engine<Probe>) {
        engine.io.order_locks.fence(Some(&["BTCUSDT".into()]));
    }

    /// One logical order read spans several physical attempts (pages, coins);
    /// a fence raised after its first page was released is not cleared by rows
    /// the venue read before it.
    #[cfg(feature = "order-locks")]
    #[test]
    fn a_logical_read_counts_only_from_its_earliest_released_leg() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            legs: true,
            reconcile_reads: true,
            ..Probe::default()
        });
        release(&mut engine, tokens[0], 1.);
        fenced(&mut engine);
        release(&mut engine, tokens[1], 500.);
        response(&mut engine, tokens[1], 501.);
        assert_eq!(engine.venue.responses, 1);
        assert!(
            engine.io.order_locks.uncertain("BTCUSDT"),
            "the first page predates the fence"
        );
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn a_commands_read_legs_count_from_its_earliest_leg() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            command: Some(7),
            legs: true,
            reconcile_reads: true,
            ..Probe::default()
        });
        release(&mut engine, tokens[0], 1.);
        fenced(&mut engine);
        release(&mut engine, tokens[1], 500.);
        response(&mut engine, tokens[1], 501.);
        assert_eq!(engine.venue.responses, 1);
        assert!(engine.io.order_locks.uncertain("BTCUSDT"));
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn a_logical_read_released_wholly_after_the_fence_clears_it() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            legs: true,
            reconcile_reads: true,
            ..Probe::default()
        });
        fenced(&mut engine);
        release(&mut engine, tokens[0], 1.);
        release(&mut engine, tokens[1], 500.);
        response(&mut engine, tokens[1], 501.);
        assert!(!engine.io.order_locks.uncertain("BTCUSDT"));
    }

    /// Separate owners are separate logical reads: a stale owner, retired or
    /// not, never lowers another owner's freshness.
    #[cfg(feature = "order-locks")]
    #[test]
    fn an_earlier_owners_read_mark_never_reaches_another_owner() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            reconcile_reads: true,
            ..Probe::default()
        });
        release(&mut engine, tokens[0], 1.);
        response(&mut engine, tokens[0], 2.);
        fenced(&mut engine);
        release(&mut engine, tokens[1], 500.);
        response(&mut engine, tokens[1], 501.);
        assert_eq!(engine.venue.responses, 2);
        assert!(!engine.io.order_locks.uncertain("BTCUSDT"));
    }

    /// A write released under the reading command takes no read mark.
    #[cfg(feature = "order-locks")]
    #[test]
    fn a_mutation_leg_never_marks_its_owners_read() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            command: Some(7),
            reconcile_reads: true,
            ..Probe::default()
        });
        let (_, write, _) = mutation(&mut engine, 7, 0.);
        release(&mut engine, write, 1.);
        fenced(&mut engine);
        release(&mut engine, tokens[0], 500.);
        response(&mut engine, tokens[0], 501.);
        assert!(!engine.io.order_locks.uncertain("BTCUSDT"));
    }

    /// Marks live only as long as their owner's contexts; the registry's own
    /// pruning bounds them.
    #[cfg(feature = "order-locks")]
    #[test]
    fn a_read_mark_is_dropped_with_its_owners_last_context() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            ..Probe::default()
        });
        release(&mut engine, tokens[0], 1.);
        response(&mut engine, tokens[0], 2.);
        assert_eq!(engine.io.request_contexts.read_mark_count(), 1);
        Cx {
            now: 5_000.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 1_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        )
        .unwrap();
        assert_eq!(engine.io.request_contexts.read_mark_count(), 0);
    }
}
