//! Pure private V3 payload and frame decoding; no live socket/store ownership.
use crate::v3_ws_public::{WooV3WsPublicPayload, decode_public_payload};
use crate::v3_ws_session::{require_boolean, require_decimal, require_integer};
use crate::v3_ws_state::{WOO_V3_WS_BLOCKERS, WooV3WsBlocker, WooV3WsBlockerCode};
use crate::v3_ws_topics::{
    PrivateChannel, PublicChannel, WooV3WsPrivateTopic, WooV3WsTopicDescriptor,
    parse_woo_v3_ws_topic, require_symbol,
};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_types::object_map::JsObjectMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
fn literal_0<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "PURE_SPOT" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported source literal"))
    }
}
fn literal_1<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "MARGIN" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported source literal"))
    }
}
fn literal_2<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "FUTURE" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported source literal"))
    }
}
fn literal_3<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "CROSS" || value == "ISOLATED" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported source literal"))
    }
}
fn literal_4<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "ONE_WAY" || value == "HEDGE_MODE" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported source literal"))
    }
}
fn literal_5<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "BOTH" || value == "LONG" || value == "SHORT" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported source literal"))
    }
}
fn literal_6<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    let value = f64::deserialize(d)?;
    if value == 0.0 {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported source literal"))
    }
}
fn literal_7<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "BUY" || value == "SELL" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported source literal"))
    }
}
fn literal_8<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    let value = f64::deserialize(d)?;
    if value == 1.0 || value == 2.0 {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported source literal"))
    }
}
fn literal_9<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    let value = f64::deserialize(d)?;
    if value == 3.0 {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported source literal"))
    }
}

