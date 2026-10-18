//! GRVT market metadata validation before order signing.

use safe_cex_types::retained_json::RetainedJsonInput as Input;

/// The source returns decimal exponents unchanged, including negative zero.
pub fn get_instrument_signing_metadata<'a>(
    market_id: &str,
    info: Option<&'a Input>,
) -> Result<(String, f64, f64), String> {
    let hash = info
        .and_then(|value| value.get("ih"))
        .and_then(Input::as_text)
        .and_then(|units| String::from_utf16(units).ok());
    let decimals = |key| {
        info.and_then(|value| value.get(key))
            .and_then(Input::as_f64)
            .filter(|value| {
                value.is_finite() && value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_991.0
            })
    };
    if let (Some(hash), Some(base), Some(quote)) = (hash, decimals("bd"), decimals("qd"))
        && hash.starts_with("0x")
        && hash.len() > 2
        && hash.len() <= 66
        && hash[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Ok((hash, base, quote));
    }
    Err(format!(
        "Market {market_id} does not have valid GRVT signing metadata (ih/bd/qd); refresh markets before placing an order"
    ))
}
