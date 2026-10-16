//! Fixed spot/prediction-market method bodies from PolymarketExchange.
//! Stateful construction, trading and lifecycle remain separate pending rows.
use safe_cex_types::{ExchangeName, PositionMode};
use std::future::{Ready, ready};

/// Source `PolymarketExchange.get name` has no receiver state.
pub fn name() -> ExchangeName {
    ExchangeName::Polymarket
}

/// Source `PolymarketExchange.getPositionMode`.
pub async fn get_position_mode() -> PositionMode {
    PositionMode::OneWay
}

/// Source `PolymarketExchange.fetchSymbolMaxLeverage`; the symbol is ignored.
pub async fn fetch_symbol_max_leverage(_symbol: &str) -> f64 {
    1.0
}

/// Source `PolymarketExchange.setLeverage`; preserve the warning and its failure.
pub fn set_leverage<E>(
    log: &mut impl FnMut(&str, &str) -> Result<(), E>,
    _symbol: &str,
    _leverage: f64,
) -> Ready<Result<(), E>> {
    // JS async methods execute this synchronous prefix at call time.
    ready(log(
        "Leverage is not supported on Polymarket (always 1x)",
        "warning",
    ))
}

/// Source `PolymarketExchange.changePositionMode`; ignores the requested mode.
pub fn change_position_mode<E>(
    log: &mut impl FnMut(&str, &str) -> Result<(), E>,
    _hedged: bool,
) -> Ready<Result<(), E>> {
    // JS async methods execute this synchronous prefix at call time.
    ready(log(
        "Position mode changes not supported on Polymarket",
        "warning",
    ))
}

/// Source `PolymarketExchange.changeMarginType`; ignores the requested margin mode.
pub fn change_margin_type<E>(
    log: &mut impl FnMut(&str, &str) -> Result<(), E>,
    _isolated: bool,
) -> Ready<Result<(), E>> {
    // JS async methods execute this synchronous prefix at call time.
    ready(log(
        "Margin type changes not supported on Polymarket",
        "warning",
    ))
}
