//! History coverage: command owners bind query completion, while background
//! account diagnostics have an explicit scope and never claim a query owner.
use crate::log::code;
use crate::paging::{CursorPage, CursorStep, CursorWalk, PageRow, RangeWalk, Stall, Step};
use crate::request_context::{Attempt, Context, Error as ContextError, Owner};
use crate::{CommandId, Cx, ErrorKind, Level, Value, VenueError};

#[cfg(feature = "http-plans")]
#[path = "history_plan.rs"]
mod plan;
#[cfg(feature = "http-plans")]
pub use plan::{PlanFailureAdmission, PlanHistoryRows, PlanReadCompletion, PlanRowsAdmission};

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

mod authority_sealed {
    pub trait Sealed {}
}

/// Explicit unmanaged ownership for history sites which have not migrated.
/// The default preserves their constructor and terminal signatures.
/// It cannot be constructed to call the sealed terminal trait directly.
///
/// ```compile_fail,E0451
/// use safe_cex_engine::history::LegacyAuthority;
/// let _ = LegacyAuthority { _private: () };
/// ```
pub struct LegacyAuthority {
    _private: (),
}
impl authority_sealed::Sealed for LegacyAuthority {}

/// Engine-owned history authority. Venue crates cannot invent an implementation.
#[doc(hidden)]
pub trait HistoryAuthority: authority_sealed::Sealed {
    type Terminal;
    fn validate<P>(&self, cx: &Cx<'_, P>) -> Result<(), ContextError>;
    fn denied(error: ContextError) -> Self::Terminal;
    fn discard_page(&mut self);
    fn finish<P>(
        self,
        cx: &mut Cx<'_, P>,
        command: CommandId,
        incomplete: Option<String>,
        result: Result<String, VenueError>,
    ) -> Self::Terminal;
}

impl HistoryAuthority for LegacyAuthority {
    type Terminal = ();
    fn validate<P>(&self, _: &Cx<'_, P>) -> Result<(), ContextError> {
        Ok(())
    }
    fn denied(_: ContextError) {}
    fn discard_page(&mut self) {}
    fn finish<P>(
        self,
        cx: &mut Cx<'_, P>,
        command: CommandId,
        incomplete: Option<String>,
        result: Result<String, VenueError>,
    ) {
        match result {
            Ok(result) => {
                if let Some(message) = incomplete {
                    cx.event(
                        Level::Warn,
                        code::HISTORY_INCOMPLETE,
                        message,
                        vec![("command", Value::Num(f64::from(command)))],
                    );
                }
                cx.done(command, result);
            }
            Err(mut error) => {
                if error.kind.is_none() {
                    error.kind = Some(ErrorKind::Internal);
                }
                cx.fail(command, error);
            }
        }
    }
}

/// Opaque original Read-root ownership. Only the engine may construct it.
pub struct ReadAuthority {
    root: Box<Context>,
    page: Option<(Context, Attempt)>,
    identities: PageIdentities,
}

#[derive(Default)]
struct PageIdentities(std::collections::HashSet<String>);

impl PageIdentities {
    fn fresh(&mut self, walk: &CursorWalk, identity: String) -> bool {
        !walk.contains_identity(&identity) && self.0.insert(identity)
    }

    fn commit(&mut self, walk: &mut CursorWalk) {
        for identity in self.0.drain() {
            walk.fresh(identity);
        }
    }

