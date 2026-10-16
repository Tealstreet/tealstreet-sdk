//! Public websocket string bodies at the source casing/conversion/table boundaries.

pub fn convert_to_ws_symbol<E>(
    symbol: &[u16],
    lower_case: impl FnOnce(&[u16]) -> Result<Vec<u16>, E>,
) -> Result<Vec<u16>, E> {
    let symbol = if symbol.len() >= 2
        && matches!(symbol[0], 0x45 | 0x65)
        && matches!(symbol[1], 0x2d | 0x5f)
    {
        &symbol[2..]
    } else {
        symbol
    };
    let normalized: Vec<_> = symbol
        .iter()
        .copied()
        .filter(|unit| !matches!(unit, 0x2d | 0x5f | 0x2f))
        .collect();
    let normalized = lower_case(&normalized)?;
    Ok("e_".encode_utf16().chain(normalized).collect())
}

pub fn format_kline_topic<I, E>(
    symbol: &[u16],
    interval: I,
    convert_symbol_as_template: impl FnOnce(&[u16]) -> Result<Vec<u16>, E>,
    interval_as_template: impl FnOnce(I) -> Result<Vec<u16>, E>,
) -> Result<Vec<u16>, E> {
    // Source template evaluation converts the symbol result before the table term.
    let symbol = convert_symbol_as_template(symbol)?;
    let interval = interval_as_template(interval)?;
    Ok("market_"
        .encode_utf16()
        .chain(symbol)
        .chain("_kline_".encode_utf16())
        .chain(interval)
        .collect())
}
