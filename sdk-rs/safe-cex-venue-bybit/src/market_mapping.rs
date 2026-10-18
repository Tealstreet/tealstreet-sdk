//! Pure mapping slice of fetchMarketsHelper; the request sequence lives with each class.
use safe_cex_num::js_retained_parse_float;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
};

#[derive(Clone, Copy)]
pub(crate) enum Family {
    Linear,
    Inverse,
    Spot,
}

pub(crate) fn map_market(
    raw: &Input,
    fees: &[Input],
    category: &str,
    family: Family,
) -> Result<Result<Market, String>, String> {
    let spot = matches!(family, Family::Spot);
    if raw.is_nullish() {
        return Err(format!(
            "Cannot read properties of {} (reading 'symbol')",
            if raw.is_null() { "null" } else { "undefined" }
        ));
    }
    let nested = |parent: &str, key: &str| -> Result<f64, String> {
        let object = raw
            .get(parent)
            .filter(|value| !value.is_nullish())
            .ok_or_else(|| {
                format!(
                    "Cannot read properties of {} (reading '{key}')",
                    if raw.get(parent).is_some_and(Input::is_null) {
                        "null"
                    } else {
                        "undefined"
                    }
                )
            })?;
        js_retained_parse_float(object.get(key)).map_err(|error| error.message.to_owned())
    };
    let mut fee = None;
    if !spot {
        for candidate in fees {
            if candidate.is_nullish() {
                return Err(format!(
                    "Cannot read properties of {} (reading 'symbol')",
                    if candidate.is_null() {
                        "null"
                    } else {
                        "undefined"
                    }
                ));
            }
            let same_symbol = match (candidate.get("symbol"), raw.get("symbol")) {
                (Some(Input::Array(_) | Input::Object(_)), _)
                | (_, Some(Input::Array(_) | Input::Object(_))) => false,
                (Some(left), Some(right))
                    if left.as_f64().is_some() && right.as_f64().is_some() =>
                {
                    left.as_f64() == right.as_f64()
                }
                (left, right) => {
                    left.unwrap_or(&Input::Undefined) == right.unwrap_or(&Input::Undefined)
                }
            };
            if same_symbol {
                fee = Some(candidate);
                break;
            }
        }
    }
    let precision = MarketPrecision {
        base: None,
        quote: None,
        amount: nested("lotSizeFilter", "qtyStep")?,
        price: nested("priceFilter", "tickSize")?,
    };
    let limits = MarketLimits {
        cost: None,
        amount: MarketLimitsAmount {
            min: nested("lotSizeFilter", "minOrderQty")?,
            max: (nested("lotSizeFilter", "maxOrderQty")?).into(),
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
        let value = js_retained_parse_float(raw.get("deliveryTime"))
            .map_err(|error| error.message.to_owned())?;
        value != 0. && !value.is_nan()
    };
    let taker_fee = fee
        .map(|f| {
            js_retained_parse_float(f.get("takerFeeRate")).map_err(|error| error.message.to_owned())
        })
        .transpose()?;
    let maker_fee = fee
        .map(|f| {
            js_retained_parse_float(f.get("makerFeeRate")).map_err(|error| error.message.to_owned())
        })
        .transpose()?;
    let Some(id) = raw
        .get("symbol")
        .and_then(Input::as_text)
        .and_then(|text| String::from_utf16(text).ok())
    else {
        let value = raw.get("symbol").map_or_else(
            || "undefined".into(),
            |value| {
                value
                    .json_stringify()
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "undefined".into())
            },
        );
        return Ok(Err(format!(
            "Refused Bybit market row id={value} symbol={value}: expected strings"
        )));
    };
    let (Some(base), Some(quote)) = (
        raw.get("baseCoin")
            .and_then(Input::as_text)
            .and_then(|text| String::from_utf16(text).ok()),
        raw.get("quoteCoin")
            .and_then(Input::as_text)
            .and_then(|text| String::from_utf16(text).ok()),
    ) else {
        let field = |key| {
            raw.get(key).map_or_else(
                || "undefined".into(),
                |value| {
                    value
                        .json_stringify()
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "undefined".into())
                },
            )
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
        runtime: Default::default(),
        id: id.clone(),
        symbol: id,
        base,
        quote,
        active: raw.get("status").is_some_and(|value| {
            value.text_equals("Trading") || (!spot && value.text_equals("PreLaunch"))
        }),
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
        info: Some(raw.clone()),
        pretty_symbol: None,
        contract_size: None,
        is_read_only: None,
        option: None,
    }))
}

#[cfg(test)]
mod tests;
