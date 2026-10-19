//! Borrowed records for the public swap feeds.
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;

pub fn number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Number<'a> {
        Bare(f64),
        Text(#[serde(borrow)] Str<'a>),
    }
    let n = match Number::deserialize(d)? {
        Number::Bare(n) => n,
        Number::Text(s) => s.parse().map_err(serde::de::Error::custom)?,
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err(serde::de::Error::custom("non-finite number"))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub asset: Str<'a>,
    #[serde(borrow)]
    pub currency: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub status: f64,
    #[serde(deserialize_with = "number")]
    pub quantity_precision: f64,
    #[serde(deserialize_with = "number")]
    pub price_precision: f64,
    #[serde(deserialize_with = "number")]
    pub trade_min_limit: f64,
    #[serde(default, deserialize_with = "number")]
    pub max_long_leverage: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Daily<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub last_price: f64,
    #[serde(deserialize_with = "number")]
    pub price_change_percent: f64,
    #[serde(deserialize_with = "number")]
    pub volume: f64,
    #[serde(deserialize_with = "number")]
    pub quote_volume: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Premium<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub mark_price: f64,
    #[serde(deserialize_with = "number")]
    pub index_price: f64,
    #[serde(deserialize_with = "number")]
    pub last_funding_rate: f64,
    #[serde(deserialize_with = "number")]
    pub next_funding_time: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTicker<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(default, deserialize_with = "number")]
    pub trade_price: f64,
    #[serde(deserialize_with = "number")]
    pub fair_price: f64,
    #[serde(deserialize_with = "number")]
    pub index_price: f64,
    #[serde(deserialize_with = "number")]
    pub funding_rate: f64,
    #[serde(deserialize_with = "number")]
    pub change_percentage: f64,
    #[serde(rename = "volumeF", deserialize_with = "number")]
    pub volume: f64,
    #[serde(rename = "valueF", deserialize_with = "number")]
    pub value: f64,
}

#[derive(Deserialize)]
pub struct Level {
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub volume: f64,
}

#[derive(Deserialize)]
pub struct Kline {
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
impl From<Kline> for safe_cex_engine::model::Candle {
    fn from(k: Kline) -> Self {
        Self {
            timestamp: k.time / 1000.,
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Depth {
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandleData {
    pub kline_infos_vo: Vec<Kline>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message<'a> {
    #[serde(default, borrow)]
    pub data_type: Str<'a>,
    #[serde(borrow)]
    pub data: Option<&'a serde_json::value::RawValue>,
    pub ping: Option<serde::de::IgnoredAny>,
    pub pong: Option<serde::de::IgnoredAny>,
}
