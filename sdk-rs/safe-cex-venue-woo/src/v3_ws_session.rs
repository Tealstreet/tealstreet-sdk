//! Pure listen-key and optional heartbeat operations; no socket lifecycle is enabled.
use crate::v3_contracts::{WooV3Endpoint, WooV3HttpMethod};
use crate::v3_request::WooV3RequestInput;
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_types::object_map::JsObjectMap;
use serde::{Deserialize, Serialize};

// ts: requireBoolean
pub fn require_boolean(value: Option<&JsNumberInput>, path: &str) -> Result<bool, String> {
    value
        .and_then(|v| v.clone().decode::<bool>().ok())
        .ok_or_else(|| format!("{path} must be a boolean"))
}
// ts: requireInteger
pub fn require_integer(
    value: Option<&JsNumberInput>,
    path: &str,
    minimum: f64,
) -> Result<f64, String> {
    let number = value.and_then(|v| v.clone().decode::<f64>().ok());
    // JS `!(n < minimum)`. is_finite() above means NaN cannot reach here
    // today, so clippy's `>=` rewrite would be equivalent - but the guard
    // order is what makes it equivalent, and the source expression is what
    // the parity row maps. Keep the spelling; the allow records why.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    match number {
        Some(n)
            if n.is_finite()
                && n.trunc() == n
                && n.abs() <= 9_007_199_254_740_991.0
                && !(n < minimum) =>
        {
            Ok(n)
        }
        _ => Err(format!(
            "{path} must be a safe integer of at least {}",
            safe_cex_num::js_number_string(minimum)
        )),
    }
}
fn input_string(value: Option<&JsNumberInput>) -> Option<String> {
    value.and_then(|v| v.clone().decode::<String>().ok())
}
fn nonempty_string(value: Option<&JsNumberInput>, path: &str) -> Result<String, String> {
    input_string(value)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{path} must be a non-empty string"))
}
// ts: requireDecimal
pub fn require_decimal(value: Option<&JsNumberInput>, path: &str) -> Result<String, String> {
    if let Some(decimal) = input_string(value) {
        let unsigned = decimal.strip_prefix('-').unwrap_or(&decimal);
        let mut parts = unsigned.split('.');
        let whole = parts.next().unwrap_or_default();
        let fraction = parts.next();
        let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        if parts.next().is_none()
            && digits(whole)
            && match fraction {
                None => !whole.is_empty(),
                Some(f) => digits(f) && (!whole.is_empty() || !f.is_empty()),
            }
        {
            return Ok(decimal);
        }
    }
    Err(format!("{path} must be a finite decimal string"))
}
// ts: requireNonNegativeDecimal
pub fn require_non_negative_decimal(
    value: Option<&JsNumberInput>,
    path: &str,
) -> Result<String, String> {
    let decimal = require_decimal(value, path)?;
    if decimal.starts_with('-') {
        return Err(format!("{path} must be a non-negative decimal string"));
    }
    Ok(decimal)
}
// ts: requirePositiveDecimal
pub fn require_positive_decimal(
    value: Option<&JsNumberInput>,
    path: &str,
) -> Result<String, String> {
    let decimal = require_decimal(value, path)?;
    if decimal.starts_with('-') || !decimal.bytes().any(|b| (b'1'..=b'9').contains(&b)) {
        return Err(format!("{path} must be a positive decimal string"));
    }
    Ok(decimal)
}
fn record(value: Option<&JsNumberInput>, path: &str) -> Result<JsObjectMap<JsNumberInput>, String> {
    value
        .and_then(|v| v.clone().decode::<JsObjectMap<JsNumberInput>>().ok())
        .ok_or_else(|| format!("{path} must be an object"))
}
// ts: buildWooV3ListenKeyRequest
pub fn build_woo_v3_listen_key_request() -> WooV3RequestInput {
    let body = serde_json::from_str::<JsObjectMap<JsNumberInput>>(r#"{"type":"WEBSOCKET"}"#)
        .expect("static request body");
    WooV3RequestInput {
        method: WooV3HttpMethod::Post,
        path: WooV3Endpoint::AccountListenKey,
        query: None,
        body: Some(body),
    }
}
// ts: WooV3ListenKey
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3ListenKey {
    pub auth_key: String,
    pub expired_time: f64,
    pub response_timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3ListenKey")]
struct ListenKeyFields {
    #[serde(rename = "authKey")]
    auth_key: String,
    #[serde(rename = "expiredTime")]
    expired_time: f64,
    #[serde(rename = "responseTimestamp")]
    response_timestamp: f64,
}
impl Serialize for WooV3ListenKey {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        ListenKeyFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3ListenKey {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|d| ListenKeyFields::deserialize(safe_cex_types::strict_decode::object(d)))
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3ListenKey {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { auth_key, expired_time, response_timestamp });
    }
}
// ts: decodeWooV3ListenKey
pub fn decode_woo_v3_listen_key(input: Option<&JsNumberInput>) -> Result<WooV3ListenKey, String> {
    let response = record(input, "WOO V3 listen key response")?;
    if !require_boolean(response.get("success"), "WOO V3 listen key.success")? {
        return Err("WOO V3 listen key response reports success=false".into());
    }
    let response_timestamp = require_integer(
        response.get("timestamp"),
        "WOO V3 listen key.timestamp",
        0.0,
    )?;
    let data = record(response.get("data"), "WOO V3 listen key.data")?;
    let expired_time = require_integer(
        data.get("expiredTime"),
        "WOO V3 listen key.data.expiredTime",
        1.0,
    )?;
    if expired_time <= response_timestamp {
        return Err("WOO V3 listen key must expire after its response timestamp".into());
    }
    Ok(WooV3ListenKey {
        auth_key: nonempty_string(data.get("authKey"), "WOO V3 listen key.data.authKey")?,
        expired_time,
        response_timestamp,
    })
}
// ts: buildWooV3PrivateWebSocketUrl
pub fn build_woo_v3_private_websocket_url(
    environment: &str,
    auth_key: Option<&JsNumberInput>,
) -> Result<String, String> {
    let key = nonempty_string(auth_key, "WOO V3 private websocket authKey")?;
    let base = if environment == "livenet" {
        "wss://wss.woox.io/v3/private"
    } else {
        "wss://wss.staging.woox.io/v3/private"
    };
    let mut encoded = String::new();
    for byte in key.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(encoded, "%{byte:02X}").expect("String write");
        }
    }
    Ok(format!("{base}?key={encoded}"))
}
fn ping_literal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "PING" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("cmd must be PING"))
    }
}
fn pong_literal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "PONG" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("cmd must be PONG"))
    }
}
fn true_literal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    if bool::deserialize(d)? {
        Ok(true)
    } else {
        Err(serde::de::Error::custom("success must be true"))
    }
}
// ts: WooV3WsPing
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsPing {
    pub cmd: String,
    pub ts: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsPing")]
struct PingFields {
    #[serde(deserialize_with = "ping_literal")]
    cmd: String,
    ts: f64,
}
impl Serialize for WooV3WsPing {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        PingFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsPing {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|d| PingFields::deserialize(safe_cex_types::strict_decode::object(d)))
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsPing {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { cmd, ts });
    }
}
// ts: WooV3WsPong
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsPong {
    pub cmd: String,
    pub success: bool,
    pub time: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsPong")]
struct PongFields {
    #[serde(deserialize_with = "pong_literal")]
    cmd: String,
    #[serde(deserialize_with = "true_literal")]
    success: bool,
    time: f64,
}
impl Serialize for WooV3WsPong {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        PongFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsPong {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|d| PongFields::deserialize(safe_cex_types::strict_decode::object(d)))
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsPong {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { cmd, success, time });
    }
}
// ts: buildWooV3WsPing
pub fn build_woo_v3_ws_ping(timestamp: Option<&JsNumberInput>) -> Result<WooV3WsPing, String> {
    Ok(WooV3WsPing {
        cmd: "PING".into(),
        ts: require_integer(timestamp, "WOO V3 websocket ping timestamp", 0.0)?,
    })
}
// ts: decodeWooV3WsPong
pub fn decode_woo_v3_ws_pong(input: Option<&JsNumberInput>) -> Result<WooV3WsPong, String> {
    let frame = record(input, "WOO V3 websocket pong")?;
    if input_string(frame.get("cmd")).as_deref() != Some("PONG") {
        return Err("WOO V3 websocket pong.cmd must be PONG".into());
    }
    if !require_boolean(frame.get("success"), "WOO V3 websocket pong.success")? {
        return Err("WOO V3 websocket pong reports success=false".into());
    }
    Ok(WooV3WsPong {
        cmd: "PONG".into(),
        success: true,
        time: require_integer(frame.get("time"), "WOO V3 websocket pong.time", 0.0)?,
    })
}
// ts: WOO_V3_WS_HEARTBEAT
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WsHeartbeatContract {
    pub classification: &'static str,
    pub server_initiated_ping_documented: bool,
    pub client_ping_command: &'static str,
    pub server_pong_command: &'static str,
}
pub const WOO_V3_WS_HEARTBEAT: WsHeartbeatContract = WsHeartbeatContract {
    classification: "optional-latency-probe-not-keepalive",
    server_initiated_ping_documented: false,
    client_ping_command: "PING",
    server_pong_command: "PONG",
};
