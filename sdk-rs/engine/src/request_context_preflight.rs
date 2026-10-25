//! Retained preflight generation and exact native continuation retirement.
use super::*;

#[path = "request_context_preflight_call.rs"]
mod call;
pub(super) use call::State;

#[path = "request_context_preflight_ready.rs"]
mod ready;

#[path = "request_context_preflight_session.rs"]
mod session_wait;

impl ContextRegistry {
    fn prepare_preflight(
        &mut self,
        command: CommandId,
        policy: &PreflightPolicy,
        now: f64,
    ) -> Result<(), Error> {
        if !now.is_finite() {
            return Err(Error::Invalid);
        }
        policy.validate()?;
        self.prune(now);
        if self.commands.contains_key(&command) {
            return Err(Error::State);
        }
        if self.commands.len() >= CAPACITY {
            return Err(Error::Capacity);
        }
        self.commands.try_reserve(1).map_err(|_| Error::Capacity)?;
        let required = self
            .commands
            .values()
            .filter(|owner| owner.retained())
            .count()
            + 1;
        self.retirements
            .try_reserve_exact(required.saturating_sub(self.retirements.len()))
            .map_err(|_| Error::Capacity)?;
        self.preflight_ready
            .try_reserve_exact(required.saturating_sub(self.preflight_ready.len()))
            .map_err(|_| Error::Capacity)?;
        self.preflight_initial
            .try_reserve_exact(required.saturating_sub(self.preflight_initial.len()))
            .map_err(|_| Error::Capacity)?;
        Ok(())
    }

    fn begin_preflight(
        &mut self,
        command: CommandId,
        generation: u64,
        policy: PreflightPolicy,
        now: f64,
    ) -> Preflight {
        let mutation_declared = policy.purpose == PreflightPurpose::WriteAfterReads;
        let credentialed = policy.read_credential.is_some() || policy.write_credential.is_some();
        self.commands.insert(
            command,
            CommandOwner {
                generation,
                phase: CommandPhase::Preflight(State::new(policy)),
                last_deadline: now,
                writes: None,
                mutation_declared,
                pending: 0,
                deliveries: 0,
                waits: 0,
                intent: self
                    .dispatch
                    .filter(|(id, _)| *id == command)
                    .map(|(_, intent)| intent),
                leg_intents: Vec::new(),
                credentialed,
                inherited_uncertain: false,
            },
        );
        let handle = Preflight::new(&self.issuer, command, generation);
        self.preflight_initial.push(handle.clone());
        handle
    }

    fn preflight_owner(&self, preflight: &Preflight) -> Result<&CommandOwner, Error> {
        if !preflight.issued_by(&self.issuer) {
            return Err(Error::Revoked);
        }
        let Owner::Command { id, generation } = preflight.owner() else {
            return Err(Error::Invalid);
        };
        let owner = self.commands.get(&id).ok_or(Error::Revoked)?;
        if owner.generation != generation || !matches!(owner.phase, CommandPhase::Preflight(_)) {
            return Err(Error::Revoked);
        }
        Ok(owner)
    }

    fn seal_preflight(
        &self,
        preflight: &Preflight,
        now: f64,
        delivery: Option<&Delivery>,
        error: Option<VenueError>,
    ) -> Result<Seal, Error> {
        if !now.is_finite() {
            return Err(Error::Invalid);
        }
        let owner = self.preflight_owner(preflight)?;
        let CommandPhase::Preflight(state) = &owner.phase else {
            unreachable!()
        };
        state.validate_seal(delivery, error.is_some(), owner.pending, owner.deliveries)?;
        match (state.policy.purpose, error) {
            (PreflightPurpose::ReadOnly, error) => Ok(Seal::Legacy(error)),
            (PreflightPurpose::WriteAfterReads, Some(error)) => {
                // The same empty-journal projection as a Context mutation refused
                // before registration bounds text/IDs; NotSent unless inherited.
                let error = CommandWrites::new(CAPACITY)?
                    .inheriting(owner.inherited_uncertain)
                    .fail(error)?;
                Ok(Seal::Failure(error, Vec::new()))
            }
            (PreflightPurpose::WriteAfterReads, None) => Err(Error::State),
        }
    }

