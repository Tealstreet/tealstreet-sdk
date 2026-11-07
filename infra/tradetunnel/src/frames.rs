// Shared server/client wire shapes; keep serialized keys unchanged.
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Map;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize)]
// TS sends any JSON body; server decoding deliberately remains map-only.
pub(crate) struct HttpRequestArgs<D = Map<String, serde_json::Value>> {
    pub(crate) headers: Map<String, serde_json::Value>,
    pub(crate) method: String,
    pub(crate) url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data: Option<D>,
    pub(crate) params: Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(remote = "HttpRequestArgs")]
struct HttpRequestArgsWire<D> {
    headers: Map<String, serde_json::Value>,
    method: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<D>,
    params: Map<String, serde_json::Value>,
}

struct HttpRequestArgsMapVisitor<D>(std::marker::PhantomData<D>);
impl<'de, D: Deserialize<'de>> Visitor<'de> for HttpRequestArgsMapVisitor<D> {
    type Value = HttpRequestArgs<D>;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an HTTP request object")
    }
    fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
        HttpRequestArgsWire::<D>::deserialize(serde::de::value::MapAccessDeserializer::new(map))
    }
}
impl<'de, D: Deserialize<'de>> Deserialize<'de> for HttpRequestArgs<D> {
    fn deserialize<I: serde::Deserializer<'de>>(d: I) -> Result<Self, I::Error> {
        d.deserialize_map(HttpRequestArgsMapVisitor::<D>(std::marker::PhantomData))
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RpcHttpResponse {
    #[serde(rename = "type")]
    pub(crate) response_type: String,
    pub(crate) nonce: String,
    pub(crate) data: ResponseData,
}

#[derive(Deserialize)]
#[serde(remote = "RpcHttpResponse")]
struct RpcHttpResponseWire {
    #[serde(rename = "type")]
    response_type: String,
    nonce: String,
    data: ResponseData,
}

struct RpcHttpResponseMapVisitor;
impl<'de> Visitor<'de> for RpcHttpResponseMapVisitor {
    type Value = RpcHttpResponse;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an RPC HTTP response object")
    }
    fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
        RpcHttpResponseWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
    }
}
impl<'de> Deserialize<'de> for RpcHttpResponse {
    fn deserialize<I: serde::Deserializer<'de>>(d: I) -> Result<Self, I::Error> {
        d.deserialize_map(RpcHttpResponseMapVisitor)
    }
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Deserialize)]
#[serde(remote = "ResponseData")]
struct ResponseDataWire {
    config: Config,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_encoded: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_encoding: Option<String>,
    status: u16,
    status_text: String,
    headers: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_dump: Option<RequestDump>,
}

struct ResponseDataMapVisitor;
impl<'de> Visitor<'de> for ResponseDataMapVisitor {
    type Value = ResponseData;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an HTTP response data object")
    }
    fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
        ResponseDataWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
    }
}
impl<'de> Deserialize<'de> for ResponseData {
    fn deserialize<I: serde::Deserializer<'de>>(d: I) -> Result<Self, I::Error> {
        d.deserialize_map(ResponseDataMapVisitor)
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Config {
    pub(crate) url: String,
}

#[derive(Deserialize)]
#[serde(remote = "Config")]
struct ConfigWire {
    url: String,
}

struct ConfigMapVisitor;
impl<'de> Visitor<'de> for ConfigMapVisitor {
    type Value = Config;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a response config object")
    }
    fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
        ConfigWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
    }
}
impl<'de> Deserialize<'de> for Config {
    fn deserialize<I: serde::Deserializer<'de>>(d: I) -> Result<Self, I::Error> {
        d.deserialize_map(ConfigMapVisitor)
    }
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Deserialize)]
#[serde(remote = "RequestDump")]
struct RequestDumpWire {
    method: String,
    url: String,
    headers: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    // FINDING (SDK transport consumer): owned Value decoding can erase lexical -0
    // in this diagnostic field. The server owns this shared wire shape; SDK HOW
    // logs the original frame and never derives RestResponse data from this body.
    // Pinned by how::wire_tests::finding_request_dump_body_rebuffers_signed_zero.
    body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    form_data: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_form_data: Option<String>,
}

struct RequestDumpMapVisitor;
impl<'de> Visitor<'de> for RequestDumpMapVisitor {
    type Value = RequestDump;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a request dump object")
    }
    fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
        RequestDumpWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))
    }
}
impl<'de> Deserialize<'de> for RequestDump {
    fn deserialize<I: serde::Deserializer<'de>>(d: I) -> Result<Self, I::Error> {
        d.deserialize_map(RequestDumpMapVisitor)
    }
}
