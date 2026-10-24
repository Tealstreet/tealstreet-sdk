//! Exact retained-owner activation; native FIFO selection stays with the venue.
use super::super::owned_call::Stage;
use super::*;
use crate::engine::{Engine, Venue};

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

    fn consume_preflight_activation(&mut self, handle: &Preflight) -> bool {
        let Ok(state) = self.preflight_state_mut(handle) else {
            // A terminal or a Plan transition consumed this preflight.
            return true;
        };
        if state
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
        if self.io.http_background_activation.is_some() && !self.background_activation_owns(owner) {
            return Err(Error::Revoked);
        }
        // A ready founder's own physical Plan (#1919) is its same admission.
        if self.io.http_preflight_ready.as_ref().is_some_and(|ready| {
            ready.owner() != owner && !self.io.request_contexts.founded_by(owner, ready.owner())
        }) {
            Err(Error::Revoked)
        } else {
            Ok(())
        }
    }

    pub(in crate::engine) fn validate_preflight_activation(
        &self,
        handle: &Preflight,
    ) -> Result<(), Error> {
        if !self.now.is_finite() {
            return Err(Error::Invalid);
        }
        let state = self.io.request_contexts.preflight_state(handle)?;
        if self.io.http_background_activation.is_some()
            || self.io.http_preflight_ready.as_ref().is_some_and(|ready| {
                !ready.issued_by(&self.io.request_contexts.issuer)
                    || ready.owner() != handle.owner()
            })
            || self.io.http_plan_ready.is_some()
            || self.io.http_plan_expiring.is_some()
        {
            return Err(Error::Revoked);
        }
        let allowed = if let Some(delivery) = self.io.http_delivery.as_ref() {
            state.handoff.as_ref() == Some(delivery)
                || state.call.first().is_some_and(|call| {
                    call.matches(delivery) || call.admission.predecessor.as_ref() == Some(delivery)
                })
        } else {
            state.activation == Activation::Initial
                || (state.activation == Activation::Activating
                    && self.io.http_preflight_ready.as_ref().is_some_and(|ready| {
                        ready.owner() == handle.owner()
                            && ready.issued_by(&self.io.request_contexts.issuer)
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
        self.io.request_contexts.preflight_state(handle)?;
        // Failure-only cleanup outside all active loans never grants progress.
        if self.io.http_delivery.is_none()
            && self.io.http_plan_ready.is_none()
            && self.io.http_plan_expiring.is_none()
            && self.io.http_preflight_ready.is_none()
            && self.io.command_entry.is_none()
        {
            Ok(())
        } else {
            Err(Error::Revoked)
        }
    }

    /// Relinquish this genuine initial callback without creating I/O or time.
    pub fn park_preflight(&mut self, handle: &Preflight) -> Result<(), Error> {
        self.validate_preflight_activation(handle)?;
        let Owner::Command { id, .. } = handle.owner() else {
            return Err(Error::Invalid);
        };
        let state = self.io.request_contexts.preflight_state(handle)?;
        if !self.io.entry_bound(id)
            || state.activation != Activation::Initial
            || !state.call.is_empty()
            || state.handoff.is_some()
        {
            return Err(Error::Revoked);
        }
        self.io
            .request_contexts
            .preflight_state_mut(handle)?
            .activation = Activation::Parked;
        Ok(())
    }

    /// Queue a single wake intent. This callback gains no authority over it.
    pub fn request_preflight_ready(&mut self, handle: &Preflight) -> Result<(), Error> {
        if !self.now.is_finite() {
            return Err(Error::Invalid);
        }
        self.io.request_contexts.queue_preflight_ready(handle)
    }

    pub(in crate::engine) fn owned_http_admission(
        &self,
        context: &Context,
    ) -> Result<Option<super::super::owned_call::Admission>, Error> {
        self.validate_preflight_ready_owner(context.owner())?;
        if self
            .io
            .http_plan_ready
            .as_ref()
            .is_some_and(|ready| ready.owner() != context.owner())
        {
            return Err(Error::Revoked);
        }
        let admission = self
            .io
            .request_contexts
            .owned_admission(context, self.io.http_delivery.as_ref())?;
        if admission.as_ref().is_some_and(|a| a.predecessor.is_none())
            && let Some(plan) = self.io.request_contexts.plan_from_context(context)
            && !self.io.request_contexts.initial_plan_custody(&plan)?
            && !self
                .io
                .http_plan_ready
                .as_ref()
                .is_some_and(|ready| ready.same_plan(&plan))
        {
            return Err(Error::Revoked);
        }
        if admission.as_ref().is_some_and(|a| a.predecessor.is_none())
            && let Ok(handle) = self.io.request_contexts.owner_from_context(context)
        {
            self.validate_preflight_activation(&handle)?;
        }
        Ok(admission)
    }
}

impl<V: Venue> Engine<V> {
    #[inline]
    pub(in crate::engine) fn flush_preflight_readiness(&mut self, now: f64) {
        self.flush_background_tasks(now);
        if self.io.request_contexts.preflight_ready.is_empty() {
            return;
        }
        self.flush_live_preflight_readiness(now);
    }

    #[inline(never)]
    fn flush_live_preflight_readiness(&mut self, now: f64) {
        if !now.is_finite()
            || self.io.command_entry.is_some()
            || self.io.http_delivery.is_some()
            || self.io.http_plan_ready.is_some()
            || self.io.http_plan_expiring.is_some()
            || self.io.http_preflight_ready.is_some()
            || !self.io.request_contexts.plan_scope_clear()
        {
            return;
        }
        self.io.request_contexts.end_preflight_admission();
        self.sweep_executions(now);
        self.flush_http_owner_retirements();
        loop {
            while let Some(handle) = self.io.request_contexts.next_preflight_ready() {
                self.assert_live();
                self.io.http_preflight_ready = Some(handle.clone());
                self.io.http_plan_poison = true;
                #[cfg(feature = "order-locks")]
                let response = self.io.order_response_token.take();
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
                    self.io.order_response_token = response;
                }
                self.io.http_preflight_ready = None;
                self.io.http_plan_poison = false;
                self.assert_live();
                self.io.request_contexts.end_plan_admission();
                self.sweep_executions(now);
                self.flush_http_owner_retirements();
            }
            self.flush_http_plans(now);
            self.finish_deferred_commands(now);
            #[cfg(feature = "managed-sockets")]
            if self.io.managed_socket_active() {
                self.fence_socket_actions(now);
                self.dispatch_actions(now);
                self.drain_socket_frames(now);
                self.process_socket_work(now);
                self.dispatch_actions(now);
                self.fence_socket_actions(now);
            }
            if self.io.request_contexts.preflight_ready.is_empty() {
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
