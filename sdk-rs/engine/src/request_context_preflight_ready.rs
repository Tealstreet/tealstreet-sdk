//! Exact retained-owner activation; native FIFO selection stays with the venue.
use super::super::owned_call::Stage;
use super::*;
use crate::engine::{Engine, Venue};
#[cfg(feature = "condition-waits")]
use crate::{Action, TimerId};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Activation {
    Initial,
    Dormant,
    Parked,
    Queued,
    Activating,
    Consumed,
}

impl ContextRegistry {
    pub(in crate::engine) fn end_preflight_admission(&mut self) {
        while let Some(handle) = self.preflight_initial.pop() {
            if let Ok(state) = self.preflight_state_mut(&handle)
                && state.activation == Activation::Initial
            {
                state.activation = Activation::Dormant;
            }
        }
    }

    pub(in crate::engine) fn queue_preflight_ready(
        &mut self,
        handle: &Preflight,
    ) -> Result<(), Error> {
        let state = self.preflight_state(handle)?;
        if state.activation == Activation::Queued {
            return Ok(());
        }
        if state.activation != Activation::Parked
            || !state.call.is_empty()
            || state.handoff.is_some()
        {
            return Err(Error::Revoked);
        }
        assert!(self.preflight_ready.len() < self.preflight_ready.capacity());
        self.preflight_ready.push_back(handle.clone());
        self.preflight_state_mut(handle)?.activation = Activation::Queued;
        Ok(())
    }

    #[cfg(feature = "condition-waits")]
    fn next_preflight_ready(&mut self) -> Option<(Preflight, Option<TimerId>)> {
        while let Some(handle) = self.preflight_ready.pop_front() {
            if let Ok(state) = self.preflight_state_mut(&handle)
                && state.activation == Activation::Queued
            {
                state.activation = Activation::Activating;
                state.condition_park = false;
                let timer = state.condition.take();
                return Some((handle, timer));
            }
        }
        None
    }
    #[cfg(not(feature = "condition-waits"))]
    fn next_preflight_ready(&mut self) -> Option<Preflight> {
        while let Some(handle) = self.preflight_ready.pop_front() {
            if let Ok(state) = self.preflight_state_mut(&handle)
                && state.activation == Activation::Queued
            {
                state.activation = Activation::Activating;
                return Some(handle);
            }
        }
        None
    }

    /// A condition-parked Preflight holding nothing may fail from any callback.
    #[cfg(feature = "condition-waits")]
    pub(in crate::engine) fn condition_parked(&self, handle: &Preflight) -> bool {
        self.preflight_state(handle).is_ok_and(|state| {
            state.condition_park
                && matches!(state.activation, Activation::Parked | Activation::Queued)
                && state.call.is_empty()
                && state.handoff.is_none()
        })
    }

    /// Forget a Preflight's engine bound; the caller clears the host timer.
    #[cfg(feature = "condition-waits")]
    pub(in crate::engine) fn take_preflight_condition(
        &mut self,
        handle: &Preflight,
    ) -> Option<TimerId> {
        let timer = self.preflight_state_mut(handle).ok()?.condition.take()?;
        self.condition_preflights.retain(|(own, _)| *own != timer);
        Some(timer)
    }

    fn consume_preflight_activation(&mut self, handle: &Preflight) -> bool {
        let Ok(state) = self.preflight_state_mut(handle) else {
            // A terminal or a Plan transition consumed this preflight.
            return true;
        };
        // A condition re-park from ready custody is progress: it waits again.
        #[cfg(feature = "condition-waits")]
        if state.condition_park && state.activation == Activation::Parked {
            return true;
        }
        // A read parked on a login Session is progress: its drain issues it.
        if state.session_parked
            || state
                .call
                .first()
                .is_some_and(|call| matches!(call.stage, Stage::Active { .. }))
        {
            state.activation = Activation::Consumed;
            true
        } else {
            false
        }
    }
}

