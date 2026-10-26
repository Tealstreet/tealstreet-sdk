use super::{BufferVisitor, Capture, RAW_TOKEN, RetainedJsonInput as Input, UTF16_UNITS};
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

enum Tag {
    Undefined,
    Null,
    Bool,
    Text,
    Lexeme,
    Float,
    Array,
    Object,
}
impl<'de> serde::Deserialize<'de> for Tag {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct TagVisitor;
        impl Visitor<'_> for TagVisitor {
            type Value = Tag;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Tag, E> {
                match value {
                    "Undefined" => Ok(Tag::Undefined),
                    "Null" => Ok(Tag::Null),
                    "Bool" => Ok(Tag::Bool),
                    "Text" => Ok(Tag::Text),
                    "Lexeme" => Ok(Tag::Lexeme),
                    "Float" => Ok(Tag::Float),
                    "Array" => Ok(Tag::Array),
                    "Object" => Ok(Tag::Object),
                    _ => Err(E::custom("invalid retained tag")),
                }
            }
            fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Tag, E> {
                let text = std::str::from_utf8(bytes)
                    .map_err(|_| E::invalid_value(de::Unexpected::Bytes(bytes), &self))?;
                self.visit_str(text)
            }
            fn visit_byte_buf<E: de::Error>(self, bytes: Vec<u8>) -> Result<Tag, E> {
                self.visit_bytes(&bytes)
            }
        }
        d.deserialize_string(TagVisitor)
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
            let tag: Tag = seq
                .next_element()?
                .ok_or_else(|| de::Error::custom("missing retained tag"))?;
            let value = match tag {
                Tag::Undefined => {
                    seq.next_element::<()>()?
                        .ok_or_else(|| de::Error::custom("missing retained payload"))?;
                    Input::Undefined
                }
                Tag::Null => {
                    seq.next_element::<()>()?
                        .ok_or_else(|| de::Error::custom("missing retained payload"))?;
                    Input::Null
                }
                Tag::Bool => Input::Bool(
                    seq.next_element::<bool>()?
                        .ok_or_else(|| de::Error::custom("missing retained payload"))?,
                ),
                Tag::Text => Input::Text(
                    seq.next_element::<Units>()?
                        .ok_or_else(|| de::Error::custom("missing retained payload"))?
                        .0,
                ),
                Tag::Lexeme => Input::Lexeme(
                    seq.next_element::<String>()?
                        .ok_or_else(|| de::Error::custom("missing retained payload"))?,
                ),
                Tag::Float => Input::Float(
                    seq.next_element::<f64>()?
                        .ok_or_else(|| de::Error::custom("missing retained payload"))?,
                ),
                Tag::Array => Input::Array(
                    seq.next_element::<Vec<Payload>>()?
                        .ok_or_else(|| de::Error::custom("missing retained payload"))?
                        .into_iter()
                        .map(|value| value.0)
                        .collect(),
                ),
                Tag::Object => Input::Object(
                    seq.next_element::<Vec<(Units, Payload)>>()?
                        .ok_or_else(|| de::Error::custom("missing retained payload"))?
                        .into_iter()
                        .map(|(key, value)| (key.0, value.0))
                        .collect(),
                ),
            };
            if seq.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom("extra retained payload"));
            }
            Ok(value)
        }
    }
    d.deserialize_seq(PayloadVisitor)
}
struct Unit(u16);
impl<'de> serde::Deserialize<'de> for Unit {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct UnitVisitor;
        impl Visitor<'_> for UnitVisitor {
            type Value = Unit;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a UTF-16 code unit")
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Unit, E> {
                u16::try_from(value)
                    .map(Unit)
                    .map_err(|_| E::custom("invalid retained UTF-16 unit"))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Unit, E> {
                u16::try_from(value)
                    .map(Unit)
                    .map_err(|_| E::custom("invalid retained UTF-16 unit"))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Unit, E> {
                if value.is_finite() && value.fract() == 0.0 && (0.0..=65535.0).contains(&value) {
                    Ok(Unit(value as u16))
                } else {
                    Err(E::custom("invalid retained UTF-16 unit"))
                }
            }
        }
        d.deserialize_f64(UnitVisitor)
    }
}
struct Units(Vec<u16>);
impl<'de> serde::Deserialize<'de> for Units {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct UnitsVisitor;
        impl<'de> Visitor<'de> for UnitsVisitor {
            type Value = Units;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("UTF-16 code units")
            }
            fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Units, E> {
                let (pairs, remainder) = bytes.as_chunks::<2>();
                if !remainder.is_empty() {
                    return Err(E::custom("invalid retained UTF-16 bytes"));
                }
                Ok(Units(
                    pairs.iter().copied().map(u16::from_ne_bytes).collect(),
                ))
            }
            fn visit_byte_buf<E: de::Error>(self, bytes: Vec<u8>) -> Result<Units, E> {
                self.visit_bytes(&bytes)
            }
            fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<Units, D::Error> {
                serde::Deserialize::deserialize(d)
                    .map(|values: Vec<Unit>| Units(values.into_iter().map(|unit| unit.0).collect()))
            }
        }
        d.deserialize_newtype_struct(UTF16_UNITS, UnitsVisitor)
    }
}

struct NativePayload<'a>(&'a Input);
impl Serialize for NativePayload<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(2))?;
        let tag = match self.0 {
            Input::Undefined => "Undefined",
            Input::Null => "Null",
            Input::Bool(_) => "Bool",
            Input::Text(_) => "Text",
            Input::Lexeme(_) => "Lexeme",
            Input::Float(_) => "Float",
            Input::Array(_) => "Array",
            Input::Object(_) => "Object",
        };
        seq.serialize_element(tag)?;
        match self.0 {
            Input::Undefined | Input::Null => seq.serialize_element(&())?,
            Input::Bool(value) => seq.serialize_element(value)?,
            Input::Text(value) => seq.serialize_element(value)?,
            Input::Lexeme(value) => seq.serialize_element(value)?,
            Input::Float(value) => seq.serialize_element(value)?,
            Input::Array(values) => {
                seq.serialize_element(&values.iter().map(NativePayload).collect::<Vec<_>>())?
            }
            Input::Object(values) => seq.serialize_element(
                &values
                    .iter()
                    .map(|(key, value)| (key, NativePayload(value)))
                    .collect::<Vec<_>>(),
            )?,
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
    pub fn into_native_payload<'de>(self) -> impl Deserializer<'de, Error = serde_json::Error> {
        super::payload::Payload::Node(self)
    }
}
