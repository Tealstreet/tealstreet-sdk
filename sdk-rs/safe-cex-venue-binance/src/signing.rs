//! ts: signBinanceRequest. The Axios config remains structured until signing completes.
use safe_cex_crypto::{Encoding, SigningOptions, can_sign_payload, sign_payload};
use safe_cex_num::{js_number_string, js_truthy};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_types::ExchangeName;
use serde_json::{Map, Value};

fn index(key: &str) -> Option<u32> {
    key.parse::<u32>()
        .ok()
        .filter(|n| *n != u32::MAX && n.to_string() == key)
}
fn entries(map: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut rows: Vec<_> = map.iter().collect();
    rows.sort_by_key(|(key, _)| index(key).map_or((1, 0), |i| (0, i)));
    rows
}
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => js_number_string(n.as_f64().unwrap_or(f64::NAN)),
        Value::Bool(v) => v.to_string(),
        Value::Null => String::new(),
        Value::Array(items) => items.iter().map(text).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}
fn header_value(value: &Value) -> Value {
    match value {
        Value::Null | Value::Bool(false) => value.clone(),
        Value::Array(items) => Value::Array(items.iter().map(header_value).collect()),
        _ => Value::String(
            text(value)
                .chars()
                .filter(|c| !matches!(u32::from(*c), 0..=8 | 10..=31 | 127))
                .collect::<String>()
                .trim_matches([' ', '\t'])
                .to_owned(),
        ),
    }
}
fn encode(value: &str) -> String {
    let mut out = String::new();
    const HEX: &[u8] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[(byte >> 4) as usize]));
            out.push(char::from(HEX[(byte & 15) as usize]));
        }
    }
    out
}
fn query_field(key: &str, value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                query_field(key, item, output);
            }
        }
        Value::Object(map) => {
            for (name, item) in entries(map) {
                query_field(&format!("{key}[{name}]"), item, output);
            }
        }
        _ => output.push(format!("{}={}", encode(key), encode(&text(value)))),
    }
}
/// Factory codec corresponding to qs.stringify(params, { arrayFormat: 'repeat' }).
pub fn serialize_query(params: &Value) -> String {
    let mut fields = vec![];
    match params {
        Value::Object(map) => {
            for (key, value) in entries(map) {
                query_field(key, value, &mut fields);
            }
        }
        Value::Array(items) => {
            for (i, value) in items.iter().enumerate() {
                query_field(&i.to_string(), value, &mut fields);
            }
        }
        _ => {}
    }
    fields.join("&")
}
fn spread(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(map)) => map.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v.clone()))
            .collect(),
        // ts-bug: signBinanceRequest spreads string bodies into indexed characters.
        Some(Value::String(s)) => s
            .encode_utf16()
            .enumerate()
            .map(|(i, c)| (i.to_string(), Value::String(String::from_utf16_lossy(&[c]))))
            .collect(),
        _ => Map::new(),
    }
}
fn numeric(value: f64) -> Value {
    // Query spelling remains JavaScript's even for a non-finite injected clock/window.
    let text = js_number_string(value);
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}
/// Sign the original structured config, without Axios body transformation.
/// The returned params/body omission is authoritative for subsequent transmission.
pub async fn sign_binance_request(
    config: &Map<String, Value>,
    options: &SigningOptions<'_>,
    exchange: ExchangeName,
    remote_signer_present: bool,
    recv_window: f64,
    clock: &VirtualClock,
) -> Result<Map<String, Value>, String> {
    let mut next = config.clone();
    if remote_signer_present || options.key.is_empty() || !can_sign_payload(options) {
        return Ok(next);
    }
    let input = config
        .get("data")
        .filter(|v| js_truthy(Some(v)))
        .or_else(|| config.get("params").filter(|v| js_truthy(Some(v))));
    let mut data = spread(input);
    data.shift_remove("signature");
    data.insert("timestamp".into(), numeric(clock.get_current_time_ms()));
    data.insert("recvWindow".into(), numeric(recv_window));
    let signed = sign_payload(
        options,
        exchange,
        &serialize_query(&Value::Object(data.clone())),
        Encoding::Hex,
    )
    .await?;
    data.insert("signature".into(), Value::String(signed.signature));
    next.insert("params".into(), Value::Object(data));
    if let Some(headers) = signed.headers {
        // Last: a replayed config still carries the previous signature's headers.
        let mut merged = spread(config.get("headers"));
        merged.extend(headers.into_iter().map(|(k, v)| (k, Value::String(v))));
        let mut normalized = Map::new();
        for (key, value) in entries(&merged) {
            let lower = safe_cex_num::js_trim(key).to_lowercase();
            if lower.is_empty() {
                continue;
            }
            let name = normalized
                .keys()
                .rev()
                .find(|k: &&String| k.to_lowercase() == lower)
                .cloned()
                .unwrap_or_else(|| key.clone());
            if normalized.get(&name) != Some(&Value::Bool(false)) {
                normalized.insert(name, header_value(value));
            }
        }
        normalized.retain(|_, v| !v.is_null() && v != &Value::Bool(false));
        // AxiosHeaders.toJSON(true), the string-valued transport header boundary.
        for value in normalized.values_mut() {
            if let Value::Array(items) = value {
                *value = Value::String(items.iter().map(text).collect::<Vec<_>>().join(", "));
            }
        }
        next.insert("headers".into(), Value::Object(normalized));
    }
    next.shift_remove("data");
    Ok(next)
}

