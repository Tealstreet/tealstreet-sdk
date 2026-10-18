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

enum WireGrvtOrderSide {
    Buy,
    Sell,
}
impl<'de> Deserialize<'de> for WireGrvtOrderSide {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "BUY" => Ok(Self::Buy),
            "SELL" => Ok(Self::Sell),
            _ => Err(serde::de::Error::unknown_variant(&value, &["BUY", "SELL"])),
        }
    }
}
impl<'de> Deserialize<'de> for GrvtOrderSide {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireGrvtOrderSide::deserialize(deserializer)?;
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

enum WireGrvtOrderType {
    Limit,
    Market,
}
impl<'de> Deserialize<'de> for WireGrvtOrderType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "LIMIT" => Ok(Self::Limit),
            "MARKET" => Ok(Self::Market),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["LIMIT", "MARKET"],
            )),
        }
    }
}
impl<'de> Deserialize<'de> for GrvtOrderType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireGrvtOrderType::deserialize(deserializer)?;
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

enum WireGrvtTimeInForce {
    GoodTillTime,
    AllOrNone,
    ImmediateOrCancel,
    FillOrKill,
}
impl<'de> Deserialize<'de> for WireGrvtTimeInForce {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "GOOD_TILL_TIME" => Ok(Self::GoodTillTime),
            "ALL_OR_NONE" => Ok(Self::AllOrNone),
            "IMMEDIATE_OR_CANCEL" => Ok(Self::ImmediateOrCancel),
            "FILL_OR_KILL" => Ok(Self::FillOrKill),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &[
                    "GOOD_TILL_TIME",
                    "ALL_OR_NONE",
                    "IMMEDIATE_OR_CANCEL",
                    "FILL_OR_KILL",
                ],
            )),
        }
    }
}
impl<'de> Deserialize<'de> for GrvtTimeInForce {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireGrvtTimeInForce::deserialize(deserializer)?;
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

enum WireGrvtCandlestickInterval {
    Ci1M,
    Ci3M,
    Ci5M,
    Ci15M,
    Ci30M,
    Ci1H,
    Ci2H,
    Ci4H,
    Ci6H,
    Ci8H,
    Ci12H,
    Ci1D,
    Ci3D,
    Ci5D,
    Ci1W,
    Ci2W,
    Ci3W,
    Ci4W,
}
impl<'de> Deserialize<'de> for WireGrvtCandlestickInterval {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "CI_1_M" => Ok(Self::Ci1M),
            "CI_3_M" => Ok(Self::Ci3M),
            "CI_5_M" => Ok(Self::Ci5M),
            "CI_15_M" => Ok(Self::Ci15M),
            "CI_30_M" => Ok(Self::Ci30M),
            "CI_1_H" => Ok(Self::Ci1H),
            "CI_2_H" => Ok(Self::Ci2H),
            "CI_4_H" => Ok(Self::Ci4H),
            "CI_6_H" => Ok(Self::Ci6H),
            "CI_8_H" => Ok(Self::Ci8H),
            "CI_12_H" => Ok(Self::Ci12H),
            "CI_1_D" => Ok(Self::Ci1D),
            "CI_3_D" => Ok(Self::Ci3D),
            "CI_5_D" => Ok(Self::Ci5D),
            "CI_1_W" => Ok(Self::Ci1W),
            "CI_2_W" => Ok(Self::Ci2W),
            "CI_3_W" => Ok(Self::Ci3W),
            "CI_4_W" => Ok(Self::Ci4W),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &[
                    "CI_1_M", "CI_3_M", "CI_5_M", "CI_15_M", "CI_30_M", "CI_1_H", "CI_2_H",
                    "CI_4_H", "CI_6_H", "CI_8_H", "CI_12_H", "CI_1_D", "CI_3_D", "CI_5_D",
                    "CI_1_W", "CI_2_W", "CI_3_W", "CI_4_W",
                ],
            )),
        }
    }
}
impl<'de> Deserialize<'de> for GrvtCandlestickInterval {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireGrvtCandlestickInterval::deserialize(deserializer)?;
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

enum WireGrvtOrderStatus {
    Pending,
    Open,
    Filled,
    Rejected,
    Cancelled,
}
impl<'de> Deserialize<'de> for WireGrvtOrderStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "PENDING" => Ok(Self::Pending),
            "OPEN" => Ok(Self::Open),
            "FILLED" => Ok(Self::Filled),
            "REJECTED" => Ok(Self::Rejected),
            "CANCELLED" => Ok(Self::Cancelled),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["PENDING", "OPEN", "FILLED", "REJECTED", "CANCELLED"],
            )),
        }
    }
}
impl<'de> Deserialize<'de> for GrvtOrderStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireGrvtOrderStatus::deserialize(deserializer)?;
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

