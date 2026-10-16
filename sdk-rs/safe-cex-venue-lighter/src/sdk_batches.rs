//! SDK batch records retain overlapping union members without selecting a variant.
use crate::sdk_orders::{CreateLimitOrderParams, CreateMarketOrderParams};
use crate::sdk_transactions::{CancelOrderTxReq, CreateOrderTxReq, TransferTxReq, WithdrawTxReq};
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};
use safe_cex_types::object_map::JsObjectMap;
use serde::{Deserialize, Serialize};

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// A source transaction-union value retaining fields from overlapping members.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct BatchTransaction(JsNumberInput);
impl BatchTransaction {
    pub fn retained(&self) -> &JsNumberInput {
        &self.0
    }
}
impl<'de> Deserialize<'de> for BatchTransaction {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(decoder)?;
        if input.clone().decode::<CreateOrderTxReq>().is_ok()
            || input.clone().decode::<CancelOrderTxReq>().is_ok()
            || input.clone().decode::<TransferTxReq>().is_ok()
            || input.clone().decode::<WithdrawTxReq>().is_ok()
        {
            Ok(Self(input))
        } else {
            Err(serde::de::Error::custom(
                "BatchTransaction does not match any source union member",
            ))
        }
    }
}
impl JsNumberOutput for BatchTransaction {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, tuple(value));
    }
}

/// A source order-union value; validation does not select or project a member.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct BatchCreateOrder(JsNumberInput);
impl BatchCreateOrder {
    pub fn retained(&self) -> &JsNumberInput {
        &self.0
    }
}
impl<'de> Deserialize<'de> for BatchCreateOrder {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(decoder)?;
        if input.clone().decode::<CreateLimitOrderParams>().is_ok()
            || input.clone().decode::<CreateMarketOrderParams>().is_ok()
        {
            Ok(Self(input))
        } else {
            Err(serde::de::Error::custom(
                "BatchCreateOrder does not match any source union member",
            ))
        }
    }
}
impl JsNumberOutput for BatchCreateOrder {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, tuple(value));
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchTxReq {
    pub transactions: Vec<BatchTransaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "BatchTxReq", rename_all = "camelCase")]
struct BatchTxReqWire {
    transactions: Vec<BatchTransaction>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
}
impl<'de> Deserialize<'de> for BatchTxReq {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                BatchTxReqWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BatchTxReq {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { transactions, account_index, api_key_index });
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchCreateOrdersParams {
    pub orders: Vec<BatchCreateOrder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_index: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "BatchCreateOrdersParams", rename_all = "camelCase")]
struct BatchCreateOrdersParamsWire {
    orders: Vec<BatchCreateOrder>,
    #[serde(default, deserialize_with = "present")]
    account_index: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    api_key_index: Option<f64>,
}
impl<'de> Deserialize<'de> for BatchCreateOrdersParams {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                BatchCreateOrdersParamsWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BatchCreateOrdersParams {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { orders, account_index, api_key_index });
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchTxResponse {
    pub tx_hashes: Vec<String>,
    pub success_indices: Vec<f64>,
    pub failed_indices: Vec<f64>,
    pub errors: JsObjectMap<String>,
}
#[derive(Deserialize)]
#[serde(remote = "BatchTxResponse", rename_all = "camelCase")]
struct BatchTxResponseWire {
    tx_hashes: Vec<String>,
    success_indices: Vec<f64>,
    failed_indices: Vec<f64>,
    errors: JsObjectMap<String>,
}
impl<'de> Deserialize<'de> for BatchTxResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                BatchTxResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BatchTxResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx_hashes, success_indices, failed_indices, errors });
    }
}
