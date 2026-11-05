//! Aster-specific envelopes and partial snapshots; shared Binance records are identical on wire.
pub use safe_cex_venue_binance::wire::{Daily, KlineEvent, KlineRow};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::borrow::Cow;

type Str<'a> = Cow<'a, str>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInfo<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub status: Str<'a>,
    #[serde(borrow)]
    pub base_asset: Str<'a>,
    #[serde(borrow)]
    pub quote_asset: Str<'a>,
    pub price_precision: i32,
    pub quantity_precision: i32,
    #[serde(borrow, default)]
    pub filters: Vec<Filter<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter<'a> {
    #[serde(borrow)]
    pub filter_type: Str<'a>,
    #[serde(borrow, default)]
    pub tick_size: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub min_qty: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub max_qty: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub step_size: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub notional: Option<Str<'a>>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow, default)]
    pub stream: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub data: Option<&'a RawValue>,
    #[serde(borrow, default)]
    pub e: Option<Str<'a>>,
    #[serde(default)]
    pub id: Option<i64>,
}

pub enum FastAggregate<'a> {
    Tickers(Vec<TickerEvent<'a>>),
    Marks(Vec<MarkEvent<'a>>),
}

impl<'a> FastAggregate<'a> {
    pub fn decode(text: &'a str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }
}

impl<'de> Deserialize<'de> for FastAggregate<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Clone, Copy, Deserialize)]
        #[serde(field_identifier)]
        enum Field {
            #[serde(rename = "stream")]
            Stream,
            #[serde(rename = "data")]
            Data,
            #[serde(rename = "e")]
            Event,
            #[serde(rename = "id")]
            Id,
            #[serde(other)]
            Other,
        }
        #[derive(Deserialize)]
        struct Text<'a>(#[serde(borrow)] Str<'a>);
        struct Aggregate;
        impl<'de> serde::de::Visitor<'de> for Aggregate {
            type Value = FastAggregate<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a stream-first Aster ticker or mark aggregate")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut seen = 0u8;
                let mut stream = None;
                let mut rows = None;
                while let Some(field) = map.next_key::<Field>()? {
                    if matches!(field, Field::Other) {
                        map.next_value::<serde::de::IgnoredAny>()?;
                        continue;
                    }
                    let bit = 1 << field as u8;
                    if seen & bit != 0 {
                        return Err(serde::de::Error::custom("duplicate Aster envelope field"));
                    }
                    seen |= bit;
                    match field {
                        Field::Stream => {
                            stream = map.next_value::<Option<Text<'de>>>()?.map(|text| text.0);
                        }
                        Field::Data => {
                            rows = Some(match stream.as_deref() {
                                Some("!ticker@arr") => FastAggregate::Tickers(map.next_value()?),
                                Some("!markPrice@arr@1s" | "!markPrice@arr") => {
                                    FastAggregate::Marks(map.next_value()?)
                                }
                                _ => {
                                    return Err(serde::de::Error::custom(
                                        "not a stream-first aggregate",
                                    ));
                                }
                            });
                        }
                        Field::Event => {
                            map.next_value::<Option<Text<'de>>>()?;
                        }
                        Field::Id => {
                            if map.next_value::<Option<i64>>()?.is_some() {
                                return Err(serde::de::Error::custom(
                                    "Aster subscription acknowledgement",
                                ));
                            }
                        }
                        Field::Other => unreachable!(),
                    }
                }
                rows.ok_or_else(|| serde::de::Error::missing_field("data"))
            }
        }
        d.deserialize_map(Aggregate)
    }
}

#[cfg(test)]
mod aggregate_tests {
    use super::FastAggregate;

    #[test]
    fn stream_first_aggregates_decode_without_a_raw_body_scan() {
        let text = r#"{"str\u0065am":"\u0021ticker@arr","data":[{"s":"BTC\u0055SDT","c":"101"}],"e":null,"id":null}"#;
        let Some(FastAggregate::Tickers(rows)) = FastAggregate::decode(text) else {
            panic!("stream-first ticker should decode its typed rows directly");
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].s, "BTCUSDT");
        assert_eq!(rows[0].c.as_deref(), Some("101"));
        for stream in ["!markPrice@arr", "!markPrice@arr@1s"] {
            let text =
                format!(r#"{{"stream":"{stream}","data":[{{"s":"BTCUSDT","p":"100","T":2}}]}}"#);
            let Some(FastAggregate::Marks(rows)) = FastAggregate::decode(&text) else {
                panic!("stream-first marks should decode their typed rows directly");
            };
            assert_eq!(rows[0].p.as_deref(), Some("100"));
            assert_eq!(rows[0].funding_time, Some(2.0));
        }
    }

    #[test]
    fn reordered_and_typed_error_frames_select_the_general_decoder() {
        for text in [
            r#"{"data":[{"s":"BTCUSDT","c":"101"}],"stream":"!ticker@arr"}"#,
            r#"{"stream":"!ticker@arr","data":[{"s":"BTCUSDT","c":"101"},{"s":"BTCUSDT","c":102}]}"#,
        ] {
            assert!(FastAggregate::decode(text).is_none(), "{text}");
        }
    }
}

#[derive(Deserialize)]
pub struct TickerEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow, default)]
    pub c: Option<Str<'a>>,
    #[serde(rename = "P", borrow, default)]
    pub percentage: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub v: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub q: Option<Str<'a>>,
}

#[derive(Deserialize)]
pub struct MarkEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow, default)]
    pub p: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub i: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub r: Option<Str<'a>>,
    #[serde(rename = "T", default)]
    pub funding_time: Option<f64>,
}

#[derive(Deserialize)]
pub struct PartialDepth<'a> {
    #[serde(borrow, default)]
    pub s: Option<Str<'a>>,
    #[serde(default)]
    pub u: Option<u64>,
    #[serde(borrow)]
    pub b: Vec<[Str<'a>; 2]>,
    #[serde(borrow)]
    pub a: Vec<[Str<'a>; 2]>,
}

#[derive(Deserialize)]
pub struct TradeEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub p: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interest<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub open_interest: &'a RawValue,
}

#[derive(Deserialize)]
pub struct InterestEnvelope<'a> {
    #[serde(borrow)]
    pub data: &'a RawValue,
}
