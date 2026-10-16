//! Serde's native data model at the typed boundary, before JSON wire projection.
use super::{Js, Thrown};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde::Deserializer;
use serde::de::{self, IntoDeserializer, Visitor};
use serde_json::Error;

fn fault(error: impl std::fmt::Display) -> Thrown {
    Thrown::type_error(&error.to_string())
}

pub(super) fn encode<T: serde::Serialize + ?Sized>(value: &T) -> Result<Js, Thrown> {
    safe_cex_num::numeric_ingress::js_capture_output(value)
        .map(from_input)
        .map_err(fault)
}

fn from_input(value: Input) -> Js {
    match value {
        Input::Null => Js::Null,
        Input::Bool(value) => Js::Bool(value),
        Input::Text(value) => Js::str(&value),
        Input::Float(value) => Js::Number(value),
        Input::Lexeme(value) => Js::Number(value.parse().expect("captured numeric lexeme")),
        Input::Array(values) => Js::array(values.into_iter().map(from_input).collect()),
        Input::Object(values) => {
            let object = Js::object(vec![]);
            for (key, value) in values {
                object
                    .define(&key, from_input(value))
                    .expect("fresh object data property");
            }
            object
        }
    }
}

fn capture(value: &Js, depth: usize) -> Result<Input, Thrown> {
    if depth >= 128 {
        return Err(Thrown::type_error("typed value recursion limit exceeded"));
    }
    Ok(match value {
        Js::Undefined | Js::Null | Js::Function(_) | Js::Promise(_) => Input::Null,
        Js::Bool(value) => Input::Bool(*value),
        Js::Number(value) => Input::Float(*value),
        Js::BigInt(value) => Input::Text(value.to_string()),
        Js::String(value) => Input::Text(
            String::from_utf16(value)
                .map_err(|_| Thrown::type_error("lone UTF-16 surrogate at a typed boundary"))?,
        ),
        Js::Array(values) => Input::Array(
            values
                .borrow()
                .iter()
                .map(|value| {
                    value
                        .as_ref()
                        .map_or(Ok(Input::Null), |value| capture(value, depth + 1))
                })
                .collect::<Result<_, _>>()?,
        ),
        Js::Object(_) => {
            let mut fields = vec![];
            for (key, value) in value.own_entries()? {
                if matches!(value, Js::Undefined | Js::Function(_)) {
                    continue;
                }
                fields.push((String::from_utf16_lossy(&key), capture(&value, depth + 1)?));
            }
            Input::Object(fields)
        }
        Js::Map(_) | Js::Set(_) => Input::Object(vec![]),
    })
}

pub(super) fn decode<T: de::DeserializeOwned>(value: &Js) -> Result<T, Thrown> {
    T::deserialize(NativeInput(capture(value, 0)?)).map_err(fault)
}

struct NativeInput(Input);
impl<'de> IntoDeserializer<'de, Error> for NativeInput {
    type Deserializer = Self;
    fn into_deserializer(self) -> Self {
        self
    }
}
impl<'de> Deserializer<'de> for NativeInput {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Input::Null => visitor.visit_unit(),
            Input::Bool(value) => visitor.visit_bool(value),
            Input::Text(value) => visitor.visit_string(value),
            Input::Float(value) => {
                if !value.is_finite()
                    && std::any::type_name::<V::Value>()
                        == std::any::type_name::<serde_json::Value>()
                {
                    return Err(de::Error::custom(
                        "nonfinite native Number cannot enter serde_json::Value",
                    ));
                }
                visitor.visit_f64(value)
            }
            Input::Lexeme(_) => unreachable!("native input capture uses numeric visitor values"),
            Input::Array(values) => {
                de::value::SeqDeserializer::new(values.into_iter().map(NativeInput))
                    .deserialize_any(visitor)
            }
            Input::Object(values) => de::value::MapDeserializer::new(
                values
                    .into_iter()
                    .map(|(key, value)| (key, NativeInput(value))),
            )
            .deserialize_any(visitor),
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if matches!(self.0, Input::Null) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            Input::Text(value) => visitor.visit_enum(value.into_deserializer()),
            Input::Object(values) if values.len() == 1 => {
                de::value::MapAccessDeserializer::new(de::value::MapDeserializer::new(
                    values
                        .into_iter()
                        .map(|(key, value)| (key, NativeInput(value))),
                ))
                .deserialize_enum(name, variants, visitor)
            }
            _ => Err(de::Error::custom(
                "expected string or externally tagged enum object",
            )),
        }
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}
