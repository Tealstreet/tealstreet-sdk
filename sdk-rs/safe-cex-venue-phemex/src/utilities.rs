//! Pure Phemex instance-method bodies with explicit inputs.
use safe_cex_types::PositionSide;

pub fn calculate_price_diff(price: f64, mark_price: f64, side: PositionSide) -> f64 {
    if side == PositionSide::Long {
        safe_cex_num::subtract(mark_price, price)
    } else {
        safe_cex_num::subtract(price, mark_price)
    }
}

/// Shared UTF-8 string boundary; absent input follows the source empty fallback.
pub fn format_symbol_in(symbol: Option<&str>) -> String {
    symbol.unwrap_or("").to_uppercase()
}

/// The first matching market wins even when its info symbol is falsy.
/// JSON-domain info is returned without converting its type or numeric lexeme.
pub fn format_symbol_out(
    markets: Option<&[safe_cex_types::Market]>,
    symbol: Option<&str>,
) -> serde_json::Value {
    let found = markets
        .and_then(|markets| {
            markets
                .iter()
                .find(|market| Some(market.symbol.as_str()) == symbol)
        })
        .and_then(|market| market.info.as_ref())
        .and_then(|info| info.get("symbol"));
    if safe_cex_num::js_truthy(found) {
        found.expect("truthy value exists").clone()
    } else {
        serde_json::Value::String(symbol.unwrap_or("").to_owned())
    }
}
