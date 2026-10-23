//! Time-window paging whose completeness is decided by span, not by a full
//! page. A port of `packages/safe-cex/src/exchanges/income-paging.ts`; read its
//! header for why each rule exists. The walk is a state machine so venues can
//! drive it from responses without async code.
use std::collections::HashSet;

/// One row as the walk sees it: its time and a key unique per record.
pub struct PageRow {
    pub timestamp: f64,
    pub identity: String,
}

pub struct RangeWalk {
    start: f64,
    end: f64,
    chunk_ms: f64,
    page_limit: usize,
    max_pages_per_chunk: u32,
    chunk_start: f64,
    pending: Vec<(f64, f64)>,
    pages: u32,
    chunk_incomplete: bool,
    seen: HashSet<String>,
    pub truncated: bool,
    pub exhausted_chunks: u32,
    pub requests: u32,
}

/// What the walk wants next.
#[derive(Debug, PartialEq)]
pub enum Step {
    /// Request this inclusive window.
    Fetch {
        from: f64,
        to: f64,
    },
    /// The chunk is done; wait the venue's chunk delay, then call `next` again.
    ChunkDone,
    Finished,
}

impl RangeWalk {
    pub fn new(start: f64, end: f64, chunk_ms: f64, page_limit: usize) -> Self {
        let valid = end > start && chunk_ms > 0.0 && page_limit > 0;
        Self {
            start,
            end,
            chunk_ms,
            page_limit,
            max_pages_per_chunk: 50,
            // An invalid range starts past its end, so `next` finishes at once.
            chunk_start: if valid { start } else { end },
            pending: if valid {
                vec![(start, end.min(start + chunk_ms))]
            } else {
                Vec::new()
            },
            pages: 0,
            chunk_incomplete: false,
            seen: HashSet::new(),
            truncated: false,
            exhausted_chunks: 0,
            requests: 0,
        }
    }

    pub fn step(&mut self) -> Step {
        loop {
            if self.pages >= self.max_pages_per_chunk {
                self.chunk_incomplete = true;
                self.pending.clear();
            }
            if let Some((from, to)) = self.pending.pop() {
                if from > to {
                    continue;
                }
                self.pages += 1;
                self.requests += 1;
                return Step::Fetch { from, to };
            }
            if self.chunk_incomplete {
                self.truncated = true;
                self.exhausted_chunks += 1;
            }
            self.chunk_start += self.chunk_ms;
            if self.chunk_start >= self.end {
                return Step::Finished;
            }
            self.pages = 0;
            self.chunk_incomplete = false;
            self.pending.push((
                self.chunk_start,
                self.end.min(self.chunk_start + self.chunk_ms),
            ));
            return Step::ChunkDone;
        }
    }

    /// Known incomplete chunks, including an active chunk whose gap has not
    /// reached rollover yet. A finished last chunk is already counted.
    pub fn incomplete_chunks(&self) -> u32 {
        self.exhausted_chunks.saturating_add(u32::from(
            self.chunk_start < self.end && self.chunk_incomplete,
        ))
    }

    /// A failed page leaves its rows unknown, not absent.
    pub fn failed(&mut self) {
        self.chunk_incomplete = true;
    }

    /// Record a page for window `(from, to)`; returns the indexes of rows that
    /// are new and inside the requested range.
    pub fn page(&mut self, from: f64, to: f64, rows: &[PageRow]) -> Vec<usize> {
        let mut fresh = Vec::new();
        if rows.is_empty() {
            return fresh;
        }
        let mut oldest = f64::INFINITY;
        let mut newest = f64::NEG_INFINITY;
        let mut inside = 0;
        let mut times = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let t = row.timestamp;
            if !t.is_finite() || t < self.start || t > self.end {
                continue;
            }
            times.push(t);
            if t >= from && t <= to {
                inside += 1;
            }
            oldest = oldest.min(t);
            newest = newest.max(t);
            if self.seen.insert(row.identity.clone()) {
                fresh.push(index);
            }
        }
        if !oldest.is_finite() || inside == 0 {
            self.chunk_incomplete = true;
            return fresh;
        }
        let direction = direction(&times);
        if fresh.is_empty() {
            if rows.len() < self.page_limit {
                return fresh;
            }
            self.chunk_incomplete = true;
            if direction != Direction::Ascending && oldest - 1.0 >= from {
                self.pending.push((from, oldest - 1.0));
            }
            if direction != Direction::Descending && newest + 1.0 <= to {
                self.pending.push((newest + 1.0, to));
            }
            return fresh;
        }
        if direction != Direction::Ascending && oldest >= from {
            self.pending.push((from, oldest));
        }
        if direction != Direction::Descending && newest <= to {
            self.pending.push((newest, to));
        }
        fresh
    }
}

