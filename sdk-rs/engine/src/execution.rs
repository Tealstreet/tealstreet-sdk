//! Validated financial preparation, separate from native execution identity,
//! display observations and delivery. No progress is consumed by preparation.
use crate::num;

/// Arithmetic is a wire-compatibility policy, not market lot-size rounding.
#[derive(Clone, Copy, Debug)]
pub enum Arithmetic {
    Binary,
    DecimalRounded,
}

impl Arithmetic {
    fn subtract(self, a: f64, b: f64) -> Result<f64, Rejection> {
        finite(match self {
            Self::Binary => a - b,
            Self::DecimalRounded => num::decimal_subtract(a, b),
        })
    }
}

/// Quote values and quantities must describe the same cumulative interval.
#[derive(Clone, Copy, Debug)]
pub enum CumulativePrice {
    Direct(f64),
    Value {
        cumulative: Option<f64>,
        /// A fallback authorized by the native protocol, not an invented VWAP.
        fallback: Option<f64>,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum Report {
    Cumulative {
        quantity: f64,
        price: CumulativePrice,
    },
    /// Last-quantity evidence can only use a direct execution price. A quote
    /// delta may span a different interval and cannot price this amount.
    Last {
        amount: f64,
        cumulative: Option<f64>,
        price: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    InvalidQuantity,
    InvalidValue,
    Unpriced,
    QuantityRegressed,
    ValueRegressed,
    NonFiniteArithmetic,
}

/// A trusted native bootstrap or accepted-delivery anchor. Its quantity does
/// not prove delivery of every preceding interval: native gap policies remain
/// explicit and must use an interval extension rather than reinterpret it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cursor {
    quantity: f64,
    value: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prepared {
    amount: f64,
    price: f64,
    next: Cursor,
}

impl Prepared {
    pub fn amount(self) -> f64 {
        self.amount
    }

    pub fn price(self) -> f64 {
        self.price
    }

    /// Install only after native identity/side/dedup admission and delivery.
    pub fn next(self) -> Cursor {
        self.next
    }
}

impl Cursor {
    pub fn seed(quantity: f64, value: Option<f64>) -> Result<Self, Rejection> {
        nonnegative(quantity, Rejection::InvalidQuantity)?;
        if let Some(value) = value {
            nonnegative(value, Rejection::InvalidValue)?;
        }
        Ok(Self { quantity, value })
    }

    /// Recover a historical prefix without emitting it. Missing quote stays
    /// missing unless the native protocol supplies a cumulative average.
    pub fn seed_with_average(
        quantity: f64,
        value: Option<f64>,
        average: Option<f64>,
    ) -> Result<Self, Rejection> {
        nonnegative(quantity, Rejection::InvalidQuantity)?;
        let value = match value {
            Some(value) => Some(value),
            None => match average {
                Some(average) => {
                    nonnegative(average, Rejection::InvalidValue)?;
                    if average == 0.0 {
                        None
                    } else {
                        Some(finite(average * quantity)?)
                    }
                }
                None => None,
            },
        };
        Self::seed(quantity, value)
    }

    pub fn quantity(self) -> f64 {
        self.quantity
    }

    /// Only for native recovery/bootstrap, never healthy display observations.
    pub fn merge_seed(self, seed: Self) -> Self {
        if seed.quantity > self.quantity {
            seed
        } else if seed.quantity == self.quantity {
            Self {
                value: seed.value.or(self.value),
                ..self
            }
        } else {
            self
        }
    }

    pub fn prepare(
        self,
        report: Report,
        arithmetic: Arithmetic,
    ) -> Result<Option<Prepared>, Rejection> {
        let (amount, price, next) = match report {
            Report::Last {
                amount,
                cumulative,
                price,
            } => {
                nonnegative(amount, Rejection::InvalidQuantity)?;
                if let Some(quantity) = cumulative {
                    nonnegative(quantity, Rejection::InvalidQuantity)?;
                }
                if amount == 0.0 {
                    return Ok(None);
                }
                positive_price(price)?;
                let quantity = match cumulative {
                    Some(quantity) => self.quantity.max(quantity),
                    None => finite(self.quantity + amount)?,
                };
                let value = (quantity == self.quantity).then_some(self.value).flatten();
                (amount, price, Self::seed(quantity, value)?)
            }
            Report::Cumulative { quantity, price } => {
                nonnegative(quantity, Rejection::InvalidQuantity)?;
                if quantity < self.quantity {
                    return Err(Rejection::QuantityRegressed);
                }
                if quantity == self.quantity {
                    return Ok(None);
                }
                let amount = arithmetic.subtract(quantity, self.quantity)?;
                if amount <= 0.0 {
                    return Err(Rejection::InvalidQuantity);
                }
                let (price, value) = match price {
                    CumulativePrice::Direct(price) => (price, None),
                    CumulativePrice::Value {
                        cumulative,
                        fallback,
                    } => {
                        if let Some(value) = cumulative {
                            // Preserve native recovery for a finite negative
                            // regression too; it is still rejected before effects.
                            if value.is_finite()
                                && self.value.is_some_and(|previous| value < previous)
                            {
                                return Err(Rejection::ValueRegressed);
                            }
                            nonnegative(value, Rejection::InvalidValue)?;
                        }
                        if let Some(price) = fallback {
                            positive_price(price)?;
                        }
                        let previous = self.value.or((self.quantity == 0.0).then_some(0.0));
                        let delta = cumulative
                            .zip(previous)
                            .map(|(value, previous)| arithmetic.subtract(value, previous))
                            .transpose()?;
                        let price = match delta.filter(|delta| *delta > 0.0) {
                            Some(delta) => finite(delta / amount)?,
                            None => fallback.ok_or(Rejection::Unpriced)?,
                        };
                        let next = Self::seed_with_average(quantity, cumulative, fallback)?;
                        (price, next.value)
                    }
                };
                positive_price(price)?;
                (amount, price, Self::seed(quantity, value)?)
            }
        };
        Ok(Some(Prepared {
            amount,
            price,
            next,
        }))
    }
}

fn finite(value: f64) -> Result<f64, Rejection> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Rejection::NonFiniteArithmetic)
    }
}

