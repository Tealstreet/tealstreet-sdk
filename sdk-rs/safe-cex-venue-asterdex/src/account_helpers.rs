//! Pure private-stream account parsers. Socket lifecycle is owned by BaseWebSocket.
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_num::{JsCoercionError, js_string, js_truthy, parse_float};
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
    let parse = |key: &str| -> Result<f64, JsCoercionError> {
        let value = data.get(key);
        if js_truthy(value) {
            Ok(parse_float(&js_string(value)?))
        } else {
            Ok(0.)
        }
    };
    let filled = parse("z")?;
    let amount = parse("q")?;
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
    order_data: &Js,
    current_time_ms: impl FnOnce() -> f64,
) -> Result<Option<CumulativeOrderFill>, Thrown> {
    let id = order_data.get("i")?;
    let order_id = if id.is_nullish() {
        None
    } else {
        let method = id.get("toString")?;
        if !method.is_callable() {
            return Err(Thrown::type_error(
                "orderData.i?.toString is not a function",
            ));
        }
        Some(String::from_utf16_lossy(
            &method.call(id, vec![])?.to_js_string()?,
        ))
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
        status: if order_data.get("X")?.strict_equals(&Js::str("FILLED")) {
            safe_cex_types::OrderStatus::Closed
        } else {
            safe_cex_types::OrderStatus::Open
        },
        filled,
        remaining: (amount - filled).max(0.),
        last_update,
    }))
}
fn parse_order_number(data: &Js, key: &str) -> Result<f64, Thrown> {
    let value = data.get(key)?;
    if value.truthy() {
        value.parse_float()
    } else {
        Ok(0.)
    }
}
// ts: AsterDEXPrivateWebsocket.parseFiniteNumber
pub fn parse_finite_number(value: &Js) -> Option<f64> {
    match value {
        Js::Number(_) => {}
        Js::String(units)
            if !safe_cex_num::js_trim(&String::from_utf16_lossy(units)).is_empty() => {}
        _ => return None,
    }
    let number = value.to_number().ok()?;
    number.is_finite().then_some(number)
}
// ts: AsterDEXPrivateWebsocket.getOrderUpdateTime
pub fn get_order_update_time(
    data: &Js,
    current_time_ms: impl FnOnce() -> f64,
) -> Result<f64, Thrown> {
    let time = data.get("T")?;
    let time = if time.truthy() { time } else { data.get("E")? };
    let time = time.to_number()?;
    Ok(if time.is_finite() && time > 0. {
        time
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
        && let Some(balances) = parse_asset_balance_updates(
            &safe_cex_exchange::js_runtime::value::from_json(data.get("B").unwrap_or(&Value::Null)),
            None,
            "f",
            Some("l"),
        )
        .ok()
        .flatten()
    {
        effects.push(AccountUpdateEffect::ApplyBalances(balances));
    }
    effects.push(AccountUpdateEffect::EmitAccount(data));
    Ok(effects)
}

// ts: AsterDEXPrivateWebsocket.parseAssetBalanceUpdates
pub fn parse_asset_balance_updates(
    balances: &Js,
    total_field: Option<&str>,
    free_field: &str,
    used_field: Option<&str>,
) -> Result<Option<Vec<AssetBalanceUpdate>>, Thrown> {
    let Js::Array(items) = balances else {
        return Ok(None);
    };
    let mut updates = Vec::with_capacity(items.len());
    let iterator = balances.iterator()?;
    loop {
        let step = iterator.call_method("next", vec![])?;
        if step.get("done")?.truthy() {
            break;
        }
        let balance = step.get("value")?;
        let get = |key: &str| {
            if balance.is_nullish() {
                Ok(Js::Undefined)
            } else {
                balance.get(key)
            }
        };
        let asset = match get("a")? {
            Js::String(_) => String::from_utf16_lossy(
                &balance
                    .get("a")?
                    .call_method("trim", vec![])?
                    .to_js_string()?,
            ),
            _ => String::new(),
        };
        let free = parse_finite_number(&get(free_field)?);
        let used = if let Some(field) = used_field.filter(|field| !field.is_empty()) {
            parse_finite_number(&get(field)?)
        } else {
            None
        };
        let total = if let Some(field) = total_field.filter(|field| !field.is_empty()) {
            parse_finite_number(&get(field)?)
        } else {
            free.zip(used).map(|(a, b)| a + b)
        };
        let used = used.or_else(|| total.zip(free).map(|(a, b)| a - b));
        if asset.is_empty() {
            return Ok(None);
        }
        let (Some(total), Some(free), Some(used)) = (total, free, used) else {
            return Ok(None);
        };
        if used < 0. {
            return Ok(None);
        }
        updates.push(AssetBalanceUpdate {
            asset,
            total,
            free,
            used,
        });
    }
    Ok(Some(updates))
}

// ts: AsterDEXPrivateWebsocket.getPositionSide
pub fn get_position_side(
    position: &Js,
    amount: f64,
) -> Result<safe_cex_types::PositionSide, Thrown> {
    let side = position.get("ps")?;
    let long = if side.strict_equals(&Js::str("BOTH")) {
        amount > 0.
    } else {
        position.get("ps")?.strict_equals(&Js::str("LONG"))
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
    positions: &Js,
    existing_positions: &[safe_cex_types::Position],
    currency_keys: &[String],
) -> Result<Option<Vec<(String, f64)>>, Thrown> {
    let mut deltas: Vec<(String, f64)> = Vec::new();
    if matches!(positions, Js::Undefined) {
        return Ok(Some(deltas));
    }
    let Js::Array(_) = positions else {
        return Ok(None);
    };
    let iterator = positions.iterator()?;
    let currency_keys = safe_cex_num::js_own_keys(currency_keys.iter().map(String::as_str));
    loop {
        let step = iterator.call_method("next", vec![])?;
        if step.get("done")?.truthy() {
            break;
        }
        let position = step.get("value")?;
        let get = |key: &str| {
            if position.is_nullish() {
                Ok(Js::Undefined)
            } else {
                position.get(key)
            }
        };
        let symbol = match get("s")? {
            Js::String(_) => String::from_utf16_lossy(&position.get("s")?.to_js_string()?),
            _ => String::new(),
        };
        let Some(amount) = parse_finite_number(&get("pa")?) else {
            return Ok(None);
        };
        let Some(upnl) = parse_finite_number(&get("up")?) else {
            return Ok(None);
        };
        if symbol.is_empty() {
            return Ok(None);
        }
        // A valid symbol/amount/upnl guarantees position is an object here.
        let side = get_position_side(&position, amount)?;
        let previous = existing_positions
            .iter()
            .find(|p| p.symbol == symbol && p.side == side)
            .map_or(0.0, |p| p.unrealized_pnl);
        if !previous.is_finite() {
            return Ok(None);
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
    Ok(Some(deltas))
}
