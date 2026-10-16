//! WEEX retry helpers, with source host dependencies supplied explicitly.
use crate::types::ENDPOINTS;

/// Source config fields with the imported error-classifier and delay capability
/// bound by the calling transport. Callbacks capture the original source error.
#[derive(Debug, Clone, Copy)]
pub struct WeexRetryConfig {
    pub retries: u32,
    pub retry_condition: fn(
        Option<&[u16]>,
        Option<&[u16]>,
        Option<f64>,
        Option<f64>,
        &mut dyn FnMut() -> bool,
    ) -> bool,
    pub retry_delay: fn(f64, Option<f64>, &mut dyn FnMut() -> f64) -> f64,
    pub should_reset_timeout: bool,
}

pub const WEEX_RETRY_CONFIG: WeexRetryConfig = WeexRetryConfig {
    retries: 3,
    retry_condition: weex_retry_condition,
    retry_delay: weex_retry_delay,
    should_reset_timeout: true,
};

pub fn weex_retry_condition(
    method: Option<&[u16]>,
    url: Option<&[u16]>,
    response_status: Option<f64>,
    retry_count: Option<f64>,
    is_network_error: &mut dyn FnMut() -> bool,
) -> bool {
    should_retry_weex_request(method, url, response_status, retry_count, is_network_error)
}

pub fn weex_retry_delay(
    _retry_count: f64,
    response_status: Option<f64>,
    rate_limit_delay: &mut dyn FnMut() -> f64,
) -> f64 {
    if response_status == Some(429.0) {
        rate_limit_delay()
    } else {
        0.0
    }
}

/// The source typeof guard rejects every kind other than Number and String.
/// Other includes undefined, null, booleans, objects, functions, symbols and BigInt.
#[derive(Debug, Clone, Copy)]
pub enum WeexRetryAfterValue<'a> {
    Number(f64),
    String(&'a [u16]),
    Other,
}

/// `now` is the source clock value; `parse_date` is its Date.parse capability,
/// receiving original UTF16 rather than an implicit UTF8 or UTC projection.
pub fn parse_retry_after_ms(
    value: WeexRetryAfterValue<'_>,
    now: f64,
    parse_date: impl FnOnce(&[u16]) -> f64,
) -> f64 {
    let (seconds, text) = match value {
        WeexRetryAfterValue::Other => return 0.0,
        WeexRetryAfterValue::Number(number) => (
            number,
            safe_cex_num::js_number_string(number)
                .encode_utf16()
                .collect::<Vec<_>>(),
        ),
        WeexRetryAfterValue::String(units) => {
            // A lone surrogate cannot occur in StringNumericLiteral; Number
            // returns NaN, while Date.parse still receives the original units.
            let number = String::from_utf16(units)
                .map_or(f64::NAN, |text| safe_cex_num::number_from_string(&text));
            (number, units.to_vec())
        }
    };
    if seconds.is_finite() && seconds > 0.0 {
        return seconds * 1_000.0;
    }
    let timestamp = parse_date(&text);
    if timestamp.is_finite() {
        safe_cex_num::math_max(timestamp - now, 0.0)
    } else {
        0.0
    }
}

/// Flattened optional Axios config fields, represented as complete UTF16 strings.
pub fn is_weex_read_only_request(method: Option<&[u16]>, url: Option<&[u16]>) -> bool {
    // Replacement of a lone surrogate cannot create an accepted ASCII method.
    let method = method.map(|units| String::from_utf16_lossy(units).to_uppercase());
    match method.as_deref() {
        Some("GET" | "HEAD") => true,
        Some("POST") => {
            let Some(url) = url else {
                return false;
            };
            let path = url.split(|unit| *unit == u16::from(b'?')).next().unwrap();
            let income = ENDPOINTS
                .iter()
                .find(|(key, _)| *key == "INCOME")
                .unwrap()
                .1;
            path.iter().copied().eq(income.encode_utf16())
        }
        _ => false,
    }
}

/// The host binds the imported axios-retry isNetworkError function. Preserve its
/// short-circuit call order rather than copying a second network-error policy.
pub fn should_retry_weex_request(
    method: Option<&[u16]>,
    url: Option<&[u16]>,
    response_status: Option<f64>,
    retry_count: Option<f64>,
    is_network_error: impl FnOnce() -> bool,
) -> bool {
    if !is_weex_read_only_request(method, url) {
        return false;
    }
    if is_network_error() {
        return true;
    }
    if response_status != Some(429.0) {
        return false;
    }
    retry_count.unwrap_or(0.0) < 1.0
}

pub fn clamp_weex_rate_limit_delay(delay_ms: f64) -> f64 {
    safe_cex_num::math_min(safe_cex_num::math_max(delay_ms, 0.0), 60_000.0)
}

/// Source string code units, including lone surrogates in an arbitrary prefix.
pub fn parse_rate_limit_interval_ms(header_name: &[u16]) -> f64 {
    let Some((unit, prefix)) = header_name.split_last() else {
        return 0.0;
    };
    let unit_ms = match *unit {
        83 | 115 => 1_000.0,
        77 | 109 => 60_000.0,
        72 | 104 => 3_600_000.0,
        68 | 100 => 86_400_000.0,
        _ => return 0.0,
    };
    let digits_start = prefix
        .iter()
        .rposition(|unit| !(48..=57).contains(unit))
        .map_or(0, |index| index + 1);
    if digits_start == 0 || digits_start == prefix.len() || prefix[digits_start - 1] != 45 {
        return 0.0;
    }
    let digits: String = prefix[digits_start..]
        .iter()
        .map(|unit| char::from(*unit as u8))
        .collect();
    let interval = safe_cex_num::parse_float(&digits);
    if interval.is_finite() && interval > 0.0 {
        interval * unit_ms
    } else {
        0.0
    }
}
