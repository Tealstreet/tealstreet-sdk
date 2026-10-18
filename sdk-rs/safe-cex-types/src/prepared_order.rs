//! Internal venue input after public validation or an Order replacement copy.
//! This carrier is not a second public placement contract: source amendments
//! can copy undefined type/side/reduceOnly, while PlaceOrderOpts retains strict inputs.
use crate::{
    ModelField, OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PlaceOrderOpts,
};
use serde::{Serialize, Serializer, ser::SerializeStruct};

/// Symbol/ID and amount shapes retain existing native venue interfaces (including
/// UTF-16 symbols and a property-present optional amendment amount).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PreparedOrderOpts<S = String, A = f64, R = ModelField<bool>> {
    pub symbol: S,
    pub r#type: ModelField<OrderType>,
    pub side: ModelField<OrderSide>,
    pub amount: A,
    pub price: Option<f64>,
    pub trigger_price: Option<f64>,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub reduce_only: R,
    pub time_in_force: Option<OrderTimeInForce>,
    pub trigger_type: Option<OrderStopTriggerType>,
    pub tp_trigger_type: Option<OrderStopTriggerType>,
    pub sl_trigger_type: Option<OrderStopTriggerType>,
    pub client_order_id: Option<S>,
    pub max_slippage_pct: Option<f64>,
    pub close: Option<bool>,
    pub implied_volatility: Option<f64>,
}

impl From<&PlaceOrderOpts> for PreparedOrderOpts {
    fn from(opts: &PlaceOrderOpts) -> Self {
        Self {
            symbol: opts.symbol.clone(),
            r#type: opts.r#type.into(),
            side: opts.side.into(),
            amount: opts.amount,
            price: opts.price,
            trigger_price: opts.trigger_price,
            stop_loss: opts.stop_loss,
            take_profit: opts.take_profit,
            reduce_only: opts.reduce_only.into(),
            time_in_force: opts.time_in_force,
            trigger_type: opts.trigger_type,
            tp_trigger_type: opts.tp_trigger_type,
            sl_trigger_type: opts.sl_trigger_type,
            client_order_id: opts.client_order_id.clone(),
            max_slippage_pct: opts.max_slippage_pct,
            close: opts.close,
            implied_volatility: opts.implied_volatility,
        }
    }
}
impl From<PlaceOrderOpts> for PreparedOrderOpts {
    fn from(opts: PlaceOrderOpts) -> Self {
        Self::from(&opts)
    }
}
impl From<&PreparedOrderOpts> for PreparedOrderOpts {
    fn from(opts: &PreparedOrderOpts) -> Self {
        opts.clone()
    }
}

impl Serialize for PreparedOrderOpts {
    fn serialize<T: Serializer>(&self, serializer: T) -> Result<T::Ok, T::Error> {
        let mut record = serializer.serialize_struct(crate::model_field::MODEL_RECORD, 17)?;
        record.serialize_field("symbol", &self.symbol)?;
        self.r#type.serialize_field(&mut record, "type")?;
        self.side.serialize_field(&mut record, "side")?;
        record.serialize_field("amount", &self.amount)?;
        if let Some(value) = &self.price {
            record.serialize_field("price", value)?;
        }
        if let Some(value) = &self.trigger_price {
            record.serialize_field("triggerPrice", value)?;
        }
        if let Some(value) = &self.stop_loss {
            record.serialize_field("stopLoss", value)?;
        }
        if let Some(value) = &self.take_profit {
            record.serialize_field("takeProfit", value)?;
        }
        self.reduce_only
            .serialize_field(&mut record, "reduceOnly")?;
        if let Some(value) = &self.time_in_force {
            record.serialize_field("timeInForce", value)?;
        }
        if let Some(value) = &self.trigger_type {
            record.serialize_field("triggerType", value)?;
        }
        if let Some(value) = &self.tp_trigger_type {
            record.serialize_field("tpTriggerType", value)?;
        }
        if let Some(value) = &self.sl_trigger_type {
            record.serialize_field("slTriggerType", value)?;
        }
        if let Some(value) = &self.client_order_id {
            record.serialize_field("clientOrderId", value)?;
        }
        if let Some(value) = &self.max_slippage_pct {
            record.serialize_field("maxSlippagePct", value)?;
        }
        if let Some(value) = &self.close {
            record.serialize_field("close", value)?;
        }
        if let Some(value) = &self.implied_volatility {
            record.serialize_field("impliedVolatility", value)?;
        }
        record.end()
    }
}

fn deserialize_optional<'de, T: serde::Deserialize<'de>, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Ok(<ModelField<T> as serde::Deserialize>::deserialize(deserializer)?.into_value())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreparedWire {
    symbol: String,
    #[serde(default)]
    r#type: ModelField<OrderType>,
    #[serde(default)]
    side: ModelField<OrderSide>,
    amount: f64,
    #[serde(default, deserialize_with = "deserialize_optional")]
    price: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_optional")]
    trigger_price: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_optional")]
    stop_loss: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_optional")]
    take_profit: Option<f64>,
    #[serde(default)]
    reduce_only: ModelField<bool>,
    #[serde(default, deserialize_with = "deserialize_optional")]
    time_in_force: Option<OrderTimeInForce>,
    #[serde(default, deserialize_with = "deserialize_optional")]
    trigger_type: Option<OrderStopTriggerType>,
    #[serde(default, deserialize_with = "deserialize_optional")]
    tp_trigger_type: Option<OrderStopTriggerType>,
    #[serde(default, deserialize_with = "deserialize_optional")]
    sl_trigger_type: Option<OrderStopTriggerType>,
    #[serde(default, deserialize_with = "deserialize_optional")]
    client_order_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional")]
    max_slippage_pct: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_optional")]
    close: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional")]
    implied_volatility: Option<f64>,
}
impl<'de> serde::Deserialize<'de> for PreparedOrderOpts {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Record;
        impl<'de> serde::de::Visitor<'de> for Record {
            type Value = PreparedOrderOpts;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("prepared venue order input")
            }
            fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Self::Value, D::Error> {
                let wire = <PreparedWire as serde::Deserialize>::deserialize(
                    crate::strict_decode::object(deserializer),
                )?;
                Ok(PreparedOrderOpts {
                    symbol: wire.symbol,
                    r#type: wire.r#type,
                    side: wire.side,
                    amount: wire.amount,
                    price: wire.price,
                    trigger_price: wire.trigger_price,
                    stop_loss: wire.stop_loss,
                    take_profit: wire.take_profit,
                    reduce_only: wire.reduce_only,
                    time_in_force: wire.time_in_force,
                    trigger_type: wire.trigger_type,
                    tp_trigger_type: wire.tp_trigger_type,
                    sl_trigger_type: wire.sl_trigger_type,
                    client_order_id: wire.client_order_id,
                    max_slippage_pct: wire.max_slippage_pct,
                    close: wire.close,
                    implied_volatility: wire.implied_volatility,
                })
            }
        }
        deserializer.deserialize_newtype_struct(crate::model_field::MODEL_RECORD, Record)
    }
}
