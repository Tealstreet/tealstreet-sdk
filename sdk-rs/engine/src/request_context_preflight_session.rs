//! A retained Preflight read parked on a login Session before it is issued.
use super::super::owned_call::Stage;
use super::ready::Activation;
use super::*;
use crate::TimerId;
use crate::engine::{Engine, Venue};
use crate::request_context::{RetryEvidence, Session, plan::CallTerms};

impl ContextRegistry {
    pub(in crate::engine) fn preflight_registered(&self, handle: &Preflight) -> bool {
        self.preflight_state(handle).is_ok()
    }
}

impl<P> Cx<'_, P> {
    /// Park this Preflight's next prerequisite read on a login Session before
    /// it consumes an attempt. `session_ready` issues it under the same
    /// Preflight generation; nothing is spent while parked. A caller receipt
    /// cap bounds the wait with one timer, since the read window is unbounded.
    pub fn preflight_session_wait(
        &mut self,
        handle: &Preflight,
        policy: Policy,
        terms: CallTerms,
        session: &Session,
        pending: P,
    ) -> Result<(), (P, Error)> {
        let checked = (|| {
            let generation = self.io.core.request_contexts.validate_session(session)?;
            self.validate_preflight_activation(handle)?;
            let state = self.io.core.request_contexts.preflight_state(handle)?;
            if !state.call.is_empty() || !state.admits_handoff(self.io.core.http_delivery.as_ref())
            {
                return Err(Error::Revoked);
            }
            self.validate_preflight_read(handle, &policy, terms, true)?;
            self.reserve_preflight_wait(handle)
                .map(|cap| (generation, cap))
        })();
        let (generation, (cap, timer)) = match checked {
            Ok(checked) => checked,
            Err(error) => return Err((pending, error)),
        };
        let state = self
            .io
            .core
            .request_contexts
            .preflight_state_mut(handle)
            .expect("validated parked Preflight");
        state.session_parked = true;
        // The park consumed the handoff; its resume or ended disposition is
        // the only custody that issues this read.
        state.handoff = None;
        self.io.session_waits.park_preflight_read(
            generation,
            handle.clone(),
            (policy, terms),
            cap,
            timer,
            pending,
        );
        Ok(())
    }

    /// Park a failed read's declared retry on a login Session, in the failed
    /// delivery's custody (or as a re-park from its ended disposition),
    /// without selecting the next attempt.
    pub fn preflight_retry_session_wait(
        &mut self,
        handle: &Preflight,
        context: &Context,
        evidence: RetryEvidence,
        session: &Session,
        pending: P,
    ) -> Result<(), (P, Error)> {
        let checked = (|| {
            let generation = self.io.core.request_contexts.validate_session(session)?;
            if evidence == RetryEvidence::DefinitiveRefusal
                || context.operation() != Operation::Read
            {
                return Err(Error::RetryForbidden);
            }
            context.retry(evidence, self.now)?;
            self.io.core.request_contexts.validate(context, self.now)?;
            if self
                .io
                .core
                .request_contexts
                .preflight_state(handle)?
                .session_parked
            {
                return Err(Error::Revoked);
            }
            let call = self.io.core.request_contexts.read_call(handle, context)?;
            let delivered = self.io.core.http_delivery.as_ref().is_some_and(|delivery| {
                call.matches(delivery) && matches!(call.stage, Stage::Active { .. })
            });
            let reparked = matches!(call.stage, Stage::Parked)
                && self.validate_preflight_activation(handle).is_ok();
            if !(delivered || reparked) {
                return Err(Error::Revoked);
            }
            if call.selected >= call.terms.attempts {
                return Err(Error::RetryForbidden);
            }
            call.window.physical_deadline(self.now)?;
            self.reserve_preflight_wait(handle)
                .map(|cap| (generation, cap))
        })();
        let (generation, (cap, timer)) = match checked {
            Ok(checked) => checked,
            Err(error) => return Err((pending, error)),
        };
        self.io.retire_http_leg(context);
        let state = self
            .io
            .core
            .request_contexts
            .preflight_state_mut(handle)
            .expect("validated parked Preflight");
        state.session_parked = true;
        state.call.first_mut().expect("validated call").stage = Stage::Parked;
        self.io.session_waits.park_preflight_retry(
            generation,
            handle.clone(),
            context.clone(),
            cap,
            timer,
            pending,
        );
        Ok(())
    }

    /// Every fallible reservation a park needs, before any commit: wait and
    /// ended storage, resume custody, and the cap timer's ID.
    fn reserve_preflight_wait(
        &mut self,
        handle: &Preflight,
    ) -> Result<(f64, Option<TimerId>), Error> {
        if self.io.session_waits.len() >= CAPACITY {
            return Err(Error::Capacity);
        }
        self.io.session_waits.reserve_ended()?;
        let parked = self.io.session_waits.parked_units();
        self.io
            .core
            .session_resumed
            .try_reserve(parked + 1)
            .map_err(|_| Error::Capacity)?;
        let cap = self
            .io
            .core
            .request_contexts
            .preflight_receipt_cap(handle)?
            .map(|(_, cap)| cap);
        let Some(cap) = cap else {
            return Ok((f64::INFINITY, None));
        };
        if !self.now.is_finite() || cap - self.now < 1. {
            return Err(Error::TimedOut);
        }
        let timer = TimerId(self.io.reserve_ids(1).ok_or(Error::Capacity)?.start);
        self.io.core.timers.insert(timer, ());
        self.io
            .core
            .actions
            .push(crate::Action::SetTimer { timer, at_ms: cap });
        Ok((cap, Some(timer)))
    }

    #[inline]
    pub(in crate::engine) fn resume_preflight_read(
        &mut self,
        handle: &Preflight,
        policy: Policy,
        terms: CallTerms,
    ) -> Result<Context, Error> {
        self.core().resume_preflight_read(handle, policy, terms)
    }

    #[inline]
    pub(in crate::engine) fn resume_preflight_retry(
        &mut self,
        handle: &Preflight,
        context: &Context,
    ) -> Result<Context, Error> {
        self.core().resume_preflight_retry(handle, context)
    }

    #[inline]
    // Only some feature combinations call this.
    #[cfg_attr(feature = "http-plans", allow(dead_code))]
    fn resumed_preflight(&mut self, handle: &Preflight) {
        self.core().resumed_preflight(handle)
    }

    #[inline]
    pub(in crate::engine) fn expire_parked_preflight(&mut self, handle: &Preflight) {
        self.core().expire_parked_preflight(handle)
    }
}