/// Which end of a range the first window covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowOrder {
    NewestFirst,
    OldestFirst,
}

/// Splits the inclusive range `[from, to]` into disjoint inclusive windows
/// `[a, b]` with `b - a <= width`, each starting 1 ms past its neighbour, as
/// venues with a maximum query span require. `None` when more than
/// `max_windows` would be needed or the range is empty or invalid.
pub fn windows(
    from: f64,
    to: f64,
    width: f64,
    order: WindowOrder,
    max_windows: usize,
) -> Option<std::collections::VecDeque<(f64, f64)>> {
    if !(from <= to && width >= 0.0 && from.is_finite() && to.is_finite() && width.is_finite()) {
        return None;
    }
    let mut out = std::collections::VecDeque::new();
    match order {
        WindowOrder::OldestFirst => {
            let mut a = from;
            while a <= to {
                if out.len() >= max_windows {
                    return None;
                }
                let b = (a + width).min(to);
                out.push_back((a, b));
                // Past 2^53 the step can round away; refuse instead of looping.
                if b + 1.0 <= a {
                    return None;
                }
                a = b + 1.0;
            }
        }
        WindowOrder::NewestFirst => {
            let mut b = to;
            while b >= from {
                if out.len() >= max_windows {
                    return None;
                }
                let a = (b - width).max(from);
                out.push_back((a, b));
                if a - 1.0 >= b {
                    return None;
                }
                b = a - 1.0;
            }
        }
    }
    Some(out)
}

/// Why a continuation walk stopped before the venue said it was done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stall {
    /// The venue returned a cursor it had already returned.
    RepeatedCursor,
    /// A page held no rows yet carried a cursor.
    EmptyPage,
    /// Every row on the page had been seen before.
    NoNewRows,
    /// The page budget ran out with a cursor still pending.
    PageBudget,
}

/// What a continuation walk wants after a page.
#[derive(Debug, PartialEq, Eq)]
pub enum CursorStep {
    /// Request the next page with this cursor.
    More(String),
    Done,
    Stalled(Stall),
}

/// One completed page as the walk sees it.
pub struct CursorPage {
    /// The continuation the venue returned; empty means none.
    pub next: Option<String>,
    /// Rows on the page as returned, before any filtering.
    pub rows: usize,
    /// Rows the walk had not seen before (see [`CursorWalk::fresh`]).
    pub fresh: usize,
    /// The venue still wants rows (its own limit and window checks).
    pub want_more: bool,
}

/// Continuation paging by an opaque token or id the venue returns, until it
/// stops returning one. A venue opts into each stall rule it enforces, so a
/// read that cannot progress fails closed instead of returning partial rows.
pub struct CursorWalk {
    max_pages: u32,
    pages: u32,
    stall_on_empty_page: bool,
    stall_on_no_new_rows: bool,
    end_on_no_new_rows: bool,
    end_on_unchanged_cursor: bool,
    cursor: Option<String>,
    cursors: HashSet<String>,
    seen: HashSet<String>,
}

impl CursorWalk {
    pub fn new(max_pages: u32) -> Self {
        Self {
            max_pages,
            pages: 0,
            stall_on_empty_page: false,
            stall_on_no_new_rows: false,
            end_on_no_new_rows: false,
            end_on_unchanged_cursor: false,
            cursor: None,
            cursors: HashSet::new(),
            seen: HashSet::new(),
        }
    }

    /// Construct a walk whose first request already has an endpoint bound.
    pub fn starting_at(max_pages: u32, cursor: String) -> Self {
        let mut walk = Self::new(max_pages);
        walk.cursors.insert(cursor.clone());
        walk.cursor = Some(cursor);
        walk
    }

    /// Stall when a page holds no rows but carries a cursor.
    pub fn stall_on_empty_page(mut self) -> Self {
        self.stall_on_empty_page = true;
        self
    }

    /// Stall when a nonempty page holds only rows seen before.
    pub fn stall_on_no_new_rows(mut self) -> Self {
        self.stall_on_no_new_rows = true;
        self
    }

    /// Some endpoints prove exhaustion by repeating the last page's identities.
    /// Keep this distinct from a no-progress failure on other endpoints.
    pub fn end_on_no_new_rows(mut self) -> Self {
        self.end_on_no_new_rows = true;
        self
    }

    /// Some endpoints use the same continuation to signal exhaustion.
    pub fn end_on_unchanged_cursor(mut self) -> Self {
        self.end_on_unchanged_cursor = true;
        self
    }

