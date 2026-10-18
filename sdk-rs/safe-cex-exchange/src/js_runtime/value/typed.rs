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

pub(super) fn from_input(value: Input) -> Js {
    match value {
        Input::Undefined => Js::Undefined,
        Input::Null => Js::Null,
        Input::Bool(value) => Js::Bool(value),
        Input::Text(value) => Js::utf16(&value),
        Input::Float(value) => Js::Number(value),
        Input::Lexeme(value) => Js::Number(value.parse().expect("captured numeric lexeme")),
        Input::Array(values) => Js::array(values.into_iter().map(from_input).collect()),
        Input::Object(values) => {
            let object = Js::object(vec![]);
            for (key, value) in values {
                object
                    .define_key(&key, from_input(value))
                    .expect("fresh object data property");
            }
            object
        }
    }
}

pub(super) fn capture(
    value: &Js,
    depth: usize,
    present_fields: bool,
    data_only: bool,
) -> Result<Input, Thrown> {
    if depth >= 128 {
        return Err(Thrown::type_error("typed value recursion limit exceeded"));
    }
    if present_fields && value.is_callable() {
        return Err(Thrown::type_error("callable cannot enter a declared DTO"));
    }
    Ok(match value {
        Js::SymbolIterator => return Err(fault("Cannot decode a Symbol as typed data")),
        Js::Undefined => Input::Undefined,
        Js::Null | Js::Function(_) | Js::Promise(_) => Input::Null,
        Js::Bool(value) => Input::Bool(*value),
        Js::Number(value) => Input::Float(*value),
        Js::BigInt(_) if data_only => {
            return Err(Thrown::type_error("Do not know how to serialize a BigInt"));
        }
        Js::BigInt(value) => Input::Text(value.to_string().encode_utf16().collect()),
        Js::String(value) => Input::Text(value.to_vec()),
        Js::Array(values) => Input::Array(
            values
                .borrow()
                .iter()
                .map(|value| {
                    value.as_ref().map_or(Ok(Input::Null), |value| {
                        capture(value, depth + 1, present_fields, data_only)
                    })
                })
                .collect::<Result<_, _>>()?,
        ),
        Js::Builtin(reference) if reference.is_callable() => Input::Null,
        Js::Object(_) | Js::Builtin(_) => {
            let mut fields = vec![];
            for (key, value) in value.own_entries()? {
                if !present_fields && (matches!(value, Js::Function(_)) || value.is_callable()) {
                    continue;
                }
                fields.push((
                    key.to_vec(),
                    capture(&value, depth + 1, present_fields, data_only)?,
                ));
            }
            Input::Object(fields)
        }
        Js::Map(_) | Js::Set(_) => Input::Object(vec![]),
    })
}

pub(super) fn decode<T: de::DeserializeOwned>(
    value: &Js,
    present_fields: bool,
) -> Result<T, Thrown> {
    let mode = if present_fields {
        NativeMode::Preserve
    } else {
        NativeMode::Normal
    };
    T::deserialize(NativeInput(capture(value, 0, present_fields, false)?, mode)).map_err(fault)
}

