//! Inverse routing read from the symbol, so a row routes before its market loads.
use safe_cex_types::Position;

// ts: isBybitInverseSymbol — /^[A-Z0-9]+USD([FGHJKMNQUVXZ]\d{2})?$/
pub fn is_bybit_inverse_symbol(symbol: &str) -> bool {
    let bytes = symbol.as_bytes();
    if !bytes
        .iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return false;
    }
    let perpetual = bytes.len() > 3 && bytes.ends_with(b"USD");
    let dated = bytes.len() > 6 && {
        let tail = &bytes[bytes.len() - 6..];
        &tail[..3] == b"USD"
            && b"FGHJKMNQUVXZ".contains(&tail[3])
            && tail[4].is_ascii_digit()
            && tail[5].is_ascii_digit()
    };
    perpetual || dated
}

// ts: positionUpnlInQuote — an inverse position's upnl is in the coin; value it at the
// USD notional over coin contracts mapPosition sized it with.
pub fn position_upnl_in_quote(position: &Position) -> f64 {
    let upnl = if position.unrealized_pnl.is_nan() {
        0.
    } else {
        position.unrealized_pnl
    };
    let contracts = position.contracts;
    if !is_bybit_inverse_symbol(&position.symbol) || contracts == 0. || contracts.is_nan() {
        return upnl;
    }
    (upnl * position.notional) / contracts
}
