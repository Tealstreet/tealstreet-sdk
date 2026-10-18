//! Bybit market-startup diagnostics; active Hyro recovery owns its rejection state.
use crate::exchange_helpers::{number_or_undefined, string_or_undefined};
use safe_cex_num::{TableHit, js_number, js_number_string, js_string, js_table_lookup, js_truthy};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::fmt;

pub const BYBIT_MARKETS_FAILURE_SENTRY_COOLDOWN_MS: u64 = 5 * 60_000;

fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
fn object<'de, D: Deserializer<'de>>(d: D) -> Result<Map<String, Value>, D::Error> {
    let value = safe_cex_types::retained_json::deserialize_retained_json(d)?;
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(serde::de::Error::custom(
            "Bybit response requires an object record",
        )),
    }
}
fn present_object<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Map<String, Value>>, D::Error> {
    object(d).map(Some)
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitCreateOrderResponse {
    #[serde(deserialize_with = "safe_cex_types::retained_json::deserialize_retained_json")]
    pub data: Value,
    #[serde(deserialize_with = "object")]
    pub payload: Map<String, Value>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub terminal: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub recorded: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present_object",
        skip_serializing_if = "Option::is_none"
    )]
    pub recovered_order: Option<Map<String, Value>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub reconciled: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub authoritative_seen: Option<bool>,
}

