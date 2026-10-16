//! Pure public V3 payload decoding; no socket or store registration.
use crate::v3_ws_quotes::{WooV3WsBbo, WooV3WsTrade};
use crate::v3_ws_session::{
    require_boolean, require_decimal, require_integer, require_non_negative_decimal,
    require_positive_decimal,
};
use crate::v3_ws_topics::{
    PlainKind, WooV3WsKlineInterval, WooV3WsTopicDescriptor, require_symbol,
};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_types::object_map::JsObjectMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ts: WooV3WsBookLevel
pub type WooV3WsBookLevel = [String; 2];

// ts: WooV3WsOrderBookUpdate
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsOrderBookUpdate {
    pub symbol: String,
    pub previous_timestamp: f64,
    pub timestamp: f64,
    pub asks: Vec<WooV3WsBookLevel>,
    pub bids: Vec<WooV3WsBookLevel>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsOrderBookUpdate")]
struct WooV3WsOrderBookUpdateFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "previousTimestamp")]
    previous_timestamp: f64,
    #[serde(rename = "timestamp")]
    timestamp: f64,
    #[serde(rename = "asks")]
    asks: Vec<WooV3WsBookLevel>,
    #[serde(rename = "bids")]
    bids: Vec<WooV3WsBookLevel>,
}
impl Serialize for WooV3WsOrderBookUpdate {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        WooV3WsOrderBookUpdateFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsOrderBookUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|d| {
                WooV3WsOrderBookUpdateFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsOrderBookUpdate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, previous_timestamp, timestamp, asks, bids });
    }
}

// ts: WooV3WsOrderBook10
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsOrderBook10 {
    pub symbol: String,
    pub timestamp: f64,
    pub asks: Vec<WooV3WsBookLevel>,
    pub bids: Vec<WooV3WsBookLevel>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsOrderBook10")]
struct WooV3WsOrderBook10Fields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "timestamp")]
    timestamp: f64,
    #[serde(rename = "asks")]
    asks: Vec<WooV3WsBookLevel>,
    #[serde(rename = "bids")]
    bids: Vec<WooV3WsBookLevel>,
}
impl Serialize for WooV3WsOrderBook10 {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        WooV3WsOrderBook10Fields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsOrderBook10 {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|d| {
                WooV3WsOrderBook10Fields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsOrderBook10 {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, timestamp, asks, bids });
    }
}

// ts: WooV3WsTicker
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsTicker {
    pub symbol: String,
    pub open: String,
    pub close: String,
    pub high: String,
    pub low: String,
    pub volume: String,
    pub amount: String,
    pub aggregated_volume: String,
    pub aggregated_amount: String,
    pub trade_count: f64,
    pub timestamp: f64,
    pub last_trade_timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsTicker")]
struct WooV3WsTickerFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "open")]
    open: String,
    #[serde(rename = "close")]
    close: String,
    #[serde(rename = "high")]
    high: String,
    #[serde(rename = "low")]
    low: String,
    #[serde(rename = "volume")]
    volume: String,
    #[serde(rename = "amount")]
    amount: String,
    #[serde(rename = "aggregatedVolume")]
    aggregated_volume: String,
    #[serde(rename = "aggregatedAmount")]
    aggregated_amount: String,
    #[serde(rename = "tradeCount")]
    trade_count: f64,
    #[serde(rename = "timestamp")]
    timestamp: f64,
    #[serde(rename = "lastTradeTimestamp")]
    last_trade_timestamp: f64,
}
impl Serialize for WooV3WsTicker {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        WooV3WsTickerFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsTicker {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|d| {
                WooV3WsTickerFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsTicker {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, open, close, high, low, volume, amount, aggregated_volume, aggregated_amount, trade_count, timestamp, last_trade_timestamp });
    }
}

