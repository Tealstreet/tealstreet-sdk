//! BingX TradFi placement-message helpers; no transport or Store error conversion.

pub fn build_tradfi_one_way_mode_error(
    symbol: &str,
    original_error_message: Option<&str>,
) -> String {
    let details = original_error_message
        .filter(|message| !message.is_empty())
        .map(|message| format!(" (BingX message: {message})"))
        .unwrap_or_default();
    format!(
        "BingX OpenAPI requires Hedged position mode for non-crypto symbols. Switch this BingX account to Hedged mode and retry ({symbol}).{details}"
    )
}

pub fn normalize_order_placement_error(symbol: &str, error_message: &str) -> String {
    // The source regexp is ASCII, case-insensitive, unanchored, and lacks /u.
    if error_message
        .to_ascii_lowercase()
        .contains("non-crypto symbol with one-way mode not support openapi and copy trade")
    {
        return build_tradfi_one_way_mode_error(symbol, Some(error_message));
    }
    error_message.to_owned()
}