// ts: WooV3WsAccount
#[derive(Debug, Clone, PartialEq)]
pub enum WooV3WsAccount {
    PureSpot {
        mode: String,
        total_account_value: String,
    },
    Margin {
        mode: String,
        leverage: f64,
        margin_ratio: String,
        open_margin_ratio: String,
        initial_margin_ratio: String,
        maintenance_margin_ratio: String,
        total_collateral: String,
        free_collateral: String,
        total_account_value: String,
    },
    Future {
        mode: String,
        default_margin_mode: String,
        position_mode: String,
        margin_ratio: String,
        open_margin_ratio: String,
        initial_margin_ratio: String,
        maintenance_margin_ratio: String,
        total_collateral: String,
        free_collateral: String,
        total_account_value: String,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsAccount", untagged)]
enum WooV3WsAccountFields {
    PureSpot {
        #[serde(rename = "mode", deserialize_with = "literal_0")]
        mode: String,
        #[serde(rename = "totalAccountValue")]
        total_account_value: String,
    },
    Margin {
        #[serde(rename = "mode", deserialize_with = "literal_1")]
        mode: String,
        #[serde(rename = "leverage")]
        leverage: f64,
        #[serde(rename = "marginRatio")]
        margin_ratio: String,
        #[serde(rename = "openMarginRatio")]
        open_margin_ratio: String,
        #[serde(rename = "initialMarginRatio")]
        initial_margin_ratio: String,
        #[serde(rename = "maintenanceMarginRatio")]
        maintenance_margin_ratio: String,
        #[serde(rename = "totalCollateral")]
        total_collateral: String,
        #[serde(rename = "freeCollateral")]
        free_collateral: String,
        #[serde(rename = "totalAccountValue")]
        total_account_value: String,
    },
    Future {
        #[serde(rename = "mode", deserialize_with = "literal_2")]
        mode: String,
        #[serde(rename = "defaultMarginMode", deserialize_with = "literal_3")]
        default_margin_mode: String,
        #[serde(rename = "positionMode", deserialize_with = "literal_4")]
        position_mode: String,
        #[serde(rename = "marginRatio")]
        margin_ratio: String,
        #[serde(rename = "openMarginRatio")]
        open_margin_ratio: String,
        #[serde(rename = "initialMarginRatio")]
        initial_margin_ratio: String,
        #[serde(rename = "maintenanceMarginRatio")]
        maintenance_margin_ratio: String,
        #[serde(rename = "totalCollateral")]
        total_collateral: String,
        #[serde(rename = "freeCollateral")]
        free_collateral: String,
        #[serde(rename = "totalAccountValue")]
        total_account_value: String,
    },
}
impl Serialize for WooV3WsAccount {
    fn serialize<S: serde::Serializer>(&self, e: S) -> Result<S::Ok, S::Error> {
        WooV3WsAccountFields::serialize(self, e)
    }
}
impl<'de> Deserialize<'de> for WooV3WsAccount {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3WsAccountFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsAccount {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum {PureSpot{mode,total_account_value}, Margin{mode,leverage,margin_ratio,open_margin_ratio,initial_margin_ratio,maintenance_margin_ratio,total_collateral,free_collateral,total_account_value}, Future{mode,default_margin_mode,position_mode,margin_ratio,open_margin_ratio,initial_margin_ratio,maintenance_margin_ratio,total_collateral,free_collateral,total_account_value}});
    }
}

// ts: WooV3WsBalanceRow
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsBalanceRow {
    pub token: String,
    pub holding: String,
    pub frozen: String,
    pub interest: String,
    pub pending_short_quantity: String,
    pub pending_long_quantity: String,
    pub staked: String,
    pub unbonding: String,
    pub vault: String,
    pub launchpad_vault: String,
    pub earn: String,
    pub average_open_price: String,
    pub pnl24h: String,
    pub roi24h: String,
    pub fee24h: String,
    pub mark_price: String,
    pub version: f64,
    pub trial_fund_bonus: String,
    pub loss_cover_ratio: f64,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsBalanceRow")]
struct WooV3WsBalanceRowFields {
    #[serde(rename = "token")]
    token: String,
    #[serde(rename = "holding")]
    holding: String,
    #[serde(rename = "frozen")]
    frozen: String,
    #[serde(rename = "interest")]
    interest: String,
    #[serde(rename = "pendingShortQuantity")]
    pending_short_quantity: String,
    #[serde(rename = "pendingLongQuantity")]
    pending_long_quantity: String,
    #[serde(rename = "staked")]
    staked: String,
    #[serde(rename = "unbonding")]
    unbonding: String,
    #[serde(rename = "vault")]
    vault: String,
    #[serde(rename = "launchpadVault")]
    launchpad_vault: String,
    #[serde(rename = "earn")]
    earn: String,
    #[serde(rename = "averageOpenPrice")]
    average_open_price: String,
    #[serde(rename = "pnl24h")]
    pnl24h: String,
    #[serde(rename = "roi24h")]
    roi24h: String,
    #[serde(rename = "fee24h")]
    fee24h: String,
    #[serde(rename = "markPrice")]
    mark_price: String,
    #[serde(rename = "version")]
    version: f64,
    #[serde(rename = "trialFundBonus")]
    trial_fund_bonus: String,
    #[serde(rename = "lossCoverRatio")]
    loss_cover_ratio: f64,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3WsBalanceRow {
    fn serialize<S: serde::Serializer>(&self, e: S) -> Result<S::Ok, S::Error> {
        WooV3WsBalanceRowFields::serialize(self, e)
    }
}
impl<'de> Deserialize<'de> for WooV3WsBalanceRow {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3WsBalanceRowFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsBalanceRow {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {token,holding,frozen,interest,pending_short_quantity,pending_long_quantity,staked,unbonding,vault,launchpad_vault,earn,average_open_price,pnl24h,roi24h,fee24h,mark_price,version,trial_fund_bonus,loss_cover_ratio,timestamp});
    }
}

// ts: WooV3WsPositionRow
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsPositionRow {
    pub symbol: String,
    pub holding: String,
    pub pending_long_quantity: String,
    pub pending_short_quantity: String,
    pub average_open_price: String,
    pub pnl24h: String,
    pub roi24h: String,
    pub fee24h: String,
    pub settle_price: String,
    pub mark_price: String,
    pub opening_timestamp: f64,
    pub adl_quantile: f64,
    pub leverage: f64,
    pub margin_mode: String,
    pub position_side: String,
    pub isolated_token: String,
    pub isolated_amount: String,
    pub isolated_frozen_long: String,
    pub isolated_frozen_short: String,
    pub version: f64,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsPositionRow")]
struct WooV3WsPositionRowFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "holding")]
    holding: String,
    #[serde(rename = "pendingLongQuantity")]
    pending_long_quantity: String,
    #[serde(rename = "pendingShortQuantity")]
    pending_short_quantity: String,
    #[serde(rename = "averageOpenPrice")]
    average_open_price: String,
    #[serde(rename = "pnl24h")]
    pnl24h: String,
    #[serde(rename = "roi24h")]
    roi24h: String,
    #[serde(rename = "fee24h")]
    fee24h: String,
    #[serde(rename = "settlePrice")]
    settle_price: String,
    #[serde(rename = "markPrice")]
    mark_price: String,
    #[serde(rename = "openingTimestamp")]
    opening_timestamp: f64,
    #[serde(rename = "adlQuantile")]
    adl_quantile: f64,
    #[serde(rename = "leverage")]
    leverage: f64,
    #[serde(rename = "marginMode", deserialize_with = "literal_3")]
    margin_mode: String,
    #[serde(rename = "positionSide", deserialize_with = "literal_5")]
    position_side: String,
    #[serde(rename = "isolatedToken")]
    isolated_token: String,
    #[serde(rename = "isolatedAmount")]
    isolated_amount: String,
    #[serde(rename = "isolatedFrozenLong")]
    isolated_frozen_long: String,
    #[serde(rename = "isolatedFrozenShort")]
    isolated_frozen_short: String,
    #[serde(rename = "version")]
    version: f64,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3WsPositionRow {
    fn serialize<S: serde::Serializer>(&self, e: S) -> Result<S::Ok, S::Error> {
        WooV3WsPositionRowFields::serialize(self, e)
    }
}
impl<'de> Deserialize<'de> for WooV3WsPositionRow {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3WsPositionRowFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsPositionRow {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {symbol,holding,pending_long_quantity,pending_short_quantity,average_open_price,pnl24h,roi24h,fee24h,settle_price,mark_price,opening_timestamp,adl_quantile,leverage,margin_mode,position_side,isolated_token,isolated_amount,isolated_frozen_long,isolated_frozen_short,version,timestamp});
    }
}

