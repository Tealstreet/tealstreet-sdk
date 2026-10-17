//! Pure Bybit order-history, closed-PnL and transaction-log source mappings.
use crate::api::truthy;
use safe_cex_num::{js_string, js_trim, math_round, parse_float, parse_int};
use safe_cex_types::*;
use serde_json::{Map, Value};

fn text(value: Option<&Value>) -> Result<String, &'static str> {
    js_string(
        value
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )
    .map_err(|e| e.message)
}
// Shared identical toNumber declarations in all three maintained source files.
pub fn to_number(value: Option<&Value>) -> Result<f64, &'static str> {
    let parsed = parse_float(&text(value)?);
    Ok(if parsed.is_finite() { parsed } else { 0. })
}
pub fn to_optional_number(value: Option<&Value>) -> Result<Option<f64>, &'static str> {
    let raw = text(value)?;
    let raw = js_trim(&raw);
    if raw.is_empty() {
        return Ok(None);
    }
    let parsed = parse_float(raw);
    Ok((parsed.is_finite() && parsed != 0.).then_some(parsed))
}

fn inherited(key: &str) -> bool {
    matches!(
        key,
        "__proto__"
            | "constructor"
            | "toString"
            | "toLocaleString"
            | "valueOf"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "__defineGetter__"
            | "__defineSetter__"
            | "__lookupGetter__"
            | "__lookupSetter__"
    )
}
fn check_table_key(key: &str) -> Result<(), &'static str> {
    if inherited(key) {
        Err("Bybit native enum cannot retain inherited table value")
    } else {
        Ok(())
    }
}
fn order_type(key: &str) -> Result<Option<OrderType>, &'static str> {
    check_table_key(key)?;
    Ok(match key {
        "Limit" => Some(OrderType::Limit),
        "Market" => Some(OrderType::Market),
        "Stop" | "StopLoss" | "PartialStopLoss" => Some(OrderType::StopLoss),
        "TakeProfit" | "PartialTakeProfit" => Some(OrderType::TakeProfit),
        "TrailingStop" => Some(OrderType::TrailingStopLoss),
        _ => None,
    })
}
pub fn map_bybit_historical_order(
    row: &Map<String, Value>,
) -> Result<Option<HistoricalOrder>, &'static str> {
    let id = text(row.get("orderId"))?;
    let symbol = text(row.get("symbol"))?;
    let created_time = parse_int(&text(row.get("createdTime"))?, Some(10));
    if id.is_empty() || symbol.is_empty() || created_time == 0. || created_time.is_nan() {
        return Ok(None);
    }
    let side = text(row.get("side"))?;
    let side = if inherited(&side) {
        Err("Bybit native enum cannot retain inherited table value")
    } else {
        Ok(match side.as_str() {
            "Buy" => OrderSide::Buy,
            "Sell" => OrderSide::Sell,
            _ => return Ok(None),
        })
    };
    let stop_type = text(row.get("stopOrderType"))?;
    let stop_type = js_trim(&stop_type);
    let kind = if stop_type.is_empty() {
        Ok(None)
    } else {
        order_type(stop_type)
    };
    // An inherited stop value is truthy and suppresses orderType coercion.
    let kind = match kind {
        Ok(Some(kind)) => Ok(kind),
        Err(error) => Err(error),
        Ok(None) => order_type(&text(row.get("orderType"))?).map(|v| v.unwrap_or(OrderType::Limit)),
    };
    let status = text(row.get("orderStatus"))?;
    let status = check_table_key(&status).map(|()| match status.as_str() {
        "Created" | "New" | "Active" | "Untriggered" | "PartiallyFilled" => OrderStatus::Open,
        "PendingCancel" | "Cancelled" => OrderStatus::Canceled,
        _ => OrderStatus::Closed,
    });
    let price = to_optional_number(row.get("price"))?;
    let average_price = to_optional_number(row.get("avgPrice"))?;
    let amount = to_number(row.get("qty"))?;
    let filled = to_number(row.get("cumExecQty"))?;
    let reduce_only = row.get("reduceOnly").is_some_and(truthy);
    let idx = text(row.get("positionIdx"))?;
    let position_side = check_table_key(&idx).map(|()| match idx.as_str() {
        "0" => Some(ExecutionPositionSide::Both(BothPositionSide::Both)),
        "1" => Some(ExecutionPositionSide::Side(PositionSide::Long)),
        "2" => Some(ExecutionPositionSide::Side(PositionSide::Short)),
        _ => None,
    });
    let updated = parse_int(&text(row.get("updatedTime"))?, Some(10));
    Ok(Some(HistoricalOrder {
        id,
        symbol,
        side: side?,
        r#type: kind?,
        status: status?,
        price,
        average_price,
        amount,
        filled,
        reduce_only: Some(reduce_only),
        position_side: position_side?,
        created_time,
        updated_time: (updated != 0. && !updated.is_nan()).then_some(updated),
        realized_pnl: None,
        fees: None,
        fee_asset: None,
        rebate: None,
        rebate_asset: None,
    }))
}
pub fn closed_number(
    value: &safe_cex_exchange::js_runtime::value::Js,
) -> Result<f64, safe_cex_exchange::js_runtime::value::Thrown> {
    let parsed = if value.is_nullish() {
        f64::NAN
    } else {
        value.parse_float()?
    };
    Ok(if parsed.is_finite() { parsed } else { 0. })
}
pub fn closed_timestamp(
    value: &safe_cex_exchange::js_runtime::value::Js,
) -> Result<f64, safe_cex_exchange::js_runtime::value::Thrown> {
    let parsed = if value.is_nullish() {
        0.
    } else {
        parse_int(&String::from_utf16_lossy(&value.to_js_string()?), Some(10))
    };
    Ok(if parsed.is_finite() && parsed > 0. {
        parsed
    } else {
        0.
    })
}
pub fn map_bybit_closed_pnl(
    row: &safe_cex_exchange::js_runtime::value::Js,
    account_name: safe_cex_exchange::js_runtime::value::Js,
    is_inverse: bool,
) -> Result<safe_cex_exchange::js_runtime::value::Js, safe_cex_exchange::js_runtime::value::Thrown>
{
    use safe_cex_exchange::js_runtime::value::Js;
    let number = |key| closed_number(&row.get(key)?);
    let timestamp = |key| closed_timestamp(&row.get(key)?);
    let symbol = row.get("symbol")?;
    let symbol = if symbol.truthy() {
        Js::String(symbol.to_js_string()?)
    } else {
        Js::str("")
    };
    let created_time = timestamp("createdTime")?;
    if !symbol.truthy() || created_time == 0. {
        return Ok(Js::Null);
    }
    let open_fee = number("openFee")?;
    let close_fee = number("closeFee")?;
    let closed_size = number("closedSize")?;
    let exit_price = number("avgExitPrice")?;
    let contracts = if is_inverse && exit_price > 0. {
        math_round((closed_size / exit_price) * 1e8) / 1e8
    } else {
        closed_size
    };
    let notional = if is_inverse {
        closed_size
    } else {
        number("cumExitValue")?
    };
    let entry_price = number("avgEntryPrice")?;
    let pnl = number("closedPnl")?;
    let side = if row.get("side")?.strict_equals(&Js::str("Sell")) {
        "long"
    } else {
        "short"
    };
    let closed_time = timestamp("updatedTime")?;
    Ok(Js::object(vec![
        ("provenance", Js::str("venue")),
        ("symbol", symbol),
        ("notional", Js::Number(notional)),
        ("contracts", Js::Number(contracts)),
        ("entryPrice", Js::Number(entry_price)),
        ("exitPrice", Js::Number(exit_price)),
        ("unrealizedPnl", Js::Number(pnl)),
        ("side", Js::str(side)),
        ("fees", Js::Number(open_fee + close_fee)),
        ("time", Js::Number(created_time)),
        (
            "closedTime",
            Js::Number(if closed_time == 0. {
                created_time
            } else {
                closed_time
            }),
        ),
        ("account", account_name),
        ("leverage", Js::Number(number("leverage")?)),
    ]))
}
fn income_table(
    fields: Vec<(&str, safe_cex_exchange::js_runtime::value::Js)>,
) -> safe_cex_exchange::js_runtime::value::Js {
    use safe_cex_exchange::js_runtime::value::Js;
    let table = Js::object(fields);
    if let Js::Object(object) = &table {
        *object.proto.borrow_mut() = Some(Js::Builtin(
            safe_cex_num::PrototypeRealm::current().reference(safe_cex_num::ProtoMember::Prototype),
        ));
    }
    table
}
pub fn bybit_transaction_category(
    value: &safe_cex_exchange::js_runtime::value::Js,
) -> Result<safe_cex_exchange::js_runtime::value::Js, safe_cex_exchange::js_runtime::value::Thrown>
{
    use safe_cex_exchange::js_runtime::value::Js;
    let key = if value.is_nullish() {
        Js::str("")
    } else {
        Js::String(value.to_js_string()?)
    };
    let table = income_table(vec![
        ("TRADE", Js::str("realized_pnl")),
        ("DELIVERY", Js::str("realized_pnl")),
        ("SETTLEMENT", Js::str("funding")),
        ("LIQUIDATION", Js::str("liquidation")),
        ("TRANSFER_IN", Js::str("transfer")),
        ("TRANSFER_OUT", Js::str("transfer")),
        ("FEE_REFUND", Js::str("rebate")),
    ]);
    let category = table.get_property(&key)?;
    Ok(if category.truthy() {
        category
    } else {
        Js::str("other")
    })
}
pub fn map_bybit_transaction_to_income_records(
    row: &safe_cex_exchange::js_runtime::value::Js,
) -> Result<safe_cex_exchange::js_runtime::value::Js, safe_cex_exchange::js_runtime::value::Thrown>
{
    use safe_cex_exchange::js_runtime::value::{Js, Thrown};
    let time = row.get("transactionTime")?;
    let time = if time.is_nullish() {
        Js::str("0")
    } else {
        time
    };
    let timestamp = parse_int(&String::from_utf16_lossy(&time.to_js_string()?), Some(10));
    if !timestamp.is_finite() || timestamp <= 0. {
        return Ok(Js::array(vec![]));
    }
    let cash_flow = closed_number(&row.get("cashFlow")?)?;
    let funding = closed_number(&row.get("funding")?)?;
    let fee = closed_number(&row.get("fee")?)?;
    let category = bybit_transaction_category(&row.get("type")?)?;
    let or = |value: Js, fallback: Js| if value.truthy() { value } else { fallback };
    let symbol = or(row.get("symbol")?, Js::str(""));
    let asset = or(row.get("currency")?, Js::str("USDT"));
    let trade_id = or(row.get("tradeId")?, Js::Undefined);
    let id = row.get("tradeId")?;
    let id = if id.truthy() {
        id
    } else {
        or(row.get("orderId")?, Js::str(""))
    };
    let base = Js::array(vec![
        id,
        symbol.clone(),
        row.get("type")?,
        row.get("transactionTime")?,
    ]);
    let filter = Js::function("Boolean", |_, args| {
        Ok(Js::Bool(args.first().is_some_and(Js::truthy)))
    });
    let base_id = base
        .call_method("filter", vec![filter])?
        .call_method("join", vec![Js::str("-")])?;
    let income_types = income_table(vec![
        ("realized_pnl", Js::str("REALIZED_PNL")),
        ("funding", Js::str("FUNDING_FEE")),
        ("commission", Js::str("COMMISSION")),
        ("rebate", Js::str("REBATE")),
        ("transfer", Js::str("TRANSFER")),
        ("liquidation", Js::str("LIQUIDATION")),
        ("other", Js::str("OTHER")),
    ]);
    let mut records = Vec::new();
    let mut push = |income, category: Js, suffix: &str| -> Result<(), Thrown> {
        let mut id = base_id.to_js_string()?.to_vec();
        id.extend(format!(":{suffix}").encode_utf16());
        let record = Js::object(vec![
            ("symbol", symbol.clone()),
            ("incomeType", income_types.get_property(&category)?),
            ("income", Js::Number(income)),
            ("asset", asset.clone()),
            ("timestamp", Js::Number(timestamp)),
            ("tranId", Js::utf16(&id)),
            ("category", category),
        ]);
        if trade_id.truthy() {
            record.define("tradeId", trade_id.clone())?;
        }
        records.push(record);
        Ok(())
    };
    if cash_flow != 0. {
        push(cash_flow, category, "flow")?;
    }
    if funding != 0. {
        push(funding, Js::str("funding"), "funding")?;
    }
    if fee != 0. {
        push(
            -fee,
            Js::str(if fee > 0. { "commission" } else { "rebate" }),
            "fee",
        )?;
    }
    Ok(Js::array(records))
}
