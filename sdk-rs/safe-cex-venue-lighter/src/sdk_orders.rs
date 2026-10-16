//! SDK order request and statistics records.
use serde::{Deserialize, Serialize};
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
// ts: CancelOrderParams
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "CancelOrderParams", rename_all = "camelCase")]
struct CancelOrderParamsWire {
    #[serde(default, deserialize_with = "present")]
    order_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    client_order_id: Option<String>,
    symbol: String,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
}
impl<'de> Deserialize<'de> for CancelOrderParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        CancelOrderParamsWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}
// ts: CancelAllOrdersParams
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelAllOrdersParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "CancelAllOrdersParams", rename_all = "camelCase")]
struct CancelAllOrdersParamsWire {
    #[serde(default, deserialize_with = "present")]
    symbol: Option<String>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
}
impl<'de> Deserialize<'de> for CancelAllOrdersParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        CancelAllOrdersParamsWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}
// ts: QueryFillsParams
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryFillsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "QueryFillsParams", rename_all = "camelCase")]
struct QueryFillsParamsWire {
    #[serde(default, deserialize_with = "present")]
    symbol: Option<String>,
    #[serde(default, deserialize_with = "present")]
    order_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    start_time: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    end_time: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    page: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    page_size: Option<f64>,
}
impl<'de> Deserialize<'de> for QueryFillsParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        QueryFillsParamsWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}
// ts: OrderStats
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderStats {
    pub total_orders: f64,
    pub active_orders: f64,
    pub filled_orders: f64,
    pub cancelled_orders: f64,
    pub total_volume: String,
    pub total_fees: String,
    pub average_fill_rate: f64,
}
#[derive(Deserialize)]
#[serde(remote = "OrderStats", rename_all = "camelCase")]
struct OrderStatsWire {
    total_orders: f64,
    active_orders: f64,
    filled_orders: f64,
    cancelled_orders: f64,
    total_volume: String,
    total_fees: String,
    average_fill_rate: f64,
}
impl<'de> Deserialize<'de> for OrderStats {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        OrderStatsWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

use crate::sdk_protocol_types::{OrderSide, OrderStatus, OrderType, TimeInForce, TriggerStatus};

// ts: OrderInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderInfo {
    pub order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    pub market_id: String,
    pub symbol: String,
    pub account: String,
    pub r#type: OrderType,
    pub side: OrderSide,
    pub price: String,
    pub quantity: String,
    pub filled_quantity: String,
    pub remaining_quantity: String,
    pub average_price: String,
    pub status: OrderStatus,
    pub time_in_force: TimeInForce,
    pub expiry: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_status: Option<TriggerStatus>,
    pub reduce_only: bool,
    pub post_only: bool,
    pub fee: String,
    pub fee_token: String,
    pub created_at: f64,
    pub updated_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
    pub nonce: f64,
    pub account_index: f64,
    pub api_key_index: f64,
}
#[derive(Deserialize)]
#[serde(remote = "OrderInfo", rename_all = "camelCase")]
struct OrderInfoWire {
    order_id: String,
    #[serde(default, deserialize_with = "present")]
    client_order_id: Option<String>,
    market_id: String,
    symbol: String,
    account: String,
    r#type: OrderType,
    side: OrderSide,
    price: String,
    quantity: String,
    filled_quantity: String,
    remaining_quantity: String,
    average_price: String,
    status: OrderStatus,
    time_in_force: TimeInForce,
    expiry: f64,
    #[serde(default, deserialize_with = "present")]
    stop_price: Option<String>,
    #[serde(default, deserialize_with = "present")]
    trigger_status: Option<TriggerStatus>,
    reduce_only: bool,
    post_only: bool,
    fee: String,
    fee_token: String,
    created_at: f64,
    updated_at: f64,
    #[serde(default, deserialize_with = "present")]
    tx_hash: Option<String>,
    nonce: f64,
    account_index: f64,
    api_key_index: f64,
}
impl<'de> Deserialize<'de> for OrderInfo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        OrderInfoWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: Fill
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fill {
    pub fill_id: String,
    pub order_id: String,
    pub trade_id: String,
    pub symbol: String,
    pub price: String,
    pub quantity: String,
    pub quote_quantity: String,
    pub fee: String,
    pub fee_token: String,
    pub is_maker: bool,
    pub side: OrderSide,
    pub timestamp: f64,
}
#[derive(Deserialize)]
#[serde(remote = "Fill", rename_all = "camelCase")]
struct FillWire {
    fill_id: String,
    order_id: String,
    trade_id: String,
    symbol: String,
    price: String,
    quantity: String,
    quote_quantity: String,
    fee: String,
    fee_token: String,
    is_maker: bool,
    side: OrderSide,
    timestamp: f64,
}
impl<'de> Deserialize<'de> for Fill {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        FillWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: CreateLimitOrderParams
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLimitOrderParams {
    pub symbol: String,
    pub side: OrderSide,
    pub price: String,
    pub quantity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<TimeInForce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "CreateLimitOrderParams", rename_all = "camelCase")]