// ts: WooV3WsExecutionReport
#[derive(Debug, Clone, PartialEq)]
pub enum WooV3WsExecutionReport {
    Fill {
        message_type: f64,
        symbol: String,
        client_order_id: f64,
        order_id: f64,
        order_type: String,
        side: String,
        position_side: String,
        quantity: String,
        price: String,
        trade_id: f64,
        executed_quantity: String,
        executed_price: String,
        fee: String,
        fee_asset: String,
        total_executed_quantity: String,
        average_executed_price: String,
        status: String,
        reason: String,
        order_tag: String,
        total_fee: String,
        fee_currency: String,
        total_rebate: String,
        rebate_currency: String,
        visible_quantity: String,
        timestamp: f64,
        reduce_only: bool,
        maker: bool,
        leverage: f64,
        matched_rpi: bool,
        margin_mode: String,
    },
    Reject {
        message_type: f64,
        symbol: String,
        order_id: f64,
        client_order_id: f64,
        reason: String,
    },
    Cancel {
        message_type: f64,
        symbol: String,
        reason: String,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsExecutionReport", untagged)]
enum WooV3WsExecutionReportFields {
    Fill {
        #[serde(rename = "messageType", deserialize_with = "literal_6")]
        message_type: f64,
        #[serde(rename = "symbol")]
        symbol: String,
        #[serde(rename = "clientOrderId")]
        client_order_id: f64,
        #[serde(rename = "orderId")]
        order_id: f64,
        #[serde(rename = "orderType")]
        order_type: String,
        #[serde(rename = "side", deserialize_with = "literal_7")]
        side: String,
        #[serde(rename = "positionSide", deserialize_with = "literal_5")]
        position_side: String,
        #[serde(rename = "quantity")]
        quantity: String,
        #[serde(rename = "price")]
        price: String,
        #[serde(rename = "tradeId")]
        trade_id: f64,
        #[serde(rename = "executedQuantity")]
        executed_quantity: String,
        #[serde(rename = "executedPrice")]
        executed_price: String,
        #[serde(rename = "fee")]
        fee: String,
        #[serde(rename = "feeAsset")]
        fee_asset: String,
        #[serde(rename = "totalExecutedQuantity")]
        total_executed_quantity: String,
        #[serde(rename = "averageExecutedPrice")]
        average_executed_price: String,
        #[serde(rename = "status")]
        status: String,
        #[serde(rename = "reason")]
        reason: String,
        #[serde(rename = "orderTag")]
        order_tag: String,
        #[serde(rename = "totalFee")]
        total_fee: String,
        #[serde(rename = "feeCurrency")]
        fee_currency: String,
        #[serde(rename = "totalRebate")]
        total_rebate: String,
        #[serde(rename = "rebateCurrency")]
        rebate_currency: String,
        #[serde(rename = "visibleQuantity")]
        visible_quantity: String,
        #[serde(rename = "timestamp")]
        timestamp: f64,
        #[serde(rename = "reduceOnly")]
        reduce_only: bool,
        #[serde(rename = "maker")]
        maker: bool,
        #[serde(rename = "leverage")]
        leverage: f64,
        #[serde(rename = "matchedRpi")]
        matched_rpi: bool,
        #[serde(rename = "marginMode", deserialize_with = "literal_3")]
        margin_mode: String,
    },
    Reject {
        #[serde(rename = "messageType", deserialize_with = "literal_8")]
        message_type: f64,
        #[serde(rename = "symbol")]
        symbol: String,
        #[serde(rename = "orderId")]
        order_id: f64,
        #[serde(rename = "clientOrderId")]
        client_order_id: f64,
        #[serde(rename = "reason")]
        reason: String,
    },
    Cancel {
        #[serde(rename = "messageType", deserialize_with = "literal_9")]
        message_type: f64,
        #[serde(rename = "symbol")]
        symbol: String,
        #[serde(rename = "reason")]
        reason: String,
    },
}
impl Serialize for WooV3WsExecutionReport {
    fn serialize<S: serde::Serializer>(&self, e: S) -> Result<S::Ok, S::Error> {
        WooV3WsExecutionReportFields::serialize(self, e)
    }
}
impl<'de> Deserialize<'de> for WooV3WsExecutionReport {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3WsExecutionReportFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsExecutionReport {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum {Fill{message_type,symbol,client_order_id,order_id,order_type,side,position_side,quantity,price,trade_id,executed_quantity,executed_price,fee,fee_asset,total_executed_quantity,average_executed_price,status,reason,order_tag,total_fee,fee_currency,total_rebate,rebate_currency,visible_quantity,timestamp,reduce_only,maker,leverage,matched_rpi,margin_mode}, Reject{message_type,symbol,order_id,client_order_id,reason}, Cancel{message_type,symbol,reason}});
    }
}

// ts: WooV3WsAlgoExecutionRow
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsAlgoExecutionRow {
    pub symbol: String,
    pub root_algo_order_id: f64,
    pub parent_algo_order_id: f64,
    pub algo_order_id: f64,
    pub client_algo_order_id: f64,
    pub order_tag: String,
    pub status: String,
    pub algo_type: String,
    pub side: String,
    pub position_side: String,
    pub quantity: String,
    pub trigger_price: String,
    pub trigger_price_type: String,
    pub trigger_status: String,
    pub price: String,
    pub order_type: String,
    pub trigger_execution_price: String,
    pub trigger_timestamp: f64,
    pub trade_id: f64,
    pub executed_price: String,
    pub executed_quantity: String,
    pub fee: String,
    pub fee_asset: String,
    pub reason: String,
    pub total_executed_quantity: String,
    pub average_executed_price: String,
    pub total_fee: String,
    pub fee_currency: String,
    pub total_rebate: String,
    pub rebate_currency: String,
    pub visible_quantity: String,
    pub timestamp: f64,
    pub reduce_only: bool,
    pub activated_price: String,
    pub callback_ratio: String,
    pub callback_value: String,
    pub extreme_price: String,
    pub triggered: bool,
    pub activated: bool,
    pub maker: bool,
    pub root_algo_status: String,
    pub algo_status: String,
    pub leverage: f64,
    pub margin_mode: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsAlgoExecutionRow")]
struct WooV3WsAlgoExecutionRowFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "rootAlgoOrderId")]
    root_algo_order_id: f64,
    #[serde(rename = "parentAlgoOrderId")]
    parent_algo_order_id: f64,
    #[serde(rename = "algoOrderId")]
    algo_order_id: f64,
    #[serde(rename = "clientAlgoOrderId")]
    client_algo_order_id: f64,
    #[serde(rename = "orderTag")]
    order_tag: String,
    #[serde(rename = "status")]
    status: String,
    #[serde(rename = "algoType")]
    algo_type: String,
    #[serde(rename = "side", deserialize_with = "literal_7")]
    side: String,
    #[serde(rename = "positionSide", deserialize_with = "literal_5")]
    position_side: String,
    #[serde(rename = "quantity")]
    quantity: String,
    #[serde(rename = "triggerPrice")]
    trigger_price: String,
    #[serde(rename = "triggerPriceType")]
    trigger_price_type: String,
    #[serde(rename = "triggerStatus")]
    trigger_status: String,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "orderType")]
    order_type: String,
    #[serde(rename = "triggerExecutionPrice")]
    trigger_execution_price: String,
    #[serde(rename = "triggerTimestamp")]
    trigger_timestamp: f64,
    #[serde(rename = "tradeId")]
    trade_id: f64,
    #[serde(rename = "executedPrice")]
    executed_price: String,
    #[serde(rename = "executedQuantity")]
    executed_quantity: String,
    #[serde(rename = "fee")]
    fee: String,
    #[serde(rename = "feeAsset")]
    fee_asset: String,
    #[serde(rename = "reason")]
    reason: String,
    #[serde(rename = "totalExecutedQuantity")]
    total_executed_quantity: String,
    #[serde(rename = "averageExecutedPrice")]
    average_executed_price: String,
    #[serde(rename = "totalFee")]
    total_fee: String,
    #[serde(rename = "feeCurrency")]
    fee_currency: String,
    #[serde(rename = "totalRebate")]
    total_rebate: String,
    #[serde(rename = "rebateCurrency")]
    rebate_currency: String,
    #[serde(rename = "visibleQuantity")]
    visible_quantity: String,
    #[serde(rename = "timestamp")]
    timestamp: f64,
    #[serde(rename = "reduceOnly")]
    reduce_only: bool,
    #[serde(rename = "activatedPrice")]
    activated_price: String,
    #[serde(rename = "callbackRatio")]
    callback_ratio: String,
    #[serde(rename = "callbackValue")]
    callback_value: String,
    #[serde(rename = "extremePrice")]
    extreme_price: String,
    #[serde(rename = "triggered")]
    triggered: bool,
    #[serde(rename = "activated")]
    activated: bool,
    #[serde(rename = "maker")]
    maker: bool,
    #[serde(rename = "rootAlgoStatus")]
    root_algo_status: String,
    #[serde(rename = "algoStatus")]
    algo_status: String,
    #[serde(rename = "leverage")]
    leverage: f64,
    #[serde(rename = "marginMode", deserialize_with = "literal_3")]
    margin_mode: String,
}
impl Serialize for WooV3WsAlgoExecutionRow {
    fn serialize<S: serde::Serializer>(&self, e: S) -> Result<S::Ok, S::Error> {
        WooV3WsAlgoExecutionRowFields::serialize(self, e)
    }
}
impl<'de> Deserialize<'de> for WooV3WsAlgoExecutionRow {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3WsAlgoExecutionRowFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsAlgoExecutionRow {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {symbol,root_algo_order_id,parent_algo_order_id,algo_order_id,client_algo_order_id,order_tag,status,algo_type,side,position_side,quantity,trigger_price,trigger_price_type,trigger_status,price,order_type,trigger_execution_price,trigger_timestamp,trade_id,executed_price,executed_quantity,fee,fee_asset,reason,total_executed_quantity,average_executed_price,total_fee,fee_currency,total_rebate,rebate_currency,visible_quantity,timestamp,reduce_only,activated_price,callback_ratio,callback_value,extreme_price,triggered,activated,maker,root_algo_status,algo_status,leverage,margin_mode});
    }
}

