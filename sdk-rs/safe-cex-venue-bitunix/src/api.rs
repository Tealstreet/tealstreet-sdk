//! Client construction and authentication over host transport/header projections.
use safe_cex_num::{
    JsJsonValue, js_json_stringify, js_number_string, js_own_keys, js_string, js_truthy,
};
use serde_json::Value;

type Query<'a> = dyn FnMut(&Value, &'static str, &'static str) -> Result<String, String> + 'a;
type Clock<'a> = dyn FnMut() -> Result<f64, String> + 'a;
type Nonce<'a> = dyn FnMut() -> Result<String, String> + 'a;
type Header<'a> = dyn FnMut(&str) -> Result<Option<Value>, String> + 'a;
type Report<'a> = dyn FnMut(String, &Value) -> Result<(), String> + 'a;
type Request = fn(
    &Value,
    &Value,
    &mut Query<'_>,
    &mut Clock<'_>,
    &mut Nonce<'_>,
) -> Result<Option<JsJsonValue>, String>;
type Resend<'a> = dyn FnMut(&Value) -> Result<Option<Value>, String> + 'a;
type Seen<'a> = dyn FnMut(&Value) -> bool + 'a;
type Response = for<'a> fn(
    &'a Value,
    &mut Header<'_>,
    &mut Report<'_>,
    &mut Resend<'_>,
    &mut Seen<'_>,
) -> Result<Received<'a>, String>;
type Condition<'a> = dyn FnMut() -> Result<bool, String> + 'a;
/// ts: the inline retryCondition. Both arms are host-bound and evaluated per error.
pub type RetryCondition = fn(&mut Condition<'_>, &mut Condition<'_>) -> Result<bool, String>;

/// The response interceptor's result: the resend's promise, or the response itself.
pub enum Received<'a> {
    Resent(Value),
    Response(&'a Value),
}

// ts: isBitunixSignatureRejection
pub fn is_bitunix_signature_rejection(data: &Value) -> Result<bool, safe_cex_num::JsCoercionError> {
    let field = |key| data.as_object().and_then(|object| object.get(key));
    if safe_cex_num::js_number(field("code"))? == 10007.0 {
        return Ok(true);
    }
    let message = match field("msg").filter(|value| !value.is_null()) {
        Some(value) => js_string(Some(value))?,
        None => String::new(),
    };
    // A non-unicode /i regex never folds a non-ASCII unit onto an ASCII one.
    Ok(message
        .to_ascii_lowercase()
        .contains("sign signature error"))
}

/// networkErrorOnRead()(error) || signatureRejectedResponse(isBitunixSignatureRejection)(error)
pub fn retry_condition(
    network_error_on_read: &mut Condition<'_>,
    signature_rejected: &mut Condition<'_>,
) -> Result<bool, String> {
    Ok(network_error_on_read()? || signature_rejected()?)
}

pub fn create_api<C, E>(
    testnet: bool,
    public_data_only: bool,
    create: impl FnOnce(JsJsonValue) -> Result<C, E>,
    retry: impl FnOnce(&C, RetryCondition, f64) -> Result<(), E>,
    transport: impl FnOnce(&C) -> Result<(), E>,
    request: impl FnOnce(&C, Request) -> Result<(), E>,
    response: impl FnOnce(&C, Response) -> Result<(), E>,
) -> Result<C, E> {
    let client = create(JsJsonValue::Object(vec![
        (
            "baseURL".into(),
            JsJsonValue::String(
                crate::types::BASE_URL
                    .iter()
                    .find(|(k, _)| *k == "livenet")
                    .unwrap()
                    .1
                    .into(),
            ),
        ),
        ("timeout".into(), JsJsonValue::Number(20_000.0)),
        (
            "headers".into(),
            JsJsonValue::Object(if testnet {
                vec![("x-simulated-trading".into(), JsJsonValue::Number(1.0))]
            } else {
                vec![]
            }),
        ),
    ]))?;
    retry(&client, retry_condition, 3.0)?;
    if !public_data_only {
        transport(&client)?;
    }
    request(&client, sign_request)?;
    response(&client, receive_response)?;
    Ok(client)
}