    fn discard(&mut self) {
        self.0.clear();
    }
}
impl authority_sealed::Sealed for ReadAuthority {}
impl HistoryAuthority for ReadAuthority {
    type Terminal = Result<(), ContextError>;
    fn validate<P>(&self, cx: &Cx<'_, P>) -> Result<(), ContextError> {
        cx.validate_read_completion_context(&self.root)
    }
    fn denied(error: ContextError) -> Self::Terminal {
        Err(error)
    }
    fn discard_page(&mut self) {
        self.page = None;
        self.identities.discard();
    }
    fn finish<P>(
        self,
        cx: &mut Cx<'_, P>,
        command: CommandId,
        incomplete: Option<String>,
        result: Result<String, VenueError>,
    ) -> Self::Terminal {
        // Both success and failure need the read companion check. Generic
        // Context terminals do not reject a newly declared mutation sibling.
        self.validate(cx)?;
        match result {
            Ok(result) => {
                if let Some(message) = incomplete {
                    cx.event(
                        Level::Warn,
                        code::HISTORY_INCOMPLETE,
                        message,
                        vec![("command", Value::Num(f64::from(command)))],
                    );
                }
                cx.done_context(&self.root, result)
            }
            Err(mut error) => {
                if error.kind.is_none() {
                    error.kind = Some(ErrorKind::Internal);
                }
                cx.fail_context(&self.root, error)
            }
        }
    }
}

/// Read completion has no numeric constructor or fallback.
///
/// ```compile_fail,E0599
/// use safe_cex_engine::history::ReadCompletion;
/// let _ = ReadCompletion::new(7);
/// ```
pub type ReadCompletion = Completion<ReadAuthority>;
/// Managed rows require an admitted delivery before any row mutation.
///
/// ```compile_fail,E0599
/// use safe_cex_engine::history::ReadHistoryRows;
/// fn unchecked(mut rows: ReadHistoryRows<String>) {
///     rows.stage("unvalidated".into());
/// }
/// ```
pub type ReadHistoryRows<T> = HistoryRows<T, ReadAuthority>;
/// Cursor admission and identity mutation also require the delivery guard.
///
/// ```compile_fail,E0599
/// use safe_cex_engine::history::ReadCursorHistory;
/// fn unchecked(mut history: ReadCursorHistory<String>) {
///     history.fresh("unvalidated".into());
/// }
/// ```
pub type ReadCursorHistory<T> = CursorHistory<T, ReadAuthority>;
/// Range geometry selection does not authorize unchecked response admission.
///
/// ```compile_fail,E0599
/// use safe_cex_engine::history::ReadRangeHistory;
/// fn unchecked(mut history: ReadRangeHistory<String>) {
///     history.accept_page();
/// }
/// ```
pub type ReadRangeHistory<T> = RangeHistory<T, ReadAuthority>;

/// One query owner, retained across all pages and composite stages. Consuming
/// completion prevents finishing this owner twice; legacy `Cx` APIs still exist
/// for operations which have not migrated.
#[must_use = "history must finish or fail through its owner"]
pub struct Completion<A = LegacyAuthority> {
    command: CommandId,
    incomplete: Option<String>,
    authority: A,
}

impl Completion<LegacyAuthority> {
    pub fn new(command: CommandId) -> Self {
        Self {
            command,
            incomplete: None,
            authority: LegacyAuthority { _private: () },
        }
    }
}

impl<A: HistoryAuthority> Completion<A> {
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
    pub fn finish<P>(self, cx: &mut Cx<'_, P>, result: Result<String, VenueError>) -> A::Terminal {
        self.authority
            .finish(cx, self.command, self.incomplete, result)
    }
}

/// Stages rows until a response is validated. A later failed page can return
/// only previously accepted rows; rejected response bodies never enter coverage.
#[must_use = "history rows must finish through their owner"]
pub struct HistoryRows<T, A = LegacyAuthority> {
    completion: Completion<A>,
    accepted: Vec<T>,
    pending: Vec<T>,
    encode: fn(&[T]) -> Result<String, VenueError>,
}

impl<T, A: HistoryAuthority> HistoryRows<T, A> {
    pub fn new(completion: Completion<A>, encode: fn(&[T]) -> Result<String, VenueError>) -> Self {
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
    fn stage_inner(&mut self, row: T) {
        self.pending.push(row);
    }

    /// Retry only the current unvalidated page. Previously admitted output and
    /// its completion/coverage authority are never rolled back.
    pub fn discard_staged_page(&mut self) {
        self.pending.clear();
        self.completion.authority.discard_page();
    }

    /// Call only after the page's HTTP, envelope and row validation succeeds.
    fn accept_page_inner(&mut self) {
        self.accepted.append(&mut self.pending);
        self.completion.authority.discard_page();
    }

    /// Range selection is validated before mutating either row set. Indices
    /// come from the engine pager, in original native-page order.
    fn accept_selected_page_where(
        &mut self,
        selected: &[usize],
        mut retain: impl FnMut(&T) -> bool,
    ) -> bool {
        if selected.iter().any(|index| *index >= self.pending.len())
            || selected.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return false;
        }
        let mut selected = selected.iter().copied().peekable();
        for (index, row) in self.pending.drain(..).enumerate() {
            if selected.peek() == Some(&index) {
                selected.next();
                if retain(&row) {
                    self.accepted.push(row);
                }
            }
        }
        self.completion.authority.discard_page();
        true
    }

    pub fn finish<P>(self, cx: &mut Cx<'_, P>, result: Result<(), VenueError>) -> A::Terminal {
        let encode = self.encode;
        self.finish_with(cx, result, |rows| encode(&rows))
    }

    /// Strict endpoint-policy rejection is not a recoverable failed page, even
    /// when earlier pages were accepted. Never publish its rows or marker.
    pub fn reject<P>(self, cx: &mut Cx<'_, P>, error: VenueError) -> A::Terminal {
        self.completion.finish(cx, Err(error))
    }