enum WireGrvtAssetKind {
    Perpetual,
    Future,
    Call,
    Put,
}
impl<'de> Deserialize<'de> for WireGrvtAssetKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "PERPETUAL" => Ok(Self::Perpetual),
            "FUTURE" => Ok(Self::Future),
            "CALL" => Ok(Self::Call),
            "PUT" => Ok(Self::Put),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["PERPETUAL", "FUTURE", "CALL", "PUT"],
            )),
        }
    }
}
impl<'de> Deserialize<'de> for GrvtAssetKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireGrvtAssetKind::deserialize(deserializer)?;
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
enum WireGrvtLiquidity {
    Maker,
    Taker,
}
impl<'de> Deserialize<'de> for WireGrvtLiquidity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "MAKER" => Ok(Self::Maker),
            "TAKER" => Ok(Self::Taker),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["MAKER", "TAKER"],
            )),
        }
    }
}
impl<'de> Deserialize<'de> for GrvtLiquidity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireGrvtLiquidity::deserialize(deserializer)?;
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
enum WireGrvtJsonRpcVersion {
    V2,
}
impl<'de> Deserialize<'de> for WireGrvtJsonRpcVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "2.0" => Ok(Self::V2),
            _ => Err(serde::de::Error::unknown_variant(&value, &["2.0"])),
        }
    }
}
impl<'de> Deserialize<'de> for GrvtJsonRpcVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireGrvtJsonRpcVersion::deserialize(deserializer)?;
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
enum WireGrvtSubscriptionMethod {
    Subscribe,
    Unsubscribe,
}
impl<'de> Deserialize<'de> for WireGrvtSubscriptionMethod {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "subscribe" => Ok(Self::Subscribe),
            "unsubscribe" => Ok(Self::Unsubscribe),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["subscribe", "unsubscribe"],
            )),
        }
    }
}
impl<'de> Deserialize<'de> for GrvtSubscriptionMethod {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireGrvtSubscriptionMethod::deserialize(deserializer)?;
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
enum WireGrvtBuilderUnauthorizedReason {
    Unauthorized,
    CapTooLow,
}
impl<'de> Deserialize<'de> for WireGrvtBuilderUnauthorizedReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "unauthorized" => Ok(Self::Unauthorized),
            "cap-too-low" => Ok(Self::CapTooLow),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["unauthorized", "cap-too-low"],
            )),
        }
    }
}
impl<'de> Deserialize<'de> for GrvtBuilderUnauthorizedReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireGrvtBuilderUnauthorizedReason::deserialize(deserializer)?;
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
enum WireGrvtTriggerKind {
    StopLoss,
    TakeProfit,
}
impl<'de> Deserialize<'de> for WireGrvtTriggerKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "STOP_LOSS" => Ok(Self::StopLoss),
            "TAKE_PROFIT" => Ok(Self::TakeProfit),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["STOP_LOSS", "TAKE_PROFIT"],
            )),
        }
    }
}
impl<'de> Deserialize<'de> for GrvtTriggerKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireGrvtTriggerKind::deserialize(deserializer)?;
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

#[cfg(test)]
mod accepted_wire_string_shapes {
    use super::*;
    use serde::{Deserialize, de::DeserializeOwned};
    use serde_json::{Value, json};

