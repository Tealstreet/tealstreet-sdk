//! One engine command-callback loan before HTTP context issuance.
use super::*;
use crate::request_context::{CommandEntryError, Context, Operation, Policy, Scope};

pub(super) struct Entry {
    command: CommandId,
    resources: crate::resource_ids::ResourceIds,
    state: State,
    /// This entry's own caller-wait receipt, if it has one.
    receipt: Option<u64>,
}

/// The caller's remaining wait for the current command, as observed from its
/// genuine entry callback. Observation only: it grants no send, completion or
/// retry right, and nothing renews it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandWait {
    /// Whole milliseconds left (floored), always at least 1.
    Remaining(u32),
    /// The caller's deadline has passed, or is less than 1 ms away.
    Expired,
}

/// The stamp count at which expired, uncarried stamps are dropped. Not a hard
/// cap: unexpired waits a venue never answers stay until they expire.
const WAIT_STAMPS: usize = 4096;

/// The SDK's documented default caller wait, used when the host sends none.
pub const DEFAULT_COMMAND_WAIT_MS: u32 = 120_000;
/// The host's own bound on a caller wait.
pub const MAX_COMMAND_WAIT_MS: u32 = 2_147_483_647;
/// Absolute caller deadlines by command, each tied to the receipt that set it.
/// A held or deferred command's stamp leaves only with its own terminal, so it
/// still reads `Expired`. Past capacity, expired stamps nothing carries are
/// dropped: a venue that never answers cannot grow this without bound.
#[derive(Default)]
pub(super) struct Waits {
    /// (command, receipt, deadline); few live at once, so a scan is cheapest.
    stamps: Vec<(CommandId, u64, f64)>,
    receipts: u64,
    /// The exact receipt the current dispatch carries into its entry: an
    /// external receipt, or the one a held or deferred command arrived with.
    pending: Option<(CommandId, Option<u64>)>,
    /// Receipts of a released batch whose dispatch has not begun yet.
    batched: Vec<u64>,
}

impl Waits {
    /// Stamp one external receipt unless its command already has a live one;
    /// a duplicate never replaces it and gets no stamp of its own.
    /// An exhausted receipt sequence never wraps: the receipt goes unstamped.
    pub(super) fn receive(&mut self, command: CommandId, deadline: Option<f64>) {
        let own = if self.stamp(command).is_some() {
            None
        } else if let Some((deadline, receipt)) = deadline.zip(self.receipts.checked_add(1)) {
            self.receipts = receipt;
            self.stamps.push((command, receipt, deadline));
            Some(receipt)
        } else {
            None
        };
        self.pending = Some((command, own));
    }

    pub(super) fn end_receipt(&mut self) {
        self.pending = None;
    }

    /// The receipt `command` carries in the current dispatch; never inferred
    /// from another receipt's live stamp for the same id.
    fn entry_receipt(&self, command: CommandId) -> Option<u64> {
        match self.pending {
            Some((pending, own)) if pending == command => own,
            _ => None,
        }
    }

    /// The receipt to keep with `command` while it is held or deferred.
    pub(super) fn receipt_of(&self, command: &Command) -> Option<u64> {
        command
            .completion_id()
            .and_then(|command| self.entry_receipt(command))
    }

    /// Dispatch a held or deferred command with the receipt it arrived with;
    /// returns the outer dispatch's provenance for the caller to restore.
    pub(super) fn carry(
        &mut self,
        pending: Option<(CommandId, Option<u64>)>,
    ) -> Option<(CommandId, Option<u64>)> {
        if let Some((_, Some(receipt))) = pending
            && let Some(index) = self.batched.iter().position(|held| *held == receipt)
        {
            self.batched.swap_remove(index);
        }
        std::mem::replace(&mut self.pending, pending)
    }

    /// A released batch's receipts stay carried until each one's dispatch.
    pub(super) fn batch(&mut self, receipt: Option<u64>) {
        self.batched.extend(receipt);
    }

    /// Remove `command`'s stamp at its terminal. Inside its entry only that
    /// entry's own receipt may be removed, so a duplicate cannot clear it.
    /// Outside an entry, a stamp a held or deferred command still carries is
    /// kept: that command has not run, so the terminal is another receipt's.
    fn finish(&mut self, command: CommandId, entry_receipt: Option<Option<u64>>, carried: bool) {
        let matches = self
            .stamp(command)
            .is_some_and(|(receipt, _)| match entry_receipt {
                Some(own) => own == Some(receipt),
                None => !carried,
            });
        if matches && let Some(index) = self.stamps.iter().position(|(held, ..)| *held == command) {
            self.stamps.swap_remove(index);
        }
    }

