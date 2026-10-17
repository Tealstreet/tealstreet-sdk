//! Signer wire declarations; transport strings retain the shared UTF-8 boundary.
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum EthSignatureKind {
    EthereumSignature,
    EIP1271Signature,
}
impl<'de> serde::Deserialize<'de> for EthSignatureKind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(decoder)?.as_str() {
            "EthereumSignature" => Ok(Self::EthereumSignature),
            "EIP1271Signature" => Ok(Self::EIP1271Signature),
            _ => Err(serde::de::Error::custom(
                "unsupported Ethereum signature type",
            )),
        }
    }
}
impl JsNumberOutput for EthSignatureKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { EthereumSignature, EIP1271Signature });
    }
}

// ts: TxEthSignature
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TxEthSignature {
    #[serde(rename = "type")]
    pub kind: EthSignatureKind,
    pub signature: String,
}
#[derive(serde::Deserialize)]
#[serde(remote = "TxEthSignature")]
struct TxEthSignatureWire {
    #[serde(rename = "type")]
    kind: EthSignatureKind,
    signature: String,
}
impl<'de> serde::Deserialize<'de> for TxEthSignature {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                TxEthSignatureWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for TxEthSignature {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { kind, signature });
    }
}