    fn check<T: DeserializeOwned>(name: &str, literals: &[(&str, fn(T) -> bool)]) {
        for (literal, native_variant) in literals {
            let raw = serde_json::to_string(literal).unwrap();
            let decoded: T = serde_json::from_str(&raw).unwrap();
            assert!(
                native_variant(decoded),
                "GRVT WIRE STRING {name} native variant for {literal}"
            );
            let decoded: T = serde_json::from_value(json!(literal)).unwrap();
            assert!(
                native_variant(decoded),
                "GRVT WIRE STRING {name} native variant via value for {literal}"
            );
            let mut decoder = serde_json::Deserializer::from_str(&raw);
            let decoded = <T as Deserialize>::deserialize(&mut decoder).unwrap();
            decoder.end().unwrap();
            assert!(
                native_variant(decoded),
                "GRVT WIRE STRING {name} qualified native variant for {literal}"
            );
            let mut tagged = serde_json::Map::new();
            tagged.insert((*literal).to_owned(), Value::Null);
            let tagged = Value::Object(tagged);
            assert!(
                serde_json::from_value::<T>(tagged.clone()).is_err(),
                "GRVT WIRE STRING {name} tagged object accepted for {literal}"
            );
            assert!(
                <T as Deserialize>::deserialize(&tagged).is_err(),
                "GRVT WIRE STRING {name} borrowed tagged object accepted for {literal}"
            );
            let raw = serde_json::to_string(&tagged).unwrap();
            assert!(
                serde_json::from_str::<T>(&raw).is_err(),
                "GRVT WIRE STRING {name} text tagged object accepted for {literal}"
            );
            let mut decoder = serde_json::Deserializer::from_str(&raw);
            assert!(
                <T as Deserialize>::deserialize(&mut decoder).is_err(),
                "GRVT WIRE STRING {name} qualified tagged object accepted for {literal}"
            );
        }
        for raw in [
            Value::Null,
            json!(true),
            json!(0),
            json!([]),
            json!({}),
            json!("__UNKNOWN_GRVT_LITERAL__"),
        ] {
            assert!(
                serde_json::from_value::<T>(raw.clone()).is_err(),
                "GRVT WIRE STRING {name} invalid shape accepted: {raw}"
            );
        }
    }

    #[test]
    fn wire_grvt_order_side_requires_string_shape() {
        check::<WireGrvtOrderSide>(
            "WireGrvtOrderSide",
            &[
                ("BUY", |value| matches!(value, WireGrvtOrderSide::Buy)),
                ("SELL", |value| matches!(value, WireGrvtOrderSide::Sell)),
            ],
        );
    }

    #[test]
    fn wire_grvt_order_type_requires_string_shape() {
        check::<WireGrvtOrderType>(
            "WireGrvtOrderType",
            &[
                ("LIMIT", |value| matches!(value, WireGrvtOrderType::Limit)),
                ("MARKET", |value| matches!(value, WireGrvtOrderType::Market)),
            ],
        );
    }

    #[test]
    fn wire_grvt_time_in_force_requires_string_shape() {
        check::<WireGrvtTimeInForce>(
            "WireGrvtTimeInForce",
            &[
                ("GOOD_TILL_TIME", |value| {
                    matches!(value, WireGrvtTimeInForce::GoodTillTime)
                }),
                ("ALL_OR_NONE", |value| {
                    matches!(value, WireGrvtTimeInForce::AllOrNone)
                }),
                ("IMMEDIATE_OR_CANCEL", |value| {
                    matches!(value, WireGrvtTimeInForce::ImmediateOrCancel)
                }),
                ("FILL_OR_KILL", |value| {
                    matches!(value, WireGrvtTimeInForce::FillOrKill)
                }),
            ],
        );
    }

