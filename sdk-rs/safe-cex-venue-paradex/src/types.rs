//! Declared Paradex response shapes, corresponding to paradex.types.ts.
use serde::{Deserialize, Serialize};

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MarketResponseStatus {
    #[serde(rename = "ACTIVE")]
    Active,
    #[serde(rename = "INACTIVE")]
    Inactive,
}
impl<'de> Deserialize<'de> for MarketResponseStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "ACTIVE" => Ok(Self::Active),
            "INACTIVE" => Ok(Self::Inactive),
            _ => Err(serde::de::Error::custom("unknown MarketResponseStatus")),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for MarketResponseStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Active, Inactive });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderResponseSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
impl<'de> Deserialize<'de> for OrderResponseSide {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "BUY" => Ok(Self::Buy),
            "SELL" => Ok(Self::Sell),
            _ => Err(serde::de::Error::custom("unknown OrderResponseSide")),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Buy, Sell });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderResponseStatus {
    #[serde(rename = "NEW")]
    New,
    #[serde(rename = "OPEN")]
    Open,
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "CANCELED")]
    Canceled,
    #[serde(rename = "UNTRIGGERED")]
    Untriggered,
    #[serde(rename = "REJECTED")]
    Rejected,
}
impl<'de> Deserialize<'de> for OrderResponseStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "NEW" => Ok(Self::New),
            "OPEN" => Ok(Self::Open),
            "PENDING" => Ok(Self::Pending),
            "FILLED" => Ok(Self::Filled),
            "CANCELED" => Ok(Self::Canceled),
            "UNTRIGGERED" => Ok(Self::Untriggered),
            "REJECTED" => Ok(Self::Rejected),
            _ => Err(serde::de::Error::custom("unknown OrderResponseStatus")),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { New, Open, Pending, Filled, Canceled, Untriggered, Rejected });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderResponseType {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "STOP_LIMIT")]
    StopLimit,
    #[serde(rename = "STOP_MARKET")]
    StopMarket,
    #[serde(rename = "STOP_LOSS_LIMIT")]
    StopLossLimit,
    #[serde(rename = "STOP_LOSS_MARKET")]
    StopLossMarket,
    #[serde(rename = "TAKE_PROFIT_LIMIT")]
    TakeProfitLimit,
    #[serde(rename = "TAKE_PROFIT_MARKET")]
    TakeProfitMarket,
}
impl<'de> Deserialize<'de> for OrderResponseType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "LIMIT" => Ok(Self::Limit),
            "MARKET" => Ok(Self::Market),
            "STOP_LIMIT" => Ok(Self::StopLimit),
            "STOP_MARKET" => Ok(Self::StopMarket),
            "STOP_LOSS_LIMIT" => Ok(Self::StopLossLimit),
            "STOP_LOSS_MARKET" => Ok(Self::StopLossMarket),
            "TAKE_PROFIT_LIMIT" => Ok(Self::TakeProfitLimit),
            "TAKE_PROFIT_MARKET" => Ok(Self::TakeProfitMarket),
            _ => Err(serde::de::Error::custom("unknown OrderResponseType")),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Limit, Market, StopLimit, StopMarket, StopLossLimit, StopLossMarket, TakeProfitLimit, TakeProfitMarket });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderResponseInstruction {
    #[serde(rename = "GTC")]
    Gtc,
    #[serde(rename = "POST_ONLY")]
    PostOnly,
    #[serde(rename = "IOC")]
    Ioc,
    #[serde(rename = "RPI")]
    Rpi,
}
impl<'de> Deserialize<'de> for OrderResponseInstruction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "GTC" => Ok(Self::Gtc),
            "POST_ONLY" => Ok(Self::PostOnly),
            "IOC" => Ok(Self::Ioc),
            "RPI" => Ok(Self::Rpi),
            _ => Err(serde::de::Error::custom("unknown OrderResponseInstruction")),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseInstruction {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Gtc, PostOnly, Ioc, Rpi });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderResponseStp {
    #[serde(rename = "EXPIRE_MAKER")]
    ExpireMaker,
    #[serde(rename = "EXPIRE_TAKER")]
    ExpireTaker,
    #[serde(rename = "EXPIRE_BOTH")]
    ExpireBoth,
}
impl<'de> Deserialize<'de> for OrderResponseStp {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "EXPIRE_MAKER" => Ok(Self::ExpireMaker),
            "EXPIRE_TAKER" => Ok(Self::ExpireTaker),
            "EXPIRE_BOTH" => Ok(Self::ExpireBoth),
            _ => Err(serde::de::Error::custom("unknown OrderResponseStp")),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseStp {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { ExpireMaker, ExpireTaker, ExpireBoth });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderResponseTimeInForce {
    #[serde(rename = "GTT")]
    Gtt,
    #[serde(rename = "FOK")]
    Fok,
    #[serde(rename = "IOC")]
    Ioc,
}
impl<'de> Deserialize<'de> for OrderResponseTimeInForce {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "GTT" => Ok(Self::Gtt),
            "FOK" => Ok(Self::Fok),
            "IOC" => Ok(Self::Ioc),
            _ => Err(serde::de::Error::custom("unknown OrderResponseTimeInForce")),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponseTimeInForce {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Gtt, Fok, Ioc });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BatchCancelOrderResultStatus {
    #[serde(rename = "QUEUED_FOR_CANCELLATION")]
    QueuedForCancellation,
    #[serde(rename = "ALREADY_CLOSED")]
    AlreadyClosed,
    #[serde(rename = "NOT_FOUND")]
    NotFound,
}
impl<'de> Deserialize<'de> for BatchCancelOrderResultStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "QUEUED_FOR_CANCELLATION" => Ok(Self::QueuedForCancellation),
            "ALREADY_CLOSED" => Ok(Self::AlreadyClosed),
            "NOT_FOUND" => Ok(Self::NotFound),
            _ => Err(serde::de::Error::custom(
                "unknown BatchCancelOrderResultStatus",
            )),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BatchCancelOrderResultStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { QueuedForCancellation, AlreadyClosed, NotFound });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PositionResponseStatus {
    #[serde(rename = "OPEN")]
    Open,
    #[serde(rename = "CLOSED")]
    Closed,
    #[serde(rename = "LIQUIDATED")]
    Liquidated,
}
impl<'de> Deserialize<'de> for PositionResponseStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "OPEN" => Ok(Self::Open),
            "CLOSED" => Ok(Self::Closed),
            "LIQUIDATED" => Ok(Self::Liquidated),
            _ => Err(serde::de::Error::custom("unknown PositionResponseStatus")),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PositionResponseStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Open, Closed, Liquidated });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PositionResponseSide {
    #[serde(rename = "LONG")]
    Long,
    #[serde(rename = "SHORT")]
    Short,
}
impl<'de> Deserialize<'de> for PositionResponseSide {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "LONG" => Ok(Self::Long),
            "SHORT" => Ok(Self::Short),
            _ => Err(serde::de::Error::custom("unknown PositionResponseSide")),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PositionResponseSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Long, Short });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TradeResponseSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
