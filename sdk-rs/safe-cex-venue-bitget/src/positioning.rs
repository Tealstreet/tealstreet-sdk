//! Positioning field consumers; cache resolution and exchange assembly are separate.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{MarginType, PositionMode};
use std::future::Future;

pub async fn get_or_fetch_position_mode<F>(
    symbol: &str,
    resolve: impl FnOnce(&str) -> F,
    uses_uta: impl FnOnce() -> bool,
    report: impl FnOnce(&str) -> Result<(), ExchangeError>,
) -> Result<PositionMode, ExchangeError>
where
    F: Future<Output = Result<Option<PositionMode>, ExchangeError>>,
{
    if let Some(mode) = resolve(symbol).await? {
        return Ok(mode);
    }
    let prefix = if uses_uta() { "Bitget UTA " } else { "" };
    let message = format!("Could not determine {prefix}position mode for symbol {symbol}");
    report(&message)?;
    Err(ExchangeError::new(message))
}

pub async fn get_or_fetch_margin_type<F>(
    symbol: &str,
    resolve: impl FnOnce(&str) -> F,
    uses_uta: impl FnOnce() -> bool,
    report: impl FnOnce(&str) -> Result<(), ExchangeError>,
) -> Result<MarginType, ExchangeError>
where
    F: Future<Output = Result<Option<MarginType>, ExchangeError>>,
{
    if let Some(margin) = resolve(symbol).await? {
        return Ok(margin);
    }
    let prefix = if uses_uta() { "Bitget UTA " } else { "" };
    let message = format!("Could not determine {prefix}margin type for symbol {symbol}");
    report(&message)?;
    Err(ExchangeError::new(message))
}
