//! TP/SL identifiers, trigger selection and reduce-only cache precedence.
use safe_cex_num::{JsCoercionError, js_retained_string};
use safe_cex_types::OrderStopTriggerType;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub enum TpslLeg {
    Tp,
    Sl,
}

pub fn get_tpsl_leg_order_id(
    raw_order_id: Option<&Input>,
    position_id: Option<&Input>,
    leg: TpslLeg,
    disambiguate: bool,
) -> Result<String, JsCoercionError> {
    let order_id = if matches!(raw_order_id, None | Some(Input::Undefined | Input::Null)) {
        String::new()
    } else {
        js_retained_string(raw_order_id)?
    };
    if !disambiguate {
        return Ok(order_id);
    }
    let position_part = if matches!(position_id, None | Some(Input::Undefined | Input::Null))
        || matches!(position_id, Some(Input::Text(s)) if s.is_empty())
    {
        "position".to_owned()
    } else {
        js_retained_string(position_id)?
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
    let value = raw_trigger_type
        .filter(|value| value.is_truthy())
        .cloned()
        .unwrap_or_else(|| Input::Text(vec![]));
    let units = safe_cex_types::js_coercion::js_retained_string_utf16(&value)?;
    use safe_cex_exchange::js_runtime::value::Js;
    let raw = Js::utf16(&units)
        .call_method("toUpperCase", vec![])
        .expect("stock String.toUpperCase cannot throw");
    if raw.strict_equals(&Js::str("MARK")) || raw.strict_equals(&Js::str("MARK_PRICE")) {
        return Ok("MARK_PRICE");
    }
    if raw.strict_equals(&Js::str("LAST")) || raw.strict_equals(&Js::str("LAST_PRICE")) {
        return Ok("LAST_PRICE");
    }
    Ok(if trigger_type == Some(OrderStopTriggerType::LastPrice) {
        "LAST_PRICE"
    } else {
        "MARK_PRICE"
    })
}

pub fn get_bitunix_reduce_only(
    raw_reduce_only: Option<&Input>,
    client_id: Option<&Input>,
    fallback: bool,
    cache: &HashMap<String, bool>,
) -> Result<bool, JsCoercionError> {
    if let Some(Input::Bool(value)) = raw_reduce_only {
        return Ok(*value);
    }
    if let Some(Input::Text(value)) = raw_reduce_only {
        use safe_cex_exchange::js_runtime::value::Js;
        let value = Js::utf16(value)
            .call_method("toLowerCase", vec![])
            .expect("stock String.toLowerCase cannot throw");
        if value.strict_equals(&Js::str("true")) {
            return Ok(true);
        }
        if value.strict_equals(&Js::str("false")) {
            return Ok(false);
        }
    }
    let cached = if matches!(client_id, None | Some(Input::Undefined | Input::Null)) {
        None
    } else {
        cache.get(&js_retained_string(client_id)?).copied()
    };
    Ok(cached.unwrap_or(fallback))
}
