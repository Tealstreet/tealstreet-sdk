//! Opaque native canonical work; production builder adoption is a later phase.
use safe_cex_engine::signing::{Identity, SignError, SignRequest, SignResult};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep};

pub const UNSIGNED_HEADERS: &[(&str, &str)] = &[
    ("Content-Type", "application/json"),
    ("User-Agent", "Tealstreet"),
];
fn public_headers() -> Vec<(&'static str, String)> {
    UNSIGNED_HEADERS
        .iter()
        .map(|&(name, value)| (name, value.into()))
        .collect()
}
pub struct Http {
    identity: Identity,
    method: Method,
    path_query: String,
    body: Option<String>,
    stamp: Option<f64>,
}
impl Http {
    pub fn new(
        identity: Identity,
        method: Method,
        path_query: String,
        body: Option<String>,
    ) -> Self {
        Self {
            identity,
            method,
            path_query,
            body,
            stamp: None,
        }
    }
    pub fn unsigned(&self) -> (String, Vec<(&'static str, String)>, Option<String>) {
        (
            format!("{}{}", crate::paths::REST, self.path_query),
            public_headers(),
            self.body.clone(),
        )
    }
}
impl SignStep for Http {
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let _ = (&self.identity, self.method, &self.stamp);
        Err(SignError::Unavailable)
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Err(SignError::InvalidRequest)
    }
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}
pub struct Upgrade {
    identity: Identity,
    stamp: Option<f64>,
}
impl Upgrade {
    pub fn new(identity: Identity) -> Self {
        Self {
            identity,
            stamp: None,
        }
    }
    pub fn unsigned(&self) -> safe_cex_engine::SocketSpec {
        safe_cex_engine::SocketSpec::new("wss://ws-contract.weex.com/v3/ws/private")
            .headers(public_headers())
            .channel(safe_cex_engine::SocketChannel::Private)
    }
}
impl SignStep for Upgrade {
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let _ = (&self.identity, &self.stamp);
        Err(SignError::Unavailable)
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Err(SignError::InvalidRequest)
    }
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}
