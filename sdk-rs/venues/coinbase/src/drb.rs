//! Shared JSON-RPC transport for the Coinbase derivatives gateway.
//! Consumers own their pending contexts and forward replies/loss to this transport.
use safe_cex_engine::{Cx, ErrorKind, RequestId, SocketId, classify::Verdict};
use serde_json::Value;

pub const HTTP_URL: &str = "https://drb.coinbase.com/api/v2";
pub const WS_URL: &str = "wss://drb.coinbase.com/ws/api/v2";
pub const PUBLIC_WS_URL: &str = "wss://streams.drb.coinbase.com/ws/api/v2";
pub const MAX_FRAME_BYTES: usize = 32 * 1024;
pub type RpcId = u64;

/// HTTP bearer sessions and authenticated main WebSocket sessions are distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Http,
    WebSocket(SocketId),
}

/// Released requests never grant automatic resend permission.
#[derive(Debug)]
pub struct Failure {
    pub verdict: Verdict,
    pub code: Option<i64>,
}

#[derive(Default)]
pub struct Transport;

impl Transport {
    /// Apply documented account-summary limits without replenishing used credits.
    pub fn set_matching_limits(&mut self, _rate: f64, _burst: f64) -> Result<(), String> {
        Err("drb interface stub".into())
    }
    pub fn http<P>(
        &mut self,
        _cx: &mut Cx<'_, P>,
        _method: &str,
        _params: Value,
        _authorization: Option<&str>,
        _pending: P,
    ) -> Result<(RpcId, RequestId), String> {
        Err("drb interface stub".into())
    }
    pub fn websocket<P>(
        &mut self,
        _cx: &mut Cx<'_, P>,
        _socket: SocketId,
        _method: &str,
        _params: Value,
    ) -> Result<RpcId, String> {
        Err("drb interface stub".into())
    }
    pub fn response(&mut self, _id: RpcId, _status: u16, _body: &str) -> Result<Value, Failure> {
        Err(Failure {
            verdict: Verdict::Unknown(None),
            code: None,
        })
    }
    pub fn failed(&mut self, _id: RpcId, _kind: ErrorKind) -> Option<Failure> {
        None
    }
    pub fn disconnected(&mut self, _socket: SocketId) -> Vec<(RpcId, Failure)> {
        Vec::new()
    }
    pub fn expire(&mut self, _now_ms: f64) -> Vec<(RpcId, Failure)> {
        Vec::new()
    }
}

pub fn to_instrument(_symbol: &str) -> Result<String, String> {
    Err("drb interface stub".into())
}
pub fn from_instrument(_instrument: &str) -> Result<String, String> {
    Err("drb interface stub".into())
}
