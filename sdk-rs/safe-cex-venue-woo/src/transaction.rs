//! Legacy transaction rows, retaining native numeric observations before projection.
use crate::{response, utils::map_woo_hedge_transaction_side};
use safe_cex_num::{
    TableHit,
    get_key::{GetKeyObject, GetKeyValue, v},
    js_number, js_string, js_truthy,
};
use safe_cex_types::{BothPositionSide, ExecutionPositionSide, OrderSide, PositionSide};
use serde_json::{Map, Value};

pub struct ParsedWooTransaction {
    pub symbol: String,
    pub side: OrderSide,
    pub price: f64,
    pub qty: f64,
    pub timestamp_milliseconds: f64,
    pub timestamp_seconds: f64,
    pub order_id: Option<String>,
    pub fill_id: Option<String>,
    pub maker: Option<bool>,
    pub commission: Option<f64>,
    pub commission_asset: Option<Value>,
    pub realized_pnl: Option<f64>,
    pub position_side: ExecutionPositionSide,
}
fn field<'a>(row: &'a Map<String, Value>, key: &str) -> Result<Option<&'a Value>, String> {
    match v(Some(GetKeyObject::Json(row)), key) {
        TableHit::Mapped(GetKeyValue::Json(value)) => Ok(Some(value)),
        TableHit::Mapped(GetKeyValue::Undefined) | TableHit::Missing => Ok(None),
        TableHit::Inherited(_) => Err("transaction field cannot carry an inherited member".into()),
    }
}
fn first<'a>(
    row: &'a Map<String, Value>,
    keys: &[&str],
    truthy: bool,
) -> Result<Option<&'a Value>, String> {
    let mut result = None;
    for key in keys {
        result = field(row, key)?;
        if if truthy {
            js_truthy(result)
        } else {
            result.is_some_and(|v| !v.is_null())
        } {
            return Ok(result);
        }
    }
    Ok(result)
}
fn number(value: Option<&Value>) -> Result<f64, String> {
    js_number(value).map_err(|e| e.message.to_owned())
}
fn text(value: Option<&Value>) -> Result<String, String> {
    js_string(value).map_err(|e| e.message.to_owned())
}
fn id(value: Option<&Value>) -> Result<Option<String>, String> {
    if matches!(value, None | Some(Value::Null)) || value.and_then(Value::as_str) == Some("") {
        return Ok(None);
    }
    text(value).map(Some)
}
// ts: WOOXExchange#parseTransactionHistoryRow
pub fn parse_transaction_history_row(
    row: &Map<String, Value>,
) -> Result<Option<ParsedWooTransaction>, String> {
    let Some(symbol) = response::normalize_transaction_symbol(field(row, "symbol")?) else {
        return Ok(None);
    };
    if symbol.is_empty() {
        return Ok(None);
    }
    let raw_side = field(row, "side")?;
    let raw_side = if js_truthy(raw_side) {
        text(raw_side)?
    } else {
        String::new()
    }
    .to_uppercase();
    let hedge = map_woo_hedge_transaction_side(&raw_side);
    let side = match hedge {
        Some((side, _)) => side,
        None => match raw_side.as_str() {
            "BUY" => OrderSide::Buy,
            "SELL" => OrderSide::Sell,
            _ => return Ok(None),
        },
    };
    let price = number(first(
        row,
        &[
            "executedPrice",
            "price",
            "averageExecutedPrice",
            "average_executed_price",
            "lastPrice",
        ],
        false,
    )?)?;
    let qty = number(first(
        row,
        &["executedQuantity", "quantity", "executed", "lastQty"],
        false,
    )?)?;
    let timestamp = response::normalize_timestamp_milliseconds(first(
        row,
        &[
            "executedTimestamp",
            "executed_time",
            "tradeTime",
            "timestamp",
            "created_time",
            "createdTime",
            "updated_time",
            "updatedTime",
        ],
        true,
    )?)
    .map_err(|e| e.message.to_owned())?;
    if !price.is_finite() || price <= 0.0 || !qty.is_finite() || qty <= 0.0 || timestamp.is_none() {
        return Ok(None);
    }
    let timestamp_milliseconds = timestamp.unwrap_or_default();
    let order_id = id(first(row, &["orderId", "order_id", "id"], false)?)?;
    let fill_id = id(first(row, &["id", "tradeId", "trade_id"], false)?)?;
    let fee = number(first(row, &["fee", "total_fee"], false)?)?;
    let commission = fee.is_finite().then_some(fee);
    let commission_asset = first(row, &["feeAsset", "fee_asset"], false)?;
    let raw_pnl = first(row, &["realizedPnl", "realized_pnl"], false)?;
    let pnl = if matches!(raw_pnl, None | Some(Value::Null)) {
        f64::NAN
    } else {
        number(raw_pnl)?
    };
    let realized_pnl = pnl.is_finite().then_some(pnl);
    let raw_maker = first(row, &["isMaker", "is_maker"], false)?;
    let maker = if matches!(raw_maker, None | Some(Value::Null)) {
        None
    } else {
        let n = number(raw_maker)?;
        Some(n != 0.0 && !n.is_nan())
    };
    let raw_position = first(row, &["positionSide", "position_side"], false)?;
    let raw_position = if matches!(raw_position, None | Some(Value::Null)) {
        String::new()
    } else {
        text(raw_position)?
    }
    .to_uppercase();
    let position_side = match raw_position.as_str() {
        "LONG" => ExecutionPositionSide::Side(PositionSide::Long),
        "SHORT" => ExecutionPositionSide::Side(PositionSide::Short),
        _ => hedge.map_or(
            ExecutionPositionSide::Both(BothPositionSide::Both),
            |(_, side)| ExecutionPositionSide::Side(side),
        ),
    };
    Ok(Some(ParsedWooTransaction {
        symbol,
        side,
        price,
        qty,
        timestamp_milliseconds,
        timestamp_seconds: (timestamp_milliseconds / 1000.0).floor(),
        order_id,
        fill_id,
        maker,
        commission,
        commission_asset: commission_asset
            .filter(|value| js_truthy(Some(value)))
            .cloned(),
        realized_pnl,
        position_side,
    }))
}
