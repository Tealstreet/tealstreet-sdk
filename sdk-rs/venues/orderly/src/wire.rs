use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::borrow::Cow;

pub fn number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Number(f64),
        Text(#[serde(borrow)] Cow<'a, str>),
        Null,
    }
    let n = match N::deserialize(d)? {
        N::Number(n) => n,
        N::Text(s) => s.parse().map_err(serde::de::Error::custom)?,
        N::Null => return Err(serde::de::Error::custom("null number")),
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err(serde::de::Error::custom("nonfinite number"))
    }
}

#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(default, borrow)]
    pub event: Cow<'a, str>,
    #[serde(default, borrow)]
    pub topic: Cow<'a, str>,
    #[serde(default, borrow)]
    pub id: Cow<'a, str>,
    pub success: Option<bool>,
    #[serde(default)]
    pub ts: u64,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
    #[serde(default, borrow)]
    pub message: Cow<'a, str>,
}

#[derive(Deserialize)]
pub struct Rest<'a> {
    pub success: bool,
    #[serde(default, borrow)]
    pub message: Cow<'a, str>,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
}
pub fn checked(response: safe_cex_engine::Response<'_>) -> Result<Rest<'_>, String> {
    let r: Rest<'_> = serde_json::from_str(response.body).map_err(|e| e.to_string())?;
    if !(200..300).contains(&response.status) || !r.success || r.data.is_none() {
        return Err(format!("Orderly HTTP {}: {}", response.status, r.message));
    }
    Ok(r)
}

#[derive(Deserialize)]
pub struct MarketRow<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    pub base_tick: f64,
    #[serde(deserialize_with = "number")]
    pub quote_tick: f64,
    #[serde(deserialize_with = "number")]
    pub base_min: f64,
    #[serde(deserialize_with = "number")]
    pub base_max: f64,
    #[serde(default, deserialize_with = "number")]
    pub base_imr: f64,
    #[serde(default, deserialize_with = "number")]
    pub min_leverage: f64,
    #[serde(default, deserialize_with = "number")]
    pub max_leverage: f64,
}

#[derive(Deserialize)]
pub struct TickerRow<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(rename = "24h_open", deserialize_with = "number")]
    pub open: f64,
    #[serde(rename = "24h_close", deserialize_with = "number")]
    pub close: f64,
    #[serde(default, deserialize_with = "number")]
    pub index_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub mark_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub last_funding_rate: f64,
    #[serde(default, deserialize_with = "number")]
    pub next_funding_time: f64,
    #[serde(default, deserialize_with = "number")]
    pub open_interest: f64,
    #[serde(rename = "24h_volume", default, deserialize_with = "number")]
    pub volume: f64,
    #[serde(rename = "24h_amount", default, deserialize_with = "number")]
    pub amount: f64,
}

#[derive(Deserialize)]
pub struct LiveTicker<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(default, deserialize_with = "number")]
    pub close: f64,
    #[serde(default, deserialize_with = "number")]
    pub volume: f64,
    #[serde(default, deserialize_with = "number")]
    pub amount: f64,
    #[serde(default, deserialize_with = "number")]
    pub bid: f64,
    #[serde(default, deserialize_with = "number")]
    pub ask: f64,
    #[serde(default, deserialize_with = "number")]
    pub price: f64,
    #[serde(rename = "fundingRate", default, deserialize_with = "number")]
    pub funding_rate: f64,
    #[serde(rename = "fundingTs", default, deserialize_with = "number")]
    pub funding_time: f64,
}

#[derive(Deserialize)]
pub struct Depth {
    #[serde(rename = "prevTs")]
    pub previous: Option<u64>,
    pub symbol: Option<String>,
    #[serde(default)]
    pub ts: u64,
    #[serde(default)]
    pub timestamp: u64,
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
}
impl Depth {
    pub fn valid(&self, sorted: bool) -> bool {
        [&self.bids, &self.asks].iter().all(|side| {
            side.iter()
                .all(|(p, q)| p.is_finite() && *p > 0.0 && q.is_finite() && *q >= 0.0)
        }) && (!sorted
            || (self.bids.windows(2).all(|w| w[0].0 > w[1].0)
                && self.asks.windows(2).all(|w| w[0].0 < w[1].0)))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kline {
    #[serde(alias = "start_timestamp", deserialize_with = "number")]
    pub start_time: f64,
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
impl Kline {
    pub fn candle(self) -> Result<safe_cex_engine::model::Candle, String> {
        if !(0.0..=253402300799000.0).contains(&self.start_time)
            || self.volume < 0.0
            || self.open <= 0.0
            || self.close <= 0.0
            || self.low <= 0.0
            || self.high < self.low
            || self.high < self.open.max(self.close)
            || self.low > self.open.min(self.close)
        {
            return Err("Orderly invalid candle".into());
        }
        Ok(safe_cex_engine::model::Candle {
            timestamp: self.start_time / 1000.0,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
        })
    }
}
