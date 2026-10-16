//! Canonical linear-perpetual journal record; no venue adapters or folding here.
//! ```compile_fail
//! use safe_cex_types::position_record::WirePositionCloseReason;
//! ```
//! ```compile_fail
//! use safe_cex_types::position_record::strict_codecs::WirePositionRecord;
//! ```
use crate::PositionSide;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionCloseReason {
    User,
    Liquidation,
    Adl,
    Settlement,
}
// Canonical spellings occur only on this private wire companion.
#[derive(Serialize, Deserialize)]
enum WirePositionCloseReason {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "liquidation")]
    Liquidation,
    #[serde(rename = "adl")]
    Adl,
    #[serde(rename = "settlement")]
    Settlement,
}
impl Serialize for PositionCloseReason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::User => WirePositionCloseReason::User,
            Self::Liquidation => WirePositionCloseReason::Liquidation,
            Self::Adl => WirePositionCloseReason::Adl,
            Self::Settlement => WirePositionCloseReason::Settlement,
        };
        wire.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for PositionCloseReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        WirePositionCloseReason::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &text,
        ))
        .map(|wire| match wire {
            WirePositionCloseReason::User => Self::User,
            WirePositionCloseReason::Liquidation => Self::Liquidation,
            WirePositionCloseReason::Adl => Self::Adl,
            WirePositionCloseReason::Settlement => Self::Settlement,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionProvenance {
    Reported,
    Derived,
}
// Canonical spellings occur only on this private wire companion.
#[derive(Serialize, Deserialize)]
enum WirePositionProvenance {
    #[serde(rename = "reported")]
    Reported,
    #[serde(rename = "derived")]
    Derived,
}
impl Serialize for PositionProvenance {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Reported => WirePositionProvenance::Reported,
            Self::Derived => WirePositionProvenance::Derived,
        };
        wire.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for PositionProvenance {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        WirePositionProvenance::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &text,
        ))
        .map(|wire| match wire {
            WirePositionProvenance::Reported => Self::Reported,
            WirePositionProvenance::Derived => Self::Derived,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionSourceAxis {
    ClosedPositions,
    ClosedPositionsOrders,
    Fills,
    FillsOrders,
    Orders,
}
// Canonical spellings occur only on this private wire companion.
#[derive(Serialize, Deserialize)]
enum WirePositionSourceAxis {
    #[serde(rename = "closed-positions")]
    ClosedPositions,
    #[serde(rename = "closed-positions+orders")]
    ClosedPositionsOrders,
    #[serde(rename = "fills")]
    Fills,
    #[serde(rename = "fills+orders")]
    FillsOrders,
    #[serde(rename = "orders")]
    Orders,
}
impl Serialize for PositionSourceAxis {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::ClosedPositions => WirePositionSourceAxis::ClosedPositions,
            Self::ClosedPositionsOrders => WirePositionSourceAxis::ClosedPositionsOrders,
            Self::Fills => WirePositionSourceAxis::Fills,
            Self::FillsOrders => WirePositionSourceAxis::FillsOrders,
            Self::Orders => WirePositionSourceAxis::Orders,
        };
        wire.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for PositionSourceAxis {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        WirePositionSourceAxis::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &text,
        ))
        .map(|wire| match wire {
            WirePositionSourceAxis::ClosedPositions => Self::ClosedPositions,
            WirePositionSourceAxis::ClosedPositionsOrders => Self::ClosedPositionsOrders,
            WirePositionSourceAxis::Fills => Self::Fills,
            WirePositionSourceAxis::FillsOrders => Self::FillsOrders,
            WirePositionSourceAxis::Orders => Self::Orders,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionRecordLeg {
    Long,
    Short,
    Both,
}
// Canonical spellings occur only on this private wire companion.
#[derive(Serialize, Deserialize)]
enum WirePositionRecordLeg {
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "short")]
    Short,
    #[serde(rename = "both")]
    Both,
}
impl Serialize for PositionRecordLeg {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Long => WirePositionRecordLeg::Long,
            Self::Short => WirePositionRecordLeg::Short,
            Self::Both => WirePositionRecordLeg::Both,
        };
        wire.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for PositionRecordLeg {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        WirePositionRecordLeg::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &text,
        ))
        .map(|wire| match wire {
            WirePositionRecordLeg::Long => Self::Long,
            WirePositionRecordLeg::Short => Self::Short,
            WirePositionRecordLeg::Both => Self::Both,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionRecordState {
    Open,
    Closed,
}
// Canonical spellings occur only on this private wire companion.
#[derive(Serialize, Deserialize)]
enum WirePositionRecordState {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "closed")]
    Closed,
}
impl Serialize for PositionRecordState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Open => WirePositionRecordState::Open,
            Self::Closed => WirePositionRecordState::Closed,
        };
        wire.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for PositionRecordState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        WirePositionRecordState::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &text,
        ))
        .map(|wire| match wire {
            WirePositionRecordState::Open => Self::Open,
            WirePositionRecordState::Closed => Self::Closed,
        })
    }
}

