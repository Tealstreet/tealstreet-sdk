pub use safe_cex_types::{OrderSide, strict_decode};
#[path = "../src/market_data.rs"]
mod market_data;

use market_data::{Candle, OrderBook, OrderBookOrders};
use serde_json::{Value, json};

#[test]
fn candles_keep_milliseconds_zeroes_and_optional_enum_identity() {
    let value = json!({"timestamp": 1700000040000_f64, "open": 0.0, "high": 2.0,
        "low": 0.0, "close": 1.0, "volume": 0.0});
    let mut candle: Candle = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&candle).unwrap(), value);
    candle.last_side = Some(OrderSide::Sell);
    assert_eq!(serde_json::to_value(&candle).unwrap()["lastSide"], "sell");
    for invalid in [Value::Null, json!("unknown"), json!(0)] {
        let mut row = value.clone();
        row["lastSide"] = invalid;
        assert!(serde_json::from_value::<Candle>(row).is_err());
    }
    assert!(serde_json::from_value::<Candle>(json!([1, 2, 3, 4, 5, 6])).is_err());
}

#[test]
fn mutable_books_keep_zero_prices_and_require_object_levels() {
    let value = json!({"bids": [{"price": 0.0, "amount": 2.0, "total": 2.0}], "asks": []});
    let mut book: OrderBook = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&book).unwrap(), value);
    book.bids[0].amount = 0.0;
    book.asks.push(OrderBookOrders {
        price: 1.0,
        amount: 3.0,
        total: 3.0,
    });
    assert_eq!(book.bids[0].price, 0.0);
    for invalid in [
        Value::Null,
        json!([]),
        json!({"bids": null, "asks": []}),
        json!({"bids": [[0, 2, 2]], "asks": []}),
        json!({"bids": [{"price": 0, "amount": 2}], "asks": []}),
    ] {
        assert!(serde_json::from_value::<OrderBook>(invalid).is_err());
    }
}
