//! Pure private-stream account parsers. Socket lifecycle is owned by BaseWebSocket.
use safe_cex_num::{JsCoercionError, js_number, js_string, js_trim, js_truthy, parse_float};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExecutionReportUpdate {
    pub status: safe_cex_num::TableHit<safe_cex_types::OrderStatus>,
    pub filled: f64,
    pub remaining: f64,
}

// ts: AsterDEXPrivateWebsocket#handleExecutionReport
pub fn handle_execution_report(
    data: &Value,
    existing_order_ids: &[String],
) -> Result<Option<(String, ExecutionReportUpdate)>, JsCoercionError> {
    if data.is_null() {
        return Err(JsCoercionError {
            name: "TypeError",
            message: "Cannot read properties of null (reading 'i')",
        });
    }
    let id = data.get("i");
    if id.is_none_or(Value::is_null) {
        return Ok(None);
    }
    if id.is_some_and(|value| {
        value
            .as_object()
            .is_some_and(|object| object.contains_key("toString"))
    }) {
        return Err(JsCoercionError {
            name: "TypeError",
            message: "data.i?.toString is not a function",
        });
    }
    let order_id = js_string(id)?;
    if !existing_order_ids
        .iter()
        .any(|existing| existing == &order_id)
    {
        return Ok(None);
    }
    const STATUS: &[(&str, safe_cex_types::OrderStatus)] = &[
        ("NEW", safe_cex_types::OrderStatus::Open),
        ("PARTIALLY_FILLED", safe_cex_types::OrderStatus::Open),
        ("FILLED", safe_cex_types::OrderStatus::Closed),
        ("CANCELED", safe_cex_types::OrderStatus::Canceled),
        ("REJECTED", safe_cex_types::OrderStatus::Canceled),
        ("EXPIRED", safe_cex_types::OrderStatus::Canceled),
    ];
    let status_key = js_string(data.get("X"))?;
    let status = match safe_cex_num::js_table_lookup(STATUS, &status_key) {
        safe_cex_num::TableHit::Mapped(status) => safe_cex_num::TableHit::Mapped(*status),
        safe_cex_num::TableHit::Inherited(member) => safe_cex_num::TableHit::Inherited(member),
        safe_cex_num::TableHit::Missing => {
            safe_cex_num::TableHit::Mapped(safe_cex_types::OrderStatus::Open)
        }
    };
    let filled = parse_order_number(data, "z")?;
    let amount = parse_order_number(data, "q")?;
    Ok(Some((
        order_id,
        ExecutionReportUpdate {
            status,
            filled,
            remaining: amount - filled,
        },
    )))
}

#[derive(Debug, Clone, PartialEq)]
pub struct CumulativeOrderFill {
    pub order_id: String,
    pub status: safe_cex_types::OrderStatus,
    pub filled: f64,
    pub remaining: f64,
    pub last_update: f64,
}

// ts: AsterDEXPrivateWebsocket#mirrorCumulativeOrderFill
pub fn mirror_cumulative_order_fill(
    order_data: &Value,
    current_time_ms: impl FnOnce() -> f64,
) -> Result<Option<CumulativeOrderFill>, JsCoercionError> {
    if order_data.is_null() {
        return Err(JsCoercionError {
            name: "TypeError",
            message: "Cannot read properties of null (reading 'i')",
        });
    }
    let id = order_data.get("i");
    if id.is_some_and(|value| {
        value
            .as_object()
            .is_some_and(|object| object.contains_key("toString"))
    }) {
        return Err(JsCoercionError {
            name: "TypeError",
            message: "orderData.i?.toString is not a function",
        });
    }
    let order_id = if id.is_none_or(Value::is_null) {
        None
    } else {
        Some(js_string(id)?)
    };
    let filled = parse_order_number(order_data, "z")?;
    let amount = parse_order_number(order_data, "q")?;
    let Some(order_id) = order_id.filter(|id| !id.is_empty()) else {
        return Ok(None);
    };
    if !filled.is_finite() || !amount.is_finite() {
        return Ok(None);
    }
    let last_update = get_order_update_time(order_data, current_time_ms)?;
    Ok(Some(CumulativeOrderFill {
        order_id,
        status: if order_data.get("X").and_then(Value::as_str) == Some("FILLED") {
            safe_cex_types::OrderStatus::Closed
        } else {
            safe_cex_types::OrderStatus::Open
        },
        filled,
        remaining: (amount - filled).max(0.0),
        last_update,
    }))
}

fn parse_order_number(order_data: &Value, field: &str) -> Result<f64, JsCoercionError> {
    let value = order_data.get(field);
    if js_truthy(value) {
        Ok(parse_float(&js_string(value)?))
    } else {
        Ok(0.0)
    }
}

// ts: AsterDEXPrivateWebsocket.parseFiniteNumber
pub fn parse_finite_number(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(_)) => {}
        Some(Value::String(s)) if !js_trim(s).is_empty() => {}
        _ => return None,
    }
    let value = js_number(value).ok()?;
    value.is_finite().then_some(value)
}

// ts: AsterDEXPrivateWebsocket.getOrderUpdateTime
pub fn get_order_update_time(
    order_data: &Value,
    current_time_ms: impl FnOnce() -> f64,
) -> Result<f64, JsCoercionError> {
    if order_data.is_null() {
        return Err(JsCoercionError {
            name: "TypeError",
            message: "Cannot read properties of null (reading 'T')",
        });
    }
    let time = order_data.get("T");
    let event_time = js_number(if js_truthy(time) {
        time
    } else {
        order_data.get("E")
    })?;
    Ok(if event_time.is_finite() && event_time > 0.0 {
        event_time
    } else {
        current_time_ms()
    })
}

