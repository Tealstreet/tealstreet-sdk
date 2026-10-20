//! Borrowed wire records; numeric strings decode directly into numbers.
use serde::Deserialize;
use std::borrow::Cow;

pub fn number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Number(f64),
        Text(#[serde(borrow)] Cow<'a, str>),
    }
    let n = match N::deserialize(d)? {
        N::Number(n) => Ok(n),
        N::Text(s) => s.trim().parse().map_err(serde::de::Error::custom),
    }?;
    if n.is_finite() {
        Ok(n)
    } else {
        Err(serde::de::Error::custom("nonfinite number"))
    }
}
pub fn optional_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    struct N(#[serde(deserialize_with = "number")] f64);
    Ok(Option::<N>::deserialize(d)?.map(|n| n.0))
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow)]
    pub channel: Cow<'a, str>,
    #[serde(borrow)]
    pub data: &'a serde_json::value::RawValue,
}
#[derive(Deserialize)]
pub struct Meta {
    pub universe: Vec<Instrument>,
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
    #[serde(borrow)]
    pub asset_positions: Vec<AssetPosition<'a>>,
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
    #[serde(default, deserialize_with = "number")]
    pub closed_pnl: f64,
    #[serde(default, deserialize_with = "number")]
    pub fee: f64,
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
    pub universe: Vec<SpotMarket>,
    pub tokens: Vec<SpotToken>,
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
}
#[derive(Deserialize)]
pub struct SpotCtx {
    #[serde(default)]
    pub coin: String,
    #[serde(flatten)]
    pub context: AssetCtx,
}