impl<'de> Deserialize<'de> for TradeResponseSide {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "BUY" => Ok(Self::Buy),
            "SELL" => Ok(Self::Sell),
            _ => Err(serde::de::Error::custom("unknown TradeResponseSide")),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for TradeResponseSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Buy, Sell });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MarginConfigEntryMarginType {
    #[serde(rename = "CROSS")]
    Cross,
    #[serde(rename = "ISOLATED")]
    Isolated,
}
impl<'de> Deserialize<'de> for MarginConfigEntryMarginType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "CROSS" => Ok(Self::Cross),
            "ISOLATED" => Ok(Self::Isolated),
            _ => Err(serde::de::Error::custom(
                "unknown MarginConfigEntryMarginType",
            )),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for MarginConfigEntryMarginType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Cross, Isolated });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WsMessageType {
    #[serde(rename = "subscribe")]
    Subscribe,
    #[serde(rename = "unsubscribe")]
    Unsubscribe,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "trades")]
    Trades,
    #[serde(rename = "orderbook")]
    Orderbook,
    #[serde(rename = "ticker")]
    Ticker,
    #[serde(rename = "orders")]
    Orders,
    #[serde(rename = "positions")]
    Positions,
    #[serde(rename = "balances")]
    Balances,
}
impl<'de> Deserialize<'de> for WsMessageType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "subscribe" => Ok(Self::Subscribe),
            "unsubscribe" => Ok(Self::Unsubscribe),
            "ping" => Ok(Self::Ping),
            "pong" => Ok(Self::Pong),
            "error" => Ok(Self::Error),
            "trades" => Ok(Self::Trades),
            "orderbook" => Ok(Self::Orderbook),
            "ticker" => Ok(Self::Ticker),
            "orders" => Ok(Self::Orders),
            "positions" => Ok(Self::Positions),
            "balances" => Ok(Self::Balances),
            _ => Err(serde::de::Error::custom("unknown WsMessageType")),
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WsMessageType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Subscribe, Unsubscribe, Ping, Pong, Error, Trades, Orderbook, Ticker, Orders, Positions, Balances });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountResponse {
    pub account: String,
    pub account_number: String,
    pub address: String,
    pub public_key: String,
    pub initial_margin_requirement: String,
    pub maintenance_margin_requirement: String,
    pub account_value: String,
    pub total_collateral: String,
    pub free_collateral: String,
    pub margin_cushion: String,
    pub settlement_asset: String,
    pub updated_at: f64,
    pub status: String,
    pub seq_no: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "AccountResponse")]
