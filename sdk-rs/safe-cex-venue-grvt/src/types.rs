//! Exact GRVT string unions. Deserialization accepts strings only.
use serde::{Deserialize, Deserializer, Serialize};

/// TypeScript `GrvtOrderSide` accepted string values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GrvtOrderSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}

#[derive(Deserialize)]
enum WireGrvtOrderSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
impl<'de> Deserialize<'de> for GrvtOrderSide {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire = WireGrvtOrderSide::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&value),
        )?;
        Ok(match wire {
            WireGrvtOrderSide::Buy => Self::Buy,
            WireGrvtOrderSide::Sell => Self::Sell,
        })
    }
}

/// TypeScript `GrvtOrderType` accepted string values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GrvtOrderType {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
}

#[derive(Deserialize)]
enum WireGrvtOrderType {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
}
impl<'de> Deserialize<'de> for GrvtOrderType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire = WireGrvtOrderType::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&value),
        )?;
        Ok(match wire {
            WireGrvtOrderType::Limit => Self::Limit,
            WireGrvtOrderType::Market => Self::Market,
        })
    }
}

/// TypeScript `GrvtTimeInForce` accepted string values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GrvtTimeInForce {
    #[serde(rename = "GOOD_TILL_TIME")]
    GoodTillTime,
    #[serde(rename = "ALL_OR_NONE")]
    AllOrNone,
    #[serde(rename = "IMMEDIATE_OR_CANCEL")]
    ImmediateOrCancel,
    #[serde(rename = "FILL_OR_KILL")]
    FillOrKill,
}

#[derive(Deserialize)]
enum WireGrvtTimeInForce {
    #[serde(rename = "GOOD_TILL_TIME")]
    GoodTillTime,
    #[serde(rename = "ALL_OR_NONE")]
    AllOrNone,
    #[serde(rename = "IMMEDIATE_OR_CANCEL")]
    ImmediateOrCancel,
    #[serde(rename = "FILL_OR_KILL")]
    FillOrKill,
}
impl<'de> Deserialize<'de> for GrvtTimeInForce {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire = WireGrvtTimeInForce::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&value),
        )?;
        Ok(match wire {
            WireGrvtTimeInForce::GoodTillTime => Self::GoodTillTime,
            WireGrvtTimeInForce::AllOrNone => Self::AllOrNone,
            WireGrvtTimeInForce::ImmediateOrCancel => Self::ImmediateOrCancel,
            WireGrvtTimeInForce::FillOrKill => Self::FillOrKill,
        })
    }
}

/// TypeScript `GrvtCandlestickInterval` accepted string values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GrvtCandlestickInterval {
    #[serde(rename = "CI_1_M")]
    Ci1M,
    #[serde(rename = "CI_3_M")]
    Ci3M,
    #[serde(rename = "CI_5_M")]
    Ci5M,
    #[serde(rename = "CI_15_M")]
    Ci15M,
    #[serde(rename = "CI_30_M")]
    Ci30M,
    #[serde(rename = "CI_1_H")]
    Ci1H,
    #[serde(rename = "CI_2_H")]
    Ci2H,
    #[serde(rename = "CI_4_H")]
    Ci4H,
    #[serde(rename = "CI_6_H")]
    Ci6H,
    #[serde(rename = "CI_8_H")]
    Ci8H,
    #[serde(rename = "CI_12_H")]
    Ci12H,
    #[serde(rename = "CI_1_D")]
    Ci1D,
    #[serde(rename = "CI_3_D")]
    Ci3D,
    #[serde(rename = "CI_5_D")]
    Ci5D,
    #[serde(rename = "CI_1_W")]
    Ci1W,
    #[serde(rename = "CI_2_W")]
    Ci2W,
    #[serde(rename = "CI_3_W")]
    Ci3W,
    #[serde(rename = "CI_4_W")]
    Ci4W,
}

