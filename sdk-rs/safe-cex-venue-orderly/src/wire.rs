//! Wire declarations from orderly.types.ts. All numeric entry passes through shared ingress.
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};
use serde::{Deserialize, Serialize};

// Absence is optional; a present null is not a TypeScript number/string/record.
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AccountMode {
    #[serde(rename = "FUTURES")]
    Futures,
}
#[derive(Deserialize)]
#[serde(remote = "AccountMode", rename_all = "UPPERCASE")]
enum AccountModeWire {
    Futures,
}
impl<'de> Deserialize<'de> for AccountMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        AccountModeWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
impl JsNumberOutput for AccountMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Futures });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EditOrderType {
    #[serde(rename = "LIMIT")]
    Limit,
}
#[derive(Deserialize)]
#[serde(remote = "EditOrderType", rename_all = "UPPERCASE")]
enum EditOrderTypeWire {
    Limit,
}
impl<'de> Deserialize<'de> for EditOrderType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        EditOrderTypeWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
impl JsNumberOutput for EditOrderType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Limit });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EditOrderSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
#[derive(Deserialize)]
#[serde(remote = "EditOrderSide", rename_all = "UPPERCASE")]
enum EditOrderSideWire {
    Buy,
    Sell,
}
impl<'de> Deserialize<'de> for EditOrderSide {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        EditOrderSideWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
impl JsNumberOutput for EditOrderSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Buy, Sell });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountResponse {
    pub account_id: String,
    pub email: String,
    pub account_mode: AccountMode,
    pub max_leverage: f64,
    pub taker_fee_rate: f64,
    pub maker_fee_rate: f64,
    pub futures_taker_fee_rate: f64,
    pub futures_maker_fee_rate: f64,
    pub maintenance_cancel_orders: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "AccountResponse")]
struct AccountResponseWire {
    account_id: String,
    email: String,
    account_mode: AccountMode,
    max_leverage: f64,
    taker_fee_rate: f64,
    maker_fee_rate: f64,
    futures_taker_fee_rate: f64,
    futures_maker_fee_rate: f64,
    maintenance_cancel_orders: bool,
}
impl Serialize for AccountResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        AccountResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for AccountResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                AccountResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for AccountResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account_id, email, account_mode, max_leverage, taker_fee_rate, maker_fee_rate, futures_taker_fee_rate, futures_maker_fee_rate, maintenance_cancel_orders });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BalanceHolding {
    pub updated_time: f64,
    pub token: String,
    pub holding: f64,
    pub frozen: f64,
    pub pending_short: f64,
    pub available_balance: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "BalanceHolding")]
struct BalanceHoldingWire {
    updated_time: f64,
    token: String,
    holding: f64,
    frozen: f64,
    pending_short: f64,
    #[serde(rename = "availableBalance")]
    available_balance: f64,
}
impl Serialize for BalanceHolding {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BalanceHoldingWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for BalanceHolding {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                BalanceHoldingWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BalanceHolding {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { updated_time, token, holding, frozen, pending_short, available_balance });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BalanceResponse {
    pub holding: Vec<BalanceHolding>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "BalanceResponse")]
struct BalanceResponseWire {
    holding: Vec<BalanceHolding>,
}
impl Serialize for BalanceResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BalanceResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for BalanceResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                BalanceResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BalanceResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { holding });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaceOrderData {
    pub order_id: f64,
    pub order_type: String,
    pub order_price: f64,
    pub order_quantity: f64,
    pub order_amount: f64,
    pub client_order_id: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PlaceOrderData")]
struct PlaceOrderDataWire {
    order_id: f64,
    order_type: String,
    order_price: f64,
    order_quantity: f64,
    order_amount: f64,
    client_order_id: f64,
}
impl Serialize for PlaceOrderData {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        PlaceOrderDataWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for PlaceOrderData {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                PlaceOrderDataWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for PlaceOrderData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { order_id, order_type, order_price, order_quantity, order_amount, client_order_id });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaceOrderResponse {
    pub success: bool,
    pub timestamp: String,
    pub data: PlaceOrderData,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PlaceOrderResponse")]
struct PlaceOrderResponseWire {
    success: bool,
    timestamp: String,
    data: PlaceOrderData,
}
impl Serialize for PlaceOrderResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        PlaceOrderResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for PlaceOrderResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                PlaceOrderResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for PlaceOrderResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { success, timestamp, data });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditOrderRequest {
    pub order_id: String,
    pub symbol: String,
    pub order_type: EditOrderType,
    pub side: EditOrderSide,
    pub order_price: Option<f64>,
    pub order_quantity: Option<f64>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "EditOrderRequest")]
struct EditOrderRequestWire {
    order_id: String,
    symbol: String,
    order_type: EditOrderType,
    side: EditOrderSide,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    order_price: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    order_quantity: Option<f64>,
}
impl Serialize for EditOrderRequest {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        EditOrderRequestWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for EditOrderRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                EditOrderRequestWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for EditOrderRequest {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { order_id, symbol, order_type, side, order_price, order_quantity });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditOrderData {
    pub status: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "EditOrderData")]
struct EditOrderDataWire {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    status: Option<String>,
}
impl Serialize for EditOrderData {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        EditOrderDataWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for EditOrderData {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                EditOrderDataWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for EditOrderData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { status });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditOrderResponse {
    pub success: bool,
    pub timestamp: Option<f64>,
    pub code: Option<f64>,
    pub message: Option<String>,
    pub data: Option<EditOrderData>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "EditOrderResponse")]
struct EditOrderResponseWire {
    success: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    timestamp: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    code: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    message: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    data: Option<EditOrderData>,
}
impl Serialize for EditOrderResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        EditOrderResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for EditOrderResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                EditOrderResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for EditOrderResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { success, timestamp, code, message, data });
    }
}
