//! GRVT borrowed wire records; identifiers retain integer precision.
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
pub struct Instrument<'a> {
    #[serde(borrow, rename = "i", alias = "instrument")]
    pub id: Str<'a>,
    #[serde(borrow, rename = "ih", alias = "instrument_hash", default)]
    pub hash: Str<'a>,
    #[serde(borrow, rename = "b", alias = "base")]
    pub base: Str<'a>,
    #[serde(borrow, rename = "q", alias = "quote")]
    pub quote: Str<'a>,
    #[serde(borrow, rename = "k", alias = "kind")]
    pub kind: Str<'a>,
    #[serde(rename = "ts", alias = "tick_size")]
    pub tick: Number,
    #[serde(rename = "ms", alias = "min_size")]
    pub min: Number,
    #[serde(rename = "mp", alias = "max_position_size", default)]
    pub max: Option<Number>,
    #[serde(rename = "mxs", alias = "max_size", default)]
    pub max_size: Option<Number>,
    #[serde(rename = "ia", alias = "is_active", default)]
    pub active: Option<bool>,
    #[serde(rename = "bd", alias = "base_decimals", default)]
    pub base_decimals: Option<i32>,
    #[serde(rename = "qd", alias = "quote_decimals", default)]
    pub quote_decimals: Option<i32>,
}
#[derive(Deserialize)]
pub struct Level {
    #[serde(rename = "p", alias = "price")]
    pub price: Number,
    #[serde(rename = "s", alias = "size")]
    pub size: Number,
}
#[derive(Default, Deserialize)]
pub struct Feed<'a> {
    #[serde(borrow, rename = "i", alias = "instrument", default)]
    pub instrument: Str<'a>,
    #[serde(rename = "lp", alias = "last_price", default)]
    pub last: Option<DeltaNumber>,
    #[serde(rename = "mp", alias = "mark_price", default)]
    pub mark: Option<DeltaNumber>,
    #[serde(rename = "ip", alias = "index_price", default)]
    pub index: Option<DeltaNumber>,
    #[serde(rename = "bb", alias = "best_bid_price", default)]
    pub bid: Option<DeltaNumber>,
    #[serde(rename = "ba", alias = "best_ask_price", default)]
    pub ask: Option<DeltaNumber>,
    #[serde(rename = "fr2", alias = "funding_rate", default)]
    pub funding: Option<DeltaNumber>,
    #[serde(rename = "oi", alias = "open_interest", default)]
    pub interest: Option<DeltaNumber>,
    #[serde(rename = "nf", alias = "next_funding_time", default)]
    pub funding_time: Option<DeltaNumber>,
    #[serde(rename = "bv", alias = "buy_volume_24h_b", default)]
    pub buy_volume: Option<DeltaNumber>,
    #[serde(rename = "sv", alias = "sell_volume_24h_b", default)]
    pub sell_volume: Option<DeltaNumber>,
    #[serde(rename = "op", alias = "open_price", default)]
    pub open_price: Option<DeltaNumber>,
    #[serde(rename = "b", alias = "bids", default)]
    pub bids: Option<Vec<Level>>,
    #[serde(rename = "a", alias = "asks", default)]
    pub asks: Option<Vec<Level>>,
    #[serde(rename = "ot", alias = "open_time", default)]
    pub open_time: Option<Number>,
    #[serde(rename = "o", alias = "open", default)]
    pub open: Option<Number>,
    #[serde(rename = "h", alias = "high", default)]
    pub high: Option<Number>,
    #[serde(rename = "l", alias = "low", default)]
    pub low: Option<Number>,
    #[serde(rename = "c", alias = "close", default)]
    pub close: Option<Number>,
    #[serde(rename = "vb", alias = "volume_b", default)]
    pub volume: Option<Number>,
}
#[derive(Default, Deserialize)]
pub struct Message<'a> {
    #[serde(borrow, rename = "s", alias = "stream", default)]
    pub stream: Str<'a>,
    #[serde(borrow, rename = "s1", alias = "selector", default)]
    pub selector: Str<'a>,
    #[serde(borrow, rename = "sn", alias = "sequence_number", default)]
    pub sequence: Text<'a>,
    #[serde(borrow, rename = "f", alias = "feed", default)]
    pub feed: Option<Feed<'a>>,
    #[serde(borrow, rename = "r", alias = "result", default)]
    pub result: Option<SubscriptionResult<'a>>,
}
#[derive(Default, Deserialize)]
pub struct Error<'a> {
    #[serde(borrow, rename = "c", alias = "code", default)]
    pub code: Text<'a>,
    #[serde(
        borrow,
        rename = "m",
        alias = "message",
        default,
        deserialize_with = "text_or_none"
    )]
    pub message: Option<Str<'a>>,
}
#[derive(Default, Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow, rename = "c", alias = "code", default)]
    pub code: Text<'a>,
    #[serde(
        borrow,
        rename = "m",
        alias = "message",
        default,
        deserialize_with = "text_or_none"
    )]
    pub message: Option<Str<'a>>,
    #[serde(borrow, rename = "e", alias = "error", default)]
    pub error: Option<Error<'a>>,
}
/// A null or non-string message is absent, never a reason to misread the envelope.
fn text_or_none<'de: 'a, 'a, D: Deserializer<'de>>(d: D) -> Result<Option<Str<'a>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Message<'a> {
        Text(#[serde(borrow)] Str<'a>),
        Other(de::IgnoredAny),
    }
    Ok(match Option::<Message<'a>>::deserialize(d)? {
        Some(Message::Text(text)) => Some(text),
        _ => None,
    })
}
pub fn check(response: &safe_cex_engine::Response<'_>) -> Result<(), String> {
    let env: Envelope<'_> =
        serde_json::from_str(response.body).map_err(|_| "Invalid GRVT response")?;
    let (code, message) = env.error.as_ref().map_or(
        (&*env.code, env.message.as_deref().unwrap_or_default()),
        |e| (&*e.code, e.message.as_deref().unwrap_or_default()),
    );
    if !(200..300).contains(&response.status) || (!code.is_empty() && code != "0") {
        let mut error = format!(
            "GRVT rejected request (HTTP {}, code {})",
            response.status, code
        );
        if !message.is_empty() {
            error.push_str(": ");
            error.extend(message.chars().take(200));
        }
        return Err(error);
    }
    Ok(())
}
/// True only when GRVT answered a non-5xx status with its own error code, so a write was
/// definitively not applied. Server/gateway failures and unparseable bodies stay unknown.
pub fn venue_rejected(response: &safe_cex_engine::Response<'_>) -> bool {
    let code = code(response);
    response.status < 500 && !code.is_empty() && code != "0"
}
/// A failed write is rejected only when GRVT answered below 5xx with an error code.
pub fn write_outcome(response: &safe_cex_engine::Response<'_>) -> safe_cex_engine::Outcome {
    if venue_rejected(response) {
        safe_cex_engine::Outcome::Rejected
    } else {
        safe_cex_engine::Outcome::Unknown
    }
}
impl Feed<'_> {
    pub fn candle(&self) -> Option<safe_cex_engine::model::Candle> {
        let c = safe_cex_engine::model::Candle {
            timestamp: self.open_time?.0 / 1e9,
            open: self.open?.0,
            high: self.high?.0,
            low: self.low?.0,
            close: self.close?.0,
            volume: self.volume?.0,
        };
        (c.timestamp > 0.
            && c.open > 0.
            && c.high >= c.low
            && c.low > 0.
            && c.close > 0.
            && c.volume >= 0.)
            .then_some(c)
    }
}

#[derive(Default, Deserialize)]
pub struct SubscriptionResult<'a> {
    #[serde(borrow, rename = "s", alias = "stream", default)]
    pub stream: Str<'a>,
    #[serde(borrow, rename = "s1", alias = "subs", default)]
    pub subs: Vec<Str<'a>>,
    #[serde(borrow, rename = "fs", alias = "first_sequence_number", default)]
    pub first: Vec<Text<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct DeltaNumber(pub f64);
// Ticker deltas send "" for a cleared value; absent fields stay `None`.
impl<'de> Deserialize<'de> for DeltaNumber {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_engine::de::number_or_zero(d).map(Self)
    }
}

pub fn code(response: &safe_cex_engine::Response<'_>) -> String {
    serde_json::from_str::<Envelope<'_>>(response.body)
        .map(|e| {
            e.error
                .map_or_else(|| e.code.to_string(), |e| e.code.to_string())
        })
        .unwrap_or_default()
}
