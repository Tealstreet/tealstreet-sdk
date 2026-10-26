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
