//! Float-faithful placement/display arithmetic. Rendering-only orderbook totals
//! and candle-volume aggregates deliberately use plain `+` outside this crate.
//! Reference: [safe-cex guidance](../../packages/safe-cex/CLAUDE.md).
//! Hermetic tests: `cargo test -p safe-cex-num`. Node differential gate:
//! `cargo test -p safe-cex-num --all-targets -- --include-ignored`.

/// ECMAScript String.prototype.trim over Rust Unicode strings.
/// Uses WhiteSpace + LineTerminator, including BOM and excluding NEL.
/// Lone UTF-16 surrogates are outside Rust str's representable domain.
pub fn js_trim(value: &str) -> &str {
    value.trim_matches(js_whitespace)
}
/// ECMAScript String.prototype.trimStart, preserving trailing whitespace.
pub fn js_trim_start(value: &str) -> &str {
    value.trim_start_matches(js_whitespace)
}
/// ECMAScript String.prototype.trimEnd, preserving leading whitespace.
pub fn js_trim_end(value: &str) -> &str {
    value.trim_end_matches(js_whitespace)
}
/// ECMAScript split(/\s+/): coalesce runs but preserve boundary empty segments.
/// For an empty input this returns one empty segment, just as JavaScript does.
pub fn js_split_whitespace(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_run = false;
    for (offset, c) in value.char_indices() {
        if js_whitespace(c) {
            if !in_run {
                parts.push(&value[start..offset]);
            }
            in_run = true;
            start = offset + c.len_utf8();
        } else {
            in_run = false;
        }
    }
    parts.push(&value[start..]);
    parts
}

/// ECMAScript Number-to-String for signed or byte-compared numeric text.
/// Handles exponent thresholds and signed zero; nonfinite JSON needs caller policy.
pub fn js_number_string(value: f64) -> String {
    safe_cex_types::js_coercion::js_number_string(value)
}

/// Number(value) at arithmetic consumers of a retained numeric runtime property.
pub fn numeric_value_to_number(value: &safe_cex_types::NumericValue) -> f64 {
    match value {
        safe_cex_types::NumericValue::Number(value) => *value,
        safe_cex_types::NumericValue::Text(value) => number_from_string(value),
    }
}

