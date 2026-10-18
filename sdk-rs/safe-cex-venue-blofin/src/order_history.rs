//! Function-for-function port of blofin.order-history.ts.
use safe_cex_num::{
    TableHit, js_table_lookup, numeric_ingress::JsNumberOutput, parse_float, parse_int,
};
use safe_cex_types::{
    BothPositionSide, ExecutionPositionSide, HistoricalOrder, OrderSide, OrderStatus, OrderType,
    PositionSide,
    js_coercion::{JsCoercionError, js_string},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Capture the complete open input before any JSON numeric projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HistoryInput {
    #[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]
    value: Value,
}
impl JsNumberOutput for HistoryInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {} passthrough { value });
    }
}
impl HistoryInput {
    pub(crate) fn from_retained(value: Value) -> Self {
        Self { value }
    }
    fn field(&self, key: &str) -> Result<Option<Self>, JsCoercionError> {
        if self.value.is_null() {
            return Err(JsCoercionError {
                name: "TypeError",
                message: "Cannot read properties of null (reading 'orderId')",
            });
        }
        Ok(self.value.get(key).cloned().map(|value| Self { value }))
    }
}
fn text(value: Option<&HistoryInput>, fallback: &str) -> Result<String, JsCoercionError> {
    match value
        .map(|value| &value.value)
        .filter(|value| !value.is_null())
    {
        Some(value) => js_string(Some(value)),
        None => Ok(fallback.into()),
    }
}

/// parseFloat(String(value ?? '')), with nonfinite results replaced by +0.
pub fn to_number(value: Option<&HistoryInput>) -> Result<f64, JsCoercionError> {
    let parsed = parse_float(&text(value, "")?);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

pub fn to_optional_number(value: Option<&HistoryInput>) -> Result<Option<f64>, JsCoercionError> {
    let parsed = parse_float(&text(value, "")?);
    Ok((parsed.is_finite() && parsed != 0.0).then_some(parsed))
}

const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("limit", OrderType::Limit),
    ("market", OrderType::Market),
    ("post_only", OrderType::Limit),
    ("fok", OrderType::Limit),
    ("ioc", OrderType::Limit),
    ("trigger", OrderType::Conditional),
];
const ORDER_STATUS: &[(&str, OrderStatus)] = &[
    ("live", OrderStatus::Open),
    ("partially_filled", OrderStatus::Open),
    ("filled", OrderStatus::Closed),
    ("canceled", OrderStatus::Canceled),
    ("cancelled", OrderStatus::Canceled),
];
fn mapped<T: Copy>(table: &[(&str, T)], key: &str, fallback: T) -> Result<T, JsCoercionError> {
    match js_table_lookup(table, key) {
        TableHit::Mapped(value) => Ok(*value),
        TableHit::Missing => Ok(fallback),
        TableHit::Inherited(_) => Err(JsCoercionError {
            name: "RepresentationError",
            message: "Inherited history table value is outside the shared order enum",
        }),
    }
}

pub fn map_blofin_historical_order(
    row: &HistoryInput,
    mut resolve_symbol: impl FnMut(&str) -> Result<Option<String>, safe_cex_types::StoreReadError>,
) -> Result<Option<HistoricalOrder>, crate::reads::MappingError> {
    let id = text(row.field("orderId")?.as_ref(), "")?;
    let inst_id = text(row.field("instId")?.as_ref(), "")?;
    let created_time = parse_int(&text(row.field("createTime")?.as_ref(), "0")?, Some(10));
    if id.is_empty() || inst_id.is_empty() || created_time == 0.0 || created_time.is_nan() {
        return Ok(None);
    }
    let side = text(row.field("side")?.as_ref(), "")?.to_lowercase();
    if side != "buy" && side != "sell" {
        return Ok(None);
    }
    let position_side = text(row.field("positionSide")?.as_ref(), "")?.to_lowercase();
    let symbol = resolve_symbol(&inst_id)?
        .filter(|s| !s.is_empty())
        .unwrap_or(inst_id);
    let order_type = mapped(
        ORDER_TYPE,
        &text(row.field("orderType")?.as_ref(), "")?.to_lowercase(),
        OrderType::Limit,
    )?;
    let status = mapped(
        ORDER_STATUS,
        &text(row.field("state")?.as_ref(), "")?.to_lowercase(),
        OrderStatus::Closed,
    )?;
    let price = to_optional_number(row.field("price")?.as_ref())?;
    let average_price = to_optional_number(row.field("averagePrice")?.as_ref())?;
    let amount = to_number(row.field("size")?.as_ref())?;
    let filled = to_number(row.field("filledSize")?.as_ref())?;
    let reduce_only = text(row.field("reduceOnly")?.as_ref(), "")?.to_lowercase() == "true"
        || row
            .field("reduceOnly")?
            .is_some_and(|v| v.value == Value::Bool(true));
    let position_side = if position_side == "net" {
        ExecutionPositionSide::Both(BothPositionSide::Both)
    } else {
        ExecutionPositionSide::Side(if position_side == "short" {
            PositionSide::Short
        } else {
            PositionSide::Long
        })
    };
    let updated = parse_int(&text(row.field("updateTime")?.as_ref(), "0")?, Some(10));
    Ok(Some(HistoricalOrder {
        id,
        symbol,
        side: if side == "buy" {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        },
        r#type: order_type,
        status,
        price,
        average_price,
        amount,
        filled,
        reduce_only: Some(reduce_only),
        position_side: Some(position_side),
        created_time,
        updated_time: (updated != 0.0 && !updated.is_nan()).then_some(updated),
        realized_pnl: None,
        fees: None,
        fee_asset: None,
        rebate: None,
        rebate_asset: None,
    }))
}
