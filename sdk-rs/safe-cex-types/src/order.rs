//! Canonical SDK Order, placement and amendment contracts.
use crate::ModelField;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderStatus {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "canceled")]
    Canceled,
    #[serde(rename = "creating")]
    Creating,
    #[serde(rename = "updating")]
    Updating,
    #[serde(rename = "canceling")]
    Canceling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderType {
    #[serde(rename = "market")]
    Market,
    #[serde(rename = "limit")]
    Limit,
    #[serde(rename = "conditional")]
    Conditional,
    #[serde(rename = "stop_market")]
    StopLoss,
    #[serde(rename = "take_profit_market")]
    TakeProfit,
    #[serde(rename = "trailing_stop_market")]
    TrailingStopLoss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderSide {
    #[serde(rename = "buy")]
    Buy,
    #[serde(rename = "sell")]
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderTimeInForce {
    #[serde(rename = "GoodTillCancel")]
    GoodTillCancel,
    #[serde(rename = "ImmediateOrCancel")]
    ImmediateOrCancel,
    #[serde(rename = "FillOrKill")]
    FillOrKill,
    #[serde(rename = "PostOnly")]
    PostOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderStopTriggerType {
    #[serde(rename = "mark_price")]
    MarkPrice,
    #[serde(rename = "last_price")]
    LastPrice,
    #[serde(rename = "index_price")]
    IndexPrice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderRejectReason {
    #[serde(rename = "PostOnly")]
    PostOnly,
    #[serde(rename = "InsufficientBalance")]
    InsufficientBalance,
    #[serde(rename = "ReduceOnly")]
    ReduceOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OrderExtra {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub remaining_native: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub filled_in_placement_units: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub remaining_notional: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub conditional_order_price: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub reject_reason: Option<OrderRejectReason>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_from_chaser: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub chaser_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub chaser_attempts: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub chaser_max_attempts: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, TS)]
#[ts(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct Order {
    pub id: String,
    #[ts(as = "Option<String>")]
    pub parent_id: ModelField<String>,
    #[ts(as = "OrderStatus")]
    pub status: ModelField<OrderStatus>,
    pub symbol: String,
    #[ts(as = "OrderType")]
    pub r#type: ModelField<OrderType>,
    #[ts(as = "OrderSide")]
    pub side: ModelField<OrderSide>,
    #[ts(as = "f64")]
    pub price: ModelField<f64>,
    pub amount: f64,
    pub filled: f64,
    pub remaining: f64,
    #[ts(as = "bool")]
    pub reduce_only: ModelField<bool>,
    #[ts(as = "Option<bool>")]
    pub close: ModelField<bool>,
    #[ts(type = "any", optional)]
    pub info: ModelField<crate::retained_json::RetainedJsonInput>,
    // The venue client order ID where the adapter maps it to the top level
    // (Hyperliquid's cloid): `info` is stripped before a worker posts the store.
    #[ts(as = "Option<String>")]
    pub client_order_id: ModelField<String>,
    #[ts(as = "Option<OrderTimeInForce>")]
    pub time_in_force: ModelField<OrderTimeInForce>,
    pub last_update: f64,
    #[ts(as = "Option<OrderStopTriggerType>")]
    pub trigger_type: ModelField<OrderStopTriggerType>,
    #[ts(as = "Option<OrderStopTriggerType>")]
    pub tp_trigger_type: ModelField<OrderStopTriggerType>,
    #[ts(as = "Option<OrderStopTriggerType>")]
    pub sl_trigger_type: ModelField<OrderStopTriggerType>,
    #[ts(as = "Option<f64>")]
    pub take_profit: ModelField<f64>,
    #[ts(as = "Option<f64>")]
    pub stop_loss: ModelField<f64>,
    #[ts(inline)]
    #[ts(as = "Option<OrderExtra>")]
    pub extra: ModelField<OrderExtra>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct PlaceOrderOpts {
    pub symbol: String,
    pub r#type: OrderType,
    pub side: OrderSide,
    pub amount: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub price: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub trigger_price: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub stop_loss: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub take_profit: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub reduce_only: Option<bool>,
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
    pub trigger_type: Option<OrderStopTriggerType>,
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
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub client_order_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_slippage_pct: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub close: Option<bool>,
    /// Price an option order by implied volatility, as a fraction. Option markets only.
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub implied_volatility: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct UpdateOrderOptsUpdateVariant0 {
    #[serde(rename = "amount")]
    pub amount: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(skip)]
    pub price: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct UpdateOrderOptsUpdateVariant1 {
    #[serde(rename = "price")]
    pub price: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(skip)]
    pub amount: Option<f64>,
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum UpdateOrderOptsUpdate {
    Variant0(#[ts(inline)] UpdateOrderOptsUpdateVariant0),
    Variant1(#[ts(inline)] UpdateOrderOptsUpdateVariant1),
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct UpdateOrderOpts {
    #[serde(rename = "order")]
    pub order: Order,
    #[serde(rename = "update")]
    #[ts(inline)]
    pub update: UpdateOrderOptsUpdate,
}

impl serde::Serialize for Order {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct(crate::model_field::MODEL_RECORD, 22)?;
        state.serialize_field("id", &self.id)?;
        self.parent_id.serialize_field(&mut state, "parentId")?;
        self.status.serialize_field(&mut state, "status")?;
        state.serialize_field("symbol", &self.symbol)?;
        self.r#type.serialize_field(&mut state, "type")?;
        self.side.serialize_field(&mut state, "side")?;
        self.price.serialize_field(&mut state, "price")?;
        state.serialize_field("amount", &self.amount)?;
        state.serialize_field("filled", &self.filled)?;
        state.serialize_field("remaining", &self.remaining)?;
        self.reduce_only.serialize_field(&mut state, "reduceOnly")?;
        self.close.serialize_field(&mut state, "close")?;
        match &self.info {
            ModelField::Absent => {}
            ModelField::Undefined
            | ModelField::Value(crate::retained_json::RetainedJsonInput::Undefined) => {
                state.skip_field("info")?
            }
            ModelField::Value(value) => state.serialize_field("info", value)?,
        }
        self.client_order_id
            .serialize_field(&mut state, "clientOrderId")?;
        self.time_in_force
            .serialize_field(&mut state, "timeInForce")?;
        state.serialize_field("lastUpdate", &self.last_update)?;
        self.trigger_type
            .serialize_field(&mut state, "triggerType")?;
        self.tp_trigger_type
            .serialize_field(&mut state, "tpTriggerType")?;
        self.sl_trigger_type
            .serialize_field(&mut state, "slTriggerType")?;
        self.take_profit.serialize_field(&mut state, "takeProfit")?;
        self.stop_loss.serialize_field(&mut state, "stopLoss")?;
        self.extra.serialize_field(&mut state, "extra")?;
        state.end()
    }
}
