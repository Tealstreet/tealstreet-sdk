//! Engine authority for one physical execution serving several command owners.
//! Pure state: managed HTTP, registry and timer hooks bind to it separately
//! (docs/plans/RUST_EXECUTION_GROUPS.md, "Settled API choices for review").
use crate::request_context::{Error, Operation};
use crate::{CommandId, ErrorKind, OrderIdentityReceipt, Outcome, VenueError};
use std::collections::HashMap;

pub(crate) const MAX_OBSERVERS: usize = 64;
const MAX_MEMBERS: usize = 256;
const MAX_EXECUTIONS: usize = 256;
const MAX_PROGRESS: usize = 256;
const MAX_KEY: usize = 128;

/// Opaque and never reused, so an old handle cannot reach a newer execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Execution(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Window {
    BeforeRelease,
    UntilTerminal,
}

/// A command owner as issued by the registry: exact generation and raw bound.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Member {
    pub command: CommandId,
    pub generation: u64,
    pub operation: Operation,
    pub deadline: f64,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Root {
    Command(Member),
    /// An existing background read Context keeps its own publication rights.
    Background { deadline: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Observer {
    execution: Execution,
    command: CommandId,
    generation: u64,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Joined {
    /// Retained identity progress, replayed once; it is not a new send.
    Live(Vec<OrderIdentityReceipt>),
    Already,
    /// Joined the next execution of the same background root.
    Pending,
}

/// One terminal per observer generation. `journal` means the hook must seal
/// through the shared write journal, which alone decides the outcome.
#[derive(Debug, PartialEq)]
pub(crate) struct Sealed {
    pub command: CommandId,
    pub generation: u64,
    pub result: Result<String, VenueError>,
    pub journal: bool,
}

#[derive(Clone)]
struct Entry {
    command: CommandId,
    generation: u64,
    deadline: f64,
    key: String,
}

struct Group {
    operation: Operation,
    window: Window,
    deadline: f64,
    background: bool,
    legs: usize,
    released: bool,
    terminal: bool,
    observers: Vec<Entry>,
    pending: Vec<Entry>,
    progress: Vec<OrderIdentityReceipt>,
}

#[derive(Default)]
pub(crate) struct Groups {
    next: u64,
    groups: HashMap<u64, Group>,
    members: HashMap<CommandId, (u64, u64)>,
}

impl Groups {
    pub(crate) fn begin(
        &mut self,
        root: Root,
        operation: Operation,
        window: Window,
        timeout_ms: u32,
        now: f64,
    ) -> Result<(Execution, Option<Observer>), Error> {
        let _ = (root, operation, window, timeout_ms, now);
        Err(Error::State)
    }

    pub(crate) fn begin_successor(
        &mut self,
        previous: Execution,
        root_deadline: f64,
        timeout_ms: u32,
        now: f64,
    ) -> Result<(Execution, Vec<Sealed>), Error> {
        let _ = (previous, root_deadline, timeout_ms, now);
        Err(Error::State)
    }

    pub(crate) fn join(
        &mut self,
        execution: Execution,
        member: Member,
        now: f64,
    ) -> Result<(Observer, Joined), Error> {
        let _ = (execution, member, now);
        Err(Error::State)
    }

    pub(crate) fn deadline(&self, execution: Execution) -> Option<f64> {
        let _ = execution;
        None
    }

    pub(crate) fn leg_issued(&mut self, execution: Execution) -> Result<(), Error> {
        let _ = execution;
        Err(Error::State)
    }

    pub(crate) fn leg_released(&mut self, execution: Execution) -> Result<(), Error> {
        let _ = execution;
        Err(Error::State)
    }

    pub(crate) fn leg_retired(&mut self, execution: Execution) -> Result<(), Error> {
        let _ = execution;
        Err(Error::State)
    }

    /// Store/native-cache publication is allowed only for a live owner set.
    pub(crate) fn may_publish(&self, execution: Execution) -> bool {
        let _ = execution;
        false
    }

    pub(crate) fn member(&self, command: CommandId) -> Option<(Execution, u64)> {
        let _ = command;
        None
    }

    pub(crate) fn timer(&self, execution: Execution) -> Option<f64> {
        let _ = execution;
        None
    }

    pub(crate) fn expire(&mut self, execution: Execution, now: f64) -> Vec<Sealed> {
        let _ = (execution, now);
        Vec::new()
    }

    pub(crate) fn detach(
        &mut self,
        observer: Observer,
        error: VenueError,
    ) -> Result<Sealed, Error> {
        let _ = (observer, error);
        Err(Error::State)
    }

    pub(crate) fn complete(
        &mut self,
        execution: Execution,
        project: impl FnMut(&str) -> Result<String, VenueError>,
    ) -> Result<Vec<Sealed>, Error> {
        let _ = (execution, project);
        Err(Error::State)
    }

    pub(crate) fn abort(
        &mut self,
        execution: Execution,
        error: VenueError,
    ) -> Result<Vec<Sealed>, Error> {
        let _ = (execution, error);
        Err(Error::State)
    }

    pub(crate) fn progress(
        &mut self,
        execution: Execution,
        receipts: Vec<OrderIdentityReceipt>,
    ) -> Result<Vec<(CommandId, u64, Vec<OrderIdentityReceipt>)>, Error> {
        let _ = (execution, receipts);
        Err(Error::State)
    }
}

#[cfg(test)]
#[path = "execution_group_tests.rs"]
mod tests;