    /// The cursor for the next request; `None` for the first page.
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    /// Pages recorded so far.
    pub fn pages(&self) -> u32 {
        self.pages
    }

    /// Records a row identity; true when it was not seen before.
    pub fn fresh(&mut self, identity: String) -> bool {
        self.seen.insert(identity)
    }

    /// Inspect committed identities without admitting an unvalidated page.
    pub(crate) fn contains_identity(&self, identity: &str) -> bool {
        self.seen.contains(identity)
    }

    /// Starts another stage of the same read: a new first page, keeping the
    /// rows already seen.
    pub fn restart(&mut self) {
        self.pages = 0;
        self.cursor = None;
        self.cursors.clear();
    }

    /// Records a completed page and decides what follows it.
    pub fn advance(&mut self, page: CursorPage) -> CursorStep {
        self.pages += 1;
        let Some(next) = page.next.filter(|next| !next.is_empty()) else {
            return CursorStep::Done;
        };
        if !page.want_more
            || (self.end_on_no_new_rows && page.fresh == 0)
            || (self.end_on_unchanged_cursor && self.cursor.as_ref() == Some(&next))
        {
            return CursorStep::Done;
        }
        if !self.cursors.insert(next.clone()) {
            return CursorStep::Stalled(Stall::RepeatedCursor);
        }
        if self.stall_on_empty_page && page.rows == 0 {
            return CursorStep::Stalled(Stall::EmptyPage);
        }
        if self.stall_on_no_new_rows && page.rows > 0 && page.fresh == 0 {
            return CursorStep::Stalled(Stall::NoNewRows);
        }
        if self.pages >= self.max_pages {
            return CursorStep::Stalled(Stall::PageBudget);
        }
        self.cursor = Some(next.clone());
        CursorStep::More(next)
    }
}

/// What an offset walk wants after a page.
#[derive(Debug, PartialEq, Eq)]
pub enum OffsetStep {
    /// Request the next page at this row offset (also [`OffsetWalk::page`]).
    More(u64),
    Done,
    Stalled(Stall),
}

/// Offset or page-number paging. The offset advances by the rows a page
/// actually returned, so a venue that serves short pages skips nothing; an
/// empty page (or, opted in, a page of no new rows) while the venue reports
/// more cannot progress and stalls.
pub struct OffsetWalk {
    max_pages: u32,
    max_offset: u64,
    fixed_step: Option<u64>,
    stall_on_no_new_rows: bool,
    pages: u32,
    offset: u64,
}

impl OffsetWalk {
    pub fn new(max_pages: u32) -> Self {
        Self {
            max_pages,
            max_offset: u64::MAX,
            fixed_step: None,
            stall_on_no_new_rows: false,
            pages: 0,
            offset: 0,
        }
    }

    /// Stall once the next offset would pass `max_offset` (inclusive).
    pub fn max_offset(mut self, max_offset: u64) -> Self {
        self.max_offset = max_offset;
        self
    }

    /// Advance by a fixed page size instead of the rows returned, for venues
    /// whose contract serves full pages while they report more.
    pub fn fixed_step(mut self, step: u64) -> Self {
        self.fixed_step = Some(step);
        self
    }

    /// Stall when a nonempty page holds only rows seen before.
    pub fn stall_on_no_new_rows(mut self) -> Self {
        self.stall_on_no_new_rows = true;
        self
    }

    /// Row offset of the page to request.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// One-based number of the page to request.
    pub fn page(&self) -> u32 {
        self.pages + 1
    }

    /// Starts another stage of the same read from its first page.
    pub fn restart(&mut self) {
        self.pages = 0;
        self.offset = 0;
    }

    /// Records a page of `rows` (of which `fresh` were new) and whether the
    /// venue reports more.
    pub fn advance(&mut self, rows: usize, fresh: usize, more: bool) -> OffsetStep {
        self.pages += 1;
        if !more {
            return OffsetStep::Done;
        }
        if rows == 0 {
            return OffsetStep::Stalled(Stall::EmptyPage);
        }
        if self.stall_on_no_new_rows && fresh == 0 {
            return OffsetStep::Stalled(Stall::NoNewRows);
        }
        if self.pages >= self.max_pages {
            return OffsetStep::Stalled(Stall::PageBudget);
        }
        let next = self
            .offset
            .saturating_add(self.fixed_step.unwrap_or(rows as u64));
        if next > self.max_offset {
            return OffsetStep::Stalled(Stall::PageBudget);
        }
        self.offset = next;
        OffsetStep::More(next)
    }
}

