//! Borrowed wire records; numeric strings decode directly into numbers.
use serde::Deserialize;
use std::borrow::Cow;

pub fn number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Number(f64),
        Text(#[serde(borrow)] Cow<'a, str>),
    }
    let n = match N::deserialize(d)? {
        N::Number(n) => Ok(n),
        N::Text(s) => s.parse().map_err(serde::de::Error::custom),
    }?;
    if n.is_finite() {
        Ok(n)
    } else {
        Err(serde::de::Error::custom("nonfinite number"))
    }
}
pub fn optional_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    struct N(#[serde(deserialize_with = "number")] f64);
    Ok(Option::<N>::deserialize(d)?.map(|n| n.0))
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow)]
    pub channel: Cow<'a, str>,
    #[serde(borrow)]
    pub data: &'a serde_json::value::RawValue,
}
#[derive(Deserialize)]
pub struct Meta {
    pub universe: Vec<Instrument>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument {
    pub name: String,
    pub sz_decimals: i32,
    #[serde(default = "one")]
    pub max_leverage: f64,
    #[serde(default)]
    pub is_delisted: bool,
}
fn one() -> f64 {
    1.0
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetCtx {
    #[serde(default, deserialize_with = "optional_number")]
    pub mid_px: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    pub mark_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub oracle_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub prev_day_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub day_ntl_vlm: f64,
    #[serde(default, deserialize_with = "number")]
    pub day_base_vlm: f64,
    #[serde(default, deserialize_with = "number")]
    pub open_interest: f64,
    #[serde(default, deserialize_with = "number")]
    pub funding: f64,
}
#[derive(Deserialize)]
pub struct Dex {
    pub name: String,
}
#[derive(Deserialize)]
pub struct Mids<'a> {
    #[serde(borrow)]
    pub mids: std::collections::HashMap<Cow<'a, str>, Cow<'a, str>>,
}
#[derive(Clone, Deserialize)]
pub struct Level {
    #[serde(deserialize_with = "number")]
    pub px: f64,
    #[serde(deserialize_with = "number")]
    pub sz: f64,
}
#[derive(Deserialize)]
pub struct Depth<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    pub time: f64,
    pub levels: [Vec<Level>; 2],
}
#[derive(Deserialize)]
pub struct Bbo<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    pub time: f64,
    pub bbo: [Option<Level>; 2],
}
#[derive(Deserialize)]
pub struct CandleRow<'a> {
    pub t: f64,
    #[serde(default, borrow)]
    pub s: Cow<'a, str>,
    #[serde(default, borrow)]
    pub i: Cow<'a, str>,
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
impl CandleRow<'_> {
    pub fn candle(&self) -> safe_cex_engine::model::Candle {
        safe_cex_engine::model::Candle {
            timestamp: self.t / 1000.0,
            open: self.o,
            high: self.h,
            low: self.l,
            close: self.c,
            volume: self.v,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FastCtx {
    #[serde(default, deserialize_with = "optional_number")]
    pub mark_px: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub mid_px: Option<f64>,
}
