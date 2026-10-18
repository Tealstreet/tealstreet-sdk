use super::{RetainedJsonInput as Input, UTF16_UNITS};
use serde::de::{self, Deserializer, IntoDeserializer, Visitor};
use serde_json::Error;

pub(super) enum Payload {
    Node(Input),
    Value(Input),
    Units(Vec<u16>),
    Entry(Vec<u16>, Input),
    Tag(&'static str),
}

impl<'de> IntoDeserializer<'de, Error> for Payload {
    type Deserializer = Self;
    fn into_deserializer(self) -> Self {
        self
    }
}

impl<'de> Deserializer<'de> for Payload {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Self::Node(value) => {
                let tag = match &value {
                    Input::Undefined => "Undefined",
                    Input::Null => "Null",
                    Input::Bool(_) => "Bool",
                    Input::Text(_) => "Text",
                    Input::Lexeme(_) => "Lexeme",
                    Input::Float(_) => "Float",
                    Input::Array(_) => "Array",
                    Input::Object(_) => "Object",
                };
                de::value::SeqDeserializer::new([Self::Tag(tag), Self::Value(value)].into_iter())
                    .deserialize_any(visitor)
            }
            Self::Tag(tag) => visitor.visit_borrowed_str(tag),
            Self::Units(units) | Self::Value(Input::Text(units)) => {
                de::value::SeqDeserializer::new(units.into_iter()).deserialize_any(visitor)
            }
            Self::Entry(key, value) => {
                de::value::SeqDeserializer::new([Self::Units(key), Self::Node(value)].into_iter())
                    .deserialize_any(visitor)
            }
            Self::Value(Input::Undefined | Input::Null) => visitor.visit_unit(),
            Self::Value(Input::Bool(value)) => visitor.visit_bool(value),
            Self::Value(Input::Float(value)) => visitor.visit_f64(value),
            Self::Value(Input::Lexeme(value)) => visitor.visit_string(value),
            Self::Value(Input::Array(values)) => {
                de::value::SeqDeserializer::new(values.into_iter().map(Self::Node))
                    .deserialize_any(visitor)
            }
            Self::Value(Input::Object(values)) => de::value::SeqDeserializer::new(
                values
                    .into_iter()
                    .map(|(key, value)| Self::Entry(key, value)),
            )
            .deserialize_any(visitor),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if matches!(self, Self::Value(Input::Undefined | Input::Null)) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        if _name == UTF16_UNITS
            && let Self::Units(units) | Self::Value(Input::Text(units)) = self
        {
            return visit_units(&units, visitor);
        }
        visitor.visit_newtype_struct(self)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct map struct enum identifier ignored_any
    }
}

fn visit_units<'de, V: Visitor<'de>>(units: &[u16], visitor: V) -> Result<V::Value, Error> {
    // SAFETY: initialized u16 storage has no padding and u8 alignment is 1.
    // The owner lives through visit_bytes; the decoder reads native-endian pairs.
    let bytes = unsafe {
        std::slice::from_raw_parts(units.as_ptr().cast::<u8>(), std::mem::size_of_val(units))
    };
    visitor.visit_bytes(bytes)
}
