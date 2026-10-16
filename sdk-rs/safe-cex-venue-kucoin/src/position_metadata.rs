//! Futures position metadata through the shared position-fetch operation.
use safe_cex_exchange::{exchange::Exchange, runtime::ExchangeError};
use safe_cex_types::{MarginType, PositionMode};
use std::rc::Rc;

// ts: KucoinExchange.getOrFetchPositionMode
pub async fn get_or_fetch_position_mode(
    venue: Rc<dyn Exchange>,
    symbol: &str,
) -> Result<PositionMode, ExchangeError> {
    let positions = venue
        .base()
        .get_or_fetch_positions_for_symbol(venue.clone(), symbol)
        .await?;
    let first = positions.first().ok_or_else(|| {
        ExchangeError::new(format!(
            "No positions found for symbol {symbol} needed to determine position mode"
        ))
    })?;
    first.position_mode.ok_or_else(|| {
        ExchangeError::new(format!(
            "Position missing position mode for symbol {symbol}"
        ))
    })
}

// ts: KucoinExchange.getOrFetchMarginType
pub async fn get_or_fetch_margin_type(
    venue: Rc<dyn Exchange>,
    symbol: &str,
) -> Result<MarginType, ExchangeError> {
    let positions = venue
        .base()
        .get_or_fetch_positions_for_symbol(venue.clone(), symbol)
        .await?;
    let first = positions.first().ok_or_else(|| {
        ExchangeError::new(format!(
            "No positions found for symbol {symbol} needed to determine margin type"
        ))
    })?;
    first.margin_type.ok_or_else(|| {
        ExchangeError::new(format!("Position missing margin type for symbol {symbol}"))
    })
}
