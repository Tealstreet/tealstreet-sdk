//! CDP credential exchange and per-channel access-token lifetime.
use crate::{drb::Channel, sign::Credentials};
use serde_json::Value;

pub struct Session;
impl Session {
    pub fn new(_channel: Channel) -> Self {
        Self
    }
    /// Create a fresh signed CDP JWT for each public/auth call.
    pub fn params(
        &self,
        _credentials: &Credentials,
        _now_ms: f64,
        _fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
    ) -> Result<Value, String> {
        Err("drb interface stub".into())
    }
    /// Use request-start time so network latency cannot extend token validity.
    pub fn accept(&mut self, _result: Value, _started_ms: f64) -> Result<(), String> {
        Err("drb interface stub".into())
    }
    pub fn refresh_at(&self) -> Option<f64> {
        None
    }
    pub fn needs_refresh(&self, _now_ms: f64) -> bool {
        true
    }
    pub fn authorization(&self, _now_ms: f64) -> Option<String> {
        None
    }
    pub fn invalidate(&mut self) {}
}
