//! TP/SL identifiers, trigger selection and reduce-only cache precedence.
use safe_cex_num::{JsCoercionError, js_string};
use safe_cex_types::OrderStopTriggerType;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub enum TpslLeg {
    Tp,
    Sl,
}

pub fn get_tpsl_leg_order_id(
    raw_order_id: Option<&Value>,
    position_id: Option<&Value>,
    leg: TpslLeg,
    disambiguate: bool,
) -> Result<String, JsCoercionError> {
    let order_id = if matches!(raw_order_id, None | Some(Value::Null)) {
        String::new()
    } else {
        js_string(raw_order_id)?
    };
    if !disambiguate {
        return Ok(order_id);
    }
    let position_part = if matches!(position_id, None | Some(Value::Null))
        || matches!(position_id, Some(Value::String(s)) if s.is_empty())
    {
        "position".to_owned()
    } else {
        js_string(position_id)?
    };
    let leg = match leg {
        TpslLeg::Tp => "tp",
        TpslLeg::Sl => "sl",
    };
    Ok(format!("{order_id}__{position_part}__{leg}"))
}

pub fn get_bitunix_trigger_type(
    trigger_type: Option<OrderStopTriggerType>,
    raw_trigger_type: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
) -> Result<&'static str, JsCoercionError> {
    let raw = if raw_trigger_type.is_some_and(|value| value.is_truthy()) {
        safe_cex_num::js_retained_string(raw_trigger_type)?
    } else {
        String::new()
    }
    .to_uppercase();
    if raw == "MARK" || raw == "MARK_PRICE" {
        return Ok("MARK_PRICE");
    }
    if raw == "LAST" || raw == "LAST_PRICE" {
        return Ok("LAST_PRICE");
    }
    Ok(if trigger_type == Some(OrderStopTriggerType::LastPrice) {
        "LAST_PRICE"
    } else {
        "MARK_PRICE"
    })
}

pub fn get_bitunix_reduce_only(
    raw_reduce_only: Option<&Value>,
    client_id: Option<&Value>,
    fallback: bool,
    cache: &HashMap<String, bool>,
) -> Result<bool, JsCoercionError> {
    if let Some(Value::Bool(value)) = raw_reduce_only {
        return Ok(*value);
    }
    if let Some(Value::String(value)) = raw_reduce_only {
        if value.to_lowercase() == "true" {
            return Ok(true);
        }
        if value.to_lowercase() == "false" {
            return Ok(false);
        }
    }
    let cached = if matches!(client_id, None | Some(Value::Null)) {
        None
    } else {
        cache.get(&js_string(client_id)?).copied()
    };
    Ok(cached.unwrap_or(fallback))
}
