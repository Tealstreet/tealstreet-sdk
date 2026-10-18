//! Pure mapping slice of fetchMarketsHelper; the request sequence lives with each class.
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
) -> Result<Result<Market, String>, String> {
    let spot = matches!(family, Family::Spot);
    if raw.is_null() {
        return Err("Cannot read properties of null (reading 'symbol')".into());
    }
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
            let same_symbol = match (candidate.get("symbol"), raw.get("symbol")) {
                (Some(Value::Array(_) | Value::Object(_)), _)
                | (_, Some(Value::Array(_) | Value::Object(_))) => false,
                (Some(Value::Number(left)), Some(Value::Number(right))) => {
                    left.as_f64() == right.as_f64()
                }
                (left, right) => left == right,
            };
            if same_symbol {
                fee = Some(candidate);
                break;
            }
        }
    }
    let precision = MarketPrecision {
        amount: nested("lotSizeFilter", "qtyStep")?,
        price: nested("priceFilter", "tickSize")?,
    };
    let limits = MarketLimits {
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
    };
    let is_expiring = if spot {
        false
    } else {
        let value = parse_float(raw.get("deliveryTime"))?;
        value != 0. && !value.is_nan()
    };
    let taker_fee = fee
        .map(|f| parse_float(f.get("takerFeeRate")))
        .transpose()?;
    let maker_fee = fee
        .map(|f| parse_float(f.get("makerFeeRate")))
        .transpose()?;
    let Some(id) = raw.get("symbol").and_then(Value::as_str) else {
        let value = raw
            .get("symbol")
            .map_or_else(|| "undefined".into(), Value::to_string);
        return Ok(Err(format!(
            "Refused Bybit market row id={value} symbol={value}: expected strings"
        )));
    };
    let (Some(base), Some(quote)) = (
        raw.get("baseCoin").and_then(Value::as_str),
        raw.get("quoteCoin").and_then(Value::as_str),
    ) else {
        let field = |key| {
            raw.get(key)
                .map_or_else(|| "undefined".into(), Value::to_string)
        };
        return Ok(Err(format!(
            "Refused Bybit market row id={}: base={} quote={}: expected strings",
            field("symbol"),
            field("baseCoin"),
            field("quoteCoin")
        )));
    };
    // Preserve raw instrument metadata independently of typed asset admission.
    Ok(Ok(Market {
        id: id.into(),
        symbol: id.into(),
        base: base.into(),
        quote: quote.into(),
        active: raw["status"] == "Trading" || (!spot && raw["status"] == "PreLaunch"),
        precision,
        limits,
        is_expiring: Some(is_expiring),
        // The bybit connection marks only inverse rows; the other families always say which.
        is_inverse: match family {
            Family::Linear => (category == "inverse").then_some(true),
            _ => Some(category == "inverse"),
        },
        is_spot: spot.then_some(true),
        is_perpetual: None,
        taker_fee,
        maker_fee,
        info: Some(raw.clone().into()),
        pretty_symbol: None,
        contract_size: None,
        is_read_only: None,
        option: None,
    }))
}

#[cfg(test)]
mod tests;