// ts: AssetBalanceUpdate (the private output record of parseAssetBalanceUpdates)
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AssetBalanceUpdate {
    pub asset: String,
    pub total: f64,
    pub free: f64,
    pub used: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AccountUpdateEffect<'a> {
    ApplyBalances(Vec<AssetBalanceUpdate>),
    EmitAccount(&'a Value),
}

// ts: AsterDEXPrivateWebsocket#handleAccountUpdate
pub fn handle_account_update(
    data: &Value,
) -> Result<Vec<AccountUpdateEffect<'_>>, JsCoercionError> {
    if data.is_null() {
        return Err(JsCoercionError {
            name: "TypeError",
            message: "Cannot read properties of null (reading 'B')",
        });
    }
    let mut effects = Vec::new();
    if js_truthy(data.get("B"))
        && let Some(balances) = parse_asset_balance_updates(data.get("B"), None, "f", Some("l"))
    {
        effects.push(AccountUpdateEffect::ApplyBalances(balances));
    }
    effects.push(AccountUpdateEffect::EmitAccount(data));
    Ok(effects)
}

// ts: AsterDEXPrivateWebsocket.parseAssetBalanceUpdates
pub fn parse_asset_balance_updates(
    balances: Option<&Value>,
    total_field: Option<&str>,
    free_field: &str,
    used_field: Option<&str>,
) -> Option<Vec<AssetBalanceUpdate>> {
    let balances = balances?.as_array()?;
    let mut updates = Vec::with_capacity(balances.len());
    for balance in balances {
        let asset = balance
            .get("a")
            .and_then(Value::as_str)
            .map(js_trim)
            .unwrap_or("");
        let free = parse_finite_number(balance.get(free_field));
        let used = used_field
            .filter(|s| !s.is_empty())
            .and_then(|field| parse_finite_number(balance.get(field)));
        let total = if let Some(field) = total_field.filter(|s| !s.is_empty()) {
            parse_finite_number(balance.get(field))
        } else {
            free.zip(used).map(|(a, b)| a + b)
        };
        let derived_used = used.or_else(|| total.zip(free).map(|(a, b)| a - b));
        if asset.is_empty() {
            return None;
        }
        let (total, free, used) = (total?, free?, derived_used?);
        if used < 0.0 {
            return None;
        }
        // TS validates inputs, but intentionally does not recheck overflowed sums/differences.
        updates.push(AssetBalanceUpdate {
            asset: asset.into(),
            total,
            free,
            used,
        });
    }
    Some(updates)
}

// ts: AsterDEXPrivateWebsocket.getPositionSide
pub fn get_position_side(
    position: &Value,
    position_amount: f64,
) -> Result<safe_cex_types::PositionSide, JsCoercionError> {
    if position.is_null() {
        return Err(JsCoercionError {
            name: "TypeError",
            message: "Cannot read properties of null (reading 'ps')",
        });
    }
    let side = position.get("ps").and_then(Value::as_str);
    let long = if side == Some("BOTH") {
        position_amount > 0.0
    } else {
        side == Some("LONG")
    };
    Ok(if long {
        safe_cex_types::PositionSide::Long
    } else {
        safe_cex_types::PositionSide::Short
    })
}

// ts: AsterDEXPrivateWebsocket.getAccountUpdateUpnlDeltas
// Ordered entries preserve Map iteration order and non-finite accumulated values.
// Currency keys must retain insertion order before JS own-key enumeration; do not
// sort them. The shared Balance.currencies JsObjectMap now provides those keys.
pub fn get_account_update_upnl_deltas(
    positions: Option<&Value>,
    existing_positions: &[safe_cex_types::Position],
    currency_keys: &[String],
) -> Option<Vec<(String, f64)>> {
    let mut deltas: Vec<(String, f64)> = Vec::new();
    let Some(positions) = positions else {
        return Some(deltas);
    };
    let positions = positions.as_array()?;
    let currency_keys = safe_cex_num::js_own_keys(currency_keys.iter().map(String::as_str));
    for position in positions {
        let symbol = position.get("s").and_then(Value::as_str).unwrap_or("");
        let amount = parse_finite_number(position.get("pa"))?;
        let upnl = parse_finite_number(position.get("up"))?;
        if symbol.is_empty() {
            return None;
        }
        // A valid symbol/amount/upnl guarantees position is an object here.
        let side = get_position_side(position, amount).ok()?;
        let previous = existing_positions
            .iter()
            .find(|p| p.symbol == symbol && p.side == side)
            .map_or(0.0, |p| p.unrealized_pnl);
        if !previous.is_finite() {
            return None;
        }
        let asset = currency_keys
            .iter()
            .copied()
            .find(|currency| symbol.ends_with(currency))
            .filter(|currency| !currency.is_empty())
            .or_else(|| symbol.ends_with("USDT").then_some("USDT"));
        if let Some(asset) = asset {
            if let Some((_, total)) = deltas.iter_mut().find(|(key, _)| key == asset) {
                // JS || clears NaN/zero, but retains either infinity.
                let prior = if total.is_nan() || *total == 0.0 {
                    0.0
                } else {
                    *total
                };
                *total = prior + upnl - previous;
            } else {
                deltas.push((asset.into(), 0.0 + upnl - previous));
            }
        }
    }
    Some(deltas)
}
