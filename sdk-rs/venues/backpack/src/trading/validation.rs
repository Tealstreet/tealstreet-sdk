use super::{NativeOrder, Step};
use safe_cex_engine::{ErrorKind, Outcome, VenueError, model::Market, num};

fn refused(message: &str) -> VenueError {
    VenueError::new(message)
        .kind(ErrorKind::InvalidInput)
        .outcome(Outcome::NotSent)
}

fn grid(text: &str, step: f64) -> Result<f64, VenueError> {
    let value = text
        .parse::<f64>()
        .map_err(|_| refused("Backpack wire amount/price is not numeric"))?;
    if !value.is_finite()
        || value <= 0.
        || !step.is_finite()
        || step <= 0.
        || value != num::adjust(value, step)
    {
        return Err(refused(
            "Backpack wire amount/price is outside its native grid",
        ));
    }
    Ok(value)
}

pub(super) fn validate_order(order: &NativeOrder, markets: &[Market]) -> Result<(), VenueError> {
    let market = markets
        .iter()
        .find(|market| market.id == order.symbol)
        .ok_or_else(|| refused("Backpack wire order market is missing"))?;
    let quantity = match (&order.quantity, &order.trigger_quantity) {
        (Some(quantity), None) | (None, Some(quantity)) => quantity,
        _ => return Err(refused("Backpack wire order needs one quantity")),
    };
    let amount = grid(quantity, market.precision.amount)?;
    if !market.limits.amount.min.is_finite()
        || !market.limits.amount.max.is_finite()
        || amount < market.limits.amount.min
        || (market.limits.amount.max > 0. && amount > market.limits.amount.max)
    {
        return Err(refused(
            "Backpack wire order amount is outside market bounds",
        ));
    }
    if order.order_type == "Limit" && order.price.is_none() {
        return Err(refused("Backpack wire limit order needs price"));
    }
    for price in [
        &order.price,
        &order.trigger_price,
        &order.take_profit_trigger_price,
        &order.stop_loss_trigger_price,
    ]
    .into_iter()
    .flatten()
    {
        let price = grid(price, market.precision.price)?;
        if market
            .limits
            .price
            .is_some_and(|limit| !limit.min.is_finite() || price < limit.min)
        {
            return Err(refused("Backpack wire order price is below market minimum"));
        }
    }
    Ok(())
}

// Capturing only the immutable catalogue keeps this directly usable as
// WritePlan::validate's Fn(&Step) -> Result<(), VenueError> predicate.
pub(super) fn validate_write(markets: &[Market]) -> impl Fn(&Step) -> Result<(), VenueError> + '_ {
    move |step| match step {
        Step::Place { order, .. } => validate_order(order, markets),
        Step::Pair(rows) => {
            if rows.is_empty() || rows.len() > 2 {
                return Err(refused("Backpack paired write needs one or two rows"));
            }
            rows.iter()
                .try_for_each(|order| validate_order(order, markets))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::model::{Limits, Min, MinMax, Precision};

    fn market() -> Market {
        Market {
            id: "SOL_USDC_PERP".into(),
            symbol: "SOL_USDC_PERP".into(),
            pretty_symbol: None,
            base: "SOL".into(),
            quote: "USDC".into(),
            active: true,
            precision: Precision {
                amount: 0.1,
                price: 0.1,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.14,
                    max: 0.95,
                },
                price: Some(Min { min: 1.04 }),
                leverage: MinMax { min: 1., max: 10. },
            },
            contract_size: Some(1.),
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: Some(true),
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }
    }

    fn order(quantity: &str, price: &str) -> NativeOrder {
        NativeOrder {
            symbol: "SOL_USDC_PERP".into(),
            order_type: "Limit",
            quantity: Some(quantity.into()),
            price: Some(price.into()),
            ..Default::default()
        }
    }

    #[test]
    fn wire_quantities_validate_bounds_grid_and_finiteness() {
        let markets = [market()];
        let validate = validate_write(&markets);
        for quantity in ["1.0", "0.1", "0.15", "0", "NaN", "inf"] {
            let error = validate(&Step::Place {
                order: Box::new(order(quantity, "140.0")),
                optional: false,
            })
            .err();
            assert!(
                error.is_some_and(|error| error.kind == Some(ErrorKind::InvalidInput)
                    && error.outcome == Some(Outcome::NotSent)),
                "{quantity}"
            );
        }
    }

    #[test]
    fn wire_prices_validate_limit_and_every_protective_field() {
        let markets = [market()];
        let validate = validate_write(&markets);
        for field in 0..4 {
            for price in ["1.0", "1.15", "0", "NaN", "inf"] {
                let mut row = order("0.9", "140.0");
                match field {
                    0 => row.price = Some(price.into()),
                    1 => row.trigger_price = Some(price.into()),
                    2 => row.take_profit_trigger_price = Some(price.into()),
                    _ => row.stop_loss_trigger_price = Some(price.into()),
                }
                assert!(
                    validate(&Step::Place {
                        order: Box::new(row),
                        optional: true
                    })
                    .is_err()
                );
            }
        }
    }

    #[test]
    fn pair_count_and_last_wire_row_are_validated() {
        let markets = [market()];
        let validate = validate_write(&markets);
        let valid = order("0.9", "140.0");
        assert!(validate(&Step::Pair(vec![])).is_err());
        assert!(validate(&Step::Pair(vec![valid.clone(); 3])).is_err());
        assert!(validate(&Step::Pair(vec![valid.clone(), order("1.0", "140.0")])).is_err());
        assert!(validate(&Step::Pair(vec![valid.clone(), valid])).is_ok());
    }
}
