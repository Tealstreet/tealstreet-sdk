//! Inert private regular/algo orders and transactions. No live adapter or store mutation.
use crate::v3_private::{
    WooV3AccountPositionMode, WooV3PaginatedRows, WooV3PositionSide,
    assert_position_side_matches_mode, decimal_to_number, decode_paginated_rows,
    normalize_perpetual_symbol, optional_decimal_string, require_array, require_boolean,
    require_decimal_string, require_enum, require_finite_number, require_integer, require_record,
    require_string,
};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_types::{
    BothPositionSide, Execution, ExecutionPositionSide, HistoricalTrade, OrderSide, OrderStatus,
    OrderStopTriggerType, OrderTimeInForce, OrderType,
};
use serde::{Deserialize, Serialize};
fn deserialize_present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
fn nullable_decimal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(d)
}
fn maker_flag<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    let v = f64::deserialize(d)?;
    if v == 0.0 || v == 1.0 {
        Ok(v)
    } else {
        Err(serde::de::Error::custom("isMaker must be 0 or 1"))
    }
}
// ts: WooV3OrderSide
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3OrderSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3OrderSide")]
enum AcceptedWooV3OrderSideWire {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
impl<'de> Deserialize<'de> for WooV3OrderSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3OrderSideWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3OrderSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Buy,Sell});
    }
}
// ts: WooV3OrderStatus
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3OrderStatus {
    #[serde(rename = "NEW")]
    New,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "PARTIAL_FILLED")]
    PartialFilled,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "REJECTED")]
    Rejected,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3OrderStatus")]
enum AcceptedWooV3OrderStatusWire {
    #[serde(rename = "NEW")]
    New,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "PARTIAL_FILLED")]
    PartialFilled,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "REJECTED")]
    Rejected,
}
impl<'de> Deserialize<'de> for WooV3OrderStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3OrderStatusWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3OrderStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{New,Cancelled,PartialFilled,Filled,Rejected});
    }
}
// ts: WooV3RegularOrderType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3RegularOrderType {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "IOC")]
    Ioc,
    #[serde(rename = "POST_ONLY")]
    PostOnly,
    #[serde(rename = "AC")]
    Ac,
    #[serde(rename = "FOK")]
    Fok,
    #[serde(rename = "LIQUIDATE")]
    Liquidate,
    #[serde(rename = "BID")]
    Bid,
    #[serde(rename = "ASK")]
    Ask,
    #[serde(rename = "RPI")]
    Rpi,
    #[serde(rename = "LIQUIDATE_BLP")]
    LiquidateBlp,
    #[serde(rename = "ADL")]
    Adl,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3RegularOrderType")]
enum AcceptedWooV3RegularOrderTypeWire {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "IOC")]
    Ioc,
    #[serde(rename = "POST_ONLY")]
    PostOnly,
    #[serde(rename = "AC")]
    Ac,
    #[serde(rename = "FOK")]
    Fok,
    #[serde(rename = "LIQUIDATE")]
    Liquidate,
    #[serde(rename = "BID")]
    Bid,
    #[serde(rename = "ASK")]
    Ask,
    #[serde(rename = "RPI")]
    Rpi,
    #[serde(rename = "LIQUIDATE_BLP")]
    LiquidateBlp,
    #[serde(rename = "ADL")]
    Adl,
}
impl<'de> Deserialize<'de> for WooV3RegularOrderType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3RegularOrderTypeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3RegularOrderType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Limit,Market,Ioc,PostOnly,Ac,Fok,Liquidate,Bid,Ask,Rpi,LiquidateBlp,Adl});
    }
}
// ts: WooV3AlgoType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3AlgoType {
    #[serde(rename = "STOP")]
    Stop,
    #[serde(rename = "OCO")]
    Oco,
    #[serde(rename = "TRAILING_STOP")]
    TrailingStop,
    #[serde(rename = "POSITIONAL_TP_SL")]
    PositionalTpSl,
    #[serde(rename = "TP_SL")]
    TpSl,
    #[serde(rename = "BRACKET")]
    Bracket,
    #[serde(rename = "STOP_BRACKET")]
    StopBracket,
    #[serde(rename = "TAKE_PROFIT")]
    TakeProfit,
    #[serde(rename = "STOP_LOSS")]
    StopLoss,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3AlgoType")]
enum AcceptedWooV3AlgoTypeWire {
    #[serde(rename = "STOP")]
    Stop,
    #[serde(rename = "OCO")]
    Oco,
    #[serde(rename = "TRAILING_STOP")]
    TrailingStop,
    #[serde(rename = "POSITIONAL_TP_SL")]
    PositionalTpSl,
    #[serde(rename = "TP_SL")]
    TpSl,
    #[serde(rename = "BRACKET")]
    Bracket,
    #[serde(rename = "STOP_BRACKET")]
    StopBracket,
    #[serde(rename = "TAKE_PROFIT")]
    TakeProfit,
    #[serde(rename = "STOP_LOSS")]
    StopLoss,
}
impl<'de> Deserialize<'de> for WooV3AlgoType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3AlgoTypeWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AlgoType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Stop,Oco,TrailingStop,PositionalTpSl,TpSl,Bracket,StopBracket,TakeProfit,StopLoss});
    }
}
// ts: WooV3TriggerPriceType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3TriggerPriceType {
    #[serde(rename = "MARKET_PRICE")]
    MarketPrice,
    #[serde(rename = "INDEX_PRICE")]
    IndexPrice,
    #[serde(rename = "LAST_PRICE")]
    LastPrice,
}

// Remote variants must retain the public enum names.
#[allow(clippy::enum_variant_names)]
#[derive(Deserialize)]
#[serde(remote = "WooV3TriggerPriceType")]
enum AcceptedWooV3TriggerPriceTypeWire {
    #[serde(rename = "MARKET_PRICE")]
    MarketPrice,
    #[serde(rename = "INDEX_PRICE")]
    IndexPrice,
    #[serde(rename = "LAST_PRICE")]
    LastPrice,
}
impl<'de> Deserialize<'de> for WooV3TriggerPriceType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3TriggerPriceTypeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3TriggerPriceType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{MarketPrice,IndexPrice,LastPrice});
    }
}
// ts: WooV3TriggerStatus
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3TriggerStatus {
    #[serde(rename = "USELESS")]
    Useless,
    #[serde(rename = "TRIGGERED")]
    Triggered,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "SUCCESS")]
    Success,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3TriggerStatus")]