// ts: WooV3WsPrivatePayload
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3WsPrivatePayload {
    Account(WooV3WsAccount),
    Balance(Vec<WooV3WsBalanceRow>),
    Position(Vec<WooV3WsPositionRow>),
    Execution(WooV3WsExecutionReport),
    AlgoExecution(Vec<WooV3WsAlgoExecutionRow>),
}
#[derive(Deserialize)]
#[serde(remote = "WooV3WsPrivatePayload", untagged)]
enum PrivatePayloadFields {
    Account(WooV3WsAccount),
    Balance(Vec<WooV3WsBalanceRow>),
    Position(Vec<WooV3WsPositionRow>),
    Execution(WooV3WsExecutionReport),
    AlgoExecution(Vec<WooV3WsAlgoExecutionRow>),
}
impl<'de> Deserialize<'de> for WooV3WsPrivatePayload {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(PrivatePayloadFields::deserialize)
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsPrivatePayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Account(v),Balance(v),Position(v),Execution(v),AlgoExecution(v)});
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsPrivateDecodedPayload {
    pub payload: WooV3WsPrivatePayload,
    pub blockers: Vec<WooV3WsBlocker>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsPrivateDecodedPayload")]
struct PrivateDecodedFields {
    payload: WooV3WsPrivatePayload,
    blockers: Vec<WooV3WsBlocker>,
}
impl Serialize for WooV3WsPrivateDecodedPayload {
    fn serialize<S: serde::Serializer>(&self, e: S) -> Result<S::Ok, S::Error> {
        PrivateDecodedFields::serialize(self, e)
    }
}
impl<'de> Deserialize<'de> for WooV3WsPrivateDecodedPayload {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                PrivateDecodedFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsPrivateDecodedPayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{payload,blockers});
    }
}
fn public_topic<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<WooV3WsTopicDescriptor, D::Error> {
    let topic = WooV3WsTopicDescriptor::deserialize(d)?;
    if matches!(topic, WooV3WsTopicDescriptor::Private { .. }) {
        Err(serde::de::Error::custom("topic must be public"))
    } else {
        Ok(topic)
    }
}
fn private_topic<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<WooV3WsTopicDescriptor, D::Error> {
    let topic = WooV3WsTopicDescriptor::deserialize(d)?;
    if matches!(topic, WooV3WsTopicDescriptor::Private { .. }) {
        Ok(topic)
    } else {
        Err(serde::de::Error::custom("topic must be private"))
    }
}
// ts: WooV3WsDecodedFrame
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3WsDecodedFrame {
    Public {
        channel: PublicChannel,
        topic: WooV3WsTopicDescriptor,
        #[serde(rename = "sentTimestamp")]
        sent_timestamp: f64,
        payload: WooV3WsPublicPayload,
        blockers: [WooV3WsBlocker; 0],
    },
    Private {
        channel: PrivateChannel,
        topic: WooV3WsTopicDescriptor,
        #[serde(rename = "sentTimestamp")]
        sent_timestamp: f64,
        payload: WooV3WsPrivatePayload,
        blockers: Vec<WooV3WsBlocker>,
    },
}
#[derive(Deserialize)]
#[serde(remote = "WooV3WsDecodedFrame", untagged)]
enum FrameFields {
    Public {
        channel: PublicChannel,
        #[serde(deserialize_with = "public_topic")]
        topic: WooV3WsTopicDescriptor,
        #[serde(rename = "sentTimestamp")]
        sent_timestamp: f64,
        payload: WooV3WsPublicPayload,
        blockers: [WooV3WsBlocker; 0],
    },
    Private {
        channel: PrivateChannel,
        #[serde(deserialize_with = "private_topic")]
        topic: WooV3WsTopicDescriptor,
        #[serde(rename = "sentTimestamp")]
        sent_timestamp: f64,
        payload: WooV3WsPrivatePayload,
        blockers: Vec<WooV3WsBlocker>,
    },
}
impl<'de> Deserialize<'de> for WooV3WsDecodedFrame {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| FrameFields::deserialize(safe_cex_types::strict_decode::object(d)))
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsDecodedFrame {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Public{channel,topic,sent_timestamp,payload,blockers},Private{channel,topic,sent_timestamp,payload,blockers}});
    }
}

