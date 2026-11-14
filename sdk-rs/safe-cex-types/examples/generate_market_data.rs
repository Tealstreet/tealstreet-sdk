// Compile the slice before the integrator installs its root re-exports.
pub use safe_cex_types::{OrderSide, strict_decode};
#[path = "../src/market_data.rs"]
mod market_data;

use market_data::{Candle, OrderBook, OrderBookOrders};
use ts_rs::{Config, TS};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::new();
    println!(
        "{}",
        serde_json::to_string(&[
            ("Candle", Candle::export_to_string(&config)?),
            ("OrderBook", OrderBook::export_to_string(&config)?),
            (
                "OrderBookOrders",
                OrderBookOrders::export_to_string(&config)?
            ),
        ])?
    );
    Ok(())
}
