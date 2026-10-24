//! Opaque dispatch recipe and public codec declarations.
pub mod dispatch;
use serde::Deserialize;
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrokerOptions {
    pub partner: String,
    pub broker_name: String,
    pub verify: Option<bool>,
}
impl BrokerOptions {
    pub(crate) fn descriptor(&self) -> dispatch::Broker {
        dispatch::Broker {
            partner: self.partner.clone(),
            name: self.broker_name.clone(),
            verify: self.verify.unwrap_or(true),
        }
    }
}
safe_cex_crypto::redacted_debug!(BrokerOptions);
pub(crate) const PUBLIC_HEADERS: &[(&str, &str)] = &[
    ("X-CHANNEL-API-CODE", "3zexp"),
    ("Content-Type", "application/json"),
];
pub(crate) fn public_headers() -> Vec<(&'static str, String)> {
    PUBLIC_HEADERS
        .iter()
        .map(|(name, value)| (*name, (*value).into()))
        .collect()
}
#[cfg(test)]
#[path = "../tests/support/native_sign.rs"]
mod native;
#[cfg(test)]
pub(crate) use native::{Credentials, headers};