pub fn sign_request(
    config: &Value,
    options: &Value,
    query: &mut Query<'_>,
    clock: &mut Clock<'_>,
    nonce: &mut Nonce<'_>,
) -> Result<Option<JsJsonValue>, String> {
    if let Some(url) = config.get("url").filter(|v| !v.is_null()) {
        let url = url
            .as_str()
            .ok_or_else(|| "config.url.includes is not a function".to_owned())?;
        if url.contains("public") {
            return Ok(None);
        }
    }
    if config.is_null() {
        return Err("Cannot read properties of null (reading 'params')".into());
    }
    let params = if js_truthy(config.get("params")) {
        Some(query(&config["params"], "repeat", "localeCompare")?)
    } else {
        None
    };
    // A replayed config carries the body Axios already serialized: sign those bytes.
    let data = if let Some(Value::String(text)) = config.get("data") {
        Some(text.clone())
    } else if js_truthy(config.get("data")) {
        js_json_stringify(&JsJsonValue::from(config["data"].clone()))
            .map_err(|e| e.message.to_owned())?
    } else {
        None
    };
    let timestamp = clock()?;
    let nonce = nonce()?;
    let key = options.get("key").filter(|v| !v.is_null()).map_or_else(
        || Ok(String::new()),
        |v| js_string(Some(v)).map_err(|e| e.message.to_owned()),
    )?;
    let params = params.unwrap_or_default().replace(['=', '&'], "");
    let mut to_sign = format!("{nonce}{}{key}{params}", js_number_string(timestamp));
    if let Some(data) = data.filter(|s| !s.is_empty()) {
        to_sign.push_str(&data);
    }
    let digest = crate::utils::sha256_hex(&to_sign);
    let secret = js_string(options.get("secret")).map_err(|e| e.message.to_owned())?;
    let signature = crate::utils::sha256_hex(&format!("{digest}{secret}"));
    let mut headers = match config.get("headers") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Object(headers)) => js_own_keys(headers.keys().map(String::as_str))
            .into_iter()
            .map(|k| (k.to_owned(), JsJsonValue::from(headers[k].clone())))
            .collect(),
        _ => {
            return Err("Request headers must be an object at the typed transport boundary".into());
        }
    };
    for (key, value) in [
        (
            "api-key",
            options
                .get("key")
                .cloned()
                .map_or(JsJsonValue::Undefined, JsJsonValue::from),
        ),
        ("sign", JsJsonValue::String(signature)),
        ("nonce", JsJsonValue::String(nonce)),
        ("timestamp", JsJsonValue::Number(timestamp)),
    ] {
        if let Some((_, v)) = headers.iter_mut().find(|(k, _)| k == key) {
            *v = value;
        } else {
            headers.push((key.into(), value));
        }
    }
    Ok(Some(JsJsonValue::Object(headers)))
}

fn header_matches(value: Option<Value>, expected: &str) -> Result<bool, String> {
    let text = value.as_ref().filter(|v| !v.is_null()).map_or_else(
        || Ok(String::new()),
        |v| js_string(Some(v)).map_err(|e| e.message.to_owned()),
    )?;
    Ok(text.to_lowercase() == expected)
}
pub fn receive_response<'a>(
    response: &'a Value,
    header: &mut Header<'_>,
    report: &mut Report<'_>,
    resend: &mut Resend<'_>,
    seen: &mut Seen<'_>,
) -> Result<Received<'a>, String> {
    if let Some(resent) = resend(response)?.filter(|value| js_truthy(Some(value))) {
        return Ok(Received::Resent(resent));
    }
    // An axios-retry resend reaches here twice (its chain, then the original's): emit once.
    if seen(response) {
        return Ok(Received::Response(response));
    }
    let config = response.get("config");
    let suppress = config.and_then(|c| c.get("__suppressExchangeError"))
        == Some(&Value::Bool(true))
        || header_matches(header("x-tealstreet-suppress-exchange-error")?, "1")?
        || header_matches(header("x-tealstreet-suppress-exchange-error")?, "true")?;
    let suppress_rate = config.and_then(|c| c.get("__suppressExchangeRateLimitError"))
        == Some(&Value::Bool(true))
        || header_matches(
            header("x-tealstreet-suppress-exchange-rate-limit-error")?,
            "1",
        )?
        || header_matches(
            header("x-tealstreet-suppress-exchange-rate-limit-error")?,
            "true",
        )?;
    if suppress {
        return Ok(Received::Response(response));
    }
    let data = response.get("data");
    if js_truthy(data) && !js_truthy(data.and_then(|v| v.get("data"))) {
        let message = data.and_then(|v| v.get("msg"));
        if js_truthy(message) {
            let message = js_string(message).map_err(|e| e.message.to_owned())?;
            let rate = message.to_lowercase().contains("request too frequently");
            if !(suppress_rate && rate) {
                report(message, response)?;
            }
        }
    }
    Ok(Received::Response(response))
}
