#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}
impl<'de> serde::Deserialize<'de> for Side {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(deserializer)?.as_str() {
            "buy" => Ok(Self::Buy),
            "sell" => Ok(Self::Sell),
            other => Err(serde::de::Error::unknown_variant(other, &["buy", "sell"])),
        }
    }
}

mod private {
    #[derive(Debug, serde::Serialize)]
    pub struct Record {
        pub value: bool,
    }
    impl<'de> serde::Deserialize<'de> for Record {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let map = <serde_json::Map<String, serde_json::Value> as serde::Deserialize>::deserialize(deserializer)?;
            let value = map.get("value").and_then(serde_json::Value::as_bool)
                .ok_or_else(|| serde::de::Error::custom("value must be a boolean"))?;
            Ok(Self { value })
        }
    }
}
pub use private::Record as Exported;

pub fn native_contract_enabled() -> bool {
    cfg!(feature = "native")
}

pub fn has_private_helper() {
    struct LocalHelper;
    let _ = std::mem::size_of::<LocalHelper>();
}
