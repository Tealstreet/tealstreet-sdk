//! Unknown numeric fields in Classic v2 and Unified v3 account/position responses.
use safe_cex_num::{js_trim, parse_float};

// ts: BitgetExchange.toFiniteNumber (observed typeof branches)
#[derive(Clone, Copy, Debug)]
pub enum FiniteNumberInput<'a> {
    Number(f64),
    String(&'a str),
    Other,
}

// ts: BitgetExchange.toFiniteNumber
pub fn to_finite_number(value: FiniteNumberInput<'_>, fallback: f64) -> f64 {
    match value {
        FiniteNumberInput::Number(value) => {
            if value.is_finite() {
                value
            } else {
                fallback
            }
        }
        FiniteNumberInput::String(value) => {
            if js_trim(value).is_empty() {
                return fallback;
            }
            let parsed = parse_float(value);
            if parsed.is_finite() { parsed } else { fallback }
        }
        FiniteNumberInput::Other => fallback,
    }
}

// ts: BitgetExchange.firstFiniteNumber
pub fn first_finite_number(values: &[FiniteNumberInput<'_>], fallback: f64) -> f64 {
    for value in values {
        let parsed = to_finite_number(*value, f64::NAN);
        if parsed.is_finite() {
            return parsed;
        }
    }
    fallback
}
