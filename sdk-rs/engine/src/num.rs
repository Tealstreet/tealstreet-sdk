//! Precision helpers for venue numbers.

/// Venue metadata is untrusted; `f64::clamp` panics on unordered bounds.
pub fn clamp_leverage(value: f64, min: f64, max: f64) -> Result<f64, &'static str> {
    if !value.is_finite() || value <= 0.0 {
        return Err("Invalid leverage");
    }
    if !min.is_finite() || !max.is_finite() || min <= 0.0 || min > max {
        return Err("Invalid market leverage limits");
    }
    Ok(value.clamp(min, max))
}

/// Parse a venue numeric string; malformed input reads as 0 rather than NaN so
/// it can never poison store arithmetic.
pub fn parse(text: &str) -> f64 {
    parse_decimal(text).unwrap_or(0.0)
}

/// A finite decimal string, as JavaScript's `Number` reads it after trimming;
/// `None` for blank, malformed or non-finite text.
pub fn parse_decimal(text: &str) -> Option<f64> {
    short_decimal(text)
        .or_else(|| text.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

// Up to 15 digits keep the mantissa and the power of ten exact in f64, so one
// division rounds the same rational as the general parser, only faster.
fn short_decimal(text: &str) -> Option<f64> {
    const POWERS: [f64; 16] = [
        1.0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15,
    ];
    let bytes = text.as_bytes();
    let negative = bytes.first() == Some(&b'-');
    let digits = if negative { &bytes[1..] } else { bytes };
    if digits.len() > 16 {
        return None;
    }
    let (mut mantissa, mut count, mut point) = (0u64, 0usize, None);
    for (index, &byte) in digits.iter().enumerate() {
        if byte.is_ascii_digit() {
            mantissa = mantissa * 10 + u64::from(byte - b'0');
            count += 1;
        } else if byte == b'.' && point.is_none() {
            point = Some(index);
        } else {
            return None;
        }
    }
    if count == 0 || count > 15 {
        return None;
    }
    let scale = point.map_or(0, |index| digits.len() - index - 1);
    let value = mantissa as f64 / POWERS[scale];
    Some(if negative { -value } else { value })
}

/// Digits after the decimal point in the shortest representation of `step`.
pub fn decimals(step: f64) -> u32 {
    if step.fract() == 0.0 {
        return 0;
    }
    let text = step.to_string();
    text.find('.')
        .map_or(0, |dot| (text.len() - dot - 1) as u32)
}

/// Round half up (toward +inf), the rounding venues and the TS SDK expect.
pub fn round_half_up(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// Round `value` to the nearest multiple of `step`, then trim float noise to
/// the step's decimal places.
pub fn adjust(value: f64, step: f64) -> f64 {
    if step <= 0.0 || !step.is_finite() {
        return value;
    }
    let multiplier = 1.0 / step;
    let adjusted = round_half_up(value * multiplier) / multiplier;
    let factor = 10f64.powi(decimals(step) as i32);
    round_half_up(adjusted * factor) / factor
}

/// Render a number for a request parameter: shortest round-trip digits, never
/// exponent notation.
pub fn format(value: f64) -> String {
    let text = value.to_string();
    if text.contains('e') {
        format!("{value:.12}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leverage_bounds_cannot_panic_or_produce_invalid_writes() {
        assert_eq!(clamp_leverage(5.0, 1.0, 20.0), Ok(5.0));
        assert_eq!(clamp_leverage(25.0, 1.0, 20.0), Ok(20.0));
        assert_eq!(clamp_leverage(0.5, 1.0, 20.0), Ok(1.0));
        for (min, max) in [
            (1.0, 0.0),
            (20.0, 1.0),
            (0.0, 20.0),
            (f64::NAN, 20.0),
            (1.0, f64::NAN),
            (1.0, f64::INFINITY),
        ] {
            assert!(clamp_leverage(5.0, min, max).is_err());
        }
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(clamp_leverage(value, 1.0, 20.0).is_err());
        }
    }

    #[test]
    fn adjusts_to_step_without_float_noise() {
        assert_eq!(adjust(0.123456, 0.001), 0.123);
        assert_eq!(adjust(27123.456, 0.1), 27123.5);
        assert_eq!(adjust(3.0, 1.0), 3.0);
        assert_eq!(adjust(0.000_123_7, 0.000_001), 0.000_124);
        assert_eq!(decimals(0.000_000_01), 8);
        assert_eq!(decimals(10.0), 0);
    }

    #[test]
    fn formats_without_exponents() {
        assert_eq!(format(0.000_000_1), "0.0000001");
        assert_eq!(format(27123.5), "27123.5");
        assert_eq!(format(5.0), "5");
        assert_eq!(parse("not a number"), 0.0);
    }

    #[test]
    fn short_decimals_match_the_general_parser() {
        for text in [
            "0",
            "-0",
            "1",
            "1.",
            ".5",
            "-.5",
            "0.1",
            "0.3",
            "27123.456",
            "-1.000000000001",
            "123456789012345",
            "1234567890.12345",
            "999999999999999.",
            "0.000000000000001",
            "0.0000000000000001",
            "1234567890123456",
            "12345678901234567",
        ] {
            let expected = text.parse::<f64>().unwrap();
            let got = parse_decimal(text).unwrap();
            assert_eq!(got.to_bits(), expected.to_bits(), "{text}");
        }
        for text in [
            "", "-", ".", "-.", "1.2.3", "1e", "abc", "NaN", "inf", "1e400",
        ] {
            assert_eq!(parse_decimal(text), None, "{text}");
        }
        assert_eq!(parse_decimal(" 2.5 "), Some(2.5));
        assert_eq!(parse_decimal("1e3"), Some(1000.0));
        assert_eq!(parse_decimal("+4"), Some(4.0));
    }
}
