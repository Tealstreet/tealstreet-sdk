//! V3 request input shared by inert builders; no transport is registered here.
use serde::{Deserialize, Serialize};

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

use crate::v3_contracts::{WooV3Endpoint, WooV3HttpMethod};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_types::object_map::JsObjectMap;

// ts: WooV3RequestInput
#[derive(Debug, Clone)]
pub struct WooV3RequestInput {
    pub method: WooV3HttpMethod,
    pub path: WooV3Endpoint,
    pub query: Option<JsObjectMap<JsNumberInput>>,
    pub body: Option<JsObjectMap<JsNumberInput>>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3RequestInput")]
struct WooV3RequestInputWire {
    #[serde(rename = "method")]
    method: WooV3HttpMethod,
    #[serde(rename = "path")]
    path: WooV3Endpoint,
    #[serde(
        rename = "query",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    query: Option<JsObjectMap<JsNumberInput>>,
    #[serde(
        rename = "body",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    body: Option<JsObjectMap<JsNumberInput>>,
}
impl Serialize for WooV3RequestInput {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3RequestInputWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3RequestInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3RequestInputWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3RequestInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { method, path, query, body });
    }
}
