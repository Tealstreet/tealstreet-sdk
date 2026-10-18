//! Source-corresponding WEEX enum normalizers on retained JSON inputs.
use crate::types::{
    MARGIN_TYPE, ORDER_SIDE, ORDER_STATUS, ORDER_TRIGGER_TYPE, ORDER_TYPE, POSITION_MODE,
    POSITION_SIDE,
};
use safe_cex_num::numeric_ingress::{JsNumberOutput, js_deserialize_passthrough};
use safe_cex_num::{JsCoercionError, js_string, js_truthy};
use safe_cex_types::{
    MarginType, OrderSide, OrderStatus, OrderStopTriggerType, OrderType, PositionMode, PositionSide,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Retained JSON argument; absent arguments use `None` at each normalizer.
/// Capture numeric lexemes before constructing the opaque value, including overflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WeexUtilityInput {
    #[serde(deserialize_with = "js_deserialize_passthrough")]
    value: Value,
}
impl WeexUtilityInput {
    /// Project only values already captured by this input's ingress boundary.
    pub(crate) fn pick_retained(&self, keys: &[&str]) -> Option<Self> {
        keys.iter()
            .find_map(|key| self.value.get(*key).filter(|value| !value.is_null()))
            .map(|value| Self {
                value: value.clone(),
            })
    }

    /// Borrow the captured argument without rebuilding a potentially lossy Value.
    pub fn as_value(&self) -> &Value {
        &self.value
    }
}
impl JsNumberOutput for WeexUtilityInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {} passthrough { value });
    }
}

#[cfg(test)]
#[path = "../tests/support/owned_snapshots.rs"]
mod owned_snapshots;

fn normalized_key(value: Option<&WeexUtilityInput>) -> Result<String, JsCoercionError> {
    let value = value.map(|input| &input.value);
    // String(value || '').toUpperCase(), with coercion failures propagated.
    if !js_truthy(value) {
        return Ok(String::new());
    }
    Ok(js_string(value)?.to_uppercase())
}

pub fn normalize_position_mode(
    value: Option<&WeexUtilityInput>,
) -> Result<PositionMode, JsCoercionError> {
    let normalized = normalized_key(value)?;
    let found = POSITION_MODE
        .iter()
        .find(|(key, _)| *key == normalized)
        .map(|(_, value)| *value);
    Ok(found.unwrap_or(PositionMode::OneWay))
}

pub fn normalize_margin_type(
    value: Option<&WeexUtilityInput>,
) -> Result<MarginType, JsCoercionError> {
    let normalized = normalized_key(value)?;
    let found = MARGIN_TYPE
        .iter()
        .find(|(key, _)| *key == normalized)
        .map(|(_, value)| *value);
    Ok(found.unwrap_or(MarginType::Cross))
}

pub fn normalize_position_side(
    value: Option<&WeexUtilityInput>,
) -> Result<PositionSide, JsCoercionError> {
    let normalized = normalized_key(value)?;
    let found = POSITION_SIDE
        .iter()
        .find(|(key, _)| *key == normalized)
        .map(|(_, value)| *value);
    Ok(found.unwrap_or(PositionSide::Long))
}

pub fn normalize_order_side(
    value: Option<&WeexUtilityInput>,
) -> Result<Option<OrderSide>, JsCoercionError> {
    let normalized = normalized_key(value)?;
    let found = ORDER_SIDE
        .iter()
        .find(|(key, _)| *key == normalized)
        .map(|(_, value)| *value);
    Ok(found)
}

pub fn normalize_order_status(
    value: Option<&WeexUtilityInput>,
) -> Result<OrderStatus, JsCoercionError> {
    let normalized = normalized_key(value)?;
    let found = ORDER_STATUS
        .iter()
        .find(|(key, _)| *key == normalized)
        .map(|(_, value)| *value);
    Ok(found.unwrap_or(OrderStatus::Open))
}

