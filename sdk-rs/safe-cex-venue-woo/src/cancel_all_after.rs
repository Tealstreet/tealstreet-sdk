//! Inert dead-man-switch descriptors; no transport or timer is owned here.
use safe_cex_num::numeric_ingress::{JsNumberInput, js_value_as_f64};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

// ts: WOO_V3_CANCEL_ALL_AFTER_LIMITS
pub const WOO_V3_CANCEL_ALL_AFTER_LIMITS: &[(&str, f64)] = &[
    ("minimumTriggerAfterMs", 0.0),
    ("maximumTriggerAfterMs", 900_000.0),
    ("requestsPerSecond", 10.0),
];
// ts: WOO_V3_CANCEL_ALL_AFTER_PARITY
pub const WOO_V3_CANCEL_ALL_AFTER_PARITY: &[(&str, bool)] = &[
    ("vipOnly", true),
    ("cancelsOrdinaryOrders", true),
    ("cancelsAlgoOrders", true),
    ("zeroCancelsTimer", true),
    ("repeatedCallReplacesTimer", true),
    ("requestAndResponseFixtures", true),
    ("runtimeImported", false),
    ("transportOwner", false),
    ("credentialedDemoLifecycle", false),
];
// ts: requireRecord
pub fn require_record<'a>(
    input: Option<&'a Value>,
    path: &str,
) -> Result<&'a Map<String, Value>, String> {
    input
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{path} must be an object"))
}
// ts: requireExactKeys
pub fn require_exact_keys(
    input: &Map<String, Value>,
    expected: &[&str],
    path: &str,
) -> Result<(), String> {
    let mut actual: Vec<_> = input.keys().map(String::as_str).collect();
    let mut expected = expected.to_vec();
    actual.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    expected.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    if actual != expected {
        return Err(format!(
            "{path} must contain exactly {}",
            expected.join(", ")
        ));
    }
    Ok(())
}
// ts: requireNonnegativeSafeInteger
pub fn require_nonnegative_safe_integer(input: Option<&Value>, path: &str) -> Result<f64, String> {
    input
        .and_then(js_value_as_f64)
        .filter(|n| n.fract() == 0.0 && (0.0..=9_007_199_254_740_991.0).contains(n))
        .ok_or_else(|| format!("{path} must be a nonnegative safe integer"))
}
// ts: buildWooV3CancelAllAfterRequest
pub fn build_woo_v3_cancel_all_after_request(input: Option<&Value>) -> Result<Value, String> {
    let trigger_after = input
        .and_then(js_value_as_f64)
        .filter(|n| n.fract() == 0.0 && (0.0..=900_000.0).contains(n))
        .ok_or_else(|| {
            "WOO V3 triggerAfter must be a safe integer from 0 through 900000 milliseconds"
                .to_owned()
        })?;
    let path = serde_json::to_value(crate::v3_contracts::WooV3Endpoint::TradeCancelAllAfter)
        .map_err(|e| e.to_string())?;
    Ok(json!({ "method": "POST", "path": path, "body": { "triggerAfter": trigger_after } }))
}
// ts: decodeWooV3CancelAllAfterResponse
pub fn decode_woo_v3_cancel_all_after_response(input: Option<&Value>) -> Result<Value, String> {
    let path = "WOO V3 cancel-all-after response";
    let response = require_record(input, path)?;
    require_exact_keys(response, &["success", "timestamp", "data"], path)?;
    if response.get("success") != Some(&Value::Bool(true)) {
        return Err(format!("{path}.success must be true"));
    }
    let data = require_record(response.get("data"), &format!("{path}.data"))?;
    require_exact_keys(data, &["expectedTriggerTime"], &format!("{path}.data"))?;
    let timestamp =
        require_nonnegative_safe_integer(response.get("timestamp"), &format!("{path}.timestamp"))?;
    let expected_trigger_time = require_nonnegative_safe_integer(
        data.get("expectedTriggerTime"),
        &format!("{path}.data.expectedTriggerTime"),
    )?;
    Ok(
        json!({ "accepted": true, "timestamp": timestamp, "expectedTriggerTime": expected_trigger_time }),
    )
}

fn post_literal<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<crate::v3_contracts::WooV3HttpMethod, D::Error> {
    let v = crate::v3_contracts::WooV3HttpMethod::deserialize(d)?;
    if v == crate::v3_contracts::WooV3HttpMethod::Post {
        Ok(v)
    } else {
        Err(serde::de::Error::custom(
            "cancel-all-after method must be POST",
        ))
    }
}
fn cancel_path_literal<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<crate::v3_contracts::WooV3Endpoint, D::Error> {
    let v = crate::v3_contracts::WooV3Endpoint::deserialize(d)?;
    if v == crate::v3_contracts::WooV3Endpoint::TradeCancelAllAfter {
        Ok(v)
    } else {
        Err(serde::de::Error::custom(
            "cancel-all-after path must match its endpoint",
        ))
    }
}
fn accepted_literal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let v = bool::deserialize(d)?;
    if v {
        Ok(v)
    } else {
        Err(serde::de::Error::custom("accepted must be true"))
    }
}
// Capture numbers before nested Serde buffering; every record requires map ingress.

#[derive(Debug, Clone)]
pub struct WooV3CancelAllAfterBody {
    pub trigger_after: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3CancelAllAfterBody")]
struct WooV3CancelAllAfterBodyFields {
    #[serde(rename = "triggerAfter")]
    trigger_after: f64,
}
impl Serialize for WooV3CancelAllAfterBody {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3CancelAllAfterBodyFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3CancelAllAfterBody {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3CancelAllAfterBodyFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3CancelAllAfterBody {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{trigger_after});
    }
}
// ts: WooV3CancelAllAfterRequest

#[derive(Debug, Clone)]
pub struct WooV3CancelAllAfterRequest {
    pub method: crate::v3_contracts::WooV3HttpMethod,
    pub path: crate::v3_contracts::WooV3Endpoint,
    pub body: WooV3CancelAllAfterBody,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3CancelAllAfterRequest")]
struct WooV3CancelAllAfterRequestFields {
    #[serde(rename = "method", deserialize_with = "post_literal")]
    method: crate::v3_contracts::WooV3HttpMethod,
    #[serde(rename = "path", deserialize_with = "cancel_path_literal")]
    path: crate::v3_contracts::WooV3Endpoint,
    #[serde(rename = "body")]
    body: WooV3CancelAllAfterBody,
}
impl Serialize for WooV3CancelAllAfterRequest {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3CancelAllAfterRequestFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3CancelAllAfterRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3CancelAllAfterRequestFields::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3CancelAllAfterRequest {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{method,path,body});
    }
}
// ts: WooV3CancelAllAfterResponse

#[derive(Debug, Clone)]
pub struct WooV3CancelAllAfterResponse {
    pub accepted: bool,
    pub timestamp: f64,
    pub expected_trigger_time: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3CancelAllAfterResponse")]
struct WooV3CancelAllAfterResponseFields {
    #[serde(rename = "accepted", deserialize_with = "accepted_literal")]
    accepted: bool,
    #[serde(rename = "timestamp")]
    timestamp: f64,
    #[serde(rename = "expectedTriggerTime")]
    expected_trigger_time: f64,
}
impl Serialize for WooV3CancelAllAfterResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3CancelAllAfterResponseFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3CancelAllAfterResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3CancelAllAfterResponseFields::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3CancelAllAfterResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{accepted,timestamp,expected_trigger_time});
    }
}
