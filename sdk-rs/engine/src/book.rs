//! Price-sorted order book. Levels are (price, amount) pairs; cumulative totals
//! are computed only when the book is written to a frame, not on every update.
use crate::wire::{FrameWriter, section};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookSide {
    Bids,
    Asks,
}

#[derive(Clone, Debug, Default)]
pub struct Book {
    /// Highest price first.
    bids: Vec<(f64, f64)>,
    /// Lowest price first.
    asks: Vec<(f64, f64)>,
}

impl Book {
    pub fn clear(&mut self) {
        self.bids.clear();
        self.asks.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.bids.is_empty() && self.asks.is_empty()
    }

    /// Replace one side with already-sorted levels (venue snapshots arrive sorted).
    pub fn load_sorted(&mut self, side: BookSide, levels: impl IntoIterator<Item = (f64, f64)>) {
        let target = self.side_mut(side);
        target.clear();
        target.extend(levels.into_iter().filter(|(_, amount)| *amount != 0.0));
    }

    /// Adopt a decoded snapshot without allocating a second copy of its levels.
    pub fn load_sorted_owned(&mut self, side: BookSide, mut levels: Vec<(f64, f64)>) {
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

    /// Write the full book as a `BOOK` section: rows of `[price, amount, total]`.
    pub fn write(&self, market: u32, w: &mut FrameWriter<'_>) {
        w.reserve(self.frame_size_hint());
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

    pub(crate) fn frame_size_hint(&self) -> usize {
        24 + 24 * (self.bids.len() + self.asks.len())
    }

    fn side_mut(&mut self, side: BookSide) -> &mut Vec<(f64, f64)> {
        match side {
            BookSide::Bids => &mut self.bids,
            BookSide::Asks => &mut self.asks,
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
}
