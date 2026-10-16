//! Decode native class arguments into declared DTO fields without a JSON hop.
use crate::private_event_value::{EventValue, PrivateEnum};
use serde::de::{self, IntoDeserializer, Visitor};
pub fn decode<T: de::DeserializeOwned>(value: EventValue) -> Result<T, de::value::Error> {
    T::deserialize(Native(value))
}
struct Native(EventValue);
impl<'de> de::Deserializer<'de> for Native {
    type Error = de::value::Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.0 {
            EventValue::Undefined | EventValue::Null => visitor.visit_unit(),
            EventValue::Bool(v) => visitor.visit_bool(v),
            EventValue::Number(v) => visitor.visit_f64(v),
            EventValue::String(v) => visitor.visit_string(v),
            EventValue::Enum(v) => {
                visitor.visit_string(v.wire().as_str().expect("categorical enum").into())
            }
            EventValue::Array(v) => visitor.visit_seq(de::value::SeqDeserializer::new(
                v.borrow().clone().into_iter().map(Native),
            )),
            EventValue::Object(v) => visitor.visit_map(de::value::MapDeserializer::new(
                v.borrow()
                    .fields
                    .iter()
                    .map(|(k, v)| (k.clone(), Native(v.clone())))
                    .collect::<Vec<_>>()
                    .into_iter(),
            )),
            EventValue::Builtin(reference) if !reference.is_callable() => {
                visitor.visit_map(de::value::MapDeserializer::new(
                    reference.own_fields().into_iter().map(|(key, value)| {
                        (key, Native(EventValue::from_prototype(value.into())))
                    }),
                ))
            }
            EventValue::Builtin(_) | EventValue::Inherited(_) => Err(de::Error::custom(
                "inherited function/prototype cannot enter a declared DTO",
            )),
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        if self.0.nullish() {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let text = match self.0 {
            EventValue::String(v) => v,
            EventValue::Enum(v) => v.wire().as_str().expect("categorical enum").into(),
            _ => {
                return Err(de::Error::custom(
                    "declared enum requires a categorical string",
                ));
            }
        };
        visitor.visit_enum(text.into_deserializer())
    }
    serde::forward_to_deserialize_any! {bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier ignored_any}
}
impl<'de> IntoDeserializer<'de, de::value::Error> for Native {
    type Deserializer = Self;
    fn into_deserializer(self) -> Self {
        self
    }
}
// Keep categorical native enum type explicit in this adapter's documentation.
const _: Option<PrivateEnum> = None;
