use safe_cex_registry::{public_data::*, registry::*};
fn main() {
    // Keep a Rust-side exhaustive guard as well as the wire-side serde inventory:
    // serde(skip) variants deliberately do not appear in the latter.
    let _review_every_rust_variant = |channel: safe_cex_types::gateway::GatewayChannel| {
        use safe_cex_types::gateway::GatewayChannel::*;
        match channel {
            Auth | Ticker | Tickers | Orderbook | Candle | Trade | Market => (),
        }
    };
    let channels = enum_wire_inventory::<safe_cex_types::gateway::GatewayChannel>()
        .expect("GatewayChannel decoder must expose its nonempty wire inventory");
    let keys: Vec<_> = [
        ("BTC:USDT", "1m"),
        ("A:B:C", "1h"),
        ("", "1M"),
        ("雪:😀", "5m"),
    ]
    .map(|(symbol, interval)| {
        let key = get_candle_subscription_key(symbol, interval);
        serde_json::json!({"key": key, "parts": parse_candle_subscription_key(&key)})
    })
    .into();
    println!(
        "{}",
        serde_json::json!({"channels": channels, "authReply": serde_json::from_value::<safe_cex_types::gateway::GatewayMessage>(serde_json::json!({"type":"subscribed","exchangeId":"bybit","channel":"auth"})).unwrap(), "rejectsNullSymbol": serde_json::from_value::<safe_cex_types::gateway::GatewayMessage>(serde_json::json!({"type":"data","exchangeId":"bybit","symbol":null})).is_err(), "malformedAstral": parse_candle_subscription_key("😀"), "registry": EXCHANGE_REGISTRY, "names": get_registered_exchange_names(), "ping": EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS, "reconnect": EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS, "keys": keys})
    );
}

// Strict string decoders forward D::Error to their derived wire enum. Capture
// unknown_variant metadata there; direct enum derives expose the same list.
#[derive(Debug)]
enum EnumInventoryError {
    Variants(&'static [&'static str]),
    Other(String),
}
impl std::fmt::Display for EnumInventoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Variants(variants) => write!(f, "captured {} enum variants", variants.len()),
            Self::Other(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for EnumInventoryError {}
impl serde::de::Error for EnumInventoryError {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self::Other(message.to_string())
    }
    fn unknown_variant(_variant: &str, expected: &'static [&'static str]) -> Self {
        Self::Variants(expected)
    }
}
fn enum_wire_inventory<T: serde::de::DeserializeOwned>()
-> Result<&'static [&'static str], EnumInventoryError> {
    match T::deserialize(&mut EnumInventory) {
        Err(EnumInventoryError::Variants(variants)) if !variants.is_empty() => Ok(variants),
        Err(EnumInventoryError::Variants(_)) => Err(EnumInventoryError::Other(
            "empty enum wire inventory".into(),
        )),
        Err(error) => Err(error),
        Ok(_) => Err(EnumInventoryError::Other(
            "decoder accepted inventory sentinel without metadata".into(),
        )),
    }
}
struct EnumInventory;
impl<'de> serde::Deserializer<'de> for &mut EnumInventory {
    type Error = EnumInventoryError;
    fn deserialize_enum<V: serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(EnumInventoryError::Variants(variants))
    }
    fn deserialize_string<V: serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_string("__registry_enum_inventory_invalid__".into())
    }
    fn deserialize_str<V: serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_string(visitor)
    }
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(EnumInventoryError::Other(
            "decoder did not expose enum metadata".into(),
        ))
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    #[derive(Deserialize)]
    enum Direct {
        First,
        Second,
    }
    struct Strict;
    impl<'de> Deserialize<'de> for Strict {
        fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
            let value = String::deserialize(decoder)?;
            Direct::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
                .map(|_| Self)
        }
    }
    struct Unsupported<const MODE: u8>;
    impl<'de, const MODE: u8> Deserialize<'de> for Unsupported<MODE> {
        fn deserialize<D: serde::Deserializer<'de>>(_decoder: D) -> Result<Self, D::Error> {
            match MODE {
                0 => Err(serde::de::Error::custom("no metadata")),
                1 => Ok(Self),
                _ => Err(serde::de::Error::unknown_variant("sentinel", &[])),
            }
        }
    }
    #[test]
    fn direct_and_strict_string_enum_decoders_supply_the_same_native_inventory() {
        assert_eq!(
            enum_wire_inventory::<Direct>().unwrap(),
            &["First", "Second"]
        );
        assert_eq!(
            enum_wire_inventory::<Strict>().unwrap(),
            &["First", "Second"]
        );
    }
    #[test]
    fn gateway_strict_decoder_supplies_all_seven_native_wire_members() {
        assert_eq!(
            enum_wire_inventory::<safe_cex_types::gateway::GatewayChannel>().unwrap(),
            &[
                "auth",
                "ticker",
                "tickers",
                "orderbook",
                "candle",
                "trade",
                "market"
            ]
        );
    }
    #[test]
    fn missing_successful_and_empty_metadata_are_rejected() {
        assert!(matches!(
            enum_wire_inventory::<Unsupported<0>>(),
            Err(EnumInventoryError::Other(_))
        ));
        assert!(matches!(
            enum_wire_inventory::<Unsupported<1>>(),
            Err(EnumInventoryError::Other(_))
        ));
        assert!(matches!(
            enum_wire_inventory::<Unsupported<2>>(),
            Err(EnumInventoryError::Other(_))
        ));
    }
}