impl<P> Cx<'_, P> {
    pub(in crate::engine) fn validate_preflight_ready_owner(
        &self,
        owner: Owner,
    ) -> Result<(), Error> {
        // A ready founder's own physical Plan (#1919) is its same admission.
        if self
            .io
            .core
            .http_preflight_ready
            .as_ref()
            .is_some_and(|ready| {
                ready.owner() != owner
                    && !self
                        .io
                        .core
                        .request_contexts
                        .founded_by(owner, ready.owner())
            })
        {
            Err(Error::Revoked)
        } else {
            Ok(())
        }
    }

    #[inline]
    pub(in crate::engine) fn validate_preflight_activation(
        &self,
        handle: &Preflight,
    ) -> Result<(), Error> {
        self.core_ref().validate_preflight_activation(handle)
    }

    // Only some feature combinations call this.
    #[cfg_attr(feature = "condition-waits", allow(dead_code))]
    #[inline]
    pub(in crate::engine) fn validate_preflight_failure_custody(
        &self,
        handle: &Preflight,
    ) -> Result<(), Error> {
        self.core_ref().validate_preflight_failure_custody(handle)
    }

    /// Relinquish this genuine initial callback without creating I/O or time.
    pub fn park_preflight(&mut self, handle: &Preflight) -> Result<(), Error> {
        self.validate_preflight_activation(handle)?;
        let Owner::Command { id, .. } = handle.owner() else {
            return Err(Error::Invalid);
        };
        let state = self.io.core.request_contexts.preflight_state(handle)?;
        if !self.io.entry_bound(id)
            || state.activation != Activation::Initial
            || !state.call.is_empty()
            || state.handoff.is_some()
        {
            return Err(Error::Revoked);
        }
        self.io
            .core
            .request_contexts
            .preflight_state_mut(handle)?
            .activation = Activation::Parked;
        Ok(())
    }

    /// Park in genuine entry or Preflight-ready custody, with no I/O. `None` is
    /// unbounded; `Some(ms)` arms one engine timer that expires into
    /// `on_condition_wait_expired`. Release with `request_preflight_ready`.
    #[cfg(feature = "condition-waits")]
    pub fn park_preflight_for(
        &mut self,
        handle: &Preflight,
        bound_ms: Option<u32>,
    ) -> Result<(), Error> {
        self.validate_preflight_activation(handle)?;
        let Owner::Command { id, .. } = handle.owner() else {
            return Err(Error::Invalid);
        };
        let state = self.io.core.request_contexts.preflight_state(handle)?;
        if bound_ms == Some(0)
            || (bound_ms.is_some() && state.policy.purpose != PreflightPurpose::WriteAfterReads)
        {
            return Err(Error::Invalid);
        }
        let custody = match state.activation {
            Activation::Initial => self.io.entry_bound(id),
            Activation::Activating => true,
            _ => false,
        };
        if !custody || !state.call.is_empty() || state.handoff.is_some() {
            return Err(Error::Revoked);
        }
        self.park_preflight_condition(handle, bound_ms)
    }

    #[cfg(feature = "condition-waits")]
    #[inline]
    fn park_preflight_condition(
        &mut self,
        handle: &Preflight,
        bound_ms: Option<u32>,
    ) -> Result<(), Error> {
        self.core().park_preflight_condition(handle, bound_ms)
    }

    #[cfg(feature = "condition-waits")]
    #[inline]
    pub(in crate::engine) fn seal_condition_preflight(
        &mut self,
        preflight: &Preflight,
        error: VenueError,
    ) -> Result<Seal, Error> {
        self.core().seal_condition_preflight(preflight, error)
    }

    #[cfg(feature = "condition-waits")]
    #[inline]
    // Only some feature combinations call this.
    #[cfg_attr(feature = "condition-waits", allow(dead_code))]
    pub(in crate::engine) fn clear_preflight_condition(&mut self, handle: &Preflight) {
        self.core().clear_preflight_condition(handle)
    }

    /// Queue a single wake intent. This callback gains no authority over it.
    pub fn request_preflight_ready(&mut self, handle: &Preflight) -> Result<(), Error> {
        if !self.now.is_finite() {
            return Err(Error::Invalid);
        }
        self.io.core.request_contexts.queue_preflight_ready(handle)
    }

    pub(in crate::engine) fn owned_http_admission(
        &self,
        context: &Context,
    ) -> Result<Option<super::super::owned_call::Admission>, Error> {
        self.validate_preflight_ready_owner(context.owner())?;
        if self
            .io
            .core
            .http_plan_ready
            .as_ref()
            .is_some_and(|ready| ready.owner() != context.owner())
        {
            return Err(Error::Revoked);
        }
        let admission = self
            .io
            .core
            .request_contexts
            .owned_admission(context, self.io.core.http_delivery.as_ref())?;
        if admission.as_ref().is_some_and(|a| {
            a.predecessor.is_none() && !self.io.core.session_resumed.contains(&a.attempt.request)
        }) && let Some(plan) = self.io.core.request_contexts.plan_from_context(context)
            && !self.io.core.request_contexts.initial_plan_custody(&plan)?
            && !self
                .io
                .core
                .http_plan_ready
                .as_ref()
                .is_some_and(|ready| ready.same_plan(&plan))
        {
            return Err(Error::Revoked);
        }
        if admission.as_ref().is_some_and(|a| {
            a.predecessor.is_none() && !self.io.core.session_resumed.contains(&a.attempt.request)
        }) && let Ok(handle) = self.io.core.request_contexts.owner_from_context(context)
        {
            self.validate_preflight_activation(&handle)?;
        }
        Ok(admission)
    }
}

