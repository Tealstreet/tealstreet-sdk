//! Execution normalization and the venue's bounded websocket execution cache.
use crate::response::{field_string, float, get, text as string, text, truthy};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{parse_float, parse_int};
use safe_cex_types::retained_json::RetainedJsonInput as J;
use safe_cex_types::{Execution, OrderSide};

// ts: ApexOmniExchange#normalizeApexSymbolKey
pub fn normalize_apex_symbol_key(value: &J) -> Result<String, String> {
    let value = if truthy(value) {
        string(value).map_err(|e| e.message)?
    } else {
        String::new()
    };
    let normalized: String = value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let normalized = normalized.strip_prefix("PERP").unwrap_or(&normalized);
    let normalized = normalized.strip_suffix("PERP").unwrap_or(normalized);
    Ok(normalized
        .strip_suffix("SWAP")
        .unwrap_or(normalized)
        .to_owned())
}

// ts: ApexOmniExchange#normalizeExecutionTimestampToSeconds
pub fn normalize_execution_timestamp_to_seconds(
    value: &J,
    date_parse: impl FnOnce(&str) -> Result<f64, ExchangeError>,
) -> Result<Option<f64>, ExchangeError> {
    if matches!(value, J::Undefined | J::Null) {
        return Ok(None);
    }
    if let J::Text(units) = value
        && (units.is_empty() || !units.iter().all(|b| (48..=57).contains(b)))
    {
        let value = field_string(value)?;
        let date = date_parse(&value)?;
        return Ok(date.is_finite().then(|| (date / 1000.0).floor()));
    }
    let parsed = parse_int(&text(value)?, Some(10));
    if !parsed.is_finite() || parsed <= 0.0 {
        return Ok(None);
    }
    Ok(Some(if parsed > 1_000_000_000_000.0 {
        (parsed / 1000.0).floor()
    } else {
        parsed
    }))
}

fn first_truthy<'a>(row: &'a J, names: &[&str]) -> &'a J {
    names
        .iter()
        .map(|name| get(row, name))
        .find(|value| truthy(value))
        .unwrap_or(&J::Undefined)
}

pub(crate) fn execution_side(value: &J) -> Result<Option<OrderSide>, ExchangeError> {
    let text = text(value)?;
    let value = safe_cex_num::js_trim(&text).to_uppercase();
    Ok(match value.as_str() {
        "BUY" | "B" | "LONG" => Some(OrderSide::Buy),
        "SELL" | "S" | "SHORT" => Some(OrderSide::Sell),
        _ => None,
    })
}

pub(crate) fn parse(value: &J) -> Result<f64, String> {
    Ok(parse_float(&string(value).map_err(|e| e.message)?))
}

pub(crate) fn row_symbol(row: &J) -> &J {
    first_truthy(
        row,
        &[
            "symbol",
            "s",
            "instId",
            "instrument",
            "market",
            "crossSymbolName",
            "marketSymbol",
        ],
    )
}

pub(crate) fn row_time(row: &J) -> &J {
    first_truthy(
        row,
        &[
            "createdAt",
            "fillTime",
            "createdTime",
            "timestamp",
            "ts",
            "timeStamp",
            "time",
            "updatedAt",
            "updatedTime",
            "tradeTime",
            "transactTime",
            "E",
            "t",
            "T",
        ],
    )
}

pub(crate) fn map_ws_execution(
    row: &J,
    symbol: &str,
    quote: Option<&str>,
    date_parse: impl FnOnce(&str) -> Result<f64, ExchangeError>,
) -> Result<Option<Execution>, ExchangeError> {
    let Some(side) = execution_side(first_truthy(row, &["side", "S"]))? else {
        return Ok(None);
    };
    let Some(timestamp) = normalize_execution_timestamp_to_seconds(row_time(row), date_parse)?
    else {
        return Ok(None);
    };
    if timestamp == 0.0 || timestamp.is_nan() {
        return Ok(None);
    }
    let price = float(first_truthy(
        row,
        &["price", "p", "fillPrice", "fillPx", "execPrice"],
    ))?;
    let qty = float(first_truthy(
        row,
        &[
            "fillSize",
            "execQty",
            "matchFillSize",
            "lastFillSize",
            "qty",
            "q",
            "size",
            "v",
            "amount",
        ],
    ))?;
    if !price.is_finite() || !qty.is_finite() || price <= 0.0 || qty <= 0.0 {
        return Ok(None);
    }
    let order = first_truthy(
        row,
        &["orderId", "orderID", "id", "i", "clientOrderId", "clientId"],
    );
    let order_id = if matches!(order, J::Undefined) {
        String::new()
    } else {
        text(order)?
    };
    let fee_value = first_truthy(
        row,
        &[
            "fee",
            "fillFee",
            "execFee",
            "cumSuccessFillFee",
            "cumMatchFillFee",
        ],
    );
    let fee = if matches!(fee_value, J::Undefined) {
        0.0
    } else {
        float(fee_value)?
    };
    let asset = first_truthy(
        row,
        &[
            "token",
            "feeToken",
            "feeAsset",
            "feeCurrency",
            "settleAssetId",
        ],
    );
    Ok(Some(Execution {
        timestamp,
        symbol: symbol.to_owned(),
        order_id: if order_id.is_empty() {
            None
        } else {
            Some(order_id)
        },
        price,
        qty,
        side,
        commission: fee.is_finite().then(|| fee.abs()),
        commission_asset: if matches!(asset, J::Undefined) {
            quote.map(str::to_owned)
        } else {
            Some(field_string(asset)?)
        },
        id: None,
        position_side: None,
        realized_pnl: None,
        maker: None,
        venue_reported_revenue: None,
        venue_reported_revenue_asset: None,
    }))
}
