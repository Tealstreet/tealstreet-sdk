// Serde derive supplies its canonical wire-variant list to deserialize_enum.
// Exhaustive matches in each test additionally pin Rust-only and skipped variants.
#[derive(Default)]
struct EnumInventory(Option<&'static [&'static str]>);
impl<'de> serde::Deserializer<'de> for &mut EnumInventory {
    type Error = InventoryError;
    fn deserialize_enum<V: serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0 = Some(variants);
        Err(serde::de::Error::custom(
            "inventory captured; no value requested",
        ))
    }
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_str("\0inventory-only-invalid-variant")
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

pub fn enum_inventory<T: serde::de::DeserializeOwned>() -> Vec<&'static str> {
    let mut inventory = EnumInventory::default();
    let error = T::deserialize(&mut inventory).err();
    inventory
        .0
        .or_else(|| error.and_then(|error| error.0))
        .expect("derived enum metadata")
        .to_vec()
}

pub fn assert_string_only<T: serde::de::DeserializeOwned>(
    literals: &serde_json::Value,
    name: &str,
) {
    for literal in literals.as_array().unwrap() {
        let text = literal.as_str().unwrap();
        for candidate in [
            format!(" {text}"),
            format!("{text} "),
            format!("\u{feff}{text}"),
            format!("{text}\u{feff}"),
            text.to_lowercase(),
            text.to_uppercase(),
            format!("{text}-unknown"),
        ] {
            let candidate = serde_json::Value::String(candidate);
            if !literals.as_array().unwrap().contains(&candidate) {
                assert!(
                    serde_json::from_value::<T>(candidate.clone()).is_err(),
                    "{name} rejects altered literal {candidate}"
                );
            }
        }
        for payload in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!([]),
        ] {
            let value = serde_json::json!({literal.as_str().unwrap():payload});
            assert!(
                serde_json::from_value::<T>(value.clone()).is_err(),
                "{name} rejects tagged object {value}"
            );
        }
    }
    for value in [
        serde_json::Value::Null,
        serde_json::json!(0),
        serde_json::json!(true),
        serde_json::json!([]),
    ] {
        assert!(
            serde_json::from_value::<T>(value.clone()).is_err(),
            "{name} rejects non-string {value}"
        );
    }
}

#[derive(Debug)]
struct InventoryError(Option<&'static [&'static str]>);
impl std::fmt::Display for InventoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("enum inventory probe")
    }
}
impl std::error::Error for InventoryError {}
impl serde::de::Error for InventoryError {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Self(None)
    }
    fn unknown_variant(_: &str, expected: &'static [&'static str]) -> Self {
        Self(Some(expected))
    }
}
