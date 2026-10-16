//! Signer request domain. BigInt-bearing records intentionally have no JSON
//! codec: their signed, SDK and wire projections are different boundaries.
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

// ts: LighterSignerProvider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LighterSignerProvider {
    #[serde(rename = "tealstreet-sdk")]
    TealstreetSdk,
}
#[derive(Deserialize)]
#[serde(remote = "LighterSignerProvider")]
enum LighterSignerProviderWire {
    #[serde(rename = "tealstreet-sdk")]
    TealstreetSdk,
}
impl<'de> Deserialize<'de> for LighterSignerProvider {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        LighterSignerProviderWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

// ts: LighterCreateOrderRequest.orderType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LighterSignerOrderType {
    Limit,
    Market,
    StopLoss,
    TakeProfit,
}
#[derive(Deserialize)]
#[serde(remote = "LighterSignerOrderType", rename_all = "snake_case")]
enum LighterSignerOrderTypeWire {
    Limit,
    Market,
    StopLoss,
    TakeProfit,
}
impl<'de> Deserialize<'de> for LighterSignerOrderType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        LighterSignerOrderTypeWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

// ts: LighterCreateOrderRequest.timeInForce
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LighterSignerTimeInForce {
    GoodTillCancel,
    ImmediateOrCancel,
    FillOrKill,
    PostOnly,
}
#[derive(Deserialize)]
#[serde(remote = "LighterSignerTimeInForce")]
enum LighterSignerTimeInForceWire {
    GoodTillCancel,
    ImmediateOrCancel,
    FillOrKill,
    PostOnly,
}
impl<'de> Deserialize<'de> for LighterSignerTimeInForce {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        LighterSignerTimeInForceWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

// ts: LighterCreateGroupedOrdersRequest.groupingType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LighterSignerGroupingType {
    OneTriggersOther = 1,
    OneCancelsOther = 2,
    OneTriggersOneCancelsOther = 3,
}
impl Serialize for LighterSignerGroupingType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}
impl<'de> Deserialize<'de> for LighterSignerGroupingType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match f64::deserialize(deserializer)? {
            1.0 => Ok(Self::OneTriggersOther),
            2.0 => Ok(Self::OneCancelsOther),
            3.0 => Ok(Self::OneTriggersOneCancelsOther),
            _ => Err(serde::de::Error::custom("expected groupingType 1, 2 or 3")),
        }
    }
}

// ts: LighterUpdateLeverageRequest.marginMode
// The zero literal also admits IEEE negative zero. Keep its bit in the domain
// instead of silently normalizing it through a unit enum discriminant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LighterSignerMarginMode(f64);
impl LighterSignerMarginMode {
    pub const CROSS: Self = Self(0.0);
    pub const ISOLATED: Self = Self(1.0);
}

impl TryFrom<f64> for LighterSignerMarginMode {
    type Error = &'static str;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value == 0.0 || value == 1.0 {
            Ok(Self(value))
        } else {
            Err("expected marginMode 0 or 1")
        }
    }
}
impl From<LighterSignerMarginMode> for f64 {
    fn from(value: LighterSignerMarginMode) -> Self {
        value.0
    }
}
impl Serialize for LighterSignerMarginMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.0)
    }
}
impl<'de> Deserialize<'de> for LighterSignerMarginMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(f64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

// ts: LighterCreateOrderRequest
#[derive(Debug, Clone, PartialEq)]
pub struct LighterCreateOrderRequest {
    pub order_id: BigInt,
    pub market_id: f64,
    pub is_buy: bool,
    pub price: BigInt,
    pub amount: BigInt,
    pub post_only: bool,
    pub reduce_only: bool,
    pub order_type: LighterSignerOrderType,
    pub trigger_price: Option<BigInt>,
    pub time_in_force: Option<LighterSignerTimeInForce>,
    pub integrator_account_index: Option<BigInt>,
    pub integrator_maker_fee: Option<f64>,
    pub integrator_taker_fee: Option<f64>,
}

// ts: LighterCreateGroupedOrdersRequest
#[derive(Debug, Clone, PartialEq)]
pub struct LighterCreateGroupedOrdersRequest {
    pub grouping_type: LighterSignerGroupingType,
    pub orders: Vec<LighterCreateOrderRequest>,
    pub integrator_account_index: Option<BigInt>,
    pub integrator_maker_fee: Option<f64>,
    pub integrator_taker_fee: Option<f64>,
}

// ts: LighterCancelOrderRequest
#[derive(Debug, Clone, PartialEq)]
pub struct LighterCancelOrderRequest {
    pub order_id: BigInt,
    pub market_id: f64,
}

// ts: LighterUpdateLeverageRequest
#[derive(Debug, Clone, PartialEq)]
pub struct LighterUpdateLeverageRequest {
    pub market_id: f64,
    pub leverage: f64,
    pub margin_mode: LighterSignerMarginMode,
}

// ts: LighterApproveIntegratorRequest
#[derive(Debug, Clone, PartialEq)]
pub struct LighterApproveIntegratorRequest {
    pub integrator_account_index: BigInt,
    pub max_perps_taker_fee: f64,
    pub max_perps_maker_fee: f64,
    pub max_spot_taker_fee: f64,
    pub max_spot_maker_fee: f64,
    pub approval_expiry: BigInt,
    pub l1_signature: Option<String>,
}
