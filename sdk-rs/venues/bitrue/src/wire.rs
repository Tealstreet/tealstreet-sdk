//! Borrowed Bitrue fields; numeric identifiers never pass through floats.
use serde::{Deserialize, Deserializer, de};
use std::{borrow::Cow, fmt, ops::Deref};
pub type Str<'a> = Cow<'a, str>;
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct Number(#[serde(deserialize_with = "number")] pub f64);
fn number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    struct Visitor;
    impl de::Visitor<'_> for Visitor {
        type Value = f64;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("finite number or numeric string")
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
            if v.is_finite() {
                Ok(v)
            } else {
                Err(E::custom("nonfinite number"))
            }
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
            v.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| E::custom("invalid number"))
        }
    }
    d.deserialize_any(Visitor)
}
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
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    pub price_precision: u32,
    pub multiplier: Number,
    pub min_order_volume: Number,
    pub max_market_volume: Number,
    #[serde(default)]
    pub max_limit_volume: Number,
    #[serde(default)]
    pub min_order_money: Number,
    pub status: Number,
}
#[derive(Deserialize, Default)]
pub struct Tick {
    #[serde(default, alias = "last")]
    pub close: Number,
    #[serde(default)]
    pub rose: Number,
    #[serde(default)]
    pub vol: Number,
    #[serde(default)]
    pub amount: Number,
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
    pub tick: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    pub data: Option<&'a serde_json::value::RawValue>,
}
#[derive(Deserialize)]
pub struct Depth {
    #[serde(alias = "bids")]
    pub buys: Vec<[Number; 2]>,
    pub asks: Vec<[Number; 2]>,
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
