//! Classic v2 fills; v3 trade direction and execution fields are a different schema.
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
