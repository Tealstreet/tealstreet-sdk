//! WEEX execution and historical-trade projections into the shared models.
use crate::types::POSITION_SIDE;
use crate::utils::{
    WeexUtilityError, WeexUtilityInput, normalize_order_side, normalize_timestamp_to_ms,
    normalize_timestamp_to_seconds, to_integer_string, to_number,
};
use safe_cex_num::{js_string, js_truthy};
use safe_cex_types::{BothPositionSide, Execution, ExecutionPositionSide, HistoricalTrade, Market};
use serde_json::Value;

#[derive(Debug)]
pub(crate) struct ExecutionProjectionRefusal {
    pub error: WeexUtilityError,
    pub value: Value,
}

#[derive(Debug)]
pub(crate) enum ExecutionRowError {
    Source(WeexUtilityError),
    Projection(ExecutionProjectionRefusal),
}

fn optional_bool(value: Option<&Value>, field: &str) -> Result<Option<bool>, WeexUtilityError> {
    value
        .map(|value| {
            value.as_bool().ok_or_else(|| WeexUtilityError {
                name: "TypeError",
                message: format!("{field} requires a boolean"),
            })
        })
        .transpose()
}

pub fn normalize_execution(
    raw: &WeexUtilityInput,
    market: Option<&Market>,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<Option<Execution>, WeexUtilityError> {
    normalize_execution_row(raw, market, local_to_utc).map_err(|error| match error {
        ExecutionRowError::Source(error) => error,
        ExecutionRowError::Projection(refusal) => refusal.error,
    })
}

// Both the public utility and the constructed binding use this one pipeline.
// Only closed output-field refusals are recoverable at the batch boundary.
pub(crate) fn normalize_execution_row(
    raw: &WeexUtilityInput,
    market: Option<&Market>,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<Option<Execution>, ExecutionRowError> {
    normalize_execution_fields(raw, market, local_to_utc)
        .map_err(ExecutionRowError::Source)?
        .map(|execution| {
            project_execution(execution, raw, market).map_err(ExecutionRowError::Projection)
        })
        .transpose()
}

// Each caller finishes source coercions before admitting the closed model fields.
fn normalize_execution_fields(
    raw: &WeexUtilityInput,
    market: Option<&Market>,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<Option<Execution>, WeexUtilityError> {
    let value = raw.as_value();
    if value.is_null() {
        return Err(WeexUtilityError {
            name: "TypeError",
            message: "Cannot read properties of null (reading 'symbol')".to_owned(),
        });
    }
    let symbol = match raw.pick_retained(&["symbol"]) {
        Some(symbol) => js_string(Some(symbol.as_value()))?,
        None => market.map_or_else(String::new, |market| market.symbol.clone()),
    };
    let side = normalize_order_side(raw.pick_retained(&["side", "orderSide"]).as_ref())?;
    let price = to_number(raw.pick_retained(&["price"]).as_ref(), None)?;
    let qty = to_number(raw.pick_retained(&["qty", "fillSize"]).as_ref(), None)?.abs();
    let timestamp = normalize_timestamp_to_seconds(
        raw.pick_retained(&["time", "createdTime", "updatedTime"])
            .as_ref(),
        local_to_utc,
    )?;
    if symbol.is_empty() || side.is_none() || price <= 0.0 || qty <= 0.0 || timestamp == 0.0 {
        return Ok(None);
    }
    let fill_id = raw.pick_retained(&["tradeId", "fillId", "id"]);
    let id = if js_truthy(fill_id.as_ref().map(WeexUtilityInput::as_value)) {
        Some(to_integer_string(fill_id.as_ref())?)
    } else {
        None
    };
    let order_id = if js_truthy(value.get("orderId")) {
        Some(to_integer_string(raw.pick_retained(&["orderId"]).as_ref())?)
    } else {
        None
    };
    let realized_pnl = to_number(
        raw.pick_retained(&["realizedPnl", "realizePnl"]).as_ref(),
        None,
    )?;
    let position_key = if js_truthy(value.get("positionSide")) {
        js_string(value.get("positionSide"))?.to_uppercase()
    } else {
        String::new()
    };
    let position_side = POSITION_SIDE
        .iter()
        .find(|(key, _)| *key == position_key)
        .map_or(
            ExecutionPositionSide::Both(BothPositionSide::Both),
            |(_, side)| ExecutionPositionSide::Side(*side),
        );
    let commission = to_number(
        raw.pick_retained(&["commission", "fillFee"]).as_ref(),
        Some(f64::NAN),
    )?
    .abs();
    let commission = if commission != 0.0 && !commission.is_nan() {
        Some(commission)
    } else {
        None
    };
    Ok(Some(Execution {
        id,
        timestamp,
        symbol,
        order_id,
        price,
        qty,
        side: side.unwrap(),
        commission,
        commission_asset: None,
        position_side: Some(position_side),
        realized_pnl: Some(realized_pnl),
        maker: None,
        venue_reported_revenue: None,
        venue_reported_revenue_asset: None,
    }))
}

fn project_execution(
    mut execution: Execution,
    raw: &WeexUtilityInput,
    market: Option<&Market>,
) -> Result<Execution, ExecutionProjectionRefusal> {
    execution.maker = match raw.as_value().get("maker") {
        Some(value) => optional_bool(Some(value), "Execution.maker").map_err(|error| {
            ExecutionProjectionRefusal {
                error,
                value: value.clone(),
            }
        })?,
        None => None,
    };
    execution.commission_asset = match raw.pick_retained(&["commissionAsset", "coin"]) {
        Some(asset) => Some(
            asset
                .as_value()
                .as_str()
                .ok_or_else(|| ExecutionProjectionRefusal {
                    error: WeexUtilityError {
                        name: "TypeError",
                        message: "Execution.commissionAsset requires a string".to_owned(),
                    },
                    value: asset.as_value().clone(),
                })?
                .to_owned(),
        ),
        None => market.map(|market| market.quote.clone()),
    };
    Ok(execution)
}

pub fn normalize_historical_trade(
    raw: &WeexUtilityInput,
    market: Option<&Market>,
    mut local_to_utc: impl FnMut(f64) -> f64,
) -> Result<Option<HistoricalTrade>, WeexUtilityError> {
    let Some(execution) = normalize_execution_fields(raw, market, &mut local_to_utc)? else {
        return Ok(None);
    };
    if execution.order_id.as_deref().is_none_or(str::is_empty) {
        return Ok(None);
    }
    let quote_qty = to_number(raw.pick_retained(&["quoteQty"]).as_ref(), None)?;
    let time = normalize_timestamp_to_ms(
        raw.pick_retained(&["time", "createdTime", "updatedTime"])
            .as_ref(),
        local_to_utc,
    )?;
    let buyer = optional_bool(raw.as_value().get("buyer"), "HistoricalTrade.buyer")?;
    let execution = project_execution(execution, raw, market).map_err(|refusal| refusal.error)?;
    Ok(Some(HistoricalTrade {
        id: execution.id,
        symbol: execution.symbol,
        order_id: execution.order_id.unwrap(),
        price: execution.price,
        qty: execution.qty,
        quote_qty: Some(quote_qty),
        realized_pnl: execution.realized_pnl,
        position_side: execution
            .position_side
            .unwrap_or(ExecutionPositionSide::Both(BothPositionSide::Both)),
        side: execution.side,
        buyer,
        maker: execution.maker,
        commission: execution.commission,
        commission_asset: execution.commission_asset,
        time,
    }))
}
