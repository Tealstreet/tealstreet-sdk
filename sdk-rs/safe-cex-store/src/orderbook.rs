//! Independent sorting and indexed accumulation from utils/orderbook.ts.
use safe_cex_types::OrderBook;
pub fn calc_order_book_total(book: &mut OrderBook) {
    let mut bid_total = 0.0;
    for i in 0..book.bids.len() {
        bid_total += book.bids[i].amount;
        book.bids[i].total = bid_total;
    }
    let mut ask_total = 0.0;
    for i in 0..book.asks.len() {
        ask_total += book.asks[i].amount;
        book.asks[i].total = ask_total;
    }
}

/// Stable source subtraction comparator, independent of total accumulation.
pub fn sort_order_book(book: &mut OrderBook) {
    for (levels, descending) in [(&mut book.asks, false), (&mut book.bids, true)] {
        let result = safe_cex_num::sorting::stable_sort::<_, std::convert::Infallible>(
            levels,
            |left, right| {
                let difference = if descending {
                    right.price - left.price
                } else {
                    left.price - right.price
                };
                Ok(difference
                    .partial_cmp(&0.0)
                    .unwrap_or(std::cmp::Ordering::Equal))
            },
        );
        match result {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }
}
