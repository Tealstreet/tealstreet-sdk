use super::Placement;
use safe_cex_engine::{Outcome, VenueError, num};

pub(super) struct Rules {
    pub step: f64,
    pub tick: f64,
    pub min: f64,
    pub max: f64,
    pub min_notional: f64,
    pub mark: f64,
    pub long: f64,
    pub short: f64,
}

fn refusal() -> VenueError {
    VenueError::new("Paradex native order violates market quantity, price or notional rules")
        .outcome(Outcome::NotSent)
}

pub(super) fn validate_write(rows: &[Placement]) -> Result<(), VenueError> {
    if rows.is_empty() || rows.len() > 10 {
        return Err(refusal());
    }
    for row in rows {
        validate_values(
            &row.rules,
            row.side,
            &row.size,
            row.price.as_deref(),
            row.trigger_price.as_deref(),
            row.flags.is_some(),
        )?;
    }
    Ok(())
}

pub(super) fn validate_values(
    rules: &Rules,
    side: &str,
    size: &str,
    price: Option<&str>,
    trigger: Option<&str>,
    reduce_only: bool,
) -> Result<(), VenueError> {
    let size = num::parse_decimal(size).ok_or_else(refusal)?;
    if !rules.step.is_finite()
        || rules.step <= 0.0
        || !rules.tick.is_finite()
        || rules.tick <= 0.0
        || !rules.min.is_finite()
        || rules.min < 0.0
        || !rules.max.is_finite()
        || rules.max < 0.0
        || !rules.min_notional.is_finite()
        || rules.min_notional < 0.0
        || size <= 0.0
        || size < rules.min
        || (rules.max > 0.0 && size > rules.max)
        || num::adjust(size, rules.step) != size
    {
        return Err(refusal());
    }
    for value in price.into_iter().chain(trigger) {
        let value = num::parse_decimal(value).ok_or_else(refusal)?;
        if value <= 0.0 || num::adjust(value, rules.tick) != value {
            return Err(refusal());
        }
    }
    // Paradex values minimum notional at mark; only a full position close is exempt.
    let full_close = reduce_only
        && size
            == if side == "SELL" {
                rules.long
            } else {
                rules.short
            };
    if rules.min_notional > 0.0
        && !full_close
        && (!rules.mark.is_finite()
            || rules.mark <= 0.0
            || !meets_notional(size, rules.mark, rules.min_notional))
    {
        return Err(refusal());
    }
    Ok(())
}

// Compare decimal wire values without rounding a product across the minimum.
fn meets_notional(size: f64, price: f64, minimum: f64) -> bool {
    fn parts(value: f64) -> Option<(u128, i32)> {
        let text = value.to_string();
        let (digits, exponent) = match text.split_once('e') {
            Some((digits, exponent)) => (digits, exponent.parse::<i32>().ok()?),
            None => (text.as_str(), 0),
        };
        let fraction = digits
            .split_once('.')
            .map_or(0, |(_, fraction)| fraction.len());
        let digits = digits.replace('.', "");
        let coefficient = digits.trim_end_matches('0');
        if coefficient.is_empty() {
            return Some((0, 0));
        }
        Some((
            coefficient.parse().ok()?,
            exponent - i32::try_from(fraction).ok()?
                + i32::try_from(digits.len() - coefficient.len()).ok()?,
        ))
    }
    if minimum == 0.0 {
        return true;
    }
    let Some((left, right)) = (|| {
        let (size, size_power) = parts(size)?;
        let (price, price_power) = parts(price)?;
        let (minimum, minimum_power) = parts(minimum)?;
        Some((
            (
                size.checked_mul(price)?.to_string(),
                size_power + price_power,
            ),
            (minimum.to_string(), minimum_power),
        ))
    })() else {
        return false;
    };
    let Ok(left_digits) = i32::try_from(left.0.len()) else {
        return false;
    };
    let Ok(right_digits) = i32::try_from(right.0.len()) else {
        return false;
    };
    match (left_digits + left.1).cmp(&(right_digits + right.1)) {
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Equal => {
            let width = left.0.len().max(right.0.len());
            left.0
                .bytes()
                .chain(std::iter::repeat(b'0'))
                .take(width)
                .cmp(right.0.bytes().chain(std::iter::repeat(b'0')).take(width))
                != std::cmp::Ordering::Less
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_decimal_notional_boundary_is_inclusive() {
        assert!(meets_notional(0.7, 0.1, 0.07));
        assert!(!meets_notional(0.69, 0.1, 0.07));
        assert!(!meets_notional(1.0, 1.0, 1.0000000000000002));
        assert!(meets_notional(1.0, 1.0, 1.0));
        assert!(meets_notional(1e-9, 1e-9, 1e-18));
        assert!(meets_notional(1e100, 1e100, 1e200));
    }

    fn rules() -> Rules {
        Rules {
            step: 0.01,
            tick: 0.1,
            min: 0.01,
            max: 5.0,
            min_notional: 10.0,
            mark: 100.0,
            long: 0.01,
            short: 0.0,
        }
    }
    #[test]
    fn minimum_notional_uses_mark_and_only_exempts_an_exact_full_close() {
        let r = rules();
        assert!(validate_values(&r, "SELL", "0.01", None, None, true).is_ok());
        assert!(validate_values(&r, "SELL", "0.02", None, None, true).is_err());
        assert!(validate_values(&r, "BUY", "0.01", None, None, true).is_err());
        assert!(validate_values(&r, "SELL", "0.01", Some("10000"), None, false).is_err());
        assert!(validate_values(&r, "BUY", "0.1", Some("1"), None, false).is_ok());
    }
    #[test]
    fn exact_wire_values_must_be_positive_and_on_grid() {
        for (size, price, trigger) in [
            ("0", Some("100"), None),
            ("1.005", Some("100"), None),
            ("6", None, None),
            ("1", Some("100.05"), None),
            ("1", None, Some("90.05")),
            ("NaN", None, None),
        ] {
            let e = validate_values(&rules(), "BUY", size, price, trigger, false).unwrap_err();
            assert_eq!(e.outcome, Some(Outcome::NotSent));
        }
    }
}
