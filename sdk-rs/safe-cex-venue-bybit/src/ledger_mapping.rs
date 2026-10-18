//! Native Bybit order-history, closed-PnL and transaction-log source mappings.
use safe_cex_num::{math_round, parse_int};
use serde_json::Value;

pub fn to_optional_number(
    value: &safe_cex_exchange::js_runtime::value::Js,
) -> Result<safe_cex_exchange::js_runtime::value::Js, safe_cex_exchange::js_runtime::value::Thrown>
{
    use safe_cex_exchange::js_runtime::value::Js;
    let raw = if value.is_nullish() {
        Js::str("")
    } else {
        Js::String(value.to_js_string()?)
    };
    let raw = raw.call_method("trim", vec![])?;
    if !raw.truthy() {
        return Ok(Js::Undefined);
    }
    let parsed = raw.parse_float()?;
    Ok(if parsed.is_finite() && parsed != 0. {
        Js::Number(parsed)
    } else {
        Js::Undefined
    })
}
fn json_table(value: &Value) -> safe_cex_exchange::js_runtime::value::Js {
    use safe_cex_exchange::js_runtime::value::{Js, from_json};
    let table = from_json(value);
    if let Js::Object(object) = &table {
        *object.proto.borrow_mut() = Some(Js::Builtin(
            safe_cex_num::PrototypeRealm::current().reference(safe_cex_num::ProtoMember::Prototype),
        ));
    }
    table
}
pub fn map_bybit_historical_order(
    row: &safe_cex_exchange::js_runtime::value::Js,
) -> Result<safe_cex_exchange::js_runtime::value::Js, safe_cex_exchange::js_runtime::value::Thrown>
{
    use safe_cex_exchange::js_runtime::value::{Js, Thrown};
    let string = |key: &str, fallback: &str| -> Result<Js, Thrown> {
        let value = row.get(key)?;
        Ok(if value.is_nullish() {
            Js::str(fallback)
        } else {
            Js::String(value.to_js_string()?)
        })
    };
    let id = string("orderId", "")?;
    let symbol = string("symbol", "")?;
    let created = parse_int(
        &String::from_utf16_lossy(&string("createdTime", "0")?.to_js_string()?),
        Some(10),
    );
    if !id.truthy() || !symbol.truthy() || created == 0. || created.is_nan() {
        return Ok(Js::Null);
    }
    let side = json_table(&crate::types::ORDER_SIDE).get_property(&string("side", "")?)?;
    if !side.truthy() {
        return Ok(Js::Null);
    }
    let stop = string("stopOrderType", "")?.call_method("trim", vec![])?;
    let types = json_table(&crate::types::ORDER_TYPE);
    let kind = if stop.truthy() {
        types.get_property(&stop)?
    } else {
        stop
    };
    let kind = if kind.truthy() {
        kind
    } else {
        types.get_property(&string("orderType", "")?)?
    };
    let kind = if kind.truthy() {
        kind
    } else {
        Js::str("limit")
    };
    let status =
        json_table(&crate::types::ORDER_STATUS).get_property(&string("orderStatus", "")?)?;
    let status = if status.is_nullish() {
        Js::str("closed")
    } else {
        status
    };
    let price = to_optional_number(&row.get("price")?)?;
    let average = to_optional_number(&row.get("avgPrice")?)?;
    let amount = to_number(&row.get("qty")?)?;
    let filled = to_number(&row.get("cumExecQty")?)?;
    let reduce_only = row.get("reduceOnly")?.truthy();
    let sides = ledger_table(vec![
        ("0", Js::str("both")),
        ("1", Js::str("long")),
        ("2", Js::str("short")),
    ]);
    let position_side = sides.get_property(&string("positionIdx", "")?)?;
    let updated = parse_int(
        &String::from_utf16_lossy(&string("updatedTime", "0")?.to_js_string()?),
        Some(10),
    );
    Ok(Js::object(vec![
        ("id", id),
        ("symbol", symbol),
        ("side", side),
        ("type", kind),
        ("status", status),
        ("price", price),
        ("averagePrice", average),
        ("amount", Js::Number(amount)),
        ("filled", Js::Number(filled)),
        ("reduceOnly", Js::Bool(reduce_only)),
        ("positionSide", position_side),
        ("createdTime", Js::Number(created)),
        (
            "updatedTime",
            if updated != 0. && !updated.is_nan() {
                Js::Number(updated)
            } else {
                Js::Undefined
            },
        ),
    ]))
}
pub fn to_number(
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
    let number = |key| to_number(&row.get(key)?);
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
fn ledger_table(
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
    let table = ledger_table(vec![
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
    let cash_flow = to_number(&row.get("cashFlow")?)?;
    let funding = to_number(&row.get("funding")?)?;
    let fee = to_number(&row.get("fee")?)?;
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
    let income_types = ledger_table(vec![
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
