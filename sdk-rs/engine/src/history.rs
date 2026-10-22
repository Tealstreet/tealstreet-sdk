//! History coverage: command owners bind query completion, while background
//! account diagnostics have an explicit scope and never claim a query owner.
use crate::log::code;
use crate::paging::{CursorPage, CursorStep, CursorWalk, PageRow, RangeWalk, Stall, Step};
use crate::{CommandId, Cx, ErrorKind, Level, Value, VenueError};

/// Missing coverage in automatic account reconciliation, not a user query.
/// Severity is an engine policy; venues report the native coverage fact.
#[derive(Clone, Copy, Debug)]
pub enum BackgroundCoverage {
    Incomplete,
    Exhausted,
}

impl BackgroundCoverage {
    /// No command or caller-provided fields: an automatic reconciliation cannot
    /// mark whichever unrelated history query happens to be awaiting a response.
    pub fn report<P>(self, cx: &mut Cx<'_, P>, message: impl Into<String>) {
        let level = match self {
            Self::Incomplete => Level::Warn,
            Self::Exhausted => Level::Error,
        };
        cx.event(
            level,
            code::HISTORY_INCOMPLETE,
            message,
            vec![("scope", Value::Str("account/background".into()))],
        );
    }
}

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

    pub fn accepted_len(&self) -> usize {
        self.accepted.len()
    }

    pub fn staged_len(&self) -> usize {
        self.pending.len()
    }

    /// Rows may be streamed before the final HTTP/envelope verdict is known.
    pub fn stage(&mut self, row: T) {
        self.pending.push(row);
    }

    /// Call only after the page's HTTP, envelope and row validation succeeds.
    pub fn accept_page(&mut self) {
        self.accepted.append(&mut self.pending);
    }

    /// Range selection is validated before mutating either row set. Indices
    /// come from the engine pager, in original native-page order.
    fn accept_selected_page(&mut self, selected: &[usize]) -> bool {
        if selected.iter().any(|index| *index >= self.pending.len())
            || selected.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return false;
        }
        let mut selected = selected.iter().copied().peekable();
        for (index, row) in self.pending.drain(..).enumerate() {
            if selected.peek() == Some(&index) {
                selected.next();
                self.accepted.push(row);
            }
        }
        true
    }

    pub fn finish<P>(self, cx: &mut Cx<'_, P>, result: Result<(), VenueError>) {
        let encode = self.encode;
        self.finish_with(cx, result, |rows| encode(&rows));
    }

    /// Strict endpoint-policy rejection is not a recoverable failed page, even
    /// when earlier pages were accepted. Never publish its rows or marker.
    pub fn reject<P>(self, cx: &mut Cx<'_, P>, error: VenueError) {
        self.completion.finish(cx, Err(error));
    }

    /// Final query shaping may sort/dedupe/limit accepted rows, using captured
    /// endpoint metadata. The finalizer never sees a failed page's staged rows.
    pub fn finish_with<P>(
        mut self,
        cx: &mut Cx<'_, P>,
        result: Result<(), VenueError>,
        encode: impl FnOnce(Vec<T>) -> Result<String, VenueError>,
    ) {
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
        self.completion.finish(cx, encode(self.accepted));
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

    pub fn accepted_len(&self) -> usize {
        self.rows.accepted_len()
    }

    pub fn staged_len(&self) -> usize {
        self.rows.staged_len()
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
        let encode = self.rows.encode;
        self.finish_with(cx, result, |rows| encode(&rows));
    }

    pub fn reject<P>(self, cx: &mut Cx<'_, P>, error: VenueError) {
        let error = self.lifecycle_error.map_or(error, |message| {
            VenueError::new(message).kind(ErrorKind::Internal)
        });
        self.rows.reject(cx, error);
    }

    pub fn finish_with<P>(
        self,
        cx: &mut Cx<'_, P>,
        result: Result<(), VenueError>,
        encode: impl FnOnce(Vec<T>) -> Result<String, VenueError>,
    ) {
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
        self.rows.finish_with(cx, result, encode);
    }
}