struct CreateLimitOrderParamsWire {
    symbol: String,
    side: OrderSide,
    price: String,
    quantity: String,
    #[serde(default, deserialize_with = "present")]
    time_in_force: Option<TimeInForce>,
    #[serde(default, deserialize_with = "present")]
    post_only: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    reduce_only: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    client_order_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    expiry: Option<f64>,
}
impl<'de> Deserialize<'de> for CreateLimitOrderParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        CreateLimitOrderParamsWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: CreateMarketOrderParams
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMarketOrderParams {
    pub symbol: String,
    pub side: OrderSide,
    pub quantity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "CreateMarketOrderParams", rename_all = "camelCase")]
struct CreateMarketOrderParamsWire {
    symbol: String,
    side: OrderSide,
    quantity: String,
    #[serde(default, deserialize_with = "present")]
    reduce_only: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    client_order_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
}
impl<'de> Deserialize<'de> for CreateMarketOrderParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        CreateMarketOrderParamsWire::deserialize(safe_cex_types::strict_decode::object(
            deserializer,
        ))
    }
}

// ts: CreateStopLossOrderParams
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStopLossOrderParams {
    pub symbol: String,
    pub side: OrderSide,
    pub stop_price: String,
    pub quantity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "CreateStopLossOrderParams", rename_all = "camelCase")]
struct CreateStopLossOrderParamsWire {
    symbol: String,
    side: OrderSide,
    stop_price: String,
    quantity: String,
    #[serde(default, deserialize_with = "present")]
    price: Option<String>,
    #[serde(default, deserialize_with = "present")]
    reduce_only: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    client_order_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    expiry: Option<f64>,
}
impl<'de> Deserialize<'de> for CreateStopLossOrderParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        CreateStopLossOrderParamsWire::deserialize(safe_cex_types::strict_decode::object(
            deserializer,
        ))
    }
}

// ts: CreateTakeProfitOrderParams
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTakeProfitOrderParams {
    pub symbol: String,
    pub side: OrderSide,
    pub take_profit_price: String,
    pub quantity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "CreateTakeProfitOrderParams", rename_all = "camelCase")]
struct CreateTakeProfitOrderParamsWire {
    symbol: String,
    side: OrderSide,
    take_profit_price: String,
    quantity: String,
    #[serde(default, deserialize_with = "present")]
    price: Option<String>,
    #[serde(default, deserialize_with = "present")]
    reduce_only: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    client_order_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    expiry: Option<f64>,
}
impl<'de> Deserialize<'de> for CreateTakeProfitOrderParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        CreateTakeProfitOrderParamsWire::deserialize(safe_cex_types::strict_decode::object(
            deserializer,
        ))
    }
}

// ts: QueryOrdersParams
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryOrdersParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<OrderStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<OrderSide>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<OrderType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "QueryOrdersParams", rename_all = "camelCase")]
struct QueryOrdersParamsWire {
    #[serde(default, deserialize_with = "present")]
    symbol: Option<String>,
    #[serde(default, deserialize_with = "present")]
    status: Option<OrderStatus>,
    #[serde(default, deserialize_with = "present")]
    side: Option<OrderSide>,
    #[serde(default, deserialize_with = "present")]
    r#type: Option<OrderType>,
    #[serde(default, deserialize_with = "present")]
    start_time: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    end_time: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    page: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    page_size: Option<f64>,
}
impl<'de> Deserialize<'de> for QueryOrdersParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        QueryOrdersParamsWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: ModifyOrderParams
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifyOrderParams {
    pub order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "ModifyOrderParams", rename_all = "camelCase")]
struct ModifyOrderParamsWire {
    order_id: String,
    #[serde(default, deserialize_with = "present")]
    price: Option<String>,
    #[serde(default, deserialize_with = "present")]
    quantity: Option<String>,
    #[serde(default, deserialize_with = "present")]
    expiry: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
}
impl<'de> Deserialize<'de> for ModifyOrderParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ModifyOrderParamsWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: BatchCancelOrdersParams
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchCancelOrdersParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "BatchCancelOrdersParams", rename_all = "camelCase")]
struct BatchCancelOrdersParamsWire {
    #[serde(default, deserialize_with = "present")]
    order_ids: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    client_order_ids: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    symbol: Option<String>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
}
impl<'de> Deserialize<'de> for BatchCancelOrdersParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BatchCancelOrdersParamsWire::deserialize(safe_cex_types::strict_decode::object(
            deserializer,
        ))
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for CreateLimitOrderParams {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, side, price, quantity, time_in_force, post_only, reduce_only, client_order_id, account_index, api_key_index, expiry });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for CreateMarketOrderParams {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, side, quantity, reduce_only, client_order_id, account_index, api_key_index });
    }
}
