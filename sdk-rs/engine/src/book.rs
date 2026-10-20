//! Price-sorted order book. Levels are (price, amount) pairs; cumulative totals
//! are computed only when the book is written to a frame, not on every update.
use crate::wire::{FrameWriter, section};
#[cfg(feature = "book-deltas")]
use std::sync::atomic::{AtomicU64, Ordering};

// A venue may replace the whole Book on a gap. Instance-local counters would
// mistake two separately loaded snapshots for the same generation.
#[cfg(feature = "book-deltas")]
static NEXT_RESET: AtomicU64 = AtomicU64::new(1);

#[cfg(feature = "book-deltas")]
fn next_reset() -> u64 {
    NEXT_RESET.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookSide {
    Bids,
    Asks,
}

#[derive(Debug)]
#[cfg_attr(not(feature = "book-deltas"), derive(Clone, Default))]
pub struct Book {
    /// Highest price first.
    bids: Vec<(f64, f64)>,
    /// Lowest price first.
    asks: Vec<(f64, f64)>,
    #[cfg(feature = "book-deltas")]
    reset: u64,
}

#[cfg(feature = "book-deltas")]
impl Default for Book {
    fn default() -> Self {
        Self {
            bids: Vec::new(),
            asks: Vec::new(),
            reset: next_reset(),
        }
    }
}

#[cfg(feature = "book-deltas")]
impl Clone for Book {
    fn clone(&self) -> Self {
        Self {
            bids: self.bids.clone(),
            asks: self.asks.clone(),
            reset: self.reset,
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.bids.clone_from(&source.bids);
        self.asks.clone_from(&source.asks);
        self.reset = source.reset;
    }
}

impl Book {
    pub fn clear(&mut self) {
        #[cfg(feature = "book-deltas")]
        {
            self.reset = next_reset();
        }
        self.bids.clear();
        self.asks.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.bids.is_empty() && self.asks.is_empty()
    }

    /// Replace one side with already-sorted levels (venue snapshots arrive sorted).
    pub fn load_sorted(&mut self, side: BookSide, levels: impl IntoIterator<Item = (f64, f64)>) {
        #[cfg(feature = "book-deltas")]
        {
            self.reset = next_reset();
        }
        let target = self.side_mut(side);
        target.clear();
        target.extend(levels.into_iter().filter(|(_, amount)| *amount != 0.0));
    }

    /// Adopt a decoded snapshot without allocating a second copy of its levels.
    pub fn load_sorted_owned(&mut self, side: BookSide, mut levels: Vec<(f64, f64)>) {
        #[cfg(feature = "book-deltas")]
        {
            self.reset = next_reset();
        }
        levels.retain(|(_, amount)| *amount != 0.0);
        *self.side_mut(side) = levels;
    }

    /// Upsert one level; an amount of zero removes it.
    pub fn apply(&mut self, side: BookSide, price: f64, amount: f64) {
        let levels = self.side_mut(side);
        let found = match side {
            BookSide::Bids => levels.binary_search_by(|(p, _)| price.total_cmp(p)),
            BookSide::Asks => levels.binary_search_by(|(p, _)| p.total_cmp(&price)),
        };
        match (found, amount == 0.0) {
            (Ok(index), true) => {
                levels.remove(index);
            }
            (Ok(index), false) => levels[index].1 = amount,
            (Err(_), true) => {}
            (Err(index), false) => levels.insert(index, (price, amount)),
        }
    }

    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|(price, _)| *price)
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|(price, _)| *price)
    }

    pub fn bids(&self) -> &[(f64, f64)] {
        &self.bids
    }

    pub fn asks(&self) -> &[(f64, f64)] {
        &self.asks
    }

    #[cfg(feature = "book-deltas")]
    pub(crate) fn reset_generation(&self) -> u64 {
        self.reset
    }

    /// Absolute changed levels, with amount zero for deletions. Totals are
    /// derived by the reader after merging these rows into its sorted book.
    pub fn write_delta(&self, previous: &Self, market: u32, w: &mut FrameWriter<'_>) -> bool {
        let mut bids = 0u32;
        let mut asks = 0u32;
        differences(&previous.bids, &self.bids, true, |_, _| bids += 1);
        differences(&previous.asks, &self.asks, false, |_, _| asks += 1);
        if bids == 0 && asks == 0 {
            return false;
        }
        w.begin(section::BOOK_DELTA);
        w.u32s([market, bids, asks, 0]);
        differences(&previous.bids, &self.bids, true, |price, amount| {
            w.f64s([price, amount]);
        });
        differences(&previous.asks, &self.asks, false, |price, amount| {
            w.f64s([price, amount]);
        });
        w.end();
        true
    }

    /// Write the full book as a `BOOK` section: rows of `[price, amount, total]`.
    pub fn write(&self, market: u32, w: &mut FrameWriter<'_>) {
        w.begin(section::BOOK);
        w.u32s([market, self.bids.len() as u32, self.asks.len() as u32, 0]);
        for levels in [&self.bids, &self.asks] {
            let mut total = 0.0;
            for &(price, amount) in levels.iter() {
                total += amount;
                w.f64s([price, amount, total]);
            }
        }
        w.end();
    }

    fn side_mut(&mut self, side: BookSide) -> &mut Vec<(f64, f64)> {
        match side {
            BookSide::Bids => &mut self.bids,
            BookSide::Asks => &mut self.asks,
        }
    }
}

