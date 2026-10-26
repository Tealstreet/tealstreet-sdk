//! Borrowed wire records; numeric strings decode directly into numbers.
use serde::Deserialize;
use std::borrow::Cow;

pub use safe_cex_engine::de::{number, optional_number};
// Preserve field absence without accepting explicit invalid/null values that
// the existing required-number decoder rejects.
fn reported_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    number(d).map(Some)
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow)]
    pub channel: Cow<'a, str>,
    #[serde(borrow)]
    pub data: &'a serde_json::value::RawValue,
}
#[derive(Deserialize)]
pub struct Meta<'a> {
    #[serde(borrow)]
    pub universe: Vec<&'a serde_json::value::RawValue>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument {
    pub name: String,
    pub sz_decimals: i32,
    #[serde(default = "one")]
    pub max_leverage: f64,
    #[serde(default)]
    pub is_delisted: bool,
    #[serde(default)]
    pub only_isolated: bool,
    #[serde(default)]
    pub margin_mode: Option<String>,
}
impl Instrument {
    /// Cross margin is refused for these assets, so writes must default to isolated.
    pub fn isolated_only(&self) -> bool {
        self.only_isolated
            || matches!(
                self.margin_mode.as_deref(),
                Some("strictIsolated" | "noCross")
            )
    }
}
fn one() -> f64 {
    1.0
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetCtx {
    #[serde(default, deserialize_with = "optional_number")]
    pub mid_px: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    pub mark_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub oracle_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub prev_day_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub day_ntl_vlm: f64,
    #[serde(default, deserialize_with = "number")]
    pub day_base_vlm: f64,
    #[serde(default, deserialize_with = "number")]
    pub open_interest: f64,
    #[serde(default, deserialize_with = "number")]
    pub funding: f64,
}
#[derive(Deserialize)]
pub struct Dex {
    pub name: String,
}
#[derive(Deserialize)]
pub struct Mids<'a> {
    #[serde(borrow)]
    pub mids: std::collections::HashMap<Cow<'a, str>, Cow<'a, str>>,
}
#[derive(Clone, Deserialize)]
pub struct Level {
    #[serde(deserialize_with = "number")]
    pub px: f64,
    #[serde(deserialize_with = "number")]
    pub sz: f64,
}
#[derive(Deserialize)]
pub struct Depth<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    pub time: f64,
    pub levels: [Vec<Level>; 2],
}
#[derive(Deserialize)]
pub struct Bbo<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    pub time: f64,
    pub bbo: [Option<Level>; 2],
}
#[derive(Deserialize)]
pub struct CandleRow<'a> {
    pub t: f64,
    #[serde(default, borrow)]
    pub s: Cow<'a, str>,
    #[serde(default, borrow)]
    pub i: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    pub o: f64,
    #[serde(deserialize_with = "number")]
    pub h: f64,
    #[serde(deserialize_with = "number")]
    pub l: f64,
    #[serde(deserialize_with = "number")]
    pub c: f64,
    #[serde(deserialize_with = "number")]
    pub v: f64,
}
impl CandleRow<'_> {
    pub fn candle(&self) -> safe_cex_engine::model::Candle {
        safe_cex_engine::model::Candle {
            timestamp: self.t / 1000.0,
            open: self.o,
            high: self.h,
            low: self.l,
            close: self.c,
            volume: self.v,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FastCtx {
    #[serde(default, deserialize_with = "optional_number")]
    pub mark_px: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub mid_px: Option<f64>,
}

#[derive(Deserialize)]
pub struct Role<'a> {
    #[serde(borrow)]
    pub role: Cow<'a, str>,
    #[serde(default, borrow)]
    pub data: RoleData<'a>,
}
#[derive(Default, Deserialize)]
pub struct RoleData<'a> {
    #[serde(default, borrow)]
    pub user: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    pub master: Option<Cow<'a, str>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginSummary {
    #[serde(deserialize_with = "number")]
    pub account_value: f64,
    #[serde(deserialize_with = "number")]
    pub total_margin_used: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clearinghouse<'a> {
    /// Rows parse one by one so a malformed row cannot discard its neighbours.
    #[serde(borrow)]
    pub asset_positions: Vec<&'a serde_json::value::RawValue>,
    #[serde(default)]
    pub margin_summary: Option<MarginSummary>,
    #[serde(default)]
    pub cross_margin_summary: Option<MarginSummary>,
    #[serde(default)]
    pub time: Option<f64>,
    #[serde(default)]
    pub server_time: Option<f64>,
}
#[derive(Deserialize)]
pub struct AssetPosition<'a> {
    #[serde(borrow)]
    pub position: PositionRow<'a>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRow<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    pub szi: f64,
    #[serde(deserialize_with = "optional_number")]
    pub entry_px: Option<f64>,
    #[serde(deserialize_with = "number")]
    pub position_value: f64,
    #[serde(deserialize_with = "number")]
    pub unrealized_pnl: f64,
    #[serde(default, deserialize_with = "optional_number")]
    pub liquidation_px: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub margin_used: Option<f64>,
    pub leverage: Leverage<'a>,
}
#[derive(Deserialize)]
pub struct Leverage<'a> {
    #[serde(rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    pub value: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearingEnvelope<'a> {
    #[serde(default, borrow)]
    pub dex: Cow<'a, str>,
    #[serde(default, borrow)]
    pub clearinghouse_state: Option<Clearinghouse<'a>>,
    #[serde(default)]
    pub server_time: Option<f64>,
    #[serde(default)]
    pub time: Option<f64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotState<'a> {
    #[serde(borrow)]
    pub balances: Vec<SpotBalance<'a>>,
    #[serde(default)]
    pub token_to_available_after_maintenance: Vec<(u32, Num)>,
}
#[derive(Deserialize)]
pub struct Num(#[serde(deserialize_with = "number")] pub f64);
#[derive(Deserialize)]
pub struct SpotBalance<'a> {
    #[serde(default)]
    pub token: Option<u32>,
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    pub total: f64,
    #[serde(deserialize_with = "number")]
    pub hold: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotEnvelope<'a> {
    #[serde(borrow)]
    pub spot_state: SpotState<'a>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    pub oid: u64,
    #[serde(borrow)]
    pub side: Cow<'a, str>,
    #[serde(default, deserialize_with = "number")]
    pub limit_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub orig_sz: f64,
    #[serde(deserialize_with = "number")]
    pub sz: f64,
    #[serde(default)]
    pub reduce_only: bool,
    #[serde(default, borrow)]
    pub cloid: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    pub order_type: Cow<'a, str>,
    #[serde(default, deserialize_with = "optional_number")]
    pub trigger_px: Option<f64>,
    #[serde(default, borrow)]
    pub tpsl: Cow<'a, str>,
    #[serde(default, borrow)]
    pub tif: Cow<'a, str>,
    #[serde(default)]
    pub is_trigger: bool,
    #[serde(default)]
    pub timestamp: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderUpdate<'a> {
    #[serde(borrow)]
    pub order: OrderRow<'a>,
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    #[serde(default)]
    pub status_timestamp: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillRow<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    #[serde(borrow)]
    pub side: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    pub px: f64,
    #[serde(deserialize_with = "number")]
    pub sz: f64,
    pub time: f64,
    pub tid: u64,
    pub oid: u64,
    #[serde(default, borrow)]
    pub dir: Cow<'a, str>,
    #[serde(default, deserialize_with = "optional_number")]
    pub start_position: Option<f64>,
    #[serde(default, deserialize_with = "reported_number")]
    pub closed_pnl: Option<f64>,
    #[serde(default, deserialize_with = "reported_number")]
    pub fee: Option<f64>,
    #[serde(default, borrow)]
    pub fee_token: Cow<'a, str>,
    #[serde(default)]
    pub crossed: bool,
    #[serde(default, deserialize_with = "optional_number")]
    pub builder_fee: Option<f64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fills<'a> {
    #[serde(default)]
    pub is_snapshot: bool,
    #[serde(borrow)]
    pub fills: Vec<FillRow<'a>>,
}
#[derive(Deserialize)]
pub struct ActiveAsset<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    pub leverage: Leverage<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserEvents<'a> {
    #[serde(default, borrow)]
    pub fills: Vec<FillRow<'a>>,
    #[serde(default, borrow)]
    pub orders: Vec<OrderUpdate<'a>>,
    #[serde(default)]
    pub is_snapshot: bool,
}

