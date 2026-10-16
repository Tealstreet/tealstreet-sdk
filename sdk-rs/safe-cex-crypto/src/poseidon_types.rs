//! Poseidon loader interface records. These records impose no cryptographic key validation.
use serde::{Deserialize, Serialize};

// ts: SchnorrKeyPair
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchnorrKeyPair {
    pub private_key: String,
    pub public_key: String,
}
#[derive(Deserialize)]
#[serde(remote = "SchnorrKeyPair", rename_all = "camelCase")]
struct SchnorrKeyPairWire {
    private_key: String,
    public_key: String,
}
impl<'de> Deserialize<'de> for SchnorrKeyPair {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        SchnorrKeyPairWire::deserialize(safe_cex_types::strict_decode::object(decoder))
    }
}
// ts: WasmError
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WasmError {
    pub error: String,
}
#[derive(Deserialize)]
#[serde(remote = "WasmError")]
struct WasmErrorWire {
    error: String,
}
impl<'de> Deserialize<'de> for WasmError {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        WasmErrorWire::deserialize(safe_cex_types::strict_decode::object(decoder))
    }
}
