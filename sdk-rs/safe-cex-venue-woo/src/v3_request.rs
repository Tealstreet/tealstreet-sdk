//! Pure V3 request descriptors; no transport or migration gate is owned here.
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
use serde_json::{Map, Value};

// ts: WooV3RequestAuth
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3RequestAuth {
    pub api_key: String,
    pub api_secret: String,
    pub timestamp: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3RequestAuth")]
struct WooV3RequestAuthWire {
    #[serde(rename = "apiKey")]
    api_key: String,
    #[serde(rename = "apiSecret")]
    api_secret: String,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3RequestAuth {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3RequestAuthWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3RequestAuth {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3RequestAuthWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3RequestAuth {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { api_key, api_secret, timestamp });
    }
}

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

// ts: WooV3PreparedRequest
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3PreparedRequest {
    pub method: WooV3HttpMethod,
    pub path: String,
    pub body: Option<String>,
    pub headers: JsObjectMap<String>,
    pub signature_payload: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PreparedRequest")]
struct WooV3PreparedRequestWire {
    #[serde(rename = "method")]
    method: WooV3HttpMethod,
    #[serde(rename = "path")]
    path: String,
    #[serde(
        rename = "body",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    body: Option<String>,
    #[serde(rename = "headers")]
    headers: JsObjectMap<String>,
    #[serde(
        rename = "signaturePayload",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    signature_payload: Option<String>,
}
impl Serialize for WooV3PreparedRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3PreparedRequestWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3PreparedRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3PreparedRequestWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PreparedRequest {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { method, path, body, headers, signature_payload });
    }
}

// ts: serializeQuery
pub fn serialize_query(query: Option<&Value>) -> Result<String, String> {
    crate::api::sorted_query(
        &safe_cex_exchange::js_runtime::value::from_json(query.unwrap_or(&Value::Null)),
        true,
    )
}

// ts: prepareWooV3Request
pub async fn prepare_woo_v3_request(
    input: &Map<String, Value>,
    auth: Option<&WooV3RequestAuth>,
) -> Result<WooV3PreparedRequest, String> {
    let method = input
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            match input.get("method") {
                None => "Cannot read properties of undefined (reading 'toUpperCase')",
                Some(Value::Null) => "Cannot read properties of null (reading 'toUpperCase')",
                Some(_) => "input.method.toUpperCase is not a function",
            }
            .to_owned()
        })?
        .to_uppercase();
    let is_query_method = matches!(method.as_str(), "GET" | "DELETE");
    let input_path = input.get("path").and_then(Value::as_str).ok_or_else(|| {
        match input.get("path") {
            None => "Cannot read properties of undefined (reading 'startsWith')",
            Some(Value::Null) => "Cannot read properties of null (reading 'startsWith')",
            Some(_) => "input.path.startsWith is not a function",
        }
        .to_owned()
    })?;
    if !input_path.starts_with("/v3/") {
        return Err("WOO V3 requests must use a /v3/ path".into());
    }
    let body_value = input
        .get("body")
        .filter(|value| safe_cex_num::js_truthy(Some(value)));
    let query_value = input
        .get("query")
        .filter(|value| safe_cex_num::js_truthy(Some(value)));
    if is_query_method && body_value.is_some() {
        return Err(format!(
            "{method} WOO V3 requests must place parameters in the query string"
        ));
    }
    if !is_query_method && query_value.is_some() {
        return Err(format!(
            "{method} WOO V3 requests must place parameters in the JSON body"
        ));
    }
    let query = serialize_query(query_value)?;
    let path = if query.is_empty() {
        input_path.into()
    } else {
        format!("{input_path}?{query}")
    };
    let body = body_value
        .map(|value| {
            safe_cex_num::js_json_stringify(&safe_cex_num::JsJsonValue::from(value.clone()))
                .map_err(|error| error.message.to_owned())?
                .ok_or_else(|| "WOO V3 JSON body serialized as undefined".to_owned())
        })
        .transpose()?;
    let mut headers = JsObjectMap::new();
    let signature_payload = if input_path.starts_with("/v3/public/") {
        None
    } else {
        let auth = auth.ok_or_else(|| {
            "Authenticated WOO V3 requests require explicit credentials and timestamp".to_owned()
        })?;
        let timestamp = safe_cex_num::js_number_string(auth.timestamp);
        let payload = format!("{timestamp}{method}{path}{}", body.as_deref().unwrap_or(""));
        let signature = safe_cex_crypto::sign_payload(
            &safe_cex_crypto::SigningOptions {
                key: &auth.api_key,
                secret: &auth.api_secret,
                payload_signer: None,
            },
            safe_cex_types::ExchangeName::Woo,
            &payload,
            safe_cex_crypto::Encoding::Hex,
        )
        .await?
        .signature;
        headers.insert("x-api-key".into(), auth.api_key.clone());
        headers.insert("x-api-timestamp".into(), timestamp);
        headers.insert("x-api-signature".into(), signature);
        Some(payload)
    };
    if body.is_some() {
        headers.insert("content-type".into(), "application/json".into());
    }
    // Refuse only after the source query/body/signing phases have completed.
    let method = match method.as_str() {
        "GET" => WooV3HttpMethod::Get,
        "POST" => WooV3HttpMethod::Post,
        "PUT" => WooV3HttpMethod::Put,
        "DELETE" => WooV3HttpMethod::Delete,
        _ => return Err("WOO V3 prepared method refuses an unsupported method".into()),
    };
    Ok(WooV3PreparedRequest {
        method,
        path,
        body,
        headers,
        signature_payload,
    })
}