#[derive(PartialEq)]
enum Direction {
    Ascending,
    Descending,
    Unknown,
}

fn direction(times: &[f64]) -> Direction {
    if times.len() < 2 {
        return Direction::Unknown;
    }
    let ascending = times.windows(2).all(|w| w[1] >= w[0]);
    let descending = times.windows(2).all(|w| w[1] <= w[0]);
    match (ascending, descending) {
        (true, false) => Direction::Ascending,
        (false, true) => Direction::Descending,
        _ => Direction::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(times: &[f64]) -> Vec<PageRow> {
        times
            .iter()
            .map(|&t| PageRow {
                timestamp: t,
                identity: format!("{t}"),
            })
            .collect()
    }

    #[test]
    fn walks_an_ascending_venue_past_a_full_page() {
        let mut walk = RangeWalk::new(0.0, 100.0, 1_000.0, 3);
        assert_eq!(
            walk.step(),
            Step::Fetch {
                from: 0.0,
                to: 100.0
            }
        );
        assert_eq!(walk.page(0.0, 100.0, &rows(&[1.0, 2.0, 3.0])).len(), 3);
        assert_eq!(
            walk.step(),
            Step::Fetch {
                from: 3.0,
                to: 100.0
            }
        );
        assert_eq!(walk.page(3.0, 100.0, &rows(&[3.0, 4.0])).len(), 1);
        assert_eq!(
            walk.step(),
            Step::Fetch {
                from: 4.0,
                to: 100.0
            }
        );
        assert!(walk.page(4.0, 100.0, &rows(&[4.0])).is_empty());
        assert_eq!(walk.step(), Step::Finished);
        assert!(!walk.truncated);
    }

    fn page(next: Option<&str>, rows: usize, fresh: usize) -> CursorPage {
        CursorPage {
            next: next.map(str::to_owned),
            rows,
            fresh,
            want_more: true,
        }
    }

    #[test]
    fn cursor_walk_follows_tokens_until_the_venue_stops_returning_one() {
        let mut walk = CursorWalk::new(10);
        assert_eq!(walk.cursor(), None);
        assert_eq!(
            walk.advance(page(Some("a"), 2, 2)),
            CursorStep::More("a".into())
        );
        assert_eq!(walk.cursor(), Some("a"));
        assert_eq!(walk.advance(page(Some(""), 1, 1)), CursorStep::Done);
        assert_eq!(walk.pages(), 2);
    }

    #[test]
    fn cursor_walk_stops_when_the_venue_has_enough_even_with_a_cursor() {
        let mut walk = CursorWalk::new(10);
        let mut full = page(Some("a"), 5, 5);
        full.want_more = false;
        assert_eq!(walk.advance(full), CursorStep::Done);
    }

    #[test]
    fn cursor_walk_fails_closed_on_repeats_budget_and_opted_in_stalls() {
        let mut walk = CursorWalk::new(10);
        assert_eq!(
            walk.advance(page(Some("a"), 1, 1)),
            CursorStep::More("a".into())
        );
        assert_eq!(
            walk.advance(page(Some("a"), 1, 1)),
            CursorStep::Stalled(Stall::RepeatedCursor)
        );

        let mut walk = CursorWalk::new(2);
        assert_eq!(
            walk.advance(page(Some("a"), 1, 1)),
            CursorStep::More("a".into())
        );
        assert_eq!(
            walk.advance(page(Some("b"), 1, 1)),
            CursorStep::Stalled(Stall::PageBudget)
        );
        // The last page may still end the walk inside the budget.
        let mut walk = CursorWalk::new(1);
        assert_eq!(walk.advance(page(None, 1, 1)), CursorStep::Done);

        // Empty and repeated pages pass unless the venue opts in.
        let mut walk = CursorWalk::new(10);
        assert_eq!(
            walk.advance(page(Some("a"), 0, 0)),
            CursorStep::More("a".into())
        );
        assert_eq!(
            walk.advance(page(Some("b"), 3, 0)),
            CursorStep::More("b".into())
        );
        let mut walk = CursorWalk::new(10)
            .stall_on_empty_page()
            .stall_on_no_new_rows();
        assert_eq!(
            walk.advance(page(Some("a"), 0, 0)),
            CursorStep::Stalled(Stall::EmptyPage)
        );
        let mut walk = CursorWalk::new(10).stall_on_no_new_rows();
        assert_eq!(
            walk.advance(page(Some("a"), 3, 0)),
            CursorStep::Stalled(Stall::NoNewRows)
        );
    }

    #[test]
    fn cursor_walk_restart_keeps_seen_rows_and_forgets_cursors() {
        let mut walk = CursorWalk::new(2);
        assert!(walk.fresh("x".into()));
        assert_eq!(
            walk.advance(page(Some("a"), 1, 1)),
            CursorStep::More("a".into())
        );
        walk.restart();
        assert_eq!(walk.cursor(), None);
        assert_eq!(walk.pages(), 0);
        assert!(!walk.fresh("x".into()));
        assert_eq!(
            walk.advance(page(Some("a"), 1, 1)),
            CursorStep::More("a".into())
        );
    }

    #[test]
    fn windows_tile_the_range_disjointly_in_either_order() {
        let oldest: Vec<_> = windows(0.0, 25.0, 9.0, WindowOrder::OldestFirst, 10)
            .unwrap()
            .into();
        assert_eq!(oldest, [(0.0, 9.0), (10.0, 19.0), (20.0, 25.0)]);
        let newest: Vec<_> = windows(0.0, 25.0, 9.0, WindowOrder::NewestFirst, 10)
            .unwrap()
            .into();
        assert_eq!(newest, [(16.0, 25.0), (6.0, 15.0), (0.0, 5.0)]);
        // A single millisecond, an exact fit and the window budget.
        assert_eq!(
            windows(5.0, 5.0, 9.0, WindowOrder::NewestFirst, 1).unwrap(),
            [(5.0, 5.0)]
        );
        assert_eq!(
            windows(0.0, 9.0, 9.0, WindowOrder::OldestFirst, 1).unwrap(),
            [(0.0, 9.0)]
        );
        assert!(windows(0.0, 10.0, 9.0, WindowOrder::OldestFirst, 1).is_none());
        assert!(windows(2.0, 1.0, 9.0, WindowOrder::OldestFirst, 10).is_none());
        assert!(windows(0.0, f64::NAN, 9.0, WindowOrder::OldestFirst, 10).is_none());
        // A step that rounds away fails instead of looping forever.
        assert!(windows(0.0, 1e300, 9.0, WindowOrder::NewestFirst, usize::MAX).is_none());
    }

    #[test]
    fn offset_walk_advances_by_rows_returned_and_fails_closed() {
        let mut walk = OffsetWalk::new(10);
        assert_eq!((walk.offset(), walk.page()), (0, 1));
        // A short page advances by what it returned, not the page size.
        assert_eq!(walk.advance(3, 3, true), OffsetStep::More(3));
        assert_eq!((walk.offset(), walk.page()), (3, 2));
        assert_eq!(walk.advance(2, 2, false), OffsetStep::Done);
        let mut walk = OffsetWalk::new(10);
        assert_eq!(
            walk.advance(0, 0, true),
            OffsetStep::Stalled(Stall::EmptyPage)
        );
        let mut walk = OffsetWalk::new(10);
        assert_eq!(walk.advance(5, 0, true), OffsetStep::More(5));
        let mut walk = OffsetWalk::new(10).stall_on_no_new_rows();
        assert_eq!(
            walk.advance(5, 0, true),
            OffsetStep::Stalled(Stall::NoNewRows)
        );
        let mut walk = OffsetWalk::new(2);
        assert_eq!(walk.advance(5, 5, true), OffsetStep::More(5));
        assert_eq!(
            walk.advance(5, 5, true),
            OffsetStep::Stalled(Stall::PageBudget)
        );
        let mut walk = OffsetWalk::new(10).max_offset(9);
        assert_eq!(walk.advance(5, 5, true), OffsetStep::More(5));
        assert_eq!(
            walk.advance(5, 5, true),
            OffsetStep::Stalled(Stall::PageBudget)
        );
        walk.restart();
        assert_eq!((walk.offset(), walk.page()), (0, 1));
        let mut walk = OffsetWalk::new(10).fixed_step(100);
        assert_eq!(walk.advance(3, 3, true), OffsetStep::More(100));
    }

    #[test]
    fn a_failed_page_marks_the_walk_incomplete() {
        let mut walk = RangeWalk::new(0.0, 20.0, 10.0, 10);
        assert_eq!(
            walk.step(),
            Step::Fetch {
                from: 0.0,
                to: 10.0
            }
        );
        walk.failed();
        assert_eq!(walk.step(), Step::ChunkDone);
        assert_eq!(
            walk.step(),
            Step::Fetch {
                from: 10.0,
                to: 20.0
            }
        );
        assert_eq!(walk.step(), Step::Finished);
        assert!(walk.truncated);
        assert_eq!(walk.exhausted_chunks, 1);
    }
}
