//! Declared WEEX response shapes, after JSON parsing and before normalization.
//! These codecs validate the declared shape, not unchecked Axios runtime inputs.
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};
use safe_cex_types::object_map::JsObjectMap;
use serde::{Deserialize, Serialize};

/// The number|string elements of WeexKlineRow; numbers retain binary64 bits.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WeexNumberOrString {
    Number(f64),
    String(String),
}

#[derive(Deserialize)]
#[serde(remote = "WeexNumberOrString", untagged)]
enum NumberOrStringWire {
    Number(f64),
    String(String),
}

impl<'de> Deserialize<'de> for WeexNumberOrString {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(NumberOrStringWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

impl JsNumberOutput for WeexNumberOrString {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Number(value), String(value) });
    }
}

pub type WeexKlineRow = (
    WeexNumberOrString,
    String,
    String,
    String,
    String,
    String,
    WeexNumberOrString,
    String,
    WeexNumberOrString,
    String,
    String,
);

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

/// TypeScript Record<string, any> admits arrays as well as ordinary objects.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum WeexExchangeInfoRecord {
    Object(JsObjectMap<JsNumberInput>),
    Array(Vec<JsNumberInput>),
}

#[derive(Deserialize)]
#[serde(remote = "WeexExchangeInfoRecord", untagged)]
enum ExchangeInfoRecordWire {
    Object(JsObjectMap<JsNumberInput>),
    Array(Vec<JsNumberInput>),
}

impl<'de> Deserialize<'de> for WeexExchangeInfoRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(ExchangeInfoRecordWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WeexExchangeInfoRecord {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Object(value), Array(value) });
    }
}

/// Keep any-valued properties as retained input, never a lossy Value buffer.
#[derive(Debug, Clone, Serialize)]
pub struct WeexExchangeInfoResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<Vec<WeexExchangeInfoRecord>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbols: Option<Vec<WeexExchangeInfoRecord>>,
}

#[derive(Deserialize)]
#[serde(remote = "WeexExchangeInfoResponse")]
struct ExchangeInfoWire {
    #[serde(default, deserialize_with = "deserialize_present")]
    assets: Option<Vec<WeexExchangeInfoRecord>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    symbols: Option<Vec<WeexExchangeInfoRecord>>,
}

impl<'de> Deserialize<'de> for WeexExchangeInfoResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                ExchangeInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl JsNumberOutput for WeexExchangeInfoResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { assets, symbols });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeexOrderResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Deserialize)]
#[serde(remote = "WeexOrderResult", rename_all = "camelCase")]
struct OrderResultWire {
    #[serde(default, deserialize_with = "deserialize_present")]
    order_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    client_order_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    success: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_present")]
    error_code: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    error_message: Option<String>,
}

impl<'de> Deserialize<'de> for WeexOrderResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                OrderResultWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl JsNumberOutput for WeexOrderResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { order_id, client_order_id, success, error_code, error_message });
    }
}
