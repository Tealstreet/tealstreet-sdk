//! Pure Kraken public-websocket helper bodies; socket lifecycle remains separate.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{TableHit, js_table_lookup};
use safe_cex_store::Store;

pub fn get_ticker_product_ids(
    store: &dyn Store,
    get_active_symbols: impl FnOnce() -> Result<Vec<String>, ExchangeError>,
) -> Result<Vec<String>, ExchangeError> {
    let product_ids: Vec<String> = store.markets()?.iter().map(|m| m.symbol.clone()).collect();
    let active_symbols = get_active_symbols()?;
    if active_symbols.is_empty() {
        return Ok(product_ids);
    }
    let filtered: Vec<String> = product_ids
        .iter()
        .filter(|symbol| active_symbols.contains(symbol))
        .cloned()
        .collect();
    if filtered.is_empty() {
        Ok(product_ids)
    } else {
        Ok(filtered)
    }
}

pub fn map_timeframe(timeframe: &[u16]) -> TableHit<f64> {
    let table = [
        ("1m", 1.0),
        ("3m", 3.0),
        ("5m", 5.0),
        ("15m", 15.0),
        ("30m", 30.0),
        ("1h", 60.0),
        ("2h", 120.0),
        ("4h", 240.0),
        ("6h", 360.0),
        ("12h", 720.0),
        ("1d", 1440.0),
        ("1w", 10080.0),
    ];
    match String::from_utf16(timeframe)
        .ok()
        .map(|key| js_table_lookup(&table, &key))
    {
        Some(TableHit::Mapped(value)) => TableHit::Mapped(*value),
        Some(TableHit::Inherited(member)) => TableHit::Inherited(member),
        _ => TableHit::Mapped(1.0),
    }
}
