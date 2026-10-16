//! SDK transaction request records; no transaction validation is implied.
use crate::sdk_protocol_types::{OrderSide, OrderType, TimeInForce};
use serde::{Deserialize, Serialize};
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
// ts: TransactOpts
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactOpts {
    pub nonce: f64,
    pub expiry: f64,
    pub account_index: f64,
    pub api_key_index: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_limit: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "TransactOpts", rename_all = "camelCase")]
struct TransactOptsWire {
    nonce: f64,
    expiry: f64,
    account_index: f64,
    api_key_index: f64,
    #[serde(default, deserialize_with = "present")]
    gas_price: Option<String>,
    #[serde(default, deserialize_with = "present")]
    gas_limit: Option<f64>,
}
impl<'de> Deserialize<'de> for TransactOpts {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                TransactOptsWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for TransactOpts {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { nonce, expiry, account_index, api_key_index, gas_price, gas_limit });
    }
}
// ts: CreateOrderTxReq
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOrderTxReq {
    pub symbol: String,
    pub r#type: OrderType,
    pub side: OrderSide,
    pub price: String,
    pub quantity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<TimeInForce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "CreateOrderTxReq", rename_all = "camelCase")]
struct CreateOrderTxReqWire {
    symbol: String,
    r#type: OrderType,
    side: OrderSide,
    price: String,
    quantity: String,
    #[serde(default, deserialize_with = "present")]
    time_in_force: Option<TimeInForce>,
    #[serde(default, deserialize_with = "present")]
    reduce_only: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    post_only: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    stop_price: Option<String>,
    #[serde(default, deserialize_with = "present")]
    client_order_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    nonce: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    expiry: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
}
impl<'de> Deserialize<'de> for CreateOrderTxReq {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                CreateOrderTxReqWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for CreateOrderTxReq {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, r#type, side, price, quantity, time_in_force, reduce_only, post_only, stop_price, client_order_id, nonce, expiry, account_index, api_key_index });
    }
}
// ts: CancelOrderTxReq
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderTxReq {
    pub symbol: String,
    pub order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "CancelOrderTxReq", rename_all = "camelCase")]
struct CancelOrderTxReqWire {
    symbol: String,
    order_id: String,
    #[serde(default, deserialize_with = "present")]
    nonce: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    expiry: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
}
impl<'de> Deserialize<'de> for CancelOrderTxReq {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                CancelOrderTxReqWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for CancelOrderTxReq {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, order_id, nonce, expiry, account_index, api_key_index });
    }
}
// ts: TransferTxReq
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferTxReq {
    pub token: String,
    pub recipient: String,
    pub amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "TransferTxReq", rename_all = "camelCase")]
struct TransferTxReqWire {
    token: String,
    recipient: String,
    amount: String,
    #[serde(default, deserialize_with = "present")]
    memo: Option<String>,
    #[serde(default, deserialize_with = "present")]
    nonce: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    expiry: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
}
impl<'de> Deserialize<'de> for TransferTxReq {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                TransferTxReqWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for TransferTxReq {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { token, recipient, amount, memo, nonce, expiry, account_index, api_key_index });
    }
}
// ts: WithdrawTxReq
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawTxReq {
    pub token: String,
    pub amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l1_recipient: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_withdraw: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "WithdrawTxReq", rename_all = "camelCase")]
struct WithdrawTxReqWire {
    token: String,
    amount: String,
    #[serde(default, deserialize_with = "present")]
    l1_recipient: Option<String>,
    #[serde(default, deserialize_with = "present")]
    fast_withdraw: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    nonce: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    expiry: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
}
impl<'de> Deserialize<'de> for WithdrawTxReq {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                WithdrawTxReqWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WithdrawTxReq {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { token, amount, l1_recipient, fast_withdraw, nonce, expiry, account_index, api_key_index });
    }
}
// ts: UpdateApiKeyTxReq
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateApiKeyTxReq {
    pub api_key_index: f64,
    pub new_public_key: String,
    pub permissions: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "UpdateApiKeyTxReq", rename_all = "camelCase")]
struct UpdateApiKeyTxReqWire {
    api_key_index: f64,
    new_public_key: String,
    permissions: f64,
    #[serde(default, deserialize_with = "present")]
    nonce: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    expiry: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
}
impl<'de> Deserialize<'de> for UpdateApiKeyTxReq {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                UpdateApiKeyTxReqWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for UpdateApiKeyTxReq {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { api_key_index, new_public_key, permissions, nonce, expiry, account_index });
    }
}
// ts: TxSignature
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TxSignature {
    pub r: String,
    pub s: String,
    pub v: f64,
}
#[derive(Deserialize)]
#[serde(remote = "TxSignature", rename_all = "camelCase")]
struct TxSignatureWire {
    r: String,
    s: String,
    v: f64,
}
impl<'de> Deserialize<'de> for TxSignature {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                TxSignatureWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for TxSignature {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { r, s, v });
    }
}