    /// Final query shaping may sort/dedupe/limit accepted rows, using captured
    /// endpoint metadata. The finalizer never sees a failed page's staged rows.
    pub fn finish_with<P>(
        mut self,
        cx: &mut Cx<'_, P>,
        result: Result<(), VenueError>,
        encode: impl FnOnce(Vec<T>) -> Result<String, VenueError>,
    ) -> A::Terminal {
        // A managed owner must be live before a captured finalizer can touch
        // native state. The terminal revalidates before any companion notice.
        if let Err(error) = self.completion.authority.validate(cx) {
            return A::denied(error);
        }
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
        self.completion.finish(cx, encode(self.accepted))
    }
}

/// Cursor paging and accepted rows share one owner. Stalls record missing
/// coverage here; a venue never constructs marker fields or a replacement owner.
#[must_use = "cursor history must finish or fail through its owner"]
pub struct CursorHistory<T, A = LegacyAuthority> {
    rows: HistoryRows<T, A>,
    walk: CursorWalk,
    stall_message: fn(Stall) -> String,
    finished: bool,
    stage_has_rows: bool,
    lifecycle_error: Option<&'static str>,
}

impl<T, A: HistoryAuthority> CursorHistory<T, A> {
    pub fn new(
        completion: Completion<A>,
        walk: CursorWalk,
        stall_message: fn(Stall) -> String,
        encode: fn(&[T]) -> Result<String, VenueError>,
    ) -> Self {
        Self {
            rows: HistoryRows::new(completion, encode),
            walk,
            stall_message,
            finished: false,
            stage_has_rows: false,
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

    pub fn pages(&self) -> u32 {
        self.walk.pages()
    }

    fn fresh_inner(&mut self, identity: String) -> bool {
        self.walk.fresh(identity)
    }

    fn stage_inner(&mut self, row: T) {
        self.rows.stage_inner(row);
    }

    /// A transport or session retry may replace an unvalidated response body,
    /// never an already completed stage or a poisoned cursor lifecycle.
    /// Legacy direct `fresh` observations are not rolled back. Read admission
    /// keeps identities local until advance, so it drops those with staging.
    #[must_use]
    pub fn discard_staged_page(&mut self) -> bool {
        if self.finished || self.lifecycle_error.is_some() {
            self.lifecycle_error
                .get_or_insert("history cursor page discarded after completion");
            return false;
        }
        self.rows.discard_staged_page();
        true
    }

    /// Start a new endpoint window/stage with a fresh page budget and cursor
    /// history, retaining accepted rows, identities and missing-coverage cause.
    #[must_use]
    pub fn restart_stage(&mut self) -> bool {
        if !self.finished || !self.rows.pending.is_empty() || self.lifecycle_error.is_some() {
            self.lifecycle_error = Some("history cursor stage restarted before completion");
            return false;
        }
        self.rows.completion.authority.discard_page();
        self.walk.restart();
        self.finished = false;
        self.stage_has_rows = false;
        true
    }

    fn advance_page(&mut self, page: CursorPage) -> CursorStep {
        self.advance_inner(page, true)
    }

    /// A bounded discovery/proof stage can end without proving complete query
    /// coverage. Preserve cursor-cycle checks and lifecycle admission, but do
    /// not label its page budget as missing result rows. Its caller must retain
    /// the incomplete proof (for example, a nondefinitive lookup miss).
    fn advance_probe_inner(&mut self, page: CursorPage) -> CursorStep {
        if self.stage_has_rows || self.rows.staged_len() > 0 {
            self.lifecycle_error = Some("history probe cannot admit result rows");
            return CursorStep::Done;
        }
        self.advance_inner(page, false)
    }

    fn advance_inner(&mut self, page: CursorPage, budget_is_incomplete: bool) -> CursorStep {
        if self.finished || self.lifecycle_error.is_some() {
            return CursorStep::Done;
        }
        self.stage_has_rows |= self.rows.staged_len() > 0;
        self.rows.accept_page_inner();
        let step = self.walk.advance(page);
        self.finished = !matches!(step, CursorStep::More(_));
        if let CursorStep::Stalled(reason) = step
            && (budget_is_incomplete || reason != Stall::PageBudget)
        {
            self.rows
                .completion
                .incomplete((self.stall_message)(reason));
        }
        step
    }

    /// A fully validated native page can still advertise coverage the adapter
    /// cannot consume. Record that fact atomically with admission, never from a
    /// failed response or after the owner has completed.
    fn advance_coverage_inner(&mut self, page: CursorPage, gap: Option<String>) -> CursorStep {
        if self.finished || self.lifecycle_error.is_some() {
            self.lifecycle_error
                .get_or_insert("history cursor coverage admitted after completion");
            return CursorStep::Done;
        }
        if let Some(message) = gap {
            self.rows.completion.incomplete(message);
        }
        self.advance_page(page)
    }

    pub fn finish<P>(self, cx: &mut Cx<'_, P>, result: Result<(), VenueError>) -> A::Terminal {
        let encode = self.rows.encode;
        self.finish_with(cx, result, |rows| encode(&rows))
    }

    pub fn reject<P>(self, cx: &mut Cx<'_, P>, error: VenueError) -> A::Terminal {
        let error = self.lifecycle_error.map_or(error, |message| {
            VenueError::new(message).kind(ErrorKind::Internal)
        });
        self.rows.reject(cx, error)
    }

    pub fn finish_with<P>(
        self,
        cx: &mut Cx<'_, P>,
        result: Result<(), VenueError>,
        encode: impl FnOnce(Vec<T>) -> Result<String, VenueError>,
    ) -> A::Terminal {
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
        self.rows.finish_with(cx, result, encode)
    }
}

/// Range splitting, identity selection and accepted-row completion share one
/// owner. A page can affect range coverage/dedupe only after admission.
#[must_use = "range history must finish or fail through its owner"]
pub struct RangeHistory<T, A = LegacyAuthority> {
    rows: HistoryRows<T, A>,
    walk: RangeWalk,
    pending_facts: Vec<PageRow>,
    active: Option<(f64, f64)>,
    finished: bool,
    lifecycle_error: Option<&'static str>,
    incomplete_message: fn(u32) -> String,
    request_budget: Option<(u32, String)>,
    retired_requests: u64,
    started: bool,
}

impl<T, A: HistoryAuthority> RangeHistory<T, A> {
    pub fn new(
        completion: Completion<A>,
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
            request_budget: None,
            retired_requests: 0,
            started: false,
        }
    }

    /// Bound selected native requests across every chunk, using the walk's
    /// existing count. A deferred fetch is selected once and resumed as-is.
    pub fn with_request_budget(mut self, budget: u32, message: impl Into<String>) -> Self {
        if budget == 0
            || self.started
            || self.walk.requests != 0
            || self.active.is_some()
            || self.finished
        {
            self.lifecycle_error = Some("history range request budget must be set before fetching");
        } else {
            self.request_budget = Some((budget, message.into()));
        }
        self
    }

    pub fn command(&self) -> CommandId {
        self.rows.command()
    }

    pub fn accepted_len(&self) -> usize {
        self.rows.accepted_len()
    }

    /// Replace only a completed range stage, retaining the logical owner,
    /// accepted output, first coverage gap and total request budget.
    #[must_use]
    pub fn restart_stage(&mut self, walk: RangeWalk) -> bool {
        if !self.finished
            || self.active.is_some()
            || !self.pending_facts.is_empty()
            || self.rows.staged_len() != 0
            || self.lifecycle_error.is_some()
            || walk.requests != 0
        {
            self.lifecycle_error
                .get_or_insert("history range stage restarted before completion");
            return false;
        }
        self.record_range_gap();
        self.retired_requests = self
            .retired_requests
            .saturating_add(u64::from(self.walk.requests));
        self.rows.completion.authority.discard_page();
        self.walk = walk;
        self.finished = false;
        self.started = true;
        true
    }

    fn record_range_gap(&mut self) {
        let gaps = self.walk.incomplete_chunks();
        if gaps > 0 {
            self.rows
                .completion
                .incomplete((self.incomplete_message)(gaps));
        }
    }

    /// Bind a selected fetch to narrower native request bounds before issuing
    /// it. Inclusive endpoints may pad a zero-width walk only internally;
    /// coverage must use the range actually requested from the venue.
    /// Bounds come from the command or selected geometry, never response data.
    #[must_use]
    pub fn restrict_active_range(&mut self, from: f64, to: f64) -> bool {
        let valid = self.active.is_some_and(|(selected_from, selected_to)| {
            from.is_finite()
                && to.is_finite()
                && selected_from <= from
                && from <= to
                && to <= selected_to
        }) && !self.finished
            && self.lifecycle_error.is_none()
            && self.pending_facts.is_empty()
            && self.rows.staged_len() == 0;
        if !valid {
            self.lifecycle_error
                .get_or_insert("history range request bounds are invalid or already staged");
            return false;
        }
        self.active = Some((from, to));
        true
    }

    /// Row and native range facts are staged together, never independently.
    fn stage_inner(&mut self, row: T, facts: PageRow) {
        if self.active.is_none() || self.finished || self.lifecycle_error.is_some() {
            self.lifecycle_error
                .get_or_insert("history range row staged without an active fetch");
            return;
        }
        self.rows.stage_inner(row);
        self.pending_facts.push(facts);
    }

    fn accept_page_inner(&mut self, gap: Option<String>, retain: impl FnMut(&T) -> bool) {
        if self.lifecycle_error.is_some()
            || self.finished
            || self.active.is_none()
            || self.pending_facts.len() != self.rows.staged_len()
        {
            self.lifecycle_error
                .get_or_insert("history range admitted without an active fetch");
            return;
        }
        if let Some(message) = gap {
            self.rows.completion.incomplete(message);
            self.walk.failed();
        }
        let (from, to) = self.active.take().expect("active checked");
        let selected = self.walk.page(from, to, &self.pending_facts);
        if !self.rows.accept_selected_page_where(&selected, retain) {
            self.lifecycle_error = Some("history range selection is invalid");
        }
        self.pending_facts.clear();
    }

    /// The adapter supplies transport and any ChunkDone pacing; only the owner
    /// advances bounds or decides when the range has finished.
    pub fn step(&mut self) -> Step {
        self.started = true;
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
        if matches!(step, Step::Fetch { .. })
            && let Some((budget, message)) = &self.request_budget
            && self
                .retired_requests
                .saturating_add(u64::from(self.walk.requests))
                > u64::from(*budget)
        {
            self.rows.completion.incomplete(message.clone());
            self.finished = true;
            return Step::Finished;
        }
        match step {
            Step::Fetch { from, to } => self.active = Some((from, to)),
            Step::Finished => self.finished = true,
            Step::ChunkDone => {}
        }
        step
    }

    /// Signing/policy/lifecycle errors are strict, even after accepted rows.
    pub fn reject<P>(self, cx: &mut Cx<'_, P>, error: VenueError) -> A::Terminal {
        let error = self.lifecycle_error.map_or(error, |message| {
            VenueError::new(message).kind(ErrorKind::Internal)
        });
        self.rows.reject(cx, error)
    }

    pub fn finish<P>(self, cx: &mut Cx<'_, P>, result: Result<(), VenueError>) -> A::Terminal {
        let encode = self.rows.encode;
        self.finish_with(cx, result, |rows| encode(&rows))
    }

    pub fn finish_with<P>(
        mut self,
        cx: &mut Cx<'_, P>,
        result: Result<(), VenueError>,
        encode: impl FnOnce(Vec<T>) -> Result<String, VenueError>,
    ) -> A::Terminal {
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
        self.record_range_gap();
        self.rows.finish_with(cx, result, encode)
    }
}

// Legacy call sites retain their exact mutation signatures. Read callers reach
// these shared private algorithms only through an admitted delivery guard.
impl<T> HistoryRows<T, LegacyAuthority> {
    pub fn stage(&mut self, row: T) {
        self.stage_inner(row);
    }
    pub fn accept_page(&mut self) {
        self.accept_page_inner();
    }
}
impl<T> CursorHistory<T, LegacyAuthority> {
    pub fn fresh(&mut self, identity: String) -> bool {
        self.fresh_inner(identity)
    }
    pub fn stage(&mut self, row: T) {
        self.stage_inner(row);
    }
    pub fn advance(&mut self, page: CursorPage) -> CursorStep {
        self.advance_page(page)
    }
    pub fn advance_probe(&mut self, page: CursorPage) -> CursorStep {
        self.advance_probe_inner(page)
    }
    pub fn advance_with_coverage(&mut self, page: CursorPage, gap: Option<String>) -> CursorStep {
        self.advance_coverage_inner(page, gap)
    }
}
impl<T> RangeHistory<T, LegacyAuthority> {
    pub fn stage(&mut self, row: T, facts: PageRow) {
        self.stage_inner(row, facts);
    }
    pub fn accept_page(&mut self) {
        self.accept_page_inner(None, |_| true);
    }
    pub fn accept_page_where(&mut self, retain: impl FnMut(&T) -> bool) {
        self.accept_page_inner(None, retain);
    }
    pub fn accept_page_with_coverage(&mut self, gap: Option<String>) {
        self.accept_page_inner(gap, |_| true);
    }
}

impl ReadCompletion {
    /// Bind the matching command's exact original engine-issued Read root.
    /// This read-only check allocates no work or send rights. Duplicate bindings
    /// are allowed; only the first valid context terminal can complete the owner.
    /// Plan/observer handles and physical child legs are not original roots.
    pub fn from_context<P>(
        cx: &Cx<'_, P>,
        command: CommandId,
        root: Context,
    ) -> Result<Self, ContextError> {
        cx.validate_read_root_context(&root)?;
        if !matches!(root.owner(), Owner::Command { id, .. } if id == command) {
            return Err(ContextError::Invalid);
        }
        Ok(Self {
            command,
            incomplete: None,
            authority: ReadAuthority {
                root: Box::new(root),
                page: None,
                identities: PageIdentities::default(),
            },
        })
    }

