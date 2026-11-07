use crate::{AnyTimeframe, Candle, Market, OrderBook, Ticker, Timeframe};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GatewayMessageType {
    Subscribe,
    Unsubscribe,
    Subscribed,
    Unsubscribed,
    Data,
    Error,
    Ping,
    Pong,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GatewayChannel {
    // ts-bug: ExternalPublicDataProvider.#onOpen casts auth outside the declared union.
    Auth,
    Ticker,
    Tickers,
    Orderbook,
    Candle,
    Trade,
    Market,
}

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayMessage {
    #[serde(rename = "type")]
    pub message_type: GatewayMessageType,
    pub exchange_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub channel: Option<GatewayChannel>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub symbol: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub interval: Option<AnyTimeframe>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::retained_json::deserialize_present_retained_json"
    )]
    pub data: Option<serde_json::Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub error: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum DataTag {
    Data,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum TickerChannel {
    Ticker,
    Tickers,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum OrderbookChannel {
    Orderbook,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum CandleChannel {
    Candle,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum MarketChannel {
    Market,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GatewayTickerData {
    Ticker(Box<Ticker>),
    Tickers(Vec<Ticker>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayTickerUpdate {
    #[serde(rename = "type")]
    message_type: DataTag,
    pub exchange_id: String,
    channel: TickerChannel,
    pub data: GatewayTickerData,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayOrderbookUpdate {
    #[serde(rename = "type")]
    message_type: DataTag,
    pub exchange_id: String,
    channel: OrderbookChannel,
    pub symbol: String,
    pub data: OrderBook,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayCandleUpdate {
    #[serde(rename = "type")]
    message_type: DataTag,
    pub exchange_id: String,
    channel: CandleChannel,
    pub symbol: String,
    pub interval: Timeframe,
    pub data: Candle,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayMarketUpdate {
    #[serde(rename = "type")]
    message_type: DataTag,
    pub exchange_id: String,
    channel: MarketChannel,
    pub data: Vec<Market>,
}

impl GatewayTickerUpdate {
    pub fn new(exchange_id: String, plural: bool, data: GatewayTickerData) -> Self {
        Self {
            message_type: DataTag::Data,
            exchange_id,
            channel: if plural {
                TickerChannel::Tickers
            } else {
                TickerChannel::Ticker
            },
            data,
        }
    }
}
impl GatewayOrderbookUpdate {
    pub fn new(exchange_id: String, symbol: String, data: OrderBook) -> Self {
        Self {
            message_type: DataTag::Data,
            exchange_id,
            channel: OrderbookChannel::Orderbook,
            symbol,
            data,
        }
    }
}
impl GatewayCandleUpdate {
    pub fn new(exchange_id: String, symbol: String, interval: Timeframe, data: Candle) -> Self {
        Self {
            message_type: DataTag::Data,
            exchange_id,
            channel: CandleChannel::Candle,
            symbol,
            interval,
            data,
        }
    }
}
impl GatewayMarketUpdate {
    pub fn new(exchange_id: String, data: Vec<Market>) -> Self {
        Self {
            message_type: DataTag::Data,
            exchange_id,
            channel: MarketChannel::Market,
            data,
        }
    }
}

// Private wire companions keep serde metadata without exposing permissive decoders.
mod strict_codecs {
    use super::{
        AnyTimeframe, Candle, CandleChannel, DataTag, GatewayChannel, GatewayMessageType,
        GatewayTickerData, Market, MarketChannel, OrderBook, OrderbookChannel, TickerChannel,
        Timeframe, present,
    };
    #[cfg(test)]
    use super::{
        GatewayCandleUpdate, GatewayMarketUpdate, GatewayMessage, GatewayOrderbookUpdate,
        GatewayTickerUpdate,
    };
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::GatewayMessageType")]
    #[serde(rename_all = "lowercase")]
    enum WireGatewayMessageType {
        Subscribe,
        Unsubscribe,
        Subscribed,
        Unsubscribed,
        Data,
        Error,
        Ping,
        Pong,
    }

    impl<'de> serde::Deserialize<'de> for super::GatewayMessageType {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireGatewayMessageType::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::GatewayChannel")]
    #[serde(rename_all = "lowercase")]
    enum WireGatewayChannel {
        // ts-bug: ExternalPublicDataProvider.#onOpen casts auth outside the declared union.
        Auth,
        Ticker,
        Tickers,
        Orderbook,
        Candle,
        Trade,
        Market,
    }

    impl<'de> serde::Deserialize<'de> for super::GatewayChannel {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireGatewayChannel::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::GatewayMessage")]
    #[serde(rename_all = "camelCase")]
    struct WireGatewayMessage {
        #[serde(rename = "type")]
        message_type: GatewayMessageType,
        exchange_id: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "present"
        )]
        channel: Option<GatewayChannel>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "present"
        )]
        symbol: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "present"
        )]
        interval: Option<AnyTimeframe>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::retained_json::deserialize_present_retained_json"
        )]
        data: Option<serde_json::Value>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "present"
        )]
        error: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "present"
        )]
        request_id: Option<String>,
    }

    impl<'de> serde::Deserialize<'de> for super::GatewayMessage {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireGatewayMessage::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::DataTag")]
    #[serde(rename_all = "lowercase")]
    enum WireDataTag {
        Data,
    }

    impl<'de> serde::Deserialize<'de> for super::DataTag {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireDataTag::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::TickerChannel")]
    #[serde(rename_all = "lowercase")]
    enum WireTickerChannel {
        Ticker,
        Tickers,
    }

    impl<'de> serde::Deserialize<'de> for super::TickerChannel {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireTickerChannel::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderbookChannel")]
    #[serde(rename_all = "lowercase")]
    enum WireOrderbookChannel {
        Orderbook,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderbookChannel {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderbookChannel::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::CandleChannel")]
    #[serde(rename_all = "lowercase")]
    enum WireCandleChannel {
        Candle,
    }

    impl<'de> serde::Deserialize<'de> for super::CandleChannel {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireCandleChannel::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::MarketChannel")]
    #[serde(rename_all = "lowercase")]
    enum WireMarketChannel {
        Market,
    }

    impl<'de> serde::Deserialize<'de> for super::MarketChannel {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireMarketChannel::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::GatewayTickerUpdate")]
    #[serde(rename_all = "camelCase")]
    struct WireGatewayTickerUpdate {
        #[serde(rename = "type")]
        message_type: DataTag,
        exchange_id: String,
        channel: TickerChannel,
        data: GatewayTickerData,
    }

    impl<'de> serde::Deserialize<'de> for super::GatewayTickerUpdate {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireGatewayTickerUpdate::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::GatewayOrderbookUpdate")]
    #[serde(rename_all = "camelCase")]
    struct WireGatewayOrderbookUpdate {
        #[serde(rename = "type")]
        message_type: DataTag,
        exchange_id: String,
        channel: OrderbookChannel,
        symbol: String,
        data: OrderBook,
    }

    impl<'de> serde::Deserialize<'de> for super::GatewayOrderbookUpdate {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireGatewayOrderbookUpdate::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::GatewayCandleUpdate")]
    #[serde(rename_all = "camelCase")]
    struct WireGatewayCandleUpdate {
        #[serde(rename = "type")]
        message_type: DataTag,
        exchange_id: String,
        channel: CandleChannel,
        symbol: String,
        interval: Timeframe,
        data: Candle,
    }

    impl<'de> serde::Deserialize<'de> for super::GatewayCandleUpdate {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireGatewayCandleUpdate::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::GatewayMarketUpdate")]
    #[serde(rename_all = "camelCase")]
    struct WireGatewayMarketUpdate {
        #[serde(rename = "type")]
        message_type: DataTag,
        exchange_id: String,
        channel: MarketChannel,
        data: Vec<Market>,
    }

    impl<'de> serde::Deserialize<'de> for super::GatewayMarketUpdate {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireGatewayMarketUpdate::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[cfg(test)]
    mod shape_tests {
        use super::*;
        #[test]
        fn every_shared_codec_has_a_narrow_input_shape() {
            crate::strict_decode::tests::string_union::<GatewayMessageType>(
                "GatewayMessageType",
                WireGatewayMessageType::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(GatewayMessageType::Subscribe)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayMessageType::Unsubscribe)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayMessageType::Subscribed)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayMessageType::Unsubscribed)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayMessageType::Data)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayMessageType::Error)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayMessageType::Ping)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayMessageType::Pong)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<GatewayChannel>(
                "GatewayChannel",
                WireGatewayChannel::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(GatewayChannel::Auth)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayChannel::Ticker)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayChannel::Tickers)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayChannel::Orderbook)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayChannel::Candle)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayChannel::Trade)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(GatewayChannel::Market)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::record::<GatewayMessage>("GatewayMessage");
            crate::strict_decode::tests::string_union::<DataTag>(
                "DataTag",
                WireDataTag::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(DataTag::Data)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<TickerChannel>(
                "TickerChannel",
                WireTickerChannel::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(TickerChannel::Ticker)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(TickerChannel::Tickers)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<OrderbookChannel>(
                "OrderbookChannel",
                WireOrderbookChannel::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderbookChannel::Orderbook)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<CandleChannel>(
                "CandleChannel",
                WireCandleChannel::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(CandleChannel::Candle)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<MarketChannel>(
                "MarketChannel",
                WireMarketChannel::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(MarketChannel::Market)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::record::<GatewayTickerUpdate>("GatewayTickerUpdate");
            crate::strict_decode::tests::record::<GatewayOrderbookUpdate>("GatewayOrderbookUpdate");
            crate::strict_decode::tests::record::<GatewayCandleUpdate>("GatewayCandleUpdate");
            crate::strict_decode::tests::record::<GatewayMarketUpdate>("GatewayMarketUpdate");
        }
    }
}
