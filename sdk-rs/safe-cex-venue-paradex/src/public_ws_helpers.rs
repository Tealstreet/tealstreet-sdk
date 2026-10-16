//! Public websocket channel naming, independent of subscription lifecycle.

// ts: getOrderBookChannel
pub fn get_order_book_channel(symbol: &str) -> String {
    format!("order_book.{symbol}.snapshot@15@100ms")
}
