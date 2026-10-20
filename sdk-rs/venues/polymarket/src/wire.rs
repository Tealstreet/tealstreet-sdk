use serde::Deserialize;
use std::borrow::Cow;

pub use safe_cex_engine::de::Number;
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
    pub order_price_min_tick_size: Option<Number>,
    pub order_min_size: Option<Number>,
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