pub fn normalize_order_type(
    value: Option<&WeexUtilityInput>,
) -> Result<OrderType, JsCoercionError> {
    let normalized = normalized_key(value)?;
    let found = ORDER_TYPE
        .iter()
        .find(|(key, _)| *key == normalized)
        .map(|(_, value)| *value);
    Ok(found.unwrap_or(OrderType::Limit))
}

pub fn normalize_trigger_type(
    value: Option<&WeexUtilityInput>,
) -> Result<OrderStopTriggerType, JsCoercionError> {
    let normalized = normalized_key(value)?;
    let found = ORDER_TRIGGER_TYPE
        .iter()
        .find(|(key, _)| *key == normalized)
        .map(|(_, value)| *value);
    Ok(found.unwrap_or(OrderStopTriggerType::LastPrice))
}

pub fn to_number(
    value: Option<&WeexUtilityInput>,
    fallback: Option<f64>,
) -> Result<f64, JsCoercionError> {
    let fallback = fallback.unwrap_or(0.0);
    let value = value.map(WeexUtilityInput::as_value);
    if value.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {
        return Ok(fallback);
    }
    let parsed = safe_cex_num::parse_float(&js_string(value)?);
    Ok(if parsed.is_finite() { parsed } else { fallback })
}

pub fn to_integer_string(value: Option<&WeexUtilityInput>) -> Result<String, JsCoercionError> {
    let value = value.map(WeexUtilityInput::as_value);
    if value.is_none_or(Value::is_null) {
        return Ok(String::new());
    }
    js_string(value)
}

pub fn normalize_percentage(
    raw_percentage: Option<&WeexUtilityInput>,
    last: f64,
    open: f64,
) -> Result<f64, JsCoercionError> {
    let parsed = to_number(raw_percentage, Some(f64::NAN))?;
    if parsed.is_finite() {
        return Ok(if parsed.abs() <= 1.0 {
            parsed * 100.0
        } else {
            parsed
        });
    }
    if open == 0.0 || open.is_nan() {
        return Ok(0.0);
    }
    Ok(((last - open) / open) * 100.0)
}

/// The caller supplies the source host's local-wall-time to UTC conversion,
/// including its DST disambiguation. No implicit UTC host is assumed here.
pub fn normalize_timestamp_to_ms(
    value: Option<&WeexUtilityInput>,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<f64, JsCoercionError> {
    let value = value.map(WeexUtilityInput::as_value);
    if value.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {
        return Ok(0.0);
    }
    let numeric = safe_cex_num::js_number(value)?;
    if numeric.is_finite() && numeric > 0.0 {
        if numeric >= 1_000_000_000_000_000_000.0 {
            return Ok((numeric / 1_000_000.0).floor());
        }
        if numeric >= 1_000_000_000_000_000.0 {
            return Ok((numeric / 1_000.0).floor());
        }
        if numeric >= 1_000_000_000_000.0 {
            return Ok(numeric.floor());
        }
        return Ok((numeric * 1000.0).floor());
    }
    let parsed_date = safe_cex_num::js_date_parse(&js_string(value)?, local_to_utc);
    Ok(if parsed_date.is_finite() {
        parsed_date
    } else {
        0.0
    })
}

pub fn normalize_timestamp_to_seconds(
    value: Option<&WeexUtilityInput>,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<f64, JsCoercionError> {
    let milliseconds = normalize_timestamp_to_ms(value, local_to_utc)?;
    Ok(if milliseconds != 0.0 {
        (milliseconds / 1000.0).floor()
    } else {
        0.0
    })
}

/// Source normalizer failures, including dynamic V8 iterable diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeexUtilityError {
    pub name: &'static str,
    pub message: String,
}
impl std::fmt::Display for WeexUtilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}
impl std::error::Error for WeexUtilityError {}
impl From<JsCoercionError> for WeexUtilityError {
    fn from(error: JsCoercionError) -> Self {
        Self {
            name: error.name,
            message: error.message.into(),
        }
    }
}
fn utility_type_error(message: impl Into<String>) -> WeexUtilityError {
    WeexUtilityError {
        name: "TypeError",
        message: message.into(),
    }
}