fn nonnegative(value: f64, error: Rejection) -> Result<(), Rejection> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(error)
    }
}

fn positive_price(price: f64) -> Result<(), Rejection> {
    if price.is_finite() && price > 0.0 {
        Ok(())
    } else {
        Err(Rejection::Unpriced)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cumulative(quantity: f64, value: Option<f64>, fallback: Option<f64>) -> Report {
        Report::Cumulative {
            quantity,
            price: CumulativePrice::Value {
                cumulative: value,
                fallback,
            },
        }
    }

    #[test]
    fn unpriced_preparation_cannot_consume_corrected_quantity() {
        let cursor = Cursor::default();
        assert_eq!(
            cursor.prepare(cumulative(0.4, Some(0.0), None), Arithmetic::DecimalRounded),
            Err(Rejection::Unpriced)
        );
        let first = cursor
            .prepare(
                cumulative(0.4, Some(40.0), None),
                Arithmetic::DecimalRounded,
            )
            .unwrap()
            .unwrap();
        assert_eq!((first.price(), first.amount()), (100.0, 0.4));
        let second = first
            .next()
            .prepare(
                cumulative(1.0, Some(106.0), None),
                Arithmetic::DecimalRounded,
            )
            .unwrap()
            .unwrap();
        assert_eq!((second.price(), second.amount()), (110.0, 0.6));
        assert_eq!(
            first
                .next()
                .prepare(cumulative(0.4, Some(40.0), None), Arithmetic::Binary),
            Ok(None)
        );
    }

    #[test]
    fn missing_quote_is_not_a_zero_prefix() {
        let cursor = Cursor::seed(0.4, None).unwrap();
        assert_eq!(
            cursor.prepare(cumulative(1.0, Some(106.0), None), Arithmetic::Binary),
            Err(Rejection::Unpriced)
        );
        let fill = cursor
            .prepare(
                cumulative(1.0, Some(106.0), Some(110.0)),
                Arithmetic::DecimalRounded,
            )
            .unwrap()
            .unwrap();
        assert_eq!((fill.price(), fill.amount()), (110.0, 0.6));
    }

    #[test]
    fn unavailable_average_is_distinct_from_an_explicit_zero_quote() {
        assert_eq!(
            Cursor::seed_with_average(0.4, None, Some(0.0)),
            Cursor::seed(0.4, None)
        );
        assert_eq!(
            Cursor::seed_with_average(0.4, Some(0.0), Some(0.0)),
            Cursor::seed(0.4, Some(0.0))
        );
        assert_ne!(Cursor::seed(0.4, None), Cursor::seed(0.4, Some(0.0)));
        assert_eq!(
            Cursor::seed(0.4, Some(40.0)).unwrap().prepare(
                cumulative(0.5, Some(-1.0), Some(106.0)),
                Arithmetic::DecimalRounded
            ),
            Err(Rejection::ValueRegressed)
        );
    }

    #[test]
    fn direct_late_amount_keeps_the_anchor_and_its_quote_bound_together() {
        let cursor = Cursor::seed(0.8, Some(80.0)).unwrap();
        let late = cursor
            .prepare(
                Report::Last {
                    amount: 0.4,
                    cumulative: Some(0.4),
                    price: 99.0,
                },
                Arithmetic::Binary,
            )
            .unwrap()
            .unwrap();
        assert_eq!(late.amount(), 0.4);
        assert_eq!(late.next(), cursor);
        let higher = late
            .next()
            .prepare(
                Report::Last {
                    amount: 0.2,
                    cumulative: Some(1.0),
                    price: 99.0,
                },
                Arithmetic::Binary,
            )
            .unwrap()
            .unwrap();
        assert_eq!(higher.next(), Cursor::seed(1.0, None).unwrap());
        let unanchored = higher
            .next()
            .prepare(
                Report::Last {
                    amount: 0.25,
                    cumulative: None,
                    price: 99.0,
                },
                Arithmetic::Binary,
            )
            .unwrap()
            .unwrap();
        assert_eq!(unanchored.next().quantity(), 1.25);
    }

    #[test]
    fn bad_seed_and_derived_arithmetic_never_become_empty_history() {
        for bad in [-1.0, f64::NAN, f64::INFINITY] {
            assert!(Cursor::seed(bad, None).is_err());
            assert!(Cursor::seed(1.0, Some(bad)).is_err());
        }
        assert_eq!(
            Cursor::seed_with_average(2.0, None, Some(f64::MAX)),
            Err(Rejection::NonFiniteArithmetic)
        );
        let cursor = Cursor::seed(f64::MAX, None).unwrap();
        assert_eq!(
            cursor.prepare(
                Report::Last {
                    amount: f64::MAX,
                    cumulative: None,
                    price: 1.0
                },
                Arithmetic::Binary
            ),
            Err(Rejection::NonFiniteArithmetic)
        );
        assert_eq!(
            Cursor::default().prepare(
                cumulative(f64::MIN_POSITIVE, Some(f64::MAX), None),
                Arithmetic::Binary
            ),
            Err(Rejection::NonFiniteArithmetic)
        );
        assert_eq!(
            Cursor::default().prepare(
                cumulative(f64::from_bits(1), Some(1.0), None),
                Arithmetic::DecimalRounded
            ),
            Err(Rejection::NonFiniteArithmetic)
        );
    }

    #[test]
    fn regression_rejection_does_not_replace_a_valid_cursor() {
        let cursor = Cursor::seed(0.4, Some(40.0)).unwrap();
        assert_eq!(
            cursor.prepare(
                cumulative(0.5, Some(20.0), Some(100.0)),
                Arithmetic::DecimalRounded
            ),
            Err(Rejection::ValueRegressed)
        );
        assert_eq!(
            cursor.prepare(
                cumulative(0.3, Some(30.0), Some(100.0)),
                Arithmetic::DecimalRounded
            ),
            Err(Rejection::QuantityRegressed)
        );
        assert_eq!(
            cursor.merge_seed(Cursor::seed(0.3, Some(30.0)).unwrap()),
            cursor
        );
        assert_eq!(Cursor::seed(0.4, None).unwrap().merge_seed(cursor), cursor);
    }

    #[test]
    fn price_and_quantity_are_validated_before_native_dedup_can_commit() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                Cursor::default()
                    .prepare(
                        Report::Last {
                            amount: 0.4,
                            cumulative: Some(0.4),
                            price: bad
                        },
                        Arithmetic::Binary
                    )
                    .is_err()
            );
            assert!(
                Cursor::default()
                    .prepare(
                        Report::Cumulative {
                            quantity: 0.4,
                            price: CumulativePrice::Direct(bad)
                        },
                        Arithmetic::Binary
                    )
                    .is_err()
            );
        }
        let binary = Cursor::seed(0.2, None)
            .unwrap()
            .prepare(
                Report::Cumulative {
                    quantity: 0.3,
                    price: CumulativePrice::Direct(99.0),
                },
                Arithmetic::Binary,
            )
            .unwrap()
            .unwrap();
        let decimal = Cursor::seed(0.2, None)
            .unwrap()
            .prepare(
                Report::Cumulative {
                    quantity: 0.3,
                    price: CumulativePrice::Direct(99.0),
                },
                Arithmetic::DecimalRounded,
            )
            .unwrap()
            .unwrap();
        assert_eq!(binary.amount(), 0.3 - 0.2);
        assert_eq!(decimal.amount(), 0.1);
    }
}