fn record(input: Option<&JsNumberInput>, path: &str) -> Result<JsObjectMap<JsNumberInput>, String> {
    input
        .and_then(|v| v.clone().decode::<JsObjectMap<JsNumberInput>>().ok())
        .ok_or_else(|| format!("{path} must be an object"))
}
fn array(input: Option<&JsNumberInput>, path: &str) -> Result<Vec<JsNumberInput>, String> {
    input
        .and_then(|v| v.clone().decode::<Vec<JsNumberInput>>().ok())
        .ok_or_else(|| format!("{path} must be an array"))
}
fn text(input: Option<&JsNumberInput>) -> Option<String> {
    input.and_then(|v| v.clone().decode::<String>().ok())
}
fn string(input: Option<&JsNumberInput>, path: &str, empty: bool) -> Result<String, String> {
    text(input)
        .filter(|s| empty || !s.is_empty())
        .ok_or_else(|| {
            format!(
                "{path} must be {}",
                if empty {
                    "a string"
                } else {
                    "a non-empty string"
                }
            )
        })
}
fn symbol(input: Option<&JsNumberInput>, path: &str) -> Result<String, String> {
    let value = text(input).map(Value::String);
    require_symbol(value.as_ref(), path)
}
fn enumeration(
    input: Option<&JsNumberInput>,
    values: &[&str],
    path: &str,
) -> Result<String, String> {
    text(input)
        .filter(|s| values.contains(&s.as_str()))
        .ok_or_else(|| format!("{path} must be one of {}", values.join(", ")))
}
// ts: decodeAccount
pub fn decode_account(input: Option<&JsNumberInput>) -> Result<WooV3WsAccount, String> {
    let path = "WOO V3 websocket account.data";
    let row = record(input, path)?;
    let mode = enumeration(
        row.get("m"),
        &["PURE_SPOT", "MARGIN", "FUTURE"],
        &format!("{path}.m"),
    )?;
    let total_account_value = require_decimal(row.get("v"), &format!("{path}.v"))?;
    if mode == "PURE_SPOT" {
        return Ok(WooV3WsAccount::PureSpot {
            mode,
            total_account_value,
        });
    }
    let margin_ratio = require_decimal(row.get("mr"), &format!("{path}.mr"))?;
    let open_margin_ratio = require_decimal(row.get("omr"), &format!("{path}.omr"))?;
    let initial_margin_ratio = require_decimal(row.get("imr"), &format!("{path}.imr"))?;
    let maintenance_margin_ratio = require_decimal(row.get("mmr"), &format!("{path}.mmr"))?;
    let total_collateral = require_decimal(row.get("tc"), &format!("{path}.tc"))?;
    let free_collateral = require_decimal(row.get("fc"), &format!("{path}.fc"))?;
    if mode == "MARGIN" {
        return Ok(WooV3WsAccount::Margin {
            mode,
            leverage: require_integer(row.get("l"), &format!("{path}.l"), 1.0)?,
            margin_ratio,
            open_margin_ratio,
            initial_margin_ratio,
            maintenance_margin_ratio,
            total_collateral,
            free_collateral,
            total_account_value,
        });
    }
    Ok(WooV3WsAccount::Future {
        mode,
        default_margin_mode: enumeration(
            row.get("M"),
            &["CROSS", "ISOLATED"],
            &format!("{path}.M"),
        )?,
        position_mode: enumeration(
            row.get("P"),
            &["ONE_WAY", "HEDGE_MODE"],
            &format!("{path}.P"),
        )?,
        margin_ratio,
        open_margin_ratio,
        initial_margin_ratio,
        maintenance_margin_ratio,
        total_collateral,
        free_collateral,
        total_account_value,
    })
}

