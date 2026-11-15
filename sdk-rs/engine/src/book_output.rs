//! Native book output. Full totals and delta deletes retain the format-1 layout.
use crate::{
    book::Book,
    wire::{FrameWriter, section},
};

pub fn write_delta(book: &Book, previous: &Book, market: u32, w: &mut FrameWriter<'_>) -> bool {
    let mut bids = 0u32;
    let mut asks = 0u32;
    differences(previous.bids(), book.bids(), true, |_, _| bids += 1);
    differences(previous.asks(), book.asks(), false, |_, _| asks += 1);
    if bids == 0 && asks == 0 {
        return false;
    }
    w.begin(section::BOOK_DELTA);
    w.u32s([market, bids, asks, 0]);
    differences(previous.bids(), book.bids(), true, |price, amount| {
        w.f64s([price, amount]);
    });
    differences(previous.asks(), book.asks(), false, |price, amount| {
        w.f64s([price, amount]);
    });
    w.end();
    true
}

/// Write the full book as a `BOOK` section: rows of `[price, amount, total]`.
pub fn write(book: &Book, market: u32, w: &mut FrameWriter<'_>) {
    w.begin(section::BOOK);
    w.u32s([
        market,
        book.bids().len() as u32,
        book.asks().len() as u32,
        0,
    ]);
    for levels in [book.bids(), book.asks()] {
        let mut total = 0.0;
        for &(price, amount) in levels.iter() {
            total += amount;
            w.f64s([price, amount, total]);
        }
    }
    w.end();
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
