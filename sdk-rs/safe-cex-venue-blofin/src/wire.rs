//! Borrowed BloFin payloads; unknown fields never enter the store.
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct Number(#[serde(deserialize_with = "number")] pub f64);
fn number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Bare(f64),
        Text(#[serde(borrow)] Str<'a>),
    }
    Ok(match N::deserialize(d)? {
        N::Bare(n) => n,
        N::Text(s) => safe_cex_engine::num::parse(&s),
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Instrument<'a> {
    #[serde(borrow)]
    pub inst_id: Str<'a>,
    #[serde(borrow)]
    pub inst_type: Str<'a>,
    #[serde(borrow)]
    pub contract_type: Str<'a>,
    #[serde(borrow)]
    pub base_currency: Str<'a>,
    #[serde(borrow)]
    pub quote_currency: Str<'a>,
    #[serde(borrow)]
    pub state: Str<'a>,
    pub contract_value: Number,
    pub min_size: Number,
    #[serde(default)]
    pub lot_size: Number,
    pub tick_size: Number,
    pub max_limit_size: Number,
    pub max_market_size: Number,
    pub max_leverage: Number,
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Tick<'a> {
    #[serde(borrow)]
    pub inst_id: Str<'a>,
    #[serde(default)]
    pub last: Number,
    #[serde(default)]
    pub open24h: Number,
    #[serde(default)]
    pub bid_price: Number,
    #[serde(default)]
    pub ask_price: Number,
    #[serde(default)]
    pub vol_currency24h: Number,
    #[serde(default)]
    pub vol24h: Number,
    #[serde(default)]
    pub funding_rate: Number,
    #[serde(default)]
    pub funding_time: Number,
}
#[derive(Deserialize, Default)]
pub struct Arg<'a> {
    #[serde(borrow, default)]
    pub channel: Str<'a>,
    #[serde(borrow, default, rename = "instId")]
    pub inst_id: Str<'a>,
}
#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(borrow, default)]
    pub arg: Arg<'a>,
    #[serde(borrow, default)]
    pub action: Str<'a>,
    #[serde(borrow, default)]
    pub event: Str<'a>,
    #[serde(borrow, default)]
    pub code: Str<'a>,
}
#[derive(Deserialize)]
pub struct Data<T> {
    pub data: T,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    Many(Vec<T>),
    One(T),
}
impl<T> OneOrMany<T> {
    pub fn first(&self) -> Option<&T> {
        match self {
            Self::Many(v) => v.first(),
            Self::One(v) => Some(v),
        }
    }
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Depth {
    pub bids: Vec<Vec<Number>>,
    pub asks: Vec<Vec<Number>>,
    #[serde(default)]
    pub seq_id: Option<Sequence>,
    #[serde(default)]
    pub prev_seq_id: Option<Sequence>,
    #[serde(default)]
    pub ts: Number,
}

#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow)]
    pub code: Str<'a>,
    #[serde(borrow, default)]
    pub msg: Str<'a>,
}
pub fn check(response: &safe_cex_engine::Response<'_>) -> Result<(), String> {
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "BloFin HTTP {}: {}",
            response.status, response.body
        ));
    }
    let envelope: Envelope<'_> = serde_json::from_str(response.body).map_err(|e| e.to_string())?;
    if envelope.code != "0" {
        return Err(format!("BloFin {}: {}", envelope.code, envelope.msg));
    }
    Ok(())
}

#[derive(Clone, Copy, Deserialize)]
#[serde(try_from = "SequenceWire<'_>")]
pub struct Sequence(pub u64);
#[derive(Deserialize)]
#[serde(untagged)]
enum SequenceWire<'a> {
    Bare(u64),
    Text(#[serde(borrow)] Str<'a>),
}
impl TryFrom<SequenceWire<'_>> for Sequence {
    type Error = String;
    fn try_from(value: SequenceWire<'_>) -> Result<Self, Self::Error> {
        match value {
            SequenceWire::Bare(v) => Ok(Self(v)),
            SequenceWire::Text(v) => v
                .parse()
                .map(Self)
                .map_err(|_| "Invalid book sequence".into()),
        }
    }
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum CandleWire {
    Row(Vec<Number>),
    Object(CandleObject),
}
#[derive(Deserialize)]
pub struct CandleObject {
    #[serde(alias = "timestamp", alias = "t", alias = "time")]
    pub ts: Number,
    #[serde(alias = "o")]
    pub open: Number,
    #[serde(alias = "h")]
    pub high: Number,
    #[serde(alias = "l")]
    pub low: Number,
    #[serde(alias = "c")]
    pub close: Number,
    #[serde(default, alias = "vol", alias = "v")]
    pub volume: Number,
}
impl CandleWire {
    pub fn candle(&self) -> Option<safe_cex_engine::model::Candle> {
        match self {
            Self::Row(row) => super::candle(row),
            Self::Object(c) if c.ts.0 > 0.0 => Some(safe_cex_engine::model::Candle {
                timestamp: if c.ts.0 > 1e12 {
                    c.ts.0 / 1000.0
                } else {
                    c.ts.0
                },
                open: c.open.0,
                high: c.high.0,
                low: c.low.0,
                close: c.close.0,
                volume: c.volume.0,
            }),
            _ => None,
        }
    }
}
