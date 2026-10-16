//! WOO signing views and typed transport-context projection.
use safe_cex_crypto::{Encoding, SigningOptions, sign_payload};
use safe_cex_transport::{RestRequest, RestTransportContext, TransportProvider};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Map, Value};

/// Keeps the signed preimage distinct from the configuration handed to Axios.
pub struct WooSignedRequest {
    pub config: Value,
    pub preimage: String,
}
fn truthy(value: &Value) -> bool {
    safe_cex_num::js_truthy(Some(value))
}
fn keys(map: &Map<String, Value>) -> Vec<&str> {
    safe_cex_num::js_own_keys(map.keys().map(String::as_str))
}
fn json_text(value: &Value) -> Result<String, String> {
    safe_cex_num::js_json_stringify(&safe_cex_num::JsJsonValue::from(value.clone()))
        .map_err(|error| error.message.to_owned())?
        .ok_or_else(|| "JSON value unexpectedly serialized as undefined".to_owned())
}
fn encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            output.push(char::from(byte));
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}
fn query(value: &Value) -> Result<String, String> {
    fn visit(value: &Value, prefix: &str, pairs: &mut Vec<String>) -> Result<(), String> {
        match value {
            Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    visit(value, &format!("{prefix}[{index}]"), pairs)?;
                }
            }
            Value::Object(values) => {
                for key in keys(values) {
                    visit(&values[key], &format!("{prefix}[{key}]"), pairs)?;
                }
            }
            Value::Null => pairs.push(format!("{}=", encode(prefix))),
            other => pairs.push(format!(
                "{}={}",
                encode(prefix),
                encode(&safe_cex_num::js_string(Some(other)).map_err(|e| e.message.to_owned())?)
            )),
        }
        Ok(())
    }
    let mut pairs = Vec::new();
    match value {
        Value::Object(values) => {
            for key in keys(values) {
                visit(&values[key], key, &mut pairs)?;
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                visit(value, &index.to_string(), &mut pairs)?;
            }
        }
        _ => {}
    }
    Ok(pairs.join("&"))
}
fn request_method(config: &Map<String, Value>, expression: &str) -> Result<String, String> {
    match config.get("method") {
        Some(Value::String(value)) if !value.is_empty() => Ok(value.to_uppercase()),
        Some(Value::Object(value)) if value.get("toUpperCase").is_some_and(|v| !v.is_null()) => {
            Err(format!("{expression} is not a function"))
        }
        _ => Ok("GET".into()),
    }
}
fn absolute_url(config: &Map<String, Value>) -> Result<Option<&str>, String> {
    match config.get("url") {
        Some(Value::String(url)) => Ok(url.starts_with("http").then_some(url.as_str())),
        None | Some(Value::Null) => Ok(None),
        _ => Err("nextConfig?.url?.startsWith is not a function".into()),
    }
}
fn proxy_request(config: &Map<String, Value>, url: &str) -> RestRequest {
    let timeout_ms = config
        .get("timeout")
        .and_then(safe_cex_num::numeric_ingress::js_value_as_f64)
        .filter(|v| {
            v.is_finite()
                && v.fract() == 0.
                && *v >= -9_223_372_036_854_775_808.
                && *v < 9_223_372_036_854_775_808.
        })
        .map(|v| v as i64);
    RestRequest {
        url: url.into(),
        method: config
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .into(),
        params: config.get("params").cloned(),
        body: config
            .get("data")
            .cloned()
            .map(safe_cex_transport::RestBody::Untransformed),
        base_url: config
            .get("baseURL")
            .and_then(Value::as_str)
            .map(str::to_owned),
        timeout_ms,
        headers: config
            .get("headers")
            .and_then(Value::as_object)
            .map(|headers| {
                headers
                    .iter()
                    .filter_map(|(name, value)| value.as_str().map(|v| (name.clone(), v.into())))
                    .collect()
            })
            .unwrap_or_default(),
        ..Default::default()
    }
}

// ts: signV3
pub async fn sign_v3(
    config: &Value,
    options: &ExchangeOptions,
    provider: &dyn TransportProvider,
    timestamp: f64,
) -> Result<WooSignedRequest, String> {
    let mut next = config
        .as_object()
        .cloned()
        .ok_or("WOO signing config must be an object")?;
    let method = request_method(&next, "c.method?.toUpperCase")?;
    next.insert("method".into(), Value::String(method.clone()));
    if let Some(url) = absolute_url(&next)? {
        let request = proxy_request(&next, url);
        let prefix = provider.proxy_prefix(RestTransportContext {
            exchange: &ExchangeName::Woo,
            options,
            config: &request,
        });
        if !prefix.is_empty() {
            next.insert("url".into(), Value::String(format!("{prefix}/{url}")));
        }
    }
    if let Some(params) = next.get("params").filter(|params| truthy(params)) {
        let url = next
            .get("url")
            .map(|v| safe_cex_num::js_string(Some(v)))
            .transpose()
            .map_err(|e| e.message.to_owned())?
            .unwrap_or_else(|| "undefined".into());
        let url = format!("{url}?{}", query(params)?);
        next.insert("url".into(), Value::String(url));
        next.remove("params");
    }
    let url = match next.get("url") {
        None | Some(Value::Null) => String::new(),
        value => safe_cex_num::js_string(value).map_err(|e| e.message.to_owned())?,
    };
    let body = next
        .get("data")
        .filter(|data| truthy(data))
        .map(json_text)
        .transpose()?
        .unwrap_or_default();
    let preimage = format!(
        "{}{method}{url}{body}",
        safe_cex_num::js_number_string(timestamp)
    );
    let signature = sign_payload(
        &SigningOptions {
            key: &options.key,
            secret: &options.secret,
            payload_signer: None,
        },
        ExchangeName::Woo,
        &preimage,
        Encoding::Hex,
    )
    .await?
    .signature;
    let mut headers = next
        .get("headers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for name in ["x-api-timestamp", "x-api-signature"] {
        headers.retain(|key, _| !key.eq_ignore_ascii_case(name));
    }
    headers.insert(
        "x-api-timestamp".into(),
        Value::String(safe_cex_num::js_number_string(timestamp)),
    );
    headers.insert("x-api-signature".into(), Value::String(signature));
    next.insert("headers".into(), Value::Object(headers));
    Ok(WooSignedRequest {
        config: Value::Object(next),
        preimage,
    })
}

