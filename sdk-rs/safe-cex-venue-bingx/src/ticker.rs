//! Synthetic bid/ask from BingX last price and its price precision.
use safe_cex_num::{add, math_max, subtract};
use safe_cex_types::MarketPrecision;

/// Return (bid, ask), retaining the source numeric operations and their order.
pub fn derive_ticker_bid_ask(precision: Option<&MarketPrecision>, last: f64) -> (f64, f64) {
    if !last.is_finite() || last <= 0.0 {
        return (0.0, 0.0);
    }
    let price_step = precision
        .filter(|p| p.price.is_finite() && p.price > 0.0)
        .map_or(0.0, |p| p.price);
    (
        math_max(subtract(last, price_step), 0.0),
        add(last, price_step),
    )
}
