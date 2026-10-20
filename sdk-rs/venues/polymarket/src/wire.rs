use serde::Deserialize;
use std::borrow::Cow;

pub use safe_cex_engine::de::Number;
/// Public discovery fields: blank reads as 0, null and malformed reject the row.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct PublicNumber(
    #[serde(deserialize_with = "safe_cex_engine::de::number_or_blank_zero")] pub f64,
);
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gamma<'a> {
    #[serde(borrow)]
    pub condition_id: Cow<'a, str>,
    #[serde(borrow)]
    pub slug: Cow<'a, str>,
    #[serde(default, borrow)]
    pub question: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    pub description: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    pub end_date: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub clob_token_ids: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub outcomes: Option<Cow<'a, str>>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub enable_order_book: Option<bool>,
    pub accepting_orders: Option<bool>,
    pub order_price_min_tick_size: Option<PublicNumber>,
    pub order_min_size: Option<PublicNumber>,
    pub neg_risk: Option<bool>,
}
#[derive(Deserialize)]
pub struct Level {
    pub price: Number,
    pub size: Number,
}
#[derive(Default, Deserialize)]
pub struct PublicEvent<'a> {
    #[serde(default, borrow)]
    pub event_type: Cow<'a, str>,
    #[serde(default, borrow)]
    pub asset_id: Cow<'a, str>,
    #[serde(default)]
    pub timestamp: Option<Number>,
    #[serde(default)]
    pub bids: Option<Vec<Level>>,
    #[serde(default)]
    pub asks: Option<Vec<Level>>,
    #[serde(default, borrow)]
    pub price_changes: Vec<Change<'a>>,
    pub price: Option<Number>,
    pub new_tick_size: Option<Number>,
    #[serde(default, borrow)]
    pub assets_ids: Vec<Cow<'a, str>>,
}
#[derive(Deserialize)]
pub struct Change<'a> {
    #[serde(borrow)]
    pub asset_id: Cow<'a, str>,
    #[serde(borrow)]
    pub side: Cow<'a, str>,
    pub price: Number,
    pub size: Number,
    pub best_bid: Option<Number>,
    pub best_ask: Option<Number>,
}
#[derive(Deserialize)]
pub struct HistoryPoint {
    pub t: Number,
    pub p: Number,
}
#[derive(Deserialize)]
pub struct ApiError<'a> {
    #[serde(default, borrow)]
    pub error: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    pub message: Option<Cow<'a, str>>,
}
