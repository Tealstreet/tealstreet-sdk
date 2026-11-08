use crate::sign;
use safe_cex_engine::{Outcome, VenueError, num};

#[derive(Clone, Copy)]
pub(super) struct Rules {
    pub step: u64,
    pub size_factor: f64,
    pub price_factor: f64,
    pub tick: f64,
    pub min: f64,
    pub max: f64,
    pub min_quote: f64,
}

pub(super) struct NativeWrite<'a> {
    pub orders: &'a [sign::Order],
    pub rules: Rules,
    pub grouped: bool,
    pub full_close: bool,
}

fn refusal() -> VenueError {
    VenueError::new("Lighter native order violates market quantity, price or notional rules")
        .outcome(Outcome::NotSent)
}

pub(super) fn validate_write(write: &NativeWrite<'_>) -> Result<(), VenueError> {
    let rules = write.rules;
    let limit = if write.grouped { 3 } else { 31 };
    if write.orders.is_empty()
        || write.orders.len() > limit
        || (write.grouped && write.orders.len() < 2)
        || rules.step == 0
        || !rules.min.is_finite()
        || rules.min < 0.0
        || !rules.max.is_finite()
        || rules.max <= 0.0
        || !rules.min_quote.is_finite()
        || rules.min_quote < 0.0
        || !rules.tick.is_finite()
        || rules.tick <= 0.0
    {
        return Err(refusal());
    }
    for (index, order) in write.orders.iter().enumerate() {
        let price = f64::from(order.price) / rules.price_factor;
        let trigger = f64::from(order.trigger_price) / rules.price_factor;
        if price <= 0.0
            || num::adjust(price, rules.tick) != price
            || (matches!(order.order_type, 2 | 4)
                && (trigger <= 0.0 || num::adjust(trigger, rules.tick) != trigger))
        {
            return Err(refusal());
        }
        // Native OTO/OTOCO children size themselves from their parent's fill.
        if write.grouped
            && index > 0
            && order.amount == 0
            && order.reduce_only
            && matches!(order.order_type, 2 | 4)
        {
            continue;
        }
        let amount = unscale(order.amount, rules.size_factor).map_err(|_| refusal())?;
        let minimum_exempt = write.full_close && order.reduce_only && order.order_type == 1;
        if order.amount == 0
            || order.amount > (1 << 48) - 1
            || !order.amount.is_multiple_of(rules.step)
            || amount > rules.max
            || (!minimum_exempt
                && (amount < rules.min || !meets_notional(amount, price, rules.min_quote)))
        {
            return Err(refusal());
        }
    }
    Ok(())
}

pub(super) fn unscale(value: u64, factor: f64) -> Result<f64, String> {
    value
        .to_string()
        .parse::<f64>()
        .map(|n| n / factor)
        .map_err(|_| "Invalid Lighter scaled amount".into())
}

pub(super) fn maximum(rounded: u64, maximum: f64, factor: f64) -> Result<u64, String> {
    let maximum = if unscale(rounded, factor)? > maximum {
        rounded.saturating_sub(1)
    } else {
        rounded
    };
    Ok(maximum.min((1 << 48) - 1))
}

pub(super) fn split(amount: u64, max: u64, step: u64) -> Result<Vec<u64>, String> {
    if step == 0 || amount == 0 || !amount.is_multiple_of(step) || max / step == 0 {
        return Err("Invalid Lighter native lot size".into());
    }
    let units = amount / step;
    let count = units.div_ceil(max / step);
    if count > 10_000 {
        return Err("Lighter order would create too many lots".into());
    }
    let base = units / count;
    let extra = units % count;
    Ok((0..count)
        .map(|index| (base + u64::from(index < extra)) * step)
        .collect())
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

    #[test]
    fn balanced_lots_conserve_integer_steps_and_never_cross_maximum() {
        for units in 1..250 {
            for maximum in 1..20 {
                for step in 1..5 {
                    let rows = split(units * step, maximum * step + step - 1, step).unwrap();
                    assert_eq!(rows.iter().sum::<u64>(), units * step);
                    assert!(
                        rows.iter()
                            .all(|n| *n > 0 && n.is_multiple_of(step) && *n <= maximum * step)
                    );
                }
            }
        }
        assert_eq!(maximum(29, 0.29, 100.0).unwrap(), 29);
        assert_eq!(maximum(30, 0.295, 100.0).unwrap(), 29);
        assert_eq!(split(8, 5, 1).unwrap(), [4, 4]);
        assert_eq!(split(17, 5, 1).unwrap(), [5, 4, 4, 4]);
        assert!(split(11, 5, 2).is_err());
        assert!(split(50_001, 5, 1).is_err());
        assert_eq!(split(49_999, 5, 1).unwrap().len(), 10_000);
    }
    fn rules() -> Rules {
        Rules {
            step: 1,
            size_factor: 1.0,
            price_factor: 1.0,
            tick: 1.0,
            min: 3.0,
            max: 5.0,
            min_quote: 400.0,
        }
    }
    fn order() -> sign::Order {
        sign::Order {
            order_id: 1,
            market_id: 1,
            is_buy: true,
            price: 100,
            amount: 4,
            order_type: 1,
            time_in_force: 0,
            reduce_only: false,
            trigger_price: 0,
            order_expiry: None,
        }
    }
    #[test]
    fn native_minima_apply_to_opening_markets_and_partial_reductions() {
        let mut native = order();
        native.amount = 2;
        for (reduce_only, full_close) in [(false, false), (true, false), (false, true)] {
            native.reduce_only = reduce_only;
            assert_eq!(
                validate_write(&NativeWrite {
                    orders: std::slice::from_ref(&native),
                    rules: rules(),
                    grouped: false,
                    full_close
                })
                .unwrap_err()
                .outcome,
                Some(Outcome::NotSent)
            );
        }
        native.reduce_only = true;
        assert!(
            validate_write(&NativeWrite {
                orders: &[native],
                rules: rules(),
                grouped: false,
                full_close: true
            })
            .is_ok()
        );
    }
    #[test]
    fn zero_amount_requires_a_native_grouped_protective_child() {
        let parent = order();
        let mut child = order();
        child.order_type = 2;
        child.reduce_only = true;
        child.trigger_price = 90;
        child.price = 89;
        child.amount = 0;
        assert!(
            validate_write(&NativeWrite {
                orders: std::slice::from_ref(&child),
                rules: rules(),
                grouped: false,
                full_close: false
            })
            .is_err()
        );
        assert!(
            validate_write(&NativeWrite {
                orders: &[parent, child],
                rules: rules(),
                grouped: true,
                full_close: false
            })
            .is_ok()
        );
    }
    #[test]
    fn batch_count_and_rounded_trigger_are_checked() {
        let mut native = order();
        assert!(
            validate_write(&NativeWrite {
                orders: &vec![native.clone(); 32],
                rules: rules(),
                grouped: false,
                full_close: false
            })
            .is_err()
        );
        native.order_type = 2;
        native.trigger_price = 0;
        assert!(
            validate_write(&NativeWrite {
                orders: &[native],
                rules: rules(),
                grouped: false,
                full_close: false
            })
            .is_err()
        );
    }
}
