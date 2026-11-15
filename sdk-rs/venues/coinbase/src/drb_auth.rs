//! CDP credential exchange and per-channel access-token lifetime.
use crate::{drb::Channel, sign::Credentials};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Mutex;
use zeroize::{Zeroize, Zeroizing};

/// A session belongs to one HTTP identity or one physical WebSocket connection.
/// Tokens are erased on invalidation/drop and never exposed through Debug.
pub struct Session {
    channel: Channel,
    token: Option<Zeroizing<String>>,
    /// Latest observed authentication start, retained after token invalidation.
    started_ms: f64,
    /// Params can be generated through a shared reference; retain the latest issued start.
    issued_ms: Mutex<f64>,
    invalidated_through_ms: Option<f64>,
    refresh_ms: Option<f64>,
}
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("channel", &self.channel)
            .field("refresh_ms", &self.refresh_ms)
            .finish_non_exhaustive()
    }
}
#[derive(Deserialize)]
struct AuthResult {
    access_token: String,
    expires_in: u32,
    #[serde(rename = "scope")]
    _scope: String,
    token_type: String,
}
impl Drop for AuthResult {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}
impl Session {
    pub fn new(channel: Channel) -> Self {
        Self {
            channel,
            token: None,
            started_ms: 0.,
            issued_ms: Mutex::new(0.),
            invalidated_through_ms: None,
            refresh_ms: None,
        }
    }
    /// Create a fresh signed CDP JWT for each public/auth call. The credential
    /// travels only in JSON params, never URL/query or WS upgrade headers.
    /// Retain now_ms as the request start passed to accept.
    pub fn params(
        &self,
        credentials: &Credentials,
        now_ms: f64,
        fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
    ) -> Result<Value, String> {
        self.params_at(credentials, now_ms, now_ms, fill)
    }
    /// Gateway JWTs use protocol UTC; session expiry and invalidation retain the
    /// scheduling clock captured for the request's owner.
    pub(crate) fn params_at(
        &self,
        credentials: &Credentials,
        started_ms: f64,
        protocol_ms: f64,
        fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
    ) -> Result<Value, String> {
        let uri = match self.channel {
            Channel::Http => Some("POST drb.coinbase.com/api/v2/public/auth"),
            Channel::WebSocket(_) => None,
        };
        let jwt = Zeroizing::new(credentials.drb_jwt(protocol_ms, uri, fill)?);
        self.remember_start(started_ms)?;
        Ok(json!({"grant_type":"coinbase_cdp","token":jwt.as_str()}))
    }
    /// This placeholder is staged with a signing recipe before it can leave the
    /// engine. It contains no JWT until the installed provider signs at dispatch.
    pub(crate) fn pending_params(&self, started_ms: f64) -> Result<Value, String> {
        self.remember_start(started_ms)?;
        Ok(json!({"grant_type":"coinbase_cdp","token":""}))
    }
    fn remember_start(&self, now_ms: f64) -> Result<(), String> {
        if !now_ms.is_finite() || now_ms < 0. {
            return Err("drb invalid authentication clock".into());
        }
        let mut issued = self
            .issued_ms
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *issued = issued.max(now_ms);
        Ok(())
    }
    /// Use request-start time so network latency cannot extend token validity.
    /// Call only after Transport has correlated a successful public/auth reply.
    pub fn accept(&mut self, result: Value, started_ms: f64) -> Result<(), String> {
        if !started_ms.is_finite()
            || started_ms < 0.
            || started_ms < self.started_ms
            || self
                .invalidated_through_ms
                .is_some_and(|at| started_ms <= at)
        {
            return Err("drb invalid or stale authentication clock".into());
        }
        self.started_ms = started_ms;
        self.clear_token();
        let mut result: AuthResult =
            serde_json::from_value(result).map_err(|_| "drb invalid public/auth result")?;
        if result.access_token.is_empty()
            || result.access_token.len() > 16_384
            || result
                .access_token
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
            || result.expires_in == 0
            || result.token_type != "bearer"
        {
            return Err("drb invalid public/auth result".into());
        }
        // Scope must be a string, but an empty scope can be a valid read session.
        let policy_ms: f64 = match self.channel {
            Channel::Http => 15. * 60. * 1000.,
            Channel::WebSocket(_) => 49. * 60. * 1000.,
        };
        let lifetime_ms = (f64::from(result.expires_in) * 1000.).min(policy_ms);
        let refresh_ms = started_ms + lifetime_ms - (lifetime_ms * 0.1).min(30_000.);
        if !refresh_ms.is_finite() || refresh_ms <= started_ms {
            return Err("drb invalid authentication expiry".into());
        }
        self.token = Some(Zeroizing::new(std::mem::take(&mut result.access_token)));
        self.refresh_ms = Some(refresh_ms);
        Ok(())
    }
    pub fn refresh_at(&self) -> Option<f64> {
        self.refresh_ms
    }
    pub fn needs_refresh(&self, now_ms: f64) -> bool {
        !now_ms.is_finite()
            || now_ms < self.started_ms
            || self.refresh_ms.is_none_or(|refresh| now_ms >= refresh)
    }
    pub fn authorization(&self, now_ms: f64) -> Option<String> {
        if self.channel != Channel::Http || self.needs_refresh(now_ms) {
            return None;
        }
        self.token
            .as_ref()
            .map(|token| format!("Bearer {}", token.as_str()))
    }
    /// Erase the cache and fence every authentication issued before invalidation.
    /// Fresh exchanges must use a later start time; equal timestamps fail closed.
    pub fn invalidate(&mut self) {
        let issued = self
            .issued_ms
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        self.invalidated_through_ms = Some(self.started_ms.max(*issued));
        self.clear_token();
    }
    fn clear_token(&mut self) {
        self.token = None;
        self.refresh_ms = None;
    }
}
