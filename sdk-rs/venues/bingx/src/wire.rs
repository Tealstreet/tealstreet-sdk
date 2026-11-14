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
    // Documented as deprecated: its absence must not empty the market list.
    #[serde(default, deserialize_with = "number")]
    pub trade_min_limit: f64,
    #[serde(default, deserialize_with = "safe_cex_engine::de::number_or_zero")]
    pub trade_min_quantity: f64,
    #[serde(
        default,
        rename = "tradeMinUSDT",
        deserialize_with = "safe_cex_engine::de::number_or_zero"
    )]
    pub trade_min_usdt: f64,
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
struct GeneralMessage<'a> {
    #[serde(default, borrow)]
    data_type: Str<'a>,
    #[serde(borrow)]
    data: Option<&'a serde_json::value::RawValue>,
    ping: Option<serde::de::IgnoredAny>,
    pong: Option<serde::de::IgnoredAny>,
}

struct Contracts<'a>(Vec<LiveTicker<'a>>);
impl<'de> Deserialize<'de> for Contracts<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "camelCase")]
        enum Field {
            Contracts,
            #[serde(other)]
            Other,
        }
        struct Rows;
        impl<'de> serde::de::Visitor<'de> for Rows {
            type Value = Contracts<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a BingX ticker data object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut rows = None;
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Contracts => {
                            if rows.is_some() {
                                return Err(serde::de::Error::duplicate_field("contracts"));
                            }
                            rows = Some(map.next_value()?);
                        }
                        Field::Other => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Contracts(rows.unwrap_or_default()))
            }
        }
        deserializer.deserialize_map(Rows)
    }
}

pub struct Message<'a> {
    pub data_type: Str<'a>,
    pub data: Option<&'a serde_json::value::RawValue>,
    pub tickers: Option<Vec<LiveTicker<'a>>>,
    pub depth: Option<Depth>,
    pub ping: Option<serde::de::IgnoredAny>,
    pub pong: Option<serde::de::IgnoredAny>,
}

impl<'a> Message<'a> {
    pub fn decode(text: &'a str) -> serde_json::Result<Self> {
        // Keep the general decoder for reordered fields and valid prefixes before typed row errors.
        serde_json::from_str(text).or_else(|_| {
            serde_json::from_str::<GeneralMessage<'a>>(text).map(|message| Self {
                data_type: message.data_type,
                data: message.data,
                tickers: None,
                depth: None,
                ping: message.ping,
                pong: message.pong,
            })
        })
    }
}

#[derive(Deserialize, Clone, Copy)]
#[serde(field_identifier, rename_all = "camelCase")]
enum MessageField {
    DataType,
    Data,
    Ping,
    Pong,
    #[serde(other)]
    Other,
}

impl<'de> Deserialize<'de> for Message<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Text<'a>(#[serde(borrow)] Str<'a>);
        struct Envelope;
        impl<'de> serde::de::Visitor<'de> for Envelope {
            type Value = Message<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a BingX public envelope")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut message = Message {
                    data_type: Cow::Borrowed(""),
                    data: None,
                    tickers: None,
                    depth: None,
                    ping: None,
                    pong: None,
                };
                let mut seen = 0u8;
                while let Some(field) = map.next_key::<MessageField>()? {
                    let (bit, name) = match field {
                        MessageField::DataType => (1, "dataType"),
                        MessageField::Data => (2, "data"),
                        MessageField::Ping => (4, "ping"),
                        MessageField::Pong => (8, "pong"),
                        MessageField::Other => (0, ""),
                    };
                    if seen & bit != 0 {
                        return Err(serde::de::Error::duplicate_field(name));
                    }
                    seen |= bit;
                    match field {
                        MessageField::DataType => {
                            message.data_type = map.next_value::<Text<'de>>()?.0
                        }
                        MessageField::Data if message.data_type == "market.contracts" => {
                            message.tickers = map
                                .next_value::<Option<Contracts<'de>>>()?
                                .map(|data| data.0);
                        }
                        MessageField::Data
                            if message.data_type.starts_with("market.depth.")
                                && message.data_type.ends_with(".step0.level100") =>
                        {
                            message.depth = map.next_value()?;
                        }
                        MessageField::Data => message.data = map.next_value()?,
                        MessageField::Ping => message.ping = map.next_value()?,
                        MessageField::Pong => message.pong = map.next_value()?,
                        MessageField::Other => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(message)
            }
        }
        deserializer.deserialize_map(Envelope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn depth_is_decoded_during_the_first_envelope_pass() {
        let frame = r#"{"dataType":"market.depth.BTC-USDT.step0.level100","data":{"bids":[{"price":"91","volume":2}],"asks":[{"price":92,"volume":"3"}]}}"#;
        let message = Message::decode(frame).unwrap();
        assert!(
            message.data.is_none(),
            "valid depth must not be scanned as raw JSON and parsed a second time"
        );
        let depth = message.depth.unwrap();
        assert_eq!((depth.bids[0].price, depth.bids[0].volume), (91., 2.));
        assert_eq!((depth.asks[0].price, depth.asks[0].volume), (92., 3.));
    }

    #[test]
    fn reordered_and_malformed_depth_keep_the_raw_fallback() {
        for frame in [
            r#"{"data":{"bids":[{"price":"91","volume":2}],"asks":[]},"dataType":"market.depth.BTC-USDT.step0.level100"}"#,
            r#"{"dataType":"market.depth.BTC-USDT.step0.level100","data":{"bids":[{"price":"91","volume":2}],"asks":[{"price":true,"volume":3}]}}"#,
        ] {
            let message = Message::decode(frame).unwrap();
            assert!(message.data.is_some());
            assert!(message.depth.is_none());
        }
    }

    #[test]
    fn ticker_rows_are_decoded_during_the_first_envelope_pass() {
        let row =
            r#"{"symbol":"BTC-USDT","fairPrice":"91","indexPrice":"92","fundingRate":"0.01"}"#;
        let normal = format!(r#"{{"dataType":"market.contracts","data":{{"contracts":[{row}]}}}}"#);
        let message = Message::decode(&normal).unwrap();
        assert!(message.tickers.is_some());
        let reordered =
            format!(r#"{{"data":{{"contracts":[{row}]}},"dataType":"market.contracts"}}"#);
        let message = Message::decode(&reordered).unwrap();
        assert!(message.tickers.is_none());
        assert!(message.data.is_some());
    }

    #[test]
    fn typed_row_errors_and_reordered_envelopes_keep_raw_data() {
        for text in [
            r#"{"dataType":"market.contracts","data":{"contracts":[{"symbol":"BTC-USDT","fairPrice":true,"indexPrice":"92","fundingRate":"0.01"}]}}"#,
            r#"{"data":{"contracts":[]},"dataType":"market.contracts"}"#,
        ] {
            let message = Message::decode(text).unwrap();
            assert!(message.tickers.is_none());
            assert!(message.data.is_some());
        }
        let message =
            Message::decode(r#"{"dataType":"market.contracts","data":{"contracts":[]}}"#).unwrap();
        assert!(message.tickers.unwrap().is_empty());
        assert!(message.data.is_none());
    }

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
