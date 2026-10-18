//! Borrowed position ids and standalone protection snapshots.
use crate::{
    order_links::{get_tpsl_raw_order_id, is_standalone_tpsl_order},
    tpsl_helpers::{TpslLeg, get_bitunix_trigger_type},
};
use safe_cex_num::{JsCoercionError, js_truthy};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{Order, OrderType, Position, PositionSide};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;

pub struct TpslLegSnapshot {
    pub price: f64,
    pub qty: f64,
    pub stop_type: &'static str,
    pub order_type: String,
    pub order_price: f64,
}
fn info<'a>(order: &'a Order, key: &str) -> Option<&'a Input> {
    order.info.as_ref().and_then(|v| v.get(key))
}
fn strict_equal(a: &Input, b: &Input) -> bool {
    match (a, b) {
        (Input::Float(_) | Input::Lexeme(_), Input::Float(_) | Input::Lexeme(_)) => {
            a.as_f64().zip(b.as_f64()).is_some_and(|(a, b)| a == b)
        }
        (Input::Array(_), Input::Array(_)) | (Input::Object(_), Input::Object(_)) => {
            std::ptr::eq(a, b)
        }
        _ => a == b,
    }
}
fn retained_finite(value: Option<&Input>, fallback: f64) -> Result<f64, JsCoercionError> {
    if value.is_none_or(|value| value.is_nullish() || value.text_equals("")) {
        return Ok(fallback);
    }
    let value = safe_cex_num::js_retained_number(value)?;
    Ok(if value.is_finite() { value } else { fallback })
}
pub fn get_tpsl_leg_snapshot(
    order: &Order,
    leg: TpslLeg,
    orders: &[Order],
) -> Result<Option<TpslLegSnapshot>, JsCoercionError> {
    let raw_id = get_tpsl_raw_order_id(order)?;
    let position_id = info(order, "positionId");
    let leg_type = match leg {
        TpslLeg::Tp => OrderType::TakeProfit,
        TpslLeg::Sl => OrderType::StopLoss,
    };
    let related = if order.r#type == leg_type && is_standalone_tpsl_order(order, orders) {
        Some(order)
    } else {
        let mut found = None;
        for candidate in orders {
            if candidate.symbol == order.symbol
                && candidate.r#type == leg_type
                && is_standalone_tpsl_order(candidate, orders)
            {
                if get_tpsl_raw_order_id(candidate)? == raw_id
                    || position_id
                        .filter(|v| !v.is_null())
                        .zip(info(candidate, "positionId"))
                        .is_some_and(|(a, b)| strict_equal(a, b))
                {
                    found = Some(candidate);
                    break;
                }
            }
        }
        found
    };
    let info = related
        .and_then(|o| o.info.as_ref())
        .filter(|value| value.is_truthy())
        .or_else(|| order.info.as_ref().filter(|value| value.is_truthy()));
    let field = |name: &str| info.and_then(|v| v.get(name));
    let (price_key, qty_key, stop_key, type_key, order_price_key) = match leg {
        TpslLeg::Tp => (
            "tpPrice",
            "tpQty",
            "tpStopType",
            "tpOrderType",
            "tpOrderPrice",
        ),
        TpslLeg::Sl => (
            "slPrice",
            "slQty",
            "slStopType",
            "slOrderType",
            "slOrderPrice",
        ),
    };
    let price = match related {
        Some(o) => o.price.to_number(),
        None => retained_finite(field(price_key), f64::NAN)?,
    };
    let qty = match related {
        Some(o) => o.amount,
        None => retained_finite(field(qty_key), f64::NAN)?,
    };
    if !price.is_finite() || price <= 0.0 {
        return Ok(None);
    }
    let trigger = related.and_then(|o| match leg {
        TpslLeg::Tp => (o.tp_trigger_type).into_value(),
        TpslLeg::Sl => o.sl_trigger_type.into_value(),
    });
    let raw_type = field(type_key);
    Ok(Some(TpslLegSnapshot {
        price,
        qty,
        stop_type: get_bitunix_trigger_type(trigger, field(stop_key))?,
        order_type: if raw_type.is_some_and(Input::is_truthy) {
            safe_cex_num::js_retained_string(raw_type)?
        } else {
            "MARKET".into()
        }
        .to_uppercase(),
        order_price: retained_finite(field(order_price_key), f64::NAN)?,
    }))
}
pub fn get_position_id<'a>(
    symbol: &str,
    side: PositionSide,
    positions: &'a [Position],
    cache: &'a HashMap<String, Input>,
) -> Option<&'a Input> {
    let position = positions
        .iter()
        .find(|p| p.symbol == symbol && p.side == side && p.contracts > 0.0);
    position
        .and_then(|p| p.info.as_ref())
        .and_then(|i| i.get("positionId"))
        .filter(|value| value.is_truthy())
        .or_else(|| {
            cache.get(&format!(
                "{symbol}:{}",
                if side == PositionSide::Long {
                    "long"
                } else {
                    "short"
                }
            ))
        })
}
pub async fn hydrate_position_id_before_formatting<P, O, E, F1, F2>(
    symbol: &str,
    order_type: impl Into<safe_cex_types::ModelField<OrderType>>,
    side: impl Into<safe_cex_types::ModelField<safe_cex_types::OrderSide>>,
    amount: f64,
    reduce_only: Option<bool>,
    positions: &[Position],
    cache: &HashMap<String, Value>,
    fetch_positions: impl FnOnce() -> F1,
    update_positions: impl FnOnce(P) -> Result<(), E>,
    fetch_orders: impl FnOnce() -> F2,
    update_orders: impl FnOnce(O) -> Result<(), E>,
) -> Result<(), E>
where
    F1: Future<Output = Result<P, E>>,
    F2: Future<Output = Result<O, E>>,
{
    let order_type = order_type.into();
    let side = side.into();
    let is_tpsl = order_type == OrderType::StopLoss || order_type == OrderType::TakeProfit;
    if !is_tpsl && reduce_only != Some(true) {
        return Ok(());
    }
    let full = is_tpsl && (amount == 0.0 || amount.is_nan());
    let side = if side == safe_cex_types::OrderSide::Buy {
        PositionSide::Short
    } else {
        PositionSide::Long
    };
    let Some(position) = positions
        .iter()
        .find(|p| p.symbol == symbol && p.side == side && p.contracts > 0.0)
    else {
        return Ok(());
    };
    let raw_id = position.info.as_ref().and_then(|i| i.get("positionId"));
    let cached = cache.get(&format!(
        "{symbol}:{}",
        if side == PositionSide::Long {
            "long"
        } else {
            "short"
        }
    ));
    if !raw_id.is_some_and(Input::is_truthy) && !js_truthy(cached) {
        update_positions(fetch_positions().await?)?;
    }
    if full {
        update_orders(fetch_orders().await?)?;
    }
    Ok(())
}
