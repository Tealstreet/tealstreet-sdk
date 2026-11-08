//! Validate the complete native quantity plan before preparing any write.
use safe_cex_engine::model::Market;

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

pub(super) fn validate(
    intent: f64,
    lots: &[f64],
    market: &Market,
    min_value: f64,
    reference: Option<f64>,
) -> Result<(), String> {
    let bounds = &market.limits.amount;
    if lots.is_empty()
        || !bounds.min.is_finite()
        || bounds.min < 0.
        || bounds.max.is_nan()
        || bounds.max <= 0.
        || bounds.min > bounds.max
    {
        return Err("Invalid split plan quantity limits".into());
    }
    let step = Decimal::parse(market.precision.amount)?;
    if step.units == 0 {
        return Err("Split plan requires a positive native quantity step".into());
    }
    let expected = Decimal::parse(intent)?.at_scale(step.scale)?;
    if expected == 0 || expected % step.units != 0 {
        return Err("Order intent must be a positive native step multiple".into());
    }
    let minimum = Decimal::parse(min_value)?;
    let price = if min_value > 0. {
        Some(Decimal::parse(
            reference
                .filter(|value| value.is_finite() && *value > 0.)
                .ok_or("Split plan requires a positive notional reference price")?,
        )?)
    } else {
        None
    };
    let mut total = 0u128;
    for &lot in lots {
        let quantity = Decimal::parse(lot)?;
        let units = quantity.at_scale(step.scale)?;
        if units == 0 || units % step.units != 0 || lot > bounds.max {
            return Err("Split order lot exceeds native quantity limits or step".into());
        }
        if lot < bounds.min {
            return Err("Split order lot is below the market minimum".into());
        }
        if let Some(price) = &price
            && !quantity.product_at_least(price, &minimum)?
        {
            return Err(format!(
                "Split order lot value is below the BingX minimum of {min_value} USDT"
            ));
        }
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
