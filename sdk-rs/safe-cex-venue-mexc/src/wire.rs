//! Borrowed MEXC futures records. Numeric identifiers never pass through f64.
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;

pub fn number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Bare(f64),
        Text(#[serde(borrow)] Str<'a>),
    }
    let n = match N::deserialize(d)? {
        N::Bare(n) => n,
        N::Text(n) => n.parse().map_err(serde::de::Error::custom)?,
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err(serde::de::Error::custom("non-finite number"))
    }
}

#[derive(Deserialize)]
pub struct Envelope<'a> {
    pub success: Option<bool>,
    #[serde(default, deserialize_with = "number")]
    pub code: f64,
    #[serde(borrow, default, alias = "msg")]
    pub message: Str<'a>,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
}
impl<'a> Envelope<'a> {
    pub fn checked(body: &'a str) -> Result<Self, String> {
        let e: Self = serde_json::from_str(body).map_err(|e| format!("MEXC response: {e}"))?;
        if e.success != Some(true) || e.code != 0.0 {
            return Err(format!("MEXC {}: {}", e.code, e.message));
        }
        Ok(e)
    }
    pub fn payload(&self) -> Result<&'a str, String> {
        self.data
            .map(RawValue::get)
            .ok_or_else(|| "MEXC response has no data".into())
    }
}

#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(borrow, default)]
    pub channel: Str<'a>,
    #[serde(borrow, default)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
    pub success: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub base_coin: Str<'a>,
    #[serde(borrow)]
    pub quote_coin: Str<'a>,
    #[serde(borrow, default)]
    pub settle_coin: Str<'a>,
    pub api_allowed: Option<bool>,
    pub is_hidden: Option<bool>,
    #[serde(default, deserialize_with = "number")]
    pub state: f64,
    #[serde(deserialize_with = "number")]
    pub contract_size: f64,
    #[serde(deserialize_with = "number")]
    pub vol_unit: f64,
    #[serde(deserialize_with = "number")]
    pub price_unit: f64,
    #[serde(deserialize_with = "number")]
    pub min_vol: f64,
    #[serde(deserialize_with = "number")]
    pub max_vol: f64,
    #[serde(deserialize_with = "number")]
    pub min_leverage: f64,
    #[serde(deserialize_with = "number")]
    pub max_leverage: f64,
    #[serde(default, deserialize_with = "number")]
    pub maker_fee_rate: f64,
    #[serde(default, deserialize_with = "number")]
    pub taker_fee_rate: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tick<'a> {
    #[serde(borrow, default)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub last_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub fair_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub index_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub bid1: f64,
    #[serde(default, deserialize_with = "number")]
    pub ask1: f64,
    #[serde(default, deserialize_with = "number")]
    pub rise_fall_rate: f64,
    #[serde(default, deserialize_with = "number")]
    pub hold_vol: f64,
    #[serde(default, deserialize_with = "number")]
    pub funding_rate: f64,
    #[serde(default, alias = "nextSettleTime", deserialize_with = "number")]
    pub funding_time: f64,
    #[serde(default, deserialize_with = "number")]
    pub volume24: f64,
    #[serde(default, deserialize_with = "number")]
    pub amount24: f64,
}

#[derive(Deserialize)]
pub struct Depth {
    pub version: u64,
    #[serde(default)]
    pub bids: Vec<Level>,
    #[serde(default)]
    pub asks: Vec<Level>,
}
#[derive(Deserialize)]
pub struct Level(
    #[serde(deserialize_with = "number")] pub f64,
    #[serde(deserialize_with = "number")] pub f64,
    #[serde(default)] pub serde::de::IgnoredAny,
);

#[derive(Deserialize)]
pub struct Kline<'a> {
    #[serde(default, alias = "time", deserialize_with = "number")]
    pub t: f64,
    #[serde(alias = "open", deserialize_with = "number")]
    pub o: f64,
    #[serde(alias = "high", deserialize_with = "number")]
    pub h: f64,
    #[serde(alias = "low", deserialize_with = "number")]
    pub l: f64,
    #[serde(alias = "close", deserialize_with = "number")]
    pub c: f64,
    #[serde(default, alias = "vol", deserialize_with = "number")]
    pub q: f64,
    #[serde(borrow, default)]
    pub interval: Str<'a>,
}
#[derive(Deserialize)]
pub struct Klines {
    pub time: Vec<f64>,
    pub open: Vec<f64>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    pub vol: Vec<f64>,
}

pub fn millis(t: f64) -> f64 {
    if t >= 1e12 {
        t.floor()
    } else {
        (t * 1000.0).floor()
    }
}
pub fn compact(s: &str) -> String {
    s.replace('_', "")
}
pub fn interval(s: &str) -> Option<&'static str> {
    Some(match s {
        "1m" => "Min1",
        "5m" => "Min5",
        "15m" => "Min15",
        "30m" => "Min30",
        "1h" => "Min60",
        "4h" => "Hour4",
        "1d" => "Day1",
        "1w" => "Week1",
        _ => return None,
    })
}
