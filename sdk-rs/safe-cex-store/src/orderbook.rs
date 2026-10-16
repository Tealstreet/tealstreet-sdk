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
    fn sorted(levels: &mut [safe_cex_types::OrderBookOrders], descending: bool) {
        if levels.len() < 2 {
            return;
        }
        let mid = levels.len() / 2;
        let (left, right) = levels.split_at_mut(mid);
        sorted(left, descending);
        sorted(right, descending);
        let left = left.to_vec();
        let right = right.to_vec();
        let (mut l, mut r) = (0, 0);
        for dest in levels {
            // ts-bug: sortOrderBook — NaN subtraction compares as equality,
            // so malformed NaN-price books have a nontransitive comparator.
            let take_right = r < right.len()
                && (l == left.len()
                    || if descending {
                        left[l].price - right[r].price < 0.0
                    } else {
                        right[r].price - left[l].price < 0.0
                    });
            if take_right {
                *dest = right[r].clone();
                r += 1;
            } else {
                *dest = left[l].clone();
                l += 1;
            }
        }
    }
    sorted(&mut book.asks, false);
    sorted(&mut book.bids, true);
}
