//! Borrowed public rows; numeric strings are decoded directly.
pub use safe_cex_engine::de::{number, number_or_blank_zero, optional_number};
use serde::Deserialize;
use std::borrow::Cow;
/// Public ticker overlay: null is absent, blank reads as 0, malformed skips the row.
fn public_optional_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    struct N(#[serde(deserialize_with = "number_or_blank_zero")] f64);
    Ok(Option::<N>::deserialize(d)?.map(|n| n.0))
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(default, borrow)]
    pub result: Cow<'a, str>,
    #[serde(default, borrow)]
    pub error: Cow<'a, str>,
    #[serde(default, borrow)]
    pub status: Cow<'a, str>,
}
pub fn success(response: safe_cex_engine::Response<'_>) -> bool {
    (200..300).contains(&response.status)
        && serde_json::from_str::<Envelope<'_>>(response.body).is_ok_and(|s| {
            s.result == "success"
                && s.error.is_empty()
                && (s.status.is_empty() || s.status == "success")
        })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(default, borrow)]
    pub base: Cow<'a, str>,
    #[serde(default, borrow)]
    pub quote: Cow<'a, str>,
    #[serde(default)]
    pub tradeable: Option<bool>,
    #[serde(default, borrow)]
    pub status: Cow<'a, str>,
    #[serde(default, deserialize_with = "optional_number")]
    pub contract_value_trade_precision: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub tick_size: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub contract_size: Option<f64>,
    #[serde(default)]
    pub margin_levels: Vec<MarginLevel>,
    #[serde(default)]
    pub last_trading_time: Option<serde::de::IgnoredAny>,
    #[serde(default, borrow)]
    pub contract_type: Cow<'a, str>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginLevel {
    #[serde(deserialize_with = "number")]
    pub initial_margin: f64,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestTicker<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub bid: Option<f64>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub ask: Option<f64>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub last: Option<f64>,
    #[serde(default, deserialize_with = "number_or_blank_zero")]
    pub mark_price: f64,
    #[serde(default, deserialize_with = "number_or_blank_zero")]
    pub index_price: f64,
    #[serde(default, deserialize_with = "number_or_blank_zero")]
    pub change24h: f64,
    #[serde(default, deserialize_with = "number_or_blank_zero")]
    pub funding_rate: f64,
    #[serde(default, deserialize_with = "number_or_blank_zero")]
    pub open_interest: f64,
    #[serde(default, deserialize_with = "number_or_blank_zero")]
    pub vol24h: f64,
    #[serde(default, deserialize_with = "number_or_blank_zero")]
    pub volume_quote: f64,
}
#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(default, borrow)]
    pub event: Cow<'a, str>,
    #[serde(default, borrow)]
    pub feed: Cow<'a, str>,
    #[serde(default, borrow, alias = "symbol")]
    pub product_id: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct WsTicker {
    #[serde(default, alias = "best_bid", deserialize_with = "optional_number")]
    pub bid: Option<f64>,
    #[serde(default, alias = "best_ask", deserialize_with = "optional_number")]
    pub ask: Option<f64>,
    #[serde(
        default,
        alias = "price",
        alias = "last_trade",
        deserialize_with = "optional_number"
    )]
    pub last: Option<f64>,
    #[serde(default, alias = "markPrice", deserialize_with = "optional_number")]
    pub mark: Option<f64>,
    #[serde(default, alias = "indexPrice", deserialize_with = "optional_number")]
    pub index: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub relative_funding_rate: Option<f64>,
    #[serde(
        default,
        alias = "next_funding",
        alias = "nextFundingTime",
        deserialize_with = "optional_number"
    )]
    pub next_funding_rate_time: Option<f64>,
    #[serde(
        default,
        alias = "open24h",
        alias = "open",
        deserialize_with = "optional_number"
    )]
    pub open_24h: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub change: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub volume: Option<f64>,
    #[serde(default, rename = "volumeQuote", deserialize_with = "optional_number")]
    pub volume_quote: Option<f64>,
    #[serde(default, rename = "openInterest", deserialize_with = "optional_number")]
    pub open_interest: Option<f64>,
}
#[derive(Deserialize)]
pub struct Level {
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub qty: f64,
}
#[derive(Deserialize)]
pub struct Depth<'a> {
    pub seq: u64,
    #[serde(default)]
    pub bids: Option<Vec<Level>>,
    #[serde(default)]
    pub asks: Option<Vec<Level>>,
    #[serde(default, borrow)]
    pub side: Cow<'a, str>,
    #[serde(default, deserialize_with = "number")]
    pub price: f64,
    #[serde(default, deserialize_with = "number")]
    pub qty: f64,
}
#[derive(Deserialize)]
pub struct CandleRow {
    #[serde(deserialize_with = "number")]
    pub time: f64,
    #[serde(deserialize_with = "number")]
    pub open: f64,
    #[serde(deserialize_with = "number")]
    pub high: f64,
    #[serde(deserialize_with = "number")]
    pub low: f64,
    #[serde(deserialize_with = "number")]
    pub close: f64,
    #[serde(deserialize_with = "number")]
    pub volume: f64,
}
impl CandleRow {
    pub fn candle(self) -> safe_cex_engine::model::Candle {
        safe_cex_engine::model::Candle {
            timestamp: self.time / 1000.0,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
        }
    }
}

#[derive(Deserialize)]
pub struct CandleEnvelope<'a> {
    pub candles: Vec<serde::de::IgnoredAny>,
    #[serde(default)]
    pub more_candles: bool,
    #[serde(default, borrow)]
    pub error: Cow<'a, str>,
    #[serde(default, borrow)]
    pub status: Cow<'a, str>,
}
