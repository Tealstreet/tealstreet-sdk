use crate::exchange::SubscribedCandle;
use safe_cex_types::{AnyTimeframe, Candle, ModelField, OrderSide, model_field::MODEL_RECORD};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{MapAccess, Visitor},
    ser::SerializeStruct,
};
use std::fmt;

impl Serialize for SubscribedCandle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct(MODEL_RECORD, 9)?;
        record.serialize_field("timestamp", &self.candle.timestamp)?;
        record.serialize_field("open", &self.candle.open)?;
        record.serialize_field("high", &self.candle.high)?;
        record.serialize_field("low", &self.candle.low)?;
        record.serialize_field("close", &self.candle.close)?;
        record.serialize_field("volume", &self.candle.volume)?;
        if let Some(side) = &self.candle.last_side {
            record.serialize_field("lastSide", side)?;
        }
        record.serialize_field("symbol", &self.symbol)?;
        self.interval.serialize_field(&mut record, "interval")?;
        record.end()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wire {
    timestamp: f64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    #[serde(default)]
    last_side: ModelField<OrderSide>,
    symbol: String,
    #[serde(default)]
    interval: ModelField<AnyTimeframe>,
}
impl From<Wire> for SubscribedCandle {
    fn from(value: Wire) -> Self {
        Self {
            candle: Candle {
                timestamp: value.timestamp,
                open: value.open,
                high: value.high,
                low: value.low,
                close: value.close,
                volume: value.volume,
                last_side: value.last_side.into_value(),
            },
            symbol: value.symbol,
            interval: value.interval,
        }
    }
}
struct RecordVisitor;
impl<'de> Visitor<'de> for RecordVisitor {
    type Value = SubscribedCandle;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a subscribed candle record")
    }
    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        decoder: D,
    ) -> Result<Self::Value, D::Error> {
        decoder.deserialize_map(self)
    }
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        Wire::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(Into::into)
    }
}
impl<'de> Deserialize<'de> for SubscribedCandle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_newtype_struct(MODEL_RECORD, RecordVisitor)
    }
}
