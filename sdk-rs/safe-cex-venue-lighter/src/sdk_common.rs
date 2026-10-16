//! SDK common leaf records. String representation restrictions are declared
//! on their manifest rows; record decoders reject positional arrays and nulls.
use serde::{Deserialize, Serialize};

// A present optional field must decode as T: only absence supplies None.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

// ts: Balance
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    pub symbol: String,
    pub token_address: String,
    pub available: String,
    pub locked: String,
    pub total: String,
    pub decimals: f64,
}
#[derive(Deserialize)]
#[serde(remote = "Balance", rename_all = "camelCase")]
struct BalanceWire {
    symbol: String,
    token_address: String,
    available: String,
    locked: String,
    total: String,
    decimals: f64,
}
impl<'de> Deserialize<'de> for Balance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BalanceWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: Ticker
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticker {
    pub symbol: String,
    pub last_price: String,
    pub price_change: String,
    pub price_change_percent: String,
    pub high24h: String,
    pub low24h: String,
    pub volume24h: String,
    pub quote_volume24h: String,
    pub bid_price: String,
    pub bid_quantity: String,
    pub ask_price: String,
    pub ask_quantity: String,
    pub timestamp: f64,
}
#[derive(Deserialize)]
#[serde(remote = "Ticker", rename_all = "camelCase")]
struct TickerWire {
    symbol: String,
    last_price: String,
    price_change: String,
    price_change_percent: String,
    high24h: String,
    low24h: String,
    volume24h: String,
    quote_volume24h: String,
    bid_price: String,
    bid_quantity: String,
    ask_price: String,
    ask_quantity: String,
    timestamp: f64,
}
impl<'de> Deserialize<'de> for Ticker {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        TickerWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: OrderBookLevel
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderBookLevel {
    pub price: String,
    pub quantity: String,
}
#[derive(Deserialize)]
#[serde(remote = "OrderBookLevel", rename_all = "camelCase")]
struct OrderBookLevelWire {
    price: String,
    quantity: String,
}
impl<'de> Deserialize<'de> for OrderBookLevel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        OrderBookLevelWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderBookLevel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { price, quantity });
    }
}