impl crate::engine::CoreCx<'_> {
    // Only some feature combinations call this.
    #[cfg_attr(
        all(feature = "http-plans", not(feature = "condition-waits")),
        allow(dead_code)
    )]
    #[inline]
    // Only some feature combinations call this.
    #[cfg_attr(feature = "condition-waits", allow(dead_code))]
    pub(in crate::engine) fn validate_preflight_activation(
        &self,
        handle: &Preflight,
    ) -> Result<(), Error> {
        self.core_ref().validate_preflight_activation(handle)
    }

    // Only some feature combinations call this.
    #[cfg_attr(
        all(feature = "http-plans", not(feature = "condition-waits")),
        allow(dead_code)
    )]
    #[inline]
    pub(in crate::engine) fn validate_preflight_failure_custody(
        &self,
        handle: &Preflight,
    ) -> Result<(), Error> {
        self.core_ref().validate_preflight_failure_custody(handle)
    }

    #[cfg(feature = "condition-waits")]
    #[inline(never)]
    fn park_preflight_condition(
        &mut self,
        handle: &Preflight,
        bound_ms: Option<u32>,
    ) -> Result<(), Error> {
        let bound = match bound_ms {
            Some(ms) => {
                self.io
                    .core
                    .timers
                    .try_reserve(1)
                    .map_err(|_| Error::Capacity)?;
                self.io
                    .core
                    .request_contexts
                    .condition_preflights
                    .try_reserve(1)
                    .map_err(|_| Error::Capacity)?;
                let timer = TimerId(self.io.reserve_ids(1).ok_or(Error::Capacity)?.start);
                Some((timer, self.now + f64::from(ms)))
            }
            None => None,
        };
        let state = self.io.core.request_contexts.preflight_state_mut(handle)?;
        state.activation = Activation::Parked;
        state.condition_park = true;
        if let Some((timer, at_ms)) = bound {
            state.condition = Some(timer);
            self.io.core.timers.insert(timer, ());
            self.io
                .core
                .request_contexts
                .condition_preflights
                .push((timer, handle.clone()));
            self.io.core.actions.push(Action::SetTimer { timer, at_ms });
        }
        Ok(())
    }

    /// A condition-parked owner holds no call or loan: any callback may end it.
    #[cfg(feature = "condition-waits")]
    pub(in crate::engine) fn seal_condition_preflight(
        &mut self,
        preflight: &Preflight,
        error: VenueError,
    ) -> Result<Seal, Error> {
        let parked = self.io.core.request_contexts.condition_parked(preflight);
        if !parked {
            self.validate_preflight_failure_custody(preflight)?;
        }
        let delivery = if parked {
            None
        } else {
            self.io.core.http_delivery.as_ref()
        };
        let seal = self.io.core.request_contexts.seal_preflight(
            preflight,
            self.now,
            delivery,
            Some(error),
        )?;
        self.clear_preflight_condition(preflight);
        Ok(seal)
    }

    #[cfg(feature = "condition-waits")]
    pub(in crate::engine) fn clear_preflight_condition(&mut self, handle: &Preflight) {
        if let Some(timer) = self
            .io
            .core
            .request_contexts
            .take_preflight_condition(handle)
        {
            self.io.core.timers.remove(&timer);
            self.io.core.actions.push(Action::ClearTimer { timer });
        }
    }
}

