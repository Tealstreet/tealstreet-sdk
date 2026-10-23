//! Venue-facing execution groups: one physical execution, several command
//! observers, each sealed once through its own root (RUST_EXECUTION_GROUPS.md).
use super::*;
use crate::execution_group::Sealed;
pub use crate::execution_group::{Joined, Window};
use crate::request_context::{Context, Error, Owner, Policy};

/// Opaque execution authority. Legs are issued with `http_leg_context` on
/// `context()`; it carries no command completion or entry rights.
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionHandle(Context);

impl ExecutionHandle {
    pub fn context(&self) -> &Context {
        &self.0
    }
}

impl<P> Io<P> {
    pub(super) fn rearm_execution_timer(&mut self) {
        // Called on every HTTP completion; skip the scan when no group exists.
        let next = if self.request_contexts.has_executions() {
            self.request_contexts.next_execution_deadline()
        } else {
            None
        };
        if self.execution_timer.map(|(_, at)| at) == next {
            return;
        }
        if let Some((timer, _)) = self.execution_timer.take() {
            self.actions.push(Action::ClearTimer { timer });
        }
        if let Some(at_ms) = next {
            let timer = TimerId(self.next());
            self.actions.push(Action::SetTimer { timer, at_ms });
            self.execution_timer = Some((timer, at_ms));
        }
    }
}