/// Numeric literal for L2CreateOrderTxInfo.txType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2CreateOrderTxType {
    Value,
}
impl Serialize for L2CreateOrderTxType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Value => s.serialize_u8(14),
        }
    }
}
impl<'de> Deserialize<'de> for L2CreateOrderTxType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if f64::deserialize(d)? == 14.0 {
            Ok(Self::Value)
        } else {
            Err(serde::de::Error::custom(
                "expected L2CreateOrderTxInfo.txType literal 14",
            ))
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2CreateOrderTxType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Value });
    }
}
// ts: L2CreateOrderTxInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct L2CreateOrderTxInfo {
    pub tx_type: L2CreateOrderTxType,
    pub market_id: String,
    pub order_type: OrderType,
    pub side: OrderSide,
    pub price: String,
    pub quantity: String,
    pub time_in_force: TimeInForce,
    pub reduce_only: bool,
    pub post_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    pub opts: TransactOpts,
}
#[derive(Deserialize)]
#[serde(remote = "L2CreateOrderTxInfo", rename_all = "camelCase")]
struct L2CreateOrderTxInfoWire {
    tx_type: L2CreateOrderTxType,
    market_id: String,
    order_type: OrderType,
    side: OrderSide,
    price: String,
    quantity: String,
    time_in_force: TimeInForce,
    reduce_only: bool,
    post_only: bool,
    #[serde(default, deserialize_with = "present")]
    stop_price: Option<String>,
    #[serde(default, deserialize_with = "present")]
    client_order_id: Option<String>,
    opts: TransactOpts,
}
impl<'de> Deserialize<'de> for L2CreateOrderTxInfo {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                L2CreateOrderTxInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2CreateOrderTxInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx_type, market_id, order_type, side, price, quantity, time_in_force, reduce_only, post_only, stop_price, client_order_id, opts });
    }
}
/// Numeric literal for L2CancelOrderTxInfo.txType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2CancelOrderTxType {
    Value,
}
impl Serialize for L2CancelOrderTxType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Value => s.serialize_u8(15),
        }
    }
}
impl<'de> Deserialize<'de> for L2CancelOrderTxType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if f64::deserialize(d)? == 15.0 {
            Ok(Self::Value)
        } else {
            Err(serde::de::Error::custom(
                "expected L2CancelOrderTxInfo.txType literal 15",
            ))
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2CancelOrderTxType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Value });
    }
}
// ts: L2CancelOrderTxInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct L2CancelOrderTxInfo {
    pub tx_type: L2CancelOrderTxType,
    pub market_id: String,
    pub order_id: String,
    pub opts: TransactOpts,
}
#[derive(Deserialize)]
#[serde(remote = "L2CancelOrderTxInfo", rename_all = "camelCase")]
struct L2CancelOrderTxInfoWire {
    tx_type: L2CancelOrderTxType,
    market_id: String,
    order_id: String,
    opts: TransactOpts,
}
impl<'de> Deserialize<'de> for L2CancelOrderTxInfo {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                L2CancelOrderTxInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2CancelOrderTxInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx_type, market_id, order_id, opts });
    }
}
/// Numeric literal for L2TransferTxInfo.txType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2TransferTxType {
    Value,
}
impl Serialize for L2TransferTxType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Value => s.serialize_u8(16),
        }
    }
}
impl<'de> Deserialize<'de> for L2TransferTxType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if f64::deserialize(d)? == 16.0 {
            Ok(Self::Value)
        } else {
            Err(serde::de::Error::custom(
                "expected L2TransferTxInfo.txType literal 16",
            ))
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2TransferTxType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Value });
    }
}
// ts: L2TransferTxInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct L2TransferTxInfo {
    pub tx_type: L2TransferTxType,
    pub token_address: String,
    pub recipient: String,
    pub amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    pub opts: TransactOpts,
}
#[derive(Deserialize)]
#[serde(remote = "L2TransferTxInfo", rename_all = "camelCase")]
struct L2TransferTxInfoWire {
    tx_type: L2TransferTxType,
    token_address: String,
    recipient: String,
    amount: String,
    #[serde(default, deserialize_with = "present")]
    memo: Option<String>,
    opts: TransactOpts,
}
impl<'de> Deserialize<'de> for L2TransferTxInfo {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                L2TransferTxInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2TransferTxInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx_type, token_address, recipient, amount, memo, opts });
    }
}
/// Numeric literal for L2WithdrawTxInfo.txType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2WithdrawTxType {
    Value,
}
impl Serialize for L2WithdrawTxType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Value => s.serialize_u8(17),
        }
    }
}
impl<'de> Deserialize<'de> for L2WithdrawTxType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if f64::deserialize(d)? == 17.0 {
            Ok(Self::Value)
        } else {
            Err(serde::de::Error::custom(
                "expected L2WithdrawTxInfo.txType literal 17",
            ))
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2WithdrawTxType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Value });
    }
}
// ts: L2WithdrawTxInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct L2WithdrawTxInfo {
    pub tx_type: L2WithdrawTxType,
    pub token_address: String,
    pub amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l1_recipient: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_withdraw: Option<bool>,
    pub opts: TransactOpts,
}
#[derive(Deserialize)]
#[serde(remote = "L2WithdrawTxInfo", rename_all = "camelCase")]
struct L2WithdrawTxInfoWire {
    tx_type: L2WithdrawTxType,
    token_address: String,
    amount: String,
    #[serde(default, deserialize_with = "present")]
    l1_recipient: Option<String>,
    #[serde(default, deserialize_with = "present")]
    fast_withdraw: Option<bool>,
    opts: TransactOpts,
}
impl<'de> Deserialize<'de> for L2WithdrawTxInfo {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                L2WithdrawTxInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2WithdrawTxInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx_type, token_address, amount, l1_recipient, fast_withdraw, opts });
    }
}
/// Numeric literal for L2UpdateApiKeyTxInfo.txType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2UpdateApiKeyTxType {
    Value,
}
impl Serialize for L2UpdateApiKeyTxType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Value => s.serialize_u8(18),
        }
    }
}
impl<'de> Deserialize<'de> for L2UpdateApiKeyTxType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if f64::deserialize(d)? == 18.0 {
            Ok(Self::Value)
        } else {
            Err(serde::de::Error::custom(
                "expected L2UpdateApiKeyTxInfo.txType literal 18",
            ))
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2UpdateApiKeyTxType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Value });
    }
}
// ts: L2UpdateApiKeyTxInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct L2UpdateApiKeyTxInfo {
    pub tx_type: L2UpdateApiKeyTxType,
    pub api_key_index: f64,
    pub new_public_key: String,
    pub permissions: f64,
    pub opts: TransactOpts,
}
#[derive(Deserialize)]
#[serde(remote = "L2UpdateApiKeyTxInfo", rename_all = "camelCase")]
struct L2UpdateApiKeyTxInfoWire {
    tx_type: L2UpdateApiKeyTxType,
    api_key_index: f64,
    new_public_key: String,
    permissions: f64,
    opts: TransactOpts,
}
impl<'de> Deserialize<'de> for L2UpdateApiKeyTxInfo {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                L2UpdateApiKeyTxInfoWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2UpdateApiKeyTxInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx_type, api_key_index, new_public_key, permissions, opts });
    }
}
/// Numeric literal for L2LiquidationTxInfo.txType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2LiquidationTxType {
    Value,
}
impl Serialize for L2LiquidationTxType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Value => s.serialize_u8(19),
        }
    }
}
impl<'de> Deserialize<'de> for L2LiquidationTxType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if f64::deserialize(d)? == 19.0 {
            Ok(Self::Value)
        } else {
            Err(serde::de::Error::custom(
                "expected L2LiquidationTxInfo.txType literal 19",
            ))
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2LiquidationTxType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Value });
    }
}
// ts: L2LiquidationTxInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct L2LiquidationTxInfo {
    pub tx_type: L2LiquidationTxType,
    pub liquidatee_account: String,
    pub market_id: String,
    pub quantity: String,
    pub opts: TransactOpts,
}
#[derive(Deserialize)]
#[serde(remote = "L2LiquidationTxInfo", rename_all = "camelCase")]
struct L2LiquidationTxInfoWire {
    tx_type: L2LiquidationTxType,
    liquidatee_account: String,
    market_id: String,
    quantity: String,
    opts: TransactOpts,
}
impl<'de> Deserialize<'de> for L2LiquidationTxInfo {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                L2LiquidationTxInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2LiquidationTxInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx_type, liquidatee_account, market_id, quantity, opts });
    }
}
/// Numeric literal for L2DeleverageTxInfo.txType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2DeleverageTxType {
    Value,
}
impl Serialize for L2DeleverageTxType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Value => s.serialize_u8(20),
        }
    }
}
impl<'de> Deserialize<'de> for L2DeleverageTxType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if f64::deserialize(d)? == 20.0 {
            Ok(Self::Value)
        } else {
            Err(serde::de::Error::custom(
                "expected L2DeleverageTxInfo.txType literal 20",
            ))
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2DeleverageTxType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Value });
    }
}
// ts: L2DeleverageTxInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct L2DeleverageTxInfo {
    pub tx_type: L2DeleverageTxType,
    pub deleveragee_account: String,
    pub market_id: String,
    pub quantity: String,
    pub opts: TransactOpts,
}
#[derive(Deserialize)]
#[serde(remote = "L2DeleverageTxInfo", rename_all = "camelCase")]
struct L2DeleverageTxInfoWire {
    tx_type: L2DeleverageTxType,
    deleveragee_account: String,
    market_id: String,
    quantity: String,
    opts: TransactOpts,
}
impl<'de> Deserialize<'de> for L2DeleverageTxInfo {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                L2DeleverageTxInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2DeleverageTxInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx_type, deleveragee_account, market_id, quantity, opts });
    }
}
/// Numeric literal for L2FundingFeeTxInfo.txType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2FundingFeeTxType {
    Value,
}
impl Serialize for L2FundingFeeTxType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Value => s.serialize_u8(21),
        }
    }
}
impl<'de> Deserialize<'de> for L2FundingFeeTxType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if f64::deserialize(d)? == 21.0 {
            Ok(Self::Value)
        } else {
            Err(serde::de::Error::custom(
                "expected L2FundingFeeTxInfo.txType literal 21",
            ))
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2FundingFeeTxType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Value });
    }
}
// ts: L2FundingFeeTxInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct L2FundingFeeTxInfo {
    pub tx_type: L2FundingFeeTxType,
    pub market_id: String,
    pub funding_rate: String,
    pub opts: TransactOpts,
}
#[derive(Deserialize)]
#[serde(remote = "L2FundingFeeTxInfo", rename_all = "camelCase")]
struct L2FundingFeeTxInfoWire {
    tx_type: L2FundingFeeTxType,
    market_id: String,
    funding_rate: String,
    opts: TransactOpts,
}
impl<'de> Deserialize<'de> for L2FundingFeeTxInfo {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                L2FundingFeeTxInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2FundingFeeTxInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx_type, market_id, funding_rate, opts });
    }
}

