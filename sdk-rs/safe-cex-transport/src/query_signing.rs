//! Opt-in native query wire DTOs; legacy signer APIs keep Value parameters.
use crate::query::{deserialize_query_wire, query_wire_omitted, serialize_query_wire};
use indexmap::IndexMap;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{ExchangeName, QueryValue};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningConfig {
    pub method: String,
    pub url: String,
    #[serde(rename = "baseURL")]
    pub base_url: String,
    #[serde(
        skip_serializing_if = "query_wire_omitted",
        serialize_with = "serialize_query_wire"
    )]
    pub params: Option<QueryValue>,
    #[serde(
        skip_serializing_if = "crate::native_data::omitted",
        serialize_with = "crate::native_data::serialize_optional"
    )]
    pub data: Option<Input>,
    pub headers: IndexMap<String, String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpSigningRequest {
    pub id: String,
    pub exchange_name: ExchangeName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
    pub config: SigningConfig,
}
#[derive(Default, Serialize)]
pub struct SignedConfig {
    pub url: Option<String>,
    #[serde(
        skip_serializing_if = "query_wire_omitted",
        serialize_with = "serialize_query_wire"
    )]
    pub params: Option<QueryValue>,
    #[serde(
        skip_serializing_if = "crate::native_data::omitted",
        serialize_with = "crate::native_data::serialize_optional"
    )]
    pub data: Option<Input>,
    pub headers: IndexMap<String, String>,
}
#[derive(Default, Serialize)]
pub struct HttpSigningResponse {
    pub id: String,
    pub config: Option<SignedConfig>,
    pub error: Option<String>,
}
#[derive(Deserialize)]
#[serde(remote = "SigningConfig")]
#[serde(rename_all = "camelCase")]
struct WireSigningConfig {
    method: String,
    url: String,
    #[serde(rename = "baseURL")]
    base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "deserialize_query_wire")]
    params: Option<QueryValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "crate::native_data::deserialize_present")]
    data: Option<Input>,
    headers: IndexMap<String, String>,
}
impl<'de> Deserialize<'de> for SigningConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireSigningConfig::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

#[derive(Deserialize)]
#[serde(remote = "HttpSigningRequest")]
#[serde(rename_all = "camelCase")]
struct WireHttpSigningRequest {
    id: String,
    exchange_name: ExchangeName,
    #[serde(skip_serializing_if = "Option::is_none")]
    account_name: Option<String>,
    config: SigningConfig,
}
impl<'de> Deserialize<'de> for HttpSigningRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireHttpSigningRequest::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

#[derive(Deserialize)]
#[serde(remote = "SignedConfig")]
struct WireSignedConfig {
    url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_query_wire")]
    params: Option<QueryValue>,
    #[serde(default, deserialize_with = "crate::native_data::deserialize_present")]
    data: Option<Input>,
    headers: IndexMap<String, String>,
}
impl<'de> Deserialize<'de> for SignedConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireSignedConfig::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

#[derive(Deserialize)]
#[serde(remote = "HttpSigningResponse")]
struct WireHttpSigningResponse {
    id: String,
    config: Option<SignedConfig>,
    error: Option<String>,
}
impl<'de> Deserialize<'de> for HttpSigningResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireHttpSigningResponse::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}
