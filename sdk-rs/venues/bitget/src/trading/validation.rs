use super::{Payload, Planned};
use crate::paths;
use safe_cex_engine::{ErrorKind, Outcome, VenueError, num};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum QuantityMode {
    Required,
    FullPosition,
    Unchanged,
}

#[derive(Clone)]
pub(super) struct NativeRules {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub tick: f64,
    pub min_value: f64,
    pub market_price: Option<f64>,
    pub inverse: bool,
    pub reduce_only: bool,
    pub limit: bool,
    pub quantity: QuantityMode,
}

fn invalid(message: &str) -> VenueError {
    VenueError::new(format!("Bitget {message}"))
        .kind(ErrorKind::InvalidInput)
        .outcome(Outcome::NotSent)
}

fn positive(text: &str) -> Result<f64, VenueError> {
    text.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n > 0.0)
        .ok_or_else(|| invalid("wire value must be finite and positive"))
}

pub(super) fn wire_price(value: f64, tick: f64) -> Result<f64, VenueError> {
    if !tick.is_finite() || tick <= 0.0 || !value.is_finite() || value <= 0.0 {
        return Err(invalid("price or tick is invalid"));
    }
    positive(&super::number(value, tick))
}

impl NativeRules {
    pub(super) fn minimum(&self, price: Option<f64>) -> Result<f64, VenueError> {
        if !self.min.is_finite()
            || self.min < 0.0
            || self.max.is_nan()
            || self.max <= 0.0
            || !self.step.is_finite()
            || self.step <= 0.0
            || !self.tick.is_finite()
            || self.tick <= 0.0
            || !self.min_value.is_finite()
            || self.min_value < 0.0
        {
            return Err(invalid("native order limits are invalid"));
        }
        let minimum = if self.reduce_only || self.min_value == 0.0 {
            self.min
        } else {
            let value_per_unit = if self.inverse {
                1.0
            } else {
                (if self.limit { price } else { self.market_price })
                    .filter(|price| price.is_finite() && *price > 0.0)
                    .ok_or_else(|| invalid("minimum value requires a positive price reference"))?
            };
            self.min.max(self.min_value / value_per_unit)
        };
        if !minimum.is_finite() || minimum > self.max {
            return Err(invalid(
                "minimum quantity or value exceeds the native maximum",
            ));
        }
        Ok(minimum)
    }
}

// The callback reads only captured native rules and the exact payload to send.
pub(super) fn validate_write(write: &Planned) -> Result<(), VenueError> {
    if write.path == paths::BATCH_PLACE {
        let rows = write
            .payload
            .order_list
            .as_deref()
            .ok_or_else(|| invalid("batch has no rows"))?;
        if !(1..=50).contains(&rows.len()) {
            return Err(invalid("batch must contain 1..50 rows"));
        }
        for row in rows {
            validate_row(row, true)?;
        }
    }
    validate_row(&write.payload, write.path == paths::UTA_PLACE)
}

fn validate_row(row: &Payload, required: bool) -> Result<(), VenueError> {
    let Some(rules) = &row.native_rules else {
        return if required {
            Err(invalid("order has no captured native rules"))
        } else {
            Ok(())
        };
    };
    let price = row
        .new_price
        .as_deref()
        .or(row.price.as_deref())
        .map(positive)
        .transpose()?;
    if rules.limit && price.is_none() {
        return Err(invalid("limit order requires a wire price"));
    }
    for text in [
        row.price.as_deref(),
        row.new_price.as_deref(),
        row.trigger_price.as_deref(),
        row.new_trigger_price.as_deref(),
        row.take_profit.as_deref(),
        row.stop_loss.as_deref(),
        row.preset_stop_surplus_price.as_deref(),
        row.preset_stop_loss_price.as_deref(),
        row.new_preset_stop_surplus_price.as_deref(),
        row.new_preset_stop_loss_price.as_deref(),
        row.tp_limit_price.as_deref(),
        row.sl_limit_price.as_deref(),
        row.execute_price.as_deref().filter(|price| *price != "0"),
    ]
    .into_iter()
    .flatten()
    .chain(
        row.trailing_stop_params
            .iter()
            .flatten()
            .map(|p| p.activation_price.as_str()),
    ) {
        let value = positive(text)?;
        if num::adjust(value, rules.tick) != value {
            return Err(invalid("wire price is outside native tick precision"));
        }
    }
    let minimum = rules.minimum(price)?;
    let quantity = row
        .new_size
        .as_deref()
        .or(row.qty.as_deref())
        .or(row.size.as_deref());
    match quantity {
        None if rules.quantity != QuantityMode::Required => Ok(()),
        Some("" | "0") if rules.quantity == QuantityMode::FullPosition => Ok(()),
        Some(text) => {
            let amount = positive(text)?;
            if amount < minimum || amount > rules.max || num::adjust(amount, rules.step) != amount {
                return Err(invalid(
                    "wire quantity violates native minimum, maximum, step or value",
                ));
            }
            Ok(())
        }
        None => Err(invalid("order requires a wire quantity")),
    }
}

pub(super) fn split_sizes(
    total: f64,
    minimum: f64,
    max: f64,
    step: f64,
) -> Result<Vec<f64>, VenueError> {
    let units = (total / step).round();
    let rounded_cap = (max / step).round();
    let cap = if num::adjust(rounded_cap * step, step) > max {
        rounded_cap - 1.0
    } else {
        rounded_cap
    }
    .min(units);
    let rounded_min = num::adjust(minimum, step);
    let min_lot = if rounded_min >= minimum {
        rounded_min
    } else {
        num::adjust(rounded_min + step, step)
    };
    let min_units = (min_lot / step).round().max(1.0);
    if !total.is_finite()
        || total <= 0.0
        || !units.is_finite()
        || units > 9_007_199_254_740_991.0
        || !cap.is_finite()
        || cap < min_units
        || min_units > units
        || units / cap > 10_000.0
    {
        return Err(invalid("order has no valid bounded native lot allocation"));
    }
    let mut sizes = vec![];
    let mut left = units;
    while left > 0.0 {
        let size = left.min(cap);
        sizes.push(size);
        left -= size;
    }
    if let Some((tail, donors)) = sizes.split_last_mut() {
        let mut short = (min_units - *tail).max(0.0);
        for donor in donors.iter_mut().rev() {
            let moved = short.min((*donor - min_units).max(0.0));
            *donor -= moved;
            *tail += moved;
            short -= moved;
        }
        if short > 0.0 {
            return Err(invalid(
                "split tail cannot meet native quantity or value minimum",
            ));
        }
    }
    let sizes: Vec<_> = sizes
        .into_iter()
        .map(|units| num::adjust(units * step, step))
        .collect();
    if sizes.iter().any(|amount| {
        !amount.is_finite()
            || *amount < minimum
            || *amount <= 0.0
            || *amount > max
            || num::adjust(*amount, step) != *amount
    }) || num::adjust(sizes.iter().sum(), step) != total
    {
        return Err(invalid(
            "native lot allocation does not preserve valid wire quantities",
        ));
    }
    Ok(sizes)
}
