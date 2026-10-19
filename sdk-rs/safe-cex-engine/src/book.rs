//! Price-sorted order book. Levels are (price, amount) pairs; cumulative totals
//! are computed only when the book is emitted, not on every update.
//!
//! Every change is also logged as an op so a host can mirror the book without
//! copying it: triples of `[code, price, amount]`, where code is one of the
//! `OP_*` constants. Hosts drain the log with [`Book::take_ops`].
use crate::model::{BookLevel, OrderBook};

pub const OP_BID: f64 = 0.0;
pub const OP_ASK: f64 = 1.0;
/// Both sides emptied.
pub const OP_RESET: f64 = 2.0;
pub const OP_RESET_BIDS: f64 = 3.0;
pub const OP_RESET_ASKS: f64 = 4.0;

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
    ops: Vec<f64>,
}

impl Book {
    pub fn clear(&mut self) {
        self.bids.clear();
        self.asks.clear();
        self.ops.clear();
        self.ops.extend([OP_RESET, 0.0, 0.0]);
    }

    pub fn is_empty(&self) -> bool {
        self.bids.is_empty() && self.asks.is_empty()
    }

    /// Replace one side with already-sorted levels (venue snapshots arrive sorted).
    pub fn load_sorted(&mut self, side: BookSide, levels: impl IntoIterator<Item = (f64, f64)>) {
        let (reset, code) = match side {
            BookSide::Bids => (OP_RESET_BIDS, OP_BID),
            BookSide::Asks => (OP_RESET_ASKS, OP_ASK),
        };
        let (target, ops) = match side {
            BookSide::Bids => (&mut self.bids, &mut self.ops),
            BookSide::Asks => (&mut self.asks, &mut self.ops),
        };
        target.clear();
        target.extend(levels.into_iter().filter(|(_, amount)| *amount != 0.0));
        ops.extend([reset, 0.0, 0.0]);
        for &(price, amount) in target.iter() {
            ops.extend([code, price, amount]);
        }
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
            (Ok(index), false) if levels[index].1 == amount => return,
            (Ok(index), false) => levels[index].1 = amount,
            (Err(_), true) => return,
            (Err(index), false) => levels.insert(index, (price, amount)),
        }
        let code = match side {
            BookSide::Bids => OP_BID,
            BookSide::Asks => OP_ASK,
        };
        self.ops.extend([code, price, amount]);
    }

    /// Append the ops logged since the previous call to `out`.
    pub fn take_ops(&mut self, out: &mut Vec<f64>) {
        out.append(&mut self.ops);
    }

    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|(price, _)| *price)
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|(price, _)| *price)
    }

    pub fn to_model(&self) -> OrderBook {
        OrderBook {
            bids: with_totals(&self.bids),
            asks: with_totals(&self.asks),
        }
    }

    fn side_mut(&mut self, side: BookSide) -> &mut Vec<(f64, f64)> {
        match side {
            BookSide::Bids => &mut self.bids,
            BookSide::Asks => &mut self.asks,
        }
    }
}

fn with_totals(levels: &[(f64, f64)]) -> Vec<BookLevel> {
    let mut total = 0.0;
    levels
        .iter()
        .map(|&(price, amount)| {
            total += amount;
            BookLevel {
                price,
                amount,
                total,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let model = book.to_model();
        let bids: Vec<_> = model
            .bids
            .iter()
            .map(|l| (l.price, l.amount, l.total))
            .collect();
        let asks: Vec<_> = model
            .asks
            .iter()
            .map(|l| (l.price, l.amount, l.total))
            .collect();
        assert_eq!(bids, [(12.0, 2.0, 2.0), (10.0, 1.0, 3.0)]);
        assert_eq!(asks, [(20.0, 1.0, 1.0), (21.0, 3.0, 4.0), (22.0, 5.0, 9.0)]);
        assert_eq!((book.best_bid(), book.best_ask()), (Some(12.0), Some(20.0)));
    }

    #[test]
    fn logs_only_effective_changes() {
        let mut book = Book::default();
        book.load_sorted(BookSide::Bids, [(10.0, 1.0)]);
        book.apply(BookSide::Bids, 10.0, 1.0);
        book.apply(BookSide::Asks, 11.0, 0.0);
        book.apply(BookSide::Asks, 12.0, 2.0);
        let mut ops = Vec::new();
        book.take_ops(&mut ops);
        assert_eq!(
            ops,
            [
                OP_RESET_BIDS,
                0.0,
                0.0,
                OP_BID,
                10.0,
                1.0,
                OP_ASK,
                12.0,
                2.0
            ]
        );
        book.take_ops(&mut ops);
        assert_eq!(ops.len(), 9, "log drained");
    }
}
