//! Engine authority for one physical execution serving several command owners.
//! Pure state: managed HTTP, registry and timer hooks bind to it separately
//! (docs/plans/RUST_EXECUTION_GROUPS.md, "Settled API choices for review").
use crate::request_context::{Error, Operation};
use crate::{CommandId, ErrorKind, OrderIdentityReceipt, Outcome, VenueError};

pub(crate) const MAX_OBSERVERS: usize = 64;
const MAX_MEMBERS: usize = 256;
#[cfg_attr(not(test), allow(dead_code))] // Background roots and writes: Bitrue/Hyro phases.
const MAX_PROGRESS: usize = 256;
const MAX_KEY: usize = 128;

/// Opaque and never reused, so an old handle cannot reach a newer execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Execution(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
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
    Background {
        deadline: f64,
    },
    /// An admitted Plan's physical owner: its root bounds the group and only
    /// its seal completes it. The founder is the first ordinary observer.
    #[cfg_attr(not(feature = "http-plans"), allow(dead_code))]
    Planned {
        deadline: f64,
        founder: Member,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Observer {
    execution: Execution,
    command: CommandId,
    generation: u64,
}

#[derive(Debug, PartialEq)]
pub enum Joined {
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
    /// Carries the Plan journal's own projection; the observer has no journal.
    pub projected: bool,
    /// A planned observer sealed before its Plan: the registry substitutes
    /// the physical journal's non-consuming projection for its outcome.
    #[cfg_attr(not(feature = "http-plans"), allow(dead_code))]
    pub early: Option<Execution>,
}

#[derive(Clone)]
struct Entry {
    /// The execution the owner joined; kept across successor adoption.
    token: Execution,
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
    planned: bool,
    legs: usize,
    released: bool,
    terminal: bool,
    observers: Vec<Entry>,
    pending: Vec<Entry>,
    progress: Vec<OrderIdentityReceipt>,
    /// Retained progress overflowed, so a late joiner could miss identities.
    truncated: bool,
    /// A successor adopted this execution's queue; it admits no more joiners.
    succeeded: bool,
}

#[derive(Default)]
pub(crate) struct Groups {
    next: u64,
    groups: crate::IdMap<u64, Group>,
    members: std::collections::HashMap<CommandId, (u64, u64)>,
}

impl Entry {
    fn new(member: Member, token: Execution, cap: f64, now: f64) -> Result<Self, Error> {
        if !now.is_finite() || !member.deadline.is_finite() || member.key.len() > MAX_KEY {
            return Err(Error::Invalid);
        }
        if member.deadline.min(cap) <= now {
            return Err(Error::TimedOut);
        }
        Ok(Self {
            token,
            command: member.command,
            generation: member.generation,
            deadline: member.deadline.min(cap),
            key: member.key,
        })
    }
}

impl Group {
    fn live(&self) -> usize {
        self.observers.len() + self.pending.len()
    }

    /// Message and kind come from the caller. A write outcome belongs to the
    /// shared journal unless this seal proves nothing was or will be sent.
    fn seal(&self, entry: Entry, result: Result<String, VenueError>, settled: bool) -> Sealed {
        let journal = self.operation == Operation::Mutation;
        // A planned group never sees its Plan's releases; only the Plan seal
        // (`complete`) knows the write outcome.
        let keep = !self.planned && settled && !self.released && self.legs == 0;
        let result = result.map_err(|mut error| {
            if journal && !keep {
                error.outcome = None;
            }
            error
        });
        Sealed {
            command: entry.command,
            generation: entry.generation,
            result,
            journal,
            projected: self.planned,
            early: self.planned.then_some(entry.token),
        }
    }

    fn timeout(&self) -> VenueError {
        let error = VenueError::new("Execution deadline expired").kind(ErrorKind::Timeout);
        if self.operation == Operation::Mutation && !self.released && !self.planned {
            error.outcome(Outcome::NotSent)
        } else {
            error
        }
    }
}

impl Groups {
    fn issue(&mut self) -> Result<u64, Error> {
        let id = self.next.checked_add(1).ok_or(Error::Capacity)?;
        self.next = id;
        Ok(id)
    }

    fn group(&self, execution: Execution) -> Result<&Group, Error> {
        self.groups.get(&execution.0).ok_or(Error::Revoked)
    }

    fn open(&mut self, execution: Execution) -> Result<&mut Group, Error> {
        self.groups
            .get_mut(&execution.0)
            .filter(|group| !group.terminal)
            .ok_or(Error::Revoked)
    }

    fn admit(&self, member: &Member) -> Result<(), Error> {
        if self.members.contains_key(&member.command) {
            return Err(Error::State);
        }
        if self.members.len() >= MAX_MEMBERS {
            return Err(Error::Capacity);
        }
        Ok(())
    }

    fn retire(&mut self, id: u64) {
        if self
            .groups
            .get(&id)
            .is_some_and(|group| group.terminal && group.legs == 0 && group.pending.is_empty())
        {
            self.groups.remove(&id);
        }
    }

    fn release_members(&mut self, sealed: &[Sealed]) {
        for seal in sealed {
            if self.members.get(&seal.command).map(|(_, g)| *g) == Some(seal.generation) {
                self.members.remove(&seal.command);
            }
        }
    }

    pub(crate) fn begin(
        &mut self,
        root: Root,
        operation: Operation,
        window: Window,
        timeout_ms: u32,
        now: f64,
    ) -> Result<(Execution, Option<Observer>), Error> {
        if !now.is_finite() || timeout_ms == 0 {
            return Err(Error::Invalid);
        }
        let bound = now + f64::from(timeout_ms);
        let planned = matches!(root, Root::Planned { .. });
        let (deadline, founder, background) = match root {
            Root::Command(member) => {
                if member.operation != operation {
                    return Err(Error::Invalid);
                }
                // The token is fixed below once the identity is issued.
                let entry = Entry::new(member.clone(), Execution(0), bound, now)?;
                self.admit(&member)?;
                (entry.deadline, Some(entry), false)
            }
            Root::Background { deadline } => {
                if operation != Operation::Read || !deadline.is_finite() {
                    return Err(Error::Invalid);
                }
                if deadline <= now {
                    return Err(Error::TimedOut);
                }
                (deadline.min(bound), None, true)
            }
            Root::Planned { deadline, founder } => {
                if window != Window::UntilTerminal
                    || !deadline.is_finite()
                    || founder.operation != operation
                {
                    return Err(Error::Invalid);
                }
                let entry = Entry::new(founder.clone(), Execution(0), deadline, now)?;
                self.admit(&founder)?;
                (deadline, Some(entry), false)
            }
        };
        let id = self.issue()?;
        let execution = Execution(id);
        let founder = founder.map(|entry| Entry {
            token: execution,
            ..entry
        });
        let observer = founder.as_ref().map(|entry| {
            self.members.insert(entry.command, (id, entry.generation));
            Observer {
                execution,
                command: entry.command,
                generation: entry.generation,
            }
        });
        self.groups.insert(
            id,
            Group {
                operation,
                window,
                deadline,
                background,
                planned,
                legs: 0,
                released: false,
                terminal: false,
                observers: founder.into_iter().collect(),
                pending: Vec::new(),
                progress: Vec::new(),
                truncated: false,
                succeeded: false,
            },
        );
        Ok((execution, observer))
    }

    /// The next read of the same background root adopts its queued joiners.
    pub(crate) fn begin_successor(
        &mut self,
        previous: Execution,
        root_deadline: f64,
        timeout_ms: u32,
        now: f64,
    ) -> Result<(Execution, Vec<Sealed>), Error> {
        let old = self.group(previous)?;
        if !old.background {
            return Err(Error::Invalid);
        }
        if !old.released || old.succeeded {
            return Err(Error::State);
        }
        let window = old.window;
        let (execution, _) = self.begin(
            Root::Background {
                deadline: root_deadline,
            },
            Operation::Read,
            window,
            timeout_ms,
            now,
        )?;
        let old = self.groups.get_mut(&previous.0).expect("validated above");
        old.succeeded = true;
        let queued = std::mem::take(&mut old.pending);
        let group = self.groups.get_mut(&execution.0).expect("just issued");
        let mut sealed = Vec::new();
        for mut entry in queued {
            if entry.deadline <= now {
                sealed.push(group.seal(entry, Err(group.timeout()), false));
                continue;
            }
            entry.deadline = entry.deadline.min(group.deadline);
            self.members
                .insert(entry.command, (execution.0, entry.generation));
            group.observers.push(entry);
        }
        self.release_members(&sealed);
        self.retire(previous.0);
        Ok((execution, sealed))
    }

    pub(crate) fn join(
        &mut self,
        execution: Execution,
        member: Member,
        now: f64,
    ) -> Result<(Observer, Joined), Error> {
        self.open(execution)?;
        let observer = Observer {
            execution,
            command: member.command,
            generation: member.generation,
        };
        if let Some(&(id, generation)) = self.members.get(&member.command) {
            return if id == execution.0 && generation == member.generation {
                Ok((observer, Joined::Already))
            } else {
                Err(Error::State)
            };
        }
        let group = self.groups.get(&execution.0).expect("open above");
        if group.succeeded {
            return Err(Error::State);
        }
        if member.operation != group.operation {
            return Err(Error::Invalid);
        }
        let pending = group.window == Window::BeforeRelease && group.released;
        if pending && !group.background {
            return Err(Error::State);
        }
        let cap = if pending {
            f64::INFINITY
        } else {
            group.deadline
        };
        let entry = Entry::new(member.clone(), execution, cap, now)?;
        self.admit(&member)?;
        let group = self.groups.get_mut(&execution.0).expect("open above");
        if group.live() >= MAX_OBSERVERS || (!pending && group.truncated) {
            return Err(Error::Capacity);
        }
        self.members
            .insert(entry.command, (execution.0, entry.generation));
        if pending {
            group.pending.push(entry);
            return Ok((observer, Joined::Pending));
        }
        group.observers.push(entry);
        Ok((observer, Joined::Live(group.progress.clone())))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Earliest timer across every held execution, without allocating.
    pub(crate) fn next_timer(&self) -> Option<f64> {
        self.groups
            .keys()
            .filter_map(|id| self.timer(Execution(*id)))
            .min_by(f64::total_cmp)
    }

    /// Every held execution in issue order, retired or not.
    pub(crate) fn executions(&self) -> Vec<Execution> {
        let mut ids = self.groups.keys().copied().collect::<Vec<_>>();
        ids.sort_unstable();
        ids.into_iter().map(Execution).collect()
    }

    pub(crate) fn deadline(&self, execution: Execution) -> Option<f64> {
        self.group(execution).ok().map(|group| group.deadline)
    }

    pub(crate) fn leg_issued(&mut self, execution: Execution) -> Result<(), Error> {
        let group = self.open(execution)?;
        if group.planned || (!group.background && group.observers.is_empty()) {
            return Err(Error::Revoked);
        }
        group.legs = group.legs.checked_add(1).ok_or(Error::Capacity)?;
        Ok(())
    }

    pub(crate) fn leg_released(&mut self, execution: Execution) -> Result<(), Error> {
        let group = self.groups.get_mut(&execution.0).ok_or(Error::Revoked)?;
        if group.legs == 0 {
            return Err(Error::State);
        }
        group.released = true;
        Ok(())
    }

    pub(crate) fn leg_retired(&mut self, execution: Execution) -> Result<(), Error> {
        let group = self.groups.get_mut(&execution.0).ok_or(Error::Revoked)?;
        group.legs = group.legs.checked_sub(1).ok_or(Error::State)?;
        self.retire(execution.0);
        Ok(())
    }

    /// Store/native-cache publication is allowed only for a live owner set.
    pub(crate) fn may_publish(&self, execution: Execution) -> bool {
        self.group(execution)
            .is_ok_and(|group| !group.terminal && (group.background || !group.observers.is_empty()))
    }

    /// The checks `begin`/`join` would make for this member, without state.
    #[cfg_attr(not(feature = "http-plans"), allow(dead_code))]
    pub(crate) fn admissible(&self, command: CommandId, key: &str) -> Result<(), Error> {
        if key.len() > MAX_KEY {
            return Err(Error::Invalid);
        }
        if self.members.contains_key(&command) {
            return Err(Error::State);
        }
        if self.members.len() >= MAX_MEMBERS {
            return Err(Error::Capacity);
        }
        Ok(())
    }

    /// Live observers of one execution; none once it is terminal.
    #[cfg_attr(not(feature = "http-plans"), allow(dead_code))]
    pub(crate) fn observer_count(&self, execution: Execution) -> usize {
        self.groups
            .get(&execution.0)
            .filter(|group| !group.terminal)
            .map_or(0, |group| group.observers.len())
    }

    /// Live observer commands of one execution, in join order.
    #[cfg_attr(
        not(all(feature = "http-plans", feature = "order-locks")),
        allow(dead_code)
    )]
    pub(crate) fn observer_commands(&self, execution: Execution) -> Vec<CommandId> {
        self.group(execution).map_or_else(
            |_| Vec::new(),
            |group| group.observers.iter().map(|entry| entry.command).collect(),
        )
    }

    pub(crate) fn planned(&self, execution: Execution) -> bool {
        self.group(execution).is_ok_and(|group| group.planned)
    }

    pub(crate) fn has_observers(&self, execution: Execution) -> bool {
        self.group(execution)
            .is_ok_and(|group| !group.observers.is_empty())
    }

    #[cfg_attr(not(test), allow(dead_code))] // Background roots and writes: Bitrue/Hyro phases.
    pub(crate) fn member(&self, command: CommandId) -> Option<(Execution, u64)> {
        self.members
            .get(&command)
            .map(|&(id, generation)| (Execution(id), generation))
    }

    pub(crate) fn timer(&self, execution: Execution) -> Option<f64> {
        let group = self.group(execution).ok()?;
        let observers = group
            .observers
            .iter()
            .filter(|_| !group.terminal)
            .map(|entry| entry.deadline)
            .filter(|deadline| *deadline < group.deadline);
        let pending = group.pending.iter().map(|entry| entry.deadline);
        // A Plan's own timer owns its root.
        let execution_term =
            (!group.terminal && !group.planned && group.legs == 0).then_some(group.deadline);
        observers
            .chain(pending)
            .chain(execution_term)
            .min_by(f64::total_cmp)
    }

    pub(crate) fn expire(&mut self, execution: Execution, now: f64) -> Vec<Sealed> {
        if !now.is_finite() {
            return Vec::new();
        }
        let Some(group) = self.groups.get_mut(&execution.0) else {
            return Vec::new();
        };
        let mut sealed = Vec::new();
        let timeout = group.timeout();
        // The execution bound wins over a same-instant leg timeout, so owners
        // get the typed timeout before the leg's own failure is delivered.
        let closes = !group.terminal && !group.planned && now >= group.deadline;
        let (expired, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut group.observers)
            .into_iter()
            .partition(|entry| {
                closes || (entry.deadline < group.deadline && entry.deadline <= now)
            });
        group.observers = kept;
        let (expired_pending, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut group.pending)
            .into_iter()
            .partition(|entry| entry.deadline <= now);
        group.pending = kept;
        if closes || (!group.background && group.observers.is_empty()) {
            group.terminal = true;
        }
        let settled = group.terminal;
        for entry in expired {
            sealed.push(group.seal(entry, Err(timeout.clone()), settled));
        }
        for entry in expired_pending {
            sealed.push(group.seal(entry, Err(timeout.clone()), false));
        }
        self.release_members(&sealed);
        self.retire(execution.0);
        sealed
    }

    pub(crate) fn detach(
        &mut self,
        observer: Observer,
        error: VenueError,
    ) -> Result<Sealed, Error> {
        let Some(&(id, generation)) = self.members.get(&observer.command) else {
            return Err(Error::Revoked);
        };
        if generation != observer.generation {
            return Err(Error::Revoked);
        }
        let group = self.groups.get_mut(&id).ok_or(Error::Revoked)?;
        let found = |entries: &mut Vec<Entry>| {
            entries
                .iter()
                .position(|entry| {
                    entry.command == observer.command && entry.token == observer.execution
                })
                .map(|at| entries.remove(at))
        };
        let (entry, queued) = match found(&mut group.observers) {
            Some(entry) => (entry, false),
            None => (found(&mut group.pending).ok_or(Error::Revoked)?, true),
        };
        if !queued && !group.background && group.observers.is_empty() {
            group.terminal = true;
        }
        let sealed = group.seal(entry, Err(error), !queued && group.terminal);
        self.members.remove(&observer.command);
        self.retire(id);
        Ok(sealed)
    }

    pub(crate) fn complete(
        &mut self,
        execution: Execution,
        project: impl FnMut(&str) -> Result<String, VenueError>,
    ) -> Result<Vec<Sealed>, Error> {
        let mut sealed = Vec::new();
        self.complete_into(execution, project, &mut sealed)?;
        Ok(sealed)
    }

    /// `complete` into caller-reserved storage: with capacity for every live
    /// observer, sealing moves values and allocates nothing.
    pub(crate) fn complete_into(
        &mut self,
        execution: Execution,
        mut project: impl FnMut(&str) -> Result<String, VenueError>,
        sealed: &mut Vec<Sealed>,
    ) -> Result<(), Error> {
        let group = self.open(execution).map_err(|_| Error::Stale)?;
        group.terminal = true;
        let start = sealed.len();
        for entry in std::mem::take(&mut group.observers) {
            let result = project(&entry.key);
            sealed.push(if group.planned {
                // The Plan journal's own projection, verbatim.
                Sealed {
                    command: entry.command,
                    generation: entry.generation,
                    result,
                    journal: group.operation == Operation::Mutation,
                    projected: true,
                    early: None,
                }
            } else {
                group.seal(entry, result, true)
            });
        }
        self.release_members(&sealed[start..]);
        self.retire(execution.0);
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))] // Background roots and writes: Bitrue/Hyro phases.
    pub(crate) fn abort(
        &mut self,
        execution: Execution,
        error: VenueError,
    ) -> Result<Vec<Sealed>, Error> {
        self.complete(execution, |_| Err(error.clone()))
    }

    #[cfg_attr(not(test), allow(dead_code))] // Background roots and writes: Bitrue/Hyro phases.
    pub(crate) fn progress(
        &mut self,
        execution: Execution,
        receipts: Vec<OrderIdentityReceipt>,
    ) -> Result<Vec<(CommandId, u64, Vec<OrderIdentityReceipt>)>, Error> {
        let group = self.open(execution)?;
        // Live owners always receive identities; only late-join replay is bounded.
        if group.truncated || group.progress.len() + receipts.len() > MAX_PROGRESS {
            group.truncated = true;
        } else {
            group.progress.extend(receipts.iter().cloned());
        }
        Ok(group
            .observers
            .iter()
            .map(|entry| (entry.command, entry.generation, receipts.clone()))
            .collect())
    }
}

#[cfg(test)]
#[path = "execution_group_tests.rs"]
mod tests;
