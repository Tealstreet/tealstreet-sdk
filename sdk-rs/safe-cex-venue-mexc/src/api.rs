//! MEXC request primitives; query collation and the client are separate pending rows.
use crate::types::PUBLIC_ENDPOINTS;
use hmac::{Hmac, Mac};
use safe_cex_num::{JsCoercionError, JsJsonValue};
use sha2::Sha256;

pub fn sign_mexc_request(
    secret: &str,
    api_key: &str,
    request_time: &str,
    request_param: &str,
) -> Result<String, String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|e| e.to_string())?;
    mac.update(format!("{api_key}{request_time}{request_param}").as_bytes());
    Ok(mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub fn sign_mexc_ws_login(secret: &str, api_key: &str, req_time: &str) -> Result<String, String> {
    sign_mexc_request(secret, api_key, req_time, "")
}

// ts: isMexcSignatureRejection
pub fn is_mexc_signature_rejection(data: &serde_json::Value) -> Result<bool, JsCoercionError> {
    let field = |key| data.as_object().and_then(|object| object.get(key));
    if safe_cex_num::js_number(field("code"))? == 602.0 {
        return Ok(true);
    }
    let detail = [field("message"), field("msg")]
        .into_iter()
        .flatten()
        .find(|value| !value.is_null());
    let text = match detail {
        Some(value) => safe_cex_num::js_string(Some(value))?,
        None => String::new(),
    };
    // A non-unicode /i regex never folds a non-ASCII unit onto an ASCII one.
    Ok(text
        .to_ascii_lowercase()
        .contains("signature verification failed"))
}

pub fn is_mexc_public_endpoint(url: Option<&str>) -> bool {
    let Some(url) = url.filter(|s| !s.is_empty()) else {
        return false;
    };
    let path = url.split('?').next().unwrap_or("");
    PUBLIC_ENDPOINTS.as_array().is_some_and(|endpoints| {
        endpoints.iter().any(|endpoint| {
            endpoint.as_str().is_some_and(|endpoint| {
                path == endpoint || path.starts_with(&format!("{endpoint}/"))
            })
        })
    })
}

pub fn normalize_request_data(
    data: &safe_cex_exchange::js_runtime::value::Js,
) -> Result<String, safe_cex_exchange::js_runtime::value::Thrown> {
    use safe_cex_exchange::js_runtime::value::{Js, Thrown};
    let units = match data {
        Js::Undefined | Js::Null => Vec::new(),
        Js::String(units) => units.to_vec(),
        _ => safe_cex_exchange::js_runtime::globals::stringify_json(data)?.unwrap_or_default(),
    };
    String::from_utf16(&units).map_err(|_| {
        Thrown::error(
            "RepresentationError",
            "request text is outside the UTF-8 preimage boundary",
        )
    })
}

// ts: serializeMexcParams. qs's default RFC3986 codec encodes brackets too.
fn query_encode(text: &str) -> String {
    let mut encoded = String::new();
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[(byte >> 4) as usize]));
            encoded.push(char::from(HEX[(byte & 15) as usize]));
        }
    }
    encoded
}

pub fn serialize_mexc_params(params: &JsJsonValue) -> Result<String, JsCoercionError> {
    use safe_cex_num::{JsRootCollator, js_number_string, js_own_keys};
    fn entries(value: &JsJsonValue) -> Vec<(String, &JsJsonValue)> {
        match value {
            JsJsonValue::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v))
                .collect(),
            JsJsonValue::Object(items) => {
                let mut keys = Vec::new();
                let mut values = std::collections::HashMap::new();
                for (key, value) in items {
                    if values.insert(key.as_str(), value).is_none() {
                        keys.push(key.as_str());
                    }
                }
                js_own_keys(keys)
                    .into_iter()
                    .filter_map(|key| values.get(key).map(|value| (key.to_owned(), *value)))
                    .collect()
            }
            _ => vec![],
        }
    }
    fn write(
        prefix: &str,
        value: &JsJsonValue,
        collator: &JsRootCollator,
        output: &mut Vec<String>,
    ) {
        let scalar = match value {
            JsJsonValue::Undefined => return,
            JsJsonValue::Null => Some(String::new()),
            JsJsonValue::String(s) => Some(s.clone()),
            JsJsonValue::Number(n) => Some(js_number_string(*n)),
            JsJsonValue::Bool(v) => Some(v.to_string()),
            JsJsonValue::BigInt(v) => Some(v.to_string()),
            _ => None,
        };
        if let Some(text) = scalar {
            output.push(format!("{}={}", query_encode(prefix), query_encode(&text)));
            return;
        }
        let mut keys = entries(value);
        keys.sort_by(|(a, _), (b, _)| collator.compare(a, b));
        for (key, child) in keys {
            let next = if matches!(value, JsJsonValue::Array(_)) {
                prefix.to_owned()
            } else {
                format!("{prefix}[{key}]")
            };
            write(&next, child, collator, output);
        }
    }
    let collator = JsRootCollator::try_new()?;
    let mut keys = entries(params);
    keys.sort_by(|(a, _), (b, _)| collator.compare(a, b));
    let mut output = vec![];
    for (key, value) in keys {
        write(&key, value, &collator, &mut output);
    }
    Ok(output.join("&"))
}

pub fn get_query_string(
    params: &JsJsonValue,
    url: Option<&str>,
) -> Result<String, JsCoercionError> {
    // Project the richer JSON/BigInt transport into the shared truthiness
    // primitive's Value domain. Objects/arrays need no recursive conversion;
    // infinity uses a finite nonzero representative, NaN the null representative.
    let projected = match params {
        JsJsonValue::Undefined | JsJsonValue::Null => serde_json::Value::Null,
        JsJsonValue::Bool(v) => serde_json::Value::Bool(*v),
        JsJsonValue::Number(v) => {
            serde_json::Number::from_f64(if v.is_infinite() { 1.0 } else { *v })
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        JsJsonValue::String(v) => serde_json::Value::String(v.clone()),
        JsJsonValue::BigInt(v) => {
            serde_json::Value::Number(v.to_string().parse().map_err(|_| JsCoercionError {
                name: "TypeError",
                message: "Cannot project BigInt query parameters",
            })?)
        }
        _ => serde_json::Value::Array(vec![]),
    };
    let truthy = safe_cex_num::js_truthy(Some(&projected));
    if truthy {
        serialize_mexc_params(params)
    } else {
        Ok(url.unwrap_or("").split('?').nth(1).unwrap_or("").to_owned())
    }
}
