//! Owned decoded-JSON fields, with numeric capture before private typed decoding.
//! Input order and in-place values are retained; this is not a live JS object ABI.
use safe_cex_num::numeric_ingress::{JsNumberInput, js_capture_output, js_value_as_f64};
use safe_cex_types::{object_map::JsObjectMap, retained_json::RetainedJsonInput};
use serde::{Deserialize, Serialize, Serializer, de::Error, ser::SerializeMap};
use serde_json::Value;

pub(crate) struct RecordFields {
    pub keys: Vec<String>,
    pub additional: Value,
}

pub(crate) fn capture<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
    known: &[&str],
) -> Result<(JsNumberInput, RecordFields), D::Error> {
    let input = JsNumberInput::deserialize(deserializer)?;
    let observed = js_capture_output(&input).map_err(D::Error::custom)?;
    let RetainedJsonInput::Object(entries) = normalize(observed) else {
        return Err(D::Error::custom(
            "Crypto.com wire record requires an object",
        ));
    };
    let keys = entries
        .iter()
        .map(|(key, _)| String::from_utf16(key).map_err(D::Error::custom))
        .collect::<Result<Vec<_>, _>>()?;
    let additional = RetainedJsonInput::Object(
        entries
            .into_iter()
            .filter(|(key, _)| {
                !known
                    .iter()
                    .any(|name| name.encode_utf16().eq(key.iter().copied()))
            })
            .collect(),
    )
    .into_value()
    .map_err(D::Error::custom)?;
    Ok((input, RecordFields { keys, additional }))
}

fn normalize(input: RetainedJsonInput) -> RetainedJsonInput {
    match input {
        RetainedJsonInput::Object(entries) => RetainedJsonInput::Object(
            entries
                .into_iter()
                .map(|(key, value)| {
                    (
                        String::from_utf16(&key).expect("typed numeric capture has UTF-8 keys"),
                        normalize(value),
                    )
                })
                .collect::<JsObjectMap<_>>()
                .into_iter()
                .map(|(key, value)| (key.encode_utf16().collect(), value))
                .collect(),
        ),
        RetainedJsonInput::Array(values) => {
            RetainedJsonInput::Array(values.into_iter().map(normalize).collect())
        }
        value => value,
    }
}

pub(crate) fn keys(captured: &[String], known: &[(&str, bool)], additional: &Value) -> Vec<String> {
    let mut result = JsObjectMap::new();
    let extra = additional.as_object();
    for key in captured {
        let present = known.iter().find(|(name, _)| *name == key).map_or_else(
            || extra.is_some_and(|map| map.contains_key(key)),
            |(_, present)| *present,
        );
        if present {
            result.insert(key.clone(), ());
        }
    }
    for (key, present) in known {
        if *present {
            result.insert((*key).to_owned(), ());
        }
    }
    if let Some(extra) = extra {
        for key in extra.keys() {
            if !known.iter().any(|(name, _)| *name == key) {
                result.insert(key.clone(), ());
            }
        }
    }
    result.into_iter().map(|(key, ())| key).collect()
}

/// Serialize retained extras as native JS Numbers, never opaque number tokens.
/// The same events reach the existing numeric-output guard before JSON projection.
pub(crate) struct ExtraValue<'a>(pub &'a Value);
impl Serialize for ExtraValue<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        match self.0 {
            Value::Null => serializer.serialize_unit(),
            Value::Bool(value) => serializer.serialize_bool(*value),
            Value::String(value) => serializer.serialize_str(value),
            Value::Number(_) => serializer.serialize_f64(
                js_value_as_f64(self.0)
                    .ok_or_else(|| serde::ser::Error::custom("invalid retained JS Number"))?,
            ),
            Value::Array(values) => {
                let mut output = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    output.serialize_element(&ExtraValue(value))?;
                }
                output.end()
            }
            Value::Object(values) => {
                let ordered: JsObjectMap<_> = values
                    .iter()
                    .map(|(key, value)| (key.clone(), value))
                    .collect();
                let mut output = serializer.serialize_map(Some(ordered.len()))?;
                for (key, value) in ordered {
                    output.serialize_entry(&key, &ExtraValue(value))?;
                }
                output.end()
            }
        }
    }
}
