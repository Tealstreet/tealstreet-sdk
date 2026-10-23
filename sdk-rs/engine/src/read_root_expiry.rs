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
        if self.engine.io.read_roots_expiring {
            self.engine.io.http_owner_poison = true;
        }
    }
}

impl<P> Io<P> {
    /// One engine timer for the earliest tracked deadline; never a venue timer.
    fn rearm_read_root_timer(&mut self) {
        let next = self.read_roots.iter().map(|(at, ..)| *at).reduce(f64::min);
        if self.read_root_timer.map(|(_, at)| at) == next {
            return;
        }
        if let Some((timer, _)) = self.read_root_timer.take() {
            self.actions.push(Action::ClearTimer { timer });
        }
        if let Some(at_ms) = next {
            let timer = TimerId(self.next());
            self.actions.push(Action::SetTimer { timer, at_ms });
            self.read_root_timer = Some((timer, at_ms));
        }
    }

    /// Every terminal of a tracked owner leaves the index.
    pub(super) fn untrack_read_root(&mut self, command: CommandId) {
        if let Some(index) = self.read_roots.iter().position(|(_, id, _)| *id == command) {
            self.read_roots.swap_remove(index);
            self.rearm_read_root_timer();
        }
    }

    pub(super) fn read_root_timer(&self, timer: TimerId) -> bool {
        self.read_root_timer.is_some_and(|(own, _)| own == timer)
    }
}

impl<V: Venue> Engine<V> {
    /// Retire every tracked root due at `now` before anything can stamp, run a
    /// codec or lend authority: prepare all, mark all, then per owner discard,
    /// hand back its continuations without a Cx, and seal once.
    pub(super) fn expire_read_roots(&mut self, now: f64) {
        if self.io.read_roots.is_empty()
            || !now.is_finite()
            || !self.io.read_roots.iter().any(|(at, ..)| now >= *at)
        {
            return;
        }
        self.io.read_roots_expiring = true;
        let scope = SweepScope { engine: self };
        let io = &mut scope.engine.io;
        let mut due = Vec::new();
        let mut cleanup = Vec::new();
        let reserved = due.try_reserve_exact(io.read_roots.len()).is_ok()
            && cleanup.try_reserve_exact(io.read_roots.len()).is_ok();
        assert!(reserved, "read root expiry storage");
        for &(at, id, generation) in &io.read_roots {
            if now >= at {
                due.push((id, generation));
            }
        }
        // Prepare: every owner's requests and continuation storage, before any mark.
        for &(id, generation) in &due {
            let owner = Owner::Command { id, generation };
            let owned =
                |managed: &http_lifecycle::Managed| managed.delivery().context.owner() == owner;
            let count = io
                .managed_http
                .values()
                .filter(|managed| owned(managed))
                .count();
            let mut requests = Vec::new();
            let mut pending = Vec::new();
            let reserved = requests.try_reserve_exact(count).is_ok()
                && pending.try_reserve_exact(count).is_ok()
                && io.actions.try_reserve(count * 2 + 3).is_ok();
            assert!(reserved, "read root expiry storage");
            requests.extend(
                io.managed_http
                    .iter()
                    .filter(|(_, managed)| owned(managed))
                    .map(|(request, _)| *request),
            );
            cleanup.push((owner, requests, pending));
        }
        // Mark every due root first: no cleanup can publish for another.
        for &(id, generation) in &due {
            io.request_contexts.expire_read_root(id, generation);
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
        scope.engine.io.read_roots_expiring = false;
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
            Err(reason) => return self.with_command_entry(Operation::Read, |_, _| Err(reason)),
        };
        if !self.now.is_finite() || deadline - self.now < 1. {
            return Err(CommandEntryError::Revoked);
        }
        let command = self.io.command_entry_id().expect("fresh entry");
        // Every check and reservation precedes the only spend, the root's ID.
        let checked = credential
            .as_ref()
            .map_or(Ok(()), Credential::validate)
            .and_then(|()| self.io.request_contexts.reserve_command_root(command))
            .and_then(|()| {
                if self.io.read_roots.len() >= self.io.read_roots.capacity() {
                    return Err(Error::Capacity);
                }
                self.io.actions.try_reserve(2).map_err(|_| Error::Capacity)
            });
        if let Err(reason) = checked {
            return self.with_command_entry(Operation::Read, |_, _| Err(reason));
        }
        let policy = Policy {
            operation: Operation::Read,
            credential,
            timeout_ms: (deadline - self.now).ceil().min(f64::from(u32::MAX)) as u32,
            pacing,
            retry: RetryPolicy::Never,
        };
        self.with_command_entry(Operation::Read, |cx, command| {
            let root =
                cx.make_http_context(Scope::Command(command), policy, None, Some(deadline))?;
            cx.io.request_contexts.retain_command_entry(&root);
            cx.io.request_contexts.track_read_root(&root);
            let Owner::Command { generation, .. } = root.owner() else {
                unreachable!("command scope")
            };
            cx.io.read_roots.push((deadline, command, generation));
            cx.io.rearm_read_root_timer();
            Ok(root)
        })
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
        match self.with_command_entry(operation, |_, _| Err::<(), _>(reason)) {
            Err(CommandEntryError::Rejected(_)) => Ok(()),
            other => other,
        }
    }
}
