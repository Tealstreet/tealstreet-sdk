//! Closed signing-only HTTP work. The supplied receipt owns the exact nonce.
use safe_cex_engine::signing::{Identity, SignError, SignReceipt, SignRequest};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Descriptor {
    pub(crate) kind: Kind,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) form: String,
    pub(crate) nonce: String,
    pub(crate) stamp_ms: u64,
    pub(crate) testnet: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub enum Kind {
    #[serde(rename = "kraken.futures.http")]
    Futures,
    #[serde(rename = "kraken.spot.http")]
    Spot,
}
impl Descriptor {
    pub fn from_json(json: &str) -> Result<Self, SignError> {
        serde_json::from_str(json).map_err(|_| SignError::InvalidRequest)
    }
    pub fn work(&self, _: &Identity, _: SignReceipt) -> Result<SignRequest, SignError> {
        Err(SignError::Unsupported)
    }
    pub fn describe(_: &SignRequest) -> Result<Self, SignError> {
        Err(SignError::Unsupported)
    }
}
#[cfg(test)]
#[path = "hosted_tests.rs"]
mod tests;