struct AccountResponseWire {
    account: String,
    account_number: String,
    address: String,
    public_key: String,
    initial_margin_requirement: String,
    maintenance_margin_requirement: String,
    account_value: String,
    total_collateral: String,
    free_collateral: String,
    margin_cushion: String,
    settlement_asset: String,
    updated_at: f64,
    status: String,
    seq_no: f64,
}
impl Serialize for AccountResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        AccountResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for AccountResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                AccountResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for AccountResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account, account_number, address, public_key, initial_margin_requirement, maintenance_margin_requirement, account_value, total_collateral, free_collateral, margin_cushion, settlement_asset, updated_at, status, seq_no });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BalanceResponse {
    pub asset: String,
    pub free: String,
    pub locked: String,
    pub wallet_balance: String,
    pub unrealized_pnl: String,
    pub margin_balance: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BalanceResponse")]
struct BalanceResponseWire {
    asset: String,
    free: String,
    locked: String,
    wallet_balance: String,
    unrealized_pnl: String,
    margin_balance: String,
}
impl Serialize for BalanceResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BalanceResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for BalanceResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                BalanceResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BalanceResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { asset, free, locked, wallet_balance, unrealized_pnl, margin_balance });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketResponseDelta1CrossMarginParams {
    pub imf_base: Option<String>,
    pub imf_factor: Option<String>,
    pub imf_shift: Option<String>,
    pub mmf_factor: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "MarketResponseDelta1CrossMarginParams")]
