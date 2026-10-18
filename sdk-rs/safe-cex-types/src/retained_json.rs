//! Canonical retained JSON capture, extracted from numeric ingress.
//! Numeric lexemes and visitor bits survive until explicit JSON projection.
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Error, Value};
use std::fmt;
mod codec;
mod native;
mod parser;
mod payload;
mod writer;
pub use codec::{RETAINED_NATIVE, deserialize_native_payload};
const UTF16_UNITS: &str = "$safe_cex::Utf16Units";
const RAW_TOKEN: &str = "$serde_json::private::RawValue";
#[derive(Debug, Clone, PartialEq)]
pub enum RetainedJsonInput {
    /// Native JavaScript undefined; never constructed from JSON input.
    Undefined,
    Null,
    Bool(bool),
    Text(Vec<u16>),
    Lexeme(String),
    Float(f64),
    Array(Vec<Self>),
    Object(Vec<(Vec<u16>, Self)>),
}
impl RetainedJsonInput {
    /// JSON.parse semantics, retaining native UTF-16 and binary64 values.
    pub fn parse_json(text: &[u16]) -> Result<Self, Error> {
        parser::parse(text, false, 0)
    }
    pub fn from_value(value: Value) -> Result<Self, Error> {
        from_value_at_depth(value, 0)
    }
    pub fn into_value(self) -> Result<Value, Error> {
        into_value(self)
    }
}
/// Capture a declared opaque JSON field without losing numeric lexemes.
pub fn deserialize_retained_json<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Value, D::Error> {
    RetainedJsonInput::deserialize(deserializer)?
        .into_value()
        .map_err(de::Error::custom)
}
/// A present info:any field retains null; only absence defaults to None.
pub fn deserialize_present_retained_json<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    deserialize_retained_json(deserializer).map(Some)
}
fn from_value_at_depth(value: Value, depth: usize) -> Result<RetainedJsonInput, Error> {
    if depth >= 128 {
        return Err(de::Error::custom(
            "numeric ingress recursion limit exceeded",
        ));
    }
    Ok(match value {
        Value::Null => RetainedJsonInput::Null,
        Value::Bool(value) => RetainedJsonInput::Bool(value),
        Value::String(value) => RetainedJsonInput::Text(value.encode_utf16().collect()),
        Value::Number(value) => RetainedJsonInput::Lexeme(value.to_string()),
        Value::Array(values) => RetainedJsonInput::Array(
            values
                .into_iter()
                .map(|value| from_value_at_depth(value, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(values) => RetainedJsonInput::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        key.encode_utf16().collect(),
                        from_value_at_depth(value, depth + 1)?,
                    ))
                })
                .collect::<Result<_, Error>>()?,
        ),
    })
}

// Serde RawValue carries original JSON text. Reuse the same grammar while
// retaining number spelling for the existing numeric-ingress profile.
fn raw_node(raw: &str, depth: usize) -> Result<RetainedJsonInput, Error> {
    if depth >= 128 {
        return Err(de::Error::custom(
            "numeric ingress recursion limit exceeded",
        ));
    }
    parser::parse(&raw.encode_utf16().collect::<Vec<_>>(), true, depth)
}

struct Capture;
impl<'de> Visitor<'de> for Capture {
    type Value = RetainedJsonInput;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON numeric input")
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        BufferVisitor.visit_f64(v)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        if map.next_key::<String>()?.as_deref() != Some(RAW_TOKEN) {
            return Err(de::Error::custom("missing raw JSON capture token"));
        }
        let raw: String = map.next_value()?;
        if map.next_key::<String>()?.is_some() {
            return Err(de::Error::custom("extra raw JSON capture entry"));
        }
        raw_node(&raw, 0).map_err(de::Error::custom)
    }
    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(BufferVisitor)
    }
}

impl<'de> Deserialize<'de> for RetainedJsonInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_newtype_struct(RETAINED_NATIVE, codec::RetainedVisitor)
    }
}