impl crate::engine::CoreCx<'_> {
    /// Issue a parked read at its Session's resume, under the drain's delivery.
    pub(in crate::engine) fn resume_preflight_read(
        &mut self,
        handle: &Preflight,
        policy: Policy,
        terms: CallTerms,
    ) -> Result<Context, Error> {
        let state = self.io.core.request_contexts.preflight_state(handle)?;
        if !state.session_parked || !state.call.is_empty() {
            return Err(Error::Revoked);
        }
        self.validate_preflight_read(handle, &policy, terms, true)?;
        let read = self.issue_preflight_read(handle, policy, terms, true)?;
        self.resumed_preflight(handle);
        Ok(read)
    }

    /// Select a parked retry's next attempt at its Session's resume, under the
    /// drain's delivery, inside the existing budget and declared window.
    pub(in crate::engine) fn resume_preflight_retry(
        &mut self,
        handle: &Preflight,
        context: &Context,
    ) -> Result<Context, Error> {
        if !self
            .io
            .core
            .request_contexts
            .preflight_state(handle)?
            .session_parked
        {
            return Err(Error::Revoked);
        }
        self.io.core.request_contexts.validate(context, self.now)?;
        let call = self.io.core.request_contexts.read_call(handle, context)?;
        if !matches!(call.stage, Stage::Parked) {
            return Err(Error::Revoked);
        }
        if call.selected >= call.terms.attempts {
            return Err(Error::RetryForbidden);
        }
        call.window.physical_deadline(self.now)?;
        let predecessor = self.io.core.http_delivery.clone();
        self.io
            .core
            .request_contexts
            .preflight_state_mut(handle)
            .expect("validated parked Preflight")
            .call
            .first_mut()
            .expect("validated call")
            .select(self.now, predecessor)
            .expect("validated retry");
        self.resumed_preflight(handle);
        Ok(context.clone())
    }

    /// The read is issued: unpark, and a resume with no delivery is its
    /// attempt's custody for the rest of this event only.
    fn resumed_preflight(&mut self, handle: &Preflight) {
        let state = self
            .io
            .core
            .request_contexts
            .preflight_state_mut(handle)
            .expect("resumed Preflight");
        state.session_parked = false;
        let request = state
            .call
            .first()
            .expect("resumed call")
            .admission
            .attempt
            .request;
        if self.io.core.http_delivery.is_none() {
            self.io.core.session_resumed.push(request);
        }
    }

    /// The caller's receipt cap passed while the read was parked: seal the
    /// Preflight with Timeout, on engine authority and with no venue callback.
    pub(in crate::engine) fn expire_parked_preflight(&mut self, handle: &Preflight) {
        let Ok(state) = self.io.core.request_contexts.preflight_state_mut(handle) else {
            return;
        };
        state.session_parked = false;
        let error = VenueError::new("Session wait timed out").kind(crate::ErrorKind::Timeout);
        let seal = self
            .io
            .core
            .request_contexts
            .seal_preflight(handle, self.now, None, Some(error))
            .expect("parked Preflight has no outstanding attempt");
        let Owner::Command { id, .. } = handle.owner() else {
            unreachable!()
        };
        self.finish_sealed_command(id, None, seal)
            .expect("exact parked Preflight expiry");
    }
}