struct MarketResponseDelta1CrossMarginParamsWire {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    imf_base: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    imf_factor: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    imf_shift: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    mmf_factor: Option<String>,
}
impl Serialize for MarketResponseDelta1CrossMarginParams {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        MarketResponseDelta1CrossMarginParamsWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for MarketResponseDelta1CrossMarginParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                MarketResponseDelta1CrossMarginParamsWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for MarketResponseDelta1CrossMarginParams {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { imf_base, imf_factor, imf_shift, mmf_factor });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketResponse {
    pub market: Option<String>,
    pub symbol: Option<String>,
    pub status: Option<MarketResponseStatus>,
    pub base_currency: String,
    pub quote_currency: String,
    pub settlement_currency: Option<String>,
    pub step_size: Option<String>,
    pub order_size_increment: Option<String>,
    pub tick_size: Option<String>,
    pub price_tick_size: Option<String>,
    pub index_price: Option<String>,
    pub oracle_price: Option<String>,
    pub min_order_size: Option<String>,
    pub min_notional: Option<String>,
    pub max_order_size: Option<String>,
    pub max_position_size: Option<String>,
    pub position_limit: Option<String>,
    pub initial_margin_fraction: Option<String>,
    pub maintenance_margin_fraction: Option<String>,
    pub baseline_position_size: Option<String>,
    pub incremental_position_size: Option<String>,
    pub incremental_initial_margin_fraction: Option<String>,
    pub delta1_cross_margin_params: Option<MarketResponseDelta1CrossMarginParams>,
    pub maker_fee: Option<String>,
    pub taker_fee: Option<String>,
    pub asset_kind: Option<String>,
    pub market_kind: Option<String>,
    pub option_type: Option<String>,
    pub strike_price: Option<String>,
    pub expiry_at: Option<f64>,
    pub open_at: Option<f64>,
    pub max_open_orders: Option<f64>,
    pub price_bands_width: Option<String>,
    pub max_slippage: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "MarketResponse")]
