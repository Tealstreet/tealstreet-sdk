//! Extracted market helpers; the caller supplies the Store's market indexes.
use safe_cex_num::{TableHit, js_table_lookup};
use safe_cex_types::Market;

pub fn to_bingx_symbol(
    symbol: &str,
    markets_by_symbol: &[(String, Market)],
    markets_by_id: &[(String, Market)],
) -> String {
    let market = match js_table_lookup(markets_by_symbol, symbol) {
        TableHit::Missing => js_table_lookup(markets_by_id, symbol),
        hit => hit,
    };
    if let TableHit::Mapped(market) = market
        && !market.id.is_empty()
    {
        return market.id.clone();
    }
    if symbol.contains('-') {
        return symbol.to_owned();
    }
    if symbol.ends_with("USDT") || symbol.ends_with("USDC") {
        let (base, quote) = symbol.split_at(symbol.len() - 4);
        return format!("{base}-{quote}");
    }
    symbol.to_owned()
}

pub fn is_likely_tradfi_market(market: &Market) -> bool {
    let first = market.id.split('-').next().unwrap_or_default();
    let contract_symbol = if first.is_empty() {
        &market.base
    } else {
        first
    };
    // Source regexps lack /u: ASCII case folding, no whitespace trimming.
    let folded = contract_symbol.to_ascii_lowercase();
    folded.ends_with("usd") && !(folded.ends_with("usdt") || folded.ends_with("usdc"))
}
