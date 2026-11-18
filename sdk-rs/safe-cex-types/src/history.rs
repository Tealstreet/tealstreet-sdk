//! Rust-owned history and ledger contracts; quantities and money retain venue units.
use crate::{ExecutionPositionSide, OrderSide, OrderStatus, OrderType, deserialize_present};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct HistoricalTrade {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
    pub symbol: String,
    pub order_id: String,
    pub price: f64,
    pub qty: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub quote_qty: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub realized_pnl: Option<f64>,
    #[ts(inline)]
    pub position_side: ExecutionPositionSide,
    pub side: OrderSide,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub buyer: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub maker: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub commission: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub commission_asset: Option<String>,
    pub time: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct HistoricalOrder {
    pub id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub r#type: OrderType,
    pub status: OrderStatus,
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
    pub average_price: Option<f64>,
    pub amount: f64,
    pub filled: f64,
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
    #[ts(inline)]
    pub position_side: Option<ExecutionPositionSide>,
    pub created_time: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_time: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    /// Venue-reported gross PnL; absence means unmapped or unreported, never zero.
    pub realized_pnl: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    /// Positive is a cost to the trader; do not net rebates into fees.
    pub fees: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub fee_asset: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    /// Positive is received by the trader, separate from fees.
    pub rebate: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub rebate_asset: Option<String>,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum DataProvenance {
    #[serde(rename = "venue")]
    Venue,
    #[serde(rename = "derived")]
    Derived,
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryNotional {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryEntryPrice {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryExitPrice {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryUnrealizedPnl {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryFees {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryContracts {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryLeverage {
    Variant0(f64),
    Variant1(String),
}

fn required_nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ClosedPnlEntry {
    /// @ts-mutable
    #[serde(rename = "provenance")]
    pub provenance: DataProvenance,
    /// @ts-mutable
    #[serde(rename = "symbol")]
    pub symbol: String,
    /// @ts-mutable
    #[serde(rename = "notional")]
    #[serde(deserialize_with = "required_nullable")]
    #[ts(inline)]
    pub notional: Option<ClosedPnlEntryNotional>,
    /// @ts-mutable
    #[serde(rename = "entryPrice")]
    #[serde(deserialize_with = "required_nullable")]
    #[ts(inline)]
    pub entry_price: Option<ClosedPnlEntryEntryPrice>,
    /// @ts-mutable
    #[serde(rename = "exitPrice")]
    #[serde(deserialize_with = "required_nullable")]
    #[ts(inline)]
    pub exit_price: Option<ClosedPnlEntryExitPrice>,
    /// @ts-mutable
    #[serde(rename = "unrealizedPnl")]
    #[serde(deserialize_with = "required_nullable")]
    #[ts(inline)]
    pub unrealized_pnl: Option<ClosedPnlEntryUnrealizedPnl>,
    /// @ts-mutable
    #[serde(rename = "side")]
    pub side: String,
    /// @ts-mutable
    #[serde(rename = "fees")]
    #[serde(deserialize_with = "required_nullable")]
    #[ts(inline)]
    pub fees: Option<ClosedPnlEntryFees>,
    /// @ts-mutable
    #[serde(rename = "time")]
    #[serde(deserialize_with = "required_nullable")]
    pub time: Option<f64>,
    /// @ts-mutable
    #[serde(rename = "closedTime")]
    #[serde(deserialize_with = "required_nullable")]
    pub closed_time: Option<f64>,
    /// @ts-mutable
    #[serde(rename = "account")]
    pub account: String,
    /// @ts-mutable
    #[serde(rename = "contracts")]
    #[serde(deserialize_with = "required_nullable")]
    #[ts(inline)]
    pub contracts: Option<ClosedPnlEntryContracts>,
    /// @ts-mutable
    #[serde(rename = "leverage")]
    #[serde(deserialize_with = "required_nullable")]
    #[ts(inline)]
    pub leverage: Option<ClosedPnlEntryLeverage>,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum IncomeCategory {
    #[serde(rename = "realized_pnl")]
    RealizedPnl,
    #[serde(rename = "funding")]
    Funding,
    #[serde(rename = "commission")]
    Commission,
    #[serde(rename = "rebate")]
    Rebate,
    #[serde(rename = "transfer")]
    Transfer,
    #[serde(rename = "liquidation")]
    Liquidation,
    #[serde(rename = "other")]
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct IncomeRecord {
    /// @ts-mutable
    #[serde(rename = "symbol")]
    pub symbol: String,
    /// @ts-mutable
    #[serde(rename = "incomeType")]
    pub income_type: String,
    /// @ts-mutable
    #[serde(rename = "income")]
    pub income: f64,
    /// @ts-mutable
    #[serde(rename = "asset")]
    pub asset: String,
    /// @ts-mutable
    #[serde(rename = "timestamp")]
    pub timestamp: f64,
    /// @ts-mutable
    #[serde(rename = "tranId")]
    pub tran_id: String,
    /// @ts-mutable
    #[serde(rename = "tradeId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub trade_id: Option<String>,
    /// @ts-mutable
    #[serde(rename = "category")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub category: Option<IncomeCategory>,
}

mod strict_codecs {
    use super::*;

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::HistoricalTrade")]
    #[serde(rename_all = "camelCase")]
    struct WireHistoricalTrade {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        id: Option<String>,
        symbol: String,
        order_id: String,
        price: f64,
        qty: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        quote_qty: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        realized_pnl: Option<f64>,
        position_side: ExecutionPositionSide,
        side: OrderSide,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        buyer: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        maker: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        commission: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        commission_asset: Option<String>,
        time: f64,
    }

    impl<'de> serde::Deserialize<'de> for super::HistoricalTrade {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireHistoricalTrade::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::HistoricalOrder")]
    #[serde(rename_all = "camelCase")]
    struct WireHistoricalOrder {
        id: String,
        symbol: String,
        side: OrderSide,
        r#type: OrderType,
        status: OrderStatus,
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
        average_price: Option<f64>,
        amount: f64,
        filled: f64,
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
        position_side: Option<ExecutionPositionSide>,
        created_time: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        updated_time: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        realized_pnl: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        fees: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        fee_asset: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        rebate: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        rebate_asset: Option<String>,
    }

    impl<'de> serde::Deserialize<'de> for super::HistoricalOrder {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireHistoricalOrder::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::DataProvenance")]
    enum WireDataProvenance {
        #[serde(rename = "venue")]
        Venue,
        #[serde(rename = "derived")]
        Derived,
    }

    impl<'de> serde::Deserialize<'de> for super::DataProvenance {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireDataProvenance::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ClosedPnlEntry")]
    struct WireClosedPnlEntry {
        #[serde(rename = "provenance")]
        provenance: DataProvenance,
        #[serde(rename = "symbol")]
        symbol: String,
        #[serde(rename = "notional")]
        #[serde(deserialize_with = "required_nullable")]
        notional: Option<ClosedPnlEntryNotional>,
        #[serde(rename = "entryPrice")]
        #[serde(deserialize_with = "required_nullable")]
        entry_price: Option<ClosedPnlEntryEntryPrice>,
        #[serde(rename = "exitPrice")]
        #[serde(deserialize_with = "required_nullable")]
        exit_price: Option<ClosedPnlEntryExitPrice>,
        #[serde(rename = "unrealizedPnl")]
        #[serde(deserialize_with = "required_nullable")]
        unrealized_pnl: Option<ClosedPnlEntryUnrealizedPnl>,
        #[serde(rename = "side")]
        side: String,
        #[serde(rename = "fees")]
        #[serde(deserialize_with = "required_nullable")]
        fees: Option<ClosedPnlEntryFees>,
        #[serde(rename = "time")]
        #[serde(deserialize_with = "required_nullable")]
        time: Option<f64>,
        #[serde(rename = "closedTime")]
        #[serde(deserialize_with = "required_nullable")]
        closed_time: Option<f64>,
        #[serde(rename = "account")]
        account: String,
        #[serde(rename = "contracts")]
        #[serde(deserialize_with = "required_nullable")]
        contracts: Option<ClosedPnlEntryContracts>,
        #[serde(rename = "leverage")]
        #[serde(deserialize_with = "required_nullable")]
        leverage: Option<ClosedPnlEntryLeverage>,
    }

    impl<'de> serde::Deserialize<'de> for super::ClosedPnlEntry {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireClosedPnlEntry::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::IncomeCategory")]
    enum WireIncomeCategory {
        #[serde(rename = "realized_pnl")]
        RealizedPnl,
        #[serde(rename = "funding")]
        Funding,
        #[serde(rename = "commission")]
        Commission,
        #[serde(rename = "rebate")]
        Rebate,
        #[serde(rename = "transfer")]
        Transfer,
        #[serde(rename = "liquidation")]
        Liquidation,
        #[serde(rename = "other")]
        Other,
    }

    impl<'de> serde::Deserialize<'de> for super::IncomeCategory {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireIncomeCategory::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::IncomeRecord")]
    struct WireIncomeRecord {
        #[serde(rename = "symbol")]
        symbol: String,
        #[serde(rename = "incomeType")]
        income_type: String,
        #[serde(rename = "income")]
        income: f64,
        #[serde(rename = "asset")]
        asset: String,
        #[serde(rename = "timestamp")]
        timestamp: f64,
        #[serde(rename = "tranId")]
        tran_id: String,
        #[serde(rename = "tradeId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        trade_id: Option<String>,
        #[serde(rename = "category")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        category: Option<IncomeCategory>,
    }

    impl<'de> serde::Deserialize<'de> for super::IncomeRecord {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireIncomeRecord::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn string_union_metadata_remains_closed() {
            crate::strict_decode::tests::string_union::<DataProvenance>(
                "DataProvenance",
                WireDataProvenance::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(DataProvenance::Venue)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(DataProvenance::Derived)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<IncomeCategory>(
                "IncomeCategory",
                WireIncomeCategory::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(IncomeCategory::RealizedPnl)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Funding)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Commission)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Rebate)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Transfer)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Liquidation)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Other)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
        }
    }
}

// Untagged Content buffers arbitrary-precision decimals as maps. Retained scalar
// capture distinguishes real numbers from objects imitating Serde number markers.
enum NumberOrText {
    Text(String),
    Number(f64),
}
fn number_or_text<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<NumberOrText, D::Error> {
    use crate::retained_json::RetainedJsonInput;
    match RetainedJsonInput::deserialize(deserializer)? {
        RetainedJsonInput::Text(units) => String::from_utf16(&units)
            .map(NumberOrText::Text)
            .map_err(serde::de::Error::custom),
        RetainedJsonInput::Float(value) => Ok(NumberOrText::Number(value)),
        RetainedJsonInput::Lexeme(value) => serde_json::from_str::<f64>(&value)
            .map(NumberOrText::Number)
            .map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom("expected a number or string")),
    }
}

impl<'de> Deserialize<'de> for ClosedPnlEntryNotional {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        number_or_text(deserializer).map(|value| match value {
            NumberOrText::Text(value) => Self::Variant0(value),
            NumberOrText::Number(value) => Self::Variant1(value),
        })
    }
}

impl<'de> Deserialize<'de> for ClosedPnlEntryEntryPrice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        number_or_text(deserializer).map(|value| match value {
            NumberOrText::Text(value) => Self::Variant0(value),
            NumberOrText::Number(value) => Self::Variant1(value),
        })
    }
}

impl<'de> Deserialize<'de> for ClosedPnlEntryExitPrice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        number_or_text(deserializer).map(|value| match value {
            NumberOrText::Text(value) => Self::Variant0(value),
            NumberOrText::Number(value) => Self::Variant1(value),
        })
    }
}

impl<'de> Deserialize<'de> for ClosedPnlEntryUnrealizedPnl {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        number_or_text(deserializer).map(|value| match value {
            NumberOrText::Text(value) => Self::Variant0(value),
            NumberOrText::Number(value) => Self::Variant1(value),
        })
    }
}

impl<'de> Deserialize<'de> for ClosedPnlEntryFees {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        number_or_text(deserializer).map(|value| match value {
            NumberOrText::Text(value) => Self::Variant0(value),
            NumberOrText::Number(value) => Self::Variant1(value),
        })
    }
}

impl<'de> Deserialize<'de> for ClosedPnlEntryContracts {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        number_or_text(deserializer).map(|value| match value {
            NumberOrText::Text(value) => Self::Variant0(value),
            NumberOrText::Number(value) => Self::Variant1(value),
        })
    }
}

impl<'de> Deserialize<'de> for ClosedPnlEntryLeverage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        number_or_text(deserializer).map(|value| match value {
            NumberOrText::Text(value) => Self::Variant1(value),
            NumberOrText::Number(value) => Self::Variant0(value),
        })
    }
}