// ts: PaginationParams
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginationParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "PaginationParams", rename_all = "camelCase")]
struct PaginationParamsWire {
    #[serde(default, deserialize_with = "present")]
    page: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    page_size: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    start_time: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    end_time: Option<f64>,
}
impl<'de> Deserialize<'de> for PaginationParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PaginationParamsWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: MarketStatus
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MarketStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "paused")]
    Paused,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "pre_launch")]
    PreLaunch,
}
#[derive(Deserialize)]
#[serde(remote = "MarketStatus")]
enum MarketStatusWire {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "paused")]
    Paused,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "pre_launch")]
    PreLaunch,
}
impl<'de> Deserialize<'de> for MarketStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        MarketStatusWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
// ts: LighterConfig
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LighterConfig {
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_delay: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "LighterConfig", rename_all = "camelCase")]
struct LighterConfigWire {
    base_url: String,
    #[serde(default, deserialize_with = "present")]
    ws_url: Option<String>,
    #[serde(default, deserialize_with = "present")]
    api_key: Option<String>,
    #[serde(default, deserialize_with = "present")]
    api_secret: Option<String>,
    #[serde(default, deserialize_with = "present")]
    private_key: Option<String>,
    #[serde(default, deserialize_with = "present")]
    chain_id: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    timeout: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    debug: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    max_retries: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    retry_delay: Option<f64>,
}
impl<'de> Deserialize<'de> for LighterConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        LighterConfigWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: Account
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub address: String,
    pub account_index: f64,
    pub public_key: String,
    pub nonce: f64,
    pub balances: Vec<Balance>,
    pub created_at: f64,
    pub updated_at: f64,
}
#[derive(Deserialize)]
#[serde(remote = "Account", rename_all = "camelCase")]
struct AccountWire {
    address: String,
    account_index: f64,
    public_key: String,
    nonce: f64,
    balances: Vec<Balance>,
    created_at: f64,
    updated_at: f64,
}
impl<'de> Deserialize<'de> for Account {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        AccountWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: Market
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    pub market_id: String,
    pub symbol: String,
    pub base_token: String,
    pub quote_token: String,
    pub base_token_address: String,
    pub quote_token_address: String,
    pub min_order_size: String,
    pub max_order_size: String,
    pub price_tick_size: String,
    pub quantity_tick_size: String,
    pub maker_fee: String,
    pub taker_fee: String,
    pub status: MarketStatus,
    pub base_decimals: f64,
    pub quote_decimals: f64,
}
#[derive(Deserialize)]
#[serde(remote = "Market", rename_all = "camelCase")]
struct MarketWire {
    market_id: String,
    symbol: String,
    base_token: String,
    quote_token: String,
    base_token_address: String,
    quote_token_address: String,
    min_order_size: String,
    max_order_size: String,
    price_tick_size: String,
    quantity_tick_size: String,
    maker_fee: String,
    taker_fee: String,
    status: MarketStatus,
    base_decimals: f64,
    quote_decimals: f64,
}
impl<'de> Deserialize<'de> for Market {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        MarketWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: Position
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub market_id: String,
    pub symbol: String,
    pub side: safe_cex_types::PositionSide,
    pub size: String,
    pub entry_price: String,
    pub mark_price: String,
    pub liquidation_price: String,
    pub unrealized_pnl: String,
    pub realized_pnl: String,
    pub margin: String,
    pub leverage: f64,
    pub notional: String,
    pub created_at: f64,
    pub updated_at: f64,
}
#[derive(Deserialize)]
#[serde(remote = "Position", rename_all = "camelCase")]
struct PositionWire {
    market_id: String,
    symbol: String,
    side: safe_cex_types::PositionSide,
    size: String,
    entry_price: String,
    mark_price: String,
    liquidation_price: String,
    unrealized_pnl: String,
    realized_pnl: String,
    margin: String,
    leverage: f64,
    notional: String,
    created_at: f64,
    updated_at: f64,
}
impl<'de> Deserialize<'de> for Position {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PositionWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: Trade
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub trade_id: String,
    pub symbol: String,
    pub price: String,
    pub quantity: String,
    pub quote_quantity: String,
    pub side: safe_cex_types::OrderSide,
    pub timestamp: f64,
    pub is_buyer_maker: bool,
}
#[derive(Deserialize)]
#[serde(remote = "Trade", rename_all = "camelCase")]
struct TradeWire {
    trade_id: String,
    symbol: String,
    price: String,
    quantity: String,
    quote_quantity: String,
    side: safe_cex_types::OrderSide,
    timestamp: f64,
    is_buyer_maker: bool,
}
impl<'de> Deserialize<'de> for Trade {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        TradeWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// ts: OrderBook
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub last_update_id: f64,
    pub timestamp: f64,
}
#[derive(Deserialize)]
#[serde(remote = "OrderBook", rename_all = "camelCase")]
struct OrderBookWire {
    symbol: String,
    bids: Vec<OrderBookLevel>,
    asks: Vec<OrderBookLevel>,
    last_update_id: f64,
    timestamp: f64,
}
impl<'de> Deserialize<'de> for OrderBook {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        OrderBookWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}
// ts: ApiResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T> {
    pub code: crate::sdk_protocol_types::ResultCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub timestamp: f64,
}
#[derive(Deserialize)]
#[serde(
    remote = "ApiResponse",
    rename_all = "camelCase",
    bound(deserialize = "T: Deserialize<'de>")
)]
struct ApiResponseWire<T> {
    code: crate::sdk_protocol_types::ResultCode,
    message: String,
    #[serde(default, deserialize_with = "present")]
    data: Option<T>,
    #[serde(default, deserialize_with = "present")]
    error: Option<String>,
    timestamp: f64,
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ApiResponse<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ApiResponseWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}
// ts: PaginatedResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: f64,
    pub page: f64,
    pub page_size: f64,
    pub total_pages: f64,
    pub has_next: bool,
    pub has_prev: bool,
}
#[derive(Deserialize)]
#[serde(
    remote = "PaginatedResponse",
    rename_all = "camelCase",
    bound(deserialize = "T: Deserialize<'de>")
)]
struct PaginatedResponseWire<T> {
    items: Vec<T>,
    total: f64,
    page: f64,
    page_size: f64,
    total_pages: f64,
    has_next: bool,
    has_prev: bool,
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for PaginatedResponse<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PaginatedResponseWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl<T: safe_cex_num::numeric_ingress::JsNumberOutput> safe_cex_num::numeric_ingress::JsNumberOutput
    for ApiResponse<T>
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, message, data, error, timestamp });
    }
}

impl<T: safe_cex_num::numeric_ingress::JsNumberOutput> safe_cex_num::numeric_ingress::JsNumberOutput
    for PaginatedResponse<T>
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { items, total, page, page_size, total_pages, has_next, has_prev });
    }
}
