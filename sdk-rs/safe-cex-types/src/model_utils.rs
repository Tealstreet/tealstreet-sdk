//! Model utilities; behavioral reference: packages/safe-cex/src/utils/CLAUDE.md.
use crate::{IncomeCategory, PositionMode, PositionSide};
use serde_json::{Map, Value};

use crate::js_coercion::{JsCoercionError, js_string};
fn string(value: &Value) -> Result<String, JsCoercionError> {
    js_string(Some(value))
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(n) => n.as_f64().is_none_or(|v| v != 0.0 && !v.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

pub fn income_category_from_type(value: Option<&Value>) -> Result<IncomeCategory, JsCoercionError> {
    let text = value
        .filter(|v| !v.is_null())
        .map(string)
        .transpose()?
        .unwrap_or_default()
        .to_uppercase();
    Ok(match text.as_str() {
        "REALIZED_PNL" | "DELIVERED_SETTELMENT" => IncomeCategory::RealizedPnl,
        "FUNDING_FEE" => IncomeCategory::Funding,
        "COMMISSION" | "OPTIONS_PREMIUM_FEE" | "POSITION_LIMIT_INCREASE_FEE" => {
            IncomeCategory::Commission
        }
        "COMMISSION_REBATE" | "REFERRAL_KICKBACK" | "API_REBATE" => IncomeCategory::Rebate,
        "TRANSFER"
        | "INTERNAL_TRANSFER"
        | "CROSS_COLLATERAL_TRANSFER"
        | "COIN_SWAP_DEPOSIT"
        | "COIN_SWAP_WITHDRAW"
        | "AUTO_EXCHANGE" => IncomeCategory::Transfer,
        "INSURANCE_CLEAR" => IncomeCategory::Liquidation,
        _ => IncomeCategory::Other,
    })
}

pub fn normalize_execution_position_side(
    value: Option<&Value>,
) -> Result<&'static str, JsCoercionError> {
    let text = value
        .filter(|v| !v.is_null())
        .map(string)
        .transpose()?
        .unwrap_or_default();
    Ok(match text.trim_matches(|c| matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')).to_lowercase().as_str() {
        "long" | "open long" | "close long" => "long",
        "short" | "open short" | "close short" => "short",
        _ => "both",
    })
}

pub fn create_position_id(
    symbol: &str,
    side: Option<PositionSide>,
    mode: Option<PositionMode>,
) -> String {
    if mode == Some(PositionMode::OneWay) || side.is_none() {
        return symbol.into();
    }
    format!(
        "{symbol}:{}",
        if side == Some(PositionSide::Long) {
            "long"
        } else {
            "short"
        }
    )
}

fn entries(object: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut entries: Vec<_> = object.iter().collect();
    entries.sort_by_key(|(key, _)| {
        crate::js_coercion::js_array_index(key).map_or((1, 0), |index| (0, index))
    });
    entries
}
fn replace_all(text: &str, needle: &str, replacement: &str) -> Result<String, JsCoercionError> {
    let result = crate::js_coercion::js_string_replace_utf16(
        &text.encode_utf16().collect::<Vec<_>>(),
        &needle.encode_utf16().collect::<Vec<_>>(),
        &replacement.encode_utf16().collect::<Vec<_>>(),
        true,
    );
    String::from_utf16(&result).map_err(|_| JsCoercionError {
        name: "RepresentationError",
        message: "replacement output contains a lone UTF-16 surrogate",
    })
}

pub fn inverse_obj(object: &Map<String, Value>) -> Result<Map<String, Value>, JsCoercionError> {
    {
        let inverted: Map<_, _> = entries(object)
            .into_iter()
            .map(|(key, value)| Ok((string(value)?, Value::String(key.clone()))))
            .collect::<Result<_, JsCoercionError>>()?;
        Ok(entries(&inverted)
            .into_iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect())
    }
}

// ts-bug: replaceUrlParams — ordinary-object lookup includes Object.prototype.
fn inherited_param(key: &str) -> Option<Value> {
    match key {
        "__proto__" => Some(Value::String("[object Object]".into())),
        "constructor" => Some(Value::String("function Object() { [native code] }".into())),
        "__defineGetter__"
        | "__defineSetter__"
        | "hasOwnProperty"
        | "__lookupGetter__"
        | "__lookupSetter__"
        | "isPrototypeOf"
        | "propertyIsEnumerable"
        | "toString"
        | "valueOf"
        | "toLocaleString" => Some(Value::String(format!(
            "function {key}() {{ [native code] }}"
        ))),
        _ => None,
    }
}

type UrlParamsReplacement = (Option<String>, Map<String, Value>);
pub fn replace_url_params(
    url: Option<&str>,
    params: &Map<String, Value>,
) -> Result<UrlParamsReplacement, JsCoercionError> {
    let Some(url) = url.filter(|s| !s.is_empty()) else {
        return Ok((url.map(str::to_owned), params.clone()));
    };
    let mut url = url.to_owned();
    let mut params = params.clone();
    let mut start = 0;
    let original = url.clone();
    while let Some(offset) = original[start..].find('{') {
        let begin = start + offset;
        let Some(end) = original[begin..].find('}') else {
            break;
        };
        let end = begin + end;
        let key = &original[begin + 1..end];
        if !key.is_empty()
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && let Some(value) = params
                .get(key)
                .cloned()
                .or_else(|| inherited_param(key))
                .filter(truthy)
        {
            let text = if value
                .as_object()
                .is_some_and(|object| object.contains_key("toString"))
            {
                return Err(JsCoercionError {
                    name: "TypeError",
                    message: "params[key].toString is not a function",
                });
            } else {
                string(&value)?
            };
            url = replace_all(&url, &original[begin..=end], &text)?;
            params.shift_remove(key);
        }
        start = begin + 1;
    }
    Ok((Some(url), params))
}

pub fn is_plain_object(value: &Value) -> bool {
    value.is_object()
}
pub fn fast_clone(value: &Value) -> Value {
    match value {
        Value::Array(array) => Value::Array(array.iter().map(fast_clone).collect()),
        Value::Object(object) => Value::Object(
            entries(object)
                .into_iter()
                // ts-bug: fastClone — assigning __proto__ uses the legacy setter, not an own key.
                .filter(|(key, _)| key.as_str() != "__proto__")
                .map(|(key, value)| (key.clone(), fast_clone(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}
pub fn clone(value: &Value) -> Value {
    fast_clone(value)
}

pub fn omit_undefined(object: &[(String, Option<Value>)]) -> Map<String, Value> {
    // ts-bug: omitUndefined — lodash baseSet drops these three own keys.
    let filtered: Map<String, Value> = object
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "__proto__" | "constructor" | "prototype"))
        .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone())))
        .collect();
    entries(&filtered)
        .into_iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
