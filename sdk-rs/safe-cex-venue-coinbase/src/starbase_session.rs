//! Pure cancel-on-disconnect contracts; no transport or activation.
use crate::starbase::CoinbaseStarbaseProtocolError;
use serde::Serialize;
use serde_json::{Map, Value};

// ts: CoinbaseStarbaseWebsocketSessionState
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseWebsocketSessionState {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "activating")]
    Activating,
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "disconnected")]
    Disconnected,
    #[serde(rename = "rate-limit-backoff")]
    RateLimitBackoff,
    #[serde(rename = "failed")]
    Failed,
}

impl<'de> serde::Deserialize<'de> for CoinbaseStarbaseWebsocketSessionState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        match value.as_str() {
            "idle" => Ok(Self::Idle),
            "activating" => Ok(Self::Activating),
            "ready" => Ok(Self::Ready),
            "disconnected" => Ok(Self::Disconnected),
            "rate-limit-backoff" => Ok(Self::RateLimitBackoff),
            "failed" => Ok(Self::Failed),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &[
                    "idle",
                    "activating",
                    "ready",
                    "disconnected",
                    "rate-limit-backoff",
                    "failed",
                ],
            )),
        }
    }
}

// ts: CoinbaseStarbaseCancelOnDisconnectPolicy
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseCancelOnDisconnectPolicy {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<CoinbaseStarbaseCancelOnDisconnectScope>,
}

// ts: CoinbaseStarbaseWebsocketDisconnectReason
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseWebsocketDisconnectReason {
    #[serde(rename = "heartbeat-timeout")]
    HeartbeatTimeout,
    #[serde(rename = "rate-limit")]
    RateLimit,
    #[serde(rename = "transport")]
    Transport,
}

impl<'de> serde::Deserialize<'de> for CoinbaseStarbaseWebsocketDisconnectReason {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        match value.as_str() {
            "heartbeat-timeout" => Ok(Self::HeartbeatTimeout),
            "rate-limit" => Ok(Self::RateLimit),
            "transport" => Ok(Self::Transport),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["heartbeat-timeout", "rate-limit", "transport"],
            )),
        }
    }
}

// ts: CoinbaseStarbaseCancelOnDisconnectScope
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseCancelOnDisconnectScope {
    #[serde(rename = "connection")]
    Connection,
    #[serde(rename = "account")]
    Account,
}

// ts: CoinbaseStarbaseSessionRequest
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseSessionRequest<Method: AsRef<str>, Params: Into<Map<String, Value>>> {
    pub method: Method,
    pub params: Params,
}

// ts: CoinbaseStarbaseCancelOnDisconnectParams
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseCancelOnDisconnectParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<CoinbaseStarbaseCancelOnDisconnectScope>,
}
impl From<CoinbaseStarbaseCancelOnDisconnectParams> for Map<String, Value> {
    fn from(params: CoinbaseStarbaseCancelOnDisconnectParams) -> Self {
        let mut map = Self::new();
        if let Some(scope) = params.scope {
            let scope = match scope {
                CoinbaseStarbaseCancelOnDisconnectScope::Connection => "connection",
                CoinbaseStarbaseCancelOnDisconnectScope::Account => "account",
            };
            map.insert("scope".into(), Value::String(scope.into()));
        }
        map
    }
}

// ts: CoinbaseStarbaseCancelOnDisconnectResult
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseCancelOnDisconnectResult {
    pub enabled: bool,
    pub scope: CoinbaseStarbaseCancelOnDisconnectScope,
}

// ts: protocolError
fn protocol_error(message: &str) -> CoinbaseStarbaseProtocolError {
    CoinbaseStarbaseProtocolError::new(message)
}

// ts: isRecord
fn is_record(value: &Value) -> bool {
    value.is_object()
}

