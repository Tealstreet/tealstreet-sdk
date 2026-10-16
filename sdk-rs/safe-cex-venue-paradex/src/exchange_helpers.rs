//! Source correspondence: standalone pricing helpers and interval conversion in paradex.exchange.ts.
use safe_cex_num::{JsCoercionError, adjust, js_number, js_number_string, parse_float};
use serde_json::Value;

// ts: isPositiveFinitePrice
pub fn is_positive_finite_price(price: Option<f64>) -> bool {
    price.is_some_and(|price| price.is_finite() && price > 0.0)
}

// ts: normalizePositiveFraction
pub fn normalize_positive_fraction(value: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    let parsed = js_number(value)?;
    if !parsed.is_finite() || parsed <= 0.0 {
        return Ok(None);
    }
    Ok(Some(if parsed > 1.0 { parsed / 100.0 } else { parsed }))
}

// ts: roundPriceToStep
pub fn round_price_to_step(price: f64, price_step: f64, direction: &str) -> f64 {
    let step = if price_step.is_finite() && price_step > 0.0 {
        price_step
    } else {
        0.000001
    };
    let ratio = price / step;
    let rounded_ratio = if direction == "up" {
        (ratio - f64::EPSILON).ceil()
    } else {
        (ratio + f64::EPSILON).floor()
    };
    adjust(rounded_ratio * step, step)
}

// ts: ParadexExchange#convertIntervalToTradingViewResolution
pub fn convert_interval_to_trading_view_resolution(interval: &str) -> Result<String, String> {
    // JavaScript \d without Unicode extensions matches only ASCII digits.
    let bytes = interval.as_bytes();
    let valid = bytes.len() >= 2
        && bytes[..bytes.len() - 1].iter().all(u8::is_ascii_digit)
        && matches!(
            bytes[bytes.len() - 1],
            b'm' | b'h' | b'd' | b'w' | b'M' | b'y'
        );
    if !valid {
        return Err(format!(
            "Invalid interval format: {interval}. Expected format: <number><unit> (e.g., \"1m\", \"5m\", \"1h\", \"1d\")"
        ));
    }
    // The validated capture contains only decimal digits: parseFloat and parseInt agree.
    let value = parse_float(&interval[..interval.len() - 1]);
    let resolution = match bytes[bytes.len() - 1] {
        b'm' => js_number_string(value),
        b'h' => js_number_string(value * 60.0),
        b'd' => format!("{}D", js_number_string(value)),
        b'w' => format!("{}W", js_number_string(value)),
        b'M' => format!("{}M", js_number_string(value)),
        b'y' => format!("{}M", js_number_string(value * 12.0)),
        _ => unreachable!("validated source regex unit"),
    };
    Ok(resolution)
}
