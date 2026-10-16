//! SDK order API records; runtime requests are separate.
use crate::sdk_protocol_types::OrderStatus;
use serde::{Deserialize, Serialize};
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}

// ts: ApiOrder
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiOrder {
    pub order_id: String,
    pub market_id: f64,
    pub symbol: String,
    pub account_index: f64,
    pub price: String,
    pub quantity: String,
    pub filled_quantity: String,
    pub remaining_quantity: String,
    pub is_buy: bool,
    pub post_only: bool,
    pub reduce_only: bool,
    pub created_at: f64,
    pub updated_at: f64,
    pub status: OrderStatus,
}
#[derive(Deserialize)]
#[serde(remote = "ApiOrder", rename_all = "camelCase")]
struct ApiOrderWire {
    order_id: String,
    market_id: f64,
    symbol: String,
    account_index: f64,
    price: String,
    quantity: String,
    filled_quantity: String,
    remaining_quantity: String,
    is_buy: bool,
    post_only: bool,
    reduce_only: bool,
    created_at: f64,
    updated_at: f64,
    status: OrderStatus,
}
impl<'de> Deserialize<'de> for ApiOrder {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                ApiOrderWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ApiOrder {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { order_id, market_id, symbol, account_index, price, quantity, filled_quantity, remaining_quantity, is_buy, post_only, reduce_only, created_at, updated_at, status });
    }
}

// ts: AccountOrdersResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountOrdersResponse {
    pub orders: Vec<ApiOrder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(remote = "AccountOrdersResponse", rename_all = "camelCase")]
struct AccountOrdersResponseWire {
    orders: Vec<ApiOrder>,
    #[serde(default, deserialize_with = "present")]
    total: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    next_cursor: Option<String>,
}
impl<'de> Deserialize<'de> for AccountOrdersResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                AccountOrdersResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for AccountOrdersResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { orders, total, next_cursor });
    }
}

// Named representation of the source inline object.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiOrderBook {
    pub market_id: f64,
    pub symbol: String,
    pub best_bid: String,
    pub best_ask: String,
    pub bids: Vec<crate::sdk_common::OrderBookLevel>,
    pub asks: Vec<crate::sdk_common::OrderBookLevel>,
    pub timestamp: f64,
}
#[derive(Deserialize)]
#[serde(remote = "ApiOrderBook", rename_all = "camelCase")]
struct ApiOrderBookWire {
    market_id: f64,
    symbol: String,
    best_bid: String,
    best_ask: String,
    bids: Vec<crate::sdk_common::OrderBookLevel>,
    asks: Vec<crate::sdk_common::OrderBookLevel>,
    timestamp: f64,
}
impl<'de> Deserialize<'de> for ApiOrderBook {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                ApiOrderBookWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ApiOrderBook {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { market_id, symbol, best_bid, best_ask, bids, asks, timestamp });
    }
}

// ts: OrderBooksResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderBooksResponse {
    pub order_books: Vec<ApiOrderBook>,
}
#[derive(Deserialize)]
#[serde(remote = "OrderBooksResponse", rename_all = "camelCase")]
struct OrderBooksResponseWire {
    order_books: Vec<ApiOrderBook>,
}
impl<'de> Deserialize<'de> for OrderBooksResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                OrderBooksResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderBooksResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { order_books });
    }
}

// Named representation of the source inline object.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiOrderBookOrderLevel {
    pub price: String,
    pub quantity: String,
    pub order_count: f64,
}
#[derive(Deserialize)]
#[serde(remote = "ApiOrderBookOrderLevel", rename_all = "camelCase")]
struct ApiOrderBookOrderLevelWire {
    price: String,
    quantity: String,
    order_count: f64,
}
impl<'de> Deserialize<'de> for ApiOrderBookOrderLevel {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                ApiOrderBookOrderLevelWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ApiOrderBookOrderLevel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { price, quantity, order_count });
    }
}

// ts: OrderBookOrdersResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderBookOrdersResponse {
    pub market_id: f64,
    pub bids: Vec<ApiOrderBookOrderLevel>,
    pub asks: Vec<ApiOrderBookOrderLevel>,
    pub timestamp: f64,
}
#[derive(Deserialize)]
#[serde(remote = "OrderBookOrdersResponse", rename_all = "camelCase")]
struct OrderBookOrdersResponseWire {
    market_id: f64,
    bids: Vec<ApiOrderBookOrderLevel>,
    asks: Vec<ApiOrderBookOrderLevel>,
    timestamp: f64,
}
impl<'de> Deserialize<'de> for OrderBookOrdersResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                OrderBookOrdersResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderBookOrdersResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { market_id, bids, asks, timestamp });
    }
}

// Named representation of the source inline object.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiOrderBookDetails {
    pub market_id: f64,
    pub symbol: String,
    pub total_bid_volume: String,
    pub total_ask_volume: String,
    pub bid_order_count: f64,
    pub ask_order_count: f64,
    pub mid_price: String,
    pub spread: String,
    pub spread_percentage: String,
    pub last_update: f64,
}
#[derive(Deserialize)]
#[serde(remote = "ApiOrderBookDetails", rename_all = "camelCase")]
struct ApiOrderBookDetailsWire {
    market_id: f64,
    symbol: String,
    total_bid_volume: String,
    total_ask_volume: String,
    bid_order_count: f64,
    ask_order_count: f64,
    mid_price: String,
    spread: String,
    spread_percentage: String,
    last_update: f64,
}
impl<'de> Deserialize<'de> for ApiOrderBookDetails {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                ApiOrderBookDetailsWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ApiOrderBookDetails {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { market_id, symbol, total_bid_volume, total_ask_volume, bid_order_count, ask_order_count, mid_price, spread, spread_percentage, last_update });
    }
}

// ts: OrderBookDetailsResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderBookDetailsResponse {
    pub details: Vec<ApiOrderBookDetails>,
}
#[derive(Deserialize)]
#[serde(remote = "OrderBookDetailsResponse", rename_all = "camelCase")]
struct OrderBookDetailsResponseWire {
    details: Vec<ApiOrderBookDetails>,
}
impl<'de> Deserialize<'de> for OrderBookDetailsResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                OrderBookDetailsResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderBookDetailsResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { details });
    }
}
