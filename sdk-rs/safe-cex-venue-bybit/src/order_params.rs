//! Typed Bybit execution controls. Extension presence is resolved before shared decoding.
use safe_cex_types::{OrderTimeInForce, OrderType, PlaceOrderOpts};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::borrow::Borrow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BybitBboSideType {
    Queue,
    Counterparty,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BybitBboLevel(u8);
impl TryFrom<f64> for BybitBboLevel {
    type Error = &'static str;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_finite() && value.fract() == 0. && (1. ..=5.).contains(&value) {
            Ok(Self(value as u8))
        } else {
            Err("Bybit BBO level must be an integer from 1 to 5")
        }
    }
}
impl<'de> Deserialize<'de> for BybitBboLevel {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(f64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
impl Serialize for BybitBboLevel {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.0)
    }
}
#[derive(Debug, Clone)]
pub struct BybitBboOrderOpts {
    pub base: PlaceOrderOpts,
    pub side_type: BybitBboSideType,
    pub level: BybitBboLevel,
}
#[derive(Debug, Clone)]
pub struct BybitTickSizeMarketOrderOpts {
    pub base: PlaceOrderOpts,
    pub ticks: f64,
}
// Deliberate stronger representation: simultaneous extensions cannot be constructed.
#[derive(Debug, Clone)]
pub enum BybitPlaceOrderOpts {
    Plain(PlaceOrderOpts),
    Bbo(BybitBboOrderOpts),
    TickSize(BybitTickSizeMarketOrderOpts),
}
impl From<PlaceOrderOpts> for BybitPlaceOrderOpts {
    fn from(base: PlaceOrderOpts) -> Self {
        Self::Plain(base)
    }
}
impl Borrow<PlaceOrderOpts> for BybitPlaceOrderOpts {
    fn borrow(&self) -> &PlaceOrderOpts {
        match self {
            Self::Plain(base) => base,
            Self::Bbo(opts) => &opts.base,
            Self::TickSize(opts) => &opts.base,
        }
    }
}
// Capture the entire input before flatten or other serde buffers see numbers.
impl<'de> Deserialize<'de> for BybitPlaceOrderOpts {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use safe_cex_num::numeric_ingress::JsNumberInput;
        #[derive(Deserialize)]
        struct Wire {
            #[serde(flatten)]
            base: PlaceOrderOpts,
            #[serde(rename = "bybitBbo", default, deserialize_with = "present_extension")]
            bbo: Option<BboWire>,
            #[serde(
                rename = "bybitSlippageToleranceTicks",
                default,
                deserialize_with = "present_extension"
            )]
            ticks: Option<f64>,
        }
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|d| {
                let Wire { base, bbo, ticks } =
                    Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
                match (bbo, ticks) {
                    (Some(_), Some(_)) => Err(serde::de::Error::custom(
                        "Bybit BBO and TickSize slippage options are mutually exclusive",
                    )),
                    (Some(BboWire { side_type, level }), None) => {
                        Ok(Self::Bbo(BybitBboOrderOpts {
                            base,
                            side_type,
                            level,
                        }))
                    }
                    (None, Some(ticks)) => {
                        Ok(Self::TickSize(BybitTickSizeMarketOrderOpts { base, ticks }))
                    }
                    (None, None) => Ok(Self::Plain(base)),
                }
            })
            .map_err(serde::de::Error::custom)
    }
}
fn present_extension<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BboWire {
    side_type: BybitBboSideType,
    level: BybitBboLevel,
}
impl<'de> Deserialize<'de> for BboWire {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            side_type: BybitBboSideType,
            level: BybitBboLevel,
        }
        let Wire { side_type, level } =
            Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self { side_type, level })
    }
}
impl Serialize for BybitPlaceOrderOpts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            #[serde(flatten)]
            base: &'a PlaceOrderOpts,
            #[serde(rename = "bybitBbo", skip_serializing_if = "Option::is_none")]
            bbo: Option<BboWire>,
            #[serde(
                rename = "bybitSlippageToleranceTicks",
                skip_serializing_if = "Option::is_none"
            )]
            ticks: Option<f64>,
        }
        let (base, bbo, ticks) = match self {
            Self::Plain(base) => (base, None, None),
            Self::Bbo(BybitBboOrderOpts {
                base,
                side_type,
                level,
            }) => (
                base,
                Some(BboWire {
                    side_type: *side_type,
                    level: *level,
                }),
                None,
            ),
            Self::TickSize(BybitTickSizeMarketOrderOpts { base, ticks }) => {
                (base, None, Some(*ticks))
            }
        };
        Wire { base, bbo, ticks }.serialize(serializer)
    }
}

