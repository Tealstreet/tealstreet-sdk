//! Bybit's explicit-status and proxy-envelope normalization.
use safe_cex_num::JsCoercionError;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct BybitResponseMeta {
    pub is_success: bool,
    pub has_explicit_status: bool,
    pub ret_code: Option<f64>,
    pub ret_msg: Option<String>,
    pub raw_response: Option<Input>,
}

fn normalize_message(value: &Input) -> Result<Option<String>, JsCoercionError> {
    let Some(units) = value.as_text() else {
        return Ok(None);
    };
    let text = String::from_utf16(units).map_err(|_| JsCoercionError {
        name: "TypeError",
        message: "Bybit ret_msg cannot be represented as a Rust string",
    })?;
    Ok(Some(safe_cex_num::js_trim(&text))
        .filter(|s| !s.is_empty())
        .map(str::to_owned))
}
pub(crate) fn normalize_code(value: &Input) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return number.is_finite().then_some(number);
    }
    let text = String::from_utf16(value.as_text()?).ok()?;
    let text = safe_cex_num::js_trim(&text);
    if text.is_empty() {
        return None;
    }
    let number = safe_cex_num::number_from_string(text);
    number.is_finite().then_some(number)
}
fn parse_raw_response(value: &Input) -> Option<Input> {
    if value.is_object() {
        return Some(value.clone());
    }
    let parsed = Input::parse_json(value.as_text()?).ok()?;
    parsed.is_object().then_some(parsed)
}
fn first_defined_code<'a>(values: impl IntoIterator<Item = &'a Input>) -> Option<f64> {
    values.into_iter().find_map(normalize_code)
}
fn first_defined_message<'a>(
    values: impl IntoIterator<Item = &'a Input>,
) -> Result<Option<String>, JsCoercionError> {
    for value in values {
        if let Some(message) = normalize_message(value)? {
            return Ok(Some(message));
        }
    }
    Ok(None)
}
pub fn get_bybit_response_meta(payload: &Input) -> Result<BybitResponseMeta, JsCoercionError> {
    let raw = payload
        .get("raw_response")
        .filter(|v| !v.is_nullish())
        .or_else(|| payload.get("rawResponse"));
    let raw_response = raw.and_then(parse_raw_response);
    let envelopes = [payload, raw_response.as_ref().unwrap_or(&Input::Null)];
    let ret_code = first_defined_code(envelopes.iter().flat_map(|e| {
        ["retCode", "ret_code", "code"].map(|key| e.get(key).unwrap_or(&Input::Undefined))
    }));
    let ret_msg = first_defined_message(envelopes.iter().flat_map(|e| {
        ["retMsg", "ret_msg", "message", "msg"].map(|key| e.get(key).unwrap_or(&Input::Undefined))
    }))?;
    Ok(BybitResponseMeta {
        is_success: ret_code.map_or(ret_msg.as_deref() == Some("OK"), |code| code == 0.0),
        has_explicit_status: ret_code.is_some() || ret_msg.is_some(),
        ret_code,
        ret_msg,
        raw_response,
    })
}

// Private remote companion: the public path requires a map before serde can
// select its otherwise-permissive positional record visitor.
#[derive(Serialize, Deserialize)]
#[serde(remote = "BybitResponseMeta")]
#[serde(rename_all = "camelCase")]
struct BybitResponseMetaWire {
    is_success: bool,
    has_explicit_status: bool,
    #[serde(
        default,
        deserialize_with = "present_meta",
        skip_serializing_if = "Option::is_none"
    )]
    ret_code: Option<f64>,
    #[serde(
        default,
        deserialize_with = "present_meta",
        skip_serializing_if = "Option::is_none"
    )]
    ret_msg: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_raw_response",
        skip_serializing_if = "Option::is_none"
    )]
    raw_response: Option<Input>,
}
impl<'de> Deserialize<'de> for BybitResponseMeta {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                BybitResponseMetaWire::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl Serialize for BybitResponseMeta {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BybitResponseMetaWire::serialize(self, serializer)
    }
}

fn present_meta<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
fn present_raw_response<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Input>, D::Error> {
    let value = Input::deserialize(d)?;
    // Record<string, any> includes arrays in TS; do not narrow this to maps.
    if value.is_object() || value.is_array() {
        Ok(Some(value))
    } else {
        Err(serde::de::Error::custom(
            "rawResponse must be a non-null object or array",
        ))
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitResponseMeta {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {
            is_success, has_explicit_status, ret_code, ret_msg
        } passthrough { raw_response });
    }
}
