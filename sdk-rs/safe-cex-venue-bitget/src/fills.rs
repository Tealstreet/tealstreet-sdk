//! Classic v2 and UTA v3 fills; v3 trade direction and execution fields are a different schema.
use safe_cex_num::{JsCoercionError, js_string, parse_float};
use safe_cex_types::{BothPositionSide, ExecutionPositionSide, OrderSide, PositionSide};
use serde_json::Value;

// ts: num
pub fn num(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let text = js_string(Some(
        value
            .filter(|v| !v.is_null())
            .unwrap_or(&Value::String(String::new())),
    ))?;
    let parsed = parse_float(&text);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

// ts: totalFeeOf
pub fn total_fee_of(row: Option<&Value>) -> Result<f64, JsCoercionError> {
    let Some(detail) = row
        .and_then(|r| r.get("feeDetail"))
        .and_then(Value::as_array)
    else {
        return num(row.and_then(|r| r.get("fee")));
    };
    let mut total = 0.0;
    for entry in detail {
        total += num(entry.get("totalFee"))?;
    }
    Ok(total)
}

// ts: BitgetLeg (private source return shape of legAndDirectionOf)
#[derive(Debug, PartialEq)]
pub struct BitgetLeg {
    pub position_side: ExecutionPositionSide,
    pub direction: OrderSide,
}

// ts: legAndDirectionOf
pub fn leg_and_direction_of(row: Option<&Value>) -> Result<Option<BitgetLeg>, JsCoercionError> {
    let empty = Value::String(String::new());
    let trade_side = js_string(Some(
        row.and_then(|r| r.get("tradeSide"))
            .filter(|v| !v.is_null())
            .unwrap_or(&empty),
    ))?
    .to_lowercase();
    let raw_side = js_string(Some(
        row.and_then(|r| r.get("side"))
            .filter(|v| !v.is_null())
            .unwrap_or(&empty),
    ))?
    .to_lowercase();
    if raw_side != "buy" && raw_side != "sell" {
        return Ok(None);
    }
    if trade_side.ends_with("_single") {
        return Ok(Some(BitgetLeg {
            position_side: ExecutionPositionSide::Both(BothPositionSide::Both),
            direction: if raw_side == "buy" {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            },
        }));
    }
    let leg = if trade_side.contains("long") {
        PositionSide::Long
    } else if trade_side.contains("short") {
        PositionSide::Short
    } else if raw_side == "buy" {
        PositionSide::Long
    } else {
        PositionSide::Short
    };
    let opening = trade_side.starts_with("open");
    let closing = trade_side.starts_with("close");
    if !opening && !closing {
        return Ok(None);
    }
    let direction = if (leg == PositionSide::Long) == opening {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    Ok(Some(BitgetLeg {
        position_side: ExecutionPositionSide::Side(leg),
        direction,
    }))
}

// ts: BitgetFillDeps
pub struct BitgetFillDeps<'a> {
    pub resolve_symbol:
        &'a mut dyn FnMut(
            &str,
        )
            -> Result<Option<String>, safe_cex_exchange::runtime::ExchangeError>,
    pub to_timestamp:
        &'a mut dyn FnMut(f64) -> Result<f64, safe_cex_exchange::runtime::ExchangeError>,
}