    #[test]
    fn wire_grvt_candlestick_interval_requires_string_shape() {
        check::<WireGrvtCandlestickInterval>(
            "WireGrvtCandlestickInterval",
            &[
                ("CI_1_M", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci1M)
                }),
                ("CI_3_M", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci3M)
                }),
                ("CI_5_M", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci5M)
                }),
                ("CI_15_M", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci15M)
                }),
                ("CI_30_M", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci30M)
                }),
                ("CI_1_H", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci1H)
                }),
                ("CI_2_H", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci2H)
                }),
                ("CI_4_H", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci4H)
                }),
                ("CI_6_H", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci6H)
                }),
                ("CI_8_H", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci8H)
                }),
                ("CI_12_H", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci12H)
                }),
                ("CI_1_D", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci1D)
                }),
                ("CI_3_D", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci3D)
                }),
                ("CI_5_D", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci5D)
                }),
                ("CI_1_W", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci1W)
                }),
                ("CI_2_W", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci2W)
                }),
                ("CI_3_W", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci3W)
                }),
                ("CI_4_W", |value| {
                    matches!(value, WireGrvtCandlestickInterval::Ci4W)
                }),
            ],
        );
    }

    #[test]
    fn wire_grvt_order_status_requires_string_shape() {
        check::<WireGrvtOrderStatus>(
            "WireGrvtOrderStatus",
            &[
                ("PENDING", |value| {
                    matches!(value, WireGrvtOrderStatus::Pending)
                }),
                ("OPEN", |value| matches!(value, WireGrvtOrderStatus::Open)),
                ("FILLED", |value| {
                    matches!(value, WireGrvtOrderStatus::Filled)
                }),
                ("REJECTED", |value| {
                    matches!(value, WireGrvtOrderStatus::Rejected)
                }),
                ("CANCELLED", |value| {
                    matches!(value, WireGrvtOrderStatus::Cancelled)
                }),
            ],
        );
    }

    #[test]
    fn wire_grvt_asset_kind_requires_string_shape() {
        check::<WireGrvtAssetKind>(
            "WireGrvtAssetKind",
            &[
                ("PERPETUAL", |value| {
                    matches!(value, WireGrvtAssetKind::Perpetual)
                }),
                ("FUTURE", |value| matches!(value, WireGrvtAssetKind::Future)),
                ("CALL", |value| matches!(value, WireGrvtAssetKind::Call)),
                ("PUT", |value| matches!(value, WireGrvtAssetKind::Put)),
            ],
        );
    }

    #[test]
    fn wire_grvt_liquidity_requires_string_shape() {
        check::<WireGrvtLiquidity>(
            "WireGrvtLiquidity",
            &[
                ("MAKER", |value| matches!(value, WireGrvtLiquidity::Maker)),
                ("TAKER", |value| matches!(value, WireGrvtLiquidity::Taker)),
            ],
        );
    }

    #[test]
    fn wire_grvt_json_rpc_version_requires_string_shape() {
        check::<WireGrvtJsonRpcVersion>(
            "WireGrvtJsonRpcVersion",
            &[("2.0", |value| matches!(value, WireGrvtJsonRpcVersion::V2))],
        );
    }

    #[test]
    fn wire_grvt_subscription_method_requires_string_shape() {
        check::<WireGrvtSubscriptionMethod>(
            "WireGrvtSubscriptionMethod",
            &[
                ("subscribe", |value| {
                    matches!(value, WireGrvtSubscriptionMethod::Subscribe)
                }),
                ("unsubscribe", |value| {
                    matches!(value, WireGrvtSubscriptionMethod::Unsubscribe)
                }),
            ],
        );
    }

    #[test]
    fn wire_grvt_builder_unauthorized_reason_requires_string_shape() {
        check::<WireGrvtBuilderUnauthorizedReason>(
            "WireGrvtBuilderUnauthorizedReason",
            &[
                ("unauthorized", |value| {
                    matches!(value, WireGrvtBuilderUnauthorizedReason::Unauthorized)
                }),
                ("cap-too-low", |value| {
                    matches!(value, WireGrvtBuilderUnauthorizedReason::CapTooLow)
                }),
            ],
        );
    }

    #[test]
    fn wire_grvt_trigger_kind_requires_string_shape() {
        check::<WireGrvtTriggerKind>(
            "WireGrvtTriggerKind",
            &[
                ("STOP_LOSS", |value| {
                    matches!(value, WireGrvtTriggerKind::StopLoss)
                }),
                ("TAKE_PROFIT", |value| {
                    matches!(value, WireGrvtTriggerKind::TakeProfit)
                }),
            ],
        );
    }
}
