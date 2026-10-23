//! One engine command-callback loan before HTTP context issuance.
use super::*;
use crate::request_context::{CommandEntryError, Context, Operation, Policy, Scope};

pub(super) struct Entry {
    command: CommandId,
    resources: crate::resource_ids::ResourceIds,
    state: State,
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
        });
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
                self.io
                    .actions
                    .push(Action::Notice(Notice::CommandFailed { command, error }));
                Err(CommandEntryError::Rejected(reason))
            }
        }
    }
}