struct MarketResponseWire {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    market: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    symbol: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    status: Option<MarketResponseStatus>,
    base_currency: String,
    quote_currency: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    settlement_currency: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    step_size: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    order_size_increment: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    tick_size: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    price_tick_size: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    index_price: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    oracle_price: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    min_order_size: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    min_notional: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    max_order_size: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    max_position_size: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    position_limit: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    initial_margin_fraction: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    maintenance_margin_fraction: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    baseline_position_size: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    incremental_position_size: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    incremental_initial_margin_fraction: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    delta1_cross_margin_params: Option<MarketResponseDelta1CrossMarginParams>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    maker_fee: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    taker_fee: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    asset_kind: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    market_kind: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    option_type: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    strike_price: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    expiry_at: Option<f64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    open_at: Option<f64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    max_open_orders: Option<f64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    price_bands_width: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    max_slippage: Option<String>,
}
impl Serialize for MarketResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        MarketResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for MarketResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                MarketResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for MarketResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { market, symbol, status, base_currency, quote_currency, settlement_currency, step_size, order_size_increment, tick_size, price_tick_size, index_price, oracle_price, min_order_size, min_notional, max_order_size, max_position_size, position_limit, initial_margin_fraction, maintenance_margin_fraction, baseline_position_size, incremental_position_size, incremental_initial_margin_fraction, delta1_cross_margin_params, maker_fee, taker_fee, asset_kind, market_kind, option_type, strike_price, expiry_at, open_at, max_open_orders, price_bands_width, max_slippage });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderResponse {
    pub id: String,
    pub client_id: Option<String>,
    pub client_order_id: Option<String>,
    pub market: String,
    pub side: OrderResponseSide,
    pub size: String,
    pub remaining_size: String,
    pub price: String,
    pub average_fill_price: String,
    pub status: OrderResponseStatus,
    pub r#type: OrderResponseType,
    pub instruction: Option<OrderResponseInstruction>,
    pub flags: Option<Vec<String>>,
    pub stp: Option<OrderResponseStp>,
    pub on_behalf_of_account: Option<String>,
    pub time_in_force: Option<OrderResponseTimeInForce>,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
    pub trigger_price: Option<String>,
    pub post_only: Option<bool>,
    pub reduce_only: Option<bool>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderResponse")]
struct OrderResponseWire {
    id: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    client_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    client_order_id: Option<String>,
    market: String,
    side: OrderResponseSide,
    size: String,
    remaining_size: String,
    price: String,
    average_fill_price: String,
    status: OrderResponseStatus,
    r#type: OrderResponseType,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    instruction: Option<OrderResponseInstruction>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    flags: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    stp: Option<OrderResponseStp>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    on_behalf_of_account: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    time_in_force: Option<OrderResponseTimeInForce>,
    created_at: String,
    updated_at: String,
    expires_at: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    trigger_price: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    post_only: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    reduce_only: Option<bool>,
}
impl Serialize for OrderResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        OrderResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for OrderResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                OrderResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, client_id, client_order_id, market, side, size, remaining_size, price, average_fill_price, status, r#type, instruction, flags, stp, on_behalf_of_account, time_in_force, created_at, updated_at, expires_at, trigger_price, post_only, reduce_only });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchOrderError {
    pub error: Option<String>,
    pub message: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BatchOrderError")]
struct BatchOrderErrorWire {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    error: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    message: Option<String>,
}
impl Serialize for BatchOrderError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BatchOrderErrorWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for BatchOrderError {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                BatchOrderErrorWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BatchOrderError {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { error, message });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchOrderResponse {
    pub errors: Option<Vec<BatchOrderError>>,
    pub orders: Option<Vec<OrderResponse>>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BatchOrderResponse")]
struct BatchOrderResponseWire {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    errors: Option<Vec<BatchOrderError>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    orders: Option<Vec<OrderResponse>>,
}
impl Serialize for BatchOrderResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BatchOrderResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for BatchOrderResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                BatchOrderResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BatchOrderResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { errors, orders });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchCancelOrderResult {
    pub account: Option<String>,
    pub client_id: Option<String>,
    pub id: Option<String>,
    pub market: Option<String>,
    pub status: Option<BatchCancelOrderResultStatus>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BatchCancelOrderResult")]
struct BatchCancelOrderResultWire {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    account: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    client_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    market: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    status: Option<BatchCancelOrderResultStatus>,
}
impl Serialize for BatchCancelOrderResult {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BatchCancelOrderResultWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for BatchCancelOrderResult {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                BatchCancelOrderResultWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BatchCancelOrderResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account, client_id, id, market, status });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchCancelOrderResponse {
    pub results: Option<Vec<BatchCancelOrderResult>>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BatchCancelOrderResponse")]
struct BatchCancelOrderResponseWire {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    results: Option<Vec<BatchCancelOrderResult>>,
}
impl Serialize for BatchCancelOrderResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BatchCancelOrderResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for BatchCancelOrderResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                BatchCancelOrderResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BatchCancelOrderResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { results });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionResponse {
    pub id: String,
    pub account: String,
    pub market: String,
    pub status: PositionResponseStatus,
    pub side: PositionResponseSide,
    pub size: String,
    pub average_entry_price: String,
    pub average_entry_price_usd: String,
    pub average_exit_price: String,
    pub unrealized_pnl: String,
    pub unrealized_funding_pnl: String,
    pub cost: String,
    pub cost_usd: String,
    pub cached_funding_index: String,
    pub last_updated_at: f64,
    pub created_at: f64,
    pub last_fill_id: String,
    pub seq_no: f64,
    pub liquidation_price: String,
    pub leverage: String,
    pub realized_positional_pnl: String,
    pub realized_positional_funding_pnl: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "PositionResponse")]
struct PositionResponseWire {
    id: String,
    account: String,
    market: String,
    status: PositionResponseStatus,
    side: PositionResponseSide,
    size: String,
    average_entry_price: String,
    average_entry_price_usd: String,
    average_exit_price: String,
    unrealized_pnl: String,
    unrealized_funding_pnl: String,
    cost: String,
    cost_usd: String,
    cached_funding_index: String,
    last_updated_at: f64,
    created_at: f64,
    last_fill_id: String,
    seq_no: f64,
    liquidation_price: String,
    leverage: String,
    realized_positional_pnl: String,
    realized_positional_funding_pnl: String,
}
impl Serialize for PositionResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        PositionResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for PositionResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                PositionResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PositionResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, account, market, status, side, size, average_entry_price, average_entry_price_usd, average_exit_price, unrealized_pnl, unrealized_funding_pnl, cost, cost_usd, cached_funding_index, last_updated_at, created_at, last_fill_id, seq_no, liquidation_price, leverage, realized_positional_pnl, realized_positional_funding_pnl });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TickerResponseGreeks {
    pub delta: String,
    pub gamma: String,
    pub vega: String,
    pub rho: Option<String>,
    pub vanna: Option<String>,
    pub volga: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "TickerResponseGreeks")]
