use serde::Deserialize;
use serde_json::value::RawValue;
use std::borrow::Cow;

pub(crate) use safe_cex_engine::de::number;

#[derive(Deserialize)]
pub(crate) struct Instrument {
    pub symbol: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub trading_state: String,
    pub base_asset_name: String,
    pub quote_asset_name: String,
    #[serde(deserialize_with = "number")]
    pub base_increment: f64,
    #[serde(deserialize_with = "number")]
    pub quote_increment: f64,
    #[serde(default, deserialize_with = "number")]
    pub min_quantity: f64,
    #[serde(default, deserialize_with = "number")]
    pub min_notional_value: f64,
    #[serde(default, deserialize_with = "number")]
    pub qty_24hr: f64,
    #[serde(default, deserialize_with = "number")]
    pub notional_24hr: f64,
    #[serde(default, deserialize_with = "number")]
    pub open_interest: f64,
    #[serde(default, deserialize_with = "number")]
    pub funding_interval: f64,
    pub quote: Option<Quote>,
}
#[derive(Deserialize)]
pub(crate) struct Quote {
    #[serde(deserialize_with = "number")]
    pub best_bid_price: f64,
    #[serde(deserialize_with = "number")]
    pub best_ask_price: f64,
    #[serde(deserialize_with = "number")]
    pub trade_price: f64,
    #[serde(deserialize_with = "number")]
    pub mark_price: f64,
    #[serde(deserialize_with = "number")]
    pub index_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub predicted_funding: f64,
}
#[derive(Deserialize)]
pub(crate) struct Product {
    pub product_id: String,
    #[serde(default)]
    pub product_type: String,
    #[serde(default)]
    pub base_currency_id: String,
    pub quote_currency_id: String,
    pub status: String,
    #[serde(default)]
    pub is_disabled: bool,
    #[serde(default)]
    pub trading_disabled: bool,
    #[serde(deserialize_with = "number")]
    pub base_increment: f64,
    #[serde(deserialize_with = "number")]
    pub price_increment: f64,
    #[serde(deserialize_with = "number")]
    pub base_min_size: f64,
    #[serde(deserialize_with = "number")]
    pub base_max_size: f64,
    #[serde(default)]
    pub price: String,
    #[serde(default)]
    pub volume_24h: String,
    #[serde(default)]
    pub price_percentage_change_24h: String,
    pub future_product_details: Option<Future>,
}
#[derive(Deserialize)]
pub(crate) struct Future {
    #[serde(default)]
    pub contract_expiry_type: String,
    #[serde(default)]
    pub contract_root_unit: String,
    #[serde(default)]
    pub contract_size: String,
    pub perpetual_details: Option<Perpetual>,
}
#[derive(Deserialize)]
pub(crate) struct Perpetual {
    #[serde(default)]
    pub open_interest: String,
    #[serde(default)]
    pub funding_time: String,
}
#[derive(Deserialize)]
pub(crate) struct Envelope<'a> {
    #[serde(default, borrow)]
    pub channel: Cow<'a, str>,
    #[serde(default, rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    #[serde(default, borrow)]
    pub message: Cow<'a, str>,
    pub sequence_num: Option<u64>,
    #[serde(borrow)]
    pub events: Option<&'a RawValue>,
}
#[derive(Deserialize)]
pub(crate) struct DepthEvent<'a> {
    #[serde(rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    #[serde(borrow)]
    pub product_id: Cow<'a, str>,
    pub updates: Vec<Level<'a>>,
}
#[derive(Deserialize)]
pub(crate) struct Level<'a> {
    #[serde(borrow)]
    pub side: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    pub price_level: f64,
    #[serde(deserialize_with = "number")]
    pub new_quantity: f64,
}
#[derive(Deserialize)]
pub(crate) struct CandleRow {
    #[serde(deserialize_with = "number")]
    pub start: f64,
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
    #[serde(default)]
    pub product_id: String,
}
#[derive(Deserialize)]
pub(crate) struct CandleEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub candles: Vec<CandleRow>,
}
#[derive(Deserialize)]
pub(crate) struct TradeEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub trades: Vec<Trade>,
}
#[derive(Deserialize)]
pub(crate) struct Trade {
    pub product_id: String,
    pub time: String,
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub size: f64,
}