pub fn normalize_order_book_side(
    rows: Option<&WeexUtilityInput>,
) -> Result<Vec<safe_cex_types::OrderBookOrders>, WeexUtilityError> {
    let rows = rows.map(WeexUtilityInput::as_value);
    if !js_truthy(rows) {
        return Ok(Vec::new());
    }
    let rows = rows
        .and_then(Value::as_array)
        .ok_or_else(|| utility_type_error("(rows || []).reduce is not a function"))?;
    let mut acc = Vec::new();
    for row in rows {
        let chars;
        let values = match row {
            Value::Array(values) => values,
            Value::String(value) => {
                chars = value
                    .chars()
                    .take(2)
                    .map(|c| Value::String(c.to_string()))
                    .collect::<Vec<_>>();
                &chars
            }
            other => {
                let prefix = match other {
                    Value::Null => "object null".into(),
                    Value::Bool(value) => format!("boolean {value}"),
                    Value::Number(_) => format!("number {}", js_string(Some(other))?),
                    _ => "object".into(),
                };
                return Err(utility_type_error(format!(
                    "{prefix} is not iterable (cannot read property Symbol(Symbol.iterator))"
                )));
            }
        };
        let price = values.first().map(|value| WeexUtilityInput {
            value: value.clone(),
        });
        let amount = values.get(1).map(|value| WeexUtilityInput {
            value: value.clone(),
        });
        let parsed_price = to_number(price.as_ref(), None)?;
        let parsed_amount = to_number(amount.as_ref(), None)?;
        if parsed_price > 0.0 && parsed_amount >= 0.0 {
            acc.push(safe_cex_types::OrderBookOrders {
                price: parsed_price,
                amount: parsed_amount,
                total: 0.0,
            });
        }
    }
    Ok(acc)
}

pub fn normalize_order_book(
    raw: Option<&WeexUtilityInput>,
) -> Result<safe_cex_types::OrderBook, WeexUtilityError> {
    let value = raw.map(WeexUtilityInput::as_value);
    if value.is_none_or(Value::is_null) {
        return Err(utility_type_error(format!(
            "Cannot read properties of {} (reading 'bids')",
            if value.is_none() { "undefined" } else { "null" }
        )));
    }
    let pick = |first: &str, second: &str| {
        value
            .and_then(|v| v.get(first))
            .filter(|v| !v.is_null())
            .or_else(|| value.and_then(|v| v.get(second)))
            .map(|v| WeexUtilityInput { value: v.clone() })
    };
    let mut book = safe_cex_types::OrderBook {
        bids: normalize_order_book_side(pick("bids", "b").as_ref())?,
        asks: normalize_order_book_side(pick("asks", "a").as_ref())?,
    };
    safe_cex_store::orderbook::calc_order_book_total(&mut book);
    Ok(book)
}

pub fn fake_positions_for_market(
    market: &safe_cex_types::Market,
    position_mode: PositionMode,
    margin_type: MarginType,
    leverage: f64,
) -> Vec<safe_cex_types::Position> {
    let create_fake = |side| safe_cex_types::Position {
        id: safe_cex_types::model_utils::create_position_id(
            &market.symbol,
            Some(side),
            Some(position_mode),
        ),
        symbol: market.symbol.clone(),
        side: side.into(),
        entry_price: 0.0,
        notional: 0.0,
        leverage,
        unrealized_pnl: 0.0,
        contracts: 0.0,
        liquidation_price: 0.0,
        margin_type: (Some(margin_type)).into(),
        position_mode: (Some(position_mode)).into(),
        info: (Some(serde_json::json!({}))).into(),
        realized_pnl: (None).into(),
        margin_size: (None).into(),
        extra: (None).into(),
        last_update: (None).into(),
        option: (None).into(),
    };
    vec![
        create_fake(PositionSide::Long),
        create_fake(PositionSide::Short),
    ]
}

