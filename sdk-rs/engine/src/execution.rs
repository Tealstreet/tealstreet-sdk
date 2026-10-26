//! Validated financial preparation, separate from native execution identity,
//! display observations and delivery. No progress is consumed by preparation.
use crate::num;

/// Arithmetic is a wire-compatibility policy, not market lot-size rounding.
#[derive(Clone, Copy, Debug)]
pub enum Arithmetic {
    Binary,
    DecimalRounded,
    DecimalCapped20,
}

impl Arithmetic {
    fn subtract(self, a: f64, b: f64) -> Result<f64, Rejection> {
        finite(match self {
            Self::Binary => a - b,
            Self::DecimalRounded => num::decimal_subtract(a, b),
            Self::DecimalCapped20 => num::decimal_subtract_capped20(a, b),
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

/// A native observation may precede identity admission; inferred quote evidence
/// belongs only to the separately admitted delivery's `next` cursor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedObservation {
    observation: Option<Cursor>,
    delivery: Option<Prepared>,
}

impl PreparedObservation {
    pub fn observation(self) -> Option<Cursor> {
        self.observation
    }

    pub fn delivery(self) -> Option<Prepared> {
        self.delivery
    }
}

impl Prepared {
    /// Scale only the public amount; cumulative anchors remain in native units.
    pub fn scaled(self, scale: f64, rounding: ExecutionRounding) -> Result<Self, Rejection> {
        let amount = scaled_quantity(self.amount, scale, rounding)?;
        let price = rounding.apply(self.price)?;
        if amount <= 0.0 {
            return Err(Rejection::InvalidQuantity);
        }
        positive_price(price)?;
        Ok(Self {
            amount,
            price,
            ..self
        })
    }

    /// Round only the public execution. Its cumulative anchor retains the
    /// native unrounded quantity and quote for the next report.
    pub fn rounded(self, rounding: ExecutionRounding) -> Result<Self, Rejection> {
        let amount = rounding.apply(self.amount)?;
        let price = rounding.apply(self.price)?;
        if amount <= 0.0 {
            return Err(Rejection::InvalidQuantity);
        }
        positive_price(price)?;
        Ok(Self {
            amount,
            price,
            ..self
        })
    }

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
    /// Native observed-average policy: last quantity wins by presence, and a
    /// missing direct price uses the legacy quote-delta/selected-amount quotient.
    /// This is deliberately separate from the strict direct-price `Report::Last`.
    /// Whole-interval quote inference currently requires Binary arithmetic.
    /// Commit the observation before native dedup. After a unique Fill, only
    /// enrich a still-live same-endpoint cursor from `delivery.next()`.
    pub fn prepare_observed_average(
        self,
        quantity: f64,
        last: Option<f64>,
        average: Option<f64>,
        direct: Option<f64>,
        arithmetic: Arithmetic,
    ) -> Result<PreparedObservation, Rejection> {
        nonnegative(quantity, Rejection::InvalidQuantity)?;
        if let Some(last) = last {
            nonnegative(last, Rejection::InvalidQuantity)?;
        }
        if let Some(average) = average {
            nonnegative(average, Rejection::InvalidValue)?;
        }
        if let Some(direct) = direct {
            nonnegative(direct, Rejection::InvalidValue)?;
        }
        // Validate supplied product even for quiet/regressed/direct-priced
        // reports. An absent or zero average is not trusted historical quote.
        let value = finite(average.unwrap_or(0.0) * quantity)?;
        let growth = arithmetic.subtract(quantity, self.quantity)?.max(0.0);
        let amount = last.unwrap_or(growth);
        let observation = (quantity > self.quantity)
            .then(|| Self::seed(quantity, average.filter(|n| *n > 0.0).map(|_| value)))
            .transpose()?;
        if amount == 0.0 {
            return Ok(PreparedObservation {
                observation,
                delivery: None,
            });
        }
        let price = match direct {
            Some(price) => price,
            None => finite(arithmetic.subtract(value, self.value.unwrap_or(0.0))? / amount)?,
        };
        positive_price(price)?;
        let mut next = observation.unwrap_or(self);
        if observation.is_some()
            && next.value.is_none()
            && direct.is_some()
            && matches!(arithmetic, Arithmetic::Binary)
            && amount == growth
        {
            // A priced last event proves the whole new interval only when it
            // exactly covers growth. Overflow/unavailable prefix must not turn
            // a valid direct execution into a rejected execution.
            if let Ok(inferred) = self.direct_suffix(quantity, price) {
                next = inferred;
            }
        }
        Ok(PreparedObservation {
            observation,
            delivery: Some(Prepared {
                amount,
                price,
                next,
            }),
        })
    }

    /// Admit an observation without deciding whether it represents a live Fill.
    /// Missing historical quote and native lifecycle decisions remain explicit.
    pub fn prepare_prefix(
        previous: Option<Self>,
        quantity: f64,
        value: Option<f64>,
        arithmetic: Arithmetic,
    ) -> Result<Option<Self>, Rejection> {
        let next = Self::seed(quantity, value)?;
        if quantity > 0.0 && value == Some(0.0) {
            return Err(Rejection::InvalidValue);
        }
        if let Some(previous) = previous {
            if quantity <= previous.quantity {
                return Ok((quantity == previous.quantity
                    && previous.value.is_none()
                    && value.is_some())
                .then_some(next));
            }
            if let Some(previous_value) = previous.value {
                let value = value.ok_or(Rejection::Unpriced)?;
                if arithmetic.subtract(value, previous_value)? <= 0.0 {
                    return Err(Rejection::ValueRegressed);
                }
            }
        }
        Ok(Some(next))
    }

    /// Bind an average to its cumulative quantity, or extend the historical
    /// quote with a native-authorized direct price for the new suffix only.
    pub fn prepare_average(
        self,
        quantity: f64,
        average: Option<f64>,
        fallback: Option<f64>,
        arithmetic: Arithmetic,
    ) -> Result<Option<Prepared>, Rejection> {
        nonnegative(quantity, Rejection::InvalidQuantity)?;
        if quantity <= self.quantity {
            return self.prepare(
                Report::Cumulative {
                    quantity,
                    price: CumulativePrice::Value {
                        cumulative: None,
                        fallback: None,
                    },
                },
                arithmetic,
            );
        }
        let value = match average {
            Some(average) => {
                nonnegative(average, Rejection::InvalidValue)?;
                finite(average * quantity)?
            }
            None => {
                let price = fallback.ok_or(Rejection::Unpriced)?;
                positive_price(price)?;
                let previous = self
                    .value
                    .or((self.quantity == 0.0).then_some(0.0))
                    .ok_or(Rejection::Unpriced)?;
                let delta = arithmetic.subtract(quantity, self.quantity)?;
                finite(previous + finite(delta * price)?)?
            }
        };
        self.prepare(
            Report::Cumulative {
                quantity,
                price: CumulativePrice::Value {
                    cumulative: Some(value),
                    fallback: None,
                },
            },
            arithmetic,
        )
    }

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

    pub fn value(self) -> Option<f64> {
        self.value
    }

    /// A validated correction may fill missing quote evidence at an already
    /// admitted endpoint, without moving quantity or replacing a known value.
    pub fn enrich_quote(self, observation: Self) -> Option<Self> {
        (self.quantity == observation.quantity
            && self.value.is_none()
            && observation.value.is_some())
        .then_some(observation)
    }

    /// Extend a known quote anchor using a direct price for this exact suffix.
    pub fn direct_suffix(self, quantity: f64, price: f64) -> Result<Self, Rejection> {
        let interval = QuantityInterval::new(self.quantity, quantity)?;
        positive_price(price)?;
        let previous = self
            .value
            .or((self.quantity == 0.0).then_some(0.0))
            .ok_or(Rejection::Unpriced)?;
        let value = finite(previous + finite(interval.length() * price)?)?;
        Self::seed(quantity, Some(value))
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

/// Display/REST observations do not establish which executions were delivered.
#[derive(Clone, Copy, Debug)]
pub struct ObservedProgress {
    previous: f64,
    cumulative: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedObserved {
    delivery: Prepared,
    observed: f64,
    position_amount: f64,
}

impl PreparedObserved {
    pub fn delivery(self) -> Prepared {
        self.delivery
    }

    pub fn observed(self) -> f64 {
        self.observed
    }

    pub fn position_amount(self) -> f64 {
        self.position_amount
    }
}

impl ObservedProgress {
    pub fn new(previous: f64, cumulative: Option<f64>) -> Result<Self, Rejection> {
        nonnegative(previous, Rejection::InvalidQuantity)?;
        if let Some(quantity) = cumulative {
            nonnegative(quantity, Rejection::InvalidQuantity)?;
        }
        Ok(Self {
            previous,
            cumulative,
        })
    }

    pub fn quantity(self) -> f64 {
        self.previous.max(self.cumulative.unwrap_or(self.previous))
    }

    pub fn delivery_start(self, delivered: f64) -> Result<f64, Rejection> {
        nonnegative(delivered, Rejection::InvalidQuantity)?;
        Ok(if self.cumulative.is_some_and(|n| n > self.previous) {
            self.previous.max(delivered)
        } else {
            delivered
        })
    }

    /// A native last quantity prices only the newly observed suffix. Unlike
    /// authoritative last evidence, it cannot replay quantity already observed.
    /// Accepted None still permits the caller's quiet identity bookkeeping.
    pub fn prepare_capped(
        self,
        last: Option<f64>,
        price: f64,
    ) -> Result<Option<PreparedObserved>, Rejection> {
        if let Some(amount) = last {
            nonnegative(amount, Rejection::InvalidQuantity)?;
        }
        let amount = match self.cumulative {
            Some(quantity) => {
                let growth = (quantity - self.previous).max(0.0);
                last.map_or(growth, |amount| amount.min(growth))
            }
            None => last.unwrap_or(0.0),
        };
        self.prepare(Cursor::seed(self.previous, None)?, Some(amount), price)
    }

    /// Native identity admission decides whether last quantity is authoritative.
    /// Missing last quantity uses only the proven observed/delivered suffix.
    pub fn prepare(
        self,
        delivered: Cursor,
        last: Option<f64>,
        price: f64,
    ) -> Result<Option<PreparedObserved>, Rejection> {
        let amount = match last {
            Some(amount) => {
                nonnegative(amount, Rejection::InvalidQuantity)?;
                amount
            }
            None => match self.cumulative {
                Some(quantity) => (quantity - self.delivery_start(delivered.quantity)?).max(0.0),
                None => return Ok(None),
            },
        };
        let Some(delivery) = delivered.prepare(
            Report::Last {
                amount,
                cumulative: self.cumulative,
                price,
            },
            Arithmetic::Binary,
        )?
        else {
            return Ok(None);
        };
        let observed = match self.cumulative {
            Some(_) => self.quantity(),
            None => finite(self.previous + amount)?,
        };
        let position_amount = self.cumulative.map_or(amount, |quantity| {
            amount.min((quantity - self.previous).max(0.0))
        });
        Ok(Some(PreparedObserved {
            delivery,
            observed,
            position_amount,
        }))
    }
}

/// Exact native financial output policy, independent of market lot sizes.
#[derive(Clone, Copy, Debug)]
pub enum ExecutionRounding {
    Binary,
    Significant15,
    ScaledSignificant15,
}

impl ExecutionRounding {
    fn apply(self, value: f64) -> Result<f64, Rejection> {
        finite(value)?;
        finite(match self {
            Self::Binary => value,
            Self::Significant15 => num::significant15(value),
            Self::ScaledSignificant15 => num::significant15_scaled(value),
        })
    }
}

pub fn scaled_quantity(
    quantity: f64,
    scale: f64,
    rounding: ExecutionRounding,
) -> Result<f64, Rejection> {
    nonnegative(quantity, Rejection::InvalidQuantity)?;
    if !scale.is_finite() || scale <= 0.0 {
        return Err(Rejection::InvalidQuantity);
    }
    rounding.apply(finite(quantity * scale)?)
}

#[derive(Clone, Copy, Debug)]
pub struct QuantityInterval {
    start: f64,
    end: f64,
}

impl QuantityInterval {
    pub fn new(start: f64, end: f64) -> Result<Self, Rejection> {
        nonnegative(start, Rejection::InvalidQuantity)?;
        nonnegative(end, Rejection::InvalidQuantity)?;
        if end <= start {
            return Err(Rejection::InvalidQuantity);
        }
        finite(end - start)?;
        Ok(Self { start, end })
    }

    pub fn suffix(end: f64, last: f64, rounding: ExecutionRounding) -> Result<Self, Rejection> {
        nonnegative(end, Rejection::InvalidQuantity)?;
        if !last.is_finite() || last <= 0.0 || last > end {
            return Err(Rejection::InvalidQuantity);
        }
        Self::new(rounding.apply(end - last)?, end)
    }

    pub fn length(self) -> f64 {
        self.end - self.start
    }
}

/// Uniform prices may price clipped quantities; endpoint VWAP may only price
/// its complete interval. Both variants contain validated positive prices.
#[derive(Clone, Copy, Debug)]
pub struct PricedInterval {
    interval: QuantityInterval,
    price: f64,
    uniform: bool,
    rounding: ExecutionRounding,
}

impl PricedInterval {
    pub fn uniform(
        interval: QuantityInterval,
        price: f64,
        rounding: ExecutionRounding,
    ) -> Result<Self, Rejection> {
        positive_price(price)?;
        let price = rounding.apply(price)?;
        positive_price(price)?;
        Ok(Self {
            interval,
            price,
            uniform: true,
            rounding,
        })
    }

    pub fn quotes(
        from: Cursor,
        to: Cursor,
        rounding: ExecutionRounding,
    ) -> Result<Self, Rejection> {
        let interval = QuantityInterval::new(from.quantity, to.quantity)?;
        let previous = from
            .value
            .or((from.quantity == 0.0).then_some(0.0))
            .ok_or(Rejection::Unpriced)?;
        let value = to.value.ok_or(Rejection::Unpriced)?;
        if value < previous {
            return Err(Rejection::ValueRegressed);
        }
        let price = rounding.apply(finite(finite(value - previous)? / interval.length())?)?;
        positive_price(price)?;
        Ok(Self {
            interval,
            price,
            uniform: false,
            rounding,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntervalDelivery {
    price: f64,
    amount: f64,
}

impl IntervalDelivery {
    pub fn price(self) -> f64 {
        self.price
    }

    pub fn amount(self) -> f64 {
        self.amount
    }
}

/// Bounded delivered-interval union. The floor suppresses retired/bootstrap
/// quantities; it does not claim that every preceding execution was delivered.
#[derive(Debug)]
pub struct IntervalHistory {
    floor: f64,
    limit: usize,
    intervals: Vec<(f64, f64)>,
}

/// Exclusive state access lasts until commit or drop, so a proposal cannot
/// become stale or be reused. Preparation performs no mutation/allocation.
#[derive(Debug)]
pub struct PreparedInterval<'a> {
    history: &'a mut IntervalHistory,
    interval: Option<QuantityInterval>,
    delivery: Option<IntervalDelivery>,
}

impl IntervalHistory {
    pub fn new(floor: f64, limit: usize) -> Result<Self, Rejection> {
        nonnegative(floor, Rejection::InvalidQuantity)?;
        if limit == 0 {
            return Err(Rejection::InvalidQuantity);
        }
        Ok(Self {
            floor,
            limit,
            intervals: Vec::new(),
        })
    }

    pub fn prepare(&mut self, proof: PricedInterval) -> Result<PreparedInterval<'_>, Rejection> {
        let start = proof.interval.start.max(self.floor);
        let end = proof.interval.end;
        let (interval, delivery) = if end <= start {
            (None, None)
        } else {
            let mut amount = end - start;
            for &(from, to) in &self.intervals {
                amount = finite(amount - (end.min(to) - start.max(from)).max(0.0))?;
            }
            amount = amount.max(0.0);
            // Quantity coverage alone cannot price an arbitrary VWAP subset.
            if !proof.uniform
                && amount > 0.0
                && (start != proof.interval.start || amount != proof.interval.length())
            {
                return Err(Rejection::Unpriced);
            }
            let amount = proof.rounding.apply(amount)?;
            let delivery = (amount > 0.0).then_some(IntervalDelivery {
                price: proof.price,
                amount,
            });
            (Some(QuantityInterval { start, end }), delivery)
        };
        Ok(PreparedInterval {
            history: self,
            interval,
            delivery,
        })
    }
}

impl PreparedInterval<'_> {
    /// Native admission has completed. Commit before emitting the returned Fill.
    pub fn commit(self) -> Option<IntervalDelivery> {
        if let Some(interval) = self.interval {
            let intervals = &mut self.history.intervals;
            intervals.push((interval.start, interval.end));
            intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged = 0;
            for index in 0..intervals.len() {
                let interval = intervals[index];
                if merged > 0 && interval.0 <= intervals[merged - 1].1 {
                    intervals[merged - 1].1 = intervals[merged - 1].1.max(interval.1);
                } else {
                    intervals[merged] = interval;
                    merged += 1;
                }
            }
            intervals.truncate(merged);
            if intervals.len() > self.history.limit {
                self.history.floor = intervals.remove(0).1;
            }
        }
        self.delivery
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

    #[test]
    fn observed_average_separates_observation_from_unique_delivery_quote() {
        for average in [None, Some(0.0)] {
            let prepared = Cursor::default()
                .prepare_observed_average(1.0, Some(1.0), average, Some(100.0), Arithmetic::Binary)
                .unwrap();
            let observed = prepared.observation().unwrap();
            assert_eq!(observed, Cursor::seed(1.0, None).unwrap());
            let fill = prepared.delivery().unwrap();
            assert_eq!((fill.price(), fill.amount()), (100.0, 1.0));
            let unique = observed.enrich_quote(fill.next()).unwrap();
            assert_eq!(unique.value(), Some(100.0));
            let follow = unique
                .prepare_observed_average(2.0, None, Some(100.0), None, Arithmetic::Binary)
                .unwrap()
                .delivery()
                .unwrap();
            assert_eq!((follow.price(), follow.amount()), (100.0, 1.0));
            // A duplicate may install the observation but cannot install next.
            let duplicate_follow = observed
                .prepare_observed_average(2.0, None, Some(100.0), None, Arithmetic::Binary)
                .unwrap()
                .delivery()
                .unwrap();
            assert_eq!(duplicate_follow.price(), 200.0);
        }
    }

    #[test]
    fn observed_average_requires_whole_growth_and_known_prefix_for_inference() {
        for (cursor, quantity, last) in [
            (Cursor::default(), 10.0, 1.0),
            (Cursor::seed(1.0, None).unwrap(), 2.0, 1.0),
            (Cursor::seed(0.2, Some(20.0)).unwrap(), 0.3, 0.1),
        ] {
            let result = cursor
                .prepare_observed_average(
                    quantity,
                    Some(last),
                    None,
                    Some(100.0),
                    Arithmetic::Binary,
                )
                .unwrap();
            assert_eq!(result.observation().unwrap().value(), None);
            assert_eq!(result.delivery().unwrap().next().value(), None);
        }
        let quiet = Cursor::default()
            .prepare_observed_average(1.0, Some(0.0), None, Some(100.0), Arithmetic::Binary)
            .unwrap();
        assert!(quiet.delivery().is_none());
        assert_eq!(quiet.observation().unwrap().value(), None);
    }

    #[test]
    fn observed_average_does_not_rewrite_existing_or_regressed_quote() {
        let previous = Cursor::seed(2.0, Some(200.0)).unwrap();
        for quantity in [0.0, 1.0, 2.0] {
            let result = previous
                .prepare_observed_average(
                    quantity,
                    Some(0.25),
                    Some(1.0),
                    Some(90.0),
                    Arithmetic::Binary,
                )
                .unwrap();
            assert!(result.observation().is_none());
            let fill = result.delivery().unwrap();
            assert_eq!(
                (fill.price(), fill.amount(), fill.next()),
                (90.0, 0.25, previous)
            );
        }
        let average = previous
            .prepare_observed_average(3.0, Some(1.0), Some(1.0), Some(90.0), Arithmetic::Binary)
            .unwrap();
        // Direct price wins even when an unused average quote regresses.
        assert_eq!(average.observation().unwrap().value(), Some(3.0));
        assert_eq!(average.delivery().unwrap().next().value(), Some(3.0));
        assert!(
            previous
                .enrich_quote(Cursor::seed(2.0, Some(999.0)).unwrap())
                .is_none()
        );
        assert!(
            Cursor::seed(2.0, None)
                .unwrap()
                .enrich_quote(Cursor::seed(3.0, Some(300.0)).unwrap())
                .is_none()
        );
    }

    #[test]
    fn observed_average_preserves_arbitrary_last_and_zero_presence_policies() {
        let previous = Cursor::seed(1.0, Some(100.0)).unwrap();
        let fill = previous
            .prepare_observed_average(2.0, Some(0.25), Some(110.0), None, Arithmetic::Binary)
            .unwrap()
            .delivery()
            .unwrap();
        assert_eq!((fill.amount(), fill.price()), (0.25, 480.0));
        let quiet = previous
            .prepare_observed_average(2.0, Some(0.0), Some(110.0), Some(0.0), Arithmetic::Binary)
            .unwrap();
        assert!(quiet.delivery().is_none());
        assert_eq!(quiet.observation().unwrap().value(), Some(220.0));
        assert!(
            previous
                .prepare_observed_average(
                    2.0,
                    Some(1.0),
                    Some(110.0),
                    Some(0.0),
                    Arithmetic::Binary
                )
                .is_err()
        );
    }

    #[test]
    fn observed_average_rejects_supplied_invalid_values_even_when_unused() {
        for quantity in [2.0, 3.0, 4.0] {
            for last in [0.0, 1.0] {
                assert!(
                    Cursor::seed(3.0, Some(300.0))
                        .unwrap()
                        .prepare_observed_average(
                            quantity,
                            Some(last),
                            Some(f64::MAX),
                            Some(100.0),
                            Arithmetic::Binary
                        )
                        .is_err()
                );
            }
        }
        for bad in [-1.0, f64::NAN, f64::INFINITY] {
            for (quantity, last, average, direct) in [
                (bad, Some(0.0), None, None),
                (1.0, Some(bad), None, Some(100.0)),
                (1.0, Some(0.0), Some(bad), Some(100.0)),
                (1.0, Some(0.0), None, Some(bad)),
            ] {
                assert!(
                    Cursor::default()
                        .prepare_observed_average(
                            quantity,
                            last,
                            average,
                            direct,
                            Arithmetic::Binary
                        )
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn observed_average_inferred_overflow_is_best_effort_only() {
        let previous = Cursor::seed(1.0, Some(f64::MAX)).unwrap();
        let result = previous
            .prepare_observed_average(2.0, Some(1.0), None, Some(f64::MAX), Arithmetic::Binary)
            .unwrap();
        let fill = result.delivery().unwrap();
        assert_eq!(
            (fill.amount(), fill.price(), fill.next().value()),
            (1.0, f64::MAX, None)
        );
        assert!(
            previous
                .prepare_observed_average(
                    2.0,
                    Some(0.0),
                    Some(f64::MAX),
                    Some(100.0),
                    Arithmetic::Binary
                )
                .is_err()
        );
    }

    #[test]
    fn observed_average_nonbinary_delivery_does_not_infer_binary_quote() {
        for arithmetic in [Arithmetic::DecimalRounded, Arithmetic::DecimalCapped20] {
            let previous = Cursor::seed(0.2, Some(20.0)).unwrap();
            let result = previous
                .prepare_observed_average(0.3, None, None, Some(100.0), arithmetic)
                .unwrap();
            let fill = result.delivery().unwrap();
            assert_eq!(fill.amount(), arithmetic.subtract(0.3, 0.2).unwrap());
            assert_eq!(fill.price(), 100.0);
            assert_eq!(fill.next().value(), None);
        }
    }

    fn uniform(start: f64, end: f64, price: f64) -> PricedInterval {
        PricedInterval::uniform(
            QuantityInterval::new(start, end).unwrap(),
            price,
            ExecutionRounding::Significant15,
        )
        .unwrap()
    }

    fn interval_fill(history: &mut IntervalHistory, start: f64, end: f64) -> Option<(f64, f64)> {
        history
            .prepare(uniform(start, end, 100.0))
            .unwrap()
            .commit()
            .map(|delivery| (delivery.price(), delivery.amount()))
    }

    #[test]
    fn capped_last_preserves_full_observation_and_native_suffix() {
        for (last, amount) in [(Some(0.25), 0.25), (Some(4.0), 1.0), (None, 1.0)] {
            let prepared = ObservedProgress::new(2.0, Some(3.0))
                .unwrap()
                .prepare_capped(last, 100.0)
                .unwrap()
                .unwrap();
            assert_eq!(prepared.delivery().amount(), amount);
            assert_eq!(prepared.delivery().price(), 100.0);
            assert_eq!(prepared.observed(), 3.0);
            assert_eq!(prepared.delivery().next().quantity(), 3.0);
        }
        let gap = ObservedProgress::new(0.0, Some(10.0))
            .unwrap()
            .prepare_capped(Some(0.5), 100.0)
            .unwrap()
            .unwrap();
        assert_eq!(gap.delivery().amount(), 0.5);
        assert_eq!(gap.observed(), 10.0);
    }

    #[test]
    fn capped_last_quiet_observations_do_not_require_unused_price() {
        for (previous, cumulative, last) in [
            (2.0, Some(2.0), Some(0.5)),
            (2.0, Some(1.0), Some(0.5)),
            (2.0, Some(3.0), Some(0.0)),
            (2.0, None, None),
        ] {
            let progress = ObservedProgress::new(previous, cumulative).unwrap();
            assert_eq!(progress.prepare_capped(last, 0.0), Ok(None));
            assert_eq!(
                progress.quantity(),
                previous.max(cumulative.unwrap_or(previous))
            );
        }
        assert_eq!(
            ObservedProgress::new(2.0, Some(2.0))
                .unwrap()
                .prepare_capped(Some(-1.0), 100.0),
            Err(Rejection::InvalidQuantity)
        );
    }

    #[test]
    fn capped_last_validates_before_progress_and_checks_unanchored_addition() {
        let progress = ObservedProgress::new(0.25, Some(0.75)).unwrap();
        for price in [0.0, -1.0, f64::INFINITY, f64::NAN] {
            assert_eq!(
                progress.prepare_capped(Some(0.5), price),
                Err(Rejection::Unpriced)
            );
        }
        let corrected = progress.prepare_capped(Some(0.5), 100.0).unwrap().unwrap();
        assert_eq!(corrected.delivery().amount(), 0.5);
        assert_eq!(corrected.observed(), 0.75);
        assert_eq!(
            ObservedProgress::new(f64::MAX, None)
                .unwrap()
                .prepare_capped(Some(f64::MAX), 100.0),
            Err(Rejection::NonFiniteArithmetic)
        );
        let unanchored = ObservedProgress::new(2.0, None)
            .unwrap()
            .prepare_capped(Some(0.5), 100.0)
            .unwrap()
            .unwrap();
        assert_eq!(unanchored.observed(), 2.5);
        assert_eq!(unanchored.delivery().amount(), 0.5);
    }

    #[test]
    fn prefix_observation_preserves_missing_only_enrichment_and_quiet_history() {
        let missing = Cursor::seed(1.0, None).unwrap();
        let known = Cursor::seed(1.0, Some(100.0)).unwrap();
        let policy = Arithmetic::DecimalCapped20;
        assert_eq!(
            Cursor::prepare_prefix(Some(missing), 1.0, Some(100.0), policy),
            Ok(Some(known))
        );
        assert_eq!(
            Cursor::prepare_prefix(Some(known), 1.0, Some(200.0), policy),
            Ok(None)
        );
        assert_eq!(
            Cursor::prepare_prefix(Some(known), 0.5, Some(50.0), policy),
            Ok(None)
        );
        assert_eq!(
            Cursor::prepare_prefix(Some(known), 2.0, None, policy),
            Err(Rejection::Unpriced)
        );
        assert_eq!(
            Cursor::prepare_prefix(Some(known), 2.0, Some(100.0), policy),
            Err(Rejection::ValueRegressed)
        );
        assert_eq!(
            Cursor::prepare_prefix(Some(known), 1.0, Some(f64::INFINITY), policy),
            Err(Rejection::InvalidValue)
        );
        assert_eq!(
            Cursor::prepare_prefix(None, 1.0, Some(0.0), policy),
            Err(Rejection::InvalidValue)
        );
        assert_eq!(
            Cursor::prepare_prefix(None, 0.0, Some(0.0), policy),
            Ok(Some(Cursor::seed(0.0, Some(0.0)).unwrap()))
        );
        assert_eq!(
            Cursor::prepare_prefix(Some(missing), 2.0, Some(200.0), policy),
            Ok(Some(Cursor::seed(2.0, Some(200.0)).unwrap()))
        );
    }

    #[test]
    fn finite_prefix_observation_does_not_authorize_an_overflowing_public_price() {
        let previous = Cursor::seed(1.0, Some(1.0)).unwrap();
        let next = Cursor::prepare_prefix(
            Some(previous),
            1.5,
            Some(1.5e308),
            Arithmetic::DecimalCapped20,
        )
        .unwrap()
        .unwrap();
        assert_eq!(next.quantity(), 1.5);
        assert_eq!(next.value(), Some(1.5e308));
        assert_eq!(
            previous.prepare(
                Report::Cumulative {
                    quantity: 1.5,
                    price: CumulativePrice::Value {
                        cumulative: Some(1.5e308),
                        fallback: None
                    },
                },
                Arithmetic::DecimalCapped20
            ),
            Err(Rejection::NonFiniteArithmetic)
        );
        let corrected = previous
            .prepare(
                Report::Cumulative {
                    quantity: 1.5,
                    price: CumulativePrice::Value {
                        cumulative: Some(150.0),
                        fallback: None,
                    },
                },
                Arithmetic::DecimalCapped20,
            )
            .unwrap()
            .unwrap()
            .scaled(0.001, ExecutionRounding::ScaledSignificant15)
            .unwrap();
        assert_eq!((corrected.price(), corrected.amount()), (298.0, 0.0005));
        assert_eq!(corrected.next(), Cursor::seed(1.5, Some(150.0)).unwrap());
    }

    #[test]
    fn capped_decimal_and_scaled_output_keep_native_anchors_and_reject_bad_outputs() {
        let next_quantity = 0.30000000000000004;
        let prepared = Cursor::default()
            .prepare(
                Report::Cumulative {
                    quantity: next_quantity,
                    price: CumulativePrice::Value {
                        cumulative: Some(36.0),
                        fallback: None,
                    },
                },
                Arithmetic::DecimalCapped20,
            )
            .unwrap()
            .unwrap();
        let scaled = prepared
            .scaled(0.001, ExecutionRounding::ScaledSignificant15)
            .unwrap();
        assert_eq!((scaled.price(), scaled.amount()), (120.0, 0.0003));
        assert_eq!(scaled.next(), prepared.next());
        assert_eq!(scaled.next().quantity(), next_quantity);
        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                prepared
                    .scaled(scale, ExecutionRounding::ScaledSignificant15)
                    .is_err()
            );
        }
        let large = Cursor::default()
            .prepare(
                Report::Last {
                    amount: 2.0,
                    cumulative: None,
                    price: 100.0,
                },
                Arithmetic::Binary,
            )
            .unwrap()
            .unwrap();
        assert!(
            large
                .scaled(f64::MAX, ExecutionRounding::ScaledSignificant15)
                .is_err()
        );
        let tiny_price = Cursor::default()
            .prepare(
                Report::Last {
                    amount: 1.0,
                    cumulative: None,
                    price: 1e-310,
                },
                Arithmetic::Binary,
            )
            .unwrap()
            .unwrap();
        assert!(
            tiny_price
                .scaled(1.0, ExecutionRounding::ScaledSignificant15)
                .is_err()
        );
        let tiny_prefix = Cursor::seed(1e-21, Some(1.0)).unwrap();
        assert_eq!(
            tiny_prefix.prepare(
                Report::Cumulative {
                    quantity: 2e-21,
                    price: CumulativePrice::Value {
                        cumulative: Some(2.0),
                        fallback: None
                    },
                },
                Arithmetic::DecimalCapped20
            ),
            Err(Rejection::InvalidQuantity)
        );
    }

    #[test]
    fn average_and_direct_quote_preparation_keep_the_historical_anchor() {
        let seed = Cursor::seed(1.0, Some(100.0)).unwrap();
        let direct = seed
            .prepare_average(2.0, None, Some(112.0), Arithmetic::Binary)
            .unwrap()
            .unwrap();
        assert_eq!((direct.amount(), direct.price()), (1.0, 112.0));
        assert_eq!(direct.next(), Cursor::seed(2.0, Some(212.0)).unwrap());
        let average = direct
            .next()
            .prepare_average(3.0, Some(110.0), Some(999.0), Arithmetic::Binary)
            .unwrap()
            .unwrap();
        assert_eq!((average.amount(), average.price()), (1.0, 118.0));
        assert_eq!(average.next(), Cursor::seed(3.0, Some(330.0)).unwrap());
    }

    #[test]
    fn rounded_execution_keeps_unrounded_cumulative_quote_and_quantity() {
        let seed = Cursor::seed(1.0, Some(100.0)).unwrap();
        let raw = seed
            .prepare_average(1.4, Some(1000.1), None, Arithmetic::Binary)
            .unwrap()
            .unwrap();
        let rounded = raw.rounded(ExecutionRounding::Significant15).unwrap();
        assert_eq!((rounded.price(), rounded.amount()), (3250.35, 0.4));
        assert_eq!(rounded.next(), raw.next());
        assert_eq!(rounded.next().quantity(), 1.4);
        assert_eq!(rounded.next().value(), Some(1000.1 * 1.4));
    }

    #[test]
    fn average_quote_rejections_do_not_invent_a_baseline_or_use_a_fallback() {
        let missing = Cursor::seed(1.0, None).unwrap();
        assert_eq!(
            missing.prepare_average(2.0, Some(110.0), Some(100.0), Arithmetic::Binary),
            Err(Rejection::Unpriced)
        );
        assert_eq!(
            missing.prepare_average(2.0, None, Some(100.0), Arithmetic::Binary),
            Err(Rejection::Unpriced)
        );
        let seed = Cursor::seed(1.0, Some(100.0)).unwrap();
        for average in [f64::NAN, f64::INFINITY, -1.0, 1e308] {
            assert!(
                seed.prepare_average(2.0, Some(average), Some(100.0), Arithmetic::Binary)
                    .is_err()
            );
        }
        assert!(
            seed.prepare_average(2.0, None, Some(f64::INFINITY), Arithmetic::Binary)
                .is_err()
        );
        let huge = Cursor::seed(1.0, Some(1e308)).unwrap();
        assert_eq!(
            huge.prepare_average(2.0, None, Some(1e308), Arithmetic::Binary),
            Err(Rejection::NonFiniteArithmetic)
        );
        assert_eq!(
            seed.prepare_average(1.0, Some(f64::NAN), None, Arithmetic::Binary),
            Ok(None)
        );
        assert_eq!(
            seed.prepare_average(0.5, Some(100.0), None, Arithmetic::Binary),
            Err(Rejection::QuantityRegressed)
        );
    }

    #[test]
    fn observed_progress_separates_delivery_position_and_display() {
        let observed = ObservedProgress::new(0.005, Some(0.006)).unwrap();
        let delivered = Cursor::seed(0.005, None).unwrap();
        let prepared = observed
            .prepare(delivered, Some(0.003), 59000.0)
            .unwrap()
            .unwrap();
        assert_eq!(prepared.delivery().amount(), 0.003);
        assert_eq!(prepared.delivery().next().quantity(), 0.006);
        assert_eq!(prepared.observed(), 0.006);
        assert_eq!(prepared.position_amount(), 0.006 - 0.005);

        let observed = ObservedProgress::new(0.006, Some(0.006)).unwrap();
        for invalid in [Some(0.0), Some(-0.001)] {
            assert!(matches!(
                observed.prepare(delivered, invalid, 59000.0),
                Ok(None) | Err(_)
            ));
        }
        let correction = observed.prepare(delivered, None, 59000.0).unwrap().unwrap();
        assert_eq!(correction.delivery().amount(), 0.006 - 0.005);
        assert_eq!(correction.position_amount(), 0.0);
        assert_eq!(
            observed.prepare(correction.delivery().next(), None, 59000.0),
            Ok(None)
        );

        let blind = ObservedProgress::new(0.010, Some(0.011)).unwrap();
        assert_eq!(
            blind
                .prepare(delivered, None, 59000.0)
                .unwrap()
                .unwrap()
                .delivery()
                .amount(),
            0.011 - 0.010
        );
        let late = ObservedProgress::new(0.010, Some(0.006)).unwrap();
        let fill = late
            .prepare(delivered, Some(0.001), 59000.0)
            .unwrap()
            .unwrap();
        assert_eq!(fill.observed(), 0.010);
        assert_eq!(fill.position_amount(), 0.0);
        assert_eq!(fill.delivery().next().quantity(), 0.006);
    }

    #[test]
    fn observed_missing_cumulative_checks_both_additions_and_invalid_prices() {
        let observed = ObservedProgress::new(10.0, None).unwrap();
        let delivered = Cursor::seed(4.0, None).unwrap();
        let fill = observed
            .prepare(delivered, Some(2.0), 100.0)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                fill.observed(),
                fill.delivery().next().quantity(),
                fill.position_amount()
            ),
            (12.0, 6.0, 2.0)
        );
        assert_eq!(observed.prepare(delivered, None, 100.0), Ok(None));
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(observed.prepare(delivered, Some(2.0), bad).is_err());
        }
        let huge = Cursor::seed(f64::MAX, None).unwrap();
        assert_eq!(
            observed.prepare(huge, Some(f64::MAX), 100.0),
            Err(Rejection::NonFiniteArithmetic)
        );
        assert_eq!(
            ObservedProgress::new(f64::MAX, None).unwrap().prepare(
                delivered,
                Some(f64::MAX),
                100.0
            ),
            Err(Rejection::NonFiniteArithmetic)
        );
        for bad in [-1.0, f64::NAN, f64::INFINITY] {
            assert!(ObservedProgress::new(bad, None).is_err());
            assert!(ObservedProgress::new(0.0, Some(bad)).is_err());
        }
    }

    #[test]
    fn interval_union_delivers_late_uncovered_intervals_and_bridges_once() {
        let mut history = IntervalHistory::new(0.0, 500).unwrap();
        assert_eq!(interval_fill(&mut history, 6.0, 8.0), Some((100.0, 2.0)));
        assert_eq!(interval_fill(&mut history, 4.0, 6.0), Some((100.0, 2.0)));
        assert_eq!(interval_fill(&mut history, 5.0, 9.0), Some((100.0, 1.0)));
        assert_eq!(history.intervals, [(4.0, 9.0)]);
        assert_eq!(interval_fill(&mut history, 4.0, 9.0), None);
        assert_eq!(interval_fill(&mut history, 2.0, 4.0), Some((100.0, 2.0)));
        assert_eq!(history.intervals, [(2.0, 9.0)]);
        assert_eq!(history.floor, 0.0);
    }

    #[test]
    fn interval_proposals_have_no_effect_until_consuming_commit() {
        let mut history = IntervalHistory::new(0.0, 500).unwrap();
        {
            let _abandoned = history.prepare(uniform(0.0, 1.0, 100.0)).unwrap();
        }
        assert!(history.intervals.is_empty());
        assert_eq!(interval_fill(&mut history, 0.0, 1.0), Some((100.0, 1.0)));
        assert_eq!(interval_fill(&mut history, 0.0, 1.0), None);
    }

    #[test]
    fn interval_cap_merges_before_eviction_and_floor_only_suppresses_retired_quantity() {
        let mut history = IntervalHistory::new(1.0, 2).unwrap();
        assert_eq!(interval_fill(&mut history, 0.0, 2.0), Some((100.0, 1.0)));
        assert_eq!(interval_fill(&mut history, 4.0, 5.0), Some((100.0, 1.0)));
        assert_eq!(interval_fill(&mut history, 2.0, 4.0), Some((100.0, 2.0)));
        assert_eq!(
            history.floor, 1.0,
            "bridging must merge before cap eviction"
        );
        assert_eq!(history.intervals, [(1.0, 5.0)]);
        interval_fill(&mut history, 7.0, 8.0);
        interval_fill(&mut history, 10.0, 11.0);
        assert_eq!(history.floor, 5.0);
        assert_eq!(history.intervals, [(7.0, 8.0), (10.0, 11.0)]);
        assert_eq!(interval_fill(&mut history, 0.0, 5.0), None);
        assert_eq!(interval_fill(&mut history, 4.0, 6.0), Some((100.0, 1.0)));
        assert_eq!(history.intervals.len(), 2);

        let mut native_cap = IntervalHistory::new(0.0, 500).unwrap();
        for index in 0..501 {
            interval_fill(
                &mut native_cap,
                2.0 * index as f64,
                2.0 * index as f64 + 1.0,
            );
        }
        assert_eq!(native_cap.intervals.len(), 500);
        assert_eq!(native_cap.floor, 1.0);
    }

    #[test]
    fn aggregate_vwap_cannot_price_a_partially_overlapped_or_floor_clipped_subset() {
        let quote = |from, from_value, to, to_value| {
            PricedInterval::quotes(
                Cursor::seed(from, Some(from_value)).unwrap(),
                Cursor::seed(to, Some(to_value)).unwrap(),
                ExecutionRounding::Significant15,
            )
            .unwrap()
        };
        let mut history = IntervalHistory::new(0.0, 500).unwrap();
        interval_fill(&mut history, 6.0, 8.0);
        assert_eq!(
            history.prepare(quote(4.0, 400.0, 8.0, 1000.0)).unwrap_err(),
            Rejection::Unpriced
        );
        assert_eq!(history.intervals, [(6.0, 8.0)]);
        assert_eq!(interval_fill(&mut history, 4.0, 8.0), Some((100.0, 2.0)));
        assert!(
            history
                .prepare(quote(4.0, 400.0, 8.0, 1000.0))
                .unwrap()
                .commit()
                .is_none()
        );
        let mut history = IntervalHistory::new(2.0, 500).unwrap();
        assert_eq!(
            history.prepare(quote(0.0, 0.0, 4.0, 400.0)).unwrap_err(),
            Rejection::Unpriced
        );
        assert!(history.intervals.is_empty());
        let delivery = history
            .prepare(quote(2.0, 200.0, 4.0, 440.0))
            .unwrap()
            .commit()
            .unwrap();
        assert_eq!((delivery.price(), delivery.amount()), (120.0, 2.0));
    }

    #[test]
    fn interval_evidence_checks_products_values_division_and_exact_native_rounding() {
        let rounding = ExecutionRounding::Significant15;
        assert_eq!(scaled_quantity(0.3, 3.0, rounding), Ok(0.9));
        assert_eq!(
            scaled_quantity(f64::MAX, 10.0, rounding),
            Err(Rejection::NonFiniteArithmetic)
        );
        assert_eq!(
            Cursor::seed_with_average(30.0, None, Some(1e308)),
            Err(Rejection::NonFiniteArithmetic)
        );
        assert!(
            Cursor::seed(30.0, Some(3300.0))
                .unwrap()
                .direct_suffix(40.0, f64::MAX)
                .is_err()
        );
        assert_eq!(
            Cursor::seed(10.0, None).unwrap().direct_suffix(20.0, 100.0),
            Err(Rejection::Unpriced)
        );
        assert_eq!(
            Cursor::seed(10.0, Some(0.0))
                .unwrap()
                .direct_suffix(20.0, 100.0)
                .unwrap()
                .value(),
            Some(1000.0)
        );
        for invalid in [0.0, -1.0, 11.0, f64::INFINITY, f64::NAN] {
            assert!(QuantityInterval::suffix(10.0, invalid, rounding).is_err());
        }
        let from = Cursor::seed(0.0, None).unwrap();
        let to = Cursor::seed(f64::MIN_POSITIVE, Some(f64::MAX)).unwrap();
        assert!(PricedInterval::quotes(from, to, rounding).is_err());
        assert!(
            PricedInterval::quotes(
                Cursor::seed(1.0, None).unwrap(),
                Cursor::seed(2.0, Some(100.0)).unwrap(),
                rounding
            )
            .is_err()
        );
        let mut history = IntervalHistory::new(0.0, 500).unwrap();
        let interval = QuantityInterval::suffix(0.3, 0.1, rounding).unwrap();
        let delivery = history
            .prepare(PricedInterval::uniform(interval, 1.2345678901234567, rounding).unwrap())
            .unwrap()
            .commit()
            .unwrap();
        assert_eq!(
            (delivery.price(), delivery.amount()),
            (1.23456789012346, 0.1)
        );
        for bad in [0.0, -1.0, f64::INFINITY, f64::NAN] {
            assert!(
                PricedInterval::uniform(QuantityInterval::new(0.0, 1.0).unwrap(), bad, rounding)
                    .is_err()
            );
        }
        assert!(IntervalHistory::new(0.0, 0).is_err());
        assert!(IntervalHistory::new(f64::NAN, 500).is_err());
    }

    #[test]
    fn quote_enrichment_fills_only_missing_evidence_at_the_same_quantity() {
        let delivered = Cursor::seed(10.0, None).unwrap();
        let correction = Cursor::seed(10.0, Some(1000.0)).unwrap();
        assert_eq!(delivered.enrich_quote(correction), Some(correction));
        assert_eq!(
            correction.enrich_quote(Cursor::seed(10.0, Some(1100.0)).unwrap()),
            None
        );
        assert_eq!(
            delivered.enrich_quote(Cursor::seed(20.0, Some(2000.0)).unwrap()),
            None
        );
        assert_eq!(
            delivered.enrich_quote(Cursor::seed(5.0, Some(500.0)).unwrap()),
            None
        );
        assert_eq!(delivered.enrich_quote(delivered), None);
        let explicit_zero = Cursor::seed(10.0, Some(0.0)).unwrap();
        assert_eq!(explicit_zero.enrich_quote(correction), None);
        assert_eq!(delivered.enrich_quote(explicit_zero), Some(explicit_zero));
        assert_eq!(
            Cursor::default()
                .enrich_quote(Cursor::seed(0.0, Some(0.0)).unwrap())
                .unwrap()
                .quantity(),
            0.0
        );
    }

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