#[derive(Deserialize)]
enum WireGrvtCandlestickInterval {
    #[serde(rename = "CI_1_M")]
    Ci1M,
    #[serde(rename = "CI_3_M")]
    Ci3M,
    #[serde(rename = "CI_5_M")]
    Ci5M,
    #[serde(rename = "CI_15_M")]
    Ci15M,
    #[serde(rename = "CI_30_M")]
    Ci30M,
    #[serde(rename = "CI_1_H")]
    Ci1H,
    #[serde(rename = "CI_2_H")]
    Ci2H,
    #[serde(rename = "CI_4_H")]
    Ci4H,
    #[serde(rename = "CI_6_H")]
    Ci6H,
    #[serde(rename = "CI_8_H")]
    Ci8H,
    #[serde(rename = "CI_12_H")]
    Ci12H,
    #[serde(rename = "CI_1_D")]
    Ci1D,
    #[serde(rename = "CI_3_D")]
    Ci3D,
    #[serde(rename = "CI_5_D")]
    Ci5D,
    #[serde(rename = "CI_1_W")]
    Ci1W,
    #[serde(rename = "CI_2_W")]
    Ci2W,
    #[serde(rename = "CI_3_W")]
    Ci3W,
    #[serde(rename = "CI_4_W")]
    Ci4W,
}
impl<'de> Deserialize<'de> for GrvtCandlestickInterval {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire = WireGrvtCandlestickInterval::deserialize(serde::de::value::StrDeserializer::<
            D::Error,
        >::new(&value))?;
        Ok(match wire {
            WireGrvtCandlestickInterval::Ci1M => Self::Ci1M,
            WireGrvtCandlestickInterval::Ci3M => Self::Ci3M,
            WireGrvtCandlestickInterval::Ci5M => Self::Ci5M,
            WireGrvtCandlestickInterval::Ci15M => Self::Ci15M,
            WireGrvtCandlestickInterval::Ci30M => Self::Ci30M,
            WireGrvtCandlestickInterval::Ci1H => Self::Ci1H,
            WireGrvtCandlestickInterval::Ci2H => Self::Ci2H,
            WireGrvtCandlestickInterval::Ci4H => Self::Ci4H,
            WireGrvtCandlestickInterval::Ci6H => Self::Ci6H,
            WireGrvtCandlestickInterval::Ci8H => Self::Ci8H,
            WireGrvtCandlestickInterval::Ci12H => Self::Ci12H,
            WireGrvtCandlestickInterval::Ci1D => Self::Ci1D,
            WireGrvtCandlestickInterval::Ci3D => Self::Ci3D,
            WireGrvtCandlestickInterval::Ci5D => Self::Ci5D,
            WireGrvtCandlestickInterval::Ci1W => Self::Ci1W,
            WireGrvtCandlestickInterval::Ci2W => Self::Ci2W,
            WireGrvtCandlestickInterval::Ci3W => Self::Ci3W,
            WireGrvtCandlestickInterval::Ci4W => Self::Ci4W,
        })
    }
}

/// TypeScript `GrvtOrderStatus` accepted string values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GrvtOrderStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "OPEN")]
    Open,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "REJECTED")]
    Rejected,
    #[serde(rename = "CANCELLED")]
    Cancelled,
}

#[derive(Deserialize)]
enum WireGrvtOrderStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "OPEN")]
    Open,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "REJECTED")]
    Rejected,
    #[serde(rename = "CANCELLED")]
    Cancelled,
}
impl<'de> Deserialize<'de> for GrvtOrderStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire = WireGrvtOrderStatus::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&value),
        )?;
        Ok(match wire {
            WireGrvtOrderStatus::Pending => Self::Pending,
            WireGrvtOrderStatus::Open => Self::Open,
            WireGrvtOrderStatus::Filled => Self::Filled,
            WireGrvtOrderStatus::Rejected => Self::Rejected,
            WireGrvtOrderStatus::Cancelled => Self::Cancelled,
        })
    }
}

/// TypeScript `GrvtAssetKind` accepted string values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GrvtAssetKind {
    #[serde(rename = "PERPETUAL")]
    Perpetual,
    #[serde(rename = "FUTURE")]
    Future,
    #[serde(rename = "CALL")]
    Call,
    #[serde(rename = "PUT")]
    Put,
}

#[derive(Deserialize)]
enum WireGrvtAssetKind {
    #[serde(rename = "PERPETUAL")]
    Perpetual,
    #[serde(rename = "FUTURE")]
    Future,
    #[serde(rename = "CALL")]
    Call,
    #[serde(rename = "PUT")]
    Put,
}
impl<'de> Deserialize<'de> for GrvtAssetKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire = WireGrvtAssetKind::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&value),
        )?;
        Ok(match wire {
            WireGrvtAssetKind::Perpetual => Self::Perpetual,
            WireGrvtAssetKind::Future => Self::Future,
            WireGrvtAssetKind::Call => Self::Call,
            WireGrvtAssetKind::Put => Self::Put,
        })
    }
}

