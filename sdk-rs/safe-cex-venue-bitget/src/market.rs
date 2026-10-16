//! Pure market resolution over typed shared Market records.
use safe_cex_types::Market;

// ts: BitgetExchange._resolveMarket
pub fn resolve_market<'a>(
    symbol: &str,
    margin_coin: &str,
    markets: &'a [Market],
    mut market_by_symbol: impl FnMut(&str) -> Option<&'a Market>,
) -> Option<&'a Market> {
    // ts-bug: _resolveMarket's plain-object index can return inherited functions
    // or objects. CORRECTED at the chosen typed Market lookup boundary; see manifest.
    if let Some(market) = market_by_symbol(symbol) {
        return Some(market);
    }
    let normalized = symbol.to_uppercase().replace(['-', '_'], "");
    let normalized = normalized.strip_suffix("PERP").unwrap_or(&normalized);
    let normalized = normalized.strip_suffix("SWAP").unwrap_or(normalized);
    if let Some(market) = market_by_symbol(normalized) {
        return Some(market);
    }
    if !normalized.ends_with("USDT")
        && !normalized.ends_with("USD")
        && let Some(market) = market_by_symbol(&format!("{normalized}USDT"))
    {
        return Some(market);
    }
    let base = normalized.strip_suffix("USDT").unwrap_or(normalized);
    // ts-bug: _resolveMarket removes USD before BUSD, so BTCBUSD becomes BTCB.
    // Preserve sequential replacements, including PERP before SWAP above.
    let base = base.strip_suffix("USD").unwrap_or(base);
    let base = base.strip_suffix("BUSD").unwrap_or(base);
    markets
        .iter()
        .find(|market| market.base == base && market.quote == margin_coin)
}

// ts: BitgetInverseExchange.getMarginCoinForSymbol
pub fn get_margin_coin_for_symbol<'a>(
    symbol: &str,
    mut market_by_symbol: impl FnMut(&str) -> Option<&'a Market>,
) -> String {
    // ts-bug: getMarginCoinForSymbol reads .base on inherited plain-object values;
    // its declared string result can actually be undefined. Typed lookup excludes
    // that source bug (CORRECTED); explicit Market entries with those keys work.
    if let Some(market) = market_by_symbol(symbol) {
        return market.base.clone();
    }
    symbol.strip_suffix("USD").unwrap_or(symbol).to_owned()
}
