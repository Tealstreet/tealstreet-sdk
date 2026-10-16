//! WOO symbol spelling and transaction-side normalization.
use safe_cex_types::{OrderSide, PositionSide};

// ts: normalizeSymbol
pub fn normalize_symbol(value: &str) -> String {
    let upper = value.to_uppercase();
    let normalized = upper
        .rsplit(':')
        .next()
        .filter(|v| !v.is_empty())
        .unwrap_or(&upper);
    for separator in ['_', '-'] {
        let parts: Vec<_> = normalized.split(separator).collect();
        if parts.len() >= 3 {
            if matches!(parts[0], "PERP" | "SPOT") {
                return format!("{}{}", parts[1], parts[2]);
            }
            if parts.last() == Some(&"PERP") {
                return format!("{}{}", parts[0], parts[1]);
            }
        }
    }
    if !normalized.contains(['_', '-'])
        && let Some(base) = normalized.strip_suffix("PERP")
    {
        return format!("{base}USDT");
    }
    normalized
        .chars()
        .filter(|c| !matches!(c, '_' | '-' | '.' | '/' | ':'))
        .collect()
}

// ts: reverseSymbol
pub fn reverse_symbol(value: &str) -> String {
    let upper = value.to_uppercase();
    let mut normalized = upper
        .rsplit(':')
        .next()
        .filter(|v| !v.is_empty())
        .unwrap_or(&upper);
    for suffix in [".P", "_PERP", "-PERP", "PERP", "USDT"] {
        normalized = normalized.strip_suffix(suffix).unwrap_or(normalized);
    }
    let asset: String = normalized
        .chars()
        .filter(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        .collect();
    format!("PERP_{asset}_USDT")
}

// ts: mapWooHedgeTransactionSide
pub fn map_woo_hedge_transaction_side(value: &str) -> Option<(OrderSide, PositionSide)> {
    let upper = value.to_uppercase().replace('_', " ");
    let normalized = safe_cex_num::js_split_whitespace(&upper)
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    match normalized.as_str() {
        "OPEN LONG" => Some((OrderSide::Buy, PositionSide::Long)),
        "CLOSE LONG" => Some((OrderSide::Sell, PositionSide::Long)),
        "OPEN SHORT" => Some((OrderSide::Sell, PositionSide::Short)),
        "CLOSE SHORT" => Some((OrderSide::Buy, PositionSide::Short)),
        _ => None,
    }
}
