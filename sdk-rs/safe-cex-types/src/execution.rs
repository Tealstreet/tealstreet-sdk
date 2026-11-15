//! Public live-fill, historical execution and public-trade contracts.
use crate::{ExecutionPositionSide, OrderSide};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// A live incremental execution event; never substitute historical Execution.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OrderFillEvent {
    /// Venue execution identity when reported, otherwise the native order identity.
    pub id: String,
    pub amount: f64,
    pub price: f64,
    pub side: OrderSide,
    pub symbol: String,
    /// Native order identity used by the chaser.
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub order_id: Option<String>,
    /// Live venue PnL convention; distinct from historical gross realized PnL.
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub realized_pnl: Option<f64>,
    /// Venue-reported execution fee; absent is distinct from zero.
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub fee: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct Execution {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
    /// Seconds, unlike PublicTrade.timestamp and HistoricalTrade.time (milliseconds).
    pub timestamp: f64,
    pub symbol: String,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub order_id: Option<String>,
    pub price: f64,
    pub qty: f64,
    pub side: OrderSide,
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
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    /// Absent means unreported; both is explicit one-way attribution.
    #[ts(inline)]
    pub position_side: Option<ExecutionPositionSide>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    /// Venue-reported gross realized PnL; commission is separate.
    pub realized_pnl: Option<f64>,
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
    /// Venue-reported integrator revenue, distinct from user commission.
    pub venue_reported_revenue: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub venue_reported_revenue_asset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct PublicTrade {
    pub id: String,
    pub symbol: String,
    pub price: f64,
    pub qty: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub quote_qty: Option<f64>,
    /// Milliseconds.
    pub timestamp: f64,
    pub side: OrderSide,
}

mod strict_codecs {
    use super::*;
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PublicTrade")]
    #[serde(rename_all = "camelCase")]
    struct WirePublicTrade {
        id: String,
        symbol: String,
        price: f64,
        qty: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        quote_qty: Option<f64>,
        timestamp: f64,
        side: OrderSide,
    }

    impl<'de> serde::Deserialize<'de> for super::PublicTrade {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WirePublicTrade::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::Execution")]
    #[serde(rename_all = "camelCase")]
    struct WireExecution {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        id: Option<String>,
        timestamp: f64,
        symbol: String,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        order_id: Option<String>,
        price: f64,
        qty: f64,
        side: OrderSide,
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
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        position_side: Option<ExecutionPositionSide>,
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
        maker: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        venue_reported_revenue: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        venue_reported_revenue_asset: Option<String>,
    }

    impl<'de> serde::Deserialize<'de> for super::Execution {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExecution::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderFillEvent")]
    #[serde(rename_all = "camelCase")]
    struct WireOrderFillEvent {
        id: String,
        amount: f64,
        price: f64,
        side: OrderSide,
        symbol: String,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        order_id: Option<String>,
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
        fee: Option<f64>,
    }
    impl<'de> serde::Deserialize<'de> for super::OrderFillEvent {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            WireOrderFillEvent::deserialize(crate::strict_decode::object(d))
        }
    }
}
