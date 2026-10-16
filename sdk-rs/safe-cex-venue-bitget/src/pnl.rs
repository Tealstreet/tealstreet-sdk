//! PnL decisions over the shared typed Position/Ticker numeric fields.
use safe_cex_types::{Position, PositionSide};

// ts: BitgetExchange.calculateUpnlFromLastPrice
pub fn calculate_upnl_from_last_price(
    positions: &[Position],
    mut ticker_last: impl FnMut(&str) -> Option<f64>,
) -> f64 {
    // These declared domain fields are numbers. This is the numeric branch of
    // toFiniteNumber, not a substitute for its pending unknown/string parser.
    let finite = |value: f64| if value.is_finite() { value } else { 0.0 };
    positions.iter().fold(0.0, |acc, position| {
        let ticker = ticker_last(&position.symbol);
        let contracts = finite(position.contracts).abs();
        let entry_price = finite(position.entry_price);
        let api_unrealized_pnl = finite(position.unrealized_pnl);
        if contracts == 0.0 {
            return acc;
        }
        let last_price = ticker.map(finite).unwrap_or(0.0);
        if last_price == 0.0 {
            return acc + api_unrealized_pnl;
        }
        // PositionSide's declared domain is exactly long or short.
        let direction = if position.side == PositionSide::Long {
            1.0
        } else {
            -1.0
        };
        let recalculated = direction * (last_price - entry_price) * contracts;
        acc + if recalculated.is_finite() {
            recalculated
        } else {
            api_unrealized_pnl
        }
    })
}

// ts: BitgetExchange.calculateAccountUpnl
pub fn calculate_account_upnl(
    api_upnl: f64,
    positions: &[Position],
    ticker_last: impl FnMut(&str) -> Option<f64>,
) -> f64 {
    let normalized_api_upnl = if api_upnl.is_finite() { api_upnl } else { 0.0 };
    let has_open_positions = positions
        .iter()
        .any(|position| position.contracts.is_finite() && position.contracts.abs() > 0.0);
    if !has_open_positions {
        return normalized_api_upnl;
    }
    calculate_upnl_from_last_price(positions, ticker_last)
}
