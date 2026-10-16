//! SDK protocol value domains. Numeric TS enums admit number-typed values,
//! including unknown members and negative zero; literal aliases remain closed.
use serde::{Deserialize, Serialize};

// ts: TxTypeValue
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TxTypeValue {
    L2CreateOrder = 14,
    L2CancelOrder = 15,
    L2Transfer = 16,
    L2Withdraw = 17,
    L2UpdateApiKey = 18,
    L2Liquidation = 19,
    L2Deleverage = 20,
    L2FundingFee = 21,
    L2CreateGroupedOrders = 28,
}
impl Serialize for TxTypeValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}
impl<'de> Deserialize<'de> for TxTypeValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match f64::deserialize(deserializer)? {
            14.0 => Ok(Self::L2CreateOrder),
            15.0 => Ok(Self::L2CancelOrder),
            16.0 => Ok(Self::L2Transfer),
            17.0 => Ok(Self::L2Withdraw),
            18.0 => Ok(Self::L2UpdateApiKey),
            19.0 => Ok(Self::L2Liquidation),
            20.0 => Ok(Self::L2Deleverage),
            21.0 => Ok(Self::L2FundingFee),
            28.0 => Ok(Self::L2CreateGroupedOrders),
            _ => Err(serde::de::Error::custom("expected TxTypeValue literal")),
        }
    }
}

// ts: OrderType
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderType(pub f64);
impl OrderType {
    pub const LIMIT: Self = Self(0.0);
    pub const MARKET: Self = Self(1.0);
    pub const STOP_LOSS: Self = Self(2.0);
    pub const TAKE_PROFIT: Self = Self(3.0);
    pub const TWAP: Self = Self(4.0);
}

// ts: TimeInForce
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimeInForce(pub f64);
impl TimeInForce {
    pub const IMMEDIATE_OR_CANCEL: Self = Self(0.0);
    pub const GOOD_TILL_TIME: Self = Self(1.0);
    pub const POST_ONLY: Self = Self(2.0);
}

// ts: OrderStatus
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderStatus(pub f64);
impl OrderStatus {
    pub const PENDING: Self = Self(0.0);
    pub const OPEN: Self = Self(1.0);
    pub const PARTIALLY_FILLED: Self = Self(2.0);
    pub const FILLED: Self = Self(3.0);
    pub const CANCELLED: Self = Self(4.0);
    pub const REJECTED: Self = Self(5.0);
    pub const EXPIRED: Self = Self(6.0);
}

// ts: TriggerStatus
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TriggerStatus(pub f64);
impl TriggerStatus {
    pub const INACTIVE: Self = Self(0.0);
    pub const ACTIVE: Self = Self(1.0);
    pub const TRIGGERED: Self = Self(2.0);
    pub const CANCELLED: Self = Self(3.0);
}

// ts: MarginMode
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarginMode(pub f64);
impl MarginMode {
    pub const CROSS: Self = Self(0.0);
    pub const ISOLATED: Self = Self(1.0);
}

// ts: GroupingType
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupingType(pub f64);
impl GroupingType {
    pub const ONE_TRIGGERS_THE_OTHER: Self = Self(1.0);
    pub const ONE_CANCELS_THE_OTHER: Self = Self(2.0);
    pub const ONE_TRIGGERS_A_ONE_CANCELS_THE_OTHER: Self = Self(3.0);
}

// ts: OrderSide
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderSide(pub f64);
impl OrderSide {
    pub const BUY: Self = Self(0.0);
    pub const SELL: Self = Self(1.0);
}

// ts: WsMessageType
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
    #[serde(rename = "update")]
    Update,
    #[serde(rename = "error")]
    Error,
}
#[derive(Deserialize)]
#[serde(remote = "WsMessageType")]
enum WsMessageTypeWire {
    #[serde(rename = "subscribe")]
    Subscribe,
    #[serde(rename = "unsubscribe")]
    Unsubscribe,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "update")]
    Update,
    #[serde(rename = "error")]
    Error,
}
impl<'de> Deserialize<'de> for WsMessageType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WsMessageTypeWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}

// ts: WsChannel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WsChannel {
    #[serde(rename = "orderbook")]
    OrderBook,
    #[serde(rename = "trades")]
    Trades,
    #[serde(rename = "ticker")]
    Ticker,
    #[serde(rename = "orders")]
    Orders,
    #[serde(rename = "positions")]
    Positions,
    #[serde(rename = "balances")]
    Balances,
}
#[derive(Deserialize)]
#[serde(remote = "WsChannel")]
enum WsChannelWire {
    #[serde(rename = "orderbook")]
    OrderBook,
    #[serde(rename = "trades")]
    Trades,
    #[serde(rename = "ticker")]
    Ticker,
    #[serde(rename = "orders")]
    Orders,
    #[serde(rename = "positions")]
    Positions,
    #[serde(rename = "balances")]
    Balances,
}
impl<'de> Deserialize<'de> for WsChannel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WsChannelWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}

// ts: ResultCode
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResultCode(pub f64);
impl ResultCode {
    pub const SUCCESS: Self = Self(0.0);
    pub const INVALID_PARAMS: Self = Self(1.0);
    pub const AUTH_FAILED: Self = Self(2.0);
    pub const INSUFFICIENT_BALANCE: Self = Self(3.0);
    pub const ORDER_NOT_FOUND: Self = Self(4.0);
    pub const MARKET_NOT_FOUND: Self = Self(5.0);
    pub const RATE_LIMIT_EXCEEDED: Self = Self(6.0);
    pub const INTERNAL_ERROR: Self = Self(7.0);
    pub const NONCE_TOO_LOW: Self = Self(8.0);
    pub const NONCE_TOO_HIGH: Self = Self(9.0);
    pub const INVALID_SIGNATURE: Self = Self(10.0);
    pub const TX_EXPIRED: Self = Self(11.0);
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ResultCode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, tuple(value));
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, tuple(value));
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, tuple(value));
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for TimeInForce {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, tuple(value));
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for TxTypeValue {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { L2CreateOrder, L2CancelOrder, L2Transfer, L2Withdraw, L2UpdateApiKey, L2Liquidation, L2Deleverage, L2FundingFee, L2CreateGroupedOrders });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, tuple(value));
    }
}
