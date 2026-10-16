//! Sorted in-place book updates. `bids` represents the TS bids/asks literal choice.
use safe_cex_types::{OrderBook, OrderBookOrders};

fn binary_search_price(levels: &[OrderBookOrders], price: f64, descending: bool) -> isize {
    let mut left = 0_isize;
    let mut right = levels.len() as isize - 1;
    while left <= right {
        let mid = (((left + right) as u32) >> 1) as isize;
        let mid_price = levels[mid as usize].price;
        if mid_price == price {
            return mid;
        }
        if if descending {
            mid_price > price
        } else {
            mid_price < price
        } {
            left = mid + 1;
        } else {
            right = mid - 1;
        }
    }
    -(left + 1)
}

/// Object form preserves the supplied total only on insertion.
pub fn update_order_book(
    book: &mut OrderBook,
    bids: bool,
    update: OrderBookOrders,
    upsert: bool,
) -> isize {
    let levels = if bids { &mut book.bids } else { &mut book.asks };
    let index = binary_search_price(levels, update.price, bids);
    if index < 0 {
        if !upsert || update.amount == 0.0 {
            return -1;
        }
        let insert_at = -(index + 1) as usize;
        levels.insert(insert_at, update);
        insert_at as isize
    } else if update.amount == 0.0 {
        levels.remove(index as usize);
        -1
    } else {
        levels[index as usize].amount = update.amount;
        index
    }
}

/// Primitive form constructs a level only on insertion; updates retain total.
pub fn update_order_book_by_price(
    book: &mut OrderBook,
    bids: bool,
    price: f64,
    amount: f64,
    upsert: bool,
) -> isize {
    let levels = if bids { &mut book.bids } else { &mut book.asks };
    let index = binary_search_price(levels, price, bids);
    if index < 0 {
        if !upsert || amount == 0.0 {
            return -1;
        }
        let insert_at = -(index + 1) as usize;
        levels.insert(
            insert_at,
            OrderBookOrders {
                price,
                amount,
                total: 0.0,
            },
        );
        insert_at as isize
    } else if amount == 0.0 {
        levels.remove(index as usize);
        -1
    } else {
        levels[index as usize].amount = amount;
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_typescript_sequence_oracle() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../tests/order_book_oracle.json")).unwrap();
        for case in cases {
            let mut book: OrderBook = serde_json::from_value(case["initial"].clone()).unwrap();
            for step in case["steps"].as_array().unwrap() {
                let bids = step["side"] == "bids";
                let upsert = step["upsert"].as_bool().unwrap();
                let result = if step["object"].as_bool().unwrap() {
                    update_order_book(
                        &mut book,
                        bids,
                        serde_json::from_value(step["update"].clone()).unwrap(),
                        upsert,
                    )
                } else {
                    update_order_book_by_price(
                        &mut book,
                        bids,
                        step["update"]["price"].as_f64().unwrap(),
                        step["update"]["amount"].as_f64().unwrap(),
                        upsert,
                    )
                };
                assert_eq!(result as i64, step["index"].as_i64().unwrap());
                crate::test_support::assert_json_eq(
                    &serde_json::to_value(&book).unwrap(),
                    &step["book"],
                );
            }
        }
    }
    #[test]
    fn ieee_comparison_and_zero_behavior() {
        let mut book = OrderBook {
            bids: vec![],
            asks: vec![],
        };
        assert_eq!(
            update_order_book_by_price(&mut book, true, f64::INFINITY, 1.0, true),
            0
        );
        assert_eq!(
            update_order_book_by_price(&mut book, true, 10.0, 1.0, true),
            1
        );
        assert_eq!(
            update_order_book_by_price(&mut book, true, f64::NAN, 1.0, true),
            0
        );
        assert!(book.bids[0].price.is_nan());
        assert_eq!(
            update_order_book_by_price(&mut book, false, 10.0, -0.0, true),
            -1
        );
        assert_eq!(
            update_order_book_by_price(&mut book, false, 10.0, f64::NAN, true),
            0
        );
        assert!(book.asks[0].amount.is_nan());
    }
}
