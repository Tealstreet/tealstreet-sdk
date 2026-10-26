//! Borrowed WOO records; absent financial values stay absent.
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;

pub use safe_cex_engine::de::{number, optional_number};

pub fn id<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Text(String),
        Integer(u64),
    }
    Ok(match Option::<Id>::deserialize(d)? {
        Some(Id::Text(s)) => s,
        Some(Id::Integer(n)) => n.to_string(),
        None => String::new(),
    })
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    pub success: Option<bool>,
    #[serde(borrow, default, alias = "msg")]
    pub message: Str<'a>,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
}
impl<'a> Envelope<'a> {
    pub fn payload(&self) -> Result<&'a str, String> {
        self.data
            .map(RawValue::get)
            .ok_or_else(|| "WOO response has no data".into())
    }
    pub fn checked(body: &'a str) -> Result<Self, String> {
        let e: Self = serde_json::from_str(body).map_err(|e| format!("WOO response: {e}"))?;
        if e.success != Some(true) {
            return Err(format!("WOO request failed: {}", e.message));
        }
        Ok(e)
    }
}

#[derive(Deserialize)]
pub struct Symbol<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
}

#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(borrow, default)]
    pub event: Str<'a>,
    #[serde(borrow, default)]
    pub topic: Str<'a>,
    #[serde(default, deserialize_with = "number")]
    pub ts: f64,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
    pub success: Option<bool>,
}

#[derive(Deserialize)]
pub struct TickerMessage<'a> {
    #[serde(borrow, default)]
    pub event: Str<'a>,
    #[serde(borrow, default)]
    pub topic: Str<'a>,
    #[serde(default, rename = "ts", deserialize_with = "number")]
    _ts: f64,
    #[serde(rename = "success")]
    _success: Option<bool>,
    #[serde(borrow)]
    pub data: Vec<Tick<'a>>,
}

#[derive(Deserialize)]
pub struct Instrument<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
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
    #[serde(default, deserialize_with = "optional_number")]
    pub price_range: Option<f64>,
}
#[derive(Deserialize)]
pub struct Statistics<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(rename = "24h_open", deserialize_with = "number")]
    pub open: f64,
    #[serde(rename = "24h_close", deserialize_with = "number")]
    pub close: f64,
    #[serde(rename = "24h_volume", deserialize_with = "number")]
    pub volume: f64,
    #[serde(rename = "24h_amount", deserialize_with = "number")]
    pub amount: f64,
    #[serde(default, deserialize_with = "optional_number")]
    pub index_price: Option<f64>,
    #[serde(deserialize_with = "number")]
    pub mark_price: f64,
    #[serde(deserialize_with = "number")]
    pub last_funding_rate: f64,
    #[serde(deserialize_with = "number")]
    pub next_funding_time: f64,
    #[serde(deserialize_with = "number")]
    pub open_interest: f64,
}
#[derive(Deserialize)]
pub struct Funding<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub est_funding_rate: f64,
    #[serde(deserialize_with = "number")]
    pub next_funding_time: f64,
}
#[derive(Deserialize)]
pub struct Tick<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(default, deserialize_with = "optional_number")]
    pub close: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub volume: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub amount: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub bid: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub ask: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub price: Option<f64>,
}

#[derive(Deserialize)]
pub struct Level {
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub quantity: f64,
}
#[derive(Deserialize)]
pub struct Snapshot {
    pub success: bool,
    #[serde(deserialize_with = "number")]
    pub timestamp: f64,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}
#[derive(Deserialize)]
pub struct Delta {
    #[serde(default, rename = "prevTs", deserialize_with = "optional_number")]
    pub previous: Option<f64>,
    pub bids: Vec<(Price, Price)>,
    pub asks: Vec<(Price, Price)>,
}
#[derive(Deserialize)]
pub struct Price(#[serde(deserialize_with = "number")] pub f64);
impl Delta {
    pub fn valid(&self) -> bool {
        self.bids
            .iter()
            .chain(&self.asks)
            .all(|(p, q)| p.0 > 0.0 && q.0 >= 0.0)
    }
}
#[derive(Deserialize)]
pub struct Kline {
    #[serde(alias = "startTime", deserialize_with = "number")]
    pub start_timestamp: f64,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Deserialize)]
    struct Row {
        #[serde(deserialize_with = "id")]
        id: String,
    }

    #[test]
    fn ids_preserve_string_integer_and_null_semantics() {
        for (raw, expected) in [
            ("null", ""),
            (r#""""#, ""),
            ("0", "0"),
            (r#""0001""#, "0001"),
            (r#""1.0""#, "1.0"),
            (r#"" ""#, " "),
            (r#""\u0031""#, "1"),
            ("9007199254740993", "9007199254740993"),
            ("18446744073709551615", "18446744073709551615"),
        ] {
            let row: Row = serde_json::from_str(&format!(r#"{{"id":{raw}}}"#)).unwrap();
            assert_eq!(row.id, expected, "{raw}");
        }
    }
    #[test]
    fn ids_reject_noninteger_numeric_and_non_scalar_types() {
        for raw in [
            "-1",
            "-0",
            "1.0",
            "1e0",
            "18446744073709551616",
            "true",
            "false",
            "[]",
            "{}",
        ] {
            assert!(
                serde_json::from_str::<Row>(&format!(r#"{{"id":{raw}}}"#)).is_err(),
                "{raw}"
            );
        }
    }
}