    /// `command`'s live (receipt, deadline).
    pub(super) fn stamp(&self, command: CommandId) -> Option<(u64, f64)> {
        self.stamps
            .iter()
            .find(|(held, ..)| *held == command)
            .map(|(_, receipt, deadline)| (*receipt, *deadline))
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.stamps.len()
    }

    #[cfg(test)]
    pub(super) fn exhaust_receipts(&mut self) {
        self.receipts = u64::MAX;
    }

    #[cfg(test)]
    pub(super) fn deadline(&self, command: CommandId) -> Option<f64> {
        self.stamp(command).map(|(_, at)| at)
    }
}

/// A finite caller deadline strictly after `now`.
pub(super) fn wait_deadline(now: f64, wait_ms: u32) -> Option<f64> {
    let deadline = now + f64::from(wait_ms);
    (now.is_finite() && deadline.is_finite() && deadline > now).then_some(deadline)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Fresh,
    Bound,
    Legacy,
    Rejected,
    Spent,
}

impl<P> Io<P> {
    pub(super) fn begin_command_entry(&mut self, command: Option<CommandId>) {
        assert!(self.command_entry.is_none(), "nested native command entry");
        self.command_entry = command.map(|command| Entry {
            command,
            resources: self.next_id,
            state: State::Fresh,
            receipt: self.command_waits.entry_receipt(command),
        });
    }

    /// The command's terminal ends its caller wait (exact receipt inside its
    /// own entry; the live command's otherwise).
    pub(super) fn finish_command_wait(&mut self, command: CommandId) {
        let entry = self
            .command_entry
            .as_ref()
            .filter(|entry| entry.command == command)
            .map(|entry| entry.receipt);
        let carried = self
            .command_waits
            .stamp(command)
            .is_some_and(|(receipt, _)| self.carries(receipt));
        self.command_waits.finish(command, entry, carried);
    }

    /// Whether a held cancellation, deferred cancel-all or released batch
    /// still carries `receipt`.
    fn carries(&self, receipt: u64) -> bool {
        #[cfg(feature = "order-locks")]
        if self.order_locks.carries(receipt) {
            return true;
        }
        self.placement_acks.carries(receipt) || self.command_waits.batched.contains(&receipt)
    }

    /// Stamp one external receipt; past capacity, first drop expired stamps
    /// that no held or deferred command carries.
    pub(super) fn receive_command_wait(
        &mut self,
        command: CommandId,
        deadline: Option<f64>,
        now: f64,
    ) {
        if self.command_waits.stamps.len() >= WAIT_STAMPS {
            let mut index = 0;
            while let Some(&(_, receipt, at)) = self.command_waits.stamps.get(index) {
                if at > now || self.carries(receipt) {
                    index += 1;
                } else {
                    self.command_waits.stamps.swap_remove(index);
                }
            }
        }
        self.command_waits.receive(command, deadline);
    }
    pub(super) fn end_command_entry(&mut self) {
        self.command_entry = None;
    }
    /// Existing resources are not a second ID allocator. Changing their state,
    /// or staging an outbound operation without a new ID, burns fresh custody.
    pub(super) fn opaque_entry_activity(&mut self) {
        if let Some(entry) = &mut self.command_entry
            && entry.state == State::Fresh
        {
            entry.state = State::Legacy;
        }
    }
    pub(super) fn numeric_entry_completion_allowed(&self, command: CommandId) -> bool {
        self.command_entry.as_ref().is_none_or(|entry| {
            entry.command != command || matches!(entry.state, State::Fresh | State::Legacy)
        })
    }
    pub(super) fn entry_context_allowed(&self, scope: Scope) -> bool {
        self.command_entry.as_ref().is_none_or(|entry| {
            scope != Scope::Command(entry.command)
                || !matches!(entry.state, State::Rejected | State::Spent)
        })
    }
    /// Read-only: this exact command's genuine callback holds its issued root.
    pub(super) fn entry_bound(&self, command: CommandId) -> bool {
        self.command_entry
            .as_ref()
            .is_some_and(|entry| entry.command == command && entry.state == State::Bound)
    }
    pub(super) fn entry_completed(&mut self, command: CommandId) {
        if let Some(entry) = &mut self.command_entry
            && entry.command == command
        {
            entry.state = State::Spent;
        }
    }
}

