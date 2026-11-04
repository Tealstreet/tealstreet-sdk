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

#[cfg(feature = "write-preflight-wait")]
impl<P> Io<P> {
    /// A parked write Preflight's cap timer; an early or non-finite fire
    /// re-arms for the exact cap (true: consumed here), so a host's one wake
    /// is never spent without the cap staying armed.
    pub(super) fn write_preflight_timer(&mut self, timer: TimerId, now: f64) -> bool {
        let Some(&(at_ms, ..)) = self
            .core
            .write_preflight_waits
            .iter()
            .find(|(.., own)| *own == timer)
        else {
            return false;
        };
        if !now.is_finite() || now < at_ms {
            self.core.actions.push(Action::SetTimer { timer, at_ms });
        }
        true
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
}

// Concrete metadata is shared across venue engines; move-only continuations
// remain typed and are reserved separately before any owner is marked.
struct ReadRootCleanup {
    owner: Owner,
    requests: Vec<RequestId>,
}

impl IoCore {
    #[inline(never)]
    fn prepare_read_root_sweep(&self, now: f64) -> Vec<ReadRootCleanup> {
        let mut cleanup = Vec::new();
        assert!(
            cleanup.try_reserve_exact(self.read_roots.len()).is_ok(),
            "read root expiry storage"
        );
        for &(_, id, generation, _) in self.read_roots.iter().filter(|(at, ..)| now >= *at) {
            let owner = Owner::Command { id, generation };
            let owned =
                |managed: &http_lifecycle::Managed| managed.delivery().context.owner() == owner;
            let count = self
                .managed_http
                .values()
                .filter(|managed| owned(managed))
                .count();
            let mut requests = Vec::new();
            assert!(
                requests.try_reserve_exact(count).is_ok(),
                "read root expiry storage"
            );
            requests.extend(
                self.managed_http
                    .iter()
                    .filter(|(_, managed)| owned(managed))
                    .map(|(request, _)| *request),
            );
            cleanup.push(ReadRootCleanup { owner, requests });
        }
        cleanup
    }

    #[inline(never)]
    fn mark_read_root_sweep(&mut self, cleanup: &[ReadRootCleanup]) {
        // Per request: timer clear and cancel. Per owner: its timer clear,
        // terminal notice and any classified error notice.
        let actions = cleanup.iter().fold(0_usize, |actions, root| {
            actions.saturating_add(root.requests.len().saturating_mul(2).saturating_add(3))
        });
        assert!(
            self.actions.try_reserve(actions).is_ok(),
            "read root expiry storage"
        );
        #[cfg(test)]
        {
            self.read_root_sweep_capacity = Some(self.actions.capacity());
        }
        for root in cleanup {
            let Owner::Command { id, generation } = root.owner else {
                unreachable!()
            };
            self.request_contexts.expire_read_root(id, generation);
        }
    }
}

#[cfg(feature = "write-preflight-wait")]
impl CoreCx<'_> {
    /// A parked write Preflight past its caller's cap fails exactly as an
    /// expired entry does, through the owner-aware Preflight seal; one that left
    /// its park is no longer bounded. Runs outside every command loan.
    #[inline(never)]
    fn expire_write_preflights(&mut self, now: f64) {
        if self.io.core.write_preflight_waits.is_empty() || !now.is_finite() {
            return;
        }
        let mut index = 0;
        while index < self.io.core.write_preflight_waits.len() {
            let (at_ms, preflight, timer) = self.io.core.write_preflight_waits[index].clone();
            let waiting = self.io.core.request_contexts.preflight_waiting(&preflight);
            if waiting && now < at_ms {
                index += 1;
                continue;
            }
            // Seal first: the cap leaves the index only once the owner has its
            // terminal, so a refused seal can never strand a parked caller.
            if waiting {
                let sealed = self.fail_preflight(&preflight, Error::TimedOut.venue_error());
                if sealed.is_err() {
                    debug_assert!(false, "a due write Preflight cap must seal: {sealed:?}");
                    index += 1;
                    continue;
                }
            }
            self.io.core.write_preflight_waits.swap_remove(index);
            self.io.core.actions.push(Action::ClearTimer { timer });
        }
    }
}

impl<V: Venue> Engine<V> {
    /// Retire every tracked root due at `now` before anything can stamp, run a
    /// codec or lend authority: prepare all, mark all, then per owner discard,
    /// hand back its continuations without a Cx, and seal once.
    pub(super) fn expire_read_roots(&mut self, now: f64) {
        #[cfg(feature = "write-preflight-wait")]
        Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        }
        .core()
        .expire_write_preflights(now);
        if self.io.core.read_roots.is_empty()
            || !now.is_finite()
            || !self.io.core.read_roots.iter().any(|(at, ..)| now >= *at)
        {
            return;
        }
        self.io.core.read_roots_expiring = true;
        let scope = SweepScope { engine: self };
        let io = &mut scope.engine.io;
        let mut pending = Vec::new();
        assert!(
            pending.try_reserve_exact(io.core.read_roots.len()).is_ok(),
            "read root expiry storage"
        );
        let cleanup = io.core.prepare_read_root_sweep(now);
        // Typed continuations stay here; reserve every owner's capacity before
        // the shared mark-all phase can retire any authority.
        for root in &cleanup {
            let mut values = Vec::new();
            assert!(
                values.try_reserve_exact(root.requests.len()).is_ok(),
                "read root expiry storage"
            );
            pending.push(values);
        }
        io.core.mark_read_root_sweep(&cleanup);
        for (root, mut pending) in cleanup.iter().zip(pending) {
            let owner = root.owner;
            let Owner::Command { id, generation } = owner else {
                unreachable!()
            };
            for &request in &root.requests {
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
    /// Settle a tracked root whose answer is another owner's published read,
    /// from that read's own delivery: the delivery lends this root no authority.
    pub fn settle_caller_read_root(
        &mut self,
        root: &Context,
        result: Result<String, VenueError>,
    ) -> Result<(), Error> {
        let Owner::Command { id, .. } = root.owner() else {
            return Err(Error::Invalid);
        };
        if !self.io.core.request_contexts.tracked_owner(root) {
            return Err(Error::Invalid);
        }
        let delivery = self.io.core.http_delivery.take();
        let settled = self.finish_command(id, result, Some(root));
        self.io.core.http_delivery = delivery;
        settled
    }

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