/// Inline `GrvtFill.liquidity` string union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GrvtLiquidity {
    #[serde(rename = "MAKER")]
    Maker,
    #[serde(rename = "TAKER")]
    Taker,
}
#[derive(Deserialize)]
enum WireGrvtLiquidity {
    #[serde(rename = "MAKER")]
    Maker,
    #[serde(rename = "TAKER")]
    Taker,
}
impl<'de> Deserialize<'de> for GrvtLiquidity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire = WireGrvtLiquidity::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&value),
        )?;
        Ok(match wire {
            WireGrvtLiquidity::Maker => Self::Maker,
            WireGrvtLiquidity::Taker => Self::Taker,
        })
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtAssetKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Perpetual, Future, Call, Put });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GrvtJsonRpcVersion {
    #[serde(rename = "2.0")]
    V2,
}
#[derive(Deserialize)]
enum WireGrvtJsonRpcVersion {
    #[serde(rename = "2.0")]
    V2,
}
impl<'de> Deserialize<'de> for GrvtJsonRpcVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire = WireGrvtJsonRpcVersion::deserialize(serde::de::value::StrDeserializer::<
            D::Error,
        >::new(&value))?;
        Ok(match wire {
            WireGrvtJsonRpcVersion::V2 => Self::V2,
        })
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtJsonRpcVersion {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { V2 });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GrvtSubscriptionMethod {
    #[serde(rename = "subscribe")]
    Subscribe,
    #[serde(rename = "unsubscribe")]
    Unsubscribe,
}
#[derive(Deserialize)]
enum WireGrvtSubscriptionMethod {
    #[serde(rename = "subscribe")]
    Subscribe,
    #[serde(rename = "unsubscribe")]
    Unsubscribe,
}
impl<'de> Deserialize<'de> for GrvtSubscriptionMethod {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire = WireGrvtSubscriptionMethod::deserialize(serde::de::value::StrDeserializer::<
            D::Error,
        >::new(&value))?;
        Ok(match wire {
            WireGrvtSubscriptionMethod::Subscribe => Self::Subscribe,
            WireGrvtSubscriptionMethod::Unsubscribe => Self::Unsubscribe,
        })
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtSubscriptionMethod {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Subscribe, Unsubscribe });
    }
}

/// Closed reasons from TypeScript GrvtBuilderUnauthorizedReason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GrvtBuilderUnauthorizedReason {
    #[serde(rename = "unauthorized")]
    Unauthorized,
    #[serde(rename = "cap-too-low")]
    CapTooLow,
}
#[derive(Deserialize)]
enum WireGrvtBuilderUnauthorizedReason {
    #[serde(rename = "unauthorized")]
    Unauthorized,
    #[serde(rename = "cap-too-low")]
    CapTooLow,
}
impl<'de> Deserialize<'de> for GrvtBuilderUnauthorizedReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire =
            WireGrvtBuilderUnauthorizedReason::deserialize(serde::de::value::StrDeserializer::<
                D::Error,
            >::new(&value))?;
        Ok(match wire {
            WireGrvtBuilderUnauthorizedReason::Unauthorized => Self::Unauthorized,
            WireGrvtBuilderUnauthorizedReason::CapTooLow => Self::CapTooLow,
        })
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtBuilderUnauthorizedReason {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Unauthorized, CapTooLow });
    }
}

/// Inline type union in TypeScript GrvtTriggerMetadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GrvtTriggerKind {
    #[serde(rename = "STOP_LOSS")]
    StopLoss,
    #[serde(rename = "TAKE_PROFIT")]
    TakeProfit,
}
#[derive(Deserialize)]
enum WireGrvtTriggerKind {
    #[serde(rename = "STOP_LOSS")]
    StopLoss,
    #[serde(rename = "TAKE_PROFIT")]
    TakeProfit,
}
impl<'de> Deserialize<'de> for GrvtTriggerKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire = WireGrvtTriggerKind::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&value),
        )?;
        Ok(match wire {
            WireGrvtTriggerKind::StopLoss => Self::StopLoss,
            WireGrvtTriggerKind::TakeProfit => Self::TakeProfit,
        })
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtTriggerKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { StopLoss, TakeProfit });
    }
}
