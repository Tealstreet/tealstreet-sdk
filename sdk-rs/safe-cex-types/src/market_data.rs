//! Public market data contracts. Book levels remain mutable in generated TS.
use crate::OrderSide;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct Candle {
    pub timestamp: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_side: Option<OrderSide>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrderBookOrders {
    /// @ts-mutable
    pub price: f64,
    /// @ts-mutable
    pub amount: f64,
    /// @ts-mutable
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrderBook {
    /// @ts-mutable
    pub bids: Vec<OrderBookOrders>,
    /// @ts-mutable
    pub asks: Vec<OrderBookOrders>,
}

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

mod strict_codecs {
    use super::{OrderBookOrders, OrderSide, deserialize_present};
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(remote = "super::Candle")]
    #[serde(rename_all = "camelCase")]
    struct WireCandle {
        timestamp: f64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        last_side: Option<OrderSide>,
    }

    impl<'de> Deserialize<'de> for super::Candle {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireCandle::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(Deserialize)]
    #[serde(remote = "super::OrderBookOrders")]
    #[serde(rename_all = "camelCase")]
    struct WireOrderBookOrders {
        price: f64,
        amount: f64,
        total: f64,
    }

    impl<'de> Deserialize<'de> for super::OrderBookOrders {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOrderBookOrders::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(Deserialize)]
    #[serde(remote = "super::OrderBook")]
    #[serde(rename_all = "camelCase")]
    struct WireOrderBook {
        bids: Vec<OrderBookOrders>,
        asks: Vec<OrderBookOrders>,
    }

    impl<'de> Deserialize<'de> for super::OrderBook {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOrderBook::deserialize(crate::strict_decode::object(deserializer))
        }
    }
}
