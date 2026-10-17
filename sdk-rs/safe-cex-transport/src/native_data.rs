//! Retained adapter data views and final protocol JSON serialization.
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde::{Serialize, Serializer};

pub(crate) fn parse(text: &str) -> Result<Input, serde_json::Error> {
    Input::parse_json(&text.encode_utf16().collect::<Vec<_>>())
}
pub(crate) fn property<'a>(value: &'a Input, name: &str) -> Option<&'a Input> {
    let Input::Object(entries) = value else {
        return None;
    };
    entries
        .iter()
        .rev()
        .find(|(key, _)| key.iter().copied().eq(name.encode_utf16()))
        .map(|(_, value)| value)
}
pub(crate) fn take_property(value: &mut Input, name: &str) -> Option<Input> {
    let Input::Object(entries) = value else {
        return None;
    };
    let index = entries
        .iter()
        .rposition(|(key, _)| key.iter().copied().eq(name.encode_utf16()))?;
    Some(entries.remove(index).1)
}
pub(crate) fn number(value: &Input) -> Option<f64> {
    match value {
        Input::Float(value) => Some(*value),
        Input::Lexeme(value) => value.parse().ok(),
        _ => None,
    }
}
pub(crate) fn entries(value: &Input) -> Vec<(Vec<u16>, &Input)> {
    match value {
        Input::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, value)| (index.to_string().encode_utf16().collect(), value))
            .collect(),
        Input::Object(values) => {
            let mut assigned = indexmap::IndexMap::new();
            for (key, value) in values {
                assigned.insert(key.as_slice(), value);
            }
            safe_cex_types::js_coercion::js_own_keys_utf16(assigned.keys().copied())
                .into_iter()
                .map(|key| (key.to_vec(), assigned[key]))
                .collect()
        }
        _ => Vec::new(),
    }
}
pub(crate) fn serialize_optional<S: Serializer>(
    value: &Option<Input>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => Wire(value).serialize(serializer),
        None => serializer.serialize_none(),
    }
}
pub(crate) fn omitted(value: &Option<Input>) -> bool {
    matches!(value, None | Some(Input::Undefined))
}
/// A borrowed wire-only view, never used to deliver native callbacks.
pub(crate) struct Wire<'a>(pub &'a Input);
impl Serialize for Wire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text =
            safe_cex_num::js_retained_json_stringify(self.0).map_err(serde::ser::Error::custom)?;
        match text {
            Some(text) => serde_json::value::RawValue::from_string(text)
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer),
            None => serializer.serialize_none(),
        }
    }
}

pub(crate) fn deserialize_present<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Input>, D::Error> {
    <Input as serde::Deserialize>::deserialize(deserializer).map(Some)
}
pub(crate) fn text(value: &Input) -> Option<String> {
    match value {
        Input::Text(units) => String::from_utf16(units).ok(),
        _ => None,
    }
}
pub(crate) fn object(fields: impl IntoIterator<Item = (impl AsRef<str>, Input)>) -> Input {
    Input::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.as_ref().encode_utf16().collect(), value))
            .collect(),
    )
}
pub(crate) fn from_json(value: serde_json::Value) -> Result<Input, crate::TransportError> {
    Input::from_value(value).map_err(|error| crate::TransportError::Host(error.to_string()))
}
/// A deliberate diagnostic/typed-metadata JSON view, never adapter data.
pub(crate) fn wire_json(value: &Input) -> Result<serde_json::Value, crate::TransportError> {
    serde_json::to_value(Wire(value))
        .map_err(|error| crate::TransportError::Host(error.to_string()))
}
