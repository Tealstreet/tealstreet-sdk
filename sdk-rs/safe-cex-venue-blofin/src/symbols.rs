//! Source store-backed symbol/id projections, including inherited lookup hits.
use safe_cex_num::{TableHit, js_table_lookup};
use safe_cex_store::Store;

pub fn symbol_to_market_id(
    symbol: &str,
    store: &dyn Store,
) -> Result<Option<String>, safe_cex_types::StoreReadError> {
    let markets = store.markets_by_symbol()?;
    if let Some(market) = markets.get(symbol) {
        return Ok(Some(market.id.clone()));
    }
    // Stock inherited functions/prototype are truthy, but have no .id property.
    Ok(match js_table_lookup::<&str, ()>(&[], symbol) {
        TableHit::Inherited(_) => None,
        _ => Some(symbol.into()),
    })
}
pub fn market_id_to_symbol(
    market_id: &str,
    store: &dyn Store,
) -> Result<Option<String>, safe_cex_types::StoreReadError> {
    let markets = store.markets_by_id()?;
    if let Some(market) = markets.get(market_id) {
        return Ok(Some(market.symbol.clone()));
    }
    Ok(match js_table_lookup::<&str, ()>(&[], market_id) {
        TableHit::Inherited(_) => None,
        _ => Some(market_id.into()),
    })
}
