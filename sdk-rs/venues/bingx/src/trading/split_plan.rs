//! Validate the complete native quantity plan before preparing any write.
use safe_cex_engine::model::Market;
use safe_cex_engine::{ErrorKind, Outcome, VenueError};

pub(super) struct WireOrder {
    quantity: Option<f64>,
    price: Option<f64>,
    reference: Option<f64>,
    min_quantity: f64,
    max_quantity: f64,
    quantity_step: f64,
    price_tick: f64,
    min_value: f64,
}

impl WireOrder {
    pub(super) fn new(
        quantity: Option<f64>,
        price: Option<f64>,
        reference: Option<f64>,
        market: &Market,
        min_value: f64,
    ) -> Self {
        Self {
            quantity,
            price,
            reference,
            min_quantity: market.limits.amount.min,
            max_quantity: market.limits.amount.max,
            quantity_step: market.precision.amount,
            price_tick: market.precision.price,
            min_value,
        }
    }
}

pub(super) fn validate_write(write: &WireOrder) -> Result<(), VenueError> {
    validate_order(write).map_err(|message| {
        VenueError::new(message)
            .kind(ErrorKind::InvalidInput)
            .outcome(Outcome::NotSent)
    })
}

fn arithmetic_error() -> String {
    "Split plan cannot be validated with exact decimal arithmetic".into()
}

struct Decimal {
    units: u128,
    scale: u32,
}

impl Decimal {
    fn parse(value: f64) -> Result<Self, String> {
        if !value.is_finite() || value < 0. {
            return Err(arithmetic_error());
        }
        let text = value.to_string();
        let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
        Ok(Self {
            units: format!("{whole}{fraction}")
                .parse()
                .map_err(|_| arithmetic_error())?,
            scale: u32::try_from(fraction.len()).map_err(|_| arithmetic_error())?,
        })
    }

    fn at_scale(&self, scale: u32) -> Result<u128, String> {
        let exponent = scale.checked_sub(self.scale).ok_or_else(arithmetic_error)?;
        let factor = 10u128.checked_pow(exponent).ok_or_else(arithmetic_error)?;
        self.units.checked_mul(factor).ok_or_else(arithmetic_error)
    }

    fn product_at_least(&self, price: &Self, minimum: &Self) -> Result<bool, String> {
        let product = Self {
            units: self
                .units
                .checked_mul(price.units)
                .ok_or_else(arithmetic_error)?,
            scale: self
                .scale
                .checked_add(price.scale)
                .ok_or_else(arithmetic_error)?,
        };
        let scale = product.scale.max(minimum.scale);
        Ok(product.at_scale(scale)? >= minimum.at_scale(scale)?)
    }
}

fn validate_order(write: &WireOrder) -> Result<(), String> {
    if let Some(price) = write.price {
        let tick = Decimal::parse(write.price_tick)?;
        let price = Decimal::parse(price)?;
        let scale = price.scale.max(tick.scale);
        if tick.units == 0
            || price.units == 0
            || price.at_scale(scale)? % tick.at_scale(scale)? != 0
        {
            return Err("Order price requires a positive native tick multiple".into());
        }
    }
    // Native position-derived closes keep their venue-specific quantity policy.
    let Some(quantity) = write.quantity else {
        return Ok(());
    };
    if !write.min_quantity.is_finite()
        || write.min_quantity < 0.
        || write.max_quantity.is_nan()
        || write.max_quantity <= 0.
        || write.min_quantity > write.max_quantity
    {
        return Err("Invalid split plan quantity limits".into());
    }
    let step = Decimal::parse(write.quantity_step)?;
    if step.units == 0 {
        return Err("Split plan requires a positive native quantity step".into());
    }
    let decimal = Decimal::parse(quantity)?;
    let units = decimal.at_scale(step.scale)?;
    if units == 0 || units % step.units != 0 || quantity > write.max_quantity {
        return Err("Split order lot exceeds native quantity limits or step".into());
    }
    if quantity < write.min_quantity {
        return Err("Split order lot is below the market minimum".into());
    }
    let minimum = Decimal::parse(write.min_value)?;
    if write.min_value > 0. {
        let price = Decimal::parse(
            write
                .reference
                .filter(|value| value.is_finite() && *value > 0.)
                .ok_or("Split plan requires a positive notional reference price")?,
        )?;
        if !decimal.product_at_least(&price, &minimum)? {
            return Err(format!(
                "Split order lot value is below the BingX minimum of {} USDT",
                write.min_value
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_total(intent: f64, lots: &[f64], step: f64) -> Result<(), String> {
    let step = Decimal::parse(step)?;
    if step.units == 0 || lots.is_empty() {
        return Err("Split plan requires a positive native quantity step and lots".into());
    }
    let expected = Decimal::parse(intent)?.at_scale(step.scale)?;
    if expected == 0 || expected % step.units != 0 {
        return Err("Order intent must be a positive native step multiple".into());
    }
    let mut total = 0u128;
    for &lot in lots {
        let units = Decimal::parse(lot)?.at_scale(step.scale)?;
        total = total.checked_add(units).ok_or_else(arithmetic_error)?;
    }
    if total != expected {
        return Err("Split order lots do not exactly preserve the order intent".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Decimal;

    #[test]
    fn wire_decimal_products_keep_an_inclusive_minimum() {
        for (quantity, price, minimum, accepted) in [
            (0.1, 0.7, 0.07, true),
            (0.099, 0.7, 0.07, false),
            (0.0003, 10_000., 3., true),
            (0.0003, 9_999.99, 3., false),
        ] {
            assert_eq!(
                Decimal::parse(quantity)
                    .expect("quantity")
                    .product_at_least(
                        &Decimal::parse(price).expect("price"),
                        &Decimal::parse(minimum).expect("minimum")
                    )
                    .expect("bounded product"),
                accepted,
            );
        }
    }
}