impl<P> Cx<'_, P> {
    /// An engine refusal before the venue's entry ends under this receipt's
    /// own entry, so a duplicate id never clears the live command's wait.
    pub(super) fn refuse_before_entry(&mut self, command: CommandId, error: &str) {
        self.io.begin_command_entry(Some(command));
        self.fail(command, error);
        self.io.end_command_entry();
    }

    /// The caller's remaining wait, only inside this command's genuine entry
    /// callback (before or after admission, never once rejected or spent)
    /// and only for its own receipt. `None` in every borrowed callback.
    pub fn command_wait(&self) -> Option<CommandWait> {
        let entry = self.io.command_entry.as_ref()?;
        if self.io.http_delivery.is_some()
            || !matches!(entry.state, State::Fresh | State::Bound | State::Legacy)
        {
            return None;
        }
        let (receipt, deadline) = self.io.command_waits.stamp(entry.command)?;
        if entry.receipt != Some(receipt) || !self.now.is_finite() {
            return None;
        }
        let left = (deadline - self.now).floor();
        Some(if left >= 1.0 {
            CommandWait::Remaining(left.min(f64::from(u32::MAX)) as u32)
        } else {
            CommandWait::Expired
        })
    }

    /// Finish an expired Fresh entry through the shared entry rejection: one
    /// typed Timeout terminal (outcome NotSent for a mutation, none for a
    /// read) with no admission, Context, request or timer. `Ok` means that
    /// terminal was emitted and the command is finished (unlike admission's
    /// `Ok`). `Revoked` and no effect unless the wait is `Expired` and Fresh.
    pub fn reject_expired_command(
        &mut self,
        operation: Operation,
    ) -> Result<(), CommandEntryError> {
        if self.command_wait() != Some(CommandWait::Expired) {
            return Err(CommandEntryError::Revoked);
        }
        match self.with_command_entry(operation, |_, _| {
            Err::<(), _>(crate::request_context::Error::TimedOut)
        }) {
            Err(CommandEntryError::Rejected(_)) => Ok(()),
            Err(error) => Err(error),
            Ok(()) => unreachable!("expiry never admits"),
        }
    }

    /// Admit the first HTTP Context of this genuine engine command callback.
    /// Rejected already emitted one failure; do not finalize it a second time.
    /// This is initial admission only, never authority for siblings or retries.
    pub fn admit_http_command(&mut self, policy: Policy) -> Result<Context, CommandEntryError> {
        self.with_command_entry(policy.operation, |cx, command| {
            let context = cx.http_context(Scope::Command(command), policy)?;
            cx.io.request_contexts.retain_command_entry(&context);
            Ok(context)
        })
    }

    /// One initial command transaction shared by Context and opaque Preflight
    /// issuers. Only this callback loan can reject an unissued command.
    pub(in crate::engine) fn with_command_entry<T>(
        &mut self,
        operation: Operation,
        issue: impl FnOnce(&mut Self, CommandId) -> Result<T, crate::request_context::Error>,
    ) -> Result<T, CommandEntryError> {
        let Some(entry) = &mut self.io.command_entry else {
            return Err(CommandEntryError::Revoked);
        };
        if entry.state != State::Fresh {
            return Err(CommandEntryError::Revoked);
        }
        if entry.resources != self.io.next_id || self.io.http_delivery.is_some() {
            entry.state = State::Legacy;
            return Err(CommandEntryError::Revoked);
        }
        let command = entry.command;
        if !self
            .io
            .request_contexts
            .prepare_command_entry(command, self.now)
        {
            self.io.command_entry.as_mut().unwrap().state = State::Spent;
            return Err(CommandEntryError::Revoked);
        }
        // Suspend the fresh loan before ordinary issuance reserves an ID. Only
        // this exact transaction may produce its success or unissued rejection.
        self.io.command_entry.as_mut().unwrap().state = State::Bound;
        match issue(self, command) {
            Ok(owner) => Ok(owner),
            Err(reason) => {
                // Typed issuance errors occur before owner insertion. Never
                // erase a previous owner or leave opaque work runnable.
                assert!(self.io.request_contexts.command_entry_unowned(command));
                let entry = self
                    .io
                    .command_entry
                    .as_mut()
                    .expect("active command entry");
                assert!(entry.command == command && entry.state == State::Bound);
                entry.state = State::Rejected;
                let mut error = reason.venue_error();
                if operation == Operation::Read {
                    error.outcome = None;
                }
                #[cfg(feature = "order-locks")]
                {
                    self.io.order_locks.finish(command, true);
                    self.io.active_commands.remove(&command);
                }
                self.io.finish_command_wait(command);
                self.io
                    .actions
                    .push(Action::Notice(Notice::CommandFailed { command, error }));
                Err(CommandEntryError::Rejected(reason))
            }
        }
    }
}
