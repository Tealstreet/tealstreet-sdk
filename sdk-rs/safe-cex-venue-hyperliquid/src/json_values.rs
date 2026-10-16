//! Raw JSON own-property snapshots. Keys/strings are UTF-16; numbers are binary64.
//! JSON.parse key replacement/order is retained. No Value ingress/prototype claim.
use crate::spot_json::{JsString, number_token, string_token};
use serde::Deserialize;
use serde_json::value::RawValue;

#[derive(Debug, Clone, PartialEq)]
pub struct JsonObject<T>(pub Vec<(JsString, T)>);
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(JsString),
    Array(Vec<JsonValue>),
    Object(JsonObject<JsonValue>),
}

fn array_index(key: &[u16]) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key[0] == b'0' as u16) {
        return None;
    }
    let mut value = 0u32;
    for &unit in key {
        if !(b'0' as u16..=b'9' as u16).contains(&unit) {
            return None;
        }
        value = value
            .checked_mul(10)?
            .checked_add((unit - b'0' as u16) as u32)?;
    }
    (value != u32::MAX).then_some(value)
}
impl<T> JsonObject<T> {
    pub fn get(&self, key: &[u16]) -> Option<&T> {
        self.0
            .iter()
            .find(|(name, _)| name.0 == key)
            .map(|(_, value)| value)
    }
}
impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for JsonObject<T> {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(decoder)?;
        let token = raw.get();
        let bytes = token.as_bytes();
        if bytes.first() != Some(&b'{') {
            return Err(serde::de::Error::custom("expected JSON object"));
        }
        let mut offset = 1;
        let skip = |offset: &mut usize| {
            while matches!(bytes.get(*offset), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                *offset += 1;
            }
        };
        // Parse values only AFTER last-duplicate selection: earlier values may
        // fail a typed dictionary constraint, but JSON.parse has replaced them.
        let mut fields: Vec<(JsString, String)> = Vec::new();
        loop {
            skip(&mut offset);
            if bytes.get(offset) == Some(&b'}') {
                break;
            }
            let start = offset;
            offset += 1;
            while let Some(character) = bytes.get(offset) {
                offset += 1;
                if *character == b'\\' {
                    offset += 1;
                } else if *character == b'"' {
                    break;
                }
            }
            let key =
                JsString(string_token(&token[start..offset]).map_err(serde::de::Error::custom)?);
            skip(&mut offset);
            offset += 1; // RawValue has validated the colon.
            skip(&mut offset);
            let mut decoder = serde_json::Deserializer::from_str(&token[offset..]);
            let value =
                Box::<RawValue>::deserialize(&mut decoder).map_err(serde::de::Error::custom)?;
            offset += value.get().len();
            if let Some((_, old)) = fields.iter_mut().find(|(name, _)| *name == key) {
                *old = value.get().to_owned();
            } else {
                fields.push((key, value.get().to_owned()));
            }
            skip(&mut offset);
            if bytes.get(offset) == Some(&b'}') {
                break;
            }
            offset += 1; // Validated comma.
        }
        // Enumerable integer keys precede other keys; noninteger insertion order
        // remains stable, including __proto__ (JSON.parse creates an own key).
        fields.sort_by_key(|(key, _)| array_index(&key.0).map_or((1, 0), |index| (0, index)));
        fields
            .into_iter()
            .map(|(key, raw)| {
                serde_json::from_str::<T>(&raw)
                    .map(|value| (key, value))
                    .map_err(serde::de::Error::custom)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }
}
impl<'de> Deserialize<'de> for JsonValue {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(decoder)?;
        let token = raw.get();
        let result: Result<Self, serde_json::Error> = match token.as_bytes().first() {
            Some(b'n') => Ok(Self::Null),
            Some(b't') => Ok(Self::Bool(true)),
            Some(b'f') => Ok(Self::Bool(false)),
            Some(b'"') => serde_json::from_str(token).map(Self::String),
            Some(b'[') => serde_json::from_str(token).map(Self::Array),
            Some(b'{') => serde_json::from_str(token).map(Self::Object),
            _ => number_token(token)
                .map(Self::Number)
                .map_err(<serde_json::Error as serde::de::Error>::custom),
        };
        result.map_err(serde::de::Error::custom)
    }
}

/// TypeScript's any-valued string index signatures also accept arrays.
/// Retain that native alternative instead of forcing every value into a map.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonContainer {
    Object(JsonObject<JsonValue>),
    Array(Vec<JsonValue>),
}
impl<'de> Deserialize<'de> for JsonContainer {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(decoder)?;
        match raw.get().as_bytes().first() {
            Some(b'{') => serde_json::from_str(raw.get()).map(Self::Object),
            Some(b'[') => serde_json::from_str(raw.get()).map(Self::Array),
            _ => Err(<serde_json::Error as serde::de::Error>::custom(
                "expected JSON object or array",
            )),
        }
        .map_err(serde::de::Error::custom)
    }
}

// JSON serialization for deserialized own-data snapshots. Native values are
// observed independently before this optional wire operation. Raw JSON keys
// retain UTF-16 units, including lone surrogates; a String map-key adapter would
// refuse them. Manually assembled Vec entries are not normalized by this API.
impl<T: serde::Serialize> serde::Serialize for JsonObject<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut json = String::from("{");
        for (i, (key, value)) in self.0.iter().enumerate() {
            if i != 0 {
                json.push(',');
            }
            json.push_str(&serde_json::to_string(key).map_err(serde::ser::Error::custom)?);
            json.push(':');
            json.push_str(&serde_json::to_string(value).map_err(serde::ser::Error::custom)?);
        }
        json.push('}');
        let raw = RawValue::from_string(json).map_err(serde::ser::Error::custom)?;
        serde::Serialize::serialize(&raw, serializer)
    }
}
impl serde::Serialize for JsonValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Number(value) => {
                let json = if value.is_finite() {
                    safe_cex_num::js_number_string(*value)
                } else {
                    "null".to_string()
                };
                let raw = RawValue::from_string(json).map_err(serde::ser::Error::custom)?;
                serde::Serialize::serialize(&raw, serializer)
            }
            Self::String(value) => serde::Serialize::serialize(value, serializer),
            Self::Array(value) => serde::Serialize::serialize(value, serializer),
            Self::Object(value) => serde::Serialize::serialize(value, serializer),
        }
    }
}
impl serde::Serialize for JsonContainer {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Object(value) => serde::Serialize::serialize(value, serializer),
            Self::Array(value) => serde::Serialize::serialize(value, serializer),
        }
    }
}
