//! Binance USD-M wire records. Each struct names only the fields we read;
//! string fields borrow from the frame unless they contain escapes.
use serde::Deserialize;
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
    #[serde(borrow)]
    pub margin_asset: Str<'a>,
    #[serde(borrow, default)]
    pub contract_type: Str<'a>,
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
    pub step_size: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub min_qty: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub max_qty: Option<Str<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Daily<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub last_price: Str<'a>,
    #[serde(borrow)]
    pub price_change_percent: Str<'a>,
    #[serde(borrow)]
    pub volume: Str<'a>,
    #[serde(borrow)]
    pub quote_volume: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookTickerRest<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub bid_price: Str<'a>,
    #[serde(borrow)]
    pub ask_price: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PremiumIndex<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub mark_price: Str<'a>,
    #[serde(borrow)]
    pub index_price: Str<'a>,
    #[serde(borrow)]
    pub last_funding_rate: Str<'a>,
    pub next_funding_time: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthSnapshot<'a> {
    pub last_update_id: u64,
    #[serde(borrow)]
    pub bids: Vec<[Str<'a>; 2]>,
    #[serde(borrow)]
    pub asks: Vec<[Str<'a>; 2]>,
}

/// One element of `!ticker@arr`.
#[derive(Deserialize)]
pub struct TickerEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub c: Str<'a>,
    #[serde(rename = "P", borrow)]
    pub percentage: Str<'a>,
    #[serde(borrow)]
    pub v: Str<'a>,
    #[serde(borrow)]
    pub q: Str<'a>,
}

/// One element of `!markPrice@arr@1s`.
#[derive(Deserialize)]
pub struct MarkPriceEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub p: Str<'a>,
    #[serde(borrow)]
    pub i: Str<'a>,
    #[serde(borrow)]
    pub r: Str<'a>,
    #[serde(rename = "T")]
    pub funding_time: f64,
}

#[derive(Deserialize)]
pub struct BookTickerEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub b: Str<'a>,
    #[serde(borrow)]
    pub a: Str<'a>,
}

#[derive(Deserialize)]
pub struct DepthEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(rename = "U")]
    pub first_update_id: u64,
    #[serde(rename = "u")]
    pub final_update_id: u64,
    #[serde(default)]
    pub pu: Option<u64>,
    #[serde(borrow, default)]
    pub b: Vec<[Str<'a>; 2]>,
    #[serde(borrow, default)]
    pub a: Vec<[Str<'a>; 2]>,
}

#[derive(Deserialize)]
pub struct KlineEvent<'a> {
    #[serde(borrow)]
    pub k: Kline<'a>,
}

#[derive(Deserialize)]
pub struct Kline<'a> {
    pub t: f64,
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub i: Str<'a>,
    #[serde(borrow)]
    pub o: Str<'a>,
    #[serde(borrow)]
    pub h: Str<'a>,
    #[serde(borrow)]
    pub l: Str<'a>,
    #[serde(borrow)]
    pub c: Str<'a>,
    #[serde(borrow)]
    pub v: Str<'a>,
}

/// REST kline row: `[openTime, open, high, low, close, volume, ...]`.
#[derive(Deserialize)]
pub struct KlineRow<'a>(
    pub f64,
    #[serde(borrow)] pub Str<'a>,
    #[serde(borrow)] pub Str<'a>,
    #[serde(borrow)] pub Str<'a>,
    #[serde(borrow)] pub Str<'a>,
    #[serde(borrow)] pub Str<'a>,
    serde::de::IgnoredAny,
    serde::de::IgnoredAny,
    serde::de::IgnoredAny,
    serde::de::IgnoredAny,
    serde::de::IgnoredAny,
    serde::de::IgnoredAny,
);

/// Value of the top-level `"e"` field, found without parsing the frame.
pub fn event_type(text: &str) -> Option<&str> {
    let start = text.find("\"e\":\"")? + 5;
    let end = text[start..].find('"')? + start;
    Some(&text[start..end])
}
