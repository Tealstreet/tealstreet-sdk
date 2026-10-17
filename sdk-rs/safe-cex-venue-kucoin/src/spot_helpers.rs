//! Store-only Spot position/leverage behavior; no transport or timers.
use safe_cex_store::{Store, StoreMutationError, StoreUpdate};
use safe_cex_types::{FetchedLeverage, MarginType, Position, PositionMode, PositionSide};
use std::collections::HashSet;
// ts: KucoinSpotExchange.fetchPositions
pub fn fetch_positions(store: &dyn Store, symbols: Option<&[String]>) -> Vec<Position> {
    let selected =
        symbols.map(|symbols| symbols.iter().map(String::as_str).collect::<HashSet<_>>());
    store
        .markets()
        .iter()
        .filter(|market| {
            market.active
                && selected
                    .as_ref()
                    .is_none_or(|set| set.contains(market.symbol.as_str()))
        })
        .map(|market| Position {
            id: format!("{}:long", market.symbol),
            symbol: market.symbol.clone(),
            side: safe_cex_types::ModelField::Value(PositionSide::Long),
            entry_price: 0.0,
            notional: 0.0,
            leverage: 1.0,
            unrealized_pnl: 0.0,
            contracts: 0.0,
            liquidation_price: 0.0,
            margin_type: (Some(MarginType::Isolated)).into(),
            position_mode: (Some(PositionMode::OneWay)).into(),
            info: (Some(serde_json::json!({"spot":true}))).into(),
            realized_pnl: (None).into(),
            margin_size: (None).into(),
            extra: (None).into(),
            last_update: (None).into(),
            option: (None).into(),
        })
        .collect()
}
// ts: KucoinSpotExchange.fetchSymbolMaxLeverage
pub fn fetch_symbol_max_leverage(_symbol: &str) -> f64 {
    1.0
}
// ts: KucoinSpotExchange.setLeverage
pub fn set_leverage(
    store: &dyn Store,
    symbol: &str,
    _leverage: f64,
    _side: Option<PositionSide>,
) -> Result<(), StoreMutationError> {
    let mut fetched = store.fetched_leverage().clone();
    fetched.insert(
        symbol.to_owned(),
        FetchedLeverage {
            buy_leverage: 1.0,
            sell_leverage: 1.0,
            margin_type: None,
            position_mode: None,
            is_fetching: None,
        },
    );
    store.update(
        StoreUpdate {
            fetched_leverage: Some(fetched),
            ..Default::default()
        },
        false,
    )
}
// ts: KucoinSpotExchange.changeMarginType
pub fn change_margin_type(_isolated: bool, _symbol: Option<&str>) {}
// ts: KucoinSpotExchange.changePositionMode
pub fn change_position_mode(_hedged: bool, _symbol: Option<&str>) {}