impl<V: Venue> Engine<V> {
    /// A parked read whose Session ended: unpark it and, if the Preflight is
    /// live, run the ended hook in its own ready custody. A hook that neither
    /// re-parks, issues nor finishes fails the Preflight once with `error`.
    pub(in crate::engine) fn end_preflight_session_wait(
        &mut self,
        now: f64,
        handle: Preflight,
        pending: V::Pending,
        error: VenueError,
    ) {
        let Ok(state) = self.io.core.request_contexts.preflight_state_mut(&handle) else {
            self.venue.on_session_wait_expired(
                &mut Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                },
                pending,
                error,
            );
            return;
        };
        state.session_parked = false;
        let prior = std::mem::replace(&mut state.activation, Activation::Activating);
        self.io.core.http_preflight_ready = Some(handle.clone());
        self.io.core.http_plan_poison = true;
        #[cfg(feature = "order-locks")]
        let response = self.io.core.order_response_token.take();
        self.venue.on_preflight_session_wait_ended(
            &mut Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            },
            &handle,
            pending,
            error.clone(),
        );
        if let Ok(state) = self.io.core.request_contexts.preflight_state_mut(&handle) {
            let active = state
                .call
                .first()
                .is_some_and(|call| matches!(call.stage, Stage::Active { .. }));
            if state.session_parked {
                state.activation = prior;
            } else if active {
                state.activation = Activation::Consumed;
            } else {
                Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                }
                .fail_preflight(&handle, error)
                .expect("ended Preflight in its ready custody");
            }
        }
        #[cfg(feature = "order-locks")]
        {
            self.io.core.order_response_token = response;
        }
        self.io.core.http_preflight_ready = None;
        self.io.core.http_plan_poison = false;
    }
}

#[cfg(test)]
#[path = "request_context_preflight_session_tests.rs"]
mod tests;
