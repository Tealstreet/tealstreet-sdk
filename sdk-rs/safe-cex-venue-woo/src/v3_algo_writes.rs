//! Inert WOO V3 algo write contracts. No transport or live exchange route is enabled.
use crate::v3_contracts::{WooV3Endpoint, WooV3HttpMethod};
use crate::v3_private::WooV3AccountPositionMode;
use crate::v3_private_orders::{WooV3AlgoOrder, WooV3OrderSide};
use crate::v3_request::WooV3RequestInput;
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};
use safe_cex_types::object_map::JsObjectMap;
use serde::{Deserialize, Serialize};
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
fn literal<'de, D: serde::Deserializer<'de>>(d: D, expected: &str) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if s == expected {
        Ok(s)
    } else {
        Err(serde::de::Error::custom(format!("expected {expected}")))
    }
}
fn stop_family<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    literal(d, "STOP_MARKET")
}
fn trailing_family<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    literal(d, "TRAILING_STOP")
}
fn positional_family<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    literal(d, "POSITIONAL_TP_SL")
}
fn callback_source<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    literal(d, "WOO_NATIVE_CALLBACK_VALUE")
}
fn identity_evidence<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    literal(d, "request-target-only-response-does-not-echo-id-or-status")
}
fn all_scope<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    literal(d, "all-algo-orders-all-symbols")
}
fn accepted<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let value = bool::deserialize(d)?;
    if value {
        Ok(true)
    } else {
        Err(serde::de::Error::custom("accepted must be true"))
    }
}
// ts: WooV3AlgoTriggerPriceType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3AlgoTriggerPriceType {
    #[serde(rename = "MARKET_PRICE")]
    MarketPrice,
    #[serde(rename = "MARK_PRICE")]
    MarkPrice,
    #[serde(rename = "INDEX_PRICE")]
    IndexPrice,
    #[serde(rename = "LAST_PRICE")]
    LastPrice,
}

// Remote variants must retain the public enum names.
#[allow(clippy::enum_variant_names)]
#[derive(Deserialize)]
#[serde(remote = "WooV3AlgoTriggerPriceType")]
enum AcceptedWooV3AlgoTriggerPriceTypeWire {
    #[serde(rename = "MARKET_PRICE")]
    MarketPrice,
    #[serde(rename = "MARK_PRICE")]
    MarkPrice,
    #[serde(rename = "INDEX_PRICE")]
    IndexPrice,
    #[serde(rename = "LAST_PRICE")]
    LastPrice,
}
impl<'de> Deserialize<'de> for WooV3AlgoTriggerPriceType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3AlgoTriggerPriceTypeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl JsNumberOutput for WooV3AlgoTriggerPriceType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{MarketPrice,MarkPrice,IndexPrice,LastPrice});
    }
}
// Supporting exact anonymous string union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3AlgoPlaceFamily {
    #[serde(rename = "STOP_MARKET")]
    StopMarket,
    #[serde(rename = "TRAILING_STOP")]
    TrailingStop,
    #[serde(rename = "POSITIONAL_TP_SL")]
    PositionalTpSl,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3AlgoPlaceFamily")]
enum AcceptedWooV3AlgoPlaceFamilyWire {
    #[serde(rename = "STOP_MARKET")]
    StopMarket,
    #[serde(rename = "TRAILING_STOP")]
    TrailingStop,
    #[serde(rename = "POSITIONAL_TP_SL")]
    PositionalTpSl,
}
impl<'de> Deserialize<'de> for WooV3AlgoPlaceFamily {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3AlgoPlaceFamilyWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl JsNumberOutput for WooV3AlgoPlaceFamily {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{StopMarket,TrailingStop,PositionalTpSl});
    }
}
// Supporting exact anonymous string union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3AlgoPlaceType {
    #[serde(rename = "STOP")]
    Stop,
    #[serde(rename = "TRAILING_STOP")]
    TrailingStop,
    #[serde(rename = "POSITIONAL_TP_SL")]
    PositionalTpSl,
    #[serde(rename = "TAKE_PROFIT")]
    TakeProfit,
    #[serde(rename = "STOP_LOSS")]
    StopLoss,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3AlgoPlaceType")]
