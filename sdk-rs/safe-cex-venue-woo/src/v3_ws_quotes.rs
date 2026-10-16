//! BBO and Trade declarations used by the inert ticker composer.
use serde::{Deserialize, Serialize};

fn trade_side<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if matches!(value.as_str(), "BUY" | "SELL") {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "WOO V3 trade side must be BUY or SELL",
        ))
    }
}
fn trade_source<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let value = f64::deserialize(deserializer)?;
    if value == 0.0 || value == 1.0 {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "WOO V3 trade source must be 0 or 1",
        ))
    }
}
// ts: WooV3WsBbo
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsBbo {
    pub symbol: String,
    pub ask_price: String,
    pub ask_quantity: String,
    pub bid_price: String,
    pub bid_quantity: String,
    pub timestamp: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsBbo")]
struct WooV3WsBboWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "askPrice")]
    ask_price: String,
    #[serde(rename = "askQuantity")]
    ask_quantity: String,
    #[serde(rename = "bidPrice")]
    bid_price: String,
    #[serde(rename = "bidQuantity")]
    bid_quantity: String,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3WsBbo {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3WsBboWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3WsBbo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3WsBboWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsBbo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, ask_price, ask_quantity, bid_price, bid_quantity, timestamp });
    }
}

// ts: WooV3WsTrade
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsTrade {
    pub symbol: String,
    pub price: String,
    pub quantity: String,
    pub side: String,
    pub source: f64,
    pub matched_rpi: bool,
    pub timestamp: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsTrade")]
struct WooV3WsTradeWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "quantity")]
    quantity: String,
    #[serde(rename = "side", deserialize_with = "trade_side")]
    side: String,
    #[serde(rename = "source", deserialize_with = "trade_source")]
    source: f64,
    #[serde(rename = "matchedRpi")]
    matched_rpi: bool,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3WsTrade {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3WsTradeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3WsTrade {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3WsTradeWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsTrade {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, price, quantity, side, source, matched_rpi, timestamp });
    }
}
