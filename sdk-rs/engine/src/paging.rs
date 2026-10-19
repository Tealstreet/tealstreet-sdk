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
