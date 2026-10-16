#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Pure market mapping slice awaits fetch orchestration"
    )
)]
//! Pure mapping slice of fetchMarketsHelper; request scheduling remains pending.
use crate::exchange_state::parse_float;
use safe_cex_types::{
    Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
};
use serde_json::Value;

#[derive(Clone, Copy)]
pub(crate) enum Family {
    Linear,
    Inverse,
    Spot,
}

pub(crate) fn map_market(
    raw: &Value,
    fees: &[Value],
    category: &str,
    family: Family,
) -> Result<Market, String> {
    let spot = matches!(family, Family::Spot);
    let text = |key: &str| {
        raw.get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("Bybit typed Market requires string {key}"))
    };
    let nested = |parent: &str, key: &str| -> Result<f64, String> {
        let object = raw
            .get(parent)
            .filter(|value| !value.is_null())
            .ok_or_else(|| {
                format!(
                    "Cannot read properties of {} (reading '{key}')",
                    if raw.get(parent).is_some() {
                        "null"
                    } else {
                        "undefined"
                    }
                )
            })?;
        parse_float(object.get(key)).map_err(str::to_owned)
    };
    let mut fee = None;
    if !spot {
        for candidate in fees {
            if candidate.is_null() {
                return Err("Cannot read properties of null (reading 'symbol')".into());
            }
            if candidate.get("symbol") == raw.get("symbol") {
                fee = Some(candidate);
                break;
            }
        }
    }
    // Preserve all instrument metadata, notably symbolType=innovation (crypto).
    // The shared instrument classifier, not marketRegion, owns classification.
    Ok(Market {
        id: text("symbol")?,
        symbol: text("symbol")?,
        base: text("baseCoin")?,
        quote: text("quoteCoin")?,
        active: raw["status"] == "Trading" || (!spot && raw["status"] == "PreLaunch"),
        precision: MarketPrecision {
            amount: nested("lotSizeFilter", "qtyStep")?,
            price: nested("priceFilter", "tickSize")?,
        },
        limits: MarketLimits {
            amount: MarketLimitsAmount {
                min: nested("lotSizeFilter", "minOrderQty")?,
                max: nested("lotSizeFilter", "maxOrderQty")?,
            },
            leverage: MarketLimitsLeverage {
                min: if spot {
                    1.
                } else {
                    nested("leverageFilter", "minLeverage")?
                },
                max: if spot {
                    1.
                } else {
                    nested("leverageFilter", "maxLeverage")?
                },
            },
            price: None,
        },
        is_expiring: Some(if spot {
            false
        } else {
            let value = parse_float(raw.get("deliveryTime"))?;
            value != 0. && !value.is_nan()
        }),
        is_inverse: (!matches!(family, Family::Linear)).then_some(category == "inverse"),
        is_spot: spot.then_some(true),
        is_perpetual: None,
        taker_fee: fee
            .map(|f| parse_float(f.get("takerFeeRate")))
            .transpose()?,
        maker_fee: fee
            .map(|f| parse_float(f.get("makerFeeRate")))
            .transpose()?,
        info: Some(raw.clone()),
        pretty_symbol: None,
        contract_size: None,
        is_read_only: None,
        option: None,
    })
}

#[cfg(test)]
mod tests;