// ts: WooV3WsKline
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsKline {
    pub symbol: String,
    pub interval: WooV3WsKlineInterval,
    pub open: String,
    pub close: String,
    pub high: String,
    pub low: String,
    pub volume: String,
    pub amount: String,
    pub start_timestamp: f64,
    pub end_timestamp: f64,
    pub timestamp: f64,
    pub last_trade_timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsKline")]
struct WooV3WsKlineFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "interval")]
    interval: WooV3WsKlineInterval,
    #[serde(rename = "open")]
    open: String,
    #[serde(rename = "close")]
    close: String,
    #[serde(rename = "high")]
    high: String,
    #[serde(rename = "low")]
    low: String,
    #[serde(rename = "volume")]
    volume: String,
    #[serde(rename = "amount")]
    amount: String,
    #[serde(rename = "startTimestamp")]
    start_timestamp: f64,
    #[serde(rename = "endTimestamp")]
    end_timestamp: f64,
    #[serde(rename = "timestamp")]
    timestamp: f64,
    #[serde(rename = "lastTradeTimestamp")]
    last_trade_timestamp: f64,
}
impl Serialize for WooV3WsKline {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        WooV3WsKlineFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsKline {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|d| {
                WooV3WsKlineFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsKline {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, interval, open, close, high, low, volume, amount, start_timestamp, end_timestamp, timestamp, last_trade_timestamp });
    }
}

// ts: WooV3WsPrice
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsPrice {
    pub symbol: String,
    pub price: String,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsPrice")]
struct WooV3WsPriceFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3WsPrice {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        WooV3WsPriceFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsPrice {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|d| {
                WooV3WsPriceFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsPrice {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, price, timestamp });
    }
}

// ts: WooV3WsFundingRate
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsFundingRate {
    pub symbol: String,
    pub rate: String,
    pub next_funding_timestamp: f64,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsFundingRate")]
struct WooV3WsFundingRateFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "rate")]
    rate: String,
    #[serde(rename = "nextFundingTimestamp")]
    next_funding_timestamp: f64,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3WsFundingRate {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        WooV3WsFundingRateFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsFundingRate {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|d| {
                WooV3WsFundingRateFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsFundingRate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, rate, next_funding_timestamp, timestamp });
    }
}

// ts: WooV3WsOpenInterest
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsOpenInterest {
    pub symbol: String,
    pub open_interest: String,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsOpenInterest")]
struct WooV3WsOpenInterestFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "openInterest")]
    open_interest: String,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3WsOpenInterest {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        WooV3WsOpenInterestFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsOpenInterest {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|d| {
                WooV3WsOpenInterestFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsOpenInterest {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, open_interest, timestamp });
    }
}

// ts: WooV3WsPublicPayload
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3WsPublicPayload {
    OrderBookUpdate(WooV3WsOrderBookUpdate),
    OrderBook10(WooV3WsOrderBook10),
    Bbo(WooV3WsBbo),
    Trade(WooV3WsTrade),
    Ticker(WooV3WsTicker),
    Kline(WooV3WsKline),
    Price(WooV3WsPrice),
    FundingRate(WooV3WsFundingRate),
    OpenInterest(WooV3WsOpenInterest),
}
#[derive(Deserialize)]
#[serde(remote = "WooV3WsPublicPayload", untagged)]
enum PublicPayloadFields {
    OrderBookUpdate(WooV3WsOrderBookUpdate),
    OrderBook10(WooV3WsOrderBook10),
    Bbo(WooV3WsBbo),
    Trade(WooV3WsTrade),
    Ticker(WooV3WsTicker),
    Kline(WooV3WsKline),
    Price(WooV3WsPrice),
    FundingRate(WooV3WsFundingRate),
    OpenInterest(WooV3WsOpenInterest),
}
impl<'de> Deserialize<'de> for WooV3WsPublicPayload {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|d| {
                PublicPayloadFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsPublicPayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { OrderBookUpdate(value), OrderBook10(value), Bbo(value), Trade(value), Ticker(value), Kline(value), Price(value), FundingRate(value), OpenInterest(value) });
    }
}