enum AcceptedWooV3AlgoPlaceTypeWire {
    #[serde(rename = "STOP")]
    Stop,
    #[serde(rename = "TRAILING_STOP")]
    TrailingStop,
    #[serde(rename = "POSITIONAL_TP_SL")]
    PositionalTpSl,
    #[serde(rename = "TAKE_PROFIT")]
    TakeProfit,
    #[serde(rename = "STOP_LOSS")]
    StopLoss,
}
impl<'de> Deserialize<'de> for WooV3AlgoPlaceType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3AlgoPlaceTypeWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}
impl JsNumberOutput for WooV3AlgoPlaceType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Stop,TrailingStop,PositionalTpSl,TakeProfit,StopLoss});
    }
}
// ts: WooV3StopMarketPlaceInput
#[derive(Debug, Clone)]
pub struct WooV3StopMarketPlaceInput {
    pub symbol: String,
    pub side: WooV3OrderSide,
    pub position_mode: WooV3AccountPositionMode,
    pub client_algo_order_id: Option<f64>,
    pub algo_order_tag: Option<String>,
    pub family: String,
    pub quantity: String,
    pub trigger_price: String,
    pub trigger_price_type: WooV3AlgoTriggerPriceType,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3StopMarketPlaceInput")]
struct WooV3StopMarketPlaceInputFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "side")]
    side: WooV3OrderSide,
    #[serde(rename = "positionMode")]
    position_mode: WooV3AccountPositionMode,
    #[serde(
        rename = "clientAlgoOrderId",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    client_algo_order_id: Option<f64>,
    #[serde(
        rename = "algoOrderTag",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    algo_order_tag: Option<String>,
    #[serde(rename = "family", deserialize_with = "stop_family")]
    family: String,
    #[serde(rename = "quantity")]
    quantity: String,
    #[serde(rename = "triggerPrice")]
    trigger_price: String,
    #[serde(rename = "triggerPriceType")]
    trigger_price_type: WooV3AlgoTriggerPriceType,
}
impl Serialize for WooV3StopMarketPlaceInput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3StopMarketPlaceInputFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3StopMarketPlaceInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3StopMarketPlaceInputFields::deserialize(safe_cex_types::strict_decode::object(
                    d,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3StopMarketPlaceInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{symbol,side,position_mode,client_algo_order_id,algo_order_tag,family,quantity,trigger_price,trigger_price_type});
    }
}
// Supporting anonymous record.
#[derive(Debug, Clone)]
pub struct WooV3NativeCallback {
    pub source: String,
    pub value: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3NativeCallback")]
struct WooV3NativeCallbackFields {
    #[serde(rename = "source", deserialize_with = "callback_source")]
    source: String,
    #[serde(rename = "value")]
    value: String,
}
impl Serialize for WooV3NativeCallback {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3NativeCallbackFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3NativeCallback {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3NativeCallbackFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3NativeCallback {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{source,value});
    }
}
// ts: WooV3TrailingPlaceInput
#[derive(Debug, Clone)]
pub struct WooV3TrailingPlaceInput {
    pub symbol: String,
    pub side: WooV3OrderSide,
    pub position_mode: WooV3AccountPositionMode,
    pub client_algo_order_id: Option<f64>,
    pub algo_order_tag: Option<String>,
    pub family: String,
    pub quantity: String,
    pub callback: WooV3NativeCallback,
    pub activated_price: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3TrailingPlaceInput")]
struct WooV3TrailingPlaceInputFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "side")]
    side: WooV3OrderSide,
    #[serde(rename = "positionMode")]
    position_mode: WooV3AccountPositionMode,
    #[serde(
        rename = "clientAlgoOrderId",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    client_algo_order_id: Option<f64>,
    #[serde(
        rename = "algoOrderTag",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    algo_order_tag: Option<String>,
    #[serde(rename = "family", deserialize_with = "trailing_family")]
    family: String,
    #[serde(rename = "quantity")]
    quantity: String,
    #[serde(rename = "callback")]
    callback: WooV3NativeCallback,
    #[serde(
        rename = "activatedPrice",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    activated_price: Option<String>,
}
impl Serialize for WooV3TrailingPlaceInput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3TrailingPlaceInputFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3TrailingPlaceInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3TrailingPlaceInputFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3TrailingPlaceInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{symbol,side,position_mode,client_algo_order_id,algo_order_tag,family,quantity,callback,activated_price});
    }
}
// ts: WooV3PositionalTriggerInput
#[derive(Debug, Clone)]
pub struct WooV3PositionalTriggerInput {
    pub trigger_price: String,
    pub trigger_price_type: WooV3AlgoTriggerPriceType,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PositionalTriggerInput")]
struct WooV3PositionalTriggerInputFields {
    #[serde(rename = "triggerPrice")]
    trigger_price: String,
    #[serde(rename = "triggerPriceType")]
    trigger_price_type: WooV3AlgoTriggerPriceType,
}
impl Serialize for WooV3PositionalTriggerInput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3PositionalTriggerInputFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3PositionalTriggerInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3PositionalTriggerInputFields::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3PositionalTriggerInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{trigger_price,trigger_price_type});
    }
}
// ts: WooV3PositionalPlaceInput
#[derive(Debug, Clone)]
pub struct WooV3PositionalPlaceInput {
    pub symbol: String,
    pub side: WooV3OrderSide,
    pub position_mode: WooV3AccountPositionMode,
    pub client_algo_order_id: Option<f64>,
    pub algo_order_tag: Option<String>,
    pub family: String,
    pub stop_loss: Option<WooV3PositionalTriggerInput>,
    pub take_profit: Option<WooV3PositionalTriggerInput>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PositionalPlaceInput")]
struct WooV3PositionalPlaceInputFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "side")]
    side: WooV3OrderSide,
    #[serde(rename = "positionMode")]
    position_mode: WooV3AccountPositionMode,
    #[serde(
        rename = "clientAlgoOrderId",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    client_algo_order_id: Option<f64>,
    #[serde(
        rename = "algoOrderTag",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    algo_order_tag: Option<String>,
    #[serde(rename = "family", deserialize_with = "positional_family")]
    family: String,
    #[serde(
        rename = "stopLoss",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    stop_loss: Option<WooV3PositionalTriggerInput>,
    #[serde(
        rename = "takeProfit",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    take_profit: Option<WooV3PositionalTriggerInput>,
}
impl Serialize for WooV3PositionalPlaceInput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3PositionalPlaceInputFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3PositionalPlaceInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3PositionalPlaceInputFields::deserialize(safe_cex_types::strict_decode::object(
                    d,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3PositionalPlaceInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{symbol,side,position_mode,client_algo_order_id,algo_order_tag,family,stop_loss,take_profit});
    }
}
// ts: WooV3AlgoPlaceResponseRow
#[derive(Debug, Clone)]
pub struct WooV3AlgoPlaceResponseRow {
    pub algo_order_id: f64,
    pub client_algo_order_id: f64,
    pub algo_type: WooV3AlgoPlaceType,
    pub quantity: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AlgoPlaceResponseRow")]
struct WooV3AlgoPlaceResponseRowFields {
    #[serde(rename = "algoOrderId")]
    algo_order_id: f64,
    #[serde(rename = "clientAlgoOrderId")]
    client_algo_order_id: f64,
    #[serde(rename = "algoType")]
    algo_type: WooV3AlgoPlaceType,
    #[serde(rename = "quantity")]
    quantity: String,
}
impl Serialize for WooV3AlgoPlaceResponseRow {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AlgoPlaceResponseRowFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AlgoPlaceResponseRow {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AlgoPlaceResponseRowFields::deserialize(safe_cex_types::strict_decode::object(
                    d,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3AlgoPlaceResponseRow {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{algo_order_id,client_algo_order_id,algo_type,quantity});
    }
}
// Supporting anonymous record.
#[derive(Debug, Clone)]
pub struct WooV3AlgoAmendUpdate {
    pub trigger_price: Option<String>,
    pub trigger_price_type: Option<WooV3AlgoTriggerPriceType>,
    pub quantity: Option<String>,
    pub callback_value: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AlgoAmendUpdate")]
struct WooV3AlgoAmendUpdateFields {
    #[serde(
        rename = "triggerPrice",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    trigger_price: Option<String>,
    #[serde(
        rename = "triggerPriceType",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    trigger_price_type: Option<WooV3AlgoTriggerPriceType>,
    #[serde(
        rename = "quantity",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    quantity: Option<String>,
    #[serde(
        rename = "callbackValue",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    callback_value: Option<String>,
}
impl Serialize for WooV3AlgoAmendUpdate {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AlgoAmendUpdateFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AlgoAmendUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AlgoAmendUpdateFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3AlgoAmendUpdate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{trigger_price,trigger_price_type,quantity,callback_value});
    }
}
// ts: WooV3AlgoAmendInput
#[derive(Debug, Clone)]
pub struct WooV3AlgoAmendInput {
    pub current_order: WooV3AlgoOrder,
    pub target: WooV3AlgoWriteTarget,
    pub update: WooV3AlgoAmendUpdate,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AlgoAmendInput")]
struct WooV3AlgoAmendInputFields {
    #[serde(rename = "currentOrder")]
    current_order: WooV3AlgoOrder,
    #[serde(rename = "target")]
    target: WooV3AlgoWriteTarget,
    #[serde(rename = "update")]
    update: WooV3AlgoAmendUpdate,
}
impl Serialize for WooV3AlgoAmendInput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AlgoAmendInputFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AlgoAmendInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AlgoAmendInputFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3AlgoAmendInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{current_order,target,update});
    }
}
// ts: WooV3AlgoMutationResult
#[derive(Debug, Clone)]
pub struct WooV3AlgoMutationResult {
    pub accepted: bool,
    pub target: WooV3AlgoWriteTarget,
    pub timestamp: f64,
    pub identity_evidence: String,
    pub gateway_data: JsObjectMap<JsNumberInput>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AlgoMutationResult")]
struct WooV3AlgoMutationResultFields {
    #[serde(rename = "accepted", deserialize_with = "accepted")]
    accepted: bool,
    #[serde(rename = "target")]
    target: WooV3AlgoWriteTarget,
    #[serde(rename = "timestamp")]
    timestamp: f64,
    #[serde(rename = "identityEvidence", deserialize_with = "identity_evidence")]
    identity_evidence: String,
    #[serde(rename = "gatewayData")]
    gateway_data: JsObjectMap<JsNumberInput>,
}
impl Serialize for WooV3AlgoMutationResult {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AlgoMutationResultFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AlgoMutationResult {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AlgoMutationResultFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3AlgoMutationResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{accepted,target,timestamp,identity_evidence,gateway_data});
    }
}
// ts: WooV3AlgoCancelPendingResult
#[derive(Debug, Clone)]
pub struct WooV3AlgoCancelPendingResult {
    pub accepted: bool,
    pub scope: String,
    pub timestamp: f64,
    pub gateway_data: JsObjectMap<JsNumberInput>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AlgoCancelPendingResult")]
struct WooV3AlgoCancelPendingResultFields {
    #[serde(rename = "accepted", deserialize_with = "accepted")]
    accepted: bool,
    #[serde(rename = "scope", deserialize_with = "all_scope")]
    scope: String,
    #[serde(rename = "timestamp")]
    timestamp: f64,
    #[serde(rename = "gatewayData")]
    gateway_data: JsObjectMap<JsNumberInput>,
}
impl Serialize for WooV3AlgoCancelPendingResult {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AlgoCancelPendingResultFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AlgoCancelPendingResult {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AlgoCancelPendingResultFields::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3AlgoCancelPendingResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{accepted,scope,timestamp,gateway_data});
    }
}
// ts: WooV3AlgoWriteTarget
// An exclusive owned ID sum; the dynamic validator below still rejects both/missing IDs in source order.
#[derive(Debug, Clone, PartialEq)]
pub enum WooV3AlgoWriteTarget {
    AlgoOrderId(f64),
    ClientAlgoOrderId(f64),
}
impl<'de> Deserialize<'de> for WooV3AlgoWriteTarget {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(d)?;
        let row = input
            .decode::<JsObjectMap<JsNumberInput>>()
            .map_err(serde::de::Error::custom)?;
        let parse = |key: &str| {
            row.get(key)
                .unwrap()
                .clone()
                .decode::<f64>()
                .map_err(serde::de::Error::custom)
        };
        match (
            row.contains_key("algoOrderId"),
            row.contains_key("clientAlgoOrderId"),
        ) {
            (true, false) => Ok(Self::AlgoOrderId(parse("algoOrderId")?)),
            (false, true) => Ok(Self::ClientAlgoOrderId(parse("clientAlgoOrderId")?)),
            _ => Err(serde::de::Error::custom("target requires exactly one ID")),
        }
    }
}
impl Serialize for WooV3AlgoWriteTarget {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = s.serialize_map(Some(1))?;
        match self {
            Self::AlgoOrderId(v) => map.serialize_entry("algoOrderId", v)?,
            Self::ClientAlgoOrderId(v) => map.serialize_entry("clientAlgoOrderId", v)?,
        };
        map.end()
    }
}
impl JsNumberOutput for WooV3AlgoWriteTarget {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{AlgoOrderId(id),ClientAlgoOrderId(id)});
    }
}
// ts: WooV3AlgoPlaceInput
#[derive(Debug, Clone)]
pub enum WooV3AlgoPlaceInput {
    StopMarket(WooV3StopMarketPlaceInput),
    TrailingStop(WooV3TrailingPlaceInput),
    PositionalTpSl(WooV3PositionalPlaceInput),
}
impl<'de> Deserialize<'de> for WooV3AlgoPlaceInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(d)?;
        let row = input
            .clone()
            .decode::<JsObjectMap<JsNumberInput>>()
            .map_err(serde::de::Error::custom)?;
        let family = row
            .get("family")
            .and_then(|v| v.clone().decode::<String>().ok())
            .ok_or_else(|| serde::de::Error::custom("family must be a string"))?;
        match family.as_str() {
            "STOP_MARKET" => input.decode().map(Self::StopMarket),
            "TRAILING_STOP" => input.decode().map(Self::TrailingStop),
            "POSITIONAL_TP_SL" => input.decode().map(Self::PositionalTpSl),
            _ => return Err(serde::de::Error::custom("unsupported family")),
        }
        .map_err(serde::de::Error::custom)
    }
}
impl Serialize for WooV3AlgoPlaceInput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::StopMarket(v) => v.serialize(s),
            Self::TrailingStop(v) => v.serialize(s),
            Self::PositionalTpSl(v) => v.serialize(s),
        }
    }
}
impl JsNumberOutput for WooV3AlgoPlaceInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{StopMarket(v),TrailingStop(v),PositionalTpSl(v)});
    }
}
// ts: WooV3AlgoPlaceBuildResult
#[derive(Debug, Clone)]
pub enum WooV3AlgoPlaceBuildResult {
    Ready {
        request: WooV3RequestInput,
        family: WooV3AlgoPlaceFamily,
        trigger_semantics: &'static str,
    },
    Blocked {
        blocker: &'static str,
        family: WooV3AlgoPlaceFamily,
    },
}
// ts: WooV3AlgoPlaceDecodeResult
#[derive(Debug, Clone)]
pub enum WooV3AlgoPlaceDecodeResult {
    Ready {
        row: WooV3AlgoPlaceResponseRow,
        timestamp: f64,
        identity_evidence: &'static str,
    },
    Blocked {
        blocker: &'static str,
        rows: Vec<WooV3AlgoPlaceResponseRow>,
        timestamp: f64,
    },
}
// ts: WooV3AlgoAmendBuildResult
#[derive(Debug, Clone)]
pub enum WooV3AlgoAmendBuildResult {
    Ready {
        request: WooV3RequestInput,
        target: WooV3AlgoWriteTarget,
        update_semantics: &'static str,
    },
    Blocked {
        blocker: &'static str,
        target: WooV3AlgoWriteTarget,
    },
}
// ts: WooV3AlgoCancelPendingBuildResult
#[derive(Debug, Clone)]
pub enum WooV3AlgoCancelPendingBuildResult {
    Ready {
        request: WooV3RequestInput,
        scope: &'static str,
    },
    Blocked {
        blocker: &'static str,
        symbol: String,
    },
}
// ts: WooV3AlgoWriteResponseError
#[derive(Debug, Clone)]
pub struct WooV3AlgoWriteResponseError {
    pub name: String,
    pub message: String,
    pub code: Option<f64>,
    pub gateway_response: JsObjectMap<JsNumberInput>,
    pub contract_status: &'static str,
}
impl WooV3AlgoWriteResponseError {
    // ts: constructor
    pub fn new(
        message: String,
        gateway_response: &JsObjectMap<JsNumberInput>,
        code: Option<f64>,
    ) -> Self {
        Self {
            name: "WooV3AlgoWriteResponseError".into(),
            message,
            code,
            gateway_response: gateway_response.clone(),
            contract_status: "vendor-error-envelope-undocumented",
        }
    }
}
// Supporting concrete exception transport.
#[derive(Debug, Clone)]
pub enum WooV3AlgoWriteError {
    Validation(String),
    Gateway(WooV3AlgoWriteResponseError),
}
impl From<String> for WooV3AlgoWriteError {
    fn from(s: String) -> Self {
        Self::Validation(s)
    }
}
impl From<&str> for WooV3AlgoWriteError {
    fn from(s: &str) -> Self {
        Self::Validation(s.into())
    }
}
// Supporting immutable metadata value representation (copies are owned).
#[derive(Debug, Clone, Copy)]
pub struct WooV3AlgoWriteParity {
    pub request_and_response_fixtures: bool,
    pub runtime_imported: bool,
    pub credentialed_demo_lifecycle: bool,
    pub trigger_price_type: &'static str,
    pub trailing_input: &'static str,
    pub positional_response_identity: &'static str,
    pub positional_update: &'static str,
    pub partial_fill_and_quantity_amend: &'static str,
    pub symbol_pending_cancel: &'static str,
    pub cancel_response_identity: &'static str,
}
// ts: WOO_V3_ALGO_WRITE_PARITY
pub const WOO_V3_ALGO_WRITE_PARITY: WooV3AlgoWriteParity = WooV3AlgoWriteParity {
    request_and_response_fixtures: true,
    runtime_imported: false,
    credentialed_demo_lifecycle: false,
    trigger_price_type: "MARKET_PRICE-only",
    trailing_input: "vendor-native-callbackValue-no-safe-cex-conversion",
    positional_response_identity: "blocked-pending-demo-evidence",
    positional_update: "blocked-official-edit-schema-has-no-childOrders",
    partial_fill_and_quantity_amend: "blocked-pending-demo-evidence",
    symbol_pending_cancel: "blocked-published-query-schema-conflicts-with-prose",
    cancel_response_identity: "request-target-only",
};
// ts: requireRecord
pub fn require_record(
    v: Option<&JsNumberInput>,
    path: &str,
) -> Result<JsObjectMap<JsNumberInput>, String> {
    crate::v3_regular_writes::require_record(v, path)
}
// ts: requireArray
pub fn require_array(v: Option<&JsNumberInput>, path: &str) -> Result<Vec<JsNumberInput>, String> {
    v.and_then(|v| v.clone().decode().ok())
        .ok_or_else(|| format!("{path} must be an array"))
}
// ts: requireBoolean
pub fn require_boolean(v: Option<&JsNumberInput>, path: &str) -> Result<bool, String> {
    crate::v3_regular_writes::require_boolean(v, path)
}
// ts: requireInteger
pub fn require_integer(v: Option<&JsNumberInput>, path: &str, minimum: f64) -> Result<f64, String> {
    crate::v3_regular_writes::require_integer(v, path, minimum)
}
// ts: requireString
pub fn require_string(v: Option<&JsNumberInput>, path: &str) -> Result<String, String> {
    crate::v3_regular_writes::require_string(v, path)
}
// ts: requireEnum
pub fn require_enum(
    v: Option<&JsNumberInput>,
    values: &[&str],
    path: &str,
) -> Result<String, String> {
    crate::v3_regular_writes::require_enum(v, values, path)
}
// ts: requireDecimal
pub fn require_decimal(
    v: Option<&JsNumberInput>,
    path: &str,
    allow_zero: bool,
) -> Result<String, String> {
    crate::v3_regular_writes::require_decimal(v, path, allow_zero)
}
// ts: decimalParts
pub fn decimal_parts(v: &str) -> (String, String) {
    crate::v3_regular_writes::decimal_parts(v)
}
// ts: compareDecimals
pub fn compare_decimals(a: &str, b: &str) -> i32 {
    crate::v3_regular_writes::compare_decimals(a, b)
}
// ts: requirePerpetualSymbol
pub fn require_perpetual_symbol(v: &str) -> Result<String, String> {
    crate::v3_regular_writes::require_perpetual_symbol(v)
}
fn symbol_input(v: Option<&JsNumberInput>) -> Result<String, String> {
    let s = v
        .and_then(|v| v.clone().decode::<String>().ok())
        .ok_or("WOO V3 symbol must use PERP_BASE_QUOTE form")?;
    require_perpetual_symbol(&s)
}
fn capture<T: Serialize + JsNumberOutput>(v: &T) -> JsNumberInput {
    v.check_number_output();
    safe_cex_num::numeric_ingress::js_from_str(
        &serde_json::to_string(v).expect("validated finite descriptor field"),
    )
    .expect("captured descriptor field")
}
fn text(v: &str) -> JsNumberInput {
    capture(&v.to_string())
}
fn stored(v: Option<&JsNumberInput>, path: &str) -> Result<String, String> {
    v.and_then(|v| v.clone().decode::<String>().ok())
        .ok_or_else(|| format!("{path} must be a string"))
}
fn value_string(v: Option<&JsNumberInput>) -> Option<String> {
    v.and_then(|v| v.clone().decode::<String>().ok())
}
fn target_map(t: &WooV3AlgoWriteTarget) -> JsObjectMap<JsNumberInput> {
    match t {
        WooV3AlgoWriteTarget::AlgoOrderId(n) => {
            [("algoOrderId".into(), capture(n))].into_iter().collect()
        }
        WooV3AlgoWriteTarget::ClientAlgoOrderId(n) => [("clientAlgoOrderId".into(), capture(n))]
            .into_iter()
            .collect(),
    }
}
fn request(
    method: WooV3HttpMethod,
    path: WooV3Endpoint,
    body: Option<JsObjectMap<JsNumberInput>>,
    query: Option<JsObjectMap<JsNumberInput>>,
) -> WooV3RequestInput {
    WooV3RequestInput {
        method,
        path,
        body,
        query,
    }
}
// ts: validateTarget
pub fn validate_target(v: &JsObjectMap<JsNumberInput>) -> Result<WooV3AlgoWriteTarget, String> {
    let has = v.contains_key("algoOrderId");
    if has == v.contains_key("clientAlgoOrderId") {
        return Err("WOO V3 algo target requires exactly one ID".into());
    }
    if has {
        Ok(WooV3AlgoWriteTarget::AlgoOrderId(require_integer(
            v.get("algoOrderId"),
            "WOO V3 algoOrderId",
            1.0,
        )?))
    } else {
        Ok(WooV3AlgoWriteTarget::ClientAlgoOrderId(require_integer(
            v.get("clientAlgoOrderId"),
            "WOO V3 clientAlgoOrderId",
            1.0,
        )?))
    }
}
// ts: validateCommon
pub fn validate_common(
    v: &JsObjectMap<JsNumberInput>,
) -> Result<JsObjectMap<JsNumberInput>, String> {
    let symbol = symbol_input(v.get("symbol"))?;
    let side = require_enum(v.get("side"), &["BUY", "SELL"], "WOO V3 algo side")?;
    let mode = require_enum(
        v.get("positionMode"),
        &["ONE_WAY", "HEDGE_MODE"],
        "WOO V3 algo positionMode",
    )?;
    if let Some(id) = v.get("clientAlgoOrderId") {
        require_integer(Some(id), "WOO V3 clientAlgoOrderId", 1.0)?;
    }
    if let Some(tag) = v.get("algoOrderTag") {
        let s = tag
            .clone()
            .decode::<String>()
            .map_err(|_| "WOO V3 algoOrderTag must contain 1 to 64 characters")?;
        if !(1..=64).contains(&s.encode_utf16().count()) {
            return Err("WOO V3 algoOrderTag must contain 1 to 64 characters".into());
        }
    }
    let pos = if mode == "ONE_WAY" {
        "BOTH"
    } else if side == "BUY" {
        "SHORT"
    } else {
        "LONG"
    };
    let mut out: JsObjectMap<JsNumberInput> = [
        ("symbol".into(), text(&symbol)),
        ("side".into(), text(&side)),
        ("positionSide".into(), text(pos)),
    ]
    .into_iter()
    .collect();
    for key in ["clientAlgoOrderId", "algoOrderTag"] {
        if let Some(v) = v.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    Ok(out)
}
// ts: hasUnsupportedTrigger
pub fn has_unsupported_trigger(v: WooV3AlgoTriggerPriceType) -> bool {
    v != WooV3AlgoTriggerPriceType::MarketPrice
}
fn unsupported(v: Option<&JsNumberInput>) -> bool {
    value_string(v).as_deref() != Some("MARKET_PRICE")
}
fn family(v: &JsObjectMap<JsNumberInput>) -> Result<WooV3AlgoPlaceFamily, String> {
    match require_enum(
        v.get("family"),
        &["STOP_MARKET", "TRAILING_STOP", "POSITIONAL_TP_SL"],
        "WOO V3 algo family",
    )?
    .as_str()
    {
        "STOP_MARKET" => Ok(WooV3AlgoPlaceFamily::StopMarket),
        "TRAILING_STOP" => Ok(WooV3AlgoPlaceFamily::TrailingStop),
        _ => Ok(WooV3AlgoPlaceFamily::PositionalTpSl),
    }
}
// ts: buildWooV3AlgoPlaceRequest
pub fn build_woo_v3_algo_place_request(
    v: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3AlgoPlaceBuildResult, String> {
    let common = validate_common(v)?;
    let family = family(v)?;
    let blocked = || WooV3AlgoPlaceBuildResult::Blocked {
        blocker: "only-market-price-trigger-is-documented",
        family,
    };
    if family == WooV3AlgoPlaceFamily::StopMarket {
        if unsupported(v.get("triggerPriceType")) {
            return Ok(blocked());
        }
        let mut body = common;
        body.insert("algoType".into(), text("STOP"));
        body.insert(
            "quantity".into(),
            text(&require_decimal(
                v.get("quantity"),
                "WOO V3 stop quantity",
                false,
            )?),
        );
        body.insert("marginMode".into(), text("CROSS"));
        body.insert(
            "triggerPrice".into(),
            text(&require_decimal(
                v.get("triggerPrice"),
                "WOO V3 stop triggerPrice",
                false,
            )?),
        );
        body.insert("triggerPriceType".into(), text("MARKET_PRICE"));
        body.insert("type".into(), text("MARKET"));
        body.insert("reduceOnly".into(), capture(&true));
        return Ok(WooV3AlgoPlaceBuildResult::Ready {
            request: request(
                WooV3HttpMethod::Post,
                WooV3Endpoint::TradeAlgoOrder,
                Some(body),
                None,
            ),
            family,
            trigger_semantics: "official-last-price-trigger",
        });
    }
    if family == WooV3AlgoPlaceFamily::TrailingStop {
        let callback = require_record(v.get("callback"), "WOO V3 trailing callback")?;
        require_enum(
            callback.get("source"),
            &["WOO_NATIVE_CALLBACK_VALUE"],
            "WOO V3 trailing callback source",
        )?;
        let mut body = common;
        body.insert("algoType".into(), text("TRAILING_STOP"));
        body.insert(
            "quantity".into(),
            text(&require_decimal(
                v.get("quantity"),
                "WOO V3 trailing quantity",
                false,
            )?),
        );
        body.insert("marginMode".into(), text("CROSS"));
        body.insert("type".into(), text("MARKET"));
        body.insert(
            "callbackValue".into(),
            text(&require_decimal(
                callback.get("value"),
                "WOO V3 trailing callbackValue",
                false,
            )?),
        );
        if let Some(a) = v.get("activatedPrice") {
            body.insert(
                "activatedPrice".into(),
                text(&require_decimal(
                    Some(a),
                    "WOO V3 trailing activatedPrice",
                    false,
                )?),
            );
        }
        body.insert("reduceOnly".into(), capture(&true));
        return Ok(WooV3AlgoPlaceBuildResult::Ready {
            request: request(
                WooV3HttpMethod::Post,
                WooV3Endpoint::TradeAlgoOrder,
                Some(body),
                None,
            ),
            family,
            trigger_semantics: "vendor-native-callback-value-no-safe-cex-conversion",
        });
    }
    let mut triggers = Vec::new();
    for (key, kind) in [("takeProfit", "TAKE_PROFIT"), ("stopLoss", "STOP_LOSS")] {
        if let Some(t) = v.get(key) {
            triggers.push((
                kind,
                require_record(Some(t), &format!("WOO V3 positional {kind}"))?,
            ));
        }
    }
    if triggers.is_empty() {
        return Err("WOO V3 positional TP/SL requires at least one child".into());
    }
    if triggers
        .iter()
        .any(|(_, t)| unsupported(t.get("triggerPriceType")))
    {
        return Ok(blocked());
    }
    let mut children = Vec::new();
    for (kind, t) in triggers {
        let child: JsObjectMap<JsNumberInput> = [
            ("algoType".into(), text(kind)),
            ("side".into(), common.get("side").unwrap().clone()),
            (
                "triggerPrice".into(),
                text(&require_decimal(
                    t.get("triggerPrice"),
                    &format!("WOO V3 positional {kind} triggerPrice"),
                    false,
                )?),
            ),
            ("type".into(), text("CLOSE_POSITION")),
            ("triggerPriceType".into(), text("MARKET_PRICE")),
        ]
        .into_iter()
        .collect();
        children.push(child);
    }
    let mut body: JsObjectMap<JsNumberInput> = [
        ("symbol".into(), common.get("symbol").unwrap().clone()),
        ("algoType".into(), text("POSITIONAL_TP_SL")),
        ("childOrders".into(), capture(&children)),
        (
            "positionSide".into(),
            common.get("positionSide").unwrap().clone(),
        ),
        ("marginMode".into(), text("CROSS")),
    ]
    .into_iter()
    .collect();
    for key in ["clientAlgoOrderId", "algoOrderTag"] {
        if let Some(v) = v.get(key) {
            body.insert(key.into(), v.clone());
        }
    }
    Ok(WooV3AlgoPlaceBuildResult::Ready {
        request: request(
            WooV3HttpMethod::Post,
            WooV3Endpoint::TradeAlgoOrder,
            Some(body),
            None,
        ),
        family,
        trigger_semantics: "official-last-price-positional-children",
    })
}
// Supporting anonymous envelope result.
#[derive(Debug, Clone)]
pub struct WooV3AlgoSuccessData {
    pub data: JsObjectMap<JsNumberInput>,
    pub timestamp: f64,
}
// ts: decodeSuccessData
pub fn decode_success_data(
    v: Option<&JsNumberInput>,
    name: &str,
) -> Result<WooV3AlgoSuccessData, WooV3AlgoWriteError> {
    let response = require_record(v, &format!("WOO V3 {name} response"))?;
    if !require_boolean(response.get("success"), &format!("WOO V3 {name}.success"))? {
        let code = response
            .get("code")
            .map(|v| require_integer(Some(v), &format!("WOO V3 {name}.code"), 0.0))
            .transpose()?;
        let message = match response.get("message") {
            None => format!("WOO V3 {name} failed with an undocumented error envelope"),
            v => require_string(v, &format!("WOO V3 {name}.message"))?,
        };
        return Err(WooV3AlgoWriteError::Gateway(
            WooV3AlgoWriteResponseError::new(message, &response, code),
        ));
    }
    let data = require_record(response.get("data"), &format!("WOO V3 {name}.data"))?;
    let timestamp = require_integer(
        response.get("timestamp"),
        &format!("WOO V3 {name}.timestamp"),
        0.0,
    )?;
    Ok(WooV3AlgoSuccessData { data, timestamp })
}
fn place_type(s: &str) -> WooV3AlgoPlaceType {
    match s {
        "STOP" => WooV3AlgoPlaceType::Stop,
        "TRAILING_STOP" => WooV3AlgoPlaceType::TrailingStop,
        "POSITIONAL_TP_SL" => WooV3AlgoPlaceType::PositionalTpSl,
        "TAKE_PROFIT" => WooV3AlgoPlaceType::TakeProfit,
        "STOP_LOSS" => WooV3AlgoPlaceType::StopLoss,
        _ => unreachable!("validated algo type"),
    }
}
// ts: decodePlaceRow
pub fn decode_place_row(
    v: Option<&JsNumberInput>,
    path: &str,
) -> Result<WooV3AlgoPlaceResponseRow, String> {
    let row = require_record(v, path)?;
    Ok(WooV3AlgoPlaceResponseRow {
        algo_order_id: require_integer(
            row.get("algoOrderId"),
            &format!("{path}.algoOrderId"),
            1.0,
        )?,
        client_algo_order_id: require_integer(
            row.get("clientAlgoOrderId"),
            &format!("{path}.clientAlgoOrderId"),
            0.0,
        )?,
        algo_type: place_type(&require_enum(
            row.get("algoType"),
            &[
                "STOP",
                "TRAILING_STOP",
                "POSITIONAL_TP_SL",
                "TAKE_PROFIT",
                "STOP_LOSS",
            ],
            &format!("{path}.algoType"),
        )?),
        quantity: require_decimal(row.get("quantity"), &format!("{path}.quantity"), true)?,
    })
}
#[derive(Debug, Clone)]
pub enum WooV3AlgoPlaceResponseError<E> {
    Response(WooV3AlgoWriteError),
    Membership(E),
}
impl<E> From<String> for WooV3AlgoPlaceResponseError<E> {
    fn from(error: String) -> Self {
        Self::Response(error.into())
    }
}
impl<E> From<&str> for WooV3AlgoPlaceResponseError<E> {
    fn from(error: &str) -> Self {
        Self::Response(error.into())
    }
}
impl<E> From<WooV3AlgoWriteError> for WooV3AlgoPlaceResponseError<E> {
    fn from(error: WooV3AlgoWriteError) -> Self {
        Self::Response(error)
    }
}
// ts: decodeWooV3AlgoPlaceResponse
pub fn decode_woo_v3_algo_place_response(
    v: Option<&JsNumberInput>,
    build: &WooV3AlgoPlaceBuildResult,
    known: &[f64],
) -> Result<WooV3AlgoPlaceDecodeResult, WooV3AlgoWriteError> {
    decode_woo_v3_algo_place_response_with(v, build, |id| {
        Ok::<_, std::convert::Infallible>(known.contains(&id))
    })
    .map_err(|error| match error {
        WooV3AlgoPlaceResponseError::Response(error) => error,
        WooV3AlgoPlaceResponseError::Membership(error) => match error {},
    })
}
pub fn decode_woo_v3_algo_place_response_with<E>(
    v: Option<&JsNumberInput>,
    build: &WooV3AlgoPlaceBuildResult,
    mut known_algo_order_ids: impl FnMut(f64) -> Result<bool, E>,
) -> Result<WooV3AlgoPlaceDecodeResult, WooV3AlgoPlaceResponseError<E>> {
    let WooV3AlgoPlaceBuildResult::Ready {
        request: r, family, ..
    } = build
    else {
        return Err("WOO V3 algo place response requires its matching place request".into());
    };
    if r.method != WooV3HttpMethod::Post
        || r.path != WooV3Endpoint::TradeAlgoOrder
        || r.body.is_none()
    {
        return Err("WOO V3 algo place response requires its matching place request".into());
    }
    let WooV3AlgoSuccessData { data, timestamp } = decode_success_data(v, "place algo order")?;
    let raw = require_array(data.get("rows"), "WOO V3 place algo order.data.rows")?;
    let rows = raw
        .iter()
        .enumerate()
        .map(|(i, v)| decode_place_row(Some(v), &format!("WOO V3 place algo order.data.rows[{i}]")))
        .collect::<Result<Vec<_>, _>>()?;
    if rows.is_empty() {
        return Err("WOO V3 place algo order.data.rows must not be empty".into());
    }
    let mut ids = Vec::new();
    for row in &rows {
        if known_algo_order_ids(row.algo_order_id)
            .map_err(WooV3AlgoPlaceResponseError::Membership)?
        {
            return Err("WOO V3 algo place response reused a known algoOrderId".into());
        }
        if ids.contains(&row.algo_order_id) {
            return Err("WOO V3 algo place response repeated an algoOrderId".into());
        }
        ids.push(row.algo_order_id);
    }
    if *family == WooV3AlgoPlaceFamily::PositionalTpSl {
        return Ok(WooV3AlgoPlaceDecodeResult::Blocked {
            blocker: "positional-response-parent-child-identity-requires-demo-evidence",
            rows,
            timestamp,
        });
    }
    if rows.len() != 1 {
        return Err(format!(
            "WOO V3 {} place response must contain exactly one row",
            if *family == WooV3AlgoPlaceFamily::StopMarket {
                "STOP_MARKET"
            } else {
                "TRAILING_STOP"
            }
        )
        .into());
    }
    let row = rows.into_iter().next().unwrap();
    let expected = if *family == WooV3AlgoPlaceFamily::StopMarket {
        WooV3AlgoPlaceType::Stop
    } else {
        WooV3AlgoPlaceType::TrailingStop
    };
    if row.algo_type != expected {
        return Err("WOO V3 algo place response algoType does not match request".into());
    }
    let body = r.body.as_ref().unwrap();
    if value_string(body.get("quantity")).is_none_or(|q| compare_decimals(&row.quantity, &q) != 0) {
        return Err("WOO V3 algo place response quantity does not match request".into());
    }
    if let Some(id) = body.get("clientAlgoOrderId") {
        if id.clone().decode::<f64>().ok() != Some(row.client_algo_order_id) {
            return Err(
                "WOO V3 algo place response clientAlgoOrderId does not match request".into(),
            );
        }
    }
    Ok(WooV3AlgoPlaceDecodeResult::Ready {
        row,
        timestamp,
        identity_evidence: "response-echoed-algo-order-id",
    })
}
// ts: targetMatchesOrder
pub fn target_matches_order(t: &WooV3AlgoWriteTarget, o: &JsObjectMap<JsNumberInput>) -> bool {
    match t {
        WooV3AlgoWriteTarget::AlgoOrderId(id) => {
            o.get("algoOrderId")
                .and_then(|v| v.clone().decode::<f64>().ok())
                == Some(*id)
        }
        WooV3AlgoWriteTarget::ClientAlgoOrderId(id) => {
            o.get("clientAlgoOrderId")
                .and_then(|v| v.clone().decode::<f64>().ok())
                == Some(*id)
        }
    }
}
// ts: buildWooV3AlgoAmendRequest
pub fn build_woo_v3_algo_amend_request(
    v: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3AlgoAmendBuildResult, String> {
    let target = validate_target(&require_record(
        v.get("target"),
        "WOO V3 algo amend target",
    )?)?;
    let o = require_record(v.get("currentOrder"), "WOO V3 algo amend currentOrder")?;
    let blocked = |blocker| WooV3AlgoAmendBuildResult::Blocked {
        blocker,
        target: target.clone(),
    };
    if !target_matches_order(&target, &o) {
        return Ok(blocked("target-id-mismatch"));
    }
    if compare_decimals(
        &stored(
            o.get("totalExecutedQuantity"),
            "WOO V3 algo current totalExecutedQuantity",
        )?,
        "0",
    ) != 0
    {
        return Ok(blocked(
            "partial-fill-algo-amend-semantics-require-demo-evidence",
        ));
    }
    // Source declared isTriggered:boolean is checked at its short-circuit use boundary.
    if value_string(o.get("algoStatus")).as_deref() != Some("NEW")
        || value_string(o.get("rootAlgoStatus")).as_deref() != Some("NEW")
        || require_boolean(o.get("isTriggered"), "WOO V3 algo current isTriggered")?
        || value_string(o.get("triggerStatus")).as_deref() != Some("USELESS")
    {
        return Ok(blocked("terminal-or-triggered-algo-cannot-be-amended"));
    }
    let update = require_record(v.get("update"), "WOO V3 algo amend update")?;
    if let Some(q) = update.get("quantity") {
        require_decimal(Some(q), "WOO V3 algo amend quantity", false)?;
        return Ok(blocked("quantity-amend-semantics-require-demo-evidence"));
    }
    let kind = value_string(o.get("algoType"));
    if matches!(
        kind.as_deref(),
        Some("POSITIONAL_TP_SL" | "TP_SL" | "TAKE_PROFIT" | "STOP_LOSS")
    ) {
        return Ok(blocked("positional-child-update-not-documented"));
    }
    if kind.as_deref() == Some("TRAILING_STOP") || update.contains_key("callbackValue") {
        if let Some(c) = update.get("callbackValue") {
            require_decimal(Some(c), "WOO V3 algo amend callbackValue", false)?;
        }
        return Ok(blocked("trailing-callback-update-not-documented"));
    }
    if kind.as_deref() != Some("STOP") {
        return Ok(blocked("algo-family-update-not-supported"));
    }
    if value_string(o.get("triggerPriceType")).as_deref() != Some("MARKET_PRICE") {
        return Ok(blocked("only-market-price-trigger-is-documented"));
    }
    if !o.contains_key("triggerPrice") {
        return Ok(blocked("stop-trigger-provenance-incomplete"));
    }
    if update.contains_key("triggerPriceType") && unsupported(update.get("triggerPriceType")) {
        return Ok(blocked("only-market-price-trigger-is-documented"));
    }
    if !update.contains_key("triggerPrice") {
        return Err("WOO V3 STOP amend requires triggerPrice".into());
    }
    let price = require_decimal(
        update.get("triggerPrice"),
        "WOO V3 algo amend triggerPrice",
        false,
    )?;
    if compare_decimals(
        &price,
        &stored(o.get("triggerPrice"), "WOO V3 algo current triggerPrice")?,
    ) == 0
    {
        return Err("WOO V3 algo amend must change triggerPrice".into());
    }
    let mut body = target_map(&target);
    body.insert("triggerPrice".into(), text(&price));
    if update.contains_key("triggerPriceType") {
        body.insert("triggerPriceType".into(), text("MARKET_PRICE"));
    }
    Ok(WooV3AlgoAmendBuildResult::Ready {
        request: request(
            WooV3HttpMethod::Put,
            WooV3Endpoint::TradeAlgoOrder,
            Some(body),
            None,
        ),
        target,
        update_semantics: "untriggered-zero-fill-stop-trigger-only",
    })
}
// ts: buildWooV3AlgoCancelRequest
pub fn build_woo_v3_algo_cancel_request(
    v: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3RequestInput, String> {
    Ok(request(
        WooV3HttpMethod::Delete,
        WooV3Endpoint::TradeAlgoOrder,
        None,
        Some(target_map(&validate_target(v)?)),
    ))
}
// ts: buildWooV3AlgoCancelPendingRequest
// None represents the source default all; Some(symbol) represents its object scope arm.
pub fn build_woo_v3_algo_cancel_pending_request(
    symbol: Option<&str>,
) -> Result<WooV3AlgoCancelPendingBuildResult, String> {
    match symbol {
        Some(s) => Ok(WooV3AlgoCancelPendingBuildResult::Blocked {
            blocker: "symbol-scoped-pending-cancel-query-schema-conflicts-with-description",
            symbol: require_perpetual_symbol(s)?,
        }),
        None => Ok(WooV3AlgoCancelPendingBuildResult::Ready {
            request: request(
                WooV3HttpMethod::Delete,
                WooV3Endpoint::TradeAlgoOrders,
                None,
                None,
            ),
            scope: "all-algo-orders-all-symbols",
        }),
    }
}
// ts: decodeMutation
pub fn decode_mutation(
    v: Option<&JsNumberInput>,
    name: &str,
    t: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3AlgoMutationResult, WooV3AlgoWriteError> {
    let WooV3AlgoSuccessData { data, timestamp } = decode_success_data(v, name)?;
    Ok(WooV3AlgoMutationResult {
        accepted: true,
        target: validate_target(t)?,
        timestamp,
        identity_evidence: "request-target-only-response-does-not-echo-id-or-status".into(),
        gateway_data: data,
    })
}
// ts: decodeWooV3AlgoAmendResponse
pub fn decode_woo_v3_algo_amend_response(
    v: Option<&JsNumberInput>,
    t: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3AlgoMutationResult, WooV3AlgoWriteError> {
    decode_mutation(v, "amend algo order", t)
}
// ts: decodeWooV3AlgoCancelResponse
pub fn decode_woo_v3_algo_cancel_response(
    v: Option<&JsNumberInput>,
    t: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3AlgoMutationResult, WooV3AlgoWriteError> {
    decode_mutation(v, "cancel algo order", t)
}
// ts: decodeWooV3AlgoCancelPendingResponse
pub fn decode_woo_v3_algo_cancel_pending_response(
    v: Option<&JsNumberInput>,
) -> Result<WooV3AlgoCancelPendingResult, WooV3AlgoWriteError> {
    let WooV3AlgoSuccessData { data, timestamp } =
        decode_success_data(v, "cancel pending algo orders")?;
    Ok(WooV3AlgoCancelPendingResult {
        accepted: true,
        scope: "all-algo-orders-all-symbols".into(),
        timestamp,
        gateway_data: data,
    })
}
impl JsNumberOutput for WooV3AlgoPlaceBuildResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Ready{request,family,trigger_semantics},Blocked{blocker,family}});
    }
}
impl JsNumberOutput for WooV3AlgoPlaceDecodeResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Ready{row,timestamp,identity_evidence},Blocked{blocker,rows,timestamp}});
    }
}
impl JsNumberOutput for WooV3AlgoAmendBuildResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Ready{request,target,update_semantics},Blocked{blocker,target}});
    }
}
impl JsNumberOutput for WooV3AlgoCancelPendingBuildResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Ready{request,scope},Blocked{blocker,symbol}});
    }
}
impl JsNumberOutput for WooV3AlgoWriteResponseError {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{name,message,code,gateway_response,contract_status});
    }
}
impl JsNumberOutput for WooV3AlgoWriteError {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Validation(message),Gateway(error)});
    }
}
impl JsNumberOutput for WooV3AlgoSuccessData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{data,timestamp});
    }
}
impl JsNumberOutput for WooV3AlgoWriteParity {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{request_and_response_fixtures,runtime_imported,credentialed_demo_lifecycle,trigger_price_type,trailing_input,positional_response_identity,positional_update,partial_fill_and_quantity_amend,symbol_pending_cancel,cancel_response_identity});
    }
}
