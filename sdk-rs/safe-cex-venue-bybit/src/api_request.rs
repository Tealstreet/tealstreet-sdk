//! Request interceptor composed by the Bybit API factory.
use safe_cex_crypto::{Encoding, SigningOptions, can_sign_payload, sign_payload};
use safe_cex_num::js_number_string;
use safe_cex_transport::provider::RestRequest;
use safe_cex_types::ExchangeName;
use serde_json::{Map, Value};

pub(crate) fn ordered_entries(map: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut entries: Vec<_> = map.iter().collect();
    // Object.keys visits canonical array indices before other insertion-ordered keys.
    entries.sort_by_key(|(key, _)| {
        key.parse::<u32>()
            .ok()
            .filter(|n| *n != u32::MAX && n.to_string() == **key)
            .map_or((1, 0), |n| (0, n))
    });
    entries
}

fn header_value(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|c| *c == '\t' || (*c >= ' ' && *c != '\u{7f}'))
        .collect();
    cleaned.trim_matches([' ', '\t']).to_owned()
}

fn encode(value: &str) -> String {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut out = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[(b >> 4) as usize]));
            out.push(char::from(HEX[(b & 15) as usize]));
        }
    }
    out
}

fn query_pair(key: &str, value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Array(values) => {
            for (i, value) in values.iter().enumerate() {
                query_pair(&format!("{key}[{i}]"), value, out);
            }
        }
        Value::Object(map) => {
            for (child, value) in ordered_entries(map) {
                query_pair(&format!("{key}[{child}]"), value, out);
            }
        }
        _ => {
            let value = match value {
                Value::Null => String::new(),
                Value::String(value) => value.clone(),
                Value::Number(n) => js_number_string(
                    n.as_f64()
                        .unwrap_or_else(|| safe_cex_num::number_from_string(&n.to_string())),
                ),
                _ => value.to_string(),
            };
            out.push(format!("{}={}", encode(key), encode(&value)));
        }
    }
}

fn query_bytes(params: &Map<String, Value>) -> String {
    let mut pairs = Vec::new();
    for (key, value) in ordered_entries(params) {
        query_pair(key, value, &mut pairs);
    }
    pairs.join("&")
}

fn string_own_entries(text: &str) -> Result<Vec<(String, String)>, String> {
    text.encode_utf16()
        .enumerate()
        .map(|(index, unit)| {
            char::from_u32(u32::from(unit))
                .map(|value| (index.to_string(), value.to_string()))
                .ok_or_else(|| {
                    "Bybit params spread cannot represent a lone UTF-16 code unit".into()
                })
        })
        .collect()
}

struct RequestSigning<'a> {
    exchange: ExchangeName,
    credentials: SigningOptions<'a>,
    remote_signer: bool,
    recv_window: f64,
    timestamp: &'a dyn Fn() -> f64,
    request_id: &'a str,
    site_headers: &'a Map<String, Value>,
}

