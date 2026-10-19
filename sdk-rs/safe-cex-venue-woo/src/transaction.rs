//! Legacy transaction rows, retaining native numeric observations before projection.
use crate::utils::{map_woo_hedge_transaction_side, normalize_symbol};
use safe_cex_num::{js_retained_number as js_number, js_retained_truthy as js_truthy};
use safe_cex_types::{
    BothPositionSide, ExecutionPositionSide, OrderSide, PositionSide,
    retained_json::RetainedJsonInput as Input,
};

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
    pub commission_asset: Option<Input>,
    pub realized_pnl: Option<f64>,
    pub position_side: ExecutionPositionSide,
}
fn field<'a>(row: &'a Input, key: &str) -> Option<&'a Input> {
    for candidate in [
        key.to_owned(),
        safe_cex_num::lodash_snake_case(key),
        safe_cex_num::lodash_camel_case(key),
    ] {
        if let Some(value) = row.get(&candidate) {
            return Some(value);
        }
    }
    None
}
fn first<'a>(row: &'a Input, keys: &[&str], truthy: bool) -> Option<&'a Input> {
    let mut result = None;
    for key in keys {
        result = field(row, key);
        if if truthy {
            result.is_some_and(js_truthy)
        } else {
            result.is_some_and(|v| !v.is_nullish())
        } {
            return result;
        }
    }
    result
}
fn number(value: Option<&Input>) -> Result<f64, String> {
    js_number(value).map_err(|e| e.message.to_owned())
}
fn text(value: Option<&Input>) -> Result<String, String> {
    let units =
        safe_cex_types::js_coercion::js_retained_string_utf16(value.unwrap_or(&Input::Undefined))
            .map_err(|e| e.message.to_owned())?;
    Ok(String::from_utf16_lossy(&units))
}
fn id(value: Option<&Input>) -> Result<Option<String>, String> {
    if value.is_none_or(Input::is_nullish) || value.is_some_and(|v| v.text_equals("")) {
        return Ok(None);
    }
    text(value).map(Some)
}
fn timestamp(raw: Option<&Input>) -> Result<Option<f64>, String> {
    if raw.is_none_or(Input::is_nullish) || raw.is_some_and(|v| v.text_equals("")) {
        return Ok(None);
    }
    let value = number(raw)?;
    Ok((value.is_finite() && value >= 0.0).then(|| {
        if value >= 1_000_000_000_000.0 {
            value.floor()
        } else {
            (value * 1000.0).floor()
        }
    }))
}
// ts: WOOXExchange#parseTransactionHistoryRow
pub fn parse_transaction_history_row(row: &Input) -> Result<Option<ParsedWooTransaction>, String> {
    if !row.is_nullish() && row.as_object().is_none() && row.as_array().is_none() {
        return Err(format!(
            "Cannot use 'in' operator to search for 'symbol' in {}",
            text(Some(row))?
        ));
    }
    let Some(units) = field(row, "symbol")
        .and_then(Input::as_text)
        .filter(|s| !s.is_empty())
    else {
        return Ok(None);
    };
    let symbol = normalize_symbol(&String::from_utf16_lossy(units).replace('/', "_"));
    if symbol.is_empty() {
        return Ok(None);
    }
    let raw_side = field(row, "side");
    let raw_side = if raw_side.is_some_and(js_truthy) {
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
    ))?;
    let qty = number(first(
        row,
        &["executedQuantity", "quantity", "executed", "lastQty"],
        false,
    ))?;
    let timestamp = timestamp(first(
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
    ))?;
    if !price.is_finite() || price <= 0.0 || !qty.is_finite() || qty <= 0.0 || timestamp.is_none() {
        return Ok(None);
    }
    let timestamp_milliseconds = timestamp.unwrap_or_default();
    let order_id = id(first(row, &["orderId", "order_id", "id"], false))?;
    let fill_id = id(first(row, &["id", "tradeId", "trade_id"], false))?;
    let fee = number(first(row, &["fee", "total_fee"], false))?;
    let commission = fee.is_finite().then_some(fee);
    let commission_asset = first(row, &["feeAsset", "fee_asset"], false);
    let raw_pnl = first(row, &["realizedPnl", "realized_pnl"], false);
    let pnl = if raw_pnl.is_none_or(Input::is_nullish) {
        f64::NAN
    } else {
        number(raw_pnl)?
    };
    let realized_pnl = pnl.is_finite().then_some(pnl);
    let raw_maker = first(row, &["isMaker", "is_maker"], false);
    let maker = if raw_maker.is_none_or(Input::is_nullish) {
        None
    } else {
        let n = number(raw_maker)?;
        Some(n != 0.0 && !n.is_nan())
    };
    let raw_position = first(row, &["positionSide", "position_side"], false);
    let raw_position = if raw_position.is_none_or(Input::is_nullish) {
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
        commission_asset: commission_asset.filter(|value| js_truthy(value)).cloned(),
        realized_pnl,
        position_side,
    }))
}