    pub fn root(&self) -> &Context {
        &self.authority.root
    }
    pub fn validate_delivery<P>(&self, cx: &Cx<'_, P>, leg: &Context) -> Result<(), ContextError> {
        self.delivery_attempt(cx, leg).map(|_| ())
    }
    fn delivery_attempt<P>(&self, cx: &Cx<'_, P>, leg: &Context) -> Result<Attempt, ContextError> {
        let attempt = cx.read_http_delivery_attempt(self.root(), leg)?;
        if self
            .authority
            .page
            .as_ref()
            .is_some_and(|(page, bound)| page != leg || *bound != attempt)
        {
            return Err(ContextError::State);
        }
        Ok(attempt)
    }
    fn bind_page(&mut self, leg: &Context, attempt: Attempt) {
        if self.authority.page.is_none() {
            self.authority.page = Some((leg.clone(), attempt));
        }
    }
}

/// Synchronous data admission. Drop the immutable Cx borrow before diagnostics,
/// Store publication or terminal completion; re-admit if more staging follows.
///
/// ```no_run
/// use safe_cex_engine::{Cx, Level};
/// use safe_cex_engine::history::ReadHistoryRows;
/// use safe_cex_engine::request_context::{Context, Error};
/// fn accepted<P>(rows: &mut ReadHistoryRows<String>, cx: &mut Cx<'_, P>, leg: &Context)
///     -> Result<(), Error>
/// {
///     {
///         let mut page = rows.admit(cx, leg)?;
///         page.stage("validated row".into());
///     }
///     cx.event(Level::Warn, "row.skipped", "native diagnostic", vec![]);
///     rows.admit(cx, leg)?.accept_page();
///     Ok(())
/// }
/// ```
pub struct RowsAdmission<'a, 'io, T, P> {
    owner: &'a mut ReadHistoryRows<T>,
    _cx: &'a Cx<'io, P>,
    leg: &'a Context,
    attempt: Attempt,
}
pub struct CursorAdmission<'a, 'io, T, P> {
    owner: &'a mut ReadCursorHistory<T>,
    _cx: &'a Cx<'io, P>,
    leg: &'a Context,
    attempt: Attempt,
}
pub struct RangeAdmission<'a, 'io, T, P> {
    owner: &'a mut ReadRangeHistory<T>,
    _cx: &'a Cx<'io, P>,
    leg: &'a Context,
    attempt: Attempt,
}

impl<T> ReadHistoryRows<T> {
    pub fn root(&self) -> &Context {
        self.completion.root()
    }
    pub fn validate_delivery<P>(&self, cx: &Cx<'_, P>, leg: &Context) -> Result<(), ContextError> {
        self.completion.validate_delivery(cx, leg)
    }
    pub fn admit<'a, 'io, P>(
        &'a mut self,
        cx: &'a Cx<'io, P>,
        leg: &'a Context,
    ) -> Result<RowsAdmission<'a, 'io, T, P>, ContextError> {
        let attempt = self.completion.delivery_attempt(cx, leg)?;
        Ok(RowsAdmission {
            owner: self,
            _cx: cx,
            leg,
            attempt,
        })
    }
}
impl<T> ReadCursorHistory<T> {
    pub fn root(&self) -> &Context {
        self.rows.root()
    }
    pub fn validate_delivery<P>(&self, cx: &Cx<'_, P>, leg: &Context) -> Result<(), ContextError> {
        self.rows.validate_delivery(cx, leg)
    }
    pub fn admit<'a, 'io, P>(
        &'a mut self,
        cx: &'a Cx<'io, P>,
        leg: &'a Context,
    ) -> Result<CursorAdmission<'a, 'io, T, P>, ContextError> {
        let attempt = self.rows.completion.delivery_attempt(cx, leg)?;
        Ok(CursorAdmission {
            owner: self,
            _cx: cx,
            leg,
            attempt,
        })
    }
}
impl<T> ReadRangeHistory<T> {
    pub fn root(&self) -> &Context {
        self.rows.root()
    }
    pub fn validate_delivery<P>(&self, cx: &Cx<'_, P>, leg: &Context) -> Result<(), ContextError> {
        self.rows.validate_delivery(cx, leg)
    }
    pub fn admit<'a, 'io, P>(
        &'a mut self,
        cx: &'a Cx<'io, P>,
        leg: &'a Context,
    ) -> Result<RangeAdmission<'a, 'io, T, P>, ContextError> {
        let attempt = self.rows.completion.delivery_attempt(cx, leg)?;
        Ok(RangeAdmission {
            owner: self,
            _cx: cx,
            leg,
            attempt,
        })
    }
}
impl<T, P> RowsAdmission<'_, '_, T, P> {
    pub fn stage(&mut self, row: T) {
        self.owner.completion.bind_page(self.leg, self.attempt);
        self.owner.stage_inner(row);
    }
    pub fn accept_page(&mut self) {
        self.owner.completion.bind_page(self.leg, self.attempt);
        self.owner.accept_page_inner();
    }
}
impl<T, P> CursorAdmission<'_, '_, T, P> {
    fn commit_page_identities(&mut self) {
        if self.owner.finished || self.owner.lifecycle_error.is_some() {
            return;
        }
        self.owner
            .rows
            .completion
            .authority
            .identities
            .commit(&mut self.owner.walk);
    }