struct TickerResponseGreeksWire {
    delta: String,
    gamma: String,
    vega: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    rho: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    vanna: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    volga: Option<String>,
}
impl Serialize for TickerResponseGreeks {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        TickerResponseGreeksWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for TickerResponseGreeks {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                TickerResponseGreeksWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for TickerResponseGreeks {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { delta, gamma, vega, rho, vanna, volga });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TickerResponse {
    pub symbol: String,
    pub mark_price: String,
    pub last_traded_price: String,
    pub bid: String,
    pub ask: String,
    pub volume_24h: String,
    pub open_interest: String,
    pub funding_rate: String,
    pub price_change_rate_24h: String,
    pub underlying_price: Option<String>,
    pub delta: Option<String>,
    pub mark_iv: Option<String>,
    pub greeks: Option<TickerResponseGreeks>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "TickerResponse")]
struct TickerResponseWire {
    symbol: String,
    mark_price: String,
    last_traded_price: String,
    bid: String,
    ask: String,
    volume_24h: String,
    open_interest: String,
    funding_rate: String,
    price_change_rate_24h: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    underlying_price: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    delta: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    mark_iv: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    greeks: Option<TickerResponseGreeks>,
}
impl Serialize for TickerResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        TickerResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for TickerResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                TickerResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for TickerResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, mark_price, last_traded_price, bid, ask, volume_24h, open_interest, funding_rate, price_change_rate_24h, underlying_price, delta, mark_iv, greeks });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderbookResponse {
    pub bids: Vec<(String, String)>,
    pub asks: Vec<(String, String)>,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderbookResponse")]
struct OrderbookResponseWire {
    bids: Vec<(String, String)>,
    asks: Vec<(String, String)>,
    timestamp: f64,
}
impl Serialize for OrderbookResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        OrderbookResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for OrderbookResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                OrderbookResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderbookResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { bids, asks, timestamp });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradeResponse {
    pub id: String,
    pub price: String,
    pub size: String,
    pub side: TradeResponseSide,
    pub created_at: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "TradeResponse")]
struct TradeResponseWire {
    id: String,
    price: String,
    size: String,
    side: TradeResponseSide,
    created_at: String,
}
impl Serialize for TradeResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        TradeResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for TradeResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                TradeResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for TradeResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, price, size, side, created_at });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarginConfigEntry {
    pub market: String,
    pub leverage: f64,
    pub margin_type: MarginConfigEntryMarginType,
    pub isolated_margin_leverage: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "MarginConfigEntry")]
struct MarginConfigEntryWire {
    market: String,
    leverage: f64,
    margin_type: MarginConfigEntryMarginType,
    isolated_margin_leverage: f64,
}
impl Serialize for MarginConfigEntry {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        MarginConfigEntryWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for MarginConfigEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                MarginConfigEntryWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for MarginConfigEntry {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { market, leverage, margin_type, isolated_margin_leverage });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarginResponse {
    pub account: String,
    pub margin_methodology: String,
    pub configs: Vec<MarginConfigEntry>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "MarginResponse")]
struct MarginResponseWire {
    account: String,
    margin_methodology: String,
    configs: Vec<MarginConfigEntry>,
}
impl Serialize for MarginResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        MarginResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for MarginResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                MarginResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for MarginResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account, margin_methodology, configs });
    }
}
