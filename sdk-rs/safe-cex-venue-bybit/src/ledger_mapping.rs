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
pub fn to_timestamp(value: Option<&Value>) -> Result<f64, &'static str> {
    let parsed = parse_int(&text(value)?, Some(10));
    Ok(if parsed.is_finite() && parsed > 0. {
        parsed
    } else {
        0.
    })
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
pub fn map_bybit_closed_pnl(
    row: &Map<String, Value>,
    account_name: &str,
    is_inverse: bool,
) -> Result<Option<ClosedPnlEntry>, &'static str> {
    let symbol = match row.get("symbol").filter(|v| truthy(v)) {
        Some(v) => js_string(Some(v)).map_err(|e| e.message)?,
        None => String::new(),
    };
    let created_time = to_timestamp(row.get("createdTime"))?;
    if symbol.is_empty() || created_time == 0. {
        return Ok(None);
    }
    let open_fee = to_number(row.get("openFee"))?;
    let close_fee = to_number(row.get("closeFee"))?;
    let closed_size = to_number(row.get("closedSize"))?;
    let exit_price = to_number(row.get("avgExitPrice"))?;
    let contracts = if is_inverse && exit_price > 0. {
        math_round((closed_size / exit_price) * 1e8) / 1e8
    } else {
        closed_size
    };
    let notional = if is_inverse {
        closed_size
    } else {
        to_number(row.get("cumExitValue"))?
    };
    let entry_price = to_number(row.get("avgEntryPrice"))?;
    let pnl = to_number(row.get("closedPnl"))?;
    let side = if row.get("side").and_then(Value::as_str) == Some("Sell") {
        "long"
    } else {
        "short"
    };
    let closed_time = to_timestamp(row.get("updatedTime"))?;
    let leverage = to_number(row.get("leverage"))?;
    Ok(Some(ClosedPnlEntry {
        provenance: DataProvenance::Venue,
        symbol,
        notional: ClosedPnlEntryNotional::Variant1(notional),
        contracts: ClosedPnlEntryContracts::Variant1(contracts),
        entry_price: ClosedPnlEntryEntryPrice::Variant1(entry_price),
        exit_price: ClosedPnlEntryExitPrice::Variant1(exit_price),
        unrealized_pnl: ClosedPnlEntryUnrealizedPnl::Variant1(pnl),
        side: side.into(),
        fees: ClosedPnlEntryFees::Variant1(open_fee + close_fee),
        time: created_time,
        closed_time: if closed_time == 0. {
            created_time
        } else {
            closed_time
        },
        account: account_name.into(),
        leverage: ClosedPnlEntryLeverage::Variant0(leverage),
    }))
}
pub fn bybit_transaction_category(value: Option<&Value>) -> Result<IncomeCategory, &'static str> {
    let key = text(value)?;
    check_table_key(&key)?;
    Ok(match key.as_str() {
        "TRADE" | "DELIVERY" => IncomeCategory::RealizedPnl,
        "SETTLEMENT" => IncomeCategory::Funding,
        "LIQUIDATION" => IncomeCategory::Liquidation,
        "TRANSFER_IN" | "TRANSFER_OUT" => IncomeCategory::Transfer,
        "FEE_REFUND" => IncomeCategory::Rebate,
        _ => IncomeCategory::Other,
    })
}
fn retained_string(value: Option<&Value>, fallback: &str) -> Result<String, &'static str> {
    match value.filter(|v| truthy(v)) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err("Bybit native income string cannot retain truthy nonstring"),
        None => Ok(fallback.into()),
    }
}
pub fn map_bybit_transaction_to_income_records(
    row: &Map<String, Value>,
) -> Result<Vec<IncomeRecord>, &'static str> {
    let timestamp = parse_int(&text(row.get("transactionTime"))?, Some(10));
    if !timestamp.is_finite() || timestamp <= 0. {
        return Ok(vec![]);
    }
    let cash_flow = to_number(row.get("cashFlow"))?;
    let funding = to_number(row.get("funding"))?;
    let fee = to_number(row.get("fee"))?;
    let type_key = text(row.get("type"))?;
    let category = bybit_transaction_category(Some(&Value::String(type_key)));
    // Preserve identity coercion even on zero-term rows before any typed refusal.
    let id = row
        .get("tradeId")
        .filter(|v| truthy(v))
        .or_else(|| row.get("orderId").filter(|v| truthy(v)));
    let base_id = [
        id,
        row.get("symbol"),
        row.get("type"),
        row.get("transactionTime"),
    ]
    .into_iter()
    .flatten()
    .filter(|v| truthy(v))
    .map(|v| js_string(Some(v)).map_err(|e| e.message))
    .collect::<Result<Vec<_>, _>>()?
    .join("-");
    if cash_flow == 0. && funding == 0. && fee == 0. {
        return Ok(vec![]);
    }
    let symbol = retained_string(row.get("symbol"), "")?;
    let asset = retained_string(row.get("currency"), "USDT")?;
    let trade_id = row
        .get("tradeId")
        .filter(|v| truthy(v))
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or("Bybit native income tradeId cannot retain truthy nonstring")
        })
        .transpose()?;
    let mut records = Vec::new();
    let mut push = |income, category: IncomeCategory, suffix| {
        let income_type = match category {
            IncomeCategory::RealizedPnl => "REALIZED_PNL",
            IncomeCategory::Funding => "FUNDING_FEE",
            IncomeCategory::Commission => "COMMISSION",
            IncomeCategory::Rebate => "REBATE",
            IncomeCategory::Transfer => "TRANSFER",
            IncomeCategory::Liquidation => "LIQUIDATION",
            IncomeCategory::Other => "OTHER",
        };
        records.push(IncomeRecord {
            symbol: symbol.clone(),
            income_type: income_type.into(),
            income,
            asset: asset.clone(),
            timestamp,
            tran_id: format!("{base_id}:{suffix}"),
            trade_id: trade_id.clone(),
            category: Some(category),
        });
    };
    if cash_flow != 0. {
        push(cash_flow, category?, "flow");
    }
    if funding != 0. {
        push(funding, IncomeCategory::Funding, "funding");
    }
    if fee != 0. {
        push(
            -fee,
            if fee > 0. {
                IncomeCategory::Commission
            } else {
                IncomeCategory::Rebate
            },
            "fee",
        );
    }
    Ok(records)
}
