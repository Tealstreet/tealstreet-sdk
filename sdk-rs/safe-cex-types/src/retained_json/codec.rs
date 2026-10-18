use super::{BufferVisitor, Capture, RAW_TOKEN, RetainedJsonInput as Input};
use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeSeq, Serializer};
use std::fmt;

pub const RETAINED_NATIVE: &str = "$safe_cex::RetainedJsonInput";

pub(super) struct RetainedVisitor;
impl<'de> Visitor<'de> for RetainedVisitor {
    type Value = Input;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("retained JSON or native data")
    }
    fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<Input, D::Error> {
        d.deserialize_newtype_struct(RAW_TOKEN, Capture)
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Input, E> {
        BufferVisitor.visit_f64(value)
    }
    fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Input, A::Error> {
        Capture.visit_map(map)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Input, A::Error> {
        deserialize_native_payload(de::value::SeqAccessDeserializer::new(seq))
    }
}

struct Payload(Input);
impl<'de> serde::Deserialize<'de> for Payload {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        deserialize_native_payload(d).map(Self)
    }
}

pub fn deserialize_native_payload<'de, D: Deserializer<'de>>(d: D) -> Result<Input, D::Error> {
    struct PayloadVisitor;
    impl<'de> Visitor<'de> for PayloadVisitor {
        type Value = Input;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a retained native payload")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Input, A::Error> {
            let tag: String = seq
                .next_element()?
                .ok_or_else(|| de::Error::custom("missing retained tag"))?;
            macro_rules! next {
                ($ty:ty) => {
                    seq.next_element::<$ty>()?
                        .ok_or_else(|| de::Error::custom("missing retained payload"))?
                };
            }
            let value = match tag.as_str() {
                "Undefined" => {
                    next!(());
                    Input::Undefined
                }
                "Null" => {
                    next!(());
                    Input::Null
                }
                "Bool" => Input::Bool(next!(bool)),
                "Text" => Input::Text(units(next!(Vec<f64>))?),
                "Lexeme" => Input::Lexeme(next!(String)),
                "Float" => Input::Float(next!(f64)),
                "Array" => Input::Array(
                    next!(Vec<Payload>)
                        .into_iter()
                        .map(|value| value.0)
                        .collect(),
                ),
                "Object" => Input::Object(
                    next!(Vec<(Vec<f64>, Payload)>)
                        .into_iter()
                        .map(|(key, value)| Ok((units(key)?, value.0)))
                        .collect::<Result<_, A::Error>>()?,
                ),
                _ => return Err(de::Error::custom("invalid retained tag")),
            };
            if seq.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom("extra retained payload"));
            }
            Ok(value)
        }
    }
    d.deserialize_seq(PayloadVisitor)
}
fn units<E: de::Error>(values: Vec<f64>) -> Result<Vec<u16>, E> {
    values
        .into_iter()
        .map(|value| {
            if value.is_finite() && value.fract() == 0.0 && (0.0..=65535.0).contains(&value) {
                Ok(value as u16)
            } else {
                Err(de::Error::custom("invalid retained UTF-16 unit"))
            }
        })
        .collect()
}

struct NativePayload<'a>(&'a Input);
impl Serialize for NativePayload<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(2))?;
        macro_rules! payload {
            ($tag:literal, $value:expr) => {{
                seq.serialize_element($tag)?;
                seq.serialize_element($value)?;
            }};
        }
        match self.0 {
            Input::Undefined => payload!("Undefined", &()),
            Input::Null => payload!("Null", &()),
            Input::Bool(value) => payload!("Bool", value),
            Input::Text(value) => payload!("Text", value),
            Input::Lexeme(value) => payload!("Lexeme", value),
            Input::Float(value) => payload!("Float", value),
            Input::Array(values) => payload!(
                "Array",
                &values.iter().map(NativePayload).collect::<Vec<_>>()
            ),
            Input::Object(values) => payload!(
                "Object",
                &values
                    .iter()
                    .map(|(key, value)| (key, NativePayload(value)))
                    .collect::<Vec<_>>()
            ),
        }
        seq.end()
    }
}
struct NativeOrJson<'a>(&'a Input);
impl Serialize for NativeOrJson<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            let text = self
                .0
                .json_stringify()
                .map_err(serde::ser::Error::custom)?
                .unwrap_or_else(|| "null".into());
            serde_json::value::RawValue::from_string(text)
                .map_err(serde::ser::Error::custom)?
                .serialize(s)
        } else {
            NativePayload(self.0).serialize(s)
        }
    }
}
impl Serialize for Input {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_newtype_struct(RETAINED_NATIVE, &NativeOrJson(self))
    }
}

impl Input {
    pub fn into_native_payload(self) -> Self {
        let (tag, value) = match self {
            Self::Undefined => ("Undefined", Self::Null),
            Self::Null => ("Null", Self::Null),
            Self::Bool(value) => ("Bool", Self::Bool(value)),
            Self::Text(value) => (
                "Text",
                Self::Array(
                    value
                        .into_iter()
                        .map(|unit| Self::Float(f64::from(unit)))
                        .collect(),
                ),
            ),
            Self::Lexeme(value) => ("Lexeme", Self::Text(value.encode_utf16().collect())),
            Self::Float(value) => ("Float", Self::Float(value)),
            Self::Array(values) => (
                "Array",
                Self::Array(values.into_iter().map(Self::into_native_payload).collect()),
            ),
            Self::Object(values) => (
                "Object",
                Self::Array(
                    values
                        .into_iter()
                        .map(|(key, value)| {
                            Self::Array(vec![
                                Self::Array(
                                    key.into_iter()
                                        .map(|unit| Self::Float(f64::from(unit)))
                                        .collect(),
                                ),
                                value.into_native_payload(),
                            ])
                        })
                        .collect(),
                ),
            ),
        };
        Self::Array(vec![Self::Text(tag.encode_utf16().collect()), value])
    }
}