// ts: signV1
pub(crate) fn sorted_query(data: &Value, repeat: bool) -> Result<String, String> {
    let collator = safe_cex_num::JsRootCollator::try_new().map_err(|e| e.message.to_owned())?;
    fn visit(
        value: &Value,
        prefix: &str,
        pairs: &mut Vec<String>,
        collator: &safe_cex_num::JsRootCollator,
        repeat: bool,
    ) -> Result<(), String> {
        match value {
            Value::Array(values) => {
                let mut indices: Vec<_> = (0..values.len()).collect();
                indices.sort_by(|a, b| collator.compare(&a.to_string(), &b.to_string()));
                for index in indices {
                    let child = if repeat {
                        prefix.to_owned()
                    } else {
                        format!("{prefix}[{index}]")
                    };
                    visit(&values[index], &child, pairs, collator, repeat)?;
                }
            }
            Value::Object(values) => {
                let mut names = keys(values);
                names.sort_by(|a, b| collator.compare(a, b));
                for name in names {
                    visit(
                        &values[name],
                        &format!("{prefix}[{name}]"),
                        pairs,
                        collator,
                        repeat,
                    )?;
                }
            }
            Value::Null => pairs.push(format!("{}=", encode(prefix))),
            value => pairs.push(format!(
                "{}={}",
                encode(prefix),
                encode(&safe_cex_num::js_string(Some(value)).map_err(|e| e.message.to_owned())?)
            )),
        }
        Ok(())
    }
    let mut pairs = Vec::new();
    match data {
        Value::Object(values) => {
            let mut names = keys(values);
            names.sort_by(|a, b| collator.compare(a, b));
            for name in names {
                visit(&values[name], name, &mut pairs, &collator, repeat)?;
            }
        }
        Value::Array(values) => {
            let mut indices: Vec<_> = (0..values.len()).collect();
            indices.sort_by(|a, b| collator.compare(&a.to_string(), &b.to_string()));
            for index in indices {
                visit(
                    &values[index],
                    &index.to_string(),
                    &mut pairs,
                    &collator,
                    repeat,
                )?;
            }
        }
        _ => {}
    }
    Ok(pairs.join("&"))
}

pub async fn sign_v1(
    config: &Value,
    options: &ExchangeOptions,
    provider: &dyn TransportProvider,
    timestamp: f64,
) -> Result<WooSignedRequest, String> {
    let mut next = config
        .as_object()
        .cloned()
        .ok_or("WOO signing config must be an object")?;
    let method = request_method(&next, "nextConfig.method?.toUpperCase")?;
    let mutation = matches!(method.as_str(), "DELETE" | "POST" | "PUT");
    if let Some(url) = absolute_url(&next)? {
        let request = proxy_request(&next, url);
        let prefix = provider.proxy_prefix(RestTransportContext {
            exchange: &ExchangeName::Woo,
            options,
            config: &request,
        });
        if !prefix.is_empty() {
            next.insert("url".into(), Value::String(format!("{prefix}/{url}")));
        }
    }
    let data = next
        .get("data")
        .filter(|v| truthy(v))
        .or_else(|| next.get("params").filter(|v| truthy(v)));
    let query = sorted_query(data.unwrap_or(&Value::Null), true)?;
    if !query.is_empty() && !mutation {
        let url = safe_cex_num::js_string(next.get("url")).map_err(|e| e.message.to_owned())?;
        next.insert("url".into(), Value::String(format!("{url}?{query}")));
        next.remove("params");
    }
    let preimage = format!("{query}|{}", safe_cex_num::js_number_string(timestamp));
    let signature = sign_payload(
        &SigningOptions {
            key: &options.key,
            secret: &options.secret,
            payload_signer: None,
        },
        ExchangeName::Woo,
        &preimage,
        Encoding::Hex,
    )
    .await?
    .signature;
    let mut headers = next
        .get("headers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for name in ["x-api-timestamp", "x-api-signature"] {
        headers.retain(|key, _| !key.eq_ignore_ascii_case(name));
    }
    headers.insert(
        "x-api-timestamp".into(),
        Value::String(safe_cex_num::js_number_string(timestamp)),
    );
    headers.insert("x-api-signature".into(), Value::String(signature));
    if mutation {
        next.insert("data".into(), Value::String(query));
        next.remove("params");
        if !headers.iter().any(|(key, value)| {
            key.eq_ignore_ascii_case("content-type") && value == &Value::Bool(false)
        }) {
            let key = headers
                .keys()
                .find(|key| key.eq_ignore_ascii_case("content-type"))
                .cloned()
                .unwrap_or_else(|| "Content-Type".into());
            headers.insert(
                key,
                Value::String("application/x-www-form-urlencoded".into()),
            );
        }
    }
    next.insert("headers".into(), Value::Object(headers));
    Ok(WooSignedRequest {
        config: Value::Object(next),
        preimage,
    })
}
