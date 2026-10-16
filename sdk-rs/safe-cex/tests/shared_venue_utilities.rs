use safe_cex::{calc_order_book_total, create_position_id, normalize_execution_position_side};
use safe_cex_types::{OrderBook, OrderBookOrders, PositionSide};
#[test]
fn venue_utilities_are_callable_and_accumulation_preserves_order() {
    let mut book = OrderBook {
        bids: vec![
            OrderBookOrders {
                price: 2.0,
                amount: 3.0,
                total: 99.0,
            },
            OrderBookOrders {
                price: 7.0,
                amount: 4.0,
                total: 99.0,
            },
        ],
        asks: vec![
            OrderBookOrders {
                price: 9.0,
                amount: 5.0,
                total: 99.0,
            },
            OrderBookOrders {
                price: 1.0,
                amount: 6.0,
                total: 99.0,
            },
        ],
    };
    calc_order_book_total(&mut book);
    assert_eq!(
        book.bids
            .iter()
            .map(|x| (x.price, x.total))
            .collect::<Vec<_>>(),
        [(2.0, 3.0), (7.0, 7.0)]
    );
    assert_eq!(
        book.asks
            .iter()
            .map(|x| (x.price, x.total))
            .collect::<Vec<_>>(),
        [(9.0, 5.0), (1.0, 11.0)]
    );
    assert_eq!(
        create_position_id("BTCUSDT", Some(PositionSide::Long), None),
        "BTCUSDT:long"
    );
    assert_eq!(
        normalize_execution_position_side(Some(&serde_json::json!(" Close Long "))).unwrap(),
        "long"
    );
}

#[test]
fn wire_object_coercion_propagates_type_error_through_facade() {
    let bad = serde_json::json!({"toString":"long"});
    let error = normalize_execution_position_side(Some(&bad)).unwrap_err();
    assert_eq!(error.name, "TypeError");
    assert_eq!(error.message, "Cannot convert object to primitive value");
}
