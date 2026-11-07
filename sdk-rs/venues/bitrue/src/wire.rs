//! Borrowed Bitrue fields; numeric identifiers never pass through floats.
use serde::{Deserialize, Deserializer, de};
use std::{borrow::Cow, fmt, ops::Deref};
pub type Str<'a> = Cow<'a, str>;
pub use safe_cex_engine::de::Number;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Text<'a>(pub Cow<'a, str>);
impl Deref for Text<'_> {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for Text<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl<'de: 'a, 'a> Deserialize<'de> for Text<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Text<'de>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("text or integer identifier")
            }
            fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
                Ok(Text(Cow::Borrowed(v)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(Text(Cow::Owned(v.to_owned())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(Text(Cow::Owned(v)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(Text(Cow::Owned(v.to_string())))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(Text(Cow::Owned(v.to_string())))
            }
        }
        d.deserialize_any(Visitor)
    }
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow)]
    pub code: Text<'a>,
    #[serde(borrow, default)]
    pub msg: Str<'a>,
}
pub fn check(response: &safe_cex_engine::Response<'_>) -> Result<(), String> {
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "Bitrue HTTP {}: {}",
            response.status, response.body
        ));
    }
    let e: Envelope<'_> = serde_json::from_str(response.body).map_err(|e| e.to_string())?;
    if matches!(&*e.code, "0" | "200") {
        Ok(())
    } else {
        Err(format!("Bitrue {}: {}", e.code, e.msg))
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct PublicNumber(#[serde(deserialize_with = "public_number")] pub f64);
fn public_number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Number(f64),
        Text(#[serde(borrow)] Cow<'a, str>),
    }
    let n = match N::deserialize(d)? {
        N::Number(n) => Some(n).filter(|n| n.is_finite()),
        N::Text(s) => safe_cex_engine::startup::number_or_zero(&s),
    };
    n.ok_or_else(|| serde::de::Error::custom("invalid public number"))
}

/// The identity of a contract row that failed to decode.
#[derive(Deserialize)]
pub struct ContractKey<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    pub price_precision: u32,
    pub multiplier: PublicNumber,
    pub min_order_volume: PublicNumber,
    pub max_market_volume: PublicNumber,
    #[serde(default)]
    pub max_limit_volume: PublicNumber,
    #[serde(default)]
    pub min_order_money: PublicNumber,
    pub status: PublicNumber,
}
fn present_number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Number>, D::Error> {
    Number::deserialize(d).map(Some)
}
// Decode the public tick once; only candles need their extra numeric fields.
#[derive(Deserialize)]
pub struct Payload<'a> {
    #[serde(default, alias = "last", deserialize_with = "present_number")]
    pub close: Option<Number>,
    #[serde(default)]
    pub rose: Number,
    #[serde(default)]
    pub vol: Number,
    #[serde(default)]
    pub amount: Number,
    #[serde(default, alias = "bids")]
    pub buys: Option<Vec<[Number; 2]>>,
    #[serde(default)]
    pub asks: Option<Vec<[Number; 2]>>,
    #[serde(borrow, default)]
    pub ds: Str<'a>,
    #[serde(default)]
    pub id: Number,
    #[serde(borrow, default)]
    pub open: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    pub high: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    pub low: Option<&'a serde_json::value::RawValue>,
}
impl<'a> Payload<'a> {
    pub fn into_candle(self) -> Option<CandleWire<'a>> {
        let number = |raw: Option<&serde_json::value::RawValue>| {
            serde_json::from_str::<Number>(raw?.get()).ok()
        };
        Some(CandleWire {
            ds: self.ds,
            id: self.id,
            open: number(self.open)?,
            high: number(self.high)?,
            low: number(self.low)?,
            close: self.close?,
            vol: self.vol,
        })
    }
}
#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(borrow, default)]
    pub channel: Str<'a>,
    #[serde(borrow, default)]
    pub event: Str<'a>,
    #[serde(borrow, default)]
    pub event_rep: Str<'a>,
    #[serde(borrow, default)]
    pub cb_id: Str<'a>,
    #[serde(borrow, default)]
    pub status: Str<'a>,
    #[serde(borrow, default)]
    pub ping: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    pub tick: Option<Payload<'a>>,
    #[serde(borrow, default)]
    pub data: Option<&'a serde_json::value::RawValue>,
}
#[derive(Deserialize)]
pub struct CandleWire<'a> {
    #[serde(borrow, default)]
    pub ds: Str<'a>,
    #[serde(default)]
    pub id: Number,
    pub open: Number,
    pub high: Number,
    pub low: Number,
    pub close: Number,
    #[serde(default)]
    pub vol: Number,
}
