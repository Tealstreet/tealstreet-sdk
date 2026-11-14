//! Canonical SDK Order, placement and amendment contracts.
//! ```compile_fail
//! use safe_cex_types::order::strict_codecs::WireOrder;
//! ```
use crate::{ModelField, deserialize_present};
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

mod strict_codecs {
    use super::*;

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderStatus")]
    enum WireOrderStatus {
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

    impl<'de> serde::Deserialize<'de> for super::OrderStatus {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderStatus::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderType")]
    enum WireOrderType {
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

    impl<'de> serde::Deserialize<'de> for super::OrderType {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderType::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderSide")]
    enum WireOrderSide {
        #[serde(rename = "buy")]
        Buy,
        #[serde(rename = "sell")]
        Sell,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderSide {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderSide::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderTimeInForce")]
    enum WireOrderTimeInForce {
        #[serde(rename = "GoodTillCancel")]
        GoodTillCancel,
        #[serde(rename = "ImmediateOrCancel")]
        ImmediateOrCancel,
        #[serde(rename = "FillOrKill")]
        FillOrKill,
        #[serde(rename = "PostOnly")]
        PostOnly,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderTimeInForce {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderTimeInForce::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderStopTriggerType")]
    // Remote variants must retain the canonical public variant names.
    #[allow(clippy::enum_variant_names)]
    enum WireOrderStopTriggerType {
        #[serde(rename = "mark_price")]
        MarkPrice,
        #[serde(rename = "last_price")]
        LastPrice,
        #[serde(rename = "index_price")]
        IndexPrice,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderStopTriggerType {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderStopTriggerType::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderRejectReason")]
    enum WireOrderRejectReason {
        #[serde(rename = "PostOnly")]
        PostOnly,
        #[serde(rename = "InsufficientBalance")]
        InsufficientBalance,
        #[serde(rename = "ReduceOnly")]
        ReduceOnly,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderRejectReason {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderRejectReason::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderExtra")]
    #[serde(rename_all = "camelCase")]
    struct WireOrderExtra {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        remaining_native: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        filled_in_placement_units: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        remaining_notional: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        conditional_order_price: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        reject_reason: Option<OrderRejectReason>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        is_from_chaser: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        chaser_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        chaser_attempts: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        chaser_max_attempts: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderExtra {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOrderExtra::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::Order")]
    #[serde(rename_all = "camelCase")]
    struct WireOrder {
        id: String,
        #[serde(default)]
        parent_id: ModelField<String>,
        #[serde(default)]
        status: ModelField<OrderStatus>,
        symbol: String,
        #[serde(default)]
        r#type: ModelField<OrderType>,
        #[serde(default)]
        side: ModelField<OrderSide>,
        #[serde(default)]
        price: ModelField<f64>,
        amount: f64,
        filled: f64,
        remaining: f64,
        #[serde(default)]
        reduce_only: ModelField<bool>,
        #[serde(default)]
        close: ModelField<bool>,
        #[serde(default, deserialize_with = "crate::model_field::deserialize_info")]
        info: ModelField<crate::retained_json::RetainedJsonInput>,
        #[serde(default)]
        client_order_id: ModelField<String>,
        #[serde(default)]
        time_in_force: ModelField<OrderTimeInForce>,
        last_update: f64,
        #[serde(default)]
        trigger_type: ModelField<OrderStopTriggerType>,
        #[serde(default)]
        tp_trigger_type: ModelField<OrderStopTriggerType>,
        #[serde(default)]
        sl_trigger_type: ModelField<OrderStopTriggerType>,
        #[serde(default)]
        take_profit: ModelField<f64>,
        #[serde(default)]
        stop_loss: ModelField<f64>,
        #[serde(default)]
        extra: ModelField<OrderExtra>,
    }

    impl<'de> serde::Deserialize<'de> for super::Order {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct RecordVisitor;
            impl<'de> serde::de::Visitor<'de> for RecordVisitor {
                type Value = super::Order;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a model record")
                }
                fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                    self,
                    decoder: D,
                ) -> Result<Self::Value, D::Error> {
                    WireOrder::deserialize(crate::strict_decode::object(decoder))
                }
                // Map-forwarding decoders (flatten/MapAccessDeserializer) skip the hint.
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    map: A,
                ) -> Result<Self::Value, A::Error> {
                    WireOrder::deserialize(crate::strict_decode::object(
                        serde::de::value::MapAccessDeserializer::new(map),
                    ))
                }
            }
            deserializer.deserialize_newtype_struct(crate::model_field::MODEL_RECORD, RecordVisitor)
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PlaceOrderOpts")]
    #[serde(rename_all = "camelCase")]
    struct WirePlaceOrderOpts {
        symbol: String,
        r#type: OrderType,
        side: OrderSide,
        amount: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        price: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        trigger_price: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        stop_loss: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        take_profit: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        reduce_only: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        time_in_force: Option<OrderTimeInForce>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        trigger_type: Option<OrderStopTriggerType>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        tp_trigger_type: Option<OrderStopTriggerType>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        sl_trigger_type: Option<OrderStopTriggerType>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        client_order_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        max_slippage_pct: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        close: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        implied_volatility: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::PlaceOrderOpts {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WirePlaceOrderOpts::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::UpdateOrderOptsUpdateVariant0")]
    struct WireUpdateOrderOptsUpdateVariant0 {
        #[serde(rename = "amount")]
        amount: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        price: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::UpdateOrderOptsUpdateVariant0 {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireUpdateOrderOptsUpdateVariant0::deserialize(crate::strict_decode::object(
                deserializer,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::UpdateOrderOptsUpdateVariant1")]
    struct WireUpdateOrderOptsUpdateVariant1 {
        #[serde(rename = "price")]
        price: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        amount: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::UpdateOrderOptsUpdateVariant1 {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireUpdateOrderOptsUpdateVariant1::deserialize(crate::strict_decode::object(
                deserializer,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::UpdateOrderOpts")]
    struct WireUpdateOrderOpts {
        #[serde(rename = "order")]
        order: Order,
        #[serde(rename = "update")]
        update: UpdateOrderOptsUpdate,
    }

    impl<'de> serde::Deserialize<'de> for super::UpdateOrderOpts {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireUpdateOrderOpts::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[cfg(test)]
    mod shape_tests {
        use super::*;

        #[test]
        fn every_order_codec_has_a_narrow_input_shape() {
            crate::strict_decode::tests::string_union::<OrderStatus>(
                "OrderStatus",
                WireOrderStatus::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderStatus::Open)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStatus::Closed)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStatus::Canceled)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStatus::Creating)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStatus::Updating)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStatus::Canceling)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<OrderType>(
                "OrderType",
                WireOrderType::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderType::Market)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderType::Limit)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderType::Conditional)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderType::StopLoss)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderType::TakeProfit)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderType::TrailingStopLoss)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<OrderSide>(
                "OrderSide",
                WireOrderSide::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderSide::Buy)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderSide::Sell)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<OrderTimeInForce>(
                "OrderTimeInForce",
                WireOrderTimeInForce::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderTimeInForce::GoodTillCancel)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderTimeInForce::ImmediateOrCancel)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderTimeInForce::FillOrKill)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderTimeInForce::PostOnly)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<OrderStopTriggerType>(
                "OrderStopTriggerType",
                WireOrderStopTriggerType::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderStopTriggerType::MarkPrice)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStopTriggerType::LastPrice)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStopTriggerType::IndexPrice)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<OrderRejectReason>(
                "OrderRejectReason",
                WireOrderRejectReason::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderRejectReason::PostOnly)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderRejectReason::InsufficientBalance)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderRejectReason::ReduceOnly)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::record::<OrderExtra>("OrderExtra");
            crate::strict_decode::tests::record::<Order>("Order");
            crate::strict_decode::tests::record::<PlaceOrderOpts>("PlaceOrderOpts");
            crate::strict_decode::tests::record::<UpdateOrderOptsUpdateVariant0>(
                "UpdateOrderOptsUpdateVariant0",
            );
            crate::strict_decode::tests::record::<UpdateOrderOptsUpdateVariant1>(
                "UpdateOrderOptsUpdateVariant1",
            );
            crate::strict_decode::tests::record::<UpdateOrderOpts>("UpdateOrderOpts");
        }
    }
}