fn differences(
    previous: &[(f64, f64)],
    current: &[(f64, f64)],
    descending: bool,
    mut changed: impl FnMut(f64, f64),
) {
    let (mut old, mut new) = (0, 0);
    while old < previous.len() || new < current.len() {
        match (previous.get(old), current.get(new)) {
            (Some(&(old_price, old_amount)), Some(&(price, amount))) => {
                let order = if descending {
                    price.total_cmp(&old_price)
                } else {
                    old_price.total_cmp(&price)
                };
                match order {
                    std::cmp::Ordering::Less => {
                        changed(old_price, 0.0);
                        old += 1;
                    }
                    std::cmp::Ordering::Greater => {
                        changed(price, amount);
                        new += 1;
                    }
                    std::cmp::Ordering::Equal => {
                        if old_amount != amount {
                            changed(price, amount);
                        }
                        old += 1;
                        new += 1;
                    }
                }
            }
            (Some(&(price, _)), None) => {
                changed(price, 0.0);
                old += 1;
            }
            (None, Some(&(price, amount))) => {
                changed(price, amount);
                new += 1;
            }
            (None, None) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adopts_snapshot_allocation_and_removes_empty_levels() {
        let mut book = Book::default();
        book.apply(BookSide::Bids, 100.0, 1.0);
        let levels = vec![(12.0, 2.0), (11.0, 0.0), (10.0, 3.0)];
        let allocation = levels.as_ptr();
        book.load_sorted_owned(BookSide::Bids, levels);
        assert_eq!(book.bids().as_ptr(), allocation);
        assert_eq!(book.bids(), [(12.0, 2.0), (10.0, 3.0)]);
    }

    #[test]
    fn keeps_sides_sorted_and_removes_zero_levels() {
        let mut book = Book::default();
        for (price, amount) in [(10.0, 1.0), (12.0, 2.0), (11.0, 3.0)] {
            book.apply(BookSide::Bids, price, amount);
            book.apply(BookSide::Asks, price + 10.0, amount);
        }
        book.apply(BookSide::Bids, 11.0, 0.0);
        book.apply(BookSide::Asks, 22.0, 5.0);
        book.apply(BookSide::Asks, 99.0, 0.0);
        assert_eq!(book.bids(), [(12.0, 2.0), (10.0, 1.0)]);
        assert_eq!(book.asks(), [(20.0, 1.0), (21.0, 3.0), (22.0, 5.0)]);
        assert_eq!((book.best_bid(), book.best_ask()), (Some(12.0), Some(20.0)));
    }

    #[test]
    fn delta_rows_reconstruct_sorted_books_and_cumulative_totals() {
        let mut actual = Book::default();
        for index in 1..=30 {
            actual.apply(BookSide::Bids, index as f64, 1.);
            actual.apply(BookSide::Asks, 40. + index as f64, 2.);
        }
        let previous = actual.clone();
        let mut rebuilt = previous.clone();
        #[cfg(feature = "book-deltas")]
        {
            let allocation = rebuilt.bids.as_ptr();
            rebuilt.clone_from(&previous);
            assert_eq!(
                allocation,
                rebuilt.bids.as_ptr(),
                "baseline copying reuses capacity"
            );
        }
        // Cover removals before, within and after surviving levels, inserts
        // between levels, unchanged rows and complete removal of one side.
        for price in [30., 15., 1.] {
            actual.apply(BookSide::Bids, price, 0.);
        }
        for price in [31., 20.5, 0.5] {
            actual.apply(BookSide::Bids, price, 3.);
        }
        actual.apply(BookSide::Bids, 18., 9.);
        actual.load_sorted(BookSide::Asks, []);
        let mut writer = FrameWriter::new(Vec::new(), 1.);
        assert!(actual.write_delta(&previous, 7, &mut writer));
        let frame = writer.finish();
        let (kind, payload) = crate::wire::sections(&frame).next().unwrap();
        assert_eq!(kind, section::BOOK_DELTA);
        let word = |at: usize| u32::from_le_bytes(payload[at..at + 4].try_into().unwrap()) as usize;
        assert_eq!(word(0), 7);
        let bids = word(4);
        for (index, pair) in payload[16..].as_chunks::<16>().0.iter().enumerate() {
            let price = f64::from_le_bytes(pair[..8].try_into().unwrap());
            let amount = f64::from_le_bytes(pair[8..].try_into().unwrap());
            rebuilt.apply(
                if index < bids {
                    BookSide::Bids
                } else {
                    BookSide::Asks
                },
                price,
                amount,
            );
        }
        assert_eq!(rebuilt.bids(), actual.bids());
        assert_eq!(rebuilt.asks(), actual.asks());
        let full = |book: &Book| {
            let mut writer = FrameWriter::new(Vec::new(), 1.);
            book.write(7, &mut writer);
            writer.finish()
        };
        assert_eq!(
            full(&rebuilt),
            full(&actual),
            "derived totals equal a full-book frame"
        );
        let mut writer = FrameWriter::new(Vec::new(), 2.);
        assert!(!actual.write_delta(&actual, 7, &mut writer));
        assert!(writer.is_empty());
    }
}
