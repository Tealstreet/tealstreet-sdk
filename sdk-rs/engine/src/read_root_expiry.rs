//! Engine-owned expiry for tracked caller-wait Read roots
//! (docs/plans/RUST_READ_ROOT_EXPIRY.md).
use super::*;
use crate::request_context::{
    CommandEntryError, Context, Credential, Error, Operation, Owner, Pacing, Policy, RetryPolicy,
    Scope,
};

/// A half-run sweep never leaves a half-retired owner: an unwind poisons.
struct SweepScope<'a, V: Venue> {
    engine: &'a mut Engine<V>,
}
impl<V: Venue> Drop for SweepScope<'_, V> {
    fn drop(&mut self) {
        if self.engine.io.core.read_roots_expiring {
            self.engine.io.core.http_owner_poison = true;
        }
    }
}

impl<P> Io<P> {
    #[inline]
    pub(super) fn read_root_timer(&mut self, timer: TimerId, now: f64) -> bool {
        self.core_io().read_root_timer(timer, now)
    }
}

impl crate::engine::CoreIo<'_> {
    /// Every terminal of a tracked owner leaves the index and clears its own
    /// timer; nothing here needs a fresh identity.
    pub(super) fn untrack_read_root(&mut self, command: CommandId) {
        if let Some(index) = self
            .core
            .read_roots
            .iter()
            .position(|(_, id, ..)| *id == command)
        {
            let (.., timer) = self.core.read_roots.swap_remove(index);
            self.core.actions.push(Action::ClearTimer { timer });
        }
    }

    /// A tracked root's own timer; one that fires early re-arms the same
    /// identity for the exact deadline (true: consumed here).
    pub(super) fn read_root_timer(&mut self, timer: TimerId, now: f64) -> bool {
        let Some(&(at_ms, ..)) = self.core.read_roots.iter().find(|(.., own)| *own == timer) else {
            return false;
        };
        if now < at_ms {
            self.core.actions.push(Action::SetTimer { timer, at_ms });
        }
        true
    }
}

impl<V: Venue> Engine<V> {
    /// Retire every tracked root due at `now` before anything can stamp, run a
    /// codec or lend authority: prepare all, mark all, then per owner discard,
    /// hand back its continuations without a Cx, and seal once.
    pub(super) fn expire_read_roots(&mut self, now: f64) {
        if self.io.core.read_roots.is_empty()
            || !now.is_finite()
            || !self.io.core.read_roots.iter().any(|(at, ..)| now >= *at)
        {
            return;
        }
        self.io.core.read_roots_expiring = true;
        let scope = SweepScope { engine: self };
        let io = &mut scope.engine.io;
        let mut due = Vec::new();
        let mut cleanup = Vec::new();
        let reserved = due.try_reserve_exact(io.core.read_roots.len()).is_ok()
            && cleanup.try_reserve_exact(io.core.read_roots.len()).is_ok();
        assert!(reserved, "read root expiry storage");
        for &(at, id, generation, _) in &io.core.read_roots {
            if now >= at {
                due.push((id, generation));
            }
        }
        // Prepare: every owner's requests and continuation storage, and the
        // whole sweep's actions in aggregate, before any mark.
        let mut actions = 0_usize;
        for &(id, generation) in &due {
            let owner = Owner::Command { id, generation };
            let owned =
                |managed: &http_lifecycle::Managed| managed.delivery().context.owner() == owner;
            let count = io
                .core
                .managed_http
                .values()
                .filter(|managed| owned(managed))
                .count();
            let mut requests = Vec::new();
            let mut pending = Vec::new();
            let reserved = requests.try_reserve_exact(count).is_ok()
                && pending.try_reserve_exact(count).is_ok();
            assert!(reserved, "read root expiry storage");
            // Per request: timer clear and cancel. Per owner: its timer clear,
            // terminal notice and any classified error notice.
            actions = actions.saturating_add(count.saturating_mul(2).saturating_add(3));
            requests.extend(
                io.core
                    .managed_http
                    .iter()
                    .filter(|(_, managed)| owned(managed))
                    .map(|(request, _)| *request),
            );
            cleanup.push((owner, requests, pending));
        }
        assert!(
            io.core.actions.try_reserve(actions).is_ok(),
            "read root expiry storage"
        );
        #[cfg(test)]
        {
            io.core.read_root_sweep_capacity = Some(io.core.actions.capacity());
        }
        // Mark every due root first: no cleanup can publish for another.
        for &(id, generation) in &due {
            io.core.request_contexts.expire_read_root(id, generation);
        }
        for (owner, requests, mut pending) in cleanup {
            let Owner::Command { id, generation } = owner else {
                unreachable!()
            };
            for request in requests {
                if let Some(held) = scope.engine.io.discard_managed(request) {
                    pending.push(held.value);
                }
            }
            scope.engine.venue.read_root_retired(owner, pending);
            let seal = scope
                .engine
                .io
                .core
                .request_contexts
                .seal_expired_root(id, generation);
            let mut cx = Cx {
                now,
                store: &mut scope.engine.store,
                io: &mut scope.engine.io,
            };
            cx.finish_sealed_command(id, None, seal)
                .expect("expired root seals once");
        }
        scope.engine.io.core.read_roots_expiring = false;
    }
}

