use super::RetainedJsonInput as Input;
use serde::Deserializer;
use serde::de::{self, IntoDeserializer, Visitor};
use serde_json::Error;

impl Input {
    pub fn decode_native<T: de::DeserializeOwned>(self, present_fields: bool) -> Result<T, Error> {
        let mode = if present_fields {
            NativeMode::Preserve
        } else {
            NativeMode::Normal
        };
        T::deserialize(NativeInput(self, mode))
    }
}

#[derive(Clone, Copy, PartialEq)]
enum NativeMode {
    Normal,
    Preserve,
    ModelRecord,
    InfoRecord,
    StoreRecord,
    Key,
}
impl NativeMode {
    fn child(self) -> Self {
        if matches!(
            self,
            Self::ModelRecord | Self::InfoRecord | Self::StoreRecord
        ) {
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
            // Only the typed String view is well-formed. Retained native fields
            // take the newtype/payload branch below and keep the original units.
            Input::Text(value) => visitor.visit_string(String::from_utf16_lossy(&value)),
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
            Input::Lexeme(value) => visitor.visit_f64(value.parse().map_err(de::Error::custom)?),
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
                            || (self.1 == NativeMode::StoreRecord
                                && key.iter().copied().eq("zkLinkConfig".encode_utf16()))
                    })
                    .map(|(key, value)| {
                        (
                            NativeInput(Input::Text(key), NativeMode::Key),
                            NativeInput(value, self.1.child()),
                        )
                    }),
            )
            .deserialize_any(visitor),
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if self.1 == NativeMode::Key {
            return self.deserialize_any(visitor);
        }
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
        if self.1 == NativeMode::Key {
            return self.deserialize_any(visitor);
        }
        if name == crate::retained_json::RETAINED_NATIVE {
            return self.0.into_native_payload().deserialize_seq(visitor);
        }
        if name == crate::model_field::MODEL_FIELD && matches!(self.0, Input::Undefined) {
            return visitor.visit_unit();
        }
        if name == crate::model_field::MODEL_RECORD {
            self.1 = NativeMode::ModelRecord;
        }
        if name == crate::model_field::INFO_RECORD {
            self.1 = NativeMode::InfoRecord;
        }
        if name == crate::STORE_RECORD {
            self.1 = NativeMode::StoreRecord;
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
            Input::Text(value) => {
                visitor.visit_enum(String::from_utf16_lossy(&value).into_deserializer())
            }
            Input::Object(values) if values.len() == 1 => de::value::MapAccessDeserializer::new(
                de::value::MapDeserializer::new(values.into_iter().map(|(key, value)| {
                    (
                        NativeInput(Input::Text(key), NativeMode::Key),
                        NativeInput(value, self.1.child()),
                    )
                })),
            )
            .deserialize_enum(name, variants, visitor),
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
    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Input::Text(units) = &self.0
            && units.len() <= 64
            && units.iter().all(|unit| *unit < 128)
        {
            let mut bytes = [0u8; 64];
            for (byte, unit) in bytes.iter_mut().zip(units) {
                *byte = *unit as u8;
            }
            return visitor
                .visit_str(std::str::from_utf8(&bytes[..units.len()]).expect("ASCII identifier"));
        }
        self.deserialize_any(visitor)
    }
    serde::forward_to_deserialize_any! {
        bool f32 f64 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct map struct ignored_any
    }
}