    fn inherit_preflight_uncertainty(&mut self, preflight: &Preflight) -> Result<(), Error> {
        let owner = self.preflight_owner(preflight)?;
        let CommandPhase::Preflight(state) = &owner.phase else {
            unreachable!()
        };
        if state.policy.purpose != PreflightPurpose::WriteAfterReads {
            return Err(Error::Invalid);
        }
        let Owner::Command { id, .. } = preflight.owner() else {
            unreachable!()
        };
        self.commands
            .get_mut(&id)
            .expect("validated preflight owner")
            .inherited_uncertain = true;
        Ok(())
    }

    pub(crate) fn has_retirements(&self) -> bool {
        !self.retirements.is_empty()
    }

    pub(crate) fn next_retirement(&self) -> Option<HttpOwnerRetirement> {
        self.retirements.front().cloned()
    }

    pub(crate) fn retirement_current(&self, token: &HttpOwnerRetirement) -> bool {
        let preflight = token.preflight();
        if !matches!(preflight.owner(), Owner::Command { .. })
            && !self.physical_owner(preflight.owner())
        {
            return false;
        }
        preflight.issued_by(&self.issuer)
            && self.retirements.front().is_some_and(|first| first.matches_preflight(preflight))
            && self.owner_entry(preflight.owner()).is_some_and(|owner| {
                owner.pending == 0
                    && matches!(&owner.phase, CommandPhase::Retiring(held) if held.matches_preflight(preflight))
            })
    }

    pub(crate) fn complete_retirement(&mut self, token: &HttpOwnerRetirement) -> bool {
        if !self.retirement_current(token) {
            return false;
        }
        let owner = token.preflight().owner();
        self.preflight_ready.retain(|held| held.owner() != owner);
        self.preflight_initial.retain(|held| held.owner() != owner);
        match owner {
            Owner::Command { id, .. } => {
                self.commands.remove(&id);
            }
            Owner::Execution { generation } => {
                self.physical.remove(&generation);
            }
            _ => return false,
        }
        self.contexts
            .retain(|_, context| context.owner() != token.preflight().owner());
        self.retirements.pop_front();
        true
    }
}

impl<P> Cx<'_, P> {
    /// Retain a command generation while bounded prerequisite reads run. This
    /// issues no Context, request, root deadline or write permission. Initial
    /// admission consumes this genuine command callback's one entry loan.
    /// Rejected already emitted one failure; Revoked emits no second terminal.
    pub fn begin_preflight(
        &mut self,
        policy: PreflightPolicy,
    ) -> Result<Preflight, crate::request_context::CommandEntryError> {
        let operation = match policy.purpose {
            PreflightPurpose::ReadOnly => Operation::Read,
            PreflightPurpose::WriteAfterReads => Operation::Mutation,
        };
        self.with_command_entry(operation, |cx, command| cx.issue_preflight(command, policy))
    }

    /// Opt into the exact caller receipt cap before asynchronous Read work.
    /// Missing receipt uses the shared typed refusal. Expired capture is Revoked
    /// without effects; Fresh custody remains for reject_expired_command(Read).
    pub fn begin_preflight_with_command_wait(
        &mut self,
        policy: PreflightPolicy,
    ) -> Result<Preflight, crate::request_context::CommandEntryError> {
        use crate::request_context::CommandEntryError;
        if policy.purpose != PreflightPurpose::ReadOnly || !self.io.fresh_command_entry() {
            return Err(CommandEntryError::Revoked);
        }
        let cap = match self.io.command_receipt() {
            Ok(cap) => cap,
            Err(reason) => return Err(self.reject_command_entry(Operation::Read, reason)),
        };
        if !self.now.is_finite() || cap.1 - self.now < 1. {
            return Err(CommandEntryError::Revoked);
        }
        self.with_command_entry(Operation::Read, |cx, command| {
            let preflight = cx.issue_preflight(command, policy)?;
            cx.io
                .core
                .request_contexts
                .bind_preflight_receipt(&preflight, cap)
                .expect("newly issued preflight");
            Ok(preflight)
        })
    }

    #[inline]
    pub(in crate::engine) fn issue_preflight(
        &mut self,
        command: CommandId,
        policy: PreflightPolicy,
    ) -> Result<Preflight, Error> {
        self.core().issue_preflight(command, policy)
    }

    #[inline]
    pub fn done_preflight(&mut self, preflight: &Preflight, result: String) -> Result<(), Error> {
        self.core().done_preflight(preflight, result)
    }

    #[inline]
    pub fn inherit_preflight_uncertainty(&mut self, preflight: &Preflight) -> Result<(), Error> {
        self.core().inherit_preflight_uncertainty(preflight)
    }

    #[inline]
    pub fn fail_preflight(
        &mut self,
        preflight: &Preflight,
        error: impl Into<VenueError>,
    ) -> Result<(), Error> {
        self.core().fail_preflight(preflight, error)
    }
}