#[derive(Clone, Copy, PartialEq)]
enum NativeMode {
    Normal,
    Preserve,
    ModelRecord,
    InfoRecord,
}
impl NativeMode {
    fn child(self) -> Self {
        if matches!(self, Self::ModelRecord | Self::InfoRecord) {
            Self::Normal
        } else {
            self
        }
    }
}
struct NativeInput(Input, NativeMode);
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
            Input::Undefined | Input::Null => visitor.visit_unit(),
            Input::Bool(value) => visitor.visit_bool(value),
            Input::Text(value) => {
                visitor.visit_string(String::from_utf16(&value).map_err(de::Error::custom)?)
            }
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
            Input::Array(values) => de::value::SeqDeserializer::new(
                values
                    .into_iter()
                    .map(|value| NativeInput(value, self.1.child())),
            )
            .deserialize_any(visitor),
            Input::Object(values) => de::value::MapDeserializer::new(
                values
                    .into_iter()
                    .filter(|(key, value)| {
                        !matches!(value, Input::Undefined)
                            || matches!(self.1, NativeMode::Preserve | NativeMode::ModelRecord)
                            || (self.1 == NativeMode::InfoRecord
                                && key.iter().copied().eq("info".encode_utf16()))
                    })
                    .map(|(key, value)| {
                        Ok((
                            String::from_utf16(&key).map_err(de::Error::custom)?,
                            NativeInput(value, self.1.child()),
                        ))
                    })
                    .collect::<Result<Vec<_>, Error>>()?
                    .into_iter(),
            )
            .deserialize_any(visitor),
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if matches!(self.0, Input::Null | Input::Undefined) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        mut self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        if name == safe_cex_types::retained_json::RETAINED_NATIVE {
            return NativeInput(self.0.into_native_payload(), NativeMode::Preserve)
                .deserialize_seq(visitor);
        }
        if name == safe_cex_types::model_field::MODEL_FIELD && matches!(self.0, Input::Undefined) {
            return visitor.visit_unit();
        }
        if name == safe_cex_types::model_field::MODEL_RECORD {
            self.1 = NativeMode::ModelRecord;
        }
        if name == safe_cex_types::model_field::INFO_RECORD {
            self.1 = NativeMode::InfoRecord;
        }
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            Input::Text(value) => visitor.visit_enum(
                String::from_utf16(&value)
                    .map_err(de::Error::custom)?
                    .into_deserializer(),
            ),
            Input::Object(values) if values.len() == 1 => {
                de::value::MapAccessDeserializer::new(de::value::MapDeserializer::new(
                    values
                        .into_iter()
                        .map(|(key, value)| {
                            Ok((
                                String::from_utf16(&key).map_err(de::Error::custom)?,
                                NativeInput(value, self.1.child()),
                            ))
                        })
                        .collect::<Result<Vec<_>, Error>>()?
                        .into_iter(),
                ))
                .deserialize_enum(name, variants, visitor)
            }
            _ => Err(de::Error::custom(
                "expected string or externally tagged enum object",
            )),
        }
    }
    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Input::Float(value) = self.0
            && value.is_finite()
            && value.fract() == 0.0
            && (-128.0..128.0).contains(&value)
        {
            return visitor.visit_i8(value as i8);
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Input::Float(value) = self.0
            && value.is_finite()
            && value.fract() == 0.0
            && (-32768.0..32768.0).contains(&value)
        {
            return visitor.visit_i16(value as i16);
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Input::Float(value) = self.0
            && value.is_finite()
            && value.fract() == 0.0
            && (-2147483648.0..2147483648.0).contains(&value)
        {
            return visitor.visit_i32(value as i32);
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Input::Float(value) = self.0
            && value.is_finite()
            && value.fract() == 0.0
            && (-2.0f64.powi(63)..2.0f64.powi(63)).contains(&value)
        {
            return visitor.visit_i64(value as i64);
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_i128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Input::Float(value) = self.0
            && value.is_finite()
            && value.fract() == 0.0
            && (-2.0f64.powi(127)..2.0f64.powi(127)).contains(&value)
        {
            return visitor.visit_i128(value as i128);
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Input::Float(value) = self.0
            && value.is_finite()
            && value.fract() == 0.0
            && (0.0..256.0).contains(&value)
        {
            return visitor.visit_u8(value as u8);
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Input::Float(value) = self.0
            && value.is_finite()
            && value.fract() == 0.0
            && (0.0..65536.0).contains(&value)
        {
            return visitor.visit_u16(value as u16);
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Input::Float(value) = self.0
            && value.is_finite()
            && value.fract() == 0.0
            && (0.0..4294967296.0).contains(&value)
        {
            return visitor.visit_u32(value as u32);
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Input::Float(value) = self.0
            && value.is_finite()
            && value.fract() == 0.0
            && (0.0..2.0f64.powi(64)).contains(&value)
        {
            return visitor.visit_u64(value as u64);
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Input::Float(value) = self.0
            && value.is_finite()
            && value.fract() == 0.0
            && (0.0..2.0f64.powi(128)).contains(&value)
        {
            return visitor.visit_u128(value as u128);
        }
        self.deserialize_any(visitor)
    }
    serde::forward_to_deserialize_any! {
        bool f32 f64 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}
