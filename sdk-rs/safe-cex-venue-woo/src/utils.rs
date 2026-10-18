//! WOO symbol spelling and transaction-side normalization.
use safe_cex_types::{OrderSide, PositionSide};

// ts: normalizeSymbol
pub fn normalize_symbol(value: &str) -> String {
    String::from_utf16(&normalize_symbol_utf16(
        &value.encode_utf16().collect::<Vec<_>>(),
    ))
    .expect("normalizing a Unicode scalar string preserves scalar text")
}

pub(crate) fn normalize_symbol_utf16(value: &[u16]) -> Vec<u16> {
    let mut upper = Vec::new();
    for scalar in char::decode_utf16(value.iter().copied()) {
        match scalar {
            Ok(scalar) => {
                for mapped in scalar.to_uppercase() {
                    upper.extend_from_slice(mapped.encode_utf16(&mut [0; 2]));
                }
            }
            Err(surrogate) => upper.push(surrogate.unpaired_surrogate()),
        }
    }
    let normalized = upper
        .rsplit(|unit| *unit == u16::from(b':'))
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(&upper);
    let equals = |units: &[u16], text: &str| units.iter().copied().eq(text.encode_utf16());
    for separator in *b"_-" {
        let parts: Vec<_> = normalized
            .split(|unit| *unit == u16::from(separator))
            .collect();
        if parts.len() >= 3 {
            if equals(parts[0], "PERP") || equals(parts[0], "SPOT") {
                return [parts[1], parts[2]].concat();
            }
            if equals(parts[parts.len() - 1], "PERP") {
                return [parts[0], parts[1]].concat();
            }
        }
    }
    if !normalized
        .iter()
        .any(|unit| *unit == u16::from(b'_') || *unit == u16::from(b'-'))
        && normalized.ends_with(&"PERP".encode_utf16().collect::<Vec<_>>())
    {
        let mut result = normalized[..normalized.len() - 4].to_vec();
        result.extend("USDT".encode_utf16());
        return result;
    }
    normalized
        .iter()
        .copied()
        .filter(|unit| {
            !b"_-./:"
                .iter()
                .any(|separator| *unit == u16::from(*separator))
        })
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