#[derive(Deserialize)]
pub struct SpotMeta {
    pub universe: Vec<safe_cex_engine::startup::Row<SpotMarket>>,
    pub tokens: Vec<safe_cex_engine::startup::Row<SpotToken>>,
}
impl SpotMeta {
    /// One pass when every row is well formed; a malformed row falls back to
    /// the tolerant per-row parse (see `startup::rows`).
    pub fn parse(text: &str) -> serde_json::Result<Self> {
        #[derive(Deserialize)]
        struct WellFormed {
            universe: Vec<SpotMarket>,
            tokens: Vec<SpotToken>,
        }
        use safe_cex_engine::startup::Row;
        match serde_json::from_str::<WellFormed>(text) {
            Ok(meta) => Ok(Self {
                universe: meta
                    .universe
                    .into_iter()
                    .map(|row| Row(Some(row)))
                    .collect(),
                tokens: meta.tokens.into_iter().map(|row| Row(Some(row))).collect(),
            }),
            Err(_) => serde_json::from_str(text),
        }
    }
}
#[derive(Deserialize)]
pub struct SpotMarket {
    pub name: String,
    pub tokens: Vec<u32>,
    pub index: u32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotToken {
    pub name: String,
    pub index: u32,
    pub sz_decimals: i32,
    #[serde(default)]
    pub full_name: Option<String>,
}
#[derive(Deserialize)]
pub struct SpotCtx {
    #[serde(default)]
    pub coin: String,
    #[serde(flatten)]
    pub context: AssetCtx,
}

#[cfg(test)]
mod numeric_tests {
    use super::*;
    use serde::de::value::{
        BorrowedStrDeserializer, Error, F64Deserializer, I64Deserializer, StrDeserializer,
        StringDeserializer, U64Deserializer,
    };

