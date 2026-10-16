//! Pure query and public-route helpers shared by the venue's API factories.
use indexmap::IndexMap;
use safe_cex_num::{JsJsonValue, js_number_string, js_own_keys};

// ts: stringifyQuery
/// `qs.stringify` default indexed-array form over prepared, acyclic data.
/// Undefined entries and empty containers emit nothing; null emits an empty value.
/// Date/Buffer instances and custom object hooks need a richer host value model.
pub fn stringify_query(params: &JsJsonValue, encode: bool) -> String {
    let mut output = Vec::new();
    entries(params, |key, value| append(key, value, encode, &mut output));
    output.join("&")
}

fn entries(value: &JsJsonValue, mut visit: impl FnMut(&str, &JsJsonValue)) {
    match value {
        JsJsonValue::Object(values) => {
            // JS assignment keeps first insertion position and last assigned value.
            let map: IndexMap<&str, &JsJsonValue> =
                values.iter().map(|(k, v)| (k.as_str(), v)).collect();
            for key in js_own_keys(map.keys().copied()) {
                visit(key, map[key]);
            }
        }
        JsJsonValue::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                visit(&index.to_string(), value);
            }
        }
        _ => {}
    }
}

fn append(key: &str, value: &JsJsonValue, encode: bool, output: &mut Vec<String>) {
    let scalar = match value {
        JsJsonValue::Undefined => return,
        JsJsonValue::Null => String::new(),
        JsJsonValue::Bool(value) => value.to_string(),
        JsJsonValue::Number(value) => js_number_string(*value),
        JsJsonValue::String(value) => value.clone(),
        JsJsonValue::BigInt(value) => value.to_string(),
        JsJsonValue::Array(_) | JsJsonValue::Object(_) => {
            entries(value, |child, value| {
                append(&format!("{key}[{child}]"), value, encode, output)
            });
            return;
        }
    };
    output.push(if encode {
        format!("{}={}", escape(key), escape(&scalar))
    } else {
        format!("{key}={scalar}")
    });
}

fn escape(value: &str) -> String {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    encoded
}

// ts: isPublicEndpoint
pub fn is_public_endpoint(url: Option<&str>) -> bool {
    url.is_some_and(|url| {
        !url.is_empty()
            && crate::spot_types::KUCOIN_SPOT_PUBLIC_ENDPOINTS
                .iter()
                .any(|endpoint| url.starts_with(endpoint))
    })
}