fn record(input: Option<&JsNumberInput>, path: &str) -> Result<JsObjectMap<JsNumberInput>, String> {
    input
        .and_then(|v| v.clone().decode::<JsObjectMap<JsNumberInput>>().ok())
        .ok_or_else(|| format!("{path} must be an object"))
}
fn array(input: Option<&JsNumberInput>, path: &str) -> Result<Vec<JsNumberInput>, String> {
    input
        .and_then(|v| v.clone().decode::<Vec<JsNumberInput>>().ok())
        .ok_or_else(|| format!("{path} must be an array"))
}
fn text(input: Option<&JsNumberInput>) -> Option<String> {
    input.and_then(|v| v.clone().decode::<String>().ok())
}
// ts: decodeLevels
pub fn decode_levels(
    value: Option<&JsNumberInput>,
    path: &str,
    allow_zero_quantity: bool,
) -> Result<Vec<WooV3WsBookLevel>, String> {
    array(value, path)?
        .iter()
        .enumerate()
        .map(|(index, input)| {
            let parts = input
                .clone()
                .decode::<Vec<JsNumberInput>>()
                .ok()
                .filter(|parts| parts.len() == 2)
                .ok_or_else(|| format!("{path}[{index}] must be a price/quantity tuple"))?;
            let price = require_positive_decimal(parts.first(), &format!("{path}[{index}][0]"))?;
            let quantity = if allow_zero_quantity {
                require_non_negative_decimal(parts.get(1), &format!("{path}[{index}][1]"))?
            } else {
                require_positive_decimal(parts.get(1), &format!("{path}[{index}][1]"))?
            };
            Ok([price, quantity])
        })
        .collect()
}
// ts: assertPayloadSymbol
pub fn assert_payload_symbol(
    row: &JsObjectMap<JsNumberInput>,
    descriptor: &WooV3WsTopicDescriptor,
    path: &str,
) -> Result<String, String> {
    let input = text(row.get("s")).map(Value::String);
    let symbol = require_symbol(input.as_ref(), &format!("{path}.s"))?;
    let expected = match descriptor {
        WooV3WsTopicDescriptor::Book { symbol, .. }
        | WooV3WsTopicDescriptor::Kline { symbol, .. }
        | WooV3WsTopicDescriptor::Public { symbol, .. } => symbol,
        WooV3WsTopicDescriptor::Private { .. } => {
            return Err(format!("{path} requires a public topic descriptor"));
        }
    };
    if &symbol != expected {
        return Err(format!("{path}.s does not match frame topic symbol"));
    }
    Ok(symbol)
}
// ts: decodePublicPayload
pub fn decode_public_payload(
    descriptor: &WooV3WsTopicDescriptor,
    input: Option<&JsNumberInput>,
) -> Result<WooV3WsPublicPayload, String> {
    let kind = match descriptor {
        WooV3WsTopicDescriptor::Book { kind, .. } => kind.as_str(),
        WooV3WsTopicDescriptor::Kline { kind, .. } => kind.as_str(),
        WooV3WsTopicDescriptor::Public { kind, .. } => kind.as_str(),
        WooV3WsTopicDescriptor::Private { .. } => {
            return Err("WOO V3 websocket payload requires a public topic descriptor".into());
        }
    };
    let path = format!("WOO V3 websocket {kind}.data");
    let row = record(input, &path)?;
    let symbol = assert_payload_symbol(&row, descriptor, &path)?;
    let integer = |key: &str| require_integer(row.get(key), &format!("{path}.{key}"), 0.0);
    let positive = |key: &str| require_positive_decimal(row.get(key), &format!("{path}.{key}"));
    let nonnegative =
        |key: &str| require_non_negative_decimal(row.get(key), &format!("{path}.{key}"));
    let levels = |key: &str, zero| decode_levels(row.get(key), &format!("{path}.{key}"), zero);
    match descriptor {
        WooV3WsTopicDescriptor::Book { .. } => {
            let previous_timestamp = integer("prevTs")?;
            let timestamp = integer("ts")?;
            if timestamp <= previous_timestamp {
                return Err(format!("{path}.ts must follow prevTs"));
            }
            Ok(WooV3WsPublicPayload::OrderBookUpdate(
                WooV3WsOrderBookUpdate {
                    symbol,
                    previous_timestamp,
                    timestamp,
                    asks: levels("asks", true)?,
                    bids: levels("bids", true)?,
                },
            ))
        }
        WooV3WsTopicDescriptor::Public {
            kind: PlainKind::Orderbook10,
            ..
        } => Ok(WooV3WsPublicPayload::OrderBook10(WooV3WsOrderBook10 {
            symbol,
            timestamp: integer("ts")?,
            asks: levels("asks", false)?,
            bids: levels("bids", false)?,
        })),
        WooV3WsTopicDescriptor::Public {
            kind: PlainKind::Bbo | PlainKind::Bborpi,
            ..
        } => Ok(WooV3WsPublicPayload::Bbo(WooV3WsBbo {
            symbol,
            ask_price: positive("ap")?,
            ask_quantity: positive("aq")?,
            bid_price: positive("bp")?,
            bid_quantity: positive("bq")?,
            timestamp: integer("ts")?,
        })),
        WooV3WsTopicDescriptor::Public {
            kind: PlainKind::Trade,
            ..
        } => {
            let source = integer("src")?;
            if source != 0.0 && source != 1.0 {
                return Err(format!("{path}.src must be 0 or 1"));
            }
            let price = positive("px")?;
            let quantity = positive("sx")?;
            let side = text(row.get("sd"))
                .filter(|s| matches!(s.as_str(), "BUY" | "SELL"))
                .ok_or_else(|| format!("{path}.sd must be one of BUY, SELL"))?;
            Ok(WooV3WsPublicPayload::Trade(WooV3WsTrade {
                symbol,
                price,
                quantity,
                side,
                source,
                matched_rpi: require_boolean(row.get("rpi"), &format!("{path}.rpi"))?,
                timestamp: integer("ts")?,
            }))
        }
        WooV3WsTopicDescriptor::Public {
            kind: PlainKind::Ticker,
            ..
        } => Ok(WooV3WsPublicPayload::Ticker(WooV3WsTicker {
            symbol,
            open: positive("o")?,
            close: positive("c")?,
            high: positive("h")?,
            low: positive("l")?,
            volume: nonnegative("v")?,
            amount: nonnegative("a")?,
            aggregated_volume: nonnegative("q")?,
            aggregated_amount: nonnegative("u")?,
            trade_count: integer("cnt")?,
            timestamp: integer("ts")?,
            last_trade_timestamp: integer("tts")?,
        })),
        WooV3WsTopicDescriptor::Kline {
            interval: expected, ..
        } => {
            let interval=text(row.get("t")).and_then(|s| WooV3WsKlineInterval::parse(&s)).ok_or_else(|| format!("{path}.t must be one of 1m, 3m, 5m, 15m, 30m, 1h, 2h, 4h, 6h, 12h, 1d, 3d, 1w, 1M"))?;
            if &interval != expected {
                return Err(format!("{path}.t does not match frame topic interval"));
            }
            let start_timestamp = integer("st")?;
            let end_timestamp = integer("et")?;
            if end_timestamp <= start_timestamp {
                return Err(format!("{path}.et must follow st"));
            }
            Ok(WooV3WsPublicPayload::Kline(WooV3WsKline {
                symbol,
                interval,
                open: positive("o")?,
                close: positive("c")?,
                high: positive("h")?,
                low: positive("l")?,
                volume: nonnegative("v")?,
                amount: nonnegative("a")?,
                start_timestamp,
                end_timestamp,
                timestamp: integer("ts")?,
                last_trade_timestamp: integer("tts")?,
            }))
        }
        WooV3WsTopicDescriptor::Public {
            kind: PlainKind::Indexprice | PlainKind::Markprice,
            ..
        } => Ok(WooV3WsPublicPayload::Price(WooV3WsPrice {
            symbol,
            price: positive("px")?,
            timestamp: integer("ts")?,
        })),
        WooV3WsTopicDescriptor::Public {
            kind: PlainKind::Estfundingrate,
            ..
        } => Ok(WooV3WsPublicPayload::FundingRate(WooV3WsFundingRate {
            symbol,
            rate: require_decimal(row.get("r"), &format!("{path}.r"))?,
            next_funding_timestamp: integer("ft")?,
            timestamp: integer("ts")?,
        })),
        WooV3WsTopicDescriptor::Public {
            kind: PlainKind::Openinterest,
            ..
        } => Ok(WooV3WsPublicPayload::OpenInterest(WooV3WsOpenInterest {
            symbol,
            open_interest: nonnegative("oi")?,
            timestamp: integer("ts")?,
        })),
        WooV3WsTopicDescriptor::Private { .. } => unreachable!("public descriptor validated"),
    }
}
