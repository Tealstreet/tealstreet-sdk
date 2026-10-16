// Shared server/client wire shapes; keep serialized keys unchanged.
use serde::{Deserialize, Serialize};
use serde_json::Map;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize)]
// TS sends any JSON body; server decoding deliberately remains map-only.
pub(crate) struct HttpRequestArgs<D = Map<String, serde_json::Value>> {
    pub(crate) headers: Map<String, serde_json::Value>,
    pub(crate) method: String,
    pub(crate) url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data: Option<D>,
    pub(crate) params: Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RpcHttpResponse {
    #[serde(rename = "type")]
    pub(crate) response_type: String,
    pub(crate) nonce: String,
    pub(crate) data: ResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResponseData {
    pub(crate) config: Config,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data_raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data_encoded: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data_encoding: Option<String>,
    pub(crate) status: u16,
    pub(crate) status_text: String,
    pub(crate) headers: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_dump: Option<RequestDump>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Config {
    pub(crate) url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequestDump {
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) headers: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    // FINDING (SDK transport consumer): owned Value decoding can erase lexical -0
    // in this diagnostic field. The server owns this shared wire shape; SDK HOW
    // logs the original frame and never derives RestResponse data from this body.
    // Pinned by how::wire_tests::finding_request_dump_body_rebuffers_signed_zero.
    pub(crate) body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) form_data: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) raw_form_data: Option<String>,
}