pub fn js_to_fixed(value: f64, digits: f64) -> Result<String, &'static str> {
    use num_bigint::BigUint;
    let digits = if digits.is_nan() { 0.0 } else { digits.trunc() };
    if !(0.0..=100.0).contains(&digits) {
        return Err("toFixed() digits argument must be between 0 and 100");
    }
    if !value.is_finite() || value.abs() >= 1e21 {
        return Ok(js_number_string(value));
    }
    let digits = digits as u32;
    let bits = value.abs().to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = (bits & ((1_u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1_u64 << 52 };
    let power = if exponent == 0 {
        -1074
    } else {
        exponent - 1023 - 52
    };
    let mut scaled = BigUint::from(mantissa) * BigUint::from(10_u32).pow(digits);
    if power >= 0 {
        scaled <<= power as usize;
    } else {
        let divisor = BigUint::from(1_u32) << (-power as usize);
        let remainder = &scaled % &divisor;
        scaled /= &divisor;
        if remainder * 2_u32 >= divisor {
            scaled += 1_u32;
        }
    }
    let mut text = scaled.to_string();
    if digits > 0 {
        while text.len() <= digits as usize {
            text.insert(0, '0');
        }
        text.insert(text.len() - digits as usize, '.');
    }
    if value < 0.0 {
        text.insert(0, '-');
    }
    Ok(text)
}

/// ECMAScript Number.prototype.toPrecision over the exact binary value: the
/// nearest `precision` significant digits, ties to the larger magnitude.
pub fn js_to_precision(value: f64, precision: f64) -> Result<String, &'static str> {
    use num_bigint::BigUint;
    if !value.is_finite() {
        return Ok(js_number_string(value));
    }
    let precision = if precision.is_nan() {
        0.0
    } else {
        precision.trunc()
    };
    if !(1.0..=100.0).contains(&precision) {
        return Err("toPrecision() argument must be between 1 and 100");
    }
    let p = precision as i64;
    if value == 0.0 {
        let mut text = "0".to_owned();
        if p > 1 {
            text.push('.');
            text.extend(std::iter::repeat_n('0', (p - 1) as usize));
        }
        return Ok(text);
    }
    let bits = value.abs().to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i64;
    let mantissa = (bits & ((1_u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1_u64 << 52 };
    let power = if exponent == 0 {
        -1074
    } else {
        exponent - 1075
    };
    // |value| = num / den exactly.
    let (num, den) = if power >= 0 {
        (
            BigUint::from(mantissa) << power as usize,
            BigUint::from(1_u32),
        )
    } else {
        (
            BigUint::from(mantissa),
            BigUint::from(1_u32) << (-power) as usize,
        )
    };
    let ten = BigUint::from(10_u32);
    let pow10 = |n: i64| ten.pow(n.unsigned_abs() as u32);
    // 10^e <= |value| < 10^(e+1), starting from the f64 estimate.
    let at_least = |e: i64| {
        if e >= 0 {
            num >= &den * pow10(e)
        } else {
            &num * pow10(e) >= den
        }
    };
    let mut e = value.abs().log10().floor() as i64;
    while !at_least(e) {
        e -= 1;
    }
    while at_least(e + 1) {
        e += 1;
    }
    // n = round(|value| / 10^(e-p+1)), ties to the larger n.
    let shift = e - p + 1;
    let (scaled_num, scaled_den) = if shift >= 0 {
        (num, den * pow10(shift))
    } else {
        (num * pow10(shift), den)
    };
    let mut n = &scaled_num / &scaled_den;
    if (&scaled_num % &scaled_den) * 2_u32 >= scaled_den {
        n += 1_u32;
    }
    if n == pow10(p) {
        n = pow10(p - 1);
        e += 1;
    }
    let digits = n.to_string();
    let mut text = if value < 0.0 {
        "-".to_owned()
    } else {
        String::new()
    };
    if e < -6 || e >= p {
        text.push_str(&digits[..1]);
        if p > 1 {
            text.push('.');
            text.push_str(&digits[1..]);
        }
        text.push('e');
        text.push(if e >= 0 { '+' } else { '-' });
        text.push_str(&e.unsigned_abs().to_string());
    } else if e == p - 1 {
        text.push_str(&digits);
    } else if e >= 0 {
        text.push_str(&digits[..(e + 1) as usize]);
        text.push('.');
        text.push_str(&digits[(e + 1) as usize..]);
    } else {
        text.push_str("0.");
        text.extend(std::iter::repeat_n('0', (-(e + 1)) as usize));
        text.push_str(&digits);
    }
    Ok(text)
}

/// JavaScript Math.round: ties toward positive infinity; preserves NaN and -0.
/// For rendering precision and NaN-to-zero policy, use display_round instead.
pub fn math_round(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let floor = value.floor();
    let rounded = if value - floor < 0.5 {
        floor
    } else {
        floor + 1.0
    };
    if rounded == 0.0 {
        0.0_f64.copysign(value)
    } else {
        rounded
    }
}

fn pow10(n: f64) -> f64 {
    10.0_f64.powf(n)
}

/// Rust representation of TypeScript's `number | string` precision input.
pub enum PrecisionInput {
    Number(f64),
    String(String),
}

impl From<f64> for PrecisionInput {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}
impl From<&str> for PrecisionInput {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}
impl From<String> for PrecisionInput {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

/// Preserves JavaScript number strings and explicit trailing decimal digits.
pub fn after_decimal(num: impl Into<PrecisionInput>) -> f64 {
    match num.into() {
        PrecisionInput::Number(n) => decimals(n),
        PrecisionInput::String(s) => decimal_string(&s),
    }
}

fn decimal_string(text: &str) -> f64 {
    if text.is_empty() {
        return 2.0;
    }
    let parts: Vec<_> = text.split(['e', 'E']).collect();
    if parts.len() == 2 {
        let mantissa = parts[0]
            .split('.')
            .nth(1)
            .map_or(0, |s| s.encode_utf16().count());
        let exponent = number_from_string(parts[1]);
        return if exponent.is_finite() {
            (mantissa as f64 - exponent).max(0.0)
        } else {
            2.0
        };
    }
    text.split_once('.')
        .map_or(0.0, |(_, tail)| tail.encode_utf16().count() as f64)
}

fn decimals(value: f64) -> f64 {
    if value.is_finite() && value.fract() == 0.0 {
        0.0
    } else {
        decimal_string(&js_number_string(value))
    }
}

/// ECMAScript Number(string), including full-string validation and radix prefixes.
pub fn number_from_string(text: &str) -> f64 {
    let text = text.trim_matches(js_whitespace);
    if text.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16_u32),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ] {
        if let Some(digits) = text.strip_prefix(prefix) {
            if digits.is_empty() {
                return f64::NAN;
            }
            let mut bits = String::new();
            let width = radix.ilog2();
            for c in digits.chars() {
                let Some(digit) = c.to_digit(radix) else {
                    return f64::NAN;
                };
                bits.push_str(&format!("{digit:0width$b}", width = width as usize));
            }
            let bits = bits.trim_start_matches('0');
            if bits.is_empty() {
                return 0.0;
            }
            if bits.len() <= 53 {
                return u64::from_str_radix(bits, 2).unwrap() as f64;
            }
            let mut significand = u64::from_str_radix(&bits[..53], 2).unwrap();
            let tail = &bits[53..];
            if tail.starts_with('1') && (tail[1..].contains('1') || significand % 2 == 1) {
                significand += 1;
            }
            return significand as f64 * 2.0_f64.powf((bits.len() - 53) as f64);
        }
    }
    if matches!(
        text.to_ascii_lowercase().as_str(),
        "inf" | "+inf" | "-inf" | "infinity" | "+infinity" | "-infinity"
    ) && !matches!(text, "Infinity" | "+Infinity" | "-Infinity")
    {
        return f64::NAN;
    }
    text.parse().unwrap_or(f64::NAN)
}

fn js_whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

pub fn adjust(value: f64, step: impl Into<PrecisionInput>) -> f64 {
    let step = step.into();
    let (numeric_step, precision) = match step {
        PrecisionInput::Number(n) => (n, decimals(n)),
        PrecisionInput::String(s) => (number_from_string(&s), decimal_string(&s)),
    };
    let multiplier = 1.0 / numeric_step;
    let adjusted = math_round(value * multiplier) / multiplier;
    let factor = pow10(precision);
    math_round(adjusted * factor) / factor
}

pub fn add(a: f64, b: f64) -> f64 {
    let factor = pow10(decimals(a).max(decimals(b)));
    math_round((a + b) * factor) / factor
}

pub fn subtract(a: f64, b: f64) -> f64 {
    let factor = pow10(decimals(a).max(decimals(b)));
    math_round((a - b) * factor) / factor
}

// ts-bug: multiply1 — the original max-decimal rule loses small products.
pub fn multiply1(a: f64, b: f64) -> f64 {
    let factor = pow10(decimals(a).max(decimals(b)));
    math_round(a * b * factor) / factor
}

// ts-bug: divide1 — integer inputs round fractions to integers.
pub fn divide1(a: f64, b: f64) -> f64 {
    let factor = pow10(decimals(a).max(decimals(b)));
    math_round((a / b) * factor) / factor
}

fn fixed_number(value: f64, digits: u32) -> f64 {
    if !value.is_finite() || value.abs() >= 1e21 {
        return value;
    }
    let bits = value.abs().to_bits();
    let exponent = ((bits >> 52) & 2047) as i32;
    let mantissa = (bits & ((1_u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1_u64 << 52 };
    let scaled = u128::from(mantissa) * 5_u128.pow(digits);
    let shift = (if exponent == 0 {
        -1074
    } else {
        exponent - 1075
    }) + digits as i32;
    let integer = if shift >= 0 {
        scaled << shift
    } else if -shift >= 128 {
        0
    } else {
        let shift = -shift as u32;
        (scaled >> shift)
            + u128::from((scaled & ((1_u128 << shift) - 1)) >= (1_u128 << (shift - 1)))
    };
    let text = format!("{integer:0width$}", width = digits as usize + 1);
    let split = text.len() - digits as usize;
    let fixed = format!(
        "{}{}.{}",
        if value < 0.0 { "-" } else { "" },
        &text[..split],
        &text[split..]
    );
    fixed.parse().expect("decimal digits")
}

pub fn handle_rounding_error(value: f64) -> f64 {
    if value.is_nan() {
        0.0
    } else {
        fixed_number(value, 10)
    }
}

/// Rendering-only magnitude-dependent precision; NaN becomes zero.
/// This is not JavaScript Math.round; arithmetic callers use math_round.
pub fn display_round(value: f64) -> f64 {
    if value.is_nan() {
        return 0.0;
    }
    let magnitude = value.abs();
    let digits = if magnitude == 0.0 {
        1
    } else if magnitude >= 1000.0 {
        2
    } else if magnitude >= 10.0 {
        3
    } else if magnitude >= 1.0 {
        4
    } else if magnitude >= 0.0001 {
        5
    } else if magnitude >= 0.00001 {
        6
    } else if magnitude >= 0.000001 {
        7
    } else {
        8
    };
    fixed_number(value, digits)
}

pub fn round_usd(value: f64) -> f64 {
    math_round(value * 100.0) / 100.0
}

fn parse_integer(text: &str, radix: u32) -> f64 {
    let text = text.trim_start_matches(js_whitespace);
    let (negative, rest) = if let Some(t) = text.strip_prefix('-') {
        (true, t)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let (radix, rest) = if radix == 0 && (rest.starts_with("0x") || rest.starts_with("0X")) {
        (16, &rest[2..])
    } else {
        (if radix == 0 { 10 } else { radix }, rest)
    };
    let digits: String = rest
        .chars()
        .take_while(|c| c.is_ascii() && c.is_digit(radix))
        .collect();
    if digits.is_empty() {
        return f64::NAN;
    }
    let value = if radix == 10 {
        digits.parse().unwrap_or(f64::INFINITY)
    } else {
        number_from_string(&format!("0x{digits}"))
    };
    if negative { -value } else { value }
}

pub fn parse_precision(precision: Option<&str>) -> Result<Option<String>, &'static str> {
    let Some(precision) = precision else {
        return Ok(None);
    };
    let value = parse_integer(precision, 10);
    if value.is_nan() || value < 0.0 {
        return Ok(None);
    }
    if value > 536_870_888.0 {
        return Err("Invalid string length");
    }
    Ok(Some(if value == 0.0 {
        "1".into()
    } else {
        format!("0.{}1", "0".repeat(value as usize - 1))
    }))
}

pub mod ccxt_precision;

pub fn multiply(a: f64, b: f64) -> f64 {
    let factor = pow10(decimals(a) + decimals(b));
    math_round(a * b * factor) / factor
}

/// `None` selects the TypeScript default of eight decimal places.
pub fn divide(a: f64, b: f64, min_decimals: Option<f64>) -> f64 {
    let min_decimals = min_decimals.unwrap_or(8.0);
    let precision = if min_decimals.is_nan() {
        f64::NAN
    } else {
        decimals(a).max(decimals(b)).max(min_decimals)
    };
    let factor = pow10(precision);
    math_round((a / b) * factor) / factor
}

fn is_smaller(candidate: &str, current: &str) -> bool {
    if candidate.len() != current.len() {
        candidate.len() < current.len()
    } else {
        candidate < current
    }
}

/// Chooses the oldest valid cursor without parsing or normalizing its digits.
/// Callers extract string IDs at the wire boundary; numeric JSON is not accepted.
pub fn oldest_cursor_id<S: AsRef<str>>(ids: &[S]) -> Option<String> {
    let mut oldest: Option<String> = None;
    for raw in ids {
        let id = raw.as_ref().trim_matches(js_whitespace);
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if oldest
            .as_ref()
            .is_none_or(|current| is_smaller(id, current))
        {
            oldest = Some(id.to_owned());
        }
    }
    oldest
}

/// The TypeScript input shape, with `extra.contractsNative` flattened.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeSizeInputs {
    pub contracts: f64,
    pub notional: Option<f64>,
    pub contracts_native: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeSizeMarket {
    pub contract_size: Option<f64>,
    pub is_inverse: Option<bool>,
}

fn or_default(value: Option<f64>, fallback: f64) -> f64 {
    value
        .filter(|v| *v != 0.0 && !v.is_nan())
        .unwrap_or(fallback)
}

/// Inverse handling is sizing only; price/PnL remains unsolved and refused.
pub fn native_position_size(position: &NativeSizeInputs, market: &NativeSizeMarket) -> f64 {
    let from_extra = or_default(position.contracts_native, 0.0).abs();
    let contract_size = or_default(market.contract_size, 1.0);
    let source = if market.is_inverse.unwrap_or(false) {
        position.notional
    } else {
        Some(position.contracts)
    };
    let derived = or_default(source, 0.0).abs() / contract_size;
    if from_extra.is_nan() || derived.is_nan() {
        f64::NAN
    } else if derived == 0.0 && from_extra == 0.0 {
        0.0
    } else {
        from_extra.max(derived)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn javascript_rounding_edges() {
        assert_eq!(math_round(-0.5).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(math_round(-1.5), -1.0);
        assert_eq!(math_round(0.49999999999999994), 0.0);
        assert_eq!(math_round(4503599627370495.5), 4503599627370496.0);
        assert!(math_round(f64::NAN).is_nan());
        assert_eq!(math_round(f64::INFINITY), f64::INFINITY);
    }

    #[test]
    fn placement_arithmetic_and_precision() {
        assert_eq!(add(0.1, 0.2), 0.3);
        assert_eq!(subtract(0.3, 0.1), 0.2);
        assert_eq!(multiply(0.0002, 0.0003), 0.00000006);
        assert_eq!(divide(1.0, 3.0, None), 0.33333333);
        assert_eq!(divide(-1.0, 2.0, Some(0.0)).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(adjust(-0.5, 1.0).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(adjust(-1.235, "0.01"), -1.24);
        assert!(adjust(1.0, f64::NAN).is_nan());
        assert!(adjust(1.0, f64::INFINITY).is_nan());
        assert_eq!(after_decimal("1.2300"), 4.0);
        assert_eq!(after_decimal("2.5E-7"), 8.0);
        assert_eq!(after_decimal(5e-324), 324.0);
        assert_eq!(after_decimal(f64::NAN), 0.0);
        assert_eq!(after_decimal(f64::INFINITY), 0.0);
    }

    #[test]
    fn string_number_infinity_is_case_sensitive() {
        for text in ["INFINITY", "Inf", "INF", "inf", "+infinity", "-INFINITY"] {
            assert!(number_from_string(text).is_nan(), "{text}");
        }
        for text in ["Infinity", "+Infinity", "1e999"] {
            assert_eq!(number_from_string(text), f64::INFINITY);
        }
        assert_eq!(number_from_string("-Infinity"), f64::NEG_INFINITY);
        assert_eq!(
            adjust(4.0706880089046605e23, "0x56333e7122f01abec371"),
            4.0706880089046605e23
        );
    }

    #[test]
    fn cursor_digits_remain_exact() {
        let empty: [&str; 0] = [];
        assert_eq!(oldest_cursor_id(&empty), None);
        assert_eq!(oldest_cursor_id(&["007", "7"]), Some("7".into()));
        assert_eq!(
            oldest_cursor_id(&["953507828759029500001", "953507828759029500000"]),
            Some("953507828759029500000".into())
        );
        assert_eq!(
            oldest_cursor_id(&["9007199254740993", "9007199254740992"]),
            Some("9007199254740992".into())
        );
        let long = "9".repeat(1000);
        let shorter = "1".repeat(999);
        assert_eq!(oldest_cursor_id(&[long, shorter.clone()]), Some(shorter));
        assert_eq!(
            oldest_cursor_id(&["\u{feff}12\u{feff}", "\u{0085}11\u{0085}", "١", "-1", ""]),
            Some("12".into())
        );
    }

    #[test]
    fn native_sizing_uses_notional_only_for_inverse() {
        let p = NativeSizeInputs {
            contracts: 2.0,
            notional: Some(100.0),
            contracts_native: Some(0.1),
        };
        let mut m = NativeSizeMarket {
            contract_size: Some(0.01),
            is_inverse: Some(false),
        };
        assert_eq!(native_position_size(&p, &m), 200.0);
        m.is_inverse = Some(true);
        assert_eq!(native_position_size(&p, &m), 10000.0);
        m.contract_size = Some(f64::NAN);
        assert_eq!(native_position_size(&p, &m), 100.0);
        m.contract_size = Some(f64::INFINITY);
        let p = NativeSizeInputs {
            notional: Some(f64::INFINITY),
            ..p
        };
        assert!(native_position_size(&p, &m).is_nan());
        let p = NativeSizeInputs::default();
        m.contract_size = Some(-1.0);
        assert_eq!(native_position_size(&p, &m).to_bits(), 0.0_f64.to_bits());
    }
}

pub mod coercion;
pub mod order;
pub mod order_policy;
pub mod sizing;
pub use coercion::{
    JsCoercionError, JsNumberObject, JsPrimitive, js_number, js_number_object, js_number_primitive,
    js_retained_number, js_retained_parse_float, js_retained_string, js_string, js_to_number,
    js_to_number_object, js_truthy, parse_float, parse_int,
};

/// Compatibility alias for display_round, not JavaScript Math.round.
pub use display_round as round;

mod own_keys;
pub use own_keys::{
    js_array_index, js_object_or_array_entry, js_own_keys, normalize_json_order,
    normalize_object_order,
};

mod json_stringify;
pub use json_stringify::{
    JsJsonValue, js_json_stringify, js_retained_json_stringify, js_retained_truthy,
};

mod v8_math;
pub use v8_math::math_log10;

mod v8_pow;
pub use v8_pow::math_pow;

mod bigint;
pub use bigint::{
    JsBigIntError, JsBigIntErrorKind, js_bigint, js_bigint_object, js_bigint_primitive,
};

mod collation;
pub use collation::JsRootCollator;

mod math_extrema;
pub use math_extrema::{math_max, math_min};

mod date;
pub use date::{js_date_parse, js_date_parse_utf16, js_date_to_iso_string, js_time_clip};

mod lodash_case;
pub use lodash_case::{lodash_camel_case, lodash_snake_case};

mod table_lookup;
pub use table_lookup::{ProtoMember, TableHit, js_table_lookup};

mod string_replace;
pub use string_replace::{js_replace, js_replace_all, js_replace_all_utf16, js_replace_utf16};

mod lodash_random;
pub use lodash_random::{lodash_random, lodash_random_json};

pub mod get_key;

/// Whole-path JSON Number ingress.
pub mod numeric_ingress;

pub mod prototype_realm;
pub use prototype_realm::{
    BuiltinIdentity, BuiltinReference, FunctionMethod, PrototypeRealm, PrototypeRealmBinding,
    PrototypeRealmFuture, PrototypeScalar, PrototypeValue,
};

pub mod sorting;