// ts: L2TxInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum L2TxInfo {
    CreateOrder(L2CreateOrderTxInfo),
    CancelOrder(L2CancelOrderTxInfo),
    Transfer(L2TransferTxInfo),
    Withdraw(L2WithdrawTxInfo),
    UpdateApiKey(L2UpdateApiKeyTxInfo),
    Liquidation(L2LiquidationTxInfo),
    Deleverage(L2DeleverageTxInfo),
    FundingFee(L2FundingFeeTxInfo),
}
#[derive(Deserialize)]
#[serde(remote = "L2TxInfo", untagged)]
enum L2TxInfoWire {
    CreateOrder(L2CreateOrderTxInfo),
    CancelOrder(L2CancelOrderTxInfo),
    Transfer(L2TransferTxInfo),
    Withdraw(L2WithdrawTxInfo),
    UpdateApiKey(L2UpdateApiKeyTxInfo),
    Liquidation(L2LiquidationTxInfo),
    Deleverage(L2DeleverageTxInfo),
    FundingFee(L2FundingFeeTxInfo),
}
impl<'de> Deserialize<'de> for L2TxInfo {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(L2TxInfoWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for L2TxInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { CreateOrder(value), CancelOrder(value), Transfer(value), Withdraw(value), UpdateApiKey(value), Liquidation(value), Deleverage(value), FundingFee(value) });
    }
}

