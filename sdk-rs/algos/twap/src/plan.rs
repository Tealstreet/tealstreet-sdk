//! Pure checks over the exact scheduled writes, before any slice is released.

use safe_cex_engine::error::{ErrorKind, Outcome, VenueError};

use crate::{OrderKind, TwapOrderRequest};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OrderRules {
    pub minimum_amount: f64,
    /// Zero or positive infinity means the venue publishes no upper bound.
    pub maximum_amount: f64,
    pub amount_step: Option<f64>,
    pub price_tick: Option<f64>,
    /// Execution price of a native marketable limit behind a market intent.
    pub wire_price: Option<f64>,
    /// Supplied by the native adapter, including contract units and close exemptions.
    pub notional: NotionalCheck,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum NotionalCheck {
    /// Notional unknown: the native adapter supplied no minimum.
    #[default]
    Unknown,
    /// An explicit native exemption, never inferred from missing data.
    Exempt,
    Required(NotionalRule),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NotionalRule {
    pub minimum: f64,
    /// Quote value of one amount unit at this write's execution/reference price.
    pub per_unit: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedOrder {
    pub request: TwapOrderRequest,
    pub rules: OrderRules,
}

pub(crate) fn invalid(reason: impl Into<String>) -> VenueError {
    VenueError::new(reason)
        .kind(ErrorKind::InvalidInput)
        .outcome(Outcome::NotSent)
}

fn on_grid(value: f64, step: f64) -> bool {
    let units = value / step;
    units.is_finite() && (units - units.round()).abs() <= 1e-9
}

/// Matches WritePlan's pure `Fn(&T) -> Result<(), VenueError>` gate.
pub fn validate_write(write: &PlannedOrder) -> Result<(), VenueError> {
    let PlannedOrder { request, rules } = write;
    let amount = request.amount;
    if !(amount.is_finite() && amount > 0.0) {
        return Err(invalid("slice amount must be finite and positive"));
    }
    if !(rules.minimum_amount.is_finite() && rules.minimum_amount >= 0.0)
        || rules.maximum_amount.is_nan()
        || rules.maximum_amount < 0.0
    {
        return Err(invalid("invalid native quantity bounds"));
    }
    if amount < rules.minimum_amount {
        return Err(invalid("slice amount is below the native minimum"));
    }
    if rules.maximum_amount > 0.0 && amount > rules.maximum_amount {
        return Err(invalid("slice amount exceeds the native maximum"));
    }
    if let Some(step) = rules.amount_step
        && (!(step.is_finite() && step > 0.0) || !on_grid(amount, step))
    {
        return Err(invalid("slice amount is off the native step"));
    }
    let price = if request.kind == OrderKind::Limit {
        Some(
            request
                .price
                .ok_or_else(|| invalid("slice limit price is unavailable"))?,
        )
    } else {
        rules.wire_price
    };
    if let Some(price) = price {
        if !(price.is_finite() && price > 0.0) {
            return Err(invalid("slice execution price must be finite and positive"));
        }
        if let Some(tick) = rules.price_tick
            && (!(tick.is_finite() && tick > 0.0) || !on_grid(price, tick))
        {
            return Err(invalid("slice price is off the native tick"));
        }
    }
    if let NotionalCheck::Required(notional) = rules.notional {
        let value = amount * notional.per_unit;
        if !(notional.minimum.is_finite()
            && notional.minimum >= 0.0
            && notional.per_unit.is_finite()
            && notional.per_unit > 0.0
            && value.is_finite())
        {
            return Err(invalid("native notional reference is unavailable"));
        }
        if value < notional.minimum {
            return Err(invalid("slice value is below the native minimum notional"));
        }
    }
    Ok(())
}
