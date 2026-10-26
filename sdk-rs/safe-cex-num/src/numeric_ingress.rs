//! JSON numeric ingress before Serde's tagged, flattened or explicit buffers.
//!
//! Capture at the outer boundary: a decoder cannot recover a sign erased before
//! it is called. Explicit intermediate buffers must use `JsNumberInput`, not
//! `serde_json::Value`, whose float visitor projects infinity to null.
use serde::de::{
    self, DeserializeOwned, DeserializeSeed, IntoDeserializer, MapAccess, SeqAccess, Visitor,
};
use serde::{Deserialize, Deserializer, Serialize};
mod eligibility;
mod output;
mod passthrough;
pub use passthrough::{JsNumberPassthrough, js_deserialize_passthrough};
mod position_record_output;
mod shared_output;
pub use eligibility::JsNumberOutput;
use safe_cex_types::retained_json::RetainedJsonInput;
use serde_json::{Error, Value};

/// A JSON input buffer retaining numeric lexemes, or numeric visitor bits after
/// Serde has buffered a number. This is not a wire serializer.
#[derive(Debug, Clone)]
pub struct JsNumberInput(Node);

use safe_cex_types::retained_json::RetainedJsonInput as Node;

/// Decode JSON with ECMAScript Number values before any typed Serde buffering.
pub fn js_from_str<T: DeserializeOwned + Serialize + JsNumberOutput>(
    text: &str,
) -> Result<T, Error> {
    let mut decoder = serde_json::Deserializer::from_str(text);
    let input = JsNumberInput::deserialize(&mut decoder)?;
    decoder.end()?;
    input.decode()
}

/// Decode an owned Value without routing its numbers through deserialize_any.
pub fn js_from_value<T: DeserializeOwned + Serialize + JsNumberOutput>(
    value: Value,
) -> Result<T, Error> {
    JsNumberInput::from(RetainedJsonInput::from_value(value)?).decode()
}

/// Decode a borrowed Value, preserving its lexical numbers; output is owned.
pub fn js_from_value_ref<T: DeserializeOwned + Serialize + JsNumberOutput>(
    value: &Value,
) -> Result<T, Error> {
    js_from_value(value.clone())
}

/// Read a JSON Number including overflow and negative underflow. No coercion of
/// strings, null or booleans, and no synthetic NaN JSON carrier.
pub fn js_value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.to_string().parse().ok(),
        _ => None,
    }
}

// The lower types layer owns capture so shared model info fields and this
// typed adapter use exactly the same lexeme and native-event visitor.
impl From<RetainedJsonInput> for JsNumberInput {
    fn from(input: RetainedJsonInput) -> Self {
        Self(input)
    }
}

impl<'de> Deserialize<'de> for JsNumberInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(RetainedJsonInput::deserialize(deserializer)?))
    }
}

struct Array(std::vec::IntoIter<Node>);
impl<'de> SeqAccess<'de> for Array {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        self.0
            .next()
            .map(|value| seed.deserialize(JsNumberDeserializer(value)))
            .transpose()
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}
struct Object {
    entries: std::vec::IntoIter<(Vec<u16>, Node)>,
    value: Option<Node>,
}
impl<'de> MapAccess<'de> for Object {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        match self.entries.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(String::from_utf16_lossy(&key).into_deserializer())
                    .map(Some)
            }
            None => Ok(None),
        }
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        seed.deserialize(JsNumberDeserializer(
            self.value
                .take()
                .ok_or_else(|| de::Error::custom("value requested before key"))?,
        ))
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

impl<'de> Deserializer<'de> for JsNumberDeserializer {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        // Serde has no marker trait for a lossless JSON buffer. Fail closed for
        // its stock Value visitor (including type aliases and generic result
        // types) instead of silently projecting a native infinity to null.
        if std::any::type_name::<V::Value>()
            .split(['<', '>', ',', ' ', '&', '(', ')', '[', ']', ';'])
            .any(|name| name == std::any::type_name::<Value>())
        {
            return Err(de::Error::custom(
                "numeric ingress forbids serde_json::Value buffers; use JsNumberInput",
            ));
        }
        match self.0 {
            Node::Undefined | Node::Null => visitor.visit_unit(),
            Node::Bool(value) => visitor.visit_bool(value),
            Node::Text(value) => visitor.visit_string(String::from_utf16_lossy(&value)),
            Node::Float(value) => visitor.visit_f64(value),
            Node::Lexeme(value) => {
                let number: f64 = value.parse().map_err(de::Error::custom)?;
                if number.is_finite()
                    && number.fract() == 0.0
                    && !(number == 0.0 && number.is_sign_negative())
                {
                    if (0.0..18446744073709551616.0).contains(&number) {
                        return visitor.visit_u64(number as u64);
                    }
                    if (-9223372036854775808.0..0.0).contains(&number) {
                        return visitor.visit_i64(number as i64);
                    }
                }
                visitor.visit_f64(number)
            }
            Node::Array(values) => {
                let mut seq = Array(values.into_iter());
                let result = visitor.visit_seq(&mut seq)?;
                if seq.0.len() != 0 {
                    return Err(de::Error::custom("trailing array elements"));
                }
                Ok(result)
            }
            Node::Object(entries) => {
                let mut map = Object {
                    entries: entries.into_iter(),
                    value: None,
                };
                let result = visitor.visit_map(&mut map)?;
                if map.entries.len() != 0 || map.value.is_some() {
                    return Err(de::Error::custom("trailing object entries"));
                }
                Ok(result)
            }
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if matches!(self.0, Node::Null | Node::Undefined) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        if name == safe_cex_types::retained_json::RETAINED_NATIVE {
            return self.0.into_native_payload().deserialize_seq(visitor);
        }
        if name == safe_cex_types::model_field::MODEL_FIELD && matches!(self.0, Node::Undefined) {
            visitor.visit_unit()
        } else {
            visitor.visit_newtype_struct(self)
        }
    }
    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if !matches!(self.0, Node::Object(_)) {
            return Err(de::Error::custom("expected a JSON map"));
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if !matches!(self.0, Node::Array(_)) {
            return Err(de::Error::custom("expected a JSON array"));
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            Node::Text(value) => {
                visitor.visit_enum(String::from_utf16_lossy(&value).into_deserializer())
            }
            Node::Object(entries) => {
                if entries.len() != 1 {
                    return Err(de::Error::custom(
                        "expected a single externally tagged enum variant",
                    ));
                }
                de::value::MapAccessDeserializer::new(Object {
                    entries: entries.into_iter(),
                    value: None,
                })
                .deserialize_enum(name, variants, visitor)
            }
            _ => Err(de::Error::custom(
                "expected string or externally tagged enum object",
            )),
        }
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        unit unit_struct tuple tuple_struct struct identifier ignored_any
    }
}

