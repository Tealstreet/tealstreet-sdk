//! ApeX qs serializers over prepared data; preserve falsy values on the primary path.
use crate::data::truthy;
use safe_cex_num::{JsJsonValue, JsRootCollator, js_number_string};

fn encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            output.push(byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

fn entries(value: &JsJsonValue) -> Vec<(String, &JsJsonValue)> {
    match value {
        JsJsonValue::Object(values) => {
            let mut result: Vec<(String, &JsJsonValue)> = Vec::new();
            for (key, value) in values {
                if let Some(prior) = result.iter_mut().find(|(name, _)| name == key) {
                    prior.1 = value;
                } else {
                    result.push((key.clone(), value));
                }
            }
            result
        }
        JsJsonValue::Array(values) => values
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect(),
        _ => Vec::new(),
    }
}

fn append(
    value: &JsJsonValue,
    prefix: &str,
    legacy: bool,
    collator: &JsRootCollator,
    output: &mut Vec<String>,
) {
    if matches!(value, JsJsonValue::Undefined | JsJsonValue::Null) || legacy && !truthy(value) {
        return;
    }
    let scalar = match value {
        JsJsonValue::String(value) => Some(value.clone()),
        JsJsonValue::Number(value) => Some(js_number_string(*value)),
        JsJsonValue::Bool(value) => Some(value.to_string()),
        JsJsonValue::BigInt(value) => Some(value.to_string()),
        _ => None,
    };
    if let Some(value) = scalar {
        output.push(format!("{}={}", encode(prefix), encode(&value)));
    } else {
        let mut children = entries(value);
        children.sort_by(|(a, _), (b, _)| collator.compare(a, b));
        for (key, child) in children {
            append(child, &format!("{prefix}[{key}]"), legacy, collator, output);
        }
    }
}

fn serialize(payload: &JsJsonValue, legacy: bool) -> Result<String, String> {
    let collator = JsRootCollator::try_new().map_err(|e| e.to_string())?;
    let mut output = Vec::new();
    let mut keys = entries(payload);
    keys.sort_by(|(a, _), (b, _)| collator.compare(a, b));
    for (key, value) in keys {
        append(value, &key, legacy, &collator, &mut output);
    }
    Ok(output.join("&"))
}

// ts: serializeApexPayload
pub fn serialize_apex_payload(payload: &JsJsonValue) -> Result<String, String> {
    if let JsJsonValue::String(value) = payload {
        return Ok(value.clone());
    }
    serialize(payload, false)
}

// ts: serializeApexPayloadLegacy
pub fn serialize_apex_payload_legacy(payload: &JsJsonValue) -> Result<String, String> {
    serialize(payload, true)
}
