//! Borrowed quote records. Parsing rejects non-finite financial values.
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::borrow::Cow;

pub fn number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Text(Cow<'a, str>),
        Number(f64),
    }
    let n = match N::deserialize(d)? {
        N::Text(s) => safe_cex_engine::startup::number_or_zero(&s)
            .ok_or_else(|| serde::de::Error::custom("invalid public number"))?,
        N::Number(n) => n,
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err(serde::de::Error::custom("non-finite number"))
    }
}
pub fn optional_number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Text(Cow<'a, str>),
        Number(f64),
    }
    let Some(value) = Option::<N<'_>>::deserialize(d)? else {
        return Ok(None);
    };
    let value = match value {
        N::Text(text) if text.is_empty() => return Ok(None),
        N::Text(text) => text.parse().map_err(serde::de::Error::custom)?,
        N::Number(n) => n,
    };
    if value.is_finite() {
        Ok(Some(value))
    } else {
        Err(serde::de::Error::custom("non-finite number"))
    }
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Contract<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(borrow)]
    pub cross_symbol_name: Cow<'a, str>,
    #[serde(borrow)]
    pub base_token_id: Cow<'a, str>,
    #[serde(borrow)]
    pub settle_asset_id: Cow<'a, str>,
    pub enable_trade: bool,
    #[serde(deserialize_with = "number")]
    pub step_size: f64,
    #[serde(deserialize_with = "number")]
    pub tick_size: f64,
    #[serde(deserialize_with = "number")]
    pub min_order_size: f64,
    #[serde(deserialize_with = "number")]
    pub max_order_size: f64,
    #[serde(deserialize_with = "number")]
    pub display_min_leverage: f64,
    #[serde(deserialize_with = "number")]
    pub display_max_leverage: f64,
    #[serde(default, deserialize_with = "number")]
    pub l2_pair_id: f64,
    #[serde(default, deserialize_with = "number")]
    pub initial_margin_rate: f64,
    #[serde(default, deserialize_with = "number")]
    pub default_initial_margin_rate: f64,
    #[serde(default, deserialize_with = "number")]
    pub max_market_price_range: f64,
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(default, deserialize_with = "optional_number")]
    pub code: Option<f64>,
    #[serde(default, borrow)]
    pub data: Option<&'a RawValue>,
}
#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(default, borrow)]
    pub topic: Cow<'a, str>,
    #[serde(default, borrow)]
    pub op: Cow<'a, str>,
    #[serde(default, borrow, rename = "type")]
    pub kind: Cow<'a, str>,
    #[serde(default, borrow)]
    pub data: Option<&'a RawValue>,
}
#[derive(Deserialize)]
pub struct Tick<'a> {
    #[serde(borrow)]
    pub s: Cow<'a, str>,
    #[serde(default, deserialize_with = "optional_number")]
    pub p: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub mp: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub xp: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub pr: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub o: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub fr: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub v: Option<f64>,
    #[serde(default, deserialize_with = "optional_number", rename = "to")]
    pub turnover: Option<f64>,
}
#[derive(Deserialize)]
pub struct Level(
    #[serde(deserialize_with = "number")] pub f64,
    #[serde(deserialize_with = "number")] pub f64,
);
#[derive(Deserialize)]
pub struct Depth {
    pub u: u64,
    #[serde(default)]
    pub a: Vec<Level>,
    #[serde(default)]
    pub b: Vec<Level>,
}
#[derive(Deserialize)]
pub struct Candle {
    #[serde(deserialize_with = "number")]
    pub start: f64,
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
#[derive(Deserialize)]
pub struct Kline {
    #[serde(deserialize_with = "number")]
    pub t: f64,
    #[serde(deserialize_with = "number")]
    pub o: f64,
    #[serde(deserialize_with = "number")]
    pub h: f64,
    #[serde(deserialize_with = "number")]
    pub l: f64,
    #[serde(deserialize_with = "number")]
    pub c: f64,
    #[serde(deserialize_with = "number")]
    pub v: f64,
}
pub struct Decimal {
    pub value: f64,
    pub scale: u32,
}
impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum N<'a> {
            Text(Cow<'a, str>),
            Number(f64),
        }
        let (value, scale) = match N::deserialize(d)? {
            N::Number(n) => (n, safe_cex_engine::num::decimals(n)),
            N::Text(text) => {
                let value = text.parse::<f64>().map_err(serde::de::Error::custom)?;
                let (mantissa, exponent) = text
                    .split_once(['e', 'E'])
                    .map_or((text.as_ref(), 0), |(m, e)| {
                        (m, e.parse::<i32>().unwrap_or(0))
                    });
                let fraction = mantissa.split_once('.').map_or(0, |(_, d)| d.len() as i32);
                (value, fraction.saturating_sub(exponent).max(0) as u32)
            }
        };
        if !value.is_finite() || scale > 308 {
            return Err(serde::de::Error::custom("invalid decimal quote"));
        }
        Ok(Self { value, scale })
    }
}
#[derive(Deserialize)]
pub struct Trade {
    #[serde(deserialize_with = "number", rename = "T")]
    pub time: f64,
    pub p: Decimal,
    pub v: Decimal,
}
