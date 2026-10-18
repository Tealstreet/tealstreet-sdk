//! Bybit options as an instrument: symbol routing, instrument rows and greeks.
use safe_cex_types::{
    Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketLimitsPrice,
    MarketPrecision, OptionRight, OptionTerms, OptionTermsSettlement, OptionTermsStyle,
};
use serde_json::Value;

/// Measured on 2026-10-14 against Black-Scholes on mainnet BTC options; Bybit's docs do not state them.
pub const BYBIT_GREEK_UNITS: &str = "Black-Scholes per 1 unit of the underlying, in the premium asset: vega per 1 vol point, theta per calendar day";

// ts: num — parseFloat(String(value ?? '')), 0 when not finite.
fn num(value: Option<&Value>) -> Result<f64, String> {
    let text = match value {
        None | Some(Value::Null) => String::new(),
        Some(value) => safe_cex_num::js_string(Some(value)).map_err(|e| e.message.to_string())?,
    };
    let parsed = safe_cex_num::parse_float(&text);
    Ok(if parsed.is_finite() { parsed } else { 0. })
}
fn nested<'a>(raw: &'a Value, outer: &str, inner: &str) -> Option<&'a Value> {
    raw.get(outer)
        .filter(|value| !value.is_null())
        .and_then(|value| value.get(inner))
}
// ts: strikeOf — the third '-' segment, where the first `d` marks a decimal point.
fn strike_of(symbol: &str) -> Result<f64, String> {
    let segment = symbol
        .split('-')
        .nth(2)
        .map(|part| Value::String(part.replacen('d', ".", 1)));
    num(segment.as_ref())
}

// ts: mapBybitOptionInstrument — undefined when the row is not tradable.
pub fn map_bybit_option_instrument(raw: &Value) -> Result<Option<Market>, String> {
    if raw.get("status").and_then(Value::as_str) != Some("Trading") {
        return Ok(None);
    }
    let symbol = raw.get("symbol").and_then(Value::as_str).unwrap_or("");
    let strike = strike_of(symbol)?;
    let right = match raw.get("optionsType").and_then(Value::as_str) {
        Some("Call") => OptionRight::Call,
        Some("Put") => OptionRight::Put,
        _ => return Ok(None),
    };
    if strike == 0. {
        return Ok(None);
    }
    let text = |key: &str| {
        raw.get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    Ok(Some(Market {
        id: symbol.into(),
        symbol: symbol.into(),
        pretty_symbol: None,
        base: text("baseCoin"),
        quote: text("quoteCoin"),
        active: true,
        precision: MarketPrecision {
            amount: num(nested(raw, "lotSizeFilter", "qtyStep"))?,
            price: num(nested(raw, "priceFilter", "tickSize"))?,
        },
        limits: MarketLimits {
            amount: MarketLimitsAmount {
                min: num(nested(raw, "lotSizeFilter", "minOrderQty"))?,
                max: num(nested(raw, "lotSizeFilter", "maxOrderQty"))?,
            },
            leverage: MarketLimitsLeverage { min: 1., max: 1. },
            price: Some(MarketLimitsPrice {
                min: num(nested(raw, "priceFilter", "minPrice"))?,
            }),
        },
        option: Some(OptionTerms {
            underlying: text("baseCoin"),
            expiry: num(raw.get("deliveryTime"))?,
            strike,
            right,
            style: OptionTermsStyle::European,
            settlement: OptionTermsSettlement::Cash,
            premium_asset: text("quoteCoin"),
            settle_asset: text("settleCoin"),
        }),
        contract_size: Some(1.),
        is_expiring: Some(true),
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        info: Some(raw.clone().into()),
        taker_fee: None,
        maker_fee: None,
    }))
}

// ts: isBybitOptionSymbol — /^[A-Z0-9]+-\d{1,2}[A-Z]{3}\d{2}-\d+([d.]\d+)?-[CP](-[A-Z]+)?$/
pub fn is_bybit_option_symbol(symbol: &str) -> bool {
    let parts: Vec<&str> = symbol.split('-').collect();
    if !(4..=5).contains(&parts.len()) {
        return false;
    }
    let all = |s: &str, f: fn(&u8) -> bool| !s.is_empty() && s.as_bytes().iter().all(f);
    let base = all(parts[0], |b| b.is_ascii_uppercase() || b.is_ascii_digit());
    let date = parts[1].as_bytes();
    let date_ok = (6..=7).contains(&date.len()) && {
        let day = &date[..date.len() - 5];
        let rest = &date[date.len() - 5..];
        day.iter().all(u8::is_ascii_digit)
            && rest[..3].iter().all(u8::is_ascii_uppercase)
            && rest[3..].iter().all(u8::is_ascii_digit)
    };
    let strike = {
        let s = parts[2];
        match s.find(['d', '.']) {
            Some(i) => all(&s[..i], u8::is_ascii_digit) && all(&s[i + 1..], u8::is_ascii_digit),
            None => all(s, u8::is_ascii_digit),
        }
    };
    let right = matches!(parts[3], "C" | "P");
    let settle = parts.get(4).is_none_or(|s| all(s, u8::is_ascii_uppercase));
    base && date_ok && strike && right && settle
}