// ts: mapBitgetFill
pub fn map_bitget_fill(
    row: Option<&Value>,
    deps: &mut BitgetFillDeps<'_>,
) -> Result<Option<safe_cex_types::Execution>, safe_cex_exchange::runtime::ExchangeError> {
    use safe_cex_exchange::runtime::ExchangeError;
    use safe_cex_num::{js_truthy, parse_int};
    let coercion = |e: JsCoercionError| ExchangeError {
        name: e.name.into(),
        message: e.message.into(),
        metadata: Box::default(),
    };
    let empty = Value::String(String::new());
    let zero = Value::String("0".into());
    let get = |key: &str| row.and_then(|r| r.get(key));
    let venue_symbol = js_string(Some(
        get("symbol").filter(|v| !v.is_null()).unwrap_or(&empty),
    ))
    .map_err(coercion)?;
    let Some(symbol) = (deps.resolve_symbol)(&venue_symbol)?.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let ms = parse_int(
        &js_string(Some(get("cTime").filter(|v| !v.is_null()).unwrap_or(&zero)))
            .map_err(coercion)?,
        Some(10),
    );
    if !ms.is_finite() || ms <= 0.0 {
        return Ok(None);
    }
    let qty = num(get("baseVolume")).map_err(coercion)?;
    let price = num(get("price")).map_err(coercion)?;
    if qty <= 0.0 || price <= 0.0 {
        return Ok(None);
    }
    let fee = total_fee_of(row).map_err(coercion)?;
    let Some(leg) = leg_and_direction_of(row).map_err(coercion)? else {
        return Ok(None);
    };
    // Object-literal evaluation calls the clock before coercing either ID.
    let timestamp = (deps.to_timestamp)(ms)?;
    let id = if js_truthy(get("tradeId")) {
        Some(js_string(get("tradeId")).map_err(coercion)?)
    } else {
        None
    };
    let order_id = if js_truthy(get("orderId")) {
        Some(js_string(get("orderId")).map_err(coercion)?)
    } else {
        None
    };
    // Optional [0] also reads a numeric property on an object, even though
    // totalFeeOf only sums real arrays.
    let first_fee = get("feeDetail").and_then(|v| match v {
        Value::Array(a) => a.first(),
        Value::Object(m) => m.get("0"),
        _ => None,
    });
    let usdt = Value::String("USDT".into());
    let commission_asset = js_string(Some(
        first_fee
            .and_then(|v| v.get("feeCoin"))
            .filter(|v| !v.is_null())
            .unwrap_or(&usdt),
    ))
    .map_err(coercion)?;
    let realized_pnl = num(get("profit")).map_err(coercion)?;
    let maker = js_string(Some(
        get("tradeScope").filter(|v| !v.is_null()).unwrap_or(&empty),
    ))
    .map_err(coercion)?
    .to_lowercase()
        == "maker";
    Ok(Some(safe_cex_types::Execution {
        timestamp,
        symbol,
        id,
        order_id,
        price,
        qty,
        side: leg.direction,
        commission: Some(-fee),
        commission_asset: Some(commission_asset),
        realized_pnl: Some(realized_pnl),
        position_side: Some(leg.position_side),
        maker: Some(maker),
        venue_reported_revenue: None,
        venue_reported_revenue_asset: None,
    }))
}

// ts: mapBitgetFills
pub fn map_bitget_fills(
    rows: &[Value],
    deps: &mut BitgetFillDeps<'_>,
) -> Result<Vec<safe_cex_types::Execution>, safe_cex_exchange::runtime::ExchangeError> {
    let mut executions = Vec::new();
    for row in rows {
        if let Some(execution) = map_bitget_fill(Some(row), deps)? {
            executions.push(execution);
        }
    }
    Ok(executions)
}

// ts: optionalNum
pub fn optional_num(value: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    let text = js_string(Some(
        value
            .filter(|v| !v.is_null())
            .unwrap_or(&Value::String(String::new())),
    ))?;
    let parsed = parse_float(&text);
    Ok(parsed.is_finite().then_some(parsed))
}

// ts: totalUtaFeeOf
pub fn total_uta_fee_of(row: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    let Some(detail) = row
        .and_then(|r| r.get("feeDetail"))
        .and_then(Value::as_array)
    else {
        return optional_num(row.and_then(|r| r.get("fee")));
    };
    let mut total = 0.0;
    let mut found = false;
    for entry in detail {
        // `entry?.fee ?? entry?.totalFee`: only a nullish fee falls through.
        let fee = entry
            .get("fee")
            .filter(|v| !v.is_null())
            .or_else(|| entry.get("totalFee"));
        if let Some(fee) = optional_num(fee)? {
            total += fee;
            found = true;
        }
    }
    Ok(found.then_some(total))
}