async fn prepare_request(
    mut request: RestRequest,
    context: RequestSigning<'_>,
) -> Result<RestRequest, String> {
    let query = if let Some(native) = request.query_params.take() {
        if request.params.is_some() {
            return Err("Cannot combine legacy and native query parameters".into());
        }
        let mut params = match native {
            safe_cex_types::QueryValue::Object(params) => params,
            safe_cex_types::QueryValue::Array(values) => values
                .into_iter()
                .enumerate()
                .map(|(index, value)| (index.to_string(), value))
                .collect(),
            safe_cex_types::QueryValue::String(text) => string_own_entries(&text)?
                .into_iter()
                .map(|(key, value)| (key, safe_cex_types::QueryValue::String(value)))
                .collect(),
            safe_cex_types::QueryValue::Null
            | safe_cex_types::QueryValue::Undefined
            | safe_cex_types::QueryValue::Bool(_)
            | safe_cex_types::QueryValue::Number(_) => Default::default(),
        };
        params.insert(
            "requestId".into(),
            safe_cex_types::QueryValue::String(context.request_id.into()),
        );
        let native = safe_cex_types::QueryValue::Object(params);
        let query = safe_cex_transport::query::render_query(
            &native,
            safe_cex_transport::query::QueryProfile {
                key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
                arrays: safe_cex_transport::query::QueryArrayFormat::Indexed,
                encode: true,
            },
        )
        .map_err(|error| error.to_string())?;
        request.query_params = Some(native);
        query
    } else {
        let mut params = match request.params.take() {
            Some(Value::Object(map)) => map,
            Some(Value::Array(values)) => values
                .into_iter()
                .enumerate()
                .map(|(index, value)| (index.to_string(), value))
                .collect(),
            Some(Value::String(text)) => string_own_entries(&text)?
                .into_iter()
                .map(|(key, value)| (key, Value::String(value)))
                .collect(),
            None | Some(Value::Null | Value::Bool(_) | Value::Number(_)) => Map::new(),
        };
        params.insert("requestId".into(), Value::String(context.request_id.into()));
        let query = query_bytes(&params);
        request.params = Some(Value::Object(params));
        query
    };
    if super::api::is_public_endpoint(Some(&request.url)) {
        request.skip_remote_signing = true;
    }
    if request.method == "get" && request.url == "/v5/market/kline" {
        request.headers.clear();
        for (k, v) in context.site_headers {
            request.headers.insert(
                k.clone(),
                header_value(v.as_str().ok_or("Bybit site header must be a string")?),
            );
        }
        return Ok(request);
    }
    if context.remote_signer
        || context.credentials.key.is_empty()
        || !can_sign_payload(&context.credentials)
    {
        return Ok(request);
    }
    let data = if request.method == "get" {
        query
    } else if let Some(body) = &request.body {
        // A resent config carries the body axios already serialized; sign it as sent.
        match body.untransformed().map_err(|e| e.to_string())? {
            safe_cex_types::retained_json::RetainedJsonInput::Text(text) => {
                String::from_utf16(text).map_err(|error| error.to_string())?
            }
            value => safe_cex_num::js_retained_json_stringify(value)
                .map_err(|error| error.to_string())?
                .unwrap_or_default(),
        }
    } else {
        String::new()
    };
    let timestamp = js_number_string((context.timestamp)());
    let payload = format!(
        "{}{}{}{}",
        timestamp,
        context.credentials.key,
        js_number_string(context.recv_window),
        data
    );
    // Cancellation during the signer await changes only this owned request, never shared state.
    let signed = sign_payload(
        &context.credentials,
        context.exchange,
        &payload,
        Encoding::Hex,
    )
    .await?;
    // Signer headers last: a replayed config still carries the previous signature's.
    for (key, value) in signed.headers.unwrap_or_default() {
        request.headers.insert(key, value);
    }
    request
        .headers
        .insert("X-BAPI-SIGN".into(), signed.signature);
    request
        .headers
        .insert("X-BAPI-API-KEY".into(), context.credentials.key.into());
    request.headers.insert("X-BAPI-TIMESTAMP".into(), timestamp);
    // AxiosHeaders normalizes case after the object spreads, retaining the first spelling.
    let entries = std::mem::take(&mut request.headers);
    for (key, value) in entries {
        let existing = request
            .headers
            .keys()
            .find(|old| old.eq_ignore_ascii_case(&key))
            .cloned();
        request
            .headers
            .insert(existing.unwrap_or(key), header_value(&value));
    }
    Ok(request)
}

#[cfg(test)]
mod tests;

/// The tunnel wraps a non-JSON body as `{ raw_response }`; a direct adapter hands the string.
pub(crate) fn blocked_by_cdn(data: &Value) -> bool {
    let raw = match data {
        Value::String(text) => Some(text.as_str()),
        Value::Object(map) => map.get("raw_response").and_then(Value::as_str),
        _ => None,
    };
    raw.is_some_and(|text| text.to_ascii_lowercase().contains("cloudfront"))
}

/// CloudFront refusing the egress address answers 403 before Bybit sees the request:
/// not the key, so `blocked_by_cdn` keeps it out of the auth wording and backoff.
pub(crate) fn http_failure_message(
    status: u16,
    status_text: Option<&str>,
    blocked_by_cdn: bool,
) -> String {
    match status {
        _ if blocked_by_cdn => {
            format!("HTTP {status} Forbidden — blocked by Bybit's CDN before reaching Bybit")
        }
        401 => "HTTP 401 Unauthorized — API key rejected".into(),
        403 => "HTTP 403 Forbidden — API key rejected".into(),
        _ => match status_text.filter(|reason| !reason.is_empty()) {
            Some(reason) => format!("HTTP {status} {reason}"),
            None => format!("HTTP {status}"),
        },
    }
}

pub(crate) mod client;
