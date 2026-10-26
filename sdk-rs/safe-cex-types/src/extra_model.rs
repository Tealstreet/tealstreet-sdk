//! Remaining standalone models and public data-source capabilities.
use crate::{
    Balance, Candle, FetchedLeverage, Market, OHLCVOptions, Order, OrderBook, OrderSide,
    OrderStatus, Position, StoreDataLoaded, StoreOptions, Ticker,
};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;
use std::sync::LazyLock;

pub static TIMEFRAME_TO_SECONDS: LazyLock<IndexMap<&'static str, f64>> = LazyLock::new(|| {
    [
        ("1s", 1.0),
        ("5s", 5.0),
        ("15s", 15.0),
        ("30s", 30.0),
        ("1m", 60.0),
        ("3m", 180.0),
        ("5m", 300.0),
        ("15m", 900.0),
        ("30m", 1800.0),
        ("1h", 3600.0),
        ("2h", 7200.0),
        ("3h", 10800.0),
        ("4h", 14400.0),
        ("5h", 18000.0),
        ("6h", 21600.0),
        ("7h", 25200.0),
        ("8h", 28800.0),
        ("9h", 32400.0),
        ("10h", 36000.0),
        ("11h", 39600.0),
        ("12h", 43200.0),
        ("24h", 86400.0),
        ("1d", 86400.0),
        ("2d", 172800.0),
        ("3d", 259200.0),
        ("1w", 604800.0),
        ("1M", 2592000.0),
    ]
    .into_iter()
    .collect()
});
pub const READ_ONLY_ORDER_STATUSES: [OrderStatus; 3] = [
    OrderStatus::Creating,
    OrderStatus::Updating,
    OrderStatus::Canceling,
];
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderFillEvent {
    pub id: String,
    pub amount: f64,
    pub price: f64,
    pub side: OrderSide,
    pub symbol: String,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub order_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub realized_pnl: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub fee: Option<f64>,
}
/// Rust model mutability is controlled by ownership and mutable borrows.
pub type Writable<T> = T;
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WritableStoreData {
    pub latency: f64,
    pub balance: Balance,
    pub markets: Vec<Market>,
    pub tickers: Vec<Ticker>,
    pub orders: Vec<Order>,
    pub positions: Vec<Position>,
    pub loaded: StoreDataLoaded,
    pub options: StoreOptions,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub user_data: Option<Value>,
    pub fetched_leverage: IndexMap<String, FetchedLeverage>,
}
pub trait PublicDataStore {
    fn subscribe(&self, callback: crate::PublicDataSubscriber) -> Box<dyn Fn()>;
    fn tickers(&self) -> Result<Vec<Ticker>, crate::StoreReadError>;
    fn markets(&self) -> Result<Vec<Market>, crate::StoreReadError>;
    fn loaded(&self) -> Result<StoreDataLoaded, crate::StoreReadError>;
}
pub trait PublicDataSource {
    fn store(&self) -> &dyn PublicDataStore;
    fn listen_order_book(&self, symbol: &str, callback: Box<dyn Fn(OrderBook)>) -> Box<dyn Fn()>;
    fn listen_ohlcv(&self, options: OHLCVOptions, callback: Box<dyn Fn(Candle)>) -> Box<dyn Fn()>;
}

// Private wire companions keep serde metadata without exposing permissive decoders.
mod strict_codecs {
    use super::{
        Balance, FetchedLeverage, IndexMap, Market, Order, OrderSide, Position, StoreDataLoaded,
        StoreOptions, Ticker, Value,
    };
    #[cfg(test)]
    use super::{OrderFillEvent, WritableStoreData};
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderFillEvent")]
    #[serde(rename_all = "camelCase")]
    struct WireOrderFillEvent {
        id: String,
        amount: f64,
        price: f64,
        side: OrderSide,
        symbol: String,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        order_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        realized_pnl: Option<f64>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        fee: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderFillEvent {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOrderFillEvent::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::WritableStoreData")]
    #[serde(rename_all = "camelCase")]
    struct WireWritableStoreData {
        latency: f64,
        balance: Balance,
        markets: Vec<Market>,
        tickers: Vec<Ticker>,
        orders: Vec<Order>,
        positions: Vec<Position>,
        loaded: StoreDataLoaded,
        options: StoreOptions,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        user_data: Option<Value>,
        fetched_leverage: IndexMap<String, FetchedLeverage>,
    }

    impl<'de> serde::Deserialize<'de> for super::WritableStoreData {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireWritableStoreData::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[cfg(test)]
    mod shape_tests {
        use super::*;
        #[test]
        fn every_shared_codec_has_a_narrow_input_shape() {
            crate::strict_decode::tests::record::<OrderFillEvent>("OrderFillEvent");
            crate::strict_decode::tests::record::<WritableStoreData>("WritableStoreData");
        }
    }
}