// ts: utaLegAndDirectionOf
pub fn uta_leg_and_direction_of(row: Option<&Value>) -> Result<Option<BitgetLeg>, JsCoercionError> {
    let empty = Value::String(String::new());
    let text = |key: &str| {
        js_string(Some(
            row.and_then(|r| r.get(key))
                .filter(|v| !v.is_null())
                .unwrap_or(&empty),
        ))
        .map(|s| s.to_lowercase())
    };
    let trade_side = text("tradeSide")?;
    let raw_side = text("side")?;
    if raw_side != "buy" && raw_side != "sell" {
        return Ok(None);
    }
    let direction = if raw_side == "buy" {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    // `row?.posSide ?? row?.holdSide` is coerced once per comparison in source.
    let held = row
        .and_then(|r| r.get("posSide"))
        .filter(|v| !v.is_null())
        .or_else(|| row.and_then(|r| r.get("holdSide")))
        .filter(|v| !v.is_null())
        .unwrap_or(&empty);
    let explicit = if js_string(Some(held))?.to_lowercase() == "long" {
        Some(PositionSide::Long)
    } else if js_string(Some(held))?.to_lowercase() == "short" {
        Some(PositionSide::Short)
    } else if trade_side.contains("long") {
        Some(PositionSide::Long)
    } else if trade_side.contains("short") {
        Some(PositionSide::Short)
    } else {
        None
    };
    if let Some(leg) = explicit {
        return Ok(Some(BitgetLeg {
            position_side: ExecutionPositionSide::Side(leg),
            direction,
        }));
    }
    let opening = trade_side.starts_with("open");
    let closing = trade_side.starts_with("close");
    if !opening && !closing {
        return Ok(None);
    }
    let leg = if (direction == OrderSide::Buy) == opening {
        PositionSide::Long
    } else {
        PositionSide::Short
    };
    Ok(Some(BitgetLeg {
        position_side: ExecutionPositionSide::Side(leg),
        direction,
    }))
}

// ts: mapBitgetUtaFill
pub fn map_bitget_uta_fill(
    row: Option<&Value>,
    deps: &mut BitgetFillDeps<'_>,
) -> Result<Option<safe_cex_types::Execution>, safe_cex_exchange::runtime::ExchangeError> {
    use safe_cex_exchange::runtime::ExchangeError;
    use safe_cex_num::{js_truthy, parse_int};
    let coercion = |e: JsCoercionError| ExchangeError {
        name: e.name.into(),
        message: e.message.into(),
        metadata: Box::default(),
    };
    let empty = Value::String(String::new());
    let zero = Value::String("0".into());
    let get = |key: &str| row.and_then(|r| r.get(key));
    let venue_symbol = js_string(Some(
        get("symbol").filter(|v| !v.is_null()).unwrap_or(&empty),
    ))
    .map_err(coercion)?;
    let Some(symbol) = (deps.resolve_symbol)(&venue_symbol)?.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let ms = parse_int(
        &js_string(Some(
            get("createdTime").filter(|v| !v.is_null()).unwrap_or(&zero),
        ))
        .map_err(coercion)?,
        Some(10),
    );
    if !ms.is_finite() || ms <= 0.0 {
        return Ok(None);
    }
    let qty = num(get("execQty")).map_err(coercion)?;
    let price = num(get("execPrice")).map_err(coercion)?;
    if qty <= 0.0 || price <= 0.0 {
        return Ok(None);
    }
    let Some(leg) = uta_leg_and_direction_of(row).map_err(coercion)? else {
        return Ok(None);
    };
    let fee = total_uta_fee_of(row).map_err(coercion)?;
    // Optional [0] also reads a numeric property on an object.
    let first_fee = get("feeDetail").and_then(|v| match v {
        Value::Array(a) => a.first(),
        Value::Object(m) => m.get("0"),
        _ => None,
    });
    let fee_coin = first_fee.and_then(|v| v.get("feeCoin"));
    let commission_asset = if js_truthy(fee_coin) {
        Some(js_string(fee_coin).map_err(coercion)?)
    } else {
        None
    };
    // Object-literal evaluation calls the clock before coercing either ID.
    let timestamp = (deps.to_timestamp)(ms)?;
    let id = if js_truthy(get("execId")) {
        Some(js_string(get("execId")).map_err(coercion)?)
    } else if js_truthy(get("execLinkId")) {
        Some(js_string(get("execLinkId")).map_err(coercion)?)
    } else {
        None
    };
    let order_id = if js_truthy(get("orderId")) {
        Some(js_string(get("orderId")).map_err(coercion)?)
    } else {
        None
    };
    let realized_pnl = optional_num(get("execPnl")).map_err(coercion)?;
    let maker = if js_truthy(get("tradeScope")) {
        Some(
            js_string(get("tradeScope"))
                .map_err(coercion)?
                .to_lowercase()
                == "maker",
        )
    } else {
        None
    };
    Ok(Some(safe_cex_types::Execution {
        timestamp,
        symbol,
        id,
        order_id,
        price,
        qty,
        side: leg.direction,
        commission: fee,
        commission_asset,
        realized_pnl,
        position_side: Some(leg.position_side),
        maker,
        venue_reported_revenue: None,
        venue_reported_revenue_asset: None,
    }))
}

// ts: mapBitgetUtaFills
pub fn map_bitget_uta_fills(
    rows: &[Value],
    deps: &mut BitgetFillDeps<'_>,
) -> Result<Vec<safe_cex_types::Execution>, safe_cex_exchange::runtime::ExchangeError> {
    let mut executions = Vec::new();
    for row in rows {
        if let Some(execution) = map_bitget_uta_fill(Some(row), deps)? {
            executions.push(execution);
        }
    }
    Ok(executions)
}