    pub fn fresh(&mut self, identity: String) -> bool {
        self.owner.rows.completion.bind_page(self.leg, self.attempt);
        self.owner
            .rows
            .completion
            .authority
            .identities
            .fresh(&self.owner.walk, identity)
    }
    pub fn stage(&mut self, row: T) {
        self.owner.rows.completion.bind_page(self.leg, self.attempt);
        self.owner.stage_inner(row);
    }
    pub fn advance(&mut self, page: CursorPage) -> CursorStep {
        self.owner.rows.completion.bind_page(self.leg, self.attempt);
        self.commit_page_identities();
        self.owner.advance_page(page)
    }
    pub fn advance_probe(&mut self, page: CursorPage) -> CursorStep {
        self.owner.rows.completion.bind_page(self.leg, self.attempt);
        if !self.owner.stage_has_rows && self.owner.rows.staged_len() == 0 {
            self.commit_page_identities();
        }
        self.owner.advance_probe_inner(page)
    }
    pub fn advance_with_coverage(&mut self, page: CursorPage, gap: Option<String>) -> CursorStep {
        self.owner.rows.completion.bind_page(self.leg, self.attempt);
        self.commit_page_identities();
        self.owner.advance_coverage_inner(page, gap)
    }
}
impl<T, P> RangeAdmission<'_, '_, T, P> {
    pub fn stage(&mut self, row: T, facts: PageRow) {
        self.owner.rows.completion.bind_page(self.leg, self.attempt);
        self.owner.stage_inner(row, facts);
    }
    pub fn accept_page(&mut self) {
        self.owner.rows.completion.bind_page(self.leg, self.attempt);
        self.owner.accept_page_inner(None, |_| true);
    }
    pub fn accept_page_where(&mut self, retain: impl FnMut(&T) -> bool) {
        self.owner.rows.completion.bind_page(self.leg, self.attempt);
        self.owner.accept_page_inner(None, retain);
    }
    pub fn accept_page_with_coverage(&mut self, gap: Option<String>) {
        self.owner.rows.completion.bind_page(self.leg, self.attempt);
        self.owner.accept_page_inner(gap, |_| true);
    }
}

#[cfg(test)]
mod cursor_probe_tests {
    use super::*;

