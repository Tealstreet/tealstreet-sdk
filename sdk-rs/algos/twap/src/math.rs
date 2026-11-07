//! Ports of the TS `safe-math` and lodash helpers TWAP sizing depends on.

/// JS `Math.round`: half rounds toward +inf.
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// `afterDecimal`: digits after the point in the shortest representation.
fn after_decimal(value: f64) -> i32 {
    if value.fract() == 0.0 || !value.is_finite() {
        return 0;
    }
    let text = value.to_string();
    text.find('.')
        .map_or(0, |dot| (text.len() - dot - 1) as i32)
}

fn pow10(exponent: i32) -> f64 {
    10f64.powi(exponent)
}

pub fn add(a: f64, b: f64) -> f64 {
    let factor = pow10(after_decimal(a).max(after_decimal(b)));
    js_round((a + b) * factor) / factor
}

pub fn subtract(a: f64, b: f64) -> f64 {
    let factor = pow10(after_decimal(a).max(after_decimal(b)));
    js_round((a - b) * factor) / factor
}

pub fn multiply(a: f64, b: f64) -> f64 {
    let factor = pow10(after_decimal(a) + after_decimal(b));
    js_round(a * b * factor) / factor
}

/// lodash `random(lower, upper)` over a unit draw in `[0, 1)`: an inclusive
/// integer when both bounds are integers, otherwise a float.
pub fn lodash_random(lower: f64, upper: f64, unit: f64) -> f64 {
    let (lower, upper) = if lower > upper {
        (upper, lower)
    } else {
        (lower, upper)
    };
    if lower.fract() != 0.0 || upper.fract() != 0.0 {
        let digits = unit.to_string().len() as i32 - 1;
        return (lower + unit * (upper - lower + pow10(-digits))).min(upper);
    }
    lower + (unit * (upper - lower + 1.0)).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_math_matches_ts_rounding() {
        assert_eq!(add(0.1, 0.2), 0.3);
        assert_eq!(subtract(0.3, 0.1), 0.2);
        assert_eq!(multiply(0.2, 0.05), 0.01);
        assert_eq!(multiply(7.0, 0.01), 0.07);
    }

    #[test]
    fn lodash_random_integer_bounds_are_inclusive() {
        assert_eq!(lodash_random(0.0, 10.0, 0.0), 0.0);
        assert_eq!(lodash_random(0.0, 10.0, 0.999_999), 10.0);
        assert_eq!(lodash_random(-10.0, 10.0, 0.5), 0.0);
        assert_eq!(lodash_random(10.0, -10.0, 0.0), -10.0);
        assert_eq!(lodash_random(0.0, 0.0, 0.7), 0.0);
    }

    #[test]
    fn lodash_random_float_bounds_stay_in_range() {
        let value = lodash_random(0.0, 2.5, 0.5);
        assert!((1.25..=2.5).contains(&value));
        assert!(lodash_random(0.0, 2.5, 0.999_999_9) <= 2.5);
        assert_eq!(lodash_random(0.0, 2.5, 0.0), 0.0);
    }
}