// ts: decodeBalanceRows
pub fn decode_balance_rows(
    input: Option<&JsNumberInput>,
) -> Result<Vec<WooV3WsBalanceRow>, String> {
    let data = record(input, "WOO V3 websocket balance.data")?;
    let rows = array(
        data.get("balances"),
        "WOO V3 websocket balance.data.balances",
    )?;
    rows.iter()
        .enumerate()
        .map(|(index, value)| {
            let path = format!("WOO V3 websocket balance.data.balances[{index}]");
            let row = record(Some(value), &path)?;
            Ok(WooV3WsBalanceRow {
                token: string(row.get("t"), &format!("{path}.t"), false)?,
                holding: require_decimal(row.get("h"), &format!("{path}.h"))?,
                frozen: require_decimal(row.get("f"), &format!("{path}.f"))?,
                interest: require_decimal(row.get("i"), &format!("{path}.i"))?,
                pending_short_quantity: require_decimal(row.get("psq"), &format!("{path}.psq"))?,
                pending_long_quantity: require_decimal(row.get("plq"), &format!("{path}.plq"))?,
                staked: require_decimal(row.get("s"), &format!("{path}.s"))?,
                unbonding: require_decimal(row.get("u"), &format!("{path}.u"))?,
                vault: require_decimal(row.get("v"), &format!("{path}.v"))?,
                launchpad_vault: require_decimal(row.get("l"), &format!("{path}.l"))?,
                earn: require_decimal(row.get("e"), &format!("{path}.e"))?,
                average_open_price: require_decimal(row.get("aop"), &format!("{path}.aop"))?,
                pnl24h: require_decimal(row.get("pnl"), &format!("{path}.pnl"))?,
                roi24h: require_decimal(row.get("roi"), &format!("{path}.roi"))?,
                fee24h: require_decimal(row.get("fee"), &format!("{path}.fee"))?,
                mark_price: require_decimal(row.get("mp"), &format!("{path}.mp"))?,
                version: require_integer(row.get("ver"), &format!("{path}.ver"), 0.0)?,
                trial_fund_bonus: require_decimal(row.get("b"), &format!("{path}.b"))?,
                loss_cover_ratio: require_integer(row.get("lcr"), &format!("{path}.lcr"), 0.0)?,
                timestamp: require_integer(row.get("ts"), &format!("{path}.ts"), 0.0)?,
            })
        })
        .collect()
}

// ts: decodePositionRows
pub fn decode_position_rows(
    input: Option<&JsNumberInput>,
) -> Result<Vec<WooV3WsPositionRow>, String> {
    let data = record(input, "WOO V3 websocket position.data")?;
    let rows = array(
        data.get("positions"),
        "WOO V3 websocket position.data.positions",
    )?;
    rows.iter()
        .enumerate()
        .map(|(index, value)| {
            let path = format!("WOO V3 websocket position.data.positions[{index}]");
            let row = record(Some(value), &path)?;
            Ok(WooV3WsPositionRow {
                symbol: symbol(row.get("s"), &format!("{path}.s"))?,
                holding: require_decimal(row.get("h"), &format!("{path}.h"))?,
                pending_long_quantity: require_decimal(row.get("plq"), &format!("{path}.plq"))?,
                pending_short_quantity: require_decimal(row.get("psq"), &format!("{path}.psq"))?,
                average_open_price: require_decimal(row.get("aop"), &format!("{path}.aop"))?,
                pnl24h: require_decimal(row.get("pnl"), &format!("{path}.pnl"))?,
                roi24h: require_decimal(row.get("roi"), &format!("{path}.roi"))?,
                fee24h: require_decimal(row.get("fee"), &format!("{path}.fee"))?,
                settle_price: require_decimal(row.get("sp"), &format!("{path}.sp"))?,
                mark_price: require_decimal(row.get("mp"), &format!("{path}.mp"))?,
                opening_timestamp: require_integer(row.get("ot"), &format!("{path}.ot"), 0.0)?,
                adl_quantile: require_integer(row.get("aq"), &format!("{path}.aq"), 0.0)?,
                leverage: require_integer(row.get("lv"), &format!("{path}.lv"), 1.0)?,
                margin_mode: enumeration(
                    row.get("m"),
                    &["CROSS", "ISOLATED"],
                    &format!("{path}.m"),
                )?,
                position_side: enumeration(
                    row.get("ps"),
                    &["BOTH", "LONG", "SHORT"],
                    &format!("{path}.ps"),
                )?,
                isolated_token: string(row.get("it"), &format!("{path}.it"), true)?,
                isolated_amount: require_decimal(row.get("ia"), &format!("{path}.ia"))?,
                isolated_frozen_long: require_decimal(row.get("il"), &format!("{path}.il"))?,
                isolated_frozen_short: require_decimal(row.get("is"), &format!("{path}.is"))?,
                version: require_integer(row.get("ver"), &format!("{path}.ver"), 0.0)?,
                timestamp: require_integer(row.get("ts"), &format!("{path}.ts"), 0.0)?,
            })
        })
        .collect()
}

