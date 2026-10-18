//! Retained route declaration; test-only comparison bodies are retired.
use serde::Serialize;

// ts: WooV3PublicParityRouteId
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub enum WooV3PublicParityRouteId {
    #[serde(rename = "markets")]
    Markets,
    #[serde(rename = "tickers")]
    Tickers,
    #[serde(rename = "orderbook")]
    Orderbook,
    #[serde(rename = "recent-klines")]
    RecentKlines,
    #[serde(rename = "historical-klines")]
    HistoricalKlines,
}
impl<'de> serde::Deserialize<'de> for WooV3PublicParityRouteId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        #[derive(serde::Deserialize)]
        #[serde(remote = "WooV3PublicParityRouteId")]
        enum Wire {
            #[serde(rename = "markets")]
            Markets,
            #[serde(rename = "tickers")]
            Tickers,
            #[serde(rename = "orderbook")]
            Orderbook,
            #[serde(rename = "recent-klines")]
            RecentKlines,
            #[serde(rename = "historical-klines")]
            HistoricalKlines,
        }
        Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
