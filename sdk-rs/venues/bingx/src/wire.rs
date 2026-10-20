//! Borrowed records for the public swap feeds.
use serde::Deserialize;
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;

use safe_cex_engine::de::lenient_optional_number;
pub use safe_cex_engine::de::number;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub asset: Str<'a>,
    #[serde(borrow)]
    pub currency: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub status: f64,
    #[serde(deserialize_with = "number")]
    pub quantity_precision: f64,
    #[serde(deserialize_with = "number")]
    pub price_precision: f64,
    #[serde(deserialize_with = "number")]
    pub trade_min_limit: f64,
    #[serde(default, deserialize_with = "number")]
    pub max_long_leverage: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Daily<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub last_price: f64,
    #[serde(deserialize_with = "number")]
    pub price_change_percent: f64,
    #[serde(deserialize_with = "number")]
    pub volume: f64,
    #[serde(deserialize_with = "number")]
    pub quote_volume: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Premium<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub mark_price: f64,
    #[serde(deserialize_with = "number")]
    pub index_price: f64,
    #[serde(deserialize_with = "number")]
    pub last_funding_rate: f64,
    #[serde(deserialize_with = "number")]
    pub next_funding_time: f64,
}

/// The identity of a list row that failed to map, for the skipped-row log.
#[derive(Deserialize)]
pub struct RowIdentity<'a> {
    #[serde(default, borrow)]
    pub symbol: Option<Str<'a>>,
}
pub fn row_symbol(item: &str) -> Option<Str<'_>> {
    serde_json::from_str::<RowIdentity<'_>>(item)
        .ok()
        .and_then(|row| row.symbol)
        .filter(|symbol| !symbol.is_empty())
}

/// A signed request refused at the signature check, before any processing.
#[derive(Deserialize)]
pub struct Rejection<'a> {
    #[serde(default)]
    pub code: Option<i64>,
    #[serde(default, borrow)]
    pub msg: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTicker<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(default, deserialize_with = "number")]
    pub trade_price: f64,
    #[serde(deserialize_with = "number")]
    pub fair_price: f64,
    #[serde(deserialize_with = "number")]
    pub index_price: f64,
    #[serde(deserialize_with = "number")]
    pub funding_rate: f64,
    #[serde(default, deserialize_with = "lenient_optional_number")]
    pub change_percentage: Option<f64>,
    #[serde(
        rename = "volumeF",
        default,
        deserialize_with = "lenient_optional_number"
    )]
    pub volume: Option<f64>,
    #[serde(
        rename = "valueF",
        default,
        deserialize_with = "lenient_optional_number"
    )]
    pub value: Option<f64>,
}

#[derive(Deserialize)]
pub struct Level {
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub volume: f64,
}

#[derive(Deserialize)]
pub struct Kline {
    #[serde(deserialize_with = "number")]
    pub time: f64,
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
impl From<Kline> for safe_cex_engine::model::Candle {
    fn from(k: Kline) -> Self {
        Self {
            timestamp: k.time / 1000.,
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Depth {
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandleData {
    pub kline_infos_vo: Vec<Kline>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message<'a> {
    #[serde(default, borrow)]
    pub data_type: Str<'a>,
    #[serde(borrow)]
    pub data: Option<&'a serde_json::value::RawValue>,
    pub ping: Option<serde::de::IgnoredAny>,
    pub pong: Option<serde::de::IgnoredAny>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Deserialize)]
    struct Numbers {
        #[serde(deserialize_with = "number")]
        strict: f64,
        #[serde(deserialize_with = "lenient_optional_number")]
        optional: Option<f64>,
    }
    #[test]
    fn numeric_wire_forms_and_placeholders() {
        for value in ["12.5", "\"12.5\"", "12", "-12"] {
            let row: Numbers =
                serde_json::from_str(&format!(r#"{{"strict":{value},"optional":{value}}}"#))
                    .unwrap();
            assert_eq!(Some(row.strict), row.optional);
        }
        for value in ["null", "\"-\"", "\"NaN\"", "\"inf\""] {
            let row: Numbers =
                serde_json::from_str(&format!(r#"{{"strict":1,"optional":{value}}}"#)).unwrap();
            assert_eq!(row.optional, None);
            assert!(
                serde_json::from_str::<Numbers>(&format!(r#"{{"strict":{value},"optional":0}}"#))
                    .is_err()
            );
        }
        for value in ["true", "[]", "{}"] {
            assert!(
                serde_json::from_str::<Numbers>(&format!(r#"{{"strict":1,"optional":{value}}}"#))
                    .is_err()
            );
        }
    }
}