impl<P> Cx<'_, P> {
    /// Issue this command's original Read root at the caller's exact receipt
    /// deadline, tracked for engine-owned expiry. Fresh entry only. An expired
    /// wait is `Revoked` with no effect (finish it with
    /// `reject_expired_command`); a missing receipt is one typed refusal.
    pub fn admit_caller_read_root(
        &mut self,
        credential: Option<Credential>,
        pacing: Option<Pacing>,
    ) -> Result<Context, CommandEntryError> {
        if !self.io.fresh_command_entry() {
            return Err(CommandEntryError::Revoked);
        }
        let deadline = match self.io.command_receipt() {
            Ok((_, deadline)) => deadline,
            Err(reason) => return Err(self.reject_command_entry(Operation::Read, reason)),
        };
        if !self.now.is_finite() || deadline - self.now < 1. {
            return Err(CommandEntryError::Revoked);
        }
        let command = self.io.command_entry_id().expect("fresh entry");
        // Every check and reservation precedes the only spend: the root's and
        // its timer's identities, taken together.
        let checked = self.root_admission_checks(command, credential.as_ref(), pacing);
        if let Err(reason) = checked {
            return Err(self.reject_command_entry(Operation::Read, reason));
        }
        let policy = Policy {
            operation: Operation::Read,
            credential,
            timeout_ms: (deadline - self.now).ceil().min(f64::from(u32::MAX)) as u32,
            pacing,
            retry: RetryPolicy::Never,
        };
        let now = self.now;
        self.with_command_entry(Operation::Read, |cx, command| {
            let ids = cx.io.reserve_ids(2).ok_or(Error::Capacity)?;
            let root = cx.io.core.request_contexts.issue_until(
                ids.start,
                Scope::Command(command),
                policy,
                now,
                None,
                None,
                Some(deadline),
            )?;
            cx.io.core.request_contexts.retain_command_entry(&root);
            cx.io.core.request_contexts.track_read_root(&root);
            let Owner::Command { generation, .. } = root.owner() else {
                unreachable!("command scope")
            };
            let timer = TimerId(ids.start + 1);
            cx.io
                .core
                .read_roots
                .push((deadline, command, generation, timer));
            cx.io.core.actions.push(Action::SetTimer {
                timer,
                at_ms: deadline,
            });
            Ok(root)
        })
    }

    #[inline]
    fn root_admission_checks(
        &mut self,
        command: CommandId,
        credential: Option<&Credential>,
        pacing: Option<Pacing>,
    ) -> Result<(), Error> {
        self.core()
            .root_admission_checks(command, credential, pacing)
    }

    #[inline]
    pub fn reject_unstamped_command(
        &mut self,
        operation: Operation,
    ) -> Result<(), CommandEntryError> {
        self.core().reject_unstamped_command(operation)
    }
}

impl crate::engine::CoreCx<'_> {
    /// Effect-free checks and fallible reservations before any identity spend.
    fn root_admission_checks(
        &mut self,
        command: CommandId,
        credential: Option<&Credential>,
        pacing: Option<Pacing>,
    ) -> Result<(), Error> {
        if let Some(credential) = credential {
            credential.validate()?;
        }
        if pacing.is_some_and(|p| !p.minimum_gap_ms.is_finite() || p.minimum_gap_ms < 0.) {
            return Err(Error::Invalid);
        }
        if !self.io.core.next_id.available(2)
            || self.io.core.read_roots.len() >= self.io.core.read_roots.capacity()
        {
            return Err(Error::Capacity);
        }
        self.io
            .core
            .request_contexts
            .reserve_command_root(command)?;
        self.io
            .core
            .actions
            .try_reserve(1)
            .map_err(|_| Error::Capacity)
    }

    /// A genuine Fresh entry whose own receipt is missing (exhausted sequence,
    /// duplicate live legacy id, unstampable clock) ends with one typed
    /// refusal: Read with no outcome, Mutation NotSent. `Ok` means that
    /// refusal was emitted; `Revoked` and no effect when a receipt exists or
    /// the entry is not Fresh.
    pub fn reject_unstamped_command(
        &mut self,
        operation: Operation,
    ) -> Result<(), CommandEntryError> {
        if !self.io.fresh_command_entry() {
            return Err(CommandEntryError::Revoked);
        }
        let Err(reason) = self.io.command_receipt() else {
            return Err(CommandEntryError::Revoked);
        };
        match self.reject_command_entry(operation, reason) {
            CommandEntryError::Rejected(_) => Ok(()),
            error => Err(error),
        }
    }
}
