//! Pure order classification; source precedence and source labels are distinct.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{js_string, js_truthy, parse_float};
use safe_cex_types::{OrderTimeInForce, OrderType};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HyperliquidOrderInferenceSource {
    OrderTypeLimit,
    OrderTypeTp,
    OrderTypeSl,
    TriggerPxTpslTp,
    TriggerPxTpslSl,
    TriggerPxOnly,
    NoOrderTypeFallback,
    FallbackMarket,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HyperliquidOrderInferenceResult {
    pub r#type: OrderType,
    pub price: f64,
    pub source: HyperliquidOrderInferenceSource,
}

fn text(value: Option<&Value>) -> Result<String, ExchangeError> {
    match value.filter(|value| !value.is_null()) {
        None => Ok(String::new()),
        Some(value) => js_string(Some(value)).map_err(|error| {
            let mut result = ExchangeError::new(error.message);
            result.name = error.name.into();
            result
        }),
    }
}

pub fn to_number_or_zero(value: Option<&Value>) -> Result<f64, ExchangeError> {
    let n = parse_float(&text(value)?);
    Ok(if n.is_finite() { n } else { 0.0 })
}

pub fn infer_hyperliquid_order_type_and_price(
    order: &Value,
) -> Result<HyperliquidOrderInferenceResult, ExchangeError> {
    if order.is_null() {
        let mut error = ExchangeError::new("Cannot read properties of null (reading 'orderType')");
        error.name = "TypeError".into();
        return Err(error);
    }
    let order_type = text(
        order
            .get("orderType")
            .filter(|value| js_truthy(Some(value))),
    )?;
    let trigger_px = order.get("triggerPx");
    // Coerce tpsl before branching, even when Limit will win.
    let tpsl = text(order.get("tpsl").filter(|value| js_truthy(Some(value))))?;
    let limit_px = order.get("limitPx");
    use HyperliquidOrderInferenceSource as Source;
    let (kind, price, source) = if order_type == "Limit" {
        (
            OrderType::Limit,
            to_number_or_zero(limit_px)?,
            Source::OrderTypeLimit,
        )
    } else if order_type == "Take Profit Market" || (js_truthy(trigger_px) && tpsl == "tp") {
        (
            OrderType::TakeProfit,
            to_number_or_zero(trigger_px)?,
            if order_type.is_empty() {
                Source::TriggerPxTpslTp
            } else {
                Source::OrderTypeTp
            },
        )
    } else if order_type == "Stop Market" || (js_truthy(trigger_px) && tpsl == "sl") {
        (
            OrderType::StopLoss,
            to_number_or_zero(trigger_px)?,
            if order_type.is_empty() {
                Source::TriggerPxTpslSl
            } else {
                Source::OrderTypeSl
            },
        )
    } else if js_truthy(trigger_px) {
        (
            OrderType::StopLoss,
            to_number_or_zero(trigger_px)?,
            Source::TriggerPxOnly,
        )
    } else if js_truthy(limit_px) {
        (
            OrderType::Limit,
            to_number_or_zero(limit_px)?,
            Source::NoOrderTypeFallback,
        )
    } else {
        (OrderType::Market, 0.0, Source::FallbackMarket)
    };
    Ok(HyperliquidOrderInferenceResult {
        r#type: kind,
        price,
        source,
    })
}

pub fn parse_hyperliquid_order_time_in_force(tif: Option<&Value>) -> Option<OrderTimeInForce> {
    match tif.and_then(Value::as_str) {
        Some("Alo") => Some(OrderTimeInForce::PostOnly),
        Some("Gtc") => Some(OrderTimeInForce::GoodTillCancel),
        _ => None,
    }
}