/// Range splitting, identity selection and accepted-row completion share one
/// owner. A page can affect range coverage/dedupe only after admission.
#[must_use = "range history must finish or fail through its owner"]
pub struct RangeHistory<T> {
    rows: HistoryRows<T>,
    walk: RangeWalk,
    pending_facts: Vec<PageRow>,
    active: Option<(f64, f64)>,
    finished: bool,
    lifecycle_error: Option<&'static str>,
    incomplete_message: fn(u32) -> String,
}

impl<T> RangeHistory<T> {
    pub fn new(
        completion: Completion,
        walk: RangeWalk,
        incomplete_message: fn(u32) -> String,
        encode: fn(&[T]) -> Result<String, VenueError>,
    ) -> Self {
        Self {
            rows: HistoryRows::new(completion, encode),
            walk,
            pending_facts: Vec::new(),
            active: None,
            finished: false,
            lifecycle_error: None,
            incomplete_message,
        }
    }

    pub fn command(&self) -> CommandId {
        self.rows.command()
    }

    pub fn accepted_len(&self) -> usize {
        self.rows.accepted_len()
    }

    /// Row and native range facts are staged together, never independently.
    pub fn stage(&mut self, row: T, facts: PageRow) {
        if self.active.is_none() || self.finished || self.lifecycle_error.is_some() {
            self.lifecycle_error
                .get_or_insert("history range row staged without an active fetch");
            return;
        }
        self.rows.stage(row);
        self.pending_facts.push(facts);
    }

    /// Call only after HTTP/native status and the entire page parse succeed.
    /// A malformed page must finish with Err instead; its prefix stays staged.
    pub fn accept_page(&mut self) {
        if self.lifecycle_error.is_some()
            || self.finished
            || self.active.is_none()
            || self.pending_facts.len() != self.rows.staged_len()
        {
            self.lifecycle_error
                .get_or_insert("history range admitted without an active fetch");
            return;
        }
        let (from, to) = self.active.take().expect("active checked");
        let selected = self.walk.page(from, to, &self.pending_facts);
        if !self.rows.accept_selected_page(&selected) {
            self.lifecycle_error = Some("history range selection is invalid");
        }
        self.pending_facts.clear();
    }

    /// The adapter supplies transport and any ChunkDone pacing; only the owner
    /// advances bounds or decides when the range has finished.
    pub fn step(&mut self) -> Step {
        if self.lifecycle_error.is_some()
            || self.finished
            || self.active.is_some()
            || !self.pending_facts.is_empty()
            || self.rows.staged_len() != 0
        {
            self.lifecycle_error
                .get_or_insert("history range stepped before page admission or after completion");
            return Step::Finished;
        }
        let step = self.walk.step();
        match step {
            Step::Fetch { from, to } => self.active = Some((from, to)),
            Step::Finished => self.finished = true,
            Step::ChunkDone => {}
        }
        step
    }

    /// Signing/policy/lifecycle errors are strict, even after accepted rows.
    pub fn reject<P>(self, cx: &mut Cx<'_, P>, error: VenueError) {
        let error = self.lifecycle_error.map_or(error, |message| {
            VenueError::new(message).kind(ErrorKind::Internal)
        });
        self.rows.reject(cx, error);
    }

    pub fn finish<P>(self, cx: &mut Cx<'_, P>, result: Result<(), VenueError>) {
        let encode = self.rows.encode;
        self.finish_with(cx, result, |rows| encode(&rows));
    }

    pub fn finish_with<P>(
        mut self,
        cx: &mut Cx<'_, P>,
        result: Result<(), VenueError>,
        encode: impl FnOnce(Vec<T>) -> Result<String, VenueError>,
    ) {
        let lifecycle_error = self.lifecycle_error.or_else(|| {
            if result.is_ok() && !self.finished {
                Some("history range completed before its final step")
            } else if result.is_err() && self.active.is_none() {
                Some("history range page failed without an active fetch")
            } else {
                None
            }
        });
        if let Some(message) = lifecycle_error {
            return self
                .rows
                .reject(cx, VenueError::new(message).kind(ErrorKind::Internal));
        }
        let gaps = self.walk.incomplete_chunks();
        if gaps > 0 {
            self.rows
                .completion
                .incomplete((self.incomplete_message)(gaps));
        }
        self.rows.finish_with(cx, result, encode);
    }
}