    #[derive(Deserialize)]
    struct Number(#[serde(deserialize_with = "number")] f64);

    #[test]
    fn numeric_tokens_and_strings_preserve_rounding_and_signed_zero() {
        for token in [
            "0",
            "-0",
            "-0.0",
            "1",
            "-1",
            "0.125",
            "1e-4",
            "9007199254740993",
            "18446744073709551615",
            "-9223372036854775808",
            "18446744073709551616",
        ] {
            let expected = token.parse::<f64>().unwrap().to_bits();
            let raw = serde_json::from_str::<Number>(token).unwrap().0;
            let quoted = serde_json::from_str::<Number>(&format!("\" \\t{token} \\n\""))
                .unwrap()
                .0;
            assert_eq!(raw.to_bits(), expected, "{token}");
            assert_eq!(quoted.to_bits(), expected, "quoted {token}");
        }
        let escaped = serde_json::from_str::<Number>(r#""\u2003-\u0030.0\u2003""#)
            .unwrap()
            .0;
        assert_eq!(escaped.to_bits(), (-0.0f64).to_bits());
    }

    #[test]
    fn numeric_visitor_accepts_borrowed_owned_and_integer_inputs() {
        for actual in [
            number(StrDeserializer::<Error>::new(" -0 ")).unwrap(),
            number(BorrowedStrDeserializer::<Error>::new(" -0 ")).unwrap(),
            number(StringDeserializer::<Error>::new(" -0 ".into())).unwrap(),
        ] {
            assert_eq!(actual.to_bits(), (-0.0f64).to_bits());
        }
        for value in [i64::MIN, -9_007_199_254_740_993, 0, i64::MAX] {
            assert_eq!(
                number(I64Deserializer::<Error>::new(value))
                    .unwrap()
                    .to_bits(),
                (value as f64).to_bits()
            );
        }
        for value in [0, 9_007_199_254_740_993, u64::MAX] {
            assert_eq!(
                number(U64Deserializer::<Error>::new(value))
                    .unwrap()
                    .to_bits(),
                (value as f64).to_bits()
            );
        }
    }

    #[test]
    fn numeric_values_reject_nonfinite_and_non_numeric_shapes() {
        for raw in [
            "null",
            "true",
            "false",
            "[]",
            "{}",
            "1e400",
            r#""NaN""#,
            r#""nan""#,
            r#""inf""#,
            r#""-inf""#,
            r#""Infinity""#,
            r#""1e400""#,
            r#""""#,
            r#"" ""#,
            r#""--1""#,
            r#""1x""#,
        ] {
            assert!(serde_json::from_str::<Number>(raw).is_err(), "{raw}");
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(number(F64Deserializer::<Error>::new(value)).is_err());
        }
    }

    #[test]
    fn direct_and_flattened_contexts_preserve_numeric_forms_and_optional_defaults() {
        let raw = r#"{"midPx":" \t0.125 ","markPx":"\u0031.5","oraclePx":" -0 ","prevDayPx":9007199254740993,"dayNtlVlm":"18446744073709551615","dayBaseVlm":-9223372036854775808,"openInterest":3,"funding":" -1.25e-4 "}"#;
        let direct: AssetCtx = serde_json::from_str(raw).unwrap();
        let flattened: SpotCtx = serde_json::from_str(raw).unwrap();
        for ctx in [&direct, &flattened.context] {
            assert_eq!(ctx.mid_px, Some(0.125));
            assert_eq!(ctx.mark_px, 1.5);
            assert_eq!(ctx.oracle_px.to_bits(), (-0.0f64).to_bits());
            assert_eq!(
                ctx.prev_day_px.to_bits(),
                (9_007_199_254_740_993u64 as f64).to_bits()
            );
            assert_eq!(ctx.day_ntl_vlm.to_bits(), (u64::MAX as f64).to_bits());
            assert_eq!(ctx.day_base_vlm.to_bits(), (i64::MIN as f64).to_bits());
            assert_eq!(ctx.open_interest, 3.0);
            assert_eq!(ctx.funding, -0.000125);
        }
        for raw in ["{}", r#"{"midPx":null}"#] {
            let direct: AssetCtx = serde_json::from_str(raw).unwrap();
            let flattened: SpotCtx = serde_json::from_str(raw).unwrap();
            for ctx in [&direct, &flattened.context] {
                assert_eq!(ctx.mid_px, None);
                assert_eq!(ctx.mark_px, 0.0);
                assert_eq!(ctx.day_ntl_vlm, 0.0);
            }
        }
    }

    #[test]
    fn direct_and_flattened_contexts_reject_supplied_invalid_numbers() {
        for field in [
            "midPx",
            "markPx",
            "oraclePx",
            "prevDayPx",
            "dayNtlVlm",
            "dayBaseVlm",
            "openInterest",
            "funding",
        ] {
            for value in ["false", "[]", "{}", r#""NaN""#, r#""bad""#, r#""1e400""#] {
                let raw = format!(r#"{{"{field}":{value}}}"#);
                assert!(
                    serde_json::from_str::<AssetCtx>(&raw).is_err(),
                    "direct {raw}"
                );
                assert!(
                    serde_json::from_str::<SpotCtx>(&raw).is_err(),
                    "flattened {raw}"
                );
            }
            if field != "midPx" {
                let raw = format!(r#"{{"{field}":null}}"#);
                assert!(serde_json::from_str::<AssetCtx>(&raw).is_err());
                assert!(serde_json::from_str::<SpotCtx>(&raw).is_err());
            }
        }
    }
}