struct PlacementNumbers {
    amount: f64,
    price: Option<f64>,
    trigger_price: Option<f64>,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
    max_slippage_pct: Option<f64>,
    ticks: Option<f64>,
    level: Option<u8>,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PlacementNumbers {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { amount, price, trigger_price, stop_loss, take_profit, max_slippage_pct, ticks, level });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitPlaceOrderOpts {
    fn check_number_output(&self) {
        // Both destructures are exhaustive: adding a local or shared field must
        // force an eligibility review, without a foreign trait implementation.
        let (base, ticks, level) = match self {
            Self::Plain(base) => (base, None, None),
            Self::Bbo(BybitBboOrderOpts {
                base,
                side_type: _,
                level: BybitBboLevel(level),
            }) => (base, None, Some(*level)),
            Self::TickSize(BybitTickSizeMarketOrderOpts { base, ticks }) => {
                (base, Some(*ticks), None)
            }
        };
        let PlaceOrderOpts {
            symbol: _,
            r#type: _,
            side: _,
            amount,
            price,
            trigger_price,
            stop_loss,
            take_profit,
            reduce_only: _,
            time_in_force: _,
            trigger_type: _,
            tp_trigger_type: _,
            sl_trigger_type: _,
            client_order_id: _,
            max_slippage_pct,
            close: _,
            implied_volatility: _,
        } = base;
        PlacementNumbers {
            amount: *amount,
            price: *price,
            trigger_price: *trigger_price,
            stop_loss: *stop_loss,
            take_profit: *take_profit,
            max_slippage_pct: *max_slippage_pct,
            ticks,
            level,
        }
        .check_number_output();
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitOrderExecutionParams {
    #[serde(
        default,
        deserialize_with = "execution_side",
        skip_serializing_if = "Option::is_none"
    )]
    pub bbo_side_type: Option<BybitBboSideType>,
    #[serde(
        default,
        deserialize_with = "execution_level",
        skip_serializing_if = "Option::is_none"
    )]
    pub bbo_level: Option<String>,
    #[serde(
        default,
        deserialize_with = "execution_mode",
        skip_serializing_if = "Option::is_none"
    )]
    pub slippage_tolerance_type: Option<String>,
    #[serde(
        default,
        deserialize_with = "execution_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub slippage_tolerance: Option<String>,
}
impl<'de> Deserialize<'de> for BybitOrderExecutionParams {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let fields = serde_json::Map::<String, Value>::deserialize(d)?;
        Ok(Self {
            bbo_side_type: fields
                .get("bboSideType")
                .map(|v| execution_side(v.clone()))
                .transpose()
                .map_err(serde::de::Error::custom)?
                .flatten(),
            bbo_level: fields
                .get("bboLevel")
                .map(|v| execution_level(v.clone()))
                .transpose()
                .map_err(serde::de::Error::custom)?
                .flatten(),
            slippage_tolerance_type: fields
                .get("slippageToleranceType")
                .map(|v| execution_mode(v.clone()))
                .transpose()
                .map_err(serde::de::Error::custom)?
                .flatten(),
            slippage_tolerance: fields
                .get("slippageTolerance")
                .map(|v| execution_string(v.clone()))
                .transpose()
                .map_err(serde::de::Error::custom)?
                .flatten(),
        })
    }
}
fn has_at_most_two_decimals(value: f64) -> bool {
    // Source Math.round differs at negative half-integers; the source tolerance
    // makes either distance far greater than 1e-9 there.
    (value * 100. - (value * 100.).round()).abs() < 1e-9
}
fn has_attached_protection(opts: &PlaceOrderOpts) -> bool {
    opts.trigger_price.is_some() || opts.stop_loss.is_some() || opts.take_profit.is_some()
}
pub fn get_bybit_market_slippage_params(
    opts: &PlaceOrderOpts,
) -> Result<BybitOrderExecutionParams, &'static str> {
    let Some(value) = opts.max_slippage_pct else {
        return Ok(BybitOrderExecutionParams::default());
    };
    if opts.r#type != OrderType::Market || has_attached_protection(opts) {
        return Err("Bybit maxSlippagePct is only supported for plain market orders");
    }
    if !value.is_finite() || !(0.01..=10.).contains(&value) || !has_at_most_two_decimals(value) {
        return Err("Bybit maxSlippagePct must be from 0.01 to 10 with at most two decimals");
    }
    // Preserve ECMAScript spelling for the native request payload.
    let text = safe_cex_num::js_number_string(value);
    Ok(BybitOrderExecutionParams {
        slippage_tolerance_type: Some("Percent".into()),
        slippage_tolerance: Some(text),
        ..Default::default()
    })
}
pub fn has_bybit_single_order_params(opts: &BybitPlaceOrderOpts) -> bool {
    !matches!(opts, BybitPlaceOrderOpts::Plain(_))
        || (opts.borrow() as &PlaceOrderOpts)
            .max_slippage_pct
            .is_some()
}
pub fn get_bybit_order_execution_params(
    opts: &BybitPlaceOrderOpts,
    category: &str,
) -> Result<BybitOrderExecutionParams, &'static str> {
    let base: &PlaceOrderOpts = opts.borrow();
    match opts {
        BybitPlaceOrderOpts::Plain(_) => get_bybit_market_slippage_params(base),
        BybitPlaceOrderOpts::Bbo(opts) => {
            if !matches!(category, "linear" | "inverse") {
                return Err("Bybit BBO orders are only supported for linear and inverse products");
            }
            if base.r#type != OrderType::Limit {
                return Err("Bybit BBO requires a plain limit order");
            }
            if base
                .time_in_force
                .is_some_and(|t| t != OrderTimeInForce::GoodTillCancel)
            {
                return Err("Bybit BBO currently supports only GoodTillCancel time in force");
            }
            if base.price.is_some()
                || has_attached_protection(base)
                || base.max_slippage_pct.is_some()
            {
                return Err(
                    "Bybit BBO cannot include price, conditional, attached TP/SL, or slippage fields",
                );
            }
            Ok(BybitOrderExecutionParams {
                bbo_side_type: Some(opts.side_type),
                bbo_level: Some(opts.level.0.to_string()),
                ..Default::default()
            })
        }
        BybitPlaceOrderOpts::TickSize(opts) => {
            if base.r#type != OrderType::Market
                || base.price.is_some()
                || has_attached_protection(base)
                || base.max_slippage_pct.is_some()
            {
                return Err("Bybit TickSize slippage is only supported for plain market orders");
            }
            if !opts.ticks.is_finite()
                || opts.ticks.fract() != 0.
                || !(1. ..=10000.).contains(&opts.ticks)
            {
                return Err("Bybit TickSize slippage must be an integer from 1 to 10000");
            }
            Ok(BybitOrderExecutionParams {
                slippage_tolerance_type: Some("TickSize".into()),
                slippage_tolerance: Some((opts.ticks as u16).to_string()),
                ..Default::default()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_nonfinite_options_cannot_reach_execution_parameters() {
        let base: PlaceOrderOpts = serde_json::from_value(serde_json::json!({
            "symbol":"BTCUSDT", "type":"market", "side":"buy", "amount":1
        }))
        .unwrap();
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(BybitBboLevel::try_from(value).is_err());
            let mut percent = base.clone();
            percent.max_slippage_pct = Some(value);
            assert!(get_bybit_market_slippage_params(&percent).is_err());
            let ticks = BybitPlaceOrderOpts::TickSize(BybitTickSizeMarketOrderOpts {
                base: base.clone(),
                ticks: value,
            });
            assert!(get_bybit_order_execution_params(&ticks, "linear").is_err());
        }
    }
}

fn execution_side<'de, D: Deserializer<'de>>(d: D) -> Result<Option<BybitBboSideType>, D::Error> {
    <BybitBboSideType as Deserialize>::deserialize(d).map(Some)
}
fn execution_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
fn execution_level<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let value = String::deserialize(d)?;
    if matches!(value.as_str(), "1" | "2" | "3" | "4" | "5") {
        Ok(Some(value))
    } else {
        Err(serde::de::Error::custom(
            "bboLevel must be a string literal from 1 through 5",
        ))
    }
}
fn execution_mode<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let value = String::deserialize(d)?;
    if matches!(value.as_str(), "Percent" | "TickSize") {
        Ok(Some(value))
    } else {
        Err(serde::de::Error::custom(
            "slippageToleranceType must be Percent or TickSize",
        ))
    }
}

// Private derived decoder supplies canonical serde spelling metadata. Only the
// string-required public Deserialize implementation can construct the public type.
#[derive(Serialize, Deserialize)]
enum BybitBboSideTypeWire {
    Queue,
    Counterparty,
}
impl<'de> Deserialize<'de> for BybitBboSideType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = <String as Deserialize>::deserialize(d)?;
        let wire = BybitBboSideTypeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))?;
        Ok(match wire {
            BybitBboSideTypeWire::Queue => Self::Queue,
            BybitBboSideTypeWire::Counterparty => Self::Counterparty,
        })
    }
}
#[cfg(test)]
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn bybit_bbo_side_type_accepted_set() {
    let _exhaustive = |value: BybitBboSideType| match value {
        BybitBboSideType::Queue | BybitBboSideType::Counterparty => (),
    };
    crate::union_audit::check::<BybitBboSideType, BybitBboSideTypeWire>("BybitBboSideType");
}

impl Serialize for BybitBboSideType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Queue => BybitBboSideTypeWire::Queue,
            Self::Counterparty => BybitBboSideTypeWire::Counterparty,
        };
        wire.serialize(serializer)
    }
}
