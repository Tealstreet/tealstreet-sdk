//! Error telemetry behavior: packages/safe-cex/src/exchanges/base.ts.
use crate::{
    js_runtime::value::{Js, Thrown, from_json},
    runtime::ExchangeError,
};
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpFailure {
    pub status: f64,
    pub status_text: String,
}
#[derive(Clone, Debug)]
pub struct HttpFailureRecord {
    pub status: f64,
    pub status_text: String,
    pub url: Option<String>,
    pub at: f64,
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpFailureContext {
    pub recent_http_status: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_http_status_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_http_url: Option<String>,
    pub recent_http_age_ms: f64,
}
pub const HTTP_FAILURE_CORRELATION_WINDOW_MS: f64 = 1000.;
fn read_status_text(shape: &Map<String, Value>) -> String {
    ["statusText", "status_text"]
        .into_iter()
        .find_map(|key| {
            shape
                .get(key)
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
        })
        .unwrap_or("")
        .into()
}
pub fn get_http_failure(response: &Value) -> Option<HttpFailure> {
    let shape = response.as_object()?;
    let status = shape.get("status")?.as_f64()?;
    if status < 400. {
        return None;
    }
    Some(HttpFailure {
        status,
        status_text: read_status_text(shape),
    })
}
pub fn record_http_failure(
    response: &Value,
    now: f64,
    url: Option<&str>,
) -> Option<HttpFailureRecord> {
    let failure = get_http_failure(response)?;
    Some(HttpFailureRecord {
        status: failure.status,
        status_text: failure.status_text,
        url: url.map(str::to_owned),
        at: now,
    })
}
fn tag_safe_url(url: Option<&str>) -> Option<String> {
    let path = url?.split('?').next()?;
    if path.is_empty() {
        None
    } else {
        Some(utf16_prefix(path, 80))
    }
}
pub fn read_http_failure_context(
    record: Option<&HttpFailureRecord>,
    now: f64,
) -> Option<HttpFailureContext> {
    let record = record?;
    let age = now - record.at;
    if age.partial_cmp(&0.) == Some(std::cmp::Ordering::Less)
        || age.partial_cmp(&HTTP_FAILURE_CORRELATION_WINDOW_MS) == Some(std::cmp::Ordering::Greater)
    {
        return None;
    }
    Some(HttpFailureContext {
        recent_http_status: record.status,
        recent_http_status_text: (!record.status_text.is_empty())
            .then(|| record.status_text.clone()),
        recent_http_url: tag_safe_url(record.url.as_deref()),
        recent_http_age_ms: (age / 100. + 0.5).floor() * 100.,
    })
}
fn utf16_prefix(value: &str, limit: usize) -> String {
    String::from_utf16_lossy(&value.encode_utf16().take(limit).collect::<Vec<_>>())
}
fn js_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Object(_) => "[object Object]".into(),
        Value::Array(items) => items
            .iter()
            .map(|v| {
                if v.is_null() {
                    String::new()
                } else {
                    js_string(v)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        _ => value.to_string(),
    }
}
pub fn append_http_failure(message: Option<&Value>, response: &Value) -> String {
    let message = message
        .filter(|m| !m.is_null())
        .map(js_string)
        .unwrap_or_default();
    let Some(failure) = get_http_failure(response) else {
        return message;
    };
    let status = format!("HTTP {}", failure.status);
    if message.contains(&status) {
        message
    } else {
        format!("{message} ({status})")
    }
}
fn describe_body(value: Option<&Value>) -> String {
    value
        .map(Value::to_string)
        .unwrap_or_else(|| "undefined".into())
}
pub fn describe_http_failure(label: &str, response: &Value) -> String {
    let status = get_http_failure(response)
        .map(|failure| failure.status.to_string())
        .unwrap_or_else(|| "unknown".into());
    let body = describe_body(response.get("data"));
    let detail = if !body.is_empty() && body != "null" && body != "undefined" {
        format!(": {}", utf16_prefix(&body, 300))
    } else {
        String::new()
    };
    format!("{label} API error (HTTP {status}){detail}")
}
pub enum ExchangeErrorInput {
    Message(String),
    Error(Box<ExchangeError>),
    /// A selected response value, before the source reporter inspects/coerces it.
    Raw(Value),
    Missing,
}
pub(crate) fn truthy(value: &Value) -> bool {
    safe_cex_num::js_truthy(Some(value))
}
pub(crate) fn assign(target: &Js, source: &Js) -> Result<(), ExchangeError> {
    for (key, value) in source.own_entries()? {
        target.define_key(&key, value)?;
    }
    Ok(())
}
pub(crate) fn insert_option(map: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    if let Some(value) = value {
        map.insert(key.into(), value.clone());
    }
}
fn sentry_context(error: &Js) -> Result<Js, Thrown> {
    let context = error.get("context")?;
    let property = |key: &str| {
        if context.is_nullish() {
            Ok(Js::Undefined)
        } else {
            context.get(key)
        }
    };
    let original = error.get("originalError")?;
    let original_message = if original.is_nullish() {
        Js::Undefined
    } else {
        original.get("message")?
    };
    let context_code = property("errorCode")?;
    let code = if context_code.truthy() {
        context_code
    } else {
        error.get("code")?
    };
    let config = error.get("config")?;
    let config = if config.truthy() {
        let headers = config.get("headers")?;
        let headers = if headers.truthy() {
            Js::array(
                headers
                    .own_keys()
                    .iter()
                    .map(|key| Js::utf16(key))
                    .collect(),
            )
        } else {
            Js::Undefined
        };
        Js::object(vec![
            ("baseURL", config.get("baseURL")?),
            ("timeout", config.get("timeout")?),
            ("headers", headers),
        ])
    } else {
        Js::Undefined
    };
    Ok(Js::object(vec![
        (
            "errorType",
            Js::str(if property("isNetworkError")?.truthy() {
                "network"
            } else {
                "api"
            }),
        ),
        ("errorCode", code),
        ("statusCode", property("statusCode")?),
        ("isTimeout", property("isTimeout")?),
        ("isCORS", property("isCORS")?),
        ("url", property("url")?),
        ("method", property("method")?),
        ("statusText", property("statusText")?),
        ("enhancedMessage", error.get("enhancedMessage")?),
        ("originalMessage", original_message),
        ("timestamp", property("timestamp")?),
        ("config", config),
    ]))
}
pub(crate) fn formatted_context(error: &ExchangeError) -> Js {
    let source = Js::object(vec![
        (
            "context",
            error
                .context
                .as_ref()
                .map(from_json)
                .unwrap_or(Js::Undefined),
        ),
        ("code", error.code.clone().unwrap_or(Js::Undefined)),
        (
            "config",
            error
                .config
                .as_ref()
                .map(from_json)
                .unwrap_or(Js::Undefined),
        ),
        (
            "originalError",
            error
                .original_error
                .as_ref()
                .map(from_json)
                .unwrap_or(Js::Undefined),
        ),
        (
            "enhancedMessage",
            error
                .enhanced_message
                .as_deref()
                .map(Js::str)
                .unwrap_or(Js::Undefined),
        ),
    ]);
    sentry_context(&source).expect("typed error metadata has ordinary data properties")
}
pub(crate) fn raw_formatted_context(error: &Value) -> Result<Js, ExchangeError> {
    sentry_context(&from_json(error)).map_err(Into::into)
}

pub fn format_error_for_sentry(error: &ExchangeError) -> Js {
    let flattened = formatted_context(error);
    let tags = Js::object(vec![]);
    let extra = Js::object(vec![]);
    for (key, value) in flattened.own_entries().expect("fresh formatter fields") {
        let target = if matches!(
            String::from_utf16_lossy(&key).as_str(),
            "errorType" | "errorCode" | "statusCode" | "isTimeout" | "isCORS"
        ) {
            &tags
        } else {
            &extra
        };
        target
            .define_key(&key, value)
            .expect("fresh formatter property");
    }
    Js::object(vec![("tags", tags), ("extra", extra)])
}

/// Host identity facts retain Error and XMLHttpRequest instanceof checks across runtimes.
/// Timestamp is supplied by the host as Date.toISOString(), rather than virtual trading time.
pub fn enhance_axios_error(
    source: &Value,
    input_is_error: bool,
    exchange_name: Option<&str>,
    timestamp: &str,
    request_is_browser_xhr: bool,
) -> Result<ExchangeError, ExchangeError> {
    if source.is_null() {
        return Err(ExchangeError::new(
            "Cannot read properties of null (reading 'config')",
        ));
    }
    let get = |key: &str| source.get(key);
    let text = |key: &str| {
        get(key)
            .map(js_string)
            .unwrap_or_else(|| "undefined".into())
    };
    let mut error = ExchangeError::new(if input_is_error {
        text("message")
    } else {
        js_string(source)
    });
    let mut context = Map::new();
    context.insert("timestamp".into(), json!(timestamp));
    if let Some(config) = get("config").filter(|v| truthy(v)) {
        insert_option(&mut context, "url", config.get("url"));
        if let Some(method) = config.get("method").filter(|v| !v.is_null()) {
            let Some(method) = method.as_str() else {
                return Err(ExchangeError::new(
                    "error.config.method?.toUpperCase is not a function",
                ));
            };
            context.insert("method".into(), json!(method.to_uppercase()));
        }
    }
    let url = context.get("url").filter(|v| truthy(v)).map(js_string);
    let suffix = |prefix: &str| {
        url.as_ref()
            .map(|url| format!("{prefix}{url}"))
            .unwrap_or_default()
    };
    let response = get("response").filter(|v| truthy(v));
    let message = if response.is_none() && get("request").is_some_and(truthy) {
        context.insert("isNetworkError".into(), json!(true));
        match text("code").as_str() {
            "ECONNABORTED" => {
                context.insert("isTimeout".into(), json!(true));
                format!("Request timeout{}", suffix(" to "))
            }
            "ERR_NETWORK" => {
                if request_is_browser_xhr {
                    context.insert("isCORS".into(), json!(true));
                    format!(
                        "Network error (possibly CORS){}",
                        suffix(" when accessing ")
                    )
                } else {
                    format!("Network connection failed{}", suffix(" to "))
                }
            }
            _ if text("message") == "Network Error" => {
                if request_is_browser_xhr {
                    context.insert("isCORS".into(), json!(true));
                    format!(
                        "Network error (possibly CORS){}",
                        suffix(" when accessing ")
                    )
                } else {
                    format!("Network connection failed{}", suffix(" to "))
                }
            }
            _ if get("code").is_some_and(truthy) => {
                context.insert("errorCode".into(), get("code").unwrap().clone());
                format!("Network error ({}){}", text("code"), suffix(" to "))
            }
            _ => format!("Network request failed{}", suffix(" to ")),
        }
    } else if let Some(response) = response {
        insert_option(&mut context, "statusCode", response.get("status"));
        insert_option(&mut context, "statusText", response.get("statusText"));
        let server = ["msg", "message", "error", "retMsg", "code"]
            .into_iter()
            .find_map(|key| {
                response
                    .get("data")
                    .and_then(|v| v.get(key))
                    .filter(|v| truthy(v))
            });
        format!(
            "{} {} failed with {} {}{}",
            context
                .get("method")
                .filter(|v| truthy(v))
                .map(js_string)
                .unwrap_or_else(|| "Request".into()),
            url.as_deref().unwrap_or("to API"),
            response
                .get("status")
                .map(js_string)
                .unwrap_or_else(|| "undefined".into()),
            response
                .get("statusText")
                .map(js_string)
                .unwrap_or_else(|| "undefined".into()),
            server
                .map(|v| format!(": {}", js_string(v)))
                .unwrap_or_default()
        )
    } else {
        format!(
            "Request setup error{}: {}",
            suffix(" for "),
            text("message")
        )
    };
    let message = exchange_name
        .filter(|name| !name.is_empty())
        .map(|name| format!("[{name}] {message}"))
        .unwrap_or(message);
    if input_is_error {
        if let Some(name) = get("name").and_then(Value::as_str) {
            error.name = name.into();
        }
        error.stack = get("stack").and_then(Value::as_str).map(str::to_owned);
    }
    error.message = message.clone();
    error.enhanced_message = Some(message.clone());
    error.context = Some(Value::Object(context));
    error.original_error = Some(if input_is_error {
        let mut original = source.as_object().cloned().unwrap_or_default();
        original.insert("message".into(), json!(message));
        Value::Object(original)
    } else {
        source.clone()
    });
    error.is_axios_error = true;
    error.request = get("request").cloned();
    error.config = get("config").cloned();
    error.code = get("code").map(from_json);
    error.response = get("response").cloned();
    Ok(error)
}

pub fn is_network_error(error: &Value) -> bool {
    if !error.get("isAxiosError").is_some_and(truthy) || error.get("response").is_some_and(truthy) {
        return false;
    }
    let Some(code) = error.get("code").filter(|v| truthy(v)) else {
        return false;
    };
    code != "ECONNABORTED"
        && (matches!(
            code.as_str(),
            Some("ERR_NETWORK" | "ETIMEDOUT" | "ENOTFOUND" | "ECONNREFUSED" | "ECONNRESET")
        ) || error.get("message").is_some_and(|v| v == "Network Error"))
}

#[cfg(test)]
#[path = "../tests/shared_truthiness/mod.rs"]
mod shared_truthiness;