impl<P> Cx<'_, P> {
    /// Begin a read execution founded by this command's genuine entry root.
    /// The execution bound is capped by the founder and never raised later.
    pub fn begin_execution(
        &mut self,
        founder: &Context,
        policy: Policy,
        window: Window,
        key: &str,
    ) -> Result<ExecutionHandle, Error> {
        if !self.entry_callback(founder) {
            return Err(Error::Revoked);
        }
        if !self.io.next_id.available(1) {
            return Err(Error::Capacity);
        }
        let identity = self.io.reserve_ids(1).ok_or(Error::Capacity)?.start;
        let root = self
            .io
            .request_contexts
            .begin_execution(identity, founder, policy, window, key, self.now)?;
        self.io.rearm_execution_timer();
        Ok(ExecutionHandle(root))
    }

    /// Begin a read execution rooted in a live background read, which keeps
    /// its own publication rights. Commands may join; none founds it.
    pub fn begin_background_execution(
        &mut self,
        background: &Context,
        policy: Policy,
        window: Window,
    ) -> Result<ExecutionHandle, Error> {
        if self.io.http_delivery.is_some() || !self.io.next_id.available(1) {
            return Err(if self.io.http_delivery.is_some() {
                Error::Revoked
            } else {
                Error::Capacity
            });
        }
        let identity = self.io.reserve_ids(1).ok_or(Error::Capacity)?.start;
        let root = self
            .io
            .request_contexts
            .begin_background_execution(identity, background, policy, window, self.now)?;
        self.io.rearm_execution_timer();
        Ok(ExecutionHandle(root))
    }

    /// The next read of the same account adopts owners that joined after
    /// `previous` was released; owners already past their deadline time out
    /// once. A retired `previous` (nothing queued) is `Revoked`: begin afresh.
    pub fn begin_successor_execution(
        &mut self,
        previous: &ExecutionHandle,
        background: &Context,
        policy: Policy,
    ) -> Result<ExecutionHandle, Error> {
        if self.io.http_delivery.is_some() {
            return Err(Error::Revoked);
        }
        if !self.io.next_id.available(1) {
            return Err(Error::Capacity);
        }
        let identity = self.io.reserve_ids(1).ok_or(Error::Capacity)?.start;
        let (root, sealed) = self.io.request_contexts.begin_successor_execution(
            identity,
            &previous.0,
            background,
            policy,
            self.now,
        )?;
        self.finish_sealed(sealed);
        Ok(ExecutionHandle(root))
    }

    /// Join with this command's genuine entry root. No new attempt is made and
    /// the execution bound is unchanged.
    pub fn join_execution(
        &mut self,
        execution: &ExecutionHandle,
        root: &Context,
        key: &str,
    ) -> Result<Joined, Error> {
        if !self.entry_callback(root) {
            return Err(Error::Revoked);
        }
        self.sweep_executions();
        let joined = self
            .io
            .request_contexts
            .join_execution(&execution.0, root, key, self.now)?;
        self.io.rearm_execution_timer();
        Ok(joined)
    }

    /// Retire only this observer; siblings and shared physical work continue.
    pub fn detach_execution_observer(
        &mut self,
        root: &Context,
        error: impl Into<VenueError>,
    ) -> Result<(), Error> {
        self.validate_preflight_ready_owner(root.owner())?;
        let sealed = self
            .io
            .request_contexts
            .detach_observer(root, error.into())?;
        self.finish_sealed(vec![sealed]);
        Ok(())
    }

    /// Seal every live observer once with its projected result, in the exact
    /// delivery callback of one of this execution's legs.
    pub fn complete_execution(
        &mut self,
        execution: &ExecutionHandle,
        project: impl FnMut(&str) -> Result<String, VenueError>,
    ) -> Result<(), Error> {
        if self
            .io
            .http_delivery
            .as_ref()
            .is_none_or(|delivery| delivery.context.owner() != execution.0.owner())
        {
            return Err(Error::Revoked);
        }
        // Owners already past their deadline time out before any projection.
        self.sweep_executions();
        let sealed = self
            .io
            .request_contexts
            .seal_execution(&execution.0, project)?;
        self.finish_sealed(sealed);
        Ok(())
    }

    /// Seal every live observer with one error, outside any delivery or in
    /// one of this execution's own deliveries.
    pub fn abort_execution(
        &mut self,
        execution: &ExecutionHandle,
        error: impl Into<VenueError>,
    ) -> Result<(), Error> {
        self.validate_preflight_ready_owner(execution.0.owner())?;
        if self
            .io
            .http_delivery
            .as_ref()
            .is_some_and(|delivery| delivery.context.owner() != execution.0.owner())
        {
            return Err(Error::Revoked);
        }
        self.sweep_executions();
        let error = error.into();
        let sealed = self
            .io
            .request_contexts
            .seal_execution(&execution.0, |_| Err(error.clone()))?;
        self.finish_sealed(sealed);
        Ok(())
    }

    /// Publication gate for this exact execution leg, before any codec,
    /// native-cache or Store change. Grants no send or completion rights.
    pub fn validate_execution_read_delivery(
        &self,
        execution: &ExecutionHandle,
        leg: &Context,
    ) -> Result<(), Error> {
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::Revoked)?;
        if delivery.context != *leg
            || leg.owner() != execution.0.owner()
            || !matches!(leg.owner(), Owner::Execution { .. })
        {
            return Err(Error::Revoked);
        }
        self.io.request_contexts.validate(leg, self.now)
    }

    /// Begin/join run only in the observer command's own genuine callback,
    /// in addition to the registry's exact root/issuer/generation checks.
    fn entry_callback(&self, root: &Context) -> bool {
        matches!(root.owner(), Owner::Command { id, .. } if self.io.entry_bound(id))
            && self.io.http_delivery.is_none()
    }

    fn finish_sealed(&mut self, sealed: Vec<Sealed>) {
        for seal in sealed {
            // Entry-owned observer roots are retained until this exact seal.
            let root = self
                .io
                .request_contexts
                .observer_authority(&seal)
                .expect("retained observer root");
            let finished = self.finish_command_as(seal.command, seal.result, Some(&root), true);
            assert!(finished.is_ok(), "observer seal {finished:?}");
        }
        self.io.revoke_unsent_execution_legs();
        self.io.rearm_execution_timer();
    }

    /// Engine-owned expiry and post-callback sweep; one terminal path.
    pub(super) fn sweep_executions(&mut self) {
        if !self.io.request_contexts.has_executions() {
            self.io.request_contexts.clear_retired_roots();
            return;
        }
        let sealed = self.io.request_contexts.expire_executions(self.now);
        self.finish_sealed(sealed);
    }
}
