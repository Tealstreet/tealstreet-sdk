use super::{Placement, SizeRules, positive};
use safe_cex_engine::model::Market;
use safe_cex_engine::{ErrorKind, Outcome, VenueError, num};

pub(super) struct PlacementWrite<'a> {
    pub payloads: &'a [Placement],
    pub family: u8,
    pub rules: &'a SizeRules,
    pub market: &'a Market,
}

pub(super) fn validate_placement_write(write: &PlacementWrite<'_>) -> Result<(), VenueError> {
    validate(write).map_err(|message| {
        VenueError::new(message)
            .kind(ErrorKind::InvalidInput)
            .outcome(Outcome::NotSent)
    })
}

fn validate(write: &PlacementWrite<'_>) -> Result<(), String> {
    let precision = write.market.precision;
    let first = write
        .payloads
        .first()
        .ok_or("Empty BloFin placement write")?;
    if write.family > 2 || !precision.is_tradable() {
        return Err("Invalid BloFin placement rules".into());
    }
    if (write.family == 0 && write.payloads.len() > 20)
        || (write.family != 0 && write.payloads.len() != 1)
    {
        return Err("BloFin placement write exceeds its native batch count".into());
    }
    for bound in [
        write.rules.min,
        write.rules.max_limit,
        write.rules.max_market,
    ] {
        positive(bound, "native size bound")?;
    }
    for payload in write.payloads {
        if payload.inst_id != write.market.id || payload.margin_mode != first.margin_mode {
            return Err("BloFin placement batch must share instrument and margin mode".into());
        }
        let shape_valid = match write.family {
            0 => match payload.order_type {
                Some("market") => true,
                Some("limit" | "post_only") => payload.price.is_some(),
                _ => false,
            },
            1 => {
                payload.order_type.is_none()
                    && (payload.protection.tp_trigger_price.is_some()
                        || payload.protection.sl_trigger_price.is_some())
            }
            _ => {
                payload.order_type == Some("trigger")
                    && payload.trigger_price.is_some()
                    && payload.order_price.is_some()
            }
        };
        if !shape_valid {
            return Err("Incomplete BloFin placement wire fields".into());
        }
        let size = payload.size.parse::<f64>().map_err(|e| e.to_string())?;
        if write.family != 1 || size != -1.0 {
            write
                .rules
                .validate(size, write.rules.maximum(payload), precision.amount)?;
        }
        let price = |text: &str, market_execution: bool| -> Result<(), String> {
            let value = text.parse::<f64>().map_err(|e| e.to_string())?;
            if market_execution && value == -1.0 {
                return Ok(());
            }
            positive(value, "wire price")?;
            if num::adjust(value, precision.price) != value {
                return Err("BloFin wire price violates tickSize".into());
            }
            Ok(())
        };
        for value in [&payload.price, &payload.trigger_price]
            .into_iter()
            .flatten()
        {
            price(value, false)?;
        }
        if let Some(value) = &payload.order_price {
            price(value, true)?;
        }
        for protection in std::iter::once(&payload.protection).chain(&payload.attach_algo_orders) {
            for value in [&protection.tp_trigger_price, &protection.sl_trigger_price]
                .into_iter()
                .flatten()
            {
                price(value, false)?;
            }
            for value in [&protection.tp_order_price, &protection.sl_order_price]
                .into_iter()
                .flatten()
            {
                price(value, true)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trading::Protection;

    fn payload() -> Placement {
        Placement {
            inst_id: "BTC-USDT".into(),
            margin_mode: "cross".into(),
            position_side: "net".into(),
            side: "buy",
            size: "4".into(),
            client_order_id: "control".into(),
            broker_id: None,
            order_type: Some("limit"),
            price: Some("60000".into()),
            reduce_only: None,
            trigger_price: None,
            trigger_price_type: None,
            order_price: None,
            protection: Protection::default(),
            attach_algo_orders: vec![],
        }
    }

    fn check(rows: &[Placement], family: u8) -> Result<(), VenueError> {
        let market = Market {
            id: "BTC-USDT".into(),
            symbol: "BTCUSDT".into(),
            base: "BTC".into(),
            quote: "USDT".into(),
            active: true,
            pretty_symbol: None,
            precision: safe_cex_engine::model::Precision {
                amount: 1.0,
                price: 0.1,
            },
            limits: safe_cex_engine::model::Limits {
                amount: safe_cex_engine::model::MinMax {
                    min: 0.004,
                    max: 5.0,
                },
                leverage: safe_cex_engine::model::MinMax {
                    min: 1.0,
                    max: 150.0,
                },
                price: None,
            },
            contract_size: Some(0.001),
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        };
        let rules = SizeRules {
            min: 4.0,
            max_limit: 5.0,
            max_market: 5.0,
        };
        let validate_fn: fn(&PlacementWrite<'_>) -> Result<(), VenueError> =
            validate_placement_write;
        validate_fn(&PlacementWrite {
            payloads: rows,
            family,
            rules: &rules,
            market: &market,
        })
    }

    fn batch(count: usize) -> Vec<Placement> {
        (0..count)
            .map(|index| {
                let mut row = payload();
                row.client_order_id = format!("control{index}");
                row
            })
            .collect()
    }

    #[test]
    fn batch_count_rejects_twenty_first_element() {
        let error = check(&batch(21), 0).expect_err("oversized batch");
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }

    #[test]
    fn invalid_last_instrument_rejects_homogeneous_batch() {
        let first = payload();
        let mut last = payload();
        last.inst_id = "ETH-USDT".into();
        let error = check(&[first, last], 0).expect_err("mixed instruments");
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }

    #[test]
    fn valid_twenty_element_batch_control() {
        assert!(check(&batch(20), 0).is_ok());
    }

    #[test]
    fn last_wire_size_and_prices_are_checked() {
        for invalid in ["size", "price", "trigger", "margin"] {
            let first = payload();
            let mut last = payload();
            match invalid {
                "size" => last.size = "3".into(),
                "price" => last.price = Some("60000.05".into()),
                "trigger" => last.protection.sl_trigger_price = Some("0".into()),
                _ => last.margin_mode = "isolated".into(),
            }
            let error = check(&[first, last], 0).expect_err("invalid last wire element");
            assert_eq!(error.outcome, Some(Outcome::NotSent));
            assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        }
    }

    #[test]
    fn single_write_routes_refuse_extra_rows_and_empty_writes() {
        for family in [0, 1, 2] {
            assert!(check(&[], family).is_err());
        }
        assert!(check(&[payload(), payload()], 1).is_err());
        assert!(check(&[payload(), payload()], 2).is_err());
    }
}