impl crate::engine::CoreRef<'_> {
    pub(in crate::engine) fn validate_preflight_activation(
        &self,
        handle: &Preflight,
    ) -> Result<(), Error> {
        if !self.now.is_finite() {
            return Err(Error::Invalid);
        }
        let state = self.io.core.request_contexts.preflight_state(handle)?;
        if state.session_parked
            || self
                .io
                .core
                .http_preflight_ready
                .as_ref()
                .is_some_and(|ready| {
                    !ready.issued_by(&self.io.core.request_contexts.issuer)
                        || ready.owner() != handle.owner()
                })
            || self.io.core.http_plan_ready.is_some()
            || self.io.core.http_plan_expiring.is_some()
        {
            return Err(Error::Revoked);
        }
        let allowed = if let Some(delivery) = self.io.core.http_delivery.as_ref() {
            state.handoff.as_ref() == Some(delivery)
                || state.call.first().is_some_and(|call| {
                    call.matches(delivery) || call.admission.predecessor.as_ref() == Some(delivery)
                })
        } else {
            state.activation == Activation::Initial
                || (state.activation == Activation::Activating
                    && self
                        .io
                        .core
                        .http_preflight_ready
                        .as_ref()
                        .is_some_and(|ready| {
                            ready.owner() == handle.owner()
                                && ready.issued_by(&self.io.core.request_contexts.issuer)
                        }))
        };
        allowed.then_some(()).ok_or(Error::Revoked)
    }

    pub(in crate::engine) fn validate_preflight_failure_custody(
        &self,
        handle: &Preflight,
    ) -> Result<(), Error> {
        if self.validate_preflight_activation(handle).is_ok() {
            return Ok(());
        }
        if !self.now.is_finite() {
            return Err(Error::Invalid);
        }
        self.io.core.request_contexts.preflight_state(handle)?;
        // Failure-only cleanup outside all active loans never grants progress.
        if self.io.core.http_delivery.is_none()
            && self.io.core.http_plan_ready.is_none()
            && self.io.core.http_plan_expiring.is_none()
            && self.io.core.http_preflight_ready.is_none()
            && self.io.core.command_entry.is_none()
        {
            Ok(())
        } else {
            Err(Error::Revoked)
        }
    }
}

impl<V: Venue> Engine<V> {
    /// The next queued wake. A condition park's engine bound ends with it.
    #[cfg(feature = "condition-waits")]
    fn next_ready_preflight(&mut self) -> Option<Preflight> {
        let (handle, timer) = self.io.core.request_contexts.next_preflight_ready()?;
        if let Some(timer) = timer {
            self.io
                .core
                .request_contexts
                .condition_preflights
                .retain(|(own, _)| *own != timer);
            self.io.core.timers.remove(&timer);
            self.io.core.actions.push(Action::ClearTimer { timer });
        }
        Some(handle)
    }
    #[cfg(not(feature = "condition-waits"))]
    #[inline]
    fn next_ready_preflight(&mut self) -> Option<Preflight> {
        self.io.core.request_contexts.next_preflight_ready()
    }