    fn owner() -> CursorHistory<String> {
        CursorHistory::new(
            Completion::new(1),
            CursorWalk::new(2),
            |reason| format!("{reason:?}"),
            |_| Ok("[]".into()),
        )
    }
    fn page(next: &str) -> CursorPage {
        CursorPage {
            next: Some(next.into()),
            rows: 0,
            fresh: 0,
            want_more: true,
        }
    }
    #[test]
    fn probe_budget_keeps_stage_lifecycle_and_prior_query_omissions() {
        let mut history = owner();
        assert_eq!(
            history.advance_probe(page("a")),
            CursorStep::More("a".into())
        );
        assert_eq!(history.pages(), 1);
        assert_eq!(
            history.advance_probe(page("b")),
            CursorStep::Stalled(Stall::PageBudget)
        );
        assert!(history.rows.completion.incomplete.is_none());
        assert!(history.restart_stage());
        history.advance(page("a"));
        history.advance(page("b"));
        assert_eq!(
            history.rows.completion.incomplete.as_deref(),
            Some("PageBudget")
        );
        assert!(history.restart_stage());
        history.advance_probe(page("a"));
        history.advance_probe(page("b"));
        assert_eq!(
            history.rows.completion.incomplete.as_deref(),
            Some("PageBudget")
        );
    }
    #[test]
    fn probe_cycle_still_precedes_budget_and_cannot_restart_early() {
        let mut history = owner();
        history.advance_probe(page("a"));
        assert_eq!(
            history.advance_probe(page("a")),
            CursorStep::Stalled(Stall::RepeatedCursor)
        );
        assert_eq!(
            history.rows.completion.incomplete.as_deref(),
            Some("RepeatedCursor")
        );
        let mut early = owner();
        early.stage("pending".into());
        assert!(!early.restart_stage());
        assert!(early.lifecycle_error.is_some());
    }
    #[test]
    fn probe_cannot_hide_staged_or_previously_admitted_rows_in_its_stage() {
        for admitted in [false, true] {
            let mut history = owner();
            history.stage("unproven".into());
            if admitted {
                history.advance(page("a"));
            }
            assert_eq!(history.advance_probe(page("b")), CursorStep::Done);
            assert_eq!(
                history.lifecycle_error,
                Some("history probe cannot admit result rows")
            );
            assert!(!history.restart_stage());
        }
        let mut history = owner();
        history.stage("complete prior stage".into());
        history.advance(CursorPage {
            next: None,
            rows: 1,
            fresh: 1,
            want_more: false,
        });
        assert!(history.restart_stage());
        history.advance_probe(page("a"));
        history.advance_probe(page("b"));
        assert_eq!(history.accepted_len(), 1);
        assert!(history.lifecycle_error.is_none());
    }
}

#[cfg(test)]
mod range_stage_tests {
    use super::*;

