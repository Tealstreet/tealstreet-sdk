//! History records retain binary64 timestamps and venue net-fee conventions.
use crate::history_numbers::{income_to_number, order_to_number, order_to_optional_number};
use safe_cex_num::{JsCoercionError, js_number_string, js_string, js_truthy, parse_int};
use safe_cex_types::{
    ClosedPnlEntry, ClosedPnlEntryContracts, ClosedPnlEntryEntryPrice, ClosedPnlEntryExitPrice,
    ClosedPnlEntryFees, ClosedPnlEntryLeverage, ClosedPnlEntryNotional,
    ClosedPnlEntryUnrealizedPnl, DataProvenance, HistoricalOrder, IncomeCategory, IncomeRecord,
    OrderSide, OrderStatus, OrderType,
};
use serde_json::Value;

fn text(value: Option<&Value>, fallback: &str) -> Result<String, JsCoercionError> {
    value
        .filter(|v| !v.is_null())
        .map_or_else(|| Ok(fallback.into()), |v| js_string(Some(v)))
}
fn time(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    Ok(parse_int(&text(value, "0")?, Some(10)))
}
fn truthy_number(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}
fn check_row(row: &Value, field: &'static str) -> Result<(), JsCoercionError> {
    if row.is_null() {
        Err(JsCoercionError {
            name: "TypeError",
            message: field,
        })
    } else {
        Ok(())
    }
}

pub fn map_bitunix_trade_to_income_records(
    row: &Value,
) -> Result<Vec<IncomeRecord>, JsCoercionError> {
    check_row(row, "Cannot read properties of null (reading 'ctime')")?;
    let timestamp = time(row.get("ctime"))?;
    let symbol = text(row.get("symbol"), "")?;
    if !truthy_number(timestamp) || symbol.is_empty() {
        return Ok(vec![]);
    }
    let id = text(
        row.get("tradeId"),
        &format!("{}:{symbol}", js_number_string(timestamp)),
    )?;
    let realized = income_to_number(row.get("realizedPNL"))?;
    let fee = income_to_number(row.get("fee"))?;
    let asset = if js_truthy(row.get("marginCoin")) {
        js_string(row.get("marginCoin"))?
    } else {
        "USDT".into()
    };
    let mut records = vec![];
    if realized != 0.0 {
        records.push(IncomeRecord {
            symbol: symbol.clone(),
            income_type: "REALIZED_PNL".into(),
            income: realized,
            asset: asset.clone(),
            timestamp,
            tran_id: format!("{id}:pnl"),
            trade_id: Some(id.clone()),
            category: Some(IncomeCategory::RealizedPnl),
        });
    }
    if fee != 0.0 {
        records.push(IncomeRecord {
            symbol,
            income_type: if fee > 0.0 { "COMMISSION" } else { "REBATE" }.into(),
            income: -fee,
            asset,
            timestamp,
            tran_id: format!("{id}:fee"),
            trade_id: Some(id),
            category: Some(if fee > 0.0 {
                IncomeCategory::Commission
            } else {
                IncomeCategory::Rebate
            }),
        });
    }
    Ok(records)
}

pub fn map_bitunix_closed_position(
    row: &Value,
    account_name: &str,
) -> Result<Option<ClosedPnlEntry>, JsCoercionError> {
    check_row(row, "Cannot read properties of null (reading 'ctime')")?;
    let open_time = time(row.get("ctime"))?;
    let symbol = text(row.get("symbol"), "")?;
    if !truthy_number(open_time) || symbol.is_empty() {
        return Ok(None);
    }
    let close_price = income_to_number(row.get("closePrice"))?;
    let contracts = income_to_number(row.get("qty"))?;
    let closed = time(row.get("mtime"))?;
    Ok(Some(ClosedPnlEntry {
        provenance: DataProvenance::Venue,
        symbol,
        notional: ClosedPnlEntryNotional::Variant1(contracts * close_price),
        contracts: ClosedPnlEntryContracts::Variant1(contracts),
        entry_price: ClosedPnlEntryEntryPrice::Variant1(income_to_number(row.get("entryPrice"))?),
        exit_price: ClosedPnlEntryExitPrice::Variant1(close_price),
        unrealized_pnl: ClosedPnlEntryUnrealizedPnl::Variant1(income_to_number(
            row.get("realizedPNL"),
        )?),
        side: if text(row.get("side"), "")?.to_uppercase() == "SELL" {
            "short"
        } else {
            "long"
        }
        .into(),
        fees: ClosedPnlEntryFees::Variant1(income_to_number(row.get("fee"))?),
        time: open_time,
        closed_time: if truthy_number(closed) {
            closed
        } else {
            open_time
        },
        account: account_name.into(),
        leverage: ClosedPnlEntryLeverage::Variant0(income_to_number(row.get("leverage"))?),
    }))
}

pub fn map_bitunix_historical_order(
    row: &Value,
) -> Result<Option<HistoricalOrder>, JsCoercionError> {
    check_row(row, "Cannot read properties of null (reading 'orderId')")?;
    let id = text(row.get("orderId"), "")?;
    let symbol = text(row.get("symbol"), "")?;
    let created_time = time(row.get("ctime"))?;
    if id.is_empty() || symbol.is_empty() || !truthy_number(created_time) {
        return Ok(None);
    }
    let side = text(row.get("side"), "")?.to_uppercase();
    if !["BUY", "SELL"].contains(&side.as_str()) {
        return Ok(None);
    }
    let updated = time(row.get("mtime"))?;
    Ok(Some(HistoricalOrder {
        id,
        symbol,
        side: if side == "BUY" {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        },
        r#type: if text(row.get("orderType"), "")?.to_uppercase() == "MARKET" {
            OrderType::Market
        } else {
            OrderType::Limit
        },
        status: match text(row.get("status"), "")?.to_uppercase().as_str() {
            "INIT" | "NEW" | "PART_FILLED" => OrderStatus::Open,
            "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
            _ => OrderStatus::Closed,
        },
        price: order_to_optional_number(row.get("price"))?,
        average_price: order_to_optional_number(row.get("avgPrice"))?,
        amount: order_to_number(row.get("qty"))?,
        filled: order_to_number(row.get("tradeQty"))?,
        reduce_only: Some(
            row.get("reduceOnly") == Some(&Value::Bool(true))
                || text(row.get("reduceOnly"), "")?.to_lowercase() == "true",
        ),
        created_time,
        updated_time: truthy_number(updated).then_some(updated),
        position_side: None,
        realized_pnl: None,
        fees: None,
        fee_asset: None,
        rebate: None,
        rebate_asset: None,
    }))
}