impl<'de> IntoDeserializer<'de, Error> for JsNumberDeserializer {
    type Deserializer = Self;
    fn into_deserializer(self) -> Self {
        self
    }
}

/// Opaque decoder supplied only to a checked decode callback. Its fields are
/// private; retain JsNumberInput when a nested or generic buffer is needed.
pub struct JsNumberDeserializer(Node);

impl JsNumberInput {
    /// Decode and check numeric fields before JSON serialization hides a loss.
    pub fn decode<T: DeserializeOwned + Serialize + JsNumberOutput>(self) -> Result<T, Error> {
        self.decode_with(T::deserialize)
    }
    /// Use a private remote-derived wire decoder returning a Serialize model.
    /// The check compares native numeric events, not serialized JSON values.
    pub fn decode_with<T: Serialize + JsNumberOutput>(
        self,
        decode: impl FnOnce(JsNumberDeserializer) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let expected = self.clone();
        let result = decode(JsNumberDeserializer(self.0))?;
        result.check_number_output();
        let actual = output::capture(&result)?;
        check_numbers(&expected.0, &actual.0, "$")?;
        Ok(result)
    }
}
fn as_number(node: &Node) -> Option<f64> {
    match node {
        Node::Float(v) => Some(*v),
        Node::Lexeme(v) => v.parse().ok(),
        _ => None,
    }
}
fn check_numbers(expected: &Node, actual: &Node, path: &str) -> Result<(), Error> {
    if let Some(number) = as_number(expected) {
        let actual = as_number(actual).ok_or_else(|| <Error as de::Error>::custom(format!("numeric ingress lost Number at {path}; serde_json::Value buffers are forbidden; use JsNumberInput")))?;
        if number.to_bits() != actual.to_bits() {
            return Err(de::Error::custom(format!(
                "numeric ingress changed Number bits at {path}"
            )));
        }
    } else {
        match (expected, actual) {
            (Node::Object(expected), Node::Object(actual)) => {
                for (key, expected) in expected {
                    if let Some((_, actual)) = actual.iter().find(|(name, _)| name == key) {
                        check_numbers(
                            expected,
                            actual,
                            &format!("{path}.{}", String::from_utf16_lossy(key)),
                        )?;
                    }
                }
            }
            (Node::Array(expected), Node::Array(actual)) => {
                for (i, (expected, actual)) in expected.iter().zip(actual).enumerate() {
                    check_numbers(expected, actual, &format!("{path}[{i}]"))?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}
impl Serialize for JsNumberInput {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// Capture serialized native values before JSON would project non-finite numbers.
pub fn js_capture_output<T: Serialize + ?Sized>(value: &T) -> Result<RetainedJsonInput, Error> {
    output::capture(value).map(|value| value.0)
}

#[cfg(test)]
mod output_controls {
    use super::*;
    fn unchecked<T: DeserializeOwned + Serialize>(raw: &str) -> Result<T, Error> {
        let input: JsNumberInput = serde_json::from_str(raw)?;
        let expected = input.clone();
        let value = T::deserialize(JsNumberDeserializer(input.0))?;
        check_numbers(&expected.0, &output::capture(&value)?.0, "$")?;
        Ok(value)
    }
    #[derive(Debug, Deserialize, Serialize)]
    struct Generic<T> {
        data: T,
    }
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(untagged)]
    enum OpaqueUnion {
        Json(Value),
    }
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(tag = "kind")]
    enum OpaqueTagged {
        Data { data: Value },
    }
    #[derive(Debug, Deserialize, Serialize)]
    struct OpaqueFlattened {
        #[serde(flatten)]
        inner: Generic<Value>,
    }

    #[test]
    fn opaque_value_projection_fails_whole_output_guard_by_name() {
        // Negative controls: Serde Content hides Value's visitor from the input
        // decoder. Native Infinity becomes null inside each opaque wrapper.
        for spelling in ["1e400", "-1e400"] {
            let errors = [
                unchecked::<OpaqueUnion>(spelling)
                    .expect_err("opaque Value projection consumption guard"),
                unchecked::<OpaqueTagged>(&format!(r#"{{"kind":"Data","data":{spelling}}}"#))
                    .expect_err("opaque Value projection consumption guard"),
                unchecked::<OpaqueFlattened>(&format!(r#"{{"data":{spelling}}}"#))
                    .expect_err("opaque Value projection consumption guard"),
            ];
            for error in errors {
                assert!(
                    error.to_string().contains("numeric ingress lost Number at"),
                    "opaque Value projection consumption guard: {error}"
                );
            }
        }
    }
}