    /// An engine-bounded Preflight condition wait expired: the venue fails it in
    /// failure custody, or the engine seals it once as NotSent.
    #[cfg(feature = "condition-waits")]
    #[inline(never)]
    pub(in crate::engine) fn expire_preflight_condition(
        &mut self,
        now: f64,
        timer: TimerId,
    ) -> bool {
        let conditions = &mut self.io.core.request_contexts.condition_preflights;
        let Some(index) = conditions.iter().position(|(own, _)| *own == timer) else {
            return false;
        };
        let (_, handle) = conditions.remove(index);
        self.io.core.timers.remove(&timer);
        let live = self
            .io
            .core
            .request_contexts
            .preflight_state_mut(&handle)
            .is_ok_and(|state| {
                let live = state.condition == Some(timer) && state.activation == Activation::Parked;
                if live {
                    state.condition = None;
                }
                live
            });
        if !live {
            return true;
        }
        let error = VenueError::new("Condition wait expired")
            .kind(crate::ErrorKind::Timeout)
            .outcome(crate::Outcome::NotSent);
        self.venue.on_condition_wait_expired(
            &mut Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            },
            &crate::request_context::ConditionOwner::Preflight(handle.clone()),
            error.clone(),
        );
        if self
            .io
            .core
            .request_contexts
            .preflight_state(&handle)
            .is_ok_and(|state| state.activation == Activation::Parked)
        {
            Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            }
            .fail_preflight(&handle, error)
            .expect("expired condition-parked Preflight");
        }
        true
    }

    #[inline]
    pub(in crate::engine) fn flush_preflight_readiness(&mut self, now: f64) {
        if self.io.core.request_contexts.preflight_ready.is_empty() {
            return;
        }
        self.flush_live_preflight_readiness(now);
    }

    #[inline(never)]
    fn flush_live_preflight_readiness(&mut self, now: f64) {
        if !now.is_finite()
            || self.io.core.command_entry.is_some()
            || self.io.core.http_delivery.is_some()
            || self.io.core.http_plan_ready.is_some()
            || self.io.core.http_plan_expiring.is_some()
            || self.io.core.http_preflight_ready.is_some()
            || !self.io.core.request_contexts.plan_scope_clear()
        {
            return;
        }
        self.io.core.request_contexts.end_preflight_admission();
        self.sweep_executions(now);
        self.flush_http_owner_retirements();
        loop {
            while let Some(handle) = self.next_ready_preflight() {
                self.assert_live();
                self.io.core.http_preflight_ready = Some(handle.clone());
                self.io.core.http_plan_poison = true;
                #[cfg(feature = "order-locks")]
                let response = self.io.core.order_response_token.take();
                self.io.core.http_history_failure = None;
                self.venue.on_http_preflight_ready(
                    &mut Cx {
                        now,
                        store: &mut self.store,
                        io: &mut self.io,
                    },
                    &handle,
                );
                if !self
                    .io
                    .core
                    .request_contexts
                    .consume_preflight_activation(&handle)
                {
                    Cx {
                        now,
                        store: &mut self.store,
                        io: &mut self.io,
                    }
                    .fail_preflight(
                        &handle,
                        VenueError::new("HTTP preflight readiness made no progress")
                            .kind(crate::ErrorKind::Internal),
                    )
                    .expect("exact stalled preflight activation");
                }
                #[cfg(feature = "order-locks")]
                {
                    self.io.core.order_response_token = response;
                }
                self.io.core.http_preflight_ready = None;
                self.io.core.http_plan_poison = false;
                self.assert_live();
                self.io.core.request_contexts.end_plan_admission();
                self.sweep_executions(now);
                self.flush_http_owner_retirements();
            }
            self.flush_http_plans(now);
            self.finish_deferred_commands(now);
            #[cfg(feature = "managed-sockets")]
            if V::managed_sockets() && self.io.managed_socket_active() {
                self.fence_socket_actions(now);
                self.dispatch_actions(now);
                self.drain_socket_frames(now);
                self.process_socket_work(now);
                self.dispatch_actions(now);
                self.fence_socket_actions(now);
            }
            if self.io.core.request_contexts.preflight_ready.is_empty() {
                break;
            }
        }
    }
}

#[cfg(test)]
#[path = "request_context_planned_ready_tests.rs"]
mod planned_ready_tests;
#[cfg(test)]
#[path = "request_context_preflight_ready_tests.rs"]
mod tests;
