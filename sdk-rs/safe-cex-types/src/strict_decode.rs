//! Shape guards for TypeScript records; tuple-shaped protocols remain arrays.
//!
//! The permissive metadata companions are inaccessible to callers.
//! ```compile_fail
//! use safe_cex_types::strict_codecs::WireOrderSide;
//! ```
//! ```compile_fail
//! use safe_cex_types::store_model::strict_codecs::WireDisplayedPosition;
//! ```
use serde::de::{Deserializer, Visitor};

/// Require a map before the private serde-derived record decoder runs.
pub fn object<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> impl Deserializer<'de, Error = D::Error> {
    Object(deserializer)
}

struct Object<D>(D);
impl<'de, D: Deserializer<'de>> Deserializer<'de> for Object<D> {
    type Error = D::Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.0.deserialize_map(visitor)
    }
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.deserialize_map(visitor)
    }
    // Explicit serde forwarding keeps every generated method visible to inventory.
    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use serde::Deserialize;
    use serde_json::json;
    #[derive(Debug)]
    pub(crate) struct Inventory(pub(crate) Vec<String>);
    impl std::fmt::Display for Inventory {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl std::error::Error for Inventory {}
    impl serde::de::Error for Inventory {
        fn custom<T: std::fmt::Display>(message: T) -> Self {
            Self(vec![message.to_string()])
        }
        fn unknown_variant(_value: &str, expected: &'static [&'static str]) -> Self {
            Self(expected.iter().map(|value| (*value).to_owned()).collect())
        }
    }
    pub(crate) fn string_union<T: for<'de> Deserialize<'de>>(
        name: &str,
        actual: Vec<String>,
        expected: Vec<String>,
    ) {
        assert_eq!(actual, expected, "{name} serde spelling metadata widened");
        for spelling in &actual {
            for case in [spelling.to_uppercase(), spelling.to_lowercase()] {
                if !actual.contains(&case) {
                    assert!(
                        <T as Deserialize>::deserialize(json!(case)).is_err(),
                        "{name}: unexpected case spelling"
                    );
                }
            }
            assert!(serde_json::from_value::<T>(json!(spelling)).is_ok());
            for value in [
                json!({spelling.clone():null}),
                json!([spelling]),
                json!(null),
                json!(true),
                json!(1),
                json!(format!(" {spelling}")),
                json!(format!("\u{feff}{spelling}")),
                json!(format!("{spelling}__unknown")),
            ] {
                assert!(
                    <T as Deserialize>::deserialize(&value).is_err(),
                    "{name} accepted {value}"
                );
            }
        }
    }
    pub(crate) fn record<T: for<'de> Deserialize<'de>>(name: &str) {
        for value in [
            json!([]),
            json!([null]),
            json!([1, 2, false, "x", {}, []]),
            json!(null),
            json!(true),
            json!(1),
            json!("record"),
        ] {
            let error = <T as Deserialize>::deserialize(value.clone())
                .err()
                .expect("record shape must fail")
                .to_string();
            assert!(
                error.contains("invalid type")
                    && (error.contains("map") || error.contains("struct")),
                "{name}:{value}: unrelated error: {error}"
            );
        }
    }
}
