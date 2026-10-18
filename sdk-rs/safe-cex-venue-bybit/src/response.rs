//! Bybit's explicit-status and proxy-envelope normalization.
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct BybitResponseMeta {
    pub is_success: bool,
    pub has_explicit_status: bool,
    pub ret_code: Option<f64>,
    pub ret_msg: Option<Vec<u16>>,
    pub raw_response: Option<Input>,
}

fn normalize_message(value: &Input) -> Option<Vec<u16>> {
    let units = value.as_text()?;
    let whitespace = |unit: &u16| {
        char::from_u32(u32::from(*unit)).is_some_and(|character| {
            let mut text = [0; 4];
            safe_cex_num::js_trim(character.encode_utf8(&mut text)).is_empty()
        })
    };
    let start = units.iter().position(|unit| !whitespace(unit))?;
    let end = units.iter().rposition(|unit| !whitespace(unit)).unwrap() + 1;
    Some(units[start..end].to_vec())
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
fn first_defined_message<'a>(values: impl IntoIterator<Item = &'a Input>) -> Option<Vec<u16>> {
    for value in values {
        if let Some(message) = normalize_message(value) {
            return Some(message);
        }
    }
    None
}
pub fn get_bybit_response_meta(payload: &Input) -> BybitResponseMeta {
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
    }));
    BybitResponseMeta {
        is_success: ret_code.map_or(ret_msg.as_deref() == Some(&[79, 75]), |code| code == 0.0),
        has_explicit_status: ret_code.is_some() || ret_msg.is_some(),
        ret_code,
        ret_msg,
        raw_response,
    }
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
        deserialize_with = "present_message",
        serialize_with = "serialize_message",
        skip_serializing_if = "Option::is_none"
    )]
    ret_msg: Option<Vec<u16>>,
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
fn present_message<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<u16>>, D::Error> {
    let value = Input::deserialize(d)?;
    value
        .as_text()
        .map(|units| Some(units.to_vec()))
        .ok_or_else(|| serde::de::Error::custom("retMsg must be a string"))
}
fn serialize_message<S: serde::Serializer>(
    value: &Option<Vec<u16>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value
        .as_ref()
        .map(|units| Input::Text(units.clone()))
        .serialize(serializer)
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