/// `signBinanceRequest` over the canonical native graph. The assembled COIN-M
/// client keeps shallow aliases and native numbers until the shared query codec
/// consumes them; this is the same source helper as the typed extraction above.
pub async fn sign_binance_request_native(
    config: &safe_cex_exchange::js_runtime::value::Js,
    options: &safe_cex_exchange::js_runtime::value::Js,
    exchange: ExchangeName,
    recv_window: safe_cex_exchange::js_runtime::value::Js,
    clock: &VirtualClock,
    payload_signer: Option<&dyn safe_cex_crypto::PayloadSigner>,
) -> Result<safe_cex_exchange::js_runtime::value::Js, safe_cex_exchange::js_runtime::value::Thrown>
{
    use safe_cex_exchange::js_runtime::{
        client::axios_headers_from,
        library::qs_stringify,
        value::{Js, Thrown, utf16},
    };
    let next = config.spread_object()?;
    let key = options.get("key")?;
    let secret = options.get("secret")?;
    if options.get("remoteSigner")?.truthy()
        || !key.truthy()
        || (!secret.truthy() && payload_signer.is_none())
    {
        return Ok(next);
    }
    let body = config.get("data")?;
    let input = if body.truthy() {
        body
    } else {
        config.get("params")?
    };
    let data = input.spread_object()?;
    data.delete(&utf16("signature"))?;
    data.set("timestamp", Js::Number(clock.get_current_time_ms()))?;
    data.set("recvWindow", recv_window)?;
    let query = qs_stringify(&data, &Js::object(vec![("arrayFormat", Js::str("repeat"))]))?;
    let key = String::from_utf16(&key.to_js_string()?)
        .map_err(|_| Thrown::type_error("lone UTF-16 surrogate at signing key boundary"))?;
    let secret = if secret.truthy() {
        String::from_utf16(&secret.to_js_string()?)
            .map_err(|_| Thrown::type_error("lone UTF-16 surrogate at signing secret boundary"))?
    } else {
        String::new()
    };
    let signed = sign_payload(
        &SigningOptions {
            key: &key,
            secret: &secret,
            payload_signer,
        },
        exchange,
        &query,
        Encoding::Hex,
    )
    .await
    .map_err(|error| Thrown::error("Error", &error))?;
    data.set("signature", Js::str(&signed.signature))?;
    next.set("params", data)?;
    if let Some(headers) = signed.headers {
        let merged = config.get("headers")?.spread_object()?;
        for (key, value) in headers {
            merged.set(&key, Js::str(&value))?;
        }
        next.set("headers", axios_headers_from(&merged)?)?;
    }
    next.delete(&utf16("data"))?;
    Ok(next)
}
