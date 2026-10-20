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
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
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
}
