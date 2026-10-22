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
}

impl ExecutionRounding {
    fn apply(self, value: f64) -> Result<f64, Rejection> {
        finite(value)?;
        finite(match self {
            Self::Binary => value,
            Self::Significant15 => num::significant15(value),
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