// ts: parseScope
fn parse_scope(
    value: Option<&Value>,
) -> Result<CoinbaseStarbaseCancelOnDisconnectScope, CoinbaseStarbaseProtocolError> {
    match value.and_then(Value::as_str) {
        Some("connection") => Ok(CoinbaseStarbaseCancelOnDisconnectScope::Connection),
        Some("account") => Ok(CoinbaseStarbaseCancelOnDisconnectScope::Account),
        _ => Err(protocol_error(
            "Invalid Starbase cancel-on-disconnect scope",
        )),
    }
}

// ts: buildScopeParams
fn build_scope_params(
    scope: Option<&Value>,
) -> Result<CoinbaseStarbaseCancelOnDisconnectParams, CoinbaseStarbaseProtocolError> {
    Ok(CoinbaseStarbaseCancelOnDisconnectParams {
        scope: scope.map(|value| parse_scope(Some(value))).transpose()?,
    })
}

// ts: buildCoinbaseStarbaseEnableCancelOnDisconnect
pub fn build_coinbase_starbase_enable_cancel_on_disconnect(
    scope: Option<&Value>,
) -> Result<
    CoinbaseStarbaseSessionRequest<&'static str, CoinbaseStarbaseCancelOnDisconnectParams>,
    CoinbaseStarbaseProtocolError,
> {
    Ok(CoinbaseStarbaseSessionRequest {
        method: "private/enable_cancel_on_disconnect",
        params: build_scope_params(scope)?,
    })
}

// ts: buildCoinbaseStarbaseDisableCancelOnDisconnect
pub fn build_coinbase_starbase_disable_cancel_on_disconnect(
    scope: Option<&Value>,
) -> Result<
    CoinbaseStarbaseSessionRequest<&'static str, CoinbaseStarbaseCancelOnDisconnectParams>,
    CoinbaseStarbaseProtocolError,
> {
    Ok(CoinbaseStarbaseSessionRequest {
        method: "private/disable_cancel_on_disconnect",
        params: build_scope_params(scope)?,
    })
}

// ts: buildCoinbaseStarbaseGetCancelOnDisconnect
pub fn build_coinbase_starbase_get_cancel_on_disconnect(
    scope: Option<&Value>,
) -> Result<
    CoinbaseStarbaseSessionRequest<&'static str, CoinbaseStarbaseCancelOnDisconnectParams>,
    CoinbaseStarbaseProtocolError,
> {
    Ok(CoinbaseStarbaseSessionRequest {
        method: "private/get_cancel_on_disconnect",
        params: build_scope_params(scope)?,
    })
}

// ts: decodeCoinbaseStarbaseCancelOnDisconnectMutationResult
pub fn decode_coinbase_starbase_cancel_on_disconnect_mutation_result(
    value: Option<&Value>,
) -> Result<&'static str, CoinbaseStarbaseProtocolError> {
    if value.and_then(Value::as_str) != Some("ok") {
        return Err(protocol_error(
            "Invalid Starbase cancel-on-disconnect mutation result",
        ));
    }
    Ok("ok")
}

// ts: decodeCoinbaseStarbaseGetCancelOnDisconnectResult
pub fn decode_coinbase_starbase_get_cancel_on_disconnect_result(
    value: Option<&Value>,
) -> Result<CoinbaseStarbaseCancelOnDisconnectResult, CoinbaseStarbaseProtocolError> {
    let Some(map) = value.filter(|v| is_record(v)).and_then(Value::as_object) else {
        return Err(protocol_error(
            "Invalid Starbase cancel-on-disconnect result: expected an object",
        ));
    };
    if map.len() != 2 || !map.contains_key("enabled") || !map.contains_key("scope") {
        return Err(protocol_error(
            "Invalid Starbase cancel-on-disconnect result: expected exact enabled and scope fields",
        ));
    }
    let Some(enabled) = map.get("enabled").and_then(Value::as_bool) else {
        return Err(protocol_error(
            "Invalid Starbase cancel-on-disconnect enabled value",
        ));
    };
    Ok(CoinbaseStarbaseCancelOnDisconnectResult {
        enabled,
        scope: parse_scope(map.get("scope"))?,
    })
}
