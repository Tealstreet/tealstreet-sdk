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
        let next = self.request_contexts.next_execution_deadline();
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

    /// Join with this command's genuine entry root. No new attempt is made and
    /// the execution bound is unchanged.
    pub fn join_execution(
        &mut self,
        execution: &ExecutionHandle,
        root: &Context,
        key: &str,
    ) -> Result<Joined, Error> {
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
        let sealed = self
            .io
            .request_contexts
            .seal_execution(&execution.0, project)?;
        self.finish_sealed(sealed);
        Ok(())
    }

    /// Seal every live observer with one error, inside or outside a delivery.
    pub fn abort_execution(
        &mut self,
        execution: &ExecutionHandle,
        error: impl Into<VenueError>,
    ) -> Result<(), Error> {
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

    fn finish_sealed(&mut self, sealed: Vec<Sealed>) {
        for seal in sealed {
            match self.io.request_contexts.observer_authority(&seal) {
                Some(root) => {
                    let finished =
                        self.finish_command_as(seal.command, seal.result, Some(&root), true);
                    debug_assert!(finished.is_ok(), "observer seal {finished:?}");
                }
                None => self.io.request_contexts.forget_observer(seal.command),
            }
        }
        self.io.rearm_execution_timer();
    }

    /// Engine-owned expiry and post-callback sweep; one terminal path.
    pub(super) fn sweep_executions(&mut self) {
        let sealed = self.io.request_contexts.expire_executions(self.now);
        self.finish_sealed(sealed);
    }
}