impl crate::engine::CoreCx<'_> {
    // Private issuer shared with pure engine state fixtures. Public initial
    // admission always obtains command/generation custody from the callback.
    pub(in crate::engine) fn issue_preflight(
        &mut self,
        command: CommandId,
        policy: PreflightPolicy,
    ) -> Result<Preflight, Error> {
        if self.io.core.http_delivery.is_some() {
            return Err(Error::Revoked);
        }
        #[cfg(feature = "order-locks")]
        if (policy.purpose == PreflightPurpose::ReadOnly
            && self.io.core.order_locks.contains(command))
            || !self.io.core.order_locks.can_manage_unsent(command)
        {
            return Err(Error::State);
        }
        self.io
            .core
            .request_contexts
            .prepare_preflight(command, &policy, self.now)?;
        let generation = self.io.reserve_ids(1).ok_or(Error::Capacity)?.start;
        let writes = policy.purpose == PreflightPurpose::WriteAfterReads;
        let preflight = self
            .io
            .core
            .request_contexts
            .begin_preflight(command, generation, policy, self.now);
        #[cfg(feature = "order-locks")]
        if writes {
            self.io.core.order_locks.manage(command);
        }
        #[cfg(not(feature = "order-locks"))]
        let _ = writes;
        Ok(preflight)
    }

    /// Cached ReadOnly completion. Outstanding read-call and write-plan
    /// completion use their own authority operations; they cannot bypass this.
    pub fn done_preflight(&mut self, preflight: &Preflight, result: String) -> Result<(), Error> {
        self.validate_preflight_activation(preflight)?;
        let seal = self.io.core.request_contexts.seal_preflight(
            preflight,
            self.now,
            self.io.core.http_delivery.as_ref(),
            None,
        )?;
        let Owner::Command { id, .. } = preflight.owner() else {
            return Err(Error::Invalid);
        };
        self.finish_sealed_command(id, Some(result), seal)
    }

    /// Record that an earlier attempt of this intent may have reached the venue.
    /// Monotonic: every later failure of this command projects at least Unknown.
    /// It grants no send, resend, renewal, import or success authority; the
    /// resulting Unknown follows the existing Unknown reconciliation.
    pub fn inherit_preflight_uncertainty(&mut self, preflight: &Preflight) -> Result<(), Error> {
        self.io
            .core
            .request_contexts
            .inherit_preflight_uncertainty(preflight)
    }

    /// Failure before any plan was admitted proves this command sent no write;
    /// an inherited maybe-sent fact still projects Unknown.
    /// Expiry cannot turn this handle into success, retry or send authority.
    pub fn fail_preflight(
        &mut self,
        preflight: &Preflight,
        error: impl Into<VenueError>,
    ) -> Result<(), Error> {
        self.validate_preflight_failure_custody(preflight)?;
        let seal = self.io.core.request_contexts.seal_preflight(
            preflight,
            self.now,
            self.io.core.http_delivery.as_ref(),
            Some(error.into()),
        )?;
        let Owner::Command { id, .. } = preflight.owner() else {
            return Err(Error::Invalid);
        };
        self.finish_sealed_command(id, None, seal)
    }
}
