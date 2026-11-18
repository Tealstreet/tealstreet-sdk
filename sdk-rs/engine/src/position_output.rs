//! SDK position output semantics. Internal venue numbers remain available for
//! native risk decisions; every public output uses the same display policy.
use crate::model::{Market, Position};
use serde::{Serialize, Serializer};

pub(crate) fn display(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

pub(crate) fn liquidation(value: f64) -> Option<f64> {
    (value.is_finite() && value > 0.0).then_some(value)
}

pub(crate) fn serialize_display<S: Serializer>(value: &f64, s: S) -> Result<S::Ok, S::Error> {
    display(*value).serialize(s)
}

pub(crate) fn optional_absent(value: &Option<f64>) -> bool {
    value.is_none_or(|v| !v.is_finite())
}

pub(crate) fn serialize_liquidation<S: Serializer>(value: &f64, s: S) -> Result<S::Ok, S::Error> {
    liquidation(*value).serialize(s)
}

fn divide(a: f64, b: f64) -> f64 {
    let digits = crate::num::decimals(a).max(crate::num::decimals(b)).max(8);
    let factor = 10f64.powi(digits as i32);
    crate::num::round_half_up((a / b) * factor) / factor
}

/// The final two revision-2 columns. Presence bits distinguish absent metadata
/// from an invalid native size, which must never enable a base-unit fallback.
pub(crate) fn native_extras(
    position: &Position,
    market: Option<&Market>,
    units: u32,
) -> [Option<f64>; 2] {
    let Some(market) = market else {
        return [None; 2];
    };
    let inverse = market.is_inverse == Some(true);
    let size = market.contract_size.filter(|v| *v > 0.0).unwrap_or(1.0);
    match units {
        1 | 3 => [
            Some(divide(
                if inverse {
                    position.notional
                } else {
                    position.contracts
                },
                size,
            )),
            None,
        ],
        2 => [
            Some(position.notional),
            Some(display(position.unrealized_pnl)),
        ],
        4 if inverse => [
            Some(divide(position.notional, size)),
            Some(display(position.unrealized_pnl)),
        ],
        _ if inverse => [
            Some(position.notional),
            Some(display(position.unrealized_pnl)),
        ],
        _ => [None; 2],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_liquidation_is_null_and_positive_is_unchanged() {
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(liquidation(value), None);
            assert_eq!(serde_json::to_string(&liquidation(value)).unwrap(), "null");
        }
        assert_eq!(liquidation(123.5), Some(123.5));
    }
    #[test]
    fn native_lot_division_has_no_binary_rounding_residue() {
        assert_eq!(divide(0.3, 0.1), 3.0);
        assert_eq!(display(f64::NAN), 0.0);
    }
}