/// Own non-nullish field precedence; executable inherited results are refused.
pub fn get_bybit_error_field(
    payload: Option<&Value>,
    keys: &[&str],
) -> Result<Option<Value>, &'static str> {
    let Some(payload) = payload.filter(|v| matches!(v, Value::Object(_) | Value::Array(_))) else {
        return Ok(None);
    };
    for key in keys {
        if let Some(value) = safe_cex_num::js_object_or_array_entry(payload, key) {
            if !value.is_null() {
                return Ok(Some(value.clone()));
            }
            // Own null shadows any inherited member of the same name.
            continue;
        }
        if *key == "length"
            && let Value::Array(values) = payload
        {
            return Ok(Some(Value::from(values.len())));
        }
        if matches!(
            js_table_lookup::<&str, Value>(&[], key),
            TableHit::Inherited(_)
        ) || (payload.is_array()
            && matches!(
                *key,
                "at" | "concat"
                    | "copyWithin"
                    | "fill"
                    | "find"
                    | "findIndex"
                    | "findLast"
                    | "findLastIndex"
                    | "lastIndexOf"
                    | "pop"
                    | "push"
                    | "reverse"
                    | "shift"
                    | "unshift"
                    | "slice"
                    | "sort"
                    | "splice"
                    | "includes"
                    | "indexOf"
                    | "join"
                    | "keys"
                    | "entries"
                    | "values"
                    | "forEach"
                    | "filter"
                    | "flat"
                    | "flatMap"
                    | "map"
                    | "every"
                    | "some"
                    | "reduce"
                    | "reduceRight"
                    | "toReversed"
                    | "toSorted"
                    | "toSpliced"
                    | "with"
            ))
        {
            return Err("Bybit retained JSON cannot carry an inherited error field");
        }
    }
    Ok(None)
}
// All telemetry property names are non-prototype names. Optional chaining also
// boxes scalars, whose named properties here are absent on stock prototypes.
fn prop<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    value.and_then(|v| v.get(key))
}
fn nullish_text(value: Option<&Value>) -> Result<String, &'static str> {
    js_string(
        value
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )
    .map_err(|e| e.message)
}
#[derive(Debug, Clone, PartialEq)]
pub struct BybitMarketsFailureTelemetry {
    pub reason: String,
    pub http_status: Option<Value>,
    pub error_code: Option<String>,
    pub ret_code: Option<f64>,
    pub endpoint: Option<String>,
    pub method: Option<String>,
}
pub fn get_bybit_markets_failure_telemetry(
    cause: Option<&Value>,
) -> Result<BybitMarketsFailureTelemetry, &'static str> {
    let response = prop(cause, "response");
    let data = prop(response, "data");
    let ret_msg = nullish_text(
        prop(data, "retMsg")
            .filter(|v| !v.is_null())
            .or_else(|| prop(data, "ret_msg")),
    )?
    .to_lowercase();
    let message = nullish_text(prop(cause, "message"))?.to_lowercase();
    let http_status = prop(response, "status").cloned();
    let error_code = string_or_undefined(prop(cause, "code")).map(str::to_owned);
    let ret_code = number_or_undefined(
        prop(data, "retCode")
            .filter(|v| !v.is_null())
            .or_else(|| prop(data, "ret_code")),
    );
    let config = prop(cause, "config");
    let endpoint = string_or_undefined(prop(config, "url")).map(str::to_owned);
    let method = string_or_undefined(prop(config, "method")).map(str::to_uppercase);
    let strict_status = http_status
        .as_ref()
        .and_then(safe_cex_num::numeric_ingress::js_value_as_f64);
    let reason = if ret_msg.contains("too many")
        || ret_msg.contains("rate limit")
        || strict_status == Some(429.)
    {
        "rate-limited".into()
    } else if ret_msg.contains("banned") || strict_status == Some(403.) {
        "blocked".into()
    } else if message.contains("timeout")
        || message.contains("timed out")
        || error_code.as_deref() == Some("ECONNABORTED")
    {
        "timeout".into()
    } else if let Some(status) = strict_status {
        format!("HTTP {}", js_number_string(status))
    } else if message.contains("network") {
        "network".into()
    } else {
        "unknown".into()
    };
    Ok(BybitMarketsFailureTelemetry {
        reason,
        http_status,
        error_code,
        ret_code,
        endpoint,
        method,
    })
}
pub fn describe_bybit_markets_failure(cause: Option<&Value>) -> Result<String, &'static str> {
    Ok(get_bybit_markets_failure_telemetry(cause)?.reason)
}
pub fn is_retryable_bybit_trading_markets_failure(
    cause: Option<&Value>,
) -> Result<bool, &'static str> {
    let t = get_bybit_markets_failure_telemetry(cause)?;
    if matches!(t.reason.as_str(), "timeout" | "network") {
        return Ok(true);
    }
    let Some(status) = t.http_status.as_ref() else {
        return Ok(false);
    };
    if safe_cex_num::numeric_ingress::js_value_as_f64(status) == Some(408.) {
        return Ok(true);
    }
    let numeric = js_number(Some(status)).map_err(|e| e.message)?;
    Ok((500. ..600.).contains(&numeric))
}
#[derive(Debug, Clone, PartialEq)]
pub struct BybitTradingMarketsRequestError {
    pub cause: Option<Value>,
    pub message: String,
    pub name: &'static str,
    pub safe_cex_sentry_report_key: String,
    pub safe_cex_sentry_report_cooldown_ms: u64,
    pub safe_cex_sentry_tags: Map<String, Value>,
    pub safe_cex_sentry_extra: Map<String, Value>,
}
impl BybitTradingMarketsRequestError {
    pub fn new(cause: Option<&Value>, retry_count: Option<f64>) -> Result<Self, &'static str> {
        let t = get_bybit_markets_failure_telemetry(cause)?;
        let retry_count = retry_count.unwrap_or(0.);
        let retry = serde_json::Number::from_f64(retry_count)
            .ok_or("Bybit JSON diagnostic metadata requires a finite retry count")?;
        let mut tags = Map::new();
        let mut extra = Map::new();
        tags.insert(
            "worker_error_family".into(),
            Value::String("bybit_markets_startup".into()),
        );
        tags.insert(
            "bybit_markets_failure_reason".into(),
            Value::String(t.reason.clone()),
        );
        tags.insert(
            "bybit_markets_retry_count".into(),
            Value::Number(retry.clone()),
        );
        extra.insert(
            "bybitMarketsFailureReason".into(),
            Value::String(t.reason.clone()),
        );
        extra.insert("bybitMarketsRetryCount".into(), Value::Number(retry));
        if let Some(status) = t.http_status {
            tags.insert("bybit_markets_http_status".into(), status.clone());
            extra.insert("bybitMarketsHttpStatus".into(), status);
        }
        if let Some(code) = t.error_code {
            tags.insert(
                "bybit_markets_error_code".into(),
                Value::String(code.clone()),
            );
            extra.insert("bybitMarketsErrorCode".into(), Value::String(code));
        }
        if let Some(code) = t.ret_code {
            let v =
                Value::Number(serde_json::Number::from_f64(code).expect("finite retCode guard"));
            tags.insert("bybit_markets_ret_code".into(), v.clone());
            extra.insert("bybitMarketsRetCode".into(), v);
        }
        if let Some(endpoint) = t.endpoint {
            extra.insert("bybitMarketsEndpoint".into(), Value::String(endpoint));
        }
        if let Some(method) = t.method {
            extra.insert("bybitMarketsMethod".into(), Value::String(method));
        }
        Ok(Self {
            cause: cause.cloned(),
            message: format!("Failed to fetch Bybit Trading markets ({})", t.reason),
            name: "BybitTradingMarketsRequestError",
            safe_cex_sentry_report_key: format!("bybit:trading-markets:{}", t.reason),
            safe_cex_sentry_report_cooldown_ms: BYBIT_MARKETS_FAILURE_SENTRY_COOLDOWN_MS,
            safe_cex_sentry_tags: tags,
            safe_cex_sentry_extra: extra,
        })
    }
}
impl fmt::Display for BybitTradingMarketsRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.message.is_empty() {
            f.write_str(self.name)
        } else {
            write!(f, "{}: {}", self.name, self.message)
        }
    }
}
impl std::error::Error for BybitTradingMarketsRequestError {}
pub fn is_bybit_leverage_not_modified(
    data: Option<&Map<String, Value>>,
) -> Result<bool, &'static str> {
    let owned = data
        .map(|m| Value::Object(m.clone()))
        .unwrap_or(Value::Null);
    let code = js_number(crate::json_field::field(&owned, "retCode")?).map_err(|e| e.message)?;
    let message = crate::json_field::field(&owned, "retMsg")?;
    let message = if js_truthy(message) {
        js_string(message).map_err(|e| e.message)?
    } else {
        String::new()
    }
    .to_lowercase();
    Ok(code == 110043. || (message.contains("leverage") && message.contains("not modified")))
}