    fn owner() -> RangeHistory<String> {
        RangeHistory::new(
            Completion::new(71),
            RangeWalk::new(0., 10., 10., 200),
            |_| "range gap".into(),
            |_| Ok("[]".into()),
        )
    }
    fn next_fetch(history: &mut RangeHistory<String>) -> (f64, f64) {
        match history.step() {
            Step::Fetch { from, to } => (from, to),
            other => panic!("expected fetch: {other:?}"),
        }
    }
    fn finish_empty(history: &mut RangeHistory<String>) {
        for _ in 0..100 {
            match history.step() {
                Step::Fetch { .. } => history.accept_page(),
                Step::ChunkDone => {}
                Step::Finished => return,
            }
        }
        panic!("bounded range must finish");
    }
    #[test]
    fn composite_stage_retains_owner_rows_and_first_gap() {
        let mut history = owner();
        next_fetch(&mut history);
        history.stage(
            "first currency".into(),
            PageRow {
                timestamp: 1.,
                identity: "usdt".into(),
            },
        );
        history.accept_page_with_coverage(Some("first gap".into()));
        finish_empty(&mut history);
        assert!(history.restart_stage(RangeWalk::new(0., 10., 10., 200)));
        assert_eq!(history.command(), 71);
        assert_eq!(
            history.rows.completion.incomplete.as_deref(),
            Some("first gap")
        );
        next_fetch(&mut history);
        history.stage(
            "second currency".into(),
            PageRow {
                timestamp: 1.,
                identity: "usdc".into(),
            },
        );
        history.accept_page_with_coverage(Some("later gap".into()));
        finish_empty(&mut history);
        assert_eq!(history.rows.accepted, ["first currency", "second currency"]);
        assert_eq!(
            history.rows.completion.incomplete.as_deref(),
            Some("first gap")
        );
    }
    #[test]
    fn early_restart_poisoning_does_not_drop_staged_rows_or_change_the_walk() {
        let mut history = owner();
        next_fetch(&mut history);
        history.stage(
            "pending".into(),
            PageRow {
                timestamp: 1.,
                identity: "pending".into(),
            },
        );
        history.rows.completion.incomplete("existing gap");
        assert!(!history.restart_stage(RangeWalk::new(100., 200., 100., 1)));
        assert_eq!(history.active, Some((0., 10.)));
        assert_eq!(history.rows.pending, ["pending"]);
        assert_eq!(history.pending_facts.len(), 1);
        assert_eq!(
            history.rows.completion.incomplete.as_deref(),
            Some("existing gap")
        );
        assert_eq!(history.retired_requests, 0);
        assert_eq!(history.walk.requests, 1);
        assert!(history.lifecycle_error.is_some());
    }
    #[test]
    fn filtered_output_does_not_count_as_accepted_but_still_advances_coverage() {
        let mut history = owner();
        next_fetch(&mut history);
        history.stage(
            "excluded".into(),
            PageRow {
                timestamp: 5.,
                identity: "seen".into(),
            },
        );
        history.accept_page_where(|_| false);
        assert_eq!(history.accepted_len(), 0);
        assert_eq!(next_fetch(&mut history), (5., 10.));
        history.stage(
            "duplicate".into(),
            PageRow {
                timestamp: 5.,
                identity: "seen".into(),
            },
        );
        history.accept_page_where(|_| true);
        assert_eq!(
            history.accepted_len(),
            0,
            "filtered identity already covered"
        );
        assert!(history.rows.pending.is_empty());
    }
    #[test]
    fn request_budget_spans_stages_and_cannot_reopen_after_exhaustion() {
        let mut history = owner().with_request_budget(2, "shared budget");
        next_fetch(&mut history);
        history.accept_page();
        assert_eq!(history.step(), Step::Finished);
        assert!(history.restart_stage(RangeWalk::new(0., 10., 10., 200)));
        next_fetch(&mut history);
        history.accept_page();
        assert_eq!(history.step(), Step::Finished);
        assert!(history.restart_stage(RangeWalk::new(0., 10., 10., 200)));
        assert_eq!(history.step(), Step::Finished, "third fetch never issued");
        assert!(history.active.is_none());
        assert_eq!(
            history.rows.completion.incomplete.as_deref(),
            Some("shared budget")
        );
        assert!(history.restart_stage(RangeWalk::new(0., 10., 10., 200)));
        assert_eq!(
            history.step(),
            Step::Finished,
            "restart never renews budget"
        );
    }
    #[test]
    fn restarted_owner_cannot_replace_its_budget() {
        let mut history = owner().with_request_budget(1, "original budget");
        next_fetch(&mut history);
        history.accept_page();
        assert_eq!(history.step(), Step::Finished);
        assert!(history.restart_stage(RangeWalk::new(0., 10., 10., 200)));
        let mut history = history.with_request_budget(100, "reset budget");
        assert!(history.lifecycle_error.is_some());
        assert_eq!(history.step(), Step::Finished);
        assert!(history.active.is_none());
        assert_eq!(history.request_budget, Some((1, "original budget".into())));
    }

