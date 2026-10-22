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
    /// Nothing is published on terminal rejection or failed serialization.
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
            Err(mut error) => {
                // Unclassified legacy page failures remain visible typed failures;
                // preserve specific native/transport kinds when supplied.
                if error.kind.is_none() {
                    error.kind = Some(ErrorKind::Internal);
                }
                cx.fail(self.command, error);
            }
        }
    }
}

/// Stages rows until a response is validated. A later failed page can return
/// only previously accepted rows; rejected response bodies never enter coverage.
#[must_use = "history rows must finish through their owner"]
pub struct HistoryRows<T> {
    completion: Completion,
    accepted: Vec<T>,
    pending: Vec<T>,
    encode: fn(&[T]) -> Result<String, VenueError>,
}

impl<T> HistoryRows<T> {
    pub fn new(completion: Completion, encode: fn(&[T]) -> Result<String, VenueError>) -> Self {
        Self {
            completion,
            accepted: Vec::new(),
            pending: Vec::new(),
            encode,
        }
    }

    pub fn command(&self) -> CommandId {
        self.completion.command()
    }

    /// Rows may be streamed before the final HTTP/envelope verdict is known.
    pub fn stage(&mut self, row: T) {
        self.pending.push(row);
    }

    /// Call only after the page's HTTP, envelope and row validation succeeds.
    pub fn accept_page(&mut self) {
        self.accepted.append(&mut self.pending);
    }

    pub fn finish<P>(mut self, cx: &mut Cx<'_, P>, result: Result<(), VenueError>) {
        if let Err(error) = result {
            if self.accepted.is_empty() {
                return self.completion.finish(cx, Err(error));
            }
            self.completion
                .incomplete(format!("history is INCOMPLETE: {}", error.message));
        } else if !self.pending.is_empty() {
            return self.completion.finish(
                cx,
                Err(
                    VenueError::new("history completed with an unvalidated page")
                        .kind(ErrorKind::Internal),
                ),
            );
        }
        // Encoding failure still rejects; it cannot publish malformed partial rows.
        self.completion.finish(cx, (self.encode)(&self.accepted));
    }
}

/// Cursor paging and accepted rows share one owner. Stalls record missing
/// coverage here; a venue never constructs marker fields or a replacement owner.
#[must_use = "cursor history must finish or fail through its owner"]
pub struct CursorHistory<T> {
    rows: HistoryRows<T>,
    walk: CursorWalk,
    stall_message: fn(Stall) -> String,
    finished: bool,
    lifecycle_error: Option<&'static str>,
}

impl<T> CursorHistory<T> {
    pub fn new(
        completion: Completion,
        walk: CursorWalk,
        stall_message: fn(Stall) -> String,
        encode: fn(&[T]) -> Result<String, VenueError>,
    ) -> Self {
        Self {
            rows: HistoryRows::new(completion, encode),
            walk,
            stall_message,
            finished: false,
            lifecycle_error: None,
        }
    }

    pub fn command(&self) -> CommandId {
        self.rows.command()
    }

    pub fn cursor(&self) -> Option<&str> {
        self.walk.cursor()
    }

    pub fn fresh(&mut self, identity: String) -> bool {
        self.walk.fresh(identity)
    }

    pub fn stage(&mut self, row: T) {
        self.rows.stage(row);
    }

    /// Start a new endpoint window/stage with a fresh page budget and cursor
    /// history, retaining accepted rows, identities and missing-coverage cause.
    #[must_use]
    pub fn restart_stage(&mut self) -> bool {
        if !self.finished || !self.rows.pending.is_empty() || self.lifecycle_error.is_some() {
            self.lifecycle_error = Some("history cursor stage restarted before completion");
            return false;
        }
        self.walk.restart();
        self.finished = false;
        true
    }

    pub fn advance(&mut self, page: CursorPage) -> CursorStep {
        if self.finished || self.lifecycle_error.is_some() {
            return CursorStep::Done;
        }
        self.rows.accept_page();
        let step = self.walk.advance(page);
        self.finished = !matches!(step, CursorStep::More(_));
        if let CursorStep::Stalled(reason) = step {
            self.rows
                .completion
                .incomplete((self.stall_message)(reason));
        }
        step
    }

    pub fn finish<P>(self, cx: &mut Cx<'_, P>, result: Result<(), VenueError>) {
        let lifecycle_error = self.lifecycle_error.or_else(|| {
            (result.is_ok() && !self.finished)
                .then_some("history cursor completed with a page still pending")
        });
        if let Some(message) = lifecycle_error {
            return self
                .rows
                .completion
                .finish(cx, Err(VenueError::new(message).kind(ErrorKind::Internal)));
        }
        self.rows.finish(cx, result);
    }
}
