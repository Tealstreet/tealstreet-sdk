//! Canonical Position contract and its private wire codecs.
//! ```compile_fail
//! use safe_cex_types::position::strict_codecs::WirePosition;
//! ```
use crate::{
    MarginType, ModelField, OptionGreeks, PositionMode, PositionSide, deserialize_present,
};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct PositionOption {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub greeks: Option<OptionGreeks>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct PositionExtra {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub contracts_native: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub unrealized_pnl_native: Option<f64>,
}

/// Canonical SDK position. Notional and liquidationPrice are required and nullable when unknown.
/// contracts is the whole base-unit quantity; extra.contractsNative retains native placement units.
/// marginSize is absolute quote-currency margin when published by the venue; cross margin may be zero.
#[derive(Debug, Clone, PartialEq, TS)]
#[ts(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct Position {
    pub id: String,
    pub symbol: String,
    #[ts(as = "PositionSide")]
    pub side: ModelField<PositionSide>,
    pub entry_price: f64,
    #[ts(type = "number | null")]
    pub notional: Option<f64>,
    pub leverage: f64,
    #[ts(as = "Option<MarginType>")]
    pub margin_type: ModelField<MarginType>,
    #[ts(as = "Option<PositionMode>")]
    pub position_mode: ModelField<PositionMode>,
    pub unrealized_pnl: f64,
    #[ts(as = "Option<f64>")]
    pub realized_pnl: ModelField<f64>,
    pub contracts: f64,
    #[ts(type = "number | null")]
    pub liquidation_price: Option<f64>,
    #[ts(as = "Option<f64>")]
    pub margin_size: ModelField<f64>,
    #[ts(type = "any", optional)]
    pub info: ModelField<crate::retained_json::RetainedJsonInput>,
    #[ts(inline)]
    #[ts(as = "Option<PositionExtra>")]
    pub extra: ModelField<PositionExtra>,
    #[ts(as = "Option<f64>")]
    pub last_update: ModelField<f64>,
    #[ts(inline)]
    #[ts(as = "Option<PositionOption>")]
    pub option: ModelField<PositionOption>,
}

// Keep permissive serde companions private; public decoding uses the strict map guard.
mod strict_codecs {
    use super::*;

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PositionOption")]
    #[serde(rename_all = "camelCase")]
    struct WirePositionOption {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        greeks: Option<OptionGreeks>,
    }

    impl<'de> serde::Deserialize<'de> for super::PositionOption {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WirePositionOption::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PositionExtra")]
    #[serde(rename_all = "camelCase")]
    struct WirePositionExtra {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        contracts_native: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        unrealized_pnl_native: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::PositionExtra {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WirePositionExtra::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    fn required_nullable_number<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<f64>, D::Error> {
        serde::Deserialize::deserialize(deserializer)
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::Position")]
    #[serde(rename_all = "camelCase")]
    struct WirePosition {
        id: String,
        symbol: String,
        #[serde(default)]
        side: ModelField<PositionSide>,
        entry_price: f64,
        #[serde(deserialize_with = "required_nullable_number")]
        notional: Option<f64>,
        leverage: f64,
        #[serde(default)]
        margin_type: ModelField<MarginType>,
        #[serde(default)]
        position_mode: ModelField<PositionMode>,
        unrealized_pnl: f64,
        #[serde(default)]
        realized_pnl: ModelField<f64>,
        contracts: f64,
        #[serde(deserialize_with = "required_nullable_number")]
        liquidation_price: Option<f64>,
        #[serde(default)]
        margin_size: ModelField<f64>,
        #[serde(default, deserialize_with = "crate::model_field::deserialize_info")]
        info: ModelField<crate::retained_json::RetainedJsonInput>,
        #[serde(default)]
        extra: ModelField<PositionExtra>,
        #[serde(default)]
        last_update: ModelField<f64>,
        #[serde(default)]
        option: ModelField<PositionOption>,
    }

    impl<'de> serde::Deserialize<'de> for super::Position {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct RecordVisitor;
            impl<'de> serde::de::Visitor<'de> for RecordVisitor {
                type Value = super::Position;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a model record")
                }
                fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                    self,
                    decoder: D,
                ) -> Result<Self::Value, D::Error> {
                    WirePosition::deserialize(crate::strict_decode::object(decoder))
                }
                // Map-forwarding decoders (flatten/MapAccessDeserializer) skip the hint.
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    map: A,
                ) -> Result<Self::Value, A::Error> {
                    WirePosition::deserialize(crate::strict_decode::object(
                        serde::de::value::MapAccessDeserializer::new(map),
                    ))
                }
            }
            deserializer.deserialize_newtype_struct(crate::model_field::MODEL_RECORD, RecordVisitor)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn position_codecs_accept_only_records() {
            crate::strict_decode::tests::record::<Position>("Position");
            crate::strict_decode::tests::record::<PositionExtra>("PositionExtra");
            crate::strict_decode::tests::record::<PositionOption>("PositionOption");
        }
    }
}

impl serde::Serialize for Position {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct(crate::model_field::MODEL_RECORD, 17)?;
        serialize_position_fields(self, &mut state)?;
        state.end()
    }
}
pub(crate) fn serialize_position_fields<S: serde::ser::SerializeStruct>(
    position: &Position,
    state: &mut S,
) -> Result<(), S::Error> {
    state.serialize_field("id", &position.id)?;
    state.serialize_field("symbol", &position.symbol)?;
    position.side.serialize_field(state, "side")?;
    state.serialize_field("entryPrice", &position.entry_price)?;
    state.serialize_field("notional", &position.notional)?;
    state.serialize_field("leverage", &position.leverage)?;
    position.margin_type.serialize_field(state, "marginType")?;
    position
        .position_mode
        .serialize_field(state, "positionMode")?;
    state.serialize_field("unrealizedPnl", &position.unrealized_pnl)?;
    position
        .realized_pnl
        .serialize_field(state, "realizedPnl")?;
    state.serialize_field("contracts", &position.contracts)?;
    state.serialize_field("liquidationPrice", &position.liquidation_price)?;
    position.margin_size.serialize_field(state, "marginSize")?;
    match &position.info {
        ModelField::Absent => {}
        ModelField::Undefined
        | ModelField::Value(crate::retained_json::RetainedJsonInput::Undefined) => {
            state.skip_field("info")?
        }
        ModelField::Value(value) => state.serialize_field("info", value)?,
    }
    position.extra.serialize_field(state, "extra")?;
    position.last_update.serialize_field(state, "lastUpdate")?;
    position.option.serialize_field(state, "option")?;
    Ok(())
}
