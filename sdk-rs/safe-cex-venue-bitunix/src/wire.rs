//! Borrowed Bitunix wire records. No intermediate JSON object graph.
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};
use std::{borrow::Cow, fmt};
pub type Str<'a> = Cow<'a, str>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Num(pub f64);
impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Number;
        impl Visitor<'_> for Number {
            type Value = Num;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a number or decimal string")
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Num, E> {
                Ok(Num(if v.is_finite() { v } else { 0.0 }))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Num, E> {
                Ok(Num(v as f64))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Num, E> {
                Ok(Num(v as f64))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Num, E> {
                Ok(Num(safe_cex_engine::num::parse(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Num, E> {
                Ok(Num(0.0))
            }
        }
        d.deserialize_any(Number)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum Supported<'a> {
    Bool(bool),
    Number(f64),
    Text(#[serde(borrow)] Str<'a>),
}
impl Supported<'_> {
    pub fn allowed(&self) -> bool {
        match self {
            Self::Bool(v) => *v,
            Self::Number(v) => *v != 0.0,
            Self::Text(v) => !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "false" | "0" | "no" | "n" | "disabled" | "unsupported"
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pair<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub base: Str<'a>,
    #[serde(borrow)]
    pub quote: Str<'a>,
    #[serde(borrow)]
    pub symbol_status: Str<'a>,
    pub base_precision: Num,
    pub quote_precision: Num,
    pub max_limit_order_volume: Num,
    #[serde(borrow, default)]
    pub is_api_supported: Option<Supported<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Daily<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    pub last: Num,
    pub open: Num,
    pub mark_price: Num,
    pub base_vol: Num,
    pub quote_vol: Num,
}
#[derive(Deserialize, Default)]
pub struct Message<'a> {
    #[serde(borrow, default)]
    pub ch: Str<'a>,
    #[serde(borrow, default)]
    pub channel: Str<'a>,
    #[serde(borrow, default)]
    pub symbol: Str<'a>,
    #[serde(borrow, default)]
    pub op: Str<'a>,
    #[serde(borrow, default)]
    pub data: Option<&'a serde_json::value::RawValue>,
}
#[derive(Deserialize)]
pub struct Tick {
    pub la: Num,
    pub o: Num,
    pub b: Num,
    pub q: Num,
}
#[derive(Deserialize)]
pub struct Price {
    pub mp: Num,
    pub ip: Num,
    pub fr: Num,
    pub ft: Num,
}
#[derive(Deserialize)]
pub struct Depth {
    pub bids: Vec<[Num; 2]>,
    pub asks: Vec<[Num; 2]>,
}
#[derive(Deserialize)]
pub struct Top {
    pub b: Vec<[Num; 2]>,
    pub a: Vec<[Num; 2]>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kline {
    pub time: Num,
    pub open: Num,
    pub high: Num,
    pub low: Num,
    pub close: Num,
    #[serde(default)]
    pub volume: Num,
    #[serde(default)]
    pub quote_vol: Num,
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    pub code: Num,
    #[serde(borrow, default)]
    pub msg: Str<'a>,
    #[serde(borrow, default)]
    pub data: Option<&'a serde_json::value::RawValue>,
}
pub fn data(text: &str) -> Result<&str, String> {
    let e: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if e.code.0 != 0.0 {
        return Err(e.msg.into_owned());
    }
    e.data
        .map(|r| r.get())
        .ok_or_else(|| "Bitunix response has no data".into())
}
