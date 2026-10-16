//! Transport-free Starbase protocol primitives.
use serde::Serialize;
use std::fmt;

// ts: CoinbaseStarbaseProtocolError
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoinbaseStarbaseProtocolError {
    pub name: String,
    pub message: String,
}
impl CoinbaseStarbaseProtocolError {
    // ts: CoinbaseStarbaseProtocolError.constructor
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            name: "CoinbaseStarbaseProtocolError".into(),
            message: message.into(),
        }
    }
}
impl fmt::Display for CoinbaseStarbaseProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.name.is_empty() {
            f.write_str(&self.message)
        } else if self.message.is_empty() {
            f.write_str(&self.name)
        } else {
            write!(f, "{}: {}", self.name, self.message)
        }
    }
}
impl std::error::Error for CoinbaseStarbaseProtocolError {}

// ts: CoinbaseStarbaseJsonRpcErrorObject
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseJsonRpcErrorObject {
    pub code: f64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ts: CoinbaseStarbaseJsonRpcErrorResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseJsonRpcErrorResponse {
    pub jsonrpc: CoinbaseStarbaseJsonRpcVersion,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<f64>,
    pub error: CoinbaseStarbaseJsonRpcErrorObject,
}

// Rust representation of the inline TypeScript literal field `jsonrpc: '2.0'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseJsonRpcVersion {
    #[serde(rename = "2.0")]
    V2,
}

// ts: requireCoinbaseStarbaseMethod
pub fn require_coinbase_starbase_method(
    value: Option<&serde_json::Value>,
) -> Result<&str, CoinbaseStarbaseProtocolError> {
    if let Some(method) = value.and_then(serde_json::Value::as_str) {
        let name = method
            .strip_prefix("public/")
            .or_else(|| method.strip_prefix("private/"));
        if let Some(name) = name {
            let bytes = name.as_bytes();
            if bytes.first().is_some_and(u8::is_ascii_lowercase)
                && bytes
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
            {
                return Ok(method);
            }
        }
    }
    Err(CoinbaseStarbaseProtocolError::new(
        "Invalid Starbase JSON-RPC method",
    ))
}

// ts: isRecord
pub fn is_record(value: Option<&serde_json::Value>) -> bool {
    value.is_some_and(serde_json::Value::is_object)
}

// ts: isSafeRequestId
pub fn is_safe_request_id(value: Option<&serde_json::Value>) -> bool {
    value.and_then(serde_json::Value::as_f64).is_some_and(|v| {
        v.is_finite() && v.fract() == 0.0 && v > 0.0 && v <= 9_007_199_254_740_991.0
    })
}

// ts: parseRpcError
pub fn parse_rpc_error(
    value: Option<&serde_json::Value>,
) -> Option<CoinbaseStarbaseJsonRpcErrorObject> {
    if !is_record(value) {
        return None;
    }
    let value = value?;
    let code = value.get("code")?.as_f64()?;
    if !code.is_finite() || code.fract() != 0.0 {
        return None;
    }
    Some(CoinbaseStarbaseJsonRpcErrorObject {
        code,
        message: value.get("message")?.as_str()?.into(),
        data: value.get("data").cloned(),
    })
}

// ts: CoinbaseStarbaseAuthResult.token_type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseTokenType {
    #[serde(rename = "bearer")]
    Bearer,
}

// ts: CoinbaseStarbaseAuthResult
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseAuthResult {
    pub access_token: String,
    pub expires_in: f64,
    pub scope: String,
    pub token_type: CoinbaseStarbaseTokenType,
}

// ts: validateAuthResult
pub fn validate_auth_result(
    result: Option<&serde_json::Value>,
) -> Result<(), CoinbaseStarbaseProtocolError> {
    let valid = result
        .and_then(serde_json::Value::as_object)
        .is_some_and(|result| {
            result
                .get("access_token")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|v| !v.is_empty())
                && result
                    .get("expires_in")
                    .and_then(serde_json::Value::as_f64)
                    .is_some_and(|v| v.is_finite() && v.fract() == 0.0 && v > 0.0)
                && result
                    .get("scope")
                    .is_some_and(serde_json::Value::is_string)
                && result.get("token_type").and_then(serde_json::Value::as_str) == Some("bearer")
        });
    if valid {
        Ok(())
    } else {
        Err(CoinbaseStarbaseProtocolError::new(
            "Invalid public/auth result",
        ))
    }
}

// ts: CoinbaseStarbaseTransportKind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseTransportKind {
    #[serde(rename = "http")]
    Http,
    #[serde(rename = "websocket")]
    Websocket,
}

// ts: resolveCoinbaseStarbaseEndpoint
pub fn resolve_coinbase_starbase_endpoint(kind: CoinbaseStarbaseTransportKind) -> &'static str {
    match kind {
        CoinbaseStarbaseTransportKind::Http => "https://drb.coinbase.com/api/v2",
        CoinbaseStarbaseTransportKind::Websocket => "wss://drb.coinbase.com/ws/api/v2",
    }
}

// ts: CoinbaseStarbaseTransportContext
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseTransportContext {
    pub endpoint: String,
    pub kind: CoinbaseStarbaseTransportKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<String>,
}

// ts: CoinbaseStarbaseHeartbeatSignal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseHeartbeatSignal {
    #[serde(rename = "heartbeat")]
    Heartbeat,
    #[serde(rename = "test_request")]
    TestRequest,
}

// ts: CoinbaseStarbaseTestResult
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseTestResult {
    pub version: String,
}