/// Normalize the source tuple or object, using the caller's Date.parse timezone.
pub fn normalize_candle(
    row: Option<&WeexUtilityInput>,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<Option<safe_cex_types::Candle>, WeexUtilityError> {
    let row = row.map(WeexUtilityInput::as_value);
    if row.is_none_or(Value::is_null) {
        return Err(utility_type_error(format!(
            "Cannot read properties of {} (reading 'timestamp')",
            if row.is_none() { "undefined" } else { "null" }
        )));
    }
    let pick = |index: usize, keys: &[&str]| {
        let value = if let Some(values) = row.and_then(Value::as_array) {
            values.get(index)
        } else {
            keys.iter()
                .find_map(|key| row.and_then(|v| v.get(*key)).filter(|v| !v.is_null()))
        };
        value.map(|value| WeexUtilityInput {
            value: value.clone(),
        })
    };
    let candle = safe_cex_types::Candle {
        timestamp: normalize_timestamp_to_seconds(
            pick(0, &["timestamp", "t", "time"]).as_ref(),
            local_to_utc,
        )?,
        open: to_number(pick(1, &["open", "o"]).as_ref(), None)?,
        high: to_number(pick(2, &["high", "h"]).as_ref(), None)?,
        low: to_number(pick(3, &["low", "l"]).as_ref(), None)?,
        close: to_number(pick(4, &["close", "c"]).as_ref(), None)?,
        volume: to_number(pick(5, &["volume", "v"]).as_ref(), None)?,
        last_side: None,
    };
    if [
        candle.timestamp,
        candle.open,
        candle.high,
        candle.low,
        candle.close,
        candle.volume,
    ]
    .iter()
    .any(|value| !value.is_finite())
    {
        Ok(None)
    } else {
        Ok(Some(candle))
    }
}

/// Select the first USDT row, then mirror the source's balance fallbacks.
pub fn normalize_balance(
    rows: &WeexUtilityInput,
) -> Result<safe_cex_types::Balance, WeexUtilityError> {
    let values = rows.value.as_array().ok_or_else(|| {
        utility_type_error(if rows.value.is_null() {
            "Cannot read properties of null (reading 'find')".to_owned()
        } else {
            "rows.find is not a function".to_owned()
        })
    })?;
    let mut selected = None;
    for row in values {
        if row.is_null() {
            return Err(utility_type_error(
                "Cannot read properties of null (reading 'asset')".to_owned(),
            ));
        }
        let asset = row.get("asset");
        let asset = if js_truthy(asset) {
            asset
        } else {
            row.get("coin")
        };
        if js_string(asset)?.to_uppercase() == "USDT" {
            selected = Some(row);
            break;
        }
    }
    let usdt = selected
        .or_else(|| values.first())
        .filter(|row| js_truthy(Some(row)));
    let number = |keys: &[&str]| {
        let value = keys
            .iter()
            .find_map(|key| usdt.and_then(|row| row.get(*key)).filter(|v| !v.is_null()));
        to_number(
            value
                .map(|value| WeexUtilityInput {
                    value: value.clone(),
                })
                .as_ref(),
            None,
        )
    };
    let total = number(&["balance", "amount"])?;
    let free = number(&["availableBalance", "amount"])?;
    let upnl = number(&["unrealizePnl"])?;
    let frozen = number(&["frozen"])?;
    Ok(safe_cex_types::Balance {
        total,
        free,
        used: if frozen != 0.0 {
            frozen
        } else {
            safe_cex_num::math_max(total - free, 0.0)
        },
        upnl,
        info: Some(rows.value.clone()),
        currencies: None,
        coins: None,
    })
}