// ts: decodeAlgoExecutionRows
pub fn decode_algo_execution_rows(
    input: Option<&JsNumberInput>,
) -> Result<Vec<WooV3WsAlgoExecutionRow>, String> {
    let rows = array(input, "WOO V3 websocket algoexecutionreport.data")?;
    rows.iter()
        .enumerate()
        .map(|(index, value)| {
            let path = format!("WOO V3 websocket algoexecutionreport.data[{index}]");
            let row = record(Some(value), &path)?;
            Ok(WooV3WsAlgoExecutionRow {
                symbol: symbol(row.get("s"), &format!("{path}.s"))?,
                root_algo_order_id: require_integer(row.get("raid"), &format!("{path}.raid"), 1.0)?,
                parent_algo_order_id: require_integer(
                    row.get("paid"),
                    &format!("{path}.paid"),
                    0.0,
                )?,
                algo_order_id: require_integer(row.get("aid"), &format!("{path}.aid"), 1.0)?,
                client_algo_order_id: require_integer(
                    row.get("caid"),
                    &format!("{path}.caid"),
                    0.0,
                )?,
                order_tag: string(row.get("tg"), &format!("{path}.tg"), true)?,
                status: string(row.get("ss"), &format!("{path}.ss"), false)?,
                algo_type: string(row.get("at"), &format!("{path}.at"), false)?,
                side: enumeration(row.get("sd"), &["BUY", "SELL"], &format!("{path}.sd"))?,
                position_side: enumeration(
                    row.get("ps"),
                    &["BOTH", "LONG", "SHORT"],
                    &format!("{path}.ps"),
                )?,
                quantity: require_decimal(row.get("sx"), &format!("{path}.sx"))?,
                trigger_price: require_decimal(row.get("trp"), &format!("{path}.trp"))?,
                trigger_price_type: string(row.get("trpt"), &format!("{path}.trpt"), false)?,
                trigger_status: string(row.get("tss"), &format!("{path}.tss"), false)?,
                price: require_decimal(row.get("px"), &format!("{path}.px"))?,
                order_type: string(row.get("t"), &format!("{path}.t"), false)?,
                trigger_execution_price: require_decimal(row.get("tpx"), &format!("{path}.tpx"))?,
                trigger_timestamp: require_integer(row.get("tt"), &format!("{path}.tt"), 0.0)?,
                trade_id: require_integer(row.get("tid"), &format!("{path}.tid"), 0.0)?,
                executed_price: require_decimal(row.get("epx"), &format!("{path}.epx"))?,
                executed_quantity: require_decimal(row.get("esx"), &format!("{path}.esx"))?,
                fee: require_decimal(row.get("f"), &format!("{path}.f"))?,
                fee_asset: string(row.get("fa"), &format!("{path}.fa"), true)?,
                reason: string(row.get("rs"), &format!("{path}.rs"), true)?,
                total_executed_quantity: require_decimal(row.get("tesx"), &format!("{path}.tesx"))?,
                average_executed_price: require_decimal(row.get("aepx"), &format!("{path}.aepx"))?,
                total_fee: require_decimal(row.get("tf"), &format!("{path}.tf"))?,
                fee_currency: string(row.get("tfc"), &format!("{path}.tfc"), true)?,
                total_rebate: require_decimal(row.get("tr"), &format!("{path}.tr"))?,
                rebate_currency: string(row.get("trc"), &format!("{path}.trc"), true)?,
                visible_quantity: require_decimal(row.get("vsx"), &format!("{path}.vsx"))?,
                timestamp: require_integer(row.get("ts"), &format!("{path}.ts"), 0.0)?,
                reduce_only: require_boolean(row.get("ro"), &format!("{path}.ro"))?,
                activated_price: require_decimal(row.get("apx"), &format!("{path}.apx"))?,
                callback_ratio: require_decimal(row.get("cr"), &format!("{path}.cr"))?,
                callback_value: require_decimal(row.get("cv"), &format!("{path}.cv"))?,
                extreme_price: require_decimal(row.get("ep"), &format!("{path}.ep"))?,
                triggered: require_boolean(row.get("it"), &format!("{path}.it"))?,
                activated: require_boolean(row.get("ia"), &format!("{path}.ia"))?,
                maker: require_boolean(row.get("im"), &format!("{path}.im"))?,
                root_algo_status: string(row.get("ras"), &format!("{path}.ras"), false)?,
                algo_status: string(row.get("as"), &format!("{path}.as"), false)?,
                leverage: require_integer(row.get("lv"), &format!("{path}.lv"), 1.0)?,
                margin_mode: enumeration(
                    row.get("m"),
                    &["CROSS", "ISOLATED"],
                    &format!("{path}.m"),
                )?,
            })
        })
        .collect()
}
// ts: decodeExecutionReport
pub fn decode_execution_report(
    input: Option<&JsNumberInput>,
) -> Result<WooV3WsExecutionReport, String> {
    let path = "WOO V3 websocket executionreport.data";
    let row = record(input, path)?;
    let message_type = require_integer(row.get("mt"), &format!("{path}.mt"), 0.0)?;
    if message_type == 1.0 || message_type == 2.0 {
        return Ok(WooV3WsExecutionReport::Reject {
            message_type,
            symbol: symbol(row.get("s"), &format!("{path}.s"))?,
            order_id: require_integer(row.get("oid"), &format!("{path}.oid"), 1.0)?,
            client_order_id: require_integer(row.get("cid"), &format!("{path}.cid"), 0.0)?,
            reason: string(row.get("rs"), &format!("{path}.rs"), true)?,
        });
    }
    if message_type == 3.0 {
        return Ok(WooV3WsExecutionReport::Cancel {
            message_type,
            symbol: symbol(row.get("s"), &format!("{path}.s"))?,
            reason: string(row.get("rs"), &format!("{path}.rs"), true)?,
        });
    }
    if message_type != 0.0 {
        return Err(format!("{path}.mt must be 0, 1, 2, or 3"));
    }
    Ok(WooV3WsExecutionReport::Fill {
        message_type,
        symbol: symbol(row.get("s"), &format!("{path}.s"))?,
        client_order_id: require_integer(row.get("cid"), &format!("{path}.cid"), 0.0)?,
        order_id: require_integer(row.get("oid"), &format!("{path}.oid"), 1.0)?,
        order_type: string(row.get("t"), &format!("{path}.t"), false)?,
        side: enumeration(row.get("sd"), &["BUY", "SELL"], &format!("{path}.sd"))?,
        position_side: enumeration(
            row.get("ps"),
            &["BOTH", "LONG", "SHORT"],
            &format!("{path}.ps"),
        )?,
        quantity: require_decimal(row.get("sx"), &format!("{path}.sx"))?,
        price: require_decimal(row.get("px"), &format!("{path}.px"))?,
        trade_id: require_integer(row.get("tid"), &format!("{path}.tid"), 0.0)?,
        executed_quantity: require_decimal(row.get("esx"), &format!("{path}.esx"))?,
        executed_price: require_decimal(row.get("epx"), &format!("{path}.epx"))?,
        fee: require_decimal(row.get("f"), &format!("{path}.f"))?,
        fee_asset: string(row.get("fa"), &format!("{path}.fa"), true)?,
        total_executed_quantity: require_decimal(row.get("tesx"), &format!("{path}.tesx"))?,
        average_executed_price: require_decimal(row.get("aepx"), &format!("{path}.aepx"))?,
        status: string(row.get("ss"), &format!("{path}.ss"), false)?,
        reason: string(row.get("rs"), &format!("{path}.rs"), true)?,
        order_tag: string(row.get("tg"), &format!("{path}.tg"), true)?,
        total_fee: require_decimal(row.get("tf"), &format!("{path}.tf"))?,
        fee_currency: string(row.get("tfc"), &format!("{path}.tfc"), true)?,
        total_rebate: require_decimal(row.get("tr"), &format!("{path}.tr"))?,
        rebate_currency: string(row.get("trc"), &format!("{path}.trc"), true)?,
        visible_quantity: require_decimal(row.get("vsx"), &format!("{path}.vsx"))?,
        timestamp: require_integer(row.get("ts"), &format!("{path}.ts"), 0.0)?,
        reduce_only: require_boolean(row.get("ro"), &format!("{path}.ro"))?,
        maker: require_boolean(row.get("mk"), &format!("{path}.mk"))?,
        leverage: require_integer(row.get("lv"), &format!("{path}.lv"), 1.0)?,
        matched_rpi: require_boolean(row.get("rpi"), &format!("{path}.rpi"))?,
        margin_mode: enumeration(row.get("m"), &["CROSS", "ISOLATED"], &format!("{path}.m"))?,
    })
}
// ts: decodePrivatePayload
pub fn decode_private_payload(
    topic: WooV3WsPrivateTopic,
    input: Option<&JsNumberInput>,
) -> Result<WooV3WsPrivateDecodedPayload, String> {
    let (payload, code) = match topic {
        WooV3WsPrivateTopic::Account => (
            WooV3WsPrivatePayload::Account(decode_account(input)?),
            WooV3WsBlockerCode::PrivateAccountReplacementSemanticsUndocumented,
        ),
        WooV3WsPrivateTopic::Balance => (
            WooV3WsPrivatePayload::Balance(decode_balance_rows(input)?),
            WooV3WsBlockerCode::PrivateBalanceReplacementSemanticsUndocumented,
        ),
        WooV3WsPrivateTopic::Position => (
            WooV3WsPrivatePayload::Position(decode_position_rows(input)?),
            WooV3WsBlockerCode::PrivatePositionReplacementSemanticsUndocumented,
        ),
        WooV3WsPrivateTopic::Executionreport => (
            WooV3WsPrivatePayload::Execution(decode_execution_report(input)?),
            WooV3WsBlockerCode::ExecutionStatusLifecycleIncomplete,
        ),
        WooV3WsPrivateTopic::Algoexecutionreport => (
            WooV3WsPrivatePayload::AlgoExecution(decode_algo_execution_rows(input)?),
            WooV3WsBlockerCode::AlgoShapeAndLifecycleIncomplete,
        ),
    };
    let blocker = WOO_V3_WS_BLOCKERS
        .iter()
        .find(|entry| entry.code == code)
        .expect("source blocker inventory")
        .clone();
    Ok(WooV3WsPrivateDecodedPayload {
        payload,
        blockers: vec![blocker],
    })
}
// ts: decodeWooV3WsDataFrame
pub fn decode_woo_v3_ws_data_frame(
    input: Option<&JsNumberInput>,
) -> Result<WooV3WsDecodedFrame, String> {
    let frame = record(input, "WOO V3 websocket data frame")?;
    let topic_input = text(frame.get("topic")).map(Value::String);
    let topic = parse_woo_v3_ws_topic(topic_input.as_ref())?;
    let sent_timestamp = require_integer(frame.get("ts"), "WOO V3 websocket data frame.ts", 0.0)?;
    if let WooV3WsTopicDescriptor::Private { kind, .. } = &topic {
        let decoded = decode_private_payload(*kind, frame.get("data"))?;
        Ok(WooV3WsDecodedFrame::Private {
            channel: PrivateChannel::Private,
            topic,
            sent_timestamp,
            payload: decoded.payload,
            blockers: decoded.blockers,
        })
    } else {
        let payload = decode_public_payload(&topic, frame.get("data"))?;
        Ok(WooV3WsDecodedFrame::Public {
            channel: PublicChannel::Public,
            topic,
            sent_timestamp,
            payload,
            blockers: [],
        })
    }
}
