//! Live typed symbol and margin-coin helpers; no transport or scheduling ownership.
use safe_cex_store::Store;

// ts: KucoinExchange.toApiSymbol
pub fn to_api_symbol(
    symbol: Option<&str>,
    store: &dyn Store,
) -> Result<String, safe_cex_types::StoreReadError> {
    let Some(symbol) = symbol.filter(|s| !s.is_empty()) else {
        return Ok(String::new());
    };
    Ok(store
        .markets_by_symbol()?
        .get(symbol)
        .map(|m| m.id.as_str())
        .filter(|id| !id.is_empty())
        .unwrap_or(symbol)
        .to_owned())
}
// ts: KucoinExchange.apiMarginCoin
// ts-bug: KucoinExchange.apiMarginCoin retains the legacy SUSDT testnet spelling.
pub fn api_margin_coin(testnet: bool) -> &'static str {
    if testnet { "SUSDT" } else { "USDT" }
}