enum AcceptedWooV3TriggerStatusWire {
    #[serde(rename = "USELESS")]
    Useless,
    #[serde(rename = "TRIGGERED")]
    Triggered,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "SUCCESS")]
    Success,
}
impl<'de> Deserialize<'de> for WooV3TriggerStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3TriggerStatusWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3TriggerStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Useless,Triggered,Cancelled,Success});
    }
}
// ts: WooV3MarginMode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3MarginMode {
    #[serde(rename = "ISOLATED")]
    Isolated,
    #[serde(rename = "CROSSED")]
    Crossed,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3MarginMode")]
enum AcceptedWooV3MarginModeWire {
    #[serde(rename = "ISOLATED")]
    Isolated,
    #[serde(rename = "CROSSED")]
    Crossed,
}
impl<'de> Deserialize<'de> for WooV3MarginMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3MarginModeWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3MarginMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Isolated,Crossed});
    }
}
// ts: WooV3GatewayAlgoQuantity
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum WooV3GatewayAlgoQuantity {
    Decimal(String),
    Number(f64),
}
impl<'de> Deserialize<'de> for WooV3GatewayAlgoQuantity {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(d)?;
        if let Ok(s) = input.clone().decode::<String>() {
            Ok(Self::Decimal(s))
        } else {
            input
                .decode::<f64>()
                .map(Self::Number)
                .map_err(serde::de::Error::custom)
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3GatewayAlgoQuantity {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Decimal(v),Number(v)});
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum WooV3OrderAmount {
    Null,
    Text(String),
}
impl<'de> Deserialize<'de> for WooV3OrderAmount {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(d)?;
        if input.clone().decode::<()>().is_ok() {
            Ok(Self::Null)
        } else {
            input
                .decode::<String>()
                .map(Self::Text)
                .map_err(serde::de::Error::custom)
        }
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3OrderAmount {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Null,Text(v)});
    }
}
// ts: WooV3RegularOrder
#[derive(Debug, Clone)]
pub struct WooV3RegularOrder {
    pub symbol: String,
    pub status: WooV3OrderStatus,
    pub side: WooV3OrderSide,
    pub position_side: WooV3PositionSide,
    pub created_time: f64,
    pub order_id: f64,
    pub order_tag: String,
    pub price: String,
    pub r#type: WooV3RegularOrderType,
    pub quantity: String,
    pub amount: WooV3OrderAmount,
    pub visible: String,
    pub executed: String,
    pub total_fee: String,
    pub fee_asset: String,
    pub total_rebate: f64,
    pub rebate_currency: String,
    pub client_order_id: f64,
    pub reduce_only: bool,
    pub realized_pnl: Option<String>,
    pub average_executed_price: String,
    pub leverage: Option<f64>,
    pub margin_mode: Option<WooV3MarginMode>,
    pub bid_ask_level: Option<f64>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3RegularOrder")]
struct WooV3RegularOrderFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "status")]
    status: WooV3OrderStatus,
    #[serde(rename = "side")]
    side: WooV3OrderSide,
    #[serde(rename = "positionSide")]
    position_side: WooV3PositionSide,
    #[serde(rename = "createdTime")]
    created_time: f64,
    #[serde(rename = "orderId")]
    order_id: f64,
    #[serde(rename = "orderTag")]
    order_tag: String,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "type")]
    r#type: WooV3RegularOrderType,
    #[serde(rename = "quantity")]
    quantity: String,
    #[serde(rename = "amount")]
    amount: WooV3OrderAmount,
    #[serde(rename = "visible")]
    visible: String,
    #[serde(rename = "executed")]
    executed: String,
    #[serde(rename = "totalFee")]
    total_fee: String,
    #[serde(rename = "feeAsset")]
    fee_asset: String,
    #[serde(rename = "totalRebate")]
    total_rebate: f64,
    #[serde(rename = "rebateCurrency")]
    rebate_currency: String,
    #[serde(rename = "clientOrderId")]
    client_order_id: f64,
    #[serde(rename = "reduceOnly")]
    reduce_only: bool,
    #[serde(
        rename = "realizedPnl",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    realized_pnl: Option<String>,
    #[serde(rename = "averageExecutedPrice")]
    average_executed_price: String,
    #[serde(
        rename = "leverage",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    leverage: Option<f64>,
    #[serde(
        rename = "marginMode",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    margin_mode: Option<WooV3MarginMode>,
    #[serde(
        rename = "bidAskLevel",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    bid_ask_level: Option<f64>,
}
impl Serialize for WooV3RegularOrder {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3RegularOrderFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3RegularOrder {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3RegularOrderFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3RegularOrder {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{symbol,status,side,position_side,created_time,order_id,order_tag,price,r#type,quantity,amount,visible,executed,total_fee,fee_asset,total_rebate,rebate_currency,client_order_id,reduce_only,realized_pnl,average_executed_price,leverage,margin_mode,bid_ask_level});
    }
}
// ts: WooV3AlgoOrder
#[derive(Debug, Clone)]
pub struct WooV3AlgoOrder {
    pub algo_order_id: f64,
    pub client_algo_order_id: f64,
    pub root_algo_order_id: f64,
    pub parent_algo_order_id: f64,
    pub symbol: String,
    pub algo_order_tag: String,
    pub algo_type: WooV3AlgoType,
    pub side: WooV3OrderSide,
    pub position_side: WooV3PositionSide,
    pub quantity: WooV3GatewayAlgoQuantity,
    pub is_triggered: bool,
    pub trigger_status: WooV3TriggerStatus,
    pub r#type: Option<WooV3RegularOrderType>,
    pub root_algo_status: WooV3OrderStatus,
    pub algo_status: WooV3OrderStatus,
    pub trigger_price_type: WooV3TriggerPriceType,
    pub trigger_time: String,
    pub trigger_price: Option<String>,
    pub price: Option<String>,
    pub visible_quantity: Option<String>,
    pub total_executed_quantity: String,
    pub average_executed_price: String,
    pub total_fee: String,
    pub fee_asset: String,
    pub total_rebate: f64,
    pub rebate_currency: String,
    pub reduce_only: bool,
    pub created_time: f64,
    pub updated_time: f64,
    pub is_activated: bool,
    pub callback_rate: Option<String>,
    pub activated_price: Option<String>,
    pub leverage: Option<f64>,
    pub margin_mode: Option<WooV3MarginMode>,
    pub child_orders: Vec<WooV3AlgoOrder>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AlgoOrder")]
struct WooV3AlgoOrderFields {
    #[serde(rename = "algoOrderId")]
    algo_order_id: f64,
    #[serde(rename = "clientAlgoOrderId")]
    client_algo_order_id: f64,
    #[serde(rename = "rootAlgoOrderId")]
    root_algo_order_id: f64,
    #[serde(rename = "parentAlgoOrderId")]
    parent_algo_order_id: f64,
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "algoOrderTag")]
    algo_order_tag: String,
    #[serde(rename = "algoType")]
    algo_type: WooV3AlgoType,
    #[serde(rename = "side")]
    side: WooV3OrderSide,
    #[serde(rename = "positionSide")]
    position_side: WooV3PositionSide,
    #[serde(rename = "quantity")]
    quantity: WooV3GatewayAlgoQuantity,
    #[serde(rename = "isTriggered")]
    is_triggered: bool,
    #[serde(rename = "triggerStatus")]
    trigger_status: WooV3TriggerStatus,
    #[serde(
        rename = "type",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    r#type: Option<WooV3RegularOrderType>,
    #[serde(rename = "rootAlgoStatus")]
    root_algo_status: WooV3OrderStatus,
    #[serde(rename = "algoStatus")]
    algo_status: WooV3OrderStatus,
    #[serde(rename = "triggerPriceType")]
    trigger_price_type: WooV3TriggerPriceType,
    #[serde(rename = "triggerTime")]
    trigger_time: String,
    #[serde(
        rename = "triggerPrice",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    trigger_price: Option<String>,
    #[serde(
        rename = "price",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    price: Option<String>,
    #[serde(
        rename = "visibleQuantity",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    visible_quantity: Option<String>,
    #[serde(rename = "totalExecutedQuantity")]
    total_executed_quantity: String,
    #[serde(rename = "averageExecutedPrice")]
    average_executed_price: String,
    #[serde(rename = "totalFee")]
    total_fee: String,
    #[serde(rename = "feeAsset")]
    fee_asset: String,
    #[serde(rename = "totalRebate")]
    total_rebate: f64,
    #[serde(rename = "rebateCurrency")]
    rebate_currency: String,
    #[serde(rename = "reduceOnly")]
    reduce_only: bool,
    #[serde(rename = "createdTime")]
    created_time: f64,
    #[serde(rename = "updatedTime")]
    updated_time: f64,
    #[serde(rename = "isActivated")]
    is_activated: bool,
    #[serde(
        rename = "callbackRate",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    callback_rate: Option<String>,
    #[serde(
        rename = "activatedPrice",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    activated_price: Option<String>,
    #[serde(
        rename = "leverage",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    leverage: Option<f64>,
    #[serde(
        rename = "marginMode",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    margin_mode: Option<WooV3MarginMode>,
    #[serde(rename = "childOrders")]
    child_orders: Vec<WooV3AlgoOrder>,
}
impl Serialize for WooV3AlgoOrder {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AlgoOrderFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AlgoOrder {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AlgoOrderFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AlgoOrder {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{algo_order_id,client_algo_order_id,root_algo_order_id,parent_algo_order_id,symbol,algo_order_tag,algo_type,side,position_side,quantity,is_triggered,trigger_status,r#type,root_algo_status,algo_status,trigger_price_type,trigger_time,trigger_price,price,visible_quantity,total_executed_quantity,average_executed_price,total_fee,fee_asset,total_rebate,rebate_currency,reduce_only,created_time,updated_time,is_activated,callback_rate,activated_price,leverage,margin_mode,child_orders});
    }
}
// ts: WooV3Transaction
#[derive(Debug, Clone)]
pub struct WooV3Transaction {
    pub id: f64,
    pub symbol: String,
    pub order_id: f64,
    pub order_tag: String,
    pub executed_price: String,
    pub executed_quantity: String,
    pub is_maker: f64,
    pub is_match_rpi: bool,
    pub side: WooV3OrderSide,
    pub fee: f64,
    pub fee_asset: String,
    pub realized_pnl: Option<String>,
    pub executed_timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3Transaction")]
struct WooV3TransactionFields {
    #[serde(rename = "id")]
    id: f64,
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "orderId")]
    order_id: f64,
    #[serde(rename = "orderTag")]
    order_tag: String,
    #[serde(rename = "executedPrice")]
    executed_price: String,
    #[serde(rename = "executedQuantity")]
    executed_quantity: String,
    #[serde(rename = "isMaker", deserialize_with = "maker_flag")]
    is_maker: f64,
    #[serde(rename = "isMatchRpi")]
    is_match_rpi: bool,
    #[serde(rename = "side")]
    side: WooV3OrderSide,
    #[serde(rename = "fee")]
    fee: f64,
    #[serde(rename = "feeAsset")]
    fee_asset: String,
    #[serde(rename = "realizedPnl", deserialize_with = "nullable_decimal")]
    realized_pnl: Option<String>,
    #[serde(rename = "executedTimestamp")]
    executed_timestamp: f64,
}
impl Serialize for WooV3Transaction {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3TransactionFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3Transaction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3TransactionFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3Transaction {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{id,symbol,order_id,order_tag,executed_price,executed_quantity,is_maker,is_match_rpi,side,fee,fee_asset,realized_pnl,executed_timestamp});
    }
}
// ts: WooV3MappedTransaction
#[derive(Debug, Clone)]
pub struct WooV3MappedTransaction {
    pub execution: Execution,
    pub historical_trade: Option<HistoricalTrade>,
    pub deferred_historical_trade_reason: Option<String>,
    pub maker: bool,
    pub is_match_rpi: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3MappedTransaction")]
struct WooV3MappedTransactionFields {
    #[serde(rename = "execution")]
    execution: Execution,
    #[serde(
        rename = "historicalTrade",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    historical_trade: Option<HistoricalTrade>,
    #[serde(
        rename = "deferredHistoricalTradeReason",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    deferred_historical_trade_reason: Option<String>,
    #[serde(rename = "maker")]
    maker: bool,
    #[serde(rename = "isMatchRpi")]
    is_match_rpi: bool,
}
impl Serialize for WooV3MappedTransaction {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3MappedTransactionFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3MappedTransaction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3MappedTransactionFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3MappedTransaction {
    fn check_number_output(&self) {
        let Self {
            execution,
            historical_trade,
            deferred_historical_trade_reason,
            maker,
            is_match_rpi,
        } = self;
        execution.check_number_output();
        deferred_historical_trade_reason.check_number_output();
        maker.check_number_output();
        is_match_rpi.check_number_output();
        // Shared HistoricalTrade has no blanket eligibility impl. Destructure every field
        // here so shared schema additions fail this compile check rather than escape it.
        if let Some(HistoricalTrade {
            id,
            symbol,
            order_id,
            price,
            qty,
            quote_qty,
            realized_pnl,
            position_side,
            side,
            buyer,
            maker,
            commission,
            commission_asset,
            time,
        }) = historical_trade
        {
            id.check_number_output();
            symbol.check_number_output();
            order_id.check_number_output();
            price.check_number_output();
            qty.check_number_output();
            quote_qty.check_number_output();
            realized_pnl.check_number_output();
            position_side.check_number_output();
            side.check_number_output();
            buyer.check_number_output();
            maker.check_number_output();
            commission.check_number_output();
            commission_asset.check_number_output();
            time.check_number_output();
        }
    }
}
// ts: WooV3DeferredAlgoOrder
#[derive(Debug, Clone)]
pub struct WooV3DeferredAlgoOrder {
    pub order: WooV3AlgoOrder,
    pub reason: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3DeferredAlgoOrder")]
struct WooV3DeferredAlgoOrderFields {
    #[serde(rename = "order")]
    order: WooV3AlgoOrder,
    #[serde(rename = "reason")]
    reason: String,
}
impl Serialize for WooV3DeferredAlgoOrder {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3DeferredAlgoOrderFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3DeferredAlgoOrder {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3DeferredAlgoOrderFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3DeferredAlgoOrder {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{order,reason});
    }
}
// ts: WooV3AlgoOrderMapping
#[derive(Debug, Clone)]
pub struct WooV3AlgoOrderMapping {
    pub orders: Vec<WooV3MappedOrder<WooV3AlgoOrder>>,
    pub deferred: Vec<WooV3DeferredAlgoOrder>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AlgoOrderMapping")]
struct WooV3AlgoOrderMappingFields {
    #[serde(rename = "orders")]
    orders: Vec<WooV3MappedOrder<WooV3AlgoOrder>>,
    #[serde(rename = "deferred")]
    deferred: Vec<WooV3DeferredAlgoOrder>,
}
impl Serialize for WooV3AlgoOrderMapping {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AlgoOrderMappingFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AlgoOrderMapping {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AlgoOrderMappingFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AlgoOrderMapping {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{orders,deferred});
    }
}
#[derive(Debug, Clone)]
pub struct WooV3MappedOrder<T> {
    pub id: String,
    pub parent_id: Option<String>,
    pub status: OrderStatus,
    pub symbol: String,
    pub r#type: OrderType,
    pub side: OrderSide,
    pub price: f64,
    pub amount: f64,
    pub filled: f64,
    pub remaining: f64,
    pub reduce_only: bool,
    pub time_in_force: Option<OrderTimeInForce>,
    pub tp_trigger_type: Option<OrderStopTriggerType>,
    pub sl_trigger_type: Option<OrderStopTriggerType>,
    pub last_update: f64,
    pub info: T,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3MappedOrder", rename_all = "camelCase")]
struct WooV3MappedOrderFields<T> {
    pub id: String,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub parent_id: Option<String>,
    pub status: OrderStatus,
    pub symbol: String,
    pub r#type: OrderType,
    pub side: OrderSide,
    pub price: f64,
    pub amount: f64,
    pub filled: f64,
    pub remaining: f64,
    pub reduce_only: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub time_in_force: Option<OrderTimeInForce>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tp_trigger_type: Option<OrderStopTriggerType>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub sl_trigger_type: Option<OrderStopTriggerType>,
    pub last_update: f64,
    pub info: T,
}
impl<T: Serialize> Serialize for WooV3MappedOrder<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3MappedOrderFields::serialize(self, s)
    }
}
impl<
    'de,
    T: serde::de::DeserializeOwned + Serialize + safe_cex_num::numeric_ingress::JsNumberOutput,
> Deserialize<'de> for WooV3MappedOrder<T>
{
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3MappedOrderFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl<T: safe_cex_num::numeric_ingress::JsNumberOutput> safe_cex_num::numeric_ingress::JsNumberOutput
    for WooV3MappedOrder<T>
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{id,parent_id,status,symbol,r#type,side,price,amount,filled,remaining,reduce_only,time_in_force,tp_trigger_type,sl_trigger_type,last_update,info});
    }
}

fn enum_field<
    T: serde::de::DeserializeOwned + Serialize + safe_cex_num::numeric_ingress::JsNumberOutput,
>(
    value: Option<&JsNumberInput>,
    values: &[&str],
    path: &str,
) -> Result<T, String> {
    require_enum(value, values, path)?;
    value
        .unwrap()
        .clone()
        .decode::<T>()
        .map_err(|e| e.to_string())
}
fn enum_text<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v)
        .expect("native string enum")
        .trim_matches('"')
        .to_owned()
}
// ts: mapSide
pub fn map_side(side: WooV3OrderSide) -> OrderSide {
    match side {
        WooV3OrderSide::Buy => OrderSide::Buy,
        WooV3OrderSide::Sell => OrderSide::Sell,
    }
}
// ts: mapPendingStatus
pub fn map_pending_status(status: WooV3OrderStatus, path: &str) -> Result<OrderStatus, String> {
    if !matches!(
        status,
        WooV3OrderStatus::New | WooV3OrderStatus::PartialFilled
    ) {
        return Err(format!(
            "{path}={} is not an incomplete/pending order",
            enum_text(&status)
        ));
    }
    Ok(OrderStatus::Open)
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WooV3RegularOrderMappingType {
    pub r#type: OrderType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<OrderTimeInForce>,
}
// ts: mapRegularOrderType
pub fn map_regular_order_type(
    kind: WooV3RegularOrderType,
    path: &str,
) -> Result<WooV3RegularOrderMappingType, String> {
    let (r#type, time_in_force) = match kind {
        WooV3RegularOrderType::Limit => (OrderType::Limit, Some(OrderTimeInForce::GoodTillCancel)),
        WooV3RegularOrderType::Market => (OrderType::Market, None),
        WooV3RegularOrderType::PostOnly => (OrderType::Limit, Some(OrderTimeInForce::PostOnly)),
        WooV3RegularOrderType::Ioc => (OrderType::Limit, Some(OrderTimeInForce::ImmediateOrCancel)),
        WooV3RegularOrderType::Fok => (OrderType::Limit, Some(OrderTimeInForce::FillOrKill)),
        _ => {
            return Err(format!(
                "{path}={} has no lossless safe-cex order mapping",
                enum_text(&kind)
            ));
        }
    };
    Ok(WooV3RegularOrderMappingType {
        r#type,
        time_in_force,
    })
}
// ts: mapTriggerPriceType
pub fn map_trigger_price_type(kind: WooV3TriggerPriceType) -> OrderStopTriggerType {
    if kind == WooV3TriggerPriceType::IndexPrice {
        OrderStopTriggerType::IndexPrice
    } else {
        OrderStopTriggerType::LastPrice
    }
}
// ts: decodeRegularOrder
pub fn decode_regular_order(
    input: Option<&JsNumberInput>,
    path: &str,
) -> Result<WooV3RegularOrder, String> {
    let row = require_record(input, path)?;
    let amount = if let Some(v) = row.get("amount") {
        if v.clone().decode::<()>().is_ok() {
            WooV3OrderAmount::Null
        } else if v.clone().decode::<String>().ok().as_deref() == Some("null") {
            WooV3OrderAmount::Text("null".into())
        } else {
            WooV3OrderAmount::Text(require_decimal_string(Some(v), &format!("{path}.amount"))?)
        }
    } else {
        return Err(format!("{path}.amount must be a finite decimal string"));
    };
    let realized_pnl =
        optional_decimal_string(row.get("realizedPnl"), &format!("{path}.realizedPnl"))?;
    let margin_mode = row
        .get("marginMode")
        .map(|v| {
            enum_field(
                Some(v),
                &["ISOLATED", "CROSSED"],
                &format!("{path}.marginMode"),
            )
        })
        .transpose()?;
    Ok(WooV3RegularOrder {
        symbol: require_string(row.get("symbol"), &format!("{path}.symbol"), false)?,
        status: enum_field(
            row.get("status"),
            &["NEW", "CANCELLED", "PARTIAL_FILLED", "FILLED", "REJECTED"],
            &format!("{path}.status"),
        )?,
        side: enum_field(row.get("side"), &["BUY", "SELL"], &format!("{path}.side"))?,
        position_side: enum_field(
            row.get("positionSide"),
            &["BOTH", "LONG", "SHORT"],
            &format!("{path}.positionSide"),
        )?,
        created_time: require_integer(row.get("createdTime"), &format!("{path}.createdTime"), 0.0)?,
        order_id: require_integer(row.get("orderId"), &format!("{path}.orderId"), 1.0)?,
        order_tag: require_string(row.get("orderTag"), &format!("{path}.orderTag"), false)?,
        price: require_decimal_string(row.get("price"), &format!("{path}.price"))?,
        r#type: enum_field(
            row.get("type"),
            &[
                "LIMIT",
                "MARKET",
                "IOC",
                "POST_ONLY",
                "AC",
                "FOK",
                "LIQUIDATE",
                "BID",
                "ASK",
                "RPI",
                "LIQUIDATE_BLP",
                "ADL",
            ],
            &format!("{path}.type"),
        )?,
        quantity: require_decimal_string(row.get("quantity"), &format!("{path}.quantity"))?,
        amount,
        visible: require_decimal_string(row.get("visible"), &format!("{path}.visible"))?,
        executed: require_decimal_string(row.get("executed"), &format!("{path}.executed"))?,
        total_fee: require_decimal_string(row.get("totalFee"), &format!("{path}.totalFee"))?,
        fee_asset: require_string(row.get("feeAsset"), &format!("{path}.feeAsset"), true)?,
        total_rebate: require_finite_number(
            row.get("totalRebate"),
            &format!("{path}.totalRebate"),
        )?,
        rebate_currency: require_string(
            row.get("rebateCurrency"),
            &format!("{path}.rebateCurrency"),
            true,
        )?,
        client_order_id: require_integer(
            row.get("clientOrderId"),
            &format!("{path}.clientOrderId"),
            0.0,
        )?,
        reduce_only: require_boolean(row.get("reduceOnly"), &format!("{path}.reduceOnly"))?,
        realized_pnl,
        average_executed_price: require_decimal_string(
            row.get("averageExecutedPrice"),
            &format!("{path}.averageExecutedPrice"),
        )?,
        leverage: row
            .get("leverage")
            .map(|v| require_integer(Some(v), &format!("{path}.leverage"), 1.0))
            .transpose()?,
        margin_mode,
        bid_ask_level: row
            .get("bidAskLevel")
            .map(|v| require_integer(Some(v), &format!("{path}.bidAskLevel"), 0.0))
            .transpose()?,
    })
}
// ts: decodeAlgoQuantity
pub fn decode_algo_quantity(
    value: Option<&JsNumberInput>,
    path: &str,
) -> Result<WooV3GatewayAlgoQuantity, String> {
    if value.is_some_and(|v| v.clone().decode::<String>().is_ok()) {
        return require_decimal_string(value, path).map(WooV3GatewayAlgoQuantity::Decimal);
    }
    let number = require_finite_number(value, path)?;
    if number < 0.0 {
        return Err(format!("{path} must not be negative"));
    }
    Ok(WooV3GatewayAlgoQuantity::Number(number))
}
// ts: decodeAlgoOrder
pub fn decode_algo_order(
    input: Option<&JsNumberInput>,
    path: &str,
    depth: usize,
) -> Result<WooV3AlgoOrder, String> {
    if depth > 8 {
        return Err(format!(
            "{path} exceeds the supported child-order nesting depth"
        ));
    }
    let row = require_record(input, path)?;
    let kind = row
        .get("type")
        .map(|v| {
            enum_field(
                Some(v),
                &[
                    "LIMIT",
                    "MARKET",
                    "IOC",
                    "POST_ONLY",
                    "AC",
                    "FOK",
                    "LIQUIDATE",
                    "BID",
                    "ASK",
                    "RPI",
                ],
                &format!("{path}.type"),
            )
        })
        .transpose()?;
    let margin_mode = row
        .get("marginMode")
        .map(|v| {
            enum_field(
                Some(v),
                &["ISOLATED", "CROSSED"],
                &format!("{path}.marginMode"),
            )
        })
        .transpose()?;
    let children = if let Some(value) = row.get("childOrders") {
        require_array(Some(value), &format!("{path}.childOrders"))?
            .iter()
            .enumerate()
            .map(|(i, child)| {
                decode_algo_order(Some(child), &format!("{path}.childOrders[{i}]"), depth + 1)
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    Ok(WooV3AlgoOrder {
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
        root_algo_order_id: require_integer(
            row.get("rootAlgoOrderId"),
            &format!("{path}.rootAlgoOrderId"),
            1.0,
        )?,
        parent_algo_order_id: require_integer(
            row.get("parentAlgoOrderId"),
            &format!("{path}.parentAlgoOrderId"),
            0.0,
        )?,
        symbol: require_string(row.get("symbol"), &format!("{path}.symbol"), false)?,
        algo_order_tag: require_string(
            row.get("algoOrderTag"),
            &format!("{path}.algoOrderTag"),
            false,
        )?,
        algo_type: enum_field(
            row.get("algoType"),
            &[
                "STOP",
                "OCO",
                "TRAILING_STOP",
                "POSITIONAL_TP_SL",
                "TP_SL",
                "BRACKET",
                "STOP_BRACKET",
                "TAKE_PROFIT",
                "STOP_LOSS",
            ],
            &format!("{path}.algoType"),
        )?,
        side: enum_field(row.get("side"), &["BUY", "SELL"], &format!("{path}.side"))?,
        position_side: enum_field(
            row.get("positionSide"),
            &["BOTH", "LONG", "SHORT"],
            &format!("{path}.positionSide"),
        )?,
        quantity: decode_algo_quantity(row.get("quantity"), &format!("{path}.quantity"))?,
        is_triggered: require_boolean(row.get("isTriggered"), &format!("{path}.isTriggered"))?,
        trigger_status: enum_field(
            row.get("triggerStatus"),
            &["USELESS", "TRIGGERED", "CANCELLED", "SUCCESS"],
            &format!("{path}.triggerStatus"),
        )?,
        r#type: kind,
        root_algo_status: enum_field(
            row.get("rootAlgoStatus"),
            &["NEW", "CANCELLED", "PARTIAL_FILLED", "FILLED", "REJECTED"],
            &format!("{path}.rootAlgoStatus"),
        )?,
        algo_status: enum_field(
            row.get("algoStatus"),
            &["NEW", "CANCELLED", "PARTIAL_FILLED", "FILLED", "REJECTED"],
            &format!("{path}.algoStatus"),
        )?,
        trigger_price_type: enum_field(
            row.get("triggerPriceType"),
            &["MARKET_PRICE", "INDEX_PRICE", "LAST_PRICE"],
            &format!("{path}.triggerPriceType"),
        )?,
        trigger_time: require_decimal_string(
            row.get("triggerTime"),
            &format!("{path}.triggerTime"),
        )?,
        trigger_price: row
            .get("triggerPrice")
            .map(|v| require_decimal_string(Some(v), &format!("{path}.triggerPrice")))
            .transpose()?,
        price: row
            .get("price")
            .map(|v| require_decimal_string(Some(v), &format!("{path}.price")))
            .transpose()?,
        visible_quantity: row
            .get("visibleQuantity")
            .map(|v| require_decimal_string(Some(v), &format!("{path}.visibleQuantity")))
            .transpose()?,
        total_executed_quantity: require_decimal_string(
            row.get("totalExecutedQuantity"),
            &format!("{path}.totalExecutedQuantity"),
        )?,
        average_executed_price: require_decimal_string(
            row.get("averageExecutedPrice"),
            &format!("{path}.averageExecutedPrice"),
        )?,
        total_fee: require_decimal_string(row.get("totalFee"), &format!("{path}.totalFee"))?,
        fee_asset: require_string(row.get("feeAsset"), &format!("{path}.feeAsset"), true)?,
        total_rebate: require_finite_number(
            row.get("totalRebate"),
            &format!("{path}.totalRebate"),
        )?,
        rebate_currency: require_string(
            row.get("rebateCurrency"),
            &format!("{path}.rebateCurrency"),
            true,
        )?,
        reduce_only: require_boolean(row.get("reduceOnly"), &format!("{path}.reduceOnly"))?,
        created_time: require_integer(row.get("createdTime"), &format!("{path}.createdTime"), 0.0)?,
        updated_time: require_integer(row.get("updatedTime"), &format!("{path}.updatedTime"), 0.0)?,
        is_activated: require_boolean(row.get("isActivated"), &format!("{path}.isActivated"))?,
        callback_rate: row
            .get("callbackRate")
            .map(|v| require_decimal_string(Some(v), &format!("{path}.callbackRate")))
            .transpose()?,
        activated_price: row
            .get("activatedPrice")
            .map(|v| require_decimal_string(Some(v), &format!("{path}.activatedPrice")))
            .transpose()?,
        leverage: row
            .get("leverage")
            .map(|v| require_integer(Some(v), &format!("{path}.leverage"), 1.0))
            .transpose()?,
        margin_mode,
        child_orders: children,
    })
}
// ts: decodeTransaction
pub fn decode_transaction(
    input: Option<&JsNumberInput>,
    path: &str,
) -> Result<WooV3Transaction, String> {
    let row = require_record(input, path)?;
    let is_maker = require_integer(row.get("isMaker"), &format!("{path}.isMaker"), 0.0)?;
    if is_maker != 0.0 && is_maker != 1.0 {
        return Err(format!("{path}.isMaker must be 0 or 1"));
    }
    let realized_pnl = if row
        .get("realizedPnl")
        .is_some_and(|v| v.clone().decode::<()>().is_ok())
    {
        None
    } else {
        Some(require_decimal_string(
            row.get("realizedPnl"),
            &format!("{path}.realizedPnl"),
        )?)
    };
    Ok(WooV3Transaction {
        id: require_integer(row.get("id"), &format!("{path}.id"), 1.0)?,
        symbol: require_string(row.get("symbol"), &format!("{path}.symbol"), false)?,
        order_id: require_integer(row.get("orderId"), &format!("{path}.orderId"), 1.0)?,
        order_tag: require_string(row.get("orderTag"), &format!("{path}.orderTag"), false)?,
        executed_price: require_decimal_string(
            row.get("executedPrice"),
            &format!("{path}.executedPrice"),
        )?,
        executed_quantity: require_decimal_string(
            row.get("executedQuantity"),
            &format!("{path}.executedQuantity"),
        )?,
        is_maker,
        is_match_rpi: require_boolean(row.get("isMatchRpi"), &format!("{path}.isMatchRpi"))?,
        side: enum_field(row.get("side"), &["BUY", "SELL"], &format!("{path}.side"))?,
        fee: require_finite_number(row.get("fee"), &format!("{path}.fee"))?,
        fee_asset: require_string(row.get("feeAsset"), &format!("{path}.feeAsset"), false)?,
        realized_pnl,
        executed_timestamp: require_integer(
            row.get("executedTimestamp"),
            &format!("{path}.executedTimestamp"),
            0.0,
        )?,
    })
}
// ts: decodeWooV3PendingOrders
pub fn decode_woo_v3_pending_orders(
    input: Option<&JsNumberInput>,
) -> Result<WooV3PaginatedRows<WooV3RegularOrder>, String> {
    decode_paginated_rows(input, "pending orders", |v, p| {
        decode_regular_order(Some(v), p)
    })
}
// ts: decodeWooV3PendingAlgoOrders
pub fn decode_woo_v3_pending_algo_orders(
    input: Option<&JsNumberInput>,
) -> Result<WooV3PaginatedRows<WooV3AlgoOrder>, String> {
    decode_paginated_rows(input, "pending algo orders", |v, p| {
        decode_algo_order(Some(v), p, 0)
    })
}
// ts: decodeWooV3Transactions
pub fn decode_woo_v3_transactions(
    input: Option<&JsNumberInput>,
) -> Result<WooV3PaginatedRows<WooV3Transaction>, String> {
    decode_paginated_rows(input, "transaction history", |v, p| {
        decode_transaction(Some(v), p)
    })
}
// ts: mapWooV3PendingOrders
pub fn map_woo_v3_pending_orders(
    rows: &[WooV3RegularOrder],
    account_position_mode: WooV3AccountPositionMode,
) -> Result<Vec<WooV3MappedOrder<WooV3RegularOrder>>, String> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let path = format!("WOO V3 pending order[{i}]");
            assert_position_side_matches_mode(
                row.position_side,
                account_position_mode,
                &format!("{path}.positionSide"),
            )?;
            let mapped_type = map_regular_order_type(row.r#type, &format!("{path}.type"))?;
            let amount = decimal_to_number(
                &row.quantity,
                &format!("{path}.quantity"),
                Some(f64::from_bits(1)),
            )?;
            let filled = decimal_to_number(&row.executed, &format!("{path}.executed"), Some(0.0))?;
            let remaining = safe_cex_num::subtract(amount, filled);
            if remaining < 0.0 {
                return Err(format!("{path}.executed must not exceed quantity"));
            }
            Ok(WooV3MappedOrder {
                id: safe_cex_num::js_number_string(row.order_id),
                parent_id: None,
                status: map_pending_status(row.status, &format!("{path}.status"))?,
                symbol: normalize_perpetual_symbol(&row.symbol, &format!("{path}.symbol"))?,
                r#type: mapped_type.r#type,
                side: map_side(row.side),
                price: decimal_to_number(&row.price, &format!("{path}.price"), Some(0.0))?,
                amount,
                filled,
                remaining,
                reduce_only: row.reduce_only,
                time_in_force: mapped_type.time_in_force,
                tp_trigger_type: None,
                sl_trigger_type: None,
                last_update: row.created_time,
                info: row.clone(),
            })
        })
        .collect()
}
fn visit_algo(
    row: &WooV3AlgoOrder,
    root_id: f64,
    path: &str,
    mode: WooV3AccountPositionMode,
    orders: &mut Vec<WooV3MappedOrder<WooV3AlgoOrder>>,
    deferred: &mut Vec<WooV3DeferredAlgoOrder>,
) -> Result<(), String> {
    assert_position_side_matches_mode(row.position_side, mode, &format!("{path}.positionSide"))?;
    map_pending_status(row.algo_status, &format!("{path}.algoStatus"))?;
    if !row.child_orders.is_empty() {
        for (i, child) in row.child_orders.iter().enumerate() {
            visit_algo(
                child,
                root_id,
                &format!("{path}.childOrders[{i}]"),
                mode,
                orders,
                deferred,
            )?;
        }
        return Ok(());
    }
    let reason = if !matches!(
        row.algo_type,
        WooV3AlgoType::TakeProfit | WooV3AlgoType::StopLoss
    ) {
        Some(format!(
            "{} has no lossless absolute-trigger safe-cex mapping",
            enum_text(&row.algo_type)
        ))
    } else if matches!(row.quantity, WooV3GatewayAlgoQuantity::Number(_)) {
        Some("official algo quantity number/string drift requires runtime evidence".into())
    } else if row.trigger_price.is_none() {
        Some(format!(
            "{} is missing an absolute triggerPrice",
            enum_text(&row.algo_type)
        ))
    } else {
        None
    };
    if let Some(reason) = reason {
        deferred.push(WooV3DeferredAlgoOrder {
            order: row.clone(),
            reason,
        });
        return Ok(());
    }
    let WooV3GatewayAlgoQuantity::Decimal(quantity) = &row.quantity else {
        unreachable!()
    };
    let amount = decimal_to_number(
        quantity,
        &format!("{path}.quantity"),
        Some(f64::from_bits(1)),
    )?;
    let filled = decimal_to_number(
        &row.total_executed_quantity,
        &format!("{path}.totalExecutedQuantity"),
        Some(0.0),
    )?;
    let remaining = safe_cex_num::subtract(amount, filled);
    if remaining < 0.0 {
        return Err(format!(
            "{path}.totalExecutedQuantity must not exceed quantity"
        ));
    }
    let is_stop_loss = row.algo_type == WooV3AlgoType::StopLoss;
    let trigger_type = map_trigger_price_type(row.trigger_price_type);
    orders.push(WooV3MappedOrder {
        id: safe_cex_num::js_number_string(row.algo_order_id),
        parent_id: Some(safe_cex_num::js_number_string(root_id)),
        status: OrderStatus::Open,
        symbol: normalize_perpetual_symbol(&row.symbol, &format!("{path}.symbol"))?,
        r#type: if is_stop_loss {
            OrderType::StopLoss
        } else {
            OrderType::TakeProfit
        },
        side: map_side(row.side),
        price: decimal_to_number(
            row.trigger_price.as_ref().unwrap(),
            &format!("{path}.triggerPrice"),
            Some(f64::from_bits(1)),
        )?,
        amount,
        filled,
        remaining,
        reduce_only: row.reduce_only,
        time_in_force: None,
        tp_trigger_type: (!is_stop_loss).then_some(trigger_type),
        sl_trigger_type: is_stop_loss.then_some(trigger_type),
        last_update: row.updated_time,
        info: row.clone(),
    });
    Ok(())
}
// ts: mapWooV3PendingAlgoOrders
pub fn map_woo_v3_pending_algo_orders(
    rows: &[WooV3AlgoOrder],
    account_position_mode: WooV3AccountPositionMode,
) -> Result<WooV3AlgoOrderMapping, String> {
    let mut orders = Vec::new();
    let mut deferred = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        visit_algo(
            row,
            row.root_algo_order_id,
            &format!("WOO V3 pending algo order[{i}]"),
            account_position_mode,
            &mut orders,
            &mut deferred,
        )?;
    }
    Ok(WooV3AlgoOrderMapping { orders, deferred })
}
// ts: mapWooV3Transactions
pub fn map_woo_v3_transactions(
    rows: &[WooV3Transaction],
    account_position_mode: WooV3AccountPositionMode,
) -> Result<Vec<WooV3MappedTransaction>, String> {
    rows.iter().enumerate().map(|(i,row)|{
 let path=format!("WOO V3 transaction[{i}]");let symbol=normalize_perpetual_symbol(&row.symbol,&format!("{path}.symbol"))?;let side=map_side(row.side);
 let price=decimal_to_number(&row.executed_price,&format!("{path}.executedPrice"),Some(f64::from_bits(1)))?;
 let qty=decimal_to_number(&row.executed_quantity,&format!("{path}.executedQuantity"),Some(f64::from_bits(1)))?;
 let realized_pnl=row.realized_pnl.as_ref().map(|v|decimal_to_number(v,&format!("{path}.realizedPnl"),None)).transpose()?;
 let historical_trade=(account_position_mode==WooV3AccountPositionMode::OneWay).then(||HistoricalTrade {id:Some(safe_cex_num::js_number_string(row.id)),symbol:symbol.clone(),order_id:safe_cex_num::js_number_string(row.order_id),price,qty,quote_qty:None,realized_pnl,position_side:ExecutionPositionSide::Both(BothPositionSide::Both),side,buyer:None,maker:Some(row.is_maker==1.0),commission:Some(row.fee),commission_asset:Some(row.fee_asset.clone()),time:row.executed_timestamp});
 let deferred_historical_trade_reason=historical_trade.is_none().then(||"WOO V3 transaction history omits positionSide required by hedge-mode HistoricalTrade".into());
 Ok(WooV3MappedTransaction {execution:Execution {id:Some(safe_cex_num::js_number_string(row.id)),timestamp:(row.executed_timestamp/1000.0).floor(),symbol,order_id:Some(safe_cex_num::js_number_string(row.order_id)),price,qty,side,commission:Some(row.fee),commission_asset:Some(row.fee_asset.clone()),position_side:None,realized_pnl:None,maker:None,venue_reported_revenue:None,venue_reported_revenue_asset:None},historical_trade,deferred_historical_trade_reason,maker:row.is_maker==1.0,is_match_rpi:row.is_match_rpi})
 }).collect()
}
