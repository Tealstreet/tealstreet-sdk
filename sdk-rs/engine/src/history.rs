//! Command-owned history completion. Venues supply rows and endpoint facts;
//! this path owns semantic completeness and its ordering before the result.
use crate::log::code;
use crate::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use crate::{CommandId, Cx, ErrorKind, Level, Value, VenueError};

/// One query owner, retained across all pages and composite stages. Consuming
/// completion prevents finishing this owner twice; legacy `Cx` APIs still exist
/// for operations which have not migrated.
#[must_use = "history must finish or fail through its owner"]
pub struct Completion {
    command: CommandId,
    incomplete: Option<String>,
}

impl Completion {
    pub fn new(command: CommandId) -> Self {
        Self {
            command,
            incomplete: None,
        }
    }

    pub fn command(&self) -> CommandId {
        self.command
    }

    /// Retain the first missing-coverage cause across later successful pages.
    /// Nothing is published if this query subsequently fails.
    pub fn incomplete(&mut self, message: impl Into<String>) {
        if self.incomplete.is_none() {
            self.incomplete = Some(message.into());
        }
    }

    /// Serialization failures belong in `result` too, so they cannot publish
    /// retained rows or claim successful completion.
    pub fn finish<P>(self, cx: &mut Cx<'_, P>, result: Result<String, VenueError>) {
        match result {
            Ok(result) => {
                if let Some(message) = self.incomplete {
                    cx.event(
                        Level::Warn,
                        code::HISTORY_INCOMPLETE,
                        message,
                        vec![("command", Value::Num(f64::from(self.command)))],
                    );
                }
                cx.done(self.command, result);
            }
            Err(error) => cx.fail(self.command, error),
        }
    }
}

/// Cursor paging and its completion share the same owner. Stalls record missing
/// coverage here; a venue never constructs marker fields or a replacement owner.
#[must_use = "cursor history must finish or fail through its owner"]
pub struct CursorHistory {
    completion: Completion,
    walk: CursorWalk,
    stall_message: fn(Stall) -> String,
    finished: bool,
}

impl CursorHistory {
    pub fn new(
        completion: Completion,
        walk: CursorWalk,
        stall_message: fn(Stall) -> String,
    ) -> Self {
        Self {
            completion,
            walk,
            stall_message,
            finished: false,
        }
    }

    pub fn command(&self) -> CommandId {
        self.completion.command()
    }

    pub fn cursor(&self) -> Option<&str> {
        self.walk.cursor()
    }

    pub fn fresh(&mut self, identity: String) -> bool {
        self.walk.fresh(identity)
    }

    pub fn advance(&mut self, page: CursorPage) -> CursorStep {
        if self.finished {
            return CursorStep::Done;
        }
        let step = self.walk.advance(page);
        self.finished = !matches!(step, CursorStep::More(_));
        if let CursorStep::Stalled(reason) = step {
            self.completion.incomplete((self.stall_message)(reason));
        }
        step
    }

    pub fn finish<P>(self, cx: &mut Cx<'_, P>, result: Result<String, VenueError>) {
        let result = result.and_then(|rows| {
            if self.finished {
                Ok(rows)
            } else {
                Err(
                    VenueError::new("history cursor completed with a page still pending")
                        .kind(ErrorKind::Internal),
                )
            }
        });
        self.completion.finish(cx, result);
    }
}
