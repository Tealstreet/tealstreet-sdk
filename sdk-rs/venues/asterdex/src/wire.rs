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

// Stream-first frames decode typed bodies without a separate raw-body scan.
pub enum FastAggregate<'a> {
    Tickers(Vec<TickerEvent<'a>>),
    Marks(Vec<MarkEvent<'a>>),
    Depth {
        stream: Str<'a>,
        event: PartialDepth<'a>,
    },
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
                                Some(value) if super::is_partial_stream(value) => {
                                    FastAggregate::Depth {
                                        stream: stream.as_ref().unwrap().clone(),
                                        event: map.next_value()?,
                                    }
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
    use super::{FastAggregate, MarkEvent, PartialDepth, TickerEvent};

    #[test]
    fn partial_depth_numbers_borrow_unescaped_frame_storage() {
        let (depth, allocations) = allocation_count::measure(|| {
            serde_json::from_str::<PartialDepth<'_>>(
                r#"{"b":[["100.1","2"],["99.9","3"]],"a":[["101","4"],["102.5","5"]]}"#,
            )
            .unwrap()
        });
        assert_eq!(allocations, 2, "only the two level vectors allocate");
        for value in depth.b.iter().chain(&depth.a).flatten() {
            assert!(matches!(value, std::borrow::Cow::Borrowed(_)));
        }
    }

    #[test]
    fn partial_depth_level_borrowing_keeps_escape_and_error_semantics() {
        let depth: PartialDepth<'_> =
            serde_json::from_str(r#"{"b":[["1\u0030.5","2"]],"a":[["11","3"]]}"#).unwrap();
        assert_eq!(depth.b[0][0], "10.5");
        assert!(matches!(depth.b[0][0], std::borrow::Cow::Owned(_)));
        for text in [
            r#"{"b":[["10"]],"a":[]}"#,
            r#"{"b":[["10","1","2"]],"a":[]}"#,
            r#"{"b":[[10,"1"]],"a":[]}"#,
            r#"{"b":[[null,"1"]],"a":[]}"#,
            r#"{"b":[["10",true]],"a":[]}"#,
            r#"{"b":[],"b":[],"a":[]}"#,
        ] {
            assert!(serde_json::from_str::<PartialDepth<'_>>(text).is_err());
        }
    }

    #[test]
    fn optional_market_numbers_borrow_unescaped_frame_storage() {
        let (mark, allocations) = allocation_count::measure(|| {
            serde_json::from_str::<MarkEvent<'_>>(
                r#"{"s":"BTCUSDT","p":"100.1","i":"100.2","r":"0.0001","T":2}"#,
            )
            .unwrap()
        });
        assert_eq!(allocations, 0);
        assert!(matches!(mark.s, std::borrow::Cow::Borrowed(_)));
        for value in [mark.p, mark.i, mark.r] {
            assert!(matches!(value, Some(std::borrow::Cow::Borrowed(_))));
        }
        let ticker: TickerEvent<'_> =
            serde_json::from_str(r#"{"s":"BTCUSDT","c":"100.1","P":"2","v":"3","q":"4"}"#).unwrap();
        for value in [ticker.c, ticker.percentage, ticker.v, ticker.q] {
            assert!(matches!(value, Some(std::borrow::Cow::Borrowed(_))));
        }
    }

    #[test]
    fn borrowed_market_numbers_keep_escape_null_and_error_semantics() {
        let mark: MarkEvent<'_> =
            serde_json::from_str(r#"{"s":"BTC\u0055SDT","p":"1\u0030.1","i":null,"r":""}"#)
                .unwrap();
        assert_eq!(mark.s, "BTCUSDT");
        assert!(matches!(mark.s, std::borrow::Cow::Owned(_)));
        assert_eq!(mark.p.as_deref(), Some("10.1"));
        assert!(matches!(mark.p, Some(std::borrow::Cow::Owned(_))));
        assert!(mark.i.is_none());
        assert_eq!(mark.r.as_deref(), Some(""));
        for text in [
            r#"{"s":"BTCUSDT"}"#,
            r#"{"s":"BTCUSDT","p":null,"i":null,"r":null}"#,
        ] {
            let mark: MarkEvent<'_> = serde_json::from_str(text).unwrap();
            assert!(mark.p.is_none() && mark.i.is_none() && mark.r.is_none());
        }
        for text in [
            r#"{"s":"BTCUSDT","p":10}"#,
            r#"{"s":"BTCUSDT","p":true}"#,
            r#"{"s":"BTCUSDT","p":{}}"#,
            r#"{"s":"BTCUSDT","p":"1","p":"2"}"#,
        ] {
            assert!(serde_json::from_str::<MarkEvent<'_>>(text).is_err());
        }
        let ticker: TickerEvent<'_> =
            serde_json::from_str(r#"{"s":"BTCUSDT","c":"1\u0030.1","P":null,"v":"","q":"4"}"#)
                .unwrap();
        assert_eq!(ticker.c.as_deref(), Some("10.1"));
        assert!(matches!(ticker.c, Some(std::borrow::Cow::Owned(_))));
        assert!(ticker.percentage.is_none());
        assert_eq!(ticker.v.as_deref(), Some(""));
    }

    mod allocation_count {
        use std::alloc::{GlobalAlloc, Layout, System};
        use std::cell::Cell;

        thread_local! { static CALLS: Cell<Option<usize>> = const { Cell::new(None) }; }
        struct Counter;
        fn allocated() {
            let _ = CALLS.try_with(|calls| {
                if let Some(count) = calls.get() {
                    calls.set(Some(count + 1));
                }
            });
        }
        unsafe impl GlobalAlloc for Counter {
            unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
                allocated();
                unsafe { System.alloc(layout) }
            }
            unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
                allocated();
                unsafe { System.alloc_zeroed(layout) }
            }
            unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
                allocated();
                unsafe { System.realloc(pointer, layout, size) }
            }
            unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
                unsafe { System.dealloc(pointer, layout) }
            }
        }
        #[global_allocator]
        static ALLOCATOR: Counter = Counter;

        pub fn measure<T>(f: impl FnOnce() -> T) -> (T, usize) {
            struct Reset;
            impl Drop for Reset {
                fn drop(&mut self) {
                    CALLS.with(|calls| calls.set(None));
                }
            }
            CALLS.with(|calls| calls.set(Some(0)));
            let _reset = Reset;
            let result = f();
            (result, CALLS.with(|calls| calls.get().unwrap()))
        }
    }

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
    fn stream_first_partial_depth_uses_typed_single_pass_decoding() {
        for stream in [
            "btcusdt@depth5",
            "ethusdt@depth10@250ms",
            "btcusdt@depth20@100ms",
        ] {
            let text = format!(
                r#"{{"stream":"{stream}","data":{{"s":"BTCUSDT","u":2,"b":[["100","1"]],"a":[["101","2"]]}}}}"#
            );
            assert!(FastAggregate::decode(&text).is_some(), "{stream}");
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
    pub s: Cow<'a, str>,
    #[serde(borrow, default, deserialize_with = "optional_text")]
    pub c: Option<Str<'a>>,
    #[serde(rename = "P", borrow, default, deserialize_with = "optional_text")]
    pub percentage: Option<Str<'a>>,
    #[serde(borrow, default, deserialize_with = "optional_text")]
    pub v: Option<Str<'a>>,
    #[serde(borrow, default, deserialize_with = "optional_text")]
    pub q: Option<Str<'a>>,
}

#[derive(Deserialize)]
pub struct MarkEvent<'a> {
    #[serde(borrow)]
    pub s: Cow<'a, str>,
    #[serde(borrow, default, deserialize_with = "optional_text")]
    pub p: Option<Str<'a>>,
    #[serde(borrow, default, deserialize_with = "optional_text")]
    pub i: Option<Str<'a>>,
    #[serde(borrow, default, deserialize_with = "optional_text")]
    pub r: Option<Str<'a>>,
    #[serde(rename = "T", default)]
    pub funding_time: Option<f64>,
}

// Serde's borrowing special case recognizes direct Cow fields, not aliases or
// Option<Cow>. Escape decoding still owns its resulting string.
fn optional_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Str<'de>>, D::Error> {
    #[derive(Deserialize)]
    struct Text<'a>(#[serde(borrow)] Cow<'a, str>);
    Option::<Text<'de>>::deserialize(d).map(|value| value.map(|text| text.0))
}

fn depth_levels<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<[Str<'de>; 2]>, D::Error> {
    #[derive(Deserialize)]
    struct Level<'a>(#[serde(borrow)] Cow<'a, str>, #[serde(borrow)] Cow<'a, str>);
    struct Levels;
    impl<'de> serde::de::Visitor<'de> for Levels {
        type Value = Vec<[Str<'de>; 2]>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("an array of price and size string pairs")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut rows = Vec::new();
            while let Some(Level(price, size)) = seq.next_element()? {
                rows.push([price, size]);
            }
            Ok(rows)
        }
    }
    d.deserialize_seq(Levels)
}

#[derive(Deserialize)]
pub struct PartialDepth<'a> {
    #[serde(borrow, default)]
    pub s: Option<Str<'a>>,
    #[serde(default)]
    pub u: Option<u64>,
    #[serde(borrow, deserialize_with = "depth_levels")]
    pub b: Vec<[Str<'a>; 2]>,
    #[serde(borrow, deserialize_with = "depth_levels")]
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