struct BufferVisitor;
impl<'de> Visitor<'de> for BufferVisitor {
    type Value = RetainedJsonInput;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(RetainedJsonInput::Null)
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
        Ok(RetainedJsonInput::Bool(v))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(RetainedJsonInput::Text(v.encode_utf16().collect()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(RetainedJsonInput::Text(v.encode_utf16().collect()))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(RetainedJsonInput::Lexeme(v.to_string()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(RetainedJsonInput::Lexeme(v.to_string()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(RetainedJsonInput::Float(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut entries = Vec::new();
        while let Some(value) = seq.next_element()? {
            entries.push(value);
        }
        Ok(RetainedJsonInput::Array(entries))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut entries = Vec::new();
        while let Some((key, value)) = map.next_entry::<String, RetainedJsonInput>()? {
            entries.push((key.encode_utf16().collect(), value));
        }
        Ok(RetainedJsonInput::Object(entries))
    }
}

fn into_value(input: RetainedJsonInput) -> Result<Value, serde_json::Error> {
    Ok(match input {
        RetainedJsonInput::Undefined => {
            return Err(de::Error::custom("undefined has no standalone JSON value"));
        }
        RetainedJsonInput::Null => Value::Null,
        RetainedJsonInput::Bool(value) => Value::Bool(value),
        RetainedJsonInput::Text(value) => {
            Value::String(String::from_utf16(&value).map_err(de::Error::custom)?)
        }
        RetainedJsonInput::Lexeme(value) => serde_json::from_str(&value)?,
        RetainedJsonInput::Float(value) if value.is_nan() => {
            return Err(de::Error::custom("numeric passthrough refuses NaN JSON"));
        }
        // Content buffers retain bits, not spelling. Valid overflowing JSON
        // lexemes carry infinity until the shared JS wire projection runs.
        RetainedJsonInput::Float(value) if value.is_infinite() => {
            serde_json::from_str(if value.is_sign_negative() {
                "-1e400"
            } else {
                "1e400"
            })?
        }
        RetainedJsonInput::Float(value) => Value::from(value),
        RetainedJsonInput::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| {
                    if matches!(value, RetainedJsonInput::Undefined) {
                        Ok(Value::Null)
                    } else {
                        into_value(value)
                    }
                })
                .collect::<Result<_, _>>()?,
        ),
        RetainedJsonInput::Object(values) => Value::Object(
            values
                .into_iter()
                .filter(|(_, value)| !matches!(value, RetainedJsonInput::Undefined))
                .map(|(key, value)| {
                    Ok((
                        String::from_utf16(&key).map_err(de::Error::custom)?,
                        into_value(value)?,
                    ))
                })
                .collect::<Result<_, serde_json::Error>>()?,
        ),
    })
}

impl From<Value> for RetainedJsonInput {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(value) => Self::Bool(value),
            Value::String(value) => Self::Text(value.encode_utf16().collect()),
            Value::Number(value) => Self::Lexeme(value.to_string()),
            Value::Array(values) => Self::Array(values.into_iter().map(Self::from).collect()),
            Value::Object(values) => Self::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key.encode_utf16().collect(), Self::from(value)))
                    .collect(),
            ),
        }
    }
}
impl RetainedJsonInput {
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(values) => values
                .iter()
                .rev()
                .find(|(name, _)| name.iter().copied().eq(key.encode_utf16()))
                .map(|(_, value)| value),
            _ => None,
        }
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Self> {
        match self {
            Self::Object(values) => values
                .iter_mut()
                .rev()
                .find(|(units, _)| units.iter().copied().eq(key.encode_utf16()))
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&[(Vec<u16>, Self)]> {
        match self {
            Self::Object(values) => Some(values),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut Vec<(Vec<u16>, Self)>> {
        match self {
            Self::Object(values) => Some(values),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut [Self]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    pub fn is_nullish(&self) -> bool {
        matches!(self, Self::Null | Self::Undefined)
    }

    pub fn text_equals(&self, expected: &str) -> bool {
        self.as_text()
            .is_some_and(|units| units.iter().copied().eq(expected.encode_utf16()))
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(*value),
            Self::Lexeme(value) => value.parse().ok(),
            _ => None,
        }
    }
}
pub fn deserialize_present_retained_info<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<RetainedJsonInput>, D::Error> {
    RetainedJsonInput::deserialize(d).map(Some)
}

impl RetainedJsonInput {
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(value) => *value,
            Self::Text(value) => !value.is_empty(),
            Self::Float(value) => *value != 0.0 && !value.is_nan(),
            Self::Lexeme(value) => value
                .parse::<f64>()
                .is_ok_and(|value| value != 0.0 && !value.is_nan()),
            Self::Array(_) | Self::Object(_) => true,
        }
    }
}

impl RetainedJsonInput {
    pub fn as_text(&self) -> Option<&[u16]> {
        if let Self::Text(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }
    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::RetainedJsonInput;
    use serde_json::Value;

    #[test]
    fn retained_projection_array_preserves_every_element() {
        let input = RetainedJsonInput::Array(vec![
            RetainedJsonInput::Lexeme("11".into()),
            RetainedJsonInput::Lexeme("23".into()),
        ]);
        assert!(
            matches!(&input, RetainedJsonInput::Array(values) if values.len() == 2),
            "RETAINED-PROJECTION array source length"
        );
        let value = input.into_value().unwrap();
        let array = value.as_array().unwrap();
        assert_eq!(array.len(), 2, "RETAINED-PROJECTION array native length");
        let expected = [11, 23];
        let mut comparisons = 0;
        for (index, native) in array.iter().enumerate() {
            let expected = expected
                .get(index)
                .expect("RETAINED-PROJECTION array native index coverage");
            assert_eq!(
                native.as_i64().unwrap(),
                *expected,
                "RETAINED-PROJECTION array element identity"
            );
            comparisons += 1;
        }
        assert_eq!(comparisons, 2, "RETAINED-PROJECTION array comparisons");
    }

    #[test]
    fn lexical_parser_keeps_the_callers_starting_depth() {
        assert!(super::raw_node("1", 127).is_ok());
        assert!(super::raw_node("[1]", 127).is_err());
        assert!(super::raw_node("1", 128).is_err());
    }

    #[test]
    fn retained_projection_object_preserves_every_member() {
        let input = RetainedJsonInput::Object(vec![
            (
                "first".encode_utf16().collect(),
                RetainedJsonInput::Lexeme("11".into()),
            ),
            (
                "second".encode_utf16().collect(),
                RetainedJsonInput::Lexeme("23".into()),
            ),
        ]);
        assert!(
            matches!(&input, RetainedJsonInput::Object(values) if values.len() == 2),
            "RETAINED-PROJECTION object source length"
        );
        let value = input.into_value().unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 2, "RETAINED-PROJECTION object native length");
        let expected = [("first", 11), ("second", 23)];
        let mut comparisons = 0;
        for (key, native) in object {
            let (_, expected) = expected
                .iter()
                .find(|(name, _)| *name == key)
                .expect("RETAINED-PROJECTION object native key coverage");
            assert_eq!(
                native.as_i64().unwrap(),
                *expected,
                "RETAINED-PROJECTION object member identity"
            );
            comparisons += 1;
        }
        assert_eq!(comparisons, 2, "RETAINED-PROJECTION object comparisons");
    }

    #[test]
    fn retained_array_cardinality_is_independent_of_decoder_output() {
        struct Sequence(std::array::IntoIter<&'static str, 3>);
        impl<'de> serde::de::SeqAccess<'de> for Sequence {
            type Error = serde_json::Error;
            fn next_element_seed<T: serde::de::DeserializeSeed<'de>>(
                &mut self,
                seed: T,
            ) -> Result<Option<T::Value>, Self::Error> {
                self.0
                    .next()
                    .map(|text| seed.deserialize(&mut serde_json::Deserializer::from_str(text)))
                    .transpose()
            }
        }
        let input = serde::de::Visitor::visit_seq(
            super::BufferVisitor,
            Sequence(["11", "23", "37"].into_iter()),
        )
        .unwrap();
        let RetainedJsonInput::Array(entries) = &input else {
            panic!("RETAINED-CARDINALITY array shape");
        };
        assert_eq!(entries.len(), 3, "RETAINED-CARDINALITY captured length");
        let value = input.into_value().unwrap();
        let array = value.as_array().unwrap();
        assert_eq!(array.len(), 3, "RETAINED-CARDINALITY projected length");
        let mut comparisons = 0;
        for (native, expected) in array.iter().zip([11, 23, 37]) {
            assert_eq!(native.as_i64().unwrap(), expected);
            comparisons += 1;
        }
        assert_eq!(comparisons, 3, "RETAINED-CARDINALITY executed comparisons");
    }

    #[test]
    fn extracted_capture_preserves_numeric_lexemes_and_nested_order() {
        for text in ["-0", "-0.0", "1e400", "-1e400", "-1e-400", "1.5", "1e3"] {
            let raw = format!(r#"{{"z":[{text}],"a":true}}"#);
            let input: RetainedJsonInput = serde_json::from_str(&raw).unwrap();
            let value = input.into_value().unwrap();
            assert_eq!(
                value["z"].as_array().unwrap().len(),
                1,
                "RETAINED-CARDINALITY native nested array length"
            );
            let numeric = value["z"][0].as_number().unwrap().to_string();
            assert_eq!(
                numeric.parse::<f64>().unwrap().to_bits(),
                text.parse::<f64>().unwrap().to_bits(),
                "retained numeric bits {text}"
            );
            assert_eq!(
                value
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["z", "a"]
            );
            let owned = RetainedJsonInput::from_value(value.clone())
                .unwrap()
                .into_value()
                .unwrap();
            assert_eq!(owned, value);
        }
        let raw = r#"{"$serde_json::private::Number":"1e400"}"#;
        let value = serde_json::from_str::<RetainedJsonInput>(raw)
            .unwrap()
            .into_value()
            .unwrap();
        assert!(
            value.is_object(),
            "private-looking own key remains an object"
        );
        assert!(
            RetainedJsonInput::Float(f64::NAN).into_value().is_err(),
            "retained JSON refuses NaN"
        );
    }

    #[test]
    fn info_presence_preserves_explicit_null() {
        #[derive(serde::Deserialize)]
        struct Info {
            #[serde(default, deserialize_with = "super::deserialize_present_retained_json")]
            info: Option<Value>,
        }
        assert!(serde_json::from_str::<Info>("{}").unwrap().info.is_none());
        let ordinary = serde_json::json!({"info":{"price":1.5}});
        assert_eq!(
            serde_json::from_value::<Info>(ordinary.clone())
                .unwrap()
                .info,
            Some(ordinary["info"].clone()),
            "ordinary existing Value info unchanged"
        );
        assert_eq!(
            serde_json::from_str::<Info>(r#"{"info":null}"#)
                .unwrap()
                .info,
            Some(Value::Null)
        );
    }
}