// ts: SignedTx
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SignedTx {
    pub tx: L2TxInfo,
    pub signature: TxSignature,
    pub signer: String,
}
#[derive(Deserialize)]
#[serde(remote = "SignedTx")]
struct SignedTxWire {
    tx: L2TxInfo,
    signature: TxSignature,
    signer: String,
}
impl<'de> Deserialize<'de> for SignedTx {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                SignedTxWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for SignedTx {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx, signature, signer });
    }
}

// ts: TxReceipt
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TxReceipt {
    pub tx_hash: String,
    pub tx_type: crate::sdk_protocol_types::TxTypeValue,
    pub status: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_number: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_timestamp: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<safe_cex_num::numeric_ingress::JsNumberInput>,
}
#[derive(Deserialize)]
#[serde(remote = "TxReceipt", rename_all = "camelCase")]
struct TxReceiptWire {
    tx_hash: String,
    tx_type: crate::sdk_protocol_types::TxTypeValue,
    status: f64,
    #[serde(default, deserialize_with = "present")]
    block_number: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    block_timestamp: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    gas_used: Option<String>,
    #[serde(default, deserialize_with = "present")]
    error: Option<String>,
    #[serde(default, deserialize_with = "present")]
    data: Option<safe_cex_num::numeric_ingress::JsNumberInput>,
}
impl<'de> Deserialize<'de> for TxReceipt {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                TxReceiptWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for TxReceipt {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx_hash, tx_type, status, block_number, block_timestamp, gas_used, error, data });
    }
}