/// A position's gross PnL and separately attributed costs in its native settlement currency.
/// Readonly TypeScript fields become owned Rust values; adapters construct one record.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRecord {
    pub key: String,
    pub account: String,
    pub symbol: String,
    pub settle_currency: String,
    pub side: PositionSide,
    pub position_side: PositionRecordLeg,
    pub state: PositionRecordState,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub opened_at: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub closed_at: Option<f64>,
    pub closed_size: f64,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_size: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub leverage: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub closed_notional: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub venue_id: Option<String>,
    pub avg_entry_price: f64,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub avg_exit_price: Option<f64>,
    pub realized_pnl: f64,
    pub fees: f64,
    pub funding: f64,
    pub funding_attributed: bool,
    #[serde(
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub close_reason: Option<PositionCloseReason>,
    pub provenance: PositionProvenance,
    pub source_axis: PositionSourceAxis,
    pub close_count: f64,
}

mod strict_codecs {
    use super::{
        PositionCloseReason, PositionProvenance, PositionRecordLeg, PositionRecordState,
        PositionSide, PositionSourceAxis,
    };
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PositionRecord", rename_all = "camelCase")]
    struct WirePositionRecord {
        key: String,
        account: String,
        symbol: String,
        settle_currency: String,
        side: PositionSide,
        position_side: PositionRecordLeg,
        state: PositionRecordState,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        opened_at: Option<f64>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        closed_at: Option<f64>,
        closed_size: f64,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        max_size: Option<f64>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        leverage: Option<f64>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        closed_notional: Option<f64>,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        venue_id: Option<String>,
        avg_entry_price: f64,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        avg_exit_price: Option<f64>,
        realized_pnl: f64,
        fees: f64,
        funding: f64,
        funding_attributed: bool,
        #[serde(
            default,
            deserialize_with = "crate::deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        close_reason: Option<PositionCloseReason>,
        provenance: PositionProvenance,
        source_axis: PositionSourceAxis,
        close_count: f64,
    }
    impl<'de> serde::Deserialize<'de> for super::PositionRecord {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WirePositionRecord::deserialize(crate::strict_decode::object(deserializer))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        WirePositionCloseReason, WirePositionProvenance, WirePositionRecordLeg,
        WirePositionRecordState, WirePositionSourceAxis,
    };
    use crate::strict_decode::tests::{Inventory, string_union};
    use serde::Deserialize;
    #[test]
    fn serde_accepted_spelling_inventory_matches_source() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/position_record_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let spec: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        macro_rules! inventory {
            ($name:ident,$wire:ident) => {{
                let actual = $wire::deserialize(
                    serde::de::value::StrDeserializer::<Inventory>::new("__inventory__"),
                )
                .err()
                .unwrap()
                .0;
                let expected = spec["unions"][stringify!($name)]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect();
                let first = actual[0].clone();
                string_union::<super::$name>(stringify!($name), actual, expected);
                for bad in [serde_json::json!({(first.clone()):null}), serde_json::json!(null), serde_json::json!([]),serde_json::json!(1),serde_json::json!(true)] {
                    assert!(super::$name::deserialize(bad).is_err(), "public inherent-call regression must remain strict");
                }
            }};
        }
        inventory!(PositionCloseReason, WirePositionCloseReason);
        inventory!(PositionProvenance, WirePositionProvenance);
        inventory!(PositionSourceAxis, WirePositionSourceAxis);
        inventory!(PositionRecordLeg, WirePositionRecordLeg);
        inventory!(PositionRecordState, WirePositionRecordState);
    }
}
