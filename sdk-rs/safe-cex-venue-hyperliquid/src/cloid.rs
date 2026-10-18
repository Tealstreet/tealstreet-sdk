//! Hyperliquid client order IDs (cloids) from hyperliquid.cloid.ts.
//! `0x` + 32 lowercase hex chars; the venue echoes them lowercase, so an
//! uppercase caller ID would never match its placement record.
use crate::order_inference::parse_hyperliquid_order_time_in_force;
use safe_cex_exchange::client_order_id::{
    ClientOrderIdRule, ClientOrderIdRuleOverrides, pattern_client_order_id_rule,
};
use safe_cex_num::{js_number, js_truthy};
use safe_cex_types::{OrderSide, OrderTimeInForce, OrderType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::rc::Rc;

/// The source RegExp's display, used verbatim in the client-order-ID requirement.
pub const HYPERLIQUID_CLOID_PATTERN: &str = "/^0x[0-9a-f]{32}$/";
const CLOID_BYTES: usize = 16;

/// `HYPERLIQUID_CLOID_PATTERN.test(value)` on a string.
pub fn is_hyperliquid_cloid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 2 + 2 * CLOID_BYTES
        && bytes.starts_with(b"0x")
        && bytes[2..]
            .iter()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// The hex spelling of 16 random bytes, in source byte order.
pub fn format_hyperliquid_cloid(bytes: [u8; CLOID_BYTES]) -> String {
    let mut hex = String::from("0x");
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// A fresh cloid from the platform CSPRNG (`crypto.getRandomValues` on wasm).
pub fn mint_hyperliquid_cloid() -> Result<String, String> {
    let mut bytes = [0; CLOID_BYTES];
    getrandom::getrandom(&mut bytes).map_err(|error| error.to_string())?;
    Ok(format_hyperliquid_cloid(bytes))
}

/// Honors a caller ID on every shape except trailing stops, and mints otherwise.
pub fn hyperliquid_client_order_id_rule() -> ClientOrderIdRule {
    pattern_client_order_id_rule(
        "Hyperliquid".into(),
        HYPERLIQUID_CLOID_PATTERN.into(),
        Rc::new(is_hyperliquid_cloid),
        ClientOrderIdRuleOverrides {
            is_honorable_shape: Some(Rc::new(|opts| opts.r#type != OrderType::TrailingStopLoss)),
            mint: Some(Rc::new(|| {
                mint_hyperliquid_cloid().expect("platform CSPRNG unavailable for cloid minting")
            })),
        },
    )
}

/// The cloid a venue row/frame carries; absent, `null` and malformed read as None.
pub fn read_hyperliquid_cloid(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|text| is_hyperliquid_cloid(text))
        .map(str::to_owned)
}

/// What was signed under one cloid. `price` is the trigger price for TP/SL and
/// the limit price otherwise (a market order is a Gtc limit at the slippage cap).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperliquidPlacementRecord {
    pub r#type: OrderType,
    pub price: f64,
    pub side: OrderSide,
    pub amount: f64,
    pub reduce_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<OrderTimeInForce>,
    pub symbol: String,
    pub placed_at: f64,
}

#[derive(Deserialize)]
#[serde(remote = "HyperliquidPlacementRecord", rename_all = "camelCase")]
struct HyperliquidPlacementRecordWire {
    r#type: OrderType,
    price: f64,
    side: OrderSide,
    amount: f64,
    reduce_only: bool,
    #[serde(default, deserialize_with = "crate::spot_json::present")]
    time_in_force: Option<OrderTimeInForce>,
    symbol: String,
    placed_at: f64,
}

impl<'de> Deserialize<'de> for HyperliquidPlacementRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                HyperliquidPlacementRecordWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for HyperliquidPlacementRecord {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {
            r#type, price, side, amount, reduce_only, time_in_force, symbol, placed_at
        });
    }
}

fn number(value: Option<&Value>) -> Result<f64, String> {
    js_number(value).map_err(|error| error.to_string())
}
fn property<'a>(value: Option<&'a Value>, key: &str) -> Result<Option<&'a Value>, String> {
    match value {
        None | Some(Value::Null) => Err(format!(
            "Cannot read properties of {} (reading '{key}')",
            if value.is_none() { "undefined" } else { "null" }
        )),
        Some(Value::Object(object)) => Ok(object.get(key)),
        Some(_) => Ok(None),
    }
}

/// Built from the wire actually sent (`orderRequestToOrderWire`'s JSON, where
/// `triggerPx` is a string), so it cannot drift from what the venue received.
pub fn placement_record_from_wire(
    wire: &Value,
    symbol: String,
    placed_at: f64,
) -> Result<HyperliquidPlacementRecord, String> {
    let order_type = property(Some(wire), "t")?;
    let trigger = property(order_type, "trigger")?.filter(|trigger| js_truthy(Some(trigger)));
    let r#type = match trigger {
        Some(trigger) if property(Some(trigger), "tpsl")? == Some(&Value::from("tp")) => {
            OrderType::TakeProfit
        }
        Some(_) => OrderType::StopLoss,
        None => OrderType::Limit,
    };
    let price = match trigger {
        Some(trigger) => number(property(Some(trigger), "triggerPx")?)?,
        None => number(property(Some(wire), "p")?)?,
    };
    let side = if js_truthy(property(Some(wire), "b")?) {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    let amount = number(property(Some(wire), "s")?)?;
    let reduce_only = match property(Some(wire), "r")? {
        Some(Value::Bool(flag)) => *flag,
        other => {
            return Err(format!(
                "placement record reduceOnly must be a boolean wire field, got {other:?}"
            ));
        }
    };
    let time_in_force = match trigger {
        Some(_) => None,
        None => {
            let limit = property(Some(wire), "t").and_then(|t| property(t, "limit"))?;
            let tif = match limit {
                None | Some(Value::Null) => None,
                limit => property(limit, "tif")?,
            };
            parse_hyperliquid_order_time_in_force(tif)
        }
    };
    Ok(HyperliquidPlacementRecord {
        r#type,
        price,
        side,
        amount,
        reduce_only,
        time_in_force,
        symbol,
        placed_at,
    })
}