    #[test]
    fn clipped_native_range_preserves_outside_row_coverage_without_continuation() {
        let mut history = RangeHistory::new(
            Completion::new(71),
            RangeWalk::new(0., 1., 1., 200),
            |_| "gap".into(),
            |_: &[String]| Ok("[]".into()),
        );
        assert_eq!(next_fetch(&mut history), (0., 1.));
        assert!(history.restrict_active_range(0., 0.));
        history.stage(
            "outside".into(),
            PageRow {
                timestamp: 1.,
                identity: "outside".into(),
            },
        );
        history.accept_page_where(|_| false);
        assert_eq!(history.accepted_len(), 0);
        assert_eq!(
            history.step(),
            Step::Finished,
            "outside row cannot extend the native range"
        );
        assert!(history.walk.incomplete_chunks() > 0);
    }

    #[test]
    fn native_bounds_cannot_expand_invert_or_change_after_staging() {
        for (from, to) in [(-1., 1.), (0., 11.), (2., 1.), (f64::NAN, 1.)] {
            let mut history = owner();
            next_fetch(&mut history);
            assert!(!history.restrict_active_range(from, to));
            assert_eq!(history.active, Some((0., 10.)));
            assert!(history.lifecycle_error.is_some());
        }
        let mut history = owner();
        assert!(!history.restrict_active_range(0., 1.));
        let mut history = owner();
        next_fetch(&mut history);
        history.stage(
            "pending".into(),
            PageRow {
                timestamp: 1.,
                identity: "pending".into(),
            },
        );
        assert!(!history.restrict_active_range(0., 1.));
        assert_eq!(history.rows.pending, ["pending"]);
        assert_eq!(history.active, Some((0., 10.)));
    }
}

#[cfg(test)]
mod read_page_identity_tests {
    use super::{CursorWalk, PageIdentities};

    #[test]
    fn unvalidated_read_page_does_not_enter_committed_cursor_identities() {
        let walk = CursorWalk::new(3);
        let mut pending = PageIdentities::default();
        assert!(pending.fresh(&walk, "unvalidated".into()));
        assert!(!pending.fresh(&walk, "unvalidated".into()));
        assert!(!walk.contains_identity("unvalidated"));
    }

    #[test]
    fn failed_read_page_discard_keeps_its_rows_fresh_for_a_retry() {
        let walk = CursorWalk::new(3);
        let mut pending = PageIdentities::default();
        assert!(pending.fresh(&walk, "retry".into()));
        pending.discard();
        assert!(pending.fresh(&walk, "retry".into()));
    }

    #[test]
    fn admitted_read_identities_remain_deduped_across_later_stages() {
        let mut walk = CursorWalk::new(3);
        let mut pending = PageIdentities::default();
        assert!(pending.fresh(&walk, "accepted".into()));
        pending.commit(&mut walk);
        assert!(walk.contains_identity("accepted"));
        walk.restart();
        assert!(!pending.fresh(&walk, "accepted".into()));
        assert!(pending.fresh(&walk, "new".into()));
    }
}

#[cfg(test)]
#[path = "history_context_tests.rs"]
mod context_tests;
